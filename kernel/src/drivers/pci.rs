use crate::arch::{inl, outl};

const CONFIG_ADDRESS: u16 = 0xcf8;
const CONFIG_DATA: u16 = 0xcfc;

#[derive(Clone, Copy)]
pub struct Device {
    pub bus: u8,
    pub slot: u8,
    pub function: u8,
}

impl Device {
    pub fn read(&self, offset: u8) -> u32 {
        read(self.bus, self.slot, self.function, offset)
    }

    pub fn write(&self, offset: u8, value: u32) {
        write(self.bus, self.slot, self.function, offset, value)
    }

    pub fn enable_io_bus_master(&self) {
        let command_status = self.read(0x04);
        self.write(0x04, command_status | 0x5);
    }
}

pub fn find(vendor: u16, device: u16) -> Option<Device> {
    for bus in 0..=255u8 {
        for slot in 0..32u8 {
            let header = read(bus, slot, 0, 0x0c);
            let functions = if header & (1 << 23) != 0 { 8 } else { 1 };
            for function in 0..functions {
                let id = read(bus, slot, function, 0);
                if id == 0xffff_ffff {
                    continue;
                }
                if id as u16 == vendor && (id >> 16) as u16 == device {
                    return Some(Device {
                        bus,
                        slot,
                        function,
                    });
                }
            }
        }
    }
    None
}

fn read(bus: u8, slot: u8, function: u8, offset: u8) -> u32 {
    let address = config_address(bus, slot, function, offset);
    unsafe {
        outl(CONFIG_ADDRESS, address);
        inl(CONFIG_DATA)
    }
}

fn write(bus: u8, slot: u8, function: u8, offset: u8, value: u32) {
    let address = config_address(bus, slot, function, offset);
    unsafe {
        outl(CONFIG_ADDRESS, address);
        outl(CONFIG_DATA, value);
    }
}

fn config_address(bus: u8, slot: u8, function: u8, offset: u8) -> u32 {
    0x8000_0000
        | (u32::from(bus) << 16)
        | (u32::from(slot) << 11)
        | (u32::from(function) << 8)
        | u32::from(offset & 0xfc)
}
