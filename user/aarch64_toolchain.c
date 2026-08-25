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
    OBJECT_CAPACITY = 2048,
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

static void write_bytes(const void *bytes, size_t count) {
    syscall4(SYS_WRITE, (uintptr_t)bytes, count, 0, 0);
}

static void write_text(const char *text) { write_bytes(text, length(text)); }

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

enum {
    BUILD_LANGUAGE_ASM = 1,
    BUILD_LANGUAGE_C = 2,
    MIN_BUILD_INPUTS = 2,
    MAX_BUILD_INPUTS = 6,
    MAX_BUILD_PATH_BYTES = 96,
    BUILD_SOURCE_CAPACITY = 768,
    BUILD_STATE_BYTES = 120,
    BUILD_STATE_SUFFIX_BYTES = 6,
};

struct build_input {
    uint8_t language;
    const char *source_path;
    size_t source_path_length;
    const char *object_path;
    size_t object_path_length;
};

struct build_manifest {
    struct build_input inputs[MAX_BUILD_INPUTS];
    size_t input_count;
    const char *output_path;
    size_t output_path_length;
    char entry[MAX_LABEL_BYTES + 1];
    size_t entry_length;
};

static int build_path_byte(char byte) {
    return (byte >= 'a' && byte <= 'z') ||
           (byte >= 'A' && byte <= 'Z') ||
           (byte >= '0' && byte <= '9') || byte == '/' || byte == '.' ||
           byte == '_' || byte == '-';
}

static int build_field(const char *source, size_t source_length,
                       size_t *cursor, char terminator, const char **field,
                       size_t *field_length) {
    size_t start = *cursor;
    while (*cursor < source_length && source[*cursor] != terminator) {
        if (!build_path_byte(source[*cursor])) return 0;
        ++*cursor;
    }
    size_t count = *cursor - start;
    if (*cursor == source_length || count == 0 ||
        count > MAX_BUILD_PATH_BYTES || source[start] != '/')
        return 0;
    *field = source + start;
    *field_length = count;
    ++*cursor;
    return 1;
}

static int build_path_equal(const char *first, size_t first_length,
                            const char *second, size_t second_length) {
    if (first_length != second_length) return 0;
    for (size_t index = 0; index < first_length; ++index)
        if (first[index] != second[index]) return 0;
    return 1;
}

