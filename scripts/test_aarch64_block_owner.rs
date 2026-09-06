//! Host hardware adapters for exact production queue and filesystem lock code.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier, mpsc};
use std::thread;
use std::time::{Duration, Instant};

mod arch {
    use super::*;
    thread_local! { static CPU: Cell<usize> = const { Cell::new(0) }; }
    static CLOCK: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    pub static DEADLINES: AtomicU64 = AtomicU64::new(0);
    pub fn cpu_index() -> usize {
        CPU.get()
    }
    pub fn set_cpu(cpu: usize) {
        CPU.set(cpu);
    }
    fn now() -> u64 {
        CLOCK.get_or_init(Instant::now).elapsed().as_millis() as u64
    }
    pub fn counter_deadline_millis(millis: u64) -> u64 {
        assert_eq!(
            millis, 5_000,
            "production request deadline must remain unchanged"
        );
        DEADLINES.fetch_add(1, Ordering::AcqRel);
        now() + millis
    }
    pub fn counter_deadline_expired(deadline: u64) -> bool {
        now() >= deadline
    }
}

fn fatal(message: &str) -> ! {
    panic!("{message}")
}

mod aarch64_virtio_blk {
    include!("block_queue.rs");

    // No timer thread runs in this test: progress must come from the CPU0
    // contender executing the production filesystem lock path.
    struct LockedState {
        lock: AtomicBool,
    }
    static STATE: LockedState = LockedState {
        lock: AtomicBool::new(false),
    };
    static DEVICE_OPERATIONS: AtomicU64 = AtomicU64::new(0);
    fn notify_service_waiters() {
        std::thread::yield_now();
    }
    fn wait_for_service_event() {
        std::thread::yield_now();
    }
    fn device_operation(action: impl FnOnce() -> bool) -> bool {
        assert_eq!(crate::arch::cpu_index(), 0, "non-owner touched device");
        assert!(
            STATE
                .lock
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
        );
        DEVICE_OPERATIONS.fetch_add(1, Ordering::AcqRel);
        let result = action();
        STATE.lock.store(false, Ordering::Release);
        result
    }
    fn byte(lba: u32, index: usize) -> u8 {
        (lba as usize + index) as u8
    }
    fn read_on_owner(device: usize, lba: u32, output: &mut [u8]) -> bool {
        device_operation(|| {
            assert_eq!(device, 1);
            for (index, value) in output.iter_mut().enumerate() {
                *value = byte(lba, index);
            }
            true
        })
    }
    fn write_on_owner(device: usize, lba: u32, input: &[u8]) -> bool {
        device_operation(|| {
            assert_eq!(device, 1);
            assert!(
                input
                    .iter()
                    .enumerate()
                    .all(|(index, value)| *value == byte(lba, index))
            );
            true
        })
    }
    fn flush_on_owner(device: usize) -> bool {
        device_operation(|| {
            assert_eq!(device, 1);
            true
        })
    }
    pub fn read(lba: u32, length: usize) {
        let mut bytes = vec![0u8; length];
        assert!(queue_request(REQUEST_READ, 1, lba, None, Some(&mut bytes)));
        assert!(
            bytes
                .iter()
                .enumerate()
                .all(|(index, value)| *value == byte(lba, index))
        );
    }
    pub fn write(lba: u32, length: usize) {
        let bytes: Vec<_> = (0..length).map(|index| byte(lba, index)).collect();
        assert!(queue_request(REQUEST_WRITE, 1, lba, Some(&bytes), None));
    }
    pub fn flush() {
        assert!(queue_request(REQUEST_FLUSH, 1, 0, None, None));
    }
    pub fn pending() -> usize {
        SERVICE
            .iter()
            .filter(|slot| slot.state.load(Ordering::Acquire) == SLOT_READY)
            .count()
    }
    pub fn operations() -> u64 {
        DEVICE_OPERATIONS.load(Ordering::Acquire)
    }
    pub fn lock_device(locked: bool) {
        STATE.lock.store(locked, Ordering::Release);
    }
    pub fn reset() {
        assert!(
            SERVICE
                .iter()
                .all(|slot| slot.state.load(Ordering::Acquire) == SLOT_FREE)
        );
        assert!(!STATE.lock.load(Ordering::Acquire));
        reset_service_affinity_evidence();
        DEVICE_OPERATIONS.store(0, Ordering::Release);
        crate::arch::DEADLINES.store(0, Ordering::Release);
    }
}

mod vfs {
    use core::cell::UnsafeCell;
    use core::sync::atomic::{AtomicBool, Ordering};
    type State = u64;
    struct LockedState {
        lock: AtomicBool,
        state: UnsafeCell<State>,
    }
    unsafe impl Sync for LockedState {}
    static STATE: LockedState = LockedState {
        lock: AtomicBool::new(false),
        state: UnsafeCell::new(0),
    };
    include!("vfs_lock.rs");
    pub fn run(action: impl FnOnce()) {
        with_state(|_| action());
    }
}

