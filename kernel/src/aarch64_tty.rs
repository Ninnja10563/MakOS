use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use makos_tty::{ByteSink, LineDiscipline, ReadError, ReceiveResult, Signal, Termios, WindowSize};

const MAX_PROCESSES: usize = 32;
const MAX_TASKS: usize = 128;
const INPUT_BYTES: usize = 4096;
const LINE_BYTES: usize = 1024;
const RECORDS: usize = 64;
const DEFAULT_ROWS: u16 = 22;
const DEFAULT_COLUMNS: u16 = 58;
const DEFAULT_PIXEL_WIDTH: u16 = 720;
const DEFAULT_PIXEL_HEIGHT: u16 = 420;
pub const SIGINT: u32 = 2;
pub const SIGQUIT: u32 = 3;
pub const SIGBUS: u32 = 7;
pub const SIGCONT: u32 = 18;
pub const SIGTSTP: u32 = 20;
pub const SIGWINCH: u32 = 28;
const SIGKILL: u32 = 9;
const SIGSTOP: u32 = 19;
pub const SIG_DFL: u64 = 0;
pub const SIG_IGN: u64 = 1;
const SIGNAL_COUNT: usize = 64;
const SA_NOCLDSTOP: u64 = 0x0000_0001;
const SA_NOCLDWAIT: u64 = 0x0000_0002;
const SA_SIGINFO: u64 = 0x0000_0004;
const SA_RESTORER: u64 = 0x0400_0000;
const SA_ONSTACK: u64 = 0x0800_0000;
const SA_RESTART: u64 = 0x1000_0000;
const SA_NODEFER: u64 = 0x4000_0000;
const SA_RESETHAND: u64 = 0x8000_0000;
const SUPPORTED_ACTION_FLAGS: u64 = SA_NOCLDSTOP
    | SA_NOCLDWAIT
    | SA_SIGINFO
    | SA_RESTORER
    | SA_ONSTACK
    | SA_RESTART
    | SA_NODEFER
    | SA_RESETHAND;

pub const TCSANOW: u64 = 0;
pub const TCSADRAIN: u64 = 1;
pub const TCSAFLUSH: u64 = 2;
pub const TCIFLUSH: u64 = 0;
pub const TCOFLUSH: u64 = 1;
pub const TCIOFLUSH: u64 = 2;
pub const TIOCGWINSZ: u64 = 0x5413;
pub const TIOCSWINSZ: u64 = 0x5414;

const ICRNL: u32 = 0x0100;
const OPOST: u32 = 0x0001;
const ONLCR: u32 = 0x0004;
const ISIG: u32 = 0x0001;
const ICANON: u32 = 0x0002;
const ECHO: u32 = 0x0008;
const ECHOE: u32 = 0x0010;
const ECHOK: u32 = 0x0020;
const NOFLSH: u32 = 0x0080;
const VINTR: usize = 0;
const VQUIT: usize = 1;
const VERASE: usize = 2;
const VKILL: usize = 3;
const VEOF: usize = 4;
const VTIME: usize = 5;
const VMIN: usize = 6;
const VSUSP: usize = 10;

type KernelLineDiscipline = LineDiscipline<INPUT_BYTES, LINE_BYTES, RECORDS>;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserTermios {
    pub input_flags: u32,
    pub output_flags: u32,
    pub control_flags: u32,
    pub local_flags: u32,
    pub control_characters: [u8; 32],
    pub input_speed: u32,
    pub output_speed: u32,
}

const _: () = assert!(core::mem::size_of::<UserTermios>() == 56);

impl UserTermios {
    fn from_kernel(value: Termios) -> Self {
        let mut control_characters = [0; 32];
        control_characters[VINTR] = value.interrupt;
        control_characters[VQUIT] = value.quit;
        control_characters[VERASE] = value.erase;
        control_characters[VKILL] = value.kill;
        control_characters[VEOF] = value.eof;
        control_characters[VTIME] = 0;
        control_characters[VMIN] = value.minimum_read;
        control_characters[VSUSP] = value.suspend;
        Self {
            input_flags: if value.map_cr_to_nl { ICRNL } else { 0 },
            output_flags: if value.output_crlf { OPOST | ONLCR } else { 0 },
            control_flags: 0,
            local_flags: if value.signals { ISIG } else { 0 }
                | if value.canonical { ICANON } else { 0 }
                | if value.echo { ECHO } else { 0 }
                | if value.echo_erase { ECHOE } else { 0 }
                | if value.echo_kill { ECHOK } else { 0 }
                | if value.no_flush_on_signal { NOFLSH } else { 0 },
            control_characters,
            input_speed: 38_400,
            output_speed: 38_400,
        }
    }

