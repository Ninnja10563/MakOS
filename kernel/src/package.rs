use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const MAX_NAME: usize = 32;
const MAX_VERSION: usize = 16;
const MAX_CONTENT: usize = 255;
const MAX_DEPENDENCY: usize = 255;
const MANIFEST_PREFIX: &[u8; 8] = b"MAKPKG1\0";
const MANIFEST_CAPACITY: usize =
    8 + 1 + MAX_NAME + 1 + MAX_VERSION + 2 + MAX_CONTENT + 1 + MAX_DEPENDENCY;

const REPOSITORY_MODULUS: [u8; 256] = makos_crypto::decode_hex_256(
    b"ba8d9d8181585920c54a3f1440aab2be7523de28bc6076312b5d1a81e7ed6a902387913a22b22dcfa940028aca21fe7642dd9be867eb13073aa4c5a7c224599079790b5cb26f3d30b78f03f5c89bbf8457c110e67a35396d729d733df0999e99977d6724dfad8fb5001210246fdad52f1c144e6bbfac86a27dac4212b5ac0726a4c51e465b42a29609a40c4d486be2ef1ba19e5d735230c9da8b97fe1b28362e064bc18a1b9d91f346f590eec0525733123a743a9751b87f407f73bea5d90c9dd03b5ce0ee61ecf4b048bc4f2f0c09c9e32147be65fa9b8bd54363c6ab019c1974fcb20d5f2ad1d8a074d57f4b7f79d942359211adc88c72a2a088f87828716d",
);

#[derive(Clone, Copy)]
struct Generation {
    valid: bool,
    name: [u8; MAX_NAME],
    name_length: usize,
    version: [u8; MAX_VERSION],
    version_length: usize,
    content_hash: [u8; 32],
    dependency_hash: [u8; 32],
    authenticated: bool,
}

impl Generation {
    const EMPTY: Self = Self {
        valid: false,
        name: [0; MAX_NAME],
        name_length: 0,
        version: [0; MAX_VERSION],
        version_length: 0,
        content_hash: [0; 32],
        dependency_hash: [0; 32],
        authenticated: false,
    };
}

struct State {
    slots: [Generation; 2],
    active: usize,
    rollback: usize,
}

struct LockedState {
    lock: AtomicBool,
    state: UnsafeCell<State>,
}

unsafe impl Sync for LockedState {}

static STORE: LockedState = LockedState {
    lock: AtomicBool::new(false),
    state: UnsafeCell::new(State {
        slots: [Generation::EMPTY; 2],
        active: 0,
        rollback: 1,
    }),
};

const RUNTIME_LEGACY: u64 = 1;
const RUNTIME_PERSISTENT: u64 = 2;
const RUNTIME_ERROR: u64 = 3;
static RUNTIME_MODE_COUNT: AtomicU64 = AtomicU64::new(0);
static RUNTIME_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
pub struct RuntimeStatus {
    pub mode: u8,
    pub package_count: u8,
    pub generation: u64,
}

