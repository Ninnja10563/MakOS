use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use makos_boot_api::{FramebufferInfo, PixelFormat};

const SHADOW_WIDTH: usize = 1280;
const SHADOW_HEIGHT: usize = 800;
const SHADOW_PIXELS: usize = SHADOW_WIDTH * SHADOW_HEIGHT;

// Cursor-free scene copy. Normal drawing updates scanout + shadow; pointer
// overlay touches scanout only. Cursor restore therefore never depends on
// pixels captured from a prior cursor position.
static mut SCENE_SHADOW: [u32; SHADOW_PIXELS] = [0; SHADOW_PIXELS];
static SHADOW_ENABLED: AtomicBool = AtomicBool::new(false);
static SHADOW_FRAMEBUFFER: AtomicU64 = AtomicU64::new(0);
static SHADOW_WIDTH_ACTIVE: AtomicU32 = AtomicU32::new(0);
static SHADOW_HEIGHT_ACTIVE: AtomicU32 = AtomicU32::new(0);

pub fn install_scene_shadow(info: FramebufferInfo) -> bool {
    if info.address == 0
        || info.width == 0
        || info.height == 0
        || info.width as usize > SHADOW_WIDTH
        || info.height as usize > SHADOW_HEIGHT
    {
        SHADOW_ENABLED.store(false, Ordering::Release);
        return false;
    }
    SHADOW_ENABLED.store(false, Ordering::Release);
    SHADOW_FRAMEBUFFER.store(info.address, Ordering::Relaxed);
    SHADOW_WIDTH_ACTIVE.store(info.width, Ordering::Relaxed);
    SHADOW_HEIGHT_ACTIVE.store(info.height, Ordering::Relaxed);
    SHADOW_ENABLED.store(true, Ordering::Release);
    true
}

#[derive(Clone, Copy)]
pub struct Color {
    red: u8,
    green: u8,
    blue: u8,
}

impl Color {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }
}

pub struct Screen {
    info: FramebufferInfo,
    scene_shadow: bool,
}

impl Screen {
    pub fn new(info: FramebufferInfo) -> Option<Self> {
        if info.address == 0
            || info.width == 0
            || info.height == 0
            || info.stride < info.width
            || !matches!(info.pixel_format, PixelFormat::Rgb | PixelFormat::Bgr)
        {
            return None;
        }
        let required = u64::from(info.stride)
            .checked_mul(u64::from(info.height))?
            .checked_mul(4)?;
        if required > info.byte_len {
            return None;
        }
        let scene_shadow = SHADOW_ENABLED.load(Ordering::Acquire)
            && info.address == SHADOW_FRAMEBUFFER.load(Ordering::Relaxed)
            && info.width <= SHADOW_WIDTH_ACTIVE.load(Ordering::Relaxed)
            && info.height <= SHADOW_HEIGHT_ACTIVE.load(Ordering::Relaxed);
        Some(Self { info, scene_shadow })
    }

    pub const fn width(&self) -> u32 {
        self.info.width
    }

    pub const fn height(&self) -> u32 {
        self.info.height
    }

    pub fn clear(&mut self, color: Color) {
        self.fill_rect(0, 0, self.info.width, self.info.height, color);
    }

    pub fn fill_rect(&mut self, x: u32, y: u32, width: u32, height: u32, color: Color) {
        let end_x = x.saturating_add(width).min(self.info.width);
        let end_y = y.saturating_add(height).min(self.info.height);
        for py in y.min(end_y)..end_y {
            for px in x.min(end_x)..end_x {
                self.pixel(px, py, color);
            }
        }
    }

    pub fn draw_text(&mut self, mut x: u32, y: u32, scale: u32, text: &str, color: Color) {
        for character in text.bytes() {
            if character == b' ' {
                x = x.saturating_add(4 * scale);
                continue;
            }
            let glyph = glyph(character);
            for (row, bits) in glyph.iter().copied().enumerate() {
                for column in 0..5u32 {
                    if bits & (1 << (4 - column)) != 0 {
                        self.fill_rect(
                            x + column * scale,
                            y + row as u32 * scale,
                            scale,
                            scale,
                            color,
                        );
                    }
                }
            }
            x = x.saturating_add(6 * scale);
        }
    }

    pub fn read_raw_pixel(&self, x: u32, y: u32) -> Option<u32> {
        if x >= self.info.width || y >= self.info.height {
            return None;
        }
        let pixel_index = u64::from(y) * u64::from(self.info.stride) + u64::from(x);
        let address = (self.info.address + pixel_index * 4) as *const u32;
        Some(unsafe { ptr::read_volatile(address) })
    }

