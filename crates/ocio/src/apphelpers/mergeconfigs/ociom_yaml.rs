// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The OCIOM file format, which holds a config merger's parameters: a port of
//! `src/OpenColorIO/apphelpers/mergeconfigs/OCIOMYaml.h` and `OCIOMYaml.cpp` @ v2.5.2.

use std::collections::HashSet;

use ocio_ops::exception::Exception;
use ocio_ops::logging::log_warning;
use ocio_ops::parse_utils::split_string_env_style;
use ocio_ops::utils::pystring::os_path;
use ocio_ops::utils::string_utils::{c_str, join, split};

use crate::apphelpers::mergeconfigs::merge_configs_helpers::{
    ConfigMerger, ConfigMergingParameters, MergeStrategies,
};
use crate::ocio_yaml::{LoadError, LoadResult, load_string, load_string_vec, stoi};
use crate::path_utils::abs_path;
use crate::yaml_cpp::emitter::Emitter;
use crate::yaml_cpp::emitter_manip::EmitterManip::{
    BeginMap, BeginSeq, Block, EndMap, EndSeq, Flow, Key, Newline, Value,
};
use crate::yaml_cpp::node::{Node, NodeType};
use crate::yaml_cpp::parse::load as yaml_load;
use crate::yaml_cpp::stream::IStream;

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

/// The strategy an OCIOM file names (`PreferInput`, `PreferBase`, `InputOnly`, `BaseOnly`,
/// `Remove`, spelled exactly so), or [`MergeStrategies::Unspecified`] for any other name
/// (`Unspecified` too). The name ends at its first NUL.
///
/// Upstream looks the name up in a static map that `OCIOMYaml`'s constructor fills the first
/// time, without a lock (docs/improvements.md, U-177); its callers all construct one first. The
/// port matches the names directly.
///
/// Port of `OCIOMYaml::StrategyStringToEnum` and `OCIOMYaml::OCIOMYaml`
/// (OCIOMYaml.cpp:22-34, 481-490 @ v2.5.2).
pub(crate) fn strategy_string_to_enum(enum_str: &[u8]) -> MergeStrategies {
    match c_str(enum_str) {
        b"PreferInput" => MergeStrategies::PreferInput,
        b"PreferBase" => MergeStrategies::PreferBase,
        b"InputOnly" => MergeStrategies::InputOnly,
        b"BaseOnly" => MergeStrategies::BaseOnly,
        b"Remove" => MergeStrategies::Remove,
        _ => MergeStrategies::Unspecified,
    }
}

/// `node.Mark().line + 1`, as upstream prints it (an `int` addition). A zombie throws
/// `InvalidNode`.
fn line_of(node: &Node) -> LoadResult<i32> {
    Ok(node.mark()?.line.wrapping_add(1))
}

/// The error "At line N, the value of the property 'key' from 'nodeName' failed: msg", the line
/// of the key; or the error that reading the key or its mark gives.
///
/// Port of `OCIOMYaml::throwValueError` (OCIOMYaml.cpp:68-81 @ v2.5.2).
fn throw_value_error(node_name: &[u8], key: &Node, msg: &[u8]) -> LoadError {
    let key_name = match load_string(key) {
        Ok(name) => name,
        Err(err) => return err,
    };
    let line = match line_of(key) {
        Ok(line) => line,
        Err(err) => return err,
    };
    let mut os = format!("At line {line}, the value of the property '").into_bytes();
    os.extend_from_slice(&key_name);
    os.extend_from_slice(b"' from '");
    os.extend_from_slice(node_name);
    os.extend_from_slice(b"' failed: ");
    os.extend_from_slice(msg);
    LoadError::Ocio(Exception::new(os))
}

/// Fails on the first key of the map that repeats an earlier one (keys compared as strings):
/// "Key-value pair with key 'key' specified more than once. ", through [`throw_value_error`].
/// A key that isn't a string throws yaml-cpp's bad conversion; the elements of a sequence, whose
/// keys are invalid nodes, throw yaml-cpp's invalid node.
///
/// Port of `OCIOMYaml::CheckDuplicates` (OCIOMYaml.cpp:83-102 @ v2.5.2).
fn check_duplicates(node: &Node) -> LoadResult<()> {
    let mut keyset = HashSet::new();
    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if !keyset.contains(&key) {
            keyset.insert(key);
        } else {
            let mut os = b"Key-value pair with key '".to_vec();
            os.extend_from_slice(&key);
            os.extend_from_slice(b"' specified more than once. ");
            return Err(throw_value_error(node.tag()?, &iter.first, &os));
        }
    }
    Ok(())
}

