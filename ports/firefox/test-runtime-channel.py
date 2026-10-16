#!/usr/bin/env python3
"""Private pinned-source tests of real channel diagnostics and callback bodies.

Host interface/clock/transport adapters are not Gecko or guest qualification.
No canonical checkout, release artifact, profile, provenance or guest mutation.
"""
from __future__ import annotations

import argparse
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "ports/firefox"
PATCH = PORT / "patches/0063-makos-channel-diagnostics.patch"
ADMISSION_PATCH = PORT / "patches/0064-makos-channel-admission.patch"
HEADER = "mfbt/MakOSRuntimeEvidence.h"
DOC = "docshell/base/nsDocShell.cpp"
URI = "uriloader/base/nsURILoader.cpp"
JS = "browser/components/urlbar/UrlbarInput.sys.mjs"


def item(text: str, declaration: str) -> str:
    start = text.index(declaration)
    end = text.index("{", start) + 1
    depth = 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end]


def private_apply(source: Path, directory: Path) -> tuple[dict, dict]:
    lock = dict(line.split("=", 1) for line in (PORT / "source.lock").read_text().splitlines())
    checkout = directory / "source"
    checkout.mkdir()
    subprocess.run(["git", "init", "-q", str(checkout)], check=True)
    for path in (DOC, URI, JS):
        content = subprocess.check_output([
            "git", "-C", str(source), "show", f"{lock['FIREFOX_COMMIT']}:{path}"])
        destination = checkout / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(content)
    for patch, include in (("0044-makos-navigation-status-marker.patch", DOC),
                           ("0061-makos-runtime-evidence.patch", HEADER),
                           ("0062-makos-navigation-diagnostics.patch", None)):
        subprocess.run(["git", "apply", *([f"--include={include}"] if include else []),
                        str(PORT / "patches" / patch)], cwd=checkout, check=True)
    before = {path: (checkout / path).read_text() for path in (HEADER, DOC, URI)}
    subprocess.run(["git", "apply", "--check", str(PATCH)], cwd=checkout, check=True)
    subprocess.run(["git", "apply", str(PATCH)], cwd=checkout, check=True)
    subprocess.run(["git", "apply", "--check", str(ADMISSION_PATCH)], cwd=checkout, check=True)
    subprocess.run(["git", "apply", str(ADMISSION_PATCH)], cwd=checkout, check=True)
    after = {path: (checkout / path).read_text() for path in (HEADER, DOC, URI)}
    return before, after


def patch_fragments() -> tuple[dict, dict]:
    """Only contiguous patch text; never pretend omitted source was executed."""
    before, after = {}, {}
    for section in PATCH.read_text().split("diff --git ")[1:]:
        path = section.splitlines()[0].split(" b/", 1)[1]
        assert path in (HEADER, DOC, URI)
        old, new = [], []
        left = right = 0
        for line in section.splitlines()[1:]:
            if line.startswith("@@"):
                assert left == right == 0
                match = re.fullmatch(r"@@ -\d+,(\d+) \+\d+,(\d+) @@", line)
                assert match
                left, right = map(int, match.groups())
            elif left or right:
                assert line[:1] in (" ", "+", "-")
                if line[0] != "+":
                    old.append(line[1:]); left -= 1
                if line[0] != "-":
                    new.append(line[1:]); right -= 1
                assert left >= 0 and right >= 0
        assert left == right == 0
        before[path], after[path] = "\n".join(old) + "\n", "\n".join(new) + "\n"
    assert set(after) == {HEADER, DOC, URI}
    subprocess.run(["git", "apply", "--numstat", str(PATCH)], check=True,
                   stdout=subprocess.PIPE)
    # Apply exact admission hunks to the contiguous extracted production header.
    # No invented omitted source or independent admission implementation.
    sections = ADMISSION_PATCH.read_text().split("diff --git ")[1:]
    assert len(sections) == 1 and sections[0].splitlines()[0].endswith(" b/" + HEADER)
    hunks = re.split(r"(?m)^@@ [^\n]+\n", sections[0])[1:]
    assert hunks
    for hunk in hunks:
        old, new = [], []
        for line in hunk.splitlines():
            if not line or line.startswith("\\ No newline"):
                continue
            assert line[:1] in (" ", "+", "-")
            if line[0] != "+":
                old.append(line[1:])
            if line[0] != "-":
                new.append(line[1:])
        previous, revised = "\n".join(old) + "\n", "\n".join(new) + "\n"
        assert after[HEADER].count(previous) == 1, "admission hunk must match exact extracted production bytes"
        after[HEADER] = after[HEADER].replace(previous, revised, 1)
    subprocess.run(["git", "apply", "--numstat", str(ADMISSION_PATCH)], check=True,
                   stdout=subprocess.PIPE)
    return before, after