    pub fn write_raw_pixel(&mut self, x: u32, y: u32, value: u32) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }
        let pixel_index = u64::from(y) * u64::from(self.info.stride) + u64::from(x);
        let address = (self.info.address + pixel_index * 4) as *mut u32;
        unsafe { ptr::write_volatile(address, value) };
        self.write_shadow_pixel(x, y, value);
    }

    pub fn read_scene_pixel(&self, x: u32, y: u32) -> Option<u32> {
        if !self.shadow_matches(x, y) {
            return self.read_raw_pixel(x, y);
        }
        let index = y as usize * SHADOW_WIDTH + x as usize;
        Some(unsafe { ptr::read_volatile((&raw const SCENE_SHADOW).cast::<u32>().add(index)) })
    }

    pub fn write_overlay_raw_pixel(&mut self, x: u32, y: u32, value: u32) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }
        let pixel_index = u64::from(y) * u64::from(self.info.stride) + u64::from(x);
        let address = (self.info.address + pixel_index * 4) as *mut u32;
        unsafe { ptr::write_volatile(address, value) };
    }

    pub fn fill_overlay_rect(&mut self, x: u32, y: u32, width: u32, height: u32, color: Color) {
        let end_x = x.saturating_add(width).min(self.info.width);
        let end_y = y.saturating_add(height).min(self.info.height);
        let raw = self.raw_color(color);
        for py in y.min(end_y)..end_y {
            for px in x.min(end_x)..end_x {
                self.write_overlay_raw_pixel(px, py, raw);
            }
        }
    }

    fn pixel(&mut self, x: u32, y: u32, color: Color) {
        let pixel_index = u64::from(y) * u64::from(self.info.stride) + u64::from(x);
        let address = (self.info.address + pixel_index * 4) as *mut u8;
        let (first, third) = match self.info.pixel_format {
            PixelFormat::Rgb => (color.red, color.blue),
            PixelFormat::Bgr => (color.blue, color.red),
            _ => return,
        };
        unsafe {
            ptr::write_volatile(address, first);
            ptr::write_volatile(address.add(1), color.green);
            ptr::write_volatile(address.add(2), third);
            ptr::write_volatile(address.add(3), 0);
        }
        self.write_shadow_pixel(x, y, self.raw_color(color));
    }

    fn raw_color(&self, color: Color) -> u32 {
        match self.info.pixel_format {
            PixelFormat::Rgb => {
                u32::from(color.red) | (u32::from(color.green) << 8) | (u32::from(color.blue) << 16)
            }
            PixelFormat::Bgr => {
                u32::from(color.blue) | (u32::from(color.green) << 8) | (u32::from(color.red) << 16)
            }
            _ => 0,
        }
    }

    fn shadow_matches(&self, x: u32, y: u32) -> bool {
        self.scene_shadow && x < self.info.width && y < self.info.height
    }

    fn write_shadow_pixel(&self, x: u32, y: u32, value: u32) {
        if !self.shadow_matches(x, y) {
            return;
        }
        let index = y as usize * SHADOW_WIDTH + x as usize;
        unsafe { ptr::write_volatile((&raw mut SCENE_SHADOW).cast::<u32>().add(index), value) };
    }
}

