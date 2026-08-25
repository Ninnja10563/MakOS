    .section .text._start,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    // Create a blocking IPv4 UDP socket.
    mov x0, #2
    mov x1, #2
    mov x2, #17
    mov x8, #47
    svc #0
    mov x19, x0
    cmn x0, #1
    b.eq failure

    // Connect to QEMU slirp's real DNS service at 10.0.2.3:53.
    mov x0, x19
    adrp x1, dns_address
    add x1, x1, :lo12:dns_address
    mov x2, #8
    mov x8, #48
    svc #0
    cmp x0, #1
    b.ne failure

    mov x0, x19
    adrp x1, dns_query
    add x1, x1, :lo12:dns_query
    mov x2, #29
    mov x3, #0
    mov x8, #49
    svc #0
    cmp x0, #29
    b.ne failure

    // This receive must block on AP1. CPU0 later drains the virtio RX ring,
    // publishes the socket wake, and sends the scheduler SGI.
    mov x0, x19
    adrp x1, dns_response
    add x1, x1, :lo12:dns_response
    mov x2, #512
    mov x3, #0
    mov x8, #50
    svc #0
    cmp x0, #29
    b.lo failure

    adrp x1, dns_response
    add x1, x1, :lo12:dns_response
    ldrb w2, [x1]
    cmp w2, #0x4d
    b.ne failure
    ldrb w2, [x1, #1]
    cmp w2, #0x4c
    b.ne failure
    ldrb w2, [x1, #2]
    tst w2, #0x80
    b.eq failure

    mov x0, x19
    mov x8, #51
    svc #0
    cmp x0, #1
    b.ne failure

    mov x0, #63
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
dns_address:
    .hword 2
    .byte 0x00, 0x35
    .byte 10, 0, 2, 3
dns_query:
    .byte 0x4d, 0x4c, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00
    .byte 0x00, 0x00, 0x00, 0x00
    .byte 7, 'e', 'x', 'a', 'm', 'p', 'l', 'e'
    .byte 3, 'c', 'o', 'm', 0
    .byte 0x00, 0x01, 0x00, 0x01

    .section .bss,"aw",%nobits
    .balign 16
dns_response:
    .skip 512
