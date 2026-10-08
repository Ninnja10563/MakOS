//! Host adapters around the complete, unmodified production AArch64 VM module.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Barrier, Mutex};
use std::time::Duration;

const PID: u64 = 4;
const ROOT: u64 = 0x4000_1000;
const OTHER_ROOT: u64 = 0x4000_2000;
const PAGE: u64 = 0x8000_0000;
const RW: u64 = 3;
const RX: u64 = 5;

struct LockedOperationPause {
    entered: Sender<()>,
    resume: Mutex<Receiver<()>>,
}
static ALLOCATION_PAUSE: Mutex<Option<Arc<LockedOperationPause>>> = Mutex::new(None);
static PROTECT_PAUSE: Mutex<Option<Arc<LockedOperationPause>>> = Mutex::new(None);

fn pause_locked_operation(slot: &Mutex<Option<Arc<LockedOperationPause>>>) {
    let pause = slot.lock().unwrap().take();
    if let Some(pause) = pause {
        assert!(
            aarch64_vm::lock_held() && arch::interrupts_masked(),
            "VM mutation must keep metadata and PTE transaction locked"
        );
        pause.entered.send(()).unwrap();
        pause
            .resume
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
    }
}

fn arm_locked_pause(slot: &Mutex<Option<Arc<LockedOperationPause>>>) -> (Receiver<()>, Sender<()>) {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    *slot.lock().unwrap() = Some(Arc::new(LockedOperationPause {
        entered: entered_tx,
        resume: Mutex::new(resume_rx),
    }));
    (entered_rx, resume_tx)
}

#[macro_export]
macro_rules! serial_println {
    ($($argument:tt)*) => { let _ = format_args!($($argument)*); };
}

fn fatal(message: &str) -> ! {
    panic!("kernel fatal: {message}");
}

mod aarch64_vm {
    include!("vm_production.rs");

    pub fn lock_held() -> bool {
        VM.lock.load(Ordering::Acquire)
    }
}

mod aarch64_process {
    #[derive(PartialEq)]
    pub enum ProcessRole {
        Firefox,
        Test,
    }
    pub fn current_app_role() -> ProcessRole {
        ProcessRole::Test
    }
}

mod vfs {
    #[derive(Clone, Copy)]
    pub struct MountedPackageFile {
        pub path: [u8; 1],
        pub path_length: usize,
        pub identity: u64,
    }

    #[derive(Clone, Copy)]
    pub enum ReadOnlyFileBacking {
        Embedded(&'static [u8]),
        Package(MountedPackageFile),
    }

    #[derive(Clone, Copy)]
    pub struct SharedMemoryBacking {
        pub object: u16,
        pub size: u64,
    }

    pub fn read_only_backing_for_fd(fd: u64) -> Option<ReadOnlyFileBacking> {
        (fd == 7 || fd == 8).then_some(ReadOnlyFileBacking::Package(MountedPackageFile {
            path: [b'x'],
            path_length: 1,
            identity: fd,
        }))
    }

    pub fn shared_memory_backing_for_fd(fd: u64, _write: bool) -> Option<SharedMemoryBacking> {
        (fd == 9).then_some(SharedMemoryBacking {
            object: 1,
            size: 4096,
        })
    }
}

mod mm {
    use super::*;

    #[repr(align(4096))]
    struct Frame([u8; 4096]);
    static FRAMES: Mutex<BTreeMap<u64, Box<Frame>>> = Mutex::new(BTreeMap::new());
    static FREED: Mutex<Vec<u64>> = Mutex::new(Vec::new());
    pub static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
    pub static ALLOCATION_FAILURES: AtomicUsize = AtomicUsize::new(0);

