use crate::arch::outl;
use crate::drivers::pci;
use core::arch::asm;
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

const VENDOR_INTEL: u16 = 0x8086;
const DEVICE_PIIX3_UHCI: u16 = 0x7020;
const COMMAND: u16 = 0x00;
const STATUS: u16 = 0x02;
const INTERRUPT_ENABLE: u16 = 0x04;
const FRAME_NUMBER: u16 = 0x06;
const FRAME_LIST_BASE: u16 = 0x08;
const PORT1: u16 = 0x10;
const PORT2: u16 = 0x12;
const LINK_TERMINATE: u32 = 1;
const LINK_QH: u32 = 2;
const LINK_DEPTH_FIRST: u32 = 4;
const TD_ACTIVE: u32 = 1 << 23;
const TD_NAK: u32 = 1 << 19;
const PID_OUT: u8 = 0xe1;
const PID_IN: u8 = 0x69;
const PID_SETUP: u8 = 0x2d;

#[repr(C, align(4096))]
struct FrameList([u32; 1024]);

#[repr(C, align(16))]
struct QueueHead {
    horizontal: u32,
    element: u32,
}

#[derive(Clone, Copy)]
#[repr(C, align(16))]
struct TransferDescriptor {
    link: u32,
    status: u32,
    token: u32,
    buffer: u32,
}

impl TransferDescriptor {
    const EMPTY: Self = Self {
        link: LINK_TERMINATE,
        status: 0,
        token: 0,
        buffer: 0,
    };
}

static mut FRAMES: FrameList = FrameList([LINK_TERMINATE; 1024]);
static mut QUEUE: QueueHead = QueueHead {
    horizontal: LINK_TERMINATE,
    element: LINK_TERMINATE,
};
static mut DESCRIPTORS: [TransferDescriptor; 10] = [TransferDescriptor::EMPTY; 10];
static mut SETUP: [u8; 8] = [0; 8];
static mut DATA: [u8; 64] = [0; 64];
static HID_READY: AtomicBool = AtomicBool::new(false);
static HID_ENDPOINT: AtomicU8 = AtomicU8::new(0);
static HID_TOGGLE: AtomicBool = AtomicBool::new(false);
static HID_CAPS_LOCK: AtomicBool = AtomicBool::new(false);
static mut HID_PREVIOUS: [u8; 6] = [0; 6];

pub fn self_test() {
    let device = pci::find(VENDOR_INTEL, DEVICE_PIIX3_UHCI)
        .unwrap_or_else(|| crate::fatal("UHCI PCI controller absent"));
    device.enable_io_bus_master();
    let bar4 = device.read(0x20);
    if bar4 & 1 == 0 {
        crate::fatal("UHCI BAR4 is not I/O space");
    }
    let io = (bar4 & 0xffe0) as u16;
    reset_controller(io);
    let port = if reset_port(io + PORT1) {
        io + PORT1
    } else if reset_port(io + PORT2) {
        io + PORT2
    } else {
        crate::fatal("UHCI connected USB device absent");
    };
    start_schedule(io);

    let device_descriptor = control_in(io, 0, 0x80, 6, 0x0100, 0, 18)
        .unwrap_or_else(|| crate::fatal("USB device descriptor transfer failed"));
    if device_descriptor.len() != 18
        || device_descriptor[0] != 18
        || device_descriptor[1] != 1
        || device_descriptor[7] != 8
    {
        crate::fatal("USB invalid device descriptor");
    }
    if !control_no_data(io, 0, 0x00, 5, 1, 0) {
        crate::fatal("USB SET_ADDRESS failed");
    }
    delay();
    let config = control_in(io, 1, 0x80, 6, 0x0200, 0, 34)
        .unwrap_or_else(|| crate::fatal("USB configuration descriptor transfer failed"));
    let endpoint = hid_keyboard_endpoint(config)
        .unwrap_or_else(|| crate::fatal("USB HID keyboard interface absent"));
    if endpoint & 0x80 == 0 {
        crate::fatal("USB HID keyboard interface absent");
    }
    if !control_no_data(io, 1, 0x00, 9, 1, 0) {
        crate::fatal("USB SET_CONFIGURATION failed");
    }
    if !control_no_data(io, 1, 0x21, 11, 0, 0) {
        crate::fatal("USB HID SET_PROTOCOL failed");
    }
    HID_ENDPOINT.store(endpoint & 0x0f, Ordering::Release);
    HID_TOGGLE.store(false, Ordering::Release);
    HID_READY.store(true, Ordering::Release);
    crate::serial_println!(
        "MAKOS_USB_OK controller=uhci device=keyboard control_transfer=1 descriptor=1 hid=1 pci={:02x}:{:02x}.{} io={:#x} port={}",
        device.bus,
        device.slot,
        device.function,
        io,
        if port == io + PORT1 { 1 } else { 2 }
    );
}

