#![no_std]

pub const SECTOR_BYTES: usize = 512;
pub const CATALOG_SECTORS: u64 = 4;
pub const MAX_PACKAGES: usize = 8;
pub const MAX_DEPENDENCIES: usize = 3;
pub const MAX_NAME_BYTES: usize = 32;
pub const MAX_VERSION_BYTES: usize = 16;
pub const DEPENDENCY_FORMAT_MAGIC: &[u8; 8] = b"MAKDEP1\0";
/// Production region: 384 MiB..512 MiB on current 1 GiB MakOS data volume.
pub const PRODUCTION_BASE_SECTOR: u64 = 786_432;
pub const PRODUCTION_SLOT_SECTORS: u64 = 131_072;
pub const PRODUCTION_END_SECTOR: u64 = PRODUCTION_BASE_SECTOR + 2 * PRODUCTION_SLOT_SECTORS;

const FORMAT_VERSION: u32 = 1;
const HEADER_MAGIC: [u8; 8] = *b"MAKPTS01";
const HEADER_PREPARING: u32 = 1;
const HEADER_COMMITTED: u32 = 2;
const ENTRY_BYTES: usize = 256;
const DEPENDENCY_BYTES: usize = 51;
const PAYLOAD_START_SECTOR: u64 = 1 + CATALOG_SECTORS;

pub trait SectorDevice {
    fn sector_count(&self) -> u64;
    fn read_sector(&mut self, sector: u64, output: &mut [u8; SECTOR_BYTES]) -> bool;
    fn write_sector(&mut self, sector: u64, input: &[u8; SECTOR_BYTES]) -> bool;
    fn flush(&mut self) -> bool;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Io,
    InvalidGeometry,
    InvalidName,
    InvalidVersion,
    InvalidDependency,
    DependencyMissing,
    DependencyVersion,
    DependencyCycle,
    DependedOn,
    AlreadyInstalled,
    NotInstalled,
    NoSpace,
    Corrupt,
    OutputTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequirementKind {
    Exact,
    AtLeast,
}

#[derive(Clone, Copy, Debug)]
pub struct Dependency<'a> {
    pub name: &'a [u8],
    pub kind: RequirementKind,
    pub version: &'a [u8],
}

impl Dependency<'_> {
    pub const EMPTY: Self = Self {
        name: &[],
        kind: RequirementKind::Exact,
        version: &[],
    };
}

