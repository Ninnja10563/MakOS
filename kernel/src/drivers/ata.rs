use crate::arch::{inb, outb};
use core::arch::asm;

const DATA: u16 = 0x170;
const ERROR_FEATURES: u16 = 0x171;
const SECTOR_COUNT: u16 = 0x172;
const LBA_LOW: u16 = 0x173;
const LBA_MID: u16 = 0x174;
const LBA_HIGH: u16 = 0x175;
const DRIVE_HEAD: u16 = 0x176;
const STATUS_COMMAND: u16 = 0x177;
const ALT_STATUS_CONTROL: u16 = 0x376;
const BSY: u8 = 0x80;
const DRQ: u8 = 0x08;
const ERR: u8 = 0x01;

pub struct AtaDisk {
    slave: bool,
    sectors: u64,
}

impl AtaDisk {
    pub fn identify_secondary() -> Option<Self> {
        Self::identify_secondary_device(false)
    }

    pub fn identify_secondary_slave() -> Option<Self> {
        Self::identify_secondary_device(true)
    }

    fn identify_secondary_device(slave: bool) -> Option<Self> {
        unsafe {
            outb(DRIVE_HEAD, if slave { 0xb0 } else { 0xa0 });
            io_delay();
            outb(SECTOR_COUNT, 0);
            outb(LBA_LOW, 0);
            outb(LBA_MID, 0);
            outb(LBA_HIGH, 0);
            outb(STATUS_COMMAND, 0xec);
        }
        if unsafe { inb(STATUS_COMMAND) } == 0 {
            return None;
        }
        if !wait_ready() || unsafe { inb(LBA_MID) != 0 || inb(LBA_HIGH) != 0 } {
            return None;
        }
        while unsafe { inb(STATUS_COMMAND) } & DRQ == 0 {
            if unsafe { inb(STATUS_COMMAND) } & ERR != 0 {
                return None;
            }
            core::hint::spin_loop();
        }
        let mut identify = [0u16; 256];
        for word in &mut identify {
            *word = unsafe { inw(DATA) };
        }
        let lba28 = u64::from(identify[60]) | (u64::from(identify[61]) << 16);
        let lba48 = u64::from(identify[100])
            | (u64::from(identify[101]) << 16)
            | (u64::from(identify[102]) << 32)
            | (u64::from(identify[103]) << 48);
        let sectors = if identify[83] & (1 << 10) != 0 {
            lba48
        } else {
            lba28
        };
        (sectors > 0).then_some(Self { slave, sectors })
    }

    pub const fn sectors(&self) -> u64 {
        self.sectors
    }

    pub fn read_sector(&mut self, lba: u32, buffer: &mut [u8; 512]) -> bool {
        if u64::from(lba) >= self.sectors || lba >= (1 << 28) {
            return false;
        }
        if !self.select_lba(lba, 0x20) || !wait_drq() {
            return false;
        }
        for index in 0..256 {
            let word = unsafe { inw(DATA) }.to_le_bytes();
            buffer[index * 2..index * 2 + 2].copy_from_slice(&word);
        }
        true
    }

    pub fn read_sectors_8(&mut self, lba: u32, buffer: &mut [u8; 4096]) -> bool {
        for sector in 0..8usize {
            let mut bytes = [0u8; 512];
            let Some(sector_lba) = lba.checked_add(sector as u32) else {
                return false;
            };
            if !self.read_sector(sector_lba, &mut bytes) {
                return false;
            }
            buffer[sector * 512..(sector + 1) * 512].copy_from_slice(&bytes);
        }
        true
    }

    pub fn write_sector(&mut self, lba: u32, buffer: &[u8; 512]) -> bool {
        self.write_sector_unflushed(lba, buffer) && self.flush()
    }

    pub(crate) fn write_sector_unflushed(&mut self, lba: u32, buffer: &[u8; 512]) -> bool {
        if u64::from(lba) >= self.sectors || lba >= (1 << 28) {
            return false;
        }
        if !self.select_lba(lba, 0x30) || !wait_drq() {
            return false;
        }
        for chunk in buffer.chunks_exact(2) {
            unsafe { outw(DATA, u16::from_le_bytes([chunk[0], chunk[1]])) };
        }
        wait_ready()
    }

    pub(crate) fn write_sectors_8_unflushed(&mut self, lba: u32, buffer: &[u8; 4096]) -> bool {
        for sector in 0..8usize {
            let Some(sector_lba) = lba.checked_add(sector as u32) else {
                return false;
            };
            let Some(bytes) = buffer
                .get(sector * 512..(sector + 1) * 512)
                .and_then(|bytes| bytes.try_into().ok())
            else {
                return false;
            };
            if !self.write_sector_unflushed(sector_lba, bytes) {
                return false;
            }
        }
        true
    }

    pub fn flush(&mut self) -> bool {
        if !wait_ready() {
            return false;
        }
        unsafe { outb(STATUS_COMMAND, 0xe7) };
        wait_ready()
    }

    fn select_lba(&self, lba: u32, command: u8) -> bool {
        if !wait_ready() {
            return false;
        }
        unsafe {
            outb(ALT_STATUS_CONTROL, 2); // disable ATA IRQ; PIO polling
            outb(
                DRIVE_HEAD,
                0xe0 | if self.slave { 0x10 } else { 0 } | ((lba >> 24) as u8 & 0x0f),
            );
            outb(ERROR_FEATURES, 0);
            outb(SECTOR_COUNT, 1);
            outb(LBA_LOW, lba as u8);
            outb(LBA_MID, (lba >> 8) as u8);
            outb(LBA_HIGH, (lba >> 16) as u8);
            outb(STATUS_COMMAND, command);
        }
        true
    }
}

fn wait_ready() -> bool {
    for _ in 0..10_000_000 {
        let status = unsafe { inb(STATUS_COMMAND) };
        if status & ERR != 0 {
            return false;
        }
        if status & BSY == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

fn wait_drq() -> bool {
    for _ in 0..10_000_000 {
        let status = unsafe { inb(STATUS_COMMAND) };
        if status & ERR != 0 {
            return false;
        }
        if status & BSY == 0 && status & DRQ != 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

unsafe fn io_delay() {
    for _ in 0..4 {
        let _ = unsafe { inb(ALT_STATUS_CONTROL) };
    }
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    unsafe {
        asm!("in ax, dx", out("ax") value, in("dx") port, options(nomem, nostack, preserves_flags))
    };
    value
}

#[inline]
unsafe fn outw(port: u16, value: u16) {
    unsafe {
        asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags))
    };
}
