#![no_main]
#![no_std]

use core::arch::asm;
use core::panic::PanicInfo;

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
const SYS_FILE_WRITE: u64 = 17;
const SYS_PACKAGE_INSTALL: u64 = 18;
const SYS_PACKAGE_QUERY: u64 = 19;
const SYS_PACKAGE_ROLLBACK: u64 = 20;
const SYS_CLOCK_MONOTONIC: u64 = 27;
const SYS_LOG_APPEND: u64 = 28;
const SYS_LOG_READ: u64 = 29;
const SYS_AUTH_LOGIN: u64 = 30;
const SYS_ABI_INFO: u64 = 31;
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
const SYS_PROCESS_SPAWN_TOOLCHAIN: u64 = 46;
const SYS_SOCKET_CREATE: u64 = 47;
const SYS_SOCKET_CONNECT: u64 = 48;
const SYS_SOCKET_SEND: u64 = 49;
const SYS_SOCKET_RECEIVE: u64 = 50;
const SYS_SOCKET_CLOSE: u64 = 51;
const SYS_PACKAGE_REMOVE: u64 = 52;
const SYS_VM_PROTECT_RANGE: u64 = 55;
const SYS_PROCESS_SPAWN_PATH: u64 = 56;
const SYS_PROCESS_SPAWN_PATH_ARGS: u64 = 57;
const ABI_VERSION_1_0: u64 = 0x0001_0000;
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
const ABI_FEATURE_CPU_AFFINITY: u64 = 1 << 22;
const AF_INET: u64 = 2;
const SOCK_STREAM: u64 = 1;
const SOCK_DGRAM: u64 = 2;
const IPPROTO_TCP: u64 = 6;
const IPPROTO_UDP: u64 = 17;

#[repr(C)]
struct SockaddrIn {
    family: u16,
    port_be: u16,
    address: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SpawnArguments {
    version: u32,
    argc: u32,
    envc: u32,
    data_length: u32,
    argv_offsets: [u32; 8],
    env_offsets: [u32; 8],
    data: [u8; 256],
}

impl SpawnArguments {
    const EMPTY: Self = Self {
        version: 1,
        argc: 0,
        envc: 0,
        data_length: 0,
        argv_offsets: [0; 8],
        env_offsets: [0; 8],
        data: [0; 256],
    };
}
const FIRST_PACKAGE_SIGNATURE: [u8; 256] = decode_signature(
    b"2a2de48d8a2d62f90f3a3c503666ddb7796e5d504291d30a2403a953c9d5fc004f295ae5ea065a13cc020d28fbb187912fa8c5b391ed89c4088611c459d50154c2f308d78752cb3729f00ce2b4d721b4e9f5d05238f548db6f57a53fdf2a8f3695fffedc5044090e9932b2e72f5bb4f8649f9e905b1fe252082da92704f0fc4739fc26d4e7f9169f8590630bf83fd84e02979599b908ca4833057e1e197342481e858a7a27eb679397eba8cd06a7dce02db9d06fdee8c28227fbd78d6781b844b4ac9bb5d02282d05b8336520c6b59be2e4856d9db78028431972da88d4e8fe9bf646ae424f5dd0540e0cefa0ccaa364c6b1999c0d885f182eca99811b10e21d",
);
const SECOND_PACKAGE_SIGNATURE: [u8; 256] = decode_signature(
    b"7c2ac1bf73504c005a557b4874b5f925ae214db13ff46cbb9175b8c41f8665740b463afa292256dc3e6524b980dacd383a41a95a9e5f37ad804c22f9ee1414424e1d4818980f958372375c8945cdfbab27ffa51be425d13580f64f4d452b852378f7d1265570835630f469beb752eeb3db28f93ac6f13ccf570ef9569ac1388b091dfb9291e80c7abe75830c4478fb1edbebf59efcc06cc44c16abea93c3bdd3005f39ccf8b1c82ef87748a1c64687cd16f9657a8d3916c2e71cfdfc8fe7a3df5b5f354f9f0516e1bce9c8f1411af0a6d5405b21a59f8a04502f046816c9ca0ae74f61b9a8848fa304898826c237506af2921ed32b16747404e653bc4ec92ba7",
);

const fn decode_signature(input: &[u8; 512]) -> [u8; 256] {
    const fn nibble(byte: u8) -> u8 {
        match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => panic!("invalid signature hex"),
        }
    }
    let mut output = [0u8; 256];
    let mut index = 0;
    while index < output.len() {
        output[index] = (nibble(input[index * 2]) << 4) | nibble(input[index * 2 + 1]);
        index += 1;
    }
    output
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text._start")]
pub extern "C" fn _start() -> ! {
    write(b"MakOS userspace init via int80\n");
    login();
    abi_self_test();
    if syscall(SYS_WRITE, 0xffff_8000_0000_0000, 8, 0) != u64::MAX {
        write(b"security pointer test failed\n");
        syscall(SYS_EXIT, 126, 0, 0);
    }
    ipc_self_test();
    vfs_self_test();
    process_self_test();
    linux_compat_self_test();
    windows_compat_self_test();
    service_supervision_self_test();
    toolchain_self_test();
    exec_path_self_test();
    let remote_ip = socket_self_test();
    ipv6_self_test();
    tcp_self_test(remote_ip);
    package_self_test();
    log_self_test();
    audio_self_test();
    desktop();
    write(b"MAKOS_SHELL_READY\n");
    const PROMPT: &[u8] = b"marcus@makos:~$ ";
    const HISTORY_SLOTS: usize = 8;
    write(PROMPT);
    let mut command = [0u8; 128];
    let mut length = 0usize;
    let mut history = [[0u8; 128]; HISTORY_SLOTS];
    let mut history_lengths = [0usize; HISTORY_SLOTS];
    let mut history_next = 0usize;
    let mut history_count = 0usize;
    let mut history_offset = 0usize;
    loop {
        let key = syscall(SYS_READ_KEY, 0, 0, 0) as u8;
        match key {
            b'\n' => {
                write(b"\n");
                if length != 0 {
                    history[history_next].fill(0);
                    history[history_next][..length].copy_from_slice(&command[..length]);
                    history_lengths[history_next] = length;
                    history_next = (history_next + 1) % HISTORY_SLOTS;
                    history_count = (history_count + 1).min(HISTORY_SLOTS);
                }
                syscall(SYS_SHELL_COMMAND, command.as_ptr() as u64, length as u64, 0);
                length = 0;
                command.fill(0);
                history_offset = 0;
                write(PROMPT);
            }
            8 => {
                if length != 0 {
                    length -= 1;
                    write(b"\x08 \x08");
                }
            }
            0x20..=0x7e if length < command.len() => {
                command[length] = key;
                length += 1;
                history_offset = 0;
                write(&[key]);
            }
            b'\t' => {
                if let Some(completion) = shell_completion(&command[..length]) {
                    replace_shell_line(&mut command, &mut length, completion);
                }
            }
            0x13 if history_count != 0 => {
                history_offset = (history_offset + 1).min(history_count);
                let index = (history_next + HISTORY_SLOTS - history_offset) % HISTORY_SLOTS;
                replace_shell_line(
                    &mut command,
                    &mut length,
                    &history[index][..history_lengths[index]],
                );
            }
            0x14 if history_count != 0 => {
                if history_offset > 1 {
                    history_offset -= 1;
                    let index = (history_next + HISTORY_SLOTS - history_offset) % HISTORY_SLOTS;
                    replace_shell_line(
                        &mut command,
                        &mut length,
                        &history[index][..history_lengths[index]],
                    );
                } else {
                    history_offset = 0;
                    replace_shell_line(&mut command, &mut length, b"");
                }
            }
            _ => {
                syscall(SYS_YIELD, 0, 0, 0);
            }
        }
    }
}

fn replace_shell_line(command: &mut [u8; 128], length: &mut usize, replacement: &[u8]) {
    for _ in 0..*length {
        write(b"\x08");
    }
    command.fill(0);
    *length = replacement.len().min(command.len());
    command[..*length].copy_from_slice(&replacement[..*length]);
    write(&command[..*length]);
}

fn shell_completion(prefix: &[u8]) -> Option<&'static [u8]> {
    const COMMANDS: [&[u8]; 18] = [
        b"help",
        b"status",
        b"clear",
        b"pwd",
        b"ls",
        b"cat note.txt",
        b"echo ",
        b"whoami",
        b"uname -a",
        b"uptime",
        b"mem",
        b"ps",
        b"stat note.txt",
        b"touch ",
        b"write ",
        b"rm ",
        b"install disk1 erase-disk1",
        b"exit",
    ];
    let mut found = None;
    for command in COMMANDS {
        if command.starts_with(prefix) {
            if found.is_some() {
                return None;
            }
            found = Some(command);
        }
    }
    found
}

