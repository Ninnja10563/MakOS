use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU32, Ordering, compiler_fence};

const MMIO_BASE: u64 = 0x0a00_0000;
const MMIO_STRIDE: u64 = 0x200;
const MMIO_SLOTS: usize = 32;
// QEMU virt connects virtio-mmio slot n to SPI 16+n. GIC INTIDs add the
// architectural SPI base (32), so slot n is INTID 48+n.
const MMIO_GIC_INTID_BASE: u32 = 48;
const MAGIC: u32 = 0x7472_6976;
const DEVICE_INPUT: u32 = 18;
const VERSION_MODERN: u32 = 2;
const QUEUE_SIZE: u16 = 32;
const MAX_DEVICES: usize = 4;
const DESC_OFFSET: u64 = 0;
const AVAIL_OFFSET: u64 = 512;
const USED_OFFSET: u64 = 1024;
const BUFFER_OFFSET: u64 = 1536;
const EVENT_BYTES: u32 = 8;

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
const REG_QUEUE_DESC_LOW: u64 = 0x080;
const REG_QUEUE_DRIVER_LOW: u64 = 0x090;
const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;
const REG_CONFIG: u64 = 0x100;

const STATUS_ACKNOWLEDGE: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;
const STATUS_FAILED: u32 = 128;
const VIRTIO_F_VERSION_1: u32 = 1;

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const EV_REL: u16 = 2;
const EV_ABS: u16 = 3;
const SYN_REPORT: u16 = 0;
const REL_X: u16 = 0;
const REL_Y: u16 = 1;
const REL_HWHEEL: u16 = 6;
const REL_WHEEL: u16 = 8;
const ABS_X: u16 = 0;
const ABS_Y: u16 = 1;
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_RIGHTSHIFT: u16 = 54;
const KEY_LEFTCTRL: u16 = 29;
const KEY_RIGHTCTRL: u16 = 97;
const KEY_LEFTMETA: u16 = 125;
const KEY_RIGHTMETA: u16 = 126;
const KEY_S: u16 = 31;
const KEY_A: u16 = 30;
const KEY_C: u16 = 46;
const KEY_L: u16 = 38;
const KEY_V: u16 = 47;
const KEY_X: u16 = 45;
const KEY_EDITOR_SAVE: u8 = 0x83;
const KEY_SELECT_ALL: u8 = 0x84;
const KEY_COPY: u8 = 0x85;
const KEY_CUT: u8 = 0x86;
const KEY_PASTE: u8 = 0x87;
const KEY_FOCUS_LOCATION: u8 = 0x88;

const KEY_QUEUE_SIZE: usize = 64;
static mut KEY_QUEUE: [u8; KEY_QUEUE_SIZE] = [0; KEY_QUEUE_SIZE];
static KEY_HEAD: AtomicU8 = AtomicU8::new(0);
static KEY_TAIL: AtomicU8 = AtomicU8::new(0);
static SHIFT: AtomicBool = AtomicBool::new(false);
static CONTROL: AtomicBool = AtomicBool::new(false);
static META: AtomicBool = AtomicBool::new(false);
static CAPS_LOCK: AtomicBool = AtomicBool::new(false);
static CURSOR_X: AtomicU32 = AtomicU32::new(400);
static CURSOR_Y: AtomicU32 = AtomicU32::new(300);
static CURSOR_BUTTONS: AtomicU8 = AtomicU8::new(0);
static SCROLL_X: AtomicI32 = AtomicI32::new(0);
static SCROLL_Y: AtomicI32 = AtomicI32::new(0);
static SURFACE_KEY_QUEUED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct InputDevice {
    base: u64,
    interrupt_id: u32,
    queue_frame: u64,
    queue_size: u16,
    last_used: u16,
    avail_index: u16,
    abs_max_x: u32,
    abs_max_y: u32,
}

