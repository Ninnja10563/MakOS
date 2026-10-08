//! Host event/counter/transport adapters for production TX and socket locks.
#![allow(dead_code)]

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

static SERIAL: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

#[macro_export]
macro_rules! serial_println {
    ($($argument:tt)*) => {{
        $crate::SERIAL.lock().unwrap().push(format!($($argument)*));
    }};
}

mod arch {
    use super::*;
    thread_local! { static CPU: Cell<usize> = const { Cell::new(0) }; }
    static CLOCK: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    pub static DEADLINES: AtomicU64 = AtomicU64::new(0);
    pub fn cpu_index() -> usize { CPU.get() }
    pub fn set_cpu(cpu: usize) { CPU.set(cpu); }
    fn now() -> u64 { CLOCK.get_or_init(Instant::now).elapsed().as_millis() as u64 }
    pub fn counter_deadline_millis(millis: u64) -> u64 {
        assert_eq!(millis, 5_000, "production request deadline must remain unchanged");
        DEADLINES.fetch_add(1, Ordering::AcqRel);
        now() + millis
    }
    pub fn counter_deadline_expired(deadline: u64) -> bool { now() >= deadline }
}

fn fatal(message: &str) -> ! { panic!("{message}") }

// Immutable guest-probe barriers are deliberately inert here: all completion
// must come from the production contention/timer helper, never a test hook.
mod aarch64_net_progress_probe {
    pub fn before_tx_publish(_: u8, _: &[u8]) {}
    pub fn after_tx_publish(_: u8, _: &[u8]) {}
    pub fn on_owner_socket_wait() {}
}

// The epoll lock also assists the pre-existing block-owner dependency. This
// network fixture records invocation only; it does not claim block I/O proof.
mod aarch64_virtio_blk {
    use std::sync::atomic::{AtomicU64, Ordering};
    pub static CALLS: AtomicU64 = AtomicU64::new(0);
    pub fn service_requests_while_waiting() -> usize {
        CALLS.fetch_add(1, Ordering::Relaxed);
        0
    }
}

mod aarch64_virtio_net {
    include!("net_queue.rs");

