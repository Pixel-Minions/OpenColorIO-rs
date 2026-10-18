# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for the app helpers and grading values (Phase 5, chunk O5.2).

- object_calls: a config, value objects built from object specs (grading values, ...), then
  calls in order on them, on the objects the calls make or return (ColorSpaceMenuParameters,
  ColorSpaceMenuHelper, LegacyViewingPipeline, MixingColorSpaceManager, MixingSlider, ...) and
  on the module functions of DisplayViewHelpers and ColorSpaceHelpers; each call's result or
  exception and log; and the processors the calls made, written out as processor_ops writes
  them.

It runs on config_calls' engine (config_api.py): the same environment, files, values, calls and
dumps.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import PyOpenColorIO as OCIO

from . import spec
from .commands import command
from .config_api import RequestError, make_config, run_request
from .processor_ops import _processor_dump

# The binding's helper modules, which the calls reach by these names ("on").
HELPER_MODULES = ("DisplayViewHelpers", "ColorSpaceHelpers")


def _value_objects(specs):
    """The value objects of the request's "objects": {name: object spec} (spec.value_object).
    A spec the oracle can't read is the request's error; what the binding's constructors and
    setters raise is the library's, and fails the config's making (no call runs then)."""
    if not isinstance(specs, dict):
        raise RequestError(f"objects must be an object of name: object spec, not {specs!r}")
    out = {}
    for name, object_spec in specs.items():
        if name in ("config", "OCIO", *HELPER_MODULES):
            raise RequestError(f"objects can't be stored as {name!r}")
        try:
            out[name] = spec.value_object(object_spec)
        except (OCIO.Exception, OCIO.ExceptionMissingFile):
            raise
        except (ValueError, KeyError, AttributeError, TypeError) as exc:
            raise RequestError(f"objects[{name!r}]: {exc}") from exc
    return out


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
      calls     the calls (see config_api.run_calls), on "config" unless they say otherwise:
                {"new": class, ...} makes ColorSpaceMenuParameters(config), ColorSpaceMenuHelper
                (parameters), LegacyViewingPipeline(), MixingColorSpaceManager(config) and the
                grading values; "on": "DisplayViewHelpers" or "ColorSpaceHelpers" calls those
                modules' functions (GetProcessor, GetIdentityProcessor, AddDisplayView,
                RemoveDisplayView; AddColorSpace); {"ref": name} passes a stored object
      processors
                names of objects the calls stored ("as"), each a Processor, written out after
                the calls as processor_ops writes a processor
    result:
      dir, config, config_log, calls
                as config_calls gives them; "config" also reports an exception the objects'
                constructors or setters raised (no call runs then)
      processors  per name, in order: {"cache_id", "isNoOp", "hasChannelCrosstalk", "isDynamic",
                "processor_metadata", "group"}, its values as checks.dump writes them
    blobs: the arrays the processors' dumps hold (checks.dump)

    A request it can't run exactly is refused, as config_calls refuses it; so are a name in
    processors that no call stored, or that holds something other than a Processor.
    """
    processors = args.get("processors", [])
    if not isinstance(processors, list) or not all(isinstance(n, str) for n in processors):
        raise RequestError(f"processors must be a list of names, not {processors!r}")
    held = {}

    def make_subject(source, objects):
        held["objects"] = objects
        for name in HELPER_MODULES:
            objects[name] = getattr(OCIO, name)
        config = make_config("raw" if source is None else source)
        objects.update(_value_objects(args.get("objects", {})))
        return config

    result = run_request(args, "config", make_subject, extra_keys=("objects", "processors"))
    objects, out = held.get("objects", {}), []
    result["processors"] = []
    if "config" not in objects:
        return result, out
    for name in processors:
        proc = objects.get(name)
        if not isinstance(proc, OCIO.Processor):
            raise RequestError(f"processors: {name!r} isn't a Processor a call stored")
        result["processors"].append(_processor_dump(proc, out))
    return result, out
