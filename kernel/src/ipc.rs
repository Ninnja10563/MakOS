use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

const MAX_CHANNELS: usize = 8;
const MAX_EVENTS: usize = 8;
const MAX_HANDLES: usize = 32;
const QUEUE_CAPACITY: usize = 16;
const RIGHT_SEND: u8 = 1;
const RIGHT_RECEIVE: u8 = 2;
const RIGHT_SIGNAL: u8 = 4;
const RIGHT_WAIT: u8 = 8;
const KIND_CHANNEL: u8 = 1;
const KIND_EVENT: u8 = 2;

#[derive(Clone, Copy)]
struct Handle {
    used: bool,
    kind: u8,
    object: u8,
    side: u8,
    rights: u8,
    owner_pid: u64,
}

impl Handle {
    const EMPTY: Self = Self {
        used: false,
        kind: 0,
        object: 0,
        side: 0,
        rights: 0,
        owner_pid: 0,
    };
}

#[derive(Clone, Copy)]
struct Queue {
    data: [u64; QUEUE_CAPACITY],
    head: u8,
    length: u8,
}

impl Queue {
    const EMPTY: Self = Self {
        data: [0; QUEUE_CAPACITY],
        head: 0,
        length: 0,
    };

    fn push(&mut self, value: u64) -> bool {
        if self.length as usize == QUEUE_CAPACITY {
            return false;
        }
        let tail = (self.head as usize + self.length as usize) % QUEUE_CAPACITY;
        self.data[tail] = value;
        self.length += 1;
        true
    }

    fn pop(&mut self) -> Option<u64> {
        if self.length == 0 {
            return None;
        }
        let value = self.data[self.head as usize];
        self.head = ((self.head as usize + 1) % QUEUE_CAPACITY) as u8;
        self.length -= 1;
        Some(value)
    }
}

#[derive(Clone, Copy)]
struct Channel {
    used: bool,
    inbound: [Queue; 2],
}

impl Channel {
    const EMPTY: Self = Self {
        used: false,
        inbound: [Queue::EMPTY; 2],
    };
}

#[derive(Clone, Copy)]
struct Event {
    used: bool,
    signaled: bool,
    owner_pid: u64,
    waiter_tid: u64,
}

impl Event {
    const EMPTY: Self = Self {
        used: false,
        signaled: false,
        owner_pid: 0,
        waiter_tid: 0,
    };
}

struct State {
    handles: [Handle; MAX_HANDLES],
    channels: [Channel; MAX_CHANNELS],
    events: [Event; MAX_EVENTS],
}

struct LockedState {
    lock: AtomicBool,
    state: UnsafeCell<State>,
}

unsafe impl Sync for LockedState {}

static STATE: LockedState = LockedState {
    lock: AtomicBool::new(false),
    state: UnsafeCell::new(State {
        handles: [Handle::EMPTY; MAX_HANDLES],
        channels: [Channel::EMPTY; MAX_CHANNELS],
        events: [Event::EMPTY; MAX_EVENTS],
    }),
};

pub fn create_pair() -> Option<(u64, u64)> {
    let owner_pid = current_pid();
    with_state(|state| {
        let channel = state.channels.iter().position(|entry| !entry.used)?;
        let first = state.handles.iter().position(|entry| !entry.used)?;
        state.handles[first].used = true;
        let second = state
            .handles
            .iter()
            .enumerate()
            .find(|(index, entry)| *index != first && !entry.used)
            .map(|(index, _)| index)?;
        state.channels[channel] = Channel {
            used: true,
            inbound: [Queue::EMPTY; 2],
        };
        state.handles[first] = Handle {
            used: true,
            kind: KIND_CHANNEL,
            object: channel as u8,
            side: 0,
            rights: RIGHT_SEND | RIGHT_RECEIVE,
            owner_pid,
        };
        state.handles[second] = Handle {
            used: true,
            kind: KIND_CHANNEL,
            object: channel as u8,
            side: 1,
            rights: RIGHT_SEND | RIGHT_RECEIVE,
            owner_pid,
        };
        Some(((first + 1) as u64, (second + 1) as u64))
    })
}