/// "The value 'name' is not recognized. ", for a strategy's name.
fn not_recognized(strategy: &[u8]) -> Vec<u8> {
    let mut os = b"The value '".to_vec();
    os.extend_from_slice(strategy);
    os.extend_from_slice(b"' is not recognized. ");
    os
}

/// A section's strategy: the map under the section's key `pnode`, whose last `strategy` names
/// it (its other keys are read as strings and ignored). A value that isn't a map, and a name
/// that isn't a strategy's (or no `strategy` key), fail.
///
/// Port of `OCIOMYaml::genericStrategyHandler` (OCIOMYaml.cpp:104-133 @ v2.5.2).
fn generic_strategy_handler(pnode: &Node, node: &Node) -> LoadResult<MergeStrategies> {
    if node.node_type()? != NodeType::Map {
        return Err(throw_value_error(
            node.tag()?,
            pnode,
            b"The value type of a property 'strategy' needs to be a map.",
        ));
    }

    let mut strategy = Vec::new();
    for it in node.iter() {
        let prop = it.first.as_::<Vec<u8>>()?;
        let value = it.second.as_::<Vec<u8>>()?;

        if prop == b"strategy" {
            strategy = value;
        }
    }

    let srategy_enum = strategy_string_to_enum(&strategy);
    if srategy_enum == MergeStrategies::Unspecified {
        return Err(throw_value_error(
            node.tag()?,
            pnode,
            &not_recognized(&strategy),
        ));
    }
    Ok(srategy_enum)
}

/// A merge's `options`: the family prefixes, the flags (`assume_common_reference_space` as the
/// opposite of `adjust_input_reference_space`) and the default strategy; other keys are
/// ignored. Values are read as strings or booleans, as yaml-cpp converts them.
///
/// Port of `OCIOMYaml::loadOptions` (OCIOMYaml.cpp:135-185 @ v2.5.2).
fn load_options(node: &Node, params: &ConfigMergingParameters) -> LoadResult<()> {
    check_duplicates(node)?;

    for it in node.iter() {
        let key = it.first.as_::<Vec<u8>>()?;
        match key.as_slice() {
            b"input_family_prefix" => {
                params.set_input_family_prefix(it.second.as_::<Vec<u8>>()?);
            }
            b"base_family_prefix" => {
                params.set_base_family_prefix(it.second.as_::<Vec<u8>>()?);
            }
            b"input_first" => params.set_input_first(it.second.as_::<bool>()?),
            b"error_on_conflict" => params.set_error_on_conflict(it.second.as_::<bool>()?),
            b"avoid_duplicates" => params.set_avoid_duplicates(it.second.as_::<bool>()?),
            b"adjust_input_reference_space" => {
                params.set_adjust_input_reference_space(it.second.as_::<bool>()?);
            }
            b"assume_common_reference_space" => {
                // Need to support this as a synonym for adjust_input_reference_space.
                params.set_adjust_input_reference_space(!it.second.as_::<bool>()?);
            }
            b"default_strategy" => {
                let strategy = it.second.as_::<Vec<u8>>()?;
                let srategy_enum = strategy_string_to_enum(&strategy);
                if srategy_enum == MergeStrategies::Unspecified {
                    return Err(throw_value_error(
                        node.tag()?,
                        &it.first,
                        &not_recognized(&strategy),
                    ));
                }
                params.set_default_strategy(srategy_enum);
            }
            _ => {}
        }
    }
    Ok(())
}

