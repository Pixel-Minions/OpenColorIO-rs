# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for the writers of CLF, CTF, CDL, CC and CCC (card p4-oracle, chunk O4.3).

- write_transform: GroupTransform.write(formatName, config) of a transform, or of a
  processor's createGroupTransform() (optimized or not): the text it returns, and the bytes
  of the file the other overload writes.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import os
import tempfile

import PyOpenColorIO as OCIO

from . import spec
from .checks import check_keys
from .commands import RAISED, _processor, captured_log, command, exception_result, wheel_raised
from .config_api import RequestError, text

PROCESSOR_KEYS = {"config", "transform", "src", "dst", "direction", "optimization"}

# The FormatMetadata methods "metadata" may call on the group's metadata before it is written.
METADATA_METHODS = {"__setitem__", "addChildElement", "setElementName", "setElementValue",
                    "setName", "setID", "clear"}


def _group(args, stage, blobs):
    """The GroupTransform to write: the "group" spec's, wrapped in a new GroupTransform when
    it isn't one; or the processor's createGroupTransform(), of getOptimizedProcessor(flags)
    when "optimization" is given. `stage[0]` names the step in progress."""
    if "group" in args:
        stage[0] = "transform"
        transform = spec.transform(args["group"], blobs)
        if isinstance(transform, OCIO.GroupTransform):
            return transform
        group = OCIO.GroupTransform()
        group.appendTransform(transform)
        return group
    _, processor = _processor({k: v for k, v in args["processor"].items()
                               if k != "optimization"}, stage, blobs)
    if "optimization" in args["processor"]:
        stage[0] = "optimize"
        processor = processor.getOptimizedProcessor(spec.flags(args["processor"]["optimization"]))
    stage[0] = "group"
    return processor.createGroupTransform()


def _written_text(group, format_name, config):
    """write(formatName, config): {"bytes": hex} of the text, or {"undecodable": hex} where the
    binding couldn't decode it as UTF-8."""
    try:
        return {"bytes": group.write(format_name, config).encode("utf-8").hex()}
    except UnicodeDecodeError as exc:
        return {"undecodable": bytes(exc.object).hex()}


def _written_file(group, format_name, config):
    """write(formatName, fileName, config) into a new temporary file (a std::ofstream in text
    mode): the file's bytes, hex."""
    with tempfile.TemporaryDirectory() as root:
        path = os.path.join(root, "written")
        group.write(format_name, path, config)
        with open(path, "rb") as f:
            return f.read().hex()


@command
def write_transform(args, blobs):
    """Writes a transform in a format through GroupTransform.write.

    args:
      group       a transform spec (spec.py): written as is if it is a GroupTransform, else as
                  the one transform of a new GroupTransform
      processor   or: {config, transform or src/dst, direction} as cpu_apply takes them, and
                  "optimization" (flags, spec.flags): its createGroupTransform(), of
                  getOptimizedProcessor(flags) when "optimization" is given
      config      the config write takes (spec.config; default the raw config). With
                  "processor" the processor's config is the one given there
      format      the format's name (text or {"bytes": hex}), as GroupTransform.GetWriteFormats
                  lists them
      metadata    optional: [[method, arg, ...], ...], FormatMetadata methods (__setitem__,
                  which is the binding's addAttribute, addChildElement, setElementName,
                  setElementValue, setName, setID, clear) called in order on
                  the group's getFormatMetadata() before it is written; string arguments are
                  text or {"bytes": hex}
    request blobs: the transform spec's blobs (see spec.py)
    result:
      text        write(format, config): {"bytes": hex} of the text it returns, or
                  {"undecodable": hex} when it isn't UTF-8
      file        write(format, fileName, config): the bytes of the file it writes, hex (a
                  std::ofstream in text mode: on Windows each LF is written as CR LF)
      exception, stage   when OCIO or the binding raised: {"type", "message"}, and where:
                  "config", "transform", "processor", "optimize", "group", "metadata" or
                  "write"
      log         OCIO's log messages
    blobs: none

    An unknown key, both or neither of group and processor, or a format that isn't text or
    {"bytes": hex}, is refused.
    """
    check_keys("write_transform", args, {"group", "processor", "config", "format", "metadata"})
    if ("group" in args) == ("processor" in args):
        raise RequestError("write_transform takes a group or a processor, not both or neither")
    if "processor" in args:
        check_keys("write_transform: processor", args["processor"], PROCESSOR_KEYS)
    format_name = text("format", args.get("format"))
    metadata = args.get("metadata", [])
    if not isinstance(metadata, list) or not all(
            isinstance(m, list) and m and m[0] in METADATA_METHODS for m in metadata):
        raise RequestError(f"metadata must be a list of [method, args...] with a method of "
                           f"{sorted(METADATA_METHODS)}, not {metadata!r}")
    stage, result = ["config"], {}
    with captured_log() as log:
        try:
            config = spec.config(args.get("config"))
            group = _group(args, stage, blobs)
            stage[0] = "metadata"
            for method, *values in metadata:
                getattr(group.getFormatMetadata(), method)(
                    *[text(f"metadata {method}", v) if isinstance(v, (str, dict)) else v
                      for v in values])
            stage[0] = "write"
            result["text"] = _written_text(group, format_name, config)
            result["file"] = _written_file(group, format_name, config)
        except RAISED as exc:
            if not wheel_raised(exc):
                raise
            result = {"exception": exception_result(exc), "stage": stage[0]}
    result["log"] = log
    return result, []
