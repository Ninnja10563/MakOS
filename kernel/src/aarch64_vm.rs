use core::cell::UnsafeCell;
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use makos_vm_space::RegionTable;

pub const PAGE_SIZE: u64 = 4096;
pub const PROT_READ: u64 = 1;
pub const PROT_WRITE: u64 = 2;
pub const PROT_EXEC: u64 = 4;

const MAX_PROCESSES: usize = 64;
// Gecko routinely keeps thousands of VMAs alive (JIT reservations, guard
// pages, thread stacks, and shared-memory views).  Exhausting this metadata
// table is reported to userspace as mmap(ENOMEM), even while physical RAM is
// still available.
const MAX_REGIONS: usize = 8192;
// Large sparse reservations are normal for modern JITs. Physical frames are
// committed on first access, so this limits virtual span rather than RAM use.
const MAX_PAGES_PER_CALL: usize = 4 * 1024 * 1024;
const JIT_RESERVATION_TEST_BYTES: u64 = 2044 * 1024 * 1024;
const MAX_FILE_BACKINGS: usize = 1024;
const VM_TRACE_LIMIT: u64 = 8;
static FIREFOX_FILE_MAP_TRACES: AtomicU64 = AtomicU64::new(0);
static SHMEM_MAP_TRACES: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
struct ProcessVm {
    pid: u64,
    root: u64,
    break_base: u64,
    current_break: u64,
}

#[derive(Clone, Copy)]
struct FileBacking {
    pid: u64,
    base: u64,
    pages: usize,
    file_offset: u64,
    source: FileBackingSource,
}

#[derive(Clone, Copy)]
enum FileBackingSource {
    Empty,
    ReadOnly(crate::vfs::ReadOnlyFileBacking),
    Shared(crate::vfs::SharedMemoryBacking),
}

impl FileBacking {
    const EMPTY: Self = Self {
        pid: 0,
        base: 0,
        pages: 0,
        file_offset: 0,
        source: FileBackingSource::Empty,
    };

    fn end(self) -> Option<u64> {
        self.base.checked_add(self.pages as u64 * PAGE_SIZE)
    }
}

impl ProcessVm {
    const EMPTY: Self = Self {
        pid: 0,
        root: 0,
        break_base: 0,
        current_break: 0,
    };
}

struct VmState {
    processes: [ProcessVm; MAX_PROCESSES],
    regions: RegionTable<MAX_REGIONS>,
    file_backings: [FileBacking; MAX_FILE_BACKINGS],
}

impl VmState {
    const fn new() -> Self {
        Self {
            processes: [ProcessVm::EMPTY; MAX_PROCESSES],
            regions: RegionTable::new(),
            file_backings: [FileBacking::EMPTY; MAX_FILE_BACKINGS],
        }
    }
}

struct LockedVm {
    lock: AtomicBool,
    state: UnsafeCell<VmState>,
}

unsafe impl Sync for LockedVm {}

static VM: LockedVm = LockedVm {
    lock: AtomicBool::new(false),
    state: UnsafeCell::new(VmState::new()),
};

fn with_state<R>(function: impl FnOnce(&mut VmState) -> R) -> R {
    while VM
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *VM.state.get() });
    VM.lock.store(false, Ordering::Release);
    result
}

pub fn initialize() {
    // VmState is hundreds of KiB. Constructing it as one temporary overflows
    // the 64 KiB boot stack and corrupts neighboring kernel statics.
    with_state(|state| {
        for process in &mut state.processes {
            *process = ProcessVm::EMPTY;
        }
        state.regions.clear();
        for backing in &mut state.file_backings {
            *backing = FileBacking::EMPTY;
        }
    });
    if valid_protection(PROT_WRITE | PROT_EXEC)
        || !valid_protection(PROT_READ | PROT_WRITE)
        || pages_for_length(0).is_some()
        || pages_for_length(PAGE_SIZE + 1) != Some(2)
        || pages_for_length(JIT_RESERVATION_TEST_BYTES).is_none()
    {
        crate::fatal("AArch64 VM policy self-test failed");
    }
    crate::serial_println!(
        "MAKOS_AARCH64_VM_READY page_size=4096 image=0x10000000-0x14000000 heap=0x14000000-0x18000000 mmap=0x80000000-0x3c0000000 stack=0x3fe000000-0x400000000 wx=denied"
    );
}

