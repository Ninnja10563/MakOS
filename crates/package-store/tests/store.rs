use makos_package_store::{
    Dependency, Error, Manifest, RequirementKind, SECTOR_BYTES, SectorDevice, Store,
    decode_dependencies,
};

const SECTORS: usize = 64;
const SLOT_SECTORS: u64 = 32;

#[derive(Clone)]
struct MemoryDisk {
    durable: Vec<[u8; SECTOR_BYTES]>,
    pending: Vec<(u64, [u8; SECTOR_BYTES])>,
    fail_after: Option<usize>,
    mutations: usize,
}

impl MemoryDisk {
    fn blank() -> Self {
        Self {
            durable: vec![[0; SECTOR_BYTES]; SECTORS],
            pending: Vec::new(),
            fail_after: None,
            mutations: 0,
        }
    }

    fn crash(mut self) -> Self {
        self.pending.clear();
        self.fail_after = None;
        self.mutations = 0;
        self
    }

    fn should_fail(&mut self) -> bool {
        let fail = self.fail_after == Some(self.mutations);
        self.mutations += 1;
        fail
    }
}

impl SectorDevice for MemoryDisk {
    fn sector_count(&self) -> u64 {
        self.durable.len() as u64
    }

    fn read_sector(&mut self, sector: u64, output: &mut [u8; SECTOR_BYTES]) -> bool {
        let Some(stored) = self.durable.get(sector as usize) else {
            return false;
        };
        *output = self
            .pending
            .iter()
            .rev()
            .find(|(pending_sector, _)| *pending_sector == sector)
            .map_or(*stored, |(_, pending)| *pending);
        true
    }

    fn write_sector(&mut self, sector: u64, input: &[u8; SECTOR_BYTES]) -> bool {
        if sector >= self.sector_count() || self.should_fail() {
            return false;
        }
        self.pending.push((sector, *input));
        true
    }

    fn flush(&mut self) -> bool {
        if self.should_fail() {
            return false;
        }
        for (sector, data) in self.pending.drain(..) {
            self.durable[sector as usize] = data;
        }
        true
    }
}

fn manifest<'a>(
    name: &'a [u8],
    version: &'a [u8],
    dependencies: &'a [Dependency<'a>],
) -> Manifest<'a> {
    Manifest {
        name,
        version,
        dependencies,
    }
}

#[test]
fn payloads_survive_reopen_replace_and_remove() {
    let mut store = Store::open(MemoryDisk::blank(), 0, SLOT_SECTORS).unwrap();
    let libc_payload = [0x5au8; 900];
    store
        .install(manifest(b"libc", b"1.2.0", &[]), &libc_payload)
        .unwrap();
    let dependencies = [Dependency {
        name: b"libc",
        kind: RequirementKind::AtLeast,
        version: b"1.1.0",
    }];
    store
        .install(
            manifest(b"shell", b"2.0.0", &dependencies),
            b"shell payload spanning sectors: ok",
        )
        .unwrap();

    let disk = store.into_inner().crash();
    let mut reopened = Store::open(disk, 0, SLOT_SECTORS).unwrap();
    assert_eq!(
        reopened.installed(b"shell").unwrap().unwrap().version(),
        b"2.0.0"
    );
    let mut payload = [0u8; 900];
    let count = reopened.read_payload(b"libc", &mut payload).unwrap();
    assert_eq!(count, libc_payload.len());
    assert_eq!(payload, libc_payload);
    let mut middle = [0u8; 530];
    assert_eq!(
        reopened.read_payload_at(b"libc", 257, &mut middle).unwrap(),
        middle.len()
    );
    assert_eq!(middle, [0x5a; 530]);
    let state = reopened.state().unwrap();
    assert_eq!(state.package_count, 2);
    let shell = reopened.package(1).unwrap().unwrap();
    assert_eq!(shell.name(), b"shell");
    assert!(shell.payload_first_sector >= 5);
    let mut listed = [makos_package_store::PackageInfo::EMPTY; 8];
    assert_eq!(reopened.packages(&mut listed).unwrap(), state);
    assert_eq!(listed[0].name(), b"libc");
    assert_eq!(reopened.remove(b"libc"), Err(Error::DependedOn));
    reopened.remove(b"shell").unwrap();
    reopened.remove(b"libc").unwrap();
    assert_eq!(reopened.installed(b"libc").unwrap(), None);
}

#[test]
fn versioned_dependency_wire_format_decodes_and_rejects_trailing_bytes() {
    let encoded = b"MAKDEP1\0\x01\x04\x01\x05libc1.2.0";
    let mut dependencies = [Dependency::EMPTY; 3];
    let count = decode_dependencies(encoded, &mut dependencies).unwrap();
    assert_eq!(count, 1);
    assert_eq!(dependencies[0].name, b"libc");
    assert_eq!(dependencies[0].kind, RequirementKind::AtLeast);
    assert_eq!(dependencies[0].version, b"1.2.0");
    let mut trailing = encoded.to_vec();
    trailing.push(0);
    assert_eq!(
        decode_dependencies(&trailing, &mut dependencies),
        Err(Error::InvalidDependency)
    );
}

#[test]
fn dependency_versions_and_cycles_are_rejected_before_commit() {
    let mut store = Store::open(MemoryDisk::blank(), 0, SLOT_SECTORS).unwrap();
    let missing = [Dependency {
        name: b"libc",
        kind: RequirementKind::Exact,
        version: b"1.0.0",
    }];
    assert_eq!(
        store.install(manifest(b"app", b"1.0.0", &missing), b"app"),
        Err(Error::DependencyMissing)
    );
    store
        .install(manifest(b"libc", b"1.0.0", &[]), b"libc")
        .unwrap();
    store
        .install(manifest(b"app", b"1.0.0", &missing), b"app")
        .unwrap();
    assert_eq!(
        store.replace(manifest(b"libc", b"2.0.0", &[]), b"new libc"),
        Err(Error::DependencyVersion)
    );
    let cycle = [Dependency {
        name: b"app",
        kind: RequirementKind::AtLeast,
        version: b"1.0.0",
    }];
    assert_eq!(
        store.replace(manifest(b"libc", b"1.0.0", &cycle), b"cycle"),
        Err(Error::DependencyCycle)
    );
    assert_eq!(
        store.installed(b"libc").unwrap().unwrap().version(),
        b"1.0.0"
    );
}

#[test]
fn every_interrupted_update_recovers_old_complete_generation() {
    let mut baseline = Store::open(MemoryDisk::blank(), 0, SLOT_SECTORS).unwrap();
    baseline
        .install(manifest(b"core", b"1.0.0", &[]), b"old payload")
        .unwrap();
    let baseline_disk = baseline.into_inner().crash();

    let mut observed_success = false;
    for fail_after in 0..20 {
        let mut injected = baseline_disk.clone();
        injected.fail_after = Some(fail_after);
        let mut store = Store::open(injected, 0, SLOT_SECTORS).unwrap();
        let result = store.replace(manifest(b"core", b"1.1.0", &[]), b"new payload");
        let crashed = store.into_inner().crash();
        let mut recovered = Store::open(crashed, 0, SLOT_SECTORS).unwrap();
        let version = recovered.installed(b"core").unwrap().unwrap();
        if result.is_err() {
            assert_eq!(version.version(), b"1.0.0", "fail_after={fail_after}");
        } else {
            observed_success = true;
            assert_eq!(version.version(), b"1.1.0");
        }
    }
    assert!(observed_success);
}

#[test]
fn interrupted_first_install_recovers_empty_store() {
    for fail_after in 0..10 {
        let mut disk = MemoryDisk::blank();
        disk.fail_after = Some(fail_after);
        let mut store = Store::open(disk, 0, SLOT_SECTORS).unwrap();
        let result = store.install(manifest(b"core", b"1.0.0", &[]), b"payload");
        let crashed = store.into_inner().crash();
        let mut recovered = Store::open(crashed, 0, SLOT_SECTORS).unwrap();
        if result.is_err() {
            assert_eq!(recovered.installed(b"core").unwrap(), None);
        }
    }
}

#[test]
fn corrupt_new_payload_falls_back_to_previous_generation() {
    let mut store = Store::open(MemoryDisk::blank(), 0, SLOT_SECTORS).unwrap();
    store
        .install(manifest(b"core", b"1.0.0", &[]), b"old payload")
        .unwrap();
    store
        .replace(manifest(b"core", b"1.1.0", &[]), b"new payload")
        .unwrap();
    let mut disk = store.into_inner().crash();
    // Generation 2 occupies slot 1; payload starts after header + four catalog sectors.
    disk.durable[(SLOT_SECTORS + 5) as usize][0] ^= 0xff;
    let mut recovered = Store::open(disk, 0, SLOT_SECTORS).unwrap();
    assert_eq!(
        recovered.installed(b"core").unwrap().unwrap().version(),
        b"1.0.0"
    );
    let mut payload = [0u8; 32];
    let count = recovered.read_payload(b"core", &mut payload).unwrap();
    assert_eq!(&payload[..count], b"old payload");
}

#[test]
fn rollback_recommits_previous_payload_atomically() {
    let mut store = Store::open(MemoryDisk::blank(), 0, SLOT_SECTORS).unwrap();
    store
        .install(manifest(b"core", b"1.0", &[]), b"old payload")
        .unwrap();
    store
        .replace(manifest(b"core", b"2.0", &[]), b"new payload")
        .unwrap();
    assert_eq!(store.rollback().unwrap(), 3);
    assert_eq!(store.installed(b"core").unwrap().unwrap().version(), b"1.0");
    let mut payload = [0u8; 32];
    let count = store.read_payload(b"core", &mut payload).unwrap();
    assert_eq!(&payload[..count], b"old payload");
    assert_eq!(store.rollback().unwrap(), 4);
    assert_eq!(store.installed(b"core").unwrap().unwrap().version(), b"2.0");
}

#[test]
fn interrupted_rollback_keeps_one_complete_selected_generation() {
    let mut baseline = Store::open(MemoryDisk::blank(), 0, SLOT_SECTORS).unwrap();
    baseline
        .install(manifest(b"core", b"1.0", &[]), b"old payload")
        .unwrap();
    baseline
        .replace(manifest(b"core", b"2.0", &[]), b"new payload")
        .unwrap();
    let baseline_disk = baseline.into_inner().crash();

    for fail_after in 0..4 {
        let mut disk = baseline_disk.clone();
        disk.fail_after = Some(fail_after);
        let mut store = Store::open(disk, 0, SLOT_SECTORS).unwrap();
        let result = store.rollback();
        let mut recovered = Store::open(store.into_inner().crash(), 0, SLOT_SECTORS).unwrap();
        let version = recovered.installed(b"core").unwrap().unwrap();
        if result.is_err() {
            assert_eq!(version.version(), b"2.0");
        } else {
            assert_eq!(version.version(), b"1.0");
        }
    }
}
