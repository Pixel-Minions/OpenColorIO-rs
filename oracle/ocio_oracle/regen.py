# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Fixture groups for ``cargo xtask oracle regen <group>``.

Each group writes files under ``<out_dir>/<group>/`` and returns their paths relative to
``<out_dir>``. Only platform-independent text goes here (PLAN.md §8); pixel checks run live.
"""

import json
import os

import PyOpenColorIO as OCIO

from . import text
from .commands import captured_log

GROUPS = {}


def group(fn):
    GROUPS[fn.__name__] = fn
    return fn


def _write(out_dir, rel, data):
    path = os.path.join(out_dir, *rel.split("/"))
    os.makedirs(os.path.dirname(path), exist_ok=True)
    if isinstance(data, str):
        data = data.encode("utf-8")
    with open(path, "wb") as f:
        f.write(data)
    return rel


@group
def builtin_configs(out_dir):
    """For each built-in config: serialize() output, cache ID and log messages.

    The embedded source text (BuiltinConfigRegistry()[name]) is not a fixture: the Windows
    wheel embeds it with CRLF line endings and the Linux wheel with LF, so it is checked
    live on each platform instead.
    """
    written = []
    registry = OCIO.BuiltinConfigRegistry()
    for name, _ui, _rec, _dflt in registry.getBuiltinConfigs():
        base = f"builtin_configs/{name}"
        with captured_log() as log:
            config = OCIO.Config.CreateFromBuiltinConfig(name)
            written.append(_write(out_dir, f"{base}/serialize.ocio", config.serialize()))
            written.append(_write(out_dir, f"{base}/cache_id.txt", config.getCacheID()))
        written.append(_write(out_dir, f"{base}/log.txt", "".join(m if m.endswith("\n") else m + "\n" for m in log)))
    return written


@group
def yaml_emitter(out_dir):
    """S1 edge cases for the YAML emitter: for each case of text.emitter_cases(), its spec
    (spec.json, the inputs, see text.serialize_built_config) and serialize() of the config built
    from it (serialize.ocio). For each case of text.substitution_cases(), spec.json holds the
    spec with placeholders and the substitutions, placeholder.ocio is serialize() of that
    spec, and serialize.ocio is serialize() of the spec with the strings put in."""
    written = []

    def spec_json(value):
        return json.dumps(value, ensure_ascii=True, indent=1, sort_keys=True) + "\n"

    for name, spec in text.emitter_cases().items():
        base = f"yaml_emitter/{name}"
        written.append(_write(out_dir, f"{base}/spec.json", spec_json(spec)))
        written.append(_write(out_dir, f"{base}/serialize.ocio", text.build_config(spec).serialize()))
    for name, case in text.substitution_cases().items():
        base = f"yaml_emitter/{name}"
        real = text.substitute(case["spec"], dict(case["substitutions"]))
        written.append(_write(out_dir, f"{base}/spec.json", spec_json(case)))
        written.append(_write(out_dir, f"{base}/placeholder.ocio",
                              text.build_config(case["spec"]).serialize()))
        written.append(_write(out_dir, f"{base}/serialize.ocio", text.build_config(real).serialize()))
    return written


def run(name, out_dir):
    if name not in GROUPS:
        raise SystemExit(f"unknown fixture group {name!r}; known: {', '.join(sorted(GROUPS))}")
    files = GROUPS[name](out_dir)
    return {"group": name, "generator": f"ocio_oracle regen {name}", "ocio_version": OCIO.__version__, "files": files}
