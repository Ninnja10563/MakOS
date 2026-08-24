#!/usr/bin/env python3
"""Host safety tests for exclusive sparse install-target creation."""

import pathlib
import tempfile

import create_blank_install_target


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="makos-install-target-test-") as temporary:
        root = pathlib.Path(temporary)
        source = root / "source.img"
        source.write_bytes(b"source" + bytes(4096 * 512 - 6))
        target = root / "target.img"
        size = create_blank_install_target.create(source, target)
        assert size == source.stat().st_size == target.stat().st_size
        assert target.read_bytes() == bytes(size)

        target.write_bytes(b"preserve")
        try:
            create_blank_install_target.create(source, target)
        except ValueError:
            pass
        else:
            raise AssertionError("existing target unexpectedly overwritten")
        assert target.read_bytes() == b"preserve"

        try:
            create_blank_install_target.create(source, source)
        except ValueError:
            pass
        else:
            raise AssertionError("source/target alias unexpectedly accepted")

        invalid = root / "invalid.img"
        invalid.write_bytes(b"unaligned")
        try:
            create_blank_install_target.create(invalid, root / "new.img")
        except ValueError:
            pass
        else:
            raise AssertionError("invalid source geometry unexpectedly accepted")
    print("MAKOS_INSTALL_TARGET_TEST_OK exclusive=1 preserve=1 alias_refusal=1 geometry=1")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
