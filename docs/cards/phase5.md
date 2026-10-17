# Phase 5 cards: dynamic properties, grading ops and app helpers

**Status: planned, 2026-10-17.** The owner decided P5-1 to P5-6 on 2026-10-17, all as
recommended. Phase 5 starts once M2's Phase 4 cards have landed; parts of it can start earlier, as "Order and parallelism"
says.

**Goal: milestone M3** (PLAN.md §10, Phase 5, ~10 pw): OCIO 2.5.2's dynamic and grading
features and its app helpers, byte-exact with the `opencolorio==2.5.2` wheel:
- **dynamic properties of every type** (exposure, contrast, gamma, and the grading primary,
  RGB curve, hue curve and tone values): read and set through `Processor`, `CPUProcessor` and
  `GpuShaderDesc`, live in the CPU renderers and as GPU uniforms;
- **the grading ops**, each a whole family (op data, inverse, CPU renderers, GPU writer in all
  10 languages) with its transform, its YAML and CTF forms: ExposureContrast, GradingPrimary,
  GradingTone, GradingHueCurve, and the transform of GradingRGBCurve (its op landed in Phase 2);
- **the app helpers:** `ColorSpaceMenuParameters` and `ColorSpaceMenuHelper` (with the
  category helpers and `ColorSpaceHelpers::AddColorSpace`), the `DisplayViewHelpers`,
  `LegacyViewingPipeline`, `MixingColorSpaceManager` and `MixingSlider`;
- **the Phase 4 leftovers:** the CTF reader and writer arms of the four ops (4.5i, 4.6f), and
  every upstream test of earlier phases that waits for this one.

The exact Lut3D inverse, which PLAN.md's M3 row also names, landed in Phase 2 (2.2d).
Config merging (`ConfigMerger`, the OCIOM format) is Phase 9's.

"Byte-exact" means, on Windows and Rocky Linux 9:
- **pixels:** every op, both directions, at every bit depth, layout and optimization level, for
  default and set dynamic values, bit for bit with the wheel on the same OS and CPU (§3);
- **GPU:** shader text, uniform names, types, buffer offsets and live values (after each set
  of a dynamic property), and textures, in all 10 languages;
- **text:** the transforms' and grading values' `operator<<`, their YAML (`serialize`) and CTF
  text, every exception and warning, the app helpers' menus, names, families, descriptions
  and hierarchy levels, byte for byte; the processors the helpers return, op for op with their
  cache IDs.

Each work package below is a list of **chunks**. A chunk is one commit and is mergeable on its
own (`CLAUDE.md` → "Chunks"). Line counts are upstream's non-blank lines at v2.5.2 without
`//` comments (block comments counted: the counts are upper bounds). Paths are relative to
`src/OpenColorIO/` unless they start with `tests/` or `include/`.

## How this phase works

Everything in `docs/cards/phase3.md` → "How this phase works" and `docs/cards/phase4.md` →
"How this phase works" still applies: cards and PRs, the chunk gate, oracle-first, owner items,
verifier rounds with mutation testing. What changes in Phase 5:

- **Numeric by default.** Every op chunk runs `cargo xtask gate --release --rocky`, and every
  family runs through the battery (`ocio_testkit::battery`): parameter cases per style
  (`mutation_bases`, which generate the extreme, NaN and infinite parameters), both
  directions, fast math on and off, the `quick` tier per chunk and `full` per land.
- **Compile-time SSE2 branches.** The ExposureContrast, GradingPrimary and GradingTone
  renderers have `#if OCIO_USE_SSE2` branches (`ssePower`, `_mm_*` arithmetic) that both x86-64
  wheels compile in whatever the CPU and the optimization flags: the port follows those
  branches, operation for operation, as `p2-rgbcurve` did for GradingRGBCurve's LinLog. They
  are not numeric profiles: one renderer per op, in `sse2.rs`-style modules where `unsafe` is
  allowed. Where they call libm (`powf(2.f, exposure)` once per `apply`, the tone curves' `log`
  and `exp`) the call is the platform's (D12). `docs/wheel-inspect.md` checks each kernel's
  operand order and whether a call is libm's, SVML's or inlined.
- **Dynamic values are inputs.** A test of a dynamic op gives the wheel and the port the same
  sequence: build the processor, take its dynamic property, set a value, apply (CPU) or read the
  uniforms (GPU), set another value, and so on (O5.1). The battery's cases may name a dynamic
  property and the values to set before the pixels are compared.
- **Grading values validate.** `GradingPrimary`, `GradingTone`, `GradingRGBM(SW)` and the hue
  curves refuse values with texts the tests pin (`validate`), at the setter of a dynamic property
  as well as at the transform's `validate` (`DynamicProperty_tests.cpp setter_validation`).
- **App helpers are text.** Menus, names and families are compared byte for byte through O5.2,
  over the built-in configs, upstream's test configs and generated ones (categories, encodings,
  roles, aliases, inactive spaces, named transforms, Unicode names).
- **Owner items** stop at the owner, labelled `waiver`, `deviation`, `api` or `dependency`, as
  in Phases 3 and 4. Oracle chunks are reviewed by a verifier agent (the owner's decision of
  2026-10-05).

