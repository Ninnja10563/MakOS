use core::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_arch = "aarch64")]
use core::cell::UnsafeCell;

pub const ROOT_UID: u32 = 0;
pub const INIT_UID: u32 = 1000;
pub const INIT_GID: u32 = 1000;
pub const WORKER_UID: u32 = 1001;
pub const WORKER_GID: u32 = 1001;
pub const COMPAT_UID: u32 = 1002;
pub const COMPAT_GID: u32 = 1002;
pub const BROWSER_UID: u32 = 1003;
pub const BROWSER_GID: u32 = 1003;
pub const CAP_CONSOLE: u64 = 1 << 0;
pub const CAP_GRAPHICS: u64 = 1 << 1;
pub const CAP_IPC: u64 = 1 << 2;
pub const CAP_NETWORK: u64 = 1 << 3;
pub const CAP_PROCESS: u64 = 1 << 4;
pub const CAP_FILE_WRITE: u64 = 1 << 5;
pub const CAP_INPUT: u64 = 1 << 6;
pub const CAP_SYNC: u64 = 1 << 7;
pub const CAP_AUDIO: u64 = 1 << 8;
pub const CAP_SERVICE_PUBLISH: u64 = 1 << 9;

static POINTER_DENIAL_REPORTED: AtomicBool = AtomicBool::new(false);

#[cfg(target_arch = "x86_64")]
static INIT_AUTHENTICATED: AtomicBool = AtomicBool::new(false);
#[cfg(target_arch = "x86_64")]
const LOGIN_NAME: &[u8] = b"marcus";
#[cfg(target_arch = "x86_64")]
const PASSWORD_SALT: &[u8] = b"MakOS-v1-user:";
#[cfg(target_arch = "x86_64")]
const PASSWORD_ITERATIONS: u32 = 100_000;
#[cfg(target_arch = "x86_64")]
const PASSWORD_HASH: [u8; 32] = [
    0x96, 0x78, 0xbc, 0x48, 0x44, 0xda, 0x75, 0x09, 0xff, 0xc3, 0x43, 0x0e, 0x8a, 0x56, 0x7c, 0x21,
    0xbb, 0xea, 0xc4, 0x92, 0xe6, 0xa2, 0x31, 0xdd, 0xec, 0xe5, 0x8a, 0x73, 0xb3, 0x7f, 0x68, 0x4b,
];

