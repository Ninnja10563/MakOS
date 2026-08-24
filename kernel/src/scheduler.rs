use core::arch::global_asm;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const TASK_COUNT: usize = 8;
const FIRST_DYNAMIC_TASK: usize = 3;
const STACK_SIZE: usize = 16 * 1024;
const QUANTUM_TICKS: u64 = 5;

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static TASK_A_COUNTER: AtomicU64 = AtomicU64::new(0);
static TASK_B_COUNTER: AtomicU64 = AtomicU64::new(0);
static mut CURRENT: usize = 0;
static mut TASKS: [Task; TASK_COUNT] = [Task {
    rsp: 0,
    active: false,
    pid: 0,
    tid: 0,
    address_space: 0,
    ring0_stack_top: 0,
}; TASK_COUNT];
static mut STACK_A: KernelStack = KernelStack([0; STACK_SIZE]);
static mut STACK_B: KernelStack = KernelStack([0; STACK_SIZE]);

#[derive(Clone, Copy)]
#[repr(C)]
struct Task {
    rsp: u64,
    active: bool,
    pid: u64,
    tid: u64,
    address_space: u64,
    ring0_stack_top: u64,
}

#[repr(C, align(16))]
struct KernelStack([u8; STACK_SIZE]);

unsafe extern "C" {
    fn makos_context_switch(old_rsp: *mut u64, new_rsp: u64);
}

global_asm!(
    r#"
.global makos_context_switch
makos_context_switch:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov [rdi], rsp
    mov rsp, rsi
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret
"#
);

pub fn init() {
    if INITIALIZED.swap(true, Ordering::AcqRel) {
        crate::fatal("scheduler initialized twice");
    }
    let tasks = (&raw mut TASKS).cast::<Task>();
    unsafe {
        (*tasks.add(0)).rsp = 0;
        (*tasks.add(0)).active = true;
        (*tasks.add(1)).rsp = prepare_stack((&raw mut STACK_A).cast::<u8>(), task_a);
        (*tasks.add(1)).active = true;
        (*tasks.add(2)).rsp = prepare_stack((&raw mut STACK_B).cast::<u8>(), task_b);
        (*tasks.add(2)).active = true;
    }
    crate::serial_println!(
        "scheduler tasks={} quantum_ticks={}",
        TASK_COUNT,
        QUANTUM_TICKS
    );
}

pub fn on_tick(tick: u64) {
    if !INITIALIZED.load(Ordering::Acquire) || tick % QUANTUM_TICKS != 0 {
        return;
    }
    unsafe {
        let tasks = (&raw mut TASKS).cast::<Task>();
        let current_ptr = &raw mut CURRENT;
        let current = current_ptr.read();
        let next = next_active(tasks, current).unwrap_or(current);
        if next == current {
            return;
        }
        switch_to(tasks, current_ptr, current, next);
    }
}

pub fn yield_current() {
    unsafe {
        let tasks = (&raw mut TASKS).cast::<Task>();
        let current_ptr = &raw mut CURRENT;
        let current = current_ptr.read();
        let next = next_active(tasks, current).unwrap_or(current);
        if next != current {
            switch_to(tasks, current_ptr, current, next);
        }
    }
}

pub fn block_current() {
    unsafe {
        let tasks = (&raw mut TASKS).cast::<Task>();
        let current_ptr = &raw mut CURRENT;
        let current = current_ptr.read();
        (*tasks.add(current)).active = false;
        let next = next_active(tasks, current)
            .unwrap_or_else(|| crate::fatal("blocked with no runnable task"));
        switch_to(tasks, current_ptr, current, next);
    }
}

pub fn wake(pid: u64, tid: u64) -> bool {
    unsafe {
        let tasks = (&raw mut TASKS).cast::<Task>();
        for index in 0..TASK_COUNT {
            let task = &mut *tasks.add(index);
            if task.pid == pid && task.tid == tid && task.rsp != 0 && !task.active {
                task.active = true;
                return true;
            }
        }
    }
    false
}

pub fn configure_init(pid: u64, address_space: u64, ring0_stack_top: u64) {
    unsafe {
        let task = (&raw mut TASKS).cast::<Task>();
        (*task).pid = pid;
        (*task).tid = pid;
        (*task).address_space = address_space;
        (*task).ring0_stack_top = ring0_stack_top;
    }
}

pub fn spawn_user(
    pid: u64,
    address_space: u64,
    ring0_stack_base: *mut u8,
    ring0_stack_top: u64,
    entry: extern "C" fn() -> !,
) -> bool {
    unsafe {
        let tasks = (&raw mut TASKS).cast::<Task>();
        for index in FIRST_DYNAMIC_TASK..TASK_COUNT {
            let task = &*tasks.add(index);
            if task.rsp != 0 && task.pid == pid {
                return false;
            }
        }
        let Some(index) =
            (FIRST_DYNAMIC_TASK..TASK_COUNT).find(|index| (*tasks.add(*index)).rsp == 0)
        else {
            return false;
        };
        let task = &mut *tasks.add(index);
        *task = Task {
            rsp: prepare_stack(ring0_stack_base, entry),
            active: true,
            pid,
            tid: pid,
            address_space,
            ring0_stack_top,
        };
    }
    true
}

