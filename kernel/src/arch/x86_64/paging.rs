use core::arch::asm;
use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering};

const ENTRIES: usize = 512;
const GIB: u64 = 1024 * 1024 * 1024;
const PRESENT: u64 = 1;
const WRITABLE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const HUGE_PAGE: u64 = 1 << 7;
const NO_EXECUTE: u64 = 1 << 63;
const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;
const IA32_EFER: u32 = 0xc000_0080;
const EFER_NXE: u64 = 1 << 11;
static ROOT_TABLE: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    enable_execute_disable();
    let pml4 = allocate_table();
    let pdpt = allocate_table();
    unsafe { write_entry(pml4, 0, pdpt | PRESENT | WRITABLE) };

    // Early identity map: 0..4 GiB with 2 MiB leaves. Covers kernel, boot
    // handoff, current stacks, ACPI, LAPIC, and QEMU GOP framebuffer.
    for gib_index in 0..4u64 {
        let page_directory = allocate_table();
        unsafe {
            write_entry(
                pdpt,
                gib_index as usize,
                page_directory | PRESENT | WRITABLE,
            );
            for entry in 0..ENTRIES {
                let physical = gib_index * GIB + entry as u64 * 2 * 1024 * 1024;
                write_entry(
                    page_directory,
                    entry,
                    physical | PRESENT | WRITABLE | HUGE_PAGE,
                );
            }
        }
    }

    unsafe { asm!("mov cr3, {}", in(reg) pml4, options(nostack, preserves_flags)) };
    let active: u64;
    unsafe { asm!("mov {}, cr3", out(reg) active, options(nomem, nostack, preserves_flags)) };
    if active & ADDRESS_MASK != pml4 {
        crate::fatal("CR3 activation failed");
    }
    ROOT_TABLE.store(pml4, Ordering::Release);
    super::interrupts::page_fault_self_test();
    crate::serial_println!("paging cr3={:#x} identity_gib=4 page_fault=ok", pml4);
}

pub fn map_user_page_in(
    pml4: u64,
    virtual_address: u64,
    physical_address: u64,
    writable: bool,
    executable: bool,
) {
    if virtual_address & 0xfff != 0 || physical_address & 0xfff != 0 {
        crate::fatal("unaligned user-page mapping");
    }
    let pml4_index = ((virtual_address >> 39) & 0x1ff) as usize;
    let pdpt_index = ((virtual_address >> 30) & 0x1ff) as usize;
    let pd_index = ((virtual_address >> 21) & 0x1ff) as usize;
    let pt_index = ((virtual_address >> 12) & 0x1ff) as usize;
    let pdpt = get_or_create(pml4, pml4_index);
    let pd = get_or_create(pdpt, pdpt_index);
    let pt = get_or_create(pd, pd_index);
    let mut flags = PRESENT | USER;
    if writable {
        flags |= WRITABLE;
    }
    if !executable {
        flags |= NO_EXECUTE;
    }
    let slot = (pt as *mut u64).wrapping_add(pt_index);
    if unsafe { slot.read_volatile() } & PRESENT != 0 {
        crate::fatal("duplicate user-page mapping");
    }
    unsafe {
        slot.write_volatile(physical_address | flags);
        if pml4 == root_table() {
            asm!("invlpg [{}]", in(reg) virtual_address, options(nostack, preserves_flags));
        }
    }
}

pub fn new_user_address_space() -> u64 {
    let source_root = root_table();
    let source_pdpt = unsafe { *((source_root as *const u64).add(0)) } & ADDRESS_MASK;
    let root = allocate_table();
    let pdpt = allocate_table();
    unsafe {
        write_entry(root, 0, pdpt | PRESENT | WRITABLE | USER);
        for index in 0..4 {
            write_entry(pdpt, index, *((source_pdpt as *const u64).add(index)));
        }
    }
    root
}

pub fn switch_address_space(root: u64) {
    if root & 0xfff != 0 {
        crate::fatal("unaligned address-space root");
    }
    let active: u64;
    unsafe { asm!("mov {}, cr3", out(reg) active, options(nomem, nostack, preserves_flags)) };
    if active & ADDRESS_MASK != root {
        unsafe { asm!("mov cr3, {}", in(reg) root, options(nostack, preserves_flags)) };
    }
}

