use makos_gpt::DiskLayout;
use makos_makfs4::{BLOCK_BYTES, Catalog, Inode, RECORD_BYTES, Superblock, newest_superblock};
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const SECTOR_BYTES: u64 = 512;
const SUPERBLOCK_LBAS: [u64; 2] = [112, 120];
const METADATA_FIRST_BLOCK: u64 = 16;
const METADATA_LIMIT_BLOCK: u64 = 256;
const DATA_START_BLOCK: u64 = 131_072;
const MAXIMUM_INODES: u32 = 512;
const INODE_TABLE_BLOCKS: u32 = 64;
const MAKOS_DATA_TYPE: [u8; 16] = [
    0x74, 0x8f, 0x6a, 0x8d, 0x33, 0x3e, 0x44, 0x4d, 0xa2, 0xe7, 0x0f, 0x5a, 0x4b, 0x4f, 0x53, 0x01,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Report {
    pub generation: u64,
    pub root_slot: usize,
    pub inodes: u32,
    pub files: u32,
    pub directories: u32,
    pub symlinks: u32,
    pub allocated_blocks: u64,
    pub volume_offset_bytes: u64,
}

pub fn check_path(path: impl AsRef<Path>) -> Result<Report, String> {
    let mut file = File::open(path.as_ref()).map_err(|error| error.to_string())?;
    let bytes = file.metadata().map_err(|error| error.to_string())?.len();
    if !bytes.is_multiple_of(SECTOR_BYTES) {
        return Err("disk length is not sector aligned".into());
    }
    let layout = makos_gpt::classify(bytes / SECTOR_BYTES, MAKOS_DATA_TYPE, |lba, output| {
        file.seek(SeekFrom::Start(lba.saturating_mul(SECTOR_BYTES)))
            .and_then(|_| file.read_exact(output))
            .is_ok()
    });
    match layout {
        DiskLayout::LegacyRaw { .. } => check_volume(&mut file, 0, bytes),
        DiskLayout::Gpt(partition) => {
            let offset = partition
                .first_lba
                .checked_mul(SECTOR_BYTES)
                .ok_or_else(|| "GPT partition offset overflow".to_string())?;
            let length = partition
                .sectors
                .checked_mul(SECTOR_BYTES)
                .ok_or_else(|| "GPT partition length overflow".to_string())?;
            check_volume(&mut file, offset, length)
        }
        DiskLayout::Invalid => Err("protective MBR has no valid MakOS GPT data partition".into()),
    }
}

pub fn check(reader: &mut (impl Read + Seek), bytes: u64) -> Result<Report, String> {
    check_volume(reader, 0, bytes)
}

fn check_volume(
    reader: &mut (impl Read + Seek),
    volume_offset: u64,
    bytes: u64,
) -> Result<Report, String> {
    if !bytes.is_multiple_of(BLOCK_BYTES) {
        return Err("volume length is not 4 KiB aligned".into());
    }
    let block_count = bytes / BLOCK_BYTES;
    let mut roots = [Err(makos_makfs4::Error::Corrupt); 2];
    for (slot, lba) in SUPERBLOCK_LBAS.iter().copied().enumerate() {
        let record = read_record(reader, volume_offset + lba * SECTOR_BYTES)?;
        roots[slot] = Superblock::decode(&record).and_then(|superblock| {
            validate_geometry(superblock, block_count)
                .map(|_| superblock)
                .map_err(|_| makos_makfs4::Error::Corrupt)
        });
    }
    let (superblock, root_slot) = newest_superblock(roots[0], roots[1])
        .map_err(|_| "both redundant superblocks are invalid".to_string())?;
    let catalog_record = read_record(
        reader,
        volume_offset + superblock.catalog_block * BLOCK_BYTES,
    )?;
    let catalog = Catalog::decode(&catalog_record, block_count, superblock.data_start)
        .map_err(|_| "active catalog is corrupt".to_string())?;
    validate_catalog_geometry(superblock, catalog)?;
    if catalog.generation != superblock.generation {
        return Err("catalog/root generation mismatch".into());
    }

    let mut inodes = vec![None; catalog.maximum_inodes as usize];
    for index in 0..catalog.maximum_inodes {
        let relative_offset = catalog
            .inode_table_block
            .checked_mul(BLOCK_BYTES)
            .and_then(|base| base.checked_add(u64::from(index) * RECORD_BYTES as u64))
            .ok_or_else(|| "inode table offset overflow".to_string())?;
        let offset = volume_offset
            .checked_add(relative_offset)
            .ok_or_else(|| "inode table offset overflow".to_string())?;
        let record = read_record(reader, offset)?;
        if record.iter().all(|byte| *byte == 0) {
            continue;
        }
        let inode = Inode::decode(&record)
            .and_then(|inode| {
                inode
                    .validate_on_volume(superblock.data_start, superblock.block_count)
                    .map(|_| inode)
            })
            .map_err(|_| format!("inode record {index} is corrupt"))?;
        if inode.inode != u64::from(index) + 1 || inode.generation > superblock.generation {
            return Err(format!(
                "inode record {index} has invalid identity/generation"
            ));
        }
        inodes[index as usize] = Some(inode);
    }
    if inodes.iter().flatten().count() != catalog.inode_count as usize {
        return Err("catalog inode count mismatch".into());
    }
    let root = inodes[0].ok_or_else(|| "root inode is missing".to_string())?;
    if root.inode != 1 || root.parent != 1 || root.mode & 0o170000 != 0o040000 {
        return Err("root inode identity/type is invalid".into());
    }

    let data_blocks = superblock.block_count - superblock.data_start;
    let mut expected_bitmap = vec![false; data_blocks as usize];
    let mut children = HashSet::new();
    let mut files = 0u32;
    let mut directories = 0u32;
    let mut symlinks = 0u32;
    for inode in inodes.iter().flatten().copied() {
        match inode.mode & 0o170000 {
            0o040000 => directories += 1,
            0o100000 => files += 1,
            0o120000 => symlinks += 1,
            _ => return Err(format!("inode {} has unsupported type", inode.inode)),
        }
        if inode.inode != 1 {
            let parent_index = inode
                .parent
                .checked_sub(1)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| format!("inode {} has invalid parent", inode.inode))?;
            let parent = inodes
                .get(parent_index)
                .copied()
                .flatten()
                .filter(|parent| parent.mode & 0o170000 == 0o040000)
                .ok_or_else(|| format!("inode {} parent is missing/not-directory", inode.inode))?;
            if !children.insert((parent.inode, inode.name().to_vec())) {
                return Err(format!("duplicate child name below inode {}", parent.inode));
            }
            validate_parent_chain(&inodes, inode)?;
        }
        for extent in inode.extents.extents() {
            let end = extent
                .end_block()
                .ok_or_else(|| format!("inode {} extent overflows", inode.inode))?;
            for block in extent.start_block..end {
                let relative = usize::try_from(block - superblock.data_start)
                    .map_err(|_| "extent index overflow".to_string())?;
                if expected_bitmap[relative] {
                    return Err(format!("data block {block} is multiply referenced"));
                }
                expected_bitmap[relative] = true;
            }
        }
    }

    let bitmap_bytes = read_blocks(
        reader,
        volume_offset,
        superblock.bitmap_block,
        superblock.bitmap_blocks,
    )?;
    for (relative, expected) in expected_bitmap.iter().copied().enumerate() {
        let actual = bitmap_bytes[relative / 8] & (1 << (relative % 8)) != 0;
        if actual != expected {
            return Err(format!(
                "bitmap mismatch at data block {}",
                superblock.data_start + relative as u64
            ));
        }
    }
    let used_bitmap_bytes = data_blocks.div_ceil(8) as usize;
    if !data_blocks.is_multiple_of(8) {
        let invalid_mask = !((1u8 << (data_blocks % 8)) - 1);
        if bitmap_bytes[used_bitmap_bytes - 1] & invalid_mask != 0 {
            return Err("bitmap has allocated bits beyond volume data range".into());
        }
    }
    if bitmap_bytes[used_bitmap_bytes..]
        .iter()
        .any(|byte| *byte != 0)
    {
        return Err("bitmap has allocated bits beyond volume data range".into());
    }

    Ok(Report {
        generation: superblock.generation,
        root_slot,
        inodes: catalog.inode_count,
        files,
        directories,
        symlinks,
        allocated_blocks: expected_bitmap.iter().filter(|value| **value).count() as u64,
        volume_offset_bytes: volume_offset,
    })
}

