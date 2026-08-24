#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include "py/compile.h"
#include "py/builtin.h"
#include "py/gc.h"
#include "py/mperrno.h"
#include "py/obj.h"
#include "py/runtime.h"
#include "shared/runtime/gchelper.h"

enum {
    SYS_WRITE = 0,
    SYS_EXIT = 5,
    SYS_OPEN = 11,
    SYS_READ = 12,
    SYS_CLOSE = 13,
    SCRIPT_BYTES = 2048,
    HEAP_BYTES = 256 * 1024,
};

static uint8_t heap[HEAP_BYTES] __attribute__((aligned(16)));
static char script[SCRIPT_BYTES + 1];

static uint64_t syscall4(uint64_t number, uint64_t first, uint64_t second,
                         uint64_t third, uint64_t fourth) {
    register uint64_t x0 __asm__("x0") = first;
    register uint64_t x1 __asm__("x1") = second;
    register uint64_t x2 __asm__("x2") = third;
    register uint64_t x3 __asm__("x3") = fourth;
    register uint64_t x8 __asm__("x8") = number;
    __asm__ volatile("svc #0" : "+r"(x0)
                     : "r"(x1), "r"(x2), "r"(x3), "r"(x8)
                     : "memory", "cc");
    return x0;
}

static void write_text(const char *text) {
    syscall4(SYS_WRITE, (uintptr_t)text, strlen(text), 0, 0);
}

static void terminate(uint64_t status) __attribute__((noreturn));
static void terminate(uint64_t status) {
    syscall4(SYS_EXIT, status, 0, 0, 0);
    for (;;) {
        __asm__ volatile("wfe");
    }
}

mp_uint_t mp_hal_stdout_tx_strn(const char *text, mp_uint_t length) {
    return syscall4(SYS_WRITE, (uintptr_t)text, length, 0, 0);
}

int mp_hal_stdin_rx_chr(void) {
    return -1;
}

void gc_collect(void) {
    gc_collect_start();
    gc_helper_collect_regs_and_stack();
    gc_collect_end();
}

mp_lexer_t *mp_lexer_new_from_file(qstr filename) {
    (void)filename;
    mp_raise_OSError(MP_ENOENT);
}

mp_import_stat_t mp_import_stat(const char *path) {
    (void)path;
    return MP_IMPORT_STAT_NO_EXIST;
}

void nlr_jump_fail(void *value) {
    (void)value;
    write_text("MicroPython fatal: uncaught NLR\n");
    terminate(70);
}

void __fatal_error(const char *message) {
    write_text("MicroPython fatal: ");
    write_text(message);
    write_text("\n");
    terminate(71);
}

void _start(const char *path) {
    if (path == NULL) {
        write_text("python: missing script path\n");
        terminate(2);
    }
    size_t path_length = strnlen(path, 128);
    if (path_length == 0 || path_length == 128) {
        write_text("python: invalid script path\n");
        terminate(2);
    }
    uint64_t fd = syscall4(SYS_OPEN, (uintptr_t)path, path_length, 0, 0);
    if (fd == UINT64_MAX) {
        write_text("python: cannot open script\n");
        terminate(2);
    }
    uint64_t got = syscall4(SYS_READ, fd, (uintptr_t)script, SCRIPT_BYTES, 0);
    uint64_t closed = syscall4(SYS_CLOSE, fd, 0, 0, 0);
    if (got == UINT64_MAX || got > SCRIPT_BYTES || closed == UINT64_MAX) {
        write_text("python: cannot read script\n");
        terminate(2);
    }
    script[got] = 0;

    mp_cstack_init_with_sp_here(48 * 1024);
    gc_init(heap, heap + sizeof(heap));
    mp_init();
    write_text("MakOS MicroPython 1.28.0\n");

    int status = 0;
    nlr_buf_t nlr;
    if (nlr_push(&nlr) == 0) {
        mp_lexer_t *lexer = mp_lexer_new_from_str_len(
            MP_QSTR__lt_stdin_gt_, script, (size_t)got, 0);
        qstr source_name = lexer->source_name;
        mp_parse_tree_t tree = mp_parse(lexer, MP_PARSE_FILE_INPUT);
        mp_obj_t module = mp_compile(&tree, source_name, true);
        mp_call_function_0(module);
        nlr_pop();
        write_text("MAKOS_PYTHON_OK implementation=micropython version=1.28.0 parser=1 compiler=bytecode vm=1 gc=tracing source=vfs\n");
    } else {
        mp_obj_print_exception(&mp_plat_print, MP_OBJ_FROM_PTR(nlr.ret_val));
        status = 1;
    }
    mp_deinit();
    terminate((uint64_t)status);
}
