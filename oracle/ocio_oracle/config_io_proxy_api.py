# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for a config's I/O proxy (card p4-ocioz-fu).

- config_io_proxy: calls on Config.getConfigIOProxy() of a config, in order (getLutData,
  getConfigData, getFastLutFileHash), each reported as what it returns or raises.

A config read from an .ocioz archive (spec.config {"file": "$FILES/a.ocioz"} through
with_files) has the archive's proxy; a config read from a file has none.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

from . import spec
from .checks import check_keys
from .commands import RAISED, captured_log, command, exception_result, wheel_raised
from .config_api import RequestError, hex_bytes, text

# The calls and whether each takes a path.
_CALLS = {"getLutData": True, "getConfigData": False, "getFastLutFileHash": True}


@command
def config_io_proxy(args, blobs):
    """Calls a config's I/O proxy.

    args:
      config      a config (spec.config, e.g. {"file": "$FILES/a.ocioz"} through with_files)
      calls       a list, each {"getLutData": path}, {"getConfigData": null} or
                  {"getFastLutFileHash": path}; a path is text or {"bytes": hex}
    result:
      "proxy": whether the config has one (getConfigIOProxy() isn't None); where it has,
      "calls": per call, in order, {"data": a blob index} (getLutData's bytes),
      {"text": {"bytes": hex}} (the strings of getConfigData and getFastLutFileHash), or
      {"exception": {"type", "message"}}; a string that isn't UTF-8, which the binding can't
      return, {"exception": {"type": "UnicodeDecodeError", "undecodable": hex of its bytes}}
      exception, stage   when reading the config raised: {"type", "message"}, "config"
      log         OCIO's log messages
    blobs: getLutData's bytes, as the result indexes them

    An unknown key, a call that isn't one of the three, a call without its path or with an
    argument it doesn't take, or a path it can't read, is refused.
    """
    check_keys("config_io_proxy", args, {"config", "calls"})
    calls = args.get("calls")
    if not isinstance(calls, list):
        raise RequestError(f"config_io_proxy calls must be a list, not {calls!r}")
    parsed = []
    for call in calls:
        if not isinstance(call, dict) or len(call) != 1:
            raise RequestError(f"config_io_proxy: a call is {{name: argument}}, not {call!r}")
        ((name, value),) = call.items()
        if name not in _CALLS:
            raise RequestError(f"config_io_proxy: unknown call {name!r}")
        if _CALLS[name]:
            parsed.append((name, (text(f"config_io_proxy {name}", value),)))
        elif value is not None:
            raise RequestError(f"config_io_proxy: {name} takes no argument, not {value!r}")
        else:
            parsed.append((name, ()))
    result, out = {}, []
    with captured_log() as log:
        try:
            config = spec.config(args.get("config"))
        except RAISED as exc:
            if not wheel_raised(exc):
                raise
            config, result = None, {"exception": exception_result(exc), "stage": "config"}
        if config is not None:
            proxy = config.getConfigIOProxy()
            result["proxy"] = proxy is not None
            if proxy is not None:
                reports = []
                for name, call_args in parsed:
                    try:
                        value = getattr(proxy, name)(*call_args)
                    except UnicodeDecodeError as exc:
                        # pybind11's conversion of a returned string that isn't UTF-8:
                        # exc.object holds the string's bytes.
                        reports.append({"exception": {"type": "UnicodeDecodeError",
                                                      "undecodable": bytes(exc.object).hex()}})
                        continue
                    except RAISED as exc:
                        if not wheel_raised(exc):
                            raise
                        reports.append({"exception": exception_result(exc)})
                        continue
                    if name == "getLutData":
                        out.append(bytes(value))
                        reports.append({"data": len(out) - 1})
                    else:
                        reports.append({"text": hex_bytes(value)})
                result["calls"] = reports
    result["log"] = log
    return result, out
