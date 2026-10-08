//! Host memory/state adapters for the exact production EL0-entry predicates.
#![allow(dead_code)]

include!("arch_policy.rs");

// No address at this sentinel is ever dereferenced: kernel-root validation
// must reject it before the page-table walk.
fn kernel_root() -> u64 {
    0x1000
}

thread_local! {
    static ACTIVE_ROOT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CPU_PFR1: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
fn cached_active_root() -> u64 {
    ACTIVE_ROOT.get()
}
fn user_cpu_features() -> u64 {
    CPU_PFR1.get()
}

mod arch {
    pub const USER_ADDRESS_BASE: u64 = super::USER_ADDRESS_BASE;
    pub const USER_MMAP_LIMIT: u64 = super::USER_MMAP_LIMIT;
    pub const USER_STACK_TOP: u64 = super::USER_STACK_TOP;
    pub fn kernel_root() -> u64 {
        super::kernel_root()
    }
}

mod aarch64_vm {
    use makos_vm_space::RegionTable;
    use std::cell::{Cell, RefCell};
    include!("vm_policy.rs");

    struct VmState {
        processes: [ProcessVm; 4],
        regions: RegionTable<16>,
    }
    thread_local! {
        static STATE: RefCell<VmState> = RefCell::new(VmState {
            processes: [ProcessVm::EMPTY; 4],
            regions: RegionTable::new(),
        });
        static QUERIES: Cell<usize> = const { Cell::new(0) };
    }
    fn with_state<R>(function: impl FnOnce(&mut VmState) -> R) -> R {
        QUERIES.set(QUERIES.get() + 1);
        STATE.with_borrow_mut(function)
    }
    pub fn reset() {
        STATE.with_borrow_mut(|state| {
            state.processes = [ProcessVm::EMPTY; 4];
            state.regions.clear();
        });
        QUERIES.set(0);
    }
    pub fn reserve(pid: u64, root: u64, address: u64, protection: u8) {
        STATE.with_borrow_mut(|state| {
            let index = (pid - 1) as usize;
            state.processes[index] = ProcessVm {
                pid,
                root,
                generation: 0,
                break_base: 0,
                current_break: 0,
            };
            assert!(state.regions.allocate_fixed(
                pid,
                address & !4095,
                1,
                protection,
                super::USER_ADDRESS_BASE,
                super::USER_STACK_TOP,
                4096
            ));
        });
    }
    pub fn queries() -> usize {
        QUERIES.get()
    }
}

#[repr(align(4096))]
struct HostTable([u64; 512]);

struct Tables {
    pages: Vec<Box<HostTable>>,
}

impl Tables {
    fn new() -> Self {
        aarch64_vm::reset();
        CPU_PFR1.set(0);
        Self {
            pages: vec![Box::new(HostTable([0; 512]))],
        }
    }
    fn root(&self) -> u64 {
        self.pages[0].0.as_ptr() as u64
    }
    fn slots(&mut self, address: u64) -> [*mut u64; 4] {
        let mut table = self.root();
        let mut slots = [std::ptr::null_mut(); 4];
        for (level, shift) in [39, 30, 21, 12].into_iter().enumerate() {
            let slot = unsafe { (table as *mut u64).add(((address >> shift) & 511) as usize) };
            slots[level] = slot;
            if level < 3 {
                if unsafe { slot.read_volatile() } == 0 {
                    let page = Box::new(HostTable([0; 512]));
                    let physical = page.0.as_ptr() as u64;
                    assert_eq!(physical & ADDRESS_MASK, physical);
                    unsafe { slot.write_volatile(physical | TABLE_DESCRIPTOR) };
                    self.pages.push(page);
                }
                table = unsafe { slot.read_volatile() } & ADDRESS_MASK;
            }
        }
        slots
    }
    fn leaf(&mut self, address: u64, descriptor: u64) {
        let slot = self.slots(address)[3];
        unsafe { slot.write_volatile(descriptor) };
    }
    fn context(&mut self, address: u64) -> UserContext {
        self.leaf(address, executable_leaf());
        let stack = 0x9000_3000;
        self.leaf(
            stack - 1,
            0x1000 | TABLE_DESCRIPTOR | AP_USER_RW | ACCESS_FLAG | PXN | UXN,
        );
        UserContext {
            registers: [0; 31],
            elr: address,
            spsr: 0,
            esr: 0,
            far: 0,
            sp_el0: stack,
            ttbr0: self.root(),
            tpidr_el0: 0,
            vector_registers: [0; 32],
            fpcr: 0,
            fpsr: 0,
        }
    }
}

fn executable_leaf() -> u64 {
    0x2000 | TABLE_DESCRIPTOR | AP_USER_RO | ACCESS_FLAG | PXN
}

#[test]
fn loader_thread_entry_uses_executable_mapping() {
    let mut tables = Tables::new();
    let context = tables.context(0x280a_dc14);
    assert!(
        user_context_entry_valid(&context, tables.root()),
        "legitimate dynamic-loader thread entry rejected"
    );
    assert!(user_address_executable_in(tables.root(), context.elr));
}

#[test]
fn low_high_and_last_user_instructions_are_allowed() {
    for address in [
        USER_ADDRESS_BASE,
        USER_IMAGE_LIMIT,
        0x280a_dc14,
        USER_MMAP_BASE + 0x1004,
        USER_ADDRESS_LIMIT - 4,
    ] {
        let mut tables = Tables::new();
        let context = tables.context(address);
        assert!(
            user_context_entry_valid(&context, tables.root()),
            "pc={address:#x}"
        );
    }
}

#[test]
fn pc_alignment_and_user_bounds_are_mandatory() {
    let mut tables = Tables::new();
    let mut context = tables.context(0x280a_dc14);
    for address in [
        0,
        USER_ADDRESS_BASE - 4,
        USER_ADDRESS_LIMIT,
        USER_ADDRESS_LIMIT + 4,
        u64::MAX,
        0x280a_dc15,
        0x280a_dc16,
        0x280a_dc17,
    ] {
        context.elr = address;
        assert!(
            !user_context_entry_valid(&context, tables.root()),
            "pc={address:#x}"
        );
        assert_eq!(
            user_instruction_mapping_in(tables.root(), address),
            UserInstructionMapping::Denied
        );
    }
}

#[test]
fn leaf_permissions_cannot_be_relaxed_by_rx_reservation() {
    let address = 0x280a_dc14;
    let denied = [
        executable_leaf() | UXN,
        (executable_leaf() & !(0b11 << 6)) | AP_USER_RW,
        executable_leaf() & !(0b11 << 6),
        (executable_leaf() & !(0b11 << 6)) | (0b10 << 6),
        executable_leaf() & !PXN,
        executable_leaf() & !ACCESS_FLAG,
        executable_leaf() & !0b11,
        (executable_leaf() & !0b11) | 0b01,
        (executable_leaf() & !0b11) | 0b10,
    ];
    for descriptor in denied {
        let mut tables = Tables::new();
        let context = tables.context(address);
        aarch64_vm::reserve(1, tables.root(), address, 5);
        tables.leaf(address, descriptor);
        assert_eq!(
            user_instruction_mapping_in(tables.root(), address),
            UserInstructionMapping::Denied,
            "descriptor={descriptor:#x}"
        );
        assert!(!user_context_entry_valid(&context, tables.root()));
        assert_eq!(
            aarch64_vm::queries(),
            0,
            "denied resident PTE consulted VMA fallback"
        );
    }
}

#[test]
fn parent_permissions_and_malformed_descriptors_are_denied() {
    let address = 0x280a_dc14;
    for level in 0..3 {
        for corruption in [1u64 << 60, 1u64 << 61, 1, 2, 0] {
            let mut tables = Tables::new();
            tables.leaf(address, executable_leaf());
            let slot = tables.slots(address)[level];
            let prior = unsafe { slot.read_volatile() };
            let denied = if corruption > 3 {
                prior | corruption
            } else {
                (prior & !0b11) | corruption
            };
            unsafe { slot.write_volatile(denied) };
            aarch64_vm::reserve(1, tables.root(), address, 5);
            assert_eq!(
                user_instruction_mapping_in(tables.root(), address),
                UserInstructionMapping::Denied,
                "level={level} descriptor={denied:#x}"
            );
            assert!(!user_instruction_pointer_valid_in(tables.root(), address));
            assert_eq!(aarch64_vm::queries(), 0);
        }
    }
}

#[test]
fn zero_table_addresses_are_denied_without_null_dereference() {
    let address = 0x280a_dc14;
    for level in 0..3 {
        let mut tables = Tables::new();
        tables.leaf(address, executable_leaf());
        let slot = tables.slots(address)[level];
        unsafe { slot.write_volatile(TABLE_DESCRIPTOR) };
        aarch64_vm::reserve(1, tables.root(), address, 5);
        assert_eq!(
            user_instruction_mapping_in(tables.root(), address),
            UserInstructionMapping::Denied
        );
        assert!(!user_instruction_pointer_valid_in(tables.root(), address));
        assert_eq!(aarch64_vm::queries(), 0);
    }
}

#[test]
fn parent_privileged_xn_and_readonly_do_not_deny_el0_rx() {
    let address = 0x280a_dc14;
    let mut tables = Tables::new();
    tables.leaf(address, executable_leaf());
    for slot in tables.slots(address).into_iter().take(3) {
        let prior = unsafe { slot.read_volatile() };
        // PXNTable and APTable read-only preserve the intended EL0 RX access.
        unsafe { slot.write_volatile(prior | (1 << 59) | (1 << 62)) };
    }
    assert!(user_address_executable_in(tables.root(), address));
}

#[test]
fn zero_missing_entries_are_distinct_from_invalid_present_entries() {
    let address = 0x280a_dc14;
    for level in 0..4 {
        let mut tables = Tables::new();
        tables.leaf(address, executable_leaf());
        let slot = tables.slots(address)[level];
        unsafe { slot.write_volatile(0) };
        assert_eq!(
            user_instruction_mapping_in(tables.root(), address),
            UserInstructionMapping::Absent
        );
        assert!(!user_instruction_pointer_valid_in(tables.root(), address));
        aarch64_vm::reserve(1, tables.root(), address, 5);
        assert!(
            user_instruction_pointer_valid_in(tables.root(), address),
            "missing level={level}"
        );
        let queries = aarch64_vm::queries();
        assert!(!user_address_executable_in(tables.root(), address));
        assert_eq!(
            aarch64_vm::queries(),
            queries,
            "resident-only signal predicate consulted VM state"
        );
    }
}

#[test]
fn signal_executable_check_uses_only_current_resident_translation() {
    let address = 0x280a_dc14;
    let mut tables = Tables::new();
    aarch64_vm::reserve(1, tables.root(), address, 5);
    ACTIVE_ROOT.set(tables.root());
    assert!(!user_address_executable(address));
    assert_eq!(aarch64_vm::queries(), 0);
    tables.leaf(address, executable_leaf());
    assert!(user_address_executable(address));
    tables.leaf(address, executable_leaf() | UXN);
    assert!(!user_address_executable(address));
    ACTIVE_ROOT.set(0);
    assert!(!user_address_executable(address));
    assert_eq!(aarch64_vm::queries(), 0);
}

#[test]
fn invalid_roots_fail_before_any_memory_access() {
    let mut tables = Tables::new();
    let mut context = tables.context(0x280a_dc14);
    for root in [0, kernel_root(), tables.root() + 1] {
        assert_eq!(
            user_instruction_mapping_in(root, context.elr),
            UserInstructionMapping::Denied
        );
        assert!(!user_instruction_pointer_valid_in(root, context.elr));
        assert!(!user_address_executable_in(root, context.elr));
        context.ttbr0 = root;
        assert!(!user_context_entry_valid(&context, root));
    }
    // A mismatched context must not even dereference this aligned sentinel.
    context.ttbr0 = u64::MAX & !4095;
    assert!(!user_context_entry_valid(&context, tables.root()));
    assert_eq!(aarch64_vm::queries(), 0);
}

#[test]
fn executable_page_in_other_process_is_not_authority() {
    let mut owner = Tables::new();
    let mut context = owner.context(0x280a_dc14);
    let mut other = Tables::new();
    // Install only a valid stack in the other root; its code VA is absent.
    other.leaf(
        context.sp_el0 - 1,
        0x1000 | TABLE_DESCRIPTOR | AP_USER_RW | ACCESS_FLAG | PXN | UXN,
    );
    assert!(!user_context_entry_valid(&context, other.root()));
    aarch64_vm::reserve(1, owner.root(), context.elr, 5);
    context.ttbr0 = other.root();
    assert!(!user_context_entry_valid(&context, other.root()));
    assert!(!user_address_executable_in(other.root(), context.elr));
}

#[test]
fn lazy_reservation_requires_exact_rx_and_matching_root() {
    let address = USER_MMAP_BASE + 0x4000;
    for protection in 0..=15 {
        let tables = Tables::new();
        aarch64_vm::reserve(1, tables.root(), address, protection);
        assert_eq!(
            user_instruction_pointer_valid_in(tables.root(), address),
            protection == 5,
            "protection={protection}"
        );
        assert_eq!(
            aarch64_vm::executable_region_in(tables.root(), address),
            protection == 5
        );
    }
    let owner = Tables::new();
    let other = Tables::new();
    aarch64_vm::reserve(1, owner.root(), address, 5);
    aarch64_vm::reserve(2, other.root(), address, 3);
    assert!(user_instruction_pointer_valid_in(owner.root(), address));
    assert!(!user_instruction_pointer_valid_in(other.root(), address));
    assert!(!user_instruction_pointer_valid_in(
        owner.root(),
        address + PAGE_SIZE
    ));
    assert!(!user_instruction_pointer_valid_in(
        owner.root(),
        address - 4
    ));
}

#[test]
fn lazy_executable_return_can_fault_after_eret_without_eager_io() {
    let address = USER_MMAP_BASE + PAGE_SIZE;
    let mut tables = Tables::new();
    let mut context = tables.context(address - 4);
    context.elr = address;
    aarch64_vm::reserve(1, tables.root(), address, 5);
    assert_eq!(
        user_instruction_mapping_in(tables.root(), address),
        UserInstructionMapping::Absent
    );
    assert!(user_context_entry_valid(&context, tables.root()));
    assert_eq!(
        user_instruction_mapping_in(tables.root(), address),
        UserInstructionMapping::Absent,
        "entry validation unexpectedly populated lazy page"
    );
    // The actual fault handler remains a guest/runtime concern. Model only
    // its resulting RX leaf and verify the same predicate accepts residency.
    tables.leaf(address, executable_leaf());
    assert!(user_context_entry_valid(&context, tables.root()));
}

#[test]
fn ordinary_stack_must_remain_aligned_writable_and_nonexecutable() {
    let mut tables = Tables::new();
    let mut context = tables.context(0x280a_dc14);
    let valid_sp = context.sp_el0;
    for stack in [
        0,
        USER_ADDRESS_BASE - 16,
        USER_ADDRESS_LIMIT + 16,
        valid_sp + 1,
        valid_sp + 8,
        valid_sp + PAGE_SIZE,
    ] {
        context.sp_el0 = stack;
        assert!(
            !user_context_entry_valid(&context, tables.root()),
            "sp={stack:#x}"
        );
    }
    context.sp_el0 = valid_sp;
    for descriptor in [
        0,
        executable_leaf(),
        0x1000 | TABLE_DESCRIPTOR | AP_USER_RO | ACCESS_FLAG | PXN | UXN,
        0x1000 | TABLE_DESCRIPTOR | AP_USER_RW | ACCESS_FLAG | PXN,
    ] {
        tables.leaf(valid_sp - 1, descriptor);
        assert!(!user_context_entry_valid(&context, tables.root()));
    }
}

#[test]
fn legacy_stack_and_nzcv_policy_are_preserved() {
    let mut tables = Tables::new();
    let mut context = tables.context(0x280a_dc14);
    for stack in [LEGACY_USER_STACK_TOP, USER_STACK_TOP] {
        context.sp_el0 = stack;
        assert!(user_context_entry_valid(&context, tables.root()));
    }
    for flags in 0..16u64 {
        context.spsr = flags << 28;
        assert!(user_context_entry_valid(&context, tables.root()));
    }
    for bit in 0..64 {
        if (28..32).contains(&bit) {
            continue;
        }
        context.spsr = 1u64 << bit;
        assert!(
            !user_context_entry_valid(&context, tables.root()),
            "spsr bit={bit}"
        );
    }
}

#[test]
fn all_nzcv_and_btype_combinations_are_valid_on_a_bti_pe() {
    let mut tables = Tables::new();
    let mut context = tables.context(0x8423_cba8);
    // Unrelated feature fields must neither enable nor disable BTI.
    for pfr1 in [1, 0xffff_ffff_ffff_fff1] {
        CPU_PFR1.set(pfr1);
        for nzcv in 0..16u64 {
            for btype in 0..4u64 {
                context.spsr = nzcv << 28 | btype << 10;
                assert!(
                    user_spsr_valid_for_features(context.spsr, pfr1),
                    "defined NZCV/BTYPE rejected: pfr1={pfr1:#x} spsr={:#x}",
                    context.spsr
                );
                assert!(user_context_entry_valid(&context, tables.root()));
                assert_eq!(context.spsr, nzcv << 28 | btype << 10);
            }
        }
    }
}

#[test]
fn mac_firefox_saved_branch_state_is_valid_only_with_bti() {
    let mut tables = Tables::new();
    let mut context = tables.context(0x8423_cba8);
    context.sp_el0 = 0x896b_7ff0;
    tables.leaf(
        context.sp_el0 - 1,
        0x1000 | TABLE_DESCRIPTOR | AP_USER_RW | ACCESS_FLAG | PXN | UXN,
    );
    context.spsr = 0x8000_0400;
    assert!(!user_context_entry_valid(&context, tables.root()));
    CPU_PFR1.set(1);
    assert!(
        user_context_entry_valid(&context, tables.root()),
        "legitimate Mac Firefox saved branch state rejected"
    );
    assert_eq!(context.spsr, 0x8000_0400, "validation changed saved SPSR");
}

#[test]
fn unsupported_and_reserved_bti_ids_do_not_authorize_branch_state() {
    let mut tables = Tables::new();
    let mut context = tables.context(0x8423_cba8);
    for bt in 0..16u64 {
        if bt == 1 {
            continue;
        }
        for other_features in [0, 0xffff_ffff_ffff_fff0] {
            let pfr1 = other_features | bt;
            CPU_PFR1.set(pfr1);
            for nzcv in 0..16u64 {
                context.spsr = nzcv << 28;
                assert!(user_spsr_valid_for_features(context.spsr, pfr1));
                assert!(user_context_entry_valid(&context, tables.root()));
                for btype in 1..4u64 {
                    context.spsr = nzcv << 28 | btype << 10;
                    assert!(
                        !user_spsr_valid_for_features(context.spsr, pfr1),
                        "unsupported BTYPE accepted: pfr1={pfr1:#x} spsr={:#x}",
                        context.spsr
                    );
                    assert!(!user_context_entry_valid(&context, tables.root()));
                }
            }
        }
    }
}

#[test]
fn bti_does_not_authorize_any_other_spsr_bit() {
    let mut tables = Tables::new();
    let mut context = tables.context(0x8423_cba8);
    CPU_PFR1.set(1);
    for nzcv in 0..16u64 {
        for btype in 0..4u64 {
            for bit in 0..64 {
                if (28..32).contains(&bit) || (10..12).contains(&bit) {
                    continue;
                }
                context.spsr = nzcv << 28 | btype << 10 | 1u64 << bit;
                assert!(
                    !user_spsr_valid_for_features(context.spsr, 1),
                    "privileged or unsupported SPSR bit accepted: bit={bit} spsr={:#x}",
                    context.spsr
                );
                assert!(!user_context_entry_valid(&context, tables.root()));
            }
        }
    }
}

#[test]
fn btype_never_bypasses_selected_root_pc_or_stack_checks() {
    for btype in 1..4u64 {
        let mut tables = Tables::new();
        let mut valid = tables.context(0x8423_cba8);
        valid.spsr = 0x8000_0000 | btype << 10;
        CPU_PFR1.set(1);
        assert!(user_context_entry_valid(&valid, tables.root()));

        assert!(!user_context_entry_valid(&valid, 0));
        assert!(!user_context_entry_valid(&valid, tables.root() + PAGE_SIZE));
        for root in [0, kernel_root(), tables.root() + 1] {
            let mut invalid = valid;
            invalid.ttbr0 = root;
            assert!(!user_context_entry_valid(&invalid, root));
        }

        for pc in [0, USER_ADDRESS_BASE - 4, USER_ADDRESS_LIMIT, valid.elr + 1] {
            let mut invalid = valid;
            invalid.elr = pc;
            assert!(!user_context_entry_valid(&invalid, tables.root()));
        }
        for descriptor in [
            0,
            executable_leaf() | UXN,
            (executable_leaf() & !(0b11 << 6)) | AP_USER_RW,
            executable_leaf() & !PXN,
        ] {
            tables.leaf(valid.elr, descriptor);
            assert!(!user_context_entry_valid(&valid, tables.root()));
        }
        tables.leaf(valid.elr, executable_leaf());

        for sp in [
            0,
            valid.sp_el0 + 1,
            valid.sp_el0 + 8,
            valid.sp_el0 + PAGE_SIZE,
        ] {
            let mut invalid = valid;
            invalid.sp_el0 = sp;
            assert!(!user_context_entry_valid(&invalid, tables.root()));
        }
        for descriptor in [
            0,
            executable_leaf(),
            0x1000 | TABLE_DESCRIPTOR | AP_USER_RO | ACCESS_FLAG | PXN | UXN,
            0x1000 | TABLE_DESCRIPTOR | AP_USER_RW | ACCESS_FLAG | PXN,
        ] {
            tables.leaf(valid.sp_el0 - 1, descriptor);
            assert!(!user_context_entry_valid(&valid, tables.root()));
        }
    }
}

#[test]
fn saved_branch_state_survives_production_capture_and_restore() {
    assert_eq!(core::mem::offset_of!(ExceptionFrame, spsr), 256);
    assert_eq!(core::mem::offset_of!(UserContext, spsr), 256);
    let mut tables = Tables::new();
    let template = tables.context(0x8423_cba8);
    CPU_PFR1.set(1);
    assert_eq!(
        UserContext::initial(template.elr, template.sp_el0, tables.root(), 42).spsr,
        0
    );
    for nzcv in 0..16u64 {
        for btype in 0..4u64 {
            let saved = nzcv << 28 | btype << 10;
            let frame = ExceptionFrame {
                registers: core::array::from_fn(|index| 0x4000 + index as u64),
                elr: template.elr,
                spsr: saved,
                esr: 0x9200_0007,
                far: 0x8100_0000,
                sp_el0: template.sp_el0,
                ttbr0: tables.root(),
                tpidr_el0: 0x9000_0000,
                vector_registers: core::array::from_fn(|index| u128::MAX - index as u128),
                fpcr: 0x0040_0000,
                fpsr: 0x0000_0010,
            };
            let context = UserContext::capture(&frame);
            assert_eq!(context.spsr, saved, "capture changed saved SPSR");
            assert!(user_context_entry_valid(&context, tables.root()));
            let mut restored = ExceptionFrame {
                registers: [0; 31],
                elr: 0,
                spsr: 0,
                esr: 0,
                far: 0,
                sp_el0: 0,
                ttbr0: 0,
                tpidr_el0: 0,
                vector_registers: [0; 32],
                fpcr: 0,
                fpsr: 0,
            };
            context.restore(&mut restored);
            assert_eq!(restored.spsr, saved, "restore changed saved SPSR");
            assert_eq!(restored.registers, frame.registers);
            assert_eq!(restored.elr, frame.elr);
            assert_eq!(restored.esr, frame.esr);
            assert_eq!(restored.far, frame.far);
            assert_eq!(restored.sp_el0, frame.sp_el0);
            assert_eq!(restored.ttbr0, frame.ttbr0);
            assert_eq!(restored.tpidr_el0, frame.tpidr_el0);
            assert_eq!(restored.vector_registers, frame.vector_registers);
            assert_eq!(restored.fpcr, frame.fpcr);
            assert_eq!(restored.fpsr, frame.fpsr);
        }
    }
}