#[derive(Clone, Copy, Debug)]
pub struct Manifest<'a> {
    pub name: &'a [u8],
    pub version: &'a [u8],
    pub dependencies: &'a [Dependency<'a>],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Installed {
    pub generation: u64,
    pub version: [u8; MAX_VERSION_BYTES],
    pub version_length: u8,
    pub payload_length: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoreState {
    pub generation: u64,
    pub package_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageInfo {
    pub name: [u8; MAX_NAME_BYTES],
    pub name_length: u8,
    pub version: [u8; MAX_VERSION_BYTES],
    pub version_length: u8,
    pub payload_length: u64,
    /// Device-relative first sector, valid for current activated snapshot.
    pub payload_first_sector: u64,
}

impl PackageInfo {
    pub const EMPTY: Self = Self {
        name: [0; MAX_NAME_BYTES],
        name_length: 0,
        version: [0; MAX_VERSION_BYTES],
        version_length: 0,
        payload_length: 0,
        payload_first_sector: 0,
    };

    pub fn name(&self) -> &[u8] {
        &self.name[..usize::from(self.name_length)]
    }

    pub fn version(&self) -> &[u8] {
        &self.version[..usize::from(self.version_length)]
    }
}

impl Installed {
    pub fn version(&self) -> &[u8] {
        &self.version[..usize::from(self.version_length)]
    }
}

#[derive(Clone, Copy)]
struct Entry {
    used: bool,
    name: [u8; MAX_NAME_BYTES],
    name_length: u8,
    version: [u8; MAX_VERSION_BYTES],
    version_length: u8,
    dependencies: [StoredDependency; MAX_DEPENDENCIES],
    dependency_count: u8,
    payload_offset: u64,
    payload_length: u64,
    payload_crc: u32,
}

impl Entry {
    const EMPTY: Self = Self {
        used: false,
        name: [0; MAX_NAME_BYTES],
        name_length: 0,
        version: [0; MAX_VERSION_BYTES],
        version_length: 0,
        dependencies: [StoredDependency::EMPTY; MAX_DEPENDENCIES],
        dependency_count: 0,
        payload_offset: 0,
        payload_length: 0,
        payload_crc: 0,
    };

    fn name(&self) -> &[u8] {
        &self.name[..usize::from(self.name_length)]
    }

    fn version(&self) -> &[u8] {
        &self.version[..usize::from(self.version_length)]
    }
}

#[derive(Clone, Copy)]
struct StoredDependency {
    name: [u8; MAX_NAME_BYTES],
    name_length: u8,
    kind: RequirementKind,
    version: [u8; MAX_VERSION_BYTES],
    version_length: u8,
}

impl StoredDependency {
    const EMPTY: Self = Self {
        name: [0; MAX_NAME_BYTES],
        name_length: 0,
        kind: RequirementKind::Exact,
        version: [0; MAX_VERSION_BYTES],
        version_length: 0,
    };

    fn name(&self) -> &[u8] {
        &self.name[..usize::from(self.name_length)]
    }

    fn version(&self) -> &[u8] {
        &self.version[..usize::from(self.version_length)]
    }
}

#[derive(Clone, Copy)]
struct Snapshot {
    slot: u8,
    generation: u64,
    entries: [Entry; MAX_PACKAGES],
    count: usize,
}

impl Snapshot {
    const EMPTY: Self = Self {
        slot: 1,
        generation: 0,
        entries: [Entry::EMPTY; MAX_PACKAGES],
        count: 0,
    };
}

#[derive(Clone, Copy)]
struct Header {
    generation: u64,
    count: usize,
    catalog_crc: u32,
    state: u32,
}

pub struct Store<D> {
    device: D,
    base_sector: u64,
    slot_sectors: u64,
}

impl<D: SectorDevice> Store<D> {
    pub fn open(device: D, base_sector: u64, slot_sectors: u64) -> Result<Self, Error> {
        if slot_sectors <= PAYLOAD_START_SECTOR
            || base_sector
                .checked_add(slot_sectors.checked_mul(2).ok_or(Error::InvalidGeometry)?)
                .is_none_or(|end| end > device.sector_count())
        {
            return Err(Error::InvalidGeometry);
        }
        Ok(Self {
            device,
            base_sector,
            slot_sectors,
        })
    }

    pub fn into_inner(self) -> D {
        self.device
    }

    pub fn installed(&mut self, name: &[u8]) -> Result<Option<Installed>, Error> {
        validate_name(name)?;
        let snapshot = self.recover()?;
        Ok(find_entry(&snapshot, name).map(|entry| Installed {
            generation: snapshot.generation,
            version: entry.version,
            version_length: entry.version_length,
            payload_length: entry.payload_length,
        }))
    }

    pub fn state(&mut self) -> Result<StoreState, Error> {
        let snapshot = self.recover()?;
        Ok(StoreState {
            generation: snapshot.generation,
            package_count: snapshot.count,
        })
    }

    pub fn package(&mut self, index: usize) -> Result<Option<PackageInfo>, Error> {
        let snapshot = self.recover()?;
        let Some(entry) = snapshot
            .entries
            .get(index)
            .filter(|_| index < snapshot.count)
        else {
            return Ok(None);
        };
        Ok(Some(PackageInfo {
            name: entry.name,
            name_length: entry.name_length,
            version: entry.version,
            version_length: entry.version_length,
            payload_length: entry.payload_length,
            payload_first_sector: self.payload_sector(snapshot.slot, entry.payload_offset)?,
        }))
    }

    pub fn packages(
        &mut self,
        output: &mut [PackageInfo; MAX_PACKAGES],
    ) -> Result<StoreState, Error> {
        output.fill(PackageInfo::EMPTY);
        let snapshot = self.recover()?;
        for (destination, entry) in output.iter_mut().zip(&snapshot.entries[..snapshot.count]) {
            *destination = PackageInfo {
                name: entry.name,
                name_length: entry.name_length,
                version: entry.version,
                version_length: entry.version_length,
                payload_length: entry.payload_length,
                payload_first_sector: self.payload_sector(snapshot.slot, entry.payload_offset)?,
            };
        }
        Ok(StoreState {
            generation: snapshot.generation,
            package_count: snapshot.count,
        })
    }

    pub fn read_payload(&mut self, name: &[u8], output: &mut [u8]) -> Result<usize, Error> {
        validate_name(name)?;
        let snapshot = self.recover()?;
        let entry = *find_entry(&snapshot, name).ok_or(Error::NotInstalled)?;
        let length = usize::try_from(entry.payload_length).map_err(|_| Error::OutputTooSmall)?;
        if output.len() < length {
            return Err(Error::OutputTooSmall);
        }
        self.read_payload_bytes(snapshot.slot, &entry, &mut output[..length])?;
        Ok(length)
    }

    pub fn read_payload_at(
        &mut self,
        name: &[u8],
        offset: u64,
        output: &mut [u8],
    ) -> Result<usize, Error> {
        validate_name(name)?;
        let snapshot = self.recover()?;
        let entry = *find_entry(&snapshot, name).ok_or(Error::NotInstalled)?;
        if offset > entry.payload_length {
            return Err(Error::OutputTooSmall);
        }
        let count = output
            .len()
            .min(usize::try_from(entry.payload_length - offset).unwrap_or(usize::MAX));
        let mut copied = 0usize;
        while copied < count {
            let absolute = entry
                .payload_offset
                .checked_add(offset)
                .and_then(|value| value.checked_add(copied as u64))
                .ok_or(Error::Corrupt)?;
            let aligned = absolute / SECTOR_BYTES as u64 * SECTOR_BYTES as u64;
            let within = (absolute % SECTOR_BYTES as u64) as usize;
            let mut sector = [0u8; SECTOR_BYTES];
            self.read(self.payload_sector(snapshot.slot, aligned)?, &mut sector)?;
            let chunk = (count - copied).min(SECTOR_BYTES - within);
            output[copied..copied + chunk].copy_from_slice(&sector[within..within + chunk]);
            copied += chunk;
        }
        Ok(count)
    }

    pub fn install(&mut self, manifest: Manifest<'_>, payload: &[u8]) -> Result<u64, Error> {
        validate_manifest(&manifest)?;
        if payload.is_empty() {
            return Err(Error::NoSpace);
        }
        let active = self.recover()?;
        if find_entry(&active, manifest.name).is_some() {
            return Err(Error::AlreadyInstalled);
        }
        if active.count == MAX_PACKAGES {
            return Err(Error::NoSpace);
        }
        let mut next = active;
        next.slot ^= 1;
        next.generation = active.generation.checked_add(1).ok_or(Error::NoSpace)?;
        next.entries[next.count] = entry_from_manifest(&manifest, payload.len() as u64);
        next.count += 1;
        validate_dependencies(&next)?;
        self.commit(&active, &mut next, Some((manifest.name, payload)))?;
        Ok(next.generation)
    }

    pub fn replace(&mut self, manifest: Manifest<'_>, payload: &[u8]) -> Result<u64, Error> {
        validate_manifest(&manifest)?;
        if payload.is_empty() {
            return Err(Error::NoSpace);
        }
        let active = self.recover()?;
        let index = find_entry_index(&active, manifest.name).ok_or(Error::NotInstalled)?;
        let mut next = active;
        next.slot ^= 1;
        next.generation = active.generation.checked_add(1).ok_or(Error::NoSpace)?;
        next.entries[index] = entry_from_manifest(&manifest, payload.len() as u64);
        validate_dependencies(&next)?;
        self.commit(&active, &mut next, Some((manifest.name, payload)))?;
        Ok(next.generation)
    }

    pub fn remove(&mut self, name: &[u8]) -> Result<u64, Error> {
        validate_name(name)?;
        let active = self.recover()?;
        let index = find_entry_index(&active, name).ok_or(Error::NotInstalled)?;
        let mut next = active;
        next.slot ^= 1;
        next.generation = active.generation.checked_add(1).ok_or(Error::NoSpace)?;
        let mut cursor = index;
        while cursor + 1 < next.count {
            next.entries[cursor] = next.entries[cursor + 1];
            cursor += 1;
        }
        next.count -= 1;
        next.entries[next.count] = Entry::EMPTY;
        validate_dependencies(&next).map_err(|error| match error {
            Error::DependencyMissing => Error::DependedOn,
            other => other,
        })?;
        self.commit(&active, &mut next, None)?;
        Ok(next.generation)
    }

    /// Select previous complete slot by atomically advancing only its header.
    /// Current slot remains valid until previous header is durable.
    pub fn rollback(&mut self) -> Result<u64, Error> {
        let first = self.read_snapshot(0);
        let second = self.read_snapshot(1);
        if matches!(first, Err(Error::Io)) || matches!(second, Err(Error::Io)) {
            return Err(Error::Io);
        }
        let (current, previous) = match (first, second) {
            (Ok(a), Ok(b)) if a.generation >= b.generation => (a, b),
            (Ok(a), Ok(b)) => (b, a),
            _ => return Err(Error::NotInstalled),
        };
        let generation = current.generation.checked_add(1).ok_or(Error::NoSpace)?;
        let mut sector = [0u8; SECTOR_BYTES];
        self.read(self.slot_base(previous.slot), &mut sector)?;
        let mut header = decode_header(&sector)?;
        header.generation = generation;
        self.write(self.slot_base(previous.slot), &encode_header(header))?;
        self.flush()?;
        Ok(generation)
    }

    fn recover(&mut self) -> Result<Snapshot, Error> {
        let first = self.read_snapshot(0);
        let second = self.read_snapshot(1);
        if matches!(first, Err(Error::Io)) || matches!(second, Err(Error::Io)) {
            // Either slot could be newest. Never turn an uncertain read into a
            // silent rollback.
            return Err(Error::Io);
        }
        match (first, second) {
            (Ok(a), Ok(b)) => Ok(if a.generation >= b.generation { a } else { b }),
            (Ok(a), Err(_)) => Ok(a),
            (Err(_), Ok(b)) => Ok(b),
            (Err(Error::NotInstalled), Err(Error::NotInstalled)) => Ok(Snapshot::EMPTY),
            _ => Err(Error::Corrupt),
        }
    }

    fn read_snapshot(&mut self, slot: u8) -> Result<Snapshot, Error> {
        let mut sector = [0u8; SECTOR_BYTES];
        self.read(self.slot_base(slot), &mut sector)?;
        if sector.iter().all(|byte| *byte == 0) {
            return Err(Error::NotInstalled);
        }
        let header = decode_header(&sector)?;
        if header.state == HEADER_PREPARING {
            return Err(Error::NotInstalled);
        }
        if header.state != HEADER_COMMITTED {
            return Err(Error::Corrupt);
        }
        let mut catalog = [[0u8; SECTOR_BYTES]; CATALOG_SECTORS as usize];
        for (index, output) in catalog.iter_mut().enumerate() {
            self.read(self.slot_base(slot) + 1 + index as u64, output)?;
        }
        if crc32_sectors(&catalog) != header.catalog_crc {
            return Err(Error::Corrupt);
        }
        let mut snapshot = Snapshot {
            slot,
            generation: header.generation,
            entries: [Entry::EMPTY; MAX_PACKAGES],
            count: header.count,
        };
        for index in 0..header.count {
            snapshot.entries[index] = decode_entry(catalog_entry(&catalog, index))?;
        }
        validate_dependencies(&snapshot).map_err(|_| Error::Corrupt)?;
        for index in 0..snapshot.count {
            self.validate_payload(slot, &snapshot.entries[index])?;
        }
        Ok(snapshot)
    }

    fn commit(
        &mut self,
        active: &Snapshot,
        next: &mut Snapshot,
        replacement: Option<(&[u8], &[u8])>,
    ) -> Result<(), Error> {
        let capacity = (self.slot_sectors - PAYLOAD_START_SECTOR)
            .checked_mul(SECTOR_BYTES as u64)
            .ok_or(Error::NoSpace)?;
        let preparing = encode_header(Header {
            generation: next.generation,
            count: next.count,
            catalog_crc: 0,
            state: HEADER_PREPARING,
        });
        self.write(self.slot_base(next.slot), &preparing)?;
        self.flush()?;

        let mut payload_cursor = 0u64;
        for index in 0..next.count {
            payload_cursor = align_sector(payload_cursor).ok_or(Error::NoSpace)?;
            let length = next.entries[index].payload_length;
            if payload_cursor
                .checked_add(length)
                .is_none_or(|end| end > capacity)
            {
                return Err(Error::NoSpace);
            }
            next.entries[index].payload_offset = payload_cursor;
            let replacement_payload = replacement
                .filter(|(name, _)| *name == next.entries[index].name())
                .map(|(_, payload)| payload);
            next.entries[index].payload_crc = if let Some(payload) = replacement_payload {
                self.write_new_payload(next.slot, payload_cursor, payload)?
            } else {
                let old = *find_entry(active, next.entries[index].name()).ok_or(Error::Corrupt)?;
                self.copy_payload(active.slot, &old, next.slot, payload_cursor)?
            };
            payload_cursor += length;
        }

        let mut catalog = [[0u8; SECTOR_BYTES]; CATALOG_SECTORS as usize];
        for index in 0..next.count {
            encode_entry(&next.entries[index], catalog_entry_mut(&mut catalog, index));
        }
        for (index, input) in catalog.iter().enumerate() {
            self.write(self.slot_base(next.slot) + 1 + index as u64, input)?;
        }
        self.flush()?;
        let committed = encode_header(Header {
            generation: next.generation,
            count: next.count,
            catalog_crc: crc32_sectors(&catalog),
            state: HEADER_COMMITTED,
        });
        self.write(self.slot_base(next.slot), &committed)?;
        self.flush()?;
        Ok(())
    }

    fn validate_payload(&mut self, slot: u8, entry: &Entry) -> Result<(), Error> {
        let capacity = (self.slot_sectors - PAYLOAD_START_SECTOR) * SECTOR_BYTES as u64;
        if entry
            .payload_offset
            .checked_add(entry.payload_length)
            .is_none_or(|end| end > capacity)
        {
            return Err(Error::Corrupt);
        }
        let mut remaining = entry.payload_length;
        let mut offset = entry.payload_offset;
        let mut crc = Crc32::new();
        while remaining != 0 {
            let mut sector = [0u8; SECTOR_BYTES];
            self.read(self.payload_sector(slot, offset)?, &mut sector)?;
            let count = remaining.min(SECTOR_BYTES as u64) as usize;
            crc.update(&sector[..count]);
            remaining -= count as u64;
            offset += count as u64;
        }
        if crc.finish() != entry.payload_crc {
            return Err(Error::Corrupt);
        }
        Ok(())
    }

    fn read_payload_bytes(
        &mut self,
        slot: u8,
        entry: &Entry,
        output: &mut [u8],
    ) -> Result<(), Error> {
        let mut copied = 0usize;
        while copied < output.len() {
            let mut sector = [0u8; SECTOR_BYTES];
            self.read(
                self.payload_sector(slot, entry.payload_offset + copied as u64)?,
                &mut sector,
            )?;
            let count = (output.len() - copied).min(SECTOR_BYTES);
            output[copied..copied + count].copy_from_slice(&sector[..count]);
            copied += count;
        }
        Ok(())
    }

    fn write_new_payload(&mut self, slot: u8, offset: u64, payload: &[u8]) -> Result<u32, Error> {
        let mut written = 0usize;
        while written < payload.len() {
            let count = (payload.len() - written).min(SECTOR_BYTES);
            let mut sector = [0u8; SECTOR_BYTES];
            sector[..count].copy_from_slice(&payload[written..written + count]);
            self.write(self.payload_sector(slot, offset + written as u64)?, &sector)?;
            written += count;
        }
        Ok(crc32(payload))
    }

    fn copy_payload(
        &mut self,
        source_slot: u8,
        source: &Entry,
        target_slot: u8,
        target_offset: u64,
    ) -> Result<u32, Error> {
        let mut copied = 0u64;
        let mut crc = Crc32::new();
        while copied < source.payload_length {
            let mut sector = [0u8; SECTOR_BYTES];
            self.read(
                self.payload_sector(source_slot, source.payload_offset + copied)?,
                &mut sector,
            )?;
            let count = (source.payload_length - copied).min(SECTOR_BYTES as u64) as usize;
            crc.update(&sector[..count]);
            if count < SECTOR_BYTES {
                sector[count..].fill(0);
            }
            self.write(
                self.payload_sector(target_slot, target_offset + copied)?,
                &sector,
            )?;
            copied += count as u64;
        }
        let checksum = crc.finish();
        if checksum != source.payload_crc {
            return Err(Error::Corrupt);
        }
        Ok(checksum)
    }

    fn payload_sector(&self, slot: u8, byte_offset: u64) -> Result<u64, Error> {
        if byte_offset % SECTOR_BYTES as u64 != 0 {
            return Err(Error::Corrupt);
        }
        let relative = PAYLOAD_START_SECTOR
            .checked_add(byte_offset / SECTOR_BYTES as u64)
            .ok_or(Error::NoSpace)?;
        if relative >= self.slot_sectors {
            return Err(Error::NoSpace);
        }
        self.slot_base(slot)
            .checked_add(relative)
            .ok_or(Error::NoSpace)
    }

    fn slot_base(&self, slot: u8) -> u64 {
        self.base_sector + u64::from(slot) * self.slot_sectors
    }

    fn read(&mut self, sector: u64, output: &mut [u8; SECTOR_BYTES]) -> Result<(), Error> {
        self.device
            .read_sector(sector, output)
            .then_some(())
            .ok_or(Error::Io)
    }

    fn write(&mut self, sector: u64, input: &[u8; SECTOR_BYTES]) -> Result<(), Error> {
        self.device
            .write_sector(sector, input)
            .then_some(())
            .ok_or(Error::Io)
    }

    fn flush(&mut self) -> Result<(), Error> {
        self.device.flush().then_some(()).ok_or(Error::Io)
    }
}

/// Decode signed dependency field format:
/// `MAKDEP1\0 || count || (name_len, kind, version_len, name, version)*`.
pub fn decode_dependencies<'a>(
    input: &'a [u8],
    output: &mut [Dependency<'a>; MAX_DEPENDENCIES],
) -> Result<usize, Error> {
    if input.len() < DEPENDENCY_FORMAT_MAGIC.len() + 1
        || &input[..DEPENDENCY_FORMAT_MAGIC.len()] != DEPENDENCY_FORMAT_MAGIC
    {
        return Err(Error::InvalidDependency);
    }
    let count = usize::from(input[DEPENDENCY_FORMAT_MAGIC.len()]);
    if count > MAX_DEPENDENCIES {
        return Err(Error::InvalidDependency);
    }
    let mut cursor = DEPENDENCY_FORMAT_MAGIC.len() + 1;
    for index in 0..count {
        let header = input
            .get(cursor..cursor + 3)
            .ok_or(Error::InvalidDependency)?;
        cursor += 3;
        let name_length = usize::from(header[0]);
        let kind = match header[1] {
            0 => RequirementKind::Exact,
            1 => RequirementKind::AtLeast,
            _ => return Err(Error::InvalidDependency),
        };
        let version_length = usize::from(header[2]);
        let end = cursor
            .checked_add(name_length)
            .and_then(|value| value.checked_add(version_length))
            .ok_or(Error::InvalidDependency)?;
        let fields = input.get(cursor..end).ok_or(Error::InvalidDependency)?;
        let (name, version) = fields.split_at(name_length);
        output[index] = Dependency {
            name,
            kind,
            version,
        };
        cursor = end;
    }
    if cursor != input.len() {
        return Err(Error::InvalidDependency);
    }
    // Reuse manifest validation for names, versions, duplicates, and self is
    // checked when actual package name is known by Store::install/replace.
    for (index, dependency) in output[..count].iter().enumerate() {
        validate_name(dependency.name).map_err(|_| Error::InvalidDependency)?;
        parse_version(dependency.version).map_err(|_| Error::InvalidDependency)?;
        if output[..index]
            .iter()
            .any(|previous| previous.name == dependency.name)
        {
            return Err(Error::InvalidDependency);
        }
    }
    Ok(count)
}

fn validate_manifest(manifest: &Manifest<'_>) -> Result<(), Error> {
    validate_name(manifest.name)?;
    parse_version(manifest.version)?;
    if manifest.dependencies.len() > MAX_DEPENDENCIES {
        return Err(Error::InvalidDependency);
    }
    for (index, dependency) in manifest.dependencies.iter().enumerate() {
        validate_name(dependency.name).map_err(|_| Error::InvalidDependency)?;
        parse_version(dependency.version).map_err(|_| Error::InvalidDependency)?;
        if dependency.name == manifest.name
            || manifest.dependencies[..index]
                .iter()
                .any(|previous| previous.name == dependency.name)
        {
            return Err(Error::InvalidDependency);
        }
    }
    Ok(())
}

fn validate_name(name: &[u8]) -> Result<(), Error> {
    if name.is_empty()
        || name.len() > MAX_NAME_BYTES
        || !name
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        || !name[0].is_ascii_lowercase()
    {
        return Err(Error::InvalidName);
    }
    Ok(())
}

fn parse_version(version: &[u8]) -> Result<[u32; 3], Error> {
    if version.is_empty() || version.len() > MAX_VERSION_BYTES {
        return Err(Error::InvalidVersion);
    }
    let mut result = [0u32; 3];
    let mut part = 0usize;
    let mut digits = 0usize;
    let mut component_start = true;
    let mut leading_zero = false;
    for byte in version.iter().copied().chain(core::iter::once(b'.')) {
        if byte == b'.' {
            if digits == 0 || part >= 3 {
                return Err(Error::InvalidVersion);
            }
            part += 1;
            digits = 0;
            component_start = true;
            leading_zero = false;
        } else if byte.is_ascii_digit() {
            if part >= 3 || (!component_start && leading_zero) {
                return Err(Error::InvalidVersion);
            }
            if component_start {
                leading_zero = byte == b'0';
            }
            component_start = false;
            result[part] = result[part]
                .checked_mul(10)
                .and_then(|value| value.checked_add(u32::from(byte - b'0')))
                .ok_or(Error::InvalidVersion)?;
            digits += 1;
        } else {
            return Err(Error::InvalidVersion);
        }
    }
    if !(2..=3).contains(&part) {
        return Err(Error::InvalidVersion);
    }
    Ok(result)
}

fn entry_from_manifest(manifest: &Manifest<'_>, payload_length: u64) -> Entry {
    let mut entry = Entry::EMPTY;
    entry.used = true;
    entry.name[..manifest.name.len()].copy_from_slice(manifest.name);
    entry.name_length = manifest.name.len() as u8;
    entry.version[..manifest.version.len()].copy_from_slice(manifest.version);
    entry.version_length = manifest.version.len() as u8;
    entry.payload_length = payload_length;
    entry.dependency_count = manifest.dependencies.len() as u8;
    for (output, input) in entry.dependencies.iter_mut().zip(manifest.dependencies) {
        output.name[..input.name.len()].copy_from_slice(input.name);
        output.name_length = input.name.len() as u8;
        output.kind = input.kind;
        output.version[..input.version.len()].copy_from_slice(input.version);
        output.version_length = input.version.len() as u8;
    }
    entry
}

fn validate_dependencies(snapshot: &Snapshot) -> Result<(), Error> {
    for entry in &snapshot.entries[..snapshot.count] {
        for dependency in &entry.dependencies[..usize::from(entry.dependency_count)] {
            let target = find_entry(snapshot, dependency.name()).ok_or(Error::DependencyMissing)?;
            let actual = parse_version(target.version()).map_err(|_| Error::Corrupt)?;
            let required = parse_version(dependency.version()).map_err(|_| Error::Corrupt)?;
            let satisfied = match dependency.kind {
                RequirementKind::Exact => actual == required,
                RequirementKind::AtLeast => actual >= required,
            };
            if !satisfied {
                return Err(Error::DependencyVersion);
            }
        }
    }
    for root in 0..snapshot.count {
        let mut visiting = [false; MAX_PACKAGES];
        if dependency_cycle(snapshot, root, &mut visiting) {
            return Err(Error::DependencyCycle);
        }
    }
    Ok(())
}

fn dependency_cycle(
    snapshot: &Snapshot,
    index: usize,
    visiting: &mut [bool; MAX_PACKAGES],
) -> bool {
    if visiting[index] {
        return true;
    }
    visiting[index] = true;
    let entry = &snapshot.entries[index];
    for dependency in &entry.dependencies[..usize::from(entry.dependency_count)] {
        if let Some(child) = find_entry_index(snapshot, dependency.name())
            && dependency_cycle(snapshot, child, visiting)
        {
            return true;
        }
    }
    visiting[index] = false;
    false
}

fn find_entry<'a>(snapshot: &'a Snapshot, name: &[u8]) -> Option<&'a Entry> {
    find_entry_index(snapshot, name).map(|index| &snapshot.entries[index])
}