def erase_diagnostics(text: str, path: str) -> str:
    helper = "MakOSDocLoadTrace" if path == DOC else "MakOSChannelTrace"
    block = re.compile(r"#ifdef XP_MAKOS\ntemplate <size_t Size>\nstatic void " + helper +
                       r"[\s\S]*?#endif\n\n")
    text, count = block.subn("", text)
    assert count == 1
    includes = () if path == DOC else (
        '#include "mozilla/MakOSRuntimeEvidence.h"\n',
        '#include "nsIURI.h"\n')
    for include in includes:
        # Only the diagnostic patch's added occurrence is removed.
        assert include in text
        text = text.replace(include, "", 1)
    text = re.sub(r'^ *MAKOS_(?:DOC_LOAD|CHANNEL_TRACE)\([^\n]*\);\n', "", text,
                  flags=re.MULTILINE)
    if path == DOC:
        equivalent = ("  nsresult openResult = OpenInitializedChannel(channel, uriLoader, openFlags);\n"
                      "  return openResult;")
        assert text.count(equivalent) == 1
        text = text.replace(equivalent, "  return OpenInitializedChannel(channel, uriLoader, openFlags);")
    return text


def structural(before: dict, after: dict, source: Path | None) -> None:
    for path in (DOC, URI):
        assert erase_diagnostics(after[path], path) == before[path], \
            f"non-diagnostic production change: {path}"
    emitter = item(after[HEADER], "inline bool WriteLoadDiagnostic(")
    preceding = after[HEADER].index("\n\n// Bounded load-path diagnostics")
    ending = after[HEADER].index(emitter) + len(emitter)
    assert after[HEADER][ending] == "\n"
    assert after[HEADER][:preceding] + after[HEADER][ending + 1:] == before[HEADER]
    assert emitter.count("write(") == 1
    assert 'char record[256]' in emitter and 'aStageLength > 32' in emitter
    assert "MAKOS_FIREFOX_PAGE_STOP" not in PATCH.read_text()
    assert "nsIIdentChannel.h" not in PATCH.read_text(), "nsIIdentChannel has no standalone generated header"
    assert ('#  define MAKOS_CHANNEL_TRACE(stage, request, result) \\\n'
            '    MakOSChannelTrace(stage, request, result)') in after[URI]
    assert ('#  define MAKOS_DOC_LOAD(stage, result, state, channel) \\\n'
            '    MakOSDocLoadTrace(stage, result, state, channel, \\\n'
            '                      mBrowsingContext->IsTopContent())') in after[DOC]
    # Verify actual IDL owns the added native type/header, independently of mocks.
    if source is not None:
        lock = dict(line.split("=", 1) for line in (PORT / "source.lock").read_text().splitlines())
        idl = subprocess.check_output(["git", "-C", str(source), "show",
            f"{lock['FIREFOX_COMMIT']}:netwerk/base/nsIChannel.idl"], text=True)
        assert "interface nsIIdentChannel" in idl
        assert "attribute uint64_t channelId" in idl


