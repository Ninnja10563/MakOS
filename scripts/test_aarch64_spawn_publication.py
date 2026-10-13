#!/usr/bin/env python3
"""Force AP dispatch before Toolchain credential binding, using production code.

Production spawn/admission/publication/discard and security routines are compiled
unchanged. The ELF loader, wake delivery, session lock and resource reclamation
are explicit host adapters, not guest execution or macOS qualification.
"""

from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
SECURITY = (ROOT / "kernel/src/security.rs").read_text()


def item(source: str, marker: str) -> str:
    start = source.index(marker)
    opening = source.index("{", start)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


admission = item(PROCESS, "fn startup_dispatch_allowed(")
publication = item(PROCESS, "fn publish_toolchain_startup(")
spawn = item(PROCESS, "pub fn spawn_toolchain(")
discard = item(PROCESS, "fn discard_spawned(")
install = item(PROCESS, "fn install_loaded_process(")
schedule = item(PROCESS, "fn schedule_next_for_cpu(")
initial = re.search(r"startup_ready: ([^,\n]+),", install).group(1)
assert initial == "role != ProcessRole::Toolchain"
assert "startup_ready: false" in item(PROCESS, "impl ContextSlot {")
assert schedule.index("!startup_dispatch_allowed(slot)") < schedule.index(
    "slot.role == ProcessRole::SmpProbe"
)
assert schedule.index("!startup_dispatch_allowed(slot)") < schedule.index(
    "slot.affinity_mask"
)
assert "return false;" in schedule.split("!startup_dispatch_allowed(slot)", 1)[1].split("}", 1)[0]
assert spawn.index("register_session_process(") < spawn.index("publish_toolchain_startup(pid)")
assert spawn.index("publish_toolchain_startup(pid)") < spawn.index("notify_idle_cpus()")
assert "discard_spawned(pid);" in spawn
assert "startup_ready: true" in item(PROCESS, "pub fn clone_thread(")

# Preserve actual capability values, role policy, binding lookup and clearing;
# only the SessionState storage/lock is replaced by the host adapter.
constants = []
for name in ("INIT_UID", "INIT_GID", "CAP_CONSOLE", "CAP_GRAPHICS", "CAP_IPC",
             "CAP_NETWORK", "CAP_PROCESS", "CAP_FILE_WRITE", "CAP_INPUT",
             "CAP_SYNC", "CAP_AUDIO", "CAP_SERVICE_PUBLISH"):
    constants.append(re.search(rf"^pub const {name}:.*;$", SECURITY, re.MULTILINE).group(0))
constants.append(re.search(r"^const MAX_SESSION_PROCESSES:.*;$", SECURITY, re.MULTILINE).group(0))
security = "\n".join(constants) + "\n"
security += "#[derive(Clone, Copy, Debug, Eq, PartialEq)]\n" + item(SECURITY, "pub struct Credentials {") + "\n"
for name in ("INIT_CREDENTIALS", "LOGIN_CREDENTIALS", "UNPRIVILEGED_CREDENTIALS"):
    marker = "pub const " if name == "INIT_CREDENTIALS" else "const "
    security += item(SECURITY, marker + name + ":") + ";\n"
security += "#[derive(Clone, Copy)]\n" + item(SECURITY, "struct CredentialBinding {") + "\n"
security += item(SECURITY, "impl CredentialBinding {") + "\n"
security += "#[derive(Clone, Copy, Debug, Eq, PartialEq)]\n" + item(SECURITY, "pub enum SessionProcessRole {") + "\n"
for marker in ("pub fn register_session_process(", "pub fn credentials_for_process(",
               "pub fn clear_process_credentials("):
    security += item(SECURITY, marker) + "\n"

with tempfile.TemporaryDirectory(prefix="makos-spawn-publication-") as temporary:
    directory = Path(temporary)
    library = directory / "libmakos_process_table.rlib"
    subprocess.run(
        ["rustc", "--edition=2024", "--crate-name", "makos_process_table", "--crate-type", "rlib",
         str(ROOT / "crates/process-table/src/lib.rs"), "-o", str(library)], check=True,
    )
    (directory / "test.rs").write_text((ROOT / "scripts/test_aarch64_spawn_publication.rs").read_text())
    (directory / "security_production.rs").write_text(security)

    def compile_fixture(name: str, ready: str = admission, creation: str = spawn,
                        release: str = discard) -> Path:
        (directory / "process_production.rs").write_text(
            "\n".join((ready, publication, creation, release)) +
            f"\nfn initial_startup_ready(role: ProcessRole) -> bool {{ {initial} }}\n"
        )
        binary = directory / name
        subprocess.run(
            ["rustc", "--edition=2024", "--test", "-D", "warnings", str(directory / "test.rs"),
             "--extern", f"makos_process_table={library}", "-o", str(binary)], check=True,
        )
        return binary

    subprocess.run([str(compile_fixture("spawn-publication-tests")), "--test-threads=1"],
                   check=True, timeout=30)

    def reject(name: str, test: str, diagnostic: str, **changes: str) -> None:
        result = subprocess.run([str(compile_fixture(name, **changes)), "--exact", test],
                                capture_output=True, text=True, timeout=15)
        output = result.stdout + result.stderr
        if result.returncode == 0 or diagnostic not in output:
            raise SystemExit(f"{name} did not reject restored startup defect:\n{output}")

    reject("missing-startup-admission", "first_ap_dispatch_observes_bound_credentials",
           "AP must never execute before its credentials are bound",
           ready=admission.replace("slot.is_some_and(|slot| slot.startup_ready)", "slot.is_some()"))
    reject("missing-publication-wake", "first_ap_dispatch_observes_bound_credentials",
           "published startup must send a fresh wake",
           creation=spawn.replace("    notify_idle_cpus();\n", ""))
    reject("credential-refusal-leaks-pending", "refused_credentials_reap_pending_task_and_root",
           "refused startup must reclaim its root",
           release=discard.replace("    cleanup_reaped(pid, resource, status);",
                                   "    let _ = (pid, resource, status);"))

print("MAKOS_AARCH64_SPAWN_PUBLICATION_HOST_OK startup=production-code "
      "process_table=production-code security=production-code "
      "elf,wake,session-lock,reclaim=host-adapters "
      "negative_controls=missing-admission,missing-wake,missing-reclaim")
