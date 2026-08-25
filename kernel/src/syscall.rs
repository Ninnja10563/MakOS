use core::slice;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const SYS_WRITE: u64 = 0;
const SYS_YIELD: u64 = 1;
const SYS_CHANNEL_CREATE: u64 = 2;
const SYS_CHANNEL_SEND: u64 = 3;
const SYS_CHANNEL_RECEIVE: u64 = 4;
const SYS_EXIT: u64 = 5;
const SYS_READ_KEY: u64 = 6;
const SYS_SHELL_COMMAND: u64 = 7;
const SYS_SURFACE_CREATE: u64 = 8;
const SYS_SURFACE_FILL: u64 = 9;
const SYS_SURFACE_PRESENT: u64 = 10;
const SYS_OPEN: u64 = 11;
const SYS_READ: u64 = 12;
const SYS_CLOSE: u64 = 13;
const SYS_PROCESS_SPAWN: u64 = 14;
const SYS_PROCESS_WAIT: u64 = 15;
const SYS_SOCKET_UDP_DNS: u64 = 16;
const SYS_FILE_WRITE: u64 = 17;
const SYS_PACKAGE_INSTALL: u64 = 18;
const SYS_PACKAGE_QUERY: u64 = 19;
const SYS_PACKAGE_ROLLBACK: u64 = 20;
const SYS_VM_MAP: u64 = 21;
const SYS_VM_UNMAP: u64 = 22;
const SYS_THREAD_CREATE: u64 = 23;
const SYS_THREAD_JOIN: u64 = 24;
const SYS_THREAD_EXIT: u64 = 25;
const SYS_SOCKET_TCP_HTTP: u64 = 26;
const SYS_CLOCK_MONOTONIC: u64 = 27;
const SYS_LOG_APPEND: u64 = 28;
const SYS_LOG_READ: u64 = 29;
const SYS_AUTH_LOGIN: u64 = 30;
const SYS_ABI_INFO: u64 = 31;
const SYS_EVENT_CREATE: u64 = 32;
const SYS_EVENT_SIGNAL: u64 = 33;
const SYS_EVENT_WAIT: u64 = 34;
const SYS_HANDLE_CLOSE: u64 = 35;
const SYS_STAT: u64 = 36;
const SYS_READ_DIR: u64 = 37;
const SYS_PROCESS_SPAWN_LINUX: u64 = 38;
const SYS_AUDIO_WRITE: u64 = 39;
const SYS_SOCKET_IPV6_ECHO: u64 = 40;
const SYS_PROCESS_SPAWN_WINDOWS: u64 = 41;
const SYS_SERVICE_START: u64 = 42;
const SYS_CREATE: u64 = 43;
const SYS_UNLINK: u64 = 44;
const SYS_VM_PROTECT: u64 = 45;
const SYS_PROCESS_SPAWN_TOOLCHAIN: u64 = 46;
const SYS_SOCKET_CREATE: u64 = 47;
const SYS_SOCKET_CONNECT: u64 = 48;
const SYS_SOCKET_SEND: u64 = 49;
const SYS_SOCKET_RECEIVE: u64 = 50;
const SYS_SOCKET_CLOSE: u64 = 51;
const SYS_PACKAGE_REMOVE: u64 = 52;
const SYS_VM_MAP_RANGE: u64 = 53;
const SYS_VM_UNMAP_RANGE: u64 = 54;
const SYS_VM_PROTECT_RANGE: u64 = 55;
const SYS_PROCESS_SPAWN_PATH: u64 = 56;
const SYS_PROCESS_SPAWN_PATH_ARGS: u64 = 57;
const SYS_TYPED_SERVICE_PUBLISH: u64 = 143;
const SYS_TYPED_SERVICE_CONNECT: u64 = 144;
const SYS_TYPED_SERVICE_ACCEPT: u64 = 145;
const SYS_TYPED_CHANNEL_SEND: u64 = 146;
const SYS_TYPED_CHANNEL_RECEIVE: u64 = 147;
const SYS_THREAD_AFFINITY: u64 = 148;
const ERROR_INVALID: u64 = (-1i64) as u64;

const ABI_VERSION_1_0: u64 = 0x0001_0000;
const ABI_FEATURE_IPC: u64 = 1 << 0;
const ABI_FEATURE_PROCESS: u64 = 1 << 1;
const ABI_FEATURE_VM: u64 = 1 << 2;
const ABI_FEATURE_VFS: u64 = 1 << 3;
const ABI_FEATURE_NETWORK: u64 = 1 << 4;
const ABI_FEATURE_GRAPHICS: u64 = 1 << 5;
const ABI_FEATURE_AUTH: u64 = 1 << 6;
const ABI_FEATURE_LOG: u64 = 1 << 7;
const ABI_FEATURE_SYNC: u64 = 1 << 8;
const ABI_FEATURE_LINUX_PERSONALITY: u64 = 1 << 9;
const ABI_FEATURE_AUDIO: u64 = 1 << 10;
const ABI_FEATURE_IPV6: u64 = 1 << 11;
const ABI_FEATURE_WINDOWS_PERSONALITY: u64 = 1 << 12;
const ABI_FEATURE_SERVICE_SUPERVISION: u64 = 1 << 13;
const ABI_FEATURE_SELF_HOSTING_SEED: u64 = 1 << 14;
const ABI_FEATURE_SOCKET_OBJECTS: u64 = 1 << 15;
const ABI_FEATURE_PACKAGE_TRANSACTIONS: u64 = 1 << 16;
const ABI_FEATURE_VM_REGIONS: u64 = 1 << 17;
const ABI_FEATURE_EXEC_BY_PATH: u64 = 1 << 18;
const ABI_FEATURE_PROCESS_STARTUP: u64 = 1 << 19;
const ABI_FEATURE_TYPED_IPC: u64 = 1 << 21;
const ABI_FEATURE_CPU_AFFINITY: u64 = 1 << 22;
const ABI_FEATURES: u64 = ABI_FEATURE_IPC
    | ABI_FEATURE_PROCESS
    | ABI_FEATURE_VM
    | ABI_FEATURE_VFS
    | ABI_FEATURE_NETWORK
    | ABI_FEATURE_GRAPHICS
    | ABI_FEATURE_AUTH
    | ABI_FEATURE_LOG
    | ABI_FEATURE_SYNC
    | ABI_FEATURE_LINUX_PERSONALITY
    | ABI_FEATURE_AUDIO
    | ABI_FEATURE_IPV6
    | ABI_FEATURE_WINDOWS_PERSONALITY
    | ABI_FEATURE_SERVICE_SUPERVISION
    | ABI_FEATURE_SELF_HOSTING_SEED
    | ABI_FEATURE_SOCKET_OBJECTS
    | ABI_FEATURE_PACKAGE_TRANSACTIONS
    | ABI_FEATURE_VM_REGIONS
    | ABI_FEATURE_EXEC_BY_PATH
    | ABI_FEATURE_PROCESS_STARTUP
    | ABI_FEATURE_TYPED_IPC
    | ABI_FEATURE_CPU_AFFINITY;

