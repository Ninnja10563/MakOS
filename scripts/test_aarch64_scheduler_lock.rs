use core::cell::{Cell, UnsafeCell};
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use makos_process_table::{ProcessState, ProcessTable};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

#[path = "ticket_production.rs"]
mod ticket_lock;
use ticket_lock::TicketLock;

thread_local! { static IRQ_MASK: Cell<bool> = const { Cell::new(false) }; }

mod arch {
    pub(crate) struct LocalInterruptMask(bool);
    impl LocalInterruptMask {
        pub(crate) fn acquire() -> Self {
            Self(crate::IRQ_MASK.with(|mask| mask.replace(true)))
        }
    }
    impl Drop for LocalInterruptMask {
        fn drop(&mut self) {
            let (next, serving) = crate::PROCESSES.lock.test_counters();
            assert_eq!(next, serving, "restore IRQs only after scheduler-lock release");
            crate::IRQ_MASK.with(|mask| mask.set(self.0));
        }
    }
}

struct SchedulerState { value: u64 }
struct LockedProcesses { lock: TicketLock, state: UnsafeCell<SchedulerState> }
unsafe impl Sync for LockedProcesses {}
static PROCESSES: LockedProcesses = LockedProcesses {
    lock: TicketLock::new(), state: UnsafeCell::new(SchedulerState { value: 0 }),
};
include!("wrapper_production.rs");

fn wait_reserved(lock: &TicketLock, expected: u64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while lock.test_counters().0 != expected {
        assert!(Instant::now() < deadline, "host contender did not reserve its ticket");
        thread::yield_now();
    }
}

#[test]
fn fifo_reservations_prevent_barging() {
    let lock = TicketLock::new();
    let owner = lock.lock();
    let ap1 = lock.test_reserve();
    let ap2 = lock.test_reserve();
    let bsp = lock.test_reserve();
    let newcomer = lock.test_reserve();
    assert_eq!([ap1, ap2, bsp, newcomer], [1, 2, 3, 4]);
    assert!(!lock.test_has_turn(newcomer), "a newcomer must not pass a queued owner");
    drop(owner);
    for (index, ticket) in [ap1, ap2, bsp, newcomer].into_iter().enumerate() {
        assert!(lock.test_has_turn(ticket));
        for later in ticket + 1..=newcomer {
            assert!(!lock.test_has_turn(later), "a newcomer must not pass a queued owner");
        }
        drop(lock.test_enter(ticket));
        assert_eq!(lock.test_counters(), (5, index as u64 + 2));
    }
    let guard = lock.lock();
    assert_eq!(lock.test_counters(), (6, 5));
    drop(guard);
    assert_eq!(lock.test_counters(), (6, 6));
}

#[test]
fn legacy_test_and_set_permits_known_waiter_overtaking() {
    // Replay a legal atomic-operation interleaving of the previous with_state
    // lock, rather than depending on the host thread scheduler to reproduce it.
    // AP1 has already failed acquisition. Nothing in a Boolean lock records
    // that fact, so the releasing CPU can win again before AP1 retries.
    let legacy = AtomicBool::new(true);
    assert!(legacy.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err());
    for _ in 0..16 {
        legacy.store(false, Ordering::Release);
        while legacy.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        assert!(legacy.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err());
    }
    legacy.store(false, Ordering::Release);

    // The same caller order with the actual production reservation/admission
    // methods preserves AP1's turn even if the old owner runs first afterward.
    let fifo = TicketLock::new();
    let owner = fifo.lock();
    let waiting_ap = fifo.test_reserve();
    let old_owner_again = fifo.test_reserve();
    drop(owner);
    for _ in 0..16 {
        assert!(!fifo.test_has_turn(old_owner_again));
        assert!(fifo.test_has_turn(waiting_ap));
    }
    drop(fifo.test_enter(waiting_ap));
    drop(fifo.test_enter(old_owner_again));
    assert_eq!(fifo.test_counters(), (3, 3));
}

