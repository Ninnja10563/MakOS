    .section .text._start,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    // Open the world-readable persistent boot counter without truncation.
    // Fsync validates the descriptor, releases the VFS lock, then issues a
    // real block FLUSH without granting this pre-login probe extra authority.
    adrp x0, boot_path
    add x0, x0, :lo12:boot_path
    mov x1, #15
    mov x2, #0
    mov x3, #2
    mov x8, #11
    svc #0
    mov x19, x0
    cmn x0, #1
    b.eq failure

    mov x0, x19
    mov x8, #97
    svc #0
    cmp x0, #0
    b.ne failure

    mov x0, x19
    mov x8, #13
    svc #0
    cmp x0, #1
    b.ne failure

    mov x0, #65
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

    .section .rodata,"a",%progbits
boot_path:
    .ascii "/boot-count.txt"
