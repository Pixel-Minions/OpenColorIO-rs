#!/usr/bin/env bash
# Runs a command in the Rocky Linux 9 reference container, with this checkout at /work.
#   scripts/rocky9.sh cargo test --workspace
#   scripts/rocky9.sh cargo xtask oracle check-all
#
# Build output, the oracle's environment and its cache live in a Docker volume per checkout
# (fast Linux filesystem, never shared with Windows); the cargo registry and the uv cache are
# shared volumes.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="ocio-rs-rocky9:1"
if ! docker image inspect "$image" >/dev/null 2>&1; then
    docker build -t "$image" "$root/docker/rocky9"
fi
checkout_id="$(printf '%s' "$root" | md5sum | cut -c1-12)"
# Git Bash on Windows rewrites paths; keep /work as is.
export MSYS_NO_PATHCONV=1
tty_flag=""
if [ -t 0 ] && [ -t 1 ]; then tty_flag="-t"; fi
docker run --rm -i $tty_flag \
    -v "$root:/work" \
    -v "ocio-rs-target-$checkout_id:/work/target/rocky9" \
    -v ocio-rs-cargo-registry:/opt/cargo/registry \
    -v ocio-rs-uv-cache:/opt/uv/cache \
    -e CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-auto}" \
    "$image" "$@"