pub fn install(
    name: &[u8],
    version: &[u8],
    content: &[u8],
    dependency: &[u8],
    signature: &[u8; 256],
) -> bool {
    if name.is_empty()
        || name.len() > MAX_NAME
        || version.is_empty()
        || version.len() > MAX_VERSION
        || content.is_empty()
        || content.len() > MAX_CONTENT
        || dependency.is_empty()
        || dependency.len() > MAX_DEPENDENCY
    {
        crate::log::audit(b"audit: package install rejected");
        return false;
    }
    if !verify_manifest(name, version, content, dependency, signature) {
        crate::log::audit(b"audit: package signature rejected");
        return false;
    }
    let legacy_dependency = dependency == b"libc";
    let mut dependencies =
        [makos_package_store::Dependency::EMPTY; makos_package_store::MAX_DEPENDENCIES];
    let dependency_count = if legacy_dependency {
        0
    } else {
        let Ok(count) = makos_package_store::decode_dependencies(dependency, &mut dependencies)
        else {
            crate::log::audit(b"audit: package dependencies rejected");
            return false;
        };
        count
    };
    let (result, persistent) = with_store(|store| {
        match crate::fs::package_transaction_store() {
            Ok(Some(mut persistent)) => {
                let manifest = makos_package_store::Manifest {
                    name,
                    version,
                    dependencies: &dependencies[..dependency_count],
                };
                return match persistent.installed(name) {
                    Ok(Some(_)) => (persistent.replace(manifest, content).is_ok(), true),
                    Ok(None) => (persistent.install(manifest, content).is_ok(), true),
                    Err(_) => (false, true),
                };
            }
            Ok(None) => {}
            Err(_) => return (false, true),
        }
        if !legacy_dependency {
            return (false, false);
        }
        let target = store.rollback;
        let mut generation = Generation::EMPTY;
        generation.valid = true;
        generation.name[..name.len()].copy_from_slice(name);
        generation.name_length = name.len();
        generation.version[..version.len()].copy_from_slice(version);
        generation.version_length = version.len();
        generation.content_hash = makos_crypto::sha256(content);
        generation.dependency_hash = makos_crypto::sha256(dependency);
        generation.authenticated = true;
        store.slots[target] = generation;
        store.rollback = store.active;
        store.active = target;
        record_runtime_legacy_count(1);
        (true, false)
    });
    if result && persistent {
        let _ = crate::fs::refresh_transaction_packages();
    }
    crate::log::audit(if result {
        b"audit: package install committed"
    } else {
        b"audit: package install rejected"
    });
    result
}

pub fn query(name: &[u8], output: &mut [u8]) -> Option<usize> {
    with_store(|store| {
        match crate::fs::package_transaction_store() {
            Ok(Some(mut persistent)) => {
                let installed = persistent.installed(name).ok()??;
                let version = installed.version();
                let length = version.len().min(output.len());
                output[..length].copy_from_slice(&version[..length]);
                return Some(length);
            }
            Ok(None) => {}
            Err(_) => return None,
        }
        let generation = store.slots[store.active];
        if !generation.valid
            || !generation.authenticated
            || &generation.name[..generation.name_length] != name
        {
            return None;
        }
        let length = generation.version_length.min(output.len());
        output[..length].copy_from_slice(&generation.version[..length]);
        Some(length)
    })
}

pub fn rollback() -> bool {
    let (result, persistent) = with_store(|store| {
        match crate::fs::package_transaction_store() {
            Ok(Some(mut persistent)) => return (persistent.rollback().is_ok(), true),
            Ok(None) => {}
            Err(_) => return (false, true),
        }
        if !store.slots[store.rollback].valid {
            return (false, false);
        }
        core::mem::swap(&mut store.active, &mut store.rollback);
        record_runtime_legacy_count(usize::from(store.slots[store.active].valid));
        (true, false)
    });
    if result && persistent {
        let _ = crate::fs::refresh_transaction_packages();
    }
    crate::log::audit(if result {
        b"audit: package rollback committed"
    } else {
        b"audit: package rollback rejected"
    });
    result
}

pub fn remove(name: &[u8]) -> bool {
    let (result, persistent) = with_store(|store| {
        match crate::fs::package_transaction_store() {
            Ok(Some(mut persistent)) => return (persistent.remove(name).is_ok(), true),
            Ok(None) => {}
            Err(_) => return (false, true),
        }
        let current = store.slots[store.active];
        if !current.valid || !current.authenticated || &current.name[..current.name_length] != name
        {
            return (false, false);
        }
        let target = store.rollback;
        store.slots[target] = Generation::EMPTY;
        store.rollback = store.active;
        store.active = target;
        record_runtime_legacy_count(0);
        (true, false)
    });
    if result && persistent {
        let _ = crate::fs::refresh_transaction_packages();
    }
    crate::log::audit(if result {
        b"audit: package removal committed"
    } else {
        b"audit: package removal rejected"
    });
    result
}

