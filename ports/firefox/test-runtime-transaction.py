#!/usr/bin/env python3
"""Host regression of production transaction diagnostics; never browser proof.

All patched/compiler inputs live in private temporary directories. Full mode
executes extracted, complete OnMsgNewTransaction and ResolveHost bodies against
their preceding-patch baseline using explicit host adapters. Other production
branches receive exact diagnostic-erasure checks only. --patch-only executes
header helpers/macros and checks contiguous patch fragments, not omitted bodies.
"""
from __future__ import annotations

import argparse
import importlib.util
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "ports/firefox"
PATCH = PORT / "patches/0067-makos-transaction-boundary.patch"
HEADER = "mfbt/MakOSRuntimeEvidence.h"
MANAGER = "netwerk/protocol/http/nsHttpConnectionMgr.cpp"
HANDLER = "netwerk/protocol/http/nsHttpHandler.cpp"
DNS = "netwerk/protocol/http/DnsAndConnectSocket.cpp"
PATHS = (HEADER, MANAGER, HANDLER, DNS)
spec = importlib.util.spec_from_file_location(
    "transaction_boundary_fixture", PORT / "test-runtime-http-boundary.py")
boundary = importlib.util.module_from_spec(spec)
assert spec.loader
spec.loader.exec_module(boundary)
item = boundary.channel.item


def patch_hunks():
    subprocess.run(["git", "apply", "--numstat", str(PATCH)], check=True,
                   stdout=subprocess.PIPE)
    result = {}
    for section in PATCH.read_text().split("diff --git ")[1:]:
        path = section.splitlines()[0].split(" b/", 1)[1]
        assert path in PATHS and path not in result, "bounded unique patch paths"
        chunks, old, new = [], [], []
        left = right = 0
        for line in section.splitlines()[1:]:
            if line.startswith("@@"):
                assert left == right == 0, "complete previous hunk"
                if old or new:
                    chunks.append(("\n".join(old) + "\n", "\n".join(new) + "\n"))
                old, new = [], []
                match = re.fullmatch(r"@@ -\d+(?:,(\d+))? \+\d+(?:,(\d+))? @@.*", line)
                assert match, "unified hunk header"
                left, right = (int(value) if value is not None else 1
                               for value in match.groups())
            elif left or right:
                assert line and line[0] in " +-", "literal unified hunk body"
                if line[0] != "+":
                    old.append(line[1:])
                    left -= 1
                if line[0] != "-":
                    new.append(line[1:])
                    right -= 1
                assert min(left, right) >= 0
        assert left == right == 0, "complete final hunk"
        if old or new:
            chunks.append(("\n".join(old) + "\n", "\n".join(new) + "\n"))
        result[path] = chunks
    assert set(result) == set(PATHS), "exact four-file scope"
    return result


def fixtures(source, directory):
    hunks = patch_hunks()
    _, preceding = boundary.fixtures(source, directory)
    if source is None:
        before = {HEADER: preceding[HEADER]}
        after = {HEADER: preceding[HEADER]}
        for old, new in hunks[HEADER]:
            assert after[HEADER].count(old) == 1, "exact preceding header hunk"
            after[HEADER] = after[HEADER].replace(old, new, 1)
        for path in PATHS[1:]:
            before[path] = "".join(old for old, _ in hunks[path])
            after[path] = "".join(new for _, new in hunks[path])
        return before, after
    checkout = directory / "source"
    lock = dict(line.split("=", 1) for line in (PORT / "source.lock").read_text().splitlines())
    for path in PATHS[1:]:
        target = checkout / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(subprocess.check_output([
            "git", "-C", str(source), "show", f"{lock['FIREFOX_COMMIT']}:{path}"]))
        # Replay only earlier patches touching this path, never canonical files.
        for patch in sorted((PORT / "patches").glob("*.patch")):
            if patch.name >= PATCH.name:
                continue
            marker = f"diff --git a/{path} b/{path}\n"
            if marker in patch.read_text():
                subprocess.run(["git", "apply", f"--include={path}", str(patch)],
                               cwd=checkout, check=True)
    before = {path: (checkout / path).read_text() for path in PATHS}
    subprocess.run(["git", "apply", "--check", str(PATCH)], cwd=checkout, check=True)
    subprocess.run(["git", "apply", str(PATCH)], cwd=checkout, check=True)
    return before, {path: (checkout / path).read_text() for path in PATHS}