    pub fn allocate_frame() -> Option<u64> {
        pause_locked_operation(&ALLOCATION_PAUSE);
        if ALLOCATION_FAILURES
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                count.checked_sub(1)
            })
            .is_ok()
        {
            return None;
        }
        let frame = Box::new(Frame([0xa5; 4096]));
        let address = frame.0.as_ptr() as u64;
        assert_eq!(address & 4095, 0);
        assert!(FRAMES.lock().unwrap().insert(address, frame).is_none());
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        Some(address)
    }

    pub fn free_frame(frame: u64) -> Result<(), ()> {
        let removed = FRAMES.lock().unwrap().remove(&frame);
        assert!(removed.is_some(), "freed unowned or already-freed frame");
        FREED.lock().unwrap().push(frame);
        Ok(())
    }

    pub fn free_frames() -> usize {
        8192 - FRAMES.lock().unwrap().len()
    }
    pub fn live_count() -> usize {
        FRAMES.lock().unwrap().len()
    }
    pub fn freed_count() -> usize {
        FREED.lock().unwrap().len()
    }
    pub fn was_freed(frame: u64) -> bool {
        FREED.lock().unwrap().contains(&frame)
    }
    pub fn bytes(frame: u64) -> Vec<u8> {
        FRAMES.lock().unwrap()[&frame].0.to_vec()
    }
    pub fn reset() {
        FRAMES.lock().unwrap().clear();
        FREED.lock().unwrap().clear();
        ALLOCATIONS.store(0, Ordering::SeqCst);
        ALLOCATION_FAILURES.store(0, Ordering::SeqCst);
    }
}

mod arch {
    use super::*;

    thread_local! {
        static IRQ_MASK_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        static MASK_NOTICE: std::cell::RefCell<Option<Sender<()>>> = const { std::cell::RefCell::new(None) };
    }

    pub struct LocalInterruptMask;
    impl LocalInterruptMask {
        pub fn acquire() -> Self {
            IRQ_MASK_DEPTH.set(IRQ_MASK_DEPTH.get() + 1);
            MASK_NOTICE.with_borrow_mut(|notice| {
                if let Some(notice) = notice.take() {
                    notice.send(()).unwrap();
                }
            });
            Self
        }
    }
    impl Drop for LocalInterruptMask {
        fn drop(&mut self) {
            IRQ_MASK_DEPTH.set(IRQ_MASK_DEPTH.get().checked_sub(1).unwrap());
        }
    }
    pub fn interrupts_masked() -> bool {
        IRQ_MASK_DEPTH.get() != 0
    }
    pub fn notify_next_mask_acquire(notice: Sender<()>) {
        MASK_NOTICE.with_borrow_mut(|slot| *slot = Some(notice));
    }

    pub const USER_ADDRESS_BASE: u64 = 0x1000_0000;
    pub const USER_HEAP_BASE: u64 = 0x1400_0000;
    pub const USER_HEAP_LIMIT: u64 = 0x1800_0000;
    pub const USER_MMAP_BASE: u64 = 0x8000_0000;
    pub const USER_MMAP_LIMIT: u64 = 0x3_c000_0000;
    pub const USER_STACK_TOP: u64 = 0x4_0000_0000;

    #[derive(Clone, Copy, Debug)]
    pub struct Mapping {
        pub frame: u64,
        pub read: bool,
        pub write: bool,
        pub execute: bool,
    }
    static PAGES: Mutex<BTreeMap<(u64, u64), Mapping>> = Mutex::new(BTreeMap::new());
    pub static INSTALLS: AtomicUsize = AtomicUsize::new(0);
    pub static CODE_SYNCS: AtomicUsize = AtomicUsize::new(0);

