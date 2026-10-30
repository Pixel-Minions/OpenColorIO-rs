# P32: small legacy lattices and cache order (WP7.1c, first chunk)

Use the existing O7.1 `legacy_gpu_shader` native client, linked to the official wheel. No
oracle command, helper visibility, library API, dependency or arithmetic change is needed.

The initial sweep uses upstream NoOps tests' diagonal scale and 256-entry squaring LUT in
four chains: analytic only, LUT only, analytic/LUT/analytic, and two separated LUTs. Compare
both small edge lengths (2 and 3), all seven named optimization levels, all ten shader
languages, and no/ordinary/legacy cache priming. The legacy-before request uses edge 2,
so a later edge 3 checks the original flags-only cache behavior (I-501).

Compare shader text, processor/GPU/shader cache IDs, queries, extraction refusals, complete
logs, all texture metadata and every 1D/3D texture value bit. Uniform and dynamic-property
values cannot silently go unmeasured: these non-dynamic chains require the wheel to report
none. Normalization helpers are copied from the existing API shader sweep.

`legacy_gpu_oracle` belongs in the CPU-dependent alias because legacy LUT baking depends on
the selected CPU renderer. The complete-manifest runner discovers the additional target;
no existing target or filter is removed. This adds no upstream C++/GPU marker or waiver.

Require four Windows/Rocky debug/release focused profiles, then normal final-head checks.
Broader WP7.1c coverage remains: larger/boundary edges, allocation cuts, FileNoOp, dynamic
ops inside/outside the lattice and the remaining request-order variations. Do not describe
this first chunk as the whole legacy parity sweep or as GPU pixel execution coverage.

No FixedFunctionTransform, ACES2 failure investigation, GPU waiver, threshold or held test
belongs to this chunk. The separate debugger's worktree/branch and protected evidence stay
untouched; all 59 fixed-function GPU cases stay unmerged.

## Focused evidence

Source `14b03497` passed Windows/Rocky debug/release in CI 41100526872. Each profile compared
1,680 requests: 1,554 shader/texture outcomes and 126 matching reference refusals, with zero
ignored tests. Exact source identities, guardrails, target clippy, generated main-mode checks
and identical generated files were independently verified in
`docs/legacy-gpu-small-validation.json`. No upstream marker or count is added.

The final landing still requires normal CI and CPU-profile validation of the added target.
This first chunk does not close the broader WP7.1c checklist or claim new pixel executions.
