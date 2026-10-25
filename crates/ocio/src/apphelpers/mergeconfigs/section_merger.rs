// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The section mergers of a config merge: a port of
//! `src/OpenColorIO/apphelpers/mergeconfigs/SectionMerger.h` and `SectionMerger.cpp` @ v2.5.2.
//!
//! A merge copies the base config, then lets one merger per section of the config change that
//! copy, each by its strategy (`ConfigMerger::mergeConfigs`, `ConfigMergingHelpers::MergeConfigs`,
//! MergeConfigsHelpers.cpp:681-760): [`GeneralMerger`], [`RolesMerger`], [`FileRulesMerger`],
//! the displays and views, the view transforms, [`LooksMerger`], the color spaces and the named
//! transforms.
//!
//! Upstream's mergers are classes over [`MergeHandlerOptions`]: a base class, `SectionMerger`,
//! whose `merge` calls the handler of the section's strategy, and a subclass per section that
//! overrides the handlers it supports. Here the base class's state is [`SectionMerger`], and its
//! virtual functions are the trait [`Section`], which each merger implements.
//!
//! `SectionMerger.h` declares `mergeStringVecWithoutDuplicate` and `inputFirstMergeStringVec`
//! (`SectionMerger.h:112-116` @ v2.5.2), which OCIO 2.5.2 neither defines nor calls.

// `mergeConfigs` and `MergeConfigs` (9.2j, p9-merge-3) run the mergers; until then only the
// tests do.
#![cfg_attr(not(test), allow(dead_code))]

use ocio_ops::exception::{Exception, ExceptionKind, Result};
use ocio_ops::logging::log_warning;
use ocio_ops::platform::strcasecmp;

use crate::apphelpers::mergeconfigs::merge_configs_helpers::{
    ConfigMergingParameters, MergeStrategies,
};
use crate::config::Config;
use crate::custom_keys::CustomKeysContainer;
use crate::file_rules::FileRules;

/// The configs and parameters of a merge.
///
/// Port of `MergeHandlerOptions` (SectionMerger.h:22-28 @ v2.5.2). Upstream holds references to
/// shared pointers, any of which may be null; the configs here are references, which can't be,
/// and the parameters an `Option` (Python's `MergeConfigs(None, base, input)` reaches the
/// mergers with null parameters).
pub(crate) struct MergeHandlerOptions<'a> {
    /// `baseConfig`.
    pub(crate) base_config: &'a Config,
    /// `inputConfig`.
    pub(crate) input_config: &'a Config,
    /// `params`.
    pub(crate) params: Option<&'a ConfigMergingParameters>,
    /// `mergedConfig`: a copy of the base config, which the mergers change.
    pub(crate) merged_config: &'a mut Config,
}

/// The state every section merger has.
///
/// Port of `SectionMerger`'s members (SectionMerger.h:30-120 @ v2.5.2). `m_inputToBaseGtScene`
/// and `m_inputToBaseGtDisplay` belong to the view transforms and color spaces mergers (9.2g,
/// 9.2h).
pub(crate) struct SectionMerger<'o, 'a> {
    /// `m_baseConfig`.
    pub(crate) base_config: &'a Config,
    /// `m_inputConfig`.
    pub(crate) input_config: &'a Config,
    /// `m_mergedConfig`.
    pub(crate) merged_config: &'o mut Config,
    /// `m_params`.
    pub(crate) params: &'a ConfigMergingParameters,
    /// `m_strategy`, which each merger's constructor sets.
    pub(crate) strategy: MergeStrategies,
}

impl<'o, 'a> SectionMerger<'o, 'a> {
    /// The state of a merger over `options`, its strategy unspecified until the merger's own
    /// constructor sets it. Fails for null parameters.
    ///
    /// Port of `SectionMerger::SectionMerger` (SectionMerger.h:34-42 @ v2.5.2).
    pub(crate) fn new(options: &'o mut MergeHandlerOptions<'a>) -> Result<SectionMerger<'o, 'a>> {
        let Some(params) = options.params else {
            return Err(Exception::new(
                "SectionMerger arguments were not initialized.",
            ));
        };
        Ok(SectionMerger {
            base_config: options.base_config,
            input_config: options.input_config,
            merged_config: &mut *options.merged_config,
            params,
            strategy: MergeStrategies::Unspecified,
        })
    }

    /// Logs `s` as a warning, or with `must_throw` (the option to stop at a conflict), returns
    /// it as the error.
    ///
    /// Port of `SectionMerger::notify` (SectionMerger.cpp:62-74 @ v2.5.2).
    pub(crate) fn notify(&self, s: &[u8], must_throw: bool) -> Result<()> {
        if !must_throw {
            // By logging, we can see all errors.
            log_warning(s);
            Ok(())
        } else {
            // By throwing, we only get the first error on conflict, but also stop the merge.
            Err(Exception::new(s))
        }
    }
}