#[test]
fn queued_aps_and_bsp_precede_fast_reacquisition() {
    let lock = Arc::new(TicketLock::new());
    let order = Arc::new([const { AtomicU64::new(0) }; 4]);
    let sequence = Arc::new(AtomicUsize::new(0));
    let owner = lock.lock();
    let mut threads = Vec::new();
    for role in [1, 2, 0] {
        let worker_lock = Arc::clone(&lock);
        let order = Arc::clone(&order);
        let sequence = Arc::clone(&sequence);
        threads.push(thread::spawn(move || {
            let _guard = worker_lock.lock();
            let index = sequence.fetch_add(1, Ordering::Relaxed);
            order[index].store(role + 1, Ordering::Relaxed);
        }));
        wait_reserved(&lock, threads.len() as u64 + 1);
    }
    drop(owner);
    // This caller was the previous owner and may run immediately. Its new
    // ticket still follows both APs and the queued BSP completion poll.
    let guard = lock.lock();
    let index = sequence.fetch_add(1, Ordering::Relaxed);
    order[index].store(4, Ordering::Relaxed);
    drop(guard);
    for handle in threads { handle.join().unwrap(); }
    assert_eq!(order.each_ref().map(|value| value.load(Ordering::Acquire)), [2, 3, 1, 4]);
}

#[test]
fn ticket_wrap_releases_the_next_owner() {
    let lock = TicketLock::test_at(u64::MAX - 1);
    let owner = lock.lock();
    let before_wrap = lock.test_reserve();
    let after_wrap = lock.test_reserve();
    assert_eq!(before_wrap, u64::MAX);
    assert_eq!(after_wrap, 0);
    drop(owner);
    assert!(lock.test_has_turn(before_wrap));
    assert!(!lock.test_has_turn(after_wrap));
    drop(lock.test_enter(before_wrap));
    assert_eq!(lock.test_counters(), (1, 0), "ticket release must wrap to the next reservation");
    drop(lock.test_enter(after_wrap));
    assert_eq!(lock.test_counters(), (1, 1));
    drop(lock.lock());
    assert_eq!(lock.test_counters(), (2, 2));
}

struct SharedCounter {
    lock: TicketLock,
    entered: AtomicBool,
    value: UnsafeCell<u64>,
}
unsafe impl Sync for SharedCounter {}

#[test]
fn exclusion_and_release_publish_protected_state() {
    let shared = Arc::new(SharedCounter {
        lock: TicketLock::new(), entered: AtomicBool::new(false), value: UnsafeCell::new(0),
    });
    let mut threads = Vec::new();
    for _ in 0..4 {
        let shared = Arc::clone(&shared);
        threads.push(thread::spawn(move || {
            for _ in 0..128 {
                let _guard = shared.lock.lock();
                assert!(!shared.entered.swap(true, Ordering::SeqCst));
                unsafe { *shared.value.get() += 1; }
                assert!(shared.entered.swap(false, Ordering::SeqCst));
            }
        }));
    }
    for handle in threads { handle.join().unwrap(); }
    let _guard = shared.lock.lock();
    assert_eq!(unsafe { *shared.value.get() }, 512);
}

#[test]
fn unwinding_drops_the_acquired_guard() {
    let lock = TicketLock::new();
    let panic = std::panic::catch_unwind(|| {
        let _guard = lock.lock();
        panic!("intentional guarded host unwind");
    });
    assert!(panic.is_err());
    assert_eq!(lock.test_counters(), (1, 1));
    drop(lock.lock());
    assert_eq!(lock.test_counters(), (2, 2));
}

#[test]
fn scheduler_wrapper_restores_prior_mask() {
    for prior in [false, true] {
        IRQ_MASK.with(|mask| mask.set(prior));
        let observed = with_state(|state| {
            assert!(IRQ_MASK.with(Cell::get),
                    "scheduler state must be protected from local interrupt re-entry");
            let (next, serving) = PROCESSES.lock.test_counters();
            assert_eq!(next.wrapping_sub(serving), 1);
            state.value += 1;
            state.value
        });
        assert_ne!(observed, 0);
        assert_eq!(IRQ_MASK.with(Cell::get), prior);
        let (next, serving) = PROCESSES.lock.test_counters();
        assert_eq!(next, serving);
    }
    IRQ_MASK.with(|mask| mask.set(false));
}

#[test]
fn interrupt_adapter_preserves_nested_prior_state() {
    IRQ_MASK.with(|mask| mask.set(false));
    let outer = arch::LocalInterruptMask::acquire();
    assert!(IRQ_MASK.with(Cell::get));
    let inner = arch::LocalInterruptMask::acquire();
    assert!(IRQ_MASK.with(Cell::get));
    drop(inner);
    assert!(IRQ_MASK.with(Cell::get));
    drop(outer);
    assert!(!IRQ_MASK.with(Cell::get));
}

