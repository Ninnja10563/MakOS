//! Bounded Firefox file-syscall failure evidence; never changes syscall results.
//!
//! Only the normal SVC dispatch completion calls this module. Early blocking
//! branches and scheduler returns are outside this observation scope. Arguments
//! remain uninterpreted entry registers: no pointer dereference or VFS lookup.

use core::sync::atomic::{AtomicU64, Ordering};

const RECORD_LIMIT: u64 = 4;
// Four records plus one first-overflow notice per operation, then saturated.
static COUNTS: [AtomicU64; 14] = [const { AtomicU64::new(0) }; 14];

fn operation_index(number: u64) -> Option<usize> {
    match number {
        11 => Some(0),
        12 => Some(1),
        17 => Some(2),
        36 => Some(3),
        53 => Some(4),
        87 => Some(5),
        89 => Some(6),
        92 => Some(7),
        96 => Some(8),
        97 => Some(9),
        100 => Some(10),
        101 => Some(11),
        132 => Some(12),
        133 => Some(13),
        _ => None,
    }
}

pub fn record(number: u64, args: [u64; 6], result: u64, root: u64) {
    if (result as i64) >= 0 {
        return;
    }
    let Some(index) = operation_index(number) else {
        return;
    };
    let count = &COUNTS[index];
    if count.load(Ordering::Acquire) > RECORD_LIMIT {
        return;
    }
    if crate::aarch64_process::current_app_role()
        != crate::aarch64_process::ProcessRole::Firefox
    {
        return;
    }
    let Ok(prior) = count.fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
        (value <= RECORD_LIMIT).then_some(value + 1)
    }) else {
        return;
    };
    if prior == RECORD_LIMIT {
        crate::serial_println!(
            "MAKOS_AARCH64_FILE_SYSCALL_FAILURE_OVERFLOW op={} cap={} dropped=1 count_kind=at-first-overflow total_later_drops=not-reported scope=normal-dispatch",
            number,
            RECORD_LIMIT,
        );
        return;
    }
    let cpu = crate::arch::cpu_index();
    let tid = crate::aarch64_process::current_tid();
    let time_ms = crate::arch::uptime_millis();
    crate::serial_println!(
        "MAKOS_AARCH64_FILE_SYSCALL_FAILURE op={} result={} arg0={:#x} arg1={:#x} arg2={:#x} arg3={:#x} arg4={:#x} arg5={:#x} cpu={} tid={} root={:#x} time_ms={} clock=architectural-counter args=raw-entry-registers scope=normal-dispatch",
        number, result as i64, args[0], args[1], args[2], args[3], args[4], args[5],
        cpu, tid, root, time_ms,
    );
}