/// A section's strategy: its own, or the default strategy where it is unspecified.
///
/// Port of the constructors of `RolesMerger`, `FileRulesMerger`, ... (SectionMerger.h:144-155,
/// 171-182, ... @ v2.5.2), whose getters resolve the default already.
fn section_strategy(
    strategy: MergeStrategies,
    params: &ConfigMergingParameters,
) -> MergeStrategies {
    if strategy != MergeStrategies::Unspecified {
        strategy
    } else {
        params.default_strategy()
    }
}

/// Logs that the section `name` doesn't support the strategy `strategy`.
///
/// Port of `SectionMerger::handlePreferInput`, ... (SectionMerger.h:74-97 @ v2.5.2).
fn log_unsupported(name: &str, strategy: &str) {
    log_warning(format!(
        "{name} section does not support strategy '{strategy}'"
    ));
}

/// What a section merger does: upstream's virtual functions of `SectionMerger`.
///
/// Port of `SectionMerger` (SectionMerger.h:30-120 @ v2.5.2).
pub(crate) trait Section<'o, 'a> {
    /// The merger's state.
    fn state(&self) -> &SectionMerger<'o, 'a>;

    /// The section's name, in messages.
    ///
    /// Port of `SectionMerger::getName` (SectionMerger.h:119 @ v2.5.2).
    fn name(&self) -> &'static str;

    /// Port of `SectionMerger::handlePreferInput` (SectionMerger.h:74-77 @ v2.5.2).
    fn handle_prefer_input(&mut self) -> Result<()> {
        log_unsupported(self.name(), "PreferInput");
        Ok(())
    }

    /// Port of `SectionMerger::handlePreferBase` (SectionMerger.h:79-82 @ v2.5.2).
    fn handle_prefer_base(&mut self) -> Result<()> {
        log_unsupported(self.name(), "PreferBase");
        Ok(())
    }

    /// Port of `SectionMerger::handleInputOnly` (SectionMerger.h:84-87 @ v2.5.2).
    fn handle_input_only(&mut self) -> Result<()> {
        log_unsupported(self.name(), "InputOnly");
        Ok(())
    }

    /// Port of `SectionMerger::handleBaseOnly` (SectionMerger.h:89-92 @ v2.5.2).
    fn handle_base_only(&mut self) -> Result<()> {
        log_unsupported(self.name(), "BaseOnly");
        Ok(())
    }

    /// Port of `SectionMerger::handleRemove` (SectionMerger.h:94-97 @ v2.5.2).
    fn handle_remove(&mut self) -> Result<()> {
        log_unsupported(self.name(), "Remove");
        Ok(())
    }

    /// Merges the section by its strategy; nothing for an unspecified one.
    ///
    /// Port of `SectionMerger::merge` (SectionMerger.h:44-68 @ v2.5.2).
    fn merge(&mut self) -> Result<()> {
        match self.state().strategy {
            MergeStrategies::PreferInput => self.handle_prefer_input(),
            MergeStrategies::PreferBase => self.handle_prefer_base(),
            MergeStrategies::InputOnly => self.handle_input_only(),
            MergeStrategies::BaseOnly => self.handle_base_only(),
            MergeStrategies::Remove => self.handle_remove(),
            // Nothing to do.
            MergeStrategies::Unspecified => Ok(()),
        }
    }
}

//
// Important implementation note: All of the section merger code assumes the merged config
// is initialized from the base config.
//

/////////////////////////////////// GeneralMerger ////////////////////////////////////////

/// Sets the merged config's version to the higher of the input's and the base's, and at least
/// 2.0.
///
/// Port of `setMergedConfigVersion` (SectionMerger.cpp:86-113 @ v2.5.2), whose arithmetic is
/// `unsigned int`.
fn set_merged_config_version(
    config: &mut Config,
    input_major: u32,
    input_minor: u32,
    base_major: u32,
    base_minor: u32,
) -> Result<()> {
    let version = |major: u32, minor: u32| major.wrapping_mul(100).wrapping_add(minor);

    let mut major = base_major;
    let mut minor = base_minor;

    // Use the higher of the input or base version.
    if version(input_major, input_minor) > version(base_major, base_minor) {
        major = input_major;
        minor = input_minor;
    }

    // A merge always produces at least a v2 config.
    const MIN_MAJOR: u32 = 2;
    const MIN_MINOR: u32 = 0;

    if version(MIN_MAJOR, MIN_MINOR) > version(major, minor) {
        major = MIN_MAJOR;
        minor = MIN_MINOR;
    }

    config.set_version(major, minor)
}

/// Merges the config's name, description, version and luma coefficients, by the default
/// strategy (the properties of no section).
///
/// Port of `GeneralMerger` (SectionMerger.h:122-139, SectionMerger.cpp:118-278 @ v2.5.2).
pub(crate) struct GeneralMerger<'o, 'a>(SectionMerger<'o, 'a>);

