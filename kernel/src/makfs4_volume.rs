use crate::block::DataDisk;
use alloc::vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use makos_makfs4::{
    BLOCK_BYTES, BlockBitmap, Catalog, CommitPhase, CommitSequencer, DirectoryIndex,
    Error as FormatError, ExtentMap, Inode, RECORD_BYTES, Superblock, newest_superblock,
};

const SECTOR_BYTES: u64 = 512;
const SECTORS_PER_BLOCK: u64 = BLOCK_BYTES / SECTOR_BYTES;
const SUPERBLOCK_A_LBA: u32 = 112;
const SUPERBLOCK_B_LBA: u32 = 120;
const METADATA_FIRST_BLOCK: u64 = 16;
const PACKAGE_HEADER_LBA: u32 = 2048;
const PACKAGE_MAGIC: [u8; 8] = *b"MAKPKG01";
const PACKAGE_LIMIT_BLOCK: u64 = 131_072; // 512 MiB
const PACKAGE_TRANSACTION_BASE_LBA: u64 = makos_package_store::PRODUCTION_BASE_SECTOR;
const MIN_VOLUME_BLOCKS: u64 = 262_144; // 1 GiB
const MAX_VOLUME_BLOCKS: u64 = MIN_VOLUME_BLOCKS; // 1 GiB initial volume
const MAXIMUM_INODES: u32 = 512;
const DIRECTORY_INDEX_BUCKETS: usize = 1024;
const INODE_TABLE_BLOCKS: u32 = 64;
const INODE_DIRTY_WORDS: usize = MAXIMUM_INODES as usize / 64;
const METADATA_LIMIT_BLOCK: u64 = 256; // Package header starts here.

#[derive(Clone, Copy)]
struct MetadataSet {
    bitmap_block: u64,
    catalog_block: u64,
    inode_table_block: u64,
}

static ACTIVE_GENERATION: AtomicU64 = AtomicU64::new(0);
static ACTIVE_CATALOG_BLOCK: AtomicU64 = AtomicU64::new(0);
static ACTIVE_BITMAP_BLOCK: AtomicU64 = AtomicU64::new(0);
static ACTIVE_BLOCK_COUNT: AtomicU64 = AtomicU64::new(0);
static MUTATION_LOCK: AtomicBool = AtomicBool::new(false);

struct InodeCache {
    ready: bool,
    generation: u64,
    inode_table_block: u64,
    inode_count: u32,
    free_hint: u32,
    entries: [Option<Inode>; MAXIMUM_INODES as usize],
    child_index: DirectoryIndex<{ MAXIMUM_INODES as usize }, DIRECTORY_INDEX_BUCKETS>,
    metadata_sets: [MetadataSet; 3],
    inode_dirty: [[u64; INODE_DIRTY_WORDS]; 3],
    bitmap_dirty: [u64; 3],
}

impl InodeCache {
    const EMPTY: Self = Self {
        ready: false,
        generation: 0,
        inode_table_block: 0,
        inode_count: 0,
        free_hint: 1,
        entries: [None; MAXIMUM_INODES as usize],
        child_index: DirectoryIndex::EMPTY,
        metadata_sets: [MetadataSet {
            bitmap_block: 0,
            catalog_block: 0,
            inode_table_block: 0,
        }; 3],
        inode_dirty: [[u64::MAX; INODE_DIRTY_WORDS]; 3],
        bitmap_dirty: [u64::MAX; 3],
    };
}

struct LockedInodeCache {
    lock: AtomicBool,
    value: core::cell::UnsafeCell<InodeCache>,
}

unsafe impl Sync for LockedInodeCache {}

