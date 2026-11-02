# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Native rejection controls for complete-profile evidence, not OCIO output goldens."""
import copy
import hashlib
import json
import pathlib
import shutil
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from sde_complete_profile import AUDIT, LUT, STEM, complete
from p76_boundary_controls import BoundaryControls


def write(path, value):
    path.write_text(json.dumps(value), encoding='utf-8')


class ProfileControls(unittest.TestCase):
    def setUp(self):
        boundary = BoundaryControls()
        boundary.setUp()
        self.addCleanup(boundary.doCleanups)
        self.directory = boundary.directory
        self.head = 'a' * 40
        self.kernel = self.directory / 'kernel'
        self.lut = self.directory / 'lut'
        plans = [('legacy_gpu_oracle', [r['Name'] for r in boundary.rows], ['test']),
                 ('api_formats_oracle', ['ordinary', *LUT], ['test']),
                 ('ocio', [AUDIT], ['lib'])]
        expected = {f'/work/target/release/{name}': dict(Package='file:///work/crates/ocio#0',
                    Target=dict(name=name, kind=kind)) for name, _, kind in plans}
        artifacts = [dict(reason='compiler-artifact', profile=dict(test=True), executable=exe,
                    package_id=metadata['Package'], target=metadata['Target']) for exe, metadata in expected.items()]
        for root, lut in [(self.kernel, False), (self.lut, True)]:
            root.mkdir()
            write(root / 'expected-executables.json', expected)
            (root / 'compiler-artifacts.jsonl').write_text('\n'.join(json.dumps(a) for a in artifacts), encoding='utf-8')
            preflight = root / 'preflight'
            preflight.mkdir()
            for name, names, _ in plans:
                selected = [n for n in names if (n in LUT) == lut]
                for parent, selected_names in [(root, selected), (preflight, [n for n in names if n == AUDIT])]:
                    directory = parent / name
                    directory.mkdir()
                    rows = []
                    for index, case in enumerate(selected_names):
                        log = f'test-{index}.log'
                        if name == 'legacy_gpu_oracle':
                            original = next(r for r in boundary.rows if r['Name'] == case)
                            raw = (boundary.directory / original['Log']).read_bytes()
                        else:
                            raw = (f'running 1 test\ntest {case} ... ok\n'
                                   f'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; {len(names)-1} filtered out; finished in 1s\n').encode()
                        (directory / log).write_bytes(raw)
                        rows.append(dict(Name=case, Log=log, LogSHA256=hashlib.sha256(raw).hexdigest(),
                                         Passed=True, ExitCode=0, EmulatorError=False, TimedOut=False))
                    write(directory / 'report.json', dict(Executable=f'/work/target/release/{name}',
                        Unfiltered=names, Selected=selected_names, Results=rows, AllSelectedPassed=True,
                        ExactlyOnce=True, ExcludedByOriginalArgs=sorted(set(names)-set(selected_names))))
            summary = dict(Head=self.head, CargoCommand=STEM + (LUT if lut else ['--skip', LUT[0], '--skip', LUT[1]]),
                SDEPrefix=['sde', '-snb', '--'], AllSelectedTestsPassed=True, CargoExit=0,
                CompleteExecutableManifest=True, ExpectedExecutables=3, ObservedExecutables=3,
                AggregatedShards=4 if not lut else None)
            write(root / 'summary.json', summary)
            self.totals(root)

    def totals(self, root):
        summary = json.loads((root / 'summary.json').read_text())
        total = sum(len(json.loads(p.read_text())['Selected']) for p in root.glob('*/report.json'))
        summary.update(SelectedTests=total, PassedTests=total)
        write(root / 'summary.json', summary)

    def proof(self):
        return complete(self.kernel, self.lut, 'snb', self.head)

    def test_complete_alias_includes_both_luts_and_boundary(self):
        proof = self.proof()
        self.assertTrue(proof['CompleteCPUProfile'])
        self.assertEqual(proof['NativeTests'], 12)
        self.assertTrue(proof['BoundaryInputUnion']['ExactlyOnce'])

    def test_missing_lut_cannot_be_a_complete_profile(self):
        path = self.lut / 'api_formats_oracle/report.json'
        report = json.loads(path.read_text())
        report['Selected'].pop()
        report['Results'].pop()
        report['ExcludedByOriginalArgs'] = sorted(set(report['Unfiltered']) - set(report['Selected']))
        write(path, report)
        self.totals(self.lut)
        with self.assertRaises(RuntimeError):
            self.proof()

    def test_duplicate_between_kernel_and_lut_is_rejected(self):
        path = self.lut / 'api_formats_oracle/report.json'
        report = json.loads(path.read_text())
        kernel = json.loads((self.kernel / 'api_formats_oracle/report.json').read_text())
        row = copy.deepcopy(kernel['Results'][0])
        shutil.copyfile(self.kernel / 'api_formats_oracle' / row['Log'], self.lut / 'api_formats_oracle/duplicate.log')
        row['Log'] = 'duplicate.log'
        report['Selected'].append(row['Name'])
        report['Results'].append(row)
        report['ExcludedByOriginalArgs'] = []
        write(path, report)
        self.totals(self.lut)
        with self.assertRaises(RuntimeError):
            self.proof()

    def test_source_or_cpu_mismatch_is_rejected(self):
        path = self.lut / 'summary.json'
        original = json.loads(path.read_text())
        for mutation in [dict(Head='b' * 40), dict(SDEPrefix=['sde', '-nhm', '--'])]:
            write(path, dict(original, **mutation))
            with self.assertRaises(RuntimeError):
                self.proof()

    def test_lut_raw_hash_mismatch_is_rejected(self):
        path = self.lut / 'api_formats_oracle/report.json'
        report = json.loads(path.read_text())
        report['Results'][0]['LogSHA256'] = '0' * 64
        write(path, report)
        with self.assertRaises(RuntimeError):
            self.proof()

    def test_incomplete_native_inventory_is_rejected(self):
        (self.lut / 'ocio/report.json').unlink()
        with self.assertRaises(RuntimeError):
            self.proof()

    def test_missing_preflight_is_rejected(self):
        path = self.lut / 'preflight/ocio/report.json'
        report = json.loads(path.read_text())
        report.update(Selected=[], Results=[])
        write(path, report)
        with self.assertRaises(RuntimeError):
            self.proof()

    def test_changed_original_filters_are_rejected(self):
        path = self.kernel / 'summary.json'
        summary = json.loads(path.read_text())
        summary['CargoCommand'].append('another-filter')
        write(path, summary)
        with self.assertRaises(RuntimeError):
            self.proof()


if __name__ == '__main__':
    unittest.main()
