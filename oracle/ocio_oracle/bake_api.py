# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for the LUT baker (card p4-oracle, chunk O4.4).

- bake: a Baker on a config, its setters and format metadata in order, then bake(): the
  text it returns and the bytes of the file the other overload writes, the getters after the
  setters, or what raised, and the log.

Configs that name files (LUTs, search paths) run through with_files (files_api.py), with
{"file": "$FILES/config.ocio"} as the config.

Baked values come from the CPU processors, so they depend on the CPU's kernels and, where
libm is called, on the platform: callers compare them live, never as fixtures.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import os
import tempfile

import PyOpenColorIO as OCIO

from . import spec
from .checks import check_keys
from .commands import RAISED, captured_log, command, exception_result, wheel_raised
from .config_api import RequestError, text

# The Baker setters "calls" may use.
SETTERS = {"setFormat", "setInputSpace", "setShaperSpace", "setLooks", "setTargetSpace",
           "setDisplayView", "setShaperSize", "setCubeSize"}

# The FormatMetadata methods "metadata" may call on the baker's getFormatMetadata().
METADATA_METHODS = {"__setitem__", "addChildElement", "setElementName", "setElementValue",
                    "setName", "setID", "clear"}

# The getters reported after the setters.
GETTERS = ("getFormat", "getInputSpace", "getShaperSpace", "getLooks", "getTargetSpace",
           "getDisplay", "getView", "getShaperSize", "getCubeSize")


def _checked_calls(what, calls, allowed):
    """`calls` ([[method, arg, ...], ...] with methods of `allowed`), string arguments read as
    text or {"bytes": hex}; refused otherwise."""
    if not isinstance(calls, list) or not all(
            isinstance(c, list) and c and c[0] in allowed for c in calls):
        raise RequestError(f"{what} must be a list of [method, args...] with a method of "
                           f"{sorted(allowed)}, not {calls!r}")
    out = []
    for method, *values in calls:
        values = [text(f"{what} {method}", v) if isinstance(v, (str, dict)) else v
                  for v in values]
        if any(isinstance(v, bool) or not isinstance(v, (str, bytes, int)) for v in values):
            raise RequestError(f"{what} {method}: arguments are strings or integers: {values!r}")
        out.append((method, values))
    return out


def _decoded(fn):
    """fn()'s text as {"bytes": hex}, or {"undecodable": hex} where the binding couldn't decode
    it as UTF-8."""
    try:
        value = fn()
    except UnicodeDecodeError as exc:
        return {"undecodable": bytes(exc.object).hex()}
    return value if isinstance(value, int) else {"bytes": value.encode("utf-8").hex()}


def _baked_file(baker):
    """bake(fileName) into a new temporary file (a std::ofstream in text mode): its bytes, hex."""
    with tempfile.TemporaryDirectory() as root:
        path = os.path.join(root, "baked")
        baker.bake(path)
        with open(path, "rb") as f:
            return f.read().hex()


@command
def bake(args, blobs):
    """Bakes a LUT.

    args:
      config      the config (spec.config: {"yaml": text}, {"builtin": name}, {"file": path},
                  or null for the raw config), given to setConfig first
      metadata    optional: [[method, arg, ...], ...], FormatMetadata methods (__setitem__,
                  which is the binding's addAttribute, addChildElement, setElementName,
                  setElementValue, setName, setID, clear) called in order on the baker's
                  getFormatMetadata(), before the setters
      calls       [[setter, arg, ...], ...]: Baker setters (setFormat, setInputSpace,
                  setShaperSpace, setLooks, setTargetSpace, setDisplayView, setShaperSize,
                  setCubeSize) called in order; arguments are text, {"bytes": hex} or integers
    result:
      getters     after the setters: {getter: value}: strings {"bytes": hex} (or
                  {"undecodable": hex}), sizes as integers
      text        bake(): {"bytes": hex} of the text, or {"undecodable": hex}
      file        bake(fileName): the bytes of the file it writes, hex (text mode: on
                  Windows each LF is written as CR LF)
      exception, stage   when OCIO or the binding raised: {"type", "message"}, and where:
                  "config", "metadata", "setters" or "bake"
      log         OCIO's log messages
    blobs: none

    An unknown key, a method that isn't listed, or an argument it can't read, is refused.
    """
    check_keys("bake", args, {"config", "metadata", "calls"})
    metadata = _checked_calls("metadata", args.get("metadata", []), METADATA_METHODS)
    calls = _checked_calls("calls", args.get("calls", []), SETTERS)
    stage, result = ["config"], {}
    with captured_log() as log:
        try:
            config = spec.config(args.get("config"))
            baker = OCIO.Baker()
            baker.setConfig(config)
            stage[0] = "metadata"
            for method, values in metadata:
                getattr(baker.getFormatMetadata(), method)(*values)
            stage[0] = "setters"
            for method, values in calls:
                getattr(baker, method)(*values)
            result["getters"] = {g: _decoded(getattr(baker, g)) for g in GETTERS}
            stage[0] = "bake"
            result["text"] = _decoded(baker.bake)
            result["file"] = _baked_file(baker)
        except RAISED as exc:
            if not wheel_raised(exc):
                raise
            result = {"exception": exception_result(exc), "stage": stage[0]}
    result["log"] = log
    return result, []