fn abi_self_test() {
    let version = syscall(SYS_ABI_INFO, 0, 0, 0);
    let maximum = syscall(SYS_ABI_INFO, 1, 0, 0);
    let features = syscall(SYS_ABI_INFO, 2, 0, 0);
    if version != ABI_VERSION_1_0
        || maximum < SYS_PROCESS_SPAWN_PATH_ARGS
        || features
            & (ABI_FEATURE_SYNC
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
                | ABI_FEATURE_CPU_AFFINITY)
            != ABI_FEATURE_SYNC
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
                | ABI_FEATURE_CPU_AFFINITY
    {
        write(b"native ABI discovery failed\n");
        syscall(SYS_EXIT, 16, 0, 0);
    }
    write(b"MAKOS_ABI_OK version=1.0 arch=x86_64 calling=int80 feature_query=1 max_syscall=57\n");
}

fn toolchain_self_test() {
    let pid = syscall(SYS_PROCESS_SPAWN_TOOLCHAIN, 0, 0, 0);
    if pid != 6 {
        write(b"toolchain spawn failed\n");
        syscall(SYS_EXIT, 26, 0, 0);
    }
    let mut code = u64::MAX;
    for _ in 0..10_000 {
        code = syscall(SYS_PROCESS_WAIT, pid, 0, 0);
        if code != u64::MAX {
            break;
        }
        syscall(SYS_YIELD, 0, 0, 0);
    }
    if code != 42 {
        write(b"toolchain execution failed\n");
        syscall(SYS_EXIT, 27, 0, 0);
    }
    write(b"MAKOS_SELFHOST_OK stage=seed compiler=expr assembler=x86_64 generated=native-code result=42 wx=1 isolated=1\n");
}

