use core::cell::UnsafeCell;
#[cfg(target_arch = "aarch64")]
use core::sync::atomic::AtomicU64;
use core::sync::atomic::{AtomicBool, Ordering};
use makos_boot_api::FramebufferInfo;
use makos_tty::{
    AnsiParser, Attributes, ByteSink, EraseDisplay, EraseLine, LineDiscipline, Report,
    TerminalBackend, Termios, WindowSize,
};

const MAX_WIDTH: u32 = 720;
const MAX_HEIGHT: u32 = 420;
const MAX_PIXELS: usize = MAX_WIDTH as usize * MAX_HEIGHT as usize;
const MAX_SURFACES: usize = 8;
const DESKTOP_SURFACES: usize = 6;
const CURSOR_WIDTH: usize = 12;
const CURSOR_HEIGHT: usize = 17;
const CURSOR_PIXELS: usize = CURSOR_WIDTH * CURSOR_HEIGHT;
const LOGIN_WIDTH: u32 = 700;
const LOGIN_HEIGHT: u32 = 440;
const LOGIN_FIELD_X: u32 = 190;
const LOGIN_FIELD_WIDTH: u32 = 450;
const LOGIN_USERNAME_Y: u32 = 116;
const LOGIN_PASSWORD_Y: u32 = 196;
const LOGIN_FIELD_HEIGHT: u32 = 44;
const MONITOR_SURFACE: usize = 0;
const TERMINAL_SURFACE: usize = 1;
const SETTINGS_SURFACE: usize = 2;
const TEXT_EDIT_SURFACE: usize = 3;
const BROWSER_SURFACE: usize = 4;
const FILES_SURFACE: usize = 5;
// Firefox can keep its UI thread busy while restoring a profile. Keep a full
// burst of ordinary typing until Gecko returns to its native event pump; the
// u8 ring indices make 256 entries (255 usable) the largest bounded queue.
const SURFACE_EVENT_QUEUE: usize = 256;
const TERMINAL_COLUMNS: usize = 58;
const TERMINAL_ROWS: usize = 22;
const TERMINAL_CELLS: usize = TERMINAL_COLUMNS * TERMINAL_ROWS;
const TERMINAL_CLIPBOARD_BYTES: usize = TERMINAL_CELLS + TERMINAL_ROWS;
const TERMINAL_CELL_WIDTH: u32 = 12;
const TERMINAL_CELL_HEIGHT: u32 = 18;
const TASKBAR_HEIGHT: u32 = 48;
const TASKBAR_APP_X: u32 = 126;
const TASKBAR_APP_GAP: u32 = 4;
const START_BUTTON_X: u32 = 6;
const START_BUTTON_WIDTH: u32 = 112;
const START_MENU_WIDTH: u32 = 286;
const START_MENU_ITEM_HEIGHT: u32 = 36;
const RESIZE_GRIP: u32 = 18;
const SETTINGS_USERNAME_BYTES: usize = 31;
const SETTINGS_PASSWORD_BYTES: usize = 64;
const SETTINGS_STATUS_READY: u8 = 0;
const SETTINGS_STATUS_CREATED: u8 = 1;
const SETTINGS_STATUS_MISMATCH: u8 = 2;
const SETTINGS_STATUS_INVALID_USERNAME: u8 = 3;
const SETTINGS_STATUS_INVALID_PASSWORD: u8 = 4;
const SETTINGS_STATUS_EXISTS: u8 = 5;
const SETTINGS_STATUS_FULL: u8 = 6;
const SETTINGS_STATUS_STORAGE: u8 = 7;
const SETTINGS_STATUS_PERMISSION: u8 = 8;

#[cfg(target_arch = "aarch64")]
const CURSOR_BACKEND: &str = "virtio-gpu-plane";
#[cfg(target_arch = "x86_64")]
const CURSOR_BACKEND: &str = "compositor-shadow";

type TerminalInput = LineDiscipline<256, 128, 16>;

#[cfg(target_arch = "aarch64")]
fn graphics_uptime_millis() -> u64 {
    crate::arch::uptime_millis()
}

#[cfg(target_arch = "x86_64")]
fn graphics_uptime_millis() -> u64 {
    crate::arch::monotonic_ticks().saturating_mul(10)
}

#[cfg(target_arch = "aarch64")]
static INPUT_BATCHING: AtomicBool = AtomicBool::new(false);
#[cfg(target_arch = "aarch64")]
static INPUT_BATCH_DIRTY: AtomicBool = AtomicBool::new(false);
#[cfg(target_arch = "aarch64")]
static INPUT_LAST_COMPOSE_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(target_arch = "aarch64")]
static TASKBAR_CLOCK_NEXT_CHECK_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(target_arch = "aarch64")]
static TASKBAR_CLOCK_MINUTE: AtomicU64 = AtomicU64::new(u64::MAX);
#[cfg(target_arch = "aarch64")]
static MONITOR_NEXT_REFRESH_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(target_arch = "aarch64")]
static MONITOR_LIVE_REPORTED: AtomicBool = AtomicBool::new(false);
#[cfg(target_arch = "aarch64")]
static DEFERRED_COMPOSE_PENDING: AtomicBool = AtomicBool::new(false);
#[cfg(target_arch = "aarch64")]
static GPU_NONOWNER_COMPOSE_DEFERRALS: AtomicU64 = AtomicU64::new(0);
#[cfg(target_arch = "aarch64")]
static GPU_OWNER_DEFERRED_COMPOSES: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
struct TerminalCell {
    byte: u8,
    attributes: Attributes,
}

impl TerminalCell {
    const BLANK: Self = Self {
        byte: b' ',
        attributes: Attributes::default_terminal(),
    };
}

struct DiscardBytes;

impl ByteSink for DiscardBytes {
    fn write(&mut self, _bytes: &[u8]) {}
}

fn current_pid() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        crate::scheduler::current_pid()
    }
    #[cfg(target_arch = "aarch64")]
    {
        crate::aarch64_process::current_pid()
    }
}

fn flush_scanout() {
    #[cfg(target_arch = "aarch64")]
    crate::aarch64_virtio_gpu::flush();
}

fn flush_scanout_rect(x: u32, y: u32, width: u32, height: u32) {
    #[cfg(target_arch = "aarch64")]
    crate::aarch64_virtio_gpu::flush_rect(x, y, width, height);
    #[cfg(target_arch = "x86_64")]
    let _ = (x, y, width, height);
}

#[derive(Clone, Copy)]
struct Surface {
    created: bool,
    owner_pid: u64,
    width: u32,
    height: u32,
    backing_width: u32,
    backing_height: u32,
    x: u32,
    y: u32,
    presented: bool,
    minimized: bool,
    /// Backing pixels changed since last accepted present. Geometry/chrome
    /// changes compose directly and therefore do not use this flag.
    dirty: bool,
}

impl Surface {
    const EMPTY: Self = Self {
        created: false,
        owner_pid: 0,
        width: 0,
        height: 0,
        backing_width: 0,
        backing_height: 0,
        x: 0,
        y: 0,
        presented: false,
        minimized: false,
        dirty: false,
    };
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SurfaceEvent {
    pub kind: u32,
    pub key: u32,
    pub modifiers: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl SurfaceEvent {
    const EMPTY: Self = Self {
        kind: 0,
        key: 0,
        modifiers: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };
}

struct State {
    framebuffer: FramebufferInfo,
    pixels: [[u32; MAX_PIXELS]; MAX_SURFACES],
    surfaces: [Surface; MAX_SURFACES],
    z_order: [usize; MAX_SURFACES],
    focused_surface: Option<usize>,
    cursor_x: u32,
    cursor_y: u32,
    cursor_buttons: u8,
    cursor_under: [u32; CURSOR_PIXELS],
    cursor_under_valid: bool,
    cursor_under_x: u32,
    cursor_under_y: u32,
    drag_surface: Option<usize>,
    drag_offset_x: i32,
    drag_offset_y: i32,
    drag_x: u32,
    drag_y: u32,
    drag_outline_valid: bool,
    resize_surface: Option<usize>,
    resize_offset_x: i32,
    resize_offset_y: i32,
    resize_width: u32,
    resize_height: u32,
    login_console: bool,
    login_phase: u8,
    console_column: u32,
    console_row: u32,
    presents: u64,
    terminal_cells: [TerminalCell; TERMINAL_CELLS],
    terminal_alternate_cells: [TerminalCell; TERMINAL_CELLS],
    terminal_parser: AnsiParser,
    terminal_input: TerminalInput,
    terminal_window_size: WindowSize,
    terminal_column: usize,
    terminal_row: usize,
    terminal_saved_column: usize,
    terminal_saved_row: usize,
    terminal_main_column: usize,
    terminal_main_row: usize,
    terminal_scroll_top: usize,
    terminal_scroll_bottom: usize,
    terminal_attributes: Attributes,
    terminal_cursor_visible: bool,
    terminal_alternate: bool,
    terminal_auto_wrap: bool,
    terminal_insert_mode: bool,
    terminal_application_cursor: bool,
    terminal_application_keypad: bool,
    terminal_bracketed_paste: bool,
    terminal_selection_anchor: usize,
    terminal_selection_end: usize,
    terminal_selecting: bool,
    start_menu_open: bool,
    settings_user_open: bool,
    settings_user_field: u8,
    settings_username: [u8; SETTINGS_USERNAME_BYTES],
    settings_username_len: u8,
    settings_password: [u8; SETTINGS_PASSWORD_BYTES],
    settings_password_len: u8,
    settings_confirmation: [u8; SETTINGS_PASSWORD_BYTES],
    settings_confirmation_len: u8,
    settings_user_status: u8,
    settings_submit_pending: bool,
    settings_submit_after_ms: u64,
    signout_pending: bool,
    pressed_button: u8,
    pressed_surface: usize,
    pressed_value: u8,
    surface_events: [[SurfaceEvent; SURFACE_EVENT_QUEUE]; MAX_SURFACES],
    surface_event_heads: [u8; MAX_SURFACES],
    surface_event_tails: [u8; MAX_SURFACES],
    surface_event_enabled: [bool; MAX_SURFACES],
}

struct LockedState {
    lock: AtomicBool,
    ready: AtomicBool,
    state: UnsafeCell<State>,
}

unsafe impl Sync for LockedState {}

static GRAPHICS: LockedState = LockedState {
    lock: AtomicBool::new(false),
    ready: AtomicBool::new(false),
    state: UnsafeCell::new(State {
        framebuffer: FramebufferInfo::EMPTY,
        pixels: [[0; MAX_PIXELS]; MAX_SURFACES],
        surfaces: [Surface::EMPTY; MAX_SURFACES],
        z_order: [0, 1, 2, 3, 4, 5, 6, 7],
        focused_surface: None,
        cursor_x: 640,
        cursor_y: 400,
        cursor_buttons: 0,
        cursor_under: [0; CURSOR_PIXELS],
        cursor_under_valid: false,
        cursor_under_x: 0,
        cursor_under_y: 0,
        drag_surface: None,
        drag_offset_x: 0,
        drag_offset_y: 0,
        drag_x: 0,
        drag_y: 0,
        drag_outline_valid: false,
        resize_surface: None,
        resize_offset_x: 0,
        resize_offset_y: 0,
        resize_width: 0,
        resize_height: 0,
        login_console: false,
        login_phase: 0,
        console_column: 0,
        console_row: 0,
        presents: 0,
        terminal_cells: [TerminalCell::BLANK; TERMINAL_CELLS],
        terminal_alternate_cells: [TerminalCell::BLANK; TERMINAL_CELLS],
        terminal_parser: AnsiParser::new(),
        terminal_input: TerminalInput::new(Termios::raw()),
        terminal_window_size: WindowSize::new(
            TERMINAL_ROWS as u16,
            TERMINAL_COLUMNS as u16,
            (TERMINAL_COLUMNS as u32 * TERMINAL_CELL_WIDTH) as u16,
            (TERMINAL_ROWS as u32 * TERMINAL_CELL_HEIGHT) as u16,
        ),
        terminal_column: 0,
        terminal_row: 0,
        terminal_saved_column: 0,
        terminal_saved_row: 0,
        terminal_main_column: 0,
        terminal_main_row: 0,
        terminal_scroll_top: 0,
        terminal_scroll_bottom: TERMINAL_ROWS - 1,
        terminal_attributes: Attributes::default_terminal(),
        terminal_cursor_visible: true,
        terminal_alternate: false,
        terminal_auto_wrap: true,
        terminal_insert_mode: false,
        terminal_application_cursor: false,
        terminal_application_keypad: false,
        terminal_bracketed_paste: false,
        terminal_selection_anchor: 0,
        terminal_selection_end: 0,
        terminal_selecting: false,
        start_menu_open: false,
        settings_user_open: false,
        settings_user_field: 0,
        settings_username: [0; SETTINGS_USERNAME_BYTES],
        settings_username_len: 0,
        settings_password: [0; SETTINGS_PASSWORD_BYTES],
        settings_password_len: 0,
        settings_confirmation: [0; SETTINGS_PASSWORD_BYTES],
        settings_confirmation_len: 0,
        settings_user_status: SETTINGS_STATUS_READY,
        settings_submit_pending: false,
        settings_submit_after_ms: 0,
        signout_pending: false,
        pressed_button: 0,
        pressed_surface: 0,
        pressed_value: 0,
        surface_events: [[SurfaceEvent::EMPTY; SURFACE_EVENT_QUEUE]; MAX_SURFACES],
        surface_event_heads: [0; MAX_SURFACES],
        surface_event_tails: [0; MAX_SURFACES],
        surface_event_enabled: [false; MAX_SURFACES],
    }),
};

pub fn init(framebuffer: FramebufferInfo) {
    if !crate::framebuffer::install_scene_shadow(framebuffer) {
        crate::fatal("graphics framebuffer exceeds cursor-safe shadow bounds");
    }
    with_lock(|state| state.framebuffer = framebuffer);
    GRAPHICS.ready.store(true, Ordering::Release);
    crate::serial_println!(
        "graphics service=online scanout={}x{} surface_max={}x{}",
        framebuffer.width,
        framebuffer.height,
        MAX_WIDTH,
        MAX_HEIGHT
    );
}

pub fn show_login() {
    with_lock(|state| {
        let Some(mut screen) = crate::framebuffer::Screen::new(state.framebuffer) else {
            return;
        };
        // 95.css is an HTML stylesheet. These primitives port its visual grammar
        // directly to MakOS framebuffer widgets: system colors, hard bevels,
        // sunken inputs, title bars, buttons, and dotted keyboard focus.
        screen.clear(crate::framebuffer::Color::new(0, 128, 128));
        let panel_x = state.framebuffer.width.saturating_sub(LOGIN_WIDTH) / 2;
        let panel_y = state.framebuffer.height.saturating_sub(LOGIN_HEIGHT) / 2;
        draw_raised_panel(&mut screen, panel_x, panel_y, LOGIN_WIDTH, LOGIN_HEIGHT);
        screen.fill_rect(
            panel_x + 4,
            panel_y + 4,
            LOGIN_WIDTH - 8,
            28,
            crate::framebuffer::Color::new(0, 0, 128),
        );
        screen.draw_text(
            panel_x + 12,
            panel_y + 11,
            2,
            "MAKOS LOGON",
            crate::framebuffer::Color::new(255, 255, 255),
        );
        draw_raised_panel(&mut screen, panel_x + LOGIN_WIDTH - 30, panel_y + 7, 20, 20);
        screen.draw_text(
            panel_x + LOGIN_WIDTH - 26,
            panel_y + 10,
            2,
            "X",
            crate::framebuffer::Color::new(0, 0, 0),
        );
        screen.draw_text(
            panel_x + 40,
            panel_y + 55,
            2,
            "ENTER YOUR USER NAME AND PASSWORD.",
            crate::framebuffer::Color::new(0, 0, 0),
        );
        screen.draw_text(
            panel_x + 40,
            panel_y + 79,
            2,
            "ACTIVE FIELD SHOWS FOCUS. TAB OR ENTER MOVES NEXT.",
            crate::framebuffer::Color::new(0, 0, 0),
        );
        screen.draw_text(
            panel_x + 40,
            panel_y + 130,
            2,
            "USER NAME:",
            crate::framebuffer::Color::new(0, 0, 0),
        );
        draw_sunken_field(
            &mut screen,
            panel_x + LOGIN_FIELD_X,
            panel_y + LOGIN_USERNAME_Y,
            LOGIN_FIELD_WIDTH,
            LOGIN_FIELD_HEIGHT,
        );
        screen.draw_text(
            panel_x + 40,
            panel_y + 210,
            2,
            "PASSWORD:",
            crate::framebuffer::Color::new(0, 0, 0),
        );
        draw_sunken_field(
            &mut screen,
            panel_x + LOGIN_FIELD_X,
            panel_y + LOGIN_PASSWORD_Y,
            LOGIN_FIELD_WIDTH,
            LOGIN_FIELD_HEIGHT,
        );
        draw_raised_panel(&mut screen, panel_x + 410, panel_y + 275, 230, 46);
        screen.draw_text(
            panel_x + 444,
            panel_y + 291,
            2,
            "LOG ON  [ENTER]",
            crate::framebuffer::Color::new(0, 0, 0),
        );
        draw_sunken_panel(&mut screen, panel_x + 40, panel_y + 347, 600, 56);
        screen.draw_text(
            panel_x + 55,
            panel_y + 366,
            2,
            "CLICK QEMU WINDOW, THEN TYPE. START WITH USER NAME.",
            crate::framebuffer::Color::new(0, 0, 0),
        );
        state.login_console = true;
        state.pressed_button = 0;
        state.pressed_surface = 0;
        state.pressed_value = 0;
        state.settings_submit_pending = false;
        state.settings_submit_after_ms = 0;
        state.signout_pending = false;
        state.login_phase = 0;
        state.console_column = 0;
        state.console_row = 0;
        draw_login_focus(state, &mut screen);
        state.cursor_under_valid = false;
        capture_cursor_under(state, &screen);
        draw_cursor(&mut screen, state.cursor_x, state.cursor_y);
        crate::serial_println!(
            "MAKOS_LOGIN_UI_OK framebuffer={}x{} prompt=visible console=live cursor={} theme=95css-native",
            state.framebuffer.width,
            state.framebuffer.height,
            CURSOR_BACKEND,
        );
    });
    flush_scanout();
}

pub fn hide_login() {
    with_lock(|state| state.login_console = false);
}

pub fn console_write(bytes: &[u8]) {
    #[cfg(target_arch = "aarch64")]
    if bytes == b"MAKOS_LOGIN_RETRY\n" {
        show_login();
        return;
    }
    let bytes = if bytes.strip_prefix(b"MAKOS_LOGIN_READY\n").is_some() {
        b"" as &[u8]
    } else if bytes.starts_with(b"MAKOS_") || bytes.starts_with(b"MakOS userspace") {
        return;
    } else {
        bytes
    };
    with_lock(|state| {
        if !state.login_console {
            terminal_write_locked(state, bytes);
            return;
        }
        let Some(mut screen) = crate::framebuffer::Screen::new(state.framebuffer) else {
            return;
        };
        restore_cursor_under(state, &mut screen, state.cursor_x, state.cursor_y);
        let panel_x = state.framebuffer.width.saturating_sub(LOGIN_WIDTH) / 2;
        let panel_y = state.framebuffer.height.saturating_sub(LOGIN_HEIGHT) / 2;
        if bytes == b"password: " {
            erase_login_caret(state, &mut screen);
            state.login_phase = 1;
            state.console_column = 0;
            draw_login_focus(state, &mut screen);
            state.cursor_under_valid = false;
            capture_cursor_under(state, &screen);
            draw_cursor(&mut screen, state.cursor_x, state.cursor_y);
            return;
        }
        let origin_x = panel_x + LOGIN_FIELD_X + 12;
        let origin_y = panel_y
            + if state.login_phase == 0 {
                LOGIN_USERNAME_Y + 14
            } else {
                LOGIN_PASSWORD_Y + 14
            };
        let background = crate::framebuffer::Color::new(255, 255, 255);
        let foreground = crate::framebuffer::Color::new(0, 0, 0);
        for byte in bytes.iter().copied() {
            match byte {
                b'\r' => {}
                b'\n' => {
                    // Next explicit prompt advances field focus.
                }
                8 => {
                    // Caret occupies otherwise-empty cell after current text.
                    // Clear it before moving left, then erase deleted glyph.
                    erase_login_caret(state, &mut screen);
                    state.console_column = state.console_column.saturating_sub(1);
                    screen.fill_rect(
                        origin_x + state.console_column * 12,
                        origin_y.saturating_sub(2),
                        12,
                        18,
                        background,
                    );
                    draw_login_caret(state, &mut screen);
                    crate::serial_println!(
                        "MAKOS_LOGIN_BACKSPACE_OK field={} column={} stale_caret=0",
                        if state.login_phase == 0 {
                            "username"
                        } else {
                            "password"
                        },
                        state.console_column,
                    );
                }
                0x20..=0x7e => {
                    if state.console_column >= 34 {
                        continue;
                    }
                    let x = origin_x + state.console_column * 12;
                    // Remove full 18-pixel caret before painting glyph. Previous
                    // code cleared only glyph height, leaving caret fragments.
                    screen.fill_rect(x, origin_y.saturating_sub(2), 12, 18, background);
                    let character = [byte];
                    if let Ok(text) = core::str::from_utf8(&character) {
                        screen.draw_text(x, origin_y, 2, text, foreground);
                    }
                    state.console_column += 1;
                    draw_login_caret(state, &mut screen);
                }
                _ => {}
            }
        }
        state.cursor_under_valid = false;
        capture_cursor_under(state, &screen);
        draw_cursor(&mut screen, state.cursor_x, state.cursor_y);
    });
    flush_scanout();
}

pub fn terminal_write(bytes: &[u8]) {
    with_lock(|state| terminal_write_locked(state, bytes));
    flush_scanout();
}

pub fn terminal_input_byte(byte: u8) {
    static REPORTED: AtomicBool = AtomicBool::new(false);
    with_lock(|state| {
        let mut echo = DiscardBytes;
        let _ = state.terminal_input.receive(byte, &mut echo);
    });
    if !REPORTED.swap(true, Ordering::AcqRel) {
        crate::serial_println!(
            "MAKOS_TERMINAL_INPUT_OK discipline=raw bounded=256 lowercase=1 punctuation=1"
        );
    }
}

pub fn terminal_read_byte() -> Option<u8> {
    with_lock(|state| {
        let mut byte = [0u8; 1];
        match state.terminal_input.read(&mut byte) {
            Ok(1) => Some(byte[0]),
            _ => None,
        }
    })
}

pub fn terminal_window_size() -> WindowSize {
    with_lock(|state| state.terminal_window_size)
}

fn sync_terminal_window_size() {
    #[cfg(target_arch = "aarch64")]
    {
        let size = terminal_window_size();
        crate::aarch64_tty::set_window_size_from_compositor(crate::aarch64_tty::UserWindowSize {
            rows: size.rows,
            columns: size.columns,
            pixel_width: size.pixel_width,
            pixel_height: size.pixel_height,
        });
    }
}

pub fn terminal_clear() {
    with_lock(|state| {
        state.terminal_cells.fill(TerminalCell::BLANK);
        state.terminal_column = 0;
        state.terminal_row = 0;
        state.terminal_attributes = Attributes::default_terminal();
        state.terminal_parser = AnsiParser::new();
        redraw_terminal_direct(state, true);
        crate::serial_println!(
            "MAKOS_TERMINAL_CLEAR_OK retained_grid=1 ansi=vt100 line_discipline=raw"
        );
    });
    flush_scanout();
}

fn terminal_write_locked(state: &mut State, bytes: &[u8]) {
    state.terminal_selecting = false;
    state.terminal_selection_anchor = 0;
    state.terminal_selection_end = 0;
    if !state.surfaces[TERMINAL_SURFACE].created {
        return;
    }
    let mut parser = core::mem::take(&mut state.terminal_parser);
    parser.advance(bytes, &mut GraphicsTerminal { state });
    state.terminal_parser = parser;
    redraw_terminal_direct(state, true);
}

struct GraphicsTerminal<'a> {
    state: &'a mut State,
}

impl TerminalBackend for GraphicsTerminal<'_> {
    fn put_byte(&mut self, byte: u8) {
        let (columns, _) = terminal_dimensions(self.state);
        if self.state.terminal_insert_mode {
            let row_start = self.state.terminal_row * TERMINAL_COLUMNS;
            let column = self.state.terminal_column.min(columns - 1);
            for target in (column + 1..columns).rev() {
                self.state.terminal_cells[row_start + target] =
                    self.state.terminal_cells[row_start + target - 1];
            }
        }
        let index = self.state.terminal_row * TERMINAL_COLUMNS + self.state.terminal_column;
        self.state.terminal_cells[index] = TerminalCell {
            byte,
            attributes: self.state.terminal_attributes,
        };
        if self.state.terminal_column + 1 < columns {
            self.state.terminal_column += 1;
        } else if self.state.terminal_auto_wrap {
            self.state.terminal_column = 0;
            terminal_line_feed(self.state);
        }
    }

