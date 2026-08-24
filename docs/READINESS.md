# Readiness and epoll

## Targeted scheduler wakes

AArch64 direct pipe, Unix socketpair, UDP, and TCP waits register a bounded
`WaitSource` in their existing process-table slot. State changes wake only the
matching object plus `Any` waiters. `poll`, `epoll`, `pselect`, record locks,
signals, and TTY events retain wildcard behavior, including existing timeout
and signal retry semantics. Network RX records at most `PUMP_FRAMES` handles
per bounded pump pass; no dynamic allocation or unbounded registry was added.

Firefox surface keys add a separate bounded, one-shot scheduler hint for the
browser process leader. If Gecko's main thread is still inside its timed
app-shell condvar wait, the hint remains until that thread becomes ready, then
selects it ahead of the ordinary round-robin pass. The hint expires after 100
timer ticks. Pointer motion does not create hints, and no futex waiter is
forcibly removed, so pthread condvar state remains intact.

MakOS AArch64 exposes native syscalls 104-107 for process-owned epoll. The
allocation-free `makos-readiness` core stores four generation-tagged instances
and 16 watches per instance. It implements ADD, DELETE, MODIFY, duplicate
rejection, exact 64-bit user data, level triggering, edge transitions,
one-shot disable/rearm, bounded output, owner isolation, and exit cleanup.

Kernel backend maps regular files, directories, pipes, TTY output, and connected
socket handles. Blocking `epoll_wait` uses same saved-context syscall retry as
blocking pipes and poll: kernel rewinds ELR to original `svc`, removes task from
runnable set, then wakes it from pipe/socket state change or timer deadline.
No busy loop or false readiness result is exposed to userspace.

Official musl 1.2.6 maps Linux AArch64 `epoll_create1`, `epoll_ctl`, and
`epoll_pwait` into this ABI. Per-task signal masks survive clone and masked-wait
syscall retry. `ppoll`/`epoll_pwait` install temporary masks atomically, return
`EINTR`, run a real handler, then restore the original mask. Native syscall 109
adds bounded `pselect` read/write/exception fd sets with same atomic mask and
scheduler wait. HVF runtime proves fd-set mutation, timeout clearing, `EBADF`,
pipe wake, signal interruption, level repeat, edge transition, one-shot rearm,
UDP readiness after a real DNS exchange, and asynchronous TCP readiness after
a real HTTP exchange through virtio-net.

Current limits are explicit: network RX uses a bounded timer bottom half rather
than its device IRQ, nested epoll is rejected, four instances/16 watches are
fixed, and sole-runnable-task waits return `EAGAIN` because kernel lacks a
dedicated idle scheduling context.