pub fn destroy_user_address_space(root: u64) -> usize {
    if root == root_table() || root == active_root() || root & 0xfff != 0 {
        crate::fatal("invalid address-space destruction");
    }
    let mut freed = 0usize;
    for pml4_index in 0..ENTRIES {
        let pml4_entry = unsafe { *((root as *const u64).add(pml4_index)) };
        if pml4_entry & (PRESENT | USER) != (PRESENT | USER) || pml4_entry & HUGE_PAGE != 0 {
            continue;
        }
        let pdpt = pml4_entry & ADDRESS_MASK;
        for pdpt_index in 0..ENTRIES {
            let pdpt_entry = unsafe { *((pdpt as *const u64).add(pdpt_index)) };
            if pdpt_entry & (PRESENT | USER) != (PRESENT | USER) {
                continue;
            }
            if pdpt_entry & HUGE_PAGE != 0 {
                crate::fatal("user 1-GiB page destruction unsupported");
            }
            let pd = pdpt_entry & ADDRESS_MASK;
            for pd_index in 0..ENTRIES {
                let pd_entry = unsafe { *((pd as *const u64).add(pd_index)) };
                if pd_entry & (PRESENT | USER) != (PRESENT | USER) {
                    continue;
                }
                if pd_entry & HUGE_PAGE != 0 {
                    crate::fatal("user 2-MiB page destruction unsupported");
                }
                let pt = pd_entry & ADDRESS_MASK;
                for pt_index in 0..ENTRIES {
                    let slot = unsafe { (pt as *mut u64).add(pt_index) };
                    let entry = unsafe { slot.read_volatile() };
                    if entry & (PRESENT | USER) == (PRESENT | USER) {
                        unsafe { slot.write_volatile(0) };
                        free_frame(entry & ADDRESS_MASK);
                        freed += 1;
                    }
                }
                unsafe { write_entry(pd, pd_index, 0) };
                free_frame(pt);
                freed += 1;
            }
            unsafe { write_entry(pdpt, pdpt_index, 0) };
            free_frame(pd);
            freed += 1;
        }
        unsafe { write_entry(root, pml4_index, 0) };
        free_frame(pdpt);
        freed += 1;
    }
    free_frame(root);
    freed + 1
}

pub fn unmap_user_page(virtual_address: u64) -> Option<u64> {
    if virtual_address & 0xfff != 0 {
        return None;
    }
    let root = active_root();
    let pml4_index = ((virtual_address >> 39) & 0x1ff) as usize;
    let pdpt_index = ((virtual_address >> 30) & 0x1ff) as usize;
    let pd_index = ((virtual_address >> 21) & 0x1ff) as usize;
    let pt_index = ((virtual_address >> 12) & 0x1ff) as usize;
    let pdpt = child(root, pml4_index)?;
    let pd = child(pdpt, pdpt_index)?;
    let pt = child(pd, pd_index)?;
    let slot = (pt as *mut u64).wrapping_add(pt_index);
    let entry = unsafe { slot.read_volatile() };
    if entry & (PRESENT | USER) != (PRESENT | USER) {
        return None;
    }
    unsafe {
        slot.write_volatile(0);
        asm!("invlpg [{}]", in(reg) virtual_address, options(nostack, preserves_flags));
    }
    Some(entry & ADDRESS_MASK)
}

