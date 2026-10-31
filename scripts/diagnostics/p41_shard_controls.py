# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Adversarial protocol controls over actual shard artifacts; never a CPU-coverage claim."""
import copy
import json
import pathlib
import shutil
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from sde_join_shards import aggregate
from sde_cpu_tests import selected_shard


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')


def controls(inputs, output):
    output.mkdir(parents=True, exist_ok=False)
    positive = aggregate(inputs, output / 'positive')
    # This workflow deliberately requests only the actual plan audit in the complete
    # native target graph. Full production inventories are verified separately.
    if positive['SelectedTests'] != 1 or positive['AggregatedShards'] != 4:
        raise RuntimeError('Protocol smoke is not the one-audit four-shard contract')
    source_root = next(path.parent for path in inputs[0].rglob('summary.json'))
    expected = json.loads((source_root / 'expected-executables.json').read_text())
    native_count = 0
    partition_counts = [0] * 4
    for path in source_root.glob('*/report.json'):
        report = json.loads(path.read_text())
        names = report['Unfiltered']
        partitions = [selected_shard(expected[report['Executable']], names, dict(Index=i, Count=4))
                      for i in range(4)]
        flattened = [name for part in partitions for name in part]
        if len(flattened) != len(set(flattened)) or set(flattened) != set(names):
            raise RuntimeError('Original native name partition lost or duplicated inputs')
        native_count += len(names)
        for index, part in enumerate(partitions):
            partition_counts[index] += len(part)
    if not native_count or not min(partition_counts):
        raise RuntimeError('Native input plan must have four nonempty partitions')
    rejected = []
    with tempfile.TemporaryDirectory(prefix='sde-shard-controls-') as temporary:
        root = pathlib.Path(temporary)
        copied = []
        for index, source in enumerate(inputs):
            destination = root / f'shard-{index}'
            shutil.copytree(source, destination)
            copied.append(destination)

        def rejects(directories, label):
            try:
                aggregate(directories, root / ('invalid-' + label))
            except RuntimeError:
                rejected.append(label)
                return
            raise RuntimeError('Accepted invalid proof: ' + label)

        rejects(copied[:-1], 'missing-shard')
        rejects([copied[0], copied[0], copied[2], copied[3]], 'duplicate-shard')
        owner = next(path for directory in copied for path in directory.rglob('report.json')
                     if 'preflight' not in path.parts and json.loads(path.read_text())['Results'])
        original = owner.read_bytes()
        value = json.loads(original)
        for label, mutation in [
            ('missing-result', lambda v: v.update(Results=[])),
            ('duplicate-result', lambda v: v['Results'].append(copy.deepcopy(v['Results'][0]))),
            ('changed-log-hash', lambda v: v['Results'][0].update(LogSHA256='0' * 64)),
            ('emulator-error', lambda v: v['Results'][0].update(EmulatorError=True)),
        ]:
            changed = copy.deepcopy(value)
            mutation(changed)
            write(owner, changed)
            rejects(copied, label)
            owner.write_bytes(original)
        summary = next(copied[0].rglob('summary.json'))
        original = summary.read_bytes()
        changed = json.loads(original)
        changed['SDEPrefix'] = ['different-cpu-prefix']
        write(summary, changed)
        rejects(copied, 'different-cpu-prefix')
        summary.write_bytes(original)
        preflight = next(path for path in copied[0].rglob('report.json')
                         if 'preflight' in path.parts and json.loads(path.read_text())['Results'])
        original = preflight.read_bytes()
        changed = json.loads(original)
        changed.update(Selected=[], Results=[])
        write(preflight, changed)
        rejects(copied, 'missing-live-preflight')
        preflight.write_bytes(original)
    result = dict(ProtocolOnly=True, CompleteCPUProfileClaim=False,
                  ActualAuditPassed=True, OriginalTargetCount=positive['ExpectedExecutables'],
                  FullNativeInputCount=native_count, PartitionCounts=partition_counts,
                  Rejected=rejected, AllControlsPassed=len(rejected) == 8)
    write(output / 'controls.json', result)
    return result


if __name__ == '__main__':
    if len(sys.argv) != 6:
        raise SystemExit('usage: p41_shard_controls.py OUTPUT SHARD0 SHARD1 SHARD2 SHARD3')
    print(json.dumps(controls([pathlib.Path(p) for p in sys.argv[2:]], pathlib.Path(sys.argv[1])), indent=2))