pub fn attach_process(pid: u64, root: u64, image_end: u64) -> bool {
    let break_base = align_up(image_end.max(crate::arch::USER_HEAP_BASE));
    if pid == 0 || root == 0 || break_base > crate::arch::USER_HEAP_LIMIT {
        return false;
    }
    with_state(|state| {
        if state.processes.iter().any(|process| process.pid == pid) {
            return false;
        }
        let Some(slot) = state.processes.iter_mut().find(|process| process.pid == 0) else {
            return false;
        };
        *slot = ProcessVm {
            pid,
            root,
            break_base,
            current_break: break_base,
        };
        true
    })
}

/// Clone VM accounting for fork. Resident pages were copied by architecture
/// code. Immutable file backings remain lazy; shared mappings become private
/// snapshots until child `execve`, matching current launcher-only fork scope.
pub fn clone_process(parent_pid: u64, child_pid: u64, child_root: u64) -> bool {
    if parent_pid == 0 || child_pid == 0 || child_root == 0 {
        return false;
    }
    with_state(|state| {
        let Some(parent) = state
            .processes
            .iter()
            .copied()
            .find(|process| process.pid == parent_pid)
        else {
            return false;
        };
        if state
            .processes
            .iter()
            .any(|process| process.pid == child_pid)
        {
            return false;
        }
        let readonly_needed = state
            .file_backings
            .iter()
            .filter(|backing| {
                backing.pid == parent_pid
                    && matches!(backing.source, FileBackingSource::ReadOnly(_))
            })
            .count();
        if state
            .file_backings
            .iter()
            .filter(|backing| backing.pid == 0)
            .count()
            < readonly_needed
            || state.processes.iter().all(|process| process.pid != 0)
            || !state.regions.clone_owner(parent_pid, child_pid)
        {
            return false;
        }
        for index in 0..state.file_backings.len() {
            let backing = state.file_backings[index];
            if backing.pid != parent_pid
                || !matches!(backing.source, FileBackingSource::ReadOnly(_))
            {
                continue;
            }
            let slot = state
                .file_backings
                .iter_mut()
                .find(|candidate| candidate.pid == 0)
                .expect("file backing clone preflight mismatch");
            *slot = FileBacking {
                pid: child_pid,
                ..backing
            };
        }
        let slot = state
            .processes
            .iter_mut()
            .find(|process| process.pid == 0)
            .expect("VM process clone preflight mismatch");
        *slot = ProcessVm {
            pid: child_pid,
            root: child_root,
            break_base: parent.break_base,
            current_break: parent.current_break,
        };
        true
    })
}

pub fn close_process(pid: u64) -> (usize, usize) {
    with_state(|state| {
        if let Some(root) = process_root(state, pid) {
            detach_shared_pages(state, pid, root);
        }
        let forgotten = state.regions.forget(pid);
        state
            .file_backings
            .iter_mut()
            .filter(|backing| backing.pid == pid)
            .for_each(|backing| *backing = FileBacking::EMPTY);
        if let Some(process) = state
            .processes
            .iter_mut()
            .find(|process| process.pid == pid)
        {
            *process = ProcessVm::EMPTY;
        }
        forgotten
    })
}

/// Replace VM bookkeeping during successful `exec`. Page-table teardown stays
/// with process owner after it switches away from old root.
pub fn replace_process(pid: u64, root: u64, image_end: u64) -> Option<(u64, usize, usize)> {
    let break_base = align_up(image_end.max(crate::arch::USER_HEAP_BASE));
    if pid == 0 || root == 0 || break_base > crate::arch::USER_HEAP_LIMIT {
        return None;
    }
    with_state(|state| {
        let index = state
            .processes
            .iter()
            .position(|process| process.pid == pid)?;
        let previous_root = state.processes[index].root;
        detach_shared_pages(state, pid, previous_root);
        let (regions, pages) = state.regions.forget(pid);
        state
            .file_backings
            .iter_mut()
            .filter(|backing| backing.pid == pid)
            .for_each(|backing| *backing = FileBacking::EMPTY);
        state.processes[index] = ProcessVm {
            pid,
            root,
            break_base,
            current_break: break_base,
        };
        Some((previous_root, regions, pages))
    })
}

pub fn map(pid: u64, length: u64, protection: u64) -> Option<u64> {
    let pages = pages_for_length(length)?;
    if !valid_protection(protection) {
        return None;
    }
    with_state(|state| {
        process_root(state, pid)?;
        state.regions.allocate_first_fit(
            pid,
            pages,
            protection as u8,
            crate::arch::USER_MMAP_BASE,
            crate::arch::USER_MMAP_LIMIT,
            PAGE_SIZE,
        )
    })
}

