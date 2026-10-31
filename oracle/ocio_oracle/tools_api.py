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

from .checks import check_bool, check_keys
from .child_env import allowed_environment
from .commands import command

# The longest a tool may run, unless the request says otherwise: the largest bake of a sweep
# takes seconds.
TIMEOUT_SECONDS = 600

# On Windows, exit codes from here up are NTSTATUS failure codes: the process died
# (0xC0000005, an access violation), where a tool's own exit code is 0 or 1.
_NTSTATUS_ERROR = 0xC0000000

# `tool | head -c n`, its arguments being n and the tool's command line.
_HEAD_LINE = '"${@:2}" | head -c "$1" > /dev/null; exit "${PIPESTATUS[0]}"'

# SetFileAttributesW's values.
_ATTRIBUTES = {"r": 0x1, "h": 0x2, "s": 0x4}
_FILE_ATTRIBUTE_NORMAL = 0x80


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


def _timeout(value):
    """`value`, refused unless it is a positive number of seconds."""
    if type(value) not in (int, float) or value <= 0:
        raise ValueError(f"tool_run: timeout_seconds must be positive, not {value!r}")
    return value


def _attributes(value, inputs):
    """`value`, [[relative path, letters], ...], checked: an input file, and letters of "hsr"
    (hidden, system, read-only). Windows only: other systems have no such attributes."""
    if not isinstance(value, list) or (value and os.name != "nt"):
        raise ValueError(f"tool_run: attributes must be a list, on Windows: {value!r}")
    for item in value:
        if not (isinstance(item, list) and len(item) == 2 and item[0] in inputs
                and isinstance(item[1], str) and item[1] and set(item[1]) <= set("hsr")):
            raise ValueError(f"tool_run: bad entry of attributes: {item!r}")
    return value


def _set_attributes(cwd, attributes):
    """Gives input files their attributes, which the tools' file opening looks at."""
    import ctypes
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.SetFileAttributesW.argtypes = [ctypes.c_wchar_p, ctypes.c_ulong]
    for name, letters in attributes:
        value = sum(_ATTRIBUTES[letter] for letter in letters)
        if not kernel32.SetFileAttributesW(os.path.join(cwd, *name.split("/")), value):
            raise ctypes.WinError(ctypes.get_last_error())


def _clear_attributes(cwd, attributes):
    """Takes attributes off again: a read-only file can't be removed."""
    if attributes:
        import ctypes
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.SetFileAttributesW.argtypes = [ctypes.c_wchar_p, ctypes.c_ulong]
        for name, _ in attributes:
            kernel32.SetFileAttributesW(os.path.join(cwd, *name.split("/")), _FILE_ATTRIBUTE_NORMAL)


def _pipe_keys(args):
    """The keys of how a tool meets a pipe: `closed_pipe`, `sigpipe_ignored`, `head_bytes`,
    checked. Linux only: Windows has no SIGPIPE."""
    closed = args.get("closed_pipe")
    ignored = check_bool("tool_run: sigpipe_ignored", args.get("sigpipe_ignored", False))
    head = args.get("head_bytes")
    if closed is None and head is None and not ignored:
        return closed, ignored, head
    if os.name == "nt":
        raise ValueError("tool_run: closed_pipe, sigpipe_ignored and head_bytes are Linux's")
    if closed not in (None, "stdout", "stderr"):
        raise ValueError(f"tool_run: closed_pipe must be stdout or stderr, not {closed!r}")
    if head is not None and (type(head) is not int or head <= 0):
        raise ValueError(f"tool_run: head_bytes must be positive, not {head!r}")
    if (head is not None or closed) and args.get("merge_stderr"):
        raise ValueError("tool_run: merge_stderr goes with neither closed_pipe nor head_bytes")
    if head is not None and (closed or ignored):
        raise ValueError("tool_run: head_bytes goes with neither closed_pipe nor sigpipe_ignored")
    return closed, ignored, head


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
           index], ...], optional, "attributes": [[relative path of an input, letters of
           "hsr"], ...], optional, Windows only, "merge_stderr": true to send stderr into the
           stdout pipe, so that the bytes show the order they were written in, optional,
           "timeout_seconds": a number, optional, TIMEOUT_SECONDS without it, and on Linux:
           "closed_pipe": "stdout" or "stderr", optional, that stream is a pipe whose read end
           is closed, "sigpipe_ignored": true for a tool that starts with SIGPIPE ignored,
           optional, "head_bytes": n, optional, the shell line `tool | head -c n`, whose
           exit_code is the tool's PIPESTATUS}. The tool's environment is the oracle's
           allowlist (child_env.py) plus `env`; its stdin is empty.

    Returns {"exit_code": int, "crashed": null or how it died, "timed_out": bool, "stdout":
    blob index, "stderr": blob index, "files": [{"path", "contents": blob index}]}, `files`
    being what is new or changed under `cwd`. A tool that dies, or that times out, reports
    `crashed`, and such a response is never cached (ocio_testkit oracle.rs, `cacheable`).
    """
    check_keys("tool_run", args, {"tool", "argv", "cwd", "env", "files", "attributes",
                                  "merge_stderr", "timeout_seconds", "closed_pipe",
                                  "sigpipe_ignored", "head_bytes"})
    program = _program(args.get("tool"))
    argv = _strings("argv", args.get("argv"))
    cwd = args.get("cwd")
    env = _environment(args.get("env", {}))
    inputs = _inputs(args.get("files", []), blobs)
    attributes = _attributes(args.get("attributes", []), inputs)
    merge_stderr = check_bool("tool_run: merge_stderr", args.get("merge_stderr", False))
    timeout = _timeout(args.get("timeout_seconds", TIMEOUT_SECONDS))
    closed, ignored, head = _pipe_keys(args)
    _prepare(cwd, inputs)
    try:
        if attributes:
            _set_attributes(cwd, attributes)
        try:
            streams = {"stdout": subprocess.PIPE,
                       "stderr": subprocess.STDOUT if merge_stderr else subprocess.PIPE}
            command = [program] + argv
            if head is not None:
                command = ["/bin/bash", "-c", _HEAD_LINE, "bash", str(head)] + command
            if closed:
                read_end, streams[closed] = os.pipe()
                os.close(read_end)
            try:
                # Python ignores SIGPIPE: the tool inherits that, or `restore_signals` gives it
                # the default.
                run = subprocess.run(command, cwd=cwd, env=allowed_environment(env),
                                     stdin=subprocess.DEVNULL, timeout=timeout, check=False,
                                     restore_signals=not ignored, **streams)
            finally:
                if closed:
                    os.close(streams[closed])
        except subprocess.TimeoutExpired as error:
            out = [error.stdout or b"", error.stderr or b""]
            return {"exit_code": None, "crashed": {"timeout_seconds": timeout},
                    "timed_out": True, "stdout": 0, "stderr": 1, "files": []}, out
        out = [run.stdout or b"", run.stderr or b""]
        files = _written(cwd, inputs, out)
        result = {**_exit(run.returncode), "timed_out": False, "stdout": 0, "stderr": 1,
                  "files": files}
        return result, out
    finally:
        _clear_attributes(cwd, attributes)
        shutil.rmtree(cwd)
