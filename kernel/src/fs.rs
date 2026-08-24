use crate::block::DataDisk;

const MAGIC: [u8; 8] = *b"MAKFS001";
const VERSION: u32 = 1;
const SUPERBLOCK_LBA: u32 = 1;
const BACKUP_LBA: u32 = 2;
const ROOT_DIRECTORY_LBA: u32 = 3;
const FILE_DATA_LBA: u32 = 4;
const UPDATE_LBA: u32 = 5;
const USER_FILE_LBA: u32 = 6;
const LEGACY_DYNAMIC_FILE_LBA_BASE: u32 = 7;
const DYNAMIC_ALLOCATION_LBA: u32 = 7;
const DYNAMIC_INODE_LBA_BASE: u32 = 8;
const DYNAMIC_DATA_LBA_BASE: u32 = 32;
const DYNAMIC_DATA_BLOCK_COUNT: usize = 80;
const DYNAMIC_BITMAP_BYTES: usize = DYNAMIC_DATA_BLOCK_COUNT.div_ceil(8);
const MAX_DYNAMIC_BLOCKS: usize = crate::vfs::MAX_FILE_BYTES.div_ceil(512);
const ROOT_MAGIC: [u8; 8] = *b"MAKDIR01";
const FILE_NAME: &[u8] = b"boot-count.txt";
const PACKAGE_HEADER_LBA: u32 = 2048;
const PACKAGE_ENTRY_LBA: u64 = 2049;
const PACKAGE_DATA_LBA: u64 = 4096;
const PACKAGE_TRANSACTION_BASE_LBA: u64 = makos_package_store::PRODUCTION_BASE_SECTOR;
const STATIC_PACKAGE_FILE_LIMIT: usize =
    crate::vfs::SYSTEM_PACKAGE_FILE_COUNT - makos_package_store::MAX_PACKAGES;

// Block adapter for layout-v1 durable package region. Region opening below
// proves full geometry and legacy-static-package non-overlap before any write.
impl makos_package_store::SectorDevice for DataDisk {
    fn sector_count(&self) -> u64 {
        DataDisk::sectors(self)
    }

    fn read_sector(&mut self, sector: u64, output: &mut [u8; 512]) -> bool {
        u32::try_from(sector)
            .ok()
            .is_some_and(|lba| DataDisk::read_sector(self, lba, output))
    }

    fn write_sector(&mut self, sector: u64, input: &[u8; 512]) -> bool {
        if crate::vfs::package_transaction_sector_pinned(sector) {
            crate::serial_println!(
                "MAKOS_PACKAGE_PINNED_SLOT_BLOCKED sector={} result=retry-after-close",
                sector,
            );
            return false;
        }
        u32::try_from(sector)
            .ok()
            .is_some_and(|lba| DataDisk::write_sector(self, lba, input))
    }

    fn flush(&mut self) -> bool {
        DataDisk::flush(self)
    }
}

/// Open durable package region only when current disk geometry supports it and
/// legacy static payload does not overlap it. Small old images keep RAM store.
pub(crate) fn package_transaction_store()
-> Result<Option<makos_package_store::Store<DataDisk>>, makos_package_store::Error> {
    let Some(mut disk) = DataDisk::identify_secondary() else {
        return Ok(None);
    };
    if disk.sectors() < makos_package_store::PRODUCTION_END_SECTOR {
        return Ok(None);
    }
    let mut header = [0u8; 512];
    if !disk.read_sector(PACKAGE_HEADER_LBA, &mut header) {
        return Err(makos_package_store::Error::Io);
    }
    if header[..8] == *b"MAKPKG01" {
        if crc32(&header[..508]) != read_u32(&header, 508).unwrap_or(0)
            || read_u64(&header, 32).is_none_or(|end| end > PACKAGE_TRANSACTION_BASE_LBA)
        {
            return Err(makos_package_store::Error::InvalidGeometry);
        }
    }
    makos_package_store::Store::open(
        disk,
        makos_package_store::PRODUCTION_BASE_SECTOR,
        makos_package_store::PRODUCTION_SLOT_SECTORS,
    )
    .map(Some)
}

#[derive(Clone, Copy)]
struct Superblock {
    generation: u64,
    boot_count: u64,
}