    struct State { ready: bool, ipv6_ready: bool }
    struct LockedState { lock: AtomicBool, value: UnsafeCell<State> }
    unsafe impl Sync for LockedState {}
    static STATE: LockedState = LockedState {
        lock: AtomicBool::new(false),
        value: UnsafeCell::new(State { ready: true, ipv6_ready: true }),
    };
    static OPERATIONS: AtomicU64 = AtomicU64::new(0);
    static KIND_OPERATIONS: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];
    static EXPECT_SOCKET_HELD: AtomicBool = AtomicBool::new(false);
    static TRANSPORT_FAIL: AtomicBool = AtomicBool::new(false);

    fn notify_tx_waiters() { std::thread::yield_now(); }
    fn wait_for_tx_event() { std::thread::yield_now(); }
    fn with_state<R>(action: impl FnOnce(&mut State) -> R) -> R {
        assert_eq!(crate::arch::cpu_index(), 0, "non-owner touched device");
        assert!(STATE.lock.compare_exchange(false, true, Ordering::Acquire,
            Ordering::Relaxed).is_ok(), "device lock entered recursively");
        let result = action(unsafe { &mut *STATE.value.get() });
        STATE.lock.store(false, Ordering::Release);
        result
    }
    fn ipv4(seed: u8) -> [u8; 4] { [10, 0, 2, seed] }
    fn ipv6(seed: u8) -> [u8; 16] {
        let mut address = [0u8; 16];
        address[0] = 0xfd;
        address[15] = seed;
        address
    }
    fn mac(seed: u8) -> [u8; 6] { [0x52, 0x54, 0, 0x12, 0x34, seed] }
    fn seed(remote: u16, local: u16) -> u8 {
        let seed: u8 = (remote - 12_000).try_into().unwrap();
        assert_eq!(local, 40_000 + u16::from(seed));
        seed
    }
    fn record(kind: u8) -> bool {
        assert_eq!(crate::arch::cpu_index(), 0, "non-owner touched device");
        if EXPECT_SOCKET_HELD.load(Ordering::Acquire) {
            assert!(crate::socket::locked(), "AP must retain socket serialization through TX");
        }
        OPERATIONS.fetch_add(1, Ordering::AcqRel);
        KIND_OPERATIONS[usize::from(kind - 1)].fetch_add(1, Ordering::AcqRel);
        !TRANSPORT_FAIL.load(Ordering::Acquire)
    }
    fn check_payload(seed: u8, payload: &[u8]) {
        assert!(payload.iter().enumerate().all(|(index, value)|
            *value == seed.wrapping_add(index as u8)), "copied payload identity mismatch");
    }
    fn udp_send_on_owner(ip: [u8; 4], remote: u16, local: u16, payload: &[u8]) -> Option<usize> {
        with_state(|_| {
            let seed = seed(remote, local);
            assert_eq!(ip, ipv4(seed));
            check_payload(seed, payload);
            record(TX_KIND_UDP4).then_some(payload.len())
        })
    }
    fn udp6_send_on_owner(ip: [u8; 16], remote: u16, local: u16, payload: &[u8]) -> Option<usize> {
        with_state(|_| {
            let seed = seed(remote, local);
            assert_eq!(ip, ipv6(seed));
            check_payload(seed, payload);
            record(TX_KIND_UDP6).then_some(payload.len())
        })
    }
    fn tcp_connect_on_owner(ip: [u8; 4], remote: u16, local: u16) -> Option<TcpConnection> {
        with_state(|_| {
            let seed = seed(remote, local);
            assert_eq!(ip, ipv4(seed));
            record(TX_KIND_TCP4_CONNECT).then_some(TcpConnection {
                remote_ip: ip, remote_mac: mac(seed), local_port: local,
                remote_port: remote, transmit_sequence: 0x1000 + u32::from(seed),
                receive_sequence: 0x2000 + u32::from(seed),
                receive_window: 4096 + u16::from(seed), closed: false,
            })
        })
    }
    fn tcp6_connect_on_owner(ip: [u8; 16], remote: u16, local: u16) -> Option<Tcp6Connection> {
        with_state(|_| {
            let seed = seed(remote, local);
            assert_eq!(ip, ipv6(seed));
            record(TX_KIND_TCP6_CONNECT).then_some(Tcp6Connection {
                remote_ip: ip, remote_mac: mac(seed), local_port: local,
                remote_port: remote, transmit_sequence: 0x1000 + u32::from(seed),
                receive_sequence: 0x2000 + u32::from(seed),
                receive_window: 4096 + u16::from(seed), closed: false,
            })
        })
    }
    fn segment(kind: u8, remote_mac: [u8; 6], local: u16, remote: u16,
               sequence: u32, ack: u32, flags: u8, window: u16, payload: &[u8]) -> Option<()> {
        let seed = seed(remote, local);
        assert_eq!(remote_mac, mac(seed));
        assert_eq!(sequence, 0x1000 + u32::from(seed));
        assert_eq!(ack, 0x2000 + u32::from(seed));
        assert_eq!(window, 4096 + u16::from(seed));
        assert!(matches!(flags, 0x10 | 0x11 | 0x18));
        check_payload(seed, payload);
        record(kind).then_some(())
    }
    fn send_tcp(_: &mut State, remote_mac: [u8; 6], ip: [u8; 4], local: u16,
                remote: u16, sequence: u32, ack: u32, flags: u8,
                window: u16, payload: &[u8]) -> Option<()> {
        assert_eq!(ip, ipv4(seed(remote, local)));
        segment(TX_KIND_TCP4_SEGMENT, remote_mac, local, remote, sequence, ack, flags, window, payload)
    }
    fn send_tcp_v6(_: &mut State, remote_mac: [u8; 6], ip: [u8; 16], local: u16,
                   remote: u16, sequence: u32, ack: u32, flags: u8,
                   window: u16, payload: &[u8]) -> Option<()> {
        assert_eq!(ip, ipv6(seed(remote, local)));
        segment(TX_KIND_TCP6_SEGMENT, remote_mac, local, remote, sequence, ack, flags, window, payload)
    }

    #[derive(Debug)]
    pub struct Outcome {
        pub count: usize, pub remote_mac: [u8; 6], pub transmit: u32,
        pub receive: u32, pub window: u16,
    }
    fn request(kind: u8, seed: u8, length: usize, flags: u8) -> TxServiceRequest {
        let mut request = TxServiceRequest::EMPTY;
        request.kind = kind;
        if matches!(kind, TX_KIND_UDP6 | TX_KIND_TCP6_CONNECT | TX_KIND_TCP6_SEGMENT) {
            request.remote_ip = ipv6(seed);
        } else {
            request.remote_ip[..4].copy_from_slice(&ipv4(seed));
        }
        request.remote_mac = mac(seed);
        request.local_port = 40_000 + u16::from(seed);
        request.remote_port = 12_000 + u16::from(seed);
        request.sequence = 0x1000 + u32::from(seed);
        request.acknowledgment = 0x2000 + u32::from(seed);
        request.flags = flags;
        request.receive_window = 4096 + u16::from(seed);
        request.length = length.try_into().unwrap();
        for (index, value) in request.payload.iter_mut().enumerate() {
            *value = seed.wrapping_add(index as u8);
        }
        request
    }
    pub fn run(kind: u8, seed: u8, length: usize, flags: u8) -> Option<Outcome> {
        let result = queue_tx_request(request(kind, seed, length, flags), kind >= 3)?;
        Some(Outcome { count: result.count, remote_mac: result.remote_mac,
            transmit: result.transmit_sequence, receive: result.receive_sequence,
            window: result.receive_window })
    }
    pub fn pending() -> usize {
        TX_SERVICE.iter().filter(|slot| slot.state.load(Ordering::Acquire) == TX_SLOT_READY).count()
    }
    pub fn all_free() -> bool {
        TX_SERVICE.iter().all(|slot| slot.state.load(Ordering::Acquire) == TX_SLOT_FREE)
    }
    pub fn operations() -> u64 { OPERATIONS.load(Ordering::Acquire) }
    pub fn kind_operations(kind: usize) -> u64 { KIND_OPERATIONS[kind - 1].load(Ordering::Acquire) }
    pub fn lock_device(locked: bool) { STATE.lock.store(locked, Ordering::Release); }
    pub fn lock_service(locked: bool) { TX_OWNER_ACTIVE.store(locked, Ordering::Release); }
    pub fn expect_socket(held: bool) { EXPECT_SOCKET_HELD.store(held, Ordering::Release); }
    pub fn fail_transport(fail: bool) { TRANSPORT_FAIL.store(fail, Ordering::Release); }
    pub fn report_timeout() {
        report_tx_timeout(&TX_SERVICE[3], &request(TX_KIND_UDP4, 7, 5, 0));
    }
    pub fn reset() {
        assert!(all_free(), "previous test left live requests");
        assert!(!STATE.lock.load(Ordering::Acquire));
        assert!(!TX_OWNER_ACTIVE.load(Ordering::Acquire));
        reset_tx_affinity_evidence();
        OPERATIONS.store(0, Ordering::Release);
        for count in &KIND_OPERATIONS { count.store(0, Ordering::Release); }
        EXPECT_SOCKET_HELD.store(false, Ordering::Release);
        TRANSPORT_FAIL.store(false, Ordering::Release);
        crate::arch::DEADLINES.store(0, Ordering::Release);
        crate::aarch64_virtio_blk::CALLS.store(0, Ordering::Release);
        crate::arch::set_cpu(0);
        crate::SERIAL.lock().unwrap().clear();
    }
}

