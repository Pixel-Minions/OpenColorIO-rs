# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
# Diagnostic only. Run every whole-merge part, plan audit and general/roles control in SDE.
import hashlib
import json
import os
import pathlib
import re
import shlex
import signal
import subprocess
import threading
import time

out = pathlib.Path('kept/p29-sde')
out.mkdir(parents=True, exist_ok=True)
windows = os.name == 'nt'
variable = 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER' if windows else 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER'
runner = shlex.split(os.environ[variable])
assert runner[-1] == '--'
artifacts = [json.loads(line) for line in (out / 'compiler-artifacts.jsonl').read_text().splitlines()]
binaries = [item['executable'] for item in artifacts if item.get('reason') == 'compiler-artifact' and item['target']['name'] == 'ocio' and item['target']['kind'] == ['lib'] and item['profile']['test'] and item.get('executable')]
assert len(binaries) == 1
binary = pathlib.Path(binaries[0]).resolve()
listed = subprocess.run([str(binary), '--list', '--format', 'terse'], check=True, capture_output=True, text=True, timeout=120)
(out / 'native-test-list.txt').write_text(listed.stdout)
lines = [line for line in listed.stdout.splitlines() if line]
assert all(line.endswith(': test') for line in lines)
names = [line.removesuffix(': test') for line in lines]
assert len(names) == len(set(names))
prefix = 'apphelpers::mergeconfigs::section_merger::oracle_tests::'
plan = [prefix + 'whole_merge_parts_cover_original_cases_once']
plan += [prefix + f'whole_merges_part_{part}' for part in range(4)]
plan += [prefix + 'general_and_roles_match_the_wheel']
assert set(plan) == {name for name in names if name.startswith(prefix)}
report = dict(DiagnosticOnly=True, NativeTestCount=len(names), NativeManifestSHA256=hashlib.sha256(listed.stdout.encode()).hexdigest(), Selected=plan, Results=[], AllSixSelectedTestsPassed=False, FullProductionCoveragePassed=False, Runner=runner)


def save():
    (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')


def run_case(index, name):
    started = time.monotonic()
    error = threading.Event()
    timed_out = False
    path = out / f'test-{index}.log'
    process = subprocess.Popen(runner + [str(binary), '--exact', name, '--nocapture', '--test-threads=1'], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding='utf-8', errors='replace', start_new_session=not windows)

    def consume():
        with path.open('w', encoding='utf-8') as log:
            for line in process.stdout:
                log.write(line)
                log.flush()
                if 'SDE ERROR' in line or 'maxmimal number of threads' in line:
                    error.set()

    def kill_owned():
        if windows:
            subprocess.run(['taskkill', '/PID', str(process.pid), '/T', '/F'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
        else:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

    reader = threading.Thread(target=consume, daemon=True)
    reader.start()
    try:
        while process.poll() is None or reader.is_alive():
            if error.is_set() or time.monotonic() - started > 1200:
                timed_out = not error.is_set()
                kill_owned()
                break
            time.sleep(.1)
        code = process.wait(timeout=30)
        reader.join(timeout=10)
    finally:
        if process.poll() is None:
            kill_owned()
    text = path.read_text(errors='replace')
    summary = rf'^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; {len(names)-1} filtered out;'
    passed = code == 0 and not error.is_set() and not timed_out and not reader.is_alive() and 'running 1 test' in text and f'test {name} ...' in text and re.search(summary, text, re.M) is not None
    rows = re.findall(r'whole-merge part (\d+)/(\d+): checked (\d+) of (\d+) original cases', text)
    return dict(Name=name, Passed=passed, ExitCode=code, EmulatorError=error.is_set(), TimedOut=timed_out, ElapsedSeconds=round(time.monotonic()-started, 2), Counts=[[int(value) for value in row] for row in rows], Log=path.name, LogSHA256=hashlib.sha256(path.read_bytes()).hexdigest())


save()
for index, name in enumerate(plan):
    print(f'START {index+1}/{len(plan)} {name}', flush=True)
    result = run_case(index, name)
    report['Results'].append(result)
    save()
    print(json.dumps(result), flush=True)
    if not result['Passed']:
        raise SystemExit(86 if result['EmulatorError'] else 124 if result['TimedOut'] else 1)
counts = [row['Counts'][0] for row in report['Results'][1:5]]
assert all(len(row['Counts']) == 1 for row in report['Results'][1:5])
assert [row[0] for row in counts] == list(range(4))
assert all(row[1] == 4 and row[3] == counts[0][3] for row in counts)
assert sum(row[2] for row in counts) == counts[0][3]
report['AllSixSelectedTestsPassed'] = True
report['AllOriginalWholeMergeCasesComparedOnce'] = True
save()
print('PASS complete original whole-merge case plan and all selected tests; not full production coverage.', flush=True)
