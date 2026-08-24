//! QEMU `virt` PL031 real-time clock.

const BASE: u64 = 0x0901_0000;
const DATA: u64 = 0x000;
const PERIPHERAL_ID: [u8; 4] = [0x31, 0x10, 0x14, 0x00];
const CELL_ID: [u8; 4] = [0x0d, 0xf0, 0x05, 0xb1];

pub fn init() {
    for (index, expected) in PERIPHERAL_ID.iter().enumerate() {
        if read32(BASE + 0xfe0 + index as u64 * 4) as u8 != *expected {
            crate::fatal("AArch64 PL031 peripheral ID mismatch");
        }
    }
    for (index, expected) in CELL_ID.iter().enumerate() {
        if read32(BASE + 0xff0 + index as u64 * 4) as u8 != *expected {
            crate::fatal("AArch64 PL031 cell ID mismatch");
        }
    }
    let seconds = unix_seconds();
    if seconds < 1_577_836_800 {
        crate::fatal("AArch64 PL031 time predates 2020");
    }
    crate::serial_println!(
        "MAKOS_AARCH64_RTC_OK transport=pl031 clock=realtime unix_seconds={} tls_validation=ready",
        seconds,
    );
}

pub fn unix_seconds() -> u64 {
    u64::from(read32(BASE + DATA))
}

// Keep MMIO as one plain load. LLVM may otherwise select a post-indexed load;
// QEMU/HVF cannot decode that trapped access and aborts on a missing ESR ISV.
#[inline(never)]
fn read32(address: u64) -> u32 {
    let value: u64;
    unsafe {
        core::arch::asm!(
            "ldr {value:w}, [{address}]",
            address = in(reg) address,
            value = lateout(reg) value,
            options(nostack, readonly),
        )
    };
    value as u32
}