def erase(text, path):
    if path == HEADER:
        text, count = re.subn(
            r"// BEGIN MakOS transaction diagnostics\n[\s\S]*?"
            r"// END MakOS transaction diagnostics\n\n", "", text)
        assert count == 1, "exact helper region"
        text, count = re.subn(
            r"\n// BEGIN MakOS transaction macros\n[\s\S]*?"
            r"// END MakOS transaction macros\n", "", text)
        assert count == 1, "exact macro region"
        return text
    include = '#include "mozilla/MakOSRuntimeEvidence.h"\n'
    assert text.count(include) == 1, "one added diagnostic include"
    text = text.replace(include, "", 1)
    text = re.sub(r"^ *MAKOS_TX_(?:CAPTURE|VALUE|TRACE|MANAGER)\([\s\S]*?\);\n",
                  "", text, flags=re.M)
    return text


def structural(before, after):
    for path in PATHS:
        assert erase(after[path], path) == before[path], \
            "exact diagnostic-erasure policy restoration: " + path
    record = item(after[HEADER], "inline bool WriteTransactionRecord(")
    assert record.count("write(") == 1 and "char record[256]" in record
    assert "getpid()" in record
    added = "\n".join(line[1:] for line in PATCH.read_text().splitlines()
                      if line.startswith("+") and not line.startswith("+++"))
    assert not any(token in added for token in (
        "GetAsciiSpec(", "GetSpec(", "TLS_OK", "PAGE_STOP", "HTTP_OK")), \
        "no URL capture or qualification marker"
    macros = re.search(r"// BEGIN MakOS transaction macros\n[\s\S]*?"
                       r"// END MakOS transaction macros", after[HEADER])[0]
    trace = macros.split("#  define MAKOS_TX_TRACE(", 1)[1].split("#  define MAKOS_TX_MANAGER", 1)[0]
    assert "->" not in trace and "this" not in trace, "trace uses captured scalars"
    assert after[HANDLER].count("MAKOS_TX_MANAGER(true);") == 1
    assert after[HANDLER].count("MAKOS_TX_MANAGER(false);") == 1
    stages = re.findall(r'MAKOS_TX_TRACE\([^,]+,\s*"([^"]+)"', added)
    assert stages and all(len(stage) <= 32 for stage in stages)
    dispatch = re.findall(
        r'rv = (?:TryDispatchTransaction|DispatchTransaction)\([^;]*\);\n\s*'
        r'MAKOS_TX_TRACE\(makosChannel, "process-dispatch-return", rv, true, 0, 0\);',
        after[MANAGER])
    assert len(dispatch) == 4, "actual dispatch results immediately follow four real calls"
    assert after[MANAGER].count('"process-dispatch-return"') == 4
    assert re.search(r'MAKOS_TX_TRACE\(makosChannel, "wildcard-connection-limit", '
                     r'NS_OK, false, 0,\s*0\);', after[MANAGER]), \
        "wildcard limit branch is neutral, not an actual call result"
    return macros


TRANSPORT = r'''
#include <algorithm>
#include <atomic>
#include <cassert>
#include <cerrno>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <functional>
#include <mutex>
#include <string>
#include <thread>
#include <vector>
#include <unistd.h>
template<class T>struct Atomic {
 std::atomic<T> value;explicit Atomic(T initial):value(initial){}
 operator T()const{return value.load();}
 Atomic& operator=(T next){value.store(next);return *this;}
 bool compareExchange(T previous,T next){return value.compare_exchange_strong(previous,next);}
 T exchange(T next){return value.exchange(next);}
};
std::mutex outputMutex;
std::vector<std::string> records;
unsigned writes=0;
std::atomic<unsigned> pidCalls{0};
int fault=-2;
constexpr uint32_t recordedPid=0x1234abcd;
pid_t diagnostic_getpid(){++pidCalls;errno=EDOM;return static_cast<pid_t>(recordedPid);}
ssize_t diagnostic_write(int fd,const void* bytes,size_t size){
 std::lock_guard<std::mutex> guard(outputMutex);
 assert(fd==STDERR_FILENO && size<=256 && "bounded single transport");
 ++writes;errno=EIO;
 ssize_t count=fault==-2?static_cast<ssize_t>(size):fault;
 assert(count<0 || static_cast<size_t>(count)<=size);
 records.emplace_back(static_cast<const char*>(bytes),count>0?static_cast<size_t>(count):0);
 return count;
}
#define write diagnostic_write
#define getpid diagnostic_getpid
namespace mozilla::makos { HELPERS }
#undef getpid
#undef write
#define XP_MAKOS 1
MACROS
void select(uint64_t id=28,uint32_t flags=13){mozilla::makos::ObserveHttpStateChannel(id,flags);}
bool tx(const char* stage="stage",uint32_t result=0x81234567,bool actual=true,
        uint64_t id=28,uint32_t arg0=0x12345678,uint32_t arg1=0x9abcdef0){
 return mozilla::makos::WriteTransactionDiagnostic(stage,strlen(stage),result,actual,id,arg0,arg1);
}
unsigned countPrefix(const char* prefix){
 unsigned count=0;for(const auto& record:records)if(record.rfind(prefix,0)==0)++count;return count;
}
std::string expected(const char* stage,uint32_t result,bool actual,uint64_t id,uint32_t arg0,uint32_t arg1){
 char buffer[256];int count=snprintf(buffer,sizeof(buffer),
 "MAKOS_FIREFOX_TRANSACTION stage=%s pid=0x%08x channel=0x%016llx result=0x%08x result_kind=%s arg0=0x%08x arg1=0x%08x scope=diagnostic-only\n",
 stage,recordedPid,static_cast<unsigned long long>(id),result,actual?"actual":"neutral",arg0,arg1);
 assert(count>0 && static_cast<size_t>(count)<sizeof(buffer));return buffer;
}
bool has(const char* stage,uint32_t result,bool actual,uint64_t id,uint32_t arg0,uint32_t arg1){
 auto value=expected(stage,result,actual,id,arg0,arg1);
 return std::find(records.begin(),records.end(),value)!=records.end();
}
struct Identity {
 uint64_t id=28;unsigned gets=0;
 uint64_t ChannelId(){++gets;errno=ERANGE;return id;}
};
'''


