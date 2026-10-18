# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""The environment of the oracle's child processes: an allowlist, never the caller's whole
environment.

A child process that OCIO runs in would otherwise see every variable of whoever started the
tests, and OCIO prints them: a context that loads the whole environment lists each one in its
repr and its cache ID. ocio_testkit starts the oracle itself with the same allowlist
(ALLOWED_VARIABLES in crates/ocio-testkit/src/oracle.rs); this module gives the processes the
oracle starts the same, plus the variables a command deliberately sets.

Not a command module: it defines no command.
"""

import os

# What Python and the wheel need: Windows' system directory (Python's os.urandom, sockets and
# the C runtime look it up), and the temporary directory (tempfile; with_files and the config
# commands write their files under it).
ALLOWED = ("SYSTEMROOT", "TEMP", "TMP", "TMPDIR")


def allowed_environment(deliberate=None):
    """The allowlisted variables of this process (os.environ, whose names Windows upper-cases),
    then `deliberate` ({name: value}), the variables the command sets on purpose."""
    env = {name: os.environ[name] for name in ALLOWED if name in os.environ}
    env.update(deliberate or {})
    return env