mod volume {
    use core::cell::UnsafeCell;
    use core::sync::atomic::{AtomicBool, Ordering};
    type InodeCache = u64;
    struct LockedCache {
        lock: AtomicBool,
        value: UnsafeCell<InodeCache>,
    }
    unsafe impl Sync for LockedCache {}
    static INODE_CACHE: LockedCache = LockedCache {
        lock: AtomicBool::new(false),
        value: UnsafeCell::new(0),
    };
    static MUTATION_LOCK: AtomicBool = AtomicBool::new(false);
    include!("inode_lock.rs");
    include!("mutation_lock.rs");
    pub fn cache(action: impl FnOnce()) {
        with_inode_cache(|_| action());
    }
    pub fn mutate(action: impl FnOnce()) {
        let _guard = MutationGuard::acquire();
        action();
    }
}

fn await_pending(count: usize) {
    let start = Instant::now();
    while aarch64_virtio_blk::pending() != count {
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "AP did not publish request"
        );
        thread::yield_now();
    }
}

#[derive(Clone, Copy)]
enum Lock {
    Vfs,
    Cache,
    Mutation,
}
fn locked(lock: Lock, action: impl FnOnce()) {
    match lock {
        Lock::Vfs => vfs::run(action),
        Lock::Cache => volume::cache(action),
        Lock::Mutation => volume::mutate(action),
    }
}

fn contended_lock_progress(lock: Lock) {
    aarch64_virtio_blk::reset();
    let (acquired, acquisition) = mpsc::channel();
    let ap = thread::spawn(move || {
        arch::set_cpu(1);
        locked(lock, || {
            acquired.send(()).unwrap();
            aarch64_virtio_blk::read(51, 4096);
            aarch64_virtio_blk::write(72, 512);
            aarch64_virtio_blk::flush();
        });
    });
    acquisition.recv_timeout(Duration::from_secs(2)).unwrap();
    await_pending(1);
    assert_eq!(aarch64_virtio_blk::operations(), 0);
    let (finished, completion) = mpsc::channel();
    let owner = thread::spawn(move || {
        arch::set_cpu(0);
        locked(lock, || finished.send(()).unwrap());
    });
    // This is a host regression bound, not a modified guest timeout. Without
    // the contention hook CPU0 cannot acquire the lock and the AP stays READY.
    completion
        .recv_timeout(Duration::from_secs(2))
        .expect("CPU0/AP filesystem-block circular wait");
    ap.join().unwrap();
    owner.join().unwrap();
    assert_eq!(
        aarch64_virtio_blk::service_affinity_evidence(),
        (3, 3, 1, 1, 1, 0)
    );
    assert_eq!(arch::DEADLINES.load(Ordering::Acquire), 3);
}

#[test]
fn vfs_contention_completes_ap_io_without_timer() {
    contended_lock_progress(Lock::Vfs);
}
#[test]
fn inode_cache_contention_completes_ap_io_without_timer() {
    contended_lock_progress(Lock::Cache);
}
#[test]
fn mutation_contention_completes_ap_io_without_timer() {
    contended_lock_progress(Lock::Mutation);
}

#[test]
fn three_concurrent_requests_preserve_identity_and_owner_affinity() {
    aarch64_virtio_blk::reset();
    let barrier = Arc::new(Barrier::new(4));
    let mut aps = Vec::new();
    for cpu in 1..=3 {
        let barrier = barrier.clone();
        aps.push(thread::spawn(move || {
            arch::set_cpu(cpu);
            barrier.wait();
            match cpu {
                1 => aarch64_virtio_blk::read(9, 512),
                2 => aarch64_virtio_blk::write(17, 4096),
                _ => aarch64_virtio_blk::flush(),
            }
        }));
    }
    barrier.wait();
    await_pending(3);
    arch::set_cpu(1);
    assert_eq!(aarch64_virtio_blk::service_requests_while_waiting(), 0);
    assert_eq!(aarch64_virtio_blk::operations(), 0);
    arch::set_cpu(0);
    assert_eq!(aarch64_virtio_blk::service_requests_while_waiting(), 3);
    for ap in aps {
        ap.join().unwrap();
    }
    assert_eq!(
        aarch64_virtio_blk::service_affinity_evidence(),
        (3, 3, 1, 1, 1, 0)
    );
}

#[test]
fn recursive_device_entry_defers_and_timer_service_remains_available() {
    aarch64_virtio_blk::reset();
    let ap = thread::spawn(|| {
        arch::set_cpu(1);
        aarch64_virtio_blk::read(97, 512);
    });
    await_pending(1);
    arch::set_cpu(0);
    aarch64_virtio_blk::lock_device(true);
    assert_eq!(aarch64_virtio_blk::service_requests_while_waiting(), 0);
    assert_eq!(aarch64_virtio_blk::service_requests_from_timer(), 0);
    assert_eq!(aarch64_virtio_blk::pending(), 1);
    assert_eq!(aarch64_virtio_blk::operations(), 0);
    aarch64_virtio_blk::lock_device(false);
    assert_eq!(aarch64_virtio_blk::service_requests_from_timer(), 1);
    ap.join().unwrap();
    assert_eq!(
        aarch64_virtio_blk::service_affinity_evidence(),
        (1, 1, 1, 0, 0, 1)
    );
}