CPP = r'''
#include <atomic>
#include <cassert>
#include <cerrno>
#include <cstdint>
#include <cstring>
#include <mutex>
#include <string>
#include <thread>
#include <type_traits>
#include <vector>
#include <unistd.h>
template<class T> class Atomic {
  std::atomic<T> value;
 public:
  explicit Atomic(T initial):value(initial) {}
  operator T() const { return value.load(); }
  Atomic& operator=(T next) { value.store(next); return *this; }
  bool compareExchange(T old, T next) { return value.compare_exchange_strong(old,next); }
};
std::mutex transportMutex;
std::vector<std::string> records;
unsigned writes=0;
int fault=-2;
ssize_t diagnostic_write(int fd,const void* bytes,size_t size) {
  std::lock_guard<std::mutex> lock(transportMutex);
  assert(fd==STDERR_FILENO && size<256 && "bounded single write");
  ++writes; errno=EIO;
  ssize_t count=fault==-2 ? static_cast<ssize_t>(size) : fault;
  records.emplace_back(static_cast<const char*>(bytes), count>0 ? count : 0);
  return count;
}
#define write diagnostic_write
namespace mozilla::makos { EMITTER }
#undef write
using nsresult=uint32_t;
// Callback-only adapter constants are unused in the source-free subset.
[[maybe_unused]] constexpr nsresult NS_OK=0,NS_ERROR_FAILURE=0x80004005,NS_ERROR_UNEXPECTED=0x8000ffff,
 NS_ERROR_WONT_HANDLE_CONTENT=0x804b0019,NS_ERROR_NO_CONTENT=0x804b0011,
 NS_BINDING_ABORTED=0x804b0002,NS_ERROR_NET_ERROR_RESPONSE=0x804b0051,
 NS_ERROR_NET_EMPTY_RESPONSE=0x804b0052;
#define NS_FAILED(rv) (((rv)&0x80000000u)!=0)
#define NS_SUCCEEDED(rv) (!NS_FAILED(rv))
#define NS_IMETHODIMP nsresult
#define NS_ENSURE_ARG_POINTER(p) do {if(!(p)) return NS_ERROR_UNEXPECTED;} while(false)
#define NS_ENSURE_SUCCESS(rv,result) do {if(NS_FAILED(rv)) return result;} while(false)
#define LOG(...) do {} while(false)
#define LOG_ERROR(...) do {} while(false)
#define LOG_ENABLED() false
#define MOZ_ASSERT(...) do {} while(false)
#define NS_ASSERTION(...) do {} while(false)
std::vector<std::string> calls;
bool clobber=false;
void metadata() { if(clobber) errno=ENOSPC; }
struct Scenario {
 nsresult open=0,async=0,status=0,statusGetter=0,responseGetter=0,
   uriGetter=0,decodedGetter=0,dispatch=0,listenerStart=0;
 uint32_t response=200; int64_t length=1; uint64_t decoded=1;
 bool http=true,ident=true,listener=true,blank=false,received=false;
 std::string scheme="https";
} scenario;
namespace mozilla::StaticPrefs {
bool browser_http_blank_page_with_error_response_enabled() { return scenario.blank; }
}
struct nsIURI {
 bool SchemeIs(const char* scheme) { metadata(); return scenario.scheme==scheme; }
 template<class T> void GetAsciiSpec(T&) {}
} uri;
struct nsIRequest { virtual ~nsIRequest()=default;
 nsresult GetStatus(nsresult* out) { calls.push_back("status"); *out=scenario.status; return scenario.statusGetter; }
};
struct nsIIdentChannel { virtual ~nsIIdentChannel()=default;
 uint64_t ChannelId() {metadata();return 0xfedcba9876543210ULL;}
};
struct nsIStreamListener {
 nsresult OnStartRequest(nsIRequest*) {calls.push_back("listener-start");return scenario.listenerStart;}
 nsresult OnStopRequest(nsIRequest*,nsresult result) {calls.push_back("listener-stop:"+std::to_string(result));return NS_ERROR_FAILURE;}
} listener;
struct nsIChannel:nsIRequest,nsIIdentChannel {
 nsresult GetURI(nsIURI** out) {metadata();*out=NS_SUCCEEDED(scenario.uriGetter)?&uri:nullptr;return scenario.uriGetter;}
 nsresult AsyncOpen(nsIStreamListener*) {calls.push_back("async");return scenario.async;}
};
struct nsIHttpChannel:nsIChannel {
 nsresult GetResponseStatus(uint32_t* out) {*out=scenario.response;return scenario.responseGetter;}
 nsresult GetContentLength(int64_t* out) {*out=scenario.length;return NS_OK;}
 nsresult GetDecodedBodySize(uint64_t* out) {*out=scenario.decoded;return scenario.decodedGetter;}
} channel;
struct Query {
 nsIRequest* value;nsresult* result;
 template<class T> T* get() const {
   metadata(); T* ptr=dynamic_cast<T*>(value);
   if constexpr(std::is_same_v<T,nsIHttpChannel>) if(!scenario.http) ptr=nullptr;
   if constexpr(std::is_same_v<T,nsIIdentChannel>) if(!scenario.ident) ptr=nullptr;
   if(result) *result=ptr?NS_OK:NS_ERROR_FAILURE;
   return ptr;
 }
};
Query do_QueryInterface(nsIRequest* value,nsresult* result=nullptr) {return {value,result};}
template<class T> struct nsCOMPtr {
 T* value=nullptr;
 nsCOMPtr()=default;
 nsCOMPtr(T* ptr):value(ptr) {}
 nsCOMPtr(Query query):value(query.get<T>()) {}
 ~nsCOMPtr() {if(value && clobber) errno=ERANGE;}
 T* operator->() const {return value;}
 operator T*() const {return value;}
 T* get() const {return value;}
 nsCOMPtr& operator=(T* ptr) {value=ptr;return *this;}
};
template<class T> T** getter_AddRefs(nsCOMPtr<T>& ptr) {return &ptr.value;}
struct nsDocShellLoadState {
 nsIURI* URI() {metadata();return &uri;}
 uint64_t GetLoadIdentifier() {metadata();return 0x123456789abcdef0ULL;}
} load;
struct nsIInterfaceRequestor {} context;
struct nsAutoCString {const char* get() const {return "not-logged";}};
DOC_HELPER
CHANNEL_HELPER
#define MAKOS_CHANNEL_TRACE(stage,request,result) MakOSChannelTrace(stage,request,result)
struct nsURILoader {
 nsresult OpenChannel(nsIChannel*,uint32_t,nsIInterfaceRequestor*,bool,nsIStreamListener** out) {
   calls.push_back("open");*out=&listener;return scenario.open;
 }
 nsresult OpenURI(nsIChannel*,uint32_t,nsIInterfaceRequestor*);
 nsresult Original(nsIChannel*,uint32_t,nsIInterfaceRequestor*);
};
struct ContentType {void Truncate() {calls.push_back("truncate");}};
struct nsDocumentOpenInfo {
 nsCOMPtr<nsIStreamListener> m_targetStreamListener;
 ContentType mContentType;bool mUsedContentHandler=true,mReceivedData=false;
 nsDocumentOpenInfo():m_targetStreamListener(scenario.listener?&listener:nullptr),mReceivedData(scenario.received) {}
 nsresult DispatchContent(nsIRequest*) {calls.push_back("dispatch");return scenario.dispatch;}
 nsresult CheckContentLengthDiscrepancy(nsIRequest*);
 nsresult OnStartRequest(nsIRequest*);
 nsresult StartOriginal(nsIRequest*);
 nsresult OnStopRequest(nsIRequest*,nsresult);
 nsresult StopOriginal(nsIRequest*,nsresult);
};
ORIGINAL_OPEN
PATCHED_OPEN
CHECK_LENGTH
ORIGINAL_START
PATCHED_START
ORIGINAL_STOP
PATCHED_STOP
std::string record(const std::string& stage,uint32_t result,uint64_t loadId,uint64_t channelId,uint32_t flags) {
 char text[256]; int size=snprintf(text,sizeof(text),
 "MAKOS_FIREFOX_LOAD_DIAGNOSTIC stage=%s result=0x%08x load=0x%016llx channel=0x%016llx flags=0x%08x scope=diagnostic-only\n",
 stage.c_str(),result,static_cast<unsigned long long>(loadId),static_cast<unsigned long long>(channelId),flags);
 assert(size>0 && size<256);return text;
}
bool has(const std::string& stage,uint32_t result) {
 for(const auto& value:records) if(value==record(stage,result,0,0xfedcba9876543210ULL,4)) return true;
  return false;
}
void armForCallbacks() {
 assert(mozilla::makos::WriteLoadDiagnostic("arm",3,NS_OK,1,2,13));
 assert(writes==1);
 records.clear(); writes=0; // Static admission count remains one; never reset.
}
int main(int argc,char** argv) {
 assert(argc==2); std::string mode=argv[1];
 if(mode=="cap" || mode=="concurrent") {
   auto emit=[] {for(int i=0;i<150;++i) mozilla::makos::WriteLoadDiagnostic("test",4,NS_ERROR_FAILURE,1,2,15);};
   if(mode=="concurrent") {std::vector<std::thread> threads;for(int i=0;i<8;++i)threads.emplace_back(emit);for(auto& thread:threads)thread.join();}
   else emit();
   assert(writes==128 && "bounded admission");
   for(const auto& value:records)assert(value==record("test",NS_ERROR_FAILURE,1,2,15) && "exact fields");
 } else if(mode=="admission-http" || mode=="admission-https") {
   const uint32_t rejected[]={0,8,9,2,4,10,12,1};
   errno=EACCES;
   for(int repeat=0;repeat<150;++repeat)for(uint32_t flags:rejected)
     assert(!mozilla::makos::WriteLoadDiagnostic("startup",7,0,1,2,flags) && "startup unarmed rejection");
   assert(writes==0 && errno==EACCES && "startup consumes zero attempts");
   uint32_t trigger=mode=="admission-http"?11:13;
   assert(mozilla::makos::WriteLoadDiagnostic("trigger",7,NS_ERROR_FAILURE,3,4,trigger));
   assert(records.back()==record("trigger",NS_ERROR_FAILURE,3,4,trigger));
   for(int i=0;i<127;++i) {
     uint32_t flags=rejected[i%8];
     assert(mozilla::makos::WriteLoadDiagnostic("callback",8,0,0,5,flags) && "unknown callbacks retained after arm");
     assert(records.back()==record("callback",0,0,5,flags));
   }
   assert(writes==128 && "startup consumes zero attempts");
   for(int i=0;i<150;++i) {
     assert(!mozilla::makos::WriteLoadDiagnostic("trigger",7,0,3,4,trigger) && "trigger never resets budget");
     assert(!mozilla::makos::WriteLoadDiagnostic("callback",8,0,0,0,0) && "exhaustion stays closed");
   }
   assert(writes==128 && errno==EACCES);
 } else if(mode=="invalid-stage") {
   assert(!mozilla::makos::WriteLoadDiagnostic("012345678901234567890123456789012",33,0,1,2,13));
   assert(!mozilla::makos::WriteLoadDiagnostic("unknown",7,0,0,0,0) && "invalid stage must not arm");
   assert(writes==0);
   assert(mozilla::makos::WriteLoadDiagnostic("trigger",7,0,1,2,13));
   for(int i=0;i<127;++i)assert(mozilla::makos::WriteLoadDiagnostic("callback",8,0,0,0,0));
   assert(writes==128 && "invalid stage consumes zero attempts");
   assert(!mozilla::makos::WriteLoadDiagnostic("trigger",7,0,1,2,13));
 } else if(mode=="transport") {
   errno=EACCES;
   assert(!mozilla::makos::WriteLoadDiagnostic("012345678901234567890123456789012",33,0,0,0,0));
   assert(writes==0 && errno==EACCES);
   assert(mozilla::makos::WriteLoadDiagnostic("01234567890123456789012345678901",32,0xffffffff,UINT64_MAX,UINT64_MAX,UINT32_MAX));
   assert(records.back()==record("01234567890123456789012345678901",0xffffffff,UINT64_MAX,UINT64_MAX,UINT32_MAX));
   assert(errno==EACCES && "emitter errno restored");
   fault=1;assert(!mozilla::makos::WriteLoadDiagnostic("short",5,0,0,0,0));
   assert(writes==2 && records.back().size()==1 && "no retry");
   fault=-1;for(int i=0;i<200;++i)assert(!mozilla::makos::WriteLoadDiagnostic("error",5,0,0,0,0));
   assert(writes==128 && "failed attempts consumed");
   assert(errno==EACCES && "emitter errno restored");
 } else if(mode=="metadata") {
   clobber=true;errno=EBUSY;
   MakOSDocLoadTrace("doc",0x81234567,&load,&channel,true);
   assert(errno==EBUSY && "metadata destructor errno restored");
   assert(records.back()==record("doc",0x81234567,0x123456789abcdef0ULL,0xfedcba9876543210ULL,13) && "doc metadata fields");
   scenario.scheme="http";scenario.ident=false;
   MakOSDocLoadTrace("channel",NS_OK,nullptr,&channel,false);
   assert(errno==EBUSY && records.back()==record("channel",NS_OK,0,0,10));
   MakOSDocLoadTrace("none",NS_ERROR_FAILURE,nullptr,nullptr,false);
   assert(errno==EBUSY && records.back()==record("none",NS_ERROR_FAILURE,0,0,8));
   scenario.ident=true;scenario.scheme="https";
   MakOSChannelTrace("callback",&channel,0x87654321);
   assert(errno==EBUSY && "metadata destructor errno restored");
   assert(records.back()==record("callback",0x87654321,0,0xfedcba9876543210ULL,4) && "unknown top flag");
   scenario.uriGetter=NS_ERROR_FAILURE;
   MakOSChannelTrace("no-uri",&channel,NS_OK);
   assert(errno==EBUSY && records.back()==record("no-uri",0,0,0xfedcba9876543210ULL,0));
 } else if(mode.rfind("open",0)==0) {
   armForCallbacks();
   int n=std::stoi(mode.substr(4));
   if(n==1)scenario.open=NS_ERROR_WONT_HANDLE_CONTENT;
   if(n==2)scenario.open=NS_ERROR_FAILURE;
   if(n==3)scenario.async=NS_ERROR_FAILURE;
   if(n==4)scenario.async=NS_ERROR_NO_CONTENT;
   nsURILoader loader;
   calls.clear();auto expected=loader.Original(&channel,3,&context);auto originalCalls=calls;
   calls.clear();auto actual=loader.OpenURI(&channel,3,&context);
   assert(actual==expected && calls==originalCalls && "OpenURI behavior unchanged");
   assert(has("loader-content-return",scenario.open) && "raw content result");
   if(n==1)assert(calls==std::vector<std::string>{"open"} && has("loader-content-declined",scenario.open));
   else assert(has("loader-async-return",scenario.async) && "raw async result");
   if(n==4)assert(actual==NS_OK && has("loader-no-content",NS_ERROR_NO_CONTENT));
 } else if(mode.rfind("start",0)==0) {
   armForCallbacks();
   int n=std::stoi(mode.substr(5));
   if(n==1)scenario.response=204;
   if(n==2)scenario.status=NS_ERROR_FAILURE;
   if(n==3)scenario.listenerStart=NS_ERROR_FAILURE;
   if(n==4){scenario.response=500;scenario.length=0;}
   if(n==5)scenario.http=false;
   nsDocumentOpenInfo original,patched;
   calls.clear();auto expected=original.StartOriginal(&channel);auto originalCalls=calls;
   calls.clear();auto actual=patched.OnStartRequest(&channel);
   assert(actual==expected && calls==originalCalls && "start behavior unchanged");
   assert(has("request-start-enter",NS_OK) && "start entry diagnostic");
 } else if(mode.rfind("stop",0)==0) {
   armForCallbacks();
   int n=std::stoi(mode.substr(4));nsresult status=0x81234567;
   if(n==1){scenario.response=500;scenario.decoded=0;}
   if(n==2){scenario.response=404;scenario.decoded=0;}
   if(n==3)scenario.listener=false;
   if(n==4){scenario.response=500;scenario.decoded=0;scenario.received=true;}
   nsDocumentOpenInfo original,patched;
   calls.clear();auto expected=original.StopOriginal(&channel,status);auto originalCalls=calls;
   calls.clear();auto actual=patched.OnStopRequest(&channel,status);
   assert(actual==expected && calls==originalCalls && "stop forwarding unchanged");
   assert(!patched.mUsedContentHandler && !patched.m_targetStreamListener);
   assert(has("request-stop-enter",status) && "stop entry raw status");
   if(scenario.listener)assert(has("request-stop-forward",n==1?NS_ERROR_NET_ERROR_RESPONSE:n==2?NS_ERROR_NET_EMPTY_RESPONSE:status) && "stop forward adjusted status");
   else assert(records.size()==1);
 } else assert(false);
}
'''


