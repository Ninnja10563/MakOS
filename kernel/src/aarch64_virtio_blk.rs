use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering, compiler_fence};

const MMIO_BASE: u64 = 0x0a00_0000;
const MMIO_STRIDE: u64 = 0x200;
const MMIO_SLOTS: usize = 32;
const MAX_DEVICES: usize = 2;
const MAGIC: u32 = 0x7472_6976;
const VERSION_MODERN: u32 = 2;
const DEVICE_BLOCK: u32 = 2;

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
const VIRTIO_BLK_F_FLUSH: u32 = 1 << 9;

const QUEUE_SIZE: u16 = 8;
const DESC_OFFSET: u64 = 0x000;
const AVAIL_OFFSET: u64 = 0x100;
const USED_OFFSET: u64 = 0x200;
const REQUEST_OFFSET: u64 = 0x400;
const DATA_OFFSET: u64 = 0x500;
const STATUS_OFFSET: u64 = 0x1500;
const DESC_F_NEXT: u16 = 1;
const DESC_F_WRITE: u16 = 2;
const REQUEST_READ: u32 = 0;
const REQUEST_WRITE: u32 = 1;
const REQUEST_FLUSH: u32 = 4;
const FAST_COMPLETION_SPINS: u32 = 10_000_000;
const MAX_COMPLETION_SPINS: u32 = 200_000_000;

#[derive(Clone, Copy)]
struct State {
    ready: bool,
    base: u64,
    queue_frame: u64,
    sectors: u64,
    avail_index: u16,
    last_used: u16,
    flush_supported: bool,
}

impl State {
    const EMPTY: Self = Self {
        ready: false,
        base: 0,
        queue_frame: 0,
        sectors: 0,
        avail_index: 0,
        last_used: 0,
        flush_supported: false,
    };
}

struct LockedState {
    lock: AtomicBool,
    states: UnsafeCell<[State; MAX_DEVICES]>,
    count: UnsafeCell<usize>,
}

unsafe impl Sync for LockedState {}

static STATE: LockedState = LockedState {
    lock: AtomicBool::new(false),
    states: UnsafeCell::new([State::EMPTY; MAX_DEVICES]),
    count: UnsafeCell::new(0),
};
static SOURCE_WRITES_FROZEN: AtomicBool = AtomicBool::new(false);

pub struct SourceWriteFreeze {
    thaw_on_drop: bool,
}

impl SourceWriteFreeze {
    pub fn keep_until_shutdown(mut self) {
        self.thaw_on_drop = false;
    }
}

impl Drop for SourceWriteFreeze {
    fn drop(&mut self) {
        if self.thaw_on_drop {
            SOURCE_WRITES_FROZEN.store(false, Ordering::Release);
        }
    }
}

pub fn init() {
    SOURCE_WRITES_FROZEN.store(false, Ordering::Release);
    let mut states = [State::EMPTY; MAX_DEVICES];
    let mut slots = [0usize; MAX_DEVICES];
    let mut count = 0usize;
    // QEMU assigns command-line virtio-mmio devices from highest slot down.
    // Descending discovery therefore gives stable disk0=first, disk1=second.
    for slot in (0..MMIO_SLOTS).rev() {
        let base = MMIO_BASE + slot as u64 * MMIO_STRIDE;
        if read32(base + REG_MAGIC) == MAGIC
            && read32(base + REG_VERSION) == VERSION_MODERN
            && read32(base + REG_DEVICE_ID) == DEVICE_BLOCK
        {
            if count == MAX_DEVICES {
                crate::fatal("too many AArch64 virtio-blk disks");
            }
            states[count] = configure(base);
            slots[count] = slot;
            count += 1;
        }
    }
    if count == 0 {
        crate::fatal("AArch64 virtio-blk data disk absent");
    }
    with_locked_state(|destinations, destination_count| {
        *destinations = states;
        *destination_count = count;
    });
    for device in 0..count {
        let mut sector_reads = [0u8; 4096];
        for sector in 0..8u32 {
            let mut bytes = [0u8; 512];
            if !read_sector_on(device, 4096 + sector, &mut bytes) {
                crate::fatal("virtio-blk 512-byte self-test read failed");
            }
            let offset = sector as usize * 512;
            sector_reads[offset..offset + 512].copy_from_slice(&bytes);
        }
        let mut block_read = [0u8; 4096];
        if !read_sectors_8_on(device, 4096, &mut block_read) || block_read != sector_reads {
            crate::fatal("virtio-blk 4KiB DMA self-test mismatch");
        }
        crate::serial_println!(
            "MAKOS_AARCH64_BLOCK_OK transport=virtio-mmio device={} slot={} sectors={} queue={} read=1 write=1 flush=request read4k=1 write4k=1",
            device,
            slots[device],
            states[device].sectors,
            QUEUE_SIZE,
        );
    }
    crate::serial_println!("MAKOS_AARCH64_BLOCK_ENUM_OK devices={}", count);
}

