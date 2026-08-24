use core::cell::UnsafeCell;
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};

pub const PROT_READ: u64 = 1;
pub const PROT_WRITE: u64 = 2;
pub const PROT_EXEC: u64 = 4;
const PAGE_SIZE: usize = 4096;
const MAX_REGIONS: usize = 16;
const MAX_PAGES_PER_REGION: usize = 16;
const ARENA_PAGES: usize = 256;
pub const USER_VM_BASE: u64 = crate::process::USER_CODE_BASE + 0x10_0000;

#[derive(Clone, Copy)]
struct Region {
    used: bool,
    owner_pid: u64,
    base: u64,
    pages: usize,
    writable: u16,
    executable: u16,
}

impl Region {
    const EMPTY: Self = Self {
        used: false,
        owner_pid: 0,
        base: 0,
        pages: 0,
        writable: 0,
        executable: 0,
    };
}

struct State {
    regions: [Region; MAX_REGIONS],
}

struct LockedState {
    lock: AtomicBool,
    state: UnsafeCell<State>,
}

unsafe impl Sync for LockedState {}

static STATE: LockedState = LockedState {
    lock: AtomicBool::new(false),
    state: UnsafeCell::new(State {
        regions: [Region::EMPTY; MAX_REGIONS],
    }),
};

pub fn map(pid: u64, length: usize, protection: u64) -> Option<u64> {
    let pages = page_count(length)?;
    if pages > MAX_PAGES_PER_REGION || !valid_protection(protection) {
        return None;
    }
    with_state(|state| {
        let slot = state.regions.iter().position(|region| !region.used)?;
        let start = (0..=ARENA_PAGES.checked_sub(pages)?).find(|start| {
            let base = USER_VM_BASE + (*start * PAGE_SIZE) as u64;
            !state.regions.iter().any(|region| {
                region.used
                    && region.owner_pid == pid
                    && overlaps(base, pages, region.base, region.pages)
            })
        })?;
        let base = USER_VM_BASE + (start * PAGE_SIZE) as u64;
        let root = current_address_space(pid)?;
        let writable = protection & PROT_WRITE != 0;
        let executable = protection & PROT_EXEC != 0;
        let mut mapped = 0usize;
        while mapped < pages {
            let Some(frame) = crate::mm::allocate_frame() else {
                rollback_mapping(base, mapped);
                return None;
            };
            unsafe { ptr::write_bytes(frame as *mut u8, 0, PAGE_SIZE) };
            crate::arch::map_user_page_in(
                root,
                base + (mapped * PAGE_SIZE) as u64,
                frame,
                writable,
                executable,
            );
            mapped += 1;
        }
        let mask = page_mask(pages);
        state.regions[slot] = Region {
            used: true,
            owner_pid: pid,
            base,
            pages,
            writable: if writable { mask } else { 0 },
            executable: if executable { mask } else { 0 },
        };
        Some(base)
    })
}

pub fn unmap(pid: u64, address: u64, length: usize) -> bool {
    if address & (PAGE_SIZE as u64 - 1) != 0 {
        return false;
    }
    let Some(pages) = page_count(length) else {
        return false;
    };
    with_state(|state| {
        let Some(index) = state.regions.iter().position(|region| {
            region.used
                && region.owner_pid == pid
                && region.base == address
                && region.pages == pages
        }) else {
            return false;
        };
        for page in 0..pages {
            let virtual_address = address + (page * PAGE_SIZE) as u64;
            let Some(frame) = crate::arch::unmap_user_page(virtual_address) else {
                crate::fatal("tracked VM page absent during unmap");
            };
            if crate::mm::free_frame(frame).is_err() {
                crate::fatal("VM frame reclaim failed");
            }
        }
        state.regions[index] = Region::EMPTY;
        true
    })
}

pub fn protect(pid: u64, address: u64, length: usize, protection: u64) -> bool {
    if address & (PAGE_SIZE as u64 - 1) != 0 || !valid_protection(protection) {
        return false;
    }
    let Some(pages) = page_count(length) else {
        return false;
    };
    with_state(|state| {
        let Some(region) = state.regions.iter_mut().find(|region| {
            let Some(end) = address.checked_add((pages * PAGE_SIZE) as u64) else {
                return false;
            };
            region.used
                && region.owner_pid == pid
                && address >= region.base
                && end <= region.base + (region.pages * PAGE_SIZE) as u64
        }) else {
            return false;
        };
        let first = ((address - region.base) / PAGE_SIZE as u64) as usize;
        let writable = protection & PROT_WRITE != 0;
        let executable = protection & PROT_EXEC != 0;
        for page in first..first + pages {
            let virtual_address = region.base + (page * PAGE_SIZE) as u64;
            if !crate::arch::protect_user_page(virtual_address, writable, executable) {
                crate::fatal("tracked VM page absent during protect");
            }
            let bit = 1u16 << page;
            if writable {
                region.writable |= bit;
            } else {
                region.writable &= !bit;
            }
            if executable {
                region.executable |= bit;
            } else {
                region.executable &= !bit;
            }
        }
        true
    })
}

pub fn forget_all(pid: u64) -> (usize, usize) {
    with_state(|state| {
        let mut regions = 0usize;
        let mut pages = 0usize;
        for region in &mut state.regions {
            if region.used && region.owner_pid == pid {
                regions += 1;
                pages += region.pages;
                *region = Region::EMPTY;
            }
        }
        (regions, pages)
    })
}

fn current_address_space(pid: u64) -> Option<u64> {
    if pid != crate::scheduler::current_pid() {
        return None;
    }
    crate::scheduler::address_space(pid)
}

fn page_count(length: usize) -> Option<usize> {
    (length != 0).then_some(length.checked_add(PAGE_SIZE - 1)? / PAGE_SIZE)
}

fn valid_protection(protection: u64) -> bool {
    protection & !(PROT_READ | PROT_WRITE | PROT_EXEC) == 0
        && protection & PROT_READ != 0
        && !(protection & PROT_WRITE != 0 && protection & PROT_EXEC != 0)
}

fn page_mask(pages: usize) -> u16 {
    if pages == 16 {
        u16::MAX
    } else {
        (1u16 << pages) - 1
    }
}

fn overlaps(first_base: u64, first_pages: usize, second_base: u64, second_pages: usize) -> bool {
    let first_end = first_base + (first_pages * PAGE_SIZE) as u64;
    let second_end = second_base + (second_pages * PAGE_SIZE) as u64;
    first_base < second_end && second_base < first_end
}

fn rollback_mapping(base: u64, mapped: usize) {
    for page in 0..mapped {
        let address = base + (page * PAGE_SIZE) as u64;
        if let Some(frame) = crate::arch::unmap_user_page(address) {
            let _ = crate::mm::free_frame(frame);
        }
    }
}

fn with_state<R>(function: impl FnOnce(&mut State) -> R) -> R {
    while STATE
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *STATE.state.get() });
    STATE.lock.store(false, Ordering::Release);
    result
}
