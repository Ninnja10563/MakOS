#!/usr/bin/env python3
"""Exercise the production dispatch-epoch policy and timer-spanning workload."""

from pathlib import Path
import os
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
PROBE = (ROOT / "ports/musl/pthread-probe.c").read_text()

# Lifecycle wiring: each clone snapshots all AP totals under the scheduler lock;
# leaders/forks/empty slots carry no inherited epoch. Explicit affinity remains
# authoritative and migration still happens only in the existing timer branch.
assert "automatic_load_baseline: application_placement.map_or([0; 3], |placement| placement.loads)" in PROCESS
assert PROCESS.count("automatic_load_baseline: [0; 3]") == 3
policy = PROCESS.split("fn rebalance_application_on_timer(", 1)[1].split("#[derive", 1)[0]
for guard in ("!production_ap_worker(&slot)", "slot.affinity_user_set", "slot.automatic_migrated", "slot.automatic_cpu != cpu as u8", "slot.affinity_mask != 0xe"):
    assert guard in policy, guard
assert "slot.automatic_load_baseline" in policy
assert "crate::aarch64_application_balance::migration_target(" in policy
assert "let migration = if timer {" in PROCESS
assert "const APPLICATION_REBALANCE_DISPATCH_DELTA: u64 = 64;" in PROCESS
assert "slot.affinity_user_set = true;" in PROCESS
worker = PROBE.split("static void *production_smp_worker(", 1)[1].split("static void *production_input_watcher(", 1)[0]
assert worker.index("production_auto_load();") < worker.index("production_auto_release, 1") < worker.index("sched_setaffinity(")

rustc = shutil.which("rustc")
assert rustc, "rustc is required for the production balancing behavior test"
clang = os.environ.get("MAKOS_TEST_CLANG") or shutil.which("clang") or shutil.which("cc")
if not clang:
    bases = [ROOT]
    if ROOT.parent.name == "build":
        bases.append(ROOT.parent.parent)
    clang = next((str(base / "build/host-tools/llvm19/usr/bin/clang-19") for base in bases
                  if (base / "build/host-tools/llvm19/usr/bin/clang-19").exists()), None)
assert clang, "a host C compiler is required for the production workload behavior test"

load = "static void production_auto_load(void)" + PROBE.split("static void production_auto_load(void)", 1)[1].split("static void *production_smp_worker(", 1)[0]
harness = r"""
#include <assert.h>
#include <stdint.h>
static volatile uint64_t production_auto_checksum;
static unsigned yields, clocks;
static uint64_t clock_start;
static long makos_call(long number, long first, long second) {
    assert(first == 0 && second == 0);
    if (number == 1) { assert(clocks == 0); yields++; return 0; }
    assert(number == 27 && yields == 4096);
    // All dispatches finished with no elapsed clock tick, as on a fast HVF.
    // Advance the fake monotonic clock only after entering the compute phase.
    if (clocks) assert(production_auto_checksum != 0);
    return (long)(clock_start + clocks++ / 4);
}
""" + load + r"""
int main(void) {
    for (unsigned iteration = 0; iteration < 2; iteration++) {
        clock_start = iteration ? UINT64_MAX - 2 : 100;
        yields = clocks = 0;
        production_auto_checksum = 0;
        production_auto_load();
        assert(yields == 4096);
        assert(clocks == 21);
        assert(production_auto_checksum != 0);
    }
    return 0;
}
"""

with tempfile.TemporaryDirectory(prefix="makos-application-balance-") as temporary:
    directory = Path(temporary)
    rust_tests = directory / "policy-tests"
    subprocess.run([rustc, "--edition=2024", "--test", str(ROOT / "kernel/src/aarch64_application_balance.rs"), "-o", str(rust_tests)], check=True)
    subprocess.run([str(rust_tests)], check=True)
    c_file = directory / "workload.c"
    c_file.write_text(harness)
    workload = directory / "workload"
    subprocess.run([clang, "-std=c17", "-O2", "-Wall", "-Wextra", "-Werror", str(c_file), "-o", str(workload)], check=True)
    subprocess.run([str(workload)], check=True)

print("AArch64 application balancing test passed policy=thread-dispatch-epoch delta=64 workload=4096-yields,el0-timer-work affinity=authoritative")
