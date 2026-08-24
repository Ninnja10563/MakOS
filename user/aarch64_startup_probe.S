.section .text._start,"ax",@progbits
.global _start
.type _start, %function
_start:
    // Kernel supplies x0=argc, x1=argv, x2=envp and canonical SysV stack.
    mov x3, sp
    b startup_main
.size _start, .-_start

