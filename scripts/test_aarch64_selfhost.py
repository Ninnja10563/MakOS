#!/usr/bin/env python3
"""Structural guard for guest-native multi-function AArch64 compilation/linking."""

from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[1]
TOOLCHAIN = (ROOT / "user/aarch64_toolchain.c").read_text()
SHELL = (ROOT / "user/aarch64_shell.c").read_text()
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
SECURITY = (ROOT / "kernel/src/security.rs").read_text()
BUILD = (ROOT / "kernel/build.rs").read_text()
VFS = (ROOT / "kernel/src/vfs.rs").read_text()
RUNTIME = (ROOT / "scripts/boot_test_aarch64.py").read_text()
FOCUSED_RUNTIME = (ROOT / "scripts/boot_test_aarch64_selfhost.py").read_text()
REPOSITORY_C = (ROOT / "user/aarch64_selfhost_probe.c").read_text()
REPOSITORY_ASM = (ROOT / "user/aarch64_selfhost_probe.S").read_text()
PRODUCTION_C = (ROOT / "ports/musl/shared-demo.c").read_text()
SELFHOST_STDINT = (ROOT / "sdk/selfhost/include/stdint.h").read_text()


def require(source: str, fragment: str) -> None:
    if fragment not in source:
        raise AssertionError(f"missing self-hosting guard: {fragment}")


for fragment in (
    "void *memcpy(void *destination, const void *source, size_t count)",
    "volatile uint8_t *output = destination;",
    "const volatile uint8_t *input = source;",
    "output[index] = input[index]",
    "return destination;",
):
    require(TOOLCHAIN, fragment)
require(BUILD, '"--no-undefined"')


