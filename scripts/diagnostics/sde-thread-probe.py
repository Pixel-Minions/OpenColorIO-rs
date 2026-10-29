# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Diagnostic only. Bound every emulator process; preserve default failure as a control."""
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


def run(name, command, count, seconds):
    timed_out = False
    with (out / (name + ".log")).open("w", encoding="utf-8") as log:
        process = subprocess.Popen(command + [binary, str(count)], stdout=log, stderr=subprocess.STDOUT, start_new_session=not windows)
        try:
            code = process.wait(timeout=seconds)
        except subprocess.TimeoutExpired:
            timed_out = True
            if windows:
                subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"], check=False, stdout=log, stderr=subprocess.STDOUT)
            else:
                os.killpg(process.pid, signal.SIGKILL)
            code = process.wait(timeout=15)
    text = (out / (name + ".log")).read_text(encoding="utf-8", errors="replace")
    return {"code": code, "timed_out": timed_out, "completed": f"COMPLETE count={count} sequential-workers=1" in text, "thread_limit_error": "maxmimal number of threads" in text, "count": count}


native = run("native", [], 2048, 90)
baseline = run("default-128", runner, 384, 90)
fixed = run("documented-4096", runner[:-1] + ["-bbthreads", "4096", "--"], 2048, 90)
report = {"native": native, "baseline": baseline, "documented_4096": fixed, "max_concurrent_workers": 1}
(out / "thread-control-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
print(json.dumps(report, indent=2), flush=True)
assert native["code"] == 0 and native["completed"] and not native["timed_out"]
assert baseline["thread_limit_error"] and not baseline["completed"]
assert fixed["code"] == 0 and fixed["completed"] and not fixed["timed_out"] and not fixed["thread_limit_error"]