mod socket {
    use core::cell::UnsafeCell;
    use core::sync::atomic::{AtomicBool, Ordering};
    pub struct State { pub published: u64 }
    struct LockedState { lock: AtomicBool, value: UnsafeCell<State> }
    unsafe impl Sync for LockedState {}
    static STATE: LockedState = LockedState {
        lock: AtomicBool::new(false), value: UnsafeCell::new(State { published: 0 }),
    };
    include!("socket_lock.rs");
    pub fn run<R>(action: impl FnOnce(&mut State) -> R) -> R { with_state(action) }
    pub fn locked() -> bool { STATE.lock.load(Ordering::Acquire) }
}

mod epoll {
    use core::cell::UnsafeCell;
    use core::sync::atomic::{AtomicBool, Ordering};
    const MAX_INSTANCES: usize = 4;
    const MAX_WATCHES: usize = 16;
    pub struct Table<const INSTANCES: usize, const WATCHES: usize> {
        pub observed: u64,
    }
    struct LockedState {
        lock: AtomicBool,
        table: UnsafeCell<Table<MAX_INSTANCES, MAX_WATCHES>>,
    }
    unsafe impl Sync for LockedState {}
    static STATE: LockedState = LockedState {
        lock: AtomicBool::new(false), table: UnsafeCell::new(Table { observed: 0 }),
    };
    include!("epoll_lock.rs");
    pub fn run<R>(action: impl FnOnce(&mut Table<MAX_INSTANCES, MAX_WATCHES>) -> R) -> R {
        with_table(action)
    }
}

