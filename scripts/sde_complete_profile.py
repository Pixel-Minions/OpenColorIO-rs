# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Prove the kernel/LUT partition covers the complete native CPU alias once."""
import argparse
import json
import pathlib
import re

from sde_cpu_tests import LUT_SWEEPS, write_json
from sde_join_shards import check_results, recorded_executable, reports, require
from sde_targeted import boundary_union, selection

LUT = list(LUT_SWEEPS)
STEM = ['cargo', 'cpu-tests', '--release', '--no-fail-fast', '--']
AUDIT = 'apphelpers::mergeconfigs::section_merger::oracle_tests::whole_merge_parts_cover_original_cases_once'


def load(root):
    summary = json.loads((root / 'summary.json').read_text(encoding='utf-8'))
    require(summary['AllSelectedTestsPassed'] and summary['CargoExit'] == 0
            and summary['CompleteExecutableManifest'], 'Incomplete CPU selection')
    expected = json.loads((root / 'expected-executables.json').read_text(encoding='utf-8'))
    artifacts = [json.loads(line) for line in (root / 'compiler-artifacts.jsonl').read_text(encoding='utf-8').splitlines()]
    compiled = {recorded_executable(a['executable']): dict(Package=a['package_id'], Target=a['target'])
                for a in artifacts if a.get('reason') == 'compiler-artifact'
                and a['profile']['test'] and a.get('executable')}
    require(compiled == expected, 'Native compiler/executable inventories differ')
    found = reports(root, expected)
    require(summary['ExpectedExecutables'] == summary['ObservedExecutables'] == len(found),
            'Native target totals differ')
    total = 0
    for directory, report in found.values():
        check_results(directory, report)
        require(report['ExcludedByOriginalArgs'] == sorted(set(report['Unfiltered']) - set(report['Selected'])),
                'Native exclusions differ')
        total += len(report['Selected'])
    require(summary['SelectedTests'] == summary['PassedTests'] == total, 'Native case totals differ')
    audits = []
    for directory, report in reports(root / 'preflight', expected).values():
        check_results(directory, report)
        audits.extend(report['Selected'])
    require(audits == [AUDIT], 'Missing actual Cargo runner preflight')
    return summary, found


def complete(kernel_root, lut_root, cpu, head):
    require(re.fullmatch(r'[0-9a-f]{40}', head) is not None, 'Invalid expected source SHA')
    kernels, kernel_reports = load(kernel_root)
    luts, lut_reports = load(lut_root)
    require(kernels.get('Head') == luts.get('Head') == head, 'Profile source SHA differs')
    require(kernels['SDEPrefix'] == luts['SDEPrefix'] and '-' + cpu in kernels['SDEPrefix']
            and kernels['SDEPrefix'][-1] == '--', 'Profile CPU/emulator differs')
    require(kernels['CargoCommand'] == STEM + ['--skip', LUT[0], '--skip', LUT[1]]
            and luts['CargoCommand'] == STEM + LUT, 'Profile changed the original partition')
    require(kernels.get('AggregatedShards') == 4, 'Missing original four kernel shards')
    require(set(kernel_reports) == set(lut_reports), 'Kernel/LUT target sets differ')
    total = 0
    boundary = None
    targets = []
    for key, (directory, kernel) in kernel_reports.items():
        _, lut = lut_reports[key]
        require(kernel['Unfiltered'] == lut['Unfiltered'], 'Kernel/LUT native inventory differs')
        original = set(kernel['Unfiltered'])
        selected_kernel, selected_lut = set(kernel['Selected']), set(lut['Selected'])
        require(not selected_kernel & selected_lut and selected_kernel | selected_lut == original,
                'Complete CPU profile has missing or duplicate cases')
        expected_lut = set(LUT) if key.endswith('/test/api_formats_oracle') else set()
        require(selected_lut == expected_lut, 'LUT filter selected unexpected cases')
        if key.endswith('/test/legacy_gpu_oracle'):
            rows = {row['Name']: row for row in kernel['Results']}
            names = selection('boundary')[2]
            require(set(names) <= set(rows), 'Missing boundary audit or part')
            boundary = boundary_union([rows[name] for name in names], directory, len(original))
        total += len(original)
        targets.append(dict(Target=key, Cases=len(original), Kernels=len(selected_kernel), LUT=len(selected_lut)))
    require(boundary is not None, 'The complete profile did not prove the original boundary union')
    return dict(Head=head, CPU=cpu, CompleteCPUProfile=True, ExactlyOnce=True,
                NativeTargets=len(targets), NativeTests=total, BoundaryInputUnion=boundary,
                Targets=targets, KernelSummary=str(kernel_root / 'summary.json'),
                LUTSummary=str(lut_root / 'summary.json'))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel', type=pathlib.Path, required=True)
    parser.add_argument('--lut', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--cpu', required=True)
    parser.add_argument('--head', required=True)
    args = parser.parse_args()
    roots = [p.parent for p in args.lut.rglob('summary.json') if p.parent.name != 'preflight']
    require(len(roots) == 1, 'Expected one complete LUT run')
    proof = complete(args.kernel, roots[0], args.cpu, args.head)
    write_json(args.output, proof)
    print(json.dumps(proof, indent=2))
