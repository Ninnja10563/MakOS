use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use makos_elf64::{ET_DYN, Elf64, PT_INTERP, PT_LOAD};
use makos_pe64::{IMAGE_SCN_MEM_EXECUTE, IMAGE_SCN_MEM_READ, IMAGE_SCN_MEM_WRITE, Pe64};

pub const USER_CODE_BASE: u64 = 0x1_0000_0000;
pub const USER_CODE_LIMIT: u64 = USER_CODE_BASE + 0x20_000;
pub const USER_STACK_BASE: u64 = USER_CODE_BASE + 0x30_000;
const USER_STACK_PAGES: usize = 4;
const USER_STACK_END: u64 = USER_STACK_BASE + USER_STACK_PAGES as u64 * 4096;
pub const USER_STACK_TOP: u64 = USER_STACK_END - 8;
pub const SPAWN_ARGUMENTS_VERSION: u32 = 1;
pub const SPAWN_ARGUMENTS_BYTES: usize = 336;
const SPAWN_MAX_ARGUMENTS: usize = 8;
const SPAWN_MAX_ENVIRONMENT: usize = 8;
const SPAWN_DATA_BYTES: usize = 256;

static INIT_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/init.elf"));
static WORKER_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/worker.elf"));
static LINUX_COMPAT_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/linux_compat.elf"));
static WINDOWS_COMPAT_PE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/windows_compat.exe"));
static SERVICE_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/service.elf"));
static TOOLCHAIN_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/toolchain.elf"));
static DYNAMIC_LINKER_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ld-makos.so"));
static DYNAMIC_APP_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/dynamic-app.elf"));
static DYNAMIC_LIBRARY_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/libmakosdemo.so"));
const DYNAMIC_PID: u64 = 9;
const DYNAMIC_APP_BASE: u64 = 0x1_0100_0000;
const DYNAMIC_LIBRARY_BASE: u64 = 0x1_0200_0000;
const DYNAMIC_IMAGE_BYTES: u64 = 0x10_0000;
const RING0_STACK_BYTES: usize = 64 * 1024;
pub const USER_THREAD_STACK_BASE: u64 = USER_CODE_BASE + 0x20_0000;
pub const USER_THREAD_STACK_TOP: u64 = USER_THREAD_STACK_BASE + 4096 - 8;

#[repr(C, align(16))]
struct Ring0Stack([u8; RING0_STACK_BYTES]);

static mut INIT_RING0_STACK: Ring0Stack = Ring0Stack([0; RING0_STACK_BYTES]);
static mut WORKER_RING0_STACK: Ring0Stack = Ring0Stack([0; RING0_STACK_BYTES]);
static mut LINUX_RING0_STACK: Ring0Stack = Ring0Stack([0; RING0_STACK_BYTES]);
static mut WINDOWS_RING0_STACK: Ring0Stack = Ring0Stack([0; RING0_STACK_BYTES]);
static mut SERVICE_RING0_STACK: Ring0Stack = Ring0Stack([0; RING0_STACK_BYTES]);
static mut TOOLCHAIN_RING0_STACK: Ring0Stack = Ring0Stack([0; RING0_STACK_BYTES]);
static mut DYNAMIC_RING0_STACK: Ring0Stack = Ring0Stack([0; RING0_STACK_BYTES]);
const PATH_EXEC_SLOTS: usize = 2;
const PATH_EXEC_FIRST_PID: u64 = 7;
static mut PATH_EXEC_RING0_STACKS: [Ring0Stack; PATH_EXEC_SLOTS] =
    [const { Ring0Stack([0; RING0_STACK_BYTES]) }; PATH_EXEC_SLOTS];
static mut THREAD_RING0_STACK: Ring0Stack = Ring0Stack([0; RING0_STACK_BYTES]);
const STILL_RUNNING: u64 = u64::MAX;
static WORKER_EXIT: AtomicU64 = AtomicU64::new(STILL_RUNNING);
static LINUX_EXIT: AtomicU64 = AtomicU64::new(STILL_RUNNING);
static WINDOWS_EXIT: AtomicU64 = AtomicU64::new(STILL_RUNNING);
static SERVICE_EXIT: AtomicU64 = AtomicU64::new(STILL_RUNNING);
static TOOLCHAIN_EXIT: AtomicU64 = AtomicU64::new(STILL_RUNNING);
static DYNAMIC_EXIT: AtomicU64 = AtomicU64::new(STILL_RUNNING);
static DYNAMIC_APP_ENTRY: AtomicU64 = AtomicU64::new(0);
static PATH_EXEC_EXITS: [AtomicU64; PATH_EXEC_SLOTS] =
    [const { AtomicU64::new(STILL_RUNNING) }; PATH_EXEC_SLOTS];
static PATH_EXEC_ENTRIES: [AtomicU64; PATH_EXEC_SLOTS] =
    [const { AtomicU64::new(0) }; PATH_EXEC_SLOTS];
static PATH_EXEC_STACKS: [AtomicU64; PATH_EXEC_SLOTS] =
    [const { AtomicU64::new(USER_STACK_TOP) }; PATH_EXEC_SLOTS];
static PATH_EXEC_ARGC: [AtomicU64; PATH_EXEC_SLOTS] =
    [const { AtomicU64::new(0) }; PATH_EXEC_SLOTS];
static PATH_EXEC_ARGV: [AtomicU64; PATH_EXEC_SLOTS] =
    [const { AtomicU64::new(0) }; PATH_EXEC_SLOTS];
static PATH_EXEC_ENVP: [AtomicU64; PATH_EXEC_SLOTS] =
    [const { AtomicU64::new(0) }; PATH_EXEC_SLOTS];
