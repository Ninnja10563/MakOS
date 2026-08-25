use core::arch::asm;
use core::cell::UnsafeCell;
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use makos_elf64::{EM_AARCH64, ET_DYN, ET_EXEC, Elf64, PT_INTERP, PT_LOAD};
use makos_futex::{FutexKey, FutexTable, TaskId, WaitError, WaitHandle};
use makos_process_table::{ProcessTable, WaitResult};

const PAGE_SIZE: u64 = 4096;
const USER_STACK_PAGES: usize = 64;
const MAX_LOAD_SEGMENTS: usize = 4;
const MAX_PROCESSES: usize = 128;
const MAX_FUTEX_WAITERS: usize = MAX_PROCESSES;
const SYSV_MAX_ARGUMENTS: usize = 64;
const SYSV_MAX_ENVIRONMENT: usize = 64;
pub const SPAWN_ARGUMENTS_VERSION: u32 = 1;
pub const SPAWN_ARGUMENTS_BYTES: usize = 336;
const SPAWN_MAX_ARGUMENTS: usize = 8;
const SPAWN_MAX_ENVIRONMENT: usize = 8;
const SPAWN_DATA_BYTES: usize = 256;
const SURFACE_PRIORITY_TICKS: u64 = 1_000;
const DYNAMIC_APP_BASE: u64 = 0x1000_0000;
const DYNAMIC_LOADER_BASE: u64 = 0x2800_0000;

static INIT_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-init.elf"));
static SHELL_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-shell.elf"));
static BROWSER_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-browser.elf"));
static FILES_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-files.elf"));
static TEXTEDIT_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-textedit.elf"));
static PYTHON_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-python.elf"));
static STARTUP_PROBE_ELF: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-startup-probe.elf"));
static MUSL_PROBE_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-musl-probe.elf"));
static MUSL_CRT_PROBE_ELF: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-musl-crt-probe.elf"));
static MUSL_PTHREAD_PROBE_ELF: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-musl-pthread-probe.elf"));
static MUSL_DYNAMIC_LOADER_ELF: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-musl-loader.so"));
static MUSL_INTERP_PROBE_ELF: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-musl-interp-probe.elf"));
static MUSL_DYNAMIC_PROBE_ELF: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-musl-dynamic-probe.elf"));
static MUSL_DSO_PROBE_ELF: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-musl-dso-probe.elf"));
static MUSL_DLOPEN_PROBE_ELF: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-musl-dlopen-probe.elf"));
static MUSL_EXEC_CALLER_ELF: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-musl-exec-caller.elf"));
static TOOLCHAIN_ELF: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-toolchain.elf"));
static SMP_PROBE_ELF: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-smp-probe.elf"));
static SMP_IPC_PROBE_ELF: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-smp-ipc-probe.elf"));

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ProcessRole {
    None,
    Init,
    Worker,
    Shell,
    Browser,
    Files,
    TextEdit,
    Python,
    Nano,
    Native,
    SmpProbe,
    Firefox,
}

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
struct ContextSlot {
    pid: u64,
    group_pid: u64,
    role: ProcessRole,
    context: crate::arch::UserContext,
    clear_child_tid: u64,
    robust_list_head: u64,
    robust_list_length: u64,
    futex_wait: Option<WaitHandle>,
    sleep_deadline: u64,
    io_wait: bool,
    io_source: makos_readiness::WaitSource,
    io_deadline: u64,
    input_wait: bool,
    name: [u8; 16],
}

impl ContextSlot {
    const EMPTY: Self = Self {
        pid: 0,
        group_pid: 0,
        role: ProcessRole::None,
        context: crate::arch::UserContext::initial(0, 0, 0, 0),
        clear_child_tid: 0,
        robust_list_head: 0,
        robust_list_length: 0,
        futex_wait: None,
        sleep_deadline: 0,
        io_wait: false,
        io_source: makos_readiness::WaitSource::Any,
        io_deadline: 0,
        input_wait: false,
        name: [0; 16],
    };
}

struct SchedulerState {
    table: ProcessTable<MAX_PROCESSES>,
    futex: FutexTable<MAX_FUTEX_WAITERS>,
    contexts: [ContextSlot; MAX_PROCESSES],
    session_active: bool,
    self_test_session: bool,
    timer_switches: u64,
    timer_dispatches: [u64; MAX_PROCESSES],
    spawned_roots: [u64; MAX_PROCESSES],
    reaped_processes: u64,
    reclaimed_frames: usize,
    reaped_vm_regions: usize,
    reaped_vm_pages: usize,
}

impl SchedulerState {
    const fn new() -> Self {
        Self {
            table: ProcessTable::new(),
            futex: FutexTable::new(),
            contexts: [ContextSlot::EMPTY; MAX_PROCESSES],
            session_active: false,
            self_test_session: false,
            timer_switches: 0,
            timer_dispatches: [0; MAX_PROCESSES],
            spawned_roots: [0; MAX_PROCESSES],
            reaped_processes: 0,
            reclaimed_frames: 0,
            reaped_vm_regions: 0,
            reaped_vm_pages: 0,
        }
    }

    fn schedule_next_for_cpu(&mut self, cpu: usize) -> Option<makos_process_table::ProcessInfo> {
        let contexts = &self.contexts;
        self.table.schedule_next_where_on(cpu, |info| {
            let slot = contexts.iter().find(|slot| slot.pid == info.pid);
            if slot.is_some_and(|slot| slot.role == ProcessRole::SmpProbe) {
                cpu < SMP_PROBE_AFFINITY.len()
                    && SMP_PROBE_AFFINITY[cpu].load(Ordering::Acquire) == info.pid
            } else {
                cpu == 0
                    || slot.is_some_and(|slot| {
                        slot.pid != slot.group_pid && slot.role == ProcessRole::Firefox
                    })
            }
        })
    }
}

struct LockedProcesses {
    lock: AtomicBool,
    state: UnsafeCell<SchedulerState>,
}

unsafe impl Sync for LockedProcesses {}

static PROCESSES: LockedProcesses = LockedProcesses {
    lock: AtomicBool::new(false),
    state: UnsafeCell::new(SchedulerState::new()),
};
static SLEEP_BLOCK_REPORTED: AtomicBool = AtomicBool::new(false);
static SLEEP_WAKE_REPORTED: AtomicBool = AtomicBool::new(false);
static IO_BLOCK_REPORTED: AtomicBool = AtomicBool::new(false);
static IO_WAKE_REPORTED: AtomicBool = AtomicBool::new(false);
static IDLE_IO_REPORTED: AtomicBool = AtomicBool::new(false);
static INPUT_BLOCK_REPORTED: AtomicBool = AtomicBool::new(false);
static IDLE_SLEEP_REPORTED: AtomicBool = AtomicBool::new(false);
static FIREFOX_INPUT_WATCHER_TID: AtomicU64 = AtomicU64::new(0);
static SURFACE_PRIORITY_TID: AtomicU64 = AtomicU64::new(0);
static SURFACE_PRIORITY_DEADLINE: AtomicU64 = AtomicU64::new(0);
static SURFACE_MAIN_HANDOFF_PENDING: AtomicBool = AtomicBool::new(false);
static SURFACE_MAIN_HANDOFF_REPORTED: AtomicBool = AtomicBool::new(false);
static SURFACE_PRIORITY_REPORTED: AtomicBool = AtomicBool::new(false);
static THREAD_CREATE_TRACES: AtomicU64 = AtomicU64::new(0);
static THREAD_EXIT_TRACES: AtomicU64 = AtomicU64::new(0);
static SMP_PROBE_ACTIVE_MASK: AtomicU64 = AtomicU64::new(0);
static SMP_PROBE_PEAK_MASK: AtomicU64 = AtomicU64::new(0);
static SMP_PROBE_TIDS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static SMP_PROBE_RELEASE: AtomicBool = AtomicBool::new(false);
static SMP_PROBE_IDLE_MASK: AtomicU64 = AtomicU64::new(0);
static SMP_PROBE_RESUME_MASK: AtomicU64 = AtomicU64::new(0);
static SMP_PROBE_FUTEX_IDLE_MASK: AtomicU64 = AtomicU64::new(0);
static SMP_PROBE_FUTEX_RESUME_MASK: AtomicU64 = AtomicU64::new(0);
static SMP_PROBE_AFFINITY: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static SMP_PROBE_IO_IDLE_MASK: AtomicU64 = AtomicU64::new(0);
static SMP_PROBE_IO_RESUME_MASK: AtomicU64 = AtomicU64::new(0);
static SMP_PROBE_IPC_IDLE_MASK: AtomicU64 = AtomicU64::new(0);
static SMP_PROBE_IPC_RESUME_MASK: AtomicU64 = AtomicU64::new(0);
const THREAD_TRACE_LIMIT: u64 = 8;

#[inline]
fn scheduler_cpu() -> usize {
    crate::arch::cpu_index()
}

#[inline]
fn notify_idle_cpus() {
    // Scheduler state unlock publishes with Release before this DSB/SEV.
    // Closed AP gate currently turns this into a harmless wake/recheck.
    unsafe { asm!("dsb ish", "sev", options(nostack)) };
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum IoBlockResult {
    Switched,
    TimedOut,
    Failed,
}

enum FutexBlockResult {
    Context(crate::arch::UserContext),
    SecondaryIdle,
    BspIdle(u64),
}

#[derive(Clone, Copy)]
pub struct ProcessReport {
    pub exit_status: u64,
    pub reclaimed_frames: usize,
}

#[derive(Clone, Copy)]
pub struct RuntimeStats {
    pub live: usize,
    pub runnable: usize,
    pub blocked: usize,
    pub zombies: usize,
    pub current_pid: u64,
    pub timer_switches: u64,
}

pub fn runtime_stats() -> RuntimeStats {
    with_state(|state| {
        let (live, runnable, zombies) = state.table.counts();
        RuntimeStats {
            live,
            runnable,
            blocked: live.saturating_sub(runnable),
            zombies,
            current_pid: state.table.current_pid_on(scheduler_cpu()).unwrap_or(0),
            timer_switches: state.timer_switches,
        }
    })
}

pub fn report_runtime_tasks() {
    with_state(|state| {
        for (index, slot) in state
            .contexts
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.pid != 0)
        {
            let Some(info) = state.table.get(slot.pid) else {
                continue;
            };
            let role = match slot.role {
                ProcessRole::None => "none",
                ProcessRole::Init => "init",
                ProcessRole::Worker => "worker",
                ProcessRole::Shell => "shell",
                ProcessRole::Browser => "browser",
                ProcessRole::Files => "files",
                ProcessRole::TextEdit => "textedit",
                ProcessRole::Python => "python",
                ProcessRole::Nano => "nano",
                ProcessRole::Native => "native",
                ProcessRole::SmpProbe => "smp-probe",
                ProcessRole::Firefox => "firefox",
            };
            let task_state = match info.state {
                makos_process_table::ProcessState::Empty => "empty",
                makos_process_table::ProcessState::Ready => "ready",
                makos_process_table::ProcessState::Running => "running",
                makos_process_table::ProcessState::Blocked => "blocked",
                makos_process_table::ProcessState::Zombie => "zombie",
            };
            let futex_address = slot
                .futex_wait
                .and_then(|handle| state.futex.key(handle).ok())
                .map_or(0, |key| key.address);
            let resident_pages = crate::arch::user_resident_pages(slot.context.ttbr0).unwrap_or(0);
            crate::serial_println!(
                "MAKOS_TASK tid={} pid={} role={} state={} pc={:#x} futex={} futex_address={:#x} io={} input={} sleep_deadline={} dispatches={} resident_pages={} resident_kib={}",
                slot.pid,
                slot.group_pid,
                role,
                task_state,
                slot.context.elr,
                slot.futex_wait.is_some(),
                futex_address,
                slot.io_wait,
                slot.input_wait,
                slot.sleep_deadline,
                state.timer_dispatches[index],
                resident_pages,
                resident_pages.saturating_mul(4),
            );
        }
    });
}

pub fn run_init_self_test() -> ProcessReport {
    let free_before = crate::mm::free_frames();
    reset_scheduler();
    with_state(|state| state.self_test_session = true);
    let (pid, process) = spawn_process(0, INIT_ELF, 0, ProcessRole::Init)
        .unwrap_or_else(|| crate::fatal("AArch64 init spawn failed"));
    crate::serial_println!(
        "process arch=aarch64 pid={} elf=ok entry={:#x} stack={:#x} el=0 ttbr0={:#x}",
        pid,
        process.entry,
        crate::arch::USER_STACK_TOP,
        process.root,
    );
    let exit_status = run_ready_session();
    let (
        reclaimed_frames,
        reaped_processes,
        timer_switches,
        first_dispatches,
        second_dispatches,
        isolated,
        reaped_vm_regions,
        reaped_vm_pages,
    ) = with_state(|state| {
        (
            state.reclaimed_frames,
            state.reaped_processes,
            state.timer_switches,
            state.timer_dispatches[0],
            state.timer_dispatches[1],
            state.spawned_roots[0] != 0
                && state.spawned_roots[1] != 0
                && state.spawned_roots[0] != state.spawned_roots[1],
            state.reaped_vm_regions,
            state.reaped_vm_pages,
        )
    });
    if exit_status != 42
        || timer_switches < 4
        || reaped_processes != 2
        || first_dispatches == 0
        || second_dispatches == 0
        || !isolated
        || reaped_vm_regions != 2
        || reaped_vm_pages != 2
        || crate::mm::free_frames() != free_before
    {
        crate::fatal("AArch64 init exit/reclaim self-test failed");
    }
    crate::serial_println!(
        "MAKOS_AARCH64_SCHEDULER_OK processes=2 timer_preemptions={} dispatches={},{} context=x0-x30,elr,spsr,sp_el0,ttbr0,tpidr_el0,q0-q31,fpcr,fpsr isolated_ttbr0=1 spawn=1 concurrent=1 patterns=distinct exit=1 wait=1 reap=1 free_balance=1",
        timer_switches,
        first_dispatches,
        second_dispatches,
    );
    crate::serial_println!(
        "MAKOS_AARCH64_PROCESS_OK pid={} exit={} scheduler=saved-context address_space=isolated lifecycle=ready,running,zombie,reaped reclaimed_frames={} free_balance=1",
        pid,
        exit_status,
        reclaimed_frames,
    );
    crate::serial_println!(
        "MAKOS_AARCH64_VM_REAP_OK processes=2 regions={} pages={} cleanup=metadata,leaf,table free_balance=1",
        reaped_vm_regions,
        reaped_vm_pages,
    );
    reset_scheduler();
    ProcessReport {
        exit_status,
        reclaimed_frames,
    }
}

pub fn run_smp_userspace_self_test() {
    const PROBE_COUNT: usize = 4;
    let free_before = crate::mm::free_frames();
    reset_scheduler();
    SMP_PROBE_ACTIVE_MASK.store(0, Ordering::Release);
    SMP_PROBE_PEAK_MASK.store(0, Ordering::Release);
    SMP_PROBE_RELEASE.store(false, Ordering::Release);
    SMP_PROBE_IDLE_MASK.store(0, Ordering::Release);
    SMP_PROBE_RESUME_MASK.store(0, Ordering::Release);
    SMP_PROBE_FUTEX_IDLE_MASK.store(0, Ordering::Release);
    SMP_PROBE_FUTEX_RESUME_MASK.store(0, Ordering::Release);
    SMP_PROBE_IO_IDLE_MASK.store(0, Ordering::Release);
    SMP_PROBE_IO_RESUME_MASK.store(0, Ordering::Release);
    for tid in &SMP_PROBE_AFFINITY {
        tid.store(0, Ordering::Release);
    }
    for tid in &SMP_PROBE_TIDS {
        tid.store(0, Ordering::Release);
    }
    let mut pids = [0u64; PROBE_COUNT];
    for (index, pid) in pids.iter_mut().enumerate() {
        *pid = spawn_process(
            0,
            SMP_PROBE_ELF,
            index as u64,
            ProcessRole::SmpProbe,
        )
        .unwrap_or_else(|| crate::fatal("AArch64 SMP probe spawn failed"))
        .0;
    }
    for (cpu, pid) in pids.iter().copied().enumerate() {
        SMP_PROBE_AFFINITY[cpu].store(pid, Ordering::Release);
    }

    crate::arch::enable_smp_probe_scheduler();
    let dispatch_deadline = crate::arch::counter_deadline_millis(2_000);
    while SMP_PROBE_ACTIVE_MASK.load(Ordering::Acquire) & 0b1110 != 0b1110 {
        if crate::arch::counter_deadline_expired(dispatch_deadline) {
            crate::fatal("AArch64 SMP probe AP dispatch timeout");
        }
        core::hint::spin_loop();
    }

    let (pid, context) = with_state(|state| {
        let process = state
            .schedule_next_for_cpu(0)
            .unwrap_or_else(|| crate::fatal("AArch64 SMP probe BSP run queue empty"));
        let context = state
            .contexts
            .iter()
            .find(|slot| slot.pid == process.pid)
            .unwrap_or_else(|| crate::fatal("AArch64 SMP probe BSP context absent"))
            .context;
        (process.pid, context)
    });
    smp_probe_enter(pid);
    // APs have selected distinct contexts and wait immediately before their
    // EL0 transition. Release all four PEs together so the overlap proof is
    // deterministic on both fast HVF and constrained TCG hosts.
    SMP_PROBE_RELEASE.store(true, Ordering::Release);
    unsafe { asm!("dsb ish", "sev", options(nostack)) };
    crate::arch::switch_address_space(context.ttbr0);
    let _ = crate::arch::enter_user_context(&context);
    crate::arch::switch_address_space(crate::arch::kernel_root());
    smp_probe_leave();

    let completion_deadline = crate::arch::counter_deadline_millis(20_000);
    loop {
        let complete = with_state(|state| {
            pids.iter().all(|pid| {
                state.table.get(*pid).is_some_and(|info| {
                    info.state == makos_process_table::ProcessState::Zombie
                })
            })
        });
        if complete && SMP_PROBE_ACTIVE_MASK.load(Ordering::Acquire) == 0 {
            break;
        }
        if crate::arch::counter_deadline_expired(completion_deadline) {
            crate::fatal("AArch64 SMP probe completion timeout");
        }
        core::hint::spin_loop();
    }
    crate::arch::disable_smp_probe_scheduler();

    let mut statuses = [0u64; PROBE_COUNT];
    for (index, pid) in pids.iter().copied().enumerate() {
        let (resource, status) = with_state(|state| {
            let WaitResult::Reaped {
                resource,
                exit_status,
                ..
            } = state.table.wait(0, pid)
            else {
                crate::fatal("AArch64 SMP probe reap failed");
            };
            if let Some(slot) = state.contexts.iter_mut().find(|slot| slot.pid == pid) {
                *slot = ContextSlot::EMPTY;
            }
            (resource, exit_status)
        });
        statuses[index] = status;
        cleanup_reaped(pid, resource, status);
    }
    let tids = [
        SMP_PROBE_TIDS[0].load(Ordering::Acquire),
        SMP_PROBE_TIDS[1].load(Ordering::Acquire),
        SMP_PROBE_TIDS[2].load(Ordering::Acquire),
        SMP_PROBE_TIDS[3].load(Ordering::Acquire),
    ];
    let peak = SMP_PROBE_PEAK_MASK.load(Ordering::Acquire);
    let idle = SMP_PROBE_IDLE_MASK.load(Ordering::Acquire);
    let resumed = SMP_PROBE_RESUME_MASK.load(Ordering::Acquire);
    let futex_idle = SMP_PROBE_FUTEX_IDLE_MASK.load(Ordering::Acquire);
    let futex_resumed = SMP_PROBE_FUTEX_RESUME_MASK.load(Ordering::Acquire);
    let io_idle = SMP_PROBE_IO_IDLE_MASK.load(Ordering::Acquire);
    let io_resumed = SMP_PROBE_IO_RESUME_MASK.load(Ordering::Acquire);
    let free_balance = crate::mm::free_frames() == free_before;
    if statuses != [40, 41, 42, 43]
        || peak != 0b1111
        || idle & 0b1110 != 0b1110
        || resumed & 0b1110 != 0b1110
        || futex_idle & 0b1110 != 0b1110
        || futex_resumed & 0b1110 != 0b1110
        || io_idle & 0b1110 != 0b1110
        || io_resumed & 0b1110 != 0b1110
        || tids.contains(&0)
        || tids
            .iter()
            .enumerate()
            .any(|(index, tid)| tids[..index].contains(tid))
        || !free_balance
    {
        crate::serial_println!(
            "MAKOS_AARCH64_SMP_PROBE_DIAGNOSTIC peak={:#x} idle={:#x} resumed={:#x} io_idle={:#x} io_resumed={:#x} futex_idle={:#x} futex_resumed={:#x} statuses={},{},{},{} tids={},{},{},{} free_balance={}",
            peak,
            idle,
            resumed,
            io_idle,
            io_resumed,
            futex_idle,
            futex_resumed,
            statuses[0],
            statuses[1],
            statuses[2],
            statuses[3],
            tids[0],
            tids[1],
            tids[2],
            tids[3],
            u8::from(free_balance),
        );
        crate::fatal("AArch64 SMP userspace proof failed");
    }
    crate::serial_println!(
        "MAKOS_AARCH64_SMP_USER_OK cpus=4 tids={},{},{},{} overlap_mask={:#x} el=0 timer_ppi=per-cpu ap_block_idle=sleep-until wake=timer idle_mask={:#x} resume_mask={:#x} ap_io_idle=poll-timeout wake=timer io_idle_mask={:#x} io_resume_mask={:#x} ap_futex_idle=timeout wake=timer futex_idle_mask={:#x} futex_resume_mask={:#x} statuses=40,41,42,43 scheduler_scope=boot-probe desktop_gate=closed free_balance=1",
        tids[0], tids[1], tids[2], tids[3], peak, idle, resumed, io_idle, io_resumed, futex_idle, futex_resumed,
    );
    reset_scheduler();
}

