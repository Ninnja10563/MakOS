//! Allocation-free Ethernet/IPv4 wire helpers for AArch64 virtio-net.

pub const ETHERNET_HEADER: usize = 14;
pub const IPV4_MIN_HEADER: usize = 20;
pub const IPV6_HEADER: usize = 40;
pub const UDP_HEADER: usize = 8;
pub const TCP_MIN_HEADER: usize = 20;
pub const FRAME_CAPACITY: usize = 1514;
pub const TCP_PAYLOAD_MAX: usize = 1400;

const ETHERTYPE_IPV4: u16 = 0x0800;
const ETHERTYPE_ARP: u16 = 0x0806;
const ETHERTYPE_IPV6: u16 = 0x86dd;
const IPPROTO_TCP: u8 = 6;
const IPPROTO_UDP: u8 = 17;
const IPPROTO_ICMPV6: u8 = 58;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UdpPacket<'a> {
    pub source_ip: [u8; 4],
    pub destination_ip: [u8; 4],
    pub source_port: u16,
    pub destination_port: u16,
    pub payload: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TcpSegment<'a> {
    pub source_ip: [u8; 4],
    pub destination_ip: [u8; 4],
    pub source_port: u16,
    pub destination_port: u16,
    pub sequence: u32,
    pub acknowledgment: u32,
    pub flags: u8,
    pub window: u16,
    pub payload: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UdpPacketV6<'a> {
    pub source_ip: [u8; 16],
    pub destination_ip: [u8; 16],
    pub source_port: u16,
    pub destination_port: u16,
    pub payload: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TcpSegmentV6<'a> {
    pub source_ip: [u8; 16],
    pub destination_ip: [u8; 16],
    pub source_port: u16,
    pub destination_port: u16,
    pub sequence: u32,
    pub acknowledgment: u32,
    pub flags: u8,
    pub window: u16,
    pub payload: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouterAdvertisement {
    pub router_ip: [u8; 16],
    pub router_mac: [u8; 6],
    pub prefix: [u8; 16],
    pub prefix_length: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DhcpReply {
    pub message_type: u8,
    pub address: [u8; 4],
    pub subnet: [u8; 4],
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
    pub server: [u8; 4],
}

pub fn build_arp_request(
    output: &mut [u8],
    source_mac: [u8; 6],
    source_ip: [u8; 4],
    target_ip: [u8; 4],
) -> Option<usize> {
    if output.len() < 42 {
        return None;
    }
    output[..6].fill(0xff);
    output[6..12].copy_from_slice(&source_mac);
    put16(&mut output[12..14], ETHERTYPE_ARP);
    put16(&mut output[14..16], 1);
    put16(&mut output[16..18], ETHERTYPE_IPV4);
    output[18] = 6;
    output[19] = 4;
    put16(&mut output[20..22], 1);
    output[22..28].copy_from_slice(&source_mac);
    output[28..32].copy_from_slice(&source_ip);
    output[32..38].fill(0);
    output[38..42].copy_from_slice(&target_ip);
    Some(42)
}

pub fn parse_arp_reply(
    frame: &[u8],
    local_mac: [u8; 6],
    local_ip: [u8; 4],
    expected_ip: [u8; 4],
) -> Option<[u8; 6]> {
    if frame.len() < 42
        || frame[..6] != local_mac
        || get16(&frame[12..14]) != ETHERTYPE_ARP
        || get16(&frame[14..16]) != 1
        || get16(&frame[16..18]) != ETHERTYPE_IPV4
        || frame[18] != 6
        || frame[19] != 4
        || get16(&frame[20..22]) != 2
        || frame[28..32] != expected_ip
        || frame[38..42] != local_ip
    {
        return None;
    }
    let mut mac = [0; 6];
    mac.copy_from_slice(&frame[22..28]);
    (mac != [0; 6] && mac != [0xff; 6]).then_some(mac)
}

#[allow(clippy::too_many_arguments)]
pub fn build_udp(
    output: &mut [u8],
    source_mac: [u8; 6],
    destination_mac: [u8; 6],
    source_ip: [u8; 4],
    destination_ip: [u8; 4],
    source_port: u16,
    destination_port: u16,
    payload: &[u8],
    identification: u16,
) -> Option<usize> {
    let udp_length = UDP_HEADER.checked_add(payload.len())?;
    let ip_length = IPV4_MIN_HEADER.checked_add(udp_length)?;
    let frame_length = ETHERNET_HEADER.checked_add(ip_length)?;
    if output.len() < frame_length || ip_length > u16::MAX as usize {
        return None;
    }
    output[..frame_length].fill(0);
    output[..6].copy_from_slice(&destination_mac);
    output[6..12].copy_from_slice(&source_mac);
    put16(&mut output[12..14], ETHERTYPE_IPV4);
    write_ipv4_header(
        &mut output[14..34],
        ip_length as u16,
        identification,
        IPPROTO_UDP,
        source_ip,
        destination_ip,
    );
    let udp = &mut output[34..frame_length];
    put16(&mut udp[..2], source_port);
    put16(&mut udp[2..4], destination_port);
    put16(&mut udp[4..6], udp_length as u16);
    udp[8..].copy_from_slice(payload);
    let checksum = transport_checksum(source_ip, destination_ip, IPPROTO_UDP, udp);
    put16(
        &mut udp[6..8],
        if checksum == 0 { 0xffff } else { checksum },
    );
    Some(frame_length)
}

pub fn parse_udp(frame: &[u8]) -> Option<UdpPacket<'_>> {
    let (source_ip, destination_ip, protocol, payload) = parse_ipv4(frame)?;
    if protocol != IPPROTO_UDP || payload.len() < UDP_HEADER {
        return None;
    }
    let length = usize::from(get16(&payload[4..6]));
    if length < UDP_HEADER || length > payload.len() {
        return None;
    }
    let datagram = &payload[..length];
    let supplied = get16(&datagram[6..8]);
    if supplied != 0 && transport_checksum(source_ip, destination_ip, IPPROTO_UDP, datagram) != 0 {
        return None;
    }
    Some(UdpPacket {
        source_ip,
        destination_ip,
        source_port: get16(&datagram[..2]),
        destination_port: get16(&datagram[2..4]),
        payload: &datagram[8..],
    })
}

