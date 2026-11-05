# Release checklist

The steps to `v1.0.0` (`1.0.0+ocio.2.5.2`), in order. PLAN.md section 3 is what must hold; this
list only fixes the order.

**The SDE rule (owner decision 2026-11-01, G4).** One full SDE run on the release commit, then a
rerun of only the affected CPU profiles if needed.

`scripts/sde_release_ready.py` passing doesn't mean the run may start. It proves that
`upstream-map.toml` is closed, and the tree still changes after that. Don't dispatch the full
matrix until step 3 is done.

1. **Close the work.** `upstream-map.toml` is closed (`scripts/sde_release_ready.py` passes), and
   every release item the owner hasn't moved past 1.0.0 has landed, packaging included. The
   packages carry the license files of the crates in `THIRD-PARTY-NOTICES.md`, and
   `cargo xtask wheel-check` holds the wheel to that.
2. **Close the decisions.** The owner has decided the final state of `docs/improvements.md`
   (2026-11-03: every entry stays as it is for 1.0.0), and every exception has its row in
   `docs/deviations.md` or `waivers.toml`. The name and hosting are decided (PLAN.md D6 and D14).
3. **Make the release commit.** Bump the version to `1.0.0+ocio.2.5.2`, the four crates'
   `version = "=..."` requirements in `[workspace.dependencies]` to `=1.0.0` with it, and the
   wheel's to `1.0.0` in `py-wheel/pyproject.toml`: a test of xtask holds the two together. This
   commit is the one to be tagged. Check the packages on it:
   `cargo package --offline -p opencolorio-rs-ops -p opencolorio-rs-formats -p opencolorio-rs-gpu -p opencolorio-rs`,
   on Windows and in Rocky Linux 9. Not `--workspace`: that also packages the `publish = false`
   crates, and fails on them.
4. **Qualify it.** Dispatch the one full SDE run on that commit.
5. **If something fails or changes.** A fix makes a new release commit. Rerun only the CPU profiles
   it affects, and record which commit each profile passed on.
6. **Tag.** With section 3 holding on the release commit, tag `v1.0.0`. The release note is the
   `1.0.0` section of `CHANGELOG.md`, which gets its date with the tag. It starts with the OCIO
   version it matches, and states:
   - Python 3.13 only (D14), on Windows x86-64 and on Linux x86-64 with glibc 2.34 or newer;
   - the four tools shipped (PLAN.md section 4);
   - the accepted speed misses (`docs/cards/phase8.md`);
   - fuzzing comes after 1.0.0 (PLAN.md section 8).
7. **Publish.** The owner publishes the four crates on crates.io (PLAN.md D6), from the tag, each
   after what it depends on: `opencolorio-rs-ops`, then `opencolorio-rs-formats` and
   `opencolorio-rs-gpu`, then `opencolorio-rs`. A published version can't be replaced.
8. **Upload the wheels.** After the crates, the owner uploads the two wheels to PyPI as
   `opencolorio-rs`: Windows `cp313-cp313-win_amd64` and Linux
   `cp313-cp313-manylinux_2_34_x86_64`, both from the artifacts of the tagged commit's CI run
   (see "The wheel" below). The artifacts expire 30 days after the run. The first upload needs
   an account-wide PyPI token: a token scoped to the project can only be made once the project
   exists.

## The wheel

`cargo xtask wheel-build` builds it into `<target>/wheel/`, and `cargo xtask wheel-check` builds
it and checks it installed. CI runs the check on both platforms and keeps each wheel as the run's
artifact, for 30 days. Nothing in this repository publishes it: no workflow has a publish step or
a token, and a test holds that.

- **Version.** PyPI refuses a local segment, so the wheel's version is the workspace's without
  `+ocio.2.5.2`: `1.0.0` for `1.0.0+ocio.2.5.2` (owner decision 2026-11-02). `Summary` says which
  OCIO release it matches, and `PyOpenColorIO.__version__` stays that release's, `2.5.2`.
- **Windows.** `cp313-cp313-win_amd64`.
- **Linux.** `cp313-cp313-manylinux_2_34_x86_64`, built in the Rocky Linux 9 image. The tag comes
  from all the wheel's binaries, the tools included (`xtask/src/wheel_audit.rs`): glibc 2.34 is the
  image's, and they need its `GLIBC_2.34` symbols. Nothing is bundled: they need only `libc`, `libm`,
  `libgcc_s` and the dynamic loader, and the wheel has no C or C++ code, so no `libstdc++`. The
  official wheels are `manylinux_2_17` and `manylinux_2_28`; this one doesn't install on systems
  older than glibc 2.34, such as Rocky Linux 8.
- **Contents.** The package as `cargo xtask py-build` lays it out, with the extension, upstream's
  `__init__.py` and `command_line.py`, and the four tools in `PyOpenColorIO/bin`; the four console
  scripts; the license files of the project and of the 21 crates it links.
- **Build requirement.** maturin at the pin of `py-wheel/pyproject.toml`, with the hashes of
  `py-wheel/build-constraints.txt`. When the pin moves, regenerate those with the command in that
  file.
