# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Join complete, disjoint SDE partitions into the original full-profile format."""
import hashlib
import json
import ntpath
import pathlib
import posixpath
import re
import shutil
import sys

from sde_cpu_tests import selected_shard, target_key, write_json


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def read(path):
    return json.loads(path.read_text(encoding='utf-8'))


def recorded_executable(path):
    if re.match(r'^[A-Za-z]:', path) or path.startswith('\\\\'):
        return ntpath.normcase(ntpath.normpath(path))
    return posixpath.normpath(path)


def reports(root, expected):
    found = {}
    for path in root.glob('*/report.json'):
        report = read(path)
        executable = report['Executable']
        require(executable in expected, 'Unexpected executable report')
        key = target_key(expected[executable])
        require(key not in found, 'Duplicate logical target report')
        found[key] = (path.parent, report)
    require(set(found) == {target_key(m) for m in expected.values()},
            'Incomplete native executable manifest')
    return found


def check_results(directory, report):
    names = report['Selected']
    unfiltered = report['Unfiltered']
    rows = report['Results']
    require(len(unfiltered) == len(set(unfiltered)), 'Duplicate unfiltered name')
    require(len(names) == len(set(names)), 'Duplicate selected name')
    require(report['AllSelectedPassed'] and report['ExactlyOnce'], 'Incomplete selected tests')
    observed = [row['Name'] for row in rows]
    require(len(observed) == len(set(observed)) == len(names) and set(observed) == set(names),
            'Missing or duplicated case result')
    for row in rows:
        require(row['Passed'] and row['ExitCode'] == 0 and not row['EmulatorError']
                and not row['TimedOut'], 'A selected case did not pass')
        name = row['Log']
        require(pathlib.PurePath(name).name == name and '/' not in name and '\\' not in name,
                'Case log must be a local basename')
        data = (directory / name).read_bytes()
        require(hashlib.sha256(data).hexdigest() == row['LogSHA256'], 'Case log hash mismatch')
        text = data.decode('utf-8', errors='replace')
        footer = rf'^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; {len(unfiltered)-1} filtered out;'
        require('running 1 test' in text and f"test {row['Name']} " in text
                and re.search(footer, text, re.M) is not None, 'Missing actual one-test pass footer')
        require('SDE ERROR' not in text and 'maxmimal number of threads' not in text,
                'Emulator error in raw case log')


def load(directory):
    roots = [p.parent for p in directory.rglob('summary.json') if p.parent.name != 'preflight']
    require(len(roots) == 1, 'Each shard must contain one complete run')
    root = roots[0]
    summary = read(root / 'summary.json')
    require(summary['AllSelectedTestsPassed'] and summary['CargoExit'] == 0
            and summary['CompleteExecutableManifest'], 'Incomplete shard summary')
    shard = summary.get('Shard')
    require(isinstance(shard, dict) and set(shard) == {'Index', 'Count'}, 'Missing shard identity')
    require(type(shard['Count']) is int and type(shard['Index']) is int, 'Shard identity must use integers')
    require(1 <= shard['Count'] <= 16 and 0 <= shard['Index'] < shard['Count'], 'Invalid shard')
    require(summary.get('RequiresShardAggregation') is True, 'Unexpected unpartitioned input')
    expected = read(root / 'expected-executables.json')
    artifacts = [json.loads(line) for line in (root / 'compiler-artifacts.jsonl').read_text().splitlines()]
    # Compiler paths belong to the recorded host, not the machine doing aggregation.
    compiled = {recorded_executable(item['executable']): dict(Package=item['package_id'], Target=item['target'])
                for item in artifacts if item.get('reason') == 'compiler-artifact'
                and item['profile']['test'] and item.get('executable')}
    require(compiled == expected, 'Native compiler inventory differs from selected executables')
    require(len(expected) == len({target_key(m) for m in expected.values()}), 'Duplicate target keys')
    found = reports(root, expected)
    require(summary['ExpectedExecutables'] == summary['ObservedExecutables'] == len(found),
            'Shard target count differs')
    selected_count = 0
    original_count = 0
    excluded = []
    for key, (path, report) in found.items():
        require(report.get('Shard') == shard and report.get('LogicalTarget') == key,
                'Report shard/target identity differs')
        original = report['OriginalSelected']
        require(len(original) == len(set(original)) and set(original).issubset(report['Unfiltered']),
                'Invalid original native selection')
        require(report['ExcludedByOriginalArgs'] == sorted(set(report['Unfiltered']) - set(original)),
                'Original exclusions differ')
        require(report['Selected'] == selected_shard(expected[report['Executable']], original, shard),
                'Partition is not the native deterministic selection')
        check_results(path, report)
        selected_count += len(report['Selected'])
        original_count += len(original)
        excluded.extend(report['ExcludedByOriginalArgs'])
    require(summary['SelectedTests'] == summary['PassedTests'] == selected_count
            and summary['OriginalSelectedTests'] == original_count, 'Shard totals differ')
    require(sorted(summary['ExcludedByOriginalArgs']) == sorted(excluded), 'Summary exclusions differ')
    preflight = reports(root / 'preflight', expected)
    audits = []
    for path, report in preflight.values():
        check_results(path, report)
        audits.extend(report['Selected'])
    require(audits == ['apphelpers::mergeconfigs::section_merger::oracle_tests::whole_merge_parts_cover_original_cases_once'],
            'Real Cargo runner audit is missing')
    return root, summary, expected, found