#[allow(clippy::too_many_arguments)]
pub fn build_tcp(
    output: &mut [u8],
    source_mac: [u8; 6],
    destination_mac: [u8; 6],
    source_ip: [u8; 4],
    destination_ip: [u8; 4],
    source_port: u16,
    destination_port: u16,
    sequence: u32,
    acknowledgment: u32,
    flags: u8,
    receive_window: u16,
    payload: &[u8],
    identification: u16,
) -> Option<usize> {
    let tcp_length = TCP_MIN_HEADER.checked_add(payload.len())?;
    let ip_length = IPV4_MIN_HEADER.checked_add(tcp_length)?;
    let frame_length = ETHERNET_HEADER.checked_add(ip_length)?;
    if output.len() < frame_length || ip_length > u16::MAX as usize {
        return None;
    }
    output[..frame_length].fill(0);
    output[..6].copy_from_slice(&destination_mac);
    output[6..12].copy_from_slice(&source_mac);
    put16(&mut output[12..14], ETHERTYPE_IPV4);
    write_ipv4_header(
        &mut output[14..34],
        ip_length as u16,
        identification,
        IPPROTO_TCP,
        source_ip,
        destination_ip,
    );
    let tcp = &mut output[34..frame_length];
    put16(&mut tcp[..2], source_port);
    put16(&mut tcp[2..4], destination_port);
    put32(&mut tcp[4..8], sequence);
    put32(&mut tcp[8..12], acknowledgment);
    tcp[12] = 5 << 4;
    tcp[13] = flags;
    put16(&mut tcp[14..16], receive_window);
    tcp[20..].copy_from_slice(payload);
    let checksum = transport_checksum(source_ip, destination_ip, IPPROTO_TCP, tcp);
    put16(&mut tcp[16..18], checksum);
    Some(frame_length)
}

pub fn parse_tcp(frame: &[u8]) -> Option<TcpSegment<'_>> {
    let (source_ip, destination_ip, protocol, payload) = parse_ipv4(frame)?;
    if protocol != IPPROTO_TCP || payload.len() < TCP_MIN_HEADER {
        return None;
    }
    let header_length = usize::from(payload[12] >> 4) * 4;
    if header_length < TCP_MIN_HEADER || header_length > payload.len() {
        return None;
    }
    if transport_checksum(source_ip, destination_ip, IPPROTO_TCP, payload) != 0 {
        return None;
    }
    Some(TcpSegment {
        source_ip,
        destination_ip,
        source_port: get16(&payload[..2]),
        destination_port: get16(&payload[2..4]),
        sequence: get32(&payload[4..8]),
        acknowledgment: get32(&payload[8..12]),
        flags: payload[13],
        window: get16(&payload[14..16]),
        payload: &payload[header_length..],
    })
}

