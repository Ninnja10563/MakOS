#!/usr/bin/env python3
"""Run production failure diagnostic with host identity/clock/serial adapters.

No guest, disk, pointer dereference, or browser repair is simulated. Each case
runs in a fresh process; production saturated counters are never reset.
"""
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = (ROOT / "kernel/src/aarch64_file_failure_trace.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
MAIN = (ROOT / "kernel/src/main.rs").read_text()


def body(source: str, signature: str) -> str:
    start = source.index("{", source.index(signature))
    depth = 0
    for index in range(start, len(source)):
        depth += (source[index] == "{") - (source[index] == "}")
        if depth == 0:
            return source[start + 1:index]
    raise AssertionError(f"unterminated function: {signature}")


assert re.search(r'#\[cfg\(target_arch = "aarch64"\)\]\s*mod aarch64_file_failure_trace;', MAIN)
svc = body(ARCH, "fn handle_svc(")
capture = re.search(
    r"let file_trace_args = \[\s*"
    + "".join(r"frame\.registers\[" + str(index) + r"\],\s*" for index in range(6))
    + r"\];\s*frame\.registers\[0\] = match frame\.registers\[8\] \{", svc)
assert capture, "six raw entry arguments must be copied immediately before normal dispatch"
dispatch = body(svc[capture.start():], "frame.registers[0] = match frame.registers[8]")
hook = re.search(
    r"crate::aarch64_file_failure_trace::record\(\s*frame\.registers\[8\],\s*"
    r"file_trace_args,\s*frame\.registers\[0\],\s*frame\.ttbr0,\s*\);", svc)
assert hook and svc.count("aarch64_file_failure_trace::record(") == 1
assert "aarch64_file_failure_trace" not in dispatch
assert re.fullmatch(r"\s*;\s*", svc[capture.end() + len(dispatch) + 1:hook.start()]), (
    "failure hook must immediately follow completed normal match after handler guards drop"
)
assert re.fullmatch(r"\s*finish_signal_delivery\(frame\);\s*", svc[hook.end():]), (
    "failure hook must precede final signal-frame mutation"
)
assert "SYS_STAT_EXTENDED" in dispatch and "SYS_FSTAT_EXTENDED" in dispatch
clock = body(ARCH, "pub fn uptime_millis()")
assert "read_virtual_counter()" in clock and "TIMER_TICKS" not in clock

HOST = r'''
use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};
static ROLE: AtomicU64 = AtomicU64::new(1);
static ROLE_CALLS: AtomicU64 = AtomicU64::new(0);
static TID_CALLS: AtomicU64 = AtomicU64::new(0);
static CLOCK_CALLS: AtomicU64 = AtomicU64::new(0);
static CPU_CALLS: AtomicU64 = AtomicU64::new(0);
static SERIAL: Mutex<Vec<String>> = Mutex::new(Vec::new());
mod arch {
    pub fn uptime_millis() -> u64 {
        super::CLOCK_CALLS.fetch_add(1, super::Ordering::Relaxed); 123456
    }
    pub fn cpu_index() -> usize {
        super::CPU_CALLS.fetch_add(1, super::Ordering::Relaxed); 3
    }
}
mod aarch64_process {
    #[derive(PartialEq)]
    pub enum ProcessRole { Firefox, Native }
    pub fn current_app_role() -> ProcessRole {
        super::ROLE_CALLS.fetch_add(1, super::Ordering::Relaxed);
        if super::ROLE.load(super::Ordering::Relaxed) == 1 { ProcessRole::Firefox }
        else { ProcessRole::Native }
    }
    pub fn current_tid() -> u64 {
        super::TID_CALLS.fetch_add(1, super::Ordering::Relaxed); 73
    }
}
#[macro_export]
macro_rules! serial_println {
    ($($value:tt)*) => {{ crate::SERIAL.lock().unwrap().push(format!($($value)*)); }};
}
mod trace { include!("production.rs"); }
const OPS: [u64;14] = [11,12,17,36,53,87,89,92,96,97,100,101,132,133];
const ARGS: [u64;6] = [u64::MAX,0,0x8000000000000000,0xabc,0x12345678,0xffee];
fn emit(op: u64) { trace::record(op, ARGS, (-22i64) as u64, 0x4012e000); }
fn queries() -> [u64;4] {
    [&ROLE_CALLS,&TID_CALLS,&CLOCK_CALLS,&CPU_CALLS].map(|x| x.load(Ordering::Relaxed))
}
fn lines(prefix: &str) -> Vec<String> {
    SERIAL.lock().unwrap().iter().filter(|s| s.starts_with(prefix)).cloned().collect()
}
fn failures() -> Vec<String> { lines("MAKOS_AARCH64_FILE_SYSCALL_FAILURE ") }
fn overflow() -> Vec<String> { lines("MAKOS_AARCH64_FILE_SYSCALL_FAILURE_OVERFLOW ") }
fn fields(line: &str, required: &[&str]) {
    for field in required {
        assert!(line.split_whitespace().any(|part| part == *field), "missing identity field {field}: {line}");
    }
}
fn main() {
    match std::env::args().nth(1).unwrap().as_str() {
        "fast-success-unknown" => {
            for op in OPS {
                for result in [0,1,i64::MAX as u64] { trace::record(op, ARGS, result, 0); }
            }
            for op in [0,10,13,148,u64::MAX] { emit(op); }
            assert_eq!(queries(), [0;4], "success and unknown must not query role or identity");
            assert!(SERIAL.lock().unwrap().is_empty());
        }
        "role-gate" => {
            ROLE.store(0, Ordering::Relaxed);
            for op in OPS { for _ in 0..8 { emit(op); } }
            assert_eq!(queries(), [112,0,0,0]);
            assert!(SERIAL.lock().unwrap().is_empty(), "non-Firefox must be ignored");
            ROLE.store(1, Ordering::Relaxed);
            for op in OPS { for _ in 0..4 { emit(op); } }
            assert_eq!(failures().len(), 56, "other roles must not spend Firefox budget");
            assert!(overflow().is_empty());
        }
        "raw-identity" => {
            emit(132);
            let records = failures(); assert_eq!(records.len(),1);
            fields(&records[0], &["op=132", "result=-22", "arg0=0xffffffffffffffff",
                "arg1=0x0", "arg2=0x8000000000000000", "arg3=0xabc",
                "arg4=0x12345678", "arg5=0xffee", "cpu=3", "tid=73",
                "root=0x4012e000", "time_ms=123456", "clock=architectural-counter",
                "args=raw-entry-registers", "scope=normal-dispatch"]);
            assert_eq!(queries(), [1,1,1,1]);
            trace::record(133, ARGS, i64::MIN as u64, 0);
            fields(&failures()[1], &["op=133", "result=-9223372036854775808", "root=0x0"]);
        }
        "per-op-budget" => {
            for op in OPS {
                for _ in 0..40 { emit(op); }
                let records: Vec<_> = failures().into_iter().filter(|s| s.split_whitespace().any(|v| v == format!("op={op}"))).collect();
                assert_eq!(records.len(),4,"per-operation failure budget must be four");
            }
            assert_eq!(failures().len(),56);
            let dropped=overflow(); assert_eq!(dropped.len(),14,"one first-overflow notice per operation");
            for record in dropped {
                fields(&record,&["cap=4","dropped=1","count_kind=at-first-overflow",
                    "total_later_drops=not-reported","scope=normal-dispatch"]);
            }
            assert_eq!(queries(),[70,56,56,56],"exhausted fast path must not lookup identity");
            let before=queries();
            for _ in 0..100 { for op in OPS { emit(op); } }
            assert_eq!(queries(),before,"saturated path must remain query-free");
        }
        "concurrent" => {
            let barrier=std::sync::Arc::new(std::sync::Barrier::new(17));
            let workers: Vec<_>=(0..16).map(|_| {
                let barrier=barrier.clone();
                std::thread::spawn(move || { barrier.wait(); for _ in 0..20 { emit(17); } })
            }).collect();
            barrier.wait(); for worker in workers { worker.join().unwrap(); }
            assert_eq!(failures().len(),4,"concurrent admission must cap failure records at four");
            assert_eq!(overflow().len(),1,"concurrent overflow notice must be once-only");
            assert_eq!(TID_CALLS.load(Ordering::Relaxed),4);
            assert_eq!(CLOCK_CALLS.load(Ordering::Relaxed),4);
            let before=queries(); emit(17); assert_eq!(queries(),before);
            emit(36); assert_eq!(failures().len(),5,"other operation retains independent budget");
        }
        _ => panic!("unknown case"),
    }
}
'''

assert "monotonic_ticks" not in SOURCE
assert "unsafe" not in SOURCE
assert "crate::vfs" not in SOURCE and "crate::block" not in SOURCE

with tempfile.TemporaryDirectory(prefix="makos-file-failure-trace-") as temporary:
    directory = Path(temporary)
    fixture = directory / "host.rs"
    fixture.write_text(HOST)

    def compile_fixture(name: str, source: str) -> Path:
        (directory / "production.rs").write_text(re.sub(r"(?m)^//!", "//", source))
        binary = directory / name
        subprocess.run(["rustc", "--edition=2024", "-Dwarnings", str(fixture),
                        "-o", str(binary)], check=True)
        return binary

    binary = compile_fixture("file-failure", SOURCE)
    for case in ("fast-success-unknown", "role-gate", "raw-identity", "per-op-budget", "concurrent"):
        subprocess.run([str(binary), case], check=True, timeout=10)

    def reject(name: str, mutated: str, case: str, diagnostic: str) -> None:
        assert mutated != SOURCE, f"{name}: mutation did not change source"
        result = subprocess.run([str(compile_fixture(name, mutated)), case],
                                capture_output=True, text=True, timeout=10)
        assert result.returncode != 0 and diagnostic in result.stdout + result.stderr, (
            f"{name}: behavioral rejection missing: {result.stdout}{result.stderr}"
        )

    reject("ungated-success", SOURCE.replace("if (result as i64) >= 0 {", "if false && (result as i64) >= 0 {"),
           "fast-success-unknown", "success and unknown must not query role or identity")
    reject("uncapped", SOURCE.replace("const RECORD_LIMIT: u64 = 4;", "const RECORD_LIMIT: u64 = 1000;"),
           "per-op-budget", "per-operation failure budget must be four")
    reject("missing-arg", SOURCE.replace("arg5={:#x}", "omitted_arg5={:#x}"),
           "raw-identity", "missing identity field arg5=0xffee")
    reject("missing-result", SOURCE.replace("result={} arg0", "omitted_result={} arg0"),
           "raw-identity", "missing identity field result=-22")

print("MAKOS_AARCH64_FILE_FAILURE_TRACE_HOST_OK logic=production-module "
      "identity,clock,serial=host-adapters ops=14 failure_cap=4 overflow=once-per-op "
      "cases=5 negative_controls=ungated-success,uncapped,missing-arg,missing-result "
      "scope=bounded-diagnostics-not-firefox-repair-or-runtime-pass")