def _ordered_results(original, combined):
    observed = [row['Name'] for _, _, row in combined]
    require(len(observed) == len(set(observed)) == len(original)
            and set(observed) == set(original), 'Full profile union is incomplete')
    positions = {name: index for index, name in enumerate(original)}
    return sorted(combined, key=lambda item: positions[item[2]['Name']])


def aggregate(directories, output):
    loaded = [load(pathlib.Path(d)) for d in directories]
    require(loaded, 'No shard artifacts')
    count = loaded[0][1]['Shard']['Count']
    require(len(loaded) == count and {x[1]['Shard']['Index'] for x in loaded} == set(range(count)),
            'Missing or duplicate shards')
    loaded.sort(key=lambda x: x[1]['Shard']['Index'])
    first_root, first_summary, expected, first_reports = loaded[0]
    original_plan = {}
    for _, summary, _, found in loaded:
        require(summary['CargoCommand'] == first_summary['CargoCommand']
                and summary['SDEPrefix'] == first_summary['SDEPrefix'], 'Command or CPU prefix differs')
        require(summary.get('Head') == first_summary.get('Head'), 'Source commit differs')
        require(set(found) == set(first_reports), 'Native target sets differ')
        for key, (_, report) in found.items():
            _, first = first_reports[key]
            for field in ('Unfiltered', 'OriginalSelected', 'ExcludedByOriginalArgs', 'CargoHarnessArgs', 'WorkingDirectory'):
                require(report[field] == first[field], f'Native plan differs: {key}/{field}')
            original_plan[key] = dict(Unfiltered=first['Unfiltered'], Selected=first['OriginalSelected'])
    merged = {}
    for key, (_, first) in first_reports.items():
        combined = [(shard, path, row) for shard, (_, _, _, found) in enumerate(loaded)
                    for path, report in [found[key]] for row in report['Results']]
        merged[key] = _ordered_results(first['OriginalSelected'], combined)
    require(not output.exists(), 'Output must be a new directory')
    output.mkdir(parents=True)
    write_json(output / 'expected-executables.json', expected)
    shutil.copyfile(first_root / 'compiler-artifacts.jsonl', output / 'compiler-artifacts.jsonl')
    shutil.copytree(first_root / 'preflight', output / 'preflight')
    for key, (old_path, first) in first_reports.items():
        directory = output / old_path.name
        directory.mkdir()
        report = {k: v for k, v in first.items() if k not in ('Shard', 'OriginalSelected', 'LogicalTarget')}
        report['Selected'] = first['OriginalSelected']
        report['Results'] = []
        for shard, path, row in merged[key]:
            row = dict(row)
            name = f'shard-{shard}-{row["Log"]}'
            shutil.copyfile(path / row['Log'], directory / name)
            row['Log'] = name
            report['Results'].append(row)
        write_json(directory / 'report.json', report)
    summary = {k: v for k, v in first_summary.items()
               if k not in ('Shard', 'RequiresShardAggregation', 'OriginalSelectedTests')}
    total = sum(len(p['Selected']) for p in original_plan.values())
    summary.update(SelectedTests=total, PassedTests=total, AggregatedShards=count,
                   ShardPlanSHA256=hashlib.sha256(json.dumps(original_plan, sort_keys=True).encode()).hexdigest())
    write_json(output / 'summary.json', summary)
    return summary


if __name__ == '__main__':
    if len(sys.argv) < 4:
        raise SystemExit('usage: sde_join_shards.py OUTPUT SHARD_DIRECTORY...')
    print(json.dumps(aggregate(sys.argv[2:], pathlib.Path(sys.argv[1])), indent=2))