pub fn send(handle: u64, value: u64) -> bool {
    with_state(|state| {
        let Some(entry) = resolve_handle(state, handle) else {
            return false;
        };
        if entry.kind != KIND_CHANNEL || entry.rights & RIGHT_SEND == 0 {
            return false;
        }
        let channel = entry.object as usize;
        let destination = (entry.side ^ 1) as usize;
        state.channels[channel].inbound[destination].push(value)
    })
}

pub fn receive(handle: u64) -> Option<u64> {
    with_state(|state| {
        let entry = resolve_handle(state, handle)?;
        if entry.kind != KIND_CHANNEL || entry.rights & RIGHT_RECEIVE == 0 {
            return None;
        }
        state.channels[entry.object as usize].inbound[entry.side as usize].pop()
    })
}

pub fn create_event(initially_signaled: bool) -> Option<u64> {
    let owner_pid = current_pid();
    with_state(|state| {
        let event = state.events.iter().position(|entry| !entry.used)?;
        let handle = state.handles.iter().position(|entry| !entry.used)?;
        state.events[event] = Event {
            used: true,
            signaled: initially_signaled,
            owner_pid,
            waiter_tid: 0,
        };
        state.handles[handle] = Handle {
            used: true,
            kind: KIND_EVENT,
            object: event as u8,
            side: 0,
            rights: RIGHT_SIGNAL | RIGHT_WAIT,
            owner_pid,
        };
        Some((handle + 1) as u64)
    })
}

pub fn signal_event(handle: u64) -> bool {
    with_state(|state| {
        let Some(entry) = resolve_handle(state, handle) else {
            return false;
        };
        if entry.kind != KIND_EVENT || entry.rights & RIGHT_SIGNAL == 0 {
            return false;
        }
        let event = &mut state.events[entry.object as usize];
        if !event.used || event.owner_pid != entry.owner_pid {
            return false;
        }
        if event.waiter_tid == 0 {
            event.signaled = true;
        } else {
            let waiter_tid = event.waiter_tid;
            event.waiter_tid = 0;
            if !wake_task(event.owner_pid, waiter_tid) {
                event.signaled = true;
            }
        }
        true
    })
}

fn prepare_event_wait(handle: u64) -> Option<bool> {
    let should_block = with_state(|state| {
        let entry = resolve_handle(state, handle)?;
        if entry.kind != KIND_EVENT || entry.rights & RIGHT_WAIT == 0 {
            return None;
        }
        let event = &mut state.events[entry.object as usize];
        if !event.used || event.owner_pid != entry.owner_pid {
            return None;
        }
        if event.signaled {
            event.signaled = false;
            Some(false)
        } else if event.waiter_tid == 0 {
            event.waiter_tid = current_tid();
            Some(true)
        } else {
            None
        }
    });
    should_block
}

#[cfg(target_arch = "x86_64")]
pub fn wait_event(handle: u64) -> bool {
    let Some(should_block) = prepare_event_wait(handle) else {
        return false;
    };
    if should_block {
        crate::scheduler::block_current();
    }
    true
}

#[cfg(target_arch = "aarch64")]
pub fn wait_event_from_exception(handle: u64, frame: &mut crate::arch::ExceptionFrame) -> bool {
    let Some(should_block) = prepare_event_wait(handle) else {
        return false;
    };
    if !should_block {
        return true;
    }
    let tid = current_tid();
    if crate::aarch64_process::block_current_for_ipc(frame) {
        crate::serial_println!(
            "MAKOS_AARCH64_IPC_WAIT_OK pid={} tid={} handle={} state=blocked wake=event",
            current_pid(),
            tid,
            handle,
        );
        true
    } else {
        with_state(|state| {
            for event in &mut state.events {
                if event.used && event.waiter_tid == tid {
                    event.waiter_tid = 0;
                }
            }
        });
        false
    }
}