impl InputDevice {
    const EMPTY: Self = Self {
        base: 0,
        interrupt_id: 0,
        queue_frame: 0,
        queue_size: 0,
        last_used: 0,
        avail_index: 0,
        abs_max_x: 32_767,
        abs_max_y: 32_767,
    };
}

static mut DEVICES: [InputDevice; MAX_DEVICES] = [InputDevice::EMPTY; MAX_DEVICES];
static DEVICE_COUNT: AtomicU8 = AtomicU8::new(0);
static POLLING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct PointerReport {
    x: u32,
    y: u32,
    buttons: u8,
}

impl PointerReport {
    const EMPTY: Self = Self {
        x: 0,
        y: 0,
        buttons: 0,
    };
}

struct PointerBatch {
    edges: [PointerReport; QUEUE_SIZE as usize],
    edge_count: usize,
    motion_dirty: bool,
}

impl PointerBatch {
    const fn new() -> Self {
        Self {
            edges: [PointerReport::EMPTY; QUEUE_SIZE as usize],
            edge_count: 0,
            motion_dirty: false,
        }
    }

    fn record_edge(&mut self) {
        if self.edge_count == self.edges.len() {
            crate::fatal("virtio-input pointer edge batch overflow");
        }
        self.edges[self.edge_count] = PointerReport {
            x: CURSOR_X.load(Ordering::Acquire),
            y: CURSOR_Y.load(Ordering::Acquire),
            buttons: CURSOR_BUTTONS.load(Ordering::Acquire),
        };
        self.edge_count += 1;
        // This edge report also carries all motion observed before the edge.
        self.motion_dirty = false;
    }

    fn dispatch(self) {
        let mut last = None;
        for report in self.edges[..self.edge_count].iter().copied() {
            crate::graphics::mouse_packet(report.x, report.y, report.buttons);
            last = Some((report.x, report.y, report.buttons));
        }
        if self.motion_dirty {
            let report = (
                CURSOR_X.load(Ordering::Acquire),
                CURSOR_Y.load(Ordering::Acquire),
                CURSOR_BUTTONS.load(Ordering::Acquire),
            );
            if last != Some(report) {
                crate::graphics::mouse_packet(report.0, report.1, report.2);
            }
        }
        let scroll_x = SCROLL_X.swap(0, Ordering::AcqRel);
        let scroll_y = SCROLL_Y.swap(0, Ordering::AcqRel);
        if scroll_x != 0 || scroll_y != 0 {
            crate::graphics::mouse_scroll(scroll_x, scroll_y);
        }
    }
}

pub fn init() {
    let mut count = 0usize;
    for slot in 0..MMIO_SLOTS {
        let base = MMIO_BASE + slot as u64 * MMIO_STRIDE;
        if read32(base + REG_MAGIC) != MAGIC
            || read32(base + REG_VERSION) != VERSION_MODERN
            || read32(base + REG_DEVICE_ID) != DEVICE_INPUT
        {
            continue;
        }
        if count == MAX_DEVICES {
            crate::fatal("too many AArch64 virtio-input devices");
        }
        crate::serial_println!("virtio-input probe base={:#x} slot={}", base, slot);
        let device = configure(base, slot);
        unsafe {
            (&raw mut DEVICES)
                .cast::<InputDevice>()
                .add(count)
                .write(device)
        };
        count += 1;
    }
    if count == 0 {
        crate::fatal("AArch64 virtio-input device absent");
    }
    DEVICE_COUNT.store(count as u8, Ordering::Release);
    for index in 0..count {
        let device = unsafe { &*(&raw const DEVICES).cast::<InputDevice>().add(index) };
        // Drop any configuration-time edge before unmasking the SPI. Event
        // descriptors are already published, so every subsequent edge can be
        // drained without racing partial device construction.
        acknowledge_device_interrupt(device);
        crate::arch::enable_virtio_mmio_interrupt(device.interrupt_id);
        crate::serial_println!(
            "MAKOS_AARCH64_INPUT_IRQ_ROUTE_OK intid={} target_cpu=0 trigger=edge-rising transport=virtio-mmio",
            device.interrupt_id,
        );
    }
    crate::serial_println!(
        "MAKOS_AARCH64_INPUT_OK transport=virtio-mmio devices={} eventq={} polling=single-consumer event_drain=1 notify=batched pointer_motion=coalesced pointer_edges=preserved keyboard_syn=ignored absolute_pointer=1 keyboard=1 delivery=gicv2-spi timer_fallback=100hz",
        count,
        QUEUE_SIZE,
    );
}

