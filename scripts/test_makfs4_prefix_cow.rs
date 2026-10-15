// Host fixture compiled by test_makfs4_prefix_cow.py. Production code is
// extracted verbatim; this file only supplies devices, time and test inputs.
// Injected failures always lead immediately to crash/remount. No assertion
// grants continued-I/O safety after an ambiguous failed root flush, indefinite
// old-root snapshot retention, or hardware-sector atomicity.
extern crate alloc;
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;

const BLOCKS: u64 = 262_144;
type Sectors = BTreeMap<u32, [u8; 512]>;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Op { Read, Write, Flush }
#[derive(Clone, Debug)]
struct Event { op: Op, lba: u32, success: bool }
#[derive(Clone, Default)]
struct Device {
    durable: Sectors,
    pending: Sectors,
    events: Vec<Event>,
    fault: Option<(Op, usize)>,
    seen: usize,
    tripped: bool,
    failed_flush_persists: bool,
}
static DEVICE: Mutex<Option<Device>> = Mutex::new(None);
fn device<R>(f: impl FnOnce(&mut Device) -> R) -> R {
    f(DEVICE.lock().unwrap().as_mut().unwrap())
}
impl Device {
    fn operation(&mut self, op: Op, lba: u32) -> bool {
        let fail = if let Some((wanted, count)) = self.fault {
            if op == wanted {
                self.seen += 1;
                self.seen == count
            } else { false }
        } else { false };
        self.tripped |= fail;
        self.events.push(Event { op, lba, success: !fail });
        !fail
    }
    fn persist(&mut self) { self.durable.append(&mut self.pending); }
}
mod block {
    pub struct DataDisk;
    impl DataDisk {
        pub fn identify_secondary() -> Option<Self> { Some(Self) }
        pub fn read_sector(&mut self, lba: u32, output: &mut [u8; 512]) -> bool {
            super::device(|disk| {
                if !disk.operation(super::Op::Read, lba) { return false; }
                *output = *disk.pending.get(&lba).or_else(|| disk.durable.get(&lba)).unwrap_or(&[0; 512]);
                true
            })
        }
        pub fn write_sector(&mut self, lba: u32, input: &[u8; 512]) -> bool {
            super::device(|disk| {
                if !disk.operation(super::Op::Write, lba) { return false; }
                disk.pending.insert(lba, *input);
                true
            })
        }
        #[cfg(target_arch = "aarch64")]
        pub fn read_sectors_8(&mut self, lba: u32, output: &mut [u8; 4096]) -> bool {
            for index in 0..8 {
                let mut sector = [0; 512];
                if !self.read_sector(lba + index, &mut sector) { return false; }
                output[index as usize * 512..(index as usize + 1) * 512].copy_from_slice(&sector);
            }
            true
        }
        #[cfg(target_arch = "aarch64")]
        pub fn write_sectors_8(&mut self, lba: u32, input: &[u8; 4096]) -> bool {
            for index in 0..8 {
                let sector = input[index as usize * 512..(index as usize + 1) * 512].try_into().unwrap();
                if !self.write_sector(lba + index, sector) { return false; }
            }
            true
        }
        pub fn flush(&mut self) -> bool {
            super::device(|disk| {
                let success = disk.operation(super::Op::Flush, 0);
                if success || disk.failed_flush_persists { disk.persist(); }
                success
            })
        }
    }
}
#[cfg(target_arch = "aarch64")]
mod aarch64_virtio_blk {
    pub fn service_requests_while_waiting() {} // Host fixture runs single-threaded.
}
fn current_unix_seconds() -> u64 { 1_700_000_000 }
include!("production.rs");

