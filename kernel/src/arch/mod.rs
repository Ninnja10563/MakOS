#[cfg(target_arch = "aarch64")]
mod aarch64;
#[cfg(target_arch = "x86_64")]
mod x86_64;

#[cfg(target_arch = "aarch64")]
pub(crate) use aarch64::{
    ExceptionFrame, UserContext, counter_deadline_expired, counter_deadline_millis, cpu_index,
    disable_smp_probe_scheduler, enable_smp_probe_scheduler, enter_user_context,
    idle_secondary_after_smp_probe, input_service_affinity_evidence,
    network_rx_affinity_evidence, reset_input_service_affinity_evidence,
    reset_network_rx_affinity_evidence, return_to_kernel, send_scheduler_ipi,
    service_input_on_owner_cpu, service_network_rx_on_owner_cpu, smp_probe_scheduler_enabled,
    start_scheduler_timer, stop_scheduler_timer, user_range_readable, user_range_writable,
};
#[cfg(target_arch = "aarch64")]
pub use aarch64::{
    USER_ADDRESS_BASE, USER_HEAP_BASE, USER_HEAP_LIMIT, USER_IMAGE_LIMIT, USER_MMAP_BASE,
    USER_MMAP_LIMIT, USER_STACK_BOTTOM, USER_STACK_TOP, clone_user_address_space_eager,
    destroy_user_address_space, disable_interrupts, enable_interrupts, exception_self_test,
    halt_forever, init_exceptions, init_mmu, init_smp, init_timer, kernel_root, map_user_page_in,
    map_user_page_permissions_in, monotonic_ticks, new_user_address_space, protect_user_page_in,
    protect_user_page_permissions_in, switch_address_space, sync_user_code, unmap_user_page_in,
    uptime_millis, user_address_executable, user_page_physical_in, user_resident_pages,
};

#[cfg(target_arch = "x86_64")]
pub(crate) use x86_64::{SavedRegisters, TrapFrame};
#[cfg(target_arch = "x86_64")]
pub use x86_64::{
    destroy_user_address_space, disable_interrupts, enable_interrupts, enable_legacy_input_irqs,
    enter_user, enter_user_startup, halt_forever, init_cpu_tables, init_interrupts,
    init_local_apic, init_paging, init_smp, map_user_page_in, monotonic_ticks,
    new_user_address_space, protect_user_page, set_ring0_stack, switch_address_space,
    unmap_user_page, user_page_writable,
};
#[cfg(target_arch = "x86_64")]
pub(crate) use x86_64::{inb, inl, outb, outl};
