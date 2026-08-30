use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// Global slots cover every process namespace. Firefox forks while holding
// dozens of descriptors; children share descriptions but need their own slots.
const MAX_OPEN_FILES: usize = 512;
const MAX_FILE_DESCRIPTIONS: usize = 256;
const MAX_PIPES: usize = 16;
// Match Firefox's POSIX IPC batch size. The count stays below u8::MAX because
// Pipe stores the queued count compactly and descriptor numbers are u8-backed.
const MAX_SOCKET_RIGHTS: usize = 200;
const MAX_WORKING_DIRECTORIES: usize = 32;
const MAX_RECORD_LOCKS: usize = 64;
pub(crate) const MAX_PATH_BYTES: usize = 4096;
// POSIX requires PIPE_BUF >= 512 and atomic writes through that bound.
const PIPE_BYTES: usize = 512;
const DIRECTORY_NAME_BYTES: usize = 255;
pub(crate) const MAX_FILE_BYTES: usize = 2048;
pub(crate) const DYNAMIC_FILE_COUNT: usize = 16;
pub(crate) const DYNAMIC_NAME_BYTES: usize = 32;
pub(crate) const SYSTEM_PACKAGE_FILE_COUNT: usize = 384;
pub(crate) const SYSTEM_PACKAGE_PATH_BYTES: usize = 256;
const BOOT_FILE_PATH: &[u8] = b"/boot-count.txt";
const USER_FILE_PATH: &[u8] = b"/home/user/note.txt";
const USER_FILE_NAME: &[u8] = b"note.txt";
const USER_PREFIX: &[u8] = b"/home/user/";
const ROOT_PATH: &[u8] = b"/";
const HOME_PATH: &[u8] = b"/home";
const USER_DIRECTORY_PATH: &[u8] = b"/home/user";
const URANDOM_PATH: &[u8] = b"/dev/urandom";
const NULL_PATH: &[u8] = b"/dev/null";
const ZERO_PATH: &[u8] = b"/dev/zero";
const DEFAULT_WORKING_DIRECTORY: &[u8] = USER_DIRECTORY_PATH;
const ACCOUNT_DB_NAME: &[u8] = b".accounts";
const NODE_BOOT: u8 = 0;
const NODE_USER: u8 = 1;
const NODE_DYNAMIC_BASE: u8 = 2;
const NODE_DIRECTORY_ROOT: u8 = 250;
const NODE_DIRECTORY_HOME: u8 = 251;
const NODE_DIRECTORY_USER: u8 = 252;
pub const KIND_FILE: u32 = 1;
pub const KIND_DIRECTORY: u32 = 2;
pub const KIND_SYMLINK: u32 = 7;
const STATUS_NONBLOCK: u32 = 0x800;
const DESCRIPTION_FILE: u8 = 1;
const DESCRIPTION_PIPE_READ: u8 = 2;
const DESCRIPTION_PIPE_WRITE: u8 = 3;
const DESCRIPTION_DIRECTORY: u8 = 4;
const DESCRIPTION_SYSTEM_FILE: u8 = 5;
const DESCRIPTION_RANDOM: u8 = 6;
const DESCRIPTION_MAKFS4_FILE: u8 = 7;
const DESCRIPTION_MAKFS4_DIRECTORY: u8 = 8;
const DESCRIPTION_SHMEM: u8 = 9;
const DESCRIPTION_NULL: u8 = 10;
const DESCRIPTION_ZERO: u8 = 11;
const DESCRIPTION_PACKAGE_DIRECTORY: u8 = 12;
const DESCRIPTION_SOCKETPAIR: u8 = 13;
const NO_PACKAGE_FILE: u16 = u16::MAX;
const NO_MAKFS4_INODE: u16 = u16::MAX;
const NO_SHMEM_OBJECT: u16 = 0;
const SHMEM_TRACE_LIMIT: u64 = 8;
static SHMEM_OPEN_TRACES: AtomicU64 = AtomicU64::new(0);
static SHMEM_CREATE_TRACES: AtomicU64 = AtomicU64::new(0);
static SHMEM_UNLINK_TRACES: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
struct SystemFile {
    node: u8,
    inode: u64,
    mode: u32,
    path: &'static [u8],
    data: &'static [u8],
}

#[cfg(target_arch = "aarch64")]
static SYSTEM_FILES: [SystemFile; 5] = [
    SystemFile {
        node: 248,
        inode: 6,
        mode: 0o100555,
        path: b"/usr/lib/libc.so",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-musl-loader.so")),
    },
    SystemFile {
        node: 249,
        inode: 7,
        mode: 0o100555,
        path: b"/usr/lib/libmakosdemo.so",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-libmakosdemo.so")),
    },
    SystemFile {
        node: 247,
        inode: 8,
        mode: 0o100555,
        path: b"/usr/bin/makos-exec-target",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/aarch64-musl-exec-target.elf")),
    },
    SystemFile {
        node: 246,
        inode: 9,
        mode: 0o100444,
        path: b"/usr/src/makos/ports/musl/shared-demo.c",
        data: include_bytes!("../../ports/musl/shared-demo.c"),
    },
    SystemFile {
        node: 245,
        inode: 10,
        mode: 0o100444,
        path: b"/usr/include/stdint.h",
        data: include_bytes!("../../sdk/selfhost/include/stdint.h"),
    },
];
#[cfg(not(target_arch = "aarch64"))]
static SYSTEM_FILES: [SystemFile; 0] = [];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescriptorError {
    BadFile,
    Invalid,
    TooMany,
    Again,
    BrokenPipe,
    TooLarge,
    Io,
    Permission,
    Busy,
    Exists,
    NotDirectory,
    NotEmpty,
    NotFound,
    IllegalSeek,
    NoSpace,
    Loop,
}

impl DescriptorError {
    pub const fn abi(self) -> u64 {
        let errno = match self {
            Self::BadFile => 9i64,
            Self::Invalid => 22,
            Self::TooMany => 24,
            Self::Again => 11,
            Self::BrokenPipe => 32,
            Self::TooLarge => 27,
            Self::Io => 5,
            Self::Permission => 13,
            Self::Busy => 16,
            Self::Exists => 17,
            Self::NotDirectory => 20,
            Self::NotEmpty => 39,
            Self::NotFound => 2,
            Self::IllegalSeek => 29,
            Self::NoSpace => 28,
            Self::Loop => 40,
        };
        (-errno) as u64
    }
}

#[derive(Clone, Copy)]
pub(crate) struct MountedDynamicFile {
    pub used: bool,
    pub kind: u32,
    pub name: [u8; DYNAMIC_NAME_BYTES],
    pub name_length: usize,
    pub data: [u8; MAX_FILE_BYTES],
    pub data_length: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct MountedPackageFile {
    pub used: bool,
    pub path: [u8; SYSTEM_PACKAGE_PATH_BYTES],
    pub path_length: usize,
    pub size: u64,
    pub first_lba: u64,
    pub sectors: u64,
    pub data_crc: u32,
    pub transaction: bool,
    pub transaction_name: [u8; makos_package_store::MAX_NAME_BYTES],
    pub transaction_name_length: u8,
}

#[derive(Clone, Copy)]
pub(crate) enum ReadOnlyFileBacking {
    Embedded(&'static [u8]),
    Package(MountedPackageFile),
}

#[derive(Clone, Copy)]
pub(crate) struct SharedMemoryBacking {
    pub object: crate::aarch64_shmem::ObjectId,
    pub size: u64,
}

impl MountedPackageFile {
    pub const EMPTY: Self = Self {
        used: false,
        path: [0; SYSTEM_PACKAGE_PATH_BYTES],
        path_length: 0,
        size: 0,
        first_lba: 0,
        sectors: 0,
        data_crc: 0,
        transaction: false,
        transaction_name: [0; makos_package_store::MAX_NAME_BYTES],
        transaction_name_length: 0,
    };
}

#[derive(Clone, Copy)]
struct PackageSnapshot {
    used: bool,
    transaction: bool,
    size: u64,
    first_lba: u64,
    sectors: u64,
    data_crc: u32,
}

impl PackageSnapshot {
    const EMPTY: Self = Self {
        used: false,
        transaction: false,
        size: 0,
        first_lba: 0,
        sectors: 0,
        data_crc: 0,
    };

    const fn capture(file: MountedPackageFile) -> Self {
        Self {
            used: file.used,
            transaction: file.transaction,
            size: file.size,
            first_lba: file.first_lba,
            sectors: file.sectors,
            data_crc: file.data_crc,
        }
    }

