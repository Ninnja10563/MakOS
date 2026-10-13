use makos_process_table::{ProcessTable, WaitResult};
use std::sync::Mutex;

const MAX_PROCESSES: usize = 128;
const TOOLCHAIN_ELF: &[u8] = b"host ELF-load adapter";

#[derive(Clone, Copy, Debug, PartialEq)]
enum ProcessRole { None, Shell, Native, Toolchain }

#[derive(Clone, Copy)]
struct ContextSlot {
    pid: u64,
    group_pid: u64,
    role: ProcessRole,
    startup_ready: bool,
    affinity_mask: u8,
}
impl ContextSlot {
    const EMPTY: Self = Self {
        pid: 0, group_pid: 0, role: ProcessRole::None,
        startup_ready: false, affinity_mask: 0,
    };
}

struct SchedulerState {
    table: ProcessTable<MAX_PROCESSES>,
    contexts: [ContextSlot; MAX_PROCESSES],
    prepublication_wakes: usize,
    publication_wakes: usize,
    selected: Option<u64>,
    last_created: u64,
    reclaimed: Vec<(u64, u64, u64)>,
}
static STATE: Mutex<Option<SchedulerState>> = Mutex::new(None);

fn with_state<R>(function: impl FnOnce(&mut SchedulerState) -> R) -> R {
    function(STATE.lock().unwrap().as_mut().unwrap())
}

mod security {
    use std::sync::Mutex;
    include!("security_production.rs");
    struct SessionState {
        active: bool, generation: u64, uid: u32, gid: u32,
        bindings: [CredentialBinding; MAX_SESSION_PROCESSES],
    }
    static SESSION: Mutex<SessionState> = Mutex::new(SessionState {
        active: false, generation: 17, uid: INIT_UID, gid: INIT_GID,
        bindings: [CredentialBinding::EMPTY; MAX_SESSION_PROCESSES],
    });
    fn with_session<R>(function: impl FnOnce(&mut SessionState) -> R) -> R {
        function(&mut SESSION.lock().unwrap())
    }
    pub fn reset(active: bool) {
        with_session(|session| {
            session.active = active;
            session.bindings.fill(CredentialBinding::EMPTY);
        });
    }
    pub fn exhaust_bindings() {
        for pid in 1000..1000 + MAX_SESSION_PROCESSES as u64 {
            assert!(register_session_process(pid, SessionProcessRole::Native));
        }
    }
}

mod vfs { pub const MAX_PATH_BYTES: usize = 4096; }
mod graphics { pub fn reset_gpu_service_affinity_evidence() {} }
fn reset_toolchain_smp_evidence() {}
fn current_pid() -> u64 { with_state(|state| state.table.current_pid_on(0).unwrap_or(0)) }
fn current_app_role() -> ProcessRole {
    let pid = current_pid();
    with_state(|state| state.contexts.iter().find(|slot| slot.pid == pid)
               .map_or(ProcessRole::None, |slot| slot.role))
}
fn fatal(message: &str) -> ! { panic!("{message}") }
#[macro_export]
macro_rules! serial_println { ($($value:tt)*) => { { let _ = format_args!($($value)*); } }; }

struct LoadedProcess { root: u64, entry: u64 }

fn spawn_process_sysv(parent: u64, elf: &[u8], role: ProcessRole,
                      argv: &[&[u8]], envp: &[&[u8]]) -> Option<(u64, LoadedProcess)> {
    assert_eq!(elf, TOOLCHAIN_ELF);
    assert_eq!(argv.len(), 2);
    assert_eq!(argv[0], b"/system/aarch64-toolchain");
    assert!(envp == [b"MODE=fixture".as_slice()] || envp == [b"MODE=build".as_slice()]);
    let root = 0x22_000;
    let pid = with_state(|state| {
        let pid = state.table.spawn(parent, root).unwrap();
        let slot = state.contexts.iter_mut().find(|slot| slot.pid == 0).unwrap();
        *slot = ContextSlot {
            pid, group_pid: pid, role,
            startup_ready: initial_startup_ready(role), affinity_mask: 0x2,
        };
        state.last_created = pid;
        pid
    });
    // Force a legal competing scheduler lookup BEFORE this loader returns to
    // the production spawn routine, hence before its credential registration.
    // This represents an unrelated SGI/timer, not just the creation wake.
    notify_idle_cpus();
    Some((pid, LoadedProcess { root, entry: 0x1000_0000 }))
}

fn notify_idle_cpus() {
    with_state(|state| {
        let ready = state.contexts.iter().any(|slot| {
            slot.role == ProcessRole::Toolchain && slot.startup_ready
        });
        if ready { state.publication_wakes += 1; } else { state.prepublication_wakes += 1; }
        for cpu in 1..4 {
            let contexts = &state.contexts;
            let selected = state.table.schedule_next_where_on(cpu, |info| {
                let slot = contexts.iter().find(|slot| slot.pid == info.pid);
                startup_dispatch_allowed(slot)
                    && slot.is_some_and(|slot| slot.affinity_mask & (1 << cpu) != 0)
            });
            if let Some(info) = selected {
                let credentials = security::credentials_for_process(info.pid)
                    .expect("AP must never execute before its credentials are bound");
                assert_eq!(credentials.capabilities, security::CAP_CONSOLE | security::CAP_FILE_WRITE);
                assert_eq!((credentials.uid, credentials.gid), (1000, 1000));
                assert_eq!(state.table.running_cpu(info.pid), Some(cpu));
                state.selected = Some(info.pid);
            }
        }
    });
}

fn cleanup_reaped(pid: u64, root: u64, status: u64) {
    security::clear_process_credentials(pid);
    with_state(|state| state.reclaimed.push((pid, root, status)));
}
include!("process_production.rs");

