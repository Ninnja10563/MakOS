use super::{inb, outb};
use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const IDT_ENTRIES: usize = 256;
const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xa0;
const PIC2_DATA: u16 = 0xa1;
const PIT_COMMAND: u16 = 0x43;
const PIT_CHANNEL0: u16 = 0x40;
const TIMER_HZ: u32 = 100;

static TICKS: AtomicU64 = AtomicU64::new(0);
static BREAKPOINTS: AtomicU64 = AtomicU64::new(0);
static PAGE_FAULT_TESTS: AtomicU64 = AtomicU64::new(0);
static PAGE_FAULT_EXPECTED: AtomicBool = AtomicBool::new(false);
static PAGE_FAULT_RESUME: AtomicU64 = AtomicU64::new(0);
static M2_REPORTED: AtomicBool = AtomicBool::new(false);
static mut IDT: [IdtEntry; IDT_ENTRIES] = [IdtEntry::MISSING; IDT_ENTRIES];

#[derive(Clone, Copy)]
#[repr(C, packed)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    attributes: u8,
    offset_middle: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const MISSING: Self = Self {
        offset_low: 0,
        selector: 0,
        ist: 0,
        attributes: 0,
        offset_middle: 0,
        offset_high: 0,
        reserved: 0,
    };

    fn interrupt_gate(handler: usize, selector: u16) -> Self {
        Self {
            offset_low: handler as u16,
            selector,
            ist: 0,
            attributes: 0x8e,
            offset_middle: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }

    fn user_gate(handler: usize, selector: u16) -> Self {
        let mut gate = Self::interrupt_gate(handler, selector);
        gate.attributes = 0xee;
        gate
    }
}

#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct TrapFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
}

#[repr(C)]
pub(crate) struct SavedRegisters {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rax: u64,
}

unsafe extern "C" {
    fn makos_isr3();
    fn makos_isr8();
    fn makos_isr13();
    fn makos_isr14();
    fn makos_irq32();
    fn makos_irq33();
    fn makos_irq44();
    fn makos_isr128();
    fn makos_irq255();
    fn makos_page_fault_probe();
}

global_asm!(
    r#"
.global makos_isr3
makos_isr3:
    push 0
    push 3
    jmp makos_interrupt_common

.global makos_isr8
makos_isr8:
    push 8
    jmp makos_interrupt_common

.global makos_isr13
makos_isr13:
    push 13
    jmp makos_interrupt_common

.global makos_isr14
makos_isr14:
    push 14
    jmp makos_interrupt_common

.global makos_irq32
makos_irq32:
    push 0
    push 32
    jmp makos_interrupt_common

.global makos_irq33
makos_irq33:
    push 0
    push 33
    jmp makos_interrupt_common

.global makos_irq44
makos_irq44:
    push 0
    push 44
    jmp makos_interrupt_common

.global makos_isr128
makos_isr128:
    push 0
    push 128
    jmp makos_interrupt_common

.global makos_irq255
makos_irq255:
    push 0
    push 255
    jmp makos_interrupt_common

.global makos_page_fault_probe
makos_page_fault_probe:
    sub rsp, 8
    lea rax, [rip + .Lpage_fault_resume]
    mov rdi, rax
    call makos_expect_page_fault
    add rsp, 8
    mov rax, 0x100000000
    mov rax, [rax]
.Lpage_fault_resume:
    ret

makos_interrupt_common:
    cld
    push rax
    push rcx
    push rdx
    push rbx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    mov rdi, [rsp + 120]
    mov rsi, [rsp + 128]
    lea rdx, [rsp + 136]
    mov rcx, rsp
    mov r12, rsp
    and rsp, -16
    call makos_interrupt_dispatch
    mov rsp, r12

    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rbx
    pop rdx
    pop rcx
    pop rax
    add rsp, 16
    iretq
"#
);

