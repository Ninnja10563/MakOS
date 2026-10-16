#!/usr/bin/env python3
"""Production-derived HTTP-boundary diagnostic tests, not browser qualification.

All compiler inputs and patched source fixtures are private temporary files.
Host transport, channel, and callback adapters do not model Gecko lifetime,
networking, threading, or guest behavior. Source-free mode executes only the
production header helpers and checks patch-fragment policy restoration.
"""
from __future__ import annotations

import argparse
import importlib.util
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "ports/firefox"
PATCH = PORT / "patches/0066-makos-http-boundary-state.patch"
HEADER = "mfbt/MakOSRuntimeEvidence.h"
HTTP = "netwerk/protocol/http/nsHttpChannel.cpp"
spec = importlib.util.spec_from_file_location("boundary_channel_fixture", PORT / "test-runtime-channel.py")
channel = importlib.util.module_from_spec(spec)
assert spec.loader
spec.loader.exec_module(channel)


def patch_hunks():
    # Reject malformed/truncated unified diffs before extracting any fixture.
    subprocess.run(["git", "apply", "--numstat", str(PATCH)], check=True,
                   stdout=subprocess.PIPE)
    result = {}
    for section in PATCH.read_text().split("diff --git ")[1:]:
        path = section.splitlines()[0].split(" b/", 1)[1]
        assert path in (HEADER, HTTP), "bounded patch file scope"
        chunks = []
        old, new = [], []
        left = right = 0
        for line in section.splitlines()[1:]:
            if line.startswith("@@"):
                assert left == right == 0
                if old or new:
                    chunks.append(("\n".join(old) + "\n", "\n".join(new) + "\n"))
                old, new = [], []
                match = re.match(r"@@ -\d+(?:,(\d+))? \+\d+(?:,(\d+))? @@", line)
                assert match
                left, right = (int(value) if value else 1 for value in match.groups())
            elif left or right:
                line = line or " "
                assert line[0] in " +-"
                if line[0] != "+":
                    old.append(line[1:]); left -= 1
                if line[0] != "-":
                    new.append(line[1:]); right -= 1
                assert min(left, right) >= 0
        assert left == right == 0
        if old or new:
            chunks.append(("\n".join(old) + "\n", "\n".join(new) + "\n"))
        result[path] = chunks
    assert set(result) == {HEADER, HTTP}
    return result


def fixtures(source, directory):
    hunks = patch_hunks()
    if source is None:
        _, base = channel.patch_fragments()
        before = {HEADER: base[HEADER], HTTP: "".join(old for old, _ in hunks[HTTP])}
        after = {HEADER: before[HEADER], HTTP: "".join(new for _, new in hunks[HTTP])}
        for old, new in hunks[HEADER]:
            assert after[HEADER].count(old) == 1, "exact production header hunk"
            after[HEADER] = after[HEADER].replace(old, new, 1)
        return before, after
    channel.private_apply(source, directory)
    checkout = directory / "source"
    commit = dict(line.split("=", 1) for line in (PORT / "source.lock").read_text().splitlines())["FIREFOX_COMMIT"]
    target = checkout / HTTP
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(subprocess.check_output(["git", "-C", str(source), "show", f"{commit}:{HTTP}"]))
    subprocess.run(["git", "apply", f"--include={HTTP}", str(PORT / "patches/0065-makos-http-continuations.patch")], cwd=checkout, check=True)
    before = {path: (checkout / path).read_text() for path in (HEADER, HTTP)}
    subprocess.run(["git", "apply", "--check", str(PATCH)], cwd=checkout, check=True)
    subprocess.run(["git", "apply", str(PATCH)], cwd=checkout, check=True)
    after = {path: (checkout / path).read_text() for path in before}
    return before, after