pub fn map_file(
    pid: u64,
    requested: u64,
    length: u64,
    protection: u64,
    fixed: bool,
    fd: u64,
    file_offset: u64,
) -> Option<u64> {
    let pages = pages_for_length(length)?;
    if !valid_protection(protection) || file_offset & (PAGE_SIZE - 1) != 0 {
        return None;
    }
    let file = crate::vfs::read_only_backing_for_fd(fd)?;
    if fixed {
        if requested & (PAGE_SIZE - 1) != 0
            || requested < crate::arch::USER_MMAP_BASE
            || requested.checked_add(pages as u64 * PAGE_SIZE)? > crate::arch::USER_MMAP_LIMIT
        {
            return None;
        }
        let covered = with_state(|state| range_covered(&state.regions, pid, requested, pages));
        if covered && !unmap(pid, requested, pages as u64 * PAGE_SIZE) {
            return None;
        }
    }
    let base = with_state(|state| {
        process_root(state, pid)?;
        let base = if fixed {
            state
                .regions
                .allocate_fixed(
                    pid,
                    requested,
                    pages,
                    protection as u8,
                    crate::arch::USER_MMAP_BASE,
                    crate::arch::USER_MMAP_LIMIT,
                    PAGE_SIZE,
                )
                .then_some(requested)?
        } else {
            state.regions.allocate_first_fit(
                pid,
                pages,
                protection as u8,
                crate::arch::USER_MMAP_BASE,
                crate::arch::USER_MMAP_LIMIT,
                PAGE_SIZE,
            )?
        };
        let Some(slot) = state
            .file_backings
            .iter_mut()
            .find(|backing| backing.pid == 0)
        else {
            if !state.regions.remove(pid, base, pages, PAGE_SIZE) {
                crate::fatal("AArch64 file-map rollback lost region");
            }
            return None;
        };
        *slot = FileBacking {
            pid,
            base,
            pages,
            file_offset,
            source: FileBackingSource::ReadOnly(file),
        };
        Some(base)
    })?;
    if crate::aarch64_process::current_app_role() == crate::aarch64_process::ProcessRole::Firefox
        && FIREFOX_FILE_MAP_TRACES.fetch_add(1, Ordering::Relaxed) < VM_TRACE_LIMIT
    {
        let path = match &file {
            crate::vfs::ReadOnlyFileBacking::Embedded(_) => "<embedded>",
            crate::vfs::ReadOnlyFileBacking::Package(package) => {
                core::str::from_utf8(&package.path[..package.path_length]).unwrap_or("<invalid>")
            }
        };
        crate::serial_println!(
            "firefox-map path={} base={:#x} length={:#x} prot={} fixed={} offset={:#x}",
            path,
            base,
            length,
            protection,
            u8::from(fixed),
            file_offset,
        );
    }
    Some(base)
}

pub fn map_shared(
    pid: u64,
    requested: u64,
    length: u64,
    protection: u64,
    fixed: bool,
    fd: u64,
    file_offset: u64,
) -> Option<u64> {
    let pages = pages_for_length(length)?;
    if !valid_protection(protection) || file_offset & (PAGE_SIZE - 1) != 0 {
        return None;
    }
    let backing = crate::vfs::shared_memory_backing_for_fd(fd, protection & PROT_WRITE != 0)?;
    if file_offset >= backing.size
        || file_offset.checked_add(length)? > backing.size
        || (fixed
            && (requested & (PAGE_SIZE - 1) != 0
                || requested < crate::arch::USER_MMAP_BASE
                || requested.checked_add(pages as u64 * PAGE_SIZE)? > crate::arch::USER_MMAP_LIMIT))
    {
        return None;
    }
    if fixed {
        let covered = with_state(|state| range_covered(&state.regions, pid, requested, pages));
        if covered && !unmap(pid, requested, pages as u64 * PAGE_SIZE) {
            return None;
        }
    }
    let base = with_state(|state| {
        process_root(state, pid)?;
        let base = if fixed {
            state
                .regions
                .allocate_fixed(
                    pid,
                    requested,
                    pages,
                    protection as u8,
                    crate::arch::USER_MMAP_BASE,
                    crate::arch::USER_MMAP_LIMIT,
                    PAGE_SIZE,
                )
                .then_some(requested)?
        } else {
            state.regions.allocate_first_fit(
                pid,
                pages,
                protection as u8,
                crate::arch::USER_MMAP_BASE,
                crate::arch::USER_MMAP_LIMIT,
                PAGE_SIZE,
            )?
        };
        let Some(slot) = state
            .file_backings
            .iter_mut()
            .find(|candidate| candidate.pid == 0)
        else {
            if !state.regions.remove(pid, base, pages, PAGE_SIZE) {
                crate::fatal("AArch64 shared-map rollback lost region");
            }
            return None;
        };
        if !crate::aarch64_shmem::retain_mapping(backing.object) {
            if !state.regions.remove(pid, base, pages, PAGE_SIZE) {
                crate::fatal("AArch64 shared-map ref rollback lost region");
            }
            return None;
        }
        *slot = FileBacking {
            pid,
            base,
            pages,
            file_offset,
            source: FileBackingSource::Shared(backing),
        };
        Some(base)
    })?;
    if SHMEM_MAP_TRACES.fetch_add(1, Ordering::Relaxed) < VM_TRACE_LIMIT {
        crate::serial_println!(
            "MAKOS_AARCH64_SHMEM_MAP pid={} object={} base={:#x} length={:#x} prot={} fixed={} offset={:#x}",
            pid,
            backing.object,
            base,
            length,
            protection,
            u8::from(fixed),
            file_offset,
        );
    }
    Some(base)
}

