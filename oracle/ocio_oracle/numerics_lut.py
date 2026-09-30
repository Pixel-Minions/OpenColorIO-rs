# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle commands for spike S4: CPU dispatch and the Lut3D renderers.

Like every oracle command, these report what the library does and never compute expected
values.
"""

import os
import subprocess

import numpy as np
import PyOpenColorIO as OCIO

from . import spec
from .commands import captured_log, command, exception_result


@command
def lut3d_apply(args, blobs):
    """Applies a Lut3DTransform built from raw LUT values to packed F32 RGBA pixels.

    args:
      interpolation  INTERP_* name (required)
      direction      TRANSFORM_DIR_* name (default forward)
      width          image width in pixels; the pixels are laid out as rows of this width
                     (default: all pixels on one row). OCIO's CPU renderers receive one row
                     per call, so width 1 makes every call a single pixel.
      optimization   optimization flags (see spec.flags); absent means
                     getDefaultCPUProcessor()
    blobs: [LUT values: float32, gridSize^3 * 3 values in Lut3DTransform.setData order
            (red index slowest, blue index fastest, then r, g, b),
            input pixels: float32 RGBA]
    result: {"grid_size", "optimized_ops", "processor_cache_id", "cpu_cache_id", "log"}
            or {"exception", "log"}
    blobs: [output pixels: float32 RGBA]
    """
    with captured_log() as log:
        try:
            lut = np.frombuffer(blobs[0], dtype=np.float32).copy()
            transform = OCIO.Lut3DTransform()
            transform.setData(lut)
            transform.setInterpolation(getattr(OCIO, args["interpolation"]))

            # A new config per call: each config has its own processor cache, and inline
            # LUTs with equal min/max values can share a cache key within one config.
            config = OCIO.Config.CreateRaw()
            direction = getattr(OCIO, args.get("direction", "TRANSFORM_DIR_FORWARD"))
            proc = config.getProcessor(transform, direction)
            flags = spec.flags(args.get("optimization"))
            if "optimization" in args:
                cpu = proc.getOptimizedCPUProcessor(OCIO.BIT_DEPTH_F32, OCIO.BIT_DEPTH_F32, flags)
            else:
                cpu = proc.getDefaultCPUProcessor()

            src = np.frombuffer(blobs[1], dtype=np.float32).copy()
            npix = src.size // 4
            width = int(args.get("width", npix))
            if width <= 0 or npix % width != 0:
                raise ValueError(f"{npix} pixels do not make rows of width {width}")
            height = npix // width
            dst = np.zeros(npix * 4, dtype=np.float32)
            row = 16 * width
            src_desc = OCIO.PackedImageDesc(src, width, height, 4, OCIO.BIT_DEPTH_F32, 4, 16, row)
            dst_desc = OCIO.PackedImageDesc(dst, width, height, 4, OCIO.BIT_DEPTH_F32, 4, 16, row)
            cpu.apply(src_desc, dst_desc)

            group = proc.getOptimizedProcessor(flags).createGroupTransform()
            result = {
                "grid_size": transform.getGridSize(),
                "optimized_ops": [type(t).__name__ for t in group],
                "processor_cache_id": proc.getCacheID(),
                "cpu_cache_id": cpu.getCacheID(),
            }
            out = [dst.tobytes()]
        except OCIO.Exception as exc:
            result, out = {"exception": exception_result(exc)}, []
    result["log"] = log
    return result, out


@command
def cpu_info(args, blobs):
    """What OCIO's CPUInfo reports on this machine, as printed by the wheel's `ociocpuinfo`.

    The wheel builds `ociocpuinfo` from upstream's CPUInfo.cpp with the library's generated
    CPUInfoConfig.h, so its has*() lines combine the CPU's flags with the build's OCIO_USE_*
    switches.

    result: {"fields": {name: value as printed}, "text": the app's output}
    """
    exe = "ociocpuinfo.exe" if os.name == "nt" else "ociocpuinfo"
    path = os.path.join(os.path.dirname(OCIO.__file__), "bin", exe)
    run = subprocess.run([path], capture_output=True, check=True)
    text = run.stdout.decode("latin-1")
    fields = {}
    for line in text.splitlines():
        key, sep, value = line.partition(": ")
        if sep:
            fields[key.strip()] = value
    return {"fields": fields, "text": text}, []