pub fn protect_user_page(virtual_address: u64, writable: bool, executable: bool) -> bool {
    if virtual_address & 0xfff != 0 || (writable && executable) {
        return false;
    }
    let root = active_root();
    let pml4_index = ((virtual_address >> 39) & 0x1ff) as usize;
    let pdpt_index = ((virtual_address >> 30) & 0x1ff) as usize;
    let pd_index = ((virtual_address >> 21) & 0x1ff) as usize;
    let pt_index = ((virtual_address >> 12) & 0x1ff) as usize;
    let Some(pdpt) = child(root, pml4_index) else {
        return false;
    };
    let Some(pd) = child(pdpt, pdpt_index) else {
        return false;
    };
    let Some(pt) = child(pd, pd_index) else {
        return false;
    };
    let slot = (pt as *mut u64).wrapping_add(pt_index);
    let entry = unsafe { slot.read_volatile() };
    if entry & (PRESENT | USER) != (PRESENT | USER) {
        return false;
    }
    let mut flags = PRESENT | USER;
    if writable {
        flags |= WRITABLE;
    }
    if !executable {
        flags |= NO_EXECUTE;
    }
    unsafe {
        slot.write_volatile((entry & ADDRESS_MASK) | flags);
        asm!("invlpg [{}]", in(reg) virtual_address, options(nostack, preserves_flags));
    }
    true
}

pub fn user_page_writable(virtual_address: u64) -> Option<bool> {
    let indexes = [
        ((virtual_address >> 39) & 0x1ff) as usize,
        ((virtual_address >> 30) & 0x1ff) as usize,
        ((virtual_address >> 21) & 0x1ff) as usize,
        ((virtual_address >> 12) & 0x1ff) as usize,
    ];
    let mut table = active_root();
    for index in &indexes[..3] {
        let entry = unsafe { *((table as *const u64).add(*index)) };
        if entry & (PRESENT | USER) != (PRESENT | USER) || entry & HUGE_PAGE != 0 {
            return None;
        }
        table = entry & ADDRESS_MASK;
    }
    let entry = unsafe { *((table as *const u64).add(indexes[3])) };
    (entry & (PRESENT | USER) == (PRESENT | USER)).then_some(entry & WRITABLE != 0)
}

fn active_root() -> u64 {
    let root: u64;
    unsafe { asm!("mov {}, cr3", out(reg) root, options(nomem, nostack, preserves_flags)) };
    root & ADDRESS_MASK
}

fn child(table: u64, index: usize) -> Option<u64> {
    let entry = unsafe { *((table as *const u64).add(index)) };
    (entry & PRESENT != 0 && entry & HUGE_PAGE == 0).then_some(entry & ADDRESS_MASK)
}

pub(super) fn root_table() -> u64 {
    let root = ROOT_TABLE.load(Ordering::Acquire);
    if root == 0 {
        crate::fatal("page-table root unavailable");
    }
    root
}

fn allocate_table() -> u64 {
    let frame = crate::mm::allocate_frame().unwrap_or_else(|| crate::fatal("page-table frame OOM"));
    unsafe { ptr::write_bytes(frame as *mut u8, 0, 4096) };
    frame
}

fn free_frame(frame: u64) {
    if crate::mm::free_frame(frame).is_err() {
        crate::fatal("address-space frame reclaim failed");
    }
}

fn get_or_create(table: u64, index: usize) -> u64 {
    let slot = (table as *mut u64).wrapping_add(index);
    let mut entry = unsafe { slot.read_volatile() };
    if entry & PRESENT == 0 {
        let child = allocate_table();
        entry = child | PRESENT | WRITABLE | USER;
        unsafe { slot.write_volatile(entry) };
    } else {
        if entry & HUGE_PAGE != 0 {
            crate::fatal("user mapping collides with huge page");
        }
        if entry & USER == 0 {
            entry |= USER;
            unsafe { slot.write_volatile(entry) };
        }
    }
    entry & ADDRESS_MASK
}

fn enable_execute_disable() {
    let low: u32;
    let high: u32;
    unsafe {
        asm!("rdmsr", in("ecx") IA32_EFER, out("eax") low, out("edx") high, options(nomem, nostack, preserves_flags));
    }
    let value = ((high as u64) << 32) | low as u64 | EFER_NXE;
    unsafe {
        asm!("wrmsr", in("ecx") IA32_EFER, in("eax") value as u32, in("edx") (value >> 32) as u32, options(nomem, nostack, preserves_flags));
    }
}

unsafe fn write_entry(table: u64, index: usize, value: u64) {
    debug_assert!(index < ENTRIES);
    unsafe { (table as *mut u64).add(index).write_volatile(value) };
}
