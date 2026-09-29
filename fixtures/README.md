# Fixtures

Reference outputs from the official OpenColorIO 2.5.2 wheel (`oracle/`), committed because
they are identical on every platform: config YAML, CLF/CTF, baked LUTs, shader text, cache
IDs, error text. Pixel checks are never committed; they run live against the oracle on each
machine (PLAN.md §8).

- Only `cargo xtask oracle regen <group>` writes here, and it updates `MANIFEST.toml`.
- `cargo xtask fixtures verify` (in CI) checks every file against its hash.
- `cargo xtask oracle check <group>` regenerates a group on another platform and proves the
  committed bytes are platform-independent.
- The owner reviews every change here (CODEOWNERS).
