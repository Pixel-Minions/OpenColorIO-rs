# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for the processors' caches and the environment that controls them
(card p1-processor, WP 1.8g, 1.8h1 and 1.8h3).

- processor_cache: steps on a raw config and the processors it makes, in a new Python process
  whose environment the case sets, and which processors came back as the same object.

OCIO reads OCIO_DISABLE_ALL_CACHES and OCIO_DISABLE_PROCESSOR_CACHES when it creates a cache,
and OCIO_DISABLE_CACHE_FALLBACK and OCIO_OPTIMIZATION_FLAGS at each call; the oracle's own
environment has none of them, and changing it would leak into the next commands. So each case
runs in its own process, like logging_environment's.

Which objects are the same is what the caches decide: pybind11 returns the Python object it
already has for a C++ object, and every object the steps make stays alive until the case ends,
so two steps give the same Python object exactly when OCIO returned the same processor.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import json
import os
import subprocess
import sys

from .commands import command

# The steps a case can take, with the number of arguments each takes.
STEPS = {
    "config": 0,
    "env": 2,
    "set_cache_flags": 1,
    "clear_cache": 0,
    "processor": 3,
    "optimized": 5,
    "cpu": 5,
    "gpu": 3,
}

# Runs in the new process of a case. It reads the case from stdin and writes its report to
# stdout.
_CHILD = r"""
import json, os, sys
case = json.loads(sys.stdin.read())
sys.path.insert(0, case["oracle_path"])
import PyOpenColorIO as OCIO
from ocio_oracle import spec

blobs = [bytes.fromhex(blob) for blob in case["blobs"]]
config = None
objects = {}
results = []
for step in case["steps"]:
    kind, args = step[0], step[1:]
    try:
        if kind == "config":
            config = OCIO.Config.CreateRaw()
        elif kind == "env":
            name, value = args
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value
        elif kind == "set_cache_flags":
            config.setProcessorCacheFlags(getattr(OCIO, args[0]))
        elif kind == "clear_cache":
            config.clearProcessorCache()
        elif kind == "processor":
            name, transform, direction = args
            objects[name] = config.getProcessor(spec.transform(transform, blobs),
                                                getattr(OCIO, direction))
        elif kind == "gpu":
            name, of, flags = args
            objects[name] = (objects[of].getDefaultGPUProcessor() if flags is None
                             else objects[of].getOptimizedGPUProcessor(spec.flags(flags)))
        else:
            name, of, in_bd, out_bd, flags = args
            getter = (objects[of].getOptimizedProcessor if kind == "optimized"
                      else objects[of].getOptimizedCPUProcessor)
            objects[name] = getter(getattr(OCIO, in_bd), getattr(OCIO, out_bd),
                                   spec.flags(flags))
        results.append(None)
    except OCIO.Exception as exc:
        results.append({"type": type(exc).__name__, "message": str(exc)})
names = list(objects)
same = {name: next(first for first in names if objects[first] is objects[name])
        for name in names}
ids = {name: objects[name].getCacheID() for name in names}
sys.stdout.write(json.dumps({"steps": results, "same": same, "cache_ids": ids}))
"""


def _check_case(c, case):
    if not isinstance(case, dict) or set(case) != {"env", "steps"}:
        raise ValueError(f"cases[{c}] must have exactly the keys env and steps")
    env = case["env"]
    if not isinstance(env, dict) or not all(
            isinstance(k, str) and k.startswith("OCIO_") and isinstance(v, str)
            for k, v in env.items()):
        raise ValueError(f"cases[{c}].env must map OCIO_* names to strings")
    names = set()
    for s, step in enumerate(case["steps"]):
        what = f"cases[{c}].steps[{s}]"
        if not isinstance(step, list) or not step or step[0] not in STEPS:
            raise ValueError(f"{what} must be a list naming one of {sorted(STEPS)}")
        if len(step) - 1 != STEPS[step[0]]:
            raise ValueError(f"{what}: {step[0]} takes {STEPS[step[0]]} arguments")
        if step[0] == "env":
            if not isinstance(step[1], str) or not step[1].startswith("OCIO_"):
                raise ValueError(f"{what}: only OCIO_* variables")
            if step[2] is not None and not isinstance(step[2], str):
                raise ValueError(f"{what}: a value is a string or null")
        if step[0] in ("processor", "optimized", "cpu", "gpu"):
            if not isinstance(step[1], str) or step[1] in names:
                raise ValueError(f"{what}: names must be new strings")
            names.add(step[1])
        if step[0] in ("optimized", "cpu", "gpu") and step[2] not in names:
            raise ValueError(f"{what}: unknown processor {step[2]!r}")


