# MicroPython for MakOS

Real upstream MicroPython 1.28.0 parser, compiler, bytecode VM, object model,
exceptions, and tracing GC, built as a freestanding AArch64 MakOS EL0 process.
No host interpreter or command parser is substituted.

`build-makos.sh` verifies the official release archive SHA-256, builds the
upstream core, then produces `build/ports/micropython/micropython-makos.elf`.
The process receives a MakOS path in `x0`, reads at most the MakFS file limit,
compiles it as file input, executes it, prints through `SYS_WRITE`, and exits
through `SYS_EXIT`. External imports remain disabled until directory-backed
module lookup is added.

Inside MakOS:

```text
write demo.py print([x*x for x in range(6)])
python demo.py
```

`python FILE` creates an isolated process/address space, copies its canonical
path into the new process stack, assigns current user's read-only VFS identity,
waits for exit, then reclaims ELF, stack, page-table, TTY, FD, and VM resources.

Host verification: `MAKOS_MICROPYTHON_SOURCE=/path/to/source ./test.sh`.

License: upstream MicroPython is MIT; see its `LICENSE` file in fetched source.
