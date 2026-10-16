#!/usr/bin/env python3
"""Exercise patched navigation boundaries, not browser/guest qualification.

Private pinned-source application; real C++ Fixup body/native emitter and JS
open/catch boundary. Gecko services, atomics and transports are host adapters.
No user checkout edits, preferences, network, browser build or guest execution.
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
PATCH = PORT / "patches/0062-makos-navigation-diagnostics.patch"
HEADER = "mfbt/MakOSRuntimeEvidence.h"
DOC = "docshell/base/nsDocShell.cpp"
JS = "browser/components/urlbar/UrlbarInput.sys.mjs"


def item(source: str, declaration: str) -> str:
    start = source.index(declaration)
    end = source.index("{", start) + 1
    depth = 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


def fragments() -> tuple[dict[str, str], dict[str, str]]:
    old, new = {}, {}
    for section in PATCH.read_text().split("diff --git ")[1:]:
        path = section.splitlines()[0].split(" b/", 1)[1]
        assert path in (HEADER, DOC, JS)
        before, after = [], []
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
                    before.append(line[1:])
                    left -= 1
                if line[0] != "-":
                    after.append(line[1:])
                    right -= 1
                assert left >= 0 and right >= 0
        assert left == right == 0
        old[path], new[path] = "\n".join(before), "\n".join(after)
    assert set(new) == {HEADER, DOC, JS}
    subprocess.run(["git", "apply", "--numstat", str(PATCH)], check=True,
                   stdout=subprocess.PIPE)
    return old, new


def private_apply(source: Path, directory: Path) -> dict[str, str]:
    lock = dict(line.split("=", 1) for line in (PORT / "source.lock").read_text().splitlines())
    checkout = directory / "private-source"
    checkout.mkdir()
    subprocess.run(["git", "init", "-q", str(checkout)], check=True)
    for path in (DOC, JS):
        content = subprocess.check_output([
            "git", "-C", str(source), "show", f"{lock['FIREFOX_COMMIT']}:{path}"])
        destination = checkout / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(content)
    for patch, include in (("0044-makos-navigation-status-marker.patch", DOC),
                           ("0061-makos-runtime-evidence.patch", HEADER)):
        subprocess.run(["git", "apply", f"--include={include}",
                        str(PORT / "patches" / patch)], cwd=checkout, check=True)
    subprocess.run(["git", "apply", "--check", str(PATCH)], cwd=checkout, check=True)
    subprocess.run(["git", "apply", str(PATCH)], cwd=checkout, check=True)
    return {path: (checkout / path).read_text() for path in (HEADER, DOC, JS)}


def js_boundary(source: str) -> str:
    start = source.index("    // Notify about the start of navigation.")
    end = source.index("    this.view.close({ showFocusBorder: false });", start)
    return source[start:end] + "    this.view.close({ showFocusBorder: false });\n"


CPP = r'''
#include <atomic>
#include <cassert>
#include <cerrno>
#include <cstdint>
#include <cstring>
#include <memory>
#include <mutex>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>
#include <unistd.h>
template<class T> class Atomic {
  std::atomic<T> value;
 public:
  explicit Atomic(T initial): value(initial) {}
  operator T() const { return value.load(); }
  bool compareExchange(T old, T next) { return value.compare_exchange_strong(old, next); }
};
std::mutex transport_lock;
std::vector<std::string> records;
int fault = -2;
unsigned writes = 0;
ssize_t diagnostic_write(int fd, const void* data, size_t size) {
  std::lock_guard<std::mutex> lock(transport_lock);
  assert(fd == STDERR_FILENO && size < 192);
  ++writes;
  errno=EIO;
  ssize_t count = fault == -2 ? static_cast<ssize_t>(size) : fault;
  records.emplace_back(static_cast<const char*>(data), count > 0 ? count : 0);
  return count;
}
#define write diagnostic_write
namespace mozilla::makos {
EMITTER
}
#undef write
using nsresult = uint32_t;
using nsAString = std::string;
constexpr nsresult NS_OK=0, NS_ERROR_FAILURE=0x80004005,
  NS_ERROR_MALFORMED_URI=0x804b000a, NS_ERROR_LOAD_SHOWED_ERRORPAGE=0x804b001e;
constexpr uint32_t LOAD_FLAGS_ERROR_LOAD_CHANGES_RV=1, LOAD_NORMAL=0;
#define NS_FAILED(rv) (((rv) & 0x80000000u) != 0)
#define MOZ_LOG(...) do {} while (false)
#define MOZ_ALWAYS_SUCCEEDS(expr) assert((expr) == NS_OK)
#define MAKE_LOAD_TYPE(mode, flags) ((mode) | (flags))
std::vector<std::string> calls;
struct Scenario { bool allowed=true, state=true, display=false, history=false,
  throwing=false, principal=false; nsresult create=NS_OK, load=NS_OK; } scenario;
struct nsIURI {} uriObject;
struct nsIPrincipal {} principalObject;
template<class T> struct Ptr {
  T* value=nullptr;
  explicit operator bool() const { return value != nullptr; }
  T* get() const { return value; }
  Ptr& operator=(T* ptr) { value=ptr; return *this; }
};
template<class T> using RefPtr=Ptr<T>;
template<class T> using nsCOMPtr=Ptr<T>;
template<class T> T** getter_AddRefs(Ptr<T>& ptr) { return &ptr.value; }
const char* operator""_ns(const char* text, size_t) { return text; }
struct LoadURIOptions { uint32_t mLoadFlags=0; Ptr<nsIPrincipal> mTriggeringPrincipal; };
struct Context {
  template<class... Args> void SetActiveSessionHistoryEntry(Args...) {
    calls.push_back("set-history");
  }
};
struct nsDocShellLoadState {
  static nsresult CreateFromLoadURIOptions(Context*, const nsAString&,
      const LoadURIOptions&, nsDocShellLoadState**);
} stateObject;
nsresult nsDocShellLoadState::CreateFromLoadURIOptions(Context*, const nsAString&,
    const LoadURIOptions&, nsDocShellLoadState** out) {
  calls.push_back("create"); *out=scenario.state ? &stateObject : nullptr;
  return scenario.create;
}
nsresult NS_NewURI(nsIURI** out, const char*) {
  calls.push_back("new-about-blank"); *out=&uriObject; return NS_OK;
}
namespace nsContentUtils {
nsIPrincipal* GetSystemPrincipal() { calls.push_back("system-principal"); return &principalObject; }
}
namespace mozilla { bool SessionHistoryInParent() { calls.push_back("history-query"); return scenario.history; } }
struct SessionHistoryInfo { template<class... Args> explicit SessionHistoryInfo(Args...) {} };
template<class T, class... Args> auto MakeUnique(Args... args) { return std::make_unique<T>(args...); }
const char* nsLiteralCString(const char* text) { return text; }
int Nothing() { return 0; }
struct PromiseFlatString { const nsAString& text; explicit PromiseFlatString(const nsAString& t):text(t) {}
  const char* get() const { return text.c_str(); } };
struct nsDocShell {
  Context context; Context* mBrowsingContext=&context;
  std::unique_ptr<SessionHistoryInfo> mActiveEntry;
  bool IsNavigationAllowed() { calls.push_back("allowed"); return scenario.allowed; }
  bool DisplayLoadError(nsresult, void*, const char*, void*) {
    calls.push_back("display-error"); return scenario.display;
  }
  nsresult LoadURI(const RefPtr<nsDocShellLoadState>& state, bool first) {
    assert(state && first); calls.push_back("load");
    if (scenario.throwing) throw std::runtime_error("load-thrown");
    return scenario.load;
  }
  nsresult FixupOriginal(const nsAString&, const LoadURIOptions&);
  nsresult FixupAndLoadURIString(const nsAString&, const LoadURIOptions&);
};
ORIGINAL
PATCHED
int main(int argc, char** argv) {
  assert(argc == 2); std::string mode=argv[1];
  if (mode == "cap" || mode == "concurrent") {
    auto emit=[] { for (int i=0; i<100; ++i)
      mozilla::makos::WriteNavigationDiagnostic("test",4,0x81234567,true); };
    if (mode == "concurrent") {
      std::vector<std::thread> threads;
      for (int i=0;i<8;++i) threads.emplace_back(emit);
      for (auto& thread:threads) thread.join();
    } else emit();
    assert(writes == 64 && "bounded native admission");
    for (const auto& record:records) assert(record ==
      "MAKOS_FIREFOX_NAV_DIAGNOSTIC stage=test result=0x81234567 load_state=1 scope=diagnostic-only\n"
      && "native result identity");
    return 0;
  }
  if (mode == "short" || mode == "error") {
    fault=mode == "short" ? 3 : -1;
    for (int i=0;i<100;++i) {
      errno=EDOM;
      assert(!mozilla::makos::WriteNavigationDiagnostic("test",4,0,false));
      assert(errno==EDOM && "diagnostic must preserve errno");
    }
    assert(writes == 64 && "failure consumes admission without retry");
    for (const auto& record:records) assert(record.size() == (fault > 0 ? 3u : 0u));
    return 0;
  }
  int number=std::stoi(mode); LoadURIOptions options;
  switch(number) {
    case 0: scenario.allowed=false; break;
    case 1: scenario.create=0x80001234; break;
    case 2: scenario.state=false; break;
    case 3: scenario.create=NS_ERROR_MALFORMED_URI; scenario.display=true;
      scenario.history=true; options.mLoadFlags=LOAD_FLAGS_ERROR_LOAD_CHANGES_RV; break;
    case 4: scenario.create=NS_ERROR_MALFORMED_URI; scenario.display=false; break;
    case 5: scenario.load=0x80005678; break;
    case 6: break;
    case 7: scenario.throwing=true; break;
    case 8: scenario.create=NS_ERROR_MALFORMED_URI; scenario.display=true;
      options.mTriggeringPrincipal=&principalObject; break;
    default: assert(false);
  }
  nsDocShell doc; nsresult expected=0, actual=0; bool oldThrow=false, newThrow=false;
  try { expected=doc.FixupOriginal("private-input",options); }
  catch(const std::runtime_error&) { oldThrow=true; }
  auto expectedCalls=calls; calls.clear();
  errno=EDOM;
  try { actual=doc.FixupAndLoadURIString("private-input",options); }
  catch(const std::runtime_error&) { newThrow=true; }
  assert(errno==EDOM && "diagnostic must preserve errno");
  assert(expected == actual && oldThrow == newThrow && "navigation return unchanged");
  assert(calls == expectedCalls && "navigation call order unchanged");
  std::vector<std::string> stages;
  for (const auto& record:records) {
    assert(record.find("private-input") == std::string::npos);
    assert(record.back() == '\n');
    auto start=record.find("stage=")+6;
    stages.push_back(record.substr(start,record.find(' ',start)-start));
  }
  if (number==0) assert(stages == std::vector<std::string>{"fixup-denied"});
  else {
    assert(stages[0]=="fixup-allowed" && stages[1]=="fixup-created");
    if (number==7) assert(stages.size()==2);
    else {
      assert(stages.size()==3);
      std::string final=number==3 ? "fixup-errorpage-return" :
        ((number==5 || number==6) ? "fixup-load-return" : "fixup-failure-return");
      assert(stages.back()==final);
    }
    if (number==1) assert(records[1].find("result=0x80001234")!=std::string::npos && "creation error identity");
    if (number==2) assert(records[1].find("result=0x00000000 load_state=0")!=std::string::npos);
    if (number==5) assert(records.back().find("result=0x80005678")!=std::string::npos && "load error identity");
  }
}
'''


JAVASCRIPT = r'''
const assert = require("node:assert/strict");
let records=[], calls=[], os="MakOS", throwingDump=false, opens=0, getters=0;
const Services={appinfo:{get OS() { getters++; return os; }}};
function dump(record) { records.push(record); if (throwingDump) throw Error("dump"); }
const Cr={NS_ERROR_LOAD_SHOWED_ERRORPAGE:0x804b001e};
HELPER
function original(url,openUILinkWhere,params,resultDetails) { ORIGINAL }
function patched(url,openUILinkWhere,params,resultDetails) { PATCHED }
function run(fn, error) {
  calls=[]; opens=0;
  const context={
    _notifyStartNavigation(details) { assert.equal(details,17); calls.push("notify"); },
    window:{openTrustedLinkIn(url,where,params) {
      opens++; calls.push("open"); assert.equal(url,"private-input");
      assert.equal(where,"current"); assert.equal(params.secret,123);
      if (error) throw error;
    }},
    handleRevert() { calls.push("revert"); },
    view:{close(value) { assert.deepEqual(value,{showFocusBorder:false}); calls.push("close"); }}
  };
  let outcome;
  try { outcome=fn.call(context,"private-input","current",{secret:123},17); }
  catch(error) { outcome=error.message; }
  return {outcome,calls:[...calls],opens};
}
for (const error of [null,{result:Cr.NS_ERROR_LOAD_SHOWED_ERRORPAGE},
                     {result:0x80001234},{}, {get result(){throw Error("result-getter");}}]) {
  for (const fail of [false,true]) {
    throwingDump=fail; records=[]; makosNavigationAttempts=0;
    const expected=run(original,error); const actual=run(patched,error);
    assert.deepEqual(actual,expected,"JS navigation behavior unchanged");
    assert(records.every(record=>!record.includes("private-input")));
    if (error && Object.getOwnPropertyDescriptor(error,"result")?.value === 0x80001234) assert(records.some(record=>
      record.includes("stage=urlbar-open-catch result=0x80001234")),"JS exception result identity");
  }
}
throwingDump=false; records=[]; makosNavigationAttempts=0;
for(let i=0;i<100;i++) reportMakOSNavigation("test",0);
assert.equal(records.length,32,"bounded JS attempts");
const before=getters; reportMakOSNavigation("test",0); assert.equal(getters,before);
records=[]; makosNavigationAttempts=0; os="Linux";
reportMakOSNavigation("test",0); assert.equal(records.length,0);
os="MakOS"; throwingDump=true;
for(let i=0;i<100;i++) reportMakOSNavigation("test",0);
assert.equal(records.length,32,"failed JS attempts consumed");
'''


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-dir", type=Path)
    args = parser.parse_args()
    old, new = fragments()
    source = args.source_dir or ROOT / "build/ports/firefox/source"
    pinned = args.source_dir is not None or (source / ".git").exists()
    declaration = "nsresult nsDocShell::FixupAndLoadURIString("
    emitter = item(new[HEADER], "inline bool WriteNavigationDiagnostic(")
    patched = item(new[DOC], declaration)
    original = item(old[DOC], declaration).replace("::FixupAndLoadURIString(", "::FixupOriginal(", 1)
    helper = "let makosNavigationAttempts = 0;\n" + item(new[JS], "function reportMakOSNavigation(")
    assert new[DOC].count("#ifdef XP_MAKOS") == 7
    assert emitter.count("write(") == 1 and "aStageLength > 32" in emitter
    assert "Services.appinfo.OS !== \"MakOS\"" in helper
    assert "AppConstants.platform" not in helper
    assert "MAKOS_FIREFOX_PAGE_STOP" not in PATCH.read_text()
    compiler = "/usr/bin/c++" if Path("/usr/bin/c++").exists() else "c++"
    node = shutil.which("node")
    if node is None:
        raise RuntimeError("host Node.js required for exact JavaScript boundary tests")
    with tempfile.TemporaryDirectory(prefix="makos-navigation-test-") as temporary:
        directory = Path(temporary)
        if pinned:
            files = private_apply(source.resolve(), directory)
            assert item(files[DOC], declaration) == patched
            assert item(files[HEADER], "inline bool WriteNavigationDiagnostic(") == emitter
            assert item(files[JS], "function reportMakOSNavigation(") == item(new[JS], "function reportMakOSNavigation(")
            assert js_boundary(files[JS]) == js_boundary(new[JS])
        def native(name: str, emit: str = emitter, body: str = patched) -> Path:
            path = directory / (name + ".cpp")
            path.write_text(CPP.replace("EMITTER", emit).replace("ORIGINAL", original).replace("PATCHED", body))
            binary = directory / name
            subprocess.run([compiler,"-std=c++17","-Wall","-Wextra","-Werror","-O1",
                            "-D_FORTIFY_SOURCE=2","-DXP_MAKOS","-pthread",str(path),"-o",str(binary)],check=True)
            return binary
        binary = native("navigation")
        for mode in [str(i) for i in range(9)] + ["cap","concurrent","short","error"]:
            subprocess.run([str(binary),mode],check=True,timeout=15)
        js = JAVASCRIPT.replace("HELPER",helper).replace("ORIGINAL",js_boundary(old[JS])).replace("PATCHED",js_boundary(new[JS]))
        script = directory / "navigation.js"
        script.write_text(js)
        subprocess.run([node,str(script)],check=True,timeout=15)
        controls = [
            ("uncapped",emitter.replace("prior >= 64","prior >= 640"),patched,"cap","bounded native admission"),
            ("wrong-result",emitter.replace("(aResult >> shift)","((aResult & 0u) >> shift)"),patched,"1","creation error identity"),
            ("changed-return",emitter,patched.replace("return NS_ERROR_FAILURE;","return NS_OK;"),"1","navigation return unchanged"),
        ]
        for name, emit, body, mode, diagnostic in controls:
            result = subprocess.run([str(native(name,emit,body)),mode],capture_output=True,text=True,timeout=15)
            assert result.returncode != 0 and diagnostic in result.stderr, (name,result.stdout,result.stderr)
        for name, mutant, diagnostic in [
            ("js-uncapped",js.replace("makosNavigationAttempts >= 32","makosNavigationAttempts >= 320"),"bounded JS attempts"),
            ("js-error",js.replace('reportMakOSNavigation("urlbar-open-catch", result);','reportMakOSNavigation("urlbar-open-catch", null);'),"JS exception result identity"),
        ]:
            script.write_text(mutant)
            result = subprocess.run([node,str(script)],capture_output=True,text=True,timeout=15)
            assert result.returncode != 0 and diagnostic in result.stderr, (name,result.stdout,result.stderr)
    print("MAKOS_FIREFOX_NAVIGATION_DIAGNOSTICS_HOST_OK native_cases=13 js_behavior_cases=10 "
          "negative_controls=5 source=" + ("pinned-private-apply" if pinned else "patch-extracted-only") +
          " native_transport=bounded-single-write js_transport=best-effort-dump "
          "navigation_policy=unchanged guest_execution=not-run browser_qualification=not-run")


if __name__ == "__main__":
    main()
