#![no_main]
#![no_std]

use core::mem::MaybeUninit;
use core::ptr;
use makos_boot_api::{
    BOOT_CONFIG_CAPACITY, BootInfo, FramebufferInfo, MemoryMapInfo, PixelFormat as BootPixelFormat,
};
#[cfg(target_arch = "aarch64")]
use makos_elf64::EM_AARCH64;
#[cfg(target_arch = "x86_64")]
use makos_elf64::EM_X86_64;
use makos_elf64::{Elf64, PT_LOAD};
use uefi::boot::{self, AllocateType};
use uefi::fs::FileSystem;
use uefi::mem::memory_map::{MemoryMap, MemoryType};
use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};
use uefi::table::cfg::ConfigTableEntry;
use uefi::{cstr16, println};

const PAGE_SIZE: u64 = 4096;

static mut BOOT_INFO: MaybeUninit<BootInfo> = MaybeUninit::uninit();

#[derive(Clone, Copy)]
struct LoadedKernel {
    entry: u64,
    start: u64,
    end: u64,
}

#[entry]
fn main() -> Status {
    uefi::helpers::init().expect("UEFI helper initialization failed");
    println!("MakOS loader v0.1");

    let (kernel, config, config_length) = {
        let sfs =
            boot::get_image_file_system(boot::image_handle()).expect("boot filesystem unavailable");
        let mut fs = FileSystem::new(sfs);
        let image = fs
            .read(cstr16!("\\KERNEL.ELF"))
            .expect("KERNEL.ELF unavailable");
        let loaded = load_kernel(&image).expect("invalid or unloadable KERNEL.ELF");
        println!(
            "Kernel loaded at {:#x}..{:#x}; entry {:#x}",
            loaded.start, loaded.end, loaded.entry
        );
        let file = fs
            .read(cstr16!("\\MAKOS.CFG"))
            .expect("MAKOS.CFG unavailable");
        if file.is_empty() || file.len() > BOOT_CONFIG_CAPACITY {
            panic!("MAKOS.CFG has invalid length");
        }
        let mut config = [0u8; BOOT_CONFIG_CAPACITY];
        config[..file.len()].copy_from_slice(&file);
        println!("Boot config loaded; {} bytes", file.len());
        (loaded, config, file.len() as u32)
    };

    let framebuffer = discover_framebuffer();
    let acpi_rsdp = discover_acpi_rsdp();
    println!("Exiting UEFI boot services");

    // All protocol guards and allocation-owning Vecs are gone before this.
    let memory_map = unsafe { boot::exit_boot_services(None) };
    let meta = memory_map.meta();
    let map_ptr = memory_map.buffer().as_ptr() as u64;

    let info = BootInfo {
        framebuffer,
        memory_map: MemoryMapInfo {
            address: map_ptr,
            entry_count: meta.entry_count() as u64,
            descriptor_size: meta.desc_size as u32,
            descriptor_version: meta.desc_version,
        },
        acpi_rsdp,
        kernel_physical_start: kernel.start,
        kernel_physical_end: kernel.end,
        config_length,
        config,
        ..BootInfo::new()
    };

    let info_ptr = (&raw mut BOOT_INFO).cast::<BootInfo>();
    unsafe { info_ptr.write(info) };
    // Backing pool contains final memory map. Kernel owns it after handoff.
    core::mem::forget(memory_map);

    call_kernel(kernel.entry, info_ptr.cast_const())
}

#[cfg(target_arch = "x86_64")]
fn call_kernel(entry: u64, info: *const BootInfo) -> ! {
    let entry: extern "sysv64" fn(*const BootInfo) -> ! =
        unsafe { core::mem::transmute(entry as usize) };
    entry(info)
}

#[cfg(target_arch = "aarch64")]
fn call_kernel(entry: u64, info: *const BootInfo) -> ! {
    let entry: extern "C" fn(*const BootInfo) -> ! =
        unsafe { core::mem::transmute(entry as usize) };
    entry(info)
}

