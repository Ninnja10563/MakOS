//! Production-derived source-contract fixture. Not device, lock, scheduler, or QEMU proof.
//! production.rs contains byte-exact candidate declarations; wire stays intact.

#[allow(dead_code)] // Only unused APIs of the complete unchanged wire module.
#[path = "aarch64_net_wire.rs"]
mod wire;
use wire::{FRAME_CAPACITY, TCP_PAYLOAD_MAX, TcpSegment, TcpSegmentV6};

// Host-only State/lock/TX adapters. No simulated RX or waiter return values.
struct State {
    ready: bool,
    base: u64,
    mac: [u8; 6],
    ipv4: [u8; 4],
    ipv6: [u8; 16],
    ipv6_link_local: [u8; 16],
    ipv6_prefix_length: u8,
    ipv6_gateway: [u8; 16],
    ipv6_gateway_mac: [u8; 6],
    ipv6_ready: bool,
    receive: Queue,
    deferred_rx: DeferredRx,
}
static STATE: std::sync::Mutex<Option<State>> = std::sync::Mutex::new(None);
static TRANSMITTED: std::sync::Mutex<Vec<Vec<u8>>> = std::sync::Mutex::new(Vec::new());
fn with_state<R>(function: impl FnOnce(&mut State) -> R) -> R {
    function(STATE.lock().unwrap().as_mut().expect("host state installed"))
}
mod arch { pub fn cpu_index() -> usize { 0 } }
fn fatal(message: &'static str) -> ! { panic!("unexpected production fatal: {message}") }
fn fail(_base: u64, message: &'static str) -> ! { fatal(message) }
fn transmit_frame(_state: &mut State, frame: &[u8]) -> Option<()> {
    assert!(!frame.is_empty() && frame.len() <= FRAME_CAPACITY);
    TRANSMITTED.lock().unwrap().push(frame.to_vec());
    Some(())
}
fn memory_barrier() { std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst); }
fn read32(address: u64) -> u32 { unsafe { core::ptr::read_volatile(address as *const u32) } }
fn write32(address: u64, value: u32) { unsafe { core::ptr::write_volatile(address as *mut u32, value) } }
include!("production.rs");

const MAC: [u8; 6] = [8, 9, 10, 11, 12, 13];
const PEER_MAC: [u8; 6] = [2, 3, 4, 5, 6, 7];
const LOCAL4: [u8; 4] = [10, 0, 2, 15];
const REMOTE4: [u8; 4] = [10, 0, 2, 2];
const LOCAL6: [u8; 16] = [0x21; 16];
const REMOTE6: [u8; 16] = [0x32; 16];
const LOCAL_PORT: u16 = 41000;
const MATCH_PAYLOAD: &[u8] = b"matched-reply";
const OTHER_PAYLOAD: &[u8] = b"unrelated-reply";
const SEQUENCE: u32 = 12345;
const ACKNOWLEDGMENT: u32 = 67890;
const FLAGS: u8 = 0x12;

#[derive(Clone, Copy)]
enum Kind { Udp, Tcp4, Tcp6 }
impl Kind {
    fn parse(value: &str) -> Self {
        match value { "udp" => Self::Udp, "tcp4" => Self::Tcp4,
            "tcp6" => Self::Tcp6, _ => panic!("unknown frame kind: {value}") }
    }
    fn remote_port(self) -> u16 { match self { Self::Udp => 53, _ => 443 } }
}
fn packet(kind: Kind, desired: bool) -> Vec<u8> {
    let mut bytes = [0u8; FRAME_CAPACITY];
    let source_port = kind.remote_port() + if desired { 0 } else { 7 };
    let destination_port = LOCAL_PORT + if desired { 0 } else { 7 };
    let payload = if desired { MATCH_PAYLOAD } else { OTHER_PAYLOAD };
    let length = match kind {
        Kind::Udp => wire::build_udp(&mut bytes, PEER_MAC, MAC, REMOTE4, LOCAL4,
            source_port, destination_port, payload, 9),
        Kind::Tcp4 => wire::build_tcp(&mut bytes, PEER_MAC, MAC, REMOTE4, LOCAL4,
            source_port, destination_port, SEQUENCE, ACKNOWLEDGMENT, FLAGS, 32768, payload, 10),
        Kind::Tcp6 => wire::build_tcp_v6(&mut bytes, PEER_MAC, MAC, REMOTE6, LOCAL6,
            source_port, destination_port, SEQUENCE, ACKNOWLEDGMENT, FLAGS, 32768, payload),
    }.expect("real wire builder");
    let frame = bytes[..length].to_vec();
    match kind {
        Kind::Udp => assert_eq!(wire::parse_udp(&frame).unwrap().payload, payload),
        Kind::Tcp4 => assert_eq!(wire::parse_tcp(&frame).unwrap().payload, payload),
        Kind::Tcp6 => assert_eq!(wire::parse_tcp_v6(&frame).unwrap().payload, payload),
    }
    frame
}

fn rejected_tuple(kind: Kind, field: &str) -> Vec<u8> {
    let mut source4 = REMOTE4;
    let mut destination4 = LOCAL4;
    let mut source6 = REMOTE6;
    let mut destination6 = LOCAL6;
    let mut source_port = kind.remote_port();
    let mut destination_port = LOCAL_PORT;
    match field {
        "source-ip" => { source4[3] ^= 1; source6[15] ^= 1; }
        "destination-ip" => { destination4[3] ^= 1; destination6[15] ^= 1; }
        "source-port" => source_port += 1,
        "destination-port" => destination_port += 1,
        _ => panic!("unknown tuple field"),
    }
    let mut bytes = [0; FRAME_CAPACITY];
    let length = match kind {
        Kind::Udp => wire::build_udp(&mut bytes, PEER_MAC, MAC, source4, destination4,
            source_port, destination_port, MATCH_PAYLOAD, 29),
        Kind::Tcp4 => wire::build_tcp(&mut bytes, PEER_MAC, MAC, source4, destination4,
            source_port, destination_port, SEQUENCE, ACKNOWLEDGMENT, FLAGS, 32768, MATCH_PAYLOAD, 30),
        Kind::Tcp6 => wire::build_tcp_v6(&mut bytes, PEER_MAC, MAC, source6, destination6,
            source_port, destination_port, SEQUENCE, ACKNOWLEDGMENT, FLAGS, 32768, MATCH_PAYLOAD),
    }.unwrap();
    let frame = bytes[..length].to_vec();
    match kind {
        Kind::Udp => {
            let decoded = wire::parse_udp(&frame).unwrap();
            assert_eq!((decoded.source_ip, decoded.destination_ip, decoded.source_port, decoded.destination_port),
                       (source4, destination4, source_port, destination_port));
            assert_eq!(decoded.payload, MATCH_PAYLOAD);
        }
        Kind::Tcp4 => {
            let decoded = wire::parse_tcp(&frame).unwrap();
            assert_eq!((decoded.source_ip, decoded.destination_ip, decoded.source_port, decoded.destination_port),
                       (source4, destination4, source_port, destination_port));
            assert_eq!(decoded.payload, MATCH_PAYLOAD);
        }
        Kind::Tcp6 => {
            let decoded = wire::parse_tcp_v6(&frame).unwrap();
            assert_eq!((decoded.source_ip, decoded.destination_ip, decoded.source_port, decoded.destination_port),
                       (source6, destination6, source_port, destination_port));
            assert_eq!(decoded.payload, MATCH_PAYLOAD);
        }
    }
    frame
}

// Fixture-only bootstrap reply construction; every reply preflighted by real
// parser. ICMPv6 checksum is input construction, never a replacement parser.
fn icmp6(source: [u8; 16], destination: [u8; 16], mut payload: Vec<u8>) -> Vec<u8> {
    let mut pseudo = Vec::new();
    pseudo.extend_from_slice(&source);
    pseudo.extend_from_slice(&destination);
    pseudo.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    pseudo.extend_from_slice(&[0, 0, 0, 58]);
    pseudo.extend_from_slice(&payload);
    let mut sum = 0u32;
    for word in pseudo.chunks(2) {
        sum += u32::from(u16::from_be_bytes([word[0], *word.get(1).unwrap_or(&0)]));
    }
    while sum >> 16 != 0 { sum = (sum & 0xffff) + (sum >> 16); }
    payload[2..4].copy_from_slice(&(!(sum as u16)).to_be_bytes());
    let mut frame = vec![0; 54 + payload.len()];
    frame[..6].copy_from_slice(&MAC);
    frame[6..12].copy_from_slice(&PEER_MAC);
    frame[12..14].copy_from_slice(&0x86ddu16.to_be_bytes());
    frame[14] = 0x60;
    frame[18..20].copy_from_slice(&(payload.len() as u16).to_be_bytes());
    frame[20] = 58;
    frame[21] = 255;
    frame[22..38].copy_from_slice(&source);
    frame[38..54].copy_from_slice(&destination);
    frame[54..].copy_from_slice(&payload);
    frame
}
fn bootstrap_frame(kind: &str) -> Vec<u8> {
    match kind {
        "dhcp" => {
            let mut payload = [0u8; 244];
            payload[..3].copy_from_slice(&[2, 1, 6]);
            payload[4..8].copy_from_slice(&DHCP_TRANSACTION.to_be_bytes());
            payload[16..20].copy_from_slice(&LOCAL4);
            payload[28..34].copy_from_slice(&MAC);
            payload[236..240].copy_from_slice(&[99, 130, 83, 99]);
            payload[240..].copy_from_slice(&[53, 1, 2, 255]);
            let mut output = [0; FRAME_CAPACITY];
            let length = wire::build_udp(&mut output, PEER_MAC, MAC, REMOTE4, LOCAL4,
                                        67, 68, &payload, 19).unwrap();
            let frame = output[..length].to_vec();
            assert_eq!(wire::parse_dhcp(&frame, DHCP_TRANSACTION, MAC).unwrap().message_type, 2);
            frame
        }
        "arp" => {
            let mut frame = vec![0; 42];
            assert_eq!(wire::build_arp_request(&mut frame, PEER_MAC, REMOTE4, LOCAL4), Some(42));
            frame[..6].copy_from_slice(&MAC);
            frame[20..22].copy_from_slice(&2u16.to_be_bytes());
            frame[32..38].copy_from_slice(&MAC);
            assert_eq!(wire::parse_arp_reply(&frame, MAC, LOCAL4, REMOTE4), Some(PEER_MAC));
            frame
        }
        "ndp" => {
            let mut payload = vec![0; 32];
            payload[0] = 136;
            payload[4] = 0x40;
            payload[8..24].copy_from_slice(&REMOTE6);
            payload[24..26].copy_from_slice(&[2, 1]);
            payload[26..32].copy_from_slice(&PEER_MAC);
            let frame = icmp6(REMOTE6, LOCAL6, payload);
            assert_eq!(wire::parse_neighbor_advertisement(&frame, LOCAL6, REMOTE6), Some(PEER_MAC));
            frame
        }
        "ipv6" => {
            let mut payload = vec![0; 48];
            payload[0] = 134;
            payload[6..8].copy_from_slice(&1800u16.to_be_bytes());
            payload[16..20].copy_from_slice(&[3, 4, 64, 0x40]);
            payload[20..24].copy_from_slice(&3600u32.to_be_bytes());
            payload[32..36].copy_from_slice(&[0x20, 1, 0x0d, 0xb8]);
            let mut source = [0; 16];
            source[..2].copy_from_slice(&[0xfe, 0x80]);
            source[15] = 1;
            let frame = icmp6(source, wire::link_local_from_mac(MAC), payload);
            assert_eq!(wire::parse_router_advertisement(&frame).unwrap().prefix_length, 64);
            frame
        }
        _ => panic!("unknown bootstrap kind"),
    }
}

struct HostRing {
    _ring: Box<[u64; 512]>,
    _mmio: Box<[u64; 64]>,
    _buffers: Vec<Box<[u64; 256]>>,
    posted: u16,
}
impl HostRing {
    fn new(frames: &[Vec<u8>]) -> Self {
        assert!(frames.len() <= QUEUE_SIZE as usize);
        assert_eq!(BUFFER_BYTES as usize, 256 * core::mem::size_of::<u64>());
        let mut ring = Box::new([0u64; 512]);
        let mut mmio = Box::new([0u64; 64]);
        let mut buffers = Vec::new();
        let mut queue = Queue::EMPTY;
        queue.ring = ring.as_mut_ptr() as u64;
        queue.available = QUEUE_SIZE;
        write16_memory(queue.ring + AVAIL_OFFSET + 2, queue.available);
        // Allocate every descriptor buffer, even when no completion is posted.
        for index in 0..QUEUE_SIZE as usize {
            let mut buffer = Box::new([0u64; 256]);
            let address = buffer.as_mut_ptr() as u64;
            queue.buffers[index] = address;
            if let Some(frame) = frames.get(index) {
                assert!(frame.len() + VIRTIO_NET_HEADER as usize <= BUFFER_BYTES as usize);
                unsafe { core::ptr::copy_nonoverlapping(frame.as_ptr(),
                    (address + VIRTIO_NET_HEADER) as *mut u8, frame.len()); }
                write32_memory(queue.ring + USED_OFFSET + 4 + index as u64 * 8, index as u32);
                write32_memory(queue.ring + USED_OFFSET + 8 + index as u64 * 8,
                               (frame.len() + VIRTIO_NET_HEADER as usize) as u32);
            }
            buffers.push(buffer);
        }
        write16_memory(queue.ring + USED_OFFSET + 2, frames.len() as u16);
        let base = mmio.as_mut_ptr() as u64;
        write32(base + REG_INTERRUPT_STATUS, 1);
        let mut slot = STATE.lock().unwrap();
        assert!(slot.is_none(), "single live host State owner");
        *slot = Some(State { ready: true, base, mac: MAC, ipv4: LOCAL4, ipv6: LOCAL6,
            ipv6_link_local: [0; 16], ipv6_prefix_length: 0, ipv6_gateway: [0; 16],
            ipv6_gateway_mac: [0; 6], ipv6_ready: false, receive: queue,
            deferred_rx: DeferredRx::EMPTY });
        Self { _ring: ring, _mmio: mmio, _buffers: buffers, posted: frames.len() as u16 }
    }
    fn check_consumed(&self, count: u16) {
        assert!(count <= self.posted);
        with_state(|state| {
            let queue = &state.receive;
            assert_eq!(queue.used, count, "production used-ring advancement");
            assert_eq!(queue.available, QUEUE_SIZE + count, "production RX repost count");
            assert_eq!(read16_memory(queue.ring + AVAIL_OFFSET + 2), QUEUE_SIZE + count);
            for index in 0..count {
                assert_eq!(read16_memory(queue.ring + AVAIL_OFFSET + 4 + u64::from(index) * 2), index);
            }
            if count != 0 {
                assert_eq!(read32(state.base + REG_QUEUE_NOTIFY), RECEIVE_QUEUE);
                assert_eq!(read32(state.base + REG_INTERRUPT_ACK), 1);
            }
        });
    }
    fn snapshot_raw(&self) -> (Vec<u64>, Vec<u64>, Vec<Vec<u64>>, u16, u16) {
        let (used, available) = with_state(|state| (state.receive.used, state.receive.available));
        (self._ring.to_vec(), self._mmio.to_vec(),
         self._buffers.iter().map(|buffer| buffer.to_vec()).collect(), used, available)
    }
    fn drain(&self) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        for _ in 0..=QUEUE_SIZE * 2 {
            let mut output = [0u8; FRAME_CAPACITY];
            let Some(length) = poll_frame(&mut output) else { return frames; };
            frames.push(output[..length].to_vec());
        }
        panic!("unexpected unbounded RX drain")
    }
}
impl Drop for HostRing {
    fn drop(&mut self) {
        // Clear pointer-bearing State before dropping backing allocations.
        // Poison recovery avoids a second panic masking a contract assertion.
        *STATE.lock().unwrap_or_else(|error| error.into_inner()) = None;
    }
}
fn matched_reply(kind: Kind) {
    with_state(|state| match kind {
        Kind::Udp => {
            let mut output = [0; FRAME_CAPACITY];
            let size = wait_udp(state, REMOTE4, kind.remote_port(), LOCAL_PORT, &mut output)
                .expect("desired UDP reply not reached");
            assert_eq!(&output[..size], MATCH_PAYLOAD);
        }
        Kind::Tcp4 | Kind::Tcp6 => {
            let segment = match kind {
                Kind::Tcp4 => wait_tcp(state, REMOTE4, kind.remote_port(), LOCAL_PORT),
                Kind::Tcp6 => wait_tcp_v6(state, REMOTE6, kind.remote_port(), LOCAL_PORT),
                Kind::Udp => unreachable!(),
            }.expect("desired TCP reply not reached");
            assert_eq!(segment.sequence, SEQUENCE);
            assert_eq!(segment.acknowledgment, ACKNOWLEDGMENT);
            assert_eq!(segment.flags, FLAGS);
            assert_eq!(segment.payload(), MATCH_PAYLOAD);
        }
    });
}
fn bootstrap(kind: &str) -> bool {
    with_state(|state| match kind {
        "dhcp" => wait_dhcp(state, 2).is_some_and(|reply| reply.address == LOCAL4),
        "arp" => resolve_arp(state, REMOTE4) == Some(PEER_MAC),
        "ndp" => resolve_ndp(state, REMOTE6) == Some(PEER_MAC),
        "ipv6" => {
            configure_ipv6(state);
            if state.ipv6_ready {
                assert_eq!(state.ipv6_prefix_length, 64);
                assert_eq!(state.ipv6_gateway_mac, PEER_MAC);
                assert_eq!(&state.ipv6[..4], &[0x20, 1, 0x0d, 0xb8]);
                assert_eq!(&state.ipv6[8..], &state.ipv6_link_local[8..]);
                assert_eq!(&state.ipv6_gateway[..2], &[0xfe, 0x80]);
            }
            state.ipv6_ready
        }
        _ => panic!("unknown bootstrap"),
    })
}
fn save_frames(name: &str, frames: &[Vec<u8>]) {
    let directory = std::path::PathBuf::from(std::env::args().nth(2).expect("input directory"));
    std::fs::create_dir_all(&directory).unwrap();
    let mut manifest = String::from("order,filename,bytes\n");
    for (index, frame) in frames.iter().enumerate() {
        let filename = format!("{name}-{index}.bin");
        std::fs::write(directory.join(&filename), frame).unwrap();
        manifest.push_str(&format!("{index},{filename},{}\n", frame.len()));
    }
    std::fs::write(directory.join(format!("{name}-manifest.csv")), manifest).unwrap();
}
fn saved_frames(count: usize) -> Vec<Vec<u8>> {
    (0..count).map(|index| {
        let mut frame = packet(Kind::Udp, false);
        // Distinct Ethernet source byte; UDP tuple/checksum remain valid.
        frame[11] = index as u8;
        assert!(wire::parse_udp(&frame).is_some());
        frame
    }).collect()
}
fn seed(frames: &[Vec<u8>]) {
    save_frames("deferred", frames);
    with_state(|state| for frame in frames { state.deferred_rx.push(frame); });
}
fn select(expected: &[u8]) -> Option<Vec<u8>> {
    with_state(|state| {
        let mut output = [0; FRAME_CAPACITY];
        wait_matching_frame(state, &mut output, WAIT_SPINS, |frame| frame == expected)
            .map(|length| output[..length].to_vec())
    })
}