TRANSPORT_MAIN = r'''
int main(int argc,char** argv){
 assert(argc==2);std::string mode=argv[1];errno=EBUSY;
 if(mode=="filter" || mode=="http"){
  for(unsigned i=0;i<100;++i){
   select(0,13);select(29,0);select(29,4);select(29,9);
   assert(!tx() && !tx("zero",0,false,0) && !tx("background",0,false,29) && "unselected channel skipped");
  }
  assert(writes==0 && pidCalls==0 && "background consumes no attempts");
  select(28,mode=="http"?11:13);select(29,13);
  assert(!tx("mismatch",0,false,29) && "first selected channel immutable");
  assert(!tx("zero",0,false,0) && "zero channel skipped");
  for(unsigned i=0;i<32;++i)assert(tx());
  assert(!tx() && "saturation is not stage success");
  for(unsigned i=0;i<100;++i){select(29);assert(!tx());}
  assert(writes==33 && countPrefix("MAKOS_FIREFOX_TRANSACTION_SATURATED ")==1 && "bounded stage and overflow attempts");
 }else if(mode=="cap" || mode=="concurrent"){
  select();auto run=[] {errno=EBUSY;for(unsigned i=0;i<100;++i)tx();assert(errno==EBUSY && "thread errno preserved");};
  if(mode=="concurrent"){std::vector<std::thread> threads;for(unsigned i=0;i<8;++i)threads.emplace_back(run);for(auto& thread:threads)thread.join();}else run();
  assert(writes==33 && pidCalls==33 && countPrefix("MAKOS_FIREFOX_TRANSACTION ")==32 && countPrefix("MAKOS_FIREFOX_TRANSACTION_SATURATED ")==1 && "bounded stage and overflow attempts");
  char buffer[256];int n=snprintf(buffer,sizeof(buffer),"MAKOS_FIREFOX_TRANSACTION_SATURATED pid=0x%08x channel=0x000000000000001c cap=32 further_records=omitted scope=diagnostic-only\n",recordedPid);
  assert(n>0 && static_cast<size_t>(n)<sizeof(buffer));
  assert(std::find(records.begin(),records.end(),buffer)!=records.end() && "exact saturation fields");
 }else if(mode=="transport" || mode=="saturation-short" || mode=="saturation-error"){
  select();
  if(mode=="transport"){
   fault=1;assert(!tx());assert(writes==1 && records.back().size()==1 && "short write not retried");
   fault=-1;for(unsigned i=0;i<100;++i)assert(!tx());
   assert(writes==33 && "failed and short attempts consumed including overflow");
  }else{
   for(unsigned i=0;i<32;++i)assert(tx());
   fault=mode=="saturation-short"?1:-1;assert(!tx());fault=-2;
   for(unsigned i=0;i<100;++i)assert(!tx());
   assert(writes==33 && "failed saturation not retried");
  }
 }else if(mode=="invalid-stage"){
  const char* invalid="012345678901234567890123456789012";
  assert(strlen(invalid)==33);select();
  for(unsigned i=0;i<40;++i){assert(!tx(invalid));assert(!mozilla::makos::WriteTransactionRecord(invalid,33,0,false,28,0,0,false));}
  assert(writes==0 && pidCalls==0 && "invalid stage skipped before transport");
  for(unsigned i=0;i<32;++i)assert(tx() && "invalid stage consumes no budget");
  assert(writes==32);
 }else if(mode=="fields"){
  select(0xfedcba9876543210ULL);
  const char* longest="0123456789abcdef" "0123456789abcdef";
  assert(strlen(longest)==32 && "maximum admitted stage length");
  assert(tx(longest,0x81234567,true,0xfedcba9876543210ULL,0x12345678,0x9abcdef0));
  assert(has(longest,0x81234567,true,0xfedcba9876543210ULL,0x12345678,0x9abcdef0) && "exact all fields actual");
  assert(tx("neutral",0,false,0xfedcba9876543210ULL,UINT32_MAX,0));
  assert(has("neutral",0,false,0xfedcba9876543210ULL,UINT32_MAX,0) && "exact all fields neutral");
  assert(tx("max",UINT32_MAX,true,0xfedcba9876543210ULL,UINT32_MAX,UINT32_MAX));
  assert(has("max",UINT32_MAX,true,0xfedcba9876543210ULL,UINT32_MAX,UINT32_MAX) && "exact unsigned maxima");
 }else if(mode=="capture"){
  Identity value;unsigned evaluations=0;
  MAKOS_TX_CAPTURE(id,(++evaluations,&value));
  assert(id==28 && evaluations==1 && value.gets==1 && "identity evaluated once");
  assert(errno==EBUSY && "getter errno preserved");
  Identity* missing=nullptr;MAKOS_TX_CAPTURE(zero,missing);
  assert(zero==0 && "null capture zero");
  value.id=99;select();MAKOS_TX_TRACE(id,"captured",0,false,0,0);
  assert(has("captured",0,false,28,0,0) && "captured identity stable");
 }else if(mode=="latch-race"){
  std::vector<std::thread> threads;for(unsigned i=1;i<=8;++i)threads.emplace_back([i]{select(i);});for(auto& thread:threads)thread.join();
  uint64_t winner=mozilla::makos::HttpStateChannel();assert(winner>=1 && winner<=8);
  for(unsigned i=1;i<=8;++i){select(i);assert(tx("race",0,false,i)==(i==winner) && "latch race admits one channel");}
  assert(writes==1);
 }else if(mode=="manager-local" || mode=="manager-ipc" || mode=="manager-error" || mode=="manager-short" || mode=="manager-concurrent"){
  bool remote=mode=="manager-ipc";
  if(mode=="manager-error" || mode=="manager-short"){
   fault=mode=="manager-error"?-1:1;assert(!mozilla::makos::WriteTransactionManager(remote));fault=-2;
  }else if(mode=="manager-concurrent"){
   std::vector<std::thread> threads;for(unsigned i=0;i<8;++i)threads.emplace_back([]{errno=EBUSY;mozilla::makos::WriteTransactionManager(false);assert(errno==EBUSY);});for(auto& thread:threads)thread.join();
  }else{
   assert(mozilla::makos::WriteTransactionManager(remote) && "startup manager accepted");
   assert(has(remote?"manager-ipc":"manager-local",0,false,0,0,0) && "startup manager exact fields");
  }
  for(unsigned i=0;i<100;++i)assert(!mozilla::makos::WriteTransactionManager(!remote) && "manager once including failed writes");
  assert(writes==1 && "manager once including failed writes");
  assert(mozilla::makos::HttpStateChannel()==0 && "manager does not select channel");
  select();for(unsigned i=0;i<32;++i)assert(tx());assert(!tx());
  assert(writes==34 && "manager independent transaction budget");
 }else if(mode=="budgets"){
  select();for(unsigned i=0;i<100;++i)tx();
  assert(writes==33);
  assert(mozilla::makos::WriteTransactionManager(true));
  for(unsigned i=0;i<32;++i)assert(mozilla::makos::WriteHttpStateDiagnostic("old",3,0,false,false,28,0,0,0));
  assert(!mozilla::makos::WriteHttpStateDiagnostic("old",3,0,false,false,28,0,0,0));
  for(unsigned i=0;i<128;++i)assert(mozilla::makos::WriteLoadDiagnostic("old",3,0,17,28,13));
  assert(!mozilla::makos::WriteLoadDiagnostic("old",3,0,17,28,13));
  assert(writes==195 && "old budgets unchanged and independent");
 }else assert(false);
 assert(errno==EBUSY && "diagnostic errno preserved");
}
'''