    fn carriage_return(&mut self) {
        self.state.terminal_column = 0;
    }

    fn line_feed(&mut self) {
        // Existing EL0 shell writes LF without POSIX OPOST. Retain newline
        // behavior while the new fd path emits CRLF explicitly.
        self.state.terminal_column = 0;
        terminal_line_feed(self.state);
    }

    fn backspace(&mut self) {
        if self.state.terminal_column != 0 {
            self.state.terminal_column -= 1;
            let index = self.state.terminal_row * TERMINAL_COLUMNS + self.state.terminal_column;
            self.state.terminal_cells[index] = TerminalCell::BLANK;
        }
    }

    fn tab(&mut self) {
        let (columns, _) = terminal_dimensions(self.state);
        self.state.terminal_column = ((self.state.terminal_column / 8 + 1) * 8).min(columns - 1);
    }

    fn move_cursor(&mut self, row: u16, column: u16) {
        let (columns, rows) = terminal_dimensions(self.state);
        self.state.terminal_row = usize::from(row).min(rows - 1);
        self.state.terminal_column = usize::from(column).min(columns - 1);
    }

    fn set_cursor_column(&mut self, column: u16) {
        let (columns, _) = terminal_dimensions(self.state);
        self.state.terminal_column = usize::from(column).min(columns - 1);
    }

    fn move_relative(&mut self, row_delta: i16, column_delta: i16) {
        let (columns, rows) = terminal_dimensions(self.state);
        self.state.terminal_row = add_signed(self.state.terminal_row, row_delta, rows - 1);
        self.state.terminal_column =
            add_signed(self.state.terminal_column, column_delta, columns - 1);
    }

    fn erase_display(&mut self, mode: EraseDisplay) {
        let (columns, rows) = terminal_dimensions(self.state);
        let cursor = self.state.terminal_row * TERMINAL_COLUMNS + self.state.terminal_column;
        match mode {
            EraseDisplay::CursorToEnd => {
                self.state.terminal_cells
                    [cursor..self.state.terminal_row * TERMINAL_COLUMNS + columns]
                    .fill(TerminalCell::BLANK);
                for row in self.state.terminal_row + 1..rows {
                    terminal_blank_row(self.state, row, columns);
                }
            }
            EraseDisplay::StartToCursor => {
                for row in 0..self.state.terminal_row {
                    terminal_blank_row(self.state, row, columns);
                }
                let start = self.state.terminal_row * TERMINAL_COLUMNS;
                self.state.terminal_cells[start..=cursor].fill(TerminalCell::BLANK);
            }
            EraseDisplay::All | EraseDisplay::Scrollback => {
                for row in 0..rows {
                    terminal_blank_row(self.state, row, columns);
                }
            }
        }
    }

    fn erase_line(&mut self, mode: EraseLine) {
        let (columns, _) = terminal_dimensions(self.state);
        let start = self.state.terminal_row * TERMINAL_COLUMNS;
        match mode {
            EraseLine::CursorToEnd => self.state.terminal_cells
                [start + self.state.terminal_column..start + columns]
                .fill(TerminalCell::BLANK),
            EraseLine::StartToCursor => self.state.terminal_cells
                [start..=start + self.state.terminal_column]
                .fill(TerminalCell::BLANK),
            EraseLine::All => {
                self.state.terminal_cells[start..start + columns].fill(TerminalCell::BLANK)
            }
        }
    }

    fn set_attributes(&mut self, attributes: Attributes) {
        self.state.terminal_attributes = attributes;
    }

    fn set_cursor_visible(&mut self, visible: bool) {
        self.state.terminal_cursor_visible = visible;
    }

    fn use_alternate_screen(&mut self, enabled: bool) {
        if enabled == self.state.terminal_alternate {
            return;
        }
        if enabled {
            self.state.terminal_main_column = self.state.terminal_column;
            self.state.terminal_main_row = self.state.terminal_row;
        }
        core::mem::swap(
            &mut self.state.terminal_cells,
            &mut self.state.terminal_alternate_cells,
        );
        self.state.terminal_alternate = enabled;
        if enabled {
            self.state.terminal_cells.fill(TerminalCell::BLANK);
            self.state.terminal_column = 0;
            self.state.terminal_row = 0;
        } else {
            self.state.terminal_column = self.state.terminal_main_column;
            self.state.terminal_row = self.state.terminal_main_row;
        }
    }

    fn save_cursor(&mut self) {
        self.state.terminal_saved_column = self.state.terminal_column;
        self.state.terminal_saved_row = self.state.terminal_row;
    }

    fn restore_cursor(&mut self) {
        let (columns, rows) = terminal_dimensions(self.state);
        self.state.terminal_column = self.state.terminal_saved_column.min(columns - 1);
        self.state.terminal_row = self.state.terminal_saved_row.min(rows - 1);
    }

    fn set_scroll_region(&mut self, top: u16, bottom: Option<u16>) {
        let (_, rows) = terminal_dimensions(self.state);
        let top = usize::from(top).min(rows - 1);
        let bottom = bottom.map_or(rows - 1, usize::from).min(rows - 1);
        if top < bottom {
            self.state.terminal_scroll_top = top;
            self.state.terminal_scroll_bottom = bottom;
            self.state.terminal_column = 0;
            self.state.terminal_row = 0;
        }
    }

    fn scroll(&mut self, lines: i16) {
        terminal_scroll(self.state, lines);
    }

    fn insert_blank(&mut self, count: u16) {
        let (columns, _) = terminal_dimensions(self.state);
        let row = self.state.terminal_row * TERMINAL_COLUMNS;
        let column = self.state.terminal_column;
        let count = usize::from(count).min(columns - column);
        for target in (column + count..columns).rev() {
            self.state.terminal_cells[row + target] =
                self.state.terminal_cells[row + target - count];
        }
        self.state.terminal_cells[row + column..row + column + count].fill(TerminalCell::BLANK);
    }

    fn delete_characters(&mut self, count: u16) {
        let (columns, _) = terminal_dimensions(self.state);
        let row = self.state.terminal_row * TERMINAL_COLUMNS;
        let column = self.state.terminal_column;
        let count = usize::from(count).min(columns - column);
        for target in column..columns - count {
            self.state.terminal_cells[row + target] =
                self.state.terminal_cells[row + target + count];
        }
        self.state.terminal_cells[row + columns - count..row + columns].fill(TerminalCell::BLANK);
    }

    fn erase_characters(&mut self, count: u16) {
        let (columns, _) = terminal_dimensions(self.state);
        let row = self.state.terminal_row * TERMINAL_COLUMNS;
        let end = (self.state.terminal_column + usize::from(count)).min(columns);
        self.state.terminal_cells[row + self.state.terminal_column..row + end]
            .fill(TerminalCell::BLANK);
    }

    fn insert_lines(&mut self, count: u16) {
        terminal_shift_lines(self.state, usize::from(count), true);
    }

    fn delete_lines(&mut self, count: u16) {
        terminal_shift_lines(self.state, usize::from(count), false);
    }

    fn set_application_cursor_keys(&mut self, enabled: bool) {
        self.state.terminal_application_cursor = enabled;
    }

    fn set_application_keypad(&mut self, enabled: bool) {
        self.state.terminal_application_keypad = enabled;
    }

    fn set_auto_wrap(&mut self, enabled: bool) {
        self.state.terminal_auto_wrap = enabled;
    }

    fn set_bracketed_paste(&mut self, enabled: bool) {
        self.state.terminal_bracketed_paste = enabled;
    }

    fn set_insert_mode(&mut self, enabled: bool) {
        self.state.terminal_insert_mode = enabled;
    }

    fn request_report(&mut self, report: Report) {
        let mut bytes = [0u8; 16];
        let length = match report {
            Report::DeviceStatus => copy_bytes(&mut bytes, b"\x1b[0n"),
            Report::CursorPosition => terminal_cursor_report(
                &mut bytes,
                self.state.terminal_row + 1,
                self.state.terminal_column + 1,
            ),
            Report::PrimaryDeviceAttributes => copy_bytes(&mut bytes, b"\x1b[?1;0c"),
        };
        let mut echo = DiscardBytes;
        for byte in bytes[..length].iter().copied() {
            let _ = self.state.terminal_input.receive(byte, &mut echo);
        }
    }

    fn reset(&mut self) {
        self.state.terminal_cells.fill(TerminalCell::BLANK);
        self.state.terminal_column = 0;
        self.state.terminal_row = 0;
        self.state.terminal_scroll_top = 0;
        self.state.terminal_scroll_bottom = terminal_dimensions(self.state).1 - 1;
        self.state.terminal_attributes = Attributes::default_terminal();
        self.state.terminal_cursor_visible = true;
        self.state.terminal_auto_wrap = true;
        self.state.terminal_insert_mode = false;
        self.state.terminal_application_cursor = false;
        self.state.terminal_application_keypad = false;
        self.state.terminal_bracketed_paste = false;
    }
}

fn add_signed(value: usize, delta: i16, maximum: usize) -> usize {
    if delta < 0 {
        value.saturating_sub(usize::from(delta.unsigned_abs()))
    } else {
        value.saturating_add(delta as usize).min(maximum)
    }
}

fn terminal_dimensions(state: &State) -> (usize, usize) {
    let surface = state.surfaces[TERMINAL_SURFACE];
    let columns = (surface.width.saturating_sub(20) / TERMINAL_CELL_WIDTH)
        .clamp(1, TERMINAL_COLUMNS as u32) as usize;
    let rows = (surface.height.saturating_sub(20) / TERMINAL_CELL_HEIGHT)
        .clamp(1, TERMINAL_ROWS as u32) as usize;
    (columns, rows)
}

fn terminal_has_selection(state: &State) -> bool {
    state.terminal_selection_anchor != state.terminal_selection_end
}

fn terminal_cell_selected(state: &State, index: usize) -> bool {
    if !terminal_has_selection(state) {
        return false;
    }
    let start = state
        .terminal_selection_anchor
        .min(state.terminal_selection_end);
    let end = state
        .terminal_selection_anchor
        .max(state.terminal_selection_end);
    (start..=end).contains(&index)
}

fn terminal_cell_at(state: &State, x: u32, y: u32) -> Option<usize> {
    let surface = state.surfaces[TERMINAL_SURFACE];
    let local_x = x.checked_sub(surface.x)?.checked_sub(10)?;
    let local_y = y.checked_sub(surface.y)?.checked_sub(10)?;
    let column = usize::try_from(local_x / TERMINAL_CELL_WIDTH).ok()?;
    let row = usize::try_from(local_y / TERMINAL_CELL_HEIGHT).ok()?;
    let (columns, rows) = terminal_dimensions(state);
    (column < columns && row < rows).then_some(row * TERMINAL_COLUMNS + column)
}

fn copy_terminal_selection(state: &State, output: &mut [u8]) -> usize {
    if !terminal_has_selection(state) {
        return 0;
    }
    let (columns, rows) = terminal_dimensions(state);
    let first = state
        .terminal_selection_anchor
        .min(state.terminal_selection_end);
    let last = state
        .terminal_selection_anchor
        .max(state.terminal_selection_end);
    let first_row = (first / TERMINAL_COLUMNS).min(rows - 1);
    let last_row = (last / TERMINAL_COLUMNS).min(rows - 1);
    let mut used = 0usize;
    for row in first_row..=last_row {
        let start_column = if row == first_row {
            (first % TERMINAL_COLUMNS).min(columns - 1)
        } else {
            0
        };
        let end_column = if row == last_row {
            (last % TERMINAL_COLUMNS).min(columns - 1)
        } else {
            columns - 1
        };
        for column in start_column..=end_column {
            if used == output.len() {
                return used;
            }
            output[used] = state.terminal_cells[row * TERMINAL_COLUMNS + column].byte;
            used += 1;
        }
        if row != last_row && used != output.len() {
            output[used] = b'\n';
            used += 1;
        }
    }
    used
}

#[cfg(target_arch = "aarch64")]
fn paste_terminal_clipboard() {
    // Leave room for bracketed-paste sentinels inside the 256-byte raw queue.
    let mut clipboard = [0u8; 244];
    let Ok(count) = crate::aarch64_clipboard::read_prefix(&mut clipboard) else {
        return;
    };
    let (accepted, bracketed) = with_lock(|state| {
        if state.focused_surface != Some(TERMINAL_SURFACE) {
            return (0usize, false);
        }
        let bracketed = state.terminal_bracketed_paste;
        let mut accepted = 0usize;
        let mut echo = DiscardBytes;
        if bracketed {
            for byte in b"\x1b[200~" {
                if matches!(
                    state.terminal_input.receive(*byte, &mut echo),
                    makos_tty::ReceiveResult::Full
                ) {
                    return (accepted, bracketed);
                }
            }
        }
        for byte in &clipboard[..count] {
            if matches!(
                state.terminal_input.receive(*byte, &mut echo),
                makos_tty::ReceiveResult::Full
            ) {
                return (accepted, bracketed);
            }
            accepted += 1;
        }
        if bracketed {
            for byte in b"\x1b[201~" {
                if matches!(
                    state.terminal_input.receive(*byte, &mut echo),
                    makos_tty::ReceiveResult::Full
                ) {
                    break;
                }
            }
        }
        (accepted, bracketed)
    });
    crate::serial_println!(
        "MAKOS_TERMINAL_CLIPBOARD_OK action=paste bytes={} bracketed={}",
        accepted,
        u8::from(bracketed),
    );
}