/// A merge's `overrides`: the name, the description, the search path (a string, or a list of
/// paths added in turn), the environment (a map), the active displays and views and the
/// inactive color spaces (lists, joined with `", "`); null values and other keys are ignored.
///
/// Port of `OCIOMYaml::loadOverrides` (OCIOMYaml.cpp:187-262 @ v2.5.2).
fn load_overrides(node: &Node, params: &ConfigMergingParameters) -> LoadResult<()> {
    check_duplicates(node)?;

    for it in node.iter() {
        let key = it.first.as_::<Vec<u8>>()?;

        if it.second.is_null()? || !it.second.is_defined() {
            continue;
        }

        match key.as_slice() {
            b"name" => params.set_name(load_string(&it.second)?),
            b"description" => params.set_description(load_string(&it.second)?),
            b"search_path" => {
                if it.second.size()? == 0 {
                    params.set_search_path(load_string(&it.second)?);
                } else {
                    for path in load_string_vec(&it.second)? {
                        params.add_search_path(path);
                    }
                }
            }
            b"environment" => {
                if it.second.node_type()? != NodeType::Map {
                    return Err(throw_value_error(
                        node.tag()?,
                        &it.first,
                        b"The value type of key 'environment' needs to be a map.",
                    ));
                }
                for env in it.second.iter() {
                    let k = env.first.as_::<Vec<u8>>()?;
                    let v = env.second.as_::<Vec<u8>>()?;
                    params.add_environment_var(k, Some(&v));
                }
            }
            b"active_displays" => {
                let display = load_string_vec(&it.second)?;
                params.set_active_displays(join(&display, b','))?;
            }
            b"active_views" => {
                let view = load_string_vec(&it.second)?;
                params.set_active_views(join(&view, b','))?;
            }
            b"inactive_colorspaces" => {
                let inactive_css = load_string_vec(&it.second)?;
                params.set_inactive_color_spaces(join(&inactive_css, b','));
            }
            _ => {}
        }
    }
    Ok(())
}

/// A merge's `params`: each section's strategy ([`generic_strategy_handler`]); another key is
/// logged as a warning, "Unsupported property in merge params: key".
///
/// Port of `OCIOMYaml::loadParams` (OCIOMYaml.cpp:264-305 @ v2.5.2).
fn load_params(node: &Node, params: &ConfigMergingParameters) -> LoadResult<()> {
    // Check for duplicates in params.
    check_duplicates(node)?;

    for it in node.iter() {
        let key = it.first.as_::<Vec<u8>>()?;
        let set: fn(&ConfigMergingParameters, MergeStrategies) = match key.as_slice() {
            b"roles" => ConfigMergingParameters::set_roles,
            b"file_rules" => ConfigMergingParameters::set_file_rules,
            b"display-views" => ConfigMergingParameters::set_display_views,
            b"view_transforms" => ConfigMergingParameters::set_view_transforms,
            b"looks" => ConfigMergingParameters::set_looks,
            b"colorspaces" => ConfigMergingParameters::set_colorspaces,
            b"named_transforms" => ConfigMergingParameters::set_named_transforms,
            _ => {
                let mut msg = b"Unsupported property in merge params: ".to_vec();
                msg.extend_from_slice(&key);
                log_warning(msg);
                continue;
            }
        };
        set(params, generic_strategy_handler(&it.first, &it.second)?);
    }
    Ok(())
}