fn current_pid() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        crate::scheduler::current_pid()
    }
    #[cfg(target_arch = "aarch64")]
    {
        crate::aarch64_process::current_pid()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Credentials {
    pub uid: u32,
    pub gid: u32,
    pub capabilities: u64,
}

pub const INIT_CREDENTIALS: Credentials = Credentials {
    uid: INIT_UID,
    gid: INIT_GID,
    capabilities: CAP_CONSOLE
        | CAP_GRAPHICS
        | CAP_IPC
        | CAP_NETWORK
        | CAP_PROCESS
        | CAP_FILE_WRITE
        | CAP_INPUT
        | CAP_SYNC
        | CAP_AUDIO
        | CAP_SERVICE_PUBLISH,
};
pub const WORKER_CREDENTIALS: Credentials = Credentials {
    uid: WORKER_UID,
    gid: WORKER_GID,
    capabilities: CAP_CONSOLE | CAP_GRAPHICS | CAP_SYNC,
};
pub const COMPAT_CREDENTIALS: Credentials = Credentials {
    uid: COMPAT_UID,
    gid: COMPAT_GID,
    capabilities: CAP_CONSOLE | CAP_SYNC,
};
pub const BROWSER_CREDENTIALS: Credentials = Credentials {
    uid: BROWSER_UID,
    gid: BROWSER_GID,
    capabilities: CAP_GRAPHICS | CAP_NETWORK | CAP_INPUT,
};
const LOGIN_CREDENTIALS: Credentials = Credentials {
    uid: 65_534,
    gid: 65_534,
    capabilities: CAP_CONSOLE | CAP_INPUT,
};
const UNPRIVILEGED_CREDENTIALS: Credentials = Credentials {
    uid: 65_534,
    gid: 65_534,
    capabilities: 0,
};

#[cfg(target_arch = "aarch64")]
const MAX_SESSION_PROCESSES: usize = 32;

#[cfg(target_arch = "aarch64")]
#[derive(Clone, Copy)]
struct CredentialBinding {
    pid: u64,
    credentials: Credentials,
    generation: u64,
}

#[cfg(target_arch = "aarch64")]
impl CredentialBinding {
    const EMPTY: Self = Self {
        pid: 0,
        credentials: UNPRIVILEGED_CREDENTIALS,
        generation: 0,
    };
}

#[cfg(target_arch = "aarch64")]
struct SessionState {
    active: bool,
    generation: u64,
    username: [u8; makos_accounts::USERNAME_BYTES],
    username_length: usize,
    uid: u32,
    gid: u32,
    bindings: [CredentialBinding; MAX_SESSION_PROCESSES],
}

#[cfg(target_arch = "aarch64")]
impl SessionState {
    const fn new() -> Self {
        Self {
            active: false,
            generation: 0,
            username: [0; makos_accounts::USERNAME_BYTES],
            username_length: 0,
            uid: 65_534,
            gid: 65_534,
            bindings: [CredentialBinding::EMPTY; MAX_SESSION_PROCESSES],
        }
    }
}

#[cfg(target_arch = "aarch64")]
struct LockedSession {
    lock: AtomicBool,
    state: UnsafeCell<SessionState>,
}

#[cfg(target_arch = "aarch64")]
unsafe impl Sync for LockedSession {}

#[cfg(target_arch = "aarch64")]
static SESSION: LockedSession = LockedSession {
    lock: AtomicBool::new(false),
    state: UnsafeCell::new(SessionState::new()),
};

#[cfg(target_arch = "aarch64")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionProcessRole {
    Browser,
    Files,
    TextEdit,
    Python,
    Nano,
    Native,
    Toolchain,
    NativeIpc,
    Firefox,
}

pub fn has_capability(capability: u64) -> bool {
    credentials().capabilities & capability == capability
}

pub fn file_access(mode: u32, owner: u32, group: u32, write: bool) -> bool {
    let credentials = credentials();
    #[cfg(target_arch = "aarch64")]
    if credentials.capabilities & CAP_FILE_WRITE != 0 {
        return true;
    }
    let shift = if credentials.uid == owner {
        6
    } else if credentials.gid == group {
        3
    } else {
        0
    };
    let permission = if write { 0o2 } else { 0o4 };
    ((mode >> shift) & permission) != 0
}

pub fn credentials() -> Credentials {
    #[cfg(target_arch = "x86_64")]
    {
        credentials_for_pid(current_pid(), INIT_AUTHENTICATED.load(Ordering::Acquire))
    }
    #[cfg(target_arch = "aarch64")]
    {
        let pid = current_pid();
        credentials_for_process(pid).unwrap_or(UNPRIVILEGED_CREDENTIALS)
    }
}

pub fn session_generation() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        u64::from(INIT_AUTHENTICATED.load(Ordering::Acquire))
    }
    #[cfg(target_arch = "aarch64")]
    {
        with_session(|session| session.generation)
    }
}

#[cfg(target_arch = "aarch64")]
pub fn credentials_for_process(pid: u64) -> Option<Credentials> {
    if pid == 0 {
        return None;
    }
    with_session(|session| {
        if let Some(binding) = session
            .bindings
            .iter()
            .find(|binding| binding.pid == pid && binding.generation == session.generation)
        {
            return Some(binding.credentials);
        }
        (pid == 1).then_some(if session.active {
            Credentials {
                uid: session.uid,
                gid: session.gid,
                capabilities: INIT_CREDENTIALS.capabilities,
            }
        } else {
            LOGIN_CREDENTIALS
        })
    })
}

#[cfg(target_arch = "aarch64")]
pub fn may_signal_process(target_pid: u64) -> bool {
    let caller_pid = current_pid();
    if caller_pid == target_pid {
        return true;
    }
    let caller = credentials();
    let Some(target) = credentials_for_process(target_pid) else {
        return false;
    };
    caller.uid == ROOT_UID
        || caller.uid == target.uid
        || caller.capabilities & CAP_PROCESS == CAP_PROCESS
}

