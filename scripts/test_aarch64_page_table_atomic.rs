//! Host memory and hardware adapters; all page-table policy is production code.
#![allow(dead_code)]

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Mutex};

static USER_PAGE_TABLE_LOCK: AtomicBool = AtomicBool::new(false);
static FRAMES: Mutex<BTreeMap<u64, Box<HostPage>>> = Mutex::new(BTreeMap::new());
static SERIAL: Mutex<Vec<String>> = Mutex::new(Vec::new());
static INVALIDATIONS: AtomicU64 = AtomicU64::new(0);

#[repr(C, align(4096))]
struct HostPage([u8; 4096]);

thread_local! {
    static IRQ_MASKED: Cell<bool> = const { Cell::new(false) };
}

struct LocalInterruptMask {
    saved: bool,
}
impl LocalInterruptMask {
    fn acquire() -> Self {
        Self {
            saved: IRQ_MASKED.replace(true),
        }
    }
}
impl Drop for LocalInterruptMask {
    fn drop(&mut self) {
        assert!(IRQ_MASKED.get());
        IRQ_MASKED.set(self.saved);
    }
}

#[macro_export]
macro_rules! serial_println {
    ($($arg:tt)*) => { $crate::SERIAL.lock().unwrap().push(format!($($arg)*)); };
}

fn fatal(message: &str) -> ! {
    panic!("{message}");
}
fn kernel_root() -> u64 {
    0x1000
}
fn cpu_index() -> usize {
    0
}

fn assert_mutation_guard() {
    assert!(
        IRQ_MASKED.get() && USER_PAGE_TABLE_LOCK.load(Ordering::Acquire),
        "page-table mutation outside IRQ-safe guard",
    );
}
fn invalidate_user_page_if_active(_root: u64, _address: u64) {
    assert_mutation_guard();
    INVALIDATIONS.fetch_add(1, Ordering::Relaxed);
}

fn host_frame(fill: u8) -> u64 {
    let page = Box::new(HostPage([fill; 4096]));
    let address = page.0.as_ptr() as u64;
    assert_eq!(address & ADDRESS_MASK, address);
    assert!(FRAMES.lock().unwrap().insert(address, page).is_none());
    address
}
fn free_host_frame(frame: u64) {
    assert!(FRAMES.lock().unwrap().remove(&frame).is_some());
}

mod mm {
    pub fn allocate_frame() -> Option<u64> {
        super::assert_mutation_guard();
        // Yield inside the actual table-allocation boundary so other host
        // threads contend while the production commit still holds its guard.
        std::thread::yield_now();
        Some(super::host_frame(0xa5))
    }
}

include!("page_tables.rs");

fn reset() {
    assert!(!USER_PAGE_TABLE_LOCK.load(Ordering::Acquire));
    assert!(!IRQ_MASKED.get());
    FRAMES.lock().unwrap().clear();
    SERIAL.lock().unwrap().clear();
    INVALIDATIONS.store(0, Ordering::Relaxed);
}

fn root() -> u64 {
    let root = host_frame(0);
    let level1 = host_frame(0);
    unsafe { write_table_entry(root, 0, level1 | TABLE_DESCRIPTOR) };
    root
}

fn leaf(root: u64, address: u64) -> u64 {
    read_table_entry(user_page_slot(root, address).unwrap() as u64, 0)
}

fn change_leaf(root: u64, address: u64, entry: u64) {
    let _guard = UserPageTableGuard::acquire();
    unsafe { write_table_entry(user_page_slot(root, address).unwrap() as u64, 0, entry) };
}

