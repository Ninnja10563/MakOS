    .section .text._start,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    // Exercise the ordinary native surface ABI from AP1. Present must retain
    // the scene and defer all virtio-GPU MMIO to CPU0's timer bottom half.
    mov x0, #96
    mov x1, #64
    mov x2, #0
    mov x8, #8
    svc #0
    mov x19, x0
    cbz x0, failure

    mov x0, x19
    movz x1, #0x3366
    movk x1, #0x00cc, lsl #16
    adrp x2, rectangle
    add x2, x2, :lo12:rectangle
    mov x8, #9
    svc #0
    cmp x0, #1
    b.ne failure

    mov x0, x19
    mov x8, #10
    svc #0
    cmp x0, #1
    b.ne failure

    // Keep the surface alive across multiple CPU0 timer service points.
    mov x8, #27
    svc #0
    add x20, x0, #20
1:
    mov x8, #27
    svc #0
    cmp x0, x20
    b.lo 1b

    mov x0, #67
    mov x8, #5
    svc #0

failure:
    mov x0, #99
    mov x8, #5
    svc #0
2:
    wfe
    b 2b
    .size _start, . - _start

    .section .rodata,"a",%progbits
    .balign 16
rectangle:
    .word 0, 0, 96, 64
