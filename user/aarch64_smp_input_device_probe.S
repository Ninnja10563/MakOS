    .section .text._start,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    // The focused runtime harness sends a real QEMU virtio-keyboard Ctrl-K.
    // The blocking syscall is retried only after the device ring wakes this AP.
    mov x8, #6
    svc #0
    cmp x0, #11
    b.ne failure

    mov x0, #61
    mov x8, #5
    svc #0

failure:
    mov x0, #99
    mov x8, #5
    svc #0
1:
    wfe
    b 1b
    .size _start, . - _start