// Byte-level read/seek view for the REAL fsck implementation; missing sparse
// sectors read as zero. This adapter never opens any user/guest disk file.
struct Snapshot { sectors: Sectors, cursor: u64 }
impl Read for Snapshot {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.cursor >= BLOCKS * BLOCK_BYTES { return Ok(0); }
        let amount = output.len().min((BLOCKS * BLOCK_BYTES - self.cursor) as usize);
        for (index, byte) in output[..amount].iter_mut().enumerate() {
            let offset = self.cursor + index as u64;
            *byte = self.sectors.get(&((offset / 512) as u32))
                .map_or(0, |sector| sector[offset as usize % 512]);
        }
        self.cursor += amount as u64;
        Ok(amount)
    }
}
impl Seek for Snapshot {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        let next = match from {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.cursor) + i128::from(offset),
            SeekFrom::End(offset) => i128::from(BLOCKS * BLOCK_BYTES) + i128::from(offset),
        };
        self.cursor = u64::try_from(next).map_err(|_| std::io::ErrorKind::InvalidInput)?;
        Ok(self.cursor)
    }
}
fn fsck() {
    let sectors = device(|disk| disk.durable.clone());
    let report = makos_makfs4_fsck::check(&mut Snapshot { sectors, cursor: 0 }, BLOCKS * BLOCK_BYTES)
        .expect("real fsck must accept committed/remounted image");
    assert_eq!(report.generation, active_superblock().unwrap().generation);
}
fn remount(warm: bool) {
    with_inode_cache(|cache| *cache = InodeCache::EMPTY);
    let mut disk = DataDisk;
    let a = read_record(&mut disk, SUPERBLOCK_A_LBA).unwrap();
    let b = read_record(&mut disk, SUPERBLOCK_B_LBA).unwrap();
    let (root, _) = newest_superblock(validate_root(&mut disk, &a, BLOCKS),
                                    validate_root(&mut disk, &b, BLOCKS)).unwrap();
    activate(root);
    if warm { load_inode_cache(&mut disk, root).unwrap(); }
}
fn crash(warm: bool) {
    device(|disk| { disk.pending.clear(); disk.fault = None; });
    remount(warm);
    fsck();
}
fn map(extents: &[(u64, u32)]) -> ExtentMap {
    let mut result = ExtentMap::EMPTY;
    for &(start_block, block_count) in extents {
        result.push(makos_makfs4::Extent { start_block, block_count }).unwrap();
    }
    result
}
fn pattern(size: usize) -> Vec<u8> {
    (0..size).map(|index| ((index * 37 + index / 4096) % 251) as u8).collect()
}
fn seed(bytes: &[u8], extents: ExtentMap, blockers: ExtentMap, warm: bool) {
    *DEVICE.lock().unwrap() = Some(Device::default());
    let (bitmap_blocks, sets) = metadata_sets(BLOCKS).unwrap();
    let set = sets[0];
    let root = Superblock { generation: 1, commit_id: 1, block_count: BLOCKS,
        data_start: PACKAGE_LIMIT_BLOCK, catalog_block: set.catalog_block,
        bitmap_block: set.bitmap_block, bitmap_blocks };
    let mut directory = Inode::EMPTY;
    directory.inode = 1; directory.generation = 1; directory.parent = 1;
    directory.mode = 0o040700; directory.set_name(b".").unwrap();
    let mut file = Inode::EMPTY;
    file.inode = 2; file.generation = 1; file.parent = 1; file.mode = 0o100600;
    file.size = bytes.len() as u64; file.extents = extents; file.set_name(b"subject").unwrap();
    let mut entries = vec![directory, file];
    if blockers.blocks() != 0 {
        let mut other = file;
        other.inode = 3; other.size = blockers.blocks() * BLOCK_BYTES;
        other.extents = blockers; other.set_name(b"occupied").unwrap(); entries.push(other);
    }
    let catalog = Catalog { generation: 1, inode_count: entries.len() as u32,
        maximum_inodes: MAXIMUM_INODES, inode_table_block: set.inode_table_block,
        inode_table_blocks: INODE_TABLE_BLOCKS };
    let mut words = vec![0; ((BLOCKS - PACKAGE_LIMIT_BLOCK) / 64) as usize];
    let mut bitmap = BlockBitmap::new_at(&mut words, PACKAGE_LIMIT_BLOCK, BLOCKS - PACKAGE_LIMIT_BLOCK).unwrap();
    for inode in &entries { for extent in inode.extents.extents() { bitmap.reserve(*extent).unwrap(); } }
    let mut disk = DataDisk;
    for (index, inode) in entries.iter().enumerate() {
        write_inode_record(&mut disk, set.inode_table_block, index as u32, &inode.encode().unwrap()).unwrap();
    }
    for (offset, chunk) in words.chunks(512).enumerate() {
        let mut block = [0; 4096];
        for (index, word) in chunk.iter().enumerate() { block[index * 8..index * 8 + 8].copy_from_slice(&word.to_le_bytes()); }
        assert!(write_block(&mut disk, set.bitmap_block + offset as u64, &block));
    }
    for (index, chunk) in bytes.chunks(4096).enumerate() {
        let mut block = [0; 4096]; block[..chunk.len()].copy_from_slice(chunk);
        assert!(write_block(&mut disk, extents.file_block_to_device(index as u64).unwrap(), &block));
    }
    write_catalog_record(&mut disk, set.catalog_block, &catalog.encode(BLOCKS, PACKAGE_LIMIT_BLOCK).unwrap()).unwrap();
    let encoded = root.encode().unwrap();
    assert!(disk.write_sector(SUPERBLOCK_A_LBA, &encoded));
    assert!(disk.write_sector(SUPERBLOCK_B_LBA, &encoded));
    assert!(disk.flush());
    remount(warm); fsck(); clear_events();
}
fn clear_events() { device(|disk| { disk.events.clear(); disk.fault = None; disk.seen = 0; disk.tripped = false; }); }
fn current_inode() -> Inode {
    let mut disk = DataDisk;
    let root = active_superblock().unwrap();
    let catalog = read_catalog(&mut disk, root).unwrap();
    read_inode_from(&mut disk, catalog, 1).unwrap().unwrap()
}
fn contents() -> Vec<u8> {
    let inode = current_inode();
    let mut bytes = Vec::new();
    for index in 0..inode.extents.blocks() {
        let mut block = [0; 4096]; read_inode_block(&mut DataDisk, &inode, index, &mut block).unwrap();
        bytes.extend(block);
    }
    bytes.truncate(inode.size as usize); bytes
}
fn expected(mut old: Vec<u8>, size: Option<u64>, offset: u64, patch: &[u8]) -> Vec<u8> {
    let length = size.unwrap_or((old.len() as u64).max(offset + patch.len() as u64));
    old.resize(length as usize, 0);
    old[offset as usize..offset as usize + patch.len()].copy_from_slice(patch);
    old
}
fn data_events(events: &[Event], op: Op) -> usize {
    events.iter().filter(|e| e.op == op && e.lba as u64 >= PACKAGE_LIMIT_BLOCK * 8).count()
}
fn change(size: Option<u64>, offset: u64, patch: &[u8], kept: u64, written_blocks: usize) {
    let prior = current_inode();
    let old = contents();
    let wanted = expected(old, size, offset, patch);
    let prefix: Vec<_> = (0..kept).map(|index| {
        let physical = prior.extents.file_block_to_device(index).unwrap();
        let mut block = [0; 4096]; assert!(read_block(&mut DataDisk, physical, &mut block)); (physical, block)
    }).collect();
    clear_events();
    rewrite_inode(1, size, offset, patch).unwrap();
    let events = device(|disk| disk.events.clone());
    assert_eq!(data_events(&events, Op::Write), written_blocks * 8, "only planned COW suffix may be written");
    let next = current_inode();
    for (index, (physical, before)) in prefix.iter().enumerate() {
        assert_eq!(next.extents.file_block_to_device(index as u64), Some(*physical));
        let mut after = [0; 4096]; assert!(read_block(&mut DataDisk, *physical, &mut after));
        assert_eq!(&after, before, "retained prefix physical bytes must remain immutable");
    }
    assert_eq!(contents(), wanted);
    assert_commit_order(&events);
    crash(true);
    assert_eq!(contents(), wanted, "real remount must recover exact bytes");
}
fn assert_commit_order(events: &[Event]) {
    let flushes: Vec<_> = events.iter().enumerate().filter(|(_, e)| e.op == Op::Flush && e.success).map(|(i, _)| i).collect();
    assert_eq!(flushes.len(), 2, "metadata and root must both be flushed");
    let root_writes: Vec<_> = events.iter().enumerate().filter(|(_, e)| e.op == Op::Write &&
        (e.lba == SUPERBLOCK_A_LBA || e.lba == SUPERBLOCK_B_LBA)).map(|(i, _)| i).collect();
    assert_eq!(root_writes.len(), 1);
    assert!(flushes[0] < root_writes[0] && root_writes[0] < flushes[1]);
    assert!(events.iter().enumerate().filter(|(_, e)| e.op == Op::Write &&
        e.lba != SUPERBLOCK_A_LBA && e.lba != SUPERBLOCK_B_LBA).all(|(i, _)| i < flushes[0]));
}
fn functional() {
    for warm in [false, true] {
        let base = PACKAGE_LIMIT_BLOCK + 10;
        seed(&pattern(4 * 4096), map(&[(base, 4)]), ExtentMap::EMPTY, warm);
        change(None, 4 * 4096, b"append", 4, 1);
        change(None, 4 * 4096 + 6, &vec![7; 5000], 4, 2);
        let old_size = current_inode().size;
        change(None, old_size + 5000, b"hole-end", old_size / 4096,
               ((old_size + 5008).div_ceil(4096) - old_size / 4096) as usize);
        let before = current_inode().size;
        change(None, 4090, &vec![9; 20], 0, before.div_ceil(4096) as usize);
        change(Some(2 * 4096 + 17), 0, &[], 2, 1);
        change(Some(4 * 4096 + 29), 0, &[], 2, 3);
        change(Some(2 * 4096), 0, &[], 2, 0);
        change(Some(0), 0, &[], 0, 0);
    }
    // Deterministic data I/O: append to a 5 MiB file writes one block, reads
    // no old data. This excludes metadata traffic, not whole-call timing.
    seed(&pattern(1280 * 4096), map(&[(PACKAGE_LIMIT_BLOCK + 10, 1280)]), ExtentMap::EMPTY, true);
    clear_events(); rewrite_inode(1, None, 1280 * 4096, b"x").unwrap();
    let events = device(|disk| disk.events.clone());
    assert_eq!(data_events(&events, Op::Read), 0);
    assert_eq!(data_events(&events, Op::Write), 8);
    crash(true);
    // Exact full-block overwrite needs no read of that old block; untouched
    // later blocks still require copying because only prefixes are retained.
    let old = pattern(4 * 4096);
    seed(&old, map(&[(PACKAGE_LIMIT_BLOCK + 10, 4)]), ExtentMap::EMPTY, true);
    let patch = vec![0x91; 4096];
    clear_events(); rewrite_inode(1, None, 4096, &patch).unwrap();
    let events = device(|disk| disk.events.clone());
    assert_eq!(data_events(&events, Op::Read), 16, "full overwritten block must skip old read");
    assert_eq!(data_events(&events, Op::Write), 24);
    crash(true);
    assert_eq!(contents(), expected(old, None, 4096, &patch));
}
fn fallback_and_rollback() {
    // No physical space after prefix: whole-file fallback uses lower physical addresses.
    seed(&pattern(4 * 4096), map(&[(BLOCKS - 4, 4)]), ExtentMap::EMPTY, true);
    change(None, 4 * 4096, b"x", 0, 5);
    assert!(current_inode().extents.extents()[0].start_block < BLOCKS - 4);
    // Tail planner reserves two end-of-device blocks, then runs out. Those
    // reservations must roll back before the lower-address fallback commits.
    seed(&pattern(3 * 4096), map(&[(BLOCKS - 5, 3)]), ExtentMap::EMPTY, true);
    let old_bitmap = active_superblock().unwrap().bitmap_block;
    let bitmap_before: Vec<_> = (old_bitmap * 8..(old_bitmap + 4) * 8)
        .map(|lba| device(|disk| *disk.durable.get(&(lba as u32)).unwrap_or(&[0; 512]))).collect();
    change(None, 3 * 4096, &vec![0x92; 3 * 4096], 0, 6);
    let bitmap_after: Vec<_> = (old_bitmap * 8..(old_bitmap + 4) * 8)
        .map(|lba| device(|disk| *disk.durable.get(&(lba as u32)).unwrap_or(&[0; 512]))).collect();
    assert_eq!(bitmap_after, bitmap_before, "fallback must not overwrite old active bitmap");
    // A 14-extent prefix cannot append a discontiguous fifteenth extent.
    let pieces: Vec<_> = (0..14).map(|i| (PACKAGE_LIMIT_BLOCK + 100 + i * 2, 1)).collect();
    let old_map = map(&pieces);
    let blocker = map(&[(PACKAGE_LIMIT_BLOCK + 127, 1)]);
    seed(&pattern(14 * 4096), old_map, blocker, false);
    change(None, 14 * 4096, b"x", 0, 15);
    assert_eq!(current_inode().extents.count(), 1);
    // Core allocation rollback after provisional reservations and a failed
    // 15th extent. Verify every bit, including retained prefix ownership.
    let mut words = [0u64; 4];
    {
        let mut bitmap = BlockBitmap::new_at(&mut words, PACKAGE_LIMIT_BLOCK, 256).unwrap();
        for extent in old_map.extents() { bitmap.reserve(*extent).unwrap(); }
        for extent in blocker.extents() { bitmap.reserve(*extent).unwrap(); }
    }
    let before = words;
    assert_eq!(BlockBitmap::new_at(&mut words, PACKAGE_LIMIT_BLOCK, 256).unwrap()
        .allocate_after_prefix(old_map, 1), Err(FormatError::TooFragmented));
    assert_eq!(words, before, "failed plan must roll back reservations only");
    // No fallback space: failure preserves durable content and bitmap.
    seed(&pattern(4096), map(&[(PACKAGE_LIMIT_BLOCK, 1)]),
         map(&[(PACKAGE_LIMIT_BLOCK + 1, (BLOCKS - PACKAGE_LIMIT_BLOCK - 1) as u32)]), true);
    let before = device(|disk| disk.durable.clone());
    assert_eq!(rewrite_inode(1, None, 4096, b"x"), Err(MountError::NoSpace));
    assert_eq!(device(|disk| disk.durable.clone()), before);
    crash(true); assert_eq!(contents(), pattern(4096));
}
fn rotating_warm_commits() {
    // No remount/cache reset between successful commits: cover incremental
    // dirty tracking across every rotating metadata target, twice or more.
    let mut wanted = pattern(4 * 4096);
    seed(&wanted, map(&[(PACKAGE_LIMIT_BLOCK + 10, 4)]), ExtentMap::EMPTY, true);
    let changes = [
        (None, 4 * 4096, b"append".to_vec()),
        (None, 4 * 4096 + 6, vec![0x73; 5000]),
        (None, 7 * 4096 + 31, b"hole-end".to_vec()),
        (None, 4090, vec![0x29; 20]),
        (Some(2 * 4096 + 17), 0, vec![]),
        (Some(5 * 4096 + 29), 0, vec![]),
        (None, 4096, vec![0x83; 4096]),
        (None, 6 * 4096, b"final".to_vec()),
        (Some(3 * 4096 + 33), 0, vec![]),
    ];
    let (_, sets) = metadata_sets(BLOCKS).unwrap();
    let mut visits = [0; 3];
    for (ordinal, (size, offset, patch)) in changes.into_iter().enumerate() {
        let prior = current_inode();
        let next_bytes = expected(wanted, size, offset, &patch);
        let unchanged = prior.size.min(next_bytes.len() as u64);
        let kept = if patch.is_empty() { unchanged } else { unchanged.min(offset) } / BLOCK_BYTES;
        let prefix: Vec<_> = (0..kept).map(|index| {
            let physical = prior.extents.file_block_to_device(index).unwrap();
            let mut bytes = [0; 4096];
            assert!(read_block(&mut DataDisk, physical, &mut bytes));
            (physical, bytes)
        }).collect();
        clear_events();
        rewrite_inode(1, size, offset, &patch).unwrap();
        let events = device(|disk| disk.events.clone());
        assert_commit_order(&events);
        let root = active_superblock().unwrap();
        assert_eq!(root.generation, ordinal as u64 + 2);
        let target = sets.iter().position(|set| set.catalog_block == root.catalog_block).unwrap();
        visits[target] += 1;
        let next = current_inode();
        with_inode_cache(|cache| {
            assert!(cache.ready, "successful warm commits must preserve ready incremental cache");
            assert_eq!(cache.generation, root.generation);
            assert_eq!(cache.inode_table_block, sets[target].inode_table_block);
            assert_eq!(cache.entries[1].unwrap(), next);
            assert_eq!(cache.inode_dirty[target], [0; INODE_DIRTY_WORDS]);
            assert_eq!(cache.bitmap_dirty[target], 0);
            for other in 0..3 {
                if other != target {
                    assert_ne!(cache.inode_dirty[other][0] & 2, 0);
                }
            }
        });
        for (index, (physical, before)) in prefix.iter().enumerate() {
            assert_eq!(next.extents.file_block_to_device(index as u64), Some(*physical));
            assert!(!events.iter().any(|event| event.op == Op::Write &&
                (*physical * 8..*physical * 8 + 8).contains(&u64::from(event.lba))));
            let mut after = [0; 4096];
            assert!(read_block(&mut DataDisk, *physical, &mut after));
            assert_eq!(&after, before);
        }
        assert_eq!(contents(), next_bytes);
        fsck();
        wanted = next_bytes;
    }
    assert!(visits.iter().all(|count| *count >= 2), "all three metadata targets must rotate repeatedly");
    crash(true);
    assert_eq!(contents(), wanted);
    println!("MAKOS_MAKFS4_PREFIX_COW_ROTATION_OK uninterrupted_commits=9 targets=3 each_target_min=2 cache=warm-incremental final=crash-remount-fsck-exact");
}
fn failure_matrix() {
    let old = pattern(3 * 4096 + 123);
    let offset = old.len() as u64;
    let patch = vec![0x81; 5000];
    let wanted = expected(old.clone(), None, offset, &patch);
    seed(&old, map(&[(PACKAGE_LIMIT_BLOCK + 10, 4)]), ExtentMap::EMPTY, true);
    let baseline = device(|disk| disk.durable.clone());
    let old_inode = current_inode();
    let old_bitmap = active_superblock().unwrap().bitmap_block;
    let preserved_lbas: Vec<_> = (0..3).flat_map(|index| {
        let physical = old_inode.extents.file_block_to_device(index).unwrap();
        physical * 8..physical * 8 + 8
    }).chain(old_bitmap * 8..(old_bitmap + 4) * 8).collect();
    clear_events(); rewrite_inode(1, None, offset, &patch).unwrap();
    let success = device(|disk| disk.events.clone());
    assert_commit_order(&success);
    let mut injected = 0;
    for op in [Op::Read, Op::Write, Op::Flush] {
        let count = success.iter().filter(|event| event.op == op).count();
        for nth in 1..=count {
            for persist_failed_flush in [false, true] {
                if op != Op::Flush && persist_failed_flush { continue; }
                *DEVICE.lock().unwrap() = Some(Device { durable: baseline.clone(), ..Device::default() });
                remount(true); clear_events();
                device(|disk| { disk.fault = Some((op, nth)); disk.failed_flush_persists = persist_failed_flush; });
                let outcome = rewrite_inode(1, None, offset, &patch);
                assert!(device(|disk| disk.tripped), "failure boundary must be exercised {op:?}/{nth}");
                let events = device(|disk| disk.events.clone());
                assert_eq!(events.iter().filter(|event| !event.success).count(), 1);
                for &lba in &preserved_lbas {
                    assert!(!events.iter().any(|event| event.op == Op::Write && event.lba == lba as u32),
                        "no failure path may even submit a retained-prefix/active-bitmap write");
                    let observed = device(|disk| *disk.pending.get(&(lba as u32))
                        .or_else(|| disk.durable.get(&(lba as u32))).unwrap_or(&[0; 512]));
                    assert_eq!(observed, *baseline.get(&(lba as u32)).unwrap_or(&[0; 512]),
                        "old prefix and active bitmap immutable at every injected failure");
                }
                crash(true);
                let recovered = contents();
                assert!(recovered == old || recovered == wanted,
                    "failure must remount one complete generation, {op:?}/{nth}");
                if outcome.is_ok() { assert_eq!(recovered, wanted); }
                injected += 1;
            }
        }
    }
    println!("MAKOS_MAKFS4_PREFIX_COW_FAILURE_MATRIX_OK injected={} model=volatile-write,flush-durable failed_flush=discard-or-persist remount=production-root-validation fsck=production prefix,active_bitmap=immutable-every-injection", injected);
}
fn main() {
    functional();
    fallback_and_rollback();
    rotating_warm_commits();
    failure_matrix();
    println!("MAKOS_MAKFS4_PREFIX_COW_BEHAVIOR_OK append=aligned,unaligned hole=zero overwrite=cross-block,full-block-no-old-read truncate=shrink,regrow,zero fallback=backward-space,partial-tail-rollback,14-extents rollback=bitmap prefix=immutable data_io_5mib_append=read0,write1block cache=cold,warm commit=real");
}