pub fn map_anonymous_fixed(pid: u64, base: u64, length: u64, protection: u64) -> Option<u64> {
    let Some(pages) = pages_for_length(length) else {
        crate::serial_println!(
            "MAKOS_AARCH64_ANON_FIXED_FAIL reason=length pid={} base={:#x} length={:#x} prot={}",
            pid,
            base,
            length,
            protection,
        );
        return None;
    };
    if !valid_protection(protection)
        || base & (PAGE_SIZE - 1) != 0
        || base < crate::arch::USER_ADDRESS_BASE
        || base.checked_add(pages as u64 * PAGE_SIZE)? > crate::arch::USER_MMAP_LIMIT
    {
        crate::serial_println!(
            "MAKOS_AARCH64_ANON_FIXED_FAIL reason=invalid pid={} base={:#x} length={:#x} prot={}",
            pid,
            base,
            length,
            protection,
        );
        return None;
    }
    // MAP_FIXED replaces every existing mapping in range.  This includes
    // low user mappings (for example ELF/brk guard pages), metadata-backed
    // VMAs, resident legacy mappings, and holes.
    if !unmap(pid, base, pages as u64 * PAGE_SIZE) {
        crate::serial_println!(
            "MAKOS_AARCH64_ANON_FIXED_FAIL reason=unmap pid={} base={:#x} length={:#x} prot={}",
            pid,
            base,
            length,
            protection,
        );
        return None;
    }
    with_state(|state| {
        if process_root(state, pid).is_none() {
            crate::serial_println!(
                "MAKOS_AARCH64_ANON_FIXED_FAIL reason=process pid={} base={:#x} length={:#x} prot={}",
                pid,
                base,
                length,
                protection,
            );
            return None;
        }
        if !state.regions.allocate_fixed(
            pid,
            base,
            pages,
            protection as u8,
            crate::arch::USER_ADDRESS_BASE,
            crate::arch::USER_MMAP_LIMIT,
            PAGE_SIZE,
        ) {
            crate::serial_println!(
                "MAKOS_AARCH64_ANON_FIXED_FAIL reason=allocate pid={} base={:#x} length={:#x} prot={} regions={} free_slots={}",
                pid,
                base,
                length,
                protection,
                state.regions.count(pid),
                state.regions.free_slots(),
            );
            return None;
        }
        Some(base)
    })
}

