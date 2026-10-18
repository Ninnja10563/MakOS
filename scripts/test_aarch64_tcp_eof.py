#!/usr/bin/env python3
"""Production-body TCP EOF regression. Generated fixtures only; no guest.

Historical baseline hashes are fixed. Candidate hashes are recorded per run,
not permanent acceptance pins: intentional future repairs require assertions,
not automatic rejection solely because source identity changed.
"""
import difflib
import hashlib
import json
from pathlib import Path
import re
import shutil
import sys
import argparse
import subprocess
import tempfile
import time
import types

SCRIPT = Path(__file__).resolve()
FIXTURE = SCRIPT.with_suffix('.rs')
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--repo-root', type=Path)
parser.add_argument('--source-root', type=Path)
parser.add_argument('--output', type=Path)
parser.add_argument('--source-contract', type=Path)
ARGS = parser.parse_args()
ROOT = (ARGS.repo_root or SCRIPT.parents[1]).resolve()
CANDIDATE = (ARGS.source_root or ROOT).resolve()
OUT = None
BASELINE = None
BASELINE_COMMIT = '92554836596589a89ceb39737d40e05ae35af35e'
LEDGER = []

def run(label, argv, expected=0):
    started = time.monotonic()
    try:
        proc = subprocess.run(argv, cwd=ROOT, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, timeout=60)
        output, status = proc.stdout, proc.returncode
    except subprocess.TimeoutExpired as error:
        output, status = error.stdout or b'', 124
    (OUT / (label + '.log')).write_bytes(output)
    row = dict(label=label, argv=list(map(str, argv)), status=status,
               expected=expected, seconds=round(time.monotonic()-started, 6))
    LEDGER.append(row)
    (OUT / 'commands.json').write_text(json.dumps(LEDGER, indent=2)+'\n')
    print(json.dumps(row), flush=True)
    if status != expected:
        raise RuntimeError(f'{label}: expected {expected}, got {status}')
    return output.decode(errors='replace')


def item(source, needle):
    start = source.index(needle)
    opening = source.index('{', start)
    depth = 0
    for end in range(opening, len(source)):
        depth += (source[end] == '{') - (source[end] == '}')
        if depth == 0:
            return source[start:end+1]
    raise AssertionError('unterminated item: '+needle)


def derived(source, needle):
    start = source.index(needle)
    prefix = source[:start].rstrip()
    derive = prefix[prefix.rfind('\n')+1:]
    assert derive.startswith('#[derive('), (needle, derive)
    return derive+'\n'+item(source, needle)


def closure(source, needle):
    start = source.index('with_state(|state| {', source.index(needle))
    value = item(source[start:], 'with_state(|state| {')
    assert source[start+len(value)] == ')'
    return value+')'



x = types.SimpleNamespace(run=run, item=item, derived=derived, closure=closure)

PINS = {
    'kernel/src/aarch64_socket.rs': '92650a6f488f7ac39478c83d41a2fb5b98452ce74a870858bfc6eadf3cb29a49',
    'kernel/src/aarch64_virtio_net.rs': '199e4c5ca34dcae2a42456b0c80408e00bb00f66bae79c449686c7d1719de415',
}
CASES = ('open-empty','open-data-drain','data-fin-partial','wrongseq-fin',
         'rst-distinction','local-shutdown-distinction','fin-empty-recv','fin-empty-read',
         'fin-readiness','data-fin-drain','repeat-eof','ack-failure','rst-after-fin',
         'fin-after-rst','fin-after-local','local-after-fin','fin-rst-fin',
         'data-fin-rst-drain','overflow-fin','seqwrap-fin','requested-mask','udp-isolation',
         'wrong-tuple','empty-output','sync-fin','sync-rst','sync-closed-debt')


def sha(raw): return hashlib.sha256(raw).hexdigest()


