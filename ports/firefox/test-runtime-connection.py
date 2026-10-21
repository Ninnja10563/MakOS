#!/usr/bin/env python3
"""Exercise claimed-connection diagnostics; no guest/browser qualification.

Pinned-source mode restores complete preceding production files after diagnostic
erasure and executes the three complete PendingTransactionInfo claim methods with
explicit host adapters. Other integration points get source checks only. Both
modes execute actual helpers/macros, including transport failures, stale snapshots,
shared budget, identity exhaustion and non-MakOS argument erasure.
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
PATCH = PORT / "patches/0068-makos-connection-claim.patch"
HEADER = "mfbt/MakOSRuntimeEvidence.h"
HTTP = "netwerk/protocol/http/"
PATHS = (HEADER,) + tuple(HTTP + name for name in (
    "DnsAndConnectSocket.cpp", "DnsAndConnectSocket.h", "HttpConnectionBase.h",
    "PendingTransactionInfo.cpp", "nsHttpConnection.cpp", "nsHttpConnectionMgr.cpp"))
spec = importlib.util.spec_from_file_location("claim_transaction", PORT / "test-runtime-transaction.py")
transaction = importlib.util.module_from_spec(spec)
assert spec.loader
spec.loader.exec_module(transaction)
item = transaction.item


def hunks():
    # Reuse the strict unified-diff parser with this patch's exact path contract.
    previous = transaction.PATCH, transaction.PATHS
    try:
        transaction.PATCH, transaction.PATHS = PATCH, PATHS
        return transaction.patch_hunks()
    finally:
        transaction.PATCH, transaction.PATHS = previous


def fixtures(source, directory):
    chunks = hunks()
    _, preceding = transaction.fixtures(source, directory)
    if source is None:
        before, after = {HEADER: preceding[HEADER]}, {HEADER: preceding[HEADER]}
        for old, new in chunks[HEADER]:
            assert after[HEADER].count(old) == 1
            after[HEADER] = after[HEADER].replace(old, new, 1)
        for path in PATHS[1:]:
            before[path] = "".join(old for old, _ in chunks[path])
            after[path] = "".join(new for _, new in chunks[path])
        return before, after
    checkout = directory / "source"
    lock = dict(line.split("=", 1) for line in (PORT / "source.lock").read_text().splitlines())
    for path in PATHS[1:]:
        target = checkout / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(subprocess.check_output([
            "git", "-C", str(source), "show", f"{lock['FIREFOX_COMMIT']}:{path}"]))
        for patch in sorted((PORT / "patches").glob("*.patch")):
            if patch.name < PATCH.name and f"diff --git a/{path} b/{path}\n" in patch.read_text():
                subprocess.run(["git", "apply", f"--include={path}", str(patch)], cwd=checkout, check=True)
    before = {path: (checkout / path).read_text() for path in PATHS}
    subprocess.run(["git", "apply", "--check", str(PATCH)], cwd=checkout, check=True)
    subprocess.run(["git", "apply", str(PATCH)], cwd=checkout, check=True)
    return before, {path: (checkout / path).read_text() for path in PATHS}


def erase(text, path):
    if path == HEADER:
        for name in ("diagnostics", "macros"):
            text, count = re.subn(r"\n// BEGIN MakOS connection claim " + name +
                r"\n[\s\S]*?// END MakOS connection claim " + name + r"\n", "", text)
            assert count == 1
        return text
    if path.endswith(".h"):
        text = text.replace('#include "mozilla/MakOSRuntimeEvidence.h"\n', "", 1)
        text, count = re.subn(r"\n  // BEGIN MakOS connection claim fields\n[\s\S]*?"
                             r"  // END MakOS connection claim fields\n", "", text)
        assert count == 1
        return text
    text = re.sub(r"\n  // BEGIN MakOS connection claim transfer\n[\s\S]*?"
                  r"  // END MakOS connection claim transfer\n", "", text)
    text = re.sub(r"^ *MAKOS_CONN_\w+\([\s\S]*?\);\n", "", text, flags=re.M)
    if path.endswith("PendingTransactionInfo.cpp"):
        text = re.sub(r"^ *MAKOS_TX_(?:CAPTURE|TRACE)\([\s\S]*?\);\n", "", text, flags=re.M)
    if path.endswith("nsHttpConnectionMgr.cpp"):
        text = re.sub(r"^ *MAKOS_TX_(?:CAPTURE|TRACE)\(makos(?:Pending|Dispatch)Channel,[\s\S]*?\);\n", "", text, flags=re.M)
    # The one blank line groups the added setup-return/transfer observation.
    if path.endswith("DnsAndConnectSocket.cpp"):
        text = text.replace("  }\n\n\n  nsCOMPtr<nsIInterfaceRequestor> callbacks;",
                            "  }\n\n  nsCOMPtr<nsIInterfaceRequestor> callbacks;", 1)
    return text


def structural(before, after):
    for path in PATHS:
        assert erase(after[path], path) == before[path], "policy erasure: " + path
    helper = item(after[HEADER], "inline ConnectionClaim NewConnectionClaim(")
    assert "prior >= 32" in helper and "aChannel != HttpStateChannel()" in helper
    assert item(after[HEADER], "inline bool WriteTransactionDiagnostic(") == item(before[HEADER], "inline bool WriteTransactionDiagnostic("), "shared budget unchanged"
    macros = re.search(r"// BEGIN MakOS connection claim macros\n[\s\S]*?"
                       r"// END MakOS connection claim macros", after[HEADER])[0]
    added = "\n".join(line[1:] for line in PATCH.read_text().splitlines()
                      if line.startswith("+") and not line.startswith("+++"))
    assert not any(token in added for token in ("GetSpec(", "GetAsciiSpec(", "RefPtr<", "NS_ADDREF", "TLS_OK", "PAGE_STOP"))
    assert all(len(stage) <= 32 for stage in re.findall(r'"((?:claim|pending|dispatch)-[a-z-]+)"', added))
    manager = after[HTTP + "nsHttpConnectionMgr.cpp"]
    assert re.search(r"rv = TryDispatchTransaction\([^;]+;\n\s*MAKOS_TX_TRACE\(makosPendingChannel, \"pending-dispatch-return\", rv, true,", manager), "actual pending dispatch result"
    dns = after[HTTP + "DnsAndConnectSocket.cpp"]
    assert '"claim-setup-return", rv, true' in dns and 'MAKOS_CONN_TRANSFER(conn.get(), makosClaim)' in dns
    close = item(after[HTTP + "nsHttpConnection.cpp"], "void nsHttpConnection::CloseTransaction(") if "mTransaction->Close(reason);" in after[HTTP + "nsHttpConnection.cpp"] else after[HTTP + "nsHttpConnection.cpp"]
    assert '"claim-close-transaction", reason, false' in close, "supplied close reason is not a call result"
    return macros


MAIN = r'''
struct Socket {
 mozilla::makos::ConnectionClaim mMakOSClaim;
 uint32_t state=0x1234;
 uint32_t MakOSClaimState()const{return state;}
};
int main(int argc,char** argv){
 assert(argc==2);std::string mode=argv[1];errno=EBUSY;
 if(mode=="admission"){
  auto empty=mozilla::makos::NewConnectionClaim(28);
  assert(!empty.channel && !empty.token && "unselected claim empty");
  select();
  assert(!mozilla::makos::NewConnectionClaim(0).channel && !mozilla::makos::NewConnectionClaim(29).channel && "foreign claim rejected");
  for(unsigned i=1;i<=32;++i){auto c=mozilla::makos::NewConnectionClaim(28);assert(c.channel==28 && c.token==i && c.observed==0 && "monotonic bounded token");}
  assert(!mozilla::makos::NewConnectionClaim(28).channel && "claim exhaustion empty");
  assert(writes==0 && "claim allocation does not emit");
 }else if(mode=="transfer"){
  select();Socket source,destination;unsigned effects=0;
  MAKOS_CONN_CLAIM((++effects,&source),28,"claim-dns");
  assert(effects==1 && source.mMakOSClaim.token==1 && "claim object evaluated once");
  assert(has("claim-dns",0,false,28,1,0x1234) && "claim exact identity and state");
  MAKOS_CONN_CAPTURE(saved,&source);
  source.mMakOSClaim={};source.state=99;
  MAKOS_CONN_TRANSFER(&destination,saved);
  assert(destination.mMakOSClaim.channel==28 && destination.mMakOSClaim.token==1 && "claim transferred from captured scalars");
  mozilla::makos::ConnectionClaim absent;
  MAKOS_CONN_TRANSFER(&destination,absent);
  assert(destination.mMakOSClaim.token==1 && "unselected transfer preserves history");
  auto* transient=new Socket;transient->mMakOSClaim=saved;
  MAKOS_CONN_CAPTURE(detached,transient);delete transient;
  MAKOS_CONN_TRACE(detached,"claim-after-callback",0x80004005,true,7);
  assert(has("claim-after-callback",0x80004005,true,28,1,7) && "no callback lifetime dependency");
 }else if(mode=="concurrent"){
  select();std::mutex lock;std::vector<uint32_t> tokens;std::vector<std::thread> threads;
  for(unsigned i=0;i<8;++i)threads.emplace_back([&]{
   for(unsigned j=0;j<16;++j){auto c=mozilla::makos::NewConnectionClaim(28);if(c.channel){std::lock_guard<std::mutex> guard(lock);tokens.push_back(c.token);}}
  });
  for(auto& thread:threads)thread.join();std::sort(tokens.begin(),tokens.end());
  assert(tokens.size()==32 && "concurrent claim cap");
  for(unsigned i=0;i<32;++i)assert(tokens[i]==i+1 && "concurrent unique tokens");
 }else if(mode=="once" || mode=="once-error" || mode=="once-short"){
  select();Socket socket;
  socket.mMakOSClaim=mozilla::makos::NewConnectionClaim(28);
  if(mode=="once-error")fault=-1;
  if(mode=="once-short")fault=1;
  for(unsigned i=0;i<100;++i)MAKOS_CONN_ONCE(&socket,1,"claim-writable",0,false);
  assert(writes==1 && "once consumes failed attempt");fault=-2;
  MAKOS_CONN_ONCE(&socket,2,"claim-readable",0,false);
  assert(writes==2 && has("claim-readable",0,false,28,1,0x1234) && "distinct once stages");
  socket.mMakOSClaim=mozilla::makos::NewConnectionClaim(28);
  MAKOS_CONN_ONCE(&socket,1,"claim-writable",0,false);
  assert(writes==3 && has("claim-writable",0,false,28,2,0x1234) && "new claim resets once history");
 }else if(mode=="budget"){
  select();Socket socket;socket.mMakOSClaim=mozilla::makos::NewConnectionClaim(28);
  for(unsigned i=0;i<31;++i)assert(tx());
  MAKOS_CONN_ONCE(&socket,1,"claim-writable",0,false);
  MAKOS_CONN_ONCE(&socket,2,"claim-readable",0,false);
  for(unsigned i=0;i<100;++i)MAKOS_CONN_TRACE(socket.mMakOSClaim,"claim-repeat",0,false,0);
  assert(writes==33 && countPrefix("MAKOS_FIREFOX_TRANSACTION ")==32 && countPrefix("MAKOS_FIREFOX_TRANSACTION_SATURATED ")==1 && "shared 32 plus one cap");
 }else assert(false);
 assert(errno==EBUSY && "claim diagnostics preserve errno");
}
'''

NON_XP = r'''
#include <cassert>
MACROS
unsigned effects=0;
unsigned effect(){return ++effects;}
int main(int argc,char**){
 assert(argc==2);
 MAKOS_CONN_CAPTURE(unavailable,nonexistent());
 MAKOS_CONN_CLAIM(nonexistent(),effect(),"ignored");
 MAKOS_CONN_TRANSFER(nonexistent(),unavailable);
 MAKOS_CONN_TRACE(unavailable,"ignored",effect(),effect(),effect());
 MAKOS_CONN_ONCE(nonexistent(),effect(),"ignored",effect(),effect());
 assert(effects==0 && "non-MakOS arguments erased");
}
'''

HOST = r'''
using nsresult=uint32_t;
struct nsISupportsWeakReference{};
using nsWeakPtr=nsISupportsWeakReference*;
std::vector<std::string> calls;
bool claimOK=true,hasTLS=true,nullOwner=true;
nsresult tlsResult=0;
std::function<void()> callback;
template<class T>struct Ptr{
 T* pointer=nullptr;Ptr()=default;Ptr(T* p):pointer(p){}
 T* get()const{return pointer;}T* operator->()const{return pointer;}
 operator T*()const{return pointer;}
};
template<class T>using nsCOMPtr=Ptr<T>;
template<class T>T** getter_AddRefs(Ptr<T>& p){return &p.pointer;}
struct Ignore{template<class T>void operator<<(T){}} Unused;
nsWeakPtr do_GetWeakReference(nsISupportsWeakReference* p){calls.push_back("weak-reference");return p;}
struct nsITLSSocketControl{
 nsresult Claim(){calls.push_back("tls-claim");if(callback)callback();return tlsResult;}
} tls;
struct NullHttpTransaction{
 bool Claim(){calls.push_back("null-claim");return claimOK;}
} nullTrans;
struct nsAHttpTransaction{
 NullHttpTransaction* QueryNullTransaction(){calls.push_back("query-null");return nullOwner?&nullTrans:nullptr;}
} owner;
struct nsHttpTransaction{
 uint64_t id=28;
 uint64_t ChannelId(){errno=ERANGE;return id;}
};
struct HttpConnectionBase:nsISupportsWeakReference{
 mozilla::makos::ConnectionClaim mMakOSClaim;
 uint32_t MakOSClaimState()const{return 3;}
 nsAHttpTransaction* Transaction(){calls.push_back("owner");return &owner;}
 void GetTLSSocketControl(nsITLSSocketControl** p){calls.push_back("tls-control");*p=hasTLS?&tls:nullptr;}
};
struct DnsAndConnectSocket:nsISupportsWeakReference{
 mozilla::makos::ConnectionClaim mMakOSClaim;
 uint32_t MakOSClaimState()const{return 0x252;}
};
'''

HOST_CLASS = r'''
namespace NAMESPACE {
struct PendingTransactionInfo{
 Ptr<nsHttpTransaction> mTransaction;
 nsWeakPtr mDnsAndSock=nullptr,mActiveConn=nullptr;
 void RememberDnsAndConnectSocket(DnsAndConnectSocket*);
 void AddDnsAndConnectSocket(DnsAndConnectSocket*);
 bool TryClaimingActiveConn(HttpConnectionBase*);
};
FUNCTIONS
}
'''

HOST_MAIN = r'''
struct Outcome{
 bool result,dns,active;uint64_t transactionId;std::vector<std::string> calls;
 bool operator==(const Outcome& b)const{return result==b.result && dns==b.dns && active==b.active && transactionId==b.transactionId && calls==b.calls;}
};
template<class Pending>Outcome exercise(const std::string& mode){
 calls.clear();callback={};nsHttpTransaction tx;Pending p;p.mTransaction.pointer=&tx;
 DnsAndConnectSocket dns;HttpConnectionBase conn;bool result=false;
 if(mode=="remember")p.RememberDnsAndConnectSocket(&dns);
 else if(mode=="add")p.AddDnsAndConnectSocket(&dns);
 else{
  if(mode=="active-reentrant")callback=[&tx]{tx.id=99;};
  result=p.TryClaimingActiveConn(&conn);callback={};
 }
 return {result,p.mDnsAndSock!=nullptr,p.mActiveConn!=nullptr,tx.id,calls};
}
int main(int argc,char** argv){
 assert(argc==2);std::string mode=argv[1];select();errno=EBUSY;
 if(mode=="active-rejected")claimOK=false;
 if(mode=="active-no-null")nullOwner=false;
 if(mode=="active-no-tls")hasTLS=false;
 if(mode=="active-tls-error")tlsResult=0x80004005;
 auto baseline=exercise<original::PendingTransactionInfo>(mode);
 auto actual=exercise<changed::PendingTransactionInfo>(mode);
 assert(actual==baseline && "claim production policy unchanged");
 assert(errno==EBUSY && "claim getter errno preserved");
 bool rejected=mode=="active-rejected" || mode=="active-no-null";
 if(rejected)assert(writes==0 && !actual.result && "failed claims not recorded");
 else{
  const char* stage=mode=="remember"?"claim-dns":mode=="add"?"claim-new-dns":"claim-tls";
  assert(writes==1 && has(stage,0,false,28,1,mode=="remember" || mode=="add"?0x252:3) && "production claim precedes callbacks with original identity");
 }
}
'''

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--source-dir", type=Path)
    group.add_argument("--patch-only", action="store_true")
    args = parser.parse_args()
    source = args.source_dir or ROOT / "build/ports/firefox/source"
    full = not args.patch_only and (args.source_dir is not None or (source / ".git").exists())
    with tempfile.TemporaryDirectory(prefix="makos-connection-") as temporary:
        directory = Path(temporary)
        before, after = fixtures(source if full else None, directory)
        macros = structural(before, after)
        declarations = (
            "inline Atomic<uint64_t>& HttpStateChannel(", "inline void ObserveHttpStateChannel(",
            "template <class Getter>\ninline uint64_t CaptureTransactionChannel(",
            "inline bool WriteTransactionRecord(", "inline bool WriteTransactionDiagnostic(",
            "struct ConnectionClaim", "inline ConnectionClaim NewConnectionClaim(")
        helpers = "\n".join(item(after[HEADER], name) + (";" if name.startswith("struct ") else "") for name in declarations)
        tx_macros = re.search(r"// BEGIN MakOS transaction macros\n[\s\S]*?// END MakOS transaction macros", after[HEADER])[0]
        common = transaction.TRANSPORT.replace("HELPERS", helpers).replace("MACROS", tx_macros + "\n" + macros)
        code = common + MAIN
        def native(name, text):
            path = directory / (name + ".cpp"); path.write_text(text)
            binary = directory / name
            subprocess.run(["/usr/bin/c++", "-std=c++17", "-Wall", "-Wextra", "-Werror", "-O1",
                            "-D_FORTIFY_SOURCE=2", "-pthread", str(path), "-o", str(binary)], check=True)
            return binary
        def execute(binary, modes):
            for mode in modes:
                subprocess.run([str(binary), mode], check=True, timeout=15)
                print("CONNECTION_CASE_OK", binary.name, mode, flush=True)
        modes = ["admission", "transfer", "concurrent", "once", "once-error", "once-short", "budget"]
        execute(native("claims", code), modes)
        execute(native("non-makos", NON_XP.replace("MACROS", macros)), ["erasure"])
        controls = [
            ("wrong-channel", transaction.mutate_function(code, "inline ConnectionClaim NewConnectionClaim(", "aChannel != HttpStateChannel()", "(aChannel != HttpStateChannel() && false)"), "admission", "unselected claim empty"),
            ("token-cap", code.replace("prior >= 32", "prior >= 33", 1), "admission", "claim exhaustion empty"),
            ("lost-transfer", code.replace("(object)->mMakOSClaim = (claim);", "(object)->mMakOSClaim = {};", 1), "transfer", "claim transferred from captured scalars"),
            ("once-repeat", code.replace("(observed & aBit)", "(observed & aBit & 0u)", 1), "once-error", "once consumes failed attempt"),
            ("budget-raised", code.replace("prior >= 33", "prior >= 34", 1), "budget", "shared 32 plus one cap"),
            ("errno-leak", code.replace("errno = savedErrno;", "(void)savedErrno;"), "transfer", "claim diagnostics preserve errno"),
        ]
        for name, mutant, mode, assertion in controls:
            assert mutant != code
            result = subprocess.run([str(native(name, mutant)), mode], capture_output=True, text=True, timeout=15)
            assert result.returncode != 0 and assertion in result.stderr, (name, result.stdout, result.stderr)
            print("CONNECTION_CONTROL_OK", name, flush=True)
        host_modes = []
        if full:
            path = HTTP + "PendingTransactionInfo.cpp"
            names = ("void PendingTransactionInfo::RememberDnsAndConnectSocket(",
                     "void PendingTransactionInfo::AddDnsAndConnectSocket(",
                     "bool PendingTransactionInfo::TryClaimingActiveConn(")
            classes = "\n".join(HOST_CLASS.replace("NAMESPACE", namespace).replace("FUNCTIONS",
                "\n".join(item(version[path], name) for name in names))
                for namespace, version in (("original", before), ("changed", after)))
            host_code = common + HOST + classes + HOST_MAIN
            host_modes = ["remember", "add", "active-success", "active-rejected", "active-no-null",
                          "active-no-tls", "active-tls-error", "active-reentrant"]
            execute(native("production-claims", host_code), host_modes)
            # Reject semantic drift that would still emit convincing claim records.
            prefix, changed = host_code.split("namespace changed {", 1)
            mutant = prefix + "namespace changed {" + transaction.mutate_function(
                changed, "bool PendingTransactionInfo::TryClaimingActiveConn(",
                "return true;", "return false;")
            assert mutant != host_code
            result = subprocess.run([str(native("claim-result-drift", mutant)), "active-success"],
                                    capture_output=True, text=True, timeout=15)
            assert result.returncode != 0 and "claim production policy unchanged" in result.stderr
            print("CONNECTION_CONTROL_OK claim-result-drift", flush=True)
        print(f"MAKOS_FIREFOX_CONNECTION_HOST_OK cases={len(modes)+1+len(host_modes)} controls={len(controls)+bool(full)} "
              f"policy={'complete-pinned-files' if full else 'patch-fragments'} guest_qualification=0", flush=True)


if __name__ == "__main__":
    main()