pub fn read_key() -> Option<u8> {
    if !HID_READY.load(Ordering::Acquire) {
        return None;
    }
    let endpoint = HID_ENDPOINT.load(Ordering::Acquire);
    let toggle = HID_TOGGLE.load(Ordering::Acquire);
    unsafe {
        ptr::write_bytes((&raw mut DATA).cast::<u8>(), 0, 8);
        setup_td(
            0,
            PID_IN,
            1,
            endpoint,
            toggle,
            8,
            (&raw mut DATA).cast::<u8>() as u32,
        );
        if !run_interrupt_in() {
            return None;
        }
        HID_TOGGLE.store(!toggle, Ordering::Release);
        let report = &*(&raw const DATA);
        let previous = &mut *(&raw mut HID_PREVIOUS);
        let mut key = None;
        let shift = report[0] & 0x22 != 0;
        for code in &report[2..8] {
            if *code != 0 && !previous.contains(code) && key.is_none() {
                if *code == 0x39 {
                    HID_CAPS_LOCK.fetch_xor(true, Ordering::AcqRel);
                } else {
                    key = hid_keycode(*code, shift, HID_CAPS_LOCK.load(Ordering::Acquire));
                }
            }
        }
        previous.copy_from_slice(&report[2..8]);
        key
    }
}

fn reset_controller(io: u16) {
    unsafe { outw(io + COMMAND, 1 << 2) };
    delay();
    unsafe { outw(io + COMMAND, 0) };
    unsafe { outw(io + COMMAND, 1 << 1) };
    for _ in 0..1_000_000 {
        if unsafe { inw(io + COMMAND) } & (1 << 1) == 0 {
            unsafe {
                outw(io + STATUS, 0xffff);
                outw(io + INTERRUPT_ENABLE, 0);
                outw(io + FRAME_NUMBER, 0);
            }
            return;
        }
        core::hint::spin_loop();
    }
    crate::fatal("UHCI host reset timeout")
}

fn reset_port(port: u16) -> bool {
    let initial = unsafe { inw(port) };
    if initial & 1 == 0 {
        return false;
    }
    unsafe { outw(port, (initial & 0x1ff5) | (1 << 9)) };
    delay();
    unsafe { outw(port, initial & 0x1ff5) };
    delay();
    for _ in 0..10 {
        let value = unsafe { inw(port) };
        unsafe { outw(port, (value & 0x1ff5) | (1 << 2)) };
        delay();
        let enabled = unsafe { inw(port) };
        if enabled & 5 == 5 {
            return true;
        }
    }
    false
}

fn start_schedule(io: u16) {
    let queue = (&raw mut QUEUE) as u32;
    unsafe {
        let frames = &raw mut FRAMES;
        for frame in (*frames).0.iter_mut() {
            *frame = queue | LINK_QH;
        }
        outl(io + FRAME_LIST_BASE, frames as u32);
        outw(io + FRAME_NUMBER, 0);
        outw(io + COMMAND, (1 << 7) | 1);
    }
}