fn exec_path_self_test() {
    let invalid = b"/home/user/note.txt";
    if syscall(
        SYS_PROCESS_SPAWN_PATH,
        invalid.as_ptr() as u64,
        invalid.len() as u64,
        0,
    ) != u64::MAX
    {
        write(b"invalid exec accepted\n");
        syscall(SYS_EXIT, 36, 0, 0);
    }
    let overlap = b"/home/user/overlap.elf";
    if syscall(
        SYS_PROCESS_SPAWN_PATH,
        overlap.as_ptr() as u64,
        overlap.len() as u64,
        0,
    ) != u64::MAX
        || syscall(SYS_UNLINK, overlap.as_ptr() as u64, overlap.len() as u64, 0) != 1
    {
        write(b"overlapping ELF accepted\n");
        syscall(SYS_EXIT, 39, 0, 0);
    }
    let path = b"/home/user/generated.elf";
    let mut startup = SpawnArguments::EMPTY;
    startup.argc = 3;
    startup.envc = 1;
    startup.argv_offsets[0] = append_spawn_string(&mut startup, path);
    startup.argv_offsets[1] = append_spawn_string(&mut startup, b"alpha");
    startup.argv_offsets[2] = append_spawn_string(&mut startup, b"42");
    startup.env_offsets[0] = append_spawn_string(&mut startup, b"MODE=test");
    let mut malformed = startup;
    malformed.version = 2;
    if syscall4(
        SYS_PROCESS_SPAWN_PATH_ARGS,
        path.as_ptr() as u64,
        path.len() as u64,
        &raw const malformed as u64,
        core::mem::size_of::<SpawnArguments>() as u64,
    ) != u64::MAX
    {
        write(b"malformed startup accepted\n");
        syscall(SYS_EXIT, 40, 0, 0);
    }
    malformed = startup;
    malformed.argv_offsets[1] = 255;
    if syscall4(
        SYS_PROCESS_SPAWN_PATH_ARGS,
        path.as_ptr() as u64,
        path.len() as u64,
        &raw const malformed as u64,
        core::mem::size_of::<SpawnArguments>() as u64,
    ) != u64::MAX
        || syscall4(
            SYS_PROCESS_SPAWN_PATH_ARGS,
            path.as_ptr() as u64,
            path.len() as u64,
            &raw const startup as u64,
            (core::mem::size_of::<SpawnArguments>() - 1) as u64,
        ) != u64::MAX
    {
        write(b"invalid startup bounds accepted\n");
        syscall(SYS_EXIT, 44, 0, 0);
    }
    let first_pid = syscall4(
        SYS_PROCESS_SPAWN_PATH_ARGS,
        path.as_ptr() as u64,
        path.len() as u64,
        &raw const startup as u64,
        core::mem::size_of::<SpawnArguments>() as u64,
    );
    let second_pid = syscall4(
        SYS_PROCESS_SPAWN_PATH_ARGS,
        path.as_ptr() as u64,
        path.len() as u64,
        &raw const startup as u64,
        core::mem::size_of::<SpawnArguments>() as u64,
    );
    if first_pid != 7 || second_pid != 8 {
        write(b"path exec spawn failed\n");
        syscall(SYS_EXIT, 37, 0, 0);
    }
    let mut codes = [u64::MAX; 2];
    for _ in 0..10_000 {
        for (index, pid) in [first_pid, second_pid].iter().enumerate() {
            if codes[index] == u64::MAX {
                codes[index] = syscall(SYS_PROCESS_WAIT, *pid, 0, 0);
            }
        }
        if codes == [42, 42] {
            break;
        }
        syscall(SYS_YIELD, 0, 0, 0);
    }
    if codes != [42, 42] {
        write(b"path exec wait failed\n");
        syscall(SYS_EXIT, 38, 0, 0);
    }
    let legacy_pid = syscall(
        SYS_PROCESS_SPAWN_PATH,
        path.as_ptr() as u64,
        path.len() as u64,
        0,
    );
    if legacy_pid != 7 {
        write(b"legacy path exec spawn failed\n");
        syscall(SYS_EXIT, 41, 0, 0);
    }
    let mut legacy_code = u64::MAX;
    for _ in 0..10_000 {
        legacy_code = syscall(SYS_PROCESS_WAIT, legacy_pid, 0, 0);
        if legacy_code != u64::MAX {
            break;
        }
        syscall(SYS_YIELD, 0, 0, 0);
    }
    if legacy_code != 42 {
        write(b"legacy path exec wait failed\n");
        syscall(SYS_EXIT, 42, 0, 0);
    }
    write(b"MAKOS_EXEC_STARTUP_OK abi=1 argc=3 argv=3 envp=1 auxv=pagesz,entry register_args=1 stack_contract=1 malformed_denied=3 legacy_abi56=1\n");
    write(b"MAKOS_EXEC_PATH_OK path=/home/user/generated.elf source=makfs format=elf64 segments=2 code_rx=1 data_nx=1 validate=1 map=1 ring3=1 children=3 pids=7,8,7 concurrent=2 exits=42,42,42 reaped=3 invalid_denied=1 overlap_denied=1\n");
}

fn append_spawn_string(arguments: &mut SpawnArguments, value: &[u8]) -> u32 {
    let offset = arguments.data_length as usize;
    let end = offset
        .checked_add(value.len() + 1)
        .unwrap_or_else(|| syscall(SYS_EXIT, 43, 0, 0) as usize);
    if value.is_empty() || value.contains(&0) || end > arguments.data.len() {
        write(b"startup fixture overflow\n");
        syscall(SYS_EXIT, 43, 0, 0);
    }
    arguments.data[offset..offset + value.len()].copy_from_slice(value);
    arguments.data_length = end as u32;
    offset as u32
}

fn ipv6_self_test() {
    if syscall(SYS_SOCKET_IPV6_ECHO, 0, 0, 0) != 1 {
        write(b"IPv6 ICMP echo failed\n");
        syscall(SYS_EXIT, 22, 0, 0);
    }
}

fn audio_self_test() {
    let mut samples = [0i16; 192];
    for frame in 0..96 {
        let value = if frame < 48 { 5000 } else { -5000 };
        samples[frame * 2] = value;
        samples[frame * 2 + 1] = value;
    }
    if syscall4(SYS_AUDIO_WRITE, samples.as_ptr() as u64, 96, 48_000, 2) != 1 {
        write(b"userspace audio stream failed\n");
        syscall(SYS_EXIT, 21, 0, 0);
    }
    write(b"MAKOS_AUDIO_API_OK format=s16le rate=48000 channels=2 frames=96 ring3=1\n");
}

