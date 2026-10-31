# P33: legacy allocation cuts and file boundaries (WP7.1c)

Extend the P32 test target using the existing native-wheel command; no oracle, public API,
dependency, arithmetic, fixture or waiver change. Own only this card and the private test
helpers in `crates/ocio/tests/legacy_gpu_oracle.rs`. Do not change either frozen PR head.

Use the allocation inputs/chains in `tests/cpu/ops/noop/NoOps_tests.cpp:17-23,160-234`
and the file-transform lattice boundary in `src/OpenColorIO/ops/noop/NoOps.cpp:266-269`
at v2.5.2. Cover explicit lg2 allocations around/between LUTs, default uniform/lg2
allocations, and upstream's `lut1d_1.spi1d` with/without surrounding analytic ops.

The seven chains cross both small edges, all seven named optimization levels and all ten
shader languages (980 requests). Reuse P32's exact cache/shader/log/texture/refusal comparison,
including every texture value bit and the assertion that no dynamic value is unmeasured.
Keep every existing P32 request; no upstream count marker is added.

Require four focused Windows/Rocky debug/release profiles and generated-file agreement.
Normal final-head and complete CPU-profile validation are required for the eventual landing.
The larger/boundary lattice and dynamic legacy cases remain separate work.

All 59 fixed-function cases remain unmerged. Do not investigate the held ACES2 tests or
GamutComp13, change GPU thresholds/waivers/skips, or access the separate debugger's worktree
or branch. Preserve the protected 2026-10-29 evidence without writes, moves or deletion.