#[allow(clippy::too_many_arguments)]
pub fn build_udp_v6(
    output: &mut [u8],
    source_mac: [u8; 6],
    destination_mac: [u8; 6],
    source_ip: [u8; 16],
    destination_ip: [u8; 16],
    source_port: u16,
    destination_port: u16,
    payload: &[u8],
) -> Option<usize> {
    let udp_length = UDP_HEADER.checked_add(payload.len())?;
    let frame_length = ETHERNET_HEADER
        .checked_add(IPV6_HEADER)?
        .checked_add(udp_length)?;
    if output.len() < frame_length || udp_length > u16::MAX as usize {
        return None;
    }
    output[..frame_length].fill(0);
    write_ipv6_envelope(
        output,
        source_mac,
        destination_mac,
        source_ip,
        destination_ip,
        IPPROTO_UDP,
        udp_length,
        64,
    )?;
    let udp = &mut output[ETHERNET_HEADER + IPV6_HEADER..frame_length];
    put16(&mut udp[..2], source_port);
    put16(&mut udp[2..4], destination_port);
    put16(&mut udp[4..6], udp_length as u16);
    udp[8..].copy_from_slice(payload);
    let checksum = transport_checksum_v6(source_ip, destination_ip, IPPROTO_UDP, udp);
    put16(
        &mut udp[6..8],
        if checksum == 0 { 0xffff } else { checksum },
    );
    Some(frame_length)
}

pub fn parse_udp_v6(frame: &[u8]) -> Option<UdpPacketV6<'_>> {
    let (source_ip, destination_ip, protocol, payload, _) = parse_ipv6(frame)?;
    if protocol != IPPROTO_UDP || payload.len() < UDP_HEADER {
        return None;
    }
    let length = usize::from(get16(&payload[4..6]));
    if length < UDP_HEADER || length > payload.len() {
        return None;
    }
    let datagram = &payload[..length];
    if get16(&datagram[6..8]) == 0
        || transport_checksum_v6(source_ip, destination_ip, IPPROTO_UDP, datagram) != 0
    {
        return None;
    }
    Some(UdpPacketV6 {
        source_ip,
        destination_ip,
        source_port: get16(&datagram[..2]),
        destination_port: get16(&datagram[2..4]),
        payload: &datagram[8..],
    })
}

#[allow(clippy::too_many_arguments)]
pub fn build_tcp_v6(
    output: &mut [u8],
    source_mac: [u8; 6],
    destination_mac: [u8; 6],
    source_ip: [u8; 16],
    destination_ip: [u8; 16],
    source_port: u16,
    destination_port: u16,
    sequence: u32,
    acknowledgment: u32,
    flags: u8,
    receive_window: u16,
    payload: &[u8],
) -> Option<usize> {
    let tcp_length = TCP_MIN_HEADER.checked_add(payload.len())?;
    let frame_length = ETHERNET_HEADER
        .checked_add(IPV6_HEADER)?
        .checked_add(tcp_length)?;
    if output.len() < frame_length || tcp_length > u16::MAX as usize {
        return None;
    }
    output[..frame_length].fill(0);
    write_ipv6_envelope(
        output,
        source_mac,
        destination_mac,
        source_ip,
        destination_ip,
        IPPROTO_TCP,
        tcp_length,
        64,
    )?;
    let tcp = &mut output[ETHERNET_HEADER + IPV6_HEADER..frame_length];
    put16(&mut tcp[..2], source_port);
    put16(&mut tcp[2..4], destination_port);
    put32(&mut tcp[4..8], sequence);
    put32(&mut tcp[8..12], acknowledgment);
    tcp[12] = 5 << 4;
    tcp[13] = flags;
    put16(&mut tcp[14..16], receive_window);
    tcp[20..].copy_from_slice(payload);
    let checksum = transport_checksum_v6(source_ip, destination_ip, IPPROTO_TCP, tcp);
    put16(&mut tcp[16..18], checksum);
    Some(frame_length)
}

pub fn parse_tcp_v6(frame: &[u8]) -> Option<TcpSegmentV6<'_>> {
    let (source_ip, destination_ip, protocol, payload, _) = parse_ipv6(frame)?;
    if protocol != IPPROTO_TCP || payload.len() < TCP_MIN_HEADER {
        return None;
    }
    let header_length = usize::from(payload[12] >> 4) * 4;
    if header_length < TCP_MIN_HEADER
        || header_length > payload.len()
        || transport_checksum_v6(source_ip, destination_ip, IPPROTO_TCP, payload) != 0
    {
        return None;
    }
    Some(TcpSegmentV6 {
        source_ip,
        destination_ip,
        source_port: get16(&payload[..2]),
        destination_port: get16(&payload[2..4]),
        sequence: get32(&payload[4..8]),
        acknowledgment: get32(&payload[8..12]),
        flags: payload[13],
        window: get16(&payload[14..16]),
        payload: &payload[header_length..],
    })
}

pub fn link_local_from_mac(mac: [u8; 6]) -> [u8; 16] {
    let mut address = [0u8; 16];
    address[0] = 0xfe;
    address[1] = 0x80;
    address[8] = mac[0] ^ 0x02;
    address[9] = mac[1];
    address[10] = mac[2];
    address[11] = 0xff;
    address[12] = 0xfe;
    address[13] = mac[3];
    address[14] = mac[4];
    address[15] = mac[5];
    address
}