pub fn run_smp_ipc_self_test() {
    let free_before = crate::mm::free_frames();
    reset_scheduler();
    SMP_PROBE_ACTIVE_MASK.store(0, Ordering::Release);
    SMP_PROBE_RELEASE.store(true, Ordering::Release);
    SMP_PROBE_IPC_IDLE_MASK.store(0, Ordering::Release);
    SMP_PROBE_IPC_RESUME_MASK.store(0, Ordering::Release);
    for tid in &SMP_PROBE_AFFINITY {
        tid.store(0, Ordering::Release);
    }

    let leader = spawn_process(0, SMP_IPC_PROBE_ELF, 0, ProcessRole::SmpProbe)
        .unwrap_or_else(|| crate::fatal("AArch64 SMP IPC probe spawn failed"))
        .0;
    // Force the event waiter onto AP1. Its clone remains Ready because only
    // the leader TID is eligible there; CPU0 admits the clone after observing
    // its fully published context.
    SMP_PROBE_AFFINITY[1].store(leader, Ordering::Release);
    crate::arch::enable_smp_probe_scheduler();

    let clone_deadline = crate::arch::counter_deadline_millis(20_000);
    let child = loop {
        let child = with_state(|state| {
            state
                .contexts
                .iter()
                .find(|slot| slot.pid != 0 && slot.pid != leader && slot.group_pid == leader)
                .map(|slot| slot.pid)
        });
        if let Some(child) = child
            && SMP_PROBE_IPC_IDLE_MASK.load(Ordering::Acquire) & 0b0010 != 0
        {
            break child;
        }
        if crate::arch::counter_deadline_expired(clone_deadline) {
            let (leader_info, child_info, cpu0, cpu1) = with_state(|state| {
                (
                    state.table.get(leader),
                    child.and_then(|tid| state.table.get(tid)),
                    state.table.current_pid_on(0),
                    state.table.current_pid_on(1),
                )
            });
            crate::serial_println!(
                "MAKOS_AARCH64_SMP_IPC_DIAGNOSTIC leader={:?} child={:?} cpu0={:?} cpu1={:?} idle_mask={:#x} resume_mask={:#x}",
                leader_info,
                child_info,
                cpu0,
                cpu1,
                SMP_PROBE_IPC_IDLE_MASK.load(Ordering::Acquire),
                SMP_PROBE_IPC_RESUME_MASK.load(Ordering::Acquire),
            );
            crate::fatal("AArch64 SMP IPC clone publication timeout");
        }
        core::hint::spin_loop();
    };

    SMP_PROBE_AFFINITY[0].store(child, Ordering::Release);
    notify_idle_cpus();
    let context = with_state(|state| {
        let process = state
            .schedule_next_for_cpu(0)
            .unwrap_or_else(|| crate::fatal("AArch64 SMP IPC signaler absent"));
        if process.pid != child {
            crate::fatal("AArch64 SMP IPC signaler affinity violated");
        }
        state
            .contexts
            .iter()
            .find(|slot| slot.pid == child)
            .unwrap_or_else(|| crate::fatal("AArch64 SMP IPC signaler context absent"))
            .context
    });
    smp_probe_enter(child);
    crate::arch::switch_address_space(context.ttbr0);
    let child_status = crate::arch::enter_user_context(&context);
    crate::arch::switch_address_space(crate::arch::kernel_root());
    smp_probe_leave();
    if child_status != 0 {
        crate::fatal("AArch64 SMP IPC signaler status invalid");
    }

    let completion_deadline = crate::arch::counter_deadline_millis(20_000);
    loop {
        let complete = with_state(|state| {
            state
                .table
                .get(leader)
                .is_some_and(|info| info.state == makos_process_table::ProcessState::Zombie)
        });
        if complete && SMP_PROBE_ACTIVE_MASK.load(Ordering::Acquire) == 0 {
            break;
        }
        if crate::arch::counter_deadline_expired(completion_deadline) {
            crate::fatal("AArch64 SMP IPC completion timeout");
        }
        core::hint::spin_loop();
    }
    crate::arch::disable_smp_probe_scheduler();

    let (resource, status) = with_state(|state| {
        let WaitResult::Reaped {
            resource,
            exit_status,
            ..
        } = state.table.wait(0, leader)
        else {
            crate::fatal("AArch64 SMP IPC leader reap failed");
        };
        if let Some(slot) = state.contexts.iter_mut().find(|slot| slot.pid == leader) {
            *slot = ContextSlot::EMPTY;
        }
        (resource, exit_status)
    });
    cleanup_reaped(leader, resource, status);
    let idle = SMP_PROBE_IPC_IDLE_MASK.load(Ordering::Acquire);
    let resumed = SMP_PROBE_IPC_RESUME_MASK.load(Ordering::Acquire);
    if status != 44
        || idle & 0b0010 == 0
        || resumed & 0b0010 == 0
        || crate::mm::free_frames() != free_before
    {
        crate::fatal("AArch64 SMP IPC userspace proof failed");
    }
    crate::serial_println!(
        "MAKOS_AARCH64_SMP_IPC_OK waiter_cpu=1 signaler_cpu=0 event=auto-reset cross-task=1 ipc_idle_mask={:#x} ipc_resume_mask={:#x} child_exit=thread-return parent_exit=44 free_balance=1 scheduler_scope=boot-probe desktop_gate=closed",
        idle,
        resumed,
    );
    reset_scheduler();
}

pub fn run_desktop_shell() -> ! {
    reset_scheduler();
    let (pid, process) = spawn_process(0, SHELL_ELF, 0, ProcessRole::Shell)
        .unwrap_or_else(|| crate::fatal("AArch64 shell spawn failed"));
    crate::serial_println!(
        "MAKOS_AARCH64_SHELL_PROCESS_OK pid={} elf=1 el=0 entry={:#x} ttbr0={:#x} syscalls=input,auth,graphics,vfs process_table=owned",
        pid,
        process.entry,
        process.root,
    );
    let status = run_ready_session();
    let reclaimed = with_state(|state| state.reclaimed_frames);
    crate::serial_println!(
        "MAKOS_AARCH64_SHELL_EXIT pid={} status={} reclaimed_frames={}",
        pid,
        status,
        reclaimed,
    );
    crate::fatal("AArch64 desktop shell exited")
}

pub fn current_pid() -> u64 {
    with_state(|state| {
        let Some(tid) = state.table.current_pid_on(scheduler_cpu()) else {
            return 0;
        };
        state
            .contexts
            .iter()
            .find(|slot| slot.pid == tid)
            .map_or(0, |slot| slot.group_pid)
    })
}

pub fn current_parent_pid() -> u64 {
    with_state(|state| {
        let tid = state.table.current_pid_on(scheduler_cpu())?;
        let group_pid = state
            .contexts
            .iter()
            .find(|slot| slot.pid == tid)
            .map(|slot| slot.group_pid)?;
        state.table.get(group_pid).map(|process| process.parent_pid)
    })
    .unwrap_or(0)
}

pub fn current_tid() -> u64 {
    with_state(|state| state.table.current_pid_on(scheduler_cpu()).unwrap_or(0))
}

pub fn set_current_thread_name(name: &[u8]) -> bool {
    if name.len() > 15 {
        return false;
    }
    with_state(|state| {
        let Some(tid) = state.table.current_pid_on(scheduler_cpu()) else {
            return false;
        };
        let Some(slot) = state.contexts.iter_mut().find(|slot| slot.pid == tid) else {
            return false;
        };
        slot.name = [0; 16];
        slot.name[..name.len()].copy_from_slice(name);
        true
    })
}

pub fn set_task_scheduler(tid: u64, policy: u64, priority: u64) -> bool {
    if policy != 0 || priority != 0 {
        return false;
    }
    with_state(|state| {
        let Some(current_tid) = state.table.current_pid_on(scheduler_cpu()) else {
            return false;
        };
        let Some(group_pid) = state
            .contexts
            .iter()
            .find(|slot| slot.pid == current_tid)
            .map(|slot| slot.group_pid)
        else {
            return false;
        };
        state
            .contexts
            .iter()
            .any(|slot| slot.pid == tid && slot.group_pid == group_pid)
    })
}

pub fn wake_task(group_pid: u64, tid: u64) -> bool {
    let woken = with_state(|state| {
        if !state
            .contexts
            .iter()
            .any(|slot| slot.pid == tid && slot.group_pid == group_pid)
        {
            return false;
        }
        state.table.wake(tid)
    });
    if woken {
        notify_idle_cpus();
    }
    woken
}

pub fn block_current_for_ipc(frame: &mut crate::arch::ExceptionFrame) -> bool {
    let captured = crate::arch::UserContext::capture(frame);
    let next = with_state(|state| {
        let cpu = scheduler_cpu();
        let tid = state.table.current_pid_on(cpu)?;
        let index = state.contexts.iter().position(|slot| slot.pid == tid)?;
        state.contexts[index].context = captured;
        if state.table.block_current_on(cpu) != Some(tid) {
            return None;
        }
        let Some(next) = state.schedule_next_for_cpu(cpu) else {
            if cpu != 0 && state.contexts[index].role == ProcessRole::SmpProbe {
                SMP_PROBE_IPC_IDLE_MASK.fetch_or(1u64 << cpu, Ordering::AcqRel);
                return Some(None);
            }
            let _ = state.table.wake(tid);
            let _ = state.table.activate_on(cpu, tid);
            return None;
        };
        state
            .contexts
            .iter()
            .find(|slot| slot.pid == next.pid)
            .map(|slot| Some(slot.context))
    });
    match next {
        Some(Some(context)) => {
            context.restore(frame);
            crate::arch::switch_address_space(context.ttbr0);
            true
        }
        Some(None) => {
            crate::arch::return_to_kernel(frame, 0);
            true
        }
        None => false,
    }
}

/// Suspend current EL0 task until pipe readiness changes or timeout expires.
/// Saved ELR points back at `svc`, making kernel retry original operation after
/// wake without exposing an intermediate EAGAIN to blocking userspace.
pub(crate) fn block_current_for_io(
    timeout_milliseconds: i64,
    frame: &mut crate::arch::ExceptionFrame,
) -> IoBlockResult {
    block_current_for_io_on(
        timeout_milliseconds,
        makos_readiness::WaitSource::Any,
        frame,
    )
}

pub(crate) fn block_current_for_io_on(
    timeout_milliseconds: i64,
    source: makos_readiness::WaitSource,
    frame: &mut crate::arch::ExceptionFrame,
) -> IoBlockResult {
    let now = crate::arch::monotonic_ticks();
    let mut captured = crate::arch::UserContext::capture(frame);
    captured.elr = captured.elr.saturating_sub(4);
    if runtime_stats().runnable <= 1 {
        // SVC entry keeps IRQs masked through publishing io_wait. A device or
        // timer becoming pending in that window makes the following WFI exit
        // immediately, so readiness cannot be lost between userspace's failed
        // operation and this wait. AArch64 currently schedules one boot CPU;
        // no remote CPU can race runnable count before IRQs are enabled.
        let idle = with_state(|state| {
            let tid = state
                .table
                .current_pid_on(scheduler_cpu())
                .ok_or(IoBlockResult::Failed)?;
            let index = state
                .contexts
                .iter()
                .position(|slot| slot.pid == tid)
                .ok_or(IoBlockResult::Failed)?;
            if !state.contexts[index].io_wait {
                state.contexts[index].io_wait = true;
                state.contexts[index].io_source = source;
                state.contexts[index].io_deadline = if timeout_milliseconds < 0 {
                    0
                } else {
                    let ticks = (timeout_milliseconds as u64).saturating_add(9) / 10;
                    now.saturating_add(ticks.max(1))
                };
            }
            let deadline = state.contexts[index].io_deadline;
            if deadline != 0 && deadline <= now {
                state.contexts[index].io_wait = false;
                state.contexts[index].io_source = makos_readiness::WaitSource::Any;
                state.contexts[index].io_deadline = 0;
                state.contexts[index].sleep_deadline = 0;
                return Err(IoBlockResult::TimedOut);
            }
            state.contexts[index].context = captured;
            Ok((tid, deadline))
        });
        let (tid, deadline) = match idle {
            Ok(value) => value,
            Err(result) => return result,
        };
        if !IDLE_IO_REPORTED.swap(true, Ordering::AcqRel) {
            crate::serial_println!(
                "MAKOS_AARCH64_IDLE_IO_OK tid={} scheduler=wfi last_runnable=1 deadline={} retry=svc busy_spin=0",
                tid,
                deadline,
            );
        }
        // Current task stays Running; sleep host CPU until next interrupt,
        // pump timer-polled devices, then retry original syscall. Persistent
        // io_deadline prevents relative timeouts from restarting each retry.
        crate::arch::enable_interrupts();
        unsafe { asm!("wfi", options(nomem, nostack)) };
        crate::arch::disable_interrupts();
        crate::aarch64_socket::pump();
        crate::aarch64_virtio_input::poll();
        crate::graphics::service_deferred_actions();
        captured.restore(frame);
        return IoBlockResult::Switched;
    }
    let switched = with_state(|state| {
        let tid = state
            .table
            .current_pid_on(scheduler_cpu())
            .ok_or(IoBlockResult::Failed)?;
        let index = state
            .contexts
            .iter()
            .position(|slot| slot.pid == tid)
            .ok_or(IoBlockResult::Failed)?;
        if !state.contexts[index].io_wait {
            state.contexts[index].io_wait = true;
            state.contexts[index].io_source = source;
            state.contexts[index].io_deadline = if timeout_milliseconds < 0 {
                0
            } else {
                let ticks = (timeout_milliseconds as u64).saturating_add(9) / 10;
                now.saturating_add(ticks.max(1))
            };
        }
        let deadline = state.contexts[index].io_deadline;
        if deadline != 0 && deadline <= now {
            state.contexts[index].io_wait = false;
            state.contexts[index].io_source = makos_readiness::WaitSource::Any;
            state.contexts[index].io_deadline = 0;
            state.contexts[index].sleep_deadline = 0;
            return Err(IoBlockResult::TimedOut);
        }
        state.contexts[index].context = captured;
        state.contexts[index].sleep_deadline = deadline;
        if state.table.block_current_on(scheduler_cpu()) != Some(tid) {
            state.contexts[index].io_wait = false;
            state.contexts[index].io_source = makos_readiness::WaitSource::Any;
            state.contexts[index].io_deadline = 0;
            state.contexts[index].sleep_deadline = 0;
            return Err(IoBlockResult::Failed);
        }
        let cpu = scheduler_cpu();
        let Some(next) = state.schedule_next_for_cpu(cpu) else {
            if cpu != 0 && state.contexts[index].role == ProcessRole::SmpProbe {
                SMP_PROBE_IO_IDLE_MASK.fetch_or(1u64 << cpu, Ordering::AcqRel);
                return Ok((tid, deadline, None));
            }
            state.contexts[index].io_wait = false;
            state.contexts[index].io_source = makos_readiness::WaitSource::Any;
            state.contexts[index].io_deadline = 0;
            state.contexts[index].sleep_deadline = 0;
            let _ = state.table.wake(tid);
            let _ = state.table.activate_on(scheduler_cpu(), tid);
            return Err(IoBlockResult::Failed);
        };
        let context = state
            .contexts
            .iter()
            .find(|slot| slot.pid == next.pid)
            .map(|slot| slot.context)
            .ok_or(IoBlockResult::Failed)?;
        Ok((tid, deadline, Some(context)))
    });
    match switched {
        Ok((tid, deadline, context)) => {
            if !IO_BLOCK_REPORTED.swap(true, Ordering::AcqRel) {
                crate::serial_println!(
                    "MAKOS_AARCH64_IO_BLOCK_OK tid={} deadline={} scheduler=blocked retry=svc",
                    tid,
                    deadline,
                );
            }
            if let Some(context) = context {
                context.restore(frame);
                crate::arch::switch_address_space(context.ttbr0);
            } else {
                crate::arch::return_to_kernel(frame, 0);
            }
            IoBlockResult::Switched
        }
        Err(result) => result,
    }
}

pub(crate) fn complete_io_wait() {
    with_state(|state| {
        let Some(tid) = state.table.current_pid_on(scheduler_cpu()) else {
            return;
        };
        let Some(slot) = state.contexts.iter_mut().find(|slot| slot.pid == tid) else {
            return;
        };
        slot.io_wait = false;
        slot.io_source = makos_readiness::WaitSource::Any;
        slot.io_deadline = 0;
        slot.sleep_deadline = 0;
    });
}