for fragment in (
    '"_start:\\n"',
    '"cmp x0, #1\\n"',
    '"b.eq success\\n"',
    '"ldr x3, [x1, #8]\\n"',
    '"ldrb w4, [x3]\\n"',
    '"mov x8, #5\\n"',
    '"svc #0\\n"',
    '"bl answer\\n"',
    '"int answer(int value) {\\n"',
    '"    int values[3] = { (value * 3) - 20, 40, 0 };\\n"',
    '"    if (values[0] >= 40) {\\n"',
    '"        return adjust(values + 1, 1);\\n"',
    '"    return 86;\\n"',
    '"int adjust(int *pointer, int delta) {\\n"',
    '"    int *next = pointer + delta;\\n"',
    '"    pointer[0] = combine(pointer[0], delta);\\n"',
    '"    int distance = next - pointer;\\n"',
    '"    int count = 0;\\n"',
    '"    while (count < distance) {\\n"',
    '"        *(pointer + delta) = pointer[0] + count + 1;\\n"',
    '"        count = count + 1;\\n"',
    '"    return *next;\\n"',
    '"int combine(int value, int delta) {\\n"',
    '"    return value + delta;\\n"',
    "static size_t assemble(",
    "static size_t compile_c(",
    "static size_t compile_c_unit(",
    "static int parse_build_manifest(",
    '"MAKBUILD1\\n"',
    '"asm /home/user/generated.s /home/user/generated-main.o\\n"',
    '"c /home/user/generated-program.c /home/user/generated-program.o\\n"',
    '"c /home/user/generated-library.c /home/user/generated-library.o\\n"',
    '"c /home/user/generated-helper.c /home/user/generated-helper.o\\n"',
    '"link /home/user/generated-aarch64.elf _start\\n"',
    '"/home/user/generated-three.build"',
    '"link /home/user/generated-three.elf _start\\n"',
    "MIN_BUILD_INPUTS = 2",
    "MAX_BUILD_INPUTS = 6",
    "MAX_BUILD_PATH_BYTES = 96",
    "BUILD_SOURCE_CAPACITY = 768",
    "BUILD_EXPANDED_SOURCE_CAPACITY = 1536",
    "BUILD_HEADER_CAPACITY = 1280",
    "MAX_BUILD_HEADER_DEPTH = 4",
    "MAX_BUILD_HEADER_DEPENDENCIES = 8",
    "MAX_BUILD_MACROS = 8",
    "MAX_BUILD_MACRO_VALUE_BYTES = 64",
    "MAX_BUILD_MACRO_PARAMETERS = 4",
    "MAX_BUILD_MACRO_ARGUMENT_BYTES = 64",
    "MAX_BUILD_MACRO_SUBSTITUTION_BYTES = 256",
    "MAX_BUILD_MACRO_EXPANSION_DEPTH = 8",
    "MAX_BUILD_CONDITIONAL_DEPTH = 4",
    "struct build_dependencies",
    "struct build_macro",
    "struct conditional_state",
    "static int expand_source_recursive(",
    "static int record_dependency(",
    "static int define_macro(",
    "static size_t macro_parameter_lookup(",
    "static int parse_macro_arguments(",
    "static int expand_macro_bytes(",
    "static int append_macro_expanded(",
    "enum read_file_failure",
    "static size_t read_file_with_failure(",
    "READ_FILE_OPEN_FAILED",
    "READ_FILE_READ_FAILED",
    "READ_FILE_CLOSE_FAILED",
    "static void report_include_failure(",
    "static size_t expand_build_source(",
    "static size_t expand_build_source_diagnostic(",
    "MAKOS_AARCH64_C_INCLUDE_ERROR path=",
    '"protected-path"',
    '"dependency"',
    '"recursive-content"',
    "static int verify_generated_header(",
    "MAKOS_AARCH64_GENERATED_HEADER_ERROR path=",
    'write_generated_header_failure(path, path_length, "length")',
    'write_generated_header_failure(path, path_length, "content")',
    "MAKOS_AARCH64_GENERATED_HEADERS_OK",
    "identity=guest-readback-exact",
    "OUTPUT_RECORD_CAPACITY = 768",
    "struct output_record",
    "static int record_append_bytes(",
    "static int record_append_text(",
    "static int record_append_decimal(",
    "static int write_record(",
    "MAKOS_AARCH64_C_HEADER_DEP_OK",
    "MAKOS_AARCH64_C_PREPROCESSOR_GUARD_OK",
    "fingerprint=expanded-source",
    "preprocessor=bounded-macro-if-expressions",
    "macro_expansion=text,function-like parameters=4",
    "expansion_depth=8",
    '"#define APPLY_DELTA(value, delta) ((value) + (delta))\\n"',
    '"#define FIVE(a, b, c, d, e) ((a) + (b))\\n"',
    '"#define PAIR(first, second) ((first) + (second))\\n"',
    '"#define LOOP(value) LOOP(value)\\n"',
    '"#define TEXT(value) #value\\n"',
    "struct preprocessor_expression",
    "static int evaluate_preprocessor_expression(",
    "static int preprocessor_expression_equality(",
    "static int preprocessor_expression_multiplicative(",
    "static int preprocessor_expression_additive(",
    "static int preprocessor_expression_shift(",
    "static int preprocessor_expression_bitwise_and(",
    "static int preprocessor_expression_bitwise_xor(",
    "static int preprocessor_expression_bitwise_or(",
    "static int preprocessor_expression_conditional(",
    "static int preprocessor_checked_value(",
    '"#if 1 == 2 < 3\\n"',
    "if_expression=defined,numeric,arithmetic,shift,",
    "comparison,bitwise,not,and,or,short-circuit,conditional ",
    "elif=selected",
    "include_guard=deduplicated",
    "cycle=denied overdepth=denied ",
    "malformed=define,endif,unterminated,",
    "duplicate-else,expression,elif-after-else,zero-divisor,",
    "shift-range,overflow,conditional-syntax,",
    "conditional-selected-trap,macro-parameters,macro-arity,",
    "macro-recursion,macro-token-op-denied ",
    '"/home/user/generated-header.build"',
    '"/home/user/generated-nested.build"',
    '"/home/user/generated-nested-control.c"',
    '"/home/user/generated-inline.h"',
    '"/home/user/generated-leaf.h"',
    "malformed_build_header",
    "malformed_build_relative",
    "malformed_build_duplicate",
    "malformed_build_missing_link",
    "minimal_build_source",
    "maximum_build_source",
    "malformed_build_too_many",
    "malformed_build_wrong_order",
    "uint64_t argc, char **argv, char **envp",
    '"/system/aarch64-toolchain", 25',
    '"MODE=fixture", 12',
    '"MODE=build", 10',
    "const char *build_manifest_path = argv[1]",
    "if (fixture_mode &&",
    "MAKOS_AARCH64_MAKBUILD_OK mode=",
    '" cache=makstate-v2 build_inputs="',
    '" state_committed=1 status=42\\n"',
    "BUILD_STATE_BYTES = 120",
    'static const char magic[] = "MAKSTATE2"',
    "state[9] = (uint8_t)input_count",
    "state[9] != input_count",
    "static uint64_t build_hash(",
    "static int build_state_path(",
    "static int build_state_path_safe(",
    "static int read_build_state(",
    "static int write_build_state(",
    "static size_t compile_build_object(",
    "static int incremental_build(",
    "saved_source_hashes[input] == source_hash",
    "saved_object_hashes[input]",
    "parse_object(object_storage[input]",
    "validate_symbols(&view)",
    "++*cache_hits",
    "++*cache_misses",
    "write_build_state(state_path",
    "static int c_compile_function(",
    "static int c_find_parameter(",
    "static int c_variable_register(",
    "static int c_pointer_register(",
    "static int c_pointer_index_register(",
    "static int c_call_argument(",
    "static int c_unary(",
    "static int c_literal_zero_operand(",
    "static struct c_local *c_find_local(",
    "static int c_store_stack_local(",
    "static int c_load_stack_local(",
    "static int c_additive(",
    "static int c_multiplicative(",
    "static int c_comparison(",
    "static int c_patch_conditional(",
    "static int c_patch_branch(",
    "static int c_declaration(",
    "static int c_assignment(",
    "static int c_control_block_body(",
    "static int c_if_statement(",
    "static int c_while(",
    "MAX_C_LOCALS = 4",
    "OBJECT_CAPACITY = 4096",
    "LINKED_CODE_CAPACITY = 1024",
    "DATA_OFFSET = 1536",
    "IMAGE_CAPACITY = 2048",
    "MAX_C_FUNCTIONS = 6",
    "MAX_C_PARAMETERS = 6",
    "MAX_C_BLOCK_DEPTH = 4",
    "MAX_RELOCATIONS = 16",
    "UINT32_C(0x1b007c00)",
    "UINT32_C(0x1ac00c00)",
    "UINT32_C(0x1b008000)",
    "UINT32_C(0x4b0003e0)",
    "UINT32_C(0x0b000000)",
    "UINT32_C(0x4b000000)",
    "UINT32_C(0x52800000)",
    "UINT32_C(0xa9ba7bfd)",
    "UINT32_C(0xa8c67bfd)",
    "UINT32_C(0x2a0003e0)",
    "UINT32_C(0xa90363f7)",
    "UINT32_C(0xa94363f7)",
    "UINT32_C(0xf9002bf9)",
    "UINT32_C(0xf9402bf9)",
    "UINT32_C(0x8b20c800)",
    "UINT32_C(0xcb000000)",
    "UINT32_C(0x9342fc00)",
    "UINT32_C(0xaa0003e0)",
    "UINT32_C(0xb9000000)",
    "UINT32_C(0xb9400000)",
    "UINT32_C(0x910003e0)",
    "UINT32_C(0x91000000)",
    "UINT32_C(0x94000000)",
    "UINT32_C(0x6b00001f)",
    "UINT32_C(0x54000000)",
    "UINT32_C(0x14000000)",
    "malformed_c_source",
    "malformed_divide_zero_source",
    "malformed_control_source",
    "malformed_loop_source",
    "malformed_assignment_source",
    "malformed_address_source",
    "malformed_address_return_source",
    "malformed_pointer_assignment_source",
    "malformed_pointer_return_source",
    "malformed_array_index_source",
    "malformed_pointer_add_source",
    "malformed_bounded_variable_add_source",
    "malformed_pointer_scalar_subtract_source",
    "malformed_duplicate_parameter_source",
    "malformed_too_many_parameters_source",
    "malformed_too_many_arguments_source",
    '"int adjust(int first, int second, int third, int fourth, int fifth, int sixth, int seventh) { return first; }\\n"',
    '"int answer(int value) { return adjust(value, value, value, value, value, value, value); }\\n"',
    "three_argument_source",
    '"int sum3(int first, int second, int third) {\\n"',
    '"    return sum3(value, 1, 1);\\n"',
    "six_argument_source",
    '"int sum6(int first, int second, int third, int fourth, int fifth, int sixth) {\\n"',
    '"    return sum6(value, 1, 1, 1, 1, 1);\\n"',
    "six_argument_code_length != 196",
    "six_argument_object_length != 808",
    "six_argument_relocations[0].offset != 172",
    "compiled_sum6(10, 5, 6, 7, 8, 6) != 42",
    "compiled_invoke6(37) != 42",
    "MAKOS_AARCH64_C_SIX_ARGUMENT_OK parameters=6 call_arguments=6",
    "UINT32_C(0xa9b97bfd)",
    "UINT32_C(0xa9056bf9)",
    "UINT32_C(0xa90673fb)",
    "UINT32_C(0xa94673fb)",
    "UINT32_C(0xa9456bf9)",
    "UINT32_C(0xa8c77bfd)",
    "signed_arithmetic_source",
    '"int divide(int value) { return value / 3; }\\n"',
    '"int remainder(int value) { return value % 6; }\\n"',
    '"int negate(int value) { return -value; }\\n"',
    "six_function_source",
    '"int stage1(int value) { return value + 1; }\\n"',
    '"int stage6(int value) { return stage5(value) + 1; }\\n"',
    "six_function_definition_count != MAX_C_FUNCTIONS",
    "six_function_relocation_count != MAX_C_FUNCTIONS - 1",
    "six_function_view.symbol_count != 1 + MAX_C_FUNCTIONS",
    'six_function_linked, sizeof(six_function_linked), "stage6"',
    "compiled_stage6(36) != 42",
    "MAKOS_AARCH64_C_SIX_FUNCTION_OK functions=6 calls=5",
    "branch_assignment_source",
    "nested_manifest_source",
    "nested_asm_source",
    "nested_c_source",
    '"int choose(int value) { int result = 0; if (value > 5) { result = value + 2; } else { result = value - 2; } return result; }\\n"',
    '"int bump(int value) { int result = value; if (value < 5) { result = result + 1; } return result; }\\n"',
    '"int nested(int value) { int result = 0; if (value > 0) { if (value > 5)',
    '"int accumulate(int value) { int result = 0; while (value > 0) { if (value > 2)',
    "MAKOS_AARCH64_C_BRANCH_BLOCK_OK forms=if,if-else,nested-if,nested-loop",
    "compiled_choose(40) != 42",
    "compiled_choose(4) != 2",
    "compiled_bump(4) != 5",
    "compiled_bump(8) != 8",
    "compiled_nested(40) != 42",
    "compiled_nested(4) != 2",
    "compiled_accumulate(4) != 6",
    "malformed_empty_else_source",
    "malformed_branch_declaration_source",
    "malformed_block_depth_source",
    "relational_greater_source",
    "relational_at_most_source",
    "signed_pointer_offset_source",
    "pointer_difference_source",
    "malformed_duplicate_function_source",
    "malformed_too_many_functions_source",
    "compile_c(malformed_c_source",
    "static size_t emit_object(",
    "static size_t emit_object_definitions(",
    "static int parse_object(",
    "static size_t link_objects(",
    "MAX_LINK_OBJECTS = MAX_BUILD_INPUTS",
    "R_AARCH64_CALL26 = 283",
    "((uint64_t)relocation_symbols[index] << 32) | R_AARCH64_CALL26",
    "main_object[corrupt_info] = (uint8_t)(R_AARCH64_CALL26 - 1)",
    "main_object[corrupt_info] = saved_type",
    "invalid_emit.offset = main_code_length",
    "main_object[corrupt_addend] = 1",
    "main_object[corrupt_addend] = 0",
    "bases[object] > capacity",
    "addend != 0",
    "link_objects(objects, object_lengths, 2",
    "link_objects(objects, object_lengths, 4",
    "link_objects(duplicate_objects, duplicate_lengths, 3",
    "program_relocations[0].offset != 92",
    "program_definition_count != 2",
    "library_definition_count != 1",
    "helper_definition_count != 1",
    "program_definitions[0].size != 140",
    "program_definitions[1].offset != 140",
    "program_definitions[1].size != 168",
    "library_definitions[0].offset != 0",
    "helper_definitions[0].size != 56",
    "library_definitions[0].size != 60",
    "library_relocation_count != 0",
    "build.inputs[input].source_path",
    "build.inputs[1].object_path",
    "build.inputs[2].object_path",
    "build.output_path, build.output_path_length",
    "main_code_length, build.entry",
    "compiled_answer(20) != 42 || compiled_answer(0) != 86",
    "compiled_adjust(forty, 1) != 42 ||",
    "compiled_adjust(scaled, 2) != 44 ||",
    "compiled_adjust(zero, 1) != 2 ||",
    "zero[0] != 1 || zero[1] != 2 || zero[2] != 0",
    "compiled_greater(6) != 42 || compiled_greater(5) != 0",
    "compiled_at_most(5) != 42 || compiled_at_most(6) != 0",
    "compiled_previous(previous_values + 1, UINT32_MAX) != 42",
    "compiled_distance(distance_values + 3, distance_values) != 3",
    "compiled_combine(40, 2) != 42",
    "compiled_helper(40) != 42",
    "main_object_length != 688 || program_object_length != 976",
    "library_object_length != 616",
    "helper_object_length != 608",
    "linked_length != 500",
    "three_argument_code_length != 140",
    "three_argument_object_length != 752",
    "three_argument_linked_length != 140 || three_argument_entry != 80",
    "compiled_sum3(40, 1, 1) != 42 || compiled_invoke3(40) != 42",
    "signed_arithmetic_code_length != 168",
    "signed_arithmetic_object_length != 784",
    "signed_arithmetic_linked_length != 168 || signed_arithmetic_entry != 0",
    "compiled_divide(20) != 6",
    "compiled_remainder(20) != 2",
    "compiled_negate(UINT32_MAX - 41) != 42",
    "image_length != 1583",
    "format=elf64-et-rel",
    "persisted_reopened=1 manifest_input_bounds=2..6 malformed_build_denied=6 malformed_c_denied=21",
    "malformed_relocation_denied=1 unresolved_symbol_denied=1",
    "duplicate_definition_denied=1",
    "PF_R | PF_X",
    "deliberately NX",
    "PROT_READ | PROT_WRITE | PROT_EXEC",
    "MAKOS_AARCH64_LINKER_OK",
    "/home/user/generated.s",
    "/home/user/generated.build",
    "/home/user/generated-program.c",
    "/home/user/generated-library.c",
    "/home/user/generated-helper.c",
    "/home/user/generated-main.o",
    "/home/user/generated-program.o",
    "/home/user/generated-library.o",
    "/home/user/generated-helper.o",
    "/home/user/generated-aarch64.elf",
):
    require(TOOLCHAIN, fragment)

