# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Entry point.

Framed mode (no arguments): one request on stdin, one response on stdout.

    u32 little-endian header length | header (UTF-8 JSON) | blob 0 | blob 1 | ...

Request header: {"cmd": str, "args": object, "blobs": [len, ...]}
Response header: {"ok": true, "result": any, "blobs": [len, ...]} or {"ok": false, "error": str}

CLI mode: ``python -m ocio_oracle regen <group> <out_dir>`` writes fixture files and prints
a JSON list of the files it wrote.
"""

import json
import struct
import sys
import traceback

from . import commands, regen


def _read_exact(stream, n):
    data = stream.read(n)
    if data is None or len(data) != n:
        raise EOFError(f"expected {n} bytes, got {0 if data is None else len(data)}")
    return data


def _write_frame(stream, header, blobs):
    header = dict(header)
    header["blobs"] = [len(b) for b in blobs]
    encoded = json.dumps(header, separators=(",", ":"), allow_nan=False).encode("utf-8")
    stream.write(struct.pack("<I", len(encoded)))
    stream.write(encoded)
    for blob in blobs:
        stream.write(blob)
    stream.flush()


def serve_one():
    stdin = sys.stdin.buffer
    stdout = sys.stdout.buffer
    # Anything a command prints must not corrupt the framed response.
    sys.stdout = sys.stderr
    try:
        (length,) = struct.unpack("<I", _read_exact(stdin, 4))
        header = json.loads(_read_exact(stdin, length).decode("utf-8"))
        blobs = [_read_exact(stdin, n) for n in header.get("blobs", [])]
        handler = commands.COMMANDS.get(header["cmd"])
        if handler is None:
            raise KeyError(f"unknown oracle command {header['cmd']!r}")
        result, out_blobs = handler(header.get("args") or {}, blobs)
        _write_frame(stdout, {"ok": True, "result": result}, out_blobs)
    except Exception:  # noqa: BLE001 - every failure is reported to the caller
        _write_frame(stdout, {"ok": False, "error": traceback.format_exc()}, [])


def main(argv):
    if not argv:
        serve_one()
        return 0
    if argv[0] == "regen" and len(argv) == 3:
        written = regen.run(argv[1], argv[2])
        print(json.dumps(written, indent=1))
        return 0
    print(__doc__, file=sys.stderr)
    return 2


sys.exit(main(sys.argv[1:]))
