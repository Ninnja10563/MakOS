//! Guest-side AArch64 installer.
//!
//! Virtio disk 0 is the running GPT live system. Disk 1 must be a distinct,
//! equally sized target. Fresh installs require wholly blank media; resume
//! accepts only an uncommitted MBR and source-identical nonzero sectors.

use alloc::format;
use makos_installer::{Disk, InstallEvent, InstallMode};

const SOURCE: usize = 0;
const TARGET: usize = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallError {
    Permission,
    DeviceCount,
    Geometry,
    SourceNotGpt,
    TargetNotBlank,
    ResumeCommitted,
    ResumeConflict,
    SourceFreeze,
    Read,
    Write,
    Verify,
    Flush,
}

struct VirtioDisk(usize);

impl Disk for VirtioDisk {
    fn sectors(&self) -> u64 {
        crate::aarch64_virtio_blk::device_sectors(self.0).unwrap_or(0)
    }

    fn read_sector(&mut self, lba: u32, output: &mut [u8; 512]) -> bool {
        crate::aarch64_virtio_blk::read_sector_on(self.0, lba, output)
    }

    fn write_sector(&mut self, lba: u32, input: &[u8; 512]) -> bool {
        crate::aarch64_virtio_blk::write_sector_on(self.0, lba, input)
    }

    fn flush(&mut self) -> bool {
        crate::aarch64_virtio_blk::flush_on(self.0)
    }

    fn read_sectors_8(&mut self, lba: u32, output: &mut [u8; 4096]) -> bool {
        crate::aarch64_virtio_blk::read_sectors_8_on(self.0, lba, output)
    }

    fn write_sectors_8(&mut self, lba: u32, input: &[u8; 4096]) -> bool {
        crate::aarch64_virtio_blk::write_sectors_8_on(self.0, lba, input)
    }
}

pub fn install_disk1(mode: InstallMode) -> Result<makos_installer::InstallReport, InstallError> {
    if !crate::security::has_capability(crate::security::CAP_FILE_WRITE)
        || crate::security::session_username(&mut [0u8; makos_accounts::USERNAME_BYTES]).is_none()
    {
        return Err(InstallError::Permission);
    }
    if crate::aarch64_virtio_blk::device_count() != 2 {
        return Err(InstallError::DeviceCount);
    }

    let source_sectors =
        crate::aarch64_virtio_blk::device_sectors(SOURCE).ok_or(InstallError::DeviceCount)?;
    let target_sectors =
        crate::aarch64_virtio_blk::device_sectors(TARGET).ok_or(InstallError::DeviceCount)?;
    crate::serial_println!(
        "MAKOS_INSTALL_SCAN source=disk0 target=disk1 source_sectors={} target_sectors={} policy={}",
        source_sectors,
        target_sectors,
        match mode {
            InstallMode::Fresh => "exact-size,whole-disk-blank",
            InstallMode::Resume => "exact-size,mbr-blank,matching-sectors-only",
        },
    );

    let mut source = VirtioDisk(SOURCE);
    let mut target = VirtioDisk(TARGET);
    let source_freeze =
        crate::aarch64_virtio_blk::freeze_source_writes().ok_or(InstallError::SourceFreeze)?;
    let result = makos_installer::install(&mut source, &mut target, mode, |event| match event {
        InstallEvent::Ready { sectors } => crate::serial_println!(
            "MAKOS_INSTALL_BEGIN source=disk0 target=disk1 mode={} sectors={} commit=protective-mbr-last source_snapshot=flush,write-frozen",
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
            "MAKOS_INSTALL_PROGRESS target=disk1 copied_sectors={} total_sectors={}",
            copied_sectors,
            sectors,
        ),
    })
    .map_err(map_error);
    if result.is_ok() {
        source_freeze.keep_until_shutdown();
    }
    result
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
        InstallError::Permission => "permission denied; log in with an administrator account",
        InstallError::DeviceCount => "requires exactly live disk0 plus target disk1",
        InstallError::Geometry => "target must have exactly same sector count as live disk",
        InstallError::SourceNotGpt => "disk0 is not a valid MakOS GPT live system",
        InstallError::TargetNotBlank => "disk1 is not completely blank; refusing overwrite",
        InstallError::ResumeCommitted => "disk1 MBR is already committed; refusing resume",
        InstallError::ResumeConflict => "disk1 contains data not matching live source",
        InstallError::SourceFreeze => "could not flush and freeze live source disk",
        InstallError::Read => "disk read failed",
        InstallError::Write => "disk write failed",
        InstallError::Verify => "read-after-write verification failed",
        InstallError::Flush => "target flush failed",
    };
    crate::graphics::terminal_write(b"install: ");
    crate::graphics::terminal_write(message.as_bytes());
    crate::graphics::terminal_write(b"\n");
    crate::serial_println!("MAKOS_INSTALL_ERROR error={:?}", error);
}

pub fn success_message(report: makos_installer::InstallReport) {
    let mib = report.sectors.saturating_mul(512) / (1024 * 1024);
    crate::graphics::terminal_write(
        format!("Installed MakOS to disk1 ({mib} MiB). Shut down, detach live disk, boot disk1.\n")
            .as_bytes(),
    );
    crate::serial_println!(
        "MAKOS_INSTALL_OK source=disk0 target=disk1 sectors={} verified=read-after-write,gpt-boundaries flush=1 mbr_commit=last source_frozen=until-shutdown written_sectors={}",
        report.sectors,
        report.written_sectors,
    );
}
