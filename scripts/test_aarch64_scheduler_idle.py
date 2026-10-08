#!/usr/bin/env python3
"""Check the production AP idle instructions against lost-wake interleavings.

The instruction-level model supplies the documented PSTATE/WFI interrupt
behavior, not a simulated MakOS runtime. A small cross-compiled ELF verifies
the actual production helper encodings without requiring host objdump/clang.
No model result substitutes for Pi/QEMU or Mac/HVF runtime qualification.
"""

from pathlib import Path
import re
import struct
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
FACADE = (ROOT / "kernel/src/arch/mod.rs").read_text()
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()


def item(source: str, declaration: str) -> str:
    start = source.index(declaration)
    opening = source.index("{", start)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


helper = item(ARCH, "pub(crate) fn wait_for_scheduler_interrupt()")
instructions = tuple(re.findall(r'^\s*"([^"\n]+)",$', helper, re.M))
assert instructions == (
    "dsb sy", "wfi", "msr daifclr, #2", "isb", "msr daifset, #0xf",
)
assert "options(nostack, preserves_flags)" in helper
assert "nomem" not in helper and "readonly" not in helper
assert "wait_for_scheduler_interrupt," in FACADE
dispatcher = item(PROCESS, "pub(crate) fn run_secondary_scheduler()")
loop_start = dispatcher.index("loop {")
masked = dispatcher.index("crate::arch::disable_interrupts();", loop_start)
fence = dispatcher.index("core::sync::atomic::compiler_fence(Ordering::SeqCst);", masked)
lookup = dispatcher.index("let selected = with_state(", fence)
idle = dispatcher.index("let Some((pid, group_pid, role, context, surface_priority)) = selected else")
wait = dispatcher.index("crate::arch::wait_for_scheduler_interrupt();", idle)
assert loop_start < masked < fence < lookup < idle < wait
assert dispatcher[lookup:idle].rstrip().endswith("});"), "scheduler lock retained through WFI"
assert "enable_interrupts()" not in dispatcher
assert 'asm!("wfi"' not in dispatcher
assert PROCESS.count("crate::arch::wait_for_scheduler_interrupt();") == 1
# The adversarial case must not depend on a periodic AP timer recovering it.
entry = item(ARCH, "pub(crate) fn enter_user_context(")
assert entry.index("aarch64_enter_user_context(context)") < entry.index("stop_scheduler_timer();")


def execute(sequence: tuple[str, ...], arrival: int | str | None,
            *, spurious: bool = False, delivery: str = "immediate") -> dict[str, object]:
    """One scheduler lookup found no work, followed by one idle attempt.

    Integer arrival points are before that instruction; 'sleep' delivers an
    SGI while WFI is waiting. IRQ delivery can be immediate or postponed until
    the instruction synchronization point. Interrupt handlers only ack; they
    do not redo the scheduler lookup. No timer or unrelated wake is generated.
    """
    masked, pending, ready = True, False, False
    acknowledgments, did_wait = 0, False
    for index, instruction in enumerate(sequence):
        if arrival == index:
            assert not ready
            pending, ready = True, True
        if pending and not masked and (delivery == "immediate" or instruction == "isb"):
            pending = False
            acknowledgments += 1
        if instruction == "wfi":
            did_wait = True
            if arrival == "sleep":
                pending, ready = True, True
            if not pending and not spurious:
                return dict(blocked=True, ready=ready, pending=pending,
                            masked=masked, acknowledgments=acknowledgments, waited=True)
        elif instruction == "msr daifclr, #2":
            masked = False
        elif instruction == "msr daifset, #0xf":
            masked = True
        elif instruction not in ("dsb sy", "isb"):
            raise AssertionError(f"unmodeled production instruction: {instruction}")
    if arrival == len(sequence):
        pending, ready = True, True
    return dict(blocked=False, ready=ready, pending=pending, masked=masked,
                acknowledgments=acknowledgments, waited=did_wait)


cases = 0
# An SGI already pending after the no-work check, between DSB and WFI, or
# arriving during WFI must cause progress even when the local timer is off.
for arrival in (0, instructions.index("wfi"), "sleep"):
    for delivery in ("immediate", "isb"):
        state = execute(instructions, arrival, delivery=delivery)
        assert not state["blocked"] and state["ready"] and state["masked"], state
        assert state["acknowledgments"] == 1 and not state["pending"], state
        cases += 1
