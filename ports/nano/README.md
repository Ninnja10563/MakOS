# GNU nano for MakOS

This is a native AArch64 port of unmodified upstream GNU nano 9.1, not a
lookalike editor. `source.lock` pins the official archive and SHA-256 digest.

## Provenance and license

- Upstream: <https://www.nano-editor.org/>
- Release: GNU nano 9.1 (2026-06-23)
- Archive: <https://ftp.gnu.org/gnu/nano/nano-9.1.tar.xz>
- SHA-256: `5f47764274cb7532349ce0aa20ec10f1e8e851a6e9fa3eb66812c43d196db042`
- Signature: <https://ftp.gnu.org/gnu/nano/nano-9.1.tar.xz.sig>
- License: GPL-3.0-or-later; documentation GFDL-1.2-or-later

Release distribution must include corresponding source, port changes, and
upstream license texts as required by GPLv3.

## Build and package

```sh
ports/nano/fetch.sh
ports/nano/build-makos.sh
ports/nano/package-makos.sh build/makos-nano-data.img
ports/nano/test.sh
```

`build-makos.sh` first builds official ncurses 6.5 as AArch64 static PIC, stages
its headers and the truthful `makos` terminfo entry, then configures official
nano sources for `aarch64-unknown-makos`. The only source patch teaches upstream
`config.sub` the MakOS OS name. Output:

`build/ports/nano/makos/src/nano`

The output is an AArch64 PIE using `/lib/ld-musl-aarch64.so.1` and the packaged
MakOS `libc.so`. It links the genuine ncurses library; host libraries are never
linked. Bootstrap profile disables NLS, libmagic, UTF-8/wide curses, and
fork/vfork-dependent external commands. Core editing, search, navigation,
selection, file open/write, backup prompts, and nano shortcuts remain upstream.

`package-makos.sh` installs `/usr/bin/nano`, `/usr/share/terminfo/m/makos`, and
the nano and ncurses license texts into a checksummed MakOS package image. The
shell's `nano [FILE]` command spawns that ELF in its own foreground process
group with `TERM=makos`.

## Verification state

Fresh cross-build, ELF audit, package verification, and kernel AArch64 compile
pass. `scripts/boot_test_aarch64_nano.py` passes two boots under Apple HVF: it
opens genuine nano, enters punctuation-bearing text, writes with Ctrl-O, exits
with Ctrl-X/status 0, reopens, then reboots and verifies the persisted byte
count before reopening again. See `DEPENDENCIES.md` and `required-abi.txt` for
bootstrap-profile limits.
