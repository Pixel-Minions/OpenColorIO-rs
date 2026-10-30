# P35: legacy large and boundary lattice data (WP7.1c)

Own this card and the existing private legacy comparison target. Existing upstream
NoOps-derived chains and native-wheel command supply every input/output. No oracle,
public helper, dependency, fixture, arithmetic or waiver change.

The upstream constructor order (`NoOps.cpp:175-206` at v2.5.2) validates Lut3DOpData
before allocating the RGBA lattice. Check edges 0/1/130/1626/u32::MAX for all four chains,
seven flags and ten languages (1,400 requests), including analytic-only bypass and I-503.
Compare the wheel's actual outcomes, never a synthesized boundary result.

At DEFAULT, compare all four chains/languages at edges 32/64/129 in full/exhaustive tier
(120 requests); quick tier checks edge 32 (40 requests), following the existing CPU-coverage
tier policy. Require all four native profiles at full tier and CPU-profile evidence at quick
tier. Keep all earlier inputs and exact comparisons. Counts and existing filters stay unchanged.

Large results are live, uncached and batched one at a time. Exact whole-outcome equality
remains; bounded failure diagnostics identify the first differing JSON path and print the
complete input request, avoiding a multi-million-value log dump.

Require focused Windows/Rocky debug/release proof, normal final-head CI and CPU evidence
before landing. Post-extraction dynamic updates and reverse legacy-to-ordinary request
order remain. Do not mark the full legacy checklist complete.

All 59 fixed-function cases remain unmerged. Do not investigate the five held GPU failures
or touch the protected evidence or external debugger's worktree/branch. Do not loosen
thresholds, add waivers or skip any held test. Frozen PR85/86 heads remain unchanged.
