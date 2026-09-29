# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle commands. Each takes (args: dict, blobs: list[bytes]) and returns
(result: JSON value, blobs: list[bytes]).

Commands report what the library does, including the exceptions and log messages it
produces, which are part of the byte-exact surface (PLAN.md §3).
"""

import contextlib
import platform
import sys

import numpy as np
import PyOpenColorIO as OCIO

from . import spec

COMMANDS = {}


def command(fn):
    COMMANDS[fn.__name__] = fn
    return fn


@contextlib.contextmanager
def captured_log():
    """Collects OCIO log messages instead of printing them."""
    messages = []
    OCIO.SetLoggingFunction(messages.append)
    try:
        yield messages
    finally:
        OCIO.ResetToDefaultLoggingFunction()


def exception_result(exc):
    return {"type": type(exc).__name__, "message": str(exc)}


# Bit depth name -> numpy dtype of one channel, as PackedImageDesc stores it.
DTYPES = {
    "BIT_DEPTH_UINT8": np.uint8,
    "BIT_DEPTH_UINT10": np.uint16,
    "BIT_DEPTH_UINT12": np.uint16,
    "BIT_DEPTH_UINT16": np.uint16,
    "BIT_DEPTH_F16": np.float16,
    "BIT_DEPTH_F32": np.float32,
}


@command
def info(args, blobs):
    """Versions and platform of the oracle."""
    return {
        "ocio_version": OCIO.__version__,
        "ocio_version_hex": OCIO.GetVersionHex(),
        "python": sys.version,
        "numpy": np.__version__,
        "platform": platform.platform(),
        "machine": platform.machine(),
        "processor": platform.processor(),
    }, []


@command
def batch(args, blobs):
    """Runs several commands in one process.

    args: {"calls": [{"cmd": str, "args": object, "blobs": [index into request blobs]}]}
    result: [{"result": any, "blobs": [index into response blobs]}]
    """
    results, out = [], []
    for call in args["calls"]:
        result, call_blobs = COMMANDS[call["cmd"]](call.get("args") or {}, [blobs[i] for i in call.get("blobs", [])])
        results.append({"result": result, "blobs": list(range(len(out), len(out) + len(call_blobs)))})
        out.extend(call_blobs)
    return results, out


def _processor(args):
    config = spec.config(args.get("config"))
    direction = getattr(OCIO, args.get("direction", "TRANSFORM_DIR_FORWARD"))
    if "transform" in args:
        return config, config.getProcessor(spec.transform(args["transform"]), direction)
    if "src" in args:
        return config, config.getProcessor(args["src"], args["dst"])
    raise ValueError("cpu_apply needs a transform or src/dst color spaces")


@command
def cpu_apply(args, blobs):
    """Applies a CPU processor to packed pixels.

    args:
      config        config spec (see spec.config); raw config when absent
      transform     transform spec, or src/dst color space names
      direction     TRANSFORM_DIR_* name (default forward)
      optimization  flags (see spec.flags); absent means getDefaultCPUProcessor()
      in_bitdepth, out_bitdepth   BIT_DEPTH_* names (default F32)
      channels      3 or 4 (default 4)
    blobs: [input pixels, little-endian, in in_bitdepth's storage type]
    result: {"processor_cache_id", "cpu_cache_id", "log"} or {"exception", "log"}
    blobs: [output pixels in out_bitdepth's storage type]
    """
    with captured_log() as log:
        try:
            _, proc = _processor(args)
            in_bd = args.get("in_bitdepth", "BIT_DEPTH_F32")
            out_bd = args.get("out_bitdepth", "BIT_DEPTH_F32")
            if "optimization" in args or in_bd != "BIT_DEPTH_F32" or out_bd != "BIT_DEPTH_F32":
                cpu = proc.getOptimizedCPUProcessor(
                    getattr(OCIO, in_bd), getattr(OCIO, out_bd), spec.flags(args.get("optimization"))
                )
            else:
                cpu = proc.getDefaultCPUProcessor()
            channels = int(args.get("channels", 4))
            src = np.frombuffer(blobs[0], dtype=DTYPES[in_bd]).copy()
            npix = src.size // channels
            dst = np.zeros(npix * channels, dtype=DTYPES[out_bd])
            src_desc = OCIO.PackedImageDesc(src, npix, 1, channels, getattr(OCIO, in_bd),
                                            src.itemsize, src.itemsize * channels, src.itemsize * channels * npix)
            dst_desc = OCIO.PackedImageDesc(dst, npix, 1, channels, getattr(OCIO, out_bd),
                                            dst.itemsize, dst.itemsize * channels, dst.itemsize * channels * npix)
            cpu.apply(src_desc, dst_desc)
            result = {"processor_cache_id": proc.getCacheID(), "cpu_cache_id": cpu.getCacheID()}
            out = [dst.tobytes()]
        except OCIO.Exception as exc:
            result, out = {"exception": exception_result(exc)}, []
    result["log"] = log
    return result, out


@command
def builtin_config_names(args, blobs):
    """Names of the built-in configs, in registry order, with their flags."""
    registry = OCIO.BuiltinConfigRegistry()
    return [
        {"name": name, "ui_name": ui, "recommended": rec, "default": dflt}
        for name, ui, rec, dflt in registry.getBuiltinConfigs()
    ], []


@command
def builtin_config_source(args, blobs):
    """The embedded YAML of a built-in config. blobs: [UTF-8 text]."""
    return None, [OCIO.BuiltinConfigRegistry()[args["name"]].encode("utf-8")]


@command
def config_serialize(args, blobs):
    """Config.serialize() and the config's cache ID. blobs: [UTF-8 YAML]."""
    with captured_log() as log:
        try:
            config = spec.config(args["config"])
            text = config.serialize()
            result, out = {"cache_id": config.getCacheID()}, [text.encode("utf-8")]
        except OCIO.Exception as exc:
            result, out = {"exception": exception_result(exc)}, []
    result["log"] = log
    return result, out


# Command modules that register with @command; imported last so `command` exists.
from . import text  # noqa: E402,F401
