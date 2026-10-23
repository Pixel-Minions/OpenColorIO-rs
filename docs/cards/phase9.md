# Phase 9 cards: ConfigUtils, config merging and the remaining library surface

**Status: draft, 2026-10-23.** Owner decisions P9-1 to P9-7 are open. The owner pulled
ConfigUtils and config merging forward (P6-11, 2026-10-17): 9 of the 384 Python tests reach
them, so these cards can start now, beside Phases 5 and 6. The other cards start as "Cards"
says.

**Goal: Phase 9's part of milestone M5** (PLAN.md §10, Phase 9, ~9 pw): OCIO 2.5.2's library
surface that no earlier phase ports, byte-exact with the `opencolorio==2.5.2` wheel:
- **ConfigUtils** (`ConfigUtils.cpp/.h`, ~1,055 lines): the interchange spaces of a config
  (`Config::IdentifyInterchangeSpace`, `IdentifyBuiltinColorSpace`), the processors between two
  configs (the 8 `GetProcessorFromConfigs` overloads), to and from the default built-in config
  (`GetProcessorToBuiltinColorSpace`, `GetProcessorFromBuiltinColorSpace`), and the helpers
  that config merging uses (reference-space converters, color-space fingerprints);
- **config merging** (`apphelpers/mergeconfigs/`, ~4,350 lines): `ConfigMergingParameters`,
  `ConfigMerger`, the OCIOM file format (read and write), the seven section mergers and
  `ConfigMergingHelpers`;
- **the system monitors** (`SystemMonitor.cpp/.h`, `SystemMonitor_windows.cpp`) and the rest of
  `Config::instantiateDisplayFromMonitorName`;
- **everything else still open on the parity dashboard** that belongs to no other phase:
  upstream tests that earlier cards left behind, and every `todo` or `partial` entry of
  `upstream-map.toml` closed or given the phase it waits for.

"Byte-exact" means, on Windows and Rocky Linux 9:
- **names and processors:** the color-space names the heuristics identify, and the processors
  between configs, op for op with their cache IDs (`processor_ops`), as the wheel's on the same
  OS and CPU (PLAN.md §3);
- **text:** merged configs (`serialize()`), OCIOM files (`ConfigMerger::serialize`),
  `operator<<` of `ConfigMergingParameters`, every exception, and every warning a merge logs,
  byte for byte;
- **monitors:** the monitor names and ICC profile paths the wheel lists on the same machine.

Each work package below is a list of **chunks**. A chunk is one commit and is mergeable on its
own (`CLAUDE.md` → "Chunks"). Line counts are upstream's non-blank lines at v2.5.2 without `//`
comments (block comments counted: the counts are upper bounds). Paths are relative to
`src/OpenColorIO/` unless they start with `tests/`, `include/` or `crates/`.

## How this phase works

Everything in `docs/cards/phase3.md`, `phase4.md` and `phase5.md` → "How this phase works"
still applies: cards and PRs, the chunk gate, oracle-first, owner items, verifier rounds with
mutation testing. What changes in Phase 9:

- **Configs in, configs and names out.** Most outputs are text (a merged config's
  `serialize()`, an OCIOM file, messages, warnings) or a processor (`processor_ops`: the ops,
  their getters, the cache ID). The oracle's `object_calls` (O5.2) runs the calls on the wheel,
  with configs from `config_calls` (O3.1) and files through `with_files` (O4.2).
- **The heuristics apply processors to pixels.** `IdentifyInterchangeSpace` and the merge's
  duplicate search apply CPU processors to fixed test values (`isIdentityTransform`, the
  fingerprints) and compare them with tolerances (`1e-3`, `5e-3`). The name they pick follows
  the pixels, so it follows the CPU kernels and the platform's libm (D12). Every chunk of this
  phase runs `cargo xtask gate --release --rocky`. A test whose configs reach a LUT or a libm
  call through the heuristics joins the `cpu-tests` alias.
- **Section mergers are internal.** The wheel exposes only whole merges (`mergeConfigs`,
  `ConfigMergingHelpers`), so each section chunk (9.2d–9.2i) lands with upstream's tests of
  that section, and the whole-merge sweep against the wheel comes with 9.2j. The section cards
  (`p9-merge-2`, `p9-merge-3`) land together, once 9.2j's sweep passes.
