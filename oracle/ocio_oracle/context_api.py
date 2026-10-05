# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for a Context's API (card p3-oracle, chunk O3.2).

- context_calls: a Context, made on its own or a config's current one, in an environment and a
  directory of files the request gives; then calls in order on it and on the objects the calls
  return or create, each call's result or exception and log.

It runs on config_calls' engine (config_api.py): the same environment, files, values, calls and
dumps. The calls of the card's checks are ordinary calls there:
- the setters: {"call": "setSearchPath", "args": [path]}, "addSearchPath", "setWorkingDir",
  "setStringVar", "clearStringVars", "setEnvironmentMode", "loadEnvironment", ...;
- resolveStringVar and resolveFileLocation with the used context: {"new": "Context", "as":
  "used"}, then {"call": "resolveFileLocation", "args": [name, {"ref": "used"}]}, then
  {"dump": "used"} (or {"call": "getStringVars", "on": "used"});
- the cache ID and operator<<: {"call": "getCacheID"} and {"call": "__repr__"}, or {"dump":
  "context"}, which holds both.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import PyOpenColorIO as OCIO

from .commands import command
from .config_api import RequestError, make_config, run_request, value_in


def make_context(source, objects):
    """The context of a request, stored as "context":
      "new"                  Context()
      {"new": {kwargs}}      Context(**kwargs): workingDir, searchPaths, stringVars (a value
                             {"map": [[name, value], ...]}), environmentMode ({"enum": name})
      {"config": source}     the current context of a config made from a config_calls source
                             (config_api.make_config), which is stored as "config" too
    Values as config_api.value_in reads them."""
    if source == "new":
        return OCIO.Context()
    if isinstance(source, dict) and len(source) == 1:
        (kind, value), = source.items()
        if kind == "new":
            if not isinstance(value, dict):
                raise RequestError(f"context {{'new': ...}} takes an object, not {value!r}")
            kwargs = {k: value_in(v, objects) for k, v in value.items()}
            return OCIO.Context(**kwargs)
        if kind == "config":
            objects["config"] = make_config(value)
            return objects["config"].getCurrentContext()
    raise RequestError(f"unknown context source {source!r}")


@command
def context_calls(args, blobs):
    """A Context, then calls on it and on the objects they give.

    args:
      context   the source (see make_context): "new", {"new": {kwargs}} or {"config": source}
      env       {name: value}: the environment of the whole request, which holds exactly these
                variables (as in config_calls)
      files     {relative path: content}: written to a new temporary directory, the working
                directory of the whole request (as in config_calls)
      calls     the calls (see config_api.run_calls), on "context" unless they say otherwise
    result:
      dir           the temporary directory's absolute path
      context       null, or {"exception"} or {"undecodable"} when making the context (or its
                    config) failed; no call runs then
      context_log   what OCIO logged while making it: [{"bytes": hex}]
      calls         per call {"result"}, {"exception"} or {"undecodable"}, and "log" (see
                    run_calls)
    blobs: none

    A request it can't run exactly is refused, as config_calls refuses it.
    """
    return run_request(args, "context", make_context), []
