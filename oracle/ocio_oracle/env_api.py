# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for OCIO's environment functions (card p3-context, after its review).

- context_env_calls: context_calls (context_api.py), run in a new Python process of its own,
  where the calls may also reach OCIO's SetEnvVariable and UnsetEnvVariable on "OCIO".

Those two change the process's environment (the C runtime's and the system's), which no
request of the oracle's own process may do: the commands after it would see the change.
A new process (`python -I`, started with an empty environment) runs the request and ends, so
nothing needs restoring. It also reports a crash instead of ending the oracle: the Windows
wheel's SetEnvVariable with a value of 32,767 UTF-16 units or more stops the process
(_wputenv_s's parameter validation).

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import json
import os
import subprocess
import sys

from .commands import command
from .config_api import RequestError

# Runs a request in a new process: reads the request from stdin, writes the result to stdout.
_CHILD = r"""
import json, os, sys
request = json.loads(sys.stdin.read())
sys.path.insert(0, request["oracle_path"])
from ocio_oracle import commands  # noqa: F401 - loads every module, as the oracle does
from ocio_oracle import config_api, context_api
# This process ends after the request, so the calls may change its environment.
config_api.MODULE_FUNCTIONS.update({"SetEnvVariable", "UnsetEnvVariable"})
try:
    result, _ = context_api.context_calls(request["args"], [])
except config_api.RequestError as exc:
    result = {"request_error": str(exc)}
else:
    result["pid"] = os.getpid()
sys.stdout.write(json.dumps(result))
"""


@command
def context_env_calls(args, blobs):
    """context_calls in a new process, where SetEnvVariable and UnsetEnvVariable may be called.

    args:     as context_calls (context_api.py); a call {"call": "SetEnvVariable", "on":
              "OCIO", "args": [name, value]} or {"call": "UnsetEnvVariable", "on": "OCIO",
              "args": [name]} changes the process's environment for the calls after it, which
              GetEnvVariable, IsEnvVariablePresent and a context's loadEnvironment then read
    result:   as context_calls, plus "pid": the process that ran the request; or, when that
              process died, {"crashed": {"returncode", "stderr": {"bytes": hex}}}
    blobs:    none

    The new process starts with an empty environment (the Python launcher adds its own
    variables, which the request's environment replaces, as context_calls does).
    """
    oracle_path = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    request = {"oracle_path": oracle_path, "args": args}
    child = subprocess.run([sys.executable, "-I", "-c", _CHILD],
                           input=json.dumps(request).encode("utf-8"), capture_output=True,
                           env={}, timeout=600, check=False)
    if child.returncode == 0:
        try:
            result = json.loads(child.stdout.decode("utf-8"))
        except ValueError:
            pass
        else:
            if "request_error" in result:
                raise RequestError(result["request_error"])
            return result, []
    return {"crashed": {"returncode": child.returncode,
                        "stderr": {"bytes": child.stderr.hex()}}}, []
