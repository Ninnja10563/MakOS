#define TEXTEDIT_EMBEDDED
#define TEXTEDIT_EVENT_INPUT
#include "aarch64_textedit.c"

__attribute__((noreturn)) void _start(const char *path) {
    textedit_run(path);
    syscall4(5, 0, 0, 0, 0);
    for (;;)
        __asm__ volatile("wfe");
}