#[test]
fn simultaneous_same_page_has_one_winner_and_reclaims_losers() {
    for _ in 0..32 {
        reset();
        let root = root();
        let candidates: Vec<_> = (1..=8).map(host_frame).collect();
        let start = Arc::new(Barrier::new(candidates.len()));
        let handles: Vec<_> = candidates
            .iter()
            .copied()
            .map(|frame| {
                let start = start.clone();
                std::thread::spawn(move || {
                    start.wait();
                    let result = map_user_page_permissions_if_absent_in(
                        root,
                        USER_MMAP_BASE,
                        frame,
                        true,
                        true,
                        false,
                    );
                    assert!(!IRQ_MASKED.get());
                    if result.is_err() {
                        free_host_frame(frame);
                    }
                    (frame, result)
                })
            })
            .collect();
        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert_eq!(
            results.iter().filter(|(_, result)| result.is_ok()).count(),
            1
        );
        let winner = results.iter().find(|(_, result)| result.is_ok()).unwrap().0;
        let actual = leaf(root, USER_MMAP_BASE);
        assert_eq!(actual & ADDRESS_MASK, winner);
        for (_, result) in results {
            if let Err(existing) = result {
                assert_eq!(
                    existing, actual,
                    "loser observed a different resident mapping"
                );
            }
        }
        assert_eq!(
            FRAMES.lock().unwrap().len(),
            5,
            "candidate or subtree leaked"
        );
        assert_eq!(INVALIDATIONS.load(Ordering::Relaxed), 1);
        assert!(user_page_access_permitted_in(
            root,
            USER_MMAP_BASE,
            true,
            false
        ));
        assert!(!user_page_access_permitted_in(
            root,
            USER_MMAP_BASE,
            false,
            true
        ));
    }
}