def source_contract(net):
    for name,signature in [('tcp4-ap','pub fn tcp_connect('),
                           ('tcp4-owner','fn tcp_connect_on_owner('),
                           ('tcp6-ap','pub fn tcp6_connect('),
                           ('tcp6-owner','fn tcp6_connect_on_owner(')]:
        body=x.item(net,signature)
        assert body.count('peer_eof: false,')==1, 'CONTRACT_CONSTRUCTOR_INIT_'+name


def generate(directory, socket, net, wire, readiness):
    directory.mkdir(parents=True)
    source_contract(net)
    net_items=[x.derived(net,'pub struct '+name) for name in
               ('TcpConnection','Tcp6Connection','TcpIngress')]
    net_items += [x.item(net,'pub fn '+name+'(') for name in
                  ('tcp_ingest','tcp6_ingest','tcp_update_receive_window',
                   'tcp6_update_receive_window','tcp_close','tcp6_close','tcp_is_closed',
                   'tcp6_is_closed','tcp_peer_eof','tcp6_peer_eof','tcp_receive','tcp6_receive')]
    net_items += [x.item(net,'struct OwnedTcpSegment {'),x.item(net,'impl OwnedTcpSegment {')]
    net_items += [line for line in net.splitlines() if line.startswith('const TCP_DEFAULT_RECEIVE_WINDOW:')]
    net_items += [line.replace('pub const','const',1) for line in wire.splitlines()
                  if line.startswith('pub const TCP_PAYLOAD_MAX:')]
    (directory/'production-net.rs').write_text('\n\n'.join(net_items)+'\n')
    (directory/'production-wire.rs').write_text('\n\n'.join(
        x.derived(wire,'pub struct '+name) for name in ('TcpSegment','TcpSegmentV6'))+'\n')
    declarations=socket[socket.index('pub const AF_INET:'):socket.index('pub fn create(')]
    pieces=[declarations]+[x.item(socket,needle) for needle in
        ('pub fn receive(','pub fn receive_from(','pub fn poll_events(',
         'fn resolve_index(','fn with_state<R>(')]
    pieces += [
        'fn ingest4(segment:crate::aarch64_net_wire::TcpSegment<\'_>)->Option<(u64,crate::aarch64_virtio_net::TcpIngress)>{'+
        x.closure(socket,'let ingress = if let Some(segment) = crate::aarch64_net_wire::parse_tcp(')+'}',
        'fn ingest6(segment:crate::aarch64_net_wire::TcpSegmentV6<\'_>)->Option<(u64,crate::aarch64_virtio_net::TcpIngress)>{'+
        x.closure(socket,'} else if let Some(segment) = crate::aarch64_net_wire::parse_tcp_v6(')+'}',
    ]
    (directory/'production-socket.rs').write_text('\n\n'.join(pieces)+'\n')
    constants='\n'.join(line for line in readiness.splitlines()
        if re.match(r'pub const EPOLL(?:IN|OUT|ERR|HUP|RDHUP):',line))
    assert len(constants.splitlines())==5
    (directory/'production-readiness.rs').write_text(constants+'\n')
    shutil.copyfile(FIXTURE,directory/'fixture.rs')
    (directory/'source-socket.rs').write_text(socket)
    (directory/'source-net.rs').write_text(net)


def replace_exact(source,old,new,count=1):
    assert source.count(old)==count,(old,source.count(old),count)
    return source.replace(old,new)


def mutant(label,socket,net,case,contract,families=('4','6')):
    directory=OUT/'runs'/label
    generate(directory,socket,net,WIRE,READINESS)
    x.run(label+'-compile',['rustc','--edition=2024','-Dwarnings',str(directory/'fixture.rs'),'-o',str(directory/'fixture')])
    for family in families:
        text=x.run(label+'-ip'+family,[str(directory/'fixture'),family,case],101)
        failures=re.findall(r'^assertion `[^`]+` failed: (.+)$',text,re.M)
        assert failures==[contract],(label,failures,contract)


