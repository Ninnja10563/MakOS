use crate::arch::{inb, outb};
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering};

const DATA: u16 = 0x60;
const STATUS: u16 = 0x64;
const COMMAND: u16 = 0x64;
static MOUSE_PHASE: AtomicU8 = AtomicU8::new(0);
static MOUSE_FIRST: AtomicU8 = AtomicU8::new(0);
static MOUSE_SECOND: AtomicU8 = AtomicU8::new(0);
static MOUSE_BUTTONS: AtomicU8 = AtomicU8::new(0);
static MOUSE_X: AtomicI32 = AtomicI32::new(640);
static MOUSE_Y: AtomicI32 = AtomicI32::new(400);
static MOUSE_PENDING: AtomicBool = AtomicBool::new(false);
static KEY_RELEASE: AtomicBool = AtomicBool::new(false);
static KEY_EXTENDED: AtomicBool = AtomicBool::new(false);
static LEFT_SHIFT: AtomicBool = AtomicBool::new(false);
static RIGHT_SHIFT: AtomicBool = AtomicBool::new(false);
static CAPS_LOCK: AtomicBool = AtomicBool::new(false);
const MOUSE_EVENT_COUNT: usize = 16;
static mut MOUSE_EVENTS: [MouseEvent; MOUSE_EVENT_COUNT] = [MouseEvent::EMPTY; MOUSE_EVENT_COUNT];
static MOUSE_EVENT_HEAD: AtomicU8 = AtomicU8::new(0);
static MOUSE_EVENT_TAIL: AtomicU8 = AtomicU8::new(0);
const KEY_QUEUE_BYTES: usize = 64;
static mut KEY_QUEUE: [u8; KEY_QUEUE_BYTES] = [0; KEY_QUEUE_BYTES];
static KEY_HEAD: AtomicU8 = AtomicU8::new(0);
static KEY_TAIL: AtomicU8 = AtomicU8::new(0);

#[derive(Clone, Copy)]
struct MouseEvent {
    x: i32,
    y: i32,
    buttons: u8,
}

impl MouseEvent {
    const EMPTY: Self = Self {
        x: 0,
        y: 0,
        buttons: 0,
    };
}

pub fn init() {
    while unsafe { inb(STATUS) } & 1 != 0 {
        let _ = unsafe { inb(DATA) };
    }
    command(0xae);
    command(0xa8);
    command(0x20);
    let mut config = read_data();
    config &= !(1 << 6); // disable controller scancode translation
    config |= 0x03; // keyboard IRQ1 and mouse IRQ12
    command(0x60);
    controller_data(config);
    keyboard_data(0xf0);
    keyboard_data(2); // scancode set 2
    keyboard_data(0xf4); // enable scanning
    mouse_data(0xf6); // defaults
    mouse_data(0xf4); // enable reporting
    crate::arch::enable_legacy_input_irqs();
    crate::serial_println!(
        "input ps2_keyboard=set2 ps2_mouse=3byte irq=1,12 coalesce=latest edge_queue={} accel=adaptive key_queue={} polling=0",
        MOUSE_EVENT_COUNT,
        KEY_QUEUE_BYTES,
    );
}

pub fn read_key() -> Option<u8> {
    if let Some(event) = pop_mouse_event() {
        crate::graphics::mouse_packet(event.x as u32, event.y as u32, event.buttons);
    } else if MOUSE_PENDING.swap(false, Ordering::AcqRel) {
        crate::graphics::mouse_packet(
            MOUSE_X.load(Ordering::Acquire) as u32,
            MOUSE_Y.load(Ordering::Acquire) as u32,
            MOUSE_BUTTONS.load(Ordering::Acquire),
        );
    }
    pop_key()
}

