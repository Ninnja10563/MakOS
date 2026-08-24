use core::cell::UnsafeCell;
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering, compiler_fence};

const MMIO_BASE: u64 = 0x0a00_0000;
const MMIO_STRIDE: u64 = 0x200;
const MMIO_SLOTS: usize = 32;
const MAGIC: u32 = 0x7472_6976;
const DEVICE_RNG: u32 = 4;
const VERSION_MODERN: u32 = 2;

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

const STATUS_ACKNOWLEDGE: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;
const STATUS_FAILED: u32 = 128;
const VIRTIO_F_VERSION_1: u32 = 1;

const DESC_OFFSET: u64 = 0;
const AVAIL_OFFSET: u64 = 64;
const USED_OFFSET: u64 = 128;
const BUFFER_OFFSET: u64 = 256;
const BUFFER_BYTES: usize = 256;

#[derive(Clone, Copy)]
struct State {
    base: u64,
    queue_frame: u64,
    avail_index: u16,
    used_index: u16,
    ready: bool,
}

impl State {
    const EMPTY: Self = Self {
        base: 0,
        queue_frame: 0,
        avail_index: 0,
        used_index: 0,
        ready: false,
    };
}

struct LockedState {
    lock: AtomicBool,
    state: UnsafeCell<State>,
}

unsafe impl Sync for LockedState {}

static RNG: LockedState = LockedState {
    lock: AtomicBool::new(false),
    state: UnsafeCell::new(State::EMPTY),
};

pub fn init() {
    let mut found = None;
    for slot in 0..MMIO_SLOTS {
        let base = MMIO_BASE + slot as u64 * MMIO_STRIDE;
        if read32(base + REG_MAGIC) == MAGIC
            && read32(base + REG_VERSION) == VERSION_MODERN
            && read32(base + REG_DEVICE_ID) == DEVICE_RNG
        {
            if found.is_some() {
                crate::fatal("multiple AArch64 virtio-rng devices");
            }
            found = Some((slot, base));
        }
    }
    let (slot, base) = found.unwrap_or_else(|| crate::fatal("AArch64 virtio-rng device absent"));
    crate::serial_println!(
        "virtio-rng trace slot={} stage=probe base={:#x}",
        slot,
        base
    );
    let frame = configure(base);
    crate::serial_println!("virtio-rng trace slot={} stage=configured", slot);
    with_state(|state| {
        *state = State {
            base,
            queue_frame: frame,
            avail_index: 0,
            used_index: 0,
            ready: true,
        };
    });
    let mut probe = [0u8; 32];
    crate::serial_println!("virtio-rng trace slot={} stage=request", slot);
    if !fill(&mut probe) || probe.iter().all(|byte| *byte == 0) {
        crate::fatal("AArch64 virtio-rng entropy probe failed");
    }
    probe.fill(0);
    crate::serial_println!(
        "MAKOS_AARCH64_RNG_OK transport=virtio-rng-mmio slot={} source=host-urandom queue=1 bytes=32 zeroized=1",
        slot,
    );
}

pub fn fill(output: &mut [u8]) -> bool {
    if output.is_empty() {
        return true;
    }
    with_state(|state| {
        if !state.ready {
            return false;
        }
        let mut offset = 0usize;
        while offset < output.len() {
            let count = (output.len() - offset).min(BUFFER_BYTES);
            let descriptor = state.queue_frame + DESC_OFFSET;
            write64_memory(descriptor, state.queue_frame + BUFFER_OFFSET);
            write32_memory(descriptor + 8, count as u32);
            write16_memory(descriptor + 12, 2); // VIRTQ_DESC_F_WRITE
            write16_memory(descriptor + 14, 0);
            write16_memory(state.queue_frame + AVAIL_OFFSET + 4, 0);
            state.avail_index = state.avail_index.wrapping_add(1);
            memory_barrier();
            write16_memory(state.queue_frame + AVAIL_OFFSET + 2, state.avail_index);
            memory_barrier();
            write32(state.base + REG_QUEUE_NOTIFY, 0);

            let mut completed = false;
            for _ in 0..10_000_000 {
                memory_barrier();
                let used = read16_memory(state.queue_frame + USED_OFFSET + 2);
                if used != state.used_index {
                    state.used_index = used;
                    completed = true;
                    break;
                }
                core::hint::spin_loop();
            }
            let used_length = read32_memory(state.queue_frame + USED_OFFSET + 8) as usize;
            if !completed || used_length < count {
                return false;
            }
            for index in 0..count {
                output[offset + index] = unsafe {
                    ptr::read_volatile(
                        (state.queue_frame + BUFFER_OFFSET + index as u64) as *const u8,
                    )
                };
            }
            let interrupts = read32(state.base + REG_INTERRUPT_STATUS);
            if interrupts != 0 {
                write32(state.base + REG_INTERRUPT_ACK, interrupts);
            }
            offset += count;
        }
        true
    })
}