pub(crate) fn owns_interrupt(interrupt_id: u32) -> bool {
    let count = usize::from(DEVICE_COUNT.load(Ordering::Acquire));
    (0..count).any(|index| {
        let device = unsafe { &*(&raw const DEVICES).cast::<InputDevice>().add(index) };
        device.interrupt_id == interrupt_id
    })
}

/// Acknowledge a device edge without entering graphics/scheduler locks.
///
/// IRQs that interrupt EL1 can arrive while a syscall owns one of those locks.
/// Clearing the transport status here prevents an interrupt storm; CPU0's
/// retained timer fallback drains the used ring on the next safe tick.
pub(crate) fn acknowledge_interrupt(interrupt_id: u32) {
    if crate::arch::cpu_index() != 0 {
        crate::fatal("AArch64 virtio-input IRQ reached non-owner CPU");
    }
    let count = usize::from(DEVICE_COUNT.load(Ordering::Acquire));
    let Some(device) = (0..count).find_map(|index| {
        let device = unsafe { &*(&raw const DEVICES).cast::<InputDevice>().add(index) };
        (device.interrupt_id == interrupt_id).then_some(device)
    }) else {
        crate::fatal("AArch64 virtio-input IRQ has no registered device");
    };
    acknowledge_device_interrupt(device);
}

pub fn poll() -> bool {
    if crate::arch::cpu_index() != 0 {
        crate::fatal("AArch64 virtio-input poll attempted from non-owner CPU");
    }
    if POLLING
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return false;
    }
    crate::graphics::begin_input_batch();
    let count = usize::from(DEVICE_COUNT.load(Ordering::Acquire));
    let mut activity = false;
    for index in 0..count {
        let device = unsafe { &mut *(&raw mut DEVICES).cast::<InputDevice>().add(index) };
        activity |= poll_device(device);
    }
    crate::graphics::end_input_batch();
    POLLING.store(false, Ordering::Release);
    if activity {
        if SURFACE_KEY_QUEUED.swap(false, Ordering::AcqRel) {
            crate::aarch64_process::prioritize_firefox_surface_thread();
        }
        crate::aarch64_process::wake_input_waiters();
    }
    activity
}

pub fn read_key() -> Option<u8> {
    if let Some(byte) = crate::graphics::terminal_read_byte() {
        return Some(byte);
    }
    let tail = usize::from(KEY_TAIL.load(Ordering::Relaxed));
    if tail == usize::from(KEY_HEAD.load(Ordering::Acquire)) {
        return None;
    }
    let byte = unsafe { (&raw const KEY_QUEUE).cast::<u8>().add(tail).read() };
    KEY_TAIL.store(((tail + 1) % KEY_QUEUE_SIZE) as u8, Ordering::Release);
    crate::graphics::terminal_input_byte(byte);
    crate::graphics::terminal_read_byte()
}

pub fn inject_key(byte: u8) {
    push_key(byte);
}