fn control_in(
    io: u16,
    address: u8,
    request_type: u8,
    request: u8,
    value: u16,
    index: u16,
    length: usize,
) -> Option<&'static [u8]> {
    if length > 64 {
        return None;
    }
    unsafe {
        ptr::write_bytes((&raw mut DATA).cast::<u8>(), 0, 64);
        write_setup(request_type, request, value, index, length as u16);
        let mut count = 1usize;
        setup_td(0, PID_SETUP, address, 0, false, 8, (&raw mut SETUP) as u32);
        let mut offset = 0usize;
        let mut toggle = true;
        while offset < length {
            let chunk = (length - offset).min(8);
            setup_td(
                count,
                PID_IN,
                address,
                0,
                toggle,
                chunk,
                (&raw mut DATA).cast::<u8>().add(offset) as u32,
            );
            offset += chunk;
            toggle = !toggle;
            count += 1;
        }
        setup_td(count, PID_OUT, address, 0, true, 0, 0);
        count += 1;
        if !run_chain(io, count) {
            return None;
        }
        let data = &*(&raw const DATA);
        Some(&data[..length])
    }
}

fn control_no_data(
    io: u16,
    address: u8,
    request_type: u8,
    request: u8,
    value: u16,
    index: u16,
) -> bool {
    unsafe {
        write_setup(request_type, request, value, index, 0);
        setup_td(0, PID_SETUP, address, 0, false, 8, (&raw mut SETUP) as u32);
        setup_td(1, PID_IN, address, 0, true, 0, 0);
        run_chain(io, 2)
    }
}

unsafe fn write_setup(kind: u8, request: u8, value: u16, index: u16, length: u16) {
    let setup = unsafe { &mut *(&raw mut SETUP) };
    setup[0] = kind;
    setup[1] = request;
    setup[2..4].copy_from_slice(&value.to_le_bytes());
    setup[4..6].copy_from_slice(&index.to_le_bytes());
    setup[6..8].copy_from_slice(&length.to_le_bytes());
}

unsafe fn setup_td(
    slot: usize,
    pid: u8,
    address: u8,
    endpoint: u8,
    toggle: bool,
    length: usize,
    buffer: u32,
) {
    let max_length = if length == 0 {
        0x7ff
    } else {
        length as u32 - 1
    };
    let td = unsafe {
        &mut *(&raw mut DESCRIPTORS)
            .cast::<TransferDescriptor>()
            .add(slot)
    };
    *td = TransferDescriptor {
        link: LINK_TERMINATE,
        status: TD_ACTIVE | (3 << 27),
        token: u32::from(pid)
            | (u32::from(address) << 8)
            | (u32::from(endpoint) << 15)
            | (u32::from(toggle) << 19)
            | (max_length << 21),
        buffer,
    };
}

unsafe fn run_chain(io: u16, count: usize) -> bool {
    let descriptors = (&raw mut DESCRIPTORS).cast::<TransferDescriptor>();
    for slot in 0..count - 1 {
        unsafe {
            (*descriptors.add(slot)).link = descriptors.add(slot + 1) as u32 | LINK_DEPTH_FIRST
        };
    }
    unsafe { (*descriptors.add(count - 1)).link = LINK_TERMINATE };
    unsafe { (*(&raw mut QUEUE)).element = descriptors as u32 };
    for _ in 0..20_000_000 {
        let last = unsafe { (*descriptors.add(count - 1)).status };
        if last & TD_ACTIVE == 0 {
            unsafe { (*(&raw mut QUEUE)).element = LINK_TERMINATE };
            for slot in 0..count {
                let status = unsafe { (*descriptors.add(slot)).status };
                if status & 0x007f_0000 != 0 || status & TD_ACTIVE != 0 {
                    return false;
                }
            }
            return unsafe { inw(io + STATUS) } & 0x10 == 0;
        }
        core::hint::spin_loop();
    }
    unsafe { (*(&raw mut QUEUE)).element = LINK_TERMINATE };
    false
}

