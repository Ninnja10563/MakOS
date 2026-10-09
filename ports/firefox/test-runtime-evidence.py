#!/usr/bin/env python3
"""Execute the real bounded emitters without changing runtime qualification.

Host adapters stand in for allocation, Gecko objects and write(2); they are
not guest JIT, rendering, or Mac/HVF evidence. With pinned source available,
apply the actual patch to private original files, never the user's checkout.
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
import resource
import shlex
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "ports/firefox"
PATCH = PORT / "patches/0061-makos-runtime-evidence.patch"
HEADER = "mfbt/MakOSRuntimeEvidence.h"
JIT = "js/src/jit/ExecutableAllocator.cpp"
WR = "gfx/layers/wr/WebRenderLayerManager.cpp"
FILES = (HEADER, "mfbt/moz.build", JIT, WR)


def item(text: str, declaration: str) -> str:
    start = text.index(declaration)
    end = text.index("{", start) + 1
    depth = 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end]


def patch_fragments() -> dict[str, str]:
    result = {}
    for section in PATCH.read_text().split("diff --git ")[1:]:
        path = section.splitlines()[0].split(" b/", 1)[1]
        assert path in FILES
        # Git permits an empty line instead of a space-only context line.
        # Parse hunk counts so that suppressed blank context is preserved,
        # without mistaking the hunk header's newline for source content.
        body = []
        old_left = new_left = 0
        for line in section.splitlines()[1:]:
            if line.startswith("@@"):
                assert old_left == new_left == 0
                match = re.fullmatch(
                    r"@@ -\d+(?:,(\d+))? \+\d+(?:,(\d+))? @@.*", line)
                assert match is not None
                old_left = int(match.group(1) or 1)
                new_left = int(match.group(2) or 1)
            elif old_left or new_left:
                assert line == "" or line[0] in "+ ", "patch must only add code"
                if not line.startswith("+"):
                    old_left -= 1
                new_left -= 1
                assert old_left >= 0 and new_left >= 0
                body.append(line[1:] if line else "")
        assert old_left == new_left == 0
        result[path] = "\n".join(body) + "\n"
    assert set(result) == set(FILES)
    subprocess.run(["git", "apply", "--numstat", str(PATCH)], check=True,
                   stdout=subprocess.PIPE)
    return result


def check_wiring(files: dict[str, str]) -> None:
    header, jit, wr = files[HEADER], files[JIT], files[WR]
    pool = item(jit, "ExecutablePool* ExecutableAllocator::createPool(")
    emitter = item(wr, "static void ReportMakOSPaintSubmission(")
    assert pool.index("if (!m_pools.put(pool))") < pool.index("MAKOS_JIT_POOL_OK")
    assert pool.index("MAKOS_JIT_POOL_OK") < pool.rindex("return pool;")
    assert "MAKOS_JIT_POOL_OK\\n" in pool
    assert "IsPaintingToWindow()" in emitter and "IsPaintingSuppressed()" in emitter
    assert "shell->GetDocument()" in emitter
    assert "GetPrimaryContentDocument" not in emitter
    assert "GetSpec(spec)" in emitter and "NS_FAILED(" in emitter
    assert "browser.xhtml\\n" in emitter and "commonDialog.xhtml\\n" in emitter
    assert wr.count("ReportMakOSPaintSubmission(") == 2
    call = "ReportMakOSPaintSubmission(aDisplayList, aDisplayListBuilder, ret,"
    assert wr.index("if (!ret)") < wr.index(call)
    assert "aRenderOffscreen, size);" in wr
    assert '"MakOSRuntimeEvidence.h",' in files["mfbt/moz.build"]
    assert len(re.findall(r"\bwrite\(", header)) == 1
    assert not re.search(r"\b(?:printf|snprintf|fflush|writev)\(", header + emitter + pool)
    assert "Size <= 256" in header and "aAttempted.exchange(true)" in header
    assert "return written == static_cast<ssize_t>(Size - 1);" in header
    # The real fd-2 console write validates before output, emits the full
    # record under SerialGuard, and returns all bytes. Synthetic host short
    # writes test false return/no retry, not retraction of an emitted prefix.
    arch = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
    write_case = arch.split("        SYS_FILE_WRITE => {", 1)[1].split(
        "        SYS_PACKAGE_INSTALL =>", 1)[0]
    assert write_case.index("user_range_readable(address, length)") < write_case.index(
        "crate::aarch64_tty::write(frame.registers[0], input)")
    tty = item((ROOT / "kernel/src/aarch64_tty.rs").read_text(), "pub fn write(")
    assert tty.index("state.fd_open(pid, fd)") < tty.index("write_tty_bytes(")
    assert tty.index("write_tty_bytes(") < tty.index("Ok(bytes.len())")
    serial = item((ROOT / "kernel/src/serial.rs").read_text(), "fn write_output_bytes(")
    assert serial.index("SerialGuard::acquire()") < serial.index("for &byte in bytes")


def private_source(source: Path, directory: Path) -> dict[str, str]:
    lock = dict(line.split("=", 1) for line in (PORT / "source.lock").read_text().splitlines())
    revision = lock["FIREFOX_COMMIT"]
    private = directory / "patched-source"
    private.mkdir()
    # Prevent git apply from discovering the outer MakOS repository and
    # silently skipping paths when retained evidence lives under build/.
    subprocess.run(["git", "init", "-q", str(private)], check=True)
    for path in FILES:
        if path == HEADER:
            continue
        original = subprocess.check_output(
            ["git", "-C", str(source), "show", f"{revision}:{path}"])
        destination = private / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(original)
    subprocess.run(["git", "apply", "--check", str(PATCH)], cwd=private, check=True)
    subprocess.run(["git", "apply", str(PATCH)], cwd=private, check=True)
    result = {path: (private / path).read_text() for path in FILES}
    check_wiring(result)
    wr = result[WR]
    call = "ReportMakOSPaintSubmission(aDisplayList, aDisplayListBuilder, ret,"
    transaction = item(wr, "void WebRenderLayerManager::EndTransactionWithoutLayer(")
    assert transaction.index("bool ret = WrBridge()->EndTransaction(") < transaction.index(call)
    assert "ReportMakOSPaintSubmission(" not in item(
        wr, "bool WebRenderLayerManager::EndEmptyTransaction(")
    bridge = subprocess.check_output(
        ["git", "-C", str(source), "show",
         f"{revision}:gfx/layers/wr/WebRenderBridgeChild.cpp"], text=True)
    sender = item(bridge, "bool WebRenderBridgeChild::EndTransaction(")
    assert "bool ret = this->SendSetDisplayList(" in sender and "return ret;" in sender
    return result


ADAPTER = r'''
#include <atomic>
#include <cassert>
#include <cerrno>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <limits>
#include <mutex>
#include <string>
#include <thread>
#include <vector>
#include <sys/types.h>
#include <unistd.h>

static std::mutex capture_lock;
static std::vector<std::string> chunks;
static ssize_t fault = -2;
static unsigned writes;
static ssize_t checked_write(int fd, const void* bytes, size_t size) {
  std::lock_guard<std::mutex> lock(capture_lock);
  assert(fd == STDERR_FILENO && size > 0 && size < 256);
  ++writes;
  ssize_t result = fault == -2 ? static_cast<ssize_t>(size) : fault;
  size_t accepted = result > 0 ? static_cast<size_t>(result) : 0;
  if (accepted > size) accepted = size;
  chunks.emplace_back(static_cast<const char*>(bytes), accepted);
  // Every syscall boundary permits an unrelated kernel record. Assertions
  // inspect complete individual writes, never join fragments into a pass.
  if (result < 0) errno = EINTR;
  return result;
}

#include "mozilla/MakOSRuntimeEvidence.h"

static const char jit_record[] = "MAKOS_JIT_POOL_OK\n";
static const char browser_record[] =
    "MAKOS_PRES_PAINT uri=chrome://browser/content/browser.xhtml\n";
static const char dialog_record[] =
    "MAKOS_PRES_PAINT uri=chrome://global/content/commonDialog.xhtml\n";
static unsigned allocations, objects, registrations, releases;
static std::string allocation_fault;
class ExecutableAllocator;
struct ExecutablePool {
  struct Allocation { char* pages; size_t size; } allocation;
  ExecutableAllocator* owner;
  ExecutablePool(ExecutableAllocator* aOwner, Allocation a)
      : allocation(a), owner(aOwner) {}
  ~ExecutablePool();
};
struct PoolSet {
  bool put(ExecutablePool*) { ++registrations; return allocation_fault != "register"; }
};
constexpr size_t ExecutableCodePageSize = 4096;
constexpr size_t OVERSIZE_ALLOCATION = std::numeric_limits<size_t>::max();
class ExecutableAllocator {
 public:
  PoolSet m_pools;
  size_t roundUpAllocationSize(size_t n, size_t) {
    return allocation_fault == "oversize" ? OVERSIZE_ALLOCATION : n;
  }
  ExecutablePool::Allocation systemAlloc(size_t n) {
    ++allocations;
    static char page[4096];
    return {allocation_fault == "map" ? nullptr : page, n};
  }
  void systemRelease(ExecutablePool::Allocation) { ++releases; }
  ExecutablePool* createPool(size_t n);
};
ExecutablePool::~ExecutablePool() { owner->systemRelease(allocation); }
template<class T> static T* js_new(ExecutableAllocator* owner, ExecutablePool::Allocation a) {
  ++objects;
  return allocation_fault == "object" ? nullptr : new T(owner, a);
}
static void js_delete(ExecutablePool* pool) { delete pool; }

using nsAutoCString = struct String {
  std::string value;
  bool EqualsLiteral(const char* literal) const { return value == literal; }
};
struct nsIURI {
  std::string value = "chrome://browser/content/browser.xhtml";
  bool fail = false;
  int GetSpec(nsAutoCString& spec) { spec.value = value; return fail ? -1 : 0; }
};
#define NS_FAILED(value) ((value) < 0)
namespace mozilla {
namespace dom {
struct Document {
  nsIURI* uri;
  nsIURI* GetDocumentURI() { return uri; }
};
}
struct PresShell {
  bool active = true, suppressed = false;
  dom::Document* document;
  dom::Document* primary;
  bool IsActive() { return active; }
  bool IsPaintingSuppressed() { return suppressed; }
  dom::Document* GetDocument() { return document; }
  dom::Document* GetPrimaryContentDocument() { return primary; }
};
}
struct nsPresContext {
  mozilla::PresShell* shell;
  mozilla::PresShell* PresShell() { return shell; }
};
struct nsIFrame {
  nsPresContext* context;
  nsPresContext* PresContext() { return context; }
};
struct nsDisplayList { bool empty = false; bool IsEmpty() { return empty; } };
struct nsDisplayListBuilder {
  nsIFrame* root;
  bool window = true;
  nsIFrame* RootReferenceFrame() { return root; }
  bool IsPaintingToWindow() { return window; }
};
struct LayoutDeviceIntSize {
  int width = 700, height = 400;
  bool IsEmpty() const { return width <= 0 || height <= 0; }
};
using namespace mozilla;
'''

TESTS = r'''
static void reset_capture() { chunks.clear(); writes = 0; fault = -2; }
static void pool_test(const std::string& mode) {
  ExecutableAllocator allocator;
  allocation_fault = mode;
  auto* pool = allocator.createPool(ExecutableCodePageSize);
  if (mode != "success") {
    assert(!pool && writes == 0 && chunks.empty());
    assert(allocations == (mode == "oversize" ? 0u : 1u));
    assert(objects == (mode == "oversize" || mode == "map" ? 0u : 1u));
    assert(registrations == (mode == "register" ? 1u : 0u));
    assert(releases == (mode == "register" || mode == "object" ? 1u : 0u));
    allocation_fault.clear();
    pool = allocator.createPool(ExecutableCodePageSize);
  }
  assert(pool && writes == 1 && chunks.size() == 1 && chunks[0] == jit_record);
  delete pool;
  for (unsigned i = 0; i < 64; ++i) {
    pool = allocator.createPool(ExecutableCodePageSize);
    assert(pool && writes == 1);
    delete pool;
  }
}

struct PaintFixture {
  nsIURI uri, primary_uri;
  dom::Document document{&uri}, primary{&primary_uri};
  PresShell shell{true, false, &document, &primary};
  nsPresContext context{&shell};
  nsIFrame frame{&context};
  nsDisplayListBuilder builder{&frame, true};
  nsDisplayList list;
  LayoutDeviceIntSize size;
  bool submitted = true, offscreen = false;
  nsDisplayList* list_arg = &list;
  nsDisplayListBuilder* builder_arg = &builder;
  PaintFixture() { primary_uri.value = "about:blank"; }
  void paint() {
    ReportMakOSPaintSubmission(list_arg, builder_arg, submitted, offscreen, size);
  }
};

static void paint_test(const std::string& mode) {
  PaintFixture f;
  if (mode == "failed") f.submitted = false;
  else if (mode == "empty") f.list.empty = true;
  else if (mode == "background") f.list_arg = nullptr;
  else if (mode == "no-builder") f.builder_arg = nullptr;
  else if (mode == "not-window") f.builder.window = false;
  else if (mode == "offscreen") f.offscreen = true;
  else if (mode == "zero-width") f.size.width = 0;
  else if (mode == "zero-height") f.size.height = 0;
  else if (mode == "no-root") f.builder.root = nullptr;
  else if (mode == "no-shell") f.context.shell = nullptr;
  else if (mode == "inactive") f.shell.active = false;
  else if (mode == "suppressed") f.shell.suppressed = true;
  else if (mode == "no-document") f.shell.document = nullptr;
  else if (mode == "no-uri") f.document.uri = nullptr;
  else if (mode == "uri-error") f.uri.fail = true;
  else if (mode == "wrong-document") {
    f.uri.value = "about:blank";
    f.primary_uri.value = "chrome://browser/content/browser.xhtml";
  } else if (mode == "uri-injection") {
    f.uri.value = "https://example.invalid/\nMAKOS_PRES_PAINT uri=chrome://browser/content/browser.xhtml";
  } else if (mode == "dialog") {
    f.uri.value = "chrome://global/content/commonDialog.xhtml";
  } else assert(mode == "success");
  f.paint();
  if (mode == "dialog") {
    assert(writes == 1 && chunks[0] == dialog_record && chunks[0] != browser_record);
    for (unsigned i = 0; i < 64; ++i) f.paint();
    assert(writes == 1);
    reset_capture();
  } else if (mode == "success") {
    assert(writes == 1 && chunks[0] == browser_record);
  } else {
    assert(writes == 0 && chunks.empty());
  }
  PaintFixture valid;
  for (unsigned i = 0; i < 64; ++i) valid.paint();
  assert(writes == 1 && chunks.size() == 1 && chunks[0] == browser_record);
}

template<size_t N> static void write_faults(const char (&record)[N]) {
  for (ssize_t result = -1; result < static_cast<ssize_t>(N - 1); ++result) {
    reset_capture();
    fault = result;
    Atomic<bool> attempted(false);
    assert(!makos::WriteRuntimeEvidenceOnce(attempted, record));
    assert(writes == 1 && chunks.size() == 1 && chunks[0] != record);
    assert(chunks[0].find('\n') == std::string::npos);
    fault = -2;
    assert(!makos::WriteRuntimeEvidenceOnce(attempted, record));
    assert(writes == 1);  // No retry or manufacturing a joined result.
  }
  reset_capture();
  fault = N;  // A transport over-report is also not a successful write.
  Atomic<bool> attempted(false);
  assert(!makos::WriteRuntimeEvidenceOnce(attempted, record));
  assert(writes == 1);
}

static void writer_test(const std::string& mode) {
  if (mode == "faults") {
    write_faults(jit_record); write_faults(browser_record); write_faults(dialog_record);
    return;
  }
  Atomic<bool> attempted(false);
  if (mode == "concurrent") {
    std::vector<std::thread> workers;
    for (unsigned i = 0; i < 16; ++i) {
      workers.emplace_back([&]() {
        (void)makos::WriteRuntimeEvidenceOnce(attempted, browser_record);
      });
    }
    for (auto& thread : workers) thread.join();
  } else {
    assert(mode == "success");
    assert(makos::WriteRuntimeEvidenceOnce(attempted, browser_record));
    assert(!makos::WriteRuntimeEvidenceOnce(attempted, browser_record));
  }
  assert(writes == 1 && chunks.size() == 1 && chunks[0] == browser_record);
}

int main(int argc, char** argv) {
  assert(argc == 3);
  std::string kind = argv[1], mode = argv[2];
  if (kind == "pool") pool_test(mode);
  else if (kind == "paint") paint_test(mode);
  else { assert(kind == "write"); writer_test(mode); }
}
'''


def no_core() -> None:
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def host_tests(files: dict[str, str], directory: Path) -> tuple[int, int]:
    base = directory / "host-tests"
    include = base / "mozilla"
    include.mkdir(parents=True)
    (include / "Atomics.h").write_text(
        "#pragma once\n#include <atomic>\nnamespace mozilla { "
        "template<class T> using Atomic = std::atomic<T>; }\n")
    header = files[HEADER]
    pool = item(files[JIT], "ExecutablePool* ExecutableAllocator::createPool(")
    report = item(files[WR], "static void ReportMakOSPaintSubmission(")
    compiler = shlex.split(os.environ.get("HOST_CXX", "c++"))
    if not compiler or not shutil.which(compiler[0]):
        raise RuntimeError("host C++ compiler missing; set HOST_CXX")

    def compile_case(name: str, writer=header, allocator=pool, painter=report) -> Path:
        # Adapt only the extracted write call; never redefine SDK macros or
        # disable Darwin fortification/-Werror to make the host test compile.
        adapted, count = re.subn(r"\bwrite(?=\s*\()", "checked_write", writer)
        assert count == 1
        (include / "MakOSRuntimeEvidence.h").write_text(adapted)
        source = base / f"{name}.cpp"
        source.write_text(ADAPTER + "\n" + allocator + "\n" + painter + "\n" + TESTS)
        binary = base / name
        subprocess.run([*compiler, "-std=c++17", "-Wall", "-Wextra", "-Werror", "-O2",
                        "-pthread", "-DXP_MAKOS", f"-I{base}", str(source), "-o", str(binary)],
                       check=True)
        return binary

    def run(binary: Path, kind: str, mode: str, expect_success: bool = True) -> None:
        result = subprocess.run([str(binary), kind, mode], capture_output=True,
                                preexec_fn=no_core)
        assert (result.returncode == 0) == expect_success, (
            binary.name, kind, mode, result.returncode, result.stderr.decode(errors="replace"))

    binary = compile_case("runtime-evidence")
    cases = [("pool", mode) for mode in ("success", "oversize", "map", "object", "register")]
    cases += [("paint", mode) for mode in (
        "success", "failed", "empty", "background", "no-builder", "not-window", "offscreen",
        "zero-width", "zero-height", "no-root", "no-shell", "inactive", "suppressed",
        "no-document", "no-uri", "uri-error", "wrong-document", "uri-injection", "dialog")]
    cases += [("write", mode) for mode in ("success", "faults", "concurrent")]
    for kind, mode in cases:
        run(binary, kind, mode)

    # Each broken production variant must compile and fail its behavioral
    # assertion; compiler failures never count as a detected negative control.
    jit_emission = re.search(r"#ifdef XP_MAKOS\n.*?#endif", pool, re.DOTALL).group()
    premature_pool = pool.replace(jit_emission + "\n", "").replace(
        "{\n", "{\n" + jit_emission + "\n", 1)
    controls = [
        ("missing-jit", header, pool.replace("MAKOS_JIT_POOL_OK", "MAKOS_JIT_POOL_ABSENT"),
         report, "pool", "success"),
        ("missing-chrome", header, pool,
         report.replace("MAKOS_PRES_PAINT uri=chrome://browser/", "MAKOS_PRES_PAINT_ABSENT uri=chrome://browser/"),
         "paint", "success"),
        ("premature-jit", header, premature_pool, report, "pool", "map"),
        ("failed-submission", header, pool, report.replace("!aSubmitted", "(aSubmitted && false)"),
         "paint", "failed"),
        ("empty-list", header, pool, report.replace("aDisplayList->IsEmpty()", "(aDisplayList->IsEmpty() && false)"),
         "paint", "empty"),
        ("wrong-document", header, pool, report.replace("shell->GetDocument()", "shell->GetPrimaryContentDocument()"),
         "paint", "wrong-document"),
        ("unbounded-writes", header.replace("aAttempted.exchange(true)", "(aAttempted.exchange(true) && false)"),
         pool, report, "write", "success"),
        ("unchecked-short-write", header.replace("written == static_cast<ssize_t>(Size - 1)", "written >= 0"),
         pool, report, "write", "faults"),
    ]
    for name, writer, allocator, painter, kind, mode in controls:
        mutated = compile_case(name, writer, allocator, painter)
        run(mutated, kind, mode, False)
    # Restore the retained header to actual production-derived contents.
    compile_case("runtime-evidence")
    return len(cases), len(controls)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-dir", type=Path, help="require private apply of pinned upstream source")
    parser.add_argument("--evidence-dir", type=Path, help="retain artifacts in a NEW directory")
    args = parser.parse_args()
    fragments = patch_fragments()
    check_wiring(fragments)
    source = args.source_dir or ROOT / "build/ports/firefox/source"
    use_source = args.source_dir is not None or (source / ".git").exists()
    source = source.resolve()

    def perform(directory: Path) -> None:
        files = private_source(source, directory) if use_source else fragments
        # The compiled functions must equal the exact patch payload/context,
        # not hand-copied replacements that could drift from the port.
        for path, declaration in ((JIT, "ExecutablePool* ExecutableAllocator::createPool("),
                                  (WR, "static void ReportMakOSPaintSubmission(")):
            assert item(files[path], declaration) == item(fragments[path], declaration)
        assert files[HEADER] == fragments[HEADER]
        cases, controls = host_tests(files, directory)
        print("MAKOS_FIREFOX_RUNTIME_EVIDENCE_HOST_OK "
              f"cases={cases} negative_controls={controls} "
              f"source={'pinned-private-apply' if use_source else 'patch-extracted-only'} "
              "jit=registered-pool paint=successful-nonempty-display-list-submission "
              "transport=single-checked-bounded-write attempts=once-per-kind "
              "guest_execution=not-run pixel_qualification=not-run")
        if args.evidence_dir:
            print(f"evidence_directory={directory}")

    if args.evidence_dir:
        destination = args.evidence_dir.resolve()
        if destination == source or source in destination.parents:
            raise ValueError("evidence directory must not alter the Firefox source tree")
        destination.mkdir(parents=True, exist_ok=False)
        perform(destination)
    else:
        with tempfile.TemporaryDirectory(prefix="makos-firefox-runtime-evidence-") as temporary:
            perform(Path(temporary))


if __name__ == "__main__":
    main()
