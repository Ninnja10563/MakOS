.section .text.gc_helper_get_regs_and_sp,"ax",%progbits
.balign 16
.global gc_helper_get_regs_and_sp
.type gc_helper_get_regs_and_sp,%function
gc_helper_get_regs_and_sp:
    stp x19, x20, [x0, #0]
    stp x21, x22, [x0, #16]
    stp x23, x24, [x0, #32]
    stp x25, x26, [x0, #48]
    stp x27, x28, [x0, #64]
    str x29, [x0, #80]
    mov x0, sp
    ret
.size gc_helper_get_regs_and_sp, . - gc_helper_get_regs_and_sp
