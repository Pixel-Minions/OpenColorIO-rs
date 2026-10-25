# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Runs a script of Python steps against PyOpenColorIO and reports each step's result as JSON.
A plain script: it imports nothing of the oracle, so the port's environment runs the same file
against the port's module (card p6-skeleton), and the two reports compare.

usage: python -I -B -X utf8 py_eval_steps.py < script.json > report.json

One script per process: whatever a script changes in the process (the current config, the
logging level, environment variables, the working directory, attributes of modules, OCIO's
caches, NumPy's print options) ends with it. py_eval_api.py starts one process per script.
A script may start threads (`threading`, for calls that meet another thread's call); it joins
them before it ends, else its process waits for them at exit.

A script is {"steps": [step, ...]}, run in order, with `import PyOpenColorIO as OCIO` done
first. A step is one of
  {"eval": expr}                                   the value of expr
  {"call": expr, "args": [expr, ...], "kwargs": [[name, expr], ...]}
                                                   expr(*args, **kwargs); args and kwargs
                                                   may be left out
  {"setattr": expr, "name": name, "value": expr}   setattr(expr, name, value); result None
  {"delattr": expr, "name": name}                  delattr(expr, name); result None
  {"setitem": expr, "index": expr, "value": expr}  expr[index] = value; result None
and may add "getters": [name, ...]: on success, each is read from the result (and called
without arguments when callable) and reported as a value of its own.

An expr is one of
  {"ref": i}                   the result of step i of this script (which must have succeeded)
  {"name": "a.b.c"}            a dotted name: "OCIO" (the package PyOpenColorIO), one of
                               BUILTINS, or one of MODULES, then attributes
  {"ref": i, "attr": "b.c"}    attributes of step i's result
  {"int": "decimal"}  {"float": "0x<16 hex digits>", the IEEE 754 binary64 bits}
  {"str": "<hex>", UTF-8 with lone surrogates kept (surrogatepass)}  {"bytes": "<hex>"}
  {"bool": true|false}  {"none": null}
  {"list": [expr, ...]}  {"tuple": [expr, ...]}  {"dict": [[key expr, value expr], ...]}
  {"array": {"dtype": "<f4", "shape": [...], "bytes": "<hex>"}}   a NumPy array (C order)
  {"scalar": {"dtype": "<f4", "bytes": "<hex>"}}                  a NumPy scalar
The names reach only what is listed: no file, process or import function. That is a guard
against mistakes in the tests' own scripts, not a sandbox: attributes reach much further
(NumPy writes files, and dunder attributes reach the import machinery). py_eval_api.py also
runs each script in a directory of its own, with an environment that holds nothing of the
caller's (SYSTEMROOT only, on Windows).

Report: {"interpreter": {"isolated", "dont_write_bytecode", "utf8_mode"} (sys.flags),
"steps": [result, ...]}. A result is
  {"value": value, "same_as": [i, ...], "getters": [[name, outcome], ...], "log": [text, ...]}
or {"raised": exception, "log": [...]}: "same_as" lists the earlier steps whose result is this
very object (every result stays alive until the script ends, so identity is id() equality;
immutable builtins, which the interpreter may share whoever made them, list none); "log" holds
the library's messages during the step (SetLoggingFunction, set before the first step), from
any thread: a message another thread logs is the log of the step running when it arrives; an
outcome is {"value": value} or {"raised": exception}.

A value is {"type": qualified name, "repr": text} (or "repr_raised": exception, when repr()
raises) and, by type:
  bool: "bool"; int: "int" (decimal); float: "float" (binary64 bits); str: "str" (text);
  bytes, bytearray: "bytes" (hex); list, tuple: "items" (values, 8 levels deep); dict:
  "items" ([key, value] pairs); a pybind11 enum value: "enum": [name, int value]; a NumPy
  array: "array": {"dtype", "shape", "strides", "bytes"}; a NumPy scalar: "scalar": {"dtype",
  "bytes"}.
An exception is {"type", "mro" (qualified names), "message": text of str(exc)} (or
"message_raised": the type of what str() raised).
Text is {"bytes": hex} of its UTF-8 (surrogatepass). In a repr or a message, a memory address
(" at 0x...") reads " at 0x?", and "address" is true: the port prints its own (P6-7).
"""

import builtins
import json
import os
import re
import sys

MODULES = ("atexit", "copy", "math", "numpy", "operator", "threading")
BUILTINS = frozenset([
    "abs", "bool", "bytearray", "bytes", "complex", "dict", "divmod", "enumerate", "float",
    "frozenset", "getattr", "hasattr", "hash", "int", "isinstance", "issubclass", "iter", "len",
    "list", "max", "min", "next", "object", "pow", "range", "repr", "reversed", "round", "set",
    "setattr", "slice", "sorted", "str", "sum", "tuple", "type", "zip",
] + [n for n, v in vars(builtins).items()
     if isinstance(v, type) and issubclass(v, BaseException)])
_ADDRESS = re.compile(r" at 0x[0-9A-Fa-f]+")
_IMMUTABLE = (type(None), bool, int, float, complex, str, bytes, tuple, frozenset)


class RequestError(Exception):
    """A request the interpreter can't read."""


def _text(s):
    return {"bytes": s.encode("utf-8", "surrogatepass").hex()}


def _masked(s):
    masked, n = _ADDRESS.subn(" at 0x?", s)
    return masked, n > 0


def _qualified(cls):
    return f"{cls.__module__}.{cls.__qualname__}"


def _float_bits(x):
    import struct
    return "0x" + struct.pack(">d", x).hex()


def _value(v, depth=0):
    out = {"type": _qualified(type(v))}
    try:
        r = repr(v)
    except Exception as exc:  # noqa: BLE001 - reported in place of the repr
        out["repr_raised"] = _exception(exc)
    else:
        r, address = _masked(r)
        out["repr"] = _text(r)
        if address:
            out["address"] = True
    if isinstance(v, bool):
        out["bool"] = v
    elif isinstance(v, int) and type(v).__module__ == "builtins":
        out["int"] = str(v)
    elif isinstance(v, float) and type(v).__module__ == "builtins":
        out["float"] = _float_bits(v)
    elif isinstance(v, str):
        out["str"] = _text(v)
    elif isinstance(v, (bytes, bytearray)):
        out["bytes"] = bytes(v).hex()
    elif isinstance(v, (list, tuple)) and depth < 8:
        out["items"] = [_value(x, depth + 1) for x in v]
    elif isinstance(v, dict) and depth < 8:
        out["items"] = [[_value(k, depth + 1), _value(x, depth + 1)] for k, x in v.items()]
    elif type(v).__module__ == "numpy":
        import numpy
        if isinstance(v, numpy.ndarray):
            out["array"] = {"dtype": v.dtype.str, "shape": list(v.shape),
                            "strides": list(v.strides), "bytes": v.tobytes().hex()}
        elif isinstance(v, numpy.generic):
            out["scalar"] = {"dtype": v.dtype.str, "bytes": v.tobytes().hex()}
    elif "__members__" in type(v).__dict__ and hasattr(v, "name"):
        # A pybind11 enum value.
        out["enum"] = [v.name, int(v)]
    return out


def _exception(exc):
    out = {"type": _qualified(type(exc)), "mro": [_qualified(c) for c in type(exc).__mro__]}
    try:
        message = str(exc)
    except Exception as inner:  # noqa: BLE001 - reported in place of the message
        out["message_raised"] = _qualified(type(inner))
    else:
        message, address = _masked(message)
        out["message"] = _text(message)
        if address:
            out["address"] = True
    return out


def _hex(arg):
    if not (isinstance(arg, str) and re.fullmatch(r"(?:[0-9a-f]{2})*", arg)):
        raise ValueError("not lower-case hex")
    return bytes.fromhex(arg)


def _literal(kind, arg):
    """A literal expression's value; ValueError, TypeError or KeyError for a bad one."""
    if kind == "int":
        if not (isinstance(arg, str) and re.fullmatch(r"-?(?:0|[1-9][0-9]*)", arg)):
            raise ValueError("not a decimal")
        return int(arg)
    if kind == "float":
        import struct
        if not (isinstance(arg, str) and re.fullmatch(r"0x[0-9a-f]{16}", arg)):
            raise ValueError("not 0x and 16 hex digits")
        return struct.unpack(">d", bytes.fromhex(arg[2:]))[0]
    if kind == "str":
        return _hex(arg).decode("utf-8", "surrogatepass")
    if kind == "bytes":
        return _hex(arg)
    if kind == "bool" and isinstance(arg, bool):
        return arg
    if kind == "none" and arg is None:
        return None
    if kind in ("array", "scalar"):
        import numpy
        if set(arg) != ({"dtype", "bytes"} | ({"shape"} if kind == "array" else set())):
            raise ValueError("bad keys")
        dtype = numpy.dtype(arg["dtype"])
        values = numpy.frombuffer(_hex(arg["bytes"]), dtype=dtype)
        if kind == "scalar":
            if len(values) != 1:
                raise ValueError("not one value")
            return values[0]
        return values.reshape(arg["shape"]).copy()
    raise ValueError("unknown kind")


class _Script:
    def __init__(self, ocio):
        self.ocio = ocio
        self.results = []  # (ok, object), kept alive until the script ends

    def _root(self, name):
        if name == "OCIO":
            return self.ocio
        if name in MODULES:
            return __import__(name)
        if name in BUILTINS:
            return getattr(builtins, name)
        raise RequestError(f"unknown name {name!r}")

    def _ref(self, i):
        if not isinstance(i, int) or isinstance(i, bool) or not 0 <= i < len(self.results):
            raise RequestError(f"no step {i!r} before this one")
        ok, v = self.results[i]
        if not ok:
            raise RequestError(f"step {i} raised")
        return v

    def expr(self, e):
        if not isinstance(e, dict) or not e:
            raise RequestError(f"bad expression {e!r}")
        keys = set(e)
        if keys == {"ref"}:
            return self._ref(e["ref"])
        if keys == {"ref", "attr"}:
            if not isinstance(e["attr"], str):
                raise RequestError(f"bad expression {e!r}")
            v = self._ref(e["ref"])
            for part in e["attr"].split("."):
                v = getattr(v, part)
            return v
        if len(keys) != 1:
            raise RequestError(f"bad expression {e!r}")
        (kind, arg), = e.items()
        if kind == "name":
            if not isinstance(arg, str):
                raise RequestError(f"bad expression {e!r}")
            first, *rest = arg.split(".")
            v = self._root(first)
            for part in rest:
                v = getattr(v, part)
            return v
        if kind == "list" and isinstance(arg, list):
            return [self.expr(x) for x in arg]
        if kind == "tuple" and isinstance(arg, list):
            return tuple(self.expr(x) for x in arg)
        if kind == "dict" and isinstance(arg, list) and all(
                isinstance(p, list) and len(p) == 2 for p in arg):
            return {self.expr(k): self.expr(x) for k, x in arg}
        try:
            return _literal(kind, arg)
        except (ValueError, TypeError, KeyError) as exc:
            raise RequestError(f"bad expression {e!r}: {exc}") from None

    @staticmethod
    def check(s):
        """The step's kind, or RequestError."""
        if not isinstance(s, dict):
            raise RequestError(f"bad step {s!r}")
        kinds = [k for k in ("eval", "call", "setattr", "delattr", "setitem") if k in s]
        if len(kinds) != 1:
            raise RequestError(f"bad step {s!r}")
        kind = kinds[0]
        allowed = {kind, "getters"} | {
            "call": {"args", "kwargs"}, "setattr": {"name", "value"}, "delattr": {"name"},
            "setitem": {"index", "value"}}.get(kind, set())
        required = allowed - {"getters", "args", "kwargs"}
        if not required <= set(s) <= allowed:
            raise RequestError(f"bad step {s!r}")
        names = list(s.get("getters", [])) + [s[k] for k in ("name",) if k in s]
        pairs = s.get("kwargs", [])
        if not (isinstance(s.get("getters", []), list) and isinstance(s.get("args", []), list)
                and isinstance(pairs, list)
                and all(isinstance(p, list) and len(p) == 2 for p in pairs)):
            raise RequestError(f"bad step {s!r}")
        names += [p[0] for p in pairs]
        if not all(isinstance(n, str) for n in names):
            raise RequestError(f"bad step {s!r}")
        return kind

    def step(self, s, kind):
        """Runs the step: its result, or the exception it raises (an attribute that a name
        reaches, a call, ...), except RequestError."""
        target = self.expr(s[kind])
        if kind == "eval":
            return target
        if kind == "call":
            args = [self.expr(a) for a in s.get("args", [])]
            kwargs = {}
            for name, value in s.get("kwargs", []):
                kwargs[name] = self.expr(value)
            return target(*args, **kwargs)
        if kind == "setattr":
            return setattr(target, s["name"], self.expr(s["value"]))
        if kind == "delattr":
            return delattr(target, s["name"])
        index, value = self.expr(s["index"]), self.expr(s["value"])
        target[index] = value
        return None


def run(script):
    """The report of a script (a dict), or RequestError."""
    if not isinstance(script, dict) or set(script) != {"steps"} or not isinstance(
            script["steps"], list):
        raise RequestError(f"bad script {script!r}")
    import PyOpenColorIO as ocio
    state = _Script(ocio)
    out = []
    log = []
    ocio.SetLoggingFunction(log.append)
    try:
        for s in script["steps"]:
            kind = state.check(s)
            del log[:]
            try:
                v = state.step(s, kind)
            except RequestError:
                raise
            except Exception as exc:  # noqa: BLE001 - the step's exception is its result
                state.results.append((False, exc))
                out.append({"raised": _exception(exc), "log": [_text(m) for m in log]})
                continue
            same = [] if isinstance(v, _IMMUTABLE) else [
                i for i, (ok, w) in enumerate(state.results) if ok and w is v]
            state.results.append((True, v))
            result = {"value": _value(v), "same_as": same}
            getters = []
            for name in s.get("getters", []):
                try:
                    g = getattr(v, name)
                    g = g() if callable(g) else g
                except Exception as exc:  # noqa: BLE001 - reported as the getter's outcome
                    getters.append([name, {"raised": _exception(exc)}])
                else:
                    getters.append([name, {"value": _value(g)}])
            result["getters"] = getters
            result["log"] = [_text(m) for m in log]
            out.append(result)
    finally:
        # Also needed for the process to end cleanly: the wheel keeps the logging function and
        # the hash function in static objects, released after the interpreter has finalized,
        # and a Python function still set then crashes the process on its way out.
        ocio.ResetToDefaultLoggingFunction()
        ocio.ResetComputeHashFunction()
    flags = sys.flags
    return {"interpreter": {"isolated": flags.isolated,
                            "dont_write_bytecode": flags.dont_write_bytecode,
                            "utf8_mode": flags.utf8_mode},
            "steps": out}


def main():
    # Only this script writes to stdout (file descriptor 1 becomes stderr until the report).
    sys.stdout.flush()
    report = os.fdopen(os.dup(1), "w", encoding="utf-8")
    os.dup2(2, 1)
    sys.stdout = sys.stderr
    script = json.loads(sys.stdin.buffer.read().decode("utf-8"))
    try:
        out = run(script)
    except RequestError as exc:
        out = {"request_error": str(exc)}
    report.write(json.dumps(out))
    report.close()


if __name__ == "__main__":
    main()
