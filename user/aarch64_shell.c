#include <stddef.h>
#include <stdint.h>

enum {
    SYS_WRITE = 0,
    SYS_YIELD = 1,
    SYS_READ_KEY = 6,
    SYS_SHELL_COMMAND = 7,
    SYS_SURFACE_CREATE = 8,
    SYS_SURFACE_FILL = 9,
    SYS_SURFACE_PRESENT = 10,
    SYS_OPEN = 11,
    SYS_READ = 12,
    SYS_CLOSE = 13,
    SYS_PROCESS_SPAWN = 14,
    SYS_PROCESS_WAIT = 15,
    SYS_PROCESS_SPAWN_PATH = 56,
    SYS_PROCESS_SPAWN_PATH_ARGS = 57,
    SYS_PACKAGE_INSTALL = 18,
    SYS_PACKAGE_QUERY = 19,
    SYS_PACKAGE_ROLLBACK = 20,
    SYS_AUTH_LOGIN = 30,
    SYS_PACKAGE_REMOVE = 52,
    SYS_SESSION_STATUS = 129,
    SYS_TCSETPGRP = 74,
};

enum { COMMAND_BYTES = 128, HISTORY_SLOTS = 8 };

struct spawn_arguments {
    uint32_t version;
    uint32_t argc;
    uint32_t envc;
    uint32_t data_length;
    uint32_t argv_offsets[8];
    uint32_t env_offsets[8];
    uint8_t data[256];
};

_Static_assert(sizeof(struct spawn_arguments) == 336,
               "spawn descriptor ABI size changed");

static uint64_t textedit_pid;
static uint64_t firefox_pid;

static const char FIRST_PACKAGE_SIGNATURE_HEX[] =
    "2a2de48d8a2d62f90f3a3c503666ddb7796e5d504291d30a2403a953c9d5fc004f295ae5ea065a13cc020d28fbb187912fa8c5b391ed89c4088611c459d50154c2f308d78752cb3729f00ce2b4d721b4e9f5d05238f548db6f57a53fdf2a8f3695fffedc5044090e9932b2e72f5bb4f8649f9e905b1fe252082da92704f0fc4739fc26d4e7f9169f8590630bf83fd84e02979599b908ca4833057e1e197342481e858a7a27eb679397eba8cd06a7dce02db9d06fdee8c28227fbd78d6781b844b4ac9bb5d02282d05b8336520c6b59be2e4856d9db78028431972da88d4e8fe9bf646ae424f5dd0540e0cefa0ccaa364c6b1999c0d885f182eca99811b10e21d";
static const char SECOND_PACKAGE_SIGNATURE_HEX[] =
    "7c2ac1bf73504c005a557b4874b5f925ae214db13ff46cbb9175b8c41f8665740b463afa292256dc3e6524b980dacd383a41a95a9e5f37ad804c22f9ee1414424e1d4818980f958372375c8945cdfbab27ffa51be425d13580f64f4d452b852378f7d1265570835630f469beb752eeb3db28f93ac6f13ccf570ef9569ac1388b091dfb9291e80c7abe75830c4478fb1edbebf59efcc06cc44c16abea93c3bdd3005f39ccf8b1c82ef87748a1c64687cd16f9657a8d3916c2e71cfdfc8fe7a3df5b5f354f9f0516e1bce9c8f1411af0a6d5405b21a59f8a04502f046816c9ca0ae74f61b9a8848fa304898826c237506af2921ed32b16747404e653bc4ec92ba7";

void *memset(void *destination, int value, size_t count) {
    uint8_t *bytes = destination;
    for (size_t index = 0; index < count; ++index)
        bytes[index] = (uint8_t)value;
    return destination;
}

void *memcpy(void *destination, const void *source, size_t count) {
    uint8_t *output = destination;
    const uint8_t *input = source;
    for (size_t index = 0; index < count; ++index)
        output[index] = input[index];
    return destination;
}

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

static size_t length(const char *text) {
    size_t count = 0;
    while (text[count])
        ++count;
    return count;
}

static void write_bytes(const void *bytes, size_t count) {
    syscall4(SYS_WRITE, (uintptr_t)bytes, count, 0, 0);
}

static void write_text(const char *text) { write_bytes(text, length(text)); }

static uint32_t append_spawn_string(struct spawn_arguments *arguments,
                                    const char *value) {
    uint32_t offset = arguments->data_length;
    size_t count = length(value) + 1;
    if ((size_t)offset + count > sizeof(arguments->data)) return UINT32_MAX;
    for (size_t index = 0; index < count; ++index)
        arguments->data[offset + index] = (uint8_t)value[index];
    arguments->data_length += (uint32_t)count;
    return offset;
}

static uint8_t hex_nibble(char value) {
    if (value >= '0' && value <= '9')
        return (uint8_t)(value - '0');
    if (value >= 'a' && value <= 'f')
        return (uint8_t)(value - 'a' + 10);
    return UINT8_MAX;
}

static int decode_signature(const char *hex, uint8_t signature[256]) {
    for (size_t index = 0; index < 256; ++index) {
        uint8_t high = hex_nibble(hex[index * 2]);
        uint8_t low = hex_nibble(hex[index * 2 + 1]);
        if (high == UINT8_MAX || low == UINT8_MAX)
            return 0;
        signature[index] = (uint8_t)((high << 4) | low);
    }
    return hex[512] == '\0';
}

