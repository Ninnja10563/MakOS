#![no_std]

pub const PT_LOAD: u32 = 1;
pub const PT_INTERP: u32 = 3;
pub const ET_EXEC: u16 = 2;
pub const ET_DYN: u16 = 3;
pub const EM_X86_64: u16 = 62;
pub const EM_AARCH64: u16 = 183;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    HeaderTooShort,
    BadMagic,
    UnsupportedClass,
    UnsupportedEndian,
    UnsupportedVersion,
    UnsupportedMachine,
    BadHeaderSize,
    BadProgramHeaderSize,
    ProgramHeaderTableOutOfBounds,
    SegmentFileLargerThanMemory,
    SegmentOutOfBounds,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgramHeader {
    pub segment_type: u32,
    pub flags: u32,
    pub offset: u64,
    pub virtual_address: u64,
    pub physical_address: u64,
    pub file_size: u64,
    pub memory_size: u64,
    pub alignment: u64,
}

pub struct Elf64<'a> {
    bytes: &'a [u8],
    elf_type: u16,
    entry: u64,
    ph_offset: usize,
    ph_count: usize,
}

impl<'a> Elf64<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        Self::parse_for_machine(bytes, EM_X86_64)
    }

    pub fn parse_for_machine(bytes: &'a [u8], machine: u16) -> Result<Self, Error> {
        Self::parse_headers_for_machine(bytes, machine, bytes.len() as u64)
    }

    /// Parse an ELF header/program-header prefix while validating segment
    /// offsets against full backing-file length. Supports sector-backed exec
    /// without copying whole executable into kernel heap.
    pub fn parse_headers_for_machine(
        bytes: &'a [u8],
        machine: u16,
        file_length: u64,
    ) -> Result<Self, Error> {
        if bytes.len() < 64 {
            return Err(Error::HeaderTooShort);
        }
        if &bytes[0..4] != b"\x7fELF" {
            return Err(Error::BadMagic);
        }
        if bytes[4] != 2 {
            return Err(Error::UnsupportedClass);
        }
        if bytes[5] != 1 {
            return Err(Error::UnsupportedEndian);
        }
        if bytes[6] != 1 || read_u32(bytes, 20) != Some(1) {
            return Err(Error::UnsupportedVersion);
        }
        if read_u16(bytes, 18) != Some(machine) {
            return Err(Error::UnsupportedMachine);
        }
        if read_u16(bytes, 52) != Some(64) {
            return Err(Error::BadHeaderSize);
        }
        if read_u16(bytes, 54) != Some(56) {
            return Err(Error::BadProgramHeaderSize);
        }

        let ph_offset = usize::try_from(read_u64(bytes, 32).unwrap())
            .map_err(|_| Error::ProgramHeaderTableOutOfBounds)?;
        let ph_count = usize::from(read_u16(bytes, 56).unwrap());
        let table_len = ph_count
            .checked_mul(56)
            .and_then(|n| ph_offset.checked_add(n))
            .ok_or(Error::ProgramHeaderTableOutOfBounds)?;
        if table_len > bytes.len() {
            return Err(Error::ProgramHeaderTableOutOfBounds);
        }

        let elf = Self {
            bytes,
            elf_type: read_u16(bytes, 16).unwrap(),
            entry: read_u64(bytes, 24).unwrap(),
            ph_offset,
            ph_count,
        };
        for ph in elf.program_headers() {
            if ph.file_size > ph.memory_size {
                return Err(Error::SegmentFileLargerThanMemory);
            }
            let end = ph
                .offset
                .checked_add(ph.file_size)
                .ok_or(Error::SegmentOutOfBounds)?;
            if end > file_length {
                return Err(Error::SegmentOutOfBounds);
            }
        }
        Ok(elf)
    }

    pub const fn entry(&self) -> u64 {
        self.entry
    }

    pub const fn elf_type(&self) -> u16 {
        self.elf_type
    }

    pub fn program_headers(&self) -> ProgramHeaders<'_> {
        ProgramHeaders { elf: self, next: 0 }
    }
}

pub struct ProgramHeaders<'a> {
    elf: &'a Elf64<'a>,
    next: usize,
}

