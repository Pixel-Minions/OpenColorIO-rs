# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""What a wheel holds, as JSON on stdout, for `cargo xtask wheel-check` (xtask/src/wheel.rs).

    python -I read_wheel.py <wheel>

Each member's size, Unix mode and digests, and the text of the `.dist-info` files that aren't
license files. It judges nothing: xtask compares.
"""

import base64
import hashlib
import json
import sys
import zipfile


def main():
    members = {}
    texts = {}
    with zipfile.ZipFile(sys.argv[1]) as wheel:
        for info in wheel.infolist():
            data = wheel.read(info)
            digest = hashlib.sha256(data)
            members[info.filename] = {
                "size": len(data),
                "mode": info.external_attr >> 16,
                "sha256": digest.hexdigest(),
                # RECORD's form (PEP 376, the wheel specification).
                "record_hash": base64.urlsafe_b64encode(digest.digest()).rstrip(b"=").decode(),
            }
            directory, _, name = info.filename.rpartition("/")
            if directory.endswith(".dist-info"):
                texts[name] = data.decode("utf-8")
    json.dump({"members": members, "texts": texts}, sys.stdout)


if __name__ == "__main__":
    main()