unsafe fn run_interrupt_in() -> bool {
    let descriptor = (&raw mut DESCRIPTORS).cast::<TransferDescriptor>();
    unsafe { (*descriptor).link = LINK_TERMINATE };
    unsafe { (*(&raw mut QUEUE)).element = descriptor as u32 };
    for _ in 0..250_000 {
        let status = unsafe { (*descriptor).status };
        if status & TD_NAK != 0 {
            unsafe { (*(&raw mut QUEUE)).element = LINK_TERMINATE };
            return false;
        }
        if status & TD_ACTIVE == 0 {
            unsafe { (*(&raw mut QUEUE)).element = LINK_TERMINATE };
            return status & 0x007f_0000 == 0 && status & 0x7ff == 7;
        }
        core::hint::spin_loop();
    }
    unsafe { (*(&raw mut QUEUE)).element = LINK_TERMINATE };
    false
}

fn hid_keyboard_endpoint(bytes: &[u8]) -> Option<u8> {
    let mut offset = 0usize;
    let mut keyboard_interface = false;
    while offset + 2 <= bytes.len() {
        let length = bytes[offset] as usize;
        if length < 2 || offset + length > bytes.len() {
            return None;
        }
        if bytes[offset + 1] == 4
            && length >= 9
            && bytes[offset + 5] == 3
            && bytes[offset + 6] == 1
            && bytes[offset + 7] == 1
        {
            keyboard_interface = true;
        } else if bytes[offset + 1] == 4 {
            keyboard_interface = false;
        } else if keyboard_interface && bytes[offset + 1] == 5 && length >= 7 {
            return Some(bytes[offset + 2]);
        }
        offset += length;
    }
    None
}

fn hid_keycode(code: u8, shift: bool, caps_lock: bool) -> Option<u8> {
    let mut byte = match code {
        0x04..=0x1d => b'a' + (code - 0x04),
        0x1e..=0x26 => (if shift { b"!@#$%^&*(" } else { b"123456789" })[(code - 0x1e) as usize],
        0x27 => {
            if shift {
                b')'
            } else {
                b'0'
            }
        }
        0x28 => b'\n',
        0x2a => 8,
        0x2b => b'\t',
        0x2c => b' ',
        0x2d => {
            if shift {
                b'_'
            } else {
                b'-'
            }
        }
        0x2e => {
            if shift {
                b'+'
            } else {
                b'='
            }
        }
        0x2f => {
            if shift {
                b'{'
            } else {
                b'['
            }
        }
        0x30 => {
            if shift {
                b'}'
            } else {
                b']'
            }
        }
        0x31 => {
            if shift {
                b'|'
            } else {
                b'\\'
            }
        }
        0x33 => {
            if shift {
                b':'
            } else {
                b';'
            }
        }
        0x34 => {
            if shift {
                b'\"'
            } else {
                b'\''
            }
        }
        0x35 => {
            if shift {
                b'~'
            } else {
                b'`'
            }
        }
        0x36 => {
            if shift {
                b'<'
            } else {
                b','
            }
        }
        0x37 => {
            if shift {
                b'>'
            } else {
                b'.'
            }
        }
        0x38 => {
            if shift {
                b'?'
            } else {
                b'/'
            }
        }
        0x4f => 0x12,
        0x50 => 0x11,
        0x51 => 0x14,
        0x52 => 0x13,
        _ => return None,
    };
    if byte.is_ascii_lowercase() && (shift ^ caps_lock) {
        byte = byte.to_ascii_uppercase();
    }
    Some(byte)
}

fn delay() {
    for _ in 0..2_000_000 {
        core::hint::spin_loop();
    }
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    unsafe {
        asm!("in ax, dx", out("ax") value, in("dx") port, options(nomem, nostack, preserves_flags))
    };
    value
}

#[inline]
unsafe fn outw(port: u16, value: u16) {
    unsafe {
        asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags))
    };
}