/// Wake only direct waiters for the changed object plus wildcard poll/epoll
/// waiters. The process table bounds both registration and wake work.
pub(crate) fn wake_io_source(source: makos_readiness::WaitSource) -> usize {
    let count = with_state(|state| {
        let mut count = 0usize;
        for index in 0..state.contexts.len() {
            if !state.contexts[index].io_wait || !state.contexts[index].io_source.woken_by(source) {
                continue;
            }
            let tid = state.contexts[index].pid;
            if state.table.wake(tid) {
                state.contexts[index].sleep_deadline = 0;
                count += 1;
            }
        }
        count
    });
    if count != 0 && !IO_WAKE_REPORTED.swap(true, Ordering::AcqRel) {
        crate::serial_println!(
            "MAKOS_AARCH64_IO_WAKE_OK tasks={} source=pipe-state retry=svc",
            count,
        );
    }
    if count != 0 {
        notify_idle_cpus();
    }
    count
}

/// Signals and readiness classes without a stable object key retain global
/// wake behavior.
pub(crate) fn wake_io_waiters() -> usize {
    wake_io_source(makos_readiness::WaitSource::Any)
}

/// Suspend current task until a keyboard or surface event arrives. Input has
/// its own wait class so pointer motion never wakes Firefox pipe/socket/poll
/// waiters and causes a thundering-herd retry storm.
pub(crate) fn block_current_for_input(frame: &mut crate::arch::ExceptionFrame) -> IoBlockResult {
    let mut captured = crate::arch::UserContext::capture(frame);
    captured.elr = captured.elr.saturating_sub(4);
    let switched = with_state(|state| {
        let tid = state
            .table
            .current_pid_on(scheduler_cpu())
            .ok_or(IoBlockResult::Failed)?;
        let index = state
            .contexts
            .iter()
            .position(|slot| slot.pid == tid)
            .ok_or(IoBlockResult::Failed)?;
        if state.contexts[index].role == ProcessRole::Firefox {
            FIREFOX_INPUT_WATCHER_TID.store(tid, Ordering::Release);
        }
        state.contexts[index].input_wait = true;
        state.contexts[index].context = captured;
        if state.table.block_current_on(scheduler_cpu()) != Some(tid) {
            state.contexts[index].input_wait = false;
            return Err(IoBlockResult::Failed);
        }
        let Some(next) = state.schedule_next_for_cpu(scheduler_cpu()) else {
            state.contexts[index].input_wait = false;
            let _ = state.table.wake(tid);
            let _ = state.table.activate_on(scheduler_cpu(), tid);
            return Err(IoBlockResult::Failed);
        };
        let context = state
            .contexts
            .iter()
            .find(|slot| slot.pid == next.pid)
            .map(|slot| slot.context)
            .ok_or(IoBlockResult::Failed)?;
        Ok((tid, context))
    });
    match switched {
        Ok((tid, context)) => {
            context.restore(frame);
            crate::arch::switch_address_space(context.ttbr0);
            if !INPUT_BLOCK_REPORTED.swap(true, Ordering::AcqRel) {
                crate::serial_println!(
                    "MAKOS_AARCH64_INPUT_BLOCK_OK tid={} scheduler=blocked retry=svc",
                    tid,
                );
            }
            IoBlockResult::Switched
        }
        Err(result) => result,
    }
}

pub(crate) fn complete_input_wait() {
    with_state(|state| {
        let Some(tid) = state.table.current_pid_on(scheduler_cpu()) else {
            return;
        };
        let Some(slot) = state.contexts.iter_mut().find(|slot| slot.pid == tid) else {
            return;
        };
        slot.input_wait = false;
    });
}

pub(crate) fn wake_input_waiters() -> usize {
    let count = with_state(|state| {
        let mut count = 0usize;
        for index in 0..state.contexts.len() {
            if !state.contexts[index].input_wait {
                continue;
            }
            if state.table.wake(state.contexts[index].pid) {
                count += 1;
            }
        }
        count
    });
    if count != 0 {
        notify_idle_cpus();
    }
    count
}

/// A queued surface key must first reach Firefox's dedicated native-event
/// watcher. `input_wait` stays set across Blocked -> Ready, so it identifies
/// both sleeping and newly woken watcher state. Fall back to process leader
/// only before watcher has registered its wait.
pub(crate) fn prioritize_firefox_surface_thread() -> bool {
    let tid = with_state(|state| {
        let input_waiter = state
            .contexts
            .iter()
            .find(|slot| slot.pid != 0 && slot.role == ProcessRole::Firefox && slot.input_wait)
            .map(|slot| slot.pid);
        input_waiter
            .or_else(|| {
                let watcher = FIREFOX_INPUT_WATCHER_TID.load(Ordering::Acquire);
                state
                    .contexts
                    .iter()
                    .find(|slot| slot.pid == watcher && slot.role == ProcessRole::Firefox)
                    .map(|slot| slot.pid)
            })
            .or_else(|| {
                state
                    .contexts
                    .iter()
                    .find(|slot| {
                        slot.pid != 0
                            && slot.pid == slot.group_pid
                            && slot.role == ProcessRole::Firefox
                    })
                    .map(|slot| slot.pid)
            })
            .unwrap_or(0)
    });
    set_surface_priority(tid)
}

/// The watcher has dequeued a key but has not posted its Gecko runnable yet.
/// Drop the watcher hint, then attach the bounded leader hint to the futex wake
/// that NS_DispatchToMainThread emits after enqueueing the runnable.
pub(crate) fn arm_firefox_process_leader_handoff() {
    SURFACE_PRIORITY_TID.store(0, Ordering::Release);
    SURFACE_PRIORITY_DEADLINE.store(0, Ordering::Release);
    let leader = with_state(|state| {
        state
            .contexts
            .iter()
            .find(|slot| {
                slot.pid != 0 && slot.pid == slot.group_pid && slot.role == ProcessRole::Firefox
            })
            .map(|slot| slot.pid)
            .unwrap_or(0)
    });
    let fallback_armed = set_surface_priority(leader);
    SURFACE_MAIN_HANDOFF_PENDING.store(true, Ordering::Release);
    if fallback_armed && !SURFACE_MAIN_HANDOFF_REPORTED.swap(true, Ordering::AcqRel) {
        crate::serial_println!(
            "MAKOS_AARCH64_SURFACE_MAIN_HANDOFF_OK tid={} source=watcher-dequeue-fallback bounded_ticks={}",
            leader,
            SURFACE_PRIORITY_TICKS,
        );
    }
}

fn set_surface_priority(tid: u64) -> bool {
    if tid == 0 {
        return false;
    }
    SURFACE_PRIORITY_TID.store(tid, Ordering::Release);
    SURFACE_PRIORITY_DEADLINE.store(
        crate::arch::monotonic_ticks().saturating_add(SURFACE_PRIORITY_TICKS),
        Ordering::Release,
    );
    true
}

pub fn sleep_until(deadline: u64, frame: &mut crate::arch::ExceptionFrame) {
    if deadline <= crate::arch::monotonic_ticks() {
        frame.registers[0] = 0;
        return;
    }
    frame.registers[0] = 0;
    if runtime_stats().runnable <= 1 {
        // No alternate EL0 task exists to run while this one sleeps. Blocking
        // then asking ProcessTable for a successor would fail and return
        // EAGAIN, turning libc nanosleep loops into 100% CPU busy-spins.
        // Remain in syscall context and let virtual timer IRQs wake EL1 until
        // deadline. Other runnable-task sleeps still use scheduler blocking.
        if !IDLE_SLEEP_REPORTED.swap(true, Ordering::AcqRel) {
            crate::serial_println!(
                "MAKOS_AARCH64_IDLE_SLEEP_OK scheduler=wfi last_runnable=1 wake=deadline-or-runnable busy_spin=0"
            );
        }
        loop {
            if crate::arch::monotonic_ticks() >= deadline {
                return;
            }
            if runtime_stats().runnable > 1 {
                break;
            }
            crate::arch::enable_interrupts();
            unsafe { asm!("wfi", options(nomem, nostack)) };
            crate::arch::disable_interrupts();
            // Virtio input/net currently use timer-polled bottom halves. Keep
            // them live during idle wait, then hand scheduler any task they
            // wake instead of monopolizing EL1 until this task's deadline.
            crate::aarch64_socket::pump();
            crate::aarch64_virtio_input::poll();
            crate::graphics::service_deferred_actions();
        }
    }
    let captured = crate::arch::UserContext::capture(frame);
    let switched = with_state(|state| {
        let cpu = scheduler_cpu();
        let tid = state.table.current_pid_on(scheduler_cpu()).ok_or(22u64)?;
        let index = state
            .contexts
            .iter()
            .position(|slot| slot.pid == tid)
            .ok_or(22u64)?;
        state.contexts[index].context = captured;
        state.contexts[index].sleep_deadline = deadline;
        if state.table.block_current_on(scheduler_cpu()) != Some(tid) {
            state.contexts[index].sleep_deadline = 0;
            return Err(22);
        }
        let Some(next) = state.schedule_next_for_cpu(scheduler_cpu()) else {
            if cpu != 0 && state.contexts[index].role == ProcessRole::SmpProbe {
                SMP_PROBE_IDLE_MASK.fetch_or(1u64 << cpu, Ordering::AcqRel);
                return Ok(None);
            }
            state.contexts[index].sleep_deadline = 0;
            let _ = state.table.wake(tid);
            let _ = state.table.activate_on(scheduler_cpu(), tid);
            return Err(11);
        };
        let context = state
            .contexts
            .iter()
            .find(|slot| slot.pid == next.pid)
            .map(|slot| slot.context)
            .ok_or(22u64)?;
        Ok(Some((tid, context)))
    });
    match switched {
        Ok(Some((tid, context))) => {
            if !SLEEP_BLOCK_REPORTED.swap(true, Ordering::AcqRel) {
                crate::serial_println!(
                    "MAKOS_AARCH64_SLEEP_BLOCK_OK tid={} deadline={} scheduler=blocked timer_wake=armed",
                    tid,
                    deadline,
                );
            }
            context.restore(frame);
            crate::arch::switch_address_space(context.ttbr0);
        }
        Ok(None) => crate::arch::return_to_kernel(frame, 0),
        Err(errno) => frame.registers[0] = negative_errno(errno),
    }
}

pub fn set_tid_address(address: u64) -> Option<u64> {
    with_state(|state| {
        let pid = state.table.current_pid_on(scheduler_cpu())?;
        let slot = state.contexts.iter_mut().find(|slot| slot.pid == pid)?;
        slot.clear_child_tid = address;
        Some(pid)
    })
}

pub fn clone_thread(
    flags: u64,
    stack: u64,
    parent_tid_address: u64,
    tls: u64,
    child_tid_address: u64,
    frame: &crate::arch::ExceptionFrame,
) -> Option<u64> {
    const REQUIRED_FLAGS: u64 = 0x0000_0100
        | 0x0000_0200
        | 0x0000_0400
        | 0x0000_0800
        | 0x0001_0000
        | 0x0004_0000
        | 0x0008_0000
        | 0x0010_0000
        | 0x0020_0000
        | 0x0040_0000;
    if flags != REQUIRED_FLAGS || stack & 15 != 0 || tls == 0 {
        return None;
    }
    let mut child_context = crate::arch::UserContext::capture(frame);
    child_context.registers[0] = 0;
    child_context.sp_el0 = stack;
    child_context.tpidr_el0 = tls;
    let tid = with_state(|state| {
        let parent_tid = state.table.current_pid_on(scheduler_cpu())?;
        let parent = state
            .contexts
            .iter()
            .find(|slot| slot.pid == parent_tid)
            .copied()?;
        let tid = state
            .table
            .spawn(parent.group_pid, parent.context.ttbr0)
            .ok()?;
        let slot = state.contexts.iter_mut().find(|slot| slot.pid == 0)?;
        *slot = ContextSlot {
            pid: tid,
            group_pid: parent.group_pid,
            role: parent.role,
            context: child_context,
            clear_child_tid: child_tid_address,
            robust_list_head: 0,
            robust_list_length: 0,
            futex_wait: None,
            sleep_deadline: 0,
            io_wait: false,
            io_source: makos_readiness::WaitSource::Any,
            io_deadline: 0,
            input_wait: false,
            name: parent.name,
        };
        state.session_active = true;
        Some((tid, parent_tid, parent.group_pid))
    })?;
    let (tid, parent_tid, group_pid) = tid;
    if !crate::aarch64_tty::register_thread(tid, group_pid, parent_tid) {
        crate::fatal("AArch64 TTY thread table full");
    }
    if parent_tid_address != 0 {
        unsafe { core::ptr::write_volatile(parent_tid_address as *mut u32, tid as u32) };
    }
    notify_idle_cpus();
    if THREAD_CREATE_TRACES.fetch_add(1, Ordering::Relaxed) < THREAD_TRACE_LIMIT {
        crate::serial_println!(
            "MAKOS_AARCH64_THREAD_CREATE_OK pid={} tid={} root={:#x} stack={:#x} tls={:#x} shared=vm,files,credentials signals=group",
            current_pid(),
            tid,
            child_context.ttbr0,
            stack,
            tls,
        );
    }
    Some(tid)
}

/// POSIX fork: clone calling thread into isolated process/address space.
/// Parent receives child PID; child resumes after SVC with x0=0.
pub fn fork_process(frame: &crate::arch::ExceptionFrame) -> Option<u64> {
    let (parent_tid, parent_pid, parent) = with_state(|state| {
        let parent_tid = state.table.current_pid_on(scheduler_cpu())?;
        let parent = state
            .contexts
            .iter()
            .find(|slot| slot.pid == parent_tid)
            .copied()?;
        Some((parent_tid, parent.group_pid, parent))
    })?;
    let (child_root, copied_pages) =
        crate::arch::clone_user_address_space_eager(parent.context.ttbr0)?;
    let mut child_context = crate::arch::UserContext::capture(frame);
    child_context.registers[0] = 0;
    child_context.ttbr0 = child_root;
    let child_pid = with_state(|state| {
        let child_pid = match state.table.spawn(parent_pid, child_root) {
            Ok(pid) => pid,
            Err(_) => return None,
        };
        let index = state.contexts.iter().position(|slot| slot.pid == 0)?;
        state.contexts[index] = ContextSlot {
            pid: child_pid,
            group_pid: child_pid,
            role: parent.role,
            context: child_context,
            clear_child_tid: 0,
            robust_list_head: 0,
            robust_list_length: 0,
            futex_wait: None,
            sleep_deadline: 0,
            io_wait: false,
            io_source: makos_readiness::WaitSource::Any,
            io_deadline: 0,
            input_wait: false,
            name: parent.name,
        };
        state.spawned_roots[index] = child_root;
        state.session_active = true;
        Some(child_pid)
    });
    let Some(child_pid) = child_pid else {
        let _ = crate::arch::destroy_user_address_space(child_root);
        return None;
    };
    if !crate::aarch64_vm::clone_process(parent_pid, child_pid, child_root)
        || !crate::aarch64_tty::fork_process(parent_pid, child_pid, parent_tid)
        || !crate::vfs::inherit_process(parent_pid, child_pid)
        || !crate::security::inherit_process_credentials(parent_pid, child_pid)
    {
        crate::fatal("AArch64 fork resource inheritance failed");
    }
    notify_idle_cpus();
    crate::serial_println!(
        "MAKOS_AARCH64_FORK_OK parent={} caller_tid={} child={} root={:#x} copied_pages={} vm=isolated files=shared-descriptions signals=inherited credentials=inherited",
        parent_pid,
        parent_tid,
        child_pid,
        child_root,
        copied_pages,
    );
    Some(child_pid)
}

pub fn futex(
    address: u64,
    operation: u32,
    value: u32,
    timeout_address: u64,
    requeue_address: u64,
    compare_value: u32,
    frame: &mut crate::arch::ExceptionFrame,
) {
    const FUTEX_WAIT: u32 = 0;
    const FUTEX_WAKE: u32 = 1;
    const FUTEX_REQUEUE: u32 = 3;
    const FUTEX_CMP_REQUEUE: u32 = 4;
    const FUTEX_PRIVATE_FLAG: u32 = 128;
    const EAGAIN: u64 = 11;
    const EINVAL: u64 = 22;
    const ETIMEDOUT: u64 = 110;
    const ENOTSUP: u64 = 95;
    let command = operation & !FUTEX_PRIVATE_FLAG;
    if address & 3 != 0 || !crate::arch::user_range_writable(address, 4) {
        frame.registers[0] = negative_errno(EINVAL);
        return;
    }
    if command == FUTEX_WAKE {
        let woken = with_state(|state| {
            let Some(tid) = state.table.current_pid_on(scheduler_cpu()) else {
                return 0;
            };
            let Some(root) = state
                .contexts
                .iter()
                .find(|slot| slot.pid == tid)
                .map(|slot| slot.context.ttbr0)
            else {
                return 0;
            };
            wake_futex_in_state(state, FutexKey::new(root, address), value as usize) as u64
        });
        frame.registers[0] = woken;
        if woken != 0 {
            notify_idle_cpus();
        }
        return;
    }
    if matches!(command, FUTEX_REQUEUE | FUTEX_CMP_REQUEUE) {
        if requeue_address & 3 != 0
            || requeue_address == address
            || !crate::arch::user_range_writable(requeue_address, 4)
        {
            frame.registers[0] = negative_errno(EINVAL);
            return;
        }
        if command == FUTEX_CMP_REQUEUE
            && unsafe { core::ptr::read_volatile(address as *const u32) } != compare_value
        {
            frame.registers[0] = negative_errno(EAGAIN);
            return;
        }
        let result = with_state(|state| {
            let Some(tid) = state.table.current_pid_on(scheduler_cpu()) else {
                return Err(negative_errno(EINVAL));
            };
            let Some(root) = state
                .contexts
                .iter()
                .find(|slot| slot.pid == tid)
                .map(|slot| slot.context.ttbr0)
            else {
                return Err(negative_errno(EINVAL));
            };
            requeue_futex_in_state(
                state,
                FutexKey::new(root, address),
                FutexKey::new(root, requeue_address),
                value as usize,
                timeout_address as usize,
            )
            .ok_or_else(|| negative_errno(EINVAL))
        });
        match result {
            Ok((woken, requeued)) => {
                frame.registers[0] = (woken + requeued) as u64;
                if woken != 0 {
                    notify_idle_cpus();
                }
            }
            Err(error) => frame.registers[0] = error,
        }
        return;
    }
    if command != FUTEX_WAIT {
        frame.registers[0] = negative_errno(ENOTSUP);
        return;
    }
    let now = crate::arch::monotonic_ticks();
    let deadline = if timeout_address == 0 {
        None
    } else {
        if !crate::arch::user_range_readable(timeout_address, 16) {
            frame.registers[0] = negative_errno(EINVAL);
            return;
        }
        let seconds = unsafe { (timeout_address as *const i64).read_unaligned() };
        let nanoseconds = unsafe { ((timeout_address + 8) as *const i64).read_unaligned() };
        if seconds < 0 || !(0..1_000_000_000).contains(&nanoseconds) {
            frame.registers[0] = negative_errno(EINVAL);
            return;
        }
        let ticks = (seconds as u64)
            .checked_mul(100)
            .and_then(|ticks| ticks.checked_add((nanoseconds as u64).div_ceil(10_000_000)));
        let Some(ticks) = ticks else {
            frame.registers[0] = negative_errno(EINVAL);
            return;
        };
        Some(now.saturating_add(ticks))
    };
    let observed = unsafe { core::ptr::read_volatile(address as *const u32) };
    frame.registers[0] = 0;
    let captured = crate::arch::UserContext::capture(frame);
    let result = with_state(|state| {
        let Some(tid) = state.table.current_pid_on(scheduler_cpu()) else {
            return Err(negative_errno(EINVAL));
        };
        let Some(index) = state.contexts.iter().position(|slot| slot.pid == tid) else {
            return Err(negative_errno(EINVAL));
        };
        let group_pid = state.contexts[index].group_pid;
        let key = FutexKey::new(state.contexts[index].context.ttbr0, address);
        let handle = match state.futex.wait(
            key,
            TaskId::new(group_pid, tid),
            observed,
            value,
            now,
            deadline,
        ) {
            Ok(handle) => handle,
            Err(WaitError::ValueMismatch) => return Err(negative_errno(EAGAIN)),
            Err(WaitError::DeadlineExpired) => return Err(negative_errno(ETIMEDOUT)),
            Err(WaitError::QueueFull) => return Err(negative_errno(EAGAIN)),
            Err(_) => return Err(negative_errno(EINVAL)),
        };
        state.contexts[index].context = captured;
        state.contexts[index].futex_wait = Some(handle);
        if state.table.block_current_on(scheduler_cpu()) != Some(tid) {
            let _ = state.futex.cancel(handle);
            let _ = state.futex.take_outcome(handle);
            state.contexts[index].futex_wait = None;
            return Err(negative_errno(EINVAL));
        }
        let cpu = scheduler_cpu();
        let Some(next) = state.schedule_next_for_cpu(cpu) else {
            if state.contexts[index].role == ProcessRole::SmpProbe {
                if cpu == 0 {
                    return Ok(FutexBlockResult::BspIdle(tid));
                }
                SMP_PROBE_FUTEX_IDLE_MASK.fetch_or(1u64 << cpu, Ordering::AcqRel);
                return Ok(FutexBlockResult::SecondaryIdle);
            }
            let _ = state.futex.cancel(handle);
            let _ = state.futex.take_outcome(handle);
            state.contexts[index].futex_wait = None;
            let _ = state.table.wake(tid);
            let _ = state.table.activate_on(scheduler_cpu(), tid);
            return Err(negative_errno(EAGAIN));
        };
        let context = state
            .contexts
            .iter()
            .find(|slot| slot.pid == next.pid)
            .map(|slot| slot.context)
            .ok_or_else(|| negative_errno(EINVAL))?;
        Ok(FutexBlockResult::Context(context))
    });
    match result {
        Ok(FutexBlockResult::Context(context)) => {
            context.restore(frame);
            crate::arch::switch_address_space(context.ttbr0);
        }
        Ok(FutexBlockResult::SecondaryIdle) => crate::arch::return_to_kernel(frame, 0),
        Ok(FutexBlockResult::BspIdle(tid)) => loop {
            let context = with_state(|state| {
                let info = state.table.get(tid)?;
                if info.state != makos_process_table::ProcessState::Ready {
                    return None;
                }
                state.table.activate_on(0, tid).ok()?;
                state
                    .contexts
                    .iter()
                    .find(|slot| slot.pid == tid)
                    .map(|slot| slot.context)
            });
            if let Some(context) = context {
                context.restore(frame);
                crate::arch::switch_address_space(context.ttbr0);
                break;
            }
            crate::arch::enable_interrupts();
            unsafe { asm!("wfi", options(nomem, nostack)) };
            crate::arch::disable_interrupts();
        },
        Err(error) => frame.registers[0] = error,
    }
}

