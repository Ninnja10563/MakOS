#![no_std]

pub const BLOCK_BYTES: u64 = 4096;
pub const RECORD_BYTES: usize = 512;
pub const MAX_PATH_BYTES: usize = 255;
pub const MAX_EXTENTS: usize = 14;
pub const FORMAT_VERSION: u32 = 4;

/// Translate one filesystem block index to its first device sector without
/// truncating non-integral geometries or wrapping large block numbers.
pub const fn block_first_sector(block: u64, sector_bytes: u64) -> Option<u64> {
    if sector_bytes == 0 || BLOCK_BYTES % sector_bytes != 0 {
        return None;
    }
    block.checked_mul(BLOCK_BYTES / sector_bytes)
}

const SUPER_MAGIC: [u8; 8] = *b"MAKFS004";
const INODE_MAGIC: [u8; 8] = *b"MAKINOD4";
const CATALOG_MAGIC: [u8; 8] = *b"MAKCAT04";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Invalid,
    Corrupt,
    NoSpace,
    TooFragmented,
    OutOfRange,
    WrongPhase,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct Extent {
    pub start_block: u64,
    pub block_count: u32,
}

impl Extent {
    pub const EMPTY: Self = Self {
        start_block: 0,
        block_count: 0,
    };

    pub fn end_block(self) -> Option<u64> {
        self.start_block.checked_add(u64::from(self.block_count))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtentMap {
    extents: [Extent; MAX_EXTENTS],
    count: u8,
}

impl ExtentMap {
    pub const EMPTY: Self = Self {
        extents: [Extent::EMPTY; MAX_EXTENTS],
        count: 0,
    };

    pub fn extents(&self) -> &[Extent] {
        &self.extents[..usize::from(self.count)]
    }

    pub const fn count(&self) -> usize {
        self.count as usize
    }

    pub fn blocks(&self) -> u64 {
        self.extents()
            .iter()
            .map(|extent| u64::from(extent.block_count))
            .sum()
    }

    pub fn push(&mut self, extent: Extent) -> Result<(), Error> {
        let end = extent.end_block().ok_or(Error::OutOfRange)?;
        if extent.block_count == 0 {
            return Err(Error::Invalid);
        }
        if let Some(last) = self.extents().last().copied() {
            let last_end = last.end_block().ok_or(Error::OutOfRange)?;
            if extent.start_block < last_end {
                return Err(Error::Invalid);
            }
            if extent.start_block == last_end {
                let merged = u64::from(last.block_count) + u64::from(extent.block_count);
                let merged = u32::try_from(merged).map_err(|_| Error::OutOfRange)?;
                self.extents[usize::from(self.count) - 1].block_count = merged;
                return Ok(());
            }
        }
        if usize::from(self.count) == MAX_EXTENTS || end < extent.start_block {
            return Err(Error::TooFragmented);
        }
        self.extents[usize::from(self.count)] = extent;
        self.count += 1;
        Ok(())
    }

    pub fn file_block_to_device(&self, file_block: u64) -> Option<u64> {
        let mut remaining = file_block;
        for extent in self.extents() {
            if remaining < u64::from(extent.block_count) {
                return extent.start_block.checked_add(remaining);
            }
            remaining -= u64::from(extent.block_count);
        }
        None
    }
}

pub struct BlockBitmap<'a> {
    words: &'a mut [u64],
    base_block: u64,
    block_count: u64,
}

impl<'a> BlockBitmap<'a> {
    pub fn new(words: &'a mut [u64], block_count: u64) -> Result<Self, Error> {
        Self::new_at(words, 0, block_count)
    }

