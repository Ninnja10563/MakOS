# TCP peer-FIN receive contract

## Failed qualification preserved first

The RX-retention commit `92554836596589a89ceb39737d40e05ae35af35e` completed
one unchanged Mac/HVF qualification. All preliminary commands and the pinned
67-patch Firefox preflight exited zero. Real Firefox failed its first navigation
after verified paint at 39,072 ms, Ctrl-A at 5,905 ms and the first character at
110 ms. It did not reach target-page pixel comparison or target TLS completion.
No fatal reason was captured. This is not a Firefox fix or qualification pass.

The complete evidence, final raw serial and all 39 retained private disks are
in `build/logs/macos-rx-retention-20261018-2d7Wc5/`. Read `MANUAL-REVIEW.txt`
for physical-tail capture limitations; never reconstruct fragmented evidence.
Every QEMU exited. No retry, later gate or visible login followed the failure.

Before separate development, the exact baseline source and independent APFS
clones of the boot/kernel/loader and fixed serial/capture outputs were preserved
in `build/logs/tcp-eof-repair-20261018-BPYVLX/`. `prior-preservation.json`
records hashes and distinct inodes. The current integrated Firefox master and
manifest still match their pinned identities; accounts and profiles are untouched.

## Separate source-supported contract violation

The production TCPv4/TCPv6 ingestion paths accept a correctly sequenced peer FIN,
advance the receive sequence and publish a closed connection. The socket layer
drains any buffered payload, but then returns `None` rather than a zero-byte
result. Its readiness path reports HUP/RDHUP without readable EOF. The native
receive/read syscall paths interpret `None` as blocking admission or `-11`;
their existing `Some(0)` paths can return zero without changing those syscalls.