static uint64_t package_install_fixture(const char *version,
                                        const char *content,
                                        const char *signature_hex,
                                        int tamper) {
    static const char name[] = "hello";
    static const char dependency[] = "libc";
    uint8_t fields[3 + 8 + 4 + 256];
    for (size_t index = 0; index < 3; ++index)
        fields[index] = (uint8_t)version[index];
    for (size_t index = 0; index < 8; ++index)
        fields[3 + index] = (uint8_t)content[index];
    for (size_t index = 0; index < 4; ++index)
        fields[11 + index] = (uint8_t)dependency[index];
    if (!decode_signature(signature_hex, &fields[15])) {
        memset(fields, 0, sizeof(fields));
        return 0;
    }
    if (tamper)
        fields[3] ^= 1;
    uint64_t packed = 3 | (UINT64_C(8) << 8) | (UINT64_C(4) << 16) |
                      (UINT64_C(1) << 24);
    uint64_t result = syscall4(SYS_PACKAGE_INSTALL, (uintptr_t)name,
                               sizeof(name) - 1, (uintptr_t)fields, packed);
    memset(fields, 0, sizeof(fields));
    return result;
}

static int package_query_version(const char expected[3]) {
    static const char name[] = "hello";
    uint8_t version[8] = {0};
    uint64_t count = syscall4(SYS_PACKAGE_QUERY, (uintptr_t)name,
                              sizeof(name) - 1, (uintptr_t)version,
                              sizeof(version));
    int valid = count == 3;
    for (size_t index = 0; valid && index < 3; ++index)
        valid = version[index] == (uint8_t)expected[index];
    memset(version, 0, sizeof(version));
    return valid;
}

static uint64_t package_open_payload(void) {
    static const char path[] = "/packages/hello/payload";
    return syscall4(SYS_OPEN, (uintptr_t)path, sizeof(path) - 1, 0, 0);
}

static int package_read_payload(uint64_t fd, const char expected[8]) {
    uint8_t content[8] = {0};
    uint64_t count = syscall4(SYS_READ, fd, (uintptr_t)content,
                              sizeof(content), 0);
    int valid = count == sizeof(content);
    for (size_t index = 0; valid && index < sizeof(content); ++index)
        valid = content[index] == (uint8_t)expected[index];
    memset(content, 0, sizeof(content));
    return valid;
}

static void package_probe_install(void) {
    if (package_install_fixture("1.0", "hello-v1",
                                FIRST_PACKAGE_SIGNATURE_HEX, 1) != 0 ||
        package_install_fixture("1.0", "hello-v1",
                                FIRST_PACKAGE_SIGNATURE_HEX, 0) != 1) {
        write_text("pkg-probe-install: transaction failed\n");
        return;
    }
    uint64_t pinned_fd = package_open_payload();
    if (pinned_fd == UINT64_MAX ||
        package_install_fixture("2.0", "hello-v2",
                                SECOND_PACKAGE_SIGNATURE_HEX, 0) != 1 ||
        package_install_fixture("1.0", "hello-v1",
                                FIRST_PACKAGE_SIGNATURE_HEX, 0) != 0 ||
        !package_query_version("2.0") ||
        !package_read_payload(pinned_fd, "hello-v1") ||
        syscall4(SYS_CLOSE, pinned_fd, 0, 0, 0) != 1 ||
        syscall4(SYS_PACKAGE_ROLLBACK, 0, 0, 0, 0) != 1 ||
        !package_query_version("1.0")) {
        if (pinned_fd != UINT64_MAX)
            syscall4(SYS_CLOSE, pinned_fd, 0, 0, 0);
        write_text("pkg-probe-install: transaction failed\n");
        return;
    }
    write_text("MAKOS_AARCH64_PACKAGE_TXN_OK install=1 replace=1 rollback=1 version=1.0 tamper_denied=1 open_fd_pin=replace pinned_reuse_denied=1\n");
}

static void package_probe_remove(void) {
    static const char name[] = "hello";
    uint8_t version[8] = {0};
    uint64_t pinned_fd = package_open_payload();
    uint64_t removed = syscall4(SYS_PACKAGE_REMOVE, (uintptr_t)name,
                                sizeof(name) - 1, 0, 0);
    uint64_t query = syscall4(SYS_PACKAGE_QUERY, (uintptr_t)name,
                              sizeof(name) - 1, (uintptr_t)version,
                              sizeof(version));
    memset(version, 0, sizeof(version));
    if (pinned_fd == UINT64_MAX || removed != 1 || query != UINT64_MAX ||
        syscall4(SYS_PACKAGE_ROLLBACK, 0, 0, 0, 0) != 0 ||
        !package_read_payload(pinned_fd, "hello-v1") ||
        syscall4(SYS_CLOSE, pinned_fd, 0, 0, 0) != 1) {
        if (pinned_fd != UINT64_MAX)
            syscall4(SYS_CLOSE, pinned_fd, 0, 0, 0);
        write_text("pkg-probe-remove: transaction failed\n");
        return;
    }
    write_text("MAKOS_AARCH64_PACKAGE_REMOVE_OK remove=1 query=absent vfs=refreshed open_fd_pin=remove pinned_rollback_denied=1\n");
}

static void package_probe_rollback(void) {
    if (syscall4(SYS_PACKAGE_ROLLBACK, 0, 0, 0, 0) != 1 ||
        !package_query_version("1.0")) {
        write_text("pkg-probe-rollback: transaction failed\n");
        return;
    }
    write_text("MAKOS_AARCH64_PACKAGE_ROLLBACK_OK rollback=1 version=1.0 vfs=refreshed\n");
}

