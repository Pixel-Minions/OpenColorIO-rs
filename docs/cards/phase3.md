# Phase 3 cards: config, transforms and processors

**Status: planned, 2026-10-05.** Phase 3 runs in parallel with Phase 2 (LUTs, fixed functions,
ACES 2.0), by the owner's decision of 2026-10-05.

**Goal: milestone M1, together with Phase 2** (PLAN.md §1, §10). Phase 3 brings OpenColorIO's
config to the port:
- the YAML reader (a port of yaml-cpp 0.8.0's parser) and OCIO's config loader, v1 and v2;
- the config model: color spaces, roles, displays and views, view transforms, looks, named
  transforms, the context, file rules and viewing rules;
- `validate()`, with its messages verbatim;
- the byte-exact YAML writer and the config cache ID;
- the color-space, display-view, look, named-transform and built-in transforms, with their op
  builders;
- the `getProcessor` overloads, and loading from a string, a file, `$OCIO`, an `ocio://` URI or a
  built-in config.

Everything that doesn't need Phase 2's ops comes first. The parts that do are marked **P2**
and gathered in late cards. A few parts wait for later phases: **P4** (file formats), **P5**
(grading and exposure-contrast ops) and **P9** (ConfigUtils).

Each work package below is a list of **chunks**. A chunk is one commit and is mergeable on its
own (`CLAUDE.md` → "Chunks"). Line counts are upstream's non-blank, non-comment lines at v2.5.2.
Paths are relative to `src/OpenColorIO/` unless they start with `tests/`. yaml-cpp and pystring
counts are estimates until their sources are pinned (owner item D3).

## How this phase works

The rules are in `CLAUDE.md`; this is where each one applies in Phase 3.

- **Cards and PRs.** As in Phase 1: one branch `card/<id>` per card, one PR per card, chunks
  committed by the implementer and landed by the orchestrator with `cargo xtask land`.
- **The chunk gate.** `cargo xtask gate --staged` before every commit. These chunks are
  platform-sensitive and use `gate --release --rocky`:
  - the YAML parser and its scalar conversions (iostream number parsing differs between the
    wheels);
  - the regex engine (`std::regex` differs between MSVC and libstdc++);
  - paths, the environment and file hashes;
  - the YAML writer, `operator<<` texts and cache IDs;
  - every chunk that adds a processor path (pixels).
- **Oracle first.** The oracle commands of "Oracle support" land first, each in its own chunk,
  labelled `oracle` and reviewed by the owner. A card may port upstream's tests before its oracle
  command lands, but it lands its oracle checks only after.
- **Testing a config.** A config is checked three ways against the wheel:
  1. its state: every getter, through `config_calls` (O3.1) and its `dump`;
  2. its text: `serialize()` and `getCacheID()` byte for byte (from WP 3.7 on);
  3. its processors: CPU pixels at every bit depth, layout and optimization level, and GPU text
     in all 10 languages, through the battery or `config_processor` (O3.5), with the same
     config and the same `getProcessor` overload on both sides.
- **Messages are output.** Every exception and warning text is compared byte for byte, including
  the line numbers that yaml-cpp reports and the texts it builds itself.
- **The environment is injected.** Configs read `OCIO_ACTIVE_DISPLAYS`, `OCIO_ACTIVE_VIEWS` and
  `OCIO_INACTIVE_COLORSPACES` when they are created, and a context without an `environment:`
  section loads the whole environment into its cache ID. Port tests go through
  `ocio_ops::platform`'s provider and never change the process's environment. The oracle sets
  variables in its own process for the duration of a call.
- **Strings are bytes** (`docs/architecture.md`). Names, descriptions, paths, rules and
  environment values are bytes that end at the first NUL, never Rust `String`.
- **Owner items** stop at the owner and carry a label: oracle changes (`oracle`), waivers,
  deviations, public API shape (`api`), new dependencies and new third-party sources
  (`dependency`).

**Where Phase 3 meets Phase 2.** An op family lands whole in its own phase, transform and glue
included. So:
- `FixedFunctionTransform` and its `BuildFixedFunctionOp` come with WP 2.3, and `Lut3DTransform`
  and `BuildLut3DOp` with WP 2.2. Phase 3 only adds their arms to the YAML reader and writer
  (fixed functions only: OCIO has no YAML for LUT transforms), to `validate()`'s version checks
  and to the config tests (card `p3-after-p2`).
- The built-in transforms are Phase 3's (WP 3.2). The registry lists all 98 from the start, so
  built-in configs load, validate and serialize without Phase 2. An entry whose ops are not
  ported yet returns a "not ported yet" error when its ops are built, as Phase 1 did for the
  baked-LUT GPU processor. Those entries get their ops in `p3-after-p2`.
- `GradingRGBCurve`'s B-spline, which the ACES 1.x output transforms use, is WP 2.4's.

**Code already on `main`.**
- `crates/ocio`: the 11 Phase 1 transform classes (Allocation, CDL without files, Exponent,
  ExponentWithLinear, Group, Log, LogAffine, LogCamera, Lut1D, Matrix, Range), each with
  validate, equality and `Display` (`operator<<`).
- `Config`: `create_raw()` (built directly, not parsed), the version, `set_major_version`, the
  current context, the processor cache with its flags and fallback, and `processor`,
  `processor_in_direction` and `processor_with_context` for a transform.
- `Context`: no state yet. `collect_context_variables` knows only the Phase 1 classes, and
  `processor_with_context` returns "the cache ID of a context is not ported yet" when a transform
  uses context variables.
- `Processor`, `CpuProcessor`, `GpuProcessor`, `create_group_transform`, `ProcessorMetadata`;
  `caching.rs` (`std::hash` per platform).
- `crates/ocio/src/yaml_cpp/`: the yaml-cpp 0.8.0 **emitter** (spike S1). Test code replays
  OCIO's writer with it (`tests/common/ocio_writer.rs`) and re-emits the 8 built-in configs
  byte for byte (`tests/s1_builtin_configs.rs`). `exp.rs` and `regex_yaml.rs` are shared by
  yaml-cpp's scanner, so the parser reuses them.
- `ocio_ops`: `cfmt` (C and iostream formatting), `number_utils` (`from_chars`, both wheels'
  branches), `string_utils`, `hash_utils` (XXH3 cache IDs), `platform` (`getenv`,
  `strcasecmp` per deviation D-4, an injectable `EnvProvider`), `logging`.
- Oracle: `config_serialize` (YAML to `serialize()` and cache ID), `serialize_built_config`,
  `builtin_config_names`, `builtin_config_source`, `transform_text`, `processor_ops`,
  `cpu_apply`, `image_apply`, `gpu_shader`. Fixture groups `builtin_configs` (each built-in
  config's `serialize()`, cache ID and log) and `yaml_emitter`.

---

## WP 3.1: the remaining transform classes (`ocio`)

Twelve classes remain of 23. Phase 3 ports five here. The others belong to their op families:
FixedFunction (WP 2.3), Lut3D (WP 2.2), ExposureContrast and the four Grading classes (Phase 5;
owner item D5).

| Chunk | Upstream | Rust (`ocio/src/...`) | Port tests |
|---|---|---|---|
| 3.1a | `transforms/ColorSpaceTransform.cpp`: the class (121), `DisplayViewTransform.cpp`: the class (136), `LookTransform.cpp`: the class (154) | `transforms/color_space_transform.rs`, `display_view_transform.rs`, `look_transform.rs` | `ColorSpaceTransform_tests.cpp` `basic`, `DisplayViewTransform_tests.cpp` `basic`; oracle `transform_text` |
| 3.1b | `transforms/BuiltinTransform.cpp/.h` (100), `transforms/builtins/BuiltinTransformRegistry.cpp/.h` (171): the registry, every entry's name and description in upstream's order (each builtin file's `RegisterAll`, the ACES 2.0 table included), the op creators as "not ported yet" | `transforms/builtin_transform.rs`, `transforms/builtins/builtin_transform_registry.rs` | `BuiltinTransform_tests.cpp` `creation`, `access`; `BuiltinTransformRegistry_tests.cpp` `basic`, `aces`; oracle `builtin_transform_names` (O3.6) |
| 3.1c | `transforms/FileTransform.cpp`: the class only (148): source, CCC id, CDL style, interpolation, validate, `operator<<`. The format registry, loading and `BuildFileTransformOps` are WP 4.1 | `transforms/file_transform.rs` | `FileTransform_tests.cpp` `basic`, `validate` |

- Each class lands with its `Transform` arms (`transform_type`, `direction`, `validate`,
  `Display`). Its `build_ops` arm is "not ported yet" until its builder lands (WP 3.2).
- `LookTransform_tests.cpp` `basic` uses a fixed function: it lands in `p3-after-p2`.

## WP 3.2: op builders and the built-in transforms (`ocio`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 3.2a | `transforms/ColorSpaceTransform.cpp`: `BuildColorSpaceOps`, the reference-space conversions, data bypass (279); `CollectContextVariables` for color spaces, named transforms and color-space transforms (`ColorSpaceTransform.cpp:396-505`); `NamedTransform::GetTransform` | `transforms/color_space_transform.rs`, `context_variable_utils.rs` | `ColorSpaceTransform_tests.cpp` tests without fixed functions; `NamedTransform_tests.cpp` `static_get_transform` |
| 3.2b | `transforms/LookTransform.cpp`: `BuildLookOps`, look options and fallback, `CollectContextVariables` (234) | `transforms/look_transform.rs` | `LookTransform_tests.cpp` `inverse_look_transform`; `Config_tests.cpp` `look_is_noop` |
| 3.2c | `transforms/DisplayViewTransform.cpp`: `BuildDisplayOps`, view transforms, looks, data bypass, `CollectContextVariables` (310) | `transforms/display_view_transform.rs` | `DisplayViewTransform_tests.cpp` `build_ops_with_looks`, `config_load`, `apply_fwd_inv` |
| 3.2d | `Config.cpp:4679-4790`: the ten `getProcessor` overloads by names, color spaces, display and view, and named transform (110); the used context's cache ID in the processor cache key (`Config.cpp:4791-4880`), with its search path, working dir and I/O proxy; `ProcessorMetadata`'s files and looks | `config.rs`, `processor.rs` | `Config_tests.cpp` group D (below); `NamedTransform_tests.cpp` `named_transform_processor`, `inactive_named_transforms`; oracle `config_processor` (O3.5) |
| 3.2e | `transforms/builtins/ColorMatrixHelpers.cpp/.h` (350), `OpHelpers.cpp/.h` (118), `BuildBuiltinTransformOps` and the registry's identity entry | `transforms/builtins/color_matrix_helpers.rs`, `op_helpers.rs` | `BuiltinTransform_tests.cpp` `color_matrix_helpers` |
| 3.2f | `transforms/builtins/ArriCameras.cpp` (100), `RedCameras.cpp` (106), `SonyCameras.cpp` (126), `PanasonicCameras.cpp` (73): Log and Matrix ops only | `transforms/builtins/{arri,red,sony,panasonic}_cameras.rs` | battery `BuiltinTransform` family over these entries, both directions |
| 3.2g | `ACES.cpp` and `Displays.cpp` entries built only from Phase 1 ops (Matrix, Log, Gamma, Range, Exponent) | `transforms/builtins/aces.rs`, `displays.rs` | battery over these entries |

- Each builder chunk ports its `CollectContextVariables` overload. `FileTransform`'s overload
  (`FileTransform.cpp:224-312`, 79) lands with 3.2a, since it needs only the context.
- The rest of the built-ins wait for Phase 2 (card `p3-after-p2`):
  - `Displays.cpp`: PQ, HLG, gamma-log and the Rec.2100 surround (fixed functions, WP 2.3);
  - `ACES.cpp`: the ACES 1.x output transforms (fixed functions and the B-spline curves of
    WP 2.4) and the ACES 2.0 output transforms (WP 2.4);
  - `CanonCameras.cpp` and `AppleCameras.cpp` (fixed functions);
  - the entries built on `OpHelpers` LUTs: their op data exists, but their pixels at float
    input need WP 2.1's float renderers.

## WP 3.3: the YAML reader (`ocio`)

**The parser: a port of yaml-cpp 0.8.0** (owner item D1). It sits next to the emitter in
`crates/ocio/src/yaml_cpp/`, with the same file-for-file layout and citations. yaml-cpp's own
tests at 0.8.0 are copied verbatim, as the emitter's were.

| Chunk | Upstream (yaml-cpp 0.8.0) | Rust (`ocio/src/yaml_cpp/...`) | Port tests |
|---|---|---|---|
| 3.3a | `stream.cpp`, `streamcharsource.h`, `stringsource.h` (~400): BOM and UTF-8/16/32 detection, lenient decoding (an overlong `C0 80` becomes NUL); `mark.h`; `exceptions.h/.cpp`: the `ErrorMsg` texts and `what()`'s "yaml-cpp: error at line L, column C: ..." | `stream.rs`, `exceptions.rs` | `test/integration/encoding_test.cpp` |
| 3.3b | `scanner.cpp/.h`, `simplekey.cpp`, `token.h`, `scantag.cpp`: indentation, flow levels, simple keys (~550) | `scanner.rs`, `simple_key.rs`, `token.rs` | the scanner parts of `test/integration/` |
| 3.3c | `scantoken.cpp`, `scanscalar.cpp/.h`, `tag.cpp`, `directives.cpp` (~600): every token, plain, quoted, literal and folded scalars, verbatim tags (`!<ColorSpace>`) | `scan_token.rs`, `scan_scalar.rs`, `tag.rs` | as 3.3b |
| 3.3d | `parser.cpp`, `singledocparser.cpp`, `collectionstack.h`, `depthguard.cpp` (~550): events, anchors and aliases, the depth limit | `parser.rs`, `single_doc_parser.rs` | `test/integration/handler_test.cpp`, `handler_spec_test.cpp`, `error_messages_test.cpp` |
| 3.3e | `nodebuilder.cpp`, `node/detail/node_data.cpp`, `node/impl.h`, `node/detail/impl.h`, iterators, `parse.cpp` (`Load`) (~600): the node graph, `Mark()`, `Tag()`, `Type()`, map lookups by key | `node.rs`, `node_builder.rs`, `parse.rs` | `test/node/node_test.cpp`, `test/integration/load_node_test.cpp` |
| 3.3f | `include/yaml-cpp/node/convert.h`, `convert.cpp` (~250): bool spellings, null, `as<double>`, `as<float>`, `as<int>`, `as<unsigned>` through `std::stringstream >> std::noskipws` after `unsetf(std::ios::dec)`, then `.inf`/`.nan`; the C++ `num_get` of each wheel (MSVC STL, libstdc++); `std::stoi` (OCIO's profile version) | `convert.rs`; `ocio-ops/src/utils/num_get.rs` | yaml-cpp's conversion tests; oracle `yaml_scalars` (O3.3), both platforms |

- The yaml-cpp file names and counts above are from memory of 0.8.0. The card's first chunk
  checks them against the pinned source (D3) and corrects this table.
- OCIO uses only `YAML::Load` and the node API, so the parser stops at nodes: no `LoadAll`,
  no event emitter (`emitfromevents.cpp`).

**OCIO's loader:** `OCIOYaml.cpp`'s `load` functions (`save` is WP 3.7) and `Config::Impl::Read`.

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 3.3g | `OCIOYaml.cpp:1-406`: `SanitizeNewlines`, the typed loaders with their messages ("At line N, 'Tag' parsing double failed with: ..."), the enum loaders, `LogUnknownKeyWarning`, `throwError`, `throwValueError`, `CheckDuplicates`, custom keys, interchange attributes (~300) | `ocio_yaml.rs` | oracle `config_calls` (O3.1) with malformed configs |
| 3.3h | The loaders of the Phase 1 transforms: Allocation, CDL, Exponent, ExponentWithLinear, Group, Log, LogAffine, LogCamera, Matrix, Range; the transform dispatch (`OCIOYaml.cpp:3196-3352`) (~560) | `ocio_yaml.rs` | oracle: each transform's spellings, defaults, unknown keys and errors, loaded and compared through `transform_text` |
| 3.3i | The loaders of the 3.1 classes: Builtin, ColorSpace, DisplayView, File, Look transforms (~170) | `ocio_yaml.rs` | as 3.3h |
| 3.3j | `View`, `ColorSpace`, `Look`, `ViewTransform`, `NamedTransform` loaders (`OCIOYaml.cpp:407-506, 3422-4069`) (~400) | `ocio_yaml.rs` | `ColorSpace_tests.cpp` `use_alias`; `NamedTransform_tests.cpp` `named_transform_io` |
| 3.3k | `FileRules` and `ViewingRules` loaders, the v1 rules' upgrade (`OCIOYaml.cpp:4070-4395`) (~180) | `ocio_yaml.rs` | `FileRules_tests.cpp` config tests (below); `ViewingRules_tests.cpp` `config_io` |
| 3.3l | `load(Config)`, part 1 (`OCIOYaml.cpp:4398-4700`): the profile version and its errors, environment, search path (a string split on `:`, even on Windows), roles, luma, displays, views, active lists (~280) | `ocio_yaml.rs` | `Config_tests.cpp` group A |
| 3.3m | `load(Config)`, part 2 (`OCIOYaml.cpp:4700-5032`): color spaces, looks, view transforms, named transforms, rules, inactive spaces, defaults, the config's directory as working dir, `loadEnvironment`; `OCIOYaml::Read` and its "Error: Loading the OCIO profile ..." wrapper; `Config::Impl::Read`; `Config::CreateFromStream` (~290) | `ocio_yaml.rs`, `config.rs` | `Config_tests.cpp` group A; the 8 built-in configs and the corpus load, compared by `dump` |

- The loader of `FixedFunctionTransform` lands in `p3-after-p2`.
- The loaders of ExposureContrast and the Grading transforms (`OCIOYaml.cpp:1192-1333,
  1519-2501`, ~1,000 lines with their savers) wait for Phase 5 (owner item D5). Until then their
  tags give "not ported yet".

## WP 3.4: the config model (`ocio`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 3.4a | `ColorSpace.cpp` (499), `TokensManager.h` (74): names, aliases, family, equality group, description, encoding, bit depth, data, allocation, categories, interop ID, AMF IDs, ICC profile name, interchange attributes, transforms, `operator<<` | `color_space.rs`, `tokens_manager.rs` | `ColorSpace_tests.cpp` `basic`, `alias`, `category`, `interop_id`, `amf_transform_ids`, `icc_profile_name`, `unknown_interchange_attrib` |
| 3.4b | `ColorSpaceSet.cpp` (280), `Look.cpp` (250), `LookParse.cpp/.h` (122) | `color_space_set.rs`, `look.rs`, `look_parse.rs` | `ColorSpaceSet_tests.cpp` (4), `LookParse_tests.cpp` (2) |
| 3.4c | `ViewTransform.cpp` (238), `NamedTransform.cpp/.h` (346) | `view_transform.rs`, `named_transform.rs` | `ViewTransform_tests.cpp` `basic`; `NamedTransform_tests.cpp` `basic`, `alias` |
| 3.4d | `Config::Create()` and `Config::Impl`'s state and constructor, which reads `OCIO_ACTIVE_DISPLAYS`, `OCIO_ACTIVE_VIEWS` and `OCIO_INACTIVE_COLORSPACES` (`Config.cpp:255-466`); `GetVersion`, `LookupEnvironment`, `LookupRole`, `GetFileReferences` (`Config.cpp:99-254`); versions (`setMinorVersion`, `setVersion`, `upgradeToLatestVersion`); name, description, family separator; environment variables and mode; search paths and working dir (`Config.cpp:2110-2295`); the copy (~500) | `config.rs` | `Config_tests.cpp` `version` parts that don't load YAML |
| 3.4e | The upstream tests deferred from Phase 1 that need only `Config::Create()` | — | `Processor_tests.cpp` `basic_cache`, `channel_crosstalk`, `optimized_processor`; `CPUProcessor_tests.cpp` `with_one_matrix`, `one_pixel`, `optimizations`, `planar_vs_packed` and the 7 `scanline_*` tests |
| 3.4f | Color spaces (`Config.cpp:2296-2783`): sets by category, lookups by name, alias or role, canonical name, indices by reference type and visibility, add, remove, `isColorSpaceUsed`, clear; `Impl::getColorSpace`, `hasColorSpace` (~450) | `config.rs` | `Config_tests.cpp` group B, as each test's other parts exist |
| 3.4g | Roles (`Config.cpp:2978-3064`), inactive spaces and `refreshActiveColorSpaces`, `buildInactiveNamesList` (`Config.cpp:5351-5463`), default luma, strict parsing, `isColorSpaceLinear` (`Config.cpp:2784-2907`) (~330) | `config.rs` | as 3.4f |
| 3.4h | `Display.cpp/.h` (148): `View`, `Display`, `ComputeDisplays`; displays and views, shared views (`Config.cpp:3332-3815`) (~450) | `display.rs`, `config.rs` | `Display_tests.cpp` `compare_displays` |
| 3.4i | Virtual display (`Config.cpp:3816-4132`), active displays and views, `getDisplayAll`, temporary displays, the display cache (`updateDisplayCache`) (~510) | `config.rs` | as 3.4f |
| 3.4j | Looks, view transforms and the default view transform, named transforms (`Config.cpp:3065-3331, 4450-4642`) (~370) | `config.rs` | as 3.4f |

- `instantiateDisplayFromMonitorName` and `instantiateDisplayFromICCProfile` need the ICC reader
  (WP 4.7) and `SystemMonitor` (Phase 9). Until then they return "not ported yet".
- `Config::CreateRaw()` switches from the state built directly to parsing upstream's internal
  profile in 3.7d.

## WP 3.5: context, environment and paths (`ocio-ops`, `ocio`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 3.5a | `Platform.cpp`: `Setenv`, `Unsetenv`, `Utf8ToUtf16`, `Utf16ToUtf8`, `CreateFileContentHash` (`st_dev:st_ino` on Linux, `st_dev:std::hash(filename)` on Windows), `CreateInputFileStream`, `CreateTempFilename`; the public `GetEnvVariable`, `SetEnvVariable`, `UnsetEnvVariable`, `IsEnvVariablePresent`; `EnvProvider` grows enumeration (`environ`, `_wenviron`), bytes and Windows' case-insensitive names (~200) | `ocio-ops/src/platform.rs` | the rest of `Platform_tests.cpp` |
| 3.5b | pystring 1.1.4 `os.path`: `join`, `normpath`, `isabs`, `dirname`, `splitext`, in its `nt` variant on Windows and `posix` on Linux, as the wheels compile it; `PathUtils.cpp` (169): `GetCwd`, `AbsPath`, `GetFastFileHash` and its cache, `SetComputeHashFunction`, `FileExists`, `ClearPathCaches` (~400) | `ocio-ops/src/utils/pystring.rs`, `ocio/src/path_utils.rs` | pystring's own `os.path` tests; `PathUtils_tests.cpp` `compute_hash` |
| 3.5c | `ParseUtils.cpp`: `BoolToString`/`FromString`, the `FromString` parsers of the Phase 1 enums, `EnvironmentModeToString`/`FromString`, `StrEqualsCaseIgnore`, `SplitStringEnvStyle`, `JoinStringEnvStyle`, `IntersectStringVecsCaseIgnore`, `FindInStringVecCaseIgnore` (~300). `StringToInt`, `StringToFloat`, `FloatToString` and the XML helpers are Phase 4's | `ocio-ops/src/parse_utils.rs` | `ParseUtils_tests.cpp` `bool_string`, `transform_direction`, `bitdepth`, `split_string_env_style`, `join_string_env_style`, `intersect_string_vecs_case_ignore` |
| 3.5d | `ContextVariableUtils.cpp/.h` (213): `ContainsContextVariables`, `LoadEnvironment`, `ResolveContextVariables` (`$V`, `${V}`, `%V%`, longest name first, 32 levels) | `context_variable_utils.rs` | `ContextVariableUtils_tests.cpp` (2) |
| 3.5e | `Context.cpp`, part 1 (~250): search paths, working dir, environment mode, `loadEnvironment`, string variables, `resolveStringVar` with the used variables, the cache ID, `operator<<`, the I/O proxy slot | `context.rs` | `Context_tests.cpp` `abs_path`, `string_vars`; oracle `context_calls` (O3.2) |
| 3.5f | `Context.cpp`, part 2 (~180): `resolveFileLocation` (absolute paths, search paths, the used context, the `ExceptionMissingFile` texts), `GetAbsoluteSearchPaths` | `context.rs` | `Context_tests.cpp` `search_paths`, `var_search_path`, `use_searchpaths`, `use_searchpaths_workingdir` |

## WP 3.6: processor API and caches (`ocio`)

Most of 3.6 landed in Phase 1 (1.8g, 1.8h). What remains:
- **`Config::Create()`** and the upstream tests deferred to it (3.4d, 3.4e);
- **the `getProcessor` overloads by names**, the used context's cache ID in the processor cache
  key, and the metadata's files and looks (3.2d);
- **`ClearAllCaches`**: its path caches in 3.10b, its file caches in WP 4.1;
- **`Processor` and `CPUProcessor` tests that need more than `Config::Create()`:**
  - `basic_cache_lut` needs `Lut3DTransform` (`p3-after-p2`);
  - `with_one_1d_lut`, `with_several_ops`, `image_desc` and the `Caching_tests.cpp`
    `processor_cache` marker read LUT files (P4);
  - `dynamic_properties`, `is_noop`, `cache_optimized_processors`, `cache_cpu_processors` and
    `cache_gpu_processors` need ExposureContrastTransform (P5), as does the verifier's surviving
    mutant M11 (the CPU cache ignoring dynamic ops);
- **left to later phases:**
  - `GetProcessorFromConfigs`, `GetProcessorToBuiltinColorSpace`,
    `GetProcessorFromBuiltinColorSpace`, `IdentifyInterchangeSpace`,
    `IdentifyBuiltinColorSpace`, and the processor's `setColorSpaceConversion` and `concatenate`
    are ConfigUtils (P9, owner item D6);
  - the legacy GPU processor is Phase 7.

## WP 3.7: the YAML writer and the config cache ID (`ocio`)

The emitter exists. These chunks port OCIO's `save` functions over it, then retire the test-only
writer replay.

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 3.7a | `OCIOYaml.cpp`: `saveDescription`, `EmitBaseTransformKeyValues`, `EmitTransformName`, the savers of every ported transform, the dispatch (`OCIOYaml.cpp:3353-3421`), interchange attributes (~380) | `ocio_yaml.rs` | `Config_tests.cpp` `range_serialization`, `exponent_serialization`, `exponent_with_linear_serialization`, `log_serialization`, `matrix_serialization`, `cdl_serialization`, `file_transform_serialization`, `serialize_group_transform` |
| 3.7b | The savers of `View`, `ColorSpace`, `Look`, `ViewTransform`, `NamedTransform`, `FileRules`, `ViewingRules` (~330) | `ocio_yaml.rs` | `ColorSpace_tests.cpp` `color_space_serialize`, `interop_id_serialization`, `icc_profile_name_serialization`; `FileRules_tests.cpp` `config_rule_customkeys`, `config_rule_u8`, `multiple_rules`, `read_write_incomplete_configs` |
| 3.7c | `save(Config)` (`OCIOYaml.cpp:5033-5419`, 326), `OCIOYaml::Write`, `Config::serialize`, `operator<<(Config)` | `ocio_yaml.rs`, `config.rs` | `Config_tests.cpp` group C; the `builtin_configs` fixtures: each built-in config loaded and serialized is the committed text |
| 3.7d | `Config::getCacheID` with and without a context (`Config.cpp:5245-5316`): the serialization's hash, the file references' fast hashes; `Impl::resetCacheIDs`, `getAllInternalTransforms` (~130); `Config::CreateRaw()` parsed from upstream's internal profile | `config.rs` | `Config_tests.cpp` `internal_raw_profile`; the `builtin_configs` cache IDs; `s1_builtin_configs.rs` and `s1_emitter_edges.rs` read through the real reader and writer |

- 3.7d replaces `tests/common/yaml_tree.rs` and `ocio_writer.rs` with the port's reader and
  writer. The `saphyr-parser` dev-dependency then goes (owner item D7).
- The savers of FixedFunction (`p3-after-p2`), ExposureContrast and Grading (P5) come with
  their loaders.

## WP 3.8: `validate()` (`ocio`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 3.8a | `Config::validate`, part 1 (`Config.cpp:1359-1800`): the cached result, color spaces, interop IDs, roles, the roles 2.2 requires, inactive spaces, displays and active lists (~330) | `config.rs` | `Config_tests.cpp` `validation`, `required_roles_for_version_2_2`, `display`, `config_v1` |
| 3.8b | `Config::validate`, part 2 (`Config.cpp:1800-2108`): transforms and the color spaces they name, looks, view transforms, the default view transform, file rules, viewing rules, named transforms; `Impl::validateView` (`Config.cpp:592-766`) (~380) | `config.rs` | `Config_tests.cpp` group B |
| 3.8c | `Impl::checkVersionConsistency` for each transform and for the config (`Config.cpp:5586-5994`, 359), the built-in styles' versions included | `config.rs` | `Config_tests.cpp` `version_validation`, `transform_versions`; `BuiltinTransformRegistry_tests.cpp` `version_1_validation`, `version_2_validation`, `version_2_1_validation`, `version_2_3_validation` |

- The fixed-function arm of 3.8c lands in `p3-after-p2`; the grading and exposure-contrast arms
  with Phase 5.

## WP 3.9: file rules, viewing rules and the regex engine (`ocio-ops`, `ocio`)

**`std::regex` (ECMAScript), as each wheel's C++ library implements it** (owner item D2). OCIO
calls it for:
- users' regex rules: `ValidateRegularExpression` and `regex_match`;
- glob rules converted to regexes: `BuildRegularExpression` and `regex_match`;
- two fixed `regex_replace` patterns: `SanitizeRegularExpression`;
- the `ocio://` pattern `ocio:\/\/([^\s]+)`: `regex_search` in `Config.cpp` and
  `BuiltinConfigRegistry.cpp`.

The engine works on bytes in the classic locale, and each platform's `regex_error` texts are
part of OCIO's messages ("File rules: invalid regular expression '...': '<what()>'.").

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 3.9a | The ECMAScript grammar of C++ `[re.grammar]`: the parser, its error codes, and each library's `what()` texts (MSVC STL, libstdc++) (~500) | `ocio-ops/src/std_regex/` | oracle `file_rules_match` (O3.4): patterns and their errors, both platforms |
| 3.9b | The matcher: `regex_match`, `regex_search`, `regex_replace` (`format_default`), backtracking, character classes (~500) | `ocio-ops/src/std_regex/` | oracle `file_rules_match`: generated patterns and paths, both platforms |
| 3.9c | `FileRules.cpp:1-536` (460): `SanitizeRegularExpression`, `ConvertToRegularExpression`, `BuildRegularExpression`, `ValidateRegularExpression`, `FileRule` (pattern, extension, regex, color space, custom keys, `matches`); `CustomKeys.h` (82) | `file_rules.rs`, `custom_keys.rs` | `FileRules_tests.cpp` `config_read_only`, `pattern_error`, `with_defaults`, `extension_error`, `clone`, `isDefault` |
| 3.9d | `FileRules.cpp:537-1050` (425): the rule list, the default and `ColorSpaceNamePathSearch` rules, insert, remove, move, validate, `operator<<`; `Config::getFileRules`, `setFileRules`, `getColorSpaceFromFilepath`, `filepathOnlyMatchesDefaultRule`, `parseColorSpaceFromString`; `PathUtils.cpp`'s `ParseColorSpaceFromString` (~470) | `file_rules.rs`, `config.rs`, `path_utils.rs` | `FileRules_tests.cpp` `config_insert_rule`, `rule_move`, `use_alias` |
| 3.9e | `ViewingRules.cpp/.h` (462); `Config::getViewingRules`, `setViewingRules` | `viewing_rules.rs`, `config.rs` | `ViewingRules_tests.cpp` `basic` |

- `FileRules_tests.cpp` has 30 tests. The ones that load YAML land with 3.3k, the ones that
  serialize with 3.7b, the rest here.

## WP 3.10: built-in configs and loading (`ocio`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 3.10a | `builtinconfigs/BuiltinConfigRegistry.cpp/.h` (167), `CGConfig.cpp`, `StudioConfig.cpp` (74), the 8 embedded `.ocio` texts, `ResolveConfigPath` (`ocio://default`, `cg-config-latest`, `studio-config-latest`), `Config::CreateFromBuiltinConfig` | `builtinconfigs/` | `BuiltinConfig_tests.cpp` `basic`, `basic_impl`, `resolve_config_path`, `create_builtin_config`; the registry's text against the wheel's, live, per platform (CRLF on Windows) |
| 3.10b | `Config::CreateFromFile` (`ocio://` URIs, the `PK` signature, whose `.ocioz` reading is P4), `CreateFromEnv` (`$OCIO`, the "Color management disabled" message), `GetCurrentConfig`, `SetCurrentConfig`, `ClearAllCaches` (its path caches) (~150) | `config.rs`, `caching.rs` | `Config_tests.cpp` `test_searchpath_filesystem`; loading `tests/data/files` configs by path |
| 3.10c | `ConfigIOProxy`: the trait, `CreateFromConfigIOProxy`, `setConfigIOProxy`, `getConfigIOProxy`, the context's proxy, `GetFastFileHash`'s proxy branch (~100); `Config::isArchivable` (`Config.cpp:6008-6082`) | `config_io_proxy.rs`, `config.rs`, `context.rs` | proxies that serve configs without LUT files; the archive tests are P4 |

## Upstream tests

**`tests/cpu/Config_tests.cpp`: 89 tests, 10,400 lines.** Split by what each needs. Each card
confirms its share by reading the tests, and moves a test to a later card only with the reason in
`upstream-map.toml`.

| Group | Lands with | Tests |
|---|---|---|
| A, load (9) | 3.3l–3.3m (`p3-yaml-load-2`) | `roles`, `colorspace_duplicate`, `cdltransform_duplicate`, `searchpath_duplicate`, `view`, `key_value_error`, `unknown_key_error`, `faulty_config_file`, `display_color_spaces_errors` |
| B, model and validate (23) | `p3-validate`, or the model chunk that completes the test | `create_raw_config`, `simple_config`, `required_roles_for_version_2_2`, `validation`, `version`, `version_validation`, `categories`, `display`, `display_view_order`, `inactive_color_space`, `is_inactive`, `inactive_color_space_precedence`, `config_v1`, `view_transforms`, `not_case_sensitive`, `look_transform`, `family_separator`, `add_remove_display`, `is_colorspace_used`, `virtual_display_with_active_displays`, `virtual_display_v2_only`, `virtual_display_exceptions`, `alias_validation` |
| C, serialize (21) | `p3-yaml-save` | `internal_raw_profile`, `serialize_group_transform`, `serialize_searchpath`, `serialize_environment`, `serialize_colorspace_displayview_transforms`, `range_serialization`, `exponent_serialization`, `exponent_with_linear_serialization`, `log_serialization`, `matrix_serialization`, `cdl_serialization`, `file_transform_serialization`, `file_transform_serialization_v1`, `active_displayview_lists`, `inactive_color_space_read_write`, `display_color_spaces_serialization`, `display_view`, `transform_versions`, `virtual_display`, `description_and_name`, `interchange_attributes` |
| D, context and processors (14) | `p3-builders` (3.2d), `test_searchpath_filesystem` with 3.10b | `test_searchpath_filesystem`, `context_variable_v1`, `context_variable_faulty_cases`, `context_variable`, `context_variable_unresolved`, `colorspacename_with_reserved_token`, `context_variable_with_role`, `context_variable_with_display_view`, `env_colorspace_name`, `exponent_vs_config_version`, `transform_with_roles`, `config_context_cacheids`, `processor_cache_with_context_variables`, `look_is_noop` |
| E, P2 (5) | `p3-after-p2` | `fixed_function_serialization`, `add_color_space`, `remove_color_space`, `get_processor_alias`, `builtin_transforms` |
| F, P4 (9) | Phase 4 (LUT files, `.ocioz`) | `context_variable_with_sanity_check`, `context_variable_with_colorspacename`, `context_variable_with_search_path_v1`, `context_variable_with_search_path_v2`, `context_variables_typical_use_cases`, `look_fallback`, `create_from_archive`, `create_from_config_io_proxy`, `set_config_io_proxy` |
| G, P5 (7) | Phase 5 | `grading_primary_serialization`, `grading_rgbcurve_serialization`, `grading_huecurve_serialization`, `grading_tone_serialization`, `exposure_contrast_serialization`, `dynamic_properties`, `optimization_with_bitdepths` |
| H, P9 (1) | Phase 9 | `get_processor_from_two_configs` |

So Phase 3 ports 67 of the 89, and `p3-after-p2` 5 more.

**The other test files**, with the tests that wait:

| File | Tests | In Phase 3 | Waits |
|---|---:|---|---|
| `ColorSpace_tests.cpp` | 14 | 11 (3.4a, 3.3j, 3.7b) | `is_colorspace_linear` (P2: Lut3D); `processor_to_known_colorspace`, `processor_to_known_colorspace_alt_config` (P9) |
| `ColorSpaceSet_tests.cpp` | 4 | 4 (3.4b) | — |
| `Context_tests.cpp` | 6 | 6 (3.5e, 3.5f) | — |
| `ContextVariableUtils_tests.cpp` | 2 | 2 (3.5d) | — |
| `Display_tests.cpp` | 3 | 3 (3.4h, then 3.7b and 3.8b) | — |
| `FileRules_tests.cpp` | 30 | 30 (3.9c–d, 3.3k, 3.7b) | — |
| `ViewingRules_tests.cpp` | 3 | 3 (3.9e, 3.3k) | — |
| `LookParse_tests.cpp` | 2 | 2 (3.4b) | — |
| `NamedTransform_tests.cpp` | 9 | 9 (3.4c, 3.2a, 3.2d, 3.3j, 3.8b) | — |
| `ViewTransform_tests.cpp` | 1 | 1 (3.4c) | — |
| `ParseUtils_tests.cpp` | 11 | 6 (3.5c) | the XML, int, float and string-vector tests (P4) |
| `PathUtils_tests.cpp` | 1 | 1 (3.5b) | — |
| `Platform_tests.cpp` | 7 | `envVariable`, `getenv`, `setenv`, `create_temp_filename`, `utf8_utf16_convert` (3.5a); `string_compare` is ported | `aligned_memory_test`, with `AlignedMalloc` (not in Phase 3) |
| `Processor_tests.cpp` | 11 | 3 (3.4e) | as WP 3.6 says |
| `CPUProcessor_tests.cpp` | 16 | 11 (3.4e) | as WP 3.6 says |
| `transforms/ColorSpaceTransform_tests.cpp` | 5 | `basic`, `context_variables` (3.1a, 3.2a) | the build tests use fixed functions (P2) |
| `transforms/DisplayViewTransform_tests.cpp` | 6 | 5 (3.1a, 3.2c) | `build_ops` uses ExposureContrast (P5) |
| `transforms/LookTransform_tests.cpp` | 5 | `inverse_look_transform`, `context_variables` | `basic`, `build_look_ops`, `build_look_options_ops` use fixed functions (P2) |
| `transforms/BuiltinTransform_tests.cpp` | 8 | `creation`, `access`, `color_matrix_helpers` | `forward_inverse`, `interpolate`, `validate`, `aces2_displayview_roundtrip`, `aces2_Aab_to_RGB_nan` (P2) |
| `transforms/builtins/BuiltinTransformRegistry_tests.cpp` | 7 | 6 (3.1b, 3.8c) | `read_write` builds every built-in's processor (P2) |
| `builtinconfigs/BuiltinConfig_tests.cpp` | 4 | 4 (3.10a) | — |
| `transforms/FileTransform_tests.cpp` | 10 | `basic`, `validate` (3.1c) | the rest (P4) |
| `transforms/GroupTransform_tests.cpp` | 3 | — | `basic` (P2: fixed function), `write_formats`, `write_with_noops` (P4) |

About 190 upstream tests land in Phase 3, and about 20 more in `p3-after-p2`.

## Oracle support (owner-reviewed chunks)

Each command lives in its own module under `oracle/ocio_oracle/` and reports what the library
does; none computes an expected value. Strings come back as bytes (hex), so text that is not UTF-8
survives. Each call captures the log.

| Chunk | Command (module) | What |
|---|---|---|
| O3.1 | `config_calls` (`config_api.py`) | A config from a spec: raw, `Config()`, YAML bytes (`CreateFromStream`), a file in a temporary directory built from given files (`CreateFromFile`), a built-in name or `ocio://` URI, or `$OCIO` (`CreateFromEnv`), with environment variables set for the call. Then calls in order on the config and on the objects it returns or the call creates (`ColorSpace`, `Look`, `ViewTransform`, `NamedTransform`, `ColorSpaceSet`, `FileRules`, `ViewingRules`, `Context`, `BuiltinTransformRegistry`): each call's result (enums by name, transforms as their class and `str()`) or exception (type and text). A `dump` call reports everything the config holds, through every getter, plus `serialize()`, `getCacheID()` and `validate()` |
| O3.2 | `context_calls` (`context_api.py`) | A `Context`, alone or a config's, with environment variables and a temporary directory of files: setters, `resolveStringVar` and `resolveFileLocation` with the used context, the cache ID, `str()`, each result or exception |
| O3.3 | `yaml_scalars` (`yaml_scalars.py`) | A batch of scalar spellings, each read as a typed config field (bool, int, float, double, string, string list): the value's bits, or the exception text. For 3.3f's sweeps |
| O3.4 | `file_rules_match` (`file_rules_api.py`) | A batch of rules (glob pattern and extension, or regex): each rule's validation exception, or for each path the rule that matches and its color space (`getColorSpaceFromFilepath` with the rule index). The black-box oracle of the regex engine, per platform |
| O3.5 | `config_processor` (`config_processors.py`) | A processor from a config by any of the 13 `getProcessor` overloads (with or without a context): its cache ID and optimized group (as `processor_ops`), CPU output (as `image_apply`), GPU shader (as `gpu_shader`). It reuses those modules' helpers by import, so no shared file changes |
| O3.6 | `builtin_transform_names` (`builtin_transforms.py`) | The built-in transform registry in order: style and description. Pixels of built-in transforms need no new command: `cpu_apply` takes a `BuiltinTransform` spec |
| O3.7 | Fixture group `config_corpus` (`regen.py`) | For each corpus config (owner item D8) and each `tests/data/files` config: `serialize()`, the cache ID without context, `validate()`'s result and the log, where `oracle check-all` shows them identical on both platforms. Added when the corpus lands |

- O3.1–O3.6 land first, in card `p3-oracle`. O3.7 lands with the corpus.
- Each card's oracle tests batch their calls (`Oracle::batch`).

## Owner decisions needed

| # | Decision | Recommendation |
|---|---|---|
| D1 | **The YAML parser** (`dependency`). PLAN.md §9 picks `saphyr-parser` | **Port yaml-cpp 0.8.0's parser**, next to the emitter port. OCIO's messages embed yaml-cpp's texts and marks ("yaml-cpp: error at line 3, column 5: ...", "At line N, ..."). Its accepted syntax, lenient decoding (the overlong NUL), tags and number conversions are part of what configs the wheel accepts. A crate would need a compatibility layer as large as the port, and would still differ at the edges. ~3,000 lines, plus yaml-cpp's own tests (rule 2 already allows them) |
| D2 | **The regex engine** (`dependency`). PLAN.md §9 picks `regex` and `fancy-regex` | **A hand-written ECMAScript engine** (`ocio-ops/src/std_regex/`), with each platform's error texts and quirks, checked against both wheels by generated patterns and paths (O3.4). `regex` lacks backreferences and lookahead; `fancy-regex` works on `str`, not bytes. Neither gives C++'s error texts. No new dependency |
| D3 | **Third-party sources and licences** (`dependency`). Phase 3 ports yaml-cpp's parser (MIT), pystring 1.1.4 `os.path` (BSD-3), and reproduces two C++ libraries' `std::regex` and `num_get` | Pin yaml-cpp 0.8.0 and pystring v1.1.4 as submodules under `upstream/`, so citations can be checked; add pystring's notice to `NOTICE`. MSVC STL (Apache-2.0 with LLVM exception) may be translated, with its notice. libstdc++ (GPL with the runtime exception) and glibc (LGPL) must not be translated into this BSD crate: reproduce their behavior from the C++ standard and check it against the platform. Worth confirming the same rule for the Phase 0 `cfmt` and `number_utils` ports |
| D4 | **Public API of the config model** (`api`) | (a) Getters return `&[u8]`; setters take `impl AsRef<[u8]>` and stop at the first NUL, as the CDL ID and `FormatMetadata` already do. No new string type. (b) `Config::new()` (`Create`) returns an editable `Config`; the loaders `from_stream`, `from_file`, `from_env`, `from_builtin_config`, `from_config_io_proxy` return `Arc<Config>`, like `create_raw()`; editing a shared config is `(*config).clone()` (`createEditableCopy`). (c) `ColorSpace`, `Look`, `ViewTransform`, `NamedTransform`, `ColorSpaceSet`, `FileRules`, `ViewingRules` and `Context` are `Clone` structs; the config copies what it is given and lends `&T`. (d) Upstream's null pointers become `Option`; by-index getters keep upstream's `""` and `-1`. (e) Lazily computed text (cache IDs, `serialize()`) is returned owned. (f) The `getProcessor` overloads: `processor_with_names(src, dst)`, `processor_with_color_spaces`, `processor_with_display_view(src, display, view, dir)`, `processor_with_named_transform(nt, dir)`, `processor_with_named_transform_name(name, dir)`, and `processor_with_context_and_…` for each context variant. (g) `ConfigIoProxy` is a trait held as `Arc<dyn ConfigIoProxy>`. (h) The current config is `ocio::current_config()` and `set_current_config()`, over a `RwLock<Option<Arc<Config>>>`: no `arc-swap`. (i) Errors stay `ocio::Exception`; yaml-cpp's exceptions are an internal type turned into upstream's text where OCIO catches them |
| D5 | **ExposureContrast and Grading transforms in configs** | **Wait for Phase 5.** Their classes wrap their op data (~3,000 lines with the B-spline and tone precomputations, which WP 2.4 also touches). Until then the YAML reader gives "not ported yet" for their 5 tags, and 7 config tests wait. The alternative: port the classes and op data here without renderers, so the reader and writer are complete in M1 |
| D6 | **ConfigUtils stays in Phase 9** | Keep `GetProcessorFromConfigs`, the built-in color space processors, `Identify*`, `setColorSpaceConversion` and `concatenate` in Phase 9, as PLAN.md has it. `instantiateDisplayFrom*` waits for the ICC reader (WP 4.7) and `SystemMonitor` (Phase 9) |
| D7 | **Retiring S1's test reader** | Once 3.7d passes the same fixtures through the real reader and writer, remove `tests/common/yaml_tree.rs`, `ocio_writer.rs` and the `saphyr-parser` dev-dependency. The checks get stronger, not weaker |
| D8 | **The corpus** (`dependency`). `corpus/` is empty | Fetch OpenColorIO-Config-ACES v1.0.0 to v4.0.0 and the legacy v1 configs (spi-vfx, spi-anim, nuke-default, aces_1.x, from imageworks/OpenColorIO-Configs) at pinned hashes, after a licence check. That is a download, so the owner or the orchestrator does it. Then O3.7 |
| D9 | **The built-in configs' text** | Embed upstream's 8 `.ocio` files byte for byte. On Windows, return them with CRLF line endings, as the Windows wheel does (PLAN.md Appendix B), and list it as an `I-` entry |
| D10 | **The Windows file hash** | `CreateFileContentHash` prints `_wstat`'s `st_dev`. Compute it as the UCRT does (the drive number of the full path) without FFI, and check it against the UCRT in `ocio-testkit`'s `crt.rs`. Cache IDs with file references are machine-specific on both platforms, so they are checked live only |

## Platform risks

- **Number parsing in YAML.** yaml-cpp reads numbers with `std::stringstream >> std::noskipws`
  after `unsetf(std::ios::dec)`, so integers take hex and octal prefixes. MSVC's `num_get` and
  libstdc++'s differ (hex floats, overflow, partial tokens). 3.3f ports both and sweeps them
  against both wheels (O3.3).
- **`std::regex`.** MSVC and libstdc++ differ in error texts and in some matches. Both recurse
  deeply on long inputs; where a wheel would crash, the port returns an error (`U-` entry, the
  owner's general rule).
- **Paths.**
  - pystring compiles its `nt` `os.path` on Windows and `posix` on Linux.
  - Windows opens files through UTF-8 to UTF-16 conversion (`MultiByteToWideChar`, lossy for
    invalid bytes); Linux uses the bytes.
  - `search_path` is split on `:` even on Windows, which breaks drive letters, as in the wheel.
  - The working dir is the config file's directory.
  - File hashes are `st_dev:st_ino` on Linux and `st_dev:std::hash(path)` on Windows.
- **Environment.**
  - Windows names are case-insensitive (`GetEnvironmentVariable`), and `_wenviron` holds hidden
    `=C:` entries with an empty name.
  - Linux reports an empty variable as present.
  - A context without `environment:` hashes the whole environment into its cache ID, so tests
    inject it and the oracle sets it per call.
- **Case folding.** Names, aliases, roles and built-in styles compare with `Strcasecmp`, which
  folds A-Z only in the port (deviation D-4).
- **Built-in config text.** CRLF on the Windows wheel, LF on Linux (D9). Parsed configs,
  `serialize()` and cache IDs are the same on both.
- **NaN text** in `operator<<` (allocation variables, matrices) follows each platform's iostreams
  (`cfmt`, `Crt::NATIVE`).

## Order and parallelism

```
p3-oracle (O3.1-O3.6) ─────────── oracle checks of every card below
p3-yaml-parser (3.3a-f) ─────────────────────┐
p3-transforms (3.1a-c) ──┬───────────────────┴─ p3-yaml-load-1 (3.3g-i) ─┐
                         └─ p3-builtins (3.2e-g)                         │
p3-context (3.5a-f) ─┬─ p3-config-1 (3.4d-g) ─ p3-config-2 (3.4h-j) ─────┤
p3-model-objects ────┘          │                                         │
  (3.4a-c)                      └─ p3-rules (3.9c-e) ─────────────────────┴─ p3-yaml-load-2 (3.3j-m)
p3-regex (3.9a-b) ──────────────────┘                                         │
                       ┌─────────────────────────┬──────────────────────────┤
                 p3-yaml-save (3.7a-d)    p3-validate (3.8a-c)     p3-builders (3.2a-d)
                       └──────────── p3-loading (3.10a-c) ─────────────────┘
Phase 2 (WP 2.1-2.4) ─── p3-after-p2 ─── p3-m1-parity ── M1
```

- **Implementer A (the critical path, text):** `p3-yaml-parser` → `p3-yaml-load-1` →
  `p3-yaml-load-2` → `p3-yaml-save` → `p3-loading`.
- **Implementer B (the model):** `p3-context` → `p3-model-objects` → `p3-config-1` →
  `p3-config-2` → `p3-validate`.
- **Implementer C:** `p3-oracle` first, then `p3-transforms` → `p3-regex` → `p3-rules` →
  `p3-builtins` → `p3-builders`, and later `p3-after-p2` and `p3-m1-parity`.
- **Verifier:** reviews every card before it lands, as in Phase 1.
- With two implementers, C's cards go to whoever is free, `p3-oracle` and `p3-transforms`
  first.

## Cards

Each card is one branch and one PR. Cards in different rows can run in parallel once their
"Needs" have landed. "Now" means the card can start immediately.

| Card | Chunks | Size | Who | Needs |
|---|---|---|---|---|
| `p3-oracle` | O3.1–O3.6, each its own chunk; the owner reviews them, labelled `oracle` | 6 | C | now |
| `p3-yaml-parser` | 3.3a–3.3f | 6, ~2,950 lines | A | now (D1, D3); O3.3 for 3.3f's sweeps |
| `p3-context` | 3.5a–3.5f | 6, ~1,500 | B | now; O3.2 for its oracle checks |
| `p3-transforms` | 3.1a–3.1c | 3, ~850 | C | now; O3.6 |
| `p3-regex` | 3.9a–3.9b | 2, ~1,000 | C | now (D2); O3.4 |
| `p3-model-objects` | 3.4a–3.4c | 3, ~1,800 | B | now; O3.1 |
| `p3-config-1` | 3.4d–3.4g | 4, ~1,300 | B | `p3-context`, `p3-model-objects` |
| `p3-config-2` | 3.4h–3.4j | 3, ~1,350 | B | `p3-config-1` |
| `p3-rules` | 3.9c–3.9e | 3, ~1,450 | C | `p3-regex`, `p3-config-1` |
| `p3-builtins` | 3.2e–3.2g | 3, ~1,300 | C | `p3-transforms` |
| `p3-yaml-load-1` | 3.3g–3.3i | 3, ~1,050 | A | `p3-yaml-parser`, `p3-transforms`, 3.5c |
| `p3-yaml-load-2` | 3.3j–3.3m | 4, ~1,150 | A | `p3-yaml-load-1`, `p3-config-2`, `p3-rules` |
| `p3-yaml-save` | 3.7a–3.7d | 4, ~1,170 | A | `p3-yaml-load-2` |
| `p3-validate` | 3.8a–3.8c | 3, ~1,070 | B | `p3-yaml-load-2` |
| `p3-builders` | 3.2a–3.2d | 4, ~1,200 | C | `p3-yaml-load-2`, `p3-transforms` |
| `p3-loading` | 3.10a–3.10c | 3, ~550 | A | `p3-yaml-save`, `p3-validate` |
| `p3-after-p2` | the **P2** parts: the remaining built-ins' ops (`Displays.cpp` fixed functions; `ACES.cpp` 1.x and 2.0 output transforms; Canon and Apple); `FixedFunctionTransform`'s YAML and version checks; `Lut3DTransform` in processors; config test group E; the transform and processor tests listed above as P2 | 4 | C | `p3-builtins`, `p3-loading`, Phase 2 WP 2.1–2.4 |
| `p3-m1-parity` | Every built-in config through the port's API against the wheel: each color space to and from the reference, each display and view, look and named transform; CPU at every bit depth, layout and optimization level; GPU in all 10 languages. Then the corpus (D8) | 2–3 | any | `p3-after-p2` |

Small cards land sooner and are easier to verify. When a card grows past about 6 chunks, split it
at a dependency boundary.

**Can start now:** `p3-oracle`, `p3-yaml-parser`, `p3-context`, `p3-transforms`, `p3-regex`
and `p3-model-objects`. None of them needs Phase 2.

**Waits for other phases:**
- **P2:** `p3-after-p2`; the pixels of built-in configs (`p3-m1-parity`).
- **P4:** `FileTransform`'s format registry, loading and ops; `.ocioz` archives; config group F;
  the CDL file tests; `GroupTransform::write`.
- **P5:** ExposureContrast and Grading transforms in configs (D5); config group G; the
  dynamic-property processor tests.
- **P9:** ConfigUtils (D6); config group H; two `ColorSpace_tests.cpp` tests.
- **Phase 7:** the legacy GPU processor.

## Phase 3 exit (M1, with Phase 2)

Through OCIO's API, on Windows and Rocky Linux 9:
- every config the wheel accepts loads to the same state, and every one it refuses gives the
  same message: the 8 built-in configs, upstream's test configs and the corpus;
- `serialize()` and the config cache ID are byte-exact;
- `validate()` gives the same result and message;
- every color space, display and view, look and named transform processor of the built-in
  configs is byte-exact on the CPU (every bit depth, layout and optimization level) and on the
  GPU (all 10 languages);
- the upstream tests of the slice are ported and counted in `docs/parity.md`, except those
  waiting for Phases 4, 5, 7 and 9, each listed in `upstream-map.toml` with its reason.