fn await_pending(count: usize) {
    let start = Instant::now();
    while aarch64_virtio_net::pending() != count {
        assert!(start.elapsed() < Duration::from_secs(2), "AP did not publish request");
        thread::yield_now();
    }
}

#[test]
fn socket_contention_completes_ap_tx_without_timer() {
    aarch64_virtio_net::reset();
    socket::run(|state| state.published = 0);
    aarch64_virtio_net::expect_socket(true);
    // Reproduce the check/publication race: CPU0's ordinary pre-RX drain has
    // found no request, then AP publishes while holding the socket lock.
    assert_eq!(aarch64_virtio_net::service_tx_requests(), 0);
    let (acquired, acquisition) = mpsc::channel();
    let ap = thread::spawn(move || {
        arch::set_cpu(1);
        socket::run(|state| {
            acquired.send(()).unwrap();
            for (kind, length, flags) in [(1, 32, 0), (5, 1400, 0x18), (6, 0, 0x10)] {
                let outcome = aarch64_virtio_net::run(kind, kind, length, flags).unwrap();
                assert_eq!(outcome.count, length);
                state.published += 1;
            }
        });
    });
    acquisition.recv_timeout(Duration::from_secs(2)).unwrap();
    await_pending(1);
    assert_eq!(aarch64_virtio_net::operations(), 0);
    let (finished, completion) = mpsc::channel();
    let owner = thread::spawn(move || {
        arch::set_cpu(0);
        socket::run(|state| {
            assert_eq!(state.published, 3, "CPU0 observed partial socket publication");
            finished.send(()).unwrap();
        });
    });
    completion.recv_timeout(Duration::from_secs(2)).expect("CPU0/AP socket-TX circular wait");
    ap.join().unwrap();
    owner.join().unwrap();
    assert_eq!(aarch64_virtio_net::tx_affinity_evidence(), (3, 3));
    assert_eq!(aarch64_virtio_net::progress_evidence(), (0, 3, 0));
    assert_eq!(aarch64_virtio_net::tcp_tx_affinity_evidence(), (2, 2, 0, 1, 1, 0));
    assert_eq!(arch::DEADLINES.load(Ordering::Acquire), 3);
    assert!(aarch64_virtio_net::all_free());
}

#[test]
fn epoll_socket_chain_completes_ap_tx_without_timer() {
    aarch64_virtio_net::reset();
    socket::run(|state| state.published = 0);
    epoll::run(|table| table.observed = 0);
    aarch64_virtio_net::expect_socket(true);
    let ap_tx = thread::spawn(|| {
        arch::set_cpu(1);
        socket::run(|state| {
            let result = aarch64_virtio_net::run(1, 11, 128, 0).unwrap();
            assert_eq!(result.count, 128);
            state.published = 42;
        });
    });
    await_pending(1);
    // AP2 holds the production epoll table lock while its readiness callback
    // contends for socket state held by AP1. Being a non-owner, AP2 cannot
    // service the copied TX request in the socket contention helper.
    let (held, epoll_held) = mpsc::channel();
    let ap_epoll = thread::spawn(move || {
        arch::set_cpu(2);
        epoll::run(|table| {
            held.send(()).unwrap();
            socket::run(|state| {
                assert_eq!(state.published, 42);
                table.observed = state.published;
            });
        });
    });
    epoll_held.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(aarch64_virtio_net::operations(), 0);
    let (finished, completion) = mpsc::channel();
    let owner = thread::spawn(move || {
        arch::set_cpu(0);
        epoll::run(|table| {
            assert_eq!(table.observed, 42, "CPU0 observed incomplete readiness callback");
            finished.send(()).unwrap();
        });
    });
    completion.recv_timeout(Duration::from_secs(2))
        .expect("CPU0/AP epoll-socket-TX circular wait");
    ap_tx.join().unwrap();
    ap_epoll.join().unwrap();
    owner.join().unwrap();
    assert_eq!(aarch64_virtio_net::tx_affinity_evidence(), (1, 1));
    assert_eq!(aarch64_virtio_net::progress_evidence(), (0, 1, 0));
    assert_eq!(arch::DEADLINES.load(Ordering::Acquire), 1);
    assert!(aarch64_virtio_blk::CALLS.load(Ordering::Acquire) > 0,
        "epoll retained its block-owner hook (invocation, not block I/O proof)");
    assert!(aarch64_virtio_net::all_free());
}

