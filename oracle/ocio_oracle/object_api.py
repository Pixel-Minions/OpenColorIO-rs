# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for the app helpers and grading values (Phase 5, chunk O5.2).

- object_calls: a config, value objects built from object specs (grading values, ...), then
  calls in order on them, on the objects the calls make or return (ColorSpaceMenuParameters,
  ColorSpaceMenuHelper, LegacyViewingPipeline, MixingColorSpaceManager, MixingSlider, ...) and
  on the module functions of DisplayViewHelpers and ColorSpaceHelpers; each call's result or
  exception and log, every value written out exactly by checks.dump (a processor as
  processor_ops writes it, a config by serialize()); and the processors the calls stored,
  written out the same way.

It takes config_calls' frame from config_api.py (the environment, the files, the config
sources, the call targets and argument values, the log), with its own call loop, whose values
go out through checks.dump: config_calls' value_out writes an object's getters and repr only,
and a grading value's fields are properties.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import copy
import types

import PyOpenColorIO as OCIO

from . import spec
from .checks import dump
from .commands import command, wheel_raised
from .config_api import (REPORTED, LogCapture, RequestError, _check_current_config, _method,
                         _target, check_keys, check_source, directory, environment, hex_bytes,
                         make_config, raised, value_in)
from .processor_ops import _processor_dump

# The binding's helper modules, which the calls reach by these names ("on").
HELPER_MODULES = ("DisplayViewHelpers", "ColorSpaceHelpers")

# The names the request's objects can't take.
RESERVED = ("config", "OCIO", *HELPER_MODULES)

# The call kinds and the keys each takes.
CALL_KEYS = {
    "call": {"call", "on", "args", "kwargs", "as"},
    "new": {"new", "args", "kwargs", "as"},
    "copy": {"copy", "as"},
    "dump": {"dump"},
}


def _library(exc):
    """Whether an exception is the library's (OCIO's, or the binding's std::runtime_error), as
    opposed to the oracle's own (a spec it can't read, pybind11 finding no overload)."""
    return wheel_raised(exc)


def _build_object(what, object_spec):
    """The value object of an object spec (spec.value_object). What the library raises goes on
    to the caller, which reports it; anything else is the request's error."""
    try:
        return spec.value_object(object_spec)
    except Exception as exc:  # noqa: BLE001 - sorted into the library's and the request's
        if _library(exc) or isinstance(exc, UnicodeDecodeError):
            raise
        raise RequestError(f"{what}: {exc!r}") from exc


def _value(v, objects):
    """A call argument: config_api.value_in's values, and {"object": spec}, a value object."""
    if isinstance(v, dict) and set(v) == {"object"}:
        return _build_object("an object value", v["object"])
    if isinstance(v, list):
        return [_value(x, objects) for x in v]
    return value_in(v, objects)


def _write(value, blobs):
    """A value written out exactly: a processor as processor_ops writes it ({"class":
    "Processor", "processor": ...}), a config by serialize() ({"class": "Config",
    "serialize": {"bytes": hex}}), anything else by checks.dump (floats as their bits, objects
    with their getters and properties, arrays among the response blobs)."""
    if isinstance(value, OCIO.Processor):
        return {"class": "Processor", "processor": _processor_dump(value, blobs)}
    if isinstance(value, OCIO.Config):
        return {"class": "Config", "serialize": hex_bytes(value.serialize())}
    return dump(value, blobs)


# pybind11's translations of the C++ standard exceptions (std::invalid_argument, out_of_range,
# ...), which a binding call raises as these exact types. Python's own subclasses of them
# (RecursionError, a RuntimeError; UnicodeDecodeError, a ValueError, handled apart) never come
# from the library.
TRANSLATED = (RuntimeError, ValueError, IndexError, OverflowError, KeyError, StopIteration)


def _call_raised(exc):
    """Whether a call raised it in the library: OCIO's exceptions, or one of pybind11's
    translations, of its exact type."""
    return isinstance(exc, (OCIO.Exception, OCIO.ExceptionMissingFile)) or type(exc) in TRANSLATED