@command
def processor_cache(args, blobs):
    """Steps on a raw config and its processors, in a new Python process per case.

    Each case starts `python -I -c` with this oracle's interpreter and environment, less every
    OCIO_* variable, plus the case's "env". Isolated mode (-I) ignores every PYTHON* variable
    and the user's site-packages. The new process runs the steps in order; a step OCIO raises
    in is reported, and the next steps still run (one that needs the object it didn't make
    stops the process, which the result shows by its returncode).

    args: {"cases": [{"env": {OCIO_* name: value}, "steps": [step, ...]}]}, the steps:
      ["config"]                          config = Config.CreateRaw(), which reads the cache
                                          variables then
      ["env", name, value or null]        sets (os.environ) or removes an OCIO_* variable
      ["set_cache_flags", name]           config.setProcessorCacheFlags(PROCESSOR_CACHE_*)
      ["clear_cache"]                     config.clearProcessorCache()
      ["processor", name, transform spec, direction name]
                                          config.getProcessor(transform, direction)
      ["optimized", name, of, in bit depth, out bit depth, flags]
                                          objects[of].getOptimizedProcessor(in, out, flags)
      ["cpu", name, of, in bit depth, out bit depth, flags]
                                          objects[of].getOptimizedCPUProcessor(in, out, flags)
      ["gpu", name, of, flags or null]    objects[of].getOptimizedGPUProcessor(flags), or
                                          getDefaultGPUProcessor() for null
      (bit depths are BIT_DEPTH_* names, flags as spec.flags takes them)
    request blobs: the transform specs' blobs, which every case and step share (see spec.py);
      the new process gets them in its input
    result: per case {"returncode": int, "report": {"steps": per step null or the exception
      {"type", "message"}, "same": for each named object, the first name (in the order the
      steps made them) of the same object, "cache_ids": each named object's getCacheID()}, or
      the raw stdout text if it isn't JSON, "stderr": index of the blob of the raw stderr}

    A step of an unknown kind, with the wrong number of arguments, naming an object twice or
    an object no step made, or setting a variable not named OCIO_*, makes the command raise.
    """
    if not isinstance(args, dict) or set(args) != {"cases"} or not isinstance(args["cases"],
                                                                              list):
        raise ValueError("processor_cache takes exactly {'cases': [...]}")
    oracle_path = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    results, out = [], []
    for c, case in enumerate(args["cases"]):
        _check_case(c, case)
        env = {k: v for k, v in os.environ.items() if k != "OCIO" and not k.startswith("OCIO_")}
        env.update(case["env"])
        child_input = {"oracle_path": oracle_path, "steps": case["steps"],
                       "blobs": [blob.hex() for blob in blobs]}
        child = subprocess.run([sys.executable, "-I", "-c", _CHILD],
                               input=json.dumps(child_input).encode("utf-8"), env=env,
                               capture_output=True, timeout=120)
        try:
            report = json.loads(child.stdout.decode("utf-8"))
        except ValueError:
            report = child.stdout.decode("utf-8", "replace")
        results.append({"returncode": child.returncode, "report": report, "stderr": len(out)})
        out.append(child.stderr)
    return results, out