    pub fn new_at(words: &'a mut [u64], base_block: u64, block_count: u64) -> Result<Self, Error> {
        let capacity = u64::try_from(words.len())
            .ok()
            .and_then(|words| words.checked_mul(64))
            .ok_or(Error::OutOfRange)?;
        if block_count == 0
            || block_count > capacity
            || base_block.checked_add(block_count).is_none()
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            words,
            base_block,
            block_count,
        })
    }

    pub const fn base_block(&self) -> u64 {
        self.base_block
    }

    pub const fn block_count(&self) -> u64 {
        self.block_count
    }

    pub fn allocated(&self, block: u64) -> Result<bool, Error> {
        let relative = block
            .checked_sub(self.base_block)
            .filter(|relative| *relative < self.block_count)
            .ok_or(Error::OutOfRange)?;
        let word = usize::try_from(relative / 64).map_err(|_| Error::OutOfRange)?;
        Ok(self.words[word] & (1u64 << (relative % 64)) != 0)
    }

    pub fn reserve(&mut self, extent: Extent) -> Result<(), Error> {
        let end = extent.end_block().ok_or(Error::OutOfRange)?;
        if extent.block_count == 0
            || extent.start_block < self.base_block
            || end > self.base_block + self.block_count
        {
            return Err(Error::OutOfRange);
        }
        for block in extent.start_block..end {
            if self.allocated(block)? {
                return Err(Error::Invalid);
            }
        }
        for block in extent.start_block..end {
            self.set(block, true)?;
        }
        Ok(())
    }

    pub fn release(&mut self, extent: Extent) -> Result<(), Error> {
        let end = extent.end_block().ok_or(Error::OutOfRange)?;
        if extent.block_count == 0
            || extent.start_block < self.base_block
            || end > self.base_block + self.block_count
        {
            return Err(Error::OutOfRange);
        }
        for block in extent.start_block..end {
            if !self.allocated(block)? {
                return Err(Error::Invalid);
            }
        }
        for block in extent.start_block..end {
            self.set(block, false)?;
        }
        Ok(())
    }

    /// First-fit contiguous allocation. Metadata can chain fourteen extents;
    /// allocator never silently overlaps or partially commits.
    pub fn allocate(&mut self, block_count: u32) -> Result<Extent, Error> {
        if block_count == 0 || u64::from(block_count) > self.block_count {
            return Err(Error::Invalid);
        }
        let wanted = u64::from(block_count);
        let mut run_start = self.base_block;
        let mut run_length = 0;
        for block in self.base_block..self.base_block + self.block_count {
            if self.allocated(block)? {
                run_length = 0;
                run_start = block.saturating_add(1);
            } else {
                run_length += 1;
                if run_length == wanted {
                    let extent = Extent {
                        start_block: run_start,
                        block_count,
                    };
                    self.reserve(extent)?;
                    return Ok(extent);
                }
            }
        }
        Err(Error::NoSpace)
    }

    fn set(&mut self, block: u64, allocated: bool) -> Result<(), Error> {
        let relative = block
            .checked_sub(self.base_block)
            .filter(|relative| *relative < self.block_count)
            .ok_or(Error::OutOfRange)?;
        let word = usize::try_from(relative / 64).map_err(|_| Error::OutOfRange)?;
        let mask = 1u64 << (relative % 64);
        if allocated {
            self.words[word] |= mask;
        } else {
            self.words[word] &= !mask;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Superblock {
    pub generation: u64,
    pub commit_id: u64,
    pub block_count: u64,
    pub data_start: u64,
    pub catalog_block: u64,
    pub bitmap_block: u64,
    pub bitmap_blocks: u32,
}

impl Superblock {
    pub fn validate(self) -> Result<Self, Error> {
        let bitmap_end = self
            .bitmap_block
            .checked_add(u64::from(self.bitmap_blocks))
            .ok_or(Error::Corrupt)?;
        if self.generation == 0
            || self.block_count == 0
            || self.bitmap_blocks == 0
            || self.data_start >= self.block_count
            || self.catalog_block >= self.block_count
            || bitmap_end > self.block_count
            || self.catalog_block >= self.bitmap_block && self.catalog_block < bitmap_end
        {
            return Err(Error::Corrupt);
        }
        Ok(self)
    }

    pub fn encode(self) -> Result<[u8; RECORD_BYTES], Error> {
        self.validate()?;
        let mut output = [0u8; RECORD_BYTES];
        output[..8].copy_from_slice(&SUPER_MAGIC);
        put_u32(&mut output, 8, FORMAT_VERSION);
        put_u64(&mut output, 16, self.generation);
        put_u64(&mut output, 24, self.commit_id);
        put_u64(&mut output, 32, self.block_count);
        put_u64(&mut output, 40, self.data_start);
        put_u64(&mut output, 48, self.catalog_block);
        put_u64(&mut output, 56, self.bitmap_block);
        put_u32(&mut output, 64, self.bitmap_blocks);
        seal(&mut output);
        Ok(output)
    }

    pub fn decode(input: &[u8; RECORD_BYTES]) -> Result<Self, Error> {
        if input[..8] != SUPER_MAGIC || get_u32(input, 8) != FORMAT_VERSION || !sealed(input) {
            return Err(Error::Corrupt);
        }
        Self {
            generation: get_u64(input, 16),
            commit_id: get_u64(input, 24),
            block_count: get_u64(input, 32),
            data_start: get_u64(input, 40),
            catalog_block: get_u64(input, 48),
            bitmap_block: get_u64(input, 56),
            bitmap_blocks: get_u32(input, 64),
        }
        .validate()
    }
}

pub fn newest_superblock(
    first: Result<Superblock, Error>,
    second: Result<Superblock, Error>,
) -> Result<(Superblock, usize), Error> {
    match (first, second) {
        (Ok(first), Ok(second)) if second.generation > first.generation => Ok((second, 1)),
        (Ok(first), Ok(_)) | (Ok(first), Err(_)) => Ok((first, 0)),
        (Err(_), Ok(second)) => Ok((second, 1)),
        (Err(_), Err(_)) => Err(Error::Corrupt),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Inode {
    pub inode: u64,
    pub generation: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub modified_ns: u64,
    /// Unix seconds. Zero identifies an inode written before the compatible
    /// timestamp extension and is interpreted from `modified_ns` by VFS.
    pub accessed_seconds: u32,
    /// Unix seconds of the last inode/content change. See `accessed_seconds`.
    pub changed_seconds: u32,
    pub parent: u64,
    path: [u8; MAX_PATH_BYTES],
    path_length: u8,
    pub extents: ExtentMap,
}

impl Inode {
    pub const EMPTY: Self = Self {
        inode: 0,
        generation: 0,
        mode: 0,
        uid: 0,
        gid: 0,
        size: 0,
        modified_ns: 0,
        accessed_seconds: 0,
        changed_seconds: 0,
        parent: 0,
        path: [0; MAX_PATH_BYTES],
        path_length: 0,
        extents: ExtentMap::EMPTY,
    };

    pub fn set_name(&mut self, name: &[u8]) -> Result<(), Error> {
        if name.is_empty()
            || name.len() > MAX_PATH_BYTES
            || name.iter().any(|byte| *byte == 0 || *byte == b'/')
        {
            return Err(Error::Invalid);
        }
        self.path.fill(0);
        self.path[..name.len()].copy_from_slice(name);
        self.path_length = name.len() as u8;
        Ok(())
    }

    pub fn name(&self) -> &[u8] {
        &self.path[..usize::from(self.path_length)]
    }

    pub fn validate(&self) -> Result<(), Error> {
        let capacity = self
            .extents
            .blocks()
            .checked_mul(BLOCK_BYTES)
            .ok_or(Error::OutOfRange)?;
        if self.inode == 0
            || self.generation == 0
            || self.name().is_empty()
            || self.name().iter().any(|byte| *byte == 0 || *byte == b'/')
            || self.size > capacity
        {
            return Err(Error::Corrupt);
        }
        Ok(())
    }

    pub fn validate_on_volume(&self, data_start: u64, block_count: u64) -> Result<(), Error> {
        self.validate()?;
        if data_start >= block_count
            || self.extents.extents().iter().any(|extent| {
                extent.start_block < data_start
                    || extent.end_block().is_none_or(|end| end > block_count)
            })
        {
            return Err(Error::Corrupt);
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<[u8; RECORD_BYTES], Error> {
        self.validate()?;
        let mut output = [0u8; RECORD_BYTES];
        output[..8].copy_from_slice(&INODE_MAGIC);
        put_u32(&mut output, 8, FORMAT_VERSION);
        put_u64(&mut output, 16, self.inode);
        put_u64(&mut output, 24, self.generation);
        put_u32(&mut output, 32, self.mode);
        put_u32(&mut output, 36, self.uid);
        put_u32(&mut output, 40, self.gid);
        // These fields consume bytes reserved by the original v4 record.
        // Existing v4 images contain zero here and remain valid.
        put_u32(&mut output, 44, self.accessed_seconds);
        put_u64(&mut output, 48, self.size);
        put_u64(&mut output, 56, self.modified_ns);
        put_u64(&mut output, 64, self.parent);
        output[72] = self.path_length;
        output[73] = self.extents.count;
        put_u32(&mut output, 76, self.changed_seconds);
        output[80..80 + self.name().len()].copy_from_slice(self.name());
        for (index, extent) in self.extents.extents().iter().enumerate() {
            let offset = 336 + index * 12;
            put_u64(&mut output, offset, extent.start_block);
            put_u32(&mut output, offset + 8, extent.block_count);
        }
        seal(&mut output);
        Ok(output)
    }

    pub fn decode(input: &[u8; RECORD_BYTES]) -> Result<Self, Error> {
        if input[..8] != INODE_MAGIC
            || get_u32(input, 8) != FORMAT_VERSION
            || !sealed(input)
            || usize::from(input[72]) > MAX_PATH_BYTES
            || usize::from(input[73]) > MAX_EXTENTS
        {
            return Err(Error::Corrupt);
        }
        let mut inode = Self {
            inode: get_u64(input, 16),
            generation: get_u64(input, 24),
            mode: get_u32(input, 32),
            uid: get_u32(input, 36),
            gid: get_u32(input, 40),
            accessed_seconds: get_u32(input, 44),
            size: get_u64(input, 48),
            modified_ns: get_u64(input, 56),
            parent: get_u64(input, 64),
            changed_seconds: get_u32(input, 76),
            path: [0; MAX_PATH_BYTES],
            path_length: input[72],
            extents: ExtentMap::EMPTY,
        };
        let name_length = usize::from(inode.path_length);
        inode.path[..name_length].copy_from_slice(&input[80..80 + name_length]);
        for index in 0..usize::from(input[73]) {
            let offset = 336 + index * 12;
            inode.extents.push(Extent {
                start_block: get_u64(input, offset),
                block_count: get_u32(input, offset + 8),
            })?;
        }
        inode.validate()?;
        Ok(inode)
    }
}

/// In-memory hash index for directory child lookup. On-disk inode records stay
/// authoritative; rebuilding after mount or metadata commit is deterministic.
#[derive(Clone, Copy)]
pub struct DirectoryIndex<const INODES: usize, const BUCKETS: usize> {
    heads: [u16; BUCKETS],
    next: [u16; INODES],
}

impl<const INODES: usize, const BUCKETS: usize> DirectoryIndex<INODES, BUCKETS> {
    pub const EMPTY: Self = Self {
        heads: [0; BUCKETS],
        next: [0; INODES],
    };

    pub fn rebuild(&mut self, entries: &[Option<Inode>; INODES]) -> Result<(), Error> {
        if BUCKETS == 0 || INODES > usize::from(u16::MAX) {
            return Err(Error::OutOfRange);
        }
        self.heads.fill(0);
        self.next.fill(0);
        for (index, inode) in entries.iter().enumerate() {
            let Some(inode) = inode else {
                continue;
            };
            let bucket = child_hash(inode.parent, inode.name()) % BUCKETS;
            self.next[index] = self.heads[bucket];
            self.heads[bucket] = u16::try_from(index + 1).map_err(|_| Error::OutOfRange)?;
        }
        Ok(())
    }

    pub fn find(&self, entries: &[Option<Inode>; INODES], parent: u64, name: &[u8]) -> Option<u32> {
        if BUCKETS == 0 {
            return None;
        }
        let mut encoded = self.heads[child_hash(parent, name) % BUCKETS];
        while encoded != 0 {
            let index = usize::from(encoded - 1);
            let inode = entries.get(index).copied().flatten()?;
            if inode.parent == parent && inode.name() == name {
                return u32::try_from(index).ok();
            }
            encoded = *self.next.get(index)?;
        }
        None
    }
}

fn child_hash(parent: u64, name: &[u8]) -> usize {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in parent.to_le_bytes().iter().chain(name) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash as usize
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Catalog {
    pub generation: u64,
    pub inode_count: u32,
    pub maximum_inodes: u32,
    pub inode_table_block: u64,
    pub inode_table_blocks: u32,
}

impl Catalog {
    pub fn validate(self, block_count: u64, data_start: u64) -> Result<Self, Error> {
        let end = self
            .inode_table_block
            .checked_add(u64::from(self.inode_table_blocks))
            .ok_or(Error::Corrupt)?;
        let record_capacity = u64::from(self.inode_table_blocks)
            .checked_mul(BLOCK_BYTES / RECORD_BYTES as u64)
            .ok_or(Error::Corrupt)?;
        if self.generation == 0
            || self.maximum_inodes == 0
            || self.inode_count > self.maximum_inodes
            || u64::from(self.maximum_inodes) > record_capacity
            || self.inode_table_blocks == 0
            || end > block_count
            || end > data_start
        {
            return Err(Error::Corrupt);
        }
        Ok(self)
    }

    pub fn encode(self, block_count: u64, data_start: u64) -> Result<[u8; RECORD_BYTES], Error> {
        self.validate(block_count, data_start)?;
        let mut output = [0u8; RECORD_BYTES];
        output[..8].copy_from_slice(&CATALOG_MAGIC);
        put_u32(&mut output, 8, FORMAT_VERSION);
        put_u64(&mut output, 16, self.generation);
        put_u32(&mut output, 24, self.inode_count);
        put_u32(&mut output, 28, self.maximum_inodes);
        put_u64(&mut output, 32, self.inode_table_block);
        put_u32(&mut output, 40, self.inode_table_blocks);
        seal(&mut output);
        Ok(output)
    }

    pub fn decode(
        input: &[u8; RECORD_BYTES],
        block_count: u64,
        data_start: u64,
    ) -> Result<Self, Error> {
        if input[..8] != CATALOG_MAGIC || get_u32(input, 8) != FORMAT_VERSION || !sealed(input) {
            return Err(Error::Corrupt);
        }
        Self {
            generation: get_u64(input, 16),
            inode_count: get_u32(input, 24),
            maximum_inodes: get_u32(input, 28),
            inode_table_block: get_u64(input, 32),
            inode_table_blocks: get_u32(input, 40),
        }
        .validate(block_count, data_start)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitPhase {
    Data,
    Inodes,
    Bitmap,
    Catalog,
    FlushMetadata,
    Superblock,
    FlushRoot,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitSequencer {
    phase: CommitPhase,
}

impl CommitSequencer {
    pub const fn new() -> Self {
        Self {
            phase: CommitPhase::Data,
        }
    }

    pub const fn phase(self) -> CommitPhase {
        self.phase
    }

    pub fn complete(&mut self, completed: CommitPhase) -> Result<CommitPhase, Error> {
        if completed != self.phase || self.phase == CommitPhase::Complete {
            return Err(Error::WrongPhase);
        }
        self.phase = match self.phase {
            CommitPhase::Data => CommitPhase::Inodes,
            CommitPhase::Inodes => CommitPhase::Bitmap,
            CommitPhase::Bitmap => CommitPhase::Catalog,
            CommitPhase::Catalog => CommitPhase::FlushMetadata,
            CommitPhase::FlushMetadata => CommitPhase::Superblock,
            CommitPhase::Superblock => CommitPhase::FlushRoot,
            CommitPhase::FlushRoot => CommitPhase::Complete,
            CommitPhase::Complete => return Err(Error::WrongPhase),
        };
        Ok(self.phase)
    }
}

impl Default for CommitSequencer {
    fn default() -> Self {
        Self::new()
    }
}

fn seal(record: &mut [u8; RECORD_BYTES]) {
    let checksum = crc32(&record[..RECORD_BYTES - 4]);
    put_u32(record, RECORD_BYTES - 4, checksum);
}

fn sealed(record: &[u8; RECORD_BYTES]) -> bool {
    crc32(&record[..RECORD_BYTES - 4]) == get_u32(record, RECORD_BYTES - 4)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn put_u32(output: &mut [u8], offset: usize, value: u32) {
    output[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(output: &mut [u8], offset: usize, value: u64) {
    output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn get_u32(input: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(input[offset..offset + 4].try_into().unwrap_or([0; 4]))
}

fn get_u64(input: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(input[offset..offset + 8].try_into().unwrap_or([0; 8]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_sector_geometry_is_exact_and_checked() {
        assert_eq!(block_first_sector(0, 512), Some(0));
        assert_eq!(block_first_sector(1, 512), Some(8));
        assert_eq!(block_first_sector(17, 512), Some(136));
        assert_eq!(block_first_sector(1, 0), None);
        assert_eq!(block_first_sector(1, 1000), None);
        assert_eq!(block_first_sector(u64::MAX, 512), None);
    }

    #[test]
    fn allocator_reserves_and_reuses_without_overlap() {
        let mut words = [0u64; 2];
        let mut bitmap = BlockBitmap::new(&mut words, 100).unwrap();
        let first = bitmap.allocate(7).unwrap();
        let second = bitmap.allocate(3).unwrap();
        assert_eq!(
            first,
            Extent {
                start_block: 0,
                block_count: 7
            }
        );
        assert_eq!(
            second,
            Extent {
                start_block: 7,
                block_count: 3
            }
        );
        bitmap.release(first).unwrap();
        assert_eq!(bitmap.allocate(5).unwrap().start_block, 0);

        let mut offset_words = [0u64; 1];
        let mut offset = BlockBitmap::new_at(&mut offset_words, 500, 64).unwrap();
        assert_eq!(offset.allocate(4).unwrap().start_block, 500);
        assert_eq!(offset.allocated(499), Err(Error::OutOfRange));
    }

    #[test]
    fn extents_merge_and_map_file_blocks() {
        let mut map = ExtentMap::EMPTY;
        map.push(Extent {
            start_block: 20,
            block_count: 2,
        })
        .unwrap();
        map.push(Extent {
            start_block: 22,
            block_count: 3,
        })
        .unwrap();
        map.push(Extent {
            start_block: 40,
            block_count: 1,
        })
        .unwrap();
        assert_eq!(map.count(), 2);
        assert_eq!(map.file_block_to_device(4), Some(24));
        assert_eq!(map.file_block_to_device(5), Some(40));
        assert_eq!(map.file_block_to_device(6), None);
    }

    #[test]
    fn inode_round_trip_rejects_corruption() {
        let mut inode = Inode {
            inode: 7,
            generation: 3,
            mode: 0o100600,
            uid: 1000,
            gid: 1000,
            size: 6000,
            modified_ns: 42,
            accessed_seconds: 1_700_000_001,
            changed_seconds: 1_700_000_002,
            parent: 2,
            ..Inode::EMPTY
        };
        inode.set_name(b"places.sqlite").unwrap();
        inode
            .extents
            .push(Extent {
                start_block: 200,
                block_count: 2,
            })
            .unwrap();
        let encoded = inode.encode().unwrap();
        assert_eq!(Inode::decode(&encoded).unwrap(), inode);
        let mut corrupt = encoded;
        corrupt[48] ^= 1;
        assert_eq!(Inode::decode(&corrupt), Err(Error::Corrupt));
    }

    #[test]
    fn inode_timestamp_extension_accepts_legacy_zero_reserved_fields() {
        let mut inode = Inode {
            inode: 9,
            generation: 1,
            mode: 0o120777,
            uid: 1000,
            gid: 1000,
            size: 4,
            modified_ns: 50,
            ..Inode::EMPTY
        };
        inode.set_name(b"link").unwrap();
        inode
            .extents
            .push(Extent {
                start_block: 300,
                block_count: 1,
            })
            .unwrap();
        let decoded = Inode::decode(&inode.encode().unwrap()).unwrap();
        assert_eq!(decoded.accessed_seconds, 0);
        assert_eq!(decoded.changed_seconds, 0);
        assert_eq!(decoded.mode & 0o170000, 0o120000);
    }

    #[test]
    fn directory_index_finds_many_siblings_and_handles_collisions() {
        let mut entries = [None; 96];
        for (index, slot) in entries.iter_mut().enumerate().skip(1).take(80) {
            let mut inode = Inode {
                inode: index as u64 + 1,
                generation: 1,
                mode: 0o100600,
                parent: if index < 70 { 1 } else { 2 },
                ..Inode::EMPTY
            };
            let mut name = [b'x'; MAX_PATH_BYTES];
            name[0] = b'a' + (index % 26) as u8;
            name[1] = b'0' + ((index / 10) % 10) as u8;
            name[2] = b'0' + (index % 10) as u8;
            inode
                .set_name(&name[..if index == 69 { MAX_PATH_BYTES } else { 3 }])
                .unwrap();
            *slot = Some(inode);
        }

        // One bucket forces every entry through collision-chain handling.
        let mut index = DirectoryIndex::<96, 1>::EMPTY;
        index.rebuild(&entries).unwrap();
        assert_eq!(index.find(&entries, 1, b"r43"), Some(43));
        assert_eq!(index.find(&entries, 2, b"s70"), Some(70));
        assert_eq!(index.find(&entries, 1, &[b'x'; MAX_PATH_BYTES]), None);
        assert_eq!(index.find(&entries, 1, b"missing"), None);

        entries[43] = None;
        index.rebuild(&entries).unwrap();
        assert_eq!(index.find(&entries, 1, b"r43"), None);
    }

    #[test]
    fn newest_valid_superblock_wins() {
        let older = Superblock {
            generation: 4,
            commit_id: 9,
            block_count: 1000,
            data_start: 100,
            catalog_block: 10,
            bitmap_block: 20,
            bitmap_blocks: 2,
        };
        let newer = Superblock {
            generation: 5,
            ..older
        };
        assert_eq!(newest_superblock(Ok(older), Ok(newer)), Ok((newer, 1)));
        assert_eq!(Superblock::decode(&newer.encode().unwrap()).unwrap(), newer);
    }

    #[test]
    fn commit_order_requires_both_flushes() {
        let mut sequence = CommitSequencer::new();
        for phase in [
            CommitPhase::Data,
            CommitPhase::Inodes,
            CommitPhase::Bitmap,
            CommitPhase::Catalog,
            CommitPhase::FlushMetadata,
            CommitPhase::Superblock,
            CommitPhase::FlushRoot,
        ] {
            sequence.complete(phase).unwrap();
        }
        assert_eq!(sequence.phase(), CommitPhase::Complete);
        assert_eq!(
            sequence.complete(CommitPhase::Complete),
            Err(Error::WrongPhase)
        );
    }
}
