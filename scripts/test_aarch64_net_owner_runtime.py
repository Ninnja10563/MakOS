#!/usr/bin/env python3
"""Host-only strict owner-progress evidence, private-image and fixture checks."""

from pathlib import Path
import struct
import tempfile
import unittest

import boot_test_aarch64_net_owner as probe


ROOT = Path(__file__).resolve().parents[1]
ARMED = probe.PREFIX + "ARMED config=test.net-owner=required\n"
PHASES = [probe.PREFIX + "PHASE_OK " + text + " transport=virtio-net-udp4\n" for text in (
    "phase=1 tid=1 root=0x4012e000 requester_cpu=1 service_cpu=0 timer_completions=0 "
    "lock_wait_completions=1 busy_deferrals=0 payload_bytes=18 irq=masked proof=socket-lock-contention",
    "phase=2 tid=2 root=0x4012e000 requester_cpu=1 service_cpu=0 timer_completions=1 "
    "lock_wait_completions=0 busy_deferrals=0 payload_bytes=18 irq=enabled proof=el1-timer",
    "phase=3 tid=3 root=0x4012e000 requester_cpu=1 service_cpu=0 timer_completions=1 "
    "lock_wait_completions=0 busy_deferrals=2 payload_bytes=18 irq=enabled proof=ready-locked-defer-unlock",
)]
REAPS = [f"process-reap arch=aarch64 pid={tid} status=42 closed_fds=0 closed_tty_fds=3 "
         "closed_surfaces=0 closed_sockets=0 closed_epolls=0 closed_ipc_handles=0 "
         "closed_futex_waiters=0 vm_regions=0 vm_pages=0 reclaimed_frames=73\n" for tid in (1, 2, 3)]
RESULT = (probe.PREFIX + "OK tids=1,2,3 phases=1,2,3 requester_cpu=1 service_cpu=0 "
          "owner_completions=3 ap_requests=3 timer_completions=2 lock_wait_completions=1 "
          "busy_deferrals=2 statuses=42,42,42 cleanup=reaped free_balance=1 "
          "transport=virtio-net-udp4 scope=immutable-boot-probe\n")
GOOD = ARMED + "".join(phase + reap for phase, reap in zip(PHASES, REAPS)) + RESULT


def item(source: str, declaration: str) -> str:
    start = source.index(declaration)
    end = source.index("{", start) + 1
    depth = 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


def small_fat_image() -> bytearray:
    # Host-only FAT fixture; it is deliberately not a bootable guest or result.
    image = bytearray(64 * 512)
    struct.pack_into("<H", image, 11, 512)
    image[13] = 1
    struct.pack_into("<H", image, 14, 32)
    image[16] = 2
    struct.pack_into("<I", image, 36, 1)
    struct.pack_into("<I", image, 44, 2)
    image[82:90] = b"FAT32   "
    image[510:512] = b"\x55\xaa"
    for copy in (32, 33):
        struct.pack_into("<I", image, copy * 512 + 3 * 4, 0x0fffffff)
    directory = 34 * 512
    image[directory:directory + 11] = b"MAKOS   CFG"
    image[directory + 11] = 0x20
    struct.pack_into("<H", image, directory + 26, 3)
    struct.pack_into("<I", image, directory + 28, len(probe.BASE_CONFIG))
    image[35 * 512:35 * 512 + len(probe.BASE_CONFIG)] = probe.BASE_CONFIG
    image[36 * 512:36 * 512 + 13] = b"KERNEL-BYTES!!"
    return image


