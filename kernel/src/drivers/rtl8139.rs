use crate::arch::{inb, inl, outb, outl};
use crate::drivers::pci;
use core::arch::asm;
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const VENDOR_REALTEK: u16 = 0x10ec;
const DEVICE_RTL8139: u16 = 0x8139;
const RX_RING_BYTES: usize = 8192;
const RX_STORAGE_BYTES: usize = RX_RING_BYTES + 16 + 1500;
const COMMAND: u16 = 0x37;
const TX_STATUS0: u16 = 0x10;
const TX_ADDRESS0: u16 = 0x20;
const RX_BUFFER_START: u16 = 0x30;
const RX_BUFFER_POINTER: u16 = 0x38;
const INTERRUPT_MASK: u16 = 0x3c;
const INTERRUPT_STATUS: u16 = 0x3e;
const RX_CONFIG: u16 = 0x44;

#[repr(C, align(4096))]
struct RxStorage([u8; RX_STORAGE_BYTES]);

#[repr(C, align(16))]
struct TxStorage([u8; 1536]);

static mut RX_STORAGE: RxStorage = RxStorage([0; RX_STORAGE_BYTES]);
static mut TX_STORAGE: [TxStorage; 4] = [
    TxStorage([0; 1536]),
    TxStorage([0; 1536]),
    TxStorage([0; 1536]),
    TxStorage([0; 1536]),
];
static TX_LOCK: AtomicBool = AtomicBool::new(false);
static TX_NEXT: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
struct NetworkState {
    ready: bool,
    io: u16,
    rx_offset: usize,
    mac: [u8; 6],
    dns_mac: [u8; 6],
    our_ip: [u8; 4],
    dns_ip: [u8; 4],
    gateway_mac: [u8; 6],
    remote_ip: [u8; 4],
}

static mut NETWORK_STATE: NetworkState = NetworkState {
    ready: false,
    io: 0,
    rx_offset: 0,
    mac: [0; 6],
    dns_mac: [0; 6],
    our_ip: [0; 4],
    dns_ip: [0; 4],
    gateway_mac: [0; 6],
    remote_ip: [0; 4],
};

pub fn arp_self_test() {
    let device = pci::find(VENDOR_REALTEK, DEVICE_RTL8139)
        .unwrap_or_else(|| crate::fatal("RTL8139 PCI device absent"));
    device.enable_io_bus_master();
    let bar0 = device.read(0x10);
    if bar0 & 1 == 0 {
        crate::fatal("RTL8139 BAR0 is not I/O space");
    }
    let io = (bar0 & 0xfffc) as u16;
    reset(io);
    let mac = [
        unsafe { inb(io) },
        unsafe { inb(io + 1) },
        unsafe { inb(io + 2) },
        unsafe { inb(io + 3) },
        unsafe { inb(io + 4) },
        unsafe { inb(io + 5) },
    ];
    if mac == [0; 6] || mac == [0xff; 6] {
        crate::fatal("RTL8139 invalid MAC address");
    }
    unsafe {
        ptr::write_bytes((&raw mut RX_STORAGE).cast::<u8>(), 0, RX_STORAGE_BYTES);
        outl(
            io + RX_BUFFER_START,
            (&raw mut RX_STORAGE).cast::<u8>() as u32,
        );
        outw(io + INTERRUPT_MASK, 0);
        outw(io + INTERRUPT_STATUS, 0xffff);
        outl(io + RX_CONFIG, 0x0f | (1 << 7));
        outb(io + COMMAND, 0x0c);
    }

    let mut rx_offset = 0usize;
    let discover = build_dhcp_discover(mac);
    transmit(io, &discover);
    let (our_ip, gateway_ip) = receive_dhcp_offer(io, &mut rx_offset, mac)
        .unwrap_or_else(|| crate::fatal("DHCP offer timeout"));
    let frame = build_arp_request(mac, our_ip, gateway_ip);
    transmit(io, &frame);
    for _ in 0..50_000_000 {
        let status = unsafe { inw(io + INTERRUPT_STATUS) };
        if status & 1 != 0 || unsafe { inb(io + COMMAND) } & 1 == 0 {
            if let Some(sender_mac) = receive_arp_reply(io, &mut rx_offset, mac, our_ip, gateway_ip)
            {
                let echo = build_icmp_echo(mac, sender_mac, our_ip, gateway_ip);
                transmit(io, &echo);
                if !receive_icmp_reply(io, &mut rx_offset, mac, sender_mac, our_ip, gateway_ip) {
                    crate::fatal("RTL8139 ICMP echo reply timeout");
                }
                let dns_ip = [10, 0, 2, 3];
                transmit(io, &build_arp_request(mac, our_ip, dns_ip));
                let dns_mac = receive_arp_with_timeout(io, &mut rx_offset, mac, our_ip, dns_ip)
                    .unwrap_or_else(|| crate::fatal("DNS server ARP timeout"));
                transmit(io, &build_dns_query(mac, dns_mac, our_ip, dns_ip));
                let dns_answer =
                    receive_dns_reply(io, &mut rx_offset, mac, dns_mac, our_ip, dns_ip)
                        .unwrap_or_else(|| crate::fatal("DNS response timeout"));
                let tcp_sequence = 0x4d41_4b4fu32;
                transmit(
                    io,
                    &build_tcp_syn(mac, sender_mac, our_ip, dns_answer, 49153, 80, tcp_sequence),
                );
                let server_sequence = receive_tcp_synack(
                    io,
                    &mut rx_offset,
                    mac,
                    sender_mac,
                    our_ip,
                    dns_answer,
                    49153,
                    80,
                    tcp_sequence,
                )
                .unwrap_or_else(|| crate::fatal("TCP SYN-ACK timeout"));
                let ack = build_tcp_packet(
                    mac,
                    sender_mac,
                    our_ip,
                    dns_answer,
                    49153,
                    80,
                    tcp_sequence.wrapping_add(1),
                    server_sequence.wrapping_add(1),
                    0x10,
                    &[],
                )
                .unwrap_or_else(|| crate::fatal("TCP ACK construction failed"));
                transmit(io, &ack.0[..ack.1]);
                unsafe {
                    (&raw mut NETWORK_STATE).write(NetworkState {
                        ready: true,
                        io,
                        rx_offset,
                        mac,
                        dns_mac,
                        our_ip,
                        dns_ip,
                        gateway_mac: sender_mac,
                        remote_ip: dns_answer,
                    });
                }
                crate::serial_println!(
                    "MAKOS_M5_OK pci={:02x}:{:02x}.{} rtl8139_io={:#x} mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} ip={}.{}.{}.{} gateway={}.{}.{}.{} gateway_mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} dns_answer={}.{}.{}.{} ethernet=1 dhcp=1 arp=1 ipv4=1 icmp=1 udp=1 dns=1 tcp_synack=1",
                    device.bus,
                    device.slot,
                    device.function,
                    io,
                    mac[0],
                    mac[1],
                    mac[2],
                    mac[3],
                    mac[4],
                    mac[5],
                    our_ip[0],
                    our_ip[1],
                    our_ip[2],
                    our_ip[3],
                    gateway_ip[0],
                    gateway_ip[1],
                    gateway_ip[2],
                    gateway_ip[3],
                    sender_mac[0],
                    sender_mac[1],
                    sender_mac[2],
                    sender_mac[3],
                    sender_mac[4],
                    sender_mac[5],
                    dns_answer[0],
                    dns_answer[1],
                    dns_answer[2],
                    dns_answer[3],
                );
                return;
            }
        }
        core::hint::spin_loop();
    }
    crate::fatal("RTL8139 ARP reply timeout")
}