def layout(label,socket,net):
    directory=OUT/'runs'/label
    directory.mkdir()
    types='\n'.join(x.derived(net,'pub struct '+name) for name in ('TcpConnection','Tcp6Connection'))
    declarations=socket[socket.index('pub const AF_INET:'):socket.index('static TCP_ASYNC_REPORTED:')]
    declarations += '\n'+ '\n'.join(x.derived(socket,key) for key in
        ('pub enum IpAddress {','pub struct Endpoint {','struct Socket {'))
    declarations += '\n'+x.item(socket,'struct State {')
    program='#![allow(dead_code)]\nmod aarch64_virtio_net {\n'+types+'\n}\n'+declarations+'''
fn main(){ println!("LAYOUT TcpConnection={} Tcp6Connection={} OptionTcp={} OptionTcp6={} Socket={} State={} alignTcp={} alignTcp6={} alignSocket={} alignState={}",
std::mem::size_of::<aarch64_virtio_net::TcpConnection>(),std::mem::size_of::<aarch64_virtio_net::Tcp6Connection>(),
std::mem::size_of::<Option<aarch64_virtio_net::TcpConnection>>(),std::mem::size_of::<Option<aarch64_virtio_net::Tcp6Connection>>(),
std::mem::size_of::<Socket>(),std::mem::size_of::<State>(),std::mem::align_of::<aarch64_virtio_net::TcpConnection>(),std::mem::align_of::<aarch64_virtio_net::Tcp6Connection>(),std::mem::align_of::<Socket>(),std::mem::align_of::<State>()); }
'''
    (directory/'layout.rs').write_text(program)
    x.run(label+'-compile',['rustc','--edition=2024','-Dwarnings',str(directory/'layout.rs'),'-o',str(directory/'layout')])
    return x.run(label+'-run',[str(directory/'layout')])


