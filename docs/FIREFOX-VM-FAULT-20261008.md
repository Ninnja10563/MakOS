# AArch64 concurrent page fault repair

The Mac/HVF run at `172975dc7d1e9a55d820ba3e7f34d8f51032756d` stopped
before Firefox painted with `duplicate AArch64 user-page mapping`. This
increment repairs concurrent demand-page publication and page-table creation
in the MakOS kernel. It preserves the strict duplicate guard, executable and
permission checks, all existing runtime gates, and Firefox provenance. Real
Firefox still requires qualification on the idle Apple Silicon Mac.

## Authoritative Mac failure

The supplied `BUILDER-HANDOFF.txt`, `FINAL-REPORT.txt`, and
`MakOS-172975dc-Mac-HVF-evidence-20261008.zip` describe an Apple M3 with
16 GiB, macOS 26.6.2, and QEMU 11.0.3/HVF. An initial unit/check attempt
failed on absent compiler-rt builtins. Following separate user authorization,
the agent prepared that prerequisite and restarted qualification. Unit/check,
image, EL0, self-host, Native/Python-role SMP, Firefox-role SMP/input, and
cursor then passed. The role fixtures are not browser/interpreter execution.

A fresh supported Mac Firefox release, all-five-ELF audit, 60-patch provenance,
packaging, integration, and runtime preflight passed. The authorized fresh
account/profile was created in the runtime's private data clone, not restored
or written into the unchanged integrated master. Preserve this Mac image:

```text
build/makos-fresh-firefox-VMF8ae/makos-integrated-3b68500032eec2d4.img
image SHA-256: 3b68500032eec2d4e0a225c44877a070f10de44e3f164cf0700f65bc827ecec3
manifest SHA-256: a9615cc28c989bfee754a1f4b448800fa22f5a4cd40f5b8a85d0eb743db0c7a1
old Mac boot SHA-256: c50849c4e089cb9f7ab6d7baff27f1e1d9853f653c50657fab3a6729f1daec54
```

The unchanged real-Firefox command exited 2 after 16.42 seconds. Final raw
serial, including cleanup drainage, contains:

```text
MAKOS_FAILURE_DETAIL: duplicate AArch64 user-page mapping
MAKOS_FATAL: duplicate AArch64 user-page mapping
```

The ZIP's `25-artifacts/firefox-runtime-latest-serial.log` is 27,377 bytes,
SHA-256 `28ee88e09caa25808caef8fea35dbf745b946dc424972a8e0f30cea679223189`.
The exact failing virtual address, caller, and interleaving were not recorded;
the preceding TID 8 stack/TLS record does not establish them. The observed
fatal and the source-confirmed races below are distinct evidence. The fresh
Mac build and idle-host report do not support blaming the Pi development host
or Apple Silicon as the cause.

No latency stage completed, no retry or final visible login ran, and no QEMU
remained. The Mac evidence directory is
`/Users/marcushuang/Desktop/Desktop/MakOS/build/logs/macos-hvf-172975dc7d1e-20261007`.
QEMU PID 58700 and harness PID 58694 used inherited QMP socketpair fd 4.
The agent retained private boot/data/vars under
`25-firefox-runtime-private/makos-aarch64-test-fckffjrh/` before normal
temporary-session cleanup. The ZIP excludes those large disks and release
caches; its Mac paths are not Pi paths.

## Kernel behavior

Previously, two faults could independently observe an absent page, populate
private frames, then call the strict mapper. A concurrent winner caused the
second call to fatal. Faults arriving after a permitted page became resident
also returned unresolved. Separately, different leaves in a new page-table
subtree could race its allocation/publication. Mapping metadata could change
while file I/O was in flight.

`kernel/src/arch/aarch64.rs` now serializes hierarchy creation and map/unmap/
protect publication under an IRQ-safe guard, using atomic descriptor reads
and writes. Nested guards restore the previous DAIF state. Broadcast TLB
maintenance is retained. `map_user_page_permissions_if_absent_in` returns the
existing descriptor without replacing it; its caller still owns the losing
candidate. The original strict mapping API retains its fatal and adds a
failure-only CPU/root/VA/candidate/existing-entry record. It does not ask the
scheduler for a PID while an outer VM lock may be held.

