# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for what the optimizer makes of a processor (Phase 1, chunk O1.1).

- processor_ops: a processor, and the optimized processor that getOptimizedProcessor makes of it
  for given bit depths and flags; for each, its cache ID, isNoOp, hasChannelCrosstalk,
  isDynamic, processor metadata and createGroupTransform(), every transform written out with
  all its getters.

Like every oracle command, it reports what the library does and never computes expected
values.

What it reports differs between the Windows and Linux wheels (D12) where the optimizer
computes values with the platform's libm: over 42 cases compared, the LUTs baked from
ExponentTransform and ExponentWithLinearTransform for integer and half inputs, their values
and the optimized processor's cache ID, which hashes them.
"""

import PyOpenColorIO as OCIO

from . import spec
from .checks import PROCESSOR_KEYS, check_keys, check_member, dump
from .commands import RAISED, _processor, captured_log, command, exception_result


def _processor_dump(proc, blobs):
    """A processor's getters that take no argument and don't build another processor, and its
    createGroupTransform() written out."""
    return {
        "cache_id": proc.getCacheID(),
        "isNoOp": proc.isNoOp(),
        "hasChannelCrosstalk": proc.hasChannelCrosstalk(),
        "isDynamic": proc.isDynamic(),
        "processor_metadata": dump(proc.getProcessorMetadata(), blobs),
        "group": dump(proc.createGroupTransform(), blobs),
    }


@command
def processor_ops(args, blobs):
    """What getOptimizedProcessor(in, out, flags) makes of a processor: both processors' cache
    IDs, flags, metadata and createGroupTransform(), each transform with all its getters.

    args:
      config, transform or src/dst, direction
                the processor, as cpu_apply takes them
      in_bitdepth, out_bitdepth
                BIT_DEPTH_* names (default BIT_DEPTH_F32)
      optimization
                flags (see spec.flags; default OPTIMIZATION_DEFAULT)
    request blobs: the transform spec's blobs (see spec.py)
    result:
      processor, optimized
                each {"cache_id": getCacheID(), "isNoOp", "hasChannelCrosstalk", "isDynamic",
                "processor_metadata": getProcessorMetadata() (the files and looks it read),
                "group": createGroupTransform()}, the objects written out as checks.dump
                writes them: {"class", "getters", "properties", "uncalled"}, and a group's
                "children", its transforms written out the same way
      exception, stage   when OCIO or the binding raised (commands.RAISED): {"type",
                "message"}, and where: "config",
                "transform", "processor" (as in cpu_apply), "group" (the processor's getters
                and createGroupTransform), "optimize" (getOptimizedProcessor) or
                "optimized_group"
      log       OCIO's log messages
    blobs: the arrays the getters returned (a LUT's getData()), in the order the result
           names them; none when OCIO raised

    A value too deep to write out, or a bit depth that isn't a BIT_DEPTH_* name, makes the
    command raise, as an unknown key does.

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
    in_bd, out_bd = (check_member(key, args.get(key, "BIT_DEPTH_F32"), OCIO.BitDepth)
                     for key in ("in_bitdepth", "out_bitdepth"))
    flags = spec.flags(args.get("optimization"))
    stage, result, out = ["config"], {}, []
    with captured_log() as log:
        try:
            _, proc = _processor(args, stage, blobs)
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
