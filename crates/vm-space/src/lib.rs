#![no_std]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Region {
    pub owner: u64,
    pub base: u64,
    pub pages: usize,
    pub protection: u8,
}

impl Region {
    const EMPTY: Self = Self {
        owner: 0,
        base: 0,
        pages: 0,
        protection: 0,
    };

    pub const fn used(self) -> bool {
        self.owner != 0
    }

    pub fn end(self, page_size: u64) -> Option<u64> {
        self.base
            .checked_add((self.pages as u64).checked_mul(page_size)?)
    }
}

pub struct RegionTable<const N: usize> {
    entries: [Region; N],
}

impl<const N: usize> RegionTable<N> {
    pub const fn new() -> Self {
        Self {
            entries: [Region::EMPTY; N],
        }
    }

    pub fn clear(&mut self) {
        for entry in &mut self.entries {
            *entry = Region::EMPTY;
        }
    }

    pub fn allocate_first_fit(
        &mut self,
        owner: u64,
        pages: usize,
        protection: u8,
        arena_base: u64,
        arena_limit: u64,
        page_size: u64,
    ) -> Option<u64> {
        if owner == 0
            || pages == 0
            || page_size == 0
            || !page_size.is_power_of_two()
            || arena_base & (page_size - 1) != 0
            || arena_limit <= arena_base
        {
            return None;
        }
        let bytes = (pages as u64).checked_mul(page_size)?;
        let slot = self.entries.iter().position(|entry| !entry.used())?;
        let mut candidate = arena_base;
        loop {
            let end = candidate.checked_add(bytes)?;
            if end > arena_limit {
                return None;
            }
            let overlap = self
                .entries
                .iter()
                .copied()
                .filter(|entry| entry.used() && entry.owner == owner)
                .filter(|entry| {
                    let entry_end = entry.end(page_size).unwrap_or(u64::MAX);
                    candidate < entry_end && entry.base < end
                })
                .min_by_key(|entry| entry.base);
            if let Some(overlap) = overlap {
                candidate = align_up(overlap.end(page_size)?, page_size)?;
                continue;
            }
            self.entries[slot] = Region {
                owner,
                base: candidate,
                pages,
                protection,
            };
            return Some(candidate);
        }
    }

    pub fn allocate_fixed(
        &mut self,
        owner: u64,
        base: u64,
        pages: usize,
        protection: u8,
        arena_base: u64,
        arena_limit: u64,
        page_size: u64,
    ) -> bool {
        if owner == 0
            || pages == 0
            || page_size == 0
            || !page_size.is_power_of_two()
            || base & (page_size - 1) != 0
            || base < arena_base
        {
            return false;
        }
        let Some(end) = base.checked_add(pages as u64 * page_size) else {
            return false;
        };
        if end > arena_limit
            || self.entries.iter().any(|entry| {
                entry.used()
                    && entry.owner == owner
                    && base < entry.end(page_size).unwrap_or(u64::MAX)
                    && entry.base < end
            })
        {
            return false;
        }
        let Some(slot) = self.entries.iter_mut().find(|entry| !entry.used()) else {
            return false;
        };
        *slot = Region {
            owner,
            base,
            pages,
            protection,
        };
        true
    }

    pub fn remove(&mut self, owner: u64, base: u64, pages: usize, page_size: u64) -> bool {
        let Some((index, region, first, after)) = self.containing(owner, base, pages, page_size)
        else {
            return false;
        };
        let left_pages = first;
        let right_pages = region.pages - after;
        let right_slot = if left_pages != 0 && right_pages != 0 {
            self.entries
                .iter()
                .enumerate()
                .find(|(candidate, entry)| *candidate != index && !entry.used())
                .map(|(candidate, _)| candidate)
        } else {
            Some(index)
        };
        let Some(right_slot) = right_slot else {
            return false;
        };
        self.entries[index] = if left_pages == 0 {
            Region::EMPTY
        } else {
            Region {
                pages: left_pages,
                ..region
            }
        };
        if right_pages != 0 {
            self.entries[right_slot] = Region {
                base: base + pages as u64 * page_size,
                pages: right_pages,
                ..region
            };
        }
        true
    }

