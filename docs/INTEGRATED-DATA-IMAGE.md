# Integrated data image

`scripts/integrate_data_image.py` creates the final package/data disk from an
existing 1 GiB MakOS data image. It never changes the source image and never
starts QEMU.

```sh
make integrated-data-aarch64 \
  SOURCE_DATA_IMAGE=build/my-existing-data.img
```

Output is content-addressed as `build/makos-integrated-<identity>.img`, with a
matching JSON manifest. Identity covers both preserved-region SHA-256 hashes,
the complete package-tree SHA-256, fixed disk layout, and pinned versions; the
filename itself uses the first 16 hexadecimal digits of the complete image's
SHA-256. Identical inputs reuse one name; differing state cannot silently
overwrite it.

## Region and preservation contract

- `[0, 1 MiB)`: MakFS/MakFS4 metadata, accounts, settings, small writable
  files. Cloned byte-for-byte.
- `[1 MiB, 512 MiB)`: read-only package metadata/payload. Initialized to zero,
  then rebuilt; stale package tail bytes cannot affect output.
- `[512 MiB, 1 GiB)`: MakFS4 writable extents, Firefox profile, user files.
  Cloned byte-for-byte.

Builder hashes both mutable regions before cloning, after packaging, and again
on source. Package spill or concurrent source mutation aborts before publish.

## Required package set and gates

Packaging uses current native Firefox stage plus current staged GNU
nano/ncurses and CPython artifacts. Final verifier requires:

- Firefox ESR 140.13.0 (`firefox`, genuine `libxul.so`, `omni.ja`, application
  metadata, Mozilla and bundled-font license documents);
- GNU nano 9.1, ncurses 6.5 terminfo, GPL and ncurses license texts;
- CPython 3.14.7, deterministic standard-library ZIP, PSF license text.

Every record and payload passes existing CRC-32 verification. Required payload
SHA-256 values must equal staging artifacts. Firefox, nano, Python must be
AArch64 `ET_DYN` executables using `/lib/ld-musl-aarch64.so.1`; `libxul.so`
must be an AArch64 `ET_DYN` shared library. Empty licenses are rejected.

Firefox additionally carries
`/usr/lib/firefox/makos-build-provenance.json`. The bounded canonical record
pins the ESR source commit and ordered patch-series identity, hashes five
outputs only after `mach build` and `audit-binary.sh` succeed, then hashes the
five exact post-strip runtime payloads. Package construction revalidates the
build stamp; integration compares every runtime hash to package metadata and
includes the full record in the semantic image identity. The strict Firefox
target repeats CRC, ELF, and provenance validation before it creates QEMU, so
old images without a record and current-looking images with stale payloads
fail before boot.

Focused network- and QEMU-independent regression test:

```sh
make test-integrated-data
```