pub fn multicast_mac(address: [u8; 16]) -> [u8; 6] {
    [
        0x33,
        0x33,
        address[12],
        address[13],
        address[14],
        address[15],
    ]
}

pub fn solicited_node_multicast(target: [u8; 16]) -> [u8; 16] {
    let mut address = [0u8; 16];
    address[0] = 0xff;
    address[1] = 0x02;
    address[11] = 0x01;
    address[12] = 0xff;
    address[13..].copy_from_slice(&target[13..]);
    address
}

pub fn build_router_solicitation(
    output: &mut [u8],
    mac: [u8; 6],
    link_local: [u8; 16],
) -> Option<usize> {
    const ALL_ROUTERS: [u8; 16] = [0xff, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2];
    let length = ETHERNET_HEADER + IPV6_HEADER + 16;
    if output.len() < length {
        return None;
    }
    output[..length].fill(0);
    write_ipv6_envelope(
        output,
        mac,
        multicast_mac(ALL_ROUTERS),
        link_local,
        ALL_ROUTERS,
        IPPROTO_ICMPV6,
        16,
        255,
    )?;
    let icmp = &mut output[ETHERNET_HEADER + IPV6_HEADER..length];
    icmp[0] = 133;
    icmp[8] = 1;
    icmp[9] = 1;
    icmp[10..16].copy_from_slice(&mac);
    let checksum = transport_checksum_v6(link_local, ALL_ROUTERS, IPPROTO_ICMPV6, icmp);
    put16(&mut icmp[2..4], checksum);
    Some(length)
}

pub fn parse_router_advertisement(frame: &[u8]) -> Option<RouterAdvertisement> {
    let (source, destination, protocol, payload, hop_limit) = parse_ipv6(frame)?;
    if protocol != IPPROTO_ICMPV6
        || hop_limit != 255
        || payload.len() < 16
        || payload[0] != 134
        || payload[1] != 0
        || get16(&payload[6..8]) == 0
        || source[0] != 0xfe
        || source[1] & 0xc0 != 0x80
        || transport_checksum_v6(source, destination, IPPROTO_ICMPV6, payload) != 0
    {
        return None;
    }
    let router_mac: [u8; 6] = frame[6..12].try_into().ok()?;
    if router_mac == [0; 6] || router_mac == [0xff; 6] || router_mac[0] & 1 != 0 {
        return None;
    }
    let mut prefix = None;
    let mut cursor = 16usize;
    while cursor < payload.len() {
        let option_type = payload[cursor];
        let units = usize::from(*payload.get(cursor + 1)?);
        let option_length = units.checked_mul(8)?;
        if option_length == 0 || cursor.checked_add(option_length)? > payload.len() {
            return None;
        }
        let option = &payload[cursor..cursor + option_length];
        if option_type == 1 && option_length >= 8 {
            if option[2..8] != router_mac {
                return None;
            }
        } else if option_type == 3
            && option_length == 32
            && option[2] == 64
            && option[3] & 0x40 != 0
            && get32(&option[4..8]) != 0
        {
            let candidate: [u8; 16] = option[16..32].try_into().ok()?;
            if candidate != [0; 16] && candidate[0] != 0xff {
                prefix = Some(candidate);
            }
        }
        cursor += option_length;
    }
    Some(RouterAdvertisement {
        router_ip: source,
        router_mac,
        prefix: prefix?,
        prefix_length: 64,
    })
}

pub fn build_neighbor_solicitation(
    output: &mut [u8],
    mac: [u8; 6],
    source: [u8; 16],
    target: [u8; 16],
) -> Option<usize> {
    let destination = solicited_node_multicast(target);
    let length = ETHERNET_HEADER + IPV6_HEADER + 32;
    if output.len() < length || target == [0; 16] || target[0] == 0xff {
        return None;
    }
    output[..length].fill(0);
    write_ipv6_envelope(
        output,
        mac,
        multicast_mac(destination),
        source,
        destination,
        IPPROTO_ICMPV6,
        32,
        255,
    )?;
    let icmp = &mut output[ETHERNET_HEADER + IPV6_HEADER..length];
    icmp[0] = 135;
    icmp[8..24].copy_from_slice(&target);
    icmp[24] = 1;
    icmp[25] = 1;
    icmp[26..32].copy_from_slice(&mac);
    let checksum = transport_checksum_v6(source, destination, IPPROTO_ICMPV6, icmp);
    put16(&mut icmp[2..4], checksum);
    Some(length)
}

