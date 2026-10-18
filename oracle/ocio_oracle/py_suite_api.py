# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for upstream's Python suite against the wheel (card p6-skeleton, O6.3,
docs/cards/phase6.md).

- py_suite: one module of upstream's tests/python, run unmodified in a new Python process of
  the oracle's environment by py_suite_runner.py, and each test's outcome and message.

The port's environment runs the same runner against the port's module (6.1b), so the two
lists compare test by test: this one is the baseline, per platform. Like every oracle
command, it reports what the library does and never computes expected values.
"""

import json
import os
import subprocess
import sys
import tempfile

from .commands import command
from .config_api import RequestError

_RUNNER = os.path.join(os.path.dirname(os.path.abspath(__file__)), "py_suite_runner.py")


def _text(s):
    # A lone surrogate (a message made from undecodable bytes) keeps its code unit.
    return {"bytes": s.encode("utf-8", "surrogatepass").hex()}


@command
def py_suite(args, blobs):
    """One module of upstream's Python suite, run against the wheel.

    args:     {"tests": upstream's tests directory (upstream/OpenColorIO/tests), "module": a
              module of its python directory, such as "ConfigTest"}
    result:   the runner's report (py_suite_runner.py): {"module", "numpy", "import_error",
              "tests": [{"id", "outcome", "message"}, ...]}, with each message as
              {"bytes": hex} of its UTF-8; or, when the process exited with a status other
              than 0 (even after writing its report) or wrote no report,
              {"crashed": {"returncode", "stderr": {"bytes": hex}}}; a process that runs
              longer than 30 minutes raises (subprocess.TimeoutExpired), and the request
              fails rather than return a result
    blobs:    none

    The module runs in a new process (`python -I -B -X utf8`), as OpenColorIOTestSuite.py
    would run it alone: TEST_DATAFILES_DIR is tests/data/files, and tests/python is first on
    sys.path. Its working directory is a new empty directory, and TEMP, TMP and TMPDIR name
    another; both are removed after the run, so a test that writes a relative path or a
    temporary file leaves nothing behind. Its environment holds only those three and, on
    Windows, SYSTEMROOT: nothing of the caller's reaches the library (a context that loads the
    whole environment prints it).
    """
    if set(args) != {"tests", "module"}:
        raise RequestError(f"py_suite: expects tests and module, got {sorted(args)}")
    tests, module = args["tests"], args["module"]
    if not isinstance(tests, str) or not isinstance(module, str):
        raise RequestError("py_suite: tests and module are strings")
    python_dir = os.path.join(tests, "python")
    if not module.isidentifier() or not os.path.isfile(os.path.join(python_dir, module + ".py")):
        raise RequestError(f"py_suite: no module {module!r} in {python_dir}")
    data_dir = os.path.join(tests, "data", "files")
    with tempfile.TemporaryDirectory(prefix="ocio-py-suite-") as root:
        cwd, tmp = os.path.join(root, "cwd"), os.path.join(root, "tmp")
        os.mkdir(cwd)
        os.mkdir(tmp)
        env = {"TEMP": tmp, "TMP": tmp, "TMPDIR": tmp}
        if os.name == "nt":
            env["SYSTEMROOT"] = os.environ["SYSTEMROOT"]
        child = subprocess.run(
            [sys.executable, "-I", "-B", "-X", "utf8", _RUNNER, python_dir, module, data_dir],
            capture_output=True, cwd=cwd, env=env, timeout=1800, check=False)
    if child.returncode == 0:
        try:
            report = json.loads(child.stdout.decode("utf-8"))
        except ValueError:
            pass
        else:
            for test in report["tests"]:
                if test["message"] is not None:
                    test["message"] = _text(test["message"])
            if report["import_error"] is not None:
                report["import_error"]["message"] = _text(report["import_error"]["message"])
            return report, []
    return {"crashed": {"returncode": child.returncode,
                        "stderr": {"bytes": child.stderr.hex()}}}, []
