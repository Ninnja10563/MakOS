#![no_main]
#![no_std]

extern crate alloc;

#[cfg(target_arch = "aarch64")]
mod aarch64_accounts;
#[cfg(target_arch = "aarch64")]
mod aarch64_clipboard;
#[cfg(target_arch = "aarch64")]
mod aarch64_desktop;
#[cfg(target_arch = "aarch64")]
mod aarch64_epoll;
#[cfg(target_arch = "aarch64")]
mod aarch64_installer;
#[cfg(target_arch = "aarch64")]
mod aarch64_net_wire;
#[cfg(target_arch = "aarch64")]
mod aarch64_process;
#[cfg(target_arch = "aarch64")]
mod aarch64_rtc;
mod aarch64_shmem;
#[cfg(target_arch = "aarch64")]
mod aarch64_socket;
#[cfg(target_arch = "aarch64")]
mod aarch64_tty;
#[cfg(target_arch = "aarch64")]
mod aarch64_virtio_blk;
#[cfg(target_arch = "aarch64")]
mod aarch64_virtio_gpu;
#[cfg(target_arch = "aarch64")]
mod aarch64_virtio_input;
#[cfg(target_arch = "aarch64")]
mod aarch64_virtio_net;
#[cfg(target_arch = "aarch64")]
mod aarch64_virtio_rng;
#[cfg(target_arch = "aarch64")]
mod aarch64_vm;
mod acpi;
mod arch;
mod block;
#[cfg(target_arch = "x86_64")]
mod compat;
#[cfg(target_arch = "x86_64")]
mod drivers;
mod framebuffer;
mod fs;
mod graphics;
mod heap;
mod ipc;
mod log;
mod makfs4_volume;
mod mm;
mod package;
#[cfg(target_arch = "x86_64")]
mod process;
#[cfg(target_arch = "x86_64")]
mod scheduler;
mod security;
mod serial;
#[cfg(target_arch = "x86_64")]
mod socket;
#[cfg(target_arch = "x86_64")]
mod syscall;
mod vfs;
#[cfg(target_arch = "x86_64")]
mod vm;
#[cfg(target_arch = "x86_64")]
mod x86_64_installer;

use core::panic::PanicInfo;
use makos_boot_api::{BOOT_ABI_VERSION, BOOT_INFO_MAGIC, BootInfo, UefiMemoryDescriptor};

#[unsafe(no_mangle)]
#[cfg(target_arch = "x86_64")]
pub extern "sysv64" fn kernel_main(boot_ptr: *const BootInfo) -> ! {
    arch::disable_interrupts();
    serial::init();
    serial_println!("MakOS kernel: entry");

    let Some(boot) = (unsafe { boot_ptr.as_ref() }).copied() else {
        fatal("null BootInfo");
    };
    if boot.magic != BOOT_INFO_MAGIC {
        fatal("bad BootInfo magic");
    }
    if boot.abi_version != BOOT_ABI_VERSION
        || boot.struct_size < core::mem::size_of::<BootInfo>() as u32
    {
        fatal("unsupported BootInfo ABI");
    }
    let boot_options = parse_boot_config(&boot);

    let (regions, conventional_pages) = memory_summary(&boot);
    let conventional_mib = conventional_pages / 256;
    serial_println!(
        "boot_info abi={} regions={} conventional_mib={} acpi={:#x}",
        boot.abi_version,
        regions,
        conventional_mib,
        boot.acpi_rsdp
    );
    serial_println!(
        "kernel={:#x}..{:#x} framebuffer={}x{} stride={}",
        boot.kernel_physical_start,
        boot.kernel_physical_end,
        boot.framebuffer.width,
        boot.framebuffer.height,
        boot.framebuffer.stride
    );

    physical_memory_self_test(&boot);
    heap::init_and_test();
    security::self_test();
    arch::init_cpu_tables();

    if let Some(mut screen) = framebuffer::Screen::new(boot.framebuffer) {
        screen.clear(framebuffer::Color::new(10, 18, 38));
        screen.fill_rect(
            0,
            0,
            screen.width(),
            12,
            framebuffer::Color::new(42, 184, 255),
        );
        screen.draw_text(40, 54, 7, "MAKOS", framebuffer::Color::new(235, 247, 255));
        screen.draw_text(
            42,
            118,
            3,
            "KERNEL ONLINE",
            framebuffer::Color::new(76, 222, 156),
        );
        screen.draw_text(
            42,
            150,
            2,
            "UEFI X86_64",
            framebuffer::Color::new(154, 172, 205),
        );
    } else {
        serial_println!("framebuffer unavailable or unsupported");
    }
    graphics::init(boot.framebuffer);

    serial_println!("MAKOS_BOOT_OK m1");
    scheduler::init();
    arch::init_interrupts();
    arch::init_paging();
    let platform = acpi::discover(&boot);
    if platform.enabled_cpu_count == 0 {
        fatal("MADT reports no enabled CPUs");
    }
    let bootstrap_apic_id = arch::init_local_apic(platform.local_apic_address);
    serial_println!(
        "acpi cpus={} lapic={:#x} bsp_apic_id={} madt_flags={:#x}",
        platform.enabled_cpu_count,
        platform.local_apic_address,
        bootstrap_apic_id,
        platform.flags
    );
    arch::init_smp(&platform, bootstrap_apic_id);
    fs::mount_and_test(boot_options.recover_makfs);
    drivers::rtl8139::arp_self_test();
    drivers::ps2::init();
    drivers::usb_uhci::self_test();
    drivers::ac97::self_test();
    graphics::show_login();
    serial_println!("launching userspace init; scheduler timer armed");
    process::launch_init()
}

