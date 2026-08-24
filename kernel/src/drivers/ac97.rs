use crate::arch::{outb, outl};
use crate::drivers::pci;
use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};

const VENDOR_INTEL: u16 = 0x8086;
const DEVICE_ICH_AC97: u16 = 0x2415;
const PCM_BYTES: usize = 4096;

#[repr(C, packed)]
struct BufferDescriptor {
    address: u32,
    samples: u16,
    flags: u16,
}

#[repr(C, align(4096))]
struct PcmBuffer([i16; PCM_BYTES / 2]);

static mut PCM: PcmBuffer = PcmBuffer([0; PCM_BYTES / 2]);
static mut DESCRIPTORS: [BufferDescriptor; 32] = [const {
    BufferDescriptor {
        address: 0,
        samples: 0,
        flags: 0,
    }
}; 32];
static BUS_MASTER_IO: AtomicU16 = AtomicU16::new(0);
static STREAM_LOCK: AtomicBool = AtomicBool::new(false);

pub fn self_test() {
    let device = pci::find(VENDOR_INTEL, DEVICE_ICH_AC97)
        .unwrap_or_else(|| crate::fatal("AC97 PCI device absent"));
    device.enable_io_bus_master();
    let mixer_bar = device.read(0x10);
    let bus_master_bar = device.read(0x14);
    if mixer_bar & 1 == 0 || bus_master_bar & 1 == 0 {
        crate::fatal("AC97 I/O BAR absent");
    }
    let mixer = (mixer_bar & 0xfffc) as u16;
    let bm = (bus_master_bar & 0xfffc) as u16;
    unsafe {
        outw(mixer + 0x00, 0); // codec reset
        outw(mixer + 0x02, 0); // master volume unmuted, 0 dB
        outw(mixer + 0x18, 0); // PCM out volume
        outw(mixer + 0x2c, 48_000); // front DAC rate
    }
    let samples = unsafe { &mut *(&raw mut PCM).cast::<PcmBuffer>() };
    for frame in 0..PCM_BYTES / 4 {
        let phase = frame % 96;
        let amplitude = if phase < 48 { 7000i16 } else { -7000i16 };
        samples.0[frame * 2] = amplitude;
        samples.0[frame * 2 + 1] = amplitude;
    }
    let descriptor = (&raw mut DESCRIPTORS).cast::<BufferDescriptor>();
    unsafe {
        (*descriptor).address = (&raw mut PCM).cast::<u8>() as u32;
        (*descriptor).samples = (PCM_BYTES / 2) as u16;
        (*descriptor).flags = 1 << 15; // interrupt on completion
        outb(bm + 0x1b, 0x02); // reset PCM-out channel
        outl(bm + 0x10, descriptor as u32);
        outb(bm + 0x15, 0); // last valid descriptor
        outb(bm + 0x1b, 0x01); // run
    }
    let mut progressed = false;
    for _ in 0..20_000_000 {
        let status = unsafe { inw(bm + 0x16) };
        let position = unsafe { inw(bm + 0x18) };
        if position != 0 || status & 0x0c != 0 {
            progressed = true;
            break;
        }
        core::hint::spin_loop();
    }
    unsafe { outb(bm + 0x1b, 0) };
    if !progressed {
        crate::fatal("AC97 DMA did not progress");
    }
    BUS_MASTER_IO.store(bm, Ordering::Release);
    crate::serial_println!(
        "MAKOS_AUDIO_OK pci={:02x}:{:02x}.{} mixer={:#x} bm={:#x} rate=48000 channels=2 pcm_dma=1",
        device.bus,
        device.slot,
        device.function,
        mixer,
        bm
    );
}

pub fn write_pcm(samples: &[i16], rate: u32, channels: u32) -> bool {
    if samples.is_empty()
        || samples.len() > PCM_BYTES / 2
        || rate != 48_000
        || channels != 2
        || !samples.len().is_multiple_of(2)
    {
        return false;
    }
    if STREAM_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return false;
    }
    let bm = BUS_MASTER_IO.load(Ordering::Acquire);
    if bm == 0 {
        STREAM_LOCK.store(false, Ordering::Release);
        return false;
    }
    let buffer = unsafe { &mut *(&raw mut PCM).cast::<PcmBuffer>() };
    buffer.0.fill(0);
    buffer.0[..samples.len()].copy_from_slice(samples);
    let descriptor = (&raw mut DESCRIPTORS).cast::<BufferDescriptor>();
    unsafe {
        (*descriptor).address = (&raw mut PCM).cast::<u8>() as u32;
        (*descriptor).samples = samples.len() as u16;
        (*descriptor).flags = 1 << 15;
        outb(bm + 0x1b, 0x02);
        outl(bm + 0x10, descriptor as u32);
        outb(bm + 0x15, 0);
        outb(bm + 0x1b, 0x01);
    }
    let mut progressed = false;
    for _ in 0..20_000_000 {
        let status = unsafe { inw(bm + 0x16) };
        let position = unsafe { inw(bm + 0x18) };
        if position != 0 || status & 0x0c != 0 {
            progressed = true;
            break;
        }
        core::hint::spin_loop();
    }
    unsafe { outb(bm + 0x1b, 0) };
    STREAM_LOCK.store(false, Ordering::Release);
    if progressed {
        crate::serial_println!(
            "MAKOS_AUDIO_STREAM_OK pid={} frames={} rate={} channels={} userspace=1 dma=1",
            crate::scheduler::current_pid(),
            samples.len() / 2,
            rate,
            channels
        );
    }
    progressed
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
