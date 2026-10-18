#!/usr/bin/env python3
"""Production I/O probe admission regression, with explicit host-only adapters.

Install alongside the other scripts before running. There is intentionally no
source-directory argument or environment override. Inputs always come from this
repository's kernel and actual process-table/readiness crates. Complete guard,
admission, block, wake and completion bodies are extracted without modification;
the Rust fixture adapts architecture, clock, scheduler selection, serial and WFI.
No real WFI, interrupts, MMIO, guest context switch or device workload executes.
Syscall arm placement and context lifecycle receive structural checks only.
"""
from pathlib import Path
import json
import re
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def item(source: str, start: str) -> str:
    offset = source.index(start)
    opening = source.index("{", offset)
    depth, end = 1, opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[offset:end]


ARMS = (
    ("rights-receive", "        SYS_RECVMSG_RIGHTS =>", ("crate::vfs::read_result_with_rights(",), ("!data_writable", "!rights_writable")),
    ("rights-send", "        SYS_SENDMSG_RIGHTS =>", ("crate::vfs::send_result_with_rights(",), ("!rights_bytes.is_some_and", "!user_range_readable(address, length)")),
    ("local-receive", "        SYS_SOCKET_RECEIVE if crate::vfs::is_pipe_owned(frame.registers[0]) =>", ("crate::vfs::read_result(",), ("!writable", "length == 0")),
    ("local-send", "        SYS_SOCKET_SEND if crate::vfs::is_pipe_owned(frame.registers[0]) =>", ("crate::vfs::write_result(",), ("!user_range_readable(address, length)", "length == 0")),
    ("file-lock", "        SYS_FD_CONTROL if matches!(frame.registers[1], 6..=8) =>", ("crate::vfs::get_file_lock(", "crate::vfs::set_file_lock("), ("!user_range_readable(address, size)",)),
    ("pipe-read", "        SYS_READ if frame.registers[0] >= 3 && crate::vfs::is_pipe_owned(frame.registers[0]) =>", ("crate::vfs::read_result(",), ("!user_range_writable(address, length)",)),
    ("pipe-write", "        SYS_FILE_WRITE if crate::vfs::is_pipe_owned(frame.registers[0]) =>", ("crate::vfs::write_result(",), ("!fault_in_user_read_buffer(address, length)", "!user_range_readable(address, length)")),
    ("network-receive", "        SYS_SOCKET_RECEIVE if crate::aarch64_socket::is_owned(frame.registers[0]) =>", ("crate::aarch64_socket::receive_from(",), ("!source_capacity_ok", "!data_writable")),
    ("network-read", "        SYS_READ if crate::aarch64_socket::is_owned(frame.registers[0]) =>", ("crate::aarch64_socket::receive(",), ("!user_range_writable(address, length)", "length == 0")),
    ("poll", "        SYS_POLL =>", ("poll_descriptors(",), ("count > 32", "timeout < -1", "!user_range_writable(address, bytes)")),
    ("epoll", "        SYS_EPOLL_WAIT =>", ("crate::aarch64_epoll::collect(",), ("maximum > 32", "timeout < -1", "!user_range_writable(address, bytes)")),
    ("pselect", "fn pselect(", ("select_snapshot(",), ("!select_set_range_valid", "timeout < -1 || wait_mask_present > 1")),
)
ARM = re.compile(r"let mut io_probe\s*=\s*crate::aarch64_process::begin_io_probe\([^;]+;")
BLOCK = re.compile(r"crate::aarch64_process::block_current_for_io(?:_on)?\(")
HANDOFF = re.compile(r"io_probe\.handoff\(\);\s*match " + BLOCK.pattern)


def verify_arms(architecture: str) -> None:
    assert len(ARM.findall(architecture)) == 12, "exact twelve pre-operation arms"
    assert len(BLOCK.findall(architecture)) == 12, "every I/O registration has a reviewed caller"
    assert len(HANDOFF.findall(architecture)) == 12, "every registration immediately follows handoff"
    for name, start, operations, validation in ARMS:
        branch = item(architecture, start)
        guards = list(ARM.finditer(branch))
        assert len(guards) == 1, name + ": exactly one scoped guard"
        arm = guards[0]
        for operation in operations:
            assert arm.end() < branch.index(operation), name + ": readiness operation preceded its arm"
        for token in validation:
            assert branch.index(token) < arm.start(), name + ": arm must follow validated arguments"
        assert branch.index("frame.registers[0] = (-22i64) as u64;") < arm.start(), \
            name + ": validation error path precedes arm"
        handoffs = list(HANDOFF.finditer(branch))
        assert len(handoffs) == 1 and handoffs[0].start() > arm.end(), \
            name + ": one ownership handoff before registration"
        if name in ("poll", "epoll", "pselect"):
            assert arm.end() < branch.index("crate::aarch64_tty::begin_wait_mask("), \
                name + ": admission precedes mask and interruption observation"
        if "block_current_for_io_on" in branch:
            assert "begin_io_probe(source)" in arm.group()
            assert "block_current_for_io_on(-1, source, frame)" in branch
        else:
            assert "begin_io_probe(makos_readiness::WaitSource::Any)" in arm.group()
        if name == "network-receive":
            endpoint = item(branch, "if let Err(error) =")
            assert "write_socket_endpoint(" in endpoint
            assert "drop(io_probe);" in endpoint, "network-receive: endpoint error must drop before signal delivery"
            assert endpoint.index("frame.registers[0] = error as u64;") < endpoint.index("drop(io_probe);") \
                < endpoint.index("finish_signal_delivery(frame);") < endpoint.index("return;"), \
                "network-receive: endpoint error must drop before signal delivery"


