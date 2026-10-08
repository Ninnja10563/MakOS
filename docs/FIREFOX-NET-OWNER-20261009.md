# Firefox network owner progress repair

The attached Apple M3 QEMU/HVF qualification of
`20346e154bc2ef1a10d85e908633757940e01c33` passes its then-required pre-Firefox gates,
including the saved-BTYPE regression. Real Firefox then fails with
`AArch64 network TX owner request timeout`. This repair addresses a concrete
CPU0/AP circular wait in the source and adds request-state diagnostics. The
Mac trace does not identify the stalled request or prove that exact
interleaving; only the unchanged Mac browser gate can qualify the repair.

## Mac evidence

The host is Apple M3, 16 GiB, macOS 26.6.2 (25G83), QEMU 11.0.3/HVF.
The last prelaunch CPU sample is 90.98% idle with normal memory pressure and
unchanged swap usage. Low background application activity is disclosed in
the report; no host-pressure cause is established.

Unit/check, image, EL0, VM-fault, BTYPE, self-host, Native/Python-role,
Firefox-role, cursor and unchanged image/provenance preflight pass. The role
fixtures are not execution of Firefox or CPython. The single strict browser
run exits 2 after 193.36 seconds. A 700x400 native surface, real blits/presents
and Mozilla TLS traffic are partial startup evidence. `MAKOS_JIT_POOL_OK`
is absent, so the strict first-paint/client-pixel phase never completes.
There are no qualified input, Ctrl-A, navigation or visible-login results.

Final raw serial is 44,223 bytes, SHA-256
`0bf5fcf0e30393ffd24bae9583ad425bd1c889fc593db67092193a62b2c23aee`,
inside the supplied `MakOS-20346e15-Mac-HVF-evidence-20261009.zip` at
`13-firefox-runtime-artifacts/firefox-runtime-latest-serial.log`.
Both failure-detail and fatal records contain the complete timeout reason.
The preceding connect record is not an identification of the failing request.

The Mac evidence directory is
`/Users/marcushuang/Desktop/Desktop/MakOS/build/logs/macos-hvf-20346e154bc2-20261009`.
Failed QEMU PID 11179 and harness PID 11167 exited. The original session was
`build/makos-aarch64-test-9pk76zio`, QMP inherited socketpair fd 4. The harness
removed its original temporary directory; external hardlinks on the Mac
retain boot/data/vars. The ZIP excludes those disks. Mac paths are not Pi
paths, and retained evidence disks must not be booted writable.

The Mac boot SHA-256 is
`975721bde9f69ea17c9c8b9110e0d13cd0788e1f3d1409ce108be8c178ce4669`.
Its preserved integrated image remains
`build/makos-fresh-firefox-VMF8ae/makos-integrated-3b68500032eec2d4.img`,
SHA-256 `3b68500032eec2d4e0a225c44877a070f10de44e3f164cf0700f65bc827ecec3`.
All-five-ELF and 60-patch provenance passed without restamping or rebuilding.

## Lock dependency and repair

An AP may hold the socket-table lock during `send_to`, TCP receive-window
updates or shutdown control. Its copied transport request waits for CPU0 to
publish `DONE`, using the existing 5,000 ms deadline. Meanwhile CPU0 can
contend for that same socket lock in a syscall or RX demultiplexing. The old
contention loop only spins. Ordinary syscalls keep IRQs masked; additionally,
the old timer network service runs only after an EL0 interruption. Neither
can break this wait. The preliminary TX-before-RX check is a hint with a
check/acquire race, not a synchronization guarantee.

`aarch64_socket::with_state` now calls a TX-only owner progress helper after
a failed lock acquisition. It does not release a socket lock held by another
CPU or expose partially updated TCP sequence/window state. CPU0 completes
the copied low-level transport operation; the AP consumes the result,
updates its socket state and releases the socket lock normally.

The epoll lock has the same progress boundary: an AP can hold epoll while a
readiness callback waits for socket state, creating a three-CPU chain back
to CPU0. Its contention helper services copied network and existing block
requests only, since readiness callbacks also inspect VFS state. Host tests
force the epoll/socket/TX chain; the guest UDP phases target the direct socket
and timer paths, not a claimed guest epoll interleaving.

The CPU0 timer also services copied TX requests when it interrupts EL1.
The service defers if the network-device lock or an earlier owner-service
invocation is active. Full RX demultiplexing still runs only from safe
EL0/owner service points, never from the contention helper or EL1 timer TX
path. APs cannot service the device. Queue capacity, payload bounds, ownership,
release/acquire publication and the fatal 5,000 ms deadline stay unchanged.

