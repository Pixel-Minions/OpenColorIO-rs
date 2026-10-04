# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for a config's version as Config.setMajorVersion sets it (card
p1-transforms).

- config_major_version: for each major version given, a new config (the raw config by
  default) whose setMajorVersion(version) is called: its getMajorVersion() and
  getMinorVersion() before and after the call, or what the call raised.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import PyOpenColorIO as OCIO

from . import spec
from .checks import check_keys, check_uint
from .commands import captured_log, command, exception_result

# What OCIO raises (in PyOpenColorIO, ExceptionMissingFile doesn't derive from OCIO.Exception).
RAISED = (OCIO.Exception, OCIO.ExceptionMissingFile)

# setMajorVersion takes a C unsigned int.
UINT_MAX = 0xFFFFFFFF


def _version(config):
    return [config.getMajorVersion(), config.getMinorVersion()]


@command
def config_major_version(args, blobs):
    """A config's major and minor versions around setMajorVersion(version).

    args:
      config    the config (see spec.config; default the raw config), built anew for each
                version
      versions  [version, ...]: the arguments of setMajorVersion, integers from 0 to UINT_MAX
    result:
      versions  per version, in order: {"before": [major, minor], "after": [major, minor]},
                with "exception": {"type", "message"} when setMajorVersion raised ("after" is
                then the config's versions after the failed call)
      log       OCIO's log messages
    blobs: none

    An unknown key, versions that aren't a list, or a version that isn't an integer from 0 to
    UINT_MAX (true and false aren't integers), is refused.
    """
    check_keys("config_major_version", args, {"config", "versions"})
    versions = args.get("versions", [])
    if not isinstance(versions, list):
        raise ValueError(f"versions must be a list, not {versions!r}")
    for version in versions:
        check_uint("a version", version, UINT_MAX)
    out = []
    with captured_log() as log:
        for version in versions:
            config = spec.config(args.get("config"))
            entry = {"before": _version(config)}
            try:
                config.setMajorVersion(version)
            except RAISED as exc:
                entry["exception"] = exception_result(exc)
            entry["after"] = _version(config)
            out.append(entry)
    return {"versions": out, "log": log}, []
