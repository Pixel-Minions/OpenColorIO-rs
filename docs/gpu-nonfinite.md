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
exercise actual harness runs, including retention of the old Matrix policy.

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

## Transition

W0003-W0012 remain registered and mandatory in the initial implementation.
The new class and finite companion must pass too, so a failed new safeguard
cannot fall back to the old waiver. A changed old Matrix tuple cannot fall
through to the new class. Only after debug and release CI pass will a separate
change retire those ten active entries. The resulting final merge candidate
then runs the normal required CI checks again.

Validation of the implementation is pending CI. No pass count in main changes
until the validated chunk is merged.