pub fn handle_page_fault(pid: u64, address: u64, write: bool, execute: bool) -> bool {
    let page = address & !(PAGE_SIZE - 1);
    let Some((root, protection, backing)) = with_state(|state| {
        let root = process_root(state, pid)?;
        let region = state.regions.find(pid, page, PAGE_SIZE)?;
        let backing = state.file_backings.iter().copied().find(|backing| {
            backing.pid == pid
                && page >= backing.base
                && page < backing.end().unwrap_or(backing.base)
        });
        Some((root, u64::from(region.protection), backing))
    }) else {
        return false;
    };
    if (execute && protection & PROT_EXEC == 0)
        || (write && protection & PROT_WRITE == 0)
        || (!write && !execute && protection & PROT_READ == 0)
        || crate::arch::user_page_physical_in(root, page).is_some()
    {
        return false;
    }
    if let Some(FileBacking {
        base,
        file_offset,
        source: FileBackingSource::Shared(shared),
        ..
    }) = backing
    {
        let offset = file_offset.checked_add(page.checked_sub(base).unwrap_or(0));
        let Some(frame) =
            offset.and_then(|offset| crate::aarch64_shmem::page_frame(shared.object, offset))
        else {
            return false;
        };
        crate::arch::map_user_page_permissions_in(
            root,
            page,
            frame,
            protection & PROT_READ != 0,
            protection & PROT_WRITE != 0,
            protection & PROT_EXEC != 0,
        );
        return true;
    }
    let Some(frame) = crate::mm::allocate_frame() else {
        return false;
    };
    unsafe { ptr::write_bytes(frame as *mut u8, 0, PAGE_SIZE as usize) };
    let output = unsafe { core::slice::from_raw_parts_mut(frame as *mut u8, PAGE_SIZE as usize) };
    let loaded = match backing {
        None => Some(0),
        Some(backing) => {
            let file_offset = backing
                .file_offset
                .saturating_add(page.saturating_sub(backing.base));
            match backing.source {
                FileBackingSource::Empty => Some(0),
                FileBackingSource::ReadOnly(crate::vfs::ReadOnlyFileBacking::Embedded(bytes)) => {
                    let offset = usize::try_from(file_offset).unwrap_or(usize::MAX);
                    let count = output.len().min(bytes.len().saturating_sub(offset));
                    if count != 0 {
                        output[..count].copy_from_slice(&bytes[offset..offset + count]);
                    }
                    Some(count)
                }
                FileBackingSource::ReadOnly(crate::vfs::ReadOnlyFileBacking::Package(file)) => {
                    crate::fs::read_package_file(&file, file_offset, output)
                }
                FileBackingSource::Shared(_) => None,
            }
        }
    };
    if loaded.is_none() {
        let _ = crate::mm::free_frame(frame);
        return false;
    }
    if protection & PROT_EXEC != 0 {
        crate::arch::sync_user_code(frame);
    }
    crate::arch::map_user_page_permissions_in(
        root,
        page,
        frame,
        protection & PROT_READ != 0,
        protection & PROT_WRITE != 0,
        protection & PROT_EXEC != 0,
    );
    true
}

/// Resolve lazy mappings before kernel copies into/out of user buffers.
pub fn fault_in_range(pid: u64, address: u64, length: usize, write: bool) -> bool {
    let Some(end) = address.checked_add(length as u64) else {
        return false;
    };
    if address < crate::arch::USER_ADDRESS_BASE
        || end > crate::arch::USER_STACK_TOP
        || length > 16 * 1024 * 1024
    {
        return false;
    }
    if length == 0 {
        return true;
    }
    let root = with_state(|state| process_root(state, pid));
    let Some(root) = root else {
        return false;
    };
    let mut page = address & !(PAGE_SIZE - 1);
    while page < end {
        if crate::arch::user_page_physical_in(root, page).is_none()
            && !handle_page_fault(pid, page, write, false)
        {
            return false;
        }
        page += PAGE_SIZE;
    }
    true
}

pub fn trace_address(pid: u64, label: &str, address: u64) {
    let page = address & !(PAGE_SIZE - 1);
    let (root, region) = with_state(|state| {
        (
            process_root(state, pid),
            state.regions.find(pid, page, PAGE_SIZE),
        )
    });
    let physical = root.and_then(|root| crate::arch::user_page_physical_in(root, page));
    crate::serial_println!(
        "MAKOS_AARCH64_VM_TRACE label={} address={:#x} page={:#x} region_base={:#x} region_pages={} prot={} physical={:#x} free_frames={}",
        label,
        address,
        page,
        region.map_or(0, |region| region.base),
        region.map_or(0, |region| region.pages),
        region.map_or(0, |region| region.protection),
        physical.unwrap_or(0),
        crate::mm::free_frames(),
    );
}

