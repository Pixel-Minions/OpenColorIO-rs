# P30: complete CPU manifests in fresh SDE processes

Prerequisite P29 preserves all 8,232 original whole-config cases in four execution parts,
with exact comparisons and unchanged coverage bounds. All four native profiles and both
Windows-HSW/Rocky-SNB trials passed; compiled missing/duplicate-input controls were rejected
and source restored. Evidence: `kept/continuation/2026-10-30/p29-verified-summary.json`.

The production runner in `scripts/sde_cpu_tests.py` must preserve Cargo's original alias,
target selection, test filters, package working directory and runtime environment. Native
Cargo build artifacts define the complete executable manifest; native libtest lists define
each selected name. An actual Cargo-runner preflight must pass before the full suite starts.
Every selected name then runs exactly once in a fresh emulator and must report one pass,
zero failures and zero ignored tests. Missing/duplicate target reports or missing test results
fail the suite. Emulator errors stop only their owned tree and stay red. Other selected tests
continue under the existing no-fail-fast command.

`scripts/sde.sh`, its pinned runtime and CPU flags are unchanged. Only kernel-job invocation
and manifest artifacts change in `.github/workflows/sde.yml`. The existing two long LUT
sweeps remain in their separate jobs; their original exclusions from kernel jobs are passed
unchanged, recorded in each native manifest and introduce no new exclusion.

Require hosted two-platform runner smoke checks, then complete kernel manifests and exact
results on the emulated profiles. Normal CI must pass on the final landing. Do not claim all
ten kernel profiles complete from a two-profile trial. No GPU tests, waiver, threshold,
library API, arithmetic, oracle command or dependency changes belong to this chunk.

## Review and validation evidence

`p29-verified-summary.json` independently checks the exact 8,232-case plan, all four native
profiles and both emulated module profiles, original comparison/coverage preservation and
compiled omission/duplication witnesses. The generated files agree across all four profiles;
C++ remains 1,188/1,191 on P28, GPU 205/264 and Python 384/384.

The initial real Cargo-runner smoke and all four protocol controls passed on Windows and
Rocky (41089164457). Forty selected executables were inventoried. Rocky's artifact export
then failed because newly created directories were private to the container root. Source
`ff1b4360` only makes each new Unix report directory host-readable; the numerical/test
execution paths are unchanged. Exact-source export validation and the complete CPU-profile
workflow are retained as separate evidence in `kept/continuation/2026-10-30/`.

The final landing requires its normal checks and actual complete kernel manifests. Neither
the one-audit smoke nor the two-profile merge-module proof is a full CPU-coverage result.
The final PR reports the exact CI heads and results; diagnostic `ci.yml` files stay outside
the landing. No held fixed-function GPU case is included.
