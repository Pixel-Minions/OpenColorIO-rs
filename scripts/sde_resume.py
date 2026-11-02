# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Complete only the unproven Windows snb cases of run41386558773.

The test checkout stays at the original source. Controller changes do not turn old
passes into qualification of a newer library; final release qualification is separate.
"""
import argparse
import copy
import hashlib
import json
import os
import pathlib
import shlex
import shutil
import subprocess
import time

from sde_complete_profile import AUDIT, STEM, LUT, complete
from sde_cpu_tests import job_budget, native_list, one_test, selected_shard, source_head, target_key, write_json
from sde_join_shards import aggregate, check_results, load, recorded_executable, reports, require

ORIGINAL_RUN = 41386558773
ORIGINAL_HEAD = 'ee5671bc54ee23eb2e5ce4a8a05a86f096fd0bf8'
COMMAND = STEM + ['--skip', LUT[0], '--skip', LUT[1]]


def read(path):
    return json.loads(path.read_text(encoding='utf-8'))


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def compiler_manifest(path):
    artifacts = [json.loads(line) for line in path.read_text().splitlines()]
    return {recorded_executable(a['executable']): dict(Package=a['package_id'], Target=a['target'])
            for a in artifacts if a.get('reason') == 'compiler-artifact'
            and a['profile']['test'] and a.get('executable')}


def validate_rows(directory, report, rows):
    names = [row['Name'] for row in rows]
    check_results(directory, dict(Unfiltered=report['Unfiltered'], Selected=names,
        Results=rows, AllSelectedPassed=all(row['Passed'] for row in rows),
        ExactlyOnce=len(names) == len(set(names))))


def baseline(directory, head=ORIGINAL_HEAD):
    provenance = read(directory / 'run.json')
    require(provenance['id'] == ORIGINAL_RUN and provenance['head_sha'] == head,
            'Baseline run/source differs')
    require(provenance['repository']['full_name'] == 'Pixel-Minions/OpenColorIO-rs', 'Baseline repository differs')
    good = {}
    for index in [0, 2, 3]:
        good[index] = load(directory / f'sde-snb-Windows-kernel-shard-{index}')
    _, summary, expected, first = good[0]
    require(summary['Head'] == head and summary['CargoCommand'] == COMMAND, 'Changed original kernel plan')
    require('-snb' in summary['SDEPrefix'] and summary['SDEPrefix'][-1] == '--', 'Baseline CPU differs')
    for index, (_, other, compiled, found) in good.items():
        require(other['Shard'] == dict(Index=index, Count=4), 'Baseline shard differs')
        require(other['Head'] == head and other['CargoCommand'] == COMMAND
                and other['SDEPrefix'] == summary['SDEPrefix'], 'Baseline command/source/CPU differs')
        require(compiled == expected and set(found) == set(first), 'Baseline native targets differ')
        for key, (_, report) in found.items():
            for field in ['Unfiltered', 'OriginalSelected', 'ExcludedByOriginalArgs', 'CargoHarnessArgs', 'WorkingDirectory']:
                require(report[field] == first[key][1][field], 'Baseline native plan differs')
    roots = list((directory / 'sde-snb-Windows-kernel-shard-1').glob('run-*'))
    require(len(roots) == 1, 'Expected the original cancelled shard')
    partial = roots[0]
    require(not (partial / 'summary.json').exists(), 'The original cancelled shard changed')
    require(read(partial / 'expected-executables.json') == expected, 'Cancelled native manifest differs')
    require(compiler_manifest(partial / 'compiler-artifacts.jsonl') == expected,
            'Cancelled compiler/native inventories differ')
    preflight = reports(partial / 'preflight', expected)
    audits = []
    for folder, report in preflight.values():
        check_results(folder, report)
        audits.extend(report['Selected'])
    require(audits == [AUDIT], 'Cancelled shard runner preflight differs')
    partial_reports = {}
    for path in partial.glob('*/report.json'):
        report = read(path)
        key = report['LogicalTarget']
        require(key in first and key not in partial_reports, 'Unexpected cancelled target')
        original = first[key][1]
        require(report['Executable'] == original['Executable'], 'Cancelled executable differs')
        for field in ['Unfiltered', 'OriginalSelected', 'ExcludedByOriginalArgs', 'CargoHarnessArgs', 'WorkingDirectory']:
            require(report[field] == original[field], 'Cancelled native selection differs')
        selected = selected_shard(expected[report['Executable']], original['OriginalSelected'], dict(Index=1, Count=4))
        require(report['Selected'] == selected and report['Shard'] == dict(Index=1, Count=4), 'Cancelled shard plan differs')
        observed = [row['Name'] for row in report['Results']]
        require(observed == selected[:len(observed)] and len(observed) == len(set(observed)), 'Cancelled results are not an exact prefix')
        validate_rows(path.parent, report, report['Results'])
        partial_reports[key] = (path.parent, report)
    missing = []
    retained = {}
    targets = {}
    for key, (_, original) in first.items():
        exe = original['Executable']
        selected = selected_shard(expected[exe], original['OriginalSelected'], dict(Index=1, Count=4))
        prior = partial_reports.get(key)
        rows = [] if prior is None else prior[1]['Results']
        for row in rows:
            retained[(key, row['Name'])] = (prior[0], row)
        missing.extend(dict(Target=key, Name=name) for name in selected if (key, name) not in retained)
        targets[key] = {field: copy.deepcopy(original[field]) for field in
            ['Executable', 'WorkingDirectory', 'CargoHarnessArgs', 'Unfiltered', 'OriginalSelected', 'ExcludedByOriginalArgs']}
        targets[key]['Selected'] = selected
    return summary, expected, targets, retained, missing, partial


def plan(directory, head=ORIGINAL_HEAD):
    summary, expected, targets, retained, missing, _ = baseline(directory, head)
    # Artifact extraction and directory iteration need not agree across hosts.
    missing = sorted(missing, key=lambda case: (case['Target'], case['Name']))
    weights = {}
    rocky = directory / 'sde-snb-Rocky-complete-profile'
    if rocky.exists():
        lut = [p.parent for p in (rocky / 'sde-lut-input').rglob('summary.json') if p.parent.name != 'preflight']
        require(len(lut) == 1, 'Rocky LUT receipts differ')
        complete(rocky / 'sde-cpu-tests-joined/profile', lut[0], 'snb', head)
        linux_expected = read(rocky / 'sde-cpu-tests-joined/profile/expected-executables.json')
        for path in (rocky / 'sde-cpu-tests-joined/profile').glob('*/report.json'):
            report = read(path)
            key = target_key(linux_expected[report['Executable']])
            for row in report['Results']:
                weights[(key, row['Name'])] = row['ElapsedSeconds']
    parts = [[] for _ in range(4)]
    loads = [0.0] * 4
    for case in sorted(missing, key=lambda c: (-weights.get((c['Target'], c['Name']), 1), c['Target'], c['Name'])):
        index = min(range(4), key=lambda i: (loads[i], i))
        parts[index].append(case)
        loads[index] += weights.get((case['Target'], case['Name']), 1)
    flattened = [(c['Target'], c['Name']) for part in parts for c in part]
    require(len(flattened) == len(set(flattened)) == len(missing), 'Missing-only plan duplicated a case')
    value = dict(SourceRun=ORIGINAL_RUN, Head=head, CPU='snb', CargoCommand=COMMAND,
        SDEPrefix=summary['SDEPrefix'], ExpectedExecutables=expected, Targets=targets,
        RetainedCancelledShardCases=len(retained), Missing=missing, Parts=parts,
        EstimatedPartSeconds=loads, CompleteCPUProfile=False)
    value['PlanSHA256'] = digest(value)
    return value


def read_plan(path):
    value = read(path)
    recorded = value.pop('PlanSHA256')
    require(digest(value) == recorded, 'Resume plan hash differs')
    value['PlanSHA256'] = recorded
    require(value['SourceRun'] == ORIGINAL_RUN and value['Head'] == ORIGINAL_HEAD and value['CPU'] == 'snb', 'Unexpected resume source/CPU')
    require(value['CargoCommand'] == COMMAND and len(value['Parts']) == 4, 'Unexpected resume command/partition')
    original = [(c['Target'], c['Name']) for c in value['Missing']]
    partition = [(c['Target'], c['Name']) for p in value['Parts'] for c in p]
    require(len(original) == len(set(original)) and len(partition) == len(set(partition))
            and sorted(partition) == sorted(original), 'Missing-only partition union differs')
    require(all(key in value['Targets'] and name in value['Targets'][key]['Selected']
                for key, name in original), 'Missing case is outside the native selection')
    return value


def execute(plan_path, checkout, output, part):
    value = read_plan(plan_path)
    checkout = checkout.resolve()
    require(os.name == 'nt' and source_head(checkout) == value['Head'], 'Execute only the original Windows source')
    for args in [['diff', '--quiet'], ['diff', '--cached', '--quiet']]:
        require(subprocess.run(['git', '-c', 'safe.directory=' + checkout.as_posix(), '-C', str(checkout), *args]).returncode == 0,
                'Qualification checkout has tracked changes')
    require(0 <= part < len(value['Parts']), 'Invalid resume partition')
    prefix = shlex.split(os.environ['CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER'])
    require(prefix == value['SDEPrefix'], 'Resume emulator/CPU prefix differs')
    require(os.environ.get('OCIO_RS_TIER') == 'quick' and os.environ.get('OCIO_RS_ORACLE_NO_CACHE') == '1', 'Resume tier/cache settings differ')
    require(not output.exists(), 'Resume output must be new')
    output.mkdir(parents=True)
    build = COMMAND[:COMMAND.index('--')] + ['--no-run', '--message-format=json']
    with (output / 'compiler-artifacts.jsonl').open('w', encoding='utf-8') as log:
        subprocess.run(build, cwd=checkout, stdout=log, check=True)
    compiled = compiler_manifest(output / 'compiler-artifacts.jsonl')
    require(compiled == value['ExpectedExecutables'], 'Rebuilt original native manifest differs')
    binaries = {target_key(metadata): exe for exe, metadata in compiled.items()}
    inventory = {key: native_list(binaries[key], []) for key in value['Targets']}
    require(inventory == {key: target['Unfiltered'] for key, target in value['Targets'].items()},
            'Rebuilt native test inventory differs')
    write_json(output / 'native-inventory.json', inventory)
    selected = value['Parts'][part]
    receipt = dict(Head=value['Head'], SourceRun=ORIGINAL_RUN, CPU='snb', Part=part, Parts=4,
        PlanSHA256=value['PlanSHA256'], SDEPrefix=prefix, Planned=selected, Results=[],
        AllSelectedPassed=False, CompleteCPUProfile=False, JobBudgetSeconds=job_budget())
    write_json(output / 'receipt.json', receipt)
    deadline = time.monotonic() + receipt['JobBudgetSeconds']
    for index, case in enumerate(selected):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        key = case['Target']
        folder = key.split('/')[0]
        cwd = (checkout / 'crates' / folder).resolve()
        require(cwd.is_dir() and cwd.is_relative_to(checkout), 'Unexpected original working directory')
        require(recorded_executable(str(cwd)) == recorded_executable(value['Targets'][key]['WorkingDirectory']),
                'Original working directory differs')
        os.chdir(cwd)
        row = one_test(prefix, binaries[key], case['Name'], len(value['Targets'][key]['Unfiltered']),
            output / f'test-{index:05}.log', min(7200, remaining))
        receipt['Results'].append(dict(Target=key, **row))
        write_json(output / 'receipt.json', receipt)
    receipt['AllSelectedPassed'] = len(receipt['Results']) == len(selected) and all(r['Passed'] for r in receipt['Results'])
    write_json(output / 'receipt.json', receipt)
    print(json.dumps(receipt, indent=2))
    return 0 if receipt['AllSelectedPassed'] else 1


def finish(directory, plan_path, part_directories, output):
    value = read_plan(plan_path)
    actual_plan = plan(directory)
    require(value == actual_plan, 'Original baseline/plan changed')
    summary, expected, targets, retained, _, partial = baseline(directory)
    resumed = {}
    parts = set()
    for directory_part in part_directories:
        directory_part = pathlib.Path(directory_part)
        paths = list(directory_part.rglob('receipt.json'))
        require(len(paths) == 1, 'Each resume partition needs one receipt')
        folder = paths[0].parent
        receipt = read(paths[0])
        index = receipt['Part']
        require(type(index) is int and index not in parts and 0 <= index < 4 and receipt['Parts'] == 4, 'Missing/duplicate resume partition')
        parts.add(index)
        require(receipt['Head'] == value['Head'] and receipt['CPU'] == 'snb' and receipt['SourceRun'] == ORIGINAL_RUN
                and receipt['PlanSHA256'] == value['PlanSHA256'] and receipt['SDEPrefix'] == value['SDEPrefix'], 'Resume provenance differs')
        require(compiler_manifest(folder / 'compiler-artifacts.jsonl') == expected,
                'Resume compiler manifest differs')
        require(read(folder / 'native-inventory.json') == {key: target['Unfiltered'] for key, target in targets.items()},
                'Resume native inventory differs')
        require(receipt['Planned'] == value['Parts'][index] and receipt['AllSelectedPassed'], 'Incomplete resume selection')
        observed = [(r['Target'], r['Name']) for r in receipt['Results']]
        require(observed == [(c['Target'], c['Name']) for c in value['Parts'][index]], 'Resume cases missing/reordered/duplicated')
        for row in receipt['Results']:
            key = (row['Target'], row['Name'])
            require(key not in resumed and key not in retained, 'A passed case was rerun or duplicated')
            validate_rows(folder, targets[key[0]], [row])
            resumed[key] = (folder, row)
    require(parts == set(range(4)) and set(resumed) == {(c['Target'], c['Name']) for c in value['Missing']}, 'Missing-only union incomplete')
    require(not output.exists(), 'Final output must be new')
    output.mkdir(parents=True)
    restored = output / 'restored-shard-1'
    restored.mkdir()
    write_json(restored / 'expected-executables.json', expected)
    shutil.copyfile(partial / 'compiler-artifacts.jsonl', restored / 'compiler-artifacts.jsonl')
    shutil.copytree(partial / 'preflight', restored / 'preflight')
    for key, original in targets.items():
        exe = original['Executable']
        folder = restored / hashlib.sha256(exe.encode()).hexdigest()
        folder.mkdir()
        report = dict(original, Shard=dict(Index=1, Count=4), LogicalTarget=key, Results=[], ExactlyOnce=True, AllSelectedPassed=True)
        for index, name in enumerate(original['Selected']):
            origin, row = retained.get((key, name), resumed.get((key, name), (None, None)))
            require(row is not None, 'Restored original shard has a missing case')
            log = f'restored-{index:05}.log'
            shutil.copyfile(origin / row['Log'], folder / log)
            report['Results'].append(dict(row, Log=log))
        write_json(folder / 'report.json', report)
    total = sum(len(t['Selected']) for t in targets.values())
    restored_summary = dict(summary, Shard=dict(Index=1, Count=4), SelectedTests=total, PassedTests=total,
        AllSelectedTestsPassed=True, CargoExit=0, ObservedExecutables=len(expected), CompleteExecutableManifest=True,
        ResumedOnlyMissing=True, OriginalRun=ORIGINAL_RUN, ResumedCases=len(resumed))
    write_json(restored / 'summary.json', restored_summary)
    kernel = output / 'kernel'
    aggregate([directory / 'sde-snb-Windows-kernel-shard-0', restored,
        directory / 'sde-snb-Windows-kernel-shard-2', directory / 'sde-snb-Windows-kernel-shard-3'], kernel)
    lut = [p.parent for p in (directory / 'sde-snb-Windows-lut').rglob('summary.json') if p.parent.name != 'preflight']
    require(len(lut) == 1, 'Original Windows LUT receipt differs')
    proof = complete(kernel, lut[0], 'snb', value['Head'])
    proof.update(RecoveredFromRun=ORIGINAL_RUN, ExecutedMissingCases=len(resumed), RetainedCancelledShardCases=len(retained), ReleaseQualification=False)
    write_json(output / 'windows-proof.json', proof)
    rocky = directory / 'sde-snb-Rocky-complete-profile'
    lut = [p.parent for p in (rocky / 'sde-lut-input').rglob('summary.json') if p.parent.name != 'preflight']
    require(len(lut) == 1, 'Original Rocky LUT receipt differs')
    rocky_proof = complete(rocky / 'sde-cpu-tests-joined/profile', lut[0], 'snb', value['Head'])
    rocky_proof.update(RecoveredFromRun=ORIGINAL_RUN, ExecutedMissingCases=0, ReleaseQualification=False)
    write_json(output / 'rocky-proof.json', rocky_proof)
    print(json.dumps(dict(Windows=proof, Rocky=rocky_proof), indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='mode', required=True)
    planning = sub.add_parser('plan')
    planning.add_argument('--baseline', type=pathlib.Path, required=True)
    planning.add_argument('--output', type=pathlib.Path, required=True)
    running = sub.add_parser('execute')
    running.add_argument('--plan', type=pathlib.Path, required=True)
    running.add_argument('--checkout', type=pathlib.Path, required=True)
    running.add_argument('--output', type=pathlib.Path, required=True)
    running.add_argument('--part', type=int, required=True)
    joining = sub.add_parser('join')
    joining.add_argument('--baseline', type=pathlib.Path, required=True)
    joining.add_argument('--plan', type=pathlib.Path, required=True)
    joining.add_argument('--output', type=pathlib.Path, required=True)
    joining.add_argument('parts', type=pathlib.Path, nargs='+')
    args = parser.parse_args()
    if args.mode == 'plan':
        value = plan(args.baseline)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        write_json(args.output, value)
        print(json.dumps(dict(Head=value['Head'], Retained=value['RetainedCancelledShardCases'],
            Missing=len(value['Missing']), Parts=[len(p) for p in value['Parts']],
            EstimatedSeconds=value['EstimatedPartSeconds'], PlanSHA256=value['PlanSHA256']), indent=2))
    elif args.mode == 'execute':
        raise SystemExit(execute(args.plan, args.checkout, args.output.resolve(), args.part))
    else:
        finish(args.baseline, args.plan, args.parts, args.output)
