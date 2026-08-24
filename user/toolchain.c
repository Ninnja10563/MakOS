#include <stdint.h>

enum {
    SYS_WRITE = 0,
    SYS_YIELD = 1,
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
};

enum {
    ELF_HEADER_SIZE = 64,
    PROGRAM_HEADER_SIZE = 56,
    CODE_OFFSET = 256,
    DATA_OFFSET = 1024,
    GENERATED_IMAGE_CAPACITY = 1536,
};

#define USER_CODE_BASE UINT64_C(0x100000000)

static uint64_t syscall4(uint64_t number, uint64_t first, uint64_t second,
                         uint64_t third, uint64_t fourth) {
    register uint64_t arg4 __asm__("r10") = fourth;
    __asm__ volatile("int $0x80"
                     : "+a"(number)
                     : "D"(first), "S"(second), "d"(third), "r"(arg4)
                     : "memory", "cc");
    return number;
}

static uint32_t parse_number(const char *source, uint32_t *cursor) {
    uint32_t value = 0;
    while (source[*cursor] >= '0' && source[*cursor] <= '9') {
        value = value * 10 + (uint32_t)(source[*cursor] - '0');
        ++*cursor;
    }
    return value;
}

static void put16(volatile uint8_t *bytes, uint32_t offset, uint16_t value) {
    bytes[offset] = (uint8_t)value;
    bytes[offset + 1] = (uint8_t)(value >> 8);
}

static void put32(volatile uint8_t *bytes, uint32_t offset, uint32_t value) {
    for (uint32_t index = 0; index < 4; ++index) {
        bytes[offset + index] = (uint8_t)(value >> (index * 8));
    }
}

static void put64(volatile uint8_t *bytes, uint32_t offset, uint64_t value) {
    for (uint32_t index = 0; index < 8; ++index) {
        bytes[offset + index] = (uint8_t)(value >> (index * 8));
    }
}

static void emit8(volatile uint8_t *bytes, uint32_t *cursor, uint8_t value) {
    bytes[(*cursor)++] = value;
}

static void emit32(volatile uint8_t *bytes, uint32_t *cursor, uint32_t value) {
    put32(bytes, *cursor, value);
    *cursor += 4;
}

static void emit64(volatile uint8_t *bytes, uint32_t *cursor, uint64_t value) {
    put64(bytes, *cursor, value);
    *cursor += 8;
}

static uint32_t emit_conditional_jump(volatile uint8_t *bytes,
                                      uint32_t *cursor, uint8_t operation) {
    emit8(bytes, cursor, 0x0f);
    emit8(bytes, cursor, operation);
    uint32_t displacement = *cursor;
    emit32(bytes, cursor, 0);
    return displacement;
}

static uint32_t emit_jump(volatile uint8_t *bytes, uint32_t *cursor) {
    emit8(bytes, cursor, 0xe9);
    uint32_t displacement = *cursor;
    emit32(bytes, cursor, 0);
    return displacement;
}

static void patch_jump(volatile uint8_t *bytes, uint32_t displacement,
                       uint32_t target) {
    put32(bytes, displacement, target - (displacement + 4));
}

