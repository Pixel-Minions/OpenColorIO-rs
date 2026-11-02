# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Native watchdog and selection controls; launches no SDE, oracle or OCIO process."""
import json
import os
import pathlib
import sys
import tempfile
import time
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from sde_cpu_tests import (JOB_BUDGET_KEY, JOB_DEADLINE_KEY, PREFIX_KEY, REPORT_KEY,
                           SHARD_KEY, executable_key, job_budget, one_test, test_binary)
from sde_targeted import PACKAGE_DIRECTORY, selection, source_head
from p76_boundary_controls import BoundaryControls
from p77_profile_controls import ProfileControls
from p97_resume_controls import ResumeControls


class TargetedControls(unittest.TestCase):
    def test_source_head_trust_is_process_scoped_and_specific(self):
        root = pathlib.Path(__file__).resolve().parents[2]
        with mock.patch('sde_targeted.subprocess.check_output', return_value='f' * 40 + '\n') as read:
            self.assertEqual(source_head(root), 'f' * 40)
        read.assert_called_once_with([
            'git', '-c', 'safe.directory=' + root.as_posix(), '-C', str(root),
            'rev-parse', '--verify', 'HEAD',
        ], text=True)
        with mock.patch('sde_targeted.subprocess.check_output', return_value='HEAD\n'):
            with self.assertRaises(RuntimeError):
                source_head(root)

    def test_actual_mounted_checkout_head_is_readable(self):
        # Run this inside Rocky before the ordinary gate sets its Git trust config:
        # the former bare rev-parse fails here for the host-owned /work checkout.
        self.assertRegex(source_head(pathlib.Path(__file__).resolve().parents[2]), r'^[0-9a-f]{40}$')

    def case(self, source, budget):
        with tempfile.TemporaryDirectory() as folder:
            directory = pathlib.Path(folder)
            script = directory / 'child.py'
            script.write_text(source, encoding='utf-8')
            return one_test([sys.executable], str(script), 'probe', 1,
                            directory / 'output.log', budget)

    def test_thread_limit_is_terminated_and_cannot_pass(self):
        started = time.monotonic()
        row = self.case("import time\nprint('SDE ERROR: Run exceeds the maxmimal number of threads', flush=True)\ntime.sleep(60)\n", 20)
        self.assertTrue(row['EmulatorError'])
        self.assertFalse(row['Passed'])
        self.assertFalse(row['TimedOut'])
        self.assertLess(time.monotonic() - started, 15)

    def test_case_timeout_is_terminated_and_cannot_pass(self):
        row = self.case('import time\ntime.sleep(60)\n', .2)
        self.assertTrue(row['TimedOut'])
        self.assertFalse(row['Passed'])
        self.assertFalse(row['EmulatorError'])

    def test_budget_is_bounded(self):
        for value in [0, -1, 18001]:
            with self.assertRaises(RuntimeError):
                one_test([], '', '', 0, pathlib.Path('unused'), value)

    def test_whole_job_budget_cannot_disable_its_four_hour_limit(self):
        for value in ['0', '-1', '14401', 'nan', 'inf', 'not-a-number']:
            with self.subTest(value=value), mock.patch.dict(os.environ, {JOB_BUDGET_KEY: value}):
                with self.assertRaises(RuntimeError):
                    job_budget()

    def budget_case(self, seconds):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            script = root / 'child.py'
            script.write_text('import time\ntime.sleep(60)\n', encoding='utf-8')
            exe = executable_key(script)
            (root / 'expected-executables.json').write_text(json.dumps({exe: dict(
                Package='file:///work/crates/probe#0', Target=dict(name='probe', kind=['test']))}))
            environment = {REPORT_KEY: str(root), PREFIX_KEY: json.dumps([sys.executable]),
                           JOB_DEADLINE_KEY: str(time.monotonic() + seconds)}
            with mock.patch.dict(os.environ, environment), mock.patch(
                    'sde_cpu_tests.native_list', return_value=['first', 'second']):
                os.environ.pop(SHARD_KEY, None)
                status = test_binary(str(script), [])
            reports = list(root.glob('*/report.json'))
            self.assertEqual(len(reports), 1)
            return status, json.loads(reports[0].read_text())

    def test_expired_job_keeps_the_plan_and_cannot_pass(self):
        status, report = self.budget_case(-1)
        self.assertNotEqual(status, 0)
        self.assertEqual(report['Selected'], ['first', 'second'])
        self.assertEqual(report['Results'], [])
        self.assertTrue(report['JobBudgetExhausted'])
        self.assertFalse(report['ExactlyOnce'])
        self.assertFalse(report['AllSelectedPassed'])

    def test_job_deadline_terminates_a_case_and_retains_unexecuted_names(self):
        status, report = self.budget_case(1)
        self.assertNotEqual(status, 0)
        self.assertEqual(report['Selected'], ['first', 'second'])
        self.assertEqual([row['Name'] for row in report['Results']], ['first'])
        self.assertTrue(report['Results'][0]['TimedOut'])
        self.assertTrue(report['JobBudgetExhausted'])
        self.assertFalse(report['AllSelectedPassed'])

    def test_boundary_selects_input_audit_and_every_original_execution_part(self):
        package, target, names = selection('boundary')
        self.assertEqual((package, target), ('opencolorio-rs', 'legacy_gpu_oracle'))
        self.assertEqual(names[0], 'boundary_legacy_parts_cover_original_requests_once')
        self.assertEqual(names[1:], [f'legacy_boundary_edges_match_native_wheel_part{i}' for i in range(7)])

    def test_each_selected_package_runs_in_its_own_directory(self):
        root = pathlib.Path(__file__).resolve().parents[2]
        for plan in ['boundary', 'python313', 'icc-generated', 'icc-corrupt']:
            package = selection(plan)[0]
            manifest = (root / 'crates' / PACKAGE_DIRECTORY[package] / 'Cargo.toml').read_text(encoding='utf-8')
            self.assertIn(f'name = "{package}"', manifest)

    def test_icc_and_python_cases_are_the_recorded_cases(self):
        self.assertEqual(selection('icc-generated'), ('ocio-tools', 'ociobakelut_icc_oracle', ['display_profiles_generated']))
        self.assertEqual(selection('icc-corrupt'), ('ocio-tools', 'ociobakelut_icc_oracle', ['corrupted_profiles']))
        self.assertEqual(selection('python313'), ('opencolorio-rs', 'api_formats_oracle', ['grading_rgb_curve_dynamic_sets_match_the_wheel_at_every_format_and_level']))


if __name__ == '__main__':
    unittest.main()