**What Phase 5 needs from Phases 1 to 4.**
- Phase 1: the op engine, `DynamicPropertyRcPtr` with its double properties (1.2b), the
  processors' dynamic queries (`isDynamic`, `hasDynamicProperty`, `getDynamicProperty`, 1.2d
  and 1.8g), the optimizer's `RemoveDynamicProperties` and its dynamic guards, and
  `GpuShaderDesc`'s live uniform getters (1.7c).
- Phase 2: GradingRGBCurve's op family (2.6a–e), the B-spline curves and their RGB fitting,
  and `DynamicPropertyGradingRGBCurve`; the fast-math functions (`ssePower` and siblings).
- Phase 3: the `Transform` enum and its `BuildOps`/`CreateTransform` dispatch, the YAML loader
  and saver (whose four grading tags and ExposureContrast tag give "not ported yet", Phase 5),
  the config model, the display/view, look and named-transform processors, the categories and
  encodings of color spaces.
- Phase 4: the CLF/CTF reader and writer (their Phase 5 elements refuse with "not ported yet",
  P4-2), `FileTransform` for the helpers' and tests' files, and the oracle's `with_files` (O4.2).

---

## WP 5.0: grading values and dynamic properties (`ocio-ops`)

| Chunk | Upstream | Rust (`ocio-ops/src/`) | Port tests |
|---|---|---|---|
| 5.0a | `include/OpenColorIO/OpenColorTransforms.h:433-496` (`GradingRGBM`, `GradingPrimary`: the fields, defaults per style, `validate`); `ops/gradingprimary/GradingPrimary.cpp/.h` (271): the pre-render values (`GradingPrimaryPreRender::update`); `DynamicProperty.cpp:142-200` (51): `DynamicPropertyGradingPrimaryImpl` (style, direction, `setValue` with validation); `operator<<` of `GradingRGBM` and `GradingPrimary` (`transforms/GradingPrimaryTransform.cpp:127-160`) | `ops/gradingprimary/grading_primary.rs`, `dynamic_property.rs` | `GradingPrimary_tests.cpp` (3); `DynamicProperty_tests.cpp` `equal_grading_primary` |
| 5.0b | `OpenColorTransforms.h:642-764` (`GradingRGBMSW`, `GradingTone`); `ops/gradingtone/GradingTone.cpp/.h` (721): `validate`, the pre-render values (`GradingTonePreRender`: the mids, highlights and shadows, whites and blacks, s-contrast coefficients); `DynamicProperty.cpp:401-441` (33): `DynamicPropertyGradingToneImpl`; `operator<<` (`transforms/GradingToneTransform.cpp:127-160`) | `ops/gradingtone/grading_tone.rs`, `dynamic_property.rs` | `GradingTone_tests.cpp` (3) |
| 5.0c | `OpenColorTransforms.h:587-641` (`GradingHueCurve`, `HueCurveType`); `ops/gradinghuecurve/GradingHueCurve.cpp/.h` (343): the curves per type, defaults, `validate`, `isIdentity`; `DynamicProperty.cpp:301-400` (77): `DynamicPropertyGradingHueCurveImpl`; `operator<<` (`transforms/GradingHueCurveTransform.cpp:157-180`) | `ops/gradinghuecurve/grading_hue_curve.rs`, `dynamic_property.rs` | `GradingHueCurve_tests.cpp` (3); `DynamicProperty_tests.cpp` `get_as`, `setter_validation` |
| 5.0d | The hue curves' parts of `ops/gradingrgbcurve/GradingBSplineCurve.cpp` (539): `PrepHueCurveData`, `CalcKsi`, `FitHueSpline`, `EstimateHueSlopes` (`17-334`), the `Create` overloads of a `HueCurveType` (`508-555`), `computeKnotsAndCoefsForHueCurve` (`864-1008`), `KnotsCoefs::evalCurveRevHue` (`1385-1460`) | `ops/gradingrgbcurve/grading_b_spline_curve.rs` | `GradingBSplineCurve_tests.cpp`'s hue parts; `DynamicProperty_tests.cpp` `grading_hue_curve_knots_coefs` |

- The values are Rust values with public fields, as upstream's structs (P5-2); the curves stay
  values, as `p2-rgbcurve` made the RGB curves (2.6a). The dynamic property handles grow from
  1.2b's enum (P5-1).
- The hue curves' fitting follows each wheel's operand order where the two compilers differ, as
  the RGB fitting does (I-91): 5.0d checks the machine code. `AddShaderEvalRevHue`
  (`1168-1246`) comes with 5.4c.

## WP 5.1: ExposureContrast (`ocio-ops`, `ocio-gpu`)

