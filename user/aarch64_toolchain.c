#include <stddef.h>
#include <stdint.h>

enum {
    SYS_WRITE = 0,
    SYS_EXIT = 5,
    SYS_OPEN = 11,
    SYS_READ = 12,
    SYS_CLOSE = 13,
    SYS_FILE_WRITE = 17,
    SYS_VM_MAP = 21,
    SYS_CREATE = 43,
    SYS_UNLINK = 44,
    SYS_VM_PROTECT = 45,
    PROT_READ = 1,
    PROT_WRITE = 2,
    PROT_EXEC = 4,
    ELF_HEADER_SIZE = 64,
    PROGRAM_HEADER_SIZE = 56,
    CODE_OFFSET = 256,
    DATA_OFFSET = 384,
    IMAGE_CAPACITY = 640,
};

static const uint64_t USER_BASE = UINT64_C(0x10000000);

void *memset(void *destination, int value, size_t count) {
    uint8_t *bytes = destination;
    for (size_t index = 0; index < count; ++index) bytes[index] = (uint8_t)value;
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
    while (text[count]) ++count;
    return count;
}

static void put16(volatile uint8_t *output, size_t offset, uint16_t value) {
    output[offset] = (uint8_t)value;
    output[offset + 1] = (uint8_t)(value >> 8);
}

static void put32(volatile uint8_t *output, size_t offset, uint32_t value) {
    for (size_t index = 0; index < 4; ++index)
        output[offset + index] = (uint8_t)(value >> (index * 8));
}

static void put64(volatile uint8_t *output, size_t offset, uint64_t value) {
    for (size_t index = 0; index < 8; ++index)
        output[offset + index] = (uint8_t)(value >> (index * 8));
}

static int consume(const char *source, size_t source_length, size_t *cursor,
                   const char *token) {
    size_t token_length = length(token);
    if (*cursor + token_length > source_length) return 0;
    for (size_t index = 0; index < token_length; ++index)
        if (source[*cursor + index] != token[index]) return 0;
    *cursor += token_length;
    return 1;
}

static int decimal(const char *source, size_t source_length, size_t *cursor,
                   uint32_t *value) {
    uint32_t result = 0;
    size_t digits = 0;
    while (*cursor < source_length && source[*cursor] >= '0' &&
           source[*cursor] <= '9') {
        result = result * 10 + (uint32_t)(source[*cursor] - '0');
        ++*cursor;
        ++digits;
        if (result > 65535) return 0;
    }
    *value = result;
    return digits != 0;
}

/* Bounded but genuine two-pass-free assembler for a documented A64 subset. */
static size_t assemble(const char *source, size_t source_length,
                       volatile uint8_t *code, size_t capacity) {
    size_t cursor = 0, output = 0;
    while (cursor < source_length) {
        uint32_t instruction;
        if (consume(source, source_length, &cursor, "mov x")) {
            uint32_t reg, immediate;
            if (!decimal(source, source_length, &cursor, &reg) || reg > 30 ||
                !consume(source, source_length, &cursor, ", #") ||
                !decimal(source, source_length, &cursor, &immediate))
                return 0;
            instruction = UINT32_C(0xd2800000) | (immediate << 5) | reg;
        } else if (consume(source, source_length, &cursor, "svc #0")) {
            instruction = UINT32_C(0xd4000001);
        } else if (consume(source, source_length, &cursor, "ret")) {
            instruction = UINT32_C(0xd65f03c0);
        } else {
            return 0;
        }
        if (cursor >= source_length || source[cursor++] != '\n' ||
            output + 4 > capacity)
            return 0;
        put32(code, output, instruction);
        output += 4;
    }
    return output;
}

static size_t emit_elf(volatile uint8_t image[IMAGE_CAPACITY],
                       const uint8_t *code, size_t code_length) {
    static const char provenance[] =
        "MakOS guest-native AArch64 assembler output\n";
    for (size_t index = 0; index < IMAGE_CAPACITY; ++index) image[index] = 0;
    image[0] = 0x7f; image[1] = 'E'; image[2] = 'L'; image[3] = 'F';
    image[4] = 2; image[5] = 1; image[6] = 1;
    put16(image, 16, 2);             /* ET_EXEC */
    put16(image, 18, 183);           /* EM_AARCH64 */
    put32(image, 20, 1);
    put64(image, 24, USER_BASE + CODE_OFFSET);
    put64(image, 32, ELF_HEADER_SIZE);
    put16(image, 52, ELF_HEADER_SIZE);
    put16(image, 54, PROGRAM_HEADER_SIZE);
    put16(image, 56, 2);

    size_t first = ELF_HEADER_SIZE;
    put32(image, first, 1);          /* PT_LOAD */
    put32(image, first + 4, 5);      /* PF_R | PF_X */
    put64(image, first + 8, 0);
    put64(image, first + 16, USER_BASE);
    put64(image, first + 24, USER_BASE);
    put64(image, first + 32, CODE_OFFSET + code_length);
    put64(image, first + 40, CODE_OFFSET + code_length);
    put64(image, first + 48, 1);

    size_t second = first + PROGRAM_HEADER_SIZE;
    put32(image, second, 1);
    put32(image, second + 4, 4);     /* PF_R, deliberately NX */
    put64(image, second + 8, DATA_OFFSET);
    put64(image, second + 16, USER_BASE + 0x1000);
    put64(image, second + 24, USER_BASE + 0x1000);
    put64(image, second + 32, sizeof(provenance) - 1);
    put64(image, second + 40, sizeof(provenance) - 1);
    put64(image, second + 48, 1);
    for (size_t index = 0; index < code_length; ++index)
        image[CODE_OFFSET + index] = code[index];
    for (size_t index = 0; index < sizeof(provenance) - 1; ++index)
        image[DATA_OFFSET + index] = (uint8_t)provenance[index];
    return DATA_OFFSET + sizeof(provenance) - 1;
}