class EvidenceTests(unittest.TestCase):
    def test_ordered_complete_evidence(self):
        self.assertEqual(probe.validate_output(GOOD), ((1, 2, 3), 2))
        self.assertEqual(probe.validate_output(GOOD.replace("\n", "\r\r\n")), ((1, 2, 3), 2))

    def test_missing_duplicate_or_fragmented_records(self):
        for record in (ARMED, *PHASES, RESULT):
            for changed in (GOOD.replace(record, ""), GOOD + record, GOOD + record[:40],
                            GOOD.replace(record, "partial-prefix" + record),
                            GOOD.replace(record, record.replace("=", "kernel-record\n=", 1))):
                with self.subTest(record=record), self.assertRaises(AssertionError):
                    probe.validate_output(changed)

    def test_out_of_order(self):
        for indexes in ((1, 0, 2), (0, 2, 1), (2, 1, 0)):
            changed = ARMED + "".join(PHASES[i] + REAPS[i] for i in indexes) + RESULT
            with self.subTest(indexes=indexes), self.assertRaisesRegex(AssertionError, "out of order"):
                probe.validate_output(changed)
        with self.assertRaises(AssertionError):
            probe.validate_output(RESULT + GOOD.replace(RESULT, ""))

    def test_source_and_irq_proof_cannot_be_substituted(self):
        for index, original, changed in (
            (0, "timer_completions=0", "timer_completions=1"),
            (0, "lock_wait_completions=1", "lock_wait_completions=0"),
            (0, "irq=masked", "irq=enabled"),
            (0, "busy_deferrals=0", "busy_deferrals=1"),
            (1, "timer_completions=1", "timer_completions=0"),
            (1, "irq=enabled", "irq=masked"),
            (1, "proof=el1-timer", "proof=el0-timer"),
            (2, "busy_deferrals=2", "busy_deferrals=0"),
            (2, "proof=ready-locked-defer-unlock", "proof=timer"),
        ):
            with self.subTest(index=index, original=original), self.assertRaises(AssertionError):
                probe.validate_output(GOOD.replace(PHASES[index], PHASES[index].replace(original, changed)))

    def test_exact_identity_transport_and_totals(self):
        for original, replacement in (("tid=1", "tid=0"), ("tid=2", "tid=1"),
            ("tids=1,2,3", "tids=1,2,4"), ("root=0x4012e000", "root=0x4012e001"),
            ("root=0x4012e000", "root=0x0"), ("requester_cpu=1", "requester_cpu=2"),
            ("service_cpu=0", "service_cpu=1"), ("owner_completions=3", "owner_completions=2"),
            ("ap_requests=3", "ap_requests=4"), ("payload_bytes=18", "payload_bytes=17"),
            ("statuses=42,42,42", "statuses=42,99,42"), ("free_balance=1", "free_balance=0"),
            ("transport=virtio-net-udp4", "transport=host-proxy")):
            with self.subTest(original=original), self.assertRaises(AssertionError):
                probe.validate_output(GOOD.replace(original, replacement))
        with self.assertRaisesRegex(AssertionError, "aggregate deferrals"):
            probe.validate_output(GOOD.replace(RESULT, RESULT.replace("busy_deferrals=2", "busy_deferrals=3")))

    def test_each_task_has_one_real_ordered_reap(self):
        for reap in REAPS:
            for changed in (GOOD.replace(reap, ""), GOOD.replace(reap, reap * 2),
                            reap + GOOD.replace(reap, ""),
                            GOOD.replace(reap, reap.replace("status=42", "status=99")),
                            GOOD.replace(reap, reap.replace("reclaimed_frames=73", "reclaimed_frames=0"))):
                with self.subTest(reap=reap), self.assertRaisesRegex(AssertionError, "ordered status-42 reap"):
                    probe.validate_output(changed)

    def test_fatal_or_truncated_final_cannot_pass(self):
        for suffix in ("MAKOS_FATAL:", "MAKOS_PANIC:", "MAKOS_FAILURE_DETAIL: reason"):
            with self.subTest(suffix=suffix), self.assertRaises(AssertionError):
                probe.validate_output(GOOD + suffix)
        with self.assertRaises(AssertionError):
            probe.validate_output(GOOD.rstrip())

    def test_immutable_workload_and_production_hook_order(self):
        net = (ROOT / "kernel/src/aarch64_virtio_net.rs").read_text()
        queue = item(net, "fn queue_tx_request(")
        before = "crate::aarch64_net_progress_probe::before_tx_publish("
        after = "crate::aarch64_net_progress_probe::after_tx_publish("
        publication = "slot.state.store(TX_SLOT_READY, Ordering::Release)"
        self.assertLess(queue.index(before), queue.index(publication))
        self.assertLess(queue.index(publication), queue.index(after))
        self.assertIn("counter_deadline_millis(5_000)", queue)
        self.assertIn('crate::fatal("AArch64 network TX owner request timeout")', queue)
        sockets = (ROOT / "kernel/src/aarch64_socket.rs").read_text()
        lock = item(sockets, "fn with_state<R>(")
        self.assertLess(lock.index("compare_exchange_weak"), lock.index("on_owner_socket_wait()"))
        self.assertLess(lock.index("on_owner_socket_wait()"), lock.index("service_tx_requests_while_waiting()"))
        module = (ROOT / "kernel/src/aarch64_net_progress_probe.rs").read_text()
        selected = item(module, "fn selected_request(")
        self.assertLess(selected.index("if tid == 0"), selected.index("current_tid()"))
        self.assertIn("ProcessRole::SmpProbe", selected)
        self.assertIn("payload != PAYLOADS", selected)
        controller = item(module, "pub(crate) fn run_phase(")
        self.assertIn("LocalInterruptMask::acquire()", controller)
        self.assertIn("crate::aarch64_socket::close(u64::MAX)", controller)
        self.assertIn("READY.load(Ordering::Acquire)\n                &&", controller)
        self.assertNotIn("service_tx_requests(", controller)
        self.assertNotIn("service_network_rx_on_owner_cpu(", controller)
        locked = item(controller, "crate::aarch64_virtio_net::with_progress_probe_state_lock(||")
        self.assertLess(locked.index("READY.load(Ordering::Acquire)"), locked.index("let deferred_before"))
        self.assertLess(locked.index("let deferred_before"), locked.index(".2 > deferred_before"))
        process = (ROOT / "kernel/src/aarch64_process.rs").read_text()
        run = item(process, "pub fn run_network_owner_progress_self_test(")
        self.assertIn("for phase in 1u8..=3", run)
        phases = item(run, "for phase in 1u8..=3")
        timer_start = "crate::arch::start_scheduler_timer();"
        timer_stop = "crate::arch::stop_scheduler_timer();"
        self.assertEqual(run.count(timer_start), 1)
        self.assertEqual(run.count(timer_stop), 1)
        self.assertLess(run.index(timer_start), run.index(phases))
        self.assertGreater(run.index(timer_stop), run.index(phases) + len(phases))
        self.assertIn("NET_OWNER_PROBE_ELF, u64::from(phase), ProcessRole::SmpProbe", run)
        self.assertIn("SMP_PROBE_AFFINITY[1].store(pid", run)
        self.assertIn("cleanup_reaped(pid, resource, status)", run)
        self.assertIn("status != 42", run)
        self.assertIn("crate::mm::free_frames() != free_before", run)
        self.assertNotIn("service_tx_requests(", run)
        self.assertNotIn("enter_user_context(", run)
        main = (ROOT / "kernel/src/main.rs").read_text()
        self.assertIn('"test.net-owner=required" => net_owner_probe = true', main)
        self.assertIn("aarch64_process::run_network_owner_progress_self_test();",
                      item(main, "if boot_options.net_owner_probe"))
        assembly = (ROOT / "user/aarch64_net_owner_probe.S").read_text()
        for number in (47, 48, 49, 51, 5):
            self.assertIn(f"mov x8, #{number}\n    svc #0", assembly)
        for payload in probe.PAYLOADS:
            self.assertIn(payload.decode().replace("\n", "\\n"), assembly)
        self.assertIn("mov x0, #42", assembly)
        self.assertNotIn("MAKOS_AARCH64_NET_OWNER_OK", assembly)