pub fn parse_neighbor_advertisement(
    frame: &[u8],
    local_ip: [u8; 16],
    expected_target: [u8; 16],
) -> Option<[u8; 6]> {
    let (source, destination, protocol, payload, hop_limit) = parse_ipv6(frame)?;
    if protocol != IPPROTO_ICMPV6
        || hop_limit != 255
        || destination != local_ip
        || payload.len() < 24
        || payload[0] != 136
        || payload[1] != 0
        || payload[4] & 0x40 == 0
        || payload[8..24] != expected_target
        || transport_checksum_v6(source, destination, IPPROTO_ICMPV6, payload) != 0
    {
        return None;
    }
    let mut cursor = 24usize;
    while cursor < payload.len() {
        let option_type = payload[cursor];
        let option_length = usize::from(*payload.get(cursor + 1)?).checked_mul(8)?;
        if option_length == 0 || cursor.checked_add(option_length)? > payload.len() {
            return None;
        }
        if option_type == 2 && option_length >= 8 {
            let mac: [u8; 6] = payload[cursor + 2..cursor + 8].try_into().ok()?;
            let ethernet_mac: [u8; 6] = frame[6..12].try_into().ok()?;
            return (mac == ethernet_mac && mac != [0; 6] && mac != [0xff; 6]).then_some(mac);
        }
        cursor += option_length;
    }
    let mac: [u8; 6] = frame[6..12].try_into().ok()?;
    (mac != [0; 6] && mac != [0xff; 6]).then_some(mac)
}

pub fn build_dhcp(
    output: &mut [u8],
    mac: [u8; 6],
    transaction: u32,
    message_type: u8,
    requested_ip: Option<[u8; 4]>,
    server: Option<[u8; 4]>,
) -> Option<usize> {
    let mut payload = [0u8; 300];
    payload[0] = 1;
    payload[1] = 1;
    payload[2] = 6;
    put32(&mut payload[4..8], transaction);
    put16(&mut payload[10..12], 0x8000);
    payload[28..34].copy_from_slice(&mac);
    payload[236..240].copy_from_slice(&[99, 130, 83, 99]);
    let mut cursor = 240;
    push_option(&mut payload, &mut cursor, 53, &[message_type])?;
    let mut client = [0u8; 7];
    client[0] = 1;
    client[1..].copy_from_slice(&mac);
    push_option(&mut payload, &mut cursor, 61, &client)?;
    push_option(&mut payload, &mut cursor, 55, &[1, 3, 6, 51, 54])?;
    if let Some(address) = requested_ip {
        push_option(&mut payload, &mut cursor, 50, &address)?;
    }
    if let Some(address) = server {
        push_option(&mut payload, &mut cursor, 54, &address)?;
    }
    *payload.get_mut(cursor)? = 255;
    cursor += 1;
    build_udp(
        output,
        mac,
        [0xff; 6],
        [0; 4],
        [255; 4],
        68,
        67,
        &payload[..cursor.max(272)],
        transaction as u16,
    )
}

pub fn parse_dhcp(frame: &[u8], transaction: u32, mac: [u8; 6]) -> Option<DhcpReply> {
    let packet = parse_udp(frame)?;
    if packet.source_port != 67 || packet.destination_port != 68 || packet.payload.len() < 240 {
        return None;
    }
    let payload = packet.payload;
    if payload[0] != 2
        || payload[1] != 1
        || payload[2] != 6
        || get32(&payload[4..8]) != transaction
        || payload[28..34] != mac
        || payload[236..240] != [99, 130, 83, 99]
    {
        return None;
    }
    let mut reply = DhcpReply {
        message_type: 0,
        address: payload[16..20].try_into().ok()?,
        subnet: [255, 255, 255, 0],
        gateway: [0; 4],
        dns: [0; 4],
        server: [0; 4],
    };
    let mut cursor = 240;
    while cursor < payload.len() {
        let code = payload[cursor];
        cursor += 1;
        if code == 255 {
            break;
        }
        if code == 0 {
            continue;
        }
        let length = usize::from(*payload.get(cursor)?);
        cursor += 1;
        let value = payload.get(cursor..cursor.checked_add(length)?)?;
        match (code, length) {
            (53, 1) => reply.message_type = value[0],
            (1, 4) => reply.subnet.copy_from_slice(value),
            (3, 4..) => reply.gateway.copy_from_slice(&value[..4]),
            (6, 4..) => reply.dns.copy_from_slice(&value[..4]),
            (54, 4) => reply.server.copy_from_slice(value),
            _ => {}
        }
        cursor += length;
    }
    if reply.message_type == 0 || reply.address == [0; 4] {
        None
    } else {
        Some(reply)
    }
}

pub fn same_subnet(left: [u8; 4], right: [u8; 4], mask: [u8; 4]) -> bool {
    (0..4).all(|index| left[index] & mask[index] == right[index] & mask[index])
}

