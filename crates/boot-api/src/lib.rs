#![no_std]

pub const BOOT_INFO_MAGIC: u64 = 0x4d41_4b4f_5342_4f4f; // "MAKOSBOO"
pub const BOOT_ABI_VERSION: u32 = 2;
pub const BOOT_CONFIG_CAPACITY: usize = 192;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PixelFormat {
    Rgb = 0,
    Bgr = 1,
    Bitmask = 2,
    BltOnly = 3,
    Unknown = u32::MAX,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct FramebufferInfo {
    pub address: u64,
    pub byte_len: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub pixel_format: PixelFormat,
}

impl FramebufferInfo {
    pub const EMPTY: Self = Self {
        address: 0,
        byte_len: 0,
        width: 0,
        height: 0,
        stride: 0,
        pixel_format: PixelFormat::Unknown,
    };
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct MemoryMapInfo {
    pub address: u64,
    pub entry_count: u64,
    pub descriptor_size: u32,
    pub descriptor_version: u32,
}

impl MemoryMapInfo {
    pub const EMPTY: Self = Self {
        address: 0,
        entry_count: 0,
        descriptor_size: 0,
        descriptor_version: 0,
    };
}

/// Stable prefix of an EFI memory descriptor. Use handoff stride, not
/// `size_of::<UefiMemoryDescriptor>()`, to find subsequent entries.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct UefiMemoryDescriptor {
    pub memory_type: u32,
    pub _padding: u32,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub page_count: u64,
    pub attributes: u64,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct BootInfo {
    pub magic: u64,
    pub abi_version: u32,
    pub struct_size: u32,
    pub framebuffer: FramebufferInfo,
    pub memory_map: MemoryMapInfo,
    pub acpi_rsdp: u64,
    pub kernel_physical_start: u64,
    pub kernel_physical_end: u64,
    pub config_length: u32,
    pub _config_reserved: u32,
    pub config: [u8; BOOT_CONFIG_CAPACITY],
}

impl BootInfo {
    pub const fn new() -> Self {
        Self {
            magic: BOOT_INFO_MAGIC,
            abi_version: BOOT_ABI_VERSION,
            struct_size: core::mem::size_of::<Self>() as u32,
            framebuffer: FramebufferInfo::EMPTY,
            memory_map: MemoryMapInfo::EMPTY,
            acpi_rsdp: 0,
            kernel_physical_start: 0,
            kernel_physical_end: 0,
            config_length: 0,
            _config_reserved: 0,
            config: [0; BOOT_CONFIG_CAPACITY],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_info_reports_current_layout() {
        let info = BootInfo::new();
        assert_eq!(info.abi_version, 2);
        assert_eq!(info.struct_size as usize, core::mem::size_of::<BootInfo>());
        assert_eq!(info.config_length, 0);
        assert_eq!(info.config, [0; BOOT_CONFIG_CAPACITY]);
    }
}

impl Default for BootInfo {
    fn default() -> Self {
        Self::new()
    }
}
