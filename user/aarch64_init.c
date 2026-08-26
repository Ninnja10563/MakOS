#include <stddef.h>
#include <stdint.h>

enum {
    SYS_WRITE = 0,
    SYS_EXIT = 5,
    SYS_PROCESS_SPAWN = 14,
    SYS_PROCESS_WAIT = 15,
    SYS_VM_MAP = 21,
    SYS_VM_UNMAP = 22,
    SYS_CLOCK_MONOTONIC = 27,
    SYS_LOG_APPEND = 28,
    SYS_LOG_READ = 29,
    SYS_ABI_INFO = 31,
    SYS_VM_PROTECT = 45,
    SYS_VM_MAP_RANGE = 53,
    SYS_VM_UNMAP_RANGE = 54,
    SYS_VM_PROTECT_RANGE = 55,
    SYS_BRK = 76,
    SYS_SURFACE_WAIT_EVENT = 140,
    SYS_ROBUST_LIST = 141,
    SYS_SIGNAL = 142,
    SYS_TYPED_CHANNEL_RECEIVE = 147,
    SYS_THREAD_AFFINITY = 148,
    SYS_SURFACE_MAIN_HANDOFF_READY = 149,
    PROT_READ = 1,
    PROT_WRITE = 2,
    PROT_EXEC = 4,
};

#define AARCH64_ABI_FEATURES \
    ((UINT64_C(1) << 0) | (UINT64_C(1) << 1) | (UINT64_C(1) << 2) | \
     (UINT64_C(1) << 3) | (UINT64_C(1) << 4) | (UINT64_C(1) << 5) | \
     (UINT64_C(1) << 6) | (UINT64_C(1) << 7) | (UINT64_C(1) << 8) | \
     (UINT64_C(1) << 11) | (UINT64_C(1) << 14) | (UINT64_C(1) << 15) | \
     (UINT64_C(1) << 16) | (UINT64_C(1) << 17) | (UINT64_C(1) << 18) | \
     (UINT64_C(1) << 19) | (UINT64_C(1) << 20) | (UINT64_C(1) << 21) | \
     (UINT64_C(1) << 22) | (UINT64_C(1) << 23))

int aarch64_context_register_test(uint64_t role);

static uint64_t syscall2(uint64_t number, uint64_t first, uint64_t second) {
    register uint64_t x0 __asm__("x0") = first;
    register uint64_t x1 __asm__("x1") = second;
    register uint64_t x8 __asm__("x8") = number;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x1), "r"(x8) : "memory", "cc");
    return x0;
}

static uint64_t syscall4(uint64_t number, uint64_t first, uint64_t second,
                         uint64_t third, uint64_t fourth) {
    register uint64_t x0 __asm__("x0") = first;
    register uint64_t x1 __asm__("x1") = second;
    register uint64_t x2 __asm__("x2") = third;
    register uint64_t x3 __asm__("x3") = fourth;
    register uint64_t x8 __asm__("x8") = number;
    __asm__ volatile("svc #0"
                     : "+r"(x0)
                     : "r"(x1), "r"(x2), "r"(x3), "r"(x8)
                     : "memory", "cc");
    return x0;
}

static void vm_self_test(void) {
    const uint64_t page = 4096;
    uint64_t region = syscall4(SYS_VM_MAP_RANGE, 3 * page,
                               PROT_READ | PROT_WRITE, 0, 0);
    if (region < UINT64_C(0x80000000) || region >= UINT64_C(0x3c0000000))
        syscall2(SYS_EXIT, 116, 0);
    volatile uint8_t *bytes = (volatile uint8_t *)(uintptr_t)region;
    bytes[0] = 0x5a;
    bytes[3 * page - 1] = 0xa5;
    if (syscall4(SYS_VM_PROTECT_RANGE, region + page, page, PROT_READ, 0) != 1 ||
        syscall4(SYS_VM_PROTECT_RANGE, region, page,
                 PROT_READ | PROT_WRITE | PROT_EXEC, 0) != 0 ||
        syscall4(SYS_VM_UNMAP_RANGE, region, 3 * page, 0, 0) != 1)
        syscall2(SYS_EXIT, 115, 0);

    uint64_t legacy = syscall4(SYS_VM_MAP, 0, 0, 0, 0);
    if (legacy == UINT64_MAX)
        syscall2(SYS_EXIT, 114, 0);
    *(volatile uint8_t *)(uintptr_t)legacy = 0xc3;
    if (syscall4(SYS_VM_PROTECT, legacy, PROT_READ | PROT_EXEC, 0, 0) != 1 ||
        syscall4(SYS_VM_UNMAP, legacy, 0, 0, 0) != 1)
        syscall2(SYS_EXIT, 113, 0);

    uint64_t heap = syscall4(SYS_BRK, 0, 0, 0, 0);
    uint64_t grown = heap + 2 * page + 17;
    if (heap < UINT64_C(0x14000000) ||
        syscall4(SYS_BRK, grown, 0, 0, 0) != grown)
        syscall2(SYS_EXIT, 112, 0);
    *(volatile uint8_t *)(uintptr_t)(grown - 1) = 0x7e;
    if (syscall4(SYS_BRK, heap, 0, 0, 0) != heap ||
        syscall4(SYS_BRK, 0, 0, 0, 0) != heap)
        syscall2(SYS_EXIT, 111, 0);

    uint64_t reaped = syscall4(SYS_VM_MAP_RANGE, page,
                               PROT_READ | PROT_WRITE, 0, 0);
    if (reaped == UINT64_MAX)
        syscall2(SYS_EXIT, 110, 0);
    *(volatile uint8_t *)(uintptr_t)reaped = 0x39;
}

