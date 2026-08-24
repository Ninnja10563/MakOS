//! Guest-side x86_64 installer.
//!
//! Secondary ATA master is running GPT live system (`disk0`); secondary ATA
//! slave is explicit target (`disk1`). Target stays unbootable until final MBR
//! write. Resume accepts only uncommitted sectors matching source exactly.

use alloc::format;
use makos_installer::{Disk, InstallEvent, InstallMode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallError {
    Permission,
    DeviceCount,
    Geometry,
    SourceNotGpt,
    TargetNotBlank,
    ResumeCommitted,
    ResumeConflict,
    Read,
    Write,
    Verify,
    Flush,
}

struct RawAta<'a>(&'a mut crate::drivers::ata::AtaDisk);

impl Disk for RawAta<'_> {
    fn sectors(&self) -> u64 {
        self.0.sectors()
    }

    fn read_sector(&mut self, lba: u32, output: &mut [u8; 512]) -> bool {
        self.0.read_sector(lba, output)
    }

    fn write_sector(&mut self, lba: u32, input: &[u8; 512]) -> bool {
        self.0.write_sector_unflushed(lba, input)
    }

    fn flush(&mut self) -> bool {
        self.0.flush()
    }

    fn read_sectors_8(&mut self, lba: u32, output: &mut [u8; 4096]) -> bool {
        self.0.read_sectors_8(lba, output)
    }

    fn write_sectors_8(&mut self, lba: u32, input: &[u8; 4096]) -> bool {
        self.0.write_sectors_8_unflushed(lba, input)
    }
}

pub fn install_disk1(mode: InstallMode) -> Result<makos_installer::InstallReport, InstallError> {
    let credentials = crate::security::credentials();
    if credentials.uid != crate::security::INIT_UID
        || !crate::security::has_capability(crate::security::CAP_FILE_WRITE)
    {
        return Err(InstallError::Permission);
    }
    let mut source =
        crate::drivers::ata::AtaDisk::identify_secondary().ok_or(InstallError::DeviceCount)?;
    let mut target = crate::drivers::ata::AtaDisk::identify_secondary_slave()
        .ok_or(InstallError::DeviceCount)?;
    let source_sectors = source.sectors();
    let target_sectors = target.sectors();
    crate::serial_println!(
        "MAKOS_X86_INSTALL_SCAN source=disk0 target=disk1 source_sectors={} target_sectors={} policy={}",
        source_sectors,
        target_sectors,
        match mode {
            InstallMode::Fresh => "exact-size,whole-disk-blank",
            InstallMode::Resume => "exact-size,mbr-blank,matching-sectors-only",
        },
    );
    let mut source = RawAta(&mut source);
    let mut target = RawAta(&mut target);
    makos_installer::install(&mut source, &mut target, mode, |event| match event {
        InstallEvent::Ready { sectors } => crate::serial_println!(
            "MAKOS_X86_INSTALL_BEGIN source=disk0 target=disk1 mode={} sectors={} commit=protective-mbr-last",
            match mode {
                InstallMode::Fresh => "fresh",
                InstallMode::Resume => "resume",
            },
            sectors,
        ),
        InstallEvent::Progress {
            copied_sectors,
            sectors,
        } => crate::serial_println!(
            "MAKOS_X86_INSTALL_PROGRESS target=disk1 copied_sectors={} total_sectors={}",
            copied_sectors,
            sectors,
        ),
    })
    .map_err(map_error)
}

fn map_error(error: makos_installer::InstallError) -> InstallError {
    match error {
        makos_installer::InstallError::Geometry => InstallError::Geometry,
        makos_installer::InstallError::SourceNotGpt => InstallError::SourceNotGpt,
        makos_installer::InstallError::TargetNotBlank => InstallError::TargetNotBlank,
        makos_installer::InstallError::ResumeCommitted => InstallError::ResumeCommitted,
        makos_installer::InstallError::ResumeConflict => InstallError::ResumeConflict,
        makos_installer::InstallError::Read => InstallError::Read,
        makos_installer::InstallError::Write => InstallError::Write,
        makos_installer::InstallError::Verify => InstallError::Verify,
        makos_installer::InstallError::Flush => InstallError::Flush,
    }
}

pub fn describe_error(error: InstallError) {
    let message = match error {
        InstallError::Permission => "permission denied; complete administrator login",
        InstallError::DeviceCount => "requires GPT live disk0 plus target disk1",
        InstallError::Geometry => "target must have exactly same sector count as live disk",
        InstallError::SourceNotGpt => "disk0 is not a valid MakOS GPT live system",
        InstallError::TargetNotBlank => "disk1 is not completely blank; refusing overwrite",
        InstallError::ResumeCommitted => "disk1 MBR is already committed; refusing resume",
        InstallError::ResumeConflict => "disk1 contains data not matching live source",
        InstallError::Read => "disk read failed",
        InstallError::Write => "disk write failed",
        InstallError::Verify => "read-after-write verification failed",
        InstallError::Flush => "target flush failed",
    };
    crate::graphics::terminal_write(b"install: ");
    crate::graphics::terminal_write(message.as_bytes());
    crate::graphics::terminal_write(b"\n");
    crate::serial_println!("MAKOS_X86_INSTALL_ERROR error={:?}", error);
}

pub fn success_message(report: makos_installer::InstallReport) {
    let mib = report.sectors.saturating_mul(512) / (1024 * 1024);
    crate::graphics::terminal_write(
        format!("Installed MakOS to disk1 ({mib} MiB). Shut down, detach live disk, boot disk1.\n")
            .as_bytes(),
    );
    crate::serial_println!(
        "MAKOS_X86_INSTALL_OK source=disk0 target=disk1 sectors={} verified=read-after-write,gpt-boundaries flush=1 mbr_commit=last written_sectors={}",
        report.sectors,
        report.written_sectors,
    );
}