impl<'o, 'a> GeneralMerger<'o, 'a> {
    /// Port of `GeneralMerger::GeneralMerger` (SectionMerger.h:125-130 @ v2.5.2).
    pub(crate) fn new(options: &'o mut MergeHandlerOptions<'a>) -> Result<Self> {
        let mut m = SectionMerger::new(options)?;
        // This merger always use the default strategy as this is for properties
        // that are not linked to a specific section.
        m.strategy = m.params.default_strategy();
        Ok(GeneralMerger(m))
    }

    /// The name and description of the overrides where they aren't empty, else of `from`; the
    /// version of both configs; the luma coefficients of `from`.
    ///
    /// The body the four handlers share (SectionMerger.cpp:118-278 @ v2.5.2): each takes the
    /// name and description of the overrides first, then of one config, and the luma of the
    /// same config.
    fn merge_from(&mut self, from_input: bool) -> Result<()> {
        let m = &mut self.0;
        let from = if from_input {
            m.input_config
        } else {
            m.base_config
        };

        // Set the config Name.
        let name = m.params.name();
        if !name.is_empty() {
            // Use name from override.
            m.merged_config.set_name(&name);
        } else {
            m.merged_config.set_name(from.name());
        }

        // Set the config description.
        let desc = m.params.description();
        if !desc.is_empty() {
            // Use description from override.
            m.merged_config.set_description(&desc);
        } else {
            m.merged_config.set_description(from.description());
        }

        // Use the higher value for ocio_profile_version.
        set_merged_config_version(
            m.merged_config,
            m.input_config.major_version(),
            m.input_config.minor_version(),
            m.base_config.major_version(),
            m.base_config.minor_version(),
        )?;

        let rgb = from.default_luma_coefs();
        m.merged_config.set_default_luma_coefs(&rgb);
        Ok(())
    }
}

impl<'o, 'a> Section<'o, 'a> for GeneralMerger<'o, 'a> {
    fn state(&self) -> &SectionMerger<'o, 'a> {
        &self.0
    }

    /// Port of `GeneralMerger::getName` (SectionMerger.h:133 @ v2.5.2).
    fn name(&self) -> &'static str {
        "General"
    }

    /// The input's name, description and luma.
    ///
    /// Port of `GeneralMerger::handlePreferInput` (SectionMerger.cpp:118-156 @ v2.5.2).
    fn handle_prefer_input(&mut self) -> Result<()> {
        self.merge_from(true)
    }

    /// The base's name, description and luma.
    ///
    /// Port of `GeneralMerger::handlePreferBase` (SectionMerger.cpp:158-196 @ v2.5.2).
    fn handle_prefer_base(&mut self) -> Result<()> {
        self.merge_from(false)
    }

    /// The input's name, description and luma.
    ///
    /// Port of `GeneralMerger::handleInputOnly` (SectionMerger.cpp:198-237 @ v2.5.2).
    fn handle_input_only(&mut self) -> Result<()> {
        // Note that the default strategy is used for the GeneralMerger but other strategies
        // may be used for other sections of the config and so always use the higher of the
        // two config versions.
        self.merge_from(true)
    }

    /// The base's name, description and luma.
    ///
    /// Port of `GeneralMerger::handleBaseOnly` (SectionMerger.cpp:239-278 @ v2.5.2).
    fn handle_base_only(&mut self) -> Result<()> {
        self.merge_from(false)
    }
}

/////////////////////////////////// RolesMerger //////////////////////////////////////////

/// Merges the roles.
///
/// Port of `RolesMerger` (SectionMerger.h:141-166, SectionMerger.cpp:284-416 @ v2.5.2).
pub(crate) struct RolesMerger<'o, 'a>(SectionMerger<'o, 'a>);

impl<'o, 'a> RolesMerger<'o, 'a> {
    /// Port of `RolesMerger::RolesMerger` (SectionMerger.h:144-155 @ v2.5.2).
    pub(crate) fn new(options: &'o mut MergeHandlerOptions<'a>) -> Result<Self> {
        let mut m = SectionMerger::new(options)?;
        m.strategy = section_strategy(m.params.roles(), m.params);
        Ok(RolesMerger(m))
    }

    /// Adds the input's roles to the merged config. A role the merged config has is replaced
    /// when the color spaces differ (ignoring case) and the roles' strategy prefers the input
    /// or takes only the input's, and is a conflict when they differ. A role named as a color
    /// space, an alias, a named transform or its alias of the merged config is a conflict and
    /// isn't added.
    ///
    /// Port of `RolesMerger::mergeInputRoles` (SectionMerger.cpp:284-376 @ v2.5.2).
    fn merge_input_roles(&mut self) -> Result<()> {
        let m = &mut self.0;
        let input = m.input_config;

        // Insert roles from input config.
        for i in 0..input.num_roles() {
            let name = input.role_name(i);
            let role_color_space_name = input.role_color_space(name);

            if m.merged_config.has_role(name) {
                // The base config already has this role.
                let base_role_color_space_name = m.merged_config.role_color_space(name);

                let strategy = m.params.roles();
                if strcasecmp(role_color_space_name, base_role_color_space_name).is_ne() {
                    // The color spaces are different. Replace based on the strategy.
                    if strategy == MergeStrategies::PreferInput
                        || strategy == MergeStrategies::InputOnly
                    {
                        m.merged_config
                            .set_role(name, Some(role_color_space_name))?;
                    }

                    let os = [
                        b"The Input config contains a role that would override Base config \
                          role '"
                            .as_slice(),
                        name,
                        b"'.",
                    ]
                    .concat();
                    m.notify(&os, m.params.is_error_on_conflict())?;
                }
                continue;
            }

            // Check for any conflicts.  Not allowing input roles to override color spaces
            // or named transforms in the base config.  The merge strategy only applies to
            // overriding base config roles.

            if let Some(existing_cs) = m.merged_config.color_space(name) {
                // There is a conflict, figure out what it is.

                let os = if strcasecmp(existing_cs.name(), name).is_eq() {
                    [
                        b"The Input config contains a role '".as_slice(),
                        name,
                        b"' that would override Base config color space '",
                        existing_cs.name(),
                        b"'.",
                    ]
                    .concat()
                } else if existing_cs.has_alias(name) {
                    [
                        b"The Input config contains a role '".as_slice(),
                        name,
                        b"' that would override an alias of Base config color space '",
                        existing_cs.name(),
                        b"'.",
                    ]
                    .concat()
                } else {
                    // (Should never happen.)
                    return Err(Exception::new(
                        [
                            b"Problem merging role: '".as_slice(),
                            name,
                            b"' due to color space conflict.",
                        ]
                        .concat(),
                    ));
                };

                m.notify(&os, m.params.is_error_on_conflict())?;
                continue;
            }

            if let Some(existing_nt) = m.merged_config.named_transform(name) {
                // There is a conflict, figure out what it is.

                let os = if strcasecmp(existing_nt.name(), name).is_eq() {
                    [
                        b"The Input config contains a role '".as_slice(),
                        name,
                        b"' that would override Base config named transform: '",
                        existing_nt.name(),
                        b"'.",
                    ]
                    .concat()
                } else if existing_nt.has_alias(name) {
                    [
                        b"The Input config contains a role '".as_slice(),
                        name,
                        b"' that would override an alias of Base config named transform: '",
                        existing_nt.name(),
                        b"'.",
                    ]
                    .concat()
                } else {
                    // (Should never happen.)
                    return Err(Exception::new(
                        [b"Problem merging role: '".as_slice(), name, b"'."].concat(),
                    ));
                };

                m.notify(&os, m.params.is_error_on_conflict())?;
                continue;
            }

            // No conflicts, go ahead and merge it.
            m.merged_config
                .set_role(name, Some(role_color_space_name))?;
        }
        Ok(())
    }
}

impl<'o, 'a> Section<'o, 'a> for RolesMerger<'o, 'a> {
    fn state(&self) -> &SectionMerger<'o, 'a> {
        &self.0
    }

    /// Port of `RolesMerger::getName` (SectionMerger.h:158 @ v2.5.2).
    fn name(&self) -> &'static str {
        "Roles"
    }

    /// Port of `RolesMerger::handlePreferInput` (SectionMerger.cpp:378-381 @ v2.5.2).
    fn handle_prefer_input(&mut self) -> Result<()> {
        self.merge_input_roles()
    }

    /// Port of `RolesMerger::handlePreferBase` (SectionMerger.cpp:383-386 @ v2.5.2).
    fn handle_prefer_base(&mut self) -> Result<()> {
        self.merge_input_roles()
    }

    /// The base's roles removed, then the input's added.
    ///
    /// Port of `RolesMerger::handleInputOnly` (SectionMerger.cpp:388-399 @ v2.5.2).
    fn handle_input_only(&mut self) -> Result<()> {
        let m = &mut self.0;
        // Remove the roles from base and take the roles from input.
        for i in 0..m.base_config.num_roles() {
            // Unset role from base config.
            m.merged_config.set_role(m.base_config.role_name(i), None)?;
        }

        // Insert roles from input config.
        self.merge_input_roles()
    }

    /// Port of `RolesMerger::handleBaseOnly` (SectionMerger.cpp:401-404 @ v2.5.2).
    fn handle_base_only(&mut self) -> Result<()> {
        // Nothing to do, since the merged config is initialized from the base config.
        Ok(())
    }

    /// The input's roles removed from the merged config.
    ///
    /// Port of `RolesMerger::handleRemove` (SectionMerger.cpp:406-416 @ v2.5.2).
    fn handle_remove(&mut self) -> Result<()> {
        let m = &mut self.0;
        for i in 0..m.input_config.num_roles() {
            if m.merged_config.has_role(m.input_config.role_name(i)) {
                // Remove the role.
                m.merged_config
                    .set_role(m.input_config.role_name(i), None)?;
            }
        }
        Ok(())
    }
}