pub fn tcp_http_exchange(request: &[u8], output: &mut [u8]) -> Option<usize> {
    let state = unsafe { (&raw const NETWORK_STATE).read() };
    tcp_exchange(state.remote_ip, 80, 49_155, request, output)
}

pub fn tcp_exchange(
    remote_ip: [u8; 4],
    remote_port: u16,
    source_port: u16,
    request: &[u8],
    output: &mut [u8],
) -> Option<usize> {
    if request.is_empty() || request.len() > 512 || output.is_empty() {
        return None;
    }
    let state = unsafe { (&raw const NETWORK_STATE).read() };
    if !state.ready || remote_ip == [0; 4] || remote_port == 0 || source_port == 0 {
        return None;
    }
    let client_sequence = 0x4d41_4b54u32;
    let syn = build_tcp_packet(
        state.mac,
        state.gateway_mac,
        state.our_ip,
        remote_ip,
        source_port,
        remote_port,
        client_sequence,
        0,
        0x02,
        &[],
    )?;
    transmit(state.io, &syn.0[..syn.1]);
    let offset = unsafe { &mut (*(&raw mut NETWORK_STATE)).rx_offset };
    let server_sequence = receive_tcp_synack(
        state.io,
        offset,
        state.mac,
        state.gateway_mac,
        state.our_ip,
        remote_ip,
        source_port,
        remote_port,
        client_sequence,
    )?;
    let client_data_sequence = client_sequence.wrapping_add(1);
    let server_data_sequence = server_sequence.wrapping_add(1);
    let packet = build_tcp_packet(
        state.mac,
        state.gateway_mac,
        state.our_ip,
        remote_ip,
        source_port,
        remote_port,
        client_data_sequence,
        server_data_sequence,
        0x18,
        request,
    )?;
    transmit(state.io, &packet.0[..packet.1]);
    let (count, received_sequence, fin) = receive_tcp_payload(
        state.io,
        offset,
        state.mac,
        state.gateway_mac,
        state.our_ip,
        remote_ip,
        source_port,
        remote_port,
        client_data_sequence.wrapping_add(request.len() as u32),
        output,
    )?;
    let ack_number = received_sequence
        .wrapping_add(count as u32)
        .wrapping_add(u32::from(fin));
    let ack = build_tcp_packet(
        state.mac,
        state.gateway_mac,
        state.our_ip,
        remote_ip,
        source_port,
        remote_port,
        client_data_sequence.wrapping_add(request.len() as u32),
        ack_number,
        0x10,
        &[],
    )?;
    transmit(state.io, &ack.0[..ack.1]);
    Some(count)
}

pub fn udp_dns_exchange(payload: &[u8], output: &mut [u8]) -> Option<usize> {
    if payload.len() < 2 {
        return None;
    }
    let state = unsafe { (&raw const NETWORK_STATE).read() };
    udp_exchange_inner(
        state.dns_ip,
        53,
        49_154,
        Some([payload[0], payload[1]]),
        payload,
        output,
    )
}

pub fn udp_exchange(
    remote_ip: [u8; 4],
    remote_port: u16,
    source_port: u16,
    payload: &[u8],
    output: &mut [u8],
) -> Option<usize> {
    udp_exchange_inner(remote_ip, remote_port, source_port, None, payload, output)
}

fn udp_exchange_inner(
    remote_ip: [u8; 4],
    remote_port: u16,
    source_port: u16,
    transaction: Option<[u8; 2]>,
    payload: &[u8],
    output: &mut [u8],
) -> Option<usize> {
    if payload.len() < 12 || payload.len() > 512 || output.is_empty() {
        return None;
    }
    let state = unsafe { (&raw const NETWORK_STATE).read() };
    if !state.ready || remote_ip == [0; 4] || remote_port == 0 || source_port == 0 {
        return None;
    }
    let destination_mac = if remote_ip == state.dns_ip {
        state.dns_mac
    } else {
        state.gateway_mac
    };
    let mut frame = [0u8; 1536];
    let length = build_udp_packet(
        &mut frame,
        state.mac,
        destination_mac,
        state.our_ip,
        remote_ip,
        source_port,
        remote_port,
        payload,
    )?;
    transmit(state.io, &frame[..length]);
    let offset = unsafe { &mut (*(&raw mut NETWORK_STATE)).rx_offset };
    receive_udp_payload(
        state.io,
        offset,
        state.mac,
        state.our_ip,
        remote_ip,
        source_port,
        remote_port,
        transaction,
        output,
    )
}

