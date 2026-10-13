//! Allocation-free FIFO admission for short kernel critical sections.
//!
//! The caller must prevent local interrupt re-entry before reserving a ticket
//! and keep that protection until the guard is dropped. Waiting CPUs cannot
//! cancel tickets or sleep while an earlier ticket can depend on them. With
//! fewer than 2^64 outstanding acquisitions, wrapping preserves ticket order.

use core::sync::atomic::{AtomicU64, Ordering};

pub(crate) struct TicketLock {
    next: AtomicU64,
    serving: AtomicU64,
}

impl TicketLock {
    pub(crate) const fn new() -> Self {
        Self {
            next: AtomicU64::new(0),
            serving: AtomicU64::new(0),
        }
    }

    pub(crate) fn lock(&self) -> TicketGuard<'_> {
        let ticket = self.reserve_ticket();
        while !self.has_turn(ticket) {
            core::hint::spin_loop();
        }
        TicketGuard { lock: self, ticket }
    }

    fn reserve_ticket(&self) -> u64 {
        // This counter establishes queue order, not protected-data visibility.
        self.next.fetch_add(1, Ordering::Relaxed)
    }

    fn has_turn(&self, ticket: u64) -> bool {
        // Pair with the previous owner's release before touching protected data.
        self.serving.load(Ordering::Acquire) == ticket
    }
}

pub(crate) struct TicketGuard<'a> {
    lock: &'a TicketLock,
    ticket: u64,
}

impl Drop for TicketGuard<'_> {
    fn drop(&mut self) {
        self.lock
            .serving
            .store(self.ticket.wrapping_add(1), Ordering::Release);
    }
}
