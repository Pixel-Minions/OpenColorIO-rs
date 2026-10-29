# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Diagnostic only. Observe thread-limit triggers without guessing a fix."""
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess

out = Path("kept/sde-thread-probe")
out.mkdir(parents=True, exist_ok=True)
windows = os.name == "nt"
variable = "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER" if windows else "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER"
runner = shlex.split(os.environ[variable])
assert runner[-1] == "--", runner
binary = str(Path("target/sde-thread-churn.exe" if windows else "target/sde-thread-churn").resolve())
subprocess.run(["rustc", "-O", "scripts/diagnostics/sde-thread-churn.rs", "-o", binary], check=True)
kit = Path(runner[0]).parent
(out / "pin-files.json").write_text(json.dumps([str(p.relative_to(kit)) for p in kit.rglob("*pin*") if p.is_file()], indent=2), encoding="utf-8")
with (out / "top-help.txt").open("w", encoding="utf-8") as log:
    subprocess.run([runner[0], "-help"], stdout=log, stderr=subprocess.STDOUT, timeout=30, check=False)


def run(name, command, count, mode):
    timed_out = False
    with (out / (name + ".log")).open("w", encoding="utf-8") as log:
        process = subprocess.Popen(command + [binary, str(count), mode], stdout=log, stderr=subprocess.STDOUT, start_new_session=not windows)
        try:
            code = process.wait(timeout=90)
        except subprocess.TimeoutExpired:
            timed_out = True
            if windows:
                subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"], check=False, stdout=log, stderr=subprocess.STDOUT)
            else:
                os.killpg(process.pid, signal.SIGKILL)
            code = process.wait(timeout=15)
    text = (out / (name + ".log")).read_text(encoding="utf-8", errors="replace")
    return {"code": code, "timed_out": timed_out, "completed": f"COMPLETE count={count} mode={mode}" in text, "thread_limit_error": "maxmimal number of threads" in text, "count": count, "mode": mode}


report = {"fix_proposed": False, "profiling_bbthreads_is_not_an_emulation_limit": True}
for mode, count in (("sequential", 8192), ("concurrent", 256)):
    native = run("native-" + mode, [], count, mode)
    baseline = run("emulated-" + mode, runner, count, mode)
    report[mode] = {"native": native, "emulated": baseline}
    (out / "thread-trigger-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    assert native["code"] == 0 and native["completed"] and not native["timed_out"]
    assert baseline["thread_limit_error"] or (baseline["code"] == 0 and baseline["completed"] and not baseline["timed_out"]), baseline
print(json.dumps(report, indent=2), flush=True)
