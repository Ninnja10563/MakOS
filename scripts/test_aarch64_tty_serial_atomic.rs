//! Host lock/PL011 adapters for the exact production serial output functions.
#![allow(dead_code)]

use core::fmt::{self, Write};
use makos_tty::{ByteSink, LineDiscipline, Termios};
use std::cell::Cell;
use std::sync::{Condvar, Mutex, MutexGuard};

include!("serial_output.rs");

static SERIAL_LOCK: Mutex<()> = Mutex::new(());
static CAPTURE: Mutex<Capture> = Mutex::new(Capture {
    bytes: Vec::new(),
    events: Vec::new(),
    next_guard: 0,
});
static GATE: Mutex<Gate> = Mutex::new(Gate {
    pause_at_first_byte: false,
    first_byte_seen: false,
    acquire_attempts: 0,
});
static GATE_CHANGED: Condvar = Condvar::new();

thread_local! {
    static ACTIVE_GUARD: Cell<usize> = const { Cell::new(0) };
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Event {
    Acquire(usize),
    Byte(usize, u8),
    Release(usize),
}

struct Capture {
    bytes: Vec<u8>,
    events: Vec<Event>,
    next_guard: usize,
}

struct Gate {
    pause_at_first_byte: bool,
    first_byte_seen: bool,
    acquire_attempts: usize,
}

struct SerialGuard {
    id: usize,
    _lock: MutexGuard<'static, ()>,
}

impl SerialGuard {
    fn acquire() -> Self {
        assert_eq!(ACTIVE_GUARD.get(), 0, "recursive serial lock acquisition");
        {
            let mut gate = GATE.lock().unwrap();
            gate.acquire_attempts += 1;
            GATE_CHANGED.notify_all();
        }
        let lock = SERIAL_LOCK.lock().unwrap();
        let id = {
            let mut capture = CAPTURE.lock().unwrap();
            capture.next_guard += 1;
            let id = capture.next_guard;
            capture.events.push(Event::Acquire(id));
            id
        };
        ACTIVE_GUARD.set(id);
        Self { id, _lock: lock }
    }
}

impl Drop for SerialGuard {
    fn drop(&mut self) {
        assert_eq!(ACTIVE_GUARD.get(), self.id);
        CAPTURE.lock().unwrap().events.push(Event::Release(self.id));
        ACTIVE_GUARD.set(0);
    }
}

struct Serial;

impl Serial {
    fn write_byte(&mut self, byte: u8) {
        let id = ACTIVE_GUARD.get();
        assert_ne!(id, 0, "serial byte emitted without the outer guard");
        {
            let mut capture = CAPTURE.lock().unwrap();
            capture.bytes.push(byte);
            capture.events.push(Event::Byte(id, byte));
        }
        let mut gate = GATE.lock().unwrap();
        if gate.pause_at_first_byte && !gate.first_byte_seen {
            gate.first_byte_seen = true;
            GATE_CHANGED.notify_all();
            // Force a second real host thread to attempt the same serial lock
            // while the first record owns it. This is deterministic contention,
            // not a sleep or a claim to execute AArch64 IRQ masking on the host.
            while gate.acquire_attempts < 2 {
                gate = GATE_CHANGED.wait(gate).unwrap();
            }
        }
    }
}

fn reset() {
    assert_eq!(ACTIVE_GUARD.get(), 0);
    let _serial = SERIAL_LOCK.lock().unwrap();
    *CAPTURE.lock().unwrap() = Capture {
        bytes: Vec::new(),
        events: Vec::new(),
        next_guard: 0,
    };
    *GATE.lock().unwrap() = Gate {
        pause_at_first_byte: false,
        first_byte_seen: false,
        acquire_attempts: 0,
    };
}

/// Reference the actual line discipline, followed by the pre-existing serial
/// LF translation. Deliberately preserve the historical ONLCR CRCRLF bytes.
fn old_tty_bytes(bytes: &[u8], output_crlf: bool) -> Vec<u8> {
    struct OldSerialSink(Vec<u8>);
    impl ByteSink for OldSerialSink {
        fn write(&mut self, bytes: &[u8]) {
            for &byte in bytes {
                if byte == b'\n' {
                    self.0.push(b'\r');
                }
                self.0.push(byte);
            }
        }
    }
    let mut termios = Termios::sane();
    termios.output_crlf = output_crlf;
    let line = LineDiscipline::<16, 8, 4>::new(termios);
    let mut sink = OldSerialSink(Vec::new());
    line.write_output(bytes, &mut sink);
    sink.0
}

fn assert_guarded_records(expected_records: usize) {
    let capture = CAPTURE.lock().unwrap();
    assert_eq!(
        capture.next_guard, expected_records,
        "TTY record split across serial critical sections"
    );
    let mut active = None;
    let mut releases = 0;
    for event in &capture.events {
        match *event {
            Event::Acquire(id) => {
                assert_eq!(active, None, "overlapping serial ownership");
                active = Some(id);
            }
            Event::Byte(id, _) => assert_eq!(active, Some(id)),
            Event::Release(id) => {
                assert_eq!(active, Some(id));
                active = None;
                releases += 1;
            }
        }
    }
    assert_eq!(active, None);
    assert_eq!(releases, expected_records);
}

const RECORD: &[u8] = b"MAKOS_MUSL_EL0_ENTRY_OK loader=musl threads=3 singleton=0x2,0x4,0x8 tids=5,6,7 pthread_create=0x280afcd4 rx=0x80000000 resume=0x80001000 calls=96 statuses=42,42,42 block=sleep-until\n";

#[test]
fn tty_record_has_one_serial_critical_section() {
    reset();
    assert!(RECORD.len() <= 512);
    write_tty_bytes(RECORD, true);
    assert_eq!(CAPTURE.lock().unwrap().bytes, old_tty_bytes(RECORD, true));
    assert_guarded_records(1);
}

#[test]
fn tty_output_preserves_existing_translation_for_all_bytes() {
    let all_bytes: Vec<u8> = (0..=255).collect();
    let mut bounded_record = vec![b'x'; 511];
    bounded_record.push(b'\n');
    let multiline = b"first\nsecond\r\n\nlast\r".repeat(300);
    for output_crlf in [false, true] {
        for bytes in [
            &b""[..],
            &b"\n"[..],
            &b"\r\n"[..],
            &b"ordinary text without newline"[..],
            &all_bytes,
            &bounded_record,
            &multiline,
        ] {
            reset();
            write_tty_bytes(bytes, output_crlf);
            assert_eq!(
                CAPTURE.lock().unwrap().bytes,
                old_tty_bytes(bytes, output_crlf)
            );
            assert_guarded_records(1);
        }
    }
}

#[test]
fn raw_serial_write_preserves_bytes() {
    reset();
    write_bytes(b"one\r\ntwo\n\0\xff");
    assert_eq!(CAPTURE.lock().unwrap().bytes, b"one\r\r\ntwo\r\n\0\xff");
    assert_guarded_records(1);
}

#[test]
fn kernel_formatted_record_uses_same_guard() {
    reset();
    print(format_args!("kernel cpu={} tid={} status={}\n", 1, 5, 42));
    assert_eq!(
        CAPTURE.lock().unwrap().bytes,
        b"kernel cpu=1 tid=5 status=42\r\n"
    );
    assert_guarded_records(1);
}

#[test]
fn fatal_detail_precedes_stop_marker_under_one_guard() {
    reset();
    fatal("synthetic host-test reason");
    let bytes = CAPTURE.lock().unwrap().bytes.clone();
    let marker = b"MAKOS_FATAL:";
    let detail = b"MAKOS_FAILURE_DETAIL: synthetic host-test reason\r\n";
    // Simulate every possible two-chunk boundary, including immediately
    // after the colon. Detection is still an immediate prefix predicate.
    for split in 0..=bytes.len() {
        let mut captured = bytes[..split].to_vec();
        if !captured.windows(marker.len()).any(|part| part == marker) {
            captured.extend_from_slice(&bytes[split..]);
        }
        let stop = captured.windows(marker.len()).position(|part| part == marker).unwrap();
        assert!(
            captured[..stop].ends_with(detail),
            "fatal reason missing before immediate stop marker at split {split}"
        );
    }
    assert_eq!(
        bytes,
        b"MAKOS_FAILURE_DETAIL: synthetic host-test reason\r\nMAKOS_FATAL: synthetic host-test reason\r\n"
    );
    assert_guarded_records(1);
}

#[test]
fn concurrent_fatal_records_keep_their_own_detail() {
    reset();
    GATE.lock().unwrap().pause_at_first_byte = true;
    let writer = std::thread::spawn(|| fatal("first synthetic reason"));
    {
        let mut gate = GATE.lock().unwrap();
        while !gate.first_byte_seen {
            gate = GATE_CHANGED.wait(gate).unwrap();
        }
    }
    let contender = std::thread::spawn(|| fatal("second synthetic reason"));
    writer.join().unwrap();
    contender.join().unwrap();
    assert_eq!(
        CAPTURE.lock().unwrap().bytes,
        b"MAKOS_FAILURE_DETAIL: first synthetic reason\r\nMAKOS_FATAL: first synthetic reason\r\nMAKOS_FAILURE_DETAIL: second synthetic reason\r\nMAKOS_FATAL: second synthetic reason\r\n"
    );
    assert_guarded_records(2);
    assert_eq!(GATE.lock().unwrap().acquire_attempts, 2);
}

#[test]
fn concurrent_kernel_trace_cannot_split_tty_record() {
    for output_crlf in [false, true] {
        reset();
        GATE.lock().unwrap().pause_at_first_byte = true;
        let writer = std::thread::spawn(move || write_tty_bytes(RECORD, output_crlf));
        {
            let mut gate = GATE.lock().unwrap();
            while !gate.first_byte_seen {
                gate = GATE_CHANGED.wait(gate).unwrap();
            }
        }
        let contender = std::thread::spawn(|| {
            print(format_args!(
                "MAKOS_AARCH64_THREAD_EXIT_OK tid={} status={}\n",
                7, 0
            ));
        });
        writer.join().unwrap();
        contender.join().unwrap();
        let mut expected = old_tty_bytes(RECORD, output_crlf);
        expected.extend_from_slice(b"MAKOS_AARCH64_THREAD_EXIT_OK tid=7 status=0\r\n");
        assert_eq!(
            CAPTURE.lock().unwrap().bytes,
            expected,
            "kernel trace spliced into TTY record"
        );
        assert_guarded_records(2);
        assert_eq!(GATE.lock().unwrap().acquire_attempts, 2);
    }
}

#[test]
fn successive_terminal_records_have_distinct_atomic_boundaries() {
    reset();
    for _ in 0..3 {
        write_tty_bytes(RECORD, true);
    }
    assert_eq!(
        CAPTURE.lock().unwrap().bytes,
        old_tty_bytes(RECORD, true).repeat(3)
    );
    assert_guarded_records(3);
}
