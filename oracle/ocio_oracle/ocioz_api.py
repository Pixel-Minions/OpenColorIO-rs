# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for .ocioz archives (card p4-oracle, chunk O4.5).

- ocioz: Config.archive of a config, reported entry by entry (its names, methods, flags,
  sizes, CRCs and contents), or ExtractOCIOZArchive of an archive, reported as the files it
  writes.

Upstream's archives aren't reproducible byte for byte: each entry carries the time it was
written (OCIOZArchive.cpp:272), and the files come in the file system's order. The port
matches them entry by entry (owner decision P4-3, deviation D-6 for the deflate bytes), so the
command reports entries, not the archive's bytes. Reading archives (Config.CreateFromFile of an
.ocioz) needs no command: config_calls through with_files does it.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import os
import tempfile
import zipfile

import PyOpenColorIO as OCIO

from . import spec
from .checks import check_keys
from .commands import RAISED, captured_log, command, exception_result, wheel_raised
from .config_api import RequestError, text


def _name(info):
    """An entry's name as the archive stores it: UTF-8 when its flag says so, else CP437."""
    encoding = "utf-8" if info.flag_bits & 0x800 else "cp437"
    return {"bytes": info.filename.encode(encoding).hex()}


def _entries(path, out):
    """The entries of the zip archive at `path`, in order; their contents go to `out` (blobs)."""
    entries = []
    with zipfile.ZipFile(path) as archive:
        for info in archive.infolist():
            out.append(archive.read(info))
            entries.append({
                "name": _name(info),
                "method": info.compress_type,
                "flag_bits": info.flag_bits,
                "create_system": info.create_system,
                "create_version": info.create_version,
                "extract_version": info.extract_version,
                "internal_attr": info.internal_attr,
                "external_attr": info.external_attr,
                "extra": info.extra.hex(),
                "crc": info.CRC,
                "size": info.file_size,
                "compressed_size": info.compress_size,
                "date_time": list(info.date_time),
                "contents": len(out) - 1,
            })
    return entries


def _written(root, out):
    """The files under `root`, by relative path ("/" separated) in sorted order; their contents
    go to `out` (blobs)."""
    files = []
    for directory, _, names in os.walk(root):
        for name in names:
            full = os.path.join(directory, name)
            with open(full, "rb") as f:
                out.append(f.read())
            relative = os.path.relpath(full, root).replace(os.sep, "/")
            files.append({"path": relative, "contents": len(out) - 1})
    return sorted(files, key=lambda f: f["path"])


@command
def ocioz(args, blobs):
    """Archives a config, or extracts an archive.

    args: one of
      archive     {"config": a config (spec.config, e.g. {"file": "$FILES/config.ocio"} through
                  with_files)}: Config.archive(path) into a new temporary file
      extract     {"archive": path (text or {"bytes": hex})}: ExtractOCIOZArchive(archive,
                  destination) into a new temporary directory
    result:
      archive:    "entries": in the archive's order, each {"name": {"bytes": hex}, "method",
                  "flag_bits", "create_system", "create_version", "extract_version",
                  "internal_attr", "external_attr", "extra" (hex), "crc", "size",
                  "compressed_size", "date_time" ([Y, M, D, h, m, s]), "contents" (a blob
                  index)}; "archive" the archive's bytes (a blob index), to extract or read
                  again. Its bytes, the dates, and the compressed sizes aren't the port's to
                  match; the "extra" fields and "external_attr" come from the files' own
                  attributes and times (on Windows, NTFS times)
      extract:    "files": every file written, by relative path ("/" separated, sorted),
                  each {"path", "contents" (a blob index)}
      exception, stage   when OCIO or the binding raised: {"type", "message"}, and where:
                  "config", "archive" or "extract"
      log         OCIO's log messages
    blobs: the contents, as the result indexes them

    An unknown key, neither or both of archive and extract, or a path it can't read, is
    refused.
    """
    check_keys("ocioz", args, {"archive", "extract"})
    if ("archive" in args) == ("extract" in args):
        raise RequestError("ocioz takes archive or extract, not both or neither")
    stage, result, out = ["config"], {}, []
    with captured_log() as log, tempfile.TemporaryDirectory() as root:
        try:
            if "archive" in args:
                check_keys("ocioz: archive", args["archive"], {"config"})
                config = spec.config(args["archive"].get("config"))
                stage[0] = "archive"
                path = os.path.join(root, "archived.ocioz")
                config.archive(path)
                result["entries"] = _entries(path, out)
                with open(path, "rb") as f:
                    out.append(f.read())
                result["archive"] = len(out) - 1
            else:
                check_keys("ocioz: extract", args["extract"], {"archive"})
                archive = text("extract archive", args["extract"].get("archive"))
                stage[0] = "extract"
                destination = os.path.join(root, "extracted")
                OCIO.ExtractOCIOZArchive(archive, destination)
                result["files"] = _written(destination, out)
        except RAISED as exc:
            if not wheel_raised(exc):
                raise
            result, out = {"exception": exception_result(exc), "stage": stage[0]}, []
    result["log"] = log
    return result, out