fn validate_parent_chain(inodes: &[Option<Inode>], inode: Inode) -> Result<(), String> {
    let mut current = inode;
    for _ in 0..inodes.len() {
        if current.inode == 1 {
            return Ok(());
        }
        let parent_index = current
            .parent
            .checked_sub(1)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| format!("inode {} has invalid parent chain", inode.inode))?;
        current = inodes
            .get(parent_index)
            .copied()
            .flatten()
            .ok_or_else(|| format!("inode {} has missing parent chain", inode.inode))?;
    }
    Err(format!(
        "inode {} parent chain contains a cycle",
        inode.inode
    ))
}

fn validate_geometry(superblock: Superblock, block_count: u64) -> Result<(), String> {
    let (bitmap_blocks, sets) = metadata_sets(block_count)?;
    if superblock.block_count != block_count
        || superblock.data_start != DATA_START_BLOCK
        || superblock.bitmap_blocks != bitmap_blocks
        || !sets.contains(&(superblock.bitmap_block, superblock.catalog_block))
    {
        return Err("superblock geometry mismatch".into());
    }
    Ok(())
}

fn validate_catalog_geometry(superblock: Superblock, catalog: Catalog) -> Result<(), String> {
    let (_, sets) = metadata_sets(superblock.block_count)?;
    if catalog.maximum_inodes != MAXIMUM_INODES
        || catalog.inode_table_blocks != INODE_TABLE_BLOCKS
        || !sets.iter().any(|(bitmap, catalog_block)| {
            *bitmap == superblock.bitmap_block
                && *catalog_block == superblock.catalog_block
                && catalog.inode_table_block == catalog_block + 1
        })
    {
        return Err("catalog metadata-set geometry mismatch".into());
    }
    Ok(())
}

