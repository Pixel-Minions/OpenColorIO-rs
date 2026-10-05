# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for the built-in transform registry (card p3-oracle, chunk O3.6).

- builtin_transform_names: the registry's entries in order, each style and description; and
  for given styles, what BuiltinTransform.setStyle does with them (the style and description
  it then reports, its repr(), what validate() raises, or what setStyle raised).

Pixels of the built-in transforms need no command of their own: cpu_apply, image_apply and
gpu_shader take a BuiltinTransform spec ({"class": "BuiltinTransform", "args": {"style":
...}}).

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import PyOpenColorIO as OCIO

from .checks import check_keys
from .commands import command
from .config_api import LogCapture, RequestError, attempt, call_out, text, value_out


def _set_style(style):
    """A BuiltinTransform after setStyle(style): its getters, repr() and validate(), or what
    setStyle raised (as config_api reports a call)."""
    transform = OCIO.BuiltinTransform()
    done = call_out(transform.setStyle, style)
    if "result" not in done:
        return done
    validated = call_out(transform.validate)
    return {"getStyle": attempt(transform.getStyle),
            "getDescription": attempt(transform.getDescription),
            "repr": attempt(lambda: repr(transform)),
            "validate": None if "result" in validated else validated}


@command
def builtin_transform_names(args, blobs):
    """The built-in transform registry, and setStyle on given styles.

    args:
      styles    optional: [style, ...] (text or {"bytes": hex}), each set on a new
                BuiltinTransform with setStyle, which looks it up case-insensitively
    result:
      builtins  BuiltinTransformRegistry().getBuiltins(), in order: [[style, description], ...],
                each string {"bytes": hex}, read entry by entry (config_api.value_out)
      count     len(BuiltinTransformRegistry()), getNumBuiltins()
      styles    per style given, in order: {"getStyle", "getDescription", "repr" (each
                {"bytes": hex}), "validate": null or {"exception"}}, or {"exception"} or
                {"undecodable"} when setStyle failed
      log       what OCIO logged: [{"bytes": hex}]
    blobs: none

    An unknown key, or a style that isn't text or {"bytes": hex}, is refused.
    """
    check_keys("builtin_transform_names", args, {"styles"})
    styles = args.get("styles", [])
    if not isinstance(styles, list):
        raise RequestError(f"styles must be a list, not {styles!r}")
    styles = [text(f"styles[{i}]", s) for i, s in enumerate(styles)]
    with LogCapture() as log:
        registry = OCIO.BuiltinTransformRegistry()
        result = {"builtins": value_out(registry.getBuiltins()),
                  "count": len(registry),
                  "styles": [_set_style(style) for style in styles]}
        result["log"] = log.take()
    return result, []