TRANSPORT = r'''
#include <atomic>
#include <cassert>
#include <cerrno>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <string>
#include <thread>
#include <vector>
#include <unistd.h>
template<class T> struct Atomic {
 std::atomic<T> value; explicit Atomic(T initial):value(initial){}
 operator T()const{return value.load();}
 Atomic& operator=(T next){value.store(next);return *this;}
 bool compareExchange(T previous,T next){return value.compare_exchange_strong(previous,next);}
 T exchange(T next){return value.exchange(next);}
};
std::mutex outputMutex;
std::vector<std::string> records;
unsigned writes=0;
int fault=-2;
ssize_t diagnostic_write(int fd,const void* data,size_t length){
 std::lock_guard<std::mutex> guard(outputMutex);
 assert(fd==STDERR_FILENO && length<=256 && "bounded single transport");
 ++writes;errno=EIO;
 ssize_t result=fault==-2?static_cast<ssize_t>(length):fault;
 records.emplace_back(static_cast<const char*>(data),result>0?static_cast<size_t>(result):0);
 return result;
}
#define write diagnostic_write
namespace mozilla::makos { EMITTER }
#undef write
bool emit(const char* stage="stage",uint32_t result=0x81234567,bool actual=true,bool after=false,
          uint64_t id=28,uint32_t wait=3,uint32_t state=5,uint32_t suspend=2){
 return mozilla::makos::WriteHttpStateDiagnostic(stage,strlen(stage),result,actual,after,id,wait,state,suspend);
}
void select(uint64_t id=28,uint32_t flags=13){mozilla::makos::ObserveHttpStateChannel(id,flags);}
unsigned saturation(){unsigned n=0;for(const auto& r:records)if(r.find("MAKOS_FIREFOX_HTTP_STATE_SATURATED ")==0)++n;return n;}
'''


TRANSPORT_MAIN = r'''
int main(int argc,char** argv){
 assert(argc==2);std::string mode=argv[1];errno=EBUSY;
 if(mode=="filter" || mode=="http"){
  for(unsigned i=0;i<100;++i){select(0,13);select(29,0);select(29,4);select(29,9);assert(!emit() && !emit("other",0,true,false,29) && !emit("zero",0,true,false,0) && "unselected channel skipped");}
  assert(writes==0 && "background consumes no attempts");
  select(28,mode=="http"?11:13);select(29,13);
  assert(!emit("other",0,true,false,29) && "first selected channel immutable");
  assert(!emit("zero",0,true,false,0) && "zero channel skipped");
  assert(emit() && "selected channel accepted");
  for(unsigned i=1;i<32;++i)assert(emit());
  assert(emit()==false && "saturation is not stage success");
  for(unsigned i=0;i<100;++i){select(29,13);select(28,13);assert(!emit() && "latch never resets budget");}
  assert(writes==33 && saturation()==1 && "bounded stage and overflow attempts");
 }else if(mode=="cap" || mode=="concurrent"){
  select();auto run=[] {int saved=errno;for(unsigned i=0;i<100;++i)emit();assert(errno==saved && "thread diagnostic errno preserved");};
  if(mode=="concurrent"){std::vector<std::thread> threads;for(unsigned i=0;i<8;++i)threads.emplace_back(run);for(auto& t:threads)t.join();}else run();
  assert(writes==33 && saturation()==1 && "bounded stage and overflow attempts");
 }else if(mode=="latch-race"){
  std::vector<std::thread> threads;for(unsigned i=1;i<=8;++i)threads.emplace_back([i]{select(i,13);});for(auto& t:threads)t.join();
  uint64_t winner=mozilla::makos::HttpStateChannel();assert(winner>=1 && winner<=8 && "first channel published");
  for(unsigned i=1;i<=8;++i){select(i,13);assert(emit("race",0,true,false,i)==(i==winner) && "first selected channel immutable");}
  assert(writes==1);
 }else if(mode=="transport"){
  select();fault=1;assert(!emit());assert(writes==1 && records.back().size()==1 && "short write not retried");
  fault=-1;for(unsigned i=0;i<100;++i)assert(!emit());
  assert(writes==33 && "failed and short attempts consumed including overflow");
 }else if(mode=="invalid-stage"){
  assert(!mozilla::makos::WriteLoadDiagnostic("012345678901234567890123456789012",33,0,1,28,13));
  assert(!emit() && "invalid load stage does not select");select();
  assert(!emit("012345678901234567890123456789012") && "invalid state stage skipped");
  assert(writes==0);for(unsigned i=0;i<32;++i)assert(emit());
  assert(writes==32 && "invalid state consumes no attempts");
 }else if(mode=="fields"){
  select(UINT64_MAX);assert(emit("01234567890123456789012345678901",UINT32_MAX,true,true,UINT64_MAX,UINT32_MAX,UINT32_MAX,UINT32_MAX));
  const auto& r=records.back();assert(!r.empty() && r.back()=='\n' && r.size()<=256 && "complete bounded record");
  assert(r.find("result=0xffffffff")!=std::string::npos && r.find("channel=0xffffffffffffffff")!=std::string::npos && "exact result and channel");
  assert(r.find("result_kind=actual")!=std::string::npos && r.find("state_phase=after")!=std::string::npos && "actual result phase");
  assert(emit("neutral",0,false,false,UINT64_MAX));
  assert(records.back().find("result_kind=neutral")!=std::string::npos && records.back().find("state_phase=entry")!=std::string::npos && "neutral entry phase");
 }else assert(false);
 assert(errno==EBUSY && "diagnostic errno preserved");
}
'''