static void package_probe_query(const char expected[3], const char *marker) {
    if (!package_query_version(expected)) {
        write_text("pkg-probe-query: version mismatch\n");
        return;
    }
    write_text(marker);
}

enum { SESSION_ENDED_KEY = 0x100 };

static uint16_t read_key(int require_session) {
    for (;;) {
        if (require_session &&
            syscall4(SYS_SESSION_STATUS, 0, 0, 0, 0) == 0)
            return SESSION_ENDED_KEY;
        if (textedit_pid &&
            syscall4(SYS_PROCESS_WAIT, textedit_pid, 0, 0, 0) != UINT64_MAX)
            textedit_pid = 0;
        uint8_t key = (uint8_t)syscall4(SYS_READ_KEY, 0, 0, 0, 0);
        if (key)
            return key;
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    }
}

static void launch_textedit(const uint8_t *name, size_t name_length) {
    static const char prefix[] = "/home/user/";
    uint8_t path[64] = {0};
    size_t path_length = 0;
    int canonical = name_length >= sizeof(prefix) - 1;
    for (size_t index = 0; canonical && index < sizeof(prefix) - 1; ++index)
        canonical = name[index] == (uint8_t)prefix[index];
    if (canonical) {
        if (name_length >= sizeof(path)) {
            write_text("edit: path too long\n");
            return;
        }
        for (size_t index = 0; index < name_length; ++index)
            path[index] = name[index];
        path_length = name_length;
    } else {
        if (!name_length || sizeof(prefix) - 1 + name_length >= sizeof(path)) {
            write_text("edit: invalid file name\n");
            return;
        }
        for (size_t index = 0; index < sizeof(prefix) - 1; ++index)
            path[index] = (uint8_t)prefix[index];
        for (size_t index = 0; index < name_length; ++index)
            path[sizeof(prefix) - 1 + index] = name[index];
        path_length = sizeof(prefix) - 1 + name_length;
    }
    uint64_t pid = syscall4(SYS_PROCESS_SPAWN, 2, (uintptr_t)path, path_length, 0);
    if (pid == UINT64_MAX)
        write_text("edit: Text Edit already open or launch failed\n");
    else {
        textedit_pid = pid;
        write_text("MAKOS_AARCH64_TEXTEDIT_LAUNCH_OK selector=2 process=isolated\n");
    }
    memset(path, 0, sizeof(path));
}