def main():
    global WIRE,READINESS,OUT,BASELINE
    if ARGS.output:
        OUT=ARGS.output.resolve(); OUT.mkdir(parents=True,exist_ok=False)
    else:
        evidence_root = ROOT / 'build/logs'
        evidence_root.mkdir(parents=True, exist_ok=True)
        OUT=Path(tempfile.mkdtemp(prefix='tcp-eof-host-',dir=evidence_root))
    BASELINE=OUT/'baseline'
    print(f'EVIDENCE_DIRECTORY={OUT}',flush=True)
    process_list=x.run('00-processes',['ps','-axo','pid=,ppid=,command='])
    assert not any(any(word in line for word in ('qemu-system-','boot_test_aarch64','firefox/build-makos.sh'))
                   for line in process_list.splitlines()),'active guest/test/build'
    x.run('01-head',['git','rev-parse','HEAD'])
    x.run('02-status',['git','status','--short'])
    x.run('03-host',['uname','-a'])
    host_system = x.run('04-host-system',['uname','-s']).strip()
    if host_system == 'Darwin':
        x.run('04-macos',['sw_vers'])
    x.run('05-rustc',['rustc','--version'])
    identities={}
    for relative,pin in PINS.items():
        original=x.run('baseline-source-'+Path(relative).stem,
                       ['git','show',BASELINE_COMMIT+':'+relative]).encode()
        assert sha(original)==pin,'historical baseline changed'
        (BASELINE/relative).parent.mkdir(parents=True,exist_ok=True)
        (BASELINE/relative).write_bytes(original)
        identities[relative]={'baseline':pin,'candidate':sha((CANDIDATE/relative).read_bytes()),
                              'canonical':sha((ROOT/relative).read_bytes())}
    socket=(CANDIDATE/'kernel/src/aarch64_socket.rs').read_text()
    net=(CANDIDATE/'kernel/src/aarch64_virtio_net.rs').read_text()
    WIRE=(ROOT/'kernel/src/aarch64_net_wire.rs').read_text()
    READINESS=(ROOT/'crates/readiness/src/lib.rs').read_text()
    for relative in ('kernel/src/aarch64_net_wire.rs','crates/readiness/src/lib.rs'):
        identities[relative]={'source':sha((ROOT/relative).read_bytes())}
    (OUT/'source-identities.json').write_text(json.dumps(identities,indent=2)+'\n')
    (OUT/'inputs').mkdir()
    helper_hashes = {path: sha(path.read_bytes()) for path in (SCRIPT, FIXTURE)}
    for path in (SCRIPT,FIXTURE):
        shutil.copyfile(path,OUT/'inputs'/path.name)
    (OUT/'inputs'/'wire.rs').write_text(WIRE)
    (OUT/'inputs'/'readiness.rs').write_text(READINESS)
    diff=''
    for relative in PINS:
        diff+=''.join(difflib.unified_diff((BASELINE/relative).read_text().splitlines(True),
                                         (CANDIDATE/relative).read_text().splitlines(True),
                                         fromfile='a/'+relative,tofile='b/'+relative))
    (OUT/'candidate.diff').write_text(diff)
    directory=OUT/'runs'/'candidate'
    generate(directory,socket,net,WIRE,READINESS)
    x.run('candidate-compile',['rustc','--edition=2024','-Dwarnings',str(directory/'fixture.rs'),'-o',str(directory/'fixture')])
    for family in ('4','6'):
        for case in CASES:
            output=x.run('case-ip'+family+'-'+case,[str(directory/'fixture'),family,case])
            assert f'CONTROL_OK family={family} case={case}\n' in output
    old_socket=(BASELINE/'kernel/src/aarch64_socket.rs').read_text()
    old_net=(BASELINE/'kernel/src/aarch64_virtio_net.rs').read_text()
    before=layout('layout-baseline',old_socket,old_net)
    after=layout('layout-candidate',socket,net)
    (OUT/'layout-comparison.json').write_text(json.dumps({'baseline':before,'candidate':after},indent=2)+'\n')

    eof_branch='''            } else if socket.peer_eof() {
                // Deliver every buffered byte before persistent orderly EOF.
                Some((0, socket.remote_endpoint()))
'''
    mutant('old-empty-none',replace_exact(socket,eof_branch,'            } else if false {\n                Some((0, socket.remote_endpoint()))\n'),net,
           'fin-empty-recv','CONTRACT_ORDERLY_FIN_RECEIVE_ZERO')
    generic=replace_exact(socket,'crate::aarch64_virtio_net::tcp_peer_eof(&connection)',
                          'crate::aarch64_virtio_net::tcp_is_closed(&connection)')
    generic=replace_exact(generic,'crate::aarch64_virtio_net::tcp6_peer_eof(&connection)',
                          'crate::aarch64_virtio_net::tcp6_is_closed(&connection)')
    mutant('generic-closed-rst',generic,net,'rst-distinction','CONTRACT_RST_NOT_EOF')
    mutant('generic-closed-local',generic,net,'local-shutdown-distinction','CONTRACT_LOCAL_CLOSE_NOT_EOF')
    early=replace_exact(socket,'            if available != 0 {',
        '            if socket.peer_eof() { return Some((0, socket.remote_endpoint())); }\n            if available != 0 {')
    mutant('eof-before-data',early,net,'data-fin-partial','CONTRACT_BUFFER_BEFORE_EOF')
    mutant('missing-eof-readiness',replace_exact(socket,' || socket.peer_eof()',''),net,
           'fin-readiness','CONTRACT_ORDERLY_FIN_READABLE_EOF')
    mutant('unmasked-eof-readiness',replace_exact(socket,'ready |= requested & EPOLLIN;','ready |= EPOLLIN;'),net,
           'requested-mask','CONTRACT_EOF_RESPECTS_MASK')
    no_reset=replace_exact(net,'\n        connection.peer_eof = false;\n','\n',2)
    mutant('rst-retains-eof',socket,no_reset,'rst-after-fin','CONTRACT_RST_CLEARS_EOF')
    terminal=replace_exact(net,'        if !connection.closed {\n            connection.peer_eof = true;\n        }',
                          '        connection.peer_eof = true;',2)
    mutant('terminal-rst-fin',socket,terminal,'fin-after-rst','CONTRACT_FIN_AFTER_RST_NOT_EOF')
    mutant('terminal-local-fin',socket,terminal,'fin-after-local','CONTRACT_FIN_AFTER_LOCAL_NOT_EOF')
    sequence=replace_exact(net,'\n    if segment.sequence != connection.receive_sequence {',
                           '\n    if segment.sequence == connection.receive_sequence {',2)
    mutant('wrong-sequence',socket,sequence,'wrongseq-fin','CONTRACT_WRONG_SEQUENCE_REJECTED')
    tuple_bypass=replace_exact(net,'    if segment.source_ip != connection.remote_ip',
                              '    if segment.source_ip == connection.remote_ip',2)
    mutant('wrong-tuple',socket,tuple_bypass,'wrong-tuple','CONTRACT_WRONG_TUPLE_REJECTED')
    once=replace_exact(socket,eof_branch,'''            } else if socket.peer_eof() {
                socket.tcp = None; socket.tcp6 = None;
                Some((0, socket.remote_endpoint()))
''')
    mutant('one-shot-eof',once,net,'repeat-eof','CONTRACT_EOF_WRAPPER')
    no_v6=replace_exact(net,'pub fn tcp6_peer_eof(connection: &Tcp6Connection) -> bool {\n    connection.peer_eof\n}',
        'pub fn tcp6_peer_eof(connection: &Tcp6Connection) -> bool {\n    connection.peer_eof && !connection.closed\n}')
    mutant('missing-v6-eof',socket,no_v6,'fin-empty-recv','CONTRACT_ORDERLY_FIN_RECEIVE_ZERO',('6',))
    no_sync=replace_exact(net,'                connection.peer_eof = true;\n','',2)
    mutant('missing-sync-fin',socket,no_sync,'sync-fin','CONTRACT_SYNC_FIN_METADATA')
    open_eof=replace_exact(socket,'            } else if socket.peer_eof() {','            } else if !socket.peer_eof() {')
    mutant('open-empty-eof',open_eof,net,'open-empty','CONTRACT_OPEN_EMPTY_WOULD_BLOCK')
    for name,signature in [('tcp4-ap','pub fn tcp_connect('),('tcp4-owner','fn tcp_connect_on_owner('),
                           ('tcp6-ap','pub fn tcp6_connect('),('tcp6-owner','fn tcp6_connect_on_owner(')]:
        body=x.item(net,signature)
        broken=replace_exact(net,body,body.replace('peer_eof: false,','peer_eof: true,'))
        path=OUT/'runs'/('missing-init-'+name+'.rs');path.write_text(broken)
        output=x.run('missing-init-'+name,[sys.executable,str(SCRIPT),'--source-contract',str(path)],1)
        failures=re.findall(r'^AssertionError: (.+)$',output,re.M)
        assert failures==['CONTRACT_CONSTRUCTOR_INIT_'+name],failures
    for relative,pin in PINS.items():
        assert sha((ROOT/relative).read_bytes())==identities[relative]['canonical'],'canonical source mutated during host fixture'
        assert sha((CANDIDATE/relative).read_bytes())==identities[relative]['candidate'],'candidate changed during regression'
    for relative in ('kernel/src/aarch64_net_wire.rs', 'crates/readiness/src/lib.rs'):
        assert sha((ROOT / relative).read_bytes()) == identities[relative]['source'], 'shared input changed during regression'
    for path, expected in helper_hashes.items():
        assert sha(path.read_bytes()) == expected, 'test helper changed during regression'
    print('MAKOS_AARCH64_TCP_EOF_HOST_OK cases=54 runtime_mutants=29 structural_mutants=4 '
          'canonical_source_unchanged=1 guest_qualification=0',flush=True)


if __name__=='__main__':
    if ARGS.source_contract:
        source_contract(ARGS.source_contract.read_text())
    else:
        main()