def erase(text, path):
    text, count = re.subn(r"// BEGIN MakOS HTTP boundary diagnostics\n[\s\S]*?// END MakOS HTTP boundary diagnostics\n\n", "", text)
    assert count == 1, "exact standalone diagnostic region"
    if path == HEADER:
        assert text.count("  ObserveHttpStateChannel(aChannel, aFlags);\n") == 1
        return text.replace("  ObserveHttpStateChannel(aChannel, aFlags);\n", "", 1)
    return re.sub(r"^ *MAKOS_HTTP_STATE_(?:CAPTURE|TRACE)\([\s\S]*?\);\n", "", text, flags=re.M)


def structural(before, after):
    for path in before:
        assert erase(after[path], path) == before[path], "exact diagnostic-erasure policy restoration: " + path
    emitter = channel.item(after[HEADER], "inline bool WriteHttpStateDiagnostic(")
    assert emitter.count("write(") == 1, "one production write expression"
    assert "char record[256]" in emitter
    patch = PATCH.read_text()
    assert not any(token in patch for token in ("GetAsciiSpec(", "PAGE_STOP", "TLS_OK", "HTTP_OK")), "no URL or qualification success emission"
    trace = after[HTTP].split("#  define MAKOS_HTTP_STATE_TRACE(", 1)[1].split("#else", 1)[0]
    assert not any(token in trace for token in ("mChannelId", "this", "LoadWait", "LoadIs", "LoadAsync")), "trace uses captured scalar identity and state"
    return emitter


HOST_COMMON = r'''
#include <functional>
using nsresult=uint32_t;
constexpr nsresult NS_OK=0,NS_ERROR_FAILURE=0x80004005,NS_ERROR_DOCUMENT_NOT_CACHED=0x804b001e;
#define NS_FAILED(x) (((x)&0x80000000u)!=0)
#define NS_SUCCEEDED(x) (!NS_FAILED(x))
#define LOG(...) ((void)0)
#define MOZ_ASSERT(...) ((void)0)
#define AUTO_PROFILER_FLOW_MARKER(...) ((void)0)
#define XP_MAKOS 1
MACROS
std::vector<std::string> calls;
std::function<void()> hook;
struct Config {nsresult open=0,setup=0,init=0,dispatch=0,read=0,continuation=0,callback=0;uint32_t afterWait=0;bool valid=false,partial=false;} cfg;
struct Ignored {template<class T>void operator<<(T)const{}};constexpr Ignored Unused{};
template<class T>struct Ptr {T* p=nullptr;Ptr()=default;Ptr(T* value):p(value){} T* operator->()const{return p;} operator T*()const{return p;}T* get()const{return p;}Ptr& operator=(T* value){p=value;return *this;}};
template<class T>T** getter_AddRefs(Ptr<T>& p){return &p.p;}
struct Promise {bool empty=true;bool IsEmpty()const{return empty;}};
struct URI {bool SchemeIs(const char*){calls.push_back("scheme");return true;}} uri;
struct Timer {void Cancel(){calls.push_back("timer-cancel");}} timer;
struct Pump {void Suspend(){calls.push_back("suspend");}} pump;
struct HttpTransactionShell {
 template<class T>nsresult AsyncRead(T*,Pump** output){calls.push_back("async-read");*output=&pump;if(hook)hook();return cfg.read;}
} transaction,sticky;
struct Handler {
 template<class T>nsresult InitiateTransaction(T,int){calls.push_back("initiate");return cfg.dispatch;}
 template<class T>nsresult InitiateTransactionWithStickyConn(T,int,HttpTransactionShell*){calls.push_back("sticky");return cfg.dispatch;}
} handler;
Handler* gHttpHandler=&handler;
struct nsICacheEntry{} cacheEntry;
struct InputStream {void CloseAndRelease(){calls.push_back("stream-close");}};
constexpr uint32_t LOAD_ONLY_FROM_CACHE=1;
constexpr unsigned RESPONSE_FROM_CACHE=1;
namespace glean::network {
enum class RaceCacheValidationLabel{eNotsent};
struct Counter {Counter& EnumGet(RaceCacheValidationLabel){return *this;}void Add(){calls.push_back("telemetry");}} race_cache_validation;
}
namespace mozilla::StaticPrefs {bool network_http_rcwn_enabled(){return true;}}
using mozilla::StaticPrefs::network_http_rcwn_enabled;
namespace StaticPrefs {bool network_http_rcwn_enabled(){return mozilla::StaticPrefs::network_http_rcwn_enabled();}}
'''


