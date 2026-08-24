# GNU nano 9.1 dependency contract

## Required

- C17-capable cross compiler and ELF linker
- POSIX-like libc and startup (`argc`, `argv`, `envp`, `errno`)
- ncurses or ncursesw headers/library
- terminfo database matching terminal `$TERM`
- terminal device supporting raw mode, dimensions, cursor addressing, erase,
  attributes, and resize notification
- persistent filesystem with seekable regular files and atomic replacement

GNU nano bundles gnulib compatibility sources in its release tarball. Gnulib is
not a replacement for missing kernel file, VM, process, signal, or TTY services.

## Optional upstream features

- gettext/libintl: translated UI; this port's bootstrap profile disables NLS
- libmagic: content-based syntax selection; disabled in bootstrap profile
- wide-character ncurses + libc locale functions: UTF-8
- external spell checker, formatter, linter, and command execution: process,
  pipe, wait, and signal APIs

## Port profiles

`host`: normal official nano feature set, `--disable-nls --disable-libmagic`.
Used only to prove pinned source configures/builds and reports version 9.1.

`makos-bootstrap` (target): implemented native AArch64 PIE. It disables NLS,
libmagic, UTF-8/wide curses, and fork/vfork-dependent external commands. It uses
official musl 1.2.6, official narrow ncurses 6.5, and the `makos` terminfo entry.
Core upstream editing and persistent file operations remain enabled. Target end
state is UTF-8 ncursesw plus normal external-tool integration. MakOS currently
rejects nano's optional defensive `SIGABRT`/`SIGSEGV` handler registrations;
normal edit/save/exit passes, while general fatal-signal breadth remains kernel
work.
