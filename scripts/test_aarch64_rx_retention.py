#!/usr/bin/env python3
"""Production-derived host regression of AArch64 RX retention.

Extract actual DeferredRx, seven waiters, shared matcher, poll_frame and raw RX
consumer. State ownership, architecture/MMIO fence and TX completion are explicit
host adapters. This is NOT QEMU, real interrupt, locking, or Firefox evidence.
Historical baseline and unchanged wire hashes pinned separately. Current source
hash and HEAD recorded; unchanged bodies/constants/acceptance predicates asserted.
Temporary inputs, generated sources and binaries cleaned on exit. Commands, full
stdout/stderr, status, timing and provenance emitted for the enclosing test log;
per-command files and metadata remain available while the temporary run is live.
"""
from pathlib import Path
import hashlib
import json
import re
import subprocess
import tempfile
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
PIN = "6c4ec7e74be6a2579eb354071002a9647aaca88e"
NET_PATH = "kernel/src/aarch64_virtio_net.rs"
WIRE_PATH = "kernel/src/aarch64_net_wire.rs"
NET_SHA = "fec1fa9277b944ee175aa4614c9cf234404092acd6d6c539d3e372579c0d4af2"
WIRE_SHA = "aab4d3aeb772c03aa68de65bcefc939cca02cee697926f134bb18cbba7e31a37"
CANDIDATE = ROOT / NET_PATH
WAITERS = ("configure_ipv6", "wait_dhcp", "resolve_ndp", "resolve_arp",
           "wait_udp", "wait_tcp", "wait_tcp_v6")
CONSTANTS = ("REG_QUEUE_NOTIFY", "REG_INTERRUPT_STATUS", "REG_INTERRUPT_ACK",
             "RECEIVE_QUEUE", "QUEUE_SIZE", "AVAIL_OFFSET", "USED_OFFSET",
             "VIRTIO_NET_HEADER", "BUFFER_BYTES", "WAIT_SPINS",
             "IPV6_CONFIG_SPINS", "DHCP_TRANSACTION")
UNCHANGED = ("#[derive(Clone, Copy)]\nstruct Queue {", "impl Queue {",
             "struct OwnedTcpSegment {", "impl OwnedTcpSegment {",
             "fn poll_receive(", "fn acknowledge_transport_interrupt(",
             "fn read8_memory(", "fn read16_memory(", "fn write16_memory(",
             "fn read32_memory(", "fn write32_memory(")
DECLARATIONS = ("struct DeferredRx {", "impl DeferredRx {", "fn wait_matching_frame(",
                "pub fn poll_frame(", *("fn " + name + "(" for name in WAITERS), *UNCHANGED)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def item(source, signature):
    assert source.count(signature) == 1, "unique declaration: " + signature
    start = source.index(signature)
    opening = source.index("{", start)
    depth, end = 1, opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


def constant(source, name):
    matches = re.findall(r"^const " + name + r": [^\n]+;$", source, re.M)
    assert len(matches) == 1, "unique constant: " + name
    return matches[0]


def compact(source):
    return re.sub(r"\s+", "", source)