fn setup(active: bool) {
    security::reset(active);
    let mut table = ProcessTable::new();
    let parent = table.spawn(0, 0x11_000).unwrap();
    assert_eq!(parent, 1);
    table.activate_on(0, parent).unwrap();
    let mut contexts = [ContextSlot::EMPTY; MAX_PROCESSES];
    contexts[0] = ContextSlot {
        pid: parent, group_pid: parent, role: ProcessRole::Shell,
        startup_ready: true, affinity_mask: 1,
    };
    *STATE.lock().unwrap() = Some(SchedulerState {
        table, contexts, prepublication_wakes: 0, publication_wakes: 0,
        selected: None, last_created: 0, reclaimed: Vec::new(),
    });
}

#[test]
fn first_ap_dispatch_observes_bound_credentials() {
    for fixture in [true, false] {
        setup(true);
        let pid = spawn_toolchain(b"/home/user/generated.build", fixture).unwrap();
        with_state(|state| {
            assert_eq!(state.prepublication_wakes, 1);
            assert_eq!(state.publication_wakes, 1, "published startup must send a fresh wake");
            assert_eq!(state.selected, Some(pid));
            assert_eq!(state.table.running_cpu(pid), Some(1));
            assert_eq!(state.table.current_pid_on(0), Some(1));
            assert!(state.reclaimed.is_empty());
        });
    }
}

#[test]
fn refused_credentials_reap_pending_task_and_root() {
    for exhausted in [false, true] {
        setup(exhausted);
        if exhausted { security::exhaust_bindings(); }
        assert_eq!(spawn_toolchain(b"/home/user/generated.build", true), None);
        let pid = with_state(|state| {
            let pid = state.last_created;
            assert_eq!(state.table.counts(), (1, 1, 0));
            assert_eq!(state.table.get(pid), None);
            assert_eq!(state.table.running_cpu(pid), None);
            assert!(!state.contexts.iter().any(|slot| slot.pid == pid));
            assert_eq!(state.reclaimed, [(pid, 0x22_000, 126)], "refused startup must reclaim its root");
            assert_eq!(state.prepublication_wakes, 1);
            assert_eq!(state.publication_wakes, 0);
            assert_eq!(state.selected, None);
            pid
        });
        assert_eq!(security::credentials_for_process(pid), None);
    }
}

#[test]
fn invalid_requests_never_create_a_pending_child() {
    setup(true);
    for path in [b"/home/user/".as_slice(), b"/system/bad", b"/home/user/a\0b"] {
        assert_eq!(spawn_toolchain(path, false), None);
    }
    let mut too_long = b"/home/user/".to_vec();
    too_long.resize(vfs::MAX_PATH_BYTES, b'x');
    assert_eq!(spawn_toolchain(&too_long, false), None);
    with_state(|state| state.contexts[0].role = ProcessRole::Native);
    assert_eq!(spawn_toolchain(b"/home/user/generated.build", false), None);
    with_state(|state| {
        assert_eq!(state.table.counts(), (1, 1, 0));
        assert_eq!(state.last_created, 0);
        assert_eq!(state.prepublication_wakes, 0);
    });
}

#[test]
fn publication_rejects_wrong_role_group_and_state() {
    for invalid in 0..6 {
        setup(true);
        let pid = with_state(|state| {
            let pid = state.table.spawn(1, 0x33_000).unwrap();
            state.contexts[1] = ContextSlot {
                pid, group_pid: if invalid == 1 { 1 } else { pid },
                role: if invalid == 0 { ProcessRole::Native } else { ProcessRole::Toolchain },
                startup_ready: invalid == 2, affinity_mask: 0x2,
            };
            if invalid == 3 { state.table.activate_on(1, pid).unwrap(); }
            if invalid == 4 { state.table.terminate(pid, 9).unwrap(); }
            if invalid == 5 { state.contexts[1] = ContextSlot::EMPTY; }
            pid
        });
        assert!(!publish_toolchain_startup(pid));
    }
    assert!(!publish_toolchain_startup(u64::MAX));
}

#[test]
fn cleanup_adapter_releases_exact_resource_and_credentials() {
    setup(true);
    assert!(security::register_session_process(2, security::SessionProcessRole::Toolchain));
    assert!(security::credentials_for_process(2).is_some());
    cleanup_reaped(2, 0x99_000, 126);
    assert_eq!(security::credentials_for_process(2), None);
    with_state(|state| assert_eq!(state.reclaimed, [(2, 0x99_000, 126)]));
}

#[test]
fn ordinary_roles_keep_existing_readiness_and_capability_policy() {
    setup(true);
    assert!(!initial_startup_ready(ProcessRole::Toolchain));
    assert!(initial_startup_ready(ProcessRole::Native));
    assert!(initial_startup_ready(ProcessRole::Shell));
    assert!(!startup_dispatch_allowed(None));
    assert!(!startup_dispatch_allowed(Some(&ContextSlot::EMPTY)));
    use security::SessionProcessRole::*;
    for (index, role) in [Browser, Files, TextEdit, Python, Nano, Native,
                         MuslDynamicProbe, Toolchain, NativeIpc, Firefox, FirefoxProbe]
        .into_iter().enumerate() {
        let pid = index as u64 + 100;
        assert!(security::register_session_process(pid, role));
        let credentials = security::credentials_for_process(pid).unwrap();
        assert_eq!((credentials.uid, credentials.gid), (1000, 1000));
        if role == Native {
            assert_eq!(credentials.capabilities, security::CAP_CONSOLE);
        }
        if role == Toolchain {
            assert_eq!(credentials.capabilities, security::CAP_CONSOLE | security::CAP_FILE_WRITE);
        }
    }
}
