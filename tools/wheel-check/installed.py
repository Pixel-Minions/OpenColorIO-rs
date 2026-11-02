# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Which PyOpenColorIO this interpreter imports, as JSON on stdout, for `cargo xtask wheel-check`
(xtask/src/wheel_check.rs). Run with `-I`, in an environment holding only the wheel, so the import
also shows the module needs nothing else, NumPy included. It judges nothing: xtask compares.
"""

import importlib.metadata
import json
import os
import sys
import sysconfig

import PyOpenColorIO
import PyOpenColorIO.command_line


def main():
    site = sysconfig.get_path("platlib")
    json.dump(
        {
            "site": site,
            "package": PyOpenColorIO.__file__,
            "extension": PyOpenColorIO.PyOpenColorIO.__file__,
            "command_line": PyOpenColorIO.command_line.__file__,
            "bin_dir": PyOpenColorIO.command_line.BIN_DIR,
            "sys_path": sys.path,
            "pth": sorted(name for name in os.listdir(site) if name.endswith(".pth")),
            "distributions": sorted(d.name for d in importlib.metadata.distributions()),
            "version": importlib.metadata.version("opencolorio-rs"),
            "python": list(sys.version_info[:2]),
            "ocio_version": PyOpenColorIO.__version__,
        },
        sys.stdout,
    )


if __name__ == "__main__":
    main()
