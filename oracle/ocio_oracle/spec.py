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
"args" may also be a list, of positional arguments.

An object spec builds a value object, an instance of a PyOpenColorIO class that isn't a
transform (GradingControlPoint, GradingBSplineCurve, GradingRGBCurve, GradingPrimary, ...), the
same way:

    {"class": "GradingBSplineCurve",          # any PyOpenColorIO class but a transform's
     "args": [[0.0, 0.0, 0.5, 0.6, 1.0, 1.0]],  # positional, or keyword arguments as an object
     "attrs": [["name", <value>], ...],       # attributes and properties set, in order
     "calls": [["setSlopes", [1.0, 0.8, 1.2]]]}  # then methods called, in order

with "factory" instead of "args" as for a transform (it must return an instance of the class).
"attrs" set fields such as GradingPrimary's, which the binding exposes as attributes rather
than setters.

Values inside "args", "factory", "attrs" and "calls" are converted recursively:
- {"enum": "NAME"} becomes the member of a PyOpenColorIO enum of that name,
  ``getattr(OCIO, "NAME")``: any other attribute of the module (a function, a class, a
  dunder) is refused. {"enum": "Class.NAME"} names a member that only a class of the binding
  exports (``ConfigMergingParameters.STRATEGY_PREFER_INPUT``: the module doesn't export
  MergeStrategies' values), ``getattr(OCIO.Class, "NAME")``, with the same refusals;
- {"transform": <spec>} becomes a transform;
- {"object": <spec>} becomes a value object (an object spec);
- {"f64": bits} becomes the float (a C double) with those bits, an unsigned 64-bit integer, as
  checks.dump writes floats: the way to pass NaNs (any sign and payload, signalling ones
  included), the infinities and -0.0, which JSON can't hold or loses;
- {"blob": i, "dtype": name} becomes a NumPy array of the spec's blob i, read as the
  little-endian NumPy dtype `name` ("float32" for the LUT transforms' setData),
  one-dimensional, or with "shape": [n, ...] in that shape (C order). It is a read-only,
  C-contiguous view of the blob's bytes, so the library reads only its entries. This is how
  a LUT's values reach Lut1DTransform.setData and Lut3DTransform.setData in one call: a
  65,536-entry half domain or a 129^3 cube doesn't fit in JSON. A blob that isn't a whole
  number of entries, or doesn't fill the shape, and a big-endian dtype (">f4"), are refused;
- lists and plain JSON values pass through.

A spec's blobs are its own list, numbered from 0. Each command that takes a transform spec
passes the request blobs that follow its own (its docstring says which are its own), so a spec
and its blobs travel together whatever the command.
"""

import struct

import numpy as np
import PyOpenColorIO as OCIO

from .checks import check_uint


def f64_from_bits(bits):
    """The float with these bits. Anything but an unsigned 64-bit integer is refused (a bool is
    an int in Python, but not bits here)."""
    if isinstance(bits, bool) or not isinstance(bits, int) or not 0 <= bits < 1 << 64:
        raise ValueError(f"f64 bits must be an unsigned 64-bit integer, not {bits!r}")
    return struct.unpack("<d", struct.pack("<Q", bits))[0]


def array_from_blob(v, blobs):
    """The NumPy array of a {"blob": i, "dtype": name} value spec, with an optional "shape"
    (see the module's docstring). Refused: other keys, or no dtype; an index that isn't an
    integer naming one of the spec's blobs (a bool isn't one); a dtype that isn't the name of a
    NumPy type of fixed size (objects aren't one), or names a big-endian one; a shape that
    isn't a list of non-negative integers; a blob that isn't a whole number of entries, or
    doesn't fill the shape."""
    if not set(v) <= {"blob", "dtype", "shape"} or "dtype" not in v:
        raise ValueError(f"a blob value spec takes blob, dtype and optionally shape: {v!r}")
    index = v["blob"]
    if isinstance(index, bool) or not isinstance(index, int) or not 0 <= index < len(blobs):
        raise ValueError(f"blob {index!r} isn't one of the spec's {len(blobs)} blobs")
    dtype = np.dtype(v["dtype"]) if isinstance(v["dtype"], str) else None
    if dtype is None or dtype.itemsize == 0 or dtype.hasobject:
        raise ValueError(f"a blob's dtype is the name of a NumPy type of fixed size, not "
                         f"{v['dtype']!r}")
    if dtype.byteorder == ">":
        raise ValueError(f"a blob's dtype is little-endian, as the blobs are, not {v['dtype']!r}")
    dtype = dtype.newbyteorder("<")
    data = blobs[index]
    if len(data) % dtype.itemsize:
        raise ValueError(f"blob {index} has {len(data)} bytes, not a whole number of "
                         f"{dtype.name} entries")
    # The binding only reads the array (setData), so a view of the blob is enough.
    array = np.frombuffer(data, dtype=dtype)
    if "shape" in v:
        shape = v["shape"]
        if not isinstance(shape, list) or any(
                isinstance(n, bool) or not isinstance(n, int) or n < 0 for n in shape):
            raise ValueError(f"a blob's shape is a list of non-negative integers, not {shape!r}")
        if int(np.prod(shape, dtype=np.int64)) != array.size:
            raise ValueError(f"blob {index} has {array.size} {dtype.name} entries, which don't "
                             f"fill the shape {shape}")
        array = array.reshape(shape)
    return array


# The metaclass of the binding's classes (pybind11_type): its transforms, value objects, enums
# and iterators. Python's own classes (OCIO.Exception, the module's type) aren't of it.
BINDING_TYPE = type(OCIO.Config)


def enum_member(name):
    """The member of a PyOpenColorIO enum named `name`: "NAME", as the module exports it, or
    "Class.NAME", as a class of the binding exports it (the module doesn't export the values
    of an enum nested in a class, such as ConfigMergingParameters.MergeStrategies). Anything
    else, such as a function of the module (GetCurrentConfig, SetEnvVariable) or of a class
    (ConfigMergingParameters.Create), a nested enum's type, a dunder (__setattr__), or a class
    that isn't the binding's, is refused: a value spec names an enum member, never a callable
    to hand to the library or to an object."""
    owner, short = OCIO, name
    if isinstance(name, str) and "." in name:
        owner_name, _, short = name.partition(".")
        owner = getattr(OCIO, owner_name, None)
        if not isinstance(owner, type) or type(owner) is not BINDING_TYPE:
            raise ValueError(f"{name!r} isn't a member of a PyOpenColorIO enum")
    member = getattr(owner, short, None) if isinstance(short, str) else None
    members = getattr(type(member), "__members__", None)
    if (member is None or type(type(member)) is not BINDING_TYPE or not isinstance(members, dict)
            or members.get(short) is not member):
        raise ValueError(f"{name!r} isn't a member of a PyOpenColorIO enum")
    return member


def binding_class(name, what):
    """The PyOpenColorIO class named `name`: one of the binding's classes. Python's own classes
    the module holds (Exception, ExceptionMissingFile, the module's type as __class__) are
    refused."""
    cls = getattr(OCIO, name, None) if isinstance(name, str) else None
    if not isinstance(cls, type) or type(cls) is not BINDING_TYPE:
        raise ValueError(f"{what} names a class of the binding, not {name!r}")
    return cls


def value(v, blobs=()):
    """The Python value of a value spec; `blobs` are the spec's blobs."""
    if isinstance(v, dict):
        if set(v) == {"enum"}:
            return enum_member(v["enum"])
        if set(v) == {"transform"}:
            return transform(v["transform"], blobs)
        if set(v) == {"object"}:
            return value_object(v["object"], blobs)
        if set(v) == {"f64"}:
            return f64_from_bits(v["f64"])
        if "blob" in v:
            return array_from_blob(v, blobs)
        raise ValueError(f"unknown value spec {v!r}")
    if isinstance(v, list):
        return [value(x, blobs) for x in v]
    return v


def _construct(what, cls, spec, blobs):
    """An instance of `cls` from a transform or object spec's "args" (keyword arguments, or a
    list of positional ones) or "factory"."""
    if "factory" in spec:
        if spec.get("args"):
            raise ValueError(f"{what} takes args or a factory, not both: {spec!r}")
        name, *args = spec["factory"]
        obj = getattr(cls, name)(*[value(a, blobs) for a in args])
        if not isinstance(obj, cls):
            raise ValueError(f"{spec['class']}.{name} returned a {type(obj).__name__}, not a "
                             f"{spec['class']}")
        return obj
    args = spec.get("args") or {}
    if isinstance(args, list):
        return cls(*[value(a, blobs) for a in args])
    return cls(**{k: value(v, blobs) for k, v in args.items()})


# The methods a spec's "calls" may not run, as they write files or depend on the process:
# - GroupTransform.write without a config writes with the process's current config
#   (PyGroupTransform.cpp:89-92, 110-113 @ v2.5.2), which latches on first use, so the result
#   would depend on what ran earlier in the oracle's process; its (formatName, fileName) form
#   writes a file anywhere;
# - Config.archive(path) (PyConfig.cpp:971-977) and Baker.bake(fileName) (PyBaker.cpp:128-133)
#   write a file anywhere, and open it before they can throw, so they create or truncate it even
#   when they fail.
# The commands that write (write_api.py, bake_api.py, ocioz_api.py, config_calls) do it their
# own way.
REFUSED_CALLS = {"write", "archive", "bake"}

# The methods whose forms with arguments a spec's "calls" may not run: Config.serialize(fileName)
# writes the config to a file anywhere (PyConfig.cpp:243-249); serialize() returns the text.
REFUSED_FILE_FORMS = {"serialize"}


def _call(obj, spec, blobs):
    """Calls the methods of a spec's "calls" on `obj`, in order. A method of REFUSED_CALLS, or
    of REFUSED_FILE_FORMS with arguments, is refused."""
    for call in spec.get("calls") or []:
        if call and (call[0] in REFUSED_CALLS or call[0] in REFUSED_FILE_FORMS and call[1:]):
            raise ValueError(f"a spec's calls can't run {call[0]!r}: {call!r}")
        getattr(obj, call[0])(*[value(a, blobs) for a in call[1:]])


def transform(spec, blobs=()):
    """The transform of a transform spec; `blobs` are the spec's blobs."""
    cls = binding_class(spec["class"], "a transform spec")
    if not issubclass(cls, OCIO.Transform):
        raise ValueError(f"a transform spec builds a transform, not a {spec['class']!r}")
    obj = _construct("a transform spec", cls, spec, blobs)
    _call(obj, spec, blobs)
    for child in spec.get("children") or []:
        obj.appendTransform(transform(child, blobs))
    return obj


# The keys of an object spec.
OBJECT_KEYS = {"class", "args", "factory", "attrs", "calls"}


def value_object(spec, blobs=()):
    """The value object of an object spec; `blobs` are the spec's blobs. Refused: a key the
    spec doesn't take, a class that isn't a PyOpenColorIO class or is a transform's (a
    transform spec builds those), and "attrs" that aren't [name, value] pairs."""
    if not isinstance(spec, dict) or "class" not in spec or not set(spec) <= OBJECT_KEYS:
        raise ValueError(f"an object spec takes class and optionally args or factory, attrs and "
                         f"calls: {spec!r}")
    cls = binding_class(spec["class"], "an object spec")
    if issubclass(cls, OCIO.Transform):
        raise ValueError(f"an object spec builds an instance of a class that isn't a "
                         f"transform's, not {spec['class']!r}")
    obj = _construct("an object spec", cls, spec, blobs)
    for attr in spec.get("attrs") or []:
        if not isinstance(attr, list) or len(attr) != 2 or not isinstance(attr[0], str):
            raise ValueError(f"an object spec's attrs are [name, value] pairs, not {attr!r}")
        setattr(obj, attr[0], value(attr[1], blobs))
    _call(obj, spec, blobs)
    return obj


def config(spec):
    """A config from {"builtin": name}, {"yaml": text}, {"file": path}, None (raw), or
    {"create": {}} (Config::Create, exposed as Config() by PyConfig.cpp:190 @ v2.5.2).
    {"create": {"processor_cache": "off"}} also disables its processor cache, as upstream's
    GPU test harness does (tests/gpu/GPUUnitTest.cpp:137-142 @ v2.5.2). An optional
    major_version calls setMajorVersion before the cache setting, as GammaOp_test.cpp:33-34
    and CDLOp_test.cpp:36-37 do. The unsigned argument and resulting minor version are the
    wheel's (Config.cpp:1285-1305); no separate minor-version override is provided.
    The only explicit processor-cache setting is "off"."""
    if spec is None:
        return OCIO.Config.CreateRaw()
    if isinstance(spec, dict) and "create" in spec:
        create = spec["create"]
        if (set(spec) != {"create"} or not isinstance(create, dict)
                or not set(create) <= {"processor_cache", "major_version"}):
            raise ValueError(f"a create config takes only optional major_version and "
                             f"processor_cache='off' settings, not {spec!r}")
        if "processor_cache" in create and create["processor_cache"] != "off":
            raise ValueError(f"a create config's processor_cache must be 'off', not "
                             f"{create['processor_cache']!r}")
        config = OCIO.Config()
        if "major_version" in create:
            config.setMajorVersion(check_uint("a create config's major_version",
                                              create["major_version"]))
        if "processor_cache" in create:
            config.setProcessorCacheFlags(OCIO.PROCESSOR_CACHE_OFF)
        return config
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
