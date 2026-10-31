# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Real runner rejection controls, using the tiny existing Rust plan audit under SDE."""
import hashlib
import json
import pathlib
import subprocess
import sys

source = pathlib.Path('scripts/sde_cpu_tests.py')
original = source.read_bytes()
text = original.decode()
out = pathlib.Path('kept/p30-controls')
out.mkdir(parents=True, exist_ok=True)
scratch = pathlib.Path('target/p30-runner-controls')
scratch.mkdir(parents=True, exist_ok=True)
base = pathlib.Path('kept/sde-cpu-tests')
audit = 'apphelpers::mergeconfigs::section_merger::oracle_tests::whole_merge_parts_cover_original_cases_once'
arguments = ['cargo', 'cpu-tests', '--release', '--no-fail-fast', '--', audit, '--exact']
loop = 'for index, name in enumerate(selected):'
report_list = "reports = [json.loads(path.read_text()) for path in root.glob('*/report.json')]"
assert text.count(loop) == text.count(report_list) == 1
mutations = (
    ('missing-case', loop, 'for index, name in enumerate(selected[:-1]):'),
    ('duplicate-case', loop, 'for index, name in enumerate(selected + selected[-1:]):'),
    ('missing-target-report', report_list, report_list + '\n    reports = reports[1:]'),
    ('duplicate-target-report', report_list, report_list + '\n    reports = reports + reports[:1]'),
)
rows = []
for name, needle, replacement in mutations:
    path = (scratch / f'{name}.py').resolve()
    path.write_text(text.replace(needle, replacement), encoding='utf-8')
    before = set(base.glob('run-*'))
    result = subprocess.run([sys.executable, str(path), *arguments], stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, encoding='utf-8', errors='replace', timeout=600)
    (out / f'{name}.log').write_text(result.stdout, encoding='utf-8')
    created = set(base.glob('run-*')) - before
    assert len(created) == 1, created
    directory = created.pop()
    if name in ('missing-case', 'duplicate-case'):
        reports = [json.loads(p.read_text()) for p in (directory / 'preflight').glob('*/report.json')]
        controls = [report for report in reports if report['Selected']]
        assert len(controls) == 1 and controls[0]['Selected'] == [audit]
        control = controls[0]
        assert not control['ExactlyOnce'] and not control['AllSelectedPassed']
        expected_count = 0 if name == 'missing-case' else 2
        assert len(control['Results']) == expected_count
        assert all(row['Passed'] and row['ExitCode'] == 0 for row in control['Results'])
        witness = 'Cargo runner preflight failed; do not start the full suite'
        assert witness in result.stdout
        detail = dict(Selected=1, Observed=expected_count, ExactlyOnce=False, Witness=witness)
    else:
        summary = json.loads((directory / 'summary.json').read_text())
        assert summary['CargoExit'] == 0 and not summary['CompleteExecutableManifest']
        assert not summary['AllSelectedTestsPassed']
        difference = -1 if name == 'missing-target-report' else 1
        assert summary['ObservedExecutables'] == summary['ExpectedExecutables'] + difference
        detail = summary
    assert result.returncode != 0
    rows.append(dict(Name=name, Rejected=True, ExitCode=result.returncode,
                     ReportDirectory=str(directory), Evidence=detail))
    assert source.read_bytes() == original
    print(f'PASS rejection control {name}', flush=True)
report = dict(Controls=rows, OriginalSourceUnchanged=True,
              OriginalSourceSHA256=hashlib.sha256(original).hexdigest(),
              ProtocolControlsOnly=True, FullCPUProfileCoveragePassed=False)
(out / 'report.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
print('All four runner omissions/duplicates rejected; production source unchanged.', flush=True)
