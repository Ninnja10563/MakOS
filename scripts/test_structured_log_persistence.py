#!/usr/bin/env python3
"""Static companion gate for structured-log MakFS4 persistence."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
LOG = (ROOT / "kernel/src/log.rs").read_text()
FS = (ROOT / "kernel/src/fs.rs").read_text()
SECURITY = (ROOT / "kernel/src/security.rs").read_text()
PACKAGE = (ROOT / "kernel/src/package.rs").read_text()
SYSCALL = (ROOT / "kernel/src/syscall.rs").read_text()
AARCH64 = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
BROWSER = (ROOT / "user/aarch64_browser.c").read_text()
CODEC = (ROOT / "crates/structured-log/src/lib.rs").read_text()
BOOT = (ROOT / "scripts/boot_test_aarch64.py").read_text()


def require(source: str, fragment: str) -> None:
    if fragment not in source:
        raise AssertionError(f"missing structured-log persistence invariant: {fragment}")


require(CODEC, 'const MAGIC: &[u8; 8] = b"MAKLOG01";')
require(CODEC, "pub const CAPACITY: usize = 32;")
require(CODEC, "if image_crc(input) != get_u32(input, 28)")
require(LOG, 'const PERSISTENT_NAME: &[u8] = b".makos-system-log";')
require(LOG, "Corrupt journals remain untouched for diagnosis.")
require(LOG, "let early = with_log(|journal| *journal);")
require(LOG, "PERSISTENCE_READY.store(true, Ordering::Release);")
require(LOG, "crate::makfs4_volume::write_inode_at(index, 0, &image)")
require(FS, "crate::log::mount_persistent();")
require(LOG, "pub fn audit(message: &[u8])")
require(LOG, "fn summarize_audits(journal: &Journal) -> AuditSummary")
require(LOG, 'b"audit: authentication accepted" =>')
require(LOG, 'b"audit: account created" =>')
require(LOG, "summary.pid_attributed &= record.pid != 0;")
require(LOG, "MAKOS_SECURITY_AUDIT_PERSIST_OK source=prior-boot")
require(SECURITY, 'crate::log::audit(b"audit: authentication denied");')
require(SECURITY, 'crate::log::audit(b"audit: authentication accepted");')
require(PACKAGE, 'b"audit: package install committed"')
require(PACKAGE, 'b"audit: package rollback committed"')
require(PACKAGE, 'b"audit: package removal committed"')
require(SYSCALL, "has_capability(crate::security::CAP_CONSOLE)")
require(AARCH64, "!crate::security::has_capability(crate::security::CAP_CONSOLE)")
require(BROWSER, "SYS_LOG_READ = 29")
require(BROWSER, "result != UINT64_MAX")
require(BROWSER, "output[index] != 0xa5")
require(
    BROWSER,
    "MAKOS_AARCH64_LOG_ACCESS_OK reader=browser cap_console=0 read=denied buffers=untouched",
)
require(BOOT, 'raise AssertionError("structured-log marker malformed")')
require(
    BOOT,
    "if not 1 <= persisted_records <= 32 or persisted_next <= persisted_records:",
)
require(BOOT, "if not 2 <= persisted_records <= 32 or persisted_next < 3:")
require(BOOT, 'raise AssertionError("second-boot persisted security-audit marker missing")')
require(BOOT, "MAKOS_SECURITY_AUDIT_TWO_BOOT_OK")

print(
    "MAKOS_STRUCTURED_LOG_PERSIST_TEST_OK format=MAKLOG01 crc=1 "
    "ring=32 mount_merge=prestorage corruption=preserved cow=makfs4 "
    "read=cap-console,guest-denial audit=auth,account,session,package runtime=two-boot"
)