NON_XP = r'''
#include <cassert>
#include <cstdint>
MACROS
unsigned effects=0;
uint32_t sideEffect(){++effects;return 1;}
int main(int argc,char**){
 assert(argc==2);
 MAKOS_TX_CAPTURE(unavailableCapture,nonexistentTransaction());
 MAKOS_TX_VALUE(unavailableValue,nonexistentValue());
 MAKOS_TX_VALUE(unusedValue,sideEffect());
 MAKOS_TX_TRACE(nonexistentChannel,"ignored",sideEffect(),false,sideEffect(),nonexistentArgument);
 MAKOS_TX_MANAGER(sideEffect());
 assert(effects==0 && "non-XP macros do not evaluate arguments");
}
'''


HOST_COMMON = r'''
using nsresult=uint32_t;
constexpr nsresult NS_OK=0,NS_ERROR_FAILURE=0x80004005;
constexpr unsigned NS_NET_STATUS_RESOLVING_HOST=17;
#define NS_FAILED(value) (((value)&0x80000000u)!=0)
#define LOG(...) ((void)0)
#define MOZ_DIAGNOSTIC_ASSERT(...) ((void)0)
std::vector<std::string> calls;
std::function<void()> callback;
struct Config {bool dnsPresent=true;unsigned retries=0;nsresult process=0;std::vector<nsresult> dnsResults{0};} cfg;
unsigned resolveCalls=0,retryCalls=0;
struct ARefBase{};
struct nsHttpTransaction:ARefBase {
 uint64_t id=28;
 uint64_t ChannelId(){errno=ERANGE;return id;}
 void SetPriority(int32_t priority){calls.push_back("priority:"+std::to_string(priority));}
 void Close(nsresult result){calls.push_back("close:"+std::to_string(result));}
 nsHttpTransaction* QueryHttpTransaction(){errno=ERANGE;return this;}
 void OnTransportStatus(void*,unsigned status,unsigned progress){calls.push_back("status:"+std::to_string(status)+":"+std::to_string(progress));}
};
template<class T>struct Ptr {
 T* pointer=nullptr;Ptr()=default;Ptr(T* value):pointer(value){}
 T* operator->()const{return pointer;}operator T*()const{return pointer;}
 Ptr& operator=(T* value){pointer=value;return *this;}
};
template<class T>using nsCOMPtr=Ptr<T>;
template<class T>T** getter_AddRefs(Ptr<T>& value){return &value.pointer;}
struct nsICancelable{} requestObject;
struct ConnectionInfo {unsigned GetOriginAttributes(){calls.push_back("attributes");return 41;}} connectionInfo;
int* gSocketTransportService=nullptr;
struct nsIDNSService {
 static constexpr unsigned RESOLVE_TYPE_DEFAULT=0,RESOLVE_WANT_RECORD_ON_ERROR=0x100,RESOLVE_BYPASS_CACHE=0x200;
 template<class T>nsresult AsyncResolveNative(const std::string& host,unsigned type,unsigned flags,void*,T*,int*,unsigned attributes,nsICancelable** request){
  calls.push_back("resolve:"+host+":"+std::to_string(type)+":"+std::to_string(flags)+":"+std::to_string(attributes));
  *request=&requestObject;nsresult result=cfg.dnsResults[std::min(static_cast<size_t>(resolveCalls),cfg.dnsResults.size()-1)];++resolveCalls;
  if(callback)callback();return result;
 }
} dnsService;
nsIDNSService* GetOrInitDNSService(){calls.push_back("dns-service");return cfg.dnsPresent?&dnsService:nullptr;}
'''