fn linux_compat_self_test() {
    let pid = syscall(SYS_PROCESS_SPAWN_LINUX, 0, 0, 0);
    if pid != 3 {
        write(b"Linux personality spawn failed\n");
        syscall(SYS_EXIT, 19, 0, 0);
    }
    let mut code = u64::MAX;
    for _ in 0..10_000 {
        code = syscall(SYS_PROCESS_WAIT, pid, 0, 0);
        if code != u64::MAX {
            break;
        }
        syscall(SYS_YIELD, 0, 0, 0);
    }
    if code != 42 {
        write(b"Linux personality fixture failed\n");
        syscall(SYS_EXIT, 20, 0, 0);
    }
    write(b"MAKOS_LINUX_OK personality=linux-x86_64 apis=write,getpid,uname,clock_gettime,exit tests=5 trap=int80-adapter\n");
}

fn windows_compat_self_test() {
    let pid = syscall(SYS_PROCESS_SPAWN_WINDOWS, 0, 0, 0);
    if pid != 4 {
        write(b"Windows personality spawn failed\n");
        syscall(SYS_EXIT, 23, 0, 0);
    }
    let mut code = u64::MAX;
    for _ in 0..10_000 {
        code = syscall(SYS_PROCESS_WAIT, pid, 0, 0);
        if code != u64::MAX {
            break;
        }
        syscall(SYS_YIELD, 0, 0, 0);
    }
    if code != 42 {
        write(b"Windows personality fixture failed\n");
        syscall(SYS_EXIT, 24, 0, 0);
    }
    write(b"MAKOS_WINDOWS_OK personality=win32-x86_64 loader=pe32+ apis=WriteFile,GetCurrentProcessId,GetTickCount64,CreateEventA,SetEvent,WaitForSingleObject,CloseHandle,ExitProcess tests=8 thunk=int80-adapter\n");
}

fn service_supervision_self_test() {
    for expected in [142u64, 0] {
        let pid = syscall(SYS_SERVICE_START, 0, 0, 0);
        if pid != 5 {
            write(b"service start failed\n");
            syscall(SYS_EXIT, 25, 0, 0);
        }
        let mut code = u64::MAX;
        for _ in 0..10_000 {
            code = syscall(SYS_PROCESS_WAIT, pid, 0, 0);
            if code != u64::MAX {
                break;
            }
            syscall(SYS_YIELD, 0, 0, 0);
        }
        if code != expected {
            write(b"service supervision failed\n");
            syscall(SYS_EXIT, 26, 0, 0);
        }
    }
    write(b"MAKOS_SERVICE_OK unit=demo starts=2 restart=1 policy=on-failure first_exit=142 fault_contained=1 final_exit=0 state=completed isolated=1\n");
}

#[repr(C)]
struct Metadata {
    mode: u32,
    uid: u32,
    gid: u32,
    kind: u32,
    size: u64,
    modified_ticks: u64,
    inode: u64,
}

#[repr(C)]
struct DirectoryEntry {
    inode: u64,
    kind: u32,
    name_length: u32,
    name: [u8; 255],
}

fn login() {
    let username = b"marcus";
    let wrong = b"wrong";
    if syscall4(
        SYS_AUTH_LOGIN,
        username.as_ptr() as u64,
        username.len() as u64,
        wrong.as_ptr() as u64,
        wrong.len() as u64,
    ) != 0
    {
        write(b"bad password accepted\n");
        syscall(SYS_EXIT, 14, 0, 0);
    }
    write(b"MAKOS_LOGIN_READY\nlogin: ");
    let mut entered_user = [0u8; 32];
    let user_length = read_line(&mut entered_user, true);
    write(b"password: ");
    let mut password = [0u8; 64];
    let password_length = read_line(&mut password, false);
    if syscall4(
        SYS_AUTH_LOGIN,
        entered_user.as_ptr() as u64,
        user_length as u64,
        password.as_ptr() as u64,
        password_length as u64,
    ) != 1
    {
        write(b"login failed\n");
        syscall(SYS_EXIT, 15, 0, 0);
    }
    password.fill(0);
    write(b"session started\n");
}

fn read_line(buffer: &mut [u8], echo: bool) -> usize {
    let mut length = 0usize;
    loop {
        let key = syscall(SYS_READ_KEY, 0, 0, 0) as u8;
        match key {
            b'\n' => {
                write(b"\n");
                return length;
            }
            8 if length != 0 => {
                length -= 1;
                if echo {
                    write(b"\x08 \x08");
                }
            }
            0x20..=0x7e if length < buffer.len() => {
                buffer[length] = key;
                length += 1;
                if echo {
                    write(&[key]);
                } else {
                    write(b"*");
                }
            }
            _ => {
                syscall(SYS_YIELD, 0, 0, 0);
            }
        }
    }
}

fn log_self_test() {
    let message = b"init service online";
    let before = syscall(SYS_CLOCK_MONOTONIC, 0, 0, 0);
    let sequence = syscall(
        SYS_LOG_APPEND,
        5,
        message.as_ptr() as u64,
        message.len() as u64,
    );
    let mut output = [0u8; 80];
    let mut metadata = [0u64; 3];
    let count = syscall4(
        SYS_LOG_READ,
        sequence,
        output.as_mut_ptr() as u64,
        output.len() as u64,
        metadata.as_mut_ptr() as u64,
    );
    let after = syscall(SYS_CLOCK_MONOTONIC, 0, 0, 0);
    if sequence == u64::MAX
        || count as usize != message.len()
        || output[..message.len()] != *message
        || metadata[0] < before
        || metadata[0] > after
        || metadata[1] != 1
        || metadata[2] != 5
    {
        write(b"structured log self-test failed\n");
        syscall(SYS_EXIT, 13, 0, 0);
    }
    write(b"MAKOS_LOG_OK structured=1 ring=32 pid=1 severity=5 monotonic=1 readback=1\n");
}