fn verify_manifest(
    name: &[u8],
    version: &[u8],
    content: &[u8],
    dependency: &[u8],
    signature: &[u8; 256],
) -> bool {
    let mut manifest = [0u8; MANIFEST_CAPACITY];
    let mut cursor = 0;
    manifest[cursor..cursor + MANIFEST_PREFIX.len()].copy_from_slice(MANIFEST_PREFIX);
    cursor += MANIFEST_PREFIX.len();
    manifest[cursor] = name.len() as u8;
    cursor += 1;
    manifest[cursor..cursor + name.len()].copy_from_slice(name);
    cursor += name.len();
    manifest[cursor] = version.len() as u8;
    cursor += 1;
    manifest[cursor..cursor + version.len()].copy_from_slice(version);
    cursor += version.len();
    manifest[cursor..cursor + 2].copy_from_slice(&(content.len() as u16).to_le_bytes());
    cursor += 2;
    manifest[cursor..cursor + content.len()].copy_from_slice(content);
    cursor += content.len();
    manifest[cursor] = dependency.len() as u8;
    cursor += 1;
    manifest[cursor..cursor + dependency.len()].copy_from_slice(dependency);
    cursor += dependency.len();
    makos_crypto::rsa2048_sha256_verify(&REPOSITORY_MODULUS, signature, &manifest[..cursor])
}

pub(crate) fn record_runtime_persistent(generation: u64, package_count: usize) {
    RUNTIME_GENERATION.store(generation, Ordering::Release);
    RUNTIME_MODE_COUNT.store(
        RUNTIME_PERSISTENT | ((package_count.min(u8::MAX as usize) as u64) << 8),
        Ordering::Release,
    );
}

pub(crate) fn record_runtime_legacy() {
    record_runtime_legacy_count(0);
}

fn record_runtime_legacy_count(package_count: usize) {
    RUNTIME_GENERATION.store(0, Ordering::Release);
    RUNTIME_MODE_COUNT.store(
        RUNTIME_LEGACY | ((package_count.min(u8::MAX as usize) as u64) << 8),
        Ordering::Release,
    );
}

pub(crate) fn record_runtime_error() {
    RUNTIME_MODE_COUNT.store(RUNTIME_ERROR, Ordering::Release);
}

pub fn runtime_status() -> RuntimeStatus {
    let packed = RUNTIME_MODE_COUNT.load(Ordering::Acquire);
    RuntimeStatus {
        mode: packed as u8,
        package_count: (packed >> 8) as u8,
        generation: RUNTIME_GENERATION.load(Ordering::Acquire),
    }
}

pub fn runtime_status_text(output: &mut [u8]) -> usize {
    let status = runtime_status();
    let label: &[u8] = match status.mode {
        1 => b"PACKAGES RAM COMPAT",
        2 => b"PACKAGES DISK A/B",
        3 => b"PACKAGES RECOVERY NEEDED",
        _ => b"PACKAGES INITIALIZING",
    };
    let mut cursor = copy_status_bytes(output, 0, label);
    if status.mode == RUNTIME_PERSISTENT as u8 {
        cursor = copy_status_bytes(output, cursor, b" GEN ");
        cursor = write_status_number(output, cursor, status.generation);
    }
    cursor = copy_status_bytes(output, cursor, b" ACTIVE ");
    write_status_number(output, cursor, u64::from(status.package_count))
}

fn copy_status_bytes(output: &mut [u8], cursor: usize, input: &[u8]) -> usize {
    let count = input.len().min(output.len().saturating_sub(cursor));
    output[cursor..cursor + count].copy_from_slice(&input[..count]);
    cursor + count
}

fn write_status_number(output: &mut [u8], cursor: usize, mut value: u64) -> usize {
    let mut digits = [0u8; 20];
    let mut count = 0usize;
    loop {
        digits[count] = b'0' + (value % 10) as u8;
        count += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    let mut end = cursor;
    for index in (0..count).rev() {
        if end == output.len() {
            break;
        }
        output[end] = digits[index];
        end += 1;
    }
    end
}

fn with_store<R>(function: impl FnOnce(&mut State) -> R) -> R {
    while STORE
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *STORE.state.get() });
    STORE.lock.store(false, Ordering::Release);
    result
}
