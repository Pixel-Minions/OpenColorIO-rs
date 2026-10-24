// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The OCIOM file format, which holds a config merger's parameters: a port of
//! `src/OpenColorIO/apphelpers/mergeconfigs/OCIOMYaml.h` and `OCIOMYaml.cpp` @ v2.5.2.

use ocio_ops::exception::Exception;
use ocio_ops::parse_utils::split_string_env_style;
use ocio_ops::utils::string_utils::c_str;

use crate::apphelpers::mergeconfigs::merge_configs_helpers::{ConfigMerger, MergeStrategies};
use crate::yaml_cpp::emitter::Emitter;
use crate::yaml_cpp::emitter_manip::EmitterManip::{
    BeginMap, BeginSeq, Block, EndMap, EndSeq, Flow, Key, Newline, Value,
};

/// The name of a strategy, as an OCIOM file and the parameters' text write it. (Upstream's
/// `"Unknown"`, for a value outside the enumeration, can't happen with the Rust enum, nor
/// through the Python binding, whose enum takes only its values.)
///
/// Port of `OCIOMYaml::EnumToStrategyString` (OCIOMYaml.cpp:492-518 @ v2.5.2).
pub(crate) fn enum_to_strategy_string(strategy: MergeStrategies) -> &'static str {
    match strategy {
        MergeStrategies::PreferInput => "PreferInput",
        MergeStrategies::PreferBase => "PreferBase",
        MergeStrategies::InputOnly => "InputOnly",
        MergeStrategies::BaseOnly => "BaseOnly",
        MergeStrategies::Remove => "Remove",
        MergeStrategies::Unspecified => "Unspecified",
    }
}

/// Why the writer stopped: an exception (which `ConfigMerger::serialize` reports after "Error
/// building YAML: "), or parameters that are `None`, which upstream dereferences
/// (docs/improvements.md, U-176).
#[derive(Debug)]
pub(crate) enum WriteError {
    Exception(Exception),
    /// The index of the parameters.
    NullParams(usize),
}

impl From<Exception> for WriteError {
    fn from(e: Exception) -> Self {
        WriteError::Exception(e)
    }
}

/// A sequence of strings, as `out << std::vector<std::string>` writes it (stlemitter.h:16-28):
/// each as its bytes.
fn put_strings(out: &mut Emitter, values: &[Vec<u8>]) {
    out.put(BeginSeq);
    for v in values {
        out.put(v.as_slice());
    }
    out.put(EndSeq);
}

/// A list of the overrides (active displays, active views, inactive color spaces) as the
/// writer splits it: `SplitStringEnvStyle` of the text when it isn't empty, which fails for a
/// text that opens a double quote it doesn't close before a separator.
fn split_list(list: &[u8]) -> Result<Vec<Vec<u8>>, WriteError> {
    if list.is_empty() {
        return Ok(Vec::new());
    }
    Ok(split_string_env_style(list)?)
}

/// A section's strategy, as `params` writes it: `key: {strategy: name}` in the block style.
fn save_strategy(out: &mut Emitter, key: &str, strategy: MergeStrategies) {
    out.put(Key).put(key);
    out.put(Value).put(BeginMap);
    out.put(Key)
        .put("strategy")
        .put(Value)
        .put(enum_to_strategy_string(strategy));
    out.put(EndMap);
}

/// The merger as the document of an OCIOM file: its version, its search paths, then each
/// merge under its output name, with its base and input, options, overrides and each section's
/// strategy (resolved by the default strategy).
///
/// Port of `save(YAML::Emitter&, const ConfigMerger&)` (OCIOMYaml.cpp:523-658 @ v2.5.2).
fn save(out: &mut Emitter, merger: &ConfigMerger) -> Result<(), WriteError> {
    let parser_major_version = merger.major_version();
    let ss = format!("{parser_major_version}.{}", merger.minor_version());

    out.put(Block);
    out.put(BeginMap);
    out.put(Key)
        .put("ociom_version")
        .put(Value)
        .put(ss.as_str());
    out.put(Key).put("search_path");
    out.put(Value).put(BeginSeq);
    for i in 0..merger.num_search_paths() {
        out.put(Value).put(merger.search_path(i).as_slice());
    }
    out.put(EndSeq);
    out.put(Newline);

    out.put(Key).put("merge");
    out.put(Value).put(BeginMap);

    for mp in 0..merger.num_config_merging_parameters() {
        // Serialized every merge section.
        let p = merger
            .params(mp)
            .ok_or(WriteError::NullParams(mp as usize))?;
        out.put(Key).put(p.output_name().as_slice());
        out.put(Value).put(BeginMap);

        out.put(Key)
            .put("base")
            .put(Value)
            .put(p.base_config_name().as_slice());
        out.put(Key)
            .put("input")
            .put(Value)
            .put(p.input_config_name().as_slice());
        out.put(Newline);

        out.put(Key).put("options");
        out.put(Value).put(BeginMap);
        out.put(Key)
            .put("input_family_prefix")
            .put(Value)
            .put(p.input_family_prefix().as_slice());
        out.put(Key)
            .put("base_family_prefix")
            .put(Value)
            .put(p.base_family_prefix().as_slice());
        out.put(Key)
            .put("input_first")
            .put(Value)
            .put(p.is_input_first());
        out.put(Key)
            .put("error_on_conflict")
            .put(Value)
            .put(p.is_error_on_conflict());
        out.put(Key)
            .put("default_strategy")
            .put(Value)
            .put(enum_to_strategy_string(p.default_strategy()));
        out.put(Key)
            .put("avoid_duplicates")
            .put(Value)
            .put(p.is_avoid_duplicates());
        out.put(Key)
            .put("adjust_input_reference_space")
            .put(Value)
            .put(p.is_adjust_input_reference_space());
        // End of options section.
        out.put(EndMap);
        out.put(Newline);

        out.put(Key).put("overrides");
        out.put(Value).put(BeginMap);
        out.put(Key).put("name").put(Value).put(p.name().as_slice());
        out.put(Key)
            .put("description")
            .put(Value)
            .put(p.description().as_slice());
        out.put(Key)
            .put("search_path")
            .put(Value)
            .put(p.search_path().as_slice());

        out.put(Key).put("environment");
        out.put(Value).put(BeginMap);
        for i in 0..p.num_environment_vars() {
            let name = p.environment_var(i);
            out.put(Key).put(name.as_slice());
            out.put(Value).put(p.environment_var_value(i).as_slice());
        }
        out.put(EndMap);
        out.put(Newline);

        out.put(Key).put("active_displays");
        let active_displays = split_list(&p.active_displays())?;
        out.put(Value).put(Flow);
        put_strings(out, &active_displays);
        out.put(Newline);

        out.put(Key).put("active_views");
        let active_views = split_list(&p.active_views())?;
        out.put(Value).put(Flow);
        put_strings(out, &active_views);

        out.put(Key).put("inactive_colorspaces");
        let inactive_colorspaces = split_list(&p.inactive_color_spaces())?;
        out.put(Value).put(Flow);
        put_strings(out, &inactive_colorspaces);

        // End of overrides section.
        out.put(EndMap);
        out.put(Newline);

        out.put(Key).put("params");
        out.put(Value).put(BeginMap);

        save_strategy(out, "roles", p.roles());
        save_strategy(out, "file_rules", p.file_rules());
        save_strategy(out, "display-views", p.display_views());
        save_strategy(out, "view_transforms", p.view_transforms());
        save_strategy(out, "looks", p.looks());
        save_strategy(out, "colorspaces", p.colorspaces());
        save_strategy(out, "named_transforms", p.named_transforms());

        // End of params section.
        out.put(EndMap);

        // End of the current merge section.
        out.put(EndMap);
    }

    // End of the merges.
    out.put(EndMap);

    out.put(EndMap);
    Ok(())
}

/// The merger as an OCIOM file: the emitter's text, up to its first NUL (`ostream <<
/// out.c_str()`).
///
/// Port of `OCIOMYaml::write` (OCIOMYaml.cpp:662-667 @ v2.5.2).
pub(crate) fn write(merger: &ConfigMerger) -> Result<Vec<u8>, WriteError> {
    let mut out = Emitter::new();
    save(&mut out, merger)?;
    Ok(c_str(out.c_str()).to_vec())
}
