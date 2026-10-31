# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for the wheel's command-line tools (card phase10-tools, chunk 2).

- tool_run: runs one of the tools the wheel ships in PyOpenColorIO/bin, with given arguments,
  environment and input files, in a given working directory, and reports its stdout, stderr
  and exit code, and the files it wrote there.

The tools print absolute paths (`Working Dir:`, error messages), so the port's tool must run in
the same directory to give the same bytes: the caller names the directory, and it holds nothing
before and after a run. The directory must not exist or must be empty; the command fills it
with the request's files, runs the tool, reports, and removes it.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import os
import re
import shutil
import subprocess

import PyOpenColorIO as OCIO

from .checks import check_keys
from .child_env import allowed_environment
from .commands import command

# The longest a tool may run: the largest bake of a sweep takes seconds.
TIMEOUT_SECONDS = 600

# On Windows, exit codes from here up are NTSTATUS failure codes: the process died
# (0xC0000005, an access violation), where a tool's own exit code is 0 or 1.
_NTSTATUS_ERROR = 0xC0000000


def _program(tool):
    """The path of the wheel's `tool`, refused unless the wheel has it."""
    if not isinstance(tool, str) or not re.fullmatch(r"ocio[a-z]+", tool):
        raise ValueError(f"tool_run: tool must be a tool's name, not {tool!r}")
    name = tool + ".exe" if os.name == "nt" else tool
    path = os.path.join(os.path.dirname(OCIO.__file__), "bin", name)
    if not os.path.isfile(path):
        raise ValueError(f"tool_run: the wheel has no tool {tool!r}")
    return path


def _strings(what, value):
    """`value`, refused unless it is a list of strings."""
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise ValueError(f"tool_run: {what} must be a list of strings, not {value!r}")
    return value


def _environment(value):
    """`value`, refused unless it is {name: value} of strings."""
    if not isinstance(value, dict) or not all(isinstance(item, str) for item in value.values()):
        raise ValueError(f"tool_run: env must be an object of strings, not {value!r}")
    return value


def _inputs(value, blobs):
    """`value`, [[relative path, blob index], ...], as {relative path: bytes}. A path has "/"
    between its components and stays inside the working directory."""
    inputs = {}
    if not isinstance(value, list):
        raise ValueError(f"tool_run: files must be a list, not {value!r}")
    for item in value:
        if not (isinstance(item, list) and len(item) == 2 and isinstance(item[0], str)
                and type(item[1]) is int and 0 <= item[1] < len(blobs)):
            raise ValueError(f"tool_run: bad entry of files: {item!r}")
        parts = item[0].split("/")
        if not all(parts) or any(part in (".", "..") or "\\" in part or ":" in part
                                 for part in parts):
            raise ValueError(f"tool_run: bad path in files: {item[0]!r}")
        if item[0] in inputs:
            raise ValueError(f"tool_run: {item[0]!r} is twice in files")
        inputs[item[0]] = blobs[item[1]]
    return inputs


def _prepare(cwd, inputs):
    """Creates the working directory `cwd`, with `inputs` in it. Refuses a `cwd` that isn't an
    absolute path, or that holds anything: the command is going to remove the directory."""
    if not isinstance(cwd, str) or not os.path.isabs(cwd):
        raise ValueError(f"tool_run: cwd must be an absolute path, not {cwd!r}")
    if os.path.lexists(cwd) and (not os.path.isdir(cwd) or os.listdir(cwd)):
        raise ValueError(f"tool_run: cwd must be empty or not exist: {cwd!r}")
    os.makedirs(cwd, exist_ok=True)
    for name, contents in inputs.items():
        path = os.path.join(cwd, *name.split("/"))
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(contents)


def _written(cwd, inputs, out):
    """Every file under `cwd` that isn't an input with its bytes unchanged, sorted by relative
    path: [{"path": "a/b.txt", "contents": a blob index}], with the bytes going to `out`."""
    found = []
    for directory, _, names in os.walk(cwd):
        for name in names:
            path = os.path.join(directory, name)
            found.append((os.path.relpath(path, cwd).replace(os.sep, "/"), path))
    files = []
    for relative, path in sorted(found):
        with open(path, "rb") as f:
            contents = f.read()
        if inputs.get(relative) != contents:
            out.append(contents)
            files.append({"path": relative, "contents": len(out) - 1})
    return files


def _exit(returncode):
    """The tool's exit code, and how it died, if it died: a signal on Linux (Python reports the
    signal's negative number), an NTSTATUS failure code on Windows."""
    died = returncode < 0 or returncode >= _NTSTATUS_ERROR
    return {"exit_code": returncode, "crashed": {"returncode": returncode} if died else None}


@command
def tool_run(args, blobs):
    """Runs a tool of the wheel.

    args: {"tool": a name such as "ociocheck", "argv": [its arguments], "cwd": an absolute path,
           empty or missing, "env": {name: value}, optional, "files": [[relative path, blob
           index], ...], optional}. The tool's environment is the oracle's allowlist
           (child_env.py) plus `env`; its stdin is empty.

    Returns {"exit_code": int, "crashed": null or how it died, "timed_out": bool, "stdout":
    blob index, "stderr": blob index, "files": [{"path", "contents": blob index}]}, `files`
    being what is new or changed under `cwd`. The exit code isn't named `returncode`, so that
    an exit code of 1, what most of a sweep gives, is cached; a tool that died, or that timed
    out, reports `crashed` and isn't (ocio_testkit oracle.rs, `cacheable`).
    """
    check_keys("tool_run", args, {"tool", "argv", "cwd", "env", "files"})
    program = _program(args.get("tool"))
    argv = _strings("argv", args.get("argv"))
    cwd = args.get("cwd")
    env = _environment(args.get("env", {}))
    inputs = _inputs(args.get("files", []), blobs)
    _prepare(cwd, inputs)
    try:
        try:
            run = subprocess.run([program] + argv, cwd=cwd, env=allowed_environment(env),
                                 stdin=subprocess.DEVNULL, capture_output=True,
                                 timeout=TIMEOUT_SECONDS, check=False)
        except subprocess.TimeoutExpired as error:
            out = [error.stdout or b"", error.stderr or b""]
            return {"exit_code": None, "crashed": {"timeout_seconds": TIMEOUT_SECONDS},
                    "timed_out": True, "stdout": 0, "stderr": 1, "files": []}, out
        out = [run.stdout, run.stderr]
        files = _written(cwd, inputs, out)
        result = {**_exit(run.returncode), "timed_out": False, "stdout": 0, "stderr": 1,
                  "files": files}
        return result, out
    finally:
        shutil.rmtree(cwd)
