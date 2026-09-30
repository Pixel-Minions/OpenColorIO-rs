# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Request checks and value encodings shared by the Phase 1 oracle commands (p1-oracle).

A command refuses a request it can't read exactly (it raises, so the call fails): a key it
doesn't know, which would otherwise pass unnoticed, such as a misspelled optional setting.
Floats go out as their bits, since JSON can't hold NaN or the infinities and a decimal could
hide a sign or a payload.
"""

import struct

import numpy as np

# The keys of the processor, as cpu_apply takes them (commands._processor).
PROCESSOR_KEYS = {"config", "transform", "src", "dst", "direction"}


def check_keys(what, spec, allowed):
    """Refuses `spec` unless it is an object whose keys are all in `allowed`."""
    if not isinstance(spec, dict):
        raise ValueError(f"{what} must be an object, not {spec!r}")
    unknown = sorted(set(spec) - set(allowed))
    if unknown:
        raise ValueError(f"{what}: unknown keys {unknown}; it takes {sorted(allowed)}")


def f64_bits(value):
    """The bits of a Python float (a C double), as an unsigned integer."""
    return struct.unpack("<Q", struct.pack("<d", value))[0]


def f32_bits(values):
    """The bits of each value, converted to a C float, as unsigned integers. The values are
    floats the library returned as float, so the conversion is exact."""
    return np.asarray(values, dtype=np.float32).view(np.uint32).tolist()
