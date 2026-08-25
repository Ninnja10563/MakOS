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
    adrp x9, child_in_el1
    add x9, x9, :lo12:child_in_el1
1:
    ldar w10, [x9]
    cbz w10, 1b
    mov x0, #56
    mov x8, #119
    svc #0
    b failure

child:
    // The boot-only SMP probe hook publishes this word after the exception
    // has entered EL1, then holds the syscall until the remote stop request is
    // visible. The normal yield ABI ignores x0 and never takes this path.
    adrp x0, child_in_el1
    add x0, x0, :lo12:child_in_el1
    mov x8, #1
    svc #0

failure:
    mov x0, #99
    mov x8, #5
    svc #0
2:
    wfe
    b 2b
    .size _start, . - _start

    .section .bss,"aw",%nobits
    .balign 16
child_in_el1:
    .word 0
    .skip 12
child_tls:
    .skip 16
    .balign 16
child_stack:
    .skip 4096
child_stack_end:
