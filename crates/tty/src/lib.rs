#![no_std]

//! Allocation-free TTY line discipline and ANSI/VT parser.
//!
//! Kernel code owns locking, blocking/wakeup, signal delivery, file
//! descriptions, rendering, and copy_to/from_user. This crate owns bounded
//! terminal state only; it performs no allocation, MMIO, or architecture work.

/// POSIX-compatible terminal size used by `TIOCGWINSZ`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct WindowSize {
    pub rows: u16,
    pub columns: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

impl WindowSize {
    pub const fn new(rows: u16, columns: u16, pixel_width: u16, pixel_height: u16) -> Self {
        Self {
            rows,
            columns,
            pixel_width,
            pixel_height,
        }
    }
}

/// Line-discipline controls corresponding to the termios settings nano and
/// ncurses use. Syscall code translates native `struct termios` flags to this
/// stable kernel-internal representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Termios {
    /// `ICANON`
    pub canonical: bool,
    /// `ECHO`
    pub echo: bool,
    /// `ECHOE`
    pub echo_erase: bool,
    /// `ECHOK`
    pub echo_kill: bool,
    /// `ISIG`
    pub signals: bool,
    /// `NOFLSH`; when false, generated signals flush pending input.
    pub no_flush_on_signal: bool,
    /// `ICRNL`
    pub map_cr_to_nl: bool,
    /// `OPOST | ONLCR`
    pub output_crlf: bool,
    /// `VMIN`; `VTIME` remains kernel timer policy.
    pub minimum_read: u8,
    pub erase: u8,
    pub kill: u8,
    pub eof: u8,
    pub interrupt: u8,
    pub quit: u8,
    pub suspend: u8,
}

impl Termios {
    pub const fn sane() -> Self {
        Self {
            canonical: true,
            echo: true,
            echo_erase: true,
            echo_kill: true,
            signals: true,
            no_flush_on_signal: false,
            map_cr_to_nl: true,
            output_crlf: true,
            minimum_read: 1,
            erase: 0x7f,
            kill: 0x15,
            eof: 0x04,
            interrupt: 0x03,
            quit: 0x1c,
            suspend: 0x1a,
        }
    }

    /// Equivalent control behavior to `cfmakeraw`: byte-at-a-time input,
    /// no echo, no terminal-generated signals, no CR/NL translation.
    pub const fn raw() -> Self {
        Self {
            canonical: false,
            echo: false,
            echo_erase: false,
            echo_kill: false,
            signals: false,
            no_flush_on_signal: false,
            map_cr_to_nl: false,
            output_crlf: false,
            minimum_read: 1,
            erase: 0x7f,
            kill: 0x15,
            eof: 0x04,
            interrupt: 0x03,
            quit: 0x1c,
            suspend: 0x1a,
        }
    }
}

impl Default for Termios {
    fn default() -> Self {
        Self::sane()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Signal {
    Interrupt,
    Quit,
    Suspend,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiveResult {
    Accepted,
    LineReady,
    EndOfFileReady,
    Signal(Signal),
    /// Fixed-capacity queue or current line cannot accept input.
    Full,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadError {
    WouldBlock,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetTermiosError {
    InputFull,
    /// Raw bytes must be read or flushed before enabling canonical mode.
    PendingRawInput,
}

/// Byte destination used for echo and output post-processing.
pub trait ByteSink {
    fn write(&mut self, bytes: &[u8]);
}

struct ByteQueue<const N: usize> {
    bytes: [u8; N],
    head: usize,
    length: usize,
}

impl<const N: usize> ByteQueue<N> {
    const fn new() -> Self {
        Self {
            bytes: [0; N],
            head: 0,
            length: 0,
        }
    }

    fn available(&self) -> usize {
        self.length
    }

    fn remaining(&self) -> usize {
        N - self.length
    }

    fn push(&mut self, byte: u8) -> bool {
        if self.length == N || N == 0 {
            return false;
        }
        let tail = (self.head + self.length) % N;
        self.bytes[tail] = byte;
        self.length += 1;
        true
    }

    fn pop(&mut self) -> Option<u8> {
        if self.length == 0 || N == 0 {
            return None;
        }
        let byte = self.bytes[self.head];
        self.head = (self.head + 1) % N;
        self.length -= 1;
        Some(byte)
    }

    fn clear(&mut self) {
        self.head = 0;
        self.length = 0;
    }
}

struct RecordQueue<const N: usize> {
    lengths: [usize; N],
    head: usize,
    length: usize,
}

impl<const N: usize> RecordQueue<N> {
    const fn new() -> Self {
        Self {
            lengths: [0; N],
            head: 0,
            length: 0,
        }
    }

    fn push(&mut self, value: usize) -> bool {
        if self.length == N || N == 0 {
            return false;
        }
        let tail = (self.head + self.length) % N;
        self.lengths[tail] = value;
        self.length += 1;
        true
    }

    fn front(&self) -> Option<usize> {
        (self.length != 0 && N != 0).then(|| self.lengths[self.head])
    }

    fn consume(&mut self, count: usize) {
        if self.length == 0 || N == 0 {
            return;
        }
        if count < self.lengths[self.head] {
            self.lengths[self.head] -= count;
        } else {
            self.head = (self.head + 1) % N;
            self.length -= 1;
        }
    }

    fn clear(&mut self) {
        self.head = 0;
        self.length = 0;
    }
}

/// Bounded POSIX-like line discipline.
///
/// `INPUT` bounds readable input, `LINE` bounds one canonical edit line, and
/// `RECORDS` bounds committed canonical records (including zero-length EOF).
pub struct LineDiscipline<const INPUT: usize, const LINE: usize, const RECORDS: usize> {
    termios: Termios,
    input: ByteQueue<INPUT>,
    records: RecordQueue<RECORDS>,
    line: [u8; LINE],
    line_length: usize,
}

impl<const INPUT: usize, const LINE: usize, const RECORDS: usize>
    LineDiscipline<INPUT, LINE, RECORDS>
{
    pub const fn new(termios: Termios) -> Self {
        Self {
            termios,
            input: ByteQueue::new(),
            records: RecordQueue::new(),
            line: [0; LINE],
            line_length: 0,
        }
    }

    pub const fn termios(&self) -> Termios {
        self.termios
    }

    /// Changes mode without discarding data. Pending canonical edit bytes move
    /// to raw input when capacity permits; otherwise remain pending until flush.
    pub fn set_termios(&mut self, termios: Termios) -> Result<(), SetTermiosError> {
        if self.termios.canonical && !termios.canonical && self.line_length != 0 {
            if self.input.remaining() < self.line_length {
                return Err(SetTermiosError::InputFull);
            }
            for index in 0..self.line_length {
                let pushed = self.input.push(self.line[index]);
                debug_assert!(pushed);
            }
            self.line_length = 0;
        }
        if self.termios.canonical && !termios.canonical {
            self.records.clear();
        }
        if !self.termios.canonical && termios.canonical && self.input.available() != 0 {
            return Err(SetTermiosError::PendingRawInput);
        }
        self.termios = termios;
        Ok(())
    }

    pub fn readable_bytes(&self) -> usize {
        if self.termios.canonical {
            self.records.front().unwrap_or(0)
        } else {
            self.input.available()
        }
    }

    pub fn poll_readable(&self) -> bool {
        if self.termios.canonical {
            self.records.front().is_some()
        } else {
            let threshold = usize::from(self.termios.minimum_read);
            threshold == 0 || self.input.available() >= threshold
        }
    }

    /// Receives one byte from keyboard/input driver. Returned signals must be
    /// delivered by kernel to foreground process group; this crate never sends
    /// signals itself.
    pub fn receive<S: ByteSink>(&mut self, mut byte: u8, echo: &mut S) -> ReceiveResult {
        if self.termios.map_cr_to_nl && byte == b'\r' {
            byte = b'\n';
        }

        if self.termios.signals {
            let signal = if byte == self.termios.interrupt {
                Some(Signal::Interrupt)
            } else if byte == self.termios.quit {
                Some(Signal::Quit)
            } else if byte == self.termios.suspend {
                Some(Signal::Suspend)
            } else {
                None
            };
            if let Some(signal) = signal {
                if !self.termios.no_flush_on_signal {
                    self.flush_input();
                }
                if self.termios.echo {
                    echo.write(&[b'^', control_letter(byte), b'\r', b'\n']);
                }
                return ReceiveResult::Signal(signal);
            }
        }

        if !self.termios.canonical {
            if !self.input.push(byte) {
                return ReceiveResult::Full;
            }
            if self.termios.echo {
                echo.write(&[byte]);
            }
            return ReceiveResult::Accepted;
        }

        if byte == self.termios.erase {
            if self.line_length != 0 {
                self.line_length -= 1;
                if self.termios.echo {
                    if self.termios.echo_erase {
                        echo.write(b"\x08 \x08");
                    } else {
                        echo.write(&[byte]);
                    }
                }
            }
            return ReceiveResult::Accepted;
        }

        if byte == self.termios.kill {
            let erased = self.line_length;
            self.line_length = 0;
            if self.termios.echo {
                if self.termios.echo_kill && self.termios.echo_erase {
                    for _ in 0..erased {
                        echo.write(b"\x08 \x08");
                    }
                } else {
                    echo.write(&[b'^', control_letter(byte), b'\r', b'\n']);
                }
            }
            return ReceiveResult::Accepted;
        }

        if byte == self.termios.eof {
            if !self.commit_line(false) {
                return ReceiveResult::Full;
            }
            return ReceiveResult::EndOfFileReady;
        }

        if byte == b'\n' {
            if !self.commit_line(true) {
                return ReceiveResult::Full;
            }
            if self.termios.echo {
                echo.write(b"\r\n");
            }
            return ReceiveResult::LineReady;
        }

        if self.line_length == LINE || LINE == 0 {
            return ReceiveResult::Full;
        }
        self.line[self.line_length] = byte;
        self.line_length += 1;
        if self.termios.echo {
            echo.write(&[byte]);
        }
        ReceiveResult::Accepted
    }

    /// Reads at most one canonical record, or currently available raw bytes.
    /// Kernel maps `WouldBlock` to sleep/EAGAIN according to file status flags.
    pub fn read(&mut self, output: &mut [u8]) -> Result<usize, ReadError> {
        if self.termios.canonical {
            let record = self.records.front().ok_or(ReadError::WouldBlock)?;
            if record == 0 {
                self.records.consume(0);
                return Ok(0);
            }
            let count = record.min(output.len());
            for slot in output.iter_mut().take(count) {
                *slot = self.input.pop().expect("record length matches input");
            }
            self.records.consume(count);
            return Ok(count);
        }

        let threshold = usize::from(self.termios.minimum_read).min(output.len());
        if self.input.available() < threshold || (self.input.available() == 0 && threshold != 0) {
            return Err(ReadError::WouldBlock);
        }
        let count = self.input.available().min(output.len());
        for slot in output.iter_mut().take(count) {
            *slot = self.input.pop().expect("available count checked");
        }
        Ok(count)
    }

    /// Implements `tcflush(fd, TCIFLUSH)` state reset.
    pub fn flush_input(&mut self) {
        self.input.clear();
        self.records.clear();
        self.line_length = 0;
    }

    /// Applies `OPOST | ONLCR` without allocation.
    pub fn write_output<S: ByteSink>(&self, bytes: &[u8], sink: &mut S) {
        if !self.termios.output_crlf {
            sink.write(bytes);
            return;
        }
        let mut start = 0;
        for (index, byte) in bytes.iter().copied().enumerate() {
            if byte == b'\n' {
                if start != index {
                    sink.write(&bytes[start..index]);
                }
                sink.write(b"\r\n");
                start = index + 1;
            }
        }
        if start != bytes.len() {
            sink.write(&bytes[start..]);
        }
    }

    fn commit_line(&mut self, newline: bool) -> bool {
        let count = self.line_length + usize::from(newline);
        if self.input.remaining() < count || self.records.length == RECORDS || RECORDS == 0 {
            return false;
        }
        for index in 0..self.line_length {
            let pushed = self.input.push(self.line[index]);
            debug_assert!(pushed);
        }
        if newline {
            let pushed = self.input.push(b'\n');
            debug_assert!(pushed);
        }
        let pushed = self.records.push(count);
        debug_assert!(pushed);
        self.line_length = 0;
        true
    }
}

fn control_letter(byte: u8) -> u8 {
    if byte == 0x7f { b'?' } else { byte ^ 0x40 }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EraseDisplay {
    CursorToEnd,
    StartToCursor,
    All,
    Scrollback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EraseLine {
    CursorToEnd,
    StartToCursor,
    All,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Attributes {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub reverse: bool,
    /// ANSI palette index 0..=15; `None` means terminal default.
    pub foreground: Option<u8>,
    pub background: Option<u8>,
}

impl Attributes {
    pub const fn default_terminal() -> Self {
        Self {
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            blink: false,
            reverse: false,
            foreground: None,
            background: None,
        }
    }
}

impl Default for Attributes {
    fn default() -> Self {
        Self::default_terminal()
    }
}

/// Rendering contract. Implementor clamps positions to current dimensions and
/// performs scrolling. Rows/columns are zero-based here; ANSI parser converts
/// one-based CSI coordinates.
pub trait TerminalBackend {
    fn put_byte(&mut self, byte: u8);
    fn carriage_return(&mut self);
    fn line_feed(&mut self);
    fn backspace(&mut self);
    fn tab(&mut self);
    fn bell(&mut self) {}
    fn move_cursor(&mut self, row: u16, column: u16);
    fn set_cursor_column(&mut self, column: u16);
    fn move_relative(&mut self, row_delta: i16, column_delta: i16);
    fn erase_display(&mut self, mode: EraseDisplay);
    fn erase_line(&mut self, mode: EraseLine);
    fn set_attributes(&mut self, attributes: Attributes);
    fn set_cursor_visible(&mut self, visible: bool);
    fn use_alternate_screen(&mut self, enabled: bool);
    fn save_cursor(&mut self);
    fn restore_cursor(&mut self);
    fn set_scroll_region(&mut self, top: u16, bottom: Option<u16>);
    fn scroll(&mut self, lines: i16);
    fn insert_blank(&mut self, _count: u16) {}
    fn delete_characters(&mut self, _count: u16) {}
    fn erase_characters(&mut self, _count: u16) {}
    fn insert_lines(&mut self, _count: u16) {}
    fn delete_lines(&mut self, _count: u16) {}
    fn set_application_cursor_keys(&mut self, _enabled: bool) {}
    fn set_application_keypad(&mut self, _enabled: bool) {}
    fn set_auto_wrap(&mut self, _enabled: bool) {}
    fn set_bracketed_paste(&mut self, _enabled: bool) {}
    fn set_insert_mode(&mut self, _enabled: bool) {}
    fn request_report(&mut self, _report: Report) {}
    fn reset(&mut self);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Report {
    DeviceStatus,
    CursorPosition,
    PrimaryDeviceAttributes,
}

const PARAMETER_CAPACITY: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParserState {
    Ground,
    Escape,
    Csi,
    IgnoreCsi,
    Osc,
    OscEscape,
    ControlString,
    ControlStringEscape,
}

/// Bounded ECMA-48/VT parser covering ncurses' core terminal operations.
pub struct AnsiParser {
    state: ParserState,
    parameters: [u16; PARAMETER_CAPACITY],
    parameter_count: usize,
    value_started: bool,
    private: bool,
    attributes: Attributes,
}

impl AnsiParser {
    pub const fn new() -> Self {
        Self {
            state: ParserState::Ground,
            parameters: [0; PARAMETER_CAPACITY],
            parameter_count: 0,
            value_started: false,
            private: false,
            attributes: Attributes::default_terminal(),
        }
    }

    pub const fn attributes(&self) -> Attributes {
        self.attributes
    }

    pub fn advance<B: TerminalBackend>(&mut self, bytes: &[u8], backend: &mut B) {
        for byte in bytes.iter().copied() {
            self.advance_byte(byte, backend);
        }
    }

    pub fn advance_byte<B: TerminalBackend>(&mut self, byte: u8, backend: &mut B) {
        match self.state {
            ParserState::Ground => match byte {
                0x1b => self.state = ParserState::Escape,
                b'\r' => backend.carriage_return(),
                b'\n' | 0x0b | 0x0c => backend.line_feed(),
                0x08 => backend.backspace(),
                b'\t' => backend.tab(),
                0x07 => backend.bell(),
                0x20..=0xff => backend.put_byte(byte),
                _ => {}
            },
            ParserState::Escape => {
                self.state = ParserState::Ground;
                match byte {
                    b'[' => self.start_csi(),
                    b']' => self.state = ParserState::Osc,
                    b'P' | b'^' | b'_' => self.state = ParserState::ControlString,
                    b'7' => backend.save_cursor(),
                    b'8' => backend.restore_cursor(),
                    b'=' => backend.set_application_keypad(true),
                    b'>' => backend.set_application_keypad(false),
                    b'D' => backend.move_relative(1, 0),
                    b'E' => {
                        backend.move_relative(1, 0);
                        backend.carriage_return();
                    }
                    b'M' => backend.scroll(-1),
                    b'c' => {
                        self.attributes = Attributes::default_terminal();
                        backend.reset();
                    }
                    0x1b => self.state = ParserState::Escape,
                    _ => {}
                }
            }
            ParserState::Csi => self.advance_csi(byte, backend),
            ParserState::IgnoreCsi => {
                if (0x40..=0x7e).contains(&byte) {
                    self.state = ParserState::Ground;
                } else if byte == 0x1b {
                    self.state = ParserState::Escape;
                }
            }
            ParserState::Osc => match byte {
                0x07 => self.state = ParserState::Ground,
                0x1b => self.state = ParserState::OscEscape,
                _ => {}
            },
            ParserState::OscEscape => {
                if byte == b'\\' {
                    self.state = ParserState::Ground;
                } else if byte != 0x1b {
                    self.state = ParserState::Osc;
                }
            }
            ParserState::ControlString => {
                if byte == 0x1b {
                    self.state = ParserState::ControlStringEscape;
                }
            }
            ParserState::ControlStringEscape => {
                if byte == b'\\' {
                    self.state = ParserState::Ground;
                } else if byte != 0x1b {
                    self.state = ParserState::ControlString;
                }
            }
        }
    }

    fn start_csi(&mut self) {
        self.state = ParserState::Csi;
        self.parameters = [0; PARAMETER_CAPACITY];
        self.parameter_count = 1;
        self.value_started = false;
        self.private = false;
    }

    fn advance_csi<B: TerminalBackend>(&mut self, byte: u8, backend: &mut B) {
        match byte {
            b'?' if !self.value_started && self.parameter_count == 1 => self.private = true,
            b'0'..=b'9' => {
                self.value_started = true;
                let index = self.parameter_count - 1;
                let digit = u16::from(byte - b'0');
                self.parameters[index] = self.parameters[index]
                    .saturating_mul(10)
                    .saturating_add(digit);
            }
            b';' => {
                if self.parameter_count == PARAMETER_CAPACITY {
                    self.state = ParserState::IgnoreCsi;
                } else {
                    self.parameter_count += 1;
                    self.value_started = false;
                }
            }
            0x40..=0x7e => {
                self.dispatch_csi(byte, backend);
                self.state = ParserState::Ground;
            }
            0x1b => self.state = ParserState::Escape,
            _ => self.state = ParserState::IgnoreCsi,
        }
    }

    fn parameter(&self, index: usize, default: u16) -> u16 {
        self.parameters
            .get(index)
            .copied()
            .filter(|value| *value != 0)
            .unwrap_or(default)
    }

    fn dispatch_csi<B: TerminalBackend>(&mut self, final_byte: u8, backend: &mut B) {
        if self.private {
            if matches!(final_byte, b'h' | b'l') {
                let enabled = final_byte == b'h';
                for mode in self.parameters.iter().copied().take(self.parameter_count) {
                    match mode {
                        1 => backend.set_application_cursor_keys(enabled),
                        7 => backend.set_auto_wrap(enabled),
                        25 => backend.set_cursor_visible(enabled),
                        47 | 1047 | 1049 => backend.use_alternate_screen(enabled),
                        2004 => backend.set_bracketed_paste(enabled),
                        _ => {}
                    }
                }
            }
            return;
        }

        let amount = self.parameter(0, 1).min(i16::MAX as u16) as i16;
        match final_byte {
            b'A' => backend.move_relative(-amount, 0),
            b'B' => backend.move_relative(amount, 0),
            b'C' => backend.move_relative(0, amount),
            b'D' => backend.move_relative(0, -amount),
            b'E' => {
                backend.move_relative(amount, 0);
                backend.carriage_return();
            }
            b'F' => {
                backend.move_relative(-amount, 0);
                backend.carriage_return();
            }
            b'G' | b'`' => backend.set_cursor_column(self.parameter(0, 1) - 1),
            b'H' | b'f' => backend.move_cursor(self.parameter(0, 1) - 1, self.parameter(1, 1) - 1),
            b'J' => backend.erase_display(match self.parameters[0] {
                1 => EraseDisplay::StartToCursor,
                2 => EraseDisplay::All,
                3 => EraseDisplay::Scrollback,
                _ => EraseDisplay::CursorToEnd,
            }),
            b'K' => backend.erase_line(match self.parameters[0] {
                1 => EraseLine::StartToCursor,
                2 => EraseLine::All,
                _ => EraseLine::CursorToEnd,
            }),
            b'@' => backend.insert_blank(self.parameter(0, 1)),
            b'L' => backend.insert_lines(self.parameter(0, 1)),
            b'M' => backend.delete_lines(self.parameter(0, 1)),
            b'P' => backend.delete_characters(self.parameter(0, 1)),
            b'X' => backend.erase_characters(self.parameter(0, 1)),
            b'm' => self.dispatch_sgr(backend),
            b's' => backend.save_cursor(),
            b'u' => backend.restore_cursor(),
            b'r' => backend.set_scroll_region(
                self.parameter(0, 1) - 1,
                self.parameters
                    .get(1)
                    .copied()
                    .filter(|value| *value != 0)
                    .map(|v| v - 1),
            ),
            b'S' => backend.scroll(amount),
            b'T' => backend.scroll(-amount),
            b'h' | b'l' => {
                let enabled = final_byte == b'h';
                for mode in self.parameters.iter().copied().take(self.parameter_count) {
                    if mode == 4 {
                        backend.set_insert_mode(enabled);
                    }
                }
            }
            b'n' => match self.parameters[0] {
                5 => backend.request_report(Report::DeviceStatus),
                6 => backend.request_report(Report::CursorPosition),
                _ => {}
            },
            b'c' => backend.request_report(Report::PrimaryDeviceAttributes),
            _ => {}
        }
    }

    fn dispatch_sgr<B: TerminalBackend>(&mut self, backend: &mut B) {
        for value in self.parameters.iter().copied().take(self.parameter_count) {
            match value {
                0 => self.attributes = Attributes::default_terminal(),
                1 => self.attributes.bold = true,
                2 => self.attributes.dim = true,
                3 => self.attributes.italic = true,
                4 => self.attributes.underline = true,
                5 => self.attributes.blink = true,
                7 => self.attributes.reverse = true,
                22 => {
                    self.attributes.bold = false;
                    self.attributes.dim = false;
                }
                23 => self.attributes.italic = false,
                24 => self.attributes.underline = false,
                25 => self.attributes.blink = false,
                27 => self.attributes.reverse = false,
                30..=37 => self.attributes.foreground = Some((value - 30) as u8),
                39 => self.attributes.foreground = None,
                40..=47 => self.attributes.background = Some((value - 40) as u8),
                49 => self.attributes.background = None,
                90..=97 => self.attributes.foreground = Some((value - 90 + 8) as u8),
                100..=107 => self.attributes.background = Some((value - 100 + 8) as u8),
                _ => {}
            }
        }
        backend.set_attributes(self.attributes);
    }
}

impl Default for AnsiParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::vec::Vec;

    #[derive(Default)]
    struct Bytes(Vec<u8>);

    impl ByteSink for Bytes {
        fn write(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
    }

    #[test]
    fn canonical_input_waits_for_line_and_erases_without_artifact() {
        let mut tty = LineDiscipline::<32, 16, 4>::new(Termios::sane());
        let mut echo = Bytes::default();
        assert_eq!(tty.receive(b'a', &mut echo), ReceiveResult::Accepted);
        assert_eq!(tty.receive(b'b', &mut echo), ReceiveResult::Accepted);
        assert_eq!(tty.readable_bytes(), 0);
        assert_eq!(tty.read(&mut [0; 8]), Err(ReadError::WouldBlock));
        assert_eq!(tty.receive(0x7f, &mut echo), ReceiveResult::Accepted);
        assert_eq!(tty.receive(b'c', &mut echo), ReceiveResult::Accepted);
        assert_eq!(tty.receive(b'\r', &mut echo), ReceiveResult::LineReady);
        let mut output = [0; 8];
        assert_eq!(tty.read(&mut output), Ok(3));
        assert_eq!(&output[..3], b"ac\n");
        assert_eq!(&echo.0, b"ab\x08 \x08c\r\n");
    }

    #[test]
    fn canonical_reads_never_cross_record_boundary() {
        let mut tty = LineDiscipline::<32, 16, 4>::new(Termios::sane());
        let mut echo = Bytes::default();
        for byte in b"one\ntwo\n" {
            tty.receive(*byte, &mut echo);
        }
        let mut output = [0; 16];
        assert_eq!(tty.read(&mut output), Ok(4));
        assert_eq!(&output[..4], b"one\n");
        assert_eq!(tty.read(&mut output), Ok(4));
        assert_eq!(&output[..4], b"two\n");
    }

    #[test]
    fn eof_is_zero_length_record_or_commits_pending_text() {
        let mut tty = LineDiscipline::<16, 8, 4>::new(Termios::sane());
        let mut echo = Bytes::default();
        assert_eq!(tty.receive(0x04, &mut echo), ReceiveResult::EndOfFileReady);
        assert_eq!(tty.read(&mut [0; 4]), Ok(0));
        tty.receive(b'x', &mut echo);
        tty.receive(0x04, &mut echo);
        let mut output = [0; 4];
        assert_eq!(tty.read(&mut output), Ok(1));
        assert_eq!(output[0], b'x');
    }

    #[test]
    fn raw_mode_delivers_punctuation_and_lowercase_unchanged() {
        let mut tty = LineDiscipline::<32, 8, 4>::new(Termios::raw());
        let mut echo = Bytes::default();
        for byte in b"a-z_9!?" {
            assert_eq!(tty.receive(*byte, &mut echo), ReceiveResult::Accepted);
        }
        let mut output = [0; 16];
        assert_eq!(tty.read(&mut output), Ok(7));
        assert_eq!(&output[..7], b"a-z_9!?");
        assert!(echo.0.is_empty());
    }

    #[test]
    fn signal_character_is_not_queued() {
        let mut tty = LineDiscipline::<16, 8, 4>::new(Termios::sane());
        let mut echo = Bytes::default();
        tty.receive(b'x', &mut echo);
        assert_eq!(
            tty.receive(0x03, &mut echo),
            ReceiveResult::Signal(Signal::Interrupt)
        );
        tty.receive(b'\n', &mut echo);
        let mut output = [0; 4];
        assert_eq!(tty.read(&mut output), Ok(1));
        assert_eq!(output[0], b'\n');
        assert_eq!(&echo.0, b"x^C\r\n\r\n");
    }

    #[test]
    fn signal_flushes_committed_input_unless_noflsh_is_set() {
        let mut tty = LineDiscipline::<16, 8, 4>::new(Termios::sane());
        let mut echo = Bytes::default();
        for byte in b"old\n" {
            tty.receive(*byte, &mut echo);
        }
        tty.receive(0x03, &mut echo);
        assert_eq!(tty.read(&mut [0; 8]), Err(ReadError::WouldBlock));

        let mut noflush = Termios::sane();
        noflush.no_flush_on_signal = true;
        let mut tty = LineDiscipline::<16, 8, 4>::new(noflush);
        for byte in b"keep\n" {
            tty.receive(*byte, &mut echo);
        }
        tty.receive(0x03, &mut echo);
        let mut output = [0; 8];
        assert_eq!(tty.read(&mut output), Ok(5));
        assert_eq!(&output[..5], b"keep\n");
    }

    #[test]
    fn output_postprocessing_maps_lf_only_when_enabled() {
        let cooked = LineDiscipline::<8, 8, 2>::new(Termios::sane());
        let raw = LineDiscipline::<8, 8, 2>::new(Termios::raw());
        let mut cooked_bytes = Bytes::default();
        let mut raw_bytes = Bytes::default();
        cooked.write_output(b"a\nb", &mut cooked_bytes);
        raw.write_output(b"a\nb", &mut raw_bytes);
        assert_eq!(&cooked_bytes.0, b"a\r\nb");
        assert_eq!(&raw_bytes.0, b"a\nb");
    }

    #[test]
    fn bounded_queue_wraps_without_losing_record_boundaries() {
        let mut tty = LineDiscipline::<5, 4, 3>::new(Termios::sane());
        let mut echo = Bytes::default();
        for byte in b"a\nb\n" {
            assert_ne!(tty.receive(*byte, &mut echo), ReceiveResult::Full);
        }
        let mut output = [0; 4];
        assert_eq!(tty.read(&mut output), Ok(2));
        assert_eq!(&output[..2], b"a\n");
        for byte in b"cd\n" {
            assert_ne!(tty.receive(*byte, &mut echo), ReceiveResult::Full);
        }
        assert_eq!(tty.read(&mut output), Ok(2));
        assert_eq!(&output[..2], b"b\n");
        assert_eq!(tty.read(&mut output), Ok(3));
        assert_eq!(&output[..3], b"cd\n");
    }

    #[test]
    fn mode_switch_preserves_pending_canonical_bytes_and_rejects_hidden_raw_bytes() {
        let mut tty = LineDiscipline::<8, 8, 2>::new(Termios::sane());
        let mut echo = Bytes::default();
        tty.receive(b'a', &mut echo);
        tty.receive(b'\n', &mut echo);
        tty.receive(b'x', &mut echo);
        assert_eq!(tty.set_termios(Termios::raw()), Ok(()));
        let mut output = [0; 4];
        assert_eq!(tty.read(&mut output), Ok(3));
        assert_eq!(&output[..3], b"a\nx");
        tty.receive(b'y', &mut echo);
        assert_eq!(
            tty.set_termios(Termios::sane()),
            Err(SetTermiosError::PendingRawInput)
        );
        tty.flush_input();
        assert_eq!(tty.set_termios(Termios::sane()), Ok(()));
    }

    #[derive(Debug, Eq, PartialEq)]
    enum Op {
        Byte(u8),
        Cr,
        Lf,
        Backspace,
        Tab,
        Move(u16, u16),
        Relative(i16, i16),
        EraseDisplay(EraseDisplay),
        EraseLine(EraseLine),
        Attributes(Attributes),
        Cursor(bool),
        Alternate(bool),
        Save,
        Restore,
        Region(u16, Option<u16>),
        Scroll(i16),
        Edit(&'static str, u16),
        Mode(&'static str, bool),
        Report(Report),
        Reset,
    }

    #[derive(Default)]
    struct Backend(Vec<Op>);

    impl TerminalBackend for Backend {
        fn put_byte(&mut self, byte: u8) {
            self.0.push(Op::Byte(byte));
        }
        fn carriage_return(&mut self) {
            self.0.push(Op::Cr);
        }
        fn line_feed(&mut self) {
            self.0.push(Op::Lf);
        }
        fn backspace(&mut self) {
            self.0.push(Op::Backspace);
        }
        fn tab(&mut self) {
            self.0.push(Op::Tab);
        }
        fn move_cursor(&mut self, row: u16, column: u16) {
            self.0.push(Op::Move(row, column));
        }
        fn set_cursor_column(&mut self, column: u16) {
            self.0.push(Op::Move(u16::MAX, column));
        }
        fn move_relative(&mut self, row_delta: i16, column_delta: i16) {
            self.0.push(Op::Relative(row_delta, column_delta));
        }
        fn erase_display(&mut self, mode: EraseDisplay) {
            self.0.push(Op::EraseDisplay(mode));
        }
        fn erase_line(&mut self, mode: EraseLine) {
            self.0.push(Op::EraseLine(mode));
        }
        fn set_attributes(&mut self, attributes: Attributes) {
            self.0.push(Op::Attributes(attributes));
        }
        fn set_cursor_visible(&mut self, visible: bool) {
            self.0.push(Op::Cursor(visible));
        }
        fn use_alternate_screen(&mut self, enabled: bool) {
            self.0.push(Op::Alternate(enabled));
        }
        fn save_cursor(&mut self) {
            self.0.push(Op::Save);
        }
        fn restore_cursor(&mut self) {
            self.0.push(Op::Restore);
        }
        fn set_scroll_region(&mut self, top: u16, bottom: Option<u16>) {
            self.0.push(Op::Region(top, bottom));
        }
        fn scroll(&mut self, lines: i16) {
            self.0.push(Op::Scroll(lines));
        }
        fn insert_blank(&mut self, count: u16) {
            self.0.push(Op::Edit("insert-blank", count));
        }
        fn delete_characters(&mut self, count: u16) {
            self.0.push(Op::Edit("delete-characters", count));
        }
        fn erase_characters(&mut self, count: u16) {
            self.0.push(Op::Edit("erase-characters", count));
        }
        fn insert_lines(&mut self, count: u16) {
            self.0.push(Op::Edit("insert-lines", count));
        }
        fn delete_lines(&mut self, count: u16) {
            self.0.push(Op::Edit("delete-lines", count));
        }
        fn set_application_cursor_keys(&mut self, enabled: bool) {
            self.0.push(Op::Mode("application-cursor", enabled));
        }
        fn set_application_keypad(&mut self, enabled: bool) {
            self.0.push(Op::Mode("application-keypad", enabled));
        }
        fn set_auto_wrap(&mut self, enabled: bool) {
            self.0.push(Op::Mode("auto-wrap", enabled));
        }
        fn set_bracketed_paste(&mut self, enabled: bool) {
            self.0.push(Op::Mode("bracketed-paste", enabled));
        }
        fn set_insert_mode(&mut self, enabled: bool) {
            self.0.push(Op::Mode("insert", enabled));
        }
        fn request_report(&mut self, report: Report) {
            self.0.push(Op::Report(report));
        }
        fn reset(&mut self) {
            self.0.push(Op::Reset);
        }
    }

    #[test]
    fn ansi_parser_handles_ncurses_core_sequences_incrementally() {
        let mut parser = AnsiParser::new();
        let mut backend = Backend::default();
        parser.advance(b"hi\x1b[", &mut backend);
        parser.advance(b"3;5H\x1b[2K\x1b[1;4;7;94mX", &mut backend);
        assert_eq!(backend.0[0], Op::Byte(b'h'));
        assert_eq!(backend.0[1], Op::Byte(b'i'));
        assert_eq!(backend.0[2], Op::Move(2, 4));
        assert_eq!(backend.0[3], Op::EraseLine(EraseLine::All));
        assert_eq!(
            backend.0[4],
            Op::Attributes(Attributes {
                bold: true,
                dim: false,
                italic: false,
                underline: true,
                blink: false,
                reverse: true,
                foreground: Some(12),
                background: None,
            })
        );
        assert_eq!(backend.0[5], Op::Byte(b'X'));
    }

    #[test]
    fn ansi_parser_handles_alternate_screen_cursor_and_scroll_region() {
        let mut parser = AnsiParser::new();
        let mut backend = Backend::default();
        parser.advance(
            b"\x1b[?1049h\x1b[?25l\x1b[2;20r\x1b[3S\x1b[s\x1b[u\x1b[?25h\x1b[?1049l",
            &mut backend,
        );
        assert_eq!(
            backend.0,
            std::vec![
                Op::Alternate(true),
                Op::Cursor(false),
                Op::Region(1, Some(19)),
                Op::Scroll(3),
                Op::Save,
                Op::Restore,
                Op::Cursor(true),
                Op::Alternate(false),
            ]
        );
    }

    #[test]
    fn oversized_or_malformed_csi_is_bounded_and_recovers() {
        let mut parser = AnsiParser::new();
        let mut backend = Backend::default();
        parser.advance(
            b"\x1b[1;2;3;4;5;6;7;8;9;10;11;12;13;14;15;16;17mZ",
            &mut backend,
        );
        assert_eq!(backend.0, std::vec![Op::Byte(b'Z')]);
        parser.advance(b"\x1b[999999999999999999999A", &mut backend);
        assert_eq!(backend.0.last(), Some(&Op::Relative(-32767, 0)));
    }

    #[test]
    fn parser_suppresses_osc_and_control_strings_then_recovers() {
        let mut parser = AnsiParser::new();
        let mut backend = Backend::default();
        parser.advance(
            b"A\x1b]0;secret title\x07B\x1bPignored\x1b\\C\x1b]2;also ignored\x1b\\D",
            &mut backend,
        );
        assert_eq!(
            backend.0,
            std::vec![
                Op::Byte(b'A'),
                Op::Byte(b'B'),
                Op::Byte(b'C'),
                Op::Byte(b'D')
            ]
        );
    }

    #[test]
    fn parser_exposes_edit_modes_and_reports_needed_by_full_screen_apps() {
        let mut parser = AnsiParser::new();
        let mut backend = Backend::default();
        parser.advance(
            b"\x1b[3@\x1b[2P\x1b[4X\x1b[2L\x1b[3M\x1b[?1h\x1b[?7l\x1b[?2004h\x1b=\x1b>\x1b[4h\x1b[5n\x1b[6n\x1b[c",
            &mut backend,
        );
        assert_eq!(
            backend.0,
            std::vec![
                Op::Edit("insert-blank", 3),
                Op::Edit("delete-characters", 2),
                Op::Edit("erase-characters", 4),
                Op::Edit("insert-lines", 2),
                Op::Edit("delete-lines", 3),
                Op::Mode("application-cursor", true),
                Op::Mode("auto-wrap", false),
                Op::Mode("bracketed-paste", true),
                Op::Mode("application-keypad", true),
                Op::Mode("application-keypad", false),
                Op::Mode("insert", true),
                Op::Report(Report::DeviceStatus),
                Op::Report(Report::CursorPosition),
                Op::Report(Report::PrimaryDeviceAttributes),
            ]
        );
    }
}