def verify_structure(candidate, baseline):
    # Exact unchanged low-level dequeue/repost and owned TCP result handling.
    for signature in UNCHANGED:
        assert item(candidate, signature) == item(baseline, signature), "UNCHANGED " + signature
    for name in CONSTANTS:
        assert constant(candidate, name) == constant(baseline, name), "CONSTANT " + name
    assert constant(candidate, "WAIT_SPINS") == "const WAIT_SPINS: usize = 20_000_000;"
    assert constant(candidate, "IPV6_CONFIG_SPINS") == "const IPV6_CONFIG_SPINS: usize = 2_000_000;"
    assert constant(candidate, "QUEUE_SIZE") == "const QUEUE_SIZE: u16 = 16;"
    # Explicit reviewed acceptance predicates. No wildcard acceptance, altered
    # tuples, DHCP transaction/type, or reduced output-capacity checks permitted.
    expected = {
        "configure_ipv6": """|bytes| {
            wire::parse_router_advertisement(bytes).is_some_and(|advertisement| {
                let mut address = advertisement.prefix;
                address[8..].copy_from_slice(&link_local[8..]);
                address != [0; 16] && address[0] != 0xff
                    && advertisement.router_mac != [0; 6] && advertisement.prefix_length == 64
            })
        }""",
        "wait_dhcp": """|bytes| { wire::parse_dhcp(bytes, DHCP_TRANSACTION, mac)
            .is_some_and(|reply| reply.message_type == message_type) }""",
        "resolve_ndp": """|bytes| {
            wire::parse_neighbor_advertisement(bytes, ipv6, target).is_some() }""",
        "resolve_arp": """|bytes| {
            wire::parse_arp_reply(bytes, mac, ipv4, target).is_some() }""",
        "wait_udp": """|bytes| { wire::parse_udp(bytes).is_some_and(|packet| {
            packet.source_ip == remote_ip && packet.destination_ip == ipv4
                && packet.source_port == remote_port && packet.destination_port == local_port
                && packet.payload.len() <= output.len()
        }) }""",
        "wait_tcp": """|bytes| { wire::parse_tcp(bytes).is_some_and(|segment| {
            segment.source_ip == remote_ip && segment.destination_ip == ipv4
                && segment.source_port == remote_port && segment.destination_port == local_port
        }) }""",
        "wait_tcp_v6": """|bytes| { wire::parse_tcp_v6(bytes).is_some_and(|segment| {
            segment.source_ip == remote_ip && segment.destination_ip == ipv6
                && segment.source_port == remote_port && segment.destination_port == local_port
        }) }""",
    }
    assert candidate.count("wait_matching_frame(") == 8, "exact seven shared-matcher routes"
    for name in WAITERS:
        body = item(candidate, "fn " + name + "(")
        assert body.count("wait_matching_frame(") == 1 and "poll_receive(" not in body, "ROUTE " + name
        spins = "IPV6_CONFIG_SPINS" if name == "configure_ipv6" else "WAIT_SPINS"
        assert f"wait_matching_frame(state, &mut frame, {spins}," in body, "SPIN_ROUTE " + name
        assert compact(item(body, "|bytes| {")) == compact(expected[name]), "ACCEPTANCE " + name
    assert "deferred_rx: DeferredRx::EMPTY," in candidate, "production State initialization"
    state = item(candidate, "struct State {")
    assert "deferred_rx: DeferredRx," in state, "production State field"
    assert "crate::arch::cpu_index() != 0" in item(candidate, "pub fn poll_frame("), "RX owner check"