pub fn mount_and_test(allow_recovery: bool) {
    let mut disk =
        DataDisk::identify_secondary().unwrap_or_else(|| crate::fatal("MakOS data disk absent"));
    if disk.sectors() < 4096 {
        crate::fatal("ATA data disk too small");
    }
    let primary = read_superblock(&mut disk, SUPERBLOCK_LBA);
    let backup = read_superblock(&mut disk, BACKUP_LBA);
    let recovered_copy = match (primary.is_some(), backup.is_some()) {
        (false, true) => Some(("primary", "backup")),
        (true, false) => Some(("backup", "primary")),
        _ => None,
    };
    if recovered_copy.is_some() && !allow_recovery {
        crate::fatal("MakFS degraded and automatic recovery disabled");
    }
    let previous = match (primary, backup) {
        (Some(first), Some(second)) => Some(if first.generation >= second.generation {
            first
        } else {
            second
        }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    };
    let state = Superblock {
        generation: previous.map_or(1, |value| value.generation.saturating_add(1)),
        boot_count: previous.map_or(1, |value| value.boot_count.saturating_add(1)),
    };
    let encoded = encode(state);
    if !disk.write_sector(BACKUP_LBA, &encoded) || !disk.write_sector(SUPERBLOCK_LBA, &encoded) {
        crate::fatal("MakFS superblock commit failed");
    }
    let verified_primary = read_superblock(&mut disk, SUPERBLOCK_LBA)
        .unwrap_or_else(|| crate::fatal("MakFS primary repair verification failed"));
    let verified_backup = read_superblock(&mut disk, BACKUP_LBA)
        .unwrap_or_else(|| crate::fatal("MakFS backup repair verification failed"));
    if verified_primary.generation != state.generation
        || verified_primary.boot_count != state.boot_count
        || verified_backup.generation != state.generation
        || verified_backup.boot_count != state.boot_count
    {
        crate::fatal("MakFS persisted state mismatch");
    }
    if let Some((degraded, source)) = recovered_copy {
        crate::serial_println!(
            "MAKOS_MAKFS_RECOVERY_OK degraded_copy={} recovered_from={} repaired=1 generation={}",
            degraded,
            source,
            state.generation
        );
    }
    let file_text = write_root_file(&mut disk, state);
    let mut user_data = [0u8; 64];
    let user_length = read_user_file(&mut disk, &mut user_data).unwrap_or(0);
    ensure_dynamic_area(&mut disk);
    reconcile_dynamic_allocation(&mut disk);
    let mut dynamic = alloc::vec![
        crate::vfs::MountedDynamicFile::EMPTY;
        crate::vfs::DYNAMIC_FILE_COUNT
    ];
    for (slot, file) in dynamic.iter_mut().enumerate() {
        *file = read_dynamic_file(&mut disk, slot);
    }
    let packages = read_package_manifest(&mut disk);
    match crate::makfs4_volume::mount_or_format(&mut disk) {
        Ok(crate::makfs4_volume::MountResult::DeferredSmallDisk) => {
            crate::serial_println!("MAKOS_MAKFS4_DEFERRED reason=disk-below-1GiB legacy_v3=active");
        }
        Ok(_) => {}
        Err(error) => {
            crate::serial_println!("MAKOS_MAKFS4_ERROR error={:?}", error);
            crate::fatal("MakFS4 mount/format failed");
        }
    }
    crate::log::mount_persistent();
    crate::vfs::mount_files(
        file_text.as_bytes(),
        &user_data[..user_length],
        &dynamic,
        &packages,
    );
    crate::serial_println!(
        "MAKOS_USER_FILE_MOUNT path=/home/user/note.txt previous={} mode=0600 uid=1000 gid=1000",
        u8::from(user_length != 0)
    );
    update_self_test(&mut disk, state);
    crate::serial_println!(
        "MAKOS_M4_OK ata_sectors={} makfs_generation={} boot_count={} checksum=ok root=/ file=/boot-count.txt mode=0644 uid=0 gid=0 bytes={}",
        disk.sectors(),
        state.generation,
        state.boot_count,
        file_text.len()
    );
}

fn read_package_manifest(disk: &mut DataDisk) -> alloc::vec::Vec<crate::vfs::MountedPackageFile> {
    let mut files = alloc::vec![
        crate::vfs::MountedPackageFile::EMPTY;
        crate::vfs::SYSTEM_PACKAGE_FILE_COUNT
    ];
    let mut header = [0u8; 512];
    if !disk.read_sector(PACKAGE_HEADER_LBA, &mut header) || header[0..8] != *b"MAKPKG01" {
        append_transaction_package_files(&mut files);
        return files;
    }
    let valid = crc32(&header[..508]) == read_u32(&header, 508).unwrap_or(0)
        && read_u32(&header, 8) == Some(1)
        && read_u64(&header, 16) == Some(PACKAGE_ENTRY_LBA)
        && read_u64(&header, 24) == Some(PACKAGE_DATA_LBA);
    let count = read_u32(&header, 12).unwrap_or(u32::MAX) as usize;
    let end_lba = read_u64(&header, 32).unwrap_or(u64::MAX);
    if !valid
        || count > STATIC_PACKAGE_FILE_LIMIT
        || PACKAGE_ENTRY_LBA + count as u64 > PACKAGE_DATA_LBA
        || end_lba < PACKAGE_DATA_LBA
        || end_lba > PACKAGE_TRANSACTION_BASE_LBA
        || end_lba > disk.sectors()
    {
        crate::fatal("MakOS package manifest invalid");
    }
    for index in 0..count {
        let mut record = [0u8; 512];
        let lba = u32::try_from(PACKAGE_ENTRY_LBA + index as u64)
            .unwrap_or_else(|_| crate::fatal("MakOS package entry LBA overflow"));
        if !disk.read_sector(lba, &mut record)
            || record[0..8] != *b"MAKFILE4"
            || crc32(&record[..508]) != read_u32(&record, 508).unwrap_or(0)
            || read_u16(&record, 10) != Some(1)
            || read_u32(&record, 12) != Some(index as u32)
        {
            crate::fatal("MakOS package entry invalid");
        }
        let path_length = read_u16(&record, 8).unwrap_or(0) as usize;
        let size = read_u64(&record, 16).unwrap_or(u64::MAX);
        let first_lba = read_u64(&record, 24).unwrap_or(0);
        let sectors = read_u64(&record, 32).unwrap_or(u64::MAX);
        let path_end = 64usize.saturating_add(path_length);
        if path_length < 2
            || path_length > crate::vfs::SYSTEM_PACKAGE_PATH_BYTES
            || path_end > 508
            || record[64] != b'/'
            || record[64..path_end].contains(&0)
            || record[64..path_end] == *b"/packages"
            || record[64..path_end].starts_with(b"/packages/")
            || sectors != size.div_ceil(512)
            || first_lba < PACKAGE_DATA_LBA
            || first_lba
                .checked_add(sectors)
                .is_none_or(|end| end > end_lba)
        {
            crate::fatal("MakOS package entry bounds invalid");
        }
        if files[..index].iter().any(|file| {
            file.used
                && file.path_length == path_length
                && file.path[..path_length] == record[64..path_end]
        }) {
            crate::fatal("MakOS package duplicate path");
        }
        let mut file = crate::vfs::MountedPackageFile::EMPTY;
        file.used = true;
        file.path[..path_length].copy_from_slice(&record[64..path_end]);
        file.path_length = path_length;
        file.size = size;
        file.first_lba = first_lba;
        file.sectors = sectors;
        file.data_crc = read_u32(&record, 40).unwrap_or(0);
        files[index] = file;
    }
    crate::serial_println!(
        "MAKOS_PACKAGE_FS_OK files={} data_lba={} end_lba={} disk_backed=1 max_file_bytes={}",
        count,
        PACKAGE_DATA_LBA,
        end_lba,
        end_lba.saturating_sub(PACKAGE_DATA_LBA) * 512
    );
    append_transaction_package_files(&mut files);
    files
}

fn append_transaction_package_files(files: &mut [crate::vfs::MountedPackageFile]) {
    match transaction_package_files() {
        Ok(transaction) => {
            for file in transaction {
                let slot = files
                    .iter_mut()
                    .find(|candidate| !candidate.used)
                    .unwrap_or_else(|| crate::fatal("MakOS transaction package VFS full"));
                *slot = file;
            }
        }
        Err(error) => {
            crate::package::record_runtime_error();
            crate::serial_println!(
                "MAKOS_PACKAGE_STORE_ERROR error={:?} activation=disabled",
                error
            );
        }
    }
}

fn transaction_package_files()
-> Result<alloc::vec::Vec<crate::vfs::MountedPackageFile>, makos_package_store::Error> {
    let Some(mut store) = package_transaction_store()? else {
        crate::package::record_runtime_legacy();
        return Ok(alloc::vec::Vec::new());
    };
    let mut packages = [makos_package_store::PackageInfo::EMPTY; makos_package_store::MAX_PACKAGES];
    let state = store.packages(&mut packages)?;
    let mut files = alloc::vec::Vec::with_capacity(state.package_count);
    for info in &packages[..state.package_count] {
        let mut file = crate::vfs::MountedPackageFile::EMPTY;
        let prefix = b"/packages/";
        let suffix = b"/payload";
        let path_length = prefix.len() + info.name().len() + suffix.len();
        if path_length > file.path.len() {
            return Err(makos_package_store::Error::Corrupt);
        }
        file.used = true;
        file.transaction = true;
        file.path[..prefix.len()].copy_from_slice(prefix);
        file.path[prefix.len()..prefix.len() + info.name().len()].copy_from_slice(info.name());
        file.path[prefix.len() + info.name().len()..path_length].copy_from_slice(suffix);
        file.path_length = path_length;
        file.size = info.payload_length;
        file.first_lba = info.payload_first_sector;
        file.sectors = info.payload_length.div_ceil(512);
        file.transaction_name[..info.name().len()].copy_from_slice(info.name());
        file.transaction_name_length = info.name_length;
        files.push(file);
    }
    crate::package::record_runtime_persistent(state.generation, state.package_count);
    crate::serial_println!(
        "MAKOS_PACKAGE_ACTIVATION_OK generation={} packages={} root=/packages backing=disk-ab",
        state.generation,
        state.package_count
    );
    Ok(files)
}

pub(crate) fn refresh_transaction_packages() -> bool {
    match transaction_package_files() {
        Ok(files) => {
            let count = files.len();
            let refreshed = crate::vfs::replace_transaction_packages(&files);
            crate::serial_println!(
                "MAKOS_PACKAGE_LIVE_REFRESH_OK result={} files={}",
                u8::from(refreshed),
                count,
            );
            refreshed
        }
        Err(error) => {
            crate::package::record_runtime_error();
            crate::serial_println!(
                "MAKOS_PACKAGE_STORE_ERROR error={:?} live_refresh=failed",
                error
            );
            false
        }
    }
}

pub fn read_package_file(
    file: &crate::vfs::MountedPackageFile,
    offset: u64,
    output: &mut [u8],
) -> Option<usize> {
    if !file.used || offset > file.size {
        crate::serial_println!(
            "MAKOS_PACKAGE_READ_REJECT used={} offset={:#x} size={:#x}",
            u8::from(file.used),
            offset,
            file.size
        );
        return None;
    }
    let count = output
        .len()
        .min(usize::try_from(file.size - offset).unwrap_or(usize::MAX));
    if count == 0 {
        return Some(0);
    }
    let Some(mut disk) = DataDisk::identify_secondary() else {
        crate::serial_println!("MAKOS_PACKAGE_READ_REJECT reason=data-disk-absent");
        return None;
    };
    let mut copied = 0usize;
    while copied < count {
        let absolute = offset.checked_add(copied as u64)?;
        let sector_index = absolute / 512;
        if sector_index >= file.sectors {
            crate::serial_println!(
                "MAKOS_PACKAGE_READ_REJECT reason=sector-bounds index={} sectors={} offset={:#x}",
                sector_index,
                file.sectors,
                offset
            );
            return None;
        }
        let lba = u32::try_from(file.first_lba.checked_add(sector_index)?).ok()?;
        if absolute % 512 == 0
            && count - copied >= 4096
            && sector_index.saturating_add(8) <= file.sectors
        {
            let mut block = [0u8; 4096];
            if !disk.read_sectors_8(lba, &mut block) {
                crate::serial_println!(
                    "MAKOS_PACKAGE_READ_REJECT reason=block-read-4k lba={} offset={:#x} copied={}",
                    lba,
                    offset,
                    copied
                );
                return None;
            }
            output[copied..copied + 4096].copy_from_slice(&block);
            copied += 4096;
            continue;
        }
        let mut sector = [0u8; 512];
        if !disk.read_sector(lba, &mut sector) {
            crate::serial_println!(
                "MAKOS_PACKAGE_READ_REJECT reason=block-read lba={} offset={:#x} copied={}",
                lba,
                offset,
                copied
            );
            return None;
        }
        let within = (absolute % 512) as usize;
        let chunk = (512 - within).min(count - copied);
        output[copied..copied + chunk].copy_from_slice(&sector[within..within + chunk]);
        copied += chunk;
    }
    Some(copied)
}

pub fn store_user_file(data: &[u8]) -> bool {
    if data.len() > 64 {
        return false;
    }
    let Some(mut disk) = DataDisk::identify_secondary() else {
        return false;
    };
    let mut record = [0u8; 512];
    record[0..8].copy_from_slice(b"MAKFILE1");
    record[8..12].copy_from_slice(&0o100600u32.to_le_bytes());
    record[12..16].copy_from_slice(&crate::security::INIT_UID.to_le_bytes());
    record[16..20].copy_from_slice(&crate::security::INIT_GID.to_le_bytes());
    record[20..24].copy_from_slice(&(data.len() as u32).to_le_bytes());
    record[24..24 + data.len()].copy_from_slice(data);
    let checksum = crc32(&record[..508]);
    record[508..512].copy_from_slice(&checksum.to_le_bytes());
    disk.write_sector(USER_FILE_LBA, &record)
}

pub fn store_dynamic_file(slot: usize, name: &[u8], data: Option<&[u8]>) -> bool {
    store_dynamic_node(slot, name, crate::vfs::KIND_FILE, data)
}

pub fn store_dynamic_directory(slot: usize, name: &[u8], active: bool) -> bool {
    store_dynamic_node(
        slot,
        name,
        crate::vfs::KIND_DIRECTORY,
        active.then_some(&[]),
    )
}

fn store_dynamic_node(slot: usize, name: &[u8], kind: u32, data: Option<&[u8]>) -> bool {
    if slot >= crate::vfs::DYNAMIC_FILE_COUNT
        || name.is_empty()
        || name.len() > crate::vfs::DYNAMIC_NAME_BYTES
        || !matches!(kind, crate::vfs::KIND_FILE | crate::vfs::KIND_DIRECTORY)
        || (kind == crate::vfs::KIND_DIRECTORY && data.is_some_and(|bytes| !bytes.is_empty()))
        || data.is_some_and(|bytes| bytes.len() > crate::vfs::MAX_FILE_BYTES)
    {
        return false;
    }
    let Some(mut disk) = DataDisk::identify_secondary() else {
        return false;
    };
    ensure_dynamic_area(&mut disk);
    store_dynamic_node_on_disk(&mut disk, slot, name, kind, data)
}

pub fn sync() -> bool {
    DataDisk::identify_secondary().is_some_and(|mut disk| disk.flush())
}

fn read_dynamic_file(disk: &mut DataDisk, slot: usize) -> crate::vfs::MountedDynamicFile {
    let Some(inode) = read_dynamic_inode(disk, slot) else {
        return crate::vfs::MountedDynamicFile::EMPTY;
    };
    if !inode.active {
        return crate::vfs::MountedDynamicFile::EMPTY;
    }
    let mut file = crate::vfs::MountedDynamicFile::EMPTY;
    let mut copied = 0usize;
    for block in &inode.blocks[..inode.block_count] {
        let mut sector = [0u8; 512];
        if !disk.read_sector(DYNAMIC_DATA_LBA_BASE + u32::from(*block), &mut sector) {
            crate::fatal("MakFS dynamic-file data read failed");
        }
        let count = (inode.data_length - copied).min(sector.len());
        file.data[copied..copied + count].copy_from_slice(&sector[..count]);
        copied += count;
    }
    if copied != inode.data_length || crc32(&file.data[..copied]) != inode.data_crc {
        crate::fatal("MakFS dynamic-file data checksum invalid");
    }
    file.used = true;
    file.kind = inode.kind;
    file.name = inode.name;
    file.name_length = inode.name_length;
    file.data_length = copied;
    file
}

#[derive(Clone, Copy)]
struct DynamicInode {
    active: bool,
    kind: u32,
    name: [u8; crate::vfs::DYNAMIC_NAME_BYTES],
    name_length: usize,
    data_length: usize,
    data_crc: u32,
    blocks: [u16; MAX_DYNAMIC_BLOCKS],
    block_count: usize,
}

impl DynamicInode {
    const EMPTY: Self = Self {
        active: false,
        kind: crate::vfs::KIND_FILE,
        name: [0; crate::vfs::DYNAMIC_NAME_BYTES],
        name_length: 0,
        data_length: 0,
        data_crc: 0,
        blocks: [0; MAX_DYNAMIC_BLOCKS],
        block_count: 0,
    };
}

fn ensure_dynamic_area(disk: &mut DataDisk) {
    if read_allocation_bitmap(disk).is_some() {
        return;
    }
    let mut v2_inode_present = false;
    for slot in 0..crate::vfs::DYNAMIC_FILE_COUNT {
        v2_inode_present |= read_dynamic_inode(disk, slot).is_some();
    }
    if v2_inode_present {
        if !write_allocation_bitmap(disk, &[0; DYNAMIC_BITMAP_BYTES]) {
            crate::fatal("MakFS allocation map recovery failed");
        }
        crate::serial_println!("MAKOS_MAKFS_BITMAP_RECOVERY_OK source=crc-inodes repaired=1");
        return;
    }
    const LEGACY_COUNT: usize = 4;
    let mut legacy = [crate::vfs::MountedDynamicFile::EMPTY; LEGACY_COUNT];
    let mut migrated = 0usize;
    for (slot, file) in legacy.iter_mut().enumerate() {
        *file = read_legacy_dynamic_file(disk, slot);
        migrated += usize::from(file.used);
    }
    if !write_allocation_bitmap(disk, &[0; DYNAMIC_BITMAP_BYTES]) {
        crate::fatal("MakFS dynamic allocation initialization failed");
    }
    for (slot, file) in legacy.iter().enumerate() {
        if file.used
            && !store_dynamic_node_on_disk(
                disk,
                slot,
                &file.name[..file.name_length],
                crate::vfs::KIND_FILE,
                Some(&file.data[..file.data_length]),
            )
        {
            crate::fatal("MakFS dynamic v1 migration failed");
        }
    }
    if migrated != 0 {
        crate::serial_println!(
            "MAKOS_MAKFS_MIGRATE_OK from=dynamic-v1 to=dynamic-v2 files={}",
            migrated
        );
    }
}

fn reconcile_dynamic_allocation(disk: &mut DataDisk) {
    let mut bitmap = [0u8; DYNAMIC_BITMAP_BYTES];
    for slot in 0..crate::vfs::DYNAMIC_FILE_COUNT {
        let Some(inode) = read_dynamic_inode(disk, slot) else {
            continue;
        };
        if !inode.active {
            continue;
        }
        for block in &inode.blocks[..inode.block_count] {
            let index = usize::from(*block);
            if allocation_bit(&bitmap, index) {
                crate::fatal("MakFS duplicate dynamic data block");
            }
            set_allocation_bit(&mut bitmap, index, true);
        }
    }
    if !write_allocation_bitmap(disk, &bitmap) {
        crate::fatal("MakFS allocation reconciliation failed");
    }
    crate::serial_println!(
        "MAKOS_MAKFS_ALLOCATOR_OK inodes={} blocks={} max_file={} bitmap=1 reconcile=1",
        crate::vfs::DYNAMIC_FILE_COUNT,
        DYNAMIC_DATA_BLOCK_COUNT,
        crate::vfs::MAX_FILE_BYTES
    );
}

fn store_dynamic_node_on_disk(
    disk: &mut DataDisk,
    slot: usize,
    name: &[u8],
    kind: u32,
    data: Option<&[u8]>,
) -> bool {
    let Some(mut bitmap) = read_allocation_bitmap(disk) else {
        return false;
    };
    let old = read_dynamic_inode(disk, slot).unwrap_or(DynamicInode::EMPTY);
    let bytes = data.unwrap_or(&[]);
    let required = if data.is_some() {
        bytes.len().div_ceil(512)
    } else {
        0
    };
    let mut blocks = [0u16; MAX_DYNAMIC_BLOCKS];
    let mut found = 0usize;
    for index in 0..DYNAMIC_DATA_BLOCK_COUNT {
        if found == required {
            break;
        }
        if !allocation_bit(&bitmap, index) {
            blocks[found] = index as u16;
            found += 1;
        }
    }
    if found != required {
        return false;
    }
    for (position, block) in blocks[..required].iter().enumerate() {
        let mut sector = [0u8; 512];
        let start = position * 512;
        let count = (bytes.len() - start).min(512);
        sector[..count].copy_from_slice(&bytes[start..start + count]);
        if !disk.write_sector(DYNAMIC_DATA_LBA_BASE + u32::from(*block), &sector) {
            return false;
        }
        set_allocation_bit(&mut bitmap, usize::from(*block), true);
    }
    if !write_allocation_bitmap(disk, &bitmap) {
        return false;
    }
    let mut inode = DynamicInode::EMPTY;
    inode.active = data.is_some();
    inode.kind = kind;
    inode.name[..name.len()].copy_from_slice(name);
    inode.name_length = name.len();
    inode.data_length = bytes.len();
    inode.data_crc = crc32(bytes);
    inode.blocks = blocks;
    inode.block_count = required;
    if !write_dynamic_inode(disk, slot, inode) {
        return false;
    }
    if old.active {
        for block in &old.blocks[..old.block_count] {
            set_allocation_bit(&mut bitmap, usize::from(*block), false);
        }
    }
    write_allocation_bitmap(disk, &bitmap)
}

fn read_dynamic_inode(disk: &mut DataDisk, slot: usize) -> Option<DynamicInode> {
    let mut record = [0u8; 512];
    if !disk.read_sector(DYNAMIC_INODE_LBA_BASE + slot as u32, &mut record) {
        return None;
    }
    let version = if record[0..8] == *b"MAKINOD2" {
        2
    } else if record[0..8] == *b"MAKINOD3" {
        3
    } else {
        return None;
    };
    let mode = read_u32(&record, 12).unwrap_or(0);
    let kind = match mode & 0o170000 {
        0o100000 => crate::vfs::KIND_FILE,
        0o040000 if version >= 3 => crate::vfs::KIND_DIRECTORY,
        _ => crate::fatal("MakFS dynamic inode kind invalid"),
    };
    if crc32(&record[..508]) != read_u32(&record, 508).unwrap_or(0)
        || record[9] as usize != slot
        || record[8] > 1
        || mode & 0o777
            != if kind == crate::vfs::KIND_FILE {
                0o600
            } else {
                0o700
            }
        || read_u32(&record, 16) != Some(crate::security::INIT_UID)
        || read_u32(&record, 20) != Some(crate::security::INIT_GID)
    {
        crate::fatal("MakFS dynamic inode metadata/checksum invalid");
    }
    let active = record[8] == 1;
    let name_length = record[10] as usize;
    let block_count = record[11] as usize;
    let data_length = read_u32(&record, 24).unwrap_or(u32::MAX) as usize;
    if name_length > crate::vfs::DYNAMIC_NAME_BYTES
        || block_count > MAX_DYNAMIC_BLOCKS
        || data_length > crate::vfs::MAX_FILE_BYTES
        || block_count != data_length.div_ceil(512)
        || (kind == crate::vfs::KIND_DIRECTORY && (block_count != 0 || data_length != 0))
        || (active && name_length == 0)
        || (!active && (block_count != 0 || data_length != 0))
    {
        crate::fatal("MakFS dynamic inode length invalid");
    }
    let mut inode = DynamicInode::EMPTY;
    inode.active = active;
    inode.kind = kind;
    inode.name[..name_length].copy_from_slice(&record[32..32 + name_length]);
    inode.name_length = name_length;
    inode.data_length = data_length;
    inode.data_crc = read_u32(&record, 28).unwrap_or(0);
    inode.block_count = block_count;
    for index in 0..block_count {
        let block = read_u16(&record, 64 + index * 2).unwrap_or(u16::MAX);
        if usize::from(block) >= DYNAMIC_DATA_BLOCK_COUNT || inode.blocks[..index].contains(&block)
        {
            crate::fatal("MakFS dynamic inode block invalid");
        }
        inode.blocks[index] = block;
    }
    Some(inode)
}

fn write_dynamic_inode(disk: &mut DataDisk, slot: usize, inode: DynamicInode) -> bool {
    let mut record = [0u8; 512];
    record[0..8].copy_from_slice(b"MAKINOD3");
    record[8] = u8::from(inode.active);
    record[9] = slot as u8;
    record[10] = inode.name_length as u8;
    record[11] = inode.block_count as u8;
    let mode = if inode.kind == crate::vfs::KIND_DIRECTORY {
        0o040700u32
    } else {
        0o100600u32
    };
    record[12..16].copy_from_slice(&mode.to_le_bytes());
    record[16..20].copy_from_slice(&crate::security::INIT_UID.to_le_bytes());
    record[20..24].copy_from_slice(&crate::security::INIT_GID.to_le_bytes());
    record[24..28].copy_from_slice(&(inode.data_length as u32).to_le_bytes());
    record[28..32].copy_from_slice(&inode.data_crc.to_le_bytes());
    record[32..32 + inode.name_length].copy_from_slice(&inode.name[..inode.name_length]);
    for (index, block) in inode.blocks[..inode.block_count].iter().enumerate() {
        record[64 + index * 2..66 + index * 2].copy_from_slice(&block.to_le_bytes());
    }
    let checksum = crc32(&record[..508]);
    record[508..512].copy_from_slice(&checksum.to_le_bytes());
    disk.write_sector(DYNAMIC_INODE_LBA_BASE + slot as u32, &record)
}

fn read_allocation_bitmap(disk: &mut DataDisk) -> Option<[u8; DYNAMIC_BITMAP_BYTES]> {
    let mut record = [0u8; 512];
    if !disk.read_sector(DYNAMIC_ALLOCATION_LBA, &mut record) || record[0..8] != *b"MAKALC02" {
        return None;
    }
    if crc32(&record[..508]) != read_u32(&record, 508)?
        || read_u32(&record, 8)? != 2
        || read_u16(&record, 12)? as usize != crate::vfs::DYNAMIC_FILE_COUNT
        || read_u16(&record, 14)? as usize != DYNAMIC_DATA_BLOCK_COUNT
    {
        return None;
    }
    let mut bitmap = [0u8; DYNAMIC_BITMAP_BYTES];
    bitmap.copy_from_slice(&record[16..16 + DYNAMIC_BITMAP_BYTES]);
    Some(bitmap)
}

fn write_allocation_bitmap(disk: &mut DataDisk, bitmap: &[u8; DYNAMIC_BITMAP_BYTES]) -> bool {
    let mut record = [0u8; 512];
    record[0..8].copy_from_slice(b"MAKALC02");
    record[8..12].copy_from_slice(&2u32.to_le_bytes());
    record[12..14].copy_from_slice(&(crate::vfs::DYNAMIC_FILE_COUNT as u16).to_le_bytes());
    record[14..16].copy_from_slice(&(DYNAMIC_DATA_BLOCK_COUNT as u16).to_le_bytes());
    record[16..16 + DYNAMIC_BITMAP_BYTES].copy_from_slice(bitmap);
    let checksum = crc32(&record[..508]);
    record[508..512].copy_from_slice(&checksum.to_le_bytes());
    disk.write_sector(DYNAMIC_ALLOCATION_LBA, &record)
}

fn allocation_bit(bitmap: &[u8; DYNAMIC_BITMAP_BYTES], index: usize) -> bool {
    bitmap[index / 8] & (1 << (index % 8)) != 0
}

fn set_allocation_bit(bitmap: &mut [u8; DYNAMIC_BITMAP_BYTES], index: usize, allocated: bool) {
    if allocated {
        bitmap[index / 8] |= 1 << (index % 8);
    } else {
        bitmap[index / 8] &= !(1 << (index % 8));
    }
}

fn read_legacy_dynamic_file(disk: &mut DataDisk, slot: usize) -> crate::vfs::MountedDynamicFile {
    let mut record = [0u8; 512];
    if !disk.read_sector(LEGACY_DYNAMIC_FILE_LBA_BASE + slot as u32, &mut record)
        || record[0..8] != *b"MAKDYN02"
    {
        return crate::vfs::MountedDynamicFile::EMPTY;
    }
    if crc32(&record[..508]) != read_u32(&record, 508).unwrap_or(0)
        || record[9] as usize != slot
        || record[8] > 1
        || read_u32(&record, 12) != Some(0o100600)
        || read_u32(&record, 16) != Some(crate::security::INIT_UID)
        || read_u32(&record, 20) != Some(crate::security::INIT_GID)
    {
        crate::fatal("MakFS dynamic-file metadata/checksum invalid");
    }
    let name_length = record[10] as usize;
    let data_length = read_u32(&record, 24).unwrap_or(u32::MAX) as usize;
    if name_length == 0 || name_length > crate::vfs::DYNAMIC_NAME_BYTES || data_length > 64 {
        crate::fatal("MakFS dynamic-file length invalid");
    }
    if record[8] == 0 {
        return crate::vfs::MountedDynamicFile::EMPTY;
    }
    let mut file = crate::vfs::MountedDynamicFile::EMPTY;
    file.used = true;
    file.name[..name_length].copy_from_slice(&record[32..32 + name_length]);
    file.name_length = name_length;
    file.data[..data_length].copy_from_slice(&record[64..64 + data_length]);
    file.data_length = data_length;
    file
}

fn read_user_file(disk: &mut DataDisk, output: &mut [u8; 64]) -> Option<usize> {
    let mut record = [0u8; 512];
    if !disk.read_sector(USER_FILE_LBA, &mut record) || record[0..8] != *b"MAKFILE1" {
        return None;
    }
    if crc32(&record[..508]) != read_u32(&record, 508)?
        || read_u32(&record, 8)? != 0o100600
        || read_u32(&record, 12)? != crate::security::INIT_UID
        || read_u32(&record, 16)? != crate::security::INIT_GID
    {
        crate::fatal("MakFS user-file metadata/checksum invalid");
    }
    let length = read_u32(&record, 20)? as usize;
    if length > output.len() {
        crate::fatal("MakFS user-file length invalid");
    }
    output[..length].copy_from_slice(&record[24..24 + length]);
    Some(length)
}

fn write_root_file(disk: &mut DataDisk, state: Superblock) -> &'static str {
    let text = match state.boot_count {
        1 => "MakOS persistent boot 1\n",
        2 => "MakOS persistent boot 2\n",
        _ => "MakOS persistent boot many\n",
    };
    let mut data = [0u8; 512];
    data[..text.len()].copy_from_slice(text.as_bytes());
    let data_crc = crc32(&data[..text.len()]);

    let mut inode = [0u8; 512];
    inode[0..8].copy_from_slice(&ROOT_MAGIC);
    inode[8..12].copy_from_slice(&1u32.to_le_bytes()); // one root entry
    inode[16] = FILE_NAME.len() as u8;
    inode[17..17 + FILE_NAME.len()].copy_from_slice(FILE_NAME);
    inode[64..68].copy_from_slice(&0o100644u32.to_le_bytes());
    inode[68..72].copy_from_slice(&0u32.to_le_bytes()); // uid
    inode[72..76].copy_from_slice(&0u32.to_le_bytes()); // gid
    inode[76..84].copy_from_slice(&(text.len() as u64).to_le_bytes());
    inode[84..92].copy_from_slice(&state.boot_count.to_le_bytes()); // monotonic timestamp
    inode[92..96].copy_from_slice(&FILE_DATA_LBA.to_le_bytes());
    inode[96..100].copy_from_slice(&data_crc.to_le_bytes());
    let inode_crc = crc32(&inode[..508]);
    inode[508..512].copy_from_slice(&inode_crc.to_le_bytes());

    if !disk.write_sector(FILE_DATA_LBA, &data) || !disk.write_sector(ROOT_DIRECTORY_LBA, &inode) {
        crate::fatal("MakFS root-file commit failed");
    }
    let mut read_inode = [0u8; 512];
    let mut read_data = [0u8; 512];
    if !disk.read_sector(ROOT_DIRECTORY_LBA, &mut read_inode)
        || !disk.read_sector(FILE_DATA_LBA, &mut read_data)
        || read_inode[0..8] != ROOT_MAGIC
        || crc32(&read_inode[..508]) != read_u32(&read_inode, 508).unwrap_or(0)
        || read_inode[17..17 + FILE_NAME.len()] != *FILE_NAME
        || crc32(&read_data[..text.len()]) != read_u32(&read_inode, 96).unwrap_or(0)
        || read_data[..text.len()] != *text.as_bytes()
    {
        crate::fatal("MakFS root-file readback failed");
    }
    text
}

