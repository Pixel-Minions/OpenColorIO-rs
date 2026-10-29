# Waivers

The owner approves each exception to PLAN.md section3. waivers.toml is the
machine-readable authority. W0001 (Windows PQ exact-math bounds) and W0002
(NaN transform-parameter payload/sign and their baked-LUT scope) retain their
existing approval and exact text there; this proposal changes neither.

## P7-6 renderer exceptions — owner approved; runtime validation pending

The project owner explicitly approved W0003 through W0012 and the single
unpublished test-helper lookup on 2026-10-27. Each entry records approved=true,
approved_by="Project owner, explicit chat approval on 2026-10-27" and that date.
No test is skipped. Only source application has been authorized so far: the
enforcement implementation, policy units, mutations and full gates remain pending.
Approval record SHA256:
8d88137448154e0b7135a01fbccd93248d49a87ecfca4ef36d3e1aca98dfe59b.

All ten upstream MatrixOps cases failed the original CPU/GPU comparison while
the complete port and actual-wheel GPU buffers were bit-identical on one context,
including every exceptional component. Numeric maxima stayed within upstream's
absolute5e-7 threshold. The input is the original256x256 RGBA default image,
SHA2562df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af,
NaN/Inf/wide=true, range[-1,2], expected-minimal-value1e-6, absolute mode and GLSL1.2.
Eight cases use getOptimizedLegacyGPUProcessor(DEFAULT,32), two use the default
ordinary GPU processor. All10 are run0 with no retests.

Exact renderer scope: Linux, Mesa vendor, llvmpipe (LLVM21.1.8,256bits),
GL4.5 compatibility Mesa25.2.7, reported GLSL4.50, EGL Mesa Project1.5,
GL_MAX_TEXTURE_SIZE16384. The structured fields retain the driver's exact spaces
and punctuation and must all match. Evidence used image
sha256:63938a1f27ca6c803c5634a86c5c1735d8b11fe31ec21cfa3c17c39978e0efda.
A different renderer/version/settings/input gets only the original unwaived comparison.

Evidence: source tree78e09ba2fceb5e61ac2a76793d81fee9aeca4c86, print-only
diagnostic20261027T183146Z, container5ebbf74345a81943bfa58968782e1d1e4668b7d025033231fb25b3e64fab29e5,
all10 actual failures/zero ignored or filtered, no OOM. Full log SHA256
e84a1fdb0b458ca298e256a5812fca1495b0850e3e90262b10c460da970da295;
parsed actual evidence SHA256
03fc9e6ff5fb2cde3eeb7dba990d9b6a12bb08ec1118f06178a277fd4cf0697a.
Raw evidence is preserved under
target/p7c/target/c-matrix-p7-6-prep/run-20261027T183146Z/.
The root cause/sampling mechanism is UNPROVEN. The current CPU oracle uses
packed65536x1/separate source and destination buffers, while upstream applies
packed256x256 in place; this unchanged layout difference is not a demonstrated cause.

Each rule additionally pins the exact32hex processor_cache_id already captured
from its actual wheel CPU response. The private guard reads the existing actual
test processor's cache_id; an absent or different processor cannot use the rule.
These source fingerprints are observed wheel values, not computed port expectations.

Only the complete observed CPU-non-NaN to GPU-NaN tuples below are covered.
The original ImageComparison and compare_image remain unchanged. Full GPU bit
identity still checks262144 floats. The wrapper requires every tuple's input,
live CPU and live wheel/port GPU bits, exact complete mismatch set, and every
scope field to match. Its use leaves upstream's full-image finite/Inf results
and all other components checked. InfNone is the upstream aggregate diagnostic:
ValidateInf skips NaN operands, so it does not imply Inf-classification equality
at these NaN mismatch positions. No blanket NaN mask or threshold increase exists.
Additional/missing sets fail when a waiver is needed. A fully passing original
upstream comparison legitimately passes without using any waiver.