static PATH_EXEC_USED: [AtomicBool; PATH_EXEC_SLOTS] =
    [const { AtomicBool::new(false) }; PATH_EXEC_SLOTS];
static SERVICE_GENERATION: AtomicU64 = AtomicU64::new(0);
static THREAD_STACK_FRAME: AtomicU64 = AtomicU64::new(0);
static THREAD_ENTRY: AtomicU64 = AtomicU64::new(0);
static THREAD_ARGUMENT: AtomicU64 = AtomicU64::new(0);
static THREAD_EXIT: AtomicU64 = AtomicU64::new(STILL_RUNNING);
const MAX_ELF_LOAD_SEGMENTS: usize = 4;

#[derive(Clone, Copy)]
struct SpawnArguments {
    argc: usize,
    envc: usize,
    argv_offsets: [usize; SPAWN_MAX_ARGUMENTS],
    env_offsets: [usize; SPAWN_MAX_ENVIRONMENT],
    data_length: usize,
    data: [u8; SPAWN_DATA_BYTES],
}

impl SpawnArguments {
    const EMPTY: Self = Self {
        argc: 0,
        envc: 0,
        argv_offsets: [0; SPAWN_MAX_ARGUMENTS],
        env_offsets: [0; SPAWN_MAX_ENVIRONMENT],
        data_length: 0,
        data: [0; SPAWN_DATA_BYTES],
    };
}

#[derive(Clone, Copy)]
struct UserStartup {
    stack_pointer: u64,
    argc: u64,
    argv: u64,
    envp: u64,
}

pub fn launch_init() -> ! {
    let root = crate::arch::new_user_address_space();
    load_elf(INIT_ELF, root);
    crate::arch::switch_address_space(root);
    let ring0_top = (&raw mut INIT_RING0_STACK).cast::<u8>() as u64 + RING0_STACK_BYTES as u64;
    crate::arch::set_ring0_stack(ring0_top);
    crate::scheduler::configure_init(1, root, ring0_top);
    let elf = Elf64::parse(INIT_ELF).unwrap_or_else(|_| crate::fatal("init ELF validation failed"));
    crate::serial_println!(
        "process init elf=ok entry={:#x} stack={:#x} ring=3 pid=1 cr3={:#x}",
        elf.entry(),
        USER_STACK_TOP,
        root
    );
    crate::arch::enter_user(elf.entry(), USER_STACK_TOP, 0)
}

fn supported_elf_layout(bytes: &[u8]) -> Option<(u64, usize)> {
    let elf = Elf64::parse(bytes).ok()?;
    let mut loaded = 0usize;
    let mut executable_entry = false;
    let mut mapped_ranges = [(0u64, 0u64); MAX_ELF_LOAD_SEGMENTS];
    for segment in elf
        .program_headers()
        .filter(|header| header.segment_type == PT_LOAD)
    {
        let end = segment.virtual_address.checked_add(segment.memory_size)?;
        let mapped_end = end.checked_add(4095)? & !4095;
        if loaded == MAX_ELF_LOAD_SEGMENTS
            || segment.virtual_address < USER_CODE_BASE
            || segment.virtual_address & 4095 != 0
            || segment.memory_size == 0
            || end > USER_CODE_LIMIT
            || segment.file_size > segment.memory_size
            || segment.flags & 4 == 0
            || segment.flags & !7 != 0
            || segment.flags & 3 == 3
            || (segment.alignment != 0
                && segment.alignment != 1
                && (!segment.alignment.is_power_of_two()
                    || segment.offset % segment.alignment
                        != segment.virtual_address % segment.alignment))
            || mapped_ranges[..loaded].iter().any(|(start, prior_end)| {
                segment.virtual_address < *prior_end && *start < mapped_end
            })
        {
            return None;
        }
        if segment.flags & 1 != 0 && (segment.virtual_address..end).contains(&elf.entry()) {
            executable_entry = true;
        }
        mapped_ranges[loaded] = (segment.virtual_address, mapped_end);
        loaded += 1;
    }
    (loaded != 0 && executable_entry).then_some((elf.entry(), loaded))
}

fn load_elf(bytes: &[u8], root: u64) -> u64 {
    load_elf_with_stack(bytes, root).0
}

fn load_elf_with_stack(bytes: &[u8], root: u64) -> (u64, [u64; USER_STACK_PAGES]) {
    let (entry, _) = supported_elf_layout(bytes)
        .unwrap_or_else(|| crate::fatal("userspace ELF validation failed"));
    let elf = Elf64::parse(bytes).unwrap_or_else(|_| crate::fatal("validated ELF changed"));
    let mut loaded = 0usize;
    for segment in elf
        .program_headers()
        .filter(|header| header.segment_type == PT_LOAD)
    {
        let page_count = segment.memory_size.div_ceil(4096) as usize;
        for page in 0..page_count {
            let frame =
                crate::mm::allocate_frame().unwrap_or_else(|| crate::fatal("init code frame OOM"));
            unsafe { ptr::write_bytes(frame as *mut u8, 0, 4096) };
            let segment_offset = page * 4096;
            let copy_count = (segment.file_size as usize)
                .saturating_sub(segment_offset)
                .min(4096);
            if copy_count != 0 {
                unsafe {
                    ptr::copy_nonoverlapping(
                        bytes.as_ptr().add(segment.offset as usize + segment_offset),
                        frame as *mut u8,
                        copy_count,
                    );
                }
            }
            crate::arch::map_user_page_in(
                root,
                segment.virtual_address + page as u64 * 4096,
                frame,
                segment.flags & 2 != 0,
                segment.flags & 1 != 0,
            );
        }
        loaded += 1;
    }
    if loaded == 0 || loaded > MAX_ELF_LOAD_SEGMENTS {
        crate::fatal("ELF load count changed after validation");
    }
    let stack_frames = map_user_stack(root);
    (entry, stack_frames)
}

fn supported_dynamic_elf_layout(
    bytes: &[u8],
    base: u64,
    require_entry: bool,
) -> Option<(u64, usize)> {
    let elf = Elf64::parse(bytes).ok()?;
    if elf.elf_type() != ET_DYN || base & 4095 != 0 {
        return None;
    }
    let image_limit = base.checked_add(DYNAMIC_IMAGE_BYTES)?;
    let entry = base.checked_add(elf.entry())?;
    let mut loaded = 0usize;
    let mut executable_segment = false;
    let mut executable_entry = false;
    let mut mapped_ranges = [(0u64, 0u64); MAX_ELF_LOAD_SEGMENTS];
    for segment in elf
        .program_headers()
        .filter(|header| header.segment_type == PT_LOAD)
    {
        let segment_start = base.checked_add(segment.virtual_address)?;
        let segment_end = segment_start.checked_add(segment.memory_size)?;
        let mapped_start = segment_start & !4095;
        let mapped_end = segment_end.checked_add(4095)? & !4095;
        if loaded == MAX_ELF_LOAD_SEGMENTS
            || segment.memory_size == 0
            || segment.file_size > segment.memory_size
            || segment_start < base
            || segment_end > image_limit
            || segment.flags & 4 == 0
            || segment.flags & !7 != 0
            || segment.flags & 3 == 3
            || segment.offset & 4095 != segment.virtual_address & 4095
            || (segment.alignment != 0
                && segment.alignment != 1
                && (!segment.alignment.is_power_of_two()
                    || segment.offset % segment.alignment
                        != segment.virtual_address % segment.alignment))
            || mapped_ranges[..loaded]
                .iter()
                .any(|(start, end)| mapped_start < *end && *start < mapped_end)
        {
            return None;
        }
        if segment.flags & 1 != 0 && (segment_start..segment_end).contains(&entry) {
            executable_entry = true;
        }
        executable_segment |= segment.flags & 1 != 0;
        mapped_ranges[loaded] = (mapped_start, mapped_end);
        loaded += 1;
    }
    (loaded != 0 && executable_segment && (!require_entry || executable_entry))
        .then_some((entry, loaded))
}

fn load_dynamic_elf(bytes: &[u8], root: u64, base: u64, require_entry: bool) -> (u64, usize) {
    let (entry, segment_count) = supported_dynamic_elf_layout(bytes, base, require_entry)
        .unwrap_or_else(|| crate::fatal("dynamic ELF validation failed"));
    let elf = Elf64::parse(bytes).unwrap_or_else(|_| crate::fatal("validated dynamic ELF changed"));
    for segment in elf
        .program_headers()
        .filter(|header| header.segment_type == PT_LOAD)
    {
        let segment_start = base + segment.virtual_address;
        let mapped_start = segment_start & !4095;
        let segment_file_end = segment_start + segment.file_size;
        let segment_memory_end = segment_start + segment.memory_size;
        let mapped_end = (segment_memory_end + 4095) & !4095;
        let mut page_address = mapped_start;
        while page_address < mapped_end {
            let frame = crate::mm::allocate_frame()
                .unwrap_or_else(|| crate::fatal("dynamic image frame OOM"));
            unsafe { ptr::write_bytes(frame as *mut u8, 0, 4096) };
            let copy_start = page_address.max(segment_start);
            let copy_end = (page_address + 4096).min(segment_file_end);
            if copy_start < copy_end {
                let source_offset = segment.offset + copy_start - segment_start;
                let destination_offset = copy_start - page_address;
                unsafe {
                    ptr::copy_nonoverlapping(
                        bytes.as_ptr().add(source_offset as usize),
                        (frame as *mut u8).add(destination_offset as usize),
                        (copy_end - copy_start) as usize,
                    );
                }
            }
            crate::arch::map_user_page_in(
                root,
                page_address,
                frame,
                segment.flags & 2 != 0,
                segment.flags & 1 != 0,
            );
            page_address += 4096;
        }
    }
    (entry, segment_count)
}

fn has_dynamic_interpreter(bytes: &[u8]) -> bool {
    let Ok(elf) = Elf64::parse(bytes) else {
        return false;
    };
    let mut interpreters = elf
        .program_headers()
        .filter(|header| header.segment_type == PT_INTERP);
    let Some(interpreter) = interpreters.next() else {
        return false;
    };
    if interpreters.next().is_some() {
        return false;
    }
    let start = interpreter.offset as usize;
    let Some(end) = start.checked_add(interpreter.file_size as usize) else {
        return false;
    };
    bytes.get(start..end) == Some(b"/system/ld-makos.so\0")
}

fn map_user_stack(root: u64) -> [u64; USER_STACK_PAGES] {
    let mut frames = [0u64; USER_STACK_PAGES];
    for page in 0..USER_STACK_PAGES {
        let stack =
            crate::mm::allocate_frame().unwrap_or_else(|| crate::fatal("user stack frame OOM"));
        unsafe { ptr::write_bytes(stack as *mut u8, 0, 4096) };
        frames[page] = stack;
        crate::arch::map_user_page_in(
            root,
            USER_STACK_BASE + page as u64 * 4096,
            stack,
            true,
            false,
        );
    }
    frames
}

