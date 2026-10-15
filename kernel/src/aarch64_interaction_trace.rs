//! Bounded, opt-in evidence for the first Firefox key's kernel interaction.
//!
//! Spans are inclusive elapsed architectural-counter time, not CPU residency
//! or exclusive work. Recording never changes scheduling or guest deadlines.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const WINDOW_MS: u64 = 30_000;
const MIN_SPAN_MS: u64 = 10;
const RECORD_LIMIT: u64 = 64;

static ARMED: AtomicBool = AtomicBool::new(false);
static PUBLISHED: AtomicBool = AtomicBool::new(false);
static START_MS: AtomicU64 = AtomicU64::new(0);
static RECORDS: AtomicU64 = AtomicU64::new(0);
static DROPPED: AtomicU64 = AtomicU64::new(0);

/// Arm once; subsequent keys cannot extend or reset this observation window.
pub fn arm() {
    if ARMED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let start_ms = crate::arch::uptime_millis();
    START_MS.store(start_ms, Ordering::Relaxed);
    crate::serial_println!(
        "MAKOS_AARCH64_INTERACTION_TRACE_ARM start_ms={} window_ms={} min_span_ms={} cap={} clock=architectural-counter inclusive=1 tid_zero=unavailable rearm=0",
        start_ms,
        WINDOW_MS,
        MIN_SPAN_MS,
        RECORD_LIMIT,
    );
    // Publish only after the arm record and start time are initialized.
    PUBLISHED.store(true, Ordering::Release);
}

pub struct Span {
    enabled: bool,
    category: &'static str,
    operation: u64,
    cpu: usize,
    tid: u64,
    root: u64,
    start_ms: u64,
}

/// Caller supplies identity. In particular IRQ callers pass tid=0 rather than
/// acquiring scheduler state while interrupting an arbitrary kernel lock.
pub fn span(category: &'static str, operation: u64, tid: u64, root: u64) -> Span {
    let published = PUBLISHED.load(Ordering::Acquire);
    let start_ms = if published { crate::arch::uptime_millis() } else { 0 };
    let window_start = START_MS.load(Ordering::Relaxed);
    let enabled = published
        && start_ms >= window_start
        && start_ms - window_start < WINDOW_MS;
    if published && start_ms >= window_start && start_ms - window_start >= WINDOW_MS {
        // Stop counter reads on later entries once this once-only window is
        // over. Admitted guards own their entry state and still complete.
        // ARMED remains set, so another key cannot re-enable publication.
        PUBLISHED.store(false, Ordering::Release);
    }
    Span {
        enabled,
        category,
        operation,
        cpu: if enabled { crate::arch::cpu_index() } else { 0 },
        tid,
        root,
        start_ms,
    }
}

/// Called once at SVC entry before handler locks. Capture entry identity only
/// while armed; later frame switching cannot relabel the initiating syscall.
pub fn syscall(number: u64, root: u64) -> Span {
    let mut result = span("syscall", number, 0, root);
    if result.enabled {
        result.tid = crate::aarch64_process::current_tid();
    }
    result
}

impl Drop for Span {
    fn drop(&mut self) {
        if !self.enabled {
            return;
        }
        let end_ms = crate::arch::uptime_millis();
        let elapsed_ms = end_ms.saturating_sub(self.start_ms);
        if elapsed_ms < MIN_SPAN_MS {
            return;
        }
        // Admission is at span entry; a long operation may finish after the
        // observation window and still produces its truthful completed span.
        if RECORDS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < RECORD_LIMIT).then_some(count + 1)
            })
            .is_err()
        {
            let prior = DROPPED.fetch_add(1, Ordering::AcqRel);
            if prior == 0 {
                crate::serial_println!(
                    "MAKOS_AARCH64_INTERACTION_TRACE_DROPPED cap={} dropped=1 count_kind=at-first-overflow total_later_drops=not-reported",
                    RECORD_LIMIT,
                );
            }
            return;
        }
        crate::serial_println!(
            "MAKOS_AARCH64_INTERACTION_SPAN category={} operation={} cpu={} tid={} tid_source={} root={:#x} start_ms={} end_ms={} elapsed_ms={} inclusive=1 clock=architectural-counter",
            self.category,
            self.operation,
            self.cpu,
            self.tid,
            if self.tid == 0 { "unavailable" } else { "entry" },
            self.root,
            self.start_ms,
            end_ms,
            elapsed_ms,
        );
    }
}
