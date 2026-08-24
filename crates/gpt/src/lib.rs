#![no_std]

const SECTOR_BYTES: u64 = 512;
const ENTRY_LIMIT: u32 = 128;
const ENTRY_BYTES: u32 = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Partition {
    pub first_lba: u64,
    pub sectors: u64,
}

impl Partition {
    pub fn translate(self, logical_lba: u64, sector_count: u64) -> Option<u64> {
        (sector_count != 0 && logical_lba.checked_add(sector_count)? <= self.sectors)
            .then_some(self.first_lba.checked_add(logical_lba)?)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiskLayout {
    LegacyRaw { sectors: u64 },
    Gpt(Partition),
    Invalid,
}

/// Locate one GPT partition by its on-disk, little-endian type GUID.
///
/// Disks without a protective MBR retain legacy raw-disk behavior. Once a
/// protective MBR is present, invalid primary and backup GPT metadata is a
/// hard failure and must never fall back to raw writes.
pub fn classify<F>(device_sectors: u64, partition_type: [u8; 16], mut read_sector: F) -> DiskLayout
where
    F: FnMut(u64, &mut [u8; 512]) -> bool,
{
    if device_sectors == 0 {
        return DiskLayout::Invalid;
    }
    let mut sector = [0u8; 512];
    if !read_sector(0, &mut sector) {
        return DiskLayout::Invalid;
    }
    let protective = sector[510..512] == [0x55, 0xaa] && sector[450] == 0xee;
    if !protective {
        return DiskLayout::LegacyRaw {
            sectors: device_sectors,
        };
    }
    find_partition(device_sectors, 1, partition_type, &mut read_sector)
        .or_else(|| {
            find_partition(
                device_sectors,
                device_sectors.checked_sub(1)?,
                partition_type,
                &mut read_sector,
            )
        })
        .map_or(DiskLayout::Invalid, DiskLayout::Gpt)
}

fn find_partition<F>(
    device_sectors: u64,
    header_lba: u64,
    partition_type: [u8; 16],
    read_sector: &mut F,
) -> Option<Partition>
where
    F: FnMut(u64, &mut [u8; 512]) -> bool,
{
    let mut header = [0u8; 512];
    read_sector(header_lba, &mut header).then_some(())?;
    if &header[..8] != b"EFI PART" || read_u32(&header, 8)? != 0x0001_0000 {
        return None;
    }
    let header_bytes = usize::try_from(read_u32(&header, 12)?).ok()?;
    if !(92..=512).contains(&header_bytes)
        || read_u64(&header, 24)? != header_lba
        || read_u64(&header, 32)? >= device_sectors
    {
        return None;
    }
    let wanted_header_crc = read_u32(&header, 16)?;
    header[16..20].fill(0);
    if crc32(&header[..header_bytes]) != wanted_header_crc {
        return None;
    }
    let first_usable = read_u64(&header, 40)?;
    let last_usable = read_u64(&header, 48)?;
    let entries_lba = read_u64(&header, 72)?;
    let entry_count = read_u32(&header, 80)?;
    let entry_bytes = read_u32(&header, 84)?;
    let wanted_entries_crc = read_u32(&header, 88)?;
    if entry_count == 0
        || entry_count > ENTRY_LIMIT
        || entry_bytes != ENTRY_BYTES
        || first_usable > last_usable
        || last_usable >= device_sectors
    {
        return None;
    }
    let array_bytes = u64::from(entry_count).checked_mul(u64::from(entry_bytes))?;
    let array_sectors = array_bytes.div_ceil(SECTOR_BYTES);
    if entries_lba.checked_add(array_sectors)? > device_sectors {
        return None;
    }
    let mut crc = u32::MAX;
    let mut remaining = usize::try_from(array_bytes).ok()?;
    let mut sector = [0u8; 512];
    for offset in 0..array_sectors {
        read_sector(entries_lba.checked_add(offset)?, &mut sector).then_some(())?;
        let count = remaining.min(512);
        crc = crc32_update(crc, &sector[..count]);
        remaining -= count;
    }
    if !crc != wanted_entries_crc {
        return None;
    }
    for index in 0..entry_count {
        let byte_offset = u64::from(index) * u64::from(entry_bytes);
        let lba = entries_lba.checked_add(byte_offset / SECTOR_BYTES)?;
        read_sector(lba, &mut sector).then_some(())?;
        let offset = usize::try_from(byte_offset % SECTOR_BYTES).ok()?;
        let entry = sector.get(offset..offset + ENTRY_BYTES as usize)?;
        if entry[..16] != partition_type {
            continue;
        }
        let first = read_u64(entry, 32)?;
        let last = read_u64(entry, 40)?;
        let sectors = last.checked_sub(first)?.checked_add(1)?;
        if first < first_usable || last > last_usable || sectors == 0 {
            return None;
        }
        return Some(Partition {
            first_lba: first,
            sectors,
        });
    }
    None
}

fn crc32(bytes: &[u8]) -> u32 {
    !crc32_update(u32::MAX, bytes)
}

fn crc32_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    crc
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::{DiskLayout, Partition, classify, crc32};
    use std::vec;
    use std::vec::Vec;

    const TYPE: [u8; 16] = [
        0x74, 0x8f, 0x6a, 0x8d, 0x33, 0x3e, 0x44, 0x4d, 0xa2, 0xe7, 0x0f, 0x5a, 0x4b, 0x4f, 0x53,
        0x01,
    ];

    fn image() -> Vec<[u8; 512]> {
        let sectors = 4096usize;
        let mut disk = vec![[0u8; 512]; sectors];
        disk[0][450] = 0xee;
        disk[0][510..512].copy_from_slice(&[0x55, 0xaa]);
        let mut entries = [0u8; 512];
        entries[..16].copy_from_slice(&TYPE);
        entries[32..40].copy_from_slice(&2048u64.to_le_bytes());
        entries[40..48].copy_from_slice(&3071u64.to_le_bytes());
        let entries_crc = crc32(&entries);
        disk[2] = entries;
        disk[sectors - 2] = entries;
        disk[1] = header(1, (sectors - 1) as u64, 2, entries_crc);
        disk[sectors - 1] = header((sectors - 1) as u64, 1, (sectors - 2) as u64, entries_crc);
        disk
    }

    fn header(current: u64, backup: u64, entries_lba: u64, entries_crc: u32) -> [u8; 512] {
        let mut value = [0u8; 512];
        value[..8].copy_from_slice(b"EFI PART");
        value[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        value[12..16].copy_from_slice(&92u32.to_le_bytes());
        value[24..32].copy_from_slice(&current.to_le_bytes());
        value[32..40].copy_from_slice(&backup.to_le_bytes());
        value[40..48].copy_from_slice(&34u64.to_le_bytes());
        value[48..56].copy_from_slice(&4062u64.to_le_bytes());
        value[72..80].copy_from_slice(&entries_lba.to_le_bytes());
        value[80..84].copy_from_slice(&4u32.to_le_bytes());
        value[84..88].copy_from_slice(&128u32.to_le_bytes());
        value[88..92].copy_from_slice(&entries_crc.to_le_bytes());
        let checksum = crc32(&value[..92]);
        value[16..20].copy_from_slice(&checksum.to_le_bytes());
        value
    }

    fn classify_image(disk: &[[u8; 512]]) -> DiskLayout {
        classify(disk.len() as u64, TYPE, |lba, output| {
            let Some(source) = usize::try_from(lba).ok().and_then(|index| disk.get(index)) else {
                return false;
            };
            *output = *source;
            true
        })
    }

    #[test]
    fn accepts_primary_and_backup_recovery() {
        let mut disk = image();
        assert_eq!(
            classify_image(&disk),
            DiskLayout::Gpt(Partition {
                first_lba: 2048,
                sectors: 1024
            })
        );
        disk[1][16] ^= 0x80;
        assert!(matches!(classify_image(&disk), DiskLayout::Gpt(_)));
    }

    #[test]
    fn protective_corruption_never_falls_back_to_raw() {
        let mut disk = image();
        disk[1][16] ^= 0x80;
        let last = disk.len() - 1;
        disk[last][16] ^= 0x80;
        assert_eq!(classify_image(&disk), DiskLayout::Invalid);
    }

    #[test]
    fn non_protective_disk_is_legacy_raw() {
        let disk = vec![[0u8; 512]; 64];
        assert_eq!(classify_image(&disk), DiskLayout::LegacyRaw { sectors: 64 });
    }

    #[test]
    fn partition_translation_rejects_end_and_overflow() {
        let partition = Partition {
            first_lba: 2048,
            sectors: 1024,
        };
        assert_eq!(partition.translate(0, 1), Some(2048));
        assert_eq!(partition.translate(1016, 8), Some(3064));
        assert_eq!(partition.translate(1024, 1), None);
        assert_eq!(partition.translate(1023, 2), None);
        assert_eq!(partition.translate(u64::MAX, 1), None);
        assert_eq!(partition.translate(0, 0), None);
    }
}
