# MakOS Text Edit

`user/aarch64_textedit.c` is a freestanding native AArch64 EL0 editor, not a
host application or scripted mock. It owns a process surface and stores bytes
through MakOS VFS calls `open(11)`, `read(12)`, `close(13)`, `file_write(17)`,
and `create(43)`.

## Data model

- One bounded 2,048-byte buffer, matching current MakFS per-file limit.
- Arbitrary printable ASCII, tabs, and LF newlines.
- Insertion, Backspace, Delete, Left, Right, Up, Down, Home, and End.
- Preferred-column vertical movement, horizontal/vertical viewport tracking.
- Ctrl-S saves; F2 remains available. Dirty state clears only after exact full
  write plus successful close. Header exposes a visible Save control.
- Mouse drag selects text with visible highlight. Ctrl/Command-A selects all;
  Ctrl/Command-C, X, and V use a per-user 64 KiB kernel clipboard. Text Edit
  accepts at most its remaining 2 KiB document capacity on paste. Backspace,
  Delete, and typing replace the selected range.
- Nonblocking empty input yields immediately without rendering; editor redraws
  only after real input, eliminating prior full-surface busy loop.
- Missing files open as empty documents. I/O failure remains visible and dirty.
- Paths are canonicalized below `/home/user`; absolute paths elsewhere,
  traversal, nested separators, controls, and names over 32 bytes are rejected.

Text Edit's legacy save path intentionally requests truncate-in-place. VFS now
supports preserving `O_RDWR` opens and rename, but editor does not yet write a
temporary file plus atomic rename. Editor makes no false atomicity guarantee.

## Runtime integration

Run `edit` for `note.txt`, or `edit NAME` for another file below `/home/user`.
Text Edit is linked into native AArch64 shell ELF and runs as a modal EL0 app
until MakOS gains saved-register process context switching. It is not a kernel
editor or host-side mock. Same process owns editor surface and real VFS handles.

AArch64 dispatcher implements ABI calls 11, 12, 13, 17, and 43 with copy-in,
copy-out, ownership, bounds, and write-capability checks. AArch64-only call 58
closes a surface without changing normative cross-architecture ABI maximum 57.
Input maps Left `0x11`, Right `0x12`, Up `0x13`, Down `0x14`, Home `0x15`, End
`0x16`, Delete `0x17`, Escape `0x1b`, F2 `0x82`, and Ctrl-S/Command-S save
`0x83`. Ctrl/Command-A/C/X/V map to `0x84`-`0x87`. Clipboard calls 110-111
validate user memory, capability, session UID, and the 64 KiB bound. Escape
refuses dirty close.
Titlebar X injects Escape, preserving same dirty-data rule. Closed editor surface
is retained and safely reused on reopen.

## Verification

```sh
clang -std=c17 -Wall -Wextra -Werror scripts/text_edit_test.c \
  -o build/text-edit-test
build/text-edit-test

make test-aarch64

clang -target aarch64-unknown-none-elf -std=c17 -ffreestanding \
  -fno-builtin -fno-stack-protector -fno-pic -fno-unwind-tables \
  -fno-asynchronous-unwind-tables -mgeneral-regs-only -Os \
  -c user/aarch64_textedit.c -o build/aarch64-textedit.o
```