fn metadata_sets(block_count: u64) -> Result<(u32, [(u64, u64); 3]), String> {
    if block_count <= DATA_START_BLOCK {
        return Err("volume has no data region".into());
    }
    let bitmap_blocks = (block_count - DATA_START_BLOCK)
        .div_ceil(8)
        .div_ceil(BLOCK_BYTES);
    let bitmap_blocks = u32::try_from(bitmap_blocks).map_err(|_| "bitmap too large")?;
    let set_blocks = u64::from(bitmap_blocks) + 1 + u64::from(INODE_TABLE_BLOCKS);
    let starts = [
        METADATA_FIRST_BLOCK,
        METADATA_FIRST_BLOCK + set_blocks,
        METADATA_FIRST_BLOCK + set_blocks * 2,
    ];
    if starts[2] + set_blocks > METADATA_LIMIT_BLOCK {
        return Err("metadata sets exceed reserved area".into());
    }
    Ok((
        bitmap_blocks,
        starts.map(|start| (start, start + u64::from(bitmap_blocks))),
    ))
}

fn read_record(reader: &mut (impl Read + Seek), offset: u64) -> Result<[u8; RECORD_BYTES], String> {
    let mut record = [0; RECORD_BYTES];
    reader
        .seek(SeekFrom::Start(offset))
        .and_then(|_| reader.read_exact(&mut record))
        .map_err(|error| error.to_string())?;
    Ok(record)
}