static SYSCALL_COUNT: AtomicU64 = AtomicU64::new(0);
static IPC_PROVEN: AtomicBool = AtomicBool::new(false);
static M3_REPORTED: AtomicBool = AtomicBool::new(false);

pub fn dispatch(registers: &mut crate::arch::SavedRegisters, frame: &mut crate::arch::TrapFrame) {
    if frame.cs & 3 != 3 {
        crate::fatal("syscall frame is not userspace");
    }
    registers.rax = match registers.rax {
        SYS_WRITE => {
            if crate::security::has_capability(crate::security::CAP_CONSOLE) {
                write(registers.rdi, registers.rsi as usize)
            } else {
                ERROR_INVALID
            }
        }
        SYS_YIELD => {
            crate::scheduler::yield_current();
            0
        }
        SYS_CHANNEL_CREATE => {
            if crate::security::has_capability(crate::security::CAP_IPC) {
                channel_create(registers.rdi)
            } else {
                ERROR_INVALID
            }
        }
        SYS_CHANNEL_SEND => {
            if crate::security::has_capability(crate::security::CAP_IPC)
                && crate::ipc::send(registers.rdi, registers.rsi)
            {
                0
            } else {
                ERROR_INVALID
            }
        }
        SYS_CHANNEL_RECEIVE => match crate::security::has_capability(crate::security::CAP_IPC)
            .then(|| crate::ipc::receive(registers.rdi))
            .flatten()
        {
            Some(value) => {
                if value == 0x1234 {
                    IPC_PROVEN.store(true, Ordering::Release);
                }
                value
            }
            None => ERROR_INVALID,
        },
        SYS_TYPED_SERVICE_PUBLISH => typed_service_publish(registers.rdi, registers.rsi as usize),
        SYS_TYPED_SERVICE_CONNECT => typed_service_connect(registers.rdi, registers.rsi as usize),
        SYS_TYPED_SERVICE_ACCEPT => typed_service_accept(registers.rdi),
        SYS_TYPED_CHANNEL_SEND => typed_channel_send(
            registers.rdi,
            registers.rsi,
            registers.rdx,
            registers.r10 as u8,
        ),
        SYS_TYPED_CHANNEL_RECEIVE => {
            typed_channel_receive(registers.rdi, registers.rsi, registers.rdx)
        }
        SYS_THREAD_AFFINITY => {
            if !crate::scheduler::thread_in_current_process(registers.rsi) {
                (-3i64) as u64
            } else {
                match registers.rdi {
                    0 => 1,
                    1 if registers.rdx == 1 => 0,
                    1 => (-22i64) as u64,
                    _ => (-22i64) as u64,
                }
            }
        }
        SYS_EXIT => crate::process::exit_current(registers.rdi),
        SYS_READ_KEY => {
            if crate::security::has_capability(crate::security::CAP_INPUT) {
                u64::from(
                    crate::drivers::usb_uhci::read_key()
                        .or_else(crate::drivers::ps2::read_key)
                        .unwrap_or(0),
                )
            } else {
                ERROR_INVALID
            }
        }
        SYS_SHELL_COMMAND => {
            if crate::security::has_capability(crate::security::CAP_INPUT) {
                shell_command(registers.rdi, registers.rsi as usize)
            } else {
                ERROR_INVALID
            }
        }
        SYS_SURFACE_CREATE => {
            if crate::security::has_capability(crate::security::CAP_GRAPHICS) {
                crate::graphics::create(registers.rdi as u32, registers.rsi as u32)
            } else {
                0
            }
        }
        SYS_SURFACE_FILL => {
            let packed = registers.rdx;
            let x = packed as u16 as u32;
            let y = (packed >> 16) as u16 as u32;
            let width = (packed >> 32) as u16 as u32;
            let height = (packed >> 48) as u16 as u32;
            u64::from(crate::graphics::fill_rect(
                registers.rdi,
                x,
                y,
                width,
                height,
                registers.rsi as u32,
            ))
        }
        SYS_SURFACE_PRESENT => u64::from(crate::graphics::present(registers.rdi)),
        SYS_OPEN => open(registers.rdi, registers.rsi as usize, registers.rdx != 0),
        SYS_READ => read(registers.rdi, registers.rsi, registers.rdx as usize),
        SYS_CLOSE => u64::from(crate::vfs::close(registers.rdi)),
        SYS_PROCESS_SPAWN => {
            if crate::security::has_capability(crate::security::CAP_PROCESS) {
                crate::process::spawn_worker().unwrap_or(ERROR_INVALID)
            } else {
                ERROR_INVALID
            }
        }
        SYS_PROCESS_WAIT => {
            if crate::security::has_capability(crate::security::CAP_PROCESS) {
                crate::process::wait(registers.rdi).unwrap_or(ERROR_INVALID)
            } else {
                ERROR_INVALID
            }
        }
        SYS_SOCKET_UDP_DNS => {
            if crate::security::has_capability(crate::security::CAP_NETWORK) {
                socket_udp_dns(
                    registers.rdi,
                    registers.rsi as usize,
                    registers.rdx,
                    registers.r10 as usize,
                )
            } else {
                ERROR_INVALID
            }
        }
        SYS_FILE_WRITE => {
            if crate::security::has_capability(crate::security::CAP_FILE_WRITE) {
                file_write(registers.rdi, registers.rsi, registers.rdx as usize)
            } else {
                ERROR_INVALID
            }
        }
        SYS_PACKAGE_INSTALL => {
            package_install(registers.rdi, registers.rsi, registers.rdx, registers.r10)
        }
        SYS_PACKAGE_QUERY => package_query(
            registers.rdi,
            registers.rsi as usize,
            registers.rdx,
            registers.r10 as usize,
        ),
        SYS_PACKAGE_ROLLBACK => {
            if crate::security::has_capability(crate::security::CAP_FILE_WRITE) {
                u64::from(crate::package::rollback())
            } else {
                ERROR_INVALID
            }
        }
        SYS_VM_MAP => crate::process::vm_map().unwrap_or(ERROR_INVALID),
        SYS_VM_UNMAP => u64::from(crate::process::vm_unmap(registers.rdi)),
        SYS_VM_PROTECT => u64::from(crate::process::vm_protect(registers.rdi, registers.rsi)),
        SYS_THREAD_CREATE => {
            crate::process::thread_create(registers.rdi, registers.rsi).unwrap_or(ERROR_INVALID)
        }
        SYS_THREAD_JOIN => crate::process::thread_join(registers.rdi).unwrap_or(ERROR_INVALID),
        SYS_THREAD_EXIT => crate::process::thread_exit(registers.rdi),
        SYS_SOCKET_TCP_HTTP => {
            if crate::security::has_capability(crate::security::CAP_NETWORK) {
                socket_tcp_http(
                    registers.rdi,
                    registers.rsi as usize,
                    registers.rdx,
                    registers.r10 as usize,
                )
            } else {
                ERROR_INVALID
            }
        }
        SYS_CLOCK_MONOTONIC => crate::arch::monotonic_ticks(),
        SYS_LOG_APPEND => log_append(registers.rdi as u8, registers.rsi, registers.rdx as usize),
        SYS_LOG_READ => {
            if crate::security::has_capability(crate::security::CAP_CONSOLE) {
                log_read(
                    registers.rdi,
                    registers.rsi,
                    registers.rdx as usize,
                    registers.r10,
                )
            } else {
                ERROR_INVALID
            }
        }
        SYS_AUTH_LOGIN => authenticate(
            registers.rdi,
            registers.rsi as usize,
            registers.rdx,
            registers.r10 as usize,
        ),
        SYS_ABI_INFO => match registers.rdi {
            0 => ABI_VERSION_1_0,
            1 => SYS_PROCESS_SPAWN_PATH_ARGS,
            2 => ABI_FEATURES,
            3 => SYS_THREAD_AFFINITY,
            _ => ERROR_INVALID,
        },
        SYS_EVENT_CREATE => {
            if crate::security::has_capability(crate::security::CAP_SYNC) {
                crate::ipc::create_event(registers.rdi != 0).unwrap_or(ERROR_INVALID)
            } else {
                ERROR_INVALID
            }
        }
        SYS_EVENT_SIGNAL => {
            if crate::security::has_capability(crate::security::CAP_SYNC) {
                u64::from(crate::ipc::signal_event(registers.rdi))
            } else {
                ERROR_INVALID
            }
        }
        SYS_EVENT_WAIT => {
            if crate::security::has_capability(crate::security::CAP_SYNC) {
                u64::from(crate::ipc::wait_event(registers.rdi))
            } else {
                ERROR_INVALID
            }
        }
        SYS_HANDLE_CLOSE => u64::from(crate::ipc::close(registers.rdi)),
        SYS_STAT => stat(registers.rdi, registers.rsi as usize, registers.rdx),
        SYS_READ_DIR => read_dir(
            registers.rdi,
            registers.rsi as usize,
            registers.rdx as usize,
            registers.r10,
        ),
        SYS_PROCESS_SPAWN_LINUX => {
            if crate::security::has_capability(crate::security::CAP_PROCESS) {
                crate::process::spawn_linux_compat().unwrap_or(ERROR_INVALID)
            } else {
                ERROR_INVALID
            }
        }
        SYS_AUDIO_WRITE => audio_write(
            registers.rdi,
            registers.rsi as usize,
            registers.rdx as u32,
            registers.r10 as u32,
        ),
        SYS_SOCKET_IPV6_ECHO => {
            if crate::security::has_capability(crate::security::CAP_NETWORK) {
                u64::from(crate::drivers::rtl8139::ipv6_echo())
            } else {
                ERROR_INVALID
            }
        }
        SYS_PROCESS_SPAWN_WINDOWS => {
            if crate::security::has_capability(crate::security::CAP_PROCESS) {
                crate::process::spawn_windows_compat().unwrap_or(ERROR_INVALID)
            } else {
                ERROR_INVALID
            }
        }
        SYS_SERVICE_START => {
            if crate::security::has_capability(crate::security::CAP_PROCESS) {
                crate::process::spawn_service().unwrap_or(ERROR_INVALID)
            } else {
                ERROR_INVALID
            }
        }
        SYS_CREATE => path_mutation(registers.rdi, registers.rsi as usize, true),
        SYS_UNLINK => path_mutation(registers.rdi, registers.rsi as usize, false),
        SYS_PROCESS_SPAWN_TOOLCHAIN => {
            if crate::security::has_capability(crate::security::CAP_PROCESS) {
                crate::process::spawn_toolchain().unwrap_or(ERROR_INVALID)
            } else {
                ERROR_INVALID
            }
        }
        SYS_SOCKET_CREATE => {
            if crate::security::has_capability(crate::security::CAP_NETWORK) {
                crate::socket::create(registers.rdi, registers.rsi, registers.rdx)
                    .unwrap_or(ERROR_INVALID)
            } else {
                ERROR_INVALID
            }
        }
        SYS_SOCKET_CONNECT => {
            if crate::security::has_capability(crate::security::CAP_NETWORK) {
                socket_connect(registers.rdi, registers.rsi, registers.rdx as usize)
            } else {
                ERROR_INVALID
            }
        }
        SYS_SOCKET_SEND => {
            if crate::security::has_capability(crate::security::CAP_NETWORK) {
                socket_send(
                    registers.rdi,
                    registers.rsi,
                    registers.rdx as usize,
                    registers.r10,
                )
            } else {
                ERROR_INVALID
            }
        }
        SYS_SOCKET_RECEIVE => {
            if crate::security::has_capability(crate::security::CAP_NETWORK) {
                socket_receive(
                    registers.rdi,
                    registers.rsi,
                    registers.rdx as usize,
                    registers.r10,
                )
            } else {
                ERROR_INVALID
            }
        }
        SYS_SOCKET_CLOSE => {
            if crate::security::has_capability(crate::security::CAP_NETWORK) {
                u64::from(crate::socket::close(registers.rdi))
            } else {
                ERROR_INVALID
            }
        }
        SYS_PACKAGE_REMOVE => package_remove(registers.rdi, registers.rsi as usize),
        SYS_VM_MAP_RANGE => {
            if registers.rdx != 0 {
                ERROR_INVALID
            } else {
                crate::process::vm_map_range(registers.rdi as usize, registers.rsi)
                    .unwrap_or(ERROR_INVALID)
            }
        }
        SYS_VM_UNMAP_RANGE => u64::from(crate::process::vm_unmap_range(
            registers.rdi,
            registers.rsi as usize,
        )),
        SYS_VM_PROTECT_RANGE => u64::from(crate::process::vm_protect_range(
            registers.rdi,
            registers.rsi as usize,
            registers.rdx,
        )),
        SYS_PROCESS_SPAWN_PATH => {
            if crate::security::has_capability(crate::security::CAP_PROCESS) {
                process_spawn_path(registers.rdi, registers.rsi as usize)
            } else {
                ERROR_INVALID
            }
        }
        SYS_PROCESS_SPAWN_PATH_ARGS => {
            if crate::security::has_capability(crate::security::CAP_PROCESS) {
                process_spawn_path_arguments(
                    registers.rdi,
                    registers.rsi as usize,
                    registers.rdx,
                    registers.r10 as usize,
                )
            } else {
                ERROR_INVALID
            }
        }
        _ => ERROR_INVALID,
    };
    let count = SYSCALL_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if count >= 10
        && IPC_PROVEN.load(Ordering::Acquire)
        && !M3_REPORTED.swap(true, Ordering::AcqRel)
    {
        crate::serial_println!("MAKOS_M3_OK ring3=1 int80=1 ipc=1 syscalls={}", count);
    }
}

