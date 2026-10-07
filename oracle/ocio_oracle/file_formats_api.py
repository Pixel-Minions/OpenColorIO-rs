# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for the file format registry (card p4-oracle, chunk O4.1).

- file_formats: the formats the library reads (FileTransform.getFormats), bakes
  (Baker.getFormats) and writes (GroupTransform.GetWriteFormats), each list in the registry's
  order, and FileTransform.IsFormatExtensionSupported of given extensions.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import PyOpenColorIO as OCIO

from .checks import check_keys
from .commands import RAISED, command, wheel_raised
from .config_api import LogCapture, RequestError, raised, text, value_out


def _supported(extension):
    """IsFormatExtensionSupported(extension): {"result": bool}, or {"exception"} where it
    raised."""
    try:
        return {"result": OCIO.FileTransform.IsFormatExtensionSupported(extension)}
    except RAISED as exc:
        if not wheel_raised(exc):
            raise
        return {"exception": raised(exc)}


@command
def file_formats(args, blobs):
    """The file format registry, as the binding lists it, and which extensions it supports.

    args:
      extensions  optional: [extension, ...] (text or {"bytes": hex}), each given to
                  FileTransform.IsFormatExtensionSupported (a leading "." is allowed, case is
                  ignored)
    result:
      read        FileTransform.getFormats(): [[name, extension], ...] in the registry's order,
                  each string {"bytes": hex}; a format that reads several extensions is listed
                  once per extension
      bake        Baker.getFormats(), the same way
      write       GroupTransform.GetWriteFormats(), the same way
      supported   per extension given, in order: {"result": bool} or {"exception"}
      log         what OCIO logged: [{"bytes": hex}]
    blobs: none

    An unknown key, or an extension that isn't text or {"bytes": hex}, is refused.
    """
    check_keys("file_formats", args, {"extensions"})
    extensions = args.get("extensions", [])
    if not isinstance(extensions, list):
        raise RequestError(f"extensions must be a list, not {extensions!r}")
    extensions = [text(f"extensions[{i}]", e) for i, e in enumerate(extensions)]
    with LogCapture() as log:
        result = {
            "read": value_out(OCIO.FileTransform.getFormats()),
            "bake": value_out(OCIO.Baker.getFormats()),
            "write": value_out(OCIO.GroupTransform.GetWriteFormats()),
            "supported": [_supported(e) for e in extensions],
        }
        result["log"] = log.take()
    return result, []