def _binding_target(target):
    """Whether a call may act on `target`, an allowlist: the PyOpenColorIO module or one of its
    helper modules, one of the binding's classes (spec.BINDING_TYPE: transforms, value objects,
    enums, iterators, vector_of_uint8_t), or an instance of one. Nothing else: Python's own
    values a call stored (lists, strings, floats, tuples, dicts, NumPy arrays), Python's
    classes the module holds (Exception, its module type), and their instances, whose methods'
    errors are never the library's. (vector_of_uint8_t is the binding's own: what its pop and
    remove raise comes from pybind11's bound vector, and is reported.)"""
    if isinstance(target, types.ModuleType):
        return target is OCIO or any(target is getattr(OCIO, name) for name in HELPER_MODULES)
    if isinstance(target, type):
        return type(target) is spec.BINDING_TYPE
    return type(type(target)) is spec.BINDING_TYPE


def _attempt(fn, stage, blobs):
    """{"result": fn() written out}, or what the library raised: {"exception", "stage"} or
    {"undecodable": hex, "stage"} ("call" when fn raised, "write" when writing the value out
    did). Returns the entry, and whether fn returned (with its value)."""
    try:
        value = fn()
    except UnicodeDecodeError as exc:
        return {"undecodable": bytes(exc.object).hex(), "stage": stage}, False, None
    except Exception as exc:  # noqa: BLE001 - sorted into the library's and the request's
        # The library's: OCIO's exceptions and pybind11's translations of the C++ standard
        # ones, of their exact types (a call acts on PyOpenColorIO targets only, so Python's
        # list and str methods can't raise here). Anything else is the request's error.
        if not _call_raised(exc):
            raise
        return {"exception": raised(exc), "stage": stage}, False, None
    first = len(blobs)
    try:
        return {"result": _write(value, blobs)}, True, value
    except UnicodeDecodeError as exc:
        del blobs[first:]
        return {"undecodable": bytes(exc.object).hex(), "stage": "write"}, True, value
    except Exception as exc:  # noqa: BLE001 - sorted into the library's and the request's
        if not _library(exc):
            raise
        del blobs[first:]
        return {"exception": raised(exc), "stage": "write"}, True, value


def _arguments(args, kwargs, objects):
    """A call's arguments, built from their value specs. What OCIO raises while building them (a
    transform's or a value object's constructor or setters: OCIO's exceptions, and the binding's
    std::runtime_error), and a string the binding can't decode, go on to the caller, which
    reports them; anything else is the request's error: a spec Python can't read, whatever it
    raises (IndexError, RecursionError, ...), and also pybind11's other translations of the C++
    standard exceptions (KeyError, IndexError, StopIteration, ValueError), which can't be told
    from Python's own here, so a request that makes the library raise one of them while its
    arguments are built is refused rather than reported."""
    try:
        return _value(args, objects), {k: _value(v, objects) for k, v in kwargs.items()}
    except RequestError:
        raise
    except Exception as exc:  # noqa: BLE001 - sorted into the library's and the request's
        if _library(exc) or isinstance(exc, UnicodeDecodeError):
            raise
        raise RequestError(f"a call's arguments: {type(exc).__name__}: {exc}") from exc


def _call_fn(i, call, objects):
    """The functions of a call: build() its arguments, and invoke(args, kwargs) what it calls;
    and the name it stores its result as."""
    kind = next((k for k in CALL_KEYS if isinstance(call, dict) and k in call), None)
    if kind is None:
        raise RequestError(f"calls[{i}]: {call!r} is none of {sorted(CALL_KEYS)}")
    check_keys(f"calls[{i}]", call, CALL_KEYS[kind])
    store = call.get("as")
    if store is not None and (not isinstance(store, str) or store in ("OCIO", *HELPER_MODULES)):
        raise RequestError(f"calls[{i}]: can't store as {store!r}")
    args, kwargs = call.get("args", []), call.get("kwargs", {})
    if not isinstance(args, list) or not isinstance(kwargs, dict):
        raise RequestError(f"calls[{i}]: args must be a list and kwargs an object")

    def build():
        return _arguments(args, kwargs, objects)

    if kind == "call":
        target = _target(call.get("on", "config"), objects)
        if not _binding_target(target):
            raise RequestError(f"calls[{i}]: a call acts on a PyOpenColorIO module, class or "
                               f"object, not a {type(target).__name__}")
        # An object's method comes from its class: an attribute set on the object (an object
        # spec's "attrs") is no method of the binding's.
        if (not isinstance(target, (type, types.ModuleType))
                and not callable(getattr(type(target), call["call"], None))):
            raise RequestError(f"calls[{i}]: {type(target).__name__} has no method "
                               f"{call['call']!r}")
        method = _method(target, call["call"])

        def invoke(call_args, call_kwargs):
            _check_current_config(target, call["call"], call_args, call_kwargs)
            return method(*call_args, **call_kwargs)
    elif kind == "new":
        cls = getattr(OCIO, call["new"], None) if isinstance(call["new"], str) else None
        if not isinstance(cls, type) or type(cls) is not spec.BINDING_TYPE:
            raise RequestError(f"calls[{i}]: {call['new']!r} isn't a class of the binding")

        def invoke(call_args, call_kwargs):
            return cls(*call_args, **call_kwargs)
    elif kind == "copy":
        source = _target(call["copy"], objects)

        def invoke(call_args, call_kwargs):
            return copy.deepcopy(source)
    else:
        source = _target(call["dump"], objects)

        def invoke(call_args, call_kwargs):
            return source
    return build, invoke, store


