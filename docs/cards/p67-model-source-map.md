# P67: complete model and public-header source mappings

Bookkeeping only, continuing the60-file audit after tools/ICC/Phase8. The seven
entries below were already implemented; their partial flags and incremental notes
predated later chunks. No runtime code, tests, oracle, fixture or exception changes.

| Upstream entry | Existing completion evidence |
|---|---|
| OpenColorAppHelpers.h | lib.rs exports menus, display/view helpers, LegacyViewingPipeline, MixingSlider/MixingColorSpaceManager and the merging helpers; add the missing mixing_helpers.rs target |
| OpenColorTransforms.h | all23 concrete *_transform.rs modules exist and match Transform dispatch; metadata/dynamic interfaces and grading primary/RGB-curve/tone/hue/spline value types are present; record their existing targets |
| Config.cpp | config.rs contains monitor/ICC display instantiation and every ConfigUtils/inter-config static overload in addition to the model, rules, YAML, validation and processor APIs |
| FileRules.cpp | file_rules.rs contains matching/regex, rule mutations/custom keys, filepath resolution, V1-to-V2 updates, rule and collection validation |
| OCIOYaml.cpp | ocio_yaml.rs dispatches every transform loader/saver and complete Config/rule serialization/version gates; the obsolete not-ported dispatch arm was already removed |
| OpOptimizers.cpp | all optimizer passes, inverse1D/3D replacement and prefix baking are present; op_optimizers_tests.rs already contains multi_op_prefix and dyn_properties_prefix, formerly described as future work |
| ViewingRules.cpp | viewing_rules.rs includes matching, color spaces/encodings/custom keys, collection mutations/text and validation/IsEncodingUsed; I-135 remains unchanged |

Keep existing approved Rust ownership/enum boundaries and native C++ pointer scope.
Complete SDE qualification and post-extraction legacy updates remain in P63; a source
map closure is not a claim that those release checks passed.

Relative to P66: seven partial entries become done. The source count becomes
640done+228reviewed=868/913, leaving39partial+6todo. README's file count changes;
no additional runtime implementation percentage or upstream test pass is claimed.
Normal CI remains required on the regenerated land candidate before merging.
