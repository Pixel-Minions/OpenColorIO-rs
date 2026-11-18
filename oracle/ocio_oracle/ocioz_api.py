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

import io
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


def _ns(value):
    """A time in nanoseconds since 1970: the integer where it fits 64 bits, else {"decimal":
    its digits} (a FILETIME near 2^63 is a time of the year 30828, 9.1e20 ns, past what JSON
    readers take as an integer)."""
    if -(1 << 63) <= value < (1 << 63):
        return value
    return {"decimal": str(value)}


def _stat(st):
    """What lstat says of a file, directory or link itself: its times ("mtime_ns", "atime_ns",
    and on Windows "birthtime_ns", the three times minizip-ng sets; see _ns) and on Windows
    its "attributes" (st_file_attributes), elsewhere its "mode" (the permission bits of
    st_mode)."""
    if os.name == "nt":
        return {"mtime_ns": _ns(st.st_mtime_ns), "atime_ns": _ns(st.st_atime_ns),
                "birthtime_ns": _ns(st.st_birthtime_ns), "attributes": st.st_file_attributes}
    return {"mtime_ns": _ns(st.st_mtime_ns), "atime_ns": _ns(st.st_atime_ns),
            "mode": stat.S_IMODE(st.st_mode)}


def _read(path, out):
    """The blob index of `path`'s bytes (they go to `out`), or None where reading it is
    refused (PermissionError)."""
    try:
        with open(path, "rb") as f:
            out.append(f.read())
    except PermissionError:
        return None
    return len(out) - 1


def _streams(path, out):
    """On Windows, the alternate data streams of the file or directory `path` (an entry named
    `a.txt:s` writes one), each {"name": {"bytes": the UTF-8 of ":s:$DATA"}, "contents": a blob
    index or null}, in the order FindFirstStreamW lists them; elsewhere none."""
    if os.name != "nt":
        return []
    import ctypes
    from ctypes import wintypes

    class FindStreamData(ctypes.Structure):
        _fields_ = [("StreamSize", ctypes.c_longlong), ("cStreamName", ctypes.c_wchar * 296)]

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.FindFirstStreamW.restype = wintypes.HANDLE
    kernel32.FindFirstStreamW.argtypes = [wintypes.LPCWSTR, ctypes.c_int, ctypes.c_void_p,
                                          wintypes.DWORD]
    kernel32.FindNextStreamW.argtypes = [wintypes.HANDLE, ctypes.c_void_p]
    kernel32.FindClose.argtypes = [wintypes.HANDLE]
    data = FindStreamData()
    handle = kernel32.FindFirstStreamW(path, 0, ctypes.byref(data), 0)
    if handle in (None, wintypes.HANDLE(-1).value):
        return []
    names = []
    try:
        while True:
            if data.cStreamName != "::$DATA":
                names.append(data.cStreamName)
            if not kernel32.FindNextStreamW(handle, ctypes.byref(data)):
                break
    finally:
        kernel32.FindClose(handle)
    streams = []
    for name in names:
        stream = name[: -len(":$DATA")] if name.endswith(":$DATA") else name
        streams.append({"name": {"bytes": name.encode("utf-8").hex()},
                        "contents": _read(path + stream, out)})
    return streams


