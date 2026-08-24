#![no_std]

pub const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
pub const IMAGE_SCN_MEM_READ: u32 = 0x4000_0000;
pub const IMAGE_SCN_MEM_WRITE: u32 = 0x8000_0000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    HeaderTooShort,
    BadDosMagic,
    BadPeSignature,
    UnsupportedMachine,
    UnsupportedOptionalHeader,
    BadAlignment,
    SectionTableOutOfBounds,
    SectionRawDataOutOfBounds,
    SectionVirtualRangeOutOfBounds,
    EntryOutOfBounds,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Section {
    pub name: [u8; 8],
    pub virtual_size: u32,
    pub virtual_address: u32,
    pub raw_size: u32,
    pub raw_offset: u32,
    pub characteristics: u32,
}

pub struct Pe64<'a> {
    bytes: &'a [u8],
    image_base: u64,
    entry_rva: u32,
    image_size: u32,
    section_offset: usize,
    section_count: usize,
}

impl<'a> Pe64<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        if bytes.len() < 64 {
            return Err(Error::HeaderTooShort);
        }
        if &bytes[..2] != b"MZ" {
            return Err(Error::BadDosMagic);
        }
        let pe_offset = read_u32(bytes, 0x3c).ok_or(Error::HeaderTooShort)? as usize;
        let coff = pe_offset
            .checked_add(4)
            .filter(|end| {
                end.checked_add(20)
                    .is_some_and(|value| value <= bytes.len())
            })
            .ok_or(Error::HeaderTooShort)?;
        if bytes.get(pe_offset..coff) != Some(b"PE\0\0") {
            return Err(Error::BadPeSignature);
        }
        if read_u16(bytes, coff) != Some(0x8664) {
            return Err(Error::UnsupportedMachine);
        }
        let section_count = read_u16(bytes, coff + 2).ok_or(Error::HeaderTooShort)? as usize;
        let optional_size = read_u16(bytes, coff + 16).ok_or(Error::HeaderTooShort)? as usize;
        let optional = coff + 20;
        if optional_size < 112
            || optional
                .checked_add(optional_size)
                .is_none_or(|end| end > bytes.len())
            || read_u16(bytes, optional) != Some(0x20b)
        {
            return Err(Error::UnsupportedOptionalHeader);
        }
        let section_alignment = read_u32(bytes, optional + 32).unwrap();
        let file_alignment = read_u32(bytes, optional + 36).unwrap();
        if section_alignment < 4096
            || !section_alignment.is_power_of_two()
            || file_alignment < 512
            || !file_alignment.is_power_of_two()
        {
            return Err(Error::BadAlignment);
        }
        let section_offset = optional + optional_size;
        if section_count == 0
            || section_count > 32
            || section_count
                .checked_mul(40)
                .and_then(|size| section_offset.checked_add(size))
                .is_none_or(|end| end > bytes.len())
        {
            return Err(Error::SectionTableOutOfBounds);
        }
        let pe = Self {
            bytes,
            image_base: read_u64(bytes, optional + 24).unwrap(),
            entry_rva: read_u32(bytes, optional + 16).unwrap(),
            image_size: read_u32(bytes, optional + 56).unwrap(),
            section_offset,
            section_count,
        };
        if pe.entry_rva >= pe.image_size {
            return Err(Error::EntryOutOfBounds);
        }
        for section in pe.sections() {
            let raw_end = (section.raw_offset as usize)
                .checked_add(section.raw_size as usize)
                .ok_or(Error::SectionRawDataOutOfBounds)?;
            if raw_end > bytes.len() {
                return Err(Error::SectionRawDataOutOfBounds);
            }
            let virtual_end = section
                .virtual_address
                .checked_add(section.virtual_size.max(section.raw_size))
                .ok_or(Error::SectionVirtualRangeOutOfBounds)?;
            if virtual_end > pe.image_size {
                return Err(Error::SectionVirtualRangeOutOfBounds);
            }
        }
        Ok(pe)
    }

    pub const fn image_base(&self) -> u64 {
        self.image_base
    }

    pub const fn image_size(&self) -> u32 {
        self.image_size
    }

    pub const fn entry(&self) -> u64 {
        self.image_base + self.entry_rva as u64
    }

    pub fn sections(&self) -> Sections<'_> {
        Sections { pe: self, next: 0 }
    }
}

