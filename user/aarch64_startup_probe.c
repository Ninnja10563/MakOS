#include <stddef.h>
#include <stdint.h>

enum {
    SYS_WRITE = 0,
    SYS_YIELD = 1,
    SYS_EXIT = 5,
    SYS_SET_TID_ADDRESS = 78,
    SYS_GETTID = 79,
    AT_NULL = 0,
    AT_PHENT = 4,
    AT_PHNUM = 5,
    AT_PAGESZ = 6,
    AT_ENTRY = 9,
    AT_UID = 11,
    AT_EUID = 12,
    AT_GID = 13,
    AT_EGID = 14,
    AT_CLKTCK = 17,
    AT_SECURE = 23,
    AT_RANDOM = 25,
    AT_EXECFN = 31,
};

extern void _start(void);
static uint32_t clear_child_tid = 0xffffffffu;
static uint64_t tls_sentinel;

static uint64_t syscall2(uint64_t number, uint64_t first, uint64_t second) {
    register uint64_t x0 __asm__("x0") = first;
    register uint64_t x1 __asm__("x1") = second;
    register uint64_t x8 __asm__("x8") = number;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x1), "r"(x8) : "memory", "cc");
    return x0;
}

static size_t length(const char *value) {
    size_t count = 0;
    while (value[count])
        ++count;
    return count;
}

static int equal(const char *left, const char *right) {
    size_t index = 0;
    while (left[index] && right[index] && left[index] == right[index])
        ++index;
    return left[index] == right[index];
}

static void exit_now(uint64_t status) {
    syscall2(SYS_EXIT, status, 0);
    for (;;)
        __asm__ volatile("wfe");
}

void startup_main(uint64_t register_argc, char **register_argv,
                  char **register_envp, uint64_t *stack) {
    uint64_t argc = stack[0];
    char **argv = (char **)&stack[1];
    if (argc != 3 || register_argc != argc || register_argv != argv ||
        !equal(argv[0], "/system/startup-probe") ||
        !equal(argv[1], "alpha") || !equal(argv[2], "42") || argv[3])
        exit_now(101);

    char **envp = &argv[argc + 1];
    if (register_envp != envp || !equal(envp[0], "MODE=test") || envp[1])
        exit_now(102);

    uint64_t *auxv = (uint64_t *)&envp[2];
    uint64_t page_size = 0, entry = 0, phent = 0, phnum = 0;
    uint64_t uid = UINT64_MAX, euid = UINT64_MAX;
    uint64_t gid = UINT64_MAX, egid = UINT64_MAX;
    uint64_t clock_tick = 0, secure = UINT64_MAX;
    const uint8_t *random = 0;
    const char *execfn = 0;
    for (size_t pair = 0; pair < 32 && auxv[pair * 2] != AT_NULL; ++pair) {
        uint64_t kind = auxv[pair * 2];
        uint64_t value = auxv[pair * 2 + 1];
        if (kind == AT_PHENT) phent = value;
        if (kind == AT_PHNUM) phnum = value;
        if (kind == AT_PAGESZ) page_size = value;
        if (kind == AT_ENTRY) entry = value;
        if (kind == AT_UID) uid = value;
        if (kind == AT_EUID) euid = value;
        if (kind == AT_GID) gid = value;
        if (kind == AT_EGID) egid = value;
        if (kind == AT_CLKTCK) clock_tick = value;
        if (kind == AT_SECURE) secure = value;
        if (kind == AT_RANDOM) random = (const uint8_t *)(uintptr_t)value;
        if (kind == AT_EXECFN) execfn = (const char *)(uintptr_t)value;
    }
    if (page_size != 4096 || entry != (uint64_t)(uintptr_t)&_start ||
        phent != 56 || phnum == 0 || uid != euid || gid != egid ||
        clock_tick != 100 || secure != 0 || !random || !execfn ||
        !equal(execfn, argv[0]))
        exit_now(103);
    uint8_t random_or = 0;
    for (size_t index = 0; index < 16; ++index)
        random_or |= random[index];
    if (!random_or)
        exit_now(104);

    uint64_t tls = (uint64_t)(uintptr_t)&tls_sentinel;
    __asm__ volatile("msr tpidr_el0, %0" : : "r"(tls) : "memory");
    uint64_t tid = syscall2(SYS_SET_TID_ADDRESS,
                            (uintptr_t)&clear_child_tid, 0);
    if (!tid || syscall2(SYS_GETTID, 0, 0) != tid)
        exit_now(105);
    syscall2(SYS_YIELD, 0, 0);
    uint64_t restored_tls;
    __asm__ volatile("mrs %0, tpidr_el0" : "=r"(restored_tls));
    if (restored_tls != tls || clear_child_tid != 0xffffffffu)
        exit_now(106);

    static const char marker[] =
        "MAKOS_AARCH64_SYSV_STARTUP_OK argc=3 argv=1 envp=1 auxv=pagesz,entry,uid,gid,random,execfn stack_align=16 registers=x0,x1,x2 tpidr=preempt-safe tid=real clear_child_tid=armed\n";
    syscall2(SYS_WRITE, (uintptr_t)marker, length(marker));
    exit_now(42);
}