def _written(root, out, skip=None):
    """What is under `root` (but the directory `skip` at its top): "files", "dirs" and "links",
    each sorted by its relative path's bytes ("/" between the components; on Linux a backslash
    is part of a name).

    A file is {"path": {"bytes": hex}, "contents": a blob index (its bytes go to `out`), or
    null where reading it is refused, and what _stat says}, a directory {"path", and what _stat
    says}, a symbolic link {"path", "target": {"bytes": hex}, and on Windows its "attributes"};
    on Windows a file or directory with alternate data streams has "streams" (_streams). Each
    is lstat'ed before it is read, so that reading can't change its access time. os.walk
    doesn't follow links to directories, and a link isn't read through. A Windows junction
    is a link too, with "junction": true (os.walk would walk into it: it is taken out)."""
    files, dirs, links = [], [], []
    for directory, dirnames, filenames in os.walk(root):
        if directory == root and skip is not None and skip in dirnames:
            dirnames.remove(skip)
        junctions = [n for n in dirnames if os.path.isjunction(os.path.join(directory, n))]
        for name in junctions:
            dirnames.remove(name)
        for name in dirnames + junctions + filenames:
            full = os.path.join(directory, name)
            relative = os.fsencode(os.path.relpath(full, root))
            relative = relative.replace(os.fsencode(os.sep), b"/")
            entry = {"path": {"bytes": relative.hex()}}
            st = os.lstat(full)
            if name in junctions:
                entry["junction"] = True
            if (name in junctions or stat.S_ISLNK(st.st_mode)
                    or (os.name == "nt" and os.path.islink(full))):
                entry["target"] = {"bytes": os.fsencode(os.readlink(full)).hex()}
                if os.name == "nt":
                    entry["attributes"] = st.st_file_attributes
                links.append(entry)
                continue
            entry.update(_stat(st))
            if stat.S_ISDIR(st.st_mode):
                dirs.append(entry)
            else:
                entry["contents"] = _read(full, out)
                files.append(entry)
            streams = _streams(full, out)
            if streams:
                entry["streams"] = streams

    def by_path(e):
        return bytes.fromhex(e["path"]["bytes"])

    return {"files": sorted(files, key=by_path), "dirs": sorted(dirs, key=by_path),
            "links": sorted(links, key=by_path)}


def _is_link(create_system, attributes):
    """Whether minizip-ng takes an entry for a symbolic link (mz_zip_attrib_is_symlink,
    mz_zip.c @ 4.0.10): a reparse point of an MS-DOS or NTFS host, an S_IFLNK mode of a Unix,
    OS X or RISC OS host."""
    if create_system in (0, 10):
        return attributes & 0x400 != 0
    if create_system in (3, 13, 19):
        mode = attributes >> 16 if attributes >> 16 else attributes
        return mode & 0o170000 == 0o120000
    return False


def _c_string(data):
    return data.split(b"\0", 1)[0]


def _unix1_link(extra):
    """The link name of an extra field's UNIX1 field (0x000d), or None."""
    at = 0
    while at + 4 <= len(extra):
        kind, length = int.from_bytes(extra[at:at + 2], "little"), \
            int.from_bytes(extra[at + 2:at + 4], "little")
        if kind == 0x000D and length > 12:
            return _c_string(extra[at + 16:at + 4 + length])
        at += 4 + length
    return None


def _central_headers(data):
    """Every central header the bytes hold, wherever it is (minizip-ng may read a central
    directory Python's zipfile doesn't): (name, made-by system, attributes, extra)."""
    headers = []
    at = data.find(b"PK\x01\x02")
    while at >= 0 and at + 46 <= len(data):
        system = data[at + 5]
        name_len = int.from_bytes(data[at + 28:at + 30], "little")
        extra_len = int.from_bytes(data[at + 30:at + 32], "little")
        attributes = int.from_bytes(data[at + 38:at + 42], "little")
        name = data[at + 46:at + 46 + name_len]
        extra = data[at + 46 + name_len:at + 46 + name_len + extra_len]
        headers.append((name, system, attributes, extra))
        at = data.find(b"PK\x01\x02", at + 1)
    return headers


def _parts(name, windows):
    """The components of a name or link target as minizip-ng resolves them (mz_path_resolve):
    "." and empty ones dropped, ".." taking the one before; on Windows in lower case, as its
    file system compares names."""
    if windows:
        name = name.replace(b"\\", b"/").lower()
    parts = []
    for part in name.split(b"/"):
        if part in (b"", b"."):
            continue
        if part == b"..":
            if parts:
                parts.pop()
            continue
        parts.append(part)
    return parts


