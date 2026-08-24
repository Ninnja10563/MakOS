use core::cell::UnsafeCell;
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub const PAGE_SIZE: u64 = 4096;
const MAX_OBJECTS: usize = 128;
const MAX_PAGES: usize = 16 * 1024;
const MAX_NAME_BYTES: usize = 128;
const MAX_OBJECT_BYTES: u64 = 1024 * 1024 * 1024;
const SHMEM_TRUNCATE_TRACE_LIMIT: u64 = 8;
static SHMEM_TRUNCATE_TRACES: AtomicU64 = AtomicU64::new(0);

pub type ObjectId = u16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Exists,
    NotFound,
    Permission,
    NoSpace,
    TooLarge,
    Invalid,
}

#[derive(Clone, Copy)]
pub struct ObjectMetadata {
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub modified_ticks: u64,
    pub inode: u64,
}

#[derive(Clone, Copy)]
struct Object {
    used: bool,
    linked: bool,
    name: [u8; MAX_NAME_BYTES],
    name_length: u8,
    mode: u32,
    uid: u32,
    gid: u32,
    size: u64,
    modified_ticks: u64,
    handle_refs: u32,
    mapping_refs: u32,
}

impl Object {
    const EMPTY: Self = Self {
        used: false,
        linked: false,
        name: [0; MAX_NAME_BYTES],
        name_length: 0,
        mode: 0,
        uid: 0,
        gid: 0,
        size: 0,
        modified_ticks: 0,
        handle_refs: 0,
        mapping_refs: 0,
    };
}

#[derive(Clone, Copy)]
struct ObjectPage {
    object: ObjectId,
    index: u32,
    frame: u64,
}

impl ObjectPage {
    const EMPTY: Self = Self {
        object: 0,
        index: 0,
        frame: 0,
    };
}

struct State {
    objects: [Object; MAX_OBJECTS],
    pages: [ObjectPage; MAX_PAGES],
}

impl State {
    const fn new() -> Self {
        Self {
            objects: [Object::EMPTY; MAX_OBJECTS],
            pages: [ObjectPage::EMPTY; MAX_PAGES],
        }
    }
}

struct LockedState {
    lock: AtomicBool,
    state: UnsafeCell<State>,
}

unsafe impl Sync for LockedState {}

static SHMEM: LockedState = LockedState {
    lock: AtomicBool::new(false),
    state: UnsafeCell::new(State::new()),
};

fn with_state<R>(function: impl FnOnce(&mut State) -> R) -> R {
    while SHMEM
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *SHMEM.state.get() });
    SHMEM.lock.store(false, Ordering::Release);
    result
}

pub fn initialize() {
    // State is too large for boot stack; clear arrays in place.
    with_state(|state| {
        state.objects.fill(Object::EMPTY);
        state.pages.fill(ObjectPage::EMPTY);
    });
    crate::serial_println!(
        "MAKOS_AARCH64_SHMEM_READY objects={} resident_pages={} max_object_mib={} named=posix lifecycle=unlink,handles,mappings",
        MAX_OBJECTS,
        MAX_PAGES,
        MAX_OBJECT_BYTES / (1024 * 1024),
    );
}

pub fn create(name: &[u8], mode: u32, uid: u32, gid: u32) -> Result<(), Error> {
    if name.is_empty() || name.len() > MAX_NAME_BYTES || name.contains(&b'/') {
        return Err(Error::Invalid);
    }
    with_state(|state| {
        if find_linked(state, name).is_some() {
            return Err(Error::Exists);
        }
        let index = state
            .objects
            .iter()
            .position(|object| !object.used)
            .ok_or(Error::NoSpace)?;
        let mut object = Object::EMPTY;
        object.used = true;
        object.linked = true;
        object.name[..name.len()].copy_from_slice(name);
        object.name_length = name.len() as u8;
        object.mode = 0o100000 | (mode & 0o777);
        object.uid = uid;
        object.gid = gid;
        object.modified_ticks = crate::arch::monotonic_ticks();
        state.objects[index] = object;
        Ok(())
    })
}

pub fn open(name: &[u8], readable: bool, writable: bool) -> Result<ObjectId, Error> {
    with_state(|state| {
        let index = find_linked(state, name).ok_or(Error::NotFound)?;
        let object = state.objects[index];
        if (readable && !crate::security::file_access(object.mode, object.uid, object.gid, false))
            || (writable
                && !crate::security::file_access(object.mode, object.uid, object.gid, true))
            || object.handle_refs == u32::MAX
        {
            return Err(Error::Permission);
        }
        state.objects[index].handle_refs += 1;
        Ok((index + 1) as ObjectId)
    })
}