pub fn init() {
    let selector = read_code_selector();
    let idt = (&raw mut IDT).cast::<IdtEntry>();
    unsafe {
        idt.add(3)
            .write(IdtEntry::interrupt_gate(makos_isr3 as usize, selector));
        idt.add(8)
            .write(IdtEntry::interrupt_gate(makos_isr8 as usize, selector));
        idt.add(13)
            .write(IdtEntry::interrupt_gate(makos_isr13 as usize, selector));
        idt.add(14)
            .write(IdtEntry::interrupt_gate(makos_isr14 as usize, selector));
        idt.add(32)
            .write(IdtEntry::interrupt_gate(makos_irq32 as usize, selector));
        idt.add(33)
            .write(IdtEntry::interrupt_gate(makos_irq33 as usize, selector));
        idt.add(44)
            .write(IdtEntry::interrupt_gate(makos_irq44 as usize, selector));
        idt.add(128)
            .write(IdtEntry::user_gate(makos_isr128 as usize, selector));
        idt.add(255)
            .write(IdtEntry::interrupt_gate(makos_irq255 as usize, selector));
    }
    let pointer = IdtPointer {
        limit: (core::mem::size_of::<[IdtEntry; IDT_ENTRIES]>() - 1) as u16,
        base: idt as u64,
    };
    unsafe { asm!("lidt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags)) };

    // Prove exception entry/restore before enabling external interrupts.
    unsafe { asm!("int3", options(nomem, nostack)) };
    if BREAKPOINTS.load(Ordering::Acquire) != 1 {
        crate::fatal("IDT breakpoint self-test failed");
    }

    remap_and_mask_pic();
    program_pit();
    crate::serial_println!("cpu idt=ok breakpoint=ok pit_hz={}", TIMER_HZ);
}

pub fn enable() {
    unsafe { asm!("sti", options(nomem, nostack, preserves_flags)) }
}

pub(super) fn page_fault_self_test() {
    unsafe { makos_page_fault_probe() };
    if PAGE_FAULT_EXPECTED.load(Ordering::Acquire) || PAGE_FAULT_TESTS.load(Ordering::Acquire) != 1
    {
        crate::fatal("page-fault recovery self-test failed");
    }
}

#[unsafe(no_mangle)]
extern "C" fn makos_expect_page_fault(resume: u64) {
    PAGE_FAULT_RESUME.store(resume, Ordering::Release);
    PAGE_FAULT_EXPECTED.store(true, Ordering::Release);
}

#[unsafe(no_mangle)]
extern "C" fn makos_interrupt_dispatch(
    vector: u64,
    error: u64,
    frame: *const TrapFrame,
    registers: *mut SavedRegisters,
) {
    match vector {
        3 => {
            BREAKPOINTS.fetch_add(1, Ordering::Release);
        }
        32 => timer_interrupt(),
        33 => {
            crate::drivers::ps2::interrupt();
            unsafe { outb(PIC1_COMMAND, 0x20) };
        }
        44 => {
            crate::drivers::ps2::interrupt();
            unsafe {
                outb(PIC2_COMMAND, 0x20);
                outb(PIC1_COMMAND, 0x20);
            }
        }
        128 => {
            if unsafe { (*frame).cs } & 3 != 3 {
                crate::fatal("int80 called outside ring 3");
            }
            if crate::scheduler::current_pid() == 3 {
                crate::compat::dispatch_linux(unsafe { &mut *registers });
            } else if crate::scheduler::current_pid() == 4 {
                crate::compat::dispatch_windows(unsafe { &mut *registers });
            } else {
                crate::syscall::dispatch(unsafe { &mut *registers }, unsafe {
                    &mut *(frame as *mut TrapFrame)
                });
            }
        }
        255 => {}
        8 | 13 | 14 => {
            let mut frame_value = unsafe { frame.read_unaligned() };
            let mut fault_address = 0;
            if vector == 14 {
                let cr2: u64;
                unsafe {
                    asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags))
                };
                if cr2 == 0x1_0000_0000
                    && error & 1 == 0
                    && PAGE_FAULT_EXPECTED.swap(false, Ordering::AcqRel)
                {
                    frame_value.rip = PAGE_FAULT_RESUME.load(Ordering::Acquire);
                    unsafe { (frame as *mut TrapFrame).write_unaligned(frame_value) };
                    PAGE_FAULT_TESTS.fetch_add(1, Ordering::Release);
                    return;
                }
                fault_address = cr2;
                crate::serial_println!(
                    "MAKOS_EXCEPTION vector={} error={:#x} rip={:#x} cr2={:#x}",
                    vector,
                    error,
                    frame_value.rip,
                    cr2
                );
            } else {
                crate::serial_println!(
                    "MAKOS_EXCEPTION vector={} error={:#x} rip={:#x} cs={:#x} rflags={:#x}",
                    vector,
                    error,
                    frame_value.rip,
                    frame_value.cs,
                    frame_value.rflags
                );
            }
            if vector != 8 && frame_value.cs & 3 == 3 {
                crate::process::fault_current(vector, error, frame_value.rip, fault_address);
            }
            crate::arch::halt_forever();
        }
        _ => crate::fatal("unexpected interrupt vector"),
    }
}

fn timer_interrupt() {
    let tick = TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    unsafe { outb(PIC1_COMMAND, 0x20) };

    if tick >= 50 && !M2_REPORTED.swap(true, Ordering::AcqRel) {
        let (task_a, task_b) = crate::scheduler::task_counters();
        if task_a != 0 && task_b != 0 {
            crate::serial_println!(
                "MAKOS_M2_OK ticks={} task_a={} task_b={} free_frames={}",
                tick,
                task_a,
                task_b,
                crate::mm::free_frames()
            );
        } else {
            crate::fatal("scheduler task progress self-test failed");
        }
    }
    crate::scheduler::on_tick(tick);
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Acquire)
}

fn read_code_selector() -> u16 {
    let selector: u16;
    unsafe { asm!("mov {0:x}, cs", out(reg) selector, options(nomem, nostack, preserves_flags)) };
    selector
}

fn remap_and_mask_pic() {
    unsafe {
        let master_mask = inb(PIC1_DATA);
        let slave_mask = inb(PIC2_DATA);
        outb(PIC1_COMMAND, 0x11);
        io_wait();
        outb(PIC2_COMMAND, 0x11);
        io_wait();
        outb(PIC1_DATA, 32);
        io_wait();
        outb(PIC2_DATA, 40);
        io_wait();
        outb(PIC1_DATA, 4);
        io_wait();
        outb(PIC2_DATA, 2);
        io_wait();
        outb(PIC1_DATA, 0x01);
        io_wait();
        outb(PIC2_DATA, 0x01);
        io_wait();
        let _ = (master_mask, slave_mask);
        // Enable IRQ0 only. All other hardware remains masked until drivers bind.
        outb(PIC1_DATA, 0xfe);
        outb(PIC2_DATA, 0xff);
    }
}

pub fn enable_legacy_input_irqs() {
    unsafe {
        // IRQ0 timer, IRQ1 keyboard, IRQ2 cascade; slave IRQ12 mouse.
        outb(PIC1_DATA, 0xf8);
        outb(PIC2_DATA, 0xef);
    }
}

fn program_pit() {
    let divisor = (1_193_182u32 / TIMER_HZ) as u16;
    unsafe {
        outb(PIT_COMMAND, 0x36);
        outb(PIT_CHANNEL0, divisor as u8);
        outb(PIT_CHANNEL0, (divisor >> 8) as u8);
    }
}

unsafe fn io_wait() {
    unsafe { outb(0x80, 0) }
}