pub fn interrupt() {
    for _ in 0..64 {
        let status = unsafe { inb(STATUS) };
        if status & 1 == 0 {
            return;
        }
        let code = unsafe { inb(DATA) };
        if status & (1 << 5) != 0 {
            mouse_byte(code);
            continue;
        }
        if code == 0xe0 {
            KEY_EXTENDED.store(true, Ordering::Release);
            continue;
        }
        if code == 0xf0 {
            KEY_RELEASE.store(true, Ordering::Release);
            continue;
        }
        let released = KEY_RELEASE.swap(false, Ordering::AcqRel);
        let extended = KEY_EXTENDED.swap(false, Ordering::AcqRel);
        if !extended && matches!(code, 0x12 | 0x59) {
            let pressed = !released;
            if code == 0x12 {
                LEFT_SHIFT.store(pressed, Ordering::Release);
            } else {
                RIGHT_SHIFT.store(pressed, Ordering::Release);
            }
            continue;
        }
        if released {
            continue;
        }
        if !extended && code == 0x58 {
            CAPS_LOCK.fetch_xor(true, Ordering::AcqRel);
            continue;
        }
        let shift = LEFT_SHIFT.load(Ordering::Acquire) || RIGHT_SHIFT.load(Ordering::Acquire);
        if let Some(key) = scancode_set2(code, extended, shift, CAPS_LOCK.load(Ordering::Acquire)) {
            push_key(key);
        }
    }
}

fn mouse_byte(byte: u8) {
    match MOUSE_PHASE.load(Ordering::Relaxed) {
        0 if byte & 0x08 != 0 => {
            MOUSE_FIRST.store(byte, Ordering::Relaxed);
            MOUSE_PHASE.store(1, Ordering::Relaxed);
        }
        1 => {
            MOUSE_SECOND.store(byte, Ordering::Relaxed);
            MOUSE_PHASE.store(2, Ordering::Relaxed);
        }
        2 => {
            let first = MOUSE_FIRST.load(Ordering::Relaxed);
            let second = MOUSE_SECOND.load(Ordering::Relaxed);
            MOUSE_PHASE.store(0, Ordering::Relaxed);
            if first & 0xc0 != 0 {
                return;
            }
            let dx = accelerate(i32::from(second as i8));
            let dy = accelerate(-i32::from(byte as i8));
            let buttons = first & 7;
            let changed = buttons != MOUSE_BUTTONS.swap(buttons, Ordering::Release);
            if dx != 0 || dy != 0 || changed {
                let x = (MOUSE_X.load(Ordering::Relaxed) + dx).clamp(0, 1279);
                let y = (MOUSE_Y.load(Ordering::Relaxed) + dy).clamp(0, 799);
                MOUSE_X.store(x, Ordering::Relaxed);
                MOUSE_Y.store(y, Ordering::Relaxed);
                if changed {
                    push_mouse_event(MouseEvent { x, y, buttons });
                }
                MOUSE_PENDING.store(true, Ordering::Release);
            }
        }
        _ => MOUSE_PHASE.store(0, Ordering::Relaxed),
    }
}

fn accelerate(delta: i32) -> i32 {
    let magnitude = delta.abs();
    if magnitude >= 6 {
        delta.saturating_mul(3)
    } else if magnitude >= 2 {
        delta.saturating_mul(2)
    } else {
        delta
    }
}

fn push_mouse_event(event: MouseEvent) {
    let head = usize::from(MOUSE_EVENT_HEAD.load(Ordering::Relaxed));
    let next = (head + 1) % MOUSE_EVENT_COUNT;
    if next == usize::from(MOUSE_EVENT_TAIL.load(Ordering::Acquire)) {
        return;
    }
    unsafe {
        (&raw mut MOUSE_EVENTS)
            .cast::<MouseEvent>()
            .add(head)
            .write(event)
    };
    MOUSE_EVENT_HEAD.store(next as u8, Ordering::Release);
}

fn pop_mouse_event() -> Option<MouseEvent> {
    let tail = usize::from(MOUSE_EVENT_TAIL.load(Ordering::Relaxed));
    if tail == usize::from(MOUSE_EVENT_HEAD.load(Ordering::Acquire)) {
        return None;
    }
    let event = unsafe {
        (&raw const MOUSE_EVENTS)
            .cast::<MouseEvent>()
            .add(tail)
            .read()
    };
    MOUSE_EVENT_TAIL.store(((tail + 1) % MOUSE_EVENT_COUNT) as u8, Ordering::Release);
    Some(event)
}

fn push_key(key: u8) {
    let head = usize::from(KEY_HEAD.load(Ordering::Relaxed));
    let next = (head + 1) % KEY_QUEUE_BYTES;
    if next == usize::from(KEY_TAIL.load(Ordering::Acquire)) {
        return;
    }
    unsafe { (&raw mut KEY_QUEUE).cast::<u8>().add(head).write(key) };
    KEY_HEAD.store(next as u8, Ordering::Release);
}

