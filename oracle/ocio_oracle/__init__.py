# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""The oracle: reference outputs from the official OpenColorIO wheel.

Expected values for OpenColorIO-rs come only from here or from upstream's tests.
Commands call the real library and report what it returns. They never compute,
adjust or hard-code expected values themselves.

Two ways in:
- framed stdin/stdout, used by ``ocio_testkit::Oracle`` (see ``__main__.py``);
- ``python -m ocio_oracle regen <group> <out_dir>``, used by ``cargo xtask oracle regen``.
"""


def _platform_without_ver():
    """Fills ``platform``'s cached ``uname()`` on Windows without starting ``cmd /c ver``.

    Importing PyOpenColorIO calls ``platform.system()`` (its ``__init__.py``), which computes
    ``platform.uname()`` once and caches it. On Windows, when the WMI query fails, as under
    Intel SDE, Python falls back to starting ``cmd /c ver`` (``platform._syscmd_ver``), and
    Pin's injection into that child now and then crashes the oracle (0xC0000005) before it
    reads its request. Here ``_syscmd_ver`` answers the version ``ver`` prints,
    ``major.minor.build`` of ``sys.getwindowsversion()``, without starting it, and ``uname()``
    runs once so its answer is cached before any import needs it. Where WMI answers, as
    outside SDE, ``_syscmd_ver`` isn't called and nothing changes.
    """
    import platform
    import sys

    if sys.platform != "win32":
        return

    def _syscmd_ver(system="", release="", version="", *args, **kwargs):
        winver = sys.getwindowsversion()
        return system, release, f"{winver.major}.{winver.minor}.{winver.build}"

    platform._syscmd_ver = _syscmd_ver
    platform.uname()


_platform_without_ver()
