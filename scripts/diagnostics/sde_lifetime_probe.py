# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
# Diagnostic only; no production flags, coverage or expected color output changes.
import json,os,pathlib,re,shlex,signal,subprocess,sys,threading,time
out=pathlib.Path('kept/sde-lifetime-probe');out.mkdir(parents=True,exist_ok=True)
windows=os.name=='nt'
var='CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER' if windows else 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER'
runner=shlex.split(os.environ[var]);assert runner[-1]=='--'
binary=pathlib.Path('target/thread-lifetime.exe' if windows else 'target/thread-lifetime').resolve()
binary.parent.mkdir(parents=True,exist_ok=True)
subprocess.run(['rustc','-O','scripts/diagnostics/sde_lifetime_threads.rs','-o',str(binary)],check=True)
kit=pathlib.Path(runner[0]).parent
pin=kit/('pin.exe' if windows else 'pin')
if pin.is_file():
    with (out/'pinned-pin-help.txt').open('w',encoding='utf-8') as log:
        subprocess.run([str(pin),'-help'],stdout=log,stderr=subprocess.STDOUT,timeout=30,check=False)

def probe(name,prefix,count):
    started=time.monotonic();error=threading.Event();timed_out=False
    path=out/(name+'.log')
    process=subprocess.Popen(prefix+[str(binary),str(count)],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,encoding='utf-8',errors='replace',start_new_session=not windows)
    def consume():
        with path.open('w',encoding='utf-8') as log:
            for line in process.stdout:
                log.write(line);log.flush()
                if 'SDE ERROR' in line or 'maxmimal number of threads' in line:error.set()
    reader=threading.Thread(target=consume,daemon=True);reader.start()
    while process.poll() is None:
        if error.is_set() or time.monotonic()-started>180:
            timed_out=not error.is_set()
            if windows:subprocess.run(['taskkill','/PID',str(process.pid),'/T','/F'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=False)
            else:
                try:os.killpg(process.pid,signal.SIGKILL)
                except ProcessLookupError:pass
            break
        time.sleep(.1)
    code=process.wait(timeout=30);reader.join(timeout=10)
    text=path.read_text(errors='replace')
    completed=[int(n) for n in re.findall(r'completed=(\d+)',text)]
    return dict(Count=count,ProcessExit=code,EmulatorError=error.is_set(),TimedOut=timed_out,Completed=(f'COMPLETE count={count} sequential-workers=1' in text),LastCompletedCheckpoint=max(completed,default=0),ElapsedSeconds=round(time.monotonic()-started,2))

native=probe('native-70000',[],70000)
assert native['Completed'] and native['ProcessExit']==0 and not native['TimedOut']
rows=[]
for count in (8192,20000,70000):
    row=probe(f'emulated-{count}',runner,count);rows.append(row)
    report=dict(DiagnosticOnly=True,Native=native,Emulated=rows,MaxConcurrentWorkers=1,CumulativeLimitReproduced=any(r['EmulatorError'] for r in rows),ProductionSDECoverageUnchanged=True)
    (out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    assert row['EmulatorError'] or (row['Completed'] and row['ProcessExit']==0 and not row['TimedOut']),row
    if row['EmulatorError']:break
print(json.dumps(report,indent=2),flush=True)