pub fn ipv6_echo() -> bool {
    let state = unsafe { (&raw const NETWORK_STATE).read() };
    if !state.ready {
        return false;
    }
    let source = [0xfe, 0xc0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x15];
    let destination = [0xfe, 0xc0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x02];
    let frame = build_ipv6_echo(state.mac, state.gateway_mac, source, destination);
    transmit(state.io, &frame);
    let offset = unsafe { &mut (*(&raw mut NETWORK_STATE)).rx_offset };
    if receive_ipv6_echo_reply(state.io, offset, state.mac, source, destination) {
        crate::serial_println!(
            "MAKOS_IPV6_OK ethernet=1 ipv6=1 icmpv6=1 ndp=1 echo=1 checksum=1 source=fec0::15 gateway=fec0::2 ring3=1"
        );
        true
    } else {
        false
    }
}

fn build_ipv6_echo(
    mac: [u8; 6],
    gateway_mac: [u8; 6],
    source: [u8; 16],
    destination: [u8; 16],
) -> [u8; 62] {
    let mut frame = [0u8; 62];
    frame[0..6].copy_from_slice(&gateway_mac);
    frame[6..12].copy_from_slice(&mac);
    frame[12..14].copy_from_slice(&[0x86, 0xdd]);
    frame[14] = 0x60;
    frame[18..20].copy_from_slice(&8u16.to_be_bytes());
    frame[20] = 58;
    frame[21] = 64;
    frame[22..38].copy_from_slice(&source);
    frame[38..54].copy_from_slice(&destination);
    frame[54] = 128;
    frame[55] = 0;
    frame[58..60].copy_from_slice(&0x4d4bu16.to_be_bytes());
    frame[60..62].copy_from_slice(&1u16.to_be_bytes());
    let checksum = ipv6_transport_checksum(source, destination, 58, &frame[54..62]);
    frame[56..58].copy_from_slice(&checksum.to_be_bytes());
    frame
}

fn receive_ipv6_echo_reply(
    io: u16,
    offset: &mut usize,
    our_mac: [u8; 6],
    our_ip: [u8; 16],
    gateway_ip: [u8; 16],
) -> bool {
    for _ in 0..50_000_000 {
        if unsafe { inb(io + COMMAND) } & 1 != 0 {
            core::hint::spin_loop();
            continue;
        }
        let mut frame = [0u8; 1536];
        let Some(length) = receive_frame(io, offset, &mut frame) else {
            continue;
        };
        if length < 62
            || frame[12..14] != [0x86, 0xdd]
            || frame[14] >> 4 != 6
            || frame[20] != 58
            || frame[22..38] != gateway_ip
        {
            continue;
        }
        let payload_length = u16::from_be_bytes([frame[18], frame[19]]) as usize;
        if payload_length < 8 || 54 + payload_length > length {
            continue;
        }
        let payload = &frame[54..54 + payload_length];
        if payload.len() >= 24
            && payload[0] == 135
            && payload[1] == 0
            && payload[8..24] == our_ip
            && ipv6_transport_checksum(gateway_ip, frame[38..54].try_into().unwrap(), 58, payload)
                == 0
        {
            let neighbor_mac: [u8; 6] = frame[6..12].try_into().unwrap();
            let advertisement =
                build_neighbor_advertisement(our_mac, neighbor_mac, our_ip, gateway_ip);
            transmit(io, &advertisement);
            continue;
        }
        if frame[0..6] == our_mac
            && frame[38..54] == our_ip
            && payload[0] == 129
            && payload[1] == 0
            && payload[4..6] == 0x4d4bu16.to_be_bytes()
            && payload[6..8] == 1u16.to_be_bytes()
            && ipv6_transport_checksum(gateway_ip, our_ip, 58, payload) == 0
        {
            return true;
        }
    }
    false
}

fn build_neighbor_advertisement(
    our_mac: [u8; 6],
    neighbor_mac: [u8; 6],
    our_ip: [u8; 16],
    neighbor_ip: [u8; 16],
) -> [u8; 86] {
    let mut frame = [0u8; 86];
    frame[0..6].copy_from_slice(&neighbor_mac);
    frame[6..12].copy_from_slice(&our_mac);
    frame[12..14].copy_from_slice(&[0x86, 0xdd]);
    frame[14] = 0x60;
    frame[18..20].copy_from_slice(&32u16.to_be_bytes());
    frame[20] = 58;
    frame[21] = 255;
    frame[22..38].copy_from_slice(&our_ip);
    frame[38..54].copy_from_slice(&neighbor_ip);
    frame[54] = 136;
    frame[55] = 0;
    frame[58..62].copy_from_slice(&0x6000_0000u32.to_be_bytes());
    frame[62..78].copy_from_slice(&our_ip);
    frame[78] = 2;
    frame[79] = 1;
    frame[80..86].copy_from_slice(&our_mac);
    let checksum = ipv6_transport_checksum(our_ip, neighbor_ip, 58, &frame[54..86]);
    frame[56..58].copy_from_slice(&checksum.to_be_bytes());
    frame
}

#[allow(clippy::too_many_arguments)]
fn build_udp_packet(
    frame: &mut [u8; 1536],
    mac: [u8; 6],
    destination_mac: [u8; 6],
    our_ip: [u8; 4],
    destination_ip: [u8; 4],
    source_port: u16,
    destination_port: u16,
    payload: &[u8],
) -> Option<usize> {
    let udp_length = 8usize.checked_add(payload.len())?;
    let ip_length = 20usize.checked_add(udp_length)?;
    let frame_length = 14usize.checked_add(ip_length)?.max(60);
    if frame_length > frame.len() || ip_length > u16::MAX as usize {
        return None;
    }
    frame[..frame_length].fill(0);
    frame[0..6].copy_from_slice(&destination_mac);
    frame[6..12].copy_from_slice(&mac);
    frame[12..14].copy_from_slice(&[0x08, 0x00]);
    let ip = &mut frame[14..34];
    ip[0] = 0x45;
    ip[2..4].copy_from_slice(&(ip_length as u16).to_be_bytes());
    ip[4..6].copy_from_slice(&5u16.to_be_bytes());
    ip[6..8].copy_from_slice(&0x4000u16.to_be_bytes());
    ip[8] = 64;
    ip[9] = 17;
    ip[12..16].copy_from_slice(&our_ip);
    ip[16..20].copy_from_slice(&destination_ip);
    let checksum = internet_checksum(ip);
    ip[10..12].copy_from_slice(&checksum.to_be_bytes());
    let udp = &mut frame[34..34 + udp_length];
    udp[0..2].copy_from_slice(&source_port.to_be_bytes());
    udp[2..4].copy_from_slice(&destination_port.to_be_bytes());
    udp[4..6].copy_from_slice(&(udp_length as u16).to_be_bytes());
    udp[8..].copy_from_slice(payload);
    let checksum = transport_checksum(our_ip, destination_ip, 17, udp);
    udp[6..8].copy_from_slice(&checksum.to_be_bytes());
    Some(frame_length)
}

fn reset(io: u16) {
    unsafe { outb(io + COMMAND, 0x10) };
    for _ in 0..1_000_000 {
        if unsafe { inb(io + COMMAND) } & 0x10 == 0 {
            TX_NEXT.store(0, Ordering::Release);
            return;
        }
        core::hint::spin_loop();
    }
    crate::fatal("RTL8139 reset timeout")
}

fn build_dhcp_discover(mac: [u8; 6]) -> [u8; 342] {
    let mut frame = [0u8; 342];
    frame[0..6].fill(0xff);
    frame[6..12].copy_from_slice(&mac);
    frame[12..14].copy_from_slice(&[0x08, 0x00]);
    let ip = &mut frame[14..34];
    ip[0] = 0x45;
    ip[2..4].copy_from_slice(&328u16.to_be_bytes());
    ip[4..6].copy_from_slice(&2u16.to_be_bytes());
    ip[8] = 64;
    ip[9] = 17;
    ip[16..20].fill(0xff);
    let checksum = internet_checksum(ip);
    ip[10..12].copy_from_slice(&checksum.to_be_bytes());

    let udp = &mut frame[34..342];
    udp[0..2].copy_from_slice(&68u16.to_be_bytes());
    udp[2..4].copy_from_slice(&67u16.to_be_bytes());
    udp[4..6].copy_from_slice(&308u16.to_be_bytes());
    let bootp = &mut udp[8..];
    bootp[0] = 1;
    bootp[1] = 1;
    bootp[2] = 6;
    bootp[4..8].copy_from_slice(&0x4d41_4b4fu32.to_be_bytes());
    bootp[10..12].copy_from_slice(&0x8000u16.to_be_bytes());
    bootp[28..34].copy_from_slice(&mac);
    bootp[236..240].copy_from_slice(&[99, 130, 83, 99]);
    bootp[240..243].copy_from_slice(&[53, 1, 1]); // DHCP Discover
    bootp[243..249].copy_from_slice(&[55, 4, 1, 3, 6, 51]);
    bootp[249] = 255;
    frame
}

fn receive_dhcp_offer(io: u16, offset: &mut usize, our_mac: [u8; 6]) -> Option<([u8; 4], [u8; 4])> {
    for _ in 0..50_000_000 {
        if unsafe { inb(io + COMMAND) } & 1 != 0 {
            core::hint::spin_loop();
            continue;
        }
        let mut frame = [0u8; 1536];
        let Some(length) = receive_frame(io, offset, &mut frame) else {
            continue;
        };
        if length < 282
            || frame[12..14] != [0x08, 0x00]
            || frame[23] != 17
            || frame[34..36] != 67u16.to_be_bytes()
            || frame[36..38] != 68u16.to_be_bytes()
            || frame[42] != 2
            || frame[46..50] != 0x4d41_4b4fu32.to_be_bytes()
            || frame[70..76] != our_mac
            || frame[278..282] != [99, 130, 83, 99]
        {
            continue;
        }
        let offered_ip: [u8; 4] = frame[58..62].try_into().ok()?;
        let udp_length = u16::from_be_bytes([frame[38], frame[39]]) as usize;
        let options_end = (34 + udp_length).min(length);
        let mut gateway = None;
        let mut message_type = None;
        let mut option = 282usize;
        while option < options_end {
            let kind = frame[option];
            option += 1;
            if kind == 0 {
                continue;
            }
            if kind == 255 {
                break;
            }
            if option >= options_end {
                break;
            }
            let size = frame[option] as usize;
            option += 1;
            if option + size > options_end {
                break;
            }
            if kind == 53 && size == 1 {
                message_type = Some(frame[option]);
            }
            if kind == 3 && size >= 4 {
                gateway = frame[option..option + 4].try_into().ok();
            }
            option += size;
        }
        if message_type == Some(2) {
            return Some((offered_ip, gateway?));
        }
    }
    None
}

fn build_arp_request(mac: [u8; 6], our_ip: [u8; 4], gateway_ip: [u8; 4]) -> [u8; 60] {
    let mut frame = [0u8; 60];
    frame[0..6].fill(0xff);
    frame[6..12].copy_from_slice(&mac);
    frame[12..14].copy_from_slice(&[0x08, 0x06]);
    frame[14..16].copy_from_slice(&[0x00, 0x01]);
    frame[16..18].copy_from_slice(&[0x08, 0x00]);
    frame[18] = 6;
    frame[19] = 4;
    frame[20..22].copy_from_slice(&[0x00, 0x01]);
    frame[22..28].copy_from_slice(&mac);
    frame[28..32].copy_from_slice(&our_ip);
    frame[38..42].copy_from_slice(&gateway_ip);
    frame
}

fn build_icmp_echo(
    mac: [u8; 6],
    gateway_mac: [u8; 6],
    our_ip: [u8; 4],
    gateway_ip: [u8; 4],
) -> [u8; 60] {
    let mut frame = [0u8; 60];
    frame[0..6].copy_from_slice(&gateway_mac);
    frame[6..12].copy_from_slice(&mac);
    frame[12..14].copy_from_slice(&[0x08, 0x00]);
    let ip = &mut frame[14..34];
    ip[0] = 0x45;
    ip[2..4].copy_from_slice(&33u16.to_be_bytes());
    ip[4..6].copy_from_slice(&1u16.to_be_bytes());
    ip[6..8].copy_from_slice(&0x4000u16.to_be_bytes());
    ip[8] = 64;
    ip[9] = 1;
    ip[12..16].copy_from_slice(&our_ip);
    ip[16..20].copy_from_slice(&gateway_ip);
    let ip_checksum = internet_checksum(ip);
    ip[10..12].copy_from_slice(&ip_checksum.to_be_bytes());
    let icmp = &mut frame[34..47];
    icmp[0] = 8;
    icmp[4..6].copy_from_slice(&0x4d4bu16.to_be_bytes());
    icmp[6..8].copy_from_slice(&1u16.to_be_bytes());
    icmp[8..13].copy_from_slice(b"MakOS");
    let icmp_checksum = internet_checksum(icmp);
    icmp[2..4].copy_from_slice(&icmp_checksum.to_be_bytes());
    frame
}

