# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Build OCIO objects from JSON descriptions.

A transform spec is a JSON object:

    {"class": "LogTransform",                 # any PyOpenColorIO transform class
     "args": {"base": 2.0},                   # constructor keyword arguments
     "calls": [["setDirection", {"enum": "TRANSFORM_DIR_INVERSE"}]],   # setters, in order
     "children": [<spec>, ...]}               # GroupTransform only

Instead of "args", "factory": ["Fit", <arg>, ...] builds the transform with the class's static
factory of that name, from positional arguments (MatrixTransform's Fit, Identity, Sat, Scale and
View); "calls" and "children" then apply as usual. It must return a transform of the class.

Values inside "args", "factory" and "calls" are converted recursively:
- {"enum": "NAME"} becomes ``getattr(OCIO, "NAME")``;
- {"transform": <spec>} becomes a transform;
- {"f64": bits} becomes the float (a C double) with those bits, an unsigned 64-bit integer, as
  checks.dump writes floats: the way to pass NaNs (any sign and payload, signalling ones
  included), the infinities and -0.0, which JSON can't hold or loses;
- lists and plain JSON values pass through.
"""

import struct

import PyOpenColorIO as OCIO


def f64_from_bits(bits):
    """The float with these bits. Anything but an unsigned 64-bit integer is refused (a bool is
    an int in Python, but not bits here)."""
    if isinstance(bits, bool) or not isinstance(bits, int) or not 0 <= bits < 1 << 64:
        raise ValueError(f"f64 bits must be an unsigned 64-bit integer, not {bits!r}")
    return struct.unpack("<d", struct.pack("<Q", bits))[0]


def value(v):
    if isinstance(v, dict):
        if set(v) == {"enum"}:
            return getattr(OCIO, v["enum"])
        if set(v) == {"transform"}:
            return transform(v["transform"])
        if set(v) == {"f64"}:
            return f64_from_bits(v["f64"])
        raise ValueError(f"unknown value spec {v!r}")
    if isinstance(v, list):
        return [value(x) for x in v]
    return v


def transform(spec):
    cls = getattr(OCIO, spec["class"])
    if "factory" in spec:
        if spec.get("args"):
            raise ValueError(f"a transform spec takes args or a factory, not both: {spec!r}")
        name, *args = spec["factory"]
        obj = getattr(cls, name)(*[value(a) for a in args])
        if not isinstance(obj, cls):
            raise ValueError(f"{spec['class']}.{name} returned a {type(obj).__name__}, not a "
                             f"{spec['class']}")
    else:
        obj = cls(**{k: value(v) for k, v in (spec.get("args") or {}).items()})
    for call in spec.get("calls") or []:
        getattr(obj, call[0])(*[value(a) for a in call[1:]])
    for child in spec.get("children") or []:
        obj.appendTransform(transform(child))
    return obj


def config(spec):
    """A config from {"builtin": name}, {"yaml": text}, {"file": path} or None (raw)."""
    if spec is None:
        return OCIO.Config.CreateRaw()
    if "builtin" in spec:
        return OCIO.Config.CreateFromBuiltinConfig(spec["builtin"])
    if "yaml" in spec:
        return OCIO.Config.CreateFromStream(spec["yaml"])
    if "file" in spec:
        return OCIO.Config.CreateFromFile(spec["file"])
    raise ValueError(f"unknown config spec {spec!r}")


def flags(v):
    """Optimization flags from a name, a list of names (OR-ed) or an integer."""
    if v is None:
        return OCIO.OPTIMIZATION_DEFAULT
    if isinstance(v, int):
        return OCIO.OptimizationFlags(v)
    if isinstance(v, str):
        return getattr(OCIO, v)
    result = 0
    for name in v:
        result |= int(getattr(OCIO, name))
    return OCIO.OptimizationFlags(result)
