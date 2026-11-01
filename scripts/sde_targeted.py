# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""The owner's bounded follow-up selection; this never claims a complete CPU profile."""
import argparse
import hashlib
import json
import os
import pathlib
import re
import shlex
import subprocess
import sys

from sde_cpu_tests import native_list, one_test, write_json


def selection(plan):
    if plan == 'boundary':
        return 'ocio', 'legacy_gpu_oracle', [
            'boundary_legacy_parts_cover_original_requests_once',
            *[f'legacy_boundary_edges_match_native_wheel_part{i}' for i in range(7)],
        ]
    if plan == 'python313':
        return 'ocio', 'api_formats_oracle', [
            'grading_rgb_curve_dynamic_sets_match_the_wheel_at_every_format_and_level',
        ]
    name = {'icc-generated': 'display_profiles_generated',
            'icc-corrupt': 'corrupted_profiles'}[plan]
    return 'ocio-tools', 'ociobakelut_icc_oracle', [name]


def boundary_union(rows, directory):
    observed = []
    for part, row in enumerate(rows[1:]):
        text = (directory / row['Log']).read_text(encoding='utf-8')
        matches = re.findall(rf'^P64 boundary part {part}/7 original indices: (\[.*\])$', text, re.M)
        if len(matches) != 1:
            raise RuntimeError('Missing or duplicate original boundary input selection')
        indices = json.loads(matches[0])
        if indices != list(range(part, 1400, 7)):
            raise RuntimeError('Boundary execution changed its original request indices')
        counts = re.findall(r'^P35 boundary: 200 exact legacy requests: (\d+) shaders/textures, (\d+) matching refusals$', text, re.M)
        if len(counts) != 1 or sum(map(int, counts[0])) != 200:
            raise RuntimeError('Boundary part did not compare all200 original requests')
        observed.extend(indices)
    if len(observed) != len(set(observed)) or sorted(observed) != list(range(1400)):
        raise RuntimeError('The boundary parts do not cover every original request exactly once')
    return dict(OriginalRequests=1400, Executions=7, RequestsPerExecution=200,
                ExactlyOnce=True, OriginalIndexSHA256=hashlib.sha256(json.dumps(sorted(observed)).encode()).hexdigest())


def run(plan, cpu):
    windows = os.name == 'nt'
    if plan == 'python313' and (not windows or cpu != 'snb'):
        raise RuntimeError('The Python313.dll case belongs to Windows snb')
    if plan.startswith('icc-') and (windows or cpu != 'snb'):
        raise RuntimeError('The owed ICC sweeps belong to Rocky snb')
    root = pathlib.Path(__file__).resolve().parents[1]
    os.chdir(root)
    triple = 'X86_64_PC_WINDOWS_MSVC' if windows else 'X86_64_UNKNOWN_LINUX_GNU'
    prefix = shlex.split(os.environ['CARGO_TARGET_' + triple + '_RUNNER'])
    if not prefix or prefix[-1] != '--' or '-' + cpu not in prefix:
        raise RuntimeError('Use the unchanged pinned scripts/sde.sh with the selected CPU')
    if os.environ.get('OCIO_RS_TIER') != ('full' if plan.startswith('icc-') else 'quick'):
        raise RuntimeError('Retain full ICC sweeps and the original quick CPU tier')
    package, target, selected = selection(plan)
    directory = (root / 'kept/sde-targeted' / f'{plan}-{cpu}').resolve()
    directory.mkdir(parents=True, exist_ok=False)
    command = ['cargo', 'test', '--release', '--package', package, '--test', target,
               '--no-run', '--message-format=json']
    with (directory / 'compiler-artifacts.jsonl').open('w', encoding='utf-8') as output:
        subprocess.run(command, stdout=output, check=True)
    artifacts = [json.loads(line) for line in (directory / 'compiler-artifacts.jsonl').read_text(encoding='utf-8').splitlines()]
    executables = [a['executable'] for a in artifacts if a.get('reason') == 'compiler-artifact'
                   and a.get('executable') and a['profile']['test'] and a['target']['name'] == target]
    if len(executables) != 1:
        raise RuntimeError('Expected exactly the explicitly selected native integration target')
    binary = str(pathlib.Path(executables[0]).resolve())
    native = native_list(binary, [])
    if not set(selected) <= set(native) or len(selected) != len(set(selected)):
        raise RuntimeError('Missing or duplicate native tests in the focused selection')
    report = dict(Plan=plan, CPU=cpu, Head=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
                  SDEPrefix=prefix, BuildCommand=command, Tier=os.environ['OCIO_RS_TIER'],
                  NativeInventory=native, Selected=selected, Results=[],
                  CompleteCPUProfile=False, AllSelectedPassed=False)
    write_json(directory / 'report.json', report)
    os.chdir(root / 'crates' / package)
    for index, name in enumerate(selected):
        # Larger full ICC sweeps may take hours; their date check uses the actual run bounds.
        # Other cases retain the existing two-hour limit. Neither changes comparisons.
        budget = 18000 if plan.startswith('icc-') else 7200
        row = one_test(prefix, binary, name, len(native), directory / f'test-{index:02}.log', budget)
        report['Results'].append(row)
        write_json(directory / 'report.json', report)
        print(json.dumps(row), flush=True)
    report['AllSelectedPassed'] = all(row['Passed'] for row in report['Results'])
    if plan == 'boundary' and report['AllSelectedPassed']:
        report['OriginalInputUnion'] = boundary_union(report['Results'], directory)
    write_json(directory / 'report.json', report)
    print(json.dumps(report, indent=2), flush=True)
    return 0 if report['AllSelectedPassed'] else 1


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', choices=['boundary', 'python313', 'icc-generated', 'icc-corrupt'], required=True)
    parser.add_argument('--cpu', choices=['nhm', 'snb'], required=True)
    arguments = parser.parse_args()
    raise SystemExit(run(arguments.plan, arguments.cpu))