fn find_entry_index(snapshot: &Snapshot, name: &[u8]) -> Option<usize> {
    snapshot.entries[..snapshot.count]
        .iter()
        .position(|entry| entry.used && entry.name() == name)
}

fn encode_header(header: Header) -> [u8; SECTOR_BYTES] {
    let mut sector = [0u8; SECTOR_BYTES];
    sector[..8].copy_from_slice(&HEADER_MAGIC);
    put_u32(&mut sector, 8, FORMAT_VERSION);
    put_u32(&mut sector, 12, header.state);
    put_u64(&mut sector, 16, header.generation);
    put_u32(&mut sector, 24, header.count as u32);
    put_u32(&mut sector, 28, header.catalog_crc);
    let checksum = crc32(&sector[..508]);
    put_u32(&mut sector, 508, checksum);
    sector
}

fn decode_header(sector: &[u8; SECTOR_BYTES]) -> Result<Header, Error> {
    if sector[..8] != HEADER_MAGIC
        || get_u32(sector, 8) != FORMAT_VERSION
        || get_u32(sector, 508) != crc32(&sector[..508])
    {
        return Err(Error::Corrupt);
    }
    let count = get_u32(sector, 24) as usize;
    if count > MAX_PACKAGES {
        return Err(Error::Corrupt);
    }
    Ok(Header {
        generation: get_u64(sector, 16),
        count,
        catalog_crc: get_u32(sector, 28),
        state: get_u32(sector, 12),
    })
}