def exercise(directory):
    print("RX_WAIT_FIXED_RUN", directory, flush=True)
    commands = []

    def run(label, argv, timeout=30):
        prefix = directory / label
        command = [str(value) for value in argv]
        prefix.with_suffix(".argv.json").write_text(json.dumps(command, indent=2) + "\n")
        print("RX_WAIT_COMMAND", json.dumps(command), flush=True)
        started = time.monotonic()
        try:
            result = subprocess.run(command, cwd=ROOT, capture_output=True, timeout=timeout)
        except subprocess.TimeoutExpired as error:
            prefix.with_suffix(".stdout").write_bytes(error.stdout or b"")
            prefix.with_suffix(".stderr").write_bytes(error.stderr or b"")
            status = {"timeout": True, "elapsed_seconds": time.monotonic() - started}
            prefix.with_suffix(".status.json").write_text(json.dumps(status, indent=2) + "\n")
            commands.append({"label": label, "argv": command, **status})
            (directory / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
            print("RX_WAIT_TIMEOUT", label, json.dumps(status), flush=True)
            print((error.stdout or b"").decode(errors="replace"), end="", flush=True)
            print((error.stderr or b"").decode(errors="replace"), end="", flush=True)
            raise SystemExit("timeout never qualifies as contract failure: " + label) from error
        status = {"returncode": result.returncode, "elapsed_seconds": time.monotonic() - started,
                  "timeout": False}
        prefix.with_suffix(".stdout").write_bytes(result.stdout)
        prefix.with_suffix(".stderr").write_bytes(result.stderr)
        prefix.with_suffix(".status.json").write_text(json.dumps(status, indent=2) + "\n")
        commands.append({"label": label, "argv": command, **status})
        (directory / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
        print("RX_WAIT_EXIT", label, json.dumps(status), flush=True)
        print(result.stdout.decode(errors="replace"), end="", flush=True)
        print(result.stderr.decode(errors="replace"), end="", flush=True)
        return result

    head = run("00-head", ["git", "rev-parse", "HEAD"])
    assert head.returncode == 0, "cannot record current HEAD"
    current_head = head.stdout.decode().strip()
    original = run("01-baseline", ["git", "show", f"{PIN}:{NET_PATH}"])
    assert original.returncode == 0 and digest(original.stdout) == NET_SHA, "baseline pin"
    wire = run("02-wire", ["git", "show", f"{PIN}:{WIRE_PATH}"])
    assert wire.returncode == 0 and digest(wire.stdout) == WIRE_SHA, "wire pin"
    assert (ROOT / WIRE_PATH).read_bytes() == wire.stdout, "working wire must remain unchanged"
    candidate_bytes = CANDIDATE.read_bytes()
    print("RX_WAIT_SOURCE_PROVENANCE", json.dumps({
        "current_head": current_head, "baseline_commit": PIN,
        "candidate_path": str(CANDIDATE), "candidate_sha256_observed": digest(candidate_bytes),
        "baseline_net_sha256": NET_SHA, "wire_sha256": WIRE_SHA,
    }), flush=True)
    candidate = candidate_bytes.decode()
    baseline = original.stdout.decode()
    # Read current canonical source once; record its digest, not a perpetual full-file pin.
    (directory / "candidate-aarch64_virtio_net.rs").write_bytes(candidate_bytes)
    (directory / "baseline-aarch64_virtio_net.rs").write_bytes(original.stdout)
    (directory / "aarch64_net_wire.rs").write_bytes(wire.stdout)
    (directory / "test_aarch64_rx_retention.py").write_bytes(Path(__file__).read_bytes())
    fixture = (HERE / "test_aarch64_rx_retention.rs").read_bytes()
    (directory / "fixed-fixture.rs").write_bytes(fixture)
    verify_structure(candidate, baseline)

    pieces = [constant(candidate, name) for name in CONSTANTS]
    pieces.extend(item(candidate, signature) for signature in DECLARATIONS)
    production = "\n\n".join(pieces) + "\n"
    (directory / "production.rs").write_text(production)
    input_metadata = {
        "current_head": current_head, "baseline_commit": PIN, "baseline_net_sha256": NET_SHA, "wire_sha256": WIRE_SHA,
        "candidate_path": str(CANDIDATE), "candidate_sha256_observed": digest(candidate_bytes),
        "candidate_full_hash_pinned": False, "production_sha256": digest(production.encode()),
        "fixture_sha256": digest(fixture), "constants": CONSTANTS,
        "declarations": {signature: digest(item(candidate, signature).encode()) for signature in DECLARATIONS},
        "host_adapters": ["subset State", "Mutex<Option<State>> with_state",
                          "CPU index zero", "fatal panic", "TX records then succeeds",
                          "heap-backed MMIO/ring/buffers", "host fence and volatile MMIO"],
        "unchanged_wire": True, "wait_spins_changed": False,
        "scope": "source-contract-host-adapters-only; not guest or Firefox causation",
        "artifact_lifetime": "temporary; cleaned on exit; enclosing test log retains emitted evidence",
    }
    (directory / "inputs.json").write_text(json.dumps(input_metadata, indent=2) + "\n")
    print("RX_WAIT_INPUTS", json.dumps(input_metadata), flush=True)

    def compile_fixture(label, location):
        binary = location / "rx-wait-fixed"
        result = run("compile-" + label, ["rustc", "--edition=2024", "-Dwarnings",
                     location / "fixed-fixture.rs", "-o", binary], timeout=60)
        assert result.returncode == 0, "compile failure never qualifies: " + label
        return binary

    binary = compile_fixture("candidate", directory)
    cases = ["fifo", "match-udp", "match-tcp4", "match-tcp6"]
    cases += [f"loss-{waiter}-{other}" for waiter in ("udp", "tcp4", "tcp6")
              for other in ("udp", "tcp4", "tcp6")]
    cases += ["fifo-deferred", "short-output", "interior-reuse", "full-saved-match",
              "full-raw-untouched", "udp-short-payload", "malformed-length-handling"]
    cases += [f"reject-{kind}-{field}" for kind in ("udp", "tcp4", "tcp6")
              for field in ("source-ip", "destination-ip", "source-port", "destination-port")]
    cases += [f"bootstrap-{mode}-{kind}" for mode in ("match", "full")
              for kind in ("dhcp", "arp", "ndp", "ipv6")]
    results = []
    for case in cases:
        result = run("case-" + case, [binary, case, directory / ("input-" + case)])
        lines = result.stdout.decode(errors="replace").splitlines()
        assert result.returncode == 0 and "RX_WAIT_FIXED_OK case=" + case in lines, "case failed: " + case
        if case.startswith("loss-"):
            assert "RX_WAIT_MATCH_REACHED case=" + case + " descriptors=2 reposted=2" in lines
        if case.startswith("bootstrap-full-"):
            assert "RX_WAIT_HOST_INSPECTION_REPUBLISH case=" + case + " successful-boot=false" in lines
        results.append({"case": case, "returncode": 0})

    # Compile-clean production-body mutations: require exact runtime assertion
    # line AND exit101. Neither compiler rejection nor arbitrary panic qualifies.
    mutants = (
        ("lost-save", "state.deferred_rx.push(&output[..length]);", "let _ = length;",
         "loss-udp-tcp4", "CONTRACT_UNRELATED_RX_LOST"),
        ("broken-push-order", "self.order[self.count] = slot;", "self.order[0] = slot;",
         "fifo-deferred", "CONTRACT_DEFERRED_FIFO"),
        ("lifo-drain", "state.deferred_rx.take_at(0, output)",
         "state.deferred_rx.take_at(state.deferred_rx.count - 1, output)",
         "fifo-deferred", "CONTRACT_DEFERRED_FIFO"),
        ("wrong-selection", "return state.deferred_rx.take_at(position, output);",
         "let _ = position; return state.deferred_rx.take_at(0, output);",
         "interior-reuse", "CONTRACT_INTERIOR_MATCH"),
        ("short-output-dequeue", "if output.len() < length {\n            return None;",
         "if output.len() < length {\n            self.count -= 1; return None;",
         "short-output", "CONTRACT_SHORT_OUTPUT"),
        ("full-raw-consume", "if state.deferred_rx.full() {", "if state.deferred_rx.full() && false {",
         "full-raw-untouched", "CONTRACT_FULL_RAW_UNTOUCHED"),
    )
    for label, before, after, case, contract in mutants:
        assert production.count(before) == 1, "unique mutant site: " + label
        location = directory / ("mutant-" + label)
        location.mkdir()
        changed = production.replace(before, after)
        (location / "production.rs").write_text(changed)
        (location / "fixed-fixture.rs").write_bytes(fixture)
        (location / "aarch64_net_wire.rs").write_bytes(wire.stdout)
        (location / "mutation.json").write_text(json.dumps({
            "label": label, "before": before, "after": after, "case": case,
            "expected_exit": 101, "expected_exact_panic_line": contract,
            "production_sha256": digest(changed.encode()),
        }, indent=2) + "\n")
        mutant = compile_fixture(label, location)
        result = run("mutant-" + label, [mutant, case, location / "input"])
        stderr = result.stderr.decode(errors="replace").splitlines()
        assert result.returncode == 101 and contract in (line.strip() for line in stderr), \
            "runtime mutant not caught by exact contract: " + label
        if label == "lost-save":
            assert "RX_WAIT_MATCH_REACHED case=loss-udp-tcp4 descriptors=2 reposted=2" \
                in result.stdout.decode().splitlines(), "lost-save must reach desired reply first"
        results.append({"mutant": label, "returncode": 101, "contract": contract})

    before = "packet.destination_ip == ipv4"
    assert candidate.count(before) == 1
    changed = candidate.replace(before, "packet.destination_ip == remote_ip")
    (directory / "mutant-acceptance-candidate.rs").write_text(changed)
    try:
        verify_structure(changed, baseline)
    except AssertionError as error:
        assert str(error) == "ACCEPTANCE wait_udp", "unexpected structural rejection"
        structural_result = {
            "result": "expected-structural-rejection", "assertion": str(error),
            "compiled": False, "runtime_claim": False,
        }
        (directory / "mutant-acceptance-result.json").write_text(json.dumps(structural_result, indent=2) + "\n")
        print("RX_WAIT_STRUCTURAL_MUTANT", json.dumps(structural_result), flush=True)
    else:
        raise AssertionError("acceptance mutation not rejected")
    summary = {
        "cases_passed": len(cases), "runtime_mutants": len(mutants), "structural_mutants": 1,
        "results": results, "candidate_sha256_observed": digest(candidate_bytes),
        "scope": "source-contract-host-adapters-only", "guest": "not-run",
        "Firefox_cause": "not-established", "capacity_progress_guarantee": False,
        "current_head": current_head, "structural_result": structural_result,
    }
    (directory / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print("RX_WAIT_SUMMARY", json.dumps(summary), flush=True)
    print(f"RX_WAIT_FIXED_COMPLETE cases={len(cases)} runtime-mutants={len(mutants)} "
          "structural-mutants=1 guest=not-run Firefox-cause=not-established", flush=True)


def main():
    directory = None
    try:
        with tempfile.TemporaryDirectory(prefix="makos-rx-retention-") as temporary:
            directory = Path(temporary)
            exercise(directory)
    finally:
        if directory is not None:
            print("RX_WAIT_EPHEMERAL_CLEANUP", json.dumps({
                "directory": str(directory), "removed": not directory.exists(),
                "artifacts": "temporary sources/inputs/binaries/per-command metadata",
                "retained_evidence": "command output/status/timing/provenance emitted to enclosing test log",
            }), flush=True)


if __name__ == "__main__":
    main()
