use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

pub const CAPACITY: usize = 64 * 1024;

struct Clipboard {
    uid: u32,
    length: usize,
    bytes: [u8; CAPACITY],
}

struct LockedClipboard {
    lock: AtomicBool,
    value: UnsafeCell<Clipboard>,
}

unsafe impl Sync for LockedClipboard {}

static CLIPBOARD: LockedClipboard = LockedClipboard {
    lock: AtomicBool::new(false),
    value: UnsafeCell::new(Clipboard {
        uid: u32::MAX,
        length: 0,
        bytes: [0; CAPACITY],
    }),
};

pub fn write(input: &[u8]) -> Result<usize, ()> {
    if input.len() > CAPACITY || !crate::security::has_capability(crate::security::CAP_INPUT) {
        return Err(());
    }
    let uid = crate::security::credentials().uid;
    with_clipboard(|clipboard| {
        clipboard.bytes.fill(0);
        clipboard.bytes[..input.len()].copy_from_slice(input);
        clipboard.uid = uid;
        clipboard.length = input.len();
    });
    Ok(input.len())
}

pub fn read(output: &mut [u8]) -> Result<usize, ()> {
    if !crate::security::has_capability(crate::security::CAP_INPUT) {
        return Err(());
    }
    let uid = crate::security::credentials().uid;
    with_clipboard(|clipboard| {
        if clipboard.uid != uid || output.len() < clipboard.length {
            return Err(());
        }
        output[..clipboard.length].copy_from_slice(&clipboard.bytes[..clipboard.length]);
        Ok(clipboard.length)
    })
}

/// Copy as much clipboard data as fits. Terminal paste uses a deliberately
/// small input queue, so requiring a 64 KiB userspace-sized destination would
/// turn an otherwise valid clipboard into an all-or-nothing paste failure.
pub fn read_prefix(output: &mut [u8]) -> Result<usize, ()> {
    if !crate::security::has_capability(crate::security::CAP_INPUT) {
        return Err(());
    }
    let uid = crate::security::credentials().uid;
    with_clipboard(|clipboard| {
        if clipboard.uid != uid {
            return Err(());
        }
        let count = output.len().min(clipboard.length);
        output[..count].copy_from_slice(&clipboard.bytes[..count]);
        Ok(count)
    })
}

pub fn clear() {
    with_clipboard(|clipboard| {
        clipboard.bytes.fill(0);
        clipboard.length = 0;
        clipboard.uid = u32::MAX;
    });
}

fn with_clipboard<R>(function: impl FnOnce(&mut Clipboard) -> R) -> R {
    while CLIPBOARD
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *CLIPBOARD.value.get() });
    CLIPBOARD.lock.store(false, Ordering::Release);
    result
}