pub fn sectors() -> Option<u64> {
    device_sectors(0)
}

pub fn device_count() -> usize {
    with_locked_state(|_, count| *count)
}

pub fn device_sectors(device: usize) -> Option<u64> {
    with_device(device, |state| state.sectors)
}

pub fn read_sector(lba: u32, output: &mut [u8; 512]) -> bool {
    read_sector_on(0, lba, output)
}

pub fn read_sector_on(device: usize, lba: u32, output: &mut [u8; 512]) -> bool {
    with_device(device, |state| {
        if u64::from(lba) >= state.sectors {
            return false;
        }
        if !submit(state, REQUEST_READ, u64::from(lba), 512) {
            return false;
        }
        for (index, byte) in output.iter_mut().enumerate() {
            *byte = read8_memory(state.queue_frame + DATA_OFFSET + index as u64);
        }
        true
    })
    .unwrap_or(false)
}

pub fn read_sectors_8(lba: u32, output: &mut [u8; 4096]) -> bool {
    read_sectors_8_on(0, lba, output)
}

pub fn read_sectors_8_on(device: usize, lba: u32, output: &mut [u8; 4096]) -> bool {
    with_device(device, |state| {
        if u64::from(lba).saturating_add(8) > state.sectors {
            return false;
        }
        if !submit(state, REQUEST_READ, u64::from(lba), 4096) {
            return false;
        }
        for (index, byte) in output.iter_mut().enumerate() {
            *byte = read8_memory(state.queue_frame + DATA_OFFSET + index as u64);
        }
        true
    })
    .unwrap_or(false)
}

pub fn write_sector(lba: u32, input: &[u8; 512]) -> bool {
    write_sector_on(0, lba, input)
}

pub fn write_sector_on(device: usize, lba: u32, input: &[u8; 512]) -> bool {
    with_device(device, |state| {
        if (device == 0 && SOURCE_WRITES_FROZEN.load(Ordering::Acquire))
            || u64::from(lba) >= state.sectors
        {
            return false;
        }
        for (index, byte) in input.iter().copied().enumerate() {
            write8_memory(state.queue_frame + DATA_OFFSET + index as u64, byte);
        }
        submit(state, REQUEST_WRITE, u64::from(lba), 512)
    })
    .unwrap_or(false)
}

pub fn write_sectors_8_on(device: usize, lba: u32, input: &[u8; 4096]) -> bool {
    with_device(device, |state| {
        if (device == 0 && SOURCE_WRITES_FROZEN.load(Ordering::Acquire))
            || u64::from(lba).saturating_add(8) > state.sectors
        {
            return false;
        }
        for (index, byte) in input.iter().copied().enumerate() {
            write8_memory(state.queue_frame + DATA_OFFSET + index as u64, byte);
        }
        submit(state, REQUEST_WRITE, u64::from(lba), 4096)
    })
    .unwrap_or(false)
}

pub fn flush() -> bool {
    flush_on(0)
}

pub fn flush_on(device: usize) -> bool {
    with_device(device, |state| {
        state.flush_supported && submit(state, REQUEST_FLUSH, 0, 0)
    })
    .unwrap_or(false)
}

pub fn freeze_source_writes() -> Option<SourceWriteFreeze> {
    with_device(0, |state| {
        if SOURCE_WRITES_FROZEN.load(Ordering::Acquire)
            || !state.flush_supported
            || !submit(state, REQUEST_FLUSH, 0, 0)
        {
            return None;
        }
        SOURCE_WRITES_FROZEN.store(true, Ordering::Release);
        Some(SourceWriteFreeze { thaw_on_drop: true })
    })
    .flatten()
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
        fail(base, "virtio-blk lacks VERSION_1");
    }
    write32(base + REG_DRIVER_FEATURES_SEL, 0);
    write32(base + REG_DEVICE_FEATURES_SEL, 0);
    let device_features = read32(base + REG_DEVICE_FEATURES);
    if device_features & VIRTIO_BLK_F_FLUSH == 0 {
        fail(base, "virtio-blk lacks FLUSH");
    }
    write32(base + REG_DRIVER_FEATURES, VIRTIO_BLK_F_FLUSH);
    write32(base + REG_DRIVER_FEATURES_SEL, 1);
    write32(base + REG_DRIVER_FEATURES, VIRTIO_F_VERSION_1);
    let feature_status = STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK;
    write32(base + REG_STATUS, feature_status);
    if read32(base + REG_STATUS) & STATUS_FEATURES_OK == 0 {
        fail(base, "virtio-blk feature negotiation failed");
    }

    write32(base + REG_QUEUE_SEL, 0);
    if read32(base + REG_QUEUE_NUM_MAX) < u32::from(QUEUE_SIZE)
        || read32(base + REG_QUEUE_READY) != 0
    {
        fail(base, "virtio-blk request queue unsupported");
    }
    let frame = crate::mm::allocate_contiguous_frames(2)
        .unwrap_or_else(|| crate::fatal("virtio-blk DMA frames OOM"));
    unsafe { core::ptr::write_bytes(frame as *mut u8, 0, 8192) };
    write32(base + REG_QUEUE_NUM, u32::from(QUEUE_SIZE));
    write_address(base + REG_QUEUE_DESC, frame + DESC_OFFSET);
    write_address(base + REG_QUEUE_DRIVER, frame + AVAIL_OFFSET);
    write_address(base + REG_QUEUE_DEVICE, frame + USED_OFFSET);
    write32(base + REG_QUEUE_READY, 1);
    memory_barrier();
    write32(base + REG_STATUS, feature_status | STATUS_DRIVER_OK);
    if read32(base + REG_STATUS) & STATUS_FAILED != 0 {
        fail(base, "virtio-blk entered FAILED state");
    }
    let sectors =
        u64::from(read32(base + REG_CONFIG)) | (u64::from(read32(base + REG_CONFIG + 4)) << 32);
    if sectors < 4096 {
        fail(base, "virtio-blk data disk too small");
    }
    State {
        ready: true,
        base,
        queue_frame: frame,
        sectors,
        avail_index: 0,
        last_used: 0,
        flush_supported: true,
    }
}