This differs from the drained orderly-shutdown contract in
[POSIX recv](https://pubs.opengroup.org/onlinepubs/9799919799/functions/recv.html).
Reset and local close must not be indiscriminately converted into orderly EOF.
Existing `closed` state represents all three, so a blanket `closed -> Some(0)`
change is not an acceptable repair.

Pinned baseline host reproduction is retained at
`build/logs/tcp-eof-contract-20261019-bG9Gz5/attempt-GDuzkK/` (the directory
uses the Mac's local date). It extracts the actual complete TCP ingest,
receive, receive_from and poll functions, connection/socket structures and
the TCP insertion closures from the production pump. Twelve controls pass;
eight IPv4/IPv6 contract cases fail with the intended assertions after valid
FIN, receive-sequence and ACK-adapter witnesses:

- `CONTRACT_ORDERLY_FIN_RECEIVE_ZERO`
- `CONTRACT_ORDERLY_FIN_READ_ZERO`
- `CONTRACT_ORDERLY_FIN_READABLE_EOF`
- `CONTRACT_ORDERLY_FIN_DRAIN_THEN_ZERO`

Compilation with `rustc --edition=2024 -Dwarnings` exits zero in 0.610306 s.
Each required contract failure exits 101, not a compiler failure or timeout.
Full sources, generated extraction, binary, outputs, statuses and timings remain
retained. An earlier extractor-signature typo failed before compilation and
is preserved separately; it is not evidence of a production failure.

The fixture uses explicit CPU0/PID1, successful ACK transport and no-op wake
adapters, plus typed TCP segments rather than a real NIC/parser. It executes
the production insertion closures, not the entire RX pump. The case named
`read` exercises the production `receive()` wrapper, not the native SYS_READ
handler or musl. Architecture/musl snapshots support source mapping only.
Real interrupts, packet delivery, scheduler wake behavior and Firefox causality
remain unproved. No occurrence of peer FIN in the failing browser channel was
established by the current diagnostic trace.

## Bounded implementation

The reviewed private candidate is applied to canonical production code. TCPv4
and TCPv6 connections carry explicit `peer_eof` state, initialized false by all
four owner/AP constructors. An accepted in-sequence FIN sets it only before a
terminal reset/local-close state; RST clears it. Socket receive drains buffered
payload before returning persistent zero-byte EOF. Poll reports readable EOF
only when EPOLLIN was requested. Synchronous FIN/RST metadata is kept coherent.

No syscall, allocator, lock, wake policy, packet acceptance, errno convention,
timeout or qualification assertion changed. Existing synchronous closed-to-zero
behavior, reset errno, full TCP half-close and post-terminal packet handling
remain outside this bounded increment. No browser causality is established.

The production-body regression is `scripts/test_aarch64_tcp_eof.py`, included
in both `make unit` and `make check`. Its 54 IPv4/IPv6 cases cover buffered-data
ordering, repeated EOF, reset/FIN/local-close ordering, sequence and tuple
rejection, overflow, wraparound, failed ACK adapter, requested masks and UDP
isolation. Twenty-nine expected runtime assertion failures exercise fifteen
distinct source transformations; four additional constructor mutations are
structural rejections, not executed connect paths. Explicit host adapters and
the native-syscall/NIC/guest limitations above remain authoritative. Each run
retains full inputs, generated sources, binaries, logs and command statuses in
a unique `build/logs/tcp-eof-host-*` directory.

The existing network-diagnostic erasure test still checks an exact whole-file
socket policy digest. Its reviewed baseline changes from historical
`f7d5050d02ea293ca192fb00650a71cc77784321ba23572884d98de13fafd367` to
`47bf6085e3f42410cabf61cf05bf40ff6a3b1868eab78b992d20b3ba78085f60`
solely for this intentional receive/readiness repair. The historical identity
is retained, the connect/diagnostic policy and architecture digest are unchanged,
and a new EOF-removal negative control increases its controls from ten to eleven.
Expectations are fixed reviewed literals, not computed from current source.

Private candidate evidence is in `build/logs/tcp-eof-repair-20261019-mkAnMy/`.
Canonical validation is in `build/logs/tcp-eof-repair-20261018-BPYVLX/`.
Fresh exact-commit Mac/HVF qualification is required before any guest claim;
reuse the unchanged 67-patch integrated image and all original gates.

## Development validation and emitted-code review

On Apple M3/macOS 26.6.2, all six recorded development commands exit zero
(observer zero): focused EOF 13.92 s, network diagnostics 5.57 s, owner check
9.44 s, full `make unit check` 497.52 s, `make image-aarch64` 15.72 s and final
focused EOF 14.75 s. The last focused run follows a host-inventory-only helper
change: `sw_vers` runs only on Darwin, avoiding a macOS dependency on Linux.
No Linux execution is claimed. Production inputs remain those pinned before
the full suite; final published-source qualification must rerun the full suite.

The marker is:

```text
MAKOS_AARCH64_TCP_EOF_HOST_OK cases=54 runtime_mutants=29 structural_mutants=4 canonical_source_unchanged=1 guest_qualification=0
```

Baseline/candidate host layouts are unchanged: TCPv4 28 bytes, TCPv6 40 bytes,
their Options 28/40, Socket 3,288 and socket State 4,615,184 bytes. Optimized
AArch64 symbols confirm unchanged locked socket storage `0x466c18` and network
storage `0x7148`. Inspected prologues keep receive/poll at `0x120`/`0xe0`, owner
connect v4/v6 at `0x6b0`/`0x710`, TX service at `0x6b0`, exception dispatcher
at `0x480` and main at `0x52710`. SVC grows by 32 bytes from `0x115f0` to
`0x11610`. CPU0/AP kernel stacks remain 1 MiB. This is selected emitted-frame
and storage review, not a complete callgraph, interrupt-depth or stack bound.
Full disassemblies, symbols, commands and binary identities are retained in
`optimized-code-review/` beneath the canonical validation directory.

Built kernel SHA-256:
`99ee634fb77b3817192cea7479c0f7b4f1865374f86bda8403751a4145f9106f`.
Built boot SHA-256:
`75441bc2ba048556386dbbfbfe6ffcb9a1a6e96b6555694bafacb38d8717f186`.
No guest has yet qualified this increment; no Firefox fix or audit upgrade is
claimed. No master image, account, profile or provenance was changed.