include_parser = TOOLCHAIN[
    TOOLCHAIN.index("static int expand_source_recursive(") :
    TOOLCHAIN.index("static size_t expand_build_source_mode(")
]
require(
    include_parser,
    "while (argument < line_end && source[argument] != terminator)",
)
if "while (!path_length && argument < line_end" in include_parser:
    raise AssertionError("quoted include parsing stops after the first path byte")
for include in (
    r'#include \"/home/user/generated-inline.h\"',
    r'#include \"/home/user/generated-leaf.h\"',
    "#include <stdint.h>",
):
    require(TOOLCHAIN, include)
if TOOLCHAIN.count(r'#include \"/home/user/generated-inline.h\"') < 2:
    raise AssertionError("repeated guarded quoted include fixture is absent")
seed_end = TOOLCHAIN.index("        fail(80);\n    if (fixture_mode) {")
readback = TOOLCHAIN.index("!verify_generated_header(", seed_end)
repository_marker = TOOLCHAIN.index("write_repository_source_marker();", readback)
if not seed_end < readback < repository_marker:
    raise AssertionError("generated-header readback is not immediate after seeding")

for function, following in (
    ("static int write_build_marker(", "static int write_build_output_marker("),
    ("static int write_build_output_marker(", "static int write_header_marker("),
    ("static int write_header_marker(", "static void write_repository_source_marker("),
):
    body = TOOLCHAIN[TOOLCHAIN.index(function) : TOOLCHAIN.index(following)]
    if body.count("write_record(&record)") != 1:
        raise AssertionError(f"{function} does not emit exactly one record")
    if "write_text(" in body or "write_bytes(" in body:
        raise AssertionError(f"{function} retains fragmented SYS_WRITE output")
