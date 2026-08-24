#![no_std]

pub const PAGE_SIZE: u64 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FreeError {
    OutOfRange,
    AlreadyFree,
    Unaligned,
}

/// First-fit physical frame allocator. Bit 1 means unavailable/allocated;
/// bit 0 means free. Initialization starts fully reserved, then releases only
/// firmware ranges explicitly proven usable.
pub struct FrameAllocator<'a> {
    words: &'a mut [u64],
    frame_count: usize,
    free_count: usize,
    next_word: usize,
}

impl<'a> FrameAllocator<'a> {
    pub fn new(words: &'a mut [u64]) -> Self {
        words.fill(u64::MAX);
        Self {
            frame_count: words.len().saturating_mul(64),
            words,
            free_count: 0,
            next_word: 0,
        }
    }

    pub const fn frame_count(&self) -> usize {
        self.frame_count
    }

    pub const fn free_count(&self) -> usize {
        self.free_count
    }

    pub const fn managed_bytes(&self) -> u64 {
        self.frame_count as u64 * PAGE_SIZE
    }

    /// Release page-aligned range during boot-map ingestion. Overlap is safe:
    /// frames already free are not counted twice.
    pub fn release_region(&mut self, physical_start: u64, page_count: u64) -> usize {
        let first = physical_start / PAGE_SIZE;
        let end = first
            .saturating_add(page_count)
            .min(self.frame_count as u64);
        let mut released = 0;
        for frame in first..end {
            let index = frame as usize;
            if self.is_used(index) {
                self.set_used(index, false);
                released += 1;
            }
        }
        self.free_count += released;
        self.next_word = self.next_word.min(first as usize / 64);
        released
    }

    pub fn reserve_region(&mut self, physical_start: u64, page_count: u64) -> usize {
        let first = physical_start / PAGE_SIZE;
        let end = first
            .saturating_add(page_count)
            .min(self.frame_count as u64);
        let mut reserved = 0;
        for frame in first..end {
            let index = frame as usize;
            if !self.is_used(index) {
                self.set_used(index, true);
                reserved += 1;
            }
        }
        self.free_count = self.free_count.saturating_sub(reserved);
        reserved
    }

    pub fn allocate(&mut self) -> Option<u64> {
        if self.free_count == 0 {
            return None;
        }
        for pass in 0..2 {
            let start = if pass == 0 { self.next_word } else { 0 };
            let end = if pass == 0 {
                self.words.len()
            } else {
                self.next_word
            };
            for word_index in start..end {
                let available = !self.words[word_index];
                if available == 0 {
                    continue;
                }
                let bit = available.trailing_zeros() as usize;
                let frame = word_index * 64 + bit;
                if frame >= self.frame_count {
                    continue;
                }
                self.words[word_index] |= 1u64 << bit;
                self.free_count -= 1;
                self.next_word = word_index;
                return Some(frame as u64 * PAGE_SIZE);
            }
        }
        None
    }

    /// Allocate a physically contiguous run. Intended for small DMA buffers;
    /// preserves first-fit behavior and never returns a partially reserved run.
    pub fn allocate_contiguous(&mut self, count: usize) -> Option<u64> {
        if count == 0 || count > self.free_count || count > self.frame_count {
            return None;
        }
        let mut run_start = 0usize;
        let mut run_length = 0usize;
        for frame in 0..self.frame_count {
            if self.is_used(frame) {
                run_length = 0;
                continue;
            }
            if run_length == 0 {
                run_start = frame;
            }
            run_length += 1;
            if run_length == count {
                for reserved in run_start..run_start + count {
                    self.set_used(reserved, true);
                }
                self.free_count -= count;
                self.next_word = run_start / 64;
                return Some(run_start as u64 * PAGE_SIZE);
            }
        }
        None
    }

    pub fn free(&mut self, physical_address: u64) -> Result<(), FreeError> {
        if physical_address % PAGE_SIZE != 0 {
            return Err(FreeError::Unaligned);
        }
        let frame =
            usize::try_from(physical_address / PAGE_SIZE).map_err(|_| FreeError::OutOfRange)?;
        if frame >= self.frame_count {
            return Err(FreeError::OutOfRange);
        }
        if !self.is_used(frame) {
            return Err(FreeError::AlreadyFree);
        }
        self.set_used(frame, false);
        self.free_count += 1;
        self.next_word = self.next_word.min(frame / 64);
        Ok(())
    }

    fn is_used(&self, frame: usize) -> bool {
        self.words[frame / 64] & (1u64 << (frame % 64)) != 0
    }

    fn set_used(&mut self, frame: usize, used: bool) {
        let mask = 1u64 << (frame % 64);
        if used {
            self.words[frame / 64] |= mask;
        } else {
            self.words[frame / 64] &= !mask;
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn allocates_only_released_frames() {
        let mut bits = [0u64; 2];
        let mut allocator = FrameAllocator::new(&mut bits);
        assert_eq!(allocator.release_region(3 * PAGE_SIZE, 2), 2);
        assert_eq!(allocator.allocate(), Some(3 * PAGE_SIZE));
        assert_eq!(allocator.allocate(), Some(4 * PAGE_SIZE));
        assert_eq!(allocator.allocate(), None);
    }

    #[test]
    fn reserve_removes_free_frames() {
        let mut bits = [0u64; 1];
        let mut allocator = FrameAllocator::new(&mut bits);
        allocator.release_region(PAGE_SIZE, 3);
        assert_eq!(allocator.reserve_region(2 * PAGE_SIZE, 1), 1);
        assert_eq!(allocator.allocate(), Some(PAGE_SIZE));
        assert_eq!(allocator.allocate(), Some(3 * PAGE_SIZE));
        assert_eq!(allocator.allocate(), None);
    }

    #[test]
    fn free_reuses_and_rejects_invalid_input() {
        let mut bits = [0u64; 1];
        let mut allocator = FrameAllocator::new(&mut bits);
        allocator.release_region(8 * PAGE_SIZE, 1);
        let frame = allocator.allocate().unwrap();
        assert_eq!(allocator.free(frame), Ok(()));
        assert_eq!(allocator.free(frame), Err(FreeError::AlreadyFree));
        assert_eq!(allocator.free(frame + 1), Err(FreeError::Unaligned));
        assert_eq!(allocator.allocate(), Some(frame));
    }

    #[test]
    fn overlapping_release_does_not_double_count() {
        let mut bits = [0u64; 1];
        let mut allocator = FrameAllocator::new(&mut bits);
        assert_eq!(allocator.release_region(PAGE_SIZE, 4), 4);
        assert_eq!(allocator.release_region(2 * PAGE_SIZE, 2), 0);
        assert_eq!(allocator.free_count(), 4);
    }

    #[test]
    fn contiguous_allocation_skips_fragmented_ranges() {
        let mut bits = [0u64; 2];
        let mut allocator = FrameAllocator::new(&mut bits);
        allocator.release_region(PAGE_SIZE, 3);
        allocator.release_region(8 * PAGE_SIZE, 4);
        allocator.reserve_region(2 * PAGE_SIZE, 1);
        assert_eq!(allocator.allocate_contiguous(3), Some(8 * PAGE_SIZE));
        assert_eq!(allocator.free_count(), 3);
    }
}