    const fn mounted(self) -> MountedPackageFile {
        MountedPackageFile {
            used: self.used,
            path: [0; SYSTEM_PACKAGE_PATH_BYTES],
            path_length: 0,
            size: self.size,
            first_lba: self.first_lba,
            sectors: self.sectors,
            data_crc: self.data_crc,
            transaction: self.transaction,
            transaction_name: [0; makos_package_store::MAX_NAME_BYTES],
            transaction_name_length: 0,
        }
    }
}

/// Replace live transaction namespace while leaving immutable image packages
/// untouched. Open file descriptions retain their captured package backing.
pub(crate) fn replace_transaction_packages(files: &[MountedPackageFile]) -> bool {
    if files.len() > makos_package_store::MAX_PACKAGES {
        return false;
    }
    with_state(|state| {
        for package in &mut state.packages {
            if package.transaction {
                *package = MountedPackageFile::EMPTY;
            }
        }
        for file in files {
            let Some(slot) = state.packages.iter_mut().find(|package| !package.used) else {
                return false;
            };
            *slot = *file;
        }
        true
    })
}

/// Refuse reuse of an A/B slot while any shared open-file description still
/// references payload bytes in that slot. Callers retry after closing FDs.
pub(crate) fn package_transaction_sector_pinned(sector: u64) -> bool {
    let base = makos_package_store::PRODUCTION_BASE_SECTOR;
    let slot_sectors = makos_package_store::PRODUCTION_SLOT_SECTORS;
    if sector < base || sector >= base + 2 * slot_sectors {
        return false;
    }
    let target_slot = (sector - base) / slot_sectors;
    with_state(|state| {
        state.descriptions.iter().any(|description| {
            let snapshot = description.package_snapshot;
            description.used
                && description.references != 0
                && snapshot.used
                && snapshot.transaction
                && snapshot.first_lba >= base
                && snapshot.first_lba < base + 2 * slot_sectors
                && (snapshot.first_lba - base) / slot_sectors == target_slot
        })
    })
}

impl MountedDynamicFile {
    pub const EMPTY: Self = Self {
        used: false,
        kind: KIND_FILE,
        name: [0; DYNAMIC_NAME_BYTES],
        name_length: 0,
        data: [0; MAX_FILE_BYTES],
        data_length: 0,
    };
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Metadata {
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub kind: u32,
    pub size: u64,
    pub modified_ticks: u64,
    pub inode: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct ExtendedMetadata {
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub kind: u32,
    pub size: u64,
    pub accessed_ns: u64,
    pub modified_ns: u64,
    pub changed_ns: u64,
    pub inode: u64,
}

const _: [(); 56] = [(); core::mem::size_of::<ExtendedMetadata>()];

#[derive(Clone, Copy)]
#[repr(C)]
pub struct DirectoryEntry {
    pub inode: u64,
    pub kind: u32,
    pub name_length: u32,
    pub name: [u8; DIRECTORY_NAME_BYTES],
}

/// POSIX `struct flock` layout for AArch64 LP64 musl.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct FileLock {
    pub lock_type: i16,
    pub whence: i16,
    pub padding: u32,
    pub start: i64,
    pub length: i64,
    pub pid: i32,
    pub reserved: u32,
}

const _: [(); 32] = [(); core::mem::size_of::<FileLock>()];

#[derive(Clone, Copy)]
struct OpenFile {
    used: bool,
    fd: u8,
    description: u8,
    close_on_exec: bool,
    owner_pid: u64,
}

impl OpenFile {
    const EMPTY: Self = Self {
        used: false,
        fd: 0,
        description: 0,
        close_on_exec: false,
        owner_pid: 0,
    };
}

#[derive(Clone, Copy)]
struct FileDescription {
    used: bool,
    references: u8,
    kind: u8,
    offset: usize,
    node: u8,
    package_file: u16,
    package_snapshot: PackageSnapshot,
    package_path_length: u16,
    package_path: [u8; SYSTEM_PACKAGE_PATH_BYTES],
    makfs4_inode: u16,
    shmem_object: u16,
    pipe: u8,
    peer_pipe: u8,
    readable: bool,
    writable: bool,
    status_flags: u32,
}

impl FileDescription {
    const EMPTY: Self = Self {
        used: false,
        references: 0,
        kind: 0,
        offset: 0,
        node: 0,
        package_file: NO_PACKAGE_FILE,
        package_snapshot: PackageSnapshot::EMPTY,
        package_path_length: 0,
        package_path: [0; SYSTEM_PACKAGE_PATH_BYTES],
        makfs4_inode: NO_MAKFS4_INODE,
        shmem_object: NO_SHMEM_OBJECT,
        pipe: 0,
        peer_pipe: 0,
        readable: false,
        writable: false,
        status_flags: 0,
    };
}

#[derive(Clone, Copy)]
struct Pipe {
    used: bool,
    data: [u8; PIPE_BYTES],
    head: u16,
    length: u16,
    readers: u8,
    writers: u8,
    rights_count: u8,
    rights_skip: u16,
    rights: [u8; MAX_SOCKET_RIGHTS],
}

impl Pipe {
    const EMPTY: Self = Self {
        used: false,
        data: [0; PIPE_BYTES],
        head: 0,
        length: 0,
        readers: 0,
        writers: 0,
        rights_count: 0,
        rights_skip: 0,
        rights: [0; MAX_SOCKET_RIGHTS],
    };
}

#[derive(Clone, Copy)]
struct DynamicFile {
    used: bool,
    kind: u32,
    name: [u8; DYNAMIC_NAME_BYTES],
    name_length: usize,
    data: [u8; MAX_FILE_BYTES],
    data_length: usize,
    modified_ticks: u64,
}

impl DynamicFile {
    const EMPTY: Self = Self {
        used: false,
        kind: KIND_FILE,
        name: [0; DYNAMIC_NAME_BYTES],
        name_length: 0,
        data: [0; MAX_FILE_BYTES],
        data_length: 0,
        modified_ticks: 0,
    };
}

#[derive(Clone, Copy)]
struct WorkingDirectory {
    used: bool,
    owner_pid: u64,
    length: u16,
    path: [u8; MAX_PATH_BYTES],
}

impl WorkingDirectory {
    const EMPTY: Self = Self {
        used: false,
        owner_pid: 0,
        length: 0,
        path: [0; MAX_PATH_BYTES],
    };
}

#[derive(Clone, Copy)]
struct RecordLock {
    used: bool,
    exclusive: bool,
    key: u32,
    owner_pid: u64,
    start: u64,
    end: u64,
}

impl RecordLock {
    const EMPTY: Self = Self {
        used: false,
        exclusive: false,
        key: 0,
        owner_pid: 0,
        start: 0,
        end: 0,
    };
}

struct State {
    mounted: bool,
    boot_data: [u8; MAX_FILE_BYTES],
    boot_length: usize,
    boot_modified_ticks: u64,
    user_data: [u8; MAX_FILE_BYTES],
    user_length: usize,
    user_modified_ticks: u64,
    dynamic: [DynamicFile; DYNAMIC_FILE_COUNT],
    packages: [MountedPackageFile; SYSTEM_PACKAGE_FILE_COUNT],
    files: [OpenFile; MAX_OPEN_FILES],
    descriptions: [FileDescription; MAX_FILE_DESCRIPTIONS],
    pipes: [Pipe; MAX_PIPES],
    working_directories: [WorkingDirectory; MAX_WORKING_DIRECTORIES],
    record_locks: [RecordLock; MAX_RECORD_LOCKS],
}

struct LockedState {
    lock: AtomicBool,
    state: UnsafeCell<State>,
}

unsafe impl Sync for LockedState {}

static STATE: LockedState = LockedState {
    lock: AtomicBool::new(false),
    state: UnsafeCell::new(State {
        mounted: false,
        boot_data: [0; MAX_FILE_BYTES],
        boot_length: 0,
        boot_modified_ticks: 0,
        user_data: [0; MAX_FILE_BYTES],
        user_length: 0,
        user_modified_ticks: 0,
        dynamic: [DynamicFile::EMPTY; DYNAMIC_FILE_COUNT],
        packages: [MountedPackageFile::EMPTY; SYSTEM_PACKAGE_FILE_COUNT],
        files: [OpenFile::EMPTY; MAX_OPEN_FILES],
        descriptions: [FileDescription::EMPTY; MAX_FILE_DESCRIPTIONS],
        pipes: [Pipe::EMPTY; MAX_PIPES],
        working_directories: [WorkingDirectory::EMPTY; MAX_WORKING_DIRECTORIES],
        record_locks: [RecordLock::EMPTY; MAX_RECORD_LOCKS],
    }),
};

pub fn mount_files(
    boot_data: &[u8],
    user_data: &[u8],
    dynamic: &[MountedDynamicFile],
    packages: &[MountedPackageFile],
) {
    if boot_data.len() > MAX_FILE_BYTES
        || user_data.len() > MAX_FILE_BYTES
        || dynamic.len() != DYNAMIC_FILE_COUNT
        || packages.len() != SYSTEM_PACKAGE_FILE_COUNT
    {
        crate::fatal("VFS mounted file too large");
    }
    with_state(|state| {
        state.boot_data.fill(0);
        state.boot_data[..boot_data.len()].copy_from_slice(boot_data);
        state.boot_length = boot_data.len();
        state.boot_modified_ticks = crate::arch::monotonic_ticks();
        state.user_data.fill(0);
        state.user_data[..user_data.len()].copy_from_slice(user_data);
        state.user_length = user_data.len();
        state.user_modified_ticks = crate::arch::monotonic_ticks();
        for (destination, source) in state.dynamic.iter_mut().zip(dynamic) {
            *destination = DynamicFile::EMPTY;
            if source.used {
                destination.used = true;
                destination.kind = source.kind;
                destination.name = source.name;
                destination.name_length = source.name_length;
                destination.data = source.data;
                destination.data_length = source.data_length;
                destination.modified_ticks = crate::arch::monotonic_ticks();
            }
        }
        state.packages.copy_from_slice(packages);
        state.files.fill(OpenFile::EMPTY);
        state.descriptions.fill(FileDescription::EMPTY);
        state.pipes.fill(Pipe::EMPTY);
        state.working_directories.fill(WorkingDirectory::EMPTY);
        state.record_locks.fill(RecordLock::EMPTY);
        state.mounted = true;
    });
}

pub fn open(path: &[u8], write: bool) -> Option<u64> {
    open_mode(path, u8::from(write), write)
}

/// Open one path with POSIX access mode 0=read, 1=write, 2=read/write.
/// Truncation is explicit so writable profile/database files can be reopened
/// without destroying existing contents.
pub fn open_mode(path: &[u8], access: u8, truncate: bool) -> Option<u64> {
    if access > 2 || (truncate && access == 0) {
        return None;
    }
    let readable = access != 1;
    let writable = access != 0;
    let owner_pid = current_pid();
    with_state(|state| {
        if !state.mounted {
            return None;
        }
        let mut resolved = alloc::vec![0u8; MAX_PATH_BYTES];
        let path_length = resolve_vfs_path(state, owner_pid, path, &mut resolved, true).ok()?;
        let path = &resolved[..path_length];
        let mut shmem_object = 0u16;
        let mut package_snapshot = PackageSnapshot::EMPTY;
        let mut package_directory_path_length = 0usize;
        let (node, description_kind, package_file, makfs4_inode) = if let Some(name) =
            shmem_name(path)
        {
            shmem_object = match crate::aarch64_shmem::open(name, readable, writable) {
                Ok(object) => {
                    if SHMEM_OPEN_TRACES.fetch_add(1, Ordering::Relaxed) < SHMEM_TRACE_LIMIT {
                        crate::serial_println!(
                            "MAKOS_SHMEM_OPEN name={} object={} read={} write={}",
                            core::str::from_utf8(name).unwrap_or("<invalid>"),
                            object,
                            u8::from(readable),
                            u8::from(writable),
                        );
                    }
                    object
                }
                Err(error) => {
                    crate::serial_println!(
                        "MAKOS_SHMEM_OPEN_FAIL name={} error={:?}",
                        core::str::from_utf8(name).unwrap_or("<invalid>"),
                        error,
                    );
                    return None;
                }
            };
            (0, DESCRIPTION_SHMEM, NO_PACKAGE_FILE, NO_MAKFS4_INODE)
        } else if path == ROOT_PATH {
            if writable || !crate::security::file_access(0o040755, 0, 0, false) {
                return None;
            }
            (
                NODE_DIRECTORY_ROOT,
                DESCRIPTION_DIRECTORY,
                NO_PACKAGE_FILE,
                NO_MAKFS4_INODE,
            )
        } else if path == HOME_PATH {
            if writable || !crate::security::file_access(0o040755, 0, 0, false) {
                return None;
            }
            (
                NODE_DIRECTORY_HOME,
                DESCRIPTION_DIRECTORY,
                NO_PACKAGE_FILE,
                NO_MAKFS4_INODE,
            )
        } else if path == USER_DIRECTORY_PATH {
            if writable
                || !crate::security::file_access(
                    0o040700,
                    crate::security::INIT_UID,
                    crate::security::INIT_GID,
                    false,
                )
            {
                return None;
            }
            (
                NODE_DIRECTORY_USER,
                DESCRIPTION_DIRECTORY,
                NO_PACKAGE_FILE,
                NO_MAKFS4_INODE,
            )
        } else if path == URANDOM_PATH {
            if writable || !crate::security::file_access(0o020666, 0, 0, false) {
                return None;
            }
            (246, DESCRIPTION_RANDOM, NO_PACKAGE_FILE, NO_MAKFS4_INODE)
        } else if path == NULL_PATH {
            (245, DESCRIPTION_NULL, NO_PACKAGE_FILE, NO_MAKFS4_INODE)
        } else if path == ZERO_PATH {
            (244, DESCRIPTION_ZERO, NO_PACKAGE_FILE, NO_MAKFS4_INODE)
        } else if path == BOOT_FILE_PATH {
            if writable
                || !crate::security::file_access(0o100644, crate::security::ROOT_UID, 0, false)
            {
                return None;
            }
            (
                NODE_BOOT,
                DESCRIPTION_FILE,
                NO_PACKAGE_FILE,
                NO_MAKFS4_INODE,
            )
        } else if path == USER_FILE_PATH {
            if (readable && !user_file_access(false)) || (writable && !user_file_access(true)) {
                return None;
            }
            (
                NODE_USER,
                DESCRIPTION_FILE,
                NO_PACKAGE_FILE,
                NO_MAKFS4_INODE,
            )
        } else if let Some(system_file) = system_file_by_path(path) {
            if writable {
                return None;
            }
            (
                system_file.node,
                DESCRIPTION_SYSTEM_FILE,
                NO_PACKAGE_FILE,
                NO_MAKFS4_INODE,
            )
        } else if let Some((index, package)) = package_file_by_path(state, path) {
            if writable {
                return None;
            }
            package_snapshot = PackageSnapshot::capture(package);
            (0, DESCRIPTION_SYSTEM_FILE, index as u16, NO_MAKFS4_INODE)
        } else if let Some(index) = package_directory_by_path(state, path) {
            if writable {
                return None;
            }
            package_directory_path_length = path.len();
            (
                0,
                DESCRIPTION_PACKAGE_DIRECTORY,
                index as u16,
                NO_MAKFS4_INODE,
            )
        } else if let Some((index, inode)) = makfs4_by_path(path) {
            let kind = inode.mode & 0o170000;
            if (readable && !crate::security::file_access(inode.mode, inode.uid, inode.gid, false))
                || (writable
                    && !crate::security::file_access(inode.mode, inode.uid, inode.gid, true))
                || (writable && kind == 0o040000)
            {
                return None;
            }
            (
                0,
                if kind == 0o040000 {
                    DESCRIPTION_MAKFS4_DIRECTORY
                } else if kind == 0o100000 {
                    DESCRIPTION_MAKFS4_FILE
                } else {
                    return None;
                },
                NO_PACKAGE_FILE,
                index as u16,
            )
        } else {
            let name = parse_dynamic_path(path)?;
            let slot = dynamic_index_by_name(state, name)?;
            let kind = state.dynamic[slot].kind;
            if (writable && kind == KIND_DIRECTORY)
                || (readable && !user_node_access(kind, false))
                || (writable && !user_node_access(kind, true))
            {
                return None;
            }
            (
                NODE_DYNAMIC_BASE + slot as u8,
                if kind == KIND_DIRECTORY {
                    DESCRIPTION_DIRECTORY
                } else {
                    DESCRIPTION_FILE
                },
                NO_PACKAGE_FILE,
                NO_MAKFS4_INODE,
            )
        };
        let Some(index) = state.files.iter().position(|file| !file.used) else {
            if shmem_object != 0 {
                crate::aarch64_shmem::release_handle(shmem_object);
            }
            return None;
        };
        let Some(fd) = (3..=u8::MAX)
            .find(|candidate| descriptor_slot(state, owner_pid, u64::from(*candidate)).is_none())
        else {
            if shmem_object != 0 {
                crate::aarch64_shmem::release_handle(shmem_object);
            }
            return None;
        };
        let Some(description) = state.descriptions.iter().position(|entry| !entry.used) else {
            if shmem_object != 0 {
                crate::aarch64_shmem::release_handle(shmem_object);
            }
            return None;
        };
        if truncate {
            if description_kind == DESCRIPTION_SHMEM {
                if crate::aarch64_shmem::truncate(shmem_object, 0).is_err() {
                    crate::aarch64_shmem::release_handle(shmem_object);
                    return None;
                }
            } else if description_kind == DESCRIPTION_MAKFS4_FILE {
                if !crate::makfs4_volume::truncate_inode(u32::from(makfs4_inode), 0).is_ok() {
                    return None;
                }
            } else {
                match dynamic_index(node) {
                    Some(slot) => {
                        if state.dynamic[slot].kind != KIND_FILE {
                            return None;
                        }
                        let name_length = state.dynamic[slot].name_length;
                        let mut name = [0u8; DYNAMIC_NAME_BYTES];
                        name[..name_length]
                            .copy_from_slice(&state.dynamic[slot].name[..name_length]);
                        if !crate::fs::store_dynamic_file(slot, &name[..name_length], Some(&[])) {
                            return None;
                        }
                        state.dynamic[slot].data.fill(0);
                        state.dynamic[slot].data_length = 0;
                        state.dynamic[slot].modified_ticks = crate::arch::monotonic_ticks();
                    }
                    None if node == NODE_USER => {
                        if !crate::fs::store_user_file(&[]) {
                            return None;
                        }
                        state.user_data.fill(0);
                        state.user_length = 0;
                        state.user_modified_ticks = crate::arch::monotonic_ticks();
                    }
                    None => return None,
                }
            }
        }
        state.files[index] = OpenFile {
            used: true,
            fd,
            description: description as u8,
            close_on_exec: false,
            owner_pid,
        };
        let mut opened_description = FileDescription {
            used: true,
            references: 1,
            kind: description_kind,
            offset: 0,
            node,
            package_file,
            package_snapshot,
            package_path_length: package_directory_path_length as u16,
            package_path: [0; SYSTEM_PACKAGE_PATH_BYTES],
            makfs4_inode,
            shmem_object,
            pipe: 0,
            peer_pipe: 0,
            readable,
            writable,
            status_flags: u32::from(access),
        };
        if package_directory_path_length != 0 {
            opened_description.package_path[..package_directory_path_length].copy_from_slice(path);
        }
        state.descriptions[description] = opened_description;
        Some(u64::from(fd))
    })
}

/// Create one bounded byte pipe in caller's FD namespace. Blocking behavior is
/// handled at syscall boundary so this locked storage layer never sleeps.
pub fn pipe_pair(close_on_exec: bool, nonblocking: bool) -> Result<(u64, u64), DescriptorError> {
    with_state(|state| {
        let owner_pid = current_pid();
        let pipe = state
            .pipes
            .iter()
            .position(|pipe| !pipe.used)
            .ok_or(DescriptorError::TooMany)?;
        let mut descriptor_slots = [usize::MAX; 2];
        let mut descriptor_count = 0usize;
        for (index, descriptor) in state.files.iter().enumerate() {
            if !descriptor.used {
                descriptor_slots[descriptor_count] = index;
                descriptor_count += 1;
                if descriptor_count == 2 {
                    break;
                }
            }
        }
        if descriptor_count != 2 {
            return Err(DescriptorError::TooMany);
        }
        let mut description_slots = [usize::MAX; 2];
        let mut description_count = 0usize;
        for (index, description) in state.descriptions.iter().enumerate() {
            if !description.used {
                description_slots[description_count] = index;
                description_count += 1;
                if description_count == 2 {
                    break;
                }
            }
        }
        if description_count != 2 {
            return Err(DescriptorError::TooMany);
        }
        let mut numbers = [0u8; 2];
        let mut number_count = 0usize;
        for candidate in 3..=u8::MAX {
            if descriptor_slot(state, owner_pid, u64::from(candidate)).is_none() {
                numbers[number_count] = candidate;
                number_count += 1;
                if number_count == 2 {
                    break;
                }
            }
        }
        if number_count != 2 {
            return Err(DescriptorError::TooMany);
        }
        state.pipes[pipe] = Pipe {
            used: true,
            readers: 1,
            writers: 1,
            ..Pipe::EMPTY
        };
        for endpoint in 0..2 {
            state.files[descriptor_slots[endpoint]] = OpenFile {
                used: true,
                fd: numbers[endpoint],
                description: description_slots[endpoint] as u8,
                close_on_exec,
                owner_pid,
            };
            state.descriptions[description_slots[endpoint]] = FileDescription {
                used: true,
                references: 1,
                kind: if endpoint == 0 {
                    DESCRIPTION_PIPE_READ
                } else {
                    DESCRIPTION_PIPE_WRITE
                },
                offset: 0,
                node: 0,
                package_file: NO_PACKAGE_FILE,
                package_snapshot: PackageSnapshot::EMPTY,
                package_path_length: 0,
                package_path: [0; SYSTEM_PACKAGE_PATH_BYTES],
                makfs4_inode: NO_MAKFS4_INODE,
                shmem_object: NO_SHMEM_OBJECT,
                pipe: pipe as u8,
                peer_pipe: 0,
                readable: endpoint == 0,
                writable: endpoint == 1,
                status_flags: u32::from(endpoint == 1)
                    | if nonblocking { STATUS_NONBLOCK } else { 0 },
            };
        }
        Ok((u64::from(numbers[0]), u64::from(numbers[1])))
    })
}

/// Create a bidirectional local stream pair. Each endpoint reads one bounded
/// kernel pipe and writes the opposite pipe; shared file descriptions preserve
/// normal dup/fork lifetime semantics.
pub fn socket_pair(close_on_exec: bool, nonblocking: bool) -> Result<(u64, u64), DescriptorError> {
    with_state(|state| {
        let owner_pid = current_pid();
        let mut pipe_slots = [usize::MAX; 2];
        let mut pipe_count = 0usize;
        for (index, pipe) in state.pipes.iter().enumerate() {
            if !pipe.used {
                pipe_slots[pipe_count] = index;
                pipe_count += 1;
                if pipe_count == 2 {
                    break;
                }
            }
        }
        if pipe_count != 2 {
            return Err(DescriptorError::TooMany);
        }
        let mut descriptor_slots = [usize::MAX; 2];
        let mut descriptor_count = 0usize;
        for (index, descriptor) in state.files.iter().enumerate() {
            if !descriptor.used {
                descriptor_slots[descriptor_count] = index;
                descriptor_count += 1;
                if descriptor_count == 2 {
                    break;
                }
            }
        }
        if descriptor_count != 2 {
            return Err(DescriptorError::TooMany);
        }
        let mut description_slots = [usize::MAX; 2];
        let mut description_count = 0usize;
        for (index, description) in state.descriptions.iter().enumerate() {
            if !description.used {
                description_slots[description_count] = index;
                description_count += 1;
                if description_count == 2 {
                    break;
                }
            }
        }
        if description_count != 2 {
            return Err(DescriptorError::TooMany);
        }
        let mut numbers = [0u8; 2];
        let mut number_count = 0usize;
        for candidate in 3..=u8::MAX {
            if descriptor_slot(state, owner_pid, u64::from(candidate)).is_none() {
                numbers[number_count] = candidate;
                number_count += 1;
                if number_count == 2 {
                    break;
                }
            }
        }
        if number_count != 2 {
            return Err(DescriptorError::TooMany);
        }
        for pipe in pipe_slots {
            state.pipes[pipe] = Pipe {
                used: true,
                readers: 1,
                writers: 1,
                ..Pipe::EMPTY
            };
        }
        for endpoint in 0..2 {
            state.files[descriptor_slots[endpoint]] = OpenFile {
                used: true,
                fd: numbers[endpoint],
                description: description_slots[endpoint] as u8,
                close_on_exec,
                owner_pid,
            };
            state.descriptions[description_slots[endpoint]] = FileDescription {
                used: true,
                references: 1,
                kind: DESCRIPTION_SOCKETPAIR,
                offset: 0,
                node: 0,
                package_file: NO_PACKAGE_FILE,
                package_snapshot: PackageSnapshot::EMPTY,
                package_path_length: 0,
                package_path: [0; SYSTEM_PACKAGE_PATH_BYTES],
                makfs4_inode: NO_MAKFS4_INODE,
                shmem_object: NO_SHMEM_OBJECT,
                pipe: pipe_slots[endpoint] as u8,
                peer_pipe: pipe_slots[1 - endpoint] as u8,
                readable: true,
                writable: true,
                status_flags: 2 | if nonblocking { STATUS_NONBLOCK } else { 0 },
            };
        }
        Ok((u64::from(numbers[0]), u64::from(numbers[1])))
    })
}

/// Send bytes and at most one queued `SCM_RIGHTS` record through an AF_UNIX
/// socketpair endpoint. Queued references keep each open-file description
/// alive until received or discarded with the pipe.
pub fn send_result_with_rights(
    fd: u64,
    input: &[u8],
    rights_fds: &[u64],
) -> Result<usize, DescriptorError> {
    if rights_fds.len() > MAX_SOCKET_RIGHTS {
        return Err(DescriptorError::Invalid);
    }
    with_state(|state| {
        let owner_pid = current_pid();
        let file_slot = descriptor_slot(state, owner_pid, fd).ok_or(DescriptorError::BadFile)?;
        let file = state.files[file_slot];
        let description = *state
            .descriptions
            .get(file.description as usize)
            .filter(|description| description.used)
            .ok_or(DescriptorError::BadFile)?;
        if description.kind != DESCRIPTION_SOCKETPAIR || !description.writable {
            return Err(DescriptorError::BadFile);
        }
        if input.is_empty() {
            return Ok(0);
        }
        let pipe_index = description.peer_pipe as usize;
        let pipe = *state
            .pipes
            .get(pipe_index)
            .filter(|pipe| pipe.used)
            .ok_or(DescriptorError::BadFile)?;
        if pipe.readers == 0 {
            return Err(DescriptorError::BrokenPipe);
        }
        if !rights_fds.is_empty() && pipe.rights_count != 0 {
            return Err(DescriptorError::Again);
        }
        let available = PIPE_BYTES - pipe.length as usize;
        if available == 0 || (input.len() <= PIPE_BYTES && available < input.len()) {
            return Err(DescriptorError::Again);
        }
        let count = input.len().min(available);

        let mut rights = [0u8; MAX_SOCKET_RIGHTS];
        for (right_index, right_fd) in rights_fds.iter().copied().enumerate() {
            let slot =
                descriptor_slot(state, owner_pid, right_fd).ok_or(DescriptorError::BadFile)?;
            let description_index = state.files[slot].description as usize;
            let queued_copies = rights[..right_index]
                .iter()
                .filter(|queued| usize::from(**queued) == description_index)
                .count();
            let right_description = state
                .descriptions
                .get(description_index)
                .filter(|description| description.used)
                .ok_or(DescriptorError::BadFile)?;
            if usize::from(right_description.references) + queued_copies >= usize::from(u8::MAX) {
                return Err(DescriptorError::TooMany);
            }
            rights[right_index] = description_index as u8;
        }

        for description in rights[..rights_fds.len()].iter().copied() {
            state.descriptions[description as usize].references += 1;
        }
        let pipe = &mut state.pipes[pipe_index];
        let tail = (pipe.head as usize + pipe.length as usize) % PIPE_BYTES;
        for (offset, byte) in input[..count].iter().enumerate() {
            pipe.data[(tail + offset) % PIPE_BYTES] = *byte;
        }
        if !rights_fds.is_empty() {
            pipe.rights_count = rights_fds.len() as u8;
            pipe.rights_skip = pipe.length;
            pipe.rights = rights;
        }
        pipe.length += count as u16;
        Ok(count)
    })
}

pub fn read(fd: u64, output: &mut [u8]) -> Option<usize> {
    read_result(fd, output).ok()
}

/// Receive bytes and any `SCM_RIGHTS` record whose first associated byte is
/// consumed. Received descriptors share queued open-file descriptions.
pub fn read_result_with_rights(
    fd: u64,
    output: &mut [u8],
    rights_out: &mut [u64],
) -> Result<(usize, usize), DescriptorError> {
    with_state(|state| {
        let owner_pid = current_pid();
        let index = descriptor_slot(state, owner_pid, fd).ok_or(DescriptorError::BadFile)?;
        let file = state.files[index];
        let description = *state
            .descriptions
            .get(file.description as usize)
            .filter(|description| description.used)
            .ok_or(DescriptorError::BadFile)?;
        if description.kind != DESCRIPTION_SOCKETPAIR || !description.readable {
            return Err(DescriptorError::BadFile);
        }
        read_socketpair_with_rights(state, owner_pid, description, output, rights_out)
    })
}

pub fn read_result(fd: u64, output: &mut [u8]) -> Result<usize, DescriptorError> {
    with_state(|state| {
        let index = descriptor_slot(state, current_pid(), fd).ok_or(DescriptorError::BadFile)?;
        let file = state.files[index];
        let description = *state
            .descriptions
            .get(file.description as usize)
            .ok_or(DescriptorError::BadFile)?;
        if !description.used
            || !description.readable
            || matches!(
                description.kind,
                DESCRIPTION_PIPE_WRITE
                    | DESCRIPTION_DIRECTORY
                    | DESCRIPTION_MAKFS4_DIRECTORY
                    | DESCRIPTION_PACKAGE_DIRECTORY
            )
        {
            return Err(DescriptorError::BadFile);
        }
        if description.kind == DESCRIPTION_SOCKETPAIR {
            return read_socketpair_with_rights(
                state,
                file.owner_pid,
                description,
                output,
                &mut [],
            )
            .map(|(count, _)| count);
        }
        if description.kind == DESCRIPTION_PIPE_READ {
            if output.is_empty() {
                return Ok(0);
            }
            let pipe = state
                .pipes
                .get_mut(description.pipe as usize)
                .filter(|pipe| pipe.used)
                .ok_or(DescriptorError::BadFile)?;
            if pipe.length == 0 {
                return if pipe.writers == 0 {
                    Ok(0)
                } else {
                    Err(DescriptorError::Again)
                };
            }
            let count = output.len().min(pipe.length as usize);
            for (offset, byte) in output[..count].iter_mut().enumerate() {
                *byte = pipe.data[(pipe.head as usize + offset) % PIPE_BYTES];
            }
            pipe.head = ((pipe.head as usize + count) % PIPE_BYTES) as u16;
            pipe.length -= count as u16;
            return Ok(count);
        }
        if description.kind == DESCRIPTION_RANDOM {
            #[cfg(target_arch = "aarch64")]
            {
                return if crate::aarch64_virtio_rng::fill(output) {
                    Ok(output.len())
                } else {
                    Err(DescriptorError::Io)
                };
            }
            #[cfg(not(target_arch = "aarch64"))]
            {
                return Err(DescriptorError::BadFile);
            }
        }
        if description.kind == DESCRIPTION_NULL {
            return Ok(0);
        }
        if description.kind == DESCRIPTION_ZERO {
            output.fill(0);
            return Ok(output.len());
        }
        if description.kind == DESCRIPTION_SHMEM {
            let count = crate::aarch64_shmem::read(
                description.shmem_object,
                description.offset as u64,
                output,
            )
            .map_err(shmem_error)?;
            state.descriptions[file.description as usize].offset = description
                .offset
                .checked_add(count)
                .ok_or(DescriptorError::TooLarge)?;
            return Ok(count);
        }
        if description.kind == DESCRIPTION_MAKFS4_FILE {
            let count = crate::makfs4_volume::read_inode_at(
                u32::from(description.makfs4_inode),
                description.offset as u64,
                output,
            )
            .map_err(makfs4_error)?;
            state.descriptions[file.description as usize].offset = description
                .offset
                .checked_add(count)
                .ok_or(DescriptorError::TooLarge)?;
            return Ok(count);
        }
        if !matches!(description.kind, DESCRIPTION_FILE | DESCRIPTION_SYSTEM_FILE) {
            return Err(DescriptorError::BadFile);
        }
        if description.kind == DESCRIPTION_SYSTEM_FILE
            && description.package_file != NO_PACKAGE_FILE
        {
            let package = description.package_snapshot.mounted();
            if !package.used {
                return Err(DescriptorError::BadFile);
            }
            let count = output.len().min(
                usize::try_from(package.size.saturating_sub(description.offset as u64))
                    .unwrap_or(usize::MAX),
            );
            let count = crate::fs::read_package_file(
                &package,
                description.offset as u64,
                &mut output[..count],
            )
            .ok_or(DescriptorError::Io)?;
            state.descriptions[file.description as usize].offset += count;
            return Ok(count);
        }
        let (data, length): (&[u8], usize) = match dynamic_index(description.node) {
            Some(slot) if state.dynamic[slot].used && state.dynamic[slot].kind == KIND_FILE => {
                (&state.dynamic[slot].data, state.dynamic[slot].data_length)
            }
            None if description.node == NODE_BOOT => (&state.boot_data, state.boot_length),
            None if description.node == NODE_USER => (&state.user_data, state.user_length),
            Some(_) => return Err(DescriptorError::BadFile),
            None => system_file_by_node(description.node)
                .map(|file| (file.data, file.data.len()))
                .ok_or(DescriptorError::BadFile)?,
        };
        let count = output.len().min(length.saturating_sub(description.offset));
        output[..count].copy_from_slice(&data[description.offset..description.offset + count]);
        state.descriptions[file.description as usize].offset += count;
        Ok(count)
    })
}

fn read_socketpair_with_rights(
    state: &mut State,
    owner_pid: u64,
    description: FileDescription,
    output: &mut [u8],
    rights_out: &mut [u64],
) -> Result<(usize, usize), DescriptorError> {
    if output.is_empty() {
        return Ok((0, 0));
    }
    let pipe_index = description.pipe as usize;
    let pipe = *state
        .pipes
        .get(pipe_index)
        .filter(|pipe| pipe.used)
        .ok_or(DescriptorError::BadFile)?;
    if pipe.length == 0 {
        return if pipe.writers == 0 {
            Ok((0, 0))
        } else {
            Err(DescriptorError::Again)
        };
    }
    let count = output.len().min(pipe.length as usize);
    let deliver_rights = pipe.rights_count != 0 && count > pipe.rights_skip as usize;
    let queued_rights = if deliver_rights {
        pipe.rights_count as usize
    } else {
        0
    };
    let received_rights = queued_rights.min(rights_out.len());

    let mut descriptor_slots = [usize::MAX; MAX_SOCKET_RIGHTS];
    let mut descriptor_count = 0usize;
    for (index, descriptor) in state.files.iter().enumerate() {
        if !descriptor.used && descriptor_count < received_rights {
            descriptor_slots[descriptor_count] = index;
            descriptor_count += 1;
        }
    }
    if descriptor_count != received_rights {
        return Err(DescriptorError::TooMany);
    }
    let mut numbers = [0u8; MAX_SOCKET_RIGHTS];
    let mut number_count = 0usize;
    for candidate in 3..=u8::MAX {
        if number_count == received_rights {
            break;
        }
        if descriptor_slot(state, owner_pid, u64::from(candidate)).is_none()
            && !numbers[..number_count].contains(&candidate)
        {
            numbers[number_count] = candidate;
            number_count += 1;
            if number_count == received_rights {
                break;
            }
        }
    }
    if number_count != received_rights {
        return Err(DescriptorError::TooMany);
    }
    for description_index in pipe.rights[..queued_rights].iter().copied() {
        if !state.descriptions[description_index as usize].used {
            return Err(DescriptorError::Io);
        }
    }

    for (offset, byte) in output[..count].iter_mut().enumerate() {
        *byte = pipe.data[(pipe.head as usize + offset) % PIPE_BYTES];
    }
    {
        let pipe = &mut state.pipes[pipe_index];
        pipe.head = ((pipe.head as usize + count) % PIPE_BYTES) as u16;
        pipe.length -= count as u16;
        if deliver_rights {
            pipe.rights_count = 0;
            pipe.rights_skip = 0;
            pipe.rights.fill(0);
        } else if pipe.rights_count != 0 {
            pipe.rights_skip -= count as u16;
        }
    }

    for right_index in 0..received_rights {
        state.files[descriptor_slots[right_index]] = OpenFile {
            used: true,
            fd: numbers[right_index],
            description: pipe.rights[right_index],
            close_on_exec: false,
            owner_pid,
        };
        rights_out[right_index] = u64::from(numbers[right_index]);
    }
    for description_index in pipe.rights[received_rights..queued_rights].iter().copied() {
        release_description(state, description_index as usize);
    }
    Ok((count, received_rights))
}

pub fn snapshot(path: &[u8], output: &mut [u8; MAX_FILE_BYTES]) -> Option<usize> {
    with_state(|state| {
        if !state.mounted {
            return None;
        }
        let mut resolved = alloc::vec![0u8; MAX_PATH_BYTES];
        let path_length = resolve_vfs_path(state, current_pid(), path, &mut resolved, true).ok()?;
        let path = &resolved[..path_length];
        if let Some((index, inode)) = makfs4_by_path(path) {
            if inode.mode & 0o170000 != 0o100000
                || !crate::security::file_access(inode.mode, inode.uid, inode.gid, false)
            {
                return None;
            }
            output.fill(0);
            return crate::makfs4_volume::read_inode_at(index, 0, output).ok();
        }
        if let Some(file) = system_file_by_path(path) {
            if file.data.len() > output.len() {
                return None;
            }
            output.fill(0);
            output[..file.data.len()].copy_from_slice(file.data);
            return Some(file.data.len());
        }
        if let Some((_, package)) = package_file_by_path(state, path) {
            let length = usize::try_from(package.size).ok()?;
            if length > output.len() {
                return None;
            }
            output.fill(0);
            return (crate::fs::read_package_file(&package, 0, &mut output[..length])
                == Some(length))
            .then_some(length);
        }
        let (data, length): (&[u8; MAX_FILE_BYTES], usize) = if path == BOOT_FILE_PATH {
            if !crate::security::file_access(0o100644, crate::security::ROOT_UID, 0, false) {
                return None;
            }
            (&state.boot_data, state.boot_length)
        } else if path == USER_FILE_PATH {
            if !user_file_access(false) {
                return None;
            }
            (&state.user_data, state.user_length)
        } else {
            if !user_file_access(false) {
                return None;
            }
            let name = parse_dynamic_path(path)?;
            let slot = dynamic_index_by_name(state, name)?;
            if state.dynamic[slot].kind != KIND_FILE {
                return None;
            }
            (&state.dynamic[slot].data, state.dynamic[slot].data_length)
        };
        output.fill(0);
        output[..length].copy_from_slice(&data[..length]);
        Some(length)
    })
}

/// Read the kernel-owned account database through the mounted VFS cache.
/// Normal pathname lookup intentionally hides this file from userspace.
#[cfg(target_arch = "aarch64")]
pub(crate) fn system_account_snapshot(output: &mut [u8; MAX_FILE_BYTES]) -> Option<usize> {
    with_state(|state| {
        if !state.mounted {
            return None;
        }
        let slot = dynamic_index_by_name(state, ACCOUNT_DB_NAME)?;
        if state.dynamic[slot].kind != KIND_FILE {
            return None;
        }
        output.fill(0);
        let length = state.dynamic[slot].data_length;
        output[..length].copy_from_slice(&state.dynamic[slot].data[..length]);
        Some(length)
    })
}

/// Atomically replace the kernel-owned account database in MakFS, then update
/// its VFS cache only after the checksummed disk transaction succeeds.
#[cfg(target_arch = "aarch64")]
pub(crate) fn system_store_accounts(data: &[u8]) -> bool {
    if data.len() > MAX_FILE_BYTES {
        return false;
    }
    with_state(|state| {
        if !state.mounted {
            return false;
        }
        let slot = dynamic_index_by_name(state, ACCOUNT_DB_NAME)
            .or_else(|| state.dynamic.iter().position(|file| !file.used));
        let Some(slot) = slot else {
            return false;
        };
        if !crate::fs::store_dynamic_file(slot, ACCOUNT_DB_NAME, Some(data)) {
            return false;
        }
        let mut file = DynamicFile::EMPTY;
        file.used = true;
        file.kind = KIND_FILE;
        file.name[..ACCOUNT_DB_NAME.len()].copy_from_slice(ACCOUNT_DB_NAME);
        file.name_length = ACCOUNT_DB_NAME.len();
        file.data[..data.len()].copy_from_slice(data);
        file.data_length = data.len();
        file.modified_ticks = crate::arch::monotonic_ticks();
        state.dynamic[slot] = file;
        true
    })
}

pub fn write(fd: u64, input: &[u8]) -> Option<usize> {
    write_result(fd, input).ok()
}

pub fn write_result(fd: u64, input: &[u8]) -> Result<usize, DescriptorError> {
    with_state(|state| {
        let index = descriptor_slot(state, current_pid(), fd).ok_or(DescriptorError::BadFile)?;
        let file = state.files[index];
        let description = *state
            .descriptions
            .get(file.description as usize)
            .ok_or(DescriptorError::BadFile)?;
        if !description.used || description.kind == DESCRIPTION_PIPE_READ {
            return Err(DescriptorError::BadFile);
        }
        if matches!(
            description.kind,
            DESCRIPTION_PIPE_WRITE | DESCRIPTION_SOCKETPAIR
        ) {
            if input.is_empty() {
                return Ok(0);
            }
            let pipe = state
                .pipes
                .get_mut(if description.kind == DESCRIPTION_SOCKETPAIR {
                    description.peer_pipe as usize
                } else {
                    description.pipe as usize
                })
                .filter(|pipe| pipe.used)
                .ok_or(DescriptorError::BadFile)?;
            if pipe.readers == 0 {
                return Err(DescriptorError::BrokenPipe);
            }
            let available = PIPE_BYTES - pipe.length as usize;
            if available == 0 || (input.len() <= PIPE_BYTES && available < input.len()) {
                return Err(DescriptorError::Again);
            }
            let count = input.len().min(available);
            let tail = (pipe.head as usize + pipe.length as usize) % PIPE_BYTES;
            for (offset, byte) in input[..count].iter().enumerate() {
                pipe.data[(tail + offset) % PIPE_BYTES] = *byte;
            }
            pipe.length += count as u16;
            return Ok(count);
        }
        if matches!(description.kind, DESCRIPTION_NULL | DESCRIPTION_ZERO) {
            return if description.writable {
                Ok(input.len())
            } else {
                Err(DescriptorError::BadFile)
            };
        }
        if description.kind == DESCRIPTION_MAKFS4_FILE {
            if !description.writable {
                return Err(DescriptorError::BadFile);
            }
            let count = crate::makfs4_volume::write_inode_at(
                u32::from(description.makfs4_inode),
                description.offset as u64,
                input,
            )
            .map_err(makfs4_error)?;
            state.descriptions[file.description as usize].offset = description
                .offset
                .checked_add(count)
                .ok_or(DescriptorError::TooLarge)?;
            return Ok(count);
        }
        if description.kind == DESCRIPTION_SHMEM {
            if !description.writable {
                return Err(DescriptorError::BadFile);
            }
            let count = crate::aarch64_shmem::write(
                description.shmem_object,
                description.offset as u64,
                input,
            )
            .map_err(shmem_error)?;
            state.descriptions[file.description as usize].offset = description
                .offset
                .checked_add(count)
                .ok_or(DescriptorError::TooLarge)?;
            return Ok(count);
        }
        if description.kind != DESCRIPTION_FILE || !description.writable {
            return Err(DescriptorError::BadFile);
        }
        let count = input
            .len()
            .min(MAX_FILE_BYTES.saturating_sub(description.offset));
        let end = description.offset + count;
        match dynamic_index(description.node) {
            Some(slot) if state.dynamic[slot].used && state.dynamic[slot].kind == KIND_FILE => {
                let mut data = state.dynamic[slot].data;
                data[description.offset..end].copy_from_slice(&input[..count]);
                let length = state.dynamic[slot].data_length.max(end);
                let name_length = state.dynamic[slot].name_length;
                if !crate::fs::store_dynamic_file(
                    slot,
                    &state.dynamic[slot].name[..name_length],
                    Some(&data[..length]),
                ) {
                    return Err(DescriptorError::BadFile);
                }
                state.dynamic[slot].data = data;
                state.dynamic[slot].data_length = length;
                state.dynamic[slot].modified_ticks = crate::arch::monotonic_ticks();
            }
            None if description.node == NODE_USER => {
                let mut data = state.user_data;
                data[description.offset..end].copy_from_slice(&input[..count]);
                let length = state.user_length.max(end);
                if !crate::fs::store_user_file(&data[..length]) {
                    return Err(DescriptorError::BadFile);
                }
                state.user_data = data;
                state.user_length = length;
                state.user_modified_ticks = crate::arch::monotonic_ticks();
            }
            _ => return Err(DescriptorError::BadFile),
        }
        state.descriptions[file.description as usize].offset = end;
        Ok(count)
    })
}

pub fn read_at(fd: u64, output: &mut [u8], offset: u64) -> Result<usize, DescriptorError> {
    let offset = usize::try_from(offset).map_err(|_| DescriptorError::TooLarge)?;
    with_state(|state| {
        let index = descriptor_slot(state, current_pid(), fd).ok_or(DescriptorError::BadFile)?;
        let description = *state
            .descriptions
            .get(state.files[index].description as usize)
            .filter(|entry| entry.used)
            .ok_or(DescriptorError::BadFile)?;
        if matches!(
            description.kind,
            DESCRIPTION_PIPE_READ
                | DESCRIPTION_PIPE_WRITE
                | DESCRIPTION_SOCKETPAIR
                | DESCRIPTION_DIRECTORY
                | DESCRIPTION_MAKFS4_DIRECTORY
                | DESCRIPTION_PACKAGE_DIRECTORY
        ) {
            return Err(DescriptorError::IllegalSeek);
        }
        if !description.readable
            || !matches!(
                description.kind,
                DESCRIPTION_FILE
                    | DESCRIPTION_SYSTEM_FILE
                    | DESCRIPTION_MAKFS4_FILE
                    | DESCRIPTION_SHMEM
            )
        {
            return Err(DescriptorError::BadFile);
        }
        if description.kind == DESCRIPTION_MAKFS4_FILE {
            return crate::makfs4_volume::read_inode_at(
                u32::from(description.makfs4_inode),
                offset as u64,
                output,
            )
            .map_err(makfs4_error);
        }
        if description.kind == DESCRIPTION_SHMEM {
            return crate::aarch64_shmem::read(description.shmem_object, offset as u64, output)
                .map_err(shmem_error);
        }
        if description.kind != DESCRIPTION_SYSTEM_FILE && offset > MAX_FILE_BYTES {
            return Err(DescriptorError::TooLarge);
        }
        if description.kind == DESCRIPTION_SYSTEM_FILE
            && description.package_file != NO_PACKAGE_FILE
        {
            let package = description.package_snapshot.mounted();
            if !package.used {
                return Err(DescriptorError::BadFile);
            }
            return crate::fs::read_package_file(&package, offset as u64, output)
                .ok_or(DescriptorError::Io);
        }
        let (data, length): (&[u8], usize) = match dynamic_index(description.node) {
            Some(slot) if state.dynamic[slot].used && state.dynamic[slot].kind == KIND_FILE => {
                (&state.dynamic[slot].data, state.dynamic[slot].data_length)
            }
            None if description.node == NODE_BOOT => (&state.boot_data, state.boot_length),
            None if description.node == NODE_USER => (&state.user_data, state.user_length),
            Some(_) => return Err(DescriptorError::BadFile),
            None => system_file_by_node(description.node)
                .map(|file| (file.data, file.data.len()))
                .ok_or(DescriptorError::BadFile)?,
        };
        let count = output.len().min(length.saturating_sub(offset));
        output[..count].copy_from_slice(&data[offset..offset + count]);
        Ok(count)
    })
}

pub fn write_at(fd: u64, input: &[u8], offset: u64) -> Result<usize, DescriptorError> {
    let offset_usize = usize::try_from(offset).map_err(|_| DescriptorError::TooLarge)?;
    with_state(|state| {
        let index = descriptor_slot(state, current_pid(), fd).ok_or(DescriptorError::BadFile)?;
        let description = *state
            .descriptions
            .get(state.files[index].description as usize)
            .filter(|entry| entry.used)
            .ok_or(DescriptorError::BadFile)?;
        if matches!(
            description.kind,
            DESCRIPTION_PIPE_READ
                | DESCRIPTION_PIPE_WRITE
                | DESCRIPTION_SOCKETPAIR
                | DESCRIPTION_DIRECTORY
                | DESCRIPTION_MAKFS4_DIRECTORY
                | DESCRIPTION_PACKAGE_DIRECTORY
        ) {
            return Err(DescriptorError::IllegalSeek);
        }
        if description.kind == DESCRIPTION_MAKFS4_FILE {
            if !description.writable {
                return Err(DescriptorError::BadFile);
            }
            return crate::makfs4_volume::write_inode_at(
                u32::from(description.makfs4_inode),
                offset,
                input,
            )
            .map_err(makfs4_error);
        }
        if description.kind == DESCRIPTION_SHMEM {
            if !description.writable {
                return Err(DescriptorError::BadFile);
            }
            return crate::aarch64_shmem::write(description.shmem_object, offset, input)
                .map_err(shmem_error);
        }
        if offset_usize > MAX_FILE_BYTES {
            return Err(DescriptorError::TooLarge);
        }
        if description.kind != DESCRIPTION_FILE || !description.writable {
            return Err(DescriptorError::BadFile);
        }
        match dynamic_index(description.node) {
            Some(slot) if state.dynamic[slot].used && state.dynamic[slot].kind == KIND_FILE => {
                if offset_usize == MAX_FILE_BYTES && !input.is_empty() {
                    return Err(DescriptorError::TooLarge);
                }
                let count = input.len().min(MAX_FILE_BYTES - offset_usize);
                let end = offset_usize + count;
                let mut data = state.dynamic[slot].data;
                if offset_usize > state.dynamic[slot].data_length {
                    data[state.dynamic[slot].data_length..offset_usize].fill(0);
                }
                data[offset_usize..end].copy_from_slice(&input[..count]);
                let length = state.dynamic[slot].data_length.max(end);
                let name_length = state.dynamic[slot].name_length;
                if !crate::fs::store_dynamic_file(
                    slot,
                    &state.dynamic[slot].name[..name_length],
                    Some(&data[..length]),
                ) {
                    return Err(DescriptorError::Io);
                }
                state.dynamic[slot].data = data;
                state.dynamic[slot].data_length = length;
                state.dynamic[slot].modified_ticks = crate::arch::monotonic_ticks();
                Ok(count)
            }
            None if description.node == NODE_USER => {
                const USER_FILE_LIMIT: usize = 64;
                if offset_usize > USER_FILE_LIMIT
                    || (offset_usize == USER_FILE_LIMIT && !input.is_empty())
                {
                    return Err(DescriptorError::TooLarge);
                }
                let count = input.len().min(USER_FILE_LIMIT - offset_usize);
                let end = offset_usize + count;
                let mut data = state.user_data;
                if offset_usize > state.user_length {
                    data[state.user_length..offset_usize].fill(0);
                }
                data[offset_usize..end].copy_from_slice(&input[..count]);
                let length = state.user_length.max(end);
                if !crate::fs::store_user_file(&data[..length]) {
                    return Err(DescriptorError::Io);
                }
                state.user_data = data;
                state.user_length = length;
                state.user_modified_ticks = crate::arch::monotonic_ticks();
                Ok(count)
            }
            _ => Err(DescriptorError::BadFile),
        }
    })
}

pub fn truncate(fd: u64, length: u64) -> Result<(), DescriptorError> {
    with_state(|state| {
        let index = descriptor_slot(state, current_pid(), fd).ok_or(DescriptorError::BadFile)?;
        let description_index = state.files[index].description as usize;
        let description = *state
            .descriptions
            .get(description_index)
            .filter(|description| {
                description.used
                    && matches!(
                        description.kind,
                        DESCRIPTION_FILE | DESCRIPTION_MAKFS4_FILE | DESCRIPTION_SHMEM
                    )
                    && description.writable
            })
            .ok_or(DescriptorError::BadFile)?;
        if description.kind == DESCRIPTION_SHMEM {
            return crate::aarch64_shmem::truncate(description.shmem_object, length)
                .map_err(shmem_error);
        }
        if description.kind == DESCRIPTION_MAKFS4_FILE {
            return crate::makfs4_volume::truncate_inode(
                u32::from(description.makfs4_inode),
                length,
            )
            .map_err(makfs4_error);
        }
        let length = usize::try_from(length).map_err(|_| DescriptorError::TooLarge)?;
        if length > MAX_FILE_BYTES {
            return Err(DescriptorError::TooLarge);
        }
        match dynamic_index(description.node) {
            Some(slot) if state.dynamic[slot].used && state.dynamic[slot].kind == KIND_FILE => {
                let mut data = state.dynamic[slot].data;
                let old_length = state.dynamic[slot].data_length;
                if length > old_length {
                    data[old_length..length].fill(0);
                } else {
                    data[length..old_length].fill(0);
                }
                let name_length = state.dynamic[slot].name_length;
                if !crate::fs::store_dynamic_file(
                    slot,
                    &state.dynamic[slot].name[..name_length],
                    Some(&data[..length]),
                ) {
                    return Err(DescriptorError::Io);
                }
                state.dynamic[slot].data = data;
                state.dynamic[slot].data_length = length;
                state.dynamic[slot].modified_ticks = crate::arch::monotonic_ticks();
                Ok(())
            }
            None if description.node == NODE_USER && length <= 64 => {
                let mut data = state.user_data;
                let old_length = state.user_length;
                if length > old_length {
                    data[old_length..length].fill(0);
                } else {
                    data[length..old_length].fill(0);
                }
                if !crate::fs::store_user_file(&data[..length]) {
                    return Err(DescriptorError::Io);
                }
                state.user_data = data;
                state.user_length = length;
                state.user_modified_ticks = crate::arch::monotonic_ticks();
                Ok(())
            }
            None if description.node == NODE_USER => Err(DescriptorError::TooLarge),
            _ => Err(DescriptorError::BadFile),
        }
    })
}

pub fn sync(fd: u64) -> Result<(), DescriptorError> {
    let valid = with_state(|state| {
        descriptor_slot(state, current_pid(), fd)
            .and_then(|slot| {
                state
                    .descriptions
                    .get(state.files[slot].description as usize)
            })
            .is_some_and(|description| {
                description.used
                    && matches!(
                        description.kind,
                        DESCRIPTION_FILE
                            | DESCRIPTION_DIRECTORY
                            | DESCRIPTION_MAKFS4_FILE
                            | DESCRIPTION_MAKFS4_DIRECTORY
                            | DESCRIPTION_PACKAGE_DIRECTORY
                            | DESCRIPTION_SHMEM
                    )
            })
    });
    if !valid {
        return Err(DescriptorError::BadFile);
    }
    if crate::fs::sync() {
        Ok(())
    } else {
        Err(DescriptorError::Io)
    }
}

pub fn stat(path: &[u8]) -> Option<Metadata> {
    stat_impl(path, true)
}

pub fn lstat(path: &[u8]) -> Option<Metadata> {
    stat_impl(path, false)
}

fn stat_impl(path: &[u8], follow_final: bool) -> Option<Metadata> {
    with_state(|state| {
        if !state.mounted {
            return None;
        }
        let mut resolved = alloc::vec![0u8; MAX_PATH_BYTES];
        let path_length =
            resolve_vfs_path(state, current_pid(), path, &mut resolved, follow_final).ok()?;
        let path = &resolved[..path_length];
        if path == ROOT_PATH {
            return directory_metadata(state, NODE_DIRECTORY_ROOT);
        }
        if path == HOME_PATH {
            return directory_metadata(state, NODE_DIRECTORY_HOME);
        }
        if path == USER_DIRECTORY_PATH {
            return directory_metadata(state, NODE_DIRECTORY_USER);
        }
        if path == BOOT_FILE_PATH {
            return file_metadata(state, NODE_BOOT);
        }
        if path == USER_FILE_PATH {
            return file_metadata(state, NODE_USER);
        }
        if path == URANDOM_PATH {
            return Some(metadata(0o020666, 0, 0, 4, 0, 0, 9));
        }
        if path == NULL_PATH {
            return Some(metadata(0o020666, 0, 0, 4, 0, 0, 10));
        }
        if path == ZERO_PATH {
            return Some(metadata(0o020666, 0, 0, 4, 0, 0, 11));
        }
        if let Some(name) = shmem_name(path) {
            let value = crate::aarch64_shmem::metadata_named(name)?;
            return Some(metadata(
                value.mode,
                value.uid,
                value.gid,
                KIND_FILE,
                value.size,
                value.modified_ticks,
                value.inode,
            ));
        }
        if let Some(system_file) = system_file_by_path(path) {
            return Some(system_file_metadata(system_file));
        }
        if let Some((index, package)) = package_file_by_path(state, path) {
            return Some(package_file_metadata(index, package));
        }
        if package_directory_by_path(state, path).is_some() {
            return package_directory_metadata(state, path);
        }
        if let Some((_, inode)) = makfs4_by_path(path) {
            return Some(makfs4_metadata(inode));
        }
        let name = parse_dynamic_path(path)?;
        let slot = dynamic_index_by_name(state, name)?;
        let node = NODE_DYNAMIC_BASE + slot as u8;
        if state.dynamic[slot].kind == KIND_DIRECTORY {
            directory_metadata(state, node)
        } else {
            file_metadata(state, node)
        }
    })
}

pub fn stat_extended(path: &[u8], follow_final: bool) -> Option<ExtendedMetadata> {
    let persistent = with_state(|state| {
        if !state.mounted {
            return None;
        }
        let mut resolved = alloc::vec![0u8; MAX_PATH_BYTES];
        let length =
            resolve_vfs_path(state, current_pid(), path, &mut resolved, follow_final).ok()?;
        makfs4_by_path(&resolved[..length]).map(|(_, inode)| makfs4_extended_metadata(inode))
    });
    persistent.or_else(|| {
        let metadata = if follow_final {
            stat(path)
        } else {
            lstat(path)
        }?;
        Some(legacy_extended_metadata(metadata))
    })
}

pub fn metadata_extended_for_fd(fd: u64) -> Result<ExtendedMetadata, DescriptorError> {
    let persistent = with_state(|state| {
        let slot = descriptor_slot(state, current_pid(), fd)?;
        let description = *state
            .descriptions
            .get(state.files[slot].description as usize)?;
        matches!(
            description.kind,
            DESCRIPTION_MAKFS4_FILE | DESCRIPTION_MAKFS4_DIRECTORY
        )
        .then(|| makfs4_description_inode(description).map(makfs4_extended_metadata))
        .flatten()
    });
    if let Some(metadata) = persistent {
        return Ok(metadata);
    }
    metadata_for_fd(fd).map(legacy_extended_metadata)
}

pub fn metadata_for_fd(fd: u64) -> Result<Metadata, DescriptorError> {
    with_state(|state| {
        let slot = descriptor_slot(state, current_pid(), fd).ok_or(DescriptorError::BadFile)?;
        let description = *state
            .descriptions
            .get(state.files[slot].description as usize)
            .filter(|description| description.used)
            .ok_or(DescriptorError::BadFile)?;
        if description.kind == DESCRIPTION_FILE {
            return file_metadata(state, description.node).ok_or(DescriptorError::BadFile);
        }
        if description.kind == DESCRIPTION_SYSTEM_FILE {
            if description.package_file != NO_PACKAGE_FILE {
                let index = description.package_file as usize;
                return description
                    .package_snapshot
                    .used
                    .then(|| package_file_metadata(index, description.package_snapshot.mounted()))
                    .ok_or(DescriptorError::BadFile);
            }
            return system_file_by_node(description.node)
                .map(system_file_metadata)
                .ok_or(DescriptorError::BadFile);
        }
        if description.kind == DESCRIPTION_DIRECTORY {
            return directory_metadata(state, description.node).ok_or(DescriptorError::BadFile);
        }
        if description.kind == DESCRIPTION_PACKAGE_DIRECTORY {
            let path = package_description_path(&description).ok_or(DescriptorError::BadFile)?;
            return package_directory_metadata(state, path).ok_or(DescriptorError::BadFile);
        }
        if matches!(
            description.kind,
            DESCRIPTION_MAKFS4_FILE | DESCRIPTION_MAKFS4_DIRECTORY
        ) {
            return makfs4_description_inode(description)
                .map(makfs4_metadata)
                .ok_or(DescriptorError::BadFile);
        }
        if description.kind == DESCRIPTION_RANDOM {
            return Ok(metadata(0o020666, 0, 0, 4, 0, 0, 9));
        }
        if description.kind == DESCRIPTION_NULL {
            return Ok(metadata(0o020666, 0, 0, 4, 0, 0, 10));
        }
        if description.kind == DESCRIPTION_ZERO {
            return Ok(metadata(0o020666, 0, 0, 4, 0, 0, 11));
        }
        if description.kind == DESCRIPTION_SHMEM {
            let value = crate::aarch64_shmem::metadata(description.shmem_object)
                .ok_or(DescriptorError::BadFile)?;
            return Ok(metadata(
                value.mode,
                value.uid,
                value.gid,
                KIND_FILE,
                value.size,
                value.modified_ticks,
                value.inode,
            ));
        }
        if matches!(
            description.kind,
            DESCRIPTION_PIPE_READ | DESCRIPTION_PIPE_WRITE | DESCRIPTION_SOCKETPAIR
        ) {
            let pipe = state
                .pipes
                .get(description.pipe as usize)
                .filter(|pipe| pipe.used)
                .ok_or(DescriptorError::BadFile)?;
            let credentials = crate::security::credentials();
            return Ok(metadata(
                0o010600,
                credentials.uid,
                credentials.gid,
                3,
                pipe.length as u64,
                0,
                0x1000 + u64::from(description.pipe),
            ));
        }
        Err(DescriptorError::BadFile)
    })
}

pub(crate) fn read_only_backing_for_fd(fd: u64) -> Option<ReadOnlyFileBacking> {
    with_state(|state| {
        let slot = descriptor_slot(state, current_pid(), fd)?;
        let description = *state
            .descriptions
            .get(state.files[slot].description as usize)?;
        if !description.used || description.kind != DESCRIPTION_SYSTEM_FILE {
            return None;
        }
        if description.package_file != NO_PACKAGE_FILE {
            return description
                .package_snapshot
                .used
                .then_some(ReadOnlyFileBacking::Package(
                    description.package_snapshot.mounted(),
                ));
        }
        system_file_by_node(description.node).map(|file| ReadOnlyFileBacking::Embedded(file.data))
    })
}

pub(crate) fn shared_memory_backing_for_fd(fd: u64, writable: bool) -> Option<SharedMemoryBacking> {
    with_state(|state| {
        let slot = descriptor_slot(state, current_pid(), fd)?;
        let description = *state
            .descriptions
            .get(state.files[slot].description as usize)?;
        if !description.used
            || description.kind != DESCRIPTION_SHMEM
            || !description.readable
            || (writable && !description.writable)
        {
            return None;
        }
        let metadata = crate::aarch64_shmem::metadata(description.shmem_object)?;
        Some(SharedMemoryBacking {
            object: description.shmem_object,
            size: metadata.size,
        })
    })
}

pub fn read_dir(path: &[u8], index: usize) -> Option<DirectoryEntry> {
    with_state(|state| {
        let mut resolved = alloc::vec![0u8; MAX_PATH_BYTES];
        let path_length = resolve_vfs_path(state, current_pid(), path, &mut resolved, true).ok()?;
        let path = &resolved[..path_length];
        if path == USER_DIRECTORY_PATH {
            let legacy_count = 1 + dynamic_child_count(state, NODE_DIRECTORY_USER);
            if index < legacy_count {
                return directory_entry(state, NODE_DIRECTORY_USER, index);
            }
            return crate::makfs4_volume::child_at(1, index - legacy_count)
                .ok()
                .flatten()
                .and_then(|(_, inode)| makfs4_directory_entry(inode));
        }
        if let Some((_, inode)) = makfs4_by_path(path) {
            if inode.mode & 0o170000 != 0o040000 {
                return None;
            }
            return crate::makfs4_volume::child_at(inode.inode, index)
                .ok()
                .flatten()
                .and_then(|(_, child)| makfs4_directory_entry(child));
        }
        if package_directory_by_path(state, path).is_some() {
            return package_directory_child(state, path, index);
        }
        let node = directory_node(state, path)?;
        directory_entry(state, node, index)
    })
}

pub fn read_directory_fd(fd: u64) -> Result<Option<(DirectoryEntry, u64)>, DescriptorError> {
    with_state(|state| {
        let slot = descriptor_slot(state, current_pid(), fd).ok_or(DescriptorError::BadFile)?;
        let description_index = state.files[slot].description as usize;
        let description = *state
            .descriptions
            .get(description_index)
            .filter(|description| {
                description.used
                    && matches!(
                        description.kind,
                        DESCRIPTION_DIRECTORY
                            | DESCRIPTION_MAKFS4_DIRECTORY
                            | DESCRIPTION_PACKAGE_DIRECTORY
                    )
            })
            .ok_or(DescriptorError::BadFile)?;
        if description.kind == DESCRIPTION_MAKFS4_DIRECTORY {
            let Some((entry, next)) =
                makfs4_directory_fd_entry(description, description.offset).map_err(makfs4_error)?
            else {
                return Ok(None);
            };
            state.descriptions[description_index].offset = next;
            return Ok(Some((entry, next as u64)));
        }
        if description.kind == DESCRIPTION_PACKAGE_DIRECTORY {
            let path = package_description_path(&description).ok_or(DescriptorError::BadFile)?;
            let entry = package_directory_fd_entry(state, path, description.offset);
            if entry.is_some() {
                state.descriptions[description_index].offset += 1;
            }
            return Ok(
                entry.map(|entry| (entry, state.descriptions[description_index].offset as u64))
            );
        }
        let entry = directory_fd_entry(state, description.node, description.offset);
        if entry.is_some() {
            state.descriptions[description_index].offset += 1;
        }
        Ok(entry.map(|entry| (entry, state.descriptions[description_index].offset as u64)))
    })
}

pub fn change_working_directory(path: &[u8]) -> Result<(), DescriptorError> {
    with_state(|state| {
        if !state.mounted {
            return Err(DescriptorError::BadFile);
        }
        let owner_pid = current_pid();
        let mut resolved = alloc::vec![0u8; MAX_PATH_BYTES];
        let length = resolve_vfs_path(state, owner_pid, path, &mut resolved, true)?;
        let path = &resolved[..length];
        let metadata = if let Some((_, inode)) = makfs4_by_path(path) {
            if inode.mode & 0o170000 != 0o040000 {
                return Err(DescriptorError::NotDirectory);
            }
            makfs4_metadata(inode)
        } else if let Some(metadata) = package_directory_metadata(state, path) {
            metadata
        } else {
            let node = directory_node(state, path).ok_or(DescriptorError::BadFile)?;
            directory_metadata(state, node).ok_or(DescriptorError::BadFile)?
        };
        if !crate::security::file_access(metadata.mode, metadata.uid, metadata.gid, false) {
            return Err(DescriptorError::BadFile);
        }
        let slot = state
            .working_directories
            .iter()
            .position(|entry| entry.used && entry.owner_pid == owner_pid)
            .or_else(|| {
                state
                    .working_directories
                    .iter()
                    .position(|entry| !entry.used)
            })
            .ok_or(DescriptorError::TooMany)?;
        let mut entry = WorkingDirectory::EMPTY;
        entry.used = true;
        entry.owner_pid = owner_pid;
        entry.length = length as u16;
        entry.path[..length].copy_from_slice(path);
        state.working_directories[slot] = entry;
        Ok(())
    })
}

pub fn working_directory(output: &mut [u8]) -> Result<usize, DescriptorError> {
    with_state(|state| {
        if !state.mounted {
            return Err(DescriptorError::BadFile);
        }
        let path = working_directory_for(state, current_pid());
        if output.len() < path.len() {
            return Err(DescriptorError::Invalid);
        }
        output[..path.len()].copy_from_slice(path);
        Ok(path.len())
    })
}

pub fn close(fd: u64) -> bool {
    with_state(|state| {
        let owner_pid = current_pid();
        let Some(index) = descriptor_slot(state, owner_pid, fd) else {
            return false;
        };
        let description = state.files[index].description as usize;
        let lock_key = state
            .descriptions
            .get(description)
            .filter(|description| description.used)
            .and_then(|description| description_lock_key(*description));
        state.files[index] = OpenFile::EMPTY;
        release_description(state, description);
        if let Some(lock_key) = lock_key {
            release_record_locks(state, owner_pid, lock_key);
        }
        true
    })
}

/// Duplicate one descriptor. Both descriptors reference one open-file
/// description, so reads, writes, and seeks share offset as required by POSIX.
pub fn duplicate(fd: u64) -> Option<u64> {
    with_state(|state| {
        let owner_pid = current_pid();
        let index = descriptor_slot(state, owner_pid, fd)?;
        let mut file = state.files[index];
        let new_fd = (3..=u8::MAX)
            .find(|candidate| descriptor_slot(state, owner_pid, u64::from(*candidate)).is_none())?;
        let destination = state.files.iter().position(|entry| !entry.used)?;
        let description = state.descriptions.get_mut(file.description as usize)?;
        if !description.used || description.references == u8::MAX {
            return None;
        }
        description.references += 1;
        file.fd = new_fd;
        file.close_on_exec = false;
        state.files[destination] = file;
        Some(u64::from(new_fd))
    })
}

pub fn duplicate_to(fd: u64, new_fd: u64, close_on_exec: bool) -> Result<u64, DescriptorError> {
    if !(3..=u64::from(u8::MAX)).contains(&new_fd) || fd == new_fd {
        return Err(DescriptorError::Invalid);
    }
    with_state(|state| {
        let owner_pid = current_pid();
        let source = descriptor_slot(state, owner_pid, fd).ok_or(DescriptorError::BadFile)?;
        let mut file = state.files[source];
        let destination = if let Some(existing) = descriptor_slot(state, owner_pid, new_fd) {
            let old_description = state.files[existing].description as usize;
            let old_lock_key = state
                .descriptions
                .get(old_description)
                .filter(|description| description.used)
                .and_then(|description| description_lock_key(*description));
            state.files[existing] = OpenFile::EMPTY;
            release_description(state, old_description);
            if let Some(lock_key) = old_lock_key {
                release_record_locks(state, owner_pid, lock_key);
            }
            existing
        } else {
            state
                .files
                .iter()
                .position(|entry| !entry.used)
                .ok_or(DescriptorError::TooMany)?
        };
        let description = state
            .descriptions
            .get_mut(file.description as usize)
            .ok_or(DescriptorError::BadFile)?;
        if !description.used || description.references == u8::MAX {
            return Err(DescriptorError::TooMany);
        }
        description.references += 1;
        file.fd = new_fd as u8;
        file.close_on_exec = close_on_exec;
        state.files[destination] = file;
        Ok(new_fd)
    })
}

pub fn duplicate_min(fd: u64, minimum: u64, close_on_exec: bool) -> Result<u64, DescriptorError> {
    if minimum > u64::from(u8::MAX) {
        return Err(DescriptorError::Invalid);
    }
    with_state(|state| {
        let owner_pid = current_pid();
        let source = descriptor_slot(state, owner_pid, fd).ok_or(DescriptorError::BadFile)?;
        let mut file = state.files[source];
        let first = minimum.max(3) as u8;
        let new_fd = (first..=u8::MAX)
            .find(|candidate| descriptor_slot(state, owner_pid, u64::from(*candidate)).is_none())
            .ok_or(DescriptorError::TooMany)?;
        let destination = state
            .files
            .iter()
            .position(|entry| !entry.used)
            .ok_or(DescriptorError::TooMany)?;
        let description = state
            .descriptions
            .get_mut(file.description as usize)
            .ok_or(DescriptorError::BadFile)?;
        if !description.used || description.references == u8::MAX {
            return Err(DescriptorError::TooMany);
        }
        description.references += 1;
        file.fd = new_fd;
        file.close_on_exec = close_on_exec;
        state.files[destination] = file;
        Ok(u64::from(new_fd))
    })
}

pub fn descriptor_flags(fd: u64) -> Result<u64, DescriptorError> {
    with_state(|state| {
        let slot = descriptor_slot(state, current_pid(), fd).ok_or(DescriptorError::BadFile)?;
        Ok(u64::from(state.files[slot].close_on_exec))
    })
}

pub fn set_descriptor_flags(fd: u64, flags: u64) -> Result<u64, DescriptorError> {
    if flags & !1 != 0 {
        return Err(DescriptorError::Invalid);
    }
    with_state(|state| {
        let slot = descriptor_slot(state, current_pid(), fd).ok_or(DescriptorError::BadFile)?;
        state.files[slot].close_on_exec = flags & 1 != 0;
        Ok(0)
    })
}

pub fn status_flags(fd: u64) -> Result<u64, DescriptorError> {
    with_state(|state| {
        let slot = descriptor_slot(state, current_pid(), fd).ok_or(DescriptorError::BadFile)?;
        let description = state
            .descriptions
            .get(state.files[slot].description as usize)
            .filter(|description| description.used)
            .ok_or(DescriptorError::BadFile)?;
        Ok(u64::from(description.status_flags))
    })
}

pub fn set_status_flags(fd: u64, flags: u64) -> Result<u64, DescriptorError> {
    if flags & !(0x3 | u64::from(STATUS_NONBLOCK)) != 0 {
        return Err(DescriptorError::Invalid);
    }
    with_state(|state| {
        let slot = descriptor_slot(state, current_pid(), fd).ok_or(DescriptorError::BadFile)?;
        let description = state
            .descriptions
            .get_mut(state.files[slot].description as usize)
            .filter(|description| description.used)
            .ok_or(DescriptorError::BadFile)?;
        description.status_flags =
            (description.status_flags & 0x3) | (flags as u32 & STATUS_NONBLOCK);
        Ok(0)
    })
}

/// Return first conflicting POSIX byte-range lock, or `F_UNLCK` when clear.
pub fn get_file_lock(fd: u64, lock: &mut FileLock) -> Result<u64, DescriptorError> {
    with_state(|state| {
        let owner_pid = current_pid();
        let slot = descriptor_slot(state, owner_pid, fd).ok_or(DescriptorError::BadFile)?;
        let description = *state
            .descriptions
            .get(state.files[slot].description as usize)
            .filter(|description| description.used && description_lock_key(**description).is_some())
            .ok_or(DescriptorError::BadFile)?;
        let key = description_lock_key(description).ok_or(DescriptorError::BadFile)?;
        let exclusive = requested_lock_mode(description, lock.lock_type)?;
        let (start, end) = normalize_lock_range(state, description, lock)?;
        let conflict = state.record_locks.iter().find(|existing| {
            existing.used
                && existing.key == key
                && existing.owner_pid != owner_pid
                && ranges_overlap(start, end, existing.start, existing.end)
                && (exclusive || existing.exclusive)
        });
        if let Some(conflict) = conflict {
            lock.lock_type = if conflict.exclusive { 1 } else { 0 };
            lock.whence = 0;
            lock.padding = 0;
            lock.start = conflict.start as i64;
            lock.length = if conflict.end == u64::MAX {
                0
            } else {
                i64::try_from(conflict.end - conflict.start).unwrap_or(i64::MAX)
            };
            lock.pid = i32::try_from(conflict.owner_pid).unwrap_or(i32::MAX);
            lock.reserved = 0;
        } else {
            lock.lock_type = 2;
            lock.pid = 0;
        }
        Ok(0)
    })
}

/// Atomically replace this process's overlapping byte-range locks. Conflicts
/// return `EAGAIN`; syscall layer supplies blocking retry for `F_SETLKW`.
pub fn set_file_lock(fd: u64, lock: &FileLock) -> Result<u64, DescriptorError> {
    with_state(|state| {
        let owner_pid = current_pid();
        let slot = descriptor_slot(state, owner_pid, fd).ok_or(DescriptorError::BadFile)?;
        let description = *state
            .descriptions
            .get(state.files[slot].description as usize)
            .filter(|description| description.used && description_lock_key(**description).is_some())
            .ok_or(DescriptorError::BadFile)?;
        let key = description_lock_key(description).ok_or(DescriptorError::BadFile)?;
        let exclusive = if lock.lock_type == 2 {
            false
        } else {
            requested_lock_mode(description, lock.lock_type)?
        };
        let (start, end) = normalize_lock_range(state, description, lock)?;
        if lock.lock_type != 2
            && state.record_locks.iter().any(|existing| {
                existing.used
                    && existing.key == key
                    && existing.owner_pid != owner_pid
                    && ranges_overlap(start, end, existing.start, existing.end)
                    && (exclusive || existing.exclusive)
            })
        {
            return Err(DescriptorError::Again);
        }

        let mut updated = state.record_locks;
        unlock_record_range(&mut updated, owner_pid, key, start, end)?;
        if lock.lock_type != 2 {
            let mut merged_start = start;
            let mut merged_end = end;
            for existing in &mut updated {
                if existing.used
                    && existing.owner_pid == owner_pid
                    && existing.key == key
                    && existing.exclusive == exclusive
                    && existing.start <= merged_end
                    && merged_start <= existing.end
                {
                    merged_start = merged_start.min(existing.start);
                    merged_end = merged_end.max(existing.end);
                    *existing = RecordLock::EMPTY;
                }
            }
            let destination = updated
                .iter()
                .position(|existing| !existing.used)
                .ok_or(DescriptorError::TooMany)?;
            updated[destination] = RecordLock {
                used: true,
                exclusive,
                key,
                owner_pid,
                start: merged_start,
                end: merged_end,
            };
        }
        state.record_locks = updated;
        Ok(0)
    })
}

fn requested_lock_mode(
    description: FileDescription,
    lock_type: i16,
) -> Result<bool, DescriptorError> {
    match lock_type {
        0 if description.readable => Ok(false),
        1 if description.writable => Ok(true),
        0 | 1 => Err(DescriptorError::BadFile),
        _ => Err(DescriptorError::Invalid),
    }
}

fn normalize_lock_range(
    state: &State,
    description: FileDescription,
    lock: &FileLock,
) -> Result<(u64, u64), DescriptorError> {
    let base = match lock.whence {
        0 => 0i128,
        1 => description.offset as i128,
        2 => i128::from(description_length(state, description).ok_or(DescriptorError::BadFile)?),
        _ => return Err(DescriptorError::Invalid),
    };
    let anchor = base
        .checked_add(i128::from(lock.start))
        .ok_or(DescriptorError::TooLarge)?;
    let (start, end) = match lock.length.cmp(&0) {
        core::cmp::Ordering::Equal => (anchor, i128::from(u64::MAX)),
        core::cmp::Ordering::Greater => (
            anchor,
            anchor
                .checked_add(i128::from(lock.length))
                .ok_or(DescriptorError::TooLarge)?,
        ),
        core::cmp::Ordering::Less => (
            anchor
                .checked_add(i128::from(lock.length))
                .ok_or(DescriptorError::TooLarge)?,
            anchor,
        ),
    };
    if start < 0 || end <= start || end > i128::from(u64::MAX) {
        return Err(DescriptorError::Invalid);
    }
    Ok((start as u64, end as u64))
}

fn unlock_record_range(
    locks: &mut [RecordLock; MAX_RECORD_LOCKS],
    owner_pid: u64,
    key: u32,
    start: u64,
    end: u64,
) -> Result<(), DescriptorError> {
    for index in 0..locks.len() {
        let existing = locks[index];
        if !existing.used
            || existing.owner_pid != owner_pid
            || existing.key != key
            || !ranges_overlap(start, end, existing.start, existing.end)
        {
            continue;
        }
        if start <= existing.start && end >= existing.end {
            locks[index] = RecordLock::EMPTY;
        } else if start <= existing.start {
            locks[index].start = end;
        } else if end >= existing.end {
            locks[index].end = start;
        } else {
            let destination = locks
                .iter()
                .position(|candidate| !candidate.used)
                .ok_or(DescriptorError::TooMany)?;
            locks[index].end = start;
            locks[destination] = RecordLock {
                start: end,
                ..existing
            };
        }
    }
    Ok(())
}

const fn ranges_overlap(
    first_start: u64,
    first_end: u64,
    second_start: u64,
    second_end: u64,
) -> bool {
    first_start < second_end && second_start < first_end
}

/// Set open-file-description offset. `whence`: 0=set, 1=current, 2=end.
/// Seeking past EOF is valid up to bounded MakFS maximum file size.
pub fn seek(fd: u64, offset: i64, whence: u64) -> Option<u64> {
    with_state(|state| {
        let index = descriptor_slot(state, current_pid(), fd)?;
        let file = state.files[index];
        let description = *state.descriptions.get(file.description as usize)?;
        if !description.used {
            return None;
        }
        let length = if description.kind == DESCRIPTION_DIRECTORY {
            directory_length(state, description.node)? as u64
        } else if description.kind == DESCRIPTION_PACKAGE_DIRECTORY {
            package_directory_length(state, package_description_path(&description)?)? as u64
        } else if description.kind == DESCRIPTION_SYSTEM_FILE
            && description.package_file != NO_PACKAGE_FILE
        {
            description.package_snapshot.used.then_some(())?;
            description.package_snapshot.size
        } else {
            description_length(state, description)?
        };
        let base = match whence {
            0 => 0i128,
            1 => description.offset as i128,
            2 => i128::from(length),
            _ => return None,
        };
        let next = base.checked_add(i128::from(offset))?;
        let maximum = match description.kind {
            DESCRIPTION_SYSTEM_FILE => length,
            DESCRIPTION_MAKFS4_FILE => crate::makfs4_volume::maximum_file_bytes(),
            DESCRIPTION_MAKFS4_DIRECTORY => crate::makfs4_volume::directory_cursor_limit(),
            DESCRIPTION_PACKAGE_DIRECTORY => length,
            DESCRIPTION_SHMEM => length,
            _ => MAX_FILE_BYTES as u64,
        };
        if !(0..=i128::from(maximum)).contains(&next) {
            return None;
        }
        state.descriptions[file.description as usize].offset = usize::try_from(next).ok()?;
        Some(next as u64)
    })
}

pub fn create(path: &[u8]) -> bool {
    with_state(|state| {
        if !state.mounted {
            return false;
        }
        let mut resolved = alloc::vec![0u8; MAX_PATH_BYTES];
        let Ok(path_length) = resolve_vfs_path(state, current_pid(), path, &mut resolved, false)
        else {
            return false;
        };
        let path = &resolved[..path_length];
        if let Some(name) = shmem_name(path) {
            let credentials = crate::security::credentials();
            let result =
                crate::aarch64_shmem::create(name, 0o600, credentials.uid, credentials.gid);
            if result.is_err()
                || SHMEM_CREATE_TRACES.fetch_add(1, Ordering::Relaxed) < SHMEM_TRACE_LIMIT
            {
                crate::serial_println!(
                    "MAKOS_SHMEM_CREATE name={} result={:?}",
                    core::str::from_utf8(name).unwrap_or("<invalid>"),
                    result,
                );
            }
            return result.is_ok();
        }
        if !user_file_access(true) {
            return false;
        }
        if crate::makfs4_volume::mounted() {
            let Some((parent, name)) = makfs4_parent_and_name(path) else {
                return false;
            };
            if !makfs4_parent_writable(parent)
                || makfs4_by_path(path).is_some()
                || parse_dynamic_path(path)
                    .and_then(|legacy| dynamic_index_by_name(state, legacy))
                    .is_some()
            {
                return false;
            }
            let credentials = crate::security::credentials();
            return crate::makfs4_volume::create_inode(
                parent,
                name,
                0o100600,
                credentials.uid,
                credentials.gid,
            )
            .is_ok();
        }
        let Some(name) = parse_dynamic_path(path) else {
            return false;
        };
        if dynamic_index_by_name(state, name).is_some()
            || parent_directory_node(state, name).is_none()
        {
            return false;
        }
        let Some(slot) = state.dynamic.iter().position(|file| !file.used) else {
            return false;
        };
        if !crate::fs::store_dynamic_file(slot, name, Some(&[])) {
            return false;
        }
        let mut file = DynamicFile::EMPTY;
        file.used = true;
        file.kind = KIND_FILE;
        file.name[..name.len()].copy_from_slice(name);
        file.name_length = name.len();
        file.modified_ticks = crate::arch::monotonic_ticks();
        state.dynamic[slot] = file;
        true
    })
}

pub fn create_symlink(target: &[u8], link_path: &[u8]) -> Result<(), DescriptorError> {
    with_state(|state| {
        if !state.mounted || !user_file_access(true) {
            return Err(DescriptorError::Permission);
        }
        if target.is_empty() || target.len() >= MAX_PATH_BYTES || target.contains(&0) {
            return Err(DescriptorError::Invalid);
        }
        let mut resolved = alloc::vec![0u8; MAX_PATH_BYTES];
        let length = resolve_vfs_path(state, current_pid(), link_path, &mut resolved, false)?;
        let path = &resolved[..length];
        let (parent, name) = makfs4_parent_and_name_result(path)?;
        if !makfs4_parent_writable(parent) {
            return Err(DescriptorError::Permission);
        }
        if makfs4_by_path(path).is_some()
            || system_file_by_path(path).is_some()
            || package_file_by_path(state, path).is_some()
            || package_directory_by_path(state, path).is_some()
            || parse_dynamic_path(path)
                .and_then(|legacy| dynamic_index_by_name(state, legacy))
                .is_some()
        {
            return Err(DescriptorError::Exists);
        }
        let credentials = crate::security::credentials();
        crate::makfs4_volume::create_symlink_inode(
            parent,
            name,
            target,
            credentials.uid,
            credentials.gid,
        )
        .map_err(makfs4_error)?;
        Ok(())
    })
}

/// Copy a link target exactly, without a trailing NUL, matching `readlink(2)`.
pub fn read_link(path: &[u8], output: &mut [u8]) -> Result<usize, DescriptorError> {
    if output.is_empty() {
        return Err(DescriptorError::Invalid);
    }
    with_state(|state| {
        if !state.mounted {
            return Err(DescriptorError::BadFile);
        }
        let mut resolved = alloc::vec![0u8; MAX_PATH_BYTES];
        let length = resolve_vfs_path(state, current_pid(), path, &mut resolved, false)?;
        let path = &resolved[..length];
        if let Some((index, inode)) = makfs4_by_path(path) {
            if inode.mode & 0o170000 != 0o120000 {
                return Err(DescriptorError::Invalid);
            }
            let count = output
                .len()
                .min(usize::try_from(inode.size).unwrap_or(usize::MAX));
            let copied = crate::makfs4_volume::read_inode_at(index, 0, &mut output[..count])
                .map_err(makfs4_error)?;
            return (copied == count)
                .then_some(copied)
                .ok_or(DescriptorError::Io);
        }
        let virtual_node_exists = matches!(
            path,
            ROOT_PATH
                | HOME_PATH
                | USER_DIRECTORY_PATH
                | BOOT_FILE_PATH
                | USER_FILE_PATH
                | URANDOM_PATH
                | NULL_PATH
                | ZERO_PATH
        ) || shmem_name(path)
            .and_then(crate::aarch64_shmem::metadata_named)
            .is_some()
            || system_file_by_path(path).is_some()
            || package_file_by_path(state, path).is_some()
            || package_directory_by_path(state, path).is_some()
            || parse_dynamic_path(path)
                .and_then(|name| dynamic_index_by_name(state, name))
                .is_some();
        if virtual_node_exists {
            Err(DescriptorError::Invalid)
        } else {
            Err(DescriptorError::NotFound)
        }
    })
}

pub fn unlink(path: &[u8]) -> bool {
    with_state(|state| {
        if !state.mounted {
            return false;
        }
        let mut resolved = alloc::vec![0u8; MAX_PATH_BYTES];
        let Ok(path_length) = resolve_vfs_path(state, current_pid(), path, &mut resolved, false)
        else {
            return false;
        };
        let path = &resolved[..path_length];
        if let Some(name) = shmem_name(path) {
            let result = crate::aarch64_shmem::unlink(name);
            if result.is_err()
                || SHMEM_UNLINK_TRACES.fetch_add(1, Ordering::Relaxed) < SHMEM_TRACE_LIMIT
            {
                crate::serial_println!(
                    "MAKOS_SHMEM_UNLINK name={} result={:?}",
                    core::str::from_utf8(name).unwrap_or("<invalid>"),
                    result,
                );
            }
            return result.is_ok();
        }
        if !user_file_access(true) {
            return false;
        }
        if let Some((index, inode)) = makfs4_by_path(path) {
            if !matches!(inode.mode & 0o170000, 0o100000 | 0o120000)
                || !makfs4_parent_writable(inode.parent)
                || state.descriptions.iter().any(|description| {
                    description.used
                        && description.makfs4_inode != NO_MAKFS4_INODE
                        && u32::from(description.makfs4_inode) == index
                })
            {
                return false;
            }
            return crate::makfs4_volume::remove_inode(index).is_ok();
        }
        let Some(name) = parse_dynamic_path(path) else {
            return false;
        };
        let Some(slot) = dynamic_index_by_name(state, name) else {
            return false;
        };
        if state.dynamic[slot].kind != KIND_FILE {
            return false;
        }
        let node = NODE_DYNAMIC_BASE + slot as u8;
        if state
            .descriptions
            .iter()
            .any(|file| file.used && file.node == node)
            || !crate::fs::store_dynamic_file(slot, name, None)
        {
            return false;
        }
        state.dynamic[slot] = DynamicFile::EMPTY;
        true
    })
}

pub fn create_directory(path: &[u8]) -> Result<(), DescriptorError> {
    with_state(|state| {
        if !state.mounted {
            return Err(DescriptorError::BadFile);
        }
        if !user_node_access(KIND_DIRECTORY, true) {
            return Err(DescriptorError::Permission);
        }
        let mut resolved = alloc::vec![0u8; MAX_PATH_BYTES];
        let path_length = resolve_vfs_path(state, current_pid(), path, &mut resolved, false)?;
        let path = &resolved[..path_length];
        if crate::makfs4_volume::mounted() {
            let (parent, name) = makfs4_parent_and_name_result(path)?;
            if !makfs4_parent_writable(parent) {
                return Err(DescriptorError::Permission);
            }
            if makfs4_by_path(path).is_some()
                || parse_dynamic_path(path)
                    .and_then(|legacy| dynamic_index_by_name(state, legacy))
                    .is_some()
            {
                return Err(DescriptorError::Exists);
            }
            let credentials = crate::security::credentials();
            crate::makfs4_volume::create_inode(
                parent,
                name,
                0o040700,
                credentials.uid,
                credentials.gid,
            )
            .map_err(makfs4_error)?;
            return Ok(());
        }
        let name = parse_dynamic_path(path).ok_or(DescriptorError::Invalid)?;
        if dynamic_index_by_name(state, name).is_some() {
            return Err(DescriptorError::Exists);
        }
        parent_directory_node(state, name).ok_or(DescriptorError::NotDirectory)?;
        let slot = state
            .dynamic
            .iter()
            .position(|node| !node.used)
            .ok_or(DescriptorError::TooMany)?;
        if !crate::fs::store_dynamic_directory(slot, name, true) {
            return Err(DescriptorError::Io);
        }
        let mut directory = DynamicFile::EMPTY;
        directory.used = true;
        directory.kind = KIND_DIRECTORY;
        directory.name[..name.len()].copy_from_slice(name);
        directory.name_length = name.len();
        directory.modified_ticks = crate::arch::monotonic_ticks();
        state.dynamic[slot] = directory;
        Ok(())
    })
}

pub fn remove_directory(path: &[u8]) -> Result<(), DescriptorError> {
    with_state(|state| {
        if !state.mounted {
            return Err(DescriptorError::BadFile);
        }
        if !user_node_access(KIND_DIRECTORY, true) {
            return Err(DescriptorError::Permission);
        }
        let mut resolved = alloc::vec![0u8; MAX_PATH_BYTES];
        let path_length = resolve_vfs_path(state, current_pid(), path, &mut resolved, false)?;
        let path = &resolved[..path_length];
        if let Some((index, inode)) = makfs4_by_path(path) {
            if inode.mode & 0o170000 != 0o040000 {
                return Err(DescriptorError::NotDirectory);
            }
            if !makfs4_parent_writable(inode.parent) {
                return Err(DescriptorError::Permission);
            }
            if crate::makfs4_volume::child_at(inode.inode, 0)
                .map_err(makfs4_error)?
                .is_some()
            {
                return Err(DescriptorError::NotEmpty);
            }
            if state.descriptions.iter().any(|description| {
                description.used
                    && description.makfs4_inode != NO_MAKFS4_INODE
                    && u32::from(description.makfs4_inode) == index
            }) || state.working_directories.iter().any(|entry| {
                entry.used && path_is_same_or_descendant(&entry.path[..entry.length as usize], path)
            }) {
                return Err(DescriptorError::Busy);
            }
            crate::makfs4_volume::remove_inode(index).map_err(makfs4_error)?;
            return Ok(());
        }
        let name = parse_dynamic_path(path).ok_or(DescriptorError::Invalid)?;
        let slot = dynamic_index_by_name(state, name).ok_or(DescriptorError::NotFound)?;
        if state.dynamic[slot].kind != KIND_DIRECTORY {
            return Err(DescriptorError::NotDirectory);
        }
        if dynamic_directory_has_children(state, name) {
            return Err(DescriptorError::NotEmpty);
        }
        let node = NODE_DYNAMIC_BASE + slot as u8;
        let absolute_length = path_length;
        if state
            .descriptions
            .iter()
            .any(|entry| entry.used && entry.node == node)
            || state.working_directories.iter().any(|entry| {
                entry.used
                    && path_is_same_or_descendant(
                        &entry.path[..entry.length as usize],
                        &resolved[..absolute_length],
                    )
            })
        {
            return Err(DescriptorError::Busy);
        }
        if !crate::fs::store_dynamic_directory(slot, name, false) {
            return Err(DescriptorError::Io);
        }
        state.dynamic[slot] = DynamicFile::EMPTY;
        Ok(())
    })
}

pub fn rename(source: &[u8], destination: &[u8]) -> bool {
    with_state(|state| {
        if !state.mounted || !user_file_access(true) {
            return false;
        }
        let mut resolved_source = alloc::vec![0u8; MAX_PATH_BYTES];
        let mut resolved_destination = alloc::vec![0u8; MAX_PATH_BYTES];
        let Ok(source_length) =
            resolve_vfs_path(state, current_pid(), source, &mut resolved_source, false)
        else {
            return false;
        };
        let Ok(destination_length) = resolve_vfs_path(
            state,
            current_pid(),
            destination,
            &mut resolved_destination,
            false,
        ) else {
            return false;
        };
        let source_path = &resolved_source[..source_length];
        let destination_path = &resolved_destination[..destination_length];
        if let Some((index, inode)) = makfs4_by_path(source_path) {
            // POSIX rename of one existing path onto itself is a successful
            // no-op. Avoid routing the same inode through replacement logic.
            if source_path == destination_path {
                return true;
            }
            let Some((parent, name)) = makfs4_parent_and_name(destination_path) else {
                return false;
            };
            let destination = makfs4_by_path(destination_path);
            if !makfs4_parent_writable(inode.parent)
                || !makfs4_parent_writable(parent)
                || parse_dynamic_path(destination_path)
                    .and_then(|legacy| dynamic_index_by_name(state, legacy))
                    .is_some()
            {
                return false;
            }
            return match destination {
                Some((destination_index, destination_inode))
                    if inode.mode & 0o170000 != 0o040000
                        && destination_inode.mode & 0o170000 != 0o040000 =>
                {
                    crate::makfs4_volume::replace_inode(index, destination_index, parent, name)
                        .is_ok()
                }
                Some(_) => false,
                None => crate::makfs4_volume::rename_inode(index, parent, name).is_ok(),
            };
        }
        let Some(source_name) = parse_dynamic_path(source_path) else {
            return false;
        };
        let Some(destination_name) = parse_dynamic_path(destination_path) else {
            return false;
        };
        let Some(slot) = dynamic_index_by_name(state, source_name) else {
            return false;
        };
        if state.dynamic[slot].kind != KIND_FILE
            || parent_directory_node(state, destination_name).is_none()
        {
            return false;
        }
        if source_name == destination_name {
            return true;
        }
        if dynamic_index_by_name(state, destination_name).is_some() {
            return false;
        }
        let node = NODE_DYNAMIC_BASE + slot as u8;
        if state
            .descriptions
            .iter()
            .any(|file| file.used && file.node == node)
        {
            return false;
        }
        let length = state.dynamic[slot].data_length;
        if !crate::fs::store_dynamic_file(
            slot,
            destination_name,
            Some(&state.dynamic[slot].data[..length]),
        ) {
            return false;
        }
        state.dynamic[slot].name.fill(0);
        state.dynamic[slot].name[..destination_name.len()].copy_from_slice(destination_name);
        state.dynamic[slot].name_length = destination_name.len();
        state.dynamic[slot].modified_ticks = crate::arch::monotonic_ticks();
        true
    })
}

pub fn close_all(pid: u64) -> usize {
    with_state(|state| {
        let mut closed = 0usize;
        for index in 0..state.files.len() {
            if state.files[index].used && state.files[index].owner_pid == pid {
                let description = state.files[index].description as usize;
                state.files[index] = OpenFile::EMPTY;
                release_description(state, description);
                closed += 1;
            }
        }
        for entry in &mut state.working_directories {
            if entry.used && entry.owner_pid == pid {
                *entry = WorkingDirectory::EMPTY;
            }
        }
        for lock in &mut state.record_locks {
            if lock.used && lock.owner_pid == pid {
                *lock = RecordLock::EMPTY;
            }
        }
        closed
    })
}

/// POSIX fork inheritance: descriptor numbers/flags copied while open-file
/// descriptions (offset/status/pipe endpoints) stay shared.
pub fn inherit_process(parent_pid: u64, child_pid: u64) -> bool {
    if parent_pid == 0 || child_pid == 0 || parent_pid == child_pid {
        return false;
    }
    with_state(|state| {
        let needed = state
            .files
            .iter()
            .filter(|file| file.used && file.owner_pid == parent_pid)
            .count();
        if state.files.iter().filter(|file| !file.used).count() < needed
            || state
                .files
                .iter()
                .any(|file| file.used && file.owner_pid == child_pid)
        {
            return false;
        }
        for file in state
            .files
            .iter()
            .filter(|file| file.used && file.owner_pid == parent_pid)
        {
            let description = &state.descriptions[file.description as usize];
            if !description.used || description.references == u8::MAX {
                return false;
            }
        }
        let parent_cwd = state
            .working_directories
            .iter()
            .copied()
            .find(|entry| entry.used && entry.owner_pid == parent_pid);
        if parent_cwd.is_some() && state.working_directories.iter().all(|entry| entry.used) {
            return false;
        }
        for index in 0..state.files.len() {
            let file = state.files[index];
            if !file.used || file.owner_pid != parent_pid {
                continue;
            }
            let destination = state
                .files
                .iter_mut()
                .find(|candidate| !candidate.used)
                .expect("FD fork preflight mismatch");
            *destination = OpenFile {
                owner_pid: child_pid,
                ..file
            };
            state.descriptions[file.description as usize].references += 1;
        }
        if let Some(cwd) = parent_cwd {
            let destination = state
                .working_directories
                .iter_mut()
                .find(|entry| !entry.used)
                .expect("CWD fork preflight mismatch");
            *destination = WorkingDirectory {
                owner_pid: child_pid,
                ..cwd
            };
        }
        true
    })
}

/// Close descriptors carrying FD_CLOEXEC while preserving shared descriptions
/// and every other descriptor owned by process.
pub fn close_on_exec(pid: u64) -> usize {
    with_state(|state| {
        let mut closed = 0usize;
        for index in 0..state.files.len() {
            if state.files[index].used
                && state.files[index].owner_pid == pid
                && state.files[index].close_on_exec
            {
                let description = state.files[index].description as usize;
                let lock_key = state
                    .descriptions
                    .get(description)
                    .filter(|description| description.used)
                    .and_then(|description| description_lock_key(*description));
                state.files[index] = OpenFile::EMPTY;
                release_description(state, description);
                if let Some(lock_key) = lock_key {
                    release_record_locks(state, pid, lock_key);
                }
                closed += 1;
            }
        }
        closed
    })
}

fn release_description(state: &mut State, index: usize) {
    let Some(description) = state.descriptions.get(index).copied() else {
        return;
    };
    if !description.used || description.references == 0 {
        return;
    }
    state.descriptions[index].references -= 1;
    if state.descriptions[index].references != 0 {
        return;
    }
    state.descriptions[index] = FileDescription::EMPTY;
    if description.kind == DESCRIPTION_SHMEM {
        crate::aarch64_shmem::release_handle(description.shmem_object);
    }
    if !matches!(
        description.kind,
        DESCRIPTION_PIPE_READ | DESCRIPTION_PIPE_WRITE | DESCRIPTION_SOCKETPAIR
    ) {
        return;
    }

    let pipe_index = description.pipe as usize;
    if description.kind == DESCRIPTION_SOCKETPAIR {
        let destroy_read_pipe = if let Some(pipe) = state.pipes.get_mut(pipe_index) {
            pipe.readers = pipe.readers.saturating_sub(1);
            pipe.readers == 0 && pipe.writers == 0
        } else {
            false
        };
        let peer_index = description.peer_pipe as usize;
        let destroy_write_pipe = if peer_index != pipe_index {
            if let Some(peer) = state.pipes.get_mut(peer_index) {
                peer.writers = peer.writers.saturating_sub(1);
                peer.readers == 0 && peer.writers == 0
            } else {
                false
            }
        } else {
            destroy_read_pipe
        };
        if destroy_read_pipe {
            destroy_pipe(state, pipe_index);
        }
        if peer_index != pipe_index && destroy_write_pipe {
            destroy_pipe(state, peer_index);
        }
        return;
    }

    let destroy = if let Some(pipe) = state.pipes.get_mut(pipe_index) {
        if description.kind == DESCRIPTION_PIPE_READ {
            pipe.readers = pipe.readers.saturating_sub(1);
        } else {
            pipe.writers = pipe.writers.saturating_sub(1);
        }
        pipe.readers == 0 && pipe.writers == 0
    } else {
        false
    };
    if destroy {
        destroy_pipe(state, pipe_index);
    }
}

fn destroy_pipe(state: &mut State, index: usize) {
    let Some(pipe) = state.pipes.get(index).copied().filter(|pipe| pipe.used) else {
        return;
    };
    state.pipes[index] = Pipe::EMPTY;
    for description in pipe.rights[..pipe.rights_count as usize].iter().copied() {
        release_description(state, description as usize);
    }
}

fn release_record_locks(state: &mut State, owner_pid: u64, key: u32) -> usize {
    let mut released = 0;
    for lock in &mut state.record_locks {
        if lock.used && lock.owner_pid == owner_pid && lock.key == key {
            *lock = RecordLock::EMPTY;
            released += 1;
        }
    }
    released
}

pub fn is_pipe_owned(fd: u64) -> bool {
    with_state(|state| {
        let Some(slot) = descriptor_slot(state, current_pid(), fd) else {
            return false;
        };
        state
            .descriptions
            .get(state.files[slot].description as usize)
            .is_some_and(|description| {
                description.used
                    && matches!(
                        description.kind,
                        DESCRIPTION_PIPE_READ | DESCRIPTION_PIPE_WRITE | DESCRIPTION_SOCKETPAIR
                    )
            })
    })
}

/// Stable object key shared by both ends of a pipe/socketpair. Direct syscall
/// waits use it to avoid waking tasks blocked on unrelated IPC objects.
pub fn io_wait_key(fd: u64) -> Option<u64> {
    const PIPE_TAG: u64 = 1 << 16;
    const SOCKETPAIR_TAG: u64 = 2 << 16;
    with_state(|state| {
        let slot = descriptor_slot(state, current_pid(), fd)?;
        let description = state
            .descriptions
            .get(state.files[slot].description as usize)?;
        if !description.used {
            return None;
        }
        match description.kind {
            DESCRIPTION_PIPE_READ | DESCRIPTION_PIPE_WRITE => {
                Some(PIPE_TAG | u64::from(description.pipe))
            }
            DESCRIPTION_SOCKETPAIR => {
                let low = description.pipe.min(description.peer_pipe);
                let high = description.pipe.max(description.peer_pipe);
                Some(SOCKETPAIR_TAG | u64::from(low) | (u64::from(high) << 8))
            }
            _ => None,
        }
    })
}

pub fn local_socket_option(fd: u64, operation: u64) -> Option<u64> {
    with_state(|state| {
        let slot = descriptor_slot(state, current_pid(), fd)?;
        let description = state
            .descriptions
            .get(state.files[slot].description as usize)?;
        if !description.used || description.kind != DESCRIPTION_SOCKETPAIR {
            return None;
        }
        match operation {
            6 => Some(1),                 // SO_TYPE = SOCK_STREAM
            7 => Some(0),                 // SO_ERROR
            8 => Some(PIPE_BYTES as u64), // SO_RCVBUF / SO_SNDBUF
            9 => Some(1),                 // SO_DOMAIN = AF_UNIX
            10 => Some(0),                // SO_PROTOCOL
            13 => Some(0),                // shutdown: accepted for local IPC
            _ => None,
        }
    })
}

pub fn is_nonblocking(fd: u64) -> Result<bool, DescriptorError> {
    with_state(|state| {
        let slot = descriptor_slot(state, current_pid(), fd).ok_or(DescriptorError::BadFile)?;
        let description = state
            .descriptions
            .get(state.files[slot].description as usize)
            .filter(|description| description.used)
            .ok_or(DescriptorError::BadFile)?;
        Ok(description.status_flags & STATUS_NONBLOCK != 0)
    })
}

pub fn poll_events(fd: u64, requested: u16) -> u16 {
    const POLLIN: u16 = 0x001;
    const POLLOUT: u16 = 0x004;
    const POLLERR: u16 = 0x008;
    const POLLHUP: u16 = 0x010;
    const POLLNVAL: u16 = 0x020;
    with_state(|state| {
        let Some(slot) = descriptor_slot(state, current_pid(), fd) else {
            return POLLNVAL;
        };
        let Some(description) = state
            .descriptions
            .get(state.files[slot].description as usize)
            .filter(|description| description.used)
        else {
            return POLLNVAL;
        };
        match description.kind {
            DESCRIPTION_FILE
            | DESCRIPTION_SYSTEM_FILE
            | DESCRIPTION_DIRECTORY
            | DESCRIPTION_MAKFS4_FILE
            | DESCRIPTION_MAKFS4_DIRECTORY
            | DESCRIPTION_PACKAGE_DIRECTORY
            | DESCRIPTION_RANDOM => {
                let mut ready = 0;
                if description.readable {
                    ready |= requested & POLLIN;
                }
                if description.writable {
                    ready |= requested & POLLOUT;
                }
                ready
            }
            DESCRIPTION_PIPE_READ => {
                let Some(pipe) = state.pipes.get(description.pipe as usize) else {
                    return POLLNVAL;
                };
                let mut ready = 0;
                if pipe.length != 0 || pipe.writers == 0 {
                    ready |= requested & POLLIN;
                }
                if pipe.writers == 0 {
                    ready |= POLLHUP;
                }
                ready
            }
            DESCRIPTION_PIPE_WRITE => {
                let Some(pipe) = state.pipes.get(description.pipe as usize) else {
                    return POLLNVAL;
                };
                let mut ready = 0;
                if pipe.readers == 0 {
                    ready |= POLLERR;
                } else if pipe.length as usize != PIPE_BYTES {
                    ready |= requested & POLLOUT;
                }
                ready
            }
            DESCRIPTION_SOCKETPAIR => {
                let Some(read_pipe) = state.pipes.get(description.pipe as usize) else {
                    return POLLNVAL;
                };
                let Some(write_pipe) = state.pipes.get(description.peer_pipe as usize) else {
                    return POLLNVAL;
                };
                let mut ready = 0;
                if read_pipe.length != 0 || read_pipe.writers == 0 {
                    ready |= requested & POLLIN;
                }
                if read_pipe.writers == 0 {
                    ready |= POLLHUP;
                }
                if write_pipe.readers == 0 {
                    ready |= POLLERR;
                } else if write_pipe.length as usize != PIPE_BYTES {
                    ready |= requested & POLLOUT;
                }
                ready
            }
            _ => POLLNVAL,
        }
    })
}

fn descriptor_slot(state: &State, owner_pid: u64, fd: u64) -> Option<usize> {
    let fd = u8::try_from(fd).ok()?;
    state
        .files
        .iter()
        .position(|file| file.used && file.owner_pid == owner_pid && file.fd == fd)
}

fn file_length(state: &State, node: u8) -> Option<usize> {
    match dynamic_index(node) {
        Some(slot) if state.dynamic[slot].used && state.dynamic[slot].kind == KIND_FILE => {
            Some(state.dynamic[slot].data_length)
        }
        None if node == NODE_BOOT => Some(state.boot_length),
        None if node == NODE_USER => Some(state.user_length),
        Some(_) => None,
        None => system_file_by_node(node).map(|file| file.data.len()),
    }
}

fn shmem_name(path: &[u8]) -> Option<&[u8]> {
    const PREFIX: &[u8] = b"/dev/shm/";
    let name = path.strip_prefix(PREFIX)?;
    (!name.is_empty() && name.len() <= 128 && !name.contains(&b'/')).then_some(name)
}

fn shmem_error(error: crate::aarch64_shmem::Error) -> DescriptorError {
    match error {
        crate::aarch64_shmem::Error::Exists => DescriptorError::Exists,
        crate::aarch64_shmem::Error::NotFound => DescriptorError::NotFound,
        crate::aarch64_shmem::Error::Permission => DescriptorError::Permission,
        crate::aarch64_shmem::Error::NoSpace => DescriptorError::NoSpace,
        crate::aarch64_shmem::Error::TooLarge => DescriptorError::TooLarge,
        crate::aarch64_shmem::Error::Invalid => DescriptorError::Invalid,
    }
}

fn makfs4_error(error: crate::makfs4_volume::MountError) -> DescriptorError {
    match error {
        crate::makfs4_volume::MountError::NoSpace => DescriptorError::NoSpace,
        crate::makfs4_volume::MountError::Geometry => DescriptorError::Invalid,
        crate::makfs4_volume::MountError::Io
        | crate::makfs4_volume::MountError::Corrupt
        | crate::makfs4_volume::MountError::PackageOverlap => DescriptorError::Io,
    }
}

/// Canonicalize a process path, then resolve persistent MakFS4 links. Link
/// targets may be absolute or relative to the containing directory and may
/// cross into package/system VFS namespaces. Forty substitutions match POSIX
/// `ELOOP` practice; every intermediate component is followed.
fn resolve_vfs_path(
    state: &State,
    owner_pid: u64,
    input: &[u8],
    output: &mut [u8],
    follow_final: bool,
) -> Result<usize, DescriptorError> {
    let mut length =
        resolve_path(state, owner_pid, input, output).ok_or(DescriptorError::Invalid)?;
    let mut scratch = alloc::vec![0u8; MAX_PATH_BYTES];
    for _ in 0..40 {
        let Some((index, inode, component_start, component_end)) =
            first_makfs4_symlink(&output[..length])?
        else {
            return Ok(length);
        };
        if !follow_final && component_end == length {
            return Ok(length);
        }
        let target_length = usize::try_from(inode.size)
            .ok()
            .filter(|length| *length != 0 && *length < MAX_PATH_BYTES)
            .ok_or(DescriptorError::Invalid)?;
        let mut target = alloc::vec![0u8; target_length];
        let copied =
            crate::makfs4_volume::read_inode_at(index, 0, &mut target).map_err(makfs4_error)?;
        if copied != target_length || target.contains(&0) {
            return Err(DescriptorError::Io);
        }
        scratch.fill(0);
        let suffix = &output[component_end..length];
        let mut candidate_length = 0usize;
        if target.first() == Some(&b'/') {
            if target
                .len()
                .checked_add(suffix.len())
                .is_none_or(|total| total >= MAX_PATH_BYTES)
            {
                return Err(DescriptorError::TooLarge);
            }
            scratch[..target.len()].copy_from_slice(&target);
            candidate_length = target.len();
        } else {
            if component_start
                .checked_add(target.len())
                .and_then(|total| total.checked_add(suffix.len()))
                .is_none_or(|total| total >= MAX_PATH_BYTES)
            {
                return Err(DescriptorError::TooLarge);
            }
            scratch[..component_start].copy_from_slice(&output[..component_start]);
            candidate_length = component_start;
            scratch[candidate_length..candidate_length + target.len()].copy_from_slice(&target);
            candidate_length += target.len();
        }
        scratch[candidate_length..candidate_length + suffix.len()].copy_from_slice(suffix);
        candidate_length += suffix.len();
        let mut canonical = alloc::vec![0u8; MAX_PATH_BYTES];
        length = resolve_path(
            state,
            owner_pid,
            &scratch[..candidate_length],
            &mut canonical,
        )
        .ok_or(DescriptorError::Invalid)?;
        output[..length].copy_from_slice(&canonical[..length]);
        output[length..].fill(0);
    }
    Err(DescriptorError::Loop)
}

/// Find first symlink while checking that traversed non-final nodes are dirs.
fn first_makfs4_symlink(
    path: &[u8],
) -> Result<Option<(u32, makos_makfs4::Inode, usize, usize)>, DescriptorError> {
    if !crate::makfs4_volume::mounted() {
        return Ok(None);
    }
    let Some(relative) = path.strip_prefix(USER_PREFIX) else {
        return Ok(None);
    };
    if relative.is_empty() {
        return Ok(None);
    }
    let mut parent = 1u64;
    let mut relative_start = 0usize;
    for component in relative.split(|byte| *byte == b'/') {
        if !valid_makfs4_component(component) {
            return Err(DescriptorError::Invalid);
        }
        let component_start = USER_PREFIX.len() + relative_start;
        let component_end = component_start + component.len();
        let Some(index) =
            crate::makfs4_volume::find_child(parent, component).map_err(makfs4_error)?
        else {
            return Ok(None);
        };
        let inode = crate::makfs4_volume::read_inode(index)
            .map_err(makfs4_error)?
            .ok_or(DescriptorError::NotFound)?;
        let kind = inode.mode & 0o170000;
        if kind == 0o120000 {
            return Ok(Some((index, inode, component_start, component_end)));
        }
        if component_end != path.len() && kind != 0o040000 {
            return Err(DescriptorError::NotDirectory);
        }
        parent = inode.inode;
        relative_start += component.len() + 1;
    }
    Ok(None)
}

fn makfs4_by_path(path: &[u8]) -> Option<(u32, makos_makfs4::Inode)> {
    if !crate::makfs4_volume::mounted() {
        return None;
    }
    let relative = path.strip_prefix(USER_PREFIX)?;
    if relative.is_empty() {
        return None;
    }
    let mut parent = 1u64;
    let mut result = None;
    for component in relative.split(|byte| *byte == b'/') {
        if component.is_empty()
            || component == b"."
            || component == b".."
            || component.len() > makos_makfs4::MAX_PATH_BYTES
            || component.contains(&0)
        {
            return None;
        }
        let index = crate::makfs4_volume::find_child(parent, component).ok()??;
        let inode = crate::makfs4_volume::read_inode(index).ok()??;
        parent = inode.inode;
        result = Some((index, inode));
    }
    result
}

fn valid_makfs4_component(component: &[u8]) -> bool {
    !component.is_empty()
        && component != b"."
        && component != b".."
        && component.len() <= makos_makfs4::MAX_PATH_BYTES
        && !component.contains(&0)
        && !component.contains(&b'/')
}

fn makfs4_parent_and_name(path: &[u8]) -> Option<(u64, &[u8])> {
    makfs4_parent_and_name_result(path).ok()
}

fn makfs4_parent_and_name_result(path: &[u8]) -> Result<(u64, &[u8]), DescriptorError> {
    if !crate::makfs4_volume::mounted() {
        return Err(DescriptorError::BadFile);
    }
    let relative = path
        .strip_prefix(USER_PREFIX)
        .ok_or(DescriptorError::Invalid)?;
    let (parent_path, name) = match relative.iter().rposition(|byte| *byte == b'/') {
        Some(separator) => (&relative[..separator], &relative[separator + 1..]),
        None => (&[][..], relative),
    };
    if !valid_makfs4_component(name)
        || (parent_path.is_empty() && (name == USER_FILE_NAME || name == ACCOUNT_DB_NAME))
    {
        return Err(DescriptorError::Invalid);
    }
    let mut parent = 1u64;
    if !parent_path.is_empty() {
        for component in parent_path.split(|byte| *byte == b'/') {
            if !valid_makfs4_component(component) {
                return Err(DescriptorError::Invalid);
            }
            let index = crate::makfs4_volume::find_child(parent, component)
                .map_err(makfs4_error)?
                .ok_or(DescriptorError::NotFound)?;
            let inode = crate::makfs4_volume::read_inode(index)
                .map_err(makfs4_error)?
                .ok_or(DescriptorError::NotFound)?;
            if inode.mode & 0o170000 != 0o040000 {
                return Err(DescriptorError::NotDirectory);
            }
            parent = inode.inode;
        }
    }
    Ok((parent, name))
}

fn makfs4_parent_writable(parent: u64) -> bool {
    let Some(index) = parent
        .checked_sub(1)
        .and_then(|index| u32::try_from(index).ok())
    else {
        return false;
    };
    crate::makfs4_volume::read_inode(index)
        .ok()
        .flatten()
        .is_some_and(|inode| {
            inode.mode & 0o170000 == 0o040000
                && crate::security::file_access(inode.mode, inode.uid, inode.gid, true)
        })
}

fn makfs4_metadata(inode: makos_makfs4::Inode) -> Metadata {
    let kind = match inode.mode & 0o170000 {
        0o040000 => KIND_DIRECTORY,
        0o120000 => KIND_SYMLINK,
        _ => KIND_FILE,
    };
    metadata(
        inode.mode,
        inode.uid,
        inode.gid,
        kind,
        inode.size,
        inode.modified_ns / 10_000_000,
        0x40_0000 + inode.inode,
    )
}

fn makfs4_extended_metadata(inode: makos_makfs4::Inode) -> ExtendedMetadata {
    let base = makfs4_metadata(inode);
    // Zero extension fields identify legacy records. Preserve their prior
    // bounded monotonic timestamp rather than fabricating a Unix epoch.
    let accessed_ns = if inode.accessed_seconds == 0 {
        inode.modified_ns
    } else {
        u64::from(inode.accessed_seconds).saturating_mul(1_000_000_000)
    };
    let changed_ns = if inode.changed_seconds == 0 {
        inode.modified_ns
    } else {
        u64::from(inode.changed_seconds).saturating_mul(1_000_000_000)
    };
    ExtendedMetadata {
        mode: base.mode,
        uid: base.uid,
        gid: base.gid,
        kind: base.kind,
        size: base.size,
        accessed_ns,
        modified_ns: inode.modified_ns,
        changed_ns,
        inode: base.inode,
    }
}

fn legacy_extended_metadata(metadata: Metadata) -> ExtendedMetadata {
    let timestamp = metadata.modified_ticks.saturating_mul(10_000_000);
    ExtendedMetadata {
        mode: metadata.mode,
        uid: metadata.uid,
        gid: metadata.gid,
        kind: metadata.kind,
        size: metadata.size,
        accessed_ns: timestamp,
        modified_ns: timestamp,
        changed_ns: timestamp,
        inode: metadata.inode,
    }
}

fn makfs4_description_inode(description: FileDescription) -> Option<makos_makfs4::Inode> {
    (description.makfs4_inode != NO_MAKFS4_INODE)
        .then(|| crate::makfs4_volume::read_inode(u32::from(description.makfs4_inode)).ok())
        .flatten()
        .flatten()
}

fn description_lock_key(description: FileDescription) -> Option<u32> {
    match description.kind {
        DESCRIPTION_FILE => Some(u32::from(description.node)),
        DESCRIPTION_MAKFS4_FILE => Some(0x1_0000 | u32::from(description.makfs4_inode)),
        _ => None,
    }
}

fn description_length(state: &State, description: FileDescription) -> Option<u64> {
    match description.kind {
        DESCRIPTION_SYSTEM_FILE if description.package_snapshot.used => {
            Some(description.package_snapshot.size)
        }
        DESCRIPTION_FILE | DESCRIPTION_SYSTEM_FILE => {
            file_length(state, description.node).map(|length| length as u64)
        }
        DESCRIPTION_MAKFS4_FILE => makfs4_description_inode(description).map(|inode| inode.size),
        DESCRIPTION_MAKFS4_DIRECTORY => makfs4_directory_length(description),
        DESCRIPTION_PACKAGE_DIRECTORY => {
            package_directory_length(state, package_description_path(&description)?)
                .map(|length| length as u64)
        }
        DESCRIPTION_SHMEM => {
            crate::aarch64_shmem::metadata(description.shmem_object).map(|metadata| metadata.size)
        }
        _ => None,
    }
}

fn system_file_by_path(path: &[u8]) -> Option<SystemFile> {
    SYSTEM_FILES.iter().copied().find(|file| file.path == path)
}

fn package_file_by_path(state: &State, path: &[u8]) -> Option<(usize, MountedPackageFile)> {
    state
        .packages
        .iter()
        .copied()
        .enumerate()
        .find(|(_, file)| file.used && &file.path[..file.path_length] == path)
}

fn package_child_parts<'a>(
    file: &'a MountedPackageFile,
    directory: &[u8],
) -> Option<(&'a [u8], bool)> {
    let path = &file.path[..file.path_length];
    if !file.used
        || path.len() <= directory.len() + 1
        || !path.starts_with(directory)
        || path[directory.len()] != b'/'
    {
        return None;
    }
    let remainder = &path[directory.len() + 1..];
    let name_length = remainder
        .iter()
        .position(|byte| *byte == b'/')
        .unwrap_or(remainder.len());
    (name_length != 0).then_some((&remainder[..name_length], name_length < remainder.len()))
}

fn package_directory_by_path(state: &State, path: &[u8]) -> Option<usize> {
    state
        .packages
        .iter()
        .enumerate()
        .find_map(|(index, file)| package_child_parts(file, path).map(|_| index))
}

fn package_description_path(description: &FileDescription) -> Option<&[u8]> {
    let length = usize::from(description.package_path_length);
    (description.kind == DESCRIPTION_PACKAGE_DIRECTORY
        && length != 0
        && length <= description.package_path.len())
    .then_some(&description.package_path[..length])
}

fn package_directory_inode(path: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in path {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    0x30_0000 | (hash & 0x0f_ffff)
}

fn package_directory_child(
    state: &State,
    directory: &[u8],
    target: usize,
) -> Option<DirectoryEntry> {
    let mut child_index = 0usize;
    for (file_index, file) in state.packages.iter().enumerate() {
        let Some((name, is_directory)) = package_child_parts(file, directory) else {
            continue;
        };
        let duplicate = state.packages[..file_index].iter().any(|previous| {
            package_child_parts(previous, directory)
                .is_some_and(|(previous_name, _)| previous_name == name)
        });
        if duplicate {
            continue;
        }
        if child_index != target {
            child_index += 1;
            continue;
        }
        let mut entry = DirectoryEntry {
            inode: if is_directory {
                package_directory_inode(name)
            } else {
                0x20_0000 + file_index as u64
            },
            kind: if is_directory {
                KIND_DIRECTORY
            } else {
                KIND_FILE
            },
            name_length: name.len() as u32,
            name: [0; DIRECTORY_NAME_BYTES],
        };
        entry.name[..name.len()].copy_from_slice(name);
        return Some(entry);
    }
    None
}

fn package_directory_length(state: &State, directory: &[u8]) -> Option<usize> {
    package_directory_by_path(state, directory)?;
    let mut count = 0usize;
    while package_directory_child(state, directory, count).is_some() {
        count += 1;
    }
    count.checked_add(2)
}

fn package_directory_metadata(state: &State, path: &[u8]) -> Option<Metadata> {
    let length = package_directory_length(state, path)?;
    Some(metadata(
        0o040555,
        crate::security::ROOT_UID,
        0,
        KIND_DIRECTORY,
        length as u64,
        0,
        package_directory_inode(path),
    ))
}

fn package_directory_fd_entry(
    state: &State,
    directory: &[u8],
    index: usize,
) -> Option<DirectoryEntry> {
    if index >= 2 {
        return package_directory_child(state, directory, index - 2);
    }
    let name: &[u8] = if index == 0 { b"." } else { b".." };
    let parent_end = directory
        .iter()
        .rposition(|byte| *byte == b'/')
        .filter(|end| *end != 0)
        .unwrap_or(1);
    let mut entry = DirectoryEntry {
        inode: if index == 0 {
            package_directory_inode(directory)
        } else {
            package_directory_inode(&directory[..parent_end])
        },
        kind: KIND_DIRECTORY,
        name_length: name.len() as u32,
        name: [0; DIRECTORY_NAME_BYTES],
    };
    entry.name[..name.len()].copy_from_slice(name);
    Some(entry)
}

pub fn system_executable(path: &[u8]) -> Option<&'static [u8]> {
    path.starts_with(b"/usr/bin/")
        .then(|| system_file_by_path(path).map(|file| file.data))
        .flatten()
}

pub fn package_executable(path: &[u8]) -> Option<alloc::vec::Vec<u8>> {
    const MAX_EXECUTABLE_BYTES: usize = 32 * 1024 * 1024;
    if !path.starts_with(b"/usr/") {
        return None;
    }
    let file = with_state(|state| package_file_by_path(state, path).map(|(_, file)| file))?;
    let length = usize::try_from(file.size).ok()?;
    if length == 0 || length > MAX_EXECUTABLE_BYTES {
        return None;
    }
    let mut bytes = alloc::vec::Vec::new();
    bytes.try_reserve_exact(length).ok()?;
    bytes.resize(length, 0);
    (crate::fs::read_package_file(&file, 0, &mut bytes) == Some(length)).then_some(bytes)
}

pub(crate) fn read_only_backing_for_path(path: &[u8]) -> Option<(ReadOnlyFileBacking, u64)> {
    with_state(|state| {
        if !state.mounted {
            return None;
        }
        if let Some(file) = system_file_by_path(path) {
            return Some((
                ReadOnlyFileBacking::Embedded(file.data),
                file.data.len() as u64,
            ));
        }
        package_file_by_path(state, path)
            .map(|(_, file)| (ReadOnlyFileBacking::Package(file), file.size))
    })
}

pub(crate) fn read_only_backing(
    backing: ReadOnlyFileBacking,
    offset: u64,
    output: &mut [u8],
) -> Option<usize> {
    match backing {
        ReadOnlyFileBacking::Embedded(bytes) => {
            let offset = usize::try_from(offset).ok()?;
            let count = output.len().min(bytes.len().saturating_sub(offset));
            output[..count].copy_from_slice(bytes.get(offset..offset + count)?);
            Some(count)
        }
        ReadOnlyFileBacking::Package(file) => crate::fs::read_package_file(&file, offset, output),
    }
}

fn system_file_by_node(node: u8) -> Option<SystemFile> {
    SYSTEM_FILES.iter().copied().find(|file| file.node == node)
}

fn system_file_metadata(file: SystemFile) -> Metadata {
    metadata(
        file.mode,
        crate::security::ROOT_UID,
        0,
        KIND_FILE,
        file.data.len() as u64,
        0,
        file.inode,
    )
}

fn package_file_metadata(index: usize, file: MountedPackageFile) -> Metadata {
    metadata(
        0o100555,
        crate::security::ROOT_UID,
        0,
        KIND_FILE,
        file.size,
        0,
        0x20_0000 + index as u64,
    )
}

fn directory_node(state: &State, path: &[u8]) -> Option<u8> {
    match path {
        ROOT_PATH => return Some(NODE_DIRECTORY_ROOT),
        HOME_PATH => return Some(NODE_DIRECTORY_HOME),
        USER_DIRECTORY_PATH => return Some(NODE_DIRECTORY_USER),
        _ => {}
    }
    let name = parse_dynamic_path(path)?;
    let slot = dynamic_index_by_name(state, name)?;
    (state.dynamic[slot].kind == KIND_DIRECTORY).then_some(NODE_DYNAMIC_BASE + slot as u8)
}

fn working_directory_for(state: &State, owner_pid: u64) -> &[u8] {
    state
        .working_directories
        .iter()
        .find(|entry| entry.used && entry.owner_pid == owner_pid)
        .map_or(DEFAULT_WORKING_DIRECTORY, |entry| {
            &entry.path[..entry.length as usize]
        })
}

fn append_path_segments(source: &[u8], output: &mut [u8], length: &mut usize) -> Option<()> {
    for segment in source.split(|byte| *byte == b'/') {
        if segment.is_empty() || segment == b"." {
            continue;
        }
        if segment == b".." {
            if *length > 1 {
                while *length > 1 && output[*length - 1] != b'/' {
                    *length -= 1;
                }
                if *length > 1 {
                    *length -= 1;
                }
            }
            continue;
        }
        if segment.contains(&0) {
            return None;
        }
        let separator = usize::from(*length > 1);
        if *length + separator + segment.len() >= output.len() {
            return None;
        }
        if separator != 0 {
            output[*length] = b'/';
            *length += 1;
        }
        output[*length..*length + segment.len()].copy_from_slice(segment);
        *length += segment.len();
    }
    Some(())
}

fn resolve_path(state: &State, owner_pid: u64, input: &[u8], output: &mut [u8]) -> Option<usize> {
    if input.is_empty() || input.contains(&0) || output.len() < 2 {
        return None;
    }
    output.fill(0);
    output[0] = b'/';
    let mut length = 1usize;
    if input[0] != b'/' {
        append_path_segments(working_directory_for(state, owner_pid), output, &mut length)?;
    }
    append_path_segments(input, output, &mut length)?;
    Some(length)
}

fn parse_dynamic_path(path: &[u8]) -> Option<&[u8]> {
    let name = path.strip_prefix(USER_PREFIX)?;
    if name.is_empty()
        || name.len() > DYNAMIC_NAME_BYTES
        || name == USER_FILE_NAME
        || name == ACCOUNT_DB_NAME
    {
        return None;
    }
    for component in name.split(|byte| *byte == b'/') {
        if component.is_empty()
            || component.len() > DIRECTORY_NAME_BYTES
            || component == b"."
            || component == b".."
            || component
                .iter()
                .any(|byte| !byte.is_ascii_alphanumeric() && !matches!(*byte, b'.' | b'_' | b'-'))
        {
            return None;
        }
    }
    Some(name)
}

fn parent_directory_node(state: &State, name: &[u8]) -> Option<u8> {
    let Some(separator) = name.iter().rposition(|byte| *byte == b'/') else {
        return Some(NODE_DIRECTORY_USER);
    };
    let parent = &name[..separator];
    let slot = dynamic_index_by_name(state, parent)?;
    (state.dynamic[slot].kind == KIND_DIRECTORY).then_some(NODE_DYNAMIC_BASE + slot as u8)
}

fn dynamic_directory_has_children(state: &State, name: &[u8]) -> bool {
    state.dynamic.iter().any(|node| {
        node.used
            && node.name_length > name.len()
            && node.name[..name.len()] == *name
            && node.name[name.len()] == b'/'
    })
}

fn path_is_same_or_descendant(path: &[u8], ancestor: &[u8]) -> bool {
    path == ancestor
        || (path.len() > ancestor.len()
            && path.starts_with(ancestor)
            && path[ancestor.len()] == b'/')
}

fn system_hidden(file: &DynamicFile) -> bool {
    file.name_length == ACCOUNT_DB_NAME.len() && &file.name[..file.name_length] == ACCOUNT_DB_NAME
}

fn dynamic_child<'a>(
    state: &'a State,
    directory_node: u8,
    wanted: usize,
) -> Option<(usize, &'a [u8])> {
    let parent = if directory_node == NODE_DIRECTORY_USER {
        None
    } else {
        let slot = dynamic_index(directory_node)?;
        let node = state.dynamic.get(slot)?;
        if !node.used || node.kind != KIND_DIRECTORY {
            return None;
        }
        Some(&node.name[..node.name_length])
    };
    let mut found = 0usize;
    for (slot, node) in state.dynamic.iter().enumerate() {
        if !node.used || system_hidden(node) {
            continue;
        }
        let path = &node.name[..node.name_length];
        let child = match parent {
            None if !path.contains(&b'/') => path,
            Some(parent)
                if path.len() > parent.len()
                    && path.starts_with(parent)
                    && path[parent.len()] == b'/' =>
            {
                let remainder = &path[parent.len() + 1..];
                if remainder.contains(&b'/') {
                    continue;
                }
                remainder
            }
            _ => continue,
        };
        if found == wanted {
            return Some((slot, child));
        }
        found += 1;
    }
    None
}

fn dynamic_child_count(state: &State, directory_node: u8) -> usize {
    let mut count = 0usize;
    while dynamic_child(state, directory_node, count).is_some() {
        count += 1;
    }
    count
}

fn dynamic_index_by_name(state: &State, name: &[u8]) -> Option<usize> {
    state.dynamic.iter().position(|file| {
        file.used && file.name_length == name.len() && &file.name[..file.name_length] == name
    })
}

fn dynamic_index(node: u8) -> Option<usize> {
    let index = node.checked_sub(NODE_DYNAMIC_BASE)? as usize;
    (index < DYNAMIC_FILE_COUNT).then_some(index)
}

fn user_file_access(write: bool) -> bool {
    user_node_access(KIND_FILE, write)
}

fn user_node_access(kind: u32, write: bool) -> bool {
    crate::security::file_access(
        if kind == KIND_DIRECTORY {
            0o040700
        } else {
            0o100600
        },
        crate::security::INIT_UID,
        crate::security::INIT_GID,
        write,
    )
}

#[cfg(target_arch = "x86_64")]
fn current_pid() -> u64 {
    crate::scheduler::current_pid()
}

#[cfg(target_arch = "aarch64")]
fn current_pid() -> u64 {
    crate::aarch64_process::current_pid()
}

fn metadata(
    mode: u32,
    uid: u32,
    gid: u32,
    kind: u32,
    size: u64,
    modified_ticks: u64,
    inode: u64,
) -> Metadata {
    Metadata {
        mode,
        uid,
        gid,
        kind,
        size,
        modified_ticks,
        inode,
    }
}

fn file_metadata(state: &State, node: u8) -> Option<Metadata> {
    match dynamic_index(node) {
        Some(slot) => {
            (state.dynamic[slot].used && state.dynamic[slot].kind == KIND_FILE).then(|| {
                metadata(
                    0o100600,
                    crate::security::INIT_UID,
                    crate::security::INIT_GID,
                    KIND_FILE,
                    state.dynamic[slot].data_length as u64,
                    state.dynamic[slot].modified_ticks,
                    6 + slot as u64,
                )
            })
        }
        None if node == NODE_BOOT => Some(metadata(
            0o100644,
            0,
            0,
            KIND_FILE,
            state.boot_length as u64,
            state.boot_modified_ticks,
            4,
        )),
        None if node == NODE_USER => Some(metadata(
            0o100600,
            crate::security::INIT_UID,
            crate::security::INIT_GID,
            KIND_FILE,
            state.user_length as u64,
            state.user_modified_ticks,
            5,
        )),
        None => None,
    }
}

fn directory_metadata(state: &State, node: u8) -> Option<Metadata> {
    match node {
        NODE_DIRECTORY_ROOT => Some(metadata(0o040755, 0, 0, KIND_DIRECTORY, 2, 1, 1)),
        NODE_DIRECTORY_HOME => Some(metadata(0o040755, 0, 0, KIND_DIRECTORY, 1, 1, 2)),
        NODE_DIRECTORY_USER => Some(metadata(
            0o040700,
            crate::security::INIT_UID,
            crate::security::INIT_GID,
            KIND_DIRECTORY,
            1 + dynamic_child_count(state, NODE_DIRECTORY_USER) as u64
                + makfs4_child_count(1).unwrap_or(0) as u64,
            state.user_modified_ticks,
            3,
        )),
        _ => {
            let slot = dynamic_index(node)?;
            let directory = &state.dynamic[slot];
            (directory.used && directory.kind == KIND_DIRECTORY).then(|| {
                metadata(
                    0o040700,
                    crate::security::INIT_UID,
                    crate::security::INIT_GID,
                    KIND_DIRECTORY,
                    dynamic_child_count(state, node) as u64,
                    directory.modified_ticks,
                    6 + slot as u64,
                )
            })
        }
    }
}

fn directory_entry(state: &State, node: u8, index: usize) -> Option<DirectoryEntry> {
    let (inode, kind, name): (u64, u32, &[u8]) = match (node, index) {
        (NODE_DIRECTORY_ROOT, 0) => (4, KIND_FILE, b"boot-count.txt"),
        (NODE_DIRECTORY_ROOT, 1) => (2, KIND_DIRECTORY, b"home"),
        (NODE_DIRECTORY_HOME, 0) => (3, KIND_DIRECTORY, b"user"),
        (NODE_DIRECTORY_USER, 0) => (5, KIND_FILE, USER_FILE_NAME),
        (NODE_DIRECTORY_USER, dynamic_entry) => {
            let (slot, child) = dynamic_child(state, node, dynamic_entry - 1)?;
            (6 + slot as u64, state.dynamic[slot].kind, child)
        }
        (_, dynamic_entry) => {
            let (slot, child) = dynamic_child(state, node, dynamic_entry)?;
            (6 + slot as u64, state.dynamic[slot].kind, child)
        }
    };
    let mut entry = DirectoryEntry {
        inode,
        kind,
        name_length: name.len() as u32,
        name: [0; DIRECTORY_NAME_BYTES],
    };
    entry.name[..name.len()].copy_from_slice(name);
    Some(entry)
}

fn makfs4_directory_entry(inode: makos_makfs4::Inode) -> Option<DirectoryEntry> {
    let name = inode.name();
    if name.len() > DIRECTORY_NAME_BYTES {
        return None;
    }
    let kind = match inode.mode & 0o170000 {
        0o040000 => KIND_DIRECTORY,
        0o100000 => KIND_FILE,
        0o120000 => KIND_SYMLINK,
        _ => return None,
    };
    let mut entry = DirectoryEntry {
        inode: 0x40_0000 + inode.inode,
        kind,
        name_length: name.len() as u32,
        name: [0; DIRECTORY_NAME_BYTES],
    };
    entry.name[..name.len()].copy_from_slice(name);
    Some(entry)
}

fn makfs4_directory_fd_entry(
    description: FileDescription,
    index: usize,
) -> Result<Option<(DirectoryEntry, usize)>, crate::makfs4_volume::MountError> {
    let directory = crate::makfs4_volume::read_inode(u32::from(description.makfs4_inode))?
        .ok_or(crate::makfs4_volume::MountError::Geometry)?;
    if directory.mode & 0o170000 != 0o040000 {
        return Err(crate::makfs4_volume::MountError::Geometry);
    }
    if index < 2 {
        let inode = if index == 0 {
            directory.inode
        } else {
            directory.parent
        };
        let name: &[u8] = if index == 0 { b"." } else { b".." };
        let mut entry = DirectoryEntry {
            inode: 0x40_0000 + inode,
            kind: KIND_DIRECTORY,
            name_length: name.len() as u32,
            name: [0; DIRECTORY_NAME_BYTES],
        };
        entry.name[..name.len()].copy_from_slice(name);
        return Ok(Some((entry, index + 1)));
    }
    Ok(
        crate::makfs4_volume::child_from(directory.inode, (index - 2) as u32)?.and_then(
            |(inode_index, inode)| {
                makfs4_directory_entry(inode).map(|entry| (entry, inode_index as usize + 3))
            },
        ),
    )
}

fn makfs4_directory_length(description: FileDescription) -> Option<u64> {
    let directory = makfs4_description_inode(description)?;
    if directory.mode & 0o170000 != 0o040000 {
        return None;
    }
    crate::makfs4_volume::child_count(directory.inode)
        .ok()
        .and_then(|count| (count as u64).checked_add(2))
}

fn directory_fd_entry(state: &State, node: u8, index: usize) -> Option<DirectoryEntry> {
    if index < 2 {
        let inode = if index == 0 {
            directory_metadata(state, node)?.inode
        } else {
            match node {
                NODE_DIRECTORY_ROOT => 1,
                NODE_DIRECTORY_HOME => 1,
                NODE_DIRECTORY_USER => 2,
                _ => {
                    let slot = dynamic_index(node)?;
                    let name = &state.dynamic[slot].name[..state.dynamic[slot].name_length];
                    directory_metadata(state, parent_directory_node(state, name)?)?.inode
                }
            }
        };
        let name: &[u8] = if index == 0 { b"." } else { b".." };
        let mut entry = DirectoryEntry {
            inode,
            kind: KIND_DIRECTORY,
            name_length: name.len() as u32,
            name: [0; DIRECTORY_NAME_BYTES],
        };
        entry.name[..name.len()].copy_from_slice(name);
        return Some(entry);
    }
    if node == NODE_DIRECTORY_USER {
        let legacy_index = index - 2;
        let legacy_count = 1 + dynamic_child_count(state, node);
        if legacy_index < legacy_count {
            return directory_entry(state, node, legacy_index);
        }
        return crate::makfs4_volume::child_at(1, legacy_index - legacy_count)
            .ok()
            .flatten()
            .and_then(|(_, inode)| makfs4_directory_entry(inode));
    }
    directory_entry(state, node, index - 2)
}

fn makfs4_child_count(parent: u64) -> Option<usize> {
    crate::makfs4_volume::child_count(parent).ok()
}

fn directory_length(state: &State, node: u8) -> Option<usize> {
    match node {
        NODE_DIRECTORY_ROOT => Some(4),
        NODE_DIRECTORY_HOME => Some(3),
        NODE_DIRECTORY_USER => {
            Some(3 + dynamic_child_count(state, node) + makfs4_child_count(1).unwrap_or(0))
        }
        _ => directory_metadata(state, node).map(|_| 2 + dynamic_child_count(state, node)),
    }
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
