#!/usr/bin/env bash
# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
#
# Fetches the `aces` corpus (corpus/README.md) into <root>/aces, and checks every file with
# `sha256sum -c` against corpus/manifest.sha256. CI runs it on both platforms, and caches <root> by
# the manifest's hash: what a cache gives is checked the same way. A file with another hash stops
# the script with a failure, whether it was just downloaded or came from the cache.
#
# usage: scripts/fetch-aces-corpus.sh <root>
#
# Only the release assets of the upstream ACES config repository, without a token.
#
# ACES_CORPUS_BASE, a URL or file:// directory, stands for the releases' base URL: it exists so
# that the hash check can be tried without the network.
set -euo pipefail

root="${1:?usage: fetch-aces-corpus.sh <root>}"
mkdir -p "$root"
root="$(cd "$root" && pwd)"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
base="${ACES_CORPUS_BASE:-https://github.com/AcademySoftwareFoundation/OpenColorIO-Config-ACES/releases/download}"

# The manifest's directory below aces/ is the release tag that holds the file: a pinned tag, and
# only these.
tag_of() {
    case "$1" in
        v1.0.0 | v2.0.0-v2.1.0 | v2.1.0-v2.2.0 | v3.0.0 | v4.0.0) printf '%s\n' "$1" ;;
        *)
            echo "fetch-aces-corpus: $1 is no pinned release tag" >&2
            return 1
            ;;
    esac
}

list="$(grep '  aces/' "$here/corpus/manifest.sha256")"
count="$(printf '%s\n' "$list" | wc -l)"
if [ "$count" -ne 37 ]; then
    echo "fetch-aces-corpus: the manifest lists $count aces files, not 37" >&2
    exit 1
fi

fetched=0
while read -r _ path; do
    directory="${path#aces/}"
    name="${directory#*/}"
    directory="${directory%%/*}"
    tag="$(tag_of "$directory")"
    if [ ! -f "$root/$path" ]; then
        mkdir -p "$root/aces/$directory"
        curl --fail --silent --show-error --location --retry 3 \
            --output "$root/$path" "$base/$tag/$name"
        fetched=$((fetched + 1))
    fi
done <<<"$list"

printf '%s\n' "$list" | (cd "$root" && sha256sum --check --strict --quiet -)
echo "fetch-aces-corpus: $count files match the manifest ($fetched fetched)"
