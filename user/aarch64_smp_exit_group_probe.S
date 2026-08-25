    .section .text._start,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    movz x0, #0x0f00
    movk x0, #0x007d, lsl #16
    adrp x1, child_stack_end
    add x1, x1, :lo12:child_stack_end
    mov x2, #0
    adrp x3, child_tls
    add x3, x3, :lo12:child_tls
    mov x4, #0
    mov x8, #80
    svc #0
    cbz x0, child
    tbnz x0, #63, failure

parent_wait:
    adrp x9, child_running
    add x9, x9, :lo12:child_running
1:
    ldar w10, [x9]
    cbz w10, 1b
    mov x0, #55
    mov x8, #119
    svc #0
    b failure

child:
    adrp x9, child_running
    add x9, x9, :lo12:child_running
    mov w10, #1
    stlr w10, [x9]
    mov x11, #1
2:
    add x11, x11, #1
    eor x11, x11, x11, ror #7
    b 2b

failure:
    mov x0, #99
    mov x8, #5
    svc #0
3:
    wfe
    b 3b
    .size _start, . - _start

    .section .bss,"aw",%nobits
    .balign 16
child_running:
    .word 0
    .skip 12
child_tls:
    .skip 16
    .balign 16
child_stack:
    .skip 4096
child_stack_end:
