#!/usr/bin/env python3
"""Run production prefix-COW, commit/cache machinery and fsck on a fake disk.

Host adapters supply sparse sector I/O, injected failures and time only. No
guest/image is opened. Failures are followed immediately by crash/remount;
continued I/O after ambiguous failed root flush is NOT qualified. Preservation
means the original ACTIVE snapshot's retained prefix, not indefinite older-root
snapshots. This is not real-device crash or Firefox qualification.
"""
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = (ROOT / "kernel/src/makfs4_volume.rs").read_text()


def item(signature: str) -> str:
    start = SOURCE.index(signature)
    brace = SOURCE.index("{", start)
    depth = 0
    for index in range(brace, len(SOURCE)):
        depth += (SOURCE[index] == "{") - (SOURCE[index] == "}")
        if depth == 0:
            return SOURCE[start:index + 1]
    raise AssertionError(signature)


prefix = SOURCE[:SOURCE.index("#[derive(Clone, Copy, Debug, Eq, PartialEq)]\npub enum MountResult")]
# Exclude unrelated mount/package constants; everything retained is verbatim.
for name in ("PACKAGE_HEADER_LBA", "PACKAGE_MAGIC", "PACKAGE_TRANSACTION_BASE_LBA",
             "MIN_VOLUME_BLOCKS", "MAX_VOLUME_BLOCKS"):
    prefix = re.sub(rf"(?m)^const {name}:.*\n", "", prefix)
functions = (
    "fn rewrite_inode(", "fn timestamp_seconds(", "fn allocate_extent_map(",
    "fn validate_geometry(", "fn read_record(", "fn validate_root(",
    "const fn ranges_overlap(", "fn read_catalog(", "fn active_catalog(",
    "fn read_inode_from(", "fn load_inode_cache(", "fn cached_inode(",
    "fn cached_inode_record(", "fn update_inode_cache_after_commit(",
    "fn mark_inode_dirty(", "fn commit_dirty_snapshot(", "fn with_inode_cache<",
    "fn commit_inode_change(", "fn commit_inode_changes(", "fn select_commit_target(",
    "fn read_bitmap_words(", "fn read_inode_block(", "const fn format_error(",
    "fn active_superblock(", "fn metadata_sets(", "fn activate(",
    "fn write_catalog_record(", "fn write_inode_record(", "fn read_block(",
    "fn write_block(", "impl MutationGuard", "impl Drop for MutationGuard",
)
production = prefix + "\nstruct MutationGuard;\n" + "\n\n".join(map(item, functions))
assert "commit_inode_changes(" in production
assert "CommitPhase::FlushMetadata" in production and "CommitPhase::FlushRoot" in production

with tempfile.TemporaryDirectory(prefix="makos-prefix-cow-") as temporary:
    directory = Path(temporary)
    (directory / "production.rs").write_text(production)
    fixture = directory / "host.rs"
    fixture.write_text((ROOT / "scripts/test_makfs4_prefix_cow.rs").read_text())
    libraries = {}
    for name, crate_path, dependencies in (
        ("makos_makfs4", "crates/makfs4/src/lib.rs", ()),
        ("makos_gpt", "crates/gpt/src/lib.rs", ()),
        ("makos_makfs4_fsck", "crates/makfs4-fsck/src/lib.rs", ("makos_makfs4", "makos_gpt")),
    ):
        library = directory / f"lib{name}.rlib"
        command = ["rustc", "--edition=2024", "-Dwarnings", "--crate-type=rlib",
                   "--crate-name", name, str(ROOT / crate_path), "-o", str(library)]
        for dependency in dependencies:
            command += ["--extern", f"{dependency}={libraries[dependency]}"]
        command += ["-L", f"dependency={directory}"]
        subprocess.run(command, check=True)
        libraries[name] = library
    binary = directory / "prefix-cow-host"
    command = ["rustc", "--edition=2024", "-Dwarnings", "-C", "opt-level=1",
               str(fixture), "-o", str(binary), "-L", f"dependency={directory}"]
    for name, library in libraries.items():
        command += ["--extern", f"{name}={library}"]
    subprocess.run(command, check=True)
    subprocess.run([str(binary)], check=True, timeout=120)
    for name, mutated, expected_failure in (
        ("whole-file-rewrite", production.replace(
            "let mut prefix_blocks = unchanged_bytes / BLOCK_BYTES;",
            "let mut prefix_blocks = unchanged_bytes / BLOCK_BYTES * 0;", 1),
         "only planned COW suffix may be written"),
        ("missing-metadata-flush", production.replace(
            "if !disk.flush() {", "if false && !disk.flush() {", 1),
         "metadata and root must both be flushed"),
        ("release-retained-prefix", production.replace(
            "let (_, replaced) = inode.extents.split_at(prefix_blocks)",
            "let (_, replaced) = inode.extents.split_at(0)", 1),
         "real fsck must accept committed/remounted image"),
    ):
        assert mutated != production, f"{name}: production mutation not applied"
        (directory / "production.rs").write_text(mutated)
        subprocess.run(command, check=True)
        result = subprocess.run([str(binary)], text=True, capture_output=True, timeout=120)
        assert result.returncode != 0 and expected_failure in result.stdout + result.stderr, (
            f"{name}: negative control not rejected by intended assertion:\n{result.stdout}{result.stderr}"
        )

print("MAKOS_MAKFS4_PREFIX_COW_HOST_OK rewrite,commit,cache,codec,fsck=production "
      "disk,time=host-adapters negative_controls=whole-file-rewrite,missing-metadata-flush,release-retained-prefix "
      "scope=simulated-durability-not-device-or-firefox-qualification")
