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
    SECTION_HEADER_SIZE = 64,
    SYMBOL_SIZE = 24,
    RELA_SIZE = 24,
    CODE_OFFSET = 256,
    DATA_OFFSET = 768,
    IMAGE_CAPACITY = 1024,
    OBJECT_CAPACITY = 1024,
    R_AARCH64_CALL26 = 283,
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

static size_t align_up(size_t value, size_t alignment) {
    return (value + alignment - 1) & ~(alignment - 1);
}

static int range_ok(size_t offset, size_t count, size_t capacity) {
    return offset <= capacity && count <= capacity - offset;
}

static void copy_bytes(volatile uint8_t *destination, const uint8_t *source,
                       size_t count) {
    for (size_t index = 0; index < count; ++index) destination[index] = source[index];
}

static uint16_t get16(const uint8_t *input, size_t offset) {
    return (uint16_t)input[offset] | ((uint16_t)input[offset + 1] << 8);
}

static uint32_t get32(const uint8_t *input, size_t offset) {
    uint32_t value = 0;
    for (size_t index = 0; index < 4; ++index)
        value |= (uint32_t)input[offset + index] << (index * 8);
    return value;
}

static uint64_t get64(const uint8_t *input, size_t offset) {
    uint64_t value = 0;
    for (size_t index = 0; index < 8; ++index)
        value |= (uint64_t)input[offset + index] << (index * 8);
    return value;
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

enum { MAX_LABELS = 8, MAX_LABEL_BYTES = 16, MAX_RELOCATIONS = 4 };

struct label {
    char name[MAX_LABEL_BYTES];
    size_t length;
    size_t offset;
};

struct relocation {
    char name[MAX_LABEL_BYTES];
    size_t length;
    size_t offset;
};

static int name_byte(char byte, int first) {
    if ((byte >= 'a' && byte <= 'z') || byte == '_') return 1;
    return !first && byte >= '0' && byte <= '9';
}

static int label_name(const char *source, size_t start, size_t end,
                      char output[MAX_LABEL_BYTES], size_t *name_length) {
    size_t count = 0;
    if (start == end || !name_byte(source[start], 1)) return 0;
    while (start < end) {
        char byte = source[start++];
        if (!name_byte(byte, count == 0) || count == MAX_LABEL_BYTES) return 0;
        output[count++] = byte;
    }
    *name_length = count;
    return 1;
}

static int same_name(const char *first, size_t first_length, const char *second,
                     size_t second_length) {
    if (first_length != second_length) return 0;
    for (size_t index = 0; index < first_length; ++index)
        if (first[index] != second[index]) return 0;
    return 1;
}

static int same_label(const struct label *label, const char *name, size_t count) {
    return same_name(label->name, label->length, name, count);
}

/* Bounded genuine two-pass assembler with labels and external BL relocations. */
static size_t assemble(const char *source, size_t source_length,
                       volatile uint8_t *code, size_t capacity,
                       struct relocation relocations[MAX_RELOCATIONS],
                       size_t *relocation_count) {
    struct label labels[MAX_LABELS] = {0};
    size_t label_count = 0, cursor = 0, output = 0;
    if (relocation_count) *relocation_count = 0;
    while (cursor < source_length) {
        size_t line_start = cursor;
        while (cursor < source_length && source[cursor] != '\n') ++cursor;
        if (cursor == source_length || cursor == line_start) return 0;
        size_t line_end = cursor++;
        if (source[line_end - 1] == ':') {
            if (label_count == MAX_LABELS ||
                !label_name(source, line_start, line_end - 1,
                            labels[label_count].name,
                            &labels[label_count].length))
                return 0;
            for (size_t index = 0; index < label_count; ++index)
                if (same_label(&labels[index], labels[label_count].name,
                               labels[label_count].length))
                    return 0;
            labels[label_count++].offset = output;
        } else {
            if (output + 4 > capacity) return 0;
            output += 4;
        }
    }

    cursor = 0;
    output = 0;
    while (cursor < source_length) {
        size_t line_end = cursor;
        while (line_end < source_length && source[line_end] != '\n') ++line_end;
        if (line_end == source_length) return 0;
        if (line_end > cursor && source[line_end - 1] == ':') {
            cursor = line_end + 1;
            continue;
        }
        uint32_t instruction;
        if (consume(source, line_end, &cursor, "mov x")) {
            uint32_t reg, immediate;
            if (!decimal(source, line_end, &cursor, &reg) || reg > 30 ||
                !consume(source, line_end, &cursor, ", #") ||
                !decimal(source, line_end, &cursor, &immediate))
                return 0;
            instruction = UINT32_C(0xd2800000) | (immediate << 5) | reg;
        } else if (consume(source, line_end, &cursor, "cmp x")) {
            uint32_t reg, immediate;
            if (!decimal(source, line_end, &cursor, &reg) || reg > 30 ||
                !consume(source, line_end, &cursor, ", #") ||
                !decimal(source, line_end, &cursor, &immediate) || immediate > 4095)
                return 0;
            instruction = UINT32_C(0xf100001f) | (immediate << 10) | (reg << 5);
        } else if (consume(source, line_end, &cursor, "ldr x")) {
            uint32_t target, base, immediate;
            if (!decimal(source, line_end, &cursor, &target) || target > 30 ||
                !consume(source, line_end, &cursor, ", [x") ||
                !decimal(source, line_end, &cursor, &base) || base > 30 ||
                !consume(source, line_end, &cursor, ", #") ||
                !decimal(source, line_end, &cursor, &immediate) ||
                immediate > 32760 || immediate % 8 != 0 ||
                !consume(source, line_end, &cursor, "]"))
                return 0;
            instruction = UINT32_C(0xf9400000) | ((immediate / 8) << 10) |
                          (base << 5) | target;
        } else if (consume(source, line_end, &cursor, "ldrb w")) {
            uint32_t target, base;
            if (!decimal(source, line_end, &cursor, &target) || target > 30 ||
                !consume(source, line_end, &cursor, ", [x") ||
                !decimal(source, line_end, &cursor, &base) || base > 30 ||
                !consume(source, line_end, &cursor, "]"))
                return 0;
            instruction = UINT32_C(0x39400000) | (base << 5) | target;
        } else if (consume(source, line_end, &cursor, "b.eq ") ||
                   consume(source, line_end, &cursor, "b.ne ")) {
            uint32_t condition = source[cursor - 3] == 'e' ? 0 : 1;
            size_t target_index = MAX_LABELS;
            for (size_t index = 0; index < label_count; ++index)
                if (same_label(&labels[index], &source[cursor], line_end - cursor)) {
                    target_index = index;
                    break;
                }
            if (target_index == MAX_LABELS) return 0;
            int64_t byte_delta = (int64_t)labels[target_index].offset -
                                 (int64_t)output;
            if (byte_delta % 4 != 0) return 0;
            int64_t delta = byte_delta / 4;
            if (delta < -262144 || delta > 262143) return 0;
            instruction = UINT32_C(0x54000000) |
                          (((uint32_t)delta & UINT32_C(0x7ffff)) << 5) |
                          condition;
            cursor = line_end;
        } else if (consume(source, line_end, &cursor, "bl ")) {
            char target[MAX_LABEL_BYTES];
            size_t target_length = 0;
            if (!label_name(source, cursor, line_end, target, &target_length))
                return 0;
            size_t target_index = MAX_LABELS;
            for (size_t index = 0; index < label_count; ++index)
                if (same_label(&labels[index], target, target_length)) {
                    target_index = index;
                    break;
                }
            instruction = UINT32_C(0x94000000);
            if (target_index != MAX_LABELS) {
                int64_t byte_delta = (int64_t)labels[target_index].offset -
                                     (int64_t)output;
                if (byte_delta % 4 != 0) return 0;
                int64_t delta = byte_delta / 4;
                if (delta < -33554432 || delta > 33554431) return 0;
                instruction |= (uint32_t)delta & UINT32_C(0x03ffffff);
            } else {
                if (!relocations || !relocation_count ||
                    *relocation_count == MAX_RELOCATIONS)
                    return 0;
                struct relocation *relocation = &relocations[*relocation_count];
                for (size_t index = 0; index < target_length; ++index)
                    relocation->name[index] = target[index];
                relocation->length = target_length;
                relocation->offset = output;
                ++*relocation_count;
            }
            cursor = line_end;
        } else if (consume(source, line_end, &cursor, "svc #0")) {
            instruction = UINT32_C(0xd4000001);
        } else if (consume(source, line_end, &cursor, "ret")) {
            instruction = UINT32_C(0xd65f03c0);
        } else {
            return 0;
        }
        if (cursor != line_end || output + 4 > capacity) return 0;
        put32(code, output, instruction);
        output += 4;
        cursor = line_end + 1;
    }
    return output;
}

/*
 * Source-driven C subset compiler.  The accepted translation-unit grammar is:
 *
 *   int identifier(int identifier) {
 *       int identifier = expression;
 *       if (expression == expression) { return expression; }
 *       while (expression != expression) { identifier = expression; }
 *       return expression;
 *   }
 *
 * Expressions contain the parameter, unsigned 16-bit constants,
 * parentheses, left-associative *, +, and -, and a bounded one-argument
 * external call.  The output follows AAPCS64, including a non-leaf save
 * frame, and is wrapped in a genuine ELF64 ET_REL object by emit_object().
 * Unsupported syntax fails closed.
 */
enum { MAX_C_LOCALS = 4 };

struct c_local {
    char name[MAX_LABEL_BYTES];
    size_t length;
};

struct c_compiler {
    const char *source;
    size_t source_length;
    size_t cursor;
    volatile uint8_t *code;
    size_t capacity;
    size_t output;
    char parameter[MAX_LABEL_BYTES];
    size_t parameter_length;
    struct c_local locals[MAX_C_LOCALS];
    size_t local_count;
    struct relocation *relocations;
    size_t *relocation_count;
};

static void c_space(struct c_compiler *compiler) {
    while (compiler->cursor < compiler->source_length) {
        char byte = compiler->source[compiler->cursor];
        if (byte != ' ' && byte != '\t' && byte != '\n' && byte != '\r') break;
        ++compiler->cursor;
    }
}

static int c_punct(struct c_compiler *compiler, char punctuation) {
    c_space(compiler);
    if (compiler->cursor == compiler->source_length ||
        compiler->source[compiler->cursor] != punctuation)
        return 0;
    ++compiler->cursor;
    return 1;
}

static int c_identifier(struct c_compiler *compiler,
                        char output[MAX_LABEL_BYTES], size_t *output_length) {
    c_space(compiler);
    size_t start = compiler->cursor;
    if (start == compiler->source_length ||
        !name_byte(compiler->source[start], 1))
        return 0;
    ++compiler->cursor;
    while (compiler->cursor < compiler->source_length &&
           name_byte(compiler->source[compiler->cursor], 0))
        ++compiler->cursor;
    return label_name(compiler->source, start, compiler->cursor, output,
                      output_length);
}

static int c_keyword(struct c_compiler *compiler, const char *keyword) {
    c_space(compiler);
    size_t keyword_length = length(keyword);
    if (compiler->cursor + keyword_length > compiler->source_length) return 0;
    for (size_t index = 0; index < keyword_length; ++index)
        if (compiler->source[compiler->cursor + index] != keyword[index])
            return 0;
    size_t end = compiler->cursor + keyword_length;
    if (end < compiler->source_length &&
        name_byte(compiler->source[end], 0))
        return 0;
    compiler->cursor = end;
    return 1;
}

static int c_variable_register(struct c_compiler *compiler, const char *name,
                               size_t name_length, uint32_t *register_out) {
    if (same_name(name, name_length, compiler->parameter,
                  compiler->parameter_length)) {
        *register_out = 23;
        return 1;
    }
    size_t local = 0;
    while (local < compiler->local_count &&
           !same_name(name, name_length, compiler->locals[local].name,
                      compiler->locals[local].length))
        ++local;
    if (local == compiler->local_count) return 0;
    *register_out = 19 + (uint32_t)local;
    return 1;
}

static int c_emit(struct c_compiler *compiler, uint32_t instruction) {
    if (compiler->output + 4 > compiler->capacity) return 0;
    put32(compiler->code, compiler->output, instruction);
    compiler->output += 4;
    return 1;
}

static int c_number(struct c_compiler *compiler, uint32_t *value) {
    c_space(compiler);
    return decimal(compiler->source, compiler->source_length,
                   &compiler->cursor, value);
}

static int c_additive(struct c_compiler *compiler, uint32_t destination);

static int c_primary(struct c_compiler *compiler, uint32_t destination) {
    if (destination > 7) return 0;
    c_space(compiler);
    if (compiler->cursor < compiler->source_length &&
        compiler->source[compiler->cursor] == '(') {
        ++compiler->cursor;
        return c_additive(compiler, destination) && c_punct(compiler, ')');
    }
    uint32_t immediate;
    size_t saved = compiler->cursor;
    if (c_number(compiler, &immediate))
        return c_emit(compiler, UINT32_C(0x52800000) |
                                (immediate << 5) | destination);
    compiler->cursor = saved;
    char identifier[MAX_LABEL_BYTES];
    size_t identifier_length;
    if (!c_identifier(compiler, identifier, &identifier_length)) return 0;
    c_space(compiler);
    if (compiler->cursor < compiler->source_length &&
        compiler->source[compiler->cursor] == '(') {
        if (destination != 0 || !compiler->relocations ||
            !compiler->relocation_count ||
            *compiler->relocation_count == MAX_RELOCATIONS)
            return 0;
        ++compiler->cursor;
        if (!c_additive(compiler, 0) || !c_punct(compiler, ')')) return 0;
        struct relocation *relocation =
            &compiler->relocations[*compiler->relocation_count];
        for (size_t index = 0; index < identifier_length; ++index)
            relocation->name[index] = identifier[index];
        relocation->length = identifier_length;
        relocation->offset = compiler->output;
        ++*compiler->relocation_count;
        return c_emit(compiler, UINT32_C(0x94000000));
    }
    uint32_t source_register = 0;
    if (!c_variable_register(compiler, identifier, identifier_length,
                             &source_register))
        return 0;
    return c_emit(compiler, UINT32_C(0x2a0003e0) |
                            (source_register << 16) | destination);
}

static int c_multiplicative(struct c_compiler *compiler,
                            uint32_t destination) {
    if (!c_primary(compiler, destination)) return 0;
    for (;;) {
        c_space(compiler);
        if (compiler->cursor == compiler->source_length ||
            compiler->source[compiler->cursor] != '*')
            return 1;
        ++compiler->cursor;
        if (!c_primary(compiler, destination + 1) ||
            !c_emit(compiler, UINT32_C(0x1b007c00) |
                              ((destination + 1) << 16) |
                              (destination << 5) | destination))
            return 0;
    }
}

static int c_additive(struct c_compiler *compiler, uint32_t destination) {
    if (!c_multiplicative(compiler, destination)) return 0;
    for (;;) {
        c_space(compiler);
        if (compiler->cursor == compiler->source_length) return 1;
        char operation = compiler->source[compiler->cursor];
        if (operation != '+' && operation != '-') return 1;
        ++compiler->cursor;
        if (!c_multiplicative(compiler, destination + 1)) return 0;
        uint32_t opcode = operation == '+' ? UINT32_C(0x0b000000)
                                           : UINT32_C(0x4b000000);
        if (!c_emit(compiler, opcode | ((destination + 1) << 16) |
                              (destination << 5) | destination))
            return 0;
    }
}

static int c_comparison(struct c_compiler *compiler,
                        uint32_t *false_condition) {
    c_space(compiler);
    if (compiler->cursor + 2 > compiler->source_length) return 0;
    char first = compiler->source[compiler->cursor];
    if (compiler->source[compiler->cursor + 1] != '=' ||
        (first != '=' && first != '!'))
        return 0;
    *false_condition = first == '=' ? 1 : 0; /* B.NE or B.EQ. */
    compiler->cursor += 2;
    return 1;
}

static int c_patch_conditional(struct c_compiler *compiler, size_t branch,
                               size_t target, uint32_t condition) {
    int64_t delta = (int64_t)target - (int64_t)branch;
    if (delta % 4 != 0) return 0;
    int64_t immediate = delta / 4;
    if (immediate < -262144 || immediate > 262143 || condition > 15) return 0;
    put32(compiler->code, branch,
          UINT32_C(0x54000000) |
              (((uint32_t)immediate & UINT32_C(0x7ffff)) << 5) | condition);
    return 1;
}

static int c_patch_branch(struct c_compiler *compiler, size_t branch,
                          size_t target) {
    int64_t delta = (int64_t)target - (int64_t)branch;
    if (delta % 4 != 0) return 0;
    int64_t immediate = delta / 4;
    if (immediate < -33554432 || immediate > 33554431) return 0;
    put32(compiler->code, branch,
          UINT32_C(0x14000000) |
              ((uint32_t)immediate & UINT32_C(0x03ffffff)));
    return 1;
}

static int c_condition(struct c_compiler *compiler,
                       uint32_t *false_condition) {
    return c_punct(compiler, '(') && c_additive(compiler, 0) &&
           c_comparison(compiler, false_condition) &&
           c_additive(compiler, 1) && c_punct(compiler, ')') &&
           c_emit(compiler, UINT32_C(0x6b00001f) | (UINT32_C(1) << 16));
}

static int c_epilogue(struct c_compiler *compiler) {
    return c_emit(compiler, UINT32_C(0xf9401bf7)) &&
           c_emit(compiler, UINT32_C(0xa9425bf5)) &&
           c_emit(compiler, UINT32_C(0xa94153f3)) &&
           c_emit(compiler, UINT32_C(0xa8c47bfd)) &&
           c_emit(compiler, UINT32_C(0xd65f03c0));
}

static int c_return_statement(struct c_compiler *compiler) {
    return c_additive(compiler, 0) && c_punct(compiler, ';') &&
           c_epilogue(compiler);
}

static int c_declaration(struct c_compiler *compiler) {
    if (compiler->local_count == MAX_C_LOCALS) return 0;
    struct c_local local = {0};
    if (!c_identifier(compiler, local.name, &local.length) ||
        same_name(local.name, local.length, compiler->parameter,
                  compiler->parameter_length))
        return 0;
    for (size_t index = 0; index < compiler->local_count; ++index)
        if (same_name(local.name, local.length, compiler->locals[index].name,
                      compiler->locals[index].length))
            return 0;
    if (!c_punct(compiler, '=') || !c_additive(compiler, 0) ||
        !c_punct(compiler, ';'))
        return 0;
    uint32_t register_index = 19 + (uint32_t)compiler->local_count;
    if (!c_emit(compiler, UINT32_C(0x2a0003e0) | register_index)) return 0;
    compiler->locals[compiler->local_count++] = local;
    return 1;
}

static int c_assignment(struct c_compiler *compiler) {
    char identifier[MAX_LABEL_BYTES] = {0};
    size_t identifier_length = 0;
    uint32_t destination = 0;
    if (!c_identifier(compiler, identifier, &identifier_length) ||
        !c_variable_register(compiler, identifier, identifier_length,
                             &destination) ||
        !c_punct(compiler, '=') || !c_additive(compiler, 0) ||
        !c_punct(compiler, ';'))
        return 0;
    return c_emit(compiler, UINT32_C(0x2a0003e0) | destination);
}

static int c_if_return(struct c_compiler *compiler) {
    uint32_t false_condition = 0;
    if (!c_condition(compiler, &false_condition)) return 0;
    size_t branch = compiler->output;
    if (!c_emit(compiler, UINT32_C(0x54000000) | false_condition) ||
        !c_punct(compiler, '{') || !c_keyword(compiler, "return") ||
        !c_return_statement(compiler) || !c_punct(compiler, '}'))
        return 0;
    return c_patch_conditional(compiler, branch, compiler->output,
                               false_condition);
}

static int c_while(struct c_compiler *compiler) {
    size_t loop = compiler->output;
    uint32_t false_condition = 0;
    if (!c_condition(compiler, &false_condition)) return 0;
    size_t exit_branch = compiler->output;
    if (!c_emit(compiler, UINT32_C(0x54000000) | false_condition) ||
        !c_punct(compiler, '{'))
        return 0;
    size_t assignments = 0;
    for (;;) {
        c_space(compiler);
        if (compiler->cursor == compiler->source_length) return 0;
        if (compiler->source[compiler->cursor] == '}') break;
        if (!c_assignment(compiler)) return 0;
        ++assignments;
    }
    if (assignments == 0 || !c_punct(compiler, '}')) return 0;
    size_t back_branch = compiler->output;
    if (!c_emit(compiler, UINT32_C(0x14000000)) ||
        !c_patch_branch(compiler, back_branch, loop) ||
        !c_patch_conditional(compiler, exit_branch, compiler->output,
                             false_condition))
        return 0;
    return 1;
}

static size_t compile_c(const char *source, size_t source_length,
                        volatile uint8_t *code, size_t capacity,
                        char function[MAX_LABEL_BYTES],
                        size_t *function_length,
                        struct relocation relocations[MAX_RELOCATIONS],
                        size_t *relocation_count) {
    if (relocation_count) *relocation_count = 0;
    struct c_compiler compiler = {
        .source = source,
        .source_length = source_length,
        .code = code,
        .capacity = capacity,
        .relocations = relocations,
        .relocation_count = relocation_count,
    };
    if (!function_length || !c_keyword(&compiler, "int") ||
        !c_identifier(&compiler, function, function_length) ||
        !c_punct(&compiler, '(') || !c_keyword(&compiler, "int") ||
        !c_identifier(&compiler, compiler.parameter,
                      &compiler.parameter_length) ||
        !c_punct(&compiler, ')') || !c_punct(&compiler, '{'))
        return 0;
    /* Preserve FP/LR and w19-w23 so emitted functions may call other objects. */
    if (!c_emit(&compiler, UINT32_C(0xa9bc7bfd)) ||
        !c_emit(&compiler, UINT32_C(0x910003fd)) ||
        !c_emit(&compiler, UINT32_C(0xa90153f3)) ||
        !c_emit(&compiler, UINT32_C(0xa9025bf5)) ||
        !c_emit(&compiler, UINT32_C(0xf9001bf7)) ||
        !c_emit(&compiler, UINT32_C(0x2a0003f7)))
        return 0;
    int terminal_return = 0;
    size_t statement_count = 0;
    for (;;) {
        c_space(&compiler);
        if (compiler.cursor == compiler.source_length) return 0;
        if (compiler.source[compiler.cursor] == '}') break;
        if (terminal_return) return 0;
        if (c_keyword(&compiler, "int")) {
            if (!c_declaration(&compiler)) return 0;
        } else if (c_keyword(&compiler, "if")) {
            if (!c_if_return(&compiler)) return 0;
        } else if (c_keyword(&compiler, "while")) {
            if (!c_while(&compiler)) return 0;
        } else if (c_keyword(&compiler, "return")) {
            if (!c_return_statement(&compiler)) return 0;
            terminal_return = 1;
        } else {
            if (!c_assignment(&compiler)) return 0;
        }
        ++statement_count;
    }
    if (statement_count == 0 || !terminal_return || !c_punct(&compiler, '}'))
        return 0;
    c_space(&compiler);
    if (compiler.cursor != compiler.source_length) return 0;
    return compiler.output;
}

static int write_file(const char *path, size_t path_length, const uint8_t *bytes,
                      size_t byte_count) {
    (void)syscall4(SYS_UNLINK, (uintptr_t)path, path_length, 0, 0);
    if (syscall4(SYS_CREATE, (uintptr_t)path, path_length, 0, 0) != 1) return 0;
    uint64_t fd = syscall4(SYS_OPEN, (uintptr_t)path, path_length, 1, 0);
    if (fd == UINT64_MAX) return 0;
    uint64_t written = syscall4(SYS_FILE_WRITE, fd, (uintptr_t)bytes,
                                byte_count, 0);
    uint64_t closed = syscall4(SYS_CLOSE, fd, 0, 0, 0);
    return written == byte_count && closed == 1;
}

static size_t read_file(const char *path, size_t path_length, uint8_t *bytes,
                        size_t capacity) {
    uint64_t fd = syscall4(SYS_OPEN, (uintptr_t)path, path_length, 0, 0);
    if (fd == UINT64_MAX) return 0;
    uint64_t count = syscall4(SYS_READ, fd, (uintptr_t)bytes, capacity, 0);
    uint64_t closed = syscall4(SYS_CLOSE, fd, 0, 0, 0);
    if (count == UINT64_MAX || count > capacity || closed != 1) return 0;
    return (size_t)count;
}

static void section(volatile uint8_t *object, size_t section_headers,
                    size_t index, uint32_t name, uint32_t type, uint64_t flags,
                    uint64_t offset, uint64_t size, uint32_t link,
                    uint32_t info, uint64_t alignment, uint64_t entry_size) {
    size_t header = section_headers + index * SECTION_HEADER_SIZE;
    put32(object, header, name);
    put32(object, header + 4, type);
    put64(object, header + 8, flags);
    put64(object, header + 24, offset);
    put64(object, header + 32, size);
    put32(object, header + 40, link);
    put32(object, header + 44, info);
    put64(object, header + 48, alignment);
    put64(object, header + 56, entry_size);
}

static void symbol(volatile uint8_t *object, size_t symbols, size_t index,
                   uint32_t name, uint16_t section_index, uint64_t value,
                   uint64_t size) {
    size_t entry = symbols + index * SYMBOL_SIZE;
    put32(object, entry, name);
    object[entry + 4] = 0x12; /* STB_GLOBAL | STT_FUNC */
    put16(object, entry + 6, section_index);
    put64(object, entry + 8, value);
    put64(object, entry + 16, size);
}

static size_t emit_object(volatile uint8_t object[OBJECT_CAPACITY],
                          const uint8_t *code, size_t code_length,
                          const char *definition, size_t definition_length,
                          const struct relocation *relocations,
                          size_t relocation_count) {
    static const uint8_t section_strings[] =
        "\0.text\0.rela.text\0.symtab\0.strtab\0.shstrtab\0";
    if (code_length == 0 || definition_length == 0 ||
        definition_length > MAX_LABEL_BYTES || relocation_count > MAX_RELOCATIONS ||
        (relocation_count != 0 && !relocations))
        return 0;
    for (size_t index = 0; index < definition_length; ++index)
        if (!name_byte(definition[index], index == 0)) return 0;

    struct relocation undefined[MAX_RELOCATIONS] = {0};
    size_t undefined_count = 0;
    uint32_t relocation_symbols[MAX_RELOCATIONS] = {0};
    uint32_t undefined_name_offsets[MAX_RELOCATIONS] = {0};
    size_t string_length = 1 + definition_length + 1;
    for (size_t index = 0; index < relocation_count; ++index) {
        if (relocations[index].length == 0 ||
            relocations[index].length > MAX_LABEL_BYTES ||
            relocations[index].offset % 4 != 0 ||
            relocations[index].offset > code_length ||
            code_length - relocations[index].offset < 4 ||
            get32(code, relocations[index].offset) != UINT32_C(0x94000000) ||
            same_name(relocations[index].name, relocations[index].length,
                      definition, definition_length))
            return 0;
        for (size_t byte = 0; byte < relocations[index].length; ++byte)
            if (!name_byte(relocations[index].name[byte], byte == 0)) return 0;
        size_t match = 0;
        while (match < undefined_count &&
               !same_name(relocations[index].name, relocations[index].length,
                          undefined[match].name, undefined[match].length))
            ++match;
        if (match == undefined_count) {
            undefined[undefined_count] = relocations[index];
            undefined_name_offsets[undefined_count] = (uint32_t)string_length;
            string_length += relocations[index].length + 1;
            ++undefined_count;
        }
        relocation_symbols[index] = 2 + (uint32_t)match;
    }

    size_t text_offset = ELF_HEADER_SIZE;
    size_t rela_offset = align_up(text_offset + code_length, 8);
    size_t rela_length = relocation_count * RELA_SIZE;
    size_t symbol_offset = align_up(rela_offset + rela_length, 8);
    size_t symbol_count = 2 + undefined_count;
    size_t string_offset = symbol_offset + symbol_count * SYMBOL_SIZE;
    size_t section_string_offset = string_offset + string_length;
    size_t section_headers = align_up(section_string_offset +
                                      sizeof(section_strings) - 1, 8);
    size_t object_length = section_headers + 6 * SECTION_HEADER_SIZE;
    if (object_length > OBJECT_CAPACITY) return 0;

    memset((void *)object, 0, OBJECT_CAPACITY);
    object[0] = 0x7f; object[1] = 'E'; object[2] = 'L'; object[3] = 'F';
    object[4] = 2; object[5] = 1; object[6] = 1;
    put16(object, 16, 1);            /* ET_REL */
    put16(object, 18, 183);          /* EM_AARCH64 */
    put32(object, 20, 1);
    put64(object, 40, section_headers);
    put16(object, 52, ELF_HEADER_SIZE);
    put16(object, 58, SECTION_HEADER_SIZE);
    put16(object, 60, 6);
    put16(object, 62, 5);
    copy_bytes(object + text_offset, code, code_length);
    for (size_t index = 0; index < relocation_count; ++index) {
        size_t entry = rela_offset + index * RELA_SIZE;
        put64(object, entry, relocations[index].offset);
        put64(object, entry + 8,
              ((uint64_t)relocation_symbols[index] << 32) | R_AARCH64_CALL26);
        put64(object, entry + 16, 0);
    }
    symbol(object, symbol_offset, 1, 1, 1, 0, code_length);
    for (size_t index = 0; index < undefined_count; ++index)
        symbol(object, symbol_offset, 2 + index,
               undefined_name_offsets[index], 0, 0, 0);
    size_t string_cursor = string_offset + 1;
    copy_bytes(object + string_cursor, (const uint8_t *)definition,
               definition_length);
    string_cursor += definition_length + 1;
    for (size_t index = 0; index < undefined_count; ++index) {
        copy_bytes(object + string_cursor,
                   (const uint8_t *)undefined[index].name,
                   undefined[index].length);
        string_cursor += undefined[index].length + 1;
    }
    copy_bytes(object + section_string_offset, section_strings,
               sizeof(section_strings) - 1);

    section(object, section_headers, 1, 1, 1, 6, text_offset, code_length,
            0, 0, 4, 0);
    section(object, section_headers, 2, 7, 4, 0, rela_offset, rela_length,
            3, 1, 8, RELA_SIZE);
    section(object, section_headers, 3, 18, 2, 0, symbol_offset,
            symbol_count * SYMBOL_SIZE, 4, 1, 8, SYMBOL_SIZE);
    section(object, section_headers, 4, 26, 3, 0, string_offset,
            string_length, 0, 0, 1, 0);
    section(object, section_headers, 5, 34, 3, 0, section_string_offset,
            sizeof(section_strings) - 1, 0, 0, 1, 0);
    return object_length;
}

struct object_view {
    const uint8_t *bytes;
    size_t length;
    size_t text_offset, text_length;
    size_t rela_offset, rela_count;
    size_t symbol_offset, symbol_count;
    size_t string_offset, string_length;
};

static int string_is(const struct object_view *object, uint32_t offset,
                     const char *expected) {
    size_t expected_length = length(expected);
    if (offset >= object->string_length ||
        expected_length >= object->string_length - offset)
        return 0;
    const uint8_t *text = object->bytes + object->string_offset + offset;
    for (size_t index = 0; index < expected_length; ++index)
        if (text[index] != (uint8_t)expected[index]) return 0;
    return text[expected_length] == 0;
}

static int parse_object(const uint8_t *bytes, size_t length,
                        struct object_view *object) {
    if (length < ELF_HEADER_SIZE || bytes[0] != 0x7f || bytes[1] != 'E' ||
        bytes[2] != 'L' || bytes[3] != 'F' || bytes[4] != 2 || bytes[5] != 1 ||
        bytes[6] != 1 || get16(bytes, 16) != 1 || get16(bytes, 18) != 183 ||
        get32(bytes, 20) != 1 || get16(bytes, 52) != ELF_HEADER_SIZE ||
        get16(bytes, 58) != SECTION_HEADER_SIZE || get16(bytes, 60) != 6 ||
        get16(bytes, 62) != 5)
        return 0;
    uint64_t section_headers64 = get64(bytes, 40);
    if (section_headers64 > SIZE_MAX) return 0;
    size_t section_headers = (size_t)section_headers64;
    if (!range_ok(section_headers, 6 * SECTION_HEADER_SIZE, length)) return 0;
    const size_t expected_names[6] = {0, 1, 7, 18, 26, 34};
    const uint32_t expected_types[6] = {0, 1, 4, 2, 3, 3};
    for (size_t index = 0; index < 6; ++index) {
        size_t header = section_headers + index * SECTION_HEADER_SIZE;
        if (get32(bytes, header) != expected_names[index] ||
            get32(bytes, header + 4) != expected_types[index])
            return 0;
    }
    size_t text = section_headers + SECTION_HEADER_SIZE;
    size_t rela = text + SECTION_HEADER_SIZE;
    size_t symbols = rela + SECTION_HEADER_SIZE;
    size_t strings = symbols + SECTION_HEADER_SIZE;
    size_t section_strings = strings + SECTION_HEADER_SIZE;
    uint64_t text_offset = get64(bytes, text + 24);
    uint64_t text_length = get64(bytes, text + 32);
    uint64_t rela_offset = get64(bytes, rela + 24);
    uint64_t rela_length = get64(bytes, rela + 32);
    uint64_t symbol_offset = get64(bytes, symbols + 24);
    uint64_t symbol_length = get64(bytes, symbols + 32);
    uint64_t string_offset = get64(bytes, strings + 24);
    uint64_t string_length = get64(bytes, strings + 32);
    uint64_t shstr_offset = get64(bytes, section_strings + 24);
    uint64_t shstr_length = get64(bytes, section_strings + 32);
    if (text_offset > SIZE_MAX || text_length > SIZE_MAX ||
        rela_offset > SIZE_MAX || rela_length > SIZE_MAX ||
        symbol_offset > SIZE_MAX || symbol_length > SIZE_MAX ||
        string_offset > SIZE_MAX || string_length > SIZE_MAX ||
        shstr_offset > SIZE_MAX || shstr_length > SIZE_MAX)
        return 0;
    if (get64(bytes, text + 8) != 6 || get64(bytes, text + 48) != 4 ||
        get32(bytes, rela + 40) != 3 || get32(bytes, rela + 44) != 1 ||
        get64(bytes, rela + 56) != RELA_SIZE || rela_length % RELA_SIZE != 0 ||
        get32(bytes, symbols + 40) != 4 || get32(bytes, symbols + 44) != 1 ||
        get64(bytes, symbols + 56) != SYMBOL_SIZE ||
        symbol_length % SYMBOL_SIZE != 0 || symbol_length < 2 * SYMBOL_SIZE ||
        !range_ok((size_t)text_offset, (size_t)text_length, length) ||
        !range_ok((size_t)rela_offset, (size_t)rela_length, length) ||
        !range_ok((size_t)symbol_offset, (size_t)symbol_length, length) ||
        !range_ok((size_t)string_offset, (size_t)string_length, length) ||
        !range_ok((size_t)shstr_offset, (size_t)shstr_length, length))
        return 0;
    object->bytes = bytes;
    object->length = length;
    object->text_offset = (size_t)text_offset;
    object->text_length = (size_t)text_length;
    object->rela_offset = (size_t)rela_offset;
    object->rela_count = (size_t)rela_length / RELA_SIZE;
    object->symbol_offset = (size_t)symbol_offset;
    object->symbol_count = (size_t)symbol_length / SYMBOL_SIZE;
    object->string_offset = (size_t)string_offset;
    object->string_length = (size_t)string_length;
    return 1;
}

static int object_string_valid(const struct object_view *object,
                               uint32_t offset) {
    if (offset == 0 || offset >= object->string_length) return 0;
    for (size_t index = offset; index < object->string_length; ++index)
        if (object->bytes[object->string_offset + index] == 0)
            return index != offset;
    return 0;
}

static int symbol_names_equal(const struct object_view *first,
                              size_t first_symbol,
                              const struct object_view *second,
                              size_t second_symbol) {
    size_t first_entry = first->symbol_offset + first_symbol * SYMBOL_SIZE;
    size_t second_entry = second->symbol_offset + second_symbol * SYMBOL_SIZE;
    uint32_t first_offset = get32(first->bytes, first_entry);
    uint32_t second_offset = get32(second->bytes, second_entry);
    if (!object_string_valid(first, first_offset) ||
        !object_string_valid(second, second_offset))
        return 0;
    for (size_t index = 0;; ++index) {
        uint8_t first_byte = first->bytes[first->string_offset + first_offset + index];
        uint8_t second_byte = second->bytes[second->string_offset + second_offset + index];
        if (first_byte != second_byte) return 0;
        if (first_byte == 0) return 1;
    }
}

static int validate_symbols(const struct object_view *object) {
    if (object->rela_count > MAX_RELOCATIONS ||
        object->symbol_count > MAX_RELOCATIONS + 2)
        return 0;
    for (size_t byte = 0; byte < SYMBOL_SIZE; ++byte)
        if (object->bytes[object->symbol_offset + byte] != 0) return 0;
    for (size_t index = 1; index < object->symbol_count; ++index) {
        size_t entry = object->symbol_offset + index * SYMBOL_SIZE;
        uint32_t name_offset = get32(object->bytes, entry);
        uint16_t section_index = get16(object->bytes, entry + 6);
        uint64_t value = get64(object->bytes, entry + 8);
        uint64_t symbol_size = get64(object->bytes, entry + 16);
        if (object->bytes[entry + 4] != 0x12 ||
            !object_string_valid(object, name_offset) ||
            (section_index != 0 && section_index != 1) ||
            (section_index == 0 && (value != 0 || symbol_size != 0)) ||
            (section_index == 1 &&
             (value >= object->text_length ||
              symbol_size > object->text_length - (size_t)value)))
            return 0;
    }
    return 1;
}

enum { MAX_LINK_OBJECTS = 3 };

static size_t link_objects(const uint8_t *const object_bytes[MAX_LINK_OBJECTS],
                           const size_t object_lengths[MAX_LINK_OBJECTS],
                           size_t object_count, uint8_t *output,
                           size_t capacity, const char *entry_name,
                           size_t *entry_out) {
    if (!entry_out || !entry_name || object_count == 0 ||
        object_count > MAX_LINK_OBJECTS)
        return 0;
    struct object_view objects[MAX_LINK_OBJECTS] = {0};
    size_t bases[MAX_LINK_OBJECTS] = {0};
    size_t output_length = 0;
    for (size_t object = 0; object < object_count; ++object) {
        if (!parse_object(object_bytes[object], object_lengths[object],
                          &objects[object]) ||
            !validate_symbols(&objects[object]))
            return 0;
        bases[object] = align_up(output_length, 4);
        if (bases[object] > capacity ||
            objects[object].text_length > capacity - bases[object])
            return 0;
        output_length = bases[object] + objects[object].text_length;
    }
    if (output_length == 0 || output_length > capacity) return 0;

    size_t entry_matches = 0;
    size_t entry_offset = 0;
    for (size_t first_object = 0; first_object < object_count; ++first_object) {
        for (size_t first_symbol = 1;
             first_symbol < objects[first_object].symbol_count; ++first_symbol) {
            size_t first_entry = objects[first_object].symbol_offset +
                                 first_symbol * SYMBOL_SIZE;
            if (get16(objects[first_object].bytes, first_entry + 6) != 1)
                continue;
            if (string_is(&objects[first_object],
                          get32(objects[first_object].bytes, first_entry),
                          entry_name)) {
                ++entry_matches;
                entry_offset = bases[first_object] +
                               (size_t)get64(objects[first_object].bytes,
                                             first_entry + 8);
            }
            for (size_t second_object = first_object;
                 second_object < object_count; ++second_object) {
                size_t start_symbol = second_object == first_object
                                          ? first_symbol + 1
                                          : 1;
                for (size_t second_symbol = start_symbol;
                     second_symbol < objects[second_object].symbol_count;
                     ++second_symbol) {
                    size_t second_entry = objects[second_object].symbol_offset +
                                          second_symbol * SYMBOL_SIZE;
                    if (get16(objects[second_object].bytes,
                              second_entry + 6) == 1 &&
                        symbol_names_equal(&objects[first_object], first_symbol,
                                           &objects[second_object], second_symbol))
                        return 0;
                }
            }
        }
    }
    if (entry_matches != 1 || entry_offset >= output_length) return 0;

    memset(output, 0, output_length);
    for (size_t object = 0; object < object_count; ++object)
        copy_bytes(output + bases[object],
                   objects[object].bytes + objects[object].text_offset,
                   objects[object].text_length);

    for (size_t object = 0; object < object_count; ++object) {
        for (size_t relocation_index = 0;
             relocation_index < objects[object].rela_count;
             ++relocation_index) {
            size_t relocation = objects[object].rela_offset +
                                relocation_index * RELA_SIZE;
            uint64_t offset64 = get64(objects[object].bytes, relocation);
            uint64_t info = get64(objects[object].bytes, relocation + 8);
            int64_t addend = (int64_t)get64(objects[object].bytes,
                                             relocation + 16);
            size_t symbol_index = (size_t)(info >> 32);
            if ((uint32_t)info != R_AARCH64_CALL26 ||
                addend != 0 ||
                symbol_index == 0 ||
                symbol_index >= objects[object].symbol_count ||
                offset64 > objects[object].text_length ||
                objects[object].text_length - (size_t)offset64 < 4)
                return 0;
            size_t symbol_entry = objects[object].symbol_offset +
                                  symbol_index * SYMBOL_SIZE;
            if (get16(objects[object].bytes, symbol_entry + 6) != 0) return 0;

            size_t target_matches = 0;
            int64_t target = 0;
            for (size_t target_object = 0; target_object < object_count;
                 ++target_object) {
                for (size_t target_symbol = 1;
                     target_symbol < objects[target_object].symbol_count;
                     ++target_symbol) {
                    size_t target_entry = objects[target_object].symbol_offset +
                                          target_symbol * SYMBOL_SIZE;
                    if (get16(objects[target_object].bytes,
                              target_entry + 6) == 1 &&
                        symbol_names_equal(&objects[object], symbol_index,
                                           &objects[target_object], target_symbol)) {
                        ++target_matches;
                        target = (int64_t)bases[target_object] +
                                 (int64_t)get64(objects[target_object].bytes,
                                                target_entry + 8) + addend;
                    }
                }
            }
            if (target_matches != 1) return 0;
            size_t source = bases[object] + (size_t)offset64;
            uint32_t instruction = get32(output, source);
            if (instruction != UINT32_C(0x94000000)) return 0;
            int64_t delta = target - (int64_t)source;
            if (delta % 4 != 0) return 0;
            int64_t immediate = delta / 4;
            if (immediate < -33554432 || immediate > 33554431) return 0;
            put32(output, source,
                  instruction |
                      ((uint32_t)immediate & UINT32_C(0x03ffffff)));
        }
    }
    *entry_out = entry_offset;
    return output_length;
}

static size_t emit_elf(volatile uint8_t image[IMAGE_CAPACITY],
                       const uint8_t *code, size_t code_length,
                       size_t entry_offset) {
    static const char provenance[] =
        "MakOS guest-native ET_REL static linker output\n";
    if (CODE_OFFSET + code_length > DATA_OFFSET || entry_offset >= code_length)
        return 0;
    memset((void *)image, 0, IMAGE_CAPACITY);
    image[0] = 0x7f; image[1] = 'E'; image[2] = 'L'; image[3] = 'F';
    image[4] = 2; image[5] = 1; image[6] = 1;
    put16(image, 16, 2);             /* ET_EXEC */
    put16(image, 18, 183);           /* EM_AARCH64 */
    put32(image, 20, 1);
    put64(image, 24, USER_BASE + CODE_OFFSET + entry_offset);
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
    copy_bytes(image + CODE_OFFSET, code, code_length);
    copy_bytes(image + DATA_OFFSET, (const uint8_t *)provenance,
               sizeof(provenance) - 1);
    return DATA_OFFSET + sizeof(provenance) - 1;
}

static void fail(uint64_t status) {
    syscall4(SYS_EXIT, status, 0, 0, 0);
    for (;;) __asm__ volatile("wfe");
}

__attribute__((section(".text._start"), noreturn)) void _start(void) {
    static const char main_source_path[] = "/home/user/generated.s";
    static const char answer_source_path[] = "/home/user/generated-answer.c";
    static const char adjust_source_path[] = "/home/user/generated-adjust.c";
    static const char main_object_path[] = "/home/user/generated-main.o";
    static const char answer_object_path[] = "/home/user/generated-answer.o";
    static const char adjust_object_path[] = "/home/user/generated-adjust.o";
    static const char output_path[] = "/home/user/generated-aarch64.elf";
    static const char main_source[] =
        "_start:\n"
        "cmp x0, #1\n"
        "b.eq success\n"
        "cmp x0, #3\n"
        "b.ne fail\n"
        "ldr x3, [x1, #8]\n"
        "ldrb w4, [x3]\n"
        "cmp x4, #97\n"
        "b.ne fail\n"
        "ldr x5, [x2, #0]\n"
        "ldrb w6, [x5]\n"
        "cmp x6, #77\n"
        "b.ne fail\n"
        "success:\n"
        "mov x0, #20\n"
        "bl answer\n"
        "mov x8, #5\n"
        "svc #0\n"
        "fail:\n"
        "mov x0, #86\n"
        "mov x8, #5\n"
        "svc #0\n";
    static const char answer_source[] =
        "int answer(int value) {\n"
        "    int normalized = (value * 3) - 20;\n"
        "    if (normalized == 40) {\n"
        "        return adjust(normalized);\n"
        "    }\n"
        "    return 86;\n"
        "}\n";
    static const char adjust_source[] =
        "int adjust(int value) {\n"
        "    int count = 0;\n"
        "    while (count != 2) {\n"
        "        value = value + 1;\n"
        "        count = count + 1;\n"
        "    }\n"
        "    return value;\n"
        "}\n";
    static const char malformed_c_source[] =
        "int answer(int value) { return value / 2; }\n";
    static const char malformed_control_source[] =
        "int answer(int value) { if (value == 1) { return 42; } }\n";
    static const char malformed_loop_source[] =
        "int adjust(int value) { while (value != 0) { value = value - 1; } }\n";
    static const char malformed_assignment_source[] =
        "int adjust(int value) { missing = value; return value; }\n";
    static const char jit_source[] = "mov x0, #42\nret\n";
    static const char marker[] =
        "MAKOS_AARCH64_LINKER_OK sources=3 languages=aarch64-asm,c-subset-v1 "
        "compiler=guest-native assembler=guest-native objects=3 "
        "format=elf64-et-rel linker=guest-native relocations=R_AARCH64_CALL26:2 "
        "symbols=_start,answer,adjust output=/home/user/generated-aarch64.elf "
        "c_source=/home/user/generated-answer.c c_abi=aapcs64-int32 "
        "c_features=parameter,local,assignment,if,equality,inequality,while,call,return "
        "nonleaf_frame=64 c_operators=mul,sub,add branch_results=42,86 "
        "loop_results=42,2 code_bytes=76,116,108 object_bytes=688,728,664 "
        "linked_bytes=300 output_bytes=815 persisted_reopened=1 malformed_c_denied=4 "
        "malformed_relocation_denied=1 unresolved_symbol_denied=1 "
        "duplicate_definition_denied=1 segments=2 "
        "code_rx=1 data_nx=1 wx_denied=1 jit_result=42\n";

    if (!write_file(main_source_path, sizeof(main_source_path) - 1,
                    (const uint8_t *)main_source, sizeof(main_source) - 1) ||
        !write_file(answer_source_path, sizeof(answer_source_path) - 1,
                    (const uint8_t *)answer_source, sizeof(answer_source) - 1) ||
        !write_file(adjust_source_path, sizeof(adjust_source_path) - 1,
                    (const uint8_t *)adjust_source, sizeof(adjust_source) - 1))
        fail(80);
    uint8_t source_input[512] = {0}, answer_input[256] = {0};
    uint8_t adjust_input[256] = {0};
    size_t source_length = read_file(main_source_path,
                                     sizeof(main_source_path) - 1,
                                     source_input, sizeof(source_input));
    size_t answer_source_length = read_file(answer_source_path,
                                            sizeof(answer_source_path) - 1,
                                            answer_input, sizeof(answer_input));
    size_t adjust_source_length = read_file(adjust_source_path,
                                            sizeof(adjust_source_path) - 1,
                                            adjust_input, sizeof(adjust_input));
    if (source_length != sizeof(main_source) - 1 ||
        answer_source_length != sizeof(answer_source) - 1 ||
        adjust_source_length != sizeof(adjust_source) - 1)
        fail(81);

    uint8_t main_code[128] = {0}, answer_code[128] = {0}, adjust_code[128] = {0};
    struct relocation main_relocations[MAX_RELOCATIONS] = {0};
    struct relocation answer_relocations[MAX_RELOCATIONS] = {0};
    struct relocation adjust_relocations[MAX_RELOCATIONS] = {0};
    size_t main_relocation_count = 0, answer_relocation_count = 0;
    size_t adjust_relocation_count = 0;
    size_t main_code_length = assemble((const char *)source_input, source_length,
                                       main_code, sizeof(main_code),
                                       main_relocations,
                                       &main_relocation_count);
    char answer_function[MAX_LABEL_BYTES] = {0};
    size_t answer_function_length = 0;
    char adjust_function[MAX_LABEL_BYTES] = {0};
    size_t adjust_function_length = 0;
    uint8_t malformed_code[128] = {0};
    char malformed_function[MAX_LABEL_BYTES] = {0};
    size_t malformed_function_length = 0;
    if (compile_c(malformed_c_source, sizeof(malformed_c_source) - 1,
                  malformed_code, sizeof(malformed_code), malformed_function,
                  &malformed_function_length, 0, 0) != 0)
        fail(82);
    malformed_function_length = 0;
    if (compile_c(malformed_control_source,
                  sizeof(malformed_control_source) - 1, malformed_code,
                  sizeof(malformed_code), malformed_function,
                  &malformed_function_length, 0, 0) != 0)
        fail(82);
    malformed_function_length = 0;
    if (compile_c(malformed_loop_source,
                  sizeof(malformed_loop_source) - 1, malformed_code,
                  sizeof(malformed_code), malformed_function,
                  &malformed_function_length, 0, 0) != 0)
        fail(82);
    malformed_function_length = 0;
    if (compile_c(malformed_assignment_source,
                  sizeof(malformed_assignment_source) - 1, malformed_code,
                  sizeof(malformed_code), malformed_function,
                  &malformed_function_length, 0, 0) != 0)
        fail(82);
    size_t answer_code_length = compile_c((const char *)answer_input,
                                          answer_source_length, answer_code,
                                          sizeof(answer_code), answer_function,
                                          &answer_function_length,
                                          answer_relocations,
                                          &answer_relocation_count);
    size_t adjust_code_length = compile_c((const char *)adjust_input,
                                          adjust_source_length, adjust_code,
                                          sizeof(adjust_code), adjust_function,
                                          &adjust_function_length,
                                          adjust_relocations,
                                          &adjust_relocation_count);
    if (main_code_length != 76 || answer_code_length != 116 ||
        adjust_code_length != 108 ||
        !same_name(answer_function, answer_function_length, "answer", 6) ||
        !same_name(adjust_function, adjust_function_length, "adjust", 6) ||
        main_relocation_count != 1 || main_relocations[0].offset != 52 ||
        answer_relocation_count != 1 || answer_relocations[0].offset != 68 ||
        !same_name(answer_relocations[0].name,
                   answer_relocations[0].length, "adjust", 6) ||
        adjust_relocation_count != 0)
        fail(82);

    uint8_t *jit = (uint8_t *)(uintptr_t)syscall4(SYS_VM_MAP, 0, 0, 0, 0);
    if ((uintptr_t)jit == UINT64_MAX) fail(83);
    size_t jit_length = assemble(jit_source, sizeof(jit_source) - 1, jit, 64,
                                 0, 0);
    if (jit_length != 8 ||
        syscall4(SYS_VM_PROTECT, (uintptr_t)jit,
                 PROT_READ | PROT_WRITE | PROT_EXEC, 0, 0) != 0 ||
        syscall4(SYS_VM_PROTECT, (uintptr_t)jit, PROT_READ | PROT_EXEC, 0, 0) != 1)
        fail(83);
    uint64_t jit_result = ((uint64_t (*)(void))(uintptr_t)jit)();
    if (jit_result != 42) fail(84);

    uint8_t rejected_object[OBJECT_CAPACITY];
    struct relocation invalid_emit = main_relocations[0];
    invalid_emit.offset = main_code_length;
    if (emit_object(rejected_object, main_code, main_code_length, "_start", 6,
                    &invalid_emit, 1) != 0)
        fail(85);

    uint8_t main_object[OBJECT_CAPACITY], answer_object[OBJECT_CAPACITY];
    uint8_t adjust_object[OBJECT_CAPACITY];
    size_t main_object_length = emit_object(main_object, main_code,
                                            main_code_length, "_start", 6,
                                            main_relocations,
                                            main_relocation_count);
    size_t answer_object_length = emit_object(answer_object, answer_code,
                                              answer_code_length,
                                              answer_function,
                                              answer_function_length,
                                              answer_relocations,
                                              answer_relocation_count);
    size_t adjust_object_length = emit_object(adjust_object, adjust_code,
                                              adjust_code_length,
                                              adjust_function,
                                              adjust_function_length,
                                              adjust_relocations,
                                              adjust_relocation_count);
    if (main_object_length != 688 || answer_object_length != 728 ||
        adjust_object_length != 664 ||
        !write_file(main_object_path, sizeof(main_object_path) - 1,
                    main_object, main_object_length) ||
        !write_file(answer_object_path, sizeof(answer_object_path) - 1,
                    answer_object, answer_object_length) ||
        !write_file(adjust_object_path, sizeof(adjust_object_path) - 1,
                    adjust_object, adjust_object_length))
        fail(85);

    memset(main_object, 0, sizeof(main_object));
    memset(answer_object, 0, sizeof(answer_object));
    memset(adjust_object, 0, sizeof(adjust_object));
    main_object_length = read_file(main_object_path,
                                   sizeof(main_object_path) - 1,
                                   main_object, sizeof(main_object));
    answer_object_length = read_file(answer_object_path,
                                     sizeof(answer_object_path) - 1,
                                     answer_object, sizeof(answer_object));
    adjust_object_length = read_file(adjust_object_path,
                                     sizeof(adjust_object_path) - 1,
                                     adjust_object, sizeof(adjust_object));
    if (!main_object_length || !answer_object_length || !adjust_object_length)
        fail(86);

    struct object_view corrupt_view;
    if (!parse_object(main_object, main_object_length, &corrupt_view)) fail(87);
    size_t corrupt_info = corrupt_view.rela_offset + 8;
    uint8_t saved_type = main_object[corrupt_info];
    main_object[corrupt_info] = (uint8_t)(R_AARCH64_CALL26 - 1);
    uint8_t linked_code[384] = {0};
    size_t entry_offset = 0;
    const uint8_t *objects[MAX_LINK_OBJECTS] = {
        main_object, answer_object, adjust_object,
    };
    size_t object_lengths[MAX_LINK_OBJECTS] = {
        main_object_length, answer_object_length, adjust_object_length,
    };
    if (link_objects(objects, object_lengths, 3, linked_code,
                     sizeof(linked_code), "_start", &entry_offset) != 0)
        fail(88);
    main_object[corrupt_info] = saved_type;
    size_t corrupt_addend = corrupt_view.rela_offset + 16;
    if (main_object[corrupt_addend] != 0) fail(88);
    main_object[corrupt_addend] = 1;
    if (link_objects(objects, object_lengths, 3, linked_code,
                     sizeof(linked_code), "_start", &entry_offset) != 0)
        fail(88);
    main_object[corrupt_addend] = 0;
    if (link_objects(objects, object_lengths, 2, linked_code,
                     sizeof(linked_code), "_start", &entry_offset) != 0)
        fail(88);
    const uint8_t *duplicate_objects[MAX_LINK_OBJECTS] = {
        main_object, answer_object, answer_object,
    };
    size_t duplicate_lengths[MAX_LINK_OBJECTS] = {
        main_object_length, answer_object_length, answer_object_length,
    };
    if (link_objects(duplicate_objects, duplicate_lengths, 3, linked_code,
                     sizeof(linked_code), "_start", &entry_offset) != 0)
        fail(88);
    size_t linked_length = link_objects(objects, object_lengths, 3, linked_code,
                                        sizeof(linked_code), "_start",
                                        &entry_offset);
    if (linked_length != 300 || entry_offset != 0) fail(89);

    uint8_t *compiled_jit =
        (uint8_t *)(uintptr_t)syscall4(SYS_VM_MAP, 0, 0, 0, 0);
    if ((uintptr_t)compiled_jit == UINT64_MAX) fail(89);
    copy_bytes(compiled_jit, linked_code, linked_length);
    if (syscall4(SYS_VM_PROTECT, (uintptr_t)compiled_jit,
                 PROT_READ | PROT_WRITE | PROT_EXEC, 0, 0) != 0 ||
        syscall4(SYS_VM_PROTECT, (uintptr_t)compiled_jit,
                 PROT_READ | PROT_EXEC, 0, 0) != 1)
        fail(89);
    uint64_t (*compiled_answer)(uint64_t) =
        (uint64_t (*)(uint64_t))(uintptr_t)(compiled_jit + 76);
    uint64_t (*compiled_adjust)(uint64_t) =
        (uint64_t (*)(uint64_t))(uintptr_t)(compiled_jit + 192);
    if (compiled_answer(20) != 42 || compiled_answer(0) != 86 ||
        compiled_adjust(40) != 42 || compiled_adjust(0) != 2)
        fail(89);

    volatile uint8_t image[IMAGE_CAPACITY];
    size_t image_length = emit_elf(image, linked_code, linked_length,
                                   entry_offset);
    if (image_length != 815 ||
        !write_file(output_path, sizeof(output_path) - 1,
                    (const uint8_t *)image, image_length))
        fail(90);
    syscall4(SYS_WRITE, (uintptr_t)marker, sizeof(marker) - 1, 0, 0);
    syscall4(SYS_EXIT, 42, 0, 0, 0);
    __builtin_unreachable();
}