#[cfg(target_arch = "x86_64")]
fn paste_terminal_clipboard() {}

fn update_terminal_window_size(state: &mut State) {
    let (columns, rows) = terminal_dimensions(state);
    let surface = state.surfaces[TERMINAL_SURFACE];
    state.terminal_window_size = WindowSize::new(
        rows as u16,
        columns as u16,
        surface.width.saturating_sub(20).min(u16::MAX as u32) as u16,
        surface.height.saturating_sub(20).min(u16::MAX as u32) as u16,
    );
    state.terminal_column = state.terminal_column.min(columns - 1);
    state.terminal_row = state.terminal_row.min(rows - 1);
    state.terminal_scroll_top = state.terminal_scroll_top.min(rows - 1);
    state.terminal_scroll_bottom = state.terminal_scroll_bottom.min(rows - 1);
    if state.terminal_scroll_top >= state.terminal_scroll_bottom {
        state.terminal_scroll_top = 0;
        state.terminal_scroll_bottom = rows - 1;
    }
}

fn terminal_line_feed(state: &mut State) {
    let (_, rows) = terminal_dimensions(state);
    if state.terminal_row == state.terminal_scroll_bottom {
        terminal_scroll(state, 1);
    } else {
        state.terminal_row = (state.terminal_row + 1).min(rows - 1);
    }
}

fn terminal_scroll(state: &mut State, lines: i16) {
    let (columns, _) = terminal_dimensions(state);
    let top = state.terminal_scroll_top;
    let bottom = state.terminal_scroll_bottom;
    let count = usize::from(lines.unsigned_abs()).min(bottom - top + 1);
    if lines > 0 {
        if count < bottom - top + 1 {
            for row in top..=bottom - count {
                terminal_copy_row(state, row + count, row, columns);
            }
        }
        for row in bottom + 1 - count..=bottom {
            terminal_blank_row(state, row, columns);
        }
    } else if lines < 0 {
        if count < bottom - top + 1 {
            for row in (top + count..=bottom).rev() {
                terminal_copy_row(state, row - count, row, columns);
            }
        }
        for row in top..top + count {
            terminal_blank_row(state, row, columns);
        }
    }
}

fn terminal_shift_lines(state: &mut State, requested: usize, insert: bool) {
    let (columns, _) = terminal_dimensions(state);
    let top = state.terminal_row.max(state.terminal_scroll_top);
    let bottom = state.terminal_scroll_bottom;
    if top > bottom {
        return;
    }
    let count = requested.min(bottom - top + 1);
    if insert {
        for row in (top + count..=bottom).rev() {
            terminal_copy_row(state, row - count, row, columns);
        }
        for row in top..top + count {
            terminal_blank_row(state, row, columns);
        }
    } else {
        if count < bottom - top + 1 {
            for row in top..=bottom - count {
                terminal_copy_row(state, row + count, row, columns);
            }
        }
        for row in bottom + 1 - count..=bottom {
            terminal_blank_row(state, row, columns);
        }
    }
}

fn terminal_copy_row(state: &mut State, source: usize, destination: usize, columns: usize) {
    let source = source * TERMINAL_COLUMNS;
    let destination = destination * TERMINAL_COLUMNS;
    for column in 0..columns {
        state.terminal_cells[destination + column] = state.terminal_cells[source + column];
    }
}

fn terminal_blank_row(state: &mut State, row: usize, columns: usize) {
    let start = row * TERMINAL_COLUMNS;
    state.terminal_cells[start..start + columns].fill(TerminalCell::BLANK);
}

fn copy_bytes(output: &mut [u8], value: &[u8]) -> usize {
    let length = value.len().min(output.len());
    output[..length].copy_from_slice(&value[..length]);
    length
}

fn terminal_cursor_report(output: &mut [u8; 16], row: usize, column: usize) -> usize {
    let mut used = copy_bytes(output, b"\x1b[");
    used += append_decimal(&mut output[used..], row);
    output[used] = b';';
    used += 1;
    used += append_decimal(&mut output[used..], column);
    output[used] = b'R';
    used + 1
}

fn append_decimal(output: &mut [u8], value: usize) -> usize {
    append_u64(output, value as u64)
}

fn append_u64(output: &mut [u8], mut value: u64) -> usize {
    let mut digits = [0u8; 20];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    copy_bytes(output, &digits[start..])
}

fn terminal_is_topmost(state: &State) -> bool {
    let surface = state.surfaces[TERMINAL_SURFACE];
    surface.created
        && surface.presented
        && !surface.minimized
        && state.z_order.iter().rev().copied().find(|index| {
            let candidate = state.surfaces[*index];
            candidate.created && candidate.presented && !candidate.minimized
        }) == Some(TERMINAL_SURFACE)
}

fn redraw_terminal_direct(state: &mut State, clear: bool) {
    if !terminal_is_topmost(state) {
        return;
    }
    let Some(mut screen) = crate::framebuffer::Screen::new(state.framebuffer) else {
        return;
    };
    restore_cursor_under(state, &mut screen, state.cursor_x, state.cursor_y);
    if clear {
        let terminal = state.surfaces[TERMINAL_SURFACE];
        screen.fill_rect(
            terminal.x,
            terminal.y,
            terminal.width,
            terminal.height,
            crate::framebuffer::Color::new(0, 0, 0),
        );
    }
    draw_terminal_contents(state, &mut screen);
    state.cursor_under_valid = false;
    capture_cursor_under(state, &screen);
    draw_cursor(&mut screen, state.cursor_x, state.cursor_y);
}

fn draw_terminal_contents(state: &State, screen: &mut crate::framebuffer::Screen) {
    let terminal = state.surfaces[TERMINAL_SURFACE];
    let visible_columns = (terminal.width.saturating_sub(20) / TERMINAL_CELL_WIDTH)
        .min(TERMINAL_COLUMNS as u32) as usize;
    let visible_rows = (terminal.height.saturating_sub(20) / TERMINAL_CELL_HEIGHT)
        .min(TERMINAL_ROWS as u32) as usize;
    for row in 0..visible_rows {
        for column in 0..visible_columns {
            let cell = state.terminal_cells[row * TERMINAL_COLUMNS + column];
            draw_terminal_cell(state, screen, column, row, cell);
        }
    }
    if state.focused_surface == Some(TERMINAL_SURFACE) && state.terminal_cursor_visible {
        draw_terminal_caret(state, screen);
    }
}

fn draw_terminal_cell(
    state: &State,
    screen: &mut crate::framebuffer::Screen,
    column: usize,
    row: usize,
    cell: TerminalCell,
) {
    let terminal = state.surfaces[TERMINAL_SURFACE];
    if 10 + (column as u32 + 1) * TERMINAL_CELL_WIDTH > terminal.width
        || 10 + (row as u32 + 1) * TERMINAL_CELL_HEIGHT > terminal.height
    {
        return;
    }
    let x = terminal.x + 10 + column as u32 * TERMINAL_CELL_WIDTH;
    let y = terminal.y + 10 + row as u32 * TERMINAL_CELL_HEIGHT;
    let (mut foreground, mut background) = terminal_cell_colors(cell.attributes);
    let index = row * TERMINAL_COLUMNS + column;
    if terminal_cell_selected(state, index) {
        foreground = crate::framebuffer::Color::new(255, 255, 255);
        background = crate::framebuffer::Color::new(0, 0, 128);
    }
    screen.fill_rect(x, y, TERMINAL_CELL_WIDTH, TERMINAL_CELL_HEIGHT, background);
    if cell.byte != b' ' {
        let character = [cell.byte];
        if let Ok(text) = core::str::from_utf8(&character) {
            screen.draw_text(x, y, 2, text, foreground);
        }
    }
    if cell.attributes.underline {
        screen.fill_rect(x, y + 15, 10, 1, foreground);
    }
}

fn terminal_cell_colors(
    attributes: Attributes,
) -> (crate::framebuffer::Color, crate::framebuffer::Color) {
    let foreground_index = attributes.foreground.map(|index| {
        if attributes.bold && index < 8 {
            index + 8
        } else {
            index
        }
    });
    let mut foreground = terminal_palette(foreground_index, true);
    let mut background = terminal_palette(attributes.background, false);
    if attributes.dim {
        foreground = terminal_palette(Some(8), true);
    }
    if attributes.reverse {
        core::mem::swap(&mut foreground, &mut background);
    }
    (foreground, background)
}

fn terminal_palette(index: Option<u8>, foreground: bool) -> crate::framebuffer::Color {
    let (red, green, blue) = match index {
        None if foreground => (192, 192, 192),
        None => (0, 0, 0),
        Some(0) => (0, 0, 0),
        Some(1) => (170, 0, 0),
        Some(2) => (0, 170, 0),
        Some(3) => (170, 85, 0),
        Some(4) => (0, 0, 170),
        Some(5) => (170, 0, 170),
        Some(6) => (0, 170, 170),
        Some(7) => (170, 170, 170),
        Some(8) => (85, 85, 85),
        Some(9) => (255, 85, 85),
        Some(10) => (85, 255, 85),
        Some(11) => (255, 255, 85),
        Some(12) => (85, 85, 255),
        Some(13) => (255, 85, 255),
        Some(14) => (85, 255, 255),
        _ => (255, 255, 255),
    };
    crate::framebuffer::Color::new(red, green, blue)
}

fn draw_terminal_caret(state: &State, screen: &mut crate::framebuffer::Screen) {
    let terminal = state.surfaces[TERMINAL_SURFACE];
    if 10 + (state.terminal_column as u32 + 1) * TERMINAL_CELL_WIDTH > terminal.width
        || 10 + (state.terminal_row as u32 + 1) * TERMINAL_CELL_HEIGHT > terminal.height
    {
        return;
    }
    let x = terminal.x + 10 + state.terminal_column as u32 * TERMINAL_CELL_WIDTH;
    let y = terminal.y + 10 + state.terminal_row as u32 * TERMINAL_CELL_HEIGHT;
    screen.fill_rect(
        x,
        y + 15,
        10,
        2,
        crate::framebuffer::Color::new(192, 192, 192),
    );
}

fn draw_login_focus(state: &State, screen: &mut crate::framebuffer::Screen) {
    let panel_x = state.framebuffer.width.saturating_sub(LOGIN_WIDTH) / 2;
    let panel_y = state.framebuffer.height.saturating_sub(LOGIN_HEIGHT) / 2;
    for phase in 0..2u8 {
        let y = panel_y
            + if phase == 0 {
                LOGIN_USERNAME_Y
            } else {
                LOGIN_PASSWORD_Y
            };
        if phase == state.login_phase {
            draw_dotted_rect(
                screen,
                panel_x + LOGIN_FIELD_X - 5,
                y - 5,
                LOGIN_FIELD_WIDTH + 10,
                LOGIN_FIELD_HEIGHT + 10,
                crate::framebuffer::Color::new(0, 0, 0),
            );
        } else {
            clear_focus_rect(
                screen,
                panel_x + LOGIN_FIELD_X - 5,
                y - 5,
                LOGIN_FIELD_WIDTH + 10,
                LOGIN_FIELD_HEIGHT + 10,
            );
        }
    }
    draw_login_caret(state, screen);
}

fn draw_login_caret(state: &State, screen: &mut crate::framebuffer::Screen) {
    let panel_x = state.framebuffer.width.saturating_sub(LOGIN_WIDTH) / 2;
    let panel_y = state.framebuffer.height.saturating_sub(LOGIN_HEIGHT) / 2;
    let x = panel_x + LOGIN_FIELD_X + 12 + state.console_column * 12;
    let y = panel_y
        + if state.login_phase == 0 {
            LOGIN_USERNAME_Y + 12
        } else {
            LOGIN_PASSWORD_Y + 12
        };
    screen.fill_rect(x, y, 2, 18, crate::framebuffer::Color::new(0, 0, 0));
}

fn erase_login_caret(state: &State, screen: &mut crate::framebuffer::Screen) {
    let panel_x = state.framebuffer.width.saturating_sub(LOGIN_WIDTH) / 2;
    let panel_y = state.framebuffer.height.saturating_sub(LOGIN_HEIGHT) / 2;
    let x = panel_x + LOGIN_FIELD_X + 12 + state.console_column * 12;
    let y = panel_y
        + if state.login_phase == 0 {
            LOGIN_USERNAME_Y + 12
        } else {
            LOGIN_PASSWORD_Y + 12
        };
    screen.fill_rect(x, y, 12, 18, crate::framebuffer::Color::new(255, 255, 255));
}

fn draw_login_button(state: &mut State, pressed: bool) {
    let Some(mut screen) = crate::framebuffer::Screen::new(state.framebuffer) else {
        return;
    };
    restore_cursor_under(state, &mut screen, state.cursor_x, state.cursor_y);
    let panel_x = state.framebuffer.width.saturating_sub(LOGIN_WIDTH) / 2;
    let panel_y = state.framebuffer.height.saturating_sub(LOGIN_HEIGHT) / 2;
    if pressed {
        draw_sunken_panel(&mut screen, panel_x + 410, panel_y + 275, 230, 46);
    } else {
        draw_raised_panel(&mut screen, panel_x + 410, panel_y + 275, 230, 46);
    }
    screen.draw_text(
        panel_x + 444 + u32::from(pressed),
        panel_y + 291 + u32::from(pressed),
        2,
        "LOG ON  [ENTER]",
        crate::framebuffer::Color::new(0, 0, 0),
    );
    state.cursor_under_valid = false;
    capture_cursor_under(state, &screen);
    draw_cursor(&mut screen, state.cursor_x, state.cursor_y);
}

fn draw_raised_panel(
    screen: &mut crate::framebuffer::Screen,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) {
    let face = crate::framebuffer::Color::new(192, 192, 192);
    let light = crate::framebuffer::Color::new(255, 255, 255);
    let shadow = crate::framebuffer::Color::new(128, 128, 128);
    let dark = crate::framebuffer::Color::new(0, 0, 0);
    screen.fill_rect(x, y, width, height, face);
    screen.fill_rect(x, y, width, 2, light);
    screen.fill_rect(x, y, 2, height, light);
    screen.fill_rect(x + width.saturating_sub(2), y, 2, height, dark);
    screen.fill_rect(x, y + height.saturating_sub(2), width, 2, dark);
    screen.fill_rect(x + 2, y + 2, width.saturating_sub(4), 1, face);
    screen.fill_rect(x + 2, y + 2, 1, height.saturating_sub(4), face);
    screen.fill_rect(
        x + width.saturating_sub(3),
        y + 2,
        1,
        height.saturating_sub(4),
        shadow,
    );
    screen.fill_rect(
        x + 2,
        y + height.saturating_sub(3),
        width.saturating_sub(4),
        1,
        shadow,
    );
}

fn draw_sunken_panel(
    screen: &mut crate::framebuffer::Screen,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) {
    let face = crate::framebuffer::Color::new(192, 192, 192);
    let light = crate::framebuffer::Color::new(255, 255, 255);
    let shadow = crate::framebuffer::Color::new(128, 128, 128);
    let dark = crate::framebuffer::Color::new(0, 0, 0);
    screen.fill_rect(x, y, width, height, face);
    screen.fill_rect(x, y, width, 2, shadow);
    screen.fill_rect(x, y, 2, height, shadow);
    screen.fill_rect(x + 2, y + 2, width.saturating_sub(4), 1, dark);
    screen.fill_rect(x + 2, y + 2, 1, height.saturating_sub(4), dark);
    screen.fill_rect(x + width.saturating_sub(2), y, 2, height, light);
    screen.fill_rect(x, y + height.saturating_sub(2), width, 2, light);
}

fn draw_sunken_field(
    screen: &mut crate::framebuffer::Screen,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) {
    draw_sunken_panel(screen, x, y, width, height);
    screen.fill_rect(
        x + 4,
        y + 4,
        width.saturating_sub(7),
        height.saturating_sub(7),
        crate::framebuffer::Color::new(255, 255, 255),
    );
}

fn draw_dotted_rect(
    screen: &mut crate::framebuffer::Screen,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    color: crate::framebuffer::Color,
) {
    for offset in (0..width).step_by(4) {
        screen.fill_rect(x + offset, y, 2, 1, color);
        screen.fill_rect(x + offset, y + height.saturating_sub(1), 2, 1, color);
    }
    for offset in (0..height).step_by(4) {
        screen.fill_rect(x, y + offset, 1, 2, color);
        screen.fill_rect(x + width.saturating_sub(1), y + offset, 1, 2, color);
    }
}

fn clear_focus_rect(
    screen: &mut crate::framebuffer::Screen,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) {
    let face = crate::framebuffer::Color::new(192, 192, 192);
    screen.fill_rect(x, y, width, 1, face);
    screen.fill_rect(x, y + height.saturating_sub(1), width, 1, face);
    screen.fill_rect(x, y, 1, height, face);
    screen.fill_rect(x + width.saturating_sub(1), y, 1, height, face);
}

pub fn create(width: u32, height: u32) -> u64 {
    create_reserved(width, height, 0)
}

/// Create a surface in a stable 1-based app slot. Slot 0 selects first free.
/// Stable slots keep title/taskbar identity independent of process launch order.
pub fn create_reserved(width: u32, height: u32, slot: u64) -> u64 {
    if !GRAPHICS.ready.load(Ordering::Acquire)
        || width == 0
        || height == 0
        || width > MAX_WIDTH
        || height > MAX_HEIGHT
        || slot > MAX_SURFACES as u64
    {
        return 0;
    }
    let pid = current_pid();
    let handle = with_lock(|state| {
        let index = if slot == 0 {
            state.surfaces.iter().position(|surface| !surface.created)
        } else {
            let preferred = slot as usize - 1;
            (!state.surfaces[preferred].created).then_some(preferred)
        };
        let Some(index) = index else {
            return 0;
        };
        state.pixels[index].fill(0xff10_1826);
        let compact = state.framebuffer.width <= 800;
        let (x, y) = if index == 0 {
            if compact { (40, 88) } else { (72, 112) }
        } else if index == TERMINAL_SURFACE && compact {
            (72, 126)
        } else if index == BROWSER_SURFACE && compact {
            (50, 100)
        } else if compact {
            (118, 118)
        } else if index == TERMINAL_SURFACE {
            (280, 190)
        } else {
            (390, 150)
        };
        state.surfaces[index] = Surface {
            created: true,
            owner_pid: pid,
            width,
            height,
            backing_width: width,
            backing_height: height,
            x,
            y,
            presented: false,
            minimized: false,
            dirty: false,
        };
        state.surface_events[index].fill(SurfaceEvent::EMPTY);
        state.surface_event_heads[index] = 0;
        state.surface_event_tails[index] = 0;
        state.surface_event_enabled[index] = false;
        if index == TERMINAL_SURFACE {
            update_terminal_window_size(state);
            terminal_write_locked(state, b"\x1b[2J\x1b[H\x1b[?25h");
            crate::serial_println!(
                "MAKOS_TERMINAL_ANSI_OK parser=bounded backend=cells cursor=1 erase=1 sgr=16-color alternate=1 winsize={}x{}",
                state.terminal_window_size.columns,
                state.terminal_window_size.rows,
            );
        }
        (index + 1) as u64
    });
    if handle == (TERMINAL_SURFACE + 1) as u64 {
        sync_terminal_window_size();
    }
    handle
}

