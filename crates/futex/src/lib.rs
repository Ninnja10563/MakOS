#![no_std]

//! Allocation-free futex wait-queue state.
//!
//! This crate deliberately does not read user memory, block threads, program a
//! timer, or choose errno values. Kernel integration performs the atomic futex
//! word check while holding its address-space/futex synchronization, calls
//! [`FutexTable::wait`], then blocks only if the returned handle remains in
//! [`WaiterState::Waiting`]. Wake and expiry callbacks identify tasks that must
//! be made runnable.

/// A futex is private to one address space at one aligned user address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FutexKey {
    pub address_space: u64,
    pub address: u64,
}

impl FutexKey {
    pub const fn new(address_space: u64, address: u64) -> Self {
        Self {
            address_space,
            address,
        }
    }

    pub const fn is_aligned(self) -> bool {
        self.address & 3 == 0
    }
}

/// Stable kernel task identity. Process identity supports exit cleanup; thread
/// identity prevents one task from occupying multiple wait slots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskId {
    pub process: u64,
    pub thread: u64,
}

impl TaskId {
    pub const fn new(process: u64, thread: u64) -> Self {
        Self { process, thread }
    }
}

/// Generation-safe reference to a bounded waiter slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaitHandle {
    slot: usize,
    generation: u64,
}

impl WaitHandle {
    pub const fn slot(self) -> usize {
        self.slot
    }