def verify_lifecycle(process: str) -> None:
    empty = item(process, "impl ContextSlot {")
    assert "io_probe_source: None," in empty and "io_probe_notified: false," in empty
    literals = list(re.finditer(r"(?:\*slot|state\.contexts\[index\]) = ContextSlot \{", process))
    assert len(literals) == 3, "all production spawn/thread context constructors reviewed"
    for match in literals:
        constructor = item(process[match.start():], match.group())
        assert "io_probe_source: None," in constructor
        assert "io_probe_notified: false," in constructor
    replacement = item(process, "pub(crate) fn exec_current(")
    assert "state.contexts[index].io_probe_source = None;" in replacement
    assert "state.contexts[index].io_probe_notified = false;" in replacement
    for start in ("fn discard_spawned(", "pub fn wait(",
                  "pub(crate) fn exit_group_from_exception(", "fn exit_thread_from_exception("):
        assert "*slot = ContextSlot::EMPTY;" in item(process, start), \
            "reap resets probe through full EMPTY context: " + start


DECLARATIONS = (
    "#[derive(Clone, Copy, Eq, PartialEq)]\npub enum ProcessRole",
    "#[derive(Clone, Copy)]\nstruct ContextSlot",
    "impl ContextSlot",
    "#[derive(Clone, Copy, Eq, PartialEq)]\npub(crate) enum IoBlockResult",
    "#[derive(Clone, Copy)]\npub struct RuntimeStats",
    "pub fn runtime_stats(",
    "fn secondary_scheduler_can_idle(",
    '#[must_use = "keep the probe alive through the operation, then drop it or hand it to registration"]\npub(crate) struct IoProbeGuard',
    "impl IoProbeGuard",
    "impl Drop for IoProbeGuard",
    "pub(crate) fn begin_io_probe(",
    "fn consume_io_probe(",
    "pub(crate) fn block_current_for_io(",
    "pub(crate) fn block_current_for_io_on(",
    "pub(crate) fn complete_io_wait(",
    "pub(crate) fn wake_io_source(",
    "pub(crate) fn wake_io_waiters(",
)


def verify_critical_sections(production: str) -> None:
    for match in re.finditer(r"with_state\(\|state\| \{", production):
        critical = item(production[match.start():], match.group())
        critical = re.sub(r"//[^\n]*", "", critical)
        assert not any(token in critical for token in (
            "crate::vfs::", "crate::aarch64_socket::", "crate::aarch64_tty::",
            "crate::aarch64_epoll::", "crate::arch::service_", "asm!(",
            "core::ptr::", "core::slice::from_raw_parts")), \
            "device/user-buffer operations must remain outside scheduler critical sections"
    block = item(production, "pub(crate) fn block_current_for_io_on(")
    assert block.count("consume_io_probe(&mut state.contexts[index], source)") == 2
    assert block.index("consume_io_probe(") < block.index("state.table.block_current_on(")
    assert block.count('unsafe { asm!("wfi", options(nomem, nostack)) };') == 1, \
        "only supported WFI adapter site"
    consume = item(production, "fn consume_io_probe(")
    assert "(observed_source == Some(source)).then_some(notified)" in consume


def run(command, *, timeout=60):
    start = time.monotonic()
    print("IO_PROBE_COMMAND", json.dumps([str(value) for value in command]), flush=True)
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        raise SystemExit(f"I/O probe regression timeout: {command!r}\n"
                         f"stdout={error.stdout!r}\nstderr={error.stderr!r}") from error
    print("IO_PROBE_COMMAND_EXIT", result.returncode,
          "elapsed_seconds=" + str(time.monotonic() - start), flush=True)
    if result.stdout:
        print("IO_PROBE_COMMAND_STDOUT", result.stdout, end="" if result.stdout.endswith("\n") else "\n")
    if result.stderr:
        print("IO_PROBE_COMMAND_STDERR", result.stderr, end="" if result.stderr.endswith("\n") else "\n")
    return result


def require_success(command, *, timeout=60):
    result = run(command, timeout=timeout)
    if result.returncode:
        raise SystemExit(f"I/O probe regression failed: {command!r}\n"
                         f"{result.stdout}{result.stderr}")
    return result


