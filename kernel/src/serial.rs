#[cfg(target_arch = "x86_64")]
use core::arch::asm;
use core::fmt::{self, Write};

#[cfg(target_arch = "x86_64")]
const COM1: u16 = 0x3f8;
#[cfg(target_arch = "aarch64")]
const PL011_BASE: usize = 0x0900_0000;

pub fn init() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x80);
        outb(COM1, 0x03);
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03);
        outb(COM1 + 2, 0xc7);
        outb(COM1 + 4, 0x0b);
    }
}

pub fn print(args: fmt::Arguments<'_>) {
    let _ = Serial.write_fmt(args);
}

pub fn write_bytes(bytes: &[u8]) {
    let mut serial = Serial;
    for &byte in bytes {
        if byte == b'\n' {
            serial.write_byte(b'\r');
        }
        serial.write_byte(byte);
    }
}

struct Serial;

impl Write for Serial {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
        Ok(())
    }
}

impl Serial {
    fn write_byte(&mut self, byte: u8) {
        #[cfg(target_arch = "aarch64")]
        unsafe {
            let flags = (PL011_BASE + 0x18) as *const u32;
            let data = PL011_BASE as *mut u32;
            while core::ptr::read_volatile(flags) & (1 << 5) != 0 {
                core::hint::spin_loop();
            }
            core::ptr::write_volatile(data, u32::from(byte));
        }
        #[cfg(target_arch = "x86_64")]
        for _ in 0..100_000 {
            if unsafe { inb(COM1 + 5) } & 0x20 != 0 {
                unsafe { outb(COM1, byte) };
                return;
            }
            core::hint::spin_loop();
        }
    }
}

#[inline]
#[cfg(target_arch = "x86_64")]
unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
#[cfg(target_arch = "x86_64")]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
    }
    value
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => ($crate::serial::print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(concat!($fmt, "\n"), $($arg)*));
}
