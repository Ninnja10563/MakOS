use core::arch::asm;

mod apic;
mod gdt;
mod interrupts;
mod paging;
mod smp;
mod user;

pub use apic::init as init_local_apic;
pub use gdt::init as init_cpu_tables;
pub use gdt::set_ring0_stack;
pub use interrupts::enable_legacy_input_irqs;
pub use interrupts::ticks as monotonic_ticks;
pub(crate) use interrupts::{SavedRegisters, TrapFrame};
pub use interrupts::{enable as enable_interrupts, init as init_interrupts};
pub use paging::init as init_paging;
pub use paging::map_user_page_in;
pub use paging::{
    destroy_user_address_space, protect_user_page, unmap_user_page, user_page_writable,
};
pub use paging::{new_user_address_space, switch_address_space};
pub use smp::init as init_smp;
pub use user::{enter as enter_user, enter_startup as enter_user_startup};

#[inline]
pub fn disable_interrupts() {
    unsafe { asm!("cli", options(nomem, nostack, preserves_flags)) }
}

pub fn halt_forever() -> ! {
    disable_interrupts();
    loop {
        unsafe { asm!("hlt", options(nomem, nostack)) }
    }
}

#[inline]
pub(crate) unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
pub(crate) unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
    }
    value
}

#[inline]
pub(crate) unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    unsafe {
        asm!("in eax, dx", out("eax") value, in("dx") port, options(nomem, nostack, preserves_flags));
    }
    value
}

#[inline]
pub(crate) unsafe fn outl(port: u16, value: u32) {
    unsafe {
        asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags));
    }
}