def main():
    process = (ROOT / "kernel/src/aarch64_process.rs").read_text()
    architecture = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
    verify_arms(architecture)
    verify_lifecycle(process)
    production = "\n\n".join(item(process, start) for start in DECLARATIONS) + "\n"
    verify_critical_sections(production)
    # Move the pipe read before arm, retaining a well-scoped guard around its
    # result match. This reproduces the forbidden observation/admission gap.
    pipe = item(architecture, ARMS[5][1])
    guard = ARM.search(pipe).group()
    observed = "match crate::vfs::read_result(frame.registers[0], output) {"
    assert pipe.count(observed) == 1
    late = pipe.replace(guard, "", 1).replace(observed,
        "let observed = crate::vfs::read_result(frame.registers[0], output);\n"
        + guard + "\n                match observed {", 1)
    try:
        verify_arms(architecture.replace(pipe, late, 1))
    except AssertionError as error:
        assert "pipe-read: readiness operation preceded its arm" in str(error)
    else:
        raise AssertionError("arm-after-observation negative control was accepted")

    receive = item(architecture, ARMS[7][1])
    endpoint = item(receive, "if let Err(error) =")
    assert endpoint.count("drop(io_probe);") == 1
    retained_guard = receive.replace(endpoint, endpoint.replace("drop(io_probe);", "", 1), 1)
    try:
        verify_arms(architecture.replace(receive, retained_guard, 1))
    except AssertionError as error:
        assert str(error) == "network-receive: endpoint error must drop before signal delivery"
    else:
        raise AssertionError("guard surviving endpoint-error signal delivery was accepted")
    print("IO_PROBE_STRUCTURAL_NEGATIVE_CONTROLS_OK arm-after-observation,endpoint-error-guard-cleanup")

    fixture = (ROOT / "scripts/test_aarch64_io_probe_atomic.rs").read_text()
    with tempfile.TemporaryDirectory(prefix="makos-io-probe-atomic-") as temporary:
        directory = Path(temporary)
        libraries = []
        for crate, folder in (("makos_process_table", "process-table"),
                              ("makos_readiness", "readiness")):
            library = directory / ("lib" + crate + ".rlib")
            require_success(["rustc", "--edition=2024", "--deny=warnings", "--crate-name", crate,
                             "--crate-type", "rlib", str(ROOT / "crates" / folder / "src/lib.rs"),
                             "-o", str(library)])
            libraries.extend(("--extern", f"{crate}={library}"))
        extracted = directory / "production.rs"
        (directory / "test.rs").write_text(fixture)

        def compile_fixture(name, source):
            extracted.write_text(source)
            binary = directory / name
            require_success(["rustc", "--edition=2024", "--deny=warnings", "--test", str(directory / "test.rs"),
                             *libraries, "-o", str(binary)])
            return binary

        baseline = compile_fixture("io-probe-atomic", production)
        success = require_success([str(baseline), "--test-threads=1"], timeout=20)
        print(success.stdout, end="")
        controls = (
            ("missing-running-wake-latch", production.replace(
                "state.contexts[index].io_probe_notified = true;",
                "state.contexts[index].io_probe_notified = false;", 1),
             "gap_wake_latches_while_running", "gap wake must latch while Running"),
            ("ignored-notification", production.replace("if notified {", "if notified && false {"),
             "gap_notification_skips_wait", "gap notification must avoid WFI"),
            ("stale-guard-cleanup", production.replace("self.tid = None;", "let _ = &self.tid;", 1),
             "handoff_cannot_clear_rearmed_same_tid", "old handed-off guard cleared new probe"),
            ("deadline-reset", production.replace("if !state.contexts[index].io_wait {", "if true {"),
             "relative_deadline_survives_reprobe_and_expires", "reprobe reset the original deadline"),
        )
        for name, mutant, test, message in controls:
            assert mutant != production, "negative control must change production: " + name
            binary = compile_fixture(name, mutant)
            negative = run([str(binary), "--exact", test, "--test-threads=1", "--nocapture"], timeout=10)
            # Rust's test harness returns 101 for an assertion panic. A signal,
            # abort, launcher error or unrelated panic is not a passing control.
            lines = {line.strip() for line in (negative.stdout + negative.stderr).splitlines()}
            expected_assertions = {message, f"assertion `left == right` failed: {message}"}
            if negative.returncode != 101 or not lines.intersection(expected_assertions):
                raise SystemExit(f"I/O probe negative control did not expose {name}: "
                                 f"exit_status={negative.returncode}, expected=101\n"
                                 f"{negative.stdout}{negative.stderr}")
            print("IO_PROBE_NEGATIVE_CONTROL_OK", name, "exit_status=101", "assertion=" + message)

    print("MAKOS_AARCH64_IO_PROBE_ATOMIC_HOST_OK",
          "cases=" + str(fixture.count("\n#[test]\n")),
          "negative_controls=6 bodies=production-complete guard,begin,consume,block,wake,complete",
          "native_negative_exit=101 compiler_warnings=denied",
          "process-table,readiness=production-crates arms=12-structural lifecycle=structural",
          "clock,frame,WFI,dispatch-selection,serial=host-adapters",
          "guest=not-run interrupts,device-progress,Firefox=not-qualified")


if __name__ == "__main__":
    main()