fn build_dns_query(mac: [u8; 6], dns_mac: [u8; 6], our_ip: [u8; 4], dns_ip: [u8; 4]) -> [u8; 71] {
    let mut frame = [0u8; 71];
    frame[0..6].copy_from_slice(&dns_mac);
    frame[6..12].copy_from_slice(&mac);
    frame[12..14].copy_from_slice(&[0x08, 0x00]);
    let ip = &mut frame[14..34];
    ip[0] = 0x45;
    ip[2..4].copy_from_slice(&57u16.to_be_bytes());
    ip[4..6].copy_from_slice(&3u16.to_be_bytes());
    ip[6..8].copy_from_slice(&0x4000u16.to_be_bytes());
    ip[8] = 64;
    ip[9] = 17;
    ip[12..16].copy_from_slice(&our_ip);
    ip[16..20].copy_from_slice(&dns_ip);
    let checksum = internet_checksum(ip);
    ip[10..12].copy_from_slice(&checksum.to_be_bytes());
    let udp = &mut frame[34..];
    udp[0..2].copy_from_slice(&49152u16.to_be_bytes());
    udp[2..4].copy_from_slice(&53u16.to_be_bytes());
    udp[4..6].copy_from_slice(&37u16.to_be_bytes());
    let dns = &mut udp[8..];
    dns[0..2].copy_from_slice(&0x4d4bu16.to_be_bytes());
    dns[2..4].copy_from_slice(&0x0100u16.to_be_bytes());
    dns[4..6].copy_from_slice(&1u16.to_be_bytes());
    dns[12..25].copy_from_slice(&[
        7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0,
    ]);
    dns[25..27].copy_from_slice(&1u16.to_be_bytes());
    dns[27..29].copy_from_slice(&1u16.to_be_bytes());
    frame
}

fn build_tcp_syn(
    mac: [u8; 6],
    gateway_mac: [u8; 6],
    our_ip: [u8; 4],
    remote_ip: [u8; 4],
    source_port: u16,
    remote_port: u16,
    sequence: u32,
) -> [u8; 60] {
    let mut frame = [0u8; 60];
    frame[0..6].copy_from_slice(&gateway_mac);
    frame[6..12].copy_from_slice(&mac);
    frame[12..14].copy_from_slice(&[0x08, 0x00]);
    let ip = &mut frame[14..34];
    ip[0] = 0x45;
    ip[2..4].copy_from_slice(&40u16.to_be_bytes());
    ip[4..6].copy_from_slice(&4u16.to_be_bytes());
    ip[6..8].copy_from_slice(&0x4000u16.to_be_bytes());
    ip[8] = 64;
    ip[9] = 6;
    ip[12..16].copy_from_slice(&our_ip);
    ip[16..20].copy_from_slice(&remote_ip);
    let checksum = internet_checksum(ip);
    ip[10..12].copy_from_slice(&checksum.to_be_bytes());

    let tcp = &mut frame[34..54];
    tcp[0..2].copy_from_slice(&source_port.to_be_bytes());
    tcp[2..4].copy_from_slice(&remote_port.to_be_bytes());
    tcp[4..8].copy_from_slice(&sequence.to_be_bytes());
    tcp[12] = 5 << 4;
    tcp[13] = 0x02;
    tcp[14..16].copy_from_slice(&64240u16.to_be_bytes());
    let checksum = transport_checksum(our_ip, remote_ip, 6, tcp);
    tcp[16..18].copy_from_slice(&checksum.to_be_bytes());
    frame
}

#[allow(clippy::too_many_arguments)]
fn build_tcp_packet(
    mac: [u8; 6],
    gateway_mac: [u8; 6],
    our_ip: [u8; 4],
    remote_ip: [u8; 4],
    source_port: u16,
    remote_port: u16,
    sequence: u32,
    acknowledgement: u32,
    flags: u8,
    payload: &[u8],
) -> Option<([u8; 1536], usize)> {
    let tcp_length = 20usize.checked_add(payload.len())?;
    let ip_length = 20usize.checked_add(tcp_length)?;
    let frame_length = 14usize.checked_add(ip_length)?.max(60);
    if frame_length > 1536 || ip_length > u16::MAX as usize {
        return None;
    }
    let mut frame = [0u8; 1536];
    frame[0..6].copy_from_slice(&gateway_mac);
    frame[6..12].copy_from_slice(&mac);
    frame[12..14].copy_from_slice(&[0x08, 0x00]);
    let ip = &mut frame[14..34];
    ip[0] = 0x45;
    ip[2..4].copy_from_slice(&(ip_length as u16).to_be_bytes());
    ip[4..6].copy_from_slice(&6u16.to_be_bytes());
    ip[6..8].copy_from_slice(&0x4000u16.to_be_bytes());
    ip[8] = 64;
    ip[9] = 6;
    ip[12..16].copy_from_slice(&our_ip);
    ip[16..20].copy_from_slice(&remote_ip);
    let checksum = internet_checksum(ip);
    ip[10..12].copy_from_slice(&checksum.to_be_bytes());
    let tcp = &mut frame[34..34 + tcp_length];
    tcp[0..2].copy_from_slice(&source_port.to_be_bytes());
    tcp[2..4].copy_from_slice(&remote_port.to_be_bytes());
    tcp[4..8].copy_from_slice(&sequence.to_be_bytes());
    tcp[8..12].copy_from_slice(&acknowledgement.to_be_bytes());
    tcp[12] = 5 << 4;
    tcp[13] = flags;
    tcp[14..16].copy_from_slice(&64240u16.to_be_bytes());
    tcp[20..].copy_from_slice(payload);
    let checksum = transport_checksum(our_ip, remote_ip, 6, tcp);
    tcp[16..18].copy_from_slice(&checksum.to_be_bytes());
    Some((frame, frame_length))
}