fn read_blocks(
    reader: &mut (impl Read + Seek),
    volume_offset: u64,
    block: u64,
    count: u32,
) -> Result<Vec<u8>, String> {
    let bytes = usize::try_from(u64::from(count) * BLOCK_BYTES)
        .map_err(|_| "metadata read too large".to_string())?;
    let mut output = vec![0; bytes];
    reader
        .seek(SeekFrom::Start(volume_offset + block * BLOCK_BYTES))
        .and_then(|_| reader.read_exact(&mut output))
        .map_err(|error| error.to_string())?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use makos_makfs4::{Extent, ExtentMap};
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    const BLOCK_COUNT: u64 = 262_144;

    fn image_path(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "makfs4-fsck-{label}-{}-{nonce}.img",
            std::process::id()
        ))
    }

    fn write_at(file: &mut File, offset: u64, bytes: &[u8]) {
        file.seek(SeekFrom::Start(offset)).unwrap();
        file.write_all(bytes).unwrap();
    }

    fn write_valid_volume(file: &mut File, base: u64) {
        let superblock = Superblock {
            generation: 3,
            commit_id: 3,
            block_count: BLOCK_COUNT,
            data_start: DATA_START_BLOCK,
            catalog_block: 20,
            bitmap_block: 16,
            bitmap_blocks: 4,
        };
        let root = {
            let mut inode = Inode::EMPTY;
            inode.inode = 1;
            inode.generation = 1;
            inode.mode = 0o040755;
            inode.parent = 1;
            inode.set_name(b".").unwrap();
            inode
        };
        let child = {
            let mut extents = ExtentMap::EMPTY;
            extents
                .push(Extent {
                    start_block: DATA_START_BLOCK,
                    block_count: 1,
                })
                .unwrap();
            let mut inode = Inode::EMPTY;
            inode.inode = 2;
            inode.generation = 2;
            inode.mode = 0o100600;
            inode.size = 4;
            inode.parent = 1;
            inode.extents = extents;
            inode.set_name(b"test.txt").unwrap();
            inode
        };
        let catalog = Catalog {
            generation: 3,
            inode_count: 2,
            maximum_inodes: MAXIMUM_INODES,
            inode_table_block: 21,
            inode_table_blocks: INODE_TABLE_BLOCKS,
        };
        let root_record = superblock.encode().unwrap();
        write_at(file, base + SUPERBLOCK_LBAS[0] * SECTOR_BYTES, &root_record);
        write_at(file, base + SUPERBLOCK_LBAS[1] * SECTOR_BYTES, &root_record);
        write_at(
            file,
            base + superblock.catalog_block * BLOCK_BYTES,
            &catalog.encode(BLOCK_COUNT, DATA_START_BLOCK).unwrap(),
        );
        write_at(file, base + 21 * BLOCK_BYTES, &root.encode().unwrap());
        write_at(
            file,
            base + 21 * BLOCK_BYTES + RECORD_BYTES as u64,
            &child.encode().unwrap(),
        );
        write_at(file, base + 16 * BLOCK_BYTES, &[1]);
    }

    fn valid_image(label: &str) -> std::path::PathBuf {
        let path = image_path(label);
        let mut file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.set_len(BLOCK_COUNT * BLOCK_BYTES).unwrap();
        write_valid_volume(&mut file, 0);
        path
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = u32::MAX;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
            }
        }
        !crc
    }

    fn gpt_header(
        current: u64,
        backup: u64,
        entries_lba: u64,
        last_usable: u64,
        entries_crc: u32,
    ) -> [u8; 512] {
        let mut header = [0; 512];
        header[..8].copy_from_slice(b"EFI PART");
        header[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        header[12..16].copy_from_slice(&92u32.to_le_bytes());
        header[24..32].copy_from_slice(&current.to_le_bytes());
        header[32..40].copy_from_slice(&backup.to_le_bytes());
        header[40..48].copy_from_slice(&34u64.to_le_bytes());
        header[48..56].copy_from_slice(&last_usable.to_le_bytes());
        header[72..80].copy_from_slice(&entries_lba.to_le_bytes());
        header[80..84].copy_from_slice(&128u32.to_le_bytes());
        header[84..88].copy_from_slice(&128u32.to_le_bytes());
        header[88..92].copy_from_slice(&entries_crc.to_le_bytes());
        let checksum = crc32(&header[..92]);
        header[16..20].copy_from_slice(&checksum.to_le_bytes());
        header
    }

    fn valid_gpt_image(label: &str) -> std::path::PathBuf {
        const DATA_FIRST_LBA: u64 = 2048;
        const ENTRY_SECTORS: u64 = 32;
        let volume_sectors = BLOCK_COUNT * BLOCK_BYTES / SECTOR_BYTES;
        let data_last_lba = DATA_FIRST_LBA + volume_sectors - 1;
        let device_sectors = data_last_lba + 35;
        let backup_header_lba = device_sectors - 1;
        let backup_entries_lba = backup_header_lba - ENTRY_SECTORS;
        let path = image_path(label);
        let mut file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.set_len(device_sectors * SECTOR_BYTES).unwrap();
        let mut mbr = [0; 512];
        mbr[450] = 0xee;
        mbr[510..512].copy_from_slice(&[0x55, 0xaa]);
        write_at(&mut file, 0, &mbr);
        let mut entries = vec![0u8; (ENTRY_SECTORS * SECTOR_BYTES) as usize];
        entries[..16].copy_from_slice(&MAKOS_DATA_TYPE);
        entries[32..40].copy_from_slice(&DATA_FIRST_LBA.to_le_bytes());
        entries[40..48].copy_from_slice(&data_last_lba.to_le_bytes());
        let entries_crc = crc32(&entries);
        write_at(&mut file, 2 * SECTOR_BYTES, &entries);
        write_at(&mut file, backup_entries_lba * SECTOR_BYTES, &entries);
        let primary = gpt_header(1, backup_header_lba, 2, data_last_lba, entries_crc);
        let backup = gpt_header(
            backup_header_lba,
            1,
            backup_entries_lba,
            data_last_lba,
            entries_crc,
        );
        write_at(&mut file, SECTOR_BYTES, &primary);
        write_at(&mut file, backup_header_lba * SECTOR_BYTES, &backup);
        write_valid_volume(&mut file, DATA_FIRST_LBA * SECTOR_BYTES);
        path
    }

    #[test]
    fn accepts_consistent_sparse_volume() {
        let path = valid_image("valid");
        let report = check_path(&path).unwrap();
        assert_eq!(report.generation, 3);
        assert_eq!(report.inodes, 2);
        assert_eq!(report.files, 1);
        assert_eq!(report.directories, 1);
        assert_eq!(report.allocated_blocks, 1);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn accepts_makos_partition_in_redundant_gpt() {
        let path = valid_gpt_image("gpt");
        let report = check_path(&path).unwrap();
        assert_eq!(report.volume_offset_bytes, 2048 * SECTOR_BYTES);
        assert_eq!(report.inodes, 2);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_bitmap_disagreement() {
        let path = valid_image("bitmap");
        let mut file = OpenOptions::new().write(true).open(&path).unwrap();
        write_at(&mut file, 16 * BLOCK_BYTES, &[0]);
        drop(file);
        assert!(check_path(&path).unwrap_err().contains("bitmap mismatch"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn falls_back_to_other_redundant_root() {
        let path = valid_image("root-fallback");
        let mut file = OpenOptions::new().write(true).open(&path).unwrap();
        write_at(&mut file, SUPERBLOCK_LBAS[1] * SECTOR_BYTES, &[0]);
        drop(file);
        assert_eq!(check_path(&path).unwrap().root_slot, 0);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_duplicate_child_names() {
        let path = valid_image("duplicate");
        let mut file = OpenOptions::new().write(true).open(&path).unwrap();
        let mut duplicate = Inode::EMPTY;
        duplicate.inode = 3;
        duplicate.generation = 3;
        duplicate.mode = 0o100600;
        duplicate.parent = 1;
        duplicate.set_name(b"test.txt").unwrap();
        write_at(
            &mut file,
            21 * BLOCK_BYTES + 2 * RECORD_BYTES as u64,
            &duplicate.encode().unwrap(),
        );
        let catalog = Catalog {
            generation: 3,
            inode_count: 3,
            maximum_inodes: MAXIMUM_INODES,
            inode_table_block: 21,
            inode_table_blocks: INODE_TABLE_BLOCKS,
        };
        write_at(
            &mut file,
            20 * BLOCK_BYTES,
            &catalog.encode(BLOCK_COUNT, DATA_START_BLOCK).unwrap(),
        );
        drop(file);
        assert!(check_path(&path).unwrap_err().contains("duplicate child"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_parent_cycles() {
        let path = valid_image("cycle");
        let mut file = OpenOptions::new().write(true).open(&path).unwrap();
        let mut cyclic = Inode::EMPTY;
        cyclic.inode = 2;
        cyclic.generation = 2;
        cyclic.mode = 0o040700;
        cyclic.parent = 2;
        cyclic.set_name(b"cycle").unwrap();
        write_at(
            &mut file,
            21 * BLOCK_BYTES + RECORD_BYTES as u64,
            &cyclic.encode().unwrap(),
        );
        drop(file);
        assert!(check_path(&path).unwrap_err().contains("cycle"));
        std::fs::remove_file(path).unwrap();
    }
}
