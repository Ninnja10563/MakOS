// Host-only extension of test_aarch64_vm_fault_commit.rs's existing adapters.
// The Python runner adds the production helper and resident read validator to
// the arch module; these adapters expose its real VM-populated physical bytes.

static CURRENT_PID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(PID);
static ACTIVE_ROOT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(ROOT);
static WRITES: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

// Rust modules cannot be reopened: this sibling supplies hardware adapters
// imported through the arch module's production include below.
mod user_read_adapters {
    use super::*;

    thread_local! {
        static DESCRIPTOR: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }

    pub fn cached_active_root() -> u64 {
        ACTIVE_ROOT.load(Ordering::SeqCst)
    }

    pub fn user_page_slot(root: u64, page: u64) -> Option<*mut u64> {
        let resident = arch::mapping(root, page)?;
        let permission = if !resident.read {
            0
        } else if resident.write {
            0b01 << 6
        } else {
            0b11 << 6
        };
        Some(DESCRIPTOR.with(|descriptor| {
            descriptor.set(0b11 | permission);
            descriptor.as_ptr()
        }))
    }

    pub fn read_table_entry(table: u64, index: usize) -> u64 {
        assert_eq!(index, 0, "adapter exposes the requested leaf only");
        unsafe { (table as *const u64).read() }
    }
}

fn setup() {
    reset();
    CURRENT_PID.store(PID, Ordering::SeqCst);
    ACTIVE_ROOT.store(ROOT, Ordering::SeqCst);
    WRITES.lock().unwrap().clear();
}

fn attempt_write(address: u64, length: usize) -> Option<usize> {
    if !arch::validated_read_buffer(address, length) {
        return None;
    }
    assert!(
        !arch::interrupts_masked(),
        "copy must run outside the VM lock"
    );
    // Capture whole bytes only after both production validations complete.
    // Real serial/TTY atomicity has its separate production emission tests.
    let root = ACTIVE_ROOT.load(Ordering::SeqCst);
    let mut bytes = Vec::with_capacity(length);
    for offset in 0..length {
        let address = address + offset as u64;
        let page = address & !4095;
        let resident = arch::mapping(root, page).expect("authorized buffer has a PTE");
        assert!(resident.read, "denied read reached copy adapter");
        bytes.push(mm::bytes(resident.frame)[(address - page) as usize]);
    }
    WRITES.lock().unwrap().push(bytes);
    Some(length)
}

fn no_work_since(allocations: usize, reads: usize) {
    assert_eq!(mm::ALLOCATIONS.load(Ordering::SeqCst), allocations);
    assert_eq!(READS.load(Ordering::SeqCst), reads);
    assert!(WRITES.lock().unwrap().is_empty());
}

#[test]
fn cold_immutable_package_buffer_reaches_one_complete_write() {
    setup();
    file_mapping(1);
    assert!(arch::mapping(ROOT, PAGE).is_none());
    assert!(!arch::user_range_readable(PAGE + 97, 17));
    assert_eq!(
        attempt_write(PAGE + 97, 17),
        Some(17),
        "cold authorized package buffer must be populated before copying"
    );
    assert_eq!(*WRITES.lock().unwrap(), vec![vec![7; 17]]);
    assert_eq!(READS.load(Ordering::SeqCst), 1);
    assert_eq!(mm::ALLOCATIONS.load(Ordering::SeqCst), 1);
    let resident = arch::mapping(ROOT, PAGE).unwrap();
    assert!(resident.read && !resident.write && !resident.execute);
}

#[test]
fn cold_rx_buffer_is_readable_without_granting_write_permission() {
    setup();
    file_mapping(RX);
    assert_eq!(attempt_write(PAGE + 17, 21), Some(21));
    let resident = arch::mapping(ROOT, PAGE).unwrap();
    assert!(resident.read && !resident.write && resident.execute);
    assert_eq!(arch::CODE_SYNCS.load(Ordering::SeqCst), 1);
    assert_eq!(*WRITES.lock().unwrap(), vec![vec![7; 21]]);
    assert!(
        aarch64_vm::map(PID, 4096, 7).is_none(),
        "W+X remains denied"
    );
}

