# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Adversarial missing-only receipt controls; no OCIO or emulator execution."""
import copy
import contextlib
import hashlib
import io
import json
import pathlib
import shutil
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import p77_profile_controls as fixtures
from sde_cpu_tests import selected_shard, write_json
from sde_join_shards import target_key
from sde_resume import ORIGINAL_HEAD, ORIGINAL_RUN, baseline, digest, finish, plan, read, read_plan


class ResumeControls(unittest.TestCase):
    def setUp(self):
        fixture = fixtures.ProfileControls()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        self.root = fixture.directory
        self.base = self.root / 'baseline'
        self.base.mkdir()
        write_json(self.base / 'run.json', dict(id=ORIGINAL_RUN, head_sha=ORIGINAL_HEAD,
            repository=dict(full_name='Pixel-Minions/OpenColorIO-rs')))
        # Extra names are protocol inputs, not synthesized OCIO expected values.
        for source in [fixture.kernel, fixture.lut]:
            for path in source.glob('**/ocio/report.json'):
                report = read(path)
                report['Unfiltered'].extend(f'protocol_input_{i}' for i in range(16))
                if path.parent.parent == fixture.kernel:
                    for name in report['Unfiltered'][1:]:
                        raw = f'running 1 test\ntest {name} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 1s\n'.encode()
                        log = name + '.log'
                        (path.parent / log).write_bytes(raw)
                        report['Selected'].append(name)
                        report['Results'].append(dict(Name=name, Log=log,
                            LogSHA256=hashlib.sha256(raw).hexdigest(), Passed=True,
                            ExitCode=0, EmulatorError=False, TimedOut=False, ElapsedSeconds=1))
                for row in report['Results']:
                    log = path.parent / row['Log']
                    raw = log.read_bytes().replace(b'0 filtered out;', b'16 filtered out;')
                    log.write_bytes(raw)
                    row['LogSHA256'] = hashlib.sha256(raw).hexdigest()
                report['ExcludedByOriginalArgs'] = sorted(set(report['Unfiltered']) - set(report['Selected']))
                write_json(path, report)
            for path in source.rglob('report.json'):
                report = read(path)
                for row in report['Results']:
                    row['ElapsedSeconds'] = 1
                write_json(path, report)
            summary = read(source / 'summary.json')
            summary['Head'] = ORIGINAL_HEAD
            write_json(source / 'summary.json', summary)
            fixture.totals(source)
        rocky = self.base / 'sde-snb-Rocky-complete-profile'
        shutil.copytree(fixture.kernel, rocky / 'sde-cpu-tests-joined/profile')
        shutil.copytree(fixture.lut, rocky / 'sde-lut-input/run-original')
        shutil.copytree(fixture.lut, self.base / 'sde-snb-Windows-lut/run-original')
        self.expected = read(fixture.kernel / 'expected-executables.json')
        self.originals = {}
        for index in range(4):
            destination = self.base / f'sde-snb-Windows-kernel-shard-{index}' / 'run-original'
            shutil.copytree(fixture.kernel, destination)
            total = original_total = 0
            exclusions = []
            for path in destination.glob('*/report.json'):
                report = read(path)
                original = report['Selected']
                key = target_key(self.expected[report['Executable']])
                report.update(OriginalSelected=original, LogicalTarget=key,
                    WorkingDirectory='/work/crates/ocio', CargoHarnessArgs=[],
                    Shard=dict(Index=index, Count=4))
                report['Selected'] = selected_shard(self.expected[report['Executable']], original, report['Shard'])
                report['Results'] = [r for r in report['Results'] if r['Name'] in report['Selected']]
                total += len(report['Selected'])
                original_total += len(original)
                exclusions.extend(report['ExcludedByOriginalArgs'])
                if index == 0:
                    self.originals[key] = (fixture.kernel / path.parent.name, copy.deepcopy(report))
                if index == 1:
                    report['Results'] = report['Results'][:1]
                    report['AllSelectedPassed'] = len(report['Results']) == len(report['Selected'])
                write_json(path, report)
            summary = read(destination / 'summary.json')
            summary.update(Shard=dict(Index=index, Count=4), RequiresShardAggregation=True,
                SelectedTests=total, PassedTests=total, OriginalSelectedTests=original_total,
                ExcludedByOriginalArgs=exclusions)
            summary.pop('AggregatedShards', None)
            if index == 1:
                (destination / 'summary.json').unlink()
            else:
                write_json(destination / 'summary.json', summary)
        self.value = plan(self.base)
        self.plan_path = self.root / 'plan.json'
        write_json(self.plan_path, self.value)
        self.parts = []
        for index, cases in enumerate(self.value['Parts']):
            folder = self.root / f'resume-{index}'
            folder.mkdir()
            self.parts.append(folder)
            shutil.copyfile(fixture.kernel / 'compiler-artifacts.jsonl', folder / 'compiler-artifacts.jsonl')
            write_json(folder / 'native-inventory.json',
                {key: target['Unfiltered'] for key, target in self.value['Targets'].items()})
            rows = []
            for position, case in enumerate(cases):
                source, report = self.originals[case['Target']]
                original = next(r for r in read(source / 'report.json')['Results'] if r['Name'] == case['Name'])
                log = f'case-{position}.log'
                shutil.copyfile(source / original['Log'], folder / log)
                rows.append(dict(original, Target=case['Target'], Log=log))
            write_json(folder / 'receipt.json', dict(Part=index, Parts=4, SourceRun=ORIGINAL_RUN,
                Head=ORIGINAL_HEAD, CPU='snb', PlanSHA256=self.value['PlanSHA256'],
                SDEPrefix=self.value['SDEPrefix'], Planned=cases, Results=rows,
                AllSelectedPassed=True, CompleteCPUProfile=False))

    def join(self):
        with contextlib.redirect_stdout(io.StringIO()):
            finish(self.base, self.plan_path, self.parts, self.root / 'joined')

    def mutate_receipt(self, change):
        folder = next(p for p in self.parts if read(p / 'receipt.json')['Results'])
        path = folder / 'receipt.json'
        value = read(path)
        change(value)
        write_json(path, value)

    def test_original_union_reconstructed_with_only_missing_cases(self):
        self.assertGreater(len(self.value['Missing']), 1)
        self.join()
        proof = read(self.root / 'joined/windows-proof.json')
        self.assertTrue(proof['CompleteCPUProfile'])
        self.assertFalse(proof['ReleaseQualification'])
        self.assertEqual(proof['ExecutedMissingCases'], len(self.value['Missing']))
        self.assertEqual(proof['BoundaryInputUnion']['OriginalRequests'], 1400)
        self.assertTrue(proof['BoundaryInputUnion']['ExactlyOnce'])
        self.assertEqual(read(self.root / 'joined/rocky-proof.json')['ExecutedMissingCases'], 0)

    def test_missing_partition_rejected(self):
        self.parts.pop()
        with self.assertRaises(RuntimeError): self.join()

    def test_duplicate_partition_rejected(self):
        self.parts[-1] = self.parts[0]
        with self.assertRaises(RuntimeError): self.join()

    def test_missing_result_rejected(self):
        self.mutate_receipt(lambda r: r['Results'].pop())
        with self.assertRaises(RuntimeError): self.join()

    def test_failed_result_rejected_even_if_summary_says_pass(self):
        self.mutate_receipt(lambda r: r['Results'][0].update(Passed=False))
        with self.assertRaises(RuntimeError): self.join()

    def test_changed_source_rejected(self):
        self.mutate_receipt(lambda r: r.update(Head='f' * 40))
        with self.assertRaises(RuntimeError): self.join()

    def test_emulator_error_rejected(self):
        self.mutate_receipt(lambda r: r['Results'][0].update(EmulatorError=True))
        with self.assertRaises(RuntimeError): self.join()

    def test_changed_native_manifest_rejected(self):
        write_json(self.parts[0] / 'native-inventory.json', {})
        with self.assertRaises(RuntimeError): self.join()

    def test_rehashed_duplicate_plan_rejected(self):
        value = copy.deepcopy(self.value)
        value['Missing'].append(value['Missing'][0])
        value['Parts'][0].append(value['Missing'][0])
        value.pop('PlanSHA256')
        value['PlanSHA256'] = digest(value)
        write_json(self.plan_path, value)
        with self.assertRaises(RuntimeError): read_plan(self.plan_path)

    def test_retained_pass_cannot_be_selected_again(self):
        _, _, _, retained, _, _ = baseline(self.base)
        key, name = next(iter(retained))
        value = copy.deepcopy(self.value)
        value['Missing'].append(dict(Target=key, Name=name))
        value['Parts'][0].append(dict(Target=key, Name=name))
        value.pop('PlanSHA256')
        value['PlanSHA256'] = digest(value)
        write_json(self.plan_path, value)
        with self.assertRaises(RuntimeError): self.join()

    def test_corrupt_actual_log_rejected(self):
        folder = next(p for p in self.parts if read(p / 'receipt.json')['Results'])
        row = read(folder / 'receipt.json')['Results'][0]
        (folder / row['Log']).write_text('not a libtest pass')
        with self.assertRaises(RuntimeError): self.join()


if __name__ == '__main__':
    unittest.main()
