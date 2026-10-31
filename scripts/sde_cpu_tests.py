# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Run the original CPU-test alias in fresh, fully inventoried emulators.

Run beneath scripts/sde.sh. Cargo chooses the original targets, environment and working
directories; its runner inventories native libtest names and emulates each selected name.
"""
import hashlib
import json
import os
import pathlib
import re
import shlex
import signal
import subprocess
import sys
import tempfile
import threading
import time

PREFIX_KEY = 'OCIO_RS_SDE_EXEC_PREFIX_JSON'
REPORT_KEY = 'OCIO_RS_SDE_EXEC_REPORT_ROOT'
SHARD_KEY = 'OCIO_RS_SDE_SHARD'

# Original quick-tier input plans in crates/ocio/tests/legacy_gpu_oracle.rs:
# 1680 / 2000 / 1680 / 1680 requests, rather than hash-only 0 / 980 / 2000 / 4060.
# Leave the previously heaviest observed bucket (2) the boundary/refusal-heavy cases.
LEGACY_FOUR_WAY = {
    'small_legacy_lattices_and_cache_order_match_native_wheel': 0,
    'legacy_allocation_cuts_and_file_boundaries_match_native_wheel': 1,
    'legacy_mixed_lut_range_and_alpha_lattices_match_native_wheel': 1,
    'legacy_large_lattice_values_match_native_wheel': 1,
    'legacy_boundary_edges_match_native_wheel': 2,
    'ordinary_after_legacy_keeps_native_cached_shader_and_texture_values': 2,
    'legacy_dynamic_positions_and_snapshot_values_match_native_wheel': 3,
}


def shard_spec(text):
    if text is None:
        return None
    if not re.fullmatch(r'\d+/\d+', text):
        raise RuntimeError('SDE shard must be zero-based INDEX/COUNT')
    index, count = map(int, text.split('/'))
    if not 1 <= count <= 16 or not 0 <= index < count:
        raise RuntimeError('SDE shard is outside the bounded 1..16 partition')
    return dict(Index=index, Count=count)


def target_key(metadata):
    package = metadata['Package'].split('#')[0].rstrip('/').rsplit('/', 1)[-1]
    target = metadata['Target']
    return package + '/' + ','.join(target['kind']) + '/' + target['name']


def selected_shard(metadata, names, shard):
    if shard is None:
        return names
    key = target_key(metadata)
    def belongs(name):
        if shard['Count'] == 4 and key == 'ocio/test/legacy_gpu_oracle' and name in LEGACY_FOUR_WAY:
            return LEGACY_FOUR_WAY[name] == shard['Index']
        digest = hashlib.sha256((key + '\0' + name).encode()).digest()
        return int.from_bytes(digest[:8], 'little') % shard['Count'] == shard['Index']
    return [name for name in names if belongs(name)]


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')


def executable_key(path):
    return os.path.normcase(str(pathlib.Path(path).resolve()))


def native_list(binary, arguments):
    result = subprocess.run([binary, *arguments, '--list', '--format', 'terse'], check=True,
                            capture_output=True, text=True, timeout=120)
    lines = [line for line in result.stdout.splitlines() if line]
    if not all(line.endswith(': test') for line in lines):
        raise RuntimeError(f'Unexpected libtest inventory: {result.stdout[-2000:]}')
    names = [line.removesuffix(': test') for line in lines]
    if len(set(names)) != len(names):
        raise RuntimeError('Duplicate native test names')
    return names


def one_test(prefix, binary, name, native_count, path):
    started = time.monotonic()
    fatal = threading.Event()
    timed_out = False
    windows = os.name == 'nt'
    process = subprocess.Popen(prefix + [binary, '--exact', name, '--nocapture', '--test-threads=1', '--color=never'],
                               stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                               encoding='utf-8', errors='replace', start_new_session=not windows)

    def read_output():
        with path.open('w', encoding='utf-8') as log:
            for line in process.stdout:
                log.write(line)
                log.flush()
                if 'SDE ERROR' in line or 'maxmimal number of threads' in line:
                    fatal.set()

    def kill_owned():
        if windows:
            subprocess.run(['taskkill', '/PID', str(process.pid), '/T', '/F'],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
        else:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

    reader = threading.Thread(target=read_output, daemon=True)
    reader.start()
    try:
        while process.poll() is None or reader.is_alive():
            if fatal.is_set() or time.monotonic() - started > 7200:
                timed_out = not fatal.is_set()
                kill_owned()
                break
            time.sleep(.1)
        code = process.wait(timeout=30)
        reader.join(timeout=10)
    finally:
        if process.poll() is None:
            kill_owned()
    text = path.read_text(encoding='utf-8', errors='replace')
    summary = rf'^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; {native_count-1} filtered out;'
    passed = code == 0 and not fatal.is_set() and not timed_out and not reader.is_alive()
    passed = passed and 'running 1 test' in text and f'test {name} ' in text and re.search(summary, text, re.M) is not None
    return dict(Name=name, Passed=passed, ExitCode=code, EmulatorError=fatal.is_set(),
                TimedOut=timed_out, ElapsedSeconds=round(time.monotonic()-started, 2),
                Log=path.name, LogSHA256=hashlib.sha256(path.read_bytes()).hexdigest())


def test_binary(binary, arguments):
    root = pathlib.Path(os.environ[REPORT_KEY])
    executable = executable_key(binary)
    expected = json.loads((root / 'expected-executables.json').read_text())
    if executable not in expected:
        raise RuntimeError(f'Cargo launched an unexpected executable: {binary}')
    directory = root / hashlib.sha256(executable.encode()).hexdigest()
    directory.mkdir()  # A duplicate Cargo invocation must not overwrite an earlier report.
    all_names = native_list(binary, [])
    original_selected = native_list(binary, arguments)
    if not set(original_selected).issubset(all_names):
        raise RuntimeError('Filtered names are not in the full native manifest')
    shard = shard_spec(os.environ.get(SHARD_KEY))
    selected = selected_shard(expected[executable], original_selected, shard)
    report = dict(Executable=executable, WorkingDirectory=os.getcwd(), CargoHarnessArgs=arguments,
                  Unfiltered=all_names, Selected=selected,
                  ExcludedByOriginalArgs=sorted(set(all_names)-set(original_selected)), Results=[],
                  AllSelectedPassed=False, ExactlyOnce=False)
    if shard is not None:
        report.update(Shard=shard, OriginalSelected=original_selected,
                      LogicalTarget=target_key(expected[executable]))
    write_json(directory / 'report.json', report)
    prefix = json.loads(os.environ[PREFIX_KEY])
    for index, name in enumerate(selected):
        print(f'SDE {index+1}/{len(selected)} {pathlib.Path(binary).name}::{name}', flush=True)
        row = one_test(prefix, binary, name, len(all_names), directory / f'test-{index:05}.log')
        report['Results'].append(row)
        write_json(directory / 'report.json', report)
        if not row['Passed']:
            print(json.dumps(row), flush=True)
    observed = [row['Name'] for row in report['Results']]
    report['ExactlyOnce'] = len(observed) == len(set(observed)) == len(selected) and set(observed) == set(selected)
    report['AllSelectedPassed'] = report['ExactlyOnce'] and all(row['Passed'] for row in report['Results'])
    write_json(directory / 'report.json', report)
    return 0 if report['AllSelectedPassed'] else 1


def cargo_command(original, config):
    return [original[0], '--config', config, *original[1:]]


def run_cargo(original):
    shard = shard_spec(os.environ.get(SHARD_KEY))
    if len(original) < 2 or original[0] != 'cargo' or original[1] != 'cpu-tests':
        raise RuntimeError('This runner accepts the original cargo cpu-tests command only')
    split = original.index('--') if '--' in original else len(original)
    build = original[:split]
    if any(arg == '--no-run' or arg.startswith('--message-format') for arg in build):
        raise RuntimeError('The runner owns native build and manifest collection')
    windows = os.name == 'nt'
    triple = 'x86_64-pc-windows-msvc' if windows else 'x86_64-unknown-linux-gnu'
    variable = 'CARGO_TARGET_' + triple.upper().replace('-', '_') + '_RUNNER'
    prefix = shlex.split(os.environ[variable])
    if not prefix or prefix[-1] != '--':
        raise RuntimeError('Run this helper beneath scripts/sde.sh')
    base = pathlib.Path('kept/sde-cpu-tests')
    base.mkdir(parents=True, exist_ok=True)
    root = pathlib.Path(tempfile.mkdtemp(prefix='run-', dir=base)).resolve()
    if not windows:
        # The Rocky container writes as root; its host must be able to export this evidence.
        root.chmod(0o755)
    build_command = build + ['--no-run', '--message-format=json']
    with (root / 'compiler-artifacts.jsonl').open('w', encoding='utf-8') as log:
        result = subprocess.run(build_command, stdout=log)
    if result.returncode:
        return result.returncode
    artifacts = [json.loads(line) for line in (root / 'compiler-artifacts.jsonl').read_text().splitlines()]
    expected = {executable_key(item['executable']): dict(Package=item['package_id'], Target=item['target'])
                for item in artifacts if item.get('reason') == 'compiler-artifact'
                and item['profile']['test'] and item.get('executable')}
    if not expected:
        raise RuntimeError('Native Cargo build produced no selected test executables')
    write_json(root / 'expected-executables.json', expected)
    environment = os.environ.copy()
    # Every job must run the real preflight audit, regardless of its production bucket.
    environment.pop(SHARD_KEY, None)
    environment[PREFIX_KEY] = json.dumps(prefix)
    runner = [sys.executable, str(pathlib.Path(__file__).resolve()), '--test-binary']
    # CLI configuration has precedence over the inherited SDE runner; the array also
    # preserves spaces in executable paths. Keep the original environment for child Cargo.
    # https://doc.rust-lang.org/cargo/reference/config.html#command-line-overrides
    config = f'target.{triple}.runner=' + json.dumps(runner, ensure_ascii=False)

    # Prove Cargo's CLI runner array overrides its inherited runner environment, using the
    # tiny real plan audit. Keep this control separate from the exactly-once production suite.
    preflight = root / 'preflight'
    preflight.mkdir()
    write_json(preflight / 'expected-executables.json', expected)
    environment[REPORT_KEY] = str(preflight)
    audit = 'apphelpers::mergeconfigs::section_merger::oracle_tests::whole_merge_parts_cover_original_cases_once'
    # Use the identical alias and feature graph so the preflight executable paths match
    # the native build manifest. All other targets record an empty selected inventory.
    check = build + ['--', audit, '--exact']
    result = subprocess.run(cargo_command(check, config), env=environment, timeout=180)
    controls = [json.loads(path.read_text()) for path in preflight.glob('*/report.json')]
    selected_controls = [control for control in controls if control['Selected']]
    if (result.returncode or len(controls) != len(expected)
            or len(selected_controls) != 1 or selected_controls[0]['Selected'] != [audit]
            or not all(control['AllSelectedPassed'] for control in controls)):
        raise RuntimeError('Cargo runner preflight failed; do not start the full suite')

    environment[REPORT_KEY] = str(root)
    if shard is not None:
        environment[SHARD_KEY] = f"{shard['Index']}/{shard['Count']}"
    result = subprocess.run(cargo_command(original, config), env=environment)
    reports = [json.loads(path.read_text()) for path in root.glob('*/report.json')]
    observed = [report['Executable'] for report in reports]
    complete = len(observed) == len(set(observed)) == len(expected) and set(observed) == set(expected)
    passed = result.returncode == 0 and complete and all(report['AllSelectedPassed'] for report in reports)
    summary = dict(CargoCommand=original, SDEPrefix=prefix, ExpectedExecutables=len(expected),
                   ObservedExecutables=len(observed), CompleteExecutableManifest=complete,
                   SelectedTests=sum(len(report['Selected']) for report in reports),
                   PassedTests=sum(row['Passed'] for report in reports for row in report['Results']),
                   ExcludedByOriginalArgs=[name for report in reports for name in report['ExcludedByOriginalArgs']],
                   AllSelectedTestsPassed=passed, CargoExit=result.returncode)
    if shard is not None:
        summary.update(Shard=shard, RequiresShardAggregation=True,
                       OriginalSelectedTests=sum(len(report['OriginalSelected']) for report in reports))
    write_json(root / 'summary.json', summary)
    print(json.dumps(summary, indent=2), flush=True)
    return 0 if passed else 1


if __name__ == '__main__':
    if len(sys.argv) >= 3 and sys.argv[1] == '--test-binary':
        raise SystemExit(test_binary(sys.argv[2], sys.argv[3:]))
    raise SystemExit(run_cargo(sys.argv[1:]))
