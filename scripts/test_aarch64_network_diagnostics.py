#!/usr/bin/env python3
"""Production diagnostic extraction; host identity/serial/network adapters.

No sockets, packets, guest or browser qualification. The real connect policy
body is preserved byte-for-byte, not executed by the host network adapter.
Fixed erasure digests intentionally require review after policy edits.
"""
from pathlib import Path
import hashlib
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOCKET = (ROOT / "kernel/src/aarch64_socket.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
PARENT = "f3cfdfea28311811aa54f4dcf02a169886466df2"
HISTORICAL_SOCKET_SHA = "f7d5050d02ea293ca192fb00650a71cc77784321ba23572884d98de13fafd367"
# Reviewed peer-FIN receive/readiness repair, after diagnostic erasure. This
# remains an exact whole-file policy pin, not a runtime-derived expectation.
# See docs/TCP-PEER-FIN-20261018.md; no connect policy or diagnostic was changed.
SOCKET_SHA = "47bf6085e3f42410cabf61cf05bf40ff6a3b1868eab78b992d20b3ba78085f60"
# Historical architecture baseline at PARENT, before the reviewed I/O admission
# repair. The historical socket digest above predates the peer-FIN repair.
HISTORICAL_ARCH_SHA = "9b0cc57201d224a86569edaeb3e946fa6c3add7ca13df22c702419f452e1d860"
# Exact whole-file baseline after diagnostic erasure; see
# docs/IO-PROBE-ATOMICITY-20261018.md. Never derive this expectation at runtime.
ARCH_SHA = "e60de711cc5d86631eb3d37dc5c485214a8b034232cc0106af5eb026fc115593"


def block(source: str, signature: str) -> str:
    start = source.index("{", source.index(signature))
    depth = 0
    for end in range(start, len(source)):
        depth += (source[end] == "{") - (source[end] == "}")
        if depth == 0:
            return source[start + 1:end]
    raise AssertionError("unterminated production block")


CONSTANTS = "".join(line + "\n" for line in SOCKET.splitlines()
                    if re.match(r"(?:static|const) FIREFOX_(?:CONNECT|POLL_DENIAL|DENIAL).*DIAGNOSTIC", line))
assert len(CONSTANTS.splitlines()) == 5
START = SOCKET.index("pub fn connect_address(")
END = SOCKET.index("fn connect_address_untraced(")
DIAGNOSTIC = SOCKET[START:END]
CONNECT_ARM = block(ARCH, "SYS_SOCKET_CONNECT =>")
OLD_CONNECT_ARM = '''
            let address = frame.registers[1];
            let length = frame.registers[2] as usize;
            if !crate::aarch64_process::network_control_allowed()
                || !user_range_readable(address, length)
            {
                ERROR_INVALID
            } else {
                read_socket_endpoint(address, length).map_or(ERROR_INVALID, |endpoint| {
                    u64::from(crate::aarch64_socket::connect_address(
                        frame.registers[0],
                        endpoint.address,
                        endpoint.port,
                    ))
                })
            }
        '''


def restored_socket(source: str) -> str:
    for line in CONSTANTS.splitlines(keepends=True):
        assert source.count(line) == 1
        source = source.replace(line, "")
    start = source.index("pub fn connect_address(")
    end = source.index("fn connect_address_untraced(")
    return source[:start] + source[end:].replace("fn connect_address_untraced(", "pub fn connect_address(", 1)


def restored_arch(source: str) -> str:
    assert source.count("crate::aarch64_socket::trace_firefox_poll_denial(") == 2
    source, count = re.subn(r"(?m)^ +crate::aarch64_socket::trace_firefox_poll_denial\(\n(?: +[^\n]*\n)+? +\);\n", "", source)
    assert count == 2
    current = block(source, "SYS_SOCKET_CONNECT =>")
    return source.replace(current, OLD_CONNECT_ARM, 1)


def digest(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


assert digest(restored_socket(SOCKET)) == SOCKET_SHA, "original socket policy changed"
assert digest(restored_arch(ARCH)) == ARCH_SHA, "original arch policy changed"
# The replaced connect arm is separately constrained, including its original
# compound short-circuit predicate, endpoint-parser call and actual bool result.
stripped_arm, count = re.subn(r"(?m)^ +crate::aarch64_socket::trace_firefox_connect_denial\(\n(?: +[^\n]*\n)+? +\);\n", "", CONNECT_ARM)
assert count == 2
stripped_arm = stripped_arm.replace(
    """read_socket_endpoint(address, length).map_or_else(
                    || {
                        ERROR_INVALID
                    },
                    |endpoint| {
                        u64::from(crate::aarch64_socket::connect_address(
                            frame.registers[0],
                            endpoint.address,
                            endpoint.port,
                        ))
                    },
                )""",
    """read_socket_endpoint(address, length).map_or(ERROR_INVALID, |endpoint| {
                    u64::from(crate::aarch64_socket::connect_address(
                        frame.registers[0],
                        endpoint.address,
                        endpoint.port,
                    ))
                })""")
assert stripped_arm == OLD_CONNECT_ARM, "connect syscall policy changed"
assert '"permission-or-user-range"' in CONNECT_ARM and '"endpoint-parse"' in CONNECT_ARM
assert '"scheduled",\n                    frame.registers[1],\n                    timeout,\n                    wait_mask,\n                    wait_mask_present,' in ARCH
assert '"direct",\n                    frame.registers[1],\n                    timeout,\n                    frame.registers[3],\n                    frame.registers[4],' in ARCH
assert "with_state(" not in DIAGNOSTIC and "unsafe" not in DIAGNOSTIC
assert "crate::vfs" not in DIAGNOSTIC and "connect_address_untraced(" in DIAGNOSTIC
assert digest(restored_socket(SOCKET.replace("return false;", "return true;", 1))) != SOCKET_SHA
changed_eof = SOCKET.replace("Some((0, socket.remote_endpoint()))", "None", 1)
assert changed_eof != SOCKET, "peer-FIN negative control was not applied"
assert digest(restored_socket(changed_eof)) != SOCKET_SHA
assert digest(restored_arch(ARCH.replace("if count > 32", "if count > 64", 1))) != ARCH_SHA
changed_probe = ARCH.replace("begin_io_probe(makos_readiness::WaitSource::Any)",
                             "begin_io_probe(makos_readiness::WaitSource::Descriptor(0))", 1)
assert changed_probe != ARCH, "I/O admission negative control was not applied"
assert digest(restored_arch(changed_probe)) != ARCH_SHA

HOST = r'''
use std::sync::{Mutex, atomic::{AtomicBool,AtomicU64,Ordering}};
static ROLE: AtomicBool=AtomicBool::new(true);
static PID: AtomicU64=AtomicU64::new(71);
static TID: AtomicU64=AtomicU64::new(73);
static CPU: AtomicU64=AtomicU64::new(3);
static ID_QUERIES: AtomicU64=AtomicU64::new(0);
static RESULT: AtomicBool=AtomicBool::new(false);
static MUTATE_ID: AtomicBool=AtomicBool::new(false);
static INNER_CALLS: AtomicU64=AtomicU64::new(0);
static PERMITTED: AtomicBool=AtomicBool::new(true);
static READABLE: AtomicBool=AtomicBool::new(true);
static PARSE: AtomicBool=AtomicBool::new(true);
static RANGE_CALLS: AtomicU64=AtomicU64::new(0);
static PARSE_CALLS: AtomicU64=AtomicU64::new(0);
static SERIAL: Mutex<Vec<String>>=Mutex::new(Vec::new());
thread_local! { static IN_LOCK: std::cell::Cell<bool>=const { std::cell::Cell::new(false) }; }
fn unlocked() { IN_LOCK.with(|v| assert!(!v.get(),"diagnostic queried under socket lock")); }
mod arch { pub fn cpu_index()->usize { super::unlocked();super::ID_QUERIES.fetch_add(1,super::Ordering::Relaxed);super::CPU.load(super::Ordering::Relaxed) as usize } }
mod aarch64_process {
    #[derive(PartialEq)] pub enum ProcessRole {Firefox,Native}
    pub fn current_app_role()->ProcessRole {super::unlocked(); if super::ROLE.load(super::Ordering::Relaxed){ProcessRole::Firefox}else{ProcessRole::Native}}
    pub fn current_pid()->u64 {super::unlocked();super::ID_QUERIES.fetch_add(1,super::Ordering::Relaxed);super::PID.load(super::Ordering::Relaxed)}
    pub fn current_tid()->u64 {super::unlocked();super::ID_QUERIES.fetch_add(1,super::Ordering::Relaxed);super::TID.load(super::Ordering::Relaxed)}
    pub fn network_control_allowed()->bool {super::PERMITTED.load(super::Ordering::Relaxed)}
}
#[macro_export] macro_rules! serial_println {($($v:tt)*)=>{{crate::unlocked();crate::SERIAL.lock().unwrap().push(format!($($v)*));}};}
mod aarch64_socket {
    use super::{AtomicU64,Ordering};
    #[derive(Clone,Copy,Debug)] pub enum IpAddress {V4([u8;4]),V6([u8;16])}
    include!("production.rs");
    fn connect_address_untraced(handle:u64,address:IpAddress,port:u16)->bool {
        super::unlocked();
        super::INNER_CALLS.fetch_add(1,Ordering::Relaxed);
        assert_eq!(handle,0x123,"handle unchanged");assert_eq!(port,443,"port unchanged");
        match address {IpAddress::V4(v)=>assert_eq!(v,[10,0,2,2]),IpAddress::V6(v)=>assert_eq!(v,[7;16])}
        super::IN_LOCK.with(|v|v.set(true));
        if super::MUTATE_ID.load(Ordering::Relaxed) {super::TID.store(99,Ordering::Relaxed);super::CPU.store(1,Ordering::Relaxed);}
        let result=super::RESULT.load(Ordering::Relaxed);
        super::IN_LOCK.with(|v|v.set(false));result
    }
    pub fn counters()->[u64;3] {[FIREFOX_CONNECT_DIAGNOSTICS.load(Ordering::Relaxed),FIREFOX_CONNECT_DENIAL_DIAGNOSTICS.load(Ordering::Relaxed),FIREFOX_POLL_DENIAL_DIAGNOSTICS.load(Ordering::Relaxed)]}
    pub fn edge_budget() {
        let counter=AtomicU64::new(0);
        assert!(firefox_network_trace_slot(&counter,0,"zero").is_none());
        assert!(firefox_network_trace_slot(&counter,0,"zero").is_none());
        assert_eq!(counter.load(Ordering::Relaxed),1,"zero budget saturates");
        let counter=AtomicU64::new(u64::MAX);
        assert!(firefox_network_trace_slot(&counter,u64::MAX,"max").is_none());
        assert_eq!(counter.load(Ordering::Relaxed),u64::MAX,"budget must not wrap");
    }
}
struct Frame {registers:[u64;6]}
struct Endpoint {address:aarch64_socket::IpAddress,port:u16}
const ERROR_INVALID:u64=u64::MAX;
fn user_range_readable(address:u64,length:usize)->bool {RANGE_CALLS.fetch_add(1,Ordering::Relaxed);assert_eq!((address,length),(0x456,28));READABLE.load(Ordering::Relaxed)}
fn read_socket_endpoint(address:u64,length:usize)->Option<Endpoint> {PARSE_CALLS.fetch_add(1,Ordering::Relaxed);assert_eq!((address,length),(0x456,28));PARSE.load(Ordering::Relaxed).then_some(Endpoint{address:aarch64_socket::IpAddress::V4([10,0,2,2]),port:443})}
fn syscall_connect(frame:&Frame)->u64 { ARM }
fn connect()->bool {aarch64_socket::connect_address(0x123,aarch64_socket::IpAddress::V4([10,0,2,2]),443)}
fn lines(prefix:&str)->Vec<String> {SERIAL.lock().unwrap().iter().filter(|s|s.starts_with(prefix)).cloned().collect()}
fn fields(line:&str,values:&[&str]) {for v in values {assert!(line.split_whitespace().any(|p|p==*v),"missing field {v}: {line}");}}
fn main() {
match std::env::args().nth(1).unwrap().as_str() {
"role"=>{
 ROLE.store(false,Ordering::Relaxed);
 for _ in 0..100 {assert!(!connect());aarch64_socket::trace_firefox_connect_denial(1,2,"bad",u64::MAX);aarch64_socket::trace_firefox_poll_denial("direct",33,-2,7,9);}
 assert!(SERIAL.lock().unwrap().is_empty(),"non-Firefox emitted");assert_eq!(ID_QUERIES.load(Ordering::Relaxed),0);assert_eq!(aarch64_socket::counters(),[0;3]);
},
"pair"=>{
 assert!(!connect(),"actual false result changed");RESULT.store(true,Ordering::Relaxed);assert!(aarch64_socket::connect_address(0x123,aarch64_socket::IpAddress::V6([7;16]),443),"actual true result changed");
 assert_eq!(INNER_CALLS.load(Ordering::Relaxed),2,"inner must run once per request");
 let a=lines("MAKOS_FIREFOX_SOCKET_CONNECT_ATTEMPT ");let r=lines("MAKOS_FIREFOX_SOCKET_CONNECT_RESULT ");assert_eq!((a.len(),r.len()),(2,2));
 fields(&a[0],&["trace=0","pid=71","tid=73","cpu=3","handle=0x123","scope=diagnostic-only"]);
 fields(&r[0],&["trace=0","pid=71","tid=73","entry_cpu=3","result=0"]);fields(&r[1],&["trace=1","result=1"]);
 assert!(a[0].contains("remote=V4([10, 0, 2, 2]):443"));assert!(r[1].contains("remote=V6([7, 7,"));
 let all=SERIAL.lock().unwrap();assert!(all[0].contains("_ATTEMPT ")&&all[1].contains("_RESULT "),"attempt/result ordering");
},
"snapshot"=>{MUTATE_ID.store(true,Ordering::Relaxed);connect();let r=lines("MAKOS_FIREFOX_SOCKET_CONNECT_RESULT ");fields(&r[0],&["tid=73","entry_cpu=3"]);assert_eq!(ID_QUERIES.load(Ordering::Relaxed),3,"identity captured once before inner");},
"denials"=>{
 aarch64_socket::trace_firefox_connect_denial(0x123,28,"endpoint-parse",u64::MAX);
 aarch64_socket::trace_firefox_poll_denial("scheduled",33,-2,0xabc,7);
 fields(&lines("MAKOS_FIREFOX_SOCKET_CONNECT_DENIED ")[0],&["handle=0x123","length=28","reason=endpoint-parse","result=-1"]);
 fields(&lines("MAKOS_FIREFOX_SOCKET_POLL_DENIED ")[0],&["abi=scheduled","count=33","timeout=-2","wait_mask=0xabc","wait_mask_present=7","reason=compound-validation","result=-22"]);
},
"budgets"=>{
 for _ in 0..150 {connect();aarch64_socket::trace_firefox_connect_denial(1,2,"bad",u64::MAX);aarch64_socket::trace_firefox_poll_denial("direct",33,0,0,0);}
 assert_eq!(lines("MAKOS_FIREFOX_SOCKET_CONNECT_ATTEMPT ").len(),32,"connect cap32");assert_eq!(lines("MAKOS_FIREFOX_SOCKET_CONNECT_RESULT ").len(),32);
 assert_eq!(lines("MAKOS_FIREFOX_SOCKET_CONNECT_DENIED ").len(),8,"denial cap8");assert_eq!(lines("MAKOS_FIREFOX_SOCKET_POLL_DENIED ").len(),8);
 let overflow=lines("MAKOS_FIREFOX_NETWORK_DIAGNOSTIC_SATURATED ");assert_eq!(overflow.len(),3,"one overflow per budget");
 for kind in ["kind=connect","kind=connect-denial","kind=poll-denial"] {assert_eq!(overflow.iter().filter(|s|s.split_whitespace().any(|v|v==kind)).count(),1);}
 for row in overflow {fields(&row,&["further_records=omitted","scope=diagnostic-only"]);assert!(!row.contains("dropped="));}
 assert_eq!(aarch64_socket::counters(),[33,9,9],"counters saturate independently");
 let before=ID_QUERIES.load(Ordering::Relaxed);connect();assert_eq!(ID_QUERIES.load(Ordering::Relaxed),before,"exhausted identity lookup");
},
"concurrent"=>{
 let barrier=std::sync::Arc::new(std::sync::Barrier::new(17));let workers:Vec<_>=(0..16).map(|_|{let b=barrier.clone();std::thread::spawn(move||{b.wait();for _ in 0..20 {connect();}})}).collect();barrier.wait();for w in workers {w.join().unwrap();}
 let a=lines("MAKOS_FIREFOX_SOCKET_CONNECT_ATTEMPT ");let r=lines("MAKOS_FIREFOX_SOCKET_CONNECT_RESULT ");assert_eq!((a.len(),r.len()),(32,32),"concurrent cap32 pairs");
 for n in 0..32 {let key=format!("trace={n}");for rows in [&a,&r] {assert_eq!(rows.iter().filter(|s|s.split_whitespace().any(|p|p==key)).count(),1,"unique paired admission");}}
 assert_eq!(lines("MAKOS_FIREFOX_NETWORK_DIAGNOSTIC_SATURATED ").len(),1,"concurrent overflow once");assert_eq!(aarch64_socket::counters(),[33,0,0]);assert_eq!(INNER_CALLS.load(Ordering::Relaxed),320);
},
"edges"=>{aarch64_socket::edge_budget();assert_eq!(lines("MAKOS_FIREFOX_NETWORK_DIAGNOSTIC_SATURATED ").len(),1);},
"syscall"=>{
 let frame=Frame{registers:[0x123,0x456,28,0,0,0]};
 PERMITTED.store(false,Ordering::Relaxed);assert_eq!(syscall_connect(&frame),ERROR_INVALID);assert_eq!(RANGE_CALLS.load(Ordering::Relaxed),0,"permission short circuit unchanged");
 PERMITTED.store(true,Ordering::Relaxed);READABLE.store(false,Ordering::Relaxed);assert_eq!(syscall_connect(&frame),ERROR_INVALID);assert_eq!(PARSE_CALLS.load(Ordering::Relaxed),0,"range short circuit unchanged");
 READABLE.store(true,Ordering::Relaxed);PARSE.store(false,Ordering::Relaxed);assert_eq!(syscall_connect(&frame),ERROR_INVALID);assert_eq!(INNER_CALLS.load(Ordering::Relaxed),0);
 PARSE.store(true,Ordering::Relaxed);assert_eq!(syscall_connect(&frame),0,"syscall false convention unchanged");RESULT.store(true,Ordering::Relaxed);assert_eq!(syscall_connect(&frame),1,"syscall true convention unchanged");
 assert_eq!(RANGE_CALLS.load(Ordering::Relaxed),4,"no repeated range checks");assert_eq!(PARSE_CALLS.load(Ordering::Relaxed),3,"no repeated endpoint parse");assert_eq!(INNER_CALLS.load(Ordering::Relaxed),2);
 let d=lines("MAKOS_FIREFOX_SOCKET_CONNECT_DENIED ");assert_eq!(d.len(),3);fields(&d[0],&["reason=permission-or-user-range"]);fields(&d[2],&["reason=endpoint-parse"]);
},
_=>panic!("unknown case")}}
'''.replace(" ARM ", CONNECT_ARM)

with tempfile.TemporaryDirectory(prefix="makos-network-diagnostics-") as temporary:
    directory = Path(temporary)
    fixture = directory / "host.rs"
    fixture.write_text(HOST)

    def compile_fixture(name: str, code: str) -> Path:
        (directory / "production.rs").write_text(code)
        binary = directory / name
        subprocess.run(["rustc", "--edition=2024", "-Dwarnings", str(fixture), "-o", str(binary)], check=True)
        return binary

    production = CONSTANTS + DIAGNOSTIC
    binary = compile_fixture("network-diagnostics", production)
    cases = ("role", "pair", "snapshot", "denials", "budgets", "concurrent", "edges", "syscall")
    for case in cases:
        subprocess.run([str(binary), case], check=True, timeout=15)
        print(f"NETWORK_DIAGNOSTIC_CASE_OK case={case}", flush=True)

    def reject(name: str, old: str, new: str, case: str, expected: str) -> None:
        assert old in production
        mutated = production.replace(old, new, 1)
        result = subprocess.run([str(compile_fixture(name, mutated)), case], capture_output=True, text=True, timeout=15)
        assert result.returncode != 0 and expected in result.stdout + result.stderr, (
            f"{name}: intended behavioral failure absent: {result.stdout}{result.stderr}")
        print(f"NETWORK_DIAGNOSTIC_NEGATIVE_OK mutation={name} expected={expected!r}", flush=True)

    reject("ungated-role", "if crate::aarch64_process::current_app_role() !=", "if false && crate::aarch64_process::current_app_role() !=", "role", "non-Firefox emitted")
    reject("uncapped", "FIREFOX_CONNECT_DIAGNOSTIC_LIMIT: u64 = 32", "FIREFOX_CONNECT_DIAGNOSTIC_LIMIT: u64 = 1000", "budgets", "connect cap32")
    reject("missing-overflow", "if trace == limit {", "if trace > limit {", "budgets", "connect cap32")
    reject("changed-bool", "    connected\n}", "    !connected\n}", "pair", "actual false result changed")
    reject("missing-result", "result={} scope=diagnostic-only", "omitted_result={} scope=diagnostic-only", "pair", "missing field result=0")
    reject("exit-identity", "trace, pid, tid, cpu, handle, address, remote_port, u8::from(connected)", "trace, pid, { let _captured_tid = tid; crate::aarch64_process::current_tid() }, cpu, handle, address, remote_port, u8::from(connected)", "snapshot", "missing field tid=73")
    reject("poll-count", "abi, count, timeout, wait_mask, wait_mask_present,", "abi, count + 1, timeout, wait_mask, wait_mask_present,", "denials", "missing field count=33")

print("MAKOS_AARCH64_NETWORK_DIAGNOSTICS_HOST_OK cases=8 negative_controls=11 "
      "logic=production-admission,wrapper,denials,connect-svc "
      "identity,serial,inner-network=host-adapters "
      "socket_policy=exact-reviewed-peer-fin-baseline socket_sha256=" + SOCKET_SHA + " "
      "historical_socket_parent=" + PARENT + " historical_socket_sha256=" + HISTORICAL_SOCKET_SHA + " "
      "arch_policy=exact-reviewed-io-admission-baseline arch_sha256=" + ARCH_SHA + " "
      "connect_cap=32 denial_caps=8,8 overflow=once-per-budget "
      "scope=diagnostic-regression-not-guest-or-browser-qualification")
