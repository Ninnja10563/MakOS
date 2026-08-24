    .section .text.aarch64_scheduler,"ax",%progbits
    .balign 16
    .global aarch64_context_register_test
    .type aarch64_context_register_test,%function
aarch64_context_register_test:
    sub sp, sp, #144
    stp x19, x20, [sp, #0]
    stp x21, x22, [sp, #16]
    stp x23, x24, [sp, #32]
    stp x25, x26, [sp, #48]
    stp x27, x28, [sp, #64]
    stp x29, x30, [sp, #80]
    str q8, [sp, #96]
    mrs x9, fpcr
    mrs x10, fpsr
    stp x9, x10, [sp, #112]
    str x0, [sp, #128]
    cbnz x0, 7f
    movi v0.16b, #0x11
    movi v8.16b, #0x28
    movi v16.16b, #0x50
    movi v31.16b, #0x7f
    mov x9, #0x400000
    msr fpcr, x9
    mov x9, #0x10
    msr fpsr, x9
    b 6f
7:
    movi v0.16b, #0x91
    movi v8.16b, #0xa8
    movi v16.16b, #0xd0
    movi v31.16b, #0xff
    mov x9, #0x800000
    msr fpcr, x9
    mov x9, #0x2
    msr fpsr, x9
6:
    mov x0, #0x100
    mov x1, #0x101
    mov x2, #0x102
    mov x3, #0x103
    mov x4, #0x104
    mov x5, #0x105
    mov x6, #0x106
    mov x7, #0x107
    mov x8, #0x108
    mov x9, #0x109
    mov x10, #0x10a
    mov x11, #0x10b
    mov x12, #0x10c
    mov x13, #0x10d
    mov x14, #0x10e
    mov x15, #0x10f
    mov x16, #0x110
    mov x17, #0x111
    mov x19, #0x113
    mov x20, #0x114
    mov x21, #0x115
    mov x22, #0x116
    mov x23, #0x117
    mov x24, #0x118
    mov x25, #0x119
    mov x26, #0x11a
    mov x27, #0x11b
    mov x28, #0x11c
    mov x29, #0x11d
    mov x30, #0x11e
    movz x18, #0xf080
    movk x18, #0x02fa, lsl #16
1:
    subs x18, x18, #1
    b.ne 1b
    cmp x0, #0x100
    b.ne 9f
    cmp x1, #0x101
    b.ne 9f
    cmp x2, #0x102
    b.ne 9f
    cmp x3, #0x103
    b.ne 9f
    cmp x4, #0x104
    b.ne 9f
    cmp x5, #0x105
    b.ne 9f
    cmp x6, #0x106
    b.ne 9f
    cmp x7, #0x107
    b.ne 9f
    cmp x8, #0x108
    b.ne 9f
    cmp x9, #0x109
    b.ne 9f
    cmp x10, #0x10a
    b.ne 9f
    cmp x11, #0x10b
    b.ne 9f
    cmp x12, #0x10c
    b.ne 9f
    cmp x13, #0x10d
    b.ne 9f
    cmp x14, #0x10e
    b.ne 9f
    cmp x15, #0x10f
    b.ne 9f
    cmp x16, #0x110
    b.ne 9f
    cmp x17, #0x111
    b.ne 9f
    cmp x19, #0x113
    b.ne 9f
    cmp x20, #0x114
    b.ne 9f
    cmp x21, #0x115
    b.ne 9f
    cmp x22, #0x116
    b.ne 9f
    cmp x23, #0x117
    b.ne 9f
    cmp x24, #0x118
    b.ne 9f
    cmp x25, #0x119
    b.ne 9f
    cmp x26, #0x11a
    b.ne 9f
    cmp x27, #0x11b
    b.ne 9f
    cmp x28, #0x11c
    b.ne 9f
    cmp x29, #0x11d
    b.ne 9f
    cmp x30, #0x11e
    b.ne 9f
    mov x18, #0x112
    movz x17, #0xf080
    movk x17, #0x02fa, lsl #16
2:
    subs x17, x17, #1
    b.ne 2b
    cmp x18, #0x112
    b.ne 9f
    ldr x10, [sp, #128]
    cbnz x10, 7f
    mrs x9, fpcr
    mov x11, #0x400000
    cmp x9, x11
    b.ne 9f
    mrs x9, fpsr
    cmp x9, #0x10
    b.ne 9f
    movi v1.16b, #0x11
    cmeq v1.16b, v0.16b, v1.16b
    uminv b1, v1.16b
    umov w9, v1.b[0]
    cmp w9, #0xff
    b.ne 9f
    movi v1.16b, #0x28
    cmeq v1.16b, v8.16b, v1.16b
    uminv b1, v1.16b
    umov w9, v1.b[0]
    cmp w9, #0xff
    b.ne 9f
    movi v1.16b, #0x50
    cmeq v1.16b, v16.16b, v1.16b
    uminv b1, v1.16b
    umov w9, v1.b[0]
    cmp w9, #0xff
    b.ne 9f
    movi v1.16b, #0x7f
    cmeq v1.16b, v31.16b, v1.16b
    uminv b1, v1.16b
    umov w9, v1.b[0]
    cmp w9, #0xff
    b.ne 9f
    b 8f
7:
    mrs x9, fpcr
    mov x11, #0x800000
    cmp x9, x11
    b.ne 9f
    mrs x9, fpsr
    cmp x9, #0x2
    b.ne 9f
    movi v1.16b, #0x91
    cmeq v1.16b, v0.16b, v1.16b
    uminv b1, v1.16b
    umov w9, v1.b[0]
    cmp w9, #0xff
    b.ne 9f
    movi v1.16b, #0xa8
    cmeq v1.16b, v8.16b, v1.16b
    uminv b1, v1.16b
    umov w9, v1.b[0]
    cmp w9, #0xff
    b.ne 9f
    movi v1.16b, #0xd0
    cmeq v1.16b, v16.16b, v1.16b
    uminv b1, v1.16b
    umov w9, v1.b[0]
    cmp w9, #0xff
    b.ne 9f
    movi v1.16b, #0xff
    cmeq v1.16b, v31.16b, v1.16b
    uminv b1, v1.16b
    umov w9, v1.b[0]
    cmp w9, #0xff
    b.ne 9f
8:
    ldp x9, x10, [sp, #112]
    msr fpcr, x9
    msr fpsr, x10
    ldr q8, [sp, #96]
    ldp x19, x20, [sp, #0]
    ldp x21, x22, [sp, #16]
    ldp x23, x24, [sp, #32]
    ldp x25, x26, [sp, #48]
    ldp x27, x28, [sp, #64]
    ldp x29, x30, [sp, #80]
    add sp, sp, #144
    mov x0, #1
    ret
9:
    ldp x9, x10, [sp, #112]
    msr fpcr, x9
    msr fpsr, x10
    ldr q8, [sp, #96]
    ldp x19, x20, [sp, #0]
    ldp x21, x22, [sp, #16]
    ldp x23, x24, [sp, #32]
    ldp x25, x26, [sp, #48]
    ldp x27, x28, [sp, #64]
    ldp x29, x30, [sp, #80]
    add sp, sp, #144
    mov x0, #0
    ret
    .size aarch64_context_register_test, . - aarch64_context_register_test