def _check_links(archive):
    """Refuses (RequestError) an archive whose links could make the extraction write outside
    the request's temporary directory, so that nothing outlives the request. The extraction
    goes into `<root>/extracted`; a link is taken where minizip-ng could take one (_is_link),
    with the targets its UNIX1 field and its contents give. Refused: an absolute target (a
    leading separator, or a ':' on Windows); a target whose ".." go above `<root>`; a link whose
    name, or whose target, goes through another link, or back into `<root>/extracted` (whose
    effect a lexical check can't follow). An archive it can't read is the wheel's to refuse.
    Returns how many link entries the archive has."""
    windows = os.name == "nt"
    try:
        with open(archive, "rb") as f:
            data = f.read()
    except OSError:
        # Nor can the wheel read it.
        return 0
    try:
        contents = {}
        with zipfile.ZipFile(io.BytesIO(data)) as z:
            for info in z.infolist():
                if _is_link(info.create_system, info.external_attr) and info.file_size < 0xFFFF:
                    contents.setdefault(_name_bytes(info), z.read(info))
    except (zipfile.BadZipFile, zipfile.LargeZipFile, OSError, EOFError, ValueError,
            NotImplementedError, RuntimeError, KeyError) as exc:
        contents = None
        error = exc
    links = []
    for name, system, attributes, extra in _central_headers(data):
        if not _is_link(system, attributes):
            continue
        targets = []
        unix1 = _unix1_link(extra)
        if unix1:
            targets.append(unix1)
        if contents is None or name not in contents:
            if not unix1:
                raise RequestError(f"ocioz: can't read the target of the link entry {name!r} "
                                   f"({error if contents is None else 'not in zipfile'})")
        else:
            targets.append(_c_string(contents[name]))
        links.append((_parts(name, windows), targets))
    names = {tuple(parts) for parts, _ in links}
    for parts, targets in links:
        for i in range(1, len(parts)):
            if tuple(parts[:i]) in names:
                raise RequestError(f"ocioz: the link {b'/'.join(parts)!r} is inside another link")
        for target in targets:
            if target.startswith((b"/", b"\\") if windows else b"/") or (windows and b":" in target):
                raise RequestError(f"ocioz: the link {b'/'.join(parts)!r} has an absolute target")
            # Depth below <root>: <root>/extracted is 1, the link's directory 1 + its parents.
            depth = len(parts)
            here = list(parts[:-1])
            if windows:
                target = target.replace(b"\\", b"/").lower()
            for part in target.split(b"/"):
                if part in (b"", b"."):
                    continue
                if part == b"..":
                    depth -= 1
                    if here:
                        here.pop()
                    if depth < 0:
                        raise RequestError(
                            f"ocioz: the link {b'/'.join(parts)!r} leads out of the request's "
                            "directory")
                    continue
                if depth == 0:
                    if part == b"extracted":
                        raise RequestError(
                            f"ocioz: the link {b'/'.join(parts)!r} goes back into the "
                            "destination")
                    here = None
                    depth = 1
                    continue
                depth += 1
                if here is not None:
                    here.append(part)
                    # Through itself, it's a loop the system refuses.
                    if tuple(here) in names and here != parts:
                        raise RequestError(
                            f"ocioz: the link {b'/'.join(parts)!r} goes through another link")
    return len(links)


def _inside(root, path):
    """Whether the absolute `path`, normalized, is `root` or under it (on Windows ignoring
    case, as its file system does)."""
    path, root = os.path.normpath(path), os.path.normpath(root)
    if os.name == "nt":
        path, root = os.path.normcase(path), os.path.normcase(root)
    return path == root or path.startswith(root + os.sep)