static uint32_t emit_elf(volatile uint8_t *image, uint32_t result) {
    static const char message[] =
        "MAKOS_GENERATED_APP_OK source=makfs loader=elf64 emitted_by=toolchain "
        "result=42 isolated=1 segments=2 data_nx=1 startup_abi=1\n";
    for (uint32_t index = 0; index < GENERATED_IMAGE_CAPACITY; ++index) {
        image[index] = 0;
    }

    image[0] = 0x7f;
    image[1] = 'E';
    image[2] = 'L';
    image[3] = 'F';
    image[4] = 2; /* ELFCLASS64 */
    image[5] = 1; /* little endian */
    image[6] = 1; /* ELF version */
    put16(image, 16, 2); /* ET_EXEC */
    put16(image, 18, 62); /* EM_X86_64 */
    put32(image, 20, 1);
    put64(image, 24, USER_CODE_BASE);
    put64(image, 32, ELF_HEADER_SIZE);
    put16(image, 52, ELF_HEADER_SIZE);
    put16(image, 54, PROGRAM_HEADER_SIZE);
    put16(image, 56, 2);

    uint32_t data_header = ELF_HEADER_SIZE + PROGRAM_HEADER_SIZE;
    put32(image, data_header, 1);     /* PT_LOAD */
    put32(image, data_header + 4, 4); /* PF_R, NX */
    put64(image, data_header + 8, DATA_OFFSET);
    put64(image, data_header + 16, USER_CODE_BASE + 0x1000);
    put64(image, data_header + 24, USER_CODE_BASE + 0x1000);
    put64(image, data_header + 32, sizeof(message) - 1);
    put64(image, data_header + 40, sizeof(message) - 1);
    put64(image, data_header + 48, 1);

    uint32_t cursor = CODE_OFFSET;
    uint32_t failures[32];
    uint32_t failure_count = 0;

#define E8(value) emit8(image, &cursor, (value))
#define E32(value) emit32(image, &cursor, (value))
#define E64(value) emit64(image, &cursor, (value))
#define FAIL_IF_NOT_EQUAL()                                                   \
    failures[failure_count++] = emit_conditional_jump(image, &cursor, 0x85)

    /* rdi=argc: accept explicit ABI57 vector or ABI56 default argv[0]. */
    E8(0x48);
    E8(0x83);
    E8(0xff);
    E8(3);
    uint32_t explicit_jump = emit_conditional_jump(image, &cursor, 0x84);
    E8(0x48);
    E8(0x83);
    E8(0xff);
    E8(1);
    FAIL_IF_NOT_EQUAL();

    /* Legacy layout: argc, argv[0], NULL, envp NULL, auxv. */
    E8(0x48);
    E8(0x83);
    E8(0x7c);
    E8(0x24);
    E8(0x00);
    E8(1);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x8d);
    E8(0x44);
    E8(0x24);
    E8(0x08);
    E8(0x48);
    E8(0x39);
    E8(0xc6);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x8b);
    E8(0x06);
    E8(0x80);
    E8(0x38);
    E8('/');
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x83);
    E8(0x7e);
    E8(0x08);
    E8(0x00);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x8d);
    E8(0x44);
    E8(0x24);
    E8(0x18);
    E8(0x48);
    E8(0x39);
    E8(0xc2);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x83);
    E8(0x3a);
    E8(0x00);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x83);
    E8(0x7c);
    E8(0x24);
    E8(0x20);
    E8(6);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x81);
    E8(0x7c);
    E8(0x24);
    E8(0x28);
    E32(4096);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x83);
    E8(0x7c);
    E8(0x24);
    E8(0x30);
    E8(9);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x8b);
    E8(0x44);
    E8(0x24);
    E8(0x38);
    E8(0x48);
    E8(0xbb);
    E64(USER_CODE_BASE);
    E8(0x48);
    E8(0x39);
    E8(0xd8);
    FAIL_IF_NOT_EQUAL();
    uint32_t legacy_success = emit_jump(image, &cursor);

    /* Explicit layout: argv={path,"alpha","42"}, env={"MODE=test"}. */
    uint32_t explicit_start = cursor;
    patch_jump(image, explicit_jump, explicit_start);
    E8(0x48);
    E8(0x83);
    E8(0x7c);
    E8(0x24);
    E8(0x00);
    E8(3);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x8d);
    E8(0x44);
    E8(0x24);
    E8(0x08);
    E8(0x48);
    E8(0x39);
    E8(0xc6);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x8d);
    E8(0x44);
    E8(0x24);
    E8(0x28);
    E8(0x48);
    E8(0x39);
    E8(0xc2);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x8b);
    E8(0x06);
    E8(0x80);
    E8(0x38);
    E8('/');
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x8b);
    E8(0x46);
    E8(0x08);
    E8(0x81);
    E8(0x38);
    E32(UINT32_C(0x68706c61));
    FAIL_IF_NOT_EQUAL();
    E8(0x66);
    E8(0x81);
    E8(0x78);
    E8(0x04);
    E8(0x61);
    E8(0x00);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x8b);
    E8(0x46);
    E8(0x10);
    E8(0x66);
    E8(0x81);
    E8(0x38);
    E8(0x34);
    E8(0x32);
    FAIL_IF_NOT_EQUAL();
    E8(0x80);
    E8(0x78);
    E8(0x02);
    E8(0x00);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x83);
    E8(0x7e);
    E8(0x18);
    E8(0x00);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x8b);
    E8(0x02);
    E8(0x48);
    E8(0xbb);
    E64(UINT64_C(0x7365743d45444f4d));
    E8(0x48);
    E8(0x39);
    E8(0x18);
    FAIL_IF_NOT_EQUAL();
    E8(0x66);
    E8(0x81);
    E8(0x78);
    E8(0x08);
    E8(0x74);
    E8(0x00);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x83);
    E8(0x7a);
    E8(0x08);
    E8(0x00);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x83);
    E8(0x7c);
    E8(0x24);
    E8(0x38);
    E8(6);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x81);
    E8(0x7c);
    E8(0x24);
    E8(0x40);
    E32(4096);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x83);
    E8(0x7c);
    E8(0x24);
    E8(0x48);
    E8(9);
    FAIL_IF_NOT_EQUAL();
    E8(0x48);
    E8(0x8b);
    E8(0x44);
    E8(0x24);
    E8(0x50);
    E8(0x48);
    E8(0xbb);
    E64(USER_CODE_BASE);
    E8(0x48);
    E8(0x39);
    E8(0xd8);
    FAIL_IF_NOT_EQUAL();

    uint32_t success = cursor;
    patch_jump(image, legacy_success, success);
    image[cursor++] = 0xb8; /* mov eax, SYS_YIELD */
    put32(image, cursor, SYS_YIELD);
    cursor += 4;
    image[cursor++] = 0xcd;
    image[cursor++] = 0x80;
    image[cursor++] = 0xb8; /* mov eax, SYS_WRITE */
    put32(image, cursor, SYS_WRITE);
    cursor += 4;
    image[cursor++] = 0x48; /* movabs rdi, message */
    image[cursor++] = 0xbf;
    put64(image, cursor, USER_CODE_BASE + 0x1000);
    cursor += 8;
    image[cursor++] = 0xbe; /* mov esi, message length */
    put32(image, cursor, sizeof(message) - 1);
    cursor += 4;
    image[cursor++] = 0xcd;
    image[cursor++] = 0x80;
    image[cursor++] = 0xb8; /* mov eax, SYS_EXIT */
    put32(image, cursor, SYS_EXIT);
    cursor += 4;
    image[cursor++] = 0xbf; /* mov edi, result */
    put32(image, cursor, result);
    cursor += 4;
    image[cursor++] = 0xcd;
    image[cursor++] = 0x80;
    image[cursor++] = 0x0f; /* ud2 if exit returns */
    image[cursor++] = 0x0b;

    uint32_t common_failure_jumps[32];
    for (uint32_t index = 0; index < failure_count; ++index) {
        patch_jump(image, failures[index], cursor);
        E8(0x6a); /* push distinct assertion code */
        E8(50 + index);
        E8(0x5f); /* pop rdi */
        common_failure_jumps[index] = emit_jump(image, &cursor);
    }
    uint32_t failure = cursor;
    E8(0xb8); /* mov eax, SYS_EXIT */
    E32(SYS_EXIT);
    E8(0xcd);
    E8(0x80);
    E8(0x0f);
    E8(0x0b);
    for (uint32_t index = 0; index < failure_count; ++index) {
        patch_jump(image, common_failure_jumps[index], failure);
    }
    uint32_t code_size = cursor - CODE_OFFSET;
    if (cursor > DATA_OFFSET || failure_count > 32) {
        return 0;
    }

    put32(image, ELF_HEADER_SIZE, 1);     /* PT_LOAD */
    put32(image, ELF_HEADER_SIZE + 4, 5); /* PF_R | PF_X */
    put64(image, ELF_HEADER_SIZE + 8, CODE_OFFSET);
    put64(image, ELF_HEADER_SIZE + 16, USER_CODE_BASE);
    put64(image, ELF_HEADER_SIZE + 24, USER_CODE_BASE);
    put64(image, ELF_HEADER_SIZE + 32, code_size);
    put64(image, ELF_HEADER_SIZE + 40, code_size);
    put64(image, ELF_HEADER_SIZE + 48, 1);

