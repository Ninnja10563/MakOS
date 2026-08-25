    .section .text._start,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    // Seed state that must survive AP1 -> AP2 migration.
    movz x19, #0x5678
    movk x19, #0x1234, lsl #16
    movz x20, #0xcdef
    movk x20, #0x89ab, lsl #16
    movz x21, #0x55aa
    movk x21, #0xaa55, lsl #16
    movz x9, #0x3344
    movk x9, #0x1122, lsl #16
    msr tpidr_el0, x9
    sub sp, sp, #16
    movz x10, #0xbeef
    movk x10, #0xfeed, lsl #16
    str x10, [sp]
    movi v0.16b, #0x5a

    // The armed scheduler fixture treats yield(target_cpu=2) as a forced
    // migration request for this immutable SmpProbe task.
    mov x0, #2
    mov x8, #1
    svc #0
    cbnz x0, failure

    movz x9, #0x5678
    movk x9, #0x1234, lsl #16
    cmp x19, x9
    b.ne failure
    movz x9, #0xcdef
    movk x9, #0x89ab, lsl #16
    cmp x20, x9
    b.ne failure
    movz x9, #0x55aa
    movk x9, #0xaa55, lsl #16
    cmp x21, x9
    b.ne failure
    mrs x9, tpidr_el0
    movz x10, #0x3344
    movk x10, #0x1122, lsl #16
    cmp x9, x10
    b.ne failure
    ldr x9, [sp]
    movz x10, #0xbeef
    movk x10, #0xfeed, lsl #16
    cmp x9, x10
    b.ne failure
    fmov x9, d0
    movz x10, #0x5a5a
    movk x10, #0x5a5a, lsl #16
    movk x10, #0x5a5a, lsl #32
    movk x10, #0x5a5a, lsl #48
    cmp x9, x10
    b.ne failure
    add sp, sp, #16

    mov x0, #71
    mov x8, #5
    svc #0

failure:
    add sp, sp, #16
    mov x0, #98
    mov x8, #5
    svc #0
1:
    wfe
    b 1b
    .size _start, . - _start
