# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Build OCIO objects from JSON descriptions.

A transform spec is a JSON object:

    {"class": "LogTransform",                 # any PyOpenColorIO transform class
     "args": {"base": 2.0},                   # constructor keyword arguments
     "calls": [["setDirection", {"enum": "TRANSFORM_DIR_INVERSE"}]],   # setters, in order
     "children": [<spec>, ...]}               # GroupTransform only

Values inside "args" and "calls" are converted recursively:
- {"enum": "NAME"} becomes ``getattr(OCIO, "NAME")``;
- {"transform": <spec>} becomes a transform;
- lists and plain JSON values pass through.
"""

import PyOpenColorIO as OCIO


def value(v):
    if isinstance(v, dict):
        if set(v) == {"enum"}:
            return getattr(OCIO, v["enum"])
        if set(v) == {"transform"}:
            return transform(v["transform"])
        raise ValueError(f"unknown value spec {v!r}")
    if isinstance(v, list):
        return [value(x) for x in v]
    return v


def transform(spec):
    cls = getattr(OCIO, spec["class"])
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