pub fn read_event(handle: u64) -> Option<SurfaceEvent> {
    let index = handle_index(handle)?;
    let pid = current_pid();
    with_lock(|state| {
        if !state.surfaces[index].created || state.surfaces[index].owner_pid != pid {
            return None;
        }
        state.surface_event_enabled[index] = true;
        let tail = usize::from(state.surface_event_tails[index]);
        if tail == usize::from(state.surface_event_heads[index]) {
            return None;
        }
        let event = state.surface_events[index][tail];
        state.surface_events[index][tail] = SurfaceEvent::EMPTY;
        state.surface_event_tails[index] = ((tail + 1) % SURFACE_EVENT_QUEUE) as u8;
        Some(event)
    })
}

pub fn event_handle_valid(handle: u64) -> bool {
    let Some(index) = handle_index(handle) else {
        return false;
    };
    let pid = current_pid();
    with_lock(|state| state.surfaces[index].created && state.surfaces[index].owner_pid == pid)
}

/// A blocked surface waiter is ready only for its own queued events. Invalid
/// or destroyed handles are also ready so the retried syscall can fail closed
/// and window teardown cannot strand a joining thread.
pub fn event_wait_ready(handle: u64, owner_pid: u64) -> bool {
    let Some(index) = handle_index(handle) else {
        return true;
    };
    with_lock(|state| {
        let surface = state.surfaces[index];
        !surface.created
            || surface.owner_pid != owner_pid
            || state.surface_event_tails[index] != state.surface_event_heads[index]
    })
}

/// Routes keys only to surfaces that opted into event delivery. Terminal,
/// login, and modal Text Edit retain the legacy key queue until migrated.
pub fn route_key_event(key: u8) -> bool {
    const KEY_EDITOR_SAVE: u8 = 0x83;
    const KEY_SELECT_ALL: u8 = 0x84;
    const KEY_COPY: u8 = 0x85;
    const KEY_CUT: u8 = 0x86;
    const KEY_PASTE: u8 = 0x87;
    let mut terminal_copy = [0u8; TERMINAL_CLIPBOARD_BYTES];
    let mut terminal_copy_length = 0usize;
    let mut terminal_paste = false;
    let action = with_lock(|state| {
        if state.login_console && key == b'\t' {
            crate::serial_println!(
                "MAKOS_LOGIN_TAB_OK from={} to={} key=Tab focus=visible",
                if state.login_phase == 0 {
                    "username"
                } else {
                    "password"
                },
                if state.login_phase == 0 {
                    "password"
                } else {
                    "log-on"
                },
            );
            return 0;
        }
        let Some(index) = state.focused_surface else {
            return 0;
        };
        if index == TERMINAL_SURFACE {
            let mut echo = DiscardBytes;
            match key {
                KEY_COPY if terminal_has_selection(state) => {
                    terminal_copy_length = copy_terminal_selection(state, &mut terminal_copy);
                }
                KEY_COPY => {
                    let _ = state.terminal_input.receive(0x03, &mut echo);
                }
                KEY_PASTE => terminal_paste = true,
                KEY_SELECT_ALL => {
                    let _ = state.terminal_input.receive(0x01, &mut echo);
                }
                KEY_CUT => {
                    let _ = state.terminal_input.receive(0x18, &mut echo);
                }
                KEY_EDITOR_SAVE => {
                    let _ = state.terminal_input.receive(0x13, &mut echo);
                }
                _ => return 0,
            }
            return 1;
        }
        if index == SETTINGS_SURFACE && state.settings_user_open {
            let mut action = 1;
            match key {
                0x1b => reset_settings_user(state, true),
                b'\t' => state.settings_user_field = (state.settings_user_field + 1) % 3,
                b'\n' if state.settings_user_field < 2 => {
                    state.settings_user_field += 1;
                }
                b'\n' => {
                    state.settings_submit_pending = true;
                    state.settings_submit_after_ms = graphics_uptime_millis() + 750;
                    action = 2;
                }
                8 => match state.settings_user_field {
                    0 if state.settings_username_len != 0 => {
                        state.settings_username_len -= 1;
                        state.settings_username[usize::from(state.settings_username_len)] = 0;
                    }
                    1 if state.settings_password_len != 0 => {
                        state.settings_password_len -= 1;
                        state.settings_password[usize::from(state.settings_password_len)] = 0;
                    }
                    2 if state.settings_confirmation_len != 0 => {
                        state.settings_confirmation_len -= 1;
                        state.settings_confirmation[usize::from(state.settings_confirmation_len)] =
                            0;
                    }
                    _ => {}
                },
                0x20..=0x7e => match state.settings_user_field {
                    0 if usize::from(state.settings_username_len) < SETTINGS_USERNAME_BYTES => {
                        let offset = usize::from(state.settings_username_len);
                        state.settings_username[offset] = key;
                        state.settings_username_len += 1;
                    }
                    1 if usize::from(state.settings_password_len) < SETTINGS_PASSWORD_BYTES => {
                        let offset = usize::from(state.settings_password_len);
                        state.settings_password[offset] = key;
                        state.settings_password_len += 1;
                    }
                    2 if usize::from(state.settings_confirmation_len) < SETTINGS_PASSWORD_BYTES => {
                        let offset = usize::from(state.settings_confirmation_len);
                        state.settings_confirmation[offset] = key;
                        state.settings_confirmation_len += 1;
                    }
                    _ => {}
                },
                _ => {}
            }
            state.settings_user_status = SETTINGS_STATUS_READY;
            compose_key_feedback(state);
            return action;
        }
        if !state.surfaces[index].created || !state.surface_event_enabled[index] {
            return 0;
        }
        push_surface_event(
            state,
            index,
            SurfaceEvent {
                kind: 1,
                key: u32::from(key),
                modifiers: 0,
                x: 0,
                y: 0,
                width: state.surfaces[index].width,
                height: state.surfaces[index].height,
            },
        );
        1
    });
    #[cfg(target_arch = "aarch64")]
    if terminal_copy_length != 0 {
        if crate::aarch64_clipboard::write(&terminal_copy[..terminal_copy_length]).is_ok() {
            crate::serial_println!(
                "MAKOS_TERMINAL_CLIPBOARD_OK action=copy bytes={} highlight=visible",
                terminal_copy_length,
            );
        }
    }
    if terminal_paste {
        paste_terminal_clipboard();
    }
    action != 0
}

#[cfg(target_arch = "aarch64")]
pub fn begin_input_batch() {
    INPUT_BATCHING.store(true, Ordering::Release);
}

#[cfg(target_arch = "aarch64")]
pub fn end_input_batch() {
    INPUT_BATCHING.store(false, Ordering::Release);
    if !INPUT_BATCH_DIRTY.load(Ordering::Acquire) {
        return;
    }
    let now = graphics_uptime_millis();
    let last = INPUT_LAST_COMPOSE_MS.load(Ordering::Acquire);
    if now.saturating_sub(last) < 33 {
        return;
    }
    let composed = with_lock(|state| {
        if state.settings_submit_pending {
            return false;
        }
        if !INPUT_BATCH_DIRTY.swap(false, Ordering::AcqRel) {
            return false;
        }
        compose(state);
        true
    });
    if composed {
        INPUT_LAST_COMPOSE_MS.store(now, Ordering::Release);
    }
}

fn compose_key_feedback(state: &mut State) {
    #[cfg(target_arch = "aarch64")]
    if INPUT_BATCHING.load(Ordering::Acquire) {
        INPUT_BATCH_DIRTY.store(true, Ordering::Release);
        return;
    }
    compose(state);
}

/// Executes CPU0-owned blocking compositor work after safe lower-EL input/timer
/// IRQ dispatch. An input IRQ that interrupted EL1 only acknowledges its edge;
/// the retained timer recovery poll reaches this service after returning safe.
#[cfg(target_arch = "aarch64")]
pub fn service_deferred_actions() {
    if !GRAPHICS.ready.load(Ordering::Acquire) {
        return;
    }
    if crate::arch::cpu_index() != 0 {
        crate::fatal("AArch64 deferred compositor service attempted from non-owner CPU");
    }
    if DEFERRED_COMPOSE_PENDING.swap(false, Ordering::AcqRel) {
        with_lock(compose_owner);
        GPU_OWNER_DEFERRED_COMPOSES.fetch_add(1, Ordering::AcqRel);
    }
    if current_pid() == 1 {
        let signout = with_lock(|state| {
            if !state.signout_pending {
                return false;
            }
            state.signout_pending = false;
            true
        });
        if signout {
            crate::serial_println!(
                "MAKOS_SIGNOUT_DEFERRED_OK source=start-menu context=svc-yield blocking-in-irq=0"
            );
            let _ = crate::security::sign_out();
            return;
        }
    }
    let submission = with_lock(|state| {
        if !state.settings_submit_pending
            || graphics_uptime_millis() < state.settings_submit_after_ms
        {
            return None;
        }
        state.settings_submit_pending = false;
        state.settings_submit_after_ms = 0;
        Some((
            state.settings_username,
            state.settings_password,
            state.settings_confirmation,
            usize::from(state.settings_username_len),
            usize::from(state.settings_password_len),
            usize::from(state.settings_confirmation_len),
        ))
    });
    if let Some((username, password, confirmation, username_len, password_len, confirmation_len)) =
        submission
    {
        crate::serial_println!(
            "MAKOS_SETTINGS_ADDUSER_DEFERRED_OK source=input-irq context=svc-yield blocking-in-irq=0 username_bytes={} password_bytes={} confirmation_bytes={}",
            username_len,
            password_len,
            confirmation_len,
        );
        submit_settings_user(
            username,
            password,
            confirmation,
            username_len,
            password_len,
            confirmation_len,
        );
    }
    service_system_monitor();
    service_taskbar_clock();
}