Timeout diagnostics report CPU, slot, kind, state, length, device-lock and
owner-active flags and service counters. They read only atomics and the
requester's private request copy, not application payloads or socket/process
state. They can distinguish an unserviced request from one already inside a
transport operation. Slow ARP/NDP/TCP/device operations remain a separate
possibility that the old Mac trace cannot exclude.
This repair does not claim to close all TCP publication races: connection
setup releases socket state across transport setup and reacquires it to
publish the connection. A server-first greeting at that boundary is not
covered by the existing client-request/response TCP fixture.

## Regression and qualification scope

Twelve production-code host tests exercise the copied queue, dispatcher and actual
socket lock with hardware adapters. Defect-restoring negative controls must
reproduce the circular wait and reject missing ownership/recursion guards.
All six negative controls pass on Pi/Debian, including removing only epoll's
TX assistance while retaining socket assistance. The block hook at epoll
contention has structural/invocation coverage here, not a claimed block-I/O
interleaving. Eleven additional host tests reject fragmented, duplicated,
misordered or inconsistent runtime evidence and unsafe private-image edits.
The added opt-in guest gate sends real UDP datagrams through AP1 socket
syscalls and CPU0-owned virtio-net, covering masked socket contention,
current-EL timer progress and interrupted device-lock deferral. Existing
TCP request/response/FIN and all Firefox gates remain unchanged.

The initial Pi/TCG development run passed the masked socket phase and its
real host-received packet, then failed the EL1 timer phase. Boot deliberately
stops CPU0's timer when `enter_user_context` returns; the new fixture had
unmasked IRQs without arming that timer. The controller now starts/stops the
real scheduler timer around its phases, with a host wiring assertion. No
deadline, parser or production completion is bypassed. Failed session
`build/makos-net-owner-bmdytxf_` retains all disks, session JSON and final
serial (SHA-256 `ac75c22247617ad7ca1923f510334dc8bd0b40e79d09ca1fadf5a4f38b2b74b1`);
QEMU 3127695 and harness 3127688 exited. The failed Make took 101.796 s.

The corrected Pi/TCG gate passes in 25.447 s, with three exact host-received
18-byte UDP payloads, one lock-wait completion, two EL1 timer completions,
one locked-device deferral, three status-42 reaps and balanced frames. This
guest deliberately reuses the freed root across sequential processes; it
does not claim simultaneous distinct address spaces. Session
`build/makos-net-owner-rydv7jdl` retains boot/data/vars, raw serial and
`session.json`; QEMU 3128418 exited zero. QMP is inherited socketpair fd 4.
Its unchanged private boot SHA-256 is
`6a7b18d3c84542b0476e216cdd97725b1f0c5f44cfe3a348726dc630439689e2`,
different from the unchanged master only by the checked opt-in configuration.

An intermediate unchanged Pi/TCG EL0 gate failed in 58.964 s: all three
validated high-PC AP entries appeared, but the final musl result missed its
existing deadline. Worker exits 6 and 5 were recorded, with no fatal or EL0
rejection; that does not identify worker 7's state. Session
`build/makos-el0-entry-gq6gwxri` retains the full failure, disks and JSON;
QEMU 3134744 and harness 3134736 exited. A separate diagnostic-only run with
failure-only register capture passed in 26.081 s without invoking capture
(`build/makos-el0-entry-_g5w93hg`). This establishes intermittency, not a
fix or a cause for the failed run. Neither result is Mac/HVF evidence.