`kernel/src/aarch64_vm.rs` snapshots process root, mapping generation,
permissions, and backing while holding the VM lock. It allocates and loads
private data without that lock, then revalidates the generation and commits
under it. A stale candidate is freed and the fault is retried against current
metadata. A concurrent winner succeeds only when the actual mapping permits
the requested access. A failed private read can likewise accept an authorized
resident winner. Private losers are freed; shared-memory frames remain owned
by their object, including when a commit loses.

Unmap, fixed replacement, protect, discard, and brk now keep metadata and
PTE mutations in one VM transaction. Destructive mutations advance a
monotonic generation, covering same-address/same-permission remapping and
root/PID reuse. Non-overlapping new reservations need not invalidate existing
faults. Shared-object page lookup and publication remain under the VM lock so
unmap cannot release the final backing reference between them.

Lock order is VM metadata, then page tables, then physical allocator; the
shared-object path enters its object lock while holding VM metadata, before
page-table commit. No VM/page-table lock spans package/block I/O or scheduler
callbacks. This matters because AP storage requests depend on CPU0 service.
Large unmap/protect/brk transactions can hold interrupts masked for longer;
their interactive cost must be assessed by the unchanged Mac gate. This is
not a scalable per-address-space locking redesign. Eager-fork source-frame
lifetime, general user-buffer pinning, heap ownership after overlapping
MAP_FIXED, COW, swap, and broader memory-pressure support remain separate
incomplete work.

## Regression coverage

The normal `make unit check` includes three new suites:

- `scripts/test_aarch64_page_table_atomic.py`: eight tests execute extracted
  production mapping, hierarchy, access, unmap, and protect code on real host
  threads. Same-page and different-page shared-subtree contention, isolation,
  permission denial, and strict duplicates are covered. Removing the strict
  guard or commit lock in generated test copies must fail.
- `scripts/test_aarch64_vm_fault_commit.py`: 23 tests compile the complete
  production VM module and real RegionTable, adapting allocation, page-table
  storage, I/O, and services. Barriers force both same-page reads before
  commit. Tests cover loser reclamation, shared ownership, failed I/O with a
  concurrent winner, permissions, unmap/protect/discard/remap/root changes
  during I/O, generation ABA, unrelated changes, serialized brk, and metadata/
  PTE coherence. Five defect-restoring negative controls must fail.
- `scripts/test_aarch64_vm_fault_runtime.py`: 13 parser/emitter tests require
  a complete single-write record, matching old EL0 proof, exact worker/work
  counts, and checked format/write failures. Fragmented records remain
  rejected. Host `-Werror` and fortification remain enabled without redefining
  the SDK's `snprintf` macro.

`ports/musl/dynamic-probe.c` retains the existing three singleton AP threads,
96 blocking code calls, status-42 joins, and unchanged EL0 result emitter.
Before those calls it barrier-releases 16 rounds of same-page first writes
and 48 distinct-page first writes across fresh 2 MiB subtrees, then checks
all peers' data (288 checks). A 66 MiB untouched virtual reservation consumes
64 demand pages for the workload, plus page tables. The separate result is
emitted with one bounded checked write after joins and successful unmap.

`make test-aarch64-vm-fault-runtime` invokes the unchanged EL0 harness once,
with the same deadlines and checks, then validates this extra complete
record. Its retained private session contains boot/data/vars, `session.json`,
and final raw serial. The guest proves real concurrent first-touch behavior;
it does not count a guaranteed losing-fault interleaving. Deterministic host
tests force that specific interleaving. Neither fixture executes Firefox.

## Pi validation

Development host: Raspberry Pi, Debian 13.7/aarch64, Rust 1.98.0, GCC 14.2,
staged LLVM 19.1.7, QEMU 10.0.11/TCG, four guest CPUs and 1 GiB guest RAM.
These are functional Pi results, not Mac/HVF timing evidence.

Full `make unit check` passes (243.696 seconds), including all 44 new host
tests and seven defect-restoring negative controls. `make image-aarch64`
passes (115.106 seconds). An earlier full-check attempt exposed missing
architecture-facade re-exports; the wiring and its structural check were
fixed before the successful complete rerun. Both logs are retained. A
launcher attempt also found `/usr/bin/time` absent before Make ran; Bash's
timing builtin was used without altering any gate.

