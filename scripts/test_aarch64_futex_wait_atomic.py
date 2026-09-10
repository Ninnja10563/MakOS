#!/usr/bin/env python3
"""Exercise the production futex read/enqueue critical section against a wake.

The kernel's scheduler-closure prefix through FutexTable::wait and its errno
mapping is used unchanged. A deterministic pre-lock scheduling hook supplies
the adversarial clear/wake ordering; the queue implementation is makos-futex.
This does not emulate interrupts or execute the guest scheduler's context swap.
"""

from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()


def item(source: str, start: str) -> str:
    offset = source.index(start)
    opening = source.index("{", offset)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[offset:end]


futex = item(PROCESS, "pub fn futex(")
wait_path = futex[futex.index("if command != FUTEX_WAIT {"):]
opening = "let result = with_state(|state| {"
before, registration_and_rest = wait_path.split(opening, 1)
registration = registration_and_rest.split("state.contexts[index].context = captured;", 1)[0]
read = "let observed = unsafe { core::ptr::read_volatile(address as *const u32) };"
assert read not in before, "futex observation moved outside scheduler lock"
assert registration.count(read) == 1
assert registration.index(read) < registration.index("state.futex.wait(")
assert "with_state" not in registration
assert not re.sub(r"//[^\n]*", "", registration[registration.index(read) + len(read):
                                                       registration.index("let handle = match")]).strip()
assert "state.table.block_current_on(scheduler_cpu())" in registration_and_rest
assert "wake_futex_in_state(state, FutexKey::new(root, address), value as usize)" in futex
constants = "\n".join(re.findall(r"    const (?:EAGAIN|EINVAL|ETIMEDOUT):.*;$", futex, re.M))
errno = item(PROCESS, "const fn negative_errno(")
function_head = """
fn register_wait(address: u64, value: u32, now: u64, deadline: Option<u64>)
    -> Result<WaitHandle, u64> {
""" + constants + "\n"
tail = "\n        Ok(handle)\n    })\n}\n"
production = errno + function_head + "    with_state(|state| {\n" + registration + tail

with tempfile.TemporaryDirectory(prefix="makos-futex-wait-atomic-") as temporary:
    directory = Path(temporary)
    library = directory / "libmakos_futex.rlib"
    subprocess.run(["rustc", "--edition=2024", "--crate-name", "makos_futex", "--crate-type", "rlib",
                    str(ROOT / "crates/futex/src/lib.rs"), "-o", str(library)], check=True)
    source = directory / "registration.rs"
    source.write_text(production)
    (directory / "test.rs").write_text((ROOT / "scripts/test_aarch64_futex_wait_atomic.rs").read_text())

    def compile_fixture(name: str) -> Path:
        binary = directory / name
        subprocess.run(["rustc", "--edition=2024", "--test", str(directory / "test.rs"),
                        "--extern", f"makos_futex={library}", "-o", str(binary)], check=True)
        return binary

    subprocess.run([str(compile_fixture("futex-wait-atomic")), "--test-threads=1"],
                   check=True, timeout=20)
    # Move just the real volatile read before lock acquisition, recreating the
    # original ordering without altering the actual queue or value predicate.
    source.write_text(errno + function_head + read + "\n    with_state(|state| {\n"
                      + registration.replace(read, "") + tail)
    negative = subprocess.run([str(compile_fixture("futex-wait-before-lock")), "--exact",
                               "clear_and_empty_wake_before_lock_rejects_stale_wait"],
                              capture_output=True, text=True, timeout=10)
    if negative.returncode == 0 or "lost wake: stale futex sample queued after clear" not in negative.stdout + negative.stderr:
        raise SystemExit(f"pre-lock observation negative control did not expose lost wake:\n{negative.stdout}{negative.stderr}")

print("MAKOS_AARCH64_FUTEX_WAIT_ATOMIC_HOST_OK registration=production-code queue=makos-futex "
      "interleaving=clear-wake-before-lock result=EAGAIN read,enqueue=same-lock "
      "post-enqueue=woken roots=isolated errno=preserved negative_control=stale-wait-reproduced")
