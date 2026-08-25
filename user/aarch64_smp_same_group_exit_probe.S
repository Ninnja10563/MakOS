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

parent:
    // The kernel fixture rendezvous-holds both tasks after SVC entry. Distinct
    // statuses prove that the first coordinator owner wins for the group.
    mov x0, #59
    mov x8, #119
    svc #0
    b failure

child:
    mov x0, #60
    mov x8, #119
    svc #0

failure:
    mov x0, #99
    mov x8, #5
    svc #0
1:
    wfe
    b 1b
    .size _start, . - _start

    .section .bss,"aw",%nobits
    .balign 16
child_tls:
    .skip 16
    .balign 16
child_stack:
    .skip 4096
child_stack_end:
