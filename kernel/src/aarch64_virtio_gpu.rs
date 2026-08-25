use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering, compiler_fence};
use makos_boot_api::{FramebufferInfo, PixelFormat};

const MMIO_BASE: u64 = 0x0a00_0000;
const MMIO_STRIDE: u64 = 0x200;
const MMIO_SLOTS: usize = 32;
const MAGIC: u32 = 0x7472_6976;
const VERSION_MODERN: u32 = 2;
const DEVICE_GPU: u32 = 16;

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

const STATUS_ACKNOWLEDGE: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;
const STATUS_FAILED: u32 = 128;
const VIRTIO_F_VERSION_1: u32 = 1;

const QUEUE_SIZE: u16 = 8;
const DESC_OFFSET: u64 = 0x000;
const AVAIL_OFFSET: u64 = 0x100;
const USED_OFFSET: u64 = 0x200;
const REQUEST_OFFSET: u64 = 0x400;
const RESPONSE_OFFSET: u64 = 0x600;
const DESC_F_NEXT: u16 = 1;
const DESC_F_WRITE: u16 = 2;

const CMD_GET_DISPLAY_INFO: u32 = 0x0100;
const CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
const CMD_RESOURCE_UNREF: u32 = 0x0102;
const CMD_SET_SCANOUT: u32 = 0x0103;
const CMD_RESOURCE_FLUSH: u32 = 0x0104;
const CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
const CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;
const CMD_RESOURCE_DETACH_BACKING: u32 = 0x0107;
const CMD_UPDATE_CURSOR: u32 = 0x0300;
const CMD_MOVE_CURSOR: u32 = 0x0301;
const RESP_OK_NODATA: u32 = 0x1100;
const RESP_OK_DISPLAY_INFO: u32 = 0x1101;
const FORMAT_B8G8R8A8_UNORM: u32 = 1;
const FORMAT_B8G8R8X8_UNORM: u32 = 2;
const FLAG_FENCE: u32 = 1;
const CURSOR_RESOURCE_ID: u32 = u32::MAX - 1;
const CURSOR_SIZE: u32 = 64;
const CURSOR_BYTES: usize = CURSOR_SIZE as usize * CURSOR_SIZE as usize * 4;
const CURSOR_GLYPH_WIDTH: usize = 12;
const CURSOR_GLYPH_HEIGHT: usize = 16;

pub const MAX_WIDTH: u32 = 1280;
pub const MAX_HEIGHT: u32 = 800;
const MAX_PIXELS: usize = MAX_WIDTH as usize * MAX_HEIGHT as usize;
const MAX_BYTES: usize = MAX_PIXELS * 4;

#[repr(C, align(4096))]
struct AlignedFramebuffer([u32; MAX_PIXELS]);

#[repr(C, align(4096))]
struct AlignedCursor([u32; CURSOR_SIZE as usize * CURSOR_SIZE as usize]);

// Kernel ELF BSS is allocated as part of the physical image span and zeroed
// by the UEFI loader. Identity mapping makes this address valid guest DMA.
static mut FRAMEBUFFER: AlignedFramebuffer = AlignedFramebuffer([0; MAX_PIXELS]);
static mut CURSOR: AlignedCursor = AlignedCursor([0; CURSOR_SIZE as usize * CURSOR_SIZE as usize]);
static WIDTH: AtomicU32 = AtomicU32::new(800);
static HEIGHT: AtomicU32 = AtomicU32::new(600);
static OWNER_SUBMISSIONS: AtomicU64 = AtomicU64::new(0);
static OWNER_TRANSFERS: AtomicU64 = AtomicU64::new(0);
static OWNER_FLUSHES: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
struct State {
    ready: bool,
    base: u64,
    queue_frame: u64,
    cursor_queue_frame: u64,
    avail_index: u16,
    last_used: u16,
    cursor_avail_index: u16,
    cursor_last_used: u16,
    cursor_ready: bool,
    resource_id: u32,
    width: u32,
    height: u32,
    host_width: u32,
    host_height: u32,
}

struct LockedState {
    lock: AtomicBool,
    state: UnsafeCell<State>,
}

unsafe impl Sync for LockedState {}

static STATE: LockedState = LockedState {
    lock: AtomicBool::new(false),
    state: UnsafeCell::new(State {
        ready: false,
        base: 0,
        queue_frame: 0,
        cursor_queue_frame: 0,
        avail_index: 0,
        last_used: 0,
        cursor_avail_index: 0,
        cursor_last_used: 0,
        cursor_ready: false,
        resource_id: 0,
        width: 800,
        height: 600,
        host_width: 0,
        host_height: 0,
    }),
};

pub fn init(width: u32, height: u32) -> FramebufferInfo {
    require_owner_cpu();
    let mut found = None;
    for slot in 0..MMIO_SLOTS {
        let base = MMIO_BASE + slot as u64 * MMIO_STRIDE;
        if read32(base + REG_MAGIC) == MAGIC
            && read32(base + REG_VERSION) == VERSION_MODERN
            && read32(base + REG_DEVICE_ID) == DEVICE_GPU
        {
            if found.is_some() {
                crate::fatal("multiple AArch64 virtio-gpu devices");
            }
            found = Some((base, slot));
        }
    }
    let Some((base, slot)) = found else {
        crate::fatal("AArch64 virtio-gpu device absent");
    };
    let mut state = configure(base);
    query_display(&mut state);
    let (width, height) = if valid_mode(width, height) {
        (width, height)
    } else {
        (800, 600)
    };
    create_scanout(&mut state, width, height)
        .unwrap_or_else(|| crate::fatal("virtio-gpu initial scanout failed"));
    // Install the guest cursor before publishing STATE.  The cursor resource
    // and cursor queue are independent of the scanout resource; pointer motion
    // can then use CMD_MOVE_CURSOR without touching or flushing scanout pixels.
    create_cursor(
        &mut state,
        640.min(width.saturating_sub(1)),
        400.min(height.saturating_sub(1)),
    )
    .unwrap_or_else(|| crate::fatal("virtio-gpu cursor plane initialization failed"));
    let info = framebuffer_info(width, height);
    with_state(|destination| *destination = state);
    crate::serial_println!(
        "MAKOS_AARCH64_GPU_OK transport=virtio-mmio slot={} queue={} cursorq={} scanout=0 mode={}x{} host={}x{} resource={} transfer=2d flush=dirty cursor=virtio-gpu-plane move=cursorq scanout_damage=none host-cursor=hidden",
        slot,
        QUEUE_SIZE,
        QUEUE_SIZE,
        width,
        height,
        state.host_width,
        state.host_height,
        state.resource_id,
    );
    info
}

pub fn dimensions() -> (u32, u32) {
    (
        WIDTH.load(Ordering::Acquire),
        HEIGHT.load(Ordering::Acquire),
    )
}

pub fn set_mode(width: u32, height: u32) -> Option<FramebufferInfo> {
    require_owner_cpu();
    if !valid_mode(width, height) {
        return None;
    }
    with_state(|state| {
        if !state.ready {
            return None;
        }
        if state.width == width && state.height == height {
            return Some(framebuffer_info(width, height));
        }
        create_scanout(state, width, height)?;
        crate::serial_println!(
            "MAKOS_DISPLAY_MODE_OK backend=virtio-gpu scanout=0 mode={}x{} resource={} stride={} bytes={}",
            width,
            height,
            state.resource_id,
            width,
            u64::from(width) * u64::from(height) * 4,
        );
        Some(framebuffer_info(width, height))
    })
}

pub fn flush() {
    let (width, height) = dimensions();
    flush_rect(0, 0, width, height);
}

pub fn flush_rect(x: u32, y: u32, width: u32, height: u32) {
    require_owner_cpu();
    with_state(|state| {
        if !state.ready || width == 0 || height == 0 {
            return;
        }
        let x = x.min(state.width);
        let y = y.min(state.height);
        let width = width.min(state.width.saturating_sub(x));
        let height = height.min(state.height.saturating_sub(y));
        if width == 0 || height == 0 {
            return;
        }
        if !transfer(state, x, y, width, height) || !resource_flush(state, x, y, width, height) {
            fail(state.base, "virtio-gpu flush failed");
        }
    });
}

pub fn move_cursor(x: u32, y: u32) {
    require_owner_cpu();
    with_state(|state| {
        if !state.ready || !state.cursor_ready {
            return;
        }
        clear_cursor_request(state);
        let request = state.cursor_queue_frame + REQUEST_OFFSET;
        write32_memory(request, CMD_MOVE_CURSOR);
        write32_memory(request + 24, 0);
        write32_memory(request + 28, x.min(state.width.saturating_sub(1)));
        write32_memory(request + 32, y.min(state.height.saturating_sub(1)));
        // resource_id is formally update-only, but QEMU's Cocoa display uses
        // this field as the cursor-layer visibility flag for MOVE_CURSOR too.
        // Linux likewise retains the active cursor ID in its move request.
        write32_memory(request + 40, CURSOR_RESOURCE_ID);
        if !cursor_submit(state, 56) {
            fail(state.base, "virtio-gpu cursor move failed");
        }
    });
}

pub fn reset_service_affinity_evidence() {
    OWNER_SUBMISSIONS.store(0, Ordering::Release);
    OWNER_TRANSFERS.store(0, Ordering::Release);
    OWNER_FLUSHES.store(0, Ordering::Release);
}

pub fn service_affinity_evidence() -> (u64, u64, u64) {
    (
        OWNER_SUBMISSIONS.load(Ordering::Acquire),
        OWNER_TRANSFERS.load(Ordering::Acquire),
        OWNER_FLUSHES.load(Ordering::Acquire),
    )
}

fn valid_mode(width: u32, height: u32) -> bool {
    matches!((width, height), (800, 600) | (1024, 768) | (1280, 800))
}

fn framebuffer_info(width: u32, height: u32) -> FramebufferInfo {
    FramebufferInfo {
        address: framebuffer_address(),
        byte_len: u64::from(width) * u64::from(height) * 4,
        width,
        height,
        stride: width,
        pixel_format: PixelFormat::Bgr,
    }
}

fn framebuffer_address() -> u64 {
    (&raw mut FRAMEBUFFER).cast::<u8>() as u64
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
        fail(base, "virtio-gpu lacks VERSION_1");
    }
    write32(base + REG_DRIVER_FEATURES_SEL, 0);
    write32(base + REG_DRIVER_FEATURES, 0);
    write32(base + REG_DRIVER_FEATURES_SEL, 1);
    write32(base + REG_DRIVER_FEATURES, VIRTIO_F_VERSION_1);
    let feature_status = STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK;
    write32(base + REG_STATUS, feature_status);
    if read32(base + REG_STATUS) & STATUS_FEATURES_OK == 0 {
        fail(base, "virtio-gpu feature negotiation failed");
    }

    let frame = configure_queue(base, 0, "virtio-gpu control queue unsupported");
    let cursor_frame = configure_queue(base, 1, "virtio-gpu cursor queue unsupported");
    memory_barrier();
    write32(base + REG_STATUS, feature_status | STATUS_DRIVER_OK);
    if read32(base + REG_STATUS) & STATUS_FAILED != 0 {
        fail(base, "virtio-gpu entered FAILED state");
    }
    State {
        ready: false,
        base,
        queue_frame: frame,
        cursor_queue_frame: cursor_frame,
        avail_index: 0,
        last_used: 0,
        cursor_avail_index: 0,
        cursor_last_used: 0,
        cursor_ready: false,
        resource_id: 0,
        width: 800,
        height: 600,
        host_width: 0,
        host_height: 0,
    }
}

fn configure_queue(base: u64, index: u32, failure: &'static str) -> u64 {
    write32(base + REG_QUEUE_SEL, index);
    if read32(base + REG_QUEUE_NUM_MAX) < u32::from(QUEUE_SIZE)
        || read32(base + REG_QUEUE_READY) != 0
    {
        fail(base, failure);
    }
    let frame =
        crate::mm::allocate_frame().unwrap_or_else(|| crate::fatal("virtio-gpu queue frame OOM"));
    unsafe { core::ptr::write_bytes(frame as *mut u8, 0, 4096) };
    write32(base + REG_QUEUE_NUM, u32::from(QUEUE_SIZE));
    write_address(base + REG_QUEUE_DESC, frame + DESC_OFFSET);
    write_address(base + REG_QUEUE_DRIVER, frame + AVAIL_OFFSET);
    write_address(base + REG_QUEUE_DEVICE, frame + USED_OFFSET);
    write32(base + REG_QUEUE_READY, 1);
    memory_barrier();
    frame
}

fn query_display(state: &mut State) {
    clear_request(state);
    write_header(state, CMD_GET_DISPLAY_INFO);
    if submit(state, 24, 408) != Some(RESP_OK_DISPLAY_INFO) {
        fail(state.base, "virtio-gpu GET_DISPLAY_INFO failed");
    }
    let response = state.queue_frame + RESPONSE_OFFSET;
    let enabled = read32_memory(response + 40);
    if enabled != 0 {
        state.host_width = read32_memory(response + 32);
        state.host_height = read32_memory(response + 36);
    }
}

fn create_scanout(state: &mut State, width: u32, height: u32) -> Option<()> {
    let old_resource = state.resource_id;
    let resource = old_resource.checked_add(1).filter(|value| *value != 0)?;
    unsafe { core::ptr::write_bytes(framebuffer_address() as *mut u8, 0, MAX_BYTES) };

    clear_request(state);
    write_header(state, CMD_RESOURCE_CREATE_2D);
    let request = state.queue_frame + REQUEST_OFFSET;
    write32_memory(request + 24, resource);
    write32_memory(request + 28, FORMAT_B8G8R8X8_UNORM);
    write32_memory(request + 32, width);
    write32_memory(request + 36, height);
    expect_nodata(state, 40)?;

    clear_request(state);
    write_header(state, CMD_RESOURCE_ATTACH_BACKING);
    write32_memory(request + 24, resource);
    write32_memory(request + 28, 1);
    write64_memory(request + 32, framebuffer_address());
    write32_memory(request + 40, width.checked_mul(height)?.checked_mul(4)?);
    write32_memory(request + 44, 0);
    expect_nodata(state, 48)?;

    clear_request(state);
    write_header(state, CMD_SET_SCANOUT);
    write_rect(request + 24, 0, 0, width, height);
    write32_memory(request + 40, 0);
    write32_memory(request + 44, resource);
    expect_nodata(state, 48)?;

    state.resource_id = resource;
    state.width = width;
    state.height = height;
    WIDTH.store(width, Ordering::Release);
    HEIGHT.store(height, Ordering::Release);
    transfer(state, 0, 0, width, height).then_some(())?;
    resource_flush(state, 0, 0, width, height).then_some(())?;

    if old_resource != 0 {
        clear_request(state);
        write_header(state, CMD_RESOURCE_DETACH_BACKING);
        write32_memory(request + 24, old_resource);
        write32_memory(request + 28, 0);
        expect_nodata(state, 32)?;

        clear_request(state);
        write_header(state, CMD_RESOURCE_UNREF);
        write32_memory(request + 24, old_resource);
        write32_memory(request + 28, 0);
        expect_nodata(state, 32)?;
    }
    state.ready = true;
    Some(())
}

fn create_cursor(state: &mut State, x: u32, y: u32) -> Option<()> {
    unsafe {
        core::ptr::write_bytes((&raw mut CURSOR).cast::<u8>(), 0, CURSOR_BYTES);
        let pixels = (&raw mut CURSOR.0).cast::<u32>();
        // Single connected triangular pointer. Previous hand-written masks
        // split into two tails after row 9, resembling duplicate/ghost cursors
        // under Cocoa/Retina scaling. Hotspot stays exact top-left pixel.
        for row in 0..CURSOR_GLYPH_HEIGHT {
            let row_width = 1 + row * (CURSOR_GLYPH_WIDTH - 1) / (CURSOR_GLYPH_HEIGHT - 1);
            for column in 0..row_width {
                let border =
                    column == 0 || column + 1 == row_width || row + 1 == CURSOR_GLYPH_HEIGHT;
                let pixel = if border { 0xff00_0000 } else { 0xffff_ffff };
                core::ptr::write_volatile(pixels.add(row * CURSOR_SIZE as usize + column), pixel);
            }
        }
    }
    clear_request(state);
    write_header_fenced(state, CMD_RESOURCE_CREATE_2D);
    let request = state.queue_frame + REQUEST_OFFSET;
    write32_memory(request + 24, CURSOR_RESOURCE_ID);
    write32_memory(request + 28, FORMAT_B8G8R8A8_UNORM);
    write32_memory(request + 32, CURSOR_SIZE);
    write32_memory(request + 36, CURSOR_SIZE);
    expect_nodata(state, 40)?;
    clear_request(state);
    write_header_fenced(state, CMD_RESOURCE_ATTACH_BACKING);
    write32_memory(request + 24, CURSOR_RESOURCE_ID);
    write32_memory(request + 28, 1);
    write64_memory(request + 32, (&raw mut CURSOR).cast::<u8>() as u64);
    write32_memory(request + 40, CURSOR_BYTES as u32);
    write32_memory(request + 44, 0);
    expect_nodata(state, 48)?;
    clear_request(state);
    write_header_fenced(state, CMD_TRANSFER_TO_HOST_2D);
    write_rect(request + 24, 0, 0, CURSOR_SIZE, CURSOR_SIZE);
    write64_memory(request + 40, 0);
    write32_memory(request + 48, CURSOR_RESOURCE_ID);
    write32_memory(request + 52, 0);
    expect_nodata(state, 56)?;
    clear_cursor_request(state);
    let cursor = state.cursor_queue_frame + REQUEST_OFFSET;
    write32_memory(cursor, CMD_UPDATE_CURSOR);
    write32_memory(cursor + 24, 0);
    write32_memory(cursor + 28, x);
    write32_memory(cursor + 32, y);
    write32_memory(cursor + 40, CURSOR_RESOURCE_ID);
    write32_memory(cursor + 44, 0);
    write32_memory(cursor + 48, 0);
    if !cursor_submit(state, 56) {
        return None;
    }
    state.cursor_ready = true;
    Some(())
}

fn transfer(state: &mut State, x: u32, y: u32, width: u32, height: u32) -> bool {
    clear_request(state);
    write_header(state, CMD_TRANSFER_TO_HOST_2D);
    let request = state.queue_frame + REQUEST_OFFSET;
    write_rect(request + 24, x, y, width, height);
    let offset = (u64::from(y) * u64::from(state.width) + u64::from(x)) * 4;
    write64_memory(request + 40, offset);
    write32_memory(request + 48, state.resource_id);
    write32_memory(request + 52, 0);
    let complete = expect_nodata(state, 56).is_some();
    if complete {
        OWNER_TRANSFERS.fetch_add(1, Ordering::AcqRel);
    }
    complete
}

fn resource_flush(state: &mut State, x: u32, y: u32, width: u32, height: u32) -> bool {
    clear_request(state);
    write_header(state, CMD_RESOURCE_FLUSH);
    let request = state.queue_frame + REQUEST_OFFSET;
    write_rect(request + 24, x, y, width, height);
    write32_memory(request + 40, state.resource_id);
    write32_memory(request + 44, 0);
    let complete = expect_nodata(state, 48).is_some();
    if complete {
        OWNER_FLUSHES.fetch_add(1, Ordering::AcqRel);
    }
    complete
}

fn expect_nodata(state: &mut State, request_length: u32) -> Option<()> {
    (submit(state, request_length, 24)? == RESP_OK_NODATA).then_some(())
}

fn clear_request(state: &State) {
    unsafe {
        core::ptr::write_bytes((state.queue_frame + REQUEST_OFFSET) as *mut u8, 0, 512);
        core::ptr::write_bytes((state.queue_frame + RESPONSE_OFFSET) as *mut u8, 0, 512);
    }
}

fn write_header(state: &State, command: u32) {
    write32_memory(state.queue_frame + REQUEST_OFFSET, command);
}

fn write_header_fenced(state: &State, command: u32) {
    write_header(state, command);
    write32_memory(state.queue_frame + REQUEST_OFFSET + 4, FLAG_FENCE);
    write64_memory(state.queue_frame + REQUEST_OFFSET + 8, u64::from(command));
}

fn clear_cursor_request(state: &State) {
    unsafe {
        core::ptr::write_bytes(
            (state.cursor_queue_frame + REQUEST_OFFSET) as *mut u8,
            0,
            512,
        );
        core::ptr::write_bytes(
            (state.cursor_queue_frame + RESPONSE_OFFSET) as *mut u8,
            0,
            512,
        );
    }
}

fn submit(state: &mut State, request_length: u32, response_length: u32) -> Option<u32> {
    require_owner_cpu();
    write_descriptor(
        state.queue_frame + DESC_OFFSET,
        state.queue_frame + REQUEST_OFFSET,
        request_length,
        DESC_F_NEXT,
        1,
    );
    write_descriptor(
        state.queue_frame + DESC_OFFSET + 16,
        state.queue_frame + RESPONSE_OFFSET,
        response_length,
        DESC_F_WRITE,
        0,
    );
    let slot = u64::from(state.avail_index % QUEUE_SIZE);
    write16_memory(state.queue_frame + AVAIL_OFFSET + 4 + slot * 2, 0);
    memory_barrier();
    state.avail_index = state.avail_index.wrapping_add(1);
    write16_memory(state.queue_frame + AVAIL_OFFSET + 2, state.avail_index);
    memory_barrier();
    write32(state.base + REG_QUEUE_NOTIFY, 0);

    let expected = state.last_used.wrapping_add(1);
    let mut complete = false;
    for _ in 0..10_000_000 {
        memory_barrier();
        if read16_memory(state.queue_frame + USED_OFFSET + 2) == expected {
            complete = true;
            break;
        }
        core::hint::spin_loop();
    }
    if !complete {
        return None;
    }
    let used_slot = u64::from(state.last_used % QUEUE_SIZE);
    if read32_memory(state.queue_frame + USED_OFFSET + 4 + used_slot * 8) != 0 {
        return None;
    }
    state.last_used = expected;
    let interrupt = read32(state.base + REG_INTERRUPT_STATUS);
    if interrupt != 0 {
        write32(state.base + REG_INTERRUPT_ACK, interrupt);
    }
    memory_barrier();
    OWNER_SUBMISSIONS.fetch_add(1, Ordering::AcqRel);
    Some(read32_memory(state.queue_frame + RESPONSE_OFFSET))
}

fn cursor_submit(state: &mut State, request_length: u32) -> bool {
    require_owner_cpu();
    let frame = state.cursor_queue_frame;
    write_descriptor(
        frame + DESC_OFFSET,
        frame + REQUEST_OFFSET,
        request_length,
        0,
        0,
    );
    let slot = u64::from(state.cursor_avail_index % QUEUE_SIZE);
    write16_memory(frame + AVAIL_OFFSET + 4 + slot * 2, 0);
    memory_barrier();
    state.cursor_avail_index = state.cursor_avail_index.wrapping_add(1);
    write16_memory(frame + AVAIL_OFFSET + 2, state.cursor_avail_index);
    memory_barrier();
    write32(state.base + REG_QUEUE_NOTIFY, 1);

    let expected = state.cursor_last_used.wrapping_add(1);
    for _ in 0..10_000_000 {
        memory_barrier();
        if read16_memory(frame + USED_OFFSET + 2) == expected {
            let used_slot = u64::from(state.cursor_last_used % QUEUE_SIZE);
            if read32_memory(frame + USED_OFFSET + 4 + used_slot * 8) != 0 {
                return false;
            }
            state.cursor_last_used = expected;
            let interrupt = read32(state.base + REG_INTERRUPT_STATUS);
            if interrupt != 0 {
                write32(state.base + REG_INTERRUPT_ACK, interrupt);
            }
            memory_barrier();
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

fn require_owner_cpu() {
    if crate::arch::cpu_index() != 0 {
        crate::fatal("AArch64 virtio-gpu MMIO attempted from non-owner CPU");
    }
}

fn write_rect(address: u64, x: u32, y: u32, width: u32, height: u32) {
    write32_memory(address, x);
    write32_memory(address + 4, y);
    write32_memory(address + 8, width);
    write32_memory(address + 12, height);
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

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    while STATE
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = f(unsafe { &mut *STATE.state.get() });
    STATE.lock.store(false, Ordering::Release);
    result
}

fn memory_barrier() {
    compiler_fence(Ordering::SeqCst);
    unsafe { core::arch::asm!("dmb ish", options(nostack)) };
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
