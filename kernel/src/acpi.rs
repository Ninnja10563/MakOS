use core::slice;
#[cfg(target_arch = "x86_64")]
use makos_acpi::MadtInfo;
#[cfg(target_arch = "aarch64")]
use makos_acpi::{ArmMadtInfo, GenericTimerInfo};
use makos_boot_api::BootInfo;

const SDT_HEADER_SIZE: usize = 36;
const MAX_TABLE_SIZE: usize = 1024 * 1024;
const EARLY_IDENTITY_LIMIT: u64 = 4 * 1024 * 1024 * 1024;

#[cfg(target_arch = "aarch64")]
#[derive(Clone, Copy)]
pub struct ArmPlatform {
    pub interrupt: ArmMadtInfo,
    pub timer: GenericTimerInfo,
}

#[cfg(target_arch = "x86_64")]
pub fn discover(boot: &BootInfo) -> MadtInfo {
    let table = find_table(boot, b"APIC").unwrap_or_else(|| crate::fatal("ACPI MADT absent"));
    makos_acpi::parse_madt(table).unwrap_or_else(|_| crate::fatal("ACPI MADT invalid"))
}

#[cfg(target_arch = "aarch64")]
pub fn discover_arm(boot: &BootInfo) -> ArmPlatform {
    let madt = find_table(boot, b"APIC").unwrap_or_else(|| crate::fatal("ACPI MADT absent"));
    let gtdt = find_table(boot, b"GTDT").unwrap_or_else(|| crate::fatal("ACPI GTDT absent"));
    ArmPlatform {
        interrupt: makos_acpi::parse_arm_madt(madt)
            .unwrap_or_else(|_| crate::fatal("ACPI Arm MADT invalid")),
        timer: makos_acpi::parse_gtdt(gtdt).unwrap_or_else(|_| crate::fatal("ACPI GTDT invalid")),
    }
}

fn find_table(boot: &BootInfo, signature: &[u8; 4]) -> Option<&'static [u8]> {
    let rsdp = boot.acpi_rsdp;
    let rsdp_v1 = physical_slice(rsdp, 20).unwrap_or_else(|| crate::fatal("ACPI RSDP unmapped"));
    if &rsdp_v1[0..8] != b"RSD PTR " || makos_acpi::checksum(rsdp_v1) != 0 {
        crate::fatal("ACPI RSDP validation failed");
    }
    let revision = rsdp_v1[15];
    let rsdt_address = read_u32(rsdp_v1, 16).unwrap() as u64;
    let (root_address, entry_size) = if revision >= 2 {
        let rsdp_v2 =
            physical_slice(rsdp, 36).unwrap_or_else(|| crate::fatal("ACPI v2 RSDP unmapped"));
        let length = read_u32(rsdp_v2, 20).unwrap() as usize;
        if !(36..=64).contains(&length) {
            crate::fatal("ACPI RSDP length invalid");
        }
        let full = physical_slice(rsdp, length)
            .unwrap_or_else(|| crate::fatal("ACPI RSDP length unmapped"));
        if makos_acpi::checksum(full) != 0 {
            crate::fatal("ACPI extended checksum failed");
        }
        let xsdt = read_u64(full, 24).unwrap();
        if xsdt != 0 {
            (xsdt, 8)
        } else {
            (rsdt_address, 4)
        }
    } else {
        (rsdt_address, 4)
    };

    let root = sdt(root_address).unwrap_or_else(|| crate::fatal("ACPI root table invalid"));
    let expected = if entry_size == 8 { b"XSDT" } else { b"RSDT" };
    if &root[0..4] != expected {
        crate::fatal("ACPI root signature mismatch");
    }
    let payload = &root[SDT_HEADER_SIZE..];
    if payload.len() % entry_size != 0 {
        crate::fatal("ACPI root entry alignment invalid");
    }
    for entry in payload.chunks_exact(entry_size) {
        let address = if entry_size == 8 {
            u64::from_le_bytes(entry.try_into().unwrap())
        } else {
            u32::from_le_bytes(entry.try_into().unwrap()) as u64
        };
        let Some(table) = sdt(address) else {
            continue;
        };
        if &table[0..4] == signature {
            return Some(table);
        }
    }
    None
}

fn sdt(address: u64) -> Option<&'static [u8]> {
    let header = physical_slice(address, SDT_HEADER_SIZE)?;
    let length = read_u32(header, 4)? as usize;
    if !(SDT_HEADER_SIZE..=MAX_TABLE_SIZE).contains(&length) {
        return None;
    }
    let table = physical_slice(address, length)?;
    (makos_acpi::checksum(table) == 0).then_some(table)
}

fn physical_slice(address: u64, length: usize) -> Option<&'static [u8]> {
    if address == 0
        || address.checked_add(length as u64)? > EARLY_IDENTITY_LIMIT
        || address > usize::MAX as u64
    {
        return None;
    }
    Some(unsafe { slice::from_raw_parts(address as *const u8, length) })
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
