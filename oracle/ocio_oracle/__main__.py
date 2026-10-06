# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Entry point.

Framed mode (no arguments): one request on stdin, one response on stdout.

    u32 little-endian header length | header (UTF-8 JSON) | blob 0 | blob 1 | ...

Request header: {"cmd": str, "args": object, "blobs": [len, ...]}
Response header: {"ok": true, "result": any, "blobs": [len, ...]} or {"ok": false, "error": str}

A command that raises gets the second form. If writing the response fails once part of it
went out, nothing more is written: the traceback goes to stderr and the process exits with 1,
so the caller never reads a frame cut short as a response.

CLI mode: ``python -m ocio_oracle regen <group> <out_dir>`` writes fixture files and prints
a JSON list of the files it wrote.
"""

import json
import struct
import sys
import traceback

from . import commands, regen

# The most bytes one read or write moves through a pipe, as on the caller's side
# (PIPE_PIECE, crates/ocio-testkit/src/oracle.rs). On Windows, a single pipe read or write of
# several megabytes can fail when the system is short of memory.
PIPE_PIECE = 64 * 1024


def _read_exact(raw, n):
    """Reads exactly n bytes from the unbuffered stream raw, at most PIPE_PIECE per read."""
    data = bytearray(n)
    view = memoryview(data)
    got = 0
    while got < n:
        count = raw.readinto(view[got:got + PIPE_PIECE])
        if not count:
            raise EOFError(f"expected {n} bytes, got {got}")
        got += count
    return bytes(data)


def _frame(header, blobs):
    """The pieces of a response frame: the length and header, then each blob."""
    header = dict(header)
    header["blobs"] = [len(b) for b in blobs]
    encoded = json.dumps(header, separators=(",", ":"), allow_nan=False).encode("utf-8")
    return [struct.pack("<I", len(encoded)) + encoded, *blobs]


def _write_all(raw, pieces):
    """Writes every piece to the unbuffered stream raw, at most PIPE_PIECE per write."""
    for piece in pieces:
        view = memoryview(piece).cast("B")
        sent = 0
        while sent < len(view):
            count = raw.write(view[sent:sent + PIPE_PIECE])
            if not count:
                raise OSError(f"writing the response stopped after {sent} bytes of a piece")
            sent += count


def serve_one():
    stdin = sys.stdin.buffer.raw
    stdout = sys.stdout.buffer.raw
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
        frame = _frame({"ok": True, "result": result}, out_blobs)
    except Exception:  # noqa: BLE001 - every failure is reported to the caller
        frame = _frame({"ok": False, "error": traceback.format_exc()}, [])
    # Nothing has been written yet. A failure from here on raises out of the process: once
    # part of the frame went out, a second frame after it would read as a truncated response.
    _write_all(stdout, frame)


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