const fn negative_errno(errno: u64) -> u64 {
    (-(errno as i64)) as u64
}

fn wake_futex_in_state(state: &mut SchedulerState, key: FutexKey, maximum: usize) -> usize {
    let mut tasks = [TaskId::new(0, 0); MAX_FUTEX_WAITERS];
    let mut handles = [None; MAX_FUTEX_WAITERS];
    let mut count = 0usize;
    let Ok(woken) = state.futex.wake(key, maximum, |task, handle| {
        tasks[count] = task;
        handles[count] = Some(handle);
        count += 1;
    }) else {
        return 0;
    };
    activate_futex_wakes(state, &tasks, &handles, count);
    woken
}

fn requeue_futex_in_state(
    state: &mut SchedulerState,
    source: FutexKey,
    target: FutexKey,
    wake_limit: usize,
    requeue_limit: usize,
) -> Option<(usize, usize)> {
    let mut tasks = [TaskId::new(0, 0); MAX_FUTEX_WAITERS];
    let mut handles = [None; MAX_FUTEX_WAITERS];
    let mut count = 0usize;
    let result = state
        .futex
        .requeue(source, target, wake_limit, requeue_limit, |task, handle| {
            tasks[count] = task;
            handles[count] = Some(handle);
            count += 1;
        })
        .ok()?;
    activate_futex_wakes(state, &tasks, &handles, count);
    Some(result)
}

fn activate_futex_wakes(
    state: &mut SchedulerState,
    tasks: &[TaskId; MAX_FUTEX_WAITERS],
    handles: &[Option<WaitHandle>; MAX_FUTEX_WAITERS],
    count: usize,
) {
    for index in 0..count {
        let task = tasks[index];
        let firefox_leader = state
            .contexts
            .iter()
            .find(|slot| slot.pid == task.thread)
            .is_some_and(|slot| slot.role == ProcessRole::Firefox && slot.pid == slot.group_pid);
        if let Some(slot) = state
            .contexts
            .iter_mut()
            .find(|slot| slot.pid == task.thread)
        {
            slot.futex_wait = None;
        }
        let _ = state.table.wake(task.thread);
        if firefox_leader && SURFACE_MAIN_HANDOFF_PENDING.swap(false, Ordering::AcqRel) {
            set_surface_priority(task.thread);
            if !SURFACE_MAIN_HANDOFF_REPORTED.swap(true, Ordering::AcqRel) {
                crate::serial_println!(
                    "MAKOS_AARCH64_SURFACE_MAIN_HANDOFF_OK tid={} source=futex-wake bounded_ticks={}",
                    task.thread,
                    SURFACE_PRIORITY_TICKS,
                );
            }
        }
        if let Some(handle) = handles[index] {
            let _ = state.futex.take_outcome(handle);
        }
    }
}

pub fn set_robust_list(head: u64, length: u64) -> bool {
    const ROBUST_HEAD_BYTES: u64 = 24;
    if length != ROBUST_HEAD_BYTES
        || head & 7 != 0
        || (head != 0 && !crate::arch::user_range_readable(head, ROBUST_HEAD_BYTES as usize))
    {
        return false;
    }
    with_state(|state| {
        let Some(tid) = state.table.current_pid_on(scheduler_cpu()) else {
            return false;
        };
        let Some(slot) = state.contexts.iter_mut().find(|slot| slot.pid == tid) else {
            return false;
        };
        slot.robust_list_head = head;
        slot.robust_list_length = length;
        true
    })
}

pub fn get_robust_list(tid: u64) -> Option<(u64, u64)> {
    with_state(|state| {
        let current = state.table.current_pid_on(scheduler_cpu())?;
        let requested = if tid == 0 { current } else { tid };
        if requested != current {
            return None;
        }
        state
            .contexts
            .iter()
            .find(|slot| slot.pid == requested)
            .map(|slot| (slot.robust_list_head, 24))
    })
}

fn robust_list_on_current_exit() {
    let registration = with_state(|state| {
        let tid = state.table.current_pid_on(scheduler_cpu())?;
        let slot = state.contexts.iter_mut().find(|slot| slot.pid == tid)?;
        let registration = (
            tid,
            slot.context.ttbr0,
            slot.robust_list_head,
            slot.robust_list_length,
        );
        slot.robust_list_head = 0;
        slot.robust_list_length = 0;
        Some(registration)
    });
    let Some((tid, root, head, length)) = registration else {
        return;
    };
    let cleaned = cleanup_robust_registration(tid, root, head, length);
    if cleaned != 0 {
        crate::serial_println!(
            "MAKOS_ROBUST_FUTEX_EXIT_OK tid={} owner_died={} wake=one",
            tid,
            cleaned,
        );
    }
}

fn robust_lists_on_group_exit() {
    let (registrations, count) = with_state(|state| {
        let mut registrations = [(0u64, 0u64, 0u64, 0u64); MAX_PROCESSES];
        let Some(caller) = state.table.current_pid_on(scheduler_cpu()) else {
            return (registrations, 0);
        };
        let Some(group_pid) = state
            .contexts
            .iter()
            .find(|slot| slot.pid == caller)
            .map(|slot| slot.group_pid)
        else {
            return (registrations, 0);
        };
        let mut count = 0usize;
        for slot in &mut state.contexts {
            if slot.pid != 0 && slot.group_pid == group_pid {
                registrations[count] = (
                    slot.pid,
                    slot.context.ttbr0,
                    slot.robust_list_head,
                    slot.robust_list_length,
                );
                slot.robust_list_head = 0;
                slot.robust_list_length = 0;
                count += 1;
            }
        }
        (registrations, count)
    });
    for (tid, root, head, length) in registrations[..count].iter().copied() {
        let _ = cleanup_robust_registration(tid, root, head, length);
    }
}

fn cleanup_robust_registration(tid: u64, root: u64, head: u64, length: u64) -> usize {
    const ROBUST_HEAD_BYTES: u64 = 24;
    const MAX_ROBUST_NODES: usize = 2048;
    if head == 0
        || length != ROBUST_HEAD_BYTES
        || head & 7 != 0
        || !crate::arch::user_range_readable(head, ROBUST_HEAD_BYTES as usize)
    {
        return 0;
    }
    let mut entry = unsafe { (head as *const u64).read_volatile() };
    let offset = unsafe { ((head + 8) as *const i64).read_volatile() };
    let pending = unsafe { ((head + 16) as *const u64).read_volatile() };
    let mut cleaned = 0usize;
    for _ in 0..MAX_ROBUST_NODES {
        if entry == head {
            break;
        }
        if entry == 0 || entry & 7 != 0 || !crate::arch::user_range_readable(entry, 8) {
            break;
        }
        let next = unsafe { (entry as *const u64).read_volatile() };
        cleaned += usize::from(mark_robust_futex_owner_dead(tid, root, entry, offset));
        entry = next;
    }
    if pending != 0 {
        cleaned += usize::from(mark_robust_futex_owner_dead(tid, root, pending, offset));
    }
    cleaned
}

fn mark_robust_futex_owner_dead(tid: u64, root: u64, node: u64, offset: i64) -> bool {
    const FUTEX_WAITERS: u32 = 0x8000_0000;
    const FUTEX_OWNER_DIED: u32 = 0x4000_0000;
    const FUTEX_TID_MASK: u32 = 0x3fff_ffff;
    let address = if offset >= 0 {
        node.checked_add(offset as u64)
    } else {
        node.checked_sub(offset.unsigned_abs())
    };
    let Some(address) = address else {
        return false;
    };
    if address & 3 != 0 || !crate::arch::user_range_writable(address, 4) {
        return false;
    }
    let word = unsafe { &*(address as *const AtomicU32) };
    let mut observed = word.load(Ordering::Acquire);
    loop {
        if observed & FUTEX_TID_MASK != tid as u32 {
            return false;
        }
        let replacement = observed & FUTEX_WAITERS | FUTEX_OWNER_DIED;
        match word.compare_exchange(observed, replacement, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => break,
            Err(updated) => observed = updated,
        }
    }
    let _ = with_state(|state| wake_futex_in_state(state, FutexKey::new(root, address), 1));
    notify_idle_cpus();
    true
}

pub fn current_app_role() -> ProcessRole {
    with_state(|state| {
        let Some(pid) = state.table.current_pid_on(scheduler_cpu()) else {
            return ProcessRole::None;
        };
        state
            .contexts
            .iter()
            .find(|slot| slot.pid == pid)
            .map_or(ProcessRole::None, |slot| slot.role)
    })
}

pub fn process_control_allowed() -> bool {
    // Boot fixture is immutable, kernel-embedded code. It exercises process
    // syscalls before interactive PID 1 can authenticate and gain CAP_PROCESS.
    crate::security::has_capability(crate::security::CAP_PROCESS)
        || with_state(|state| {
            state.self_test_session && state.table.current_pid_on(scheduler_cpu()) == Some(1)
        })
}

pub fn ipc_control_allowed() -> bool {
    // The only pre-login exception is immutable kernel-embedded boot-fixture
    // code used to exercise this path before PID1 receives session caps.
    crate::security::has_capability(crate::security::CAP_IPC)
        || current_app_role() == ProcessRole::SmpProbe
}

fn with_state<R>(function: impl FnOnce(&mut SchedulerState) -> R) -> R {
    while PROCESSES
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *PROCESSES.state.get() });
    PROCESSES.lock.store(false, Ordering::Release);
    result
}

fn reset_scheduler() {
    with_state(|state| *state = SchedulerState::new());
}

/// Closed-gate AP dispatch foundation. Every selection, including later
/// timer/yield paths, is limited to non-leader Firefox workers. Block-to-idle
/// and remote teardown gates still prevent architecture code enabling it.
pub(crate) fn run_secondary_scheduler() -> ! {
    let cpu = scheduler_cpu();
    if cpu == 0 {
        crate::fatal("AArch64 secondary dispatcher entered on BSP");
    }
    loop {
        if !crate::arch::smp_probe_scheduler_enabled() {
            crate::arch::idle_secondary_after_smp_probe();
        }
        let selected = with_state(|state| {
            if !state.session_active {
                return None;
            }
            let process = state.schedule_next_for_cpu(cpu)?;
            state
                .contexts
                .iter()
                .find(|slot| slot.pid == process.pid)
                .map(|slot| (process.pid, slot.context))
        });
        let Some((pid, context)) = selected else {
            unsafe { asm!("wfe", options(nomem, nostack)) };
            continue;
        };
        smp_probe_enter(pid);
        while !SMP_PROBE_RELEASE.load(Ordering::Acquire) {
            unsafe { asm!("wfe", options(nomem, nostack)) };
        }
        crate::arch::switch_address_space(context.ttbr0);
        let _ = crate::arch::enter_user_context(&context);
        crate::arch::switch_address_space(crate::arch::kernel_root());
        smp_probe_leave();
    }
}

fn smp_probe_enter(tid: u64) {
    let cpu = scheduler_cpu();
    let bit = 1u64 << cpu;
    if SMP_PROBE_IDLE_MASK.load(Ordering::Acquire) & bit != 0 {
        SMP_PROBE_RESUME_MASK.fetch_or(bit, Ordering::AcqRel);
    }
    if SMP_PROBE_FUTEX_IDLE_MASK.load(Ordering::Acquire) & bit != 0 {
        SMP_PROBE_FUTEX_RESUME_MASK.fetch_or(bit, Ordering::AcqRel);
    }
    if SMP_PROBE_IO_IDLE_MASK.load(Ordering::Acquire) & bit != 0 {
        SMP_PROBE_IO_RESUME_MASK.fetch_or(bit, Ordering::AcqRel);
    }
    if SMP_PROBE_IPC_IDLE_MASK.load(Ordering::Acquire) & bit != 0 {
        SMP_PROBE_IPC_RESUME_MASK.fetch_or(bit, Ordering::AcqRel);
    }
    SMP_PROBE_TIDS[cpu].store(tid, Ordering::Release);
    let active = SMP_PROBE_ACTIVE_MASK.fetch_or(bit, Ordering::AcqRel) | bit;
    let mut peak = SMP_PROBE_PEAK_MASK.load(Ordering::Acquire);
    while active.count_ones() > peak.count_ones() {
        match SMP_PROBE_PEAK_MASK.compare_exchange_weak(
            peak,
            active,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => break,
            Err(current) => peak = current,
        }
    }
}

fn smp_probe_leave() {
    SMP_PROBE_ACTIVE_MASK.fetch_and(!(1u64 << scheduler_cpu()), Ordering::AcqRel);
}

fn spawn_process(
    parent_pid: u64,
    bytes: &[u8],
    argument: u64,
    role: ProcessRole,
) -> Option<(u64, LoadedProcess)> {
    spawn_process_with_startup(parent_pid, bytes, argument, role, None)
}

fn spawn_process_with_startup(
    parent_pid: u64,
    bytes: &[u8],
    mut argument: u64,
    role: ProcessRole,
    startup: Option<&[u8]>,
) -> Option<(u64, LoadedProcess)> {
    if !with_state(|state| state.contexts.iter().any(|slot| slot.pid == 0)) {
        return None;
    }
    let process = load_process(bytes);
    if let Some(startup) = startup {
        if startup.is_empty() || startup.len() >= PAGE_SIZE as usize {
            crate::fatal("AArch64 process startup data invalid");
        }
        unsafe {
            ptr::copy_nonoverlapping(
                startup.as_ptr(),
                process.startup_frame as *mut u8,
                startup.len(),
            );
            *((process.startup_frame + startup.len() as u64) as *mut u8) = 0;
        }
        argument = crate::arch::USER_STACK_TOP - USER_STACK_PAGES as u64 * PAGE_SIZE;
    }
    let context = crate::arch::UserContext::initial(
        process.initial_entry,
        crate::arch::USER_STACK_TOP,
        process.root,
        argument,
    );
    install_loaded_process(parent_pid, process, role, context)
}

fn install_loaded_process(
    parent_pid: u64,
    process: LoadedProcess,
    role: ProcessRole,
    context: crate::arch::UserContext,
) -> Option<(u64, LoadedProcess)> {
    let pid = with_state(|state| {
        let Ok(pid) = state.table.spawn(parent_pid, process.root) else {
            return None;
        };
        if !crate::aarch64_tty::register_process(pid, parent_pid) {
            crate::fatal("AArch64 TTY process table full");
        }
        let slot = state
            .contexts
            .iter_mut()
            .find(|slot| slot.pid == 0)
            .unwrap_or_else(|| crate::fatal("AArch64 context table full"));
        *slot = ContextSlot {
            pid,
            group_pid: pid,
            role,
            context,
            clear_child_tid: 0,
            robust_list_head: 0,
            robust_list_length: 0,
            futex_wait: None,
            sleep_deadline: 0,
            io_wait: false,
            io_source: makos_readiness::WaitSource::Any,
            io_deadline: 0,
            input_wait: false,
            name: [0; 16],
        };
        let index = state
            .contexts
            .iter()
            .position(|slot| slot.pid == pid)
            .unwrap_or_else(|| crate::fatal("AArch64 spawned context absent"));
        state.spawned_roots[index] = process.root;
        state.session_active = true;
        if !crate::aarch64_vm::attach_process(pid, process.root, process.image_end) {
            crate::fatal("AArch64 VM process attach failed");
        }
        Some(pid)
    });
    match pid {
        Some(pid) => {
            notify_idle_cpus();
            Some((pid, process))
        }
        None => {
            crate::arch::destroy_user_address_space(process.root);
            None
        }
    }
}

struct SysvStartup {
    stack_pointer: u64,
    argc: u64,
    argv: u64,
    envp: u64,
}

fn spawn_process_sysv(
    parent_pid: u64,
    bytes: &[u8],
    role: ProcessRole,
    argv: &[&[u8]],
    envp: &[&[u8]],
) -> Option<(u64, LoadedProcess)> {
    if !with_state(|state| state.contexts.iter().any(|slot| slot.pid == 0)) {
        return None;
    }
    let process = load_process(bytes);
    let credentials = crate::security::credentials();
    let startup = build_sysv_startup(&process, argv, envp, credentials.uid, credentials.gid)?;
    let mut context = crate::arch::UserContext::initial(
        process.initial_entry,
        startup.stack_pointer,
        process.root,
        startup.argc,
    );
    context.registers[1] = startup.argv;
    context.registers[2] = startup.envp;
    install_loaded_process(parent_pid, process, role, context)
}

