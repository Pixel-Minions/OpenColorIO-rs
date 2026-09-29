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
