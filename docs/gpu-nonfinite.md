# GPU non-finite comparison class (W0013)

The owner approved this class on2026-10-29 after reviewing the independent
llvmpipe probes and the full-mask/three-check comparison. It covers CPU/GPU
cross-comparison results for non-finite image data on the validated renderer.
Every port/official GPU output bit remains exact, including NaN payloads.

## Acceptance and controls

The original input, GL_LINEAR sampling, transform, shader, setters, retests and
upstream tolerance remain unchanged. The complete original CPU/GPU scan still
runs and its maximum error, special-value indices and class exceptions are
recorded. No test-specific tuple lists or channel dataflow masks are added.

Three per-test checks are mandatory:

1. Port and actual official-wheel shader/resource/uniform/cache metadata and
   all GPU output bits match over the whole image.
2. Original upstream CPU/GPU rules hold on every sample whose entire fixed
   image-filter footprint is finite. Zero-weight neighboring texels count.
3. A complete uncached live-wheel companion passes with only non-finite image
   inputs replaced by zero, using the same processor and GL state. Its full
   GPU image is again exact against the wheel shader, with no CPU/GPU exception.

The fixed sampling check operates on pixels, not transform-specific channel
dependencies. A non-finite source can affect an adjacent otherwise finite
pixel. Finite-only input failures and errors outside these sampling footprints
are never excepted. Unsupported language, renderer, dimensions, viewport,
image binding or filter/wrap state fail closed. Non-finite shader parameters
and resources are outside this class.

Independent platform controls are a prerequisite. A standalone C++ program
uses EGL/GL directly, links no OCIO/Rust implementation and records260 native
observations. Rust compares complete readbacks against its actual native
results on all five upstream input profiles. The program is compiled at O0
for Rust debug and O2 for Rust release. Its input digests, exact uploads,
renderer identity, GL errors and finite controls are validated. A regression
deliberately simulating shared NaN/Inf-pixel sanitization and a NaN-payload
change must be rejected by the native/Rust comparison.

Policy controls reject finite-region errors, missing/corrupt/incomplete finite
companions, original or companion GPU bit changes, wrong renderer/language,
missing native proof, and changed shader metadata. Live-wheel rejection tests
exercise actual harness runs. The initial transition also required every old Matrix rule.

## Evidence and preservation

The independent root-cause CI run
[40892696031](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/40892696031)
completed520 observations. All519 then-recorded mismatches had non-finite
sampling dependencies. This supports the class; it does not count unexecuted
GPU tests as ported or replace validation of this implementation.

Before implementation,3964 files (572718794bytes) were moved out of target
and every file's SHA256 was verified. The owner-kept location is
`kept/gpu-nonfinite/2026-10-29/` in the primary checkout. It contains the P18/P19
raw observations, CI artifacts, source reviews and pre-implementation snapshots,
plus a preservation manifest and the owner's four explicit conditions.
Raw pixel observations remain uncommitted evidence, not expected fixtures.

New native and class evidence is written outside target at
`kept/gpu-validation/` in CI. Linux shard jobs preserve it as artifacts even
on failure. The regular CI workflow remains intact; the separate diagnostic
ci.yml branch is never a merge source.

## Validation and retirement

[CI40926626678](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/40926626678)
passed debug and release on source d423697b720179ae14e3cc691d429db4ba2aec6d.
Each profile passed all69 upstream cases, the native/Rust readback regression,
and the live and policy rejection controls. All47 non-finite cases passed their
complete finite companions; the519 excepted components across46 cases match
the earlier root-cause audit. Seven independent native processes per profile
completed260 observations each and compared35 complete readbacks bit for bit.
The native regression rejects shared non-finite-pixel sanitization and changed
NaN payloads. See [verified metadata](gpu-nonfinite-validation.json).

W0003-W0012 remained registered and mandatory throughout both passing profiles.
The transition control rejected a changed old Matrix tuple even when it would
satisfy the new class alone. Only after both results and artifacts were verified
were these ten entries retired. Their complete historical metadata is preserved
in [the archive](archive/gpu-matrix-waivers-2026-10-27.toml); the active parser
rejects per-test renderer entries. Runtime acceptance uses only W0013, with all
new native, finite-footprint, companion and rejection checks retained.

After retirement, [CI40929446610](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/40929446610)
passed both profiles again on source6c886b54. All69 cases,47 complete finite
companions,519 classified components/46 cases, native readbacks and rejection
controls passed unchanged. Both jobs independently generated identical parity
and ratchet files, and passed the main-mode bookkeeping check. These results
are also recorded in the verified metadata linked above.

The final merge candidate must pass the normal reference-platform CI workflow.
The separate validation/diagnostic ci.yml branches are never merged. Main's GPU
count changes only when the validated chunk is merged.