for fragment in (
    'if (!write_build_marker("build", build_manifest_path,',
    "if (!write_build_output_marker(build_manifest_path,",
    "!write_header_marker(&build, input, &dependencies[input])",
    'if (!write_build_marker("fixture", build_manifest_path,',
):
    require(TOOLCHAIN, fragment)

for fragment in (
    "R_AARCH64_ADR_PREL_PG_HI21",
    "R_AARCH64_ADD_ABS_LO12_NC",
    "STT_OBJECT",
    r'\0.rodata\0',
    r'\0.data\0',
    "PF_W",
    "MAKOS_AARCH64_C_GLOBAL_DATA_OK",
    'source=/usr/src/makos/ports/musl/shared-demo.c',
    '"/usr/include/stdint.h"',
    '"/home/user/makos-shared-demo.build"',
    "/home/user/makos-shared-demo.elf",
    "static int validate_data_object_behavior(",
    "static int validate_final_data_elf(",
    "malformed_relocation_pair",
    "size_t pair_index = 0",
    "(pair_index + 1) * RELA_SIZE",
    "(low_info & UINT64_C(0xffffffff00000000)) | R_AARCH64_CALL26",
    "unresolved_data_symbol",
    "duplicate_data_definition",
    "out_of_range_data_symbol",
    "put16(image, 56, (uint16_t)(1 + (rodata_length != 0) +",
    '"/home/user/makos-shared-const.build"',
    '"/home/user/makos-shared-mutable.build"',
    "PRODUCTION_SHARED_DEMO_SOURCE_FNV1A",
    "SELFHOST_STDINT_SOURCE_FNV1A",
    "identity=guest-read-exact readonly_write=denied",
    "path_length, 1, 2",
    "path_length, 1, 1",
):
    require(TOOLCHAIN, fragment)

for fragment in (
    "fn validate_static_process_image",
    "file_end > bytes.len() as u64",
    "segment.flags & 3 == 3",
    "crate::vfs::snapshot(path, &mut image)?",
    "pub fn spawn_path(path: &[u8])",
    "pub fn spawn_path_with_arguments(path: &[u8], bytes: &[u8])",
    "startup.argv_offsets[argc..].iter().any",
    '"sysv-v1"',
    "pub fn spawn_toolchain(manifest_path: &[u8], fixture: bool)",
    "current_app_role() != ProcessRole::Shell",
    'manifest_path.starts_with(b"/home/user/")',
    'b"MODE=fixture"',
    'b"MODE=build"',
    'b"/system/aarch64-toolchain", manifest_path',
    "ProcessRole::Toolchain",
    "crate::security::SessionProcessRole::Toolchain",
    "discard_spawned(pid);",
    "reset_toolchain_smp_evidence()",
    "state.least_loaded_compute_ap()",
    ".rebalance_toolchain_on_timer(cpu, prior_pid)",
    "TOOLCHAIN_REBALANCE_DISPATCH_DELTA",
    "MAKOS_AARCH64_TOOLCHAIN_PLACEMENT_OK",
    "MAKOS_AARCH64_TOOLCHAIN_DISPATCH_OK",
    "MAKOS_AARCH64_TOOLCHAIN_MIGRATION_OK",
    "take_unreported_toolchain_migrations",
    "evidence_emitter=cpu0",
    "MAKOS_AARCH64_TOOLCHAIN_SMP_OK",
    "kernel_placement=least-dispatched-idle",
    "console_gpu_handoff=ap-defer,cpu0-compose",
    "crate::graphics::service_deferred_actions()",
    "crate::graphics::gpu_service_affinity_evidence()",
):
    require(PROCESS, fragment)

