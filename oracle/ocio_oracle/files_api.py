# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command that runs another command next to given files (card p4-oracle, chunk O4.2).

- with_files: writes files into a new temporary directory, makes it the working directory,
  runs another oracle command there with "$FILES" in its arguments standing for the
  directory's absolute path, and reports that command's result with the directory's path
  written back as "$FILES".

The commands that read LUT files (processor_ops, cpu_apply, image_apply, gpu_shader,
config_calls, transform_text, ...) then need no change: a FileTransform spec names
"$FILES/lut.spi1d", and the result names it the same way on every machine.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import os

import PyOpenColorIO as OCIO

from .checks import check_keys
from .commands import COMMANDS, command
from .config_api import RequestError, directory, text


def _substitute(value, root):
    """`value` with "$FILES" replaced by `root` in every string, at any depth (the keys of
    objects too). {"bytes": hex} values are text to the commands, so they are replaced in their
    bytes."""
    if isinstance(value, str):
        return value.replace("$FILES", root)
    if isinstance(value, list):
        return [_substitute(v, root) for v in value]
    if isinstance(value, dict):
        if set(value) == {"bytes"} and isinstance(value["bytes"], str):
            data = text("a {\"bytes\": hex} value", value)
            return {"bytes": data.replace(b"$FILES", root.encode("utf-8")).hex()}
        return {_substitute(k, root): _substitute(v, root) for k, v in value.items()}
    return value


def _roots(root):
    """The spellings of `root` a result may hold: as the system gives it, and on Windows with
    forward slashes too (paths joined by OCIO and pystring mix them). Longest first."""
    spellings = {root}
    if os.name == "nt":
        spellings.add(root.replace("\\", "/"))
    return sorted(spellings, key=len, reverse=True)


def _restore(value, roots):
    """`value` with each spelling of the directory's path written back as "$FILES", in strings
    and in the bytes of {"bytes": hex} values (which the commands use for strings), at any
    depth."""
    if isinstance(value, str):
        for root in roots:
            value = value.replace(root, "$FILES")
        return value
    if isinstance(value, list):
        return [_restore(v, roots) for v in value]
    if isinstance(value, dict):
        if set(value) <= {"bytes", "undecodable"} and len(value) == 1:
            key = next(iter(value))
            if isinstance(value[key], str):
                try:
                    data = bytes.fromhex(value[key])
                except ValueError:
                    return value
                for root in roots:
                    data = data.replace(root.encode("utf-8"), b"$FILES")
                return {key: data.hex()}
        return {_restore(k, roots): _restore(v, roots) for k, v in value.items()}
    return value


@command
def with_files(args, blobs):
    """Runs a command in a new temporary directory holding given files.

    args:
      files       {relative path: content}: text (written as UTF-8), {"bytes": hex}, or
                  {"blob": i}, the request's blob i (i < file_blobs); paths use "/" and have no
                  ".", ".." or empty parts
      file_blobs  optional: how many of the request's blobs are files' (default 0); the others
                  are the command's, in order
      command     the oracle command to run (not with_files or batch)
      args        its arguments, in which every "$FILES" (in strings, object keys and the bytes
                  of {"bytes": hex} values) becomes the directory's absolute path
    result:
      the command's result, with the directory's path (also with "/" for "\\" on Windows)
      written back as "$FILES" in strings and in the bytes of {"bytes": hex} and
      {"undecodable": hex} values
    blobs: the command's

    The directory is the working directory while the command runs, and is deleted after.
    OCIO's caches are cleared before the command runs (ClearAllCaches), as in a new process.
    An unknown key, an unknown command, a file path or content it can't read, or a blob index
    out of range, is refused.
    """
    check_keys("with_files", args, {"files", "file_blobs", "command", "args"})
    name = args.get("command")
    if name in ("with_files", "batch") or name not in COMMANDS:
        raise RequestError(f"with_files can't run the command {name!r}")
    file_blobs = args.get("file_blobs", 0)
    if isinstance(file_blobs, bool) or not isinstance(file_blobs, int) \
            or not 0 <= file_blobs <= len(blobs):
        raise RequestError(f"file_blobs must be a count of the request's {len(blobs)} blobs, "
                           f"not {file_blobs!r}")
    files = args.get("files", {})
    if not isinstance(files, dict):
        raise RequestError(f"files must be an object, not {files!r}")
    contents = {}
    for path, content in files.items():
        if isinstance(content, dict) and set(content) == {"blob"}:
            index = content["blob"]
            if isinstance(index, bool) or not isinstance(index, int) \
                    or not 0 <= index < file_blobs:
                raise RequestError(f"files[{path!r}]: blob {index!r} isn't one of the "
                                   f"{file_blobs} file blobs")
            content = {"bytes": blobs[index].hex()}
        contents[path] = content
    with directory(contents) as root:
        # The file caches are keyed by path, and on Windows by a hash of the path rather than
        # the file's contents: a later call that got the same temporary path could read an
        # earlier call's LUT.
        OCIO.ClearAllCaches()
        inner = _substitute(args.get("args", {}), root)
        result, out = COMMANDS[name](inner, blobs[file_blobs:])
        return _restore(result, _roots(root)), out