pub fn close(handle: u64) -> bool {
    if handle & TYPED_HANDLE_MARKER != 0 {
        return typed_close(handle);
    }
    with_state(|state| {
        let Some(index) = handle_index(handle) else {
            return false;
        };
        let entry = state.handles[index];
        if !entry.used || entry.owner_pid != current_pid() {
            return false;
        }
        state.handles[index] = Handle::EMPTY;
        let still_referenced = state.handles.iter().any(|candidate| {
            candidate.used && candidate.kind == entry.kind && candidate.object == entry.object
        });
        if !still_referenced {
            match entry.kind {
                KIND_CHANNEL => state.channels[entry.object as usize] = Channel::EMPTY,
                KIND_EVENT => state.events[entry.object as usize] = Event::EMPTY,
                _ => return false,
            }
        }
        true
    })
}

pub fn close_all(pid: u64) -> usize {
    let legacy = with_state(|state| {
        let mut closed = 0usize;
        for handle in &mut state.handles {
            if handle.used && handle.owner_pid == pid {
                *handle = Handle::EMPTY;
                closed += 1;
            }
        }
        for index in 0..MAX_CHANNELS {
            if state.channels[index].used
                && !state.handles.iter().any(|handle| {
                    handle.used && handle.kind == KIND_CHANNEL && handle.object as usize == index
                })
            {
                state.channels[index] = Channel::EMPTY;
            }
        }
        for index in 0..MAX_EVENTS {
            if state.events[index].used
                && !state.handles.iter().any(|handle| {
                    handle.used && handle.kind == KIND_EVENT && handle.object as usize == index
                })
            {
                state.events[index] = Event::EMPTY;
            }
        }
        closed
    });
    let typed = u32::try_from(pid)
        .ok()
        .map(|pid| with_typed_state(|state| state.cleanup_pid(pid).handles_closed))
        .unwrap_or(0);
    legacy + typed
}

fn resolve_handle(state: &State, handle: u64) -> Option<Handle> {
    let index = handle_index(handle)?;
    let entry = *state.handles.get(index)?;
    (entry.used && entry.owner_pid == current_pid()).then_some(entry)
}

fn current_pid() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        crate::scheduler::current_pid()
    }
    #[cfg(target_arch = "aarch64")]
    {
        crate::aarch64_process::current_pid()
    }
}

fn current_tid() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        crate::scheduler::current_tid()
    }
    #[cfg(target_arch = "aarch64")]
    {
        crate::aarch64_process::current_tid()
    }
}

fn wake_task(pid: u64, tid: u64) -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        crate::scheduler::wake(pid, tid)
    }
    #[cfg(target_arch = "aarch64")]
    {
        crate::aarch64_process::wake_task(pid, tid)
    }
}

fn handle_index(handle: u64) -> Option<usize> {
    let index = usize::try_from(handle).ok()?.checked_sub(1)?;
    (index < MAX_HANDLES).then_some(index)
}

fn with_state<R>(function: impl FnOnce(&mut State) -> R) -> R {
    while STATE
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *STATE.state.get() });
    STATE.lock.store(false, Ordering::Release);
    result
}

const TYPED_HANDLE_MARKER: u64 = 1 << 63;
const TYPED_HANDLE_SLOT_MASK: u64 = 0xffff;
const TYPED_HANDLE_GENERATION_MASK: u64 = (1 << 47) - 1;
const TYPED_HANDLE_CAPACITY: usize = 64;
const TYPED_OBJECT_CAPACITY: usize = 64;

struct TypedLockedState {
    lock: AtomicBool,
    state: UnsafeCell<makos_ipc::IpcCore<TYPED_HANDLE_CAPACITY, TYPED_OBJECT_CAPACITY>>,
}

unsafe impl Sync for TypedLockedState {}

static TYPED_STATE: TypedLockedState = TypedLockedState {
    lock: AtomicBool::new(false),
    state: UnsafeCell::new(makos_ipc::IpcCore::new()),
};