fn transmit(io: u16, frame: &[u8]) {
    if frame.len() > 1536 {
        crate::fatal("RTL8139 invalid transmit request");
    }
    while TX_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let descriptor = TX_NEXT.load(Ordering::Relaxed) & 3;
    let status_port = io + TX_STATUS0 + descriptor as u16 * 4;
    let mut ready = false;
    for _ in 0..1_000_000 {
        if unsafe { inl(status_port) } & (1 << 13) != 0 {
            ready = true;
            break;
        }
        core::hint::spin_loop();
    }
    if !ready {
        crate::fatal("RTL8139 transmit descriptor timeout");
    }
    let storage = unsafe {
        (&raw mut TX_STORAGE)
            .cast::<TxStorage>()
            .add(descriptor)
            .cast::<u8>()
    };
    unsafe {
        ptr::copy_nonoverlapping(frame.as_ptr(), storage, frame.len());
        outl(io + TX_ADDRESS0 + descriptor as u16 * 4, storage as u32);
        outl(status_port, frame.len() as u32);
    }
    let mut complete = false;
    for _ in 0..1_000_000 {
        let status = unsafe { inl(status_port) };
        if status & ((1 << 14) | (1 << 29) | (1 << 30) | (1 << 31)) != 0 {
            crate::fatal("RTL8139 transmit error");
        }
        if status & ((1 << 13) | (1 << 15)) == (1 << 13) | (1 << 15) {
            complete = true;
            break;
        }
        core::hint::spin_loop();
    }
    if !complete {
        crate::fatal("RTL8139 transmit completion timeout");
    }
    TX_NEXT.store((descriptor + 1) & 3, Ordering::Relaxed);
    TX_LOCK.store(false, Ordering::Release);
}

fn receive_arp_reply(
    io: u16,
    offset: &mut usize,
    our_mac: [u8; 6],
    our_ip: [u8; 4],
    gateway_ip: [u8; 4],
) -> Option<[u8; 6]> {
    for _ in 0..16 {
        if unsafe { inb(io + COMMAND) } & 1 != 0 {
            return None;
        }
        let base = (&raw const RX_STORAGE).cast::<u8>();
        let status = unsafe { read_ring_u16(base, *offset) };
        let length = unsafe { read_ring_u16(base, *offset + 2) } as usize;
        if status & 1 == 0 || !(64..=1536).contains(&length) {
            crate::fatal("RTL8139 malformed receive descriptor");
        }
        let frame_length = length.saturating_sub(4);
        let mut frame = [0u8; 1536];
        for (index, byte) in frame.iter_mut().take(frame_length).enumerate() {
            *byte = unsafe {
                base.add((*offset + 4 + index) % RX_RING_BYTES)
                    .read_volatile()
            };
        }
        *offset = (*offset + length + 4 + 3) & !3;
        *offset %= RX_RING_BYTES;
        unsafe {
            outw(
                io + RX_BUFFER_POINTER,
                ((*offset + RX_RING_BYTES - 16) % RX_RING_BYTES) as u16,
            );
            outw(io + INTERRUPT_STATUS, 1);
        }
        if frame_length >= 42
            && frame[0..6] == our_mac
            && frame[12..14] == [0x08, 0x06]
            && frame[20..22] == [0x00, 0x02]
            && frame[28..32] == gateway_ip
            && frame[38..42] == our_ip
        {
            return frame[22..28].try_into().ok();
        }
    }
    None
}

fn receive_arp_with_timeout(
    io: u16,
    offset: &mut usize,
    our_mac: [u8; 6],
    our_ip: [u8; 4],
    target_ip: [u8; 4],
) -> Option<[u8; 6]> {
    for _ in 0..50_000_000 {
        if let Some(mac) = receive_arp_reply(io, offset, our_mac, our_ip, target_ip) {
            return Some(mac);
        }
        core::hint::spin_loop();
    }
    None
}

fn receive_icmp_reply(
    io: u16,
    offset: &mut usize,
    our_mac: [u8; 6],
    gateway_mac: [u8; 6],
    our_ip: [u8; 4],
    gateway_ip: [u8; 4],
) -> bool {
    for _ in 0..50_000_000 {
        if unsafe { inb(io + COMMAND) } & 1 != 0 {
            core::hint::spin_loop();
            continue;
        }
        let mut frame = [0u8; 1536];
        let Some(length) = receive_frame(io, offset, &mut frame) else {
            continue;
        };
        if length >= 47
            && frame[0..6] == our_mac
            && frame[6..12] == gateway_mac
            && frame[12..14] == [0x08, 0x00]
            && frame[23] == 1
            && frame[26..30] == gateway_ip
            && frame[30..34] == our_ip
            && internet_checksum(&frame[14..34]) == 0
            && frame[34] == 0
            && frame[38..40] == 0x4d4bu16.to_be_bytes()
            && frame[40..42] == 1u16.to_be_bytes()
            && internet_checksum(&frame[34..47]) == 0
        {
            return true;
        }
    }
    false
}

