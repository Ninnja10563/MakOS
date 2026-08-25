    .section .text._start,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    // Stay in ordinary CPU0 EL0 for one counter second without syscalls. The
    // pending virtio-net response must therefore enter through the lower-EL
    // IRQ vector; a timer-only recovery drain cannot satisfy the IRQ evidence.
    mrs x19, cntvct_el0
    mrs x20, cntfrq_el0
    add x19, x19, x20
1:
    mrs x0, cntvct_el0
    cmp x0, x19
    b.lo 1b

    mov x0, #64
    mov x8, #5
    svc #0

    mov x0, #99
    mov x8, #5
    svc #0
2:
    wfe
    b 2b
    .size _start, . - _start
