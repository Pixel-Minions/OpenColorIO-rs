# Release checklist

The steps to `v1.0.0` (`1.0.0+ocio.2.5.2`), in order. PLAN.md section 3 is what must hold; this
list only fixes the order.

**The SDE rule (owner decision 2026-11-01, G4).** One full SDE run on the release commit, then a
rerun of only the affected CPU profiles if needed.

`scripts/sde_release_ready.py` passing doesn't mean the run may start. It proves that
`upstream-map.toml` is closed, and the tree still changes after that. Don't dispatch the full
matrix until step 3 is done.

1. **Close the work.** `upstream-map.toml` is closed (`scripts/sde_release_ready.py` passes), and
   every release item the owner hasn't moved past 1.0.0 has landed, packaging and notices included.
2. **Close the decisions.** The owner has decided the distribution name and hosting (PLAN.md
   D14) and the final state of `docs/improvements.md`, and every exception has its row in
   `docs/deviations.md` or `waivers.toml`.
3. **Make the release commit.** Bump the version to `1.0.0+ocio.2.5.2`. This commit is the one to
   be tagged.
4. **Qualify it.** Dispatch the one full SDE run on that commit.
5. **If something fails or changes.** A fix makes a new release commit. Rerun only the CPU profiles
   it affects, and record which commit each profile passed on.
6. **Tag.** With section 3 holding on the release commit, tag `v1.0.0`. The release note starts
   with the OCIO version it matches, and states:
   - Python 3.13 only (D14);
   - the four tools shipped (PLAN.md section 4);
   - the accepted speed misses (`docs/cards/phase8.md`);
   - fuzzing comes after 1.0.0 (PLAN.md section 8).
