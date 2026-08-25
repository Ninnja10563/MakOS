    .section .text._start,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    // x0 is the immutable worker index. Repeated yields contend on the shared
    // Ready queue and preserve the worker's saved context between dispatches.
    mov x19, x0
    mov x20, #48
1:
    eor x21, x21, x19
    add x21, x21, x20
    mov x0, #0
    mov x8, #1
    svc #0
    cbnz x0, failure
    subs x20, x20, #1
    b.ne 1b

    add x0, x19, #80
    mov x8, #5
    svc #0

failure:
    mov x0, #97
    mov x8, #5
    svc #0
2:
    wfe
    b 2b
    .size _start, . - _start