    pub fn user_page_physical_in(root: u64, page: u64) -> Option<u64> {
        PAGES
            .lock()
            .unwrap()
            .get(&(root, page))
            .map(|mapping| mapping.frame)
    }
    pub fn mapping(root: u64, page: u64) -> Option<Mapping> {
        PAGES.lock().unwrap().get(&(root, page)).copied()
    }
    pub fn user_page_access_permitted_in(root: u64, page: u64, write: bool, execute: bool) -> bool {
        mapping(root, page).is_some_and(|resident| {
            resident.read && (!write || resident.write) && (!execute || resident.execute)
        })
    }
    pub fn map_user_page_permissions_if_absent_in(
        root: u64,
        page: u64,
        frame: u64,
        read: bool,
        write: bool,
        execute: bool,
    ) -> Result<(), u64> {
        assert!(aarch64_vm::lock_held(), "fault commit must hold VM lock");
        assert!(
            interrupts_masked(),
            "VM critical section must mask local interrupt reentry"
        );
        let mut pages = PAGES.lock().unwrap();
        if let Some(existing) = pages.get(&(root, page)) {
            return Err(existing.frame);
        }
        pages.insert(
            (root, page),
            Mapping {
                frame,
                read,
                write,
                execute,
            },
        );
        INSTALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    pub fn map_user_page_permissions_in(
        root: u64,
        page: u64,
        frame: u64,
        read: bool,
        write: bool,
        execute: bool,
    ) {
        let mut pages = PAGES.lock().unwrap();
        assert!(
            !pages.contains_key(&(root, page)),
            "duplicate AArch64 user-page mapping"
        );
        pages.insert(
            (root, page),
            Mapping {
                frame,
                read,
                write,
                execute,
            },
        );
        INSTALLS.fetch_add(1, Ordering::SeqCst);
    }
    pub fn map_user_page_in(root: u64, page: u64, frame: u64, write: bool, execute: bool) {
        map_user_page_permissions_in(root, page, frame, true, write, execute);
    }
    pub fn unmap_user_page_in(root: u64, page: u64) -> Option<u64> {
        PAGES
            .lock()
            .unwrap()
            .remove(&(root, page))
            .map(|mapping| mapping.frame)
    }
    pub fn protect_user_page_permissions_in(
        root: u64,
        page: u64,
        read: bool,
        write: bool,
        execute: bool,
    ) -> bool {
        pause_locked_operation(&PROTECT_PAUSE);
        let mut pages = PAGES.lock().unwrap();
        let Some(mapping) = pages.get_mut(&(root, page)) else {
            return false;
        };
        *mapping = Mapping {
            frame: mapping.frame,
            read,
            write,
            execute,
        };
        true
    }
    pub fn sync_user_code(_frame: u64) {
        CODE_SYNCS.fetch_add(1, Ordering::SeqCst);
    }
    pub fn reset() {
        PAGES.lock().unwrap().clear();
        INSTALLS.store(0, Ordering::SeqCst);
        CODE_SYNCS.store(0, Ordering::SeqCst);
    }
}

#[derive(Clone)]
enum ReadAction {
    None,
    Barrier(Arc<Barrier>),
    Unmap,
    ProtectNone,
    ProtectAba,
    UnrelatedProtect,
    Discard,
    ReplaceAnonymous,
    ReplaceFile,
    ReplaceRoot,
    InstallResident(bool),
    InstallResidentThenFail(bool),
    UnrelatedMapping,
    Fail,
}
static READ_ACTION: Mutex<ReadAction> = Mutex::new(ReadAction::None);
static READS: AtomicUsize = AtomicUsize::new(0);

mod fs {
    use super::*;
    pub fn read_package_file(
        file: &vfs::MountedPackageFile,
        offset: u64,
        output: &mut [u8],
    ) -> Option<usize> {
        // Another CPU may hold the global lock briefly. The calling thread's
        // mask is the reliable ownership observation: production with_state
        // always acquires this RAII mask before taking the VM lock.
        assert!(
            !arch::interrupts_masked(),
            "VM interrupt mask/lock must not span package/block I/O (CPU0 owner must make progress)"
        );
        READS.fetch_add(1, Ordering::SeqCst);
        let action = {
            let mut slot = READ_ACTION.lock().unwrap();
            let action = slot.clone();
            if !matches!(action, ReadAction::Barrier(_)) {
                *slot = ReadAction::None;
            }
            action
        };
        match action {
            ReadAction::None => {}
            ReadAction::Barrier(barrier) => {
                barrier.wait();
            }
            ReadAction::Unmap => assert!(aarch64_vm::unmap(PID, PAGE, 4096)),
            ReadAction::ProtectNone => assert!(aarch64_vm::protect(PID, PAGE, 4096, 0)),
            ReadAction::ProtectAba => {
                assert!(aarch64_vm::protect(PID, PAGE, 4096, 0));
                assert!(aarch64_vm::protect(PID, PAGE, 4096, RW));
            }
            ReadAction::UnrelatedProtect => assert!(aarch64_vm::protect(PID, PAGE + 4096, 4096, 1)),
            ReadAction::Discard => assert!(aarch64_vm::advise(PID, PAGE, 4096, 4)),
            ReadAction::ReplaceAnonymous => assert_eq!(
                aarch64_vm::map_anonymous_fixed(PID, PAGE, 4096, RW),
                Some(PAGE)
            ),
            ReadAction::ReplaceFile => assert_eq!(
                aarch64_vm::map_file(PID, PAGE, 4096, RW, true, 8, 0),
                Some(PAGE)
            ),
            ReadAction::ReplaceRoot => {
                assert!(
                    aarch64_vm::replace_process(PID, OTHER_ROOT, arch::USER_HEAP_BASE).is_some()
                );
                assert_eq!(
                    aarch64_vm::map_file(PID, PAGE, 4096, RW, true, 8, 0),
                    Some(PAGE)
                );
            }
            ReadAction::InstallResident(writable) => {
                let frame = mm::allocate_frame().unwrap();
                arch::map_user_page_permissions_in(ROOT, PAGE, frame, true, writable, false);
            }
            ReadAction::InstallResidentThenFail(writable) => {
                let frame = mm::allocate_frame().unwrap();
                arch::map_user_page_permissions_in(ROOT, PAGE, frame, true, writable, false);
                return None;
            }
            ReadAction::UnrelatedMapping => {
                assert!(aarch64_vm::map(PID, 4096, RW).is_some());
            }
            ReadAction::Fail => return None,
        }
        output.fill((file.identity + offset) as u8);
        Some(output.len())
    }
}

mod aarch64_shmem {
    use super::*;
    static FRAME: Mutex<Option<u64>> = Mutex::new(None);
    static REFERENCES: AtomicUsize = AtomicUsize::new(0);
    pub static LOOKUPS: AtomicUsize = AtomicUsize::new(0);
    pub fn page_frame(object: u16, offset: u64) -> Option<u64> {
        assert_eq!((object, offset), (1, 0));
        LOOKUPS.fetch_add(1, Ordering::SeqCst);
        let mut frame = FRAME.lock().unwrap();
        Some(*frame.get_or_insert_with(|| mm::allocate_frame().unwrap()))
    }
    pub fn retain_mapping(object: u16) -> bool {
        assert_eq!(object, 1);
        REFERENCES.fetch_add(1, Ordering::SeqCst);
        true
    }
    pub fn release_mapping(object: u16) {
        assert_eq!(object, 1);
        assert_ne!(REFERENCES.fetch_sub(1, Ordering::SeqCst), 0);
    }
    pub fn reset() {
        *FRAME.lock().unwrap() = None;
        REFERENCES.store(0, Ordering::SeqCst);
        LOOKUPS.store(0, Ordering::SeqCst);
    }
}

fn reset() {
    assert!(!aarch64_vm::lock_held());
    aarch64_vm::initialize();
    arch::reset();
    mm::reset();
    aarch64_shmem::reset();
    *READ_ACTION.lock().unwrap() = ReadAction::None;
    READS.store(0, Ordering::SeqCst);
    *ALLOCATION_PAUSE.lock().unwrap() = None;
    *PROTECT_PAUSE.lock().unwrap() = None;
    assert!(aarch64_vm::attach_process(PID, ROOT, arch::USER_HEAP_BASE));
}

fn file_mapping(protection: u64) {
    assert_eq!(
        aarch64_vm::map_file(PID, PAGE, 4096, protection, true, 7, 0),
        Some(PAGE)
    );
}

#[test]
fn simultaneous_same_page_faults_commit_one_private_frame() {
    reset();
    file_mapping(RW);
    *READ_ACTION.lock().unwrap() = ReadAction::Barrier(Arc::new(Barrier::new(2)));
    let first = std::thread::spawn(|| aarch64_vm::handle_page_fault(PID, PAGE + 3, false, false));
    let second =
        std::thread::spawn(|| aarch64_vm::handle_page_fault(PID, PAGE + 4095, true, false));
    assert!(
        first.join().unwrap(),
        "first authorized same-page fault must resolve"
    );
    assert!(
        second.join().unwrap(),
        "second authorized same-page fault must resolve"
    );
    assert_eq!(READS.load(Ordering::SeqCst), 2);
    assert_eq!(
        arch::INSTALLS.load(Ordering::SeqCst),
        1,
        "only winner may install a PTE"
    );
    assert_eq!(mm::ALLOCATIONS.load(Ordering::SeqCst), 2);
    assert_eq!(
        mm::freed_count(),
        1,
        "losing private frame must be reclaimed"
    );
    assert_eq!(mm::live_count(), 1);
    let resident = arch::mapping(ROOT, PAGE).unwrap();
    assert!(resident.read && resident.write && !resident.execute);
    assert_eq!(mm::bytes(resident.frame), vec![7; 4096]);
}

#[test]
fn existing_authorized_translation_resolves_without_allocating_again() {
    reset();
    file_mapping(RW);
    assert!(aarch64_vm::handle_page_fault(PID, PAGE, true, false));
    let before = mm::ALLOCATIONS.load(Ordering::SeqCst);
    assert!(
        aarch64_vm::handle_page_fault(PID, PAGE, false, false),
        "winner's PTE must resolve stale translation fault"
    );
    assert!(aarch64_vm::handle_page_fault(PID, PAGE, true, false));
    assert_eq!(mm::ALLOCATIONS.load(Ordering::SeqCst), before);
    assert_eq!(arch::INSTALLS.load(Ordering::SeqCst), 1);
}

#[test]
fn denied_access_never_allocates_or_performs_io() {
    for (protection, write, execute) in [
        (1, true, false),
        (RW, false, true),
        (RX, true, false),
        (0, false, false),
    ] {
        reset();
        file_mapping(protection);
        assert!(!aarch64_vm::handle_page_fault(PID, PAGE, write, execute));
        assert_eq!(mm::ALLOCATIONS.load(Ordering::SeqCst), 0);
        assert_eq!(READS.load(Ordering::SeqCst), 0);
        assert!(arch::mapping(ROOT, PAGE).is_none());
    }
}

#[test]
fn incompatible_resident_permissions_do_not_count_as_resolved() {
    for (read, write, execute, request_write, request_execute) in [
        (false, false, false, false, false),
        (true, false, false, true, false),
        (true, true, false, false, true),
    ] {
        reset();
        file_mapping(if request_execute { RX } else { RW });
        let frame = mm::allocate_frame().unwrap();
        arch::map_user_page_permissions_in(ROOT, PAGE, frame, read, write, execute);
        assert!(
            !aarch64_vm::handle_page_fault(PID, PAGE, request_write, request_execute),
            "incompatible resident mapping must remain a protection fault"
        );
        assert_eq!(READS.load(Ordering::SeqCst), 0);
        assert_eq!(arch::INSTALLS.load(Ordering::SeqCst), 1);
        assert!(!mm::was_freed(frame));
    }
}

#[test]
fn unmapped_during_io_does_not_install_or_leak_stale_frame() {
    reset();
    file_mapping(RW);
    *READ_ACTION.lock().unwrap() = ReadAction::Unmap;
    assert!(!aarch64_vm::handle_page_fault(PID, PAGE, false, false));
    assert!(arch::mapping(ROOT, PAGE).is_none());
    assert_eq!(mm::freed_count(), 1);
    assert_eq!(mm::live_count(), 0);
}

#[test]
fn protection_revoked_during_io_does_not_install_stale_permissions() {
    reset();
    file_mapping(RW);
    *READ_ACTION.lock().unwrap() = ReadAction::ProtectNone;
    assert!(!aarch64_vm::handle_page_fault(PID, PAGE, true, false));
    assert!(arch::mapping(ROOT, PAGE).is_none());
    assert_eq!(mm::freed_count(), 1);
    assert_eq!(mm::live_count(), 0);
}

#[test]
fn allocation_failure_leaves_no_translation() {
    reset();
    file_mapping(RW);
    mm::ALLOCATION_FAILURES.store(1, Ordering::SeqCst);
    assert!(!aarch64_vm::handle_page_fault(PID, PAGE, false, false));
    assert!(arch::mapping(ROOT, PAGE).is_none());
    assert_eq!(READS.load(Ordering::SeqCst), 0);
    assert_eq!(mm::live_count(), 0);
}

#[test]
fn package_io_failure_reclaims_private_frame() {
    reset();
    file_mapping(RW);
    *READ_ACTION.lock().unwrap() = ReadAction::Fail;
    assert!(!aarch64_vm::handle_page_fault(PID, PAGE, false, false));
    assert!(arch::mapping(ROOT, PAGE).is_none());
    assert_eq!(READS.load(Ordering::SeqCst), 1);
    assert_eq!(mm::freed_count(), 1);
    assert_eq!(mm::live_count(), 0);
}

#[test]
fn remap_to_anonymous_retries_instead_of_committing_old_file_bytes() {
    reset();
    file_mapping(RW);
    *READ_ACTION.lock().unwrap() = ReadAction::ReplaceAnonymous;
    assert!(aarch64_vm::handle_page_fault(PID, PAGE, true, false));
    let frame = arch::mapping(ROOT, PAGE).unwrap().frame;
    assert_eq!(
        mm::bytes(frame),
        vec![0; 4096],
        "stale file bytes must never reach new anonymous VMA"
    );
    assert_eq!(READS.load(Ordering::SeqCst), 1);
    assert_eq!(mm::ALLOCATIONS.load(Ordering::SeqCst), 2);
    assert_eq!(mm::freed_count(), 1);
}

#[test]
fn same_range_same_permissions_file_remap_requires_new_backing() {
    reset();
    file_mapping(RW);
    *READ_ACTION.lock().unwrap() = ReadAction::ReplaceFile;
    assert!(aarch64_vm::handle_page_fault(PID, PAGE, true, false));
    let frame = arch::mapping(ROOT, PAGE).unwrap().frame;
    assert_eq!(
        mm::bytes(frame),
        vec![8; 4096],
        "VMA ABA must not admit original file frame"
    );
    assert_eq!(READS.load(Ordering::SeqCst), 2);
    assert_eq!(mm::freed_count(), 1);
}

#[test]
fn process_root_replacement_never_installs_into_retired_root() {
    reset();
    file_mapping(RW);
    *READ_ACTION.lock().unwrap() = ReadAction::ReplaceRoot;
    assert!(aarch64_vm::handle_page_fault(PID, PAGE, true, false));
    assert!(arch::mapping(ROOT, PAGE).is_none());
    let frame = arch::mapping(OTHER_ROOT, PAGE).unwrap().frame;
    assert_eq!(mm::bytes(frame), vec![8; 4096]);
    assert_eq!(mm::freed_count(), 1);
}

#[test]
fn protection_aba_invalidates_in_flight_fault_generation() {
    reset();
    file_mapping(RW);
    *READ_ACTION.lock().unwrap() = ReadAction::ProtectAba;
    assert!(aarch64_vm::handle_page_fault(PID, PAGE, true, false));
    assert_eq!(
        READS.load(Ordering::SeqCst),
        2,
        "matching final protection cannot hide destructive VMA mutation"
    );
    assert_eq!(mm::freed_count(), 1);
}

#[test]
fn discard_invalidates_in_flight_private_file_population() {
    reset();
    file_mapping(RW);
    *READ_ACTION.lock().unwrap() = ReadAction::Discard;
    assert!(aarch64_vm::handle_page_fault(PID, PAGE, true, false));
    assert_eq!(
        READS.load(Ordering::SeqCst),
        2,
        "discard must invalidate unpublished pages as well as resident ones"
    );
    assert_eq!(mm::freed_count(), 1);
    assert_eq!(mm::live_count(), 1);
}

#[test]
fn unrelated_destructive_mutation_retries_without_rejecting_valid_fault() {
    reset();
    file_mapping(RW);
    assert_eq!(aarch64_vm::map(PID, 4096, RW), Some(PAGE + 4096));
    *READ_ACTION.lock().unwrap() = ReadAction::UnrelatedProtect;
    assert!(aarch64_vm::handle_page_fault(PID, PAGE, true, false));
    assert_eq!(READS.load(Ordering::SeqCst), 2);
    assert_eq!(mm::freed_count(), 1);
    assert_eq!(
        mm::bytes(arch::mapping(ROOT, PAGE).unwrap().frame),
        vec![7; 4096]
    );
}

#[test]
fn unrelated_new_mapping_does_not_invalidate_existing_fault() {
    reset();
    file_mapping(RW);
    *READ_ACTION.lock().unwrap() = ReadAction::UnrelatedMapping;
    assert!(aarch64_vm::handle_page_fault(PID, PAGE, true, false));
    assert_eq!(READS.load(Ordering::SeqCst), 1);
    assert_eq!(mm::freed_count(), 0);
}

#[test]
fn shared_resident_frame_is_never_private_freed() {
    reset();
    assert_eq!(
        aarch64_vm::map_shared(PID, PAGE, 4096, RW, true, 9, 0),
        Some(PAGE)
    );
    assert!(aarch64_vm::handle_page_fault(PID, PAGE, true, false));
    let frame = arch::mapping(ROOT, PAGE).unwrap().frame;
    assert!(aarch64_vm::handle_page_fault(PID, PAGE, false, false));
    assert_eq!(mm::freed_count(), 0);
    assert!(aarch64_vm::unmap(PID, PAGE, 4096));
    assert!(!mm::was_freed(frame));
}

#[test]
fn concurrent_shared_faults_retain_one_object_owned_frame() {
    reset();
    assert_eq!(
        aarch64_vm::map_shared(PID, PAGE, 4096, RW, true, 9, 0),
        Some(PAGE)
    );
    let barrier = Arc::new(Barrier::new(2));
    let first_barrier = Arc::clone(&barrier);
    let first = std::thread::spawn(move || {
        first_barrier.wait();
        aarch64_vm::handle_page_fault(PID, PAGE, true, false)
    });
    let second = std::thread::spawn(move || {
        barrier.wait();
        aarch64_vm::handle_page_fault(PID, PAGE, false, false)
    });
    assert!(first.join().unwrap());
    assert!(second.join().unwrap());
    assert_eq!(arch::INSTALLS.load(Ordering::SeqCst), 1);
    assert_eq!(mm::ALLOCATIONS.load(Ordering::SeqCst), 1);
    assert_eq!(mm::freed_count(), 0);
}

#[test]
fn competing_resident_install_discards_only_private_loser_and_checks_permissions() {
    for writable in [true, false] {
        reset();
        file_mapping(RW);
        *READ_ACTION.lock().unwrap() = ReadAction::InstallResident(writable);
        assert_eq!(
            aarch64_vm::handle_page_fault(PID, PAGE, true, false),
            writable
        );
        let resident = arch::mapping(ROOT, PAGE).unwrap();
        assert_eq!(resident.write, writable);
        assert!(
            !mm::was_freed(resident.frame),
            "other mapper's resident frame must not be reclaimed"
        );
        assert_eq!(mm::ALLOCATIONS.load(Ordering::SeqCst), 2);
        assert_eq!(mm::freed_count(), 1);
        assert_eq!(mm::live_count(), 1);
    }
}

#[test]
fn failed_private_io_accepts_only_authorized_concurrent_winner() {
    for writable in [true, false] {
        reset();
        file_mapping(RW);
        *READ_ACTION.lock().unwrap() = ReadAction::InstallResidentThenFail(writable);
        assert_eq!(
            aarch64_vm::handle_page_fault(PID, PAGE, true, false),
            writable,
            "failed candidate I/O must resolve an authorized winner but not a protection fault"
        );
        let resident = arch::mapping(ROOT, PAGE).unwrap();
        assert_eq!(resident.write, writable);
        assert!(!mm::was_freed(resident.frame));
        assert_eq!(READS.load(Ordering::SeqCst), 1);
        assert_eq!(mm::ALLOCATIONS.load(Ordering::SeqCst), 2);
        assert_eq!(mm::freed_count(), 1);
        assert_eq!(mm::live_count(), 1);
    }
}

#[test]
fn executable_private_page_is_initialized_before_install() {
    reset();
    file_mapping(RX);
    assert!(aarch64_vm::handle_page_fault(PID, PAGE, false, true));
    let resident = arch::mapping(ROOT, PAGE).unwrap();
    assert!(resident.read && !resident.write && resident.execute);
    assert_eq!(mm::bytes(resident.frame), vec![7; 4096]);
    assert_eq!(arch::CODE_SYNCS.load(Ordering::SeqCst), 1);
}

#[test]
fn anonymous_fault_commits_zeroes_and_keeps_roots_isolated() {
    reset();
    assert_eq!(aarch64_vm::map(PID, 4096, RW), Some(PAGE));
    assert!(aarch64_vm::attach_process(
        PID + 1,
        OTHER_ROOT,
        arch::USER_HEAP_BASE
    ));
    assert_eq!(aarch64_vm::map(PID + 1, 4096, RW), Some(PAGE));
    assert!(aarch64_vm::handle_page_fault(PID, PAGE, true, false));
    assert!(aarch64_vm::handle_page_fault(PID + 1, PAGE, true, false));
    let first = arch::mapping(ROOT, PAGE).unwrap().frame;
    let second = arch::mapping(OTHER_ROOT, PAGE).unwrap().frame;
    assert_ne!(first, second);
    assert_eq!(mm::bytes(first), vec![0; 4096]);
    assert_eq!(mm::bytes(second), vec![0; 4096]);
}

#[test]
fn concurrent_brk_growth_serializes_break_metadata_and_ptes() {
    reset();
    let requested = arch::USER_HEAP_BASE + 4096;
    let (entered, resume) = arm_locked_pause(&ALLOCATION_PAUSE);
    let first = std::thread::spawn(move || aarch64_vm::brk(PID, requested));
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let (attempted_tx, attempted_rx) = mpsc::channel();
    let second = std::thread::spawn(move || {
        arch::notify_next_mask_acquire(attempted_tx);
        aarch64_vm::brk(PID, requested)
    });
    attempted_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    resume.send(()).unwrap();
    assert_eq!(first.join().unwrap(), Some(requested));
    assert_eq!(second.join().unwrap(), Some(requested));
    assert_eq!(aarch64_vm::brk(PID, 0), Some(requested));
    assert_eq!(
        mm::ALLOCATIONS.load(Ordering::SeqCst),
        1,
        "concurrent brk must not allocate or map the same new page twice"
    );
    assert_eq!(arch::INSTALLS.load(Ordering::SeqCst), 1);
    assert_eq!(mm::live_count(), 1);
    assert_eq!(
        mm::bytes(arch::mapping(ROOT, arch::USER_HEAP_BASE).unwrap().frame),
        vec![0; 4096]
    );
}

#[test]
fn protect_keeps_metadata_and_pte_permissions_coherent_for_competing_fault() {
    reset();
    file_mapping(RW);
    assert!(aarch64_vm::handle_page_fault(PID, PAGE, true, false));
    let (entered, resume) = arm_locked_pause(&PROTECT_PAUSE);
    let protection = std::thread::spawn(|| aarch64_vm::protect(PID, PAGE, 4096, 1));
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let (attempted_tx, attempted_rx) = mpsc::channel();
    let fault = std::thread::spawn(move || {
        arch::notify_next_mask_acquire(attempted_tx);
        aarch64_vm::handle_page_fault(PID, PAGE, true, false)
    });
    attempted_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    resume.send(()).unwrap();
    assert!(protection.join().unwrap());
    assert!(
        !fault.join().unwrap(),
        "competing write fault must see completed read-only mapping"
    );
    let resident = arch::mapping(ROOT, PAGE).unwrap();
    assert!(resident.read && !resident.write && !resident.execute);
    assert_eq!(mm::ALLOCATIONS.load(Ordering::SeqCst), 1);
    assert_eq!(arch::INSTALLS.load(Ordering::SeqCst), 1);
}