The unchanged EL0 gate passes (32.404 seconds) and the added VM-fault gate
passes (28.272 seconds). Both have TIDs 5,6,7 on AP1-3, root `0x4012f000`,
96 blocking calls, three status-42 joins/reap, complete old/new guest records,
and matching master/private boot hashes. The dedicated gate records
16 same-page rounds, 48 distinct pages, base `0x80200000`, 288 coherent
checks, and successful unmap. No deterministic guest losing-fault count is
claimed.

Retained sessions:

- `build/makos-el0-entry-12a2c0vp/`, QEMU PID 3039728.
- `build/makos-vm-fault-ybg4mnj7/makos-el0-entry-w_stsmki/`, QEMU PID 3039892.

Both retain private `boot.img`, `data.img`, `vars.fd`, final `serial.log`,
and `session.json`; QMP was inherited socketpair fd 4, and both guests exited
zero. Current Pi boot SHA-256 is
`88d1891a3e5c1611b158d57734474642738ccaea0e077d81166fb15195627a85`;
kernel SHA-256 is
`412f1f98faa18abd42d1e7e8ee14e03493e70536b316543f2371632c3da72093`.
The previous boot/kernel and fixed-name focused logs/cursor captures are
preserved under `build/vm-fault-repair-20261008-JjpFAQ/`; that directory also
holds the new command logs and host evidence.

The unchanged self-host gate passes (194.248 seconds): 20 CLI builds,
21 processes, ordered graphs `4,3,2,2,3,2,2,3`, exact generated headers,
three status-42 parallel children, and locked distinct-root overlap.
Placements are `7,7,7`, dispatches `176,175,174`, migrations 30, zero evidence
drops, owner compositions 42/AP deferrals 48/pending 0, and zero GPU delayed
recoveries/timeouts/errors. Master boot pre/post SHA-256 matches. QEMU
PID 3040039/harness 3040037 used session `makos-selfhost-focused-_7zmerkl`
and inherited QMP fd 4. Its normal temporary directory was removed, but
hardlinks retained the private boot/data/vars in the preservation directory's
`selfhost-private/`; `selfhost-serial.log` and process inventory are retained.

The unchanged Native/Python-role gate passes (35.864 seconds), with four
Native and one Python-role automatic migrations, zero evidence drops and
status 42 for both. QEMU PID 3040572/harness 3040569 used temporary session
`makos-native-smp-focused-r26zr434` and inherited QMP fd 4. Its private disks
were already removed by normal cleanup before preservation; no retained
native disk is claimed. Full serial and the live process inventory remain.

The unchanged Firefox-role input/SMP gate passes (35.608 seconds), with four
automatic migrations, zero evidence drops, exact syscall-149 Ctrl-A handoff,
and status 42. QEMU PID 3040783/harness 3040780 used temporary session
`makos-production-smp-focused-00kkptqy` and inherited QMP fd 4. Hardlinks
retained its private boot/data/vars under `production-private/` before normal
cleanup; full serial/process evidence is retained. These role fixtures do
not execute Firefox or CPython.

The unchanged cursor gate passes (21.150 seconds): seven positions, zero
changed scanout pixels, hardware cursor plane/hidden host cursor, and zero
delayed recoveries/timeouts/errors. Its baseline and seven captures plus
serial are preserved; its normal temporary disks were removed. No cursor
QEMU PID was captured before exit. All runtimes were sequential, and no
QEMU/build/test remains. Final boot/kernel hashes match the pre-runtime
identities above. No real-Firefox or final visible-login run occurred locally;
the freshly qualified Mac package image is not on this Pi.

## Mac retest

Use the exact pushed repair commit supplied in chat and the updated
[Mac testing protocol](MACOS-HVF-TEST-AGENT-PROMPT.md). Rebuild the boot image;
reuse the fresh `3b68500032eec2d4` integrated image only after its exact hash
and unchanged preflight pass. This kernel/probe repair changes no Firefox
source, patch series, release provenance, or packaged runtime; it does not
require rebuilding or restamping Firefox. Stop on the first failure.

The existing 600-second probe, first-character latency strictly below 500 ms,
Ctrl-A strictly below 10,000 ms, and all other assertions remain unchanged.
Only a passing strict Firefox run permits the final sole-QEMU visible login
from private clones. Audit Partial/Missing rows remain; MakOS is not complete.