fn credentials_for_pid(pid: u64, authenticated: bool) -> Credentials {
    match pid {
        1 if !authenticated => LOGIN_CREDENTIALS,
        1 => INIT_CREDENTIALS,
        2 => WORKER_CREDENTIALS,
        3 | 4 => COMPAT_CREDENTIALS,
        5 => Credentials {
            uid: 998,
            gid: 998,
            capabilities: CAP_CONSOLE,
        },
        6 => Credentials {
            uid: INIT_UID,
            gid: INIT_GID,
            capabilities: CAP_CONSOLE | CAP_FILE_WRITE,
        },
        7 | 8 => Credentials {
            uid: 996,
            gid: 996,
            capabilities: CAP_CONSOLE,
        },
        _ => UNPRIVILEGED_CREDENTIALS,
    }
}

pub fn self_test() {
    let unknown = credentials_for_pid(u64::MAX, true);
    let kernel = credentials_for_pid(0, true);
    let authenticated_init = credentials_for_pid(1, true);
    if unknown.capabilities != 0
        || unknown.uid != 65_534
        || unknown.gid != 65_534
        || kernel.capabilities != 0
        || authenticated_init.capabilities != INIT_CREDENTIALS.capabilities
        || credentials_for_pid(1, false).capabilities != LOGIN_CREDENTIALS.capabilities
        || BROWSER_CREDENTIALS.capabilities != CAP_GRAPHICS | CAP_NETWORK | CAP_INPUT
        || BROWSER_CREDENTIALS.capabilities & (CAP_PROCESS | CAP_FILE_WRITE | CAP_AUDIO) != 0
    {
        crate::fatal("credential deny-by-default self-test failed");
    }
    crate::serial_println!(
        "MAKOS_CREDENTIAL_POLICY_OK unknown_pid=denied kernel_context=denied explicit_pid1=1 ambient_caps=0"
    );
}

#[cfg(target_arch = "x86_64")]
pub fn authenticate(username: &[u8], password: &[u8]) -> bool {
    if current_pid() != 1
        || INIT_AUTHENTICATED.load(Ordering::Acquire)
        || username != LOGIN_NAME
        || password.is_empty()
        || password.len() > 64
    {
        crate::log::audit(b"audit: authentication denied");
        return false;
    }
    let digest = makos_crypto::pbkdf2_hmac_sha256_32(password, PASSWORD_SALT, PASSWORD_ITERATIONS);
    let mut difference = 0u8;
    for index in 0..32 {
        difference |= digest[index] ^ PASSWORD_HASH[index];
    }
    if difference != 0 {
        crate::log::audit(b"audit: authentication denied");
        return false;
    }
    INIT_AUTHENTICATED.store(true, Ordering::Release);
    crate::graphics::hide_login();
    crate::serial_println!(
        "MAKOS_LOGIN_OK user=marcus uid={} gid={} session=1 password_hash=pbkdf2-hmac-sha256 iterations={} bad_password_denied=1",
        INIT_UID,
        INIT_GID,
        PASSWORD_ITERATIONS
    );
    crate::log::audit(b"audit: authentication accepted");
    true
}

