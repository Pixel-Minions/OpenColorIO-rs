# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Walks PyOpenColorIO's surface and prints it as JSON (py_surface_api.py's report). A plain
script: it imports nothing of the oracle, so the port's environment runs the same file
against the port's module, and the two reports compare.

usage: python -I -B -X utf8 py_surface_walk.py > report.json

It runs in a process of its own: reading some attributes changes what a class holds (in
CPython 3.13, reading __annotations__ of a class adds it to the class's dictionary), so a
walk in a process that has used the module could report more than the module defines.

The report (py_surface_api.py's docstring describes it) holds every name of the compiled
module PyOpenColorIO.PyOpenColorIO, in the order its dictionary holds them (the order the
binding defined them in), recursively through its classes and its submodules, and the names
the package PyOpenColorIO adds. A class's members are its own dictionary's: what it inherits
from outside the module (pybind11's pybind11_object and its metatype pybind11_type, Python's
object) is not reported, beyond the class's bases and MRO.
"""

import json
import os
import sys
import types

# Names the interpreter (or pybind11, for a submodule) gives every module, which say nothing
# about the binding: reported by name only ("runtime_names"), since some hold paths.
RUNTIME_NAMES = ("__builtins__", "__loader__", "__spec__", "__file__", "__cached__",
                 "__path__", "__package__", "__name__", "__doc__")
CLASS_SKIP = {"__doc__", "__module__", "__dict__", "__weakref__", "__qualname__"}


class Refused(Exception):
    """A value the report can't hold."""


def _text(s):
    return {"bytes": s.encode("utf-8", "surrogatepass").hex()}


def _doc(obj):
    doc = obj.__doc__
    return None if doc is None else _text(doc)


def _qualified(cls):
    return f"{cls.__module__}.{cls.__qualname__}"


def _value(v):
    """A value that isn't a class, a module or a function: its type and how it reads."""
    out = {"kind": "value", "type": _qualified(type(v))}
    if isinstance(v, str):
        out["text"] = _text(v)
    elif isinstance(v, bool) or v is None:
        out["repr"] = _text(repr(v))
    elif type(v) is int:
        out["int"] = str(v)
    elif isinstance(v, float):
        out["repr"] = _text(repr(v))
    elif type(v).__name__ == "PyCapsule":
        # pybind11's own capsules (a class's "__pybind11_module_local_..." record): only their
        # type; their repr is an address.
        pass
    else:
        # Enum values (pybind11's repr is "<Enum.NAME: n>") and pybind11's own entries
        # tables: their repr is deterministic; any other object whose repr holds an address
        # refuses the request rather than be reported.
        r = repr(v)
        if " at 0x" in r:
            raise Refused(f"py_surface: {r!r} has no stable repr")
        out["repr"] = _text(r)
        if hasattr(type(v), "__members__") and hasattr(v, "value"):
            out["enum_value"] = str(int(v.value))
    return out


def _qualname(f):
    """A callable's __qualname__ (a builtin function's names the type of its __self__), or
    None when it has none."""
    q = getattr(f, "__qualname__", None)
    return _text(q) if isinstance(q, str) else None


def _function(v):
    """A function, method, static method or property: its kind, docstrings and qualified
    names."""
    out = {"kind": type(v).__name__}
    if isinstance(v, property):
        out["doc"] = _doc(v)
        out["getter"] = v.fget is not None
        out["setter"] = v.fset is not None
        out["deleter"] = v.fdel is not None
        for name, f in (("getter", v.fget), ("setter", v.fset)):
            if f is not None:
                out[name + "_doc"] = _doc(f)
                out[name + "_qualname"] = _qualname(f)
    elif isinstance(v, (staticmethod, classmethod)):
        out["function_kind"] = type(v.__func__).__name__
        out["doc"] = _doc(v.__func__)
        out["qualname"] = _qualname(v.__func__)
    else:
        out["doc"] = _doc(v)
        out["qualname"] = _qualname(v)
    return out


def _is_function(v):
    return isinstance(v, (property, staticmethod, classmethod)) or (
        callable(v) and not isinstance(v, type))


def _class(cls, path, seen):
    out = {
        "kind": "class",
        "metatype": _qualified(type(cls)),
        "module": _text(cls.__module__),
        "qualname": _text(cls.__qualname__),
        "doc": _doc(cls),
        "bases": [_qualified(b) for b in cls.__bases__],
        "mro": [_qualified(b) for b in cls.__mro__],
        "members": _members(vars(cls), path, seen, CLASS_SKIP),
    }
    if "__members__" in cls.__dict__:
        # A pybind11 enum (__members__ is a static property of the class): its entries in
        # order, by name and value.
        out["entries"] = [[name, str(int(value))] for name, value in cls.__members__.items()]
    return out


def _module(module, path, seen):
    names = vars(module)
    return {
        "kind": "module",
        "module_name": _text(module.__name__),
        "doc": _doc(module),
        "runtime_names": [n for n in names if n in RUNTIME_NAMES],
        "members": _members(names, path, seen, set(RUNTIME_NAMES)),
    }


def _members(namespace, path, seen, skip):
    out = []
    for name, v in namespace.items():
        if name in skip:
            continue
        member_path = f"{path}.{name}"
        if isinstance(v, (type, types.ModuleType)):
            if id(v) in seen:
                out.append({"name": _text(name), "kind": "class_ref", "target": seen[id(v)]})
                continue
            seen[id(v)] = member_path
            if isinstance(v, type):
                entry = _class(v, member_path, seen)
            else:
                entry = _module(v, member_path, seen)
        elif _is_function(v):
            entry = _function(v)
        else:
            entry = _value(v)
        entry["name"] = _text(name)
        out.append(entry)
    return out


def surface():
    """The report, or Refused."""
    import PyOpenColorIO as package
    import PyOpenColorIO.PyOpenColorIO as module
    seen = {id(module): "PyOpenColorIO"}
    module_names = set(vars(module))
    return {
        "module": _module(module, "PyOpenColorIO", seen),
        # The package's own names, by type only: on Windows its __init__.py leaves the paths
        # of its installation behind ("here", "bin_dir"), which differ from machine to
        # machine; on both platforms it holds the compiled module (PyOpenColorIO).
        "package": [
            {"name": _text(n), "type": _qualified(type(v))}
            for n, v in vars(package).items()
            if n not in module_names and n not in RUNTIME_NAMES
        ],
    }


def main():
    # Only this script writes to stdout (file descriptor 1 becomes stderr until the report).
    sys.stdout.flush()
    report = os.fdopen(os.dup(1), "w", encoding="utf-8")
    os.dup2(2, 1)
    sys.stdout = sys.stderr
    try:
        out = surface()
    except Refused as exc:
        out = {"refused": str(exc)}
    report.write(json.dumps(out))
    report.close()


if __name__ == "__main__":
    main()