#[cfg(target_arch = "aarch64")]
fn service_system_monitor() {
    let now = graphics_uptime_millis();
    let next = MONITOR_NEXT_REFRESH_MS.load(Ordering::Acquire);
    if now < next
        || MONITOR_NEXT_REFRESH_MS
            .compare_exchange(
                next,
                now.saturating_add(1_000),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
    {
        return;
    }
    let refreshed = with_lock(|state| {
        let surface = state.surfaces[MONITOR_SURFACE];
        if state.login_console
            || state.focused_surface != Some(MONITOR_SURFACE)
            || !surface.presented
            || surface.minimized
        {
            return None;
        }
        let Some(mut screen) = crate::framebuffer::Screen::new(state.framebuffer) else {
            return None;
        };
        restore_cursor_under(state, &mut screen, state.cursor_x, state.cursor_y);
        draw_system_monitor_contents(state, &mut screen);
        state.cursor_under_valid = false;
        capture_cursor_under(state, &screen);
        draw_cursor(&mut screen, state.cursor_x, state.cursor_y);
        Some(surface)
    });
    let Some(surface) = refreshed else {
        return;
    };
    flush_scanout_rect(surface.x, surface.y, surface.width, surface.height);
    if !MONITOR_LIVE_REPORTED.swap(true, Ordering::AcqRel) {
        crate::serial_println!(
            "MAKOS_SYSTEM_MONITOR_LIVE_OK cadence_ms=1000 source=svc-yield damage=content-only focused-only=1"
        );
    }
}

#[cfg(target_arch = "aarch64")]
fn service_taskbar_clock() {
    let now = graphics_uptime_millis();
    let next = TASKBAR_CLOCK_NEXT_CHECK_MS.load(Ordering::Acquire);
    if now < next
        || TASKBAR_CLOCK_NEXT_CHECK_MS
            .compare_exchange(
                next,
                now.saturating_add(1_000),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
    {
        return;
    }
    let minute = crate::aarch64_rtc::unix_seconds() / 60;
    if TASKBAR_CLOCK_MINUTE.swap(minute, Ordering::AcqRel) == minute {
        return;
    }
    with_lock(|state| {
        if !state.login_console && state.surfaces.iter().any(|surface| surface.presented) {
            compose(state);
        }
    });
}

pub fn fill_rect(handle: u64, x: u32, y: u32, width: u32, height: u32, argb: u32) -> bool {
    let Some(index) = handle_index(handle) else {
        return false;
    };
    let pid = current_pid();
    with_lock(|state| {
        let surface = state.surfaces[index];
        if !surface.created || surface.owner_pid != pid {
            return false;
        }
        let end_x = x.saturating_add(width).min(surface.width);
        let end_y = y.saturating_add(height).min(surface.height);
        let mut changed = false;
        for row in y.min(end_y)..end_y {
            let start = row as usize * MAX_WIDTH as usize + x.min(end_x) as usize;
            let end = row as usize * MAX_WIDTH as usize + end_x as usize;
            let pixels = &mut state.pixels[index][start..end];
            if pixels.iter().any(|pixel| *pixel != argb) {
                pixels.fill(argb);
                changed = true;
            }
        }
        if changed {
            state.surfaces[index].dirty = true;
        }
        true
    })
}

/// Copy native-endian premultiplied ARGB pixels into caller-owned retained
/// surface storage. Source bytes remain userspace-owned and are consumed
/// synchronously; compositor never keeps their address.
pub fn blit_argb(
    handle: u64,
    bytes: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    destination_x: u32,
    destination_y: u32,
) -> bool {
    let Some(index) = handle_index(handle) else {
        return false;
    };
    let Some(row_bytes) = width.checked_mul(4) else {
        return false;
    };
    let Some(required) = height
        .checked_sub(1)
        .and_then(|rows| rows.checked_mul(stride))
        .and_then(|prefix| prefix.checked_add(row_bytes))
        .and_then(|length| usize::try_from(length).ok())
    else {
        return false;
    };
    if width == 0
        || height == 0
        || width > MAX_WIDTH
        || height > MAX_HEIGHT
        || stride < row_bytes
        || stride > MAX_WIDTH * 4
        || required > bytes.len()
    {
        return false;
    }
    let pid = current_pid();
    with_lock(|state| {
        let surface = state.surfaces[index];
        if !surface.created || surface.owner_pid != pid {
            return false;
        }
        let copy_width = width.min(surface.width.saturating_sub(destination_x));
        let copy_height = height.min(surface.height.saturating_sub(destination_y));
        let mut changed = false;
        for row in 0..copy_height {
            let source_row = row as usize * stride as usize;
            let destination_row = (destination_y + row) as usize * MAX_WIDTH as usize;
            for column in 0..copy_width {
                let source = source_row + column as usize * 4;
                let pixel = u32::from_ne_bytes([
                    bytes[source],
                    bytes[source + 1],
                    bytes[source + 2],
                    bytes[source + 3],
                ]);
                let destination = &mut state.pixels[index]
                    [destination_row + destination_x as usize + column as usize];
                if *destination != pixel {
                    *destination = pixel;
                    changed = true;
                }
            }
        }
        if changed {
            state.surfaces[index].dirty = true;
        }
        true
    })
}

/// Draw bounded ASCII into caller-owned surface backing. Browser/GUI clients
/// get retained pixels; no userspace pointer survives return.
pub fn surface_text(handle: u64, mut x: u32, mut y: u32, bytes: &[u8]) -> bool {
    let Some(index) = handle_index(handle) else {
        return false;
    };
    if bytes.len() > 1024
        || bytes
            .iter()
            .any(|byte| !matches!(*byte, b'\n' | 0x20..=0x7e))
    {
        return false;
    }
    let pid = current_pid();
    with_lock(|state| {
        let surface = state.surfaces[index];
        if !surface.created || surface.owner_pid != pid {
            return false;
        }
        let origin_x = x;
        let mut changed = false;
        for byte in bytes.iter().copied() {
            if byte == b'\n' {
                x = origin_x;
                y = y.saturating_add(16);
                continue;
            }
            let glyph = crate::framebuffer::glyph(byte);
            for (row, bits) in glyph.iter().copied().enumerate() {
                for column in 0..5u32 {
                    if bits & (1 << (4 - column)) == 0 {
                        continue;
                    }
                    let pixel_x = x.saturating_add(column);
                    let pixel_y = y.saturating_add(row as u32);
                    if pixel_x < surface.width && pixel_y < surface.height {
                        let offset = pixel_y as usize * MAX_WIDTH as usize + pixel_x as usize;
                        if state.pixels[index][offset] != 0xff00_0000 {
                            state.pixels[index][offset] = 0xff00_0000;
                            changed = true;
                        }
                    }
                }
            }
            x = x.saturating_add(8);
            if x >= surface.width {
                break;
            }
        }
        if changed {
            state.surfaces[index].dirty = true;
        }
        true
    })
}

pub fn present(handle: u64) -> bool {
    static REDUNDANT_PRESENT_REPORTED: AtomicBool = AtomicBool::new(false);
    let Some(index) = handle_index(handle) else {
        return false;
    };
    let pid = current_pid();
    with_lock(|state| {
        if !state.surfaces[index].created || state.surfaces[index].owner_pid != pid {
            return false;
        }
        let first_present = !state.surfaces[index].presented;
        if !first_present && !state.surfaces[index].dirty {
            if !REDUNDANT_PRESENT_REPORTED.swap(true, Ordering::AcqRel) {
                crate::serial_println!(
                    "MAKOS_COMPOSITOR_IDLE_OK event_driven=1 retained=1 redundant_present=skipped scanout_flush=0"
                );
            }
            return true;
        }
        state.surfaces[index].presented = true;
        if first_present {
            state.surfaces[index].minimized = false;
            state.focused_surface = Some(index);
            raise_surface(state, index);
        }
        state.surfaces[index].dirty = false;
        state.presents += 1;
        #[cfg(target_arch = "aarch64")]
        let immediate_scanout = crate::arch::cpu_index() == 0;
        #[cfg(target_arch = "x86_64")]
        let immediate_scanout = true;
        compose(state);
        let windows = state
            .surfaces
            .iter()
            .filter(|surface| surface.presented)
            .count();
        crate::serial_println!(
            "MAKOS_M7_OK graphics_abi=1 surface={}x{} compositor=1 present={} scanout={} windows={} z_order=1 clipping=1 deferred={}",
            state.surfaces[index].width,
            state.surfaces[index].height,
            state.presents,
            u8::from(immediate_scanout),
            windows,
            u8::from(!immediate_scanout),
        );
        true
    })
}

pub fn close_all(pid: u64) -> usize {
    with_lock(|state| {
        let mut closed = 0usize;
        for index in 0..MAX_SURFACES {
            if state.surfaces[index].created && state.surfaces[index].owner_pid == pid {
                state.surfaces[index] = Surface::EMPTY;
                state.pixels[index].fill(0);
                state.surface_events[index].fill(SurfaceEvent::EMPTY);
                state.surface_event_heads[index] = 0;
                state.surface_event_tails[index] = 0;
                state.surface_event_enabled[index] = false;
                closed += 1;
            }
        }
        if closed != 0 {
            state.focused_surface = None;
            state.drag_surface = None;
            state.resize_surface = None;
            state.drag_outline_valid = false;
            state.start_menu_open = false;
            compose(state);
        }
        closed
    })
}

#[cfg(target_arch = "aarch64")]
pub fn reset_session_surfaces(pid: u64) -> usize {
    let closed = close_all(pid);
    with_lock(|state| {
        state.terminal_cells.fill(TerminalCell::BLANK);
        state.terminal_alternate_cells.fill(TerminalCell::BLANK);
        state.terminal_parser = AnsiParser::new();
        state.terminal_input = TerminalInput::new(Termios::raw());
        state.terminal_column = 0;
        state.terminal_row = 0;
        state.terminal_saved_column = 0;
        state.terminal_saved_row = 0;
        state.terminal_main_column = 0;
        state.terminal_main_row = 0;
        state.terminal_scroll_top = 0;
        state.terminal_scroll_bottom = TERMINAL_ROWS - 1;
        state.terminal_attributes = Attributes::default_terminal();
        state.terminal_cursor_visible = true;
        state.terminal_alternate = false;
        state.terminal_auto_wrap = true;
        state.terminal_insert_mode = false;
        state.terminal_application_cursor = false;
        state.terminal_application_keypad = false;
        state.terminal_bracketed_paste = false;
        state.terminal_selection_anchor = 0;
        state.terminal_selection_end = 0;
        state.terminal_selecting = false;
        reset_settings_user(state, true);
        state.pressed_button = 0;
        state.pressed_surface = 0;
        state.pressed_value = 0;
        state.settings_submit_pending = false;
        state.settings_submit_after_ms = 0;
        state.signout_pending = false;
    });
    closed
}

pub fn close(handle: u64) -> bool {
    let Some(index) = handle_index(handle) else {
        return false;
    };
    let pid = current_pid();
    with_lock(|state| {
        if !state.surfaces[index].created || state.surfaces[index].owner_pid != pid {
            return false;
        }
        state.surfaces[index].presented = false;
        state.surfaces[index].minimized = false;
        state.focused_surface = topmost_visible(state, Some(index));
        state.start_menu_open = false;
        compose(state);
        true
    })
}

pub fn destroy(handle: u64) -> bool {
    let Some(index) = handle_index(handle) else {
        return false;
    };
    let pid = current_pid();
    let destroyed = with_lock(|state| {
        if !state.surfaces[index].created || state.surfaces[index].owner_pid != pid {
            return false;
        }
        state.surfaces[index] = Surface::EMPTY;
        state.pixels[index].fill(0);
        state.surface_events[index].fill(SurfaceEvent::EMPTY);
        state.surface_event_heads[index] = 0;
        state.surface_event_tails[index] = 0;
        state.surface_event_enabled[index] = false;
        if state.focused_surface == Some(index) {
            state.focused_surface = None;
        }
        if state.drag_surface == Some(index) {
            state.drag_surface = None;
        }
        if state.resize_surface == Some(index) {
            state.resize_surface = None;
        }
        compose(state);
        true
    });
    if destroyed {
        // A widget watcher may be blocked in SURFACE_WAIT_EVENT. Invalidate
        // its handle on retry so window teardown can join without waiting for
        // unrelated physical input.
        #[cfg(target_arch = "aarch64")]
        crate::aarch64_process::wake_input_waiters();
    }
    destroyed
}

fn compose(state: &mut State) {
    #[cfg(target_arch = "aarch64")]
    if crate::arch::cpu_index() != 0 {
        DEFERRED_COMPOSE_PENDING.store(true, Ordering::Release);
        GPU_NONOWNER_COMPOSE_DEFERRALS.fetch_add(1, Ordering::AcqRel);
        return;
    }
    compose_owner(state);
}

fn compose_owner(state: &mut State) {
    let Some(mut screen) = crate::framebuffer::Screen::new(state.framebuffer) else {
        return;
    };
    screen.clear(crate::framebuffer::Color::new(0, 128, 128));
    let screen_width = screen.width();
    for index in state.z_order {
        let surface = state.surfaces[index];
        if !surface.presented || surface.minimized {
            continue;
        }
        draw_raised_panel(
            &mut screen,
            surface.x.saturating_sub(4),
            surface.y.saturating_sub(30),
            surface.width.saturating_add(8),
            surface.height.saturating_add(34),
        );
        let focused = state.focused_surface == Some(index);
        screen.fill_rect(
            surface.x,
            surface.y.saturating_sub(26),
            surface.width,
            22,
            if focused {
                crate::framebuffer::Color::new(0, 0, 128)
            } else {
                crate::framebuffer::Color::new(128, 128, 128)
            },
        );
        screen.draw_text(
            surface.x + 7,
            surface.y.saturating_sub(20),
            1,
            surface_title(index),
            crate::framebuffer::Color::new(255, 255, 255),
        );
        let close_left = surface.x + surface.width.saturating_sub(20);
        let close_top = surface.y.saturating_sub(24);
        let close_pressed = state.pressed_button == 9 && state.pressed_surface == index;
        if close_pressed {
            draw_sunken_panel(&mut screen, close_left, close_top, 17, 17);
        } else {
            draw_raised_panel(&mut screen, close_left, close_top, 17, 17);
        }
        screen.draw_text(
            close_left + 6 + u32::from(close_pressed),
            close_top + 5 + u32::from(close_pressed),
            1,
            "X",
            crate::framebuffer::Color::new(0, 0, 0),
        );
        for row in 0..surface.height {
            let mut run_start = 0u32;
            let mut current = state.pixels[index][row as usize * MAX_WIDTH as usize];
            for column in 1..=surface.width {
                let next = if column == surface.width {
                    !current
                } else {
                    state.pixels[index][row as usize * MAX_WIDTH as usize + column as usize]
                };
                if next != current {
                    screen.fill_rect(
                        surface.x + run_start,
                        surface.y + row,
                        column - run_start,
                        1,
                        color(current),
                    );
                    run_start = column;
                    current = next;
                }
            }
        }
        if index == MONITOR_SURFACE {
            draw_system_monitor_contents(state, &mut screen);
        } else if index == TERMINAL_SURFACE {
            draw_terminal_contents(state, &mut screen);
        } else if index == SETTINGS_SURFACE {
            draw_settings_contents(state, &mut screen);
        }
        if state.pressed_button == 11 && state.pressed_surface == index {
            let left = surface.x + surface.width.saturating_sub(82);
            let top = surface.y + 4;
            draw_sunken_panel(&mut screen, left, top, 74, 22);
            screen.draw_text(
                left + 20,
                top + 8,
                1,
                "SAVE",
                crate::framebuffer::Color::new(0, 0, 0),
            );
        }
        draw_resize_grip(&mut screen, surface);
    }
    draw_taskbar(state, &mut screen, screen_width);
    state.cursor_under_valid = false;
    capture_cursor_under(state, &screen);
    draw_cursor(&mut screen, state.cursor_x, state.cursor_y);
    flush_scanout();
}

#[cfg(target_arch = "aarch64")]
pub fn reset_gpu_service_affinity_evidence() {
    DEFERRED_COMPOSE_PENDING.store(false, Ordering::Release);
    GPU_NONOWNER_COMPOSE_DEFERRALS.store(0, Ordering::Release);
    GPU_OWNER_DEFERRED_COMPOSES.store(0, Ordering::Release);
}

#[cfg(target_arch = "aarch64")]
pub fn gpu_service_affinity_evidence() -> (u64, u64, bool) {
    (
        GPU_OWNER_DEFERRED_COMPOSES.load(Ordering::Acquire),
        GPU_NONOWNER_COMPOSE_DEFERRALS.load(Ordering::Acquire),
        DEFERRED_COMPOSE_PENDING.load(Ordering::Acquire),
    )
}

pub fn mouse_packet(x: u32, y: u32, buttons: u8) {
    static REPORTED: AtomicBool = AtomicBool::new(false);
    let Some((
        cursor_x,
        cursor_y,
        focused,
        left_pressed,
        buttons_changed,
        action,
        action_surface,
        action_reopened,
        menu_open,
        action_width,
        action_height,
        old_cursor_x,
        old_cursor_y,
        outline_active,
        pressed_visual,
    )) = with_lock(|state| {
        let cursor_x = x.min(state.framebuffer.width.saturating_sub(1));
        let cursor_y = y.min(state.framebuffer.height.saturating_sub(1));
        let buttons = buttons & 7;
        if cursor_x == state.cursor_x
            && cursor_y == state.cursor_y
            && buttons == state.cursor_buttons
        {
            return None;
        }
        let old_cursor_x = state.cursor_x;
        let old_cursor_y = state.cursor_y;
        let left_pressed = buttons & 1 != 0 && state.cursor_buttons & 1 == 0;
        let left_released = buttons & 1 == 0 && state.cursor_buttons & 1 != 0;
        let buttons_changed = buttons & 7 != state.cursor_buttons & 7;
        state.cursor_x = cursor_x;
        state.cursor_y = cursor_y;
        state.cursor_buttons = buttons & 7;
        let mut action = 0u8;
        let mut action_surface = 0usize;
        let mut action_reopened = false;
        let mut action_width = 0u32;
        let mut action_height = 0u32;
        if state.login_console {
            // Login owns scanout until authentication. Never invoke desktop
            // composition here: no app surfaces exist yet, so that would erase
            // form into an empty desktop when user clicks.
            redraw_cursor(state, old_cursor_x, old_cursor_y);
            let panel_x = state.framebuffer.width.saturating_sub(LOGIN_WIDTH) / 2;
            let panel_y = state.framebuffer.height.saturating_sub(LOGIN_HEIGHT) / 2;
            let username = (
                panel_x + LOGIN_FIELD_X,
                panel_y + LOGIN_USERNAME_Y,
                LOGIN_FIELD_WIDTH,
                LOGIN_FIELD_HEIGHT,
            );
            let password = (
                panel_x + LOGIN_FIELD_X,
                panel_y + LOGIN_PASSWORD_Y,
                LOGIN_FIELD_WIDTH,
                LOGIN_FIELD_HEIGHT,
            );
            let logon = (panel_x + 410, panel_y + 275, 230, 46);
            if left_pressed && point_in(logon, cursor_x, cursor_y) {
                state.pressed_button = 5;
                draw_login_button(state, true);
            } else if left_released && state.pressed_button == 5 {
                state.pressed_button = 0;
                draw_login_button(state, false);
                if point_in(logon, cursor_x, cursor_y) {
                    action = 18;
                    action_surface = usize::from(state.login_phase);
                }
            } else if left_pressed && point_in(username, cursor_x, cursor_y) {
                action = if state.login_phase == 0 { 7 } else { 19 };
                action_surface = 0;
            } else if left_pressed && point_in(password, cursor_x, cursor_y) {
                action = if state.login_phase == 0 { 17 } else { 7 };
                action_surface = 1;
            }
        } else if state.pressed_button != 0 && left_released {
            let pressed = state.pressed_button;
            let pressed_surface = state.pressed_surface;
            let pressed_value = state.pressed_value;
            state.pressed_button = 0;
            state.pressed_surface = 0;
            state.pressed_value = 0;
            match pressed {
                1 if point_in(
                    monitor_refresh_button_layout(state.surfaces[MONITOR_SURFACE]),
                    cursor_x,
                    cursor_y,
                ) =>
                {
                    action = 13;
                }
                2 if point_in(
                    settings_add_user_button_layout(state.surfaces[SETTINGS_SURFACE]),
                    cursor_x,
                    cursor_y,
                ) =>
                {
                    open_settings_user(state);
                    action = 14;
                }
                3 if point_in(
                    settings_user_layout(state.surfaces[SETTINGS_SURFACE])[3],
                    cursor_x,
                    cursor_y,
                ) =>
                {
                    state.settings_submit_pending = true;
                    state.settings_submit_after_ms = graphics_uptime_millis() + 750;
                    action = 15;
                }
                4 if point_in(
                    settings_user_layout(state.surfaces[SETTINGS_SURFACE])[4],
                    cursor_x,
                    cursor_y,
                ) =>
                {
                    reset_settings_user(state, true);
                    action = 16;
                }
                6 if start_button_hit(state, cursor_x, cursor_y) => {
                    state.start_menu_open = !state.start_menu_open;
                    action = 6;
                }
                7 if start_menu_item_hit(state, cursor_x, cursor_y) == Some(pressed_surface) => {
                    action_surface = pressed_surface;
                    action_reopened = !state.surfaces[pressed_surface].presented;
                    state.surfaces[pressed_surface].presented = true;
                    state.surfaces[pressed_surface].minimized = false;
                    state.focused_surface = Some(pressed_surface);
                    state.start_menu_open = false;
                    raise_surface(state, pressed_surface);
                    action = 5;
                }
                8 if taskbar_hit(state, cursor_x, cursor_y) == Some(pressed_surface) => {
                    state.start_menu_open = false;
                    action_surface = pressed_surface;
                    if state.surfaces[pressed_surface].minimized
                        || state.focused_surface != Some(pressed_surface)
                    {
                        state.surfaces[pressed_surface].minimized = false;
                        state.focused_surface = Some(pressed_surface);
                        raise_surface(state, pressed_surface);
                    } else {
                        state.surfaces[pressed_surface].minimized = true;
                        state.focused_surface = topmost_visible(state, Some(pressed_surface));
                    }
                    action = 4;
                }
                9 if state.surfaces[pressed_surface].created
                    && close_button_hit(state.surfaces[pressed_surface], cursor_x, cursor_y) =>
                {
                    action_surface = pressed_surface;
                    action = request_surface_close(state, pressed_surface);
                }
                10 if settings_mode_index_hit(state, cursor_x, cursor_y)
                    == Some(usize::from(pressed_value)) =>
                {
                    let (width, height) =
                        [(800, 600), (1024, 768), (1280, 800)][usize::from(pressed_value)];
                    if apply_display_mode(state, width, height) {
                        action = 10;
                        action_width = width;
                        action_height = height;
                    }
                }
                11 if pressed_surface == TEXT_EDIT_SURFACE
                    && text_save_button_hit(
                        state.surfaces[TEXT_EDIT_SURFACE],
                        cursor_x,
                        cursor_y,
                    ) =>
                {
                    action_surface = TEXT_EDIT_SURFACE;
                    let surface = state.surfaces[TEXT_EDIT_SURFACE];
                    if state.surface_event_enabled[TEXT_EDIT_SURFACE] {
                        push_surface_event(
                            state,
                            TEXT_EDIT_SURFACE,
                            SurfaceEvent {
                                kind: 1,
                                key: 0x83,
                                modifiers: 0,
                                x: 0,
                                y: 0,
                                width: surface.width,
                                height: surface.height,
                            },
                        );
                    }
                    action = 12;
                }
                12 if start_menu_signout_hit(state, cursor_x, cursor_y) => {
                    state.start_menu_open = false;
                    state.signout_pending = true;
                    action = 20;
                }
                _ => {}
            }
            if !matches!(action, 2 | 11) {
                compose(state);
            }
        } else if state.resize_surface.is_some() && buttons & 1 != 0 {
            redraw_resize_outline(state, old_cursor_x, old_cursor_y);
        } else if state.resize_surface.is_some() && left_released {
            action_surface = state.resize_surface.unwrap();
            (action_width, action_height) = finish_resize(state, old_cursor_x, old_cursor_y);
            if state.surface_event_enabled[action_surface] {
                push_surface_event(
                    state,
                    action_surface,
                    SurfaceEvent {
                        kind: 3,
                        key: 0,
                        modifiers: 0,
                        x: 0,
                        y: 0,
                        width: action_width,
                        height: action_height,
                    },
                );
            }
            action = 8;
        } else if state.drag_surface.is_some() && buttons & 1 != 0 {
            redraw_drag_outline(state, old_cursor_x, old_cursor_y);
        } else if state.drag_surface.is_some() && left_released {
            action_surface = state.drag_surface.unwrap();
            finish_drag(state, old_cursor_x, old_cursor_y);
            action = 3;
        } else if state.terminal_selecting && buttons & 1 != 0 {
            if let Some(index) = terminal_cell_at(state, cursor_x, cursor_y) {
                if state.terminal_selection_end != index {
                    state.terminal_selection_end = index;
                    compose(state);
                } else {
                    redraw_cursor(state, old_cursor_x, old_cursor_y);
                }
            } else {
                redraw_cursor(state, old_cursor_x, old_cursor_y);
            }
        } else if state.terminal_selecting && left_released {
            if let Some(index) = terminal_cell_at(state, cursor_x, cursor_y) {
                state.terminal_selection_end = index;
            }
            state.terminal_selecting = false;
            compose(state);
        } else if left_pressed {
            if start_menu_signout_hit(state, cursor_x, cursor_y) {
                state.pressed_button = 12;
                compose(state);
            } else if let Some(index) = start_menu_item_hit(state, cursor_x, cursor_y) {
                state.pressed_button = 7;
                state.pressed_surface = index;
                compose(state);
            } else if start_button_hit(state, cursor_x, cursor_y) {
                state.pressed_button = 6;
                compose(state);
            } else if let Some(index) = taskbar_hit(state, cursor_x, cursor_y) {
                state.pressed_button = 8;
                state.pressed_surface = index;
                compose(state);
            } else if let Some(index) = window_hit(state, cursor_x, cursor_y) {
                action_surface = index;
                let surface = state.surfaces[index];
                state.focused_surface = Some(index);
                raise_surface(state, index);
                if close_button_hit(surface, cursor_x, cursor_y) {
                    state.pressed_button = 9;
                    state.pressed_surface = index;
                    compose(state);
                } else if resize_grip_hit(surface, cursor_x, cursor_y) {
                    state.resize_surface = Some(index);
                    state.resize_offset_x =
                        cursor_x as i32 - surface.x.saturating_add(surface.width) as i32;
                    state.resize_offset_y =
                        cursor_y as i32 - surface.y.saturating_add(surface.height) as i32;
                    state.resize_width = surface.width;
                    state.resize_height = surface.height;
                    state.drag_outline_valid = false;
                    compose(state);
                    action = 9;
                } else if titlebar_hit(surface, cursor_x, cursor_y) {
                    state.drag_surface = Some(index);
                    state.drag_offset_x = cursor_x as i32 - surface.x as i32;
                    state.drag_offset_y = cursor_y as i32 - surface.y as i32;
                    state.drag_x = surface.x;
                    state.drag_y = surface.y;
                    state.drag_outline_valid = false;
                    compose(state);
                    action = 1;
                } else if index == SETTINGS_SURFACE {
                    if state.settings_user_open {
                        let layout = settings_user_layout(surface);
                        if let Some(field) = layout[..3]
                            .iter()
                            .position(|rect| point_in(*rect, cursor_x, cursor_y))
                        {
                            state.settings_user_field = field as u8;
                            compose(state);
                        } else if point_in(layout[3], cursor_x, cursor_y) {
                            state.pressed_button = 3;
                            compose(state);
                        } else if point_in(layout[4], cursor_x, cursor_y) {
                            state.pressed_button = 4;
                            compose(state);
                        } else {
                            compose(state);
                        }
                    } else if point_in(settings_add_user_button_layout(surface), cursor_x, cursor_y)
                    {
                        state.pressed_button = 2;
                        compose(state);
                    } else if let Some(mode) = settings_mode_index_hit(state, cursor_x, cursor_y) {
                        state.pressed_button = 10;
                        state.pressed_value = mode as u8;
                        compose(state);
                    } else {
                        compose(state);
                    }
                } else if index == MONITOR_SURFACE
                    && point_in(monitor_refresh_button_layout(surface), cursor_x, cursor_y)
                {
                    state.pressed_button = 1;
                    compose(state);
                } else if cfg!(target_arch = "aarch64")
                    && index == TEXT_EDIT_SURFACE
                    && text_save_button_hit(surface, cursor_x, cursor_y)
                {
                    state.pressed_button = 11;
                    state.pressed_surface = index;
                    compose(state);
                } else if index == TERMINAL_SURFACE {
                    if let Some(cell) = terminal_cell_at(state, cursor_x, cursor_y) {
                        state.terminal_selection_anchor = cell;
                        state.terminal_selection_end = cell;
                        state.terminal_selecting = true;
                    }
                    compose(state);
                } else {
                    if state.surface_event_enabled[index] {
                        push_surface_event(
                            state,
                            index,
                            SurfaceEvent {
                                kind: 2,
                                key: 1,
                                modifiers: 0,
                                x: cursor_x.saturating_sub(surface.x) as i32,
                                y: cursor_y.saturating_sub(surface.y) as i32,
                                width: surface.width,
                                height: surface.height,
                            },
                        );
                    }
                    compose(state);
                }
            } else {
                state.start_menu_open = false;
                state.focused_surface = None;
                compose(state);
            }
        } else {
            redraw_cursor(state, old_cursor_x, old_cursor_y);
        }
        if !left_pressed
            && (cursor_x != old_cursor_x
                || cursor_y != old_cursor_y
                || buttons_changed
                || left_released)
            && state.drag_surface.is_none()
            && state.resize_surface.is_none()
            && state.pressed_button == 0
        {
            if let Some(index) = state.focused_surface {
                if state.surface_event_enabled[index] {
                    let surface = state.surfaces[index];
                    push_surface_event(
                        state,
                        index,
                        SurfaceEvent {
                            kind: 2,
                            key: u32::from(buttons & 7),
                            modifiers: 0,
                            x: cursor_x as i32 - surface.x as i32,
                            y: cursor_y as i32 - surface.y as i32,
                            width: surface.width,
                            height: surface.height,
                        },
                    );
                }
            }
        }
        Some((
            cursor_x,
            cursor_y,
            state.focused_surface.map(|index| index + 1).unwrap_or(0),
            left_pressed,
            buttons_changed,
            action,
            action_surface + 1,
            action_reopened,
            state.start_menu_open,
            action_width,
            action_height,
            old_cursor_x,
            old_cursor_y,
            state.drag_surface.is_some() || state.resize_surface.is_some(),
            state.pressed_button,
        ))
    })
    else {
        return;
    };
    #[cfg(target_arch = "aarch64")]
    crate::aarch64_virtio_gpu::move_cursor(cursor_x, cursor_y);
    #[cfg(target_arch = "aarch64")]
    if matches!(action, 17 | 18) {
        crate::aarch64_virtio_input::inject_key(b'\t');
    }
    #[cfg(target_arch = "aarch64")]
    if action == 19 {
        crate::aarch64_virtio_input::inject_key(0x1b);
    }
    if outline_active {
        flush_scanout();
    } else if cfg!(target_arch = "x86_64") {
        let left = old_cursor_x.min(cursor_x);
        let top = old_cursor_y.min(cursor_y);
        let right = old_cursor_x
            .saturating_add(CURSOR_WIDTH as u32)
            .max(cursor_x.saturating_add(CURSOR_WIDTH as u32));
        let bottom = old_cursor_y
            .saturating_add(CURSOR_HEIGHT as u32)
            .max(cursor_y.saturating_add(CURSOR_HEIGHT as u32));
        flush_scanout_rect(
            left,
            top,
            right.saturating_sub(left),
            bottom.saturating_sub(top),
        );
    } else if buttons_changed {
        // Pure AArch64 motion is a cursor-queue command only.  Button edges may
        // change native controls (notably login feedback), so publish that
        // bounded scene update without coupling every pointer packet to a 2D
        // scanout transfer.
        flush_scanout();
    }
    if !REPORTED.swap(true, Ordering::AcqRel) {
        crate::serial_println!(
            "MAKOS_MOUSE_OK x={} y={} packets=1 cursor={} redraw={} no_trails=1 input=irq coalesce=latest unchanged=noop scene_shadow={} edge_queue=32 accel=adaptive drag_outline=fast buttons={} click_focus={} focused_surface={}",
            cursor_x,
            cursor_y,
            CURSOR_BACKEND,
            if cfg!(target_arch = "aarch64") {
                "cursorq-no-scanout"
            } else {
                "shadow-restore"
            },
            if cfg!(target_arch = "aarch64") {
                "not-required"
            } else {
                "cursor-free"
            },
            buttons & 7,
            u8::from(left_pressed),
            focused,
        );
    }
    if left_pressed {
        crate::serial_println!(
            "MAKOS_CURSOR_FOCUS_OK cursor={} buttons=left hit_test=1 focused_surface={} z_order=raised x={} y={}",
            CURSOR_BACKEND,
            focused,
            cursor_x,
            cursor_y,
        );
        if pressed_visual != 0 {
            crate::serial_println!(
                "MAKOS_BUTTON_PRESS_OK control={} phase=mouse-down bevel=sunken action=on-release cancel=pointer-leave",
                pressed_visual,
            );
        }
    }
    if action == 8 && action_surface == TERMINAL_SURFACE + 1 {
        sync_terminal_window_size();
        #[cfg(target_arch = "aarch64")]
        {
            let size = terminal_window_size();
            crate::serial_println!(
                "MAKOS_TTY_RESIZE_OK rows={} columns={} pixels={}x{} sigwinch=queued-on-change",
                size.rows,
                size.columns,
                size.pixel_width,
                size.pixel_height,
            );
        }
    } else if action == 10 {
        sync_terminal_window_size();
    }
    match action {
        2 => crate::serial_println!(
            "MAKOS_WINDOW_CLOSE_OK surface={} close_button=1 taskbar_removed=1",
            action_surface
        ),
        3 => crate::serial_println!(
            "MAKOS_WINDOW_DRAG_OK surface={} outline=fast commit=release x={} y={}",
            action_surface,
            cursor_x,
            cursor_y,
        ),
        4 => crate::serial_println!(
            "MAKOS_TASKBAR_APP_OK surface={} activate=1 minimize_toggle=1",
            action_surface
        ),
        5 => crate::serial_println!(
            "MAKOS_APP_REOPEN_OK app={} source=start-menu surface={} reopened={}",
            app_id(action_surface.saturating_sub(1)),
            action_surface,
            u8::from(action_reopened),
        ),
        6 if menu_open => crate::serial_println!(
            "MAKOS_START_MENU_OK launcher=1 open=1 apps={}",
            with_lock(|state| state
                .surfaces
                .iter()
                .filter(|surface| surface.created)
                .count()),
        ),
        7 => crate::serial_println!(
            "MAKOS_LOGIN_CLICK_OK form=retained blank_screen=0 active_field={} cursor={}",
            if action_surface == 1 {
                "username"
            } else {
                "password"
            },
            CURSOR_BACKEND,
        ),
        8 => crate::serial_println!(
            "MAKOS_WINDOW_RESIZE_OK surface={} outline=fast commit=release width={} height={} backing=bounded cursor_safe=1",
            action_surface,
            action_width,
            action_height,
        ),
        10 => crate::serial_println!(
            "MAKOS_SETTINGS_DISPLAY_OK requested={}x{} applied=1 backend=virtio-gpu live=1",
            action_width,
            action_height,
        ),
        11 => crate::serial_println!(
            "MAKOS_TEXT_EDIT_CLOSE_REQUEST_OK source=titlebar key=Escape surface={}",
            action_surface,
        ),
        12 => crate::serial_println!(
            "MAKOS_TEXT_EDIT_SAVE_REQUEST_OK source=button key=Ctrl-S surface={}",
            action_surface,
        ),
        13 => {
            let stats = monitor_stats();
            crate::serial_println!(
                "MAKOS_SYSTEM_MONITOR_REFRESH_OK source=button uptime_ms={} total_mib={} free_mib={} live={} runnable={} blocked={} zombies={} current_pid={} switches={} feedback=sunken-on-press",
                stats.uptime_ms,
                stats.total_mib,
                stats.free_mib,
                stats.live,
                stats.runnable,
                stats.blocked,
                stats.zombies,
                stats.current_pid,
                stats.switches,
            );
        }
        14 => crate::serial_println!(
            "MAKOS_SETTINGS_USERS_OPEN_OK source=button fields=username,password,confirm password=hidden tab=next feedback=sunken-on-press"
        ),
        16 => {
            crate::serial_println!("MAKOS_SETTINGS_USERS_CANCEL_OK secrets=zeroed return=settings")
        }
        17 => crate::serial_println!(
            "MAKOS_LOGIN_NAV_OK source=mouse target=password key=Tab clickable=1"
        ),
        18 => crate::serial_println!(
            "MAKOS_LOGIN_BUTTON_OK source=mouse action=next-or-submit feedback=sunken-on-press"
        ),
        19 => crate::serial_println!(
            "MAKOS_LOGIN_NAV_OK source=mouse target=username reset=secure clickable=1"
        ),
        20 => crate::serial_println!(
            "MAKOS_SIGNOUT_REQUEST_OK source=start-menu feedback=sunken-on-press deferred=svc-yield"
        ),
        _ => {}
    }
}

/// Delivers wheel motion to the focused owner surface. Scroll events preserve
/// pointer-local x/y; signed horizontal/vertical deltas occupy modifiers/key.
pub fn mouse_scroll(delta_x: i32, delta_y: i32) {
    if delta_x == 0 && delta_y == 0 {
        return;
    }
    with_lock(|state| {
        if state.login_console {
            return;
        }
        let Some(index) = state.focused_surface else {
            return;
        };
        if !state.surfaces[index].created || !state.surface_event_enabled[index] {
            return;
        }
        let surface = state.surfaces[index];
        let local_x = state.cursor_x as i32 - surface.x as i32;
        let local_y = state.cursor_y as i32 - surface.y as i32;
        push_surface_event(
            state,
            index,
            SurfaceEvent {
                kind: 5,
                key: delta_y as u32,
                modifiers: delta_x as u32,
                x: local_x,
                y: local_y,
                width: surface.width,
                height: surface.height,
            },
        );
    });
}

fn surface_title(index: usize) -> &'static str {
    if index == TERMINAL_SURFACE {
        "TERMINAL"
    } else if index == SETTINGS_SURFACE {
        "SETTINGS"
    } else if index == TEXT_EDIT_SURFACE {
        "TEXT EDIT"
    } else if index == BROWSER_SURFACE {
        "BROWSER"
    } else if index == FILES_SURFACE {
        "FILES"
    } else {
        "MONITOR"
    }
}

fn app_id(index: usize) -> &'static str {
    if index == TERMINAL_SURFACE {
        "terminal"
    } else if index == SETTINGS_SURFACE {
        "settings"
    } else if index == TEXT_EDIT_SURFACE {
        "text-edit"
    } else if index == BROWSER_SURFACE {
        "browser"
    } else if index == FILES_SURFACE {
        "files"
    } else {
        "system-monitor"
    }
}

