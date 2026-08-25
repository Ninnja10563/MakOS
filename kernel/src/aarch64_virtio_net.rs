//! Native virtio-net transport and bounded IPv4/IPv6 client stack for QEMU `virt`.
//!
//! No host proxy or hypercall transport: packets cross virtio RX/TX queues.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering, compiler_fence};

use crate::aarch64_net_wire::{
    self as wire, FRAME_CAPACITY, TCP_PAYLOAD_MAX, TcpSegment, TcpSegmentV6, UdpPacket,
};

const MMIO_BASE: u64 = 0x0a00_0000;
const MMIO_STRIDE: u64 = 0x200;
const MMIO_SLOTS: usize = 32;
const MAGIC: u32 = 0x7472_6976;
const VERSION_MODERN: u32 = 2;
const DEVICE_NETWORK: u32 = 1;

const REG_MAGIC: u64 = 0x000;
const REG_VERSION: u64 = 0x004;
const REG_DEVICE_ID: u64 = 0x008;
const REG_DEVICE_FEATURES: u64 = 0x010;
const REG_DEVICE_FEATURES_SEL: u64 = 0x014;
const REG_DRIVER_FEATURES: u64 = 0x020;
const REG_DRIVER_FEATURES_SEL: u64 = 0x024;
const REG_QUEUE_SEL: u64 = 0x030;
const REG_QUEUE_NUM_MAX: u64 = 0x034;
const REG_QUEUE_NUM: u64 = 0x038;
const REG_QUEUE_READY: u64 = 0x044;
const REG_QUEUE_NOTIFY: u64 = 0x050;
const REG_INTERRUPT_STATUS: u64 = 0x060;
const REG_INTERRUPT_ACK: u64 = 0x064;
const REG_STATUS: u64 = 0x070;
const REG_QUEUE_DESC: u64 = 0x080;
const REG_QUEUE_DRIVER: u64 = 0x090;
const REG_QUEUE_DEVICE: u64 = 0x0a0;
const REG_CONFIG: u64 = 0x100;

const STATUS_ACKNOWLEDGE: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;
const STATUS_FAILED: u32 = 128;
const VIRTIO_F_VERSION_1: u32 = 1;
const VIRTIO_NET_F_MAC: u32 = 1 << 5;

const RECEIVE_QUEUE: u32 = 0;
const TRANSMIT_QUEUE: u32 = 1;
const QUEUE_SIZE: u16 = 16;
const DESC_OFFSET: u64 = 0;
const AVAIL_OFFSET: u64 = 0x200;
const USED_OFFSET: u64 = 0x400;
const VIRTIO_NET_HEADER: u64 = 12;
const BUFFER_BYTES: u32 = 2048;
const DESC_F_WRITE: u16 = 2;
const WAIT_SPINS: usize = 20_000_000;
const IPV6_CONFIG_SPINS: usize = 2_000_000;
const DHCP_TRANSACTION: u32 = 0x4d41_4b42;
const DNS_CACHE_ENTRIES: usize = 4;
const DNS_MESSAGE_CAPACITY: usize = 512;
const TCP_DEFAULT_RECEIVE_WINDOW: u16 = 32_768;
const TX_SERVICE_SLOTS: usize = 8;
const TX_SERVICE_PAYLOAD: usize = 1400;
const TX_SLOT_FREE: u8 = 0;
const TX_SLOT_WRITING: u8 = 1;
const TX_SLOT_READY: u8 = 2;
const TX_SLOT_SERVICING: u8 = 3;
const TX_SLOT_DONE: u8 = 4;
const TX_KIND_UDP4: u8 = 1;
const TX_KIND_UDP6: u8 = 2;

#[derive(Clone, Copy)]
struct DnsCacheEntry {
    query: [u8; DNS_MESSAGE_CAPACITY],
    response: [u8; DNS_MESSAGE_CAPACITY],
    query_length: u16,
    response_length: u16,
}

impl DnsCacheEntry {
    const EMPTY: Self = Self {
        query: [0; DNS_MESSAGE_CAPACITY],
        response: [0; DNS_MESSAGE_CAPACITY],
        query_length: 0,
        response_length: 0,
    };
}

#[derive(Clone, Copy)]
struct Queue {
    ring: u64,
    buffers: [u64; QUEUE_SIZE as usize],
    available: u16,
    used: u16,
}

impl Queue {
    const EMPTY: Self = Self {
        ring: 0,
        buffers: [0; QUEUE_SIZE as usize],
        available: 0,
        used: 0,
    };
}

#[derive(Clone, Copy)]
struct State {
    ready: bool,
    base: u64,
    mac: [u8; 6],
    ipv4: [u8; 4],
    subnet: [u8; 4],
    gateway: [u8; 4],
    dns: [u8; 4],
    gateway_mac: [u8; 6],
    ipv6_ready: bool,
    ipv6_link_local: [u8; 16],
    ipv6: [u8; 16],
    ipv6_prefix_length: u8,
    ipv6_gateway: [u8; 16],
    ipv6_gateway_mac: [u8; 6],
    receive: Queue,
    transmit: Queue,
    identification: u16,
    dns_cache: [DnsCacheEntry; DNS_CACHE_ENTRIES],
    next_dns_cache: usize,
}

impl State {
    const EMPTY: Self = Self {
        ready: false,
        base: 0,
        mac: [0; 6],
        ipv4: [0; 4],
        subnet: [0; 4],
        gateway: [0; 4],
        dns: [0; 4],
        gateway_mac: [0; 6],
        ipv6_ready: false,
        ipv6_link_local: [0; 16],
        ipv6: [0; 16],
        ipv6_prefix_length: 0,
        ipv6_gateway: [0; 16],
        ipv6_gateway_mac: [0; 6],
        receive: Queue::EMPTY,
        transmit: Queue::EMPTY,
        identification: 1,
        dns_cache: [DnsCacheEntry::EMPTY; DNS_CACHE_ENTRIES],
        next_dns_cache: 0,
    };
}

struct LockedState {
    lock: AtomicBool,
    value: UnsafeCell<State>,
}

unsafe impl Sync for LockedState {}

static STATE: LockedState = LockedState {
    lock: AtomicBool::new(false),
    value: UnsafeCell::new(State::EMPTY),
};
static UDP6_SEND_REPORTED: AtomicBool = AtomicBool::new(false);
static TCP6_CONNECT_REPORTED: AtomicBool = AtomicBool::new(false);
static TX_NONOWNER_REQUESTS: AtomicU64 = AtomicU64::new(0);
static TX_OWNER_COMPLETIONS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
struct TxServiceRequest {
    kind: u8,
    remote_ip: [u8; 16],
    remote_port: u16,
    local_port: u16,
    length: u16,
    payload: [u8; TX_SERVICE_PAYLOAD],
}

impl TxServiceRequest {
    const EMPTY: Self = Self {
        kind: 0,
        remote_ip: [0; 16],
        remote_port: 0,
        local_port: 0,
        length: 0,
        payload: [0; TX_SERVICE_PAYLOAD],
    };
}

struct TxServiceSlot {
    state: AtomicU8,
    request: UnsafeCell<TxServiceRequest>,
    result: UnsafeCell<usize>,
}

unsafe impl Sync for TxServiceSlot {}

impl TxServiceSlot {
    const fn new() -> Self {
        Self {
            state: AtomicU8::new(TX_SLOT_FREE),
            request: UnsafeCell::new(TxServiceRequest::EMPTY),
            result: UnsafeCell::new(usize::MAX),
        }
    }
}