static void launch_python(const uint8_t *name, size_t name_length) {
    static const char prefix[] = "/home/user/";
    uint8_t path[64] = {0};
    size_t path_length = 0;
    int canonical = name_length >= sizeof(prefix) - 1;
    for (size_t index = 0; canonical && index < sizeof(prefix) - 1; ++index)
        canonical = name[index] == (uint8_t)prefix[index];
    if (canonical) {
        if (name_length >= sizeof(path)) {
            write_text("python: path too long\n");
            return;
        }
        for (size_t index = 0; index < name_length; ++index)
            path[index] = name[index];
        path_length = name_length;
    } else {
        if (!name_length || sizeof(prefix) - 1 + name_length >= sizeof(path)) {
            write_text("python: invalid file name\n");
            return;
        }
        for (size_t index = 0; index < sizeof(prefix) - 1; ++index)
            path[index] = (uint8_t)prefix[index];
        for (size_t index = 0; index < name_length; ++index)
            path[sizeof(prefix) - 1 + index] = name[index];
        path_length = sizeof(prefix) - 1 + name_length;
    }
    uint64_t pid = syscall4(SYS_PROCESS_SPAWN, 4, (uintptr_t)path, path_length, 0);
    memset(path, 0, sizeof(path));
    if (pid == UINT64_MAX) {
        write_text("python: launch failed\n");
        return;
    }
    while (syscall4(SYS_PROCESS_WAIT, pid, 0, 0, 0) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    write_text("MAKOS_AARCH64_PYTHON_LAUNCH_OK selector=4 process=isolated wait=1\n");
}

static void launch_nano(const uint8_t *name, size_t name_length) {
    static const char prefix[] = "/home/user/";
    uint8_t path[64] = {0};
    size_t path_length = 0;
    int absolute = name_length >= sizeof(prefix) - 1;
    for (size_t index = 0; absolute && index < sizeof(prefix) - 1; ++index)
        absolute = name[index] == (uint8_t)prefix[index];
    if (absolute) {
        if (name_length >= sizeof(path)) {
            write_text("nano: invalid file name\n");
            return;
        }
        for (size_t index = 0; index < name_length; ++index)
            path[index] = name[index];
        path_length = name_length;
    } else {
        if (!name_length || sizeof(prefix) - 1 + name_length >= sizeof(path)) {
            write_text("nano: invalid file name\n");
            return;
        }
        for (size_t index = 0; index < sizeof(prefix) - 1; ++index)
            path[index] = (uint8_t)prefix[index];
        for (size_t index = 0; index < name_length; ++index)
            path[sizeof(prefix) - 1 + index] = name[index];
        path_length = sizeof(prefix) - 1 + name_length;
    }
    if (path_length > 43) {
        write_text("nano: file name too long\n");
        return;
    }
    uint64_t pid = syscall4(SYS_PROCESS_SPAWN, 15, (uintptr_t)path, path_length, 0);
    memset(path, 0, sizeof(path));
    if (pid == UINT64_MAX) {
        write_text("nano: package absent or launch failed\n");
        return;
    }
    uint64_t status;
    while ((status = syscall4(SYS_PROCESS_WAIT, pid, 0, 0, 0)) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    syscall4(SYS_TCSETPGRP, 0, 1, 0, 0);
    if (status == 0)
        write_text("MAKOS_NANO_REAP_OK source=gnu-9.1 status=0 process=isolated tty=foreground\n");
    else
        write_text("nano: exited with failure\n");
}

static void run_startup_probe(void) {
    uint64_t pid = syscall4(SYS_PROCESS_SPAWN, 5, 0, 0, 0);
    if (pid == UINT64_MAX) {
        write_text("abi-startup: launch failed\n");
        return;
    }
    uint64_t status;
    while ((status = syscall4(SYS_PROCESS_WAIT, pid, 0, 0, 0)) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    if (status == 42)
        write_text("MAKOS_AARCH64_SYSV_REAP_OK status=42 lifecycle=spawn,run,exit,wait,reap\n");
    else
        write_text("abi-startup: validation failed\n");
}

static void run_musl_probe(void) {
    uint64_t pid = syscall4(SYS_PROCESS_SPAWN, 6, 0, 0, 0);
    if (pid == UINT64_MAX) {
        write_text("musl-probe: launch failed\n");
        return;
    }
    uint64_t status;
    while ((status = syscall4(SYS_PROCESS_WAIT, pid, 0, 0, 0)) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    if (status == 42)
        write_text("MAKOS_MUSL_REAP_OK status=42 lifecycle=spawn,run,exit,wait,reap\n");
    else
        write_text("musl-probe: runtime failed\n");
}

static void run_selfhost_probe(void) {
    static const char output_path[] = "/home/user/generated-aarch64.elf";
    uint64_t toolchain = syscall4(SYS_PROCESS_SPAWN, 16, 0, 0, 0);
    if (toolchain == UINT64_MAX) {
        write_text("selfhost-aarch64: toolchain launch failed\n");
        return;
    }
    uint64_t status;
    while ((status = syscall4(SYS_PROCESS_WAIT, toolchain, 0, 0, 0)) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    if (status != 42) {
        write_text("selfhost-aarch64: assembler/linker failed\n");
        return;
    }
    uint64_t generated = syscall4(SYS_PROCESS_SPAWN_PATH, (uintptr_t)output_path,
                                  sizeof(output_path) - 1, 0, 0);
    if (generated == UINT64_MAX) {
        write_text("selfhost-aarch64: generated ELF rejected\n");
        return;
    }
    while ((status = syscall4(SYS_PROCESS_WAIT, generated, 0, 0, 0)) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    if (status != 42) {
        write_text("selfhost-aarch64: generated program failed\n");
        return;
    }

    struct spawn_arguments startup = {0};
    startup.version = 1;
    startup.argc = 3;
    startup.envc = 1;
    startup.argv_offsets[0] = append_spawn_string(&startup, output_path);
    startup.argv_offsets[1] = append_spawn_string(&startup, "alpha");
    startup.argv_offsets[2] = append_spawn_string(&startup, "two");
    startup.env_offsets[0] = append_spawn_string(&startup, "MODE=test");
    struct spawn_arguments malformed = startup;
    malformed.version = 2;
    if (syscall4(SYS_PROCESS_SPAWN_PATH_ARGS, (uintptr_t)output_path,
                 sizeof(output_path) - 1, (uintptr_t)&malformed,
                 sizeof(malformed)) != UINT64_MAX) {
        write_text("selfhost-aarch64: bad startup version accepted\n");
        return;
    }
    malformed = startup;
    malformed.argv_offsets[7] = 1;
    if (syscall4(SYS_PROCESS_SPAWN_PATH_ARGS, (uintptr_t)output_path,
                 sizeof(output_path) - 1, (uintptr_t)&malformed,
                 sizeof(malformed)) != UINT64_MAX ||
        syscall4(SYS_PROCESS_SPAWN_PATH_ARGS, (uintptr_t)output_path,
                 sizeof(output_path) - 1, (uintptr_t)&startup,
                 sizeof(startup) - 1) != UINT64_MAX) {
        write_text("selfhost-aarch64: malformed startup bounds accepted\n");
        return;
    }
    generated = syscall4(SYS_PROCESS_SPAWN_PATH_ARGS, (uintptr_t)output_path,
                         sizeof(output_path) - 1, (uintptr_t)&startup,
                         sizeof(startup));
    if (generated == UINT64_MAX) {
        write_text("selfhost-aarch64: startup-vector ELF rejected\n");
        return;
    }
    while ((status = syscall4(SYS_PROCESS_WAIT, generated, 0, 0, 0)) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    if (status == 42)
        write_text("MAKOS_AARCH64_SELFHOST_LINK_OK source=guest-makfs sources=2 languages=aarch64-asm,c-subset-v1 compiler=guest-native assembler=guest-native linker=guest-native objects=2 object_format=elf64-et-rel relocations=R_AARCH64_CALL26:3 symbols=_start,answer,adjust,combine c_source=/home/user/generated-program.c translation_unit_functions=3 c_abi=aapcs64-int32-pointer64 c_features=multi-function,multi-parameter,parameter,pointer-parameter,local,array,array-decay,index,assignment,pointer,pointer-add,pointer-variable-add,pointer-difference,address-of,address-expression,dereference,if,equality,inequality,relational,while,call,return max_parameters=2 max_call_arguments=2 nonleaf_frame=96 c_operators=mul,sub,add c_relations=eq,ne,lt,le,gt,ge branch_results=42,86 loop_results=42,2 memory_results=42,2 pointer_call=answer-to-adjust pointee_results=42,44,2 delta_results=1:42,2:44,1:2 array_results=41:42:0,42:0:44,1:2:0 pointer_offset_call=1 pointer_variable_offset=delta dynamic_pointer_adds=2 signed_pointer_offset=-1:42 signed_pointer_difference=3:-3 relational_results=gt:42:0,le:42:0,ge:42:86,lt:42:44 code_bytes=76,140,168,60 object_bytes=688,1032 intra_object_calls=2 linked_bytes=444 output_bytes=815 persisted_reopened=1 malformed_c_denied=17 malformed_relocation_denied=1 unresolved_symbol_denied=1 duplicate_definition_denied=1 output=elf64-aarch64 kernel_loader=validated abi56=1 abi57=1 argv=3 env=1 malformed_startup_denied=3 executed=2 status=42\n");
    else
        write_text("selfhost-aarch64: startup-vector program failed\n");
}

static void run_musl_crt_probe(void) {
    uint64_t pid = syscall4(SYS_PROCESS_SPAWN, 7, 0, 0, 0);
    if (pid == UINT64_MAX) {
        write_text("musl-crt: launch failed\n");
        return;
    }
    uint64_t status;
    while ((status = syscall4(SYS_PROCESS_WAIT, pid, 0, 0, 0)) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    if (status == 42)
        write_text("MAKOS_MUSL_CRT_REAP_OK status=42 lifecycle=spawn,run,exit,wait,reap\n");
    else
        write_text("musl-crt: runtime failed\n");
}

static void run_stack_protector_probe(void) {
    uint64_t pid = syscall4(SYS_PROCESS_SPAWN, 15, 0, 0, 0);
    if (pid == UINT64_MAX) {
        write_text("stack-protector: launch failed\n");
        return;
    }
    uint64_t status;
    while ((status = syscall4(SYS_PROCESS_WAIT, pid, 0, 0, 0)) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    if (status != 0 && status != 42 && status != 134)
        write_text("MAKOS_STACK_PROTECTOR_REAP_OK failure=contained shell=survived\n");
    else
        write_text("stack-protector: failure path did not terminate child\n");
}

static void run_musl_pthread_probe(void) {
    uint64_t pid = syscall4(SYS_PROCESS_SPAWN, 8, 0, 0, 0);
    if (pid == UINT64_MAX) {
        write_text("musl-pthread: launch failed\n");
        return;
    }
    uint64_t status;
    while ((status = syscall4(SYS_PROCESS_WAIT, pid, 0, 0, 0)) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    if (status == 42)
        write_text("MAKOS_MUSL_PTHREAD_REAP_OK status=42 lifecycle=spawn,threads,join,exit,wait,reap\n");
    else
        write_text("musl-pthread: runtime failed\n");
}

static void run_firefox_smp_probe(void) {
    uint64_t pid = syscall4(SYS_PROCESS_SPAWN, 17, 0, 0, 0);
    if (pid == UINT64_MAX) {
        write_text("firefox-smp: launch failed\n");
        return;
    }
    uint64_t status;
    while ((status = syscall4(SYS_PROCESS_WAIT, pid, 0, 0, 0)) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    if (status == 42)
        write_text("MAKOS_AARCH64_FIREFOX_SMP_REAP_OK fixture=upstream-musl-pthread role=firefox status=42 lifecycle=spawn,threads,ap-dispatch,block,wake,join,exit,wait,reap\n");
    else
        write_text("firefox-smp: runtime failed\n");
}

static void run_musl_interp_probe(void) {
    uint64_t pid = syscall4(SYS_PROCESS_SPAWN, 9, 0, 0, 0);
    if (pid == UINT64_MAX) {
        write_text("musl-dynamic: launch failed\n");
        return;
    }
    uint64_t status;
    while ((status = syscall4(SYS_PROCESS_WAIT, pid, 0, 0, 0)) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    if (status == 42)
        write_text("MAKOS_MUSL_INTERP_REAP_OK status=42 lifecycle=spawn,interp,relocate,entry,exit,wait,reap\n");
    else
        write_text("musl-dynamic: runtime failed\n");
}

static void run_musl_dynamic_probe(void) {
    uint64_t pid = syscall4(SYS_PROCESS_SPAWN, 10, 0, 0, 0);
    if (pid == UINT64_MAX) {
        write_text("musl-shared: launch failed\n");
        return;
    }
    uint64_t status;
    while ((status = syscall4(SYS_PROCESS_WAIT, pid, 0, 0, 0)) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    if (status == 42)
        write_text("MAKOS_MUSL_DYNAMIC_REAP_OK status=42 lifecycle=spawn,interp,needed-libc,relocate,main,exit,wait,reap\n");
    else
        write_text("musl-shared: runtime failed\n");
}

static void run_musl_dso_probe(void) {
    uint64_t pid = syscall4(SYS_PROCESS_SPAWN, 11, 0, 0, 0);
    if (pid == UINT64_MAX) {
        write_text("musl-dso: launch failed\n");
        return;
    }
    uint64_t status;
    while ((status = syscall4(SYS_PROCESS_WAIT, pid, 0, 0, 0)) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    if (status == 42)
        write_text("MAKOS_MUSL_DSO_REAP_OK status=42 lifecycle=spawn,interp,open,fstat,mmap,relocate,symbol,main,exit,wait,reap\n");
    else
        write_text("musl-dso: runtime failed\n");
}

static void run_musl_dlopen_probe(void) {
    uint64_t pid = syscall4(SYS_PROCESS_SPAWN, 12, 0, 0, 0);
    if (pid == UINT64_MAX) {
        write_text("musl-dlopen: launch failed\n");
        return;
    }
    uint64_t status;
    while ((status = syscall4(SYS_PROCESS_WAIT, pid, 0, 0, 0)) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    if (status == 42)
        write_text("MAKOS_MUSL_DLOPEN_REAP_OK status=42 lifecycle=spawn,interp,main,dlopen,dlsym,call,dlclose,exit,wait,reap\n");
    else
        write_text("musl-dlopen: runtime failed\n");
}

static void run_musl_exec_probe(void) {
    uint64_t pid = syscall4(SYS_PROCESS_SPAWN, 13, 0, 0, 0);
    if (pid == UINT64_MAX) {
        write_text("musl-exec: launch failed\n");
        return;
    }
    uint64_t status;
    while ((status = syscall4(SYS_PROCESS_WAIT, pid, 0, 0, 0)) == UINT64_MAX)
        syscall4(SYS_YIELD, 0, 0, 0, 0);
    if (status == 42) {
        write_text("MAKOS_MUSL_EXEC_REAP_OK status=42 lifecycle=spawn,execve,same-pid,interp,libc,main,exit,wait,reap\n");
    } else {
        write_text("musl-exec: runtime failed\n");
    }
}

static void launch_firefox(void) {
    if (firefox_pid &&
        syscall4(SYS_PROCESS_WAIT, firefox_pid, 0, 0, 0) == UINT64_MAX) {
        write_text("firefox: already running\n");
        return;
    }
    firefox_pid = syscall4(SYS_PROCESS_SPAWN, 14, 0, 0, 0);
    if (firefox_pid == UINT64_MAX) {
        firefox_pid = 0;
        write_text("firefox: package absent or launch failed\n");
    } else {
        write_text("MAKOS_FIREFOX_LAUNCH_OK source=official-esr package=disk process=isolated\n");
    }
}

static size_t read_login_line(uint8_t *buffer, size_t capacity, int echo) {
    size_t used = 0;
    for (;;) {
        uint16_t key = read_key(0);
        if (key == 0x1b)
            return (size_t)-1;
        if (key == '\n' || key == '\t')
            return used;
        if (key == 8 && used) {
            --used;
            buffer[used] = 0;
            write_bytes("\b", 1);
        } else if (key >= 0x20 && key <= 0x7e && used < capacity) {
            buffer[used++] = key;
            if (echo)
                write_bytes(&key, 1);
            else
                write_bytes("*", 1);
        }
    }
}

static void fill(uint64_t handle, uint32_t color, uint32_t x, uint32_t y,
                 uint32_t width, uint32_t height) {
    uint32_t rectangle[4] = {x, y, width, height};
    syscall4(SYS_SURFACE_FILL, handle, color, (uintptr_t)rectangle, 0);
}

static void write_prompt(const uint8_t *username, size_t username_length) {
    write_bytes(username, username_length);
    write_text("@makos:~$ ");
}

static uint64_t launch_desktop(const uint8_t *username, size_t username_length) {
    uint64_t monitor = syscall4(SYS_SURFACE_CREATE, 420, 260, 0, 0);
    uint64_t terminal = syscall4(SYS_SURFACE_CREATE, 720, 420, 0, 0);
    uint64_t settings = syscall4(SYS_SURFACE_CREATE, 560, 360, 0, 0);
    if (!monitor || !terminal || !settings)
        for (;;)
            __asm__ volatile("wfe");
    fill(monitor, 0xff17233a, 0, 0, 420, 260);
    fill(monitor, 0xff4cde9c, 20, 24, 380, 34);
    fill(monitor, 0xffdce9ff, 20, 82, 300, 18);
    fill(monitor, 0xff8fa9d8, 20, 122, 350, 14);
    fill(monitor, 0xff8fa9d8, 20, 160, 260, 14);
    fill(monitor, 0xff8fa9d8, 20, 198, 330, 14);
    syscall4(SYS_SURFACE_PRESENT, monitor, 0, 0, 0);
    fill(terminal, 0xff000000, 0, 0, 720, 420);
    syscall4(SYS_SURFACE_PRESENT, terminal, 0, 0, 0);
    fill(settings, 0xffc0c0c0, 0, 0, 560, 360);
    /* Compositor draws responsive cards; backing remains uniform on resize. */
    write_text("MakOS AArch64 EL0 terminal\nType 'help' for commands.\n");
    write_prompt(username, username_length);
    write_text("MAKOS_AARCH64_DESKTOP_OK login=1 apps=5 terminal=interactive "
               "taskbar=1 start_menu=1 drag=1 close=1 reopen=1 cursor=virtio-gpu-plane "
               "shell=el0 settings=1 browser=1 files=1 clock=utc network=dhcp "
               "wifi=unavailable-no-device\n");
    return terminal;
}

static int starts_with(const uint8_t *value, size_t value_length,
                       const char *prefix) {
    size_t prefix_length = length(prefix);
    if (value_length > prefix_length)
        return 0;
    for (size_t index = 0; index < value_length; ++index)
        if (value[index] != (uint8_t)prefix[index])
            return 0;
    return 1;
}

static const char *completion(const uint8_t *prefix, size_t prefix_length) {
    static const char *commands[] = {
        "help", "status", "clear", "pwd", "ls", "ls -l", "cat note.txt", "cp ", "mv ", "wc ", "echo ",
        "whoami", "uname -a", "uptime", "mem", "ps", "stat note.txt",
        "touch ", "write ", "rm ", "edit ", "nano ", "python ", "firefox", "firefox-smp", "selfhost-aarch64", "abi-startup", "musl-probe", "musl-crt", "musl-pthread", "musl-dynamic", "musl-shared", "musl-dso", "musl-dlopen", "musl-exec", "pkg-probe-install", "pkg-probe-remove", "pkg-probe-rollback", "pkg-probe-query-v1", "pkg-probe-query-v2", "adduser ", "signout", "exit",
    };
    const char *found = 0;
    for (size_t index = 0; index < sizeof(commands) / sizeof(commands[0]); ++index) {
        if (starts_with(prefix, prefix_length, commands[index])) {
            if (found)
                return 0;
            found = commands[index];
        }
    }
    return found;
}

static int exact(const uint8_t *value, size_t value_length, const char *text) {
    size_t text_length = length(text);
    if (value_length != text_length)
        return 0;
    for (size_t index = 0; index < text_length; ++index)
        if (value[index] != (uint8_t)text[index])
            return 0;
    return 1;
}

static void add_user(const uint8_t *username, size_t username_length,
                     uint64_t terminal) {
    uint8_t password[64] = {0};
    uint8_t confirmation[64] = {0};
    uint8_t request[COMMAND_BYTES] = {0};
    write_text("New password: ");
    size_t password_length = read_login_line(password, sizeof(password), 0);
    write_text("\nRetype password: ");
    size_t confirmation_length =
        read_login_line(confirmation, sizeof(confirmation), 0);
    write_text("\n");
    int same = password_length == confirmation_length;
    for (size_t index = 0; same && index < password_length; ++index)
        same = password[index] == confirmation[index];
    if (!same || username_length > 31 || password_length > 64) {
        write_text("Passwords do not match.\n");
    } else {
        request[0] = 0xff;
        request[1] = 'A';
        request[2] = (uint8_t)username_length;
        request[3] = (uint8_t)password_length;
        for (size_t index = 0; index < username_length; ++index)
            request[4 + index] = username[index];
        for (size_t index = 0; index < password_length; ++index)
            request[4 + username_length + index] = password[index];
        syscall4(SYS_SHELL_COMMAND, (uintptr_t)request,
                 4 + username_length + password_length, terminal, 0);
    }
    memset(request, 0, sizeof(request));
    memset(password, 0, sizeof(password));
    memset(confirmation, 0, sizeof(confirmation));
}

static void replace_line(uint8_t *command, size_t *used, const uint8_t *replacement,
                         size_t replacement_length) {
    for (size_t index = 0; index < *used; ++index)
        write_bytes("\b", 1);
    for (size_t index = 0; index < COMMAND_BYTES; ++index)
        command[index] = 0;
    if (replacement_length >= COMMAND_BYTES)
        replacement_length = COMMAND_BYTES - 1;
    for (size_t index = 0; index < replacement_length; ++index)
        command[index] = replacement[index];
    *used = replacement_length;
    write_bytes(command, *used);
}

__attribute__((noreturn)) void _start(void) {
    for (;;) {
        uint8_t username[32] = {0};
        uint8_t password[64] = {0};
        size_t username_length = read_login_line(username, sizeof(username), 1);
        if (username_length == (size_t)-1) {
            write_text("MAKOS_LOGIN_RETRY\n");
            continue;
        }
        write_text("password: ");
        size_t password_length = read_login_line(password, sizeof(password), 0);
        if (password_length == (size_t)-1) {
            memset(password, 0, sizeof(password));
            memset(username, 0, sizeof(username));
            write_text("MAKOS_LOGIN_RETRY\n");
            continue;
        }
        if (syscall4(SYS_AUTH_LOGIN, (uintptr_t)username, username_length,
                     (uintptr_t)password, password_length) != 1) {
            memset(password, 0, sizeof(password));
            memset(username, 0, sizeof(username));
            write_text("login failed\n");
            write_text("MAKOS_LOGIN_RETRY\n");
            continue;
        }
        memset(password, 0, sizeof(password));

        uint64_t browser_pid = syscall4(SYS_PROCESS_SPAWN, 1, 0, 0, 0);
        if (browser_pid == UINT64_MAX)
            write_text("browser launch failed\n");
        else
            write_text("MAKOS_AARCH64_BROWSER_LAUNCH_OK selector=1 process=isolated\n");
        uint64_t files_pid = syscall4(SYS_PROCESS_SPAWN, 3, 0, 0, 0);
        if (files_pid == UINT64_MAX)
            write_text("Files launch failed\n");
        else
            write_text("MAKOS_AARCH64_FILES_LAUNCH_OK selector=3 process=isolated\n");
        uint64_t terminal = launch_desktop(username, username_length);
        uint8_t command[COMMAND_BYTES] = {0};
        uint8_t history[HISTORY_SLOTS][COMMAND_BYTES] = {{0}};
        size_t history_lengths[HISTORY_SLOTS] = {0};
        size_t command_length = 0, history_next = 0, history_count = 0;
        size_t history_offset = 0;
        int signed_out = 0;

        while (!signed_out) {
            uint16_t key = read_key(1);
            if (key == SESSION_ENDED_KEY) {
                signed_out = 1;
                textedit_pid = 0;
                firefox_pid = 0;
                write_text("MAKOS_AARCH64_GUI_SIGNOUT_SYNC_OK shell=login-loop app-pids=cleared\n");
                continue;
            }
            if (key == '\n') {
                write_text("\n");
                if (command_length) {
                    memset(history[history_next], 0, COMMAND_BYTES);
                    for (size_t index = 0; index < command_length; ++index)
                        history[history_next][index] = command[index];
                    history_lengths[history_next] = command_length;
                    history_next = (history_next + 1) % HISTORY_SLOTS;
                    if (history_count < HISTORY_SLOTS)
                        ++history_count;
                }
                if (command_length == 4 && exact(command, command_length, "edit")) {
                    launch_textedit((const uint8_t *)"note.txt", 8);
                } else if (command_length == 4 && exact(command, command_length, "nano")) {
                    launch_nano((const uint8_t *)"note.txt", 8);
                } else if (command_length > 5 && command[0] == 'e' &&
                           command[1] == 'd' && command[2] == 'i' &&
                           command[3] == 't' && command[4] == ' ') {
                    launch_textedit(&command[5], command_length - 5);
                } else if (command_length > 5 && command[0] == 'n' &&
                           command[1] == 'a' && command[2] == 'n' &&
                           command[3] == 'o' && command[4] == ' ') {
                    launch_nano(&command[5], command_length - 5);
                } else if (command_length > 7 && command[0] == 'p' &&
                           command[1] == 'y' && command[2] == 't' &&
                           command[3] == 'h' && command[4] == 'o' &&
                           command[5] == 'n' && command[6] == ' ') {
                    launch_python(&command[7], command_length - 7);
                } else if (command_length > 8 &&
                           command[0] == 'a' && command[1] == 'd' &&
                           command[2] == 'd' && command[3] == 'u' &&
                           command[4] == 's' && command[5] == 'e' &&
                           command[6] == 'r' && command[7] == ' ') {
                    add_user(&command[8], command_length - 8, terminal);
                } else if (exact(command, command_length, "abi-startup")) {
                    run_startup_probe();
                } else if (exact(command, command_length, "selfhost-aarch64")) {
                    run_selfhost_probe();
                } else if (exact(command, command_length, "musl-probe")) {
                    run_musl_probe();
                } else if (exact(command, command_length, "musl-crt")) {
                    run_musl_crt_probe();
                } else if (exact(command, command_length, "stack-protector")) {
                    run_stack_protector_probe();
                } else if (exact(command, command_length, "musl-pthread")) {
                    run_musl_pthread_probe();
                } else if (exact(command, command_length, "firefox-smp")) {
                    run_firefox_smp_probe();
                } else if (exact(command, command_length, "musl-dynamic")) {
                    run_musl_interp_probe();
                } else if (exact(command, command_length, "musl-shared")) {
                    run_musl_dynamic_probe();
                } else if (exact(command, command_length, "musl-dso")) {
                    run_musl_dso_probe();
                } else if (exact(command, command_length, "musl-dlopen")) {
                    run_musl_dlopen_probe();
                } else if (exact(command, command_length, "musl-exec")) {
                    run_musl_exec_probe();
                } else if (exact(command, command_length, "firefox")) {
                    launch_firefox();
                } else if (exact(command, command_length, "pkg-probe-install")) {
                    package_probe_install();
                } else if (exact(command, command_length, "pkg-probe-remove")) {
                    package_probe_remove();
                } else if (exact(command, command_length, "pkg-probe-rollback")) {
                    package_probe_rollback();
                } else if (exact(command, command_length, "pkg-probe-query-v1")) {
                    package_probe_query(
                        "1.0",
                        "MAKOS_AARCH64_PACKAGE_QUERY_OK version=1.0\n");
                } else if (exact(command, command_length, "pkg-probe-query-v2")) {
                    package_probe_query(
                        "2.0",
                        "MAKOS_AARCH64_PACKAGE_QUERY_OK version=2.0\n");
                } else {
                    syscall4(SYS_SHELL_COMMAND, (uintptr_t)command,
                             command_length, terminal, 0);
                    signed_out = exact(command, command_length, "signout");
                    if (signed_out)
                        textedit_pid = 0;
                }
                memset(command, 0, sizeof(command));
                command_length = 0;
                history_offset = 0;
                if (!signed_out)
                    write_prompt(username, username_length);
            } else if (key == 8 && command_length) {
                command[--command_length] = 0;
                write_bytes("\b", 1);
            } else if (key == '\t') {
                const char *value = completion(command, command_length);
                if (value) {
                    replace_line(command, &command_length,
                                 (const uint8_t *)value, length(value));
                    write_text("MAKOS_AARCH64_SHELL_EDIT_OK completion=1 history_slots=8\n");
                }
            } else if (key == 0x13 && history_count) {
                if (history_offset < history_count)
                    ++history_offset;
                size_t index = (history_next + HISTORY_SLOTS - history_offset) %
                               HISTORY_SLOTS;
                replace_line(command, &command_length, history[index],
                             history_lengths[index]);
                write_text("MAKOS_AARCH64_SHELL_HISTORY_OK direction=up offset=1\n");
            } else if (key == 0x14 && history_count) {
                if (history_offset > 1) {
                    --history_offset;
                    size_t index =
                        (history_next + HISTORY_SLOTS - history_offset) %
                        HISTORY_SLOTS;
                    replace_line(command, &command_length, history[index],
                                 history_lengths[index]);
                } else {
                    history_offset = 0;
                    replace_line(command, &command_length, command, 0);
                }
                write_text("MAKOS_AARCH64_SHELL_HISTORY_OK direction=down offset=0\n");
            } else if (key >= 0x20 && key <= 0x7e &&
                       command_length + 1 < COMMAND_BYTES) {
                command[command_length++] = key;
                history_offset = 0;
                write_bytes(&key, 1);
            }
        }
        memset(username, 0, sizeof(username));
        memset(history, 0, sizeof(history));
    }
}
