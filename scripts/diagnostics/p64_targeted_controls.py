# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Native watchdog and selection controls; launches no SDE, oracle or OCIO process."""
import pathlib
import sys
import tempfile
import time
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from sde_cpu_tests import one_test
from sde_targeted import selection, source_head


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

    def test_boundary_selects_input_audit_and_every_original_execution_part(self):
        package, target, names = selection('boundary')
        self.assertEqual((package, target), ('ocio', 'legacy_gpu_oracle'))
        self.assertEqual(names[0], 'boundary_legacy_parts_cover_original_requests_once')
        self.assertEqual(names[1:], [f'legacy_boundary_edges_match_native_wheel_part{i}' for i in range(7)])

    def test_icc_and_python_cases_are_the_recorded_cases(self):
        self.assertEqual(selection('icc-generated'), ('ocio-tools', 'ociobakelut_icc_oracle', ['display_profiles_generated']))
        self.assertEqual(selection('icc-corrupt'), ('ocio-tools', 'ociobakelut_icc_oracle', ['corrupted_profiles']))
        self.assertEqual(selection('python313'), ('ocio', 'api_formats_oracle', ['grading_rgb_curve_dynamic_sets_match_the_wheel_at_every_format_and_level']))


if __name__ == '__main__':
    unittest.main()
