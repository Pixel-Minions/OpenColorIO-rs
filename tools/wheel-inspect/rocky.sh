#!/usr/bin/env bash
# wheel-inspect in the Rocky Linux 9 container (scripts/rocky9.sh):
#   tools/wheel-inspect/rocky.sh <command>
# Its Python environment lives in the container's target volume, so it never replaces the
# Windows one in tools/wheel-inspect/.venv (the container mounts this checkout).
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$root/scripts/rocky9.sh" env UV_PROJECT_ENVIRONMENT=/work/target/rocky9/wheel-inspect-venv \
    uv run --project tools/wheel-inspect wheel-inspect "$@"
