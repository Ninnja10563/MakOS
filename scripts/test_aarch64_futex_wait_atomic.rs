//! Deterministic adversarial scheduling point around real futex registration.
#![allow(dead_code)]

use makos_futex::{FutexKey, FutexTable, TaskId, WaitError, WaitHandle, WaitOutcome, WaiterState};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

struct Table {
    current: Option<u64>,
}
impl Table {
    fn current_pid_on(&self, cpu: usize) -> Option<u64> {
        assert_eq!(cpu, 1);
        self.current
    }
}
struct Context {
    ttbr0: u64,
}
struct Slot {
    pid: u64,
    group_pid: u64,
    context: Context,
}
struct SchedulerState {
    table: Table,
    contexts: Vec<Slot>,
    futex: FutexTable<4>,
}
static STATE: Mutex<SchedulerState> = Mutex::new(SchedulerState {
    table: Table { current: None },
    contexts: Vec::new(),
    futex: FutexTable::new(),
});
static BEFORE_LOCK: AtomicUsize = AtomicUsize::new(0);
static WORD_ADDRESS: AtomicU64 = AtomicU64::new(0);
static LAST_WAKE_COUNT: AtomicUsize = AtomicUsize::new(usize::MAX);
static WAKE_CALLBACKS: AtomicUsize = AtomicUsize::new(0);
const ROOT: u64 = 0x4012_e000;

fn scheduler_cpu() -> usize {
    1
}

fn wake(key: FutexKey) -> usize {
    STATE
        .lock()
        .unwrap()
        .futex
        .wake(key, usize::MAX, |task, _| {
            assert_eq!(task, TaskId::new(7, 9));
            WAKE_CALLBACKS.fetch_add(1, Ordering::Relaxed);
        })
        .unwrap()
}

fn clear_and_wake() -> usize {
    let address = WORD_ADDRESS.load(Ordering::Relaxed);
    // As in clear-child-TID, publish zero before acquiring the futex/scheduler
    // lock for wake. Deterministic hooks order accesses without host races.
    unsafe { (address as *mut u32).write_volatile(0) };
    wake(FutexKey::new(ROOT, address))
}

fn with_state<R>(function: impl FnOnce(&mut SchedulerState) -> R) -> R {
    match BEFORE_LOCK.swap(0, Ordering::Relaxed) {
        0 => {}
        1 => {
            LAST_WAKE_COUNT.store(clear_and_wake(), Ordering::Relaxed);
        }
        2 => {
            LAST_WAKE_COUNT.store(
                wake(FutexKey::new(ROOT, WORD_ADDRESS.load(Ordering::Relaxed))),
                Ordering::Relaxed,
            );
        }
        _ => panic!("unknown scheduling hook"),
    }
    let mut state = STATE.lock().unwrap();
    function(&mut state)
}

include!("registration.rs");

fn setup(word: &mut u32) -> u64 {
    *STATE.lock().unwrap() = SchedulerState {
        table: Table { current: Some(9) },
        contexts: vec![Slot {
            pid: 9,
            group_pid: 7,
            context: Context { ttbr0: ROOT },
        }],
        futex: FutexTable::new(),
    };
    let address = word as *mut u32 as u64;
    WORD_ADDRESS.store(address, Ordering::Relaxed);
    BEFORE_LOCK.store(0, Ordering::Relaxed);
    LAST_WAKE_COUNT.store(usize::MAX, Ordering::Relaxed);
    WAKE_CALLBACKS.store(0, Ordering::Relaxed);
    address
}

#[test]
fn clear_and_empty_wake_before_lock_rejects_stale_wait() {
    for deadline in [None, Some(500)] {
        let mut word = 9;
        let address = setup(&mut word);
        BEFORE_LOCK.store(1, Ordering::Relaxed);
        let result = register_wait(address, 9, 100, deadline);
        assert_eq!(word, 0);
        assert_eq!(LAST_WAKE_COUNT.load(Ordering::Relaxed), 0);
        assert_eq!(
            result,
            Err(negative_errno(11)),
            "lost wake: stale futex sample queued after clear"
        );
        let state = STATE.lock().unwrap();
        assert_eq!(state.futex.waiting(), 0);
        assert_eq!(state.futex.occupied(), 0);
    }
}

#[test]
fn enqueue_before_clear_is_woken_and_consumable() {
    let mut word = 9;
    let address = setup(&mut word);
    let handle = register_wait(address, 9, 100, None).unwrap();
    assert_eq!(
        STATE.lock().unwrap().futex.state(handle),
        Ok(WaiterState::Waiting)
    );
    assert_eq!(clear_and_wake(), 1);
    assert_eq!(word, 0);
    assert_eq!(WAKE_CALLBACKS.load(Ordering::Relaxed), 1);
    let mut state = STATE.lock().unwrap();
    assert_eq!(state.futex.waiting(), 0);
    assert_eq!(
        state.futex.take_outcome(handle),
        Ok(Some(WaitOutcome::Woken))
    );
    assert_eq!(state.futex.occupied(), 0);
}

#[test]
fn wake_without_value_change_does_not_forbid_a_new_wait() {
    let mut word = 9;
    let address = setup(&mut word);
    BEFORE_LOCK.store(2, Ordering::Relaxed);
    let handle = register_wait(address, 9, 100, None).unwrap();
    assert_eq!(LAST_WAKE_COUNT.load(Ordering::Relaxed), 0);
    assert_eq!(word, 9);
    assert_eq!(
        STATE.lock().unwrap().futex.state(handle),
        Ok(WaiterState::Waiting)
    );
    assert_eq!(clear_and_wake(), 1);
}

#[test]
fn wake_in_another_root_cannot_consume_this_wait() {
    let mut word = 9;
    let address = setup(&mut word);
    let handle = register_wait(address, 9, 100, None).unwrap();
    assert_eq!(wake(FutexKey::new(ROOT + 4096, address)), 0);
    assert_eq!(
        STATE.lock().unwrap().futex.state(handle),
        Ok(WaiterState::Waiting)
    );
    assert_eq!(wake(FutexKey::new(ROOT, address)), 1);
}

#[test]
fn existing_value_and_timeout_errno_are_preserved() {
    let mut word = 0;
    let address = setup(&mut word);
    assert_eq!(
        register_wait(address, 9, 100, None),
        Err(negative_errno(11))
    );
    // Value mismatch retains precedence over an already expired deadline.
    assert_eq!(
        register_wait(address, 9, 100, Some(99)),
        Err(negative_errno(11))
    );
    word = 9;
    assert_eq!(
        register_wait(address, 9, 100, Some(99)),
        Err(negative_errno(110))
    );
    assert_eq!(
        register_wait(address, 9, 100, Some(100)),
        Err(negative_errno(110))
    );
    assert_eq!(word, 9);
    assert_eq!(STATE.lock().unwrap().futex.waiting(), 0);
}

#[test]
fn absent_current_task_or_context_still_returns_invalid() {
    for missing_current in [true, false] {
        let mut word = 9;
        let address = setup(&mut word);
        {
            let mut state = STATE.lock().unwrap();
            if missing_current {
                state.table.current = None;
            } else {
                state.contexts.clear();
            }
        }
        assert_eq!(
            register_wait(address, 9, 100, None),
            Err(negative_errno(22))
        );
        assert_eq!(word, 9);
        assert_eq!(STATE.lock().unwrap().futex.waiting(), 0);
    }
}