fn run_ready_session() -> u64 {
    let (pid, context) = with_state(|state| {
        let process = state
            .schedule_next_for_cpu(scheduler_cpu())
            .unwrap_or_else(|| crate::fatal("AArch64 run queue empty"));
        let context = state
            .contexts
            .iter()
            .find(|slot| slot.pid == process.pid)
            .unwrap_or_else(|| crate::fatal("AArch64 process context absent"))
            .context;
        let index = state
            .contexts
            .iter()
            .position(|slot| slot.pid == process.pid)
            .unwrap_or_else(|| crate::fatal("AArch64 scheduled context absent"));
        state.timer_dispatches[index] = state.timer_dispatches[index].saturating_add(1);
        (process.pid, context)
    });
    crate::arch::switch_address_space(context.ttbr0);
    let returned_status = crate::arch::enter_user_context(&context);
    crate::arch::switch_address_space(crate::arch::kernel_root());
    let (resource, status) = with_state(|state| match state.table.wait(0, pid) {
        WaitResult::Reaped {
            resource,
            exit_status,
            ..
        } => {
            if exit_status != returned_status {
                crate::fatal("AArch64 process exit status mismatch");
            }
            if let Some(slot) = state.contexts.iter_mut().find(|slot| slot.pid == pid) {
                *slot = ContextSlot::EMPTY;
            }
            state.session_active = false;
            (resource, exit_status)
        }
        _ => crate::fatal("AArch64 process did not become reapable"),
    });
    cleanup_reaped(pid, resource, status);
    status
}

pub fn spawn_worker() -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 {
        return None;
    }
    spawn_process(parent_pid, INIT_ELF, 1, ProcessRole::Worker).map(|(pid, _)| pid)
}

pub fn spawn_browser() -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 {
        return None;
    }
    if let Some(pid) = with_state(|state| {
        state
            .contexts
            .iter()
            .find(|slot| slot.pid != 0 && slot.role == ProcessRole::Browser)
            .map(|slot| slot.pid)
    }) {
        return Some(pid);
    }
    let (pid, process) = spawn_process(parent_pid, BROWSER_ELF, 0, ProcessRole::Browser)?;
    if !crate::security::register_session_process(pid, crate::security::SessionProcessRole::Browser)
    {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_AARCH64_BROWSER_PROCESS_OK pid={} parent={} elf=1 el=0 entry={:#x} ttbr0={:#x} sandbox=graphics,network,input",
        pid,
        parent_pid,
        process.entry,
        process.root,
    );
    Some(pid)
}

pub fn spawn_files() -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 {
        return None;
    }
    if let Some(pid) = with_state(|state| {
        state
            .contexts
            .iter()
            .find(|slot| slot.pid != 0 && slot.role == ProcessRole::Files)
            .map(|slot| slot.pid)
    }) {
        return Some(pid);
    }
    let (pid, process) = spawn_process(parent_pid, FILES_ELF, 0, ProcessRole::Files)?;
    if !crate::security::register_session_process(pid, crate::security::SessionProcessRole::Files) {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_AARCH64_FILES_PROCESS_OK pid={} parent={} elf=1 el=0 entry={:#x} ttbr0={:#x} sandbox=graphics,input,vfs",
        pid,
        parent_pid,
        process.entry,
        process.root,
    );
    Some(pid)
}

pub fn spawn_text_editor(path: &[u8]) -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 || !valid_user_file_path(path) {
        return None;
    }
    if let Some(existing) = with_state(|state| {
        let slot = state
            .contexts
            .iter()
            .find(|slot| slot.pid != 0 && slot.role == ProcessRole::TextEdit)
            .copied()?;
        state.table.get(slot.pid)
    }) {
        return (existing.parent_pid == parent_pid).then_some(existing.pid);
    }
    let (pid, process) = spawn_process_with_startup(
        parent_pid,
        TEXTEDIT_ELF,
        0,
        ProcessRole::TextEdit,
        Some(path),
    )?;
    if !crate::security::register_session_process(
        pid,
        crate::security::SessionProcessRole::TextEdit,
    ) {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_AARCH64_TEXTEDIT_PROCESS_OK pid={} parent={} elf=1 el=0 entry={:#x} ttbr0={:#x} startup_path=kernel-copied",
        pid,
        parent_pid,
        process.entry,
        process.root,
    );
    Some(pid)
}

pub fn spawn_python(path: &[u8]) -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 || !valid_user_file_path(path) {
        return None;
    }
    const EXECUTABLE: &[u8] = b"/usr/bin/python3";
    if crate::vfs::read_only_backing_for_path(EXECUTABLE).is_some() {
        let process = load_dynamic_process_from_vfs(EXECUTABLE)?;
        let credentials = crate::security::credentials();
        let argv = [EXECUTABLE, b"-S".as_slice(), path];
        let envp = [
            b"PATH=/usr/bin".as_slice(),
            b"HOME=/home/user".as_slice(),
            b"TERM=makos".as_slice(),
            b"LANG=C.UTF-8".as_slice(),
            b"PYTHONHOME=/usr".as_slice(),
            b"PYTHONPATH=/usr/lib/python314.zip".as_slice(),
            b"PYTHONUTF8=1".as_slice(),
            b"PYTHONDONTWRITEBYTECODE=1".as_slice(),
        ];
        let startup = build_sysv_startup(&process, &argv, &envp, credentials.uid, credentials.gid)?;
        let mut context = crate::arch::UserContext::initial(
            process.initial_entry,
            startup.stack_pointer,
            process.root,
            startup.argc,
        );
        context.registers[1] = startup.argv;
        context.registers[2] = startup.envp;
        let (pid, process) =
            install_loaded_process(parent_pid, process, ProcessRole::Python, context)?;
        if !crate::security::register_session_process(
            pid,
            crate::security::SessionProcessRole::Python,
        ) {
            discard_spawned(pid);
            return None;
        }
        if !crate::aarch64_tty::make_foreground_child(pid, parent_pid) {
            discard_spawned(pid);
            return None;
        }
        crate::serial_println!(
            "MAKOS_CPYTHON_PROCESS_OK pid={} parent={} implementation=cpython version=3.14.7 exec=/usr/bin/python3 stdlib=python314.zip source={} pt_interp=musl fake=0 host_delegation=0 root={:#x}",
            pid,
            parent_pid,
            core::str::from_utf8(path).unwrap_or("<invalid>"),
            process.root,
        );
        return Some(pid);
    }

    // Minimal images keep genuine upstream MicroPython fallback. Presence of
    // `/usr/bin/python3` always selects CPython; invalid CPython never silently
    // degrades to another implementation.
    let (pid, process) =
        spawn_process_with_startup(parent_pid, PYTHON_ELF, 0, ProcessRole::Python, Some(path))?;
    if !crate::security::register_session_process(pid, crate::security::SessionProcessRole::Python)
    {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_AARCH64_PYTHON_PROCESS_OK pid={} parent={} elf=1 el=0 entry={:#x} ttbr0={:#x} startup_path=kernel-copied sandbox=vfs-read,console",
        pid,
        parent_pid,
        process.entry,
        process.root,
    );
    Some(pid)
}

pub fn spawn_startup_probe() -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 || current_app_role() != ProcessRole::Shell {
        return None;
    }
    let argv: [&[u8]; 3] = [b"/system/startup-probe", b"alpha", b"42"];
    let envp: [&[u8]; 1] = [b"MODE=test"];
    let (pid, process) = spawn_process_sysv(
        parent_pid,
        STARTUP_PROBE_ELF,
        ProcessRole::Native,
        &argv,
        &envp,
    )?;
    if !crate::security::register_session_process(pid, crate::security::SessionProcessRole::Native)
    {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_AARCH64_SYSV_PROCESS_OK pid={} parent={} elf=1 el=0 entry={:#x} ttbr0={:#x} argc=3 envc=1 stack=canonical",
        pid,
        parent_pid,
        process.entry,
        process.root,
    );
    Some(pid)
}

pub fn spawn_musl_probe() -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 || current_app_role() != ProcessRole::Shell {
        return None;
    }
    let argv: [&[u8]; 1] = [b"/system/musl-probe"];
    let envp: [&[u8]; 1] = [b"MODE=runtime"];
    let (pid, process) = spawn_process_sysv(
        parent_pid,
        MUSL_PROBE_ELF,
        ProcessRole::Native,
        &argv,
        &envp,
    )?;
    if !crate::security::register_session_process(pid, crate::security::SessionProcessRole::Native)
    {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_MUSL_PROCESS_OK pid={} parent={} elf=1 el=0 entry={:#x} ttbr0={:#x} libc=upstream-static sandbox=vfs-read,console",
        pid,
        parent_pid,
        process.entry,
        process.root,
    );
    Some(pid)
}

pub fn spawn_toolchain() -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 || current_app_role() != ProcessRole::Shell {
        return None;
    }
    let argv: [&[u8]; 1] = [b"/system/aarch64-toolchain"];
    let envp: [&[u8]; 1] = [b"MODE=assemble"];
    let (pid, process) = spawn_process_sysv(
        parent_pid,
        TOOLCHAIN_ELF,
        ProcessRole::Native,
        &argv,
        &envp,
    )?;
    if !crate::security::register_session_process(
        pid,
        crate::security::SessionProcessRole::Toolchain,
    )
    {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_AARCH64_TOOLCHAIN_PROCESS_OK pid={} parent={} elf=1 el=0 entry={:#x} ttbr0={:#x} source=guest-file",
        pid,
        parent_pid,
        process.entry,
        process.root,
    );
    Some(pid)
}

pub fn spawn_path(path: &[u8]) -> Option<u64> {
    let argv: [&[u8]; 1] = [path];
    spawn_path_inner(path, &argv, &[], "sysv-default")
}

pub fn spawn_path_with_arguments(path: &[u8], bytes: &[u8]) -> Option<u64> {
    let startup = parse_spawn_arguments(bytes)?;
    let mut argv: [&[u8]; SPAWN_MAX_ARGUMENTS] = [&startup.data[0..0]; SPAWN_MAX_ARGUMENTS];
    let mut envp: [&[u8]; SPAWN_MAX_ENVIRONMENT] =
        [&startup.data[0..0]; SPAWN_MAX_ENVIRONMENT];
    for (index, offset) in startup.argv_offsets[..startup.argc].iter().enumerate() {
        let value = startup_string(&startup, *offset)?;
        argv[index] = &value[..value.len() - 1];
    }
    for (index, offset) in startup.env_offsets[..startup.envc].iter().enumerate() {
        let value = startup_string(&startup, *offset)?;
        envp[index] = &value[..value.len() - 1];
    }
    spawn_path_inner(
        path,
        &argv[..startup.argc],
        &envp[..startup.envc],
        "sysv-v1",
    )
}

fn spawn_path_inner(path: &[u8], argv: &[&[u8]], envp: &[&[u8]], startup: &str) -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0
        || current_app_role() != ProcessRole::Shell
        || path.is_empty()
        || path.len() >= crate::vfs::MAX_PATH_BYTES
        || path.contains(&0)
    {
        return None;
    }
    let mut image = [0u8; crate::vfs::MAX_FILE_BYTES];
    let length = crate::vfs::snapshot(path, &mut image)?;
    let segments = validate_static_process_image(&image[..length])?;
    let (pid, process) = spawn_process_sysv(
        parent_pid,
        &image[..length],
        ProcessRole::Native,
        argv,
        envp,
    )?;
    if !crate::security::register_session_process(pid, crate::security::SessionProcessRole::Native)
    {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_AARCH64_EXEC_SPAWN pid={} parent={} source=makfs format=elf64 bytes={} segments={} entry={:#x} ttbr0={:#x} startup={} argc={} envc={}",
        pid,
        parent_pid,
        length,
        segments,
        process.entry,
        process.root,
        startup,
        argv.len(),
        envp.len(),
    );
    Some(pid)
}

fn parse_spawn_arguments(bytes: &[u8]) -> Option<SpawnArguments> {
    if bytes.len() != SPAWN_ARGUMENTS_BYTES
        || read_spawn_u32(bytes, 0)? != SPAWN_ARGUMENTS_VERSION as usize
    {
        return None;
    }
    let argc = read_spawn_u32(bytes, 4)?;
    let envc = read_spawn_u32(bytes, 8)?;
    let data_length = read_spawn_u32(bytes, 12)?;
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
        startup.argv_offsets[index] = read_spawn_u32(bytes, 16 + index * 4)?;
    }
    for index in 0..SPAWN_MAX_ENVIRONMENT {
        startup.env_offsets[index] = read_spawn_u32(bytes, 48 + index * 4)?;
    }
    startup.data.copy_from_slice(&bytes[80..SPAWN_ARGUMENTS_BYTES]);
    if startup.argv_offsets[argc..].iter().any(|offset| *offset != 0)
        || startup.env_offsets[envc..].iter().any(|offset| *offset != 0)
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

fn read_spawn_u32(bytes: &[u8], offset: usize) -> Option<usize> {
    Some(u32::from_le_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?) as usize)
}

fn startup_string(startup: &SpawnArguments, offset: usize) -> Option<&[u8]> {
    let remaining = startup.data.get(offset..startup.data_length)?;
    let length = remaining.iter().position(|byte| *byte == 0)? + 1;
    Some(&remaining[..length])
}

pub fn spawn_musl_crt_probe() -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 || current_app_role() != ProcessRole::Shell {
        return None;
    }
    let argv: [&[u8]; 2] = [b"/system/musl-crt-probe", b"crt"];
    let envp: [&[u8]; 1] = [b"MODE=crt"];
    let (pid, process) = spawn_process_sysv(
        parent_pid,
        MUSL_CRT_PROBE_ELF,
        ProcessRole::Native,
        &argv,
        &envp,
    )?;
    if !crate::security::register_session_process(pid, crate::security::SessionProcessRole::Native)
    {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_MUSL_CRT_PROCESS_OK pid={} parent={} elf=1 el=0 entry={:#x} ttbr0={:#x} startup=sysv crt1=upstream",
        pid,
        parent_pid,
        process.entry,
        process.root,
    );
    Some(pid)
}

pub fn spawn_stack_protector_probe() -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 || current_app_role() != ProcessRole::Shell {
        return None;
    }
    let argv: [&[u8]; 2] = [b"/system/musl-crt-probe", b"stack-smash"];
    let envp: [&[u8]; 1] = [b"MODE=stack-smash"];
    let (pid, process) = spawn_process_sysv(
        parent_pid,
        MUSL_CRT_PROBE_ELF,
        ProcessRole::Native,
        &argv,
        &envp,
    )?;
    if !crate::security::register_session_process(pid, crate::security::SessionProcessRole::Native)
    {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_STACK_PROTECTOR_PROCESS_OK pid={} parent={} elf=1 el=0 entry={:#x} ttbr0={:#x} instrumentation=strong",
        pid,
        parent_pid,
        process.entry,
        process.root,
    );
    Some(pid)
}

pub fn spawn_musl_pthread_probe() -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 || current_app_role() != ProcessRole::Shell {
        return None;
    }
    let argv: [&[u8]; 2] = [b"/system/musl-pthread-probe", b"pthread"];
    let envp: [&[u8]; 1] = [b"MODE=pthread"];
    let (pid, process) = spawn_process_sysv(
        parent_pid,
        MUSL_PTHREAD_PROBE_ELF,
        ProcessRole::Native,
        &argv,
        &envp,
    )?;
    if !crate::security::register_session_process(
        pid,
        crate::security::SessionProcessRole::NativeIpc,
    ) {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_MUSL_PTHREAD_PROCESS_OK pid={} parent={} elf=1 el=0 entry={:#x} ttbr0={:#x} libc=upstream-static thread_abi=clone,futex,tls",
        pid,
        parent_pid,
        process.entry,
        process.root,
    );
    Some(pid)
}

pub fn spawn_musl_interp_probe() -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 || current_app_role() != ProcessRole::Shell {
        return None;
    }
    let process = load_dynamic_process(MUSL_INTERP_PROBE_ELF, MUSL_DYNAMIC_LOADER_ELF);
    let credentials = crate::security::credentials();
    let argv: [&[u8]; 2] = [b"/system/musl-interp-probe", b"dynamic"];
    let envp: [&[u8]; 1] = [b"MODE=dynamic"];
    let startup = build_sysv_startup(&process, &argv, &envp, credentials.uid, credentials.gid)?;
    let mut context = crate::arch::UserContext::initial(
        process.initial_entry,
        startup.stack_pointer,
        process.root,
        startup.argc,
    );
    context.registers[1] = startup.argv;
    context.registers[2] = startup.envp;
    let (pid, process) = install_loaded_process(parent_pid, process, ProcessRole::Native, context)?;
    if !crate::security::register_session_process(pid, crate::security::SessionProcessRole::Native)
    {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_MUSL_INTERP_PROCESS_OK pid={} parent={} app_entry={:#x} loader_entry={:#x} at_base={:#x} pt_interp=1 ttbr0={:#x}",
        pid,
        parent_pid,
        process.entry,
        process.initial_entry,
        process.interpreter_base,
        process.root,
    );
    Some(pid)
}

pub fn spawn_musl_dynamic_probe() -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 || current_app_role() != ProcessRole::Shell {
        return None;
    }
    let process = load_dynamic_process(MUSL_DYNAMIC_PROBE_ELF, MUSL_DYNAMIC_LOADER_ELF);
    let credentials = crate::security::credentials();
    let argv: [&[u8]; 2] = [b"/system/musl-dynamic-probe", b"dynamic"];
    let envp: [&[u8]; 1] = [b"MODE=dynamic-libc"];
    let startup = build_sysv_startup(&process, &argv, &envp, credentials.uid, credentials.gid)?;
    let mut context = crate::arch::UserContext::initial(
        process.initial_entry,
        startup.stack_pointer,
        process.root,
        startup.argc,
    );
    context.registers[1] = startup.argv;
    context.registers[2] = startup.envp;
    let (pid, process) = install_loaded_process(parent_pid, process, ProcessRole::Native, context)?;
    if !crate::security::register_session_process(pid, crate::security::SessionProcessRole::Native)
    {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_MUSL_DYNAMIC_PROCESS_OK pid={} parent={} app_entry={:#x} loader_entry={:#x} dt_needed=libc.so ttbr0={:#x}",
        pid,
        parent_pid,
        process.entry,
        process.initial_entry,
        process.root,
    );
    Some(pid)
}

pub fn spawn_musl_dso_probe() -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 || current_app_role() != ProcessRole::Shell {
        return None;
    }
    let process = load_dynamic_process(MUSL_DSO_PROBE_ELF, MUSL_DYNAMIC_LOADER_ELF);
    let credentials = crate::security::credentials();
    let argv: [&[u8]; 2] = [b"/system/musl-dso-probe", b"shared"];
    let envp: [&[u8]; 1] = [b"MODE=external-dso"];
    let startup = build_sysv_startup(&process, &argv, &envp, credentials.uid, credentials.gid)?;
    let mut context = crate::arch::UserContext::initial(
        process.initial_entry,
        startup.stack_pointer,
        process.root,
        startup.argc,
    );
    context.registers[1] = startup.argv;
    context.registers[2] = startup.envp;
    let (pid, process) = install_loaded_process(parent_pid, process, ProcessRole::Native, context)?;
    if !crate::security::register_session_process(pid, crate::security::SessionProcessRole::Native)
    {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_MUSL_DSO_PROCESS_OK pid={} parent={} app_entry={:#x} loader_entry={:#x} dt_needed=libmakosdemo.so,libc.so source=/usr/lib ttbr0={:#x}",
        pid,
        parent_pid,
        process.entry,
        process.initial_entry,
        process.root,
    );
    Some(pid)
}

