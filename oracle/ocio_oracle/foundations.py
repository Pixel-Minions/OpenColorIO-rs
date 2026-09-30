# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle commands for FormatMetadata and logging (card p1-foundations: 1.2a and 1.2e).

- format_metadata_ops: calls replayed on a transform's FormatMetadata, and the result: its
  repr() (upstream's operator<<), the errors, and every node's getters.
- format_metadata_combine: the metadata of two MatrixTransforms after the optimizer composes
  them, which combines the first one's metadata with the second one's
  (MatrixOpData::compose, MatrixOpData.cpp:684-686 @ v2.5.2).
- log_message: LogMessage at each level, for each level setting, with the lines the logging
  function receives.
- logging_level_strings: LoggingLevelFromString and LoggingLevelToString.
- logging_environment: OCIO_LOGGING_LEVEL, which OCIO reads once per process and the oracle's
  own environment never has: each case runs in a new Python process with the variable as the
  case sets it, and reports the levels, what the logging function receives and the raw stderr.

Like every oracle command, these report what the library does and never compute expected
values. Requests are checked: an unknown key, operation or level name is an error, so a typo
can't pass unnoticed.

A Python logging function still installed when the interpreter exits crashes the wheel, on
both platforms. Every command here that installs one resets it before it returns, and
log_message restores the level it found, so later commands of a batch see the default state.
"""

import json
import os
import subprocess
import sys

import PyOpenColorIO as OCIO

from .commands import command, exception_result


def _check_keys(what, spec, required, optional=()):
    """Refuses a spec that isn't an object, lacks a required key or has an unknown one."""
    if not isinstance(spec, dict):
        raise ValueError(f"{what} must be an object, not {spec!r}")
    missing = sorted(set(required) - set(spec))
    unknown = sorted(set(spec) - set(required) - set(optional))
    if missing or unknown:
        raise ValueError(f"{what}: missing keys {missing}, unknown keys {unknown}; it takes "
                         f"{sorted(required)} and optionally {sorted(optional)}")


def _string(what, value):
    if not isinstance(value, str):
        raise ValueError(f"{what} must be a string, not {value!r}")
    return value


def _level(name):
    """A LoggingLevel from its name, such as "LOGGING_LEVEL_INFO"."""
    if not isinstance(name, str) or not name.startswith("LOGGING_LEVEL_") \
            or not hasattr(OCIO, name):
        raise ValueError(f"unknown logging level {name!r}")
    return getattr(OCIO, name)


# format_metadata_ops: operation -> (its keys besides "op" and "path", the call).
METADATA_OPS = {
    # The binding's __setitem__ is FormatMetadata::addAttribute (PyFormatMetadata.cpp:79).
    "add_attribute": (("name", "value"), lambda n, a: n.__setitem__(a["name"], a["value"])),
    "set_name": (("name",), lambda n, a: n.setName(a["name"])),
    "set_id": (("id",), lambda n, a: n.setID(a["id"])),
    "add_child_element": (("name", "value"), lambda n, a: n.addChildElement(a["name"], a["value"])),
    "set_element_name": (("name",), lambda n, a: n.setElementName(a["name"])),
    "set_element_value": (("value",), lambda n, a: n.setElementValue(a["value"])),
    "clear": ((), lambda n, a: n.clear()),
    # getChildElements()[i] calls getChildElement(i), which raises for a bad index.
    "get_child_element": (("index",), lambda n, a: n.getChildElements()[a["index"]]),
}


def _metadata_node(root, path):
    node = root
    for index in path:
        node = node.getChildElements()[index]
    return node


def _metadata_tree(node):
    return {
        "element_name": node.getElementName(),
        "element_value": node.getElementValue(),
        "attributes": [[name, value] for name, value in node.getAttributes()],
        "name": node.getName(),
        "id": node.getID(),
        "children": [_metadata_tree(child) for child in node.getChildElements()],
    }


@command
def format_metadata_ops(args, blobs):
    """Calls on the FormatMetadata of a new MatrixTransform (an element named "ROOT").

    args: {"scenarios": [[op, ...], ...]}; each scenario starts from a new transform.
      op: {"op": name, "path": [child index, ...], ...}. "path" leads from the transform's
      metadata to the node the call acts on, through getChildElements()[i]. The names and their
      keys, with the method each calls:
        add_attribute      name, value   node[name] = value (FormatMetadata::addAttribute)
        set_name           name          setName
        set_id             id            setID
        add_child_element  name, value   addChildElement
        set_element_name   name          setElementName
        set_element_value  value         setElementValue
        clear                            clear
        get_child_element  index         getChildElements()[index] (getChildElement)
      Strings reach OCIO as C strings: the binding passes the bytes before the first NUL.
    result: per scenario {"errors": [[op index, OCIO exception message]], "repr": repr() of the
      transform's metadata, "tree": node}, where node is {"element_name", "element_value",
      "attributes": [[name, value]], "name" (getName), "id" (getID), "children": [node]}.
    """
    _check_keys("format_metadata_ops", args, ["scenarios"])
    results = []
    for s, scenario in enumerate(args["scenarios"]):
        if not isinstance(scenario, list):
            raise ValueError(f"scenarios[{s}] must be a list of operations")
        root = OCIO.MatrixTransform().getFormatMetadata()
        errors = []
        for i, op in enumerate(scenario):
            name = op.get("op") if isinstance(op, dict) else None
            if name not in METADATA_OPS:
                raise ValueError(f"scenarios[{s}][{i}]: unknown operation in {op!r}")
            keys, call = METADATA_OPS[name]
            _check_keys(f"scenarios[{s}][{i}] ({name})", op, ("op", "path") + keys)
            for key in keys:
                if key == "index":
                    if not isinstance(op[key], int):
                        raise ValueError(f"scenarios[{s}][{i}]: index must be an integer")
                else:
                    _string(f"scenarios[{s}][{i}].{key}", op[key])
            try:
                call(_metadata_node(root, op["path"]), op)
            except OCIO.Exception as exc:
                errors.append([i, str(exc)])
        results.append({"errors": errors, "repr": repr(root), "tree": _metadata_tree(root)})
    return results, []


# The matrix of both transforms: a scale by 2 on RGB, so the optimizer composes the two into
# one and removes neither.
COMBINE_MATRIX = [2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1]


def _metadata_transform(what, spec):
    _check_keys(what, spec, ["attributes", "children"])
    transform = OCIO.MatrixTransform(matrix=COMBINE_MATRIX)
    metadata = transform.getFormatMetadata()
    for name, value in spec["attributes"]:
        metadata[_string(what, name)] = _string(what, value)
    for name, value in spec["children"]:
        metadata.addChildElement(_string(what, name), _string(what, value))
    return transform


@command
def format_metadata_combine(args, blobs):
    """The metadata of two MatrixTransforms, optimized into one.

    For each case: a GroupTransform of two MatrixTransforms that scale RGB by 2, with the given
    metadata, in a raw config with the processor cache off; the processor, optimized with
    OPTIMIZATION_DEFAULT; and its createGroupTransform(). The optimizer composes the two
    matrices, and the composed op's metadata is the first one's combined with the second
    one's (MatrixOpData::compose, MatrixOpData.cpp:684-686 @ v2.5.2).

    args: {"cases": [{"first": metadata, "second": metadata}]}
      metadata: {"attributes": [[name, value]], "children": [[name, value]]}, set with
      metadata[name] = value (addAttribute) and addChildElement, in that order.
    result: per case {"transforms": [{"class": the Python class name, "repr": repr() of its
      metadata}]}, the transforms of the optimized group in order, or {"exception": {...}}
    """
    _check_keys("format_metadata_combine", args, ["cases"])
    results = []
    for c, case in enumerate(args["cases"]):
        _check_keys(f"cases[{c}]", case, ["first", "second"])
        try:
            group = OCIO.GroupTransform()
            group.appendTransform(_metadata_transform(f"cases[{c}].first", case["first"]))
            group.appendTransform(_metadata_transform(f"cases[{c}].second", case["second"]))
            config = OCIO.Config.CreateRaw()
            config.setProcessorCacheFlags(OCIO.PROCESSOR_CACHE_OFF)
            optimized = config.getProcessor(group).getOptimizedProcessor(
                OCIO.OPTIMIZATION_DEFAULT).createGroupTransform()
            results.append({"transforms": [
                {"class": type(t).__name__, "repr": repr(t.getFormatMetadata())} for t in optimized
            ]})
        except OCIO.Exception as exc:
            results.append({"exception": exception_result(exc)})
    return results, []


@command
def log_message(args, blobs):
    """OCIO.LogMessage with every combination of a level setting, a message level and a message.

    For each setting in order: SetLoggingLevel(setting); then for each message level, and for
    each message: LogMessage(level, message), with a logging function that collects the lines
    it receives. The command then restores the level it found and the default logging function.

    args: {"settings": [level name], "levels": [level name], "messages": [str]}
      level name: "LOGGING_LEVEL_NONE", "_WARNING", "_INFO", "_DEBUG" or "_UNKNOWN".
      A message reaches OCIO as a C string: the binding passes the bytes before the first NUL.
    result: {"settings": [{"level": LoggingLevelToString(GetLoggingLevel()) after the
      setting, "calls": [{"lines": [str], "exception": {...} or null}]}]}, the calls in the
      order level, then message.
    """
    _check_keys("log_message", args, ["settings", "levels", "messages"])
    settings = [_level(name) for name in args["settings"]]
    levels = [_level(name) for name in args["levels"]]
    messages = [_string("messages[]", m) for m in args["messages"]]
    received = []
    previous = OCIO.GetLoggingLevel()
    OCIO.SetLoggingFunction(received.append)
    try:
        out = []
        for setting in settings:
            OCIO.SetLoggingLevel(setting)
            calls = []
            for level in levels:
                for message in messages:
                    received.clear()
                    try:
                        OCIO.LogMessage(level, message)
                        exception = None
                    except OCIO.Exception as exc:
                        exception = exception_result(exc)
                    calls.append({"lines": list(received), "exception": exception})
            out.append({"level": OCIO.LoggingLevelToString(OCIO.GetLoggingLevel()),
                        "calls": calls})
    finally:
        OCIO.ResetToDefaultLoggingFunction()
        OCIO.SetLoggingLevel(previous)
    return {"settings": out}, []


@command
def logging_level_strings(args, blobs):
    """LoggingLevelFromString and LoggingLevelToString.

    args: {"from_string": [str], "to_string": [level name]}
      A string reaches OCIO as a C string: the binding passes the bytes before the first NUL.
    result: {"from_string": [the level's name, such as "LOGGING_LEVEL_INFO"],
             "to_string": [LoggingLevelToString(level)]}
    """
    _check_keys("logging_level_strings", args, ["from_string", "to_string"])
    from_string = [OCIO.LoggingLevelFromString(_string("from_string[]", s)).name
                   for s in args["from_string"]]
    to_string = [OCIO.LoggingLevelToString(_level(name)) for name in args["to_string"]]
    return {"from_string": from_string, "to_string": to_string}, []


# Runs in the new process of a logging_environment case. It reads the case from stdin, writes
# its report to stdout, and resets the logging function before it exits.
_ENVIRONMENT_CHILD = r"""
import json, sys
import PyOpenColorIO as OCIO
case = json.loads(sys.stdin.read())
received = []
if case["custom_function"]:
    OCIO.SetLoggingFunction(received.append)
try:
    first = OCIO.LoggingLevelToString(OCIO.GetLoggingLevel())
    if case["set_level"] is not None:
        OCIO.SetLoggingLevel(getattr(OCIO, case["set_level"]))
    after = OCIO.LoggingLevelToString(OCIO.GetLoggingLevel())
    for level, message in case["messages"]:
        OCIO.LogMessage(getattr(OCIO, level), message)
finally:
    OCIO.ResetToDefaultLoggingFunction()
for level, message in case["messages_after_reset"]:
    OCIO.LogMessage(getattr(OCIO, level), message)
try:
    OCIO.SetLoggingFunction(None)
    null_function = None
except OCIO.Exception as exc:
    null_function = str(exc)
finally:
    OCIO.ResetToDefaultLoggingFunction()
sys.stdout.write(json.dumps({"first": first, "after": after, "received": received,
                             "null_function": null_function}))
"""


@command
def logging_environment(args, blobs):
    """OCIO_LOGGING_LEVEL, in a new Python process per case.

    OCIO reads the variable once per process, the first time it needs the level, and the
    oracle's own environment has no OCIO_* variable. Each case starts `python -c` with this
    oracle's interpreter and environment, less every OCIO_* variable, plus OCIO_LOGGING_LEVEL
    when the case sets it. The new process:
      1. when "custom_function" is true, sets a logging function that collects the lines;
      2. reads GetLoggingLevel(), which reads the variable (a bad value writes a warning to
         stderr, and "debug" a version line);
      3. calls SetLoggingLevel(set_level) when set_level isn't null (ignored when the variable
         is set), and reads GetLoggingLevel() again;
      4. calls LogMessage(level, message) for each of "messages"; without a logging function of
         its own, OCIO writes the lines to stderr;
      5. resets the logging function (ResetToDefaultLoggingFunction), and calls LogMessage for
         each of "messages_after_reset", which go to stderr;
      6. calls SetLoggingFunction(None), which OCIO refuses, and resets the logging function
         again;
      7. writes its report to stdout.

    args: {"cases": [{"env": str or null (null: unset), "custom_function": bool,
                      "set_level": level name or null, "messages": [[level name, str]],
                      "messages_after_reset": [[level name, str]]}]}
    result: per case {"returncode": int, "report": {"first": LoggingLevelToString of step 2,
      "after": of step 3, "received": [str] (the custom function's lines), "null_function":
      the message of step 6's exception, or null}, or the raw stdout text if it isn't JSON,
      "stderr": index of the blob holding the raw stderr bytes}
    """
    _check_keys("logging_environment", args, ["cases"])
    results, out = [], []
    for c, case in enumerate(args["cases"]):
        _check_keys(f"cases[{c}]", case, ["env", "custom_function", "set_level", "messages",
                                          "messages_after_reset"])
        if case["env"] is not None:
            _string(f"cases[{c}].env", case["env"])
        if not isinstance(case["custom_function"], bool):
            raise ValueError(f"cases[{c}].custom_function must be true or false")
        if case["set_level"] is not None:
            _level(case["set_level"])
        for key in ("messages", "messages_after_reset"):
            for level, message in case[key]:
                _level(level)
                _string(f"cases[{c}].{key}[]", message)
        env = {k: v for k, v in os.environ.items() if k != "OCIO" and not k.startswith("OCIO_")}
        if case["env"] is not None:
            env["OCIO_LOGGING_LEVEL"] = case["env"]
        child = subprocess.run([sys.executable, "-c", _ENVIRONMENT_CHILD],
                               input=json.dumps(case).encode("utf-8"), env=env,
                               capture_output=True, timeout=120)
        try:
            report = json.loads(child.stdout.decode("utf-8"))
        except ValueError:
            report = child.stdout.decode("utf-8", "replace")
        results.append({"returncode": child.returncode, "report": report, "stderr": len(out)})
        out.append(child.stderr)
    return results, out
