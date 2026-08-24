use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

pub const AF_INET: u64 = 2;
pub const SOCK_STREAM: u64 = 1;
pub const SOCK_DGRAM: u64 = 2;
pub const IPPROTO_TCP: u64 = 6;
pub const IPPROTO_UDP: u64 = 17;
const MAX_SOCKETS: usize = 4;
const RECEIVE_CAPACITY: usize = 512;

#[derive(Clone, Copy)]
struct Socket {
    used: bool,
    generation: u32,
    owner_pid: u64,
    kind: u8,
    protocol: u8,
    connected: bool,
    remote_ip: [u8; 4],
    remote_port: u16,
    local_port: u16,
    receive: [u8; RECEIVE_CAPACITY],
    receive_length: usize,
    receive_offset: usize,
}

impl Socket {
    const EMPTY: Self = Self {
        used: false,
        generation: 0,
        owner_pid: 0,
        kind: 0,
        protocol: 0,
        connected: false,
        remote_ip: [0; 4],
        remote_port: 0,
        local_port: 0,
        receive: [0; RECEIVE_CAPACITY],
        receive_length: 0,
        receive_offset: 0,
    };
}

struct State {
    sockets: [Socket; MAX_SOCKETS],
    next_generation: u32,
}

struct LockedState {
    lock: AtomicBool,
    state: UnsafeCell<State>,
}

unsafe impl Sync for LockedState {}

static STATE: LockedState = LockedState {
    lock: AtomicBool::new(false),
    state: UnsafeCell::new(State {
        sockets: [Socket::EMPTY; MAX_SOCKETS],
        next_generation: 1,
    }),
};

pub fn create(domain: u64, kind: u64, protocol: u64) -> Option<u64> {
    if domain != AF_INET
        || !matches!(
            (kind, protocol),
            (SOCK_DGRAM, IPPROTO_UDP) | (SOCK_STREAM, IPPROTO_TCP)
        )
    {
        return None;
    }
    let owner_pid = crate::scheduler::current_pid();
    with_state(|state| {
        let index = state.sockets.iter().position(|socket| !socket.used)?;
        let generation = state.next_generation.max(1);
        state.next_generation = generation.wrapping_add(1).max(1);
        state.sockets[index] = Socket {
            used: true,
            generation,
            owner_pid,
            kind: kind as u8,
            protocol: protocol as u8,
            local_port: if kind == SOCK_DGRAM {
                49_154 + index as u16
            } else {
                49_155 + index as u16
            },
            ..Socket::EMPTY
        };
        Some(encode_handle(index, generation))
    })
}

pub fn connect(handle: u64, remote_ip: [u8; 4], remote_port: u16) -> bool {
    if remote_ip == [0; 4] || remote_ip == [255; 4] || remote_port == 0 {
        return false;
    }
    with_state(|state| {
        let Some(index) = resolve_index(state, handle) else {
            return false;
        };
        let socket = &mut state.sockets[index];
        socket.remote_ip = remote_ip;
        socket.remote_port = remote_port;
        socket.connected = true;
        socket.receive.fill(0);
        socket.receive_length = 0;
        socket.receive_offset = 0;
        true
    })
}

pub fn send(handle: u64, payload: &[u8]) -> Option<usize> {
    if payload.is_empty() || payload.len() > RECEIVE_CAPACITY {
        return None;
    }
    let socket = with_state(|state| {
        let index = resolve_index(state, handle)?;
        let socket = state.sockets[index];
        socket.connected.then_some(socket)
    })?;
    let mut response = [0u8; RECEIVE_CAPACITY];
    let count = match (u64::from(socket.kind), u64::from(socket.protocol)) {
        (SOCK_DGRAM, IPPROTO_UDP) => crate::drivers::rtl8139::udp_exchange(
            socket.remote_ip,
            socket.remote_port,
            socket.local_port,
            payload,
            &mut response,
        )?,
        (SOCK_STREAM, IPPROTO_TCP) => crate::drivers::rtl8139::tcp_exchange(
            socket.remote_ip,
            socket.remote_port,
            socket.local_port,
            payload,
            &mut response,
        )?,
        _ => return None,
    };
    let stored = with_state(|state| {
        let index = resolve_index(state, handle)?;
        let current = &mut state.sockets[index];
        if current.generation != socket.generation || !current.connected {
            return None;
        }
        current.receive.fill(0);
        current.receive[..count].copy_from_slice(&response[..count]);
        current.receive_length = count;
        current.receive_offset = 0;
        Some(())
    });
    stored.map(|()| payload.len())
}

pub fn receive(handle: u64, output: &mut [u8]) -> Option<usize> {
    if output.is_empty() {
        return None;
    }
    with_state(|state| {
        let index = resolve_index(state, handle)?;
        let socket = &mut state.sockets[index];
        if !socket.connected {
            return None;
        }
        let count = output
            .len()
            .min(socket.receive_length.saturating_sub(socket.receive_offset));
        output[..count]
            .copy_from_slice(&socket.receive[socket.receive_offset..socket.receive_offset + count]);
        socket.receive_offset += count;
        Some(count)
    })
}

pub fn close(handle: u64) -> bool {
    with_state(|state| {
        let Some(index) = resolve_index(state, handle) else {
            return false;
        };
        state.sockets[index] = Socket::EMPTY;
        true
    })
}

pub fn close_all(pid: u64) -> usize {
    with_state(|state| {
        let mut closed = 0;
        for socket in &mut state.sockets {
            if socket.used && socket.owner_pid == pid {
                *socket = Socket::EMPTY;
                closed += 1;
            }
        }
        closed
    })
}

fn resolve_index(state: &State, handle: u64) -> Option<usize> {
    let index = usize::try_from(handle & 0xff).ok()?.checked_sub(1)?;
    let generation = u32::try_from(handle >> 8).ok()?;
    let socket = *state.sockets.get(index)?;
    (socket.used
        && socket.generation == generation
        && socket.owner_pid == crate::scheduler::current_pid())
    .then_some(index)
}

fn encode_handle(index: usize, generation: u32) -> u64 {
    (u64::from(generation) << 8) | (index as u64 + 1)
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
