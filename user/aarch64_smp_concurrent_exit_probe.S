    .section .text._start,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    // x0 is supplied by the kernel fixture as this independent process's
    // exit status. Both CPUs rendezvous inside the syscall implementation.
    mov x8, #119
    svc #0

    mov x0, #99
    mov x8, #5
    svc #0
1:
    wfe
    b 1b
    .size _start, . - _start