fn submit(state: &mut State, request_type: u32, sector: u64, data_length: u32) -> bool {
    write32_memory(state.queue_frame + REQUEST_OFFSET, request_type);
    write32_memory(state.queue_frame + REQUEST_OFFSET + 4, 0);
    write64_memory(state.queue_frame + REQUEST_OFFSET + 8, sector);
    write8_memory(state.queue_frame + STATUS_OFFSET, 0xff);

    write_descriptor(
        state.queue_frame + DESC_OFFSET,
        state.queue_frame + REQUEST_OFFSET,
        16,
        DESC_F_NEXT,
        1,
    );
    if request_type == REQUEST_FLUSH {
        write_descriptor(
            state.queue_frame + DESC_OFFSET + 16,
            state.queue_frame + STATUS_OFFSET,
            1,
            DESC_F_WRITE,
            0,
        );
    } else {
        write_descriptor(
            state.queue_frame + DESC_OFFSET + 16,
            state.queue_frame + DATA_OFFSET,
            data_length,
            DESC_F_NEXT
                | if request_type == REQUEST_READ {
                    DESC_F_WRITE
                } else {
                    0
                },
            2,
        );
        write_descriptor(
            state.queue_frame + DESC_OFFSET + 32,
            state.queue_frame + STATUS_OFFSET,
            1,
            DESC_F_WRITE,
            0,
        );
    }
    let slot = u64::from(state.avail_index % QUEUE_SIZE);
    write16_memory(state.queue_frame + AVAIL_OFFSET + 4 + slot * 2, 0);
    memory_barrier();
    state.avail_index = state.avail_index.wrapping_add(1);
    write16_memory(state.queue_frame + AVAIL_OFFSET + 2, state.avail_index);
    memory_barrier();
    write32(state.base + REG_QUEUE_NOTIFY, 0);

    let expected = state.last_used.wrapping_add(1);
    let mut completed = false;
    let mut delayed = false;
    for spin in 0..MAX_COMPLETION_SPINS {
        memory_barrier();
        if read16_memory(state.queue_frame + USED_OFFSET + 2) == expected {
            completed = true;
            break;
        }
        if spin == FAST_COMPLETION_SPINS {
            delayed = true;
            crate::serial_println!(
                "MAKOS_AARCH64_BLOCK_DELAYED request={} sector={} expected_used={} observed_used={}",
                request_type,
                sector,
                expected,
                read16_memory(state.queue_frame + USED_OFFSET + 2),
            );
        }
        core::hint::spin_loop();
    }
    if !completed {
        crate::serial_println!(
            "MAKOS_AARCH64_BLOCK_TIMEOUT request={} sector={} expected_used={} observed_used={} avail={}",
            request_type,
            sector,
            expected,
            read16_memory(state.queue_frame + USED_OFFSET + 2),
            state.avail_index,
        );
        return false;
    }
    let used_slot = u64::from(state.last_used % QUEUE_SIZE);
    let id = read32_memory(state.queue_frame + USED_OFFSET + 4 + used_slot * 8);
    state.last_used = expected;
    let interrupt = read32(state.base + REG_INTERRUPT_STATUS);
    if interrupt != 0 {
        write32(state.base + REG_INTERRUPT_ACK, interrupt);
    }
    let status = read8_memory(state.queue_frame + STATUS_OFFSET);
    if delayed {
        crate::serial_println!(
            "MAKOS_AARCH64_BLOCK_RECOVERED request={} sector={} used={} status={}",
            request_type,
            sector,
            expected,
            status,
        );
    }
    if id != 0 || status != 0 {
        crate::serial_println!(
            "MAKOS_AARCH64_BLOCK_ERROR request={} sector={} descriptor={} status={}",
            request_type,
            sector,
            id,
            status,
        );
        return false;
    }
    true
}

fn write_descriptor(address: u64, buffer: u64, length: u32, flags: u16, next: u16) {
    write64_memory(address, buffer);
    write32_memory(address + 8, length);
    write16_memory(address + 12, flags);
    write16_memory(address + 14, next);
}

fn write_address(register: u64, address: u64) {
    write32(register, address as u32);
    write32(register + 4, (address >> 32) as u32);
}

fn fail(base: u64, message: &'static str) -> ! {
    write32(base + REG_STATUS, read32(base + REG_STATUS) | STATUS_FAILED);
    crate::fatal(message)
}

fn memory_barrier() {
    compiler_fence(Ordering::SeqCst);
    unsafe { core::arch::asm!("dmb ish", options(nostack)) };
    compiler_fence(Ordering::SeqCst);
}

fn with_locked_state<T>(action: impl FnOnce(&mut [State; MAX_DEVICES], &mut usize) -> T) -> T {
    while STATE
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = action(unsafe { &mut *STATE.states.get() }, unsafe {
        &mut *STATE.count.get()
    });
    STATE.lock.store(false, Ordering::Release);
    result
}

fn with_device<T>(device: usize, action: impl FnOnce(&mut State) -> T) -> Option<T> {
    with_locked_state(|states, count| {
        if device >= *count || !states[device].ready {
            None
        } else {
            Some(action(&mut states[device]))
        }
    })
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

fn read16_memory(address: u64) -> u16 {
    unsafe { core::ptr::read_volatile(address as *const u16) }
}

fn read32_memory(address: u64) -> u32 {
    unsafe { core::ptr::read_volatile(address as *const u32) }
}

fn write8_memory(address: u64, value: u8) {
    unsafe { core::ptr::write_volatile(address as *mut u8, value) }
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
