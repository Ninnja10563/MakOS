use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use makos_accounts::{
    AccountDb, AccountRecord, AddError, AuthenticatedAccount, PASSWORD_ITERATIONS,
};

const MAX_ACCOUNTS: usize = 8;
const ACCOUNT_BYTES: usize = AccountDb::<MAX_ACCOUNTS>::encoded_len();
const LEGACY_SALT: &[u8] = b"MakOS-v1-user:";
const LEGACY_HASH: [u8; 32] = [
    0x96, 0x78, 0xbc, 0x48, 0x44, 0xda, 0x75, 0x09, 0xff, 0xc3, 0x43, 0x0e, 0x8a, 0x56, 0x7c, 0x21,
    0xbb, 0xea, 0xc4, 0x92, 0xe6, 0xa2, 0x31, 0xdd, 0xec, 0xe5, 0x8a, 0x73, 0xb3, 0x7f, 0x68, 0x4b,
];

struct State {
    database: Option<AccountDb<MAX_ACCOUNTS>>,
}

struct LockedState {
    lock: AtomicBool,
    state: UnsafeCell<State>,
}

unsafe impl Sync for LockedState {}

static STATE: LockedState = LockedState {
    lock: AtomicBool::new(false),
    state: UnsafeCell::new(State { database: None }),
};
static SALT_NONCE: AtomicU64 = AtomicU64::new(1);

pub fn initialize() {
    let mut stored = [0u8; crate::vfs::MAX_FILE_BYTES];
    let (database, created) = match crate::vfs::system_account_snapshot(&mut stored) {
        Some(length) => (
            AccountDb::<MAX_ACCOUNTS>::decode(&stored[..length])
                .unwrap_or_else(|_| crate::fatal("persistent account database invalid")),
            false,
        ),
        None => {
            let database = AccountDb::with_initial(default_account());
            if !persist(&database) {
                crate::fatal("default account database persistence failed");
            }
            (database, true)
        }
    };
    let count = database.len();
    with_state(|state| state.database = Some(database));
    crate::serial_println!(
        "MAKOS_ACCOUNTS_OK path=/home/user/.accounts hidden=1 persisted=1 created={} users={} format=v1 integrity=sha256 password=pbkdf2-hmac-sha256 iterations={}",
        u8::from(created),
        count,
        PASSWORD_ITERATIONS,
    );
}

pub fn authenticate(username: &[u8], password: &[u8]) -> Option<AuthenticatedAccount> {
    with_state(|state| state.database.as_ref()?.authenticate(username, password))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddUserError {
    Account(AddError),
    Persistence,
}

pub fn add_user(username: &[u8], password: &[u8]) -> Result<AuthenticatedAccount, AddUserError> {
    let nonce = SALT_NONCE.fetch_add(1, Ordering::AcqRel);
    let ticks = crate::arch::monotonic_ticks();
    let mut entropy = [0u8; 16];
    entropy[..8].copy_from_slice(&ticks.to_le_bytes());
    entropy[8..].copy_from_slice(&nonce.to_le_bytes());
    with_state(|state| {
        let current = state
            .database
            .as_ref()
            .unwrap_or_else(|| crate::fatal("account database not initialized"));
        let mut candidate = current.clone();
        let record = *candidate
            .add_user(username, password, &entropy)
            .map_err(AddUserError::Account)?;
        if !persist(&candidate) {
            return Err(AddUserError::Persistence);
        }
        *state
            .database
            .as_mut()
            .unwrap_or_else(|| crate::fatal("account database disappeared")) = candidate;
        Ok(AuthenticatedAccount::from(record))
    })
}

fn default_account() -> AccountRecord {
    AccountRecord::from_hash(
        b"marcus",
        crate::security::INIT_UID,
        crate::security::INIT_GID,
        PASSWORD_ITERATIONS,
        LEGACY_SALT,
        LEGACY_HASH,
    )
    .unwrap_or_else(|_| crate::fatal("compiled default account invalid"))
}

fn persist(database: &AccountDb<MAX_ACCOUNTS>) -> bool {
    let mut encoded = [0u8; ACCOUNT_BYTES];
    database.encode(&mut encoded).is_ok() && crate::vfs::system_store_accounts(&encoded)
}

fn with_state<R>(function: impl FnOnce(&mut State) -> R) -> R {
    while STATE
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *STATE.state.get() });
    STATE.lock.store(false, Ordering::Release);
    result
}
