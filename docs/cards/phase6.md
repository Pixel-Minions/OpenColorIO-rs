# Phase 6 cards: the Python module

**Status: planned, 2026-10-17.** Owner items P6-1 to P6-12 are open (below, each with a
recommendation). PLAN.md §10 starts this track after M1, which is done. It binds each part of
the Rust API as it lands, and it finishes after Phase 5 (and Phase 9's two library parts that
the suite reaches, P6-11).

**Goal: milestone M4** (PLAN.md §10, Phase 6, ~12 pw): a `PyOpenColorIO` module that behaves
like the `opencolorio==2.5.2` wheel's (owner decision D13: import name `PyOpenColorIO`, a
standalone wheel that Rust applications can also embed):
- every name the wheel's module has, at module level and nested: about 130 classes, 62
  iterator classes, 34 enums, the constants and the module functions, with upstream's keyword
  names and defaults (`src/bindings/python/`, 9,301 lines);
- pybind11's behaviour where Python code can see it: overload resolution, argument conversion,
  `TypeError`s, enums, iterators, exceptions, `__doc__`;
- NumPy and the buffer protocol for float32, float16, uint8 and uint16 images and LUTs, in
  place, with the GIL released while OCIO runs;
- Python subclasses of `ConfigIOProxy`, and Python callbacks for logging and file hashing;
- upstream's Python suite (`tests/python`, 384 tests in 49 files, counted by
  `cargo xtask upstream-tests`) passing unmodified against the port's module, on Windows and
  Rocky Linux 9.

"Byte-exact" means, for Python code calling the module (P6-7):
- **values:** what every call returns, compared with the wheel's: numbers by their bits,
  strings and bytes byte for byte, NumPy arrays by dtype, shape and bytes, objects through
  their getters;