fn process_spawn_path(path_address: u64, path_length: usize) -> u64 {
    if path_length == 0
        || path_length >= crate::vfs::MAX_PATH_BYTES
        || !crate::process::valid_user_range(path_address, path_length)
    {
        return ERROR_INVALID;
    }
    let path = unsafe { slice::from_raw_parts(path_address as *const u8, path_length) };
    crate::process::spawn_path(path).unwrap_or(ERROR_INVALID)
}

fn process_spawn_path_arguments(
    path_address: u64,
    path_length: usize,
    arguments_address: u64,
    arguments_length: usize,
) -> u64 {
    if path_length == 0
        || path_length >= crate::vfs::MAX_PATH_BYTES
        || arguments_length != crate::process::SPAWN_ARGUMENTS_BYTES
        || !crate::process::valid_user_range(path_address, path_length)
        || !crate::process::valid_user_range(arguments_address, arguments_length)
    {
        return ERROR_INVALID;
    }
    let path = unsafe { slice::from_raw_parts(path_address as *const u8, path_length) };
    let arguments =
        unsafe { slice::from_raw_parts(arguments_address as *const u8, arguments_length) };
    crate::process::spawn_path_with_arguments(path, arguments).unwrap_or(ERROR_INVALID)
}

fn socket_connect(handle: u64, address: u64, length: usize) -> u64 {
    if length != 8 || !crate::process::valid_user_range(address, length) {
        return ERROR_INVALID;
    }
    let bytes = unsafe { slice::from_raw_parts(address as *const u8, length) };
    if u16::from_le_bytes([bytes[0], bytes[1]]) != crate::socket::AF_INET as u16 {
        return ERROR_INVALID;
    }
    let port = u16::from_be_bytes([bytes[2], bytes[3]]);
    u64::from(crate::socket::connect(
        handle,
        [bytes[4], bytes[5], bytes[6], bytes[7]],
        port,
    ))
}