#[test]
fn all_six_kinds_keep_request_and_result_identity() {
    aarch64_virtio_net::reset();
    let mut aps = Vec::new();
    for kind in 1..=6 {
        aps.push(thread::spawn(move || {
            arch::set_cpu(usize::from(kind));
            let length = if matches!(kind, 3 | 4) { 0 } else { 35 + usize::from(kind) };
            let result = aarch64_virtio_net::run(kind, kind + 10, length, 0x18).unwrap();
            assert_eq!(result.count, length);
            if matches!(kind, 3 | 4) {
                assert_eq!(result.remote_mac, [0x52, 0x54, 0, 0x12, 0x34, kind + 10]);
                assert_eq!(result.transmit, 0x1000 + u32::from(kind + 10));
                assert_eq!(result.receive, 0x2000 + u32::from(kind + 10));
                assert_eq!(result.window, 4096 + u16::from(kind + 10));
            }
        }));
    }
    await_pending(6);
    assert_eq!(aarch64_virtio_net::service_tx_requests(), 6);
    for ap in aps { ap.join().unwrap(); }
    assert_eq!(aarch64_virtio_net::tx_affinity_evidence(), (6, 6));
    assert_eq!(aarch64_virtio_net::tcp_tx_affinity_evidence(), (4, 4, 2, 2, 0, 0));
    for kind in 1..=6 { assert_eq!(aarch64_virtio_net::kind_operations(kind), 1); }
    assert!(aarch64_virtio_net::all_free());
}

#[test]
fn locked_network_device_defers_without_consuming_request() {
    aarch64_virtio_net::reset();
    let ap = thread::spawn(|| { arch::set_cpu(1); aarch64_virtio_net::run(1, 7, 64, 0) });
    await_pending(1);
    aarch64_virtio_net::lock_device(true);
    assert_eq!(aarch64_virtio_net::service_tx_requests_while_waiting(), 0);
    assert_eq!(aarch64_virtio_net::service_tx_requests_from_timer(), 0);
    assert_eq!(aarch64_virtio_net::pending(), 1);
    assert_eq!(aarch64_virtio_net::operations(), 0);
    aarch64_virtio_net::lock_device(false);
    assert_eq!(aarch64_virtio_net::service_tx_requests_from_timer(), 1);
    assert_eq!(ap.join().unwrap().unwrap().count, 64);
    assert_eq!(aarch64_virtio_net::progress_evidence(), (1, 0, 2));
}

#[test]
fn active_service_defers_between_device_acquisitions() {
    aarch64_virtio_net::reset();
    let ap = thread::spawn(|| { arch::set_cpu(2); aarch64_virtio_net::run(2, 8, 71, 0) });
    await_pending(1);
    aarch64_virtio_net::lock_service(true);
    assert_eq!(aarch64_virtio_net::service_tx_requests_from_timer(), 0,
        "recursive TX service must defer");
    assert_eq!(aarch64_virtio_net::service_tx_requests_while_waiting(), 0);
    assert_eq!(aarch64_virtio_net::pending(), 1);
    assert_eq!(aarch64_virtio_net::operations(), 0);
    aarch64_virtio_net::lock_service(false);
    assert_eq!(aarch64_virtio_net::service_tx_requests_from_timer(), 1);
    assert_eq!(ap.join().unwrap().unwrap().count, 71);
    assert_eq!(aarch64_virtio_net::progress_evidence(), (1, 0, 2));
}

#[test]
fn nonowner_service_is_rejected_without_device_access() {
    aarch64_virtio_net::reset();
    arch::set_cpu(1);
    assert_eq!(aarch64_virtio_net::service_tx_requests_while_waiting(), 0);
    assert!(std::panic::catch_unwind(aarch64_virtio_net::service_tx_requests).is_err(),
        "non-owner direct service was accepted");
    assert!(std::panic::catch_unwind(aarch64_virtio_net::service_tx_requests_from_timer).is_err(),
        "non-owner timer service was accepted");
    assert_eq!(aarch64_virtio_net::operations(), 0);
    assert_eq!(aarch64_virtio_net::progress_evidence(), (0, 0, 0));
    arch::set_cpu(0);
}