pub fn spawn_musl_dlopen_probe() -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 || current_app_role() != ProcessRole::Shell {
        return None;
    }
    let process = load_dynamic_process(MUSL_DLOPEN_PROBE_ELF, MUSL_DYNAMIC_LOADER_ELF);
    let credentials = crate::security::credentials();
    let argv: [&[u8]; 2] = [b"/system/musl-dlopen-probe", b"runtime"];
    let envp: [&[u8]; 1] = [b"MODE=dlopen"];
    let startup = build_sysv_startup(&process, &argv, &envp, credentials.uid, credentials.gid)?;
    let mut context = crate::arch::UserContext::initial(
        process.initial_entry,
        startup.stack_pointer,
        process.root,
        startup.argc,
    );
    context.registers[1] = startup.argv;
    context.registers[2] = startup.envp;
    let (pid, process) = install_loaded_process(parent_pid, process, ProcessRole::Native, context)?;
    if !crate::security::register_session_process(pid, crate::security::SessionProcessRole::Native)
    {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_MUSL_DLOPEN_PROCESS_OK pid={} parent={} app_entry={:#x} loader_entry={:#x} initial_needed=libc.so runtime_path=/usr/lib/libmakosdemo.so ttbr0={:#x}",
        pid,
        parent_pid,
        process.entry,
        process.initial_entry,
        process.root,
    );
    Some(pid)
}

pub fn spawn_musl_exec_probe() -> Option<u64> {
    let parent_pid = current_pid();
    if parent_pid == 0 || current_app_role() != ProcessRole::Shell {
        return None;
    }
    let process = load_dynamic_process(MUSL_EXEC_CALLER_ELF, MUSL_DYNAMIC_LOADER_ELF);
    let credentials = crate::security::credentials();
    let argv = [b"/system/musl-exec-caller".as_slice()];
    let envp = [b"PATH=/usr/bin".as_slice()];
    let startup = build_sysv_startup(&process, &argv, &envp, credentials.uid, credentials.gid)?;
    let mut context = crate::arch::UserContext::initial(
        process.initial_entry,
        startup.stack_pointer,
        process.root,
        startup.argc,
    );
    context.registers[1] = startup.argv;
    context.registers[2] = startup.envp;
    let (pid, _process) =
        install_loaded_process(parent_pid, process, ProcessRole::Native, context)?;
    if !crate::security::register_session_process(pid, crate::security::SessionProcessRole::Native)
    {
        discard_spawned(pid);
        return None;
    }
    Some(pid)
}

pub fn spawn_firefox() -> Option<u64> {
    const PATH: &[u8] = b"/usr/lib/firefox/firefox";
    const PROFILE: &[u8] = b"/home/user/firefox-profile";
    let parent_pid = current_pid();
    if parent_pid == 0 || current_app_role() != ProcessRole::Shell {
        return None;
    }
    let credentials = crate::security::credentials();
    // Keep parent instance attached to MakOS TTY while native widget startup
    // is under active compatibility testing; prevent remote-instance handoff.
    // Give Gecko one explicit persistent profile.  Its normal profile picker
    // expects a pre-existing parent directory on a new installation.
    let profile_state = match crate::vfs::create_directory(PROFILE) {
        Ok(()) => "created",
        Err(crate::vfs::DescriptorError::Exists) => "existing",
        Err(error) => {
            crate::serial_println!(
                "MAKOS_FIREFOX_PROFILE_FAIL path=/home/user/firefox-profile error={:?}",
                error,
            );
            return None;
        }
    };
    crate::serial_println!(
        "MAKOS_FIREFOX_PROFILE_READY path=/home/user/firefox-profile state={} mode=0700 owner=session",
        profile_state,
    );
    let process = load_dynamic_process_from_vfs(PATH)?;
    let argv = [
        PATH,
        b"--no-remote".as_slice(),
        b"--new-instance".as_slice(),
        b"--profile".as_slice(),
        PROFILE,
        b"about:blank".as_slice(),
    ];
    let envp = [
        b"PATH=/usr/lib/firefox:/usr/bin".as_slice(),
        b"LD_LIBRARY_PATH=/usr/lib/firefox:/usr/lib".as_slice(),
        b"HOME=/home/user".as_slice(),
        b"TMPDIR=/home/user".as_slice(),
        b"LANG=en_US.UTF-8".as_slice(),
        b"MOZ_CRASHREPORTER_DISABLE=1".as_slice(),
        b"MOZ_NO_REMOTE=1".as_slice(),
        // MakOS currently provides one software compositor.  Keep Gecko in
        // its supported single-process mode until cross-process compositor
        // bridge support is complete; otherwise parent and content child
        // deadlock while both synchronously initialize that bridge.
        b"MOZ_FORCE_DISABLE_E10S=1".as_slice(),
        // Same bootstrap boundary for Necko: current MakOS local-socket IPC
        // cannot yet satisfy Firefox's isolated socket-process startup.
        // Gecko then runs its real DNS/TCP/TLS stack in the parent process.
        b"MOZ_DISABLE_SOCKET_PROCESS=1".as_slice(),
        // MakOS userspace currently runs on the BSP.  Firefox otherwise
        // applies its desktop minimum of two TaskController workers, adding
        // avoidable runnable contention during cold startup.
        b"MOZ_TASKCONTROLLER_THREADCOUNT=1".as_slice(),
        b"MOZ_HEADLESS_WIDTH=700".as_slice(),
        b"MOZ_HEADLESS_HEIGHT=400".as_slice(),
        b"MOZ_LOG=timestamp,sync,Widget:5,WidgetFocus:5,nsAppRunner:5".as_slice(),
    ];
    let startup = build_sysv_startup(&process, &argv, &envp, credentials.uid, credentials.gid)?;
    let mut context = crate::arch::UserContext::initial(
        process.initial_entry,
        startup.stack_pointer,
        process.root,
        startup.argc,
    );
    context.registers[1] = startup.argv;
    context.registers[2] = startup.envp;
    let (pid, process) =
        install_loaded_process(parent_pid, process, ProcessRole::Firefox, context)?;
    if !crate::security::register_session_process(pid, crate::security::SessionProcessRole::Firefox)
    {
        discard_spawned(pid);
        return None;
    }
    if let Some(browser_pid) = with_state(|state| {
        state
            .contexts
            .iter()
            .find(|slot| slot.pid != 0 && slot.role == ProcessRole::Browser)
            .map(|slot| slot.group_pid)
    }) {
        let closed = crate::graphics::close_all(browser_pid);
        crate::serial_println!(
            "MAKOS_FIREFOX_SLOT_HANDOFF_OK from=browser to=firefox slot=5 closed_surfaces={}",
            closed,
        );
    }
    crate::serial_println!(
        "MAKOS_FIREFOX_PROCESS_OK pid={} parent={} exec=/usr/lib/firefox/firefox pt_interp=musl package=disk demand_paging=1 root={:#x}",
        pid,
        parent_pid,
        process.root
    );
    Some(pid)
}

pub fn spawn_nano(path: &[u8]) -> Option<u64> {
    const EXECUTABLE: &[u8] = b"/usr/bin/nano";
    let parent_pid = current_pid();
    if parent_pid == 0 || current_app_role() != ProcessRole::Shell || !valid_user_file_path(path) {
        return None;
    }
    let process = load_dynamic_process_from_vfs(EXECUTABLE)?;
    let credentials = crate::security::credentials();
    let argv = [EXECUTABLE, path];
    let envp = [
        b"PATH=/usr/bin".as_slice(),
        b"HOME=/home/user".as_slice(),
        b"TERM=makos".as_slice(),
        b"TERMINFO=/usr/share/terminfo".as_slice(),
        b"LANG=C".as_slice(),
    ];
    let startup = build_sysv_startup(&process, &argv, &envp, credentials.uid, credentials.gid)?;
    let mut context = crate::arch::UserContext::initial(
        process.initial_entry,
        startup.stack_pointer,
        process.root,
        startup.argc,
    );
    context.registers[1] = startup.argv;
    context.registers[2] = startup.envp;
    let (pid, process) = install_loaded_process(parent_pid, process, ProcessRole::Nano, context)?;
    if !crate::security::register_session_process(pid, crate::security::SessionProcessRole::Nano) {
        discard_spawned(pid);
        return None;
    }
    if !crate::aarch64_tty::make_foreground_child(pid, parent_pid) {
        discard_spawned(pid);
        return None;
    }
    crate::serial_println!(
        "MAKOS_NANO_PROCESS_OK pid={} parent={} exec=/usr/bin/nano source=gnu-9.1 ncurses=6.5 terminfo=makos file={} pt_interp=musl root={:#x}",
        pid,
        parent_pid,
        core::str::from_utf8(path).unwrap_or("<invalid>"),
        process.root,
    );
    Some(pid)
}

/// Replace current single-threaded process image without changing PID,
/// credentials, parent, process group, working directory, or non-CLOEXEC FDs.
pub(crate) fn exec_current(
    path: &[u8],
    argv: &[&[u8]],
    envp: &[&[u8]],
    frame: &mut crate::arch::ExceptionFrame,
) -> Result<(), i64> {
    let pid = current_pid();
    if pid == 0 || current_tid() != pid {
        return Err(-11);
    }
    let eligible = with_state(|state| {
        state.table.current_pid_on(scheduler_cpu()) == Some(pid)
            && state
                .contexts
                .iter()
                .filter(|slot| slot.group_pid == pid)
                .count()
                == 1
    });
    if !eligible {
        return Err(-11);
    }
    let process = if let Some(application) = crate::vfs::system_executable(path) {
        load_dynamic_process(application, MUSL_DYNAMIC_LOADER_ELF)
    } else {
        // Package executables can exceed kernel heap. Parse bounded headers,
        // then page PT_LOAD content directly from disk on demand.
        load_dynamic_process_from_vfs(path).ok_or(-2)?
    };
    let credentials = crate::security::credentials();
    let startup =
        build_sysv_startup(&process, argv, envp, credentials.uid, credentials.gid).ok_or(-7)?;
    let mut context = crate::arch::UserContext::initial(
        process.initial_entry,
        startup.stack_pointer,
        process.root,
        startup.argc,
    );
    context.registers[1] = startup.argv;
    context.registers[2] = startup.envp;

    let replacement = with_state(|state| {
        if state.table.current_pid_on(scheduler_cpu()) != Some(pid) {
            return None;
        }
        let index = state.contexts.iter().position(|slot| slot.pid == pid)?;
        if state.contexts[index].group_pid != pid
            || state
                .contexts
                .iter()
                .filter(|slot| slot.group_pid == pid)
                .count()
                != 1
        {
            return None;
        }
        let old_root = state
            .table
            .replace_current_resource_on(scheduler_cpu(), process.root)?;
        state.contexts[index].context = context;
        state.contexts[index].clear_child_tid = 0;
        state.contexts[index].robust_list_head = 0;
        state.contexts[index].robust_list_length = 0;
        state.contexts[index].futex_wait = None;
        state.contexts[index].sleep_deadline = 0;
        state.contexts[index].io_wait = false;
        state.contexts[index].io_source = makos_readiness::WaitSource::Any;
        state.contexts[index].io_deadline = 0;
        state.contexts[index].input_wait = false;
        state.spawned_roots[index] = process.root;
        Some(old_root)
    });
    let Some(old_root) = replacement else {
        crate::arch::destroy_user_address_space(process.root);
        return Err(-11);
    };
    let (vm_root, old_regions, old_pages) =
        crate::aarch64_vm::replace_process(pid, process.root, process.image_end)
            .unwrap_or_else(|| crate::fatal("AArch64 exec VM replacement failed"));
    if vm_root != old_root {
        crate::fatal("AArch64 exec root ownership mismatch");
    }
    if !crate::aarch64_tty::exec_process(pid) {
        crate::fatal("AArch64 exec TTY transition failed");
    }
    let closed_fds = crate::vfs::close_on_exec(pid);
    let closed_sockets = crate::aarch64_socket::close_on_exec(pid);
    if closed_fds != 0 || closed_sockets != 0 {
        wake_io_waiters();
    }

    context.restore(frame);
    crate::arch::switch_address_space(process.root);
    let reclaimed = crate::arch::destroy_user_address_space(old_root);
    with_state(|state| {
        state.reclaimed_frames = state.reclaimed_frames.saturating_add(reclaimed);
        state.reaped_vm_regions = state.reaped_vm_regions.saturating_add(old_regions);
        state.reaped_vm_pages = state.reaped_vm_pages.saturating_add(old_pages);
    });
    crate::serial_println!(
        "MAKOS_AARCH64_EXEC_OK pid={} source=system-package pid_preserved=1 argv={} envp={} pt_interp=1 old_root_reclaimed={} cloexec_closed={} threads=single",
        pid,
        argv.len(),
        envp.len(),
        reclaimed,
        closed_fds + closed_sockets,
    );
    Ok(())
}

fn valid_user_file_path(path: &[u8]) -> bool {
    let Some(name) = path.strip_prefix(b"/home/user/") else {
        return false;
    };
    !name.is_empty()
        && name.len() <= 32
        && name != b"."
        && name != b".."
        && name
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-'))
}

fn discard_spawned(pid: u64) {
    const CREDENTIAL_FAILURE_STATUS: u64 = 126;
    let (resource, status) = with_state(|state| {
        let info = state
            .table
            .get(pid)
            .unwrap_or_else(|| crate::fatal("AArch64 discarded process absent"));
        state
            .table
            .terminate(pid, CREDENTIAL_FAILURE_STATUS)
            .unwrap_or_else(|| crate::fatal("AArch64 discarded process could not terminate"));
        let WaitResult::Reaped {
            resource,
            exit_status,
            ..
        } = state.table.wait(info.parent_pid, pid)
        else {
            crate::fatal("AArch64 discarded process could not reap");
        };
        if let Some(slot) = state.contexts.iter_mut().find(|slot| slot.pid == pid) {
            *slot = ContextSlot::EMPTY;
        }
        (resource, exit_status)
    });
    cleanup_reaped(pid, resource, status);
}