# WFI may be a no-op/spurious wake. Every later arrival still reaches the
# next readiness check with IRQs masked; a late pending SGI need not be lost
# just because it was not acknowledged during this brief enable window.
for arrival in range(len(instructions) + 1):
    for delivery in ("immediate", "isb"):
        state = execute(instructions, arrival, spurious=True, delivery=delivery)
        assert not state["blocked"] and state["ready"] and state["masked"], state
        assert state["acknowledgments"] == 1 or state["pending"], state
        cases += 1
quiet = execute(instructions, None)
assert quiet["blocked"] and quiet["masked"] and not quiet["ready"], quiet
spurious = execute(instructions, None, spurious=True)
assert not spurious["blocked"] and spurious["masked"] and not spurious["ready"], spurious
cases += 2

# Reorder only the production WFI past the existing enable/ISB instructions.
# This retains barriers/acknowledgment but recreates the unsafe old order.
old_order = list(instructions)
old_order.remove("wfi")
old_order.insert(old_order.index("msr daifset, #0xf"), "wfi")
arrival = old_order.index("msr daifclr, #2") + 1
lost = execute(tuple(old_order), arrival)
assert lost["blocked"] and lost["ready"] and not lost["pending"], lost
assert lost["acknowledgments"] == 1, "negative control did not consume the sole SGI"
# The literal previous enable/WFI/disable order has the same counterexample.
legacy = ("msr daifclr, #2", "wfi", "msr daifset, #0xf")
lost = execute(legacy, 1)
assert lost["blocked"] and lost["ready"] and not lost["pending"], lost


def elf_text(path: Path) -> bytes:
    data = path.read_bytes()
    header = struct.unpack_from("<16sHHIQQQIHHHHHH", data)
    assert header[0][:6] == b"\x7fELF\x02\x01" and header[2] == 183
    offset, stride, count, names_index = header[6], header[11], header[12], header[13]
    sections = [struct.unpack_from("<IIQQQQIIQQ", data, offset + index * stride)
                for index in range(count)]
    names_section = sections[names_index]
    names = data[names_section[4]:names_section[4] + names_section[5]]
    code = []
    for section in sections:
        end = names.index(0, section[0])
        name = names[section[0]:end].decode()
        if name == ".text.tested_scheduler_idle":
            code.append(data[section[4]:section[4] + section[5]])
    assert len(code) == 1, "production helper code section missing/duplicated"
    return code[0]


with tempfile.TemporaryDirectory(prefix="makos-scheduler-idle-") as temporary:
    directory = Path(temporary)
    source = directory / "idle.rs"
    # Only external symbol visibility/name is adapted. The entire production
    # helper body and inline-assembly options are passed directly to rustc.
    exported = helper.replace(
        "pub(crate) fn wait_for_scheduler_interrupt()",
        'pub extern "C" fn tested_scheduler_idle()', 1,
    )
    source.write_text("#![no_std]\nuse core::arch::asm;\n#[unsafe(no_mangle)]\n" + exported)
    binary = directory / "idle.o"
    assembly = directory / "idle.s"
    subprocess.run([
        "rustc", "--edition=2024", "--crate-type=lib", "--target=aarch64-unknown-none",
        "-C", "opt-level=2", f"--emit=obj={binary},asm={assembly}", str(source),
    ], check=True)
    code = elf_text(binary)
    assert len(code) == 24, f"unexpected helper size: {len(code)}"
    words = struct.unpack("<6I", code)
    assert words == (
        0xD5033F9F,  # DSB SY
        0xD503207F,  # WFI
        0xD50342FF,  # MSR DAIFClr,#2
        0xD5033FDF,  # ISB
        0xD5034FDF,  # MSR DAIFSet,#15
        0xD65F03C0,  # RET
    ), tuple(hex(word) for word in words)

print(f"MAKOS_AARCH64_SCHEDULER_IDLE_HOST_OK sequence=production-asm "
      f"cases={cases} entry=masked-before-lookup sleep=masked-wfi "
      "wake=sgi ack=after-wfi return=masked timer=absent "
      "negative_controls=2 lost-wake=reproduced object=aarch64-elf-exact "
      "runtime=not-claimed")
