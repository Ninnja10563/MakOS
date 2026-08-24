#![no_std]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    TooShort,
    BadSignature,
    BadLength,
    BadChecksum,
    BadEntry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MadtInfo {
    pub local_apic_address: u64,
    pub enabled_cpu_count: u32,
    pub apic_ids: [u32; 64],
    pub flags: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArmMadtInfo {
    pub gic_distributor_address: u64,
    pub gic_cpu_interface_address: u64,
    pub gic_version: u8,
    pub enabled_cpu_count: u32,
    pub mpidrs: [u64; 64],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericTimerInfo {
    pub nonsecure_el1_gsiv: u32,
    pub nonsecure_el1_flags: u32,
    pub virtual_el1_gsiv: u32,
    pub virtual_el1_flags: u32,
}

pub fn parse_madt(bytes: &[u8]) -> Result<MadtInfo, Error> {
    if bytes.len() < 44 {
        return Err(Error::TooShort);
    }
    if &bytes[0..4] != b"APIC" {
        return Err(Error::BadSignature);
    }
    let length = read_u32(bytes, 4).ok_or(Error::TooShort)? as usize;
    if length < 44 || length > bytes.len() {
        return Err(Error::BadLength);
    }
    if checksum(&bytes[..length]) != 0 {
        return Err(Error::BadChecksum);
    }
    let mut info = MadtInfo {
        local_apic_address: read_u32(bytes, 36).unwrap() as u64,
        enabled_cpu_count: 0,
        apic_ids: [0; 64],
        flags: read_u32(bytes, 40).unwrap(),
    };
    let mut offset = 44;
    while offset < length {
        if offset + 2 > length {
            return Err(Error::BadEntry);
        }
        let entry_type = bytes[offset];
        let entry_length = bytes[offset + 1] as usize;
        if entry_length < 2 || offset + entry_length > length {
            return Err(Error::BadEntry);
        }
        match entry_type {
            0 if entry_length >= 8 => {
                let flags = read_u32(bytes, offset + 4).unwrap();
                if flags & 1 != 0 {
                    let index = info.enabled_cpu_count as usize;
                    if index < info.apic_ids.len() {
                        info.apic_ids[index] = u32::from(bytes[offset + 3]);
                    }
                    info.enabled_cpu_count = info.enabled_cpu_count.saturating_add(1);
                }
            }
            5 if entry_length >= 12 => {
                info.local_apic_address = read_u64(bytes, offset + 4).unwrap();
            }
            9 if entry_length >= 16 => {
                let flags = read_u32(bytes, offset + 8).unwrap();
                if flags & 1 != 0 {
                    let index = info.enabled_cpu_count as usize;
                    if index < info.apic_ids.len() {
                        info.apic_ids[index] = read_u32(bytes, offset + 4).unwrap();
                    }
                    info.enabled_cpu_count = info.enabled_cpu_count.saturating_add(1);
                }
            }
            _ => {}
        }
        offset += entry_length;
    }
    Ok(info)
}

pub fn parse_arm_madt(bytes: &[u8]) -> Result<ArmMadtInfo, Error> {
    let length = checked_table(bytes, b"APIC", 44)?;
    let mut info = ArmMadtInfo {
        gic_distributor_address: 0,
        gic_cpu_interface_address: 0,
        gic_version: 0,
        enabled_cpu_count: 0,
        mpidrs: [0; 64],
    };
    let mut distributor_count = 0u8;
    let mut offset = 44;
    while offset < length {
        if offset + 2 > length {
            return Err(Error::BadEntry);
        }
        let entry_type = bytes[offset];
        let entry_length = bytes[offset + 1] as usize;
        if entry_length < 2 || offset + entry_length > length {
            return Err(Error::BadEntry);
        }
        match entry_type {
            // GICC: ACPI 5.0 supplied 76 bytes; later revisions appended fields.
            0x0b if entry_length >= 76 => {
                let flags = read_u32(bytes, offset + 12).ok_or(Error::BadEntry)?;
                if flags & 1 != 0 {
                    let interface = read_u64(bytes, offset + 32).ok_or(Error::BadEntry)?;
                    let index = info.enabled_cpu_count as usize;
                    if index < info.mpidrs.len() {
                        info.mpidrs[index] = read_u64(bytes, offset + 68).ok_or(Error::BadEntry)?;
                    }
                    if interface != 0 {
                        if info.gic_cpu_interface_address != 0
                            && info.gic_cpu_interface_address != interface
                        {
                            return Err(Error::BadEntry);
                        }
                        info.gic_cpu_interface_address = interface;
                    }
                    info.enabled_cpu_count = info.enabled_cpu_count.saturating_add(1);
                }
            }
            // ACPI requires exactly one GIC Distributor on Arm systems.
            0x0c if entry_length >= 24 => {
                distributor_count = distributor_count.saturating_add(1);
                info.gic_distributor_address =
                    read_u64(bytes, offset + 8).ok_or(Error::BadEntry)?;
                info.gic_version = *bytes.get(offset + 20).ok_or(Error::BadEntry)?;
            }
            _ => {}
        }
        offset += entry_length;
    }
    if distributor_count != 1
        || info.gic_distributor_address == 0
        || info.enabled_cpu_count == 0
        || !(1..=4).contains(&info.gic_version)
        || (info.gic_version <= 2 && info.gic_cpu_interface_address == 0)
    {
        return Err(Error::BadEntry);
    }
    Ok(info)
}

pub fn parse_gtdt(bytes: &[u8]) -> Result<GenericTimerInfo, Error> {
    let _ = checked_table(bytes, b"GTDT", 80)?;
    let info = GenericTimerInfo {
        nonsecure_el1_gsiv: read_u32(bytes, 56).ok_or(Error::TooShort)?,
        nonsecure_el1_flags: read_u32(bytes, 60).ok_or(Error::TooShort)?,
        virtual_el1_gsiv: read_u32(bytes, 64).ok_or(Error::TooShort)?,
        virtual_el1_flags: read_u32(bytes, 68).ok_or(Error::TooShort)?,
    };
    if info.virtual_el1_gsiv < 16
        || info.virtual_el1_gsiv >= 1020
        || info.virtual_el1_flags & !0x7 != 0
        || info.nonsecure_el1_flags & !0x7 != 0
    {
        return Err(Error::BadEntry);
    }
    Ok(info)
}

fn checked_table(bytes: &[u8], signature: &[u8; 4], minimum: usize) -> Result<usize, Error> {
    if bytes.len() < minimum {
        return Err(Error::TooShort);
    }
    if &bytes[0..4] != signature {
        return Err(Error::BadSignature);
    }
    let length = read_u32(bytes, 4).ok_or(Error::TooShort)? as usize;
    if length < minimum || length > bytes.len() {
        return Err(Error::BadLength);
    }
    if checksum(&bytes[..length]) != 0 {
        return Err(Error::BadChecksum);
    }
    Ok(length)
}

pub fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte))
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

    use super::*;
    use std::vec;

    fn madt(entries: &[u8]) -> std::vec::Vec<u8> {
        let mut table = vec![0u8; 44 + entries.len()];
        table[0..4].copy_from_slice(b"APIC");
        let length = table.len() as u32;
        table[4..8].copy_from_slice(&length.to_le_bytes());
        table[36..40].copy_from_slice(&0xfee0_0000u32.to_le_bytes());
        table[40..44].copy_from_slice(&1u32.to_le_bytes());
        table[44..].copy_from_slice(entries);
        table[9] = 0u8.wrapping_sub(checksum(&table));
        table
    }

    fn table(signature: &[u8; 4], length: usize) -> std::vec::Vec<u8> {
        let mut table = vec![0u8; length];
        table[0..4].copy_from_slice(signature);
        table[4..8].copy_from_slice(&(length as u32).to_le_bytes());
        table
    }

    #[test]
    fn parses_enabled_processors_and_override() {
        let mut entries = vec![0, 8, 0, 1, 1, 0, 0, 0, 0, 8, 1, 2, 0, 0, 0, 0];
        entries.extend_from_slice(&[5, 12, 0, 0]);
        entries.extend_from_slice(&0xfee1_0000u64.to_le_bytes());
        let table = madt(&entries);
        let info = parse_madt(&table).unwrap();
        assert_eq!(info.enabled_cpu_count, 1);
        assert_eq!(info.apic_ids[0], 1);
        assert_eq!(info.local_apic_address, 0xfee1_0000);
    }

    #[test]
    fn rejects_checksum_failure() {
        let mut table = madt(&[]);
        table[20] ^= 1;
        assert_eq!(parse_madt(&table), Err(Error::BadChecksum));
    }

    #[test]
    fn rejects_zero_length_entry() {
        let table = madt(&[7, 0]);
        assert_eq!(parse_madt(&table), Err(Error::BadEntry));
    }

    #[test]
    fn parses_arm_gicv2_madt() {
        let mut gicc = vec![0u8; 82];
        gicc[0] = 0x0b;
        gicc[1] = 82;
        gicc[12..16].copy_from_slice(&1u32.to_le_bytes());
        gicc[32..40].copy_from_slice(&0x0801_0000u64.to_le_bytes());
        gicc[68..76].copy_from_slice(&0x8000_0001u64.to_le_bytes());
        let mut gicd = vec![0u8; 24];
        gicd[0] = 0x0c;
        gicd[1] = 24;
        gicd[8..16].copy_from_slice(&0x0800_0000u64.to_le_bytes());
        gicd[20] = 2;
        let mut table = madt(&[gicc, gicd].concat());
        table[9] = 0;
        table[9] = 0u8.wrapping_sub(checksum(&table));
        let info = parse_arm_madt(&table).unwrap();
        assert_eq!(info.gic_distributor_address, 0x0800_0000);
        assert_eq!(info.gic_cpu_interface_address, 0x0801_0000);
        assert_eq!(info.gic_version, 2);
        assert_eq!(info.enabled_cpu_count, 1);
        assert_eq!(info.mpidrs[0], 0x8000_0001);
    }

    #[test]
    fn rejects_arm_madt_without_distributor() {
        let table = madt(&[]);
        assert_eq!(parse_arm_madt(&table), Err(Error::BadEntry));
    }

    #[test]
    fn parses_generic_timer_table() {
        let mut table = table(b"GTDT", 104);
        table[56..60].copy_from_slice(&30u32.to_le_bytes());
        table[60..64].copy_from_slice(&4u32.to_le_bytes());
        table[64..68].copy_from_slice(&27u32.to_le_bytes());
        table[68..72].copy_from_slice(&4u32.to_le_bytes());
        table[9] = 0u8.wrapping_sub(checksum(&table));
        let info = parse_gtdt(&table).unwrap();
        assert_eq!(info.nonsecure_el1_gsiv, 30);
        assert_eq!(info.virtual_el1_gsiv, 27);
        assert_eq!(info.virtual_el1_flags, 4);
    }
}
