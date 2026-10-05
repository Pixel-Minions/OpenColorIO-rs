# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for OCIO's debug log while it builds processors (card p1-engine, the
command the owner approved for the engine and the optimizer).

- processor_debug_log: the debug-level messages OCIO logs while a config builds a processor
  and while that processor builds its optimized CPU processor. Among them, the optimizer's
  lists of ops before and after (OpRcPtrVec::optimize, src/OpenColorIO/OpOptimizers.cpp:
  611-756 @ v2.5.2), which print SerializeOpVec: each op's getInfo() and cache ID, the only
  place the wheel shows the no-ops' cache IDs.

Like every oracle command, it reports what the library does and never computes expected
values.

It sets the logging level to debug for the call and restores the level it found, and resets
the logging function, so later commands of a batch see the state they would have seen.
"""

import PyOpenColorIO as OCIO

from . import spec
from .checks import PROCESSOR_KEYS, check_keys, check_member
from .commands import RAISED, _processor, captured_log, command, exception_result


@command
def processor_debug_log(args, blobs):
    """OCIO's debug-level log while building a processor and its optimized CPU processor.

    args:
      config, transform or src/dst, direction
                the processor, as cpu_apply takes them
      in_bitdepth, out_bitdepth
                BIT_DEPTH_* names (default BIT_DEPTH_F32)
      optimization
                flags (see spec.flags; default OPTIMIZATION_DEFAULT)
    request blobs: the transform spec's blobs (see spec.py)
    result:
      processor       the messages logged while the config built the processor
                      (config.getProcessor), each as the logging function received it
      cpu_processor   the messages logged while getOptimizedCPUProcessor(in, out, flags) built
                      the CPU processor
      cpu_cache_id    the CPU processor's getCacheID()
      level           the logging level in force before the call, which it restores
                      (LoggingLevelToString)
      exception, stage
                      when OCIO or the binding raised (commands.RAISED): {"type", "message"},
                      and where: "config", "transform", "processor" (as in cpu_apply) or
                      "cpu_processor"; "processor" and "cpu_processor" then hold the messages
                      logged so far
    blobs: none

    An unknown key, or a bit depth that isn't a BIT_DEPTH_* name, is refused.
    """
    check_keys("processor_debug_log", args, PROCESSOR_KEYS | {"in_bitdepth", "out_bitdepth",
                                                              "optimization"})
    in_bd, out_bd = (check_member(key, args.get(key, "BIT_DEPTH_F32"), OCIO.BitDepth)
                     for key in ("in_bitdepth", "out_bitdepth"))
    flags = spec.flags(args.get("optimization"))
    previous = OCIO.GetLoggingLevel()
    stage, result = ["config"], {}
    OCIO.SetLoggingLevel(OCIO.LOGGING_LEVEL_DEBUG)
    try:
        with captured_log() as processor_log:
            try:
                _, proc = _processor(args, stage, blobs)
            except RAISED as exc:
                result = {"exception": exception_result(exc), "stage": stage[0]}
        result["processor"] = list(processor_log)
        result["cpu_processor"] = []
        if "exception" not in result:
            with captured_log() as cpu_log:
                try:
                    stage[0] = "cpu_processor"
                    cpu = proc.getOptimizedCPUProcessor(in_bd, out_bd, flags)
                    result["cpu_cache_id"] = cpu.getCacheID()
                except RAISED as exc:
                    result["exception"] = exception_result(exc)
                    result["stage"] = stage[0]
            result["cpu_processor"] = list(cpu_log)
    finally:
        OCIO.SetLoggingLevel(previous)
    result["level"] = OCIO.LoggingLevelToString(previous)
    return result, []