fn update_self_test(disk: &mut DataDisk, state: Superblock) {
    const PACKAGE: &[u8] = b"makos-core|0.1.0|x86_64|kernel+init";
    let package_hash = crc32(PACKAGE);
    let active_slot = (state.generation & 1) as u8;
    let previous_slot = active_slot ^ 1;
    let mut record = [0u8; 512];
    record[0..8].copy_from_slice(b"MAKUPD01");
    record[8] = active_slot;
    record[9] = previous_slot;
    record[16..24].copy_from_slice(&state.generation.to_le_bytes());
    record[24..28].copy_from_slice(&package_hash.to_le_bytes());
    record[28..32].copy_from_slice(&(PACKAGE.len() as u32).to_le_bytes());
    record[32..32 + PACKAGE.len()].copy_from_slice(PACKAGE);
    let record_crc = crc32(&record[..508]);
    record[508..512].copy_from_slice(&record_crc.to_le_bytes());
    if !disk.write_sector(UPDATE_LBA, &record) {
        crate::fatal("update generation commit failed");
    }
    let mut verified = [0u8; 512];
    if !disk.read_sector(UPDATE_LBA, &mut verified)
        || verified[0..8] != *b"MAKUPD01"
        || crc32(&verified[..508]) != read_u32(&verified, 508).unwrap_or(0)
        || read_u32(&verified, 24) != Some(package_hash)
    {
        crate::fatal("update generation verification failed");
    }
    if !crate::security::file_access(0o100644, crate::security::ROOT_UID, 0, false)
        || crate::security::file_access(0o100644, crate::security::ROOT_UID, 0, true)
    {
        crate::fatal("filesystem permission enforcement failed");
    }
    crate::serial_println!(
        "MAKOS_UPDATE_OK package=makos-core version=0.1.0 hash={:#x} active_slot={} rollback_slot={} atomic=1 permissions=ok",
        package_hash,
        active_slot,
        previous_slot
    );
}

fn read_superblock(disk: &mut DataDisk, lba: u32) -> Option<Superblock> {
    let mut sector = [0u8; 512];
    if !disk.read_sector(lba, &mut sector) || sector[0..8] != MAGIC {
        return None;
    }
    if read_u32(&sector, 8)? != VERSION || checksum(&sector) != 0 {
        return None;
    }
    Some(Superblock {
        generation: read_u64(&sector, 16)?,
        boot_count: read_u64(&sector, 24)?,
    })
}

fn encode(state: Superblock) -> [u8; 512] {
    let mut sector = [0u8; 512];
    sector[0..8].copy_from_slice(&MAGIC);
    sector[8..12].copy_from_slice(&VERSION.to_le_bytes());
    sector[16..24].copy_from_slice(&state.generation.to_le_bytes());
    sector[24..32].copy_from_slice(&state.boot_count.to_le_bytes());
    let checksum = crc32(&sector[..508]);
    sector[508..512].copy_from_slice(&checksum.to_le_bytes());
    sector
}

fn checksum(sector: &[u8; 512]) -> u32 {
    let stored = read_u32(sector, 508).unwrap();
    stored ^ crc32(&sector[..508])
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}
