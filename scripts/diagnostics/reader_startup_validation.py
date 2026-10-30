# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
# Diagnostic CI driver only. NEVER MERGE this file or its workflow.
import json,os,pathlib,subprocess,sys
out=pathlib.Path('kept/reader-validation')
out.mkdir(parents=True,exist_ok=True)
target=pathlib.Path(os.environ.get('CARGO_TARGET_DIR','target')).resolve()
venv=target/'py-port'
clean=os.environ.copy()
clean.pop('VIRTUAL_ENV',None)
clean.pop('PYTHONHOME',None)
clean['UV_PROJECT_ENVIRONMENT']=str(venv)
subprocess.run(['uv','sync','--project','py-port','--frozen','--quiet','--no-install-project'],env=clean,check=True)
python=venv/('Scripts/python.exe' if os.name=='nt' else 'bin/python')
home=subprocess.check_output([str(python),'-c','import sys; print(sys.base_prefix)'],env=clean,text=True).strip()
commands=clean.copy()
commands.update(PYO3_PYTHON=str(python),PYTHONHOME=home)
if os.name=='nt':
    commands['PATH']=home+os.pathsep+commands.get('PATH','')
else:
    commands['LD_LIBRARY_PATH']=str(pathlib.Path(home)/'lib')+os.pathsep+commands.get('LD_LIBRARY_PATH','')
clippy=['cargo','clippy','--locked','-j2','-p','ocio-py','--features','python','--test','gpu_shader_desc_oracle','--','-D','warnings']
with (out/'clippy.log').open('w',encoding='utf-8') as log:
    result=subprocess.run(clippy,env=commands,stdout=log,stderr=subprocess.STDOUT)
assert result.returncode==0,(out/'clippy.log').read_text(errors='replace')[-16000:]
rows=[]
for repeat in range(1,6):
    args=['cargo','test','--locked','-j2','-p','ocio-py','--features','python','--test','gpu_shader_desc_oracle']
    if os.environ['PROFILE']=='release': args+=['--release']
    args+=['textures_read_during_an_extraction_are_the_wheels','--','--exact','--test-threads=1','--show-output']
    path=out/f'repeat-{repeat}.log'
    with path.open('w',encoding='utf-8') as log:
        result=subprocess.run(args,env=commands,stdout=log,stderr=subprocess.STDOUT)
    text=path.read_text(errors='replace')
    print(text[-12000:],flush=True)
    assert result.returncode==0
    assert 'test textures_read_during_an_extraction_are_the_wheels ... ok' in text
    assert 'test result: ok. 1 passed; 0 failed; 0 ignored' in text
    rows.append(dict(Repeat=repeat,Passed=True,ReaderRounds=200,ReadersPerRound=5,ExtractionsPerRound=10))
    (out/'run-summary.json').write_text(json.dumps(dict(Profile=os.environ['PROFILE'],Platform=sys.platform,ClippyPassed=True,RepeatedRuns=rows),indent=2)+'\n')