pub(crate) fn glyph(c: u8) -> [u8; 7] {
    match c {
        b'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        b'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        b'C' => [0x0f, 0x10, 0x10, 0x10, 0x10, 0x10, 0x0f],
        b'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        b'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        b'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        b'G' => [0x0f, 0x10, 0x10, 0x13, 0x11, 0x11, 0x0f],
        b'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        b'I' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1f],
        b'J' => [0x01, 0x01, 0x01, 0x01, 0x11, 0x11, 0x0e],
        b'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        b'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        b'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        b'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        b'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        b'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        b'Q' => [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
        b'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        b'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        b'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        b'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        b'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04],
        b'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a],
        b'X' => [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
        b'Y' => [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
        b'Z' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
        b'a' => [0, 0, 0x0e, 0x01, 0x0f, 0x11, 0x0f],
        b'b' => [0x10, 0x10, 0x1e, 0x11, 0x11, 0x11, 0x1e],
        b'c' => [0, 0, 0x0f, 0x10, 0x10, 0x10, 0x0f],
        b'd' => [0x01, 0x01, 0x0f, 0x11, 0x11, 0x11, 0x0f],
        b'e' => [0, 0, 0x0e, 0x11, 0x1f, 0x10, 0x0f],
        b'f' => [0x06, 0x08, 0x1e, 0x08, 0x08, 0x08, 0x08],
        b'g' => [0, 0, 0x0f, 0x11, 0x0f, 0x01, 0x0e],
        b'h' => [0x10, 0x10, 0x1e, 0x11, 0x11, 0x11, 0x11],
        b'i' => [0x04, 0, 0x0c, 0x04, 0x04, 0x04, 0x0e],
        b'j' => [0x02, 0, 0x06, 0x02, 0x02, 0x12, 0x0c],
        b'k' => [0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12],
        b'l' => [0x0c, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e],
        b'm' => [0, 0, 0x1a, 0x15, 0x15, 0x15, 0x15],
        b'n' => [0, 0, 0x1e, 0x11, 0x11, 0x11, 0x11],
        b'o' => [0, 0, 0x0e, 0x11, 0x11, 0x11, 0x0e],
        b'p' => [0, 0, 0x1e, 0x11, 0x1e, 0x10, 0x10],
        b'q' => [0, 0, 0x0f, 0x11, 0x0f, 0x01, 0x01],
        b'r' => [0, 0, 0x16, 0x19, 0x10, 0x10, 0x10],
        b's' => [0, 0, 0x0f, 0x10, 0x0e, 0x01, 0x1e],
        b't' => [0x08, 0x08, 0x1e, 0x08, 0x08, 0x09, 0x06],
        b'u' => [0, 0, 0x11, 0x11, 0x11, 0x13, 0x0d],
        b'v' => [0, 0, 0x11, 0x11, 0x11, 0x0a, 0x04],
        b'w' => [0, 0, 0x11, 0x11, 0x15, 0x15, 0x0a],
        b'x' => [0, 0, 0x11, 0x0a, 0x04, 0x0a, 0x11],
        b'y' => [0, 0, 0x11, 0x11, 0x0f, 0x01, 0x0e],
        b'z' => [0, 0, 0x1f, 0x02, 0x04, 0x08, 0x1f],
        b'0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        b'1' => [0x04, 0x0c, 0x14, 0x04, 0x04, 0x04, 0x1f],
        b'2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        b'3' => [0x1e, 0x01, 0x01, 0x0e, 0x01, 0x01, 0x1e],
        b'5' => [0x1f, 0x10, 0x10, 0x1e, 0x01, 0x01, 0x1e],
        b'_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1f],
        b'6' => [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        b'7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        b'8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        b'9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c],
        b'4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        b':' => [0, 0x04, 0x04, 0, 0x04, 0x04, 0],
        b'-' => [0, 0, 0, 0x1f, 0, 0, 0],
        b'.' => [0, 0, 0, 0, 0, 0x04, 0x04],
        b',' => [0, 0, 0, 0, 0, 0x04, 0x08],
        b'*' => [0, 0x15, 0x0e, 0x1f, 0x0e, 0x15, 0],
        b'$' => [0x04, 0x0f, 0x14, 0x0e, 0x05, 0x1e, 0x04],
        b'/' => [0x01, 0x02, 0x02, 0x04, 0x08, 0x08, 0x10],
        b'=' => [0, 0x1f, 0, 0x1f, 0, 0, 0],
        b'[' => [0x0e, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0e],
        b']' => [0x0e, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0e],
        b'(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        b')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        b'+' => [0, 0x04, 0x04, 0x1f, 0x04, 0x04, 0],
        b'<' => [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02],
        b'>' => [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08],
        b'?' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0, 0x04],
        b'!' => [0x04, 0x04, 0x04, 0x04, 0x04, 0, 0x04],
        b'@' => [0x0e, 0x11, 0x17, 0x15, 0x17, 0x10, 0x0e],
        b'#' => [0x0a, 0x1f, 0x0a, 0x0a, 0x1f, 0x0a, 0x0a],
        b'%' => [0x19, 0x1a, 0x02, 0x04, 0x08, 0x0b, 0x13],
        b'^' => [0x04, 0x0a, 0x11, 0, 0, 0, 0],
        b'&' => [0x0c, 0x12, 0x14, 0x08, 0x15, 0x12, 0x0d],
        b';' => [0, 0x04, 0x04, 0, 0x04, 0x04, 0x08],
        b'\'' => [0x04, 0x04, 0x08, 0, 0, 0, 0],
        b'\"' => [0x0a, 0x0a, 0x14, 0, 0, 0, 0],
        b'`' => [0x08, 0x04, 0x02, 0, 0, 0, 0],
        b'~' => [0, 0, 0x09, 0x16, 0, 0, 0],
        b'\\' => [0x10, 0x08, 0x08, 0x04, 0x02, 0x02, 0x01],
        b'|' => [0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        b'{' => [0x02, 0x04, 0x04, 0x08, 0x04, 0x04, 0x02],
        b'}' => [0x08, 0x04, 0x04, 0x02, 0x04, 0x04, 0x08],
        _ => [0x1f, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
    }
}
