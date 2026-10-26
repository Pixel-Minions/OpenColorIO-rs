# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Build the O7.1 client against this pinned wheel's own library and public v2.5.2 headers.

No package is installed or downloaded. Windows uses VS's release /MD runtime and an import
library generated from the wheel's actual exports; Linux selects the libstdc++ ABI its exported
symbols show. The build identity covers both native sources, every public header, and the DLL/so.
Non-package files under oracle/native also participate in oracle.rs::oracle_files cache keys.
"""

import hashlib
import os
import re
import shutil
import subprocess
import sys
import tempfile

import PyOpenColorIO as OCIO

from .child_env import allowed_environment


def _run(command, cwd, timeout=300):
    # These conventional compiler-discovery paths are deliberately supplied only to build
    # tools, never to a helper process that executes OCIO or contributes expected values.
    build_environment = allowed_environment({"ProgramFiles": "C:\\Program Files",
        "ProgramFiles(x86)": "C:\\Program Files (x86)", "ProgramW6432": "C:\\Program Files",
        "ProgramData": "C:\\ProgramData", "VSCMD_SKIP_SENDTELEMETRY": "1"} if os.name == "nt" else {"PATH": "/usr/bin:/bin"})
    process = subprocess.run(command, cwd=cwd, env=build_environment, capture_output=True,
                             timeout=timeout, check=False)
    if process.returncode:
        raise RuntimeError(f"the native oracle build failed ({process.returncode}): "
                           f"{process.stdout.decode('utf-8', 'replace')}"
                           f"{process.stderr.decode('utf-8', 'replace')}")
    return process.stdout.decode("utf-8", "replace")


def _windows_command(vcvars, command):
    from pathlib import Path

    system = Path(os.environ.get("SYSTEMROOT", "C:/Windows")) / "System32/cmd.exe"
    # A raw CreateProcess command line preserves cmd's inner quotes; list2cmdline would
    # insert backslash-quoted quotes, which cmd treats as literal backslashes.
    return f'"{system}" /d /s /c "call "{vcvars}" >nul && {command}"'


def _visual_studio(cwd):
    from pathlib import Path

    # The oracle deliberately omits installer-discovery environment variables. Prefer the
    # conventional installed layouts before asking vswhere's COM-backed instance database.
    candidates = []
    for base in ("C:/Program Files/Microsoft Visual Studio", "C:/Program Files (x86)/Microsoft Visual Studio"):
        candidates.extend(Path(base).glob("*/*/VC/Auxiliary/Build/vcvars64.bat"))
    if candidates:
        return sorted(candidates, reverse=True)[0]
    vswhere = Path("C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe")
    install = _run([str(vswhere), "-latest", "-products", "*", "-requires",
                    "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property",
                    "installationPath"], cwd).strip()
    path = Path(install) / "VC/Auxiliary/Build/vcvars64.bat"
    if not install or not path.is_file():
        raise RuntimeError("the native legacy oracle requires the installed VS x64 C++ tools")
    return path


def executable():
    """The source-keyed helper executable and its wheel provenance, built only if absent."""
    from pathlib import Path

    if OCIO.__version__ != "2.5.2":
        raise RuntimeError(f"native legacy oracle requires wheel 2.5.2, not {OCIO.__version__}")
    root = Path(__file__).resolve().parents[2]
    package = Path(OCIO.__file__).resolve().parent
    library = package / ("bin/OpenColorIO_2_5.dll" if os.name == "nt" else "libOpenColorIO.so")
    if not library.is_file():
        raise RuntimeError(f"the pinned wheel's own library was not found at {library}")
    native = root / "oracle/native"
    headers = root / "upstream/OpenColorIO/include/OpenColorIO"
    inputs = [native / "legacy_gpu.cpp", native / "transport.h", Path(__file__), headers / "OpenColorABI.h.in"]
    inputs.extend(sorted(headers.glob("*.h")))
    digest = hashlib.sha256()
    for path in inputs:
        digest.update(path.name.encode("utf-8")); digest.update(path.read_bytes())
    library_digest = hashlib.sha256(library.read_bytes()).hexdigest()
    digest.update(library_digest.encode("ascii")); digest.update(sys.platform.encode("ascii"))
    prefix = Path(sys.prefix)
    target = prefix.parent if prefix.name == "oracle-venv" else root / "target"
    build = target / "native-legacy" / digest.hexdigest()[:24]
    program = build / ("legacy_gpu.exe" if os.name == "nt" else "legacy_gpu")
    provenance = {"wheel_version": OCIO.__version__, "library_sha256": library_digest,
                  "source_key": digest.hexdigest()}
    if program.is_file():
        return program, provenance
    build.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="compile-", dir=build) as temporary:
        temporary = Path(temporary)
        include = temporary / "include/OpenColorIO"
        include.mkdir(parents=True)
        for path in headers.glob("*.h"):
            shutil.copyfile(path, include / path.name)
        abi = (headers / "OpenColorABI.h.in").read_text(encoding="utf-8")
        replacements = {"OCIO_NAMESPACE": "OpenColorIO_v2_5", "OpenColorIO_VERSION": "2.5.2",
                        "OpenColorIO_VERSION_RELEASE_TYPE": "", "OpenColorIO_VERSION_MAJOR": "2",
                        "OpenColorIO_VERSION_MINOR": "5", "OpenColorIO_VERSION_PATCH": "2"}
        for name, value in replacements.items():
            abi = abi.replace("@" + name + "@", value)
        if re.search(r"@\w+@", abi):
            raise RuntimeError("unsubstituted field in the pinned OpenColorABI.h template")
        (include / "OpenColorABI.h").write_text(abi, encoding="utf-8")
        output = temporary / program.name
        if os.name == "nt":
            vcvars = _visual_studio(temporary)
            exports = _run(_windows_command(vcvars, f'dumpbin /nologo /exports "{library}"'), temporary)
            names = re.findall(r"^\s+\d+\s+[0-9A-Fa-f]+\s+[0-9A-Fa-f]+\s+(\S+)", exports, re.M)
            if not any("getOptimizedLegacyGPUProcessor" in name for name in names):
                raise RuntimeError("the pinned wheel has no exported legacy GPU processor")
            definition = temporary / "wheel.def"
            definition.write_text("LIBRARY OpenColorIO_2_5.dll\nEXPORTS\n" + "\n".join(names) + "\n", encoding="ascii")
            import_library = temporary / "wheel.lib"
            _run(_windows_command(vcvars, f'lib /nologo /def:"{definition}" /machine:x64 /out:"{import_library}"'), temporary)
            kits = Path("C:/Program Files (x86)/Windows Kits/10")
            sdk_versions = sorted((path for path in (kits / "Include").glob("*")
                if (path / "ucrt/stddef.h").is_file() and (kits / "Lib" / path.name / "um/x64/kernel32.lib").is_file()), reverse=True)
            if not sdk_versions:
                raise RuntimeError("the native legacy oracle requires the installed Windows SDK C++ headers/libraries")
            sdk = sdk_versions[0]
            sdk_includes = " ".join(f'/I"{sdk / part}"' for part in ("ucrt", "shared", "um", "winrt"))
            sdk_lib = kits / "Lib" / sdk.name
            command = (f'cl /nologo /std:c++17 /EHsc /MD /O2 /DWIN32_LEAN_AND_MEAN /DNOMINMAX '
                       f'/I"{temporary / "include"}" {sdk_includes} "{native / "legacy_gpu.cpp"}" '
                       f'/Fe:"{output}" /link "{import_library}" /LIBPATH:"{sdk_lib / "ucrt/x64"}" '
                       f'/LIBPATH:"{sdk_lib / "um/x64"}"')
            _run(_windows_command(vcvars, command), temporary)
            # The loader looks beside the executable before PATH. This is the wheel's exact DLL.
            shutil.copyfile(library, build / library.name)
        else:
            exports = _run(["/usr/bin/nm", "-D", "--defined-only", str(library)], temporary)
            abi_flag = "1" if "__cxx11" in exports else "0"
            _run(["/usr/bin/g++", "-std=c++17", "-O2", "-DNDEBUG",
                  "-D_GLIBCXX_USE_CXX11_ABI=" + abi_flag, "-I" + str(temporary / "include"),
                  str(native / "legacy_gpu.cpp"), str(library),
                  "-Wl,-rpath," + str(library.parent), "-o", str(output)], temporary)
        os.replace(output, program)
    return program, provenance