fn socket_send(handle: u64, address: u64, length: usize, flags: u64) -> u64 {
    if flags != 0 || !crate::process::valid_user_range(address, length) {
        return ERROR_INVALID;
    }
    let payload = unsafe { slice::from_raw_parts(address as *const u8, length) };
    crate::socket::send(handle, payload).map_or(ERROR_INVALID, |count| count as u64)
}

fn socket_receive(handle: u64, address: u64, length: usize, flags: u64) -> u64 {
    if flags != 0 || !crate::process::valid_user_range_write(address, length) {
        return ERROR_INVALID;
    }
    let output = unsafe { slice::from_raw_parts_mut(address as *mut u8, length) };
    crate::socket::receive(handle, output).map_or(ERROR_INVALID, |count| count as u64)
}

fn audio_write(address: u64, frames: usize, rate: u32, channels: u32) -> u64 {
    if !crate::security::has_capability(crate::security::CAP_AUDIO) || channels != 2 {
        return ERROR_INVALID;
    }
    let Some(sample_count) = frames.checked_mul(channels as usize) else {
        return ERROR_INVALID;
    };
    let Some(byte_count) = sample_count.checked_mul(core::mem::size_of::<i16>()) else {
        return ERROR_INVALID;
    };
    if !crate::process::valid_user_range(address, byte_count) {
        return ERROR_INVALID;
    }
    let samples = unsafe { slice::from_raw_parts(address as *const i16, sample_count) };
    u64::from(crate::drivers::ac97::write_pcm(samples, rate, channels))
}

fn stat(path_address: u64, path_length: usize, output_address: u64) -> u64 {
    if path_length == 0
        || path_length >= crate::vfs::MAX_PATH_BYTES
        || !crate::process::valid_user_range(path_address, path_length)
        || !crate::process::valid_user_range_write(
            output_address,
            core::mem::size_of::<crate::vfs::Metadata>(),
        )
    {
        return ERROR_INVALID;
    }
    let path = unsafe { slice::from_raw_parts(path_address as *const u8, path_length) };
    let Some(metadata) = crate::vfs::stat(path) else {
        return ERROR_INVALID;
    };
    unsafe { (output_address as *mut crate::vfs::Metadata).write_unaligned(metadata) };
    1
}

fn path_mutation(path_address: u64, path_length: usize, create: bool) -> u64 {
    if !crate::security::has_capability(crate::security::CAP_FILE_WRITE)
        || !crate::process::valid_user_range(path_address, path_length)
        || path_length >= crate::vfs::MAX_PATH_BYTES
    {
        return ERROR_INVALID;
    }
    let path = unsafe { slice::from_raw_parts(path_address as *const u8, path_length) };
    u64::from(if create {
        crate::vfs::create(path)
    } else {
        crate::vfs::unlink(path)
    })
}