pub fn unlink(name: &[u8]) -> Result<(), Error> {
    with_state(|state| {
        let index = find_linked(state, name).ok_or(Error::NotFound)?;
        let object = state.objects[index];
        if !crate::security::file_access(object.mode, object.uid, object.gid, true) {
            return Err(Error::Permission);
        }
        state.objects[index].linked = false;
        state.objects[index].name.fill(0);
        state.objects[index].name_length = 0;
        reap_if_unused(state, index);
        Ok(())
    })
}

pub fn release_handle(id: ObjectId) {
    with_state(|state| {
        let Some(index) = object_index(state, id) else {
            return;
        };
        state.objects[index].handle_refs = state.objects[index].handle_refs.saturating_sub(1);
        reap_if_unused(state, index);
    });
}

pub fn retain_mapping(id: ObjectId) -> bool {
    with_state(|state| {
        let Some(index) = object_index(state, id) else {
            return false;
        };
        if state.objects[index].mapping_refs == u32::MAX {
            return false;
        }
        state.objects[index].mapping_refs += 1;
        true
    })
}

pub fn release_mapping(id: ObjectId) {
    with_state(|state| {
        let Some(index) = object_index(state, id) else {
            return;
        };
        state.objects[index].mapping_refs = state.objects[index].mapping_refs.saturating_sub(1);
        reap_if_unused(state, index);
    });
}

pub fn metadata(id: ObjectId) -> Option<ObjectMetadata> {
    with_state(|state| {
        let index = object_index(state, id)?;
        let object = state.objects[index];
        Some(ObjectMetadata {
            mode: object.mode,
            uid: object.uid,
            gid: object.gid,
            size: object.size,
            modified_ticks: object.modified_ticks,
            inode: 0x5348_4d00 + index as u64,
        })
    })
}

pub fn metadata_named(name: &[u8]) -> Option<ObjectMetadata> {
    with_state(|state| {
        let index = find_linked(state, name)?;
        let object = state.objects[index];
        Some(ObjectMetadata {
            mode: object.mode,
            uid: object.uid,
            gid: object.gid,
            size: object.size,
            modified_ticks: object.modified_ticks,
            inode: 0x5348_4d00 + index as u64,
        })
    })
}

pub fn truncate(id: ObjectId, length: u64) -> Result<(), Error> {
    if length > MAX_OBJECT_BYTES {
        return Err(Error::TooLarge);
    }
    let result = with_state(|state| {
        let index = object_index(state, id).ok_or(Error::NotFound)?;
        let old_length = state.objects[index].size;
        if length < old_length {
            zero_truncated_tail(state, id, length);
            if state.objects[index].mapping_refs == 0 {
                release_pages_after(state, id, length);
            }
        }
        state.objects[index].size = length;
        state.objects[index].modified_ticks = crate::arch::monotonic_ticks();
        Ok(())
    });
    if result.is_err()
        || SHMEM_TRUNCATE_TRACES.fetch_add(1, Ordering::Relaxed) < SHMEM_TRUNCATE_TRACE_LIMIT
    {
        crate::serial_println!(
            "MAKOS_SHMEM_TRUNCATE object={} length={} result={:?}",
            id,
            length,
            result
        );
    }
    result
}

pub fn read(id: ObjectId, offset: u64, output: &mut [u8]) -> Result<usize, Error> {
    with_state(|state| {
        let object_index = object_index(state, id).ok_or(Error::NotFound)?;
        let size = state.objects[object_index].size;
        if offset >= size || output.is_empty() {
            return Ok(0);
        }
        let count = output.len().min((size - offset) as usize);
        output[..count].fill(0);
        copy_from_pages(state, id, offset, &mut output[..count]);
        Ok(count)
    })
}

pub fn write(id: ObjectId, offset: u64, input: &[u8]) -> Result<usize, Error> {
    let end = offset
        .checked_add(input.len() as u64)
        .ok_or(Error::TooLarge)?;
    with_state(|state| {
        let object_index = object_index(state, id).ok_or(Error::NotFound)?;
        if end > state.objects[object_index].size {
            return Err(Error::TooLarge);
        }
        let mut copied = 0usize;
        while copied < input.len() {
            let position = offset + copied as u64;
            let page_index = (position / PAGE_SIZE) as u32;
            let in_page = (position % PAGE_SIZE) as usize;
            let count = (PAGE_SIZE as usize - in_page).min(input.len() - copied);
            let frame = get_or_allocate_page(state, id, page_index)?;
            unsafe {
                ptr::copy_nonoverlapping(
                    input[copied..copied + count].as_ptr(),
                    (frame as *mut u8).add(in_page),
                    count,
                );
            }
            copied += count;
        }
        state.objects[object_index].modified_ticks = crate::arch::monotonic_ticks();
        Ok(copied)
    })
}

