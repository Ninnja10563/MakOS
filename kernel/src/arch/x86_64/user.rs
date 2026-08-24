use core::arch::asm;

const USER_DATA_SELECTOR: u16 = (3 << 3) | 3;
const USER_CODE_SELECTOR: u16 = (4 << 3) | 3;

pub fn enter(instruction_pointer: u64, stack_pointer: u64, argument: u64) -> ! {
    enter_startup(instruction_pointer, stack_pointer, argument, 0, 0)
}

pub fn enter_startup(
    instruction_pointer: u64,
    stack_pointer: u64,
    argument_count: u64,
    arguments: u64,
    environment: u64,
) -> ! {
    unsafe {
        asm!(
            "mov ax, {user_data}",
            "mov ds, ax",
            "mov es, ax",
            "push {user_data}",
            "push rcx",
            "push 0x202",
            "push {user_code}",
            "push r8",
            "iretq",
            user_data = const USER_DATA_SELECTOR,
            user_code = const USER_CODE_SELECTOR,
            in("rcx") stack_pointer,
            in("r8") instruction_pointer,
            in("rdi") argument_count,
            in("rsi") arguments,
            in("rdx") environment,
            options(noreturn)
        )
    }
}