pub struct Sections<'a> {
    pe: &'a Pe64<'a>,
    next: usize,
}

impl Iterator for Sections<'_> {
    type Item = Section;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next == self.pe.section_count {
            return None;
        }
        let offset = self.pe.section_offset + self.next * 40;
        self.next += 1;
        Some(Section {
            name: self.pe.bytes[offset..offset + 8].try_into().unwrap(),
            virtual_size: read_u32(self.pe.bytes, offset + 8).unwrap(),
            virtual_address: read_u32(self.pe.bytes, offset + 12).unwrap(),
            raw_size: read_u32(self.pe.bytes, offset + 16).unwrap(),
            raw_offset: read_u32(self.pe.bytes, offset + 20).unwrap(),
            characteristics: read_u32(self.pe.bytes, offset + 36).unwrap(),
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

    fn valid_pe() -> std::vec::Vec<u8> {
        let mut b = vec![0u8; 0x600];
        b[..2].copy_from_slice(b"MZ");
        b[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        let c = 0x84;
        b[c..c + 2].copy_from_slice(&0x8664u16.to_le_bytes());
        b[c + 2..c + 4].copy_from_slice(&1u16.to_le_bytes());
        b[c + 16..c + 18].copy_from_slice(&240u16.to_le_bytes());
        let o = c + 20;
        b[o..o + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        b[o + 16..o + 20].copy_from_slice(&0x1000u32.to_le_bytes());
        b[o + 24..o + 32].copy_from_slice(&0x1_0000_0000u64.to_le_bytes());
        b[o + 32..o + 36].copy_from_slice(&4096u32.to_le_bytes());
        b[o + 36..o + 40].copy_from_slice(&512u32.to_le_bytes());
        b[o + 56..o + 60].copy_from_slice(&0x3000u32.to_le_bytes());
        let s = o + 240;
        b[s..s + 5].copy_from_slice(b".text");
        b[s + 8..s + 12].copy_from_slice(&0x80u32.to_le_bytes());
        b[s + 12..s + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        b[s + 16..s + 20].copy_from_slice(&0x200u32.to_le_bytes());
        b[s + 20..s + 24].copy_from_slice(&0x400u32.to_le_bytes());
        b[s + 36..s + 40]
            .copy_from_slice(&(IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_EXECUTE).to_le_bytes());
        b
    }

    #[test]
    fn parses_pe32_plus_section() {
        let bytes = valid_pe();
        let pe = Pe64::parse(&bytes).unwrap();
        assert_eq!(pe.image_base(), 0x1_0000_0000);
        assert_eq!(pe.entry(), 0x1_0000_1000);
        assert_eq!(pe.sections().next().unwrap().raw_offset, 0x400);
    }

    #[test]
    fn rejects_wrong_machine() {
        let mut b = valid_pe();
        b[0x84..0x86].copy_from_slice(&0x14cu16.to_le_bytes());
        assert_eq!(Pe64::parse(&b).err(), Some(Error::UnsupportedMachine));
    }

    #[test]
    fn rejects_bad_optional_magic() {
        let mut b = valid_pe();
        b[0x98..0x9a].copy_from_slice(&0x10bu16.to_le_bytes());
        assert_eq!(
            Pe64::parse(&b).err(),
            Some(Error::UnsupportedOptionalHeader)
        );
    }

    #[test]
    fn rejects_raw_section_overflow() {
        let mut b = valid_pe();
        let s = 0x98 + 240;
        b[s + 20..s + 24].copy_from_slice(&0x500u32.to_le_bytes());
        assert_eq!(
            Pe64::parse(&b).err(),
            Some(Error::SectionRawDataOutOfBounds)
        );
    }

    #[test]
    fn rejects_entry_outside_image() {
        let mut b = valid_pe();
        b[0x98 + 16..0x98 + 20].copy_from_slice(&0x4000u32.to_le_bytes());
        assert_eq!(Pe64::parse(&b).err(), Some(Error::EntryOutOfBounds));
    }
}