fn load_pe(bytes: &[u8], root: u64) -> u64 {
    let pe = Pe64::parse(bytes).unwrap_or_else(|_| crate::fatal("PE32+ validation failed"));
    if pe.image_base() != USER_CODE_BASE
        || pe.image_size() as u64 > USER_CODE_LIMIT - USER_CODE_BASE
        || !(USER_CODE_BASE..USER_CODE_LIMIT).contains(&pe.entry())
    {
        crate::fatal("unsupported PE32+ image layout");
    }
    let mut loaded = 0usize;
    let mut executable_entry = false;
    for section in pe.sections() {
        if section.characteristics & IMAGE_SCN_MEM_READ == 0
            || section.virtual_address & 0xfff != 0
            || section.characteristics & (IMAGE_SCN_MEM_WRITE | IMAGE_SCN_MEM_EXECUTE)
                == IMAGE_SCN_MEM_WRITE | IMAGE_SCN_MEM_EXECUTE
        {
            crate::fatal("unsupported PE32+ section flags");
        }
        let virtual_address = USER_CODE_BASE + u64::from(section.virtual_address);
        let memory_size = section.virtual_size.max(section.raw_size) as usize;
        if memory_size == 0
            || virtual_address
                .checked_add(memory_size as u64)
                .is_none_or(|end| end > USER_CODE_LIMIT)
        {
            crate::fatal("PE32+ section exceeds user code range");
        }
        let writable = section.characteristics & IMAGE_SCN_MEM_WRITE != 0;
        let executable = section.characteristics & IMAGE_SCN_MEM_EXECUTE != 0;
        if executable
            && (virtual_address..virtual_address + memory_size as u64).contains(&pe.entry())
        {
            executable_entry = true;
        }
        let page_count = memory_size.div_ceil(4096);
        for page in 0..page_count {
            let frame =
                crate::mm::allocate_frame().unwrap_or_else(|| crate::fatal("PE32+ frame OOM"));
            unsafe { ptr::write_bytes(frame as *mut u8, 0, 4096) };
            let section_offset = page * 4096;
            let copy_count = (section.raw_size as usize)
                .saturating_sub(section_offset)
                .min(4096);
            if copy_count != 0 {
                unsafe {
                    ptr::copy_nonoverlapping(
                        bytes
                            .as_ptr()
                            .add(section.raw_offset as usize + section_offset),
                        frame as *mut u8,
                        copy_count,
                    );
                }
            }
            crate::arch::map_user_page_in(
                root,
                virtual_address + page as u64 * 4096,
                frame,
                writable,
                executable,
            );
        }
        loaded += 1;
    }
    if loaded == 0 || !executable_entry {
        crate::fatal("PE32+ executable entry absent");
    }
    let _ = map_user_stack(root);
    pe.entry()
}

pub fn spawn_worker() -> Option<u64> {
    let root = crate::arch::new_user_address_space();
    load_elf(WORKER_ELF, root);
    let stack_base = (&raw mut WORKER_RING0_STACK).cast::<u8>();
    let stack_top = stack_base as u64 + RING0_STACK_BYTES as u64;
    WORKER_EXIT.store(STILL_RUNNING, Ordering::Release);
    if !crate::scheduler::spawn_user(2, root, stack_base, stack_top, enter_worker) {
        crate::arch::destroy_user_address_space(root);
        return None;
    }
    crate::serial_println!("MAKOS_PROCESS_SPAWN parent=1 child=2 cr3={:#x}", root);
    Some(2)
}

pub fn spawn_linux_compat() -> Option<u64> {
    let root = crate::arch::new_user_address_space();
    load_elf(LINUX_COMPAT_ELF, root);
    let stack_base = (&raw mut LINUX_RING0_STACK).cast::<u8>();
    let stack_top = stack_base as u64 + RING0_STACK_BYTES as u64;
    LINUX_EXIT.store(STILL_RUNNING, Ordering::Release);
    if !crate::scheduler::spawn_user(3, root, stack_base, stack_top, enter_linux_compat) {
        crate::arch::destroy_user_address_space(root);
        return None;
    }
    crate::serial_println!(
        "MAKOS_COMPAT_SPAWN personality=linux-x86_64 pid=3 cr3={:#x}",
        root
    );
    Some(3)
}

pub fn spawn_windows_compat() -> Option<u64> {
    let root = crate::arch::new_user_address_space();
    let entry = load_pe(WINDOWS_COMPAT_PE, root);
    let stack_base = (&raw mut WINDOWS_RING0_STACK).cast::<u8>();
    let stack_top = stack_base as u64 + RING0_STACK_BYTES as u64;
    WINDOWS_EXIT.store(STILL_RUNNING, Ordering::Release);
    if !crate::scheduler::spawn_user(4, root, stack_base, stack_top, enter_windows_compat) {
        crate::arch::destroy_user_address_space(root);
        return None;
    }
    crate::serial_println!(
        "MAKOS_COMPAT_SPAWN personality=win32-x86_64 pid=4 loader=pe32+ entry={:#x} cr3={:#x}",
        entry,
        root,
    );
    Some(4)
}

pub fn spawn_service() -> Option<u64> {
    let root = crate::arch::new_user_address_space();
    load_elf(SERVICE_ELF, root);
    let stack_base = (&raw mut SERVICE_RING0_STACK).cast::<u8>();
    let stack_top = stack_base as u64 + RING0_STACK_BYTES as u64;
    let generation = SERVICE_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    SERVICE_EXIT.store(STILL_RUNNING, Ordering::Release);
    if !crate::scheduler::spawn_user(5, root, stack_base, stack_top, enter_service) {
        crate::arch::destroy_user_address_space(root);
        return None;
    }
    crate::serial_println!(
        "MAKOS_SERVICE_START unit=demo pid=5 generation={} cr3={:#x}",
        generation,
        root
    );
    Some(5)
}