impl Iterator for ProgramHeaders<'_> {
    type Item = ProgramHeader;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next == self.elf.ph_count {
            return None;
        }
        let p = self.elf.ph_offset + self.next * 56;
        self.next += 1;
        Some(ProgramHeader {
            segment_type: read_u32(self.elf.bytes, p).unwrap(),
            flags: read_u32(self.elf.bytes, p + 4).unwrap(),
            offset: read_u64(self.elf.bytes, p + 8).unwrap(),
            virtual_address: read_u64(self.elf.bytes, p + 16).unwrap(),
            physical_address: read_u64(self.elf.bytes, p + 24).unwrap(),
            file_size: read_u64(self.elf.bytes, p + 32).unwrap(),
            memory_size: read_u64(self.elf.bytes, p + 40).unwrap(),
            alignment: read_u64(self.elf.bytes, p + 48).unwrap(),
        })
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
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

    fn valid_elf() -> std::vec::Vec<u8> {
        let mut b = vec![0u8; 128];
        b[0..4].copy_from_slice(b"\x7fELF");
        b[4] = 2;
        b[5] = 1;
        b[6] = 1;
        b[16..18].copy_from_slice(&2u16.to_le_bytes());
        b[18..20].copy_from_slice(&62u16.to_le_bytes());
        b[20..24].copy_from_slice(&1u32.to_le_bytes());
        b[24..32].copy_from_slice(&0x0100_0000u64.to_le_bytes());
        b[32..40].copy_from_slice(&64u64.to_le_bytes());
        b[52..54].copy_from_slice(&64u16.to_le_bytes());
        b[54..56].copy_from_slice(&56u16.to_le_bytes());
        b[56..58].copy_from_slice(&1u16.to_le_bytes());
        b[64..68].copy_from_slice(&PT_LOAD.to_le_bytes());
        b[68..72].copy_from_slice(&5u32.to_le_bytes());
        b[72..80].copy_from_slice(&120u64.to_le_bytes());
        b[80..88].copy_from_slice(&0x0100_0000u64.to_le_bytes());
        b[88..96].copy_from_slice(&0x0100_0000u64.to_le_bytes());
        b[96..104].copy_from_slice(&8u64.to_le_bytes());
        b[104..112].copy_from_slice(&16u64.to_le_bytes());
        b[112..120].copy_from_slice(&4096u64.to_le_bytes());
        b
    }

    #[test]
    fn parses_valid_load_segment() {
        let b = valid_elf();
        let elf = Elf64::parse(&b).unwrap();
        assert_eq!(elf.elf_type(), ET_EXEC);
        assert_eq!(elf.entry(), 0x0100_0000);
        let ph = elf.program_headers().next().unwrap();
        assert_eq!(ph.segment_type, PT_LOAD);
        assert_eq!(ph.file_size, 8);
        assert_eq!(ph.memory_size, 16);
    }

    #[test]
    fn rejects_wrong_machine() {
        let mut b = valid_elf();
        b[18..20].copy_from_slice(&183u16.to_le_bytes());
        assert!(matches!(Elf64::parse(&b), Err(Error::UnsupportedMachine)));
    }

    #[test]
    fn rejects_truncated_program_table() {
        let mut b = valid_elf();
        b[32..40].copy_from_slice(&100u64.to_le_bytes());
        assert!(matches!(
            Elf64::parse(&b),
            Err(Error::ProgramHeaderTableOutOfBounds)
        ));
    }

    #[test]
    fn rejects_file_size_over_memory_size() {
        let mut b = valid_elf();
        b[104..112].copy_from_slice(&4u64.to_le_bytes());
        assert!(matches!(
            Elf64::parse(&b),
            Err(Error::SegmentFileLargerThanMemory)
        ));
    }

    #[test]
    fn rejects_segment_past_file() {
        let mut b = valid_elf();
        b[96..104].copy_from_slice(&16u64.to_le_bytes());
        assert!(matches!(Elf64::parse(&b), Err(Error::SegmentOutOfBounds)));
    }

    #[test]
    fn parses_header_prefix_against_full_backing_length() {
        let b = valid_elf();
        let prefix = &b[..120];
        assert!(Elf64::parse_for_machine(prefix, EM_X86_64).is_err());
        let elf = Elf64::parse_headers_for_machine(prefix, EM_X86_64, b.len() as u64).unwrap();
        assert_eq!(elf.program_headers().next().unwrap().file_size, 8);
    }
}
