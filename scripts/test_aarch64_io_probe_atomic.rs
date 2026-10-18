//! Host interleavings around complete extracted production I/O admission bodies.
//! Real process-table/readiness crates; explicit clock/frame/dispatch/WFI adapters.
//! No host threads impersonate interrupts. Every ordering below is deterministic.
#![allow(dead_code)]

use makos_process_table::{ProcessInfo, ProcessState, ProcessTable};
use makos_readiness::WaitSource;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

// Unused ContextSlot field type only: no futex path is exercised or claimed.
type WaitHandle = u64;
const CAPACITY: usize = 8;
const SVC_RETURN: u64 = 0x1004;
const SOURCE: WaitSource = WaitSource::Descriptor(71);
const OTHER: WaitSource = WaitSource::Network(72);
static CPU: AtomicUsize = AtomicUsize::new(0);
static TICKS: AtomicU64 = AtomicU64::new(100);
static SMP_ENABLED: AtomicBool = AtomicBool::new(true);
static SELECT_NEXT: AtomicBool = AtomicBool::new(true);
static LOCKED: AtomicBool = AtomicBool::new(false);
static DEVICE_READY: AtomicBool = AtomicBool::new(false);
static INTERRUPTS_ENABLED: AtomicBool = AtomicBool::new(false);
static WFI_COUNT: AtomicUsize = AtomicUsize::new(0);
static SERVICE_COUNT: AtomicUsize = AtomicUsize::new(0);
static NOTIFICATIONS: AtomicUsize = AtomicUsize::new(0);
static SWITCH_ROOT: AtomicU64 = AtomicU64::new(0);
static WFI_EVENT: Mutex<Option<WaitSource>> = Mutex::new(None);
static IDLE_IO_REPORTED: AtomicBool = AtomicBool::new(false);
static IO_BLOCK_REPORTED: AtomicBool = AtomicBool::new(false);
static IO_WAKE_REPORTED: AtomicBool = AtomicBool::new(false);
static SMP_PROBE_IO_IDLE_MASK: AtomicU64 = AtomicU64::new(0);

#[macro_export]
macro_rules! serial_println {
    ($($argument:tt)*) => {{ let _ = format_args!($($argument)*); }};
}

// Exact production asm expression remains in the extracted function. Only
// this single known instruction spelling resolves to the bounded host adapter.
macro_rules! asm {
    ("wfi", options(nomem, nostack)) => { crate::host_wfi() };
}