pub fn spawn_toolchain() -> Option<u64> {
    let root = crate::arch::new_user_address_space();
    load_elf(TOOLCHAIN_ELF, root);
    let stack_base = (&raw mut TOOLCHAIN_RING0_STACK).cast::<u8>();
    let stack_top = stack_base as u64 + RING0_STACK_BYTES as u64;
    TOOLCHAIN_EXIT.store(STILL_RUNNING, Ordering::Release);
    if !crate::scheduler::spawn_user(6, root, stack_base, stack_top, enter_toolchain) {
        crate::arch::destroy_user_address_space(root);
        return None;
    }
    crate::serial_println!("MAKOS_TOOLCHAIN_SPAWN pid=6 format=elf64 cr3={:#x}", root);
    Some(6)
}

pub fn spawn_dynamic_app() -> Option<u64> {
    if !has_dynamic_interpreter(DYNAMIC_APP_ELF) {
        return None;
    }
    let root = crate::arch::new_user_address_space();
    let (loader_entry, _) = load_elf_with_stack(DYNAMIC_LINKER_ELF, root);
    let (application_entry, application_segments) =
        load_dynamic_elf(DYNAMIC_APP_ELF, root, DYNAMIC_APP_BASE, true);
    let (_, library_segments) =
        load_dynamic_elf(DYNAMIC_LIBRARY_ELF, root, DYNAMIC_LIBRARY_BASE, false);
    let stack_base = (&raw mut DYNAMIC_RING0_STACK).cast::<u8>();
    let stack_top = stack_base as u64 + RING0_STACK_BYTES as u64;
    DYNAMIC_APP_ENTRY.store(application_entry, Ordering::Release);
    DYNAMIC_EXIT.store(STILL_RUNNING, Ordering::Release);
    if !crate::scheduler::spawn_user(DYNAMIC_PID, root, stack_base, stack_top, enter_dynamic_app) {
        crate::arch::destroy_user_address_space(root);
        return None;
    }
    crate::serial_println!(
        "MAKOS_DYNAMIC_SPAWN pid={} loader=/system/ld-makos.so app=dynamic-app library=libmakosdemo.so interp=1 loader_entry={:#x} app_entry={:#x} app_base={:#x} library_base={:#x} segments={},{},{} cr3={:#x}",
        DYNAMIC_PID,
        loader_entry,
        application_entry,
        DYNAMIC_APP_BASE,
        DYNAMIC_LIBRARY_BASE,
        Elf64::parse(DYNAMIC_LINKER_ELF)
            .ok()?
            .program_headers()
            .filter(|header| header.segment_type == PT_LOAD)
            .count(),
        application_segments,
        library_segments,
        root,
    );
    Some(DYNAMIC_PID)
}

pub fn spawn_path(path: &[u8]) -> Option<u64> {
    let startup = default_spawn_arguments(path)?;
    spawn_path_inner(path, &startup)
}

pub fn spawn_path_with_arguments(path: &[u8], bytes: &[u8]) -> Option<u64> {
    let startup = parse_spawn_arguments(bytes)?;
    spawn_path_inner(path, &startup)
}

fn spawn_path_inner(path: &[u8], startup: &SpawnArguments) -> Option<u64> {
    let mut image = [0u8; crate::vfs::MAX_FILE_BYTES];
    let length = crate::vfs::snapshot(path, &mut image)?;
    let (entry, segments) = supported_elf_layout(&image[..length])?;
    let slot = (0..PATH_EXEC_SLOTS).find(|slot| {
        PATH_EXEC_USED[*slot]
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    })?;
    let pid = PATH_EXEC_FIRST_PID + slot as u64;
    let root = crate::arch::new_user_address_space();
    let (loaded_entry, stack_frames) = load_elf_with_stack(&image[..length], root);
    if loaded_entry != entry {
        crate::fatal("ELF entry changed while loading");
    }
    let Some(user_startup) = build_user_startup(&stack_frames, entry, startup) else {
        crate::arch::destroy_user_address_space(root);
        PATH_EXEC_USED[slot].store(false, Ordering::Release);
        return None;
    };
    let stack_base = unsafe {
        (&raw mut PATH_EXEC_RING0_STACKS)
            .cast::<Ring0Stack>()
            .add(slot)
            .cast::<u8>()
    };
    let stack_top = stack_base as u64 + RING0_STACK_BYTES as u64;
    PATH_EXEC_ENTRIES[slot].store(entry, Ordering::Release);
    PATH_EXEC_STACKS[slot].store(user_startup.stack_pointer, Ordering::Release);
    PATH_EXEC_ARGC[slot].store(user_startup.argc, Ordering::Release);
    PATH_EXEC_ARGV[slot].store(user_startup.argv, Ordering::Release);
    PATH_EXEC_ENVP[slot].store(user_startup.envp, Ordering::Release);
    PATH_EXEC_EXITS[slot].store(STILL_RUNNING, Ordering::Release);
    let enter = [enter_path_exec_0, enter_path_exec_1][slot];
    if !crate::scheduler::spawn_user(pid, root, stack_base, stack_top, enter) {
        crate::arch::destroy_user_address_space(root);
        PATH_EXEC_USED[slot].store(false, Ordering::Release);
        return None;
    }
    crate::serial_println!(
        "MAKOS_EXEC_SPAWN pid={} slot={} source=makfs format=elf64 bytes={} segments={} entry={:#x} cr3={:#x} startup=v1 argc={} envc={} stack={:#x}",
        pid,
        slot,
        length,
        segments,
        entry,
        root,
        startup.argc,
        startup.envc,
        user_startup.stack_pointer,
    );
    Some(pid)
}

fn default_spawn_arguments(path: &[u8]) -> Option<SpawnArguments> {
    if path.is_empty() || path.len() >= SPAWN_DATA_BYTES || path.contains(&0) {
        return None;
    }
    let mut startup = SpawnArguments::EMPTY;
    startup.argc = 1;
    startup.data_length = path.len() + 1;
    startup.data[..path.len()].copy_from_slice(path);
    Some(startup)
}

