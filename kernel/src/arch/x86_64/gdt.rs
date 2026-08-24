use core::arch::asm;

const KERNEL_CODE_SELECTOR: u16 = 1 << 3;
const KERNEL_DATA_SELECTOR: u16 = 2 << 3;
const TSS_SELECTOR: u16 = 5 << 3;
const STACK_SIZE: usize = 16 * 1024;

#[repr(C, packed)]
struct TaskStateSegment {
    reserved_1: u32,
    rsp: [u64; 3],
    reserved_2: u64,
    ist: [u64; 7],
    reserved_3: u64,
    reserved_4: u16,
    iomap_base: u16,
}

impl TaskStateSegment {
    const fn new() -> Self {
        Self {
            reserved_1: 0,
            rsp: [0; 3],
            reserved_2: 0,
            ist: [0; 7],
            reserved_3: 0,
            reserved_4: 0,
            iomap_base: core::mem::size_of::<Self>() as u16,
        }
    }
}

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

#[repr(C, align(16))]
struct PrivilegeStack([u8; STACK_SIZE]);

static mut GDT: [u64; 7] = [0; 7];
static mut TSS: TaskStateSegment = TaskStateSegment::new();
static mut RING0_STACK: PrivilegeStack = PrivilegeStack([0; STACK_SIZE]);
static mut DOUBLE_FAULT_STACK: PrivilegeStack = PrivilegeStack([0; STACK_SIZE]);

pub fn init() {
    let ring0_top = (&raw mut RING0_STACK).cast::<u8>() as u64 + STACK_SIZE as u64;
    let double_fault_top = (&raw mut DOUBLE_FAULT_STACK).cast::<u8>() as u64 + STACK_SIZE as u64;
    let tss_ptr = (&raw mut TSS).cast::<TaskStateSegment>();
    unsafe {
        (*tss_ptr).rsp[0] = ring0_top;
        (*tss_ptr).ist[0] = double_fault_top;
    }

    let tss_base = tss_ptr as u64;
    let tss_limit = (core::mem::size_of::<TaskStateSegment>() - 1) as u64;
    let gdt = (&raw mut GDT).cast::<u64>();
    unsafe {
        gdt.add(0).write(0);
        gdt.add(1).write(0x00af_9a00_0000_ffff);
        gdt.add(2).write(0x00af_9200_0000_ffff);
        gdt.add(3).write(0x00af_f200_0000_ffff);
        gdt.add(4).write(0x00af_fa00_0000_ffff);
        gdt.add(5).write(
            (tss_limit & 0xffff)
                | ((tss_base & 0x00ff_ffff) << 16)
                | (0x89u64 << 40)
                | (((tss_limit >> 16) & 0xf) << 48)
                | (((tss_base >> 24) & 0xff) << 56),
        );
        gdt.add(6).write(tss_base >> 32);
    }
    let pointer = DescriptorTablePointer {
        limit: (core::mem::size_of::<[u64; 7]>() - 1) as u16,
        base: gdt as u64,
    };
    unsafe {
        asm!(
            "lgdt [{table}]",
            "mov ax, {data}",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            "xor eax, eax",
            "mov fs, ax",
            "mov gs, ax",
            "push {code}",
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",
            "mov ax, {tss}",
            "ltr ax",
            table = in(reg) &pointer,
            data = const KERNEL_DATA_SELECTOR,
            code = const KERNEL_CODE_SELECTOR,
            tss = const TSS_SELECTOR,
            out("rax") _,
        );
    }

    let task_register: u16;
    unsafe { asm!("str {0:x}", out(reg) task_register, options(nomem, nostack, preserves_flags)) };
    if task_register != TSS_SELECTOR {
        crate::fatal("TSS activation failed");
    }
    crate::serial_println!("cpu gdt=owned tss={:#x} ist1=ready", task_register);
}

pub fn set_ring0_stack(stack_top: u64) {
    if stack_top == 0 || stack_top & 0xf != 0 {
        crate::fatal("invalid ring-0 stack");
    }
    unsafe { (*(&raw mut TSS)).rsp[0] = stack_top };
}
