<!-- One PR per card. It holds the card's chunk commits and lands with a merge commit, never
     squashed, so each chunk stays reviewable on its own (PLAN.md §7). -->

## Card

<!-- The card (docs/cards/...) or fix this PR implements. -->

## Chunks

<!-- One line per commit, in order: the upstream files and line ranges it ports, and its tests
     (they appear in `cargo xtask upstream-tests`). -->

## Evidence

- [ ] Every chunk passed the chunk gate on Windows (`CLAUDE.md` → "Chunks").
- [ ] Numeric chunks: tests in release too, on Windows and in Rocky Linux 9 (`scripts/rocky9.sh`).
- [ ] Formatting and parsing chunks: Rocky Linux 9.
- [ ] Oracle checks are exact.
- [ ] Independent verifier pass.
- [ ] No new waivers, `#[ignore]`, tolerances or `unsafe` outside the allowed modules.
- [ ] `upstream-map.toml` statuses updated.

## Needs the owner

<!-- Tick what applies, and add its label plus `needs-owner`. Delete this section if none. -->

- [ ] Oracle or fixture change (`oracle`)
- [ ] Waiver (`waiver`)
- [ ] Deviation (`deviation`)
- [ ] Public API shape (`api`)
- [ ] New dependency (`dependency`)