pub fn spawn_user_thread(
    pid: u64,
    tid: u64,
    address_space: u64,
    ring0_stack_base: *mut u8,
    ring0_stack_top: u64,
    entry: extern "C" fn() -> !,
) -> bool {
    unsafe {
        let tasks = (&raw mut TASKS).cast::<Task>();
        for index in FIRST_DYNAMIC_TASK..TASK_COUNT {
            let task = &*tasks.add(index);
            if task.rsp != 0 && task.pid == pid && task.tid == tid {
                return false;
            }
        }
        let Some(index) =
            (FIRST_DYNAMIC_TASK..TASK_COUNT).find(|index| (*tasks.add(*index)).rsp == 0)
        else {
            return false;
        };
        let task = &mut *tasks.add(index);
        *task = Task {
            rsp: prepare_stack(ring0_stack_base, entry),
            active: true,
            pid,
            tid,
            address_space,
            ring0_stack_top,
        };
    }
    true
}

pub fn current_pid() -> u64 {
    unsafe {
        let tasks = (&raw const TASKS).cast::<Task>();
        (*tasks.add((&raw const CURRENT).read())).pid
    }
}

pub fn current_tid() -> u64 {
    unsafe {
        let tasks = (&raw const TASKS).cast::<Task>();
        (*tasks.add((&raw const CURRENT).read())).tid
    }
}

pub fn address_space(pid: u64) -> Option<u64> {
    unsafe {
        let tasks = (&raw const TASKS).cast::<Task>();
        for index in 0..TASK_COUNT {
            let task = *tasks.add(index);
            if task.pid == pid && task.address_space != 0 {
                return Some(task.address_space);
            }
        }
    }
    None
}

pub fn reap(pid: u64) -> Option<u64> {
    unsafe {
        let tasks = (&raw mut TASKS).cast::<Task>();
        for index in 0..TASK_COUNT {
            let task = &mut *tasks.add(index);
            if task.pid == pid && task.rsp != 0 && !task.active {
                let root = task.address_space;
                *task = Task {
                    rsp: 0,
                    active: false,
                    pid: 0,
                    tid: 0,
                    address_space: 0,
                    ring0_stack_top: 0,
                };
                return (root != 0).then_some(root);
            }
        }
    }
    None
}

pub fn reap_thread(pid: u64, tid: u64) -> bool {
    unsafe {
        let tasks = (&raw mut TASKS).cast::<Task>();
        for index in FIRST_DYNAMIC_TASK..TASK_COUNT {
            let task = &mut *tasks.add(index);
            if task.pid == pid && task.tid == tid && task.rsp != 0 && !task.active {
                *task = Task {
                    rsp: 0,
                    active: false,
                    pid: 0,
                    tid: 0,
                    address_space: 0,
                    ring0_stack_top: 0,
                };
                return true;
            }
        }
    }
    false
}

pub fn exit_current() -> ! {
    unsafe {
        let tasks = (&raw mut TASKS).cast::<Task>();
        let current_ptr = &raw mut CURRENT;
        let current = current_ptr.read();
        (*tasks.add(current)).active = false;
        let next = next_active(tasks, current).unwrap_or_else(|| crate::fatal("last task exited"));
        switch_to(tasks, current_ptr, current, next);
    }
    crate::fatal("exited task resumed")
}

unsafe fn switch_to(tasks: *mut Task, current_ptr: *mut usize, current: usize, next: usize) {
    let target = unsafe { *tasks.add(next) };
    if target.address_space != 0 {
        crate::arch::switch_address_space(target.address_space);
    }
    if target.ring0_stack_top != 0 {
        crate::arch::set_ring0_stack(target.ring0_stack_top);
    }
    unsafe {
        current_ptr.write(next);
        let old_rsp = &raw mut (*tasks.add(current)).rsp;
        makos_context_switch(old_rsp, target.rsp);
    }
}

unsafe fn next_active(tasks: *mut Task, current: usize) -> Option<usize> {
    for distance in 1..=TASK_COUNT {
        let candidate = (current + distance) % TASK_COUNT;
        if unsafe { (*tasks.add(candidate)).active } {
            return Some(candidate);
        }
    }
    None
}

pub fn task_counters() -> (u64, u64) {
    (
        TASK_A_COUNTER.load(Ordering::Relaxed),
        TASK_B_COUNTER.load(Ordering::Relaxed),
    )
}

pub fn task_stats() -> (usize, usize, u64, u64) {
    unsafe {
        let tasks = (&raw const TASKS).cast::<Task>();
        let mut occupied = 0usize;
        let mut runnable = 0usize;
        for index in 0..TASK_COUNT {
            let task = *tasks.add(index);
            occupied += usize::from(task.rsp != 0 || index == (&raw const CURRENT).read());
            runnable += usize::from(task.active);
        }
        (occupied, runnable, current_pid(), current_tid())
    }
}

unsafe fn prepare_stack(stack: *mut u8, entry: extern "C" fn() -> !) -> u64 {
    let top = ((stack as usize + STACK_SIZE) & !15usize) as *mut u64;
    let rsp = unsafe { top.sub(8) };
    for index in 0..6 {
        unsafe { rsp.add(index).write(0) };
    }
    unsafe {
        rsp.add(6).write(entry as usize as u64);
        rsp.add(7).write(task_returned as usize as u64);
    }
    rsp as u64
}

extern "C" fn task_a() -> ! {
    crate::arch::enable_interrupts();
    loop {
        TASK_A_COUNTER.fetch_add(1, Ordering::Relaxed);
        core::hint::spin_loop();
    }
}

extern "C" fn task_b() -> ! {
    crate::arch::enable_interrupts();
    loop {
        TASK_B_COUNTER.fetch_add(1, Ordering::Relaxed);
        core::hint::spin_loop();
    }
}

extern "C" fn task_returned() -> ! {
    crate::fatal("kernel task returned")
}
