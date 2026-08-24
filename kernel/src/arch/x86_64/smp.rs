use core::arch::{asm, global_asm};
use core::ptr;
use core::sync::atomic::{AtomicU32, Ordering, fence};
use makos_acpi::MadtInfo;

const TRAMPOLINE_BASE: usize = 0x8000;
const TRAMPOLINE_VECTOR: u8 = (TRAMPOLINE_BASE >> 12) as u8;
const MAX_CPUS: usize = 16;
const AP_STACK_SIZE: usize = 16 * 1024;

#[repr(C, align(16))]
struct ApStacks([[u8; AP_STACK_SIZE]; MAX_CPUS - 1]);

static AP_ONLINE: AtomicU32 = AtomicU32::new(1);
static mut AP_STACKS: ApStacks = ApStacks([[0; AP_STACK_SIZE]; MAX_CPUS - 1]);

unsafe extern "C" {
    static ap_trampoline_start: u8;
    static ap_trampoline_end: u8;
    static ap_pml4_slot: u8;
    static ap_stack_slot: u8;
    static ap_entry_slot: u8;
}

global_asm!(
    r#"
.section .text.ap_trampoline,"ax"
.code16
.balign 16
.global ap_trampoline_start
ap_trampoline_start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    lgdt [0x8f40]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    .byte 0xea
    .word 0x8000 + ap_protected - ap_trampoline_start
    .word 0x08

.code32
ap_protected:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov eax, cr4
    or eax, 0x20
    mov cr4, eax
    mov eax, [0x8f00]
    mov cr3, eax
    mov ecx, 0xc0000080
    rdmsr
    or eax, 0x100
    wrmsr
    mov eax, cr0
    or eax, 0x80000000
    mov cr0, eax
    .byte 0xea
    .long 0x8000 + ap_long_mode - ap_trampoline_start
    .word 0x18

.code64
ap_long_mode:
    mov rax, 0x8f08
    mov rsp, [rax]
    xor rbp, rbp
    mov rax, 0x8f10
    mov rax, [rax]
    call rax
.Lap_hang:
    cli
    hlt
    jmp .Lap_hang

.org 0xf00
.global ap_pml4_slot
ap_pml4_slot:
    .quad 0
.global ap_stack_slot
ap_stack_slot:
    .quad 0
.global ap_entry_slot
ap_entry_slot:
    .quad 0

.org 0xf20
ap_gdt:
    .quad 0x0000000000000000
    .quad 0x00cf9a000000ffff
    .quad 0x00cf92000000ffff
    .quad 0x00af9a000000ffff
ap_gdt_ptr:
    .word 31
    .long 0x8000 + ap_gdt - ap_trampoline_start

.global ap_trampoline_end
ap_trampoline_end:
.code64
"#
);

pub fn init(platform: &MadtInfo, bootstrap_apic_id: u32) {
    let cpu_count = platform.enabled_cpu_count as usize;
    if !(1..=MAX_CPUS).contains(&cpu_count) {
        crate::fatal("SMP CPU count exceeds early limit");
    }
    copy_trampoline();
    let mut ap_index = 0usize;
    for index in 0..cpu_count {
        let apic_id = platform.apic_ids[index];
        if apic_id == bootstrap_apic_id {
            continue;
        }
        let stack_base = (&raw mut AP_STACKS).cast::<u8>() as usize + ap_index * AP_STACK_SIZE;
        let stack_top = (stack_base + AP_STACK_SIZE) & !15usize;
        write_slot(&raw const ap_pml4_slot, super::paging::root_table());
        write_slot(&raw const ap_stack_slot, stack_top as u64);
        write_slot(&raw const ap_entry_slot, makos_ap_entry as usize as u64);
        fence(Ordering::SeqCst);
        super::apic::start_ap(platform.local_apic_address, apic_id, TRAMPOLINE_VECTOR);
        let expected = (ap_index + 2) as u32;
        let mut online = false;
        for _ in 0..50_000_000 {
            if AP_ONLINE.load(Ordering::Acquire) >= expected {
                online = true;
                break;
            }
            core::hint::spin_loop();
        }
        if !online {
            crate::fatal("AP startup timeout");
        }
        ap_index += 1;
    }
    let online = AP_ONLINE.load(Ordering::Acquire);
    if online != platform.enabled_cpu_count {
        crate::fatal("SMP online count mismatch");
    }
    crate::serial_println!("smp online={} init_sipi=ok", online);
}

fn copy_trampoline() {
    let source = &raw const ap_trampoline_start as *const u8;
    let end = &raw const ap_trampoline_end as *const u8;
    let length = end as usize - source as usize;
    if length > 4096 {
        crate::fatal("AP trampoline exceeds one page");
    }
    unsafe {
        ptr::copy_nonoverlapping(source, TRAMPOLINE_BASE as *mut u8, length);
        ptr::write_bytes((TRAMPOLINE_BASE + length) as *mut u8, 0, 4096 - length);
    }
}

fn write_slot(symbol: *const u8, value: u64) {
    let start = &raw const ap_trampoline_start as *const u8;
    let offset = symbol as usize - start as usize;
    unsafe { ptr::write_volatile((TRAMPOLINE_BASE + offset) as *mut u64, value) };
}

#[unsafe(no_mangle)]
extern "C" fn makos_ap_entry() -> ! {
    unsafe { asm!("cli", options(nomem, nostack, preserves_flags)) };
    AP_ONLINE.fetch_add(1, Ordering::Release);
    loop {
        unsafe { asm!("hlt", options(nomem, nostack)) }
    }
}
