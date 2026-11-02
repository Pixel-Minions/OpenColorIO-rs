# P89: complete the public-header source audit

Bookkeeping closure of include/OpenColorIO/OpenColorIO.h @ v2.5.2, after P88
qualifies the last legacy post-extraction dynamic behavior. This is an audit of
the Rust API's existing implementation, not a new API or a C++ ABI promise.
Owned files: this card, that header's upstream-map.toml row, README.md and P63's
qualification checklist. Processor.cpp/h close separately in P88.

The following table accounts for the declaration blocks. Paths are relative to
crates/; test names name existing integration targets unless stated otherwise.

| Upstream lines / declarations | Existing implementation | Evidence |
|---|---|---|
| 79-129: Exception, ExceptionMissingFile | ocio-ops/src/exception.rs | ocio-ops/tests/exception_tests.rs; exact exception kinds/messages in API oracle targets |
| 136-248: versions, caches, logging, hash callbacks, environment, current config, config paths, archive extraction | ocio/src/lib.rs, caching.rs, config.rs, path_utils.rs, builtinconfigs/builtin_config_registry.rs, ocioz_archive.rs; ocio-ops/src/logging.rs, platform.rs | lib.rs versions; logging_oracle, env_oracle, context_oracle, builtin_configs_oracle, ocioz_extract_oracle; callback tests in path_utils_tests.rs and foreign_exception_tests.rs |
| 285-1672: Config | ocio/src/config.rs and its model/YAML/config-utils modules | config_oracle, config_processors_oracle, config_version_oracle, config_utils_oracle, config_from_configs_oracle, config_archivable_oracle; unmodified Python Config suite |
| 1726-1849: FileRules | ocio/src/file_rules.rs | file_rules_oracle; upstream file-rule tests |
| 1875-1969: ViewingRules | ocio/src/viewing_rules.rs | viewing_rules_oracle; upstream viewing-rule tests |
| 1984-2220: ColorSpace | ocio/src/color_space.rs | model_objects_oracle, model_objects_random_oracle; Python ColorSpace suite |
| 2241-2380: ColorSpaceSet | ocio/src/color_space_set.rs | color_space_set_tests; model/Python suites |
| 2393-2449: Look | ocio/src/look.rs | model_objects_oracle and Python Look suite |
| 2460-2533: NamedTransform | ocio/src/named_transform.rs | model_objects_oracle and Python NamedTransform suite |
| 2550-2628: ViewTransform | ocio/src/view_transform.rs | model_objects_oracle and Python ViewTransform suite |
| 2639-2800: Processor | ocio/src/processor.rs | processor_oracle, processor_cache_oracle, processor_gpu_oracle, dynamic_properties_oracle, legacy_gpu_oracle and P88 legacy_gpu_live_oracle |
| 2806-2880: CPUProcessor | ocio-ops/src/cpu_processor.rs | cpu_processor_oracle, cpu_processor_apply_oracle, cpu_processor_apply_rgb_oracle, ocio/tests/cpu_processor_tests.rs and unmodified Python buffer tests |
| 2883-2920: GPUProcessor | ocio-gpu/src/gpu_processor.rs | gpu_processor_oracle; all family GPU writer oracles; 264 upstream GPU cases; legacy targets above |
| 2924-2970: ProcessorMetadata | ocio/src/processor.rs | processor_oracle, family/config processor oracle checks |
| 2981-3090: Baker | ocio/src/baker.rs | baker_oracle, bake_formats_oracle, ctf_bake_oracle and upstream baker tests |
| 3100-3350: ImageDesc, PackedImageDesc, PlanarImageDesc | ocio-ops/src/image_desc.rs | image_desc_oracle, planar_image_desc_oracle, scanline_helper_oracle; D-2 bounds checks remain unchanged |
| 3361-3723: GpuShaderCreator | ocio-gpu/src/gpu_shader_desc.rs, gpu_shader.rs | gpu_shader_desc_oracle, gpu_shader_desc_log_oracle; Python shader tests and dynamic_properties_oracle |
| 3725-3860: GpuShaderDesc | same concrete Rust description, including resources, typed live uniform getters and textures | gpu_shader_desc_oracle, ocio/tests/gpu_shader_tests.rs, legacy_gpu_oracle and P88 same-descriptor update histories |
| 3865-3953: Context | ocio/src/context.rs | context_oracle, config/file-resolution targets and unmodified Python Context tests |
| 3963-3990: BuiltinTransformRegistry | ocio/src/transforms/builtins/builtin_transform_registry.rs | builtin_transforms_oracle and upstream registry tests |
| 3993-4030: BuiltinConfigRegistry | ocio/src/builtinconfigs/builtin_config_registry.rs | builtin_configs_oracle and built-in config tests |
| 4047-4083: SystemMonitors | ocio/src/system_monitor.rs and platform implementation | actual platform probes plus Python SystemMonitors and complete-module metadata checks on both platforms |
| 4095-4142: ConfigIOProxy | ocio/src/config_io_proxy.rs | config_tests.rs create_from_config_io_proxy/config_from_a_proxy, ocioz_read_oracle and foreign_exception_tests.rs |

GpuShaderCreator/GpuShaderDesc/GenericGpuShaderDesc already share one Rust type
under the owner's 2026-09-30 decision, documented at the top of
ocio-gpu/src/gpu_shader_desc.rs. The creator's begin/end are no-ops in that
implementation; the resource-key callback of the abstract C++ overload cannot
affect this concrete description. This audit introduces no additional scope
boundary. Existing approved enum, buffer, ownership and native Python extension
boundaries remain in docs/deviations.md. No declaration becomes newly N/A.

The resulting source ledger is 685 done plus 228 reviewed not applicable: all
913 files accounted for, zero partial/todo. This closes source accounting only.
The upstream test totals remain 1,189 C++ plus two reviewed exceptions, 264 GPU
and 384 unmodified Python tests. Both-platform normal CI on the exact regenerated
land head must pass before main advances. Complete SDE CPU qualification,
unexplained emulator failures, the external release audit and release packaging
remain open. Do not describe the port as finished from this source count.

The owner's final-matrix hold takes precedence over older P63 continuation notes:
all three files must land, snb must be clean, and the owner must explicitly confirm
before the final full SDE matrix starts. Native CI is the merge gate; no new SDE
run is authorized by this card.