static void fail(uint64_t status) {
    syscall4(SYS_EXIT, status, 0, 0, 0);
    for (;;) __asm__ volatile("wfe");
}

__attribute__((section(".text._start"), noreturn)) void _start(void) {
    static const char source_path[] = "/home/user/generated.s";
    static const char output_path[] = "/home/user/generated-aarch64.elf";
    static const char source[] =
        "mov x0, #42\n"
        "mov x8, #5\n"
        "svc #0\n"
        "ret\n";
    static const char jit_source[] = "mov x0, #42\nret\n";
    static const char marker[] =
        "MAKOS_AARCH64_ASSEMBLER_OK source=/home/user/generated.s "
        "output=/home/user/generated-aarch64.elf grammar=movz,svc,ret "
        "format=elf64 machine=aarch64 segments=2 code_rx=1 data_nx=1 "
        "persisted=1 wx_denied=1 jit_result=42\n";

    (void)syscall4(SYS_UNLINK, (uintptr_t)source_path,
                   sizeof(source_path) - 1, 0, 0);
    (void)syscall4(SYS_CREATE, (uintptr_t)source_path,
                   sizeof(source_path) - 1, 0, 0);
    uint64_t fd = syscall4(SYS_OPEN, (uintptr_t)source_path,
                           sizeof(source_path) - 1, 1, 0);
    if (fd == UINT64_MAX ||
        syscall4(SYS_FILE_WRITE, fd, (uintptr_t)source, sizeof(source) - 1, 0) !=
            sizeof(source) - 1 ||
        syscall4(SYS_CLOSE, fd, 0, 0, 0) != 1)
        fail(80);

    char input[128] = {0};
    fd = syscall4(SYS_OPEN, (uintptr_t)source_path,
                  sizeof(source_path) - 1, 0, 0);
    uint64_t source_length = syscall4(SYS_READ, fd, (uintptr_t)input,
                                      sizeof(input), 0);
    if (fd == UINT64_MAX || source_length != sizeof(source) - 1 ||
        syscall4(SYS_CLOSE, fd, 0, 0, 0) != 1)
        fail(81);

    uint8_t code[64] = {0};
    size_t code_length = assemble(input, (size_t)source_length, code, sizeof(code));
    if (code_length != 16) fail(82);

    uint8_t *jit = (uint8_t *)(uintptr_t)syscall4(SYS_VM_MAP, 0, 0, 0, 0);
    if ((uintptr_t)jit == UINT64_MAX) fail(83);
    size_t jit_length = assemble(jit_source, sizeof(jit_source) - 1, jit, 64);
    if (jit_length != 8 ||
        syscall4(SYS_VM_PROTECT, (uintptr_t)jit,
                 PROT_READ | PROT_WRITE | PROT_EXEC, 0, 0) != 0 ||
        syscall4(SYS_VM_PROTECT, (uintptr_t)jit, PROT_READ | PROT_EXEC, 0, 0) != 1)
        fail(83);
    uint64_t jit_result = ((uint64_t (*)(void))(uintptr_t)jit)();
    if (jit_result != 42) fail(84);

    volatile uint8_t image[IMAGE_CAPACITY];
    size_t image_length = emit_elf(image, code, code_length);
    (void)syscall4(SYS_UNLINK, (uintptr_t)output_path,
                   sizeof(output_path) - 1, 0, 0);
    (void)syscall4(SYS_CREATE, (uintptr_t)output_path,
                   sizeof(output_path) - 1, 0, 0);
    fd = syscall4(SYS_OPEN, (uintptr_t)output_path,
                  sizeof(output_path) - 1, 1, 0);
    if (fd == UINT64_MAX ||
        syscall4(SYS_FILE_WRITE, fd, (uintptr_t)image, image_length, 0) != image_length ||
        syscall4(SYS_CLOSE, fd, 0, 0, 0) != 1)
        fail(85);
    syscall4(SYS_WRITE, (uintptr_t)marker, sizeof(marker) - 1, 0, 0);
    syscall4(SYS_EXIT, 42, 0, 0, 0);
    __builtin_unreachable();
}