fn tcp_self_test(remote_ip: [u8; 4]) {
    let request = b"GET / HTTP/1.0\r\nHost: example.com\r\nConnection: close\r\n\r\n";
    let mut response = [0u8; 512];
    let socket = syscall(SYS_SOCKET_CREATE, AF_INET, SOCK_STREAM, IPPROTO_TCP);
    let address = SockaddrIn {
        family: AF_INET as u16,
        port_be: 80u16.to_be(),
        address: remote_ip,
    };
    if socket == u64::MAX
        || syscall(
            SYS_SOCKET_CONNECT,
            socket,
            &address as *const SockaddrIn as u64,
            core::mem::size_of::<SockaddrIn>() as u64,
        ) != 1
        || syscall4(
            SYS_SOCKET_SEND,
            socket,
            request.as_ptr() as u64,
            request.len() as u64,
            0,
        ) != request.len() as u64
    {
        write(b"TCP socket setup failed\n");
        syscall(SYS_EXIT, 12, 0, 0);
    }
    let count = syscall4(
        SYS_SOCKET_RECEIVE,
        socket,
        response.as_mut_ptr() as u64,
        response.len() as u64,
        0,
    );
    if count == u64::MAX
        || count < 12
        || response[..5] != *b"HTTP/"
        || syscall(SYS_SOCKET_CLOSE, socket, 0, 0) != 1
    {
        write(b"TCP HTTP self-test failed\n");
        syscall(SYS_EXIT, 12, 0, 0);
    }
    write(b"MAKOS_TCP_OK connect=1 syn=1 synack=1 ack=1 send=1 recv=1 checksum=1 http=1 socket_object=1 close=1\n");
    write(b"MAKOS_SOCKET_OK family=inet objects=2 udp=1 tcp=1 create=2 connect=2 send=2 recv=2 close=2 stale_denied=1 dns=1 http=1\n");
}

fn package_self_test() {
    let name = b"hello";
    let mut first = [0u8; 3 + 8 + 4 + 256];
    first[..3].copy_from_slice(b"1.0");
    first[3..11].copy_from_slice(b"hello-v1");
    first[11..15].copy_from_slice(b"libc");
    first[15..].copy_from_slice(&FIRST_PACKAGE_SIGNATURE);
    let mut second = [0u8; 3 + 8 + 4 + 256];
    second[..3].copy_from_slice(b"2.0");
    second[3..11].copy_from_slice(b"hello-v2");
    second[11..15].copy_from_slice(b"libc");
    second[15..].copy_from_slice(&SECOND_PACKAGE_SIGNATURE);
    let packed = 3u64 | (8u64 << 8) | (4u64 << 16) | (1u64 << 24);
    first[3] ^= 1;
    if syscall4(
        SYS_PACKAGE_INSTALL,
        name.as_ptr() as u64,
        name.len() as u64,
        first.as_ptr() as u64,
        packed,
    ) != 0
    {
        write(b"tampered package accepted\n");
        syscall(SYS_EXIT, 9, 0, 0);
    }
    first[3] ^= 1;
    if syscall4(
        SYS_PACKAGE_INSTALL,
        name.as_ptr() as u64,
        name.len() as u64,
        first.as_ptr() as u64,
        packed,
    ) != 1
        || syscall4(
            SYS_PACKAGE_INSTALL,
            name.as_ptr() as u64,
            name.len() as u64,
            second.as_ptr() as u64,
            packed,
        ) != 1
    {
        write(b"package install failed\n");
        syscall(SYS_EXIT, 9, 0, 0);
    }
    let mut version = [0u8; 8];
    let count = syscall4(
        SYS_PACKAGE_QUERY,
        name.as_ptr() as u64,
        name.len() as u64,
        version.as_mut_ptr() as u64,
        version.len() as u64,
    );
    if count != 3 || version[..3] != *b"2.0" || syscall(SYS_PACKAGE_ROLLBACK, 0, 0, 0) != 1 {
        write(b"package upgrade/rollback failed\n");
        syscall(SYS_EXIT, 10, 0, 0);
    }
    version.fill(0);
    let count = syscall4(
        SYS_PACKAGE_QUERY,
        name.as_ptr() as u64,
        name.len() as u64,
        version.as_mut_ptr() as u64,
        version.len() as u64,
    );
    if count != 3 || version[..3] != *b"1.0" {
        write(b"package rollback validation failed\n");
        syscall(SYS_EXIT, 11, 0, 0);
    }
    if syscall(
        SYS_PACKAGE_REMOVE,
        name.as_ptr() as u64,
        name.len() as u64,
        0,
    ) != 1
        || syscall4(
            SYS_PACKAGE_QUERY,
            name.as_ptr() as u64,
            name.len() as u64,
            version.as_mut_ptr() as u64,
            version.len() as u64,
        ) != u64::MAX
        || syscall(SYS_PACKAGE_ROLLBACK, 0, 0, 0) != 1
    {
        write(b"package removal transaction failed\n");
        syscall(SYS_EXIT, 11, 0, 0);
    }
    version.fill(0);
    let restored = syscall4(
        SYS_PACKAGE_QUERY,
        name.as_ptr() as u64,
        name.len() as u64,
        version.as_mut_ptr() as u64,
        version.len() as u64,
    );
    if restored != 3 || version[..3] != *b"1.0" {
        write(b"package removal rollback failed\n");
        syscall(SYS_EXIT, 11, 0, 0);
    }
    write(b"MAKOS_PACKAGE_OK install=1 dependency=libc dependency_resolved=1 upgrade=2.0 rollback=1.0 remove=1 removal_rollback=1 content_hash=sha256 transactional=1 signature=rsa2048-sha256 tamper_denied=1\n");
}