#[cfg(target_arch = "aarch64")]
pub fn authenticate(username: &[u8], password: &[u8]) -> bool {
    if current_pid() != 1
        || password.is_empty()
        || password.len() > makos_accounts::PASSWORD_BYTES
        || with_session(|session| session.active)
    {
        crate::log::audit(b"audit: authentication denied");
        return false;
    }
    let Some(account) = crate::aarch64_accounts::authenticate(username, password) else {
        crate::log::audit(b"audit: authentication denied");
        return false;
    };
    if !with_session(|session| {
        if session.active {
            return false;
        }
        session.active = true;
        session.generation = session.generation.wrapping_add(1).max(1);
        session.username.fill(0);
        session.username[..account.username().len()].copy_from_slice(account.username());
        session.username_length = account.username().len();
        session.uid = account.uid;
        session.gid = account.gid;
        session.bindings.fill(CredentialBinding::EMPTY);
        session.bindings[0] = CredentialBinding {
            pid: 1,
            credentials: Credentials {
                uid: account.uid,
                gid: account.gid,
                capabilities: INIT_CREDENTIALS.capabilities,
            },
            generation: session.generation,
        };
        true
    }) {
        crate::log::audit(b"audit: authentication denied");
        return false;
    }
    crate::graphics::hide_login();
    let name = core::str::from_utf8(account.username()).unwrap_or("invalid");
    crate::serial_println!(
        "MAKOS_LOGIN_OK user={} uid={} gid={} session=1 credential=per-process password_hash=pbkdf2-hmac-sha256 iterations={} bad_password_denied=1",
        name,
        account.uid,
        account.gid,
        makos_accounts::PASSWORD_ITERATIONS
    );
    crate::log::audit(b"audit: authentication accepted");
    true
}

#[cfg(target_arch = "aarch64")]
pub fn register_session_process(pid: u64, role: SessionProcessRole) -> bool {
    if pid == 0 || pid == 1 {
        return false;
    }
    with_session(|session| {
        if !session.active {
            return false;
        }
        let capabilities = match role {
            SessionProcessRole::Browser => CAP_GRAPHICS | CAP_NETWORK | CAP_INPUT,
            SessionProcessRole::Files | SessionProcessRole::TextEdit => {
                CAP_GRAPHICS | CAP_INPUT | CAP_FILE_WRITE | CAP_CONSOLE
            }
            SessionProcessRole::Nano => CAP_FILE_WRITE | CAP_CONSOLE,
            SessionProcessRole::Python | SessionProcessRole::Native => CAP_CONSOLE,
            SessionProcessRole::Toolchain => CAP_CONSOLE | CAP_FILE_WRITE,
            SessionProcessRole::NativeIpc => {
                CAP_CONSOLE
                    | CAP_IPC
                    | CAP_SYNC
                    | CAP_FILE_WRITE
                    | CAP_NETWORK
                    | CAP_SERVICE_PUBLISH
            }
            SessionProcessRole::Firefox => {
                CAP_GRAPHICS
                    | CAP_NETWORK
                    | CAP_INPUT
                    | CAP_FILE_WRITE
                    | CAP_CONSOLE
                    | CAP_IPC
                    | CAP_SYNC
            }
        };
        let Some(binding) = session
            .bindings
            .iter_mut()
            .find(|binding| binding.pid == pid || binding.pid == 0)
        else {
            return false;
        };
        *binding = CredentialBinding {
            pid,
            credentials: Credentials {
                uid: session.uid,
                gid: session.gid,
                capabilities,
            },
            generation: session.generation,
        };
        true
    })
}

#[cfg(target_arch = "aarch64")]
pub fn clear_process_credentials(pid: u64) {
    with_session(|session| {
        for binding in &mut session.bindings {
            if binding.pid == pid {
                *binding = CredentialBinding::EMPTY;
            }
        }
    });
}

#[cfg(target_arch = "aarch64")]
pub fn inherit_process_credentials(parent_pid: u64, child_pid: u64) -> bool {
    if parent_pid == 0 || child_pid == 0 || parent_pid == child_pid {
        return false;
    }
    with_session(|session| {
        let Some(parent) =
            session.bindings.iter().copied().find(|binding| {
                binding.pid == parent_pid && binding.generation == session.generation
            })
        else {
            return false;
        };
        let Some(slot) = session.bindings.iter_mut().find(|binding| binding.pid == 0) else {
            return false;
        };
        *slot = CredentialBinding {
            pid: child_pid,
            ..parent
        };
        true
    })
}

#[cfg(target_arch = "aarch64")]
pub fn session_username(output: &mut [u8]) -> Option<usize> {
    with_session(|session| {
        if !session.active || output.len() < session.username_length {
            return None;
        }
        output[..session.username_length]
            .copy_from_slice(&session.username[..session.username_length]);
        Some(session.username_length)
    })
}