def _run_calls(calls, objects, log, blobs):
    """The calls in order (see object_calls); per call its entry and "log"."""
    if not isinstance(calls, list):
        raise RequestError(f"calls must be a list, not {calls!r}")
    results = []
    for i, call in enumerate(calls):
        build, invoke, store = _call_fn(i, call, objects)
        log.discard()
        try:
            built = build()
        except UnicodeDecodeError as exc:
            entry = {"undecodable": bytes(exc.object).hex(), "stage": "arguments"}
        except Exception as exc:  # noqa: BLE001 - _arguments lets only the library's through
            if not _library(exc):
                raise
            entry = {"exception": raised(exc), "stage": "arguments"}
        else:
            entry, returned, value = _attempt(lambda: invoke(*built), "call", blobs)
            if returned and store is not None:
                objects[store] = value
        entry["log"] = log.take()
        results.append(entry)
    return results


def _dump_processor(proc, blobs):
    """processor_ops' dump of a processor, field by field: {"processor": ...}, or the exception
    the library raised and the stage it raised in ("group": the getters and
    createGroupTransform, as processor_ops calls it)."""
    first = len(blobs)
    try:
        return {"processor": _processor_dump(proc, blobs)}
    except UnicodeDecodeError as exc:
        del blobs[first:]
        return {"undecodable": bytes(exc.object).hex(), "stage": "group"}
    except Exception as exc:  # noqa: BLE001 - sorted into the library's and the request's
        if not _library(exc):
            raise
        del blobs[first:]
        return {"exception": raised(exc), "stage": "group"}


