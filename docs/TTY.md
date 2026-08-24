# MakOS TTY foundation

## Status

`crates/tty` is a real, allocation-free `no_std` terminal core. It is not a
fake nano command, a userspace editor, or a libc shim. It supplies two pieces
required below POSIX libc and ncurses:

- bounded canonical/raw line discipline with echo, erase, kill, EOF,
  terminal-generated signal events, `VMIN`, CR/NL mapping, output NL/CRLF
  processing, record boundaries, polling, and input flushing;
- incremental bounded ANSI/VT parser with cursor addressing/movement, erase,
  SGR attributes and 16 colors, save/restore, scroll regions, insert/delete,
  alternate screen, cursor visibility, application cursor/keypad modes,
  auto-wrap, bracketed paste, device reports, OSC/DCS suppression, and safe
  recovery from malformed/oversized CSI sequences.

Host unit tests and `aarch64-unknown-none` compilation pass. AArch64 now has
controlling-terminal kernel/syscall integration plus a live concrete cell
renderer driven by `AnsiParser`. Terminal window resize updates rows/columns,
publishes winsize, and queues `SIGWINCH`. Legacy EL0 shell keys pass through a
raw bounded discipline without changing its editing behavior. Official musl,
ncurses 6.5, and compiled `makos` terminfo are integrated on AArch64. x86_64
parity, general PTY breadth, and UTF-8 wide-cell rendering remain.

## Kernel API contract

Kernel owns one line discipline, parser, window size, foreground process group,
and wait queue per terminal/PTY:

```rust
use makos_tty::{AnsiParser, LineDiscipline, Termios, WindowSize};

type KernelLineDiscipline = LineDiscipline<4096, 1024, 64>;

struct KernelTty {
    input: KernelLineDiscipline,
    parser: AnsiParser,
    winsize: WindowSize,
    foreground_pgid: u64,
    // kernel lock + reader wait queue + concrete TerminalBackend
}
```

Capacities are policy. Overflow is explicit (`ReceiveResult::Full`), never heap
allocation or silent memory overwrite.

### Input path

1. Keyboard driver maps a key to terminal bytes. Arrow/function key encoding
   observes application-cursor/keypad state recorded by `TerminalBackend`.
2. Under TTY lock, call `LineDiscipline::receive(byte, echo_sink)` once per
   byte. Echo sink must feed bytes through same output parser/backend as writes.
3. `LineReady`, `EndOfFileReady`, or raw-mode readability wakes blocked readers.
4. `Signal(signal)` requests kernel delivery to foreground process group.
   Kernel maps interrupt/quit/suspend to `SIGINT`, `SIGQUIT`, `SIGTSTP`.
5. `read()` copies no more than one canonical record. `WouldBlock` means sleep,
   or `EAGAIN` for nonblocking file descriptions. Raw `VMIN=0` may return zero.

No blocking occurs inside crate. Kernel must release TTY lock before sleeping.

### Output path

1. `write(1|2, bytes)` applies `LineDiscipline::write_output`.
2. Result feeds `AnsiParser::advance` under TTY lock.
3. Concrete `TerminalBackend` mutates terminal cells/state, then compositor
   damages/redraws affected cells. It must clamp all coordinates and signed
   movement to current scroll region/screen.
4. `Report` callbacks enqueue terminal response bytes into input path. Suggested
   responses: `CSI 0 n`, `CSI row ; column R`, and fixed VT100-compatible DA.

Parser accepts split escape sequences across writes. OSC/DCS payload is ignored
until BEL or ST, preventing title/control payload from leaking as screen text.

### Resize path

Window/compositor resize computes rows, columns, and pixels, updates
`WindowSize`, then sends `SIGWINCH` to foreground process group when value
changes. `ioctl(TIOCGWINSZ)` returns a snapshot; `TIOCSWINSZ` updates PTY value
after permission validation. Parser has no global dimensions by design.

### Termios path

Syscall layer translates native C `struct termios` to/from `Termios`; Rust
layout is intentionally not exported as C ABI. Required mappings:

| POSIX flag/control | crate field/behavior |
| --- | --- |
| `ICANON` | `canonical` + record-aware reads |
| `ECHO`, `ECHOE`, `ECHOK` | echo fields |
| `ISIG`, `NOFLSH` | signal event + optional flush |
| `ICRNL` | `map_cr_to_nl` |
| `OPOST` + `ONLCR` | `output_crlf` |
| `VERASE`, `VKILL`, `VEOF` | control-byte fields |
| `VINTR`, `VQUIT`, `VSUSP` | signal control-byte fields |
| `VMIN` | `minimum_read` |

`set_termios` never hides queued data. Canonical-to-raw preserves committed and
edited bytes. Raw-to-canonical returns `PendingRawInput` until caller drains or
performs `TCIFLUSH`. `VTIME` needs kernel timers and is deliberately not faked.

