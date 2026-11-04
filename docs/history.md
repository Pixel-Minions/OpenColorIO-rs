# How the port was built

The port of OpenColorIO 2.5.2 landed in milestones, each a complete section of OCIO with exact
checks against the official library. Every card went through an independent review before
merging, and its tests were mutation-tested. The plan is [`PLAN.md`](../PLAN.md); the cards are
in [`cards/`](cards/).

| Milestone | Section | Version |
|---|---|---|
| **M0** | The op engine, the analytic transforms and the processor API | `0.1.0` |
| **M1** | LUTs, fixed functions (ACES 2.0 included), built-in transforms, configs read and written | `0.2.0` |
| **M2** | All 24 file formats, file-based configs and `.ocioz` archives, the baker | `0.3.0` |
| **M3** | Dynamic properties, the grading ops, the app helpers | `0.4.0` |
| **M4** | The `PyOpenColorIO`-compatible Python module | `0.5.0` |
| **M5** | Full OCIO 2.5.2 parity: GPU execution, SIMD speed, the tools, the release | `1.0.0` |

## Foundations and M0 (Phase 1)

- **The test harness:** the live oracle, hash-locked reference fixtures, the guardrails
  (`cargo xtask ci`), the parity dashboard, the Rocky Linux 9 reference container, and CI on
  Windows and Linux on every pull request.
- **Building blocks:** cache-ID hashing (XXH3-128), which reproduces the official cache IDs of
  all 8 built-in configs; OCIO's fast-math functions; CPU feature detection; the 3D LUT forward
  kernels in every SIMD variant; the three half-float conversions OCIO uses; C and C++ number
  formatting and parsing, as each platform's runtime does it; and a port of the yaml-cpp emitter.
- **The op engine:** bit-depth conversion and every pixel layout OCIO accepts; format metadata,
  dynamic properties and logging; the optimizer, with its passes, every optimization level and
  the separable-prefix bake into a 1D LUT; and the CPU processor and its renderers, with fast
  math on and off, for every SIMD profile.
- **Every analytic op family**, with its op data, validation, CPU renderers and GPU shader
  writer in all 10 languages: Matrix, Range, Exponent, ExponentWithLinear, Log, LogAffine,
  LogCamera and CDL, plus the forward 1D LUT the optimizer bakes.
- **The transforms and processors:** MatrixTransform, RangeTransform, ExponentTransform,
  ExponentWithLinearTransform, LogTransform, LogAffineTransform, LogCameraTransform,
  CDLTransform, AllocationTransform, Lut1DTransform and GroupTransform; `Config::CreateRaw()`,
  `getProcessor` with its caches, and the CPU and GPU processors.
- **An API sweep:** every analytic transform through the port's processors, on the CPU at every
  bit depth, layout and optimization level, and on the GPU in all 10 languages.

## M1 (Phases 2 and 3)

- **Phase 2** ([card](cards/phase2.md)): the 1D and 3D LUTs with their exact and fast
  inverses, composition and GPU writers; every fixed function, the ACES 1.x renderers and ACES
  2.0 (output transform, tone scale, gamut compression, RGB-to-JMh) on the CPU and the GPU; the
  GradingRGBCurve op the ACES 1.x built-ins use; and `std::regex` as each wheel's C++ library
  implements it, for file rules.
- **Phase 3** ([card](cards/phase3.md)): a port of yaml-cpp 0.8.0's parser, with its own tests;
  the config model (color spaces, roles, displays, views, looks, view and named transforms, the
  environment and context, file and viewing rules); configs read and written back byte for
  byte, with their cache IDs, for all 8 built-in configs and upstream's test configs;
  `validate()`; all 98 built-in transforms; configs from files, the environment, an I/O proxy
  and `ocio://` URIs; and the config transforms (ColorSpace, DisplayView, Look, File and
  Builtin) with their processors.

## M2 (Phase 4)

The format registry and FileTransform's loading and cache; an XML parser (a port of expat
2.7.2); the CDL, CC and CCC readers and writers; the spi1d, spi3d, spimtx, Discreet 1DL, Iridas
and Resolve `.cube`, `.itx`, `.3dl`, Houdini, Truelight, Pandora, Nuke `.vf`, cineSpace, ICC and
Iridas `.look` readers; the CLF/CTF reader and writer; the baker, in every format the official
library bakes; and `.ocioz` archives, through a port of minizip-ng 4.0.10.

## M3 (Phase 5)

The grading values (GradingPrimary, GradingTone, GradingHueCurve) with their dynamic
properties; the ExposureContrast and grading op families on the CPU and the GPU, with their
transforms, builders, YAML and CLF/CTF elements; dynamic properties end to end; and every app
helper (color space menus, display/view helpers, `LegacyViewingPipeline`, the mixing helpers).
A parity sweep checked grading text, pixels at every format and level, shaders in all 10
languages after dynamic sets, and the app helpers over upstream's test configs.

## M4 (Phase 6)

The `PyOpenColorIO` module: pybind11's dispatcher, casters, classes and enums as the official
wheel shows them; every transform class; the config and everything it holds; NumPy buffers;
the grading classes, dynamic properties and app helpers; processors and shader descriptions;
the baker; ConfigUtils and config merging; and SystemMonitors. All 384 upstream Python tests
pass unmodified, and the module's metadata matches the official wheel on both platforms, within
the approved [D-16 scope](deviations.md).

## M5 (Phases 7 to 10)

- **Phase 7, GPU execution:** the legacy GPU API, a reference client linked to the official
  wheel, and headless OpenGL on llvmpipe. All 264 upstream GPU tests compare complete rendered
  images bit for bit with the official shader's output. Legacy shader and LUT data are checked
  against the official native API in all 10 languages, including post-extraction dynamic
  updates across 5,880 live requests ([coverage](cards/p88-legacy-live-qualification.md)).
  Every shader of the 264 tests compiles on real compilers except 36 in GLSL ES 1.0, which are
  upstream's own text (open item P7-9).
- **Phase 8, hardware SIMD** ([card](cards/phase8.md)): most paths run within 10% of the
  official library on the same machine; the exceptions are accepted for 1.0.0 and listed in
  the [changelog](../CHANGELOG.md). `CpuProcessor::apply_threaded` renders rows on several
  threads.
- **Phase 9:** ConfigUtils, OS monitor support and config merging (every section, OCIOM
  chains, whole-config and single-color-space merges); the tools `ociocheck`, `ociochecklut`,
  `ociowrite` and `ociobakelut`, ICC output included; and the public-header audit
  ([P89](cards/p89-public-header-completion.md)).
- **Phase 10, the release:** source accounting (913 of 913 files), the release documents,
  clean-room rewrites of the three pieces that had been translated from libstdc++, one decision
  on every entry of the [improvement register](improvements.md), and the qualification on
  emulated CPUs ([P63](cards/p63-sde-followups.md)).
