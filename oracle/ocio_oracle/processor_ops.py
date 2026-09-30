# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for what the optimizer makes of a processor (Phase 1, chunk O1.1).

- processor_ops: a processor, and the optimized processor that getOptimizedProcessor makes of it
  for given bit depths and flags; for each, its cache ID and its createGroupTransform(), every
  transform written out with all its getters.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import numpy as np
import PyOpenColorIO as OCIO

from . import spec
from .checks import PROCESSOR_KEYS, check_keys, f64_bits
from .commands import _processor, captured_log, command, exception_result

# What OCIO raises (in PyOpenColorIO, ExceptionMissingFile doesn't derive from OCIO.Exception).
RAISED = (OCIO.Exception, OCIO.ExceptionMissingFile)

# The prefixes of the methods the dump calls without arguments.
GETTER_PREFIXES = ("get", "is", "has")

# Objects nested deeper than this are written as their class only. A processor's group is
# flat, and its deepest value is 6 levels down: group -> GradingRGBCurveTransform ->
# GradingRGBCurve -> GradingBSplineCurve -> control points -> a GradingControlPoint -> its x.
MAX_DEPTH = 8


def _is_enum(value):
    return hasattr(type(value), "__members__") and hasattr(value, "name")


def _dump(value, blobs, depth=0):
    """A JSON form of what a getter returned:
    - None, bools, ints and strings as themselves; floats (C doubles, or C floats the binding
      widened) as {"f64": their bits}; enums as {"enum": name};
    - numpy arrays as {"blob": index among the response blobs, "dtype", "shape"}, the blob
      holding the array's bytes (little-endian, C order);
    - lists, tuples and the binding's iterators as lists;
    - any other object as {"class": its type's name, "getters": each method named get*, is* or
      has* that takes no argument, called, by name, "properties": each property, by name,
      "uncalled": the get*, is* and has* methods that need arguments}, and for a GroupTransform
      "children", its transforms in order."""
    if value is None or isinstance(value, (bool, int, str)):
        return value
    if isinstance(value, float):
        return {"f64": f64_bits(value)}
    if _is_enum(value):
        return {"enum": value.name}
    if isinstance(value, np.ndarray):
        blobs.append(np.ascontiguousarray(value).astype(value.dtype.newbyteorder("<")).tobytes())
        return {"blob": len(blobs) - 1, "dtype": value.dtype.name, "shape": list(value.shape)}
    if isinstance(value, (list, tuple)) or hasattr(type(value), "__next__"):
        return [_dump(item, blobs, depth + 1) for item in value]
    if depth >= MAX_DEPTH:
        return {"class": type(value).__name__}
    out = {"class": type(value).__name__, "getters": {}, "properties": {}, "uncalled": []}
    cls = type(value)
    for name in sorted(dir(value)):
        attribute = getattr(cls, name, None)
        if isinstance(attribute, property):
            out["properties"][name] = _dump(getattr(value, name), blobs, depth + 1)
        elif name.startswith(GETTER_PREFIXES) and callable(getattr(value, name)):
            try:
                result = getattr(value, name)()
            except TypeError:
                # pybind11 found no overload without arguments.
                out["uncalled"].append(name)
                continue
            out["getters"][name] = _dump(result, blobs, depth + 1)
    if isinstance(value, OCIO.GroupTransform):
        out["children"] = [_dump(child, blobs, depth + 1) for child in value]
    return out


def _processor_dump(proc, blobs):
    return {"cache_id": proc.getCacheID(), "group": _dump(proc.createGroupTransform(), blobs)}


@command
def processor_ops(args, blobs):
    """What getOptimizedProcessor(in, out, flags) makes of a processor: both processors' cache
    IDs and createGroupTransform(), each transform with all its getters.

    args:
      config, transform or src/dst, direction
                the processor, as cpu_apply takes them
      in_bitdepth, out_bitdepth
                BIT_DEPTH_* names (default BIT_DEPTH_F32)
      optimization
                flags (see spec.flags; default OPTIMIZATION_DEFAULT)
    result:
      processor, optimized
                each {"cache_id": getCacheID(), "group": createGroupTransform() written out
                (see _dump): {"class": "GroupTransform", "getters", "properties", "uncalled",
                "children": the transforms, each written out the same way}}
      exception, stage   when OCIO raised: {"type", "message"}, and where: "config",
                "transform", "processor" (as in cpu_apply), "group" (the processor's
                createGroupTransform), "optimize" (getOptimizedProcessor) or "optimized_group"
      log       OCIO's log messages
    blobs: the arrays the getters returned (a LUT's getData()), in the order the result
           names them; none when OCIO raised

    What the dump can't express:
    - getters that need arguments, which it lists as "uncalled": Lut1DTransform.getValue(index)
      and Lut3DTransform.getValue(r, g, b), whose values getData() holds, and the grading curve
      transforms' getSlope(curve, index), whose values the curves' getSlopes() hold;
    - what the binding doesn't expose: op data a transform doesn't carry (such as a 1D LUT's
      inversion quality), and the CPU and GPU engines' choices of renderers;
    - a signalling NaN a C float getter returns: the binding widens it to a Python float, which
      quiets it (a C double comes back exact).
    """
    check_keys("processor_ops", args, PROCESSOR_KEYS | {"in_bitdepth", "out_bitdepth",
                                                        "optimization"})
    depths = []
    for key in ("in_bitdepth", "out_bitdepth"):
        name = args.get(key, "BIT_DEPTH_F32")
        if name not in OCIO.BitDepth.__members__:
            raise ValueError(f"{key}: unknown BitDepth {name!r}")
        depths.append(OCIO.BitDepth.__members__[name])
    in_bd, out_bd = depths
    flags = spec.flags(args.get("optimization"))
    stage, result, out = ["config"], {}, []
    with captured_log() as log:
        try:
            _, proc = _processor(args, stage)
            stage[0] = "group"
            result["processor"] = _processor_dump(proc, out)
            stage[0] = "optimize"
            optimized = proc.getOptimizedProcessor(in_bd, out_bd, flags)
            stage[0] = "optimized_group"
            result["optimized"] = _processor_dump(optimized, out)
        except RAISED as exc:
            result = {"exception": exception_result(exc), "stage": stage[0]}
            out = []
    result["log"] = log
    return result, out