| Chunk | Upstream (`ops/exposurecontrast/`) | Rust | Port tests |
|---|---|---|---|
| 5.1a | `ExposureContrastOpData.cpp/.h` (487): the styles (linear, video, logarithmic, each forward and reverse), exposure, contrast, gamma (each a dynamic double), pivot, log exposure step and mid-gray, `validate`, `isIdentity`, `isInverse`, `inverse`, the cache ID; `ExposureContrastOp.cpp/.h` (185): the op (no combining), `CreateExposureContrastOp` | `ocio-ops/src/ops/exposurecontrast/exposure_contrast_op_data.rs`, `exposure_contrast_op.rs` | `ExposureContrastOpData_tests.cpp` (6), `ExposureContrastOp_tests.cpp` `create`, `inverse` |
| 5.1b | `ExposureContrastOpCPU.cpp/.h` (565): the six renderers, each reading its dynamic values at `apply`, with their SSE2 branches (`ssePower`) | `exposure_contrast_op_cpu.rs` | `ExposureContrastOpCPU_tests.cpp` (5); battery family `exposure_contrast`, with dynamic sets (O5.1) |
| 5.1c | `ExposureContrastOpGPU.cpp/.h` (285): the shader per style, the dynamic values as uniforms (`addUniform` with live getters), constants otherwise | `ocio-gpu/src/ops/exposurecontrast/exposure_contrast_op_gpu.rs` | oracle `gpu_shader` in all 10 languages, and O5.1's uniform values after sets |

## WP 5.2: GradingPrimary (`ocio-ops`, `ocio-gpu`)

| Chunk | Upstream (`ops/gradingprimary/`) | Rust | Port tests |
|---|---|---|---|
| 5.2a | `GradingPrimaryOpData.cpp/.h` (268): style, direction, value, the dynamic property, `validate`, `isIdentity`, `isInverse`, `inverse`, cache ID; `GradingPrimaryOp.cpp/.h` (208): the op, `CreateGradingPrimaryOp` | `ocio-ops/src/ops/gradingprimary/grading_primary_op_data.rs`, `grading_primary_op.rs` | `GradingPrimaryOpData_tests.cpp` (3), `GradingPrimaryOp_tests.cpp` `create` |
| 5.2b | `GradingPrimaryOpCPU.cpp:1-611` (~490): the renderer base (dynamic or not), log forward and reverse, with the SSE2 branches | `grading_primary_op_cpu.rs` | `GradingPrimaryOpCPU_tests.cpp` log cases; battery family `grading_primary` (log) |
| 5.2c | `GradingPrimaryOpCPU.cpp:612-1310` (~590): linear and video, forward and reverse; `GetGradingPrimaryCPURenderer` | `grading_primary_op_cpu.rs` | `GradingPrimaryOpCPU_tests.cpp` (4 in all); the battery family's other styles |
| 5.2d | `GradingPrimaryOpGPU.cpp/.h` (515): the uniforms of a dynamic value, the three styles both ways | `ocio-gpu/src/ops/gradingprimary/grading_primary_op_gpu.rs` | oracle `gpu_shader`, O5.1 |

## WP 5.3: GradingTone (`ocio-ops`, `ocio-gpu`)

| Chunk | Upstream (`ops/gradingtone/`) | Rust | Port tests |
|---|---|---|---|
| 5.3a | `GradingToneOpData.cpp/.h` (199), `GradingToneOp.cpp/.h` (203): as 5.2a | `ocio-ops/src/ops/gradingtone/grading_tone_op_data.rs`, `grading_tone_op.rs` | `GradingToneOpData_tests.cpp` (3), `GradingToneOp_tests.cpp` `create` |
| 5.3b | `GradingToneOpCPU.cpp:1-644` (~550): the renderers' classes, the mids, highlights and shadows, forward and reverse | `grading_tone_op_cpu.rs` | `GradingToneOpCPU_tests.cpp` mids, highlights and shadows cases |
| 5.3c | `GradingToneOpCPU.cpp:645-1290` (~520): whites and blacks, s-contrast, the `apply`s, the linear style's LinLog and LogLin, `GetGradingToneCPURenderer` | `grading_tone_op_cpu.rs` | `GradingToneOpCPU_tests.cpp` (8 in all); battery family `grading_tone` |
| 5.3d | `GradingToneOpGPU.cpp:1-887` (~640): the uniforms (`AddGTProperties`), the mids, highlights and shadows shaders | `ocio-gpu/src/ops/gradingtone/grading_tone_op_gpu.rs` | oracle `gpu_shader` |
| 5.3e | `GradingToneOpGPU.cpp:888-end` (~720): whites and blacks, s-contrast, the forward and inverse programs, `GetGradingToneGPUShaderProgram` | `grading_tone_op_gpu.rs` | oracle `gpu_shader` in all 10 languages, O5.1 |

## WP 5.4: GradingHueCurve (`ocio-ops`, `ocio-gpu`)

| Chunk | Upstream (`ops/gradinghuecurve/`) | Rust | Port tests |
|---|---|---|---|
| 5.4a | `GradingHueCurveOpData.cpp/.h` (259), `GradingHueCurveOp.cpp/.h` (208): style, direction, draw-curve-only mode, the curves, the dynamic property, `validate`, cache ID | `ocio-ops/src/ops/gradinghuecurve/grading_hue_curve_op_data.rs`, `grading_hue_curve_op.rs` | `GradingHueCurveOpData_tests.cpp` (2), `GradingHueCurveOp_tests.cpp` `create` |
| 5.4b | `GradingHueCurveOpCPU.cpp/.h` (295): the draw, forward and reverse renderers, RGB to HSY and back | `grading_hue_curve_op_cpu.rs` | `GradingHueCurveOpCPU_tests.cpp` (7); battery family `grading_hue_curve` |
| 5.4c | `GradingHueCurveOpGPU.cpp/.h` (495); `GradingBSplineCurve.cpp:1168-1246` (`AddShaderEvalRevHue`) | `ocio-gpu/src/ops/gradinghuecurve/grading_hue_curve_op_gpu.rs` | oracle `gpu_shader`, O5.1 |