- **text OCIO writes:** `repr()` and `str()` (upstream's `defRepr` prints `operator<<`), and
  every exception's message, byte for byte (the Rust API already matches them);
- **exception types:** `OCIO.Exception`, `OCIO.ExceptionMissingFile`, `TypeError`,
  `IndexError`, `ValueError`, `UnicodeDecodeError` and the others, as the wheel raises them, and
  the class hierarchy as pybind11 makes it (`register_exception` derives both OCIO classes
  from Python's `Exception`, so `ExceptionMissingFile` is not a subclass of `OCIO.Exception`);
- **enums:** pybind11's: each enum reachable nested in its class and at module level
  (`export_values`), `__members__` in declaration order, `.name`, `.value`, `int()`,
  `__index__`, `==` and `hash` as pybind11 defines them, `repr` `<Enum.MEMBER: n>` and `str`
  `Enum.MEMBER`;
- **the module's surface:** names, kinds, `__module__`, `__qualname__`, `__doc__` (including
  the signature lines pybind11 writes into it) and keyword names, compared with the wheel's
  (O6.1);
- **pybind11's own text:** the `TypeError` of an argument no overload accepts ("incompatible
  function arguments. The following argument types are supported: ..."), and the other
  messages pybind11 writes (P6-7).

Each work package below is a list of **chunks**. A chunk is one commit and is mergeable on its
own (`CLAUDE.md` → "Chunks"). Line counts are upstream's non-blank lines at v2.5.2 without `//`
comments. Paths are relative to `src/bindings/python/` unless they start with `tests/` or
`src/OpenColorIO/`. Each chunk's last column lists the upstream Python test files it makes
pass: the count after each is the file's number of tests.

## How this phase works

Everything in `docs/cards/phase3.md` → "How this phase works" still applies: cards and PRs, the
chunk gate, oracle-first, owner items. What changes in Phase 6:

- **Binding, not porting, the library.** Every Python-visible behaviour comes from a pybind11
  definition in `src/bindings/python/` (PLAN.md §7: "the same names, keyword arguments and
  defaults"). The `ocio-py` crate calls the Rust API; it computes nothing itself. Where the
  binding needs something the Rust API doesn't offer yet, the chunk adds it to the library, as
  an `api` owner item.
- **Two Python environments.** The oracle's environment has the wheel
  (`oracle/`, Python 3.13, `opencolorio==2.5.2`, `numpy==2.3.5`). The port's module runs in
  a second one, `target/py-port` (P6-4), with the same interpreter and NumPy and without the
  wheel, so that `import PyOpenColorIO` finds the port's module. The two never mix (D13).
- **Upstream's suite, unmodified.** `cargo xtask py-suite` (6.1b) builds the module, then runs
  every test of `tests/python` in `target/py-port`, one module at a time, each in a fresh
  process, with unittest's loader (so that a module that fails to import fails its own tests
  only), and writes each test's outcome. CI fails when a test listed as passing fails. The
  list only grows (the ratchet, as for ported C++ tests), and `docs/parity.md` counts its
  tests ("python n/384"). Once every test passes, the M4 check runs
  `tests/python/OpenColorIOTestSuite.py` itself, as upstream's CI does.
- **The suite's baseline is the wheel.** O6.3 runs the same suite against the wheel on each
  platform. A test that fails against the wheel is listed with its message, not ported around.
- **Oracle checks beyond the suite.** Every chunk compares its classes with the wheel through
  O6.1 (the surface) and O6.2 (scripts of calls run in both environments: valid calls, wrong
  types, out-of-range indices, `None`, keyword calls, every overload). The suite alone tests a
  small part of the binding: 384 tests for about 1,060 methods.
- **pybind11 is the reference for the binding layer.** Its casters decide which Python values a
  parameter accepts (an `int` where a `float` is expected, `bytes` where `str` is, a list or a
  tuple for a `std::vector`, `None`), its dispatcher tries overloads in two passes (without,
  then with implicit conversions), and it decodes `std::string` results as strict UTF-8 (a
  non-UTF-8 name raises `UnicodeDecodeError`). The port reproduces these rules in one place
  (6.4a), and the oracle checks them through O6.2. pybind11's behaviour is what the wheel
  shows, whichever pybind11 version built it; nothing of pybind11 is translated, so it needs
  no third-party notice.
- **Objects that alias.** pybind11 holds OCIO's `shared_ptr`s: a Python object may be the same
  C++ object as one its config holds, so a change through it shows in the config. Which
  getters alias is decided per getter by the oracle (P6-8), and the Rust API gives the binding
  the shared handles it needs, as `CurrentContext` and `ConfigRules` do (owner decisions
  2026-10-06 and 2026-10-07).
- **`unsafe`.** Allowed in `ocio-py` (D8, `cargo xtask guards`), and kept to the buffer views
  and the GIL release (P6-10).
- **Owner items** stop at the owner, labelled `oracle`, `api`, `dependency` or `deviation`.

**What Phase 6 needs from Phases 1–5.**
- Phase 1: the analytic transforms, `Config::CreateRaw`, processors, CPU and GPU processors,
  `GpuShaderDesc`, the image descriptors (landed).
- Phase 2: Lut1D, Lut3D, FixedFunction, the built-in transforms (landed).
- Phase 3: the config model, its YAML, rules, contexts, the color-space, display-view, look and
  named-transform processors, the built-in configs, `ConfigIOProxy` (landed).
- Phase 4: `FileTransform` and the readers (landed); `GroupTransform::write` (4.6e), the baker
  (4.8) and `.ocioz` archives (4.9), for the tests that use them.
- Phase 5: dynamic properties, the grading transforms and their data, the app helpers (6.7).
- Phase 9: `ConfigUtils` and config merging, which 9 of the 384 tests reach (P6-11).

---

## WP 6.1: the module, its build and the suite (`ocio-py`, `xtask`)

| Chunk | Upstream | Rust | Upstream Python tests |
|---|---|---|---|
| 6.1a | `PyOpenColorIO.cpp` (83): the module, `__author__`, `__email__`, `__license__`, `__copyright__`, `__version__`, `__status__`, `__doc__`, the global functions (`ClearAllCaches`, `GetVersion`, `GetVersionHex`, the logging level, `LogMessage`, the environment variables), the two exceptions; `PyOpenColorIO.h` (181); `package/__init__.py` (39) without its DLL search, which the port's module doesn't need. `cargo xtask py-build`: the extension built with cargo and laid out as upstream's package (`target/py/PyOpenColorIO/__init__.py` and the extension), installed into `target/py-port` (P6-3, P6-4) | `ocio-py/src/py_open_color_io.rs`, `ocio-py/python/PyOpenColorIO/__init__.py`, `xtask/src/py.rs` | `OpenColorIOTest.py` (2) |
| 6.1b | The suite runner: `cargo xtask py-suite` (each test module through unittest's loader in `target/py-port`, per-test outcomes in `target/py-suite.json`), the list of passing tests and its ratchet, the Python count in `docs/parity.md`, a CI step on both platforms | `xtask/src/py.rs`, `tools/py-suite/run.py` | — |
| 6.1c | The embedding API (P6-9): registering the module with `append_to_inittab` before the interpreter starts, and handing Rust objects to Python and back; a Rust test that embeds the interpreter and runs Python against objects the Rust side made | `ocio-py/src/embed.rs` | — |

- `__version__` is `2.5.2` and `GetVersion()` returns it, as everywhere in the port (PLAN.md §2).
- `package/command_line.py` runs the wheel's `bin/` tools: it is `n/a` until the tools come
  (Phase 10).

## WP 6.2: enums and constants (`ocio-py`)

| Chunk | Upstream | Rust | Upstream Python tests |
|---|---|---|---|
| 6.2a | pybind11's `enum_` as the wheel shows it: the class, `export_values` into the parent scope, `__members__`, `name`, `value`, `__int__`, `__index__`, `__eq__`, `__ne__`, `__hash__`, `__repr__`, `__str__`, `__getstate__` and `__setstate__`, conversion from `int` where pybind11 allows it | `ocio-py/src/enums.rs` | — (O6.1, O6.2) |
| 6.2b | `PyTypes.cpp` (825): 31 enums, the string constants, the module's string-conversion functions (`BoolToString`, `TransformDirectionFromString` and the others); the enums of `PyGpuShaderCreator.cpp` (2) | `ocio-py/src/py_types.rs` | `ConstantsTest.py` (1) |

- The 34th enum, `ConfigMergingParameters.MergeStrategies` (`apphelpers/PyMergeConfigs.cpp`),
  comes with 6.8a. (PLAN.md §10 counts 35; the sources declare 34.)

## WP 6.3: classes and iterators (`ocio-py`)

Each chunk binds its classes with their iterator classes (`PyIterator`: `len`, indexing,
iteration, `IndexError` "Iterator index out of range"), their properties, `__repr__` and
`__deepcopy__`, and runs their O6.1 and O6.2 sweeps.

| Chunk | Upstream | Rust (`ocio-py/src/`) | Upstream Python tests |
|---|---|---|---|
| 6.3a | `PyTransform.cpp/.h` (79): the `Transform` base and its polymorphic returns (`polymorphic_type_hook`, `PyOpenColorIO.h`); `transforms/PyGroupTransform.cpp` (150; `write` with 4.6e), `PyAllocationTransform.cpp` (63), `PyMatrixTransform.cpp` (146), `PyRangeTransform.cpp` (85) | `py_transform.rs`, `transforms/` | `TransformsBaseTest.py` (1), `TransformsTest.py` (2), `GroupTransformTest.py` (9), `AllocationTransformTest.py` (6), `MatrixTransformTest.py` (2), `RangeTransformTest.py` (3) |
| 6.3b | `transforms/PyExponentTransform.cpp` (54), `PyExponentWithLinearTransform.cpp` (75), `PyLogTransform.cpp` (35), `PyLogAffineTransform.cpp` (109), `PyLogCameraTransform.cpp` (166) | `transforms/` | `ExponentTransformTest.py` (8), `ExponentWithLinearTransformTest.py` (10), `LogTransformTest.py` (7), `LogCameraTransformTest.py` (12) |
| 6.3c | `transforms/PyFixedFunctionTransform.cpp` (59), `PyBuiltinTransform.cpp` (31), `PyBuiltinTransformRegistry.cpp/.h` (118), `transforms/PyCDLTransform.cpp` (144), `transforms/PyFileTransform.cpp` (87) | `transforms/`, `py_builtin_transform_registry.rs` | `FixedFunctionTransformTest.py` (9), `BuiltinTransformTest.py` (5), `BuiltinTransformRegistryTest.py` (4), `CDLTransformTest.py` (20), `FileTransformTest.py` (11) |
| 6.3d | `PyContext.cpp` (224), `PyFormatMetadata.cpp` (177), `PyProcessorMetadata.cpp` (69), `PyFileRules.cpp` (72), `PyViewingRules.cpp` (118) | `py_context.rs`, `py_format_metadata.rs`, `py_processor_metadata.rs`, `py_file_rules.rs`, `py_viewing_rules.rs` | `ContextTest.py` (2), `FormatMetadataTest.py` (3), `FileRulesTest.py` (11), `ViewingRulesTest.py` (6) |
| 6.3e | `PyColorSpace.cpp` (272), `PyColorSpaceSet.cpp` (112), `PyLook.cpp` (66), `PyNamedTransform.cpp` (195), `PyViewTransform.cpp` (139) | `py_color_space.rs`, `py_color_space_set.rs`, `py_look.rs`, `py_named_transform.rs`, `py_view_transform.rs` | `LookTest.py` (10), `NamedTransformTest.py` (17), `ViewTransformTest.py` (8), `ColorSpaceTest.py` (21; `test_processor_to_known_colorspace` with 6.8a) |
| 6.3f | `PyConfig.cpp:1-700` (~700): creation (`CreateRaw`, `CreateFromEnv`, `CreateFromFile`, `CreateFromStream`, `CreateFromBuiltinConfig`, `CreateFromConfigIOProxy`), the current config, serialization, versions, the environment, search paths, roles, color spaces, rules, `validate`; `PyBuiltinConfigRegistry.cpp/.h` (128) | `py_config.rs`, `py_builtin_config_registry.rs` | `ConfigTest.py` (25, with 6.3g and 6.6a), `BuiltinConfigRegistryTest.py` (4) |
| 6.3g | `PyConfig.cpp:700-1390` (~690): displays and views, the virtual display, looks, named transforms, view transforms, the 13 `getProcessor` overloads (through 6.4a), `archive`; `transforms/PyColorSpaceTransform.cpp` (44), `PyDisplayViewTransform.cpp` (58), `PyLookTransform.cpp` (52) | `py_config.rs`, `transforms/` | `ColorSpaceTransformTest.py` (6), `DisplayViewTransformTest.py` (13), `LookTransformTest.py` (6), `OCIOZArchiveTest.py` (6, with 4.9) |
| 6.3h | `PyProcessor.cpp` (102), `PyGPUProcessor.cpp` (21), `PyGpuShaderCreator.cpp/.h` (147), `PyGpuShaderDesc.cpp` (330; its arrays through 6.5a) | `py_processor.rs`, `py_gpu_processor.rs`, `py_gpu_shader_creator.rs`, `py_gpu_shader_desc.rs` | `ProcessorTest.py` (7), `ProcessorCacheTest.py` (1), `GpuShaderDescTest.py` (6) |
| 6.3i | `PyBaker.cpp` (144) | `py_baker.rs` | `BakerTest.py` (2, with 4.8) |

## WP 6.4: overloads, casters and `TypeError` (`ocio-py`)

| Chunk | Upstream | Rust | Upstream Python tests |
|---|---|---|---|
| 6.4a | pybind11's dispatch as the wheel shows it: overloads tried in definition order, first without implicit conversions, then with them; keyword arguments and defaults; the casters of `int`, `float`, `bool`, `std::string` (from `str` and `bytes`), `std::vector` (from lists and tuples), enums, `None` for pointers, and the classes with their bases; the `TypeError` texts of a call no overload accepts and of a constructor; strict UTF-8 for returned strings; `PyIterator` (`PyUtils.h:40-61`) | `ocio-py/src/dispatch.rs`, `py_utils.rs` | — (O6.2; 121 of the suite's checks are `assertRaises(TypeError)`) |

- 6.4a lands before 6.3a: every class uses it. `Config.getProcessor`'s 13 overloads (6.3g) are
  its largest user.

## WP 6.5: NumPy and the buffer protocol (`ocio-py`)

| Chunk | Upstream | Rust | Upstream Python tests |
|---|---|---|---|
| 6.5a | `PyUtils.cpp/.h` (283): the buffer checks and their messages, the dtypes of each `BitDepth` (`bitDepthToDtype`), the channel and size checks; `PyImageDesc.cpp/.h` (76), `PyPackedImageDesc.cpp` (110), `PyPlanarImageDesc.cpp` (166): images over a NumPy buffer that stays alive with them, `getData` as an array over it | `py_utils.rs`, `py_image_desc.rs`, `py_packed_image_desc.rs`, `py_planar_image_desc.rs` | — (O6.2) |
| 6.5b | `PyCPUProcessor.cpp` (207): `apply`, `applyRGB`, `applyRGBA` on buffers and on lists, in place, with the GIL released (`gil_scoped_release`); `transforms/PyLut1DTransform.cpp` (122), `PyLut3DTransform.cpp` (123): `setData` and `getData` | `py_cpu_processor.rs`, `transforms/` | `CPUProcessorTest.py` (14), `Lut1DTransformTest.py` (11), `Lut3DTransformTest.py` (9) |

- The pixels go through the Rust API's processors; the binding copies nothing it doesn't have
  to, and a buffer's layout reaches the image descriptor as upstream's does.

## WP 6.6: Python subclasses and callbacks (`ocio-py`)

| Chunk | Upstream | Rust | Upstream Python tests |
|---|---|---|---|
| 6.6a | `PyConfigIOProxy.cpp` (52): `ConfigIOProxy` subclassed in Python (`PYBIND11_OVERRIDE_PURE`: `getConfigData`, `getLutData`, `getFastLutFileHash`); `SetLoggingFunction`, `ResetToDefaultLoggingFunction`, `SetComputeHashFunction`, `ResetComputeHashFunction` (`PyOpenColorIO.cpp`): Python callables that the library calls, taking the GIL, from any thread | `py_config_io_proxy.rs`, `py_open_color_io.rs` | `ConfigTest.py` `test_config_io_proxy` |

- An exception raised in a Python callback reaches the caller as pybind11 passes it
  (`error_already_set`): through the library to the Python code that called it. O6.2 checks it.

## WP 6.7: Phase 5's classes (`ocio-py`)

| Chunk | Upstream | Rust | Upstream Python tests |
|---|---|---|---|
| 6.7a | `PyGradingData.cpp` (533): the grading values, curves and control points; `PyDynamicProperty.cpp/.h` (145) | `py_grading_data.rs`, `py_dynamic_property.rs` | `GradingDataTest.py` (7) |
| 6.7b | `transforms/PyExposureContrastTransform.cpp` (107), `PyGradingPrimaryTransform.cpp` (65), `PyGradingRGBCurveTransform.cpp` (75), `PyGradingToneTransform.cpp` (65), `PyGradingHueCurveTransform.cpp` (75) | `transforms/` | `ExposureContrastTransformTest.py` (16), `GradingPrimaryTransformTest.py` (8), `GradingRGBCurveTransformTest.py` (8), `GradingToneTransformTest.py` (7), `GradingHueCurveTransformTest.py` (9) |
| 6.7c | `apphelpers/PyColorSpaceHelpers.cpp` (216), `PyDisplayViewHelpers.cpp` (72), `PyLegacyViewingPipeline.cpp` (52), `PyMixingHelpers.cpp` (134) | `apphelpers/` | `ColorSpaceHelpersTest.py` (5), `DisplayViewHelpersTest.py` (3), `LegacyViewingPipelineTest.py` (8), `MixingHelpersTest.py` (5) |

## WP 6.8: Phase 9's classes (`ocio-py`)

| Chunk | Upstream | Rust | Upstream Python tests |
|---|---|---|---|
| 6.8a | `apphelpers/PyMergeConfigs.cpp` (205): `ConfigMergingParameters`, `ConfigMerger`, `MergeConfigs` and the `MergeStrategies` enum; `Config.IdentifyBuiltinColorSpace`, `IdentifyInterchangeSpace` and `GetProcessorFromConfigs` (`PyConfig.cpp`, `ConfigUtils`); `PySystemMonitors.cpp/.h` (68) | `apphelpers/py_merge_configs.rs`, `py_config.rs`, `py_system_monitors.rs` | `MergeConfigsTest.py` (8), `ColorSpaceTest.py` `test_processor_to_known_colorspace` |

- These need `ConfigUtils` (`src/OpenColorIO/ConfigUtils.cpp`, ~1,050) and config merging
  (`src/OpenColorIO/apphelpers/mergeconfigs/`, ~4,350), which PLAN.md places in Phase 9
  (P6-11). `SystemMonitors` has no Python test; it binds what Phase 9 ports.

---

## Upstream tests

| File | Tests | Chunks | Waits |
|---|---:|---|---|
| `ConfigTest.py` | 25 | 6.3f, 6.3g, 6.6a | — |
| `ColorSpaceTest.py` | 21 | 6.3e | `test_processor_to_known_colorspace` (Phase 9: 6.8a) |
| `CDLTransformTest.py` | 20 | 6.3c | — |
| `NamedTransformTest.py` | 17 | 6.3e | — |
| `ExposureContrastTransformTest.py` | 16 | 6.7b | Phase 5 |
| `CPUProcessorTest.py` | 14 | 6.5b | — |
| `DisplayViewTransformTest.py` | 13 | 6.3g | — |
| `LogCameraTransformTest.py` | 12 | 6.3b | — |
| `Lut1DTransformTest.py`, `Lut3DTransformTest.py` | 20 | 6.5b | — |
| `FileTransformTest.py` | 11 | 6.3c | — |
| `FileRulesTest.py` | 11 | 6.3d | — |
| `LookTest.py` | 10 | 6.3e | — |
| `ExponentWithLinearTransformTest.py`, `ExponentTransformTest.py` | 18 | 6.3b | — |
| `GroupTransformTest.py` | 9 | 6.3a | the write tests: 4.6e |
| `FixedFunctionTransformTest.py` | 9 | 6.3c | — |
| `GradingHueCurveTransformTest.py`, `GradingRGBCurveTransformTest.py`, `GradingPrimaryTransformTest.py`, `GradingToneTransformTest.py`, `GradingDataTest.py` | 39 | 6.7a, 6.7b | Phase 5 |
| `ViewTransformTest.py` | 8 | 6.3e | — |
| `MergeConfigsTest.py` | 8 | 6.8a | Phase 9 (P6-11) |
| `LegacyViewingPipelineTest.py`, `ColorSpaceHelpersTest.py`, `DisplayViewHelpersTest.py`, `MixingHelpersTest.py` | 21 | 6.7c | Phase 5 |
| `ProcessorTest.py`, `ProcessorCacheTest.py`, `GpuShaderDescTest.py` | 14 | 6.3h | — |
| `LogTransformTest.py` | 7 | 6.3b | — |
| `ViewingRulesTest.py` | 6 | 6.3d | — |
| `OCIOZArchiveTest.py` | 6 | 6.3g | `.ocioz` archives: 4.9 |
| `LookTransformTest.py`, `ColorSpaceTransformTest.py` | 12 | 6.3g | — |
| `AllocationTransformTest.py` | 6 | 6.3a | — |
| `BuiltinTransformTest.py`, `BuiltinTransformRegistryTest.py` | 9 | 6.3c | — |
| `BuiltinConfigRegistryTest.py` | 4 | 6.3f | — |
| `RangeTransformTest.py`, `MatrixTransformTest.py`, `TransformsTest.py`, `TransformsBaseTest.py` | 8 | 6.3a | — |
| `FormatMetadataTest.py`, `ContextTest.py` | 5 | 6.3d | — |
| `OpenColorIOTest.py` | 2 | 6.1a | — |
| `BakerTest.py` | 2 | 6.3i | the baker: 4.8 |
| `ConstantsTest.py` | 1 | 6.2b | — |

384 tests: 299 with the bindings of the library already landed (or landing in Phase 4), 76
with Phase 5's, 9 with Phase 9's (P6-11). A test counts once it passes; a chunk's tests may
need a later chunk too (a `ConfigTest` test that builds a processor needs 6.3h).

## Oracle support (owner-reviewed chunks)

Each command lives in its own module under `oracle/ocio_oracle/` and reports what the wheel's
module does; none computes an expected value. The port's side runs the same request in
`target/py-port` through `ocio-testkit` (`py_port.rs` and a runner script), so both sides are
the same Python code over two modules.

| Chunk | Command (module) | What |
|---|---|---|
| O6.1 | `py_surface` (`py_surface_api.py`) | The module's surface, recursively: each name's kind (module, class, function, method, static method, property, enum, enum member, constant), `__module__`, `__qualname__`, `__doc__`, bases and MRO, enum members with their values in order, property accessors, and the constants' values. A committed fixture group (`cargo xtask oracle regen py_surface`) where `oracle check-all` shows it the same on both platforms |
| O6.2 | `py_eval` (`py_eval_api.py`) | Runs a script of steps (`import PyOpenColorIO as OCIO`, then calls on names and on the results of earlier steps), and reports each step's result: its type, `repr`, value (numbers by their bits, strings and bytes as bytes, arrays as dtype, shape and bytes, objects through a given list of getters), or the exception's type, MRO and message; the log; `id()` equalities between results (aliasing, P6-8) |
| O6.3 | `py_suite` (`py_suite_api.py`) | Upstream's suite against the wheel, as 6.1b runs it against the port: each test's outcome and message. The baseline of 6.1b's list, per platform |

- O6.1–O6.3 land first, in card `p6-oracle`, and are reviewed by a verifier agent each (P6-12).
- Scripts that read files use O4.2 (`with_files`).

## Owner decisions

**Open.** Each item has a recommendation; the cards that need it wait for it.

| # | Decision | Recommendation |
|---|---|---|
| P6-1 | **PyO3** (`dependency`) | **`pyo3`, at the version PLAN.md §9 names (0.29), pinned exactly (`=0.29.x`) in 6.1a**, with `extension-module` for the module and `auto-initialize` for 6.1c's embedding test only. 6.1a picks the latest 0.29 patch that builds with Rust 1.98.1 and Python 3.13 on both platforms, and reports it. No other crate of the PyO3 family (no `pyo3-log`, no `pyo3-build-config` overrides). The alternative is binding through the CPython C API by hand: far more `unsafe` and code, for no gain |
| P6-2 | **NumPy** (`dependency`) | **The `numpy` crate, pinned to the version that matches `pyo3` (0.29, PLAN.md §9).** Upstream returns NumPy arrays in 11 places (`py::array`: Lut1D and Lut3D data, image descriptors' data, the GPU shader's uniforms and textures) and reads Python buffers (`py::buffer`, 55 uses) in the image descriptors, the CPU processor, the LUTs and the GPU shader. Inputs go through the buffer protocol (`pyo3`'s `PyBuffer`), as upstream's do, so any buffer works, as with the wheel; the crate makes the arrays. Like pybind11, it imports NumPy only when an array is made, so `import PyOpenColorIO` works without NumPy. The alternative: no crate, calling `numpy.frombuffer` through Python, which saves a dependency and costs more glue |
| P6-3 | **The build tool** (`dependency`) | **No new tool for Phase 6: `cargo xtask py-build` builds the extension with cargo (`cdylib`) and lays it out as upstream's package.** That is all the suite and the oracle sweeps need. The wheel itself (metadata, tags, `RECORD`) waits for the release (Phase 10), where `maturin` (1.15, PLAN.md §9) is the likely choice, decided then |
| P6-4 | **The port's Python environment** (`dependency`; it downloads) | **A second uv project, `py-port/` (Python 3.13, `numpy==2.3.5`, no `opencolorio`), locked like `oracle/`.** It uses the same interpreter and NumPy as the oracle, so uv takes them from its cache where the oracle already put them; creating it on a new machine downloads them from PyPI, as the oracle's does. It needs the owner's approval as a download, once |
| P6-5 | **Python versions and ABI** | **CPython 3.13 only, with its own ABI (`cp313`), not `abi3`.** It is the oracle's version and Ultravioleta's (VFX Reference Platform CY2027), and the wheel's ABI is per version too. `abi3` would limit what the module can use (the buffer protocol is in the limited API only from 3.11, and some of `pyo3`'s faster paths are not in it) for a portability no consumer needs now. The free-threaded build (3.13t) is not supported, as the wheel's isn't. Other versions are a release question (Phase 10) |
| P6-6 | **Names and coexistence** (`api`) | **Import name `PyOpenColorIO` (D13); the package laid out as upstream's (`PyOpenColorIO/__init__.py` re-exporting the extension `PyOpenColorIO/PyOpenColorIO.*`), so `PyOpenColorIO.PyOpenColorIO` exists as in the wheel; distribution name `opencolorio-rs` (never `opencolorio`), version `0.5.0+ocio.2.5.2` at M4.** The port's module and the wheel never share an environment (P6-4): tests reach the wheel only through the oracle |
| P6-7 | **What "exact" covers in Python** (`deviation` for what it leaves out) | **Every deterministic text exact, pybind11's included:** OCIO's messages and reprs, enum `repr` and `str`, `__doc__` with pybind11's signature lines, `__module__` and `__qualname__`, and the `TypeError`s of rejected calls ("incompatible function arguments. The following argument types are supported: 1. (self: PyOpenColorIO.Config, ...) ... Invoked with: ..."). They come from one table of signatures, which the binding needs anyway. **One deviation:** the default `repr` of the classes without `__repr__` (`<PyOpenColorIO.X object at 0x...>`) prints the port's own address. The cheaper alternative: exact types for pybind11's errors but not their texts, as a deviation |
| P6-8 | **Aliasing: which returned objects share their owner's** (`api`) | **Mirror the wheel, getter by getter, as O6.2 shows it** (a change through the returned object, then the owner's getters). Where the wheel shares and the Rust API returns a copy, the Rust API gains a shared handle (as `CurrentContext` and `ConfigRules`), each listed in its chunk as an `api` item. Where the wheel copies, the binding copies |
| P6-9 | **The embedding API** (`api`) | **`ocio_py::append_to_inittab()` (registers `PyOpenColorIO` before `Py_Initialize`), and conversions of the shared Rust types (`Arc<Config>`, `Arc<Processor>`, the CPU and GPU processors, transforms) to Python objects and back that share the Rust object, so a host and its Python code see the same config.** Nothing else public; the binding's classes stay private to `ocio-py` |
| P6-10 | **`unsafe` in `ocio-py`** | **Allowed (D8), but only in `ocio-py/src/buffer.rs`:** the views of Python buffers as slices while the GIL is released, with a `SAFETY` comment each. The rest of the crate keeps `#![deny(unsafe_code)]`, and `cargo xtask guards` checks it |
| P6-11 | **Phase 9's parts that the suite reaches** | **Pull `ConfigUtils` (~1,050) and config merging (~4,350) forward, as two library cards of their own (`p9-config-utils`, `p9-merge`), so that M4 is the whole suite, as PLAN.md says.** 9 tests need them: `MergeConfigsTest.py` (8) and `ColorSpaceTest.py`'s `test_processor_to_known_colorspace`. The alternative: M4 at 375 of 384, the 9 waiting for Phase 9, and "the suite passes unmodified" at M5 |
| P6-12 | **Oracle chunks O6.1–O6.3** (`oracle`) | Reviewed by a verifier agent each, as since 2026-10-05 |

