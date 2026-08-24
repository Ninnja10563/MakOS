use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use makos_gpt::DiskLayout;

const MAKOS_DATA_TYPE: [u8; 16] = [
    0x74, 0x8f, 0x6a, 0x8d, 0x33, 0x3e, 0x44, 0x4d, 0xa2, 0xe7, 0x0f, 0x5a, 0x4b, 0x4f, 0x53, 0x01,
];

// 0=uninitialized, 1=invalid GPT, 2=legacy raw disk, 3=MakOS GPT partition.
static GEOMETRY_KIND: AtomicU64 = AtomicU64::new(0);
static GEOMETRY_START: AtomicU64 = AtomicU64::new(0);
static GEOMETRY_SECTORS: AtomicU64 = AtomicU64::new(0);
static GEOMETRY_REPORTED: AtomicBool = AtomicBool::new(false);

pub struct DataDisk {
    #[cfg(target_arch = "x86_64")]
    device: crate::drivers::ata::AtaDisk,
    start_lba: u64,
    sectors: u64,
}

impl DataDisk {
    pub fn identify_secondary() -> Option<Self> {
        #[cfg(target_arch = "x86_64")]
        {
            let mut device = crate::drivers::ata::AtaDisk::identify_secondary()?;
            let device_sectors = device.sectors();
            let (start_lba, sectors, partitioned) =
                data_geometry(device_sectors, |lba, buffer| {
                    u32::try_from(lba)
                        .ok()
                        .is_some_and(|physical| device.read_sector(physical, buffer))
                })?;
            report_geometry(start_lba, sectors, partitioned);
            Some(Self {
                device,
                start_lba,
                sectors,
            })
        }
        #[cfg(target_arch = "aarch64")]
        {
            let device_sectors = crate::aarch64_virtio_blk::sectors()?;
            let (start_lba, sectors, partitioned) =
                data_geometry(device_sectors, |lba, buffer| {
                    u32::try_from(lba).ok().is_some_and(|physical| {
                        crate::aarch64_virtio_blk::read_sector(physical, buffer)
                    })
                })?;
            report_geometry(start_lba, sectors, partitioned);
            Some(Self { start_lba, sectors })
        }
    }

    pub const fn sectors(&self) -> u64 {
        self.sectors
    }

    pub fn read_sector(&mut self, lba: u32, buffer: &mut [u8; 512]) -> bool {
        let Some(physical) = self.physical_lba(u64::from(lba), 1) else {
            return false;
        };
        #[cfg(target_arch = "x86_64")]
        return self.device.read_sector(physical, buffer);
        #[cfg(target_arch = "aarch64")]
        return crate::aarch64_virtio_blk::read_sector(physical, buffer);
    }

    pub fn read_sectors_8(&mut self, lba: u32, buffer: &mut [u8; 4096]) -> bool {
        let Some(physical) = self.physical_lba(u64::from(lba), 8) else {
            return false;
        };
        #[cfg(target_arch = "x86_64")]
        return self.device.read_sectors_8(physical, buffer);
        #[cfg(target_arch = "aarch64")]
        return crate::aarch64_virtio_blk::read_sectors_8(physical, buffer);
    }

    pub fn write_sectors_8(&mut self, lba: u32, buffer: &[u8; 4096]) -> bool {
        let Some(physical) = self.physical_lba(u64::from(lba), 8) else {
            return false;
        };
        #[cfg(target_arch = "x86_64")]
        {
            for sector in 0..8usize {
                let mut bytes = [0u8; 512];
                bytes.copy_from_slice(&buffer[sector * 512..(sector + 1) * 512]);
                let Some(sector_lba) = physical.checked_add(sector as u32) else {
                    return false;
                };
                if !self.device.write_sector(sector_lba, &bytes) {
                    return false;
                }
            }
            true
        }
        #[cfg(target_arch = "aarch64")]
        return crate::aarch64_virtio_blk::write_sectors_8_on(0, physical, buffer);
    }

    pub fn write_sector(&mut self, lba: u32, buffer: &[u8; 512]) -> bool {
        let Some(physical) = self.physical_lba(u64::from(lba), 1) else {
            return false;
        };
        #[cfg(target_arch = "x86_64")]
        return self.device.write_sector(physical, buffer);
        #[cfg(target_arch = "aarch64")]
        return crate::aarch64_virtio_blk::write_sector(physical, buffer);
    }

    pub fn flush(&mut self) -> bool {
        #[cfg(target_arch = "x86_64")]
        return self.device.flush();
        #[cfg(target_arch = "aarch64")]
        return crate::aarch64_virtio_blk::flush();
    }

    fn physical_lba(&self, logical: u64, count: u64) -> Option<u32> {
        makos_gpt::Partition {
            first_lba: self.start_lba,
            sectors: self.sectors,
        }
        .translate(logical, count)
        .and_then(|physical| u32::try_from(physical).ok())
    }
}

fn data_geometry<F>(device_sectors: u64, read_sector: F) -> Option<(u64, u64, bool)>
where
    F: FnMut(u64, &mut [u8; 512]) -> bool,
{
    match GEOMETRY_KIND.load(Ordering::Acquire) {
        1 => return None,
        2 => return Some((0, GEOMETRY_SECTORS.load(Ordering::Acquire), false)),
        3 => {
            return Some((
                GEOMETRY_START.load(Ordering::Acquire),
                GEOMETRY_SECTORS.load(Ordering::Acquire),
                true,
            ));
        }
        _ => {}
    }
    match makos_gpt::classify(device_sectors, MAKOS_DATA_TYPE, read_sector) {
        DiskLayout::LegacyRaw { sectors } => {
            GEOMETRY_SECTORS.store(sectors, Ordering::Relaxed);
            GEOMETRY_KIND.store(2, Ordering::Release);
            Some((0, sectors, false))
        }
        DiskLayout::Gpt(partition) => {
            GEOMETRY_START.store(partition.first_lba, Ordering::Relaxed);
            GEOMETRY_SECTORS.store(partition.sectors, Ordering::Relaxed);
            GEOMETRY_KIND.store(3, Ordering::Release);
            Some((partition.first_lba, partition.sectors, true))
        }
        DiskLayout::Invalid => {
            crate::serial_println!("MAKOS_GPT_DATA_ERROR reason=invalid-primary-and-backup");
            GEOMETRY_KIND.store(1, Ordering::Release);
            None
        }
    }
}

fn report_geometry(start_lba: u64, sectors: u64, partitioned: bool) {
    if partitioned && !GEOMETRY_REPORTED.swap(true, Ordering::AcqRel) {
        crate::serial_println!(
            "MAKOS_GPT_DATA_OK start_lba={} sectors={} legacy_raw=0",
            start_lba,
            sectors,
        );
    }
}
