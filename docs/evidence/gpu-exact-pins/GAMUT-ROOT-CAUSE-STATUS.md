# Finite GPU failure investigation — 2026-10-29

PR77 is merged at05c59ac4 after all17 normal checks passed. Its approved W0013
rule is unchanged: finite sampling footprints retain upstream CPU/GPU thresholds.
PR78 and PR79 separately validate156 and205 GPU cases; neither includes this
diagnostic instrumentation or relaxes any comparison.

The unmerged fixed-function card has54 passing cases and five finite failures.
Both debug and release reproduce all five. The full264-case run passed259 cases;
all49 LUT/config cases passed, including all four inverse3D Linux tests.

| Case | Original measured error | Upstream limit | Current evidence |
|---|---:|---:|---|
| ACES2 P3 4000nit round trip |0.018350264|0.018|Explicit local precision reduces measured error to0.017441862; version400 alone has no effect. Diagnostic source only. |
| ACES2 nan_bug |relative0.83815956|0.01|Finite input and outputs; full native replay and constant-image reproduction exact. Detailed arithmetic cause still unresolved. |
| ACES2 Rec709 round trip |0.004482085|0.004|Finite; native replay and constant-image reproduction exact. Precision-qualified worst error unchanged. |
| RGB to JMh |0.00022888184|0.0002|Finite; native replay and constant-image reproduction exact. Version/precision controls unchanged. |
| GamutComp13 inverse |0.000022411346|0.000003|Minimal pow shader reproduces the exact original GPU result. Three ULPs of pow difference are amplified; platform-pow substitution reduces error to0.000000954. |

The original shader, metadata and whole GPU image match the port bit for bit
before each CPU/GPU check fails. Standalone C++/EGL/OpenGL replay links no OCIO or
Rust renderer. All resource uploads read back exactly and GL errors are fatal.
O0/O2 replay on both captured profiles matched20 complete original GPU images.
Repeating each actual worst-error input pixel over a complete image also matched
its original GPU output everywhere; image gradients are not required.

Original shaders are GLSL120. A controlled version400-only experiment preserved
all five images exactly. A separate precise-local-variable experiment changed
some images and reduced only the P3 worst error below its original threshold.
These are diagnostic variants; neither is a shipped fix or new expected output.
The GLSL precise control is based on section4.9 of the
[official specification](https://registry.khronos.org/OpenGL/specs/gl/GLSLangSpec.4.60.pdf).

The minimal GamutComp13 inverse probe uses the actual public upstream worst
input. At native operand0x3f7b3d70, GPU pow is0x3f7a4c78 and platform libm is
0x3f7a4c75. The reduced shader's result0xc0451445 equals the original full GPU
result. Replacing only that pow result through a runtime uniform gives
-3.079340696 versus the actual wheel CPU's-3.079339743. No parity expectation is
calculated from port code; all reference buffers are actual captured wheel
results, and the scalar control uses platform libm.

This disproves the proposition that every remaining failure is undefined
non-finite behavior. Do not broaden W0013, add per-test waivers, loosen limits,
ignore tests or count these five as passing. Original failures remain intact.
The remaining three detailed causes require further isolation; P3's controlled
precision result narrows its cause but does not yet distinguish each compiler
transformation.

Evidence: CI40946684484 (five failures),40948001002 (259/264),40950573272
(actual captures),40952667512 (independent native replay),40953181629
(constant-input controls),40955304252 (version/precision),40956371150
(minimal pow). Complete artifacts/logs and scripts are kept in sibling ci-*
folders under kept/gpu-next/2026-10-29. The public source/destination provenance
was verified before restricted export; no user images/configs/files are used.
Diagnostic branches codex/p21-finite-diagnostics and codex/p21-native-replay
must never be merged.

Native comparator CI41017434341 was dispatched on 2026-10-30T07:02:37.9208372Z; results pending. It compiles original upstream Shader/DiffComponent functions without OCIO or Rust linkage, replaying both preserved official capture profiles at O0/O2 with equal-reference controls. This checks the comparison implementation independently; it does not yet isolate the remaining arithmetic causes. Diagnostic branch codex/p21-native-comparator must never merge. No limit or waiver changed.

CI41017434341 completed successfully. Independent artifact verification confirmed 20 native comparisons (five cases, both captured profiles, O0/O2) and 20 equal-reference controls. The original C++ comparator reproduces all five threshold failures, with maximum-error bits and indices identical to Rust. No OCIO or Rust renderer is linked. Evidence: ../ci-41017434341/verified-summary.json. This rules out a comparison implementation discrepancy for these captures; the remaining arithmetic causes are still unresolved and original tests stay failing.