#[cfg(target_arch = "aarch64")]
#[unsafe(no_mangle)]
pub extern "C" fn aarch64_kernel_main(boot_ptr: *const BootInfo) -> ! {
    arch::disable_interrupts();
    serial::init();
    serial_println!("MakOS AArch64 kernel: entry");

    let Some(boot) = (unsafe { boot_ptr.as_ref() }).copied() else {
        fatal("null BootInfo");
    };
    if boot.magic != BOOT_INFO_MAGIC
        || boot.abi_version != BOOT_ABI_VERSION
        || boot.struct_size < core::mem::size_of::<BootInfo>() as u32
    {
        fatal("unsupported BootInfo ABI");
    }
    let boot_options = parse_boot_config(&boot);
    let (regions, conventional_pages) = memory_summary(&boot);
    serial_println!(
        "boot_info abi={} arch=aarch64 regions={} conventional_mib={} dtb_or_acpi={:#x}",
        boot.abi_version,
        regions,
        conventional_pages / 256,
        boot.acpi_rsdp,
    );
    physical_memory_self_test(&boot);
    heap::init_and_test();
    security::self_test();
    let mmu = arch::init_mmu(boot.kernel_physical_start, boot.kernel_physical_end);
    serial_println!(
        "paging arch=aarch64 ttbr0={:#x} tcr={:#x} mair={:#x} identity_gib=2 owned=1",
        mmu.ttbr0,
        mmu.tcr,
        mmu.mair,
    );
    let platform = acpi::discover_arm(&boot);
    serial_println!(
        "acpi arch=aarch64 cpus={} gic_version={} gicd={:#x} gicc={:#x} virtual_timer_gsiv={}",
        platform.interrupt.enabled_cpu_count,
        platform.interrupt.gic_version,
        platform.interrupt.gic_distributor_address,
        platform.interrupt.gic_cpu_interface_address,
        platform.timer.virtual_el1_gsiv,
    );
    arch::init_exceptions();
    serial_println!("MAKOS_AARCH64_VECTOR_READY vbar=owned alignment=2048");
    arch::exception_self_test();
    serial_println!("MAKOS_AARCH64_EXCEPTION_OK brk_return=1 frame=complete");
    let timer = arch::init_timer(
        platform.interrupt.gic_distributor_address,
        platform.interrupt.gic_cpu_interface_address,
        platform.interrupt.gic_version,
        platform.timer.virtual_el1_gsiv,
        platform.timer.virtual_el1_flags,
    );
    let smp = arch::init_smp(&platform.interrupt);
    serial_println!(
        "MAKOS_AARCH64_CORE_OK el=1 stack=owned mmu=owned vectors=1 brk_return=1 gic={} timer_hz=100 timer_freq={} ticks={} acpi=1 smp_online={} smp_aps={} psci={}.{}",
        platform.interrupt.gic_version,
        timer.frequency,
        timer.ticks,
        smp.online_cpus,
        smp.secondary_cpus,
        smp.psci_major,
        smp.psci_minor,
    );
    aarch64_vm::initialize();
    aarch64_shmem::initialize();
    let process = aarch64_process::run_init_self_test();
    serial_println!(
        "userspace arch=aarch64 exit={} reclaimed_frames={} svc_abi=aarch64",
        process.exit_status,
        process.reclaimed_frames,
    );
    aarch64_process::run_smp_userspace_self_test();
    aarch64_process::run_smp_forced_migration_self_test();
    aarch64_process::run_smp_load_balancing_self_test();
    aarch64_process::run_smp_ipc_self_test();
    aarch64_process::run_smp_exit_group_self_test();
    aarch64_process::run_smp_exit_group_el1_self_test();
    aarch64_process::run_smp_concurrent_exit_group_self_test();
    aarch64_process::run_smp_same_group_exit_self_test();

    aarch64_virtio_blk::init();
    fs::mount_and_test(boot_options.recover_makfs);
    aarch64_accounts::initialize();
    aarch64_virtio_net::init();
    aarch64_virtio_rng::init();
    aarch64_rtc::init();

    let framebuffer = aarch64_virtio_gpu::init(800, 600);
    aarch64_desktop::initialize(framebuffer);
    if boot_options.smp_input_probe {
        aarch64_process::run_smp_gpu_self_test();
        aarch64_process::run_smp_block_io_self_test();
        aarch64_process::run_smp_network_rx_self_test();
        aarch64_process::run_smp_input_device_self_test();
    }
    if boot_options.smp_tcp_probe {
        aarch64_process::run_smp_tcp_tx_self_test();
    }
    aarch64_tty::initialize();
    serial_println!(
        "MAKOS_AARCH64_BOOT_OK uefi=1 hvf_ready=1 native_isa=1 framebuffer={}x{} gpu=virtio pmm=1 heap=1 mmu=1 exceptions=1 gic=2 timer=1 userspace=1 svc=1 input=virtio desktop=login",
        framebuffer.width,
        framebuffer.height,
    );
    aarch64_process::run_desktop_shell()
}

