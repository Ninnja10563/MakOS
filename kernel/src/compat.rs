use core::slice;

const LINUX_SYS_WRITE: u64 = 1;
const LINUX_SYS_GETPID: u64 = 39;
const LINUX_SYS_EXIT: u64 = 60;
const LINUX_SYS_UNAME: u64 = 63;
const LINUX_SYS_CLOCK_GETTIME: u64 = 228;
const LINUX_CLOCK_MONOTONIC: u64 = 1;
const LINUX_EBADF: i64 = -9;
const LINUX_EFAULT: i64 = -14;
const LINUX_EINVAL: i64 = -22;

const WIN_OP_WRITE_FILE: u64 = 1;
const WIN_OP_GET_PROCESS_ID: u64 = 2;
const WIN_OP_GET_TICK_COUNT: u64 = 3;
const WIN_OP_CREATE_EVENT: u64 = 4;
const WIN_OP_SET_EVENT: u64 = 5;
const WIN_OP_WAIT_OBJECT: u64 = 6;
const WIN_OP_CLOSE_HANDLE: u64 = 7;
const WIN_OP_EXIT_PROCESS: u64 = 8;
const WIN_INFINITE: u64 = 0xffff_ffff;
const WIN_WAIT_OBJECT_0: u64 = 0;
const WIN_WAIT_FAILED: u64 = 0xffff_ffff;

pub fn dispatch_linux(registers: &mut crate::arch::SavedRegisters) {
    registers.rax = match registers.rax {
        LINUX_SYS_WRITE => write(registers.rdi, registers.rsi, registers.rdx as usize),
        LINUX_SYS_GETPID => crate::scheduler::current_pid(),
        LINUX_SYS_EXIT => crate::process::exit_current(registers.rdi),
        LINUX_SYS_UNAME => uname(registers.rdi),
        LINUX_SYS_CLOCK_GETTIME => clock_gettime(registers.rdi, registers.rsi),
        _ => error(LINUX_EINVAL),
    };
}

pub fn dispatch_windows(registers: &mut crate::arch::SavedRegisters) {
    registers.rax = match registers.rax {
        WIN_OP_WRITE_FILE => windows_write(
            registers.rdi,
            registers.rsi,
            registers.rdx as usize,
            registers.r10,
        ),
        WIN_OP_GET_PROCESS_ID => crate::scheduler::current_pid(),
        WIN_OP_GET_TICK_COUNT => crate::arch::monotonic_ticks().saturating_mul(10),
        WIN_OP_CREATE_EVENT => {
            if crate::security::has_capability(crate::security::CAP_SYNC) {
                crate::ipc::create_event(registers.rdi != 0).unwrap_or(0)
            } else {
                0
            }
        }
        WIN_OP_SET_EVENT => u64::from(
            crate::security::has_capability(crate::security::CAP_SYNC)
                && crate::ipc::signal_event(registers.rdi),
        ),
        WIN_OP_WAIT_OBJECT => {
            if registers.rsi == WIN_INFINITE
                && crate::security::has_capability(crate::security::CAP_SYNC)
                && crate::ipc::wait_event(registers.rdi)
            {
                WIN_WAIT_OBJECT_0
            } else {
                WIN_WAIT_FAILED
            }
        }
        WIN_OP_CLOSE_HANDLE => u64::from(crate::ipc::close(registers.rdi)),
        WIN_OP_EXIT_PROCESS => crate::process::exit_current(registers.rdi),
        _ => 0,
    };
}

fn windows_write(handle: u64, address: u64, length: usize, written: u64) -> u64 {
    if handle != 1
        || !crate::security::has_capability(crate::security::CAP_CONSOLE)
        || !crate::process::valid_user_range(address, length)
        || !crate::process::valid_user_range_write(written, core::mem::size_of::<u32>())
    {
        return 0;
    }
    let bytes = unsafe { slice::from_raw_parts(address as *const u8, length) };
    let Ok(text) = core::str::from_utf8(bytes) else {
        return 0;
    };
    crate::serial_print!("{}", text);
    unsafe { (written as *mut u32).write_unaligned(length as u32) };
    1
}

fn write(fd: u64, address: u64, length: usize) -> u64 {
    if fd != 1 && fd != 2 {
        return error(LINUX_EBADF);
    }
    if !crate::security::has_capability(crate::security::CAP_CONSOLE)
        || !crate::process::valid_user_range(address, length)
    {
        return error(LINUX_EFAULT);
    }
    let bytes = unsafe { slice::from_raw_parts(address as *const u8, length) };
    let Ok(text) = core::str::from_utf8(bytes) else {
        return error(LINUX_EINVAL);
    };
    crate::serial_print!("{}", text);
    length as u64
}

fn uname(address: u64) -> u64 {
    const FIELD_BYTES: usize = 65;
    const FIELD_COUNT: usize = 6;
    const TOTAL_BYTES: usize = FIELD_BYTES * FIELD_COUNT;
    if !crate::process::valid_user_range_write(address, TOTAL_BYTES) {
        return error(LINUX_EFAULT);
    }
    let mut output = [0u8; TOTAL_BYTES];
    for (index, value) in [
        b"MakOS".as_slice(),
        b"makos".as_slice(),
        b"0.1.0-linux-personality".as_slice(),
        b"MakOS ABI v1".as_slice(),
        b"x86_64".as_slice(),
        b"(none)".as_slice(),
    ]
    .iter()
    .enumerate()
    {
        output[index * FIELD_BYTES..index * FIELD_BYTES + value.len()].copy_from_slice(value);
    }
    unsafe {
        core::ptr::copy_nonoverlapping(output.as_ptr(), address as *mut u8, TOTAL_BYTES);
    }
    0
}

fn clock_gettime(clock_id: u64, address: u64) -> u64 {
    if clock_id != LINUX_CLOCK_MONOTONIC || !crate::process::valid_user_range_write(address, 16) {
        return error(LINUX_EINVAL);
    }
    let ticks = crate::arch::monotonic_ticks();
    let seconds = ticks / 100;
    let nanoseconds = (ticks % 100) * 10_000_000;
    unsafe {
        (address as *mut u64).write_unaligned(seconds);
        (address as *mut u64).add(1).write_unaligned(nanoseconds);
    }
    0
}

fn error(value: i64) -> u64 {
    value as u64
}
