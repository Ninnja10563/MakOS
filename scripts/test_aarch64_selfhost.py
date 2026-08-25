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
    '"    pointer[0] = pointer[0] + delta;\\n"',
    '"    int distance = next - pointer;\\n"',
    '"    int count = 0;\\n"',
    '"    while (count < distance) {\\n"',
    '"        *(pointer + delta) = pointer[0] + count + 1;\\n"',
    '"        count = count + 1;\\n"',
    '"    return *next;\\n"',
    "static size_t assemble(",
    "static size_t compile_c(",
    "static size_t compile_c_unit(",
    "static int c_compile_function(",
    "static int c_find_parameter(",
    "static int c_variable_register(",
    "static int c_pointer_register(",
    "static int c_pointer_index_register(",
    "static int c_call_argument(",
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
    "MAX_C_FUNCTIONS = 2",
    "MAX_C_PARAMETERS = 2",
    "UINT32_C(0x1b007c00)",
    "UINT32_C(0x0b000000)",
    "UINT32_C(0x4b000000)",
    "UINT32_C(0x52800000)",
    "UINT32_C(0xa9ba7bfd)",
    "UINT32_C(0xa8c67bfd)",
    "UINT32_C(0x2a0003e0)",
    "UINT32_C(0xa90363f7)",
    "UINT32_C(0xa94363f7)",
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
    "relational_greater_source",
    "relational_at_most_source",
    "signed_pointer_offset_source",
    "pointer_difference_source",
    "malformed_duplicate_function_source",
    "compile_c(malformed_c_source",
    "static size_t emit_object(",
    "static size_t emit_object_definitions(",
    "static int parse_object(",
    "static size_t link_objects(",
    "MAX_LINK_OBJECTS = 3",
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
    "link_objects(duplicate_objects, duplicate_lengths, 3",
    "program_relocations[0].offset != 92",
    "program_definition_count != 2",
    "program_definitions[0].size != 140",
    "program_definitions[1].offset != 140",
    "program_definitions[1].size != 168",
    "compiled_answer(20) != 42 || compiled_answer(0) != 86",
    "compiled_adjust(forty, 1) != 42 ||",
    "compiled_adjust(scaled, 2) != 44 ||",
    "compiled_adjust(zero, 1) != 2 ||",
    "zero[0] != 1 || zero[1] != 2 || zero[2] != 0",
    "compiled_greater(6) != 42 || compiled_greater(5) != 0",
    "compiled_at_most(5) != 42 || compiled_at_most(6) != 0",
    "compiled_previous(previous_values + 1, UINT32_MAX) != 42",
    "compiled_distance(distance_values + 3, distance_values) != 3",
    "main_object_length != 688 || program_object_length != 920",
    "linked_length != 384",
    "image_length != 815",
    "format=elf64-et-rel",
    "persisted_reopened=1 malformed_c_denied=16",
    "malformed_relocation_denied=1 unresolved_symbol_denied=1",
    "duplicate_definition_denied=1",
    "PF_R | PF_X",
    "deliberately NX",
    "PROT_READ | PROT_WRITE | PROT_EXEC",
    "MAKOS_AARCH64_LINKER_OK",
    "/home/user/generated.s",
    "/home/user/generated-program.c",
    "/home/user/generated-main.o",
    "/home/user/generated-program.o",
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
    "16 => crate::aarch64_process::spawn_toolchain()",
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
require(SHELL, "SYS_PROCESS_SPAWN_PATH")
require(SHELL, "SYS_PROCESS_SPAWN_PATH_ARGS")
require(SHELL, "malformed.argv_offsets[7] = 1")
require(SHELL, "sizeof(startup) - 1")
require(SHELL, "objects=2 object_format=elf64-et-rel")
require(SHELL, "languages=aarch64-asm,c-subset-v1 compiler=guest-native")
require(SHELL, "relocations=R_AARCH64_CALL26:2 symbols=_start,answer,adjust")
require(SHELL, "translation_unit_functions=2")
require(SHELL, "c_abi=aapcs64-int32-pointer64")
require(SHELL, "c_features=multi-function,multi-parameter,parameter,pointer-parameter,local,array,array-decay,index,assignment,pointer,pointer-add,pointer-variable-add,pointer-difference,address-of,address-expression,dereference,if,equality,inequality,relational,while,call,return max_parameters=2 max_call_arguments=2 nonleaf_frame=96")
require(SHELL, "c_operators=mul,sub,add c_relations=eq,ne,lt,le,gt,ge branch_results=42,86")
require(SHELL, "loop_results=42,2 memory_results=42,2 pointer_call=answer-to-adjust pointee_results=42,44,2 delta_results=1:42,2:44,1:2 array_results=41:42:0,42:0:44,1:2:0 pointer_offset_call=1 pointer_variable_offset=delta dynamic_pointer_adds=2 signed_pointer_offset=-1:42 signed_pointer_difference=3:-3 relational_results=gt:42:0,le:42:0,ge:42:86,lt:42:44 code_bytes=76,140,168 object_bytes=688,920 intra_object_call=1 linked_bytes=384 output_bytes=815")
require(SHELL, "malformed_c_denied=16")
require(SHELL, "malformed_relocation_denied=1 unresolved_symbol_denied=1 duplicate_definition_denied=1")
require(SHELL, "abi56=1 abi57=1 argv=3 env=1 malformed_startup_denied=3")
require(PROCESS, "SessionProcessRole::Toolchain")
require(SECURITY, "SessionProcessRole::Toolchain => CAP_CONSOLE | CAP_FILE_WRITE")
require(RUNTIME, 'send_command(stream, "selfhost-aarch64")')
require(RUNTIME, "MAKOS_AARCH64_SELFHOST_LINK_OK")
require(FOCUSED_RUNTIME, "MAKOS_AARCH64_LINKER_OK")
require(FOCUSED_RUNTIME, "MAKOS_AARCH64_SELFHOST_LINK_OK")
require(FOCUSED_RUNTIME, "malformed_c_denied=16")
require(FOCUSED_RUNTIME, "malformed_relocation_denied=1 unresolved_symbol_denied=1")
require(FOCUSED_RUNTIME, "duplicate_definition_denied=1")
require(FOCUSED_RUNTIME, "executed=2 status=42")

print("AArch64 guest self-hosting structural test passed")