static int parse_build_manifest(const char *source, size_t source_length,
                                struct build_manifest *manifest) {
    if (!source || !manifest) return 0;
    memset(manifest, 0, sizeof(*manifest));
    size_t cursor = 0;
    if (!consume(source, source_length, &cursor, "MAKBUILD1\n")) return 0;
    while (cursor < source_length) {
        uint8_t language = 0;
        if (consume(source, source_length, &cursor, "asm "))
            language = BUILD_LANGUAGE_ASM;
        else if (consume(source, source_length, &cursor, "c "))
            language = BUILD_LANGUAGE_C;
        else if (consume(source, source_length, &cursor, "link ")) {
            if (manifest->input_count < MIN_BUILD_INPUTS ||
                manifest->inputs[0].language != BUILD_LANGUAGE_ASM)
                return 0;
            for (size_t input = 1; input < manifest->input_count; ++input)
                if (manifest->inputs[input].language != BUILD_LANGUAGE_C)
                    return 0;
            if (!build_field(source, source_length, &cursor, ' ',
                             &manifest->output_path,
                             &manifest->output_path_length))
                return 0;
            size_t entry_start = cursor;
            while (cursor < source_length && source[cursor] != '\n') {
                if (!name_byte(source[cursor], cursor == entry_start)) return 0;
                ++cursor;
            }
            manifest->entry_length = cursor - entry_start;
            if (cursor == source_length || manifest->entry_length == 0 ||
                manifest->entry_length > MAX_LABEL_BYTES)
                return 0;
            for (size_t index = 0; index < manifest->entry_length; ++index)
                manifest->entry[index] = source[entry_start + index];
            ++cursor;
            if (cursor != source_length) return 0;
            for (size_t input = 0; input < manifest->input_count; ++input)
                if (build_path_equal(manifest->output_path,
                                     manifest->output_path_length,
                                     manifest->inputs[input].source_path,
                                     manifest->inputs[input].source_path_length) ||
                    build_path_equal(manifest->output_path,
                                     manifest->output_path_length,
                                     manifest->inputs[input].object_path,
                                     manifest->inputs[input].object_path_length))
                    return 0;
            return 1;
        } else {
            return 0;
        }

        if (manifest->input_count == MAX_BUILD_INPUTS) return 0;
        struct build_input *input = &manifest->inputs[manifest->input_count];
        input->language = language;
        if (!build_field(source, source_length, &cursor, ' ',
                         &input->source_path, &input->source_path_length) ||
            !build_field(source, source_length, &cursor, '\n',
                         &input->object_path, &input->object_path_length) ||
            build_path_equal(input->source_path, input->source_path_length,
                             input->object_path, input->object_path_length))
            return 0;
        for (size_t previous = 0; previous < manifest->input_count; ++previous) {
            const struct build_input *other = &manifest->inputs[previous];
            if (build_path_equal(input->source_path, input->source_path_length,
                                 other->source_path,
                                 other->source_path_length) ||
                build_path_equal(input->source_path, input->source_path_length,
                                 other->object_path,
                                 other->object_path_length) ||
                build_path_equal(input->object_path, input->object_path_length,
                                 other->source_path,
                                 other->source_path_length) ||
                build_path_equal(input->object_path, input->object_path_length,
                                 other->object_path,
                                 other->object_path_length))
                return 0;
        }
        ++manifest->input_count;
    }
    return 0;
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
 *   int identifier(int [*]identifier) {
 *       int identifier = expression;
 *       int identifier[constant] = { expression, ... };
 *       int *identifier = &identifier;
 *       *identifier = expression;
 *       if (expression == expression) { return expression; }
 *       while (expression != expression) { identifier = expression; }
 *       return expression;
 *   }
 *
 * Expressions contain the integer parameter, unsigned 16-bit constants,
 * parentheses, left-associative *, +, and -, address-of and dereference of
 * bounded local pointers or the pointer parameter, checked fixed-array
 * indexing, constant- or scalar-variable-element pointer addition in pointer
 * initializers/calls and parenthesized dereferences, typed pointer subtraction
 * in int elements, signed relational conditions, and a bounded one- or
 * two-argument external call with
 * array-to-pointer decay.  Functions take one or two typed integer or pointer
 * parameters in AAPCS64 x0/x1.  The output follows AAPCS64,
 * including a non-leaf save frame, and is wrapped in a genuine ELF64 ET_REL
 * object by emit_object(). Unsupported syntax fails closed.
 */
enum {
    MAX_C_LOCALS = 4,
    MAX_C_STACK_SLOTS = 4,
    MAX_C_FUNCTIONS = 3,
    MAX_C_PARAMETERS = 2,
};

struct c_definition {
    char name[MAX_LABEL_BYTES];
    size_t length;
    size_t offset;
    size_t size;
};

struct c_local {
    char name[MAX_LABEL_BYTES];
    size_t length;
    int pointer;
    int address_taken;
    size_t stack_index;
    uint32_t array_length;
    uint32_t pointer_bound;
};

struct c_compiler {
    const char *source;
    size_t source_length;
    size_t cursor;
    volatile uint8_t *code;
    size_t capacity;
    size_t output;
    char parameters[MAX_C_PARAMETERS][MAX_LABEL_BYTES];
    size_t parameter_lengths[MAX_C_PARAMETERS];
    int parameter_pointers[MAX_C_PARAMETERS];
    size_t parameter_count;
    struct c_local locals[MAX_C_LOCALS];
    size_t local_count;
    size_t stack_slot_count;
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

static struct c_local *c_find_local(struct c_compiler *compiler,
                                    const char *name, size_t name_length,
                                    size_t *index_out) {
    size_t local = 0;
    while (local < compiler->local_count &&
           !same_name(name, name_length, compiler->locals[local].name,
                      compiler->locals[local].length))
        ++local;
    if (local == compiler->local_count) return 0;
    if (index_out) *index_out = local;
    return &compiler->locals[local];
}

static int c_find_parameter(struct c_compiler *compiler, const char *name,
                            size_t name_length, size_t *index_out) {
    for (size_t index = 0; index < compiler->parameter_count; ++index) {
        if (!same_name(name, name_length, compiler->parameters[index],
                       compiler->parameter_lengths[index]))
            continue;
        if (index_out) *index_out = index;
        return 1;
    }
    return 0;
}

static int c_variable_register(struct c_compiler *compiler, const char *name,
                               size_t name_length, uint32_t *register_out,
                               struct c_local **local_out,
                               size_t *local_index_out) {
    size_t parameter_index = 0;
    if (c_find_parameter(compiler, name, name_length, &parameter_index)) {
        if (compiler->parameter_pointers[parameter_index]) return 0;
        *register_out = 23 + (uint32_t)parameter_index;
        if (local_out) *local_out = 0;
        return 1;
    }
    size_t local_index = 0;
    struct c_local *local = c_find_local(compiler, name, name_length,
                                         &local_index);
    if (!local) return 0;
    *register_out = 19 + (uint32_t)local_index;
    if (local_out) *local_out = local;
    if (local_index_out) *local_index_out = local_index;
    return 1;
}

static int c_pointer_info(struct c_compiler *compiler, const char *name,
                          size_t name_length, uint32_t *register_out,
                          uint32_t *bound_out) {
    size_t parameter_index = 0;
    if (c_find_parameter(compiler, name, name_length, &parameter_index) &&
        compiler->parameter_pointers[parameter_index]) {
        *register_out = 23 + (uint32_t)parameter_index;
        if (bound_out) *bound_out = 0;
        return 1;
    }
    size_t local_index = 0;
    struct c_local *local = c_find_local(compiler, name, name_length,
                                         &local_index);
    if (!local || !local->pointer) return 0;
    *register_out = 19 + (uint32_t)local_index;
    if (bound_out) *bound_out = local->pointer_bound;
    return 1;
}

static int c_pointer_register(struct c_compiler *compiler, const char *name,
                              size_t name_length, uint32_t *register_out) {
    return c_pointer_info(compiler, name, name_length, register_out, 0);
}

static int c_pointer_index_register(struct c_compiler *compiler,
                                    const char *name, size_t name_length,
                                    uint32_t index,
                                    uint32_t *register_out) {
    uint32_t bound = 0;
    if (index >= MAX_C_STACK_SLOTS ||
        !c_pointer_info(compiler, name, name_length, register_out, &bound))
        return 0;
    return bound == 0 || index < bound;
}

static int c_emit(struct c_compiler *compiler, uint32_t instruction);

static uint32_t c_local_stack_offset(size_t stack_index) {
    return 64 + (uint32_t)stack_index * 4;
}

static int c_store_stack_local(struct c_compiler *compiler,
                               size_t stack_index, uint32_t source) {
    uint32_t offset = c_local_stack_offset(stack_index);
    return c_emit(compiler, UINT32_C(0xb9000000) |
                            ((offset / 4) << 10) |
                            (UINT32_C(31) << 5) | source);
}

static int c_load_stack_local(struct c_compiler *compiler,
                              size_t stack_index, uint32_t destination) {
    uint32_t offset = c_local_stack_offset(stack_index);
    return c_emit(compiler, UINT32_C(0xb9400000) |
                            ((offset / 4) << 10) |
                            (UINT32_C(31) << 5) | destination);
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

/*
 * Parse the intentionally bounded pointer-expression grammar
 * `pointer-or-array`, `pointer-or-array + constant`, or
 * `pointer-or-array + scalar`.  Offsets are in int elements, not bytes.  A
 * register value of 31 denotes an immediate offset.  A zero bound means the
 * pointer parameter has no compile-time extent; known local arrays and derived
 * pointers fail closed on one-past-end constants and on variable offsets whose
 * safety cannot be proven.
 */
static int c_pointer_expression(struct c_compiler *compiler,
                                uint32_t *base_register,
                                uint32_t *element_offset,
                                uint32_t *offset_register,
                                uint32_t *remaining_bound) {
    char identifier[MAX_LABEL_BYTES] = {0};
    size_t identifier_length = 0;
    uint32_t bound = 0;
    if (!c_identifier(compiler, identifier, &identifier_length) ||
        !c_pointer_info(compiler, identifier, identifier_length,
                        base_register, &bound))
        return 0;
    *element_offset = 0;
    *offset_register = 31;
    c_space(compiler);
    if (compiler->cursor < compiler->source_length &&
        compiler->source[compiler->cursor] == '+') {
        ++compiler->cursor;
        size_t saved = compiler->cursor;
        if (c_number(compiler, element_offset)) {
            if (*element_offset >= MAX_C_STACK_SLOTS) return 0;
        } else {
            compiler->cursor = saved;
            char offset_name[MAX_LABEL_BYTES] = {0};
            size_t offset_name_length = 0;
            struct c_local *offset_local = 0;
            if (!c_identifier(compiler, offset_name, &offset_name_length) ||
                !c_variable_register(compiler, offset_name,
                                     offset_name_length, offset_register,
                                     &offset_local, 0) ||
                (offset_local &&
                 (offset_local->pointer || offset_local->address_taken)) ||
                bound != 0)
                return 0;
        }
    }
    if (*offset_register == 31 && bound != 0 && *element_offset >= bound)
        return 0;
    if (remaining_bound)
        *remaining_bound = *offset_register != 31 || bound == 0
                               ? 0
                               : bound - *element_offset;
    return 1;
}

static int c_emit_pointer_value(struct c_compiler *compiler,
                                uint32_t destination, uint32_t base,
                                uint32_t element_offset,
                                uint32_t offset_register) {
    if (destination > 31 || base > 31 ||
        element_offset >= MAX_C_STACK_SLOTS || offset_register > 31)
        return 0;
    if (offset_register != 31)
        return element_offset == 0 &&
               c_emit(compiler, UINT32_C(0x8b20c800) |
                                 (offset_register << 16) | (base << 5) |
                                 destination);
    if (element_offset == 0)
        return c_emit(compiler, UINT32_C(0xaa0003e0) |
                                (base << 16) | destination);
    uint32_t byte_offset = element_offset * 4;
    return c_emit(compiler, UINT32_C(0x91000000) |
                            (byte_offset << 10) | (base << 5) |
                            destination);
}

static int c_additive(struct c_compiler *compiler, uint32_t destination);

static int c_call_argument(struct c_compiler *compiler,
                           uint32_t destination) {
    if (destination >= MAX_C_PARAMETERS) return 0;
    size_t saved = compiler->cursor;
    if (c_punct(compiler, '&')) {
        char target_name[MAX_LABEL_BYTES] = {0};
        size_t target_name_length = 0;
        if (c_identifier(compiler, target_name, &target_name_length)) {
            struct c_local *target = c_find_local(
                compiler, target_name, target_name_length, 0);
            c_space(compiler);
            if (target && !target->pointer &&
                compiler->cursor < compiler->source_length &&
                (compiler->source[compiler->cursor] == ')' ||
                 compiler->source[compiler->cursor] == ',')) {
                target->address_taken = 1;
                uint32_t offset = c_local_stack_offset(target->stack_index);
                return c_emit(compiler, UINT32_C(0x910003e0) |
                                        (offset << 10) | destination);
            }
        }
    }
    compiler->cursor = saved;
    uint32_t pointer_register = 0, element_offset = 0, offset_register = 31;
    if (c_pointer_expression(compiler, &pointer_register, &element_offset,
                             &offset_register, 0)) {
        c_space(compiler);
        if (compiler->cursor < compiler->source_length &&
            (compiler->source[compiler->cursor] == ')' ||
             compiler->source[compiler->cursor] == ',') &&
            c_emit_pointer_value(compiler, destination, pointer_register,
                                 element_offset, offset_register))
            return 1;
    }
    compiler->cursor = saved;
    return c_additive(compiler, destination);
}

static int c_primary(struct c_compiler *compiler, uint32_t destination) {
    if (destination > 7) return 0;
    c_space(compiler);
    if (compiler->cursor < compiler->source_length &&
        compiler->source[compiler->cursor] == '*') {
        ++compiler->cursor;
        uint32_t pointer_register = 0, element_offset = 0;
        uint32_t offset_register = 31;
        if (c_punct(compiler, '(')) {
            if (!c_pointer_expression(compiler, &pointer_register,
                                      &element_offset, &offset_register, 0) ||
                !c_punct(compiler, ')'))
                return 0;
        } else {
            char pointer_name[MAX_LABEL_BYTES] = {0};
            size_t pointer_name_length = 0;
            if (!c_identifier(compiler, pointer_name,
                              &pointer_name_length) ||
                !c_pointer_register(compiler, pointer_name,
                                    pointer_name_length,
                                    &pointer_register))
                return 0;
        }
        if (offset_register != 31) {
            if (!c_emit_pointer_value(compiler, 8, pointer_register,
                                      element_offset, offset_register))
                return 0;
            pointer_register = 8;
            element_offset = 0;
        }
        return c_emit(compiler, UINT32_C(0xb9400000) |
                                (element_offset << 10) |
                                (pointer_register << 5) | destination);
    }
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
        compiler->source[compiler->cursor] == '[') {
        ++compiler->cursor;
        uint32_t index = 0, pointer_register = 0;
        if (!c_number(compiler, &index) || !c_punct(compiler, ']') ||
            !c_pointer_index_register(compiler, identifier,
                                      identifier_length, index,
                                      &pointer_register))
            return 0;
        return c_emit(compiler, UINT32_C(0xb9400000) | (index << 10) |
                                (pointer_register << 5) | destination);
    }
    if (compiler->cursor < compiler->source_length &&
        compiler->source[compiler->cursor] == '(') {
        if (destination != 0 || !compiler->relocations ||
            !compiler->relocation_count ||
            *compiler->relocation_count == MAX_RELOCATIONS)
            return 0;
        ++compiler->cursor;
        if (!c_call_argument(compiler, 0)) return 0;
        if (c_punct(compiler, ',')) {
            if (!c_call_argument(compiler, 1)) return 0;
        }
        if (!c_punct(compiler, ')')) return 0;
        struct relocation *relocation =
            &compiler->relocations[*compiler->relocation_count];
        for (size_t index = 0; index < identifier_length; ++index)
            relocation->name[index] = identifier[index];
        relocation->length = identifier_length;
        relocation->offset = compiler->output;
        ++*compiler->relocation_count;
        return c_emit(compiler, UINT32_C(0x94000000));
    }
    uint32_t left_pointer = 0, left_bound = 0;
    if (c_pointer_info(compiler, identifier, identifier_length,
                       &left_pointer, &left_bound)) {
        if (compiler->cursor == compiler->source_length ||
            compiler->source[compiler->cursor] != '-')
            return 0;
        ++compiler->cursor;
        char right_name[MAX_LABEL_BYTES] = {0};
        size_t right_name_length = 0;
        uint32_t right_pointer = 0, right_bound = 0;
        if (!c_identifier(compiler, right_name, &right_name_length) ||
            !c_pointer_info(compiler, right_name, right_name_length,
                            &right_pointer, &right_bound) ||
            !c_emit(compiler, UINT32_C(0xcb000000) |
                              (right_pointer << 16) |
                              (left_pointer << 5) | destination) ||
            !c_emit(compiler, UINT32_C(0x9342fc00) |
                              (destination << 5) | destination))
            return 0;
        return 1;
    }
    uint32_t source_register = 0;
    struct c_local *local = 0;
    if (!c_variable_register(compiler, identifier, identifier_length,
                             &source_register, &local, 0) ||
        (local && local->pointer))
        return 0;
    if (local && local->address_taken)
        return c_load_stack_local(compiler, local->stack_index, destination);
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
    if (compiler->cursor == compiler->source_length) return 0;
    char first = compiler->source[compiler->cursor];
    char second = compiler->cursor + 1 < compiler->source_length
                      ? compiler->source[compiler->cursor + 1]
                      : 0;
    if (first == '=' || first == '!') {
        if (second != '=') return 0;
        *false_condition = first == '=' ? 1 : 0; /* B.NE or B.EQ. */
        compiler->cursor += 2;
    } else if (first == '<') {
        *false_condition = second == '=' ? 12 : 10; /* B.GT or B.GE. */
        compiler->cursor += second == '=' ? 2 : 1;
    } else if (first == '>') {
        *false_condition = second == '=' ? 11 : 13; /* B.LT or B.LE. */
        compiler->cursor += second == '=' ? 2 : 1;
    } else {
        return 0;
    }
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
    return c_emit(compiler, UINT32_C(0xa94363f7)) &&
           c_emit(compiler, UINT32_C(0xa9425bf5)) &&
           c_emit(compiler, UINT32_C(0xa94153f3)) &&
           c_emit(compiler, UINT32_C(0xa8c67bfd)) &&
           c_emit(compiler, UINT32_C(0xd65f03c0));
}

static int c_return_statement(struct c_compiler *compiler) {
    return c_additive(compiler, 0) && c_punct(compiler, ';') &&
           c_epilogue(compiler);
}

static int c_declaration(struct c_compiler *compiler) {
    if (compiler->local_count == MAX_C_LOCALS) return 0;
    struct c_local local = {0};
    c_space(compiler);
    if (compiler->cursor < compiler->source_length &&
        compiler->source[compiler->cursor] == '*') {
        local.pointer = 1;
        ++compiler->cursor;
    }
    if (!c_identifier(compiler, local.name, &local.length) ||
        c_find_parameter(compiler, local.name, local.length, 0))
        return 0;
    c_space(compiler);
    if (compiler->cursor < compiler->source_length &&
        compiler->source[compiler->cursor] == '[') {
        if (local.pointer) return 0;
        ++compiler->cursor;
        if (!c_number(compiler, &local.array_length) ||
            local.array_length == 0 ||
            local.array_length > MAX_C_STACK_SLOTS ||
            !c_punct(compiler, ']'))
            return 0;
        local.pointer = 1;
        local.pointer_bound = local.array_length;
    }
    for (size_t index = 0; index < compiler->local_count; ++index)
        if (same_name(local.name, local.length, compiler->locals[index].name,
                      compiler->locals[index].length))
            return 0;
    uint32_t register_index = 19 + (uint32_t)compiler->local_count;
    if (!c_punct(compiler, '=')) return 0;
    if (local.array_length != 0) {
        if (compiler->stack_slot_count + local.array_length >
                MAX_C_STACK_SLOTS ||
            !c_punct(compiler, '{'))
            return 0;
        local.stack_index = compiler->stack_slot_count;
        uint32_t offset = c_local_stack_offset(local.stack_index);
        if (!c_emit(compiler, UINT32_C(0x910003e0) | (offset << 10) |
                              register_index))
            return 0;
        for (uint32_t index = 0; index < local.array_length; ++index) {
            if (!c_additive(compiler, 0) ||
                !c_store_stack_local(compiler,
                                     local.stack_index + index, 0))
                return 0;
            if (index + 1 < local.array_length) {
                if (!c_punct(compiler, ',')) return 0;
            }
        }
        if (!c_punct(compiler, '}') || !c_punct(compiler, ';')) return 0;
        compiler->stack_slot_count += local.array_length;
    } else if (local.pointer) {
        size_t saved = compiler->cursor;
        if (c_punct(compiler, '&')) {
            char target_name[MAX_LABEL_BYTES] = {0};
            size_t target_name_length = 0;
            if (!c_identifier(compiler, target_name, &target_name_length))
                return 0;
            struct c_local *target = c_find_local(
                compiler, target_name, target_name_length, 0);
            if (!target || target->pointer) return 0;
            target->address_taken = 1;
            local.pointer_bound = 1;
            uint32_t offset = c_local_stack_offset(target->stack_index);
            if (!c_emit(compiler, UINT32_C(0x910003e0) |
                                  (offset << 10) | register_index))
                return 0;
        } else {
            compiler->cursor = saved;
            uint32_t pointer_register = 0, element_offset = 0;
            uint32_t offset_register = 31;
            if (!c_pointer_expression(compiler, &pointer_register,
                                      &element_offset,
                                      &offset_register,
                                      &local.pointer_bound) ||
                !c_emit_pointer_value(compiler, register_index,
                                      pointer_register, element_offset,
                                      offset_register))
                return 0;
        }
        if (!c_punct(compiler, ';')) return 0;
    } else {
        if (compiler->stack_slot_count == MAX_C_STACK_SLOTS) return 0;
        local.stack_index = compiler->stack_slot_count;
        if (!c_additive(compiler, 0) || !c_punct(compiler, ';') ||
            !c_emit(compiler, UINT32_C(0x2a0003e0) | register_index) ||
            !c_store_stack_local(compiler, local.stack_index, 0))
            return 0;
        ++compiler->stack_slot_count;
    }
    struct c_local *stored = &compiler->locals[compiler->local_count++];
    for (size_t index = 0; index < local.length; ++index)
        stored->name[index] = local.name[index];
    stored->length = local.length;
    stored->pointer = local.pointer;
    stored->address_taken = local.address_taken;
    stored->stack_index = local.stack_index;
    stored->array_length = local.array_length;
    stored->pointer_bound = local.pointer_bound;
    return 1;
}

static int c_assignment(struct c_compiler *compiler) {
    c_space(compiler);
    if (compiler->cursor < compiler->source_length &&
        compiler->source[compiler->cursor] == '*') {
        ++compiler->cursor;
        uint32_t pointer_register = 0, element_offset = 0;
        uint32_t offset_register = 31;
        if (c_punct(compiler, '(')) {
            if (!c_pointer_expression(compiler, &pointer_register,
                                      &element_offset, &offset_register, 0) ||
                !c_punct(compiler, ')'))
                return 0;
        } else {
            char pointer_name[MAX_LABEL_BYTES] = {0};
            size_t pointer_name_length = 0;
            if (!c_identifier(compiler, pointer_name,
                              &pointer_name_length) ||
                !c_pointer_register(compiler, pointer_name,
                                    pointer_name_length,
                                    &pointer_register))
                return 0;
        }
        if (!c_punct(compiler, '=') ||
            !c_additive(compiler, 0) || !c_punct(compiler, ';'))
            return 0;
        if (offset_register != 31) {
            if (!c_emit_pointer_value(compiler, 8, pointer_register,
                                      element_offset, offset_register))
                return 0;
            pointer_register = 8;
            element_offset = 0;
        }
        return c_emit(compiler, UINT32_C(0xb9000000) |
                                (element_offset << 10) |
                                (pointer_register << 5));
    }
    char identifier[MAX_LABEL_BYTES] = {0};
    size_t identifier_length = 0;
    uint32_t destination = 0;
    struct c_local *local = 0;
    if (!c_identifier(compiler, identifier, &identifier_length)) return 0;
    c_space(compiler);
    if (compiler->cursor < compiler->source_length &&
        compiler->source[compiler->cursor] == '[') {
        ++compiler->cursor;
        uint32_t index = 0, pointer_register = 0;
        if (!c_number(compiler, &index) || !c_punct(compiler, ']') ||
            !c_pointer_index_register(compiler, identifier,
                                      identifier_length, index,
                                      &pointer_register) ||
            !c_punct(compiler, '=') || !c_additive(compiler, 0) ||
            !c_punct(compiler, ';'))
            return 0;
        return c_emit(compiler, UINT32_C(0xb9000000) | (index << 10) |
                                (pointer_register << 5));
    }
    if (!c_variable_register(compiler, identifier, identifier_length,
                             &destination, &local, 0) ||
        (local && local->pointer) ||
        !c_punct(compiler, '=') || !c_additive(compiler, 0) ||
        !c_punct(compiler, ';'))
        return 0;
    if (!c_emit(compiler, UINT32_C(0x2a0003e0) | destination)) return 0;
    return !local || c_store_stack_local(compiler, local->stack_index, 0);
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

static int c_compile_function(struct c_compiler *compiler,
                              struct c_definition *definition) {
    compiler->parameter_count = 0;
    compiler->local_count = 0;
    compiler->stack_slot_count = 0;
    definition->length = 0;
    definition->offset = compiler->output;
    definition->size = 0;
    if (!c_keyword(compiler, "int") ||
        !c_identifier(compiler, definition->name, &definition->length) ||
        !c_punct(compiler, '('))
        return 0;
    for (;;) {
        if (compiler->parameter_count == MAX_C_PARAMETERS ||
            !c_keyword(compiler, "int"))
            return 0;
        size_t parameter = compiler->parameter_count;
        c_space(compiler);
        if (compiler->cursor < compiler->source_length &&
            compiler->source[compiler->cursor] == '*') {
            compiler->parameter_pointers[parameter] = 1;
            ++compiler->cursor;
        } else {
            compiler->parameter_pointers[parameter] = 0;
        }
        if (!c_identifier(compiler, compiler->parameters[parameter],
                          &compiler->parameter_lengths[parameter]))
            return 0;
        for (size_t previous = 0; previous < parameter; ++previous)
            if (same_name(compiler->parameters[previous],
                          compiler->parameter_lengths[previous],
                          compiler->parameters[parameter],
                          compiler->parameter_lengths[parameter]))
                return 0;
        ++compiler->parameter_count;
        if (!c_punct(compiler, ',')) break;
    }
    if (!c_punct(compiler, ')') || !c_punct(compiler, '{')) return 0;
    /* Preserve FP/LR and x19-x24; reserve four 32-bit local stack slots. */
    if (!c_emit(compiler, UINT32_C(0xa9ba7bfd)) ||
        !c_emit(compiler, UINT32_C(0x910003fd)) ||
        !c_emit(compiler, UINT32_C(0xa90153f3)) ||
        !c_emit(compiler, UINT32_C(0xa9025bf5)) ||
        !c_emit(compiler, UINT32_C(0xa90363f7)))
        return 0;
    for (size_t parameter = 0; parameter < compiler->parameter_count;
         ++parameter) {
        uint32_t source = (uint32_t)parameter;
        uint32_t destination = 23 + (uint32_t)parameter;
        uint32_t opcode = compiler->parameter_pointers[parameter]
                              ? UINT32_C(0xaa0003e0)
                              : UINT32_C(0x2a0003e0);
        if (!c_emit(compiler, opcode | (source << 16) | destination))
            return 0;
    }
    int terminal_return = 0;
    size_t statement_count = 0;
    for (;;) {
        c_space(compiler);
        if (compiler->cursor == compiler->source_length) return 0;
        if (compiler->source[compiler->cursor] == '}') break;
        if (terminal_return) return 0;
        if (c_keyword(compiler, "int")) {
            if (!c_declaration(compiler)) return 0;
        } else if (c_keyword(compiler, "if")) {
            if (!c_if_return(compiler)) return 0;
        } else if (c_keyword(compiler, "while")) {
            if (!c_while(compiler)) return 0;
        } else if (c_keyword(compiler, "return")) {
            if (!c_return_statement(compiler)) return 0;
            terminal_return = 1;
        } else {
            if (!c_assignment(compiler)) return 0;
        }
        ++statement_count;
    }
    if (statement_count == 0 || !terminal_return || !c_punct(compiler, '}'))
        return 0;
    definition->size = compiler->output - definition->offset;
    return definition->size != 0;
}

static size_t compile_c_unit(
    const char *source, size_t source_length, volatile uint8_t *code,
    size_t capacity, struct c_definition definitions[MAX_C_FUNCTIONS],
    size_t *definition_count,
    struct relocation relocations[MAX_RELOCATIONS], size_t *relocation_count) {
    if (!definition_count || !relocation_count || !relocations) return 0;
    *definition_count = 0;
    *relocation_count = 0;
    struct c_compiler compiler = {
        .source = source,
        .source_length = source_length,
        .code = code,
        .capacity = capacity,
        .relocations = relocations,
        .relocation_count = relocation_count,
    };
    for (;;) {
        c_space(&compiler);
        if (compiler.cursor == compiler.source_length) break;
        if (*definition_count == MAX_C_FUNCTIONS ||
            !c_compile_function(&compiler, &definitions[*definition_count]))
            return 0;
        for (size_t previous = 0; previous < *definition_count; ++previous)
            if (same_name(definitions[previous].name,
                          definitions[previous].length,
                          definitions[*definition_count].name,
                          definitions[*definition_count].length))
                return 0;
        ++*definition_count;
    }
    if (*definition_count == 0) return 0;
    return compiler.output;
}

static size_t compile_c(const char *source, size_t source_length,
                        volatile uint8_t *code, size_t capacity,
                        char function[MAX_LABEL_BYTES],
                        size_t *function_length,
                        struct relocation relocations[MAX_RELOCATIONS],
                        size_t *relocation_count) {
    if (!function_length) return 0;
    struct c_definition definitions[MAX_C_FUNCTIONS] = {0};
    size_t definition_count = 0;
    struct relocation local_relocations[MAX_RELOCATIONS] = {0};
    size_t local_relocation_count = 0;
    struct relocation *relocation_output = relocations
                                               ? relocations
                                               : local_relocations;
    size_t *relocation_count_output = relocation_count
                                          ? relocation_count
                                          : &local_relocation_count;
    size_t output = compile_c_unit(source, source_length, code, capacity,
                                   definitions, &definition_count,
                                   relocation_output,
                                   relocation_count_output);
    if (!output || definition_count != 1) return 0;
    for (size_t index = 0; index < definitions[0].length; ++index)
        function[index] = definitions[0].name[index];
    *function_length = definitions[0].length;
    return output;
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

static size_t emit_object_definitions(
    volatile uint8_t object[OBJECT_CAPACITY], const uint8_t *code,
    size_t code_length,
    const struct c_definition definitions[MAX_C_FUNCTIONS],
    size_t definition_count, const struct relocation *relocations,
    size_t relocation_count) {
    static const uint8_t section_strings[] =
        "\0.text\0.rela.text\0.symtab\0.strtab\0.shstrtab\0";
    if (code_length == 0 || definition_count == 0 ||
        definition_count > MAX_C_FUNCTIONS ||
        relocation_count > MAX_RELOCATIONS ||
        (relocation_count != 0 && !relocations))
        return 0;

    uint32_t definition_name_offsets[MAX_C_FUNCTIONS] = {0};
    size_t string_length = 1;
    for (size_t definition = 0; definition < definition_count; ++definition) {
        if (definitions[definition].length == 0 ||
            definitions[definition].length > MAX_LABEL_BYTES ||
            definitions[definition].offset % 4 != 0 ||
            definitions[definition].size == 0 ||
            definitions[definition].offset > code_length ||
            definitions[definition].size >
                code_length - definitions[definition].offset)
            return 0;
        for (size_t byte = 0; byte < definitions[definition].length; ++byte)
            if (!name_byte(definitions[definition].name[byte], byte == 0))
                return 0;
        for (size_t previous = 0; previous < definition; ++previous) {
            if (same_name(definitions[previous].name,
                          definitions[previous].length,
                          definitions[definition].name,
                          definitions[definition].length))
                return 0;
            size_t previous_end = definitions[previous].offset +
                                  definitions[previous].size;
            size_t definition_end = definitions[definition].offset +
                                    definitions[definition].size;
            if (definitions[previous].offset < definition_end &&
                definitions[definition].offset < previous_end)
                return 0;
        }
        definition_name_offsets[definition] = (uint32_t)string_length;
        string_length += definitions[definition].length + 1;
    }

    struct relocation undefined[MAX_RELOCATIONS] = {0};
    size_t undefined_count = 0;
    uint32_t relocation_symbols[MAX_RELOCATIONS] = {0};
    uint32_t undefined_name_offsets[MAX_RELOCATIONS] = {0};
    for (size_t index = 0; index < relocation_count; ++index) {
        if (relocations[index].length == 0 ||
            relocations[index].length > MAX_LABEL_BYTES ||
            relocations[index].offset % 4 != 0 ||
            relocations[index].offset > code_length ||
            code_length - relocations[index].offset < 4 ||
            get32(code, relocations[index].offset) != UINT32_C(0x94000000))
            return 0;
        for (size_t byte = 0; byte < relocations[index].length; ++byte)
            if (!name_byte(relocations[index].name[byte], byte == 0)) return 0;
        size_t definition_match = 0;
        while (definition_match < definition_count &&
               !same_name(relocations[index].name,
                          relocations[index].length,
                          definitions[definition_match].name,
                          definitions[definition_match].length))
            ++definition_match;
        if (definition_match < definition_count) {
            relocation_symbols[index] = 1 + (uint32_t)definition_match;
            continue;
        }
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
        relocation_symbols[index] = 1 + (uint32_t)definition_count +
                                    (uint32_t)match;
    }

    size_t text_offset = ELF_HEADER_SIZE;
    size_t rela_offset = align_up(text_offset + code_length, 8);
    size_t rela_length = relocation_count * RELA_SIZE;
    size_t symbol_offset = align_up(rela_offset + rela_length, 8);
    size_t symbol_count = 1 + definition_count + undefined_count;
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
    for (size_t definition = 0; definition < definition_count; ++definition)
        symbol(object, symbol_offset, 1 + definition,
               definition_name_offsets[definition], 1,
               definitions[definition].offset,
               definitions[definition].size);
    for (size_t index = 0; index < undefined_count; ++index)
        symbol(object, symbol_offset, 1 + definition_count + index,
               undefined_name_offsets[index], 0, 0, 0);
    size_t string_cursor = string_offset + 1;
    for (size_t definition = 0; definition < definition_count; ++definition) {
        copy_bytes(object + string_cursor,
                   (const uint8_t *)definitions[definition].name,
                   definitions[definition].length);
        string_cursor += definitions[definition].length + 1;
    }
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

static size_t emit_object(volatile uint8_t object[OBJECT_CAPACITY],
                          const uint8_t *code, size_t code_length,
                          const char *definition, size_t definition_length,
                          const struct relocation *relocations,
                          size_t relocation_count) {
    if (!definition || definition_length == 0 ||
        definition_length > MAX_LABEL_BYTES)
        return 0;
    struct c_definition single = {
        .length = definition_length,
        .offset = 0,
        .size = code_length,
    };
    for (size_t index = 0; index < definition_length; ++index)
        single.name[index] = definition[index];
    return emit_object_definitions(object, code, code_length, &single, 1,
                                   relocations, relocation_count);
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
        object->symbol_count > 1 + MAX_C_FUNCTIONS + MAX_RELOCATIONS)
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

enum { MAX_LINK_OBJECTS = MAX_BUILD_INPUTS };

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
            uint16_t relocation_section =
                get16(objects[object].bytes, symbol_entry + 6);
            if (relocation_section != 0 && relocation_section != 1) return 0;

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

static uint64_t build_hash(const uint8_t *bytes, size_t count) {
    uint64_t hash = UINT64_C(14695981039346656037);
    for (size_t index = 0; index < count; ++index) {
        hash ^= bytes[index];
        hash *= UINT64_C(1099511628211);
    }
    return hash;
}

static int build_state_path(const char *manifest_path,
                            size_t manifest_path_length,
                            char output[MAX_BUILD_PATH_BYTES +
                                        BUILD_STATE_SUFFIX_BYTES],
                            size_t *output_length) {
    static const char suffix[] = ".state";
    if (!output_length || manifest_path_length == 0 ||
        manifest_path_length > MAX_BUILD_PATH_BYTES - BUILD_STATE_SUFFIX_BYTES)
        return 0;
    for (size_t index = 0; index < manifest_path_length; ++index)
        output[index] = manifest_path[index];
    for (size_t index = 0; index < BUILD_STATE_SUFFIX_BYTES; ++index)
        output[manifest_path_length + index] = suffix[index];
    *output_length = manifest_path_length + BUILD_STATE_SUFFIX_BYTES;
    return 1;
}

static int build_state_path_safe(const struct build_manifest *build,
                                 const char *path, size_t path_length) {
    if (build_path_equal(path, path_length, build->output_path,
                         build->output_path_length))
        return 0;
    for (size_t input = 0; input < build->input_count; ++input)
        if (build_path_equal(path, path_length,
                             build->inputs[input].source_path,
                             build->inputs[input].source_path_length) ||
            build_path_equal(path, path_length,
                             build->inputs[input].object_path,
                             build->inputs[input].object_path_length))
            return 0;
    return 1;
}

static int read_build_state(
    const char *path, size_t path_length, uint64_t manifest_hash,
    size_t input_count,
    uint64_t source_hashes[MAX_BUILD_INPUTS],
    uint64_t object_hashes[MAX_BUILD_INPUTS]) {
    static const char magic[] = "MAKSTATE2";
    uint8_t state[BUILD_STATE_BYTES] = {0};
    if (input_count < MIN_BUILD_INPUTS || input_count > MAX_BUILD_INPUTS)
        return 0;
    if (read_file(path, path_length, state, sizeof(state)) != sizeof(state))
        return 0;
    for (size_t index = 0; index < sizeof(magic) - 1; ++index)
        if (state[index] != (uint8_t)magic[index]) return 0;
    if (state[9] != input_count) return 0;
    for (size_t index = 10; index < 16; ++index)
        if (state[index] != 0) return 0;
    if (get64(state, 16) != manifest_hash) return 0;
    for (size_t input = 0; input < input_count; ++input) {
        source_hashes[input] = get64(state, 24 + input * 8);
        object_hashes[input] =
            get64(state, 24 + MAX_BUILD_INPUTS * 8 + input * 8);
    }
    for (size_t input = input_count; input < MAX_BUILD_INPUTS; ++input)
        if (get64(state, 24 + input * 8) != 0 ||
            get64(state, 24 + MAX_BUILD_INPUTS * 8 + input * 8) != 0)
            return 0;
    return 1;
}

static int write_build_state(
    const char *path, size_t path_length, uint64_t manifest_hash,
    size_t input_count,
    const uint8_t *const sources[MAX_BUILD_INPUTS],
    const size_t source_lengths[MAX_BUILD_INPUTS],
    const uint8_t *const objects[MAX_BUILD_INPUTS],
    const size_t object_lengths[MAX_BUILD_INPUTS]) {
    static const char magic[] = "MAKSTATE2";
    uint8_t state[BUILD_STATE_BYTES] = {0};
    if (input_count < MIN_BUILD_INPUTS || input_count > MAX_BUILD_INPUTS)
        return 0;
    for (size_t index = 0; index < sizeof(magic) - 1; ++index)
        state[index] = (uint8_t)magic[index];
    state[9] = (uint8_t)input_count;
    put64(state, 16, manifest_hash);
    for (size_t input = 0; input < input_count; ++input) {
        if (!source_lengths[input] || !object_lengths[input]) return 0;
        put64(state, 24 + input * 8,
              build_hash(sources[input], source_lengths[input]));
        put64(state, 24 + MAX_BUILD_INPUTS * 8 + input * 8,
              build_hash(objects[input], object_lengths[input]));
    }
    return write_file(path, path_length, state, sizeof(state));
}

static size_t compile_build_object(const struct build_manifest *build,
                                   size_t input, const uint8_t *source,
                                   size_t source_length,
                                   uint8_t object[OBJECT_CAPACITY]) {
    uint8_t code[512] = {0};
    struct relocation relocations[MAX_RELOCATIONS] = {0};
    size_t relocation_count = 0;
    if (build->inputs[input].language == BUILD_LANGUAGE_ASM) {
        size_t code_length = assemble((const char *)source, source_length,
                                      code, sizeof(code), relocations,
                                      &relocation_count);
        if (!code_length) return 0;
        return emit_object(object, code, code_length, build->entry,
                           build->entry_length, relocations,
                           relocation_count);
    }
    if (build->inputs[input].language != BUILD_LANGUAGE_C) return 0;
    struct c_definition definitions[MAX_C_FUNCTIONS] = {0};
    size_t definition_count = 0;
    size_t code_length = compile_c_unit(
        (const char *)source, source_length, code, sizeof(code), definitions,
        &definition_count, relocations, &relocation_count);
    if (!code_length) return 0;
    return emit_object_definitions(object, code, code_length, definitions,
                                   definition_count, relocations,
                                   relocation_count);
}

static int incremental_build(
    const struct build_manifest *build, const char *manifest_path,
    size_t manifest_path_length, const uint8_t *manifest,
    size_t manifest_length, const uint8_t *const sources[MAX_BUILD_INPUTS],
    const size_t source_lengths[MAX_BUILD_INPUTS], size_t *cache_hits,
    size_t *cache_misses) {
    char state_path[MAX_BUILD_PATH_BYTES + BUILD_STATE_SUFFIX_BYTES] = {0};
    size_t state_path_length = 0;
    if (!cache_hits || !cache_misses ||
        !build_state_path(manifest_path, manifest_path_length, state_path,
                          &state_path_length) ||
        !build_state_path_safe(build, state_path, state_path_length))
        return 0;

    uint64_t saved_source_hashes[MAX_BUILD_INPUTS] = {0};
    uint64_t saved_object_hashes[MAX_BUILD_INPUTS] = {0};
    uint64_t manifest_hash = build_hash(manifest, manifest_length);
    int state_valid = read_build_state(
        state_path, state_path_length, manifest_hash, build->input_count,
        saved_source_hashes, saved_object_hashes);
    uint8_t object_storage[MAX_BUILD_INPUTS][OBJECT_CAPACITY] = {{0}};
    const uint8_t *objects[MAX_BUILD_INPUTS] = {0};
    size_t object_lengths[MAX_BUILD_INPUTS] = {0};
    for (size_t input = 0; input < build->input_count; ++input)
        objects[input] = object_storage[input];
    *cache_hits = 0;
    *cache_misses = 0;
    for (size_t input = 0; input < build->input_count; ++input) {
        uint64_t source_hash = build_hash(sources[input],
                                          source_lengths[input]);
        struct object_view view = {0};
        if (state_valid && saved_source_hashes[input] == source_hash) {
            object_lengths[input] = read_file(
                build->inputs[input].object_path,
                build->inputs[input].object_path_length,
                object_storage[input], sizeof(object_storage[input]));
            if (object_lengths[input] &&
                build_hash(object_storage[input], object_lengths[input]) ==
                    saved_object_hashes[input] &&
                parse_object(object_storage[input], object_lengths[input],
                             &view) &&
                validate_symbols(&view)) {
                ++*cache_hits;
                continue;
            }
        }
        object_lengths[input] = compile_build_object(
            build, input, sources[input], source_lengths[input],
            object_storage[input]);
        if (!object_lengths[input] ||
            !write_file(build->inputs[input].object_path,
                        build->inputs[input].object_path_length,
                        object_storage[input], object_lengths[input]))
            return 0;
        ++*cache_misses;
    }

    uint8_t linked_code[512] = {0};
    size_t entry_offset = 0;
    size_t linked_length = link_objects(
        objects, object_lengths, build->input_count, linked_code,
        sizeof(linked_code), build->entry, &entry_offset);
    volatile uint8_t image[IMAGE_CAPACITY];
    size_t image_length = emit_elf(image, linked_code, linked_length,
                                   entry_offset);
    if (!linked_length || !image_length ||
        !write_file(build->output_path, build->output_path_length,
                    (const uint8_t *)image, image_length) ||
        !write_build_state(state_path, state_path_length, manifest_hash,
                           build->input_count, sources, source_lengths,
                           objects, object_lengths))
        return 0;
    return 1;
}

static void write_build_marker(const char *mode, const char *manifest_path,
                               size_t manifest_path_length, int seeded,
                               size_t input_count, size_t cache_hits,
                               size_t cache_misses) {
    char inputs = (char)('0' + input_count);
    char hit = (char)('0' + cache_hits);
    char miss = (char)('0' + cache_misses);
    write_text("MAKOS_AARCH64_MAKBUILD_OK mode=");
    write_text(mode);
    write_text(" manifest=");
    write_bytes(manifest_path, manifest_path_length);
    write_text(" startup=sysv argc=2 envc=1 seeded=");
    write_text(seeded ? "1" : "0");
    write_text(" cache=makstate-v2 build_inputs=");
    write_bytes(&inputs, 1);
    write_text(" cache_hits=");
    write_bytes(&hit, 1);
    write_text(" cache_misses=");
    write_bytes(&miss, 1);
    write_text(" state_committed=1 status=42\n");
}

static void fail(uint64_t status) {
    syscall4(SYS_EXIT, status, 0, 0, 0);
    for (;;) __asm__ volatile("wfe");
}

__attribute__((section(".text._start"), noreturn)) void _start(
    uint64_t argc, char **argv, char **envp) {
    static const char fixture_manifest_path[] = "/home/user/generated.build";
    static const char alternate_manifest_path[] =
        "/home/user/generated-three.build";
    static const char main_source_path[] = "/home/user/generated.s";
    static const char program_source_path[] = "/home/user/generated-program.c";
    static const char library_source_path[] = "/home/user/generated-library.c";
    static const char helper_source_path[] = "/home/user/generated-helper.c";
    static const char build_manifest_source[] =
        "MAKBUILD1\n"
        "asm /home/user/generated.s /home/user/generated-main.o\n"
        "c /home/user/generated-program.c /home/user/generated-program.o\n"
        "c /home/user/generated-library.c /home/user/generated-library.o\n"
        "c /home/user/generated-helper.c /home/user/generated-helper.o\n"
        "link /home/user/generated-aarch64.elf _start\n";
    static const char alternate_manifest_source[] =
        "MAKBUILD1\n"
        "asm /home/user/generated.s /home/user/generated-three-main.o\n"
        "c /home/user/generated-program.c /home/user/generated-three-program.o\n"
        "c /home/user/generated-library.c /home/user/generated-three-library.o\n"
        "link /home/user/generated-three.elf _start\n";
    static const char malformed_build_header[] =
        "MAKBUILD0\n"
        "asm /home/user/generated.s /home/user/generated-main.o\n"
        "c /home/user/generated-program.c /home/user/generated-program.o\n"
        "c /home/user/generated-library.c /home/user/generated-library.o\n"
        "c /home/user/generated-helper.c /home/user/generated-helper.o\n"
        "link /home/user/generated-aarch64.elf _start\n";
    static const char malformed_build_relative[] =
        "MAKBUILD1\n"
        "asm generated.s /home/user/generated-main.o\n"
        "c /home/user/generated-program.c /home/user/generated-program.o\n"
        "c /home/user/generated-library.c /home/user/generated-library.o\n"
        "c /home/user/generated-helper.c /home/user/generated-helper.o\n"
        "link /home/user/generated-aarch64.elf _start\n";
    static const char malformed_build_duplicate[] =
        "MAKBUILD1\n"
        "asm /home/user/generated.s /home/user/generated-main.o\n"
        "c /home/user/generated-program.c /home/user/generated-main.o\n"
        "c /home/user/generated-library.c /home/user/generated-library.o\n"
        "c /home/user/generated-helper.c /home/user/generated-helper.o\n"
        "link /home/user/generated-aarch64.elf _start\n";
    static const char malformed_build_missing_link[] =
        "MAKBUILD1\n"
        "asm /home/user/generated.s /home/user/generated-main.o\n"
        "c /home/user/generated-program.c /home/user/generated-program.o\n"
        "c /home/user/generated-library.c /home/user/generated-library.o\n"
        "c /home/user/generated-helper.c /home/user/generated-helper.o\n";
    static const char minimal_build_source[] =
        "MAKBUILD1\n"
        "asm /home/user/min.s /home/user/min-main.o\n"
        "c /home/user/min.c /home/user/min-c.o\n"
        "link /home/user/min.elf _start\n";
    static const char maximum_build_source[] =
        "MAKBUILD1\n"
        "asm /home/user/max.s /home/user/max-main.o\n"
        "c /home/user/max1.c /home/user/max1.o\n"
        "c /home/user/max2.c /home/user/max2.o\n"
        "c /home/user/max3.c /home/user/max3.o\n"
        "c /home/user/max4.c /home/user/max4.o\n"
        "c /home/user/max5.c /home/user/max5.o\n"
        "link /home/user/max.elf _start\n";
    static const char malformed_build_too_many[] =
        "MAKBUILD1\n"
        "asm /home/user/max.s /home/user/max-main.o\n"
        "c /home/user/max1.c /home/user/max1.o\n"
        "c /home/user/max2.c /home/user/max2.o\n"
        "c /home/user/max3.c /home/user/max3.o\n"
        "c /home/user/max4.c /home/user/max4.o\n"
        "c /home/user/max5.c /home/user/max5.o\n"
        "c /home/user/max6.c /home/user/max6.o\n"
        "link /home/user/max.elf _start\n";
    static const char malformed_build_wrong_order[] =
        "MAKBUILD1\n"
        "c /home/user/min.c /home/user/min-c.o\n"
        "asm /home/user/min.s /home/user/min-main.o\n"
        "link /home/user/min.elf _start\n";
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
    static const char program_source[] =
        "int answer(int value) {\n"
        "    int values[3] = { (value * 3) - 20, 40, 0 };\n"
        "    if (values[0] >= 40) {\n"
        "        return adjust(values + 1, 1);\n"
        "    }\n"
        "    return 86;\n"
        "}\n"
        "int adjust(int *pointer, int delta) {\n"
        "    int *next = pointer + delta;\n"
        "    pointer[0] = combine(pointer[0], delta);\n"
        "    int distance = next - pointer;\n"
        "    int count = 0;\n"
        "    while (count < distance) {\n"
        "        *(pointer + delta) = pointer[0] + count + 1;\n"
        "        count = count + 1;\n"
        "    }\n"
        "    return *next;\n"
        "}\n";
    static const char library_source[] =
        "int combine(int value, int delta) {\n"
        "    return value + delta;\n"
        "}\n";
    static const char helper_source[] =
        "int helper(int value) {\n"
        "    return value + 2;\n"
        "}\n";
    static const char malformed_c_source[] =
        "int answer(int value) { return value / 2; }\n";
    static const char malformed_control_source[] =
        "int answer(int value) { if (value == 1) { return 42; } }\n";
    static const char malformed_loop_source[] =
        "int adjust(int value) { while (value != 0) { value = value - 1; } }\n";
    static const char malformed_assignment_source[] =
        "int adjust(int value) { missing = value; return value; }\n";
    static const char malformed_address_source[] =
        "int adjust(int value) { int *pointer = &missing; return value; }\n";
    static const char malformed_address_return_source[] =
        "int adjust(int value) { int scratch = value; return &scratch; }\n";
    static const char malformed_pointer_assignment_source[] =
        "int adjust(int value) { int scratch = value; int *pointer = &scratch; pointer = value; return scratch; }\n";
    static const char malformed_pointer_return_source[] =
        "int adjust(int *pointer) { return pointer; }\n";
    static const char malformed_array_index_source[] =
        "int answer(int value) { int values[2] = { value, 0 }; return values[2]; }\n";
    static const char malformed_pointer_add_source[] =
        "int answer(int value) { int values[2] = { value, 0 }; int *outside = values + 2; return *outside; }\n";
    static const char malformed_bounded_variable_add_source[] =
        "int answer(int value) { int values[2] = { value, 0 }; int offset = 1; int *unknown = values + offset; return *unknown; }\n";
    static const char malformed_pointer_scalar_subtract_source[] =
        "int difference(int *pointer, int delta) { return pointer - delta; }\n";
    static const char malformed_duplicate_function_source[] =
        "int answer(int value) { return value; } int answer(int value) { return value; }\n";
    static const char malformed_too_many_functions_source[] =
        "int one(int value) { return value; } int two(int value) { return value; } int three(int value) { return value; } int four(int value) { return value; }\n";
    static const char malformed_duplicate_parameter_source[] =
        "int adjust(int value, int value) { return value; }\n";
    static const char malformed_too_many_parameters_source[] =
        "int adjust(int first, int second, int third) { return first; }\n";
    static const char malformed_too_many_arguments_source[] =
        "int answer(int value) { return adjust(value, value, value); }\n";
    static const char relational_greater_source[] =
        "int greater(int value) { if (value > 5) { return 42; } return 0; }\n";
    static const char relational_at_most_source[] =
        "int at_most(int value) { if (value <= 5) { return 42; } return 0; }\n";
    static const char signed_pointer_offset_source[] =
        "int previous(int *pointer, int offset) { int *item = pointer + offset; return *item; }\n";
    static const char pointer_difference_source[] =
        "int distance(int *end, int *begin) { int count = end - begin; return count; }\n";
    static const char jit_source[] = "mov x0, #42\nret\n";
    static const char marker[] =
        "MAKOS_AARCH64_LINKER_OK sources=4 languages=aarch64-asm,c-subset-v1 "
        "compiler=guest-native assembler=guest-native objects=4 "
        "format=elf64-et-rel linker=guest-native relocations=R_AARCH64_CALL26:3 "
        "symbols=_start,answer,adjust,combine,helper output=/home/user/generated-aarch64.elf "
        "build_manifest=argv1 build_driver=makbuild-v1 build_inputs=4 cache=makstate-v2 cache_hits=0 cache_misses=4 state_committed=1 "
        "c_sources=/home/user/generated-program.c,/home/user/generated-library.c,/home/user/generated-helper.c translation_unit_functions=2,1,1 "
        "c_abi=aapcs64-int32-pointer64 "
        "c_features=multi-function,multi-parameter,parameter,pointer-parameter,local,array,array-decay,index,assignment,pointer,pointer-add,pointer-variable-add,pointer-difference,address-of,address-expression,dereference,if,equality,inequality,relational,while,call,return "
        "max_parameters=2 max_call_arguments=2 nonleaf_frame=96 c_operators=mul,sub,add c_relations=eq,ne,lt,le,gt,ge branch_results=42,86 "
        "loop_results=42,2 memory_results=42,2 pointer_call=answer-to-adjust "
        "pointee_results=42,44,2 delta_results=1:42,2:44,1:2 array_results=41:42:0,42:0:44,1:2:0 pointer_offset_call=1 pointer_variable_offset=delta dynamic_pointer_adds=2 signed_pointer_offset=-1:42 signed_pointer_difference=3:-3 relational_results=gt:42:0,le:42:0,ge:42:86,lt:42:44 "
        "code_bytes=76,140,168,60,56 object_bytes=688,976,616,608 intra_object_calls=1 cross_object_calls=2 linked_bytes=500 output_bytes=815 helper_result=42 "
        "persisted_reopened=1 manifest_input_bounds=2..6 malformed_build_denied=6 malformed_c_denied=17 "
        "malformed_relocation_denied=1 unresolved_symbol_denied=1 "
        "duplicate_definition_denied=1 segments=2 "
        "code_rx=1 data_nx=1 wx_denied=1 jit_result=42\n";

    if (argc != 2 || !argv || !argv[0] || !argv[1] || argv[2] || !envp ||
        !envp[0] || envp[1] ||
        !same_name(argv[0], length(argv[0]), "/system/aarch64-toolchain", 25))
        fail(79);
    size_t build_manifest_path_length = length(argv[1]);
    if (!build_manifest_path_length ||
        build_manifest_path_length >
            MAX_BUILD_PATH_BYTES - BUILD_STATE_SUFFIX_BYTES ||
        argv[1][0] != '/')
        fail(79);
    for (size_t index = 0; index < build_manifest_path_length; ++index)
        if (!build_path_byte(argv[1][index])) fail(79);
    int fixture_mode = same_name(envp[0], length(envp[0]), "MODE=fixture", 12);
    int build_mode = same_name(envp[0], length(envp[0]), "MODE=build", 10);
    if ((!fixture_mode && !build_mode) ||
        (fixture_mode &&
         !same_name(argv[1], build_manifest_path_length,
                    fixture_manifest_path, sizeof(fixture_manifest_path) - 1)))
        fail(79);
    const char *build_manifest_path = argv[1];

    if (fixture_mode &&
        (!write_file(build_manifest_path, build_manifest_path_length,
                     (const uint8_t *)build_manifest_source,
                     sizeof(build_manifest_source) - 1) ||
         !write_file(alternate_manifest_path,
                     sizeof(alternate_manifest_path) - 1,
                     (const uint8_t *)alternate_manifest_source,
                     sizeof(alternate_manifest_source) - 1) ||
         !write_file(main_source_path, sizeof(main_source_path) - 1,
                     (const uint8_t *)main_source, sizeof(main_source) - 1) ||
         !write_file(program_source_path, sizeof(program_source_path) - 1,
                     (const uint8_t *)program_source,
                     sizeof(program_source) - 1) ||
         !write_file(library_source_path, sizeof(library_source_path) - 1,
                     (const uint8_t *)library_source,
                     sizeof(library_source) - 1) ||
         !write_file(helper_source_path, sizeof(helper_source_path) - 1,
                     (const uint8_t *)helper_source,
                     sizeof(helper_source) - 1)))
        fail(80);
    uint8_t manifest_input[512] = {0};
    size_t manifest_length = read_file(
        build_manifest_path, build_manifest_path_length,
        manifest_input, sizeof(manifest_input));
    struct build_manifest build = {0}, minimum_build = {0};
    struct build_manifest maximum_build = {0}, malformed_build = {0};
    if (!manifest_length ||
        (fixture_mode && manifest_length != sizeof(build_manifest_source) - 1) ||
        !parse_build_manifest((const char *)manifest_input, manifest_length,
                              &build) ||
        parse_build_manifest(malformed_build_header,
                             sizeof(malformed_build_header) - 1,
                             &malformed_build) ||
        parse_build_manifest(malformed_build_relative,
                             sizeof(malformed_build_relative) - 1,
                             &malformed_build) ||
        parse_build_manifest(malformed_build_duplicate,
                             sizeof(malformed_build_duplicate) - 1,
                             &malformed_build) ||
        parse_build_manifest(malformed_build_missing_link,
                             sizeof(malformed_build_missing_link) - 1,
                             &malformed_build) ||
        !parse_build_manifest(minimal_build_source,
                              sizeof(minimal_build_source) - 1,
                              &minimum_build) ||
        minimum_build.input_count != MIN_BUILD_INPUTS ||
        !parse_build_manifest(maximum_build_source,
                              sizeof(maximum_build_source) - 1,
                              &maximum_build) ||
        maximum_build.input_count != MAX_BUILD_INPUTS ||
        parse_build_manifest(malformed_build_too_many,
                             sizeof(malformed_build_too_many) - 1,
                             &malformed_build) ||
        parse_build_manifest(malformed_build_wrong_order,
                             sizeof(malformed_build_wrong_order) - 1,
                             &malformed_build))
        fail(81);
    uint8_t source_storage[MAX_BUILD_INPUTS][BUILD_SOURCE_CAPACITY] = {{0}};
    const uint8_t *build_sources[MAX_BUILD_INPUTS] = {0};
    size_t build_source_lengths[MAX_BUILD_INPUTS] = {0};
    for (size_t input = 0; input < build.input_count; ++input) {
        build_sources[input] = source_storage[input];
        build_source_lengths[input] = read_file(
            build.inputs[input].source_path,
            build.inputs[input].source_path_length, source_storage[input],
            sizeof(source_storage[input]));
        if (!build_source_lengths[input]) fail(81);
    }
    if (fixture_mode &&
        (build.input_count != 4 ||
         build_source_lengths[0] != sizeof(main_source) - 1 ||
         build_source_lengths[1] != sizeof(program_source) - 1 ||
         build_source_lengths[2] != sizeof(library_source) - 1 ||
         build_source_lengths[3] != sizeof(helper_source) - 1))
        fail(81);
    if (build_mode) {
        size_t cache_hits = 0, cache_misses = 0;
        if (!incremental_build(&build, build_manifest_path,
                               build_manifest_path_length, manifest_input,
                               manifest_length, build_sources,
                               build_source_lengths, &cache_hits,
                               &cache_misses) ||
            cache_hits + cache_misses != build.input_count)
            fail(91);
        write_build_marker("build", build_manifest_path,
                           build_manifest_path_length, 0, build.input_count,
                           cache_hits, cache_misses);
        syscall4(SYS_EXIT, 42, 0, 0, 0);
        __builtin_unreachable();
    }

    const uint8_t *source_input = source_storage[0];
    const uint8_t *program_input = source_storage[1];
    const uint8_t *library_input = source_storage[2];
    const uint8_t *helper_input = source_storage[3];
    size_t source_length = build_source_lengths[0];
    size_t program_source_length = build_source_lengths[1];
    size_t library_source_length = build_source_lengths[2];
    size_t helper_source_length = build_source_lengths[3];
    uint8_t main_code[128] = {0}, program_code[384] = {0};
    uint8_t library_code[128] = {0};
    uint8_t helper_code[128] = {0};
    struct relocation main_relocations[MAX_RELOCATIONS] = {0};
    struct relocation program_relocations[MAX_RELOCATIONS] = {0};
    struct relocation library_relocations[MAX_RELOCATIONS] = {0};
    struct relocation helper_relocations[MAX_RELOCATIONS] = {0};
    size_t main_relocation_count = 0, program_relocation_count = 0;
    size_t library_relocation_count = 0, helper_relocation_count = 0;
    size_t main_code_length = assemble((const char *)source_input, source_length,
                                       main_code, sizeof(main_code),
                                       main_relocations,
                                       &main_relocation_count);
    struct c_definition program_definitions[MAX_C_FUNCTIONS] = {0};
    size_t program_definition_count = 0;
    struct c_definition library_definitions[MAX_C_FUNCTIONS] = {0};
    size_t library_definition_count = 0;
    struct c_definition helper_definitions[MAX_C_FUNCTIONS] = {0};
    size_t helper_definition_count = 0;
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
    malformed_function_length = 0;
    if (compile_c(malformed_address_source,
                  sizeof(malformed_address_source) - 1, malformed_code,
                  sizeof(malformed_code), malformed_function,
                  &malformed_function_length, 0, 0) != 0)
        fail(82);
    malformed_function_length = 0;
    if (compile_c(malformed_address_return_source,
                  sizeof(malformed_address_return_source) - 1,
                  malformed_code, sizeof(malformed_code), malformed_function,
                  &malformed_function_length, 0, 0) != 0)
        fail(82);
    malformed_function_length = 0;
    if (compile_c(malformed_pointer_assignment_source,
                  sizeof(malformed_pointer_assignment_source) - 1,
                  malformed_code, sizeof(malformed_code), malformed_function,
                  &malformed_function_length, 0, 0) != 0)
        fail(82);
    malformed_function_length = 0;
    if (compile_c(malformed_pointer_return_source,
                  sizeof(malformed_pointer_return_source) - 1,
                  malformed_code, sizeof(malformed_code), malformed_function,
                  &malformed_function_length, 0, 0) != 0)
        fail(82);
    malformed_function_length = 0;
    if (compile_c(malformed_array_index_source,
                  sizeof(malformed_array_index_source) - 1,
                  malformed_code, sizeof(malformed_code), malformed_function,
                  &malformed_function_length, 0, 0) != 0)
        fail(82);
    malformed_function_length = 0;
    if (compile_c(malformed_pointer_add_source,
                  sizeof(malformed_pointer_add_source) - 1,
                  malformed_code, sizeof(malformed_code), malformed_function,
                  &malformed_function_length, 0, 0) != 0)
        fail(82);
    malformed_function_length = 0;
    if (compile_c(malformed_bounded_variable_add_source,
                  sizeof(malformed_bounded_variable_add_source) - 1,
                  malformed_code, sizeof(malformed_code), malformed_function,
                  &malformed_function_length, 0, 0) != 0)
        fail(82);
    malformed_function_length = 0;
    if (compile_c(malformed_pointer_scalar_subtract_source,
                  sizeof(malformed_pointer_scalar_subtract_source) - 1,
                  malformed_code, sizeof(malformed_code), malformed_function,
                  &malformed_function_length, 0, 0) != 0)
        fail(82);
    malformed_function_length = 0;
    if (compile_c(malformed_duplicate_parameter_source,
                  sizeof(malformed_duplicate_parameter_source) - 1,
                  malformed_code, sizeof(malformed_code), malformed_function,
                  &malformed_function_length, 0, 0) != 0)
        fail(82);
    malformed_function_length = 0;
    if (compile_c(malformed_too_many_parameters_source,
                  sizeof(malformed_too_many_parameters_source) - 1,
                  malformed_code, sizeof(malformed_code), malformed_function,
                  &malformed_function_length, 0, 0) != 0)
        fail(82);
    malformed_function_length = 0;
    if (compile_c(malformed_too_many_arguments_source,
                  sizeof(malformed_too_many_arguments_source) - 1,
                  malformed_code, sizeof(malformed_code), malformed_function,
                  &malformed_function_length, 0, 0) != 0)
        fail(82);
    struct c_definition malformed_definitions[MAX_C_FUNCTIONS] = {0};
    size_t malformed_definition_count = 0;
    struct relocation malformed_relocations[MAX_RELOCATIONS] = {0};
    size_t malformed_relocation_count = 0;
    if (compile_c_unit(malformed_duplicate_function_source,
                       sizeof(malformed_duplicate_function_source) - 1,
                       malformed_code, sizeof(malformed_code),
                       malformed_definitions, &malformed_definition_count,
                       malformed_relocations,
                       &malformed_relocation_count) != 0)
        fail(82);
    malformed_definition_count = 0;
    malformed_relocation_count = 0;
    if (compile_c_unit(malformed_too_many_functions_source,
                       sizeof(malformed_too_many_functions_source) - 1,
                       malformed_code, sizeof(malformed_code),
                       malformed_definitions, &malformed_definition_count,
                       malformed_relocations,
                       &malformed_relocation_count) != 0)
        fail(82);
    size_t program_code_length = compile_c_unit(
        (const char *)program_input, program_source_length, program_code,
        sizeof(program_code), program_definitions, &program_definition_count,
        program_relocations, &program_relocation_count);
    size_t library_code_length = compile_c_unit(
        (const char *)library_input, library_source_length, library_code,
        sizeof(library_code), library_definitions, &library_definition_count,
        library_relocations, &library_relocation_count);
    size_t helper_code_length = compile_c_unit(
        (const char *)helper_input, helper_source_length, helper_code,
        sizeof(helper_code), helper_definitions, &helper_definition_count,
        helper_relocations, &helper_relocation_count);
    if (main_code_length != 76 || program_code_length != 308 ||
        library_code_length != 60 || helper_code_length != 56 ||
        program_definition_count != 2 || library_definition_count != 1 ||
        helper_definition_count != 1 ||
        !same_name(program_definitions[0].name,
                   program_definitions[0].length, "answer", 6) ||
        program_definitions[0].offset != 0 ||
        program_definitions[0].size != 140 ||
        !same_name(program_definitions[1].name,
                   program_definitions[1].length, "adjust", 6) ||
        program_definitions[1].offset != 140 ||
        program_definitions[1].size != 168 ||
        !same_name(library_definitions[0].name,
                   library_definitions[0].length, "combine", 7) ||
        library_definitions[0].offset != 0 ||
        library_definitions[0].size != 60 ||
        !same_name(helper_definitions[0].name,
                   helper_definitions[0].length, "helper", 6) ||
        helper_definitions[0].offset != 0 ||
        helper_definitions[0].size != 56 ||
        main_relocation_count != 1 || main_relocations[0].offset != 52 ||
        program_relocation_count != 2 ||
        program_relocations[0].offset != 92 ||
        !same_name(program_relocations[0].name,
                   program_relocations[0].length, "adjust", 6) ||
        !same_name(program_relocations[1].name,
                   program_relocations[1].length, "combine", 7) ||
        library_relocation_count != 0 || helper_relocation_count != 0)
        fail(82);

    uint8_t *relational_jit =
        (uint8_t *)(uintptr_t)syscall4(SYS_VM_MAP, 0, 0, 0, 0);
    if ((uintptr_t)relational_jit == UINT64_MAX) fail(83);
    char relational_function[MAX_LABEL_BYTES] = {0};
    size_t relational_function_length = 0;
    size_t greater_length = compile_c(
        relational_greater_source, sizeof(relational_greater_source) - 1,
        relational_jit, 128, relational_function,
        &relational_function_length, 0, 0);
    relational_function_length = 0;
    size_t at_most_length = compile_c(
        relational_at_most_source, sizeof(relational_at_most_source) - 1,
        relational_jit + 128, 128, relational_function,
        &relational_function_length, 0, 0);
    relational_function_length = 0;
    size_t previous_length = compile_c(
        signed_pointer_offset_source, sizeof(signed_pointer_offset_source) - 1,
        relational_jit + 256, 128, relational_function,
        &relational_function_length, 0, 0);
    relational_function_length = 0;
    size_t distance_length = compile_c(
        pointer_difference_source, sizeof(pointer_difference_source) - 1,
        relational_jit + 384, 128, relational_function,
        &relational_function_length, 0, 0);
    if (!greater_length || greater_length > 128 ||
        !at_most_length || at_most_length > 128 ||
        !previous_length || previous_length > 128 ||
        !distance_length || distance_length > 128 ||
        syscall4(SYS_VM_PROTECT, (uintptr_t)relational_jit,
                 PROT_READ | PROT_WRITE | PROT_EXEC, 0, 0) != 0 ||
        syscall4(SYS_VM_PROTECT, (uintptr_t)relational_jit,
                 PROT_READ | PROT_EXEC, 0, 0) != 1)
        fail(83);
    uint64_t (*compiled_greater)(uint64_t) =
        (uint64_t (*)(uint64_t))(uintptr_t)relational_jit;
    uint64_t (*compiled_at_most)(uint64_t) =
        (uint64_t (*)(uint64_t))(uintptr_t)(relational_jit + 128);
    uint64_t (*compiled_previous)(uint32_t *, uint64_t) =
        (uint64_t (*)(uint32_t *, uint64_t))(uintptr_t)(relational_jit + 256);
    uint64_t (*compiled_distance)(uint32_t *, uint32_t *) =
        (uint64_t (*)(uint32_t *, uint32_t *))(uintptr_t)(relational_jit + 384);
    uint32_t previous_values[2] = {42, 7};
    uint32_t distance_values[4] = {0, 0, 0, 0};
    if (compiled_greater(6) != 42 || compiled_greater(5) != 0 ||
        compiled_at_most(5) != 42 || compiled_at_most(6) != 0 ||
        compiled_previous(previous_values + 1, UINT32_MAX) != 42 ||
        compiled_distance(distance_values + 3, distance_values) != 3 ||
        (uint32_t)compiled_distance(distance_values, distance_values + 3) !=
            UINT32_MAX - 2)
        fail(83);

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

    uint8_t main_object[OBJECT_CAPACITY], program_object[OBJECT_CAPACITY];
    uint8_t library_object[OBJECT_CAPACITY];
    uint8_t helper_object[OBJECT_CAPACITY];
    size_t main_object_length = emit_object(main_object, main_code,
                                            main_code_length, build.entry,
                                            build.entry_length,
                                            main_relocations,
                                            main_relocation_count);
    size_t program_object_length = emit_object_definitions(
        program_object, program_code, program_code_length,
        program_definitions, program_definition_count, program_relocations,
        program_relocation_count);
    size_t library_object_length = emit_object_definitions(
        library_object, library_code, library_code_length,
        library_definitions, library_definition_count, library_relocations,
        library_relocation_count);
    size_t helper_object_length = emit_object_definitions(
        helper_object, helper_code, helper_code_length,
        helper_definitions, helper_definition_count, helper_relocations,
        helper_relocation_count);
    if (main_object_length != 688 || program_object_length != 976 ||
        library_object_length != 616 || helper_object_length != 608 ||
        !write_file(build.inputs[0].object_path,
                    build.inputs[0].object_path_length,
                    main_object, main_object_length) ||
        !write_file(build.inputs[1].object_path,
                    build.inputs[1].object_path_length,
                    program_object, program_object_length) ||
        !write_file(build.inputs[2].object_path,
                    build.inputs[2].object_path_length,
                    library_object, library_object_length) ||
        !write_file(build.inputs[3].object_path,
                    build.inputs[3].object_path_length,
                    helper_object, helper_object_length))
        fail(85);

    memset(main_object, 0, sizeof(main_object));
    memset(program_object, 0, sizeof(program_object));
    memset(library_object, 0, sizeof(library_object));
    memset(helper_object, 0, sizeof(helper_object));
    main_object_length = read_file(build.inputs[0].object_path,
                                   build.inputs[0].object_path_length,
                                   main_object, sizeof(main_object));
    program_object_length = read_file(build.inputs[1].object_path,
                                      build.inputs[1].object_path_length,
                                      program_object, sizeof(program_object));
    library_object_length = read_file(build.inputs[2].object_path,
                                      build.inputs[2].object_path_length,
                                      library_object, sizeof(library_object));
    helper_object_length = read_file(build.inputs[3].object_path,
                                     build.inputs[3].object_path_length,
                                     helper_object, sizeof(helper_object));
    if (!main_object_length || !program_object_length ||
        !library_object_length || !helper_object_length)
        fail(86);

    struct object_view corrupt_view;
    if (!parse_object(main_object, main_object_length, &corrupt_view)) fail(87);
    size_t corrupt_info = corrupt_view.rela_offset + 8;
    uint8_t saved_type = main_object[corrupt_info];
    main_object[corrupt_info] = (uint8_t)(R_AARCH64_CALL26 - 1);
    uint8_t linked_code[512] = {0};
    size_t entry_offset = 0;
    const uint8_t *objects[MAX_LINK_OBJECTS] = {
        main_object, program_object, library_object, helper_object,
    };
    size_t object_lengths[MAX_LINK_OBJECTS] = {
        main_object_length, program_object_length, library_object_length,
        helper_object_length,
    };
    if (link_objects(objects, object_lengths, 4, linked_code,
                     sizeof(linked_code), build.entry, &entry_offset) != 0)
        fail(88);
    main_object[corrupt_info] = saved_type;
    size_t corrupt_addend = corrupt_view.rela_offset + 16;
    if (main_object[corrupt_addend] != 0) fail(88);
    main_object[corrupt_addend] = 1;
    if (link_objects(objects, object_lengths, 4, linked_code,
                     sizeof(linked_code), build.entry, &entry_offset) != 0)
        fail(88);
    main_object[corrupt_addend] = 0;
    struct object_view program_view;
    if (!parse_object(program_object, program_object_length, &program_view) ||
        program_view.symbol_count != 4)
        fail(88);
    size_t adjust_symbol = program_view.symbol_offset + 2 * SYMBOL_SIZE;
    if (!string_is(&program_view, get32(program_object, adjust_symbol),
                   "adjust") ||
        get16(program_object, adjust_symbol + 6) != 1 ||
        get64(program_object, adjust_symbol + 8) != 140 ||
        get64(program_object, adjust_symbol + 16) != 168)
        fail(88);
    put16(program_object, adjust_symbol + 6, 0);
    put64(program_object, adjust_symbol + 8, 0);
    put64(program_object, adjust_symbol + 16, 0);
    if (link_objects(objects, object_lengths, 4, linked_code,
                     sizeof(linked_code), build.entry, &entry_offset) != 0)
        fail(88);
    put16(program_object, adjust_symbol + 6, 1);
    put64(program_object, adjust_symbol + 8, 140);
    put64(program_object, adjust_symbol + 16, 168);
    const uint8_t *duplicate_objects[MAX_LINK_OBJECTS] = {
        main_object, program_object, program_object,
    };
    size_t duplicate_lengths[MAX_LINK_OBJECTS] = {
        main_object_length, program_object_length, program_object_length,
    };
    if (link_objects(duplicate_objects, duplicate_lengths, 3, linked_code,
                     sizeof(linked_code), build.entry, &entry_offset) != 0)
        fail(88);
    if (link_objects(objects, object_lengths, 2, linked_code,
                     sizeof(linked_code), build.entry, &entry_offset) != 0)
        fail(88);
    size_t linked_length = link_objects(objects, object_lengths, 4, linked_code,
                                        sizeof(linked_code), build.entry,
                                        &entry_offset);
    if (linked_length != 500 || entry_offset != 0) fail(89);

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
    uint64_t (*compiled_adjust)(uint32_t *, uint64_t) =
        (uint64_t (*)(uint32_t *, uint64_t))(uintptr_t)(compiled_jit + 216);
    uint64_t (*compiled_combine)(uint64_t, uint64_t) =
        (uint64_t (*)(uint64_t, uint64_t))(uintptr_t)(compiled_jit + 384);
    uint64_t (*compiled_helper)(uint64_t) =
        (uint64_t (*)(uint64_t))(uintptr_t)(compiled_jit + 444);
    uint32_t forty[3] = {40, 0, 0};
    uint32_t scaled[3] = {40, 0, 0};
    uint32_t zero[3] = {0, 0, 0};
    if (compiled_answer(20) != 42 || compiled_answer(0) != 86 ||
        compiled_combine(40, 2) != 42 || compiled_helper(40) != 42 ||
        compiled_adjust(forty, 1) != 42 ||
        forty[0] != 41 || forty[1] != 42 || forty[2] != 0 ||
        compiled_adjust(scaled, 2) != 44 ||
        scaled[0] != 42 || scaled[1] != 0 || scaled[2] != 44 ||
        compiled_adjust(zero, 1) != 2 ||
        zero[0] != 1 || zero[1] != 2 || zero[2] != 0)
        fail(89);

    volatile uint8_t image[IMAGE_CAPACITY];
    size_t image_length = emit_elf(image, linked_code, linked_length,
                                   entry_offset);
    if (image_length != 815 ||
        !write_file(build.output_path, build.output_path_length,
                    (const uint8_t *)image, image_length))
        fail(90);
    char state_path[MAX_BUILD_PATH_BYTES + BUILD_STATE_SUFFIX_BYTES] = {0};
    size_t state_path_length = 0;
    if (!build_state_path(build_manifest_path, build_manifest_path_length,
                          state_path, &state_path_length) ||
        !build_state_path_safe(&build, state_path, state_path_length) ||
        !write_build_state(state_path, state_path_length,
                           build_hash(manifest_input, manifest_length),
                           build.input_count, build_sources,
                           build_source_lengths, objects, object_lengths))
        fail(90);
    write_build_marker("fixture", build_manifest_path,
                       build_manifest_path_length, 1, build.input_count, 0, 4);
    write_bytes(marker, sizeof(marker) - 1);
    syscall4(SYS_EXIT, 42, 0, 0, 0);
    __builtin_unreachable();
}
