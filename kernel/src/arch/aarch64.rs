use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use makos_acpi::ArmMadtInfo;

const PAGE_SIZE: u64 = 4096;
const BLOCK_SIZE: u64 = 2 * 1024 * 1024;
const RAM_BASE: u64 = 0x4000_0000;
const RAM_SIZE: u64 = 0x4000_0000;
const TABLE_DESCRIPTOR: u64 = 0b11;
const BLOCK_DESCRIPTOR: u64 = 0b01;
const ATTR_NORMAL: u64 = 0 << 2;
const ATTR_DEVICE: u64 = 1 << 2;
const SH_OUTER: u64 = 0b10 << 8;
const SH_INNER: u64 = 0b11 << 8;
const ACCESS_FLAG: u64 = 1 << 10;
const PXN: u64 = 1 << 53;
const UXN: u64 = 1 << 54;
const ADDRESS_MASK: u64 = 0x0000_ffff_ffff_f000;
const AP_USER_RW: u64 = 0b01 << 6;
const AP_USER_RO: u64 = 0b11 << 6;
pub const USER_ADDRESS_BASE: u64 = 0x1000_0000;
pub const USER_IMAGE_LIMIT: u64 = 0x1400_0000;
pub const USER_HEAP_BASE: u64 = 0x1400_0000;
pub const USER_HEAP_LIMIT: u64 = 0x1800_0000;
pub const USER_MMAP_BASE: u64 = 0x8000_0000;
pub const USER_MMAP_LIMIT: u64 = 0x3_c000_0000;
pub const USER_STACK_BOTTOM: u64 = 0x3_fe00_0000;
pub const USER_STACK_TOP: u64 = 0x4_0000_0000;
const USER_ADDRESS_LIMIT: u64 = USER_STACK_TOP;
const LEGACY_USER_STACK_TOP: u64 = 0x1020_0000;
const L1_SPAN: u64 = 1024 * 1024 * 1024;
const MAX_USER_COPY: usize = 16 * 1024 * 1024;

const GICD_CTLR: u64 = 0x000;
const GICD_TYPER: u64 = 0x004;
const GICD_IGROUPR0: u64 = 0x080;
const GICD_ISENABLER0: u64 = 0x100;
const GICD_ICENABLER0: u64 = 0x180;
const GICD_ICPENDR0: u64 = 0x280;
const GICD_IPRIORITYR: u64 = 0x400;
const GICD_ICFGR1: u64 = 0xc04;
const GICD_SGIR: u64 = 0xf00;
const SMP_SCHEDULER_SGI: u32 = 1;
const GICC_CTLR: u64 = 0x000;
const GICC_PMR: u64 = 0x004;
const GICC_BPR: u64 = 0x008;
const GICC_IAR: u64 = 0x00c;
const GICC_EOIR: u64 = 0x010;
const GICC_AIAR: u64 = 0x020;
const GICC_AEOIR: u64 = 0x024;

const EXCEPTION_FRAME_BYTES: usize = 832;
const BRK_SELF_TEST: u64 = 0x4d4b;
const MAX_AARCH64_CPUS: usize = 4;
const SECONDARY_CPU_COUNT: usize = MAX_AARCH64_CPUS - 1;
// APs execute the same deep syscall/teardown paths as the BSP. The former
// 64 KiB allocation overflowed during two simultaneous exit_group cleanups
// and clobbered the kernel-root word immediately below AP1's stack. Match the
// BSP's 1 MiB kernel stack before enabling broader multicore userspace.
const SECONDARY_STACK_BYTES: usize = 1024 * 1024;
const PSCI_VERSION: u64 = 0x8400_0000;
const PSCI_FEATURES: u64 = 0x8400_000a;
const PSCI_CPU_ON_64: u64 = 0xc400_0003;
// Aff3 is bits 39:32; Aff2:Aff1:Aff0 are bits 23:0. Exclude MPIDR U/MT
// state bits. (The superficially similar 0x00ff_00ff_ffff_ffff selects the
// wrong high byte and made the BSP appear absent from QEMU's ACPI MPIDR list.)
const MPIDR_AFFINITY_MASK: u64 = 0x0000_00ff_00ff_ffff;

#[repr(C, align(4096))]
struct PageTable {
    entries: [u64; 512],
}

static mut LEVEL0: PageTable = PageTable { entries: [0; 512] };
static mut LEVEL1: PageTable = PageTable { entries: [0; 512] };
static mut LOW_LEVEL2: PageTable = PageTable { entries: [0; 512] };
static mut RAM_LEVEL2: PageTable = PageTable { entries: [0; 512] };

static SYNC_SELF_TESTS: AtomicU64 = AtomicU64::new(0);
static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);
static TIMER_INTERVAL: AtomicU64 = AtomicU64::new(0);
static TIMER_FREQUENCY: AtomicU64 = AtomicU64::new(0);
static BOOT_COUNTER: AtomicU64 = AtomicU64::new(0);
static TIMER_INTID: AtomicU32 = AtomicU32::new(u32::MAX);
static TIMER_FLAGS: AtomicU32 = AtomicU32::new(0);
static GIC_DISTRIBUTOR_BASE: AtomicU64 = AtomicU64::new(0);
static GIC_CPU_BASE: AtomicU64 = AtomicU64::new(0);
static UNEXPECTED_IRQS: AtomicU64 = AtomicU64::new(0);
static IRQ_ENTRIES: AtomicU64 = AtomicU64::new(0);
static KERNEL_ROOT: AtomicU64 = AtomicU64::new(0);
static ACTIVE_USER_ROOTS: [AtomicU64; MAX_AARCH64_CPUS] =
    [const { AtomicU64::new(0) }; MAX_AARCH64_CPUS];
static FIREFOX_OPEN_TRACES: AtomicU64 = AtomicU64::new(0);
static FIREFOX_READDIR_TRACES: AtomicU64 = AtomicU64::new(0);
static FIREFOX_MISSING_SYSCALL_TRACES: AtomicU64 = AtomicU64::new(0);
static FIREFOX_BLIT_TRACES: AtomicU64 = AtomicU64::new(0);
static FIREFOX_FONT_FD: AtomicU64 = AtomicU64::new(u64::MAX);
static FIREFOX_FONT_IO_TRACES: AtomicU64 = AtomicU64::new(0);
static FIREFOX_MUTATION_TRACES: AtomicU64 = AtomicU64::new(0);
static INPUT_SERVICE_OWNER_ACTIVITY: AtomicU64 = AtomicU64::new(0);
static INPUT_SERVICE_NONOWNER_DEFERRALS: AtomicU64 = AtomicU64::new(0);
static NETWORK_RX_OWNER_FRAMES: AtomicU64 = AtomicU64::new(0);
static NETWORK_RX_NONOWNER_DEFERRALS: AtomicU64 = AtomicU64::new(0);
const FIREFOX_FILE_TRACE_LIMIT: u64 = 8;
const FIREFOX_MUTATION_TRACE_LIMIT: u64 = 16;
static SMP_ONLINE_MASK: AtomicU64 = AtomicU64::new(1);
static SMP_TEST_RUN: AtomicBool = AtomicBool::new(false);
static SMP_BSP_WORK_ACTIVE: AtomicBool = AtomicBool::new(false);
static SMP_TEST_READY_MASK: AtomicU64 = AtomicU64::new(0);
// Enabled only for the bounded boot-time EL0 multicore probe. General desktop
// and Firefox AP dispatch stays gated until remote teardown/device ownership
// is complete.
static SMP_USER_SCHEDULER_ENABLED: AtomicBool = AtomicBool::new(false);

#[repr(align(64))]
struct SmpWorkCounter(AtomicU64);

// One line per writer avoids AP-to-AP false sharing during the rendezvous.
static SMP_WORK_CPU1: SmpWorkCounter = SmpWorkCounter(AtomicU64::new(0));
static SMP_WORK_CPU2: SmpWorkCounter = SmpWorkCounter(AtomicU64::new(0));
static SMP_WORK_CPU3: SmpWorkCounter = SmpWorkCounter(AtomicU64::new(0));

#[repr(C, align(64))]
#[derive(Clone, Copy)]
struct SecondaryBootContext {
    stack_top: u64,
    ttbr0: u64,
    tcr: u64,
    mair: u64,
    sctlr: u64,
    logical_id: u64,
    expected_mpidr: u64,
}

impl SecondaryBootContext {
    const EMPTY: Self = Self {
        stack_top: 0,
        ttbr0: 0,
        tcr: 0,
        mair: 0,
        sctlr: 0,
        logical_id: 0,
        expected_mpidr: 0,
    };
}

#[repr(C, align(64))]
struct SecondaryStacks([[u8; SECONDARY_STACK_BYTES]; SECONDARY_CPU_COUNT]);

static mut SECONDARY_CONTEXTS: [SecondaryBootContext; SECONDARY_CPU_COUNT] =
    [SecondaryBootContext::EMPTY; SECONDARY_CPU_COUNT];
static mut SECONDARY_STACKS: SecondaryStacks =
    SecondaryStacks([[0; SECONDARY_STACK_BYTES]; SECONDARY_CPU_COUNT]);

#[repr(C)]
pub(crate) struct ExceptionFrame {
    pub(crate) registers: [u64; 31],
    pub(crate) elr: u64,
    pub(crate) spsr: u64,
    esr: u64,
    far: u64,
    pub(crate) sp_el0: u64,
    pub(crate) ttbr0: u64,
    pub(crate) tpidr_el0: u64,
    vector_registers: [u128; 32],
    fpcr: u64,
    fpsr: u64,
}

const _: () = assert!(core::mem::size_of::<ExceptionFrame>() == EXCEPTION_FRAME_BYTES);
const _: () = assert!(core::mem::offset_of!(ExceptionFrame, elr) == 248);
const _: () = assert!(core::mem::offset_of!(ExceptionFrame, spsr) == 256);
const _: () = assert!(core::mem::offset_of!(ExceptionFrame, sp_el0) == 280);
const _: () = assert!(core::mem::offset_of!(ExceptionFrame, ttbr0) == 288);
const _: () = assert!(core::mem::offset_of!(ExceptionFrame, tpidr_el0) == 296);
const _: () = assert!(core::mem::offset_of!(ExceptionFrame, vector_registers) == 304);
const _: () = assert!(core::mem::offset_of!(ExceptionFrame, fpcr) == 816);
const _: () = assert!(core::mem::offset_of!(ExceptionFrame, fpsr) == 824);

