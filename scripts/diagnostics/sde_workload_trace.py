# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
# Diagnostic only; NEVER MERGE. Same complete ocio library workload, owned processes only.
import json,os,pathlib,queue,re,signal,subprocess,sys,threading,time
threads=int(sys.argv[1]); assert threads in (1,4)
out=pathlib.Path('kept/sde-workload-trace');out.mkdir(parents=True,exist_ok=True)
command=['cargo','test','--locked','--release','-p','ocio','--lib','--','--nocapture',f'--test-threads={threads}']
env=os.environ.copy();env['RUST_TEST_THREADS']=str(threads)
process=subprocess.Popen(command,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,encoding='utf-8',errors='replace',start_new_session=os.name!='nt',bufsize=1)
started=time.monotonic();lines=queue.Queue();tail=[];observed=False;timed_out=False

def consume():
    for line in process.stdout: lines.put(line)
threading.Thread(target=consume,daemon=True).start()

def snapshot():
    if os.name=='nt':
        script=r'''
$sdeRootPid=[int]$env:SDE_TRACE_ROOT_PID
$sdeRows=Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name
$sdeOwned=[Collections.Generic.HashSet[int]]::new();[void]$sdeOwned.Add($sdeRootPid)
do { $sdeAdded=$false;foreach($sdeRow in $sdeRows){if($sdeOwned.Contains([int]$sdeRow.ParentProcessId)){if($sdeOwned.Add([int]$sdeRow.ProcessId)){$sdeAdded=$true}}} } while($sdeAdded)
$sdeResult=@(foreach($sdeRow in $sdeRows){if($sdeOwned.Contains([int]$sdeRow.ProcessId)){$sdeInfo=Get-Process -Id $sdeRow.ProcessId -ErrorAction SilentlyContinue;[PSCustomObject]@{PID=[int]$sdeRow.ProcessId;PPID=[int]$sdeRow.ParentProcessId;Name=$sdeRow.Name;Threads=if($sdeInfo){$sdeInfo.Threads.Count}else{$null};RSS=if($sdeInfo){$sdeInfo.WorkingSet64}else{$null}}}})
ConvertTo-Json -InputObject $sdeResult -Compress
'''
        snap_env=os.environ.copy();snap_env['SDE_TRACE_ROOT_PID']=str(process.pid)
        try:return json.loads(subprocess.check_output(['powershell.exe','-NoProfile','-NonInteractive','-Command',script],env=snap_env,text=True,timeout=15))
        except Exception as error:return [dict(SnapshotError=str(error))]
    rows=[]
    for path in pathlib.Path('/proc').glob('[0-9]*/status'):
        try:
            fields={line.split(':',1)[0]:line.split(':',1)[1].strip() for line in path.read_text().splitlines() if ':' in line}
            rows.append(dict(PID=int(path.parent.name),PPID=int(fields['PPid']),Name=fields['Name'],Threads=int(fields['Threads']),RSS=int(fields.get('VmRSS','0 kB').split()[0])*1024))
        except (FileNotFoundError,ProcessLookupError,PermissionError,KeyError):pass
    owned={process.pid}
    while True:
        expanded=owned|{r['PID'] for r in rows if r['PPID'] in owned}
        if expanded==owned:break
        owned=expanded
    return [r for r in rows if r['PID'] in owned]

def kill_owned():
    if os.name=='nt':subprocess.run(['taskkill','/PID',str(process.pid),'/T','/F'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=False)
    else:
        try:os.killpg(process.pid,signal.SIGKILL)
        except ProcessLookupError:pass

snapshots=[];next_snapshot=0;all_text=[]
try:
    with (out/'workload.log').open('w',encoding='utf-8') as log:
        while process.poll() is None or not lines.empty():
            now=time.monotonic()-started
            if now>=next_snapshot:
                snapshots.append(dict(ElapsedSeconds=round(now,2),Processes=snapshot()))
                (out/'process-thread-snapshots.json').write_text(json.dumps(snapshots,indent=2)+'\n')
                next_snapshot=now+20
            try:line=lines.get(timeout=0.5)
            except queue.Empty:line=''
            if line:
                all_text.append(line);log.write(line);log.flush();print(line,end='',flush=True)
                tail=(tail+[line.rstrip()])[-80:]
                if 'maxmimal number of threads' in line or 'SDE ERROR' in line:
                    observed=True
                    snapshots.append(dict(ElapsedSeconds=round(time.monotonic()-started,2),AtEmulatorError=True,Processes=snapshot()))
                    kill_owned();break
            if now>1200:
                timed_out=True;kill_owned();break
    code=process.wait(timeout=30)
finally:
    if process.poll() is None:kill_owned()
text=''.join(all_text)
full_pass=('test result: ok. 1280 passed; 0 failed; 0 ignored' in text) and code==0 and not observed and not timed_out
report=dict(DiagnosticOnly=True,Command=command,TestThreads=threads,RootPID=process.pid,ElapsedSeconds=round(time.monotonic()-started,2),ProcessExitCode=code,EmulatorErrorObserved=observed,TimedOut=timed_out,Full1280LibraryTestsPassed=full_pass,OriginalProductionSDECoverageUnchanged=True,LastOutput=tail)
(out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
(out/'process-thread-snapshots.json').write_text(json.dumps(snapshots,indent=2)+'\n')
print(json.dumps(report,indent=2),flush=True)
sys.exit(0 if full_pass else 86 if observed else 124 if timed_out else code or 1)
