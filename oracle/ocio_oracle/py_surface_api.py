# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for the Python module's surface (card p6-skeleton, O6.1,
docs/cards/phase6.md).

- py_surface: every name of the wheel's compiled module, PyOpenColorIO.PyOpenColorIO, walked
  recursively through its classes and submodules, and the names the package PyOpenColorIO
  adds to it, by py_surface_walk.py in a new Python process.

It reports what the module holds, in the order its dictionaries hold it (the order pybind11
defined the names in), and never computes expected values. Text is reported as bytes:
{"bytes": hex} of its UTF-8.
"""

import json
import os
import subprocess
import sys

from .commands import command
from .config_api import RequestError

_WALK = os.path.join(os.path.dirname(os.path.abspath(__file__)), "py_surface_walk.py")


@command
def py_surface(args, blobs):
    """The surface of the wheel's Python module.

    args:     {} (none)
    result:   {"module": module entry (PyOpenColorIO.PyOpenColorIO, without "name"),
               "package": [{"name", "type"}, ...]}
              - package: the names the package PyOpenColorIO has besides those of the
                compiled module and the interpreter's own (its __init__.py's), in order, by
                type only: on Windows "here" and "bin_dir", paths of the installation, and
                on both platforms "PyOpenColorIO", the compiled module;
              - an entry is {"name": text, "kind": ...} and, by kind, exactly these keys:
                module: "module_name" (__name__), "doc", "runtime_names" (the names of
                  py_surface_walk.RUNTIME_NAMES it holds, in order, without their values,
                  which may be paths), "members" (entries, its dictionary in order without
                  the runtime names);
                class: "metatype", "module", "qualname", "doc", "bases", "mro" (qualified
                  names), "members" (entries, the class's own dictionary in order, without
                  __doc__, __module__, __dict__, __weakref__ and __qualname__; what it
                  inherits from outside the module, such as pybind11_object's members, is
                  not reported), and for a pybind11 enum "entries": [[name, value], ...] in
                  __members__ order;
                class_ref: "target", the path where that class or module was first reported
                  (a class or a module reached twice);
                property, pybind11_static_property: "doc", "getter", "setter", "deleter",
                  and "getter_doc", "getter_qualname", "setter_doc", "setter_qualname" for
                  the accessors it has;
                staticmethod, classmethod: "function_kind", "doc", "qualname" (its
                  function's);
                any other callable (instancemethod, builtin_function_or_method,
                  wrapper_descriptor, ...): "doc", "qualname".
                A qualname is the callable's __qualname__ (null when it has none): a
                builtin's names the class of its __self__, pybind11's function record, whose
                name carries pybind11's ABI tag, which differs between the Windows and Linux
                wheels.
                value: "type", and "text" (str), "int" (int), nothing more (a capsule, whose
                  repr is an address), or "repr" (anything else; an enum value also has
                  "enum_value").
              Text is {"bytes": hex}; a doc of None is null.
    blobs:    none

    The walk runs in a new process (`python -I -B -X utf8`), whatever ran before in the
    oracle's: reading a class's __annotations__ adds it to the class's dictionary (CPython
    3.13). Its environment holds nothing of the caller's (only SYSTEMROOT, on Windows). An
    object whose repr holds an address refuses the request.
    """
    if args:
        raise RequestError(f"py_surface: takes no arguments, got {sorted(args)}")
    env = {}
    if os.name == "nt":
        env["SYSTEMROOT"] = os.environ["SYSTEMROOT"]
    child = subprocess.run([sys.executable, "-I", "-B", "-X", "utf8", _WALK],
                           capture_output=True, env=env, timeout=600, check=False)
    if child.returncode != 0:
        raise RuntimeError("py_surface: the walk failed: "
                           + child.stderr.decode("utf-8", "backslashreplace"))
    report = json.loads(child.stdout.decode("utf-8"))
    if "refused" in report:
        raise RequestError(report["refused"])
    return report, []