global_asm!(
    r#"
    .section .text.kernel_main,"ax",%progbits
    .balign 16
    .global kernel_main
    .type kernel_main,%function
kernel_main:
    msr tpidr_el1, xzr
    ldr x1, =__aarch64_boot_stack_top
    mov sp, x1
    b aarch64_kernel_main
    .size kernel_main, . - kernel_main

    // PSCI CPU_ON enters each secondary at this physical address with the
    // caller-supplied context pointer in x0 and its MMU disabled. All MakOS
    // kernel mappings are identity mappings, so one context contains both the
    // private stack and the exact EL1 translation regime copied from the BSP.
    .balign 64
    .global aarch64_secondary_entry
    .type aarch64_secondary_entry,%function
aarch64_secondary_entry:
    msr daifset, #0xf
    mov x19, x0
    ldr x1, [x19, #0]
    mov sp, x1
    ldr x2, [x19, #8]
    ldr x3, [x19, #16]
    ldr x4, [x19, #24]
    ldr x5, [x19, #32]
    msr mair_el1, x4
    msr tcr_el1, x3
    msr ttbr0_el1, x2
    tlbi vmalle1
    dsb sy
    isb
    ic iallu
    dsb sy
    isb
    msr sctlr_el1, x5
    isb
    mov x0, x19
    b aarch64_secondary_main
    .size aarch64_secondary_entry, . - aarch64_secondary_entry

    .section .bss.boot_stack,"aw",%nobits
    .balign 16
__aarch64_boot_stack:
    // EL1 currently shares this stack across boot, SVC, IRQ, loader, and
    // scheduler paths. Native ELF loading can exceed 64 KiB call depth.
    .space 1048576
__aarch64_boot_stack_top:

    .section .bss.aarch64_user_return,"aw",%nobits
    .balign 256
aarch64_saved_kernel_context:
    // One 256-byte callee-save/return record per supported PE. TPIDR_EL1 is
    // kernel-owned logical CPU ID; sharing this record would make concurrent
    // EL0 exits return on another CPU's stack.
    .space 1024

    .section .text.aarch64_user,"ax",%progbits
    .balign 16
    .global aarch64_enter_user_context
    .type aarch64_enter_user_context,%function
aarch64_enter_user_context:
    msr daifset, #0xf
    adrp x10, aarch64_saved_kernel_context
    add x10, x10, :lo12:aarch64_saved_kernel_context
    mrs x11, tpidr_el1
    add x10, x10, x11, lsl #8
    mov x9, sp
    str x9, [x10, #0]
    stp x19, x20, [x10, #8]
    stp x21, x22, [x10, #24]
    stp x23, x24, [x10, #40]
    stp x25, x26, [x10, #56]
    stp x27, x28, [x10, #72]
    stp x29, x30, [x10, #88]
    stp q8, q9, [x10, #112]
    stp q10, q11, [x10, #144]
    stp q12, q13, [x10, #176]
    stp q14, q15, [x10, #208]
    mrs x11, fpcr
    mrs x12, fpsr
    stp x11, x12, [x10, #240]
    mov x9, x0
    ldr x1, [x9, #248]
    ldr x2, [x9, #256]
    ldr x3, [x9, #280]
    msr elr_el1, x1
    msr spsr_el1, x2
    msr sp_el0, x3
    ldr x4, [x9, #296]
    msr tpidr_el0, x4
    ldp q0, q1, [x9, #304]
    ldp q2, q3, [x9, #336]
    ldp q4, q5, [x9, #368]
    ldp q6, q7, [x9, #400]
    ldp q8, q9, [x9, #432]
    ldp q10, q11, [x9, #464]
    ldp q12, q13, [x9, #496]
    ldp q14, q15, [x9, #528]
    ldp q16, q17, [x9, #560]
    ldp q18, q19, [x9, #592]
    ldp q20, q21, [x9, #624]
    ldp q22, q23, [x9, #656]
    ldp q24, q25, [x9, #688]
    ldp q26, q27, [x9, #720]
    ldp q28, q29, [x9, #752]
    ldp q30, q31, [x9, #784]
    ldr x1, [x9, #816]
    ldr x2, [x9, #824]
    msr fpcr, x1
    msr fpsr, x2
    isb
    ldp x0, x1, [x9, #0]
    ldp x2, x3, [x9, #16]
    ldp x4, x5, [x9, #32]
    ldp x6, x7, [x9, #48]
    ldr x8, [x9, #64]
    ldp x10, x11, [x9, #80]
    ldp x12, x13, [x9, #96]
    ldp x14, x15, [x9, #112]
    ldp x16, x17, [x9, #128]
    ldp x18, x19, [x9, #144]
    ldp x20, x21, [x9, #160]
    ldp x22, x23, [x9, #176]
    ldp x24, x25, [x9, #192]
    ldp x26, x27, [x9, #208]
    ldp x28, x29, [x9, #224]
    ldr x30, [x9, #240]
    ldr x9, [x9, #72]
    eret
    .size aarch64_enter_user_context, . - aarch64_enter_user_context

    .balign 16
    .global aarch64_user_return
    .type aarch64_user_return,%function
aarch64_user_return:
    adrp x10, aarch64_saved_kernel_context
    add x10, x10, :lo12:aarch64_saved_kernel_context
    mrs x11, tpidr_el1
    add x10, x10, x11, lsl #8
    ldr x9, [x10, #0]
    ldp x19, x20, [x10, #8]
    ldp x21, x22, [x10, #24]
    ldp x23, x24, [x10, #40]
    ldp x25, x26, [x10, #56]
    ldp x27, x28, [x10, #72]
    ldp x29, x30, [x10, #88]
    ldp q8, q9, [x10, #112]
    ldp q10, q11, [x10, #144]
    ldp q12, q13, [x10, #176]
    ldp q14, q15, [x10, #208]
    ldp x11, x12, [x10, #240]
    msr fpcr, x11
    msr fpsr, x12
    mov sp, x9
    ret
    .size aarch64_user_return, . - aarch64_user_return

    .section .text.aarch64_vectors,"ax",%progbits
    .balign 2048
    .global aarch64_vectors
aarch64_vectors:
    .macro VECTOR_SLOT target
        b \target
        .space 124
    .endm
    VECTOR_SLOT aarch64_sync_0
    VECTOR_SLOT aarch64_irq_1
    VECTOR_SLOT aarch64_fiq_2
    VECTOR_SLOT aarch64_serror_3
    VECTOR_SLOT aarch64_sync_4
    VECTOR_SLOT aarch64_irq_5
    VECTOR_SLOT aarch64_fiq_6
    VECTOR_SLOT aarch64_serror_7
    VECTOR_SLOT aarch64_sync_8
    VECTOR_SLOT aarch64_irq_9
    VECTOR_SLOT aarch64_fiq_10
    VECTOR_SLOT aarch64_serror_11
    VECTOR_SLOT aarch64_sync_12
    VECTOR_SLOT aarch64_irq_13
    VECTOR_SLOT aarch64_fiq_14
    VECTOR_SLOT aarch64_serror_15

    .macro SAVE_AND_DISPATCH kind
        sub sp, sp, #832
        stp x0, x1, [sp, #0]
        stp x2, x3, [sp, #16]
        stp x4, x5, [sp, #32]
        stp x6, x7, [sp, #48]
        stp x8, x9, [sp, #64]
        stp x10, x11, [sp, #80]
        stp x12, x13, [sp, #96]
        stp x14, x15, [sp, #112]
        stp x16, x17, [sp, #128]
        stp x18, x19, [sp, #144]
        stp x20, x21, [sp, #160]
        stp x22, x23, [sp, #176]
        stp x24, x25, [sp, #192]
        stp x26, x27, [sp, #208]
        stp x28, x29, [sp, #224]
        str x30, [sp, #240]
        mrs x2, elr_el1
        mrs x3, spsr_el1
        mrs x4, esr_el1
        mrs x5, far_el1
        stp x2, x3, [sp, #248]
        stp x4, x5, [sp, #264]
        mrs x6, sp_el0
        mrs x7, ttbr0_el1
        stp x6, x7, [sp, #280]
        mrs x8, tpidr_el0
        str x8, [sp, #296]
        stp q0, q1, [sp, #304]
        stp q2, q3, [sp, #336]
        stp q4, q5, [sp, #368]
        stp q6, q7, [sp, #400]
        stp q8, q9, [sp, #432]
        stp q10, q11, [sp, #464]
        stp q12, q13, [sp, #496]
        stp q14, q15, [sp, #528]
        stp q16, q17, [sp, #560]
        stp q18, q19, [sp, #592]
        stp q20, q21, [sp, #624]
        stp q22, q23, [sp, #656]
        stp q24, q25, [sp, #688]
        stp q26, q27, [sp, #720]
        stp q28, q29, [sp, #752]
        stp q30, q31, [sp, #784]
        mrs x8, fpcr
        mrs x9, fpsr
        add x10, sp, #816
        stp x8, x9, [x10]
        mov x0, #\kind
        mov x1, sp
        bl aarch64_exception_dispatch
        ldp x2, x3, [sp, #248]
        msr elr_el1, x2
        msr spsr_el1, x3
        ldr x4, [sp, #280]
        msr sp_el0, x4
        ldr x4, [sp, #288]
        mrs x5, ttbr0_el1
        cmp x4, x5
        b.eq 991f
        dsb ish
        msr ttbr0_el1, x4
        isb
        tlbi vmalle1
        dsb ish
        isb
991:
        ldr x4, [sp, #296]
        msr tpidr_el0, x4
        ldp q0, q1, [sp, #304]
        ldp q2, q3, [sp, #336]
        ldp q4, q5, [sp, #368]
        ldp q6, q7, [sp, #400]
        ldp q8, q9, [sp, #432]
        ldp q10, q11, [sp, #464]
        ldp q12, q13, [sp, #496]
        ldp q14, q15, [sp, #528]
        ldp q16, q17, [sp, #560]
        ldp q18, q19, [sp, #592]
        ldp q20, q21, [sp, #624]
        ldp q22, q23, [sp, #656]
        ldp q24, q25, [sp, #688]
        ldp q26, q27, [sp, #720]
        ldp q28, q29, [sp, #752]
        ldp q30, q31, [sp, #784]
        add x6, sp, #816
        ldp x4, x5, [x6]
        msr fpcr, x4
        msr fpsr, x5
        ldp x0, x1, [sp, #0]
        ldp x2, x3, [sp, #16]
        ldp x4, x5, [sp, #32]
        ldp x6, x7, [sp, #48]
        ldp x8, x9, [sp, #64]
        ldp x10, x11, [sp, #80]
        ldp x12, x13, [sp, #96]
        ldp x14, x15, [sp, #112]
        ldp x16, x17, [sp, #128]
        ldp x18, x19, [sp, #144]
        ldp x20, x21, [sp, #160]
        ldp x22, x23, [sp, #176]
        ldp x24, x25, [sp, #192]
        ldp x26, x27, [sp, #208]
        ldp x28, x29, [sp, #224]
        ldr x30, [sp, #240]
        add sp, sp, #832
        eret
    .endm

aarch64_sync_0: SAVE_AND_DISPATCH 0
aarch64_irq_1: SAVE_AND_DISPATCH 1
aarch64_fiq_2: SAVE_AND_DISPATCH 2
aarch64_serror_3: SAVE_AND_DISPATCH 3
aarch64_sync_4: SAVE_AND_DISPATCH 4
aarch64_irq_5: SAVE_AND_DISPATCH 5
aarch64_fiq_6: SAVE_AND_DISPATCH 6
aarch64_serror_7: SAVE_AND_DISPATCH 7
aarch64_sync_8: SAVE_AND_DISPATCH 8
aarch64_irq_9: SAVE_AND_DISPATCH 9
aarch64_fiq_10: SAVE_AND_DISPATCH 10
aarch64_serror_11: SAVE_AND_DISPATCH 11
aarch64_sync_12: SAVE_AND_DISPATCH 12
aarch64_irq_13: SAVE_AND_DISPATCH 13
aarch64_fiq_14: SAVE_AND_DISPATCH 14
aarch64_serror_15: SAVE_AND_DISPATCH 15
"#
);

unsafe extern "C" {
    static aarch64_vectors: u8;
    static aarch64_secondary_entry: u8;
    static aarch64_user_return: u8;
    fn aarch64_enter_user_context(context: *const UserContext) -> u64;
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct UserContext {
    pub registers: [u64; 31],
    pub elr: u64,
    pub spsr: u64,
    pub esr: u64,
    pub far: u64,
    pub sp_el0: u64,
    pub ttbr0: u64,
    pub tpidr_el0: u64,
    vector_registers: [u128; 32],
    fpcr: u64,
    fpsr: u64,
}

const _: () = assert!(core::mem::size_of::<UserContext>() == EXCEPTION_FRAME_BYTES);
const _: () = assert!(core::mem::offset_of!(UserContext, elr) == 248);
const _: () = assert!(core::mem::offset_of!(UserContext, spsr) == 256);
const _: () = assert!(core::mem::offset_of!(UserContext, sp_el0) == 280);
const _: () = assert!(core::mem::offset_of!(UserContext, ttbr0) == 288);
const _: () = assert!(core::mem::offset_of!(UserContext, tpidr_el0) == 296);
const _: () = assert!(core::mem::offset_of!(UserContext, vector_registers) == 304);
const _: () = assert!(core::mem::offset_of!(UserContext, fpcr) == 816);
const _: () = assert!(core::mem::offset_of!(UserContext, fpsr) == 824);

impl UserContext {
    pub const fn initial(entry: u64, stack: u64, root: u64, argument: u64) -> Self {
        let mut registers = [0; 31];
        registers[0] = argument;
        Self {
            registers,
            elr: entry,
            spsr: 0,
            esr: 0,
            far: 0,
            sp_el0: stack,
            ttbr0: root,
            tpidr_el0: 0,
            vector_registers: [0; 32],
            fpcr: 0,
            fpsr: 0,
        }
    }

    pub(crate) fn capture(frame: &ExceptionFrame) -> Self {
        Self {
            registers: frame.registers,
            elr: frame.elr,
            spsr: frame.spsr,
            esr: frame.esr,
            far: frame.far,
            sp_el0: frame.sp_el0,
            ttbr0: frame.ttbr0 & ADDRESS_MASK,
            tpidr_el0: frame.tpidr_el0,
            vector_registers: frame.vector_registers,
            fpcr: frame.fpcr,
            fpsr: frame.fpsr,
        }
    }

    pub(crate) fn restore(self, frame: &mut ExceptionFrame) {
        frame.registers = self.registers;
        frame.elr = self.elr;
        frame.spsr = self.spsr;
        frame.esr = self.esr;
        frame.far = self.far;
        frame.sp_el0 = self.sp_el0;
        frame.ttbr0 = self.ttbr0;
        frame.tpidr_el0 = self.tpidr_el0;
        frame.vector_registers = self.vector_registers;
        frame.fpcr = self.fpcr;
        frame.fpsr = self.fpsr;
    }
}

#[derive(Clone, Copy)]
pub struct MmuReport {
    pub ttbr0: u64,
    pub tcr: u64,
    pub mair: u64,
}

#[derive(Clone, Copy)]
pub struct TimerReport {
    pub frequency: u64,
    pub ticks: u64,
}

#[derive(Clone, Copy)]
pub struct SmpReport {
    pub online_cpus: u32,
    pub secondary_cpus: u32,
    pub psci_major: u16,
    pub psci_minor: u16,
}

#[inline]
pub fn disable_interrupts() {
    unsafe {
        asm!(
            "msr daifset, #0xf",
            options(nomem, nostack, preserves_flags)
        )
    }
}

#[inline]
pub fn enable_interrupts() {
    unsafe { asm!("msr daifclr, #2", options(nomem, nostack, preserves_flags)) }
}

pub fn current_el() -> u64 {
    let value: u64;
    unsafe { asm!("mrs {value}, CurrentEL", value = out(reg) value, options(nomem, nostack)) };
    value >> 2
}

/// Logical CPU ID installed in TPIDR_EL1 during secondary bring-up. BSP uses
/// the architectural reset value zero. Keeping this kernel-only avoids
/// colliding with EL0 TLS in TPIDR_EL0.
pub(crate) fn cpu_index() -> usize {
    let value: u64;
    unsafe { asm!("mrs {value}, TPIDR_EL1", value = out(reg) value, options(nomem, nostack)) };
    let index = value as usize;
    if index >= MAX_AARCH64_CPUS {
        crate::fatal("AArch64 logical CPU ID invalid");
    }
    index
}

/// Virtio input MMIO and deferred compositor input work are owned by CPU0.
/// AP syscall paths consume already-published logical queues and defer hardware
/// service to CPU0's timer bottom half instead of contending on the device.
pub(crate) fn service_input_on_owner_cpu() -> bool {
    if cpu_index() != 0 {
        INPUT_SERVICE_NONOWNER_DEFERRALS.fetch_add(1, Ordering::AcqRel);
        return false;
    }
    let activity = crate::aarch64_virtio_input::poll();
    crate::graphics::service_deferred_actions();
    if activity {
        INPUT_SERVICE_OWNER_ACTIVITY.fetch_add(1, Ordering::AcqRel);
    }
    activity
}

pub(crate) fn reset_input_service_affinity_evidence() {
    INPUT_SERVICE_OWNER_ACTIVITY.store(0, Ordering::Release);
    INPUT_SERVICE_NONOWNER_DEFERRALS.store(0, Ordering::Release);
}

pub(crate) fn input_service_affinity_evidence() -> (u64, u64) {
    (
        INPUT_SERVICE_OWNER_ACTIVITY.load(Ordering::Acquire),
        INPUT_SERVICE_NONOWNER_DEFERRALS.load(Ordering::Acquire),
    )
}

/// Virtio-net RX queue consumption and socket demultiplexing are CPU0-owned.
/// AP socket paths may consume already-published socket buffers, but defer RX
/// ring service so the device and socket locks never contend across CPUs.
pub(crate) fn service_network_rx_on_owner_cpu() -> usize {
    if cpu_index() != 0 {
        NETWORK_RX_NONOWNER_DEFERRALS.fetch_add(1, Ordering::AcqRel);
        return 0;
    }
    let serviced = crate::aarch64_virtio_net::service_tx_requests();
    let frames = if serviced == 0
        && !crate::aarch64_virtio_net::tx_request_publication_pending()
    {
        crate::aarch64_socket::pump()
    } else {
        0
    };
    NETWORK_RX_OWNER_FRAMES.fetch_add(frames as u64, Ordering::AcqRel);
    frames
}

pub(crate) fn reset_network_rx_affinity_evidence() {
    NETWORK_RX_OWNER_FRAMES.store(0, Ordering::Release);
    NETWORK_RX_NONOWNER_DEFERRALS.store(0, Ordering::Release);
}

pub(crate) fn network_rx_affinity_evidence() -> (u64, u64) {
    (
        NETWORK_RX_OWNER_FRAMES.load(Ordering::Acquire),
        NETWORK_RX_NONOWNER_DEFERRALS.load(Ordering::Acquire),
    )
}

fn cached_active_root() -> u64 {
    ACTIVE_USER_ROOTS[cpu_index()].load(Ordering::Acquire)
}

fn root_active_on_any_cpu(root: u64) -> bool {
    root != 0
        && ACTIVE_USER_ROOTS
            .iter()
            .any(|active| active.load(Ordering::Acquire) == root)
}

pub fn init_mmu(kernel_start: u64, kernel_end: u64) -> MmuReport {
    if current_el() != 1
        || kernel_start < RAM_BASE
        || kernel_end <= kernel_start
        || kernel_end > RAM_BASE + RAM_SIZE
    {
        crate::fatal("AArch64 EL1/kernel mapping precondition failed");
    }
    let pc: u64;
    unsafe { asm!("adr {pc}, .", pc = out(reg) pc, options(nomem, nostack)) };
    if pc < kernel_start || pc >= kernel_end {
        crate::fatal("AArch64 kernel is not identity addressed");
    }

    let (l0, l1, low_l2, ram_l2) = unsafe {
        (
            core::ptr::addr_of_mut!(LEVEL0.entries).cast::<u64>(),
            core::ptr::addr_of_mut!(LEVEL1.entries).cast::<u64>(),
            core::ptr::addr_of_mut!(LOW_LEVEL2.entries).cast::<u64>(),
            core::ptr::addr_of_mut!(RAM_LEVEL2.entries).cast::<u64>(),
        )
    };
    unsafe {
        for index in 0..512 {
            l0.add(index).write_volatile(0);
            l1.add(index).write_volatile(0);
            low_l2.add(index).write_volatile(0);
            ram_l2.add(index).write_volatile(0);
        }
        l0.write_volatile((l1 as u64 & !(PAGE_SIZE - 1)) | TABLE_DESCRIPTOR);
        l1.write_volatile((low_l2 as u64 & !(PAGE_SIZE - 1)) | TABLE_DESCRIPTOR);
        l1.add(1)
            .write_volatile((ram_l2 as u64 & !(PAGE_SIZE - 1)) | TABLE_DESCRIPTOR);
        for index in 0..512usize {
            let low_address = index as u64 * BLOCK_SIZE;
            low_l2.add(index).write_volatile(
                low_address | BLOCK_DESCRIPTOR | ATTR_DEVICE | SH_OUTER | ACCESS_FLAG | PXN | UXN,
            );
            let ram_address = RAM_BASE + index as u64 * BLOCK_SIZE;
            let executable =
                ram_address < kernel_end && ram_address.saturating_add(BLOCK_SIZE) > kernel_start;
            ram_l2.add(index).write_volatile(
                ram_address
                    | BLOCK_DESCRIPTOR
                    | ATTR_NORMAL
                    | SH_INNER
                    | ACCESS_FLAG
                    | UXN
                    | if executable { 0 } else { PXN },
            );
        }
    }

    let id_aa64mmfr0: u64;
    unsafe {
        asm!(
            "mrs {value}, ID_AA64MMFR0_EL1",
            value = out(reg) id_aa64mmfr0,
            options(nomem, nostack)
        )
    };
    let physical_range = id_aa64mmfr0 & 0xf;
    if physical_range > 6 {
        crate::fatal("AArch64 physical-address range unsupported");
    }
    let mair = 0x00ffu64;
    let tcr = 16u64 | (1 << 8) | (1 << 10) | (0b11 << 12) | (1 << 23) | (physical_range << 32);
    let ttbr0 = l0 as u64;
    let original_sctlr: u64;
    unsafe { asm!("mrs {value}, SCTLR_EL1", value = out(reg) original_sctlr) };
    let disabled_sctlr = original_sctlr & !(1 | 4 | 0x1000);
    // UCI permits EL0 JIT runtimes to perform architected cache maintenance
    // on their own mapped code after W^X transitions. UCT exposes read-only
    // cache geometry; DZE permits DC ZVA using kernel-advertised block size.
    let enabled_sctlr = original_sctlr | 1 | 4 | 0x1000 | (1 << 26) | (1 << 15) | (1 << 14);
    unsafe {
        asm!(
            "dsb sy",
            "msr SCTLR_EL1, {disabled_sctlr}",
            "isb",
            "msr MAIR_EL1, {mair}",
            "msr TCR_EL1, {tcr}",
            "msr TTBR0_EL1, {ttbr0}",
            "tlbi vmalle1",
            "dsb sy",
            "isb",
            "msr SCTLR_EL1, {enabled_sctlr}",
            "isb",
            disabled_sctlr = in(reg) disabled_sctlr,
            enabled_sctlr = in(reg) enabled_sctlr,
            mair = in(reg) mair,
            tcr = in(reg) tcr,
            ttbr0 = in(reg) ttbr0,
            options(nostack)
        );
    }
    let active_ttbr0: u64;
    unsafe { asm!("mrs {value}, TTBR0_EL1", value = out(reg) active_ttbr0) };
    if active_ttbr0 & !(PAGE_SIZE - 1) != ttbr0 {
        crate::fatal("AArch64 TTBR0 activation failed");
    }
    KERNEL_ROOT.store(ttbr0, Ordering::Release);
    MmuReport { ttbr0, tcr, mair }
}

pub fn kernel_root() -> u64 {
    let root = KERNEL_ROOT.load(Ordering::Acquire);
    if root == 0 {
        crate::fatal("AArch64 kernel address space unavailable");
    }
    root
}

pub fn new_user_address_space() -> u64 {
    let source_root = kernel_root();
    let source_l1 = table_child(source_root, 0)
        .unwrap_or_else(|| crate::fatal("AArch64 kernel L1 table absent"));
    let source_low = table_child(source_l1, 0)
        .unwrap_or_else(|| crate::fatal("AArch64 kernel low L2 table absent"));
    let source_ram = table_child(source_l1, 1)
        .unwrap_or_else(|| crate::fatal("AArch64 kernel RAM L2 table absent"));
    let root = allocate_table();
    let l1 = allocate_table();
    let low = allocate_table();
    let ram = allocate_table();
    unsafe {
        core::ptr::copy_nonoverlapping(source_low as *const u64, low as *mut u64, 512);
        core::ptr::copy_nonoverlapping(source_ram as *const u64, ram as *mut u64, 512);
        write_table_entry(root, 0, l1 | TABLE_DESCRIPTOR);
        write_table_entry(l1, 0, low | TABLE_DESCRIPTOR);
        write_table_entry(l1, 1, ram | TABLE_DESCRIPTOR);
    }
    root
}

/// Eagerly clone resident EL0 pages into an isolated address space.
/// Private copies keep teardown/refcount rules simple; `execve` normally
/// replaces Gecko launcher children immediately after fork.
pub fn clone_user_address_space_eager(source_root: u64) -> Option<(u64, usize)> {
    if source_root == kernel_root() || source_root & (PAGE_SIZE - 1) != 0 {
        return None;
    }
    let destination_root = new_user_address_space();
    let source_l1 = table_child(source_root, 0)?;
    let last_l1 = ((USER_ADDRESS_LIMIT - 1) / L1_SPAN) as usize;
    let mut copied = 0usize;
    for l1_index in 0..=last_l1 {
        let Some(source_l2) = table_child(source_l1, l1_index) else {
            continue;
        };
        for l2_index in 0..512usize {
            let descriptor = unsafe { *((source_l2 as *const u64).add(l2_index)) };
            if descriptor & 0b11 != TABLE_DESCRIPTOR {
                continue;
            }
            let source_l3 = descriptor & ADDRESS_MASK;
            for page_index in 0..512usize {
                let entry = unsafe { *((source_l3 as *const u64).add(page_index)) };
                if entry & 0b11 != TABLE_DESCRIPTOR {
                    continue;
                }
                let address = l1_index as u64 * L1_SPAN
                    + l2_index as u64 * BLOCK_SIZE
                    + page_index as u64 * PAGE_SIZE;
                if !(USER_ADDRESS_BASE..USER_ADDRESS_LIMIT).contains(&address) {
                    continue;
                }
                let Some(frame) = crate::mm::allocate_frame() else {
                    let _ = destroy_user_address_space(destination_root);
                    return None;
                };
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        (entry & ADDRESS_MASK) as *const u8,
                        frame as *mut u8,
                        PAGE_SIZE as usize,
                    );
                }
                let permission = entry & (0b11 << 6);
                map_user_page_permissions_in(
                    destination_root,
                    address,
                    frame,
                    permission != 0,
                    permission == AP_USER_RW,
                    entry & UXN == 0,
                );
                copied += 1;
            }
        }
    }
    Some((destination_root, copied))
}

pub fn map_user_page_in(
    root: u64,
    virtual_address: u64,
    physical_address: u64,
    writable: bool,
    executable: bool,
) {
    map_user_page_permissions_in(
        root,
        virtual_address,
        physical_address,
        true,
        writable,
        executable,
    );
}

pub fn map_user_page_permissions_in(
    root: u64,
    virtual_address: u64,
    physical_address: u64,
    readable: bool,
    writable: bool,
    executable: bool,
) {
    if root == kernel_root()
        || root & (PAGE_SIZE - 1) != 0
        || virtual_address & (PAGE_SIZE - 1) != 0
        || physical_address & (PAGE_SIZE - 1) != 0
        || !(USER_ADDRESS_BASE..USER_ADDRESS_LIMIT).contains(&virtual_address)
        || (!readable && (writable || executable))
        || writable && executable
    {
        crate::fatal("invalid AArch64 user-page mapping");
    }
    let l1 = table_child(root, 0).unwrap_or_else(|| crate::fatal("AArch64 user L1 table absent"));
    let l1_index = ((virtual_address / L1_SPAN) % 512) as usize;
    let low = table_child(l1, l1_index).unwrap_or_else(|| {
        let table = allocate_table();
        unsafe { write_table_entry(l1, l1_index, table | TABLE_DESCRIPTOR) };
        table
    });
    let level2_index = ((virtual_address % L1_SPAN) / BLOCK_SIZE) as usize;
    let slot = unsafe { (low as *mut u64).add(level2_index) };
    let mut descriptor = unsafe { slot.read_volatile() };
    if matches!(descriptor & 0b11, 0 | BLOCK_DESCRIPTOR) {
        let level3 = allocate_table();
        descriptor = level3 | TABLE_DESCRIPTOR;
        unsafe { slot.write_volatile(descriptor) };
    }
    if descriptor & 0b11 != TABLE_DESCRIPTOR {
        crate::fatal("AArch64 user L3 table invalid");
    }
    let level3 = descriptor & ADDRESS_MASK;
    let page_index = ((virtual_address % BLOCK_SIZE) / PAGE_SIZE) as usize;
    let page_slot = unsafe { (level3 as *mut u64).add(page_index) };
    if unsafe { page_slot.read_volatile() } & 0b11 != 0 {
        crate::fatal("duplicate AArch64 user-page mapping");
    }
    let permission = if !readable {
        0
    } else if writable {
        AP_USER_RW
    } else {
        AP_USER_RO
    };
    let execute_never = if executable { PXN } else { PXN | UXN };
    unsafe {
        page_slot.write_volatile(
            physical_address
                | TABLE_DESCRIPTOR
                | ATTR_NORMAL
                | SH_INNER
                | ACCESS_FLAG
                | permission
                | execute_never,
        )
    };
    invalidate_user_page_if_active(root, virtual_address);
}

pub fn unmap_user_page_in(root: u64, virtual_address: u64) -> Option<u64> {
    let slot = user_page_slot(root, virtual_address)?;
    let entry = unsafe { slot.read_volatile() };
    if entry & 0b11 != TABLE_DESCRIPTOR {
        return None;
    }
    unsafe { slot.write_volatile(0) };
    invalidate_user_page_if_active(root, virtual_address);
    Some(entry & ADDRESS_MASK)
}

pub fn protect_user_page_in(
    root: u64,
    virtual_address: u64,
    writable: bool,
    executable: bool,
) -> bool {
    protect_user_page_permissions_in(root, virtual_address, true, writable, executable)
}

pub fn protect_user_page_permissions_in(
    root: u64,
    virtual_address: u64,
    readable: bool,
    writable: bool,
    executable: bool,
) -> bool {
    if (!readable && (writable || executable)) || writable && executable {
        return false;
    }
    let Some(slot) = user_page_slot(root, virtual_address) else {
        return false;
    };
    let entry = unsafe { slot.read_volatile() };
    if entry & 0b11 != TABLE_DESCRIPTOR {
        return false;
    }
    let permission = if !readable {
        0
    } else if writable {
        AP_USER_RW
    } else {
        AP_USER_RO
    };
    let execute_never = if executable { PXN } else { PXN | UXN };
    let updated = (entry & !((0b11 << 6) | PXN | UXN)) | permission | execute_never;
    unsafe { slot.write_volatile(updated) };
    invalidate_user_page_if_active(root, virtual_address);
    true
}

pub fn user_page_physical_in(root: u64, virtual_address: u64) -> Option<u64> {
    let slot = user_page_slot(root, virtual_address)?;
    let entry = unsafe { slot.read_volatile() };
    if entry & 0b11 != TABLE_DESCRIPTOR {
        return None;
    }
    Some(entry & ADDRESS_MASK)
}

pub fn switch_address_space(root: u64) {
    if root & (PAGE_SIZE - 1) != 0 {
        crate::fatal("unaligned AArch64 address-space root");
    }
    let target = if root == kernel_root() { 0 } else { root };
    // Gecko runs many threads in one process. Switching between those threads
    // keeps TTBR0 unchanged; flushing the complete TLB on every pipe/poll wake
    // made startup spend most of its time rebuilding identical translations.
    let hardware_root: u64;
    unsafe {
        asm!("mrs {root}, TTBR0_EL1", root = out(reg) hardware_root, options(nostack, preserves_flags));
    }
    if hardware_root == root {
        ACTIVE_USER_ROOTS[cpu_index()].store(target, Ordering::Release);
        return;
    }
    unsafe {
        asm!(
            "dsb ish",
            "msr TTBR0_EL1, {root}",
            "isb",
            "tlbi vmalle1",
            "dsb ish",
            "isb",
            root = in(reg) root,
            options(nostack),
        )
    }
    ACTIVE_USER_ROOTS[cpu_index()].store(target, Ordering::Release);
}

pub(crate) fn enter_user_context(context: &UserContext) -> u64 {
    const USER_SPSR_ALLOWED: u64 = 0xf000_0000;
    let active_root = cached_active_root();
    let stack_valid = matches!(context.sp_el0, LEGACY_USER_STACK_TOP | USER_STACK_TOP)
        || user_stack_pointer_valid_in(context.ttbr0, context.sp_el0);
    if active_root == 0
        || context.ttbr0 != active_root
        || !(USER_ADDRESS_BASE..USER_IMAGE_LIMIT).contains(&context.elr)
        || !stack_valid
        || context.spsr & !USER_SPSR_ALLOWED != 0
    {
        crate::serial_println!(
            "AArch64 EL0 entry rejected cpu={} active_root={:#x} context_root={:#x} elr={:#x} sp={:#x} stack_valid={} spsr={:#x}",
            cpu_index(),
            active_root,
            context.ttbr0,
            context.elr,
            context.sp_el0,
            u8::from(stack_valid),
            context.spsr,
        );
        crate::fatal("AArch64 EL0 entry precondition failed");
    }
    start_scheduler_timer();
    // Keep EL1 IRQs masked across the assembly restore. The target SPSR
    // unmasks IRQs atomically with ERET; taking an EL1 timer exception while
    // the trampoline is midway through consuming its stack-resident context
    // would expose a partially restored register set to the vector path.
    let status = unsafe { aarch64_enter_user_context(context) };
    disable_interrupts();
    stop_scheduler_timer();
    status
}

fn user_stack_pointer_valid_in(root: u64, stack_pointer: u64) -> bool {
    if stack_pointer & 15 != 0 || !(USER_ADDRESS_BASE..=USER_ADDRESS_LIMIT).contains(&stack_pointer)
    {
        return false;
    }
    let stack_page = (stack_pointer - 1) & !(PAGE_SIZE - 1);
    let Some(slot) = user_page_slot(root, stack_page) else {
        return false;
    };
    let entry = unsafe { slot.read_volatile() };
    entry & 0b11 == TABLE_DESCRIPTOR && entry & (0b11 << 6) == AP_USER_RW && entry & UXN != 0
}

pub fn destroy_user_address_space(root: u64) -> usize {
    if root == kernel_root() || root_active_on_any_cpu(root) || root & (PAGE_SIZE - 1) != 0 {
        crate::fatal("invalid AArch64 address-space destruction");
    }
    let l1 = table_child(root, 0)
        .unwrap_or_else(|| crate::fatal("AArch64 user L1 table absent at destruction"));
    let mut freed = 0usize;
    let last_l1 = ((USER_ADDRESS_LIMIT - 1) / L1_SPAN) as usize;
    for l1_index in 0..=last_l1 {
        let Some(level2) = table_child(l1, l1_index) else {
            continue;
        };
        for level2_index in 0..512usize {
            let descriptor = unsafe { *((level2 as *const u64).add(level2_index)) };
            if descriptor & 0b11 != TABLE_DESCRIPTOR {
                continue;
            }
            let level3 = descriptor & ADDRESS_MASK;
            for page_index in 0..512usize {
                let entry = unsafe { *((level3 as *const u64).add(page_index)) };
                if entry & 0b11 == TABLE_DESCRIPTOR {
                    free_table_frame(entry & ADDRESS_MASK);
                    freed += 1;
                }
            }
            free_table_frame(level3);
            freed += 1;
        }
        free_table_frame(level2);
        freed += 1;
    }
    for l1_index in last_l1 + 1..512 {
        if table_child(l1, l1_index).is_some() {
            crate::fatal("AArch64 user address space has out-of-range L2");
        }
    }
    for table in [l1, root] {
        free_table_frame(table);
        freed += 1;
    }
    freed
}

pub fn user_resident_pages(root: u64) -> Option<usize> {
    if root == kernel_root() || root & (PAGE_SIZE - 1) != 0 {
        return None;
    }
    let level1 = table_child(root, 0)?;
    let last_level1 = ((USER_ADDRESS_LIMIT - 1) / L1_SPAN) as usize;
    let mut pages = 0usize;
    for level1_index in 0..=last_level1 {
        let Some(level2) = table_child(level1, level1_index) else {
            continue;
        };
        for level2_index in 0..512usize {
            let descriptor = unsafe { *((level2 as *const u64).add(level2_index)) };
            if descriptor & 0b11 != TABLE_DESCRIPTOR {
                continue;
            }
            let level3 = descriptor & ADDRESS_MASK;
            for page_index in 0..512usize {
                let entry = unsafe { *((level3 as *const u64).add(page_index)) };
                if entry & 0b11 == TABLE_DESCRIPTOR {
                    pages = pages.saturating_add(1);
                }
            }
        }
    }
    Some(pages)
}

pub fn sync_user_code(frame: u64) {
    let ctr: u64;
    unsafe { asm!("mrs {value}, CTR_EL0", value = out(reg) ctr, options(nomem, nostack)) };
    let line = 4usize << ((ctr >> 16) & 0xf);
    for offset in (0..PAGE_SIZE as usize).step_by(line) {
        unsafe {
            asm!("dc civac, {address}", address = in(reg) frame + offset as u64, options(nostack))
        };
    }
    unsafe { asm!("dsb ish", "ic iallu", "dsb ish", "isb", options(nostack),) }
}

pub(crate) fn user_range_readable(address: u64, length: usize) -> bool {
    let root = cached_active_root();
    let Some(end) = address.checked_add(length as u64) else {
        return false;
    };
    if root == 0
        || address < USER_ADDRESS_BASE
        || end > USER_ADDRESS_LIMIT
        || length > MAX_USER_COPY
    {
        return false;
    }
    if length == 0 {
        return true;
    }
    let mut page = address & !(PAGE_SIZE - 1);
    while page < end {
        let Some(slot) = user_page_slot(root, page) else {
            return false;
        };
        let entry = unsafe { slot.read_volatile() };
        if entry & 0b11 != TABLE_DESCRIPTOR || entry & (1 << 6) == 0 {
            return false;
        }
        page += PAGE_SIZE;
    }
    true
}

pub(crate) fn user_range_mapped(address: u64, length: usize) -> bool {
    let root = cached_active_root();
    let Some(end) = address.checked_add(length as u64) else {
        return false;
    };
    if root == 0
        || address < USER_ADDRESS_BASE
        || end > USER_ADDRESS_LIMIT
        || length > MAX_USER_COPY
    {
        return false;
    }
    if length == 0 {
        return true;
    }
    let mut page = address & !(PAGE_SIZE - 1);
    while page < end {
        let Some(slot) = user_page_slot(root, page) else {
            return false;
        };
        if unsafe { slot.read_volatile() } & 0b11 != TABLE_DESCRIPTOR {
            return false;
        }
        page += PAGE_SIZE;
    }
    true
}

pub(crate) fn user_range_writable(address: u64, length: usize) -> bool {
    let root = cached_active_root();
    let Some(end) = address.checked_add(length as u64) else {
        return false;
    };
    if root == 0
        || address < USER_ADDRESS_BASE
        || end > USER_ADDRESS_LIMIT
        || length > MAX_USER_COPY
    {
        return false;
    }
    if length == 0 {
        return true;
    }
    let mut page = address & !(PAGE_SIZE - 1);
    while page < end {
        let Some(slot) = user_page_slot(root, page) else {
            return false;
        };
        let entry = unsafe { slot.read_volatile() };
        if entry & 0b11 != TABLE_DESCRIPTOR || entry & (0b11 << 6) != AP_USER_RW {
            return false;
        }
        page += PAGE_SIZE;
    }
    true
}

pub fn user_address_executable(address: u64) -> bool {
    let root = cached_active_root();
    if root == 0 || address & 3 != 0 || !(USER_ADDRESS_BASE..USER_ADDRESS_LIMIT).contains(&address)
    {
        return false;
    }
    let Some(slot) = user_page_slot(root, address & !(PAGE_SIZE - 1)) else {
        return false;
    };
    let entry = unsafe { slot.read_volatile() };
    entry & 0b11 == TABLE_DESCRIPTOR && entry & (1 << 6) != 0 && entry & UXN == 0
}

fn user_page_slot(root: u64, virtual_address: u64) -> Option<*mut u64> {
    if root == kernel_root()
        || root & (PAGE_SIZE - 1) != 0
        || virtual_address & (PAGE_SIZE - 1) != 0
        || !(USER_ADDRESS_BASE..USER_ADDRESS_LIMIT).contains(&virtual_address)
    {
        return None;
    }
    let l1 = table_child(root, 0)?;
    let l1_index = ((virtual_address / L1_SPAN) % 512) as usize;
    let low = table_child(l1, l1_index)?;
    user_page_slot_from_low(low, virtual_address)
}

fn user_page_slot_from_low(low: u64, virtual_address: u64) -> Option<*mut u64> {
    let level2_index = ((virtual_address % L1_SPAN) / BLOCK_SIZE) as usize;
    let descriptor = unsafe { *((low as *const u64).add(level2_index)) };
    if descriptor & 0b11 != TABLE_DESCRIPTOR {
        return None;
    }
    let level3 = descriptor & ADDRESS_MASK;
    let page_index = ((virtual_address % BLOCK_SIZE) / PAGE_SIZE) as usize;
    Some(unsafe { (level3 as *mut u64).add(page_index) })
}

fn invalidate_user_page_if_active(root: u64, virtual_address: u64) {
    if !root_active_on_any_cpu(root) {
        return;
    }
    unsafe {
        asm!(
            "dsb ishst",
            // Inner-shareable invalidation reaches every PE currently using
            // this shared process root. Local `vae1` is insufficient once
            // Firefox threads execute one address space on multiple CPUs.
            "tlbi vae1is, {page}",
            "dsb ish",
            "isb",
            page = in(reg) virtual_address >> 12,
            options(nostack),
        )
    }
}

fn table_child(table: u64, index: usize) -> Option<u64> {
    let entry = unsafe { *((table as *const u64).add(index)) };
    (entry & 0b11 == TABLE_DESCRIPTOR).then_some(entry & ADDRESS_MASK)
}

fn allocate_table() -> u64 {
    let frame =
        crate::mm::allocate_frame().unwrap_or_else(|| crate::fatal("AArch64 page-table frame OOM"));
    unsafe { core::ptr::write_bytes(frame as *mut u8, 0, PAGE_SIZE as usize) };
    frame
}

unsafe fn write_table_entry(table: u64, index: usize, value: u64) {
    unsafe { (table as *mut u64).add(index).write_volatile(value) }
}

fn free_table_frame(frame: u64) {
    if crate::mm::free_frame(frame).is_err() {
        crate::fatal("AArch64 address-space frame reclaim failed");
    }
}

pub fn init_exceptions() {
    if current_el() != 1 || (core::ptr::addr_of!(aarch64_vectors) as u64) & 0x7ff != 0 {
        crate::fatal("AArch64 vector alignment/EL failed");
    }
    let mut cpacr: u64;
    unsafe {
        asm!("mrs {value}, CPACR_EL1", value = out(reg) cpacr, options(nomem, nostack));
        cpacr |= 0b11 << 20;
        asm!(
            "msr CPACR_EL1, {cpacr}",
            "msr VBAR_EL1, {vectors}",
            "isb",
            cpacr = in(reg) cpacr,
            vectors = in(reg) core::ptr::addr_of!(aarch64_vectors) as u64,
            options(nostack)
        )
    }
}

pub fn exception_self_test() {
    let before = SYNC_SELF_TESTS.load(Ordering::Acquire);
    unsafe { asm!("brk #0x4d4b", options(nomem, nostack)) };
    if SYNC_SELF_TESTS.load(Ordering::Acquire) != before + 1 {
        crate::fatal("AArch64 synchronous exception return failed");
    }
}

pub fn init_timer(
    distributor: u64,
    cpu_interface: u64,
    gic_version: u8,
    timer_intid: u32,
    timer_flags: u32,
) -> TimerReport {
    if gic_version != 2
        || distributor == 0
        || cpu_interface == 0
        || distributor & 0xffff != 0
        || cpu_interface & 0xfff != 0
        || !(16..32).contains(&timer_intid)
        || timer_flags & !0x7 != 0
        || timer_flags & 0x2 != 0
    {
        crate::fatal("AArch64 GICv2/timer configuration unsupported");
    }
    let interrupt_count = ((unsafe { mmio_read32(distributor + GICD_TYPER) } & 0x1f) + 1) * 32;
    if timer_intid >= interrupt_count {
        crate::fatal("AArch64 timer INTID exceeds GIC capacity");
    }
    let bit = 1u32 << timer_intid;
    unsafe {
        mmio_write32(distributor + GICD_CTLR, 0);
        mmio_write32(distributor + GICD_ICENABLER0, bit);
        mmio_write32(distributor + GICD_ICPENDR0, bit);
        let group = mmio_read32(distributor + GICD_IGROUPR0) | bit;
        mmio_write32(distributor + GICD_IGROUPR0, group);
        let priority_register = distributor + GICD_IPRIORITYR + u64::from(timer_intid & !3);
        let shift = (timer_intid & 3) * 8;
        let priority = (mmio_read32(priority_register) & !(0xff << shift)) | (0x80 << shift);
        mmio_write32(priority_register, priority);
        let config_shift = (timer_intid - 16) * 2;
        let mut config = mmio_read32(distributor + GICD_ICFGR1) & !(0b11 << config_shift);
        if timer_flags & 1 != 0 {
            config |= 0b10 << config_shift;
        }
        mmio_write32(distributor + GICD_ICFGR1, config);
        mmio_write32(distributor + GICD_ISENABLER0, bit);
        mmio_write32(cpu_interface + GICC_PMR, 0xff);
        mmio_write32(cpu_interface + GICC_BPR, 0);
        // AckCtl permits GICC_IAR to acknowledge Group1 in QEMU/HVF's
        // single-security-state GICv2 view.
        mmio_write32(cpu_interface + GICC_CTLR, 0b111);
        mmio_write32(distributor + GICD_CTLR, 0b11);
        asm!("dsb sy", "isb", options(nostack));
    }
    crate::serial_println!(
        "MAKOS_AARCH64_GIC_READY version=2 distributor={:#x} cpu_interface={:#x} intid={}",
        distributor,
        cpu_interface,
        timer_intid,
    );

    let frequency: u64;
    unsafe { asm!("mrs {value}, CNTFRQ_EL0", value = out(reg) frequency) };
    let mut counter_access: u64;
    unsafe {
        asm!("mrs {value}, CNTKCTL_EL1", value = out(reg) counter_access);
        counter_access |= 0b11;
        asm!(
            "msr CNTKCTL_EL1, {value}",
            "isb",
            value = in(reg) counter_access,
            options(nostack)
        );
    }
    let interval = frequency / 100;
    if frequency < 100 || interval == 0 {
        crate::fatal("AArch64 generic timer frequency invalid");
    }
    TIMER_TICKS.store(0, Ordering::Release);
    UNEXPECTED_IRQS.store(0, Ordering::Release);
    IRQ_ENTRIES.store(0, Ordering::Release);
    TIMER_INTERVAL.store(interval, Ordering::Release);
    TIMER_FREQUENCY.store(frequency, Ordering::Release);
    BOOT_COUNTER.store(read_virtual_counter(), Ordering::Release);
    TIMER_INTID.store(timer_intid, Ordering::Release);
    TIMER_FLAGS.store(timer_flags, Ordering::Release);
    GIC_DISTRIBUTOR_BASE.store(distributor, Ordering::Release);
    GIC_CPU_BASE.store(cpu_interface, Ordering::Release);
    program_virtual_timer(read_virtual_counter().saturating_add(interval));
    crate::serial_println!(
        "MAKOS_AARCH64_TIMER_ARMED source=cntv frequency={} interval={}",
        frequency,
        interval,
    );
    enable_interrupts();
    let start = read_virtual_counter();
    let timeout = start.saturating_add(frequency.saturating_mul(2));
    while TIMER_TICKS.load(Ordering::Acquire) < 8 && read_virtual_counter() < timeout {
        core::hint::spin_loop();
    }
    disable_interrupts();
    unsafe { asm!("msr CNTV_CTL_EL0, {value}", value = in(reg) 0u64, options(nostack)) };
    let ticks = TIMER_TICKS.load(Ordering::Acquire);
    if ticks < 8 || UNEXPECTED_IRQS.load(Ordering::Acquire) != 0 {
        crate::fatal("AArch64 timer IRQ self-test failed");
    }
    TimerReport { frequency, ticks }
}

/// Start QEMU `virt` secondary PEs through PSCI 0.2's HVC conduit.
///
/// Secondaries prove real coherent EL1 execution, then enter a closed-gate WFI
/// dispatcher. Per-CPU process ownership exists, but the private scheduler
/// gate remains false until remaining runtime safety/proof work is complete.
pub fn init_smp(info: &ArmMadtInfo) -> SmpReport {
    let cpu_count = info.enabled_cpu_count as usize;
    if cpu_count == 0 || cpu_count > MAX_AARCH64_CPUS {
        crate::fatal("AArch64 SMP CPU count unsupported");
    }
    let boot_mpidr = read_mpidr();
    let mut boot_present = false;
    let mut seen = [0u64; MAX_AARCH64_CPUS];
    for index in 0..cpu_count {
        let mpidr = info.mpidrs[index] & MPIDR_AFFINITY_MASK;
        if seen[..index].contains(&mpidr) {
            crate::fatal("AArch64 MADT contains duplicate MPIDR");
        }
        seen[index] = mpidr;
        boot_present |= mpidr == boot_mpidr;
    }
    if !boot_present {
        crate::fatal("AArch64 BSP MPIDR absent from MADT");
    }

    let version = psci_hvc(PSCI_VERSION, 0, 0, 0);
    if version < 0 || (version as u64) < 2 {
        crate::fatal("AArch64 PSCI 0.2 HVC unavailable");
    }
    if psci_hvc(PSCI_FEATURES, PSCI_CPU_ON_64, 0, 0) < 0 {
        crate::fatal("AArch64 PSCI CPU_ON_64 unavailable");
    }
    let psci_version = version as u64;
    let psci_major = (psci_version >> 16) as u16;
    let psci_minor = psci_version as u16;

    SMP_ONLINE_MASK.store(1, Ordering::Release);
    SMP_TEST_READY_MASK.store(0, Ordering::Release);
    SMP_TEST_RUN.store(false, Ordering::Release);
    SMP_BSP_WORK_ACTIVE.store(false, Ordering::Release);
    SMP_WORK_CPU1.0.store(0, Ordering::Release);
    SMP_WORK_CPU2.0.store(0, Ordering::Release);
    SMP_WORK_CPU3.0.store(0, Ordering::Release);

    let mut tcr: u64;
    let mut mair: u64;
    let mut sctlr: u64;
    unsafe {
        asm!("mrs {value}, TCR_EL1", value = out(reg) tcr, options(nomem, nostack));
        asm!("mrs {value}, MAIR_EL1", value = out(reg) mair, options(nomem, nostack));
        asm!("mrs {value}, SCTLR_EL1", value = out(reg) sctlr, options(nomem, nostack));
    }
    let root = kernel_root();
    let entry = core::ptr::addr_of!(aarch64_secondary_entry) as u64;
    let mut secondary_index = 0usize;
    for &mpidr in &seen[..cpu_count] {
        if mpidr == boot_mpidr {
            continue;
        }
        let logical_id = secondary_index + 1;
        let context = unsafe {
            core::ptr::addr_of_mut!(SECONDARY_CONTEXTS)
                .cast::<SecondaryBootContext>()
                .add(secondary_index)
        };
        let stack = unsafe {
            core::ptr::addr_of_mut!(SECONDARY_STACKS)
                .cast::<u8>()
                .add(secondary_index * SECONDARY_STACK_BYTES)
        };
        unsafe {
            context.write(SecondaryBootContext {
                stack_top: stack.add(SECONDARY_STACK_BYTES) as u64,
                ttbr0: root,
                tcr,
                mair,
                sctlr,
                logical_id: logical_id as u64,
                expected_mpidr: mpidr,
            });
            asm!("dc civac, {address}", address = in(reg) context as u64, options(nostack));
        }
        let result = psci_hvc(PSCI_CPU_ON_64, mpidr, entry, context as u64);
        if result != 0 {
            crate::fatal("AArch64 PSCI CPU_ON failed");
        }
        secondary_index += 1;
    }
    unsafe { asm!("dsb ish", "sev", options(nostack)) };

    let expected_mask = if cpu_count == 64 {
        u64::MAX
    } else {
        (1u64 << cpu_count) - 1
    };
    let deadline = read_virtual_counter()
        .saturating_add(TIMER_FREQUENCY.load(Ordering::Acquire).saturating_mul(2));
    while SMP_ONLINE_MASK.load(Ordering::Acquire) != expected_mask
        && read_virtual_counter() < deadline
    {
        core::hint::spin_loop();
    }
    if SMP_ONLINE_MASK.load(Ordering::Acquire) != expected_mask {
        crate::fatal("AArch64 secondary CPU online timeout");
    }

    // All APs must execute shared-memory work while the BSP itself remains
    // busy. Counter growth across the BSP interval proves coherent parallel
    // PE execution; merely accepting CPU_ON is insufficient evidence.
    let secondary_mask = expected_mask & !1;
    SMP_TEST_RUN.store(true, Ordering::Release);
    unsafe { asm!("dsb ish", "sev", options(nostack)) };
    let ready_deadline = read_virtual_counter()
        .saturating_add(TIMER_FREQUENCY.load(Ordering::Acquire).saturating_mul(2));
    while SMP_TEST_READY_MASK.load(Ordering::Acquire) != secondary_mask
        && read_virtual_counter() < ready_deadline
    {
        core::hint::spin_loop();
    }
    if SMP_TEST_READY_MASK.load(Ordering::Acquire) != secondary_mask {
        crate::fatal("AArch64 secondary CPU work rendezvous timeout");
    }
    let before = secondary_work_counts();
    let mut checksum = 0x4d41_4b4f_5300_0001u64;
    let progress_deadline = read_virtual_counter()
        .saturating_add(TIMER_FREQUENCY.load(Ordering::Acquire).saturating_mul(2));
    SMP_BSP_WORK_ACTIVE.store(true, Ordering::Release);
    unsafe { asm!("dsb ish", "sev", options(nostack)) };
    let mut work = 0u64;
    let after = loop {
        for _ in 0..50_000 {
            checksum = checksum.rotate_left(7) ^ work.wrapping_mul(0x9e37_79b9_7f4a_7c15);
            work = work.wrapping_add(1);
        }
        let counts = secondary_work_counts();
        if counts[..secondary_index]
            .iter()
            .zip(before[..secondary_index].iter())
            .all(|(after, before)| after > before)
        {
            break counts;
        }
        if read_virtual_counter() >= progress_deadline {
            break counts;
        }
    };
    SMP_BSP_WORK_ACTIVE.store(false, Ordering::Release);
    core::hint::black_box(checksum);
    SMP_TEST_RUN.store(false, Ordering::Release);
    unsafe { asm!("dsb ish", "sev", options(nostack)) };
    for index in 0..secondary_index {
        if after[index] <= before[index] {
            crate::fatal("AArch64 secondary CPU made no concurrent progress");
        }
    }

    crate::serial_println!(
        "MAKOS_AARCH64_SMP_OK discovered={} online={} bsp_mpidr={:#x} aps={} psci={}.{} conduit=hvc cpu_on=64 per_cpu_stacks=1 stack_bytes={} per_cpu_vbar=1 coherent_parallel_el1=1 checksum={:#x} userspace_scheduler_cpus=1 aps_after_test=idle scheduler_gate=closed ap_idle=wfi",
        cpu_count,
        SMP_ONLINE_MASK.load(Ordering::Acquire).count_ones(),
        boot_mpidr,
        secondary_index,
        psci_major,
        psci_minor,
        SECONDARY_STACK_BYTES,
        checksum,
    );
    SmpReport {
        online_cpus: cpu_count as u32,
        secondary_cpus: secondary_index as u32,
        psci_major,
        psci_minor,
    }
}

#[unsafe(no_mangle)]
extern "C" fn aarch64_secondary_main(context: *const SecondaryBootContext) -> ! {
    let Some(context) = (unsafe { context.as_ref() }) else {
        secondary_park_forever();
    };
    let logical_id = context.logical_id as usize;
    if logical_id == 0
        || logical_id >= MAX_AARCH64_CPUS
        || current_el() != 1
        || read_mpidr() != context.expected_mpidr
        || context.ttbr0 != kernel_root()
    {
        secondary_park_forever();
    }
    unsafe {
        asm!(
            "msr TPIDR_EL1, {logical_id}",
            logical_id = in(reg) context.logical_id,
            options(nomem, nostack)
        )
    };
    init_exceptions_on_current_cpu();
    init_gic_cpu_interface_on_current_cpu();
    SMP_ONLINE_MASK.fetch_or(1u64 << logical_id, Ordering::AcqRel);
    unsafe { asm!("dsb ish", "sev", options(nostack)) };

    let bit = 1u64 << logical_id;
    // Bring-up has one bounded rendezvous. WFE is appropriate only here,
    // where BSP SEV intentionally announces the test. After it completes,
    // unrelated kernel SEVs must not wake parked APs into a hot loop.
    while !SMP_TEST_RUN.load(Ordering::Acquire) {
        unsafe { asm!("wfe", options(nomem, nostack)) };
    }
    SMP_TEST_READY_MASK.fetch_or(bit, Ordering::AcqRel);
    unsafe { asm!("sev", options(nomem, nostack)) };
    while SMP_TEST_RUN.load(Ordering::Acquire) {
        if SMP_BSP_WORK_ACTIVE.load(Ordering::Acquire) {
            secondary_work_counter(logical_id).fetch_add(1, Ordering::Relaxed);
        }
        core::hint::spin_loop();
    }
    SMP_TEST_READY_MASK.fetch_and(!bit, Ordering::AcqRel);
    secondary_scheduler_idle()
}

fn secondary_scheduler_idle() -> ! {
    disable_interrupts();
    while !SMP_USER_SCHEDULER_ENABLED.load(Ordering::Acquire) {
        // WFE wakes on every routine BSP SEV and burns roughly three host cores
        // under HVF even while this gate is closed. WFI ignores event-register
        // traffic. Future gate enablement must wake each AP with an SGI; a
        // pending masked interrupt still terminates WFI before the gate check.
        unsafe { asm!("wfi", options(nomem, nostack)) };
    }
    init_secondary_timer_on_current_cpu();
    crate::aarch64_process::run_secondary_scheduler()
}

pub(crate) fn enable_smp_probe_scheduler() {
    SMP_USER_SCHEDULER_ENABLED.store(true, Ordering::Release);
    send_scheduler_ipi();
}

/// Keep the qualified AP dispatchers live for the bounded production policy.
/// Eligibility remains enforced by the process scheduler; this gate only
/// admits work after all boot probes and CPU0-owned device services are ready.
pub(crate) fn enable_production_userspace_scheduler() {
    SMP_USER_SCHEDULER_ENABLED.store(true, Ordering::Release);
    send_scheduler_ipi();
}

pub(crate) fn send_scheduler_ipi() {
    let distributor = GIC_DISTRIBUTOR_BASE.load(Ordering::Acquire);
    if distributor == 0 {
        crate::fatal("AArch64 scheduler IPI before GIC");
    }
    unsafe {
        asm!("dsb ish", options(nostack));
        // Target-list filter 1 sends this SGI to every PE except the requester.
        mmio_write32(distributor + GICD_SGIR, (1 << 24) | SMP_SCHEDULER_SGI);
        asm!("dsb ish", "isb", options(nostack));
    }
}

pub(crate) fn disable_smp_probe_scheduler() {
    SMP_USER_SCHEDULER_ENABLED.store(false, Ordering::Release);
    unsafe { asm!("dsb ish", "sev", options(nostack)) };
}

pub(crate) fn smp_probe_scheduler_enabled() -> bool {
    SMP_USER_SCHEDULER_ENABLED.load(Ordering::Acquire)
}

pub(crate) fn idle_secondary_after_smp_probe() -> ! {
    disable_interrupts();
    unsafe { asm!("msr CNTV_CTL_EL0, {value}", value = in(reg) 0u64, options(nostack)) };
    secondary_scheduler_idle()
}

fn init_exceptions_on_current_cpu() {
    let mut cpacr: u64;
    unsafe {
        asm!("mrs {value}, CPACR_EL1", value = out(reg) cpacr, options(nomem, nostack));
        cpacr |= 0b11 << 20;
        asm!(
            "msr CPACR_EL1, {cpacr}",
            "msr VBAR_EL1, {vectors}",
            "isb",
            cpacr = in(reg) cpacr,
            vectors = in(reg) core::ptr::addr_of!(aarch64_vectors) as u64,
            options(nostack)
        );
    }
}

fn init_gic_cpu_interface_on_current_cpu() {
    let cpu_interface = GIC_CPU_BASE.load(Ordering::Acquire);
    if cpu_interface == 0 {
        secondary_park_forever();
    }
    unsafe {
        mmio_write32(cpu_interface + GICC_PMR, 0xff);
        mmio_write32(cpu_interface + GICC_BPR, 0);
        mmio_write32(cpu_interface + GICC_CTLR, 0b111);
        asm!("dsb sy", "isb", options(nostack));
    }
}

fn init_secondary_timer_on_current_cpu() {
    let distributor = GIC_DISTRIBUTOR_BASE.load(Ordering::Acquire);
    let timer_intid = TIMER_INTID.load(Ordering::Acquire);
    let timer_flags = TIMER_FLAGS.load(Ordering::Acquire);
    let interval = TIMER_INTERVAL.load(Ordering::Acquire);
    if distributor == 0 || !(16..32).contains(&timer_intid) || interval == 0 {
        secondary_park_forever();
    }
    let bit = 1u32 << timer_intid;
    unsafe {
        // GICv2 PPI enable/group/priority/config registers are banked per PE.
        mmio_write32(distributor + GICD_ICENABLER0, bit);
        mmio_write32(distributor + GICD_ICPENDR0, bit);
        let group = mmio_read32(distributor + GICD_IGROUPR0) | bit;
        mmio_write32(distributor + GICD_IGROUPR0, group);
        let priority_register = distributor + GICD_IPRIORITYR + u64::from(timer_intid & !3);
        let shift = (timer_intid & 3) * 8;
        let priority = (mmio_read32(priority_register) & !(0xff << shift)) | (0x80 << shift);
        mmio_write32(priority_register, priority);
        let config_shift = (timer_intid - 16) * 2;
        let mut config = mmio_read32(distributor + GICD_ICFGR1) & !(0b11 << config_shift);
        if timer_flags & 1 != 0 {
            config |= 0b10 << config_shift;
        }
        mmio_write32(distributor + GICD_ICFGR1, config);
        mmio_write32(distributor + GICD_ISENABLER0, bit);
        asm!("dsb sy", "isb", options(nostack));
    }
    program_virtual_timer(read_virtual_counter().saturating_add(interval));
    enable_interrupts();
}

fn secondary_work_counter(logical_id: usize) -> &'static AtomicU64 {
    match logical_id {
        1 => &SMP_WORK_CPU1.0,
        2 => &SMP_WORK_CPU2.0,
        3 => &SMP_WORK_CPU3.0,
        _ => secondary_park_forever(),
    }
}

fn secondary_work_counts() -> [u64; SECONDARY_CPU_COUNT] {
    [
        SMP_WORK_CPU1.0.load(Ordering::Acquire),
        SMP_WORK_CPU2.0.load(Ordering::Acquire),
        SMP_WORK_CPU3.0.load(Ordering::Acquire),
    ]
}

fn read_mpidr() -> u64 {
    let value: u64;
    unsafe { asm!("mrs {value}, MPIDR_EL1", value = out(reg) value, options(nomem, nostack)) };
    value & MPIDR_AFFINITY_MASK
}

fn psci_hvc(function: u64, argument0: u64, argument1: u64, argument2: u64) -> i64 {
    let result: u64;
    unsafe {
        asm!(
            "hvc #0",
            inout("x0") function => result,
            in("x1") argument0,
            in("x2") argument1,
            in("x3") argument2,
            lateout("x4") _,
            lateout("x5") _,
            lateout("x6") _,
            lateout("x7") _,
            lateout("x8") _,
            lateout("x9") _,
            lateout("x10") _,
            lateout("x11") _,
            lateout("x12") _,
            lateout("x13") _,
            lateout("x14") _,
            lateout("x15") _,
            lateout("x16") _,
            lateout("x17") _,
            options(nostack)
        );
    }
    result as i64
}

fn secondary_park_forever() -> ! {
    disable_interrupts();
    loop {
        // WFI ignores the system-wide event register. IRQs remain masked and
        // AP-local timers are disabled, so routine BSP SEV traffic cannot
        // consume three host cores while userspace remains BSP-scheduled.
        unsafe { asm!("wfi", options(nomem, nostack)) };
    }
}

pub fn monotonic_ticks() -> u64 {
    TIMER_TICKS.load(Ordering::Acquire)
}

pub(crate) fn counter_deadline_millis(milliseconds: u64) -> u64 {
    let frequency = TIMER_FREQUENCY.load(Ordering::Acquire);
    read_virtual_counter().saturating_add(frequency.saturating_mul(milliseconds) / 1_000)
}

pub(crate) fn counter_deadline_expired(deadline: u64) -> bool {
    read_virtual_counter() >= deadline
}

pub fn uptime_millis() -> u64 {
    let frequency = TIMER_FREQUENCY.load(Ordering::Acquire);
    if frequency == 0 {
        return 0;
    }
    read_virtual_counter()
        .saturating_sub(BOOT_COUNTER.load(Ordering::Acquire))
        .saturating_mul(1000)
        / frequency
}

pub(crate) fn start_scheduler_timer() {
    let interval = TIMER_INTERVAL.load(Ordering::Acquire);
    if interval == 0 {
        crate::fatal("AArch64 scheduler timer unavailable");
    }
    program_virtual_timer(read_virtual_counter().saturating_add(interval));
}

pub(crate) fn stop_scheduler_timer() {
    unsafe { asm!("msr CNTV_CTL_EL0, {value}", value = in(reg) 0u64, options(nostack)) };
}

#[unsafe(no_mangle)]
extern "C" fn aarch64_exception_dispatch(kind: u64, frame: *mut ExceptionFrame) {
    let Some(frame) = (unsafe { frame.as_mut() }) else {
        crate::fatal("AArch64 null exception frame");
    };
    if matches!(kind, 1 | 5 | 9 | 13) {
        handle_irq(kind, frame);
        return;
    }
    let exception_class = frame.esr >> 26;
    let immediate = frame.esr & 0xffff;
    if kind == 8 && exception_class == 0x15 {
        handle_svc(frame);
        let _ = crate::aarch64_process::stop_remote_group_member_on_el0_return(frame);
        return;
    }
    let fault_status = frame.esr & 0x3f;
    if kind == 8
        && matches!(exception_class, 0x20 | 0x24)
        && matches!(fault_status, 0x4..=0x7)
        && crate::aarch64_vm::handle_page_fault(
            crate::aarch64_process::current_pid(),
            frame.far,
            exception_class == 0x24 && frame.esr & (1 << 6) != 0,
            exception_class == 0x20,
        )
    {
        let _ = crate::aarch64_process::stop_remote_group_member_on_el0_return(frame);
        return;
    }
    if matches!(kind, 0 | 4 | 8 | 12) && exception_class == 0x3c && immediate == BRK_SELF_TEST {
        frame.elr = frame.elr.saturating_add(4);
        SYNC_SELF_TESTS.fetch_add(1, Ordering::AcqRel);
        return;
    }
    crate::serial_println!(
        "MAKOS_AARCH64_EXCEPTION kind={} pid={} tid={} esr={:#x} far={:#x} elr={:#x} lr={:#x} sp={:#x} x0={:#x} x1={:#x} x2={:#x} spsr={:#x}",
        kind,
        crate::aarch64_process::current_pid(),
        crate::aarch64_process::current_tid(),
        frame.esr,
        frame.far,
        frame.elr,
        frame.registers[30],
        frame.sp_el0,
        frame.registers[0],
        frame.registers[1],
        frame.registers[2],
        frame.spsr,
    );
    if crate::aarch64_process::current_app_role() == crate::aarch64_process::ProcessRole::Firefox {
        crate::serial_println!(
            "MAKOS_FIREFOX_CRASH_REGS x19={:#x} x20={:#x} x21={:#x} x22={:#x} x23={:#x} x24={:#x} x25={:#x} x26={:#x} x27={:#x} x28={:#x} fp={:#x}",
            frame.registers[19],
            frame.registers[20],
            frame.registers[21],
            frame.registers[22],
            frame.registers[23],
            frame.registers[24],
            frame.registers[25],
            frame.registers[26],
            frame.registers[27],
            frame.registers[28],
            frame.registers[29],
        );
        let pid = crate::aarch64_process::current_pid();
        crate::aarch64_vm::trace_address(pid, "far", frame.far);
        crate::aarch64_vm::trace_address(pid, "x0", frame.registers[0]);
        crate::aarch64_vm::trace_address(pid, "x1", frame.registers[1]);
        crate::aarch64_vm::trace_address(pid, "x24", frame.registers[24]);
        crate::aarch64_vm::trace_address(pid, "x25", frame.registers[25]);
        crate::aarch64_vm::trace_address(pid, "sp", frame.sp_el0);
        if user_range_readable(frame.registers[24], 16) {
            let first = unsafe { core::ptr::read_volatile(frame.registers[24] as *const u64) };
            let second =
                unsafe { core::ptr::read_volatile((frame.registers[24] + 8) as *const u64) };
            crate::serial_println!(
                "MAKOS_FIREFOX_CRASH_SOURCE address={:#x} qword0={:#x} qword1={:#x}",
                frame.registers[24],
                first,
                second,
            );
        }
        let address = frame.registers[0];
        let mut reason = [0u8; 160];
        let mut length = 0usize;
        while length < reason.len() && user_range_readable(address + length as u64, 1) {
            let byte = unsafe { core::ptr::read_volatile((address + length as u64) as *const u8) };
            if byte == 0 {
                break;
            }
            if !(byte == b'\t' || (0x20..=0x7e).contains(&byte)) {
                length = 0;
                break;
            }
            reason[length] = byte;
            length += 1;
        }
        if length != 0 {
            crate::serial_println!(
                "MAKOS_FIREFOX_CRASH_REASON {}",
                core::str::from_utf8(&reason[..length]).unwrap_or("<invalid>")
            );
        }
    }
    if kind == 8 && matches!(exception_class, 0x20 | 0x24) {
        let pid = crate::aarch64_process::current_pid();
        let tid = crate::aarch64_process::current_tid();
        crate::serial_println!(
            "MAKOS_AARCH64_USER_FAULT_OK pid={} tid={} ec={:#x} signal=SIGSEGV status=139 containment=process-group",
            pid,
            tid,
            exception_class,
        );
        crate::aarch64_process::exit_group_from_exception(139, frame);
        return;
    }
    crate::fatal("unhandled AArch64 exception");
}

fn vfs_wait_source(fd: u64) -> makos_readiness::WaitSource {
    crate::vfs::io_wait_key(fd)
        .map(makos_readiness::WaitSource::Descriptor)
        .unwrap_or(makos_readiness::WaitSource::Any)
}

fn network_wait_source(handle: u64) -> makos_readiness::WaitSource {
    makos_readiness::WaitSource::Network(handle)
}

fn socket_address_length(domain: u64) -> Option<usize> {
    match domain {
        crate::aarch64_socket::AF_INET => Some(16),
        crate::aarch64_socket::AF_INET6 => Some(28),
        _ => None,
    }
}

fn read_socket_endpoint(address: u64, length: usize) -> Option<crate::aarch64_socket::Endpoint> {
    if length < 2 || !user_range_readable(address, length) {
        return None;
    }
    let bytes = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
    let family = u16::from_le_bytes([bytes[0], bytes[1]]);
    let port = (length >= 4).then(|| u16::from_be_bytes([bytes[2], bytes[3]]))?;
    match u64::from(family) {
        // Eight-byte form is retained for pre-IPv6 MakOS libc images.
        crate::aarch64_socket::AF_INET if length >= 8 => Some(crate::aarch64_socket::Endpoint {
            address: crate::aarch64_socket::IpAddress::V4([bytes[4], bytes[5], bytes[6], bytes[7]]),
            port,
        }),
        crate::aarch64_socket::AF_INET6 if length >= 28 => {
            // Flow labels and scoped link-local endpoints are not implemented.
            if bytes[4..8] != [0; 4] || bytes[24..28] != [0; 4] {
                return None;
            }
            let mut ipv6 = [0u8; 16];
            ipv6.copy_from_slice(&bytes[8..24]);
            Some(crate::aarch64_socket::Endpoint {
                address: crate::aarch64_socket::IpAddress::V6(ipv6),
                port,
            })
        }
        _ => None,
    }
}

fn write_socket_endpoint(
    endpoint: crate::aarch64_socket::Endpoint,
    address: u64,
    length_address: u64,
) -> Result<(), i64> {
    let required = match endpoint.address {
        crate::aarch64_socket::IpAddress::V4(_) => 16usize,
        crate::aarch64_socket::IpAddress::V6(_) => 28usize,
    };
    let capacity = unsafe { (length_address as *const u32).read_unaligned() } as usize;
    if capacity < required
        || !crate::aarch64_vm::fault_in_range(
            crate::aarch64_process::current_pid(),
            address,
            required,
            true,
        )
        || !user_range_writable(address, required)
    {
        return Err(-22);
    }
    let mut sockaddr = [0u8; 28];
    let port = endpoint.port.to_be_bytes();
    sockaddr[2..4].copy_from_slice(&port);
    match endpoint.address {
        crate::aarch64_socket::IpAddress::V4(ipv4) => {
            sockaddr[..2].copy_from_slice(&(crate::aarch64_socket::AF_INET as u16).to_le_bytes());
            sockaddr[4..8].copy_from_slice(&ipv4);
        }
        crate::aarch64_socket::IpAddress::V6(ipv6) => {
            sockaddr[..2].copy_from_slice(&(crate::aarch64_socket::AF_INET6 as u16).to_le_bytes());
            sockaddr[8..24].copy_from_slice(&ipv6);
        }
    }
    unsafe {
        core::ptr::copy_nonoverlapping(sockaddr.as_ptr(), address as *mut u8, required);
        (length_address as *mut u32).write_unaligned(required as u32);
    }
    Ok(())
}

fn wake_vfs_source(fd: u64) {
    crate::aarch64_process::wake_io_source(vfs_wait_source(fd));
}

fn typed_service_publish(address: u64, length: usize) -> u64 {
    if !crate::security::has_capability(crate::security::CAP_IPC)
        || !crate::security::has_capability(crate::security::CAP_SERVICE_PUBLISH)
        || !user_range_readable(address, length)
    {
        return u64::MAX;
    }
    let name = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
    crate::ipc::typed_publish(name).unwrap_or(u64::MAX)
}

fn typed_service_connect(address: u64, length: usize) -> u64 {
    if !crate::security::has_capability(crate::security::CAP_IPC)
        || !user_range_readable(address, length)
    {
        return u64::MAX;
    }
    let name = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
    crate::ipc::typed_connect(name).unwrap_or(u64::MAX)
}

fn typed_service_accept(listener: u64) -> u64 {
    if !crate::security::has_capability(crate::security::CAP_IPC)
        || !crate::security::has_capability(crate::security::CAP_SERVICE_PUBLISH)
    {
        return u64::MAX;
    }
    crate::ipc::typed_accept(listener).unwrap_or(u64::MAX)
}

fn typed_channel_send(endpoint: u64, address: u64, transfer: u64, rights: u8) -> u64 {
    if !crate::security::has_capability(crate::security::CAP_IPC)
        || !user_range_readable(address, makos_ipc::MESSAGE_WIRE_SIZE)
    {
        return u64::MAX;
    }
    let mut message = [0u8; makos_ipc::MESSAGE_WIRE_SIZE];
    unsafe {
        core::ptr::copy_nonoverlapping(
            address as *const u8,
            message.as_mut_ptr(),
            makos_ipc::MESSAGE_WIRE_SIZE,
        );
    }
    if crate::ipc::typed_send(endpoint, message, transfer, rights) {
        0
    } else {
        u64::MAX
    }
}

fn typed_channel_receive(endpoint: u64, message_address: u64, transfer_address: u64) -> u64 {
    if !crate::security::has_capability(crate::security::CAP_IPC)
        || !user_range_writable(message_address, makos_ipc::MESSAGE_WIRE_SIZE)
        || !user_range_writable(transfer_address, 8)
    {
        return u64::MAX;
    }
    let Some((message, transfer)) = crate::ipc::typed_receive(endpoint) else {
        return u64::MAX;
    };
    unsafe {
        core::ptr::copy_nonoverlapping(
            message.as_ptr(),
            message_address as *mut u8,
            makos_ipc::MESSAGE_WIRE_SIZE,
        );
        (transfer_address as *mut u64).write_unaligned(transfer);
    }
    0
}

fn handle_svc(frame: &mut ExceptionFrame) {
    const SYS_WRITE: u64 = 0;
    const SYS_YIELD: u64 = 1;
    const SYS_CHANNEL_CREATE: u64 = 2;
    const SYS_CHANNEL_SEND: u64 = 3;
    const SYS_CHANNEL_RECEIVE: u64 = 4;
    const SYS_EXIT: u64 = 5;
    const SYS_READ_KEY: u64 = 6;
    const SYS_SHELL_COMMAND: u64 = 7;
    const SYS_SURFACE_CREATE: u64 = 8;
    const SYS_SURFACE_FILL: u64 = 9;
    const SYS_SURFACE_PRESENT: u64 = 10;
    const SYS_OPEN: u64 = 11;
    const SYS_READ: u64 = 12;
    const SYS_CLOSE: u64 = 13;
    const SYS_PROCESS_SPAWN: u64 = 14;
    const SYS_PROCESS_WAIT: u64 = 15;
    const SYS_FILE_WRITE: u64 = 17;
    const SYS_PACKAGE_INSTALL: u64 = 18;
    const SYS_PACKAGE_QUERY: u64 = 19;
    const SYS_PACKAGE_ROLLBACK: u64 = 20;
    const SYS_VM_MAP: u64 = 21;
    const SYS_VM_UNMAP: u64 = 22;
    const SYS_CLOCK_MONOTONIC: u64 = 27;
    const SYS_LOG_APPEND: u64 = 28;
    const SYS_LOG_READ: u64 = 29;
    const SYS_AUTH_LOGIN: u64 = 30;
    const SYS_ABI_INFO: u64 = 31;
    const SYS_EVENT_CREATE: u64 = 32;
    const SYS_EVENT_SIGNAL: u64 = 33;
    const SYS_EVENT_WAIT: u64 = 34;
    const SYS_HANDLE_CLOSE: u64 = 35;
    const SYS_STAT: u64 = 36;
    const SYS_READ_DIR: u64 = 37;
    const SYS_CREATE: u64 = 43;
    const SYS_UNLINK: u64 = 44;
    const SYS_VM_PROTECT: u64 = 45;
    const SYS_SOCKET_CREATE: u64 = 47;
    const SYS_SOCKET_CONNECT: u64 = 48;
    const SYS_SOCKET_SEND: u64 = 49;
    const SYS_SOCKET_RECEIVE: u64 = 50;
    const SYS_SOCKET_CLOSE: u64 = 51;
    const SYS_PACKAGE_REMOVE: u64 = 52;
    const SYS_VM_MAP_RANGE: u64 = 53;
    const SYS_VM_UNMAP_RANGE: u64 = 54;
    const SYS_VM_PROTECT_RANGE: u64 = 55;
    const SYS_PROCESS_SPAWN_PATH: u64 = 56;
    const SYS_PROCESS_SPAWN_PATH_ARGS: u64 = 57;
    const SYS_SURFACE_CLOSE: u64 = 58;
    const SYS_SURFACE_TEXT: u64 = 59;
    const SYS_SURFACE_READ_EVENT: u64 = 60;
    const SYS_NET_CONFIG: u64 = 61;
    const SYS_TTY_READ: u64 = 62;
    const SYS_TTY_WRITE: u64 = 63;
    const SYS_ISATTY: u64 = 64;
    const SYS_TCGETATTR: u64 = 65;
    const SYS_TCSETATTR: u64 = 66;
    const SYS_TCFLUSH: u64 = 67;
    const SYS_IOCTL: u64 = 68;
    const SYS_SIGACTION: u64 = 69;
    const SYS_RAISE: u64 = 70;
    const SYS_GETPGRP: u64 = 71;
    const SYS_SETPGID: u64 = 72;
    const SYS_TCGETPGRP: u64 = 73;
    const SYS_TCSETPGRP: u64 = 74;
    const SYS_SIGRETURN: u64 = 75;
    const SYS_BRK: u64 = 76;
    const SYS_RENAME: u64 = 77;
    const SYS_SET_TID_ADDRESS: u64 = 78;
    const SYS_GETTID: u64 = 79;
    const SYS_THREAD_CLONE: u64 = 80;
    const SYS_THREAD_EXIT: u64 = 81;
    const SYS_FUTEX: u64 = 82;
    const SYS_GETRANDOM: u64 = 83;
    const SYS_CLOCK_REALTIME: u64 = 84;
    const SYS_PROCESS_IDENTITY: u64 = 85;
    const SYS_FD_DUP: u64 = 86;
    const SYS_FD_SEEK: u64 = 87;
    const SYS_FD_DUP3: u64 = 88;
    const SYS_FD_CONTROL: u64 = 89;
    const SYS_PIPE2: u64 = 90;
    const SYS_POLL: u64 = 91;
    const SYS_FSTAT_METADATA: u64 = 92;
    const SYS_READ_DIR_FD: u64 = 93;
    const SYS_CHDIR: u64 = 94;
    const SYS_GETCWD: u64 = 95;
    const SYS_FTRUNCATE: u64 = 96;
    const SYS_FSYNC: u64 = 97;
    const SYS_MKDIR: u64 = 98;
    const SYS_RMDIR: u64 = 99;
    const SYS_PREAD: u64 = 100;
    const SYS_PWRITE: u64 = 101;
    const SYS_CLOCK_RESOLUTION: u64 = 102;
    const SYS_SLEEP_UNTIL: u64 = 103;
    const SYS_EPOLL_CREATE: u64 = 104;
    const SYS_EPOLL_CTL: u64 = 105;
    const SYS_EPOLL_WAIT: u64 = 106;
    const SYS_EPOLL_CLOSE: u64 = 107;
    const SYS_SIGPROCMASK: u64 = 108;
    const SYS_PSELECT: u64 = 109;
    const SYS_CLIPBOARD_WRITE: u64 = 110;
    const SYS_CLIPBOARD_READ: u64 = 111;
    const SYS_PROCESS_EXEC: u64 = 112;
    const SYS_SURFACE_BLIT: u64 = 113;
    const SYS_MADVISE: u64 = 114;
    const SYS_THREAD_SET_NAME: u64 = 115;
    const SYS_THREAD_SET_SCHEDULER: u64 = 116;
    const SYS_COMPAT_MISSING: u64 = 118;
    const SYS_PROCESS_EXIT: u64 = 119;
    const SYS_MREMAP_PROBE: u64 = 120;
    const SYS_FILESYSTEM_STATS: u64 = 121;
    const SYS_UNAME: u64 = 122;
    const SYS_SURFACE_DESTROY: u64 = 123;
    const SYS_SOCKETPAIR: u64 = 124;
    const SYS_PROCESS_FORK: u64 = 125;
    const SYS_PROCESS_WAIT_STATUS: u64 = 126;
    const SYS_SENDMSG_RIGHTS: u64 = 127;
    const SYS_RECVMSG_RIGHTS: u64 = 128;
    const SYS_SESSION_STATUS: u64 = 129;
    const SYS_SYMLINK: u64 = 130;
    const SYS_READLINK: u64 = 131;
    const SYS_STAT_EXTENDED: u64 = 132;
    const SYS_FSTAT_EXTENDED: u64 = 133;
    const SYS_SOCKET_NAME: u64 = 134;
    const SYS_SOCKET_BIND: u64 = 135;
    const SYS_SURFACE_WAIT_EVENT: u64 = 140;
    const SYS_ROBUST_LIST: u64 = 141;
    const SYS_SIGNAL: u64 = 142;
    const SYS_TYPED_SERVICE_PUBLISH: u64 = 143;
    const SYS_TYPED_SERVICE_CONNECT: u64 = 144;
    const SYS_TYPED_SERVICE_ACCEPT: u64 = 145;
    const SYS_TYPED_CHANNEL_SEND: u64 = 146;
    const SYS_TYPED_CHANNEL_RECEIVE: u64 = 147;
    const ABI_FEATURE_IPC: u64 = 1 << 0;
    const ABI_FEATURE_PROCESS: u64 = 1 << 1;
    const ABI_FEATURE_VM: u64 = 1 << 2;
    const ABI_FEATURE_VFS: u64 = 1 << 3;
    const ABI_FEATURE_NETWORK: u64 = 1 << 4;
    const ABI_FEATURE_GRAPHICS: u64 = 1 << 5;
    const ABI_FEATURE_AUTH: u64 = 1 << 6;
    const ABI_FEATURE_LOG: u64 = 1 << 7;
    const ABI_FEATURE_SYNC: u64 = 1 << 8;
    const ABI_FEATURE_IPV6: u64 = 1 << 11;
    const ABI_FEATURE_SELF_HOSTING_SEED: u64 = 1 << 14;
    const ABI_FEATURE_SOCKET_OBJECTS: u64 = 1 << 15;
    const ABI_FEATURE_PACKAGE_TRANSACTIONS: u64 = 1 << 16;
    const ABI_FEATURE_VM_REGIONS: u64 = 1 << 17;
    const ABI_FEATURE_EXEC_BY_PATH: u64 = 1 << 18;
    const ABI_FEATURE_PROCESS_STARTUP: u64 = 1 << 19;
    const ABI_FEATURE_TTY_SIGNALS: u64 = 1 << 20;
    const ABI_FEATURE_TYPED_IPC: u64 = 1 << 21;
    const ABI_FEATURES: u64 = ABI_FEATURE_IPC
        | ABI_FEATURE_PROCESS
        | ABI_FEATURE_VM
        | ABI_FEATURE_VFS
        | ABI_FEATURE_NETWORK
        | ABI_FEATURE_GRAPHICS
        | ABI_FEATURE_AUTH
        | ABI_FEATURE_LOG
        | ABI_FEATURE_SYNC
        | ABI_FEATURE_IPV6
        | ABI_FEATURE_SELF_HOSTING_SEED
        | ABI_FEATURE_SOCKET_OBJECTS
        | ABI_FEATURE_PACKAGE_TRANSACTIONS
        | ABI_FEATURE_VM_REGIONS
        | ABI_FEATURE_EXEC_BY_PATH
        | ABI_FEATURE_PROCESS_STARTUP
        | ABI_FEATURE_TTY_SIGNALS
        | ABI_FEATURE_TYPED_IPC;
    const ERROR_INVALID: u64 = u64::MAX;

    match frame.registers[8] {
        SYS_RECVMSG_RIGHTS => {
            const MSG_DONTWAIT: u64 = 0x40;
            const MSG_NOSIGNAL: u64 = 0x4000;
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            let flags = frame.registers[3];
            let rights_address = frame.registers[4];
            let rights_capacity = frame.registers[5] as usize;
            let data_writable = crate::aarch64_vm::fault_in_range(
                crate::aarch64_process::current_pid(),
                address,
                length,
                true,
            ) && user_range_writable(address, length);
            let rights_bytes = rights_capacity.checked_mul(core::mem::size_of::<u64>());
            let rights_writable = rights_bytes.is_some_and(|bytes| {
                crate::aarch64_vm::fault_in_range(
                    crate::aarch64_process::current_pid(),
                    rights_address,
                    bytes,
                    true,
                ) && user_range_writable(rights_address, bytes)
            });
            if !crate::security::has_capability(crate::security::CAP_IPC)
                || !crate::vfs::is_pipe_owned(frame.registers[0])
                || flags & !(MSG_DONTWAIT | MSG_NOSIGNAL) != 0
                || length == 0
                || rights_capacity == 0
                || rights_capacity > 200
                || !data_writable
                || !rights_writable
            {
                frame.registers[0] = (-22i64) as u64;
            } else {
                let output = unsafe { core::slice::from_raw_parts_mut(address as *mut u8, length) };
                let rights = unsafe {
                    core::slice::from_raw_parts_mut(rights_address as *mut u64, rights_capacity)
                };
                match crate::vfs::read_result_with_rights(frame.registers[0], output, rights) {
                    Ok((count, rights_count)) => {
                        crate::aarch64_process::complete_io_wait();
                        if count != 0 {
                            wake_vfs_source(frame.registers[0]);
                        }
                        frame.registers[0] = ((rights_count as u64) << 32) | count as u64;
                    }
                    Err(crate::vfs::DescriptorError::Again)
                        if flags & MSG_DONTWAIT == 0
                            && !crate::vfs::is_nonblocking(frame.registers[0]).unwrap_or(true) =>
                    {
                        match crate::aarch64_process::block_current_for_io_on(
                            -1,
                            vfs_wait_source(frame.registers[0]),
                            frame,
                        ) {
                            crate::aarch64_process::IoBlockResult::Switched => return,
                            _ => frame.registers[0] = (-11i64) as u64,
                        }
                    }
                    Err(error) => {
                        crate::aarch64_process::complete_io_wait();
                        frame.registers[0] = error.abi();
                    }
                }
            }
            finish_signal_delivery(frame);
            return;
        }
        SYS_SENDMSG_RIGHTS => {
            const MSG_DONTWAIT: u64 = 0x40;
            const MSG_NOSIGNAL: u64 = 0x4000;
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            let flags = frame.registers[3];
            let rights_address = frame.registers[4];
            let rights_count = frame.registers[5] as usize;
            let rights_bytes = rights_count.checked_mul(core::mem::size_of::<u64>());
            if !crate::security::has_capability(crate::security::CAP_IPC)
                || !crate::vfs::is_pipe_owned(frame.registers[0])
                || flags & !(MSG_DONTWAIT | MSG_NOSIGNAL) != 0
                || length == 0
                || rights_count == 0
                || rights_count > 200
                || !user_range_readable(address, length)
                || !rights_bytes.is_some_and(|bytes| user_range_readable(rights_address, bytes))
            {
                frame.registers[0] = (-22i64) as u64;
            } else {
                let input = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                let rights = unsafe {
                    core::slice::from_raw_parts(rights_address as *const u64, rights_count)
                };
                match crate::vfs::send_result_with_rights(frame.registers[0], input, rights) {
                    Ok(count) => {
                        crate::aarch64_process::complete_io_wait();
                        if count != 0 {
                            wake_vfs_source(frame.registers[0]);
                        }
                        frame.registers[0] = count as u64;
                    }
                    Err(crate::vfs::DescriptorError::Again)
                        if flags & MSG_DONTWAIT == 0
                            && !crate::vfs::is_nonblocking(frame.registers[0]).unwrap_or(true) =>
                    {
                        match crate::aarch64_process::block_current_for_io_on(
                            -1,
                            vfs_wait_source(frame.registers[0]),
                            frame,
                        ) {
                            crate::aarch64_process::IoBlockResult::Switched => return,
                            _ => frame.registers[0] = (-11i64) as u64,
                        }
                    }
                    Err(error) => {
                        crate::aarch64_process::complete_io_wait();
                        frame.registers[0] = error.abi();
                    }
                }
            }
            finish_signal_delivery(frame);
            return;
        }
        SYS_SOCKET_RECEIVE if crate::vfs::is_pipe_owned(frame.registers[0]) => {
            const MSG_DONTWAIT: u64 = 0x40;
            const MSG_NOSIGNAL: u64 = 0x4000;
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            let flags = frame.registers[3];
            let capable = crate::security::has_capability(crate::security::CAP_IPC);
            let writable = crate::aarch64_vm::fault_in_range(
                crate::aarch64_process::current_pid(),
                address,
                length,
                true,
            ) && user_range_writable(address, length);
            if !capable
                || flags & !(MSG_DONTWAIT | MSG_NOSIGNAL) != 0
                || frame.registers[4] != 0
                || frame.registers[5] != 0
                || !writable
            {
                if crate::aarch64_process::current_app_role()
                    == crate::aarch64_process::ProcessRole::Firefox
                {
                    crate::serial_println!(
                        "MAKOS_FIREFOX_LOCAL_RECV_REJECT pid={} fd={} address={:#x} length={} flags={:#x} name={:#x} namelen={:#x} cap_ipc={} writable={}",
                        crate::aarch64_process::current_pid(),
                        frame.registers[0],
                        address,
                        length,
                        flags,
                        frame.registers[4],
                        frame.registers[5],
                        u8::from(capable),
                        u8::from(writable),
                    );
                }
                frame.registers[0] = (-22i64) as u64;
            } else if length == 0 {
                frame.registers[0] = 0;
            } else {
                let output = unsafe { core::slice::from_raw_parts_mut(address as *mut u8, length) };
                match crate::vfs::read_result(frame.registers[0], output) {
                    Ok(count) => {
                        crate::aarch64_process::complete_io_wait();
                        if count != 0 {
                            wake_vfs_source(frame.registers[0]);
                        }
                        frame.registers[0] = count as u64;
                    }
                    Err(crate::vfs::DescriptorError::Again)
                        if flags & MSG_DONTWAIT == 0
                            && !crate::vfs::is_nonblocking(frame.registers[0]).unwrap_or(true) =>
                    {
                        match crate::aarch64_process::block_current_for_io_on(
                            -1,
                            vfs_wait_source(frame.registers[0]),
                            frame,
                        ) {
                            crate::aarch64_process::IoBlockResult::Switched => return,
                            _ => frame.registers[0] = (-11i64) as u64,
                        }
                    }
                    Err(error) => {
                        crate::aarch64_process::complete_io_wait();
                        frame.registers[0] = error.abi();
                    }
                }
            }
            finish_signal_delivery(frame);
            return;
        }
        SYS_SOCKET_SEND if crate::vfs::is_pipe_owned(frame.registers[0]) => {
            const MSG_DONTWAIT: u64 = 0x40;
            const MSG_NOSIGNAL: u64 = 0x4000;
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            let flags = frame.registers[3];
            if !crate::security::has_capability(crate::security::CAP_IPC)
                || flags & !(MSG_DONTWAIT | MSG_NOSIGNAL) != 0
                || frame.registers[4] != 0
                || frame.registers[5] != 0
                || !user_range_readable(address, length)
            {
                frame.registers[0] = (-22i64) as u64;
            } else if length == 0 {
                frame.registers[0] = 0;
            } else {
                let input = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                match crate::vfs::write_result(frame.registers[0], input) {
                    Ok(count) => {
                        crate::aarch64_process::complete_io_wait();
                        if count != 0 {
                            wake_vfs_source(frame.registers[0]);
                        }
                        frame.registers[0] = count as u64;
                    }
                    Err(crate::vfs::DescriptorError::Again)
                        if flags & MSG_DONTWAIT == 0
                            && !crate::vfs::is_nonblocking(frame.registers[0]).unwrap_or(true) =>
                    {
                        match crate::aarch64_process::block_current_for_io_on(
                            -1,
                            vfs_wait_source(frame.registers[0]),
                            frame,
                        ) {
                            crate::aarch64_process::IoBlockResult::Switched => return,
                            _ => frame.registers[0] = (-11i64) as u64,
                        }
                    }
                    Err(error) => {
                        crate::aarch64_process::complete_io_wait();
                        frame.registers[0] = error.abi();
                    }
                }
            }
            finish_signal_delivery(frame);
            return;
        }
        // Socket option opcodes overlap VFS record-lock opcodes. Route by
        // descriptor ownership before interpreting the operation number.
        SYS_FD_CONTROL if crate::aarch64_socket::is_owned(frame.registers[0]) => {
            frame.registers[0] = crate::aarch64_socket::control(
                frame.registers[0],
                frame.registers[1],
                frame.registers[2],
            )
            .unwrap_or_else(|error| error as u64);
            finish_signal_delivery(frame);
            return;
        }
        SYS_FD_CONTROL
            if matches!(frame.registers[1], 6..=10 | 13)
                && crate::vfs::local_socket_option(frame.registers[0], frame.registers[1])
                    .is_some() =>
        {
            frame.registers[0] =
                crate::vfs::local_socket_option(frame.registers[0], frame.registers[1])
                    .unwrap_or(0);
            finish_signal_delivery(frame);
            return;
        }
        SYS_FD_CONTROL if matches!(frame.registers[1], 6..=8) => {
            let operation = frame.registers[1];
            let address = frame.registers[2];
            let size = core::mem::size_of::<crate::vfs::FileLock>();
            if !user_range_readable(address, size)
                || (operation == 6 && !user_range_writable(address, size))
            {
                frame.registers[0] = (-22i64) as u64;
            } else {
                let mut lock = unsafe { (address as *const crate::vfs::FileLock).read_unaligned() };
                let result = if operation == 6 {
                    crate::vfs::get_file_lock(frame.registers[0], &mut lock)
                } else {
                    crate::vfs::set_file_lock(frame.registers[0], &lock)
                };
                match result {
                    Ok(value) => {
                        crate::aarch64_process::complete_io_wait();
                        if operation == 6 {
                            unsafe { (address as *mut crate::vfs::FileLock).write_unaligned(lock) };
                        } else {
                            crate::aarch64_process::wake_io_waiters();
                        }
                        frame.registers[0] = value;
                    }
                    Err(crate::vfs::DescriptorError::Again) if operation == 8 => {
                        match crate::aarch64_process::block_current_for_io(-1, frame) {
                            crate::aarch64_process::IoBlockResult::Switched => return,
                            _ => frame.registers[0] = (-11i64) as u64,
                        }
                    }
                    Err(error) => {
                        crate::aarch64_process::complete_io_wait();
                        frame.registers[0] = error.abi();
                    }
                }
            }
            finish_signal_delivery(frame);
            return;
        }
        SYS_READ if frame.registers[0] >= 3 && crate::vfs::is_pipe_owned(frame.registers[0]) => {
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            if !user_range_writable(address, length) {
                frame.registers[0] = (-22i64) as u64;
            } else {
                let output = unsafe { core::slice::from_raw_parts_mut(address as *mut u8, length) };
                match crate::vfs::read_result(frame.registers[0], output) {
                    Ok(count) => {
                        crate::aarch64_process::complete_io_wait();
                        if count != 0 {
                            wake_vfs_source(frame.registers[0]);
                        }
                        frame.registers[0] = count as u64;
                    }
                    Err(crate::vfs::DescriptorError::Again)
                        if !crate::vfs::is_nonblocking(frame.registers[0]).unwrap_or(true) =>
                    {
                        match crate::aarch64_process::block_current_for_io_on(
                            -1,
                            vfs_wait_source(frame.registers[0]),
                            frame,
                        ) {
                            crate::aarch64_process::IoBlockResult::Switched => return,
                            crate::aarch64_process::IoBlockResult::TimedOut => {
                                frame.registers[0] = 0
                            }
                            crate::aarch64_process::IoBlockResult::Failed => {
                                frame.registers[0] = (-11i64) as u64
                            }
                        }
                    }
                    Err(error) => {
                        crate::aarch64_process::complete_io_wait();
                        frame.registers[0] = error.abi();
                    }
                }
            }
            finish_signal_delivery(frame);
            return;
        }
        SYS_FILE_WRITE if crate::vfs::is_pipe_owned(frame.registers[0]) => {
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            if !crate::security::has_capability(crate::security::CAP_IPC)
                || !user_range_readable(address, length)
            {
                frame.registers[0] = (-22i64) as u64;
            } else {
                let input = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                match crate::vfs::write_result(frame.registers[0], input) {
                    Ok(count) => {
                        crate::aarch64_process::complete_io_wait();
                        if count != 0 {
                            wake_vfs_source(frame.registers[0]);
                        }
                        frame.registers[0] = count as u64;
                    }
                    Err(crate::vfs::DescriptorError::Again)
                        if !crate::vfs::is_nonblocking(frame.registers[0]).unwrap_or(true) =>
                    {
                        match crate::aarch64_process::block_current_for_io_on(
                            -1,
                            vfs_wait_source(frame.registers[0]),
                            frame,
                        ) {
                            crate::aarch64_process::IoBlockResult::Switched => return,
                            crate::aarch64_process::IoBlockResult::TimedOut => {
                                frame.registers[0] = (-11i64) as u64
                            }
                            crate::aarch64_process::IoBlockResult::Failed => {
                                frame.registers[0] = (-11i64) as u64
                            }
                        }
                    }
                    Err(error) => {
                        crate::aarch64_process::complete_io_wait();
                        frame.registers[0] = error.abi();
                    }
                }
            }
            finish_signal_delivery(frame);
            return;
        }
        SYS_SOCKET_RECEIVE if crate::aarch64_socket::is_owned(frame.registers[0]) => {
            service_network_rx_on_owner_cpu();
            const MSG_DONTWAIT: u64 = 0x40;
            const MSG_NOSIGNAL: u64 = 0x4000;
            let address = frame.registers[1];
            let length = (frame.registers[2] as usize).min(4096);
            let flags = frame.registers[3];
            let source_address = frame.registers[4];
            let source_length_address = frame.registers[5];
            let pid = crate::aarch64_process::current_pid();
            let source_required = crate::aarch64_socket::domain(frame.registers[0])
                .and_then(socket_address_length)
                .unwrap_or(0);
            let data_writable = crate::aarch64_vm::fault_in_range(pid, address, length, true)
                && user_range_writable(address, length);
            let source_writable = source_address == 0
                || (source_required != 0
                    && crate::aarch64_vm::fault_in_range(
                        pid,
                        source_address,
                        source_required,
                        true,
                    )
                    && user_range_writable(source_address, source_required));
            let source_length_writable = source_length_address == 0
                || (crate::aarch64_vm::fault_in_range(pid, source_length_address, 4, true)
                    && user_range_readable(source_length_address, 4)
                    && user_range_writable(source_length_address, 4));
            let source_capacity_ok = source_address == 0
                || (source_length_address != 0
                    && source_length_writable
                    && unsafe { (source_length_address as *const u32).read_unaligned() as usize }
                        >= source_required);
            if !crate::aarch64_process::network_control_allowed()
                || flags & !(MSG_DONTWAIT | MSG_NOSIGNAL) != 0
                || length == 0
                || !data_writable
                || ((source_address == 0) != (source_length_address == 0))
                || !source_writable
                || !source_length_writable
                || !source_capacity_ok
            {
                frame.registers[0] = (-22i64) as u64;
            } else {
                let output = unsafe { core::slice::from_raw_parts_mut(address as *mut u8, length) };
                match crate::aarch64_socket::receive_from(frame.registers[0], output) {
                    Some((count, source)) => {
                        crate::aarch64_process::complete_io_wait();
                        if source_address != 0 {
                            if let Err(error) =
                                write_socket_endpoint(source, source_address, source_length_address)
                            {
                                frame.registers[0] = error as u64;
                                finish_signal_delivery(frame);
                                return;
                            }
                        }
                        frame.registers[0] = count as u64;
                    }
                    None if flags & MSG_DONTWAIT == 0
                        && !crate::aarch64_socket::is_nonblocking(frame.registers[0])
                            .unwrap_or(true) =>
                    {
                        match crate::aarch64_process::block_current_for_io_on(
                            -1,
                            network_wait_source(frame.registers[0]),
                            frame,
                        ) {
                            crate::aarch64_process::IoBlockResult::Switched => return,
                            _ => frame.registers[0] = (-11i64) as u64,
                        }
                    }
                    None => frame.registers[0] = (-11i64) as u64,
                }
            }
            finish_signal_delivery(frame);
            return;
        }
        SYS_READ if crate::aarch64_socket::is_owned(frame.registers[0]) => {
            let address = frame.registers[1];
            let length = (frame.registers[2] as usize).min(4096);
            if !crate::aarch64_process::network_control_allowed() {
                frame.registers[0] = (-22i64) as u64;
            } else if length == 0 {
                frame.registers[0] = 0;
            } else if !user_range_writable(address, length) {
                frame.registers[0] = (-22i64) as u64;
            } else {
                let output = unsafe { core::slice::from_raw_parts_mut(address as *mut u8, length) };
                match crate::aarch64_socket::receive(frame.registers[0], output) {
                    Some(count) => {
                        crate::aarch64_process::complete_io_wait();
                        frame.registers[0] = count as u64;
                    }
                    None if !crate::aarch64_socket::is_nonblocking(frame.registers[0])
                        .unwrap_or(true) =>
                    {
                        match crate::aarch64_process::block_current_for_io_on(
                            -1,
                            network_wait_source(frame.registers[0]),
                            frame,
                        ) {
                            crate::aarch64_process::IoBlockResult::Switched => return,
                            _ => frame.registers[0] = (-11i64) as u64,
                        }
                    }
                    None => frame.registers[0] = (-11i64) as u64,
                }
            }
            finish_signal_delivery(frame);
            return;
        }
        SYS_FILE_WRITE if crate::aarch64_socket::is_owned(frame.registers[0]) => {
            let address = frame.registers[1];
            let length = (frame.registers[2] as usize).min(4096);
            frame.registers[0] = if !crate::aarch64_process::network_control_allowed() {
                (-22i64) as u64
            } else if length == 0 {
                0
            } else if !user_range_readable(address, length) {
                (-22i64) as u64
            } else {
                let input = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                crate::aarch64_socket::send(frame.registers[0], input)
                    .map_or((-11i64) as u64, |count| count as u64)
            };
            finish_signal_delivery(frame);
            return;
        }
        SYS_CLOSE if frame.registers[0] >= 3 && crate::vfs::is_pipe_owned(frame.registers[0]) => {
            let source = vfs_wait_source(frame.registers[0]);
            frame.registers[0] = u64::from(crate::vfs::close(frame.registers[0]));
            crate::aarch64_process::wake_io_source(source);
            finish_signal_delivery(frame);
            return;
        }
        SYS_CLOSE if crate::aarch64_socket::is_owned(frame.registers[0]) => {
            frame.registers[0] = u64::from(crate::aarch64_socket::close(frame.registers[0]));
            finish_signal_delivery(frame);
            return;
        }
        SYS_CLOSE if frame.registers[0] >= 3 => {
            frame.registers[0] = u64::from(crate::vfs::close(frame.registers[0]));
            crate::aarch64_process::wake_io_waiters();
            finish_signal_delivery(frame);
            return;
        }
        SYS_PIPE2 => {
            const O_NONBLOCK: u64 = 0x800;
            const O_CLOEXEC: u64 = 0x80000;
            let output = frame.registers[0];
            let flags = frame.registers[1];
            frame.registers[0] = if !crate::security::has_capability(crate::security::CAP_IPC)
                || !user_range_writable(output, 8)
                || flags & !(O_NONBLOCK | O_CLOEXEC) != 0
            {
                (-22i64) as u64
            } else {
                match crate::vfs::pipe_pair(flags & O_CLOEXEC != 0, flags & O_NONBLOCK != 0) {
                    Ok((read_fd, write_fd)) => {
                        unsafe {
                            core::ptr::write_volatile(output as *mut i32, read_fd as i32);
                            core::ptr::write_volatile((output + 4) as *mut i32, write_fd as i32);
                        }
                        0
                    }
                    Err(error) => error.abi(),
                }
            };
            finish_signal_delivery(frame);
            return;
        }
        SYS_POLL => {
            let address = frame.registers[0];
            let count = frame.registers[1] as usize;
            let timeout = frame.registers[2] as i64;
            let wait_mask = frame.registers[3];
            let wait_mask_present = frame.registers[4];
            let bytes = count.saturating_mul(8);
            if count > 32
                || timeout < -1
                || wait_mask_present > 1
                || !user_range_readable(address, bytes)
                || !user_range_writable(address, bytes)
            {
                frame.registers[0] = (-22i64) as u64;
            } else if let Err(error) =
                crate::aarch64_tty::begin_wait_mask((wait_mask_present != 0).then_some(wait_mask))
            {
                frame.registers[0] = error.abi();
            } else if crate::aarch64_tty::wait_interrupted() {
                crate::aarch64_process::complete_io_wait();
                frame.registers[0] = crate::aarch64_tty::Errno::Interrupted.abi();
            } else {
                let ready = poll_descriptors(address, count);
                if crate::aarch64_tty::wait_interrupted() {
                    crate::aarch64_process::complete_io_wait();
                    frame.registers[0] = crate::aarch64_tty::Errno::Interrupted.abi();
                } else if ready != 0 || timeout == 0 {
                    crate::aarch64_process::complete_io_wait();
                    crate::aarch64_tty::finish_wait_mask();
                    frame.registers[0] = ready;
                } else {
                    match crate::aarch64_process::block_current_for_io(timeout, frame) {
                        crate::aarch64_process::IoBlockResult::Switched => return,
                        crate::aarch64_process::IoBlockResult::TimedOut => {
                            crate::aarch64_tty::finish_wait_mask();
                            frame.registers[0] = 0
                        }
                        crate::aarch64_process::IoBlockResult::Failed => {
                            crate::aarch64_tty::finish_wait_mask();
                            frame.registers[0] = (-11i64) as u64
                        }
                    }
                }
            }
            finish_signal_delivery(frame);
            return;
        }
        SYS_EPOLL_WAIT => {
            let handle = frame.registers[0];
            let address = frame.registers[1];
            let maximum = frame.registers[2] as usize;
            let timeout = frame.registers[3] as i64;
            let wait_mask = frame.registers[4];
            let wait_mask_present = frame.registers[5];
            let bytes = maximum.saturating_mul(core::mem::size_of::<makos_readiness::Event>());
            if maximum == 0
                || maximum > 32
                || timeout < -1
                || wait_mask_present > 1
                || !user_range_writable(address, bytes)
            {
                frame.registers[0] = (-22i64) as u64;
            } else if let Err(error) =
                crate::aarch64_tty::begin_wait_mask((wait_mask_present != 0).then_some(wait_mask))
            {
                frame.registers[0] = error.abi();
            } else if crate::aarch64_tty::wait_interrupted() {
                crate::aarch64_process::complete_io_wait();
                frame.registers[0] = crate::aarch64_tty::Errno::Interrupted.abi();
            } else {
                let output = unsafe {
                    core::slice::from_raw_parts_mut(address as *mut makos_readiness::Event, maximum)
                };
                match crate::aarch64_epoll::collect(handle, output) {
                    Ok(count) if count != 0 || timeout == 0 => {
                        crate::aarch64_process::complete_io_wait();
                        crate::aarch64_tty::finish_wait_mask();
                        frame.registers[0] = count as u64;
                    }
                    Ok(_) => match crate::aarch64_process::block_current_for_io(timeout, frame) {
                        crate::aarch64_process::IoBlockResult::Switched => return,
                        crate::aarch64_process::IoBlockResult::TimedOut => {
                            crate::aarch64_tty::finish_wait_mask();
                            frame.registers[0] = 0
                        }
                        crate::aarch64_process::IoBlockResult::Failed => {
                            crate::aarch64_tty::finish_wait_mask();
                            frame.registers[0] = (-11i64) as u64
                        }
                    },
                    Err(error) => {
                        crate::aarch64_process::complete_io_wait();
                        crate::aarch64_tty::finish_wait_mask();
                        frame.registers[0] = crate::aarch64_epoll::error_abi(error);
                    }
                }
            }
            finish_signal_delivery(frame);
            return;
        }
        SYS_PSELECT => {
            if pselect(frame) {
                return;
            }
            finish_signal_delivery(frame);
            return;
        }
        SYS_CLIPBOARD_WRITE => {
            let address = frame.registers[0];
            let length = frame.registers[1] as usize;
            frame.registers[0] = if length > crate::aarch64_clipboard::CAPACITY
                || !user_range_readable(address, length)
            {
                (-22i64) as u64
            } else {
                let input = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                crate::aarch64_clipboard::write(input)
                    .map(|count| count as u64)
                    .unwrap_or((-13i64) as u64)
            };
            finish_signal_delivery(frame);
            return;
        }
        SYS_CLIPBOARD_READ => {
            let address = frame.registers[0];
            let capacity = frame.registers[1] as usize;
            frame.registers[0] = if capacity > crate::aarch64_clipboard::CAPACITY
                || !user_range_writable(address, capacity)
            {
                (-22i64) as u64
            } else {
                let output =
                    unsafe { core::slice::from_raw_parts_mut(address as *mut u8, capacity) };
                crate::aarch64_clipboard::read(output)
                    .map(|count| count as u64)
                    .unwrap_or((-13i64) as u64)
            };
            finish_signal_delivery(frame);
            return;
        }
        SYS_YIELD => {
            if crate::aarch64_process::migrate_smp_probe_from_exception(
                frame.registers[0] as usize,
                frame,
            ) {
                return;
            }
            if crate::aarch64_process::hold_smp_exit_group_probe_in_el1(frame.registers[0]) {
                frame.registers[0] = 0;
                return;
            }
            service_input_on_owner_cpu();
            frame.registers[0] = 0;
            finish_signal_delivery(frame);
            crate::aarch64_process::yield_from_exception(frame);
            return;
        }
        SYS_EXIT => {
            crate::aarch64_process::exit_from_exception(frame.registers[0], frame);
            return;
        }
        SYS_THREAD_EXIT => {
            crate::aarch64_process::exit_from_exception(frame.registers[0], frame);
            return;
        }
        SYS_PROCESS_EXIT => {
            crate::aarch64_process::exit_group_from_exception(frame.registers[0], frame);
            return;
        }
        SYS_MREMAP_PROBE => {
            let address = frame.registers[0];
            let old_length = frame.registers[1] as usize;
            let new_length = frame.registers[2] as usize;
            let flags = frame.registers[3];
            frame.registers[0] = if flags != 0
                || old_length == 0
                || new_length == 0
                || address & 4095 != 0
                || old_length & 4095 != 0
                || new_length & 4095 != 0
            {
                (-22i64) as u64
            } else if !user_range_mapped(address, old_length) {
                (-14i64) as u64
            } else if old_length == new_length {
                address
            } else {
                (-12i64) as u64
            };
            finish_signal_delivery(frame);
            return;
        }
        SYS_FILESYSTEM_STATS => {
            let output = frame.registers[0];
            frame.registers[0] = if !user_range_writable(output, 5 * 8) {
                (-14i64) as u64
            } else {
                match crate::makfs4_volume::volume_stats() {
                    Ok(stats) => {
                        let values = [
                            stats.block_size,
                            stats.total_blocks,
                            stats.free_blocks,
                            stats.total_inodes,
                            stats.free_inodes,
                        ];
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                values.as_ptr(),
                                output as *mut u64,
                                values.len(),
                            );
                        }
                        0
                    }
                    Err(_) => (-5i64) as u64,
                }
            };
            finish_signal_delivery(frame);
            return;
        }
        SYS_UNAME => {
            const FIELD_BYTES: usize = 65;
            const FIELD_COUNT: usize = 6;
            let output = frame.registers[0];
            frame.registers[0] = if !user_range_writable(output, FIELD_BYTES * FIELD_COUNT) {
                (-14i64) as u64
            } else {
                let mut identity = [0u8; FIELD_BYTES * FIELD_COUNT];
                for (index, value) in [
                    b"MakOS".as_slice(),
                    b"makos".as_slice(),
                    b"0.1.0".as_slice(),
                    b"MakOS native AArch64".as_slice(),
                    b"aarch64".as_slice(),
                    b"localdomain".as_slice(),
                ]
                .iter()
                .enumerate()
                {
                    let start = index * FIELD_BYTES;
                    identity[start..start + value.len()].copy_from_slice(value);
                }
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        identity.as_ptr(),
                        output as *mut u8,
                        identity.len(),
                    );
                }
                0
            };
            finish_signal_delivery(frame);
            return;
        }
        SYS_SURFACE_DESTROY => {
            frame.registers[0] = if crate::security::has_capability(crate::security::CAP_GRAPHICS) {
                u64::from(crate::graphics::destroy(frame.registers[0]))
            } else {
                ERROR_INVALID
            };
            finish_signal_delivery(frame);
            return;
        }
        SYS_SOCKETPAIR => {
            const AF_UNIX: u64 = 1;
            const SOCK_STREAM: u64 = 1;
            const SOCK_TYPE_MASK: u64 = 0xf;
            const SOCK_NONBLOCK: u64 = 0x800;
            const SOCK_CLOEXEC: u64 = 0x80000;
            let domain = frame.registers[0];
            let kind = frame.registers[1];
            let protocol = frame.registers[2];
            let output = frame.registers[3];
            frame.registers[0] = if !crate::security::has_capability(crate::security::CAP_IPC)
                || domain != AF_UNIX
                || kind & SOCK_TYPE_MASK != SOCK_STREAM
                || kind & !(SOCK_TYPE_MASK | SOCK_NONBLOCK | SOCK_CLOEXEC) != 0
                || protocol != 0
                || !user_range_writable(output, 8)
            {
                (-22i64) as u64
            } else {
                match crate::vfs::socket_pair(kind & SOCK_CLOEXEC != 0, kind & SOCK_NONBLOCK != 0) {
                    Ok((first, second)) => {
                        unsafe {
                            core::ptr::write_volatile(output as *mut i32, first as i32);
                            core::ptr::write_volatile((output + 4) as *mut i32, second as i32);
                        }
                        if crate::aarch64_process::current_app_role()
                            == crate::aarch64_process::ProcessRole::Firefox
                        {
                            crate::serial_println!(
                                "MAKOS_FIREFOX_SOCKETPAIR_OK first={} second={} stream=1 duplex=1",
                                first,
                                second,
                            );
                        }
                        0
                    }
                    Err(error) => error.abi(),
                }
            };
            finish_signal_delivery(frame);
            return;
        }
        SYS_THREAD_CLONE => {
            let flags = frame.registers[0];
            let stack = frame.registers[1];
            let parent_tid = frame.registers[2];
            let tls = frame.registers[3];
            let child_tid = frame.registers[4];
            let pointers_valid = stack >= 16
                && stack & 15 == 0
                && user_range_readable(stack - 16, 16)
                && user_range_writable(stack - 16, 16)
                && (parent_tid == 0 || user_range_writable(parent_tid, 4))
                && (child_tid == 0 || user_range_writable(child_tid, 4))
                && tls != 0
                && user_range_readable(tls, 1);
            frame.registers[0] = if pointers_valid {
                crate::aarch64_process::clone_thread(
                    flags, stack, parent_tid, tls, child_tid, frame,
                )
                .unwrap_or(ERROR_INVALID)
            } else {
                ERROR_INVALID
            };
            finish_signal_delivery(frame);
            return;
        }
        SYS_PROCESS_FORK => {
            frame.registers[0] =
                crate::aarch64_process::fork_process(frame).unwrap_or((-12i64) as u64);
            finish_signal_delivery(frame);
            return;
        }
        SYS_FUTEX => {
            crate::aarch64_process::futex(
                frame.registers[0],
                frame.registers[1] as u32,
                frame.registers[2] as u32,
                frame.registers[3],
                frame.registers[4],
                frame.registers[5] as u32,
                frame,
            );
            finish_signal_delivery(frame);
            return;
        }
        SYS_GETRANDOM => {
            let address = frame.registers[0];
            let length = frame.registers[1] as usize;
            let flags = frame.registers[2];
            frame.registers[0] =
                if length > 64 * 1024 || flags & !3 != 0 || !user_range_writable(address, length) {
                    ERROR_INVALID
                } else {
                    let output =
                        unsafe { core::slice::from_raw_parts_mut(address as *mut u8, length) };
                    if crate::aarch64_virtio_rng::fill(output) {
                        length as u64
                    } else {
                        ERROR_INVALID
                    }
                };
            finish_signal_delivery(frame);
            return;
        }
        SYS_CLOCK_REALTIME => {
            frame.registers[0] = crate::aarch64_rtc::unix_seconds();
            finish_signal_delivery(frame);
            return;
        }
        SYS_SLEEP_UNTIL => {
            crate::aarch64_process::sleep_until(frame.registers[0], frame);
            finish_signal_delivery(frame);
            return;
        }
        SYS_PROCESS_IDENTITY => {
            let credentials = crate::security::credentials();
            frame.registers[0] = match frame.registers[0] {
                0 => crate::aarch64_process::current_pid(),
                1 => u64::from(credentials.uid),
                2 => u64::from(credentials.gid),
                3 => crate::aarch64_process::current_parent_pid(),
                _ => ERROR_INVALID,
            };
            finish_signal_delivery(frame);
            return;
        }
        SYS_EVENT_WAIT => {
            let handle = frame.registers[0];
            frame.registers[0] = 1;
            if !crate::aarch64_process::ipc_control_allowed()
                || !crate::ipc::wait_event_from_exception(handle, frame)
            {
                frame.registers[0] = ERROR_INVALID;
            }
            finish_signal_delivery(frame);
            return;
        }
        SYS_SIGRETURN => {
            if let Err(error) = crate::aarch64_tty::signal_return(frame) {
                frame.registers[0] = error.abi();
            }
            finish_signal_delivery(frame);
            return;
        }
        SYS_PROCESS_EXEC => {
            if process_exec(frame) {
                return;
            }
            finish_signal_delivery(frame);
            return;
        }
        _ => {}
    }

    frame.registers[0] = match frame.registers[8] {
        SYS_WRITE => {
            let address = frame.registers[0];
            let length = frame.registers[1] as usize;
            if !user_range_readable(address, length) {
                ERROR_INVALID
            } else {
                let bytes = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                crate::serial::write_bytes(bytes);
                crate::graphics::console_write(bytes);
                length as u64
            }
        }
        SYS_CHANNEL_CREATE => {
            let output = frame.registers[0];
            if !crate::security::has_capability(crate::security::CAP_IPC)
                || !user_range_writable(output, 16)
            {
                ERROR_INVALID
            } else if let Some((first, second)) = crate::ipc::create_pair() {
                unsafe {
                    core::ptr::write_volatile(output as *mut u64, first);
                    core::ptr::write_volatile((output + 8) as *mut u64, second);
                }
                0
            } else {
                ERROR_INVALID
            }
        }
        SYS_CHANNEL_SEND => {
            if crate::security::has_capability(crate::security::CAP_IPC)
                && crate::ipc::send(frame.registers[0], frame.registers[1])
            {
                0
            } else {
                ERROR_INVALID
            }
        }
        SYS_CHANNEL_RECEIVE => {
            if crate::security::has_capability(crate::security::CAP_IPC) {
                crate::ipc::receive(frame.registers[0]).unwrap_or(ERROR_INVALID)
            } else {
                ERROR_INVALID
            }
        }
        SYS_READ_KEY => {
            service_input_on_owner_cpu();
            if let Some(key) = crate::aarch64_virtio_input::read_key() {
                crate::aarch64_process::complete_input_wait();
                u64::from(key)
            } else {
                if crate::aarch64_process::runtime_stats().runnable > 1 {
                    match crate::aarch64_process::block_current_for_input(frame) {
                        crate::aarch64_process::IoBlockResult::Switched => return,
                        _ => 0,
                    }
                } else {
                    // Last input waiter becomes the idle task. Timer handling
                    // continues waking sleeps/network work while host CPU rests.
                    enable_interrupts();
                    unsafe { asm!("wfi", options(nomem, nostack)) };
                    disable_interrupts();
                    service_input_on_owner_cpu();
                    if let Some(key) = crate::aarch64_virtio_input::read_key() {
                        crate::aarch64_process::complete_input_wait();
                        u64::from(key)
                    } else {
                        0
                    }
                }
            }
        }
        SYS_SHELL_COMMAND => {
            let address = frame.registers[0];
            let length = frame.registers[1] as usize;
            let terminal = frame.registers[2];
            if length > 127
                || !user_range_readable(address, length)
                || !crate::security::has_capability(crate::security::CAP_GRAPHICS)
            {
                ERROR_INVALID
            } else {
                let command = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                crate::aarch64_desktop::execute_command(command, terminal);
                0
            }
        }
        SYS_SURFACE_CREATE => {
            if crate::security::has_capability(crate::security::CAP_GRAPHICS) {
                let surface = crate::graphics::create_reserved(
                    frame.registers[0] as u32,
                    frame.registers[1] as u32,
                    frame.registers[2],
                );
                if crate::aarch64_process::current_app_role()
                    == crate::aarch64_process::ProcessRole::Firefox
                {
                    crate::serial_println!(
                        "MAKOS_FIREFOX_SURFACE_OK handle={} width={} height={} slot={}",
                        surface,
                        frame.registers[0],
                        frame.registers[1],
                        frame.registers[2],
                    );
                }
                surface
            } else {
                ERROR_INVALID
            }
        }
        SYS_SURFACE_FILL => {
            let rectangle = frame.registers[2];
            if !crate::security::has_capability(crate::security::CAP_GRAPHICS)
                || !user_range_readable(rectangle, 16)
            {
                ERROR_INVALID
            } else {
                let values = unsafe { core::slice::from_raw_parts(rectangle as *const u32, 4) };
                u64::from(crate::graphics::fill_rect(
                    frame.registers[0],
                    values[0],
                    values[1],
                    values[2],
                    values[3],
                    frame.registers[1] as u32,
                ))
            }
        }
        SYS_SURFACE_PRESENT => {
            if crate::security::has_capability(crate::security::CAP_GRAPHICS) {
                u64::from(crate::graphics::present(frame.registers[0]))
            } else {
                ERROR_INVALID
            }
        }
        SYS_OPEN => {
            let address = frame.registers[0];
            let length = frame.registers[1] as usize;
            let access = frame.registers[2];
            // x3=0 preserves legacy write-open truncation; new libc callers
            // use 1=truncate or 2=preserve explicitly.
            let truncate = match frame.registers[3] {
                0 => access == 1,
                1 => true,
                2 => false,
                _ => false,
            };
            if length == 0
                || length >= crate::vfs::MAX_PATH_BYTES
                || !user_range_readable(address, length)
                || access > 2
                || frame.registers[3] > 2
                || (truncate && access == 0)
                || (access != 0
                    && !crate::security::has_capability(crate::security::CAP_FILE_WRITE))
            {
                ERROR_INVALID
            } else {
                let path = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                let result = crate::vfs::open_mode(path, access as u8, truncate);
                if path == b"/fonts/MakOSSystem-Regular.ttf" {
                    if let Some(fd) = result {
                        FIREFOX_FONT_FD.store(fd, Ordering::Relaxed);
                    }
                    crate::serial_println!("MAKOS_FONT_OPEN fd={:?}", result);
                }
                if crate::aarch64_process::current_app_role()
                    == crate::aarch64_process::ProcessRole::Firefox
                {
                    let trace = FIREFOX_OPEN_TRACES.fetch_add(1, Ordering::Relaxed);
                    if trace < FIREFOX_FILE_TRACE_LIMIT && path != b"/dev/urandom" {
                        crate::serial_println!(
                            "firefox-open trace={} path={} access={} truncate={} result={:?}",
                            trace,
                            core::str::from_utf8(path).unwrap_or("<invalid>"),
                            access,
                            truncate,
                            result,
                        );
                    }
                }
                result.unwrap_or(ERROR_INVALID)
            }
        }
        SYS_READ => {
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            let writable = crate::aarch64_vm::fault_in_range(
                crate::aarch64_process::current_pid(),
                address,
                length,
                true,
            ) && user_range_writable(address, length);
            if !writable {
                ERROR_INVALID
            } else {
                let output = unsafe { core::slice::from_raw_parts_mut(address as *mut u8, length) };
                if frame.registers[0] == 0 {
                    crate::aarch64_tty::read(0, output)
                        .map_or_else(|error| error.abi(), |count| count as u64)
                } else {
                    let result = crate::vfs::read_result(frame.registers[0], output);
                    if frame.registers[0] == FIREFOX_FONT_FD.load(Ordering::Relaxed)
                        && FIREFOX_FONT_IO_TRACES.fetch_add(1, Ordering::Relaxed)
                            < FIREFOX_FILE_TRACE_LIMIT
                    {
                        let count = result.as_ref().copied().unwrap_or(0).min(output.len());
                        crate::serial_println!(
                            "MAKOS_FONT_READ fd={} requested={} result={:?} bytes={:02x?}",
                            frame.registers[0],
                            length,
                            result,
                            &output[..count.min(8)],
                        );
                    }
                    result.map_or_else(crate::vfs::DescriptorError::abi, |count| count as u64)
                }
            }
        }
        SYS_CLOSE => {
            if frame.registers[0] < 3 {
                crate::aarch64_tty::close(frame.registers[0])
                    .map_or_else(|error| error.abi(), |_| 1)
            } else {
                u64::from(crate::vfs::close(frame.registers[0]))
            }
        }
        SYS_PROCESS_SPAWN => match frame.registers[0] {
            selector @ (0 | 1 | 3 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17)
                if crate::aarch64_process::process_control_allowed() =>
            {
                match selector {
                    0 => crate::aarch64_process::spawn_worker(),
                    1 => crate::aarch64_process::spawn_browser(),
                    3 => crate::aarch64_process::spawn_files(),
                    5 => crate::aarch64_process::spawn_startup_probe(),
                    6 => crate::aarch64_process::spawn_musl_probe(),
                    7 => crate::aarch64_process::spawn_musl_crt_probe(),
                    8 => crate::aarch64_process::spawn_musl_pthread_probe(),
                    9 => crate::aarch64_process::spawn_musl_interp_probe(),
                    10 => crate::aarch64_process::spawn_musl_dynamic_probe(),
                    11 => crate::aarch64_process::spawn_musl_dso_probe(),
                    12 => crate::aarch64_process::spawn_musl_dlopen_probe(),
                    13 => crate::aarch64_process::spawn_musl_exec_probe(),
                    14 => crate::aarch64_process::spawn_firefox(),
                    15 => crate::aarch64_process::spawn_stack_protector_probe(),
                    16 => crate::aarch64_process::spawn_toolchain(),
                    17 => crate::aarch64_process::spawn_firefox_smp_probe(),
                    _ => None,
                }
                .unwrap_or(ERROR_INVALID)
            }
            2 if matches!(
                crate::aarch64_process::current_app_role(),
                crate::aarch64_process::ProcessRole::Shell
                    | crate::aarch64_process::ProcessRole::Files
            ) =>
            {
                let address = frame.registers[1];
                let length = frame.registers[2] as usize;
                if length == 0 || length > 43 || !user_range_readable(address, length) {
                    ERROR_INVALID
                } else {
                    let path = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                    crate::aarch64_process::spawn_text_editor(path).unwrap_or(ERROR_INVALID)
                }
            }
            4 if crate::aarch64_process::current_app_role()
                == crate::aarch64_process::ProcessRole::Shell =>
            {
                let address = frame.registers[1];
                let length = frame.registers[2] as usize;
                if length == 0 || length > 43 || !user_range_readable(address, length) {
                    ERROR_INVALID
                } else {
                    let path = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                    crate::aarch64_process::spawn_python(path).unwrap_or(ERROR_INVALID)
                }
            }
            15 if crate::aarch64_process::current_app_role()
                == crate::aarch64_process::ProcessRole::Shell =>
            {
                let address = frame.registers[1];
                let length = frame.registers[2] as usize;
                if length == 0 || length > 43 || !user_range_readable(address, length) {
                    ERROR_INVALID
                } else {
                    let path = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                    crate::aarch64_process::spawn_nano(path).unwrap_or(ERROR_INVALID)
                }
            }
            _ => ERROR_INVALID,
        },
        SYS_PROCESS_WAIT => {
            if crate::aarch64_process::process_control_allowed()
                || crate::aarch64_process::current_app_role()
                    == crate::aarch64_process::ProcessRole::Files
            {
                crate::aarch64_process::wait(frame.registers[0]).unwrap_or(ERROR_INVALID)
            } else {
                ERROR_INVALID
            }
        }
        SYS_PROCESS_WAIT_STATUS => {
            match crate::aarch64_process::wait_status(
                frame.registers[0],
                frame.registers[1] != 0,
            ) {
                crate::aarch64_process::ChildWaitStatus::NoChild => (-10i64) as u64,
                crate::aarch64_process::ChildWaitStatus::Pending => 0,
                crate::aarch64_process::ChildWaitStatus::Exited(status) => {
                    status.saturating_add(1)
                }
            }
        }
        SYS_FILE_WRITE => {
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            if !user_range_readable(address, length) {
                ERROR_INVALID
            } else {
                let input = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                if matches!(frame.registers[0], 1 | 2) {
                    crate::aarch64_tty::write(frame.registers[0], input)
                        .map_or_else(|error| error.abi(), |count| count as u64)
                } else if (crate::vfs::is_pipe_owned(frame.registers[0])
                    && crate::security::has_capability(crate::security::CAP_IPC))
                    || crate::security::has_capability(crate::security::CAP_FILE_WRITE)
                {
                    crate::vfs::write_result(frame.registers[0], input)
                        .map_or_else(crate::vfs::DescriptorError::abi, |count| count as u64)
                } else {
                    ERROR_INVALID
                }
            }
        }
        SYS_PACKAGE_INSTALL => {
            const SIGNATURE_BYTES: usize = 256;
            let name_address = frame.registers[0];
            let name_length = frame.registers[1] as usize;
            let fields_address = frame.registers[2];
            let packed = frame.registers[3];
            let version_length = (packed & 0xff) as usize;
            let content_length = ((packed >> 8) & 0xff) as usize;
            let dependency_length = ((packed >> 16) & 0xff) as usize;
            let algorithm = ((packed >> 24) & 0xff) as u8;
            let total = version_length
                .checked_add(content_length)
                .and_then(|value| value.checked_add(dependency_length))
                .and_then(|value| value.checked_add(SIGNATURE_BYTES));
            if !crate::security::has_capability(crate::security::CAP_FILE_WRITE)
                || name_length == 0
                || name_length > 32
                || version_length == 0
                || content_length == 0
                || dependency_length == 0
                || algorithm != 1
                || !user_range_readable(name_address, name_length)
                || !total.is_some_and(|length| user_range_readable(fields_address, length))
            {
                ERROR_INVALID
            } else {
                let total = total.unwrap_or(0);
                let name =
                    unsafe { core::slice::from_raw_parts(name_address as *const u8, name_length) };
                let fields =
                    unsafe { core::slice::from_raw_parts(fields_address as *const u8, total) };
                let content_start = version_length;
                let dependency_start = content_start + content_length;
                let signature_start = dependency_start + dependency_length;
                let mut signature = [0u8; SIGNATURE_BYTES];
                signature.copy_from_slice(&fields[signature_start..]);
                u64::from(crate::package::install(
                    name,
                    &fields[..version_length],
                    &fields[content_start..dependency_start],
                    &fields[dependency_start..signature_start],
                    &signature,
                ))
            }
        }
        SYS_PACKAGE_QUERY => {
            let name_address = frame.registers[0];
            let name_length = frame.registers[1] as usize;
            let output_address = frame.registers[2];
            let capacity = frame.registers[3] as usize;
            if name_length == 0
                || name_length > 32
                || !user_range_readable(name_address, name_length)
                || !user_range_writable(output_address, capacity)
            {
                ERROR_INVALID
            } else {
                let name =
                    unsafe { core::slice::from_raw_parts(name_address as *const u8, name_length) };
                let output =
                    unsafe { core::slice::from_raw_parts_mut(output_address as *mut u8, capacity) };
                crate::package::query(name, output).map_or(ERROR_INVALID, |count| count as u64)
            }
        }
        SYS_PACKAGE_ROLLBACK => u64::from(
            crate::security::has_capability(crate::security::CAP_FILE_WRITE)
                && crate::package::rollback(),
        ),
        SYS_PACKAGE_REMOVE => {
            let name_address = frame.registers[0];
            let name_length = frame.registers[1] as usize;
            if !crate::security::has_capability(crate::security::CAP_FILE_WRITE)
                || name_length == 0
                || name_length > 32
                || !user_range_readable(name_address, name_length)
            {
                ERROR_INVALID
            } else {
                let name =
                    unsafe { core::slice::from_raw_parts(name_address as *const u8, name_length) };
                u64::from(crate::package::remove(name))
            }
        }
        SYS_VM_MAP => crate::aarch64_vm::map(
            crate::aarch64_process::current_pid(),
            crate::aarch64_vm::PAGE_SIZE,
            crate::aarch64_vm::PROT_READ | crate::aarch64_vm::PROT_WRITE,
        )
        .unwrap_or(ERROR_INVALID),
        SYS_VM_UNMAP => u64::from(crate::aarch64_vm::unmap(
            crate::aarch64_process::current_pid(),
            frame.registers[0],
            crate::aarch64_vm::PAGE_SIZE,
        )),
        SYS_CLOCK_MONOTONIC => monotonic_ticks(),
        SYS_LOG_APPEND => {
            let severity = frame.registers[0] as u8;
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            if !user_range_readable(address, length) {
                ERROR_INVALID
            } else {
                let message = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                crate::log::append(severity, message).unwrap_or(ERROR_INVALID)
            }
        }
        SYS_LOG_READ => {
            let sequence = frame.registers[0];
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            let metadata_address = frame.registers[3];
            if !crate::security::has_capability(crate::security::CAP_CONSOLE)
                || !user_range_writable(address, length)
                || !user_range_writable(metadata_address, 3 * core::mem::size_of::<u64>())
            {
                ERROR_INVALID
            } else {
                let output = unsafe { core::slice::from_raw_parts_mut(address as *mut u8, length) };
                if let Some((count, ticks, pid, severity)) = crate::log::read(sequence, output) {
                    unsafe {
                        (metadata_address as *mut u64).write_unaligned(ticks);
                        (metadata_address as *mut u64).add(1).write_unaligned(pid);
                        (metadata_address as *mut u64)
                            .add(2)
                            .write_unaligned(u64::from(severity));
                    }
                    count as u64
                } else {
                    ERROR_INVALID
                }
            }
        }
        SYS_AUTH_LOGIN => {
            let username_address = frame.registers[0];
            let username_length = frame.registers[1] as usize;
            let password_address = frame.registers[2];
            let password_length = frame.registers[3] as usize;
            if username_length > 32
                || password_length > 64
                || !user_range_readable(username_address, username_length)
                || !user_range_readable(password_address, password_length)
            {
                ERROR_INVALID
            } else {
                let username = unsafe {
                    core::slice::from_raw_parts(username_address as *const u8, username_length)
                };
                let password = unsafe {
                    core::slice::from_raw_parts(password_address as *const u8, password_length)
                };
                u64::from(crate::security::authenticate(username, password))
            }
        }
        SYS_SESSION_STATUS => u64::from(
            crate::aarch64_process::current_pid() == 1 && crate::security::session_active(),
        ),
        SYS_EVENT_CREATE => {
            if crate::aarch64_process::ipc_control_allowed() {
                crate::ipc::create_event(frame.registers[0] != 0).unwrap_or(ERROR_INVALID)
            } else {
                ERROR_INVALID
            }
        }
        SYS_EVENT_SIGNAL => u64::from(
            crate::aarch64_process::ipc_control_allowed()
                && crate::ipc::signal_event(frame.registers[0]),
        ),
        SYS_HANDLE_CLOSE => u64::from(
            crate::aarch64_process::ipc_control_allowed()
                && crate::ipc::close(frame.registers[0]),
        ),
        SYS_FD_DUP => crate::vfs::duplicate(frame.registers[0]).unwrap_or(ERROR_INVALID),
        SYS_FD_SEEK => {
            let result = crate::vfs::seek(
                frame.registers[0],
                frame.registers[1] as i64,
                frame.registers[2],
            );
            if frame.registers[0] == FIREFOX_FONT_FD.load(Ordering::Relaxed)
                && FIREFOX_FONT_IO_TRACES.fetch_add(1, Ordering::Relaxed) < FIREFOX_FILE_TRACE_LIMIT
            {
                crate::serial_println!(
                    "MAKOS_FONT_SEEK fd={} offset={} whence={} result={:?}",
                    frame.registers[0],
                    frame.registers[1] as i64,
                    frame.registers[2],
                    result,
                );
            }
            result.unwrap_or(ERROR_INVALID)
        }
        SYS_FD_DUP3 => {
            if frame.registers[2] > 1 {
                (-22i64) as u64
            } else {
                crate::vfs::duplicate_to(
                    frame.registers[0],
                    frame.registers[1],
                    frame.registers[2] != 0,
                )
                .unwrap_or_else(crate::vfs::DescriptorError::abi)
            }
        }
        SYS_FD_CONTROL => {
            let fd = frame.registers[0];
            let operation = frame.registers[1];
            let argument = frame.registers[2];
            if crate::aarch64_socket::is_owned(fd) {
                crate::aarch64_socket::control(fd, operation, argument)
                    .unwrap_or_else(|error| error as u64)
            } else {
                let result = match operation {
                    0 => crate::vfs::duplicate_min(fd, argument, false),
                    1 => crate::vfs::descriptor_flags(fd),
                    2 => crate::vfs::set_descriptor_flags(fd, argument),
                    3 => crate::vfs::status_flags(fd),
                    4 => crate::vfs::set_status_flags(fd, argument),
                    5 => crate::vfs::duplicate_min(fd, argument, true),
                    _ => Err(crate::vfs::DescriptorError::Invalid),
                };
                match result {
                    Ok(value) => value,
                    Err(error) => error.abi(),
                }
            }
        }
        SYS_PIPE2 => {
            const O_NONBLOCK: u64 = 0x800;
            const O_CLOEXEC: u64 = 0x80000;
            let output = frame.registers[0];
            let flags = frame.registers[1];
            if !crate::security::has_capability(crate::security::CAP_IPC)
                || !user_range_writable(output, 8)
                || flags & !(O_NONBLOCK | O_CLOEXEC) != 0
            {
                (-22i64) as u64
            } else if flags & O_NONBLOCK == 0 {
                (-95i64) as u64
            } else {
                match crate::vfs::pipe_pair(flags & O_CLOEXEC != 0, true) {
                    Ok((read_fd, write_fd)) => {
                        unsafe {
                            core::ptr::write_volatile(output as *mut i32, read_fd as i32);
                            core::ptr::write_volatile((output + 4) as *mut i32, write_fd as i32);
                        }
                        0
                    }
                    Err(error) => error.abi(),
                }
            }
        }
        SYS_POLL => {
            let address = frame.registers[0];
            let count = frame.registers[1] as usize;
            let timeout = frame.registers[2] as i64;
            let bytes = count.saturating_mul(8);
            if count > 32
                || !user_range_readable(address, bytes)
                || !user_range_writable(address, bytes)
            {
                (-22i64) as u64
            } else if timeout != 0 {
                (-95i64) as u64
            } else {
                let mut ready = 0u64;
                for index in 0..count {
                    let entry = address + index as u64 * 8;
                    let fd = unsafe { core::ptr::read_volatile(entry as *const i32) };
                    let requested = unsafe { core::ptr::read_volatile((entry + 4) as *const u16) };
                    let returned = if fd < 0 {
                        0
                    } else if matches!(fd, 1 | 2) {
                        requested & 0x004
                    } else if fd == 0 {
                        0
                    } else {
                        crate::vfs::poll_events(fd as u64, requested)
                    };
                    unsafe {
                        core::ptr::write_volatile((entry + 6) as *mut u16, returned);
                    }
                    ready += u64::from(returned != 0);
                }
                ready
            }
        }
        SYS_FSTAT_METADATA => {
            let fd = frame.registers[0];
            let output = frame.registers[1];
            if !user_range_writable(output, core::mem::size_of::<crate::vfs::Metadata>()) {
                (-22i64) as u64
            } else {
                let metadata = if fd <= 2 {
                    let credentials = crate::security::credentials();
                    Ok(crate::vfs::Metadata {
                        mode: 0o020620,
                        uid: credentials.uid,
                        gid: credentials.gid,
                        kind: 4,
                        size: 0,
                        modified_ticks: 0,
                        inode: 0x2000 + fd,
                    })
                } else {
                    crate::vfs::metadata_for_fd(fd)
                };
                match metadata {
                    Ok(metadata) => {
                        unsafe { (output as *mut crate::vfs::Metadata).write_unaligned(metadata) };
                        0
                    }
                    Err(error) => error.abi(),
                }
            }
        }
        SYS_READ_DIR_FD => {
            let fd = frame.registers[0];
            let output = frame.registers[1];
            if !user_range_writable(output, core::mem::size_of::<crate::vfs::DirectoryEntry>()) {
                (-22i64) as u64
            } else {
                let result = crate::vfs::read_directory_fd(fd);
                if crate::aarch64_process::current_app_role()
                    == crate::aarch64_process::ProcessRole::Firefox
                {
                    let trace = FIREFOX_READDIR_TRACES.fetch_add(1, Ordering::Relaxed);
                    if trace < FIREFOX_FILE_TRACE_LIMIT {
                        crate::serial_println!(
                            "firefox-readdir trace={} fd={} result={:?}",
                            trace,
                            fd,
                            result
                                .as_ref()
                                .map(|entry| entry.as_ref().map(|(_, offset)| *offset))
                        );
                    }
                }
                match result {
                    Ok(Some((entry, next_offset))) => {
                        unsafe {
                            (output as *mut crate::vfs::DirectoryEntry).write_unaligned(entry)
                        };
                        next_offset
                    }
                    Ok(None) => 0,
                    Err(error) => error.abi(),
                }
            }
        }
        SYS_CHDIR => {
            let address = frame.registers[0];
            let length = frame.registers[1] as usize;
            if length == 0 || length >= 256 || !user_range_readable(address, length) {
                (-22i64) as u64
            } else {
                let path = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                match crate::vfs::change_working_directory(path) {
                    Ok(()) => 0,
                    Err(crate::vfs::DescriptorError::Invalid) => (-22i64) as u64,
                    Err(_) => (-2i64) as u64,
                }
            }
        }
        SYS_GETCWD => {
            let output = frame.registers[0];
            let capacity = frame.registers[1] as usize;
            if capacity == 0 || capacity > 4096 || !user_range_writable(output, capacity) {
                (-22i64) as u64
            } else {
                let bytes = unsafe { core::slice::from_raw_parts_mut(output as *mut u8, capacity) };
                match crate::vfs::working_directory(&mut bytes[..capacity - 1]) {
                    Ok(length) => {
                        bytes[length] = 0;
                        (length + 1) as u64
                    }
                    Err(crate::vfs::DescriptorError::Invalid) => (-34i64) as u64,
                    Err(error) => error.abi(),
                }
            }
        }
        SYS_FTRUNCATE => crate::vfs::truncate(frame.registers[0], frame.registers[1])
            .map(|()| 0)
            .unwrap_or_else(crate::vfs::DescriptorError::abi),
        SYS_FSYNC => crate::vfs::sync(frame.registers[0])
            .map(|()| 0)
            .unwrap_or_else(crate::vfs::DescriptorError::abi),
        SYS_MKDIR | SYS_RMDIR => {
            let address = frame.registers[0];
            let length = frame.registers[1] as usize;
            if !crate::security::has_capability(crate::security::CAP_FILE_WRITE)
                || length == 0
                || length >= crate::vfs::MAX_PATH_BYTES
                || !user_range_readable(address, length)
            {
                (-22i64) as u64
            } else {
                let path = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                let result = if frame.registers[8] == SYS_MKDIR {
                    crate::vfs::create_directory(path)
                } else {
                    crate::vfs::remove_directory(path)
                };
                if crate::aarch64_process::current_app_role()
                    == crate::aarch64_process::ProcessRole::Firefox
                    && FIREFOX_MUTATION_TRACES.fetch_add(1, Ordering::Relaxed)
                        < FIREFOX_MUTATION_TRACE_LIMIT
                {
                    crate::serial_println!(
                        "firefox-directory operation={} path={} result={:?}",
                        if frame.registers[8] == SYS_MKDIR {
                            "mkdir"
                        } else {
                            "rmdir"
                        },
                        core::str::from_utf8(path).unwrap_or("<invalid>"),
                        result,
                    );
                }
                result
                    .map(|()| 0)
                    .unwrap_or_else(crate::vfs::DescriptorError::abi)
            }
        }
        SYS_PREAD => {
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            if !user_range_writable(address, length) {
                (-22i64) as u64
            } else {
                let output = unsafe { core::slice::from_raw_parts_mut(address as *mut u8, length) };
                crate::vfs::read_at(frame.registers[0], output, frame.registers[3])
                    .map_or_else(crate::vfs::DescriptorError::abi, |count| count as u64)
            }
        }
        SYS_PWRITE => {
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            if !crate::security::has_capability(crate::security::CAP_FILE_WRITE)
                || !user_range_readable(address, length)
            {
                (-22i64) as u64
            } else {
                let input = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                crate::vfs::write_at(frame.registers[0], input, frame.registers[3])
                    .map_or_else(crate::vfs::DescriptorError::abi, |count| count as u64)
            }
        }
        SYS_CLOCK_RESOLUTION => match frame.registers[0] {
            0 => 1_000_000_000,
            1 => 10_000_000,
            _ => (-22i64) as u64,
        },
        SYS_EPOLL_CREATE => {
            const EPOLL_CLOEXEC: u64 = 0x80000;
            if frame.registers[0] & !EPOLL_CLOEXEC != 0 {
                (-22i64) as u64
            } else {
                crate::aarch64_epoll::create(frame.registers[0] & EPOLL_CLOEXEC != 0)
                    .unwrap_or_else(crate::aarch64_epoll::error_abi)
            }
        }
        SYS_EPOLL_CTL => {
            let operation = frame.registers[1];
            let address = frame.registers[3];
            if operation != 2
                && !user_range_readable(address, core::mem::size_of::<makos_readiness::Event>())
            {
                (-22i64) as u64
            } else {
                let event = (operation != 2).then(|| unsafe {
                    (address as *const makos_readiness::Event).read_unaligned()
                });
                crate::aarch64_epoll::control(
                    frame.registers[0],
                    operation,
                    frame.registers[2] as i32,
                    event,
                )
                .map_or_else(crate::aarch64_epoll::error_abi, |_| 0)
            }
        }
        SYS_EPOLL_CLOSE => u64::from(crate::aarch64_epoll::close(frame.registers[0])),
        SYS_SIGPROCMASK => {
            let how = frame.registers[0];
            let replacement = frame.registers[1];
            let replacement_present = frame.registers[2];
            let old_address = frame.registers[3];
            if replacement_present > 1 || (old_address != 0 && !user_range_writable(old_address, 8))
            {
                (-22i64) as u64
            } else {
                match crate::aarch64_tty::signal_mask(
                    how,
                    (replacement_present != 0).then_some(replacement),
                ) {
                    Ok(previous) => {
                        if old_address != 0 {
                            unsafe { (old_address as *mut u64).write_unaligned(previous) };
                        }
                        0
                    }
                    Err(error) => error.abi(),
                }
            }
        }
        SYS_TYPED_SERVICE_PUBLISH => {
            typed_service_publish(frame.registers[0], frame.registers[1] as usize)
        }
        SYS_TYPED_SERVICE_CONNECT => {
            typed_service_connect(frame.registers[0], frame.registers[1] as usize)
        }
        SYS_TYPED_SERVICE_ACCEPT => typed_service_accept(frame.registers[0]),
        SYS_TYPED_CHANNEL_SEND => typed_channel_send(
            frame.registers[0],
            frame.registers[1],
            frame.registers[2],
            frame.registers[3] as u8,
        ),
        SYS_TYPED_CHANNEL_RECEIVE => typed_channel_receive(
            frame.registers[0],
            frame.registers[1],
            frame.registers[2],
        ),
        SYS_ABI_INFO => match frame.registers[0] {
            0 => 0x0001_0000,
            // 0..57 remain normative cross-architecture ABI. Target-specific
            // extensions must never move this stable maximum.
            1 => 57,
            2 => ABI_FEATURES,
            // Highest AArch64 extension implemented by this kernel.
            3 => SYS_TYPED_CHANNEL_RECEIVE,
            _ => ERROR_INVALID,
        },
        SYS_STAT => {
            let path_address = frame.registers[0];
            let path_length = frame.registers[1] as usize;
            let output = frame.registers[2];
            if path_length == 0
                || path_length >= crate::vfs::MAX_PATH_BYTES
                || !user_range_readable(path_address, path_length)
                || !user_range_writable(output, core::mem::size_of::<crate::vfs::Metadata>())
            {
                ERROR_INVALID
            } else {
                let path =
                    unsafe { core::slice::from_raw_parts(path_address as *const u8, path_length) };
                if let Some(metadata) = crate::vfs::stat(path) {
                    unsafe { (output as *mut crate::vfs::Metadata).write_unaligned(metadata) };
                    1
                } else {
                    ERROR_INVALID
                }
            }
        }
        SYS_SYMLINK => {
            let target_address = frame.registers[0];
            let target_length = frame.registers[1] as usize;
            let link_address = frame.registers[2];
            let link_length = frame.registers[3] as usize;
            if !crate::security::has_capability(crate::security::CAP_FILE_WRITE)
                || target_length == 0
                || target_length >= crate::vfs::MAX_PATH_BYTES
                || link_length == 0
                || link_length >= crate::vfs::MAX_PATH_BYTES
                || !user_range_readable(target_address, target_length)
                || !user_range_readable(link_address, link_length)
            {
                (-22i64) as u64
            } else {
                let target = unsafe {
                    core::slice::from_raw_parts(target_address as *const u8, target_length)
                };
                let link =
                    unsafe { core::slice::from_raw_parts(link_address as *const u8, link_length) };
                crate::vfs::create_symlink(target, link)
                    .map_or_else(crate::vfs::DescriptorError::abi, |_| 0)
            }
        }
        SYS_READLINK => {
            let path_address = frame.registers[0];
            let path_length = frame.registers[1] as usize;
            let output_address = frame.registers[2];
            let capacity = frame.registers[3] as usize;
            if path_length == 0
                || path_length >= crate::vfs::MAX_PATH_BYTES
                || capacity == 0
                || capacity >= crate::vfs::MAX_PATH_BYTES
                || !user_range_readable(path_address, path_length)
                || !user_range_writable(output_address, capacity)
            {
                (-22i64) as u64
            } else {
                let path =
                    unsafe { core::slice::from_raw_parts(path_address as *const u8, path_length) };
                let output =
                    unsafe { core::slice::from_raw_parts_mut(output_address as *mut u8, capacity) };
                crate::vfs::read_link(path, output)
                    .map_or_else(crate::vfs::DescriptorError::abi, |count| count as u64)
            }
        }
        SYS_STAT_EXTENDED => {
            let path_address = frame.registers[0];
            let path_length = frame.registers[1] as usize;
            let output = frame.registers[2];
            let flags = frame.registers[3];
            if path_length == 0
                || path_length >= crate::vfs::MAX_PATH_BYTES
                || flags & !1 != 0
                || !user_range_readable(path_address, path_length)
                || !user_range_writable(
                    output,
                    core::mem::size_of::<crate::vfs::ExtendedMetadata>(),
                )
            {
                (-22i64) as u64
            } else {
                let path =
                    unsafe { core::slice::from_raw_parts(path_address as *const u8, path_length) };
                if let Some(metadata) = crate::vfs::stat_extended(path, flags & 1 == 0) {
                    unsafe {
                        (output as *mut crate::vfs::ExtendedMetadata).write_unaligned(metadata)
                    };
                    0
                } else {
                    (-2i64) as u64
                }
            }
        }
        SYS_FSTAT_EXTENDED => {
            let fd = frame.registers[0];
            let output = frame.registers[1];
            if !user_range_writable(output, core::mem::size_of::<crate::vfs::ExtendedMetadata>()) {
                (-22i64) as u64
            } else {
                let metadata = if fd <= 2 {
                    let credentials = crate::security::credentials();
                    Ok(crate::vfs::ExtendedMetadata {
                        mode: 0o020620,
                        uid: credentials.uid,
                        gid: credentials.gid,
                        kind: 4,
                        size: 0,
                        accessed_ns: 0,
                        modified_ns: 0,
                        changed_ns: 0,
                        inode: 0x2000 + fd,
                    })
                } else {
                    crate::vfs::metadata_extended_for_fd(fd)
                };
                match metadata {
                    Ok(metadata) => {
                        unsafe {
                            (output as *mut crate::vfs::ExtendedMetadata).write_unaligned(metadata)
                        };
                        0
                    }
                    Err(error) => error.abi(),
                }
            }
        }
        SYS_READ_DIR => {
            let path_address = frame.registers[0];
            let path_length = frame.registers[1] as usize;
            let output = frame.registers[3];
            if path_length == 0
                || path_length >= crate::vfs::MAX_PATH_BYTES
                || !user_range_readable(path_address, path_length)
                || !user_range_writable(output, core::mem::size_of::<crate::vfs::DirectoryEntry>())
            {
                ERROR_INVALID
            } else {
                let path =
                    unsafe { core::slice::from_raw_parts(path_address as *const u8, path_length) };
                if let Some(entry) = crate::vfs::read_dir(path, frame.registers[2] as usize) {
                    unsafe { (output as *mut crate::vfs::DirectoryEntry).write_unaligned(entry) };
                    1
                } else {
                    0
                }
            }
        }
        SYS_CREATE => {
            let address = frame.registers[0];
            let length = frame.registers[1] as usize;
            if !crate::security::has_capability(crate::security::CAP_FILE_WRITE)
                || length == 0
                || length >= crate::vfs::MAX_PATH_BYTES
                || !user_range_readable(address, length)
            {
                ERROR_INVALID
            } else {
                let path = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                let result = crate::vfs::create(path);
                if crate::aarch64_process::current_app_role()
                    == crate::aarch64_process::ProcessRole::Firefox
                    && FIREFOX_MUTATION_TRACES.fetch_add(1, Ordering::Relaxed)
                        < FIREFOX_MUTATION_TRACE_LIMIT
                {
                    crate::serial_println!(
                        "firefox-create path={} result={}",
                        core::str::from_utf8(path).unwrap_or("<invalid>"),
                        u8::from(result),
                    );
                }
                u64::from(result)
            }
        }
        SYS_UNLINK => {
            let address = frame.registers[0];
            let length = frame.registers[1] as usize;
            if !crate::security::has_capability(crate::security::CAP_FILE_WRITE)
                || length == 0
                || length >= crate::vfs::MAX_PATH_BYTES
                || !user_range_readable(address, length)
            {
                ERROR_INVALID
            } else {
                let path = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                u64::from(crate::vfs::unlink(path))
            }
        }
        SYS_VM_PROTECT => u64::from(crate::aarch64_vm::protect(
            crate::aarch64_process::current_pid(),
            frame.registers[0],
            crate::aarch64_vm::PAGE_SIZE,
            frame.registers[1],
        )),
        SYS_SOCKET_CREATE => {
            if crate::aarch64_process::network_control_allowed() {
                crate::aarch64_socket::create(
                    frame.registers[0],
                    frame.registers[1],
                    frame.registers[2],
                )
                .unwrap_or(ERROR_INVALID)
            } else {
                ERROR_INVALID
            }
        }
        SYS_SOCKET_CONNECT => {
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            if !crate::aarch64_process::network_control_allowed()
                || !user_range_readable(address, length)
            {
                ERROR_INVALID
            } else {
                read_socket_endpoint(address, length).map_or(ERROR_INVALID, |endpoint| {
                    u64::from(crate::aarch64_socket::connect_address(
                        frame.registers[0],
                        endpoint.address,
                        endpoint.port,
                    ))
                })
            }
        }
        SYS_SOCKET_SEND => {
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            let flags = frame.registers[3];
            let destination_address = frame.registers[4];
            let destination_length = frame.registers[5] as usize;
            if !crate::aarch64_process::network_control_allowed()
                || flags & !(0x40 | 0x4000) != 0
                || length == 0
                || length > 4096
                || !user_range_readable(address, length)
                || ((destination_address == 0) != (destination_length == 0))
                || (destination_address != 0
                    && !user_range_readable(destination_address, destination_length))
            {
                ERROR_INVALID
            } else {
                let payload = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                let destination = if destination_address == 0 {
                    None
                } else {
                    read_socket_endpoint(destination_address, destination_length)
                };
                if destination_address != 0 && destination.is_none() {
                    ERROR_INVALID
                } else {
                    crate::aarch64_socket::send_to(frame.registers[0], payload, destination)
                        .map_or(ERROR_INVALID, |count| count as u64)
                }
            }
        }
        SYS_SOCKET_RECEIVE => {
            let address = frame.registers[1];
            let length = (frame.registers[2] as usize).min(4096);
            let flags = frame.registers[3];
            let source_address = frame.registers[4];
            let source_length_address = frame.registers[5];
            let pid = crate::aarch64_process::current_pid();
            let source_required = crate::aarch64_socket::domain(frame.registers[0])
                .and_then(socket_address_length)
                .unwrap_or(0);
            let data_writable = crate::aarch64_vm::fault_in_range(pid, address, length, true)
                && user_range_writable(address, length);
            let source_writable = source_address == 0
                || (source_required != 0
                    && crate::aarch64_vm::fault_in_range(
                        pid,
                        source_address,
                        source_required,
                        true,
                    )
                    && user_range_writable(source_address, source_required));
            let source_length_writable = source_length_address == 0
                || (crate::aarch64_vm::fault_in_range(pid, source_length_address, 4, true)
                    && user_range_readable(source_length_address, 4)
                    && user_range_writable(source_length_address, 4));
            let source_capacity_ok = source_address == 0
                || (source_length_address != 0
                    && source_length_writable
                    && unsafe { (source_length_address as *const u32).read_unaligned() as usize }
                        >= source_required);
            if !crate::aarch64_process::network_control_allowed()
                || flags & !(0x40 | 0x4000) != 0
                || length == 0
                || !data_writable
                || ((source_address == 0) != (source_length_address == 0))
                || !source_writable
                || !source_length_writable
                || !source_capacity_ok
            {
                ERROR_INVALID
            } else {
                let output = unsafe { core::slice::from_raw_parts_mut(address as *mut u8, length) };
                crate::aarch64_socket::receive_from(frame.registers[0], output).map_or_else(
                    || {
                        if crate::aarch64_socket::is_owned(frame.registers[0]) {
                            (-11i64) as u64
                        } else {
                            (-9i64) as u64
                        }
                    },
                    |(count, source)| {
                        if source_address != 0 {
                            if let Err(error) =
                                write_socket_endpoint(source, source_address, source_length_address)
                            {
                                return error as u64;
                            }
                        }
                        count as u64
                    },
                )
            }
        }
        SYS_SOCKET_NAME => {
            let handle = frame.registers[0];
            let peer = frame.registers[1] != 0;
            let address = frame.registers[2];
            let length_address = frame.registers[3];
            let pid = crate::aarch64_process::current_pid();
            if !crate::aarch64_process::network_control_allowed()
                || !crate::aarch64_vm::fault_in_range(pid, length_address, 4, true)
                || !user_range_readable(length_address, 4)
                || !user_range_writable(length_address, 4)
            {
                (-22i64) as u64
            } else {
                let endpoint = if peer {
                    crate::aarch64_socket::peer_endpoint(handle)
                } else {
                    crate::aarch64_socket::local_endpoint(handle)
                };
                endpoint
                    .and_then(|endpoint| write_socket_endpoint(endpoint, address, length_address))
                    .map_or_else(|error| error as u64, |()| 0)
            }
        }
        SYS_SOCKET_BIND => {
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            if !crate::aarch64_process::network_control_allowed() {
                (-1i64) as u64
            } else {
                read_socket_endpoint(address, length)
                    .ok_or(-22)
                    .and_then(|endpoint| {
                        crate::aarch64_socket::bind_endpoint(frame.registers[0], endpoint)
                    })
                    .map_or_else(|error| error as u64, |()| 0)
            }
        }
        SYS_SOCKET_CLOSE => {
            if crate::aarch64_process::network_control_allowed() {
                u64::from(crate::aarch64_socket::close(frame.registers[0]))
            } else {
                ERROR_INVALID
            }
        }
        SYS_VM_MAP_RANGE => {
            const MAP_SHARED: u64 = 1;
            const MAP_PRIVATE: u64 = 2;
            const MAP_FIXED: u64 = 0x10;
            const MAP_ANONYMOUS: u64 = 0x20;
            let flags = frame.registers[3];
            if flags == 0 {
                // Compatibility with pre-file-mmap MakOS musl adapters.
                crate::aarch64_vm::map(
                    crate::aarch64_process::current_pid(),
                    frame.registers[0],
                    frame.registers[1],
                )
                .unwrap_or(ERROR_INVALID)
            } else if flags & !(MAP_SHARED | MAP_PRIVATE | MAP_FIXED | MAP_ANONYMOUS) != 0
                || (flags & MAP_SHARED == 0) == (flags & MAP_PRIVATE == 0)
            {
                ERROR_INVALID
            } else if flags & MAP_ANONYMOUS != 0 {
                if flags & MAP_SHARED != 0 {
                    ERROR_INVALID
                } else if flags & MAP_FIXED != 0 {
                    crate::aarch64_vm::map_anonymous_fixed(
                        crate::aarch64_process::current_pid(),
                        frame.registers[0],
                        frame.registers[1],
                        frame.registers[2],
                    )
                    .unwrap_or(ERROR_INVALID)
                } else {
                    crate::aarch64_vm::map(
                        crate::aarch64_process::current_pid(),
                        frame.registers[1],
                        frame.registers[2],
                    )
                    .unwrap_or(ERROR_INVALID)
                }
            } else if flags & MAP_SHARED != 0 {
                crate::aarch64_vm::map_shared(
                    crate::aarch64_process::current_pid(),
                    frame.registers[0],
                    frame.registers[1],
                    frame.registers[2],
                    flags & MAP_FIXED != 0,
                    frame.registers[4],
                    frame.registers[5],
                )
                .unwrap_or(ERROR_INVALID)
            } else {
                crate::aarch64_vm::map_file(
                    crate::aarch64_process::current_pid(),
                    frame.registers[0],
                    frame.registers[1],
                    frame.registers[2],
                    flags & MAP_FIXED != 0,
                    frame.registers[4],
                    frame.registers[5],
                )
                .unwrap_or(ERROR_INVALID)
            }
        }
        SYS_VM_UNMAP_RANGE => u64::from(crate::aarch64_vm::unmap(
            crate::aarch64_process::current_pid(),
            frame.registers[0],
            frame.registers[1],
        )),
        SYS_VM_PROTECT_RANGE => u64::from(crate::aarch64_vm::protect(
            crate::aarch64_process::current_pid(),
            frame.registers[0],
            frame.registers[1],
            frame.registers[2],
        )),
        SYS_PROCESS_SPAWN_PATH => {
            let address = frame.registers[0];
            let length = frame.registers[1] as usize;
            if !crate::security::has_capability(crate::security::CAP_PROCESS)
                || length == 0
                || length >= crate::vfs::MAX_PATH_BYTES
                || !user_range_readable(address, length)
            {
                ERROR_INVALID
            } else {
                let path = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                crate::aarch64_process::spawn_path(path).unwrap_or(ERROR_INVALID)
            }
        }
        SYS_PROCESS_SPAWN_PATH_ARGS => {
            let path_address = frame.registers[0];
            let path_length = frame.registers[1] as usize;
            let arguments_address = frame.registers[2];
            let arguments_length = frame.registers[3] as usize;
            if !crate::security::has_capability(crate::security::CAP_PROCESS)
                || path_length == 0
                || path_length >= crate::vfs::MAX_PATH_BYTES
                || arguments_length != crate::aarch64_process::SPAWN_ARGUMENTS_BYTES
                || !user_range_readable(path_address, path_length)
                || !user_range_readable(arguments_address, arguments_length)
            {
                ERROR_INVALID
            } else {
                let path = unsafe {
                    core::slice::from_raw_parts(path_address as *const u8, path_length)
                };
                let arguments = unsafe {
                    core::slice::from_raw_parts(
                        arguments_address as *const u8,
                        arguments_length,
                    )
                };
                crate::aarch64_process::spawn_path_with_arguments(path, arguments)
                    .unwrap_or(ERROR_INVALID)
            }
        }
        SYS_SURFACE_CLOSE => {
            if crate::security::has_capability(crate::security::CAP_GRAPHICS) {
                u64::from(crate::graphics::close(frame.registers[0]))
            } else {
                ERROR_INVALID
            }
        }
        SYS_SURFACE_TEXT => {
            let packed = frame.registers[1];
            let address = frame.registers[2];
            let length = frame.registers[3] as usize;
            if !crate::security::has_capability(crate::security::CAP_GRAPHICS)
                || length > 1024
                || !user_range_readable(address, length)
            {
                ERROR_INVALID
            } else {
                let text = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                u64::from(crate::graphics::surface_text(
                    frame.registers[0],
                    (packed >> 32) as u32,
                    packed as u32,
                    text,
                ))
            }
        }
        SYS_SURFACE_READ_EVENT => {
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            if !crate::security::has_capability(crate::security::CAP_GRAPHICS)
                || length != core::mem::size_of::<crate::graphics::SurfaceEvent>()
                || !user_range_writable(address, length)
                || !crate::graphics::event_handle_valid(frame.registers[0])
            {
                ERROR_INVALID
            } else {
                service_input_on_owner_cpu();
                if let Some(event) = crate::graphics::read_event(frame.registers[0]) {
                    crate::aarch64_process::complete_input_wait();
                    unsafe {
                        core::ptr::write(address as *mut crate::graphics::SurfaceEvent, event)
                    };
                    length as u64
                } else if crate::aarch64_process::current_app_role()
                    == crate::aarch64_process::ProcessRole::Firefox
                {
                    // Gecko's native event pump performs a nonblocking input
                    // drain before processing internal runnables and timers.
                    // Blocking here deadlocks startup before widget Show.
                    0
                } else if crate::aarch64_process::runtime_stats().runnable > 1 {
                    match crate::aarch64_process::block_current_for_input(frame) {
                        crate::aarch64_process::IoBlockResult::Switched => return,
                        _ => 0,
                    }
                } else {
                    enable_interrupts();
                    unsafe { asm!("wfi", options(nomem, nostack)) };
                    disable_interrupts();
                    service_input_on_owner_cpu();
                    if let Some(event) = crate::graphics::read_event(frame.registers[0]) {
                        crate::aarch64_process::complete_input_wait();
                        unsafe {
                            core::ptr::write(address as *mut crate::graphics::SurfaceEvent, event)
                        };
                        length as u64
                    } else {
                        0
                    }
                }
            }
        }
        SYS_SURFACE_WAIT_EVENT => {
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            if !crate::security::has_capability(crate::security::CAP_GRAPHICS)
                || length != core::mem::size_of::<crate::graphics::SurfaceEvent>()
                || !user_range_writable(address, length)
                || !crate::graphics::event_handle_valid(frame.registers[0])
            {
                ERROR_INVALID
            } else {
                service_input_on_owner_cpu();
                if let Some(event) = crate::graphics::read_event(frame.registers[0]) {
                    crate::aarch64_process::complete_input_wait();
                    unsafe {
                        core::ptr::write(address as *mut crate::graphics::SurfaceEvent, event)
                    };
                    if event.kind == 1 {
                        crate::aarch64_process::arm_firefox_process_leader_handoff();
                    }
                    length as u64
                } else {
                    // Dedicated native-event watcher threads use this syscall.
                    // The saved ELR retries the same handle-specific dequeue
                    // after virtio input wakes the input wait class.
                    match crate::aarch64_process::block_current_for_input(frame) {
                        crate::aarch64_process::IoBlockResult::Switched => return,
                        _ => 0,
                    }
                }
            }
        }
        SYS_ROBUST_LIST => match frame.registers[0] {
            0 => {
                if crate::aarch64_process::set_robust_list(frame.registers[1], frame.registers[2]) {
                    0
                } else {
                    (-22i64) as u64
                }
            }
            1 => {
                let tid = frame.registers[1];
                let head_output = frame.registers[2];
                let length_output = frame.registers[3];
                if !user_range_writable(head_output, 8) || !user_range_writable(length_output, 8) {
                    (-22i64) as u64
                } else if let Some((head, length)) = crate::aarch64_process::get_robust_list(tid) {
                    unsafe {
                        (head_output as *mut u64).write_unaligned(head);
                        (length_output as *mut u64).write_unaligned(length);
                    }
                    0
                } else {
                    (-3i64) as u64
                }
            }
            _ => (-22i64) as u64,
        },
        SYS_SIGNAL => match frame.registers[0] {
            0 => crate::aarch64_tty::kill(frame.registers[1] as i64, frame.registers[2] as u32)
                .map_or_else(crate::aarch64_tty::Errno::abi, |_| 0),
            1 if frame.registers[1] != 0 => {
                crate::aarch64_tty::kill_task(0, frame.registers[1], frame.registers[2] as u32)
                    .map_or_else(crate::aarch64_tty::Errno::abi, |_| 0)
            }
            2 if frame.registers[1] != 0 && frame.registers[2] != 0 => {
                crate::aarch64_tty::kill_task(
                    frame.registers[1],
                    frame.registers[2],
                    frame.registers[3] as u32,
                )
                .map_or_else(crate::aarch64_tty::Errno::abi, |_| 0)
            }
            _ => (-22i64) as u64,
        },
        SYS_SURFACE_BLIT => {
            let address = frame.registers[1];
            let width = frame.registers[2] as u32;
            let height = frame.registers[3] as u32;
            let stride = frame.registers[4] as u32;
            let packed_destination = frame.registers[5];
            let length = height
                .checked_sub(1)
                .and_then(|rows| rows.checked_mul(stride))
                .and_then(|prefix| width.checked_mul(4).and_then(|row| prefix.checked_add(row)))
                .and_then(|bytes| usize::try_from(bytes).ok());
            let readable = length.is_some_and(|length| {
                crate::aarch64_vm::fault_in_range(
                    crate::aarch64_process::current_pid(),
                    address,
                    length,
                    false,
                ) && user_range_readable(address, length)
            });
            if !crate::security::has_capability(crate::security::CAP_GRAPHICS)
                || length.is_none()
                || !readable
            {
                ERROR_INVALID
            } else {
                let pixels = unsafe {
                    core::slice::from_raw_parts(address as *const u8, length.unwrap_or(0))
                };
                let result = crate::graphics::blit_argb(
                    frame.registers[0],
                    pixels,
                    width,
                    height,
                    stride,
                    (packed_destination >> 32) as u32,
                    packed_destination as u32,
                );
                if crate::aarch64_process::current_app_role()
                    == crate::aarch64_process::ProcessRole::Firefox
                    && FIREFOX_BLIT_TRACES.fetch_add(1, Ordering::Relaxed) < 8
                {
                    crate::serial_println!(
                        "MAKOS_FIREFOX_BLIT handle={} width={} height={} stride={} destination={},{} result={}",
                        frame.registers[0],
                        width,
                        height,
                        stride,
                        (packed_destination >> 32) as u32,
                        packed_destination as u32,
                        u8::from(result),
                    );
                }
                u64::from(result)
            }
        }
        SYS_MADVISE => {
            if crate::aarch64_vm::advise(
                crate::aarch64_process::current_pid(),
                frame.registers[0],
                frame.registers[1],
                frame.registers[2],
            ) {
                0
            } else {
                (-22i64) as u64
            }
        }
        SYS_THREAD_SET_NAME => {
            let address = frame.registers[0];
            let length = frame.registers[1] as usize;
            if length > 15 || !user_range_readable(address, length) {
                (-22i64) as u64
            } else {
                let name = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
                if crate::aarch64_process::set_current_thread_name(name) {
                    0
                } else {
                    (-22i64) as u64
                }
            }
        }
        SYS_THREAD_SET_SCHEDULER => {
            if crate::aarch64_process::set_task_scheduler(
                frame.registers[0],
                frame.registers[1],
                frame.registers[2],
            ) {
                0
            } else {
                (-22i64) as u64
            }
        }
        SYS_COMPAT_MISSING => {
            if crate::aarch64_process::current_app_role()
                == crate::aarch64_process::ProcessRole::Firefox
                && FIREFOX_MISSING_SYSCALL_TRACES.fetch_add(1, Ordering::Relaxed) < 128
            {
                crate::serial_println!(
                    "MAKOS_FIREFOX_MISSING_SYSCALL linux_aarch64={} arg0={:#x} arg1={:#x}",
                    frame.registers[0],
                    frame.registers[1],
                    frame.registers[2],
                );
            }
            0
        }
        SYS_NET_CONFIG => {
            let address = frame.registers[0];
            let length = frame.registers[1] as usize;
            if !crate::aarch64_process::network_control_allowed()
                || !matches!(length, 12 | 56)
                || !user_range_writable(address, length)
            {
                ERROR_INVALID
            } else if let Some(config) = crate::aarch64_virtio_net::config() {
                let output = unsafe { core::slice::from_raw_parts_mut(address as *mut u8, length) };
                output[..4].copy_from_slice(&config.ipv4);
                output[4..8].copy_from_slice(&config.gateway);
                output[8..12].copy_from_slice(&config.dns);
                if length == 56 {
                    output[12..16].copy_from_slice(&1u32.to_le_bytes());
                    let ipv6 = crate::aarch64_virtio_net::ipv6_config();
                    output[16..20].copy_from_slice(&u32::from(ipv6.is_some()).to_le_bytes());
                    output[20] = ipv6.map(|value| value.prefix_length).unwrap_or(0);
                    output[21..24].fill(0);
                    output[24..40]
                        .copy_from_slice(&ipv6.map(|value| value.address).unwrap_or([0; 16]));
                    output[40..56]
                        .copy_from_slice(&ipv6.map(|value| value.gateway).unwrap_or([0; 16]));
                }
                length as u64
            } else {
                ERROR_INVALID
            }
        }
        SYS_TTY_READ => tty_read(frame),
        SYS_TTY_WRITE => tty_write(frame),
        SYS_ISATTY => u64::from(crate::aarch64_tty::isatty(frame.registers[0])),
        SYS_TCGETATTR => tcgetattr(frame),
        SYS_TCSETATTR => tcsetattr(frame),
        SYS_TCFLUSH => crate::aarch64_tty::flush(frame.registers[0], frame.registers[1])
            .map_or_else(|error| error.abi(), |_| 0),
        SYS_IOCTL => tty_ioctl(frame),
        SYS_SIGACTION => signal_action(frame),
        SYS_RAISE => crate::aarch64_tty::raise(frame.registers[0] as u32)
            .map_or_else(|error| error.abi(), |_| 0),
        SYS_GETPGRP => crate::aarch64_tty::get_process_group().unwrap_or_else(|error| error.abi()),
        SYS_SETPGID => {
            crate::aarch64_tty::set_process_group(frame.registers[0], frame.registers[1])
                .map_or_else(|error| error.abi(), |_| 0)
        }
        SYS_TCGETPGRP => crate::aarch64_tty::terminal_process_group(frame.registers[0])
            .unwrap_or_else(|error| error.abi()),
        SYS_TCSETPGRP => {
            crate::aarch64_tty::set_terminal_process_group(frame.registers[0], frame.registers[1])
                .map_or_else(|error| error.abi(), |_| 0)
        }
        SYS_BRK => {
            crate::aarch64_vm::brk(crate::aarch64_process::current_pid(), frame.registers[0])
                .unwrap_or(ERROR_INVALID)
        }
        SYS_RENAME => {
            let source_address = frame.registers[0];
            let source_length = frame.registers[1] as usize;
            let destination_address = frame.registers[2];
            let destination_length = frame.registers[3] as usize;
            if !crate::security::has_capability(crate::security::CAP_FILE_WRITE)
                || source_length == 0
                || source_length >= crate::vfs::MAX_PATH_BYTES
                || destination_length == 0
                || destination_length >= crate::vfs::MAX_PATH_BYTES
                || !user_range_readable(source_address, source_length)
                || !user_range_readable(destination_address, destination_length)
            {
                ERROR_INVALID
            } else {
                let source = unsafe {
                    core::slice::from_raw_parts(source_address as *const u8, source_length)
                };
                let destination = unsafe {
                    core::slice::from_raw_parts(
                        destination_address as *const u8,
                        destination_length,
                    )
                };
                let result = crate::vfs::rename(source, destination);
                if crate::aarch64_process::current_app_role()
                    == crate::aarch64_process::ProcessRole::Firefox
                    && FIREFOX_MUTATION_TRACES.fetch_add(1, Ordering::Relaxed)
                        < FIREFOX_MUTATION_TRACE_LIMIT
                {
                    crate::serial_println!(
                        "firefox-rename source={} destination={} result={}",
                        core::str::from_utf8(source).unwrap_or("<invalid>"),
                        core::str::from_utf8(destination).unwrap_or("<invalid>"),
                        u8::from(result),
                    );
                }
                u64::from(result)
            }
        }
        SYS_SET_TID_ADDRESS => {
            let address = frame.registers[0];
            if address != 0 && !user_range_writable(address, core::mem::size_of::<u32>()) {
                ERROR_INVALID
            } else {
                crate::aarch64_process::set_tid_address(address).unwrap_or(ERROR_INVALID)
            }
        }
        SYS_GETTID => crate::aarch64_process::current_tid(),
        _ => ERROR_INVALID,
    };
    finish_signal_delivery(frame);
}

const EXEC_PATH_BYTES: usize = 256;
const EXEC_VECTOR_COUNT: usize = 64;
const EXEC_VALUE_BYTES: usize = 512;

fn process_exec(frame: &mut ExceptionFrame) -> bool {
    let mut path = [0u8; EXEC_PATH_BYTES];
    let path_length = match copy_exec_string(frame.registers[0], &mut path) {
        Ok(length) if length != 0 => length,
        Ok(_) => {
            frame.registers[0] = (-2i64) as u64;
            return false;
        }
        Err(error) => {
            frame.registers[0] = error as u64;
            return false;
        }
    };
    let mut argv_values = [[0u8; EXEC_VALUE_BYTES]; EXEC_VECTOR_COUNT];
    let mut argv_lengths = [0usize; EXEC_VECTOR_COUNT];
    let argument_count = match copy_exec_vector(
        frame.registers[1],
        true,
        &mut argv_values,
        &mut argv_lengths,
    ) {
        Ok(count) => count,
        Err(error) => {
            frame.registers[0] = error as u64;
            return false;
        }
    };
    let mut env_values = [[0u8; EXEC_VALUE_BYTES]; EXEC_VECTOR_COUNT];
    let mut env_lengths = [0usize; EXEC_VECTOR_COUNT];
    let environment_count =
        match copy_exec_vector(frame.registers[2], false, &mut env_values, &mut env_lengths) {
            Ok(count) => count,
            Err(error) => {
                frame.registers[0] = error as u64;
                return false;
            }
        };
    let mut arguments: [&[u8]; EXEC_VECTOR_COUNT] = [&[]; EXEC_VECTOR_COUNT];
    for index in 0..argument_count {
        arguments[index] = &argv_values[index][..argv_lengths[index]];
    }
    let mut environment: [&[u8]; EXEC_VECTOR_COUNT] = [&[]; EXEC_VECTOR_COUNT];
    for index in 0..environment_count {
        environment[index] = &env_values[index][..env_lengths[index]];
    }
    let result = crate::aarch64_process::exec_current(
        &path[..path_length],
        &arguments[..argument_count],
        &environment[..environment_count],
        frame,
    );
    if crate::aarch64_process::current_app_role() == crate::aarch64_process::ProcessRole::Firefox {
        crate::serial_println!(
            "MAKOS_FIREFOX_EXEC_ATTEMPT path={} argc={} envc={} result={}",
            core::str::from_utf8(&path[..path_length]).unwrap_or("<invalid>"),
            argument_count,
            environment_count,
            result.as_ref().map_or_else(|error| *error, |_| 0),
        );
    }
    match result {
        Ok(()) => true,
        Err(error) => {
            frame.registers[0] = error as u64;
            false
        }
    }
}

fn copy_exec_vector(
    address: u64,
    required: bool,
    values: &mut [[u8; EXEC_VALUE_BYTES]; EXEC_VECTOR_COUNT],
    lengths: &mut [usize; EXEC_VECTOR_COUNT],
) -> Result<usize, i64> {
    if address == 0 {
        return if required { Err(-14) } else { Ok(0) };
    }
    for index in 0..=EXEC_VECTOR_COUNT {
        let pointer_address = address
            .checked_add((index * core::mem::size_of::<u64>()) as u64)
            .ok_or(-14)?;
        if !user_range_readable(pointer_address, core::mem::size_of::<u64>()) {
            return Err(-14);
        }
        let pointer = unsafe { (pointer_address as *const u64).read_unaligned() };
        if pointer == 0 {
            return if required && index == 0 {
                Err(-22)
            } else {
                Ok(index)
            };
        }
        if index == EXEC_VECTOR_COUNT {
            return Err(-7);
        }
        lengths[index] = copy_exec_string(pointer, &mut values[index])?;
    }
    Err(-7)
}

fn copy_exec_string(address: u64, output: &mut [u8]) -> Result<usize, i64> {
    if address == 0 {
        return Err(-14);
    }
    for index in 0..output.len() {
        let byte_address = address.checked_add(index as u64).ok_or(-14)?;
        if !user_range_readable(byte_address, 1) {
            return Err(-14);
        }
        let byte = unsafe { (byte_address as *const u8).read_volatile() };
        if byte == 0 {
            return Ok(index);
        }
        output[index] = byte;
    }
    Err(-7)
}

const SELECT_WORDS: usize = 4;

struct SelectSnapshot {
    read: [u64; SELECT_WORDS],
    write: [u64; SELECT_WORDS],
    except: [u64; SELECT_WORDS],
    count: u64,
}

fn pselect(frame: &mut ExceptionFrame) -> bool {
    let descriptor_count = frame.registers[0] as usize;
    let read_address = frame.registers[1];
    let write_address = frame.registers[2];
    let except_address = frame.registers[3];
    let options_address = frame.registers[4];
    let options_length = frame.registers[5] as usize;
    let words = descriptor_count.saturating_add(63) / 64;
    let bytes = words.saturating_mul(8);
    if descriptor_count > SELECT_WORDS * 64
        || options_length != 24
        || !user_range_readable(options_address, options_length)
        || !select_set_range_valid(read_address, bytes)
        || !select_set_range_valid(write_address, bytes)
        || !select_set_range_valid(except_address, bytes)
    {
        frame.registers[0] = (-22i64) as u64;
        return false;
    }
    let timeout = unsafe { (options_address as *const i64).read_unaligned() };
    let wait_mask = unsafe { ((options_address + 8) as *const u64).read_unaligned() };
    let wait_mask_present = unsafe { ((options_address + 16) as *const u64).read_unaligned() };
    if timeout < -1 || wait_mask_present > 1 {
        frame.registers[0] = (-22i64) as u64;
        return false;
    }
    if let Err(error) =
        crate::aarch64_tty::begin_wait_mask((wait_mask_present != 0).then_some(wait_mask))
    {
        frame.registers[0] = error.abi();
        return false;
    }
    if crate::aarch64_tty::wait_interrupted() {
        crate::aarch64_process::complete_io_wait();
        frame.registers[0] = crate::aarch64_tty::Errno::Interrupted.abi();
        return false;
    }
    let snapshot = match select_snapshot(
        descriptor_count,
        read_address,
        write_address,
        except_address,
    ) {
        Ok(snapshot) => snapshot,
        Err(errno) => {
            crate::aarch64_process::complete_io_wait();
            crate::aarch64_tty::finish_wait_mask();
            frame.registers[0] = (-errno) as u64;
            return false;
        }
    };
    if crate::aarch64_tty::wait_interrupted() {
        crate::aarch64_process::complete_io_wait();
        frame.registers[0] = crate::aarch64_tty::Errno::Interrupted.abi();
        return false;
    }
    if snapshot.count != 0 || timeout == 0 {
        crate::aarch64_process::complete_io_wait();
        crate::aarch64_tty::finish_wait_mask();
        commit_select_snapshot(
            &snapshot,
            words,
            read_address,
            write_address,
            except_address,
        );
        frame.registers[0] = snapshot.count;
        return false;
    }
    match crate::aarch64_process::block_current_for_io(timeout, frame) {
        crate::aarch64_process::IoBlockResult::Switched => return true,
        crate::aarch64_process::IoBlockResult::TimedOut => {
            crate::aarch64_tty::finish_wait_mask();
            commit_select_snapshot(
                &snapshot,
                words,
                read_address,
                write_address,
                except_address,
            );
            frame.registers[0] = 0;
        }
        crate::aarch64_process::IoBlockResult::Failed => {
            crate::aarch64_tty::finish_wait_mask();
            frame.registers[0] = (-11i64) as u64;
        }
    }
    false
}

fn select_set_range_valid(address: u64, bytes: usize) -> bool {
    address == 0 || (user_range_readable(address, bytes) && user_range_writable(address, bytes))
}

fn select_snapshot(
    descriptor_count: usize,
    read_address: u64,
    write_address: u64,
    except_address: u64,
) -> Result<SelectSnapshot, i64> {
    const POLLIN: u16 = 0x001;
    const POLLPRI: u16 = 0x002;
    const POLLOUT: u16 = 0x004;
    const POLLNVAL: u16 = 0x020;
    let mut snapshot = SelectSnapshot {
        read: [0; SELECT_WORDS],
        write: [0; SELECT_WORDS],
        except: [0; SELECT_WORDS],
        count: 0,
    };
    for fd in 0..descriptor_count {
        let word = fd / 64;
        let bit = 1u64 << (fd % 64);
        let wants_read = select_bit_set(read_address, word, bit);
        let wants_write = select_bit_set(write_address, word, bit);
        let wants_except = select_bit_set(except_address, word, bit);
        if !wants_read && !wants_write && !wants_except {
            continue;
        }
        let requested = (if wants_read { POLLIN } else { 0 })
            | (if wants_write { POLLOUT } else { 0 })
            | (if wants_except { POLLPRI } else { 0 });
        let events = descriptor_poll_events(fd as u64, requested);
        if events & POLLNVAL != 0 {
            return Err(9);
        }
        let read_ready = wants_read && events & POLLIN != 0;
        let write_ready = wants_write && events & POLLOUT != 0;
        let except_ready = wants_except && events & POLLPRI != 0;
        if read_ready {
            snapshot.read[word] |= bit;
        }
        if write_ready {
            snapshot.write[word] |= bit;
        }
        if except_ready {
            snapshot.except[word] |= bit;
        }
        snapshot.count += u64::from(read_ready || write_ready || except_ready);
    }
    Ok(snapshot)
}

fn select_bit_set(address: u64, word: usize, bit: u64) -> bool {
    address != 0
        && unsafe { ((address + word as u64 * 8) as *const u64).read_unaligned() } & bit != 0
}

fn commit_select_snapshot(
    snapshot: &SelectSnapshot,
    words: usize,
    read_address: u64,
    write_address: u64,
    except_address: u64,
) {
    for word in 0..words {
        for (address, value) in [
            (read_address, snapshot.read[word]),
            (write_address, snapshot.write[word]),
            (except_address, snapshot.except[word]),
        ] {
            if address != 0 {
                unsafe {
                    ((address + word as u64 * 8) as *mut u64).write_unaligned(value);
                }
            }
        }
    }
}

fn descriptor_poll_events(fd: u64, requested: u16) -> u16 {
    if fd < 3 {
        crate::aarch64_tty::poll_events(fd, requested)
    } else if crate::aarch64_socket::is_owned(fd) {
        crate::aarch64_socket::poll_events(fd, u32::from(requested)) as u16
    } else {
        crate::vfs::poll_events(fd, requested)
    }
}

fn poll_descriptors(address: u64, count: usize) -> u64 {
    let mut ready = 0u64;
    for index in 0..count {
        let entry = address + index as u64 * 8;
        let fd = unsafe { core::ptr::read_volatile(entry as *const i32) };
        let requested = unsafe { core::ptr::read_volatile((entry + 4) as *const u16) };
        let returned = if fd < 0 {
            0
        } else {
            descriptor_poll_events(fd as u64, requested)
        };
        unsafe {
            core::ptr::write_volatile((entry + 6) as *mut u16, returned);
        }
        ready += u64::from(returned != 0);
    }
    ready
}

fn tty_read(frame: &ExceptionFrame) -> u64 {
    let address = frame.registers[1];
    let length = frame.registers[2] as usize;
    if !user_range_writable(address, length) {
        return crate::aarch64_tty::Errno::Invalid.abi();
    }
    let output = unsafe { core::slice::from_raw_parts_mut(address as *mut u8, length) };
    crate::aarch64_tty::read(frame.registers[0], output)
        .map_or_else(|error| error.abi(), |count| count as u64)
}

fn tty_write(frame: &ExceptionFrame) -> u64 {
    let address = frame.registers[1];
    let length = frame.registers[2] as usize;
    if !user_range_readable(address, length) {
        return crate::aarch64_tty::Errno::Invalid.abi();
    }
    let bytes = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
    crate::aarch64_tty::write(frame.registers[0], bytes)
        .map_or_else(|error| error.abi(), |count| count as u64)
}

fn tcgetattr(frame: &ExceptionFrame) -> u64 {
    let address = frame.registers[1];
    if frame.registers[2] as usize != core::mem::size_of::<crate::aarch64_tty::UserTermios>()
        || !user_range_writable(
            address,
            core::mem::size_of::<crate::aarch64_tty::UserTermios>(),
        )
    {
        return crate::aarch64_tty::Errno::Invalid.abi();
    }
    match crate::aarch64_tty::get_termios(frame.registers[0]) {
        Ok(value) => {
            unsafe { (address as *mut crate::aarch64_tty::UserTermios).write_unaligned(value) };
            0
        }
        Err(error) => error.abi(),
    }
}

fn tcsetattr(frame: &ExceptionFrame) -> u64 {
    let address = frame.registers[2];
    if frame.registers[3] as usize != core::mem::size_of::<crate::aarch64_tty::UserTermios>()
        || !user_range_readable(
            address,
            core::mem::size_of::<crate::aarch64_tty::UserTermios>(),
        )
    {
        return crate::aarch64_tty::Errno::Invalid.abi();
    }
    let value = unsafe { (address as *const crate::aarch64_tty::UserTermios).read_unaligned() };
    crate::aarch64_tty::set_termios(frame.registers[0], frame.registers[1], value)
        .map_or_else(|error| error.abi(), |_| 0)
}

fn tty_ioctl(frame: &ExceptionFrame) -> u64 {
    let address = frame.registers[2];
    if frame.registers[3] as usize != core::mem::size_of::<crate::aarch64_tty::UserWindowSize>() {
        return crate::aarch64_tty::Errno::Invalid.abi();
    }
    match frame.registers[1] {
        crate::aarch64_tty::TIOCGWINSZ
            if user_range_writable(
                address,
                core::mem::size_of::<crate::aarch64_tty::UserWindowSize>(),
            ) =>
        {
            match crate::aarch64_tty::window_size(frame.registers[0]) {
                Ok(value) => {
                    unsafe {
                        (address as *mut crate::aarch64_tty::UserWindowSize).write_unaligned(value)
                    };
                    0
                }
                Err(error) => error.abi(),
            }
        }
        crate::aarch64_tty::TIOCSWINSZ
            if user_range_readable(
                address,
                core::mem::size_of::<crate::aarch64_tty::UserWindowSize>(),
            ) =>
        {
            let value =
                unsafe { (address as *const crate::aarch64_tty::UserWindowSize).read_unaligned() };
            crate::aarch64_tty::set_window_size(frame.registers[0], value)
                .map_or_else(|error| error.abi(), |_| 0)
        }
        _ => crate::aarch64_tty::Errno::Invalid.abi(),
    }
}

fn signal_action(frame: &ExceptionFrame) -> u64 {
    let action_size = core::mem::size_of::<crate::aarch64_tty::UserSignalAction>();
    let new_address = frame.registers[1];
    let old_address = frame.registers[2];
    if frame.registers[3] as usize != action_size
        || (new_address != 0 && !user_range_readable(new_address, action_size))
        || (old_address != 0 && !user_range_writable(old_address, action_size))
    {
        return crate::aarch64_tty::Errno::Invalid.abi();
    }
    let replacement = (new_address != 0).then(|| unsafe {
        (new_address as *const crate::aarch64_tty::UserSignalAction).read_unaligned()
    });
    match crate::aarch64_tty::signal_action(frame.registers[0] as u32, replacement) {
        Ok(previous) => {
            if old_address != 0 {
                unsafe {
                    (old_address as *mut crate::aarch64_tty::UserSignalAction)
                        .write_unaligned(previous)
                };
            }
            0
        }
        Err(error) => error.abi(),
    }
}

fn finish_signal_delivery(frame: &mut ExceptionFrame) {
    match crate::aarch64_tty::deliver_pending(frame) {
        crate::aarch64_tty::Delivery::None | crate::aarch64_tty::Delivery::Handler => {}
        crate::aarch64_tty::Delivery::Terminate(signal) => {
            crate::aarch64_process::exit_from_exception(128 + u64::from(signal), frame)
        }
        crate::aarch64_tty::Delivery::Stop(signal) => {
            crate::aarch64_process::stop_from_exception(signal, frame)
        }
    }
}

fn handle_irq(kind: u64, frame: &mut ExceptionFrame) {
    let first = IRQ_ENTRIES.fetch_add(1, Ordering::AcqRel) == 0;
    if first {
        crate::serial_println!("MAKOS_AARCH64_IRQ_TRACE stage=entry");
    }
    let cpu_interface = GIC_CPU_BASE.load(Ordering::Acquire);
    if cpu_interface == 0 {
        crate::fatal("AArch64 IRQ before GIC init");
    }
    let primary = unsafe { mmio_read32(cpu_interface + GICC_IAR) };
    let alias = primary & 0x3ff == 1022;
    let acknowledge = if alias {
        unsafe { mmio_read32(cpu_interface + GICC_AIAR) }
    } else {
        primary
    };
    let intid = acknowledge & 0x3ff;
    if first {
        crate::serial_println!(
            "MAKOS_AARCH64_IRQ_TRACE stage=ack iar={:#x} intid={} group1_alias={}",
            acknowledge,
            intid,
            u8::from(alias),
        );
    }
    let timer = intid == TIMER_INTID.load(Ordering::Acquire);
    let scheduler_sgi = intid == SMP_SCHEDULER_SGI;
    if timer {
        let interval = TIMER_INTERVAL.load(Ordering::Acquire);
        program_virtual_timer(read_virtual_counter().saturating_add(interval));
        // One global 100 Hz clock: AP preemption streams must not multiply
        // monotonic time or run CPU0-owned deadline/device service.
        if cpu_index() == 0 {
            TIMER_TICKS.fetch_add(1, Ordering::AcqRel);
        }
    } else if intid < 1020 && !scheduler_sgi {
        UNEXPECTED_IRQS.fetch_add(1, Ordering::AcqRel);
    }
    if intid < 1020 {
        unsafe {
            mmio_write32(
                cpu_interface + if alias { GICC_AEOIR } else { GICC_EOIR },
                acknowledge,
            )
        };
    }
    if first {
        crate::serial_println!("MAKOS_AARCH64_IRQ_TRACE stage=eoi-return");
    }
    if kind == 9 && crate::aarch64_process::stop_remote_group_member_from_irq(frame) {
        return;
    }
    if timer {
        if cpu_index() == 0 {
            // AP block calls publish copied requests before sleeping in EL1.
            // The CPU0 timer is the production owner service point. The
            // driver defers a tick if this IRQ interrupted a direct CPU0
            // request, avoiding recursive acquisition of the device lock.
            crate::aarch64_virtio_blk::service_requests_from_timer();
        }
        if kind == 9 {
            // Socket/net state uses non-recursive locks. Only run the RX
            // bottom half when the IRQ interrupted EL0; running it over an
            // in-flight socket syscall could spin forever on its own lock.
            if cpu_index() == 0 {
                service_network_rx_on_owner_cpu();
                // Input is timer-polled hardware too. Poll before scheduling
                // so CPU-bound EL0 code cannot delay key/button delivery until
                // its next unrelated syscall; queued keys can select Firefox's
                // blocked watcher in this same 100 Hz preemption.
                service_input_on_owner_cpu();
            }
            crate::aarch64_process::preempt_from_timer(frame);
            finish_signal_delivery(frame);
        } else if cpu_index() == 0 {
            crate::aarch64_process::service_timer_waiters();
        }
    }
    if kind == 9 {
        // Close the publication race where this IRQ entered from EL0 just
        // before CPU0 installed the remote-stop target mask.
        let _ = crate::aarch64_process::stop_remote_group_member_from_irq(frame);
    }
}

pub(crate) fn return_to_kernel(frame: &mut ExceptionFrame, status: u64) {
    frame.registers[0] = status;
    frame.elr = core::ptr::addr_of!(aarch64_user_return) as u64;
    frame.spsr = 0x3c5;
    frame.sp_el0 = 0;
    frame.ttbr0 = kernel_root();
    frame.tpidr_el0 = 0;
}

fn read_virtual_counter() -> u64 {
    let value: u64;
    unsafe { asm!("mrs {value}, CNTVCT_EL0", value = out(reg) value, options(nomem, nostack)) };
    value
}

fn program_virtual_timer(deadline: u64) {
    unsafe {
        asm!(
            "msr CNTV_CVAL_EL0, {deadline}",
            "msr CNTV_CTL_EL0, {enable}",
            "isb",
            deadline = in(reg) deadline,
            enable = in(reg) 1u64,
            options(nostack)
        )
    }
}

unsafe fn mmio_read32(address: u64) -> u32 {
    unsafe { core::ptr::read_volatile(address as *const u32) }
}

unsafe fn mmio_write32(address: u64, value: u32) {
    unsafe { core::ptr::write_volatile(address as *mut u32, value) }
}

pub fn halt_forever() -> ! {
    disable_interrupts();
    loop {
        unsafe { asm!("wfe", options(nomem, nostack)) }
    }
}