fn configure(base: u64) -> u64 {
    crate::serial_println!("virtio-rng trace stage=reset");
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
        fail(base, "virtio-rng lacks VERSION_1");
    }
    write32(base + REG_DRIVER_FEATURES_SEL, 0);
    write32(base + REG_DRIVER_FEATURES, 0);
    write32(base + REG_DRIVER_FEATURES_SEL, 1);
    write32(base + REG_DRIVER_FEATURES, VIRTIO_F_VERSION_1);
    let features = STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK;
    write32(base + REG_STATUS, features);
    if read32(base + REG_STATUS) & STATUS_FEATURES_OK == 0 {
        fail(base, "virtio-rng feature negotiation failed");
    }
    write32(base + REG_QUEUE_SEL, 0);
    crate::serial_println!("virtio-rng trace stage=features-ok");
    if read32(base + REG_QUEUE_NUM_MAX) < 1 || read32(base + REG_QUEUE_READY) != 0 {
        fail(base, "virtio-rng queue unavailable");
    }
    let frame = crate::mm::allocate_frame()
        .unwrap_or_else(|| crate::fatal("AArch64 virtio-rng queue frame OOM"));
    unsafe { ptr::write_bytes(frame as *mut u8, 0, 4096) };
    write32(base + REG_QUEUE_NUM, 1);
    write_address(base + REG_QUEUE_DESC_LOW, frame + DESC_OFFSET);
    write_address(base + REG_QUEUE_DRIVER_LOW, frame + AVAIL_OFFSET);
    write_address(base + REG_QUEUE_DEVICE_LOW, frame + USED_OFFSET);
    write32(base + REG_QUEUE_READY, 1);
    memory_barrier();
    write32(base + REG_STATUS, features | STATUS_DRIVER_OK);
    crate::serial_println!("virtio-rng trace stage=driver-ok");
    if read32(base + REG_STATUS) & STATUS_FAILED != 0 {
        fail(base, "virtio-rng entered FAILED state");
    }
    frame
}

fn with_state<R>(function: impl FnOnce(&mut State) -> R) -> R {
    while RNG
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *RNG.state.get() });
    RNG.lock.store(false, Ordering::Release);
    result
}

fn fail(base: u64, message: &str) -> ! {
    write32(base + REG_STATUS, read32(base + REG_STATUS) | STATUS_FAILED);
    crate::fatal(message)
}

fn write_address(register: u64, address: u64) {
    write32(register, address as u32);
    write32(register + 4, (address >> 32) as u32);
}

fn memory_barrier() {
    compiler_fence(Ordering::SeqCst);
    unsafe { core::arch::asm!("dmb ish", options(nostack, preserves_flags)) };
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
    unsafe { ptr::read_volatile(address as *const u16) }
}

fn read32_memory(address: u64) -> u32 {
    unsafe { ptr::read_volatile(address as *const u32) }
}

fn write16_memory(address: u64, value: u16) {
    unsafe { ptr::write_volatile(address as *mut u16, value) }
}

fn write32_memory(address: u64, value: u32) {
    unsafe { ptr::write_volatile(address as *mut u32, value) }
}

fn write64_memory(address: u64, value: u64) {
    unsafe { ptr::write_volatile(address as *mut u64, value) }
}
