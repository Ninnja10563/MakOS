# MakOS agent notes

## Development and runtime environments

- The repository workspace is currently hosted on a Raspberry Pi running Debian Linux. Treat this machine as a development/editing host, not as the target used to qualify MakOS interactive or performance behavior.
- MakOS will be run and tested on a macOS device using QEMU. The primary documented interactive target is AArch64 QEMU with Apple HVF on Apple Silicon.
- Functional AArch64 boot/runtime tests may also run on the Raspberry Pi under QEMU (KVM when permission is available, otherwise TCG). Record them as Raspberry Pi evidence and do not substitute them for the required macOS/HVF qualification.
- Do not reinterpret Raspberry Pi host timings as macOS/QEMU performance evidence. In particular, run strict Firefox latency gates unchanged on the intended idle macOS host with the required generated images and QEMU installation.
- Portable source, unit, structural, and cross-build checks may run on the Debian development host when their prerequisites are available. Record the actual host and accelerator for every runtime result.

## User testing handoff

- Notify the user in chat whenever MakOS reaches a large milestone that merits testing on their macOS/QEMU machine. Do not label routine internal changes as large milestones.
- Every large-milestone notification must include a self-contained, copy-paste prompt for a testing agent. The prompt must identify the exact commit, unchanged test command(s), required idle-host/QEMU conditions, expected evidence, failure artifacts to preserve, and the rule against concurrent QEMU instances.
- Firefox is the current qualification priority. The next macOS handoff is warranted when the Firefox scheduler/runtime increment is committed and pushed; strict Firefox latency thresholds must remain unchanged.
