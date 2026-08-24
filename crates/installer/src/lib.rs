#![no_std]

const SECTOR_BYTES: usize = 512;
const BLOCK_SECTORS: u64 = 8;
const GPT_PRIMARY_SECTORS: u64 = 34;
const PROGRESS_SECTORS: u64 = 65_536;
const MAKOS_DATA_TYPE: [u8; 16] = [
    0x74, 0x8f, 0x6a, 0x8d, 0x33, 0x3e, 0x44, 0x4d, 0xa2, 0xe7, 0x0f, 0x5a, 0x4b, 0x4f, 0x53, 0x01,
];

pub trait Disk {
    fn sectors(&self) -> u64;
    fn read_sector(&mut self, lba: u32, output: &mut [u8; SECTOR_BYTES]) -> bool;
    fn write_sector(&mut self, lba: u32, input: &[u8; SECTOR_BYTES]) -> bool;
    fn flush(&mut self) -> bool;

    fn read_sectors_8(&mut self, lba: u32, output: &mut [u8; 4096]) -> bool {
        for sector in 0..8usize {
            let Some(sector_lba) = lba.checked_add(sector as u32) else {
                return false;
            };
            let Some(sector_output) = output
                .get_mut(sector * SECTOR_BYTES..(sector + 1) * SECTOR_BYTES)
                .and_then(|bytes| bytes.try_into().ok())
            else {
                return false;
            };
            if !self.read_sector(sector_lba, sector_output) {
                return false;
            }
        }
        true
    }