#[test]
fn simultaneous_distinct_pages_retain_shared_subtrees() {
    for _ in 0..32 {
        reset();
        let root = root();
        let jobs: Vec<_> = (0..12u64)
            .map(|index| {
                // Three L3 tables under the same previously absent L2 table.
                let address = USER_MMAP_BASE + (index / 4) * BLOCK_SIZE + (index % 4) * PAGE_SIZE;
                (address, host_frame((index + 1) as u8))
            })
            .collect();
        let start = Arc::new(Barrier::new(jobs.len()));
        let handles: Vec<_> = jobs
            .iter()
            .copied()
            .map(|(address, frame)| {
                let start = start.clone();
                std::thread::spawn(move || {
                    start.wait();
                    assert_eq!(
                        map_user_page_permissions_if_absent_in(
                            root, address, frame, true, true, false,
                        ),
                        Ok(())
                    );
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        for &(address, frame) in &jobs {
            assert_eq!(
                user_page_physical_in(root, address),
                Some(frame),
                "mapping lost to hierarchy publication race"
            );
        }
        assert_eq!(
            FRAMES.lock().unwrap().len(),
            2 + 1 + 3 + jobs.len(),
            "duplicate subtree allocated"
        );
        assert_eq!(INVALIDATIONS.load(Ordering::Relaxed), jobs.len() as u64);
    }
}

#[test]
fn roots_are_isolated_and_permissions_do_not_escape() {
    reset();
    let root_a = root();
    let root_b = root();
    let frame_a = host_frame(0x11);
    let frame_b = host_frame(0x22);
    map_user_page_in(root_a, USER_MMAP_BASE, frame_a, true, false);
    map_user_page_in(root_b, USER_MMAP_BASE, frame_b, false, true);
    assert_eq!(user_page_physical_in(root_a, USER_MMAP_BASE), Some(frame_a));
    assert_eq!(user_page_physical_in(root_b, USER_MMAP_BASE), Some(frame_b));
    assert!(user_page_access_permitted_in(
        root_a,
        USER_MMAP_BASE + 1,
        true,
        false
    ));
    assert!(!user_page_access_permitted_in(
        root_b,
        USER_MMAP_BASE,
        true,
        false
    ));
    assert!(user_page_access_permitted_in(
        root_b,
        USER_MMAP_BASE,
        false,
        true
    ));
    assert!(!user_page_access_permitted_in(
        root_a,
        USER_MMAP_BASE,
        false,
        true
    ));
    assert!(!user_page_access_permitted_in(
        root_b,
        USER_MMAP_BASE + 1,
        false,
        true
    ));
}

#[test]
fn strict_duplicate_remains_fatal() {
    reset();
    let root = root();
    let original = host_frame(0x5a);
    let candidate = host_frame(0x9a);
    map_user_page_in(root, USER_MMAP_BASE, original, true, false);
    let before = leaf(root, USER_MMAP_BASE);
    let result = std::panic::catch_unwind(|| {
        map_user_page_in(root, USER_MMAP_BASE, candidate, true, false);
    });
    assert!(result.is_err(), "strict duplicate guard missing");
    let panic = result.unwrap_err();
    assert_eq!(
        panic.downcast_ref::<String>().unwrap(),
        "duplicate AArch64 user-page mapping"
    );
    assert_eq!(
        leaf(root, USER_MMAP_BASE),
        before,
        "strict duplicate overwrote resident data"
    );
    assert!(!IRQ_MASKED.get());
    assert!(!USER_PAGE_TABLE_LOCK.load(Ordering::Acquire));
    let serial = SERIAL.lock().unwrap().join("\n");
    assert!(serial.contains("MAKOS_AARCH64_DUPLICATE_MAPPING cpu=0"));
    assert!(serial.contains(&format!("root={root:#x} va={USER_MMAP_BASE:#x}")));
    assert!(serial.contains(&format!(
        "candidate_pa={candidate:#x} existing_entry={before:#x}"
    )));
}

#[test]
fn mutation_span_holds_lock() {
    reset();
    let root = root();
    let frame = host_frame(0);
    assert_eq!(
        map_user_page_permissions_if_absent_in(root, USER_MMAP_BASE, frame, true, true, false),
        Ok(())
    );
    assert!(protect_user_page_in(root, USER_MMAP_BASE, false, true));
    assert!(user_page_access_permitted_in(
        root,
        USER_MMAP_BASE,
        false,
        true
    ));
    assert_eq!(unmap_user_page_in(root, USER_MMAP_BASE), Some(frame));
    assert_eq!(INVALIDATIONS.load(Ordering::Relaxed), 3);
    assert!(!IRQ_MASKED.get());
    IRQ_MASKED.set(true);
    assert_eq!(
        map_user_page_permissions_if_absent_in(root, USER_MMAP_BASE, frame, true, false, true),
        Ok(())
    );
    assert!(IRQ_MASKED.get(), "nested mask incorrectly enabled IRQs");
    IRQ_MASKED.set(false);
}

#[test]
fn concurrent_unmap_has_one_owner_and_protect_never_resurrects() {
    reset();
    let root = root();
    let frame = host_frame(0);
    map_user_page_in(root, USER_MMAP_BASE, frame, true, false);
    let start = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let start = start.clone();
            std::thread::spawn(move || {
                start.wait();
                unmap_user_page_in(root, USER_MMAP_BASE)
            })
        })
        .collect();
    let removed: Vec<_> = handles
        .into_iter()
        .filter_map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(removed, vec![frame]);
    assert!(!protect_user_page_in(root, USER_MMAP_BASE, false, true));
    assert!(!user_page_access_permitted_in(
        root,
        USER_MMAP_BASE,
        false,
        false
    ));
    assert_eq!(INVALIDATIONS.load(Ordering::Relaxed), 2);
}

#[test]
fn invalid_mapping_and_permission_requests_are_rejected() {
    reset();
    let root = root();
    let frame = host_frame(0);
    for (target_root, address, physical, read, write, execute) in [
        (0, USER_MMAP_BASE, frame, true, true, false),
        (kernel_root(), USER_MMAP_BASE, frame, true, true, false),
        (root + 1, USER_MMAP_BASE, frame, true, true, false),
        (root, USER_MMAP_BASE + 1, frame, true, true, false),
        (
            root,
            USER_ADDRESS_BASE - PAGE_SIZE,
            frame,
            true,
            true,
            false,
        ),
        (root, USER_STACK_TOP, frame, true, true, false),
        (root, USER_MMAP_BASE, 0, true, true, false),
        (root, USER_MMAP_BASE, frame + 1, true, true, false),
        (root, USER_MMAP_BASE, frame | (1 << 63), true, true, false),
        (root, USER_MMAP_BASE, frame, false, true, false),
        (root, USER_MMAP_BASE, frame, false, false, true),
        (root, USER_MMAP_BASE, frame, true, true, true),
    ] {
        assert!(
            std::panic::catch_unwind(|| {
                map_user_page_permissions_if_absent_in(
                    target_root,
                    address,
                    physical,
                    read,
                    write,
                    execute,
                )
            })
            .is_err()
        );
        assert!(!IRQ_MASKED.get());
    }
    map_user_page_in(root, USER_MMAP_BASE, frame, true, false);
    let before = leaf(root, USER_MMAP_BASE);
    assert!(!protect_user_page_permissions_in(
        root,
        USER_MMAP_BASE,
        true,
        true,
        true
    ));
    assert_eq!(leaf(root, USER_MMAP_BASE), before);
    assert!(protect_user_page_permissions_in(
        root,
        USER_MMAP_BASE,
        false,
        false,
        false
    ));
    assert!(!user_page_access_permitted_in(
        root,
        USER_MMAP_BASE,
        false,
        false
    ));
}

#[test]
fn resident_access_rejects_malformed_privileged_and_wx_entries() {
    reset();
    let root = root();
    let frame = host_frame(0);
    map_user_page_in(root, USER_MMAP_BASE, frame, true, false);
    let writable = leaf(root, USER_MMAP_BASE);
    for denied in [
        0,
        writable & !ACCESS_FLAG,
        writable & !PXN,
        writable & !UXN,
        writable & !(0b11 << 6),
        (writable & !0b11) | BLOCK_DESCRIPTOR,
        writable & !ADDRESS_MASK,
    ] {
        change_leaf(root, USER_MMAP_BASE, denied);
        assert!(!user_page_access_permitted_in(
            root,
            USER_MMAP_BASE,
            true,
            false
        ));
        assert!(!user_page_access_permitted_in(
            root,
            USER_MMAP_BASE,
            false,
            false
        ));
        assert!(!user_page_access_permitted_in(
            root,
            USER_MMAP_BASE,
            true,
            true
        ));
    }
    change_leaf(root, USER_MMAP_BASE, writable);
    let mut table = root;
    for shift in [39, 30, 21] {
        let index = ((USER_MMAP_BASE >> shift) & 511) as usize;
        let original = read_table_entry(table, index);
        unsafe { write_table_entry(table, index, original | (1 << 61)) };
        assert!(!user_page_access_permitted_in(
            root,
            USER_MMAP_BASE,
            false,
            false
        ));
        assert!(!user_page_access_permitted_in(
            root,
            USER_MMAP_BASE,
            true,
            false
        ));
        unsafe { write_table_entry(table, index, original | (1 << 62)) };
        assert!(user_page_access_permitted_in(
            root,
            USER_MMAP_BASE,
            false,
            false
        ));
        assert!(!user_page_access_permitted_in(
            root,
            USER_MMAP_BASE,
            true,
            false
        ));
        unsafe { write_table_entry(table, index, original) };
        table = original & ADDRESS_MASK;
    }
    assert!(!user_page_access_permitted_in(
        0,
        USER_MMAP_BASE,
        false,
        false
    ));
    assert!(!user_page_access_permitted_in(
        kernel_root(),
        USER_MMAP_BASE,
        false,
        false
    ));
    assert!(!user_page_access_permitted_in(
        root,
        USER_ADDRESS_BASE - 1,
        false,
        false
    ));
}