Review found an independent source-level AP idle race: after an empty
runnable check, the old loop enabled IRQs before `WFI`. An SGI could be
acknowledged in that interval, leaving the AP asleep with a Ready task and
its local timer stopped. The repair keeps PSTATE IRQ masking through the
idle instruction, then enables/acknowledges pending IRQs before rechecking
with IRQs masked. It does not mask interrupts at the GIC. Pending IRQs
masked by PSTATE are architectural WFI wake events (see page 7 of
[Arm's interrupt guidance](https://documentation-service.arm.com/static/5ed6547eca06a95ce53f9321)).
No live failure snapshot proves that the intermediate EL0 hang took this
path, and the Mac network timeout is not attributed to it.
The added host regression reads the production idle instruction sequence,
checks 20 interrupt-arrival/spurious-wake cases without a periodic timer,
and requires two old-order negative controls to lose the sole SGI. It also
cross-compiles the unmodified helper body to a real AArch64 ELF object and
checks its instruction encodings. This is instruction-model and object
coverage, not a claimed forced live-guest interleaving.

## Final-code Pi validation

Development host: Raspberry Pi, Debian 13.7, Linux
`6.18.39+rpt-rpi-v8`, QEMU 10.0.11 with TCG, four guest CPUs and 1 GiB guest
RAM. No concurrent QEMU or heavy build ran during the runtime gates.
`make image-aarch64` passes in 59.191 s; complete `make unit check` passes
in 219.522 s. This includes both new network suites, their six negative
controls, and the 20-case/two-negative idle regression. Existing tests,
Firefox source/provenance, runtime deadlines and assertions are unchanged.

The final Pi master boot SHA-256 is
`f712c55db34855535e004ffc2890aee0c12f1e3dcae948020a9cdb07ff35022b`;
kernel SHA-256 is
`5e42ef7cd3e3d359f895b320dfcb3616ecaf332a03b5c082bb29bd993b914b22`.
Each listed gate exits zero; the main master hash is unchanged before/after
each Make target. TCP/input use their existing config-specific boot clones.
Times include the original Make target and local evidence capture, not
browser latency.

| Pi/TCG Make target | Seconds | QEMU / harness PID | Inherited QMP fd |
| --- | ---: | --- | ---: |
| `test-aarch64-el0-entry-runtime` | 32.161 | 3144340 / 3144325 | 4 |
| `test-aarch64-vm-fault-runtime` | 32.383 | 3144580 / 3144565 | 4 |
| `test-aarch64-btype-runtime` | 30.549 | 3144827 / 3144806 | 4 |
| `test-aarch64-net-owner-runtime` | 22.948 | 3145059 / 3145039 | 4 |
| `test-aarch64-smp-tcp-runtime` | 28.196 | 3145218 / 3145201 | 5 |
| `test-aarch64-smp-input-runtime` | 27.337 | 3145432 / 3145413 | 4 |
| `test-aarch64-selfhost-runtime` | 198.354 | 3145639 / 3145608 | 4 |
| `test-aarch64-native-smp-runtime` | 38.815 | 3146949 / 3146926 | 4 |
| `test-aarch64-production-smp-runtime` | 35.585 | 3147213 / 3147199 | 4 |
| `test-aarch64-cursor-runtime` | 25.619 | 3147483 / 3147471 | 4 |

Logs and process/session metadata are retained under
`build/net-owner-repair-20261009-KJU8r6/` as `qualified-*.log` and
`qualified-runtime-results.json`. All three private disks for each gate
are hardlinked in its `qualified-<gate>-private/` directory before its
unchanged harness can clean up; the JSON records original paths and commands.
EL0, VM-fault, BTYPE and network-owner harnesses also retain their original
session JSON, serial and disks. These are Pi paths, not Mac artifacts.

The final network-owner session is `build/makos-net-owner-955cj22z`.
It receives all three exact UDP payloads, records two EL1 timer completions,
one lock-wait completion, one locked-device deferral and three status-42
reaps with frame balance. Private boot SHA-256 is
`89a1d9cee86322d48ccab80d06793612544037089c4218a4500c75af651dd63b`,
unchanged before/after; raw serial SHA-256 is
`6ad8b1d22f0eb51071c489f04a5a5d9d3ad5ee0a6194f1ce3fd9004189b36510`.
The TCP and SMP-input gates retain their actual transport/device proofs.
Self-hosting passes 20 CLI builds, 21 processes, graph sequence
`4,3,2,2,3,2,2,3`, exact generated headers, parallel status-42 children and
the locked distinct-root overlap snapshot. It records 31 migrations with
zero evidence drops and zero GPU errors/timeouts. Native/Python-role
migrations are 4/1; the Firefox-role fixture records 4 migrations and the
exact syscall-149 input handoff, all with zero evidence drops. These role
fixtures are not Firefox or CPython execution. Cursor passes all seven
positions with zero changed scanout pixels, errors or timeouts.

All ten final-code runtime gates pass sequentially. Earlier failed
development sessions are retained, not replaced by these results. No QEMU
remains running. Real Firefox and the final visible login were not run on
the Pi; its workspace does not contain the required Mac integrated image,
and Pi/TCG is not the interactive performance qualification target.

## Required Mac handoff

Follow the sequential [Mac test protocol](MACOS-HVF-TEST-AGENT-PROMPT.md)
at the exact pushed commit supplied in chat. Rebuild the kernel boot image;
reuse the preserved integrated master only after exact hash and unchanged
provenance verification. No Firefox source or provenance changes are needed.
Keep the 600-second probe, first-character below 500 ms, Ctrl-A below
10,000 ms and every other assertion. Stop at the first failed gate and
preserve final raw serial, including any `MAKOS_AARCH64_NET_TX_TIMEOUT`
record. Final visible login is allowed only after strict Firefox passes.

Pi/Debian and Pi/TCG results do not qualify Mac/HVF Firefox performance.
The original audit's Partial/Missing rows remain; this is not full OS
completion.