pub fn unmap(pid: u64, base: u64, length: u64) -> bool {
    let Some(pages) = exact_pages(base, length) else {
        return false;
    };
    let Some(end) = base.checked_add(pages as u64 * PAGE_SIZE) else {
        return false;
    };
    if base < crate::arch::USER_ADDRESS_BASE || end > crate::arch::USER_MMAP_LIMIT {
        return false;
    }
    let Some(root) = with_state(|state| {
        let root = process_root(state, pid)?;
        let mut cursor = base;
        while cursor < end {
            if let Some(region) = state.regions.find(pid, cursor, PAGE_SIZE) {
                let chunk_end = region.end(PAGE_SIZE)?.min(end);
                let chunk_pages = ((chunk_end - cursor) / PAGE_SIZE) as usize;
                if !state.regions.remove(pid, cursor, chunk_pages, PAGE_SIZE) {
                    crate::fatal("AArch64 VM range metadata removal failed");
                }
                cursor = chunk_end;
            } else {
                cursor += PAGE_SIZE;
            }
        }
        Some(root)
    }) else {
        return false;
    };
    for page in 0..pages {
        let address = base + page as u64 * PAGE_SIZE;
        let shared = with_state(|state| shared_backing_at(state, pid, address).is_some());
        if let Some(frame) = crate::arch::unmap_user_page_in(root, address) {
            if !shared && crate::mm::free_frame(frame).is_err() {
                crate::fatal("AArch64 VM frame release failed");
            }
        }
    }
    with_state(|state| {
        if !remove_file_backing_range(state, pid, base, pages) {
            crate::fatal("AArch64 file backing split capacity exhausted");
        }
    });
    true
}

/// Applies Linux/POSIX-style mapping advice. DONTNEED/FREE release resident
/// frames while preserving region metadata, so later faults restore zeroed
/// anonymous pages or reload immutable file backing. Other accepted values are
/// policy hints already satisfied by MakOS's demand-paged 4 KiB mappings.
pub fn advise(pid: u64, base: u64, length: u64, advice: u64) -> bool {
    const MADV_NORMAL: u64 = 0;
    const MADV_RANDOM: u64 = 1;
    const MADV_SEQUENTIAL: u64 = 2;
    const MADV_WILLNEED: u64 = 3;
    const MADV_DONTNEED: u64 = 4;
    const MADV_FREE: u64 = 8;
    const MADV_HUGEPAGE: u64 = 14;
    const MADV_NOHUGEPAGE: u64 = 15;
    const MADV_DONTDUMP: u64 = 16;
    const MADV_DODUMP: u64 = 17;

    if !matches!(
        advice,
        MADV_NORMAL
            | MADV_RANDOM
            | MADV_SEQUENTIAL
            | MADV_WILLNEED
            | MADV_DONTNEED
            | MADV_FREE
            | MADV_HUGEPAGE
            | MADV_NOHUGEPAGE
            | MADV_DONTDUMP
            | MADV_DODUMP
    ) {
        return false;
    }
    let Some(pages) = exact_pages(base, length) else {
        return false;
    };
    let Some(root) = with_state(|state| {
        let root = process_root(state, pid)?;
        if !range_covered(&state.regions, pid, base, pages) {
            return None;
        }
        if advice == MADV_FREE {
            let end = base.checked_add(pages as u64 * PAGE_SIZE)?;
            if state.file_backings.iter().any(|backing| {
                backing.pid == pid
                    && backing.base < end
                    && backing.end().is_some_and(|backing_end| base < backing_end)
            }) {
                return None;
            }
        }
        Some(root)
    }) else {
        return false;
    };
    if !matches!(advice, MADV_DONTNEED | MADV_FREE) {
        return true;
    }
    for page in 0..pages {
        let address = base + page as u64 * PAGE_SIZE;
        let shared = with_state(|state| shared_backing_at(state, pid, address).is_some());
        if let Some(frame) = crate::arch::unmap_user_page_in(root, address) {
            if !shared && crate::mm::free_frame(frame).is_err() {
                crate::fatal("AArch64 madvise frame release failed");
            }
        }
    }
    true
}

