    .section .text._start,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    // Ordinary CPU0 EL0 execution supplies production timer bottom halves.
    mov x8, #27
    svc #0
    add x19, x0, #100
1:
    mov x8, #27
    svc #0
    cmp x0, x19
    b.lo 1b

    mov x0, #70
    mov x8, #5
    svc #0

    mov x0, #99
    mov x8, #5
    svc #0
2:
    wfe
    b 2b
    .size _start, . - _start