def _check_prepare(items):
    """The "prepare" list checked: each {"path", "kind", and "target" for a link}. Refused
    (RequestError): another key or kind; a path or target that isn't text, is absolute (or on
    Windows holds a ':'), or leads out of the request's directory, lexically; a path, or a
    link's target, that is or goes through an earlier item's link (so that each lexical check
    is the system's own). Returns whether the list makes a link."""
    if not isinstance(items, list):
        raise RequestError("ocioz: prepare must be a list")
    probe = os.path.join(os.sep, "root")

    def norm(path):
        return os.path.normcase(os.path.normpath(path))

    def through(start, relative):
        # Each prefix of `relative` from `start`, before ".." takes anything away.
        current = start
        if os.name == "nt":
            relative = relative.replace("\\", "/")
        for part in relative.split("/"):
            current = os.path.join(current, part)
            if norm(current) in links:
                return True
        return False

    links = []
    for item in items:
        if not isinstance(item, dict):
            raise RequestError(f"ocioz: a prepare item must be an object, not {item!r}")
        kind = item.get("kind")
        if kind not in ("dir", "junction", "symlink"):
            raise RequestError(f"ocioz: unknown prepare kind {kind!r}")
        check_keys("ocioz: prepare", item, {"path", "kind"} | ({"target"} if kind != "dir"
                                                                else set()))
        names = [("path", item.get("path"))]
        if kind != "dir":
            names.append(("target", item.get("target")))
        for key, value in names:
            if not isinstance(value, str) or not value:
                raise RequestError(f"ocioz: prepare {key} must be text, not {value!r}")
            if os.path.isabs(value) or value.startswith(("/", "\\")) or (
                    os.name == "nt" and ":" in value):
                raise RequestError(f"ocioz: prepare {key} {value!r} is absolute")
        path = os.path.join(probe, item["path"])
        if not _inside(probe, path) or os.path.normpath(path) == os.path.normpath(probe):
            raise RequestError(f"ocioz: prepare path {item['path']!r} leads out of the "
                               "request's directory")
        if through(probe, item["path"]):
            raise RequestError(f"ocioz: prepare path {item['path']!r} goes through a link")
        if kind == "junction" and not _inside(probe, os.path.join(probe, item["target"])):
            raise RequestError(f"ocioz: prepare target {item['target']!r} leads out of the "
                               "request's directory")
        if kind == "symlink" and not _inside(
                probe, os.path.join(os.path.dirname(path), item["target"])):
            raise RequestError(f"ocioz: prepare target {item['target']!r} leads out of the "
                               "request's directory")
        if kind == "junction" and through(probe, item["target"]) or (
                kind == "symlink" and through(os.path.dirname(path), item["target"])):
            raise RequestError(f"ocioz: prepare target {item['target']!r} goes through a link")
        if kind != "dir":
            links.append(norm(path))
    return bool(links)


def _prepare(root, items):
    """Makes the "prepare" items under `root`, in their order (_check_prepare checked them), and
    reports for each whether it was made: a directory (os.makedirs), a junction (Windows only:
    _winapi.CreateJunction, its target relative to `root`) and a symbolic link (os.symlink of
    the target as given, relative to the link's directory; a directory link on Windows where
    the target ends with a separator). _check_prepare made sure that no path or target goes
    through a link, so each stays inside `root` as the system resolves it."""
    made = []
    for item in items:
        path = os.path.join(root, item["path"])
        try:
            if item["kind"] == "dir":
                os.makedirs(path)
            elif item["kind"] == "junction":
                if os.name != "nt":
                    raise OSError("junctions are Windows'")
                import _winapi
                _winapi.CreateJunction(os.path.join(root, item["target"]), path)
            else:
                os.symlink(item["target"], path,
                           target_is_directory=item["target"].endswith(("/", "\\")))
        except OSError:
            made.append(False)
            continue
        made.append(True)
    return made


def _name_bytes(info):
    encoding = "utf-8" if info.flag_bits & 0x800 else "cp437"
    return info.filename.encode(encoding)