    pub const fn generation(self) -> u64 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaiterState {
    Waiting,
    Woken,
    TimedOut,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitOutcome {
    Woken,
    TimedOut,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitError {
    /// Linux `EINVAL`: futex words are four-byte aligned.
    UnalignedAddress,
    /// Linux `EAGAIN`: value changed before the waiter was committed.
    ValueMismatch,
    /// Linux `ETIMEDOUT`: absolute deadline already passed.
    DeadlineExpired,
    /// Kernel misuse: one task already owns an unreaped slot.
    AlreadyWaiting,
    /// Fixed waiter capacity exhausted; integration may map this to `EAGAIN`.
    QueueFull,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WakeError {
    UnalignedAddress,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequeueError {
    UnalignedAddress,
    SameKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HandleError {
    InvalidOrStale,
}

#[derive(Clone, Copy)]
enum SlotState {
    Free,
    Waiting,
    Woken,
    TimedOut,
    Cancelled,
}

#[derive(Clone, Copy)]
struct Slot {
    generation: u64,
    state: SlotState,
    key: FutexKey,
    task: TaskId,
    deadline: Option<u64>,
    next: Option<usize>,
}

impl Slot {
    const EMPTY: Self = Self {
        generation: 0,
        state: SlotState::Free,
        key: FutexKey::new(0, 0),
        task: TaskId::new(0, 0),
        deadline: None,
        next: None,
    };
}

/// Fixed-capacity deterministic futex table.
///
/// Waiting slots form one intrusive FIFO. Wake scans that FIFO, therefore
/// waiters sharing a key are always selected in enqueue order even when other
/// keys are interleaved. Completed slots remain occupied until
/// [`take_outcome`](Self::take_outcome); this prevents handle reuse before the
/// scheduler observes completion.
pub struct FutexTable<const N: usize> {
    slots: [Slot; N],
    head: Option<usize>,
    tail: Option<usize>,
    occupied: usize,
    waiting: usize,
}

impl<const N: usize> FutexTable<N> {
    pub const fn new() -> Self {
        Self {
            slots: [Slot::EMPTY; N],
            head: None,
            tail: None,
            occupied: 0,
            waiting: 0,
        }
    }

    /// Commit a wait after integration atomically loaded `observed`.
    /// `deadline` is an absolute monotonic tick; `None` means infinite.
    pub fn wait(
        &mut self,
        key: FutexKey,
        task: TaskId,
        observed: u32,
        expected: u32,
        now: u64,
        deadline: Option<u64>,
    ) -> Result<WaitHandle, WaitError> {
        if !key.is_aligned() {
            return Err(WaitError::UnalignedAddress);
        }
        if observed != expected {
            return Err(WaitError::ValueMismatch);
        }
        if deadline.is_some_and(|value| value <= now) {
            return Err(WaitError::DeadlineExpired);
        }
        if self
            .slots
            .iter()
            .any(|slot| !matches!(slot.state, SlotState::Free) && slot.task == task)
        {
            return Err(WaitError::AlreadyWaiting);
        }
        let index = self
            .slots
            .iter()
            .position(|slot| matches!(slot.state, SlotState::Free))
            .ok_or(WaitError::QueueFull)?;
        let generation = self.slots[index].generation.wrapping_add(1);
        self.slots[index] = Slot {
            generation,
            state: SlotState::Waiting,
            key,
            task,
            deadline,
            next: None,
        };
        if let Some(tail) = self.tail {
            self.slots[tail].next = Some(index);
        } else {
            self.head = Some(index);
        }
        self.tail = Some(index);
        self.occupied += 1;
        self.waiting += 1;
        Ok(WaitHandle {
            slot: index,
            generation,
        })
    }

    /// Wake at most `maximum` matching tasks in FIFO order.
    ///
    /// Callback runs after each slot becomes `Woken`; scheduler integration
    /// should enqueue that task as runnable. `maximum == 0` is a no-op.
    pub fn wake(
        &mut self,
        key: FutexKey,
        maximum: usize,
        mut on_wake: impl FnMut(TaskId, WaitHandle),
    ) -> Result<usize, WakeError> {
        if !key.is_aligned() {
            return Err(WakeError::UnalignedAddress);
        }
        let mut count = 0;
        let mut previous = None;
        let mut current = self.head;
        while let Some(index) = current {
            let next = self.slots[index].next;
            if count < maximum && self.slots[index].key == key {
                self.unlink(previous, index, next);
                self.slots[index].state = SlotState::Woken;
                self.slots[index].deadline = None;
                self.slots[index].next = None;
                self.waiting -= 1;
                count += 1;
                on_wake(
                    self.slots[index].task,
                    WaitHandle {
                        slot: index,
                        generation: self.slots[index].generation,
                    },
                );
            } else {
                previous = Some(index);
            }
            current = next;
        }
        Ok(count)
    }

    /// Wake part of a source queue, then retag more source waiters to `target`.
    ///
    /// Requeued slots retain their handles, deadlines, and FIFO positions.
    /// Only woken slots leave the wait queue and invoke `on_wake`.
    pub fn requeue(
        &mut self,
        source: FutexKey,
        target: FutexKey,
        wake_limit: usize,
        requeue_limit: usize,
        mut on_wake: impl FnMut(TaskId, WaitHandle),
    ) -> Result<(usize, usize), RequeueError> {
        if !source.is_aligned() || !target.is_aligned() {
            return Err(RequeueError::UnalignedAddress);
        }
        if source == target {
            return Err(RequeueError::SameKey);
        }
        let mut woken = 0;
        let mut requeued = 0;
        let mut previous = None;
        let mut current = self.head;
        while let Some(index) = current {
            let next = self.slots[index].next;
            if self.slots[index].key == source && woken < wake_limit {
                self.unlink(previous, index, next);
                self.slots[index].state = SlotState::Woken;
                self.slots[index].deadline = None;
                self.slots[index].next = None;
                self.waiting -= 1;
                woken += 1;
                on_wake(
                    self.slots[index].task,
                    WaitHandle {
                        slot: index,
                        generation: self.slots[index].generation,
                    },
                );
            } else {
                if self.slots[index].key == source && requeued < requeue_limit {
                    self.slots[index].key = target;
                    requeued += 1;
                }
                previous = Some(index);
            }
            current = next;
        }
        Ok((woken, requeued))
    }

    /// Expire all deadlines at or before `now`, in enqueue order.
    pub fn expire(&mut self, now: u64, mut on_timeout: impl FnMut(TaskId, WaitHandle)) -> usize {
        let mut count = 0;
        let mut previous = None;
        let mut current = self.head;
        while let Some(index) = current {
            let next = self.slots[index].next;
            let expired = self.slots[index]
                .deadline
                .is_some_and(|deadline| deadline <= now);
            if expired {
                self.unlink(previous, index, next);
                self.slots[index].state = SlotState::TimedOut;
                self.slots[index].deadline = None;
                self.slots[index].next = None;
                self.waiting -= 1;
                count += 1;
                on_timeout(
                    self.slots[index].task,
                    WaitHandle {
                        slot: index,
                        generation: self.slots[index].generation,
                    },
                );
            } else {
                previous = Some(index);
            }
            current = next;
        }
        count
    }

    /// Interrupt a specific wait. Returns false if it already completed.
    pub fn cancel(&mut self, handle: WaitHandle) -> Result<bool, HandleError> {
        let index = self.validate(handle)?;
        if !matches!(self.slots[index].state, SlotState::Waiting) {
            return Ok(false);
        }
        self.unlink_index(index);
        self.slots[index].state = SlotState::Cancelled;
        self.slots[index].deadline = None;
        self.waiting -= 1;
        Ok(true)
    }

    /// Interrupt a task without retaining an external handle.
    pub fn cancel_task(&mut self, task: TaskId) -> Option<WaitHandle> {
        let index = self
            .slots
            .iter()
            .position(|slot| matches!(slot.state, SlotState::Waiting) && slot.task == task)?;
        let handle = WaitHandle {
            slot: index,
            generation: self.slots[index].generation,
        };
        self.cancel(handle).ok().filter(|changed| *changed)?;
        Some(handle)
    }

    /// Remove every slot owned by a dead process, including unreaped terminal
    /// outcomes. No callback is issued because dead tasks must not be run.
    pub fn process_exit(&mut self, process: u64) -> usize {
        let mut count = 0;
        for index in 0..N {
            if !matches!(self.slots[index].state, SlotState::Free)
                && self.slots[index].task.process == process
            {
                if matches!(self.slots[index].state, SlotState::Waiting) {
                    self.unlink_index(index);
                    self.waiting -= 1;
                }
                self.release(index);
                count += 1;
            }
        }
        count
    }

    pub fn state(&self, handle: WaitHandle) -> Result<WaiterState, HandleError> {
        let index = self.validate(handle)?;
        match self.slots[index].state {
            SlotState::Free => Err(HandleError::InvalidOrStale),
            SlotState::Waiting => Ok(WaiterState::Waiting),
            SlotState::Woken => Ok(WaiterState::Woken),
            SlotState::TimedOut => Ok(WaiterState::TimedOut),
            SlotState::Cancelled => Ok(WaiterState::Cancelled),
        }
    }

    /// Return current key for a live waiter handle. Requeue changes this value
    /// while preserving handle identity.
    pub fn key(&self, handle: WaitHandle) -> Result<FutexKey, HandleError> {
        let index = self.validate(handle)?;
        Ok(self.slots[index].key)
    }

    /// Consume a terminal result and release capacity. Waiting returns `None`.
    pub fn take_outcome(&mut self, handle: WaitHandle) -> Result<Option<WaitOutcome>, HandleError> {
        let index = self.validate(handle)?;
        let outcome = match self.slots[index].state {
            SlotState::Free => return Err(HandleError::InvalidOrStale),
            SlotState::Waiting => return Ok(None),
            SlotState::Woken => WaitOutcome::Woken,
            SlotState::TimedOut => WaitOutcome::TimedOut,
            SlotState::Cancelled => WaitOutcome::Cancelled,
        };
        self.release(index);
        Ok(Some(outcome))
    }

    pub const fn capacity(&self) -> usize {
        N
    }

    pub const fn occupied(&self) -> usize {
        self.occupied
    }

    pub const fn waiting(&self) -> usize {
        self.waiting
    }

    fn validate(&self, handle: WaitHandle) -> Result<usize, HandleError> {
        let slot = self
            .slots
            .get(handle.slot)
            .ok_or(HandleError::InvalidOrStale)?;
        if matches!(slot.state, SlotState::Free) || slot.generation != handle.generation {
            return Err(HandleError::InvalidOrStale);
        }
        Ok(handle.slot)
    }

    fn unlink_index(&mut self, target: usize) {
        let mut previous = None;
        let mut current = self.head;
        while let Some(index) = current {
            let next = self.slots[index].next;
            if index == target {
                self.unlink(previous, index, next);
                return;
            }
            previous = Some(index);
            current = next;
        }
    }

    fn unlink(&mut self, previous: Option<usize>, index: usize, next: Option<usize>) {
        if let Some(previous) = previous {
            self.slots[previous].next = next;
        } else {
            self.head = next;
        }
        if self.tail == Some(index) {
            self.tail = previous;
        }
    }

    fn release(&mut self, index: usize) {
        let generation = self.slots[index].generation;
        self.slots[index] = Slot {
            generation,
            ..Slot::EMPTY
        };
        self.occupied -= 1;
    }
}

impl<const N: usize> Default for FutexTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::vec::Vec;

    const A: FutexKey = FutexKey::new(1, 0x1000);
    const B: FutexKey = FutexKey::new(1, 0x2000);

    fn task(id: u64) -> TaskId {
        TaskId::new(id / 10, id)
    }

    fn enqueue<const N: usize>(table: &mut FutexTable<N>, key: FutexKey, id: u64) -> WaitHandle {
        table.wait(key, task(id), 7, 7, 0, None).unwrap()
    }

    #[test]
    fn wait_rejects_bad_preconditions_without_consuming_capacity() {
        let mut table = FutexTable::<2>::new();
        assert_eq!(
            table.wait(FutexKey::new(1, 3), task(1), 0, 0, 0, None),
            Err(WaitError::UnalignedAddress)
        );
        assert_eq!(
            table.wait(A, task(1), 1, 2, 0, None),
            Err(WaitError::ValueMismatch)
        );
        assert_eq!(
            table.wait(A, task(1), 2, 2, 10, Some(10)),
            Err(WaitError::DeadlineExpired)
        );
        assert_eq!(table.occupied(), 0);
    }

    #[test]
    fn wake_is_fifo_per_key_across_interleaved_keys() {
        let mut table = FutexTable::<6>::new();
        let a1 = enqueue(&mut table, A, 11);
        let _b1 = enqueue(&mut table, B, 21);
        let a2 = enqueue(&mut table, A, 12);
        let a3 = enqueue(&mut table, A, 13);
        let mut tasks = Vec::new();
        assert_eq!(table.wake(A, 2, |task, _| tasks.push(task)), Ok(2));
        assert_eq!(tasks, [task(11), task(12)]);
        assert_eq!(table.state(a1), Ok(WaiterState::Woken));
        assert_eq!(table.state(a2), Ok(WaiterState::Woken));
        assert_eq!(table.state(a3), Ok(WaiterState::Waiting));
        assert_eq!(table.waiting(), 2);
    }

    #[test]
    fn address_space_is_part_of_key() {
        let mut table = FutexTable::<2>::new();
        let other_space = FutexKey::new(2, A.address);
        let first = enqueue(&mut table, A, 11);
        let second = enqueue(&mut table, other_space, 21);
        assert_eq!(table.wake(A, usize::MAX, |_, _| {}), Ok(1));
        assert_eq!(table.state(first), Ok(WaiterState::Woken));
        assert_eq!(table.state(second), Ok(WaiterState::Waiting));
    }

    #[test]
    fn duplicate_task_and_capacity_are_bounded() {
        let mut table = FutexTable::<1>::new();
        let handle = enqueue(&mut table, A, 11);
        assert_eq!(
            table.wait(B, task(11), 0, 0, 0, None),
            Err(WaitError::AlreadyWaiting)
        );
        assert_eq!(
            table.wait(B, task(21), 0, 0, 0, None),
            Err(WaitError::QueueFull)
        );
        assert_eq!(table.cancel(handle), Ok(true));
        assert_eq!(
            table.wait(B, task(21), 0, 0, 0, None),
            Err(WaitError::QueueFull)
        );
        assert_eq!(table.take_outcome(handle), Ok(Some(WaitOutcome::Cancelled)));
        assert!(table.wait(B, task(21), 0, 0, 0, None).is_ok());
    }

    #[test]
    fn expiry_is_ordered_and_only_due_deadlines_fire() {
        let mut table = FutexTable::<4>::new();
        let late = table.wait(A, task(11), 0, 0, 0, Some(20)).unwrap();
        let early = table.wait(B, task(21), 0, 0, 0, Some(10)).unwrap();
        let forever = table.wait(A, task(12), 0, 0, 0, None).unwrap();
        let mut expired = Vec::new();
        assert_eq!(table.expire(10, |task, _| expired.push(task)), 1);
        assert_eq!(expired, [task(21)]);
        assert_eq!(table.state(early), Ok(WaiterState::TimedOut));
        assert_eq!(table.state(late), Ok(WaiterState::Waiting));
        assert_eq!(table.state(forever), Ok(WaiterState::Waiting));
        assert_eq!(table.take_outcome(early), Ok(Some(WaitOutcome::TimedOut)));
    }

    #[test]
    fn cancellation_removes_waiter_and_is_idempotent_after_completion() {
        let mut table = FutexTable::<3>::new();
        let first = enqueue(&mut table, A, 11);
        let second = enqueue(&mut table, A, 12);
        assert_eq!(table.cancel(first), Ok(true));
        assert_eq!(table.cancel(first), Ok(false));
        assert_eq!(
            table.wake(A, 1, |woken, _| assert_eq!(woken, task(12))),
            Ok(1)
        );
        assert_eq!(table.state(second), Ok(WaiterState::Woken));
        assert_eq!(table.take_outcome(first), Ok(Some(WaitOutcome::Cancelled)));
    }

    #[test]
    fn cancel_task_supports_signal_interruption() {
        let mut table = FutexTable::<2>::new();
        let handle = enqueue(&mut table, A, 11);
        assert_eq!(table.cancel_task(task(99)), None);
        assert_eq!(table.cancel_task(task(11)), Some(handle));
        assert_eq!(table.state(handle), Ok(WaiterState::Cancelled));
    }

    #[test]
    fn process_exit_frees_waiting_and_unreaped_terminal_slots() {
        let mut table = FutexTable::<4>::new();
        let waiting = enqueue(&mut table, A, 11);
        let terminal = enqueue(&mut table, B, 12);
        let survivor = enqueue(&mut table, A, 21);
        assert_eq!(table.cancel(terminal), Ok(true));
        assert_eq!(table.process_exit(1), 2);
        assert_eq!(table.occupied(), 1);
        assert_eq!(table.waiting(), 1);
        assert_eq!(table.state(waiting), Err(HandleError::InvalidOrStale));
        assert_eq!(table.state(terminal), Err(HandleError::InvalidOrStale));
        assert_eq!(table.state(survivor), Ok(WaiterState::Waiting));
    }

    #[test]
    fn stale_handle_cannot_observe_reused_slot() {
        let mut table = FutexTable::<1>::new();
        let old = enqueue(&mut table, A, 11);
        table.cancel(old).unwrap();
        table.take_outcome(old).unwrap();
        let new = enqueue(&mut table, A, 21);
        assert_eq!(old.slot(), new.slot());
        assert_ne!(old.generation(), new.generation());
        assert_eq!(table.state(old), Err(HandleError::InvalidOrStale));
        assert_eq!(table.state(new), Ok(WaiterState::Waiting));
    }

    #[test]
    fn zero_capacity_and_zero_wake_are_defined() {
        let mut empty = FutexTable::<0>::new();
        assert_eq!(
            empty.wait(A, task(11), 0, 0, 0, None),
            Err(WaitError::QueueFull)
        );
        let mut table = FutexTable::<1>::new();
        let handle = enqueue(&mut table, A, 11);
        assert_eq!(table.wake(A, 0, |_, _| panic!()), Ok(0));
        assert_eq!(table.state(handle), Ok(WaiterState::Waiting));
        assert_eq!(
            table.wake(FutexKey::new(1, 3), 1, |_, _| {}),
            Err(WakeError::UnalignedAddress)
        );
    }

    #[test]
    fn slot_reuse_preserves_fifo_links() {
        let mut table = FutexTable::<3>::new();
        let first = enqueue(&mut table, A, 11);
        let _second = enqueue(&mut table, A, 12);
        table.cancel(first).unwrap();
        table.take_outcome(first).unwrap();
        let _reused = enqueue(&mut table, A, 13);
        let mut order = Vec::new();
        assert_eq!(table.wake(A, 2, |task, _| order.push(task)), Ok(2));
        assert_eq!(order, [task(12), task(13)]);
    }

    #[test]
    fn requeue_wakes_then_retags_without_releasing_handles() {
        let mut table = FutexTable::<3>::new();
        let first = enqueue(&mut table, A, 11);
        let second = enqueue(&mut table, A, 12);
        let third = enqueue(&mut table, A, 13);
        let mut woken = Vec::new();
        assert_eq!(
            table.requeue(A, B, 1, 1, |task, _| woken.push(task)),
            Ok((1, 1))
        );
        assert_eq!(woken, [task(11)]);
        assert_eq!(table.state(first), Ok(WaiterState::Woken));
        assert_eq!(table.state(second), Ok(WaiterState::Waiting));
        assert_eq!(table.state(third), Ok(WaiterState::Waiting));
        assert_eq!(table.key(first), Ok(A));
        assert_eq!(table.key(second), Ok(B));
        assert_eq!(table.key(third), Ok(A));
        assert_eq!(table.wake(B, 1, |task, _| woken.push(task)), Ok(1));
        assert_eq!(woken, [task(11), task(12)]);
        assert_eq!(table.wake(A, 1, |task, _| woken.push(task)), Ok(1));
        assert_eq!(woken, [task(11), task(12), task(13)]);
    }

    #[test]
    fn requeue_preserves_global_fifo_and_deadlines() {
        let mut table = FutexTable::<3>::new();
        let target_first = enqueue(&mut table, B, 21);
        let source_timed = table.wait(A, task(11), 0, 0, 0, Some(10)).unwrap();
        let source_last = enqueue(&mut table, A, 12);
        assert_eq!(table.requeue(A, B, 0, 2, |_, _| panic!()), Ok((0, 2)));
        let mut expired = Vec::new();
        assert_eq!(table.expire(10, |task, _| expired.push(task)), 1);
        assert_eq!(expired, [task(11)]);
        assert_eq!(table.state(source_timed), Ok(WaiterState::TimedOut));
        let mut order = Vec::new();
        assert_eq!(table.wake(B, 2, |task, _| order.push(task)), Ok(2));
        assert_eq!(order, [task(21), task(12)]);
        assert_eq!(table.state(target_first), Ok(WaiterState::Woken));
        assert_eq!(table.state(source_last), Ok(WaiterState::Woken));
    }

    #[test]
    fn requeue_rejects_unaligned_or_identical_keys() {
        let mut table = FutexTable::<1>::new();
        assert_eq!(
            table.requeue(A, FutexKey::new(1, 3), 0, 1, |_, _| {}),
            Err(RequeueError::UnalignedAddress)
        );
        assert_eq!(
            table.requeue(A, A, 0, 1, |_, _| {}),
            Err(RequeueError::SameKey)
        );
    }
}
