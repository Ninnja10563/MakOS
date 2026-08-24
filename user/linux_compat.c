#include <stdint.h>

enum {
    LINUX_SYS_WRITE = 1,
    LINUX_SYS_GETPID = 39,
    LINUX_SYS_EXIT = 60,
    LINUX_SYS_UNAME = 63,
    LINUX_SYS_CLOCK_GETTIME = 228,
    LINUX_CLOCK_MONOTONIC = 1,
};

struct linux_utsname {
    char sysname[65];
    char nodename[65];
    char release[65];
    char version[65];
    char machine[65];
    char domainname[65];
};

struct linux_timespec {
    int64_t seconds;
    int64_t nanoseconds;
};

static long linux_syscall3(uint64_t number, uint64_t first, uint64_t second,
                           uint64_t third) {
    __asm__ volatile("int $0x80"
                     : "+a"(number)
                     : "D"(first), "S"(second), "d"(third)
                     : "memory", "cc");
    return (long)number;
}

void _start(void) {
    static const char passed[] =
        "MAKOS_LINUX_APP_OK write=1 getpid=1 uname=1 clock_gettime=1 exit=1\n";
    struct linux_utsname name;
    struct linux_timespec time;
    long pid = linux_syscall3(LINUX_SYS_GETPID, 0, 0, 0);
    long uname_result =
        linux_syscall3(LINUX_SYS_UNAME, (uintptr_t)&name, 0, 0);
    long clock_result = linux_syscall3(LINUX_SYS_CLOCK_GETTIME,
                                       LINUX_CLOCK_MONOTONIC, (uintptr_t)&time, 0);
    if (pid != 3 || uname_result != 0 || name.sysname[0] != 'M' ||
        name.machine[0] != 'x' || clock_result != 0 || time.nanoseconds < 0 ||
        time.nanoseconds >= 1000000000)
        linux_syscall3(LINUX_SYS_EXIT, 91, 0, 0);
    if (linux_syscall3(LINUX_SYS_WRITE, 1, (uintptr_t)passed,
                       sizeof(passed) - 1) != (long)(sizeof(passed) - 1))
        linux_syscall3(LINUX_SYS_EXIT, 92, 0, 0);
    linux_syscall3(LINUX_SYS_EXIT, 42, 0, 0);
    __builtin_unreachable();
}
