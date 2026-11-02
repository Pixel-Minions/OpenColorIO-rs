# Corpora

Real-world configs checked against the oracle (PLAN.md §8), by
`crates/ocio/tests/corpus_oracle.rs`. The configs themselves are never committed:

| Corpus | Source | Licence | In the repository |
|---|---|---|---|
| `aces` | OpenColorIO-Config-ACES releases v1.0.0, v2.0.0-v2.1.0, v2.1.0-v2.2.0, v3.0.0, v4.0.0: the release assets | BSD-3-Clause | hashes only |
| `imageworks` | imageworks/OpenColorIO-Configs at `fe8ac88675fab5fcfa08ce807fd07f4cbe18674f`: spi-vfx, spi-anim, nuke-default, aces_0.1.1, aces_0.7.1, aces_1.0.1-1.0.3, with their LUTs (3.1 GB, `.git` left out) | none found | hashes only; never redistribute |
| upstream's | `tests/data/files/configs`, through the submodule | BSD-3-Clause | run on every test run |
| studio configs | the owner's, local only | private | nothing, ever |

`manifest.sha256` lists each public input with its SHA-256, in `sha256sum`'s text form, relative
to the corpus root: `sha256sum -c corpus/manifest.sha256` in that root checks them.

## In CI

`.github/workflows/ci.yml` runs the `aces` corpus on both platforms, in the required checks jobs:
`scripts/fetch-aces-corpus.sh` fetches its 37 files from their pinned release tags, without a token,
into a directory `actions/cache` keeps by the hash of `manifest.sha256`, and checks every file with
`sha256sum -c` against the manifest, on a cache hit too, so a file with another hash fails the step.
`corpus_oracle` then runs with `OCIO_RS_CORPUS=aces`. Each directory of `aces/` in the manifest is the
tag of the release that holds its files. That is the only corpus in CI: `xtask`'s `corpus_ci` test
pins it. `imageworks` has no licence and studio configs never leave this machine.

## Running

```sh
# The public corpora. A requested corpus that is absent, or not byte for byte the manifest's,
# fails the test.
OCIO_RS_CORPUS=aces,imageworks OCIO_RS_CORPUS_ROOT=/path/to/corpus-data \
  cargo test -p opencolorio-rs --test corpus_oracle public_corpora -- --nocapture

# The same in Rocky Linux 9: the script mounts the root read-only at /corpus.
OCIO_RS_CORPUS=aces OCIO_RS_CORPUS_ROOT=/path/to/corpus-data \
  scripts/rocky9.sh cargo test -p opencolorio-rs --test corpus_oracle public_corpora -- --nocapture

# Studio configs, on this machine only: directories separated as in PATH.
OCIO_RS_STUDIO_CORPUS='/path/one;/path/two' \
  cargo test -p opencolorio-rs --test corpus_oracle studio_configs > target/corpus-studio.log 2>&1
```

Without `OCIO_RS_CORPUS`, the public test does nothing and upstream's configs still run. The
reports go to `<target>/corpus-report/<corpus>.txt` (counts per config) and `.jsonl` (one line per
mismatch). The studio report holds anonymous counts and kinds, keyed by a hash of the path; never
put a name, a path or a message from those configs anywhere that could leave the machine.

`OCIO_RS_CORPUS_JOBS` (configs at a time, default 4), `OCIO_RS_CORPUS_ONLY` (configs whose id holds
this text), `OCIO_RS_CORPUS_CACHE=1` (keep the oracle's answers: gigabytes for `imageworks`) and
`OCIO_RS_CORPUS_REPORT` (the report's name) help while working on a mismatch.