The tables are measured scope boundaries, not replacement pixel images. Every
CPU and both GPU buffers are obtained live, with oracle response caching disabled.
The port's GPU output is never used as an expected pixel value.
| ID | Upstream test | NaN mismatches | Maximum numeric error |
|---|---|---:|---:|
| W0003 | MatrixOps/matrix | 32 | 4.7683716e-7 |
| W0004 | MatrixOps/scale | 19 | 0.0 |
| W0005 | MatrixOps/offset | 19 | 0.0 |
| W0006 | MatrixOps/matrix_offset | 32 | 4.7683716e-7 |
| W0007 | MatrixOps/matrix_inverse | 32 | 2.3841858e-7 |
| W0008 | MatrixOps/scale_inverse | 19 | 0.0 |
| W0009 | MatrixOps/offset_inverse | 19 | 0.0 |
| W0010 | MatrixOps/matrix_offset_inverse | 32 | 4.7683716e-7 |
| W0011 | MatrixOps/matrix_offset_generic_shader | 32 | 4.7683716e-7 |
| W0012 | MatrixOps/matrix_offset_inverse_generic_shader | 32 | 4.7683716e-7 |

The following W0003-W0012 sections are historical. Their active entries were
retired only after [CI40926626678](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/40926626678)
passed debug and release with every old Matrix rule still mandatory. The exact
metadata remains in [the archive](archive/gpu-matrix-waivers-2026-10-27.toml).
W0013 below is the single active GPU class; the parser rejects per-test entries.

## W0003 — MatrixOps/matrix (retired 2026-10-29; historical evidence)

Pinned actual wheel processor_cache_id: 58a1c3b1b486f83ee34b9faeb6bc1b01.

Complete mismatch indices: 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47.

Every row has CPU non-NaN and both GPUs NaN; exact bits differ by case and component.

| Component | Input f32 hex | Live wheel CPU | Port GPU | Wheel GPU |
|---:|---|---|---|---|
| 16 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 17 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 18 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 19 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 20 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 21 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 22 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 23 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 24 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 25 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 26 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 27 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 28 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 29 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 30 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 31 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 32 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 33 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 34 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 35 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 36 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 37 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 38 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 39 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 40 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 41 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 42 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 43 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 44 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 45 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 46 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 47 | ff800000 | ff800000 | ffc00000 | ffc00000 |

## W0004 — MatrixOps/scale (retired 2026-10-29; historical evidence)

Pinned actual wheel processor_cache_id: 1f17ec51c941868a9885448181bd403d.

Complete mismatch indices: 1, 6, 11, 12, 16, 17, 21, 22, 26, 27, 28, 31, 32, 33, 37, 38, 42, 43, 47.

Every row has CPU non-NaN and both GPUs NaN; exact bits differ by case and component.

| Component | Input f32 hex | Live wheel CPU | Port GPU | Wheel GPU |
|---:|---|---|---|---|
| 1 | 00000000 | 80000000 | 7fc00000 | 7fc00000 |
| 6 | 00000000 | 00000000 | 7fc00000 | 7fc00000 |
| 11 | 00000000 | 00000000 | 7fc00000 | 7fc00000 |
| 12 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 16 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 17 | 00000000 | 80000000 | ffc00000 | ffc00000 |
| 21 | 7f800000 | ff800000 | ffc00000 | ffc00000 |
| 22 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 26 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 27 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 28 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 31 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 32 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 33 | 00000000 | 80000000 | ffc00000 | ffc00000 |
| 37 | ff800000 | 7f800000 | ffc00000 | ffc00000 |
| 38 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 42 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 43 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 47 | ff800000 | ff800000 | ffc00000 | ffc00000 |

## W0005 — MatrixOps/offset (retired 2026-10-29; historical evidence)

Pinned actual wheel processor_cache_id: 56907b6a75e171e5bb461c775198f17c.

Complete mismatch indices: 1, 6, 11, 12, 16, 17, 21, 22, 26, 27, 28, 31, 32, 33, 37, 38, 42, 43, 47.

Every row has CPU non-NaN and both GPUs NaN; exact bits differ by case and component.

| Component | Input f32 hex | Live wheel CPU | Port GPU | Wheel GPU |
|---:|---|---|---|---|
| 1 | 00000000 | 3e800000 | 7fc00000 | 7fc00000 |
| 6 | 00000000 | be800000 | 7fc00000 | 7fc00000 |
| 11 | 00000000 | 00000000 | 7fc00000 | 7fc00000 |
| 12 | 00000000 | bf000000 | ffc00000 | ffc00000 |
| 16 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 17 | 00000000 | 3e800000 | ffc00000 | ffc00000 |
| 21 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 22 | 00000000 | be800000 | ffc00000 | ffc00000 |
| 26 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 27 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 28 | 00000000 | bf000000 | ffc00000 | ffc00000 |
| 31 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 32 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 33 | 00000000 | 3e800000 | ffc00000 | ffc00000 |
| 37 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 38 | 00000000 | be800000 | ffc00000 | ffc00000 |
| 42 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 43 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 47 | ff800000 | ff800000 | ffc00000 | ffc00000 |