## Platform risks

- **Embedding.** The embedding test (6.1c) links the interpreter: `python313.dll` on Windows
  (found through uv's installation, never through `PATH`), `libpython3.13.so` in Rocky Linux 9.
  The extension module itself links neither on Linux.
- **Strings and paths.** pybind11 encodes `str` arguments as UTF-8 and decodes returned
  `std::string`s as strict UTF-8, so a name with invalid UTF-8 raises `UnicodeDecodeError` when
  read from Python, on both platforms (O6.2 checks it on the wheel). Paths are bytes in the
  Rust API (D4); `os.PathLike` is not accepted where the wheel doesn't accept it.
- **The environment.** `SetEnvVariable` and `UnsetEnvVariable` change the process environment
  as upstream's `Platform::Setenv` and `Unsetenv` do; Python's `os.environ`, a copy made at
  start-up, doesn't see the change, as with the wheel. Tests that set variables change the
  suite's process, so 6.1b runs each test module in a fresh one.
- **The GIL.** `apply` releases it; a logging or hash callback written in Python takes it back
  on the library's thread (6.6a). A callback that waits for a thread holding the GIL deadlocks,
  as with the wheel.
- **Floats in Python.** Python prints floats itself (`repr` of a `float` is the shortest
  round-trip on both platforms); OCIO's text goes through the port's `cfmt`.
- **NumPy.** `float16` arrays reach the library as `half` bits; NumPy's version is pinned on
  both sides (P6-4).

## Order and parallelism

```
p6-oracle (O6.1-O6.3) ─────────────── oracle checks of every card below
p6-skeleton (6.1a-c) ── p6-core (6.2a-b, 6.4a) ─┬─ p6-transforms (6.3a-c) ──────────┐
                                                 ├─ p6-objects (6.3d-e) ─ p6-config (6.3f-g, 6.6a)
                                                 └─ p6-buffers (6.5a-b) ─ p6-processors (6.3h) ┤
Phase 4 (4.6e, 4.8, 4.9) ── p6-baker (6.3i) and the archive and write tests ─────────────────────┤
Phase 5 ── p6-after-p5 (6.7a-c) ─────────────────────────────────────────────────────────────────┤
p9-config-utils, p9-merge (P6-11) ── p6-after-p9 (6.8a) ─────────────────────────────────────────┤
all of the above ── p6-m4-parity: the suite through OpenColorIOTestSuite.py, the full O6.1 and O6.2 sweeps ── M4
```

- **Implementer B (the Python track, PLAN.md §10):** `p6-oracle` → `p6-skeleton` → `p6-core` →
  `p6-transforms` → `p6-objects` → `p6-config` → `p6-buffers` → `p6-processors`.
- **A second implementer**, when free, takes `p6-buffers` and `p6-processors` after `p6-core`,
  and the Phase 9 library cards (P6-11).
- **Verifier:** reviews every card before it lands, with mutation testing (a caster rule, an
  overload's order, an enum's `__eq__`, a buffer check).

## Cards

Each card is one branch and one PR. "Now" means the card can start once its owner decisions
are made, without waiting for other Phase 6 cards.

| Card | Chunks | Size | Who | Needs | Owner items |
|---|---|---|---|---|---|
| `p6-oracle` | O6.1–O6.3, each its own chunk | 3 | B | now | `oracle` (P6-12) |
| `p6-skeleton` | 6.1a–6.1c | 3, ~700 | B | now | `dependency` (P6-1, P6-3, P6-4, P6-5), `api` (P6-6, P6-9), P6-10 |
| `p6-core` | 6.2a, 6.2b, 6.4a | 3, ~1,800 | B | `p6-skeleton` | P6-7 |
| `p6-transforms` | 6.3a–6.3c | 3, ~1,300 | B | `p6-core` | P6-8 |
| `p6-objects` | 6.3d–6.3e | 2, ~1,450 | B | `p6-core` | P6-8 |
| `p6-config` | 6.3f, 6.3g, 6.6a | 3, ~1,700 | B | `p6-objects` | P6-8 |
| `p6-buffers` | 6.5a–6.5b | 2, ~1,000 | B or second | `p6-core` | `dependency` (P6-2) |
| `p6-processors` | 6.3h | 1, ~600 | B or second | `p6-buffers` | — |
| `p6-baker` | 6.3i | 1, ~150 | B | `p6-config`, 4.8 | — |
| `p6-after-p5` | 6.7a–6.7c | 3, ~1,600 | B | Phase 5's library cards | — |
| `p9-config-utils`, `p9-merge` | Phase 9's `ConfigUtils` and config merging, pulled forward (their chunks in a Phase 9 card file) | ~9 | second | Phase 3 | P6-11 |
| `p6-after-p9` | 6.8a | 1, ~300 | B | `p9-config-utils`, `p9-merge`, `p6-config` | — |
| `p6-m4-parity` | The suite through `OpenColorIOTestSuite.py` on both platforms; every class's O6.1 and O6.2 sweeps complete | 1–2 | any | every card above | — |

12 Phase 6 cards, about 27 chunks. When a card grows past about 6 chunks, it splits at a
dependency boundary.

**Can start now:** `p6-oracle`, then `p6-skeleton` once P6-1 and P6-3 to P6-6 are decided.

**Waits for other phases:**
- **Phase 4:** `GroupTransform::write` (4.6e), the baker (4.8), `.ocioz` archives (4.9).
- **Phase 5:** dynamic properties, the grading transforms, the app helpers.
- **Phase 9:** `ConfigUtils` and config merging, unless pulled forward (P6-11).
- **Phase 10:** the wheel's packaging and the `bin/` tools.

## Phase 6 exit (M4)

On Windows and Rocky Linux 9, with the port's module in `target/py-port`:
- upstream's Python suite passes unmodified: all 384 tests, run by
  `tests/python/OpenColorIOTestSuite.py` itself, except any that fail against the wheel too
  (O6.3), each listed with the wheel's message;
- the module's surface matches the wheel's (O6.1), name by name;
- every method's O6.2 sweep matches: values, aliasing, exception types and texts, and
  pybind11's texts (P6-7);
- a Rust program embeds the module and shares its config with Python code (6.1c).