fn configure(base: u64, slot: usize) -> InputDevice {
    crate::serial_println!("virtio-input config base={:#x} stage=reset", base);
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
        fail_device(base, "virtio-input lacks VERSION_1");
    }
    write32(base + REG_DRIVER_FEATURES_SEL, 0);
    write32(base + REG_DRIVER_FEATURES, 0);
    write32(base + REG_DRIVER_FEATURES_SEL, 1);
    write32(base + REG_DRIVER_FEATURES, VIRTIO_F_VERSION_1);
    let features_status = STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK;
    write32(base + REG_STATUS, features_status);
    if read32(base + REG_STATUS) & STATUS_FEATURES_OK == 0 {
        fail_device(base, "virtio-input feature negotiation failed");
    }

    write32(base + REG_QUEUE_SEL, 0);
    let maximum = read32(base + REG_QUEUE_NUM_MAX);
    crate::serial_println!(
        "virtio-input config base={:#x} stage=queue maximum={}",
        base,
        maximum,
    );
    if maximum < u32::from(QUEUE_SIZE) || read32(base + REG_QUEUE_READY) != 0 {
        fail_device(base, "virtio-input event queue unsupported");
    }
    let frame =
        crate::mm::allocate_frame().unwrap_or_else(|| crate::fatal("virtio-input queue frame OOM"));
    unsafe { core::ptr::write_bytes(frame as *mut u8, 0, 4096) };
    for index in 0..usize::from(QUEUE_SIZE) {
        let descriptor = frame + DESC_OFFSET + index as u64 * 16;
        write64_memory(
            descriptor,
            frame + BUFFER_OFFSET + index as u64 * EVENT_BYTES as u64,
        );
        write32_memory(descriptor + 8, EVENT_BYTES);
        write16_memory(descriptor + 12, 2); // VIRTQ_DESC_F_WRITE
        write16_memory(descriptor + 14, 0);
        write16_memory(frame + AVAIL_OFFSET + 4 + index as u64 * 2, index as u16);
    }
    write16_memory(frame + AVAIL_OFFSET + 2, QUEUE_SIZE);
    write32(base + REG_QUEUE_NUM, u32::from(QUEUE_SIZE));
    write_address(base + REG_QUEUE_DESC_LOW, frame + DESC_OFFSET);
    write_address(base + REG_QUEUE_DRIVER_LOW, frame + AVAIL_OFFSET);
    write_address(base + REG_QUEUE_DEVICE_LOW, frame + USED_OFFSET);
    write32(base + REG_QUEUE_READY, 1);
    memory_barrier();
    write32(base + REG_STATUS, features_status | STATUS_DRIVER_OK);
    if read32(base + REG_STATUS) & STATUS_FAILED != 0 {
        fail_device(base, "virtio-input entered FAILED state");
    }
    write32(base + REG_QUEUE_NOTIFY, 0);
    crate::serial_println!("virtio-input config base={:#x} stage=driver-ok", base);

    InputDevice {
        base,
        interrupt_id: MMIO_GIC_INTID_BASE + slot as u32,
        queue_frame: frame,
        queue_size: QUEUE_SIZE,
        last_used: 0,
        avail_index: QUEUE_SIZE,
        abs_max_x: read_abs_max(base, ABS_X).unwrap_or(32_767).max(1),
        abs_max_y: read_abs_max(base, ABS_Y).unwrap_or(32_767).max(1),
    }
}

fn poll_device(device: &mut InputDevice) -> bool {
    let mut requeued = false;
    let mut pointer = PointerBatch::new();
    loop {
        memory_barrier();
        let used_index = read16_memory(device.queue_frame + USED_OFFSET + 2);
        if device.last_used == used_index {
            break;
        }
        let used_slot = u64::from(device.last_used % device.queue_size);
        let id = read32_memory(device.queue_frame + USED_OFFSET + 4 + used_slot * 8);
        if id >= u32::from(device.queue_size) {
            fail_device(device.base, "virtio-input used descriptor out of range");
        }
        let event_address = device.queue_frame + BUFFER_OFFSET + u64::from(id) * EVENT_BYTES as u64;
        let event_type = read16_memory(event_address);
        let code = read16_memory(event_address + 2);
        let value = read32_memory(event_address + 4) as i32;
        // Stage returned descriptors, then publish the new avail index once
        // the used ring is drained. This bounds the edge snapshot to one ring.
        let available_slot = u64::from(device.avail_index % device.queue_size);
        write16_memory(
            device.queue_frame + AVAIL_OFFSET + 4 + available_slot * 2,
            id as u16,
        );
        device.avail_index = device.avail_index.wrapping_add(1);
        device.last_used = device.last_used.wrapping_add(1);
        requeued = true;
        handle_event(device, &mut pointer, event_type, code, value);
    }
    if requeued {
        write16_memory(device.queue_frame + AVAIL_OFFSET + 2, device.avail_index);
        memory_barrier();
        write32(device.base + REG_QUEUE_NOTIFY, 0);
    }
    acknowledge_device_interrupt(device);
    pointer.dispatch();
    requeued
}