    fn write_sectors_8(&mut self, lba: u32, input: &[u8; 4096]) -> bool {
        for sector in 0..8usize {
            let Some(sector_lba) = lba.checked_add(sector as u32) else {
                return false;
            };
            let Some(sector_input) = input
                .get(sector * SECTOR_BYTES..(sector + 1) * SECTOR_BYTES)
                .and_then(|bytes| bytes.try_into().ok())
            else {
                return false;
            };
            if !self.write_sector(sector_lba, sector_input) {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallMode {
    Fresh,
    Resume,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallError {
    Geometry,
    SourceNotGpt,
    TargetNotBlank,
    ResumeCommitted,
    ResumeConflict,
    Read,
    Write,
    Verify,
    Flush,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstallReport {
    pub sectors: u64,
    pub written_sectors: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallEvent {
    Ready { sectors: u64 },
    Progress { copied_sectors: u64, sectors: u64 },
}

pub fn install<S, T, F>(
    source: &mut S,
    target: &mut T,
    mode: InstallMode,
    mut event: F,
) -> Result<InstallReport, InstallError>
where
    S: Disk,
    T: Disk,
    F: FnMut(InstallEvent),
{
    let sectors = source.sectors();
    if sectors != target.sectors() || sectors < 4096 || sectors > u64::from(u32::MAX) {
        return Err(InstallError::Geometry);
    }
    validate_source(source, sectors)?;
    match mode {
        InstallMode::Fresh => ensure_blank(target, sectors)?,
        InstallMode::Resume => ensure_resumable(source, target, sectors)?,
    }
    event(InstallEvent::Ready { sectors });

    let mut written_sectors = 0u64;
    let mut reported_initial_progress = false;
    let mut source_block = [0u8; 4096];
    let mut verify_block = [0u8; 4096];
    let mut lba = GPT_PRIMARY_SECTORS;
    while lba + BLOCK_SECTORS <= sectors {
        let lba32 = u32::try_from(lba).map_err(|_| InstallError::Geometry)?;
        if !source.read_sectors_8(lba32, &mut source_block) {
            return Err(InstallError::Read);
        }
        if source_block.iter().any(|byte| *byte != 0) {
            if !target.write_sectors_8(lba32, &source_block) {
                return Err(InstallError::Write);
            }
            if !target.read_sectors_8(lba32, &mut verify_block) || verify_block != source_block {
                return Err(InstallError::Verify);
            }
            written_sectors += BLOCK_SECTORS;
            if !reported_initial_progress {
                event(InstallEvent::Progress {
                    copied_sectors: lba + BLOCK_SECTORS,
                    sectors,
                });
                reported_initial_progress = true;
            }
        }
        lba += BLOCK_SECTORS;
        if lba % PROGRESS_SECTORS == GPT_PRIMARY_SECTORS % BLOCK_SECTORS {
            event(InstallEvent::Progress {
                copied_sectors: lba,
                sectors,
            });
        }
    }
    while lba < sectors {
        written_sectors += copy_sector_verified(source, target, lba)?;
        lba += 1;
    }

    for lba in 1..GPT_PRIMARY_SECTORS {
        written_sectors += copy_sector_verified(source, target, lba)?;
    }
    if !target.flush() {
        return Err(InstallError::Flush);
    }
    written_sectors += copy_sector_verified(source, target, 0)?;
    if !target.flush() {
        return Err(InstallError::Flush);
    }
    validate_target_commit(source, target, sectors)?;
    Ok(InstallReport {
        sectors,
        written_sectors,
    })
}

fn validate_source<D: Disk>(source: &mut D, sectors: u64) -> Result<(), InstallError> {
    match makos_gpt::classify(sectors, MAKOS_DATA_TYPE, |lba, output| {
        u32::try_from(lba)
            .ok()
            .is_some_and(|lba| source.read_sector(lba, output))
    }) {
        makos_gpt::DiskLayout::Gpt(_) => Ok(()),
        _ => Err(InstallError::SourceNotGpt),
    }
}

fn ensure_blank<D: Disk>(target: &mut D, sectors: u64) -> Result<(), InstallError> {
    let mut block = [0u8; 4096];
    let mut lba = 0u64;
    while lba + BLOCK_SECTORS <= sectors {
        if !target.read_sectors_8(
            u32::try_from(lba).map_err(|_| InstallError::Geometry)?,
            &mut block,
        ) {
            return Err(InstallError::Read);
        }
        if block.iter().any(|byte| *byte != 0) {
            return Err(InstallError::TargetNotBlank);
        }
        lba += BLOCK_SECTORS;
    }
    let mut sector = [0u8; SECTOR_BYTES];
    while lba < sectors {
        if !target.read_sector(
            u32::try_from(lba).map_err(|_| InstallError::Geometry)?,
            &mut sector,
        ) {
            return Err(InstallError::Read);
        }
        if sector.iter().any(|byte| *byte != 0) {
            return Err(InstallError::TargetNotBlank);
        }
        lba += 1;
    }
    Ok(())
}

fn ensure_resumable<S: Disk, T: Disk>(
    source: &mut S,
    target: &mut T,
    sectors: u64,
) -> Result<(), InstallError> {
    let mut source_sector = [0u8; SECTOR_BYTES];
    let mut target_sector = [0u8; SECTOR_BYTES];
    if !target.read_sector(0, &mut target_sector) {
        return Err(InstallError::Read);
    }
    if target_sector.iter().any(|byte| *byte != 0) {
        return Err(InstallError::ResumeCommitted);
    }
    for lba in 1..sectors {
        let lba = u32::try_from(lba).map_err(|_| InstallError::Geometry)?;
        if !target.read_sector(lba, &mut target_sector) {
            return Err(InstallError::Read);
        }
        if target_sector.iter().all(|byte| *byte == 0) {
            continue;
        }
        if !source.read_sector(lba, &mut source_sector) {
            return Err(InstallError::Read);
        }
        if target_sector != source_sector {
            return Err(InstallError::ResumeConflict);
        }
    }
    Ok(())
}

fn copy_sector_verified<S: Disk, T: Disk>(
    source: &mut S,
    target: &mut T,
    lba: u64,
) -> Result<u64, InstallError> {
    let lba = u32::try_from(lba).map_err(|_| InstallError::Geometry)?;
    let mut source_sector = [0u8; SECTOR_BYTES];
    let mut verify = [0u8; SECTOR_BYTES];
    if !source.read_sector(lba, &mut source_sector) {
        return Err(InstallError::Read);
    }
    if source_sector.iter().all(|byte| *byte == 0) {
        return Ok(0);
    }
    if !target.write_sector(lba, &source_sector) {
        return Err(InstallError::Write);
    }
    if !target.read_sector(lba, &mut verify) || verify != source_sector {
        return Err(InstallError::Verify);
    }
    Ok(1)
}

fn validate_target_commit<S: Disk, T: Disk>(
    source: &mut S,
    target: &mut T,
    sectors: u64,
) -> Result<(), InstallError> {
    let probes = [0, 1, 2, 33, 2048, sectors.saturating_sub(1)];
    let mut source_sector = [0u8; SECTOR_BYTES];
    let mut target_sector = [0u8; SECTOR_BYTES];
    for lba in probes {
        let lba = u32::try_from(lba).map_err(|_| InstallError::Geometry)?;
        if !source.read_sector(lba, &mut source_sector)
            || !target.read_sector(lba, &mut target_sector)
        {
            return Err(InstallError::Read);
        }
        if source_sector != target_sector {
            return Err(InstallError::Verify);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::{Disk, InstallError, InstallMode, install};
    use std::vec;
    use std::vec::Vec;

    struct MemoryDisk {
        data: Vec<[u8; 512]>,
        writes: Vec<u32>,
        flushes: usize,
        fail_write: Option<u32>,
    }

    impl MemoryDisk {
        fn blank(sectors: usize) -> Self {
            Self {
                data: vec![[0; 512]; sectors],
                writes: Vec::new(),
                flushes: 0,
                fail_write: None,
            }
        }
    }

    impl Disk for MemoryDisk {
        fn sectors(&self) -> u64 {
            self.data.len() as u64
        }

        fn read_sector(&mut self, lba: u32, output: &mut [u8; 512]) -> bool {
            let Some(source) = self.data.get(lba as usize) else {
                return false;
            };
            *output = *source;
            true
        }

        fn write_sector(&mut self, lba: u32, input: &[u8; 512]) -> bool {
            if self.fail_write == Some(lba) {
                return false;
            }
            let Some(target) = self.data.get_mut(lba as usize) else {
                return false;
            };
            *target = *input;
            self.writes.push(lba);
            true
        }

        fn flush(&mut self) -> bool {
            self.flushes += 1;
            true
        }
    }

    fn source() -> MemoryDisk {
        let sectors = 4096usize;
        let mut disk = MemoryDisk::blank(sectors);
        disk.data[0][450] = 0xee;
        disk.data[0][510..512].copy_from_slice(&[0x55, 0xaa]);
        let mut entries = [0u8; 512];
        entries[..16].copy_from_slice(&[
            0x74, 0x8f, 0x6a, 0x8d, 0x33, 0x3e, 0x44, 0x4d, 0xa2, 0xe7, 0x0f, 0x5a, 0x4b, 0x4f,
            0x53, 0x01,
        ]);
        entries[32..40].copy_from_slice(&2048u64.to_le_bytes());
        entries[40..48].copy_from_slice(&3071u64.to_le_bytes());
        let entries_crc = crc32(&entries);
        disk.data[2] = entries;
        disk.data[sectors - 2] = entries;
        disk.data[1] = header(1, (sectors - 1) as u64, 2, entries_crc);
        disk.data[sectors - 1] = header((sectors - 1) as u64, 1, (sectors - 2) as u64, entries_crc);
        disk.data[2048][..4].copy_from_slice(b"ESP!");
        disk.data[3000][..5].copy_from_slice(b"data!");
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

    #[test]
    fn fresh_install_commits_mbr_last() {
        let mut source = source();
        let mut target = MemoryDisk::blank(4096);
        let report = install(&mut source, &mut target, InstallMode::Fresh, |_| {}).unwrap();
        assert_eq!(report.sectors, 4096);
        assert_eq!(target.data, source.data);
        assert_eq!(target.writes.last(), Some(&0));
        assert_eq!(target.flushes, 2);
    }

    #[test]
    fn fresh_refuses_nonblank_before_write() {
        let mut source_disk = source();
        let mut target = MemoryDisk::blank(4096);
        target.data[3000][0] = 1;
        assert_eq!(
            install(&mut source_disk, &mut target, InstallMode::Fresh, |_| {}),
            Err(InstallError::TargetNotBlank)
        );
        assert!(target.writes.is_empty());

        let mut trailing_source = source();
        trailing_source.data.push([0; 512]);
        let mut trailing_target = MemoryDisk::blank(4097);
        trailing_target.data[4096][0] = 1;
        assert_eq!(
            install(
                &mut trailing_source,
                &mut trailing_target,
                InstallMode::Fresh,
                |_| {}
            ),
            Err(InstallError::TargetNotBlank)
        );
        assert!(trailing_target.writes.is_empty());
    }

    #[test]
    fn resume_accepts_matching_partial_copy() {
        let mut source = source();
        let mut target = MemoryDisk::blank(4096);
        target.data[3000] = source.data[3000];
        let report = install(&mut source, &mut target, InstallMode::Resume, |_| {}).unwrap();
        assert!(report.written_sectors > 0);
        assert_eq!(target.data, source.data);
        assert_eq!(target.writes.last(), Some(&0));
    }

    #[test]
    fn resume_refuses_committed_or_conflicting_target() {
        let mut source = source();
        let mut committed = MemoryDisk::blank(4096);
        committed.data[0][0] = 1;
        assert_eq!(
            install(&mut source, &mut committed, InstallMode::Resume, |_| {}),
            Err(InstallError::ResumeCommitted)
        );
        let mut conflicting = MemoryDisk::blank(4096);
        conflicting.data[3000][0] = 1;
        assert_eq!(
            install(&mut source, &mut conflicting, InstallMode::Resume, |_| {}),
            Err(InstallError::ResumeConflict)
        );
        assert!(committed.writes.is_empty() && conflicting.writes.is_empty());
    }

    #[test]
    fn geometry_and_source_gpt_fail_closed() {
        let mut source = source();
        let mut short = MemoryDisk::blank(4095);
        assert_eq!(
            install(&mut source, &mut short, InstallMode::Fresh, |_| {}),
            Err(InstallError::Geometry)
        );
        let mut invalid = MemoryDisk::blank(4096);
        let mut target = MemoryDisk::blank(4096);
        assert_eq!(
            install(&mut invalid, &mut target, InstallMode::Fresh, |_| {}),
            Err(InstallError::SourceNotGpt)
        );
        assert!(target.writes.is_empty());
    }

    #[test]
    fn interrupted_copy_leaves_mbr_uncommitted() {
        let mut source = source();
        let mut target = MemoryDisk::blank(4096);
        target.fail_write = Some(2048);
        assert_eq!(
            install(&mut source, &mut target, InstallMode::Fresh, |_| {}),
            Err(InstallError::Write)
        );
        assert!(target.data[0].iter().all(|byte| *byte == 0));
    }
}