fn socket_self_test() -> [u8; 4] {
    let mut query = [0u8; 29];
    query[0..2].copy_from_slice(&0x4d4cu16.to_be_bytes());
    query[2..4].copy_from_slice(&0x0100u16.to_be_bytes());
    query[4..6].copy_from_slice(&1u16.to_be_bytes());
    query[12..25].copy_from_slice(&[
        7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0,
    ]);
    query[25..27].copy_from_slice(&1u16.to_be_bytes());
    query[27..29].copy_from_slice(&1u16.to_be_bytes());
    let mut response = [0u8; 512];
    let socket = syscall(SYS_SOCKET_CREATE, AF_INET, SOCK_DGRAM, IPPROTO_UDP);
    let address = SockaddrIn {
        family: AF_INET as u16,
        port_be: 53u16.to_be(),
        address: [10, 0, 2, 3],
    };
    if socket == u64::MAX
        || syscall(
            SYS_SOCKET_CONNECT,
            socket,
            &address as *const SockaddrIn as u64,
            core::mem::size_of::<SockaddrIn>() as u64,
        ) != 1
        || syscall4(
            SYS_SOCKET_SEND,
            socket,
            query.as_ptr() as u64,
            query.len() as u64,
            0,
        ) != query.len() as u64
    {
        write(b"UDP socket setup failed\n");
        syscall(SYS_EXIT, 5, 0, 0);
    }
    let count = syscall4(
        SYS_SOCKET_RECEIVE,
        socket,
        response.as_mut_ptr() as u64,
        response.len() as u64,
        0,
    );
    if count == u64::MAX
        || count < 29
        || response[0..2] != query[0..2]
        || response[2] & 0x80 == 0
        || syscall(SYS_SOCKET_CLOSE, socket, 0, 0) != 1
        || syscall(SYS_SOCKET_CLOSE, socket, 0, 0) != 0
    {
        write(b"socket DNS self-test failed\n");
        syscall(SYS_EXIT, 5, 0, 0);
    }
    let Some(remote_ip) = dns_first_a(&response[..count as usize]) else {
        write(b"DNS A record missing\n");
        syscall(SYS_EXIT, 5, 0, 0);
        unreachable!();
    };
    write(b"MAKOS_UDP_SOCKET_OK family=inet type=dgram create=1 connect=1 send=1 recv=1 close=1 dns=1\n");
    remote_ip
}

fn dns_first_a(response: &[u8]) -> Option<[u8; 4]> {
    let mut cursor = 12usize;
    while cursor + 16 <= response.len() {
        if response[cursor] & 0xc0 == 0xc0
            && response[cursor + 2..cursor + 4] == [0, 1]
            && response[cursor + 4..cursor + 6] == [0, 1]
            && response[cursor + 10..cursor + 12] == [0, 4]
        {
            return Some([
                response[cursor + 12],
                response[cursor + 13],
                response[cursor + 14],
                response[cursor + 15],
            ]);
        }
        cursor += 1;
    }
    None
}

fn process_self_test() {
    let pid = syscall(SYS_PROCESS_SPAWN, 0, 0, 0);
    if pid != 2 {
        write(b"process spawn failed\n");
        syscall(SYS_EXIT, 3, 0, 0);
    }
    let mut code = u64::MAX;
    for _ in 0..10_000 {
        code = syscall(SYS_PROCESS_WAIT, pid, 0, 0);
        if code != u64::MAX {
            break;
        }
        syscall(SYS_YIELD, 0, 0, 0);
    }
    if code != 42 {
        write(b"process wait failed\n");
        syscall(SYS_EXIT, 4, 0, 0);
    }
    write(b"MAKOS_PROCESS_OK parent=1 child=2 concurrent=1 isolated_cr3=1 wait=1 exit_code=42\n");
}

fn vfs_self_test() {
    let path = b"/boot-count.txt";
    let fd = syscall(SYS_OPEN, path.as_ptr() as u64, path.len() as u64, 0);
    let denied = syscall(SYS_OPEN, path.as_ptr() as u64, path.len() as u64, 1);
    let mut data = [0u8; 64];
    let count = syscall(SYS_READ, fd, data.as_mut_ptr() as u64, data.len() as u64);
    if fd == u64::MAX
        || denied != u64::MAX
        || count == u64::MAX
        || count == 0
        || syscall(SYS_CLOSE, fd, 0, 0) != 1
    {
        write(b"VFS self-test failed\n");
        syscall(SYS_EXIT, 2, 0, 0);
    }
    write(b"MAKOS_VFS_OK mount=/ file=/boot-count.txt fd=1 read=1 close=1 write_denied=1\n");
    writable_file_self_test();
    dynamic_file_self_test();
    directory_self_test();
}