pub fn wait(pid: u64) -> Option<u64> {
    let parent_pid = current_pid();
    let reaped = with_state(|state| match state.table.wait(parent_pid, pid) {
        WaitResult::Reaped {
            resource,
            exit_status,
            ..
        } => {
            if let Some(slot) = state.contexts.iter_mut().find(|slot| slot.pid == pid) {
                *slot = ContextSlot::EMPTY;
            }
            Some((resource, exit_status))
        }
        WaitResult::NoChild | WaitResult::Pending => None,
    });
    let (resource, status) = reaped?;
    cleanup_reaped(pid, resource, status);
    Some(status)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChildWaitStatus {
    NoChild,
    Pending,
    Exited(u64),
}

/// Inspect or reap one direct child for POSIX waitid(P_PID).
pub fn wait_status(pid: u64, consume: bool) -> ChildWaitStatus {
    let parent_pid = current_pid();
    let status = with_state(|state| {
        let Some(info) = state.table.get(pid) else {
            return ChildWaitStatus::NoChild;
        };
        if info.parent_pid != parent_pid {
            return ChildWaitStatus::NoChild;
        }
        if info.state != makos_process_table::ProcessState::Zombie {
            return ChildWaitStatus::Pending;
        }
        ChildWaitStatus::Exited(info.exit_status)
    });
    if consume && matches!(status, ChildWaitStatus::Exited(_)) {
        return wait(pid)
            .map(ChildWaitStatus::Exited)
            .unwrap_or(ChildWaitStatus::NoChild);
    }
    status
}

/// Synchronously terminates every non-shell session process. Called by PID1
/// while it remains the active shell during logout, so no target is running.
pub fn terminate_session_apps() -> usize {
    const LOGOUT_STATUS: u64 = 128 + 15;
    let mut reaped = [(0u64, 0u64, 0u64); MAX_PROCESSES];
    let count = with_state(|state| {
        let current = state.table.current_pid_on(scheduler_cpu()).unwrap_or(0);
        let mut targets = [0u64; MAX_PROCESSES];
        let mut target_count = 0usize;
        for slot in &state.contexts {
            if slot.pid != 0 && slot.pid != current && slot.role != ProcessRole::Shell {
                targets[target_count] = slot.pid;
                target_count += 1;
            }
        }
        let mut count = 0usize;
        for pid in targets[..target_count].iter().copied() {
            let Some(info) = state.table.get(pid) else {
                continue;
            };
            if info.state != makos_process_table::ProcessState::Zombie
                && state.table.terminate(pid, LOGOUT_STATUS).is_none()
            {
                continue;
            }
            let WaitResult::Reaped {
                resource,
                exit_status,
                ..
            } = state.table.wait(info.parent_pid, pid)
            else {
                crate::fatal("AArch64 logout process did not reap");
            };
            if let Some(slot) = state.contexts.iter_mut().find(|slot| slot.pid == pid) {
                *slot = ContextSlot::EMPTY;
            }
            reaped[count] = (pid, resource, exit_status);
            count += 1;
        }
        count
    });
    for (pid, resource, status) in reaped[..count].iter().copied() {
        cleanup_reaped(pid, resource, status);
    }
    if count != 0 {
        crate::serial_println!(
            "session-process-cleanup arch=aarch64 count={} status={}",
            count,
            LOGOUT_STATUS,
        );
    }
    count
}

pub(crate) fn yield_from_exception(frame: &mut crate::arch::ExceptionFrame) {
    schedule_from_exception(frame, false);
}

pub(crate) fn service_timer_waiters() {
    let now = crate::arch::monotonic_ticks();
    let woken = with_state(|state| {
        let mut count = 0usize;
        let mut timed_out_tasks = [TaskId::new(0, 0); MAX_FUTEX_WAITERS];
        let mut timed_out_handles = [None; MAX_FUTEX_WAITERS];
        let mut timed_out_count = 0usize;
        state.futex.expire(now, |task, handle| {
            timed_out_tasks[timed_out_count] = task;
            timed_out_handles[timed_out_count] = Some(handle);
            timed_out_count += 1;
        });
        for index in 0..timed_out_count {
            let task = timed_out_tasks[index];
            if let Some(slot) = state
                .contexts
                .iter_mut()
                .find(|slot| slot.pid == task.thread)
            {
                slot.futex_wait = None;
                slot.context.registers[0] = negative_errno(110);
            }
            if state.table.wake(task.thread) {
                count += 1;
            }
            if let Some(handle) = timed_out_handles[index] {
                let _ = state.futex.take_outcome(handle);
            }
        }
        for index in 0..state.contexts.len() {
            let deadline = state.contexts[index].sleep_deadline;
            if deadline != 0 && deadline <= now {
                let tid = state.contexts[index].pid;
                state.contexts[index].sleep_deadline = 0;
                if state.table.wake(tid) {
                    count += 1;
                }
            }
        }
        count
    });
    if woken != 0 && !SLEEP_WAKE_REPORTED.swap(true, Ordering::AcqRel) {
        crate::serial_println!(
            "MAKOS_AARCH64_SLEEP_WAKE_OK tasks={} source=timer deadline=monotonic",
            woken,
        );
    }
    if woken != 0 {
        notify_idle_cpus();
    }
}

pub(crate) fn preempt_from_timer(frame: &mut crate::arch::ExceptionFrame) {
    if scheduler_cpu() == 0 {
        service_timer_waiters();
    }
    schedule_from_exception(frame, true);
}

pub(crate) fn stop_from_exception(signal: u32, frame: &mut crate::arch::ExceptionFrame) {
    let captured = crate::arch::UserContext::capture(frame);
    let switched = with_state(|state| {
        let Some(pid) = state.table.current_pid_on(scheduler_cpu()) else {
            return None;
        };
        let Some(slot) = state.contexts.iter_mut().find(|slot| slot.pid == pid) else {
            return None;
        };
        slot.context = captured;
        if state.table.block_current_on(scheduler_cpu()) != Some(pid) {
            return None;
        }
        let Some(next) = state.schedule_next_for_cpu(scheduler_cpu()) else {
            if !state.table.wake(pid) || state.table.activate_on(scheduler_cpu(), pid).is_err() {
                crate::fatal("AArch64 stopped sole process could not resume");
            }
            return None;
        };
        let context = state
            .contexts
            .iter()
            .find(|slot| slot.pid == next.pid)
            .unwrap_or_else(|| crate::fatal("AArch64 stopped-process successor absent"))
            .context;
        Some((pid, next.pid, context))
    });
    if let Some((pid, next_pid, context)) = switched {
        context.restore(frame);
        crate::arch::switch_address_space(context.ttbr0);
        crate::serial_println!(
            "process-stop arch=aarch64 pid={} signal={} next={}",
            pid,
            signal,
            next_pid,
        );
    }
}

pub fn continue_process(pid: u64) -> bool {
    let resumed = with_state(|state| state.table.wake(pid));
    if resumed {
        notify_idle_cpus();
        crate::serial_println!("process-continue arch=aarch64 pid={}", pid);
    }
    resumed
}

pub(crate) fn exit_from_exception(status: u64, frame: &mut crate::arch::ExceptionFrame) {
    if current_tid() != current_pid() {
        exit_thread_from_exception(status, frame);
        return;
    }
    let smp_probe = current_app_role() == ProcessRole::SmpProbe;
    robust_list_on_current_exit();
    clear_child_tid_on_exit();
    let (pid, next) = with_state(|state| {
        let process = state
            .table
            .exit_current_on(scheduler_cpu(), status)
            .unwrap_or_else(|| crate::fatal("AArch64 exit without running process"));
        // PID1 polls child completion from its input loop. If child was last
        // runnable task, wake blocked parent before selecting successor so
        // session does not incorrectly look finished.
        for index in 0..state.contexts.len() {
            if state.contexts[index].group_pid == process.parent_pid
                && state.contexts[index].input_wait
            {
                let _ = state.table.wake(state.contexts[index].pid);
            }
        }
        let next = state.schedule_next_for_cpu(scheduler_cpu()).map(|next| {
            let index = state
                .contexts
                .iter()
                .position(|slot| slot.pid == next.pid)
                .unwrap_or_else(|| crate::fatal("AArch64 next context absent after exit"));
            state.timer_dispatches[index] = state.timer_dispatches[index].saturating_add(1);
            state.contexts[index].context
        });
        if next.is_none()
            && !state
                .contexts
                .iter()
                .any(|slot| slot.pid != 0 && state.table.running_cpu(slot.pid).is_some())
        {
            state.session_active = false;
        }
        (process.pid, next)
    });
    let closed_ipc_handles = crate::ipc::close_all(pid);
    if !smp_probe {
        crate::serial_println!(
            "process-exit arch=aarch64 pid={} status={} closed_ipc_handles={}",
            pid,
            status,
            closed_ipc_handles,
        );
    }
    if let Some(context) = next {
        context.restore(frame);
        crate::arch::switch_address_space(context.ttbr0);
    } else {
        crate::arch::switch_address_space(crate::arch::kernel_root());
        crate::arch::return_to_kernel(frame, status);
    }
}

/// Linux/POSIX exit_group: terminate every task sharing caller's process.
/// Group leader remains a zombie until its real parent waits; worker task
/// records are reaped immediately because they own no process resources.
pub(crate) fn exit_group_from_exception(status: u64, frame: &mut crate::arch::ExceptionFrame) {
    robust_lists_on_group_exit();
    let (group_pid, caller_tid, worker_tids, worker_count, next) = with_state(|state| {
        let caller_tid = state
            .table
            .current_pid_on(scheduler_cpu())
            .unwrap_or_else(|| crate::fatal("AArch64 exit_group without current task"));
        let group_pid = state
            .contexts
            .iter()
            .find(|slot| slot.pid == caller_tid)
            .map(|slot| slot.group_pid)
            .unwrap_or_else(|| crate::fatal("AArch64 exit_group context absent"));
        let group_root = state
            .table
            .get(group_pid)
            .map(|process| process.resource)
            .unwrap_or_else(|| crate::fatal("AArch64 exit_group leader absent"));
        let mut worker_tids = [0u64; MAX_PROCESSES];
        let mut worker_count = 0usize;
        for slot in state.contexts {
            if slot.pid != 0 && slot.pid != group_pid && slot.group_pid == group_pid {
                worker_tids[worker_count] = slot.pid;
                worker_count += 1;
            }
        }

        if caller_tid == group_pid {
            state
                .table
                .exit_current_on(scheduler_cpu(), status)
                .unwrap_or_else(|| crate::fatal("AArch64 exit_group leader transition failed"));
        } else {
            state
                .table
                .exit_current_on(scheduler_cpu(), status)
                .unwrap_or_else(|| crate::fatal("AArch64 exit_group caller transition failed"));
        }

        for tid in worker_tids[..worker_count].iter().copied() {
            if tid != caller_tid {
                let Some(info) = state.table.get(tid) else {
                    crate::fatal("AArch64 exit_group worker absent");
                };
                if info.state != makos_process_table::ProcessState::Zombie
                    && state.table.terminate(tid, status).is_none()
                {
                    crate::fatal("AArch64 exit_group worker transition failed");
                }
            }
            let WaitResult::Reaped { resource, .. } = state.table.wait(group_pid, tid) else {
                crate::fatal("AArch64 exit_group worker reap failed");
            };
            if resource != group_root {
                crate::fatal("AArch64 exit_group shared root mismatch");
            }
            if let Some(slot) = state.contexts.iter_mut().find(|slot| slot.pid == tid) {
                *slot = ContextSlot::EMPTY;
            }
        }

        if caller_tid != group_pid {
            let Some(leader) = state.table.get(group_pid) else {
                crate::fatal("AArch64 exit_group leader vanished");
            };
            if leader.state != makos_process_table::ProcessState::Zombie
                && state.table.terminate(group_pid, status).is_none()
            {
                crate::fatal("AArch64 exit_group leader transition failed");
            }
        }
        let _ = state.futex.process_exit(group_pid);
        let next = state.schedule_next_for_cpu(scheduler_cpu()).map(|next| {
            let index = state
                .contexts
                .iter()
                .position(|slot| slot.pid == next.pid)
                .unwrap_or_else(|| crate::fatal("AArch64 next context absent after exit_group"));
            state.timer_dispatches[index] = state.timer_dispatches[index].saturating_add(1);
            state.contexts[index].context
        });
        if next.is_none() {
            state.session_active = false;
        }
        (group_pid, caller_tid, worker_tids, worker_count, next)
    });

    for tid in worker_tids[..worker_count].iter().copied() {
        if !crate::aarch64_tty::close_thread(tid) {
            crate::fatal("AArch64 exit_group worker TTY state absent");
        }
    }
    let closed_ipc_handles = crate::ipc::close_all(group_pid);
    crate::serial_println!(
        "MAKOS_AARCH64_EXIT_GROUP_OK pid={} caller_tid={} status={} workers={} leader=zombie shared_root=retained closed_ipc_handles={}",
        group_pid,
        caller_tid,
        status,
        worker_count,
        closed_ipc_handles,
    );
    if let Some(context) = next {
        context.restore(frame);
        crate::arch::switch_address_space(context.ttbr0);
    } else {
        crate::arch::switch_address_space(crate::arch::kernel_root());
        crate::arch::return_to_kernel(frame, status);
    }
}

fn exit_thread_from_exception(status: u64, frame: &mut crate::arch::ExceptionFrame) {
    robust_list_on_current_exit();
    clear_child_tid_on_exit();
    let (tid, group_pid, next) = with_state(|state| {
        let tid = state
            .table
            .current_pid_on(scheduler_cpu())
            .unwrap_or_else(|| crate::fatal("AArch64 thread exit without current task"));
        let group_pid = state
            .contexts
            .iter()
            .find(|slot| slot.pid == tid)
            .map(|slot| slot.group_pid)
            .unwrap_or_else(|| crate::fatal("AArch64 exiting thread context absent"));
        if group_pid == tid {
            crate::fatal("AArch64 process entered thread-exit path");
        }
        state
            .table
            .exit_current_on(scheduler_cpu(), status)
            .unwrap_or_else(|| crate::fatal("AArch64 thread exit transition failed"));
        let WaitResult::Reaped { resource, .. } = state.table.wait(group_pid, tid) else {
            crate::fatal("AArch64 thread immediate reap failed");
        };
        let group_root = state
            .contexts
            .iter()
            .find(|slot| slot.pid == group_pid)
            .map(|slot| slot.context.ttbr0)
            .unwrap_or(resource);
        if resource != group_root {
            crate::fatal("AArch64 thread shared root mismatch");
        }
        if let Some(handle) = state.futex.cancel_task(TaskId::new(group_pid, tid)) {
            let _ = state.futex.take_outcome(handle);
        }
        if let Some(slot) = state.contexts.iter_mut().find(|slot| slot.pid == tid) {
            *slot = ContextSlot::EMPTY;
        }
        let next = state.schedule_next_for_cpu(scheduler_cpu()).map(|next| {
            let index = state
                .contexts
                .iter()
                .position(|slot| slot.pid == next.pid)
                .unwrap_or_else(|| crate::fatal("AArch64 next context absent after thread exit"));
            state.timer_dispatches[index] = state.timer_dispatches[index].saturating_add(1);
            state.contexts[index].context
        });
        (tid, group_pid, next)
    });
    if THREAD_EXIT_TRACES.fetch_add(1, Ordering::Relaxed) < THREAD_TRACE_LIMIT {
        crate::serial_println!(
            "MAKOS_AARCH64_THREAD_EXIT_OK pid={} tid={} status={} reap=task-only shared_root=retained",
            group_pid,
            tid,
            status,
        );
    }
    if !crate::aarch64_tty::close_thread(tid) {
        crate::fatal("AArch64 exiting thread TTY state absent");
    }
    if let Some(context) = next {
        context.restore(frame);
        crate::arch::switch_address_space(context.ttbr0);
    } else {
        crate::arch::switch_address_space(crate::arch::kernel_root());
        crate::arch::return_to_kernel(frame, status);
    }
}

fn clear_child_tid_on_exit() {
    let (pid, root, address) = with_state(|state| {
        let Some(pid) = state.table.current_pid_on(scheduler_cpu()) else {
            return (0, 0, 0);
        };
        let Some(slot) = state.contexts.iter_mut().find(|slot| slot.pid == pid) else {
            return (pid, 0, 0);
        };
        let address = slot.clear_child_tid;
        slot.clear_child_tid = 0;
        (pid, slot.context.ttbr0, address)
    });
    if address == 0 {
        return;
    }
    if crate::arch::user_range_writable(address, core::mem::size_of::<u32>()) {
        unsafe { core::ptr::write_volatile(address as *mut u32, 0) };
        let woken = with_state(|state| {
            wake_futex_in_state(state, FutexKey::new(root, address), usize::MAX)
        });
        crate::serial_println!(
            "MAKOS_CLEAR_CHILD_TID_OK pid={} address={:#x} zeroed=1 wake={}",
            pid,
            address,
            woken,
        );
    } else {
        crate::serial_println!(
            "clear-child-tid arch=aarch64 pid={} address={:#x} result=unmapped",
            pid,
            address,
        );
    }
}

fn select_surface_priority(
    state: &mut SchedulerState,
    prior_pid: u64,
) -> Option<makos_process_table::ProcessInfo> {
    let tid = SURFACE_PRIORITY_TID.load(Ordering::Acquire);
    if tid == 0 {
        return None;
    }
    let now = crate::arch::monotonic_ticks();
    let deadline = SURFACE_PRIORITY_DEADLINE.load(Ordering::Acquire);
    if deadline == 0 || now > deadline {
        SURFACE_PRIORITY_TID.store(0, Ordering::Release);
        SURFACE_PRIORITY_DEADLINE.store(0, Ordering::Release);
        return None;
    }
    let Some(info) = state.table.get(tid) else {
        SURFACE_PRIORITY_TID.store(0, Ordering::Release);
        SURFACE_PRIORITY_DEADLINE.store(0, Ordering::Release);
        return None;
    };
    match info.state {
        makos_process_table::ProcessState::Running if tid == prior_pid => Some(info),
        makos_process_table::ProcessState::Ready => {
            if state.table.activate_on(scheduler_cpu(), tid).is_err() {
                return None;
            }
            state.table.get(tid)
        }
        makos_process_table::ProcessState::Blocked => None,
        _ => {
            SURFACE_PRIORITY_TID.store(0, Ordering::Release);
            SURFACE_PRIORITY_DEADLINE.store(0, Ordering::Release);
            None
        }
    }
}

fn schedule_from_exception(frame: &mut crate::arch::ExceptionFrame, timer: bool) {
    let cpu = scheduler_cpu();
    let captured = crate::arch::UserContext::capture(frame);
    let (prior_pid, next) = with_state(|state| {
        if !state.session_active {
            return (0, None);
        }
        let prior_pid = state
            .table
            .current_pid_on(cpu)
            .unwrap_or_else(|| crate::fatal("AArch64 schedule without current process"));
        let prior = state
            .contexts
            .iter_mut()
            .find(|slot| slot.pid == prior_pid)
            .unwrap_or_else(|| crate::fatal("AArch64 current context absent"));
        prior.context = captured;
        let priority = if cpu == 0 {
            select_surface_priority(state, prior_pid)
        } else {
            None
        };
        let next = priority
            .or_else(|| state.schedule_next_for_cpu(cpu))
            .unwrap_or_else(|| crate::fatal("AArch64 schedule found no runnable process"));
        let index = state
            .contexts
            .iter()
            .position(|slot| slot.pid == next.pid)
            .unwrap_or_else(|| crate::fatal("AArch64 selected context absent"));
        state.timer_dispatches[index] = state.timer_dispatches[index].saturating_add(1);
        if timer && next.pid != prior_pid {
            state.timer_switches = state.timer_switches.saturating_add(1);
        }
        (
            prior_pid,
            Some((next.pid, state.contexts[index].context, priority.is_some())),
        )
    });
    let Some((next_pid, context, surface_priority)) = next else {
        return;
    };
    context.restore(frame);
    if next_pid != prior_pid {
        crate::arch::switch_address_space(context.ttbr0);
    }
    if surface_priority && !SURFACE_PRIORITY_REPORTED.swap(true, Ordering::AcqRel) {
        crate::serial_println!(
            "MAKOS_AARCH64_SURFACE_PRIORITY_OK tid={} source=key scheduler=next-ready bounded_ticks={}",
            next_pid,
            SURFACE_PRIORITY_TICKS,
        );
    }
}

fn cleanup_reaped(pid: u64, resource: u64, status: u64) {
    let closed_futex_waiters = with_state(|state| state.futex.process_exit(pid));
    crate::security::clear_process_credentials(pid);
    let (vm_regions, vm_pages) = crate::aarch64_vm::close_process(pid);
    let closed_tty_fds = crate::aarch64_tty::close_process(pid);
    let closed_files = crate::vfs::close_all(pid);
    if closed_files != 0 {
        wake_io_waiters();
    }
    let closed_surfaces = crate::graphics::close_all(pid);
    let closed_sockets = crate::aarch64_socket::close_all(pid);
    let closed_epolls = crate::aarch64_epoll::close_all(pid);
    if closed_sockets != 0 {
        wake_io_waiters();
    }
    let closed_ipc_handles = crate::ipc::close_all(pid);
    let reclaimed = crate::arch::destroy_user_address_space(resource);
    with_state(|state| {
        state.reclaimed_frames = state.reclaimed_frames.saturating_add(reclaimed);
        state.reaped_processes = state.reaped_processes.saturating_add(1);
        state.reaped_vm_regions = state.reaped_vm_regions.saturating_add(vm_regions);
        state.reaped_vm_pages = state.reaped_vm_pages.saturating_add(vm_pages);
    });
    crate::serial_println!(
        "process-reap arch=aarch64 pid={} status={} closed_fds={} closed_tty_fds={} closed_surfaces={} closed_sockets={} closed_epolls={} closed_ipc_handles={} closed_futex_waiters={} vm_regions={} vm_pages={} reclaimed_frames={}",
        pid,
        status,
        closed_files,
        closed_tty_fds,
        closed_surfaces,
        closed_sockets,
        closed_epolls,
        closed_ipc_handles,
        closed_futex_waiters,
        vm_regions,
        vm_pages,
        reclaimed,
    );
}

#[derive(Clone, Copy)]
struct LoadedProcess {
    root: u64,
    entry: u64,
    initial_entry: u64,
    interpreter_base: u64,
    image_end: u64,
    startup_frame: u64,
    stack_frames: [u64; USER_STACK_PAGES],
    phdr: u64,
    phnum: u64,
}

fn load_process(bytes: &[u8]) -> LoadedProcess {
    load_process_image(bytes, ET_EXEC, 0, None)
}

fn validate_static_process_image(bytes: &[u8]) -> Option<usize> {
    let elf = Elf64::parse_for_machine(bytes, EM_AARCH64).ok()?;
    if elf.elf_type() != ET_EXEC {
        return None;
    }
    let entry = elf.entry();
    let mut load_count = 0usize;
    let mut executable_entry = false;
    let mut ranges = [(0u64, 0u64); MAX_LOAD_SEGMENTS];
    for segment in elf
        .program_headers()
        .filter(|header| header.segment_type == PT_LOAD)
    {
        let virtual_start = segment.virtual_address;
        let mapped_start = virtual_start & !(PAGE_SIZE - 1);
        let end = virtual_start.checked_add(segment.memory_size)?;
        let mapped_end = end.checked_add(PAGE_SIZE - 1)? & !(PAGE_SIZE - 1);
        let file_end = segment.offset.checked_add(segment.file_size)?;
        let alignment = segment.alignment.max(1);
        if load_count == MAX_LOAD_SEGMENTS
            || mapped_start < crate::arch::USER_ADDRESS_BASE
            || segment.memory_size == 0
            || segment.file_size > segment.memory_size
            || file_end > bytes.len() as u64
            || end > crate::arch::USER_IMAGE_LIMIT
            || segment.flags & 4 == 0
            || segment.flags & !7 != 0
            || segment.flags & 3 == 3
            || !alignment.is_power_of_two()
            || (segment.virtual_address.wrapping_sub(segment.offset)) & (alignment - 1) != 0
            || ranges[..load_count]
                .iter()
                .any(|(start, prior_end)| mapped_start < *prior_end && *start < mapped_end)
        {
            return None;
        }
        executable_entry |= segment.flags & 1 != 0 && (virtual_start..end).contains(&entry);
        ranges[load_count] = (mapped_start, mapped_end);
        load_count += 1;
    }
    (load_count != 0 && executable_entry).then_some(load_count)
}

fn load_dynamic_process(application: &[u8], loader: &[u8]) -> LoadedProcess {
    let application_elf = Elf64::parse_for_machine(application, EM_AARCH64)
        .unwrap_or_else(|_| crate::fatal("AArch64 dynamic application ELF invalid"));
    let mut interpreter_ok = false;
    for header in application_elf
        .program_headers()
        .filter(|header| header.segment_type == PT_INTERP)
    {
        let start = header.offset as usize;
        let end = start.saturating_add(header.file_size as usize);
        interpreter_ok = application.get(start..end) == Some(b"/lib/ld-musl-aarch64.so.1\0");
    }
    if application_elf.elf_type() != ET_DYN || !interpreter_ok {
        crate::fatal("AArch64 dynamic application PT_INTERP rejected");
    }
    load_process_image(
        application,
        ET_DYN,
        DYNAMIC_APP_BASE,
        Some((loader, DYNAMIC_LOADER_BASE)),
    )
}

fn load_dynamic_process_from_vfs(path: &[u8]) -> Option<LoadedProcess> {
    let (backing, file_length) = crate::vfs::read_only_backing_for_path(path)?;
    let mut header_bytes = [0u8; PAGE_SIZE as usize];
    let header_length = crate::vfs::read_only_backing(backing, 0, &mut header_bytes)?;
    let headers = &header_bytes[..header_length];
    let elf = Elf64::parse_headers_for_machine(headers, EM_AARCH64, file_length).ok()?;
    if elf.elf_type() != ET_DYN {
        return None;
    }
    let mut interpreter_ok = false;
    for header in elf
        .program_headers()
        .filter(|header| header.segment_type == PT_INTERP)
    {
        let length = usize::try_from(header.file_size).ok()?;
        let mut interpreter = [0u8; 64];
        if length > interpreter.len()
            || crate::vfs::read_only_backing(backing, header.offset, &mut interpreter[..length])
                != Some(length)
        {
            return None;
        }
        interpreter_ok = &interpreter[..length] == b"/lib/ld-musl-aarch64.so.1\0";
    }
    if !interpreter_ok {
        return None;
    }
    Some(load_process_image_backed(
        headers,
        file_length,
        backing,
        DYNAMIC_APP_BASE,
        MUSL_DYNAMIC_LOADER_ELF,
    ))
}

#[derive(Clone, Copy)]
struct MappedImage {
    entry: u64,
    image_end: u64,
    phdr: u64,
    phnum: u64,
}

fn map_image(
    bytes: &[u8],
    root: u64,
    expected_type: u16,
    base: u64,
    address_start: u64,
    address_limit: u64,
) -> MappedImage {
    let elf = Elf64::parse_for_machine(bytes, EM_AARCH64)
        .unwrap_or_else(|_| crate::fatal("AArch64 process ELF validation failed"));
    if elf.elf_type() != expected_type {
        crate::fatal("AArch64 process ELF type rejected");
    }
    let entry = base
        .checked_add(elf.entry())
        .unwrap_or_else(|| crate::fatal("AArch64 ELF entry overflow"));
    let ph_offset = u64::from_le_bytes(bytes[32..40].try_into().unwrap());
    let ph_num = u16::from_le_bytes(bytes[56..58].try_into().unwrap()) as u64;
    let ph_bytes = ph_num
        .checked_mul(56)
        .unwrap_or_else(|| crate::fatal("AArch64 ELF program-header overflow"));
    let mut load_count = 0usize;
    let mut executable_entry = false;
    let mut image_end = 0u64;
    let mut ranges = [(0u64, 0u64); MAX_LOAD_SEGMENTS];
    let mut phdr = 0u64;
    for segment in elf
        .program_headers()
        .filter(|header| header.segment_type == PT_LOAD)
    {
        let virtual_start = base
            .checked_add(segment.virtual_address)
            .unwrap_or_else(|| crate::fatal("AArch64 process segment base overflow"));
        let mapped_start = virtual_start & !(PAGE_SIZE - 1);
        let end = virtual_start
            .checked_add(segment.memory_size)
            .unwrap_or_else(|| crate::fatal("AArch64 process segment overflow"));
        let mapped_end = end
            .checked_add(PAGE_SIZE - 1)
            .unwrap_or_else(|| crate::fatal("AArch64 process mapped-end overflow"))
            & !(PAGE_SIZE - 1);
        let alignment = segment.alignment.max(1);
        if load_count == MAX_LOAD_SEGMENTS
            || mapped_start < address_start
            || segment.memory_size == 0
            || segment.file_size > segment.memory_size
            || segment
                .offset
                .checked_add(segment.file_size)
                .is_none_or(|file_end| file_end > bytes.len() as u64)
            || end > address_limit
            || segment.flags & 4 == 0
            || segment.flags & !7 != 0
            || segment.flags & 3 == 3
            || !alignment.is_power_of_two()
            || (segment.virtual_address.wrapping_sub(segment.offset)) & (alignment - 1) != 0
            || ranges[..load_count]
                .iter()
                .any(|(start, prior_end)| mapped_start < *prior_end && *start < mapped_end)
        {
            crate::fatal("AArch64 process ELF layout rejected");
        }
        executable_entry |= segment.flags & 1 != 0 && (virtual_start..end).contains(&entry);
        if ph_offset >= segment.offset
            && ph_offset
                .checked_add(ph_bytes)
                .is_some_and(|ph_end| ph_end <= segment.offset.saturating_add(segment.file_size))
        {
            phdr = virtual_start + (ph_offset - segment.offset);
        }
        ranges[load_count] = (mapped_start, mapped_end);
        image_end = image_end.max(mapped_end);
        load_count += 1;
    }
    if load_count == 0 || !executable_entry {
        crate::fatal("AArch64 process ELF lacks executable entry");
    }

    for segment in elf
        .program_headers()
        .filter(|header| header.segment_type == PT_LOAD)
    {
        let virtual_start = base + segment.virtual_address;
        let mapped_start = virtual_start & !(PAGE_SIZE - 1);
        let mapped_end = (virtual_start + segment.memory_size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let file_end = virtual_start + segment.file_size;
        let mut page_address = mapped_start;
        while page_address < mapped_end {
            let frame = crate::mm::allocate_frame()
                .unwrap_or_else(|| crate::fatal("AArch64 process page OOM"));
            unsafe { ptr::write_bytes(frame as *mut u8, 0, PAGE_SIZE as usize) };
            let copy_start = page_address.max(virtual_start);
            let copy_end = (page_address + PAGE_SIZE).min(file_end);
            if copy_start < copy_end {
                let source_offset = segment.offset + (copy_start - virtual_start);
                unsafe {
                    ptr::copy_nonoverlapping(
                        bytes.as_ptr().add(source_offset as usize),
                        (frame as *mut u8).add((copy_start - page_address) as usize),
                        (copy_end - copy_start) as usize,
                    )
                };
            }
            if segment.flags & 1 != 0 {
                crate::arch::sync_user_code(frame);
            }
            crate::arch::map_user_page_in(
                root,
                page_address,
                frame,
                segment.flags & 2 != 0,
                segment.flags & 1 != 0,
            );
            page_address += PAGE_SIZE;
        }
    }
    MappedImage {
        entry,
        image_end,
        phdr,
        phnum: ph_num,
    }
}

fn map_backed_image(
    headers: &[u8],
    file_length: u64,
    backing: crate::vfs::ReadOnlyFileBacking,
    root: u64,
    base: u64,
    address_start: u64,
    address_limit: u64,
) -> MappedImage {
    let elf = Elf64::parse_headers_for_machine(headers, EM_AARCH64, file_length)
        .unwrap_or_else(|_| crate::fatal("AArch64 backed ELF validation failed"));
    if elf.elf_type() != ET_DYN {
        crate::fatal("AArch64 backed ELF type rejected");
    }
    let entry = base
        .checked_add(elf.entry())
        .unwrap_or_else(|| crate::fatal("AArch64 backed ELF entry overflow"));
    let ph_offset = u64::from_le_bytes(headers[32..40].try_into().unwrap());
    let ph_num = u16::from_le_bytes(headers[56..58].try_into().unwrap()) as u64;
    let ph_bytes = ph_num
        .checked_mul(56)
        .unwrap_or_else(|| crate::fatal("AArch64 backed ELF program-header overflow"));
    let mut load_count = 0usize;
    let mut executable_entry = false;
    let mut image_end = 0u64;
    let mut ranges = [(0u64, 0u64); MAX_LOAD_SEGMENTS];
    let mut phdr = 0u64;
    for segment in elf
        .program_headers()
        .filter(|header| header.segment_type == PT_LOAD)
    {
        let virtual_start = base
            .checked_add(segment.virtual_address)
            .unwrap_or_else(|| crate::fatal("AArch64 backed segment base overflow"));
        let mapped_start = virtual_start & !(PAGE_SIZE - 1);
        let end = virtual_start
            .checked_add(segment.memory_size)
            .unwrap_or_else(|| crate::fatal("AArch64 backed segment overflow"));
        let mapped_end = end
            .checked_add(PAGE_SIZE - 1)
            .unwrap_or_else(|| crate::fatal("AArch64 backed mapped-end overflow"))
            & !(PAGE_SIZE - 1);
        let alignment = segment.alignment.max(1);
        if load_count == MAX_LOAD_SEGMENTS
            || mapped_start < address_start
            || segment.memory_size == 0
            || segment.file_size > segment.memory_size
            || segment
                .offset
                .checked_add(segment.file_size)
                .is_none_or(|end| end > file_length)
            || end > address_limit
            || segment.flags & 4 == 0
            || segment.flags & !7 != 0
            || segment.flags & 3 == 3
            || !alignment.is_power_of_two()
            || (segment.virtual_address.wrapping_sub(segment.offset)) & (alignment - 1) != 0
            || ranges[..load_count]
                .iter()
                .any(|(start, prior_end)| mapped_start < *prior_end && *start < mapped_end)
        {
            crate::fatal("AArch64 backed ELF layout rejected");
        }
        executable_entry |= segment.flags & 1 != 0 && (virtual_start..end).contains(&entry);
        if ph_offset >= segment.offset
            && ph_offset
                .checked_add(ph_bytes)
                .is_some_and(|ph_end| ph_end <= segment.offset.saturating_add(segment.file_size))
        {
            phdr = virtual_start + (ph_offset - segment.offset);
        }
        ranges[load_count] = (mapped_start, mapped_end);
        image_end = image_end.max(mapped_end);
        load_count += 1;
    }
    if load_count == 0 || !executable_entry {
        crate::fatal("AArch64 backed ELF lacks executable entry");
    }
    for segment in elf
        .program_headers()
        .filter(|header| header.segment_type == PT_LOAD)
    {
        let virtual_start = base + segment.virtual_address;
        let mapped_start = virtual_start & !(PAGE_SIZE - 1);
        let mapped_end = (virtual_start + segment.memory_size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let file_end = virtual_start + segment.file_size;
        let mut page_address = mapped_start;
        while page_address < mapped_end {
            let frame = crate::mm::allocate_frame()
                .unwrap_or_else(|| crate::fatal("AArch64 backed process page OOM"));
            unsafe { ptr::write_bytes(frame as *mut u8, 0, PAGE_SIZE as usize) };
            let copy_start = page_address.max(virtual_start);
            let copy_end = (page_address + PAGE_SIZE).min(file_end);
            if copy_start < copy_end {
                let source_offset = segment.offset + (copy_start - virtual_start);
                let destination = unsafe {
                    core::slice::from_raw_parts_mut(
                        (frame as *mut u8).add((copy_start - page_address) as usize),
                        (copy_end - copy_start) as usize,
                    )
                };
                if crate::vfs::read_only_backing(backing, source_offset, destination)
                    != Some(destination.len())
                {
                    crate::fatal("AArch64 backed ELF page read failed");
                }
            }
            if segment.flags & 1 != 0 {
                crate::arch::sync_user_code(frame);
            }
            crate::arch::map_user_page_in(
                root,
                page_address,
                frame,
                segment.flags & 2 != 0,
                segment.flags & 1 != 0,
            );
            page_address += PAGE_SIZE;
        }
    }
    MappedImage {
        entry,
        image_end,
        phdr,
        phnum: ph_num,
    }
}

fn load_process_image_backed(
    headers: &[u8],
    file_length: u64,
    backing: crate::vfs::ReadOnlyFileBacking,
    base: u64,
    loader: &[u8],
) -> LoadedProcess {
    let root = crate::arch::new_user_address_space();
    let application = map_backed_image(
        headers,
        file_length,
        backing,
        root,
        base,
        crate::arch::USER_ADDRESS_BASE,
        crate::arch::USER_IMAGE_LIMIT,
    );
    let mapped_loader = map_image(
        loader,
        root,
        ET_DYN,
        DYNAMIC_LOADER_BASE,
        DYNAMIC_LOADER_BASE,
        crate::arch::USER_MMAP_LIMIT,
    );
    finish_loaded_process(root, application, mapped_loader.entry, DYNAMIC_LOADER_BASE)
}

fn load_process_image(
    bytes: &[u8],
    expected_type: u16,
    base: u64,
    interpreter: Option<(&[u8], u64)>,
) -> LoadedProcess {
    if crate::arch::USER_STACK_TOP - USER_STACK_PAGES as u64 * PAGE_SIZE
        < crate::arch::USER_STACK_BOTTOM
    {
        crate::fatal("AArch64 process stack exceeds reserved arena");
    }
    let root = crate::arch::new_user_address_space();
    let application = map_image(
        bytes,
        root,
        expected_type,
        base,
        crate::arch::USER_ADDRESS_BASE,
        crate::arch::USER_IMAGE_LIMIT,
    );
    let (initial_entry, interpreter_base) = if let Some((loader, loader_base)) = interpreter {
        let mapped = map_image(
            loader,
            root,
            ET_DYN,
            loader_base,
            loader_base,
            crate::arch::USER_MMAP_LIMIT,
        );
        (mapped.entry, loader_base)
    } else {
        (application.entry, 0)
    };
    finish_loaded_process(root, application, initial_entry, interpreter_base)
}

fn finish_loaded_process(
    root: u64,
    application: MappedImage,
    initial_entry: u64,
    interpreter_base: u64,
) -> LoadedProcess {
    let mut startup_frame = 0u64;
    let mut stack_frames = [0u64; USER_STACK_PAGES];
    for page in 0..USER_STACK_PAGES {
        let stack_frame = crate::mm::allocate_frame()
            .unwrap_or_else(|| crate::fatal("AArch64 process stack OOM"));
        unsafe { ptr::write_bytes(stack_frame as *mut u8, 0, PAGE_SIZE as usize) };
        let address = crate::arch::USER_STACK_TOP - (page as u64 + 1) * PAGE_SIZE;
        crate::arch::map_user_page_in(root, address, stack_frame, true, false);
        stack_frames[USER_STACK_PAGES - page - 1] = stack_frame;
        if page + 1 == USER_STACK_PAGES {
            startup_frame = stack_frame;
        }
    }
    LoadedProcess {
        root,
        entry: application.entry,
        initial_entry,
        interpreter_base,
        image_end: application.image_end,
        startup_frame,
        stack_frames,
        phdr: application.phdr,
        phnum: application.phnum,
    }
}

fn build_sysv_startup(
    process: &LoadedProcess,
    argv_values: &[&[u8]],
    env_values: &[&[u8]],
    uid: u32,
    gid: u32,
) -> Option<SysvStartup> {
    if argv_values.is_empty()
        || argv_values.len() > SYSV_MAX_ARGUMENTS
        || env_values.len() > SYSV_MAX_ENVIRONMENT
        || argv_values
            .iter()
            .chain(env_values.iter())
            .any(|value| value.is_empty() || value.contains(&0))
        || env_values.iter().any(|value| {
            value
                .iter()
                .position(|byte| *byte == b'=')
                .is_none_or(|equals| equals == 0)
        })
    {
        return None;
    }

    let stack_bottom = crate::arch::USER_STACK_TOP - USER_STACK_PAGES as u64 * PAGE_SIZE;
    let mut cursor = crate::arch::USER_STACK_TOP;
    let mut argv = [0u64; SYSV_MAX_ARGUMENTS];
    let mut envp = [0u64; SYSV_MAX_ENVIRONMENT];
    for (index, value) in argv_values.iter().enumerate() {
        cursor = cursor.checked_sub(value.len() as u64 + 1)?;
        write_user_stack(&process.stack_frames, cursor, value)?;
        write_user_stack(&process.stack_frames, cursor + value.len() as u64, &[0])?;
        argv[index] = cursor;
    }
    for (index, value) in env_values.iter().enumerate() {
        cursor = cursor.checked_sub(value.len() as u64 + 1)?;
        write_user_stack(&process.stack_frames, cursor, value)?;
        write_user_stack(&process.stack_frames, cursor + value.len() as u64, &[0])?;
        envp[index] = cursor;
    }
    cursor = cursor.checked_sub(8)?;
    write_user_stack(&process.stack_frames, cursor, b"aarch64\0")?;
    let platform = cursor;

    let mut random = [0u8; 16];
    if !crate::aarch64_virtio_rng::fill(&mut random) {
        return None;
    }
    cursor = cursor.checked_sub(random.len() as u64)?;
    write_user_stack(&process.stack_frames, cursor, &random)?;
    let random_address = cursor;

    let auxv = [
        (3u64, process.phdr),
        (4, 56),
        (5, process.phnum),
        (6, PAGE_SIZE),
        (7, process.interpreter_base),
        (8, 0),
        (9, process.entry),
        (11, u64::from(uid)),
        (12, u64::from(uid)),
        (13, u64::from(gid)),
        (14, u64::from(gid)),
        (15, platform),
        (16, 0),
        (17, 100),
        (23, 0),
        (25, random_address),
        (26, 0),
        (31, argv[0]),
        (0, 0),
    ];
    const MAX_WORDS: usize = 1 + SYSV_MAX_ARGUMENTS + 1 + SYSV_MAX_ENVIRONMENT + 1 + 19 * 2;
    let mut words = [0u64; MAX_WORDS];
    let mut count = 0usize;
    words[count] = argv_values.len() as u64;
    count += 1;
    let argv_offset = count * 8;
    for pointer in &argv[..argv_values.len()] {
        words[count] = *pointer;
        count += 1;
    }
    count += 1;
    let envp_offset = count * 8;
    for pointer in &envp[..env_values.len()] {
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
    if cursor < stack_bottom {
        return None;
    }
    for (index, word) in words[..count].iter().enumerate() {
        write_user_stack(
            &process.stack_frames,
            cursor + index as u64 * 8,
            &word.to_le_bytes(),
        )?;
    }
    Some(SysvStartup {
        stack_pointer: cursor,
        argc: argv_values.len() as u64,
        argv: cursor + argv_offset as u64,
        envp: cursor + envp_offset as u64,
    })
}

fn write_user_stack(frames: &[u64; USER_STACK_PAGES], address: u64, bytes: &[u8]) -> Option<()> {
    let stack_bottom = crate::arch::USER_STACK_TOP - USER_STACK_PAGES as u64 * PAGE_SIZE;
    let end = address.checked_add(bytes.len() as u64)?;
    if address < stack_bottom || end > crate::arch::USER_STACK_TOP {
        return None;
    }
    let mut source_offset = 0usize;
    let mut stack_offset = (address - stack_bottom) as usize;
    while source_offset < bytes.len() {
        let frame_index = stack_offset / PAGE_SIZE as usize;
        let frame_offset = stack_offset % PAGE_SIZE as usize;
        let count = (PAGE_SIZE as usize - frame_offset).min(bytes.len() - source_offset);
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
