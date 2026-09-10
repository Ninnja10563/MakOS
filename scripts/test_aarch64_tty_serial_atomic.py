#!/usr/bin/env python3
"""Exercise production serial output with host lock/PL011 adapters.

The production byte loops and formatted-output entry point are extracted, not
reimplemented. Only the AArch64 IRQ-mask/spinlock and MMIO byte writer are host
adapters. The real TTY crate supplies the previous output translation oracle.
This is a host atomicity regression, not an AArch64 hardware/runtime result.
"""

from pathlib import Path
import re
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
SERIAL = (ROOT / "kernel/src/serial.rs").read_text()
TTY = (ROOT / "kernel/src/aarch64_tty.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()


def item(source: str, declaration: str) -> str:
    start = source.index(declaration)
    opening = source.index("{", start)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


def compact(source: str) -> str:
    return re.sub(r"\s+", "", re.sub(r"//[^\n]*", "", source))


tty_write = item(TTY, "pub fn write(fd:")
tty_compact = compact(tty_write)
serial_call = "crate::serial::write_tty_bytes(bytes,state.line.termios().output_crlf);"
assert tty_compact.count(serial_call) == 1
assert "letmutsink=TerminalDisplaySink;" in tty_compact
assert "state.line.write_output(bytes,&mutsink);" in tty_compact
assert tty_compact.index("returnErr(Errno::BadFileDescriptor);") < tty_compact.index(serial_call)
assert "!matches!(fd,1|2)||!state.fd_open(pid,fd)" in tty_compact
assert tty_compact.index(serial_call) < tty_compact.index("state.line.write_output")
assert "TerminalSink;" not in tty_write
assert "write_bytes(" not in tty_write
display = item(TTY, "impl ByteSink for TerminalDisplaySink")
assert "crate::graphics::terminal_write(bytes);" in display
assert "serial" not in display
echo = item(TTY, "impl ByteSink for TerminalSink")
assert "crate::serial::write_bytes(bytes);" in echo
assert "crate::graphics::terminal_write(bytes);" in echo

# The normal musl M_write=17 path must continue enforcing readable user memory
# and fd/TTY semantics. The record emitter does not bypass that route.
native_write_start = ARCH.index("        SYS_FILE_WRITE => {")
native_write = item(ARCH[native_write_start:], "        SYS_FILE_WRITE =>")
assert "user_range_readable(address, length)" in native_write
assert "crate::aarch64_tty::write(frame.registers[0], input)" in native_write

declarations = (
    "pub fn print(",
    "pub fn write_bytes(",
    "pub fn write_tty_bytes(",
    "fn write_output_bytes(",
    "impl Write for Serial",
)
excerpts = [item(SERIAL, declaration) for declaration in declarations]
output = item(SERIAL, "fn write_output_bytes(")
assert output.count("SerialGuard::acquire()") == 1
assert output.index("SerialGuard::acquire()") < output.index("for &byte in bytes")
assert "graphics" not in output
assert "write_output_bytes(bytes, false);" in excerpts[1]
assert "write_output_bytes(bytes, output_crlf);" in excerpts[2]

# Host tests substitute the lock, not the IRQ instructions. Keep the real
# acquire/release ordering protected structurally without executing host DAIF.
acquire = item(SERIAL, "impl SerialGuard")
release = item(SERIAL, "impl Drop for SerialGuard")
assert acquire.index('"msr daifset, #0xf"') < acquire.index("compare_exchange_weak")
assert "Ordering::Acquire" in acquire
assert release.index("SERIAL_LOCK.store(false, Ordering::Release)") < release.index('"msr daif, {saved}"')

production = "\n\n".join(excerpts).replace('#[cfg(target_arch = "aarch64")]\n', "")
assert "asm!(" not in production

with tempfile.TemporaryDirectory(prefix="makos-tty-serial-atomic-") as temporary:
    directory = Path(temporary)
    library = directory / "libmakos_tty.rlib"
    subprocess.run(
        ["rustc", "--edition=2024", "--crate-name", "makos_tty", "--crate-type", "rlib",
         str(ROOT / "crates/tty/src/lib.rs"), "-o", str(library)],
        check=True,
    )
    source = directory / "serial_output.rs"
    source.write_text(production)
    fixture = directory / "test.rs"
    fixture.write_text((ROOT / "scripts/test_aarch64_tty_serial_atomic.rs").read_text())

    def compile_fixture(name: str) -> Path:
        binary = directory / name
        subprocess.run(
            ["rustc", "--edition=2024", "--test", str(fixture),
             "--extern", f"makos_tty={library}", "-o", str(binary)],
            check=True,
        )
        return binary

    subprocess.run(
        [str(compile_fixture("tty-serial-tests")), "--test-threads=1"],
        check=True, timeout=30,
    )

    # Reintroduce the old production line-discipline-to-serial chunk boundary
    # only in the generated fixture. Its byte stream is unchanged but the
    # same complete-record critical-section assertion must reject it.
    old_tty_write = """
pub fn write_tty_bytes(bytes: &[u8], output_crlf: bool) {
    struct OldTerminalSink;
    impl makos_tty::ByteSink for OldTerminalSink {
        fn write(&mut self, bytes: &[u8]) { write_bytes(bytes); }
    }
    let mut termios = makos_tty::Termios::sane();
    termios.output_crlf = output_crlf;
    let line = makos_tty::LineDiscipline::<16, 8, 4>::new(termios);
    line.write_output(bytes, &mut OldTerminalSink);
}
"""
    source.write_text(production.replace(excerpts[2], old_tty_write))
    negative = subprocess.run(
        [str(compile_fixture("old-tty-chunks")), "--exact", "tty_record_has_one_serial_critical_section"],
        capture_output=True, text=True, timeout=15,
    )
    if negative.returncode == 0 or "TTY record split across serial critical sections" not in negative.stdout + negative.stderr:
        raise SystemExit(f"Old TTY boundary negative control did not fail correctly:\n{negative.stdout}{negative.stderr}")

    # Losing the outer guard must fail on the first attempted hardware byte,
    # even though a single-thread byte-equivalence check alone would pass.
    unguarded = output.replace('    #[cfg(target_arch = "aarch64")]\n', "")
    unguarded = unguarded.replace("    let _guard = SerialGuard::acquire();\n", "")
    adapted_output = output.replace('#[cfg(target_arch = "aarch64")]\n', "")
    source.write_text(production.replace(adapted_output, unguarded))
    negative = subprocess.run(
        [str(compile_fixture("unguarded-tty-output")), "--exact", "raw_serial_write_preserves_bytes"],
        capture_output=True, text=True, timeout=15,
    )
    if negative.returncode == 0 or "serial byte emitted without the outer guard" not in negative.stdout + negative.stderr:
        raise SystemExit(f"Missing serial guard negative control did not fail correctly:\n{negative.stdout}{negative.stderr}")

print("MAKOS_AARCH64_TTY_SERIAL_ATOMIC_HOST_OK output=production-code "
      "tty_translation=production-crate byte_equivalence=preserved "
      "record_lock=whole-write contender=kernel-formatted-log "
      "negative_controls=old-chunks,missing-guard adapters=host-mutex,byte-capture")
