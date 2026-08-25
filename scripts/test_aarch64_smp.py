#!/usr/bin/env python3
"""Static invariants for native AArch64 PSCI secondary-CPU bring-up."""

from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
MAIN = (ROOT / "kernel/src/main.rs").read_text()

required = (
    "const PSCI_CPU_ON_64: u64 = 0xc400_0003;",
    '"hvc #0"',
    ".global aarch64_secondary_entry",
    "static mut SECONDARY_STACKS: SecondaryStacks",
    "msr ttbr0_el1, x2",
    "msr sctlr_el1, x5",
    '"msr VBAR_EL1, {vectors}"',
    "SMP_ONLINE_MASK.fetch_or",
    "SMP_TEST_READY_MASK.fetch_or",
    "after[index] <= before[index]",
    "userspace_scheduler_cpus=1 aps_after_test=idle scheduler_gate=closed ap_idle=wfi",
    "static SMP_USER_SCHEDULER_ENABLED: AtomicBool = AtomicBool::new(false);",
    "fn secondary_scheduler_idle() -> !",
    "fn init_secondary_timer_on_current_cpu()",
    "pub(crate) fn enable_smp_probe_scheduler()",
    "GICD_SGIR",
    'unsafe { asm!("wfi", options(nomem, nostack)) };',
)
for token in required:
    assert token in ARCH, f"missing SMP invariant: {token}"

timer = MAIN.index("let timer = arch::init_timer(")
smp = MAIN.index("let smp = arch::init_smp(&platform.interrupt);")
userspace = MAIN.index("aarch64_process::run_init_self_test()")
assert timer < smp < userspace

print(
    "MAKOS_AARCH64_SMP_STATIC_OK psci=cpu_on_64 conduit=hvc "
    "secondary_entry=identity-mapped per_cpu=stack,vbar,logical-id "
    "parallel_el1=counter-growth scheduler_scope=boot-probe ap_idle=wfi desktop_gate=closed"
)
