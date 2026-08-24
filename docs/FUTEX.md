# Futex core

`makos-futex` provides the allocation-free wait-state foundation needed by a
real pthread implementation and Firefox. It is not a userspace compatibility
spoof: syscall and scheduler integration remain explicit kernel work.

## Contract

- Key: `(address_space, 4-byte-aligned user_address)`. Shared mappings must
  resolve to a common address-space/backing-object identity before calling the
  core.
- `WAIT`: kernel validates the user mapping, atomically reads the `u32` futex
  word under its address-space/futex lock, then passes `observed` and `expected`
  to `FutexTable::wait`. Mismatch returns `ValueMismatch` (`EAGAIN`). An expired
  absolute deadline returns `DeadlineExpired` (`ETIMEDOUT`).
- Lost-wake rule: scheduler must not release the synchronization protecting the
  value check and enqueue until the returned handle is committed. Before
  sleeping, recheck `state(handle)`; a concurrent wake may already have made the
  task runnable.
- `WAKE`: `wake(key, maximum, callback)` selects matching tasks FIFO. Callback
  receives exact task IDs and generation-safe handles; scheduler makes them
  runnable. `maximum == 0` is defined as no-op.
- Timeout: timer code calls `expire(now, callback)`. Deadlines are absolute
  monotonic ticks. Callback makes timed-out tasks runnable.
- Signal/thread cancellation: `cancel(handle)` or `cancel_task(task)` changes a
  waiting slot to `Cancelled`; syscall return maps this to `EINTR` when caused by
  a signal.
- Completion: resumed task calls `take_outcome(handle)`. Terminal slots retain
  capacity until reaped, preventing ABA reuse.
- Process exit: `process_exit(pid)` immediately removes both waiting and
  completed slots. It never schedules dead tasks.

## Bounds and ordering

`FutexTable<N>` owns exactly `N` slots and allocates nothing. One intrusive FIFO
contains active waiters; interleaved keys retain per-key enqueue order. Handle
generation rejects stale references after slot reuse. Capacity exhaustion is
`QueueFull`, normally mapped to `EAGAIN`.

## Error boundary

The core reports alignment, value mismatch, expired deadline, duplicate task,
capacity, and stale-handle errors. Kernel syscall code remains responsible for
`EFAULT`, supported futex-operation flags, timespec validation/conversion,
signal restart policy, atomic user access, scheduler locking, and copy faults.

## Verification

From the workspace root:

```sh
cargo test -p makos-futex
cargo check -p makos-futex --target aarch64-unknown-none
```

Tests cover FIFO selection, address-space isolation, mismatch/alignment errors,
bounded capacity, zero capacity/wake, absolute timeout, cancel/cancel-by-task,
process-exit cleanup, terminal reaping, stale handles, and slot-link reuse.