pub fn protect(pid: u64, base: u64, length: u64, protection: u64) -> bool {
    let Some(pages) = exact_pages(base, length) else {
        return false;
    };
    if !valid_protection(protection) {
        return false;
    }
    let Some(root) = with_state(|state| {
        let root = process_root(state, pid)?;
        if !range_covered(&state.regions, pid, base, pages) {
            let end = base.checked_add(pages as u64 * PAGE_SIZE)?;
            if base < crate::arch::USER_ADDRESS_BASE
                || end > crate::arch::USER_MMAP_LIMIT
                || (0..pages).any(|page| {
                    crate::arch::user_page_physical_in(root, base + page as u64 * PAGE_SIZE)
                        .is_none()
                })
            {
                return None;
            }
            return Some(root);
        }
        let end = base + pages as u64 * PAGE_SIZE;
        let mut cursor = base;
        let mut extra_slots = 0usize;
        while cursor < end {
            let region = state.regions.find(pid, cursor, PAGE_SIZE)?;
            let chunk_end = region.end(PAGE_SIZE)?.min(end);
            if region.protection != protection as u8 {
                extra_slots += usize::from(cursor != region.base);
                extra_slots += usize::from(chunk_end != region.end(PAGE_SIZE)?);
            }
            cursor = chunk_end;
        }
        if state.regions.free_slots() < extra_slots {
            crate::serial_println!(
                "MAKOS_AARCH64_PROTECT_FAIL reason=slots pid={} base={:#x} length={:#x} prot={} needed={} regions={} free_slots={}",
                pid,
                base,
                length,
                protection,
                extra_slots,
                state.regions.count(pid),
                state.regions.free_slots(),
            );
            return None;
        }
        cursor = base;
        while cursor < end {
            let region = state.regions.find(pid, cursor, PAGE_SIZE)?;
            let chunk_end = region.end(PAGE_SIZE)?.min(end);
            let chunk_pages = ((chunk_end - cursor) / PAGE_SIZE) as usize;
            if region.protection != protection as u8
                && !state
                    .regions
                    .protect(pid, cursor, chunk_pages, protection as u8, PAGE_SIZE)
            {
                crate::fatal("AArch64 VM range metadata protection failed");
            }
            cursor = chunk_end;
        }
        Some(root)
    }) else {
        return false;
    };
    let writable = protection & PROT_WRITE != 0;
    let executable = protection & PROT_EXEC != 0;
    for page in 0..pages {
        let address = base + page as u64 * PAGE_SIZE;
        if executable {
            if let Some(frame) = crate::arch::user_page_physical_in(root, address) {
                crate::arch::sync_user_code(frame);
            }
        }
        if crate::arch::user_page_physical_in(root, address).is_some()
            && !crate::arch::protect_user_page_permissions_in(
                root,
                address,
                protection & PROT_READ != 0,
                writable,
                executable,
            )
        {
            crate::fatal("AArch64 VM metadata/page-table mismatch on protect");
        }
    }
    true
}

pub fn brk(pid: u64, requested: u64) -> Option<u64> {
    let process = with_state(|state| {
        state
            .processes
            .iter()
            .copied()
            .find(|process| process.pid == pid)
    })?;
    if requested == 0 {
        return Some(process.current_break);
    }
    if requested < process.break_base || requested > crate::arch::USER_HEAP_LIMIT {
        return None;
    }
    let old_limit = align_up(process.current_break);
    let new_limit = align_up(requested);
    if new_limit > old_limit {
        let pages = ((new_limit - old_limit) / PAGE_SIZE) as usize;
        let mut mapped = 0usize;
        while mapped < pages {
            let Some(frame) = crate::mm::allocate_frame() else {
                rollback_brk(process.root, old_limit, mapped);
                return None;
            };
            unsafe { ptr::write_bytes(frame as *mut u8, 0, PAGE_SIZE as usize) };
            crate::arch::map_user_page_in(
                process.root,
                old_limit + mapped as u64 * PAGE_SIZE,
                frame,
                true,
                false,
            );
            mapped += 1;
        }
    } else if new_limit < old_limit {
        let pages = ((old_limit - new_limit) / PAGE_SIZE) as usize;
        for page in 0..pages {
            let address = new_limit + page as u64 * PAGE_SIZE;
            let frame = crate::arch::unmap_user_page_in(process.root, address)
                .unwrap_or_else(|| crate::fatal("AArch64 brk tracked page absent"));
            if crate::mm::free_frame(frame).is_err() {
                crate::fatal("AArch64 brk frame release failed");
            }
        }
    }
    with_state(|state| {
        let process = state
            .processes
            .iter_mut()
            .find(|process| process.pid == pid)
            .unwrap_or_else(|| crate::fatal("AArch64 brk process disappeared"));
        process.current_break = requested;
    });
    Some(requested)
}

fn process_root(state: &VmState, pid: u64) -> Option<u64> {
    state
        .processes
        .iter()
        .find(|process| process.pid == pid)
        .map(|process| process.root)
}