    fn to_kernel(self) -> Result<Termios, Errno> {
        if self.control_characters[VTIME] != 0 {
            return Err(Errno::NotSupported);
        }
        Ok(Termios {
            canonical: self.local_flags & ICANON != 0,
            echo: self.local_flags & ECHO != 0,
            echo_erase: self.local_flags & ECHOE != 0,
            echo_kill: self.local_flags & ECHOK != 0,
            signals: self.local_flags & ISIG != 0,
            no_flush_on_signal: self.local_flags & NOFLSH != 0,
            map_cr_to_nl: self.input_flags & ICRNL != 0,
            output_crlf: self.output_flags & OPOST != 0 && self.output_flags & ONLCR != 0,
            minimum_read: self.control_characters[VMIN],
            erase: self.control_characters[VERASE],
            kill: self.control_characters[VKILL],
            eof: self.control_characters[VEOF],
            interrupt: self.control_characters[VINTR],
            quit: self.control_characters[VQUIT],
            suspend: self.control_characters[VSUSP],
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserWindowSize {
    pub rows: u16,
    pub columns: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

const _: () = assert!(core::mem::size_of::<UserWindowSize>() == 8);

impl From<WindowSize> for UserWindowSize {
    fn from(value: WindowSize) -> Self {
        Self {
            rows: value.rows,
            columns: value.columns,
            pixel_width: value.pixel_width,
            pixel_height: value.pixel_height,
        }
    }
}

impl From<UserWindowSize> for WindowSize {
    fn from(value: UserWindowSize) -> Self {
        Self::new(
            value.rows,
            value.columns,
            value.pixel_width,
            value.pixel_height,
        )
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserSignalAction {
    pub handler: u64,
    pub flags: u64,
    pub restorer: u64,
    pub mask: u64,
}

const _: () = assert!(core::mem::size_of::<UserSignalAction>() == 32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Errno {
    Permission,
    NoSuchProcess,
    Interrupted,
    BadFileDescriptor,
    TryAgain,
    Invalid,
    NotTty,
    NotSupported,
}

impl Errno {
    pub const fn abi(self) -> u64 {
        let errno = match self {
            Self::Permission => 1i64,
            Self::NoSuchProcess => 3,
            Self::Interrupted => 4i64,
            Self::BadFileDescriptor => 9,
            Self::TryAgain => 11,
            Self::Invalid => 22,
            Self::NotTty => 25,
            Self::NotSupported => 95,
        };
        (-errno) as u64
    }
}

#[derive(Clone, Copy)]
struct SignalAction {
    handler: u64,
    flags: u64,
    restorer: u64,
    mask: u64,
}

impl SignalAction {
    const DEFAULT: Self = Self {
        handler: SIG_DFL,
        flags: 0,
        restorer: 0,
        mask: 0,
    };

    fn user(self) -> UserSignalAction {
        UserSignalAction {
            handler: self.handler,
            flags: self.flags,
            restorer: self.restorer,
            mask: self.mask,
        }
    }
}

#[derive(Clone, Copy)]
struct ProcessState {
    pid: u64,
    session_id: u64,
    process_group: u64,
    open_fds: u8,
    pending: u64,
    actions: [SignalAction; SIGNAL_COUNT],
}

impl ProcessState {
    const EMPTY: Self = Self {
        pid: 0,
        session_id: 0,
        process_group: 0,
        open_fds: 0,
        pending: 0,
        actions: [SignalAction::DEFAULT; SIGNAL_COUNT],
    };
}

#[derive(Clone, Copy)]
struct TaskSignalState {
    tid: u64,
    group_pid: u64,
    pending: u64,
    blocked: u64,
    saved_mask: u64,
    in_handler: bool,
    saved_context: crate::arch::UserContext,
    wait_mask_active: bool,
    wait_saved_mask: u64,
}

impl TaskSignalState {
    const EMPTY: Self = Self {
        tid: 0,
        group_pid: 0,
        pending: 0,
        blocked: 0,
        saved_mask: 0,
        in_handler: false,
        saved_context: crate::arch::UserContext::initial(0, 0, 0, 0),
        wait_mask_active: false,
        wait_saved_mask: 0,
    };
}

struct State {
    line: KernelLineDiscipline,
    window_size: WindowSize,
    controlling_session: u64,
    foreground_process_group: u64,
    processes: [ProcessState; MAX_PROCESSES],
    tasks: [TaskSignalState; MAX_TASKS],
}

impl State {
    const fn new() -> Self {
        Self {
            line: KernelLineDiscipline::new(Termios::sane()),
            window_size: WindowSize::new(
                DEFAULT_ROWS,
                DEFAULT_COLUMNS,
                DEFAULT_PIXEL_WIDTH,
                DEFAULT_PIXEL_HEIGHT,
            ),
            controlling_session: 0,
            foreground_process_group: 0,
            processes: [ProcessState::EMPTY; MAX_PROCESSES],
            tasks: [TaskSignalState::EMPTY; MAX_TASKS],
        }
    }

    fn register(&mut self, pid: u64, parent_pid: u64) -> bool {
        if pid == 0 {
            return false;
        }
        if let Some(process) = self.processes.iter_mut().find(|entry| entry.pid == pid) {
            process.open_fds |= 0b111;
            return self.ensure_main_task(pid);
        }
        let inherited = (parent_pid != 0)
            .then(|| {
                self.processes
                    .iter()
                    .find(|entry| entry.pid == parent_pid)
                    .map(|parent| (parent.session_id, parent.process_group))
            })
            .flatten();
        let Some(slot) = self.processes.iter_mut().find(|entry| entry.pid == 0) else {
            return false;
        };
        let (session_id, process_group) = inherited.unwrap_or((pid, pid));
        *slot = ProcessState {
            pid,
            session_id,
            process_group,
            open_fds: 0b111,
            ..ProcessState::EMPTY
        };
        if self.controlling_session == 0 {
            self.controlling_session = session_id;
            self.foreground_process_group = process_group;
        }
        if self.ensure_main_task(pid) {
            true
        } else {
            let index = self.process_index(pid).expect("new TTY process absent");
            self.processes[index] = ProcessState::EMPTY;
            false
        }
    }

    fn ensure_main_task(&mut self, pid: u64) -> bool {
        if self.tasks.iter().any(|task| task.tid == pid) {
            return true;
        }
        let Some(task) = self.tasks.iter_mut().find(|task| task.tid == 0) else {
            return false;
        };
        *task = TaskSignalState {
            tid: pid,
            group_pid: pid,
            ..TaskSignalState::EMPTY
        };
        true
    }

    fn process_index(&self, pid: u64) -> Option<usize> {
        self.processes.iter().position(|entry| entry.pid == pid)
    }

    fn task_index(&self, tid: u64) -> Option<usize> {
        self.tasks.iter().position(|entry| entry.tid == tid)
    }

    fn ensure_process(&mut self, pid: u64) -> Option<usize> {
        if let Some(index) = self.process_index(pid) {
            return Some(index);
        }
        self.register(pid, 0)
            .then(|| self.process_index(pid))
            .flatten()
    }

    fn fd_open(&mut self, pid: u64, fd: u64) -> bool {
        fd < 3
            && self
                .ensure_process(pid)
                .is_some_and(|index| self.processes[index].open_fds & (1 << fd) != 0)
    }

    fn queue_process_group(&mut self, process_group: u64, signal: u32) -> usize {
        let Some(bit) = signal_bit(signal) else {
            return 0;
        };
        let mut count = 0;
        for process in &mut self.processes {
            if process.pid != 0 && process.process_group == process_group {
                process.pending |= bit;
                count += 1;
            }
        }
        count
    }
}

struct LockedState {
    lock: AtomicBool,
    state: UnsafeCell<State>,
}

unsafe impl Sync for LockedState {}

static STATE: LockedState = LockedState {
    lock: AtomicBool::new(false),
    state: UnsafeCell::new(State::new()),
};

struct TerminalSink;

impl ByteSink for TerminalSink {
    fn write(&mut self, bytes: &[u8]) {
        crate::serial::write_bytes(bytes);
        crate::graphics::terminal_write(bytes);
    }
}

pub enum Delivery {
    None,
    Handler,
    Terminate(u32),
    Stop(u32),
}

pub fn initialize() {
    with_state(|state| *state = State::new());
    self_test();
    crate::serial_println!(
        "MAKOS_AARCH64_TTY_OK fds=0,1,2 controlling=1 canonical=1 raw=1 termios=1 ioctl_winsize=1 pgrp=1 signals=INT,QUIT,TSTP,WINCH sigreturn=kernel-saved"
    );
}

pub fn register_process(pid: u64, parent_pid: u64) -> bool {
    with_state(|state| state.register(pid, parent_pid))
}

/// Give an interactive child its own process group and controlling-TTY
/// foreground. Called only after kernel-validated shell spawn.
pub fn make_foreground_child(pid: u64, parent_pid: u64) -> bool {
    with_state(|state| {
        let Some(child_index) = state.process_index(pid) else {
            return false;
        };
        let Some(parent_index) = state.process_index(parent_pid) else {
            return false;
        };
        if state.processes[child_index].session_id != state.processes[parent_index].session_id {
            return false;
        }
        state.processes[child_index].process_group = pid;
        state.foreground_process_group = pid;
        true
    })
}

/// Fork process-owned signal state and caller task mask. Pending signals are
/// cleared in child; session, process group, dispositions, and TTY FDs inherit.
pub fn fork_process(parent_pid: u64, child_pid: u64, parent_tid: u64) -> bool {
    if parent_pid == 0 || child_pid == 0 || parent_tid == 0 {
        return false;
    }
    with_state(|state| {
        let Some(parent_process) = state
            .processes
            .iter()
            .copied()
            .find(|process| process.pid == parent_pid)
        else {
            return false;
        };
        let Some(parent_task) = state
            .tasks
            .iter()
            .copied()
            .find(|task| task.tid == parent_tid && task.group_pid == parent_pid)
        else {
            return false;
        };
        let Some(process_slot) = state.processes.iter_mut().find(|process| process.pid == 0) else {
            return false;
        };
        *process_slot = ProcessState {
            pid: child_pid,
            pending: 0,
            ..parent_process
        };
        let Some(task_slot) = state.tasks.iter_mut().find(|task| task.tid == 0) else {
            *process_slot = ProcessState::EMPTY;
            return false;
        };
        *task_slot = TaskSignalState {
            tid: child_pid,
            group_pid: child_pid,
            blocked: parent_task.blocked,
            ..TaskSignalState::EMPTY
        };
        true
    })
}

pub fn register_thread(tid: u64, group_pid: u64, parent_tid: u64) -> bool {
    if tid == 0 || group_pid == 0 || parent_tid == 0 {
        return false;
    }
    with_state(|state| {
        if state.process_index(group_pid).is_none() || state.task_index(tid).is_some() {
            return false;
        }
        let Some(parent) = state
            .tasks
            .iter()
            .find(|task| task.tid == parent_tid && task.group_pid == group_pid)
            .copied()
        else {
            return false;
        };
        let Some(slot) = state.tasks.iter_mut().find(|task| task.tid == 0) else {
            return false;
        };
        *slot = TaskSignalState {
            tid,
            group_pid,
            blocked: parent.blocked,
            ..TaskSignalState::EMPTY
        };
        true
    })
}

pub fn close_thread(tid: u64) -> bool {
    with_state(|state| {
        let Some(index) = state.task_index(tid) else {
            return false;
        };
        state.tasks[index] = TaskSignalState::EMPTY;
        true
    })
}

pub fn close_process(pid: u64) -> usize {
    with_state(|state| {
        let Some(index) = state.process_index(pid) else {
            return 0;
        };
        let count = state.processes[index].open_fds.count_ones() as usize;
        state.processes[index] = ProcessState::EMPTY;
        for task in &mut state.tasks {
            if task.group_pid == pid {
                *task = TaskSignalState::EMPTY;
            }
        }
        count
    })
}

pub fn close(fd: u64) -> Result<(), Errno> {
    let pid = crate::aarch64_process::current_pid();
    with_state(|state| {
        let index = state.ensure_process(pid).ok_or(Errno::BadFileDescriptor)?;
        if fd >= 3 || state.processes[index].open_fds & (1 << fd) == 0 {
            return Err(Errno::BadFileDescriptor);
        }
        state.processes[index].open_fds &= !(1 << fd);
        Ok(())
    })
}

pub fn isatty(fd: u64) -> bool {
    let pid = crate::aarch64_process::current_pid();
    with_state(|state| state.fd_open(pid, fd))
}

pub fn read(fd: u64, output: &mut [u8]) -> Result<usize, Errno> {
    let pid = crate::aarch64_process::current_pid();
    if output.is_empty() {
        return Ok(0);
    }
    crate::aarch64_virtio_input::poll();
    with_state(|state| {
        if fd != 0 || !state.fd_open(pid, fd) {
            return Err(Errno::BadFileDescriptor);
        }
        let mut sink = TerminalSink;
        let mut interrupted = false;
        while let Some(byte) = crate::aarch64_virtio_input::read_key() {
            if let ReceiveResult::Signal(signal) = state.line.receive(byte, &mut sink) {
                let signal = match signal {
                    Signal::Interrupt => SIGINT,
                    Signal::Quit => SIGQUIT,
                    Signal::Suspend => SIGTSTP,
                };
                interrupted |=
                    state.queue_process_group(state.foreground_process_group, signal) != 0;
            }
        }
        match state.line.read(output) {
            Ok(count) => Ok(count),
            Err(ReadError::WouldBlock) if interrupted => Err(Errno::Interrupted),
            Err(ReadError::WouldBlock) => Err(Errno::TryAgain),
        }
    })
}

pub fn write(fd: u64, bytes: &[u8]) -> Result<usize, Errno> {
    let pid = crate::aarch64_process::current_pid();
    with_state(|state| {
        if !matches!(fd, 1 | 2) || !state.fd_open(pid, fd) {
            return Err(Errno::BadFileDescriptor);
        }
        let mut sink = TerminalSink;
        state.line.write_output(bytes, &mut sink);
        Ok(bytes.len())
    })
}

pub fn poll_events(fd: u64, requested: u16) -> u16 {
    const POLLIN: u16 = 0x001;
    const POLLOUT: u16 = 0x004;
    const POLLNVAL: u16 = 0x020;
    let pid = crate::aarch64_process::current_pid();
    if fd == 0 {
        crate::aarch64_virtio_input::poll();
    }
    let (events, signaled) = with_state(|state| {
        if !state.fd_open(pid, fd) {
            return (POLLNVAL, false);
        }
        let mut signaled = false;
        if fd == 0 {
            let mut sink = TerminalSink;
            while let Some(byte) = crate::aarch64_virtio_input::read_key() {
                if let ReceiveResult::Signal(signal) = state.line.receive(byte, &mut sink) {
                    let signal = match signal {
                        Signal::Interrupt => SIGINT,
                        Signal::Quit => SIGQUIT,
                        Signal::Suspend => SIGTSTP,
                    };
                    signaled |=
                        state.queue_process_group(state.foreground_process_group, signal) != 0;
                }
            }
        }
        let events = match fd {
            0 if state.line.poll_readable() => requested & POLLIN,
            0 => 0,
            1 | 2 => requested & POLLOUT,
            _ => POLLNVAL,
        };
        (events, signaled)
    });
    if signaled {
        crate::aarch64_process::wake_io_waiters();
    }
    events
}

pub fn get_termios(fd: u64) -> Result<UserTermios, Errno> {
    let pid = crate::aarch64_process::current_pid();
    with_state(|state| {
        if !state.fd_open(pid, fd) {
            return Err(Errno::NotTty);
        }
        Ok(UserTermios::from_kernel(state.line.termios()))
    })
}

pub fn set_termios(fd: u64, action: u64, value: UserTermios) -> Result<(), Errno> {
    let pid = crate::aarch64_process::current_pid();
    with_state(|state| {
        if !state.fd_open(pid, fd) {
            return Err(Errno::NotTty);
        }
        if !matches!(action, TCSANOW | TCSADRAIN | TCSAFLUSH) {
            return Err(Errno::Invalid);
        }
        if action == TCSAFLUSH {
            state.line.flush_input();
        }
        state
            .line
            .set_termios(value.to_kernel()?)
            .map_err(|_| Errno::TryAgain)
    })
}

pub fn flush(fd: u64, queue: u64) -> Result<(), Errno> {
    let pid = crate::aarch64_process::current_pid();
    with_state(|state| {
        if !state.fd_open(pid, fd) {
            return Err(Errno::NotTty);
        }
        match queue {
            TCIFLUSH | TCIOFLUSH => state.line.flush_input(),
            TCOFLUSH => {}
            _ => return Err(Errno::Invalid),
        }
        Ok(())
    })
}

pub fn window_size(fd: u64) -> Result<UserWindowSize, Errno> {
    let pid = crate::aarch64_process::current_pid();
    with_state(|state| {
        if !state.fd_open(pid, fd) {
            return Err(Errno::NotTty);
        }
        Ok(state.window_size.into())
    })
}

pub fn set_window_size(fd: u64, value: UserWindowSize) -> Result<(), Errno> {
    let pid = crate::aarch64_process::current_pid();
    let changed = with_state(|state| {
        if !state.fd_open(pid, fd) {
            return Err(Errno::NotTty);
        }
        if value.rows == 0 || value.columns == 0 {
            return Err(Errno::Invalid);
        }
        let value = WindowSize::from(value);
        if value != state.window_size {
            state.window_size = value;
            state.queue_process_group(state.foreground_process_group, SIGWINCH);
            return Ok(true);
        }
        Ok(false)
    })?;
    if changed {
        crate::aarch64_process::wake_io_waiters();
    }
    Ok(())
}

pub fn set_window_size_from_compositor(value: UserWindowSize) {
    let changed = with_state(|state| {
        let value = WindowSize::from(value);
        if value.rows != 0 && value.columns != 0 && value != state.window_size {
            state.window_size = value;
            state.queue_process_group(state.foreground_process_group, SIGWINCH);
            true
        } else {
            false
        }
    });
    if changed {
        crate::aarch64_process::wake_io_waiters();
    }
}

pub fn get_process_group() -> Result<u64, Errno> {
    let pid = crate::aarch64_process::current_pid();
    with_state(|state| {
        let index = state.ensure_process(pid).ok_or(Errno::Invalid)?;
        Ok(state.processes[index].process_group)
    })
}

pub fn set_process_group(target_pid: u64, process_group: u64) -> Result<(), Errno> {
    let caller = crate::aarch64_process::current_pid();
    let target = if target_pid == 0 { caller } else { target_pid };
    let group = if process_group == 0 {
        target
    } else {
        process_group
    };
    with_state(|state| {
        if target != caller || group == 0 {
            return Err(Errno::Invalid);
        }
        let index = state.ensure_process(caller).ok_or(Errno::Invalid)?;
        let session = state.processes[index].session_id;
        if let Some(member) = state
            .processes
            .iter()
            .find(|entry| entry.pid != 0 && entry.process_group == group)
        {
            if member.session_id != session {
                return Err(Errno::Invalid);
            }
        }
        state.processes[index].process_group = group;
        Ok(())
    })
}

pub fn terminal_process_group(fd: u64) -> Result<u64, Errno> {
    let pid = crate::aarch64_process::current_pid();
    with_state(|state| {
        if !state.fd_open(pid, fd) {
            return Err(Errno::NotTty);
        }
        Ok(state.foreground_process_group)
    })
}

pub fn set_terminal_process_group(fd: u64, process_group: u64) -> Result<(), Errno> {
    let pid = crate::aarch64_process::current_pid();
    with_state(|state| {
        if !state.fd_open(pid, fd) || process_group == 0 {
            return Err(Errno::NotTty);
        }
        let caller = state.ensure_process(pid).ok_or(Errno::Invalid)?;
        let session = state.processes[caller].session_id;
        if session != state.controlling_session
            || !state.processes.iter().any(|entry| {
                entry.pid != 0
                    && entry.process_group == process_group
                    && entry.session_id == session
            })
        {
            return Err(Errno::Invalid);
        }
        state.foreground_process_group = process_group;
        Ok(())
    })
}

pub fn signal_action(
    signal: u32,
    replacement: Option<UserSignalAction>,
) -> Result<UserSignalAction, Errno> {
    let action_index = signal_index(signal).ok_or(Errno::Invalid)?;
    if matches!(signal, SIGKILL | SIGSTOP) {
        return Err(Errno::Invalid);
    }
    let pid = crate::aarch64_process::current_pid();
    with_state(|state| {
        let index = state.ensure_process(pid).ok_or(Errno::Invalid)?;
        let previous = state.processes[index].actions[action_index].user();
        if let Some(replacement) = replacement {
            let handler_executable = crate::arch::user_address_executable(replacement.handler);
            let restorer_executable = crate::arch::user_address_executable(replacement.restorer);
            if replacement.flags & !SUPPORTED_ACTION_FLAGS != 0
                || (replacement.handler > SIG_IGN && (!handler_executable || !restorer_executable))
            {
                crate::serial_println!(
                    "signal-action-denied signal={} flags={:#x} handler={:#x}:{} restorer={:#x}:{}",
                    signal,
                    replacement.flags,
                    replacement.handler,
                    handler_executable,
                    replacement.restorer,
                    restorer_executable
                );
                return Err(Errno::Invalid);
            }
            state.processes[index].actions[action_index] = SignalAction {
                handler: replacement.handler,
                flags: replacement.flags,
                restorer: replacement.restorer,
                mask: sanitize_signal_mask(replacement.mask),
            };
        }
        Ok(previous)
    })
}

/// POSIX exec transition: preserve pending signals, mask, session, process
/// group, and TTY descriptors; reset caught dispositions and handler frames.
pub fn exec_process(pid: u64) -> bool {
    with_state(|state| {
        let Some(process_index) = state.process_index(pid) else {
            return false;
        };
        for action in &mut state.processes[process_index].actions {
            if action.handler != SIG_IGN {
                *action = SignalAction::DEFAULT;
            }
        }
        let Some(task_index) = state.task_index(pid) else {
            return false;
        };
        let task = &mut state.tasks[task_index];
        task.in_handler = false;
        task.saved_mask = 0;
        task.saved_context = crate::arch::UserContext::initial(0, 0, 0, 0);
        task.wait_mask_active = false;
        task.wait_saved_mask = 0;
        true
    })
}

/// POSIX/Linux `SIG_BLOCK`, `SIG_UNBLOCK`, and `SIG_SETMASK` semantics. Signal
/// dispositions and pending process-directed signals remain process-owned;
/// masks are task-owned and inherited by `clone`.
pub fn signal_mask(how: u64, replacement: Option<u64>) -> Result<u64, Errno> {
    let tid = crate::aarch64_process::current_tid();
    with_state(|state| {
        let index = state.task_index(tid).ok_or(Errno::Invalid)?;
        let previous = state.tasks[index].blocked;
        if let Some(replacement) = replacement {
            if state.tasks[index].wait_mask_active {
                return Err(Errno::TryAgain);
            }
            let replacement = sanitize_signal_mask(replacement);
            state.tasks[index].blocked = match how {
                0 => previous | replacement,
                1 => previous & !replacement,
                2 => replacement,
                _ => return Err(Errno::Invalid),
            };
        } else if how > 2 {
            return Err(Errno::Invalid);
        }
        Ok(previous)
    })
}

/// Installs `ppoll`/`epoll_pwait`'s temporary mask once. Syscall retry after a
/// scheduler wake observes same active mask rather than replacing saved mask.
pub fn begin_wait_mask(replacement: Option<u64>) -> Result<(), Errno> {
    let Some(replacement) = replacement else {
        return Ok(());
    };
    let tid = crate::aarch64_process::current_tid();
    with_state(|state| {
        let index = state.task_index(tid).ok_or(Errno::Invalid)?;
        let replacement = sanitize_signal_mask(replacement);
        let task = &mut state.tasks[index];
        if task.wait_mask_active {
            return (task.blocked == replacement)
                .then_some(())
                .ok_or(Errno::Invalid);
        }
        task.wait_saved_mask = task.blocked;
        task.blocked = replacement;
        task.wait_mask_active = true;
        Ok(())
    })
}

pub fn finish_wait_mask() {
    let tid = crate::aarch64_process::current_tid();
    with_state(|state| {
        let Some(index) = state.task_index(tid) else {
            return;
        };
        let task = &mut state.tasks[index];
        if task.wait_mask_active {
            task.blocked = task.wait_saved_mask;
            task.wait_saved_mask = 0;
            task.wait_mask_active = false;
        }
    });
}

/// Returns true only for signals whose current disposition can interrupt a
/// masked wait. Default-ignored signals are consumed without a spurious EINTR.
pub fn wait_interrupted() -> bool {
    let pid = crate::aarch64_process::current_pid();
    let tid = crate::aarch64_process::current_tid();
    with_state(|state| {
        let Some(process_index) = state.process_index(pid) else {
            return false;
        };
        let Some(task_index) = state.task_index(tid) else {
            return false;
        };
        loop {
            let task_deliverable =
                state.tasks[task_index].pending & !state.tasks[task_index].blocked;
            let process_deliverable =
                state.processes[process_index].pending & !state.tasks[task_index].blocked;
            let (deliverable, task_directed) = if task_deliverable != 0 {
                (task_deliverable, true)
            } else {
                (process_deliverable, false)
            };
            let Some(signal) = first_signal(deliverable) else {
                return false;
            };
            let bit = signal_bit(signal).expect("selected supported signal");
            let action = state.processes[process_index].actions
                [signal_index(signal).expect("selected signal index")];
            if action.handler == SIG_IGN
                || (action.handler == SIG_DFL && matches!(signal, SIGWINCH | SIGCONT))
            {
                if task_directed {
                    state.tasks[task_index].pending &= !bit;
                } else {
                    state.processes[process_index].pending &= !bit;
                }
                continue;
            }
            return true;
        }
    })
}

pub fn raise(signal: u32) -> Result<(), Errno> {
    let bit = signal_bit(signal).ok_or(Errno::Invalid)?;
    let pid = crate::aarch64_process::current_pid();
    let result = with_state(|state| {
        let index = state.ensure_process(pid).ok_or(Errno::Invalid)?;
        state.processes[index].pending |= bit;
        Ok(())
    });
    if result.is_ok() && signal == SIGCONT {
        crate::aarch64_process::continue_process(pid);
    }
    if result.is_ok() {
        crate::aarch64_process::wake_io_waiters();
    }
    result
}

pub fn kill(target: i64, signal: u32) -> Result<usize, Errno> {
    if signal != 0 && signal_bit(signal).is_none() {
        return Err(Errno::Invalid);
    }
    let caller_pid = crate::aarch64_process::current_pid();
    let mut targets = [0u64; MAX_PROCESSES];
    let target_count = with_state(|state| {
        let caller_group = state
            .process_index(caller_pid)
            .map(|index| state.processes[index].process_group)
            .unwrap_or(0);
        let requested_group = if target == 0 {
            Some(caller_group)
        } else if target < -1 {
            target.checked_neg().map(|value| value as u64)
        } else {
            None
        };
        let mut count = 0usize;
        for process in &state.processes {
            if process.pid == 0 {
                continue;
            }
            let selected = if target > 0 {
                process.pid == target as u64
            } else if target == -1 {
                process.pid != 1 || caller_pid == 1
            } else {
                requested_group == Some(process.process_group)
            };
            if selected {
                targets[count] = process.pid;
                count += 1;
            }
        }
        count
    });
    if target_count == 0 {
        return Err(Errno::NoSuchProcess);
    }
    let mut permitted = [0u64; MAX_PROCESSES];
    let mut permitted_count = 0usize;
    for pid in targets.into_iter().take(target_count) {
        if crate::security::may_signal_process(pid) {
            permitted[permitted_count] = pid;
            permitted_count += 1;
        }
    }
    if permitted_count == 0 {
        return Err(Errno::Permission);
    }
    if signal != 0 {
        let bit = signal_bit(signal).expect("validated signal");
        with_state(|state| {
            for pid in permitted.into_iter().take(permitted_count) {
                if let Some(index) = state.process_index(pid) {
                    state.processes[index].pending |= bit;
                }
            }
        });
        if signal == SIGCONT {
            for pid in permitted.into_iter().take(permitted_count) {
                for tid in task_ids_for_process(pid) {
                    if tid != 0 {
                        crate::aarch64_process::wake_task(pid, tid);
                    }
                }
            }
        }
        crate::aarch64_process::wake_io_waiters();
    }
    Ok(permitted_count)
}

pub fn kill_task(group_pid: u64, tid: u64, signal: u32) -> Result<(), Errno> {
    if signal != 0 && signal_bit(signal).is_none() {
        return Err(Errno::Invalid);
    }
    let resolved_group = if group_pid == 0 {
        crate::aarch64_process::current_pid()
    } else {
        group_pid
    };
    let exists = with_state(|state| {
        state
            .tasks
            .iter()
            .any(|task| task.tid == tid && task.group_pid == resolved_group)
    });
    if !exists {
        return Err(Errno::NoSuchProcess);
    }
    if !crate::security::may_signal_process(resolved_group) {
        return Err(Errno::Permission);
    }
    if signal != 0 {
        let bit = signal_bit(signal).expect("validated signal");
        with_state(|state| {
            let index = state.task_index(tid).expect("validated signal task absent");
            state.tasks[index].pending |= bit;
        });
        if signal == SIGCONT {
            crate::aarch64_process::wake_task(resolved_group, tid);
        }
        crate::aarch64_process::wake_io_waiters();
    }
    Ok(())
}

fn task_ids_for_process(group_pid: u64) -> [u64; MAX_TASKS] {
    with_state(|state| {
        let mut tids = [0u64; MAX_TASKS];
        let mut count = 0usize;
        for task in &state.tasks {
            if task.tid != 0 && task.group_pid == group_pid {
                tids[count] = task.tid;
                count += 1;
            }
        }
        tids
    })
}

pub fn send_process_group(process_group: u64, signal: u32) -> Result<usize, Errno> {
    if process_group == 0 || signal_bit(signal).is_none() {
        return Err(Errno::Invalid);
    }
    let mut members = [0u64; MAX_PROCESSES];
    let count = with_state(|state| {
        let count = state.queue_process_group(process_group, signal);
        if signal == SIGCONT {
            let mut output = 0;
            for process in &state.processes {
                if process.pid != 0 && process.process_group == process_group {
                    members[output] = process.pid;
                    output += 1;
                }
            }
        }
        count
    });
    if count == 0 {
        return Err(Errno::Invalid);
    }
    if signal == SIGCONT {
        for pid in members.into_iter().take(count) {
            crate::aarch64_process::continue_process(pid);
        }
    }
    crate::aarch64_process::wake_io_waiters();
    Ok(count)
}

pub fn deliver_pending(frame: &mut crate::arch::ExceptionFrame) -> Delivery {
    let pid = crate::aarch64_process::current_pid();
    let tid = crate::aarch64_process::current_tid();
    with_state(|state| {
        let Some(process_index) = state.process_index(pid) else {
            return Delivery::None;
        };
        let Some(task_index) = state.task_index(tid) else {
            return Delivery::None;
        };
        if state.tasks[task_index].in_handler {
            return Delivery::None;
        }
        loop {
            let task_deliverable =
                state.tasks[task_index].pending & !state.tasks[task_index].blocked;
            let process_deliverable =
                state.processes[process_index].pending & !state.tasks[task_index].blocked;
            let (deliverable, task_directed) = if task_deliverable != 0 {
                (task_deliverable, true)
            } else {
                (process_deliverable, false)
            };
            let Some(signal) = first_signal(deliverable) else {
                return Delivery::None;
            };
            let bit = signal_bit(signal).expect("selected supported signal");
            if task_directed {
                state.tasks[task_index].pending &= !bit;
            } else {
                state.processes[process_index].pending &= !bit;
            }
            let action = state.processes[process_index].actions
                [signal_index(signal).expect("selected signal index")];
            if action.handler == SIG_IGN
                || (action.handler == SIG_DFL && matches!(signal, SIGWINCH | SIGCONT))
            {
                continue;
            }
            let task = &mut state.tasks[task_index];
            let restored_mask = if task.wait_mask_active {
                task.wait_mask_active = false;
                let saved = task.wait_saved_mask;
                task.wait_saved_mask = 0;
                saved
            } else {
                task.blocked
            };
            if action.handler == SIG_DFL {
                task.blocked = restored_mask;
                return if signal == SIGTSTP {
                    Delivery::Stop(signal)
                } else {
                    Delivery::Terminate(signal)
                };
            }
            task.saved_context = crate::arch::UserContext::capture(frame);
            task.saved_mask = restored_mask;
            task.blocked = restored_mask
                | action.mask
                | if action.flags & SA_NODEFER == 0 {
                    bit
                } else {
                    0
                };
            task.in_handler = true;
            if action.flags & SA_RESETHAND != 0 {
                state.processes[process_index].actions
                    [signal_index(signal).expect("selected signal index")] = SignalAction::DEFAULT;
            }
            frame.registers[0] = u64::from(signal);
            frame.registers[1] = 0;
            frame.registers[2] = 0;
            frame.registers[30] = action.restorer;
            frame.elr = action.handler;
            frame.spsr = 0;
            return Delivery::Handler;
        }
    })
}

pub fn signal_return(frame: &mut crate::arch::ExceptionFrame) -> Result<(), Errno> {
    let tid = crate::aarch64_process::current_tid();
    let context = with_state(|state| {
        let index = state.task_index(tid).ok_or(Errno::Invalid)?;
        let task = &mut state.tasks[index];
        if !task.in_handler {
            return Err(Errno::Invalid);
        }
        task.in_handler = false;
        task.blocked = task.saved_mask;
        Ok(task.saved_context)
    })?;
    context.restore(frame);
    Ok(())
}

fn with_state<R>(function: impl FnOnce(&mut State) -> R) -> R {
    while STATE
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *STATE.state.get() });
    STATE.lock.store(false, Ordering::Release);
    result
}

fn signal_index(signal: u32) -> Option<usize> {
    (1..=SIGNAL_COUNT as u32)
        .contains(&signal)
        .then_some(signal as usize - 1)
}

fn signal_bit(signal: u32) -> Option<u64> {
    signal_index(signal).map(|_| 1u64 << (signal - 1))
}

fn sanitize_signal_mask(mask: u64) -> u64 {
    mask & !(1u64 << (SIGKILL - 1)) & !(1u64 << (SIGSTOP - 1))
}

fn first_signal(mask: u64) -> Option<u32> {
    (1..=SIGNAL_COUNT as u32).find(|signal| mask & signal_bit(*signal).unwrap_or(0) != 0)
}

fn self_test() {
    let mut state = State::new();
    let first = state.register(1, 0);
    let second = state.register(2, 1);
    let queued = state.queue_process_group(1, SIGWINCH);
    if !first
        || !second
        || state.processes[0].process_group != 1
        || state.processes[1].process_group != 1
        || !state.fd_open(1, 0)
        || !state.fd_open(1, 1)
        || !state.fd_open(1, 2)
        || state.task_index(1).is_none()
        || state.task_index(2).is_none()
        || queued != 2
        || state.processes[0].pending & signal_bit(SIGWINCH).unwrap_or(0) == 0
    {
        crate::fatal("AArch64 TTY ownership/process-group self-test failed");
    }
    let sane = UserTermios::from_kernel(Termios::sane());
    let raw = UserTermios::from_kernel(Termios::raw());
    let sane_roundtrip = sane.to_kernel() == Ok(Termios::sane());
    let raw_roundtrip = raw.to_kernel() == Ok(Termios::raw());
    if !sane_roundtrip
        || !raw_roundtrip
        || sane.control_characters[VTIME] != 0
        || core::mem::size_of::<UserWindowSize>() != 8
        || core::mem::size_of::<UserSignalAction>() != 32
    {
        crate::fatal("AArch64 TTY ABI/termios self-test failed");
    }
}