HOST_CLASS = r'''
NAMESPACE_BEGIN
struct nsHttpChannel {
 uint64_t mChannelId=28;
 uint32_t wait=0,mSuspendCount=0,mLoadFlags=0;
 bool mNetworkTriggered=false,mProxyRequest=false,mWaitingForProxy=false,mCanceled=false,pending=true,asyncResume=false,mRaceCacheWithNetwork=false,mDidReval=false,mIgnoreCacheEntry=false,mCacheOpenFunc=false;
 Promise mDNSBlockingPromise;
 Ptr<nsICacheEntry> mCacheEntry;
 Ptr<HttpTransactionShell> mTransaction=&transaction;
 Ptr<Pump> mTransactionPump,mCachePump;
 Ptr<Timer> mNetworkTriggerTimer;
 URI* mURI=&uri;
 int mPriority=0;unsigned mFirstResponseSource=0;nsresult mStatus=NS_ERROR_FAILURE;
 InputStream mCacheInputStream;
 uint32_t LoadWaitForCacheEntry()const{return wait;}
 bool LoadIsPending()const{return pending;}
 bool LoadAsyncResumePending()const{return asyncResume;}
 bool AwaitingCacheCallbacks()const{return wait!=0;}
 bool CachedContentIsValid()const{return cfg.valid;}
 bool LoadCachedContentIsPartial()const{return cfg.partial;}
 void SpeculativeConnect(){calls.push_back("speculate");}
 nsresult OpenCacheEntry(bool){calls.push_back("cache-open");wait=cfg.afterWait;return cfg.open;}
 nsresult ContinueConnect(){calls.push_back("continue");return cfg.continuation;}
 nsresult ReadFromCache(){calls.push_back("cache-read");return NS_OK;}
 nsresult SetupChannelForTransaction(){calls.push_back("setup");if(hook)hook();return cfg.setup;}
 nsresult InitTransaction(){calls.push_back("init");if(hook)hook();return cfg.init;}
 nsresult OnCacheEntryAvailableInternal(nsICacheEntry*,bool,nsresult status){calls.push_back("cache-callback:"+std::to_string(status));return cfg.callback;}
 void CloseCacheEntry(bool){calls.push_back("cache-close");}
 nsresult AsyncAbort(nsresult status){calls.push_back("abort:"+std::to_string(status));return NS_OK;}
 nsresult ConnectOnTailUnblock();
 nsresult TriggerNetwork();
 nsresult DoConnectActual(HttpTransactionShell*);
 nsresult DispatchTransaction(HttpTransactionShell*);
 nsresult OnCacheEntryAvailable(nsICacheEntry*,bool,nsresult);
 void SnapshotAdapter(){MAKOS_HTTP_STATE_CAPTURE(snapshot);MAKOS_HTTP_STATE_TRACE(snapshot,"bitmap",NS_OK,false,false);}
};
FUNCTIONS
NAMESPACE_END
'''