#[test]
fn two_cold_pages_are_populated_before_one_cross_page_write() {
    setup();
    file_mapping(1);
    assert_eq!(
        aarch64_vm::map_file(PID, PAGE + 4096, 4096, 1, true, 8, 0),
        Some(PAGE + 4096)
    );
    assert_eq!(attempt_write(PAGE + 4093, 7), Some(7));
    assert_eq!(*WRITES.lock().unwrap(), vec![vec![7, 7, 7, 8, 8, 8, 8]]);
    assert_eq!(READS.load(Ordering::SeqCst), 2);
    assert_eq!(mm::ALLOCATIONS.load(Ordering::SeqCst), 2);
}

#[test]
fn crossing_into_hole_or_prot_none_never_writes_a_prefix() {
    for second_protection in [None, Some(0)] {
        setup();
        file_mapping(1);
        if let Some(protection) = second_protection {
            assert_eq!(
                aarch64_vm::map_file(PID, PAGE + 4096, 4096, protection, true, 8, 0),
                Some(PAGE + 4096)
            );
        }
        assert_eq!(attempt_write(PAGE + 4093, 7), None);
        assert!(
            WRITES.lock().unwrap().is_empty(),
            "denied suffix forbids prefix output"
        );
        assert!(arch::mapping(ROOT, PAGE + 4096).is_none());
        assert_eq!(READS.load(Ordering::SeqCst), 1);
        assert_eq!(mm::ALLOCATIONS.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn cold_prot_none_and_unmapped_buffers_do_not_allocate_or_read() {
    for mapped in [false, true] {
        setup();
        if mapped {
            file_mapping(0);
        }
        assert_eq!(attempt_write(PAGE, 17), None);
        no_work_since(0, 0);
    }
}

#[test]
fn resident_prot_none_is_rejected_after_population_preparation() {
    setup();
    file_mapping(1);
    assert!(aarch64_vm::handle_page_fault(PID, PAGE, false, false));
    assert!(aarch64_vm::protect(PID, PAGE, 4096, 0));
    assert!(arch::mapping(ROOT, PAGE).is_some());
    assert!(aarch64_vm::fault_in_range(PID, PAGE, 17, false));
    assert!(
        !arch::validated_read_buffer(PAGE, 17),
        "resident PROT_NONE must not reach the copy adapter"
    );
    assert_eq!(attempt_write(PAGE, 17), None);
    no_work_since(1, 1);
}

#[test]
fn readable_resident_page_is_not_allocated_or_read_again() {
    setup();
    file_mapping(1);
    assert_eq!(attempt_write(PAGE, 17), Some(17));
    assert_eq!(attempt_write(PAGE, 17), Some(17));
    assert_eq!(READS.load(Ordering::SeqCst), 1);
    assert_eq!(mm::ALLOCATIONS.load(Ordering::SeqCst), 1);
    assert_eq!(*WRITES.lock().unwrap(), vec![vec![7; 17], vec![7; 17]]);
}

#[test]
fn same_virtual_address_uses_only_current_process_backing() {
    setup();
    file_mapping(1);
    assert!(aarch64_vm::attach_process(
        PID + 1,
        OTHER_ROOT,
        arch::USER_HEAP_BASE
    ));
    assert_eq!(
        aarch64_vm::map_file(PID + 1, PAGE, 4096, 1, true, 8, 0),
        Some(PAGE)
    );
    assert_eq!(attempt_write(PAGE, 17), Some(17));
    assert!(arch::mapping(OTHER_ROOT, PAGE).is_none());
    CURRENT_PID.store(PID + 1, Ordering::SeqCst);
    ACTIVE_ROOT.store(OTHER_ROOT, Ordering::SeqCst);
    assert_eq!(attempt_write(PAGE, 17), Some(17));
    assert_eq!(*WRITES.lock().unwrap(), vec![vec![7; 17], vec![8; 17]]);
    assert_ne!(
        arch::mapping(ROOT, PAGE).unwrap().frame,
        arch::mapping(OTHER_ROOT, PAGE).unwrap().frame
    );
}

#[test]
fn another_process_mapping_cannot_authorize_an_unmapped_current_buffer() {
    setup();
    file_mapping(1);
    assert!(aarch64_vm::attach_process(
        PID + 1,
        OTHER_ROOT,
        arch::USER_HEAP_BASE
    ));
    CURRENT_PID.store(PID + 1, Ordering::SeqCst);
    ACTIVE_ROOT.store(OTHER_ROOT, Ordering::SeqCst);
    assert_eq!(attempt_write(PAGE, 17), None);
    assert!(arch::mapping(ROOT, PAGE).is_none());
    no_work_since(0, 0);
}

#[test]
fn invalid_ranges_and_missing_active_root_do_not_perform_io() {
    setup();
    file_mapping(1);
    for (address, length) in [
        (0, 17),
        (arch::USER_ADDRESS_BASE - 1, 1),
        (arch::USER_STACK_TOP, 1),
        (arch::USER_STACK_TOP - 1, 2),
        (u64::MAX - 3, 8),
        (PAGE, usize::MAX),
    ] {
        assert_eq!(attempt_write(address, length), None);
        no_work_since(0, 0);
    }
    ACTIVE_ROOT.store(0, Ordering::SeqCst);
    assert_eq!(attempt_write(PAGE, 17), None);
    no_work_since(0, 0);
}

#[test]
fn oversized_buffer_does_not_allocate_or_read_its_first_page() {
    setup();
    file_mapping(1);
    // A broken bound will try this legitimate first page, then encounter I/O
    // failure. The assertion detects side effects even though write fails.
    *READ_ACTION.lock().unwrap() = ReadAction::Fail;
    assert_eq!(attempt_write(PAGE, 16 * 1024 * 1024 + 1), None);
    assert_eq!(
        (
            mm::ALLOCATIONS.load(Ordering::SeqCst),
            READS.load(Ordering::SeqCst)
        ),
        (0, 0),
        "oversized syscall buffer must not allocate or perform I/O"
    );
    assert!(WRITES.lock().unwrap().is_empty());
}

#[test]
fn failed_backing_read_returns_no_bytes_and_reclaims_the_candidate_frame() {
    setup();
    file_mapping(1);
    *READ_ACTION.lock().unwrap() = ReadAction::Fail;
    assert_eq!(attempt_write(PAGE, 17), None);
    assert!(WRITES.lock().unwrap().is_empty());
    assert!(arch::mapping(ROOT, PAGE).is_none());
    assert_eq!(READS.load(Ordering::SeqCst), 1);
    assert_eq!(mm::live_count(), 0);
    assert_eq!(mm::freed_count(), 1);
}

#[test]
fn unmap_or_protection_revocation_during_population_never_copies_stale_bytes() {
    for action in [ReadAction::Unmap, ReadAction::ProtectNone] {
        setup();
        file_mapping(1);
        *READ_ACTION.lock().unwrap() = action;
        assert_eq!(attempt_write(PAGE, 17), None);
        assert!(WRITES.lock().unwrap().is_empty());
        assert!(arch::mapping(ROOT, PAGE).is_none());
        assert_eq!(mm::live_count(), 0);
    }
}

#[test]
fn zero_length_valid_buffer_performs_no_population() {
    setup();
    file_mapping(1);
    assert!(arch::validated_read_buffer(PAGE, 0));
    assert!(arch::mapping(ROOT, PAGE).is_none());
    no_work_since(0, 0);
}