HOST_CLASS = r'''
namespace NAMESPACE {
struct nsHttpConnectionMgr {
 nsresult ProcessNewTransaction(nsHttpTransaction*){calls.push_back("process");if(callback)callback();return cfg.process;}
 void OnMsgNewTransaction(int32_t,ARefBase*);
};
struct DnsAndConnectSocket {
 nsHttpTransaction* mTransaction=nullptr;
 ConnectionInfo* mConnInfo=&connectionInfo;
 struct TransportSetup {
  bool mSocketTransport=false,mStreamOut=false,mIsBackup=false;
  unsigned mDnsFlags=3;
  std::string mHost="fixture.invalid";
  Ptr<nsICancelable> mDNSRequest;
  bool ShouldRetryDNS(){calls.push_back("retry");bool retry=retryCalls++<cfg.retries;if(retry)++mDnsFlags;return retry;}
  nsresult ResolveHost(DnsAndConnectSocket*);
 };
};
FUNCTIONS
}
'''


HOST_MAIN = r'''
struct Outcome {
 nsresult result;uint64_t id;unsigned flags;bool backup,request;std::vector<std::string> calls;
 bool operator==(const Outcome& other)const{return result==other.result && id==other.id && flags==other.flags && backup==other.backup && request==other.request && calls==other.calls;}
};
template<class Manager,class Socket>Outcome exercise(const std::string& mode){
 calls.clear();callback={};resolveCalls=retryCalls=0;
 nsHttpTransaction transaction;
 if(mode.rfind("message",0)==0){
  Manager manager;if(mode=="message-identity")callback=[&transaction]{transaction.id=99;};
  manager.OnMsgNewTransaction(-17,&transaction);callback={};return {0,transaction.id,0,false,false,calls};
 }
 Socket socket;socket.mTransaction=&transaction;typename Socket::TransportSetup transport;
 transport.mIsBackup=mode=="dns-backup";
 if(mode=="dns-identity")callback=[&]{transaction.id=99;transport.mDnsFlags=99;transport.mIsBackup=true;};
 nsresult result=transport.ResolveHost(&socket);callback={};
 return {result,transaction.id,transport.mDnsFlags,transport.mIsBackup,transport.mDNSRequest!=nullptr,calls};
}
int main(int argc,char** argv){
 assert(argc==2);std::string mode=argv[1];select();errno=EBUSY;
 if(mode=="message-error" || mode=="message-identity")cfg.process=NS_ERROR_FAILURE;
 if(mode=="dns-missing")cfg.dnsPresent=false;
 if(mode=="dns-error")cfg.dnsResults={NS_ERROR_FAILURE};
 if(mode=="dns-retry-ok"){cfg.dnsResults={NS_ERROR_FAILURE,NS_OK};cfg.retries=1;}
 if(mode=="dns-retry-error"){cfg.dnsResults={NS_ERROR_FAILURE};cfg.retries=2;}
 auto baseline=exercise<original::nsHttpConnectionMgr,original::DnsAndConnectSocket>(mode);
 records.clear();writes=0;
 auto actual=exercise<changed::nsHttpConnectionMgr,changed::DnsAndConnectSocket>(mode);
 assert(actual==baseline && "production control flow calls results unchanged");
 assert(errno==EBUSY && "production diagnostics preserve getter errno");
 if(mode.rfind("message",0)==0){
  assert(has("new-message",NS_OK,false,28,static_cast<uint32_t>(-17),0) && "message entry exact fields");
  assert(has("process-return",cfg.process,true,28,static_cast<uint32_t>(-17),0) && "process result and precallback identity");
  assert(records.size()==2 && "two message observations");
 }else{
  unsigned backup=mode=="dns-backup"?1:0;
  assert(has("dns-resolve-enter",NS_OK,false,28,3,backup) && "DNS entry flags backup identity");
  if(mode=="dns-missing"){
   assert(has("dns-service-missing",NS_OK,false,28,0,backup) && "DNS missing service neutral");
   assert(records.size()==2 && actual.result==NS_ERROR_FAILURE);
  }else{
   assert(has("dns-submit-call",NS_OK,false,28,3,backup) && "DNS call captured flags");
   assert(has("dns-submit-return",cfg.dnsResults[0],true,28,3,backup) && "DNS actual result and precallback scalars");
   if(mode=="dns-retry-ok")assert(has("dns-submit-return",NS_OK,true,28,4,backup) && "DNS retry actual flags");
   if(mode=="dns-retry-error")assert(has("dns-submit-return",NS_ERROR_FAILURE,true,28,5,backup) && "DNS retry final failure");
   assert(records.size()==1+2*resolveCalls && "one pair per DNS attempt");
   if(mode=="dns-error" || mode=="dns-retry-error")assert(!actual.request && "failed DNS request cleared");
  }
 }
}
'''


