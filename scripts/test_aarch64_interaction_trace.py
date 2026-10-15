#!/usr/bin/env python3
"""Exercise real bounded diagnostic logic, not guest latency or Firefox repair.

Only the architectural clock, CPU/TID lookup and serial sink are host adapters.
Each case runs in a fresh process so production one-shot state is never reset.
"""

from pathlib import Path
import re
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
SOURCE = (ROOT / "kernel/src/aarch64_interaction_trace.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
MAIN = (ROOT / "kernel/src/main.rs").read_text()


def body(source: str, signature: str) -> str:
    start = source.index("{", source.index(signature))
    depth = 0
    for offset in range(start, len(source)):
        depth += (source[offset] == "{") - (source[offset] == "}")
        if depth == 0:
            return source[start + 1:offset]
    raise AssertionError(f"unterminated item: {signature}")


def check_clock(source: str) -> None:
    assert "crate::arch::uptime_millis()" in source, "architectural counter clock required"
    assert "monotonic_ticks" not in source, "serviced IRQ ticks cannot measure blocked CPU time"
    clock = body(ARCH, "pub fn uptime_millis()")
    assert "read_virtual_counter()" in clock
    assert "TIMER_TICKS" not in clock


check_clock(SOURCE)
assert "mod aarch64_interaction_trace;" in MAIN
prioritize = body(PROCESS, "pub(crate) fn prioritize_firefox_surface_thread(")
assert re.search(r"if tid != 0 \{\s*crate::aarch64_interaction_trace::arm\(\);\s*\}", prioritize)
assert prioritize.index("crate::aarch64_interaction_trace::arm();") < prioritize.index("set_surface_priority(tid)")
svc = body(ARCH, "fn handle_svc(")
svc_statements = re.sub(r"//[^\n]*", "", svc).lstrip()
assert svc_statements.startswith("let _interaction_span ="), "whole-SVC span must begin at entry"
assert "crate::aarch64_interaction_trace::syscall(frame.registers[8], frame.ttbr0);" in svc
assert svc.count("aarch64_interaction_trace::syscall(") == 1
irq = body(ARCH, "fn handle_irq(")
assert "let interaction_root = frame.ttbr0;" in irq
assert "aarch64_interaction_trace::syscall(" not in irq
for category, call in (
    ("timer-block-service", "crate::aarch64_virtio_blk::service_requests_from_timer();"),
    ("timer-net-tx-service", "crate::aarch64_virtio_net::service_tx_requests_from_timer();"),
    ("timer-net-rx-service", "service_network_rx_on_owner_cpu();"),
    ("timer-input-recovery", "service_input_on_owner_cpu();"),
    ("timer-preempt", "crate::aarch64_process::preempt_from_timer(frame);"),
):
    expected = (
        r'\{\s*let _interaction_span = crate::aarch64_interaction_trace::span\(\s*'
        + re.escape(f'"{category}", 0, 0, interaction_root,')
        + r'\s*\);\s*' + re.escape(call) + r'\s*\}'
    )
    assert re.search(expected, irq), f"{category} must cover exactly its scoped IRQ call, tid=0"

HOST = r'''
use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};
static CLOCK: AtomicU64 = AtomicU64::new(100);
static CLOCK_READS: AtomicU64 = AtomicU64::new(0);
static CPU: AtomicU64 = AtomicU64::new(2);
static TID_CALLS: AtomicU64 = AtomicU64::new(0);
static SERIAL: Mutex<Vec<String>> = Mutex::new(Vec::new());
mod arch {
    pub fn uptime_millis() -> u64 {
        super::CLOCK_READS.fetch_add(1, super::Ordering::AcqRel);
        super::CLOCK.load(super::Ordering::Acquire)
    }
    #[allow(dead_code)] // Only the rejected serviced-tick clock mutation uses this adapter.
    pub fn monotonic_ticks() -> u64 { 0 }
    pub fn cpu_index() -> usize { super::CPU.load(super::Ordering::Acquire) as usize }
}
mod aarch64_process {
    pub fn current_tid() -> u64 {
        super::TID_CALLS.fetch_add(1, super::Ordering::AcqRel);
        37
    }
}
#[macro_export]
macro_rules! serial_println {
    ($($value:tt)*) => {{ crate::SERIAL.lock().unwrap().push(format!($($value)*)); }};
}
mod trace {
    include!("production.rs");
}
fn now(value: u64) { CLOCK.store(value, Ordering::Release); }
fn lines(marker: &str) -> Vec<String> {
    SERIAL.lock().unwrap().iter().filter(|line| line.starts_with(marker)).cloned().collect()
}
fn spans() -> Vec<String> { lines("MAKOS_AARCH64_INTERACTION_SPAN") }
fn contains_fields(line: &str, fields: &[&str]) {
    for field in fields { assert!(line.split_whitespace().any(|word| word == *field),
        "missing field {field} in {line}"); }
}
fn span_at(start: u64, end: u64) {
    now(start);
    let span = trace::span("irq-test", 73, 0, 0x123000);
    now(end);
    drop(span);
}
fn main() {
    match std::env::args().nth(1).unwrap().as_str() {
        "disabled" => {
            let span = trace::syscall(148, 0xabc000);
            now(500);
            drop(span);
            assert!(SERIAL.lock().unwrap().is_empty());
            assert_eq!(TID_CALLS.load(Ordering::Acquire), 0,
                "disabled diagnostic must not lookup scheduler TID");
        }
        "threshold-context" => {
            trace::arm();
            let armed = lines("MAKOS_AARCH64_INTERACTION_TRACE_ARM");
            assert_eq!(armed.len(), 1);
            contains_fields(&armed[0], &["window_ms=30000", "min_span_ms=10", "cap=64",
                "clock=architectural-counter", "inclusive=1", "tid_zero=unavailable", "rearm=0"]);
            span_at(100, 109);
            assert!(spans().is_empty(), "nine milliseconds must be filtered");
            span_at(109, 119);
            let records = spans();
            assert_eq!(records.len(), 1, "ten milliseconds must be recorded");
            contains_fields(&records[0], &["category=irq-test", "operation=73", "cpu=2",
                "tid=0", "tid_source=unavailable", "root=0x123000", "start_ms=109",
                "end_ms=119", "elapsed_ms=10", "inclusive=1", "clock=architectural-counter"]);
            assert_eq!(TID_CALLS.load(Ordering::Acquire), 0,
                "IRQ span must never lookup scheduler TID");
        }
        "syscall" => {
            trace::arm();
            let span = trace::syscall(148, 0xabc000);
            now(111);
            CPU.store(3, Ordering::Release);
            drop(span);
            let records = spans();
            assert_eq!(records.len(), 1);
            contains_fields(&records[0], &["category=syscall", "operation=148", "cpu=2",
                "tid=37", "tid_source=entry", "root=0xabc000", "elapsed_ms=11"]);
            assert_eq!(TID_CALLS.load(Ordering::Acquire), 1);
        }
        "arm-once-window" => {
            trace::arm();
            now(30_099);
            trace::arm();
            let admitted = trace::span("edge", 1, 0, 0);
            now(30_100);
            let excluded = trace::syscall(148, 0);
            let expiry_reads = CLOCK_READS.load(Ordering::Acquire);
            trace::arm();
            for _ in 0..10 {
                drop(trace::syscall(148, 0));
                drop(trace::span("expired-irq", 0, 0, 0));
            }
            assert_eq!(CLOCK_READS.load(Ordering::Acquire), expiry_reads,
                "post-window disabled entries must not read architectural counter");
            now(40_100);
            drop(excluded);
            assert_eq!(CLOCK_READS.load(Ordering::Acquire), expiry_reads,
                "disabled Drop must not read architectural counter");
            drop(admitted);
            assert_eq!(lines("MAKOS_AARCH64_INTERACTION_TRACE_ARM").len(), 1,
                "arming must not extend or reset the window");
            assert_eq!(TID_CALLS.load(Ordering::Acquire), 0,
                "expired syscall diagnostic must not lookup scheduler TID");
            let records = spans();
            assert_eq!(records.len(), 1, "admitted span must survive window expiry");
            contains_fields(&records[0], &["category=edge", "start_ms=30099",
                "end_ms=40100", "elapsed_ms=10001"]);
        }
        "early-return" => {
            fn operation() {
                let _span = trace::syscall(148, 0);
                now(120);
                return;
            }
            trace::arm();
            operation();
            assert_eq!(spans().len(), 1, "Drop must record early-return SVC spans");
        }
        "budget" => {
            trace::arm();
            for _ in 0..70 { span_at(100, 109); }
            for index in 0..66 { span_at(100 + index * 10, 110 + index * 10); }
            assert_eq!(spans().len(), 64, "diagnostic record budget must remain 64");
            let dropped = lines("MAKOS_AARCH64_INTERACTION_TRACE_DROPPED");
            assert_eq!(dropped.len(), 1, "first overflow must be reported exactly once");
            contains_fields(&dropped[0], &["dropped=1", "count_kind=at-first-overflow",
                "total_later_drops=not-reported"]);
        }
        "inclusive" => {
            trace::arm();
            let outer = trace::span("outer", 1, 0, 0);
            now(105);
            let inner = trace::span("inner", 2, 0, 0);
            now(120);
            drop(inner);
            now(130);
            drop(outer);
            let records = spans();
            assert_eq!(records.len(), 2);
            contains_fields(&records[0], &["category=inner", "elapsed_ms=15", "inclusive=1"]);
            contains_fields(&records[1], &["category=outer", "elapsed_ms=30", "inclusive=1"]);
        }
        "concurrent-budget" => {
            trace::arm();
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(9));
            let mut workers = Vec::new();
            for _ in 0..8 {
                let barrier = barrier.clone();
                workers.push(std::thread::spawn(move || {
                    trace::arm();
                    let pending: Vec<_> = (0..10).map(|_| trace::span("parallel", 1, 0, 0)).collect();
                    barrier.wait();
                    barrier.wait();
                    drop(pending);
                }));
            }
            barrier.wait();
            now(110);
            barrier.wait();
            for worker in workers { worker.join().unwrap(); }
            assert_eq!(lines("MAKOS_AARCH64_INTERACTION_TRACE_ARM").len(), 1);
            assert_eq!(spans().len(), 64, "concurrent diagnostic budget must remain 64");
            assert_eq!(lines("MAKOS_AARCH64_INTERACTION_TRACE_DROPPED").len(), 1);
        }
        _ => panic!("unknown host case"),
    }
}
'''


with tempfile.TemporaryDirectory(prefix="makos-interaction-trace-") as temporary:
    directory = Path(temporary)
    fixture = directory / "host.rs"
    fixture.write_text(HOST)

    def compile_fixture(name: str, source: str, *, missing_drop: bool = False) -> Path:
        # Rust inner doc comments remain documentation, not test logic.
        source = re.sub(r"(?m)^//!", "//", source)
        if missing_drop:
            # Removing Drop deliberately strands production fields/statics.
            # Admit only that mutation's dead code so its runtime assertion,
            # rather than an unrelated warning, must reject the missing Drop.
            fixture.write_text(HOST.replace("mod trace {", "#[allow(dead_code)]\nmod trace {", 1))
        else:
            fixture.write_text(HOST)
        (directory / "production.rs").write_text(source)
        binary = directory / name
        subprocess.run(["rustc", "--edition=2021", "-Dwarnings", str(fixture),
                        "-o", str(binary)], check=True)
        return binary

    binary = compile_fixture("interaction-trace", SOURCE)
    for case in ("disabled", "threshold-context", "syscall", "arm-once-window",
                 "early-return", "budget", "inclusive", "concurrent-budget"):
        subprocess.run([str(binary), case], check=True, timeout=10)

    def reject(name: str, source: str, case: str, diagnostic: str) -> None:
        assert source != SOURCE, f"{name}: mutation did not change production code"
        result = subprocess.run([str(compile_fixture(name, source, missing_drop=name == "missing-drop")), case],
                                capture_output=True, text=True, timeout=10)
        assert result.returncode != 0 and diagnostic in result.stdout + result.stderr, (
            f"{name} failed to reject restored defect: {result.stdout}{result.stderr}"
        )

    original_drop = body(SOURCE, "fn drop(")
    reject("missing-drop", SOURCE.replace(original_drop, "", 1), "early-return",
           "Drop must record early-return SVC spans")
    unbounded = re.sub(r"(const RECORD_LIMIT: u64 = )64;", r"\g<1>u64::MAX;", SOURCE)
    reject("unbounded-budget", unbounded, "budget", "diagnostic record budget must remain 64")
    missing_overflow = SOURCE.replace("if prior == 0 {", "if false && prior == 0 {", 1)
    reject("missing-overflow", missing_overflow, "budget",
           "first overflow must be reported exactly once")
    forbidden = SOURCE.replace("crate::arch::uptime_millis()", "crate::arch::monotonic_ticks()")
    try:
        check_clock(forbidden)
    except AssertionError:
        pass
    else:
        raise AssertionError("serviced-tick negative control was accepted")
    original_span = body(SOURCE, "pub fn span(")
    forbidden_irq = SOURCE.replace(original_span,
        "let _forbidden = crate::aarch64_process::current_tid();" + original_span, 1)
    reject("irq-scheduler-lookup", forbidden_irq, "threshold-context",
           "IRQ span must never lookup scheduler TID")

print("MAKOS_AARCH64_INTERACTION_TRACE_HOST_OK logic=production-module "
      "clock,cpu,tid,serial=host-adapters cases=8 "
      "negative_controls=missing-drop,unbounded-budget,missing-overflow,serviced-tick,irq-scheduler-lookup "
      "scope=bounded-diagnostics-not-firefox-repair-or-runtime-pass")