static INODE_CACHE: LockedInodeCache = LockedInodeCache {
    lock: AtomicBool::new(false),
    value: core::cell::UnsafeCell::new(InodeCache::EMPTY),
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MountError {
    Io,
    Corrupt,
    PackageOverlap,
    Geometry,
    NoSpace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MountResult {
    DeferredSmallDisk,
    Formatted,
    Mounted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VolumeStats {
    pub block_size: u64,
    pub total_blocks: u64,
    pub free_blocks: u64,
    pub total_inodes: u64,
    pub free_inodes: u64,
}

pub fn mount_or_format(disk: &mut DataDisk) -> Result<MountResult, MountError> {
    let disk_blocks = disk.sectors() / SECTORS_PER_BLOCK;
    if disk_blocks < MIN_VOLUME_BLOCKS {
        return Ok(MountResult::DeferredSmallDisk);
    }
    reject_package_overlap(disk)?;
    let block_count = disk_blocks.min(MAX_VOLUME_BLOCKS);
    let first_record = read_record(disk, SUPERBLOCK_A_LBA)?;
    let second_record = read_record(disk, SUPERBLOCK_B_LBA)?;
    let both_blank =
        first_record.iter().all(|byte| *byte == 0) && second_record.iter().all(|byte| *byte == 0);
    let (superblock, result) = if both_blank {
        (format(disk, block_count)?, MountResult::Formatted)
    } else {
        let first = validate_root(disk, &first_record, block_count);
        let second = validate_root(disk, &second_record, block_count);
        let (superblock, _) = newest_superblock(first, second).map_err(|_| MountError::Corrupt)?;
        (superblock, MountResult::Mounted)
    };
    load_inode_cache(disk, superblock)?;
    activate(superblock);
    let root_migrated = normalize_user_root_permissions()?;
    let generation = ACTIVE_GENERATION.load(Ordering::Acquire);
    crate::serial_println!(
        "MAKOS_MAKFS4_READY state={:?} generation={} block_bytes={} volume_blocks={} data_start={} max_inodes={} extents=14 cow=inode,bitmap,catalog root=redundant flush=metadata,root",
        result,
        generation,
        BLOCK_BYTES,
        block_count,
        PACKAGE_LIMIT_BLOCK,
        MAXIMUM_INODES,
    );
    crate::serial_println!(
        "MAKOS_MAKFS4_HOME_ROOT_OK inode=1 mode=0700 uid={} gid={} migrated={}",
        crate::security::INIT_UID,
        crate::security::INIT_GID,
        u8::from(root_migrated),
    );
    Ok(result)
}

fn normalize_user_root_permissions() -> Result<bool, MountError> {
    let _guard = MutationGuard::acquire();
    let mut disk = DataDisk::identify_secondary().ok_or(MountError::Io)?;
    let active = active_superblock()?;
    let catalog = active_catalog(&mut disk, active)?;
    let mut root = read_inode_from(&mut disk, catalog, 0)?.ok_or(MountError::Geometry)?;
    if root.inode != 1
        || root.parent != 1
        || root.name() != b"."
        || root.mode & 0o170000 != 0o040000
    {
        return Err(MountError::Geometry);
    }
    let desired_mode = 0o040700;
    if root.mode == desired_mode
        && root.uid == crate::security::INIT_UID
        && root.gid == crate::security::INIT_GID
    {
        return Ok(false);
    }
    root.generation = active
        .generation
        .checked_add(1)
        .ok_or(MountError::Geometry)?;
    root.mode = desired_mode;
    root.uid = crate::security::INIT_UID;
    root.gid = crate::security::INIT_GID;
    root.changed_seconds = timestamp_seconds(current_unix_seconds());
    let record = root.encode().map_err(|_| MountError::Corrupt)?;
    commit_inode_change(&mut disk, active, catalog, 0, &record, 0, None)?;
    Ok(true)
}

pub fn mounted() -> bool {
    ACTIVE_GENERATION.load(Ordering::Acquire) != 0
}

pub fn volume_stats() -> Result<VolumeStats, MountError> {
    let mut disk = DataDisk::identify_secondary().ok_or(MountError::Io)?;
    let active = active_superblock()?;
    let catalog = active_catalog(&mut disk, active)?;
    let words = read_bitmap_words(&mut disk, active)?;
    let total_blocks = active.block_count - active.data_start;
    let full_words = usize::try_from(total_blocks / 64).map_err(|_| MountError::Geometry)?;
    let trailing = (total_blocks % 64) as u32;
    let mut allocated = words[..full_words]
        .iter()
        .map(|word| u64::from(word.count_ones()))
        .sum::<u64>();
    if trailing != 0 {
        let mask = (1u64 << trailing) - 1;
        allocated += u64::from((words[full_words] & mask).count_ones());
    }
    Ok(VolumeStats {
        block_size: BLOCK_BYTES,
        total_blocks,
        free_blocks: total_blocks.saturating_sub(allocated),
        total_inodes: u64::from(catalog.maximum_inodes),
        free_inodes: u64::from(catalog.maximum_inodes.saturating_sub(catalog.inode_count)),
    })
}

/// Largest single sparse-free file this initial volume geometry can hold.
/// Actual writes may return `NoSpace` earlier when other files own blocks.
pub fn maximum_file_bytes() -> u64 {
    ACTIVE_BLOCK_COUNT
        .load(Ordering::Acquire)
        .saturating_sub(PACKAGE_LIMIT_BLOCK)
        .saturating_mul(BLOCK_BYTES)
}

pub const fn directory_cursor_limit() -> u64 {
    MAXIMUM_INODES as u64 + 2
}

pub fn read_inode(index: u32) -> Result<Option<Inode>, MountError> {
    if !mounted() || index >= MAXIMUM_INODES {
        return Ok(None);
    }
    let mut disk = DataDisk::identify_secondary().ok_or(MountError::Io)?;
    let superblock = active_superblock()?;
    let catalog = active_catalog(&mut disk, superblock)?;
    read_inode_from(&mut disk, catalog, index)
}

pub fn read_file_block(inode: &Inode, file_block: u64, output: &mut [u8; 4096]) -> bool {
    let Some(device_block) = inode.extents.file_block_to_device(file_block) else {
        output.fill(0);
        return true;
    };
    DataDisk::identify_secondary()
        .is_some_and(|mut disk| read_block(&mut disk, device_block, output))
}

pub fn create_inode(
    parent: u64,
    name: &[u8],
    mode: u32,
    uid: u32,
    gid: u32,
) -> Result<u32, MountError> {
    create_inode_with_data(parent, name, mode, uid, gid, &[])
}

/// Create one persistent symbolic link. Target bytes are written before the
/// inode/catalog/root commit, so recovery exposes no partial link target.
pub fn create_symlink_inode(
    parent: u64,
    name: &[u8],
    target: &[u8],
    uid: u32,
    gid: u32,
) -> Result<u32, MountError> {
    if target.is_empty() || target.len() >= crate::vfs::MAX_PATH_BYTES || target.contains(&0) {
        return Err(MountError::Geometry);
    }
    create_inode_with_data(parent, name, 0o120777, uid, gid, target)
}

fn create_inode_with_data(
    parent: u64,
    name: &[u8],
    mode: u32,
    uid: u32,
    gid: u32,
    data: &[u8],
) -> Result<u32, MountError> {
    let _guard = MutationGuard::acquire();
    if !mounted()
        || !matches!(mode & 0o170000, 0o040000 | 0o100000 | 0o120000)
        || (mode & 0o170000 != 0o120000 && !data.is_empty())
    {
        return Err(MountError::Geometry);
    }
    let mut disk = DataDisk::identify_secondary().ok_or(MountError::Io)?;
    let active = active_superblock()?;
    let catalog = active_catalog(&mut disk, active)?;
    let parent_index = u32::try_from(parent.checked_sub(1).ok_or(MountError::Geometry)?)
        .map_err(|_| MountError::Geometry)?;
    let parent_inode =
        read_inode_from(&mut disk, catalog, parent_index)?.ok_or(MountError::Geometry)?;
    if parent_inode.mode & 0o170000 != 0o040000 {
        return Err(MountError::Geometry);
    }

    let mut free = cached_free_inode(catalog);
    if let Some(existing) = cached_child_index(catalog, parent, name) {
        if existing.is_some() {
            return Err(MountError::Geometry);
        }
    } else {
        for index in 1..MAXIMUM_INODES {
            match read_inode_from(&mut disk, catalog, index)? {
                Some(inode) if inode.parent == parent && inode.name() == name => {
                    return Err(MountError::Geometry);
                }
                Some(_) => {}
                None if free.is_none() => free = Some(index),
                None => {}
            }
        }
    }
    let index = free.ok_or(MountError::Geometry)?;
    let generation = active
        .generation
        .checked_add(1)
        .ok_or(MountError::Geometry)?;
    let mut inode = Inode::EMPTY;
    inode.inode = u64::from(index) + 1;
    inode.generation = generation;
    inode.mode = mode;
    inode.uid = uid;
    inode.gid = gid;
    let now = current_unix_seconds();
    inode.modified_ns = now.saturating_mul(1_000_000_000);
    inode.accessed_seconds = timestamp_seconds(now);
    inode.changed_seconds = timestamp_seconds(now);
    inode.parent = parent;
    inode.set_name(name).map_err(|_| MountError::Geometry)?;
    let mut bitmap_words = read_bitmap_words(&mut disk, active)?;
    if !data.is_empty() {
        let mut bitmap = BlockBitmap::new_at(
            &mut bitmap_words,
            active.data_start,
            active.block_count - active.data_start,
        )
        .map_err(format_error)?;
        inode.extents =
            allocate_extent_map(&mut bitmap, (data.len() as u64).div_ceil(BLOCK_BYTES))?;
        drop(bitmap);
        inode.size = data.len() as u64;
        for file_block in 0..inode.extents.blocks() {
            let mut block = [0u8; 4096];
            let start = usize::try_from(file_block.saturating_mul(BLOCK_BYTES))
                .map_err(|_| MountError::Geometry)?;
            let end = data.len().min(start.saturating_add(BLOCK_BYTES as usize));
            block[..end - start].copy_from_slice(&data[start..end]);
            let device_block = inode
                .extents
                .file_block_to_device(file_block)
                .ok_or(MountError::Corrupt)?;
            if !write_block(&mut disk, device_block, &block) {
                return Err(MountError::Io);
            }
        }
    }
    let record = inode.encode().map_err(|_| MountError::Corrupt)?;
    commit_inode_change(
        &mut disk,
        active,
        catalog,
        index,
        &record,
        1,
        (!data.is_empty()).then_some(&bitmap_words),
    )?;
    Ok(index)
}

pub fn find_child(parent: u64, name: &[u8]) -> Result<Option<u32>, MountError> {
    let mut disk = DataDisk::identify_secondary().ok_or(MountError::Io)?;
    let active = active_superblock()?;
    let catalog = active_catalog(&mut disk, active)?;
    if let Some(index) = cached_child_index(catalog, parent, name) {
        return Ok(index);
    }
    for index in 1..MAXIMUM_INODES {
        if let Some(inode) = read_inode_from(&mut disk, catalog, index)?
            && inode.parent == parent
            && inode.name() == name
        {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

pub fn child_at(parent: u64, ordinal: usize) -> Result<Option<(u32, Inode)>, MountError> {
    let mut disk = DataDisk::identify_secondary().ok_or(MountError::Io)?;
    let active = active_superblock()?;
    let catalog = active_catalog(&mut disk, active)?;
    if let Some(child) = cached_child_at(catalog, parent, ordinal, 1) {
        return Ok(child);
    }
    let mut current = 0;
    for index in 1..MAXIMUM_INODES {
        if let Some(inode) = read_inode_from(&mut disk, catalog, index)?
            && inode.parent == parent
        {
            if current == ordinal {
                return Ok(Some((index, inode)));
            }
            current += 1;
        }
    }
    Ok(None)
}

/// Resume a directory scan at raw inode-table index. Returned index lets VFS
/// advance without rescanning earlier inodes for every `getdents64` record.
pub fn child_from(parent: u64, start: u32) -> Result<Option<(u32, Inode)>, MountError> {
    let mut disk = DataDisk::identify_secondary().ok_or(MountError::Io)?;
    let active = active_superblock()?;
    let catalog = active_catalog(&mut disk, active)?;
    if let Some(child) = cached_child_at(catalog, parent, 0, start.max(1)) {
        return Ok(child);
    }
    for index in start.max(1)..MAXIMUM_INODES {
        if let Some(inode) = read_inode_from(&mut disk, catalog, index)?
            && inode.parent == parent
        {
            return Ok(Some((index, inode)));
        }
    }
    Ok(None)
}

pub fn child_count(parent: u64) -> Result<usize, MountError> {
    let mut disk = DataDisk::identify_secondary().ok_or(MountError::Io)?;
    let active = active_superblock()?;
    let catalog = active_catalog(&mut disk, active)?;
    if let Some(count) = cached_child_count(catalog, parent) {
        return Ok(count);
    }
    let mut count = 0usize;
    for index in 1..MAXIMUM_INODES {
        if read_inode_from(&mut disk, catalog, index)?.is_some_and(|inode| inode.parent == parent) {
            count += 1;
        }
    }
    Ok(count)
}

pub fn rename_inode(index: u32, parent: u64, name: &[u8]) -> Result<(), MountError> {
    let _guard = MutationGuard::acquire();
    if index == 0 {
        return Err(MountError::Geometry);
    }
    let mut disk = DataDisk::identify_secondary().ok_or(MountError::Io)?;
    let active = active_superblock()?;
    let catalog = active_catalog(&mut disk, active)?;
    let parent_index = u32::try_from(parent.checked_sub(1).ok_or(MountError::Geometry)?)
        .map_err(|_| MountError::Geometry)?;
    let parent_inode =
        read_inode_from(&mut disk, catalog, parent_index)?.ok_or(MountError::Geometry)?;
    if parent_inode.mode & 0o170000 != 0o040000 {
        return Err(MountError::Geometry);
    }
    let mut inode = read_inode_from(&mut disk, catalog, index)?.ok_or(MountError::Geometry)?;
    if inode.mode & 0o170000 == 0o040000 {
        let mut ancestor = parent;
        for _ in 0..MAXIMUM_INODES {
            if ancestor == inode.inode {
                return Err(MountError::Geometry);
            }
            if ancestor == 1 {
                break;
            }
            let ancestor_index =
                u32::try_from(ancestor.checked_sub(1).ok_or(MountError::Geometry)?)
                    .map_err(|_| MountError::Geometry)?;
            ancestor = read_inode_from(&mut disk, catalog, ancestor_index)?
                .ok_or(MountError::Geometry)?
                .parent;
        }
        if ancestor != 1 {
            return Err(MountError::Corrupt);
        }
    }
    for candidate in 1..MAXIMUM_INODES {
        if candidate != index
            && let Some(existing) = read_inode_from(&mut disk, catalog, candidate)?
            && existing.parent == parent
            && existing.name() == name
        {
            return Err(MountError::Geometry);
        }
    }
    inode.parent = parent;
    inode.generation = active
        .generation
        .checked_add(1)
        .ok_or(MountError::Geometry)?;
    inode.changed_seconds = timestamp_seconds(current_unix_seconds());
    inode.set_name(name).map_err(|_| MountError::Geometry)?;
    let record = inode.encode().map_err(|_| MountError::Corrupt)?;
    commit_inode_change(&mut disk, active, catalog, index, &record, 0, None)
}

/// Atomically renames `index` over an existing non-directory inode. Both inode
/// records, released destination extents, catalog count, and root generation
/// become visible through one MakFS4 commit.
pub fn replace_inode(
    index: u32,
    destination_index: u32,
    parent: u64,
    name: &[u8],
) -> Result<(), MountError> {
    let _guard = MutationGuard::acquire();
    if index == 0 || destination_index == 0 || index == destination_index {
        return Err(MountError::Geometry);
    }
    let mut disk = DataDisk::identify_secondary().ok_or(MountError::Io)?;
    let active = active_superblock()?;
    let catalog = active_catalog(&mut disk, active)?;
    let parent_index = u32::try_from(parent.checked_sub(1).ok_or(MountError::Geometry)?)
        .map_err(|_| MountError::Geometry)?;
    let parent_inode =
        read_inode_from(&mut disk, catalog, parent_index)?.ok_or(MountError::Geometry)?;
    let mut source = read_inode_from(&mut disk, catalog, index)?.ok_or(MountError::Geometry)?;
    let destination =
        read_inode_from(&mut disk, catalog, destination_index)?.ok_or(MountError::Geometry)?;
    if parent_inode.mode & 0o170000 != 0o040000
        || source.mode & 0o170000 == 0o040000
        || destination.mode & 0o170000 == 0o040000
        || destination.parent != parent
        || destination.name() != name
    {
        return Err(MountError::Geometry);
    }
    source.parent = parent;
    source.generation = active
        .generation
        .checked_add(1)
        .ok_or(MountError::Geometry)?;
    source.changed_seconds = timestamp_seconds(current_unix_seconds());
    source.set_name(name).map_err(|_| MountError::Geometry)?;
    let source_record = source.encode().map_err(|_| MountError::Corrupt)?;
    let empty_record = [0u8; RECORD_BYTES];
    let mut bitmap_words = read_bitmap_words(&mut disk, active)?;
    let mut bitmap = BlockBitmap::new_at(
        &mut bitmap_words,
        active.data_start,
        active.block_count - active.data_start,
    )
    .map_err(format_error)?;
    for extent in destination.extents.extents() {
        bitmap.release(*extent).map_err(format_error)?;
    }
    drop(bitmap);
    commit_inode_changes(
        &mut disk,
        active,
        catalog,
        index,
        &source_record,
        Some((destination_index, &empty_record)),
        -1,
        Some(&bitmap_words),
    )
}

pub fn remove_inode(index: u32) -> Result<(), MountError> {
    let _guard = MutationGuard::acquire();
    if index == 0 {
        return Err(MountError::Geometry);
    }
    let mut disk = DataDisk::identify_secondary().ok_or(MountError::Io)?;
    let active = active_superblock()?;
    let catalog = active_catalog(&mut disk, active)?;
    let inode = read_inode_from(&mut disk, catalog, index)?.ok_or(MountError::Geometry)?;
    if inode.mode & 0o170000 == 0o040000 {
        for candidate in 1..MAXIMUM_INODES {
            if let Some(child) = read_inode_from(&mut disk, catalog, candidate)?
                && child.parent == inode.inode
            {
                return Err(MountError::Geometry);
            }
        }
    }
    let mut bitmap_words = read_bitmap_words(&mut disk, active)?;
    let mut bitmap = BlockBitmap::new_at(
        &mut bitmap_words,
        active.data_start,
        active.block_count - active.data_start,
    )
    .map_err(format_error)?;
    for extent in inode.extents.extents() {
        bitmap.release(*extent).map_err(format_error)?;
    }
    drop(bitmap);
    commit_inode_change(
        &mut disk,
        active,
        catalog,
        index,
        &[0u8; RECORD_BYTES],
        -1,
        Some(&bitmap_words),
    )
}

pub fn read_inode_at(index: u32, offset: u64, output: &mut [u8]) -> Result<usize, MountError> {
    let mut disk = DataDisk::identify_secondary().ok_or(MountError::Io)?;
    let active = active_superblock()?;
    let catalog = active_catalog(&mut disk, active)?;
    let inode = read_inode_from(&mut disk, catalog, index)?.ok_or(MountError::Geometry)?;
    if !matches!(inode.mode & 0o170000, 0o100000 | 0o120000) {
        return Err(MountError::Geometry);
    }
    inode
        .validate_on_volume(active.data_start, active.block_count)
        .map_err(|_| MountError::Corrupt)?;
    if offset >= inode.size || output.is_empty() {
        return Ok(0);
    }
    let count = output
        .len()
        .min(usize::try_from(inode.size - offset).unwrap_or(usize::MAX));
    let mut copied = 0;
    while copied < count {
        let absolute = offset + copied as u64;
        let file_block = absolute / BLOCK_BYTES;
        let in_block = usize::try_from(absolute % BLOCK_BYTES).map_err(|_| MountError::Geometry)?;
        let amount = (count - copied).min(BLOCK_BYTES as usize - in_block);
        let mut block = [0u8; 4096];
        read_inode_block(&mut disk, &inode, file_block, &mut block)?;
        output[copied..copied + amount].copy_from_slice(&block[in_block..in_block + amount]);
        copied += amount;
    }
    drop(disk);
    let _ = touch_accessed(index, inode);
    Ok(count)
}

pub fn write_inode_at(index: u32, offset: u64, input: &[u8]) -> Result<usize, MountError> {
    if input.is_empty() {
        return Ok(0);
    }
    rewrite_inode(index, None, offset, input)?;
    Ok(input.len())
}

pub fn truncate_inode(index: u32, size: u64) -> Result<(), MountError> {
    rewrite_inode(index, Some(size), 0, &[])
}

fn rewrite_inode(
    index: u32,
    requested_size: Option<u64>,
    patch_offset: u64,
    patch: &[u8],
) -> Result<(), MountError> {
    let _guard = MutationGuard::acquire();
    let mut disk = DataDisk::identify_secondary().ok_or(MountError::Io)?;
    let active = active_superblock()?;
    let catalog = active_catalog(&mut disk, active)?;
    let mut inode = read_inode_from(&mut disk, catalog, index)?.ok_or(MountError::Geometry)?;
    if inode.mode & 0o170000 != 0o100000 {
        return Err(MountError::Geometry);
    }
    inode
        .validate_on_volume(active.data_start, active.block_count)
        .map_err(|_| MountError::Corrupt)?;
    let patch_end = patch_offset
        .checked_add(patch.len() as u64)
        .ok_or(MountError::Geometry)?;
    let new_size = requested_size.unwrap_or_else(|| inode.size.max(patch_end));
    if patch_end > new_size {
        return Err(MountError::Geometry);
    }
    if patch.is_empty() && requested_size == Some(inode.size) {
        return Ok(());
    }
    let needed_blocks = new_size.div_ceil(BLOCK_BYTES);
    if needed_blocks > active.block_count - active.data_start {
        return Err(MountError::NoSpace);
    }
    let mut bitmap_words = read_bitmap_words(&mut disk, active)?;
    let mut bitmap = BlockBitmap::new_at(
        &mut bitmap_words,
        active.data_start,
        active.block_count - active.data_start,
    )
    .map_err(format_error)?;
    let next_extents = allocate_extent_map(&mut bitmap, needed_blocks)?;

    for file_block in 0..needed_blocks {
        let mut block = [0u8; 4096];
        if file_block * BLOCK_BYTES < inode.size {
            read_inode_block(&mut disk, &inode, file_block, &mut block)?;
        }
        let block_start = file_block * BLOCK_BYTES;
        let block_end = block_start + BLOCK_BYTES;
        let copy_start = block_start.max(patch_offset);
        let copy_end = block_end.min(patch_end);
        if copy_start < copy_end {
            let source =
                usize::try_from(copy_start - patch_offset).map_err(|_| MountError::Geometry)?;
            let destination =
                usize::try_from(copy_start - block_start).map_err(|_| MountError::Geometry)?;
            let amount =
                usize::try_from(copy_end - copy_start).map_err(|_| MountError::Geometry)?;
            block[destination..destination + amount]
                .copy_from_slice(&patch[source..source + amount]);
        }
        if block_end > new_size {
            let valid = usize::try_from(new_size.saturating_sub(block_start))
                .map_err(|_| MountError::Geometry)?;
            block[valid..].fill(0);
        }
        let device_block = next_extents
            .file_block_to_device(file_block)
            .ok_or(MountError::Corrupt)?;
        if !write_block(&mut disk, device_block, &block) {
            return Err(MountError::Io);
        }
    }
    for extent in inode.extents.extents() {
        bitmap.release(*extent).map_err(format_error)?;
    }
    drop(bitmap);

    inode.generation = active
        .generation
        .checked_add(1)
        .ok_or(MountError::Geometry)?;
    inode.size = new_size;
    let now = current_unix_seconds();
    inode.modified_ns = now.saturating_mul(1_000_000_000);
    inode.changed_seconds = timestamp_seconds(now);
    inode.extents = next_extents;
    let record = inode.encode().map_err(|_| MountError::Corrupt)?;
    commit_inode_change(
        &mut disk,
        active,
        catalog,
        index,
        &record,
        0,
        Some(&bitmap_words),
    )
}

/// Persist `relatime` atime without imposing write amplification on unchanged
/// files. Legacy v4 records keep zero extension fields until next mutation.
fn touch_accessed(index: u32, previous: Inode) -> Result<(), MountError> {
    if previous.accessed_seconds == 0 {
        return Ok(());
    }
    let now = timestamp_seconds(current_unix_seconds());
    let modified = timestamp_seconds(previous.modified_ns / 1_000_000_000);
    let due = previous.accessed_seconds < modified
        || previous.accessed_seconds < previous.changed_seconds
        || now.saturating_sub(previous.accessed_seconds) >= 86_400;
    if !due || now == previous.accessed_seconds {
        return Ok(());
    }
    let _guard = MutationGuard::acquire();
    let mut disk = DataDisk::identify_secondary().ok_or(MountError::Io)?;
    let active = active_superblock()?;
    let catalog = active_catalog(&mut disk, active)?;
    let mut inode = read_inode_from(&mut disk, catalog, index)?.ok_or(MountError::Geometry)?;
    if inode.generation != previous.generation
        || inode.accessed_seconds != previous.accessed_seconds
    {
        return Ok(());
    }
    inode.generation = active
        .generation
        .checked_add(1)
        .ok_or(MountError::Geometry)?;
    inode.accessed_seconds = now;
    let record = inode.encode().map_err(|_| MountError::Corrupt)?;
    commit_inode_change(&mut disk, active, catalog, index, &record, 0, None)
}

#[cfg(target_arch = "aarch64")]
fn current_unix_seconds() -> u64 {
    crate::aarch64_rtc::unix_seconds()
}

#[cfg(not(target_arch = "aarch64"))]
fn current_unix_seconds() -> u64 {
    crate::arch::monotonic_ticks() / 100
}

fn timestamp_seconds(seconds: u64) -> u32 {
    u32::try_from(seconds).unwrap_or(u32::MAX)
}

fn allocate_extent_map(
    bitmap: &mut BlockBitmap<'_>,
    mut block_count: u64,
) -> Result<ExtentMap, MountError> {
    let mut map = ExtentMap::EMPTY;
    while block_count != 0 {
        if map.count() == makos_makfs4::MAX_EXTENTS {
            return Err(MountError::Geometry);
        }
        let mut attempt = u32::try_from(block_count).unwrap_or(u32::MAX);
        let extent = loop {
            match bitmap.allocate(attempt) {
                Ok(extent) => break extent,
                Err(FormatError::NoSpace) if attempt > 1 => attempt = attempt.div_ceil(2),
                Err(error) => return Err(format_error(error)),
            }
        };
        block_count -= u64::from(extent.block_count);
        map.push(extent).map_err(format_error)?;
    }
    Ok(map)
}

fn format(disk: &mut DataDisk, block_count: u64) -> Result<Superblock, MountError> {
    let (bitmap_blocks, sets) = metadata_sets(block_count)?;
    let active = sets[0];

    let superblock = Superblock {
        generation: 1,
        commit_id: 1,
        block_count,
        data_start: PACKAGE_LIMIT_BLOCK,
        catalog_block: active.catalog_block,
        bitmap_block: active.bitmap_block,
        bitmap_blocks,
    };
    let catalog = Catalog {
        generation: 1,
        inode_count: 1,
        maximum_inodes: MAXIMUM_INODES,
        inode_table_block: active.inode_table_block,
        inode_table_blocks: INODE_TABLE_BLOCKS,
    };
    let mut sequence = CommitSequencer::new();
    sequence
        .complete(CommitPhase::Data)
        .map_err(|_| MountError::Corrupt)?;
    zero_blocks(disk, active.inode_table_block, INODE_TABLE_BLOCKS)?;
    let mut root = Inode::EMPTY;
    root.inode = 1;
    root.generation = 1;
    root.mode = 0o040700;
    root.uid = crate::security::INIT_UID;
    root.gid = crate::security::INIT_GID;
    root.parent = 1;
    root.set_name(b".").map_err(|_| MountError::Corrupt)?;
    let root_record = root.encode().map_err(|_| MountError::Corrupt)?;
    write_inode_record(disk, active.inode_table_block, 0, &root_record)?;
    sequence
        .complete(CommitPhase::Inodes)
        .map_err(|_| MountError::Corrupt)?;
    zero_blocks(disk, active.bitmap_block, bitmap_blocks)?;
    sequence
        .complete(CommitPhase::Bitmap)
        .map_err(|_| MountError::Corrupt)?;
    let catalog_record = catalog
        .encode(block_count, PACKAGE_LIMIT_BLOCK)
        .map_err(|_| MountError::Corrupt)?;
    write_record_block(disk, active.catalog_block, &catalog_record)?;
    sequence
        .complete(CommitPhase::Catalog)
        .map_err(|_| MountError::Corrupt)?;
    if !disk.flush() {
        return Err(MountError::Io);
    }
    sequence
        .complete(CommitPhase::FlushMetadata)
        .map_err(|_| MountError::Corrupt)?;
    let encoded = superblock.encode().map_err(|_| MountError::Corrupt)?;
    if !disk.write_sector(SUPERBLOCK_A_LBA, &encoded)
        || !disk.write_sector(SUPERBLOCK_B_LBA, &encoded)
    {
        return Err(MountError::Io);
    }
    sequence
        .complete(CommitPhase::Superblock)
        .map_err(|_| MountError::Corrupt)?;
    if !disk.flush() {
        return Err(MountError::Io);
    }
    sequence
        .complete(CommitPhase::FlushRoot)
        .map_err(|_| MountError::Corrupt)?;
    if sequence.phase() != CommitPhase::Complete {
        return Err(MountError::Corrupt);
    }
    Ok(superblock)
}

fn validate_geometry(superblock: Superblock, block_count: u64) -> Result<(), MountError> {
    let (bitmap_blocks, sets) = metadata_sets(block_count)?;
    if superblock.block_count != block_count
        || superblock.data_start != PACKAGE_LIMIT_BLOCK
        || superblock.bitmap_blocks != bitmap_blocks
        || !sets.iter().any(|set| {
            set.bitmap_block == superblock.bitmap_block
                && set.catalog_block == superblock.catalog_block
        })
    {
        return Err(MountError::Geometry);
    }
    Ok(())
}

fn reject_package_overlap(disk: &mut DataDisk) -> Result<(), MountError> {
    let mut header = [0u8; 512];
    if !disk.read_sector(PACKAGE_HEADER_LBA, &mut header) {
        return Err(MountError::Io);
    }
    if header[..8] == PACKAGE_MAGIC {
        let end_lba =
            u64::from_le_bytes(header[32..40].try_into().map_err(|_| MountError::Corrupt)?);
        if end_lba > PACKAGE_TRANSACTION_BASE_LBA {
            return Err(MountError::PackageOverlap);
        }
    }
    Ok(())
}

fn read_record(disk: &mut DataDisk, lba: u32) -> Result<[u8; RECORD_BYTES], MountError> {
    let mut record = [0u8; RECORD_BYTES];
    if !disk.read_sector(lba, &mut record) {
        return Err(MountError::Io);
    }
    Ok(record)
}

fn validate_root(
    disk: &mut DataDisk,
    record: &[u8; RECORD_BYTES],
    block_count: u64,
) -> Result<Superblock, makos_makfs4::Error> {
    let superblock = Superblock::decode(record)?;
    validate_geometry(superblock, block_count).map_err(|_| makos_makfs4::Error::Corrupt)?;
    let catalog = read_catalog(disk, superblock).map_err(|_| makos_makfs4::Error::Corrupt)?;
    if catalog.generation != superblock.generation {
        return Err(makos_makfs4::Error::Corrupt);
    }
    let inode_end = catalog
        .inode_table_block
        .checked_add(u64::from(catalog.inode_table_blocks))
        .ok_or(makos_makfs4::Error::Corrupt)?;
    let bitmap_end = superblock
        .bitmap_block
        .checked_add(u64::from(superblock.bitmap_blocks))
        .ok_or(makos_makfs4::Error::Corrupt)?;
    if catalog.inode_table_block < METADATA_FIRST_BLOCK
        || inode_end > METADATA_LIMIT_BLOCK
        || ranges_overlap(
            catalog.inode_table_block,
            inode_end,
            superblock.bitmap_block,
            bitmap_end,
        )
        || (catalog.inode_table_block..inode_end).contains(&superblock.catalog_block)
    {
        return Err(makos_makfs4::Error::Corrupt);
    }
    let (_, sets) = metadata_sets(block_count).map_err(|_| makos_makfs4::Error::Corrupt)?;
    if !sets.iter().any(|set| {
        set.bitmap_block == superblock.bitmap_block
            && set.catalog_block == superblock.catalog_block
            && set.inode_table_block == catalog.inode_table_block
    }) {
        return Err(makos_makfs4::Error::Corrupt);
    }
    Ok(superblock)
}

const fn ranges_overlap(
    first_start: u64,
    first_end: u64,
    second_start: u64,
    second_end: u64,
) -> bool {
    first_start < second_end && second_start < first_end
}

fn read_catalog(disk: &mut DataDisk, superblock: Superblock) -> Result<Catalog, MountError> {
    let mut block = [0u8; 4096];
    if !read_block(disk, superblock.catalog_block, &mut block) {
        return Err(MountError::Io);
    }
    let mut record = [0u8; RECORD_BYTES];
    record.copy_from_slice(&block[..RECORD_BYTES]);
    Catalog::decode(&record, superblock.block_count, superblock.data_start)
        .map_err(|_| MountError::Corrupt)
}

fn active_catalog(disk: &mut DataDisk, superblock: Superblock) -> Result<Catalog, MountError> {
    if let Some(catalog) = with_inode_cache(|cache| {
        (cache.ready && cache.generation == superblock.generation).then_some(Catalog {
            generation: cache.generation,
            inode_count: cache.inode_count,
            maximum_inodes: MAXIMUM_INODES,
            inode_table_block: cache.inode_table_block,
            inode_table_blocks: INODE_TABLE_BLOCKS,
        })
    }) {
        return Ok(catalog);
    }
    read_catalog(disk, superblock)
}

fn read_inode_from(
    disk: &mut DataDisk,
    catalog: Catalog,
    index: u32,
) -> Result<Option<Inode>, MountError> {
    if index >= catalog.maximum_inodes {
        return Ok(None);
    }
    if let Some(inode) = cached_inode(catalog, index) {
        return Ok(inode);
    }
    let lba = catalog
        .inode_table_block
        .checked_mul(SECTORS_PER_BLOCK)
        .and_then(|lba| lba.checked_add(u64::from(index)))
        .and_then(|lba| u32::try_from(lba).ok())
        .ok_or(MountError::Geometry)?;
    let record = read_record(disk, lba)?;
    if record.iter().all(|byte| *byte == 0) {
        return Ok(None);
    }
    Inode::decode(&record)
        .map(Some)
        .map_err(|_| MountError::Corrupt)
}

fn load_inode_cache(disk: &mut DataDisk, superblock: Superblock) -> Result<(), MountError> {
    let catalog = read_catalog(disk, superblock)?;
    if catalog.maximum_inodes != MAXIMUM_INODES {
        return Err(MountError::Geometry);
    }
    let mut entries = [None; MAXIMUM_INODES as usize];
    let mut count = 0u32;
    let mut free_hint = MAXIMUM_INODES;
    for index in 0..MAXIMUM_INODES {
        let lba = catalog
            .inode_table_block
            .checked_mul(SECTORS_PER_BLOCK)
            .and_then(|lba| lba.checked_add(u64::from(index)))
            .and_then(|lba| u32::try_from(lba).ok())
            .ok_or(MountError::Geometry)?;
        let record = read_record(disk, lba)?;
        if record.iter().all(|byte| *byte == 0) {
            if index != 0 && free_hint == MAXIMUM_INODES {
                free_hint = index;
            }
            continue;
        }
        let inode = Inode::decode(&record).map_err(|_| MountError::Corrupt)?;
        inode
            .validate_on_volume(superblock.data_start, superblock.block_count)
            .map_err(|_| MountError::Corrupt)?;
        entries[index as usize] = Some(inode);
        count = count.checked_add(1).ok_or(MountError::Corrupt)?;
    }
    if count != catalog.inode_count || entries[0].is_none() {
        return Err(MountError::Corrupt);
    }
    let (_, metadata_sets) = metadata_sets(superblock.block_count)?;
    let active_set = metadata_sets
        .iter()
        .position(|set| set.inode_table_block == catalog.inode_table_block)
        .ok_or(MountError::Geometry)?;
    let mut inode_dirty = [[u64::MAX; INODE_DIRTY_WORDS]; 3];
    let mut bitmap_dirty = [u64::MAX; 3];
    inode_dirty[active_set] = [0; INODE_DIRTY_WORDS];
    bitmap_dirty[active_set] = 0;
    let mut child_index = DirectoryIndex::EMPTY;
    child_index
        .rebuild(&entries)
        .map_err(|_| MountError::Geometry)?;
    with_inode_cache(|cache| {
        *cache = InodeCache {
            ready: true,
            generation: catalog.generation,
            inode_table_block: catalog.inode_table_block,
            inode_count: catalog.inode_count,
            free_hint,
            entries,
            child_index,
            metadata_sets,
            inode_dirty,
            bitmap_dirty,
        };
    });
    Ok(())
}

/// Outer `Option` denotes cache validity; inner value denotes inode presence.
fn cached_inode(catalog: Catalog, index: u32) -> Option<Option<Inode>> {
    with_inode_cache(|cache| {
        if !cache.ready
            || cache.generation != catalog.generation
            || cache.inode_table_block != catalog.inode_table_block
            || index >= catalog.maximum_inodes
        {
            return None;
        }
        Some(cache.entries[index as usize])
    })
}

fn cached_free_inode(catalog: Catalog) -> Option<u32> {
    with_inode_cache(|cache| {
        if !cache.ready
            || cache.generation != catalog.generation
            || cache.inode_table_block != catalog.inode_table_block
        {
            return None;
        }
        let hint = cache.free_hint;
        (hint < catalog.maximum_inodes && cache.entries[hint as usize].is_none()).then_some(hint)
    })
}

/// Outer `Option` denotes cache validity; inner value is matching child.
fn cached_child_index(catalog: Catalog, parent: u64, name: &[u8]) -> Option<Option<u32>> {
    with_inode_cache(|cache| {
        if !cache.ready
            || cache.generation != catalog.generation
            || cache.inode_table_block != catalog.inode_table_block
        {
            return None;
        }
        Some(cache.child_index.find(&cache.entries, parent, name))
    })
}

/// Find `ordinal`th child at or after raw inode-table `start`.
fn cached_child_at(
    catalog: Catalog,
    parent: u64,
    ordinal: usize,
    start: u32,
) -> Option<Option<(u32, Inode)>> {
    with_inode_cache(|cache| {
        if !cache.ready
            || cache.generation != catalog.generation
            || cache.inode_table_block != catalog.inode_table_block
        {
            return None;
        }
        let mut current = 0usize;
        for index in start..catalog.maximum_inodes {
            if let Some(inode) = cache.entries[index as usize]
                && inode.parent == parent
            {
                if current == ordinal {
                    return Some(Some((index, inode)));
                }
                current += 1;
            }
        }
        Some(None)
    })
}

fn cached_child_count(catalog: Catalog, parent: u64) -> Option<usize> {
    with_inode_cache(|cache| {
        if !cache.ready
            || cache.generation != catalog.generation
            || cache.inode_table_block != catalog.inode_table_block
        {
            return None;
        }
        Some(
            cache.entries[1..catalog.maximum_inodes as usize]
                .iter()
                .filter(|inode| inode.is_some_and(|inode| inode.parent == parent))
                .count(),
        )
    })
}

/// Return exact on-disk encoding from validated cache, including zero records.
fn cached_inode_record(
    catalog: Catalog,
    index: u32,
) -> Option<Result<[u8; RECORD_BYTES], MountError>> {
    cached_inode(catalog, index).map(|inode| match inode {
        Some(inode) => inode.encode().map_err(|_| MountError::Corrupt),
        None => Ok([0; RECORD_BYTES]),
    })
}

fn update_inode_cache_after_commit(
    active: Superblock,
    old_catalog: Catalog,
    next_catalog: Catalog,
    index: u32,
    replacement: &[u8; RECORD_BYTES],
    second: Option<(u32, &[u8; RECORD_BYTES])>,
    target: MetadataSet,
    bitmap_changed: u64,
) -> Result<(), MountError> {
    let decode = |record: &[u8; RECORD_BYTES]| -> Result<Option<Inode>, MountError> {
        if record.iter().all(|byte| *byte == 0) {
            return Ok(None);
        }
        let inode = Inode::decode(record).map_err(|_| MountError::Corrupt)?;
        inode
            .validate_on_volume(active.data_start, active.block_count)
            .map_err(|_| MountError::Corrupt)?;
        Ok(Some(inode))
    };
    let replacement = decode(replacement)?;
    let second = second
        .map(|(index, record)| decode(record).map(|inode| (index, inode)))
        .transpose()?;
    with_inode_cache(|cache| {
        if !cache.ready
            || cache.generation != old_catalog.generation
            || cache.inode_table_block != old_catalog.inode_table_block
        {
            cache.ready = false;
            return;
        }
        cache.entries[index as usize] = replacement;
        if let Some((second_index, inode)) = second {
            cache.entries[second_index as usize] = inode;
        }
        if cache.child_index.rebuild(&cache.entries).is_err() {
            cache.ready = false;
            return;
        }
        cache.generation = next_catalog.generation;
        cache.inode_table_block = next_catalog.inode_table_block;
        cache.inode_count = next_catalog.inode_count;
        cache.free_hint = (1..MAXIMUM_INODES)
            .find(|candidate| cache.entries[*candidate as usize].is_none())
            .unwrap_or(MAXIMUM_INODES);
        let Some(target_index) = cache
            .metadata_sets
            .iter()
            .position(|set| set.inode_table_block == target.inode_table_block)
        else {
            cache.ready = false;
            return;
        };
        cache.inode_dirty[target_index] = [0; INODE_DIRTY_WORDS];
        cache.bitmap_dirty[target_index] = 0;
        for set_index in 0..cache.metadata_sets.len() {
            if set_index == target_index {
                continue;
            }
            mark_inode_dirty(&mut cache.inode_dirty[set_index], index);
            if let Some((second_index, _)) = second {
                mark_inode_dirty(&mut cache.inode_dirty[set_index], second_index);
            }
            cache.bitmap_dirty[set_index] |= bitmap_changed;
        }
    });
    Ok(())
}

fn mark_inode_dirty(words: &mut [u64; INODE_DIRTY_WORDS], index: u32) {
    words[index as usize / 64] |= 1u64 << (index % 64);
}

fn commit_dirty_snapshot(catalog: Catalog, target: MetadataSet) -> ([u64; INODE_DIRTY_WORDS], u64) {
    with_inode_cache(|cache| {
        if cache.ready
            && cache.generation == catalog.generation
            && cache.inode_table_block == catalog.inode_table_block
            && let Some(index) = cache
                .metadata_sets
                .iter()
                .position(|set| set.inode_table_block == target.inode_table_block)
        {
            return (cache.inode_dirty[index], cache.bitmap_dirty[index]);
        }
        ([u64::MAX; INODE_DIRTY_WORDS], u64::MAX)
    })
}

fn with_inode_cache<R>(function: impl FnOnce(&mut InodeCache) -> R) -> R {
    while INODE_CACHE
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *INODE_CACHE.value.get() });
    INODE_CACHE.lock.store(false, Ordering::Release);
    result
}

fn commit_inode_change(
    disk: &mut DataDisk,
    active: Superblock,
    catalog: Catalog,
    index: u32,
    replacement: &[u8; RECORD_BYTES],
    inode_count_delta: i32,
    bitmap_words: Option<&[u64]>,
) -> Result<(), MountError> {
    commit_inode_changes(
        disk,
        active,
        catalog,
        index,
        replacement,
        None,
        inode_count_delta,
        bitmap_words,
    )
}

fn commit_inode_changes(
    disk: &mut DataDisk,
    active: Superblock,
    catalog: Catalog,
    index: u32,
    replacement: &[u8; RECORD_BYTES],
    second: Option<(u32, &[u8; RECORD_BYTES])>,
    inode_count_delta: i32,
    bitmap_words: Option<&[u64]>,
) -> Result<(), MountError> {
    let (target, root_lba) = select_commit_target(disk, active.block_count)?;
    let (inode_dirty, bitmap_dirty) = commit_dirty_snapshot(catalog, target);
    let generation = active
        .generation
        .checked_add(1)
        .ok_or(MountError::Geometry)?;
    let mut sequence = CommitSequencer::new();
    sequence
        .complete(CommitPhase::Data)
        .map_err(|_| MountError::Corrupt)?;

    for inode_index in 0..MAXIMUM_INODES {
        let dirty = inode_dirty[inode_index as usize / 64] & (1u64 << (inode_index % 64)) != 0;
        if !dirty
            && inode_index != index
            && !second.is_some_and(|(second_index, _)| inode_index == second_index)
        {
            continue;
        }
        let source_lba = catalog
            .inode_table_block
            .checked_mul(SECTORS_PER_BLOCK)
            .and_then(|lba| lba.checked_add(u64::from(inode_index)))
            .and_then(|lba| u32::try_from(lba).ok())
            .ok_or(MountError::Geometry)?;
        let record = if inode_index == index {
            *replacement
        } else if second.is_some_and(|(second_index, _)| inode_index == second_index) {
            *second
                .map(|(_, replacement)| replacement)
                .ok_or(MountError::Geometry)?
        } else if let Some(record) = cached_inode_record(catalog, inode_index) {
            record?
        } else {
            read_record(disk, source_lba)?
        };
        write_inode_record(disk, target.inode_table_block, inode_index, &record)?;
    }
    sequence
        .complete(CommitPhase::Inodes)
        .map_err(|_| MountError::Corrupt)?;

    let mut bitmap_changed = 0u64;
    if let Some(words) = bitmap_words {
        let mut word_index = 0usize;
        for offset in 0..active.bitmap_blocks {
            let mut desired = [0u8; 4096];
            if !read_block(disk, active.bitmap_block + u64::from(offset), &mut desired) {
                return Err(MountError::Io);
            }
            let bit = 1u64.checked_shl(offset).ok_or(MountError::Geometry)?;
            let mut block_changed = false;
            for chunk in desired.chunks_exact_mut(8) {
                let encoded = if word_index < words.len() {
                    let encoded = words[word_index].to_le_bytes();
                    word_index += 1;
                    encoded
                } else {
                    [0; 8]
                };
                block_changed |= *chunk != encoded;
                chunk.copy_from_slice(&encoded);
            }
            if block_changed {
                bitmap_changed |= bit;
            }
            if (bitmap_dirty | bitmap_changed) & bit != 0
                && !write_block(disk, target.bitmap_block + u64::from(offset), &desired)
            {
                return Err(MountError::Io);
            }
        }
        if word_index != words.len() {
            return Err(MountError::Geometry);
        }
    } else {
        for offset in 0..active.bitmap_blocks {
            let bit = 1u64.checked_shl(offset).ok_or(MountError::Geometry)?;
            if bitmap_dirty & bit == 0 {
                continue;
            }
            let mut block = [0u8; 4096];
            if !read_block(disk, active.bitmap_block + u64::from(offset), &mut block)
                || !write_block(disk, target.bitmap_block + u64::from(offset), &block)
            {
                return Err(MountError::Io);
            }
        }
    }
    sequence
        .complete(CommitPhase::Bitmap)
        .map_err(|_| MountError::Corrupt)?;

    let inode_count = i64::from(catalog.inode_count)
        .checked_add(i64::from(inode_count_delta))
        .and_then(|count| u32::try_from(count).ok())
        .filter(|count| *count <= catalog.maximum_inodes)
        .ok_or(MountError::Geometry)?;
    let next_catalog = Catalog {
        generation,
        inode_count,
        inode_table_block: target.inode_table_block,
        ..catalog
    };
    let catalog_record = next_catalog
        .encode(active.block_count, active.data_start)
        .map_err(|_| MountError::Corrupt)?;
    write_catalog_record(disk, target.catalog_block, &catalog_record)?;
    sequence
        .complete(CommitPhase::Catalog)
        .map_err(|_| MountError::Corrupt)?;
    if !disk.flush() {
        return Err(MountError::Io);
    }
    sequence
        .complete(CommitPhase::FlushMetadata)
        .map_err(|_| MountError::Corrupt)?;

    let next = Superblock {
        generation,
        commit_id: generation,
        catalog_block: target.catalog_block,
        bitmap_block: target.bitmap_block,
        ..active
    };
    let encoded = next.encode().map_err(|_| MountError::Corrupt)?;
    if !disk.write_sector(root_lba, &encoded) {
        return Err(MountError::Io);
    }
    sequence
        .complete(CommitPhase::Superblock)
        .map_err(|_| MountError::Corrupt)?;
    if !disk.flush() {
        return Err(MountError::Io);
    }
    sequence
        .complete(CommitPhase::FlushRoot)
        .map_err(|_| MountError::Corrupt)?;
    if sequence.phase() != CommitPhase::Complete {
        return Err(MountError::Corrupt);
    }
    update_inode_cache_after_commit(
        active,
        catalog,
        next_catalog,
        index,
        replacement,
        second,
        target,
        bitmap_changed,
    )?;
    activate(next);
    Ok(())
}

fn select_commit_target(
    disk: &mut DataDisk,
    block_count: u64,
) -> Result<(MetadataSet, u32), MountError> {
    let first_record = read_record(disk, SUPERBLOCK_A_LBA)?;
    let second_record = read_record(disk, SUPERBLOCK_B_LBA)?;
    let first = validate_root(disk, &first_record, block_count).ok();
    let second = validate_root(disk, &second_record, block_count).ok();
    let (_, sets) = metadata_sets(block_count)?;
    let target = sets
        .iter()
        .copied()
        .find(|set| {
            first.is_none_or(|root| root.catalog_block != set.catalog_block)
                && second.is_none_or(|root| root.catalog_block != set.catalog_block)
        })
        .ok_or(MountError::Geometry)?;
    let root_lba = match (first, second) {
        (None, _) => SUPERBLOCK_A_LBA,
        (_, None) => SUPERBLOCK_B_LBA,
        (Some(first), Some(second)) if first.generation <= second.generation => SUPERBLOCK_A_LBA,
        (Some(_), Some(_)) => SUPERBLOCK_B_LBA,
    };
    Ok((target, root_lba))
}

fn read_bitmap_words(
    disk: &mut DataDisk,
    active: Superblock,
) -> Result<alloc::vec::Vec<u64>, MountError> {
    let data_blocks = active.block_count - active.data_start;
    let word_count = usize::try_from(data_blocks.div_ceil(64)).map_err(|_| MountError::Geometry)?;
    let mut words = vec![0u64; word_count];
    let mut word_index = 0;
    for offset in 0..active.bitmap_blocks {
        let mut block = [0u8; 4096];
        if !read_block(disk, active.bitmap_block + u64::from(offset), &mut block) {
            return Err(MountError::Io);
        }
        for chunk in block.chunks_exact(8) {
            if word_index == words.len() {
                break;
            }
            words[word_index] =
                u64::from_le_bytes(chunk.try_into().map_err(|_| MountError::Corrupt)?);
            word_index += 1;
        }
    }
    if word_index != words.len() {
        return Err(MountError::Corrupt);
    }
    Ok(words)
}

fn read_inode_block(
    disk: &mut DataDisk,
    inode: &Inode,
    file_block: u64,
    output: &mut [u8; 4096],
) -> Result<(), MountError> {
    let Some(device_block) = inode.extents.file_block_to_device(file_block) else {
        output.fill(0);
        return Ok(());
    };
    read_block(disk, device_block, output)
        .then_some(())
        .ok_or(MountError::Io)
}

const fn format_error(error: FormatError) -> MountError {
    match error {
        FormatError::Corrupt => MountError::Corrupt,
        FormatError::NoSpace | FormatError::TooFragmented => MountError::NoSpace,
        FormatError::Invalid | FormatError::OutOfRange | FormatError::WrongPhase => {
            MountError::Geometry
        }
    }
}

fn active_superblock() -> Result<Superblock, MountError> {
    let block_count = ACTIVE_BLOCK_COUNT.load(Ordering::Acquire);
    let generation = ACTIVE_GENERATION.load(Ordering::Acquire);
    let catalog_block = ACTIVE_CATALOG_BLOCK.load(Ordering::Acquire);
    let bitmap_block = ACTIVE_BITMAP_BLOCK.load(Ordering::Acquire);
    if generation == 0 || block_count == 0 {
        return Err(MountError::Corrupt);
    }
    let data_blocks = block_count - PACKAGE_LIMIT_BLOCK;
    let bitmap_blocks =
        u32::try_from(data_blocks.div_ceil(BLOCK_BYTES * 8)).map_err(|_| MountError::Geometry)?;
    Ok(Superblock {
        generation,
        commit_id: generation,
        block_count,
        data_start: PACKAGE_LIMIT_BLOCK,
        catalog_block,
        bitmap_block,
        bitmap_blocks,
    })
}

fn metadata_sets(block_count: u64) -> Result<(u32, [MetadataSet; 3]), MountError> {
    let data_blocks = block_count
        .checked_sub(PACKAGE_LIMIT_BLOCK)
        .ok_or(MountError::Geometry)?;
    let bitmap_blocks =
        u32::try_from(data_blocks.div_ceil(BLOCK_BYTES * 8)).map_err(|_| MountError::Geometry)?;
    let set_blocks = u64::from(bitmap_blocks) + 1 + u64::from(INODE_TABLE_BLOCKS);
    let first = MetadataSet {
        bitmap_block: METADATA_FIRST_BLOCK,
        catalog_block: METADATA_FIRST_BLOCK + u64::from(bitmap_blocks),
        inode_table_block: METADATA_FIRST_BLOCK + u64::from(bitmap_blocks) + 1,
    };
    let second_start = METADATA_FIRST_BLOCK + set_blocks;
    let second = MetadataSet {
        bitmap_block: second_start,
        catalog_block: second_start + u64::from(bitmap_blocks),
        inode_table_block: second_start + u64::from(bitmap_blocks) + 1,
    };
    let third_start = second_start + set_blocks;
    let third = MetadataSet {
        bitmap_block: third_start,
        catalog_block: third_start + u64::from(bitmap_blocks),
        inode_table_block: third_start + u64::from(bitmap_blocks) + 1,
    };
    if third.inode_table_block + u64::from(INODE_TABLE_BLOCKS) > METADATA_LIMIT_BLOCK {
        return Err(MountError::Geometry);
    }
    Ok((bitmap_blocks, [first, second, third]))
}

fn activate(superblock: Superblock) {
    ACTIVE_BLOCK_COUNT.store(superblock.block_count, Ordering::Release);
    ACTIVE_BITMAP_BLOCK.store(superblock.bitmap_block, Ordering::Release);
    ACTIVE_CATALOG_BLOCK.store(superblock.catalog_block, Ordering::Release);
    ACTIVE_GENERATION.store(superblock.generation, Ordering::Release);
}

fn zero_blocks(disk: &mut DataDisk, start: u64, count: u32) -> Result<(), MountError> {
    let zero = [0u8; 4096];
    for block in start..start + u64::from(count) {
        if !write_block(disk, block, &zero) {
            return Err(MountError::Io);
        }
    }
    Ok(())
}

fn write_record_block(
    disk: &mut DataDisk,
    block: u64,
    record: &[u8; RECORD_BYTES],
) -> Result<(), MountError> {
    let mut output = [0u8; 4096];
    output[..RECORD_BYTES].copy_from_slice(record);
    write_block(disk, block, &output)
        .then_some(())
        .ok_or(MountError::Io)
}

/// Catalog encoding occupies one sector. Remaining block bytes are reserved and
/// ignored by the format, so rewriting them on every commit only adds I/O.
fn write_catalog_record(
    disk: &mut DataDisk,
    block: u64,
    record: &[u8; RECORD_BYTES],
) -> Result<(), MountError> {
    let lba = block
        .checked_mul(SECTORS_PER_BLOCK)
        .and_then(|lba| u32::try_from(lba).ok())
        .ok_or(MountError::Geometry)?;
    disk.write_sector(lba, record)
        .then_some(())
        .ok_or(MountError::Io)
}

fn write_inode_record(
    disk: &mut DataDisk,
    inode_table_block: u64,
    index: u32,
    record: &[u8; RECORD_BYTES],
) -> Result<(), MountError> {
    if index >= MAXIMUM_INODES {
        return Err(MountError::Geometry);
    }
    let lba = inode_table_block
        .checked_mul(SECTORS_PER_BLOCK)
        .and_then(|lba| lba.checked_add(u64::from(index)))
        .and_then(|lba| u32::try_from(lba).ok())
        .ok_or(MountError::Geometry)?;
    disk.write_sector(lba, record)
        .then_some(())
        .ok_or(MountError::Io)
}

fn read_block(disk: &mut DataDisk, block: u64, output: &mut [u8; 4096]) -> bool {
    let Some(first_lba) = makos_makfs4::block_first_sector(block, SECTOR_BYTES)
        .and_then(|lba| u32::try_from(lba).ok())
    else {
        return false;
    };
    #[cfg(target_arch = "aarch64")]
    {
        return disk.read_sectors_8(first_lba, output);
    }
    #[cfg(not(target_arch = "aarch64"))]
    for sector in 0..SECTORS_PER_BLOCK as usize {
        let mut data = [0u8; 512];
        if !disk.read_sector(first_lba + sector as u32, &mut data) {
            return false;
        }
        output[sector * 512..(sector + 1) * 512].copy_from_slice(&data);
    }
    #[cfg(not(target_arch = "aarch64"))]
    true
}

fn write_block(disk: &mut DataDisk, block: u64, input: &[u8; 4096]) -> bool {
    let Some(first_lba) = makos_makfs4::block_first_sector(block, SECTOR_BYTES)
        .and_then(|lba| u32::try_from(lba).ok())
    else {
        return false;
    };
    #[cfg(target_arch = "aarch64")]
    {
        return disk.write_sectors_8(first_lba, input);
    }
    #[cfg(not(target_arch = "aarch64"))]
    for sector in 0..SECTORS_PER_BLOCK as usize {
        let mut data = [0u8; 512];
        data.copy_from_slice(&input[sector * 512..(sector + 1) * 512]);
        if !disk.write_sector(first_lba + sector as u32, &data) {
            return false;
        }
    }
    #[cfg(not(target_arch = "aarch64"))]
    true
}

struct MutationGuard;

impl MutationGuard {
    fn acquire() -> Self {
        while MUTATION_LOCK
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        Self
    }
}

impl Drop for MutationGuard {
    fn drop(&mut self) {
        MUTATION_LOCK.store(false, Ordering::Release);
    }
}
