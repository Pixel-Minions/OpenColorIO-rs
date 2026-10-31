# P29: preserve SDE coverage within the runtime thread budget

The unchanged synthetic whole-config merge sweep exhausts pinned SDE's cumulative thread
budget even when it is the only test in a fresh emulator. Minimal standalone controls and
the red Windows/Rocky reproduction are retained in `kept/continuation/2026-10-30/`.

This chunk changes test execution, with no production parser, arithmetic, API, oracle command,
dependency, waiver or GPU change. Keep all 59 fixed-function GPU cases unmerged.

## Files and work

- `section_merger_oracle_tests.rs`: four generated execution parts of the unchanged whole-case
  plan. Each original port merge and its exact serialization, load-log, merge-log and exception
  comparison runs once. A generated audit checks exhaustive, disjoint assignment; every part
  must be nonempty and check its complete assigned set.
- Each part still requests the complete live reference matrix. All original matrix coverage
  bounds run unchanged on that CPU. Reference outcome counts/sets equal the port's when every
  part's exact comparisons pass. Serialize these large native test transcripts with a private
  lock to bound peak memory. Do not lower coverage minima to fit individual parts.
- `scripts/diagnostics/p29_partition_controls.py`: compiled missing/duplicate-case mutants must
  fail the plan audit; restore source byte for byte and pass the original audit afterward.
- `scripts/diagnostics/p29_sde_parts.py`: bounded fresh-emulator trial for all four parts, the
  audit and the pre-existing general/roles test, on Windows HSW and Rocky SNB. Report every
  actual test result and case count. This is not full production CPU coverage.

## Validation and completion

The whole-case generator, reference requests, actual port setup and original global coverage
assertions are statically compared with the pre-change source; original exact comparisons
remain. No upstream marker is added, so C++ coverage remains 1,188 on prerequisite P28.

Require focused guards/clippy/main-mode generation and all six module tests in Windows/Rocky
debug/release, including both plan rejection controls in debug. Require all selected tests and
the full original whole-case count to pass the two SDE trials.

Then implement a separate coverage-preserving runner for the complete production `cpu-tests`
alias, with native test manifests and fresh emulation processes. Preserve every selected
target/name and all ten CPU/platform combinations. Its normal CI and actual SDE validation
must pass before calling the production issue fixed. Never merge a diagnostic `ci.yml`.
