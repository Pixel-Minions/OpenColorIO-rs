# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Canonical report ordering controls; generated names are protocol inputs, not OCIO outputs."""
import copy
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from sde_join_shards import _ordered_results


class JoinOrderControls(unittest.TestCase):
    def test_native_order_after_disjoint_shards(self):
        original = ['first', 'second', 'third']
        rows = [(0, 'bucket0', dict(Name='second', Log='second.log', LogSHA256='second-hash')),
                (2, 'bucket2', dict(Name='first', Log='first.log', LogSHA256='first-hash')),
                (3, 'bucket3', dict(Name='third', Log='third.log', LogSHA256='third-hash'))]
        before = copy.deepcopy(rows)
        ordered = _ordered_results(original, rows)
        self.assertEqual([r['Name'] for _, _, r in ordered], original)
        self.assertEqual(rows, before)
        for item in ordered:
            self.assertTrue(any(item is source for source in rows))

    def test_native_order_can_differ_from_alphabetical_order(self):
        original = ['zeta', 'alpha', 'omega']
        rows = [(0, 'bucket0', dict(Name='omega')),
                (1, 'bucket1', dict(Name='zeta')),
                (2, 'bucket2', dict(Name='alpha'))]
        self.assertEqual([r['Name'] for _, _, r in _ordered_results(original, rows)], original)

    def test_missing_duplicate_and_unknown_cases_rejected(self):
        original = ['first', 'second']
        for names in ([], ['first'], ['first', 'first'], ['first', 'unknown']):
            with self.subTest(names=names):
                rows = [(index, 'bucket', dict(Name=name)) for index, name in enumerate(names)]
                with self.assertRaisesRegex(RuntimeError, 'Full profile union is incomplete'):
                    _ordered_results(original, rows)

    def test_empty_native_target(self):
        self.assertEqual(_ordered_results([], []), [])


if __name__ == '__main__':
    unittest.main()
