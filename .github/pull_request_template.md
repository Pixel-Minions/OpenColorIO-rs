## Card

<!-- The porting card (docs/cards/...) or WP this PR implements. -->

## Upstream

<!-- Upstream files ported, at v2.5.2, and the upstream tests ported (they appear in `cargo xtask upstream-tests`). -->

## Evidence

- [ ] `cargo xtask ci` passes (guards, fixtures, ratchet, parity).
- [ ] Oracle checks are exact and pass on Windows and in Rocky Linux 9 (`scripts/rocky9.sh cargo test --workspace`).
- [ ] No new waivers, `#[ignore]`, tolerances or `unsafe` outside the allowed modules.
- [ ] `upstream-map.toml` statuses updated.

## Needs the owner

<!-- Public API shape, waivers, deviations, n/a entries. Delete if none. -->