@command
def ocioz(args, blobs):
    """Archives a config, or extracts an archive.

    args: one of
      archive     {"config": a config (spec.config, e.g. {"file": "$FILES/config.ocio"} through
                  with_files)}: Config.archive(path) into a new temporary file
      extract     {"archive": path (text or {"bytes": hex}), "destination" (optional): how the
                  destination is spelled, relative to the new temporary directory, a path that
                  normalizes to "extracted" (default "extracted", e.g. "sub/../extracted")}:
                  ExtractOCIOZArchive(archive, destination) into the directory "extracted" of
                  a new temporary directory; "prepare" (optional): a list of items made in
                  that temporary directory first, in order (_prepare): {"path": relative to
                  it, "kind": "dir"}, {"path", "kind": "junction", "target": relative to it}
                  or {"path", "kind": "symlink", "target": as the link holds it, relative to
                  its directory}; paths and targets that are absolute or lead out of it are
                  refused (_check_prepare), and so are link entries in the archive where
                  the list makes a link (whose effect a lexical check can't follow)
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
                  "path" {"bytes": hex}; a file has "contents" (a blob index, or null where
                  reading it is refused), "mtime_ns", "atime_ns", on Windows "birthtime_ns"
                  (integers, or {"decimal": digits} past 64 bits)
                  and "attributes", elsewhere "mode"; a directory the same but "contents"; a
                  link "target" {"bytes": hex} and on Windows "attributes"; on Windows a file
                  or directory with alternate data streams "streams". "outside": the same, for
                  what the extraction wrote in the temporary directory outside the destination
                  (through links); "prepared": with "prepare", whether each item was made (a
                  list of booleans: a symbolic link needs the privilege, or Windows' Developer
                  Mode, which os.symlink asks for); what the items made is reported with the
                  rest
      exception, stage   when OCIO or the binding raised: {"type", "message"}, and where:
                  "config", "archive" or "extract"; at the "extract" stage, also "files",
                  "dirs", "links" and "outside": what the extraction wrote before it failed
      log         OCIO's log messages
    blobs: the contents, as the result indexes them

    The times minizip-ng gives a file from an entry's DOS date are its local time zone's
    (mktime): they depend on the process's time zone, which the oracle's cache key doesn't
    cover, so a test compares them only where it knows the zone (the NTFS times don't depend
    on it).

    An unknown key, neither or both of archive and extract, or a path it can't read, is
    refused; and an archive whose links could make the extraction write outside the request's
    temporary directory (_check_links), so that nothing outlives the request.
    """
    check_keys("ocioz", args, {"archive", "extract"})
    if ("archive" in args) == ("extract" in args):
        raise RequestError("ocioz takes archive or extract, not both or neither")
    if "extract" in args:
        check_keys("ocioz: extract", args["extract"], {"archive", "destination", "prepare"})
        archive = text("extract archive", args["extract"].get("archive"))
        archive_links = _check_links(archive)
        prepare = args["extract"].get("prepare")
        if prepare is not None and _check_prepare(prepare) and archive_links:
            raise RequestError("ocioz: an archive with link entries can't be extracted where "
                               "prepare makes links")
        spelled = args["extract"].get("destination", "extracted")
        if (not isinstance(spelled, str) or os.path.isabs(spelled)
                or os.path.normpath(spelled) != "extracted"):
            raise RequestError(f"ocioz: the destination {spelled!r} must be a spelling of "
                               "\"extracted\" relative to the request's directory")
    stage, result, out = ["config"], {}, []
    with captured_log() as log, tempfile.TemporaryDirectory() as root:
        destination = os.path.join(root, "extracted")
        spelled_destination = os.path.join(root, spelled) if "extract" in args else destination

        def written(out):
            found = _written(destination, out)
            found["outside"] = _written(root, out, skip="extracted")
            return found

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
                if prepare is not None:
                    prepared = _prepare(root, prepare)
                stage[0] = "extract"
                OCIO.ExtractOCIOZArchive(archive, spelled_destination)
                result.update(written(out))
        except RAISED as exc:
            if not wheel_raised(exc):
                raise
            result, out = {"exception": exception_result(exc), "stage": stage[0]}, []
            if stage[0] == "extract":
                result.update(written(out))
        if "extract" in args and prepare is not None:
            result["prepared"] = prepared
    result["log"] = log
    return result, out
