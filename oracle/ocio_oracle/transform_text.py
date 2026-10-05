# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for what transforms say about themselves (Phase 1, chunk O1.4).

- transform_text: for transform specs, str() and repr() (upstream's operator<<, which the
  binding's repr() prints: PyUtils.h defRepr), what validate() raises, and for pairs of them,
  what equals() returns where the binding exposes it.

Like every oracle command, it reports what the library does and never computes expected
values.

What it reports differs between the Windows and Linux wheels (D12) in one way, over 247
transforms of 13 classes holding special values compared: a negative NaN prints "-nan(ind)" on
Windows and "-nan" on Linux (the platform's iostream), in repr() and in validation messages
that print the value. Zeros, denormals, infinities and large values print the same.
"""

import PyOpenColorIO as OCIO

from . import spec
from .checks import check_keys
from .commands import RAISED, captured_log, command, exception_result


def _text(transform):
    """What one built transform says about itself."""
    out = {"class": type(transform).__name__, "repr": repr(transform), "str": str(transform)}
    try:
        transform.validate()
        out["validate"] = None
    except RAISED as exc:
        out["validate"] = exception_result(exc)
    return out


def _equals(a, b):
    """a.equals(b), or None where the binding has no equals() for a's class, or none that
    takes b (it takes a transform of a's own class)."""
    method = getattr(a, "equals", None)
    if method is None:
        return None
    try:
        return bool(method(b))
    except TypeError:
        # pybind11 found no overload for b's class.
        return None


@command
def transform_text(args, blobs):
    """str(), repr(), validate() and equals() of transforms.

    args:
      transforms  transform specs (see spec.transform), each built on its own
      pairs       [[i, j], ...]: transforms[i].equals(transforms[j]) for each (optional)
    request blobs: the transform specs' blobs, which they share (see spec.py)
    result:
      transforms  per spec, in order: {"class", "repr", "str", "validate": null, or what it
                  raised ({"type", "message"})}, or {"exception", "stage": "transform"} when
                  building it raised (commands.RAISED: the binding's constructors validate some
                  transforms, and its setters check their arguments)
      pairs       per pair, in order: {"equals": true, false, or null where the binding has no
                  equals() for the first transform's class or none taking the second's, or where
                  either wasn't built}
      log         OCIO's log messages
    blobs: none

    The binding exposes equals() for the CDL, Exponent, ExponentWithLinear, ExposureContrast,
    FixedFunction, LogAffine, LogCamera, Log, Lut1D, Lut3D, Matrix and Range transforms, each
    taking a transform of its own class; Python's == on a transform compares identity, so the
    command doesn't report it. An unknown key, transforms or pairs that aren't lists, or a pair
    that doesn't name two transforms by integer index (true and false aren't indices), is
    refused.
    """
    check_keys("transform_text", args, {"transforms", "pairs"})
    specs = args.get("transforms", [])
    pairs = args.get("pairs", [])
    for key, value in (("transforms", specs), ("pairs", pairs)):
        if not isinstance(value, list):
            raise ValueError(f"{key} must be a list, not {value!r}")
    for pair in pairs:
        # A bool is an int in Python, but not an index here.
        if (not isinstance(pair, list) or len(pair) != 2
                or any(isinstance(i, bool) or not isinstance(i, int) or not 0 <= i < len(specs)
                       for i in pair)):
            raise ValueError(f"pairs: {pair!r} doesn't name two of the {len(specs)} transforms")
    built, result = [], {"transforms": [], "pairs": []}
    with captured_log() as log:
        for transform_spec in specs:
            try:
                transform = spec.transform(transform_spec, blobs)
            except RAISED as exc:
                built.append(None)
                result["transforms"].append({"exception": exception_result(exc),
                                             "stage": "transform"})
                continue
            built.append(transform)
            result["transforms"].append(_text(transform))
        for i, j in pairs:
            both = built[i] is not None and built[j] is not None
            result["pairs"].append({"equals": _equals(built[i], built[j]) if both else None})
    result["log"] = log
    return result, []