fn raise_surface(state: &mut State, index: usize) {
    let Some(position) = state
        .z_order
        .iter()
        .position(|candidate| *candidate == index)
    else {
        return;
    };
    state
        .z_order
        .copy_within(position + 1..MAX_SURFACES, position);
    state.z_order[MAX_SURFACES - 1] = index;
}

fn topmost_visible(state: &State, excluded: Option<usize>) -> Option<usize> {
    state.z_order.iter().rev().copied().find(|index| {
        Some(*index) != excluded
            && state.surfaces[*index].created
            && state.surfaces[*index].presented
            && !state.surfaces[*index].minimized
    })
}

fn window_hit(state: &State, x: u32, y: u32) -> Option<usize> {
    state.z_order.iter().rev().copied().find(|index| {
        let surface = state.surfaces[*index];
        surface.created
            && surface.presented
            && !surface.minimized
            && x >= surface.x.saturating_sub(4)
            && y >= surface.y.saturating_sub(30)
            && x < surface.x.saturating_add(surface.width).saturating_add(4)
            && y < surface.y.saturating_add(surface.height).saturating_add(4)
    })
}

fn titlebar_hit(surface: Surface, x: u32, y: u32) -> bool {
    x >= surface.x
        && x < surface.x.saturating_add(surface.width).saturating_sub(24)
        && y >= surface.y.saturating_sub(26)
        && y < surface.y.saturating_sub(4)
}

fn close_button_hit(surface: Surface, x: u32, y: u32) -> bool {
    x >= surface.x.saturating_add(surface.width).saturating_sub(20)
        && x < surface.x.saturating_add(surface.width).saturating_sub(3)
        && y >= surface.y.saturating_sub(24)
        && y < surface.y.saturating_sub(7)
}

fn text_save_button_hit(surface: Surface, x: u32, y: u32) -> bool {
    x >= surface.x.saturating_add(surface.width.saturating_sub(82))
        && x < surface.x.saturating_add(surface.width.saturating_sub(8))
        && y >= surface.y.saturating_add(4)
        && y < surface.y.saturating_add(26)
}

fn request_surface_close(state: &mut State, index: usize) -> u8 {
    #[cfg(target_arch = "aarch64")]
    let surface = state.surfaces[index];
    #[cfg(target_arch = "aarch64")]
    if index == TEXT_EDIT_SURFACE {
        if state.surface_event_enabled[index] {
            push_surface_event(
                state,
                index,
                SurfaceEvent {
                    kind: 1,
                    key: 0x1b,
                    modifiers: 0,
                    x: 0,
                    y: 0,
                    width: surface.width,
                    height: surface.height,
                },
            );
        }
        compose(state);
        return 11;
    }
    if index == SETTINGS_SURFACE {
        reset_settings_user(state, true);
    }
    #[cfg(target_arch = "aarch64")]
    if state.surface_event_enabled[index] {
        push_surface_event(
            state,
            index,
            SurfaceEvent {
                kind: 4,
                key: 0,
                modifiers: 0,
                x: 0,
                y: 0,
                width: surface.width,
                height: surface.height,
            },
        );
    }
    state.surfaces[index].presented = false;
    state.surfaces[index].minimized = false;
    state.start_menu_open = false;
    state.focused_surface = topmost_visible(state, Some(index));
    compose(state);
    2
}

fn resize_grip_hit(surface: Surface, x: u32, y: u32) -> bool {
    x >= surface
        .x
        .saturating_add(surface.width)
        .saturating_sub(RESIZE_GRIP)
        && x < surface.x.saturating_add(surface.width).saturating_add(4)
        && y >= surface
            .y
            .saturating_add(surface.height)
            .saturating_sub(RESIZE_GRIP)
        && y < surface.y.saturating_add(surface.height).saturating_add(4)
}

fn minimum_surface_size(index: usize) -> (u32, u32) {
    if index == TERMINAL_SURFACE {
        (360, 180)
    } else if index == SETTINGS_SURFACE {
        (320, 240)
    } else if index == TEXT_EDIT_SURFACE {
        (300, 200)
    } else if index == BROWSER_SURFACE {
        (320, 220)
    } else if index == FILES_SURFACE {
        (300, 220)
    } else {
        (240, 160)
    }
}

fn reset_settings_user(state: &mut State, close: bool) {
    state.settings_username.fill(0);
    state.settings_username_len = 0;
    state.settings_password.fill(0);
    state.settings_password_len = 0;
    state.settings_confirmation.fill(0);
    state.settings_confirmation_len = 0;
    state.settings_user_field = 0;
    state.settings_user_status = SETTINGS_STATUS_READY;
    state.settings_submit_pending = false;
    state.settings_submit_after_ms = 0;
    if close {
        state.settings_user_open = false;
    }
}

fn open_settings_user(state: &mut State) {
    reset_settings_user(state, false);
    state.settings_user_open = true;
}

#[cfg(target_arch = "aarch64")]
fn submit_settings_user(
    mut username: [u8; SETTINGS_USERNAME_BYTES],
    mut password: [u8; SETTINGS_PASSWORD_BYTES],
    mut confirmation: [u8; SETTINGS_PASSWORD_BYTES],
    username_len: usize,
    password_len: usize,
    confirmation_len: usize,
) {
    if password_len != confirmation_len
        || password[..password_len] != confirmation[..confirmation_len]
    {
        password.fill(0);
        confirmation.fill(0);
        with_lock(|state| {
            state.settings_confirmation.fill(0);
            state.settings_confirmation_len = 0;
            state.settings_user_field = 2;
            state.settings_user_status = SETTINGS_STATUS_MISMATCH;
            compose(state);
        });
        return;
    }

    let result = crate::security::add_user_from_system_settings(
        &username[..username_len],
        &password[..password_len],
    );
    password.fill(0);
    confirmation.fill(0);
    match result {
        Ok((uid, gid)) => {
            let safe_username = core::str::from_utf8(&username[..username_len]).unwrap_or("?");
            with_lock(|state| {
                state.settings_password.fill(0);
                state.settings_password_len = 0;
                state.settings_confirmation.fill(0);
                state.settings_confirmation_len = 0;
                state.settings_user_status = SETTINGS_STATUS_CREATED;
                compose(state);
            });
            crate::serial_println!(
                "MAKOS_SETTINGS_ADDUSER_OK user={} uid={} gid={} persisted=makfs-vfs password=pbkdf2-hmac-sha256 plaintext=never-stored",
                safe_username,
                uid,
                gid,
            );
        }
        Err(error) => {
            let status = match error {
                crate::security::AddUserError::Permission => SETTINGS_STATUS_PERMISSION,
                crate::security::AddUserError::InvalidUsername => SETTINGS_STATUS_INVALID_USERNAME,
                crate::security::AddUserError::InvalidPassword => SETTINGS_STATUS_INVALID_PASSWORD,
                crate::security::AddUserError::Exists => SETTINGS_STATUS_EXISTS,
                crate::security::AddUserError::Full => SETTINGS_STATUS_FULL,
                crate::security::AddUserError::Storage => SETTINGS_STATUS_STORAGE,
            };
            with_lock(|state| {
                state.settings_password.fill(0);
                state.settings_password_len = 0;
                state.settings_confirmation.fill(0);
                state.settings_confirmation_len = 0;
                state.settings_user_field = if matches!(
                    status,
                    SETTINGS_STATUS_INVALID_USERNAME | SETTINGS_STATUS_EXISTS
                ) {
                    0
                } else {
                    1
                };
                state.settings_user_status = status;
                compose(state);
            });
            crate::serial_println!(
                "MAKOS_SETTINGS_ADDUSER_ERROR status={} plaintext=never-logged",
                settings_status_text(status),
            );
        }
    }
    username.fill(0);
}