fn parse_spawn_arguments(bytes: &[u8]) -> Option<SpawnArguments> {
    if bytes.len() != SPAWN_ARGUMENTS_BYTES
        || read_u32(bytes, 0)? != SPAWN_ARGUMENTS_VERSION as usize
    {
        return None;
    }
    let argc = read_u32(bytes, 4)?;
    let envc = read_u32(bytes, 8)?;
    let data_length = read_u32(bytes, 12)?;
    if argc == 0
        || argc > SPAWN_MAX_ARGUMENTS
        || envc > SPAWN_MAX_ENVIRONMENT
        || data_length == 0
        || data_length > SPAWN_DATA_BYTES
    {
        return None;
    }
    let mut startup = SpawnArguments {
        argc,
        envc,
        data_length,
        ..SpawnArguments::EMPTY
    };
    for index in 0..SPAWN_MAX_ARGUMENTS {
        startup.argv_offsets[index] = read_u32(bytes, 16 + index * 4)?;
    }
    for index in 0..SPAWN_MAX_ENVIRONMENT {
        startup.env_offsets[index] = read_u32(bytes, 48 + index * 4)?;
    }
    startup
        .data
        .copy_from_slice(&bytes[80..SPAWN_ARGUMENTS_BYTES]);
    if startup.argv_offsets[argc..]
        .iter()
        .any(|offset| *offset != 0)
        || startup.env_offsets[envc..]
            .iter()
            .any(|offset| *offset != 0)
    {
        return None;
    }
    for offset in &startup.argv_offsets[..argc] {
        if startup_string(&startup, *offset)?.len() <= 1 {
            return None;
        }
    }
    for offset in &startup.env_offsets[..envc] {
        let value = startup_string(&startup, *offset)?;
        let equals = value[..value.len() - 1]
            .iter()
            .position(|byte| *byte == b'=')?;
        if equals == 0 {
            return None;
        }
    }
    Some(startup)
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<usize> {
    Some(u32::from_le_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?) as usize)
}

fn startup_string(startup: &SpawnArguments, offset: usize) -> Option<&[u8]> {
    let remaining = startup.data.get(offset..startup.data_length)?;
    let length = remaining.iter().position(|byte| *byte == 0)? + 1;
    Some(&remaining[..length])
}

fn build_user_startup(
    frames: &[u64; USER_STACK_PAGES],
    entry: u64,
    startup: &SpawnArguments,
) -> Option<UserStartup> {
    let mut cursor = USER_STACK_END;
    let mut argv = [0u64; SPAWN_MAX_ARGUMENTS];
    let mut envp = [0u64; SPAWN_MAX_ENVIRONMENT];
    for index in 0..startup.argc {
        let value = startup_string(startup, startup.argv_offsets[index])?;
        cursor = cursor.checked_sub(value.len() as u64)?;
        write_user_stack(frames, cursor, value)?;
        argv[index] = cursor;
    }
    for index in 0..startup.envc {
        let value = startup_string(startup, startup.env_offsets[index])?;
        cursor = cursor.checked_sub(value.len() as u64)?;
        write_user_stack(frames, cursor, value)?;
        envp[index] = cursor;
    }
    let auxv = [
        (6u64, 4096u64),
        (9, entry),
        (11, 996),
        (12, 996),
        (13, 996),
        (14, 996),
        (17, 100),
        (23, 0),
        (31, argv[0]),
        (0, 0),
    ];
    const MAX_WORDS: usize = 1 + SPAWN_MAX_ARGUMENTS + 1 + SPAWN_MAX_ENVIRONMENT + 1 + 10 * 2;
    let mut words = [0u64; MAX_WORDS];
    let mut count = 0usize;
    words[count] = startup.argc as u64;
    count += 1;
    let argv_address_offset = count * 8;
    for pointer in &argv[..startup.argc] {
        words[count] = *pointer;
        count += 1;
    }
    count += 1;
    let envp_address_offset = count * 8;
    for pointer in &envp[..startup.envc] {
        words[count] = *pointer;
        count += 1;
    }
    count += 1;
    for (kind, value) in auxv {
        words[count] = kind;
        words[count + 1] = value;
        count += 2;
    }
    cursor = cursor.checked_sub((count * 8) as u64)? & !15;
    if cursor < USER_STACK_BASE {
        return None;
    }
    for (index, word) in words[..count].iter().enumerate() {
        write_user_stack(frames, cursor + index as u64 * 8, &word.to_le_bytes())?;
    }
    Some(UserStartup {
        stack_pointer: cursor,
        argc: startup.argc as u64,
        argv: cursor + argv_address_offset as u64,
        envp: cursor + envp_address_offset as u64,
    })
}

fn write_user_stack(frames: &[u64; USER_STACK_PAGES], address: u64, bytes: &[u8]) -> Option<()> {
    let end = address.checked_add(bytes.len() as u64)?;
    if address < USER_STACK_BASE || end > USER_STACK_END {
        return None;
    }
    let mut source_offset = 0usize;
    let mut stack_offset = (address - USER_STACK_BASE) as usize;
    while source_offset < bytes.len() {
        let frame_index = stack_offset / 4096;
        let frame_offset = stack_offset % 4096;
        let count = (4096 - frame_offset).min(bytes.len() - source_offset);
        unsafe {
            ptr::copy_nonoverlapping(
                bytes.as_ptr().add(source_offset),
                (frames[frame_index] as *mut u8).add(frame_offset),
                count,
            );
        }
        source_offset += count;
        stack_offset += count;
    }
    Some(())
}

extern "C" fn enter_path_exec_0() -> ! {
    enter_path_exec(0)
}

extern "C" fn enter_path_exec_1() -> ! {
    enter_path_exec(1)
}

