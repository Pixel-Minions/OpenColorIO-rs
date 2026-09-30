#!/usr/bin/env bash
# Runs a command in the Rocky Linux 9 reference container, with this checkout at /work.
#   scripts/rocky9.sh cargo test --workspace
#   scripts/rocky9.sh cargo xtask oracle check-all
#
# Build output, the oracle's environment and its cache live in a Docker volume per checkout
# (fast Linux filesystem, never shared with Windows); the cargo registry and the uv cache are
# shared volumes. The checkout's volume is ocio-rs-target-<id>: <id> is the first 12 hex digits
# of the MD5 of the checkout's canonical path (below). It is created with two labels,
# ocio-rs.checkout (the checkout's path) and ocio-rs.git-common-dir (its repository), which
# `cargo xtask clean-scratch` reads to tell whose volume it is.
#
# In a linked worktree (`git worktree add`), git's metadata lives in the main checkout's .git,
# outside /work. The upstream submodule's git directory is then mounted read-only where the
# submodule's relative .git file points inside the container, so that `cargo xtask guards` can
# read the submodule's commit there too.
#
# OCIO_RS_TIER (the test tier) and RUST_TEST_THREADS pass through to the container when they are
# set here.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="ocio-rs-rocky9:1"
if ! docker image inspect "$image" >/dev/null 2>&1; then
    docker build -t "$image" "$root/docker/rocky9"
fi

# A directory as the host names it. In Git Bash, `pwd` can print an MSYS mount such as /tmp
# (Git for Windows mounts %TEMP% there), which Docker Desktop would look up inside its own VM;
# `pwd -W` prints the Windows path (D:/...). Elsewhere `pwd -W` fails and `pwd` is used.
host_path() { (cd "$1" && { pwd -W 2>/dev/null || pwd; }); }

# The canonical form that volume ids hash: forward slashes (as host_path prints them) and, for
# Windows paths (a drive letter or //), ASCII lower case, since Windows ignores case.
# `cargo xtask clean-scratch` computes the same form.
canonical() {
    case "$1" in
        [A-Za-z]:/* | //*) printf '%s' "$1" | LC_ALL=C tr 'A-Z' 'a-z' ;;
        *) printf '%s' "$1" ;;
    esac
}

# Removes `.` and `..` from an absolute path without touching the filesystem: /a/b/../c -> /a/c.
normalize() {
    local IFS=/ part
    local -a parts out=()
    read -ra parts <<<"$1"
    for part in "${parts[@]}"; do
        case "$part" in
            "" | .) ;;
            ..) if [ "${#out[@]}" -gt 0 ]; then unset "out[$((${#out[@]} - 1))]"; fi ;;
            *) out+=("$part") ;;
        esac
    done
    printf '/%s' "${out[@]}"
}

checkout="$(host_path "$root")"
volume="ocio-rs-target-$(canonical "$checkout" | md5sum | cut -c1-12)"
repository="$(git -C "$root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"

extra_mounts=()
submodule="upstream/OpenColorIO"
if [ -f "$root/$submodule/.git" ]; then
    gitdir="$(sed -n 's/^gitdir: *//p' "$root/$submodule/.git" | tr -d '\r')"
    case "$gitdir" in
        "" | /* | [A-Za-z]:*) ;; # absolute: git can't follow it inside the container anyway
        *)
            inside="$(normalize "/work/$submodule/$gitdir")"
            case "$inside" in
                /work | /work/*) ;; # inside the checkout: already mounted
                *)
                    if host="$(host_path "$root/$submodule/$gitdir" 2>/dev/null)"; then
                        extra_mounts+=(-v "$host:$inside:ro")
                    fi
                    ;;
            esac
            ;;
    esac
fi

# Git Bash on Windows rewrites paths; keep /work and the labels as they are.
export MSYS_NO_PATHCONV=1
# Labels are set only when the volume is created; `docker volume create` keeps an existing one.
if ! docker volume inspect "$volume" >/dev/null 2>&1; then
    docker volume create \
        --label "ocio-rs.checkout=$checkout" \
        --label "ocio-rs.git-common-dir=$repository" \
        "$volume" >/dev/null
fi
tty_flag=""
if [ -t 0 ] && [ -t 1 ]; then tty_flag="-t"; fi
docker run --rm -i $tty_flag \
    -v "$checkout:/work" \
    -v "$volume:/work/target/rocky9" \
    -v ocio-rs-cargo-registry:/opt/cargo/registry \
    -v ocio-rs-uv-cache:/opt/uv/cache \
    ${extra_mounts[@]+"${extra_mounts[@]}"} \
    -e CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-auto}" \
    -e OCIO_RS_TIER \
    -e RUST_TEST_THREADS \
    "$image" "$@"
