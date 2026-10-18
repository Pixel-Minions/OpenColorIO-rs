# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for scripts of Python calls on the wheel's module (card p6-skeleton, O6.2,
docs/cards/phase6.md).

- py_eval: scripts of steps (calls on PyOpenColorIO's names and on the results of earlier
  steps), each run by py_eval_steps.py in a new Python process of the oracle's environment,
  and each step's result: its value, or the exception it raised, the library's log, and which
  earlier results are the same object.

The port's environment runs the same interpreter script against the port's module, so the
two reports compare step by step. Like every oracle command, it reports what the library does
and never computes expected values.
"""

import contextlib
import json
import os
import shutil
import subprocess
import sys
import tempfile

from .commands import command
from .config_api import RequestError
from .files_api import _restore

_STEPS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "py_eval_steps.py")
_ORACLE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def environment():
    """The environment of the scripts' processes: nothing of the caller's (a context that loads
    the whole environment prints it), except SYSTEMROOT on Windows, which Python needs."""
    env = {}
    if os.name == "nt":
        env["SYSTEMROOT"] = os.environ["SYSTEMROOT"]
    return env


def _run(script, cwd):
    """One script's report, in a new process with `cwd` as its working directory."""
    child = subprocess.run([sys.executable, "-I", "-B", "-X", "utf8", _STEPS],
                           input=json.dumps(script).encode("utf-8"), capture_output=True,
                           cwd=cwd, env=environment(), timeout=1800, check=False)
    if child.returncode == 0:
        try:
            report = json.loads(child.stdout.decode("utf-8"))
        except ValueError:
            pass
        else:
            if "request_error" in report:
                raise RequestError(f"py_eval: {report['request_error']}")
            return report
    return {"crashed": {"returncode": child.returncode,
                        "stderr": {"bytes": child.stderr.hex()}}}


@command
def py_eval(args, blobs):
    """Scripts of Python steps on the wheel's module.

    args:     {"scripts": [{"steps": [step, ...]}, ...]}: py_eval_steps.py's docstring
              describes steps, expressions and each script's report
    result:   {"scripts": [report, ...]}: each script's report from py_eval_steps.py; or, when
              its process exited with a status other than 0 (even after writing its report)
              or wrote no report, {"crashed": {"returncode", "stderr": {"bytes": hex}}} (the
              stderr may hold tracebacks with absolute paths: never a fixture). A process
              that runs longer than 30 minutes raises (subprocess.TimeoutExpired), and the
              request fails rather than return a result.
    blobs:    none

    Each script runs in a new process (`python -I -B -X utf8`), so that nothing a script
    changes in its process reaches the next one, with the environment of environment(), in
    a new directory of its own, removed afterwards. That directory is empty; or, when the
    oracle runs elsewhere than its own directory (with_files makes the directory of its files
    the working directory), it holds a copy of that directory's files as they were when the
    request began (copied once, before the first script), so that each script reaches them
    by relative paths, as given, whatever the scripts before it wrote, even through the
    absolute "$FILES" path, which names with_files' own directory. Under with_files, each
    script's own directory reads "$FILES" in its report, as with_files' does, in every
    spelling a path takes there (system separators, "/" and, in a repr, doubled
    backslashes): paths come back the same on every run.
    A request the interpreter can't read is refused.
    """
    if set(args) != {"scripts"} or not isinstance(args["scripts"], list):
        raise RequestError(f"py_eval: expects {{\"scripts\": [...]}}, got {sorted(args)}")
    files = None if os.path.samefile(os.getcwd(), _ORACLE_DIR) else os.getcwd()
    with contextlib.ExitStack() as stack:
        snapshot = None
        if files is not None:
            snapshot = stack.enter_context(tempfile.TemporaryDirectory(prefix="ocio-py-eval-"))
            shutil.copytree(files, snapshot, dirs_exist_ok=True)
        reports = []
        for script in args["scripts"]:
            with tempfile.TemporaryDirectory(prefix="ocio-py-eval-") as cwd:
                if snapshot is None:
                    reports.append(_run(script, cwd))
                    continue
                shutil.copytree(snapshot, cwd, dirs_exist_ok=True)
                report = _run(script, cwd)
                reports.append(_restore(report, _spellings([cwd, files])))
    return {"scripts": reports}, []


def _spellings(directories):
    """Each spelling of `directories` that a report may hold, longest first: as the system
    gives the path and with links resolved, and on Windows also with "/" (paths that OCIO and
    pystring join) and with doubled backslashes (the repr of a str holding the path)."""
    spellings = set()
    for directory in directories:
        for path in {directory, os.path.realpath(directory)}:
            spellings.add(path)
            if os.name == "nt":
                spellings.add(path.replace("\\", "/"))
                spellings.add(path.replace("\\", "\\\\"))
    return sorted(spellings, key=len, reverse=True)