/////////////////////////////////// FileRulesMerger //////////////////////////////////////

/// Whether `e` is one that C++'s `catch(const Exception & e)` catches: OCIO's `Exception`, or
/// its subclass `ExceptionMissingFile`.
fn is_ocio_exception(e: &Exception) -> bool {
    matches!(
        e.kind(),
        ExceptionKind::Exception | ExceptionKind::MissingFile
    )
}

/// Whether the rule `f1_idx` of `f1` and the rule `f2_idx` of `f2` have the same color space,
/// pattern, regular expression and extension (ignoring case) and the same custom keys, in any
/// order (their values ignoring case). The names aren't compared.
///
/// Port of `fileRulesAreEqual` (SectionMerger.cpp:425-472 @ v2.5.2).
fn file_rules_are_equal(
    f1: &FileRules,
    f1_idx: usize,
    f2: &FileRules,
    f2_idx: usize,
) -> Result<bool> {
    // NB: No need to compare the name of the rules, that should be done in the caller.

    // Compare color space name, pattern, extension, and regex strings.

    if strcasecmp(f1.color_space(f1_idx)?, f2.color_space(f2_idx)?).is_ne()
        || strcasecmp(f1.pattern(f1_idx)?, f2.pattern(f2_idx)?).is_ne()
        || strcasecmp(f1.regex(f1_idx)?, f2.regex(f2_idx)?).is_ne()
        || strcasecmp(f1.extension(f1_idx)?, f2.extension(f2_idx)?).is_ne()
    {
        return Ok(false);
    }

    // Compare the custom keys, handling the case where they may be in a different order.

    if f1.num_custom_keys(f1_idx)? != f2.num_custom_keys(f2_idx)? {
        return Ok(false);
    }

    let mut f1_custom_keys = CustomKeysContainer::default();
    for m in 0..f1.num_custom_keys(f1_idx)? {
        f1_custom_keys.set(
            f1.custom_key_name(f1_idx, m)?,
            f1.custom_key_value(f1_idx, m)?,
        )?;
    }

    for m in 0..f2.num_custom_keys(f2_idx)? {
        // A key the first doesn't have, or of another value (`hasKey`, then `getValueForKey`).
        let key = f2.custom_key_name(f2_idx, m)?;
        if !f1_custom_keys.has_key(key)
            || strcasecmp(
                f1_custom_keys.value_for_key(key)?,
                f2.custom_key_value(f2_idx, m)?,
            )
            .is_ne()
        {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Inserts a copy of the rule `input_rule_idx` of `input` at `merged_rule_idx` of `merged`:
/// the path search rule as such, a rule without a regular expression as a glob rule (`*` for an
/// empty pattern or extension), else as a regular expression rule; with its custom keys.
///
/// Port of `copyRule` (SectionMerger.cpp:474-519 @ v2.5.2).
fn copy_rule(
    input: &FileRules,
    input_rule_idx: usize,
    merged: &mut FileRules,
    merged_rule_idx: usize,
) -> Result<()> {
    // Handle case where the rule is ColorSpaceNamePathSearch.

    let name = input.name(input_rule_idx)?;
    if strcasecmp(name, FileRules::FILE_PATH_SEARCH_RULE_NAME).is_eq() {
        return merged.insert_path_search_rule(merged_rule_idx);
    }

    // Normal rule case.

    let regex = input.regex(input_rule_idx)?;
    if regex.is_empty() {
        // The regex is empty --> handle it as a pattern & extension type rule.
        let pattern = input.pattern(input_rule_idx)?;
        let extension = input.extension(input_rule_idx)?;
        merged.insert_rule(
            merged_rule_idx,
            name,
            input.color_space(input_rule_idx)?,
            if pattern.is_empty() { b"*" } else { pattern },
            if extension.is_empty() {
                b"*"
            } else {
                extension
            },
        )?;
    } else {
        // Handle it as a regex type rule.
        merged.insert_regex_rule(
            merged_rule_idx,
            name,
            input.color_space(input_rule_idx)?,
            regex,
        )?;
    }

    // Copy over any custom keys.

    for k in 0..input.num_custom_keys(input_rule_idx)? {
        merged.set_custom_key(
            merged_rule_idx,
            input.custom_key_name(input_rule_idx, k)?,
            input.custom_key_value(input_rule_idx, k)?,
        )?;
    }
    Ok(())
}

/// "The Input config contains a value that would override the Base config: <section>: <name>",
/// the conflict of the sections that merge by name.
fn override_message(section: &[u8], name: &[u8]) -> Vec<u8> {
    [
        b"The Input config contains a value that would override the Base config: ".as_slice(),
        section,
        b": ",
        name,
    ]
    .concat()
}

/// Merges the file rules and the strict parsing flag.
///
/// Port of `FileRulesMerger` (SectionMerger.h:168-196, SectionMerger.cpp:523-722 @ v2.5.2).
pub(crate) struct FileRulesMerger<'o, 'a>(SectionMerger<'o, 'a>);

impl<'o, 'a> FileRulesMerger<'o, 'a> {
    /// Port of `FileRulesMerger::FileRulesMerger` (SectionMerger.h:171-182 @ v2.5.2).
    pub(crate) fn new(options: &'o mut MergeHandlerOptions<'a>) -> Result<Self> {
        let mut m = SectionMerger::new(options)?;
        m.strategy = section_strategy(m.params.file_rules(), m.params);
        Ok(FileRulesMerger(m))
    }

    /// Adds the rules of `input` that `merged` doesn't have (by name, ignoring case) before its
    /// default rule; a rule of both that differs is a conflict, and `merged` keeps its own.
    ///
    /// Port of `FileRulesMerger::addRulesIfNotPresent` (SectionMerger.cpp:523-566 @ v2.5.2).
    fn add_rules_if_not_present(&self, input: &FileRules, merged: &mut FileRules) -> Result<()> {
        for input_rule_idx in 0..input.num_entries() {
            let mut has_conflict = false;

            let attempt = (|| -> Result<()> {
                // Check if the rule is already present.
                let name = input.name(input_rule_idx)?;

                // This will throw if the merged rules don't contain this name.
                let merged_rule_idx = merged.index_for_rule(name)?;

                // Based on the name, this file rule exists in the merged config.

                // If the rules are not identical, need to report conflict.
                if !file_rules_are_equal(merged, merged_rule_idx, input, input_rule_idx)? {
                    has_conflict = true;

                    return Err(Exception::new(override_message(b"file_rules", name)));
                }
                Ok(())
            })();

            if let Err(e) = attempt {
                if !is_ocio_exception(&e) {
                    return Err(e);
                }
                if has_conflict {
                    // Log or throw an exception describing the conflict.
                    self.0
                        .notify(e.what(), self.0.params.is_error_on_conflict())?;
                } else {
                    // File rule does not exist, add it in the penultimate positon, before the
                    // default. (Note that a default rule is always present, so will never get
                    // here in that case.)
                    let idx = merged.num_entries() - 1;
                    copy_rule(input, input_rule_idx, merged, idx)?;
                }
            }
        }
        Ok(())
    }

    /// Adds the rules of `input` to `merged`: a rule of both that differs replaces `merged`'s
    /// in place (the default rule takes `input`'s color space) and is a conflict; the others are
    /// added before the default rule.
    ///
    /// Port of `FileRulesMerger::addRulesAndOverwrite` (SectionMerger.cpp:568-616 @ v2.5.2).
    fn add_rules_and_overwrite(&self, input: &FileRules, merged: &mut FileRules) -> Result<()> {
        for input_rule_idx in 0..input.num_entries() {
            let mut has_conflict = false;

            let name = input.name(input_rule_idx)?;
            let attempt = (|| -> Result<()> {
                // Check if the rule is already present, throw if it is not.
                let merged_rule_idx = merged.index_for_rule(name)?;

                if !file_rules_are_equal(merged, merged_rule_idx, input, input_rule_idx)? {
                    has_conflict = true;

                    // Overwrite the existing rule.
                    if strcasecmp(name, FileRules::DEFAULT_RULE_NAME).is_ne() {
                        merged.remove_rule(merged_rule_idx)?;
                        copy_rule(input, input_rule_idx, merged, merged_rule_idx)?;
                    } else {
                        merged.set_default_rule_color_space(input.color_space(input_rule_idx)?)?;
                    }

                    return Err(Exception::new(override_message(b"file_rules", name)));
                }
                Ok(())
            })();

            if let Err(e) = attempt {
                if !is_ocio_exception(&e) {
                    return Err(e);
                }
                if has_conflict {
                    self.0
                        .notify(e.what(), self.0.params.is_error_on_conflict())?;
                } else {
                    // File rule does not exist, add it in the penultimate positon, before the
                    // default. (Note that a default rule is always present, so will never get
                    // here in that case.)
                    let idx = merged.num_entries() - 1;
                    copy_rule(input, input_rule_idx, merged, idx)?;
                }
            }
        }
        Ok(())
    }

    /// The rules of the input and of the base, either first, the preferred config's where both
    /// have a rule; the strict parsing flag of the input (`prefer_input`) or the base.
    ///
    /// The body of `FileRulesMerger::handlePreferInput` and `handlePreferBase`
    /// (SectionMerger.cpp:618-676 @ v2.5.2).
    fn merge_rules(&mut self, prefer_input: bool) -> Result<()> {
        let base_fr = self.0.base_config.file_rules().get();
        let input_fr = self.0.input_config.file_rules().get();

        // Handle strictparsing.
        if prefer_input {
            let strict = self.0.input_config.is_strict_parsing_enabled();
            self.0.merged_config.set_strict_parsing_enabled(strict);
        }
        // (Prefer base: nothing to do. Keep the base config value.)

        // Technique depends on whether the input rules should go first or not.
        let merged_file_rules = if self.0.params.is_input_first() {
            // Copying file rules from input config.
            let mut merged_file_rules = (*input_fr).clone();
            if prefer_input {
                // Insert file rules from base config, if not present.
                // If it doesn't exist, add it right before the default rule.
                self.add_rules_if_not_present(&base_fr, &mut merged_file_rules)?;
            } else {
                // Insert file rules from base config.
                // If the rule already exists, overwrite it.
                // If it doesn't exist, add it right before the default rule.
                self.add_rules_and_overwrite(&base_fr, &mut merged_file_rules)?;
            }
            merged_file_rules
        } else {
            // Copying file rules from base config.
            let mut merged_file_rules = (*base_fr).clone();
            if prefer_input {
                // Insert file rules from input config.
                // If the rule already exists, overwrite it.
                // If it doesn't exist, add it right before the default rule.
                self.add_rules_and_overwrite(&input_fr, &mut merged_file_rules)?;
            } else {
                // Insert file rules from input config, if not present.
                // If it doesn't exist, add it right before the default rule.
                self.add_rules_if_not_present(&input_fr, &mut merged_file_rules)?;
            }
            merged_file_rules
        };
        self.0.merged_config.set_file_rules(&merged_file_rules);
        Ok(())
    }
}

impl<'o, 'a> Section<'o, 'a> for FileRulesMerger<'o, 'a> {
    fn state(&self) -> &SectionMerger<'o, 'a> {
        &self.0
    }

    /// Port of `FileRulesMerger::getName` (SectionMerger.h:185 @ v2.5.2).
    fn name(&self) -> &'static str {
        "FileRules"
    }

    /// Port of `FileRulesMerger::handlePreferInput` (SectionMerger.cpp:618-646 @ v2.5.2).
    fn handle_prefer_input(&mut self) -> Result<()> {
        self.merge_rules(true)
    }

    /// Port of `FileRulesMerger::handlePreferBase` (SectionMerger.cpp:648-676 @ v2.5.2).
    fn handle_prefer_base(&mut self) -> Result<()> {
        self.merge_rules(false)
    }

    /// The input's rules and strict parsing flag.
    ///
    /// Port of `FileRulesMerger::handleInputOnly` (SectionMerger.cpp:678-685 @ v2.5.2).
    fn handle_input_only(&mut self) -> Result<()> {
        let m = &mut self.0;
        // Handle strictparsing.
        m.merged_config
            .set_strict_parsing_enabled(m.input_config.is_strict_parsing_enabled());

        // Simply take the rules from the input config.
        let rules = m.input_config.file_rules().get();
        m.merged_config.set_file_rules(&rules);
        Ok(())
    }

    /// Port of `FileRulesMerger::handleBaseOnly` (SectionMerger.cpp:687-690 @ v2.5.2).
    fn handle_base_only(&mut self) -> Result<()> {
        // Supported, but nothing to do.
        Ok(())
    }

    /// The base's rules without those the input has (by name, whatever they hold), the default
    /// rule kept.
    ///
    /// Port of `FileRulesMerger::handleRemove` (SectionMerger.cpp:692-722 @ v2.5.2).
    fn handle_remove(&mut self) -> Result<()> {
        let m = &mut self.0;
        let input_fr = m.input_config.file_rules().get();
        let mut merged_file_rules = (*m.base_config.file_rules().get()).clone();

        for f in 0..input_fr.num_entries() {
            let name = input_fr.name(f)?;

            // Never remove the Default rule.
            if strcasecmp(name, FileRules::DEFAULT_RULE_NAME).is_eq() {
                continue;
            }

            // Check if the input rule name is present in the base config.
            // Will throw if the name is not present.
            if let Ok(idx) = merged_file_rules.index_for_rule(name) {
                // Remove the rule (regardless of whether the content matches the base config).
                // (Upstream's `catch(...)` ignores any error.)
                let _ = merged_file_rules.remove_rule(idx);
            }
            // Do nothing if it is not present.
        }
        m.merged_config.set_file_rules(&merged_file_rules);
        Ok(())
    }
}

/////////////////////////////////// LooksMerger //////////////////////////////////////////

/// Adds to `merged` a copy of the look `name` of `cfg`, a look `cfg` lists.
///
/// `m_mergedConfig->addLook(cfg->getLook(name))` (SectionMerger.cpp:1919-2022 @ v2.5.2).
fn add_look_of(merged: &mut Config, cfg: &Config, name: &[u8]) -> Result<()> {
    let look = cfg.look(name).expect("a look the config lists");
    merged.add_look(look)
}

/// Merges the looks.
///
/// Port of `LooksMerger` (SectionMerger.h:288-312, SectionMerger.cpp:1919-2022 @ v2.5.2).
pub(crate) struct LooksMerger<'o, 'a>(SectionMerger<'o, 'a>);

impl<'o, 'a> LooksMerger<'o, 'a> {
    /// Port of `LooksMerger::LooksMerger` (SectionMerger.h:291-302 @ v2.5.2).
    pub(crate) fn new(options: &'o mut MergeHandlerOptions<'a>) -> Result<Self> {
        let mut m = SectionMerger::new(options)?;
        m.strategy = section_strategy(m.params.looks(), m.params);
        Ok(LooksMerger(m))
    }

    /// Adds the looks of `cfg`, each replacing the merged config's look of its name.
    fn add_all(&mut self, from_input: bool) -> Result<()> {
        let m = &mut self.0;
        let cfg = if from_input {
            m.input_config
        } else {
            m.base_config
        };
        for i in 0..cfg.num_looks() {
            add_look_of(m.merged_config, cfg, cfg.look_name_by_index(i))?;
        }
        Ok(())
    }

    /// Adds the looks of `cfg` whose names the merged config doesn't have.
    fn add_missing(&mut self, from_input: bool) -> Result<()> {
        let m = &mut self.0;
        let cfg = if from_input {
            m.input_config
        } else {
            m.base_config
        };
        for i in 0..cfg.num_looks() {
            if m.merged_config.look(cfg.look_name_by_index(i)).is_none() {
                add_look_of(m.merged_config, cfg, cfg.look_name_by_index(i))?;
            }
        }
        Ok(())
    }
}

impl<'o, 'a> Section<'o, 'a> for LooksMerger<'o, 'a> {
    fn state(&self) -> &SectionMerger<'o, 'a> {
        &self.0
    }

    /// Port of `LooksMerger::getName` (SectionMerger.h:305 @ v2.5.2).
    fn name(&self) -> &'static str {
        "Looks"
    }

    /// The looks of both, the input's where both have one.
    ///
    /// Port of `LooksMerger::handlePreferInput` (SectionMerger.cpp:1919-1955 @ v2.5.2).
    fn handle_prefer_input(&mut self) -> Result<()> {
        self.0.merged_config.clear_looks();

        if self.0.params.is_input_first() {
            // Add the Looks from the input config.
            self.add_all(true)?;

            // Add the Looks from the base config if it does not exist in the merged config.
            self.add_missing(false)
        } else {
            // PreferInput, InputFirst = false
            // Add the looks from the base config.
            self.add_all(false)?;

            // Add the Looks from the input config and overwrite look if the name is the same.
            self.add_all(true)
        }
    }

    /// The looks of both, the base's where both have one.
    ///
    /// Port of `LooksMerger::handlePreferBase` (SectionMerger.cpp:1957-1993 @ v2.5.2).
    fn handle_prefer_base(&mut self) -> Result<()> {
        self.0.merged_config.clear_looks();

        if self.0.params.is_input_first() {
            // Add the Looks from the input config.
            self.add_all(true)?;

            // Add the Looks from the input config and overwrite look if the name is the same.
            self.add_all(false)
        } else {
            // PreferBase, InputFirst = false
            // Add the looks from the base config.
            self.add_all(false)?;

            // Add the Looks from the input config if it does not exist in the merged config.
            self.add_missing(true)
        }
    }

    /// Port of `LooksMerger::handleInputOnly` (SectionMerger.cpp:1995-2003 @ v2.5.2).
    fn handle_input_only(&mut self) -> Result<()> {
        self.0.merged_config.clear_looks();
        // Add the Looks from the input config.
        self.add_all(true)
    }

    /// Port of `LooksMerger::handleBaseOnly` (SectionMerger.cpp:2005-2008 @ v2.5.2).
    fn handle_base_only(&mut self) -> Result<()> {
        // Supported, but nothing to do.
        Ok(())
    }

    /// The base's looks that the input doesn't have (by name).
    ///
    /// Port of `LooksMerger::handleRemove` (SectionMerger.cpp:2010-2022 @ v2.5.2).
    fn handle_remove(&mut self) -> Result<()> {
        let m = &mut self.0;
        m.merged_config.clear_looks();
        // Add the looks from the base config if they do not exist in the INPUT config.
        for i in 0..m.base_config.num_looks() {
            let base_name = m.base_config.look_name_by_index(i);
            if m.input_config.look(base_name).is_none() {
                add_look_of(m.merged_config, m.base_config, base_name)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "section_merger_oracle_tests.rs"]
mod oracle_tests;