fn remove_file_backing_range(state: &mut VmState, pid: u64, base: u64, pages: usize) -> bool {
    let Some(end) = base.checked_add(pages as u64 * PAGE_SIZE) else {
        return false;
    };
    for index in 0..state.file_backings.len() {
        let backing = state.file_backings[index];
        if backing.pid != pid {
            continue;
        }
        let Some(backing_end) = backing.end() else {
            return false;
        };
        if base >= backing_end || backing.base >= end {
            continue;
        }
        let removed_start = base.max(backing.base);
        let removed_end = end.min(backing_end);
        let left_pages = ((removed_start - backing.base) / PAGE_SIZE) as usize;
        let right_pages = ((backing_end - removed_end) / PAGE_SIZE) as usize;
        match (left_pages != 0, right_pages != 0) {
            (true, true) => {
                let Some(right_slot) = state
                    .file_backings
                    .iter()
                    .enumerate()
                    .find(|(slot, candidate)| *slot != index && candidate.pid == 0)
                    .map(|(slot, _)| slot)
                else {
                    return false;
                };
                state.file_backings[index].pages = left_pages;
                if let FileBackingSource::Shared(shared) = backing.source
                    && !crate::aarch64_shmem::retain_mapping(shared.object)
                {
                    crate::fatal("AArch64 shared backing split ref overflow");
                }
                state.file_backings[right_slot] = FileBacking {
                    base: removed_end,
                    pages: right_pages,
                    file_offset: backing.file_offset + removed_end - backing.base,
                    ..backing
                };
            }
            (true, false) => state.file_backings[index].pages = left_pages,
            (false, true) => {
                state.file_backings[index] = FileBacking {
                    base: removed_end,
                    pages: right_pages,
                    file_offset: backing.file_offset + removed_end - backing.base,
                    ..backing
                };
            }
            (false, false) => {
                if let FileBackingSource::Shared(shared) = backing.source {
                    crate::aarch64_shmem::release_mapping(shared.object);
                }
                state.file_backings[index] = FileBacking::EMPTY;
            }
        }
    }
    true
}

fn shared_backing_at(
    state: &VmState,
    pid: u64,
    address: u64,
) -> Option<crate::vfs::SharedMemoryBacking> {
    state.file_backings.iter().find_map(|backing| {
        (backing.pid == pid
            && address >= backing.base
            && address < backing.end().unwrap_or(backing.base))
        .then_some(backing.source)
        .and_then(|source| match source {
            FileBackingSource::Shared(shared) => Some(shared),
            _ => None,
        })
    })
}

fn detach_shared_pages(state: &mut VmState, pid: u64, root: u64) {
    for backing in &mut state.file_backings {
        if backing.pid != pid {
            continue;
        }
        let FileBackingSource::Shared(shared) = backing.source else {
            continue;
        };
        for page in 0..backing.pages {
            let address = backing.base + page as u64 * PAGE_SIZE;
            let _ = crate::arch::unmap_user_page_in(root, address);
        }
        crate::aarch64_shmem::release_mapping(shared.object);
        *backing = FileBacking::EMPTY;
    }
}

fn range_covered(regions: &RegionTable<MAX_REGIONS>, pid: u64, base: u64, pages: usize) -> bool {
    let Some(end) = base.checked_add(pages as u64 * PAGE_SIZE) else {
        return false;
    };
    let mut cursor = base;
    while cursor < end {
        let Some(region) = regions.find(pid, cursor, PAGE_SIZE) else {
            return false;
        };
        let Some(region_end) = region.end(PAGE_SIZE) else {
            return false;
        };
        cursor = region_end.min(end);
    }
    true
}

fn rollback_brk(root: u64, base: u64, mapped: usize) {
    for page in 0..mapped {
        let frame = crate::arch::unmap_user_page_in(root, base + page as u64 * PAGE_SIZE)
            .unwrap_or_else(|| crate::fatal("AArch64 brk rollback lost page"));
        if crate::mm::free_frame(frame).is_err() {
            crate::fatal("AArch64 brk rollback release failed");
        }
    }
}

fn pages_for_length(length: u64) -> Option<usize> {
    if length == 0 {
        return None;
    }
    let pages = length.checked_add(PAGE_SIZE - 1)? / PAGE_SIZE;
    let pages = usize::try_from(pages).ok()?;
    (pages <= MAX_PAGES_PER_CALL).then_some(pages)
}

fn exact_pages(base: u64, length: u64) -> Option<usize> {
    if base & (PAGE_SIZE - 1) != 0 {
        return None;
    }
    pages_for_length(length)
}

fn valid_protection(protection: u64) -> bool {
    protection & !7 == 0
        && (protection == 0 || protection & PROT_READ != 0)
        && protection & (PROT_WRITE | PROT_EXEC) != (PROT_WRITE | PROT_EXEC)
}

fn align_up(value: u64) -> u64 {
    value
        .checked_add(PAGE_SIZE - 1)
        .map(|value| value & !(PAGE_SIZE - 1))
        .unwrap_or(u64::MAX)
}