fn pop_key() -> Option<u8> {
    let tail = usize::from(KEY_TAIL.load(Ordering::Relaxed));
    if tail == usize::from(KEY_HEAD.load(Ordering::Acquire)) {
        return None;
    }
    let key = unsafe { (&raw const KEY_QUEUE).cast::<u8>().add(tail).read() };
    KEY_TAIL.store(((tail + 1) % KEY_QUEUE_BYTES) as u8, Ordering::Release);
    Some(key)
}

fn scancode_set2(code: u8, extended: bool, shift: bool, caps_lock: bool) -> Option<u8> {
    if extended {
        return Some(match code {
            0x75 => 0x13, // up/history previous
            0x72 => 0x14, // down/history next
            0x6b => 0x11, // left
            0x74 => 0x12, // right
            _ => return None,
        });
    }
    let mut byte = match code {
        0x0e => {
            if shift {
                b'~'
            } else {
                b'`'
            }
        }
        0x1c => b'a',
        0x32 => b'b',
        0x21 => b'c',
        0x23 => b'd',
        0x24 => b'e',
        0x2b => b'f',
        0x34 => b'g',
        0x33 => b'h',
        0x43 => b'i',
        0x3b => b'j',
        0x42 => b'k',
        0x4b => b'l',
        0x3a => b'm',
        0x31 => b'n',
        0x44 => b'o',
        0x4d => b'p',
        0x15 => b'q',
        0x2d => b'r',
        0x1b => b's',
        0x2c => b't',
        0x3c => b'u',
        0x2a => b'v',
        0x1d => b'w',
        0x22 => b'x',
        0x35 => b'y',
        0x1a => b'z',
        0x29 => b' ',
        0x0d => b'\t',
        0x5a => b'\n',
        0x66 => 8,
        0x45 => {
            if shift {
                b')'
            } else {
                b'0'
            }
        }
        0x16 => {
            if shift {
                b'!'
            } else {
                b'1'
            }
        }
        0x1e => {
            if shift {
                b'@'
            } else {
                b'2'
            }
        }
        0x26 => {
            if shift {
                b'#'
            } else {
                b'3'
            }
        }
        0x25 => {
            if shift {
                b'$'
            } else {
                b'4'
            }
        }
        0x2e => {
            if shift {
                b'%'
            } else {
                b'5'
            }
        }
        0x36 => {
            if shift {
                b'^'
            } else {
                b'6'
            }
        }
        0x3d => {
            if shift {
                b'&'
            } else {
                b'7'
            }
        }
        0x3e => {
            if shift {
                b'*'
            } else {
                b'8'
            }
        }
        0x46 => {
            if shift {
                b'('
            } else {
                b'9'
            }
        }
        0x4e => {
            if shift {
                b'_'
            } else {
                b'-'
            }
        }
        0x55 => {
            if shift {
                b'+'
            } else {
                b'='
            }
        }
        0x54 => {
            if shift {
                b'{'
            } else {
                b'['
            }
        }
        0x5b => {
            if shift {
                b'}'
            } else {
                b']'
            }
        }
        0x5d => {
            if shift {
                b'|'
            } else {
                b'\\'
            }
        }
        0x4c => {
            if shift {
                b':'
            } else {
                b';'
            }
        }
        0x52 => {
            if shift {
                b'\"'
            } else {
                b'\''
            }
        }
        0x41 => {
            if shift {
                b'<'
            } else {
                b','
            }
        }
        0x49 => {
            if shift {
                b'>'
            } else {
                b'.'
            }
        }
        0x4a => {
            if shift {
                b'?'
            } else {
                b'/'
            }
        }
        _ => return None,
    };
    if byte.is_ascii_lowercase() && (shift ^ caps_lock) {
        byte = byte.to_ascii_uppercase();
    }
    Some(byte)
}

fn command(value: u8) {
    wait_write();
    unsafe { outb(COMMAND, value) };
}

fn controller_data(value: u8) {
    wait_write();
    unsafe { outb(DATA, value) };
}

fn keyboard_data(value: u8) {
    controller_data(value);
    let _ = read_data();
}

fn mouse_data(value: u8) {
    command(0xd4);
    controller_data(value);
    let _ = read_data();
}

fn read_data() -> u8 {
    while unsafe { inb(STATUS) } & 1 == 0 {
        core::hint::spin_loop();
    }
    unsafe { inb(DATA) }
}

fn wait_write() {
    while unsafe { inb(STATUS) } & 2 != 0 {
        core::hint::spin_loop();
    }
}