mod arch {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct UserContext {
        pub registers: [u64; 31],
        pub elr: u64,
        pub stack: u64,
        pub ttbr0: u64,
        pub kernel_return: bool,
    }
    pub type ExceptionFrame = UserContext;
    impl UserContext {
        pub const fn initial(elr: u64, stack: u64, ttbr0: u64, arg0: u64) -> Self {
            let mut registers = [0; 31];
            registers[0] = arg0;
            Self { registers, elr, stack, ttbr0, kernel_return: false }
        }
        pub fn capture(frame: &ExceptionFrame) -> Self { *frame }
        pub fn restore(&self, frame: &mut ExceptionFrame) { *frame = *self; }
    }
    pub fn monotonic_ticks() -> u64 {
        assert!(!LOCKED.load(Ordering::Relaxed), "clock adapter called under scheduler lock");
        TICKS.load(Ordering::Relaxed)
    }
    pub fn smp_probe_scheduler_enabled() -> bool { SMP_ENABLED.load(Ordering::Relaxed) }
    pub fn switch_address_space(root: u64) {
        assert!(!LOCKED.load(Ordering::Relaxed));
        SWITCH_ROOT.store(root, Ordering::Relaxed);
    }
    pub fn return_to_kernel(frame: &mut ExceptionFrame, status: u64) {
        assert!(!LOCKED.load(Ordering::Relaxed));
        frame.kernel_return = true;
        frame.registers[0] = status;
    }
    pub fn enable_interrupts() {
        assert!(!LOCKED.load(Ordering::Relaxed));
        INTERRUPTS_ENABLED.store(true, Ordering::Relaxed);
    }
    pub fn disable_interrupts() {
        assert!(!LOCKED.load(Ordering::Relaxed));
        INTERRUPTS_ENABLED.store(false, Ordering::Relaxed);
    }
    pub fn service_network_rx_on_owner_cpu() { service_device(); }
    pub fn service_input_on_owner_cpu() { service_device(); }
    fn service_device() {
        assert!(!LOCKED.load(Ordering::Relaxed), "device servicing inside scheduler critical section");
        SERVICE_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

struct SchedulerState {
    table: ProcessTable<CAPACITY>,
    contexts: [ContextSlot; CAPACITY],
    timer_switches: u64,
}
impl SchedulerState {
    const fn new() -> Self {
        Self { table: ProcessTable::new(), contexts: [ContextSlot::EMPTY; CAPACITY], timer_switches: 0 }
    }
    fn schedule_next_for_cpu(&mut self, cpu: usize) -> Option<ProcessInfo> {
        assert!(LOCKED.load(Ordering::Relaxed));
        // Eligibility policy is an adapter. Ready/Running ownership and all
        // block/wake/activate transitions come from the real process table.
        self.table.schedule_next_where_on(cpu, |_| SELECT_NEXT.load(Ordering::Relaxed))
    }
}
static STATE: Mutex<SchedulerState> = Mutex::new(SchedulerState::new());

struct LockFlag;
impl Drop for LockFlag {
    fn drop(&mut self) { LOCKED.store(false, Ordering::Relaxed); }
}
fn with_state<R>(function: impl FnOnce(&mut SchedulerState) -> R) -> R {
    let mut state = STATE.lock().unwrap();
    assert!(!LOCKED.swap(true, Ordering::Relaxed), "recursive scheduler lock");
    let _flag = LockFlag;
    function(&mut state)
}
fn scheduler_cpu() -> usize { CPU.load(Ordering::Relaxed) }
fn notify_idle_cpus() {
    assert!(!LOCKED.load(Ordering::Relaxed));
    NOTIFICATIONS.fetch_add(1, Ordering::Relaxed);
}
unsafe fn host_wfi() {
    assert!(!LOCKED.load(Ordering::Relaxed));
    assert!(INTERRUPTS_ENABLED.load(Ordering::Relaxed));
    WFI_COUNT.fetch_add(1, Ordering::Relaxed);
    // A bounded return represents one timer/interrupt opportunity, never a
    // real sleep or progress proof. Optional producer runs after registration.
    let event = WFI_EVENT.lock().unwrap().take();
    if let Some(event) = event { producer(event); }
}

include!("production.rs");

fn frame() -> arch::ExceptionFrame {
    let mut frame = arch::UserContext::initial(SVC_RETURN, 0x9000, 0x1000, 77);
    frame.registers[1] = 0xfeed;
    frame.registers[8] = 72;
    frame
}
fn setup(cpu: usize, peer: bool) -> u64 {
    assert!(!LOCKED.load(Ordering::Relaxed));
    *STATE.lock().unwrap() = SchedulerState::new();
    CPU.store(cpu, Ordering::Relaxed);
    TICKS.store(100, Ordering::Relaxed);
    SMP_ENABLED.store(true, Ordering::Relaxed);
    SELECT_NEXT.store(true, Ordering::Relaxed);
    DEVICE_READY.store(false, Ordering::Relaxed);
    INTERRUPTS_ENABLED.store(false, Ordering::Relaxed);
    WFI_COUNT.store(0, Ordering::Relaxed);
    SERVICE_COUNT.store(0, Ordering::Relaxed);
    NOTIFICATIONS.store(0, Ordering::Relaxed);
    SWITCH_ROOT.store(0, Ordering::Relaxed);
    SMP_PROBE_IO_IDLE_MASK.store(0, Ordering::Relaxed);
    IDLE_IO_REPORTED.store(false, Ordering::Relaxed);
    IO_BLOCK_REPORTED.store(false, Ordering::Relaxed);
    IO_WAKE_REPORTED.store(false, Ordering::Relaxed);
    *WFI_EVENT.lock().unwrap() = None;
    with_state(|state| {
        let tid = state.table.spawn(0, 0x1000).unwrap();
        state.contexts[0] = ContextSlot { pid: tid, group_pid: tid,
            role: ProcessRole::SmpProbe, context: frame(), ..ContextSlot::EMPTY };
        state.table.activate_on(cpu, tid).unwrap();
        if peer {
            let other = state.table.spawn(tid, 0x2000).unwrap();
            state.contexts[1] = ContextSlot { pid: other, group_pid: other,
                role: ProcessRole::SmpProbe,
                context: arch::UserContext::initial(0x5004, 0xa000, 0x2000, 88),
                ..ContextSlot::EMPTY };
        }
        tid
    })
}
fn slot(tid: u64) -> ContextSlot {
    with_state(|state| *state.contexts.iter().find(|slot| slot.pid == tid).unwrap())
}
fn status(tid: u64) -> ProcessState {
    with_state(|state| state.table.get(tid).unwrap().state)
}
fn probe_device() -> bool {
    assert!(!LOCKED.load(Ordering::Relaxed), "readiness probe under scheduler lock");
    DEVICE_READY.load(Ordering::Relaxed)
}
fn producer(source: WaitSource) -> usize {
    assert!(!LOCKED.load(Ordering::Relaxed));
    DEVICE_READY.store(true, Ordering::Relaxed);
    wake_io_source(source)
}
fn resume(tid: u64) -> arch::ExceptionFrame {
    with_state(|state| {
        if state.table.get(tid).unwrap().state != ProcessState::Running {
            state.table.activate_on(scheduler_cpu(), tid).unwrap();
        }
        state.contexts.iter().find(|slot| slot.pid == tid).unwrap().context
    })
}
fn admitted_block(timeout: i64, source: WaitSource, frame: &mut arch::ExceptionFrame) -> IoBlockResult {
    let mut guard = begin_io_probe(source);
    assert!(!probe_device());
    guard.handoff();
    let result = block_current_for_io_on(timeout, source, frame);
    drop(guard);
    result
}
fn clean_probe(tid: u64) {
    let current = slot(tid);
    assert_eq!(current.io_probe_source, None);
    assert!(!current.io_probe_notified);
}

#[test]
fn already_ready_before_arm_finishes_without_registration() {
    for cpu in [0, 1] {
        let tid = setup(cpu, false);
        DEVICE_READY.store(true, Ordering::Relaxed);
        let guard = begin_io_probe(SOURCE);
        assert!(probe_device());
        complete_io_wait();
        drop(guard);
        clean_probe(tid);
        assert_eq!(status(tid), ProcessState::Running);
        assert!(!slot(tid).io_wait);
        assert_eq!(WFI_COUNT.load(Ordering::Relaxed), 0);
    }
}

#[test]
fn gap_wake_latches_while_running() {
    for (cpu, peer) in [(0, false), (0, true), (1, false), (1, true)] {
        for timeout in [-1, 25] {
            let tid = setup(cpu, peer);
            let mut guard = begin_io_probe(SOURCE);
            assert!(!probe_device());
            assert_eq!(producer(SOURCE), 0, "Running task is not a table wake");
            assert!(slot(tid).io_probe_notified, "gap wake must latch while Running");
            guard.handoff();
            let mut output = frame();
            assert!(block_current_for_io_on(timeout, SOURCE, &mut output) == IoBlockResult::Switched);
            drop(guard);
            assert_eq!(status(tid), ProcessState::Running, "notified probe must remain Running");
            assert_eq!(output.elr, SVC_RETURN - 4);
            assert_eq!(output.registers, frame().registers, "retry must preserve syscall arguments");
            assert_eq!(slot(tid).io_deadline, if timeout < 0 { 0 } else { 103 });
            assert_eq!(WFI_COUNT.load(Ordering::Relaxed), 0);
            clean_probe(tid);
        }
    }
}

#[test]
fn gap_notification_skips_wait() {
    let tid = setup(0, false);
    let mut guard = begin_io_probe(SOURCE);
    assert!(!probe_device());
    producer(SOURCE);
    guard.handoff();
    let mut output = frame();
    assert!(block_current_for_io_on(-1, SOURCE, &mut output) == IoBlockResult::Switched);
    assert_eq!(WFI_COUNT.load(Ordering::Relaxed), 0, "gap notification must avoid WFI");
    assert_eq!(status(tid), ProcessState::Running);
}

#[test]
fn post_registration_wake_uses_real_block_ready_transitions() {
    for (cpu, peer) in [(0, true), (1, false), (1, true)] {
        for timeout in [-1, 20] {
            let tid = setup(cpu, peer);
            let mut output = frame();
            assert!(admitted_block(timeout, SOURCE, &mut output) == IoBlockResult::Switched);
            assert_eq!(status(tid), ProcessState::Blocked);
            assert_eq!(slot(tid).context.elr, SVC_RETURN - 4);
            assert_eq!(slot(tid).sleep_deadline, if timeout < 0 { 0 } else { 102 });
            assert_eq!(producer(OTHER), 0);
            assert_eq!(status(tid), ProcessState::Blocked);
            assert_eq!(producer(SOURCE), 1);
            assert_eq!(status(tid), ProcessState::Ready);
            assert_eq!(slot(tid).sleep_deadline, 0);
            assert_eq!(slot(tid).io_deadline, if timeout < 0 { 0 } else { 102 });
            assert_eq!(producer(SOURCE), 0, "repeated event cannot re-wake Ready task");
            assert_eq!(NOTIFICATIONS.load(Ordering::Relaxed), 1);
            assert_eq!(resume(tid).elr, SVC_RETURN - 4);
            complete_io_wait();
            assert!(!slot(tid).io_wait);
        }
    }
}

#[test]
fn cpu0_post_registration_event_is_bounded_wfi_retry() {
    let tid = setup(0, false);
    *WFI_EVENT.lock().unwrap() = Some(SOURCE);
    let mut output = frame();
    assert!(admitted_block(20, SOURCE, &mut output) == IoBlockResult::Switched);
    assert_eq!(status(tid), ProcessState::Running);
    assert_eq!(output.elr, SVC_RETURN - 4);
    assert!(probe_device());
    assert_eq!(WFI_COUNT.load(Ordering::Relaxed), 1);
    assert_eq!(SERVICE_COUNT.load(Ordering::Relaxed), 2);
    assert!(!INTERRUPTS_ENABLED.load(Ordering::Relaxed));
    assert_eq!(slot(tid).io_deadline, 102);
    assert_eq!(NOTIFICATIONS.load(Ordering::Relaxed), 0);
}

#[test]
fn source_matching_is_exact_or_wildcard_for_running_and_blocked() {
    let pairs = [
        (WaitSource::Descriptor(1), WaitSource::Descriptor(1), true),
        (WaitSource::Descriptor(1), WaitSource::Descriptor(2), false),
        (WaitSource::Descriptor(1), WaitSource::Network(1), false),
        (WaitSource::Network(1), WaitSource::Network(1), true),
        (WaitSource::Network(1), WaitSource::Network(2), false),
        (WaitSource::Any, WaitSource::Network(1), true),
        (WaitSource::Network(1), WaitSource::Any, true),
        (WaitSource::Any, WaitSource::Any, true),
    ];
    for (source, event, matching) in pairs {
        let tid = setup(1, false);
        let guard = begin_io_probe(source);
        for _ in 0..3 { assert_eq!(wake_io_source(event), 0); }
        assert_eq!(slot(tid).io_probe_notified, matching);
        drop(guard);
        let mut output = frame();
        assert!(admitted_block(-1, source, &mut output) == IoBlockResult::Switched);
        assert_eq!(wake_io_source(event), if matching { 1 } else { 0 });
        assert_eq!(status(tid), if matching { ProcessState::Ready } else { ProcessState::Blocked });
    }
}

#[test]
fn global_wrapper_admission_and_global_wake_remain_supported() {
    let tid = setup(1, false);
    let mut guard = begin_io_probe(WaitSource::Any);
    guard.handoff();
    assert!(block_current_for_io(-1, &mut frame()) == IoBlockResult::Switched);
    assert_eq!(status(tid), ProcessState::Blocked);
    assert_eq!(wake_io_waiters(), 1);
    assert_eq!(status(tid), ProcessState::Ready);
}

#[test]
fn relative_deadline_survives_reprobe_and_expires() {
    for (cpu, peer) in [(0, false), (0, true), (1, false), (1, true)] {
        let tid = setup(cpu, peer);
        for now in [100, 101] {
            TICKS.store(now, Ordering::Relaxed);
            let mut guard = begin_io_probe(SOURCE);
            wake_io_source(SOURCE);
            guard.handoff();
            assert!(block_current_for_io_on(20, SOURCE, &mut frame()) == IoBlockResult::Switched);
            assert_eq!(slot(tid).io_deadline, 102, "reprobe reset the original deadline");
            assert_eq!(status(tid), ProcessState::Running);
        }
        TICKS.store(102, Ordering::Relaxed);
        let mut guard = begin_io_probe(SOURCE);
        wake_io_source(SOURCE);
        guard.handoff();
        assert!(block_current_for_io_on(20, SOURCE, &mut frame()) == IoBlockResult::TimedOut,
                "expiry must precede notification retry");
        let current = slot(tid);
        assert!(!current.io_wait);
        assert_eq!(current.io_deadline, 0);
        assert_eq!(current.sleep_deadline, 0);
        assert_eq!(current.io_source, WaitSource::Any);
        clean_probe(tid);
    }
}

#[test]
fn blocked_woken_reprobe_preserves_deadline_until_expiry() {
    for (cpu, peer) in [(0, true), (1, false)] {
        let tid = setup(cpu, peer);
        assert!(admitted_block(20, SOURCE, &mut frame()) == IoBlockResult::Switched);
        assert_eq!(status(tid), ProcessState::Blocked);
        TICKS.store(101, Ordering::Relaxed);
        assert_eq!(wake_io_source(SOURCE), 1);
        resume(tid);
        assert!(admitted_block(20, SOURCE, &mut frame()) == IoBlockResult::Switched);
        assert_eq!(status(tid), ProcessState::Blocked);
        assert_eq!(slot(tid).io_deadline, 102);
        assert_eq!(slot(tid).sleep_deadline, 102);
        TICKS.store(102, Ordering::Relaxed);
        assert_eq!(wake_io_source(SOURCE), 1);
        resume(tid);
        assert!(admitted_block(20, SOURCE, &mut frame()) == IoBlockResult::TimedOut);
        assert_eq!(status(tid), ProcessState::Running);
        assert!(!slot(tid).io_wait);
        assert_eq!(slot(tid).io_deadline, 0);
    }
}

#[test]
fn infinite_deadline_and_timeout_rounding_are_preserved() {
    for (timeout, expected) in [(-1, 0), (0, 101), (1, 101), (10, 101), (11, 102)] {
        let tid = setup(1, false);
        let mut guard = begin_io_probe(SOURCE);
        wake_io_source(SOURCE);
        guard.handoff();
        assert!(block_current_for_io_on(timeout, SOURCE, &mut frame()) == IoBlockResult::Switched);
        assert_eq!(slot(tid).io_deadline, expected);
    }
    let tid = setup(0, false);
    for now in [100, 1000, u64::MAX - 1] {
        TICKS.store(now, Ordering::Relaxed);
        let mut guard = begin_io_probe(SOURCE);
        wake_io_source(SOURCE);
        guard.handoff();
        assert!(block_current_for_io_on(-1, SOURCE, &mut frame()) == IoBlockResult::Switched);
        assert_eq!(slot(tid).io_deadline, 0);
    }
}

#[test]
fn completion_clears_wait_deadline_and_probe_together() {
    let tid = setup(0, false);
    let mut old = begin_io_probe(SOURCE);
    wake_io_source(SOURCE);
    old.handoff();
    assert!(block_current_for_io_on(20, SOURCE, &mut frame()) == IoBlockResult::Switched);
    let current = begin_io_probe(SOURCE);
    wake_io_source(SOURCE);
    complete_io_wait();
    let saved = slot(tid);
    assert!(!saved.io_wait);
    assert_eq!(saved.io_source, WaitSource::Any);
    assert_eq!(saved.io_deadline, 0);
    assert_eq!(saved.sleep_deadline, 0);
    clean_probe(tid);
    drop(current);
}

#[test]
fn success_and_error_scope_drop_clear_admission_without_resetting_deadline() {
    for successful in [true, false] {
        let tid = setup(0, false);
        with_state(|state| {
            state.contexts[0].io_wait = true;
            state.contexts[0].io_source = SOURCE;
            state.contexts[0].io_deadline = 150;
        });
        let outcome: Result<usize, ()> = (|| {
            let _guard = begin_io_probe(SOURCE);
            wake_io_source(SOURCE);
            if successful { Ok(7) } else { Err(()) }
        })();
        assert_eq!(outcome.is_ok(), successful);
        clean_probe(tid);
        assert_eq!(slot(tid).io_deadline, 150, "guard cleanup must not restart syscall timeout");
        complete_io_wait();
    }
}

#[test]
fn handoff_cannot_clear_rearmed_same_tid() {
    let tid = setup(1, false);
    let mut old = begin_io_probe(SOURCE);
    old.handoff();
    assert!(block_current_for_io_on(-1, SOURCE, &mut frame()) == IoBlockResult::Switched);
    assert_eq!(wake_io_source(SOURCE), 1);
    resume(tid);
    let new = begin_io_probe(OTHER);
    wake_io_source(OTHER);
    drop(old);
    assert_eq!(slot(tid).io_probe_source, Some(OTHER), "old handed-off guard cleared new probe");
    assert!(slot(tid).io_probe_notified);
    drop(new);
    clean_probe(tid);
}

#[test]
fn guard_drop_uses_entry_tid_even_after_current_task_changes() {
    let tid = setup(0, true);
    let old = begin_io_probe(SOURCE);
    let peer = with_state(|state| state.contexts[1].pid);
    with_state(|state| state.table.activate_on(0, peer).unwrap());
    let current = begin_io_probe(OTHER);
    drop(old);
    clean_probe(tid);
    assert_eq!(slot(peer).io_probe_source, Some(OTHER));
    drop(current);
    clean_probe(peer);
}

#[test]
fn absent_or_mismatched_admission_fails_closed() {
    for cpu in [0, 1] {
        for requested in [WaitSource::Any, WaitSource::Network(71), WaitSource::Descriptor(72)] {
            let tid = setup(cpu, false);
            let mut guard = begin_io_probe(SOURCE);
            wake_io_source(SOURCE);
            guard.handoff();
            assert!(block_current_for_io_on(-1, requested, &mut frame()) == IoBlockResult::Failed);
            assert_eq!(status(tid), ProcessState::Running);
            assert!(!slot(tid).io_wait);
            clean_probe(tid);
            assert_eq!(WFI_COUNT.load(Ordering::Relaxed), 0);
        }
        let tid = setup(cpu, false);
        assert!(block_current_for_io_on(-1, SOURCE, &mut frame()) == IoBlockResult::Failed);
        assert_eq!(status(tid), ProcessState::Running);
    }
}

#[test]
fn missing_current_or_context_and_empty_cleanup_are_safe() {
    for missing_current in [true, false] {
        setup(1, false);
        with_state(|state| {
            if missing_current { let _ = state.table.block_current_on(1); }
            else { state.contexts[0] = ContextSlot::EMPTY; }
        });
        let mut guard = begin_io_probe(SOURCE);
        guard.handoff();
        assert!(block_current_for_io_on(-1, SOURCE, &mut frame()) == IoBlockResult::Failed);
        complete_io_wait();
        assert_eq!(wake_io_waiters(), 0);
    }
}

#[test]
fn no_eligible_cpu0_successor_restores_running_and_clears_wait() {
    let tid = setup(0, true);
    SELECT_NEXT.store(false, Ordering::Relaxed);
    assert!(admitted_block(20, SOURCE, &mut frame()) == IoBlockResult::Failed);
    assert_eq!(status(tid), ProcessState::Running);
    let current = slot(tid);
    assert!(!current.io_wait);
    assert_eq!(current.io_deadline, 0);
    assert_eq!(current.sleep_deadline, 0);
    clean_probe(tid);
}