toolchain_spawn = PROCESS[
    PROCESS.index("pub fn spawn_toolchain(manifest_path: &[u8], fixture: bool)") :
    PROCESS.index("pub fn spawn_path(path: &[u8])")
]
for fragment in (
    "current_app_role() != ProcessRole::Shell",
    "crate::security::register_session_process(",
    "crate::security::SessionProcessRole::Toolchain",
    "discard_spawned(pid);",
):
    require(toolchain_spawn, fragment)

for fragment in (
    "const SYS_PROCESS_SPAWN_PATH: u64 = 56;",
    "const SYS_PROCESS_SPAWN_PATH_ARGS: u64 = 57;",
    "const ABI_FEATURE_SELF_HOSTING_SEED: u64 = 1 << 14;",
    "const ABI_FEATURE_EXEC_BY_PATH: u64 = 1 << 18;",
    "const ABI_FEATURE_PROCESS_STARTUP: u64 = 1 << 19;",
    "arguments_length != crate::aarch64_process::SPAWN_ARGUMENTS_BYTES",
    "crate::aarch64_process::spawn_path(path)",
    "16 if crate::aarch64_process::process_control_allowed()",
    "fixture > 1",
    "crate::aarch64_process::spawn_toolchain(path, fixture == 1)",
    "SYS_PROCESS_WAIT_STATUS =>",
    "ChildWaitStatus::NoChild => (-10i64) as u64",
    "ChildWaitStatus::Pending => 0",
    "status.saturating_add(1)",
):
    require(ARCH, fragment)

require(BUILD, "../user/aarch64_toolchain.c")
for fragment in (
    "../user/aarch64_selfhost_probe.c",
    "../user/aarch64_selfhost_probe.S",
    "../ports/musl/shared-demo.c",
    "../sdk/selfhost/include/stdint.h",
    "generate_aarch64_selfhost_sources(&manifest, &output_dir)",
    "fn generate_aarch64_selfhost_sources(",
    'output_dir.join("aarch64-selfhost-sources.inc")',
    'output_dir.join("aarch64-selfhost-reference.o")',
    "append_c_byte_array(",
    "fn fnv1a(",
    '"PRODUCTION_SHARED_DEMO_SOURCE"',
    '"SELFHOST_STDINT_SOURCE"',
):
    require(BUILD, fragment)
for fragment in (
    'path: b"/usr/src/makos/ports/musl/shared-demo.c"',
    'data: include_bytes!("../../ports/musl/shared-demo.c")',
    'path: b"/usr/include/stdint.h"',
    'data: include_bytes!("../../sdk/selfhost/include/stdint.h")',
    "mode: 0o100444",
    "node: 242",
    "inode: 12",
    "node: 243",
    "inode: 13",
    "metadata(\n        file.mode",
    "if writable",
):
    require(VFS, fragment)
system_table = VFS.split("static SYSTEM_FILES: [SystemFile; 5] = [", 1)[1].split("];", 1)[0]
system_nodes = [int(value) for value in re.findall(r"\n\s+node: (\d+),", system_table)]
system_inodes = [int(value) for value in re.findall(r"\n\s+inode: (\d+),", system_table)]
if len(system_nodes) != 5 or len(system_nodes) != len(set(system_nodes)):
    raise AssertionError("AArch64 system-file VFS nodes are not unique")
if len(system_inodes) != 5 or len(system_inodes) != len(set(system_inodes)):
    raise AssertionError("AArch64 system-file VFS inodes are not unique")
if set(system_nodes) & {244, 245, 246} or set(system_inodes) & {9, 10, 11}:
    raise AssertionError("self-host source VFS identities overlap reserved devices")
for fragment in (
    '#include "aarch64-selfhost-sources.inc"',
    "REPOSITORY_SELFHOST_C_SOURCE",
    "REPOSITORY_SELFHOST_ASM_SOURCE",
    "REPOSITORY_SELFHOST_C_SOURCE_FNV1A",
    "REPOSITORY_SELFHOST_ASM_SOURCE_FNV1A",
    "PRODUCTION_SHARED_DEMO_SOURCE_LENGTH",
    "PRODUCTION_SHARED_DEMO_SOURCE_FNV1A",
    "SELFHOST_STDINT_SOURCE_LENGTH",
    "SELFHOST_STDINT_SOURCE_FNV1A",
    "MAKOS_AARCH64_REPOSITORY_SOURCE_OK",
    '"/home/user/makos-repo-probe.build"',
    '"/home/user/makos-repo-probe.c"',
    '"/home/user/makos-repo-probe.s"',
    '"link /home/user/makos-repo-probe.elf _start\\n"',
    "identity=build-generated-exact host_reference=compiled",
):
    require(TOOLCHAIN, fragment)
for fragment in (
    "int makos_sum3(",
    "int makos_adjust(",
    "int makos_probe(",
    "return makos_adjust(values, 1);",
):
    require(REPOSITORY_C, fragment)
for fragment in ("_start:", "bl makos_probe", "mov x8, #5", "svc #0"):
    require(REPOSITORY_ASM, fragment)
for fragment in (
    "#include <stdint.h>",
    '__attribute__((visibility("default"))) const char makos_shared_name[]',
    '"libmakosdemo.so"',
    "makos_shared_add(uint64_t left, uint64_t right)",
    "return left + right;",
):
    require(PRODUCTION_C, fragment)
for fragment in (
    "#ifndef MAKOS_SELFHOST_STDINT_H",
    "typedef unsigned long uint64_t;",
):
    require(SELFHOST_STDINT, fragment)