fn settings_status_text(status: u8) -> &'static str {
    match status {
        SETTINGS_STATUS_CREATED => "USER CREATED. ACCOUNT READY AT LOGON.",
        SETTINGS_STATUS_MISMATCH => "PASSWORDS DO NOT MATCH. TYPE CONFIRMATION AGAIN.",
        SETTINGS_STATUS_INVALID_USERNAME => "USER NAME: LOWERCASE LETTER FIRST; USE A-Z 0-9 _ -.",
        SETTINGS_STATUS_INVALID_PASSWORD => "PASSWORD MUST CONTAIN 8 TO 64 CHARACTERS.",
        SETTINGS_STATUS_EXISTS => "THAT USER NAME ALREADY EXISTS.",
        SETTINGS_STATUS_FULL => "ACCOUNT DATABASE FULL.",
        SETTINGS_STATUS_STORAGE => "ACCOUNT STORAGE FAILED.",
        SETTINGS_STATUS_PERMISSION => "ACTIVE ADMIN SESSION REQUIRED.",
        _ => "TAB MOVES FOCUS. ENTER CONTINUES. PASSWORDS STAY HIDDEN.",
    }
}

fn settings_add_user_button_layout(surface: Surface) -> (u32, u32, u32, u32) {
    let margin = 14u32;
    let gap = 7u32;
    let card_width = surface.width.saturating_sub(margin * 2);
    let card_height = surface.height.saturating_sub(margin * 2 + gap * 3) / 4;
    (
        surface.x + margin + card_width.saturating_sub(126),
        surface.y + margin + 3 * (card_height + gap) + card_height.saturating_sub(31),
        116,
        24,
    )
}

fn settings_user_layout(surface: Surface) -> [(u32, u32, u32, u32); 5] {
    let left = surface.x + 112;
    let width = surface.width.saturating_sub(126);
    let top = surface.y + 53;
    let spacing = ((surface.height.saturating_sub(132)) / 3).clamp(38, 48);
    [
        (left, top, width, 28),
        (left, top + spacing, width, 28),
        (left, top + spacing * 2, width, 28),
        (
            surface.x + surface.width.saturating_sub(198),
            surface.y + surface.height.saturating_sub(42),
            86,
            28,
        ),
        (
            surface.x + surface.width.saturating_sub(104),
            surface.y + surface.height.saturating_sub(42),
            90,
            28,
        ),
    ]
}

fn point_in(rect: (u32, u32, u32, u32), x: u32, y: u32) -> bool {
    x >= rect.0 && x < rect.0 + rect.2 && y >= rect.1 && y < rect.1 + rect.3
}

fn monitor_refresh_button_layout(surface: Surface) -> (u32, u32, u32, u32) {
    (
        surface.x + surface.width.saturating_sub(118),
        surface.y + surface.height.saturating_sub(38),
        104,
        26,
    )
}

fn settings_mode_layout(surface: Surface) -> Option<(u32, u32, u32, u32)> {
    let margin = 14u32;
    let gap = 7u32;
    let card_width = surface.width.saturating_sub(margin * 2);
    let card_height = surface.height.saturating_sub(margin * 2 + gap * 3) / 4;
    if card_height < 72 || card_width < 240 {
        return None;
    }
    let button_gap = 4;
    let start_x = surface.x + margin + 8;
    let available = card_width.saturating_sub(16 + button_gap * 2);
    let button_width = available / 3;
    Some((start_x, surface.y + margin + 43, button_width, 24))
}

fn settings_mode_index_hit(state: &State, x: u32, y: u32) -> Option<usize> {
    let surface = state.surfaces[SETTINGS_SURFACE];
    let (start_x, top, button_width, height) = settings_mode_layout(surface)?;
    if y < top || y >= top + height {
        return None;
    }
    for index in 0..3usize {
        let left = start_x + index as u32 * (button_width + 4);
        if x >= left && x < left + button_width {
            return Some(index);
        }
    }
    None
}

fn apply_display_mode(state: &mut State, width: u32, height: u32) -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        let _ = (state, width, height);
        false
    }
    #[cfg(target_arch = "aarch64")]
    {
        let Some(framebuffer) = crate::aarch64_virtio_gpu::set_mode(width, height) else {
            return false;
        };
        if !crate::framebuffer::install_scene_shadow(framebuffer) {
            crate::fatal("display mode exceeds cursor-safe shadow bounds");
        }
        state.framebuffer = framebuffer;
        state.cursor_x = state.cursor_x.min(width.saturating_sub(1));
        state.cursor_y = state.cursor_y.min(height.saturating_sub(1));
        state.cursor_under_valid = false;
        state.drag_surface = None;
        state.resize_surface = None;
        state.drag_outline_valid = false;
        for index in 0..MAX_SURFACES {
            if !state.surfaces[index].created {
                continue;
            }
            let (minimum_width, minimum_height) = minimum_surface_size(index);
            let max_width = state.surfaces[index]
                .backing_width
                .min(width.saturating_sub(8))
                .max(minimum_width.min(state.surfaces[index].backing_width));
            let max_height = state.surfaces[index]
                .backing_height
                .min(height.saturating_sub(TASKBAR_HEIGHT + 34))
                .max(minimum_height.min(state.surfaces[index].backing_height));
            state.surfaces[index].width = state.surfaces[index].width.min(max_width);
            state.surfaces[index].height = state.surfaces[index].height.min(max_height);
            state.surfaces[index].x = state.surfaces[index]
                .x
                .min(width.saturating_sub(state.surfaces[index].width + 4).max(4));
            state.surfaces[index].y = state.surfaces[index].y.min(
                height
                    .saturating_sub(TASKBAR_HEIGHT + state.surfaces[index].height + 4)
                    .max(30),
            );
        }
        if state.surfaces[TERMINAL_SURFACE].created {
            update_terminal_window_size(state);
        }
        compose(state);
        true
    }
}

fn taskbar_hit(state: &State, x: u32, y: u32) -> Option<usize> {
    if y < state.framebuffer.height.saturating_sub(TASKBAR_HEIGHT) {
        return None;
    }
    let app_width = taskbar_app_width(state.framebuffer.width);
    let mut column = 0u32;
    for index in 0..DESKTOP_SURFACES {
        let surface = state.surfaces[index];
        if !surface.created || !surface.presented {
            continue;
        }
        let left = TASKBAR_APP_X + column * (app_width + TASKBAR_APP_GAP);
        if x >= left && x < left + app_width {
            return Some(index);
        }
        column += 1;
    }
    None
}

fn start_button_hit(state: &State, x: u32, y: u32) -> bool {
    y >= state.framebuffer.height.saturating_sub(TASKBAR_HEIGHT)
        && x >= START_BUTTON_X
        && x < START_BUTTON_X + START_BUTTON_WIDTH
}

fn start_menu_item_hit(state: &State, x: u32, y: u32) -> Option<usize> {
    if !state.start_menu_open {
        return None;
    }
    let menu_y = state
        .framebuffer
        .height
        .saturating_sub(TASKBAR_HEIGHT + start_menu_height(state));
    let mut row = 0u32;
    for index in 0..DESKTOP_SURFACES {
        if !state.surfaces[index].created {
            continue;
        }
        let top = menu_y + 8 + (row + 1) * (START_MENU_ITEM_HEIGHT + 4);
        if x >= START_BUTTON_X + 8
            && x < START_BUTTON_X + START_MENU_WIDTH - 8
            && y >= top
            && y < top + START_MENU_ITEM_HEIGHT
        {
            return Some(index);
        }
        row += 1;
    }
    None
}

fn start_menu_signout_hit(state: &State, x: u32, y: u32) -> bool {
    if !state.start_menu_open {
        return false;
    }
    let menu_y = state
        .framebuffer
        .height
        .saturating_sub(TASKBAR_HEIGHT + start_menu_height(state));
    point_in(
        (
            START_BUTTON_X + 8,
            menu_y + 8,
            START_MENU_WIDTH - 16,
            START_MENU_ITEM_HEIGHT,
        ),
        x,
        y,
    )
}

fn draw_taskbar(state: &State, screen: &mut crate::framebuffer::Screen, screen_width: u32) {
    let taskbar_y = screen.height().saturating_sub(TASKBAR_HEIGHT);
    draw_raised_panel(screen, 0, taskbar_y, screen_width, TASKBAR_HEIGHT);
    if state.start_menu_open || state.pressed_button == 6 {
        draw_sunken_panel(
            screen,
            START_BUTTON_X,
            taskbar_y + 6,
            START_BUTTON_WIDTH,
            36,
        );
    } else {
        draw_raised_panel(
            screen,
            START_BUTTON_X,
            taskbar_y + 6,
            START_BUTTON_WIDTH,
            36,
        );
    }
    screen.draw_text(
        24,
        taskbar_y + 17,
        2,
        "START",
        crate::framebuffer::Color::new(0, 0, 0),
    );
    let app_width = taskbar_app_width(screen_width);
    let mut column = 0u32;
    for index in 0..DESKTOP_SURFACES {
        let surface = state.surfaces[index];
        if !surface.created || !surface.presented {
            continue;
        }
        let x = TASKBAR_APP_X + column * (app_width + TASKBAR_APP_GAP);
        if (state.focused_surface == Some(index) && !surface.minimized)
            || (state.pressed_button == 8 && state.pressed_surface == index)
        {
            draw_sunken_panel(screen, x, taskbar_y + 6, app_width, 36);
        } else {
            draw_raised_panel(screen, x, taskbar_y + 6, app_width, 36);
        }
        let label_scale = if app_width >= 196 { 2 } else { 1 };
        screen.draw_text(
            x + 12,
            taskbar_y + if label_scale == 2 { 17 } else { 21 },
            label_scale,
            surface_title(index),
            crate::framebuffer::Color::new(0, 0, 0),
        );
        column += 1;
    }
    let tray_x = screen_width.saturating_sub(146);
    draw_sunken_panel(screen, tray_x, taskbar_y + 6, 140, 36);
    let clock = taskbar_clock_text();
    let clock = core::str::from_utf8(&clock).unwrap_or("--:--");
    screen.draw_text(
        tray_x + 12,
        taskbar_y + 13,
        1,
        "NET",
        crate::framebuffer::Color::new(0, 0, 0),
    );
    screen.draw_text(
        tray_x + 88,
        taskbar_y + 13,
        1,
        clock,
        crate::framebuffer::Color::new(0, 0, 0),
    );
    screen.draw_text(
        tray_x + 12,
        taskbar_y + 27,
        1,
        if cfg!(target_arch = "aarch64") {
            "DHCP / UTC"
        } else {
            "SYSTEM OK"
        },
        crate::framebuffer::Color::new(0, 0, 0),
    );
    if state.start_menu_open {
        draw_start_menu(state, screen, taskbar_y);
    }
}

fn taskbar_clock_text() -> [u8; 5] {
    #[cfg(target_arch = "aarch64")]
    let seconds = crate::aarch64_rtc::unix_seconds();
    #[cfg(target_arch = "x86_64")]
    let seconds = crate::arch::monotonic_ticks() / 100;
    let minutes = seconds / 60 % 60;
    let hours = seconds / 3600 % 24;
    [
        b'0' + (hours / 10) as u8,
        b'0' + (hours % 10) as u8,
        b':',
        b'0' + (minutes / 10) as u8,
        b'0' + (minutes % 10) as u8,
    ]
}

fn taskbar_app_width(screen_width: u32) -> u32 {
    let tray_left = screen_width.saturating_sub(146);
    tray_left
        .saturating_sub(TASKBAR_APP_X)
        .saturating_sub(TASKBAR_APP_GAP * (DESKTOP_SURFACES as u32 - 1))
        / DESKTOP_SURFACES as u32
}

#[derive(Clone, Copy)]
struct MonitorStats {
    uptime_ms: u64,
    total_mib: u64,
    free_mib: u64,
    live: usize,
    runnable: usize,
    blocked: usize,
    zombies: usize,
    current_pid: u64,
    switches: u64,
}

fn monitor_stats() -> MonitorStats {
    #[cfg(target_arch = "aarch64")]
    {
        let processes = crate::aarch64_process::runtime_stats();
        MonitorStats {
            uptime_ms: crate::arch::uptime_millis(),
            total_mib: crate::mm::managed_mib(),
            free_mib: crate::mm::free_frames() as u64 * 4096 / (1024 * 1024),
            live: processes.live,
            runnable: processes.runnable,
            blocked: processes.blocked,
            zombies: processes.zombies,
            current_pid: processes.current_pid,
            switches: processes.timer_switches,
        }
    }
    #[cfg(target_arch = "x86_64")]
    {
        MonitorStats {
            uptime_ms: crate::arch::monotonic_ticks().saturating_mul(10),
            total_mib: crate::mm::managed_mib(),
            free_mib: crate::mm::free_frames() as u64 * 4096 / (1024 * 1024),
            live: 0,
            runnable: 0,
            blocked: 0,
            zombies: 0,
            current_pid: current_pid(),
            switches: 0,
        }
    }
}

fn draw_u64_line(
    screen: &mut crate::framebuffer::Screen,
    x: u32,
    y: u32,
    prefix: &[u8],
    value: u64,
    suffix: &[u8],
) {
    let mut line = [0u8; 96];
    let mut used = copy_bytes(&mut line, prefix);
    used += append_u64(&mut line[used..], value);
    used += copy_bytes(&mut line[used..], suffix);
    let text = core::str::from_utf8(&line[..used]).unwrap_or("?");
    screen.draw_text(x, y, 1, text, crate::framebuffer::Color::new(0, 0, 0));
}

fn draw_system_monitor_contents(state: &State, screen: &mut crate::framebuffer::Screen) {
    let surface = state.surfaces[MONITOR_SURFACE];
    let stats = monitor_stats();
    let black = crate::framebuffer::Color::new(0, 0, 0);
    screen.fill_rect(
        surface.x,
        surface.y,
        surface.width,
        surface.height,
        crate::framebuffer::Color::new(192, 192, 192),
    );
    screen.draw_text(
        surface.x + 14,
        surface.y + 12,
        1,
        "SYSTEM MONITOR / LIVE KERNEL DATA",
        black,
    );
    draw_sunken_panel(
        screen,
        surface.x + 12,
        surface.y + 34,
        surface.width.saturating_sub(24),
        surface.height.saturating_sub(84),
    );
    screen.fill_rect(
        surface.x + 16,
        surface.y + 38,
        surface.width.saturating_sub(32),
        surface.height.saturating_sub(92),
        crate::framebuffer::Color::new(255, 255, 255),
    );
    draw_u64_line(
        screen,
        surface.x + 26,
        surface.y + 50,
        b"UPTIME  ",
        stats.uptime_ms,
        b" MS",
    );
    draw_u64_line(
        screen,
        surface.x + 26,
        surface.y + 70,
        b"RAM FREE  ",
        stats.free_mib,
        b" MIB",
    );
    draw_u64_line(
        screen,
        surface.x + 218,
        surface.y + 70,
        b"TOTAL  ",
        stats.total_mib,
        b" MIB",
    );
    draw_u64_line(
        screen,
        surface.x + 26,
        surface.y + 94,
        b"PROCESSES  LIVE ",
        stats.live as u64,
        b"",
    );
    draw_u64_line(
        screen,
        surface.x + 218,
        surface.y + 94,
        b"RUNNABLE ",
        stats.runnable as u64,
        b"",
    );
    draw_u64_line(
        screen,
        surface.x + 26,
        surface.y + 114,
        b"BLOCKED  ",
        stats.blocked as u64,
        b"",
    );
    draw_u64_line(
        screen,
        surface.x + 218,
        surface.y + 114,
        b"ZOMBIES  ",
        stats.zombies as u64,
        b"",
    );
    draw_u64_line(
        screen,
        surface.x + 26,
        surface.y + 138,
        b"CURRENT PID  ",
        stats.current_pid,
        b"",
    );
    draw_u64_line(
        screen,
        surface.x + 218,
        surface.y + 138,
        b"SWITCHES  ",
        stats.switches,
        b"",
    );
    screen.draw_text(
        surface.x + 26,
        surface.y + 163,
        1,
        "SOURCE: PMM + ROUND-ROBIN SCHEDULER",
        black,
    );
    let refresh = monitor_refresh_button_layout(surface);
    if state.pressed_button == 1 {
        draw_sunken_panel(screen, refresh.0, refresh.1, refresh.2, refresh.3);
    } else {
        draw_raised_panel(screen, refresh.0, refresh.1, refresh.2, refresh.3);
    }
    screen.draw_text(refresh.0 + 18, refresh.1 + 9, 1, "REFRESH", black);
}

fn draw_settings_contents(state: &State, screen: &mut crate::framebuffer::Screen) {
    let surface = state.surfaces[SETTINGS_SURFACE];
    if state.settings_user_open {
        draw_settings_user(state, screen, surface);
        return;
    }
    let margin = 14;
    let gap = 7;
    let card_width = surface.width.saturating_sub(margin * 2);
    let card_height = surface.height.saturating_sub(margin * 2 + gap * 3) / 4;
    let black = crate::framebuffer::Color::new(0, 0, 0);
    let headings = ["DISPLAY", "NETWORK", "INPUT", "SYSTEM"];
    let display_value = match (state.framebuffer.width, state.framebuffer.height) {
        (1024, 768) => "CURRENT 1024 X 768 / LIVE",
        (1280, 800) => "CURRENT 1280 X 800 / LIVE",
        _ => "CURRENT 800 X 600 / LIVE",
    };
    let values = [
        display_value,
        if cfg!(target_arch = "aarch64") {
            "WIRED DHCP ONLINE / WI-FI DEVICE NOT FOUND"
        } else {
            "EMULATED NETWORK / WI-FI DEVICE NOT FOUND"
        },
        "ABSOLUTE TABLET / LOW LATENCY",
        "MAKOS 0.1 / CLASSIC 95 / EL0 APP",
    ];
    for index in 0..4u32 {
        let card_x = surface.x + margin;
        let card_y = surface.y + margin + index * (card_height + gap);
        draw_sunken_panel(screen, card_x, card_y, card_width, card_height);
        screen.fill_rect(
            card_x + 4,
            card_y + 4,
            card_width.saturating_sub(8),
            card_height.saturating_sub(8),
            crate::framebuffer::Color::new(255, 255, 255),
        );
        screen.draw_text(card_x + 10, card_y + 8, 1, headings[index as usize], black);
        screen.draw_text(card_x + 10, card_y + 27, 1, values[index as usize], black);
        if index == 3 {
            let mut package_status = [0u8; 64];
            let length = crate::package::runtime_status_text(&mut package_status);
            let text = core::str::from_utf8(&package_status[..length]).unwrap_or("PACKAGES ERROR");
            screen.draw_text(card_x + 10, card_y + 43, 1, text, black);
        }
    }
    if let Some((start_x, top, button_width, height)) = settings_mode_layout(surface) {
        for (index, (mode, label)) in [
            ((800, 600), "800 X 600"),
            ((1024, 768), "1024 X 768"),
            ((1280, 800), "1280 X 800"),
        ]
        .into_iter()
        .enumerate()
        {
            let left = start_x + index as u32 * (button_width + 4);
            if mode == (state.framebuffer.width, state.framebuffer.height)
                || (state.pressed_button == 10 && state.pressed_value as usize == index)
            {
                draw_sunken_panel(screen, left, top, button_width, height);
            } else {
                draw_raised_panel(screen, left, top, button_width, height);
            }
            screen.draw_text(left + 8, top + 9, 1, label, black);
        }
    }
    let add = settings_add_user_button_layout(surface);
    if state.pressed_button == 2 {
        draw_sunken_panel(screen, add.0, add.1, add.2, add.3);
    } else {
        draw_raised_panel(screen, add.0, add.1, add.2, add.3);
    }
    screen.draw_text(add.0 + 10, add.1 + 9, 1, "ADD USER...", black);
}