fn parse_ipv4(frame: &[u8]) -> Option<([u8; 4], [u8; 4], u8, &[u8])> {
    if frame.len() < ETHERNET_HEADER + IPV4_MIN_HEADER
        || get16(&frame[12..14]) != ETHERTYPE_IPV4
        || frame[14] >> 4 != 4
    {
        return None;
    }
    let header_length = usize::from(frame[14] & 0x0f) * 4;
    if header_length < IPV4_MIN_HEADER || ETHERNET_HEADER + header_length > frame.len() {
        return None;
    }
    let total_length = usize::from(get16(&frame[16..18]));
    if total_length < header_length || ETHERNET_HEADER + total_length > frame.len() {
        return None;
    }
    let header = &frame[14..14 + header_length];
    if internet_checksum(header) != 0 || get16(&header[6..8]) & 0x3fff != 0 {
        return None;
    }
    let source_ip = header[12..16].try_into().ok()?;
    let destination_ip = header[16..20].try_into().ok()?;
    Some((
        source_ip,
        destination_ip,
        header[9],
        &frame[14 + header_length..14 + total_length],
    ))
}

fn parse_ipv6(frame: &[u8]) -> Option<([u8; 16], [u8; 16], u8, &[u8], u8)> {
    if frame.len() < ETHERNET_HEADER + IPV6_HEADER
        || get16(&frame[12..14]) != ETHERTYPE_IPV6
        || frame[14] >> 4 != 6
    {
        return None;
    }
    let payload_length = usize::from(get16(&frame[18..20]));
    let end = ETHERNET_HEADER
        .checked_add(IPV6_HEADER)?
        .checked_add(payload_length)?;
    if end > frame.len() {
        return None;
    }
    let source = frame[22..38].try_into().ok()?;
    let destination = frame[38..54].try_into().ok()?;
    Some((source, destination, frame[20], &frame[54..end], frame[21]))
}

#[allow(clippy::too_many_arguments)]
fn write_ipv6_envelope(
    output: &mut [u8],
    source_mac: [u8; 6],
    destination_mac: [u8; 6],
    source_ip: [u8; 16],
    destination_ip: [u8; 16],
    next_header: u8,
    payload_length: usize,
    hop_limit: u8,
) -> Option<()> {
    if output.len() < ETHERNET_HEADER + IPV6_HEADER
        || payload_length > u16::MAX as usize
        || source_ip == [0; 16]
        || destination_ip == [0; 16]
    {
        return None;
    }
    output[..6].copy_from_slice(&destination_mac);
    output[6..12].copy_from_slice(&source_mac);
    put16(&mut output[12..14], ETHERTYPE_IPV6);
    output[14] = 0x60;
    put16(&mut output[18..20], payload_length as u16);
    output[20] = next_header;
    output[21] = hop_limit;
    output[22..38].copy_from_slice(&source_ip);
    output[38..54].copy_from_slice(&destination_ip);
    Some(())
}

fn write_ipv4_header(
    output: &mut [u8],
    total_length: u16,
    identification: u16,
    protocol: u8,
    source: [u8; 4],
    destination: [u8; 4],
) {
    output.fill(0);
    output[0] = 0x45;
    put16(&mut output[2..4], total_length);
    put16(&mut output[4..6], identification);
    put16(&mut output[6..8], 0x4000);
    output[8] = 64;
    output[9] = protocol;
    output[12..16].copy_from_slice(&source);
    output[16..20].copy_from_slice(&destination);
    let checksum = internet_checksum(output);
    put16(&mut output[10..12], checksum);
}

fn transport_checksum(source: [u8; 4], destination: [u8; 4], protocol: u8, bytes: &[u8]) -> u16 {
    let mut sum = 0u32;
    sum = checksum_bytes(sum, &source);
    sum = checksum_bytes(sum, &destination);
    sum += u32::from(protocol);
    sum += bytes.len() as u32;
    finish_checksum(checksum_bytes(sum, bytes))
}

fn transport_checksum_v6(
    source: [u8; 16],
    destination: [u8; 16],
    next_header: u8,
    bytes: &[u8],
) -> u16 {
    let mut sum = checksum_bytes(0, &source);
    sum = checksum_bytes(sum, &destination);
    let length = (bytes.len() as u32).to_be_bytes();
    sum = checksum_bytes(sum, &length);
    sum += u32::from(next_header);
    finish_checksum(checksum_bytes(sum, bytes))
}

fn internet_checksum(bytes: &[u8]) -> u16 {
    finish_checksum(checksum_bytes(0, bytes))
}

