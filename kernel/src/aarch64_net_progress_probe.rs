//! Opt-in, immutable AP UDP workload: prove owner progress in real EL1 paths.
//!
//! The hooks only delay the selected boot-test task at actual TX publication.
//! They do not transmit packets, complete requests, or call the owner service.

use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

const PAYLOADS: [&[u8]; 3] = [
    b"MAKOS_NET_OWNER_1\n",
    b"MAKOS_NET_OWNER_2\n",
    b"MAKOS_NET_OWNER_3\n",
];
static ARMED_TID: AtomicU64 = AtomicU64::new(0);
static PHASE: AtomicU8 = AtomicU8::new(0);
static REQUESTED: AtomicBool = AtomicBool::new(false);
static READY: AtomicBool = AtomicBool::new(false);
static RELEASE: AtomicBool = AtomicBool::new(false);
static CONTENDED: AtomicBool = AtomicBool::new(false);

pub(crate) fn arm(tid: u64, phase: u8) {
    if crate::arch::cpu_index() != 0 || tid == 0 || !(1..=3).contains(&phase)
        || ARMED_TID.load(Ordering::Acquire) != 0
    {
        crate::fatal("AArch64 network owner probe invalid arm");
    }
    PHASE.store(phase, Ordering::Relaxed);
    REQUESTED.store(false, Ordering::Relaxed);
    READY.store(false, Ordering::Relaxed);
    RELEASE.store(false, Ordering::Relaxed);
    CONTENDED.store(false, Ordering::Relaxed);
    ARMED_TID.store(tid, Ordering::Release);
}

pub(crate) fn disarm() {
    ARMED_TID.store(0, Ordering::Release);
}

fn selected_request(kind: u8, payload: &[u8]) -> bool {
    let tid = ARMED_TID.load(Ordering::Acquire);
    // Normal application/network calls must not acquire a scheduler lock here.
    if tid == 0 || crate::arch::cpu_index() != 1 {
        return false;
    }
    if crate::aarch64_process::current_tid() != tid {
        return false;
    }
    let phase = PHASE.load(Ordering::Acquire);
    if crate::aarch64_process::current_app_role()
        != crate::aarch64_process::ProcessRole::SmpProbe
        || kind != 1 || !(1..=3).contains(&phase)
        || payload != PAYLOADS[usize::from(phase - 1)]
    {
        crate::fatal("AArch64 network owner probe unexpected request");
    }
    true
}

pub(crate) fn before_tx_publish(kind: u8, payload: &[u8]) {
    if !selected_request(kind, payload) {
        return;
    }
    if REQUESTED.swap(true, Ordering::AcqRel) {
        crate::fatal("AArch64 network owner probe duplicate publication");
    }
    wait_until(|| RELEASE.load(Ordering::Acquire), "AArch64 network owner probe release timeout");
}

pub(crate) fn after_tx_publish(kind: u8, payload: &[u8]) {
    if !selected_request(kind, payload) {
        return;
    }
    if !REQUESTED.load(Ordering::Acquire) || !RELEASE.load(Ordering::Acquire)
        || READY.swap(true, Ordering::AcqRel)
    {
        crate::fatal("AArch64 network owner probe publication order invalid");
    }
}

/// Called by the actual socket lock's failed-CAS path, before owner progress.
pub(crate) fn on_owner_socket_wait() {
    if ARMED_TID.load(Ordering::Acquire) == 0 || crate::arch::cpu_index() != 0
        || PHASE.load(Ordering::Acquire) != 1 || !REQUESTED.load(Ordering::Acquire)
    {
        return;
    }
    if !interrupts_masked() {
        crate::fatal("AArch64 network owner contention proof lacked IRQ mask");
    }
    CONTENDED.store(true, Ordering::Release);
    RELEASE.store(true, Ordering::Release);
}

pub(crate) fn state_lock_phase_armed() -> bool {
    ARMED_TID.load(Ordering::Acquire) != 0 && PHASE.load(Ordering::Acquire) == 3
        && REQUESTED.load(Ordering::Acquire) && !RELEASE.load(Ordering::Acquire)
}

fn interrupts_masked() -> bool {
    let daif: u64;
    unsafe { core::arch::asm!("mrs {value}, daif", value = out(reg) daif, options(nomem, nostack)) };
    daif & (1 << 7) != 0
}

fn wait_until(mut condition: impl FnMut() -> bool, message: &'static str) {
    let deadline = crate::arch::counter_deadline_millis(5_000);
    while !condition() {
        if crate::arch::counter_deadline_expired(deadline) {
            crate::fatal(message);
        }
        core::hint::spin_loop();
    }
}