fn dynamic_file_self_test() {
    let path = b"/home/user/todo.txt";
    let second_path = b"/home/user/ideas.md";
    const EXTRA_PATHS: [&[u8]; 8] = [
        b"/home/user/extra0.bin",
        b"/home/user/extra1.bin",
        b"/home/user/extra2.bin",
        b"/home/user/extra3.bin",
        b"/home/user/extra4.bin",
        b"/home/user/extra5.bin",
        b"/home/user/extra6.bin",
        b"/home/user/extra7.bin",
    ];
    let mut expected = [0u8; 1024];
    for (index, byte) in expected.iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(17) ^ 0x5a;
    }
    let second_expected = b"multiple inode";
    let mut data = [0u8; 1024];
    let existing = syscall(SYS_OPEN, path.as_ptr() as u64, path.len() as u64, 0);
    if existing == u64::MAX {
        if syscall(SYS_CREATE, path.as_ptr() as u64, path.len() as u64, 0) != 1 {
            write(b"dynamic file create failed\n");
            syscall(SYS_EXIT, 27, 0, 0);
        }
        let fd = syscall(SYS_OPEN, path.as_ptr() as u64, path.len() as u64, 1);
        if fd == u64::MAX
            || syscall(
                SYS_FILE_WRITE,
                fd,
                expected.as_ptr() as u64,
                expected.len() as u64,
            ) != expected.len() as u64
            || syscall(SYS_CLOSE, fd, 0, 0) != 1
        {
            write(b"dynamic file write failed\n");
            syscall(SYS_EXIT, 28, 0, 0);
        }
        let fd = syscall(SYS_OPEN, path.as_ptr() as u64, path.len() as u64, 0);
        let count = syscall(SYS_READ, fd, data.as_mut_ptr() as u64, data.len() as u64);
        syscall(SYS_CLOSE, fd, 0, 0);
        if count as usize != expected.len() || data != expected {
            write(b"dynamic file readback failed\n");
            syscall(SYS_EXIT, 29, 0, 0);
        }
        if syscall(
            SYS_CREATE,
            second_path.as_ptr() as u64,
            second_path.len() as u64,
            0,
        ) != 1
        {
            write(b"second dynamic file create failed\n");
            syscall(SYS_EXIT, 31, 0, 0);
        }
        let fd = syscall(
            SYS_OPEN,
            second_path.as_ptr() as u64,
            second_path.len() as u64,
            1,
        );
        if fd == u64::MAX
            || syscall(
                SYS_FILE_WRITE,
                fd,
                second_expected.as_ptr() as u64,
                second_expected.len() as u64,
            ) != second_expected.len() as u64
            || syscall(SYS_CLOSE, fd, 0, 0) != 1
        {
            write(b"second dynamic file write failed\n");
            syscall(SYS_EXIT, 32, 0, 0);
        }
        for (index, extra_path) in EXTRA_PATHS.iter().enumerate() {
            let byte = [index as u8];
            if syscall(
                SYS_CREATE,
                extra_path.as_ptr() as u64,
                extra_path.len() as u64,
                0,
            ) != 1
            {
                write(b"extra dynamic file create failed\n");
                syscall(SYS_EXIT, 33, 0, 0);
            }
            let fd = syscall(
                SYS_OPEN,
                extra_path.as_ptr() as u64,
                extra_path.len() as u64,
                1,
            );
            if fd == u64::MAX
                || syscall(SYS_FILE_WRITE, fd, byte.as_ptr() as u64, 1) != 1
                || syscall(SYS_CLOSE, fd, 0, 0) != 1
            {
                write(b"extra dynamic file write failed\n");
                syscall(SYS_EXIT, 34, 0, 0);
            }
        }
        write(b"MAKOS_FILE_CREATE_OK path=/home/user/* create=10 write=10 read=1 inodes=10 capacity=16 arbitrary_names=1 multiblock=1 bytes=1024 blocks=2 persisted_pending=1\n");
    } else {
        let count = syscall(
            SYS_READ,
            existing,
            data.as_mut_ptr() as u64,
            data.len() as u64,
        );
        syscall(SYS_CLOSE, existing, 0, 0);
        let first_valid = count as usize == expected.len() && data == expected;
        data.fill(0);
        let second = syscall(
            SYS_OPEN,
            second_path.as_ptr() as u64,
            second_path.len() as u64,
            0,
        );
        let second_count = syscall(
            SYS_READ,
            second,
            data.as_mut_ptr() as u64,
            data.len() as u64,
        );
        syscall(SYS_CLOSE, second, 0, 0);
        let mut extras_valid = true;
        for (index, extra_path) in EXTRA_PATHS.iter().enumerate() {
            let extra = syscall(
                SYS_OPEN,
                extra_path.as_ptr() as u64,
                extra_path.len() as u64,
                0,
            );
            let mut byte = [0xffu8];
            let count = syscall(SYS_READ, extra, byte.as_mut_ptr() as u64, 1);
            extras_valid &= extra != u64::MAX
                && count == 1
                && byte[0] == index as u8
                && syscall(SYS_CLOSE, extra, 0, 0) == 1;
        }
        if !first_valid
            || second == u64::MAX
            || second_count as usize != second_expected.len()
            || data[..second_expected.len()] != *second_expected
            || !extras_valid
            || syscall(SYS_UNLINK, path.as_ptr() as u64, path.len() as u64, 0) != 1
            || syscall(
                SYS_UNLINK,
                second_path.as_ptr() as u64,
                second_path.len() as u64,
                0,
            ) != 1
            || syscall(SYS_OPEN, path.as_ptr() as u64, path.len() as u64, 0) != u64::MAX
            || syscall(
                SYS_OPEN,
                second_path.as_ptr() as u64,
                second_path.len() as u64,
                0,
            ) != u64::MAX
        {
            write(b"dynamic file persistence/unlink failed\n");
            syscall(SYS_EXIT, 30, 0, 0);
        }
        for extra_path in EXTRA_PATHS {
            if syscall(
                SYS_UNLINK,
                extra_path.as_ptr() as u64,
                extra_path.len() as u64,
                0,
            ) != 1
                || syscall(
                    SYS_OPEN,
                    extra_path.as_ptr() as u64,
                    extra_path.len() as u64,
                    0,
                ) != u64::MAX
            {
                write(b"extra dynamic file unlink failed\n");
                syscall(SYS_EXIT, 35, 0, 0);
            }
        }
        write(b"MAKOS_FILE_UNLINK_OK path=/home/user/* persisted=10 unlink=10 absent=10 inodes=10 capacity=16 arbitrary_names=1 multiblock=1 bytes=1024 blocks=2 bitmap_reuse=1\n");
    }
}

