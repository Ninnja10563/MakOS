#!/usr/bin/env python3
"""Structural guard for guest-native multi-function AArch64 compilation/linking."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TOOLCHAIN = (ROOT / "user/aarch64_toolchain.c").read_text()
SHELL = (ROOT / "user/aarch64_shell.c").read_text()
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
SECURITY = (ROOT / "kernel/src/security.rs").read_text()
BUILD = (ROOT / "kernel/build.rs").read_text()
RUNTIME = (ROOT / "scripts/boot_test_aarch64.py").read_text()
FOCUSED_RUNTIME = (ROOT / "scripts/boot_test_aarch64_selfhost.py").read_text()


def require(source: str, fragment: str) -> None:
    if fragment not in source:
        raise AssertionError(f"missing self-hosting guard: {fragment}")


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
    "static int c_if_return(",
    "static int c_while(",
    "MAX_C_LOCALS = 4",
    "OBJECT_CAPACITY = 2048",
    "MAX_C_FUNCTIONS = 6",
    "MAX_C_PARAMETERS = 6",
    "MAX_RELOCATIONS = 8",
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
    "image_length != 815",
    "format=elf64-et-rel",
    "persisted_reopened=1 manifest_input_bounds=2..6 malformed_build_denied=6 malformed_c_denied=18",
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
    'manifest_path.starts_with(b"/home/user/")',
    'b"MODE=fixture"',
    'b"MODE=build"',
    'b"/system/aarch64-toolchain", manifest_path',
):
    require(PROCESS, fragment)

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
):
    require(ARCH, fragment)

require(BUILD, "../user/aarch64_toolchain.c")
require(TOOLCHAIN, "parameter_pointers")
require(TOOLCHAIN, "compiler->parameter_pointers[parameter]")
require(TOOLCHAIN, "static int c_pointer_expression(")
require(TOOLCHAIN, "static int c_emit_pointer_value(")
require(TOOLCHAIN, "local.pointer_bound = local.array_length")
require(TOOLCHAIN, '"        return adjust(values + 1, 1);\\n"')
require(TOOLCHAIN, '"        *(pointer + delta) = pointer[0] + count + 1;\\n"')
require(SHELL, "MAKOS_AARCH64_SELFHOST_LINK_OK")
require(SHELL, "static void run_makbuild(")
require(SHELL, '"makbuild "')
require(SHELL, "MAKOS_AARCH64_MAKBUILD_CLI_OK")
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
require(SHELL, "c_features=multi-function,multi-parameter,six-argument,signed-arithmetic,parameter,pointer-parameter,local,array,array-decay,index,assignment,pointer,pointer-add,pointer-variable-add,pointer-difference,address-of,address-expression,dereference,if,equality,inequality,relational,while,call,return")
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
require(SHELL, "output_bytes=815 helper_result=42 persisted_reopened=1")
require(SHELL, "malformed_c_denied=18")
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
require(FOCUSED_RUNTIME, "SIX_ARGUMENT_MARKER")
require(FOCUSED_RUNTIME, "max_parameters=6 max_call_arguments=6 nonleaf_frame=96,112")
require(FOCUSED_RUNTIME, "six_argument_object=elf64-et-rel:808")
require(FOCUSED_RUNTIME, "FIXTURE_BUILD_MARKER")
require(FOCUSED_RUNTIME, "WARM_BUILD_MARKER")
require(FOCUSED_RUNTIME, "SELECTIVE_BUILD_MARKER")
require(FOCUSED_RUNTIME, "INVALIDATED_BUILD_MARKER")
require(FOCUSED_RUNTIME, "THREE_INPUT_COLD_MARKER")
require(FOCUSED_RUNTIME, "THREE_INPUT_WARM_MARKER")
require(FOCUSED_RUNTIME, "CLI_REAP_MARKER")
require(FOCUSED_RUNTIME, "THREE_CLI_REAP_MARKER")
require(FOCUSED_RUNTIME, "cache_hits=4 cache_misses=0")
require(FOCUSED_RUNTIME, "cache_hits=3 cache_misses=1")
require(FOCUSED_RUNTIME, "cache_hits=0 cache_misses=4")
require(FOCUSED_RUNTIME, "write generated-library.o corrupt")
require(FOCUSED_RUNTIME, "write generated-library.c")
require(FOCUSED_RUNTIME, "write generated.build.state corrupt")
require(FOCUSED_RUNTIME, "makbuild /home/user/generated-three.build")
require(FOCUSED_RUNTIME, "runtime_graphs=4,3")
require(FOCUSED_RUNTIME, "malformed_c_denied=18")
require(FOCUSED_RUNTIME, "manifest_input_bounds=2..6 malformed_build_denied=6")
require(FOCUSED_RUNTIME, "malformed_relocation_denied=1 unresolved_symbol_denied=1")
require(FOCUSED_RUNTIME, "duplicate_definition_denied=1")
require(FOCUSED_RUNTIME, "executed=2 status=42")

print("AArch64 guest self-hosting structural test passed")
