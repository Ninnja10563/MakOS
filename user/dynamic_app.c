#include <stddef.h>
#include <stdint.h>

extern const char makos_shared_name[];
extern uint64_t makos_shared_add(uint64_t left, uint64_t right);

enum { SYS_WRITE = 0, SYS_EXIT = 5 };

static uint64_t syscall3(uint64_t number, uint64_t first, uint64_t second,
                         uint64_t third) {
    register uint64_t rax __asm__("rax") = number;
    register uint64_t rdi __asm__("rdi") = first;
    register uint64_t rsi __asm__("rsi") = second;
    register uint64_t rdx __asm__("rdx") = third;
    __asm__ volatile("int $0x80"
                     : "+a"(rax)
                     : "D"(rdi), "S"(rsi), "d"(rdx)
                     : "rcx", "r8", "r9", "r10", "r11", "memory");
    return rax;
}

static void exit_process(uint64_t code) {
    syscall3(SYS_EXIT, code, 0, 0);
    __builtin_trap();
}

void _start(void) {
    static const char expected[] = "libmakosdemo.so";
    static const char passed[] =
        "MAKOS_DYNAMIC_APP_OK shared=libmakosdemo.so symbol=makos_shared_add "
        "result=42 plt=1 got=1\n";
    uint64_t result = makos_shared_add(20, 22);
    for (size_t index = 0; index < sizeof(expected); ++index) {
        if (makos_shared_name[index] != expected[index])
            exit_process(91);
    }
    if (result != 42)
        exit_process(92);
    syscall3(SYS_WRITE, (uintptr_t)passed, sizeof(passed) - 1, 0);
    exit_process(result);
}
