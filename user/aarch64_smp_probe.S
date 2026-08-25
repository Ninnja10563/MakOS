    .section .text._start,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    mov x19, x0
    mov x8, #27
    svc #0
    add x0, x0, #2
    mov x8, #103
    svc #0
    movz x18, #0x8480
    movk x18, #0x001e, lsl #16
    mov x17, #1
1:
    eor x17, x17, x18
    ror x17, x17, #7
    subs x18, x18, #1
    b.ne 1b
    add x0, x19, #40
    mov x8, #5
    svc #0
2:
    wfe
    b 2b
    .size _start, . - _start
