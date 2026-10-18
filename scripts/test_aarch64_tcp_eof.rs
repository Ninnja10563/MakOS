//! Extracted production functions, explicit CPU/PID/TX adapters. No network/guest claims.
#![allow(dead_code)] // Unused fields/functions retained verbatim from production extraction.
extern crate self as makos_readiness;
include!("production-readiness.rs");
#[derive(Clone, Copy)] pub enum WaitSource { Network(u64) }
#[macro_export] macro_rules! serial_println { ($($arg:tt)*) => { println!($($arg)*); } }
fn fatal(message: &'static str) -> ! { panic!("unexpected production fatal: {message}") }
mod arch { pub fn cpu_index() -> usize { 0 } }
mod aarch64_process {
    #[derive(PartialEq)] pub enum ProcessRole { Firefox, HostFixture }
    pub fn current_app_role() -> ProcessRole { ProcessRole::HostFixture }
    pub fn current_pid() -> u64 { 1 }
    pub fn wake_io_source(_source: crate::WaitSource) {}
}
mod aarch64_net_progress_probe { pub fn on_owner_socket_wait() {} }
mod aarch64_net_wire { include!("production-wire.rs"); }
mod aarch64_virtio_net {
    use crate::aarch64_net_wire::{TcpSegment, TcpSegmentV6};
    use std::sync::Mutex;
    include!("production-net.rs");
    struct State;
    static SENDS: Mutex<Vec<(u8, u32, u32, usize)>> = Mutex::new(Vec::new());
    static TX_SUCCESS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
    static SYNC_RX: Mutex<Option<OwnedTcpSegment>> = Mutex::new(None);
    fn with_state<R>(function: impl FnOnce(&mut State) -> R) -> R { function(&mut State) }
    #[allow(clippy::too_many_arguments)]
    fn send_tcp(_state: &mut State, _mac: [u8;6], _ip: [u8;4], _local: u16,
                _remote: u16, seq: u32, ack: u32, flags: u8, _window: u16,
                data: &[u8]) -> Option<()> {
        SENDS.lock().unwrap().push((flags, seq, ack, data.len()));
        TX_SUCCESS.load(std::sync::atomic::Ordering::Relaxed).then_some(())
    }
    #[allow(clippy::too_many_arguments)]
    fn send_tcp_v6(_state: &mut State, _mac: [u8;6], _ip: [u8;16], _local: u16,
                   _remote: u16, seq: u32, ack: u32, flags: u8, _window: u16,
                   data: &[u8]) -> Option<()> {
        SENDS.lock().unwrap().push((flags, seq, ack, data.len()));
        TX_SUCCESS.load(std::sync::atomic::Ordering::Relaxed).then_some(())
    }
    fn queue_tcp4_segment(_c: &TcpConnection, _flags: u8, _data: &[u8]) -> Option<usize> {
        panic!("AP path not modeled")
    }
    fn queue_tcp6_segment(_c: &Tcp6Connection, _flags: u8, _data: &[u8]) -> Option<usize> {
        panic!("AP path not modeled")
    }
    pub fn service_tx_requests_while_waiting() { panic!("unexpected lock contention") }
    pub fn fresh4() -> TcpConnection {
        TcpConnection { remote_ip:[10,0,2,2], remote_mac:[1;6], local_port:49152,
            remote_port:443, transmit_sequence:2000, receive_sequence:1000,
            receive_window:32768, closed:false, peer_eof:false }
    }
    pub fn fresh6() -> Tcp6Connection {
        Tcp6Connection { remote_ip:[7;16], remote_mac:[1;6], local_port:49152,
            remote_port:443, transmit_sequence:2000, receive_sequence:1000,
            receive_window:32768, closed:false, peer_eof:false }
    }
    pub fn sequence4(c: &TcpConnection) -> u32 { c.receive_sequence }
    pub fn sequence6(c: &Tcp6Connection) -> u32 { c.receive_sequence }
    pub fn sends() -> Vec<(u8,u32,u32,usize)> { SENDS.lock().unwrap().clone() }
    pub fn fail_tx() { TX_SUCCESS.store(false,std::sync::atomic::Ordering::Relaxed); }
    pub fn set_sequence4(c: &mut TcpConnection, seq:u32) { c.receive_sequence=seq; }
    pub fn set_sequence6(c: &mut Tcp6Connection, seq:u32) { c.receive_sequence=seq; }
    fn wait_tcp(_s:&mut State,_ip:[u8;4],_port:u16,_local:u16)->Option<OwnedTcpSegment> {
        SYNC_RX.lock().unwrap().take()
    }
    fn wait_tcp_v6(_s:&mut State,_ip:[u8;16],_port:u16,_local:u16)->Option<OwnedTcpSegment> {
        SYNC_RX.lock().unwrap().take()
    }
    pub fn synchronous(v6:bool,case:&str) {
        let rst=case=="sync-rst";
        *SYNC_RX.lock().unwrap()=Some(OwnedTcpSegment {sequence:1000,acknowledgment:2000,
            flags:if rst {4}else{0x11},payload:[0;TCP_PAYLOAD_MAX],payload_length:0});
        let mut out=[0xa5;8];
        let (result,eof)=if v6 {
            let mut c=fresh6(); if case=="sync-closed-debt" {c.closed=true;}
            let result=tcp6_receive(&mut c,&mut out); (result,tcp6_peer_eof(&c))
        } else {
            let mut c=fresh4(); if case=="sync-closed-debt" {c.closed=true;}
            let result=tcp_receive(&mut c,&mut out); (result,tcp_peer_eof(&c))
        };
        if case=="sync-fin" {
            assert_eq!(result,Some(0)); assert_eq!(eof,true,"CONTRACT_SYNC_FIN_METADATA");
        } else if rst {
            assert_eq!(result,None); assert_eq!(eof,false,"CONTRACT_SYNC_RST_NOT_EOF");
        } else {
            assert_eq!(result,Some(0)); assert_eq!(eof,false);
            println!("EXISTING_SYNC_CLOSED_ZERO_DEBT_UNCHANGED");
        }
    }
}
mod aarch64_socket {
    use core::cell::UnsafeCell;
    use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use crate::aarch64_virtio_net as net;
    include!("production-socket.rs");
    const HANDLE: u64 = 0x101;
    fn setup(v6: bool) {
        with_state(|state| {
            let mut socket = Socket::EMPTY;
            socket.used=true; socket.generation=1; socket.handle=HANDLE; socket.owner_pid=1;
            socket.domain=if v6 { AF_INET6 } else { AF_INET } as u8;
            socket.kind=SOCK_STREAM as u8; socket.protocol=IPPROTO_TCP as u8;
            socket.connected=true; socket.remote_ip=[10,0,2,2]; socket.remote_ipv6=[7;16];
            socket.local_port=49152; socket.remote_port=443;
            if v6 { socket.tcp6=Some(net::fresh6()); } else { socket.tcp=Some(net::fresh4()); }
            state.sockets[0]=socket;
        });
    }
    fn inject(v6: bool, flags: u8, sequence: u32, payload: &[u8]) -> net::TcpIngress {
        let result=if v6 {
            ingest6(crate::aarch64_net_wire::TcpSegmentV6 {
                source_ip:[7;16], destination_ip:[8;16], source_port:443,
                destination_port:49152, sequence, acknowledgment:2000, flags,
                window:32768, payload })
        } else {
            ingest4(crate::aarch64_net_wire::TcpSegment {
                source_ip:[10,0,2,2], destination_ip:[10,0,2,15], source_port:443,
                destination_port:49152, sequence, acknowledgment:2000, flags,
                window:32768, payload })
        }.expect("production pump insertion accepted fixture tuple");
        assert_eq!(result.0,HANDLE);
        result.1
    }
    fn closed(v6: bool) -> bool {
        with_state(|s| if v6 {net::tcp6_is_closed(s.sockets[0].tcp6.as_ref().unwrap())}
                   else {net::tcp_is_closed(s.sockets[0].tcp.as_ref().unwrap())})
    }
    fn eof() -> bool { with_state(|s| s.sockets[0].peer_eof()) }
    fn set_sequence(v6: bool, seq:u32) {
        with_state(|s| if v6 {net::set_sequence6(s.sockets[0].tcp6.as_mut().unwrap(),seq)}
                   else {net::set_sequence4(s.sockets[0].tcp.as_mut().unwrap(),seq)});
    }
    fn local_close(v6: bool) {
        with_state(|s| if v6 {net::tcp6_close(s.sockets[0].tcp6.as_mut().unwrap())}
                   else {net::tcp_close(s.sockets[0].tcp.as_mut().unwrap())});
    }
    fn expect_eof() {
        let mut out=[0xa5;8];
        assert_eq!(eof(),true,"CONTRACT_PEER_FIN_METADATA");
        for _ in 0..3 {
            assert_eq!(receive_from(HANDLE,&mut out).map(|x|x.0),Some(0),"CONTRACT_EOF_PERSISTENT");
            assert_eq!(out,[0xa5;8],"CONTRACT_EOF_NO_WRITE");
            assert_eq!(receive(HANDLE,&mut out),Some(0),"CONTRACT_EOF_WRAPPER");
        }
    }
    fn sequence(v6: bool) -> u32 {
        with_state(|s| if v6 {net::sequence6(s.sockets[0].tcp6.as_ref().unwrap())}
                   else {net::sequence4(s.sockets[0].tcp.as_ref().unwrap())})
    }
    fn poll() -> u32 { poll_events(HANDLE, crate::EPOLLIN|crate::EPOLLOUT|crate::EPOLLRDHUP) }
    fn peer_fin(v6: bool, payload: &[u8]) {
        let ingress=inject(v6,0x11,1000,payload);
        assert_eq!(ingress,net::TcpIngress { length:payload.len(),closed:true });
        assert!(closed(v6)); assert_eq!(sequence(v6),1001+payload.len() as u32);
        assert!(net::sends().iter().any(|&(flags,_,ack,_)|
            flags==0x10 && ack==1001+payload.len() as u32));
        assert_eq!(poll() & (crate::EPOLLHUP|crate::EPOLLRDHUP),crate::EPOLLHUP|crate::EPOLLRDHUP);
        println!("VALID_PEER_FIN_INGESTED family={} payload={} sequence={} poll={:#x}",
            if v6 {6} else {4},payload.len(),sequence(v6),poll());
    }
    pub fn scenario(v6: bool, case: &str) {
        setup(v6); let mut out=[0xa5;16];
        match case {
            "sync-fin"|"sync-rst"|"sync-closed-debt" => {net::synchronous(v6,case);}
            "open-empty" => {
                assert!(!closed(v6)); assert_eq!(receive(HANDLE,&mut out),None,"CONTRACT_OPEN_EMPTY_WOULD_BLOCK");
                assert_eq!(poll(),crate::EPOLLOUT);
            }
            "open-data-drain" => {
                assert_eq!(inject(v6,0x10,1000,b"abc"),net::TcpIngress {length:3,closed:false});
                assert_ne!(poll() & crate::EPOLLIN,0);
                assert_eq!(receive(HANDLE,&mut out[..2]),Some(2),"CONTRACT_BUFFER_BEFORE_EOF"); assert_eq!(&out[..2],b"ab");
                assert_eq!(receive(HANDLE,&mut out),Some(1)); assert_eq!(out[0],b'c');
                assert_eq!(receive(HANDLE,&mut out),None); assert_eq!(poll(),crate::EPOLLOUT);
            }
            "data-fin-partial" => {
                peer_fin(v6,b"abc"); assert_ne!(poll() & crate::EPOLLIN,0);
                assert_eq!(receive(HANDLE,&mut out[..2]),Some(2),"CONTRACT_BUFFER_BEFORE_EOF"); assert_eq!(&out[..2],b"ab");
                assert_eq!(receive(HANDLE,&mut out),Some(1)); assert_eq!(out[0],b'c');
                assert!(closed(v6));
            }
            "wrongseq-fin" => {
                assert_eq!(inject(v6,0x11,999,b""),net::TcpIngress {length:0,closed:false},"CONTRACT_WRONG_SEQUENCE_REJECTED");
                assert!(!closed(v6)); assert_eq!(sequence(v6),1000);
                assert_eq!(receive(HANDLE,&mut out),None); assert_eq!(poll(),crate::EPOLLOUT);
            }
            "rst-distinction" => {
                assert_eq!(inject(v6,0x04,1000,b""),net::TcpIngress {length:0,closed:true});
                assert!(closed(v6)); assert_eq!(sequence(v6),1000);
                assert_eq!(receive(HANDLE,&mut out),None,"CONTRACT_RST_NOT_EOF");
                assert!(net::sends().is_empty());
                println!("EXISTING_RST_STATE closed=1 peer_fin=0 receive=None no_EOF_claim=1");
            }
            "local-shutdown-distinction" => {
                local_close(v6);
                assert!(closed(v6)); assert_eq!(sequence(v6),1000);
                assert_eq!(receive(HANDLE,&mut out),None,"CONTRACT_LOCAL_CLOSE_NOT_EOF");
                assert!(net::sends().iter().any(|&(flags,_,_,_)|flags==0x11));
                println!("EXISTING_LOCAL_CLOSE_STATE closed=1 peer_fin=0 receive=None no_EOF_claim=1");
            }
            "fin-empty-recv" => {
                peer_fin(v6,b"");
                let got=receive_from(HANDLE,&mut out);
                println!("ACTUAL_RECEIVE_FROM={got:?}");
                assert_eq!(got.map(|v|v.0),Some(0),"CONTRACT_ORDERLY_FIN_RECEIVE_ZERO");
            }
            "fin-empty-read" => {
                peer_fin(v6,b"");
                let got=receive(HANDLE,&mut out); println!("ACTUAL_RECEIVE={got:?}");
                assert_eq!(got,Some(0),"CONTRACT_ORDERLY_FIN_READ_ZERO");
            }
            "fin-readiness" => {
                peer_fin(v6,b"");
                assert_ne!(poll() & crate::EPOLLIN,0,"CONTRACT_ORDERLY_FIN_READABLE_EOF");
            }
            "data-fin-drain" => {
                peer_fin(v6,b"abc");
                assert_eq!(receive(HANDLE,&mut out[..2]),Some(2)); assert_eq!(&out[..2],b"ab");
                assert_eq!(receive(HANDLE,&mut out),Some(1)); assert_eq!(out[0],b'c');
                let got=receive(HANDLE,&mut out); println!("ACTUAL_AFTER_DRAIN={got:?}");
                assert_eq!(got,Some(0),"CONTRACT_ORDERLY_FIN_DRAIN_THEN_ZERO");
            }
            "repeat-eof" => { peer_fin(v6,b""); expect_eof(); }
            "ack-failure" => {
                net::fail_tx(); peer_fin(v6,b""); expect_eof();
            }
            "rst-after-fin" => {
                peer_fin(v6,b"");
                inject(v6,0x04,sequence(v6),b"");
                assert_eq!(eof(),false,"CONTRACT_RST_CLEARS_EOF");
                assert_eq!(receive(HANDLE,&mut out),None,"CONTRACT_RST_NOT_EOF");
            }
            "fin-after-rst" => {
                inject(v6,0x04,1000,b""); inject(v6,0x11,1000,b"");
                assert_eq!(eof(),false,"CONTRACT_FIN_AFTER_RST_NOT_EOF");
                assert_eq!(receive(HANDLE,&mut out),None,"CONTRACT_RST_NOT_EOF");
            }
            "fin-after-local" => {
                local_close(v6); inject(v6,0x11,1000,b"");
                assert_eq!(eof(),false,"CONTRACT_FIN_AFTER_LOCAL_NOT_EOF");
                assert_eq!(receive(HANDLE,&mut out),None,"CONTRACT_LOCAL_CLOSE_NOT_EOF");
            }
            "local-after-fin" => { peer_fin(v6,b""); local_close(v6); expect_eof(); }
            "fin-rst-fin" => {
                peer_fin(v6,b""); inject(v6,0x04,sequence(v6),b"");
                inject(v6,0x11,sequence(v6),b"");
                assert_eq!(eof(),false,"CONTRACT_FIN_AFTER_RST_NOT_EOF");
            }
            "data-fin-rst-drain" => {
                peer_fin(v6,b"abc"); inject(v6,0x04,sequence(v6),b"");
                assert_eq!(receive(HANDLE,&mut out),Some(3)); assert_eq!(&out[..3],b"abc");
                assert_eq!(receive(HANDLE,&mut out),None,"CONTRACT_RST_NOT_EOF");
            }
            "overflow-fin" => {
                with_state(|s| {s.sockets[0].tcp_length=TCP_RECEIVE_CAPACITY;
                    s.tcp_responses[0].fill(b'x');});
                assert_eq!(inject(v6,0x11,1000,b"a"),net::TcpIngress {length:0,closed:false},"CONTRACT_OVERFLOW_FIN_NOT_ACCEPTED");
                assert_eq!(eof(),false); assert_eq!(sequence(v6),1000);
            }
            "seqwrap-fin" => {
                set_sequence(v6,u32::MAX); inject(v6,0x11,u32::MAX,b"");
                assert_eq!(sequence(v6),0,"CONTRACT_FIN_SEQUENCE_WRAP"); expect_eof();
            }
            "requested-mask" => {
                peer_fin(v6,b"");
                assert_eq!(poll_events(HANDLE,0)&crate::EPOLLIN,0,"CONTRACT_EOF_RESPECTS_MASK");
                assert_eq!(poll_events(HANDLE,crate::EPOLLOUT)&crate::EPOLLIN,0,"CONTRACT_EOF_RESPECTS_MASK");
                assert_eq!(poll_events(HANDLE,crate::EPOLLIN)&crate::EPOLLIN,crate::EPOLLIN,"CONTRACT_ORDERLY_FIN_READABLE_EOF");
            }
            "udp-isolation" => {
                with_state(|s| {let socket=&mut s.sockets[0];socket.kind=SOCK_DGRAM as u8;
                    socket.protocol=IPPROTO_UDP as u8;socket.tcp=None;socket.tcp6=None;});
                assert_eq!(receive(HANDLE,&mut out),None,"CONTRACT_UDP_EMPTY_NO_EOF");
                assert_eq!(poll()&crate::EPOLLIN,0,"CONTRACT_UDP_EMPTY_NOT_READABLE");
                with_state(|s| {s.sockets[0].udp_count=1;s.sockets[0].udp_lengths[0]=0;});
                assert_ne!(poll()&crate::EPOLLIN,0);
                assert_eq!(receive(HANDLE,&mut out),Some(0),"CONTRACT_UDP_ZERO_DATAGRAM_PRESERVED");
                assert_eq!(receive(HANDLE,&mut out),None,"CONTRACT_UDP_EMPTY_NO_EOF");
            }
            "wrong-tuple" => {
                let got=if v6 {
                    net::tcp6_ingest(&mut net::fresh6(),crate::aarch64_net_wire::TcpSegmentV6 {
                        source_ip:[9;16],destination_ip:[8;16],source_port:443,destination_port:49152,
                        sequence:1000,acknowledgment:2000,flags:0x11,window:32768,payload:b""},&mut out)
                } else {
                    net::tcp_ingest(&mut net::fresh4(),crate::aarch64_net_wire::TcpSegment {
                        source_ip:[10,0,2,3],destination_ip:[10,0,2,15],source_port:443,destination_port:49152,
                        sequence:1000,acknowledgment:2000,flags:0x11,window:32768,payload:b""},&mut out)
                };
                assert_eq!(got,None,"CONTRACT_WRONG_TUPLE_REJECTED");
            }
            "empty-output" => {
                peer_fin(v6,b""); assert_eq!(receive(HANDLE,&mut []),None);
                expect_eof();
            }
            "layout" => {
                println!("LAYOUT TcpConnection={} Tcp6Connection={} OptionTcp={} OptionTcp6={} Socket={} State={}",
                    std::mem::size_of::<net::TcpConnection>(),std::mem::size_of::<net::Tcp6Connection>(),
                    std::mem::size_of::<Option<net::TcpConnection>>(),std::mem::size_of::<Option<net::Tcp6Connection>>(),
                    std::mem::size_of::<Socket>(),std::mem::size_of::<State>());
            }
            other => panic!("unknown scenario {other}"),
        }
        println!("CONTROL_OK family={} case={case}",if v6 {6} else {4});
    }
}
fn main() {
    let args:Vec<String>=std::env::args().collect(); assert_eq!(args.len(),3);
    assert!(args[1]=="4" || args[1]=="6");
    aarch64_socket::scenario(args[1]=="6",&args[2]);
}