@command
def object_calls(args, blobs):
    """A config and value objects, then calls on them and on the objects they give.

    args:
      config    the source, as config_calls takes it (default "raw"); stored as "config"
      objects   {name: object spec} (spec.py: {"class", "args" or "factory", "attrs",
                "calls"}), built in order after the config and stored under their names: the
                way to make a value whose fields the binding sets as attributes (GradingPrimary,
                GradingTone, ...). "config", "OCIO" and the helper modules' names are refused
      env, files
                the environment and the directory of the whole request, as config_calls takes
                them
      calls     in order, each one of
                  {"call": method, "on": name (default "config"), "args": [values],
                   "kwargs": {name: value}, "as": name}
                            on.method(*args, **kwargs): "on" names a stored PyOpenColorIO
                            object (not a list, string or number a call returned), a
                            PyOpenColorIO class (its static methods), "OCIO" (config_api's
                            MODULE_FUNCTIONS), "DisplayViewHelpers" or "ColorSpaceHelpers"
                            (those modules' functions)
                  {"new": class, "args", "kwargs", "as"}
                            a PyOpenColorIO class's constructor: ColorSpaceMenuParameters
                            (config), ColorSpaceMenuHelper(parameters), LegacyViewingPipeline(),
                            MixingColorSpaceManager(config), a grading value, ...
                  {"copy": name, "as": name}   copy.deepcopy (createEditableCopy)
                  {"dump": name}               the stored object, written out
                values as config_api.value_in reads them ({"enum"}, {"transform"}, {"f64"},
                {"bytes"}, {"ref": name}, {"map"}), and {"object": spec}, a value object; "as"
                stores the result, also when writing it out raised
      processors
                names of Processors the calls stored, written out in this order after the
                calls, as processor_ops writes a processor
    result:
      dir         the request directory's absolute path
      config      null, or {"exception"} or {"undecodable"} when making the config or a value
                  object raised in the library (no call runs then)
      config_log  OCIO's log while making them: [{"bytes": hex}]
      objects     {name: {"result": the value object written out}, or the exception
                  writing it out raised, as a call's; and "log"}, when they were made
      calls       per call {"result": the value written out (see below)}, or {"exception":
                  {"type", "message": {"bytes": hex}}, "stage"} or {"undecodable": hex,
                  "stage"} where the library raised or a string couldn't be decoded: stage
                  "arguments" (building the call's arguments: OCIO's exceptions and the
                  binding's RuntimeError from a transform's or a value object's constructor
                  or setters; pybind11's other translated exceptions there refuse the
                  request), "call" (the call; "as" stores nothing when the arguments or the
                  call raised) or "write" (writing its value out; "as" still stores the value
                  then); and "log"
      processors  per name, {"processor": {"cache_id", "isNoOp", "hasChannelCrosstalk",
                  "isDynamic", "processor_metadata", "group"}}, or {"exception", "stage":
                  "group"}; and "log"
    Values written out: a processor as {"class": "Processor", "processor": as above}, a
    config as {"class": "Config", "serialize": {"bytes": hex}}, anything else as checks.dump
    writes it: floats as {"f64": bits}, enums by name, objects as {"class", "getters",
    "properties", "uncalled"} (a grading value's fields are properties), arrays as indices
    among the response blobs.
    blobs: the arrays the written values hold, in order

    A call (not a spec's calls) may write a file the request names, as config_calls' calls may:
    Config.serialize(fileName), GroupTransform.write(format, fileName, config). The requests
    are the project's own tests, and one needs it (file_transform_cache_oracle.rs rewrites the
    LUT file it reads that way); a spec's calls can't (spec.REFUSED_CALLS).

    Every request the command can't run exactly is refused (the call fails): an unknown key or
    call kind, a name no call stored, a value or object spec it can't read (whatever Python
    raises reading it), arguments no overload takes (pybind11's TypeError), and a name in
    processors that doesn't hold a Processor.
    """
    check_keys("object_calls", args, {"config", "objects", "env", "files", "calls",
                                      "processors"})
    source = args.get("config", "raw")
    check_source(source)
    specs = args.get("objects", {})
    if not isinstance(specs, dict) or any(name in RESERVED for name in specs):
        raise RequestError(f"objects must be an object of name: object spec, without the "
                           f"names {RESERVED}: {specs!r}")
    processors = args.get("processors", [])
    if not isinstance(processors, list) or not all(isinstance(n, str) for n in processors):
        raise RequestError(f"processors must be a list of names, not {processors!r}")

    result, out = {}, []
    with directory(args.get("files")) as root, environment(args.get("env")), \
            LogCapture() as log:
        result["dir"] = root
        objects = {name: getattr(OCIO, name) for name in HELPER_MODULES}
        try:
            objects["config"] = make_config(source)
            for name, object_spec in specs.items():
                objects[name] = _build_object(f"objects[{name!r}]", object_spec)
            result["config"] = None
        except UnicodeDecodeError as exc:
            result["config"] = {"undecodable": bytes(exc.object).hex()}
        except REPORTED as exc:
            result["config"] = {"exception": raised(exc)}
        result["config_log"] = log.take()
        if result["config"] is not None:
            result.update(objects={}, calls=[], processors=[])
            return result, out
        result["objects"] = {}
        for name in specs:
            entry = _attempt(lambda o=objects[name]: o, "write", out)[0]
            entry["log"] = log.take()
            result["objects"][name] = entry
        result["calls"] = _run_calls(args.get("calls", []), objects, log, out)
        result["processors"] = []
        for name in processors:
            proc = objects.get(name)
            if not isinstance(proc, OCIO.Processor):
                raise RequestError(f"processors: {name!r} isn't a Processor a call stored")
            log.discard()
            entry = _dump_processor(proc, out)
            entry["log"] = log.take()
            result["processors"].append(entry)
    return result, out
