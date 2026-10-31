# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
# Diagnostic only. Verify the partition audit rejects missing and duplicate cases.
import hashlib
import json
import pathlib
import subprocess

path = pathlib.Path('crates/ocio/src/apphelpers/mergeconfigs/section_merger_oracle_tests.rs')
original = path.read_bytes()
text = original.decode()
needle = 'index % WHOLE_MERGE_PARTS == part'
assert text.count(needle) == 1
out = pathlib.Path('kept/p29-validation/partition-controls')
out.mkdir(parents=True, exist_ok=True)
target = 'apphelpers::mergeconfigs::section_merger::oracle_tests::whole_merge_parts_cover_original_cases_once'
command = ['cargo', 'test', '--locked', '-j2', '-p', 'ocio', '--lib', target, '--', '--exact', '--nocapture']
rows = []
try:
    for name, predicate in (
        ('missing-input', f'({needle}) && index != 0'),
        ('duplicated-input', f'({needle}) || (index == 0 && part == 1)'),
    ):
        path.write_text(text.replace(needle, predicate))
        result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        (out / f'{name}.log').write_text(result.stdout)
        witness = 'assertion failed: visits.iter().all(|&visited| visited == 1)'
        rejected = result.returncode == 101 and witness in result.stdout and 'test result: FAILED. 0 passed; 1 failed; 0 ignored;' in result.stdout
        rows.append(dict(Name=name, Rejected=rejected, ExitCode=result.returncode, RuntimeWitness=witness))
        assert rejected, result.stdout[-4000:]
finally:
    path.write_bytes(original)
assert path.read_bytes() == original
result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
(out / 'restored-baseline.log').write_text(result.stdout)
assert result.returncode == 0 and 'test result: ok. 1 passed; 0 failed; 0 ignored;' in result.stdout, result.stdout[-4000:]
(out / 'report.json').write_text(json.dumps(dict(Controls=rows, RestoredBaselinePassed=True, RestoredSourceSHA256=hashlib.sha256(original).hexdigest()), indent=2) + '\n')
print('Both compiled partition mutants rejected at runtime; original source restored and audit passes.', flush=True)