fn enter_path_exec(slot: usize) -> ! {
    crate::arch::enable_interrupts();
    crate::arch::enter_user_startup(
        PATH_EXEC_ENTRIES[slot].load(Ordering::Acquire),
        PATH_EXEC_STACKS[slot].load(Ordering::Acquire),
        PATH_EXEC_ARGC[slot].load(Ordering::Acquire),
        PATH_EXEC_ARGV[slot].load(Ordering::Acquire),
        PATH_EXEC_ENVP[slot].load(Ordering::Acquire),
    )
}

extern "C" fn enter_toolchain() -> ! {
    let elf = Elf64::parse(TOOLCHAIN_ELF).unwrap_or_else(|_| crate::fatal("toolchain ELF invalid"));
    crate::arch::enable_interrupts();
    crate::arch::enter_user(elf.entry(), USER_STACK_TOP, 0)
}

extern "C" fn enter_dynamic_app() -> ! {
    let loader = Elf64::parse(DYNAMIC_LINKER_ELF)
        .unwrap_or_else(|_| crate::fatal("dynamic loader ELF invalid"));
    crate::arch::enable_interrupts();
    crate::arch::enter_user_startup(
        loader.entry(),
        USER_STACK_TOP,
        DYNAMIC_APP_BASE,
        DYNAMIC_LIBRARY_BASE,
        DYNAMIC_APP_ENTRY.load(Ordering::Acquire),
    )
}

extern "C" fn enter_service() -> ! {
    let elf = Elf64::parse(SERVICE_ELF).unwrap_or_else(|_| crate::fatal("service ELF invalid"));
    crate::arch::enable_interrupts();
    crate::arch::enter_user(
        elf.entry(),
        USER_STACK_TOP,
        SERVICE_GENERATION.load(Ordering::Acquire),
    )
}

extern "C" fn enter_windows_compat() -> ! {
    let pe = Pe64::parse(WINDOWS_COMPAT_PE)
        .unwrap_or_else(|_| crate::fatal("Windows personality PE32+ invalid"));
    crate::arch::enable_interrupts();
    crate::arch::enter_user(pe.entry(), USER_STACK_TOP, 0)
}

extern "C" fn enter_linux_compat() -> ! {
    let elf = Elf64::parse(LINUX_COMPAT_ELF)
        .unwrap_or_else(|_| crate::fatal("Linux personality ELF invalid"));
    crate::arch::enable_interrupts();
    crate::arch::enter_user(elf.entry(), USER_STACK_TOP, 0)
}

extern "C" fn enter_worker() -> ! {
    let elf = Elf64::parse(WORKER_ELF).unwrap_or_else(|_| crate::fatal("worker ELF invalid"));
    crate::arch::enable_interrupts();
    crate::arch::enter_user(elf.entry(), USER_STACK_TOP, 0)
}

pub fn exit_current(code: u64) -> ! {
    let pid = crate::scheduler::current_pid();
    let tid = crate::scheduler::current_tid();
    if tid != pid {
        crate::fatal("thread used process-exit syscall");
    }
    crate::serial_println!("MAKOS_PROCESS_EXIT pid={} code={}", pid, code);
    if pid == 2 {
        WORKER_EXIT.store(code, Ordering::Release);
    } else if pid == 3 {
        LINUX_EXIT.store(code, Ordering::Release);
    } else if pid == 4 {
        WINDOWS_EXIT.store(code, Ordering::Release);
    } else if pid == 5 {
        SERVICE_EXIT.store(code, Ordering::Release);
    } else if pid == 6 {
        TOOLCHAIN_EXIT.store(code, Ordering::Release);
    } else if pid == DYNAMIC_PID {
        DYNAMIC_EXIT.store(code, Ordering::Release);
    } else if (PATH_EXEC_FIRST_PID..PATH_EXEC_FIRST_PID + PATH_EXEC_SLOTS as u64).contains(&pid) {
        PATH_EXEC_EXITS[(pid - PATH_EXEC_FIRST_PID) as usize].store(code, Ordering::Release);
    }
    let closed_ipc_handles = crate::ipc::close_all(pid);
    crate::serial_println!(
        "MAKOS_PROCESS_EXIT_IPC_CLEANUP pid={} handles={}",
        pid,
        closed_ipc_handles,
    );
    crate::scheduler::exit_current()
}

pub fn fault_current(vector: u64, error: u64, rip: u64, address: u64) -> ! {
    let pid = crate::scheduler::current_pid();
    if pid <= 1 {
        crate::fatal("essential process faulted");
    }
    crate::serial_println!(
        "MAKOS_PROCESS_FAULT pid={} vector={} error={:#x} rip={:#x} address={:#x} contained=1",
        pid,
        vector,
        error,
        rip,
        address
    );
    exit_current(128 + vector)
}

pub fn thread_create(entry: u64, argument: u64) -> Option<u64> {
    if crate::scheduler::current_pid() != 2
        || !(USER_CODE_BASE..USER_CODE_LIMIT).contains(&entry)
        || (argument != 0 && !valid_user_range(argument, 1))
        || THREAD_STACK_FRAME.load(Ordering::Acquire) != 0
    {
        return None;
    }
    let frame = crate::mm::allocate_frame()?;
    unsafe { ptr::write_bytes(frame as *mut u8, 0, 4096) };
    let root = current_address_space();
    crate::arch::map_user_page_in(root, USER_THREAD_STACK_BASE, frame, true, false);
    let stack_base = (&raw mut THREAD_RING0_STACK).cast::<u8>();
    let stack_top = stack_base as u64 + RING0_STACK_BYTES as u64;
    THREAD_STACK_FRAME.store(frame, Ordering::Release);
    THREAD_ENTRY.store(entry, Ordering::Release);
    THREAD_ARGUMENT.store(argument, Ordering::Release);
    THREAD_EXIT.store(STILL_RUNNING, Ordering::Release);
    if !crate::scheduler::spawn_user_thread(2, 3, root, stack_base, stack_top, enter_thread) {
        THREAD_STACK_FRAME.store(0, Ordering::Release);
        let _ = crate::arch::unmap_user_page(USER_THREAD_STACK_BASE);
        let _ = crate::mm::free_frame(frame);
        return None;
    }
    Some(3)
}

