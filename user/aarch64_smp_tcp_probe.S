    .section .text._start,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    // Create a blocking IPv4 TCP socket on AP1.
    mov x0, #2
    mov x1, #1
    mov x2, #6
    mov x8, #47
    svc #0
    mov x19, x0
    cmn x0, #1
    b.eq failure

    // Connect through QEMU user networking to the host fixture.
    mov x0, x19
    adrp x1, tcp_address
    add x1, x1, :lo12:tcp_address
    mov x2, #8
    mov x8, #48
    svc #0
    cmp x0, #1
    b.ne failure

    mov x0, x19
    adrp x1, tcp_request
    add x1, x1, :lo12:tcp_request
    mov x2, #16
    mov x3, #0
    mov x8, #49
    svc #0
    cmp x0, #16
    b.ne failure

    // The syscall must block AP1 until CPU0 pumps the response and wakes it.
    mov x0, x19
    adrp x1, tcp_response
    add x1, x1, :lo12:tcp_response
    mov x2, #64
    mov x3, #0
    mov x8, #50
    svc #0
    cmp x0, #18
    b.ne failure

    adrp x1, tcp_response
    add x1, x1, :lo12:tcp_response
    adrp x2, expected_response
    add x2, x2, :lo12:expected_response
    mov x3, #18
1:
    ldrb w4, [x1], #1
    ldrb w5, [x2], #1
    cmp w4, w5
    b.ne failure
    subs x3, x3, #1
    b.ne 1b

    mov x0, x19
    mov x8, #51
    svc #0
    cmp x0, #1
    b.ne failure

    mov x0, #69
    mov x8, #5
    svc #0

failure:
    mov x0, #99
    mov x8, #5
    svc #0
2:
    wfe
    b 2b
    .size _start, . - _start

    .section .rodata,"a",%progbits
    .balign 8
tcp_address:
    .hword 2
    .byte 0x46, 0xa0
    .byte 10, 0, 2, 2
tcp_request:
    .ascii "MAKOS_AP_TCP_TX\n"
expected_response:
    .ascii "MAKOS_CPU0_TCP_RX\n"

    .section .bss,"aw",%nobits
    .balign 16
tcp_response:
    .skip 64