fn acknowledge_device_interrupt(device: &InputDevice) {
    let interrupt = read32(device.base + REG_INTERRUPT_STATUS);
    if interrupt != 0 {
        write32(device.base + REG_INTERRUPT_ACK, interrupt);
    }
}

fn handle_event(
    device: &InputDevice,
    pointer: &mut PointerBatch,
    event_type: u16,
    code: u16,
    value: i32,
) {
    match event_type {
        // Reports are emitted after the device ring is drained. In particular,
        // keyboard SYN_REPORT must not generate a redundant pointer redraw.
        EV_SYN if code == SYN_REPORT => {}
        EV_KEY if code == KEY_LEFTSHIFT || code == KEY_RIGHTSHIFT => {
            SHIFT.store(value != 0, Ordering::Release);
        }
        EV_KEY if code == KEY_LEFTCTRL || code == KEY_RIGHTCTRL => {
            CONTROL.store(value != 0, Ordering::Release);
        }
        EV_KEY if code == KEY_LEFTMETA || code == KEY_RIGHTMETA => {
            META.store(value != 0, Ordering::Release);
        }
        EV_KEY
            if code == KEY_S
                && value == 1
                && (CONTROL.load(Ordering::Acquire) || META.load(Ordering::Acquire)) =>
        {
            if !route_surface_key(KEY_EDITOR_SAVE) {
                push_key(KEY_EDITOR_SAVE);
            }
        }
        EV_KEY
            if code == KEY_L
                && value == 1
                && (CONTROL.load(Ordering::Acquire) || META.load(Ordering::Acquire)) =>
        {
            if !route_surface_key(KEY_FOCUS_LOCATION) {
                push_key(KEY_FOCUS_LOCATION);
            }
        }
        EV_KEY
            if value == 1
                && (CONTROL.load(Ordering::Acquire) || META.load(Ordering::Acquire))
                && matches!(code, KEY_A | KEY_C | KEY_X | KEY_V) =>
        {
            let key = match code {
                KEY_A => KEY_SELECT_ALL,
                KEY_C => KEY_COPY,
                KEY_X => KEY_CUT,
                _ => KEY_PASTE,
            };
            if !route_surface_key(key) {
                push_key(key);
            }
        }
        EV_KEY if value == 1 && CONTROL.load(Ordering::Acquire) => {
            // Full-screen terminal apps need every Ctrl-letter, not only the
            // desktop editor/clipboard shortcuts above. Include standard
            // Ctrl-@/[/\\/]/^/_/? punctuation mappings used by shells/editors.
            let byte = key_code(
                code,
                SHIFT.load(Ordering::Acquire),
                CAPS_LOCK.load(Ordering::Acquire),
            );
            let control = byte.and_then(|byte| match byte.to_ascii_uppercase() {
                value @ b'A'..=b'Z' => Some(value & 0x1f),
                b'@' | b' ' => Some(0x00),
                b'[' => Some(0x1b),
                b'\\' => Some(0x1c),
                b']' => Some(0x1d),
                b'^' => Some(0x1e),
                b'_' => Some(0x1f),
                b'?' => Some(0x7f),
                _ => None,
            });
            if let Some(byte) = control {
                push_key(byte);
            }
        }
        EV_KEY if code == 58 && value == 1 => {
            CAPS_LOCK.fetch_xor(true, Ordering::AcqRel);
        }
        EV_KEY if matches!(code, BTN_LEFT | BTN_RIGHT | BTN_MIDDLE) => {
            let bit = match code {
                BTN_LEFT => 1,
                BTN_RIGHT => 2,
                _ => 4,
            };
            let previous = if value != 0 {
                CURSOR_BUTTONS.fetch_or(bit, Ordering::AcqRel)
            } else {
                CURSOR_BUTTONS.fetch_and(!bit, Ordering::AcqRel)
            };
            let current = if value != 0 {
                previous | bit
            } else {
                previous & !bit
            };
            if previous != current {
                pointer.record_edge();
            }
        }
        EV_KEY if value == 1 || value == 2 => {
            if let Some(byte) = key_code(
                code,
                SHIFT.load(Ordering::Acquire),
                CAPS_LOCK.load(Ordering::Acquire),
            ) {
                if !route_surface_key(byte) {
                    push_key(byte);
                }
            }
        }
        EV_ABS if code == ABS_X && value >= 0 => {
            let (width, _) = crate::aarch64_virtio_gpu::dimensions();
            let next = scale(value as u32, device.abs_max_x, width.saturating_sub(1));
            pointer.motion_dirty |= CURSOR_X.swap(next, Ordering::AcqRel) != next;
        }
        EV_ABS if code == ABS_Y && value >= 0 => {
            let (_, height) = crate::aarch64_virtio_gpu::dimensions();
            let next = scale(value as u32, device.abs_max_y, height.saturating_sub(1));
            pointer.motion_dirty |= CURSOR_Y.swap(next, Ordering::AcqRel) != next;
        }
        EV_REL if code == REL_X => {
            let x = CURSOR_X.load(Ordering::Relaxed) as i32;
            let (width, _) = crate::aarch64_virtio_gpu::dimensions();
            let next = (x + value).clamp(0, width.saturating_sub(1) as i32) as u32;
            pointer.motion_dirty |= CURSOR_X.swap(next, Ordering::AcqRel) != next;
        }
        EV_REL if code == REL_Y => {
            let y = CURSOR_Y.load(Ordering::Relaxed) as i32;
            let (_, height) = crate::aarch64_virtio_gpu::dimensions();
            let next = (y + value).clamp(0, height.saturating_sub(1) as i32) as u32;
            pointer.motion_dirty |= CURSOR_Y.swap(next, Ordering::AcqRel) != next;
        }
        EV_REL if code == REL_HWHEEL => {
            SCROLL_X.fetch_add(value, Ordering::AcqRel);
        }
        EV_REL if code == REL_WHEEL => {
            SCROLL_Y.fetch_add(value, Ordering::AcqRel);
        }
        _ => {}
    }
}

