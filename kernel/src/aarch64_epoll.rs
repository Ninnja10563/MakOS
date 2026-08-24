//! Process-owned epoll instances over MakOS descriptors and socket handles.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use makos_readiness::{Control, Error, Event, Table};

const MAX_INSTANCES: usize = 4;
const MAX_WATCHES: usize = 16;
const FIREFOX_EPOLL_TRACE_LIMIT: u64 = 8;

struct LockedState {
    lock: AtomicBool,
    table: UnsafeCell<Table<MAX_INSTANCES, MAX_WATCHES>>,
}

unsafe impl Sync for LockedState {}

static STATE: LockedState = LockedState {
    lock: AtomicBool::new(false),
    table: UnsafeCell::new(Table::new()),
};
static REPORTED: AtomicBool = AtomicBool::new(false);
static FIREFOX_EPOLL_TRACES: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

pub(crate) fn create(close_on_exec: bool) -> Result<u64, Error> {
    let result = with_table(|table| table.create(current_pid(), close_on_exec));
    if result.is_ok() && !REPORTED.swap(true, Ordering::AcqRel) {
        crate::serial_println!(
            "MAKOS_AARCH64_EPOLL_OK instances=4 watches=16 level=1 edge=1 oneshot=1 wait=scheduler-blocked targets=pipe,socket"
        );
    }
    result
}

pub(crate) fn control(
    handle: u64,
    operation: u64,
    target: i32,
    event: Option<Event>,
) -> Result<(), Error> {
    let operation = match operation {
        1 => Control::Add,
        2 => Control::Delete,
        3 => Control::Modify,
        _ => return Err(Error::Invalid),
    };
    let result = with_table(|table| {
        table.control(
            current_pid(),
            handle,
            operation,
            target,
            event,
            valid_target,
        )
    });
    if crate::aarch64_process::current_app_role() == crate::aarch64_process::ProcessRole::Firefox
        && FIREFOX_EPOLL_TRACES.fetch_add(1, Ordering::AcqRel) < FIREFOX_EPOLL_TRACE_LIMIT
    {
        crate::serial_println!(
            "MAKOS_FIREFOX_EPOLL_CONTROL owner={} epoll={:#x} op={} target={} events={:#x} data={:#x} result={:?}",
            current_pid(),
            handle,
            operation as u8,
            target,
            event.map_or(0, |value| value.events),
            event.map_or(0, |value| value.data),
            result,
        );
    }
    result
}

pub(crate) fn collect(handle: u64, output: &mut [Event]) -> Result<usize, Error> {
    let result = with_table(|table| table.collect(current_pid(), handle, output, target_readiness));
    if let Ok(count) = result
        && count != 0
        && crate::aarch64_process::current_app_role()
            == crate::aarch64_process::ProcessRole::Firefox
        && FIREFOX_EPOLL_TRACES.fetch_add(1, Ordering::AcqRel) < FIREFOX_EPOLL_TRACE_LIMIT
    {
        crate::serial_println!(
            "MAKOS_FIREFOX_EPOLL_COLLECT owner={} epoll={:#x} count={} events={:#x} data={:#x}",
            current_pid(),
            handle,
            count,
            output[0].events,
            output[0].data,
        );
    }
    result
}

pub(crate) fn close(handle: u64) -> bool {
    with_table(|table| table.close(current_pid(), handle).is_ok())
}

pub(crate) fn close_all(pid: u64) -> usize {
    with_table(|table| table.close_owner(pid))
}

pub(crate) fn close_target(pid: u64, target: u64) -> usize {
    let Ok(target) = i32::try_from(target) else {
        return 0;
    };
    let removed = with_table(|table| table.remove_target(pid, target));
    if crate::aarch64_process::current_app_role() == crate::aarch64_process::ProcessRole::Firefox
        && FIREFOX_EPOLL_TRACES.fetch_add(1, Ordering::AcqRel) < FIREFOX_EPOLL_TRACE_LIMIT
    {
        crate::serial_println!(
            "MAKOS_FIREFOX_EPOLL_REMOVE owner={} target={} removed={}",
            pid,
            target,
            removed,
        );
    }
    removed
}

fn valid_target(target: i32) -> bool {
    if target < 0 {
        return false;
    }
    if target <= 2 || crate::aarch64_socket::is_owned(target as u64) {
        return true;
    }
    crate::vfs::poll_events(target as u64, 0) & 0x020 == 0
}

fn target_readiness(target: i32, requested: u32) -> u32 {
    if target < 0 {
        return 0x020;
    }
    if matches!(target, 1 | 2) {
        return requested & makos_readiness::EPOLLOUT;
    }
    if target == 0 {
        return 0;
    }
    if crate::aarch64_socket::is_owned(target as u64) {
        return crate::aarch64_socket::poll_events(target as u64, requested);
    }
    u32::from(crate::vfs::poll_events(target as u64, requested as u16))
}

fn current_pid() -> u64 {
    crate::aarch64_process::current_pid()
}

fn with_table<R>(function: impl FnOnce(&mut Table<MAX_INSTANCES, MAX_WATCHES>) -> R) -> R {
    while STATE
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *STATE.table.get() });
    STATE.lock.store(false, Ordering::Release);
    result
}

pub(crate) const fn error_abi(error: Error) -> u64 {
    let errno = match error {
        Error::Full => 28i64,
        Error::NotFound => 2,
        Error::Exists => 17,
        Error::Invalid => 22,
        Error::Permission => 1,
    };
    (-errno) as u64
}
