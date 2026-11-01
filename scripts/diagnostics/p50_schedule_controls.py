# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Whole-case scheduling controls over actual Rust legacy test names and original inputs."""
import hashlib
import json
import pathlib
import re
import sys
import unittest

root = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root / 'scripts'))
from sde_cpu_tests import selected_shard

NAMES = re.findall(r'#\[test\]\s+fn\s+(\w+)', (root / 'crates/ocio/tests/legacy_gpu_oracle.rs').read_text())
META = dict(Package='path+file:///work/crates/ocio#0.1.0',
            Target=dict(kind=['test'], name='legacy_gpu_oracle'))


class ScheduleControls(unittest.TestCase):
    def test_all_original_and_future_names_once_for_every_partition_count(self):
        names = NAMES + ['future_legacy_case']
        for count in range(1, 17):
            with self.subTest(count=count):
                parts = [selected_shard(META, names, dict(Index=i, Count=count)) for i in range(count)]
                flattened = [name for part in parts for name in part]
                self.assertEqual(len(flattened), len(set(flattened)))
                self.assertEqual(sorted(flattened), sorted(names))

    def test_original_quick_request_plans_are_balanced(self):
        # Verified input counts, not expected OCIO outputs. P35 large uses its original
        # quick-tier single edge (40), while the committed full-tier evidence has 120.
        counters = json.loads((root / 'docs/legacy-gpu-matrix-validation.json').read_text())['Profiles'][0]['Counters']
        weights = {
            **{f'small_legacy_lattices_and_cache_order_match_native_wheel_part{i}':
               counters['P32']['Requests'] // 4 for i in range(4)},
            'small_legacy_parts_cover_original_requests_once': 0,
            'legacy_allocation_cuts_and_file_boundaries_match_native_wheel': counters['P33']['Requests'],
            'legacy_dynamic_positions_and_snapshot_values_match_native_wheel': counters['P34']['Requests'],
            **{f'legacy_boundary_edges_match_native_wheel_part{i}':
               counters['P35 boundary']['Requests'] // 7 for i in range(7)},
            'boundary_legacy_parts_cover_original_requests_once': 0,
            'legacy_large_lattice_values_match_native_wheel': 40,
            'ordinary_after_legacy_keeps_native_cached_shader_and_texture_values': counters['P36']['Requests'],
            'legacy_mixed_lut_range_and_alpha_lattices_match_native_wheel': counters['P37']['Requests'],
        }
        self.assertEqual(counters['P32']['Requests'], 4 * 420)
        self.assertEqual(counters['P35 boundary']['Requests'], 7 * 200)
        self.assertEqual(set(weights), set(NAMES))
        parts = [selected_shard(META, NAMES, dict(Index=i, Count=4)) for i in range(4)]
        loads = [sum(weights[name] for name in part) for part in parts]
        self.assertEqual(sum(loads), 7040)
        self.assertLessEqual(max(loads), 2000)
        self.assertGreaterEqual(min(loads), 1680)

    def test_other_targets_keep_the_original_hash_assignment(self):
        other = dict(Package=META['Package'], Target=dict(kind=['test'], name='other'))
        for index in range(4):
            expected = [n for n in NAMES if int.from_bytes(hashlib.sha256(('ocio/test/other\0' + n).encode()).digest()[:8], 'little') % 4 == index]
            self.assertEqual(selected_shard(other, NAMES, dict(Index=index, Count=4)), expected)

    def test_unsharded_probe_selection_is_unchanged(self):
        self.assertEqual(selected_shard(META, NAMES, None), NAMES)


if __name__ == '__main__':
    unittest.main()
