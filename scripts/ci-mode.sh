#!/usr/bin/env bash
# Prints `flags=<the cargo xtask ci options for this CI run>` for $GITHUB_OUTPUT
# (.github/workflows/ci.yml runs it on the runner, before any container starts).
#
# - Pull requests: the ratchet baseline is the PR's base commit (`--base <base sha>`), so a
#   branch can't lower it by editing its own docs/ratchet.toml. A land commit (made by
#   `cargo xtask land`: a merge whose first parent is the PR's base) also runs --main: `main`
#   fast-forwards to it, so its generated files must be current before main moves.
# - Pushes to main, and manual runs there: --main, with the previous main as the baseline.
# - Other pushes: branch mode against the checkout's own docs/ratchet.toml.
#
# Environment: EVENT, REF, and BASE_SHA and HEAD_SHA (pull requests) or BEFORE (pushes).
set -euo pipefail
fetch() { git fetch --quiet --no-tags --depth=1 origin "$1" >&2; }
flags=""
if [ "$EVENT" = pull_request ]; then
    fetch "$BASE_SHA"
    fetch "$HEAD_SHA"
    flags="--base $BASE_SHA"
    parents="$(git cat-file -p "$HEAD_SHA" | sed -n 's/^parent //p')"
    if [ "$(printf '%s\n' "$parents" | wc -l)" -ge 2 ] \
        && [ "$(printf '%s\n' "$parents" | head -n 1)" = "$BASE_SHA" ]; then
        flags="--main $flags"
    fi
elif [ "$REF" = refs/heads/main ]; then
    flags="--main"
    if [ -n "${BEFORE:-}" ] && [ "$BEFORE" != 0000000000000000000000000000000000000000 ]; then
        fetch "$BEFORE"
        flags="--main --base $BEFORE"
    fi
fi
echo "cargo xtask ci ${flags:-(branch mode against docs/ratchet.toml in the checkout)}" >&2
echo "flags=$flags"