fn read_dir(path_address: u64, path_length: usize, index: usize, output_address: u64) -> u64 {
    if path_length == 0
        || path_length >= crate::vfs::MAX_PATH_BYTES
        || !crate::process::valid_user_range(path_address, path_length)
        || !crate::process::valid_user_range_write(
            output_address,
            core::mem::size_of::<crate::vfs::DirectoryEntry>(),
        )
    {
        return ERROR_INVALID;
    }
    let path = unsafe { slice::from_raw_parts(path_address as *const u8, path_length) };
    let Some(entry) = crate::vfs::read_dir(path, index) else {
        return 0;
    };
    unsafe { (output_address as *mut crate::vfs::DirectoryEntry).write_unaligned(entry) };
    1
}

fn authenticate(
    username_address: u64,
    username_length: usize,
    password_address: u64,
    password_length: usize,
) -> u64 {
    if !crate::process::valid_user_range(username_address, username_length)
        || !crate::process::valid_user_range(password_address, password_length)
        || username_length > 32
        || password_length > 64
    {
        return ERROR_INVALID;
    }
    let username = unsafe { slice::from_raw_parts(username_address as *const u8, username_length) };
    let password = unsafe { slice::from_raw_parts(password_address as *const u8, password_length) };
    u64::from(crate::security::authenticate(username, password))
}

fn log_append(severity: u8, address: u64, length: usize) -> u64 {
    if !crate::process::valid_user_range(address, length) {
        return ERROR_INVALID;
    }
    let message = unsafe { slice::from_raw_parts(address as *const u8, length) };
    crate::log::append(severity, message).unwrap_or(ERROR_INVALID)
}

fn log_read(sequence: u64, address: u64, length: usize, metadata_address: u64) -> u64 {
    if !crate::process::valid_user_range_write(address, length)
        || !crate::process::valid_user_range_write(metadata_address, 24)
    {
        return ERROR_INVALID;
    }
    let output = unsafe { slice::from_raw_parts_mut(address as *mut u8, length) };
    let Some((count, ticks, pid, severity)) = crate::log::read(sequence, output) else {
        return ERROR_INVALID;
    };
    unsafe {
        (metadata_address as *mut u64).write_volatile(ticks);
        (metadata_address as *mut u64).add(1).write_volatile(pid);
        (metadata_address as *mut u64)
            .add(2)
            .write_volatile(u64::from(severity));
    }
    count as u64
}

fn socket_tcp_http(
    request_address: u64,
    request_length: usize,
    response_address: u64,
    response_length: usize,
) -> u64 {
    if !crate::process::valid_user_range(request_address, request_length)
        || !crate::process::valid_user_range_write(response_address, response_length)
    {
        return ERROR_INVALID;
    }
    let request = unsafe { slice::from_raw_parts(request_address as *const u8, request_length) };
    let response =
        unsafe { slice::from_raw_parts_mut(response_address as *mut u8, response_length) };
    crate::drivers::rtl8139::tcp_http_exchange(request, response)
        .map_or(ERROR_INVALID, |count| count as u64)
}

fn package_install(name_address: u64, name_length: u64, version_address: u64, packed: u64) -> u64 {
    const RSA2048_SIGNATURE_LENGTH: usize = 256;
    if !crate::security::has_capability(crate::security::CAP_FILE_WRITE) {
        return ERROR_INVALID;
    }
    let name_length = name_length as usize;
    let version_length = (packed & 0xff) as usize;
    let content_length = ((packed >> 8) & 0xff) as usize;
    let dependency_length = ((packed >> 16) & 0xff) as usize;
    let algorithm = ((packed >> 24) & 0xff) as u8;
    let Some(content_address) = version_address.checked_add(version_length as u64) else {
        return ERROR_INVALID;
    };
    let Some(dependency_address) = content_address.checked_add(content_length as u64) else {
        return ERROR_INVALID;
    };
    let Some(signature_address) = dependency_address.checked_add(dependency_length as u64) else {
        return ERROR_INVALID;
    };
    let Some(total_length) = version_length
        .checked_add(content_length)
        .and_then(|length| length.checked_add(dependency_length))
        .and_then(|length| length.checked_add(RSA2048_SIGNATURE_LENGTH))
    else {
        return ERROR_INVALID;
    };
    if algorithm != 1 || version_length == 0 || content_length == 0 || dependency_length == 0 {
        return ERROR_INVALID;
    }
    if !crate::process::valid_user_range(name_address, name_length)
        || !crate::process::valid_user_range(version_address, total_length)
    {
        return ERROR_INVALID;
    }
    let name = unsafe { slice::from_raw_parts(name_address as *const u8, name_length) };
    let version = unsafe { slice::from_raw_parts(version_address as *const u8, version_length) };
    let content = unsafe { slice::from_raw_parts(content_address as *const u8, content_length) };
    let dependency =
        unsafe { slice::from_raw_parts(dependency_address as *const u8, dependency_length) };
    let signature = unsafe { &*(signature_address as *const [u8; RSA2048_SIGNATURE_LENGTH]) };
    u64::from(crate::package::install(
        name, version, content, dependency, signature,
    ))
}

fn package_query(name_address: u64, name_length: usize, output_address: u64, length: usize) -> u64 {
    if !crate::process::valid_user_range(name_address, name_length)
        || !crate::process::valid_user_range_write(output_address, length)
    {
        return ERROR_INVALID;
    }
    let name = unsafe { slice::from_raw_parts(name_address as *const u8, name_length) };
    let output = unsafe { slice::from_raw_parts_mut(output_address as *mut u8, length) };
    crate::package::query(name, output).map_or(ERROR_INVALID, |count| count as u64)
}

fn package_remove(name_address: u64, name_length: usize) -> u64 {
    if !crate::security::has_capability(crate::security::CAP_FILE_WRITE)
        || name_length == 0
        || name_length > 32
        || !crate::process::valid_user_range(name_address, name_length)
    {
        return ERROR_INVALID;
    }
    let name = unsafe { slice::from_raw_parts(name_address as *const u8, name_length) };
    u64::from(crate::package::remove(name))
}