def main() -> None:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--source-dir", type=Path)
    group.add_argument("--patch-only", action="store_true",
                       help="exercise source-free emitter/helper subset, not callback bodies")
    args = parser.parse_args()
    source = args.source_dir or ROOT / "build/ports/firefox/source"
    full = not args.patch_only and (args.source_dir is not None or (source / ".git").exists())
    with tempfile.TemporaryDirectory(prefix="makos-channel-diagnostics-") as temporary:
        directory = Path(temporary)
        before, after = private_apply(source, directory) if full else patch_fragments()
        structural(before, after, source if full else None)
        emitter = item(after[HEADER], "inline bool WriteLoadDiagnostic(")
        doc_helper = "template <size_t Size>\n" + item(after[DOC], "static void MakOSDocLoadTrace(")
        channel_helper = "template <size_t Size>\n" + item(after[URI], "static void MakOSChannelTrace(")
        replacements = {"EMITTER": emitter, "DOC_HELPER": doc_helper,
                        "CHANNEL_HELPER": channel_helper}
        for name, signature, replacement in (
            ("OPEN", "NS_IMETHODIMP nsURILoader::OpenURI(", "::Original("),
            ("START", "NS_IMETHODIMP nsDocumentOpenInfo::OnStartRequest(", "::StartOriginal("),
            ("STOP", "NS_IMETHODIMP nsDocumentOpenInfo::OnStopRequest(", "::StopOriginal(")):
            replacements["PATCHED_" + name] = item(after[URI], signature) if full else ""
            original = item(before[URI], signature) if full else ""
            replacements["ORIGINAL_" + name] = re.sub(r"::\w+\(", replacement, original, count=1)
        replacements["CHECK_LENGTH"] = item(before[URI], "nsresult nsDocumentOpenInfo::CheckContentLengthDiscrepancy(") if full else ""
        code = CPP
        if not full:
            code = code[:code.index(' } else if(mode.rfind("open",0)==0)')] + ' } else assert(false);\n}\n'
        for key, value in replacements.items():
            code = code.replace(key, value)

        def native(name: str, text: str) -> Path:
            source = directory / (name + ".cpp")
            source.write_text(text)
            binary = directory / name
            compiler = "/usr/bin/c++" if Path("/usr/bin/c++").exists() else shutil.which("c++")
            assert compiler, "host C++ compiler required"
            subprocess.run([compiler, "-std=c++17",
                "-Wall", "-Wextra", "-Werror", "-O1", "-D_FORTIFY_SOURCE=2",
                "-pthread", str(source), "-o", str(binary)], check=True)
            return binary

        binary = native("channel", code)
        modes = ["cap", "concurrent", "transport", "metadata",
                 "admission-http", "admission-https", "invalid-stage"]
        if full:
            modes += [f"open{i}" for i in range(5)] + [f"start{i}" for i in range(6)] + [f"stop{i}" for i in range(5)]
        for mode in modes:
            subprocess.run([str(binary), mode], check=True, timeout=15)
        controls = [
            ("uncapped", code.replace("prior >= 128", "prior >= 1280"), "cap", "bounded admission"),
            ("wrong-result", code.replace("hex(aResult, 28);", "hex(aResult & 0u, 28);"), "cap", "exact fields"),
            ("missing-restore", code.replace("~RestoreErrno() { errno = saved; }", "~RestoreErrno() { (void)saved; }"), "metadata", "metadata destructor errno restored"),
            ("repeat-write", code.replace("const ssize_t written = write(STDERR_FILENO, record, length);", "(void)write(STDERR_FILENO, record, length);\n  const ssize_t written = write(STDERR_FILENO, record, length);"), "cap", "bounded admission"),
            ("startup-unfiltered", code.replace("if (!armed)", "if (false && !armed)"), "admission-http", "startup unarmed rejection"),
            ("scheme-only", code.replace("(aFlags & 9u) == 9u && (aFlags & 6u) != 0", "(aFlags & 6u) != 0"), "admission-https", "startup unarmed rejection"),
            ("unknown-callback-filtered", code.replace("static Atomic<uint32_t> attempts(0);", "if ((aFlags & 9u) != 9u) return false;\n  static Atomic<uint32_t> attempts(0);"), "admission-http", "unknown callbacks retained after arm"),
            ("rearm-budget-reset", code.replace("uint32_t prior = attempts;", "if ((aFlags & 9u) == 9u && (aFlags & 6u) != 0) attempts = 0;\n  uint32_t prior = attempts;"), "admission-https", "trigger never resets budget"),
            ("invalid-stage-arms", code.replace("  if (aStageLength > 32) {\n    return false;\n  }\n", "", 1), "invalid-stage", "invalid stage must not arm"),
        ]
        if full:
            controls += [
                ("forward-change", code.replace(replacements["PATCHED_STOP"], replacements["PATCHED_STOP"].replace("listener->OnStopRequest(request, aStatus);", "listener->OnStopRequest(request, NS_OK);")), "stop0", "stop forwarding unchanged"),
                ("normalized-result", code.replace('MAKOS_CHANNEL_TRACE("loader-async-return", channel, rv);', 'MAKOS_CHANNEL_TRACE("loader-async-return", channel, NS_OK);'), "open4", "raw async result"),
                ("missing-result", code.replace('MAKOS_CHANNEL_TRACE("loader-async-return", channel, rv);', '((void)rv);'), "open4", "raw async result"),
            ]
        for name, mutant, mode, assertion in controls:
            assert mutant != code
            result = subprocess.run([str(native(name, mutant)), mode], capture_output=True,
                                    text=True, timeout=15)
            assert result.returncode != 0 and assertion in result.stderr, (name, result.stdout, result.stderr)
        mutated = dict(after)
        mutated[URI] = mutated[URI].replace("return NS_OK;", "return NS_ERROR_FAILURE;", 1)
        assert erase_diagnostics(mutated[URI], URI) != before[URI], "policy mutation escaped structural guard"
    print(f"MAKOS_FIREFOX_CHANNEL_DIAGNOSTICS_HOST_OK native_cases={len(modes)} negative_controls={len(controls)+1} "
          + ("source=pinned-private-apply structural=exact-policy-restoration " if full else
             "source=patch-extracted-only structural=patch-fragment-policy-restoration ")
          + "transport=128-single-write errno=getter,destructor-preserved "
          + "admission=known-top-http-https-once startup=unspent callbacks-after-arm=unfiltered "
          + ("callbacks=production-start,stop,content-length-override " if full else "callbacks=not-executed ")
          +
          "guest_execution=not-run browser_qualification=not-run")


if __name__ == "__main__":
    main()