## Integration status

1. Complete: `crates/tty` is a root workspace member and kernel dependency.
2. Complete: `kernel/src/graphics.rs` implements `TerminalBackend` with main and
   alternate cells, per-cell attributes/16 colors, cursor, scroll region,
   insert/delete, modes, reports, erase, and dynamic viewport bounds.
3. Complete on AArch64: legacy key queue passes through a raw bounded
   `LineDiscipline`; POSIX fd input uses controlling-terminal discipline.
4. Complete on AArch64: compositor resize publishes concrete cell/pixel
   `WindowSize` into controlling TTY and queues `SIGWINCH` on change.
5. AArch64: complete. x86_64 remains. AArch64 fd `0`, `1`, `2`, output,
   polling read, `isatty`, `tcgetattr`, `tcsetattr`, `tcflush`, and
   `ioctl(TIOCGWINSZ/TIOCSWINSZ)` are kernel-owned. `EAGAIN` is returned while
   no input is readable; scheduler-backed blocking read remains required.
6. AArch64: foreground process groups, `SIGINT`, `SIGQUIT`, `SIGTSTP`,
   `SIGCONT`, `SIGWINCH`, `sigaction`, kernel-saved `sigreturn`, per-task masks,
   clone inheritance, and atomic `ppoll`/`epoll_pwait` masks are implemented.
   General `kill`/`killpg`, full POSIX delivery frames, and x86_64 parity remain.
7. Add POSIX headers/libc wrappers only after syscall numbers and C ABI structs
   are frozen: `termios.h`, `sys/ioctl.h`, `unistd.h`, `signal.h`.
8. Complete: compiled `makos` terminfo advertises only implemented
   capabilities. Nano launches with `TERM=makos` and the packaged DB.
9. Complete: official ncurses 6.5 and GNU nano 9.1 cross-build through
   `ports/ncurses/build-makos.sh` and `ports/nano/build-makos.sh`.

## GNU nano `required-abi.txt` audit

| Terminal gate | Foundation evidence | Remaining integration |
| --- | --- | --- |
| `tcgetattr`, `tcsetattr` | AArch64 C ABI + calls 65/66 + musl wrappers | x86_64 parity |
| `ioctl(TIOCGWINSZ)` | AArch64 call 68 + compositor-driven winsize + musl | x86_64 parity |
| `SIGWINCH`, `sigaction`, `raise` | AArch64 calls 69/70/75 + musl restorer | general kill breadth |
| character-device stdin/out/err | inherited fd 0/1/2 + line discipline + foreground pgrp | x86_64 parity |
| ANSI/VT cursor/erase/attributes/alternate screen | live `AnsiParser` + cell `TerminalBackend` | UTF-8/wide-cell renderer work |
| `TERM` + terminfo | compiled and packaged `makos` entry | UTF-8 extensions |
| ncurses APIs | official ncurses 6.5 target library | ncursesw/locale breadth |

Native upstream GNU nano now builds, packages, and has shell/process/foreground
TTY wiring. Its focused two-boot HVF probe passes edit, Ctrl-O save, Ctrl-X
status-0 exit, reopen, persisted byte verification, reboot, and reopen again.

## Verification

```sh
cargo fmt --manifest-path crates/tty/Cargo.toml -- --check
cargo test --manifest-path crates/tty/Cargo.toml
cargo check --manifest-path crates/tty/Cargo.toml --target aarch64-unknown-none
cargo check -p makos-kernel --target aarch64-unknown-none
python3 scripts/boot_test_aarch64.py
```

Tests cover canonical editing/echo, record boundaries, EOF, signal flushing,
raw lowercase/punctuation, queue wrap, safe mode transitions, output mapping,
incremental ncurses-style CSI, DEC modes, alternate screen, scroll region,
insert/delete, reports, OSC/DCS suppression, overflow, malformed input, and
parser recovery.

AArch64 boot regression verifies live ANSI backend marker, raw disciplined
lowercase/punctuation, terminal window resize/winsize dimensions, hardware
cursor queue, and a screenshot after every cursor move. Since QMP screendumps
contain scanout rather than the separate cursor plane, every transition must
confine damage to old/new cursor bounds; returning to the start restores the
complete framebuffer scene byte-identically.

## Deliberate boundaries

- `VTIME` timeout behavior requires kernel timer/wakeup integration.
- Mouse selection/highlighting and bounded per-user clipboard copy/paste are
  implemented by the AArch64 renderer/backend. UTF-8 decoding, wide/combining
  cells, scalable glyph rasterization, and scrollback storage remain.
- PTY master/slave pairs, sessions, controlling terminal, process groups, and
  permissions belong to kernel object/process layers.
- SGR supports terminal-default plus ANSI 16-color palette. A matching terminfo
  entry must not advertise 256/true-color capabilities yet.