fn encode_entry(entry: &Entry, output: &mut [u8]) {
    output.fill(0);
    output[0] = u8::from(entry.used);
    output[1] = entry.name_length;
    output[2] = entry.version_length;
    output[3] = entry.dependency_count;
    put_u64(output, 4, entry.payload_offset);
    put_u64(output, 12, entry.payload_length);
    put_u32(output, 20, entry.payload_crc);
    output[24..56].copy_from_slice(&entry.name);
    output[56..72].copy_from_slice(&entry.version);
    for (index, dependency) in entry.dependencies.iter().enumerate() {
        let start = 72 + index * DEPENDENCY_BYTES;
        output[start] = dependency.name_length;
        output[start + 1] = match dependency.kind {
            RequirementKind::Exact => 0,
            RequirementKind::AtLeast => 1,
        };
        output[start + 2] = dependency.version_length;
        output[start + 3..start + 35].copy_from_slice(&dependency.name);
        output[start + 35..start + 51].copy_from_slice(&dependency.version);
    }
    let checksum = crc32(&output[..252]);
    put_u32(output, 252, checksum);
}

fn decode_entry(input: &[u8]) -> Result<Entry, Error> {
    if input.len() != ENTRY_BYTES || get_u32(input, 252) != crc32(&input[..252]) || input[0] != 1 {
        return Err(Error::Corrupt);
    }
    let name_length = input[1] as usize;
    let version_length = input[2] as usize;
    let dependency_count = input[3] as usize;
    if name_length == 0
        || name_length > MAX_NAME_BYTES
        || version_length == 0
        || version_length > MAX_VERSION_BYTES
        || dependency_count > MAX_DEPENDENCIES
    {
        return Err(Error::Corrupt);
    }
    let mut entry = Entry::EMPTY;
    entry.used = true;
    entry.name_length = name_length as u8;
    entry.name.copy_from_slice(&input[24..56]);
    entry.version_length = version_length as u8;
    entry.version.copy_from_slice(&input[56..72]);
    entry.dependency_count = dependency_count as u8;
    entry.payload_offset = get_u64(input, 4);
    entry.payload_length = get_u64(input, 12);
    entry.payload_crc = get_u32(input, 20);
    validate_name(entry.name()).map_err(|_| Error::Corrupt)?;
    parse_version(entry.version()).map_err(|_| Error::Corrupt)?;
    if entry.payload_length == 0 || entry.payload_offset % SECTOR_BYTES as u64 != 0 {
        return Err(Error::Corrupt);
    }
    for index in 0..dependency_count {
        let start = 72 + index * DEPENDENCY_BYTES;
        let mut dependency = StoredDependency::EMPTY;
        dependency.name_length = input[start];
        dependency.kind = match input[start + 1] {
            0 => RequirementKind::Exact,
            1 => RequirementKind::AtLeast,
            _ => return Err(Error::Corrupt),
        };
        dependency.version_length = input[start + 2];
        if usize::from(dependency.name_length) > MAX_NAME_BYTES
            || usize::from(dependency.version_length) > MAX_VERSION_BYTES
        {
            return Err(Error::Corrupt);
        }
        dependency
            .name
            .copy_from_slice(&input[start + 3..start + 35]);
        dependency
            .version
            .copy_from_slice(&input[start + 35..start + 51]);
        validate_name(dependency.name()).map_err(|_| Error::Corrupt)?;
        parse_version(dependency.version()).map_err(|_| Error::Corrupt)?;
        if dependency.name() == entry.name()
            || entry.dependencies[..index]
                .iter()
                .any(|previous| previous.name() == dependency.name())
        {
            return Err(Error::Corrupt);
        }
        entry.dependencies[index] = dependency;
    }
    Ok(entry)
}

