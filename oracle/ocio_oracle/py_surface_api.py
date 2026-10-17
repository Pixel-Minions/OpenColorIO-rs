# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for the Python module's surface (card p6-skeleton, O6.1,
docs/cards/phase6.md).

- py_surface: every name of the wheel's compiled module, PyOpenColorIO.PyOpenColorIO, walked
  recursively through its classes, and the names the package PyOpenColorIO adds to it.

It reports what the module holds, in the order its dictionaries hold it (the order pybind11
defined the names in), and never computes expected values. Text is reported as bytes:
{"bytes": hex} of its UTF-8.
"""

import importlib

from .commands import command

# Names the interpreter gives every module, which say nothing about the binding.
_MODULE_RUNTIME_NAMES = {"__builtins__", "__loader__", "__spec__", "__file__", "__cached__",
                         "__path__", "__package__"}


def _text(s):
    return {"bytes": s.encode("utf-8").hex()}


def _qualified(cls):
    return f"{cls.__module__}.{cls.__qualname__}"


def _value(v):
    """A value that isn't a class or a function: its type and how it reads."""
    out = {"kind": "value", "type": _qualified(type(v))}
    if isinstance(v, str):
        out["text"] = _text(v)
    elif isinstance(v, bool) or v is None:
        out["repr"] = _text(repr(v))
    elif isinstance(v, int) and type(v) is int:
        out["int"] = str(v)
    elif isinstance(v, float):
        out["repr"] = _text(repr(v))
    elif type(v).__name__ == "PyCapsule":
        # pybind11's own capsules (a class's "__pybind11_module_local_..." record): only their
        # type; their repr is an address.
        pass
    else:
        # Enum values (pybind11's repr is "<Enum.NAME: n>") and pybind11's own entries
        # tables: repr is deterministic for them; any other object whose repr holds an
        # address refuses the request rather than be reported.
        r = repr(v)
        if " at 0x" in r:
            raise ValueError(f"py_surface: {r!r} has no stable repr")
        out["repr"] = _text(r)
        if hasattr(type(v), "__members__") and hasattr(v, "value"):
            out["enum_value"] = str(int(v.value))
    return out


def _function(v):
    """A function, method, static method or property: its kind and docstrings."""
    kind = type(v).__name__
    out = {"kind": kind}
    if isinstance(v, property):
        out["doc"] = None if v.__doc__ is None else _text(v.__doc__)
        out["getter"] = v.fget is not None
        out["setter"] = v.fset is not None
        out["deleter"] = v.fdel is not None
        for name, f in (("getter_doc", v.fget), ("setter_doc", v.fset)):
            if f is not None:
                out[name] = None if f.__doc__ is None else _text(f.__doc__)
    elif isinstance(v, (staticmethod, classmethod)):
        f = v.__func__
        out["function_kind"] = type(f).__name__
        out["doc"] = None if f.__doc__ is None else _text(f.__doc__)
    else:
        out["doc"] = None if v.__doc__ is None else _text(v.__doc__)
    return out


def _is_function(v):
    return isinstance(v, (property, staticmethod, classmethod)) or callable(v) and not isinstance(v, type)


def _class(cls, path, seen):
    out = {
        "kind": "class",
        "metatype": _qualified(type(cls)),
        "module": _text(cls.__module__),
        "qualname": _text(cls.__qualname__),
        "doc": None if cls.__doc__ is None else _text(cls.__doc__),
        "bases": [_qualified(b) for b in cls.__bases__],
        "mro": [_qualified(b) for b in cls.__mro__],
        "members": _members(vars(cls), path, seen, skip={"__doc__", "__module__", "__dict__",
                                                         "__weakref__", "__qualname__"}),
    }
    if "__members__" in cls.__dict__:
        # A pybind11 enum (__members__ is a static property of the class): its entries in
        # order, by name and value.
        out["entries"] = [[name, str(int(value))] for name, value in cls.__members__.items()]
    return out


def _members(namespace, path, seen, skip):
    out = []
    for name, v in namespace.items():
        if name in skip:
            continue
        member_path = f"{path}.{name}"
        if isinstance(v, type):
            if id(v) in seen:
                out.append({"name": _text(name), "kind": "class_ref",
                            "target": seen[id(v)]})
                continue
            seen[id(v)] = member_path
            entry = _class(v, member_path, seen)
        elif _is_function(v):
            entry = _function(v)
        else:
            entry = _value(v)
        entry["name"] = _text(name)
        out.append(entry)
    return out


@command
def py_surface(args, blobs):
    """The surface of the wheel's Python module.

    args:     {} (none)
    result:   {"module": {"name", "doc", "members": [entry, ...]}, "package": [entry, ...]}
              - module: PyOpenColorIO.PyOpenColorIO, the compiled module, its names in the
                order its dictionary holds them (the interpreter's own names left out);
              - package: the names the package PyOpenColorIO has besides those it imports
                from the compiled module (its own __init__.py's), in order, each as
                {"name", "type"} (their values are paths of the installation);
              - entry: {"name": text, "kind": ...} and, by kind:
                class: "metatype", "module", "qualname", "doc", "bases", "mro" (qualified
                  names), "members" (entries, the class's own dictionary in order, without
                  __doc__, __module__, __dict__, __weakref__ and __qualname__), and for a
                  pybind11 enum "entries": [[name, value], ...] in __members__ order;
                class_ref: "target", the path where that class was first reported;
                instancemethod, builtin_function_or_method, function, ...: "doc";
                staticmethod, classmethod: "function_kind", "doc";
                property: "doc", "getter", "setter", "deleter", "getter_doc", "setter_doc";
                value: "type" and "text" (str), "int" (int), "repr" (anything else but a
                  capsule; an enum value also has "enum_value").
              Text is {"bytes": hex}; a doc of None is null.
    blobs:    none
    """
    if args:
        raise ValueError(f"py_surface: takes no arguments, got {sorted(args)}")
    package = importlib.import_module("PyOpenColorIO")
    module = importlib.import_module("PyOpenColorIO.PyOpenColorIO")
    seen = {}
    module_members = _members(vars(module), "PyOpenColorIO", seen,
                              skip=_MODULE_RUNTIME_NAMES | {"__name__", "__doc__"})
    module_names = set(vars(module))
    # The package's own names, by type only: on Windows its __init__.py leaves the paths of
    # its installation behind ("here", "bin_dir"), which differ from machine to machine.
    package_members = [
        {"name": _text(n), "type": _qualified(type(v))}
        for n, v in vars(package).items()
        if n not in module_names and n not in _MODULE_RUNTIME_NAMES
    ]
    return {
        "module": {"name": _text(module.__name__),
                   "doc": None if module.__doc__ is None else _text(module.__doc__),
                   "members": module_members},
        "package": package_members,
    }, []