    pub fn protect(
        &mut self,
        owner: u64,
        base: u64,
        pages: usize,
        protection: u8,
        page_size: u64,
    ) -> bool {
        let Some((index, region, first, after)) = self.containing(owner, base, pages, page_size)
        else {
            return false;
        };
        let left_pages = first;
        let right_pages = region.pages - after;
        let extras = usize::from(left_pages != 0) + usize::from(right_pages != 0);
        let mut free = [usize::MAX; 2];
        let mut found = 0;
        for (candidate, entry) in self.entries.iter().enumerate() {
            if candidate != index && !entry.used() && found < extras {
                free[found] = candidate;
                found += 1;
            }
        }
        if found != extras {
            return false;
        }
        self.entries[index] = Region {
            owner,
            base,
            pages,
            protection,
        };
        let mut next = 0;
        if left_pages != 0 {
            self.entries[free[next]] = Region {
                pages: left_pages,
                ..region
            };
            next += 1;
        }
        if right_pages != 0 {
            self.entries[free[next]] = Region {
                base: base + pages as u64 * page_size,
                pages: right_pages,
                ..region
            };
        }
        true
    }

    pub fn find(&self, owner: u64, address: u64, page_size: u64) -> Option<Region> {
        self.entries.iter().copied().find(|entry| {
            entry.used()
                && entry.owner == owner
                && address >= entry.base
                && address < entry.end(page_size).unwrap_or(entry.base)
        })
    }

    pub fn forget(&mut self, owner: u64) -> (usize, usize) {
        let mut regions = 0;
        let mut pages = 0;
        for entry in &mut self.entries {
            if entry.used() && entry.owner == owner {
                regions += 1;
                pages += entry.pages;
                *entry = Region::EMPTY;
            }
        }
        (regions, pages)
    }

    pub fn count(&self, owner: u64) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.used() && entry.owner == owner)
            .count()
    }

    pub fn free_slots(&self) -> usize {
        self.entries.iter().filter(|entry| !entry.used()).count()
    }

    /// Clone every region owned by `source` for `destination` atomically.
    /// Address-space frames remain architecture-owned; this copies VMA policy.
    pub fn clone_owner(&mut self, source: u64, destination: u64) -> bool {
        if source == 0
            || destination == 0
            || source == destination
            || self.entries.iter().any(|entry| entry.owner == destination)
        {
            return false;
        }
        let needed = self
            .entries
            .iter()
            .filter(|entry| entry.used() && entry.owner == source)
            .count();
        if self.free_slots() < needed {
            return false;
        }
        for index in 0..N {
            let source_entry = self.entries[index];
            if !source_entry.used() || source_entry.owner != source {
                continue;
            }
            let slot = self
                .entries
                .iter_mut()
                .find(|entry| !entry.used())
                .expect("region clone preflight mismatch");
            *slot = Region {
                owner: destination,
                ..source_entry
            };
        }
        true
    }

    fn containing(
        &self,
        owner: u64,
        base: u64,
        pages: usize,
        page_size: u64,
    ) -> Option<(usize, Region, usize, usize)> {
        if owner == 0 || pages == 0 || page_size == 0 || base & (page_size - 1) != 0 {
            return None;
        }
        let end = base.checked_add((pages as u64).checked_mul(page_size)?)?;
        let (index, region) = self
            .entries
            .iter()
            .copied()
            .enumerate()
            .find(|(_, entry)| {
                entry.used()
                    && entry.owner == owner
                    && base >= entry.base
                    && end <= entry.end(page_size).unwrap_or(entry.base)
            })?;
        let first = ((base - region.base) / page_size) as usize;
        Some((index, region, first, first + pages))
    }
}