#[test]
fn invalid_request_length_completes_failed_without_transport() {
    aarch64_virtio_net::reset();
    let ap = thread::spawn(|| { arch::set_cpu(1); aarch64_virtio_net::run(1, 9, 1401, 0) });
    await_pending(1);
    assert_eq!(aarch64_virtio_net::service_tx_requests_from_timer(), 1);
    assert_eq!(ap.join().unwrap().unwrap().count, usize::MAX);
    assert_eq!(aarch64_virtio_net::operations(), 0);
    assert_eq!(aarch64_virtio_net::tx_affinity_evidence(), (1, 1));
    assert!(aarch64_virtio_net::all_free());
}

#[test]
fn unknown_request_kind_completes_failed_without_transport() {
    aarch64_virtio_net::reset();
    let ap = thread::spawn(|| { arch::set_cpu(1); aarch64_virtio_net::run(99, 9, 4, 0) });
    await_pending(1);
    assert_eq!(aarch64_virtio_net::service_tx_requests(), 1);
    assert_eq!(ap.join().unwrap().unwrap().count, usize::MAX);
    assert_eq!(aarch64_virtio_net::operations(), 0);
    assert!(aarch64_virtio_net::all_free());
}

#[test]
fn full_queue_rejects_extra_request_without_overwriting_slots() {
    aarch64_virtio_net::reset();
    let mut aps = Vec::new();
    for seed in 1..=8 {
        aps.push(thread::spawn(move || {
            arch::set_cpu(usize::from(seed));
            aarch64_virtio_net::run(1, seed, usize::from(seed), 0).unwrap().count
        }));
    }
    await_pending(8);
    arch::set_cpu(3);
    assert!(aarch64_virtio_net::run(1, 30, 31, 0).is_none());
    assert_eq!(aarch64_virtio_net::tx_affinity_evidence(), (0, 8));
    arch::set_cpu(0);
    assert_eq!(aarch64_virtio_net::service_tx_requests(), 8);
    for (index, ap) in aps.into_iter().enumerate() { assert_eq!(ap.join().unwrap(), index + 1); }
    assert_eq!(aarch64_virtio_net::tx_affinity_evidence(), (8, 8));
    assert!(aarch64_virtio_net::all_free());
}

#[test]
fn failed_transport_result_is_not_counted_as_successful_tcp_data() {
    aarch64_virtio_net::reset();
    aarch64_virtio_net::fail_transport(true);
    let ap = thread::spawn(|| { arch::set_cpu(3); aarch64_virtio_net::run(5, 3, 87, 0x18) });
    await_pending(1);
    assert_eq!(aarch64_virtio_net::service_tx_requests_from_timer(), 1);
    assert_eq!(ap.join().unwrap().unwrap().count, usize::MAX);
    assert_eq!(aarch64_virtio_net::tcp_tx_affinity_evidence(), (1, 1, 0, 0, 0, 0));
    assert!(aarch64_virtio_net::all_free());
}

#[test]
fn ack_and_fin_preserve_distinct_completion_evidence() {
    aarch64_virtio_net::reset();
    for (kind, flags) in [(5, 0x10), (6, 0x11)] {
        let ap = thread::spawn(move || { arch::set_cpu(1); aarch64_virtio_net::run(kind, 3, 0, flags) });
        await_pending(1);
        assert_eq!(aarch64_virtio_net::service_tx_requests(), 1);
        assert_eq!(ap.join().unwrap().unwrap().count, 0);
    }
    assert_eq!(aarch64_virtio_net::tcp_tx_affinity_evidence(), (2, 2, 0, 0, 1, 1));
}

#[test]
fn timeout_diagnostic_uses_atomic_metadata_without_locking_device() {
    aarch64_virtio_net::reset();
    arch::set_cpu(2);
    aarch64_virtio_net::lock_device(true);
    aarch64_virtio_net::lock_service(true);
    aarch64_virtio_net::report_timeout();
    aarch64_virtio_net::lock_device(false);
    aarch64_virtio_net::lock_service(false);
    let logs = SERIAL.lock().unwrap();
    assert_eq!(logs.len(), 1);
    let record = &logs[0];
    assert!(record.starts_with("MAKOS_AARCH64_NET_TX_TIMEOUT cpu=2 slot=3 kind=1 state=0 length=5 "));
    assert!(record.contains("device_locked=1 owner_active=1"));
    assert!(record.contains("requests=0 completions=0 timer_completions=0 lock_wait_completions=0 busy_deferrals=0"));
    assert_eq!(aarch64_virtio_net::operations(), 0);
    arch::set_cpu(0);
}
