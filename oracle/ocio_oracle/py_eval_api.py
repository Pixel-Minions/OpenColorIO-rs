# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for scripts of Python calls on the wheel's module (card p6-skeleton, O6.2,
docs/cards/phase6.md).

- py_eval: scripts of steps (calls on PyOpenColorIO's names and on the results of earlier
  steps), run by py_eval_steps.py in a new Python process of the oracle's environment, and
  each step's result: its value, or the exception it raised, the library's log, and which
  earlier results are the same object.

The port's environment runs the same interpreter script against the port's module, so the
two reports compare step by step. Like every oracle command, it reports what the library does
and never computes expected values.
"""

import json
import os
import subprocess
import sys

from .commands import command
from .config_api import RequestError

_STEPS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "py_eval_steps.py")


def environment():
    """The environment of the scripts' process: nothing of the caller's (a context that loads
    the whole environment prints it), except SYSTEMROOT on Windows, which Python needs."""
    env = {}
    if os.name == "nt":
        env["SYSTEMROOT"] = os.environ["SYSTEMROOT"]
    return env


@command
def py_eval(args, blobs):
    """Scripts of Python steps on the wheel's module.

    args:     {"scripts": [{"steps": [step, ...]}, ...]}, the request of py_eval_steps.py,
              whose docstring describes steps, expressions and the report
    result:   py_eval_steps.py's report, {"scripts": [{"steps": [result, ...]}, ...]}; or,
              when the process died, {"crashed": {"returncode", "stderr": {"bytes": hex}}}
    blobs:    none

    The scripts run one after another in one new process (`python -I -B -X utf8`), in the
    oracle's working directory (with_files's, when it runs this command), with the
    environment of environment(). A request the interpreter can't read is refused.
    """
    child = subprocess.run([sys.executable, "-I", "-B", "-X", "utf8", _STEPS],
                           input=json.dumps(args).encode("utf-8"), capture_output=True,
                           env=environment(), timeout=1800, check=False)
    if child.returncode == 0:
        try:
            report = json.loads(child.stdout.decode("utf-8"))
        except ValueError:
            pass
        else:
            if "request_error" in report:
                raise RequestError(f"py_eval: {report['request_error']}")
            return report, []
    return {"crashed": {"returncode": child.returncode,
                        "stderr": {"bytes": child.stderr.hex()}}}, []