fn checksum_bytes(mut sum: u32, bytes: &[u8]) -> u32 {
    let mut chunks = bytes.chunks_exact(2);
    for pair in &mut chunks {
        sum = sum.wrapping_add(u32::from(get16(pair)));
    }
    if let [last] = chunks.remainder() {
        sum = sum.wrapping_add(u32::from(*last) << 8);
    }
    sum
}

fn finish_checksum(mut sum: u32) -> u16 {
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

fn push_option(output: &mut [u8], cursor: &mut usize, code: u8, value: &[u8]) -> Option<()> {
    let end = cursor.checked_add(2)?.checked_add(value.len())?;
    if end >= output.len() || value.len() > u8::MAX as usize {
        return None;
    }
    output[*cursor] = code;
    output[*cursor + 1] = value.len() as u8;
    output[*cursor + 2..end].copy_from_slice(value);
    *cursor = end;
    Some(())
}

fn get16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}

fn get32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn put16(output: &mut [u8], value: u16) {
    output[..2].copy_from_slice(&value.to_be_bytes());
}

fn put32(output: &mut [u8], value: u32) {
    output[..4].copy_from_slice(&value.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udp_round_trip_validates_checksum() {
        let mut frame = [0; FRAME_CAPACITY];
        let length = build_udp(
            &mut frame,
            [2, 0, 0, 0, 0, 1],
            [2, 0, 0, 0, 0, 2],
            [10, 0, 2, 15],
            [10, 0, 2, 3],
            49154,
            53,
            b"dns-query",
            7,
        )
        .unwrap();
        let packet = parse_udp(&frame[..length]).unwrap();
        assert_eq!(packet.payload, b"dns-query");
        frame[40] ^= 1;
        assert!(parse_udp(&frame[..length]).is_none());
    }

    #[test]
    fn tcp_round_trip_validates_sequence_and_payload() {
        let mut frame = [0; FRAME_CAPACITY];
        let length = build_tcp(
            &mut frame,
            [2, 0, 0, 0, 0, 1],
            [2, 0, 0, 0, 0, 2],
            [10, 0, 2, 15],
            [93, 184, 216, 34],
            49155,
            80,
            100,
            200,
            0x18,
            4096,
            b"GET / HTTP/1.1\r\n\r\n",
            8,
        )
        .unwrap();
        let segment = parse_tcp(&frame[..length]).unwrap();
        assert_eq!(segment.sequence, 100);
        assert_eq!(segment.acknowledgment, 200);
        assert_eq!(segment.window, 4096);
        assert_eq!(segment.payload, b"GET / HTTP/1.1\r\n\r\n");
    }

    #[test]
    fn arp_reply_requires_expected_identity() {
        let mac = [2, 0, 0, 0, 0, 1];
        let peer = [82, 84, 0, 18, 52, 86];
        let mut frame = [0u8; 42];
        frame[..6].copy_from_slice(&mac);
        frame[6..12].copy_from_slice(&peer);
        put16(&mut frame[12..14], ETHERTYPE_ARP);
        put16(&mut frame[14..16], 1);
        put16(&mut frame[16..18], ETHERTYPE_IPV4);
        frame[18] = 6;
        frame[19] = 4;
        put16(&mut frame[20..22], 2);
        frame[22..28].copy_from_slice(&peer);
        frame[28..32].copy_from_slice(&[10, 0, 2, 2]);
        frame[32..38].copy_from_slice(&mac);
        frame[38..42].copy_from_slice(&[10, 0, 2, 15]);
        assert_eq!(
            parse_arp_reply(&frame, mac, [10, 0, 2, 15], [10, 0, 2, 2]),
            Some(peer)
        );
    }

    #[test]
    fn dhcp_discover_is_real_broadcast_datagram() {
        let mac = [2, 0, 0, 0, 0, 1];
        let mut frame = [0; FRAME_CAPACITY];
        let length = build_dhcp(&mut frame, mac, 0x4d414b42, 1, None, None).unwrap();
        let packet = parse_udp(&frame[..length]).unwrap();
        assert_eq!(packet.source_port, 68);
        assert_eq!(packet.destination_port, 67);
        assert_eq!(&packet.payload[236..240], &[99, 130, 83, 99]);
        assert!(
            packet.payload[240..]
                .windows(3)
                .any(|bytes| bytes == [53, 1, 1])
        );
    }

    #[test]
    fn subnet_route_uses_mask() {
        assert!(same_subnet(
            [10, 0, 2, 15],
            [10, 0, 2, 3],
            [255, 255, 255, 0]
        ));
        assert!(!same_subnet(
            [10, 0, 2, 15],
            [1, 1, 1, 1],
            [255, 255, 255, 0]
        ));
    }

    #[test]
    fn ipv6_udp_and_tcp_round_trip_require_pseudoheader_checksum() {
        let source = [0xfd, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 15];
        let destination = [0xfd, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 3];
        let mut frame = [0; FRAME_CAPACITY];
        let udp_length = build_udp_v6(
            &mut frame,
            [2, 0, 0, 0, 0, 15],
            [2, 0, 0, 0, 0, 3],
            source,
            destination,
            49_152,
            53,
            b"aaaa-query",
        )
        .unwrap();
        assert_eq!(
            parse_udp_v6(&frame[..udp_length]).unwrap().payload,
            b"aaaa-query"
        );
        frame[62] ^= 1;
        assert!(parse_udp_v6(&frame[..udp_length]).is_none());

        let tcp_length = build_tcp_v6(
            &mut frame,
            [2, 0, 0, 0, 0, 15],
            [2, 0, 0, 0, 0, 3],
            source,
            destination,
            49_153,
            443,
            10,
            20,
            0x18,
            4096,
            b"tls",
        )
        .unwrap();
        let tcp = parse_tcp_v6(&frame[..tcp_length]).unwrap();
        assert_eq!(
            (tcp.sequence, tcp.acknowledgment, tcp.payload),
            (10, 20, b"tls".as_slice())
        );
        assert_eq!(tcp.window, 4096);
    }

    #[test]
    fn router_advertisement_drives_eui64_slaac() {
        let guest_mac = [2, 0, 0, 0, 0, 15];
        let router_mac = [82, 84, 0, 18, 52, 86];
        let router = [
            0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0x50, 0x54, 0, 0xff, 0xfe, 0x12, 0x34, 0x56,
        ];
        let guest = link_local_from_mac(guest_mac);
        let mut frame = [0u8; FRAME_CAPACITY];
        let length = ETHERNET_HEADER + IPV6_HEADER + 48;
        write_ipv6_envelope(
            &mut frame,
            router_mac,
            guest_mac,
            router,
            guest,
            IPPROTO_ICMPV6,
            48,
            255,
        )
        .unwrap();
        let icmp = &mut frame[54..length];
        icmp[0] = 134;
        icmp[6..8].copy_from_slice(&1800u16.to_be_bytes());
        icmp[16] = 3;
        icmp[17] = 4;
        icmp[18] = 64;
        icmp[19] = 0xc0;
        icmp[20..24].copy_from_slice(&3600u32.to_be_bytes());
        icmp[32..48].copy_from_slice(&[0xfd, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let checksum = transport_checksum_v6(router, guest, IPPROTO_ICMPV6, icmp);
        put16(&mut icmp[2..4], checksum);
        let advertisement = parse_router_advertisement(&frame[..length]).unwrap();
        assert_eq!(advertisement.router_ip, router);
        assert_eq!(advertisement.router_mac, router_mac);
        assert_eq!(advertisement.prefix_length, 64);
        let mut global = advertisement.prefix;
        global[8..].copy_from_slice(&guest[8..]);
        assert_eq!(&global[..8], &[0xfd, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&global[8..], &guest[8..]);
    }

    #[test]
    fn neighbor_discovery_uses_solicited_node_multicast() {
        let mac = [2, 0, 0, 0, 0, 15];
        let source = link_local_from_mac(mac);
        let target = [
            0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0x50, 0x54, 0, 0xff, 0xfe, 0x12, 0x34, 0x56,
        ];
        let mut frame = [0u8; FRAME_CAPACITY];
        let length = build_neighbor_solicitation(&mut frame, mac, source, target).unwrap();
        let destination = solicited_node_multicast(target);
        assert_eq!(&frame[..6], &multicast_mac(destination));
        assert_eq!(&frame[38..54], &destination);
        assert_eq!(frame[54], 135);
        assert_eq!(&frame[62..78], &target);
        assert_eq!(
            transport_checksum_v6(source, destination, IPPROTO_ICMPV6, &frame[54..length]),
            0
        );

        let router_mac = [82, 84, 0, 18, 52, 86];
        let length = ETHERNET_HEADER + IPV6_HEADER + 32;
        frame[..length].fill(0);
        write_ipv6_envelope(
            &mut frame,
            router_mac,
            mac,
            target,
            source,
            IPPROTO_ICMPV6,
            32,
            255,
        )
        .unwrap();
        let icmp = &mut frame[54..length];
        icmp[0] = 136;
        icmp[4] = 0x60;
        icmp[8..24].copy_from_slice(&target);
        icmp[24] = 2;
        icmp[25] = 1;
        icmp[26..32].copy_from_slice(&router_mac);
        let checksum = transport_checksum_v6(target, source, IPPROTO_ICMPV6, icmp);
        put16(&mut icmp[2..4], checksum);
        assert_eq!(
            parse_neighbor_advertisement(&frame[..length], source, target),
            Some(router_mac)
        );
    }
}
