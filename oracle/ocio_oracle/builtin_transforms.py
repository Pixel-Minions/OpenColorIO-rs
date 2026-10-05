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
from .commands import captured_log, command
from .config_api import REPORTED, RequestError, raised, text


def _set_style(style):
    """A BuiltinTransform after setStyle(style): its getters, repr() and validate(), or what
    setStyle raised."""
    transform = OCIO.BuiltinTransform()
    try:
        transform.setStyle(style)
    except REPORTED as exc:
        return {"exception": raised(exc)}
    out = {"getStyle": transform.getStyle(), "getDescription": transform.getDescription(),
           "repr": repr(transform)}
    try:
        transform.validate()
        out["validate"] = None
    except REPORTED as exc:
        out["validate"] = {"exception": raised(exc)}
    return out


@command
def builtin_transform_names(args, blobs):
    """The built-in transform registry, and setStyle on given styles.

    args:
      styles    optional: [style, ...] (text or {"bytes": hex}), each set on a new
                BuiltinTransform with setStyle, which looks it up case-insensitively
    result:
      builtins  BuiltinTransformRegistry().getBuiltins(), in order: [[style, description], ...]
      count     len(BuiltinTransformRegistry())
      styles    per style given, in order: {"getStyle", "getDescription", "repr", "validate":
                null or {"exception"}}, or {"exception"} when setStyle raised
      log       what OCIO logged
    blobs: none

    An unknown key, or a style that isn't text or {"bytes": hex}, is refused.
    """
    check_keys("builtin_transform_names", args, {"styles"})
    styles = args.get("styles", [])
    if not isinstance(styles, list):
        raise RequestError(f"styles must be a list, not {styles!r}")
    styles = [text(f"styles[{i}]", s) for i, s in enumerate(styles)]
    with captured_log() as log:
        registry = OCIO.BuiltinTransformRegistry()
        result = {"builtins": [[style, description]
                               for style, description in registry.getBuiltins()],
                  "count": len(registry),
                  "styles": [_set_style(style) for style in styles]}
    result["log"] = list(log)
    return result, []
