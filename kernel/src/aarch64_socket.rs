//! Process-owned AArch64 AF_INET/AF_INET6 sockets backed by native virtio-net.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub const AF_INET: u64 = 2;
pub const AF_INET6: u64 = 10;
pub const SOCK_STREAM: u64 = 1;
pub const SOCK_DGRAM: u64 = 2;
pub const IPPROTO_TCP: u64 = 6;
pub const IPPROTO_UDP: u64 = 17;
pub const SOCK_NONBLOCK: u64 = 0x800;
pub const SOCK_CLOEXEC: u64 = 0x80000;

const SOCK_TYPE_MASK: u64 = 0xf;

// Firefox opens parallel DNS, speculative, settings, and page connections.
// Eight global sockets caused ordinary HTTPS navigation to fail under normal
// browser startup load. Keep bounded storage, but size it like a desktop OS.
const MAX_SOCKETS: usize = 128;
const SOCKET_HANDLE_FIRST: u64 = 0x101;
const SOCKET_HANDLE_LAST: u64 = 0x3ff;
const UDP_RESPONSE_CAPACITY: usize = 1536;
const UDP_QUEUE_DEPTH: usize = 2;
const TCP_RECEIVE_CAPACITY: usize = 32_768;
const PUMP_FRAMES: usize = 16;
static TCP_ASYNC_REPORTED: AtomicBool = AtomicBool::new(false);
static FIREFOX_SOCKET_CREATES: AtomicU64 = AtomicU64::new(0);
static FIREFOX_SOCKET_FAILURES: AtomicU64 = AtomicU64::new(0);
static FIREFOX_SOCKET_POLLS: AtomicU64 = AtomicU64::new(0);
static FIREFOX_SOCKET_RECEIVES: AtomicU64 = AtomicU64::new(0);
const FIREFOX_SOCKET_TRACE_LIMIT: u64 = 8;
static UDP_DNS_TRACES: AtomicU64 = AtomicU64::new(0);
static IPV6_SOCKET_REPORTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpAddress {
    V4([u8; 4]),
    V6([u8; 16]),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Endpoint {
    pub address: IpAddress,
    pub port: u16,
}

#[derive(Clone, Copy)]
struct Socket {
    used: bool,
    generation: u32,
    handle: u64,
    owner_pid: u64,
    domain: u8,
    kind: u8,
    protocol: u8,
    connected: bool,
    nonblocking: bool,
    close_on_exec: bool,
    remote_ip: [u8; 4],
    remote_ipv6: [u8; 16],
    remote_port: u16,
    local_port: u16,
    tcp: Option<crate::aarch64_virtio_net::TcpConnection>,
    tcp6: Option<crate::aarch64_virtio_net::Tcp6Connection>,
    tcp_length: usize,
    tcp_offset: usize,
    udp_responses: [[u8; UDP_RESPONSE_CAPACITY]; UDP_QUEUE_DEPTH],
    udp_lengths: [usize; UDP_QUEUE_DEPTH],
    udp_sender_ips: [[u8; 4]; UDP_QUEUE_DEPTH],
    udp_sender_ipv6: [[u8; 16]; UDP_QUEUE_DEPTH],
    udp_sender_ports: [u16; UDP_QUEUE_DEPTH],
    udp_head: usize,
    udp_count: usize,
}

impl Socket {
    const EMPTY: Self = Self {
        used: false,
        generation: 0,
        handle: 0,
        owner_pid: 0,
        domain: 0,
        kind: 0,
        protocol: 0,
        connected: false,
        nonblocking: false,
        close_on_exec: false,
        remote_ip: [0; 4],
        remote_ipv6: [0; 16],
        remote_port: 0,
        local_port: 0,
        tcp: None,
        tcp6: None,
        tcp_length: 0,
        tcp_offset: 0,
        udp_responses: [[0; UDP_RESPONSE_CAPACITY]; UDP_QUEUE_DEPTH],
        udp_lengths: [0; UDP_QUEUE_DEPTH],
        udp_sender_ips: [[0; 4]; UDP_QUEUE_DEPTH],
        udp_sender_ipv6: [[0; 16]; UDP_QUEUE_DEPTH],
        udp_sender_ports: [0; UDP_QUEUE_DEPTH],
        udp_head: 0,
        udp_count: 0,
    };

    fn remote_endpoint(self) -> Endpoint {
        Endpoint {
            address: if self.domain == AF_INET6 as u8 {
                IpAddress::V6(self.remote_ipv6)
            } else {
                IpAddress::V4(self.remote_ip)
            },
            port: self.remote_port,
        }
    }
}

fn valid_remote(address: IpAddress) -> bool {
    match address {
        IpAddress::V4(address) => address != [0; 4] && address != [255; 4],
        IpAddress::V6(address) => {
            // IPv4-mapped addresses require actual dual-stack translation;
            // reject them instead of claiming IPv4 traffic as IPv6.
            let mapped = address[..10] == [0; 10] && address[10..12] == [0xff, 0xff];
            address != [0; 16] && address[0] != 0xff && !mapped
        }
    }
}

struct State {
    sockets: [Socket; MAX_SOCKETS],
    tcp_responses: [[u8; TCP_RECEIVE_CAPACITY]; MAX_SOCKETS],
    next_generation: u32,
    next_handle: u64,
    next_port: u16,
}

struct LockedState {
    lock: AtomicBool,
    value: UnsafeCell<State>,
}

unsafe impl Sync for LockedState {}

static STATE: LockedState = LockedState {
    lock: AtomicBool::new(false),
    value: UnsafeCell::new(State {
        sockets: [Socket::EMPTY; MAX_SOCKETS],
        tcp_responses: [[0; TCP_RECEIVE_CAPACITY]; MAX_SOCKETS],
        next_generation: 1,
        next_handle: SOCKET_HANDLE_FIRST,
        next_port: 49_152,
    }),
};

pub fn create(domain: u64, kind: u64, protocol: u64) -> Option<u64> {
    let base_kind = kind & SOCK_TYPE_MASK;
    let protocol = match (base_kind, protocol) {
        (SOCK_DGRAM, 0 | IPPROTO_UDP) => IPPROTO_UDP,
        (SOCK_STREAM, 0 | IPPROTO_TCP) => IPPROTO_TCP,
        _ => return None,
    };
    if !matches!(domain, AF_INET | AF_INET6)
        || kind & !(SOCK_TYPE_MASK | SOCK_NONBLOCK | SOCK_CLOEXEC) != 0
    {
        return None;
    }
    let owner_pid = crate::aarch64_process::current_pid();
    let result = with_state(|state| {
        let index = state.sockets.iter().position(|socket| !socket.used)?;
        let generation = state.next_generation.max(1);
        state.next_generation = generation.wrapping_add(1).max(1);
        let mut handle = state
            .next_handle
            .clamp(SOCKET_HANDLE_FIRST, SOCKET_HANDLE_LAST);
        loop {
            if !state
                .sockets
                .iter()
                .any(|socket| socket.used && socket.handle == handle)
            {
                break;
            }
            handle = if handle == SOCKET_HANDLE_LAST {
                SOCKET_HANDLE_FIRST
            } else {
                handle + 1
            };
            if handle == state.next_handle {
                return None;
            }
        }
        state.next_handle = if handle == SOCKET_HANDLE_LAST {
            SOCKET_HANDLE_FIRST
        } else {
            handle + 1
        };
        let local_port = state.next_port.max(49_152);
        state.next_port = if local_port == 65_535 {
            49_152
        } else {
            local_port + 1
        };
        state.sockets[index] = Socket {
            used: true,
            generation,
            handle,
            owner_pid,
            domain: domain as u8,
            kind: base_kind as u8,
            protocol: protocol as u8,
            nonblocking: kind & SOCK_NONBLOCK != 0,
            close_on_exec: kind & SOCK_CLOEXEC != 0,
            local_port,
            ..Socket::EMPTY
        };
        state.tcp_responses[index].fill(0);
        Some(handle)
    });
    if crate::aarch64_process::current_app_role() == crate::aarch64_process::ProcessRole::Firefox {
        if let Some(handle) = result {
            let count = FIREFOX_SOCKET_CREATES.fetch_add(1, Ordering::AcqRel) + 1;
            if count <= FIREFOX_SOCKET_TRACE_LIMIT {
                crate::serial_println!(
                    "MAKOS_FIREFOX_SOCKET_CREATE_OK count={} handle={:#x} kind={} protocol={} capacity={}",
                    count,
                    handle,
                    base_kind,
                    protocol,
                    MAX_SOCKETS,
                );
            }
        } else {
            let failures = FIREFOX_SOCKET_FAILURES.fetch_add(1, Ordering::AcqRel) + 1;
            crate::serial_println!(
                "MAKOS_FIREFOX_SOCKET_CREATE_FAIL failures={} reason=table-full-or-invalid capacity={}",
                failures,
                MAX_SOCKETS,
            );
        }
    }
    if domain == AF_INET6 && result.is_some() && !IPV6_SOCKET_REPORTED.swap(true, Ordering::AcqRel)
    {
        crate::serial_println!(
            "MAKOS_AARCH64_AF_INET6_SOCKET_OK sockaddr_in6=28 v6only=1 fake_mapping=0"
        );
    }
    result
}

pub fn connect(handle: u64, remote_ip: [u8; 4], remote_port: u16) -> bool {
    connect_address(handle, IpAddress::V4(remote_ip), remote_port)
}

pub fn connect_address(handle: u64, address: IpAddress, remote_port: u16) -> bool {
    if remote_port == 0 || !valid_remote(address) {
        return false;
    }
    let identity = with_state(|state| {
        let index = resolve_index(state, handle)?;
        let socket = state.sockets[index];
        (!socket.connected).then_some((
            index,
            socket.generation,
            socket.kind,
            socket.protocol,
            socket.domain,
            socket.local_port,
        ))
    });
    let Some((index, generation, kind, protocol, domain, local_port)) = identity else {
        return false;
    };
    if (domain == AF_INET as u8) != matches!(address, IpAddress::V4(_)) {
        return false;
    }
    let mut tcp = None;
    let mut tcp6 = None;
    if (u64::from(kind), u64::from(protocol)) == (SOCK_STREAM, IPPROTO_TCP) {
        match address {
            IpAddress::V4(remote_ip) => {
                let Some(connection) =
                    crate::aarch64_virtio_net::tcp_connect(remote_ip, remote_port, local_port)
                else {
                    return false;
                };
                tcp = Some(connection);
            }
            IpAddress::V6(remote_ip) => {
                let Some(connection) =
                    crate::aarch64_virtio_net::tcp6_connect(remote_ip, remote_port, local_port)
                else {
                    return false;
                };
                tcp6 = Some(connection);
            }
        }
    };
    let connected = with_state(|state| {
        let Some(current) = state.sockets.get_mut(index) else {
            return false;
        };
        if !current.used
            || current.generation != generation
            || current.owner_pid != crate::aarch64_process::current_pid()
        {
            return false;
        }
        current.connected = true;
        match address {
            IpAddress::V4(remote_ip) => current.remote_ip = remote_ip,
            IpAddress::V6(remote_ip) => current.remote_ipv6 = remote_ip,
        }
        current.remote_port = remote_port;
        current.tcp = tcp;
        current.tcp6 = tcp6;
        current.udp_lengths = [0; UDP_QUEUE_DEPTH];
        current.udp_head = 0;
        current.udp_count = 0;
        true
    });
    if connected {
        crate::aarch64_process::wake_io_source(makos_readiness::WaitSource::Network(handle));
    }
    if crate::aarch64_process::current_app_role() == crate::aarch64_process::ProcessRole::Firefox {
        crate::serial_println!(
            "MAKOS_FIREFOX_SOCKET_CONNECT handle={:#x} remote={:?}:{} result={} fake_mapping=0",
            handle,
            address,
            remote_port,
            u8::from(connected),
        );
    }
    connected
}

pub fn send(handle: u64, payload: &[u8]) -> Option<usize> {
    send_to(handle, payload, None)
}

pub fn send_to(handle: u64, payload: &[u8], destination: Option<Endpoint>) -> Option<usize> {
    if payload.is_empty() || payload.len() > 64 * 1024 {
        return None;
    }
    let snapshot = with_state(|state| {
        let index = resolve_index(state, handle)?;
        let socket = state.sockets[index];
        (socket.connected || (u64::from(socket.kind) == SOCK_DGRAM && destination.is_some()))
            .then_some((index, socket))
    })?;
    let (index, mut socket) = snapshot;
    let result = match (u64::from(socket.kind), u64::from(socket.protocol)) {
        (SOCK_DGRAM, IPPROTO_UDP) => {
            let endpoint = destination.unwrap_or_else(|| socket.remote_endpoint());
            if endpoint.port == 0
                || !valid_remote(endpoint.address)
                || (socket.domain == AF_INET as u8) != matches!(endpoint.address, IpAddress::V4(_))
            {
                return None;
            }
            let trace = UDP_DNS_TRACES.fetch_add(1, Ordering::AcqRel);
            let query_id = payload
                .get(..2)
                .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
                .unwrap_or(0);
            let query_type = payload
                .get(payload.len().saturating_sub(4)..payload.len().saturating_sub(2))
                .filter(|_| payload.len() >= 4)
                .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
                .unwrap_or(0);
            if trace < 32 {
                crate::serial_println!(
                    "MAKOS_UDP_DNS_SEND trace={} id={:#06x} qtype={} local={} remote={:?}:{} queued={}",
                    trace,
                    query_id,
                    query_type,
                    socket.local_port,
                    endpoint.address,
                    endpoint.port,
                    socket.udp_count,
                );
            }
            let sent = match endpoint.address {
                IpAddress::V4(remote_ip) => crate::aarch64_virtio_net::udp_send(
                    remote_ip,
                    endpoint.port,
                    socket.local_port,
                    payload,
                ),
                IpAddress::V6(remote_ip) => crate::aarch64_virtio_net::udp6_send(
                    remote_ip,
                    endpoint.port,
                    socket.local_port,
                    payload,
                ),
            };
            let Some(count) = sent else {
                if trace < 32 {
                    crate::serial_println!(
                        "MAKOS_UDP_DNS_RESULT trace={} id={:#06x} qtype={} result=transport-failure",
                        trace,
                        query_id,
                        query_type,
                    );
                }
                return None;
            };
            if trace < 32 {
                crate::serial_println!(
                    "MAKOS_UDP_DNS_RESULT trace={} id={:#06x} qtype={} result=sent bytes={}",
                    trace,
                    query_id,
                    query_type,
                    count,
                );
            }
            Some(payload.len())
        }
        (SOCK_STREAM, IPPROTO_TCP) => {
            if socket.domain == AF_INET as u8 {
                crate::aarch64_virtio_net::tcp_send(socket.tcp.as_mut()?, payload)
            } else {
                crate::aarch64_virtio_net::tcp6_send(socket.tcp6.as_mut()?, payload)
            }
        }
        _ => None,
    }?;
    store_snapshot(index, socket)?;
    crate::aarch64_process::wake_io_source(makos_readiness::WaitSource::Network(handle));
    Some(result)
}

pub fn receive(handle: u64, output: &mut [u8]) -> Option<usize> {
    receive_from(handle, output).map(|result| result.0)
}

pub fn receive_from(handle: u64, output: &mut [u8]) -> Option<(usize, Endpoint)> {
    if output.is_empty() {
        return None;
    }
    let (index, mut socket) = with_state(|state| {
        let index = resolve_index(state, handle)?;
        let socket = state.sockets[index];
        (socket.connected || u64::from(socket.kind) == SOCK_DGRAM).then_some((index, socket))
    })?;
    if crate::aarch64_process::current_app_role() == crate::aarch64_process::ProcessRole::Firefox {
        let trace = FIREFOX_SOCKET_RECEIVES.fetch_add(1, Ordering::AcqRel);
        if trace < FIREFOX_SOCKET_TRACE_LIMIT {
            crate::serial_println!(
                "MAKOS_FIREFOX_SOCKET_RECEIVE trace={} handle={:#x} kind={} udp_queued={} tcp_bytes={}",
                trace,
                handle,
                socket.kind,
                socket.udp_count,
                socket.tcp_length.saturating_sub(socket.tcp_offset),
            );
        }
    }
    let (count, endpoint) = match (u64::from(socket.kind), u64::from(socket.protocol)) {
        (SOCK_DGRAM, IPPROTO_UDP) => {
            if socket.udp_count == 0 {
                return None;
            }
            let head = socket.udp_head;
            let count = output.len().min(socket.udp_lengths[head]);
            output[..count].copy_from_slice(&socket.udp_responses[head][..count]);
            let sender_ip = if socket.domain == AF_INET6 as u8 {
                IpAddress::V6(socket.udp_sender_ipv6[head])
            } else {
                IpAddress::V4(socket.udp_sender_ips[head])
            };
            let sender_port = socket.udp_sender_ports[head];
            let response_id = socket.udp_responses[head]
                .get(..2)
                .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
                .unwrap_or(0);
            if UDP_DNS_TRACES.load(Ordering::Acquire) <= 32 {
                crate::serial_println!(
                    "MAKOS_UDP_DNS_RECV id={:#06x} bytes={} sender={:?}:{} remaining={}",
                    response_id,
                    count,
                    sender_ip,
                    sender_port,
                    socket.udp_count - 1,
                );
            }
            socket.udp_lengths[head] = 0;
            socket.udp_head = (head + 1) % UDP_QUEUE_DEPTH;
            socket.udp_count -= 1;
            (
                count,
                Endpoint {
                    address: sender_ip,
                    port: sender_port,
                },
            )
        }
        (SOCK_STREAM, IPPROTO_TCP) => {
            let available = socket.tcp_length.saturating_sub(socket.tcp_offset);
            if available != 0 {
                let count = output.len().min(available);
                with_state(|state| {
                    let current = state.sockets.get(index)?;
                    if !current.used
                        || current.generation != socket.generation
                        || current.owner_pid != crate::aarch64_process::current_pid()
                    {
                        return None;
                    }
                    output[..count].copy_from_slice(
                        &state.tcp_responses[index][socket.tcp_offset..socket.tcp_offset + count],
                    );
                    Some(())
                })?;
                socket.tcp_offset += count;
                if socket.tcp_offset == socket.tcp_length {
                    socket.tcp_offset = 0;
                    socket.tcp_length = 0;
                }
                let free = TCP_RECEIVE_CAPACITY
                    .saturating_sub(socket.tcp_length.saturating_sub(socket.tcp_offset));
                if socket.domain == AF_INET6 as u8 {
                    let _ = crate::aarch64_virtio_net::tcp6_update_receive_window(
                        socket.tcp6.as_mut()?,
                        free,
                    );
                } else {
                    let _ = crate::aarch64_virtio_net::tcp_update_receive_window(
                        socket.tcp.as_mut()?,
                        free,
                    );
                }
                (count, socket.remote_endpoint())
            } else if socket.nonblocking {
                return None;
            } else if socket.domain == AF_INET6 as u8 {
                (
                    crate::aarch64_virtio_net::tcp6_receive(socket.tcp6.as_mut()?, output)?,
                    socket.remote_endpoint(),
                )
            } else {
                (
                    crate::aarch64_virtio_net::tcp_receive(socket.tcp.as_mut()?, output)?,
                    socket.remote_endpoint(),
                )
            }
        }
        _ => return None,
    };
    store_snapshot(index, socket)?;
    crate::aarch64_process::wake_io_source(makos_readiness::WaitSource::Network(handle));
    Some((count, endpoint))
}

/// Bounded timer-bottom-half RX demultiplexing for UDP and TCP sockets.
pub fn pump() -> usize {
    let mut frame = [0u8; crate::aarch64_net_wire::FRAME_CAPACITY];
    let mut progressed = 0usize;
    let mut progressed_handles = [0u64; PUMP_FRAMES];
    let mut progressed_handle_count = 0usize;
    for _ in 0..PUMP_FRAMES {
        let Some(length) = crate::aarch64_virtio_net::poll_frame(&mut frame) else {
            break;
        };
        if let Some(packet) = crate::aarch64_net_wire::parse_udp(&frame[..length]) {
            let local_ip = crate::aarch64_virtio_net::config()
                .map(|config| config.ipv4)
                .unwrap_or([0; 4]);
            if packet.destination_ip != local_ip && packet.destination_ip != [255; 4] {
                continue;
            }
            let queued = with_state(|state| {
                let index = state.sockets.iter().position(|socket| {
                    socket.used
                        && socket.domain == AF_INET as u8
                        && u64::from(socket.kind) == SOCK_DGRAM
                        && u64::from(socket.protocol) == IPPROTO_UDP
                        && socket.local_port == packet.destination_port
                        && (!socket.connected
                            || (socket.remote_ip == packet.source_ip
                                && socket.remote_port == packet.source_port))
                })?;
                if packet.payload.len() > UDP_RESPONSE_CAPACITY {
                    return None;
                }
                let socket = &mut state.sockets[index];
                // A full socket drops its oldest datagram. TX must never block
                // merely because userspace has not consumed earlier RX data.
                if socket.udp_count == UDP_QUEUE_DEPTH {
                    socket.udp_lengths[socket.udp_head] = 0;
                    socket.udp_head = (socket.udp_head + 1) % UDP_QUEUE_DEPTH;
                    socket.udp_count -= 1;
                }
                let tail = (socket.udp_head + socket.udp_count) % UDP_QUEUE_DEPTH;
                socket.udp_responses[tail][..packet.payload.len()].copy_from_slice(packet.payload);
                socket.udp_lengths[tail] = packet.payload.len();
                socket.udp_sender_ips[tail] = packet.source_ip;
                socket.udp_sender_ports[tail] = packet.source_port;
                socket.udp_count += 1;
                Some(socket.handle)
            });
            if let Some(handle) = queued {
                progressed += 1;
                if !progressed_handles[..progressed_handle_count].contains(&handle) {
                    progressed_handles[progressed_handle_count] = handle;
                    progressed_handle_count += 1;
                }
            }
            continue;
        }
        if let Some(packet) = crate::aarch64_net_wire::parse_udp_v6(&frame[..length]) {
            let local_ip = crate::aarch64_virtio_net::ipv6_config()
                .map(|config| config.address)
                .unwrap_or([0; 16]);
            if packet.destination_ip != local_ip {
                continue;
            }
            let queued = with_state(|state| {
                let index = state.sockets.iter().position(|socket| {
                    socket.used
                        && socket.domain == AF_INET6 as u8
                        && u64::from(socket.kind) == SOCK_DGRAM
                        && u64::from(socket.protocol) == IPPROTO_UDP
                        && socket.local_port == packet.destination_port
                        && (!socket.connected
                            || (socket.remote_ipv6 == packet.source_ip
                                && socket.remote_port == packet.source_port))
                })?;
                if packet.payload.len() > UDP_RESPONSE_CAPACITY {
                    return None;
                }
                let socket = &mut state.sockets[index];
                if socket.udp_count == UDP_QUEUE_DEPTH {
                    socket.udp_lengths[socket.udp_head] = 0;
                    socket.udp_head = (socket.udp_head + 1) % UDP_QUEUE_DEPTH;
                    socket.udp_count -= 1;
                }
                let tail = (socket.udp_head + socket.udp_count) % UDP_QUEUE_DEPTH;
                socket.udp_responses[tail][..packet.payload.len()].copy_from_slice(packet.payload);
                socket.udp_lengths[tail] = packet.payload.len();
                socket.udp_sender_ipv6[tail] = packet.source_ip;
                socket.udp_sender_ports[tail] = packet.source_port;
                socket.udp_count += 1;
                Some(socket.handle)
            });
            if let Some(handle) = queued {
                progressed += 1;
                if !progressed_handles[..progressed_handle_count].contains(&handle) {
                    progressed_handles[progressed_handle_count] = handle;
                    progressed_handle_count += 1;
                }
            }
            continue;
        }
        let ingress = if let Some(segment) = crate::aarch64_net_wire::parse_tcp(&frame[..length]) {
            with_state(|state| {
                let index = state.sockets.iter().position(|socket| {
                    socket.used
                        && socket.domain == AF_INET as u8
                        && socket.connected
                        && u64::from(socket.kind) == SOCK_STREAM
                        && u64::from(socket.protocol) == IPPROTO_TCP
                        && socket.remote_ip == segment.source_ip
                        && socket.remote_port == segment.source_port
                        && socket.local_port == segment.destination_port
                })?;
                let (sockets, responses) = (&mut state.sockets, &mut state.tcp_responses);
                let socket = &mut sockets[index];
                let response = &mut responses[index];
                if socket.tcp_offset != 0 {
                    let remaining = socket.tcp_length.saturating_sub(socket.tcp_offset);
                    response.copy_within(socket.tcp_offset..socket.tcp_length, 0);
                    socket.tcp_offset = 0;
                    socket.tcp_length = remaining;
                }
                let start = socket.tcp_length;
                let available = TCP_RECEIVE_CAPACITY.saturating_sub(start);
                let ingress = crate::aarch64_virtio_net::tcp_ingest(
                    socket.tcp.as_mut()?,
                    segment,
                    &mut response[start..start + available],
                )?;
                socket.tcp_length += ingress.length;
                Some((socket.handle, ingress))
            })
        } else if let Some(segment) = crate::aarch64_net_wire::parse_tcp_v6(&frame[..length]) {
            with_state(|state| {
                let index = state.sockets.iter().position(|socket| {
                    socket.used
                        && socket.domain == AF_INET6 as u8
                        && socket.connected
                        && u64::from(socket.kind) == SOCK_STREAM
                        && u64::from(socket.protocol) == IPPROTO_TCP
                        && socket.remote_ipv6 == segment.source_ip
                        && socket.remote_port == segment.source_port
                        && socket.local_port == segment.destination_port
                })?;
                let (sockets, responses) = (&mut state.sockets, &mut state.tcp_responses);
                let socket = &mut sockets[index];
                let response = &mut responses[index];
                if socket.tcp_offset != 0 {
                    let remaining = socket.tcp_length.saturating_sub(socket.tcp_offset);
                    response.copy_within(socket.tcp_offset..socket.tcp_length, 0);
                    socket.tcp_offset = 0;
                    socket.tcp_length = remaining;
                }
                let start = socket.tcp_length;
                let available = TCP_RECEIVE_CAPACITY.saturating_sub(start);
                let ingress = crate::aarch64_virtio_net::tcp6_ingest(
                    socket.tcp6.as_mut()?,
                    segment,
                    &mut response[start..start + available],
                )?;
                socket.tcp_length += ingress.length;
                Some((socket.handle, ingress))
            })
        } else {
            continue;
        };
        let Some((handle, ingress)) = ingress else {
            continue;
        };
        if ingress.length != 0 || ingress.closed {
            progressed += 1;
            if !progressed_handles[..progressed_handle_count].contains(&handle) {
                progressed_handles[progressed_handle_count] = handle;
                progressed_handle_count += 1;
            }
        }
    }
    if progressed != 0 {
        for handle in progressed_handles[..progressed_handle_count]
            .iter()
            .copied()
        {
            crate::aarch64_process::wake_io_source(makos_readiness::WaitSource::Network(handle));
        }
        if !TCP_ASYNC_REPORTED.swap(true, Ordering::AcqRel) {
            crate::serial_println!(
                "MAKOS_AARCH64_TCP_ASYNC_OK source=timer-rx buffer=32768 wake=poll,epoll bounded_frames=16"
            );
        }
    }
    progressed
}

pub fn close(handle: u64) -> bool {
    let owner_pid = crate::aarch64_process::current_pid();
    let removed = with_state(|state| {
        let index = resolve_index(state, handle)?;
        let socket = state.sockets[index];
        state.sockets[index] = Socket::EMPTY;
        Some(socket)
    });
    let Some(mut socket) = removed else {
        return false;
    };
    if let Some(connection) = socket.tcp.as_mut() {
        crate::aarch64_virtio_net::tcp_close(connection);
    }
    if let Some(connection) = socket.tcp6.as_mut() {
        crate::aarch64_virtio_net::tcp6_close(connection);
    }
    crate::aarch64_epoll::close_target(owner_pid, handle);
    crate::aarch64_process::wake_io_source(makos_readiness::WaitSource::Network(handle));
    true
}

pub fn close_all(pid: u64) -> usize {
    let mut count = 0;
    loop {
        // Never stage full Socket values here: each owns multi-KiB RX buffers,
        // while scheduler kernel stacks are deliberately only 16 KiB.
        let removed = with_state(|state| {
            let socket = state
                .sockets
                .iter_mut()
                .find(|socket| socket.used && socket.owner_pid == pid)?;
            let removed = (socket.handle, socket.tcp, socket.tcp6);
            *socket = Socket::EMPTY;
            Some(removed)
        });
        let Some((handle, mut tcp, mut tcp6)) = removed else {
            break;
        };
        count += 1;
        crate::aarch64_epoll::close_target(pid, handle);
        if let Some(connection) = tcp.as_mut() {
            crate::aarch64_virtio_net::tcp_close(connection);
        }
        if let Some(connection) = tcp6.as_mut() {
            crate::aarch64_virtio_net::tcp6_close(connection);
        }
    }
    count
}

pub fn close_on_exec(pid: u64) -> usize {
    let mut count = 0;
    loop {
        let removed = with_state(|state| {
            let socket = state
                .sockets
                .iter_mut()
                .find(|socket| socket.used && socket.owner_pid == pid && socket.close_on_exec)?;
            let removed = (socket.handle, socket.tcp, socket.tcp6);
            *socket = Socket::EMPTY;
            Some(removed)
        });
        let Some((handle, mut tcp, mut tcp6)) = removed else {
            break;
        };
        count += 1;
        crate::aarch64_epoll::close_target(pid, handle);
        if let Some(connection) = tcp.as_mut() {
            crate::aarch64_virtio_net::tcp_close(connection);
        }
        if let Some(connection) = tcp6.as_mut() {
            crate::aarch64_virtio_net::tcp6_close(connection);
        }
    }
    count
}

/// Socket controls used by fcntl plus bounded socket-option queries. Socket
/// duplication remains unsupported until sockets share ordinary FD table.
pub fn control(handle: u64, operation: u64, argument: u64) -> Result<u64, i64> {
    const FD_CLOEXEC: u64 = 1;
    const O_RDWR: u64 = 2;
    with_state(|state| {
        let index = resolve_index(state, handle).ok_or(-9)?;
        if operation == 16 {
            let bytes = argument.to_le_bytes();
            let family = u16::from_le_bytes([bytes[0], bytes[1]]);
            let port = u16::from_be_bytes([bytes[2], bytes[3]]);
            let ipv4 = [bytes[4], bytes[5], bytes[6], bytes[7]];
            let local_ip = crate::aarch64_virtio_net::config()
                .map(|config| config.ipv4)
                .unwrap_or([0; 4]);
            if family != AF_INET as u16 || (ipv4 != [0; 4] && ipv4 != local_ip) {
                return Err(-99);
            }
            if port != 0
                && state.sockets.iter().enumerate().any(|(candidate, other)| {
                    candidate != index && other.used && other.local_port == port
                })
            {
                return Err(-98);
            }
            let socket = &mut state.sockets[index];
            if port != 0 {
                socket.local_port = port;
            }
            return Ok(0);
        }
        let socket = &mut state.sockets[index];
        match operation {
            1 => Ok(u64::from(socket.close_on_exec) * FD_CLOEXEC),
            2 if argument & !FD_CLOEXEC == 0 => {
                socket.close_on_exec = argument & FD_CLOEXEC != 0;
                Ok(0)
            }
            3 => Ok(O_RDWR | if socket.nonblocking { SOCK_NONBLOCK } else { 0 }),
            4 if argument & !(O_RDWR | SOCK_NONBLOCK) == 0 => {
                socket.nonblocking = argument & SOCK_NONBLOCK != 0;
                Ok(0)
            }
            6 => Ok(u64::from(socket.kind)),
            7 => Ok(0),
            8 => Ok(if u64::from(socket.kind) == SOCK_STREAM {
                TCP_RECEIVE_CAPACITY as u64
            } else {
                UDP_RESPONSE_CAPACITY as u64
            }),
            9 => Ok(u64::from(socket.domain)),
            10 => Ok(u64::from(socket.protocol)),
            11 if u64::from(socket.kind) == SOCK_STREAM && argument == 1 => Ok(0),
            11 => Err(-95),
            13 if argument == 2 && u64::from(socket.kind) == SOCK_STREAM => {
                if let Some(connection) = socket.tcp.as_mut() {
                    crate::aarch64_virtio_net::tcp_close(connection);
                }
                if let Some(connection) = socket.tcp6.as_mut() {
                    crate::aarch64_virtio_net::tcp6_close(connection);
                }
                Ok(0)
            }
            13 if argument == 2 && u64::from(socket.kind) == SOCK_DGRAM => Ok(0),
            13 => Err(-95),
            14 if socket.domain == AF_INET as u8 => {
                let ipv4 = crate::aarch64_virtio_net::config()
                    .map(|config| config.ipv4)
                    .unwrap_or([0; 4]);
                Ok(pack_sockaddr(ipv4, socket.local_port))
            }
            14 => Err(-95),
            15 if socket.domain == AF_INET as u8 && socket.connected => {
                Ok(pack_sockaddr(socket.remote_ip, socket.remote_port))
            }
            15 => Err(-107),
            _ => Err(-22),
        }
    })
}

fn pack_sockaddr(ipv4: [u8; 4], port: u16) -> u64 {
    let port = port.to_be_bytes();
    u64::from_le_bytes([
        AF_INET as u8,
        0,
        port[0],
        port[1],
        ipv4[0],
        ipv4[1],
        ipv4[2],
        ipv4[3],
    ])
}

pub fn is_nonblocking(handle: u64) -> Option<bool> {
    with_state(|state| {
        let index = resolve_index(state, handle)?;
        Some(state.sockets[index].nonblocking)
    })
}

pub fn domain(handle: u64) -> Option<u64> {
    with_state(|state| {
        let index = resolve_index(state, handle)?;
        Some(u64::from(state.sockets[index].domain))
    })
}

pub fn local_endpoint(handle: u64) -> Result<Endpoint, i64> {
    with_state(|state| {
        let index = resolve_index(state, handle).ok_or(-9)?;
        let socket = state.sockets[index];
        let address = if socket.domain == AF_INET6 as u8 {
            IpAddress::V6(
                crate::aarch64_virtio_net::ipv6_config()
                    .map(|config| config.address)
                    .unwrap_or([0; 16]),
            )
        } else {
            IpAddress::V4(
                crate::aarch64_virtio_net::config()
                    .map(|config| config.ipv4)
                    .unwrap_or([0; 4]),
            )
        };
        Ok(Endpoint {
            address,
            port: socket.local_port,
        })
    })
}

pub fn peer_endpoint(handle: u64) -> Result<Endpoint, i64> {
    with_state(|state| {
        let index = resolve_index(state, handle).ok_or(-9)?;
        let socket = state.sockets[index];
        socket
            .connected
            .then_some(socket.remote_endpoint())
            .ok_or(-107)
    })
}

pub fn bind_endpoint(handle: u64, endpoint: Endpoint) -> Result<(), i64> {
    with_state(|state| {
        let index = resolve_index(state, handle).ok_or(-9)?;
        let socket = state.sockets[index];
        let family_matches =
            (socket.domain == AF_INET as u8) == matches!(endpoint.address, IpAddress::V4(_));
        if !family_matches || socket.connected {
            return Err(-22);
        }
        let address_valid = match endpoint.address {
            IpAddress::V4(address) => {
                let configured = crate::aarch64_virtio_net::config()
                    .map(|config| config.ipv4)
                    .unwrap_or([0; 4]);
                address == [0; 4] || address == configured
            }
            IpAddress::V6(address) => {
                let configured = crate::aarch64_virtio_net::ipv6_config()
                    .map(|config| config.address)
                    .unwrap_or([0; 16]);
                address == [0; 16] || address == configured
            }
        };
        if !address_valid {
            return Err(-99);
        }
        if endpoint.port != 0
            && state.sockets.iter().enumerate().any(|(candidate, other)| {
                candidate != index
                    && other.used
                    && other.domain == socket.domain
                    && other.protocol == socket.protocol
                    && other.local_port == endpoint.port
            })
        {
            return Err(-98);
        }
        if endpoint.port != 0 {
            state.sockets[index].local_port = endpoint.port;
        }
        Ok(())
    })
}

pub fn is_owned(handle: u64) -> bool {
    with_state(|state| resolve_index(state, handle).is_some())
}

pub fn poll_events(handle: u64, requested: u32) -> u32 {
    use makos_readiness::{EPOLLERR, EPOLLHUP, EPOLLIN, EPOLLOUT, EPOLLRDHUP};

    let (ready, kind, udp_count) = with_state(|state| {
        let Some(index) = resolve_index(state, handle) else {
            return (EPOLLERR | EPOLLHUP, 0, 0);
        };
        let socket = state.sockets[index];
        if u64::from(socket.kind) == SOCK_STREAM && !socket.connected {
            return (0, socket.kind, socket.udp_count);
        }
        let mut ready = requested & EPOLLOUT;
        if (u64::from(socket.kind) == SOCK_DGRAM && socket.udp_count != 0)
            || (u64::from(socket.kind) == SOCK_STREAM && socket.tcp_offset < socket.tcp_length)
        {
            ready |= requested & EPOLLIN;
        }
        if socket
            .tcp
            .is_some_and(|connection| crate::aarch64_virtio_net::tcp_is_closed(&connection))
            || socket
                .tcp6
                .is_some_and(|connection| crate::aarch64_virtio_net::tcp6_is_closed(&connection))
        {
            ready &= !EPOLLOUT;
            ready |= EPOLLHUP | EPOLLRDHUP;
        }
        (ready, socket.kind, socket.udp_count)
    });
    if crate::aarch64_process::current_app_role() == crate::aarch64_process::ProcessRole::Firefox {
        let trace = FIREFOX_SOCKET_POLLS.fetch_add(1, Ordering::AcqRel);
        if trace < FIREFOX_SOCKET_TRACE_LIMIT {
            crate::serial_println!(
                "MAKOS_FIREFOX_SOCKET_POLL trace={} handle={:#x} kind={} requested={:#x} ready={:#x} udp_queued={}",
                trace,
                handle,
                kind,
                requested,
                ready,
                udp_count,
            );
        }
    }
    ready
}

fn store_snapshot(index: usize, socket: Socket) -> Option<()> {
    with_state(|state| {
        let current = state.sockets.get_mut(index)?;
        if !current.used
            || current.generation != socket.generation
            || current.owner_pid != crate::aarch64_process::current_pid()
        {
            return None;
        }
        *current = socket;
        Some(())
    })
}

fn resolve_index(state: &State, handle: u64) -> Option<usize> {
    state.sockets.iter().position(|socket| {
        socket.used
            && socket.handle == handle
            && socket.owner_pid == crate::aarch64_process::current_pid()
    })
}

fn with_state<R>(function: impl FnOnce(&mut State) -> R) -> R {
    while STATE
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *STATE.value.get() });
    STATE.lock.store(false, Ordering::Release);
    result
}
