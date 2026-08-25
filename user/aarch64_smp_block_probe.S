    .section .text._start,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    // The kernel creates this private opt-in fixture inode as uid/gid 1000 and
    // binds this immutable process to the minimum file-write capability. A
    // 4 KiB write, fsync, close/reopen, and 4 KiB read traverse the ordinary
    // VFS/MakFS4/block stack from AP1.
    adrp x0, probe_path
    add x0, x0, :lo12:probe_path
    mov x1, #24
    mov x2, #2
    mov x3, #2
    mov x8, #11
    svc #0
    mov x19, x0
    cmn x0, #1
    b.eq failure

    adrp x1, write_buffer
    add x1, x1, :lo12:write_buffer
    mov x20, #4096
    mov x21, #0
1:
    strb w21, [x1, x21]
    add x21, x21, #1
    cmp x21, x20
    b.ne 1b

    mov x0, x19
    mov x2, x20
    mov x8, #17
    svc #0
    cmp x0, x20
    b.ne failure

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

    adrp x0, probe_path
    add x0, x0, :lo12:probe_path
    mov x1, #24
    mov x2, #0
    mov x3, #2
    mov x8, #11
    svc #0
    mov x19, x0
    cmn x0, #1
    b.eq failure

    mov x0, x19
    adrp x1, read_buffer
    add x1, x1, :lo12:read_buffer
    mov x2, x20
    mov x8, #12
    svc #0
    cmp x0, x20
    b.ne failure

    adrp x9, write_buffer
    add x9, x9, :lo12:write_buffer
    adrp x10, read_buffer
    add x10, x10, :lo12:read_buffer
    mov x11, #0
2:
    ldrb w12, [x9, x11]
    ldrb w13, [x10, x11]
    cmp w12, w13
    b.ne failure
    add x11, x11, #1
    cmp x11, x20
    b.ne 2b

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
probe_path:
    .ascii "/home/user/.smp-block-io"

    .section .bss,"aw",%nobits
    .balign 4096
write_buffer:
    .skip 4096
read_buffer:
    .skip 4096
