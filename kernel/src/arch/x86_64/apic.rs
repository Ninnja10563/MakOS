use core::arch::{asm, x86_64::__cpuid};
use core::ptr;

const IA32_APIC_BASE: u32 = 0x1b;
const APIC_GLOBAL_ENABLE: u64 = 1 << 11;
const APIC_BASE_MASK: u64 = 0xffff_f000;
const APIC_ID: usize = 0x20;
const APIC_ICR_LOW: usize = 0x300;
const APIC_ICR_HIGH: usize = 0x310;
const APIC_SPURIOUS: usize = 0xf0;

pub fn init(madt_address: u64) -> u32 {
    let features = unsafe { __cpuid(1) };
    if features.edx & (1 << 9) == 0 {
        crate::fatal("CPU lacks local APIC");
    }
    let mut apic_base = unsafe { rdmsr(IA32_APIC_BASE) };
    let msr_address = apic_base & APIC_BASE_MASK;
    if msr_address != madt_address {
        crate::fatal("MADT/local-APIC MSR address mismatch");
    }
    apic_base |= APIC_GLOBAL_ENABLE;
    unsafe { wrmsr(IA32_APIC_BASE, apic_base) };

    let base = madt_address as *mut u32;
    let spurious = unsafe { read(base, APIC_SPURIOUS) };
    unsafe { write(base, APIC_SPURIOUS, (spurious & !0xff) | 0x100 | 0xff) };
    let id = unsafe { read(base, APIC_ID) } >> 24;
    crate::serial_println!("apic id={} base={:#x} enabled=1", id, madt_address);
    id
}

pub(super) fn start_ap(base_address: u64, apic_id: u32, vector: u8) {
    if apic_id > 255 {
        crate::fatal("x2APIC startup not implemented");
    }
    let base = base_address as *mut u32;
    unsafe {
        send_ipi(base, apic_id, 0x0000_c500); // INIT, assert, level-triggered
        delay();
        send_ipi(base, apic_id, 0x0000_8500); // INIT deassert
        delay();
        send_ipi(base, apic_id, 0x0000_0600 | u32::from(vector));
        delay();
        send_ipi(base, apic_id, 0x0000_0600 | u32::from(vector));
    }
}

unsafe fn send_ipi(base: *mut u32, apic_id: u32, command: u32) {
    while unsafe { read(base, APIC_ICR_LOW) } & (1 << 12) != 0 {
        core::hint::spin_loop();
    }
    unsafe {
        write(base, APIC_ICR_HIGH, apic_id << 24);
        write(base, APIC_ICR_LOW, command);
    }
    while unsafe { read(base, APIC_ICR_LOW) } & (1 << 12) != 0 {
        core::hint::spin_loop();
    }
}

unsafe fn delay() {
    for _ in 0..1_000_000 {
        core::hint::spin_loop();
    }
}

unsafe fn read(base: *mut u32, register: usize) -> u32 {
    unsafe { ptr::read_volatile(base.add(register / 4)) }
}

unsafe fn write(base: *mut u32, register: usize, value: u32) {
    unsafe {
        ptr::write_volatile(base.add(register / 4), value);
        let _ = ptr::read_volatile(base.add(APIC_ID / 4));
    }
}

unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high, options(nomem, nostack, preserves_flags))
    };
    (u64::from(high) << 32) | u64::from(low)
}

unsafe fn wrmsr(msr: u32, value: u64) {
    unsafe {
        asm!("wrmsr", in("ecx") msr, in("eax") value as u32, in("edx") (value >> 32) as u32, options(nomem, nostack, preserves_flags));
    }
}