fn catalog_entry(catalog: &[[u8; SECTOR_BYTES]; CATALOG_SECTORS as usize], index: usize) -> &[u8] {
    let byte = index * ENTRY_BYTES;
    let sector = byte / SECTOR_BYTES;
    let offset = byte % SECTOR_BYTES;
    &catalog[sector][offset..offset + ENTRY_BYTES]
}

fn catalog_entry_mut(
    catalog: &mut [[u8; SECTOR_BYTES]; CATALOG_SECTORS as usize],
    index: usize,
) -> &mut [u8] {
    let byte = index * ENTRY_BYTES;
    let sector = byte / SECTOR_BYTES;
    let offset = byte % SECTOR_BYTES;
    &mut catalog[sector][offset..offset + ENTRY_BYTES]
}

fn align_sector(value: u64) -> Option<u64> {
    value
        .checked_add(SECTOR_BYTES as u64 - 1)
        .map(|sum| sum / SECTOR_BYTES as u64 * SECTOR_BYTES as u64)
}

fn crc32_sectors(sectors: &[[u8; SECTOR_BYTES]; CATALOG_SECTORS as usize]) -> u32 {
    let mut crc = Crc32::new();
    for sector in sectors {
        crc.update(sector);
    }
    crc.finish()
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = Crc32::new();
    crc.update(bytes);
    crc.finish()
}