def mutate_function(code, declaration, old, new):
    function = item(code, declaration)
    assert old in function, "mutation target in " + declaration
    return code.replace(function, function.replace(old, new, 1), 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--source-dir", type=Path)
    group.add_argument("--patch-only", action="store_true")
    args = parser.parse_args()
    source = args.source_dir or ROOT / "build/ports/firefox/source"
    full = not args.patch_only and (args.source_dir is not None or (source / ".git").exists())
    with tempfile.TemporaryDirectory(prefix="makos-transaction-") as temporary:
        directory = Path(temporary)
        before, after = fixtures(source if full else None, directory)
        macros = structural(before, after)
        # Reproduce the rejected common-tail trace: its fabricated at-limit
        # value would be mislabeled actual despite perfect policy erasure.
        moved, removed = re.subn(
            r'^ *MAKOS_TX_TRACE\(makosChannel, "process-dispatch-return", rv, true, 0, 0\);\n',
            "", after[MANAGER], flags=re.M)
        assert removed == 4
        tail = "    rv = TryDispatchTransaction(ent, false, pendingTransInfo);\n  }\n"
        assert moved.count(tail) == 1
        moved = moved.replace(tail, tail +
            '  MAKOS_TX_TRACE(makosChannel, "process-dispatch-return", rv, true, 0, 0);\n', 1)
        assert erase(moved, MANAGER) == before[MANAGER]
        moved_after = dict(after)
        moved_after[MANAGER] = moved
        try:
            structural(before, moved_after)
        except AssertionError as error:
            assert "actual dispatch results immediately follow four real calls" in str(error)
        else:
            raise AssertionError("common-tail actual-result negative control was accepted")
        print("TRANSACTION_CONTROL_OK common-tail-actual-result structural-only", flush=True)
        declarations = (
            "inline Atomic<uint64_t>& HttpStateChannel(",
            "inline void ObserveHttpStateChannel(",
            "inline bool WriteHttpStateDiagnostic(",
            "inline bool WriteLoadDiagnostic(",
            "template <class Getter>\ninline uint64_t CaptureTransactionChannel(",
            "inline bool WriteTransactionRecord(",
            "inline bool WriteTransactionDiagnostic(",
            "inline bool WriteTransactionManager(")
        helpers = "\n".join(item(after[HEADER], declaration) for declaration in declarations)
        common = TRANSPORT.replace("HELPERS", helpers).replace("MACROS", macros)
        code = common + TRANSPORT_MAIN

        def native(name, content):
            path = directory / (name + ".cpp")
            path.write_text(content)
            binary = directory / name
            assert Path("/usr/bin/c++").is_file(), "Apple host /usr/bin/c++ required"
            subprocess.run(["/usr/bin/c++", "-std=c++17", "-Wall", "-Wextra", "-Werror",
                            "-O1", "-D_FORTIFY_SOURCE=2", "-pthread", str(path),
                            "-o", str(binary)], check=True)
            return binary

        def execute(binary, modes):
            for mode in modes:
                print("TRANSACTION_CASE_BEGIN", binary.name, mode, flush=True)
                subprocess.run([str(binary), mode], check=True, timeout=15)
                print("TRANSACTION_CASE_OK", binary.name, mode, flush=True)

        def reject(name, mutant, original, mode, assertion):
            assert mutant != original, "negative control must change source: " + name
            print("TRANSACTION_CONTROL_BEGIN", name, mode, flush=True)
            result = subprocess.run([str(native(name, mutant)), mode], capture_output=True,
                                    text=True, timeout=15)
            assert result.returncode != 0 and assertion in result.stderr, \
                (name, result.stdout, result.stderr)
            print("TRANSACTION_CONTROL_OK", name, flush=True)

        modes = ["filter", "http", "cap", "concurrent", "transport", "saturation-short",
                 "saturation-error", "invalid-stage", "fields", "capture", "latch-race",
                 "manager-local", "manager-ipc", "manager-error", "manager-short",
                 "manager-concurrent", "budgets"]
        execute(native("transport", code), modes)
        non_xp = NON_XP.replace("MACROS", macros)
        execute(native("non-xp", non_xp), ["no-side-effects"])
        reject("non-xp-evaluates", non_xp.replace(
            "#  define MAKOS_TX_MANAGER(remote) ((void)0)",
            "#  define MAKOS_TX_MANAGER(remote) ((void)(remote))"),
            non_xp, "no-side-effects", "non-XP macros do not evaluate arguments")
        diagnostic = "inline bool WriteTransactionDiagnostic("
        record = "inline bool WriteTransactionRecord("
        manager = "inline bool WriteTransactionManager("
        capture = "template <class Getter>\ninline uint64_t CaptureTransactionChannel("

        def changed(declaration, old, new):
            return mutate_function(code, declaration, old, new)

        controls = [
            ("uncapped", changed(diagnostic, "prior >= 33", "prior >= 330"), "cap", "bounded stage and overflow attempts"),
            ("no-saturation", changed(diagnostic, "prior >= 33", "prior >= 32"), "cap", "bounded stage and overflow attempts"),
            ("invalid-stage-budget", changed(diagnostic, "aStageLength > 32 || !aChannel ||", "!aChannel ||"), "invalid-stage", "invalid stage consumes no budget"),
            ("zero-admitted", changed(diagnostic, "aStageLength > 32 || !aChannel ||", "aStageLength > 32 ||"), "filter", "unselected channel skipped"),
            ("mismatch-admitted", changed(diagnostic, "aChannel != HttpStateChannel()", "(aChannel != HttpStateChannel() && false)"), "filter", "unselected channel skipped"),
            ("latch-reset", changed("inline void ObserveHttpStateChannel(", "HttpStateChannel().compareExchange(0, aChannel);", "HttpStateChannel() = aChannel;"), "filter", "first selected channel immutable"),
            ("background-selected", changed("inline void ObserveHttpStateChannel(", "if (aChannel && (aFlags & 9u) == 9u && (aFlags & 6u) != 0)", "(void)aFlags;\n  if (aChannel)"), "filter", "unselected channel skipped"),
            ("wrong-result", changed(record, "hex(aResult, 28);", "hex(aResult & 0u, 28);"), "fields", "exact all fields actual"),
            ("wrong-pid", changed(record, "hex(pid, 28);", "hex(pid & 0u, 28);"), "cap", "exact saturation fields"),
            ("wrong-arg0", changed(record, "hex(aArg0, 28);", "hex(aArg0 & 0u, 28);"), "fields", "exact all fields actual"),
            ("wrong-arg1", changed(record, "hex(aArg1, 28);", "hex(aArg1 & 0u, 28);"), "fields", "exact all fields actual"),
            ("wrong-channel", changed(record, "hex(aChannel, 60);", "hex(aChannel & 0u, 60);"), "cap", "exact saturation fields"),
            ("neutralized", changed(record, 'append(aActualResult ? " result_kind=actual" : " result_kind=neutral");', 'append(aActualResult ? " result_kind=neutral" : " result_kind=neutral");'), "fields", "exact all fields actual"),
            ("write-errno", changed(record, "errno = savedErrno;", "(void)savedErrno;"), "transport", "diagnostic errno preserved"),
            ("getter-errno", changed(capture, "errno = savedErrno;", "(void)savedErrno;"), "capture", "getter errno preserved"),
            ("double-write", changed(record, "const ssize_t written = write(STDERR_FILENO, record, length);", "(void)write(STDERR_FILENO, record, length);\n  const ssize_t written = write(STDERR_FILENO, record, length);"), "transport", "short write not retried"),
            ("failure-refund", changed(diagnostic,
                "return WriteTransactionRecord(aStage, aStageLength, aResult, aActualResult,\n                                aChannel, aArg0, aArg1, prior == 32);",
                "const bool complete = WriteTransactionRecord(aStage, aStageLength, aResult, aActualResult,\n                                aChannel, aArg0, aArg1, prior == 32);\n  if (!complete) attempts = prior;\n  return complete;"), "transport", "failed and short attempts consumed including overflow"),
            ("manager-repeat", changed(manager, "if (attempted.exchange(true))", "if (attempted.exchange(true) && false)"), "manager-local", "manager once including failed writes"),
            ("manager-variant", changed(manager, "return aRemote", "return !aRemote"), "manager-local", "startup manager exact fields"),
        ]
        # Mutants containing repeated formatting expressions deliberately change
        # one occurrence only, retaining use of all production parameters.
        for name, mutant, mode, assertion in controls:
            reject(name, mutant, code, mode, assertion)

        host_modes, host_controls = [], []
        if full:
            host = common + HOST_COMMON
            for namespace, values in (("original", before), ("changed", after)):
                functions = item(values[MANAGER], "void nsHttpConnectionMgr::OnMsgNewTransaction(")
                functions += "\n" + item(values[DNS], "nsresult DnsAndConnectSocket::TransportSetup::ResolveHost(")
                host += HOST_CLASS.replace("NAMESPACE", namespace).replace("FUNCTIONS", functions)
            host += HOST_MAIN
            host_modes = ["message-ok", "message-error", "message-identity", "dns-missing",
                          "dns-ok", "dns-error", "dns-retry-ok", "dns-retry-error",
                          "dns-backup", "dns-identity"]
            execute(native("branches", host), host_modes)
            host_controls = [
                ("process-status", host.replace('"process-return", rv, true, priority, 0', '"process-return", NS_OK, true, priority, 0'), "message-error", "process result and precallback identity"),
                ("process-live-id", host.replace('MAKOS_TX_TRACE(makosChannel, "process-return"', 'MAKOS_TX_TRACE((static_cast<void>(makosChannel), trans->ChannelId()), "process-return"'), "message-identity", "production diagnostics preserve getter errno"),
                ("dns-status", host.replace('"dns-submit-return", rv, true, makosFlags', '"dns-submit-return", NS_OK, true, makosFlags'), "dns-error", "DNS actual result and precallback scalars"),
                ("dns-live-flags", host.replace('"dns-submit-return", rv, true, makosFlags', '"dns-submit-return", rv, true, (static_cast<void>(makosFlags), mDnsFlags)'), "dns-identity", "DNS actual result and precallback scalars"),
                ("dns-live-backup", host.replace('"dns-submit-return", rv, true, makosFlags,\n                   makosBackup', '"dns-submit-return", rv, true, makosFlags,\n                   (static_cast<void>(makosBackup), mIsBackup)'), "dns-identity", "DNS actual result and precallback scalars"),
            ]
            for name, mutant, mode, assertion in host_controls:
                reject(name, mutant, host, mode, assertion)

        # Erasure must reject policy modifications in every affected file.
        for path in PATHS:
            assert erase(after[path] + "// non-diagnostic mutation\n", path) != before[path], \
                "policy negative control detected: " + path
    print("MAKOS_FIREFOX_TRANSACTION_HOST_OK",
          "native_cases=" + str(len(modes) + len(host_modes) + 1),
          "negative_controls=" + str(len(controls) + len(host_controls) + len(PATHS) + 2),
          "source=" + ("pinned-private-apply" if full else "patch-extracted-only"),
          "policy=exact-four-file-diagnostic-erasure admission=first-channel-process-local",
          "transport=32-plus-one-overflow manager=once-independent pid=host-adapter non-XP=arguments-erased",
          "functions=" + ("new-message,resolve-host,header-helpers" if full else "header-helpers-only"),
          "other-branches=structural-only host-adapters=explicit guest=not-run browser=not-qualified")


if __name__ == "__main__":
    main()
