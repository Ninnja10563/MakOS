/* SPDX-License-Identifier: MIT */
.section .text._start,"ax",%progbits
.global _start
.type _start,%function
_start:
    mov x29, xzr
    mov x30, xzr
    bl __makos_bootstrap_main
    mov x8, #5
    svc #0
    brk #0
.size _start, .-_start