struct Crc32(u32);

impl Crc32 {
    const fn new() -> Self {
        Self(0xffff_ffff)
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u32::from(*byte);
            for _ in 0..8 {
                self.0 = (self.0 >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(self.0 & 1)));
            }
        }
    }

    const fn finish(self) -> u32 {
        !self.0
    }
}

fn get_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn get_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_version_parser_is_strict() {
        assert_eq!(parse_version(b"0.1.23"), Ok([0, 1, 23]));
        assert_eq!(parse_version(b"1.2"), Ok([1, 2, 0]));
        assert_eq!(parse_version(b"01.0.0"), Err(Error::InvalidVersion));
        assert_eq!(parse_version(b"1"), Err(Error::InvalidVersion));
        assert_eq!(parse_version(b"1.0.0-alpha"), Err(Error::InvalidVersion));
    }

    #[test]
    fn corrupt_dependency_lengths_never_slice_past_record() {
        let manifest = Manifest {
            name: b"app",
            version: b"1.0.0",
            dependencies: &[Dependency {
                name: b"libc",
                kind: RequirementKind::Exact,
                version: b"1.0.0",
            }],
        };
        let entry = entry_from_manifest(&manifest, 1);
        let mut encoded = [0u8; ENTRY_BYTES];
        encode_entry(&entry, &mut encoded);
        encoded[72] = u8::MAX;
        let checksum = crc32(&encoded[..252]);
        put_u32(&mut encoded, 252, checksum);
        assert!(matches!(decode_entry(&encoded), Err(Error::Corrupt)));
    }
}
