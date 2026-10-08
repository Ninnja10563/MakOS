    .section .text._start,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    // Immutable native workload; the kernel supplies only phase 1, 2 or 3.
    mov x20, x0
    sub x20, x20, #1
    cmp x20, #2
    b.hi failure
    mov x0, #2
    mov x1, #2
    mov x2, #17
    mov x8, #47
    svc #0
    mov x19, x0
    tbnz x0, #63, failure

    mov x0, x19
    adrp x1, udp_address
    add x1, x1, :lo12:udp_address
    mov x2, #8
    mov x8, #48
    svc #0
    cmp x0, #1
    b.ne failure

    mov x0, x19
    adrp x1, udp_payloads
    add x1, x1, :lo12:udp_payloads
    mov x2, #18
    madd x1, x20, x2, x1
    mov x3, #0
    mov x8, #49
    svc #0
    cmp x0, #18
    b.ne failure

    mov x0, x19
    mov x8, #51
    svc #0
    cmp x0, #1
    b.ne failure
    mov x0, #42
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
    .balign 8
udp_address:
    .hword 2
    .byte 0x46, 0xa1
    .byte 10, 0, 2, 2
udp_payloads:
    .ascii "MAKOS_NET_OWNER_1\n"
    .ascii "MAKOS_NET_OWNER_2\n"
    .ascii "MAKOS_NET_OWNER_3\n"