pub(crate) fn run_phase(tid: u64, root: u64, phase: u8) {
    // Restore the caller's exact DAIF state on return. Phases 2/3 deliberately
    // enable IRQs while remaining in EL1; phase 1 must keep them masked.
    let _restore_interrupts = crate::arch::LocalInterruptMask::acquire();
    if ARMED_TID.load(Ordering::Acquire) != tid || PHASE.load(Ordering::Acquire) != phase
        || crate::arch::cpu_index() != 0 || root == 0 || root & 4095 != 0
    {
        crate::fatal("AArch64 network owner probe controller identity invalid");
    }
    let before = crate::aarch64_virtio_net::progress_evidence();
    let affinity_before = crate::aarch64_virtio_net::tx_affinity_evidence();
    wait_until(|| REQUESTED.load(Ordering::Acquire), "AArch64 network owner requester timeout");
    // One immutable process sends only one datagram. It cannot reacquire the
    // socket lock for the next phase while CPU0 is waiting for this phase.
    match phase {
        1 => {
            let _masked = crate::arch::LocalInterruptMask::acquire();
            if crate::aarch64_socket::close(u64::MAX) || !CONTENDED.load(Ordering::Acquire)
                || !interrupts_masked()
            {
                crate::fatal("AArch64 network owner socket contention proof failed");
            }
        }
        2 => {
            crate::arch::enable_interrupts();
            RELEASE.store(true, Ordering::Release);
            wait_until(|| READY.load(Ordering::Acquire)
                && crate::aarch64_virtio_net::progress_evidence().0 > before.0,
                "AArch64 network owner EL1 timer progress timeout");
            if interrupts_masked() {
                crate::fatal("AArch64 network owner EL1 timer proof masked IRQs");
            }
        }
        3 => {
            crate::arch::enable_interrupts();
            crate::aarch64_virtio_net::with_progress_probe_state_lock(|| {
                RELEASE.store(true, Ordering::Release);
                wait_until(|| READY.load(Ordering::Acquire),
                    "AArch64 network owner locked READY timeout");
                // Snapshot after READY is visible: an earlier unrelated tick
                // over this lock cannot satisfy the recursive-deferral proof.
                let deferred_before = crate::aarch64_virtio_net::progress_evidence().2;
                wait_until(|| crate::aarch64_virtio_net::progress_evidence().2 > deferred_before,
                    "AArch64 network owner recursive-deferral timeout");
                if interrupts_masked()
                    || crate::aarch64_virtio_net::progress_evidence().0 != before.0
                    || crate::aarch64_virtio_net::tx_affinity_evidence().0 != affinity_before.0
                {
                    crate::fatal("AArch64 network owner recursively serviced locked device");
                }
            });
            wait_until(|| crate::aarch64_virtio_net::progress_evidence().0 > before.0,
                "AArch64 network owner post-unlock progress timeout");
        }
        _ => crate::fatal("AArch64 network owner probe phase invalid"),
    }
    let after = crate::aarch64_virtio_net::progress_evidence();
    let (owner, requests) = crate::aarch64_virtio_net::tx_affinity_evidence();
    let timer = after.0 - before.0;
    let lock_wait = after.1 - before.1;
    let busy = after.2 - before.2;
    // requests may increment before run_phase takes its snapshot; the exact
    // cumulative count is verified by the process controller after each reap.
    if !READY.load(Ordering::Acquire) || owner != affinity_before.0 + 1
        || requests != u64::from(phase)
        || (phase == 1 && (timer != 0 || lock_wait != 1 || busy != 0))
        || (phase == 2 && (timer != 1 || lock_wait != 0 || busy != 0))
        || (phase == 3 && (timer != 1 || lock_wait != 0 || busy == 0))
    {
        crate::fatal("AArch64 network owner progress evidence invalid");
    }
    crate::serial_println!(
        "MAKOS_AARCH64_NET_OWNER_PHASE_OK phase={} tid={} root={:#x} requester_cpu=1 service_cpu=0 timer_completions={} lock_wait_completions={} busy_deferrals={} payload_bytes=18 irq={} proof={} transport=virtio-net-udp4",
        phase, tid, root, timer, lock_wait, busy,
        if phase == 1 { "masked" } else { "enabled" },
        match phase { 1 => "socket-lock-contention", 2 => "el1-timer", _ => "ready-locked-defer-unlock" },
    );
}