pub fn typed_publish(name: &[u8]) -> Option<u64> {
    let owner = typed_identity()?;
    with_typed_state(|state| state.publish(owner, name).ok().map(encode_typed_handle))
}

pub fn typed_connect(name: &[u8]) -> Option<u64> {
    let client = typed_identity()?;
    with_typed_state(|state| state.connect(client, name).ok().map(encode_typed_handle))
}

pub fn typed_accept(listener: u64) -> Option<u64> {
    let provider = typed_identity()?;
    let listener = decode_typed_handle(listener)?;
    with_typed_state(|state| {
        state
            .accept(provider, listener)
            .ok()
            .map(encode_typed_handle)
    })
}

pub fn typed_send(
    endpoint: u64,
    bytes: [u8; makos_ipc::MESSAGE_WIRE_SIZE],
    transfer_handle: u64,
    transfer_rights: u8,
) -> bool {
    let Some(sender) = typed_identity() else {
        return false;
    };
    let Some(endpoint) = decode_typed_handle(endpoint) else {
        return false;
    };
    let Ok(message) = makos_ipc::WireMessage::from_bytes(bytes) else {
        return false;
    };
    let transfer = if transfer_handle == 0 {
        if transfer_rights != 0 {
            return false;
        }
        None
    } else {
        let Some(handle) = decode_typed_handle(transfer_handle) else {
            return false;
        };
        let Some(rights) = makos_ipc::Rights::from_bits(transfer_rights) else {
            return false;
        };
        Some(makos_ipc::Transfer { handle, rights })
    };
    with_typed_state(|state| state.send(sender, endpoint, message, transfer).is_ok())
}

pub fn typed_receive(endpoint: u64) -> Option<([u8; makos_ipc::MESSAGE_WIRE_SIZE], u64)> {
    let receiver = typed_identity()?;
    let endpoint = decode_typed_handle(endpoint)?;
    with_typed_state(|state| {
        let received = state.receive(receiver, endpoint).ok()?;
        Some((
            received.message.to_bytes(),
            received.transferred.map(encode_typed_handle).unwrap_or(0),
        ))
    })
}

fn typed_close(handle: u64) -> bool {
    let Some(owner) = typed_identity() else {
        return false;
    };
    let Some(handle) = decode_typed_handle(handle) else {
        return false;
    };
    with_typed_state(|state| state.close(owner, handle).is_ok())
}

fn typed_identity() -> Option<makos_ipc::Identity> {
    let pid = u32::try_from(current_pid()).ok()?;
    let credentials = crate::security::credentials();
    Some(makos_ipc::Identity::new(
        pid,
        credentials.uid,
        crate::security::session_generation(),
    ))
}

fn encode_typed_handle(handle: makos_ipc::Handle) -> u64 {
    TYPED_HANDLE_MARKER
        | ((handle.generation() & TYPED_HANDLE_GENERATION_MASK) << 16)
        | (handle.slot() as u64 + 1)
}

fn decode_typed_handle(value: u64) -> Option<makos_ipc::Handle> {
    if value & TYPED_HANDLE_MARKER == 0 {
        return None;
    }
    let slot = usize::try_from(value & TYPED_HANDLE_SLOT_MASK)
        .ok()?
        .checked_sub(1)?;
    let generation = (value >> 16) & TYPED_HANDLE_GENERATION_MASK;
    if slot >= TYPED_HANDLE_CAPACITY || generation == 0 {
        return None;
    }
    Some(makos_ipc::Handle::from_parts(slot, generation))
}

fn with_typed_state<R>(
    function: impl FnOnce(&mut makos_ipc::IpcCore<TYPED_HANDLE_CAPACITY, TYPED_OBJECT_CAPACITY>) -> R,
) -> R {
    while TYPED_STATE
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *TYPED_STATE.state.get() });
    TYPED_STATE.lock.store(false, Ordering::Release);
    result
}