- **Null pointers.** Python passes `None` where upstream takes a `shared_ptr`. Some functions
  check for it and throw (`MergeConfigs`: "The input or base config was not set."). Others
  dereference it: probed on Windows on 2026-10-23, `Config.GetProcessorFromConfigs(None, ...)`,
  `Config.IdentifyBuiltinColorSpace(None, ...)` and `mergeConfigs()` after
  `addParams(None)` crash the wheel's process (segmentation fault). The checked ones keep
  their message (`Option`, D4(d)); the crashing ones are `U-` entries, and the port returns a
  deliberate error (the owner's general rule).
- **Owner items** stop at the owner, labelled `api`, `dependency`, `deviation` or `n/a`. Oracle
  chunks are reviewed by a verifier agent (the owner's decision of 2026-10-05).

**What Phase 9 needs from Phases 1–8.**
- Phase 1: the optimizer and `createGroupTransform` (`simplifyTransform` optimizes a group at
  `OPTIMIZATION_DEFAULT` and reads it back), the CPU processors.
- Phase 3: the config model and its copies, the `getProcessor` overloads, file and viewing
  rules, the YAML reader and writer (yaml-cpp's parser and emitter, which the OCIOM format
  also uses), the built-in configs (`ocio://default`, `ocio://cg-config-latest`), `pystring`'s
  `os.path` and `StringUtils` (`crates/ocio-ops/src/utils/`).
- Phase 4: `Config::CreateFromFile` with `.ocioz` archives (merges read configs and archives),
  the ICC reader (`instantiateDisplayFromICCProfile`, landed in 4.7b), `FileTransform` (the
  heuristics resolve LUT files).
- Phase 5: none for the port itself. Merge sweeps over configs with grading or
  ExposureContrast transforms wait for `p5-transforms`.
- Phase 6 needs Phase 9: 6.8a binds what this phase ports.

---

## WP 9.1: ConfigUtils (`ocio`)

| Chunk | Upstream | Rust (`ocio/src/`) | Port tests |
|---|---|---|---|
| 9.1a | `ConfigUtils.h` (81: the declarations, `Fingerprint`, `ColorSpaceFingerprints`, `SuspendCacheGuard`); `ConfigUtils.cpp:14-843` (525): the built-in names (`getSRGBColorSpaceName`, `getBuiltinLinearSpaceName`), `GetInterchangeRolesForColorSpaceConversion`, `containsSRGB`, `getRefSpaceName`, `getDataSpaceName`, `isIdentityTransform`, `hasNonTrivialMatrixTransform`, `containsBlockedTransform`, `excludeColorSpaceFromHeuristics`, `getReferenceSpaceFromLinearSpace`, `getReferenceSpaceFromSRGBSpace`, `IdentifyInterchangeSpace`, `IdentifyBuiltinColorSpace`; `Config::IdentifyInterchangeSpace` and `IdentifyBuiltinColorSpace` (`Config.cpp:2908-2932`) (~630, one unit: the heuristics) | `config_utils.rs`, `config.rs` | oracle sweep (`object_calls` on `Config`): both functions over the 8 built-in configs, the configs of `tests/data/files/configs`, and generated ones (interchange roles present, missing, pointing to a missing space; data, inactive and display-referred spaces; spaces with blocked transforms), each against every color space of the default built-in config, a missing name and `""`: the names, or the exception and the log |
| 9.1b | `Config.cpp:4889-5232` (~310): the 8 `GetProcessorFromConfigs` overloads (with interchange names, through the interchange roles, to a display and view, each with and without contexts), `GetProcessorToBuiltinCS`, `GetProcessorToBuiltinColorSpace`, `GetProcessorFromBuiltinColorSpace`; `Processor::Impl::concatenate` (`Processor.cpp:643-652`) | `config.rs`, `processor.rs` | `Config_tests.cpp` `get_processor_from_two_configs`; `ColorSpace_tests.cpp` `ConfigUtils/processor_to_known_colorspace`, `ConfigUtils/processor_to_known_colorspace_alt_config`; oracle sweep: every overload over pairs of 9.1a's configs, their color spaces, displays and views, contexts with variables, data spaces: `processor_ops` of the processor, or the exception |
| 9.1c | `ConfigUtils.cpp:844-1482` (449): `simplifyTransform`, `invertTransform`, `getTransformForDir`, `getRefSpaceConverter`, `transformIsEmpty`, `updateReferenceColorspace`, `updateReferenceView`, `hasColorSpaceRefType`, `initializeRefSpaceConverters`, `calcColorSpaceFingerprint`, `initializeTestVals`, `initializeColorSpaceFingerprints`, `findEquivalentColorspace` | `config_utils.rs` | `ConfigUtils_tests.cpp` `MergeConfigs/config_utils_update_reference`, `MergeConfigs/config_utils_find_equivalent_colorspace` |

- `Processor::Impl::setColorSpaceConversion` (`Processor.cpp:598-621`) has no caller in OCIO
  2.5.2: the map says so, as for `CustomKeys` `hasKey`.
- `SuspendCacheGuard` turns a shared config's processor cache off and back on through a const
  pointer (`setProcessorCacheFlags` is `const noexcept`). Upstream's flags are a plain member,
  so another thread using the config meanwhile races with the guard (undefined behaviour: a
  `U-` entry). The port's flags are atomic already (1.8g): that thread sees the cache off.
- 9.1c has no wheel-visible output of its own (the Python module doesn't expose it): its
  oracle check is the merge sweep (9.2j), as the reference-space options call it.

## WP 9.2: config merging (`ocio`)

| Chunk | Upstream (`apphelpers/mergeconfigs/`) | Rust (`ocio/src/apphelpers/mergeconfigs/`) | Port tests |
|---|---|---|---|
| 9.2a | `MergeConfigsHelpers.h:21-127` (88: `ConfigMergingParameters::Impl`, its defaults and copy); `MergeConfigsHelpers.cpp:22-412` (~330): every setter and getter, `createEditableCopy`, `operator<<`; `OCIOMYaml::StrategyStringToEnum`, `EnumToStrategyString` (`OCIOMYaml.cpp:481-519`); `include/OpenColorIO/OpenColorAppHelpers.h:542-709` (the API, P9-2) (~450) | `merge_configs_helpers.rs`, `ociom_yaml.rs` | oracle (`object_calls`): the defaults, each setter then every getter, `str()` (`operator<<`) after each, strings with commas, spaces, `""` and non-ASCII bytes |
| 9.2b | `MergeConfigsHelpers.h:129-198` (42: `ConfigMerger::Impl`, its deep copy); `MergeConfigsHelpers.cpp:449-680` without `CreateFromFile` (~210): `loadConfig` (search paths, working directory, built-in names, earlier merges' outputs), `Create`, `createEditableCopy`, the search paths, working directory, parameters, versions and merged configs; the OCIOM writer (`OCIOMYaml.cpp:520-669`, 117) with `serialize` and `operator<<` (`625-644`); `OpenColorAppHelpers.h:710-853` (~370) | `merge_configs_helpers.rs`, `ociom_yaml.rs` | oracle: the getters and setters, `addParams` and `getParams` sharing one object (P9-2), `createEditableCopy` copying it, `serialize()` after each change |
| 9.2c | The OCIOM reader: `OCIOMYaml.h` (34), `OCIOMYaml.cpp:19-480` (~430: the typed loaders, `throwValueError`, `CheckDuplicates`, the options, overrides and parameters of each merge, `countMerges`, `read`); `ConfigMerger::Impl::Read`, `CreateFromFile` (`MergeConfigsHelpers.cpp:413-447, 537-558`) (~510) | `ociom_yaml.rs`, `merge_configs_helpers.rs` | `MergeConfigsHelpers_tests.cpp` `ociom_parser`, `ociom_parser_no_overrides`, `params_serialization`, `ociom_serialization`; oracle: upstream's two `.ociom` files and generated ones (every key, unknown keys, duplicates, wrong types, every strategy spelling, empty and missing sections, YAML errors), each read then written: the parameters, the text, or the exception |
| 9.2d | `SectionMerger.h:1-167` (~130: `MergeHandlerOptions`, `SectionMerger`, `GeneralMerger`, `RolesMerger`); `SectionMerger.cpp:1-421` (300: `splitActiveList`, `mergeStringsWithoutDuplicates`, `notify`, `setMergedConfigVersion`, the general and roles sections) (~430); the test helpers (`MergeConfigsHelpers_tests.cpp:1-164`) come with its tests | `section_merger.rs` | `MergeConfigsHelpers_tests.cpp` `general_section`, `roles_section` |
| 9.2e | `FileRulesMerger` (`SectionMerger.h:168-197`, `SectionMerger.cpp:422-727`: 215), `LooksMerger` (`SectionMerger.h:288-313`, `SectionMerger.cpp:1919-2027`: 81) (~340) | `section_merger.rs` | `file_rules_section`, `looks_section` |
| 9.2f | `DisplayViewMerger` (`SectionMerger.h:198-244`, `SectionMerger.cpp:728-1682`: 698): displays, the virtual display, shared views, the active lists, viewing rules (~730; one class its test covers together, past the usual ~600, as 5.7b) | `section_merger.rs` | `displays_views_section` |
| 9.2g | `ViewTransformsMerger` (`SectionMerger.h:245-287`, `SectionMerger.cpp:1683-1918`: 183), with the reference-space adapters of 9.1c (~210) | `section_merger.rs` | `view_transforms_section` |
| 9.2h | `ColorspacesMerger` (`SectionMerger.h:314-422`, `SectionMerger.cpp:2028-3077`: 677): search paths, families and their prefixes, aliases, the duplicate search (fingerprints, 9.1c), conflicts, the inactive list (~760; one class its tests cover together, as 9.2f) | `section_merger.rs` | `colorspaces_section`, `colorspaces_section_common_reference_and_duplicates`, `overrides` |
| 9.2i | `NamedTransformsMerger` (`SectionMerger.h:423-476`, `SectionMerger.cpp:3078-3731`: 416) (~460) | `section_merger.rs` | `named_transform_section`, `named_transform_section_errors`, `colorspaces_section_errors` (it merges named transforms too) |
| 9.2j | `ConfigMerger::mergeConfigs` (`MergeConfigsHelpers.cpp:681-729`), `ConfigMergingHelpers::MergeConfigs` and `MergeColorSpace` (`731-804`); `OpenColorAppHelpers.h:855-886` (~100) | `merge_configs_helpers.rs` | `merges_with_ociom_file`, `merges_with_ocioz_file`, `merge_in_memory_configs`, `merge_single_colorspace`; the whole-merge sweep against the wheel (below) |

- **The merge sweep (9.2j).** `object_calls` merges, each merged config compared by
  `serialize()`, with the log and any exception: every strategy (6) for each section (7) and
  as the default; every option (input first, error on conflict, avoid duplicates, adjust the
  input reference space, family prefixes, the overrides of name, description, search path,
  environment, active displays and views, inactive color spaces); over upstream's merge
  configs (`tests/data/files/configs/mergeconfigs/`), pairs of the 8 built-in configs, the
  configs of `tests/data/files/configs`, and generated ones (conflicting names, aliases and
  roles, display-referred spaces, shared and virtual views, file and viewing rules);
  `ConfigMerger.mergeConfigs` over chains of merges from OCIOM files (an earlier merge's output
  as a later merge's base). The `quick` tier samples it, `full` runs every case.
- `MergeConfigsHelpers_tests.cpp` is 4,852 lines of tests: each chunk ports its own tests
  verbatim into `merge_configs_helpers_tests.rs` (next to `merge_configs_helpers.rs`; the map's
  target moves with 9.2a).
- `src/apps/ociomergeconfigs/main.cpp` is a tool: Phase 10.

## WP 9.3: system monitors (`ocio`)

| Chunk | Upstream | Rust (`ocio/src/`) | Port tests |
|---|---|---|---|
| 9.3a | `SystemMonitor.h` (36), `SystemMonitor.cpp` (92): `SystemMonitors::Get` (one list per process), `isSupported`, `getNumMonitors`, `getMonitorName`, `getProfileFilepath`, `GetICCProfileFromMonitorName`; Linux's empty list; `SystemMonitor_windows.cpp` (110): `getAllMonitorsWithQueryDisplayConfig` and `getAllMonitors` through `QueryDisplayConfig`, `DisplayConfigGetDeviceInfo`, `EnumDisplayDevicesW`, `CreateDCW`, `GetICMProfileW`, `DeleteDC` (P9-3); the rest of `Config::instantiateDisplayFromMonitorName` (`Config.cpp:4104-4118`); `OpenColorIO.h:4047-4083` (~330) | `system_monitor.rs`, `system_monitor_windows.rs`, `system_monitor_windows_ffi.rs`, `config.rs` | `Config_tests.cpp` `virtual_display`, `virtual_display_with_active_displays` (P9-4); oracle: `SystemMonitors().getMonitors()` on this machine, and `instantiateDisplayFromMonitorName` with each monitor's name, a missing name and `""`: the config's `serialize()` or the exception |

- The wheel's `OpenColorIO_2_5.dll` imports the wide functions (`GetICMProfileW`,
  `EnumDisplayDevicesW`, `QueryDisplayConfig`; checked 2026-10-23), so names and paths go
  through `Utf16ToUtf8`, as `_UNICODE` builds do.
- Upstream indexes the friendly names from `QueryDisplayConfig` with `EnumDisplayDevices`'
  adapter number (`friendlyMonitorNames.at(dispNum)`): the two lists need not match. Ported as
  is, and listed in `docs/improvements.md`.
- `SystemMonitor_macos.cpp` is macOS's (P9-5). `PySystemMonitors.cpp` binds 9.3a in 6.8a.

## WP 9.4: the tests earlier cards left behind (`ocio`)

| Chunk | What | Port tests |
|---|---|---|
| 9.4a | Upstream tests whose code has landed but which no card ported. Tests only: a test that finds a missing piece stops the chunk, and the piece gets a chunk of its own | `ColorSpaceSet_tests.cpp` `basic`, `decoupled_sets`, `order_validation`, `operations_on_set` (they need 3.4f, landed); `NamedTransform_tests.cpp` `basic`, `alias` (3.4j, 3.2a); `BuiltinTransformRegistry_tests.cpp` `Builtins/read_write` (every built-in's processor, `Config::validate`); `Exception_tests.cpp` `basic`, `missing_file` (the error kinds and their texts) |

- `Platform_tests.cpp` `aligned_memory_test` and `UnitTestMain.cpp` `UnitTest/windows_debug`
  are P9-6's.

## WP 9.5: the parity dashboard (`upstream-map.toml`, docs)

| Chunk | What |
|---|---|
| 9.5a | **Audit.** Every `todo` and `partial` entry of `upstream-map.toml` outside the bindings (Phase 6), the tools (Phase 10) and the GPU suite (Phase 7), read against upstream: each function ported, ported elsewhere, without a caller in 2.5.2, or `n/a` with its reason. The entries left: the headers no chunk claimed (`Mutex.h`, `PrivateTypes.h`, `TransformBuilder.h`, `OCIOYaml.h`), the test support (`UnitTestOptimFlags.cpp/.h`, `UnitTestMain.cpp`, `tests/testutils/UnitTest.cpp`, `tests/utils/UnitTestMain.cpp`), functions without a caller (`CustomKeys` `hasKey` and `getValueForKey`, `setColorSpaceConversion`), and every `partial` whose note ends at "comes with its card" or "not yet". Each finding is either closed in the map or listed with the phase it waits for |
| 9.5b– | One chunk per piece 9.5a finds unported (sized as it finds them), each with its tests |

- `p9-dashboard` runs last: after Phases 5, 7 and 8 have landed, while Phase 6 and 10 entries
  keep their status with their phase named.

---

## Upstream tests

| File | Tests | Chunks | Waits |
|---|---:|---|---|
| `apphelpers/MergeConfigsHelpers_tests.cpp` | 20 | 20 (9.2c–9.2j) | — |
| `ConfigUtils_tests.cpp` | 2 | 2 (9.1c) | — |
| `ColorSpace_tests.cpp` | 2 of its tests | `ConfigUtils/processor_to_known_colorspace`, `_alt_config` (9.1b) | — |
| `Config_tests.cpp` | 3 of its tests | `get_processor_from_two_configs` (9.1b), `virtual_display`, `virtual_display_with_active_displays` (9.3a) | — |
| `ColorSpaceSet_tests.cpp` | 4 | 4 (9.4a) | — |
| `NamedTransform_tests.cpp` | 2 of its tests | `basic`, `alias` (9.4a) | — |
| `transforms/builtins/BuiltinTransformRegistry_tests.cpp` | 1 of its tests | `Builtins/read_write` (9.4a) | — |
| `Exception_tests.cpp` | 2 | 2 (9.4a) | — |
| `Platform_tests.cpp` | 1 of its tests | — | `aligned_memory_test`: `n/a` (P9-6) |
| `UnitTestMain.cpp` | 1 | — | `UnitTest/windows_debug`: `n/a` (P9-6) |

36 upstream C++ tests land in Phase 9, and 2 go on the reviewed not-applicable list (P9-6).
The Python suite's `MergeConfigsTest.py` (8) and `ColorSpaceTest.py`
`test_processor_to_known_colorspace` are Phase 6's (6.8a).

`cargo xtask upstream-tests` lists 111 C++ tests not ported at `phase0` 7ffd7dce (and the 264
of the GPU suite). Besides Phase 9's 38:
- **Phase 5, on branches now (70):** `FileFormatCTF_tests.cpp` (30, 4.5i and 4.6f),
  `GradingToneOpCPU_tests.cpp` (7), the dynamic-property tests of `Processor` (6), `Config` (4),
  `OpOptimizers` (2), `Op`, `DynamicProperty`, `CPUProcessor`, `DisplayViewTransform` (1 each),
  `GpuShader` `MetalSupport7` to `9` (3), the app helpers' `DisplayViewHelpers` (5),
  `LegacyViewingPipeline` (4), `MixingHelpers` (4), and `FileTransform/context_variables` (1,
  after 4.5i).
- **Phase 7 (3):** `NoOps/partition_gpu_ops`, `NoOps/throw` (`PartitionGPUOps`, `Create3DLut`)
  and `GpuShader/MetalLutTest` (the legacy GPU processor), with the GPU suite (264).

## Oracle support (verifier-reviewed chunks)

The existing commands reach most of Phase 9:
- **`object_calls`** (O5.2, `object_api.py`): `Config`'s static methods (`IdentifyBuiltinColorSpace`,
  `IdentifyInterchangeSpace`, the 8 `GetProcessorFromConfigs`, `GetProcessorTo/FromBuiltinColorSpace`)
  with configs made by earlier calls (`{"ref"}`); `ConfigMergingParameters` and `ConfigMerger`
  (`new`, `Create`, `CreateFromFile` with the request's files, setters, `mergeConfigs`,
  `getMergedConfig`, `serialize`); `SystemMonitors`; processors written out as
  `processor_ops` does, configs by `serialize()`.
- **`config_calls`** (O3.1) for the configs; **`with_files`** (O4.2) for OCIOM files, configs
  and archives in a directory; **`py_eval`** (O6.2) for the binding's checks (6.8a).

| Chunk | Command (module) | What |
|---|---|---|
| O9.1 | `object_calls` (`object_api.py`, `config_api.py`) | Two gaps: the calls can't reach the `ConfigMergingHelpers` submodule (`MergeConfigs`, `MergeColorSpace`; `HELPER_MODULES` holds `DisplayViewHelpers` and `ColorSpaceHelpers` only), and `{"enum": name}` looks only at module level, where the wheel doesn't export `MergeStrategies`' values (`ConfigMergingParameters.STRATEGY_PREFER_INPUT` exists, `PyOpenColorIO.STRATEGY_PREFER_INPUT` doesn't; checked 2026-10-23). O9.1 adds the submodule and a class-qualified enum value (`{"enum": "ConfigMergingParameters.STRATEGY_PREFER_INPUT"}`), with contract tests |

- O9.1 lands first, in card `p9-oracle`, and a verifier agent reviews it with mutation testing.
- No fixture group: the merge and heuristic results are checked live, through the oracle's
  cache, on each platform.

## Owner decisions

| # | Decision | Recommendation |
|---|---|---|
| P9-1 | **ConfigUtils' public API** (`api`) | **Associated functions on `Config`, named after D4(f)'s `getProcessor` overloads.** `Config::identify_builtin_color_space(src: &Config, builtin: &Config, name) -> Result<&[u8]>` (borrowing from `src`, as upstream returns its name); `Config::identify_interchange_space(src, src_name, builtin, builtin_name) -> Result<(&[u8], &[u8])>` (upstream's two out-parameters, in the order Python returns them); `processor_to_builtin_color_space(src, src_name, builtin_name)` and `processor_from_builtin_color_space(builtin_name, src, src_name)`; the 8 `GetProcessorFromConfigs` as `processor_from_configs`, `processor_from_configs_with_interchange`, `processor_from_configs_to_display_view`, `processor_from_configs_to_display_view_with_interchange`, each with a `_with_contexts` twin taking `&Context` before each config. All return `Result<Arc<Processor>>`. Configs by `&Config`: upstream never checks them for null, and the wheel crashes on `None` (a `U-` entry; the binding returns the error) |
| P9-2 | **The merging API** (`api`) | (a) `MergeStrategies`, a public enum with upstream's values (`PreferInput` = 0 … `Unspecified`). (b) **`ConfigMergingParameters` is a shared handle**, as upstream's `RcPtr`: `ConfigMergingParameters::new()`; `Clone` shares; `create_editable_copy()` copies; setters take `&self` (a `Mutex` inside); getters return owned bytes (as `CurrentContext`'s do); `to_bytes()` is `operator<<`. Upstream's own tests need it: they add a parameters object to a merger, change it through `getParams(0)`, and merge with the first handle; Python sees the same object (P6-8). The alternative: a plain value the merger copies, and a second, shared type later for the binding. (c) `ConfigMerger` a value: `new()`; `from_file(path) -> Result<Arc<ConfigMerger>>` and `merge_configs(&self) -> Result<Arc<ConfigMerger>>` (upstream's const pointers, like `Config`'s loaders, D4(b)); `create_editable_copy()` a deep copy (parameters and merged configs copied, as upstream's `Impl::operator=`); `add_params(Option<ConfigMergingParameters>)` and `params(i) -> Option<ConfigMergingParameters>` (D4(d): the wheel accepts `None` and returns `None` out of range); `merged_config(i)` and `last_merged_config()` as `Option<Arc<Config>>`; `serialize() -> Result<Vec<u8>>`; search path getters keep `""` out of range. (d) `config_merging_helpers::merge_configs(params, base: Option<&Config>, input: Option<&Config>) -> Result<Config>` and `merge_color_space(params, base, color_space: Option<&ColorSpace>) -> Result<Config>`: `Option` because upstream checks for null with its own message; an editable `Config`, as upstream's `ConfigRcPtr` |
| P9-3 | **The Windows monitor API** (`dependency`, `unsafe`) | **Hand-written `extern "system"` declarations of the 6 functions and their structs, in one file, `crates/ocio/src/system_monitor_windows_ffi.rs`, added to the `unsafe` allowlist as `minizip/os_windows_ffi.rs` was (owner, 2026-10-17); no new crate.** The alternative, `windows-sys`, is a download and a large dependency for 6 functions. The public API: `SystemMonitors::get() -> &'static SystemMonitors` (made once per process, as upstream's static: monitors plugged in later are not seen, as with the wheel), `is_supported()`, `num_monitors()`, `monitor_name(i) -> Result<&[u8]>`, `profile_filepath(i) -> Result<&[u8]>` with upstream's messages. Linux returns no monitors, as the wheel |
| P9-4 | **Tests that depend on the machine's monitors** | **Port `virtual_display` and `virtual_display_with_active_displays` verbatim.** Their Windows blocks assert that a monitor with an ICC profile exists. This machine has two, and the wheel lists them (2026-10-23); whether GitHub's Windows runners have one is not known yet. If a runner lists none, the wheel fails the same assertion there, and the case comes back to the owner (a waiver for that runner, or the tests run only where the wheel lists a monitor), rather than a skip decided by an agent |
| P9-5 | **`SystemMonitor_macos.cpp`** (`n/a`) | **`n/a` for now: macOS is a later platform (D11).** The map's note says so, and the entry reopens with macOS |
| P9-6 | **Two tests not applicable** (`n/a`) | **`Platform/aligned_memory_test` and `UnitTest/windows_debug` go on the reviewed not-applicable list (PLAN.md §3).** The first tests `Platform::AlignedMalloc`/`AlignedFree`, C++ memory management: the port's only user, the Lut3D renderer, keeps its table in a `Vec<f32>` and loads it unaligned, so both functions are `n/a` too (reopened if Phase 8 needs aligned storage). The second tests upstream's test runner (the debug CRT's `isspace(INT_MAX)`, undefined in C), nothing of OCIO. `Exception_tests.cpp`'s two tests are ported (9.4a) |
| P9-7 | **What stays outside Phase 9** | **As the phases have it:** the legacy GPU path (`PartitionGPUOps`, `Create3DLut`, `getOptimizedLegacyGPUProcessor`, 3 tests) in Phase 7; the bindings (`PyMergeConfigs.cpp`, `PySystemMonitors.cpp`, the Config statics) in Phase 6 (6.8a); `ociomergeconfigs` and the other tools in Phase 10. P6-11's `p9-merge` card is split in three at its dependency boundaries (10 chunks; a card past about 6 splits) |

## Platform risks

- **Heuristics on pixels.** `isIdentityTransform` and the fingerprints apply CPU processors at
  `OPTIMIZATION_NONE` to fixed values and compare them within `1e-3` or `5e-3`. The name picked
  follows the pixels: a value at a tolerance's edge can pick differently by platform or CPU,
  as the wheel does. Checked live on both platforms; a sweep that reaches a LUT or libm joins
  `cpu-tests` and runs on SDE's CPUs too.
- **Matrices in merged configs.** `simplifyTransform` optimizes the reference-space adapters
  at `OPTIMIZATION_DEFAULT` (matrix composition in double, 1.2d) and the merged config writes
  them with yaml-cpp's precision. Any difference in the composition shows in the text.
- **Paths.** `ConfigMerger::loadConfig` builds paths with `pystring::os::path` (`join`,
  `normpath`, `isabs`): Windows' rules on Windows, POSIX on Linux (the port has both). Files
  open through `Platform::CreateInputFileStream` (UTF-8 to UTF-16 on Windows).
- **`std::vector::at` out of range.** `loadConfig` takes an earlier merge's output by the index
  of its parameters (`m_mergedConfigs.at(i)`): a base or input naming a later merge's output
  throws `std::out_of_range`, outside the merge's `try`, and Python gets an `IndexError`. Its
  text is the standard library's: "invalid vector subscript" from the Windows wheel (probed
  2026-10-23), libstdc++'s `vector::_M_range_check: __n (which is N) >= this->size() (which is
  M)` expected in Rocky Linux 9 (to be probed). The port gives each platform's text, with an
  error kind the binding maps to `IndexError`, and an `I-` entry.
- **Errors hidden by upstream.** `loadConfig` catches everything from `CreateFromFile` and
  `CreateFromBuiltinConfig` (`catch(...)`): a config that exists but fails to load becomes
  "Could not load the base or the input config". Ported, and listed in `docs/improvements.md`.
- **Monitors.** The list depends on the machine's displays, drivers and ICC profiles, and on
  `QueryDisplayConfig`'s order; it is compared with the wheel's on the same machine only.
- **Logs.** Merge conflicts are warnings (`LogWarning`) unless `error_on_conflict` makes them
  exceptions: the texts are byte for byte, through the logging callback. `object_calls`
  reports each call's log, and every check compares it, a merge with no warning included.
- **Crashes in the wheel.** Null configs and null parameters crash the wheel (above). Each such
  path gets a probe, a `U-` entry and a deliberate error; no comparison case runs them on the
  wheel.

## Order and parallelism

```
p9-oracle (O9.1) ──────────────────────────────── oracle checks of every merge card
p9-config-utils (9.1a-c) ──────────────────────────┐
p9-merge-1 (9.2a-c) ─ p9-merge-2 (9.2d-f) ─────────┴─ p9-merge-3 (9.2g-j) ── land with p9-merge-2
p9-monitors (9.3a) ─────────────────────────────── (6.8a binds it)
p9-catchup (9.4a) ──────────────────────────────── any time
Phases 5, 7, 8 landed ── p9-dashboard (9.5a-) ── Phase 9 done
```

- **Implementer A:** `p9-config-utils` → `p9-merge-3`.
- **Implementer B:** `p9-merge-1` → `p9-merge-2`; then `p9-monitors` or `p9-catchup`.
- **Any:** `p9-oracle` first; `p9-catchup` and `p9-monitors` whenever an implementer is free.
- **Verifier:** reviews every card before it lands, with mutation testing (a strategy's
  branch, a tolerance, a search path's order, a conflict's message, a role's fallback).
- `p9-merge-2` and `p9-merge-3` land in one group land, once 9.2j's sweep passes.

## Cards

Each card is one branch and one PR. "Now" means the card can start once its owner decisions are
made, without waiting for other Phase 9 cards.

| Card | Chunks | Size | Who | Needs | Owner items |
|---|---|---|---|---|---|
| `p9-oracle` | O9.1 | 1 | any | now | — (verifier-reviewed) |
| `p9-config-utils` | 9.1a–9.1c | 3, ~1,390 | A | now | P9-1 |
| `p9-merge-1` | 9.2a–9.2c | 3, ~1,330 | B | now (`p9-oracle` for its oracle tests) | P9-2 |
| `p9-merge-2` | 9.2d–9.2f | 3, ~1,500 | B | `p9-merge-1` | — |
| `p9-merge-3` | 9.2g–9.2j | 4, ~1,530 | A | `p9-merge-2`, 9.1c | — |
| `p9-monitors` | 9.3a | 1, ~330 | any | now | P9-3, P9-4, P9-5 |
| `p9-catchup` | 9.4a | 1 (tests) | any | now | — |
| `p9-dashboard` | 9.5a and the chunks it finds | 1–3 | any | Phases 5, 7 and 8 landed | P9-6, P9-7 |

8 cards, about 18 chunks.

**Can start now:** `p9-oracle`; `p9-config-utils` and `p9-merge-1` once P9-1 and P9-2 are
decided; `p9-catchup`.

**Waits for other phases:**
- **Phase 5:** merge sweeps over configs with grading transforms (`p5-transforms`).
- **Phase 6:** the binding of this phase (6.8a) waits for `p9-config-utils`, `p9-merge-3` and
  `p9-monitors`.
- **Phases 5, 7 and 8:** `p9-dashboard`.
- **Phase 10:** `ociomergeconfigs`.

## Phase 9 exit

Through OCIO's API, on Windows and Rocky Linux 9:
- the interchange spaces, built-in color spaces and processors between configs match the
  wheel's (names, processors op for op with their cache IDs, messages) over the built-in
  configs, upstream's test configs and the generated ones;
- merged configs, OCIOM files and `ConfigMergingParameters`' text are byte-exact with the
  wheel's for every strategy and option of the sweep, with the same warnings and errors;
- the system monitors and the displays made from them match the wheel's on the same machine;
- the 36 upstream tests of the phase are ported and counted in `docs/parity.md`, and P9-6's two
  are on the reviewed not-applicable list;
- `upstream-map.toml` has no `todo` or `partial` entry outside the bindings (Phase 6), the
  GPU suite and legacy GPU path (Phase 7) and the tools (Phase 10), each listed with its
  phase.