require(TOOLCHAIN, "parameter_pointers")
require(TOOLCHAIN, "compiler->parameter_pointers[parameter]")
require(TOOLCHAIN, "static int c_pointer_expression(")
require(TOOLCHAIN, "static int c_emit_pointer_value(")
require(TOOLCHAIN, "local.pointer_bound = local.array_length")
require(TOOLCHAIN, '"        return adjust(values + 1, 1);\\n"')
require(TOOLCHAIN, '"        *(pointer + delta) = pointer[0] + count + 1;\\n"')
require(SHELL, "MAKOS_AARCH64_SELFHOST_LINK_OK")
require(SHELL, "static uint64_t spawn_makbuild_process(")
require(SHELL, "static uint64_t wait_for_process(")
require(SHELL, "SYS_PROCESS_WAIT_STATUS = 126")
require(SHELL, "syscall4(SYS_PROCESS_WAIT_STATUS, pid, 1, 0, 0)")
require(SHELL, "result == (uint64_t)(int64_t)-10")
require(SHELL, "static void report_makbuild_status(")
require(SHELL, "MAKBUILD_PATH_BYTES = 96")
require(SHELL, "PARALLEL_MAKBUILD_COUNT = 3")
require(SHELL, "static void run_makbuild(")
require(SHELL, "static void run_makbuild_parallel(void)")
require(SHELL, "static void run_path(")
require(SHELL, '"makbuild "')
require(SHELL, '"makbuild-parallel"')
require(SHELL, '"run "')
require(SHELL, "MAKOS_AARCH64_MAKBUILD_CLI_OK")
require(
    SHELL,
    "MAKOS_AARCH64_MAKBUILD_PARALLEL_OK \"\n"
    "                   \"spawn_before_wait=3 statuses=42,42,42",
)
for manifest in (
    "/home/user/generated-three.build",
    "/home/user/generated-header.build",
    "/home/user/generated-nested.build",
):
    require(SHELL, manifest)
parallel_shell = SHELL[
    SHELL.index("static void run_makbuild_parallel(void)") :
    SHELL.index("static void run_path(")
]
if parallel_shell.index("pids[index] = spawn_makbuild_process(") >= parallel_shell.index(
    "statuses[index] = wait_for_process(pids[index])"
):
    raise AssertionError("parallel makbuild waits before completing all spawns")
if parallel_shell.index("statuses[index] = wait_for_process(pids[index])") >= parallel_shell.index(
    "No Toolchain child can interleave output"
):
    raise AssertionError("parallel makbuild reports before completing all waits")
require(parallel_shell, "if (pids[index] != UINT64_MAX)")
require(parallel_shell, "No Toolchain child can interleave output")
require(parallel_shell, "index < PARALLEL_MAKBUILD_COUNT")
require(parallel_shell, "statuses[0] == 42 && statuses[1] == 42 &&")
require(parallel_shell, "statuses[2] == 42")
status_report = SHELL[
    SHELL.index("static void report_makbuild_status(") :
    SHELL.index("static void run_makbuild(")
]
if status_report.count("write_bytes(record, used)") != 1:
    raise AssertionError("makbuild CLI record is not one atomic write")
