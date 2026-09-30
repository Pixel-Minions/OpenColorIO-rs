#!/usr/bin/env bash
# Runs a command in the Rocky Linux 9 reference container, with this checkout at /work.
#   scripts/rocky9.sh cargo test --workspace
#   scripts/rocky9.sh cargo xtask oracle check-all
#
# Build output, the oracle's environment and its cache live in a Docker volume per checkout
# (fast Linux filesystem, never shared with Windows); the cargo registry and the uv cache are
# shared volumes.
#
# In a linked worktree (`git worktree add`), git's metadata lives in the main checkout's .git,
# outside /work. The upstream submodule's git directory is then mounted read-only where the
# submodule's relative .git file points inside the container, so that `cargo xtask guards` can
# read the submodule's commit there too.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="ocio-rs-rocky9:1"
if ! docker image inspect "$image" >/dev/null 2>&1; then
    docker build -t "$image" "$root/docker/rocky9"
fi
checkout_id="$(printf '%s' "$root" | md5sum | cut -c1-12)"

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
                    if host="$(cd "$root/$submodule/$gitdir" 2>/dev/null && pwd)"; then
                        extra_mounts+=(-v "$host:$inside:ro")
                    fi
                    ;;
            esac
            ;;
    esac
fi

# Git Bash on Windows rewrites paths; keep /work as is.
export MSYS_NO_PATHCONV=1
tty_flag=""
if [ -t 0 ] && [ -t 1 ]; then tty_flag="-t"; fi
docker run --rm -i $tty_flag \
    -v "$root:/work" \
    -v "ocio-rs-target-$checkout_id:/work/target/rocky9" \
    -v ocio-rs-cargo-registry:/opt/cargo/registry \
    -v ocio-rs-uv-cache:/opt/uv/cache \
    ${extra_mounts[@]+"${extra_mounts[@]}"} \
    -e CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-auto}" \
    "$image" "$@"
