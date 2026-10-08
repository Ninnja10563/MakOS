    .section .text._start,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    // An immutable, kernel-armed boot fixture, never a production syscall.
    // Seed ordinary EL0 state before entering a loop consisting only of BR.
    movz x19, #0x5678
    movk x19, #0x1234, lsl #16
    movz x20, #0xcdef
    movk x20, #0x89ab, lsl #16
    movz x9, #0x3344
    movk x9, #0x1122, lsl #16
    msr tpidr_el0, x9
    sub sp, sp, #16
    str x19, [sp]
    adr x16, loop_b
    adr x17, loop_a
    mov x9, #0x80000000
    msr nzcv, x9
    br x17

    // The native linker fixes _start at 0x10000000. The kernel validates
    // these two immutable loop PCs, not an untrusted address from a syscall.
    // BR x16/x17 sets hardware PSTATE.BTYPE=1 on FEAT_BTI; every boundary
    // inside this loop has it set, without an instruction-sampling race.
    .org 128
loop_a:
    br x16
loop_b:
    br x17
    .size _start, . - _start