HOST_MAIN = r'''
bool has(const char* stage,uint32_t result,bool actual,bool after,uint32_t wait,uint32_t state,uint32_t suspend){
 char expected[256];int n=snprintf(expected,sizeof(expected),
  "MAKOS_FIREFOX_HTTP_STATE stage=%s result=0x%08x result_kind=%s state_phase=%s channel=0x000000000000001c wait=0x%08x state=0x%08x suspend=0x%08x scope=diagnostic-only\n",
  stage,result,actual?"actual":"neutral",after?"after":"entry",wait,state,suspend);
 assert(n>0 && static_cast<size_t>(n)<sizeof(expected));for(auto& r:records)if(r==expected)return true;return false;
}
struct Outcome {nsresult result;uint32_t wait;bool waiting,network;std::vector<std::string> calls;};
template<class T>Outcome exercise(const std::string& mode){
 T c;calls.clear();hook={};
 if(mode=="tail-race"){c.mNetworkTriggered=true;c.mWaitingForProxy=true;}
 if(mode=="tail-only")c.mLoadFlags=LOAD_ONLY_FROM_CACHE;
 if(mode=="tail-cache"){c.mRaceCacheWithNetwork=true;c.mCacheEntry=&cacheEntry;}
 if(mode=="tail-revalidate"){c.mRaceCacheWithNetwork=true;c.mCacheEntry=&cacheEntry;c.mDidReval=true;}
 if(mode=="trigger-canceled")c.mCanceled=true;
 if(mode=="trigger-already")c.mNetworkTriggered=true;
 if(mode=="trigger-proxy"){c.mProxyRequest=true;c.mNetworkTriggerTimer=&timer;}
 if(mode=="trigger-race"){c.wait=2;c.mCacheOpenFunc=true;}
 if(mode=="suspend"){c.mSuspendCount=2;c.asyncResume=true;}
 if(mode=="identity"){c.wait=3;c.mSuspendCount=2;hook=[&c]{c.mChannelId=99;c.wait=9;c.mSuspendCount=7;};}
 if(mode=="callback-not-pending")c.pending=false;
 if(mode=="callback-race"){c.mRaceCacheWithNetwork=true;c.mNetworkTriggered=true;}
 nsresult result;
 if(mode.rfind("tail",0)==0)result=c.ConnectOnTailUnblock();
 else if(mode.rfind("trigger",0)==0)result=c.TriggerNetwork();
 else if(mode.rfind("callback",0)==0)result=c.OnCacheEntryAvailable(&cacheEntry,false,NS_ERROR_FAILURE);
 else if(mode.rfind("setup",0)==0)result=c.DoConnectActual(nullptr);
 else result=c.DispatchTransaction(mode=="sticky"?&sticky:nullptr);
 hook={};return {result,c.wait,c.mWaitingForProxy,c.mNetworkTriggered,calls};
}
int main(int argc,char** argv){
 assert(argc==2);std::string mode=argv[1];select();errno=EBUSY;
 if(mode=="bitmap"){
  changed::nsHttpChannel c;c.wait=UINT32_MAX;c.mSuspendCount=UINT32_MAX;
  c.mNetworkTriggered=c.mProxyRequest=c.mWaitingForProxy=c.mCanceled=c.pending=c.asyncResume=c.mRaceCacheWithNetwork=true;
  c.mDNSBlockingPromise.empty=false;c.mCacheEntry=&cacheEntry;c.mTransaction=&transaction;c.mTransactionPump=&pump;c.mCachePump=&pump;
  c.SnapshotAdapter();assert(has("bitmap",0,false,false,UINT32_MAX,4095,UINT32_MAX) && "all twelve state bits preserved");return 0;
 }
 if(mode=="tail-wait"||mode=="tail-race")cfg.afterWait=2;
 if(mode=="tail-error"||mode=="tail-only")cfg.open=NS_ERROR_FAILURE;
 if(mode=="tail-cache")cfg.valid=true;
 if(mode=="setup-fail")cfg.setup=NS_ERROR_FAILURE;
 if(mode=="init-fail"||mode=="identity")cfg.init=NS_ERROR_FAILURE;
 if(mode=="dispatch-fail")cfg.dispatch=NS_ERROR_FAILURE;
 if(mode=="read-fail")cfg.read=NS_ERROR_FAILURE;
 if(mode=="callback-fail"||mode=="callback-race")cfg.callback=NS_ERROR_FAILURE;
 auto expected=exercise<original::nsHttpChannel>(mode);records.clear();auto actual=exercise<changed::nsHttpChannel>(mode);
 assert(actual.result==expected.result && actual.wait==expected.wait && actual.waiting==expected.waiting && actual.network==expected.network && actual.calls==expected.calls && "production branch policy unchanged");
 if(mode.rfind("tail",0)==0){
  uint32_t state=192+(mode=="tail-race"?5:0)+(mode=="tail-cache"||mode=="tail-revalidate"?2064:0);
  assert(has("cache-open-call",0,false,false,0,state,0) && "cache entry snapshot");
  assert(has("cache-open-return",cfg.open,true,true,cfg.afterWait,state,0) && "cache raw result and postcall state");
 }
 if(mode.rfind("setup",0)==0)assert(has("transaction-setup-return",cfg.setup,true,false,0,192,0) && "setup raw result");
 if(mode=="identity")assert(has("transaction-init-return",cfg.init,true,false,3,192,2) && "precallback identity and state preserved");
 if(mode=="init-fail")assert(has("transaction-init-return",cfg.init,true,false,0,192,0) && "init raw result");
 if(mode=="dispatch-fail")assert(has("transaction-dispatch-return",cfg.dispatch,true,false,0,192,0) && "dispatch raw result");
 if(mode=="read-fail")assert(has("transaction-read-return",cfg.read,true,false,0,192,0) && "read raw result");
 if(mode=="suspend"){assert(actual.calls.size()==6 && "original suspend count");assert(has("transaction-read-return",0,true,false,0,1216,2) && "entry phase remains precallback state");}
 if(mode.rfind("callback",0)==0){uint32_t state=mode=="callback-not-pending"?128:mode=="callback-race"?2241:192;assert(has("cache-callback-enter",NS_ERROR_FAILURE,true,false,0,state,0) && "cache callback input status");}
 assert(errno==EBUSY && "diagnostic errno preserved");
}
'''