#[derive(Clone, Copy)]
struct BootOptions {
    recover_makfs: bool,
    smp_input_probe: bool,
    smp_tcp_probe: bool,
}

fn parse_boot_config(boot: &BootInfo) -> BootOptions {
    let length = boot.config_length as usize;
    if length == 0 || length > boot.config.len() {
        fatal("invalid boot config length");
    }
    let config = core::str::from_utf8(&boot.config[..length])
        .unwrap_or_else(|_| fatal("boot config is not UTF-8"));
    let mut root_ata1 = false;
    let mut serial_log = false;
    let mut recover_makfs = false;
    let mut smp_input_probe = false;
    let mut smp_tcp_probe = false;
    for option in config.split_ascii_whitespace() {
        match option {
            "root=ata1" => root_ata1 = true,
            "log=serial" => serial_log = true,
            "makfs.recover=auto" => recover_makfs = true,
            "test.smp-input=required" => smp_input_probe = true,
            "test.smp-tcp=required" => smp_tcp_probe = true,
            _ => fatal("unsupported boot config option"),
        }
    }
    if !root_ata1 || !serial_log || !recover_makfs {
        fatal("required boot config option absent");
    }
    serial_println!(
        "MAKOS_CONFIG_OK source=fat bytes={} root=ata1 log=serial makfs_recover=auto smp_input_probe={} smp_tcp_probe={}",
        length,
        u8::from(smp_input_probe),
        u8::from(smp_tcp_probe),
    );
    BootOptions {
        recover_makfs,
        smp_input_probe,
        smp_tcp_probe,
    }
}

fn physical_memory_self_test(boot: &BootInfo) {
    let stats = mm::init(boot);
    let before = mm::free_frames();
    let first = mm::allocate_frame().unwrap_or_else(|| fatal("frame allocation 1 failed"));
    let second = mm::allocate_frame().unwrap_or_else(|| fatal("frame allocation 2 failed"));
    let third = mm::allocate_frame().unwrap_or_else(|| fatal("frame allocation 3 failed"));
    if first == second
        || first == third
        || second == third
        || first % makos_frame_allocator::PAGE_SIZE != 0
        || second % makos_frame_allocator::PAGE_SIZE != 0
        || third % makos_frame_allocator::PAGE_SIZE != 0
    {
        fatal("frame allocator uniqueness/alignment failure");
    }
    if mm::free_frame(first).is_err()
        || mm::free_frame(second).is_err()
        || mm::free_frame(third).is_err()
        || mm::free_frames() != before
    {
        fatal("frame allocator release/accounting failure");
    }
    serial_println!(
        "pmm managed_mib={} free_frames={} self_test=ok",
        stats.managed_mib,
        stats.free_frames
    );
}

fn memory_summary(boot: &BootInfo) -> (u64, u64) {
    let map = boot.memory_map;
    if map.address == 0
        || map.descriptor_size < core::mem::size_of::<UefiMemoryDescriptor>() as u32
        || map.entry_count > 16_384
    {
        fatal("invalid UEFI memory map");
    }

    let mut conventional_pages = 0u64;
    for i in 0..map.entry_count {
        let offset = i
            .checked_mul(map.descriptor_size as u64)
            .and_then(|n| map.address.checked_add(n))
            .unwrap_or_else(|| fatal("UEFI memory map overflow"));
        let descriptor = unsafe { (offset as *const UefiMemoryDescriptor).read_unaligned() };
        // EFI_CONVENTIONAL_MEMORY = 7.
        if descriptor.memory_type == 7 {
            conventional_pages = conventional_pages.saturating_add(descriptor.page_count);
        }
    }
    (map.entry_count, conventional_pages)
}

fn fatal(message: &str) -> ! {
    serial_println!("MAKOS_FATAL: {}", message);
    arch::halt_forever()
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    serial_println!("MAKOS_PANIC: {}", info);
    arch::halt_forever()
}
