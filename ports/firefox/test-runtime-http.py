#!/usr/bin/env python3
"""Private production-derived HTTP diagnostic tests; no browser qualification.

Full mode executes selected complete functions with explicit host interfaces.
Source-free mode checks exact patch policy restoration and emitter transport only.
Neither mode models DNS, HTTP transport, Gecko threading or guest behavior.
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
PORT = ROOT / 'ports/firefox'
PATCH = PORT / 'patches/0065-makos-http-continuations.patch'
HTTP = 'netwerk/protocol/http/nsHttpChannel.cpp'
CHILD = 'netwerk/protocol/http/HttpChannelChild.cpp'
CLASS = 'netwerk/url-classifier/AsyncUrlChannelClassifier.cpp'
spec = importlib.util.spec_from_file_location('channel_fixture', PORT / 'test-runtime-channel.py')
channel = importlib.util.module_from_spec(spec)
assert spec.loader
spec.loader.exec_module(channel)


def fragments():
    before, after = {}, {}
    for section in PATCH.read_text().split('diff --git ')[1:]:
        path = section.splitlines()[0].split(' b/', 1)[1]
        assert path in (HTTP, CHILD, CLASS)
        old, new = [], []
        left = right = 0
        for line in section.splitlines()[1:]:
            if line.startswith('@@'):
                assert left == right == 0
                match = re.match(r'@@ -\d+,(\d+) \+\d+,(\d+) @@', line)
                assert match
                left, right = map(int, match.groups())
            elif left or right:
                # Existing patches use bare empty context lines accepted by git.
                line = line or ' '
                assert line[0] in ' +-'
                if line[0] != '+':
                    old.append(line[1:]); left -= 1
                if line[0] != '-':
                    new.append(line[1:]); right -= 1
                assert min(left, right) >= 0
        assert left == right == 0
        before[path], after[path] = '\n'.join(old) + '\n', '\n'.join(new) + '\n'
    assert set(before) == {HTTP, CHILD, CLASS}
    return before, after


def restore(text, path):
    # Erase only MakOS diagnostic blocks, never existing platform guards.
    def block(match):
        assert any(token in match[0] for token in ('MakOS', 'MAKOS_', 'mMakOSDiagnosticChannelId'))
        return ''
    text = re.sub(r'#ifdef XP_MAKOS\n[\s\S]*?#endif\n', block, text)
    text = re.sub(r'^ *MAKOS_(?:HTTP_CONT|HTTP_CHILD_CONT|CLASS_CONT)\([\s\S]*?\);\n', '', text, flags=re.M)
    text = re.sub(r'^ *MAKOS_HTTP(?:_CHILD)?_SNAPSHOT\(\);\n', '', text, flags=re.M)
    if path == HTTP:
        text = text.replace('    nsresult classifierResult =\n', '    willCallback = NS_SUCCEEDED(\n')
        text = text.replace('        });\n    willCallback = NS_SUCCEEDED(classifierResult);', '        }));')
        text = text.replace('  nsresult continuationResult = aFunc(this);\n  return continuationResult;', '  return aFunc(this);')
        # Exactly the newly inserted blank lines adjacent to removed markers.
        text = text.replace('  CallOnModifyRequestObservers();\n\n\n', '  CallOnModifyRequestObservers();\n\n')
        text = text.replace('    mOnTailUnblock = &nsHttpChannel::AsyncOpenOnTailUnblock;\n\n\n', '    mOnTailUnblock = &nsHttpChannel::AsyncOpenOnTailUnblock;\n\n')
    elif path == CLASS:
        text = text.replace('        const nsresult dispatchResult = NS_DispatchToMainThread(r);\n        (void)dispatchResult;  // Preserve the original ignored-result policy.', '        NS_DispatchToMainThread(r);')
        text = text.replace('  rv = nsUrlClassifierDBService::BackgroundThread()->Dispatch(\n      r, NS_DISPATCH_NORMAL);\n  return rv;', '  return nsUrlClassifierDBService::BackgroundThread()->Dispatch(\n      r, NS_DISPATCH_NORMAL);')
        text = text.replace('  void CompleteClassification();\n\n\n', '  void CompleteClassification();\n\n')
        text = text.replace('  RefPtr<CallbackHolder> mCallbackHolder;\n\n\n', '  RefPtr<CallbackHolder> mCallbackHolder;\n\n')
        text = text.replace('  mCallbackHolder = new CallbackHolder(std::move(callback));\n\n}', '  mCallbackHolder = new CallbackHolder(std::move(callback));\n}')
    # The include block adds a surrounding blank after HttpLog, but before
    # Components in classifier. These exact anchor replacements preserve policy.
    text = text.replace('#include "HttpLog.h"\n\n\n', '#include "HttpLog.h"\n\n')
    text = text.replace('#include "Classifier.h"\n\n#include "mozilla/Components.h"', '#include "Classifier.h"\n#include "mozilla/Components.h"')
    return text


def structural(before, after):
    for path in before:
        assert restore(after[path], path) == before[path], 'production policy restoration: ' + path
    assert 'nsIIdentChannel.h' not in PATCH.read_text(), 'generated channel interface belongs to nsIChannel.h'
    stages = re.findall(r'MAKOS_(?:HTTP_CONT|HTTP_CHILD_CONT|CLASS_CONT)\("([a-z-]+)"', PATCH.read_text())
    assert stages and max(map(len, stages)) <= 32
    assert not any(value in PATCH.read_text() for value in ('PAGE_STOP', 'TLS_OK', 'HTTP_OK', 'GetAsciiSpec('))
    worker = after[CLASS].split('[task, workerClassifier]() -> void {', 1)[1]
    worker = worker.split('});\n', 1)[0]
    assert not any(value in worker for value in ('do_QueryInterface', '->ChannelId()', 'GetURI(')), 'no worker COM metadata access'
    assert 'task->MakOSDiagnosticChannelId()' in worker


COMMON = r'''
#include <atomic>
#include <cassert>
#include <cerrno>
#include <cstdint>
#include <cstdio>
#include <functional>
#include <string>
#include <vector>
#include <unistd.h>
template<class T> struct Atomic {std::atomic<T> v; explicit Atomic(T x):v(x){} operator T() const{return v.load();} Atomic& operator=(T x){v.store(x);return *this;} bool compareExchange(T old,T next){return v.compare_exchange_strong(old,next);}};
std::vector<std::string> records,calls;
ssize_t diagnostic_write(int fd,const void* p,size_t n){assert(fd==2 && n<256);errno=EIO;records.emplace_back(static_cast<const char*>(p),n);return n;}
#define write diagnostic_write
namespace mozilla::makos { EMITTER }
#undef write
using nsresult=uint32_t;
constexpr nsresult NS_OK=0,NS_ERROR_FAILURE=0x80004005;
[[maybe_unused]] constexpr nsresult NS_ERROR_NOT_AVAILABLE=0x80040111;
#define NS_FAILED(x) (((x)&0x80000000u)!=0)
#define NS_SUCCEEDED(x) (!NS_FAILED(x))
#define MOZ_ASSERT(...) ((void)0)
#define LOG(...) ((void)0)
#define AUTO_PROFILER_LABEL(...) ((void)0)
#define AUTO_PROFILER_FLOW_MARKER(...) ((void)0)
#define NS_IMETHODIMP nsresult
#define XP_MAKOS 1
MACROS
struct Ignored {template<class T> void operator<<(T) const{}};[[maybe_unused]] constexpr Ignored Unused{};
template<class T> struct RefPtr {T* p=nullptr;RefPtr()=default;RefPtr(T* v):p(v){} T* operator->()const{return p;} operator T*()const{return p;} RefPtr& operator=(T* v){p=v;return *this;} T* get()const{return p;}};
template<class T> using nsCOMPtr=RefPtr<T>;
template<class T>T** getter_AddRefs(RefPtr<T>& p){return &p.p;}
bool has(const char* stage,nsresult result){char prefix[256];int n=snprintf(prefix,sizeof(prefix),"MAKOS_FIREFOX_LOAD_DIAGNOSTIC stage=%s result=0x%08x load=0x0000000000000000 channel=0x000000000000001c flags=0x00000000 scope=diagnostic-only\n",stage,result);assert(n>0 && static_cast<size_t>(n)<sizeof(prefix));for(auto& r:records)if(r==prefix)return true;return false;}
void arm(){assert(mozilla::makos::WriteLoadDiagnostic("arm",3,0,0,28,13));records.clear();}
'''

HTTP_ADAPTER = r'''
struct Config {bool proxy=false,bypass=false,active=true,classify=true,auth=false,overridden=false;nsresult proxyResult=0,beginResult=0,classifierResult=0,childResult=0;bool classifierSync=false,proxySync=false;} cfg;
struct nsIStreamListener{} listener;
struct nsIURI{} uri;
struct Origin {bool IsPrivateBrowsing(){return false;}};
struct LoadInfo {int GetInnerWindowID(){return 0;} Origin GetOriginAttributes(){return {};} void GetRequestBlockingReason(uint32_t* p){calls.push_back("blocking-reason");*p=1;}} loadInfo;
struct Handler {bool Active(){return cfg.active;} template<class T>void OnFailedOpeningRequest(T*){calls.push_back("child-failed");}} handler;
Handler* gHttpHandler=&handler;
struct TimeStamp {int value=0;static TimeStamp Now(){return {10};}};
struct nsAutoCString{};
bool profiler_thread_is_being_profiled_for_markers(){return false;}
template<class... T>void profiler_add_network_marker(T...){}
enum class NetworkLoadType{LOAD_START};
enum class ClassifyType{ETP};
namespace nsHttp {constexpr int Authorization=1;}
struct RequestHead {bool HasHeader(int){return false;}};
struct Service {int Flags(){return 0;}};
constexpr unsigned LOAD_ONLY_FROM_CACHE=1,LOAD_NO_NETWORK_IO=2;
struct Group {template<class T>void AddRequest(T*,void*){calls.push_back("loadgroup-add");}} group;
std::function<void()> classification;
struct AsyncUrlChannelClassifier {template<class T>static nsresult CheckChannel(T*,std::function<void()> callback){calls.push_back("classify");if(NS_SUCCEEDED(cfg.classifierResult)){if(cfg.classifierSync)callback();else classification=callback;}return cfg.classifierResult;}};
template<class T>bool NS_ShouldClassifyChannel(T*,ClassifyType){return cfg.classify;}
template<class T,class U>void FinishAntiTrackingRedirectHeuristic(T,U){calls.push_back("heuristic");}
NAMESPACE_BEGIN
struct nsHttpChannel {
 uint64_t mChannelId=28;nsresult mStatus=NS_ERROR_FAILURE;bool mCanceled=false;unsigned mSuspendCount=0,mLoadFlags=0;bool mProxyInfo=false;
 std::function<nsresult(nsHttpChannel*)> mCallOnResume;
 TimeStamp mLastStatusReported,mChannelCreationTimestamp,mAsyncOpenTime;
 nsIURI* mURI=&uri;LoadInfo* mLoadInfo=&loadInfo;Group* mLoadGroup=&group;int mPriority=0,mCacheDisposition=0;RequestHead mRequestHead;Service mClassOfService;
 bool BypassProxy(){return cfg.bypass;} bool LoadAuthRedirectedChannel(){return cfg.auth;} bool LoadAsyncOpenTimeOverriden(){return cfg.overridden;}
 void GetRequestMethod(nsAutoCString&){} void StoreCustomAuthHeader(bool){calls.push_back("auth-header");}
 void GetURI(nsIURI** p){calls.push_back("uri");*p=&uri;}
 nsresult ResolveProxy(){calls.push_back("proxy");if(cfg.proxySync){calls.push_back("proxy-callback");BeginConnect();}return cfg.proxyResult;}
 nsresult BeginConnect(){calls.push_back("begin");return cfg.beginResult;}
 void CloseCacheEntry(bool){calls.push_back("close-cache");} nsresult AsyncAbort(nsresult r){calls.push_back("abort:"+std::to_string(r));return NS_OK;}
 void AsyncOpenFinal(TimeStamp);
 void MaybeResolveProxyAndBeginConnect();
 nsresult CallOrWaitForResume(const std::function<nsresult(nsHttpChannel*)>&);
};
struct HttpChannelChild {uint64_t mChannelId=28;LoadInfo* mLoadInfo=&loadInfo;nsresult AsyncOpenInternal(nsIStreamListener*){calls.push_back("child-internal");return cfg.childResult;} nsresult AsyncOpen(nsIStreamListener*);};
FUNCTIONS
NAMESPACE_END
'''

HTTP_MAIN = r'''
template<class T>std::vector<std::string> exercise(const std::string& mode){
 calls.clear();classification={};T c;c.mProxyInfo=cfg.proxy;
 if(mode.rfind("final",0)==0){c.AsyncOpenFinal({7});calls.push_back("final-return");if(classification){auto cb=classification;classification={};cb();}}
 else if(mode.rfind("proxy",0)==0){c.MaybeResolveProxyAndBeginConnect();}
 else {c.mCanceled=mode=="cancel";c.mSuspendCount=mode=="suspend";auto fn=[&mode](auto* self){calls.push_back("continuation");if(mode=="change-id")self->mChannelId=99;return NS_ERROR_FAILURE;};auto r=c.CallOrWaitForResume(fn);calls.push_back("result:"+std::to_string(r));if(c.mCallOnResume){calls.push_back("resume");auto pending=c.mCallOnResume;c.mCallOnResume={};calls.push_back("resumed:"+std::to_string(pending(&c)));}}
 return calls;
}
int main(int argc,char** argv){assert(argc==2);std::string mode=argv[1];arm();errno=EBUSY;
 if(mode=="child-ok"||mode=="child-error"){cfg.childResult=mode=="child-ok"?NS_OK:NS_ERROR_FAILURE;original::HttpChannelChild a;changed::HttpChannelChild b;calls.clear();auto expected=a.AsyncOpen(&listener);auto previous=calls;calls.clear();auto result=b.AsyncOpen(&listener);assert(result==expected && calls==previous && "child policy unchanged");assert(has("http-child-open-return",cfg.childResult) && "raw child result");}
 else {
 if(mode=="final-failure")cfg.classifierResult=NS_ERROR_FAILURE;
 if(mode=="final-sync")cfg.classifierSync=true;
 if(mode=="final-skip")cfg.classify=false;
 if(mode=="final-auth")cfg.auth=true;
 if(mode=="proxy-error")cfg.proxyResult=NS_ERROR_FAILURE;
 if(mode=="proxy-sync")cfg.proxySync=true;
 if(mode=="proxy-bypass")cfg.bypass=true;
 if(mode=="proxy-existing")cfg.proxy=true;
 if(mode=="proxy-inactive"){cfg.bypass=true;cfg.active=false;}
 if(mode=="proxy-begin-error"){cfg.bypass=true;cfg.beginResult=NS_ERROR_FAILURE;}
 auto expected=exercise<original::nsHttpChannel>(mode);auto actual=exercise<changed::nsHttpChannel>(mode);
 assert(actual==expected && "HTTP policy call order unchanged");
 if(mode.rfind("final",0)==0 && cfg.classify && !cfg.auth){assert(has("http-classifier-return",cfg.classifierResult) && "raw classifier result");if(NS_SUCCEEDED(cfg.classifierResult))assert(has("http-classifier-callback",NS_OK) && "classifier callback observed");}
 if(mode=="proxy-begin-error")assert(has("http-begin-connect-return",NS_ERROR_FAILURE) && "raw begin result");
 if(mode=="cancel")assert(has("http-resume-canceled",NS_ERROR_FAILURE) && "raw cancel result");
 if(mode=="direct")assert(has("http-resume-return",NS_ERROR_FAILURE) && "raw continuation result");
 if(mode=="change-id")assert(has("http-resume-return",NS_ERROR_FAILURE) && "precallback channel identity preserved");
 if(mode=="suspend")assert(has("http-resume-queued",NS_OK) && "suspension observed");
 }
 assert(errno==EBUSY && "diagnostic errno preserved");
}
'''

CLASS_ADAPTER = r'''
#include <type_traits>
#define UC_LOG(...) ((void)0)
#define NS_DISPATCH_NORMAL 0
bool mainThread=true,stopFeature=false;unsigned metadataQueries=0;
bool NS_IsMainThread(){return mainThread;}
struct nsIIdentChannel {uint64_t id=28;uint64_t ChannelId(){assert(mainThread && "metadata queried on main thread only");++metadataQueries;errno=ENOSPC;return id;}};
struct nsIChannel:nsIIdentChannel{} sourceChannel;
nsIIdentChannel* do_QueryInterface(nsIChannel* p){assert(mainThread);errno=EACCES;return p;}
template<class T>struct ComPtr:RefPtr<T>{using RefPtr<T>::RefPtr;~ComPtr(){if constexpr(std::is_same_v<T,nsIIdentChannel>)if(this->p)errno=ERANGE;}};
#define nsCOMPtr ComPtr
struct nsUrlClassifierDBServiceWorker{} worker;
struct Runnable {std::function<void()> run;};
template<class F>Runnable* NS_NewRunnableFunction(const char*,F f){return new Runnable{f};}
std::vector<Runnable*> backgroundQueue,mainQueue;
nsresult backgroundResult=0,mainResult=0;
nsresult NS_DispatchToMainThread(Runnable* r){calls.push_back("main-dispatch");if(NS_SUCCEEDED(mainResult))mainQueue.push_back(r);return mainResult;}
struct Thread {nsresult Dispatch(Runnable* r,int){calls.push_back("worker-dispatch");if(NS_SUCCEEDED(backgroundResult))backgroundQueue.push_back(r);return backgroundResult;}} workerThread;
struct nsUrlClassifierDBService {static Thread* BackgroundThread(){return &workerThread;}};
struct CallbackHolder {std::function<void()> cb;explicit CallbackHolder(std::function<void()> f):cb(f){}void Exec(){calls.push_back("callback");cb();}};
struct FeatureData {bool MaybeCompleteClassification(nsIChannel*){calls.push_back("feature");return !stopFeature;}};
NAMESPACE_BEGIN
struct FeatureTask {
 RefPtr<nsIChannel> mChannel;RefPtr<CallbackHolder> mCallbackHolder;
 uint64_t mMakOSDiagnosticChannelId=0;std::vector<FeatureData> mFeatures=std::vector<FeatureData>(2);
 FeatureTask(nsIChannel*,std::function<void()>&&);
 uint64_t MakOSDiagnosticChannelId()const{return mMakOSDiagnosticChannelId;}
 void DoLookup(nsUrlClassifierDBServiceWorker*){assert(!mainThread);calls.push_back("lookup");}
 void CompleteClassification();
};
CONSTRUCTOR
COMPLETE
nsresult dispatch(RefPtr<FeatureTask> task,RefPtr<nsUrlClassifierDBServiceWorker> workerClassifier){
 [[maybe_unused]] nsresult rv=0;
DISPATCH
}
NAMESPACE_END
'''

CLASS_MAIN = r'''
template<class T,class F>std::vector<std::string> exercise(F dispatch){
 calls.clear();backgroundQueue.clear();mainQueue.clear();mainThread=true;sourceChannel.id=28;errno=EBUSY;
 auto* task=new T(&sourceChannel,[]{calls.push_back("consumer");});
 assert(errno==EBUSY && "capture metadata destructor errno preserved");
 sourceChannel.id=99; // Worker must use constructor snapshot, not current COM data.
 auto result=dispatch(task,&worker);calls.push_back("result:"+std::to_string(result));
 mainThread=false;for(auto* r:backgroundQueue)r->run();
 mainThread=true;for(auto* r:mainQueue)r->run();
 return calls;
}
int main(int argc,char** argv){assert(argc==2);std::string mode=argv[1];arm();
 if(mode=="worker-fail")backgroundResult=NS_ERROR_FAILURE;
 if(mode=="main-fail")mainResult=NS_ERROR_FAILURE;
 if(mode=="feature-stop")stopFeature=true;
 auto expected=exercise<original::FeatureTask>(original::dispatch);
 auto actual=exercise<changed::FeatureTask>(changed::dispatch);
 assert(actual==expected && "classifier dispatch callback policy unchanged");
 assert(metadataQueries==1 && "single main-thread identity capture");
 assert(has("classifier-worker-dispatch",backgroundResult) && "raw worker dispatch result");
 if(NS_SUCCEEDED(backgroundResult))assert(has("classifier-main-dispatch",mainResult) && "raw main dispatch result");
 if(NS_SUCCEEDED(backgroundResult)&&NS_SUCCEEDED(mainResult))assert(has("classifier-callback-return",NS_OK) && "classifier completion callback observed");
 assert(errno==EBUSY && "diagnostic errno preserved");
}
'''


def main():
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group()
    group.add_argument('--source-dir', type=Path)
    group.add_argument('--patch-only', action='store_true')
    args = parser.parse_args()
    source = args.source_dir or ROOT / 'build/ports/firefox/source'
    full = not args.patch_only and (args.source_dir is not None or (source / '.git').exists())
    with tempfile.TemporaryDirectory(prefix='makos-http-diagnostic-') as temporary:
        directory = Path(temporary)
        if full:
            _, headers = channel.private_apply(source, directory)
            checkout = directory / 'source'
            commit = dict(line.split('=', 1) for line in (PORT / 'source.lock').read_text().splitlines())['FIREFOX_COMMIT']
            before = {}
            for path in (HTTP, CHILD, CLASS):
                before[path] = subprocess.check_output(['git', '-C', str(source), 'show', f'{commit}:{path}'], text=True)
                target = checkout / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(before[path])
            subprocess.run(['git', 'apply', '--check', str(PATCH)], cwd=checkout, check=True)
            subprocess.run(['git', 'apply', str(PATCH)], cwd=checkout, check=True)
            after = {path: (checkout / path).read_text() for path in before}
            assert 'interface nsIIdentChannel' in subprocess.check_output(['git', '-C', str(source), 'show', f'{commit}:netwerk/base/nsIChannel.idl'], text=True)
        else:
            _, headers = channel.patch_fragments()
            before, after = fragments()
        structural(before, after)
        emitter = channel.item(headers[channel.HEADER], 'inline bool WriteLoadDiagnostic(')
        macros = '\n'.join(re.search(r'#ifdef XP_MAKOS\n[\s\S]*?#endif', after[path])[0] for path in (HTTP, CHILD, CLASS))
        macros = re.sub(r'^#  include .*\n', '', macros, flags=re.M)
        common = COMMON.replace('EMITTER', emitter).replace('MACROS', macros)

        def native(name, code):
            target = directory / (name + '.cpp')
            target.write_text(code)
            binary = directory / name
            compiler = '/usr/bin/c++' if Path('/usr/bin/c++').exists() else shutil.which('c++')
            assert compiler
            subprocess.run([compiler, '-std=c++17', '-Wall', '-Wextra', '-Werror', '-O1', '-D_FORTIFY_SOURCE=2', '-pthread', str(target), '-o', str(binary)], check=True)
            return binary

        modes = []
        controls = []
        classifier_modes = []
        classifier_controls = []
        if full:
            code = common
            for namespace, values in (('original', before), ('changed', after)):
                functions = '\n'.join(channel.item(values[path], signature) for path, signature in (
                    (HTTP, 'void nsHttpChannel::AsyncOpenFinal('),
                    (HTTP, 'void nsHttpChannel::MaybeResolveProxyAndBeginConnect('),
                    (HTTP, 'nsresult nsHttpChannel::CallOrWaitForResume('),
                    (CHILD, 'HttpChannelChild::AsyncOpen(')))
                functions = functions.replace('HttpChannelChild::AsyncOpen(', 'nsresult HttpChannelChild::AsyncOpen(')
                code += HTTP_ADAPTER.replace('NAMESPACE_BEGIN', 'namespace ' + namespace + ' {').replace('NAMESPACE_END', '}').replace('FUNCTIONS', functions) if namespace == 'original' else HTTP_ADAPTER[HTTP_ADAPTER.index('NAMESPACE_BEGIN'):].replace('NAMESPACE_BEGIN', 'namespace changed {').replace('NAMESPACE_END', '}').replace('FUNCTIONS', functions)
            code += HTTP_MAIN
            modes = ['final-async','final-failure','final-sync','final-skip','final-auth','proxy-ok','proxy-error','proxy-sync','proxy-bypass','proxy-existing','proxy-inactive','proxy-begin-error','cancel','suspend','direct','change-id','child-ok','child-error']
            binary = native('http', code)
            for mode in modes:
                subprocess.run([str(binary), mode], check=True, timeout=15)
            controls = [
                ('classifier-result', code.replace('MAKOS_HTTP_CONT("http-classifier-return", classifierResult,', 'MAKOS_HTTP_CONT("http-classifier-return", NS_OK,'), 'final-failure', 'raw classifier result'),
                ('callback-missing', code.replace('MAKOS_HTTP_CONT("http-classifier-callback", NS_OK, self->mChannelId);', ''), 'final-async', 'classifier callback observed'),
                ('continuation-return', code.replace('return continuationResult;', 'return NS_OK;'), 'direct', 'HTTP policy call order unchanged'),
                ('child-result', code.replace('MAKOS_HTTP_CHILD_CONT("http-child-open-return", rv,', 'MAKOS_HTTP_CHILD_CONT("http-child-open-return", NS_OK,'), 'child-error', 'raw child result'),
                ('errno-restore', code.replace('errno = savedErrno;', '(void)savedErrno;'), 'direct', 'diagnostic errno preserved'),
                ('postcallback-id', code.replace('MAKOS_HTTP_CONT("http-resume-return", continuationResult, makosChannelId);', 'MAKOS_HTTP_CONT("http-resume-return", continuationResult, mChannelId);'), 'change-id', 'precallback channel identity preserved'),
            ]
            for name, mutant, mode, assertion in controls:
                assert mutant != code
                result = subprocess.run([str(native(name, mutant)), mode], capture_output=True, text=True, timeout=15)
                assert result.returncode != 0 and assertion in result.stderr, (name, result.stdout, result.stderr)
            classifier = common
            for namespace, values in (('original', before), ('changed', after)):
                constructor = channel.item(values[CLASS], 'FeatureTask::FeatureTask(')
                complete = channel.item(values[CLASS], 'void FeatureTask::CompleteClassification(')
                check = channel.item(values[CLASS], 'nsresult AsyncUrlChannelClassifier::CheckChannel(')
                dispatch = check[check.index('  nsCOMPtr<nsIRunnable> r = NS_NewRunnableFunction('):-1]
                adapter = CLASS_ADAPTER if namespace == 'original' else CLASS_ADAPTER[CLASS_ADAPTER.index('NAMESPACE_BEGIN'):]
                classifier += adapter.replace('NAMESPACE_BEGIN', 'namespace ' + namespace + ' {').replace('NAMESPACE_END', '}').replace('CONSTRUCTOR', constructor).replace('COMPLETE', complete).replace('DISPATCH\n', dispatch + '\n')
            classifier = classifier.replace('struct Runnable {', 'struct Runnable;using nsIRunnable=Runnable;\nstruct Runnable {')
            classifier += CLASS_MAIN
            classifier_modes = ['success','worker-fail','main-fail','feature-stop']
            binary = native('classifier', classifier)
            for mode in classifier_modes:
                subprocess.run([str(binary), mode], check=True, timeout=15)
            classifier_controls = [
                ('main-result', classifier.replace('MAKOS_CLASS_CONT("classifier-main-dispatch", dispatchResult,', 'MAKOS_CLASS_CONT("classifier-main-dispatch", NS_OK,'), 'main-fail', 'raw main dispatch result'),
                ('worker-result', classifier.replace('MAKOS_CLASS_CONT("classifier-worker-dispatch", rv,', 'MAKOS_CLASS_CONT("classifier-worker-dispatch", NS_OK,'), 'worker-fail', 'raw worker dispatch result'),
                ('capture-errno', classifier.replace('  errno = savedErrno;\n#endif', '  (void)savedErrno;\n#endif'), 'success', 'capture metadata destructor errno preserved'),
                ('worker-live-metadata', classifier.replace('task->MakOSDiagnosticChannelId()', 'task->mChannel->ChannelId()'), 'success', 'metadata queried on main thread only'),
                ('forced-failure-callback', classifier.replace('(void)dispatchResult;  // Preserve the original ignored-result policy.', 'if (NS_FAILED(dispatchResult)) task->CompleteClassification();'), 'main-fail', 'classifier dispatch callback policy unchanged'),
            ]
            for name, mutant, mode, assertion in classifier_controls:
                assert mutant != classifier
                result = subprocess.run([str(native(name, mutant)), mode], capture_output=True, text=True, timeout=15)
                assert result.returncode != 0 and assertion in result.stderr, (name, result.stdout, result.stderr)
        # Same actual shared emitter/macros in full and source-free mode.
        transport = common + r'''
int main(){errno=EBUSY;for(int i=0;i<150;++i)MAKOS_HTTP_CONT("http-parent-open-enter",NS_OK,28);assert(records.empty());arm();for(int i=0;i<150;++i)MAKOS_CLASS_CONT("classifier-main-dispatch",NS_ERROR_FAILURE,28);assert(records.size()==127 && "shared cap unchanged");for(auto& r:records){assert(r.find("result=0x80004005")!=std::string::npos && r.find("channel=0x000000000000001c")!=std::string::npos);assert(r.find("http:")==std::string::npos && r.find("https:")==std::string::npos && "no URL emission");}assert(errno==EBUSY && "diagnostic errno preserved");}
'''
        subprocess.run([str(native('transport', transport))], check=True, timeout=15)
        mutant = transport.replace('prior >= 128', 'prior >= 1280')
        result = subprocess.run([str(native('uncapped', mutant))], capture_output=True, text=True, timeout=15)
        assert result.returncode != 0 and 'shared cap unchanged' in result.stderr
        mutant = transport.replace('append(" scope=diagnostic-only\\n");', 'append(" scope=diagnostic-only https://private.invalid\\n");')
        assert mutant != transport
        result = subprocess.run([str(native('url-emission', mutant))], capture_output=True, text=True, timeout=15)
        assert result.returncode != 0 and 'no URL emission' in result.stderr
        mutated = dict(after)
        mutated[HTTP] = mutated[HTTP].replace('return NS_OK;', 'return NS_ERROR_FAILURE;', 1)
        assert restore(mutated[HTTP], HTTP) != before[HTTP], 'policy mutation detected'
    print('MAKOS_FIREFOX_HTTP_DIAGNOSTICS_HOST_OK', 'native_cases=' + str(len(modes)+len(classifier_modes)+1),
          'negative_controls=' + str(len(controls)+len(classifier_controls)+3),
          'source=' + ('pinned-private-apply' if full else 'patch-extracted-only'),
          'policy=exact-restoration emitter=unchanged-cap,errno,no-url',
          'functions=' + ('final,proxy-or-connect,resume,child-open' if full else 'emitter-only'),
          'classifier=' + ('production-constructor,completion,dispatch-scope' if full else 'structural-only'),
          'parent-open=structural-only guest=not-run browser=not-qualified')


if __name__ == '__main__':
    main()