require(SHELL, "MAKOS_AARCH64_RUN_OK")
require(SHELL, "source=existing-makfs seeded=0 startup=sysv status=42")
require(SHELL, "toolchain_startup=sysv manifest_arg=1")
require(SHELL, "build_inputs=4 cache=makstate-v2")
require(SHELL, "cache_hits=0 cache_misses=4 state_committed=1")
require(SHELL, "SYS_PROCESS_SPAWN_PATH")
require(SHELL, "SYS_PROCESS_SPAWN_PATH_ARGS")
require(SHELL, "malformed.argv_offsets[7] = 1")
require(SHELL, "sizeof(startup) - 1")
require(SHELL, "linker=guest-native objects=4")
require(SHELL, "object_format=elf64-et-rel")
require(SHELL, "languages=aarch64-asm,c-subset-v1 compiler=guest-native")
require(SHELL, "object_format=elf64-et-rel relocations=R_AARCH64_CALL26:3")
require(SHELL, "symbols=_start,answer,adjust,combine,helper")
require(SHELL, "build_manifest=/home/user/generated.build")
require(SHELL, "build_driver=makbuild-v1 build_inputs=4")
require(SHELL, "translation_unit_functions=2,1,1")
require(SHELL, "c_abi=aapcs64-int32-pointer64")
require(SHELL, "c_features=multi-function,multi-parameter,six-argument,signed-arithmetic,parameter,pointer-parameter,local,array,array-decay,index,assignment,pointer,pointer-add,pointer-variable-add,pointer-difference,address-of,address-expression,dereference,if,if-assignment,if-else,nested-control,equality,inequality,relational,while,call,return")
require(SHELL, "max_parameters=6 max_call_arguments=6 nonleaf_frame=96,112")
require(SHELL, "three_argument_result=42 three_argument_link=et-rel,same-object")
require(SHELL, "six_argument_result=42 six_argument_link=et-rel,same-object")
require(SHELL, "c_operators=mul,sdiv,srem,neg,sub,add")
require(SHELL, "signed_division_results=20:6,-20:-6")
require(SHELL, "signed_remainder_results=20:2,-20:-2")
require(SHELL, "unary_negation_results=42:-42,-42:42")
require(SHELL, "arithmetic_object=elf64-et-rel:784")
require(SHELL, "branch_results=42,86 loop_results=42,2 memory_results=42,2")
require(SHELL, "pointer_call=answer-to-adjust pointee_results=42,44,2")
require(SHELL, "delta_results=1:42,2:44,1:2")
require(SHELL, "array_results=41:42:0,42:0:44,1:2:0 pointer_offset_call=1")
require(SHELL, "pointer_variable_offset=delta dynamic_pointer_adds=2")
require(SHELL, "signed_pointer_offset=-1:42 signed_pointer_difference=3:-3")
require(SHELL, "relational_results=gt:42:0,le:42:0,ge:42:86,lt:42:44")
require(SHELL, "code_bytes=76,140,168,60,56 object_bytes=688,976,616,608")
require(SHELL, "intra_object_calls=1 cross_object_calls=2 linked_bytes=500")
require(SHELL, "output_bytes=1583 helper_result=42 persisted_reopened=1")
require(SHELL, "malformed_c_denied=21")
require(SHELL, "manifest_input_bounds=2..6 malformed_build_denied=6")
require(SHELL, "malformed_relocation_denied=1")
require(SHELL, "unresolved_symbol_denied=1 duplicate_definition_denied=1")
require(SHELL, "output=elf64-aarch64 kernel_loader=validated abi56=1 abi57=1")
require(SHELL, "argv=3 env=1 malformed_startup_denied=3 executed=2 status=42")
require(PROCESS, "SessionProcessRole::Toolchain")
require(SECURITY, "SessionProcessRole::Toolchain => CAP_CONSOLE | CAP_FILE_WRITE")
require(RUNTIME, 'send_command(stream, "selfhost-aarch64")')
require(RUNTIME, 'send_command(stream, "makbuild /home/user/generated.build")')
require(RUNTIME, "MAKOS_AARCH64_MAKBUILD_OK mode=build")
require(RUNTIME, "MAKOS_AARCH64_MAKBUILD_CLI_OK")
require(RUNTIME, '"{": "shift-bracket_left"')
require(RUNTIME, '"}": "shift-bracket_right"')
require(RUNTIME, "MAKOS_AARCH64_SELFHOST_LINK_OK")
require(FOCUSED_RUNTIME, "MAKOS_AARCH64_LINKER_OK")
require(FOCUSED_RUNTIME, "MAKOS_AARCH64_SELFHOST_LINK_OK")
require(FOCUSED_RUNTIME, "SIX_FUNCTION_MARKER")
require(FOCUSED_RUNTIME, "max_functions_per_unit=6 six_function_calls=5 six_function_result=42")
require(FOCUSED_RUNTIME, "branch_blocks=if,if-else,nested-if,nested-loop branch_block_body=bounded-control-assignment branch_block_max_depth=4")
require(FOCUSED_RUNTIME, "branch_block_results=42,2,5,8,42,2,1,6 branch_block_object=elf64-et-rel")
require(FOCUSED_RUNTIME, "malformed_branch_blocks=empty-else,branch-declaration-denied,depth-5-denied")
require(FOCUSED_RUNTIME, "SIX_ARGUMENT_MARKER")
require(FOCUSED_RUNTIME, "BRANCH_BLOCK_MARKER")
require(FOCUSED_RUNTIME, "max_parameters=6 max_call_arguments=6 nonleaf_frame=96,112")
require(FOCUSED_RUNTIME, "six_argument_object=elf64-et-rel:808")
require(FOCUSED_RUNTIME, "FIXTURE_BUILD_MARKER")
require(FOCUSED_RUNTIME, "WARM_BUILD_MARKER")
require(FOCUSED_RUNTIME, "SELECTIVE_BUILD_MARKER")
require(FOCUSED_RUNTIME, "INVALIDATED_BUILD_MARKER")
require(FOCUSED_RUNTIME, "THREE_INPUT_COLD_MARKER")
require(FOCUSED_RUNTIME, "THREE_INPUT_WARM_MARKER")
require(FOCUSED_RUNTIME, "HEADER_COLD_MARKER")
require(FOCUSED_RUNTIME, "HEADER_WARM_MARKER")
require(FOCUSED_RUNTIME, "HEADER_SELECTIVE_MARKER")
require(FOCUSED_RUNTIME, "HEADER_DEP_MARKER")
require(FOCUSED_RUNTIME, "HEADER_GUARD_MARKER")
require(FOCUSED_RUNTIME, "HEADER_RUN_MARKER")
require(FOCUSED_RUNTIME, "GENERATED_HEADERS_MARKER")
require(FOCUSED_RUNTIME, "identity=guest-readback-exact")
require(FOCUSED_RUNTIME, "REPOSITORY_SOURCE_MARKER")
require(FOCUSED_RUNTIME, "REPOSITORY_COLD_MARKER")
require(FOCUSED_RUNTIME, "REPOSITORY_WARM_MARKER")
require(FOCUSED_RUNTIME, "REPOSITORY_CLI_REAP_MARKER")
require(FOCUSED_RUNTIME, "REPOSITORY_RUN_MARKER")
require(FOCUSED_RUNTIME, "GLOBAL_DATA_MARKER")
require(FOCUSED_RUNTIME, "GLOBAL_DATA_COLD_MARKER")
require(FOCUSED_RUNTIME, "GLOBAL_DATA_WARM_MARKER")
require(FOCUSED_RUNTIME, "GLOBAL_DATA_CLI_REAP_MARKER")
require(FOCUSED_RUNTIME, "GLOBAL_DATA_RUN_MARKER")
require(FOCUSED_RUNTIME, "TOOLCHAIN_SMP_MARKER")
require(FOCUSED_RUNTIME, "TOOLCHAIN_PARALLEL_MARKER")
require(FOCUSED_RUNTIME, "TOOLCHAIN_PROCESS_COUNT = 21")
require(FOCUSED_RUNTIME, "def validate_parallel_makbuild(")
require(FOCUSED_RUNTIME, "expected one parallel Toolchain overlap record")
require(FOCUSED_RUNTIME, "group_pids != pids")
require(FOCUSED_RUNTIME, "len(set(roots)) != 3")
require(FOCUSED_RUNTIME, "parallel TTBR0 mismatch for pid")
require(FOCUSED_RUNTIME, "parallel initial placement affinity mismatch for pid")
require(FOCUSED_RUNTIME, "parallel migration chain source mismatch for pid")
require(FOCUSED_RUNTIME, "invalid parallel migration hop for pid")
require(FOCUSED_RUNTIME, "parallel snapshot CPUs were never present in migration history")
require(FOCUSED_RUNTIME, "parallel Toolchain child reached reap/SMP summary")
require(FOCUSED_RUNTIME, "placement/process spawn records")
require(FOCUSED_RUNTIME, "segment.count(TOOLCHAIN_SMP_MARKER) != 3")
require(FOCUSED_RUNTIME, "def validate_toolchain_smp(")
require(FOCUSED_RUNTIME, "expected {TOOLCHAIN_PROCESS_COUNT} toolchain placements")
require(FOCUSED_RUNTIME, "toolchain placement was not least-loaded")
require(FOCUSED_RUNTIME, "no automatic Toolchain migration was observed")
require(FOCUSED_RUNTIME, "invalid automatic Toolchain migration")
require(FOCUSED_RUNTIME, "migration_policy=timer-safe-dispatch-imbalance")
require(FOCUSED_RUNTIME, "migration_count != len(migrations)")
require(FOCUSED_RUNTIME, "migration_evidence_drops != 0")
require(FOCUSED_RUNTIME, "cpu_mask != 0xE")
require(FOCUSED_RUNTIME, "dispatched_cpus != {1, 2, 3}")
require(FOCUSED_RUNTIME, "CLI_REAP_MARKER")
require(FOCUSED_RUNTIME, "THREE_CLI_REAP_MARKER")
require(FOCUSED_RUNTIME, "cache_hits=4 cache_misses=0")
require(FOCUSED_RUNTIME, "cache_hits=3 cache_misses=1")
require(FOCUSED_RUNTIME, "cache_hits=0 cache_misses=4")
require(FOCUSED_RUNTIME, "write generated-library.o corrupt")
require(FOCUSED_RUNTIME, "write generated-library.c")
require(FOCUSED_RUNTIME, "write generated.build.state corrupt")
require(FOCUSED_RUNTIME, 'send_command(stream, "makbuild-parallel")')
require(FOCUSED_RUNTIME, "makbuild /home/user/generated-three.build")
require(FOCUSED_RUNTIME, "makbuild /home/user/generated-header.build")
require(FOCUSED_RUNTIME, "write generated-leaf.h")
require(FOCUSED_RUNTIME, "run generated-header.elf")
require(FOCUSED_RUNTIME, "makbuild /home/user/generated-nested.build")
require(FOCUSED_RUNTIME, "run generated-nested.elf")
require(FOCUSED_RUNTIME, "makbuild /home/user/makos-repo-probe.build")
require(FOCUSED_RUNTIME, "run makos-repo-probe.elf")
require(FOCUSED_RUNTIME, "makbuild /home/user/makos-shared-demo.build")
require(FOCUSED_RUNTIME, "run makos-shared-demo.elf")
require(FOCUSED_RUNTIME, "makbuild /home/user/makos-shared-const.build")
require(FOCUSED_RUNTIME, "run makos-shared-const.elf")
require(FOCUSED_RUNTIME, "makbuild /home/user/makos-shared-mutable.build")
require(FOCUSED_RUNTIME, "run makos-shared-mutable.elf")
require(FOCUSED_RUNTIME, "cli_builds=20")
require(
    FOCUSED_RUNTIME,
    "parallel_builds=3 parallel_commands=1 spawn_before_wait=3 "
    "command=post-login-fixed-manifests",
)
if FOCUSED_RUNTIME.count('"makbuild /home/user/generated-three.build"') != 1:
    raise AssertionError("generated-three cold build was not replaced exactly once")