## W0006 — MatrixOps/matrix_offset (retired 2026-10-29; historical evidence)

Pinned actual wheel processor_cache_id: 7948f03ec6b5d1e88a38712ad0827ae4.

Complete mismatch indices: 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47.

Every row has CPU non-NaN and both GPUs NaN; exact bits differ by case and component.

| Component | Input f32 hex | Live wheel CPU | Port GPU | Wheel GPU |
|---:|---|---|---|---|
| 16 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 17 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 18 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 19 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 20 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 21 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 22 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 23 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 24 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 25 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 26 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 27 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 28 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 29 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 30 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 31 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 32 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 33 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 34 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 35 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 36 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 37 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 38 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 39 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 40 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 41 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 42 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 43 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 44 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 45 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 46 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 47 | ff800000 | ff800000 | ffc00000 | ffc00000 |

## W0007 — MatrixOps/matrix_inverse (retired 2026-10-29; historical evidence)

Pinned actual wheel processor_cache_id: aeea5955c51458f7ac351ebaeac94c80.

Complete mismatch indices: 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47.

Every row has CPU non-NaN and both GPUs NaN; exact bits differ by case and component.

| Component | Input f32 hex | Live wheel CPU | Port GPU | Wheel GPU |
|---:|---|---|---|---|
| 16 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 17 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 18 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 19 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 20 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 21 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 22 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 23 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 24 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 25 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 26 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 27 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 28 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 29 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 30 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 31 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 32 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 33 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 34 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 35 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 36 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 37 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 38 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 39 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 40 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 41 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 42 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 43 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 44 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 45 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 46 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 47 | ff800000 | ff800000 | ffc00000 | ffc00000 |

## W0008 — MatrixOps/scale_inverse (retired 2026-10-29; historical evidence)

Pinned actual wheel processor_cache_id: d9ce8ca13cab6f4274af03b9cbe74de4.

Complete mismatch indices: 1, 6, 11, 12, 16, 17, 21, 22, 26, 27, 28, 31, 32, 33, 37, 38, 42, 43, 47.

Every row has CPU non-NaN and both GPUs NaN; exact bits differ by case and component.

| Component | Input f32 hex | Live wheel CPU | Port GPU | Wheel GPU |
|---:|---|---|---|---|
| 1 | 00000000 | 80000000 | 7fc00000 | 7fc00000 |
| 6 | 00000000 | 00000000 | 7fc00000 | 7fc00000 |
| 11 | 00000000 | 00000000 | 7fc00000 | 7fc00000 |
| 12 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 16 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 17 | 00000000 | 80000000 | ffc00000 | ffc00000 |
| 21 | 7f800000 | ff800000 | ffc00000 | ffc00000 |
| 22 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 26 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 27 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 28 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 31 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 32 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 33 | 00000000 | 80000000 | ffc00000 | ffc00000 |
| 37 | ff800000 | 7f800000 | ffc00000 | ffc00000 |
| 38 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 42 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 43 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 47 | ff800000 | ff800000 | ffc00000 | ffc00000 |

## W0009 — MatrixOps/offset_inverse (retired 2026-10-29; historical evidence)

Pinned actual wheel processor_cache_id: 7e20f38cfcd91faa67cbb355e5231951.

Complete mismatch indices: 1, 6, 11, 12, 16, 17, 21, 22, 26, 27, 28, 31, 32, 33, 37, 38, 42, 43, 47.

Every row has CPU non-NaN and both GPUs NaN; exact bits differ by case and component.