static void write_literal(const char *text, size_t length) {
    if (syscall2(SYS_WRITE, (uintptr_t)text, length) != length)
        syscall2(SYS_EXIT, 121, 0);
}

static void log_self_test(void) {
    static const char message[] = "aarch64 init online";
    static const char passed[] =
        "MAKOS_AARCH64_LOG_OK structured=1 ring=32 pid=1 severity=5 monotonic=1 readback=1\n";
    uint8_t output[80];
    uint64_t metadata[3];
    uint64_t before = syscall2(SYS_CLOCK_MONOTONIC, 0, 0);
    uint64_t sequence = syscall4(SYS_LOG_APPEND, 5, (uintptr_t)message,
                                 sizeof(message) - 1, 0);
    uint64_t count = syscall4(SYS_LOG_READ, sequence, (uintptr_t)output,
                              sizeof(output), (uintptr_t)metadata);
    uint64_t after = syscall2(SYS_CLOCK_MONOTONIC, 0, 0);
    int equal = count == sizeof(message) - 1;
    for (size_t index = 0; equal && index < sizeof(message) - 1; ++index)
        equal = output[index] == (uint8_t)message[index];
    if (sequence == UINT64_MAX || !equal || metadata[0] < before ||
        metadata[0] > after || metadata[1] != 1 || metadata[2] != 5)
        syscall2(SYS_EXIT, 122, 0);
    write_literal(passed, sizeof(passed) - 1);
}

__attribute__((noreturn)) void _start(uint64_t role) {
    static const char entered[] =
        "MakOS AArch64 EL0 init running\n";
    static const char passed[] =
        "MAKOS_AARCH64_USER_OK pid=1 el=0 elf=1 svc=1 write=1 abi=1 clock=1 isolation=ttbr0\n";
    static const char abi_passed[] =
        "MAKOS_AARCH64_ABI_OK version=1.0 normative_max=57 target_extension_max=149 features=ipc,process,vm,vfs,network,graphics,auth,log,sync,ipv6,selfhost-seed,sockets,packages,vm-regions,exec-path,startup-vectors,tty-signals,typed-ipc,cpu-affinity,surface-main-handoff truthful=1\n";
    static const char child_passed[] =
        "MAKOS_AARCH64_SCHED_CHILD_OK pid=2 register_restore=x0-x30,sp_el0,q0,q8,q16,q31,fpcr,fpsr preemptions=multiple pattern=child exit=42\n";
    static const char scheduler_passed[] =
        "MAKOS_AARCH64_SCHED_USER_OK parent=1 child=2 spawn=1 timer_only=1 concurrent=1 patterns=distinct wait=1 exit=42 register_restore=x0-x30,sp_el0,q0,q8,q16,q31,fpcr,fpsr preemptions=multiple\n";
    static const char vm_passed[] =
        "MAKOS_AARCH64_VM_OK map=1 unmap=1 protect=1 wx=denied brk=grow,shrink isolation=per-process reap=pending\n";

    if (syscall2(SYS_ABI_INFO, 0, 0) != UINT64_C(0x00010000) ||
        syscall2(SYS_ABI_INFO, 1, 0) != 57 ||
        syscall2(SYS_ABI_INFO, 2, 0) != AARCH64_ABI_FEATURES ||
        syscall2(SYS_ABI_INFO, 3, 0) != SYS_SURFACE_MAIN_HANDOFF_READY ||
        syscall2(SYS_CLOCK_MONOTONIC, 0, 0) == 0 ||
        syscall2(SYS_WRITE, UINT64_C(0x10200000), 1) != UINT64_MAX)
        syscall2(SYS_EXIT, 120, 0);
    if (role == 1) {
        if (!aarch64_context_register_test(role))
            syscall2(SYS_EXIT, 119, 0);
        vm_self_test();
        write_literal(child_passed, sizeof(child_passed) - 1);
        syscall2(SYS_EXIT, 42, 0);
        __builtin_unreachable();
    }

    write_literal(entered, sizeof(entered) - 1);
    write_literal(abi_passed, sizeof(abi_passed) - 1);
    log_self_test();
    uint64_t child = syscall2(SYS_PROCESS_SPAWN, 0, 0);
    if (child != 2)
        syscall2(SYS_EXIT, 118, 0);
    if (!aarch64_context_register_test(role))
        syscall2(SYS_EXIT, 119, 0);
    vm_self_test();
    uint64_t child_status = UINT64_MAX;
    while (child_status == UINT64_MAX)
        child_status = syscall2(SYS_PROCESS_WAIT, child, 0);
    if (child_status != 42)
        syscall2(SYS_EXIT, 117, 0);
    write_literal(scheduler_passed, sizeof(scheduler_passed) - 1);
    write_literal(vm_passed, sizeof(vm_passed) - 1);
    write_literal(passed, sizeof(passed) - 1);
    syscall2(SYS_EXIT, 42, 0);
    __builtin_trap();
}