fn main() {
    let case = std::env::args().nth(1).expect("case");
    if case == "fifo" {
        let frames = vec![packet(Kind::Udp, false), packet(Kind::Tcp4, false),
                          packet(Kind::Tcp6, false), packet(Kind::Udp, true)];
        save_frames("raw", &frames);
        let ring = HostRing::new(&frames);
        assert_eq!(ring.drain(), frames);
        ring.check_consumed(4);
    } else if let Some(kind) = case.strip_prefix("match-") {
        let kind = Kind::parse(kind);
        let frames = [packet(kind, true)];
        save_frames("raw", &frames);
        let ring = HostRing::new(&frames);
        matched_reply(kind);
        ring.check_consumed(1);
        assert!(ring.drain().is_empty());
    } else if let Some(pair) = case.strip_prefix("loss-") {
        let (waiter, unrelated) = pair.split_once('-').unwrap();
        let waiter = Kind::parse(waiter);
        let unrelated = packet(Kind::parse(unrelated), false);
        let frames = [unrelated.clone(), packet(waiter, true)];
        save_frames("raw", &frames);
        let ring = HostRing::new(&frames);
        matched_reply(waiter);
        ring.check_consumed(2);
        println!("RX_WAIT_MATCH_REACHED case={case} descriptors=2 reposted=2");
        assert!(ring.drain() == vec![unrelated], "CONTRACT_UNRELATED_RX_LOST");
    } else if case == "fifo-deferred" {
        let mut frames = saved_frames(3);
        let desired = packet(Kind::Tcp6, true);
        let tail = packet(Kind::Tcp4, false);
        frames.push(desired);
        frames.push(tail.clone());
        save_frames("raw", &frames);
        let ring = HostRing::new(&frames);
        matched_reply(Kind::Tcp6);
        ring.check_consumed(4);
        let expected = [saved_frames(3), vec![tail]].concat();
        assert!(ring.drain() == expected, "CONTRACT_DEFERRED_FIFO");
        ring.check_consumed(5);
    } else if let Some(pair) = case.strip_prefix("reject-") {
        let (kind, field) = pair.split_once('-').unwrap();
        let kind = Kind::parse(kind);
        let rejected = rejected_tuple(kind, field);
        let raw = [rejected.clone(), packet(kind, true)];
        save_frames("raw", &raw);
        let ring = HostRing::new(&raw);
        matched_reply(kind);
        // Same payload in both frames: ring witness proves rejecting wrong
        // field, not accidentally returning the first frame's matching payload.
        ring.check_consumed(2);
        assert!(ring.drain() == vec![rejected], "CONTRACT_TUPLE_REJECTION");
    } else if case == "malformed-length-handling" {
        // Preserve existing raw-consumer semantics, not a new retention claim:
        // zero bytes produce Some(0), oversized within valid DMA storage is
        // consumed/reposted but returns None. Neither becomes deferred data.
        let raw = [Vec::new(), vec![0x5a; FRAME_CAPACITY + 1], packet(Kind::Udp, true)];
        save_frames("raw", &raw);
        let ring = HostRing::new(&raw);
        matched_reply(Kind::Udp);
        ring.check_consumed(3);
        assert!(ring.drain().is_empty(), "CONTRACT_ORIGINAL_MALFORMED_HANDLING");
    } else if case == "short-output" {
        let raw = [packet(Kind::Tcp4, false)];
        save_frames("raw", &raw);
        let ring = HostRing::new(&raw);
        let saved = saved_frames(2);
        seed(&saved);
        let before = ring.snapshot_raw();
        let mut tiny = [0xa5; 1];
        assert!(poll_frame(&mut tiny).is_none(), "CONTRACT_SHORT_OUTPUT");
        assert!(tiny == [0xa5] && with_state(|state| state.deferred_rx.count) == 2,
                "CONTRACT_SHORT_OUTPUT");
        assert!(ring.snapshot_raw() == before, "CONTRACT_SHORT_OUTPUT");
        assert!(ring.drain() == [saved, raw.to_vec()].concat(), "CONTRACT_SHORT_OUTPUT");
        ring.check_consumed(1);
    } else if case == "interior-reuse" {
        save_frames("raw", &[]);
        let ring = HostRing::new(&[]);
        let saved = saved_frames(4);
        seed(&saved);
        assert!(select(&saved[1]) == Some(saved[1].clone()), "CONTRACT_INTERIOR_MATCH");
        let extra = packet(Kind::Tcp4, false);
        save_frames("appended", std::slice::from_ref(&extra));
        with_state(|state| state.deferred_rx.push(&extra));
        let expected = vec![saved[0].clone(), saved[2].clone(), saved[3].clone(), extra];
        assert!(ring.drain() == expected, "CONTRACT_INTERIOR_REUSE");
        ring.check_consumed(0);
    } else if case == "full-saved-match" {
        let raw = [packet(Kind::Tcp6, false)];
        save_frames("raw", &raw);
        let ring = HostRing::new(&raw);
        let mut saved = saved_frames(QUEUE_SIZE as usize);
        seed(&saved);
        let before = ring.snapshot_raw();
        let desired = saved.remove(7);
        assert!(select(&desired) == Some(desired), "CONTRACT_FULL_SAVED_MATCH");
        assert!(ring.snapshot_raw() == before, "CONTRACT_FULL_SAVED_MATCH");
        let extra = packet(Kind::Tcp4, false);
        save_frames("appended", std::slice::from_ref(&extra));
        with_state(|state| state.deferred_rx.push(&extra));
        saved.push(extra);
        saved.extend_from_slice(&raw);
        assert!(ring.drain() == saved, "CONTRACT_FULL_SAVED_MATCH");
        ring.check_consumed(1);
    } else if case == "full-raw-untouched" {
        let raw = [packet(Kind::Tcp6, true)];
        save_frames("raw", &raw);
        let ring = HostRing::new(&raw);
        let saved = saved_frames(QUEUE_SIZE as usize);
        seed(&saved);
        let before = ring.snapshot_raw();
        assert!(select(&raw[0]).is_none(), "CONTRACT_FULL_RAW_UNTOUCHED");
        assert!(ring.snapshot_raw() == before, "CONTRACT_FULL_RAW_UNTOUCHED");
        assert!(ring.drain() == [saved, raw.to_vec()].concat(), "CONTRACT_FULL_RAW_UNTOUCHED");
        ring.check_consumed(1);
    } else if let Some(kind) = case.strip_prefix("bootstrap-full-") {
        let raw = [bootstrap_frame(kind)];
        save_frames("raw", &raw);
        let ring = HostRing::new(&raw);
        let saved = saved_frames(QUEUE_SIZE as usize);
        seed(&saved);
        with_state(|state| state.ready = false);
        let before = ring.snapshot_raw();
        assert!(!bootstrap(kind), "CONTRACT_BOOTSTRAP_CAPACITY_FAILURE");
        assert!(ring.snapshot_raw() == before, "CONTRACT_BOOTSTRAP_CAPACITY_FAILURE");
        assert!(poll_frame(&mut [0; FRAME_CAPACITY]).is_none(), "CONTRACT_BOOTSTRAP_UNPUBLISHED");
        assert!(ring.snapshot_raw() == before, "CONTRACT_BOOTSTRAP_UNPUBLISHED");
        assert!(with_state(|state| state.deferred_rx.count) == QUEUE_SIZE as usize,
                "CONTRACT_BOOTSTRAP_UNPUBLISHED");
        // Explicit host-only republish, solely for retained-byte inspection.
        // NOT successful boot: configure_ipv4/fatal policy is not extracted.
        with_state(|state| state.ready = true);
        println!("RX_WAIT_HOST_INSPECTION_REPUBLISH case={case} successful-boot=false");
        assert!(ring.drain() == [saved, raw.to_vec()].concat(), "CONTRACT_BOOTSTRAP_CAPACITY_FAILURE");
        ring.check_consumed(1);
    } else if let Some(kind) = case.strip_prefix("bootstrap-match-") {
        let other = packet(Kind::Tcp4, false);
        let raw = [other.clone(), bootstrap_frame(kind)];
        save_frames("raw", &raw);
        let ring = HostRing::new(&raw);
        assert!(bootstrap(kind), "CONTRACT_BOOTSTRAP_MATCH");
        ring.check_consumed(2);
        assert!(ring.drain() == vec![other], "CONTRACT_BOOTSTRAP_PRESERVATION");
    } else if case == "udp-short-payload" {
        // Matching tuple but insufficient caller capacity remains deferred.
        // Fill remaining slots immediately, avoiding timeouts/spin-limit changes.
        let mut raw = vec![packet(Kind::Udp, true)];
        raw.extend(saved_frames(QUEUE_SIZE as usize - 1));
        save_frames("raw", &raw);
        let ring = HostRing::new(&raw);
        let result = with_state(|state| wait_udp(state, REMOTE4, 53, LOCAL_PORT, &mut [0; 1]));
        assert!(result.is_none(), "CONTRACT_UDP_CAPACITY_PREDICATE");
        ring.check_consumed(QUEUE_SIZE);
        assert!(ring.drain() == raw, "CONTRACT_UDP_CAPACITY_PREDICATE");
    } else {
        panic!("unknown case: {case}")
    }
    let transmitted = TRANSMITTED.lock().unwrap();
    save_frames("transmitted", &transmitted);
    println!("RX_WAIT_FIXED_OK case={case}");
}