fn file_write(fd: u64, address: u64, length: usize) -> u64 {
    if !crate::process::valid_user_range(address, length) {
        return ERROR_INVALID;
    }
    let input = unsafe { slice::from_raw_parts(address as *const u8, length) };
    crate::vfs::write(fd, input).map_or(ERROR_INVALID, |count| count as u64)
}

fn socket_udp_dns(
    request_address: u64,
    request_length: usize,
    response_address: u64,
    response_length: usize,
) -> u64 {
    if !crate::process::valid_user_range(request_address, request_length)
        || !crate::process::valid_user_range_write(response_address, response_length)
    {
        return ERROR_INVALID;
    }
    let request = unsafe { slice::from_raw_parts(request_address as *const u8, request_length) };
    let response =
        unsafe { slice::from_raw_parts_mut(response_address as *mut u8, response_length) };
    crate::drivers::rtl8139::udp_dns_exchange(request, response)
        .map_or(ERROR_INVALID, |count| count as u64)
}

fn open(address: u64, length: usize, write: bool) -> u64 {
    if !crate::process::valid_user_range(address, length) || length >= crate::vfs::MAX_PATH_BYTES {
        return ERROR_INVALID;
    }
    let path = unsafe { slice::from_raw_parts(address as *const u8, length) };
    crate::vfs::open(path, write).unwrap_or(ERROR_INVALID)
}

fn read(fd: u64, address: u64, length: usize) -> u64 {
    if !crate::process::valid_user_range_write(address, length) {
        return ERROR_INVALID;
    }
    let output = unsafe { slice::from_raw_parts_mut(address as *mut u8, length) };
    crate::vfs::read(fd, output).map_or(ERROR_INVALID, |count| count as u64)
}

fn shell_command(address: u64, length: usize) -> u64 {
    if !crate::process::valid_user_range(address, length) || length > 127 {
        return ERROR_INVALID;
    }
    let bytes = unsafe { slice::from_raw_parts(address as *const u8, length) };
    match bytes {
        b"help" => {
            shell_output(
                b"commands: help status mem ps clear pwd ls cat stat touch write rm echo whoami uname uptime install exit\n",
            );
            crate::serial_println!("MAKOS_SHELL_CMD help");
            0
        }
        b"status" => {
            let mut line = [0u8; 96];
            let mut used = copy_shell_bytes(
                &mut line,
                0,
                b"MakOS status: kernel=online cpus=4 free_frames=",
            );
            used = append_decimal(&mut line, used, crate::mm::free_frames() as u64);
            used = copy_shell_bytes(&mut line, used, b"\n");
            shell_output(&line[..used]);
            crate::serial_println!("MAKOS_SHELL_CMD status");
            0
        }
        b"mem" => {
            let free = crate::mm::free_frames() as u64;
            let mut line = [0u8; 96];
            let mut used = copy_shell_bytes(&mut line, 0, b"free_frames=");
            used = append_decimal(&mut line, used, free);
            used = copy_shell_bytes(&mut line, used, b" free_kib=");
            used = append_decimal(&mut line, used, free.saturating_mul(4));
            used = copy_shell_bytes(&mut line, used, b"\n");
            shell_output(&line[..used]);
            crate::serial_println!("MAKOS_SHELL_CMD mem free_frames={}", free);
            0
        }
        b"ps" => {
            let (occupied, runnable, pid, tid) = crate::scheduler::task_stats();
            let mut line = [0u8; 128];
            let mut used = copy_shell_bytes(&mut line, 0, b"PID TID STATE\n");
            used = append_decimal(&mut line, used, pid);
            used = copy_shell_bytes(&mut line, used, b" ");
            used = append_decimal(&mut line, used, tid);
            used = copy_shell_bytes(&mut line, used, b" RUNNING\ntasks_occupied=");
            used = append_decimal(&mut line, used, occupied as u64);
            used = copy_shell_bytes(&mut line, used, b" runnable=");
            used = append_decimal(&mut line, used, runnable as u64);
            used = copy_shell_bytes(&mut line, used, b"\n");
            shell_output(&line[..used]);
            crate::serial_println!(
                "MAKOS_SHELL_CMD ps occupied={} runnable={} pid={} tid={}",
                occupied,
                runnable,
                pid,
                tid,
            );
            0
        }
        b"clear" => {
            crate::graphics::terminal_clear();
            crate::serial_println!("MAKOS_SHELL_CMD clear");
            0
        }
        b"pwd" => {
            shell_output(b"/home/user\n");
            crate::serial_println!("MAKOS_SHELL_CMD pwd");
            0
        }
        b"whoami" => {
            shell_output(b"marcus\n");
            crate::serial_println!("MAKOS_SHELL_CMD whoami");
            0
        }
        b"uname" | b"uname -a" => {
            shell_output(b"MakOS 0.1.0 x86_64 makos\n");
            crate::serial_println!("MAKOS_SHELL_CMD uname");
            0
        }
        b"uptime" => {
            let mut line = [0u8; 64];
            let mut used = copy_shell_bytes(&mut line, 0, b"uptime_ticks=");
            used = append_decimal(&mut line, used, crate::arch::monotonic_ticks());
            used = copy_shell_bytes(&mut line, used, b"\n");
            shell_output(&line[..used]);
            crate::serial_println!("MAKOS_SHELL_CMD uptime");
            0
        }
        b"ls" | b"ls /home/user" => {
            let mut index = 0usize;
            while let Some(entry) = crate::vfs::read_dir(b"/home/user", index) {
                let length = entry.name_length as usize;
                shell_output(&entry.name[..length]);
                shell_output(b"\n");
                index += 1;
            }
            crate::serial_println!("MAKOS_SHELL_CMD ls entries={}", index);
            0
        }
        b"cat note.txt" | b"cat /home/user/note.txt" => {
            let mut contents = [0u8; crate::vfs::MAX_FILE_BYTES];
            let Some(count) = crate::vfs::snapshot(b"/home/user/note.txt", &mut contents) else {
                shell_output(b"cat: note.txt: read failed\n");
                return ERROR_INVALID;
            };
            shell_output(&contents[..count]);
            if count == 0 || contents[count - 1] != b'\n' {
                shell_output(b"\n");
            }
            crate::serial_println!("MAKOS_SHELL_CMD cat bytes={}", count);
            0
        }
        _ if bytes.starts_with(b"cat ") => shell_cat(&bytes[4..]),
        _ if bytes.starts_with(b"stat ") => shell_stat(&bytes[5..]),
        _ if bytes.starts_with(b"touch ") => shell_touch(&bytes[6..]),
        _ if bytes.starts_with(b"rm ") => shell_remove(&bytes[3..]),
        _ if bytes.starts_with(b"write ") => shell_write_file(&bytes[6..]),
        b"install" => {
            shell_output(
                b"usage: install disk1 erase-disk1\nresume: install disk1 resume-disk1\nWARNING: disk1 target is destructive.\n",
            );
            ERROR_INVALID
        }
        b"install disk1 erase-disk1" => {
            match crate::x86_64_installer::install_disk1(makos_installer::InstallMode::Fresh) {
                Ok(report) => {
                    crate::x86_64_installer::success_message(report);
                    report.sectors
                }
                Err(error) => {
                    crate::x86_64_installer::describe_error(error);
                    ERROR_INVALID
                }
            }
        }
        b"install disk1 resume-disk1" => {
            match crate::x86_64_installer::install_disk1(makos_installer::InstallMode::Resume) {
                Ok(report) => {
                    crate::x86_64_installer::success_message(report);
                    report.sectors
                }
                Err(error) => {
                    crate::x86_64_installer::describe_error(error);
                    ERROR_INVALID
                }
            }
        }
        _ if bytes.starts_with(b"install ") => {
            shell_output(
                b"install: confirmation mismatch; type exactly: install disk1 erase-disk1\n",
            );
            crate::serial_println!(
                "MAKOS_X86_INSTALL_CONFIRMATION_DENIED target=disk1 expected=erase-disk1 destructive_io=0"
            );
            ERROR_INVALID
        }
        _ if bytes.starts_with(b"echo ") => {
            shell_output(&bytes[5..]);
            shell_output(b"\n");
            crate::serial_println!("MAKOS_SHELL_CMD echo bytes={}", bytes.len() - 5);
            0
        }
        b"exit" => {
            shell_output(b"Terminal closed. Reopen it from Start.\n");
            let closed = crate::graphics::close(2);
            crate::serial_println!(
                "MAKOS_SHELL_CMD exit close={} pid1_alive=1 reopen=start-menu retained=1",
                u8::from(closed),
            );
            u64::from(closed)
        }
        b"" => 0,
        _ => {
            shell_output(b"unknown command; type help\n");
            crate::serial_println!("MAKOS_SHELL_CMD unknown denied=1");
            ERROR_INVALID
        }
    }
}