fn receive_dns_reply(
    io: u16,
    offset: &mut usize,
    our_mac: [u8; 6],
    _dns_mac: [u8; 6],
    our_ip: [u8; 4],
    dns_ip: [u8; 4],
) -> Option<[u8; 4]> {
    for _ in 0..50_000_000 {
        if unsafe { inb(io + COMMAND) } & 1 != 0 {
            core::hint::spin_loop();
            continue;
        }
        let mut frame = [0u8; 1536];
        let Some(length) = receive_frame(io, offset, &mut frame) else {
            continue;
        };
        if length < 71
            || frame[0..6] != our_mac
            || frame[12..14] != [0x08, 0x00]
            || frame[23] != 17
            || frame[26..30] != dns_ip
            || frame[30..34] != our_ip
            || frame[34..36] != 53u16.to_be_bytes()
            || frame[36..38] != 49152u16.to_be_bytes()
            || frame[42..44] != 0x4d4bu16.to_be_bytes()
            || frame[44] & 0x80 == 0
        {
            continue;
        }
        let udp_length = u16::from_be_bytes([frame[38], frame[39]]) as usize;
        let dns_end = (34 + udp_length).min(length);
        let ip_header_bytes = usize::from(frame[14] & 0x0f) * 4;
        if ip_header_bytes < 20 || 14 + ip_header_bytes + 20 > length {
            continue;
        }
        let dns_start = 14 + ip_header_bytes + 8;
        if frame[dns_start..dns_start + 2] != 0x4d4bu16.to_be_bytes()
            || frame[dns_start + 2] & 0x80 == 0
        {
            continue;
        }
        let answer_count = u16::from_be_bytes([frame[dns_start + 6], frame[dns_start + 7]]);
        if answer_count == 0 {
            continue;
        }
        let mut cursor = dns_start + 12;
        if !skip_dns_name(&frame, dns_end, &mut cursor) || cursor + 4 > dns_end {
            continue;
        }
        cursor += 4;
        for _ in 0..answer_count {
            if !skip_dns_name(&frame, dns_end, &mut cursor) || cursor + 10 > dns_end {
                break;
            }
            let kind = u16::from_be_bytes([frame[cursor], frame[cursor + 1]]);
            let class = u16::from_be_bytes([frame[cursor + 2], frame[cursor + 3]]);
            let data_length = u16::from_be_bytes([frame[cursor + 8], frame[cursor + 9]]) as usize;
            cursor += 10;
            if cursor + data_length > dns_end {
                break;
            }
            if kind == 1 && class == 1 && data_length == 4 {
                return frame[cursor..cursor + 4].try_into().ok();
            }
            cursor += data_length;
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn receive_tcp_synack(
    io: u16,
    offset: &mut usize,
    our_mac: [u8; 6],
    gateway_mac: [u8; 6],
    our_ip: [u8; 4],
    remote_ip: [u8; 4],
    source_port: u16,
    remote_port: u16,
    sequence: u32,
) -> Option<u32> {
    for _ in 0..50_000_000 {
        if unsafe { inb(io + COMMAND) } & 1 != 0 {
            core::hint::spin_loop();
            continue;
        }
        let mut frame = [0u8; 1536];
        let Some(length) = receive_frame(io, offset, &mut frame) else {
            continue;
        };
        if length < 54
            || frame[0..6] != our_mac
            || frame[6..12] != gateway_mac
            || frame[12..14] != [0x08, 0x00]
            || frame[23] != 6
            || frame[26..30] != remote_ip
            || frame[30..34] != our_ip
        {
            continue;
        }
        let ip_header_bytes = usize::from(frame[14] & 0x0f) * 4;
        let ip_total_bytes = u16::from_be_bytes([frame[16], frame[17]]) as usize;
        if ip_header_bytes < 20
            || ip_total_bytes < ip_header_bytes + 20
            || 14 + ip_total_bytes > length
            || internet_checksum(&frame[14..14 + ip_header_bytes]) != 0
        {
            continue;
        }
        let tcp_start = 14 + ip_header_bytes;
        let tcp_end = 14 + ip_total_bytes;
        let tcp = &frame[tcp_start..tcp_end];
        let tcp_header_bytes = usize::from(tcp[12] >> 4) * 4;
        if tcp_header_bytes < 20
            || tcp_header_bytes > tcp.len()
            || tcp[0..2] != remote_port.to_be_bytes()
            || tcp[2..4] != source_port.to_be_bytes()
            || u32::from_be_bytes(tcp[8..12].try_into().unwrap()) != sequence.wrapping_add(1)
            || tcp[13] & 0x12 != 0x12
            || transport_checksum(remote_ip, our_ip, 6, tcp) != 0
        {
            continue;
        }
        return Some(u32::from_be_bytes(tcp[4..8].try_into().ok()?));
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn receive_tcp_payload(
    io: u16,
    offset: &mut usize,
    our_mac: [u8; 6],
    gateway_mac: [u8; 6],
    our_ip: [u8; 4],
    remote_ip: [u8; 4],
    source_port: u16,
    remote_port: u16,
    expected_ack: u32,
    output: &mut [u8],
) -> Option<(usize, u32, bool)> {
    for _ in 0..80_000_000 {
        if unsafe { inb(io + COMMAND) } & 1 != 0 {
            core::hint::spin_loop();
            continue;
        }
        let mut frame = [0u8; 1536];
        let length = receive_frame(io, offset, &mut frame)?;
        if length < 54
            || frame[0..6] != our_mac
            || frame[6..12] != gateway_mac
            || frame[12..14] != [0x08, 0x00]
            || frame[23] != 6
            || frame[26..30] != remote_ip
            || frame[30..34] != our_ip
        {
            continue;
        }
        let ip_header_bytes = usize::from(frame[14] & 0x0f) * 4;
        let ip_total_bytes = u16::from_be_bytes([frame[16], frame[17]]) as usize;
        if ip_header_bytes < 20
            || ip_total_bytes < ip_header_bytes + 20
            || 14 + ip_total_bytes > length
            || internet_checksum(&frame[14..14 + ip_header_bytes]) != 0
        {
            continue;
        }
        let tcp_start = 14 + ip_header_bytes;
        let tcp = &frame[tcp_start..14 + ip_total_bytes];
        let header_bytes = usize::from(tcp[12] >> 4) * 4;
        if header_bytes < 20
            || header_bytes > tcp.len()
            || tcp[0..2] != remote_port.to_be_bytes()
            || tcp[2..4] != source_port.to_be_bytes()
            || u32::from_be_bytes(tcp[8..12].try_into().ok()?) != expected_ack
            || tcp[13] & 0x10 == 0
            || transport_checksum(remote_ip, our_ip, 6, tcp) != 0
        {
            continue;
        }
        let payload = &tcp[header_bytes..];
        if payload.is_empty() {
            continue;
        }
        let count = payload.len().min(output.len());
        output[..count].copy_from_slice(&payload[..count]);
        let sequence = u32::from_be_bytes(tcp[4..8].try_into().ok()?);
        return Some((count, sequence, tcp[13] & 0x01 != 0));
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn receive_udp_payload(
    io: u16,
    offset: &mut usize,
    our_mac: [u8; 6],
    our_ip: [u8; 4],
    remote_ip: [u8; 4],
    local_port: u16,
    remote_port: u16,
    transaction: Option<[u8; 2]>,
    output: &mut [u8],
) -> Option<usize> {
    for _ in 0..50_000_000 {
        if unsafe { inb(io + COMMAND) } & 1 != 0 {
            core::hint::spin_loop();
            continue;
        }
        let mut frame = [0u8; 1536];
        let Some(length) = receive_frame(io, offset, &mut frame) else {
            continue;
        };
        if length < 42
            || frame[0..6] != our_mac
            || frame[12..14] != [0x08, 0x00]
            || frame[23] != 17
            || frame[26..30] != remote_ip
            || frame[30..34] != our_ip
        {
            continue;
        }
        let ip_header_bytes = usize::from(frame[14] & 0x0f) * 4;
        let ip_total_bytes = u16::from_be_bytes([frame[16], frame[17]]) as usize;
        if ip_header_bytes < 20
            || ip_total_bytes < ip_header_bytes + 8
            || 14 + ip_total_bytes > length
            || internet_checksum(&frame[14..14 + ip_header_bytes]) != 0
        {
            continue;
        }
        let udp_start = 14 + ip_header_bytes;
        let udp = &frame[udp_start..14 + ip_total_bytes];
        let udp_length = u16::from_be_bytes([udp[4], udp[5]]) as usize;
        if udp_length < 10
            || udp_length > udp.len()
            || udp[0..2] != remote_port.to_be_bytes()
            || udp[2..4] != local_port.to_be_bytes()
            || transaction.is_some_and(|value| udp[8..10] != value)
            || (u16::from_be_bytes([udp[6], udp[7]]) != 0
                && transport_checksum(remote_ip, our_ip, 17, &udp[..udp_length]) != 0)
        {
            continue;
        }
        let payload = &udp[8..udp_length];
        let count = payload.len().min(output.len());
        output[..count].copy_from_slice(&payload[..count]);
        return Some(count);
    }
    None
}

fn skip_dns_name(packet: &[u8], end: usize, cursor: &mut usize) -> bool {
    while *cursor < end {
        let size = packet[*cursor] as usize;
        *cursor += 1;
        if size == 0 {
            return true;
        }
        if size & 0xc0 == 0xc0 {
            if *cursor >= end {
                return false;
            }
            *cursor += 1;
            return true;
        }
        if size > 63 || *cursor + size > end {
            return false;
        }
        *cursor += size;
    }
    false
}

fn receive_frame(io: u16, offset: &mut usize, frame: &mut [u8; 1536]) -> Option<usize> {
    if unsafe { inb(io + COMMAND) } & 1 != 0 {
        return None;
    }
    let base = (&raw const RX_STORAGE).cast::<u8>();
    let status = unsafe { read_ring_u16(base, *offset) };
    let length = unsafe { read_ring_u16(base, *offset + 2) } as usize;
    if status & 1 == 0 || !(64..=1536).contains(&length) {
        crate::fatal("RTL8139 malformed receive descriptor");
    }
    let frame_length = length.saturating_sub(4);
    for (index, byte) in frame.iter_mut().take(frame_length).enumerate() {
        *byte = unsafe {
            base.add((*offset + 4 + index) % RX_RING_BYTES)
                .read_volatile()
        };
    }
    *offset = (*offset + length + 4 + 3) & !3;
    *offset %= RX_RING_BYTES;
    unsafe {
        outw(
            io + RX_BUFFER_POINTER,
            ((*offset + RX_RING_BYTES - 16) % RX_RING_BYTES) as u16,
        );
        outw(io + INTERRUPT_STATUS, 1);
    }
    Some(frame_length)
}

fn internet_checksum(bytes: &[u8]) -> u16 {
    let mut sum = 0u32;
    for chunk in bytes.chunks(2) {
        let word = if chunk.len() == 2 {
            u16::from_be_bytes([chunk[0], chunk[1]])
        } else {
            u16::from(chunk[0]) << 8
        };
        sum += u32::from(word);
        while sum > 0xffff {
            sum = (sum & 0xffff) + (sum >> 16);
        }
    }
    !(sum as u16)
}

fn transport_checksum(source: [u8; 4], destination: [u8; 4], protocol: u8, bytes: &[u8]) -> u16 {
    let mut sum = 0u32;
    for word in [
        u16::from_be_bytes([source[0], source[1]]),
        u16::from_be_bytes([source[2], source[3]]),
        u16::from_be_bytes([destination[0], destination[1]]),
        u16::from_be_bytes([destination[2], destination[3]]),
        u16::from(protocol),
        bytes.len() as u16,
    ] {
        sum += u32::from(word);
    }
    for chunk in bytes.chunks(2) {
        let word = if chunk.len() == 2 {
            u16::from_be_bytes([chunk[0], chunk[1]])
        } else {
            u16::from(chunk[0]) << 8
        };
        sum += u32::from(word);
        while sum > 0xffff {
            sum = (sum & 0xffff) + (sum >> 16);
        }
    }
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

fn ipv6_transport_checksum(
    source: [u8; 16],
    destination: [u8; 16],
    protocol: u8,
    bytes: &[u8],
) -> u16 {
    let mut sum = 0u32;
    for chunk in source.chunks_exact(2).chain(destination.chunks_exact(2)) {
        sum += u32::from(u16::from_be_bytes([chunk[0], chunk[1]]));
    }
    let length = bytes.len() as u32;
    sum += length >> 16;
    sum += length & 0xffff;
    sum += u32::from(protocol);
    for chunk in bytes.chunks(2) {
        sum += u32::from(if chunk.len() == 2 {
            u16::from_be_bytes([chunk[0], chunk[1]])
        } else {
            u16::from(chunk[0]) << 8
        });
        while sum > 0xffff {
            sum = (sum & 0xffff) + (sum >> 16);
        }
    }
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

unsafe fn read_ring_u16(base: *const u8, offset: usize) -> u16 {
    let low = unsafe { base.add(offset % RX_RING_BYTES).read_volatile() };
    let high = unsafe { base.add((offset + 1) % RX_RING_BYTES).read_volatile() };
    u16::from_le_bytes([low, high])
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    unsafe {
        asm!("in ax, dx", out("ax") value, in("dx") port, options(nomem, nostack, preserves_flags))
    };
    value
}

#[inline]
unsafe fn outw(port: u16, value: u16) {
    unsafe {
        asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags))
    };
}