struct DispatchState {
    table: ProcessTable<8>,
    pids: [u64; 6],
    yields_left: [u64; 6],
    dispatches: [u64; 4],
    completed: usize,
}
struct SharedDispatch {
    lock: TicketLock,
    state: UnsafeCell<DispatchState>,
}
unsafe impl Sync for SharedDispatch {}

fn dispatch_step(shared: &SharedDispatch, cpu: usize) {
    let _guard = shared.lock.lock();
    let state = unsafe { &mut *shared.state.get() };
    if cpu == 0 {
        // The BSP waits by reading lifecycle state; it must not manufacture a
        // dispatch or consume the AP-only workload's runnable entries.
        assert_eq!(state.table.current_pid_on(0), None);
        let (_, _, zombies) = state.table.counts();
        assert_eq!(zombies, state.completed);
        return;
    }
    if let Some(pid) = state.table.current_pid_on(cpu) {
        let index = state.pids.iter().position(|candidate| *candidate == pid).unwrap();
        if state.yields_left[index] == 0 {
            let exit = state.table.exit_current_on(cpu, 80 + index as u64).unwrap();
            assert_eq!(exit.state, ProcessState::Zombie);
            state.completed += 1;
        } else {
            state.yields_left[index] -= 1;
        }
    }
    if let Some(next) = state.table.schedule_next_where_on(cpu, |_| (1..4).contains(&cpu)) {
        state.dispatches[cpu] += 1;
        assert_eq!(state.table.running_cpu(next.pid), Some(cpu));
        assert_eq!((0..4).filter(|candidate| {
            state.table.current_pid_on(*candidate) == Some(next.pid)
        }).count(), 1);
    }
}

#[test]
fn queued_six_task_yield_workload_retains_real_dispatch_bounds() {
    let mut table = ProcessTable::<8>::new();
    let pids = core::array::from_fn(|index| table.spawn(0, 0x1000 * (index as u64 + 1)).unwrap());
    let shared = Arc::new(SharedDispatch {
        lock: TicketLock::new(),
        state: UnsafeCell::new(DispatchState {
            table, pids, yields_left: [48; 6], dispatches: [0; 4], completed: 0,
        }),
    });
    let (finished_tx, finished_rx) = mpsc::channel();
    let mut workers = Vec::new();
    let mut commands = Vec::new();
    for cpu in [3, 1, 2, 0] {
        let (command_tx, command_rx) = mpsc::channel();
        commands.push(command_tx);
        let shared = Arc::clone(&shared);
        let finished_tx = finished_tx.clone();
        workers.push(thread::spawn(move || {
            while let Ok(()) = command_rx.recv() {
                dispatch_step(&shared, cpu);
                // Exercise the previous owner trying to barge ahead of APs
                // that are already queued, without pinning any guest task.
                if cpu == 3 { dispatch_step(&shared, cpu); }
                finished_tx.send(cpu).unwrap();
            }
        }));
    }
    let mut completed = false;
    for _ in 0..400 {
        let guard = shared.lock.lock();
        if unsafe { (*shared.state.get()).completed } == 6 {
            drop(guard);
            completed = true;
            break;
        }
        let mut reserved = shared.lock.test_counters().0;
        for command in &commands {
            command.send(()).unwrap();
            reserved = reserved.wrapping_add(1);
            wait_reserved(&shared.lock, reserved);
        }
        drop(guard);
        for _ in 0..4 { finished_rx.recv_timeout(Duration::from_secs(10)).unwrap(); }
    }
    drop(commands);
    for worker in workers { worker.join().unwrap(); }
    assert!(completed, "finite queued workload must complete");
    let _guard = shared.lock.lock();
    let state = unsafe { &*shared.state.get() };
    let dispatches = &state.dispatches[1..];
    let minimum = *dispatches.iter().min().unwrap();
    let maximum = *dispatches.iter().max().unwrap();
    let total = dispatches.iter().sum::<u64>();
    assert_eq!(total, 294, "six initial entries plus 288 genuine yield resumptions");
    assert!(minimum >= 32);
    assert!(total >= 288);
    assert!(maximum <= minimum.saturating_mul(3));
    for (index, pid) in state.pids.iter().enumerate() {
        let info = state.table.get(*pid).unwrap();
        assert_eq!(info.state, ProcessState::Zombie);
        assert_eq!(info.exit_status, 80 + index as u64);
        assert_eq!(state.table.running_cpu(*pid), None);
        assert_eq!(info.resource, 0x1000 * (index as u64 + 1));
    }
}