static TX_SERVICE: [TxServiceSlot; TX_SERVICE_SLOTS] =
    [const { TxServiceSlot::new() }; TX_SERVICE_SLOTS];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetConfig {
    pub ipv4: [u8; 4],
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ipv6Config {
    pub address: [u8; 16],
    pub prefix_length: u8,
    pub gateway: [u8; 16],
}

#[derive(Clone, Copy)]
pub struct TcpConnection {
    remote_ip: [u8; 4],
    remote_mac: [u8; 6],
    local_port: u16,
    remote_port: u16,
    transmit_sequence: u32,
    receive_sequence: u32,
    receive_window: u16,
    closed: bool,
}

#[derive(Clone, Copy)]
pub struct Tcp6Connection {
    remote_ip: [u8; 16],
    remote_mac: [u8; 6],
    local_port: u16,
    remote_port: u16,
    transmit_sequence: u32,
    receive_sequence: u32,
    receive_window: u16,
    closed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TcpIngress {
    pub length: usize,
    pub closed: bool,
}

pub fn init() {
    let mut found = None;
    for slot in 0..MMIO_SLOTS {
        let base = MMIO_BASE + slot as u64 * MMIO_STRIDE;
        if read32(base + REG_MAGIC) == MAGIC
            && read32(base + REG_VERSION) == VERSION_MODERN
            && read32(base + REG_DEVICE_ID) == DEVICE_NETWORK
        {
            if found.is_some() {
                crate::fatal("multiple AArch64 virtio-net devices");
            }
            found = Some((base, slot));
        }
    }
    let Some((base, slot)) = found else {
        crate::fatal("AArch64 virtio-net device absent");
    };
    crate::serial_println!(
        "virtio-net trace slot={} stage=probe base={:#x}",
        slot,
        base
    );
    let mut state = configure(base);
    crate::serial_println!("virtio-net trace slot={} stage=device-ready", slot);
    configure_ipv4(&mut state);
    configure_ipv6(&mut state);
    state.ready = true;
    let config = NetConfig {
        ipv4: state.ipv4,
        gateway: state.gateway,
        dns: state.dns,
    };
    let mac = state.mac;
    let ipv6 = state.ipv6;
    let ipv6_gateway = state.ipv6_gateway;
    let ipv6_ready = state.ipv6_ready;
    with_state(|current| *current = state);
    crate::serial_println!(
        "MAKOS_AARCH64_NET_OK transport=virtio-net-mmio slot={} mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} ipv4={}.{}.{}.{} gateway={}.{}.{}.{} dns={}.{}.{}.{} ethernet=1 dhcp=1 arp=1 ipv4_stack=1 udp=1 tcp=1 host_proxy=0",
        slot,
        mac[0],
        mac[1],
        mac[2],
        mac[3],
        mac[4],
        mac[5],
        config.ipv4[0],
        config.ipv4[1],
        config.ipv4[2],
        config.ipv4[3],
        config.gateway[0],
        config.gateway[1],
        config.gateway[2],
        config.gateway[3],
        config.dns[0],
        config.dns[1],
        config.dns[2],
        config.dns[3],
    );
    if ipv6_ready {
        crate::serial_println!(
            "MAKOS_AARCH64_IPV6_READY address={:02x?} prefix=64 gateway={:02x?} slaac=ra,eui64 ndp=ns,na udp6=1 tcp6=1 fake_mapping=0",
            ipv6,
            ipv6_gateway,
        );
    } else {
        crate::serial_println!(
            "MAKOS_AARCH64_IPV6_UNAVAILABLE reason=no-valid-router-advertisement af_inet6=socket-only fake_mapping=0"
        );
    }
}

pub fn config() -> Option<NetConfig> {
    with_state(|state| {
        state.ready.then_some(NetConfig {
            ipv4: state.ipv4,
            gateway: state.gateway,
            dns: state.dns,
        })
    })
}

pub fn ipv6_config() -> Option<Ipv6Config> {
    with_state(|state| {
        (state.ready && state.ipv6_ready).then_some(Ipv6Config {
            address: state.ipv6,
            prefix_length: state.ipv6_prefix_length,
            gateway: state.ipv6_gateway,
        })
    })
}

pub fn udp_exchange(
    remote_ip: [u8; 4],
    remote_port: u16,
    local_port: u16,
    payload: &[u8],
    output: &mut [u8],
) -> Option<usize> {
    if remote_ip == [0; 4]
        || remote_port == 0
        || local_port == 0
        || payload.is_empty()
        || payload.len() > 1400
        || output.is_empty()
    {
        return None;
    }
    with_state(|state| {
        if !state.ready {
            return None;
        }
        let dns_request = remote_ip == state.dns && remote_port == 53;
        if dns_request && let Some(length) = dns_cache_lookup(state, payload, output) {
            return Some(length);
        }
        let destination_mac = resolve_route(state, remote_ip)?;
        let mut frame = [0; FRAME_CAPACITY];
        let id = next_identification(state);
        let length = wire::build_udp(
            &mut frame,
            state.mac,
            destination_mac,
            state.ipv4,
            remote_ip,
            local_port,
            remote_port,
            payload,
            id,
        )?;
        transmit_frame(state, &frame[..length])?;
        let length = wait_udp(state, remote_ip, remote_port, local_port, output)?;
        if dns_request {
            dns_cache_store(state, payload, &output[..length]);
        }
        Some(length)
    })
}

/// Transmit one UDP datagram without waiting for its response. Receive frames
/// are centrally consumed by `aarch64_socket::pump`, so a timer tick cannot
/// steal DNS replies from a syscall spinning in `wait_udp`.
pub fn udp_send(
    remote_ip: [u8; 4],
    remote_port: u16,
    local_port: u16,
    payload: &[u8],
) -> Option<usize> {
    if remote_ip == [0; 4]
        || remote_port == 0
        || local_port == 0
        || payload.is_empty()
        || payload.len() > 1400
    {
        return None;
    }
    if crate::arch::cpu_index() != 0 {
        let mut address = [0u8; 16];
        address[..4].copy_from_slice(&remote_ip);
        return queue_udp_send(TX_KIND_UDP4, address, remote_port, local_port, payload);
    }
    udp_send_on_owner(remote_ip, remote_port, local_port, payload)
}

fn udp_send_on_owner(
    remote_ip: [u8; 4],
    remote_port: u16,
    local_port: u16,
    payload: &[u8],
) -> Option<usize> {
    with_state(|state| {
        if !state.ready {
            return None;
        }
        let destination_mac = resolve_route(state, remote_ip)?;
        let mut frame = [0; FRAME_CAPACITY];
        let id = next_identification(state);
        let length = wire::build_udp(
            &mut frame,
            state.mac,
            destination_mac,
            state.ipv4,
            remote_ip,
            local_port,
            remote_port,
            payload,
            id,
        )?;
        transmit_frame(state, &frame[..length])?;
        Some(payload.len())
    })
}

pub fn udp6_send(
    remote_ip: [u8; 16],
    remote_port: u16,
    local_port: u16,
    payload: &[u8],
) -> Option<usize> {
    if remote_ip == [0; 16]
        || remote_ip[0] == 0xff
        || remote_port == 0
        || local_port == 0
        || payload.is_empty()
        || payload.len() > 1400
    {
        return None;
    }
    if crate::arch::cpu_index() != 0 {
        return queue_udp_send(TX_KIND_UDP6, remote_ip, remote_port, local_port, payload);
    }
    udp6_send_on_owner(remote_ip, remote_port, local_port, payload)
}

fn udp6_send_on_owner(
    remote_ip: [u8; 16],
    remote_port: u16,
    local_port: u16,
    payload: &[u8],
) -> Option<usize> {
    with_state(|state| {
        if !state.ready || !state.ipv6_ready {
            return None;
        }
        let destination_mac = resolve_route_v6(state, remote_ip)?;
        let mut frame = [0; FRAME_CAPACITY];
        let length = wire::build_udp_v6(
            &mut frame,
            state.mac,
            destination_mac,
            state.ipv6,
            remote_ip,
            local_port,
            remote_port,
            payload,
        )?;
        transmit_frame(state, &frame[..length])?;
        if !UDP6_SEND_REPORTED.swap(true, Ordering::AcqRel) {
            crate::serial_println!(
                "MAKOS_AARCH64_UDP6_SEND_OK checksum=pseudoheader ndp=resolved fake_mapping=0"
            );
        }
        Some(payload.len())
    })
}

fn queue_udp_send(
    kind: u8,
    remote_ip: [u8; 16],
    remote_port: u16,
    local_port: u16,
    payload: &[u8],
) -> Option<usize> {
    let slot = TX_SERVICE.iter().find(|slot| {
        slot.state
            .compare_exchange(
                TX_SLOT_FREE,
                TX_SLOT_WRITING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    })?;
    let mut request = TxServiceRequest::EMPTY;
    request.kind = kind;
    request.remote_ip = remote_ip;
    request.remote_port = remote_port;
    request.local_port = local_port;
    request.length = payload.len() as u16;
    request.payload[..payload.len()].copy_from_slice(payload);
    unsafe {
        slot.request.get().write(request);
        slot.result.get().write(usize::MAX);
    }
    TX_NONOWNER_REQUESTS.fetch_add(1, Ordering::AcqRel);
    slot.state.store(TX_SLOT_READY, Ordering::Release);
    unsafe { core::arch::asm!("dsb ish", "sev", options(nostack)) };

    let deadline = crate::arch::counter_deadline_millis(5_000);
    while slot.state.load(Ordering::Acquire) != TX_SLOT_DONE {
        if crate::arch::counter_deadline_expired(deadline) {
            crate::fatal("AArch64 network TX owner request timeout");
        }
        unsafe { core::arch::asm!("wfe", options(nomem, nostack)) };
    }
    let result = unsafe { slot.result.get().read() };
    slot.state.store(TX_SLOT_FREE, Ordering::Release);
    (result != usize::MAX).then_some(result)
}

pub fn service_tx_requests() -> usize {
    if crate::arch::cpu_index() != 0 {
        crate::fatal("AArch64 network TX service attempted from non-owner CPU");
    }
    let mut completed = 0usize;
    for slot in &TX_SERVICE {
        if slot
            .state
            .compare_exchange(
                TX_SLOT_READY,
                TX_SLOT_SERVICING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            continue;
        }
        let request = unsafe { slot.request.get().read() };
        let length = usize::from(request.length);
        let result = if length > TX_SERVICE_PAYLOAD {
            None
        } else {
            match request.kind {
                TX_KIND_UDP4 => udp_send_on_owner(
                    [
                        request.remote_ip[0],
                        request.remote_ip[1],
                        request.remote_ip[2],
                        request.remote_ip[3],
                    ],
                    request.remote_port,
                    request.local_port,
                    &request.payload[..length],
                ),
                TX_KIND_UDP6 => udp6_send_on_owner(
                    request.remote_ip,
                    request.remote_port,
                    request.local_port,
                    &request.payload[..length],
                ),
                _ => None,
            }
        };
        unsafe { slot.result.get().write(result.unwrap_or(usize::MAX)) };
        TX_OWNER_COMPLETIONS.fetch_add(1, Ordering::AcqRel);
        slot.state.store(TX_SLOT_DONE, Ordering::Release);
        completed += 1;
    }
    if completed != 0 {
        unsafe { core::arch::asm!("dsb ish", "sev", options(nostack)) };
    }
    completed
}

pub fn reset_tx_affinity_evidence() {
    TX_NONOWNER_REQUESTS.store(0, Ordering::Release);
    TX_OWNER_COMPLETIONS.store(0, Ordering::Release);
}

pub fn tx_affinity_evidence() -> (u64, u64) {
    (
        TX_OWNER_COMPLETIONS.load(Ordering::Acquire),
        TX_NONOWNER_REQUESTS.load(Ordering::Acquire),
    )
}

fn dns_cache_lookup(state: &State, query: &[u8], output: &mut [u8]) -> Option<usize> {
    if query.len() < 12 || query.len() > DNS_MESSAGE_CAPACITY {
        return None;
    }
    for entry in &state.dns_cache {
        let query_length = usize::from(entry.query_length);
        let response_length = usize::from(entry.response_length);
        if query_length == query.len()
            && response_length >= 2
            && response_length <= output.len()
            && entry.query[12..query_length] == query[12..]
        {
            output[..response_length].copy_from_slice(&entry.response[..response_length]);
            output[..2].copy_from_slice(&query[..2]);
            return Some(response_length);
        }
    }
    None
}

fn dns_cache_store(state: &mut State, query: &[u8], response: &[u8]) {
    if query.len() < 2
        || query.len() > DNS_MESSAGE_CAPACITY
        || response.len() < 2
        || response.len() > DNS_MESSAGE_CAPACITY
    {
        return;
    }
    let slot = state.next_dns_cache % DNS_CACHE_ENTRIES;
    state.next_dns_cache = (slot + 1) % DNS_CACHE_ENTRIES;
    let entry = &mut state.dns_cache[slot];
    *entry = DnsCacheEntry::EMPTY;
    entry.query[..query.len()].copy_from_slice(query);
    entry.response[..response.len()].copy_from_slice(response);
    entry.query_length = query.len() as u16;
    entry.response_length = response.len() as u16;
}

pub fn tcp_connect(remote_ip: [u8; 4], remote_port: u16, local_port: u16) -> Option<TcpConnection> {
    if remote_ip == [0; 4] || remote_port == 0 || local_port == 0 {
        return None;
    }
    with_state(|state| {
        if !state.ready {
            return None;
        }
        let remote_mac = resolve_route(state, remote_ip)?;
        let initial = 0x4d41_0000u32
            .wrapping_add(u32::from(local_port))
            .wrapping_add(u32::from(state.identification) << 16);
        send_tcp(
            state,
            remote_mac,
            remote_ip,
            local_port,
            remote_port,
            initial,
            0,
            0x02,
            TCP_DEFAULT_RECEIVE_WINDOW,
            &[],
        )?;
        let segment = wait_tcp(state, remote_ip, remote_port, local_port)?;
        if segment.flags & 0x12 != 0x12 || segment.acknowledgment != initial.wrapping_add(1) {
            return None;
        }
        let receive_sequence = segment.sequence.wrapping_add(1);
        send_tcp(
            state,
            remote_mac,
            remote_ip,
            local_port,
            remote_port,
            initial.wrapping_add(1),
            receive_sequence,
            0x10,
            TCP_DEFAULT_RECEIVE_WINDOW,
            &[],
        )?;
        Some(TcpConnection {
            remote_ip,
            remote_mac,
            local_port,
            remote_port,
            transmit_sequence: initial.wrapping_add(1),
            receive_sequence,
            receive_window: TCP_DEFAULT_RECEIVE_WINDOW,
            closed: false,
        })
    })
}

pub fn tcp_send(connection: &mut TcpConnection, payload: &[u8]) -> Option<usize> {
    if connection.closed || payload.is_empty() {
        return None;
    }
    with_state(|state| {
        if !state.ready {
            return None;
        }
        let mut sent = 0usize;
        while sent < payload.len() {
            let count = (payload.len() - sent).min(TCP_PAYLOAD_MAX);
            let flags = if sent + count == payload.len() {
                0x18
            } else {
                0x10
            };
            send_tcp(
                state,
                connection.remote_mac,
                connection.remote_ip,
                connection.local_port,
                connection.remote_port,
                connection.transmit_sequence,
                connection.receive_sequence,
                flags,
                connection.receive_window,
                &payload[sent..sent + count],
            )?;
            connection.transmit_sequence = connection.transmit_sequence.wrapping_add(count as u32);
            sent += count;
        }
        Some(sent)
    })
}

pub fn tcp_receive(connection: &mut TcpConnection, output: &mut [u8]) -> Option<usize> {
    if output.is_empty() {
        return None;
    }
    if connection.closed {
        return Some(0);
    }
    with_state(|state| {
        loop {
            let segment = wait_tcp(
                state,
                connection.remote_ip,
                connection.remote_port,
                connection.local_port,
            )?;
            if segment.flags & 0x04 != 0 {
                connection.closed = true;
                return None;
            }
            if segment.sequence != connection.receive_sequence {
                send_tcp(
                    state,
                    connection.remote_mac,
                    connection.remote_ip,
                    connection.local_port,
                    connection.remote_port,
                    connection.transmit_sequence,
                    connection.receive_sequence,
                    0x10,
                    connection.receive_window,
                    &[],
                )?;
                continue;
            }
            if segment.payload().len() > output.len() {
                return None;
            }
            let count = segment.payload().len();
            output[..count].copy_from_slice(segment.payload());
            connection.receive_sequence = connection.receive_sequence.wrapping_add(count as u32);
            if segment.flags & 0x01 != 0 {
                connection.receive_sequence = connection.receive_sequence.wrapping_add(1);
                connection.closed = true;
            }
            if count != 0 || connection.closed {
                connection.receive_window = TCP_DEFAULT_RECEIVE_WINDOW;
                send_tcp(
                    state,
                    connection.remote_mac,
                    connection.remote_ip,
                    connection.local_port,
                    connection.remote_port,
                    connection.transmit_sequence,
                    connection.receive_sequence,
                    0x10,
                    TCP_DEFAULT_RECEIVE_WINDOW,
                    &[],
                )?;
                return Some(count);
            }
        }
    })
}

pub fn tcp_close(connection: &mut TcpConnection) {
    if connection.closed {
        return;
    }
    let _ = with_state(|state| {
        send_tcp(
            state,
            connection.remote_mac,
            connection.remote_ip,
            connection.local_port,
            connection.remote_port,
            connection.transmit_sequence,
            connection.receive_sequence,
            0x11,
            connection.receive_window,
            &[],
        )
    });
    connection.transmit_sequence = connection.transmit_sequence.wrapping_add(1);
    connection.closed = true;
}

pub fn tcp_is_closed(connection: &TcpConnection) -> bool {
    connection.closed
}

pub fn tcp6_connect(
    remote_ip: [u8; 16],
    remote_port: u16,
    local_port: u16,
) -> Option<Tcp6Connection> {
    if remote_ip == [0; 16] || remote_ip[0] == 0xff || remote_port == 0 || local_port == 0 {
        return None;
    }
    with_state(|state| {
        if !state.ready || !state.ipv6_ready {
            return None;
        }
        let remote_mac = resolve_route_v6(state, remote_ip)?;
        let initial = 0x6d41_0000u32
            .wrapping_add(u32::from(local_port))
            .wrapping_add(u32::from(state.identification) << 16);
        send_tcp_v6(
            state,
            remote_mac,
            remote_ip,
            local_port,
            remote_port,
            initial,
            0,
            0x02,
            TCP_DEFAULT_RECEIVE_WINDOW,
            &[],
        )?;
        let segment = wait_tcp_v6(state, remote_ip, remote_port, local_port)?;
        if segment.flags & 0x12 != 0x12 || segment.acknowledgment != initial.wrapping_add(1) {
            return None;
        }
        let receive_sequence = segment.sequence.wrapping_add(1);
        send_tcp_v6(
            state,
            remote_mac,
            remote_ip,
            local_port,
            remote_port,
            initial.wrapping_add(1),
            receive_sequence,
            0x10,
            TCP_DEFAULT_RECEIVE_WINDOW,
            &[],
        )?;
        let connection = Tcp6Connection {
            remote_ip,
            remote_mac,
            local_port,
            remote_port,
            transmit_sequence: initial.wrapping_add(1),
            receive_sequence,
            receive_window: TCP_DEFAULT_RECEIVE_WINDOW,
            closed: false,
        };
        if !TCP6_CONNECT_REPORTED.swap(true, Ordering::AcqRel) {
            crate::serial_println!(
                "MAKOS_AARCH64_TCP6_CONNECT_OK handshake=syn,synack,ack checksum=pseudoheader fake_mapping=0"
            );
        }
        Some(connection)
    })
}

pub fn tcp6_send(connection: &mut Tcp6Connection, payload: &[u8]) -> Option<usize> {
    if connection.closed || payload.is_empty() {
        return None;
    }
    with_state(|state| {
        if !state.ready || !state.ipv6_ready {
            return None;
        }
        let mut sent = 0usize;
        while sent < payload.len() {
            let count = (payload.len() - sent).min(TCP_PAYLOAD_MAX);
            let flags = if sent + count == payload.len() {
                0x18
            } else {
                0x10
            };
            send_tcp_v6(
                state,
                connection.remote_mac,
                connection.remote_ip,
                connection.local_port,
                connection.remote_port,
                connection.transmit_sequence,
                connection.receive_sequence,
                flags,
                connection.receive_window,
                &payload[sent..sent + count],
            )?;
            connection.transmit_sequence = connection.transmit_sequence.wrapping_add(count as u32);
            sent += count;
        }
        Some(sent)
    })
}

pub fn tcp6_receive(connection: &mut Tcp6Connection, output: &mut [u8]) -> Option<usize> {
    if output.is_empty() {
        return None;
    }
    if connection.closed {
        return Some(0);
    }
    with_state(|state| {
        loop {
            let segment = wait_tcp_v6(
                state,
                connection.remote_ip,
                connection.remote_port,
                connection.local_port,
            )?;
            if segment.flags & 0x04 != 0 {
                connection.closed = true;
                return None;
            }
            if segment.sequence != connection.receive_sequence {
                send_tcp_v6(
                    state,
                    connection.remote_mac,
                    connection.remote_ip,
                    connection.local_port,
                    connection.remote_port,
                    connection.transmit_sequence,
                    connection.receive_sequence,
                    0x10,
                    connection.receive_window,
                    &[],
                )?;
                continue;
            }
            if segment.payload().len() > output.len() {
                return None;
            }
            let count = segment.payload().len();
            output[..count].copy_from_slice(segment.payload());
            connection.receive_sequence = connection.receive_sequence.wrapping_add(count as u32);
            if segment.flags & 0x01 != 0 {
                connection.receive_sequence = connection.receive_sequence.wrapping_add(1);
                connection.closed = true;
            }
            if count != 0 || connection.closed {
                connection.receive_window = TCP_DEFAULT_RECEIVE_WINDOW;
                send_tcp_v6(
                    state,
                    connection.remote_mac,
                    connection.remote_ip,
                    connection.local_port,
                    connection.remote_port,
                    connection.transmit_sequence,
                    connection.receive_sequence,
                    0x10,
                    TCP_DEFAULT_RECEIVE_WINDOW,
                    &[],
                )?;
                return Some(count);
            }
        }
    })
}

pub fn tcp6_close(connection: &mut Tcp6Connection) {
    if connection.closed {
        return;
    }
    let _ = with_state(|state| {
        send_tcp_v6(
            state,
            connection.remote_mac,
            connection.remote_ip,
            connection.local_port,
            connection.remote_port,
            connection.transmit_sequence,
            connection.receive_sequence,
            0x11,
            connection.receive_window,
            &[],
        )
    });
    connection.transmit_sequence = connection.transmit_sequence.wrapping_add(1);
    connection.closed = true;
}

pub fn tcp6_is_closed(connection: &Tcp6Connection) -> bool {
    connection.closed
}

pub fn tcp6_update_receive_window(connection: &mut Tcp6Connection, available: usize) -> bool {
    let receive_window = available.min(u16::MAX as usize) as u16;
    if connection.closed || receive_window <= connection.receive_window {
        connection.receive_window = receive_window;
        return true;
    }
    connection.receive_window = receive_window;
    with_state(|state| {
        send_tcp_v6(
            state,
            connection.remote_mac,
            connection.remote_ip,
            connection.local_port,
            connection.remote_port,
            connection.transmit_sequence,
            connection.receive_sequence,
            0x10,
            receive_window,
            &[],
        )
        .is_some()
    })
}

pub fn tcp6_ingest(
    connection: &mut Tcp6Connection,
    segment: TcpSegmentV6<'_>,
    output: &mut [u8],
) -> Option<TcpIngress> {
    if segment.source_ip != connection.remote_ip
        || segment.source_port != connection.remote_port
        || segment.destination_port != connection.local_port
    {
        return None;
    }
    if segment.flags & 0x04 != 0 {
        connection.closed = true;
        return Some(TcpIngress {
            length: 0,
            closed: true,
        });
    }
    if segment.sequence != connection.receive_sequence {
        connection.receive_window = output.len().min(u16::MAX as usize) as u16;
        let _ = with_state(|state| {
            send_tcp_v6(
                state,
                connection.remote_mac,
                connection.remote_ip,
                connection.local_port,
                connection.remote_port,
                connection.transmit_sequence,
                connection.receive_sequence,
                0x10,
                connection.receive_window,
                &[],
            )
        });
        return Some(TcpIngress {
            length: 0,
            closed: connection.closed,
        });
    }
    if segment.payload.len() > output.len() {
        connection.receive_window = 0;
        let _ = with_state(|state| {
            send_tcp_v6(
                state,
                connection.remote_mac,
                connection.remote_ip,
                connection.local_port,
                connection.remote_port,
                connection.transmit_sequence,
                connection.receive_sequence,
                0x10,
                0,
                &[],
            )
        });
        return Some(TcpIngress {
            length: 0,
            closed: false,
        });
    }
    output[..segment.payload.len()].copy_from_slice(segment.payload);
    connection.receive_sequence = connection
        .receive_sequence
        .wrapping_add(segment.payload.len() as u32);
    if segment.flags & 0x01 != 0 {
        connection.receive_sequence = connection.receive_sequence.wrapping_add(1);
        connection.closed = true;
    }
    connection.receive_window = output
        .len()
        .saturating_sub(segment.payload.len())
        .min(u16::MAX as usize) as u16;
    if !segment.payload.is_empty() || connection.closed {
        let _ = with_state(|state| {
            send_tcp_v6(
                state,
                connection.remote_mac,
                connection.remote_ip,
                connection.local_port,
                connection.remote_port,
                connection.transmit_sequence,
                connection.receive_sequence,
                0x10,
                connection.receive_window,
                &[],
            )
        });
    }
    Some(TcpIngress {
        length: segment.payload.len(),
        closed: connection.closed,
    })
}

/// Consume at most one completed virtio RX descriptor without waiting.
pub fn poll_frame(output: &mut [u8]) -> Option<usize> {
    if crate::arch::cpu_index() != 0 {
        crate::fatal("AArch64 virtio-net RX poll attempted from non-owner CPU");
    }
    with_state(|state| {
        if state.ready {
            poll_receive(state, output)
        } else {
            None
        }
    })
}

/// Route one already-received TCP segment into a connection without waiting.
pub fn tcp_ingest(
    connection: &mut TcpConnection,
    segment: TcpSegment<'_>,
    output: &mut [u8],
) -> Option<TcpIngress> {
    if segment.source_ip != connection.remote_ip
        || segment.source_port != connection.remote_port
        || segment.destination_port != connection.local_port
    {
        return None;
    }
    if segment.flags & 0x04 != 0 {
        connection.closed = true;
        return Some(TcpIngress {
            length: 0,
            closed: true,
        });
    }
    if segment.sequence != connection.receive_sequence {
        connection.receive_window = output.len().min(u16::MAX as usize) as u16;
        let _ = with_state(|state| {
            send_tcp(
                state,
                connection.remote_mac,
                connection.remote_ip,
                connection.local_port,
                connection.remote_port,
                connection.transmit_sequence,
                connection.receive_sequence,
                0x10,
                connection.receive_window,
                &[],
            )
        });
        return Some(TcpIngress {
            length: 0,
            closed: connection.closed,
        });
    }
    if segment.payload.len() > output.len() {
        connection.receive_window = 0;
        let _ = with_state(|state| {
            send_tcp(
                state,
                connection.remote_mac,
                connection.remote_ip,
                connection.local_port,
                connection.remote_port,
                connection.transmit_sequence,
                connection.receive_sequence,
                0x10,
                0,
                &[],
            )
        });
        return Some(TcpIngress {
            length: 0,
            closed: false,
        });
    }
    output[..segment.payload.len()].copy_from_slice(segment.payload);
    connection.receive_sequence = connection
        .receive_sequence
        .wrapping_add(segment.payload.len() as u32);
    if segment.flags & 0x01 != 0 {
        connection.receive_sequence = connection.receive_sequence.wrapping_add(1);
        connection.closed = true;
    }
    connection.receive_window = output
        .len()
        .saturating_sub(segment.payload.len())
        .min(u16::MAX as usize) as u16;
    if !segment.payload.is_empty() || connection.closed {
        let _ = with_state(|state| {
            send_tcp(
                state,
                connection.remote_mac,
                connection.remote_ip,
                connection.local_port,
                connection.remote_port,
                connection.transmit_sequence,
                connection.receive_sequence,
                0x10,
                connection.receive_window,
                &[],
            )
        });
    }
    Some(TcpIngress {
        length: segment.payload.len(),
        closed: connection.closed,
    })
}

pub fn tcp_update_receive_window(connection: &mut TcpConnection, available: usize) -> bool {
    let receive_window = available.min(u16::MAX as usize) as u16;
    if connection.closed || receive_window <= connection.receive_window {
        connection.receive_window = receive_window;
        return true;
    }
    connection.receive_window = receive_window;
    with_state(|state| {
        send_tcp(
            state,
            connection.remote_mac,
            connection.remote_ip,
            connection.local_port,
            connection.remote_port,
            connection.transmit_sequence,
            connection.receive_sequence,
            0x10,
            receive_window,
            &[],
        )
        .is_some()
    })
}

fn configure(base: u64) -> State {
    write32(base + REG_STATUS, 0);
    for _ in 0..100_000 {
        if read32(base + REG_STATUS) == 0 {
            break;
        }
        core::hint::spin_loop();
    }
    write32(base + REG_STATUS, STATUS_ACKNOWLEDGE);
    write32(base + REG_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);

    write32(base + REG_DEVICE_FEATURES_SEL, 1);
    if read32(base + REG_DEVICE_FEATURES) & VIRTIO_F_VERSION_1 == 0 {
        fail(base, "virtio-net lacks VERSION_1");
    }
    write32(base + REG_DEVICE_FEATURES_SEL, 0);
    if read32(base + REG_DEVICE_FEATURES) & VIRTIO_NET_F_MAC == 0 {
        fail(base, "virtio-net lacks MAC config");
    }
    write32(base + REG_DRIVER_FEATURES_SEL, 0);
    write32(base + REG_DRIVER_FEATURES, VIRTIO_NET_F_MAC);
    write32(base + REG_DRIVER_FEATURES_SEL, 1);
    write32(base + REG_DRIVER_FEATURES, VIRTIO_F_VERSION_1);
    let feature_status = STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK;
    write32(base + REG_STATUS, feature_status);
    if read32(base + REG_STATUS) & STATUS_FEATURES_OK == 0 {
        fail(base, "virtio-net feature negotiation failed");
    }
    crate::serial_println!("virtio-net trace stage=features-ok");

    let receive = configure_queue(base, RECEIVE_QUEUE, true);
    crate::serial_println!("virtio-net trace stage=rxq-ready");
    let transmit = configure_queue(base, TRANSMIT_QUEUE, false);
    crate::serial_println!("virtio-net trace stage=txq-ready");
    memory_barrier();
    write32(base + REG_STATUS, feature_status | STATUS_DRIVER_OK);
    if read32(base + REG_STATUS) & STATUS_FAILED != 0 {
        fail(base, "virtio-net entered FAILED state");
    }
    write32(base + REG_QUEUE_NOTIFY, RECEIVE_QUEUE);
    crate::serial_println!("virtio-net trace stage=driver-ok");
    // virtio-mmio registers use 32-bit accesses. Reading config bytewise trips
    // Apple's HVF MMIO path even though TCG accepts it.
    let mac_low = read32(base + REG_CONFIG);
    let mac_high = read32(base + REG_CONFIG + 4);
    crate::serial_println!("virtio-net trace stage=config-read");
    let mac = [
        mac_low as u8,
        (mac_low >> 8) as u8,
        (mac_low >> 16) as u8,
        (mac_low >> 24) as u8,
        mac_high as u8,
        (mac_high >> 8) as u8,
    ];
    if mac == [0; 6] || mac == [0xff; 6] {
        fail(base, "virtio-net invalid MAC");
    }
    State {
        base,
        mac,
        receive,
        transmit,
        ..State::EMPTY
    }
}

fn configure_queue(base: u64, index: u32, receive: bool) -> Queue {
    write32(base + REG_QUEUE_SEL, index);
    if read32(base + REG_QUEUE_NUM_MAX) < u32::from(QUEUE_SIZE)
        || read32(base + REG_QUEUE_READY) != 0
    {
        fail(base, "virtio-net queue unsupported");
    }
    let ring = allocate_zeroed_frame("virtio-net ring OOM");
    let mut queue = Queue {
        ring,
        ..Queue::EMPTY
    };
    let buffer_count = if receive { QUEUE_SIZE as usize } else { 1 };
    for index in 0..buffer_count {
        let buffer = allocate_zeroed_frame("virtio-net buffer OOM");
        queue.buffers[index] = buffer;
        write_descriptor(
            ring + DESC_OFFSET + index as u64 * 16,
            buffer,
            BUFFER_BYTES,
            if receive { DESC_F_WRITE } else { 0 },
        );
        if receive {
            write16_memory(ring + AVAIL_OFFSET + 4 + index as u64 * 2, index as u16);
        }
    }
    if receive {
        queue.available = QUEUE_SIZE;
        write16_memory(ring + AVAIL_OFFSET + 2, queue.available);
    }
    write32(base + REG_QUEUE_NUM, u32::from(QUEUE_SIZE));
    write_address(base + REG_QUEUE_DESC, ring + DESC_OFFSET);
    write_address(base + REG_QUEUE_DRIVER, ring + AVAIL_OFFSET);
    write_address(base + REG_QUEUE_DEVICE, ring + USED_OFFSET);
    write32(base + REG_QUEUE_READY, 1);
    queue
}

fn configure_ipv4(state: &mut State) {
    let mut frame = [0; FRAME_CAPACITY];
    let discover = wire::build_dhcp(&mut frame, state.mac, DHCP_TRANSACTION, 1, None, None)
        .unwrap_or_else(|| crate::fatal("DHCP discover construction failed"));
    transmit_frame(state, &frame[..discover])
        .unwrap_or_else(|| crate::fatal("DHCP discover transmit failed"));
    let offer = wait_dhcp(state, 2).unwrap_or_else(|| crate::fatal("DHCP offer timeout"));
    let server = if offer.server == [0; 4] {
        offer.gateway
    } else {
        offer.server
    };
    let request = wire::build_dhcp(
        &mut frame,
        state.mac,
        DHCP_TRANSACTION,
        3,
        Some(offer.address),
        Some(server),
    )
    .unwrap_or_else(|| crate::fatal("DHCP request construction failed"));
    transmit_frame(state, &frame[..request])
        .unwrap_or_else(|| crate::fatal("DHCP request transmit failed"));
    let acknowledgment = wait_dhcp(state, 5).unwrap_or_else(|| crate::fatal("DHCP ACK timeout"));
    state.ipv4 = acknowledgment.address;
    state.subnet = acknowledgment.subnet;
    state.gateway = if acknowledgment.gateway == [0; 4] {
        offer.gateway
    } else {
        acknowledgment.gateway
    };
    state.dns = if acknowledgment.dns == [0; 4] {
        offer.dns
    } else {
        acknowledgment.dns
    };
    if state.gateway == [0; 4] || state.dns == [0; 4] {
        crate::fatal("DHCP omitted gateway or DNS");
    }
    state.gateway_mac = resolve_arp(state, state.gateway)
        .unwrap_or_else(|| crate::fatal("virtio-net gateway ARP timeout"));
}

fn configure_ipv6(state: &mut State) {
    state.ipv6_link_local = wire::link_local_from_mac(state.mac);
    let mut frame = [0u8; FRAME_CAPACITY];
    let Some(length) =
        wire::build_router_solicitation(&mut frame, state.mac, state.ipv6_link_local)
    else {
        return;
    };
    if transmit_frame(state, &frame[..length]).is_none() {
        return;
    }
    for _ in 0..IPV6_CONFIG_SPINS {
        let Some(length) = poll_receive(state, &mut frame) else {
            core::hint::spin_loop();
            continue;
        };
        let Some(advertisement) = wire::parse_router_advertisement(&frame[..length]) else {
            continue;
        };
        let mut address = advertisement.prefix;
        address[8..].copy_from_slice(&state.ipv6_link_local[8..]);
        if address == [0; 16]
            || address[0] == 0xff
            || advertisement.router_mac == [0; 6]
            || advertisement.prefix_length != 64
        {
            continue;
        }
        state.ipv6 = address;
        state.ipv6_prefix_length = advertisement.prefix_length;
        state.ipv6_gateway = advertisement.router_ip;
        state.ipv6_gateway_mac = advertisement.router_mac;
        state.ipv6_ready = true;
        return;
    }
}

fn wait_dhcp(state: &mut State, message_type: u8) -> Option<wire::DhcpReply> {
    let mut frame = [0; FRAME_CAPACITY];
    for _ in 0..WAIT_SPINS {
        let Some(length) = poll_receive(state, &mut frame) else {
            core::hint::spin_loop();
            continue;
        };
        if let Some(reply) = wire::parse_dhcp(&frame[..length], DHCP_TRANSACTION, state.mac)
            && reply.message_type == message_type
        {
            return Some(reply);
        }
    }
    None
}

fn resolve_route(state: &mut State, remote_ip: [u8; 4]) -> Option<[u8; 6]> {
    if remote_ip == [255; 4] {
        return Some([0xff; 6]);
    }
    let next_hop = if wire::same_subnet(state.ipv4, remote_ip, state.subnet) {
        remote_ip
    } else {
        state.gateway
    };
    if next_hop == state.gateway && state.gateway_mac != [0; 6] {
        Some(state.gateway_mac)
    } else {
        resolve_arp(state, next_hop)
    }
}

fn resolve_route_v6(state: &mut State, remote_ip: [u8; 16]) -> Option<[u8; 6]> {
    if !state.ipv6_ready || remote_ip == [0; 16] {
        return None;
    }
    if remote_ip[0] == 0xff {
        return Some(wire::multicast_mac(remote_ip));
    }
    let on_link = state.ipv6_prefix_length == 64 && remote_ip[..8] == state.ipv6[..8];
    let next_hop = if on_link {
        remote_ip
    } else {
        state.ipv6_gateway
    };
    if next_hop == state.ipv6_gateway && state.ipv6_gateway_mac != [0; 6] {
        Some(state.ipv6_gateway_mac)
    } else {
        resolve_ndp(state, next_hop)
    }
}

fn resolve_ndp(state: &mut State, target: [u8; 16]) -> Option<[u8; 6]> {
    let mut frame = [0u8; FRAME_CAPACITY];
    let length = wire::build_neighbor_solicitation(&mut frame, state.mac, state.ipv6, target)?;
    transmit_frame(state, &frame[..length])?;
    for _ in 0..WAIT_SPINS {
        let Some(length) = poll_receive(state, &mut frame) else {
            core::hint::spin_loop();
            continue;
        };
        if let Some(mac) = wire::parse_neighbor_advertisement(&frame[..length], state.ipv6, target)
        {
            return Some(mac);
        }
    }
    None
}

fn resolve_arp(state: &mut State, target: [u8; 4]) -> Option<[u8; 6]> {
    let mut frame = [0; FRAME_CAPACITY];
    let length = wire::build_arp_request(&mut frame, state.mac, state.ipv4, target)?;
    transmit_frame(state, &frame[..length])?;
    for _ in 0..WAIT_SPINS {
        let Some(length) = poll_receive(state, &mut frame) else {
            core::hint::spin_loop();
            continue;
        };
        if let Some(mac) = wire::parse_arp_reply(&frame[..length], state.mac, state.ipv4, target) {
            return Some(mac);
        }
    }
    None
}

fn wait_udp(
    state: &mut State,
    remote_ip: [u8; 4],
    remote_port: u16,
    local_port: u16,
    output: &mut [u8],
) -> Option<usize> {
    let mut frame = [0; FRAME_CAPACITY];
    for _ in 0..WAIT_SPINS {
        let Some(length) = poll_receive(state, &mut frame) else {
            core::hint::spin_loop();
            continue;
        };
        let Some(UdpPacket {
            source_ip,
            destination_ip,
            source_port,
            destination_port,
            payload,
        }) = wire::parse_udp(&frame[..length])
        else {
            continue;
        };
        if source_ip == remote_ip
            && destination_ip == state.ipv4
            && source_port == remote_port
            && destination_port == local_port
            && payload.len() <= output.len()
        {
            output[..payload.len()].copy_from_slice(payload);
            return Some(payload.len());
        }
    }
    None
}

fn wait_tcp(
    state: &mut State,
    remote_ip: [u8; 4],
    remote_port: u16,
    local_port: u16,
) -> Option<OwnedTcpSegment> {
    let mut frame = [0; FRAME_CAPACITY];
    for _ in 0..WAIT_SPINS {
        let Some(length) = poll_receive(state, &mut frame) else {
            core::hint::spin_loop();
            continue;
        };
        let Some(segment) = wire::parse_tcp(&frame[..length]) else {
            continue;
        };
        if segment.source_ip == remote_ip
            && segment.destination_ip == state.ipv4
            && segment.source_port == remote_port
            && segment.destination_port == local_port
        {
            return OwnedTcpSegment::copy_from(segment);
        }
    }
    None
}

fn wait_tcp_v6(
    state: &mut State,
    remote_ip: [u8; 16],
    remote_port: u16,
    local_port: u16,
) -> Option<OwnedTcpSegment> {
    let mut frame = [0; FRAME_CAPACITY];
    for _ in 0..WAIT_SPINS {
        let Some(length) = poll_receive(state, &mut frame) else {
            core::hint::spin_loop();
            continue;
        };
        let Some(segment) = wire::parse_tcp_v6(&frame[..length]) else {
            continue;
        };
        if segment.source_ip == remote_ip
            && segment.destination_ip == state.ipv6
            && segment.source_port == remote_port
            && segment.destination_port == local_port
        {
            return OwnedTcpSegment::copy_from_v6(segment);
        }
    }
    None
}

struct OwnedTcpSegment {
    sequence: u32,
    acknowledgment: u32,
    flags: u8,
    payload: [u8; TCP_PAYLOAD_MAX],
    payload_length: usize,
}

impl OwnedTcpSegment {
    fn copy_from(segment: TcpSegment<'_>) -> Option<Self> {
        if segment.payload.len() > TCP_PAYLOAD_MAX {
            return None;
        }
        let mut payload = [0; TCP_PAYLOAD_MAX];
        payload[..segment.payload.len()].copy_from_slice(segment.payload);
        Some(Self {
            sequence: segment.sequence,
            acknowledgment: segment.acknowledgment,
            flags: segment.flags,
            payload,
            payload_length: segment.payload.len(),
        })
    }

    fn payload(&self) -> &[u8] {
        &self.payload[..self.payload_length]
    }

    fn copy_from_v6(segment: TcpSegmentV6<'_>) -> Option<Self> {
        if segment.payload.len() > TCP_PAYLOAD_MAX {
            return None;
        }
        let mut payload = [0; TCP_PAYLOAD_MAX];
        payload[..segment.payload.len()].copy_from_slice(segment.payload);
        Some(Self {
            sequence: segment.sequence,
            acknowledgment: segment.acknowledgment,
            flags: segment.flags,
            payload,
            payload_length: segment.payload.len(),
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn send_tcp(
    state: &mut State,
    remote_mac: [u8; 6],
    remote_ip: [u8; 4],
    local_port: u16,
    remote_port: u16,
    sequence: u32,
    acknowledgment: u32,
    flags: u8,
    receive_window: u16,
    payload: &[u8],
) -> Option<()> {
    let mut frame = [0; FRAME_CAPACITY];
    let id = next_identification(state);
    let length = wire::build_tcp(
        &mut frame,
        state.mac,
        remote_mac,
        state.ipv4,
        remote_ip,
        local_port,
        remote_port,
        sequence,
        acknowledgment,
        flags,
        receive_window,
        payload,
        id,
    )?;
    transmit_frame(state, &frame[..length])
}

#[allow(clippy::too_many_arguments)]
fn send_tcp_v6(
    state: &mut State,
    remote_mac: [u8; 6],
    remote_ip: [u8; 16],
    local_port: u16,
    remote_port: u16,
    sequence: u32,
    acknowledgment: u32,
    flags: u8,
    receive_window: u16,
    payload: &[u8],
) -> Option<()> {
    let mut frame = [0; FRAME_CAPACITY];
    let length = wire::build_tcp_v6(
        &mut frame,
        state.mac,
        remote_mac,
        state.ipv6,
        remote_ip,
        local_port,
        remote_port,
        sequence,
        acknowledgment,
        flags,
        receive_window,
        payload,
    )?;
    transmit_frame(state, &frame[..length])
}

fn next_identification(state: &mut State) -> u16 {
    let value = state.identification;
    state.identification = value.wrapping_add(1).max(1);
    value
}

fn transmit_frame(state: &mut State, frame: &[u8]) -> Option<()> {
    if crate::arch::cpu_index() != 0 {
        crate::fatal("AArch64 virtio-net TX attempted from non-owner CPU");
    }
    if frame.is_empty() || frame.len() + VIRTIO_NET_HEADER as usize > BUFFER_BYTES as usize {
        return None;
    }
    let queue = &mut state.transmit;
    let buffer = queue.buffers[0];
    for offset in 0..VIRTIO_NET_HEADER {
        write8_memory(buffer + offset, 0);
    }
    for (offset, byte) in frame.iter().copied().enumerate() {
        write8_memory(buffer + VIRTIO_NET_HEADER + offset as u64, byte);
    }
    write_descriptor(
        queue.ring + DESC_OFFSET,
        buffer,
        VIRTIO_NET_HEADER as u32 + frame.len() as u32,
        0,
    );
    let slot = u64::from(queue.available % QUEUE_SIZE);
    write16_memory(queue.ring + AVAIL_OFFSET + 4 + slot * 2, 0);
    memory_barrier();
    queue.available = queue.available.wrapping_add(1);
    write16_memory(queue.ring + AVAIL_OFFSET + 2, queue.available);
    memory_barrier();
    write32(state.base + REG_QUEUE_NOTIFY, TRANSMIT_QUEUE);
    let expected = queue.used.wrapping_add(1);
    for _ in 0..WAIT_SPINS {
        memory_barrier();
        if read16_memory(queue.ring + USED_OFFSET + 2) == expected {
            let slot = u64::from(queue.used % QUEUE_SIZE);
            if read32_memory(queue.ring + USED_OFFSET + 4 + slot * 8) != 0 {
                return None;
            }
            queue.used = expected;
            acknowledge_interrupt(state.base);
            return Some(());
        }
        core::hint::spin_loop();
    }
    None
}

fn poll_receive(state: &mut State, output: &mut [u8]) -> Option<usize> {
    let queue = &mut state.receive;
    memory_barrier();
    let device_used = read16_memory(queue.ring + USED_OFFSET + 2);
    if queue.used == device_used {
        acknowledge_interrupt(state.base);
        return None;
    }
    let used_slot = u64::from(queue.used % QUEUE_SIZE);
    let descriptor = read32_memory(queue.ring + USED_OFFSET + 4 + used_slot * 8);
    let total = read32_memory(queue.ring + USED_OFFSET + 8 + used_slot * 8) as usize;
    if descriptor >= u32::from(QUEUE_SIZE) || total < VIRTIO_NET_HEADER as usize {
        fail(state.base, "virtio-net malformed RX completion");
    }
    let frame_length = total - VIRTIO_NET_HEADER as usize;
    let copied = frame_length.min(output.len());
    let buffer = queue.buffers[descriptor as usize];
    for (offset, byte) in output[..copied].iter_mut().enumerate() {
        *byte = read8_memory(buffer + VIRTIO_NET_HEADER + offset as u64);
    }
    let available_slot = u64::from(queue.available % QUEUE_SIZE);
    write16_memory(
        queue.ring + AVAIL_OFFSET + 4 + available_slot * 2,
        descriptor as u16,
    );
    queue.available = queue.available.wrapping_add(1);
    write16_memory(queue.ring + AVAIL_OFFSET + 2, queue.available);
    queue.used = queue.used.wrapping_add(1);
    memory_barrier();
    write32(state.base + REG_QUEUE_NOTIFY, RECEIVE_QUEUE);
    acknowledge_interrupt(state.base);
    (frame_length <= output.len()).then_some(copied)
}

fn acknowledge_interrupt(base: u64) {
    let status = read32(base + REG_INTERRUPT_STATUS);
    if status != 0 {
        write32(base + REG_INTERRUPT_ACK, status);
    }
}

fn allocate_zeroed_frame(message: &'static str) -> u64 {
    let frame = crate::mm::allocate_frame().unwrap_or_else(|| crate::fatal(message));
    unsafe { core::ptr::write_bytes(frame as *mut u8, 0, 4096) };
    frame
}

fn write_descriptor(address: u64, buffer: u64, length: u32, flags: u16) {
    write64_memory(address, buffer);
    write32_memory(address + 8, length);
    write16_memory(address + 12, flags);
    write16_memory(address + 14, 0);
}

fn write_address(register: u64, address: u64) {
    write32(register, address as u32);
    write32(register + 4, (address >> 32) as u32);
}

fn fail(base: u64, message: &'static str) -> ! {
    write32(base + REG_STATUS, read32(base + REG_STATUS) | STATUS_FAILED);
    crate::fatal(message)
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

fn memory_barrier() {
    compiler_fence(Ordering::SeqCst);
    unsafe { core::arch::asm!("dmb ish", options(nostack, preserves_flags)) };
    compiler_fence(Ordering::SeqCst);
}

#[inline(never)]
fn read32(address: u64) -> u32 {
    let value: u64;
    unsafe {
        core::arch::asm!(
            "ldr {value:w}, [{address}]",
            address = in(reg) address,
            value = lateout(reg) value,
            options(nostack, readonly),
        )
    };
    value as u32
}

#[inline(never)]
fn write32(address: u64, value: u32) {
    unsafe {
        core::arch::asm!(
            "str {value:w}, [{address}]",
            address = in(reg) address,
            value = in(reg) u64::from(value),
            options(nostack),
        )
    }
}

fn read8_memory(address: u64) -> u8 {
    unsafe { core::ptr::read_volatile(address as *const u8) }
}

fn write8_memory(address: u64, value: u8) {
    unsafe { core::ptr::write_volatile(address as *mut u8, value) }
}

fn read16_memory(address: u64) -> u16 {
    unsafe { core::ptr::read_volatile(address as *const u16) }
}

fn write16_memory(address: u64, value: u16) {
    unsafe { core::ptr::write_volatile(address as *mut u16, value) }
}

fn read32_memory(address: u64) -> u32 {
    unsafe { core::ptr::read_volatile(address as *const u32) }
}

fn write32_memory(address: u64, value: u32) {
    unsafe { core::ptr::write_volatile(address as *mut u32, value) }
}

fn write64_memory(address: u64, value: u64) {
    unsafe { core::ptr::write_volatile(address as *mut u64, value) }
}
