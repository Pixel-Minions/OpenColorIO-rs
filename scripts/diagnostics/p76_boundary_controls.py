# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Boundary-audit rejection controls using libtest's observed output framing."""
import hashlib
import json
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from sde_targeted import boundary_union, selection


class BoundaryControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = pathlib.Path(self.temp.name)
        self.rows = []
        for index, name in enumerate(selection('boundary')[2]):
            if index == 0:
                body = 'P64 input audit: all 1400 original requests exactly once in seven 200-request parts'
            else:
                part = index - 1
                body = (f'P64 boundary part {part}/7 original indices: {json.dumps(list(range(part, 1400, 7)))}\n'
                        'P35 boundary: 200 exact legacy requests: 100 shaders/textures, 100 matching refusals')
            # Input-only structural controls: no expected OCIO values are synthesized.
            text = (f'\nrunning 1 test\ntest {name} ... {body}\nok\n\n'
                    'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 1s\n')
            self.rows.append(dict(Name=name, Passed=True, ExitCode=0, EmulatorError=False,
                                  TimedOut=False, Log=f'test-{index:02}.log'))
            self.write(index, text)

    def write(self, index, text):
        raw = text.encode('utf-8')
        (self.directory / self.rows[index]['Log']).write_bytes(raw)
        self.rows[index]['LogSHA256'] = hashlib.sha256(raw).hexdigest()

    def change(self, index, before, after):
        text = (self.directory / self.rows[index]['Log']).read_text(encoding='utf-8')
        self.assertIn(before, text)
        self.write(index, text.replace(before, after))

    def audit(self):
        return boundary_union(self.rows, self.directory, 8)

    def test_inline_libtest_prefix_retains_exact_once_union(self):
        self.assertTrue(self.audit()['ExactlyOnce'])
        self.assertEqual(self.audit()['OriginalRequests'], 1400)

    def test_separate_lines_and_crlf_retain_same_union(self):
        expected = self.audit()
        for index, row in enumerate(self.rows):
            text = (self.directory / row['Log']).read_text(encoding='utf-8')
            text = text.replace(f"test {row['Name']} ... ", f"test {row['Name']} ... \n")
            self.write(index, text.replace('\n', '\r\n'))
        self.assertEqual(self.audit(), expected)

    def test_missing_marker_is_rejected(self):
        self.change(1, 'P64 boundary part 0/7', 'hidden output 0/7')
        with self.assertRaises(RuntimeError):
            self.audit()

    def test_duplicate_marker_is_rejected(self):
        line = (self.directory / self.rows[1]['Log']).read_text(encoding='utf-8').splitlines()[2]
        self.change(1, '\nok\n', '\n' + line + '\nok\n')
        with self.assertRaises(RuntimeError):
            self.audit()

    def test_wrong_or_foreign_test_prefix_is_rejected(self):
        self.change(1, f"test {self.rows[1]['Name']} ... P64", 'test another_case ... P64')
        with self.assertRaises(RuntimeError):
            self.audit()

    def test_wrong_input_index_is_rejected(self):
        self.change(1, '[0, 7, 14,', '[0, 0, 14,')
        with self.assertRaises(RuntimeError):
            self.audit()

    def test_noninteger_input_index_is_rejected(self):
        self.change(1, '[0, 7, 14,', '[0.0, 7, 14,')
        with self.assertRaises(RuntimeError):
            self.audit()

    def test_incomplete_comparison_count_is_rejected(self):
        self.change(1, '100 matching refusals', '99 matching refusals')
        with self.assertRaises(RuntimeError):
            self.audit()

    def test_duplicate_comparison_count_is_rejected(self):
        counts = 'P35 boundary: 200 exact legacy requests: 100 shaders/textures, 100 matching refusals'
        self.change(1, counts, counts + '\n' + counts)
        with self.assertRaises(RuntimeError):
            self.audit()

    def test_hash_mismatch_is_rejected(self):
        self.rows[1]['LogSHA256'] = '0' * 64
        with self.assertRaises(RuntimeError):
            self.audit()

    def test_missing_or_duplicated_case_is_rejected(self):
        self.rows[-1] = self.rows[-2]
        with self.assertRaises(RuntimeError):
            self.audit()

    def test_missing_plan_audit_is_rejected(self):
        self.change(0, 'P64 input audit:', 'hidden audit:')
        with self.assertRaises(RuntimeError):
            self.audit()

    def test_failed_ignored_or_emulator_case_is_rejected(self):
        self.change(1, '0 ignored;', '1 ignored;')
        with self.assertRaises(RuntimeError):
            self.audit()

    def test_failed_metadata_is_rejected(self):
        self.rows[1]['Passed'] = False
        with self.assertRaises(RuntimeError):
            self.audit()


if __name__ == '__main__':
    unittest.main()