#undef E8
#undef E32
#undef E64
#undef FAIL_IF_NOT_EQUAL

    cursor = DATA_OFFSET;
    for (uint32_t index = 0; index < sizeof(message) - 1; ++index) {
        image[cursor++] = (uint8_t)message[index];
    }
    return cursor;
}

__attribute__((section(".text._start"), noreturn)) void _start(void) {
    static const char source[] = "20+22";
    static const char passed[] =
        "MAKOS_TOOLCHAIN_APP_OK compiler=expr assembler=x86_64 source=20+22 "
        "emitted=6 result=42 wx_denied=1 rx_exec=1\n";
    static const char elf_passed[] =
        "MAKOS_TOOLCHAIN_ELF_OK path=/home/user/generated.elf format=elf64 "
        "persisted=1 emitted_by=toolchain result=42 segments=2 data_nx=1\n";
    static const char elf_persisted[] =
        "MAKOS_TOOLCHAIN_ELF_PERSIST_OK path=/home/user/generated.elf "
        "existing=1 magic=elf64 remount=1\n";
    static const char path[] = "/home/user/generated.elf";
    static const char overlap_path[] = "/home/user/overlap.elf";
    static const char overlap_passed[] =
        "MAKOS_TOOLCHAIN_ELF_REJECT_FIXTURE_OK path=/home/user/overlap.elf "
        "layout=overlapping-load-segments\n";
    uint32_t cursor = 0;
    uint32_t value = parse_number(source, &cursor);
    if (source[cursor++] != '+') {
        syscall4(SYS_EXIT, 70, 0, 0, 0);
    }
    value += parse_number(source, &cursor);
    if (source[cursor] != '\0') {
        syscall4(SYS_EXIT, 71, 0, 0, 0);
    }

    uint8_t *code = (uint8_t *)(uintptr_t)syscall4(SYS_VM_MAP, 0, 0, 0, 0);
    if ((uintptr_t)code == UINT64_MAX) {
        syscall4(SYS_EXIT, 72, 0, 0, 0);
    }
    code[0] = 0xb8; /* mov eax, imm32 */
    code[1] = (uint8_t)value;
    code[2] = (uint8_t)(value >> 8);
    code[3] = (uint8_t)(value >> 16);
    code[4] = (uint8_t)(value >> 24);
    code[5] = 0xc3; /* ret */

    if (syscall4(SYS_VM_PROTECT, (uintptr_t)code,
                 PROT_READ | PROT_WRITE | PROT_EXEC, 0, 0) != 0 ||
        syscall4(SYS_VM_PROTECT, (uintptr_t)code, PROT_READ | PROT_EXEC, 0, 0) !=
            1) {
        syscall4(SYS_EXIT, 73, 0, 0, 0);
    }
    uint32_t result = ((uint32_t(*)(void))(uintptr_t)code)();
    if (result != 42) {
        syscall4(SYS_EXIT, 74, 0, 0, 0);
    }
    volatile uint8_t image[GENERATED_IMAGE_CAPACITY];
    uint32_t image_length = emit_elf(image, result);
    if (image_length == 0) {
        syscall4(SYS_EXIT, 81, 0, 0, 0);
    }
    uint64_t created =
        syscall4(SYS_CREATE, (uintptr_t)path, sizeof(path) - 1, 0, 0);
    if (created == 0) {
        uint8_t magic[4];
        uint64_t existing =
            syscall4(SYS_OPEN, (uintptr_t)path, sizeof(path) - 1, 0, 0);
        if (existing == UINT64_MAX ||
            syscall4(SYS_READ, existing, (uintptr_t)magic, sizeof(magic), 0) !=
                sizeof(magic) ||
            magic[0] != 0x7f || magic[1] != 'E' || magic[2] != 'L' ||
            magic[3] != 'F' || syscall4(SYS_CLOSE, existing, 0, 0, 0) != 1) {
            syscall4(SYS_EXIT, 78, 0, 0, 0);
        }
        syscall4(SYS_WRITE, (uintptr_t)elf_persisted,
                 sizeof(elf_persisted) - 1, 0, 0);
    }
    uint64_t fd = syscall4(SYS_OPEN, (uintptr_t)path, sizeof(path) - 1, 1, 0);
    if (fd == UINT64_MAX) {
        syscall4(SYS_EXIT, 75, 0, 0, 0);
    }
    if (syscall4(SYS_FILE_WRITE, fd, (uintptr_t)image, image_length, 0) !=
        image_length) {
        syscall4(SYS_EXIT, 76, 0, 0, 0);
    }
    if (syscall4(SYS_CLOSE, fd, 0, 0, 0) != 1) {
        syscall4(SYS_EXIT, 77, 0, 0, 0);
    }
    uint32_t data_header = ELF_HEADER_SIZE + PROGRAM_HEADER_SIZE;
    put64(image, data_header + 16, USER_CODE_BASE);
    put64(image, data_header + 24, USER_CODE_BASE);
    (void)syscall4(SYS_UNLINK, (uintptr_t)overlap_path,
                   sizeof(overlap_path) - 1, 0, 0);
    if (syscall4(SYS_CREATE, (uintptr_t)overlap_path,
                 sizeof(overlap_path) - 1, 0, 0) != 1) {
        syscall4(SYS_EXIT, 79, 0, 0, 0);
    }
    fd = syscall4(SYS_OPEN, (uintptr_t)overlap_path,
                  sizeof(overlap_path) - 1, 1, 0);
    if (fd == UINT64_MAX ||
        syscall4(SYS_FILE_WRITE, fd, (uintptr_t)image, image_length, 0) !=
            image_length ||
        syscall4(SYS_CLOSE, fd, 0, 0, 0) != 1) {
        syscall4(SYS_EXIT, 80, 0, 0, 0);
    }
    syscall4(SYS_WRITE, (uintptr_t)passed, sizeof(passed) - 1, 0, 0);
    syscall4(SYS_WRITE, (uintptr_t)elf_passed, sizeof(elf_passed) - 1, 0, 0);
    syscall4(SYS_WRITE, (uintptr_t)overlap_passed,
             sizeof(overlap_passed) - 1, 0, 0);
    syscall4(SYS_EXIT, result, 0, 0, 0);
    __builtin_unreachable();
}