fn scale(value: u32, maximum: u32, screen_maximum: u32) -> u32 {
    u64::from(value.min(maximum))
        .saturating_mul(u64::from(screen_maximum))
        .checked_div(u64::from(maximum))
        .unwrap_or(0) as u32
}

fn push_key(byte: u8) {
    let head = usize::from(KEY_HEAD.load(Ordering::Relaxed));
    let next = (head + 1) % KEY_QUEUE_SIZE;
    if next == usize::from(KEY_TAIL.load(Ordering::Acquire)) {
        return;
    }
    unsafe { (&raw mut KEY_QUEUE).cast::<u8>().add(head).write(byte) };
    KEY_HEAD.store(next as u8, Ordering::Release);
}

fn route_surface_key(byte: u8) -> bool {
    let routed = crate::graphics::route_key_event(byte);
    if routed {
        SURFACE_KEY_QUEUED.store(true, Ordering::Release);
    }
    routed
}

fn key_code(code: u16, shift: bool, caps_lock: bool) -> Option<u8> {
    let byte = match code {
        1 => 0x1b, // Escape.
        2..=10 => (if shift { b"!@#$%^&*(" } else { b"123456789" })[(code - 2) as usize],
        11 => {
            if shift {
                b')'
            } else {
                b'0'
            }
        }
        12 => {
            if shift {
                b'_'
            } else {
                b'-'
            }
        }
        13 => {
            if shift {
                b'+'
            } else {
                b'='
            }
        }
        14 => 8,
        15 => b'\t',
        16..=25 => b"qwertyuiop"[(code - 16) as usize],
        26 => {
            if shift {
                b'{'
            } else {
                b'['
            }
        }
        27 => {
            if shift {
                b'}'
            } else {
                b']'
            }
        }
        28 => b'\n',
        30..=38 => b"asdfghjkl"[(code - 30) as usize],
        39 => {
            if shift {
                b':'
            } else {
                b';'
            }
        }
        40 => {
            if shift {
                b'\"'
            } else {
                b'\''
            }
        }
        43 => {
            if shift {
                b'|'
            } else {
                b'\\'
            }
        }
        44..=50 => b"zxcvbnm"[(code - 44) as usize],
        51 => {
            if shift {
                b'<'
            } else {
                b','
            }
        }
        52 => {
            if shift {
                b'>'
            } else {
                b'.'
            }
        }
        53 => {
            if shift {
                b'?'
            } else {
                b'/'
            }
        }
        57 => b' ',
        60 => 0x82,  // F2: Text Edit save.
        102 => 0x15, // Home.
        103 => 0x13, // Up.
        105 => 0x11, // Left.
        106 => 0x12, // Right.
        107 => 0x16, // End.
        108 => 0x14, // Down.
        111 => 0x17, // Delete.
        _ => return None,
    };
    Some(if (shift ^ caps_lock) && byte.is_ascii_lowercase() {
        byte.to_ascii_uppercase()
    } else {
        byte
    })
}

