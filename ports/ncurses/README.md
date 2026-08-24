# ncurses for MakOS

This port pins official ncurses 6.5, builds its narrow-character static library
as AArch64 PIC, and installs headers plus a truthful `makos` terminfo entry. The
entry advertises only ANSI/VT operations implemented by `crates/tty` and the
graphical terminal backend. GNU nano links this real library into its MakOS PIE.

```sh
ports/ncurses/build-makos.sh
```

Output: `build/ports/ncurses/stage`. Source archive is SHA-256 verified.