#[cfg(target_arch = "aarch64")]
pub fn session_active() -> bool {
    with_session(|session| session.active)
}

#[cfg(target_arch = "aarch64")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddUserError {
    Permission,
    InvalidUsername,
    InvalidPassword,
    Exists,
    Full,
    Storage,
}

#[cfg(target_arch = "aarch64")]
pub fn add_user(username: &[u8], password: &[u8]) -> Result<(u32, u32), AddUserError> {
    if current_pid() != 1 || !with_session(|session| session.active) {
        return Err(AddUserError::Permission);
    }
    add_user_to_active_session(username, password)
}

/// Trusted compositor entry for Settings. Unlike the EL0 syscall path, input
/// may arrive while another session process is scheduled; authority comes
/// from this kernel-only call site plus an active authenticated session.
#[cfg(target_arch = "aarch64")]
pub(crate) fn add_user_from_system_settings(
    username: &[u8],
    password: &[u8],
) -> Result<(u32, u32), AddUserError> {
    if !with_session(|session| session.active) {
        return Err(AddUserError::Permission);
    }
    add_user_to_active_session(username, password)
}

#[cfg(target_arch = "aarch64")]
fn add_user_to_active_session(
    username: &[u8],
    password: &[u8],
) -> Result<(u32, u32), AddUserError> {
    let account =
        crate::aarch64_accounts::add_user(username, password).map_err(|error| match error {
            crate::aarch64_accounts::AddUserError::Account(
                makos_accounts::AddError::InvalidUsername,
            ) => AddUserError::InvalidUsername,
            crate::aarch64_accounts::AddUserError::Account(
                makos_accounts::AddError::InvalidPassword,
            ) => AddUserError::InvalidPassword,
            crate::aarch64_accounts::AddUserError::Account(
                makos_accounts::AddError::AlreadyExists,
            ) => AddUserError::Exists,
            crate::aarch64_accounts::AddUserError::Account(
                makos_accounts::AddError::Full | makos_accounts::AddError::IdExhausted,
            ) => AddUserError::Full,
            crate::aarch64_accounts::AddUserError::Persistence => AddUserError::Storage,
        })?;
    crate::log::audit(b"audit: account created");
    Ok((account.uid, account.gid))
}

#[cfg(target_arch = "aarch64")]
pub fn sign_out() -> bool {
    if current_pid() != 1 || !with_session(|session| session.active) {
        return false;
    }
    let terminated = crate::aarch64_process::terminate_session_apps();
    let closed_files = crate::vfs::close_all(1);
    let closed_ipc_handles = crate::ipc::close_all(1);
    let closed_surfaces = crate::graphics::reset_session_surfaces(1);
    crate::aarch64_clipboard::clear();
    let generation = with_session(|session| {
        let generation = session.generation;
        session.active = false;
        session.username.fill(0);
        session.username_length = 0;
        session.uid = 65_534;
        session.gid = 65_534;
        session.bindings.fill(CredentialBinding::EMPTY);
        generation
    });
    crate::graphics::show_login();
    crate::serial_println!(
        "MAKOS_SIGNOUT_OK generation={} apps_terminated={} pid1_files={} pid1_ipc_handles={} pid1_surfaces={} credentials=cleared login=ready",
        generation,
        terminated,
        closed_files,
        closed_ipc_handles,
        closed_surfaces
    );
    crate::log::audit(b"audit: session signed out");
    true
}

pub fn report_pointer_denial() {
    if !POINTER_DENIAL_REPORTED.swap(true, Ordering::AcqRel) {
        let credentials = credentials();
        crate::serial_println!(
            "MAKOS_SECURITY_OK uid={} gid={} caps={:#x} kernel_pointer_denied=1",
            credentials.uid,
            credentials.gid,
            credentials.capabilities
        );
    }
}

#[cfg(target_arch = "aarch64")]
fn with_session<R>(function: impl FnOnce(&mut SessionState) -> R) -> R {
    while SESSION
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *SESSION.state.get() });
    SESSION.lock.store(false, Ordering::Release);
    result
}
