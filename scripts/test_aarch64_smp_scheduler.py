#!/usr/bin/env python3
"""Offline invariants for AArch64 SMP probes and bounded production dispatch."""

from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TABLE = (ROOT / "crates/process-table/src/lib.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
SERIAL = (ROOT / "kernel/src/serial.rs").read_text()
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
INPUT = (ROOT / "kernel/src/aarch64_virtio_input.rs").read_text()
TTY = (ROOT / "kernel/src/aarch64_tty.rs").read_text()
SOCKET = (ROOT / "kernel/src/aarch64_socket.rs").read_text()
NET = (ROOT / "kernel/src/aarch64_virtio_net.rs").read_text()
BLOCK = (ROOT / "kernel/src/aarch64_virtio_blk.rs").read_text()
GPU = (ROOT / "kernel/src/aarch64_virtio_gpu.rs").read_text()
GRAPHICS = (ROOT / "kernel/src/graphics.rs").read_text()
SECURITY = (ROOT / "kernel/src/security.rs").read_text()
MAIN = (ROOT / "kernel/src/main.rs").read_text()
BUILD_RS = (ROOT / "kernel/build.rs").read_text()
NETWORK_PROBE = (ROOT / "user/aarch64_smp_network_rx_probe.S").read_text()
NETWORK_IRQ_OWNER_PROBE = (
    ROOT / "user/aarch64_smp_network_irq_owner_probe.S"
).read_text()
BLOCK_PROBE = (ROOT / "user/aarch64_smp_block_probe.S").read_text()
BLOCK_OWNER_PROBE = (ROOT / "user/aarch64_smp_block_owner_probe.S").read_text()
GPU_PROBE = (ROOT / "user/aarch64_smp_gpu_probe.S").read_text()
GPU_OWNER_PROBE = (ROOT / "user/aarch64_smp_gpu_owner_probe.S").read_text()
TCP_PROBE = (ROOT / "user/aarch64_smp_tcp_probe.S").read_text()
TCP_OWNER_PROBE = (ROOT / "user/aarch64_smp_tcp_owner_probe.S").read_text()
MIGRATION_PROBE = (ROOT / "user/aarch64_smp_migration_probe.S").read_text()
LOAD_PROBE = (ROOT / "user/aarch64_smp_load_probe.S").read_text()
SHELL = (ROOT / "user/aarch64_shell.c").read_text()
DESIGN = (ROOT / "docs/AARCH64-SMP-SCHEDULER.md").read_text()
MAKEFILE = (ROOT / "Makefile").read_text()
INPUT_RUNTIME = (ROOT / "scripts/boot_test_aarch64_smp_input.py").read_text()
TCP_RUNTIME = (ROOT / "scripts/boot_test_aarch64_smp_tcp.py").read_text()
MIGRATION_RUNTIME = (ROOT / "scripts/boot_test_aarch64_smp_migration.py").read_text()
PRODUCTION_RUNTIME = (ROOT / "scripts/boot_test_aarch64_production_smp.py").read_text()
NATIVE_RUNTIME = (ROOT / "scripts/boot_test_aarch64_native_smp.py").read_text()
MUSL_PTHREAD_PROBE = (ROOT / "ports/musl/pthread-probe.c").read_text()
INPUT_CONFIG = (ROOT / "boot/MAKOS-SMP-INPUT.CFG").read_text()
TCP_CONFIG = (ROOT / "boot/MAKOS-SMP-TCP.CFG").read_text()

for token in (
    "current: [Option<usize>; MAX_SCHEDULER_CPUS]",
    "pub fn activate_on(",
    "pub fn schedule_next_on(",
    "pub fn schedule_next_where_on(",
    "pub fn block_current_on(",
    "pub fn exit_current_on(",
    "pub fn current_pid_on(",
    "pub fn running_cpu(",
    "multicore_selection_never_runs_one_context_twice",
    "affinity_selection_leaves_rejected_tasks_ready_and_unowned",
):
    assert token in TABLE, token

for token in (
    "ACTIVE_USER_ROOTS",
    "pub(crate) fn cpu_index()",
    ".space 1024",
    "aarch64_enter_user_context:\n    msr daifset, #0xf",
    "add x10, x10, x11, lsl #8",
    '"tlbi vae1is, {page}"',
    "root_active_on_any_cpu(root)",
    "static SMP_USER_SCHEDULER_ENABLED: AtomicBool = AtomicBool::new(false);",
    "fn secondary_scheduler_idle() -> !",
    "fn init_secondary_timer_on_current_cpu()",
    "if cpu_index() == 0",
    "SMP_USER_SCHEDULER_ENABLED.store(true, Ordering::Release);",
    "SMP_USER_SCHEDULER_ENABLED.store(false, Ordering::Release);",
    "pub(crate) fn enable_production_userspace_scheduler()",
    "user_stack_pointer_valid_in(context.ttbr0, context.sp_el0)",
    "Keep EL1 IRQs masked across the assembly restore.",
    "unmasks IRQs atomically with ERET",
    "pub(crate) fn send_scheduler_ipi()",
    "pub(crate) fn service_input_on_owner_cpu()",
    "pub(crate) fn service_network_rx_on_owner_cpu()",
    "crate::aarch64_virtio_blk::service_requests_from_timer();",
    "stop_remote_group_member_from_irq(frame)",
    "stop_remote_group_member_on_el0_return(frame)",
):
    assert token in ARCH, token

assert ARCH.count("crate::aarch64_process::ipc_control_allowed()") == 4
for token in (
    "static SERIAL_LOCK: AtomicBool",
    "struct SerialGuard",
    '"msr daifset, #0xf"',
    "compare_exchange_weak(false, true, Ordering::Acquire",
    "SERIAL_LOCK.store(false, Ordering::Release)",
    '"msr daif, {saved}"',
):
    assert token in SERIAL, token
assert ARCH.count("crate::aarch64_virtio_input::poll()") == 1
assert "crate::aarch64_virtio_input::poll()" not in PROCESS
assert "crate::aarch64_virtio_input::poll()" not in TTY
assert "AArch64 virtio-input poll attempted from non-owner CPU" in INPUT
for token in (
    "const MMIO_GIC_INTID_BASE: u32 = 48;",
    "interrupt_id: MMIO_GIC_INTID_BASE + slot as u32",
    "pub(crate) fn owns_interrupt(interrupt_id: u32)",
    "pub(crate) fn acknowledge_interrupt(interrupt_id: u32)",
    "crate::arch::enable_virtio_mmio_interrupt(device.interrupt_id);",
    "delivery=gicv2-spi timer_fallback=100hz",
):
    assert token in INPUT, token
for token in (
    "const GICD_ITARGETSR: u64 = 0x800;",
    "const GICD_ICFGR: u64 = 0xc00;",
    "pub(crate) fn enable_virtio_mmio_interrupt(interrupt_id: u32)",
    "GICD_IGROUPR0 + bank_offset",
    "GICD_ISENABLER0 + bank_offset",
    "0b10 << config_shift",
    "AArch64 virtio-input SPI routed away from CPU0",
    "let direct = kind == 9;",
    "crate::aarch64_virtio_input::acknowledge_interrupt(intid);",
    "MAKOS_AARCH64_INPUT_IRQ_OK",
):
    assert token in ARCH, token
for token in (
    "const MMIO_GIC_INTID_BASE: u32 = 48;",
    "NETWORK_INTERRUPT_ID: AtomicU32",
    "crate::arch::enable_virtio_mmio_interrupt(interrupt_id);",
    "pub(crate) fn owns_interrupt(interrupt_id: u32)",
    "pub(crate) fn acknowledge_interrupt(interrupt_id: u32)",
    "MAKOS_AARCH64_NETWORK_IRQ_ROUTE_OK",
    "delivery=gicv2-spi timer_fallback=100hz",
):
    assert token in NET, token
for token in (
    "let network = crate::aarch64_virtio_net::owns_interrupt(intid);",
    "AArch64 virtio-net SPI routed away from CPU0",
    "crate::aarch64_virtio_net::acknowledge_interrupt(intid);",
    "MAKOS_AARCH64_NETWORK_IRQ_OK",
    "pub(crate) fn network_irq_evidence()",
):
    assert token in ARCH, token
for token in (
    "INPUT_IRQ_MARKER",
    "input_irq_intid not in (77, 78)",
    "delivery=gicv2-spi",
):
    assert token in PRODUCTION_RUNTIME, token
assert ARCH.count("crate::aarch64_socket::pump()") == 1
assert "crate::aarch64_socket::pump()" not in PROCESS
assert "AArch64 network RX pump attempted from non-owner CPU" in SOCKET
assert "AArch64 virtio-net RX poll attempted from non-owner CPU" in NET
assert "AArch64 virtio-net TX attempted from non-owner CPU" in NET
for token in (
    "TX_SERVICE_SLOTS",
    "fn queue_udp_send(",
    "pub fn service_tx_requests()",
    "TX_OWNER_COMPLETIONS",
    "TX_NONOWNER_REQUESTS",
    "if length > TX_SERVICE_PAYLOAD",
    "TX_KIND_TCP4_CONNECT",
    "TX_KIND_TCP4_SEGMENT",
    "fn queue_tcp4_segment(",
    "pub fn tcp_tx_affinity_evidence()",
    "pub fn tx_request_publication_pending()",
    'crate::fatal("AArch64 TCPv4 RX ingest attempted from non-owner CPU")',
):
    assert token in NET, token
for token in (
    "let socket = &mut state.sockets[index];",
    "socket_state=locked-publication",
    "no task",
):
    assert token in SOCKET + PROCESS, token
assert "AArch64 virtio-blk request attempted from non-owner CPU" in BLOCK
for token in (
    "SERVICE_SLOTS",
    "fn queue_request(",
    "pub fn service_requests_from_timer()",
    "fn service_requests(timer_service: bool)",
    "OWNER_COMPLETIONS",
    "NONOWNER_REQUESTS",
    "OWNER_READ_COMPLETIONS",
    "OWNER_WRITE_COMPLETIONS",
    "OWNER_FLUSH_COMPLETIONS",
    "TIMER_SERVICE_COMPLETIONS",
    "if STATE.lock.load(Ordering::Acquire)",
    "match (kind, input.as_ref(), output.as_ref())",
    "matches!(length, 512 | SERVICE_DATA_BYTES)",
):
    assert token in BLOCK, token
for token in (
    "AArch64 virtio-gpu MMIO attempted from non-owner CPU",
    "OWNER_SUBMISSIONS",
    "OWNER_TRANSFERS",
    "OWNER_FLUSHES",
    "require_owner_cpu();",
    "pub fn reset_service_affinity_evidence()",
    "pub fn service_affinity_evidence()",
):
    assert token in GPU, token
for token in (
    "DEFERRED_COMPOSE_PENDING",
    "GPU_NONOWNER_COMPOSE_DEFERRALS",
    "GPU_OWNER_DEFERRED_COMPOSES",
    "if crate::arch::cpu_index() != 0",
    "fn flush_scanout()",
    "DEFERRED_COMPOSE_PENDING.store(true, Ordering::Release)",
    "if crate::arch::cpu_index() != 0 {\n        compose(state);\n        return;",
    "with_lock(compose_owner);",
    "pub fn reset_gpu_service_affinity_evidence()",
    "pub fn gpu_service_affinity_evidence()",
    "scanout={} windows={} z_order=1 clipping=1 deferred={}",
):
    assert token in GRAPHICS, token

for token in (
    "fn scheduler_cpu() -> usize",
    "current_pid_on(scheduler_cpu())",
    "activate_on(scheduler_cpu(),",
    "schedule_next_for_cpu(scheduler_cpu())",
    "block_current_on(scheduler_cpu())",
    "exit_current_on(scheduler_cpu(),",
    "replace_current_resource_on(scheduler_cpu(),",
    "pub(crate) fn run_secondary_scheduler() -> !",
    "schedule_next_where_on(cpu, |info|",
    "slot.pid != slot.group_pid",
    "slot.role == ProcessRole::Firefox",
    "slot.role == ProcessRole::SmpProbe",
    "pub fn run_smp_userspace_self_test()",
    "SMP_PROBE_PEAK_MASK",
    "SMP_PROBE_IDLE_MASK",
    "SMP_PROBE_RESUME_MASK",
    "SMP_PROBE_FUTEX_IDLE_MASK",
    "SMP_PROBE_FUTEX_RESUME_MASK",
    "SMP_PROBE_AFFINITY",
    "FutexBlockResult::BspIdle",
    "fn secondary_scheduler_can_idle() -> bool",
    "runtime_stats().runnable <= 1 && !secondary_idle",
    "PRODUCTION_WORKER_CPU_MASK",
    "PRODUCTION_WORKER_REPORTED_MASK",
    "PRODUCTION_WORKER_GROUP_PID",
    "PRODUCTION_WORKER_ROLE",
    "PRODUCTION_WORKER_DISPATCHES",
    "PRODUCTION_WORKER_ACTIVE_CPU_MASK",
    "PRODUCTION_WORKER_ACTIVE_TIDS",
    "PRODUCTION_WORKER_OVERLAP_CPU_MASK",
    "PRODUCTION_WORKER_OVERLAP_TIDS",
    "ProcessRole::Toolchain",
    "cpu_dispatches: [u64; 4]",
    "compute_placement_cursor: u8",
    "fn least_loaded_compute_ap(&mut self) -> ComputePlacement",
    "TOOLCHAIN_CPU_MASK",
    "TOOLCHAIN_PLACEMENTS",
    "TOOLCHAIN_DISPATCHES",
    "reset_toolchain_smp_evidence",
    "toolchain_smp_evidence",
    "MAKOS_AARCH64_TOOLCHAIN_PLACEMENT_OK",
    "MAKOS_AARCH64_TOOLCHAIN_DISPATCH_OK",
    "MAKOS_AARCH64_TOOLCHAIN_SMP_OK",
    "console_gpu_handoff=ap-defer,cpu0-compose",
    "crate::graphics::service_deferred_actions()",
    "AArch64 toolchain escaped kernel-selected AP affinity",
    "fn production_worker_enter(",
    "fn production_worker_leave(",
    "fn production_ap_worker(slot: &ContextSlot)",
    "matches!(slot.role, ProcessRole::Firefox | ProcessRole::Native)",
    "tracked_production_worker(worker.role, worker.group_pid)",
    "AArch64 production worker acquired duplicate CPU ownership",
    "MAKOS_AARCH64_PRODUCTION_SMP_READY",
    "toolchain-leaders-least-loaded-ap",
    "roles=firefox,native,toolchain",
    "MAKOS_AARCH64_PRODUCTION_SMP_DISPATCH_OK",
    "MAKOS_AARCH64_FIREFOX_SMP_OVERLAP_OK",
    "MAKOS_AARCH64_PRODUCTION_SMP_OK",
    "MAKOS_AARCH64_NATIVE_SMP_OVERLAP_OK",
    "MAKOS_AARCH64_NATIVE_SMP_DISPATCH_OK",
    "MAKOS_AARCH64_NATIVE_SMP_OK",
    "tracked_production_worker(role, group_pid)",
    "pub fn spawn_firefox_smp_probe()",
    "pub fn spawn_native_smp_probe()",
    "fixture=upstream-musl-pthread role=firefox",
    "SMP_PROBE_IO_IDLE_MASK",
    "SMP_PROBE_IO_RESUME_MASK",
    "SMP_PROBE_IPC_IDLE_MASK",
    "SMP_PROBE_IPC_RESUME_MASK",
    "pub fn run_smp_ipc_self_test()",
    "MAKOS_AARCH64_SMP_IPC_OK",
    "REMOTE_GROUP_STOP_TARGET_MASK",
    "REMOTE_GROUP_STOP_ACK_MASK",
    "REMOTE_GROUP_STOP_EARLY_MASK",
    "REMOTE_GROUP_STOP_LOCK",
    "pub fn run_smp_exit_group_self_test()",
    "MAKOS_AARCH64_SMP_EXIT_GROUP_OK",
    "SMP_PROBE_EL1_ENTER_MASK",
    "SMP_PROBE_GROUP_STOP_DEFERRED_MASK",
    "hold_smp_exit_group_probe_in_el1",
    "pub fn run_smp_exit_group_el1_self_test()",
    "MAKOS_AARCH64_SMP_EXIT_GROUP_EL1_OK",
    "SMP_PROBE_CONCURRENT_EXIT_ARRIVED_MASK",
    "SMP_PROBE_CONCURRENT_EXIT_ACQUIRED_MASK",
    "pub fn run_smp_concurrent_exit_group_self_test()",
    "MAKOS_AARCH64_SMP_CONCURRENT_EXIT_GROUP_OK",
    "SMP_PROBE_SAME_GROUP_OWNER_MASK",
    "SMP_PROBE_SAME_GROUP_JOINED_MASK",
    "RemoteGroupStop::Joined",
    "join_remote_group_stop",
    "pub fn run_smp_same_group_exit_self_test()",
    "MAKOS_AARCH64_SMP_SAME_GROUP_EXIT_OK",
    "SMP_PROBE_INPUT_WAIT_TID",
    "SMP_PROBE_INPUT_IDLE_MASK",
    "SMP_PROBE_INPUT_BLOCKED_MASK",
    "SMP_PROBE_INPUT_RESUME_MASK",
    "reset_input_service_affinity_evidence",
    "input_service_affinity_evidence",
    "pub fn run_smp_input_device_self_test()",
    "pub fn run_smp_network_rx_self_test()",
    "pub fn run_smp_block_io_self_test()",
    "pub fn run_smp_gpu_self_test()",
    "pub fn run_smp_tcp_tx_self_test()",
    "MAKOS_AARCH64_SMP_TCP_TX_OK",
    "pub fn run_smp_forced_migration_self_test()",
    "pub(crate) fn migrate_smp_probe_from_exception(",
    "MAKOS_AARCH64_SMP_MIGRATION_OK",
    "pub fn run_smp_load_balancing_self_test()",
    "fn record_smp_load_dispatch(",
    "MAKOS_AARCH64_SMP_LOAD_OK",
    "MAKOS_AARCH64_SMP_GPU_OK",
    "MAKOS_AARCH64_SMP_BLOCK_OK",
    "register_smp_block_probe(waiter)",
    "create_inode(",
    "remove_inode(probe_inode)",
    "service_point=cpu0-timer-bottom-half",
    "MAKOS_AARCH64_SMP_NETWORK_RX_READY",
    "MAKOS_AARCH64_SMP_NETWORK_RX_OK",
    "SMP_NETWORK_IRQ_OWNER_PROBE_ELF",
    "reset_network_irq_evidence",
    "network_irq_evidence",
    "wake=cpu0-rx-irq,sgi",
    "MAKOS_AARCH64_SMP_INPUT_DEVICE_READY",
    "MAKOS_AARCH64_SMP_INPUT_DEVICE_OK",
    "makos_process_table::ProcessState::Blocked",
    "SMP_PROBE_SCHEDULER_REDISPATCH_MASK",
    "return_to_kernel(frame, 0)",
    "statuses != [40, 41, 42, 43]",
    "peak != 0b1111",
    "idle & 0b1110 != 0b1110",
    "resumed & 0b1110 != 0b1110",
    "futex_idle & 0b1110 != 0b1110",
    "futex_resumed & 0b1110 != 0b1110",
    "io_idle & 0b1110 != 0b1110",
    "io_resumed & 0b1110 != 0b1110",
    'asm!("dsb ish", "sev", options(nostack))',
):
    assert token in PROCESS, token

for token in (
    "aarch64_smp_network_rx_probe.S",
    "aarch64-smp-network-rx-probe.elf",
    "aarch64_smp_network_irq_owner_probe.S",
    "aarch64-smp-network-irq-owner-probe",
    "aarch64_smp_block_probe.S",
    "aarch64-smp-block-probe.elf",
    "aarch64_smp_block_owner_probe.S",
    "aarch64-smp-block-owner-probe.elf",
    "aarch64_smp_gpu_probe.S",
    "aarch64-smp-gpu-probe",
    "aarch64_smp_gpu_owner_probe.S",
    "aarch64-smp-gpu-owner-probe",
    "aarch64_smp_tcp_probe.S",
    "aarch64-smp-tcp-probe",
    "aarch64_smp_tcp_owner_probe.S",
    "aarch64-smp-tcp-owner-probe",
    "aarch64_smp_migration_probe.S",
    "aarch64-smp-migration-probe",
    "aarch64_smp_load_probe.S",
    "aarch64-smp-load-probe",
):
    assert token in BUILD_RS, token
for token in (
    "aarch64-smp-gpu-probe.elf",
    "aarch64-smp-gpu-owner-probe.elf",
    "aarch64-smp-tcp-probe.elf",
    "aarch64-smp-tcp-owner-probe.elf",
    "aarch64-smp-migration-probe.elf",
    "aarch64-smp-load-probe.elf",
):
    assert token in PROCESS, token
assert "run_smp_network_rx_self_test();" in MAIN
for token in (
    "mrs x19, cntvct_el0",
    "mrs x20, cntfrq_el0",
    "mov x0, #64",
):
    assert token in NETWORK_IRQ_OWNER_PROBE, token
for token in (
    "MAKOS_AARCH64_NETWORK_IRQ_OK intid=76",
    "delivery=gicv2-spi intid=76 entry=lower-el dispatch=direct",
    "network=cpu0-owned-udp-tx,dns-rx-irq-wake,intid76,direct-lower-el",
):
    assert token in INPUT_RUNTIME, token
assert "run_smp_block_io_self_test();" in MAIN
assert "run_smp_gpu_self_test();" in MAIN
assert "run_smp_tcp_tx_self_test();" in MAIN
assert "run_smp_forced_migration_self_test();" in MAIN
assert "run_smp_load_balancing_self_test();" in MAIN
for token in ("mov x8, #47", "mov x8, #49", "mov x8, #50", "mov x0, #63"):
    assert token in NETWORK_PROBE, token
for token in (
    '"/home/user/.smp-block-io"',
    "mov x8, #17",
    "mov x8, #97",
    "mov x8, #12",
    "mov x0, #65",
):
    assert token in BLOCK_PROBE, token
for token in ("add x19, x0, #100", "mov x0, #66"):
    assert token in BLOCK_OWNER_PROBE, token
for token in (
    "mov x8, #8",
    "mov x8, #9",
    "mov x8, #10",
    "mov x0, #67",
):
    assert token in GPU_PROBE, token
for token in ("add x19, x0, #100", "mov x0, #68"):
    assert token in GPU_OWNER_PROBE, token
for token in (
    "mov x8, #47",
    "mov x8, #48",
    "mov x8, #49",
    "mov x8, #50",
    "mov x8, #51",
    '"MAKOS_AP_TCP_TX\\n"',
    '"MAKOS_CPU0_TCP_RX\\n"',
    "mov x0, #69",
):
    assert token in TCP_PROBE, token
for token in ("add x19, x0, #100", "mov x0, #70"):
    assert token in TCP_OWNER_PROBE, token
for token in (
    "msr tpidr_el0, x9",
    "movi v0.16b, #0x5a",
    "mov x0, #2",
    "mov x8, #1",
    "fmov x9, d0",
    "mov x0, #71",
):
    assert token in MIGRATION_PROBE, token
for token in (
    "mov x20, #48",
    "mov x8, #1",
    "subs x20, x20, #1",
    "add x0, x19, #80",
):
    assert token in LOAD_PROBE, token
for token in (
    "pub(crate) fn register_smp_block_probe",
    "capabilities: CAP_FILE_WRITE",
    "pub(crate) fn register_smp_graphics_probe",
    "capabilities: CAP_GRAPHICS",
):
    assert token in SECURITY, token

# Any CPU0 compatibility wrapper here would silently mutate CPU0 ownership
# when same syscall/exception path executes on an AP.
for cpu0_wrapper in (
    ".current_pid()",
    ".activate(",
    ".schedule_next()",
    ".block_current()",
    ".exit_current(",
    ".replace_current_resource(",
):
    assert cpu0_wrapper not in PROCESS, cpu0_wrapper

assert "scheduler scope remains bounded" in DESIGN
assert "Firefox first paint/navigation" in DESIGN
assert '"test.smp-input=required" => smp_input_probe = true' in MAIN
assert '"test.smp-tcp=required" => smp_tcp_probe = true' in MAIN
assert "if boot_options.smp_input_probe" in MAIN
assert "if boot_options.smp_tcp_probe" in MAIN
assert "test-aarch64-smp-input-runtime: image-aarch64-smp-input" in MAKEFILE
assert "test-aarch64-smp-tcp-runtime: image-aarch64-smp-tcp" in MAKEFILE
assert "test-aarch64-smp-migration-runtime: image-aarch64" in MAKEFILE
assert "test-aarch64-smp-load-runtime: image-aarch64" in MAKEFILE
assert "test-aarch64-production-smp-runtime: image-aarch64" in MAKEFILE
assert "test.smp-input=required" in INPUT_CONFIG
assert "test.smp-tcp=required" in TCP_CONFIG
for token in (
    "MAKOS_AARCH64_SMP_INPUT_DEVICE_READY",
    'send_key(stream, "ctrl-k")',
    "MAKOS_AARCH64_SMP_INPUT_DEVICE_OK",
    "input_idle_mask=0x2 input_resume_mask=0x2 status=61 free_balance=1",
    "mmio_owner=cpu0 contention=ap-deferred owner_activity=",
    "rx_mmio_owner=cpu0 contention=ap-deferred owner_frames=",
    "tx_mmio_owner=cpu0 tx_transport=bounded-copy-queue owner_transmits=",
    "tcp_ap_tx=cpu0-service-ready runtime=separate-tcp4-probe",
    "MAKOS_AARCH64_SMP_BLOCK_OK requester_cpu=1 service_cpu=0",
    "device=virtio-blk requests=read4k,write4k,fsync ring_activity=real",
    "mmio_owner=cpu0 transport=bounded-copy-queue service_point=cpu0-timer-bottom-half",
    "read_completions=",
    "write_completions=",
    "flush_completions=",
    "timer_completions=",
    "wait=bounded-el1-wfe status=65",
    "MAKOS_AARCH64_SMP_GPU_OK presenter_cpu=1 service_cpu=0",
    "device=virtio-gpu request=surface-create,fill,present ring_activity=real",
    "mmio_owner=cpu0 contention=ap-deferred service_point=cpu0-timer-bottom-half",
    "owner_composes=",
    "transfer_completions=",
):
    assert token in INPUT_RUNTIME, token

for token in (
    "MAKOS_AARCH64_SMP_TCP_TX_OK requester_cpu=1 service_cpu=0",
    "MAKOS_AARCH64_SMP_TCP_TX_EVIDENCE",
    "MAKOS_AARCH64_SMP_TCP_WAKE_OK",
    "TCP_REQUEST = b\"MAKOS_AP_TCP_TX\\n\"",
    "TCP_RESPONSE = b\"MAKOS_CPU0_TCP_RX\\n\"",
    "connect_completions=1 data_completions=1 ack_completions=1 fin_completions=1",
    "host fixture did not observe the guest FIN",
    "time.sleep(0.5)",
    "MAKOS_AARCH64_SMP_TCP_RUNTIME_OK",
):
    assert token in TCP_RUNTIME, token

for token in (
    "MAKOS_AARCH64_SMP_MIGRATION_OK tid=",
    "source_cpu=1 target_cpu=2 source_mask=0x2 target_mask=0x4",
    "migrations=1 ownership=running,ready-unowned,running",
    "context=gpr,sp,tls,simd status=71",
    "MAKOS_AARCH64_SMP_MIGRATION_RUNTIME_OK",
    "MAKOS_AARCH64_SMP_LOAD_OK tasks=6 worker_cpus=3 cpu_mask=0xe",
    "contention=yields:288 run_queue=shared-ready",
    "selection=per-cpu-round-robin ownership=exclusive",
    "MAKOS_AARCH64_SMP_LOAD_RUNTIME_OK",
):
    assert token in MIGRATION_RUNTIME, token

for token in (
    "syscall4(SYS_PROCESS_SPAWN, 17, 0, 0, 0)",
    "MAKOS_AARCH64_FIREFOX_SMP_REAP_OK",
    'exact(command, command_length, "firefox-smp")',
    "syscall4(SYS_PROCESS_SPAWN, 18, 0, 0, 0)",
    "MAKOS_AARCH64_NATIVE_SMP_REAP_OK",
    'exact(command, command_length, "native-smp")',
):
    assert token in SHELL, token

for token in (
    "production_smp_overlap_probe",
    'strcmp(argv[1], "production-smp")',
    "MAKOS_FIREFOX_SMP_PTHREAD_OVERLAP_OK workers=3",
    "native_smp_overlap_probe",
    'strcmp(argv[1], "native-smp")',
    "MAKOS_NATIVE_SMP_PTHREAD_OVERLAP_OK workers=3",
    "__atomic_fetch_or(&production_smp_ready",
):
    assert token in MUSL_PTHREAD_PROBE, token

for token in (
    "MAKOS_AARCH64_PRODUCTION_SMP_READY",
    "MAKOS_AARCH64_FIREFOX_SMP_PROCESS_OK",
    "MAKOS_AARCH64_FIREFOX_SMP_OVERLAP_OK",
    "MAKOS_AARCH64_PRODUCTION_SMP_OK",
    "MAKOS_AARCH64_FIREFOX_SMP_REAP_OK",
    "worker_cpus=",
    "fixture=upstream-musl-pthread",
    "role=firefox",
    "leader_cpu=0",
    "device_mmio_owner=cpu0",
    "ownership=exclusive",
    "concurrent=1",
    "block=ap-idle",
    "status=42",
):
    assert token in PRODUCTION_RUNTIME, token

for token in (
    "MAKOS_AARCH64_NATIVE_SMP_PROCESS_OK",
    "MAKOS_AARCH64_NATIVE_SMP_OVERLAP_OK",
    "MAKOS_NATIVE_SMP_PTHREAD_OVERLAP_OK",
    "MAKOS_AARCH64_NATIVE_SMP_OK",
    "MAKOS_AARCH64_NATIVE_SMP_REAP_OK",
    "cpu_mask != 0xE",
    "overlap_mask.bit_count() < 2",
    "role=native",
    "leader_cpu=0",
    "device_mmio_owner=cpu0",
    "status=42",
):
    assert token in NATIVE_RUNTIME, token

assert "test-aarch64-native-smp-runtime: image-aarch64" in MAKEFILE

print(
    "MAKOS_AARCH64_SMP_SCHED_FOUNDATION_OK process_table=per-cpu-current "
    "exception_paths=per-cpu kernel_return=per-cpu ttbr_cache=per-cpu "
    "tlbi=inner-shareable "
    "runtime=boot-probe,production-firefox-native-workers,toolchain-leaders,4cpus "
    "policy=interactive-leaders-cpu0,application-workers-ap-eligible,"
    "toolchain-least-loaded-ap device_mmio_owner=cpu0"
)