## WP 5.5: the transforms, their builders and their YAML (`ocio`)

| Chunk | Upstream | Rust (`ocio/src/`) | Port tests |
|---|---|---|---|
| 5.5a | `transforms/ExposureContrastTransform.cpp/.h` (225): the class, `make*Dynamic`, `validate`, `equals`, `operator<<`; `ExposureContrastOp.cpp:197-240` (`CreateExposureContrastTransform`, `BuildExposureContrastOp`); the `BuildOps` and `CreateTransform` arms (`Transform.cpp:87-90, 216-230`); `OCIOYaml.cpp:1194-1332` (122) and the tag's arms (`3251-3268, 3379-3384`) (~430) | `transforms/exposure_contrast_transform.rs`, `transform.rs`, `ocio_yaml.rs` | `ExposureContrastTransform_tests.cpp` (3), `ExposureContrastOp_tests.cpp` `create_transform`; `Config_tests.cpp` `exposure_contrast_serialization` |
| 5.5b | `transforms/GradingPrimaryTransform.cpp/.h` (168); `GradingPrimaryOp.cpp:223-260` (create and build); its `Transform.cpp` arms; `OCIOYaml.cpp:1519-1928` (377: `GradingRGBM` and the transform) (~600) | `transforms/grading_primary_transform.rs`, `transform.rs`, `ocio_yaml.rs` | `GradingPrimaryTransform_tests.cpp` (7), `GradingPrimaryOp_tests.cpp` `create_transform`, `build_ops`; `Config_tests.cpp` `grading_primary_serialization` |
| 5.5c | `transforms/GradingRGBCurveTransform.cpp/.h` (199: the class, which 3.1 left for this phase); `GradingRGBCurveOp.cpp:223-260`; its `Transform.cpp` arms (and `CreateTransform`'s, which refuses it as "not ported yet" until then); `OCIOYaml.cpp:1929-2147` (195) (~450) | `transforms/grading_rgb_curve_transform.rs`, `transform.rs`, `ocio_yaml.rs` | `GradingRGBCurveTransform_tests.cpp` (4), `GradingRGBCurveOp_tests.cpp` `create_transform`, `build_ops`; `Config_tests.cpp` `grading_rgbcurve_serialization` |
| 5.5d | `transforms/GradingToneTransform.cpp/.h` (155); `GradingToneOp.cpp:217-260`; its `Transform.cpp` arms; `OCIOYaml.cpp:2310-2500` (172: `GradingRGBMSW` and the transform) (~380) | `transforms/grading_tone_transform.rs`, `transform.rs`, `ocio_yaml.rs` | `GradingToneTransform_tests.cpp` (3), `GradingToneOp_tests.cpp` `create_transform`, `build_ops`; `Config_tests.cpp` `grading_tone_serialization` |
| 5.5e | `transforms/GradingHueCurveTransform.cpp/.h` (180); `GradingHueCurveOp.cpp:219-260`; its `Transform.cpp` arms; `OCIOYaml.cpp:2148-2309` (144) (~380) | `transforms/grading_hue_curve_transform.rs`, `transform.rs`, `ocio_yaml.rs` | `GradingHueCurveTransform_tests.cpp` (4), `GradingHueCurveOp_tests.cpp` `create_transform`, `build_ops`; `Config_tests.cpp` `grading_huecurve_serialization` |

- The YAML savers write the grading values with yaml-cpp's precision (`%.7g`/`%.15g`, 3.7); the
  sweep of `p5-m3-parity` checks every value's text against the wheel.
- The transforms' `operator<<` is the basis of Python's `repr` (Phase 6): checked through
  `transform_text`.

## WP 5.6: dynamic properties end to end (`ocio-ops`, `ocio`, `ocio-gpu`)

| Chunk | What | Port tests |
|---|---|---|
| 5.6a | The processors' dynamic properties of every type: `OpRcPtrVec::validateDynamicProperties` (`Op.cpp:375-450`) over the seven types, sharing between ops and copies (`Processor` and `CPUProcessor`), `OPTIMIZATION_NO_DYNAMIC_PROPERTIES`, the processor cache's `PROCESSOR_CACHE_SHARE_DYN_PROPERTIES`, the GPU descriptions' dynamic properties | `DynamicProperty_tests.cpp` `get_dynamic_via_cpu_processor`; `Op_tests.cpp` `OpRcPtrVec/is_noop`, `OpRcPtrVec/dynamic_property`; `OpOptimizers_tests.cpp` `dynamic_ops`, `dyn_properties_prefix`, `opt_prefix_test1`; `Processor_tests.cpp` `is_noop`, `dynamic_properties`, `unique_dynamic_properties`, `cache_optimized_processors`, `cache_cpu_processors`, `cache_gpu_processors`; `CPUProcessor_tests.cpp` `dynamic_properties` |
| 5.6b | The tests of earlier phases that build these transforms | `Config_tests.cpp` `dynamic_properties`, `optimization_with_bitdepths`, `config_context_cacheids`, `context_variables_typical_use_cases` (the last two read `exposure_contrast_*.ctf`, after 4.5i); `DisplayViewTransform_tests.cpp` `build_ops`; `GpuShader_tests.cpp` `MetalSupport7`, `MetalSupport8`, `MetalSupport9` |

## WP 5.7: the app helpers (`ocio`)

| Chunk | Upstream (`apphelpers/`) | Rust (`ocio/src/apphelpers/`) | Port tests |
|---|---|---|---|
| 5.7a | `CategoryHelpers.cpp/.h` (493): the categories and encodings of color spaces and named transforms, `FindColorSpaceInfos`, `FindAllColorSpaceInfos`, the role and hierarchy helpers | `category_helpers.rs` | `CategoryHelpers_tests.cpp` (2) |
| 5.7b | `ColorSpaceHelpers.cpp/.h` (847; one class family its tests cover together, past the usual ~600): `ColorSpaceInfo`, `ColorSpaceMenuParameters`, `ColorSpaceMenuHelper` (the menu's order, UI names, families, descriptions, hierarchy levels, the added color spaces), `ColorSpaceHelpers::AddColorSpace` | `color_space_helpers.rs` | `ColorSpaceHelpers_tests.cpp` (10); oracle O5.2 |
| 5.7c | `DisplayViewHelpers.cpp` (410): `GetProcessor`, `GetIdentityProcessor`, `AddDisplayView`, `RemoveDisplayView` (with their color spaces and file transforms) | `display_view_helpers.rs` | `DisplayViewHelpers_tests.cpp` (5); O5.2 |
| 5.7d | `LegacyViewingPipeline.cpp/.h` (412): the pipeline's transforms (linear CC, color timing CC, channel view, display view, looks override, the exposure and contrast), `getProcessor`, `operator<<` | `legacy_viewing_pipeline.rs` | `LegacyViewingPipeline_tests.cpp` (4); O5.2 |
| 5.7e | `MixingHelpers.cpp/.h` (379): `MixingSlider`, `MixingColorSpaceManager` (the mixing spaces and encodings, the processors, the slider's ranges and conversions) | `mixing_helpers.rs` | `MixingHelpers_tests.cpp` (4); O5.2 |

- `include/OpenColorIO/OpenColorAppHelpers.h:15-540` is these chunks' public API (P5-3); its
  `ConfigMergingParameters`, `ConfigMerger` and `ConfigMergingHelpers` (`542-890`) are Phase 9's.

## WP 5.8: the Phase 4 leftovers (`ocio`)

| Chunk | Upstream | Rust (`ocio/src/fileformats/ctf/`) | Port tests |
|---|---|---|---|
| 4.5i | `fileformats/ctf/CTFReaderHelper.cpp`: `CTFReaderExposureContrastElt`, `CTFReaderECParamsElt`, `CTFReaderDynamicParamElt`, `CTFReaderGradingPrimaryElt`, `ParamElt`, `CTFReaderGradingToneElt`, `ParamElt`, `CTFReaderGradingHueCurveElt` (`1557-1861, 2204-2579, 2659-2737, 2974-3244`) (~900) | `ctf_reader_helper.rs` | `FileFormatCTF_tests.cpp` `exposure_contrast_video`, `_log`, `_linear`, `_no_gamma`, `_failures`, `attribute_float_parse_extra_values`, `attribute_float_parse_leading_spaces`, `load_grading_primary_log`, `_lin`, `_video`, `_errors`, `load_grading_huecurves_log`, `load_grading_curves_errors`; `CTFTransform` `load_grading_tone`, `load_grading_tone_errors` (15) |
| 4.6f | `fileformats/ctf/CTFTransform.cpp`: `ExposureContrastWriter`, `GradingPrimaryWriter`, `GradingHueCurveWriter`, `GradingToneWriter` (`937-1052, 1280-1501, 1634-1895`) (~505) | `ctf_writer.rs` | `CTFTransform` `exposure_contrast_video_ctf`, `_log_ctf`, `_linear_ctf`, `_not_dynamic_ctf`, `_log_params_ctf`, `grading_primary_log_ctf`, `_lin_ctf`, `_video_ctf`, `grading_rgbcurve_log_ctf`, `_video_ctf`, `_lin_ctf`, `grading_huecurve_lin_ctf`, `grading_tone_log_ctf`, `_lin_ctf`, `_video_ctf` (15); oracle `write_transform` |

- These keep their Phase 4 numbers (`docs/cards/phase4.md`, card `p4-after-p5`). 4.5i needs
  5.1a–5.4a (the op data); 4.6f needs 4.5i (round trips) and 5.5 (the transforms, for
  `GroupTransform::write`).

---

## Upstream tests

| File | Tests | Phase 5 chunks | Waits |
|---|---:|---|---|
| `ops/exposurecontrast/ExposureContrastOpData_tests.cpp`, `Op`, `OpCPU` | 14 | 14 (5.1a–b, 5.5a) | — |
| `ops/gradingprimary/GradingPrimary_tests.cpp`, `OpData`, `Op`, `OpCPU` | 13 | 13 (5.0a, 5.2a–c, 5.5b) | — |
| `ops/gradingtone/GradingTone_tests.cpp`, `OpData`, `Op`, `OpCPU` | 17 | 17 (5.0b, 5.3a–c, 5.5d) | — |
| `ops/gradinghuecurve/GradingHueCurve_tests.cpp`, `OpData`, `Op`, `OpCPU` | 15 | 15 (5.0c, 5.4a–b, 5.5e) | — |
| `ops/gradingrgbcurve/GradingRGBCurveOp_tests.cpp` | 3 | `create_transform`, `build_ops` (5.5c) | — (`create` landed in 2.6d) |
| `transforms/ExposureContrastTransform_tests.cpp` | 3 | 3 (5.5a) | — |
| `transforms/GradingPrimaryTransform_tests.cpp`, `GradingRGBCurveTransform`, `GradingToneTransform`, `GradingHueCurveTransform` | 18 | 18 (5.5b–e) | — |
| `DynamicProperty_tests.cpp` | 9 | 5 (5.0a–d, 5.6a) | — (4 landed in 1.2b and 2.6b) |
| `apphelpers/CategoryHelpers_tests.cpp`, `ColorSpaceHelpers`, `DisplayViewHelpers`, `LegacyViewingPipeline`, `MixingHelpers` | 25 | 25 (5.7a–e) | — |
| `apphelpers/MergeConfigsHelpers_tests.cpp` | 20 | — | Phase 9 (config merging) |
| `fileformats/FileFormatCTF_tests.cpp` | 231 | 30 (4.5i, 4.6f) | — (the rest landed in Phase 4) |
| `Processor_tests.cpp`, `CPUProcessor_tests.cpp`, `Op_tests.cpp`, `OpOptimizers_tests.cpp` | 12 of their tests | 12 (5.6a) | — |
| `Config_tests.cpp` | 9 of its tests | group G (7: the five serializations, `dynamic_properties`, `optimization_with_bitdepths`) and `config_context_cacheids`, `context_variables_typical_use_cases` (5.5, 5.6b) | `get_processor_from_two_configs` (Phase 9) |
| `transforms/DisplayViewTransform_tests.cpp` | 1 of its tests | `build_ops` (5.6b) | — |
| `GpuShader_tests.cpp` | 3 of its tests | `MetalSupport7`, `MetalSupport8`, `MetalSupport9` (5.6b) | `MetalLutTest` (the legacy GPU processor, Phase 7) |

About 167 upstream C++ tests land in Phase 5. The GPU suite's ExposureContrast and grading tests
(`tests/gpu/ECOp_test.cpp` and the four `Grading*Op_test.cpp`: 87) run in Phase 7's harness;
Phase 5 checks the same shaders' text and uniforms against the wheel. The Python suite's
`ExposureContrastTransformTest.py`, `GradingDataTest.py` and the four grading transform tests are
Phase 6's.

## Oracle support (verifier-reviewed chunks)

Each command lives in its own module under `oracle/ocio_oracle/` and reports what the library
does; each call captures the log.

| Chunk | Command (module) | What |
|---|---|---|
| O5.1 | `dynamic_apply` (`dynamic_api.py`) | A processor from a transform spec or a config (as `cpu_apply` takes it), its CPU processor at given bit depths and optimization, then steps in order: take a dynamic property of a type from the CPU processor (or the processor, or a GPU description after `extractGpuShaderInfo`), set its value (a double by its bits, or a grading value from an object spec), apply an image (as `image_apply` describes it), read the GPU description's uniforms and their values. Each step's result or exception. The way to compare a dynamic op's pixels and uniforms after each set |
| O5.2 | `object_calls` (`object_calls_api.py`) | A PyOpenColorIO object built from a spec or a factory (`ColorSpaceMenuParameters.Create(config)`, `LegacyViewingPipeline()`, `MixingColorSpaceManager.Create(config)`, a grading value), and a list of method calls in order, each with its arguments (transform and object specs, configs): every return value written out by `checks.dump` (a processor as `processor_ops` writes it, a config by `serialize()`), or its exception. The module functions of `DisplayViewHelpers` and `ColorSpaceHelpers` too. The app helpers' and grading values' commands in one |
| O5.3 | Fixture group `grading_text` (`regen.py`) | The `operator<<` and YAML text of generated grading transforms and values (every style, default and extreme values, NaN and infinities), where `oracle check-all` shows them the same on both platforms |

- O5.1 and O5.2 land first, in card `p5-oracle`; O5.3 with `p5-m3-parity`.
- The existing commands cover the rest: `cpu_apply` and the battery for default values,
  `gpu_shader` for the shaders, `processor_ops`, `config_calls`, `transform_text`,
  `write_transform` and `with_files` for the CTF arms and the YAML. The spec builder already
  takes any PyOpenColorIO value class (`GradingPrimary`, `GradingRGBMSW`, `GradingBSplineCurve`,
  ...).

## Owner decisions

**Decided by the owner on 2026-10-17, all six as recommended.** P5-1's race of a set beside an
apply is listed as a `U-` entry with chunk 5.0a.

| # | Decision | Recommendation (decided) |
|---|---|---|
| P5-1 | **Dynamic property handles** (`api`). Upstream hands out `shared_ptr`s to mutable properties: the processor's renderers read the value at each `apply`, and an application sets it between applies, with no synchronization (setting while another thread applies is a data race, undefined behaviour) | **Keep 1.2b's `DynamicPropertyRcPtr` enum, with a variant per type: `Double` (the `AtomicU64` of 1.2b), `GradingPrimary`, `GradingRgbCurve`, `GradingHueCurve`, `GradingTone`.** The grading variants hold their value and its pre-render data behind a `Mutex`: a setter validates, precomputes and swaps both under the lock, and a renderer copies them once per `apply` where upstream reads them, so a single-threaded program sees upstream's values exactly, and a concurrent set gives one whole value or the other, never a torn one. The race is listed as a `U-` entry (the owner's general rule). `get_value`/`set_value` per variant, with upstream's `As*` casts as `as_grading_primary()` and siblings returning `Result` ("Dynamic property value is not a ...") |
| P5-2 | **Grading values** (`api`) | **Plain Rust values with public fields, as upstream's structs:** `GradingRgbm`, `GradingPrimary` (`new(style)`, `validate()`, the defaults per style), `GradingRgbmsw`, `GradingTone`; `GradingHueCurve` and its `HueCurveType` as values like 2.6a's `GradingRgbCurve` (not shared pointers; `create_editable_copy` is `clone`). `NoClampBlack`/`NoClampWhite` as associated constants. Equality as upstream's `operator==` (exact on floats) |
| P5-3 | **App helpers** (`api`) | (a) `ColorSpaceMenuParameters::new(config: Arc<Config>)` with upstream's setters and getters, strings as bytes; `ColorSpaceMenuHelper::new(params) -> Result<_>`, its getters by index returning `&[u8]`, `index_from_name`, `name_from_ui_name`; `color_space_helpers::add_color_space(config: &mut Config, ...)`. (b) `display_view_helpers::{processor, identity_processor, add_display_view, remove_display_view}`, mutating functions taking `&mut Config`. (c) `LegacyViewingPipeline` as a value with upstream's setters (transforms as `Option<Transform>`), `processor(&self, config, context) -> Result<Arc<Processor>>`, `Display`. (d) `MixingColorSpaceManager::new(config: Arc<Config>)`, `slider(&mut self) -> &mut MixingSlider` for upstream's reference-returning `getSlider` (and the overload with a range). (e) Every type `Send + Sync` where upstream's is shareable, `Clone` where it has `createEditableCopy` |
| P5-4 | **The transforms' dynamic flags** (`api`) | **As upstream:** `ExposureContrastTransform::make_exposure_dynamic`, `make_contrast_dynamic`, `make_gamma_dynamic`, `make_non_dynamic`, `is_*_dynamic`; the grading transforms' `make_dynamic`, `make_non_dynamic`, `is_dynamic`. A transform holds values; the shared property is made when the op is built, as upstream's op data makes it, so two processors of one transform don't share a property unless the processor cache shares them (`PROCESSOR_CACHE_SHARE_DYN_PROPERTIES`, 1.8g) |
| P5-5 | **Waivers** (`waiver`) | **None expected.** The renderers' libm calls (`powf`, `std::pow`, `std::log`, `sqrtf`) are the platform's on both sides (D12), and `docs/wheel-inspect.md` showed SVML only in PQ (W0001). Each op chunk checks its kernels' machine code for SVML or a fused multiply-add; one found stops the chunk and comes to the owner as a waiver request with its measured bound, as W0001 did |
| P5-6 | **Dependencies** (`dependency`) | **None.** Every chunk is a translation of upstream; the app helpers use the config model, the categories and the regular expressions already ported |

## Platform risks

- **libm.** ExposureContrast calls `powf` (`powf(2.f, exposure)` once per `apply`, and in its
  scalar loops); GradingPrimary, GradingTone and the hue curves call `std::pow` and `std::log`
  (17 and 4 sites), the tone renderer `sqrtf`. Their bytes follow each platform (D12); the
  battery runs on both, and the `full` tier sweeps the parameters.
- **SSE2 branches.** `ssePower` and `_mm_*` arithmetic in the renderers: their operand orders
  follow the wheels' machine code, which MSVC and GCC may compile differently; NaN operands use
  `sse_add`/`sse_mul` in the order each wheel uses.
- **Pre-rendered coefficients.** The grading values precompute coefficients in `float` or
  `double` with upstream's casts; a cast or an order of summation off by one place changes the
  last bit of every pixel. For dynamic values the GPU uniforms expose them (O5.1), and each
  chunk compares them as well as the pixels.
- **YAML and CTF numbers.** The grading values' text in YAML (`%.7g`) and CTF (the writer's
  precision): every value is swept against the wheel (O5.3).
- **Pass-through alpha.** Every renderer passes alpha through; the battery checks it on every
  numeric profile (`pass_through`), as LLVM's SLP vectorizer can quiet a signalling NaN in it.

## Order and parallelism

```
p5-oracle (O5.1-O5.2) ───────────────────────────── oracle checks of every card below
p5-values (5.0a-d) ─┬─ p5-ec (5.1a-c) ────────┐
                    ├─ p5-primary (5.2a-d) ───┼─ p5-transforms (5.5a-e) ─┬─ p5-dynamic (5.6a-b) ─┐
                    ├─ p5-tone (5.3a-e) ──────┤                         └─ p4-after-p5 (4.5i, 4.6f)
                    └─ p5-huecurve (5.4a-c) ──┘                                                  │
p5-helpers-1 (5.7a-b) ─ p5-helpers-2 (5.7c-e: 5.7d needs 5.5a) ─────────────────────────────────┤
all of the above ── p5-m3-parity (O5.3) ── M3 ───────────────────────────────────────────────────┘
```

- **Implementer A (values, ExposureContrast, transforms):** `p5-values` → `p5-ec` →
  `p5-transforms` → `p5-dynamic` → `p4-after-p5`.
- **Implementer B (the two large grading families):** `p5-primary` → `p5-tone`.
- **Implementer C (oracle, hue curves, app helpers):** `p5-oracle` → `p5-huecurve` (after
  5.0c–d) → `p5-helpers-1` → `p5-helpers-2` → `p5-m3-parity`.
- **Verifier:** reviews every card before it lands, with mutation testing.
- With two implementers, C's cards go to whoever is free, `p5-oracle` first; the app helpers
  need none of the grading ops but 5.7d, and can run beside them from the start.

## Cards

Each card is one branch and one PR. "Now" means the card can start without waiting for other
Phase 5 cards (its owner decisions are made).

| Card | Chunks | Size | Who | Needs | Owner items |
|---|---|---|---|---|---|
| `p5-oracle` | O5.1, O5.2, each its own chunk | 2 | C | now | — (verifier-reviewed) |
| `p5-values` | 5.0a–5.0d | 4, ~2,200 | A | now | — (P5-1, P5-2 decided) |
| `p5-ec` | 5.1a–5.1c | 3, ~1,520 | A | `p5-values` (the double properties are 1.2b's: 5.1 can start beside 5.0) | — |
| `p5-primary` | 5.2a–5.2d | 4, ~2,080 | B | 5.0a | — |
| `p5-tone` | 5.3a–5.3e | 5, ~2,840 | B | 5.0b | — |
| `p5-huecurve` | 5.4a–5.4c | 3, ~1,330 | C | 5.0c–d | — |
| `p5-transforms` | 5.5a–5.5e | 5, ~2,240 | A | `p5-ec`, `p5-primary`, `p5-tone`, `p5-huecurve` (5.5a needs only `p5-ec`) | — (P5-4 decided) |
| `p5-dynamic` | 5.6a–5.6b | 2 | A | `p5-transforms` | — |
| `p5-helpers-1` | 5.7a–5.7b | 2, ~1,340 | C | now | — (P5-3 a decided) |
| `p5-helpers-2` | 5.7c–5.7e | 3, ~1,200 | C | `p5-helpers-1`; 5.5a for 5.7d | — (P5-3 b–d decided) |
| `p4-after-p5` | 4.5i, 4.6f | 2, ~1,400 | A | 5.1a–5.4a, `p5-transforms` (4.6f) | — |
| `p5-m3-parity` | O5.3, then every op through the battery's `full` tier with dynamic sets, every shader and its uniforms in all 10 languages, the YAML and CTF text, and the app helpers over the built-in and test configs | 2–3 | any | every card above | — |

12 cards, 35 chunks. When a card grows past about 6 chunks, it splits at a dependency boundary.

**Can start now:** `p5-oracle`, `p5-values`, `p5-helpers-1`; then `p5-ec` beside `p5-values`.

**Waits for other phases:**
- **Phase 4:** M2's cards, for `p4-after-p5` and the tests that read CTF files (5.6b).
- **Phase 6:** the Python suite's grading, exposure-contrast and app-helper tests.
- **Phase 7:** the GPU suite's 87 ExposureContrast and grading tests; `MetalLutTest`.
- **Phase 9:** config merging (`MergeConfigsHelpers`, 20 tests); `get_processor_from_two_configs`.

## Phase 5 exit (M3)

Through OCIO's API, on Windows and Rocky Linux 9:
- every ExposureContrast and grading op, both directions, matches the wheel's pixels at every bit
  depth, layout and optimization level, for default values and after each set of a dynamic
  property, and its shader text, uniforms and their values in all 10 languages;
- the transforms' text, YAML and CTF forms are byte-exact, and their refusals give the same
  messages;
- the app helpers give the same menus, names, families, descriptions, hierarchy levels, configs
  and processors as the wheel over the built-in configs, upstream's test configs and the
  generated ones;
- the upstream tests of the slice are ported and counted in `docs/parity.md`, except those
  waiting for Phases 6, 7 and 9, each listed in `upstream-map.toml` with its reason.