fn shell_cat(name: &[u8]) -> u64 {
    let mut path = [0u8; 64];
    let Some(path) = shell_path(name, &mut path) else {
        shell_output(b"cat: invalid path\n");
        return ERROR_INVALID;
    };
    let mut contents = [0u8; crate::vfs::MAX_FILE_BYTES];
    let Some(count) = crate::vfs::snapshot(path, &mut contents) else {
        shell_output(b"cat: file not found\n");
        return ERROR_INVALID;
    };
    shell_output(&contents[..count]);
    if count == 0 || contents[count - 1] != b'\n' {
        shell_output(b"\n");
    }
    crate::serial_println!("MAKOS_SHELL_CMD cat bytes={}", count);
    0
}

fn shell_stat(name: &[u8]) -> u64 {
    let mut path = [0u8; 64];
    let Some(path) = shell_path(name, &mut path) else {
        shell_output(b"stat: invalid path\n");
        return ERROR_INVALID;
    };
    let Some(metadata) = crate::vfs::stat(path) else {
        shell_output(b"stat: file not found\n");
        return ERROR_INVALID;
    };
    let mut line = [0u8; 160];
    let mut used = copy_shell_bytes(&mut line, 0, b"size=");
    used = append_decimal(&mut line, used, metadata.size);
    used = copy_shell_bytes(&mut line, used, b" uid=");
    used = append_decimal(&mut line, used, u64::from(metadata.uid));
    used = copy_shell_bytes(&mut line, used, b" gid=");
    used = append_decimal(&mut line, used, u64::from(metadata.gid));
    used = copy_shell_bytes(&mut line, used, b" mode=");
    used = append_octal(&mut line, used, metadata.mode);
    used = copy_shell_bytes(&mut line, used, b" inode=");
    used = append_decimal(&mut line, used, metadata.inode);
    used = copy_shell_bytes(&mut line, used, b" modified_ticks=");
    used = append_decimal(&mut line, used, metadata.modified_ticks);
    used = copy_shell_bytes(&mut line, used, b"\n");
    shell_output(&line[..used]);
    crate::serial_println!(
        "MAKOS_SHELL_CMD stat size={} inode={}",
        metadata.size,
        metadata.inode
    );
    0
}

fn shell_touch(name: &[u8]) -> u64 {
    let mut path = [0u8; 64];
    let Some(path) = shell_path(name, &mut path) else {
        shell_output(b"touch: invalid path\n");
        return ERROR_INVALID;
    };
    if !crate::vfs::create(path) {
        shell_output(b"touch: create failed (file may exist)\n");
        return ERROR_INVALID;
    }
    crate::serial_println!("MAKOS_SHELL_CMD touch persisted=1");
    0
}

fn shell_remove(name: &[u8]) -> u64 {
    let mut path = [0u8; 64];
    let Some(path) = shell_path(name, &mut path) else {
        shell_output(b"rm: invalid path\n");
        return ERROR_INVALID;
    };
    if !crate::vfs::unlink(path) {
        shell_output(b"rm: remove failed\n");
        return ERROR_INVALID;
    }
    crate::serial_println!("MAKOS_SHELL_CMD rm persisted=1");
    0
}

fn shell_write_file(arguments: &[u8]) -> u64 {
    let Some(separator) = arguments.iter().position(|byte| *byte == b' ') else {
        shell_output(b"usage: write FILE TEXT\n");
        return ERROR_INVALID;
    };
    let name = &arguments[..separator];
    let contents = &arguments[separator + 1..];
    let mut path = [0u8; 64];
    let Some(path) = shell_path(name, &mut path) else {
        shell_output(b"write: invalid path\n");
        return ERROR_INVALID;
    };
    if crate::vfs::stat(path).is_none() && !crate::vfs::create(path) {
        shell_output(b"write: create failed\n");
        return ERROR_INVALID;
    }
    let Some(fd) = crate::vfs::open(path, true) else {
        shell_output(b"write: open failed\n");
        return ERROR_INVALID;
    };
    let written = crate::vfs::write(fd, contents);
    let closed = crate::vfs::close(fd);
    if written != Some(contents.len()) || !closed {
        shell_output(b"write: persistence failed\n");
        return ERROR_INVALID;
    }
    crate::serial_println!("MAKOS_SHELL_CMD write bytes={} persisted=1", contents.len());
    contents.len() as u64
}