fn read_abs_max(base: u64, axis: u16) -> Option<u32> {
    write8(base + REG_CONFIG, 0x12); // VIRTIO_INPUT_CFG_ABS_INFO
    write8(base + REG_CONFIG + 1, axis as u8);
    memory_barrier();
    (read8(base + REG_CONFIG + 2) >= 8).then(|| read32(base + REG_CONFIG + 8 + 4))
}

fn write_address(register: u64, address: u64) {
    write32(register, address as u32);
    write32(register + 4, (address >> 32) as u32);
}

fn fail_device(base: u64, message: &'static str) -> ! {
    write32(base + REG_STATUS, read32(base + REG_STATUS) | STATUS_FAILED);
    crate::fatal(message)
}

fn memory_barrier() {
    compiler_fence(Ordering::SeqCst);
    unsafe { core::arch::asm!("dmb ish", options(nostack)) };
    compiler_fence(Ordering::SeqCst);
}

#[inline(never)]
fn read8(address: u64) -> u8 {
    let value: u64;
    unsafe {
        core::arch::asm!(
            "ldrb {value:w}, [{address}]",
            address = in(reg) address,
            value = lateout(reg) value,
            options(nostack, readonly),
        )
    };
    value as u8
}

#[inline(never)]
fn write8(address: u64, value: u8) {
    unsafe {
        core::arch::asm!(
            "strb {value:w}, [{address}]",
            address = in(reg) address,
            value = in(reg) u64::from(value),
            options(nostack),
        )
    }
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

fn read16_memory(address: u64) -> u16 {
    unsafe { core::ptr::read_volatile(address as *const u16) }
}

fn read32_memory(address: u64) -> u32 {
    unsafe { core::ptr::read_volatile(address as *const u32) }
}

fn write16_memory(address: u64, value: u16) {
    unsafe { core::ptr::write_volatile(address as *mut u16, value) }
}

fn write32_memory(address: u64, value: u32) {
    unsafe { core::ptr::write_volatile(address as *mut u32, value) }
}

fn write64_memory(address: u64, value: u64) {
    unsafe { core::ptr::write_volatile(address as *mut u64, value) }
}
