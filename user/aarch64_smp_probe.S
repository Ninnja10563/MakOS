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
    adrp x0, futex_word
    add x0, x0, :lo12:futex_word
    mov x1, #0
    mov x2, #0
    adrp x3, futex_timeout
    add x3, x3, :lo12:futex_timeout
    mov x4, #0
    mov x5, #0
    mov x8, #82
    svc #0
    mov x9, #-110
    cmp x0, x9
    b.eq 3f
    mov x9, #-11
    cmp x0, x9
    b.ne 4f
3:
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
4:
    add x0, x19, #90
    mov x8, #5
    svc #0
2:
    wfe
    b 2b
    .size _start, . - _start

    .section .data,"aw",%progbits
    .balign 8
futex_word:
    .word 0
    .word 0
futex_timeout:
    .quad 0
    .quad 20000000