fn load_kernel(image: &[u8]) -> Result<LoadedKernel, &'static str> {
    let elf =
        Elf64::parse_for_machine(image, kernel_machine()).map_err(|_| "ELF validation failed")?;
    let mut first = u64::MAX;
    let mut last = 0u64;
    let mut load_count = 0usize;

    for ph in elf.program_headers().filter(|p| p.segment_type == PT_LOAD) {
        if ph.physical_address != ph.virtual_address {
            return Err("initial kernel requires identity physical addresses");
        }
        let segment_end = ph
            .physical_address
            .checked_add(ph.memory_size)
            .ok_or("segment address overflow")?;
        first = first.min(ph.physical_address);
        last = last.max(segment_end);
        load_count += 1;
    }
    if load_count == 0 || first == u64::MAX || last <= first {
        return Err("no nonempty PT_LOAD span");
    }

    let allocation_start = first & !(PAGE_SIZE - 1);
    let allocation_end = last
        .checked_add(PAGE_SIZE - 1)
        .ok_or("kernel span overflow")?
        & !(PAGE_SIZE - 1);
    if elf.entry() < allocation_start || elf.entry() >= allocation_end {
        return Err("entry outside load span");
    }
    let pages = usize::try_from((allocation_end - allocation_start) / PAGE_SIZE)
        .map_err(|_| "kernel too large")?;
    let allocation = boot::allocate_pages(
        AllocateType::Address(allocation_start),
        MemoryType::LOADER_DATA,
        pages,
    )
    .map_err(|_| "physical kernel allocation failed")?;
    if allocation.as_ptr() as u64 != allocation_start {
        return Err("firmware returned wrong physical address");
    }

    unsafe {
        ptr::write_bytes(
            allocation.as_ptr(),
            0,
            usize::try_from(allocation_end - allocation_start).unwrap(),
        );
    }
    for ph in elf.program_headers().filter(|p| p.segment_type == PT_LOAD) {
        if ph.file_size == 0 {
            continue;
        }
        let src_start = usize::try_from(ph.offset).map_err(|_| "segment offset too large")?;
        let byte_count = usize::try_from(ph.file_size).map_err(|_| "segment too large")?;
        let destination = usize::try_from(ph.physical_address)
            .map_err(|_| "physical address too large")? as *mut u8;
        unsafe {
            ptr::copy_nonoverlapping(image.as_ptr().add(src_start), destination, byte_count);
        }
    }

    Ok(LoadedKernel {
        entry: elf.entry(),
        start: allocation_start,
        end: allocation_end,
    })
}

#[cfg(target_arch = "x86_64")]
const fn kernel_machine() -> u16 {
    EM_X86_64
}

#[cfg(target_arch = "aarch64")]
const fn kernel_machine() -> u16 {
    EM_AARCH64
}

fn discover_framebuffer() -> FramebufferInfo {
    let Ok(handle) = boot::get_handle_for_protocol::<GraphicsOutput>() else {
        println!("GOP unavailable");
        return FramebufferInfo::EMPTY;
    };
    let Ok(mut gop) = boot::open_protocol_exclusive::<GraphicsOutput>(handle) else {
        println!("GOP open failed");
        return FramebufferInfo::EMPTY;
    };
    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();
    let format = match mode.pixel_format() {
        PixelFormat::Rgb => BootPixelFormat::Rgb,
        PixelFormat::Bgr => BootPixelFormat::Bgr,
        PixelFormat::Bitmask => BootPixelFormat::Bitmask,
        PixelFormat::BltOnly => BootPixelFormat::BltOnly,
    };
    if format == BootPixelFormat::BltOnly {
        println!("GOP mode has no linear framebuffer");
        return FramebufferInfo::EMPTY;
    }
    let mut fb = gop.frame_buffer();
    FramebufferInfo {
        address: fb.as_mut_ptr() as u64,
        byte_len: fb.size() as u64,
        width: width as u32,
        height: height as u32,
        stride: mode.stride() as u32,
        pixel_format: format,
    }
}

fn discover_acpi_rsdp() -> u64 {
    uefi::system::with_config_table(|entries| {
        entries
            .iter()
            .find(|entry| entry.guid == ConfigTableEntry::ACPI2_GUID)
            .or_else(|| {
                entries
                    .iter()
                    .find(|entry| entry.guid == ConfigTableEntry::ACPI_GUID)
            })
            .map_or(0, |entry| entry.address as u64)
    })
}
