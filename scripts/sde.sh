#!/usr/bin/env bash
# Runs a command on an emulated x86 CPU with Intel SDE, so the official wheel and the port both
# dispatch to the SIMD kernels that CPU gets (PLAN.md §8). The workstation and GitHub's runners
# only cover the kernels their own CPUs select.
#   scripts/sde.sh snb cargo test -p ocio-ops --release
#   scripts/rocky9.sh scripts/sde.sh hsw cargo test -p ocio-ops --release
#
# Cargo runs each test binary through SDE (its target runner), and SDE follows child processes,
# so the oracle's Python process sees the same CPU. The CPUs, and the kernels OCIO's CPUInfo
# picks on them:
#   nhm  Nehalem          SSE4.2, no AVX      SSE2 kernels
#   snb  Sandy Bridge     AVX                 AVX kernels
#   hsw  Haswell          AVX2, slow gather   AVX2 kernels, except where slow gather rules them out
#   skl  Skylake          AVX2                AVX2 kernels
#   skx  Skylake server   AVX-512             AVX-512 kernels
# (`sde -help` lists the others.) Set up the oracle natively first (`cargo xtask oracle info`):
# under SDE everything runs several times slower.
#
# SDE comes from Intel's download mirror under Intel's license, which the owner accepted. The
# archive is downloaded on first use into target/sde-archives/ (shared by Windows and the Rocky
# container) and checked against the SHA256 pinned here before it is unpacked into
# <cargo target dir>/sde/.
set -euo pipefail

if [ $# -lt 2 ]; then
    echo "usage: scripts/sde.sh <cpu> <command...>   (cpu: nhm, snb, hsw, skl, skx, ...)" >&2
    exit 2
fi
cpu="$1"
shift

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version="10.13.1-2026-07-28"
mirror="https://downloadmirror.intel.com/924984"
case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*)
        os=win
        sha256=74e626ede09b0baa5011fc9e51b58627ea92c3fc0bae5fd7db34b490f335f651
        exe=sde.exe
        runner_var=CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER
        ;;
    Linux)
        os=lin
        sha256=94e97d623fec54385686e1e7ba65ebc9941748c05ee451423948334892bf2b50
        exe=sde64
        runner_var=CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER
        ;;
    *)
        echo "scripts/sde.sh: unsupported OS $(uname -s)" >&2
        exit 2
        ;;
esac

archive="sde-external-$version-$os.tar.xz"
archives="$root/target/sde-archives"
kits="${CARGO_TARGET_DIR:-$root/target}/sde"
kit="$kits/sde-external-$version-$os"

if [ ! -f "$kit/$exe" ]; then
    mkdir -p "$archives" "$kits"
    if [ ! -f "$archives/$archive" ]; then
        echo "scripts/sde.sh: downloading $mirror/$archive" >&2
        curl -fsSL --retry 3 -o "$archives/$archive.part" "$mirror/$archive"
        mv "$archives/$archive.part" "$archives/$archive"
    fi
    if ! echo "$sha256  $archives/$archive" | sha256sum --check --quiet -; then
        echo "scripts/sde.sh: $archive does not have the pinned SHA256 $sha256" >&2
        exit 1
    fi
    # GNU tar reads "D:/..." as a remote host; --force-local keeps Windows paths local.
    tar --force-local -xJf "$archives/$archive" -C "$kits"
fi

sde="$kit/$exe"
flags=("-$cpu")
if [ "$os" = win ]; then
    sde="$(cygpath -m "$sde")"
    # The MSVC runtime that ships with Python picks its memcpy from the CPU features Windows
    # reports, not from CPUID, so it runs AVX code on every emulated CPU and SDE's chip check
    # stops it. CPUID, which SDE emulates, still decides every OCIO and port kernel.
    flags+=(-chip_check_disable 1)
fi
export "$runner_var=$sde ${flags[*]} --"
exec "$@"