impl<const N: usize> Default for RegionTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: u64 = 4096;

    #[test]
    fn first_fit_is_per_owner_and_reuses_holes() {
        let mut table = RegionTable::<8>::new();
        let first = table
            .allocate_first_fit(1, 4, 3, 0x2000_0000, 0x2100_0000, PAGE)
            .unwrap();
        let second = table
            .allocate_first_fit(1, 2, 1, 0x2000_0000, 0x2100_0000, PAGE)
            .unwrap();
        assert_eq!(first, 0x2000_0000);
        assert_eq!(second, first + 4 * PAGE);
        assert_eq!(
            table.allocate_first_fit(2, 8, 1, 0x2000_0000, 0x2100_0000, PAGE),
            Some(0x2000_0000)
        );
        assert!(table.remove(1, first, 4, PAGE));
        assert_eq!(
            table.allocate_first_fit(1, 1, 1, 0x2000_0000, 0x2100_0000, PAGE),
            Some(first)
        );
    }

    #[test]
    fn middle_unmap_splits_without_losing_protection() {
        let mut table = RegionTable::<4>::new();
        let base = table
            .allocate_first_fit(7, 8, 3, 0x1000, 0x20_000, PAGE)
            .unwrap();
        assert!(table.remove(7, base + 2 * PAGE, 3, PAGE));
        assert_eq!(table.count(7), 2);
        assert_eq!(table.find(7, base, PAGE).unwrap().pages, 2);
        let right = table.find(7, base + 5 * PAGE, PAGE).unwrap();
        assert_eq!(right.pages, 3);
        assert_eq!(right.protection, 3);
        assert!(table.find(7, base + 3 * PAGE, PAGE).is_none());
    }

    #[test]
    fn subrange_protect_builds_three_regions() {
        let mut table = RegionTable::<4>::new();
        let base = table
            .allocate_first_fit(1, 10, 3, 0x1000, 0x20_000, PAGE)
            .unwrap();
        assert!(table.protect(1, base + 3 * PAGE, 4, 5, PAGE));
        assert_eq!(table.count(1), 3);
        assert_eq!(table.find(1, base + 4 * PAGE, PAGE).unwrap().protection, 5);
        assert_eq!(table.find(1, base, PAGE).unwrap().protection, 3);
        assert_eq!(table.find(1, base + 8 * PAGE, PAGE).unwrap().protection, 3);
        assert_eq!(table.forget(1), (3, 10));
    }

    #[test]
    fn split_is_atomic_when_table_has_no_capacity() {
        let mut table = RegionTable::<2>::new();
        let base = table
            .allocate_first_fit(1, 8, 3, 0x1000, 0x20_000, PAGE)
            .unwrap();
        table
            .allocate_first_fit(2, 1, 1, 0x1000, 0x20_000, PAGE)
            .unwrap();
        assert!(!table.protect(1, base + 2 * PAGE, 2, 1, PAGE));
        assert_eq!(table.find(1, base + 3 * PAGE, PAGE).unwrap().protection, 3);
    }

    #[test]
    fn fixed_reservation_rejects_overlap_and_bounds() {
        let mut table = RegionTable::<3>::new();
        assert!(table.allocate_fixed(4, 0x4000, 2, 1, 0x1000, 0x10_000, PAGE));
        assert!(!table.allocate_fixed(4, 0x5000, 1, 1, 0x1000, 0x10_000, PAGE));
        assert!(table.allocate_fixed(5, 0x5000, 1, 1, 0x1000, 0x10_000, PAGE));
        assert!(!table.allocate_fixed(4, 0, 1, 1, 0x1000, 0x10_000, PAGE));
        assert!(!table.allocate_fixed(4, 0xf000, 2, 1, 0x1000, 0x10_000, PAGE));
    }

    #[test]
    fn firefox_jit_reservation_is_sparse_metadata_and_can_be_protected() {
        const JIT_BYTES: u64 = 2044 * 1024 * 1024;
        const JIT_PAGES: usize = (JIT_BYTES / PAGE) as usize;
        const MMAP_BASE: u64 = 0x8000_0000;
        const MMAP_LIMIT: u64 = 0x3c000_0000;

        let mut table = RegionTable::<4>::new();
        let base = table
            .allocate_first_fit(9, JIT_PAGES, 0, MMAP_BASE, MMAP_LIMIT, PAGE)
            .unwrap();
        assert_eq!(base, MMAP_BASE);
        assert_eq!(table.find(9, base, PAGE).unwrap().pages, JIT_PAGES);

        let executable_chunk = base + 1024 * PAGE;
        assert!(table.protect(9, executable_chunk, 16, 5, PAGE));
        assert_eq!(table.count(9), 3);
        assert_eq!(table.find(9, executable_chunk, PAGE).unwrap().protection, 5);
        assert_eq!(table.forget(9), (3, JIT_PAGES));
    }
}