/// The OCIOM document `node` into `merger`, which holds one parameters object per merge
/// ([`count_merges`]): the version (`major[.minor]`, read with `std::stoi`, at most 1.0), the
/// search paths (a string split on `:`, or a list), and each merge of `merge` under its output
/// name, with its `base`, `input`, `options`, `overrides` and `params`. Null values and other
/// keys are ignored. After each top-level key with a value, the working directory is set to the
/// directory of `filename` when it names a file.
///
/// Port of `OCIOMYaml::load(const YAML::Node&, ConfigMergerRcPtr&, const char*)`
/// (OCIOMYaml.cpp:307-430 @ v2.5.2).
fn load(node: &Node, merger: &mut ConfigMerger, filename: &[u8]) -> LoadResult<()> {
    check_duplicates(node)?;

    // Parse all properties.
    for it in node.iter() {
        let key = it.first.as_::<Vec<u8>>()?;

        if it.second.is_null()? || !it.second.is_defined() {
            continue;
        }

        match key.as_slice() {
            b"ociom_version" => {
                let version = load_string(&node.get(&b"ociom_version"[..])?)?;
                let results = split(&version, b'.');

                // `std::stoi`, its `int` taken as `unsigned int`.
                let parsed = match results.as_slice() {
                    [major] => stoi(major).map(|major| (major as u32, 0)),
                    [major, minor] => stoi(major)
                        .and_then(|major| stoi(minor).map(|minor| (major as u32, minor as u32))),
                    _ => None,
                };

                match parsed {
                    Some((major, minor)) => merger.set_version(major, minor),
                    None => {
                        let mut msg = b"The value '".to_vec();
                        msg.extend_from_slice(&version);
                        msg.extend_from_slice(b"' is not a valid OCIOM version.");
                        return Err(throw_value_error(it.second.tag()?, &it.first, &msg));
                    }
                }

                if merger.major_version() > 1 || merger.minor_version() > 0 {
                    return Err(throw_value_error(
                        it.second.tag()?,
                        &it.first,
                        b"The highest supported OCIOM file version is 1.0.",
                    ));
                }
            }
            b"search_path" => {
                if it.second.size()? == 0 {
                    let stringval = load_string(&it.second)?;
                    merger.set_search_path(stringval);
                } else {
                    for path in load_string_vec(&it.second)? {
                        merger.add_search_path(path);
                    }
                }
            }
            b"merge" => {
                if it.second.node_type()? != NodeType::Map {
                    return Err(throw_value_error(
                        it.second.tag()?,
                        &it.first,
                        b"The value type of the key 'merge' needs to be a map.",
                    ));
                }

                for (merges_counter, merge_it) in it.second.iter().enumerate() {
                    let merged_config_name = merge_it.first.as_::<Vec<u8>>()?;
                    let params = merger
                        .params(merges_counter as i32)
                        .expect("count_merges made the parameters of each merge");
                    params.set_output_name(&merged_config_name);

                    for params_it in merge_it.second.iter() {
                        let key = params_it.first.as_::<Vec<u8>>()?;

                        match key.as_slice() {
                            b"base" => {
                                params.set_base_config_name(params_it.second.as_::<Vec<u8>>()?);
                            }
                            b"input" => {
                                params.set_input_config_name(params_it.second.as_::<Vec<u8>>()?);
                            }
                            b"options" => load_options(&params_it.second, &params)?,
                            b"overrides" => load_overrides(&params_it.second, &params)?,
                            b"params" => load_params(&params_it.second, &params)?,
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }

        if !filename.is_empty() {
            // Working directory defaults to the directory of the OCIOM file.
            let realfilename = abs_path(filename)?;
            let configrootdir = os_path::dirname(&realfilename);
            merger.set_working_dir(configrootdir);
        }
    }
    Ok(())
}

/// The number of merges in the document `node`: the pairs of its `merge` map (when it has a
/// value). Fails on a key that repeats in the document or in `merge`, and on a `merge` that
/// isn't a map.
///
/// Port of `OCIOMYaml::countMerges` (OCIOMYaml.cpp:432-463 @ v2.5.2).
fn count_merges(node: &Node) -> LoadResult<usize> {
    let mut num_of_merges = 0;

    check_duplicates(node)?;

    // Parse all properties.
    for it in node.iter() {
        let key = it.first.as_::<Vec<u8>>()?;

        if it.second.is_null()? || !it.second.is_defined() {
            continue;
        }

        if key == b"merge" {
            if it.second.node_type()? != NodeType::Map {
                return Err(throw_value_error(
                    it.second.tag()?,
                    &it.first,
                    b"The value type of the key 'merge' needs to be a map.",
                ));
            }

            check_duplicates(&it.second)?;

            num_of_merges += it.second.iter().count();
        }
    }

    Ok(num_of_merges)
}

/// The merger an OCIOM file's text `istream` describes: yaml-cpp's document, the number of
/// merges, a merger with that many parameters objects (each made as `Config::Create` makes
/// their override config), then the document read into it. What fails becomes "Error: Loading
/// the OCIOM Merge parameters '<filepath>' failed. <what()>".
///
/// Port of `ConfigMerger::Impl::Read` (MergeConfigsHelpers.cpp:413-447 @ v2.5.2), with
/// `OCIOMYaml::load` and `countMerges`. (`OCIOMYaml::read`, OCIOMYaml.cpp:465-479, has no
/// caller.)
pub(crate) fn read(istream: IStream<'_>, filepath: &[u8]) -> Result<ConfigMerger, Exception> {
    let read = || -> LoadResult<ConfigMerger> {
        let node = yaml_load(istream)?;
        let num_of_merges = count_merges(&node)?;

        let mut merger = ConfigMerger::new();

        // Create the number of ConfigMergingParameters needed.
        for _ in 0..num_of_merges {
            merger.add_params(Some(ConfigMergingParameters::new()?));
        }

        load(&node, &mut merger, filepath)?;
        Ok(merger)
    };
    read().map_err(|e| failed(filepath, &e.what()))
}

/// "Error: Loading the OCIOM Merge parameters '<filepath>' failed. <what>".
pub(crate) fn failed(filepath: &[u8], what: &[u8]) -> Exception {
    let mut os = b"Error: Loading the OCIOM Merge parameters ".to_vec();
    os.push(b'\'');
    os.extend_from_slice(c_str(filepath));
    os.extend_from_slice(b"' failed. ");
    os.extend_from_slice(what);
    Exception::new(os)
}

#[cfg(test)]
#[path = "ociom_yaml_oracle_tests.rs"]
mod oracle_tests;
