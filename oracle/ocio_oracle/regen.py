# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Fixture groups for ``cargo xtask oracle regen <group>``.

Each group writes files under ``<out_dir>/<group>/`` and returns their paths relative to
``<out_dir>``. Only platform-independent text goes here (PLAN.md §8); pixel checks run live.
"""

import os

import PyOpenColorIO as OCIO

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


def run(name, out_dir):
    if name not in GROUPS:
        raise SystemExit(f"unknown fixture group {name!r}; known: {', '.join(sorted(GROUPS))}")
    files = GROUPS[name](out_dir)
    return {"group": name, "generator": f"ocio_oracle regen {name}", "ocio_version": OCIO.__version__, "files": files}
