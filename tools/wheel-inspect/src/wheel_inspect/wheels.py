# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""The pinned wheels: which files they are, where they are, and proof that they are the ones
`oracle/uv.lock` pins.

Each platform's wheel is downloaded once, from the URL in `oracle/uv.lock` and nowhere else,
into `<target>/wheel-inspect/wheels/`, and its SHA-256 and size are checked against the lock on
every run. The library and the Python module are extracted next to it and checked against the
wheel's own `RECORD`. The oracle's installed copy, when this platform has one, is compared with
them byte for byte.
"""

from __future__ import annotations

import base64
import hashlib
import os
import shutil
import tempfile
import tomllib
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path

PACKAGE = "opencolorio"
VERSION = "2.5.2"


@dataclass(frozen=True)
class PlatformSpec:
    """One platform's wheel: how to pick it from the lock and which members to inspect."""

    name: str
    wheel_suffix: str
    library: str
    module: str
    venv_site_packages: str


PLATFORMS = {
    "windows": PlatformSpec(
        "windows",
        "-cp313-cp313-win_amd64.whl",
        "PyOpenColorIO/bin/OpenColorIO_2_5.dll",
        "PyOpenColorIO/PyOpenColorIO.pyd",
        "Lib/site-packages",
    ),
    # The wheel uv installs on Rocky Linux 9 (glibc 2.34): the manylinux_2_28 build, not the
    # manylinux2014 one.
    "linux": PlatformSpec(
        "linux",
        "-cp313-cp313-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl",
        "PyOpenColorIO/libOpenColorIO.so",
        "PyOpenColorIO/PyOpenColorIO.so",
        "lib/python3.13/site-packages",
    ),
}


class WheelError(Exception):
    """A wheel is missing, or does not match the lock."""


def repo_root() -> Path:
    """The OpenColorIO-rs checkout this tool belongs to."""
    env = os.environ.get("WHEEL_INSPECT_REPO")
    starts = [Path(env)] if env else [Path(__file__).resolve(), Path.cwd().resolve()]
    for start in starts:
        for d in (start, *start.parents):
            if (d / "oracle" / "uv.lock").is_file() and (d / "rust-toolchain.toml").is_file():
                return d
    raise WheelError("cannot find the OpenColorIO-rs checkout (set WHEEL_INSPECT_REPO)")


def target_dir(root: Path) -> Path:
    """The per-platform target directory, as `ocio_testkit::paths::target_dir` picks it."""
    env = os.environ.get("CARGO_TARGET_DIR")
    if env:
        return Path(env) if Path(env).is_absolute() else root / env
    return root / "target"


@dataclass(frozen=True)
class LockedWheel:
    filename: str
    url: str
    sha256: str
    size: int


def locked_wheel(root: Path, spec: PlatformSpec) -> LockedWheel:
    """The wheel `oracle/uv.lock` pins for this platform."""
    lock = tomllib.loads((root / "oracle" / "uv.lock").read_text(encoding="utf-8"))
    packages = [p for p in lock.get("package", []) if p.get("name") == PACKAGE]
    if len(packages) != 1 or packages[0].get("version") != VERSION:
        raise WheelError(f"oracle/uv.lock does not pin {PACKAGE}=={VERSION}")
    wheels = [w for w in packages[0].get("wheels", []) if w["url"].endswith(spec.wheel_suffix)]
    if len(wheels) != 1:
        raise WheelError(f"oracle/uv.lock has {len(wheels)} wheels ending in {spec.wheel_suffix}")
    w = wheels[0]
    algo, _, digest = w["hash"].partition(":")
    if algo != "sha256":
        raise WheelError(f"unexpected hash algorithm in oracle/uv.lock: {w['hash']}")
    return LockedWheel(w["url"].rsplit("/", 1)[1], w["url"], digest, int(w["size"]))


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def _publish(path: Path, write, digest: str, what: str) -> None:
    """Writes a file under a private temporary name and moves it into place only if its
    SHA-256 is `digest`, so a failed or concurrent run never leaves a wrong file behind."""
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=path.name + ".", suffix=".part")
    try:
        with os.fdopen(fd, "wb") as f:
            write(f)
        if sha256_file(Path(tmp)) != digest:
            raise WheelError(f"{what}: SHA-256 does not match")
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def _verified_wheel(store: Path, locked: LockedWheel, log) -> Path:
    path = store / locked.filename
    if not path.is_file():
        log(f"downloading {locked.url}")

        def download(f) -> None:
            with urllib.request.urlopen(locked.url, timeout=120) as r:
                shutil.copyfileobj(r, f)

        try:
            _publish(path, download, locked.sha256, f"{locked.url} (oracle/uv.lock)")
        except OSError as e:
            raise WheelError(
                f"could not download {locked.url} ({e}); place the file at {path} by hand"
            ) from e
    if path.stat().st_size != locked.size or sha256_file(path) != locked.sha256:
        raise WheelError(f"{path}: size or SHA-256 does not match oracle/uv.lock; delete it")
    return path


def _record_digest(zf: zipfile.ZipFile, member: str) -> str:
    """The SHA-256 (hex) the wheel's RECORD lists for `member`."""
    record = next(n for n in zf.namelist() if n.endswith(".dist-info/RECORD"))
    for line in zf.read(record).decode("utf-8").splitlines():
        name, digest, _size = line.rsplit(",", 2)
        if name == member:
            algo, _, b64 = digest.partition("=")
            if algo != "sha256":
                break
            return base64.urlsafe_b64decode(b64 + "=" * (-len(b64) % 4)).hex()
    raise WheelError(f"{member} has no sha256 entry in the wheel's RECORD")


@dataclass(frozen=True)
class Member:
    """A file of the wheel, extracted and verified."""

    path: Path
    sha256: str
    size: int
    installed: Path | None  # the oracle environment's copy, when this platform has one
    installed_identical: bool | None


@dataclass(frozen=True)
class LocatedWheel:
    spec: PlatformSpec
    locked: LockedWheel
    wheel: Path
    library: Member
    module: Member


def locate(platform: str, log=lambda msg: None) -> LocatedWheel:
    """Finds (downloading once, if needed) and verifies one platform's wheel and binaries."""
    spec = PLATFORMS[platform]
    root = repo_root()
    target = target_dir(root)
    locked = locked_wheel(root, spec)
    wheel = _verified_wheel(target / "wheel-inspect" / "wheels", locked, log)
    out_dir = target / "wheel-inspect" / platform
    members = []
    with zipfile.ZipFile(wheel) as zf:
        for member in (spec.library, spec.module):
            digest = _record_digest(zf, member)
            path = out_dir / member.rsplit("/", 1)[1]
            if not path.is_file() or sha256_file(path) != digest:
                data = zf.read(member)
                _publish(path, lambda f, b=data: f.write(b), digest, f"{wheel}: {member} (RECORD)")
            installed = target / "oracle-venv" / spec.venv_site_packages / member
            identical = sha256_file(installed) == digest if installed.is_file() else None
            members.append(
                Member(
                    path,
                    digest,
                    path.stat().st_size,
                    installed if installed.is_file() else None,
                    identical,
                )
            )
    return LocatedWheel(spec, locked, wheel, members[0], members[1])
