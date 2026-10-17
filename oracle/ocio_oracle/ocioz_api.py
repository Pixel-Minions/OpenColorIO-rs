# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for .ocioz archives (card p4-oracle, chunk O4.5).

- ocioz: Config.archive of a config, reported entry by entry (its names, methods, flags,
  sizes, CRCs and contents), or ExtractOCIOZArchive of an archive, reported as the files,
  directories and links it writes (also where it fails partway).

Upstream's archives aren't reproducible byte for byte: each entry carries the time it was
written (OCIOZArchive.cpp:272), and the files come in the file system's order. The port
matches them entry by entry (owner decision P4-3, deviation D-6 for the deflate bytes), so the
command reports entries, not the archive's bytes. Reading archives (Config.CreateFromFile of an
.ocioz) needs no command: config_calls through with_files does it.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import os
import stat
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


def _attributes(full):
    """What the system says of the file or directory `full` itself: on Windows its
    "attributes" (st_file_attributes), elsewhere its "mode" (the permission bits of
    st_mode)."""
    st = os.lstat(full)
    if os.name == "nt":
        return {"attributes": st.st_file_attributes}
    return {"mode": stat.S_IMODE(st.st_mode)}


def _written(root, out):
    """What is under `root`: "files", "dirs" and "links", each sorted by its relative path's
    bytes ("/" between the components; on Linux a backslash is part of a name).

    A file is {"path": {"bytes": hex}, "contents": a blob index (its bytes go to `out`), or
    null where it can't be read, "mtime_ns", and "attributes" or "mode" (_attributes)}; a
    directory {"path", and "attributes" or "mode"}; a symbolic link {"path", "target":
    {"bytes": hex}}. os.walk doesn't follow links to directories, and a link isn't read
    through."""
    files, dirs, links = [], [], []
    for directory, dirnames, filenames in os.walk(root):
        for name in dirnames + filenames:
            full = os.path.join(directory, name)
            relative = os.fsencode(os.path.relpath(full, root))
            relative = relative.replace(os.fsencode(os.sep), b"/")
            entry = {"path": {"bytes": relative.hex()}}
            if os.path.islink(full):
                entry["target"] = {"bytes": os.fsencode(os.readlink(full)).hex()}
                links.append(entry)
            elif os.path.isdir(full):
                entry.update(_attributes(full))
                dirs.append(entry)
            else:
                try:
                    with open(full, "rb") as f:
                        out.append(f.read())
                    entry["contents"] = len(out) - 1
                except OSError:
                    entry["contents"] = None
                entry["mtime_ns"] = os.lstat(full).st_mtime_ns
                entry.update(_attributes(full))
                files.append(entry)

    def by_path(e):
        return bytes.fromhex(e["path"]["bytes"])

    return {"files": sorted(files, key=by_path), "dirs": sorted(dirs, key=by_path),
            "links": sorted(links, key=by_path)}


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
      extract:    "files", "dirs" and "links": everything written under the destination
                  (see _written), each sorted by its relative path's bytes ("/" separated),
                  "path" {"bytes": hex}; a file has "contents" (a blob index, or null
                  where it can't be read), "mtime_ns" and "attributes" (Windows) or "mode"
                  (elsewhere), a directory "attributes" or "mode", a link "target"
                  {"bytes": hex}
      exception, stage   when OCIO or the binding raised: {"type", "message"}, and where:
                  "config", "archive" or "extract"; at the "extract" stage, also "files",
                  "dirs" and "links": what the extraction wrote before it failed
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
                result.update(_written(destination, out))
        except RAISED as exc:
            if not wheel_raised(exc):
                raise
            result, out = {"exception": exception_result(exc), "stage": stage[0]}, []
            if stage[0] == "extract":
                result.update(_written(destination, out))
    result["log"] = log
    return result, out