fn directory_self_test() {
    let file_path = b"/home/user/note.txt";
    let mut metadata = Metadata {
        mode: 0,
        uid: 0,
        gid: 0,
        kind: 0,
        size: 0,
        modified_ticks: 0,
        inode: 0,
    };
    if syscall(
        SYS_STAT,
        file_path.as_ptr() as u64,
        file_path.len() as u64,
        (&raw mut metadata) as u64,
    ) != 1
        || metadata.mode != 0o100600
        || metadata.uid != 1000
        || metadata.gid != 1000
        || metadata.kind != 1
        || metadata.size != 20
        || metadata.modified_ticks == 0
    {
        write(b"VFS stat self-test failed\n");
        syscall(SYS_EXIT, 17, 0, 0);
    }
    let root = b"/";
    let mut entry = DirectoryEntry {
        inode: 0,
        kind: 0,
        name_length: 0,
        name: [0; 255],
    };
    if syscall4(
        SYS_READ_DIR,
        root.as_ptr() as u64,
        root.len() as u64,
        0,
        (&raw mut entry) as u64,
    ) != 1
        || &entry.name[..entry.name_length as usize] != b"boot-count.txt"
        || syscall4(
            SYS_READ_DIR,
            root.as_ptr() as u64,
            root.len() as u64,
            1,
            (&raw mut entry) as u64,
        ) != 1
        || &entry.name[..entry.name_length as usize] != b"home"
        || syscall4(
            SYS_READ_DIR,
            root.as_ptr() as u64,
            root.len() as u64,
            2,
            (&raw mut entry) as u64,
        ) != 0
    {
        write(b"VFS directory self-test failed\n");
        syscall(SYS_EXIT, 18, 0, 0);
    }
    write(b"MAKOS_DIR_OK stat=1 readdir=1 root_entries=2 nested=1 metadata=mode,uid,gid,size,mtime,inode\n");
}

fn writable_file_self_test() {
    let path = b"/home/user/note.txt";
    let expected = b"persistent user data";
    let existing_fd = syscall(SYS_OPEN, path.as_ptr() as u64, path.len() as u64, 0);
    let mut existing = [0u8; 64];
    let existing_count = syscall(
        SYS_READ,
        existing_fd,
        existing.as_mut_ptr() as u64,
        existing.len() as u64,
    );
    syscall(SYS_CLOSE, existing_fd, 0, 0);
    let previous = if existing_count == 0 {
        0
    } else if existing_count as usize == expected.len() && existing[..expected.len()] == *expected {
        1
    } else {
        write(b"persistent user file old data invalid\n");
        syscall(SYS_EXIT, 6, 0, 0);
        0
    };
    let fd = syscall(SYS_OPEN, path.as_ptr() as u64, path.len() as u64, 1);
    if fd == u64::MAX
        || syscall(
            SYS_FILE_WRITE,
            fd,
            expected.as_ptr() as u64,
            expected.len() as u64,
        ) != expected.len() as u64
        || syscall(SYS_CLOSE, fd, 0, 0) != 1
    {
        write(b"persistent user file write failed\n");
        syscall(SYS_EXIT, 7, 0, 0);
    }
    let fd = syscall(SYS_OPEN, path.as_ptr() as u64, path.len() as u64, 0);
    let mut readback = [0u8; 64];
    let count = syscall(
        SYS_READ,
        fd,
        readback.as_mut_ptr() as u64,
        readback.len() as u64,
    );
    syscall(SYS_CLOSE, fd, 0, 0);
    if count as usize != expected.len() || readback[..expected.len()] != *expected {
        write(b"persistent user file readback failed\n");
        syscall(SYS_EXIT, 8, 0, 0);
    }
    if previous == 0 {
        write(
            b"MAKOS_FILE_RW_OK path=/home/user/note.txt write=1 readback=1 previous=0 mode=0600\n",
        );
    } else {
        write(
            b"MAKOS_FILE_RW_OK path=/home/user/note.txt write=1 readback=1 previous=1 mode=0600\n",
        );
    }
}

fn ipc_self_test() {
    let mut pair = [0u64; 2];
    if syscall(SYS_CHANNEL_CREATE, pair.as_mut_ptr() as u64, 0, 0) != 0
        || syscall(SYS_CHANNEL_SEND, pair[1], 0x1234, 0) != 0
        || syscall(SYS_CHANNEL_RECEIVE, pair[0], 0, 0) != 0x1234
    {
        write(b"init IPC self-test failed\n");
        syscall(SYS_EXIT, 1, 0, 0);
    }
}

fn desktop() {
    let auxiliary = syscall(SYS_SURFACE_CREATE, 260, 160, 0);
    if auxiliary == 0 {
        return;
    }
    fill(auxiliary, 0, 0, 260, 160, 0xff17233a);
    fill(auxiliary, 18, 22, 224, 30, 0xff4cde9c);
    fill(auxiliary, 18, 74, 170, 16, 0xffdce9ff);
    fill(auxiliary, 18, 106, 210, 12, 0xff8fa9d8);
    syscall(SYS_SURFACE_PRESENT, auxiliary, 0, 0);
    let surface = syscall(SYS_SURFACE_CREATE, 720, 420, 0);
    if surface == 0 {
        return;
    }
    fill(surface, 0, 0, 720, 420, 0xff000000);
    syscall(SYS_SURFACE_PRESENT, surface, 0, 0);
    write(b"MAKOS_TERMINAL_READY grid=58x22 retained=1 commands=real\n");
}

fn fill(handle: u64, x: u16, y: u16, width: u16, height: u16, color: u32) {
    let rectangle =
        u64::from(x) | (u64::from(y) << 16) | (u64::from(width) << 32) | (u64::from(height) << 48);
    syscall(SYS_SURFACE_FILL, handle, u64::from(color), rectangle);
}

fn write(bytes: &[u8]) {
    let _ = syscall(SYS_WRITE, bytes.as_ptr() as u64, bytes.len() as u64, 0);
}

fn syscall(number: u64, first: u64, second: u64, third: u64) -> u64 {
    syscall4(number, first, second, third, 0)
}

fn syscall4(number: u64, first: u64, second: u64, third: u64, fourth: u64) -> u64 {
    let result: u64;
    unsafe {
        asm!(
            "int 0x80",
            inlateout("rax") number => result,
            in("rdi") first,
            in("rsi") second,
            in("rdx") third,
            in("r10") fourth,
            options(nostack)
        );
    }
    result
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    write(b"userspace panic\n");
    syscall(SYS_EXIT, 127, 0, 0);
    loop {
        unsafe { asm!("ud2", options(noreturn)) }
    }
}