| Component | Input f32 hex | Live wheel CPU | Port GPU | Wheel GPU |
|---:|---|---|---|---|
| 1 | 00000000 | be800000 | 7fc00000 | 7fc00000 |
| 6 | 00000000 | 3e800000 | 7fc00000 | 7fc00000 |
| 11 | 00000000 | 00000000 | 7fc00000 | 7fc00000 |
| 12 | 00000000 | 3f000000 | ffc00000 | ffc00000 |
| 16 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 17 | 00000000 | be800000 | ffc00000 | ffc00000 |
| 21 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 22 | 00000000 | 3e800000 | ffc00000 | ffc00000 |
| 26 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 27 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 28 | 00000000 | 3f000000 | ffc00000 | ffc00000 |
| 31 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 32 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 33 | 00000000 | be800000 | ffc00000 | ffc00000 |
| 37 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 38 | 00000000 | 3e800000 | ffc00000 | ffc00000 |
| 42 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 43 | 00000000 | 00000000 | ffc00000 | ffc00000 |
| 47 | ff800000 | ff800000 | ffc00000 | ffc00000 |

## W0010 — MatrixOps/matrix_offset_inverse (retired 2026-10-29; historical evidence)

Pinned actual wheel processor_cache_id: be70f717ca8c7c4ba55d6fd4399ec915.

Complete mismatch indices: 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47.

Every row has CPU non-NaN and both GPUs NaN; exact bits differ by case and component.

| Component | Input f32 hex | Live wheel CPU | Port GPU | Wheel GPU |
|---:|---|---|---|---|
| 16 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 17 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 18 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 19 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 20 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 21 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 22 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 23 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 24 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 25 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 26 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 27 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 28 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 29 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 30 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 31 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 32 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 33 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 34 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 35 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 36 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 37 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 38 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 39 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 40 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 41 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 42 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 43 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 44 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 45 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 46 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 47 | ff800000 | ff800000 | ffc00000 | ffc00000 |

## W0011 — MatrixOps/matrix_offset_generic_shader (retired 2026-10-29; historical evidence)

Pinned actual wheel processor_cache_id: caaa4e41060dacb9fb48acd975c581c6.

Complete mismatch indices: 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47.

Every row has CPU non-NaN and both GPUs NaN; exact bits differ by case and component.

| Component | Input f32 hex | Live wheel CPU | Port GPU | Wheel GPU |
|---:|---|---|---|---|
| 16 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 17 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 18 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 19 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 20 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 21 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 22 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 23 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 24 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 25 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 26 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 27 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 28 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 29 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 30 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 31 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 32 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 33 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 34 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 35 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 36 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 37 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 38 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 39 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 40 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 41 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 42 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 43 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 44 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 45 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 46 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 47 | ff800000 | ff800000 | ffc00000 | ffc00000 |

## W0012 — MatrixOps/matrix_offset_inverse_generic_shader (retired 2026-10-29; historical evidence)

Pinned actual wheel processor_cache_id: be70f717ca8c7c4ba55d6fd4399ec915.

Complete mismatch indices: 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47.

Every row has CPU non-NaN and both GPUs NaN; exact bits differ by case and component.

| Component | Input f32 hex | Live wheel CPU | Port GPU | Wheel GPU |
|---:|---|---|---|---|
| 16 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 17 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 18 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 19 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 20 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 21 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 22 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 23 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 24 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 25 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 26 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 27 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 28 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 29 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 30 | 00000000 | ff800000 | ffc00000 | ffc00000 |
| 31 | 7f800000 | 7f800000 | ffc00000 | ffc00000 |
| 32 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 33 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 34 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 35 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 36 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 37 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 38 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 39 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 40 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 41 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 42 | ff800000 | ff800000 | ffc00000 | ffc00000 |
| 43 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 44 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 45 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 46 | 00000000 | 7f800000 | ffc00000 | ffc00000 |
| 47 | ff800000 | ff800000 | ffc00000 | ffc00000 |

## W0013 - One GPU non-finite class (owner approved 2026-10-29; debug/release validated)

The owner approved exact whole-image GPU identity, original upstream checks on
finite GL image-filter footprints, a complete finite-input companion, and
independent native/Rust platform controls. The classifier, native readback
regression and rejection controls are in one chunk. No transform/channel mask
or per-test tuple list is added. See `docs/gpu-nonfinite.md` for scope, proof,
controls and the evidence location outside target.

Both profiles passed all69 cases, full finite companions, independent readback
regressions and rejection controls in CI40926626678 before W0003-W0012 were
retired. All519 original exceptions across46 cases match the reviewed audit.
The old entries are archived and cannot grant active exceptions; all new class
checks remain enforced. Normal CI must still pass on the final merge candidate.