pub fn page_frame(id: ObjectId, byte_offset: u64) -> Option<u64> {
    with_state(|state| {
        let index = object_index(state, id)?;
        if byte_offset >= state.objects[index].size {
            return None;
        }
        let page_index = u32::try_from(byte_offset / PAGE_SIZE).ok()?;
        get_or_allocate_page(state, id, page_index).ok()
    })
}

fn get_or_allocate_page(state: &mut State, id: ObjectId, page_index: u32) -> Result<u64, Error> {
    if let Some(page) = state
        .pages
        .iter()
        .find(|page| page.object == id && page.index == page_index)
    {
        return Ok(page.frame);
    }
    let slot = state
        .pages
        .iter()
        .position(|page| page.object == 0)
        .ok_or(Error::NoSpace)?;
    let frame = crate::mm::allocate_frame().ok_or(Error::NoSpace)?;
    unsafe { ptr::write_bytes(frame as *mut u8, 0, PAGE_SIZE as usize) };
    state.pages[slot] = ObjectPage {
        object: id,
        index: page_index,
        frame,
    };
    Ok(frame)
}

fn copy_from_pages(state: &State, id: ObjectId, offset: u64, output: &mut [u8]) {
    let mut copied = 0usize;
    while copied < output.len() {
        let position = offset + copied as u64;
        let page_index = (position / PAGE_SIZE) as u32;
        let in_page = (position % PAGE_SIZE) as usize;
        let count = (PAGE_SIZE as usize - in_page).min(output.len() - copied);
        if let Some(page) = state
            .pages
            .iter()
            .find(|page| page.object == id && page.index == page_index)
        {
            unsafe {
                ptr::copy_nonoverlapping(
                    (page.frame as *const u8).add(in_page),
                    output[copied..copied + count].as_mut_ptr(),
                    count,
                );
            }
        }
        copied += count;
    }
}

fn zero_truncated_tail(state: &mut State, id: ObjectId, length: u64) {
    let first_page = length / PAGE_SIZE;
    let first_offset = (length % PAGE_SIZE) as usize;
    for page in state.pages.iter().filter(|page| page.object == id) {
        let page_number = u64::from(page.index);
        if page_number > first_page || (page_number == first_page && first_offset == 0) {
            unsafe { ptr::write_bytes(page.frame as *mut u8, 0, PAGE_SIZE as usize) };
        } else if page_number == first_page {
            unsafe {
                ptr::write_bytes(
                    (page.frame as *mut u8).add(first_offset),
                    0,
                    PAGE_SIZE as usize - first_offset,
                )
            };
        }
    }
}

fn release_pages_after(state: &mut State, id: ObjectId, length: u64) {
    let retained_pages = length.div_ceil(PAGE_SIZE);
    for page in &mut state.pages {
        if page.object == id && u64::from(page.index) >= retained_pages {
            release_frame(page.frame);
            *page = ObjectPage::EMPTY;
        }
    }
}

fn reap_if_unused(state: &mut State, index: usize) {
    let object = state.objects[index];
    if object.linked || object.handle_refs != 0 || object.mapping_refs != 0 {
        return;
    }
    let id = (index + 1) as ObjectId;
    for page in &mut state.pages {
        if page.object == id {
            release_frame(page.frame);
            *page = ObjectPage::EMPTY;
        }
    }
    state.objects[index] = Object::EMPTY;
}

fn release_frame(frame: u64) {
    if crate::mm::free_frame(frame).is_err() {
        crate::fatal("AArch64 shared-memory frame release failed");
    }
}

fn find_linked(state: &State, name: &[u8]) -> Option<usize> {
    state.objects.iter().position(|object| {
        object.used
            && object.linked
            && object.name_length as usize == name.len()
            && &object.name[..name.len()] == name
    })
}

fn object_index(state: &State, id: ObjectId) -> Option<usize> {
    let index = usize::from(id.checked_sub(1)?);
    state.objects.get(index).filter(|object| object.used)?;
    Some(index)
}
