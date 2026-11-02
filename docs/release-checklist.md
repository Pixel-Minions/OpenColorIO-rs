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
   packages carry the license files of the crates in `THIRD-PARTY-NOTICES.md`.
2. **Close the decisions.** The owner has decided the Python wheel's hosting (PLAN.md D14; the
   distribution name is `opencolorio-rs`, and the Rust crates go to crates.io under it, D6) and
   the final state of `docs/improvements.md`, and every exception has its row in
   `docs/deviations.md` or `waivers.toml`.
3. **Make the release commit.** Bump the version to `1.0.0+ocio.2.5.2`, and the four crates'
   `version = "=..."` requirements in `[workspace.dependencies]` to `=1.0.0` with it. This commit
   is the one to be tagged. Check the packages on it:
   `cargo package --offline -p opencolorio-rs-ops -p opencolorio-rs-formats -p opencolorio-rs-gpu -p opencolorio-rs`,
   on Windows and in Rocky Linux 9. Not `--workspace`: that also packages the `publish = false`
   crates, and fails on them.
4. **Qualify it.** Dispatch the one full SDE run on that commit.
5. **If something fails or changes.** A fix makes a new release commit. Rerun only the CPU profiles
   it affects, and record which commit each profile passed on.
6. **Tag.** With section 3 holding on the release commit, tag `v1.0.0`. The release note starts
   with the OCIO version it matches, and states:
   - Python 3.13 only (D14);
   - the four tools shipped (PLAN.md section 4);
   - the accepted speed misses (`docs/cards/phase8.md`);
   - fuzzing comes after 1.0.0 (PLAN.md section 8).
7. **Publish.** The owner publishes the four crates on crates.io (PLAN.md D6), from the tag, each
   after what it depends on: `opencolorio-rs-ops`, then `opencolorio-rs-formats` and
   `opencolorio-rs-gpu`, then `opencolorio-rs`. A published version can't be replaced.