if FOCUSED_RUNTIME.count('"makbuild /home/user/generated-header.build"') != 3:
    raise AssertionError("generated-header cold build was not replaced exactly once")
if FOCUSED_RUNTIME.count('"makbuild /home/user/generated-nested.build"') != 1:
    raise AssertionError("generated-nested cold build was not replaced exactly once")
require(FOCUSED_RUNTIME, "toolchain_smp=kernel-least-loaded-ap cpu_mask=0xe")
require(FOCUSED_RUNTIME, "console_gpu_handoff=ap-defer,cpu0-compose")
require(FOCUSED_RUNTIME, "owner_composes == 0")
require(FOCUSED_RUNTIME, "ap_deferrals == 0")
require(FOCUSED_RUNTIME, "runtime_graphs=4,3,2,2,3,2,2,3")
require(FOCUSED_RUNTIME, "global-data-cold:0/3,global-data-warm:3/0")
require(FOCUSED_RUNTIME, "const-only-cold:0/2,mutable-only-cold:0/2")
require(FOCUSED_RUNTIME, "nested-cold:0/3,nested-warm:3/0")
require(FOCUSED_RUNTIME, "validate_nested_build_output")
require(FOCUSED_RUNTIME, "linked_bytes <= 512")
require(FOCUSED_RUNTIME, "nested_build=authenticated-makfs")
require(FOCUSED_RUNTIME, "linked_capacity=1024 output_bytes=1583 image_capacity=2048 data_offset=1536 control=while-to-if-else execution=42")
require(FOCUSED_RUNTIME, "validate_gpu_recovery")
require(FOCUSED_RUNTIME, "gpu_completion=fast-plus-bounded-recovery")
require(FOCUSED_RUNTIME, "identity=build-generated-exact host_reference=compiled guest_execution=42")
require(FOCUSED_RUNTIME, "production_source=/usr/src/makos/ports/musl/shared-demo.c")
require(FOCUSED_RUNTIME, "source_identity=exact-read-only")
require(FOCUSED_RUNTIME, "header=/usr/include/stdint.h")
require(FOCUSED_RUNTIME, "global_data=rodata,rwdata segments=R-X,R--,RW-NX execution=42")
require(FOCUSED_RUNTIME, "invalidations=object,source,state,header")
require(FOCUSED_RUNTIME, "header_dependency=quoted-absolute-recursive headers=2 max_depth=2 depth_limit=4 preprocessor=bounded-macro-if-expressions macros=6 conditional_depth=2 macro_expansion=text,function-like parameters=4 expansion_depth=8 if_expression=defined,numeric,arithmetic,shift,comparison,bitwise,not,and,or,short-circuit,conditional elif=selected include_guard=deduplicated fingerprint=expanded-source")
require(FOCUSED_RUNTIME, "malformed_headers=missing,relative,cycle,overdepth-denied malformed_preprocessor=define,endif,unterminated,duplicate-else,expression,elif-after-else,zero-divisor,shift-range,overflow,conditional-syntax,conditional-selected-trap,macro-parameters,macro-arity,macro-recursion,macro-token-op-denied transitive_header_execution=42")
require(FOCUSED_RUNTIME, "malformed_c_denied=21")
require(FOCUSED_RUNTIME, "manifest_input_bounds=2..6 malformed_build_denied=6")
require(FOCUSED_RUNTIME, "malformed_relocation_denied=1 unresolved_symbol_denied=1")
require(FOCUSED_RUNTIME, "duplicate_definition_denied=1")
require(FOCUSED_RUNTIME, "executed=2 status=42")

print("AArch64 guest self-hosting structural test passed")
