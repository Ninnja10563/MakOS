use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use makos_structured_log::{CAPACITY, DecodeError, IMAGE_BYTES, Journal};

const PERSISTENT_NAME: &[u8] = b".makos-system-log";

struct LockedJournal {
    lock: AtomicBool,
    journal: UnsafeCell<Journal>,
}

unsafe impl Sync for LockedJournal {}

static LOG: LockedJournal = LockedJournal {
    lock: AtomicBool::new(false),
    journal: UnsafeCell::new(Journal::new()),
};
static PERSISTENCE_READY: AtomicBool = AtomicBool::new(false);
static PERSISTENCE_BUSY: AtomicBool = AtomicBool::new(false);
static PERSISTENCE_FAILURE_REPORTED: AtomicBool = AtomicBool::new(false);

pub fn append(severity: u8, message: &[u8]) -> Option<u64> {
    let sequence = with_log(|journal| {
        journal.append(
            crate::arch::monotonic_ticks(),
            current_pid(),
            severity,
            message,
        )
    });
    if sequence.is_some() && PERSISTENCE_READY.load(Ordering::Acquire) {
        persist_snapshot();
    }
    sequence
}

pub fn read(sequence: u64, output: &mut [u8]) -> Option<(usize, u64, u64, u8)> {
    with_log(|journal| {
        let record = journal.record(sequence)?;
        let length = record.message().len().min(output.len());
        output[..length].copy_from_slice(&record.message()[..length]);
        Some((length, record.ticks, record.pid, record.severity))
    })
}

pub fn audit(message: &[u8]) {
    let _ = append(4, message);
}

/// Load durable records after MakFS4 mount, then append records emitted before
/// storage became available. Corrupt journals remain untouched for diagnosis.
pub fn mount_persistent() {
    if !crate::makfs4_volume::mounted() {
        return;
    }
    let early = with_log(|journal| *journal);
    let loaded = match load_persistent() {
        Ok(Some(journal)) => journal,
        Ok(None) => Journal::new(),
        Err(error) => {
            report_persistence_failure("load", error);
            return;
        }
    };
    let persisted_audits = summarize_audits(&loaded);
    if persisted_audits.records != 0 {
        crate::serial_println!(
            "MAKOS_SECURITY_AUDIT_PERSIST_OK source=prior-boot severity=4 records={} auth_accepted={} auth_denied={} account={} session={} package={} pid_attributed={}",
            persisted_audits.records,
            persisted_audits.authentication_accepted,
            persisted_audits.authentication_denied,
            persisted_audits.account,
            persisted_audits.session,
            persisted_audits.package,
            u8::from(persisted_audits.pid_attributed),
        );
    }
    let mut merged = loaded;
    let first = early.next_sequence().saturating_sub(CAPACITY as u64).max(1);
    for sequence in first..early.next_sequence() {
        if let Some(record) = early.record(sequence)
            && merged
                .append(record.ticks, record.pid, record.severity, record.message())
                .is_none()
        {
            report_persistence_failure("merge", PersistenceError::Sequence);
            return;
        }
    }
    with_log(|journal| *journal = merged);
    PERSISTENCE_READY.store(true, Ordering::Release);
    if write_snapshot(merged).is_err() {
        PERSISTENCE_READY.store(false, Ordering::Release);
        report_persistence_failure("write", PersistenceError::FileSystem);
        return;
    }
    crate::serial_println!(
        "MAKOS_STRUCTURED_LOG_PERSIST_OK path=/.makos-system-log format=MAKLOG01 records={} next_sequence={} cow=makfs4",
        merged.record_count(),
        merged.next_sequence(),
    );
}

#[derive(Clone, Copy)]
struct AuditSummary {
    records: usize,
    authentication_accepted: usize,
    authentication_denied: usize,
    account: usize,
    session: usize,
    package: usize,
    pid_attributed: bool,
}

fn summarize_audits(journal: &Journal) -> AuditSummary {
    let mut summary = AuditSummary {
        records: 0,
        authentication_accepted: 0,
        authentication_denied: 0,
        account: 0,
        session: 0,
        package: 0,
        pid_attributed: true,
    };
    let first = journal
        .next_sequence()
        .saturating_sub(journal.record_count() as u64)
        .max(1);
    for sequence in first..journal.next_sequence() {
        let Some(record) = journal.record(sequence) else {
            continue;
        };
        if record.severity != 4 || !record.message().starts_with(b"audit: ") {
            continue;
        }
        summary.records += 1;
        summary.pid_attributed &= record.pid != 0;
        match record.message() {
            b"audit: authentication accepted" => summary.authentication_accepted += 1,
            b"audit: authentication denied" => summary.authentication_denied += 1,
            b"audit: account created" => summary.account += 1,
            b"audit: session signed out" => summary.session += 1,
            message if message.starts_with(b"audit: package ") => summary.package += 1,
            _ => {}
        }
    }
    summary
}

fn persist_snapshot() {
    if PERSISTENCE_BUSY
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let journal = with_log(|journal| *journal);
    if write_snapshot(journal).is_err() {
        PERSISTENCE_READY.store(false, Ordering::Release);
        report_persistence_failure("append", PersistenceError::FileSystem);
    }
    PERSISTENCE_BUSY.store(false, Ordering::Release);
}

fn load_persistent() -> Result<Option<Journal>, PersistenceError> {
    let Some(index) = crate::makfs4_volume::find_child(1, PERSISTENT_NAME)
        .map_err(|_| PersistenceError::FileSystem)?
    else {
        crate::makfs4_volume::create_inode(1, PERSISTENT_NAME, 0o100600, 0, 0)
            .map_err(|_| PersistenceError::FileSystem)?;
        return Ok(None);
    };
    let inode = crate::makfs4_volume::read_inode(index)
        .map_err(|_| PersistenceError::FileSystem)?
        .ok_or(PersistenceError::FileSystem)?;
    if inode.mode & 0o170000 != 0o100000 {
        return Err(PersistenceError::Geometry);
    }
    if inode.size == 0 {
        return Ok(None);
    }
    if inode.size != IMAGE_BYTES as u64 {
        return Err(PersistenceError::Geometry);
    }
    let mut image = [0u8; IMAGE_BYTES];
    let count = crate::makfs4_volume::read_inode_at(index, 0, &mut image)
        .map_err(|_| PersistenceError::FileSystem)?;
    if count != image.len() {
        return Err(PersistenceError::Geometry);
    }
    Journal::decode(&image)
        .map(Some)
        .map_err(|error| match error {
            DecodeError::Header => PersistenceError::DecodeHeader,
            DecodeError::Checksum => PersistenceError::DecodeChecksum,
            DecodeError::Record => PersistenceError::DecodeRecord,
        })
}

fn write_snapshot(journal: Journal) -> Result<(), PersistenceError> {
    let index = match crate::makfs4_volume::find_child(1, PERSISTENT_NAME)
        .map_err(|_| PersistenceError::FileSystem)?
    {
        Some(index) => index,
        None => crate::makfs4_volume::create_inode(1, PERSISTENT_NAME, 0o100600, 0, 0)
            .map_err(|_| PersistenceError::FileSystem)?,
    };
    let image = journal.encode();
    let count = crate::makfs4_volume::write_inode_at(index, 0, &image)
        .map_err(|_| PersistenceError::FileSystem)?;
    if count != image.len() {
        return Err(PersistenceError::Geometry);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum PersistenceError {
    DecodeHeader,
    DecodeChecksum,
    DecodeRecord,
    FileSystem,
    Geometry,
    Sequence,
}

fn report_persistence_failure(stage: &str, error: PersistenceError) {
    if !PERSISTENCE_FAILURE_REPORTED.swap(true, Ordering::AcqRel) {
        crate::serial_println!(
            "MAKOS_STRUCTURED_LOG_PERSIST_ERROR stage={} error={:?} journal_preserved=1",
            stage,
            error,
        );
    }
}

#[cfg(target_arch = "x86_64")]
fn current_pid() -> u64 {
    crate::scheduler::current_pid()
}

#[cfg(target_arch = "aarch64")]
fn current_pid() -> u64 {
    crate::aarch64_process::current_pid()
}

fn with_log<R>(function: impl FnOnce(&mut Journal) -> R) -> R {
    while LOG
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *LOG.journal.get() });
    LOG.lock.store(false, Ordering::Release);
    result
}
