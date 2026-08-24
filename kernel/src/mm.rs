use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use makos_boot_api::{BootInfo, UefiMemoryDescriptor};
use makos_frame_allocator::{FrameAllocator, FreeError};

const MAX_PHYSICAL_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_FRAMES: usize = (MAX_PHYSICAL_BYTES / makos_frame_allocator::PAGE_SIZE) as usize;
const BITMAP_WORDS: usize = MAX_FRAMES / 64;
const EFI_CONVENTIONAL_MEMORY: u32 = 7;

static mut FRAME_BITMAP: [u64; BITMAP_WORDS] = [0; BITMAP_WORDS];
static ALLOCATOR: LockedAllocator = LockedAllocator::new();
static MANAGED_MIB: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
pub struct Stats {
    pub managed_mib: u64,
    pub free_frames: usize,
}

struct LockedAllocator {
    lock: AtomicBool,
    initialized: AtomicBool,
    value: UnsafeCell<MaybeUninit<FrameAllocator<'static>>>,
}

unsafe impl Sync for LockedAllocator {}

impl LockedAllocator {
    const fn new() -> Self {
        Self {
            lock: AtomicBool::new(false),
            initialized: AtomicBool::new(false),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    fn acquire(&self) {
        while self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }

    fn release(&self) {
        self.lock.store(false, Ordering::Release);
    }

    fn with<R>(&self, f: impl FnOnce(&mut FrameAllocator<'static>) -> R) -> R {
        if !self.initialized.load(Ordering::Acquire) {
            crate::fatal("physical allocator used before initialization");
        }
        self.acquire();
        let result = unsafe { f((&mut *self.value.get()).assume_init_mut()) };
        self.release();
        result
    }
}

pub fn init(boot: &BootInfo) -> Stats {
    if ALLOCATOR.initialized.load(Ordering::Acquire) {
        crate::fatal("physical allocator initialized twice");
    }
    ALLOCATOR.acquire();
    let bitmap = unsafe {
        core::slice::from_raw_parts_mut((&raw mut FRAME_BITMAP).cast::<u64>(), BITMAP_WORDS)
    };
    let mut allocator = FrameAllocator::new(bitmap);

    let map = boot.memory_map;
    for index in 0..map.entry_count {
        let address = map
            .address
            .checked_add(index.saturating_mul(map.descriptor_size as u64))
            .unwrap_or_else(|| crate::fatal("memory-map address overflow"));
        let descriptor = unsafe { (address as *const UefiMemoryDescriptor).read_unaligned() };
        if descriptor.memory_type == EFI_CONVENTIONAL_MEMORY {
            allocator.release_region(descriptor.physical_start, descriptor.page_count);
        }
    }

    // Never return physical frame zero; null physical pointers remain invalid.
    allocator.reserve_region(0, 1);
    let stats = Stats {
        // Report usable physical RAM, not highest managed address. Virtio MMIO
        // holes can make address span look like tens of GiB on a 1 GiB guest.
        managed_mib: allocator.free_count() as u64 * makos_frame_allocator::PAGE_SIZE
            / (1024 * 1024),
        free_frames: allocator.free_count(),
    };
    MANAGED_MIB.store(stats.managed_mib, Ordering::Release);
    unsafe { (*ALLOCATOR.value.get()).write(allocator) };
    ALLOCATOR.initialized.store(true, Ordering::Release);
    ALLOCATOR.release();
    stats
}

pub fn allocate_frame() -> Option<u64> {
    ALLOCATOR.with(FrameAllocator::allocate)
}

pub fn allocate_contiguous_frames(count: usize) -> Option<u64> {
    ALLOCATOR.with(|allocator| allocator.allocate_contiguous(count))
}

pub fn free_frame(address: u64) -> Result<(), FreeError> {
    ALLOCATOR.with(|allocator| allocator.free(address))
}

pub fn free_frames() -> usize {
    ALLOCATOR.with(|allocator| allocator.free_count())
}

pub fn managed_mib() -> u64 {
    MANAGED_MIB.load(Ordering::Acquire)
}