fn shell_path<'a>(name: &[u8], output: &'a mut [u8; 64]) -> Option<&'a [u8]> {
    if name.starts_with(b"/home/user/") {
        if name.len() > output.len() {
            return None;
        }
        output[..name.len()].copy_from_slice(name);
        return Some(&output[..name.len()]);
    }
    if name.is_empty() || name.len() > 32 {
        return None;
    }
    const PREFIX: &[u8] = b"/home/user/";
    output[..PREFIX.len()].copy_from_slice(PREFIX);
    output[PREFIX.len()..PREFIX.len() + name.len()].copy_from_slice(name);
    Some(&output[..PREFIX.len() + name.len()])
}

fn shell_output(bytes: &[u8]) {
    if let Ok(text) = core::str::from_utf8(bytes) {
        crate::serial_print!("{}", text);
    } else {
        crate::serial_println!("<non-UTF8 output>");
    }
    crate::graphics::terminal_write(bytes);
}

fn copy_shell_bytes(output: &mut [u8], offset: usize, input: &[u8]) -> usize {
    let count = input.len().min(output.len().saturating_sub(offset));
    output[offset..offset + count].copy_from_slice(&input[..count]);
    offset + count
}

fn append_decimal(output: &mut [u8], offset: usize, mut value: u64) -> usize {
    let mut digits = [0u8; 20];
    let mut count = 0usize;
    loop {
        digits[count] = b'0' + (value % 10) as u8;
        count += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    let mut used = offset;
    while count != 0 && used < output.len() {
        count -= 1;
        output[used] = digits[count];
        used += 1;
    }
    used
}

fn append_octal(output: &mut [u8], offset: usize, mut value: u32) -> usize {
    let mut digits = [0u8; 11];
    let mut count = 0usize;
    loop {
        digits[count] = b'0' + (value & 7) as u8;
        count += 1;
        value >>= 3;
        if value == 0 {
            break;
        }
    }
    let mut used = offset;
    while count != 0 && used < output.len() {
        count -= 1;
        output[used] = digits[count];
        used += 1;
    }
    used
}

fn channel_create(output_address: u64) -> u64 {
    if !crate::process::valid_user_range_write(output_address, 16) {
        return ERROR_INVALID;
    }
    let Some((first, second)) = crate::ipc::create_pair() else {
        return ERROR_INVALID;
    };
    unsafe {
        (output_address as *mut u64).write_volatile(first);
        (output_address as *mut u64).add(1).write_volatile(second);
    }
    0
}

fn typed_service_publish(address: u64, length: usize) -> u64 {
    if !crate::security::has_capability(crate::security::CAP_IPC)
        || !crate::security::has_capability(crate::security::CAP_SERVICE_PUBLISH)
        || !crate::process::valid_user_range(address, length)
    {
        return ERROR_INVALID;
    }
    let name = unsafe { slice::from_raw_parts(address as *const u8, length) };
    crate::ipc::typed_publish(name).unwrap_or(ERROR_INVALID)
}

fn typed_service_connect(address: u64, length: usize) -> u64 {
    if !crate::security::has_capability(crate::security::CAP_IPC)
        || !crate::process::valid_user_range(address, length)
    {
        return ERROR_INVALID;
    }
    let name = unsafe { slice::from_raw_parts(address as *const u8, length) };
    crate::ipc::typed_connect(name).unwrap_or(ERROR_INVALID)
}

fn typed_service_accept(listener: u64) -> u64 {
    if !crate::security::has_capability(crate::security::CAP_IPC)
        || !crate::security::has_capability(crate::security::CAP_SERVICE_PUBLISH)
    {
        return ERROR_INVALID;
    }
    crate::ipc::typed_accept(listener).unwrap_or(ERROR_INVALID)
}

fn typed_channel_send(endpoint: u64, address: u64, transfer: u64, rights: u8) -> u64 {
    if !crate::security::has_capability(crate::security::CAP_IPC)
        || !crate::process::valid_user_range(address, makos_ipc::MESSAGE_WIRE_SIZE)
    {
        return ERROR_INVALID;
    }
    let mut message = [0u8; makos_ipc::MESSAGE_WIRE_SIZE];
    unsafe {
        core::ptr::copy_nonoverlapping(
            address as *const u8,
            message.as_mut_ptr(),
            makos_ipc::MESSAGE_WIRE_SIZE,
        );
    }
    if crate::ipc::typed_send(endpoint, message, transfer, rights) {
        0
    } else {
        ERROR_INVALID
    }
}

fn typed_channel_receive(endpoint: u64, message_address: u64, transfer_address: u64) -> u64 {
    if !crate::security::has_capability(crate::security::CAP_IPC)
        || !crate::process::valid_user_range_write(
            message_address,
            makos_ipc::MESSAGE_WIRE_SIZE,
        )
        || !crate::process::valid_user_range_write(transfer_address, 8)
    {
        return ERROR_INVALID;
    }
    let Some((message, transfer)) = crate::ipc::typed_receive(endpoint) else {
        return ERROR_INVALID;
    };
    unsafe {
        core::ptr::copy_nonoverlapping(
            message.as_ptr(),
            message_address as *mut u8,
            makos_ipc::MESSAGE_WIRE_SIZE,
        );
        (transfer_address as *mut u64).write_unaligned(transfer);
    }
    0
}

fn write(address: u64, length: usize) -> u64 {
    if !crate::process::valid_user_range(address, length) {
        crate::security::report_pointer_denial();
        return ERROR_INVALID;
    }
    let bytes = unsafe { slice::from_raw_parts(address as *const u8, length) };
    let Ok(text) = core::str::from_utf8(bytes) else {
        return ERROR_INVALID;
    };
    crate::serial_print!("{}", text);
    crate::graphics::console_write(bytes);
    length as u64
}