fn draw_settings_user(state: &State, screen: &mut crate::framebuffer::Screen, surface: Surface) {
    let black = crate::framebuffer::Color::new(0, 0, 0);
    screen.fill_rect(
        surface.x,
        surface.y,
        surface.width,
        surface.height,
        crate::framebuffer::Color::new(192, 192, 192),
    );
    draw_raised_panel(
        screen,
        surface.x + 10,
        surface.y + 10,
        surface.width.saturating_sub(20),
        surface.height.saturating_sub(20),
    );
    screen.draw_text(surface.x + 24, surface.y + 24, 2, "ADD USER", black);
    let layout = settings_user_layout(surface);
    let labels = ["USER NAME", "PASSWORD", "CONFIRM"];
    for index in 0..3usize {
        let rect = layout[index];
        screen.draw_text(surface.x + 24, rect.1 + 10, 1, labels[index], black);
        draw_sunken_field(screen, rect.0, rect.1, rect.2, rect.3);
        if state.settings_user_field == index as u8 {
            draw_dotted_rect(
                screen,
                rect.0.saturating_sub(4),
                rect.1.saturating_sub(4),
                rect.2 + 8,
                rect.3 + 8,
                black,
            );
        }
    }
    let username =
        core::str::from_utf8(&state.settings_username[..usize::from(state.settings_username_len)])
            .unwrap_or("?");
    screen.draw_text(layout[0].0 + 8, layout[0].1 + 10, 1, username, black);
    let mut stars = [b'*'; SETTINGS_PASSWORD_BYTES];
    let password =
        core::str::from_utf8(&stars[..usize::from(state.settings_password_len)]).unwrap_or("");
    screen.draw_text(layout[1].0 + 8, layout[1].1 + 10, 1, password, black);
    stars.fill(b'*');
    let confirmation =
        core::str::from_utf8(&stars[..usize::from(state.settings_confirmation_len)]).unwrap_or("");
    screen.draw_text(layout[2].0 + 8, layout[2].1 + 10, 1, confirmation, black);
    screen.draw_text(
        surface.x + 24,
        surface.y + surface.height.saturating_sub(70),
        1,
        settings_status_text(state.settings_user_status),
        black,
    );
    for (index, label) in [(3usize, "CREATE"), (4usize, "CANCEL")] {
        let rect = layout[index];
        let pressed = state.pressed_button == if index == 3 { 3 } else { 4 };
        if pressed {
            draw_sunken_panel(screen, rect.0, rect.1, rect.2, rect.3);
        } else {
            draw_raised_panel(screen, rect.0, rect.1, rect.2, rect.3);
        }
        screen.draw_text(rect.0 + 16, rect.1 + 10, 1, label, black);
    }
}

fn draw_resize_grip(screen: &mut crate::framebuffer::Screen, surface: Surface) {
    let right = surface.x.saturating_add(surface.width).saturating_sub(4);
    let bottom = surface.y.saturating_add(surface.height).saturating_sub(4);
    let light = crate::framebuffer::Color::new(255, 255, 255);
    let shadow = crate::framebuffer::Color::new(128, 128, 128);
    for offset in [4, 8, 12] {
        let length = RESIZE_GRIP.saturating_sub(offset);
        for step in 0..length {
            screen.fill_rect(
                right.saturating_sub(step),
                bottom.saturating_sub(offset + step),
                1,
                1,
                light,
            );
            screen.fill_rect(
                right.saturating_sub(step),
                bottom.saturating_sub(offset + step + 1),
                1,
                1,
                shadow,
            );
        }
    }
}

fn draw_start_menu(state: &State, screen: &mut crate::framebuffer::Screen, taskbar_y: u32) {
    let menu_height = start_menu_height(state);
    let menu_y = taskbar_y.saturating_sub(menu_height);
    draw_raised_panel(
        screen,
        START_BUTTON_X,
        menu_y,
        START_MENU_WIDTH,
        menu_height,
    );
    let signout_y = menu_y + 8;
    if state.pressed_button == 12 {
        draw_sunken_panel(
            screen,
            START_BUTTON_X + 8,
            signout_y,
            START_MENU_WIDTH - 16,
            START_MENU_ITEM_HEIGHT,
        );
    } else {
        draw_raised_panel(
            screen,
            START_BUTTON_X + 8,
            signout_y,
            START_MENU_WIDTH - 16,
            START_MENU_ITEM_HEIGHT,
        );
    }
    screen.fill_rect(
        START_BUTTON_X + 18,
        signout_y + 8,
        20,
        20,
        crate::framebuffer::Color::new(128, 0, 0),
    );
    screen.draw_text(
        START_BUTTON_X + 48,
        signout_y + 12,
        2,
        "SIGN OUT",
        crate::framebuffer::Color::new(0, 0, 0),
    );
    let mut row = 0u32;
    for index in 0..DESKTOP_SURFACES {
        if !state.surfaces[index].created {
            continue;
        }
        let y = menu_y + 8 + (row + 1) * (START_MENU_ITEM_HEIGHT + 4);
        if state.pressed_button == 7 && state.pressed_surface == index {
            draw_sunken_panel(
                screen,
                START_BUTTON_X + 8,
                y,
                START_MENU_WIDTH - 16,
                START_MENU_ITEM_HEIGHT,
            );
        } else {
            draw_raised_panel(
                screen,
                START_BUTTON_X + 8,
                y,
                START_MENU_WIDTH - 16,
                START_MENU_ITEM_HEIGHT,
            );
        }
        screen.fill_rect(
            START_BUTTON_X + 18,
            y + 8,
            20,
            20,
            if index == TERMINAL_SURFACE {
                crate::framebuffer::Color::new(0, 0, 0)
            } else {
                crate::framebuffer::Color::new(0, 128, 128)
            },
        );
        screen.draw_text(
            START_BUTTON_X + 48,
            y + 12,
            2,
            surface_title(index),
            crate::framebuffer::Color::new(0, 0, 0),
        );
        row += 1;
    }
}

fn start_menu_height(state: &State) -> u32 {
    let items = state
        .surfaces
        .iter()
        .take(DESKTOP_SURFACES)
        .filter(|surface| surface.created)
        .count() as u32;
    14 + (items + 1) * (START_MENU_ITEM_HEIGHT + 4)
}

fn redraw_resize_outline(state: &mut State, old_cursor_x: u32, old_cursor_y: u32) {
    let Some(index) = state.resize_surface else {
        return;
    };
    let Some(mut screen) = crate::framebuffer::Screen::new(state.framebuffer) else {
        return;
    };
    restore_cursor_under(state, &mut screen, old_cursor_x, old_cursor_y);
    let surface = state.surfaces[index];
    if state.drag_outline_valid {
        xor_window_outline(
            &mut screen,
            surface.x,
            surface.y,
            state.resize_width,
            state.resize_height,
        );
    }
    let (preferred_minimum_width, preferred_minimum_height) = minimum_surface_size(index);
    let maximum_width = surface
        .backing_width
        .min(state.framebuffer.width.saturating_sub(surface.x + 4))
        .max(1);
    let maximum_height = surface
        .backing_height
        .min(
            state
                .framebuffer
                .height
                .saturating_sub(TASKBAR_HEIGHT + surface.y + 4),
        )
        .max(1);
    let minimum_width = preferred_minimum_width.min(maximum_width);
    let minimum_height = preferred_minimum_height.min(maximum_height);
    state.resize_width = (state.cursor_x as i32 - surface.x as i32 - state.resize_offset_x)
        .clamp(minimum_width as i32, maximum_width as i32) as u32;
    state.resize_height = (state.cursor_y as i32 - surface.y as i32 - state.resize_offset_y)
        .clamp(minimum_height as i32, maximum_height as i32) as u32;
    xor_window_outline(
        &mut screen,
        surface.x,
        surface.y,
        state.resize_width,
        state.resize_height,
    );
    state.drag_outline_valid = true;
    state.cursor_under_valid = false;
    capture_cursor_under(state, &screen);
    draw_cursor(&mut screen, state.cursor_x, state.cursor_y);
}

fn finish_resize(state: &mut State, old_cursor_x: u32, old_cursor_y: u32) -> (u32, u32) {
    let Some(index) = state.resize_surface else {
        return (0, 0);
    };
    if let Some(mut screen) = crate::framebuffer::Screen::new(state.framebuffer) {
        restore_cursor_under(state, &mut screen, old_cursor_x, old_cursor_y);
        if state.drag_outline_valid {
            let surface = state.surfaces[index];
            xor_window_outline(
                &mut screen,
                surface.x,
                surface.y,
                state.resize_width,
                state.resize_height,
            );
        }
    }
    state.surfaces[index].width = state.resize_width;
    state.surfaces[index].height = state.resize_height;
    if index == TERMINAL_SURFACE {
        update_terminal_window_size(state);
    }
    state.resize_surface = None;
    state.drag_outline_valid = false;
    compose(state);
    (state.resize_width, state.resize_height)
}

fn redraw_drag_outline(state: &mut State, old_cursor_x: u32, old_cursor_y: u32) {
    let Some(index) = state.drag_surface else {
        return;
    };
    let Some(mut screen) = crate::framebuffer::Screen::new(state.framebuffer) else {
        return;
    };
    restore_cursor_under(state, &mut screen, old_cursor_x, old_cursor_y);
    let surface = state.surfaces[index];
    if state.drag_outline_valid {
        xor_drag_outline(&mut screen, surface, state.drag_x, state.drag_y);
    }
    let max_x = state
        .framebuffer
        .width
        .saturating_sub(surface.width.saturating_add(4))
        .max(4);
    let max_y = state
        .framebuffer
        .height
        .saturating_sub(TASKBAR_HEIGHT + surface.height + 4)
        .max(30);
    state.drag_x = (state.cursor_x as i32 - state.drag_offset_x).clamp(4, max_x as i32) as u32;
    state.drag_y = (state.cursor_y as i32 - state.drag_offset_y).clamp(30, max_y as i32) as u32;
    xor_drag_outline(&mut screen, surface, state.drag_x, state.drag_y);
    state.drag_outline_valid = true;
    state.cursor_under_valid = false;
    capture_cursor_under(state, &screen);
    draw_cursor(&mut screen, state.cursor_x, state.cursor_y);
}

fn finish_drag(state: &mut State, old_cursor_x: u32, old_cursor_y: u32) {
    let Some(index) = state.drag_surface else {
        return;
    };
    if let Some(mut screen) = crate::framebuffer::Screen::new(state.framebuffer) {
        restore_cursor_under(state, &mut screen, old_cursor_x, old_cursor_y);
        if state.drag_outline_valid {
            xor_drag_outline(
                &mut screen,
                state.surfaces[index],
                state.drag_x,
                state.drag_y,
            );
        }
    }
    state.surfaces[index].x = state.drag_x;
    state.surfaces[index].y = state.drag_y;
    state.drag_surface = None;
    state.drag_outline_valid = false;
    compose(state);
}

fn xor_drag_outline(
    screen: &mut crate::framebuffer::Screen,
    surface: Surface,
    content_x: u32,
    content_y: u32,
) {
    xor_window_outline(screen, content_x, content_y, surface.width, surface.height);
}

fn xor_window_outline(
    screen: &mut crate::framebuffer::Screen,
    content_x: u32,
    content_y: u32,
    width: u32,
    height: u32,
) {
    let left = content_x.saturating_sub(4);
    let top = content_y.saturating_sub(30);
    let right = content_x.saturating_add(width).saturating_add(3);
    let bottom = content_y.saturating_add(height).saturating_add(3);
    for x in left..=right {
        xor_pixel(screen, x, top);
        xor_pixel(screen, x, bottom);
    }
    for y in top.saturating_add(1)..bottom {
        xor_pixel(screen, left, y);
        xor_pixel(screen, right, y);
    }
}

fn xor_pixel(screen: &mut crate::framebuffer::Screen, x: u32, y: u32) {
    if let Some(pixel) = screen.read_scene_pixel(x, y) {
        screen.write_raw_pixel(x, y, pixel ^ 0x00ff_ffff);
    }
}

#[cfg(target_arch = "x86_64")]
fn redraw_cursor(state: &mut State, old_x: u32, old_y: u32) {
    let Some(mut screen) = crate::framebuffer::Screen::new(state.framebuffer) else {
        return;
    };
    restore_cursor_under(state, &mut screen, old_x, old_y);
    capture_cursor_under(state, &screen);
    draw_cursor(&mut screen, state.cursor_x, state.cursor_y);
}

#[cfg(target_arch = "aarch64")]
fn redraw_cursor(_state: &mut State, _old_x: u32, _old_y: u32) {
    // Pointer position is submitted once after GRAPHICS is unlocked.  Keeping
    // this compositor hook empty guarantees motion never mutates scanout.
}

#[cfg(target_arch = "x86_64")]
fn restore_cursor_under(
    state: &State,
    screen: &mut crate::framebuffer::Screen,
    _requested_x: u32,
    _requested_y: u32,
) {
    if !state.cursor_under_valid {
        return;
    }
    // The saved pixels own their capture coordinates.  A compose may replace
    // the saved buffer after mouse_packet remembered an older position; using
    // that caller position would paste unrelated scene pixels into the old
    // rectangle and produce the reported dark/warped spots.
    let x = state.cursor_under_x;
    let y = state.cursor_under_y;
    for row in 0..CURSOR_HEIGHT {
        for column in 0..CURSOR_WIDTH {
            screen.write_overlay_raw_pixel(
                x.saturating_add(column as u32),
                y.saturating_add(row as u32),
                state.cursor_under[row * CURSOR_WIDTH + column],
            );
        }
    }
}

#[cfg(target_arch = "aarch64")]
fn restore_cursor_under(
    _state: &State,
    _screen: &mut crate::framebuffer::Screen,
    _x: u32,
    _y: u32,
) {
}

#[cfg(target_arch = "x86_64")]
fn capture_cursor_under(state: &mut State, screen: &crate::framebuffer::Screen) {
    state.cursor_under_x = state.cursor_x;
    state.cursor_under_y = state.cursor_y;
    for row in 0..CURSOR_HEIGHT {
        for column in 0..CURSOR_WIDTH {
            state.cursor_under[row * CURSOR_WIDTH + column] = screen
                .read_scene_pixel(
                    state.cursor_x.saturating_add(column as u32),
                    state.cursor_y.saturating_add(row as u32),
                )
                .unwrap_or(0);
        }
    }
    state.cursor_under_valid = true;
}

#[cfg(target_arch = "aarch64")]
fn capture_cursor_under(state: &mut State, _screen: &crate::framebuffer::Screen) {
    state.cursor_under_valid = false;
}

#[cfg(target_arch = "x86_64")]
fn draw_cursor(screen: &mut crate::framebuffer::Screen, x: u32, y: u32) {
    // One connected, boxed 95-style arrow. `#` is opaque border, `W` is
    // opaque fill, space is untouched scene. The cursor never samples the
    // visible framebuffer: restore data comes from the cursor-free shadow.
    const GLYPH: [&[u8; CURSOR_WIDTH]; CURSOR_HEIGHT] = [
        b"#           ",
        b"##          ",
        b"#W#         ",
        b"#WW#        ",
        b"#WWW#       ",
        b"#WWWW#      ",
        b"#WWWWW#     ",
        b"#WWWWWW#    ",
        b"#WWWWWWW#   ",
        b"#WWWWWWWW#  ",
        b"#WWWWW######",
        b"#WW#WW#     ",
        b"#W# #WW#    ",
        b"##  #WW#    ",
        b"#    #WW#   ",
        b"     #WW#   ",
        b"      ##    ",
    ];
    for (row, glyph_row) in GLYPH.iter().enumerate() {
        for (column, pixel) in glyph_row.iter().copied().enumerate() {
            let color = match pixel {
                b'#' => Some(crate::framebuffer::Color::new(0, 0, 0)),
                b'W' => Some(crate::framebuffer::Color::new(255, 255, 255)),
                _ => None,
            };
            if let Some(color) = color {
                screen.fill_overlay_rect(
                    x.saturating_add(column as u32),
                    y.saturating_add(row as u32),
                    1,
                    1,
                    color,
                );
            }
        }
    }
}

#[cfg(target_arch = "aarch64")]
fn draw_cursor(_screen: &mut crate::framebuffer::Screen, _x: u32, _y: u32) {}

fn push_surface_event(state: &mut State, index: usize, event: SurfaceEvent) {
    let head = usize::from(state.surface_event_heads[index]);
    let tail = usize::from(state.surface_event_tails[index]);
    if head != tail {
        let previous = (head + SURFACE_EVENT_QUEUE - 1) % SURFACE_EVENT_QUEUE;
        let queued = &mut state.surface_events[index][previous];
        if event.kind == 2 && queued.kind == 2 && event.key == queued.key {
            *queued = event;
            return;
        }
        if event.kind == 5 && queued.kind == 5 {
            queued.key = (queued.key as i32).saturating_add(event.key as i32) as u32;
            queued.modifiers =
                (queued.modifiers as i32).saturating_add(event.modifiers as i32) as u32;
            queued.x = event.x;
            queued.y = event.y;
            queued.width = event.width;
            queued.height = event.height;
            return;
        }
    }
    let next = (head + 1) % SURFACE_EVENT_QUEUE;
    if next == usize::from(state.surface_event_tails[index]) {
        // A producer that fills all 255 usable entries still favors the newest
        // state. Normal URLs and editor input now fit without reaching here.
        state.surface_event_tails[index] =
            ((usize::from(state.surface_event_tails[index]) + 1) % SURFACE_EVENT_QUEUE) as u8;
    }
    state.surface_events[index][head] = event;
    state.surface_event_heads[index] = next as u8;
}

fn handle_index(handle: u64) -> Option<usize> {
    let index = usize::try_from(handle).ok()?.checked_sub(1)?;
    (index < MAX_SURFACES).then_some(index)
}

fn color(argb: u32) -> crate::framebuffer::Color {
    crate::framebuffer::Color::new((argb >> 16) as u8, (argb >> 8) as u8, argb as u8)
}

fn with_lock<R>(function: impl FnOnce(&mut State) -> R) -> R {
    while GRAPHICS
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = function(unsafe { &mut *GRAPHICS.state.get() });
    GRAPHICS.lock.store(false, Ordering::Release);
    result
}
