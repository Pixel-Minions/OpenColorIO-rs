# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
# Diagnostic only; NEVER MERGE. No test bodies, thresholds or production flags change.
import hashlib
import json
import os
import pathlib
import re
import shlex
import signal
import subprocess
import threading
import time

out = pathlib.Path("kept/sde-fresh-process-trial")
out.mkdir(parents=True, exist_ok=True)
windows = os.name == "nt"
variable = (
    "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER"
    if windows
    else "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER"
)
runner = shlex.split(os.environ[variable])
assert runner[-1] == "--", runner
artifacts = [
    json.loads(line)
    for line in (out / "compiler-artifacts.jsonl").read_text().splitlines()
]
binaries = [
    item["executable"]
    for item in artifacts
    if item.get("reason") == "compiler-artifact"
    and item["target"]["name"] == "ocio"
    and item["target"]["kind"] == ["lib"]
    and item["profile"]["test"]
    and item.get("executable")
]
assert len(binaries) == 1, binaries
binary = pathlib.Path(binaries[0]).resolve()
assert binary.is_file(), binary
listed = subprocess.run(
    [str(binary), "--list", "--format", "terse"],
    check=True,
    capture_output=True,
    text=True,
    timeout=120,
)
(out / "native-test-list.txt").write_text(listed.stdout)
lines = [line for line in listed.stdout.splitlines() if line]
assert all(line.endswith(": test") for line in lines), lines[-10:]
names = [line.removesuffix(": test") for line in lines]
# Count observed in the unchanged, previously reproduced main2f workload.
assert len(names) == len(set(names)) == 1280, len(names)
first = "apphelpers::mergeconfigs::section_merger::oracle_tests::whole_merges_match_the_wheel"
assert first in names
plan = [first] + [name for name in names if name != first]
assert len(plan) == len(names) and set(plan) == set(names)
(out / "execution-plan.json").write_text(json.dumps(plan, indent=2) + "\n")
report = dict(
    DiagnosticOnly=True,
    WorkloadSource="2f768f6fb69cc53e040f8c3d1d2ea0df1d6dfc0c",
    Runner=runner,
    NativeSelectedTests=len(names),
    NativeTestListSHA256=hashlib.sha256(listed.stdout.encode()).hexdigest(),
    ExactlyOncePlan=True,
    FirstCase=first,
    Results=[],
    All1280PassedExactlyOnce=False,
    ProductionSDECoverageUnchanged=True,
)


def save():
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")


def run_case(index, name):
    command = runner + [str(binary), "--exact", name, "--nocapture", "--test-threads=1"]
    path = out / f"test-{index:04}.log"
    started = time.monotonic()
    error = threading.Event()
    timed_out = False
    process = subprocess.Popen(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        start_new_session=not windows,
    )

    def consume():
        with path.open("w", encoding="utf-8") as log:
            for line in process.stdout:
                log.write(line)
                log.flush()
                if "SDE ERROR" in line or "maxmimal number of threads" in line:
                    error.set()

    reader = threading.Thread(target=consume, daemon=True)
    reader.start()

    def kill_owned():
        if windows:
            subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
        else:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

    try:
        while process.poll() is None or reader.is_alive():
            if error.is_set() or time.monotonic() - started > 900:
                timed_out = not error.is_set()
                kill_owned()
                break
            time.sleep(0.1)
        code = process.wait(timeout=30)
        reader.join(timeout=10)
    finally:
        if process.poll() is None:
            kill_owned()
    text = path.read_text(errors="replace")
    passed = (
        code == 0
        and not error.is_set()
        and not timed_out
        and not reader.is_alive()
        and "running 1 test" in text
        and f"test {name} ..." in text
        and re.search(
            r"^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; 1279 filtered out;",
            text,
            re.MULTILINE,
        )
        is not None
    )
    return dict(
        Index=index,
        Name=name,
        Passed=passed,
        ProcessExit=code,
        EmulatorError=error.is_set(),
        TimedOut=timed_out,
        ElapsedSeconds=round(time.monotonic() - started, 2),
        Log=path.name,
        LogSHA256=hashlib.sha256(path.read_bytes()).hexdigest(),
    )


save()
for index, name in enumerate(plan):
    print(f"START {index + 1}/{len(plan)} {name}", flush=True)
    result = run_case(index, name)
    report["Results"].append(result)
    save()
    print(json.dumps(result), flush=True)
    if not result["Passed"]:
        raise SystemExit(86 if result["EmulatorError"] else 124 if result["TimedOut"] else 1)
observed = [item["Name"] for item in report["Results"]]
assert len(observed) == len(set(observed)) == len(names)
assert set(observed) == set(names)
report["All1280PassedExactlyOnce"] = True
save()
print("PASS every original library test exactly once; production SDE unchanged", flush=True)
