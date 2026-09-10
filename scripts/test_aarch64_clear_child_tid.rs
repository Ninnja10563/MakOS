//! Controlled scheduling and memory adapters for production exit-TID cleanup.
#![allow(dead_code)]

use std::cell::{Cell, RefCell};

#[macro_export]
macro_rules! serial_println {
    ($($argument:tt)*) => {{ let _ = format!($($argument)*); }};
}

#[derive(Clone, Copy)]
struct Context {
    ttbr0: u64,
}
struct Slot {
    pid: u64,
    context: Context,
    clear_child_tid: u64,
}
struct Table {
    current: Option<u64>,
}
impl Table {
    fn current_pid_on(&self, cpu: usize) -> Option<u64> {
        assert_eq!(cpu, 1);
        self.current
    }
}
struct SchedulerState {
    table: Table,
    contexts: Vec<Slot>,
}
impl SchedulerState {
    fn empty() -> Self {
        Self {
            table: Table { current: None },
            contexts: Vec::new(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct FutexKey {
    root: u64,
    address: u64,
}
impl FutexKey {
    fn new(root: u64, address: u64) -> Self {
        Self { root, address }
    }
}

thread_local! {
    static STATE: RefCell<SchedulerState> = RefCell::new(SchedulerState::empty());
    static LOCKED: Cell<bool> = const { Cell::new(false) };
    static WRITABLE: Cell<bool> = const { Cell::new(false) };
    static ADDRESS: Cell<u64> = const { Cell::new(0) };
    static WOKEN: Cell<usize> = const { Cell::new(0) };
    static EVENTS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

fn scheduler_cpu() -> usize {
    1
}
fn event(name: &'static str) {
    EVENTS.with_borrow_mut(|events| events.push(name));
}
fn with_state<R>(function: impl FnOnce(&mut SchedulerState) -> R) -> R {
    assert!(!LOCKED.replace(true), "scheduler lock recursively acquired");
    event("lock");
    let result = STATE.with_borrow_mut(function);
    assert!(LOCKED.replace(false));
    event("unlock");
    result
}

mod arch {
    pub fn user_range_writable(address: u64, bytes: usize) -> bool {
        assert!(
            !super::LOCKED.get(),
            "memory validation held scheduler lock"
        );
        assert_eq!(address, super::ADDRESS.get());
        assert_eq!(bytes, std::mem::size_of::<u32>());
        super::event("validate");
        super::WRITABLE.get()
    }
}

fn wake_futex_in_state(state: &mut SchedulerState, key: FutexKey, maximum: usize) -> usize {
    assert!(
        LOCKED.get(),
        "Ready publication occurred outside scheduler lock"
    );
    assert_eq!(key, FutexKey::new(0x4012_e000, ADDRESS.get()));
    assert_eq!(
        maximum,
        usize::MAX,
        "exit cleanup must retain wake-all semantics"
    );
    assert_eq!(
        state.contexts[0].clear_child_tid, 0,
        "registration was not consumed"
    );
    assert_eq!(
        unsafe { (key.address as *const u32).read_volatile() },
        0,
        "child TID was not zero before futex wake"
    );
    event("zero-observed");
    event("wake");
    WOKEN.get()
}

fn notify_idle_cpus() {
    assert!(
        !LOCKED.get(),
        "IPI emitted before scheduler publication unlocked"
    );
    assert!(WOKEN.get() > 0, "IPI emitted without a woken waiter");
    let preceding =
        EVENTS.with_borrow(|events| events.iter().rev().take(2).copied().collect::<Vec<_>>());
    assert_eq!(preceding, ["unlock", "wake"]);
    assert_eq!(unsafe { (ADDRESS.get() as *const u32).read_volatile() }, 0);
    event("ipi");
}

include!("cleanup.rs");

fn setup(word: &mut u32, address_zero: bool, writable: bool, woken: usize) {
    assert!(!LOCKED.get());
    let address = if address_zero {
        0
    } else {
        word as *mut u32 as u64
    };
    ADDRESS.set(address);
    WRITABLE.set(writable);
    WOKEN.set(woken);
    EVENTS.with_borrow_mut(Vec::clear);
    STATE.with_borrow_mut(|state| {
        *state = SchedulerState {
            table: Table { current: Some(7) },
            contexts: vec![Slot {
                pid: 7,
                context: Context { ttbr0: 0x4012_e000 },
                clear_child_tid: address,
            }],
        }
    });
}
fn events() -> Vec<&'static str> {
    EVENTS.with_borrow(Clone::clone)
}
fn consumed() -> bool {
    STATE.with_borrow(|state| state.contexts[0].clear_child_tid == 0)
}

#[test]
fn positive_wake_notifies_idle_cpus_after_unlock() {
    for count in [1, 2, 128] {
        let mut word = 7;
        setup(&mut word, false, true, count);
        clear_child_tid_on_exit();
        assert_eq!(word, 0);
        assert!(consumed());
        let observed = events();
        assert_eq!(
            observed.iter().filter(|event| **event == "ipi").count(),
            1,
            "Ready futex waiters were not notified"
        );
        assert_eq!(
            observed,
            [
                "lock",
                "unlock",
                "validate",
                "lock",
                "zero-observed",
                "wake",
                "unlock",
                "ipi"
            ]
        );
    }
}

#[test]
fn zero_wake_count_clears_word_without_spurious_ipi() {
    let mut word = 7;
    setup(&mut word, false, true, 0);
    clear_child_tid_on_exit();
    assert_eq!(word, 0);
    assert!(consumed());
    assert_eq!(
        events(),
        [
            "lock",
            "unlock",
            "validate",
            "lock",
            "zero-observed",
            "wake",
            "unlock"
        ]
    );
}

#[test]
fn zero_registration_does_not_validate_write_wake_or_notify() {
    let mut word = 7;
    setup(&mut word, true, true, 2);
    clear_child_tid_on_exit();
    assert_eq!(word, 7);
    assert!(consumed());
    assert_eq!(events(), ["lock", "unlock"]);
}

#[test]
fn unmapped_registration_is_consumed_without_writing_or_waking() {
    let mut word = 7;
    setup(&mut word, false, false, 2);
    clear_child_tid_on_exit();
    assert_eq!(word, 7);
    assert!(consumed());
    assert_eq!(events(), ["lock", "unlock", "validate"]);
}

#[test]
fn no_current_task_or_context_has_no_effect() {
    for missing_current in [true, false] {
        let mut word = 7;
        setup(&mut word, false, true, 2);
        STATE.with_borrow_mut(|state| {
            if missing_current {
                state.table.current = None;
            } else {
                state.contexts.clear();
            }
        });
        clear_child_tid_on_exit();
        assert_eq!(word, 7);
        assert_eq!(events(), ["lock", "unlock"]);
    }
}

#[test]
fn repeated_cleanup_does_not_repeat_write_wake_or_ipi() {
    let mut word = 7;
    setup(&mut word, false, true, 2);
    clear_child_tid_on_exit();
    assert!(consumed());
    EVENTS.with_borrow_mut(Vec::clear);
    word = 99;
    clear_child_tid_on_exit();
    assert_eq!(word, 99);
    assert_eq!(events(), ["lock", "unlock"]);
}
