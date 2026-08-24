use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};

const HEAP_SIZE: usize = 1024 * 1024;

#[repr(C, align(4096))]
struct HeapArena([u8; HEAP_SIZE]);

struct State {
    free: *mut FreeBlock,
}

#[repr(C)]
struct FreeBlock {
    size: usize,
    next: *mut FreeBlock,
}

#[repr(C)]
struct AllocationHeader {
    start: usize,
    size: usize,
}

struct LockedBump {
    locked: AtomicBool,
    initialized: AtomicBool,
    state: UnsafeCell<State>,
}

unsafe impl Sync for LockedBump {}

impl LockedBump {
    const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
            initialized: AtomicBool::new(false),
            state: UnsafeCell::new(State {
                free: ptr::null_mut(),
            }),
        }
    }

    fn lock(&self) {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }

    fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}

#[global_allocator]
static HEAP: LockedBump = LockedBump::new();
static mut ARENA: HeapArena = HeapArena([0; HEAP_SIZE]);

unsafe impl GlobalAlloc for LockedBump {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !self.initialized.load(Ordering::Acquire) {
            return ptr::null_mut();
        }
        self.lock();
        let state = unsafe { &mut *self.state.get() };
        let mut previous: *mut FreeBlock = ptr::null_mut();
        let mut block = state.free;
        let mut result = ptr::null_mut();
        while !block.is_null() {
            let block_start = block as usize;
            let block_size = unsafe { (*block).size };
            let Some(block_end) = block_start.checked_add(block_size) else {
                break;
            };
            let Some(user) = align_up(
                block_start + core::mem::size_of::<AllocationHeader>(),
                layout.align(),
            ) else {
                break;
            };
            let Some(request_end) = user.checked_add(layout.size().max(1)) else {
                break;
            };
            if request_end <= block_end {
                let next_block = unsafe { (*block).next };
                let suffix =
                    align_up(request_end, core::mem::align_of::<FreeBlock>()).unwrap_or(block_end);
                let allocation_end =
                    if block_end.saturating_sub(suffix) >= core::mem::size_of::<FreeBlock>() {
                        let replacement = suffix as *mut FreeBlock;
                        unsafe {
                            replacement.write(FreeBlock {
                                size: block_end - suffix,
                                next: next_block,
                            });
                        }
                        if previous.is_null() {
                            state.free = replacement;
                        } else {
                            unsafe { (*previous).next = replacement };
                        }
                        suffix
                    } else {
                        if previous.is_null() {
                            state.free = next_block;
                        } else {
                            unsafe { (*previous).next = next_block };
                        }
                        block_end
                    };
                let header =
                    (user - core::mem::size_of::<AllocationHeader>()) as *mut AllocationHeader;
                unsafe {
                    header.write(AllocationHeader {
                        start: block_start,
                        size: allocation_end - block_start,
                    });
                }
                result = user as *mut u8;
                break;
            }
            previous = block;
            block = unsafe { (*block).next };
        }
        self.unlock();
        result
    }

    unsafe fn dealloc(&self, allocation: *mut u8, _layout: Layout) {
        if allocation.is_null() || !self.initialized.load(Ordering::Acquire) {
            return;
        }
        let header = unsafe {
            allocation
                .sub(core::mem::size_of::<AllocationHeader>())
                .cast::<AllocationHeader>()
                .read()
        };
        self.lock();
        let state = unsafe { &mut *self.state.get() };
        let mut previous: *mut FreeBlock = ptr::null_mut();
        let mut next = state.free;
        while !next.is_null() && (next as usize) < header.start {
            previous = next;
            next = unsafe { (*next).next };
        }
        let released = header.start as *mut FreeBlock;
        unsafe {
            released.write(FreeBlock {
                size: header.size,
                next,
            });
        }
        if previous.is_null() {
            state.free = released;
        } else {
            unsafe { (*previous).next = released };
        }
        let mut merged = released;
        if !next.is_null() && header.start + unsafe { (*released).size } == next as usize {
            unsafe {
                (*released).size += (*next).size;
                (*released).next = (*next).next;
            }
        }
        if !previous.is_null()
            && previous as usize + unsafe { (*previous).size } == released as usize
        {
            unsafe {
                (*previous).size += (*released).size;
                (*previous).next = (*released).next;
            }
            merged = previous;
        }
        let _ = merged;
        self.unlock();
    }
}

pub fn init_and_test() {
    HEAP.lock();
    let start = (&raw mut ARENA).cast::<u8>() as usize;
    unsafe {
        let block = start as *mut FreeBlock;
        block.write(FreeBlock {
            size: HEAP_SIZE,
            next: ptr::null_mut(),
        });
        (*HEAP.state.get()).free = block;
    }
    HEAP.initialized.store(true, Ordering::Release);
    HEAP.unlock();

    let boxed = alloc::boxed::Box::new(0x4d41_4b4f_53u64);
    let mut vector = alloc::vec::Vec::with_capacity(64);
    for value in 0..64u64 {
        vector.push(value * value);
    }
    if *boxed != 0x4d41_4b4f_53 || vector.len() != 64 || vector[63] != 3969 {
        crate::fatal("kernel heap self-test failed");
    }
    crate::serial_println!("heap bytes={} box_vec=ok", HEAP_SIZE);
}

fn align_up(value: usize, alignment: usize) -> Option<usize> {
    value
        .checked_add(alignment - 1)
        .map(|n| n & !(alignment - 1))
}