class PrivateImageTests(unittest.TestCase):
    def test_only_private_config_changes(self):
        with tempfile.TemporaryDirectory(prefix="makos-net-config-") as directory:
            source, private = Path(directory) / "master.img", Path(directory) / "private.img"
            baseline = small_fat_image()
            source.write_bytes(baseline)
            probe.arm_private_boot(source, private)
            self.assertEqual(source.read_bytes(), baseline)
            self.assertNotEqual(private.read_bytes(), baseline)
            offsets = probe.config_offsets(source)
            probe.assert_config_only_clone(source, private, *offsets)
            changed = bytearray(private.read_bytes())
            changed[36 * 512] ^= 1
            private.write_bytes(changed)
            with self.assertRaisesRegex(AssertionError, "outside the opt-in config"):
                probe.assert_config_only_clone(source, private, *offsets)

    def test_refuse_existing_or_master_destination(self):
        with tempfile.TemporaryDirectory(prefix="makos-net-config-") as directory:
            source = Path(directory) / "master.img"
            source.write_bytes(small_fat_image())
            with self.assertRaisesRegex(AssertionError, "new and private"):
                probe.arm_private_boot(source, source)
            private = Path(directory) / "private.img"
            private.touch()
            with self.assertRaisesRegex(AssertionError, "new and private"):
                probe.arm_private_boot(source, private)

    def test_reject_unsupported_or_ambiguous_config(self):
        for offset, replacement in ((11, b"\x00\x04"), (13, b"\x02"), (16, b"\x01"),
                                   (32 * 512 + 12, b"\x04\0\0\0"),
                                   (33 * 512 + 12, b"\x04\0\0\0"),
                                   (34 * 512 + 11, b"\x10"), (34 * 512 + 26, b"\x00\x00"),
                                   (35 * 512, b"BOOT")):
            with tempfile.TemporaryDirectory(prefix="makos-net-config-") as directory:
                source = Path(directory) / "master.img"
                changed = small_fat_image()
                changed[offset:offset + len(replacement)] = replacement
                source.write_bytes(changed)
                with self.subTest(offset=offset), self.assertRaises(AssertionError):
                    probe.config_offsets(source)


if __name__ == "__main__":
    unittest.main()
