<!-- One PR per card. It holds the card's chunk commits and lands with a merge commit, never
     squashed, so each chunk stays reviewable on its own (PLAN.md §7). -->

## Card

<!-- The card (docs/cards/...) or fix this PR implements. -->

## Chunks

<!-- One line per commit, in order: the upstream files and line ranges it ports, and its tests
     (they appear in `cargo xtask upstream-tests`). -->

## Evidence

- [ ] Every chunk passed `cargo xtask gate` (`CLAUDE.md` → "Chunks").
- [ ] Numeric, formatting and parsing chunks passed `cargo xtask gate --release --rocky`: tests in release too, on Windows and in Rocky Linux 9.
- [ ] Oracle checks are exact.
- [ ] Independent verifier pass.
- [ ] No new waivers, `#[ignore]`, tolerances or `unsafe` outside the allowed modules.
- [ ] Upstream bugs and limitations the card copies are listed in `docs/improvements.md`.
- [ ] `upstream-map.toml` statuses updated. `docs/parity.md` and `docs/ratchet.toml` untouched: `cargo xtask land` regenerates them.

<!-- CI runs `cargo xtask ci` in branch mode, against the PR base's `docs/ratchet.toml`; on a
     land commit (the head `cargo xtask land` pushes) it runs `cargo xtask ci --main`, which also
     requires both generated files to be current. -->

## Needs the owner

<!-- Tick what applies, and add its label plus `needs-owner`. Delete this section if none. -->

- [ ] Oracle or fixture change (`oracle`)
- [ ] Waiver (`waiver`)
- [ ] Deviation (`deviation`)
- [ ] Public API shape (`api`)
- [ ] New dependency (`dependency`)
