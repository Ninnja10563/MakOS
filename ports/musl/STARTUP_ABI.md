# AArch64 static musl process-entry contract

Current staged musl `crt1.o` cannot consume MakOS's legacy one-value `x0`
entry. A bootable static executable needs this AArch64 SysV initial state.

## Registers and stack

- `PC` = ELF `e_entry`; `SP` is 16-byte aligned and points at `argc`.
- `x29 = 0` and `x30 = 0` are established by musl `_start`; incoming general
  registers need no argument convention.
- Initial stack, in consecutive 64-bit words:

```text
argc
argv[0] ... argv[argc-1]
0
envp[0] ... envp[n-1]
0
auxv_type, auxv_value ...
AT_NULL, 0
```

All referenced strings and auxiliary data must remain readable through libc
initialization. `argv[0]` should name the executable.

## Required auxiliary vector

For real Firefox/static TLS operation, provide at least:

- `AT_PHDR`, `AT_PHENT=56`, `AT_PHNUM`: mapped program headers.
- `AT_PAGESZ=4096`.
- `AT_ENTRY`: ELF entry address.
- `AT_UID`, `AT_EUID`, `AT_GID`, `AT_EGID`, `AT_SECURE`.
- `AT_RANDOM`: pointer to 16 unpredictable readable bytes.
- `AT_EXECFN`: pointer to executable path; strongly recommended.
- `AT_HWCAP` and `AT_HWCAP2`: only capabilities actually exposed to EL0.

`AT_PHDR`/`AT_PHNUM` are required when a binary contains `PT_TLS`; musl uses
them to locate and copy the initial TLS image. `AT_RANDOM` has a deterministic
fallback, but that is not acceptable for a browser security boundary.

## Thread runtime

MakOS preserves `TPIDR_EL0` across timer preemption and implements Linux
AArch64 `set_tid_address` (96), `gettid` (178), `clone` (220), and `futex`
(98) through native calls 78-82. Threads receive distinct stacks/TLS while
sharing VM, files, credentials, and process identity. Exit zeroes
clear-child-tid and FIFO-wakes joiners. Upstream musl create/join passes HVF.

Remaining pthread breadth: absolute realtime/bitset futex variants, broader
signal masks/delivery, cancellation edge cases, and multithreaded exec teardown.
`libpthread.a` remains musl's expected empty compatibility archive because
pthread symbols live in `libc.a`.