extern "C" fn enter_thread() -> ! {
    let entry = THREAD_ENTRY.load(Ordering::Acquire);
    let argument = THREAD_ARGUMENT.load(Ordering::Acquire);
    crate::arch::enable_interrupts();
    crate::arch::enter_user(entry, USER_THREAD_STACK_TOP, argument)
}

pub fn thread_exit(code: u64) -> ! {
    if crate::scheduler::current_tid() != 3 {
        crate::fatal("thread-exit called outside worker thread");
    }
    THREAD_EXIT.store(code, Ordering::Release);
    crate::scheduler::exit_current()
}

pub fn thread_join(tid: u64) -> Option<u64> {
    if crate::scheduler::current_pid() != 2 || tid != 3 {
        return None;
    }
    let code = THREAD_EXIT.load(Ordering::Acquire);
    if code == STILL_RUNNING {
        return None;
    }
    if !crate::scheduler::reap_thread(2, tid) {
        crate::fatal("thread scheduler-state reclaim failed");
    }
    let frame = THREAD_STACK_FRAME.swap(0, Ordering::AcqRel);
    if frame != 0 {
        if crate::arch::unmap_user_page(USER_THREAD_STACK_BASE) != Some(frame)
            || crate::mm::free_frame(frame).is_err()
        {
            crate::fatal("thread stack reclaim failed");
        }
    }
    Some(code)
}

pub fn wait(pid: u64) -> Option<u64> {
    let exit = match pid {
        2 => &WORKER_EXIT,
        3 => &LINUX_EXIT,
        4 => &WINDOWS_EXIT,
        5 => &SERVICE_EXIT,
        6 => &TOOLCHAIN_EXIT,
        DYNAMIC_PID => &DYNAMIC_EXIT,
        PATH_EXEC_FIRST_PID..=8 => &PATH_EXEC_EXITS[(pid - PATH_EXEC_FIRST_PID) as usize],
        _ => return None,
    };
    let code = exit.load(Ordering::Acquire);
    if code == STILL_RUNNING {
        return None;
    }
    let root = crate::scheduler::reap(pid)?;
    let handles = crate::ipc::close_all(pid);
    let sockets = crate::socket::close_all(pid);
    let files = crate::vfs::close_all(pid);
    let surfaces = crate::graphics::close_all(pid);
    let (vm_regions, vm_pages) = crate::vm::forget_all(pid);
    let frames = crate::arch::destroy_user_address_space(root);
    exit.store(STILL_RUNNING, Ordering::Release);
    if (PATH_EXEC_FIRST_PID..PATH_EXEC_FIRST_PID + PATH_EXEC_SLOTS as u64).contains(&pid) {
        PATH_EXEC_USED[(pid - PATH_EXEC_FIRST_PID) as usize].store(false, Ordering::Release);
    }
    crate::serial_println!(
        "MAKOS_PROCESS_REAP pid={} frames={} handles={} sockets={} files={} surfaces={} vm_regions={} vm_pages={}",
        pid,
        frames,
        handles,
        sockets,
        files,
        surfaces,
        vm_regions,
        vm_pages
    );
    if vm_regions != 0 {
        crate::serial_println!(
            "MAKOS_VM_REAP_OK pid={} regions={} pages={} address_space_destroy=1",
            pid,
            vm_regions,
            vm_pages
        );
    }
    Some(code)
}

pub fn vm_map() -> Option<u64> {
    crate::vm::map(
        crate::scheduler::current_pid(),
        4096,
        crate::vm::PROT_READ | crate::vm::PROT_WRITE,
    )
}

pub fn vm_unmap(address: u64) -> bool {
    crate::vm::unmap(crate::scheduler::current_pid(), address, 4096)
}

pub fn vm_protect(address: u64, protection: u64) -> bool {
    crate::vm::protect(crate::scheduler::current_pid(), address, 4096, protection)
}

pub fn vm_map_range(length: usize, protection: u64) -> Option<u64> {
    crate::vm::map(crate::scheduler::current_pid(), length, protection)
}

pub fn vm_unmap_range(address: u64, length: usize) -> bool {
    crate::vm::unmap(crate::scheduler::current_pid(), address, length)
}

pub fn vm_protect_range(address: u64, length: usize, protection: u64) -> bool {
    crate::vm::protect(crate::scheduler::current_pid(), address, length, protection)
}

fn current_address_space() -> u64 {
    let pid = crate::scheduler::current_pid();
    crate::scheduler::address_space(pid).unwrap_or_else(|| crate::fatal("process CR3 absent"))
}

pub fn valid_user_range(address: u64, length: usize) -> bool {
    valid_user_range_access(address, length, false)
}

pub fn valid_user_range_write(address: u64, length: usize) -> bool {
    valid_user_range_access(address, length, true)
}

fn valid_user_range_access(address: u64, length: usize, write: bool) -> bool {
    let Some(last) = address.checked_add(length.saturating_sub(1) as u64) else {
        return false;
    };
    let mut page = address & !0xfffu64;
    let last_page = last & !0xfffu64;
    loop {
        let Some(writable) = crate::arch::user_page_writable(page) else {
            return false;
        };
        if write && !writable {
            return false;
        }
        if page == last_page {
            return true;
        }
        let Some(next) = page.checked_add(4096) else {
            return false;
        };
        page = next;
    }
}