def main():
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--source-dir", type=Path)
    group.add_argument("--patch-only", action="store_true")
    args = parser.parse_args()
    source = args.source_dir or ROOT / "build/ports/firefox/source"
    full = not args.patch_only and (args.source_dir is not None or (source / ".git").exists())
    with tempfile.TemporaryDirectory(prefix="makos-http-boundary-") as temporary:
        directory = Path(temporary)
        before, after = fixtures(source if full else None, directory)
        emitter = structural(before, after)
        helpers = "\n".join(channel.item(after[HEADER], declaration) for declaration in (
            "inline Atomic<uint64_t>& HttpStateChannel(",
            "inline void ObserveHttpStateChannel(",
            "inline bool WriteHttpStateDiagnostic(",
            "inline bool WriteLoadDiagnostic("))
        common = TRANSPORT.replace("EMITTER", helpers)
        code = common + TRANSPORT_MAIN

        def native(name, text):
            path = directory / (name + ".cpp")
            path.write_text(text)
            binary = directory / name
            compiler = "/usr/bin/c++" if Path("/usr/bin/c++").exists() else shutil.which("c++")
            assert compiler, "host C++ compiler required"
            subprocess.run([compiler, "-std=c++17", "-Wall", "-Wextra", "-Werror", "-O1", "-D_FORTIFY_SOURCE=2", "-pthread", str(path), "-o", str(binary)], check=True)
            return binary

        def reject(name, mutant, original, mode, assertion):
            assert mutant != original, "negative control must modify source: " + name
            print("HTTP_BOUNDARY_CONTROL_BEGIN", name, mode, flush=True)
            result = subprocess.run([str(native(name, mutant)), mode], capture_output=True, text=True, timeout=15)
            assert result.returncode != 0 and assertion in result.stderr, (name, result.stdout, result.stderr)
            print("HTTP_BOUNDARY_CONTROL_OK", name, flush=True)

        modes = ["filter", "http", "cap", "concurrent", "latch-race", "transport", "invalid-stage", "fields"]
        binary = native("transport", code)
        for mode in modes:
            print("HTTP_BOUNDARY_CASE_BEGIN", "transport", mode, flush=True)
            subprocess.run([str(binary), mode], check=True, timeout=15)
            print("HTTP_BOUNDARY_CASE_OK", "transport", mode, flush=True)
        controls = [
            ("uncapped", code.replace("prior >= 33", "prior >= 330"), "cap", "bounded stage and overflow attempts"),
            ("latch-reset", code.replace("HttpStateChannel().compareExchange(0, aChannel);", "HttpStateChannel() = aChannel;"), "filter", "first selected channel immutable"),
            ("background-select", code.replace("if (aChannel && (aFlags & 9u) == 9u && (aFlags & 6u) != 0)", "(void)aFlags;\n  if (aChannel)"), "filter", "unselected channel skipped"),
            ("zero-admitted", code.replace("aStageLength > 32 || !aChannel ||", "aStageLength > 32 ||"), "filter", "unselected channel skipped"),
            ("wrong-result", code.replace("hex(aResult, 28);", "hex(aResult & 0u, 28);"), "fields", "exact result and channel"),
            ("missing-errno", code.replace("errno = savedErrno;", "(void)savedErrno;"), "transport", "diagnostic errno preserved"),
            ("double-write", code.replace("const ssize_t written = write(STDERR_FILENO, record, length);", "(void)write(STDERR_FILENO, record, length);\n  const ssize_t written = write(STDERR_FILENO, record, length);"), "transport", "short write not retried"),
            ("phase-normalized", code.replace('append(aActualResult ? " result_kind=actual" : " result_kind=neutral");', 'append(aActualResult ? " result_kind=neutral" : " result_kind=neutral");'), "fields", "actual result phase"),
            ("phase-after-lost", code.replace('append(aAfterState ? " state_phase=after" : " state_phase=entry");', 'append(aAfterState ? " state_phase=entry" : " state_phase=entry");'), "fields", "actual result phase"),
            ("no-overflow", code.replace("if (prior >= 33)", "if (prior >= 32)"), "cap", "bounded stage and overflow attempts"),
            ("failed-attempt-refund", code.replace("return prior < 32 && written == static_cast<ssize_t>(length);", "if (written != static_cast<ssize_t>(length)) attempts = prior;\n  return prior < 32 && written == static_cast<ssize_t>(length);"), "transport", "failed and short attempts consumed including overflow"),
            ("invalid-load-selects", code.replace("  if (aStageLength > 32) {\n    return false;\n  }\n  ObserveHttpStateChannel(aChannel, aFlags);", "  ObserveHttpStateChannel(aChannel, aFlags);\n  if (aStageLength > 32) {\n    return false;\n  }"), "invalid-stage", "invalid load stage does not select"),
        ]
        for name, mutant, mode, assertion in controls:
            reject(name, mutant, code, mode, assertion)

        host_modes = []
        host_controls = []
        if full:
            region = re.search(r"// BEGIN MakOS HTTP boundary diagnostics\n[\s\S]*?// END MakOS HTTP boundary diagnostics", after[HTTP])[0]
            host = common + HOST_COMMON.replace("MACROS", region)
            signatures = (
                "nsresult nsHttpChannel::ConnectOnTailUnblock(",
                "nsresult nsHttpChannel::TriggerNetwork(",
                "nsresult nsHttpChannel::DoConnectActual(",
                "nsresult nsHttpChannel::DispatchTransaction(",
                "nsHttpChannel::OnCacheEntryAvailable(")
            for namespace, values in (("original", before), ("changed", after)):
                functions = "\n".join(channel.item(values[HTTP], signature) for signature in signatures)
                functions = functions.replace("nsHttpChannel::OnCacheEntryAvailable(", "nsresult nsHttpChannel::OnCacheEntryAvailable(")
                host += HOST_CLASS.replace("NAMESPACE_BEGIN", "namespace " + namespace + " {").replace("NAMESPACE_END", "}").replace("FUNCTIONS", functions)
            host += HOST_MAIN
            host_modes = ["tail-ok", "tail-wait", "tail-race", "tail-error", "tail-only", "tail-cache", "tail-revalidate", "trigger-ok", "trigger-canceled", "trigger-already", "trigger-proxy", "trigger-race", "setup-ok", "setup-fail", "init-fail", "dispatch-fail", "read-fail", "sticky", "suspend", "identity", "callback-ok", "callback-fail", "callback-not-pending", "callback-race", "bitmap"]
            binary = native("branches", host)
            for mode in host_modes:
                print("HTTP_BOUNDARY_CASE_BEGIN", "branches", mode, flush=True)
                subprocess.run([str(binary), mode], check=True, timeout=15)
                print("HTTP_BOUNDARY_CASE_OK", "branches", mode, flush=True)
            host_controls = [
                ("cache-status", host.replace('"cache-open-return", rv, true, true', '"cache-open-return", NS_OK, true, true'), "tail-error", "cache raw result and postcall state"),
                ("cache-state-entry", host.replace('MAKOS_HTTP_STATE_TRACE(makosCacheAfter, "cache-open-return"', 'MAKOS_HTTP_STATE_TRACE((static_cast<void>(makosCacheAfter), makosCacheEntry), "cache-open-return"'), "tail-wait", "cache raw result and postcall state"),
                ("setup-status", host.replace('"transaction-setup-return", rv, true, false', '"transaction-setup-return", NS_OK, true, false'), "setup-fail", "setup raw result"),
                ("dispatch-status", host.replace('"transaction-dispatch-return", rv, true, false', '"transaction-dispatch-return", NS_OK, true, false'), "dispatch-fail", "dispatch raw result"),
                ("read-status", host.replace('"transaction-read-return", rv, true, false', '"transaction-read-return", NS_OK, true, false'), "read-fail", "read raw result"),
                ("live-identity", host.replace('MAKOS_HTTP_STATE_TRACE(makosState, "transaction-init-return", rv, true, false);', 'MAKOS_HTTP_STATE_CAPTURE(liveState);\n  MAKOS_HTTP_STATE_TRACE(liveState, "transaction-init-return", rv, true, false);'), "identity", "precallback identity and state preserved"),
                ("callback-status", host.replace('"cache-callback-enter", status, true, false', '"cache-callback-enter", NS_OK, true, false'), "callback-fail", "cache callback input status"),
                ("state-bit-missing", host.replace("uint32_t(bool(mCachePump)) << 9", "uint32_t(bool(mCachePump)) << 8"), "bitmap", "all twelve state bits preserved"),
            ]
            for name, mutant, mode, assertion in host_controls:
                reject(name, mutant, host, mode, assertion)

        mutated = dict(after)
        mutated[HTTP] = mutated[HTTP].replace("return NS_OK;", "return NS_ERROR_FAILURE;", 1)
        assert erase(mutated[HTTP], HTTP) != before[HTTP], "policy negative control detected"
        assert "ObserveHttpStateChannel(aChannel, aFlags);" in after[HEADER]
        assert emitter.count("write(") == 1
    print("MAKOS_FIREFOX_HTTP_BOUNDARY_HOST_OK", "native_cases=" + str(len(modes) + len(host_modes)),
          "negative_controls=" + str(len(controls) + len(host_controls) + 1),
          "source=" + ("pinned-private-apply" if full else "patch-extracted-only"),
          "policy=exact-diagnostic-erasure admission=first-channel-once transport=32-plus-one-overflow",
          "functions=" + ("tail,trigger,connect-actual,dispatch,cache-callback" if full else "header-helpers-only"),
          "other-branches=structural-only host-adapters=explicit guest=not-run browser=not-qualified")


if __name__ == "__main__":
    main()
