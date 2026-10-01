# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Request checks and value encodings shared by the Phase 1 oracle commands (p1-oracle).

A command refuses a request it can't read exactly (it raises, so the call fails): a key it
doesn't know, which would otherwise pass unnoticed, such as a misspelled optional setting, or a
value of the wrong type, which Python would otherwise convert ("false" is a true bool, 4096.9 an
int of 4096, and True an index of 1).

Floats go out as their bits, since JSON can't hold NaN or the infinities and a decimal could
hide a sign or a payload.
"""

import struct

import numpy as np
import PyOpenColorIO as OCIO

# The keys of the processor, as cpu_apply takes them (commands._processor).
PROCESSOR_KEYS = {"config", "transform", "src", "dst", "direction"}

# The largest C `unsigned`: the settings the library takes as `unsigned` go up to this.
UNSIGNED_MAX = 2 ** 32 - 1


def check_keys(what, spec, allowed):
    """Refuses `spec` unless it is an object whose keys are all in `allowed`."""
    if not isinstance(spec, dict):
        raise ValueError(f"{what} must be an object, not {spec!r}")
    unknown = sorted(set(spec) - set(allowed))
    if unknown:
        raise ValueError(f"{what}: unknown keys {unknown}; it takes {sorted(allowed)}")


def check_bool(what, value):
    """`value`, refused unless it is a JSON bool."""
    if not isinstance(value, bool):
        raise ValueError(f"{what} must be true or false, not {value!r}")
    return value


def check_uint(what, value, top=UNSIGNED_MAX):
    """`value`, refused unless it is a JSON integer from 0 to `top` (a bool isn't one)."""
    if isinstance(value, bool) or not isinstance(value, int) or not 0 <= value <= top:
        raise ValueError(f"{what} must be an integer from 0 to {top}, not {value!r}")
    return value


def check_member(what, value, enum):
    """The member of the PyOpenColorIO enum `enum` named `value`, refused unless it is one."""
    if not isinstance(value, str) or value not in enum.__members__:
        raise ValueError(f"{what}: unknown {enum.__name__} {value!r}")
    return enum.__members__[value]


def f64_bits(value):
    """The bits of a Python float (a C double), as an unsigned integer."""
    return struct.unpack("<Q", struct.pack("<d", value))[0]


def f32_bits(values):
    """The bits of each value, converted to a C float, as unsigned integers. The values are
    floats the library returned as float, so the conversion is exact."""
    return np.asarray(values, dtype=np.float32).view(np.uint32).tolist()


# The prefixes of the methods the dump calls without arguments.
GETTER_PREFIXES = ("get", "is", "has")

# The dump writes out objects this many levels down and raises below that, so that nothing is
# left out unseen. A processor's group is flat, and its deepest value is 6 levels down: group
# -> GradingRGBCurveTransform -> GradingRGBCurve -> GradingBSplineCurve -> control points ->
# a GradingControlPoint -> its x. FormatMetadata nests as deep as the file it was read from,
# so it is written out at any depth.
MAX_DEPTH = 8


def _is_enum(value):
    return hasattr(type(value), "__members__") and hasattr(value, "name")


def dump(value, blobs, depth=0):
    """A JSON form of a value the library returned, exact:
    - None, bools, ints and strings as themselves; floats (C doubles, or C floats the binding
      widened) as {"f64": their bits}; enums as {"enum": name};
    - numpy arrays as {"blob": index among the response blobs, "dtype", "shape"}, the blob
      holding the array's bytes (little-endian, C order);
    - lists, tuples and the binding's iterators as lists;
    - any other object as {"class": its type's name, "getters": each method named get*, is* or
      has* that takes no argument, called, by name, "properties": each property, by name,
      "uncalled": the get*, is* and has* methods that need arguments}, and for a GroupTransform
      "children", its transforms in order.
    An object more than MAX_DEPTH levels down raises ValueError, unless it is FormatMetadata."""
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
        return [dump(item, blobs, depth + 1) for item in value]
    if depth >= MAX_DEPTH and not isinstance(value, OCIO.FormatMetadata):
        raise ValueError(f"the dump refuses a {type(value).__name__} {depth} levels down: it "
                         f"writes out objects {MAX_DEPTH} levels down, and FormatMetadata at any "
                         f"depth")
    out = {"class": type(value).__name__, "getters": {}, "properties": {}, "uncalled": []}
    cls = type(value)
    for name in sorted(dir(value)):
        attribute = getattr(cls, name, None)
        if isinstance(attribute, property):
            out["properties"][name] = dump(getattr(value, name), blobs, depth + 1)
        elif name.startswith(GETTER_PREFIXES) and callable(getattr(value, name)):
            try:
                result = getattr(value, name)()
            except TypeError:
                # pybind11 found no overload without arguments.
                out["uncalled"].append(name)
                continue
            out["getters"][name] = dump(result, blobs, depth + 1)
    if isinstance(value, OCIO.GroupTransform):
        out["children"] = [dump(child, blobs, depth + 1) for child in value]
    return out
