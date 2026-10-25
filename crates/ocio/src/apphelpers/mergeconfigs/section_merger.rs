// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The section mergers of a config merge: a port of
//! `src/OpenColorIO/apphelpers/mergeconfigs/SectionMerger.h` and `SectionMerger.cpp` @ v2.5.2.
//!
//! A merge copies the base config, then lets one merger per section of the config change that
//! copy, each by its strategy (`ConfigMerger::mergeConfigs`, `ConfigMergingHelpers::MergeConfigs`,
//! MergeConfigsHelpers.cpp:681-760): [`GeneralMerger`], [`RolesMerger`], [`FileRulesMerger`],
//! [`DisplayViewMerger`], the view transforms, [`LooksMerger`], the color spaces and the named
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
use ocio_ops::open_color_types::{
    ColorSpaceVisibility, NamedTransformVisibility, ReferenceSpaceType, SearchReferenceSpaceType,
    ViewTransformDirection, ViewType,
};
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::string_utils::{
    StringVec, contain, find, join, remove, replace, split, trim, trim_vec,
};

use crate::apphelpers::mergeconfigs::merge_configs_helpers::{
    ConfigMergingParameters, MergeStrategies,
};
use crate::color_space::ColorSpace;
use crate::config::Config;
use crate::config_utils::{self, ColorSpaceFingerprints};
use crate::custom_keys::CustomKeysContainer;
use crate::file_rules::FileRules;
use crate::named_transform::NamedTransform;
use crate::tokens_manager::TokensManager;
use crate::transform::Transform;
use crate::viewing_rules::ViewingRules;

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
/// Port of `SectionMerger`'s members (SectionMerger.h:30-120 @ v2.5.2).
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
    /// `m_inputToBaseGtScene`: the converter from the input config's scene-referred reference
    /// space to the base config's, which the view transforms and color spaces mergers set when
    /// the parameters adjust the input's reference space; `None` (upstream's null pointer)
    /// until then.
    pub(crate) input_to_base_gt_scene: Option<Transform>,
    /// `m_inputToBaseGtDisplay`: the same for the display-referred reference spaces.
    pub(crate) input_to_base_gt_display: Option<Transform>,
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
            input_to_base_gt_scene: None,
            input_to_base_gt_display: None,
        })
    }

    /// Sets the converters from the input config's reference spaces to the base config's
    /// (`ConfigUtils::initializeRefSpaceConverters`), as the constructors of the view
    /// transforms and color spaces mergers do when the parameters adjust the input's reference
    /// space.
    ///
    /// Port of the calls in `ViewTransformsMerger::ViewTransformsMerger` and
    /// `ColorspacesMerger::ColorspacesMerger` (SectionMerger.h:260-267, 328-334 @ v2.5.2).
    fn initialize_ref_space_converters(&mut self) -> Result<()> {
        let (scene, display) =
            config_utils::initialize_ref_space_converters(self.base_config, self.input_config)?;
        self.input_to_base_gt_scene = Some(scene);
        self.input_to_base_gt_display = Some(display);
        Ok(())
    }

    /// Logs `s` as a warning, or with `must_throw` (the option to stop at a conflict), returns
    /// it as the error.
    ///
    /// Port of `SectionMerger::notify` (SectionMerger.cpp:62-74 @ v2.5.2).
    pub(crate) fn notify(&self, s: &[u8], must_throw: bool) -> Result<()> {
        notify(s, must_throw)
    }
}

/// [`SectionMerger::notify`], which reads nothing of the merger: for the functions that merge
/// into the merged config itself.
///
/// Port of `SectionMerger::notify` (SectionMerger.cpp:62-74 @ v2.5.2).
fn notify(s: &[u8], must_throw: bool) -> Result<()> {
    if !must_throw {
        // By logging, we can see all errors.
        log_warning(s);
        Ok(())
    } else {
        // By throwing, we only get the first error on conflict, but also stop the merge.
        Err(Exception::new(s))
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

/////////////////////////////////// DisplayViewMerger ////////////////////////////////////

/// The entries of a comma-separated list, as they are (not trimmed); empty for an empty list.
/// Quotes aren't read: a quoted entry that holds a comma is split.
///
/// Port of `splitActiveList` (SectionMerger.cpp:31-38 @ v2.5.2).
fn split_active_list(list: &[u8], result: &mut StringVec) {
    if !list.is_empty() {
        // TODO: Need to handle quoted substrings.
        *result = split(list, b',');
    }
}

/// Trims the entries of `merged_vec`, then adds each entry of `input_vec`, trimmed, that isn't
/// empty and that `merged_vec` doesn't have (ignoring case).
///
/// Port of `mergeStringsWithoutDuplicates` (SectionMerger.cpp:40-58 @ v2.5.2).
fn merge_strings_without_duplicates(input_vec: &StringVec, merged_vec: &mut StringVec) {
    // Note that Contain requires a full exact match, not a partial match.
    // Hence it's important that the items in mergedVec be trimmed as well.
    trim_vec(merged_vec);

    for item in input_vec {
        let trimmed_item = trim(item);
        if !trimmed_item.is_empty() && !contain(merged_vec, trimmed_item) {
            merged_vec.push(trimmed_item.to_vec());
        }
    }
}

/// The color space or encoding a viewing rule lists at an index it has.
fn listed(token: Option<&[u8]>) -> &[u8] {
    // (In range, the rules always give a name.)
    token.unwrap_or_default()
}

/// Whether the rule `r1_idx` of `r1` and the rule `r2_idx` of `r2` have the same color spaces,
/// encodings and custom keys, each in any order (as the rules' tokens compare, and the custom
/// keys' values ignoring case). The names aren't compared.
///
/// Port of `viewingRulesAreEqual` (SectionMerger.cpp:731-810 @ v2.5.2).
fn viewing_rules_are_equal(
    r1: &ViewingRules,
    r1_idx: usize,
    r2: &ViewingRules,
    r2_idx: usize,
) -> Result<bool> {
    // NB: No need to compare the name of the rules, that should be done in the caller.

    // Compare color space tokens, handling the case where they may be in a different order.

    if r1.num_color_spaces(r1_idx)? != r2.num_color_spaces(r2_idx)? {
        return Ok(false);
    }

    let mut r1_color_spaces = TokensManager::default();
    for m in 0..r1.num_color_spaces(r1_idx)? {
        r1_color_spaces.add_token(listed(r1.color_space(r1_idx, m)?));
    }

    for m in 0..r2.num_color_spaces(r2_idx)? {
        if !r1_color_spaces.has_token(listed(r2.color_space(r2_idx, m)?)) {
            return Ok(false);
        }
    }

    // Compare encoding tokens, handling the case where they may be in a different order.

    if r1.num_encodings(r1_idx)? != r2.num_encodings(r2_idx)? {
        return Ok(false);
    }

    let mut r1_encodings = TokensManager::default();
    for m in 0..r1.num_encodings(r1_idx)? {
        r1_encodings.add_token(listed(r1.encoding(r1_idx, m)?));
    }

    for m in 0..r2.num_encodings(r2_idx)? {
        if !r1_encodings.has_token(listed(r2.encoding(r2_idx, m)?)) {
            return Ok(false);
        }
    }

    // Compare the custom keys, handling the case where they may be in a different order.

    if r1.num_custom_keys(r1_idx)? != r2.num_custom_keys(r2_idx)? {
        return Ok(false);
    }

    let mut r1_custom_keys = CustomKeysContainer::default();
    for m in 0..r1.num_custom_keys(r1_idx)? {
        r1_custom_keys.set(
            r1.custom_key_name(r1_idx, m)?,
            r1.custom_key_value(r1_idx, m)?,
        )?;
    }

    for m in 0..r2.num_custom_keys(r2_idx)? {
        // A key the first doesn't have, or of another value (`hasKey`, then `getValueForKey`).
        let key = r2.custom_key_name(r2_idx, m)?;
        if !r1_custom_keys.has_key(key)
            || strcasecmp(
                r1_custom_keys.value_for_key(key)?,
                r2.custom_key_value(r2_idx, m)?,
            )
            .is_ne()
        {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Inserts a copy of the rule `src_idx` of `src` at `dst_idx` of `rules`: its name, color
/// spaces, encodings and custom keys. At the first error the copy stops, without a word, and
/// what it inserted stays.
///
/// Port of `copyViewingRule` (SectionMerger.cpp:812-841 @ v2.5.2).
fn copy_viewing_rule(src: &ViewingRules, src_idx: usize, dst_idx: usize, rules: &mut ViewingRules) {
    let mut copy = || -> Result<()> {
        rules.insert_rule(dst_idx, src.name(src_idx)?)?;

        for j in 0..src.num_color_spaces(src_idx)? as i32 {
            rules.add_color_space(dst_idx, listed(src.color_space(src_idx, j as usize)?))?;
        }

        for k in 0..src.num_encodings(src_idx)? as i32 {
            rules.add_encoding(dst_idx, listed(src.encoding(src_idx, k as usize)?))?;
        }

        for l in 0..src.num_custom_keys(src_idx)? as i32 {
            rules.set_custom_key(
                dst_idx,
                src.custom_key_name(src_idx, l as usize)?,
                src.custom_key_value(src_idx, l as usize)?,
            )?;
        }
        Ok(())
    };
    // Don't add it if any errors.
    // Continue.
    let _ = copy();
}

/// Adds to `merged_rules` the rules of `rules` it doesn't have (by name), at its end.
///
/// Port of `addUniqueViewingRules` (SectionMerger.cpp:843-861 @ v2.5.2).
fn add_unique_viewing_rules(rules: &ViewingRules, merged_rules: &mut ViewingRules) -> Result<()> {
    for i in 0..rules.num_entries() {
        let name = rules.name(i)?;
        // Take the rule from the first config if it does not exist.
        if merged_rules.index_for_rule(name).is_err() {
            // Rule does not exist in merged rules.
            // Add it.
            let end = merged_rules.num_entries();
            copy_viewing_rule(rules, i, end, merged_rules);
        }
    }
    Ok(())
}

/// The arguments of `addDisplayView` or `addSharedView` for the view `view` of `display` of
/// `cfg` (an empty display for the config's shared views): its view transform, color space,
/// looks, rule and description.
fn view_of<'c>(cfg: &'c Config, display: &[u8], view: &[u8]) -> [&'c [u8]; 5] {
    [
        cfg.display_view_transform_name(display, view),
        cfg.display_view_color_space_name(display, view),
        cfg.display_view_looks(display, view),
        cfg.display_view_rule(display, view),
        cfg.display_view_description(display, view),
    ]
}

/// The same for the view `view` of the virtual display of `cfg`.
fn virtual_view_of<'c>(cfg: &'c Config, view: &[u8]) -> [&'c [u8]; 5] {
    [
        cfg.virtual_display_view_transform_name(view),
        cfg.virtual_display_view_color_space_name(view),
        cfg.virtual_display_view_looks(view),
        cfg.virtual_display_view_rule(view),
        cfg.virtual_display_view_description(view),
    ]
}

/// `m_mergedConfig->addDisplayView(display, view, <the view of cfg>)`.
fn add_display_view_of(
    merged: &mut Config,
    cfg: &Config,
    display: &[u8],
    view: &[u8],
) -> Result<()> {
    let [vt, cs, looks, rule, desc] = view_of(cfg, display, view);
    merged.add_display_view_with_view_transform(display, view, vt, cs, looks, rule, desc)
}

/// `m_mergedConfig->addSharedView(view, <the shared view of cfg>)`.
fn add_shared_view_of(merged: &mut Config, cfg: &Config, view: &[u8]) -> Result<()> {
    let [vt, cs, looks, rule, desc] = view_of(cfg, b"", view);
    merged.add_shared_view(view, vt, cs, looks, rule, desc)
}

/// `m_mergedConfig->addVirtualDisplayView(view, <the virtual view of cfg>)`.
fn add_virtual_view_of(merged: &mut Config, cfg: &Config, view: &[u8]) -> Result<()> {
    let [vt, cs, looks, rule, desc] = virtual_view_of(cfg, view);
    merged.add_virtual_display_view(view, vt, cs, looks, rule, desc)
}

/// Merges the displays and their views, the shared views, the virtual display, the active
/// displays and views, and the viewing rules.
///
/// Port of `DisplayViewMerger` (SectionMerger.h:198-243, SectionMerger.cpp:865-1677 @ v2.5.2).
pub(crate) struct DisplayViewMerger<'o, 'a>(SectionMerger<'o, 'a>);

impl<'o, 'a> DisplayViewMerger<'o, 'a> {
    /// Port of `DisplayViewMerger::DisplayViewMerger` (SectionMerger.h:201-212 @ v2.5.2).
    pub(crate) fn new(options: &'o mut MergeHandlerOptions<'a>) -> Result<Self> {
        let mut m = SectionMerger::new(options)?;
        m.strategy = section_strategy(m.params.display_views(), m.params);
        Ok(DisplayViewMerger(m))
    }

    /// Adds, for each display of `cfg`, the views of `cfg` that the merged config's display
    /// doesn't have (by name, either kind), as the same kind of view.
    ///
    /// Port of `DisplayViewMerger::addUniqueDisplays` (SectionMerger.cpp:865-908 @ v2.5.2).
    fn add_unique_displays(&mut self, cfg: &Config) -> Result<()> {
        let merged = &mut *self.0.merged_config;
        // For each display, add any views from cfg that are not already in the merged config.

        for i in 0..cfg.num_displays_all() {
            let disp_name = cfg.display_all(i);

            // Display defined views.
            for v in 0..cfg.num_views_of_type(ViewType::DisplayDefined, disp_name) {
                let display_defined_view = cfg.view_of_type(ViewType::DisplayDefined, disp_name, v);

                // This will return true if the display contains either a display-defined or
                // shared view with this name.
                let disp_defined_exists = merged.has_view(disp_name, display_defined_view);

                if !display_defined_view.is_empty() && !disp_defined_exists {
                    // (Note this works for either the new or old style of view.)
                    add_display_view_of(merged, cfg, disp_name, display_defined_view)?;
                }
            }

            // Shared views.
            for v in 0..cfg.num_views_of_type(ViewType::Shared, disp_name) {
                let shared_view_name = cfg.view_of_type(ViewType::Shared, disp_name, v);

                let shared_view_exists = merged.has_view(disp_name, shared_view_name);

                if !shared_view_name.is_empty() && !shared_view_exists {
                    merged.add_display_shared_view(disp_name, shared_view_name)?;
                }
            }
        }
        Ok(())
    }

    /// Adds the views of the virtual display of `cfg` that the merged config's doesn't have.
    ///
    /// Port of `DisplayViewMerger::addUniqueVirtualViews` (SectionMerger.cpp:910-938 @ v2.5.2).
    fn add_unique_virtual_views(&mut self, cfg: &Config) -> Result<()> {
        let merged = &mut *self.0.merged_config;
        // Display defined views.
        for v in 0..cfg.virtual_display_num_views(ViewType::DisplayDefined) {
            let display_defined_view = cfg.virtual_display_view(ViewType::DisplayDefined, v);
            let disp_defined_exists = merged.has_virtual_view(display_defined_view);
            if !display_defined_view.is_empty() && !disp_defined_exists {
                add_virtual_view_of(merged, cfg, display_defined_view)?;
            }
        }

        // Shared views.
        for v in 0..cfg.virtual_display_num_views(ViewType::Shared) {
            let shared_view_name = cfg.virtual_display_view(ViewType::Shared, v);
            let shared_view_exists = merged.has_virtual_view(shared_view_name);
            if !shared_view_name.is_empty() && !shared_view_exists {
                merged.add_virtual_display_shared_view(shared_view_name)?;
            }
        }
        Ok(())
    }

    /// The displays of `first`, in its order, each with its views, then those `second` adds. A
    /// view of both is taken from `second` with `prefer_second` (as the kind of view it is
    /// there), else from `first`; a display-defined view of both that differs is a conflict.
    ///
    /// Port of `DisplayViewMerger::processDisplays` (SectionMerger.cpp:940-1062 @ v2.5.2).
    fn process_displays(
        &mut self,
        first: &Config,
        second: &Config,
        prefer_second: bool,
    ) -> Result<()> {
        // Iterate over the first config's displays.

        for i in 0..first.num_displays_all() {
            let disp_name = first.display_all(i);

            // Iterate over this display's display-defined views.

            for v in 0..first.num_views_of_type(ViewType::DisplayDefined, disp_name) {
                let display_defined_view =
                    first.view_of_type(ViewType::DisplayDefined, disp_name, v);

                if !display_defined_view.is_empty() {
                    // One case to be aware of is where both configs have the same display with
                    // the same view name, but it's a display-defined view in one and a shared
                    // view in the other. This check will return true if it exists in either
                    // form.
                    let exists_in_second = second.has_view(disp_name, display_defined_view);

                    if exists_in_second
                        && !Config::are_views_equal(first, second, disp_name, display_defined_view)
                    {
                        // Throw or log on conflict.
                        let os = [
                            b"The Input config contains a value that would override the Base \
                              config: display: "
                                .as_slice(),
                            disp_name,
                            b", view: ",
                            display_defined_view,
                        ]
                        .concat();
                        self.0.notify(&os, self.0.params.is_error_on_conflict())?;
                    }

                    let merged = &mut *self.0.merged_config;
                    // Display defined views.
                    if exists_in_second && prefer_second {
                        // Take the view from the second config.

                        // This was a display-defined view in the first config but it may not be
                        // in the second config.  Want to add it as the same type of view.
                        if second.is_view_shared(disp_name, display_defined_view) {
                            // Note that this may change the order in a way that does not follow
                            // the preference of input-first or base-first.
                            merged.add_display_shared_view(disp_name, display_defined_view)?;
                        } else {
                            add_display_view_of(merged, second, disp_name, display_defined_view)?;
                        }
                    } else {
                        // Take the view from the first config (where it is display-defined).
                        // (Note this works for either the new or old style of view.)
                        add_display_view_of(merged, first, disp_name, display_defined_view)?;
                    }
                }
            }

            // Iterate over this display's shared views.

            for v in 0..first.num_views_of_type(ViewType::Shared, disp_name) {
                let shared_view_name = first.view_of_type(ViewType::Shared, disp_name, v);

                if !shared_view_name.is_empty() {
                    let exists_in_second = second.has_view(disp_name, shared_view_name);

                    if exists_in_second && prefer_second {
                        // This was a shared view in the first config but it may not be in
                        // the second config.  Want to add it as the same type of view.
                        if second.is_view_shared(disp_name, shared_view_name) {
                            self.0
                                .merged_config
                                .add_display_shared_view(disp_name, shared_view_name)?;
                        } else {
                            if !Config::are_views_equal(first, second, disp_name, shared_view_name)
                            {
                                // Throw or log on conflict.
                                let os = [
                                    b"The Input config contains a value that would override the \
                                      Base config: display: "
                                        .as_slice(),
                                    disp_name,
                                    b", view: ",
                                    shared_view_name,
                                ]
                                .concat();
                                self.0.notify(&os, self.0.params.is_error_on_conflict())?;
                            }
                            add_display_view_of(
                                self.0.merged_config,
                                second,
                                disp_name,
                                shared_view_name,
                            )?;
                        }
                    } else {
                        // Note: The error-on-conflict check happens in processSharedViews,
                        // this is just adding the reference, so it's not checked again here.
                        self.0
                            .merged_config
                            .add_display_shared_view(disp_name, shared_view_name)?;
                    }
                }
            }
        }

        // Add the remaining views for all displays from the second config. (This only adds
        // views that are not already present.)

        self.add_unique_displays(second)
    }

    /// The views of the virtual display of `first`, then those `second` adds, as
    /// [`DisplayViewMerger::process_displays`] does for a display.
    ///
    /// Port of `DisplayViewMerger::processVirtualDisplay` (SectionMerger.cpp:1064-1169 @
    /// v2.5.2).
    fn process_virtual_display(
        &mut self,
        first: &Config,
        second: &Config,
        prefer_second: bool,
    ) -> Result<()> {
        for v in 0..first.virtual_display_num_views(ViewType::DisplayDefined) {
            let display_defined_view = first.virtual_display_view(ViewType::DisplayDefined, v);

            if !display_defined_view.is_empty() {
                // Check if it displays shared view exists in second config.
                let exists_in_second = second.has_virtual_view(display_defined_view);

                if exists_in_second
                    && !Config::are_virtual_views_equal(first, second, display_defined_view)
                {
                    // Throw or log on conflict.
                    let os = override_message(b"virtual_display", display_defined_view);
                    self.0.notify(&os, self.0.params.is_error_on_conflict())?;
                }

                let merged = &mut *self.0.merged_config;
                // Display defined views.
                if exists_in_second && prefer_second {
                    // Take the view from the second config.

                    // This was a display-defined view in the first config but it may not be in
                    // the second config.  Want to add it as the same type of view.
                    if second.is_virtual_view_shared(display_defined_view) {
                        merged.add_virtual_display_shared_view(display_defined_view)?;
                    } else {
                        add_virtual_view_of(merged, second, display_defined_view)?;
                    }
                } else {
                    // Take the view from the first config (where it is display-defined).
                    // (Note this works for either the new or old style of view.)
                    add_virtual_view_of(merged, first, display_defined_view)?;
                }
            }
        }

        // Iterate over this display's shared views.

        for v in 0..first.virtual_display_num_views(ViewType::Shared) {
            let shared_view_name = first.virtual_display_view(ViewType::Shared, v);

            if !shared_view_name.is_empty() {
                let exists_in_second = second.has_virtual_view(shared_view_name);

                if exists_in_second && prefer_second {
                    // This was a shared view in the first config but it may not be in
                    // the second config.  Want to add it as the same type of view.
                    if second.is_virtual_view_shared(shared_view_name) {
                        self.0
                            .merged_config
                            .add_virtual_display_shared_view(shared_view_name)?;
                    } else {
                        if !Config::are_virtual_views_equal(first, second, shared_view_name) {
                            // Throw or log on conflict.
                            let os = override_message(b"virtual_display", shared_view_name);
                            self.0.notify(&os, self.0.params.is_error_on_conflict())?;
                        }
                        add_virtual_view_of(self.0.merged_config, second, shared_view_name)?;
                    }
                } else {
                    // Note: The error-on-conflict check happens in processSharedViews,
                    // this is just adding the reference, so it's not checked again here.
                    self.0
                        .merged_config
                        .add_virtual_display_shared_view(shared_view_name)?;
                }
            }
        }

        // Add the remaining views from the second config.

        self.add_unique_virtual_views(second)
    }

    /// Adds the shared views of `cfg` that the merged config doesn't have.
    ///
    /// Port of `DisplayViewMerger::addUniqueSharedViews` (SectionMerger.cpp:1171-1192 @
    /// v2.5.2).
    fn add_unique_shared_views(&mut self, cfg: &Config) -> Result<()> {
        let merged = &mut *self.0.merged_config;
        // Add any shared views that are not already in the merged config.

        for v in 0..cfg.num_views_of_type(ViewType::Shared, b"") {
            let shared_view_name = cfg.view_of_type(ViewType::Shared, b"", v);

            // Check if shared view exists in merged config.
            let shared_view_exists = merged.has_view(b"", shared_view_name);

            if !shared_view_name.is_empty() && !shared_view_exists {
                add_shared_view_of(merged, cfg, shared_view_name)?;
            }
        }
        Ok(())
    }

    /// The shared views of `first`, each from `second` where it has one and `prefer_second`,
    /// then those only `second` has; one of both that differs is a conflict.
    ///
    /// Port of `DisplayViewMerger::processSharedViews` (SectionMerger.cpp:1194-1244 @ v2.5.2).
    fn process_shared_views(
        &mut self,
        first: &Config,
        second: &Config,
        prefer_second: bool,
    ) -> Result<()> {
        // Iterate over all shared views in the first config.
        for v in 0..first.num_views_of_type(ViewType::Shared, b"") {
            let shared_view_name = first.view_of_type(ViewType::Shared, b"", v);

            if !shared_view_name.is_empty() {
                // Check if shared view exists in second config.
                let exists_in_second = second.has_view(b"", shared_view_name);

                if exists_in_second
                    && !Config::are_views_equal(first, second, b"", shared_view_name)
                {
                    // Throw or log on conflict.
                    let os = override_message(b"shared_views", shared_view_name);
                    self.0.notify(&os, self.0.params.is_error_on_conflict())?;
                }

                if exists_in_second && prefer_second {
                    // Take the shared view from the second config.
                    // (Note this works for either the new or old style of view.)
                    add_shared_view_of(self.0.merged_config, second, shared_view_name)?;
                } else {
                    // Take the shared view from the first config.
                    add_shared_view_of(self.0.merged_config, first, shared_view_name)?;
                }
            }
        }

        // Add the remaining shared views that are only in the second config.

        self.add_unique_shared_views(second)
    }

    /// The active displays and views: the overrides where they aren't empty, else both
    /// configs' lists, either first, without duplicates.
    ///
    /// Port of `DisplayViewMerger::processActiveLists` (SectionMerger.cpp:1246-1312 @ v2.5.2).
    fn process_active_lists(&mut self) -> Result<()> {
        let m = &mut self.0;
        // Merge active_displays.
        let active_displays = m.params.active_displays();
        if !active_displays.is_empty() {
            // Take active_displays from overrides.
            m.merged_config.set_active_displays(&active_displays)?;
        } else {
            // Take active_displays from config.

            let mut merged_active_displays = StringVec::new();
            if m.params.is_input_first() {
                let mut base_active_displays = StringVec::new();
                split_active_list(&m.base_config.active_displays(), &mut base_active_displays);
                split_active_list(
                    &m.input_config.active_displays(),
                    &mut merged_active_displays,
                );

                merge_strings_without_duplicates(
                    &base_active_displays,
                    &mut merged_active_displays,
                );
            } else {
                let mut input_active_displays = StringVec::new();
                split_active_list(
                    &m.input_config.active_displays(),
                    &mut input_active_displays,
                );
                split_active_list(
                    &m.base_config.active_displays(),
                    &mut merged_active_displays,
                );

                merge_strings_without_duplicates(
                    &input_active_displays,
                    &mut merged_active_displays,
                );
            }

            // NB: StringUtils::Join adds a space after the comma.
            m.merged_config
                .set_active_displays(join(&merged_active_displays, b','))?;
        }

        // Merge active_views.
        let active_views = m.params.active_views();
        if !active_views.is_empty() {
            // Take active_views from overrides.
            m.merged_config.set_active_views(&active_views)?;
        } else {
            // Take active_views from config.

            let mut merged_active_views = StringVec::new();
            if m.params.is_input_first() {
                let mut base_active_views = StringVec::new();
                split_active_list(&m.base_config.active_views(), &mut base_active_views);
                split_active_list(&m.input_config.active_views(), &mut merged_active_views);

                merge_strings_without_duplicates(&base_active_views, &mut merged_active_views);
            } else {
                let mut input_active_views = StringVec::new();
                split_active_list(&m.input_config.active_views(), &mut input_active_views);
                split_active_list(&m.base_config.active_views(), &mut merged_active_views);

                merge_strings_without_duplicates(&input_active_views, &mut merged_active_views);
            }

            m.merged_config
                .set_active_views(join(&merged_active_views, b','))?;
        }
        Ok(())
    }

    /// The viewing rules of `first`, in its order, each from `second` where it has a rule of
    /// that name that differs and `prefer_second` (a conflict, either way), then those only
    /// `second` has.
    ///
    /// Port of `DisplayViewMerger::processViewingRules` (SectionMerger.cpp:1314-1373 @ v2.5.2).
    fn process_viewing_rules(
        &mut self,
        first: &Config,
        second: &Config,
        prefer_second: bool,
    ) -> Result<()> {
        let mut merged_rules = ViewingRules::new();

        let first_rules = first.viewing_rules().get();
        let second_rules = second.viewing_rules().get();

        for i in 0..first_rules.num_entries() {
            let mut has_conflict = false;

            let name = first_rules.name(i)?;
            let attempt = (|| -> Result<()> {
                // Check if it exists in second rules, will throw if not.
                let idx = second_rules.index_for_rule(name)?;

                if !viewing_rules_are_equal(&first_rules, i, &second_rules, idx)? {
                    has_conflict = true;

                    let end = merged_rules.num_entries();
                    if prefer_second {
                        // Take rule from the second config.
                        copy_viewing_rule(&second_rules, idx, end, &mut merged_rules);
                    } else {
                        // Found, but not overriding. Take rule from the first config.
                        copy_viewing_rule(&first_rules, i, end, &mut merged_rules);
                    }

                    return Err(Exception::new(override_message(b"viewing_rules", name)));
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
                    // Not found in second rules. Take rule from the first config.
                    let end = merged_rules.num_entries();
                    copy_viewing_rule(&first_rules, i, end, &mut merged_rules);
                }
            }
        }

        // Add the remaining rules.
        add_unique_viewing_rules(&second_rules, &mut merged_rules)?;

        self.0.merged_config.set_viewing_rules(&merged_rules);
        Ok(())
    }

    /// The shared views, displays, virtual display and viewing rules of the input and the
    /// base, `first` first, preferring `second`'s with `prefer_second`; the active lists.
    ///
    /// The body of `DisplayViewMerger::handlePreferInput` and `handlePreferBase`
    /// (SectionMerger.cpp:1375-1464 @ v2.5.2).
    fn merge_both(&mut self, first_is_input: bool, prefer_second: bool) -> Result<()> {
        let (first, second) = if first_is_input {
            (self.0.input_config, self.0.base_config)
        } else {
            (self.0.base_config, self.0.input_config)
        };

        // Clear displays and shared_views from merged config.
        self.0.merged_config.clear_displays();
        self.0.merged_config.clear_shared_views();

        // Merge displays and views.
        // The order is important: shared_views, and then displays.
        self.process_shared_views(first, second, prefer_second)?;
        self.process_displays(first, second, prefer_second)?;

        // Merge virtual_display.
        self.0.merged_config.clear_virtual_display();
        self.process_virtual_display(first, second, prefer_second)?;

        // Merge active_displays and active_views.
        self.process_active_lists()?;

        // Merge viewing_rules.
        self.process_viewing_rules(first, second, prefer_second)
    }
}

impl<'o, 'a> Section<'o, 'a> for DisplayViewMerger<'o, 'a> {
    fn state(&self) -> &SectionMerger<'o, 'a> {
        &self.0
    }

    /// Port of `DisplayViewMerger::getName` (SectionMerger.h:215 @ v2.5.2).
    fn name(&self) -> &'static str {
        "Display/Views"
    }

    /// Both configs' views and rules, the input's where both have one.
    ///
    /// Port of `DisplayViewMerger::handlePreferInput` (SectionMerger.cpp:1375-1420 @ v2.5.2).
    fn handle_prefer_input(&mut self) -> Result<()> {
        // The error_on_conflict option implies to shared_views, displays/views,
        // virtual_display, view_transforms, default_view_transform, and viewing_rules.
        if self.0.params.is_input_first() {
            self.merge_both(true, false)
        } else {
            self.merge_both(false, true)
        }
    }

    /// Both configs' views and rules, the base's where both have one.
    ///
    /// Port of `DisplayViewMerger::handlePreferBase` (SectionMerger.cpp:1422-1464 @ v2.5.2).
    fn handle_prefer_base(&mut self) -> Result<()> {
        if self.0.params.is_input_first() {
            self.merge_both(true, true)
        } else {
            self.merge_both(false, false)
        }
    }

    /// The input's views, virtual display, viewing rules and active lists (the overrides of the
    /// active lists where they aren't empty).
    ///
    /// Port of `DisplayViewMerger::handleInputOnly` (SectionMerger.cpp:1466-1508 @ v2.5.2).
    fn handle_input_only(&mut self) -> Result<()> {
        let input = self.0.input_config;
        // Clear displays and shared_views from merged config.
        self.0.merged_config.clear_displays();
        self.0.merged_config.clear_shared_views();

        // Merge displays and views.
        self.add_unique_shared_views(input)?;
        self.add_unique_displays(input)?;

        // Merge virtual_display.
        self.0.merged_config.clear_virtual_display();
        self.add_unique_virtual_views(input)?;

        let m = &mut self.0;
        // Merge active_displays.
        let active_displays = m.params.active_displays();
        if !active_displays.is_empty() {
            // Take active_displays from overrides.
            m.merged_config.set_active_displays(&active_displays)?;
        } else {
            // Take active_displays from config.
            m.merged_config
                .set_active_displays(input.active_displays())?;
        }

        // Merge active_views.
        let active_views = m.params.active_views();
        if !active_views.is_empty() {
            // Take active_views from overrides.
            m.merged_config.set_active_views(&active_views)?;
        } else {
            // Take active_views from config.
            m.merged_config.set_active_views(input.active_views())?;
        }

        // Merge viewing_rules.
        let rules = input.viewing_rules().get();
        m.merged_config.set_viewing_rules(&rules);
        Ok(())
    }

    /// The base's, with the overrides of the active lists where they aren't empty.
    ///
    /// Port of `DisplayViewMerger::handleBaseOnly` (SectionMerger.cpp:1510-1528 @ v2.5.2).
    fn handle_base_only(&mut self) -> Result<()> {
        let m = &mut self.0;
        // Process the overrides only since the merged config is initialized to
        // the base config.

        let active_displays = m.params.active_displays();
        if !active_displays.is_empty() {
            // Take active_displays from overrides.
            m.merged_config.set_active_displays(&active_displays)?;
        }

        let active_views = m.params.active_views();
        if !active_views.is_empty() {
            // Take active_views from overrides.
            m.merged_config.set_active_views(&active_views)?;
        }
        Ok(())
    }

    /// The base's shared views, views, virtual views, active displays and views and viewing
    /// rules without those the input has (by name).
    ///
    /// Port of `DisplayViewMerger::handleRemove` (SectionMerger.cpp:1530-1677 @ v2.5.2).
    fn handle_remove(&mut self) -> Result<()> {
        let m = &mut self.0;
        let (base, input) = (m.base_config, m.input_config);
        let merged = &mut *m.merged_config;

        // Remove shared_views.

        merged.clear_shared_views();

        for v in 0..base.num_views_of_type(ViewType::Shared, b"") {
            // Add shared views that are present in the base config and NOT present in the
            // input config.
            let shared_view_name = base.view_of_type(ViewType::Shared, b"", v);
            if !shared_view_name.is_empty() && !input.has_view(b"", shared_view_name) {
                add_shared_view_of(merged, base, shared_view_name)?;
            }
        }

        // Remove views from displays.

        merged.clear_displays();

        for i in 0..base.num_displays_all() {
            // Add views that are present in the base config and NOT present in the input config.

            // Display-defined views.
            let disp_name = base.display_all(i);
            for v in 0..base.num_views_of_type(ViewType::DisplayDefined, disp_name) {
                let display_defined_view =
                    base.view_of_type(ViewType::DisplayDefined, disp_name, v);
                // Check if the view is not present in the input config.
                if !display_defined_view.is_empty()
                    && !input.has_view(disp_name, display_defined_view)
                {
                    add_display_view_of(merged, base, disp_name, display_defined_view)?;
                }
            }

            // Shared views.
            for v in 0..base.num_views_of_type(ViewType::Shared, disp_name) {
                let shared_view_name = base.view_of_type(ViewType::Shared, disp_name, v);
                // Check if the view is not present in the input config.
                if !shared_view_name.is_empty() && !input.has_view(disp_name, shared_view_name) {
                    merged.add_display_shared_view(disp_name, shared_view_name)?;
                }
            }
        }

        // Remove views from virtual_display.

        merged.clear_virtual_display();

        {
            // Add virtual views that are present in the base config and NOT present in the
            // input config.

            // Display-defined views.
            for v in 0..base.virtual_display_num_views(ViewType::DisplayDefined) {
                let display_defined_view = base.virtual_display_view(ViewType::DisplayDefined, v);
                // Check if the view is not present in the input config.
                if !display_defined_view.is_empty() && !input.has_virtual_view(display_defined_view)
                {
                    // Add the display defined view
                    add_virtual_view_of(merged, base, display_defined_view)?;
                }
            }

            // Shared views.
            for v in 0..base.virtual_display_num_views(ViewType::Shared) {
                let shared_view_name = base.virtual_display_view(ViewType::Shared, v);
                // Check if the view is not present in the input config.
                if !shared_view_name.is_empty() && !input.has_virtual_view(shared_view_name) {
                    // Add the shared view
                    merged.add_virtual_display_shared_view(shared_view_name)?;
                }
            }
        }

        // Remove from active_displays.

        let mut input_active_displays = StringVec::new();
        let mut merged_active_displays = StringVec::new();
        split_active_list(&input.active_displays(), &mut input_active_displays);
        split_active_list(&base.active_displays(), &mut merged_active_displays);

        for disp in &input_active_displays {
            remove(&mut merged_active_displays, disp);
        }
        merged.set_active_displays(join(&merged_active_displays, b','))?;

        // Remove from active_views.

        let mut input_active_views = StringVec::new();
        let mut merged_active_views = StringVec::new();
        split_active_list(&input.active_views(), &mut input_active_views);
        split_active_list(&base.active_views(), &mut merged_active_views);

        for view in &input_active_views {
            remove(&mut merged_active_views, view);
        }
        merged.set_active_views(join(&merged_active_views, b','))?;

        // Handle viewing_rules.

        let mut merged_rules = ViewingRules::new();
        let input_rules = input.viewing_rules().get();
        let base_rules = base.viewing_rules().get();

        for i in 0..base_rules.num_entries() {
            let name = base_rules.name(i)?;
            // Throws if the input doesn't have the base rule.
            if input_rules.index_for_rule(name).is_err() {
                // Keep any base rules that aren't in the input.
                let end = merged_rules.num_entries();
                copy_viewing_rule(&base_rules, i, end, &mut merged_rules);
            }
        }

        merged.set_viewing_rules(&merged_rules);
        Ok(())
    }
}

/////////////////////////////////// ViewTransformsMerger /////////////////////////////////

/// Whether `first` and `second` both have a view transform named `name`, of the same reference
/// space type, whose transforms in each direction are both missing or print the same text
/// (`operator<<`, which loads no file). The family, description and categories aren't
/// compared (docs/improvements.md, I-432).
///
/// Port of `viewTransformsAreEqual` (SectionMerger.cpp:1686-1749 @ v2.5.2).
fn view_transforms_are_equal(first: &Config, second: &Config, name: &[u8]) -> bool {
    let (Some(vt1), Some(vt2)) = (first.view_transform(name), second.view_transform(name)) else {
        return false;
    };
    // Both configs have a view transform by this name, now check the parts.
    // Note: Not checking family or description since it is not a functional difference.

    // TODO: Check categories.

    if vt1.reference_space_type() != vt2.reference_space_type() {
        return false;
    }

    for dir in [
        ViewTransformDirection::ToReference,
        ViewTransformDirection::FromReference,
    ] {
        match (vt1.transform(dir), vt2.transform(dir)) {
            (None, None) => {}
            // Compare transforms. NB: This is a fast comparison that does not load file
            // transforms.
            (Some(t1), Some(t2)) => {
                if t1.to_bytes() != t2.to_bytes() {
                    return false;
                }
            }
            // One of them has a transform but the other does not.
            _ => return false,
        }
    }

    true
}

/// The converters of a merger's state, or upstream's error for a null one.
///
/// The check of `ConfigUtils::updateReferenceView` (src/OpenColorIO/ConfigUtils.cpp:1063-1068
/// @ v2.5.2) for the null converters of a merger that didn't set them.
fn converters<'m>(m: &'m SectionMerger<'_, '_>) -> Result<(&'m Transform, &'m Transform)> {
    match (&m.input_to_base_gt_scene, &m.input_to_base_gt_display) {
        (Some(scene), Some(display)) => Ok((scene, display)),
        _ => Err(Exception::new(
            "Could not update view transform reference spaces, converter transforms were not \
             initialized.",
        )),
    }
}

/// "The Input config contains a value that would override the Base config:
/// default_view_transform: <input's>", the conflict of the default view transforms.
fn default_view_transform_message(input_name: &[u8]) -> Vec<u8> {
    override_message(b"default_view_transform", input_name)
}

/// Merges the view transforms and the default view transform.
///
/// Port of `ViewTransformsMerger` (SectionMerger.h:245-286, SectionMerger.cpp:1751-1914 @
/// v2.5.2).
pub(crate) struct ViewTransformsMerger<'o, 'a>(SectionMerger<'o, 'a>);

impl<'o, 'a> ViewTransformsMerger<'o, 'a> {
    /// The merger, with the converters of the reference spaces when the parameters adjust the
    /// input's reference space (which fails as `initializeRefSpaceConverters` does).
    ///
    /// Port of `ViewTransformsMerger::ViewTransformsMerger` (SectionMerger.h:248-268 @ v2.5.2).
    pub(crate) fn new(options: &'o mut MergeHandlerOptions<'a>) -> Result<Self> {
        let mut m = SectionMerger::new(options)?;
        m.strategy = section_strategy(m.params.view_transforms(), m.params);

        if m.params.is_adjust_input_reference_space() {
            // TODO: This may throw, is it worth continuing without ref space conversion?
            // Just call LogError() and continue?
            m.initialize_ref_space_converters()?;
        }
        Ok(ViewTransformsMerger(m))
    }

    /// Adds the view transform `name` of `cfg` to the merged config, when `cfg` has it: with
    /// the converters of the reference spaces (`ConfigUtils::updateReferenceView`) when it comes
    /// from the input and the parameters adjust the input's reference space.
    ///
    /// Port of `ViewTransformsMerger::addViewTransform` (SectionMerger.cpp:1753-1771 @ v2.5.2).
    fn add_view_transform(&mut self, cfg: &Config, name: &[u8], is_input: bool) -> Result<()> {
        let m = &mut self.0;
        let Some(vt) = cfg.view_transform(name) else {
            return Ok(());
        };

        if !is_input || !m.params.is_adjust_input_reference_space() {
            m.merged_config.add_view_transform(vt)
        } else {
            // Add the reference space adapter transforms.
            let mut e_vt = vt.clone();
            let (scene, display) = converters(m)?;
            config_utils::update_reference_view(&mut e_vt, scene, display)?;
            m.merged_config.add_view_transform(&e_vt)
        }
    }

    /// Adds the view transforms of `cfg` whose names the merged config doesn't have.
    ///
    /// Port of `ViewTransformsMerger::addUniqueViewTransforms` (SectionMerger.cpp:1773-1784 @
    /// v2.5.2).
    fn add_unique_view_transforms(&mut self, cfg: &Config, is_input: bool) -> Result<()> {
        for i in 0..cfg.num_view_transforms() {
            let name = cfg.view_transform_name_by_index(i);
            // Take the view from the config if it does not exist in merged config.
            if self.0.merged_config.view_transform(name).is_none() {
                self.add_view_transform(cfg, name, is_input)?;
            }
        }
        Ok(())
    }

    /// The view transforms of `first`, in its order, each from `second` where it has one and
    /// `prefer_second`, then those only `second` has; one of both that differs is a conflict.
    ///
    /// Port of `ViewTransformsMerger::processViewTransforms` (SectionMerger.cpp:1786-1817 @
    /// v2.5.2).
    fn process_view_transforms(
        &mut self,
        first: &Config,
        second: &Config,
        prefer_second: bool,
        second_is_input: bool,
    ) -> Result<()> {
        for i in 0..first.num_view_transforms() {
            let name = first.view_transform_name_by_index(i);
            if !name.is_empty() {
                let vt2 = second.view_transform(name);
                if vt2.is_some() && !view_transforms_are_equal(first, second, name) {
                    let os = override_message(b"view_transforms", name);
                    self.0.notify(&os, self.0.params.is_error_on_conflict())?;
                }

                if vt2.is_some() && prefer_second {
                    self.add_view_transform(second, name, second_is_input)?;
                } else {
                    self.add_view_transform(first, name, !second_is_input)?;
                }
            }
        }

        // Add the remaining unique view transforms.

        self.add_unique_view_transforms(second, second_is_input)
    }

    /// The view transforms of both configs, `first` first (the input with `input_first`),
    /// preferring `second`'s with `prefer_second`; a conflict when the default view transforms
    /// differ (ignoring case), also when one is empty (docs/improvements.md, I-433).
    ///
    /// The view transforms part of `ViewTransformsMerger::handlePreferInput` and
    /// `handlePreferBase` (SectionMerger.cpp:1819-1873 @ v2.5.2), and the check of their default
    /// view transforms.
    fn merge_both(&mut self, input_first: bool, prefer_second: bool) -> Result<()> {
        let (base, input) = (self.0.base_config, self.0.input_config);
        // Merge view_transforms.
        self.0.merged_config.clear_view_transforms();
        if input_first {
            self.process_view_transforms(input, base, prefer_second, false)?;
        } else {
            self.process_view_transforms(base, input, prefer_second, true)?;
        }

        // Merge default_view_transform.
        let base_name = base.default_view_transform_name();
        let input_name = input.default_view_transform_name();
        if strcasecmp(base_name, input_name).is_ne() {
            self.0.notify(
                &default_view_transform_message(input_name),
                self.0.params.is_error_on_conflict(),
            )?;
        }
        Ok(())
    }
}

impl<'o, 'a> Section<'o, 'a> for ViewTransformsMerger<'o, 'a> {
    fn state(&self) -> &SectionMerger<'o, 'a> {
        &self.0
    }

    /// Port of `ViewTransformsMerger::getName` (SectionMerger.h:271 @ v2.5.2).
    fn name(&self) -> &'static str {
        "View Transforms"
    }

    /// Both configs' view transforms, the input's where both have one; the input's default
    /// view transform unless it is empty.
    ///
    /// Port of `ViewTransformsMerger::handlePreferInput` (SectionMerger.cpp:1819-1845 @ v2.5.2).
    fn handle_prefer_input(&mut self) -> Result<()> {
        if self.0.params.is_input_first() {
            self.merge_both(true, false)?;
        } else {
            self.merge_both(false, true)?;
        }

        // If the input config does not specify a default, keep the one from the base.
        let input_name = self.0.input_config.default_view_transform_name();
        if !input_name.is_empty() {
            self.0
                .merged_config
                .set_default_view_transform_name(input_name);
        }
        Ok(())
    }

    /// Both configs' view transforms, the base's where both have one; the base's default view
    /// transform unless it is empty.
    ///
    /// Port of `ViewTransformsMerger::handlePreferBase` (SectionMerger.cpp:1847-1873 @ v2.5.2).
    fn handle_prefer_base(&mut self) -> Result<()> {
        if self.0.params.is_input_first() {
            self.merge_both(true, true)?;
        } else {
            self.merge_both(false, false)?;
        }

        // Only use the input if the base is missing.
        let base_name = self.0.base_config.default_view_transform_name();
        if base_name.is_empty() {
            let input_name = self.0.input_config.default_view_transform_name();
            self.0
                .merged_config
                .set_default_view_transform_name(input_name);
        }
        Ok(())
    }

    /// The input's view transforms and default view transform.
    ///
    /// Port of `ViewTransformsMerger::handleInputOnly` (SectionMerger.cpp:1875-1883 @ v2.5.2).
    fn handle_input_only(&mut self) -> Result<()> {
        // Merge view_transforms.
        self.0.merged_config.clear_view_transforms();
        let input = self.0.input_config;
        self.add_unique_view_transforms(input, true)?;

        // Merge default_view_transform.
        self.0
            .merged_config
            .set_default_view_transform_name(input.default_view_transform_name());
        Ok(())
    }

    /// Port of `ViewTransformsMerger::handleBaseOnly` (SectionMerger.cpp:1885-1887 @ v2.5.2).
    fn handle_base_only(&mut self) -> Result<()> {
        Ok(())
    }

    /// The base's view transforms that the input doesn't have (by name); the default view
    /// transform emptied when it names none of them.
    ///
    /// Port of `ViewTransformsMerger::handleRemove` (SectionMerger.cpp:1889-1910 @ v2.5.2).
    fn handle_remove(&mut self) -> Result<()> {
        let m = &mut self.0;
        // Remove from view_transforms.
        m.merged_config.clear_view_transforms();
        // Add view transforms that are present in the base config and NOT present in the input
        // config.
        for i in 0..m.base_config.num_view_transforms() {
            let name = m.base_config.view_transform_name_by_index(i);
            if m.input_config.view_transform(name).is_none() {
                let vt = m
                    .base_config
                    .view_transform(name)
                    .expect("a view transform the config lists");
                m.merged_config.add_view_transform(vt)?;
            }
        }

        // Handle default_view_transform.
        // Leave the base alone unless it identified a view transform that was removed.
        let base_name = m.base_config.default_view_transform_name();
        if m.merged_config.view_transform(base_name).is_none() {
            // Set to empty string, the first view transform will be used by default.
            m.merged_config.set_default_view_transform_name(b"");
        }
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

/////////////////////////////////// ColorspacesMerger ////////////////////////////////////

/// Whether `cfg` has the search path `path`, ignoring case.
///
/// Port of `hasSearchPath` (SectionMerger.cpp:2031-2041 @ v2.5.2).
fn has_search_path(cfg: &Config, path: &[u8]) -> bool {
    (0..cfg.num_search_paths()).any(|i| strcasecmp(cfg.search_path_with_index(i), path).is_eq())
}

/// `s` with each `in_sep` replaced by `out_sep`.
///
/// Port of `replaceSeparator` (SectionMerger.cpp:2072-2078 @ v2.5.2).
fn replace_separator(s: &[u8], in_sep: u8, out_sep: u8) -> Vec<u8> {
    replace(s, &[in_sep], &[out_sep])
}

/// The converter of a merger's state for a color space, or upstream's error for a null one.
///
/// The check of `ConfigUtils::updateReferenceColorspace` (src/OpenColorIO/ConfigUtils.cpp:
/// 1008-1013 @ v2.5.2) for the null converter of a merger that didn't set it.
fn converter(t: &Option<Transform>) -> Result<&Transform> {
    t.as_ref().ok_or_else(|| {
        Exception::new("Could not update reference space, converter transform was not initialized.")
    })
}

/// Whether `strategy` takes the input's where both configs have one: `PreferInput` or
/// `InputOnly`.
fn prefers_input(strategy: MergeStrategies) -> bool {
    strategy == MergeStrategies::PreferInput || strategy == MergeStrategies::InputOnly
}

/// Merges the color spaces (scene- and display-referred, inactive ones included), the
/// environment, the search paths, the inactive color spaces and the family separator.
///
/// Port of `ColorspacesMerger` (SectionMerger.h:314-421, SectionMerger.cpp:2082-3072 @
/// v2.5.2). The header declares 15 more functions (`iterateOverColorSpaces`,
/// `detectSpecificIndirectConflicts`, the `nameConflict...` and `handleAddCsError...` ones,
/// `resolveConflict`, `handleErrorCodes`, `handleAssumeCommonReferenceOption`,
/// `handleMarkedToBeDeletedColorspaces`, `isColorspaceMarked`, `addFamilyPrefix`,
/// `replaceFamilySeparatorInFamily`) and the struct `ColorspaceNameConflict`, which OCIO 2.5.2
/// neither defines nor uses.
pub(crate) struct ColorspacesMerger<'o, 'a>(SectionMerger<'o, 'a>);

impl<'o, 'a> ColorspacesMerger<'o, 'a> {
    /// The merger, with the converters of the reference spaces when the parameters adjust the
    /// input's reference space (which fails as `initializeRefSpaceConverters` does).
    ///
    /// Port of `ColorspacesMerger::ColorspacesMerger` (SectionMerger.h:317-335 @ v2.5.2).
    pub(crate) fn new(options: &'o mut MergeHandlerOptions<'a>) -> Result<Self> {
        let mut m = SectionMerger::new(options)?;
        m.strategy = section_strategy(m.params.colorspaces(), m.params);

        if m.params.is_adjust_input_reference_space() {
            m.initialize_ref_space_converters()?;
        }
        Ok(ColorspacesMerger(m))
    }

    /// The search paths: the override where it isn't empty; else, with the color spaces'
    /// strategy `PreferInput`, the input's then those of the base the input doesn't have
    /// (ignoring case); else the base's (the merged config's) then those of the input the base
    /// doesn't have. The input-first option isn't read.
    ///
    /// Port of `ColorspacesMerger::processSearchPaths` (SectionMerger.cpp:2082-2129 @ v2.5.2).
    fn process_search_paths(&mut self) {
        let m = &mut self.0;
        let search_paths = m.params.search_path();
        if !search_paths.is_empty() {
            // Use the override.
            m.merged_config.set_search_path(&search_paths);
            return;
        }

        // Ignoring isInputFirst for the ordering of search paths because it seems that it
        // really should be driven by the strategy. E.g., if both base and input have a "luts"
        // directory, want to be looking in the right one.

        // TODO: More work is needed here.  Absolute paths should be set up for input since
        // the working dir is from the base config.

        let (base, input) = (m.base_config, m.input_config);
        if m.params.colorspaces() == MergeStrategies::PreferInput {
            m.merged_config.clear_search_paths();
            // Add all from input config.
            for i in 0..input.num_search_paths() {
                m.merged_config
                    .add_search_path(input.search_path_with_index(i));
            }

            // Only add the new ones from the base config.
            for i in 0..base.num_search_paths() {
                let path = base.search_path_with_index(i);
                if !has_search_path(input, &path) {
                    m.merged_config.add_search_path(&path);
                }
            }
        } else {
            // NB: The m_mergedConfig is initialized with the contents of the m_baseConfig.

            for i in 0..input.num_search_paths() {
                let path = input.search_path_with_index(i);
                if !has_search_path(base, &path) {
                    m.merged_config.add_search_path(&path);
                }
            }
        }
    }

    /// Prefixes `family` with the family prefix of its config (`from_base` or the input),
    /// separated as the merged config's families (the prefixes are written with `/`), and
    /// changes the separator of a family of the config that isn't preferred to the merged
    /// config's. Nothing changes unless the color spaces' strategy is `PreferInput` or
    /// `PreferBase`. A family without a prefix gets the prefix too.
    ///
    /// Port of `ColorspacesMerger::updateFamily` (SectionMerger.cpp:2131-2174 @ v2.5.2).
    fn update_family(&self, family: &mut Vec<u8>, from_base: bool) {
        let m = &self.0;
        // Note that if a prefix is present, it is always added, even if the CS did not have a
        // family.

        let merged_sep = m.merged_config.family_separator();
        let mut updated_prefix = Vec::new();
        let strategy = m.params.colorspaces();
        if strategy == MergeStrategies::PreferInput {
            if from_base {
                // If the color space is from the base config, need to update its family
                // separator.
                if !family.is_empty() {
                    *family =
                        replace_separator(family, m.base_config.family_separator(), merged_sep);
                }
                // Note: The family prefix argument must always use the default slash separator.
                // TODO: Should do this just once in the initializer.
                updated_prefix =
                    replace_separator(&m.params.base_family_prefix(), b'/', merged_sep);
            } else {
                updated_prefix =
                    replace_separator(&m.params.input_family_prefix(), b'/', merged_sep);
            }
        } else if strategy == MergeStrategies::PreferBase {
            if from_base {
                // TODO: Should do this just once in the initializer.
                updated_prefix =
                    replace_separator(&m.params.base_family_prefix(), b'/', merged_sep);
            } else {
                // If the color space is from the input config, need to update its family
                // separator.
                if !family.is_empty() {
                    *family =
                        replace_separator(family, m.input_config.family_separator(), merged_sep);
                }
                updated_prefix =
                    replace_separator(&m.params.input_family_prefix(), b'/', merged_sep);
            }
        }
        // Append the prefix to the family.
        // Note that the prefix should end with a separator, if desired.  Not adding one here.
        updated_prefix.extend_from_slice(family);
        *family = updated_prefix;
    }

    /// Adds `alias_name` to `dupe_cs`, the base color space `input_cs` duplicates, unless
    /// `dupe_cs` has it as its name or an alias; a name of `merge_config` (a color space, an
    /// alias or a role) is a conflict and isn't added.
    ///
    /// Port of `ColorspacesMerger::attemptToAddAlias` (SectionMerger.cpp:2176-2218 @ v2.5.2).
    fn attempt_to_add_alias(
        &self,
        merge_config: &Config,
        dupe_cs: &mut ColorSpace,
        input_cs: &ColorSpace,
        alias_name: &[u8],
    ) -> Result<()> {
        // It is assumed that the base and input configs start out in a legal state,
        // however, when adding anything from one config to another, must always
        // check that it doesn't conflict with anything.

        // It is assumed that the strategy is prefer_base when this function is called.

        // It's OK if aliasName is used in the duplicate color space itself.
        if strcasecmp(dupe_cs.name(), alias_name).is_eq() || dupe_cs.has_alias(alias_name) {
            // It's already present, no need to add anything.
            return Ok(());
        }

        // Check if aliasName is already a name that is used in the config.

        if let Some(conflicting_cs) = merge_config.color_space(alias_name) {
            // The conflict could be the name of a color space, an alias, or a role.
            // But it doesn't matter, the strategy is prefer base so don't want to
            // remove this conflict from the base config to accomodate adding an
            // alias from the input config.
            let os = [
                b"Input color space '".as_slice(),
                input_cs.name(),
                b"'",
                b" is a duplicate of base color space '",
                dupe_cs.name(),
                b"' but was unable to add alias '",
                alias_name,
                b"' since it conflicts with base color space '",
                conflicting_cs.name(),
                b"'.",
            ]
            .concat();
            self.0.notify(&os, self.0.params.is_error_on_conflict())?;

            return Ok(());
        }

        // No conflicts encountered, it's ok to update the alias and add it to the config.

        dupe_cs.add_alias(alias_name);
        Ok(())
    }

    /// With the option to avoid duplicates (and unless `input_cs` has the category
    /// `allow-duplicate`), looks for a color space of the base config equivalent to `input_cs`
    /// (`ConfigUtils::findEquivalentColorspace`). With the color spaces' strategy
    /// `PreferInput`, `input_cs` takes its name, aliases and categories, and it is removed from
    /// `e_base`; with `PreferBase`, it takes the name, aliases (where they don't conflict) and
    /// categories of `input_cs`, which isn't merged. Returns whether `input_cs` is to be
    /// merged.
    ///
    /// Port of `ColorspacesMerger::handleAvoidDuplicatesOption` (SectionMerger.cpp:2220-2348 @
    /// v2.5.2).
    fn handle_avoid_duplicates_option(
        &self,
        fingerprints: &ColorSpaceFingerprints,
        e_base: &mut Config,
        input_config: &Config,
        input_cs: &mut ColorSpace,
    ) -> Result<bool> {
        let m = &self.0;
        let mut not_duplicate = true;

        if !m.params.is_avoid_duplicates() {
            return Ok(not_duplicate);
        }
        // If a color space has the allow-dupes category, don't check if it's a duplicate.
        if input_cs.has_category("allow-duplicate") {
            return Ok(not_duplicate);
        }

        // Note: The search for duplicate color spaces only searches for color spaces with
        // the same reference space type (i.e., scene or display), so it won't remove spaces
        // that are otherwise equivalent (e.g., an sRGB transform).
        //
        // However, some configs may intentionally have duplicate color spaces (e.g., aliases
        // from v1 configs). Since this search is only done using spaces from the input config,
        // those duplicates in the base won't be removed. But if the input config contains
        // duplicates, those will be condensed into one space containing aliases for all of
        // names of the duplicates.
        //
        // Note: By design, inactive color spaces are included in the search for equivalents
        // (e.g., consider the CIE-XYZ-D65 space, which is typically inactive). However,
        // when the inactive list is regenerated to avoid listing removed color spaces,
        // some color spaces that were inactive may become active.
        let duplicate_in_base =
            config_utils::find_equivalent_colorspace(fingerprints, input_config, input_cs)?
                .to_vec();

        let strategy = m.params.colorspaces();
        if !duplicate_in_base.is_empty() {
            if strategy == MergeStrategies::PreferInput {
                // Add the name and aliases from the duplicate colorspace to the input
                // colorspace.
                //
                // Note that the aliases added here should not have conflicts with the base
                // config (since that's where they originated), but may cause conflicts with
                // other color spaces in the input config, but these will be handled as the
                // spaces get added to the merged config by the calling function.
                //
                // If the base config has more than one color space equivalent to inputCS, only
                // the first is replaced.

                if let Some(dupe_cs) = e_base.color_space(&duplicate_in_base) {
                    // Note that addAlias will check the argument and won't add it if it matches
                    // the name of the color space or one of the existing aliases.
                    input_cs.add_alias(dupe_cs.name());

                    for i in 0..dupe_cs.num_aliases() {
                        input_cs.add_alias(dupe_cs.alias(i));
                    }

                    // TODO: This should be controlled by a merge option.
                    for i in 0..dupe_cs.num_categories() {
                        input_cs.add_category(dupe_cs.category(i).unwrap_or_default());
                    }

                    // Trying to merge inputCS would now give misleading notifications about
                    // conflicts from the newly added aliases, so remove the duplicate.
                    // If there is more than one input CS that duplicates a given base CS,
                    // duplicateInBase will have been removed and is now an alias. Since
                    // removeCS does not work on aliases, get the name of the color space with
                    // that alias.
                    let duplicate_current_name = e_base.canonical_name(&duplicate_in_base).to_vec();
                    e_base.remove_color_space(&duplicate_current_name);

                    let os = [
                        b"Equivalent input color space '".as_slice(),
                        input_cs.name(),
                        b"' replaces '",
                        &duplicate_current_name,
                        b"' in the base config, preserving aliases.",
                    ]
                    .concat();
                    m.notify(&os, m.params.is_error_on_conflict())?;

                    // Still want the caller to proceed merging inputCS into the merge config.
                    not_duplicate = true;
                }
            } else if strategy == MergeStrategies::PreferBase {
                // Don't add the input color space, but add it's name and alias to the
                // duplicate. Need to be more careful of conflicts, since the modified color
                // space is receiving aliases from the input config and yet is not going through
                // the mergeColorSpace checking below.
                //
                // If the input config has more than one CS that is equivalent to a base CS,
                // they are all condensed into that first equivalent base CS.

                if let Some(cs) = e_base.color_space(&duplicate_in_base) {
                    let mut e_cs = cs.clone();

                    self.attempt_to_add_alias(e_base, &mut e_cs, input_cs, input_cs.name())?;

                    for i in 0..input_cs.num_aliases() {
                        self.attempt_to_add_alias(e_base, &mut e_cs, input_cs, input_cs.alias(i))?;
                    }

                    for i in 0..input_cs.num_categories() {
                        e_cs.add_category(input_cs.category(i).unwrap_or_default());
                    }

                    // Replace the color space in the merge config. (This preserves its
                    // order in the color space list.)
                    e_base.add_color_space(&e_cs)?;

                    let os = [
                        b"Equivalent base color space '".as_slice(),
                        &duplicate_in_base,
                        b"' overrides '",
                        input_cs.name(),
                        b"' in the input config, preserving aliases.",
                    ]
                    .concat();
                    m.notify(&os, m.params.is_error_on_conflict())?;

                    // The base color space is edited here, don't want to add inputCS.
                    not_duplicate = false;
                }
            }
        }
        Ok(not_duplicate)
    }

    /// Whether the input's color space `input_cs` may be merged into `merge_config`: always
    /// when no name of `merge_config` is its name; never when a role is; else by the color
    /// spaces' strategy (`PreferInput` and `InputOnly` replace the color space or take the
    /// alias of that name), each case with a message.
    ///
    /// Port of `ColorspacesMerger::colorSpaceMayBeMerged` (SectionMerger.cpp:2350-2448 @
    /// v2.5.2), whose null color space can't happen with a reference.
    fn color_space_may_be_merged(
        &self,
        merge_config: &Config,
        input_cs: &ColorSpace,
    ) -> Result<bool> {
        let m = &self.0;
        // This should only be called on color spaces from the input config.

        // NB: This routine assumes all NamedTransforms have been removed from the mergeConfig.
        // Not trying to handle name conflicts with NamedTransforms, color spaces have
        // precedence.

        let name = input_cs.name();

        // This will compare the name against roles, color space names, and aliases.
        let Some(existing_cs) = merge_config.color_space(name) else {
            // No name conflicts, go ahead and add it.
            return Ok(true);
        };

        // OK, something has this name, figure out what it is.

        // Does it have the same name as a role?
        if merge_config.has_role(name) {
            // Don't merge it if it would override a role.

            let os = [
                b"Color space '".as_slice(),
                name,
                b"' was not merged as it's identical to a role name.",
            ]
            .concat();
            m.notify(&os, m.params.is_error_on_conflict())?;

            return Ok(false);
        }

        let strategy = m.params.colorspaces();

        // Does it have the same name as another color space?
        if strcasecmp(existing_cs.name(), name).is_eq() {
            // The name matches a color space name in the mergeConfig.
            // Whether to allow the merge is based on the merge strategy.

            if prefers_input(strategy) {
                // Allow the merger.

                let os = [
                    b"Color space '".as_slice(),
                    name,
                    b"' will replace a color space in the base config.",
                ]
                .concat();
                m.notify(&os, m.params.is_error_on_conflict())?;

                Ok(true)
            } else {
                // Don't merge since it would replace a color space from the base config.

                let os = [
                    b"Color space '".as_slice(),
                    name,
                    b"' was not merged as it's already present in the base config.",
                ]
                .concat();
                m.notify(&os, m.params.is_error_on_conflict())?;

                Ok(false)
            }
        } else {
            // The name conflicts with an alias of another color space.
            // Whether to allow the merge is based on the merge strategy.

            if prefers_input(strategy) {
                // Allow the merger.

                let os = [
                    b"The name of merged color space '".as_slice(),
                    name,
                    b"'",
                    b" has a conflict with an alias in color space '",
                    existing_cs.name(),
                    b"'.",
                ]
                .concat();
                m.notify(&os, m.params.is_error_on_conflict())?;

                Ok(true)
            } else {
                // Don't merge it if it would replace an alias from the base config.

                let os = [
                    b"Color space '".as_slice(),
                    name,
                    b"' was not merged as it conflicts with an alias ",
                    b"in color space '",
                    existing_cs.name(),
                    b"'.",
                ]
                .concat();
                m.notify(&os, m.params.is_error_on_conflict())?;

                Ok(false)
            }
        }
    }

    /// Adds `e_input_cs` to `merge_config`, replacing the color space of its name, taking its
    /// name from the alias of another color space, and resolving the conflicts of its aliases
    /// with color spaces, aliases and roles by the color spaces' strategy (each with a
    /// message); records its name in `added_input_color_spaces`.
    ///
    /// Port of `ColorspacesMerger::mergeColorSpace` (SectionMerger.cpp:2450-2618 @ v2.5.2).
    fn merge_color_space(
        &self,
        merge_config: &mut Config,
        e_input_cs: &mut ColorSpace,
        added_input_color_spaces: &mut Vec<Vec<u8>>,
    ) -> Result<()> {
        let m = &self.0;
        // NB: This routine assumes all NamedTransforms have been removed from the mergeConfig.
        // Not trying to handle name conflicts with NamedTransforms, color spaces have
        // precedence.

        // Check if mergeConfig already has a color space with the same name as the input CS.

        let name = e_input_cs.name().to_vec();

        if let Some(original_cs) = merge_config.color_space(&name) {
            // Note that the color space which gets discarded and the color space being added
            // below may not have the same reference space type (i.e., scene vs. display). This
            // is currently allowed but log a warning.
            if e_input_cs.reference_space_type() != original_cs.reference_space_type() {
                let os = [
                    b"Merged color space '".as_slice(),
                    &name,
                    b"' has a different reference ",
                    b"space type than the color space it's replacing.",
                ]
                .concat();
                m.notify(&os, false)?;
            }

            // If there is a color space with this name in the existing config,
            // remove it (and any aliases it may contain). This is the case when
            // the strategy calls for replacing an existing color space.
            //
            // If the eInputCS name matched an alias rather than a color space name,
            // this does nothing (and the alias is handled below).
            //
            // The notification is handled in colorSpaceMayBeMerged to avoid having to determine
            // again whether the conflict is with the name or alias of originalCS.

            merge_config.remove_color_space(&name);
        }

        // Handle conflicts of the eInputCS name with aliases of other color spaces.

        if let Some(existing_cs) = merge_config.color_space(&name) {
            // Get the name of the color space that contains the alias.

            // Verify that the name is actually an alias rather than some other conflict.
            // (Should never happen.)
            if !existing_cs.has_alias(&name) {
                return Err(Exception::new(
                    [b"Problem merging color space: '".as_slice(), &name, b"'."].concat(),
                ));
            }

            // Remove the alias from that existing color space.
            //    Note that this conflict was detected and allowed in colorSpaceMayBeMerged
            //    based on the merge strategy, so the decision has already been made to remove
            //    this alias from a color space in the base config.

            let mut e_existing_cs = existing_cs.clone();
            e_existing_cs.remove_alias(&name);
            // Edit the colorspace in the copy of the merged config.
            merge_config.add_color_space(&e_existing_cs)?;

            // The notification is handled in colorSpaceMayBeMerged to avoid having to determine
            // again whether the conflict is with the name or alias of originalCS.
        }

        let strategy = m.params.colorspaces();

        //
        // Handle conflicts of the eInputCS aliases with other color spaces or aliases.
        //

        // First initialize the list of names, since eInputCS is being edited within the loop.
        let alias_names: Vec<Vec<u8>> = (0..e_input_cs.num_aliases())
            .map(|i| e_input_cs.alias(i).to_vec())
            .collect();

        for alias_name in &alias_names {
            let Some(conflicting_cs) = merge_config.color_space(alias_name) else {
                continue;
            };
            let conflicting_cs = conflicting_cs.clone();

            let os = if strcasecmp(conflicting_cs.name(), alias_name).is_eq() {
                // The alias conflicts with the name of an existing color space.

                let os = [
                    b"Merged color space '".as_slice(),
                    &name,
                    b"'",
                    b" has an alias '",
                    alias_name,
                    b"' that conflicts with color space '",
                    conflicting_cs.name(),
                    b"'.",
                ]
                .concat();

                if prefers_input(strategy) {
                    // Remove that base color space.
                    merge_config.remove_color_space(conflicting_cs.name());
                } else {
                    // Remove the alias from the input color space.
                    e_input_cs.remove_alias(alias_name);
                }
                os
            } else if conflicting_cs.has_alias(alias_name) {
                // The alias conflicts with an alias of the conflicting color space.

                let os = [
                    b"Merged color space '".as_slice(),
                    &name,
                    b"'",
                    b" has a conflict with alias '",
                    alias_name,
                    b"' in color space '",
                    conflicting_cs.name(),
                    b"'.",
                ]
                .concat();

                if prefers_input(strategy) {
                    // Remove the alias from that base color space.
                    let mut e_conflicting_cs = conflicting_cs.clone();
                    e_conflicting_cs.remove_alias(alias_name);

                    // Replace the colorspace in the copy of the merged config.
                    merge_config.add_color_space(&e_conflicting_cs)?;
                } else {
                    // Remove the alias from the input color space.
                    e_input_cs.remove_alias(alias_name);
                }
                os
            } else if merge_config.has_role(alias_name) {
                let os = [
                    b"Merged color space '".as_slice(),
                    &name,
                    b"'",
                    b" has an alias '",
                    alias_name,
                    b"' that conflicts with a role.",
                ]
                .concat();

                // Remove the alias from the input color space.
                e_input_cs.remove_alias(alias_name);
                os
            } else {
                // (Should never happen.)
                return Err(Exception::new(
                    [
                        b"Problem merging color space: '".as_slice(),
                        &name,
                        b"' due to its aliases.",
                    ]
                    .concat(),
                ));
            };

            m.notify(&os, m.params.is_error_on_conflict())?;
        }

        // Add the color space. This will throw if a problem is found.
        // (But all name conflicts should have been handled already.)
        merge_config.add_color_space(e_input_cs)?;

        // Keep a record that this input color space was added to allow reordering later.
        added_input_color_spaces.push(name);

        // TODO: Is it ever possible that a CS added to the list would be removed as another is
        // merged?

        // TODO: When color spaces or aliases are removed above, it's possible it could break
        // some other part of the config that referenced them. This would include elements such
        // as: environment, views, inactive_colorspaces, ColorSpaceTransforms, or
        // DisplayViewTransforms.
        Ok(())
    }

    /// The color spaces of both configs, the input's (adjusted to the base's reference spaces
    /// with the option, without duplicates of the base's with the option) merged into a copy
    /// of the merged config, then written in order (the input's first with the input-first
    /// option) with their family prefixes. The named transforms are removed.
    ///
    /// Port of `ColorspacesMerger::addColorSpaces` (SectionMerger.cpp:2620-2758 @ v2.5.2).
    fn add_color_spaces(&mut self) -> Result<()> {
        // Delete all the NamedTransforms, color spaces take precedence, so don't want them
        // interfering with merges by causing name conflicts.

        // NB: This is only intended to be called for the prefer_input and prefer_base
        // strategies.

        self.0.merged_config.clear_named_transforms();

        // Make a temp copy to merge the input color spaces into (will reorder them later).
        let mut merge_config = self.0.merged_config.clone();

        merge_config.clear_named_transforms();

        // Loop over all active and inactive color spaces of all reference types in the input
        // config. Merge them into the temp config (which already contains the base color
        // spaces).
        let mut added_input_color_spaces: Vec<Vec<u8>> = Vec::new();

        let mut fingerprints = ColorSpaceFingerprints::default();
        if self.0.params.is_avoid_duplicates() {
            config_utils::initialize_color_space_fingerprints(
                &mut fingerprints,
                self.0.base_config,
            )?;
        }

        let input = self.0.input_config;
        for i in
            0..input.num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::All)
        {
            let name = input.color_space_name_by_index_with(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All,
                i,
            );
            let Some(cs) = input.color_space(name) else {
                continue;
            };

            let mut e_cs = cs.clone();

            if self.0.params.is_adjust_input_reference_space() {
                if e_cs.reference_space_type() == ReferenceSpaceType::Display {
                    let t = converter(&self.0.input_to_base_gt_display)?;
                    config_utils::update_reference_colorspace(&mut e_cs, t)?;
                } else {
                    let t = converter(&self.0.input_to_base_gt_scene)?;
                    config_utils::update_reference_colorspace(&mut e_cs, t)?;
                }
            }

            // Doing this against the mergedConfig rather than the base config so that the most
            // recent state of any aliases that get added or color spaces that are removed are
            // considered by the duplicate consolidation process.
            let not_duplicate = self.handle_avoid_duplicates_option(
                &fingerprints,
                &mut merge_config,
                input,
                &mut e_cs,
            )?;

            if not_duplicate && self.color_space_may_be_merged(&merge_config, &e_cs)? {
                // NB: This may make changes to existing color spaces in mergeConfig
                // to resolve name conflicts.
                self.merge_color_space(
                    &mut merge_config,
                    &mut e_cs,
                    &mut added_input_color_spaces,
                )?;
            }
        }

        self.0.merged_config.clear_color_spaces();

        // Add the color spaces to the real merged config.

        if self.0.params.is_input_first() {
            // Add color spaces from the input config.
            for name in &added_input_color_spaces {
                if let Some(input_cs) = merge_config.color_space(name) {
                    let mut e_input_cs = input_cs.clone();

                    // Add family prefix.
                    let from_base = false;
                    let mut family = e_input_cs.family().to_vec();
                    self.update_family(&mut family, from_base);
                    e_input_cs.set_family(&family);

                    self.0.merged_config.add_color_space(&e_input_cs)?;

                    merge_config.remove_color_space(name);
                }
            }

            // Add color spaces from the base config.
            for i in 0..merge_config
                .num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::All)
            {
                // Note that during the merge process, some of the color spaces from the base
                // config may be replaced if their aliases are edited. This should not change
                // their order in the config, but may want to account for this, if that ever
                // changes and they get moved to the end.
                let name = merge_config.color_space_name_by_index_with(
                    SearchReferenceSpaceType::All,
                    ColorSpaceVisibility::All,
                    i,
                );
                if let Some(base_cs) = merge_config.color_space(name) {
                    let mut e_base_cs = base_cs.clone();

                    // Add family prefix.
                    let from_base = true;
                    let mut family = e_base_cs.family().to_vec();
                    self.update_family(&mut family, from_base);
                    e_base_cs.set_family(&family);

                    self.0.merged_config.add_color_space(&e_base_cs)?;
                }
            }
        } else {
            // The color spaces should already be in the correct order.
            // Copy them into the real merged config and add family prefix.
            for i in 0..merge_config
                .num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::All)
            {
                let name = merge_config.color_space_name_by_index_with(
                    SearchReferenceSpaceType::All,
                    ColorSpaceVisibility::All,
                    i,
                );
                if let Some(cs) = merge_config.color_space(name) {
                    let mut e_cs = cs.clone();

                    // Add family prefix.
                    let from_base = !added_input_color_spaces.iter().any(|n| n == name);
                    let mut family = e_cs.family().to_vec();
                    self.update_family(&mut family, from_base);
                    e_cs.set_family(&family);

                    self.0.merged_config.add_color_space(&e_cs)?;
                }
            }
        }

        // (Not cleaning up the inactive list here, it would remove named transforms, wait until
        // after NT.)

        // TODO: What if the environment contains a color space that was removed?
        Ok(())
    }

    /// The environment variables of the overrides, replacing the merged config's, where there
    /// are any.
    fn set_environment_overrides(&mut self) {
        let m = &mut self.0;
        // Take environment variables from overrides.
        m.merged_config.clear_environment_vars();
        for i in 0..m.params.num_environment_vars() {
            m.merged_config.add_environment_var(
                m.params.environment_var(i),
                Some(&m.params.environment_var_value(i)),
            );
        }
    }

    /// The inactive color spaces of the override where it isn't empty, else those of both
    /// configs (the base's from the merged config unless the input comes first), either first,
    /// trimmed, without duplicates (ignoring case).
    ///
    /// The inactive color spaces part of `ColorspacesMerger::handlePreferInput` and
    /// `handlePreferBase` (SectionMerger.cpp:2791-2819, 2859-2887 @ v2.5.2).
    fn merge_inactive_color_spaces(&mut self) {
        let m = &mut self.0;
        let inactive_cs = m.params.inactive_color_spaces();
        if !inactive_cs.is_empty() {
            // Take inactive colorspaces from overrides.
            m.merged_config.set_inactive_color_spaces(&inactive_cs);
        } else {
            // Add inactive colorspaces from input config to base config.

            let mut merged_inactive_cs = StringVec::new();
            if m.params.is_input_first() {
                let mut base_inactive_cs = StringVec::new();
                split_active_list(m.base_config.inactive_color_spaces(), &mut base_inactive_cs);
                split_active_list(
                    m.input_config.inactive_color_spaces(),
                    &mut merged_inactive_cs,
                );

                merge_strings_without_duplicates(&base_inactive_cs, &mut merged_inactive_cs);
            } else {
                let mut input_inactive_cs = StringVec::new();
                split_active_list(
                    m.input_config.inactive_color_spaces(),
                    &mut input_inactive_cs,
                );
                split_active_list(
                    m.merged_config.inactive_color_spaces(),
                    &mut merged_inactive_cs,
                );

                merge_strings_without_duplicates(&input_inactive_cs, &mut merged_inactive_cs);
            }
            m.merged_config
                .set_inactive_color_spaces(join(&merged_inactive_cs, b','));
        }
    }
}

impl<'o, 'a> Section<'o, 'a> for ColorspacesMerger<'o, 'a> {
    fn state(&self) -> &SectionMerger<'o, 'a> {
        &self.0
    }

    /// Port of `ColorspacesMerger::getName` (SectionMerger.h:348 @ v2.5.2).
    fn name(&self) -> &'static str {
        "Color Spaces"
    }

    /// The environment of both configs, the input's values where both have a variable (or the
    /// overrides'); the search paths; the inactive color spaces; the input's family separator;
    /// the color spaces of both, the input's where both have one.
    ///
    /// Port of `ColorspacesMerger::handlePreferInput` (SectionMerger.cpp:2761-2827 @ v2.5.2).
    fn handle_prefer_input(&mut self) -> Result<()> {
        // Set environment.
        //     Since the environment variables are stored inside a std::map, the key are ordered
        //     alphabetically. Therefore, there is no point to look at the option "InputFirst".
        if self.0.params.num_environment_vars() > 0 {
            self.set_environment_overrides();
        } else {
            let m = &mut self.0;
            // Add environment variables from input config to base config.
            for i in 0..m.input_config.num_environment_vars() {
                let name = m.input_config.environment_var_name_by_index(i);
                // Overwrite any existing env. variable with the same name.
                m.merged_config
                    .add_environment_var(name, Some(m.input_config.environment_var_default(name)));
            }
        }

        // Set search_path.
        self.process_search_paths();

        // Set inactive_colorspaces.
        self.merge_inactive_color_spaces();

        // Set family_separator.
        let separator = self.0.input_config.family_separator();
        self.0.merged_config.set_family_separator(separator)?;

        // Merge the color spaces.
        self.add_color_spaces()
    }

    /// The environment of both configs (or the overrides'), the base's values where both have a
    /// variable, one whose value is empty counting as missing (docs/improvements.md, I-435); the
    /// search paths; the inactive color spaces; the base's family separator; the color spaces of
    /// both, the base's where both have one.
    ///
    /// Port of `ColorspacesMerger::handlePreferBase` (SectionMerger.cpp:2829-2899 @ v2.5.2).
    fn handle_prefer_base(&mut self) -> Result<()> {
        // Set environment.
        //     Since the environment variables are stored inside a std::map, the key are ordered
        //     alphabetically. Therefore, there is no point to look at the option "InputFirst".
        if self.0.params.num_environment_vars() > 0 {
            self.set_environment_overrides();
        } else {
            let m = &mut self.0;
            // Take environment variables from config.
            for i in 0..m.input_config.num_environment_vars() {
                let name = m.input_config.environment_var_name_by_index(i);
                // If the var's default value is empty, it doesn't exist, so nothing to
                // overwrite.
                let var_does_not_exist = m.merged_config.environment_var_default(name).is_empty();
                if var_does_not_exist {
                    m.merged_config.add_environment_var(
                        name,
                        Some(m.input_config.environment_var_default(name)),
                    );
                }
            }
        }

        // Set search_path.
        self.process_search_paths();

        // Set inactive_colorspaces.
        self.merge_inactive_color_spaces();

        // Set family_separator.
        let separator = self.0.base_config.family_separator();
        self.0.merged_config.set_family_separator(separator)?;

        // Merge the color spaces.
        self.add_color_spaces()
    }

    /// The input's environment (or the overrides'), search paths (or the override), inactive
    /// color spaces (or the override), family separator and color spaces; no named transforms.
    ///
    /// Port of `ColorspacesMerger::handleInputOnly` (SectionMerger.cpp:2901-2973 @ v2.5.2).
    fn handle_input_only(&mut self) -> Result<()> {
        // Set environment.
        if self.0.params.num_environment_vars() > 0 {
            self.set_environment_overrides();
        } else {
            let m = &mut self.0;
            // Take environment variables from config.
            m.merged_config.clear_environment_vars();
            for i in 0..m.input_config.num_environment_vars() {
                let name = m.input_config.environment_var_name_by_index(i);
                m.merged_config
                    .add_environment_var(name, Some(m.input_config.environment_var_default(name)));
            }
        }

        let m = &mut self.0;
        let input = m.input_config;

        // Set search_path.
        let search_paths = m.params.search_path();
        if !search_paths.is_empty() {
            // Use the override.
            m.merged_config.set_search_path(&search_paths);
        } else {
            m.merged_config.set_search_path(input.search_path());
        }

        // Set inactive_colorspaces.
        let inactive_cs = m.params.inactive_color_spaces();
        if !inactive_cs.is_empty() {
            // Take inactive color spaces from overrides.
            m.merged_config.set_inactive_color_spaces(&inactive_cs);
        } else {
            // Take inactive color spaces from config.
            m.merged_config
                .set_inactive_color_spaces(input.inactive_color_spaces());
        }

        // Set family_separator.
        m.merged_config
            .set_family_separator(input.family_separator())?;

        // Remove all color spaces from base config.
        m.merged_config.clear_color_spaces();

        // Take color spaces from input config.
        // No error expected as it only adds the color spaces from the input config.

        // Avoid any conflicts with named transforms from the base config.
        m.merged_config.clear_named_transforms();

        // Merge the color spaces.
        let num_cs =
            input.num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::All);
        for i in 0..num_cs {
            let name = input.color_space_name_by_index_with(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All,
                i,
            );
            let cs = input
                .color_space(name)
                .expect("a color space the config lists");
            m.merged_config.add_color_space(cs)?;
        }
        Ok(())
    }

    /// The base's, with the overrides of the search paths, the environment and the inactive
    /// color spaces where they are set.
    ///
    /// Port of `ColorspacesMerger::handleBaseOnly` (SectionMerger.cpp:2975-3011 @ v2.5.2).
    fn handle_base_only(&mut self) -> Result<()> {
        // Process the overrides only since the merged config is initialized to
        // the base config.

        // Do search_path override.
        let search_paths = self.0.params.search_path();
        if !search_paths.is_empty() {
            // Use the override.
            self.0.merged_config.set_search_path(&search_paths);
        }

        // Do environment override.
        if self.0.params.num_environment_vars() > 0 {
            self.set_environment_overrides();
        }

        // Do inactive_colorspaces override.
        let inactive_cs = self.0.params.inactive_color_spaces();
        if !inactive_cs.is_empty() {
            self.0.merged_config.set_inactive_color_spaces(&inactive_cs);
        }

        // Nothing to do for display_colorspaces and colorspaces as the merged config
        // is initialized to the base config.

        // TODO: Avoid conflicts if roles are added from input config?
        Ok(())
    }

    /// The base's environment variables, search paths, inactive color spaces and color spaces
    /// without the input's: a variable the input has (whose value isn't empty, I-435), a search
    /// path found in the input's joined search path (docs/improvements.md, I-434), an inactive
    /// color space the input lists (ignoring case) and a color space of the input's name.
    ///
    /// Port of `ColorspacesMerger::handleRemove` (SectionMerger.cpp:3013-3072 @ v2.5.2).
    fn handle_remove(&mut self) -> Result<()> {
        let m = &mut self.0;
        let (base, input) = (m.base_config, m.input_config);
        // Handle environment.
        //     If an environment variable is used somewhere and got removed, validating the
        //     config will return an error.
        for i in 0..input.num_environment_vars() {
            let name = input.environment_var_name_by_index(i);
            let exists = !m.merged_config.environment_var_default(name).is_empty();
            if exists {
                m.merged_config.add_environment_var(name, None);
            }
        }

        // Handle search_path.
        m.merged_config.clear_search_paths();
        let input_search_path = input.search_path();
        for i in 0..base.num_search_paths() {
            let path = base.search_path_with_index(i);
            let found = find(&input_search_path, &path);
            if found.is_none() {
                m.merged_config.add_search_path(&path);
            }
        }

        // Handle inactive_colorspaces.
        let mut base_inactive_cs = StringVec::new();
        let mut input_inactive_cs = StringVec::new();
        let mut merged_inactive_cs = StringVec::new();
        split_active_list(base.inactive_color_spaces(), &mut base_inactive_cs);
        split_active_list(input.inactive_color_spaces(), &mut input_inactive_cs);
        trim_vec(&mut input_inactive_cs);
        for name in &base_inactive_cs {
            let trimmed_name = trim(name);
            if !trimmed_name.is_empty() && !contain(&input_inactive_cs, trimmed_name) {
                merged_inactive_cs.push(trimmed_name.to_vec());
            }
        }
        m.merged_config
            .set_inactive_color_spaces(join(&merged_inactive_cs, b','));

        // The family_separator never gets removed.

        // Handle display_colorspaces and colorspaces.
        //   This could obviously break any other part of the base config that references the
        //   removed color space, so it is up to the user to know what they are doing.

        let num_cs =
            input.num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::All);
        for i in 0..num_cs {
            let name = input.color_space_name_by_index_with(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All,
                i,
            );
            // Note: The remove does nothing if the color space is not present.
            m.merged_config.remove_color_space(name);
        }
        Ok(())
    }
}

/////////////////////////////////// NamedTransformsMerger ////////////////////////////////

/// Rewrites the merged config's inactive color spaces with only the trimmed entries that name a
/// color space or a named transform of it (by name, not alias, ignoring case), joined with
/// `", "`.
///
/// Port of `cleanUpInactiveList` (SectionMerger.cpp:2043-2070 @ v2.5.2).
fn clean_up_inactive_list(merge_config: &mut Config) {
    let mut original_list = StringVec::new();
    let mut valid_list = StringVec::new();
    split_active_list(merge_config.inactive_color_spaces(), &mut original_list);
    for item in &original_list {
        let name = trim(item);
        if let Some(existing_cs) = merge_config.color_space(name) {
            // Don't want aliases in the inactive list.
            if strcasecmp(existing_cs.name(), name).is_eq() {
                valid_list.push(name.to_vec());
            }
        } else if let Some(existing_nt) = merge_config.named_transform(name)
            && strcasecmp(existing_nt.name(), name).is_eq()
        {
            valid_list.push(name.to_vec());
        }
    }
    merge_config.set_inactive_color_spaces(join(&valid_list, b','));
}

/// Merges the named transforms, then cleans up the inactive color spaces.
///
/// Port of `NamedTransformsMerger` (SectionMerger.h:423-475, SectionMerger.cpp:3078-3728 @
/// v2.5.2). The header declares 13 more functions (`iterateOverColorSpaces`, the
/// `handleAddNtError...` ones, `handleErrorCodeWhenAddingInputFirst`, `handleErrorCodes`), which
/// OCIO 2.5.2 neither defines nor calls.
pub(crate) struct NamedTransformsMerger<'o, 'a>(SectionMerger<'o, 'a>);

impl<'o, 'a> NamedTransformsMerger<'o, 'a> {
    /// Port of `NamedTransformsMerger::NamedTransformsMerger` (SectionMerger.h:426-437 @
    /// v2.5.2).
    pub(crate) fn new(options: &'o mut MergeHandlerOptions<'a>) -> Result<Self> {
        let mut m = SectionMerger::new(options)?;
        m.strategy = section_strategy(m.params.named_transforms(), m.params);
        Ok(NamedTransformsMerger(m))
    }

    /// Prefixes `family` as the color spaces merger does: by the color spaces' strategy, not
    /// the named transforms' (docs/improvements.md, I-436).
    ///
    /// Port of `NamedTransformsMerger::updateFamily` (SectionMerger.cpp:3078-3121 @ v2.5.2).
    fn update_family(&self, family: &mut Vec<u8>, from_base: bool) {
        let m = &self.0;
        // Note that if a prefix is present, it is always added, even if the CS did not have a
        // family.

        let merged_sep = m.merged_config.family_separator();
        let mut updated_prefix = Vec::new();
        let strategy = m.params.colorspaces();
        if strategy == MergeStrategies::PreferInput {
            if from_base {
                // If the color space is from the base config, need to update its family
                // separator.
                if !family.is_empty() {
                    *family =
                        replace_separator(family, m.base_config.family_separator(), merged_sep);
                }
                // Note: The family prefix argument must always use the default slash separator.
                // TODO: Should do this just once in the initializer.
                updated_prefix =
                    replace_separator(&m.params.base_family_prefix(), b'/', merged_sep);
            } else {
                updated_prefix =
                    replace_separator(&m.params.input_family_prefix(), b'/', merged_sep);
            }
        } else if strategy == MergeStrategies::PreferBase {
            if from_base {
                // TODO: Should do this just once in the initializer.
                updated_prefix =
                    replace_separator(&m.params.base_family_prefix(), b'/', merged_sep);
            } else {
                // If the color space is from the input config, need to update its family
                // separator.
                if !family.is_empty() {
                    *family =
                        replace_separator(family, m.input_config.family_separator(), merged_sep);
                }
                updated_prefix =
                    replace_separator(&m.params.input_family_prefix(), b'/', merged_sep);
            }
        }
        // Append the prefix to the family.
        // Note that the prefix should end with a separator, if desired.  Not adding one here.
        updated_prefix.extend_from_slice(family);
        *family = updated_prefix;
    }

    /// Whether the named transform `nt` may be merged into `merge_config`: always when no
    /// color space (by name, alias or role) or named transform (by name or alias) has its name;
    /// never when a role or a color space does; never for a base one whose name is taken; else
    /// by the named transforms' strategy (`PreferInput` and `InputOnly` replace the named
    /// transform or take the alias of that name), each case with a message.
    ///
    /// Port of `NamedTransformsMerger::namedTransformMayBeMerged` (SectionMerger.cpp:3123-3261
    /// @ v2.5.2), whose null named transform can't happen with a reference.
    fn named_transform_may_be_merged(
        params: &ConfigMergingParameters,
        merge_config: &Config,
        nt: &NamedTransform,
        from_base: bool,
    ) -> Result<bool> {
        let name = nt.name();

        // This will compare the name against roles, color space names, and aliases.
        // (Note that if the role refers to a named transform, this will return null,
        //  but it's illegal for a role to point to a named transform.)
        let existing_cs = merge_config.color_space(name);

        let existing_nt = merge_config.named_transform(name);

        if existing_cs.is_none() && existing_nt.is_none() {
            // No name conflicts, go ahead and add it.
            return Ok(true);
        }

        // OK, something has this name, figure out what it is.

        // Does it have the same name as a role?
        if merge_config.has_role(name) {
            // Don't merge it if it would override a role.

            let os = [
                b"Named transform '".as_slice(),
                name,
                b"' was not merged as it's identical to a role name.",
            ]
            .concat();
            notify(&os, params.is_error_on_conflict())?;

            return Ok(false);
        }

        if let Some(existing_cs) = existing_cs {
            // Does it have the same name as another color space?
            let os = if strcasecmp(existing_cs.name(), name).is_eq() {
                // The name matches a color space name in the mergeConfig.
                // Don't merge it, color spaces always have precedence.
                [
                    b"Named transform '".as_slice(),
                    name,
                    b"' was not merged as there's a color space with that name.",
                ]
                .concat()
            } else {
                // The name conflicts with an alias of a color space.
                // Don't merge it, color spaces always have precedence.
                [
                    b"Named transform '".as_slice(),
                    name,
                    b"' was not merged as there's a color space alias with that name.",
                ]
                .concat()
            };
            notify(&os, params.is_error_on_conflict())?;

            return Ok(false);
        }

        if let Some(existing_nt) = existing_nt {
            if from_base {
                // Should not happen if the base config was legal.
                let os = [
                    b"Named transform '".as_slice(),
                    name,
                    b"' was not merged as there's more than one with that name in the base \
                      config.",
                ]
                .concat();
                notify(&os, params.is_error_on_conflict())?;

                return Ok(false);
            }

            let strategy = params.named_transforms();

            // At this point, only dealing with transforms from the input config.

            if strcasecmp(existing_nt.name(), name).is_eq() {
                // The name matches a named transform name in the mergeConfig.
                // Whether to allow the merge is based on the merge strategy.

                if prefers_input(strategy) {
                    // Allow the merger.

                    let os = [
                        b"Named transform '".as_slice(),
                        name,
                        b"' will replace a named transform in the base config.",
                    ]
                    .concat();
                    notify(&os, params.is_error_on_conflict())?;

                    return Ok(true);
                } else {
                    // Don't merge it if it would replace a named transform from the base
                    // config.

                    let os = [
                        b"Named transform '".as_slice(),
                        name,
                        b"' was not merged as it's already present in the base config.",
                    ]
                    .concat();
                    notify(&os, params.is_error_on_conflict())?;

                    return Ok(false);
                }
            } else {
                // The name conflicts with an alias of another named transform.
                // Whether to allow the merge is based on the merge strategy.

                if prefers_input(strategy) {
                    // Allow the merger.

                    let os = [
                        b"The name of merged named transform '".as_slice(),
                        name,
                        b"'",
                        b" has a conflict with an alias in named transform '",
                        existing_nt.name(),
                        b"'.",
                    ]
                    .concat();
                    notify(&os, params.is_error_on_conflict())?;

                    return Ok(true);
                } else {
                    // Don't merge it if it would replace an alias from the base config.

                    let os = [
                        b"Named transform '".as_slice(),
                        name,
                        b"' was not merged as it conflicts with an alias ",
                        b"in named transform '",
                        existing_nt.name(),
                        b"'.",
                    ]
                    .concat();
                    notify(&os, params.is_error_on_conflict())?;

                    return Ok(false);
                }
            }
        }
        Ok(false)
    }

    /// Adds `e_nt` to `merge_config`, replacing the named transform of its name, taking its
    /// name from the alias of another named transform, and resolving the conflicts of its
    /// aliases with color spaces, aliases and roles (the alias is removed) and with named
    /// transforms (by the named transforms' strategy, `PreferBase` for a base one), each with a
    /// message; records the name of an input one in `added_input_named_transforms`.
    ///
    /// Port of `NamedTransformsMerger::mergeNamedTransform` (SectionMerger.cpp:3263-3468 @
    /// v2.5.2).
    fn merge_named_transform(
        params: &ConfigMergingParameters,
        merge_config: &mut Config,
        e_nt: &mut NamedTransform,
        from_base: bool,
        added_input_named_transforms: &mut Vec<Vec<u8>>,
    ) -> Result<()> {
        // NB: This routine assumes all NamedTransforms have been removed from the mergeConfig.
        // Not trying to handle name conflicts with NamedTransforms, color spaces have
        // precedence.

        // Check if mergeConfig already has a color space with the same name as the input CS.

        let name = e_nt.name().to_vec();

        if merge_config.named_transform(&name).is_some() {
            // If there is a named transform with this name in the existing config,
            // remove it (and any aliases it may contain). This is the case when
            // the strategy calls for replacing an existing transform.
            //
            // If the eNT name matched an alias rather than a named transform name,
            // this does nothing (and the alias is handled below).
            //
            // The notification is handled in namedTransformMayBeMerged to avoid having to
            // determine again whether the conflict is with the name or alias of originalNT.

            merge_config.remove_named_transform(&name);
        }

        // Handle conflicts of the eNT name with aliases of other named transforms.
        // NB: Would not be here if there is a name conflict with anything other than
        // named transforms since the decision would have been not to merge it.

        if let Some(existing_nt) = merge_config.named_transform(&name) {
            // Get the name of the named transform that contains the alias.

            // Verify that the name is actually an alias rather than some other conflict.
            // (Should never happen.)
            if !existing_nt.has_alias(&name) {
                return Err(Exception::new(
                    [
                        b"Problem merging named transform: '".as_slice(),
                        &name,
                        b"'.",
                    ]
                    .concat(),
                ));
            }

            // Remove the alias from that existing named transform.
            //    Note that this conflict was detected and allowed in namedTransformMayBeMerged
            //    based on the merge strategy, so the decision has already been made to remove
            //    this alias from a named transform in the base config.

            let mut e_existing_nt = existing_nt.clone();
            e_existing_nt.remove_alias(&name);
            // Edit the named transform in the copy of the merged config.
            merge_config.add_named_transform(&e_existing_nt)?;

            // The notification is handled in namedTransformMayBeMerged to avoid having to
            // determine again whether the conflict is with the name or alias of originalNT.
        }

        let mut strategy = params.named_transforms();
        if from_base {
            strategy = MergeStrategies::PreferBase;
        }

        //
        // Handle conflicts of the eNT aliases with other color spaces, named transforms, and
        // etc.
        //

        // First initialize the list of names, since eNT is being edited within the loop.
        let alias_names: Vec<Vec<u8>> = (0..e_nt.num_aliases())
            .map(|i| e_nt.alias(i).to_vec())
            .collect();

        for alias_name in &alias_names {
            // Conflicts with color spaces or roles.  (Always remove this alias.)

            // (One stream for the whole alias: a message of a named transform conflict
            // follows the one of a color space conflict, as upstream's `os`.)
            let mut os: Vec<u8> = Vec::new();

            let source: &[u8] = if from_base { b"Base" } else { b"Input" };

            if let Some(conflicting_cs) = merge_config.color_space(alias_name) {
                if strcasecmp(conflicting_cs.name(), alias_name).is_eq() {
                    // The alias conflicts with the name of the conflicting color space.

                    os.extend_from_slice(
                        &[
                            b"Merged ".as_slice(),
                            source,
                            b" named transform '",
                            &name,
                            b"'",
                            b" has an alias '",
                            alias_name,
                            b"' that conflicts with color space '",
                            conflicting_cs.name(),
                            b"'.",
                        ]
                        .concat(),
                    );

                    // Remove the alias from the named transform.
                    e_nt.remove_alias(alias_name);
                } else if conflicting_cs.has_alias(alias_name) {
                    // The alias conflicts with an alias of the conflicting color space.

                    os.extend_from_slice(
                        &[
                            b"Merged ".as_slice(),
                            source,
                            b" named transform '",
                            &name,
                            b"'",
                            b" has a conflict with alias '",
                            alias_name,
                            b"' in color space '",
                            conflicting_cs.name(),
                            b"'.",
                        ]
                        .concat(),
                    );

                    // Remove the alias from the named transform.
                    e_nt.remove_alias(alias_name);
                } else if merge_config.has_role(alias_name) {
                    os.extend_from_slice(
                        &[
                            b"Merged ".as_slice(),
                            source,
                            b" named transform '",
                            &name,
                            b"'",
                            b" has an alias '",
                            alias_name,
                            b"' that conflicts with a role.",
                        ]
                        .concat(),
                    );

                    // Remove the alias from the named transform.
                    e_nt.remove_alias(alias_name);
                } else {
                    // (Should never happen.)
                    return Err(Exception::new(
                        [
                            b"Problem merging named transform: '".as_slice(),
                            &name,
                            b"' due to its aliases.",
                        ]
                        .concat(),
                    ));
                }

                // Throw if requested, otherwise log a warning.
                notify(&os, params.is_error_on_conflict())?;
            }

            // Conflicts of the alias with other named transforms.

            if let Some(conflicting_nt) = merge_config.named_transform(alias_name) {
                let conflicting_nt = conflicting_nt.clone();
                if strcasecmp(conflicting_nt.name(), alias_name).is_eq() {
                    // The alias conflicts with the name of an existing named transform.

                    os.extend_from_slice(
                        &[
                            b"Merged ".as_slice(),
                            source,
                            b" named transform '",
                            &name,
                            b"'",
                            b" has an alias '",
                            alias_name,
                            b"' that conflicts with named transform '",
                            conflicting_nt.name(),
                            b"'.",
                        ]
                        .concat(),
                    );

                    if prefers_input(strategy) {
                        // Remove that base named transform.
                        merge_config.remove_named_transform(conflicting_nt.name());
                    } else {
                        // Remove the alias from the input named transform.
                        e_nt.remove_alias(alias_name);
                    }
                } else if conflicting_nt.has_alias(alias_name) {
                    // The alias conflicts with an alias of the conflicting named transform.

                    os.extend_from_slice(
                        &[
                            b"Merged ".as_slice(),
                            source,
                            b" named transform '",
                            &name,
                            b"'",
                            b" has a conflict with alias '",
                            alias_name,
                            b"' in named transform '",
                            conflicting_nt.name(),
                            b"'.",
                        ]
                        .concat(),
                    );

                    if prefers_input(strategy) {
                        // Remove the alias from that base named transform.
                        let mut e_conflicting_nt = conflicting_nt.clone();
                        e_conflicting_nt.remove_alias(alias_name);

                        // Replace the colorspace in the copy of the merged config.
                        merge_config.add_named_transform(&e_conflicting_nt)?;
                    } else {
                        // Remove the alias from the input named transform.
                        e_nt.remove_alias(alias_name);
                    }
                } else {
                    // (Should never happen.)
                    return Err(Exception::new(
                        [
                            b"Problem merging named transform: '".as_slice(),
                            &name,
                            b"' due to its aliases.",
                        ]
                        .concat(),
                    ));
                }

                notify(&os, params.is_error_on_conflict())?;
            }
        }

        // Add the named transform. This will throw if a problem is found.
        // (But all name conflicts should have been handled already.)
        merge_config.add_named_transform(e_nt)?;

        // Keep a record that this input NT was added to allow reordering later.
        if !from_base {
            added_input_named_transforms.push(name);
        }
        Ok(())
    }

    /// Merges the named transform `name` of `cfg` (the base's with `from_base`) into
    /// `merge_config`, when it may be merged.
    ///
    /// The body of the loops of `NamedTransformsMerger::addNamedTransforms`, `handleInputOnly`,
    /// `handleBaseOnly` and `handleRemove` (SectionMerger.cpp:3470-3728 @ v2.5.2).
    fn merge_from(
        params: &ConfigMergingParameters,
        merge_config: &mut Config,
        cfg: &Config,
        name: &[u8],
        from_base: bool,
        added_input_named_transforms: &mut Vec<Vec<u8>>,
    ) -> Result<()> {
        let Some(nt) = cfg.named_transform(name) else {
            return Ok(());
        };

        let mut e_nt = nt.clone();

        if Self::named_transform_may_be_merged(params, merge_config, &e_nt, from_base)? {
            // NB: This may make changes to existing color spaces in mergeConfig
            // to resolve name conflicts.
            Self::merge_named_transform(
                params,
                merge_config,
                &mut e_nt,
                from_base,
                added_input_named_transforms,
            )?;
        }
        Ok(())
    }

    /// The named transforms of both configs (the base's first, then the input's), merged into a
    /// copy of the merged config, then written in order (the input's first with the
    /// input-first option) with their family prefixes; the inactive color spaces cleaned up.
    ///
    /// Port of `NamedTransformsMerger::addNamedTransforms` (SectionMerger.cpp:3470-3624 @
    /// v2.5.2).
    fn add_named_transforms(&mut self) -> Result<()> {
        // Delete all the NamedTransforms, color spaces take precedence, so don't want them
        // interfering with merges by causing name conflicts.

        // NB: This is only intended to be called for the prefer_input and prefer_base
        // strategies.

        // Need to add even the base to ensure there are not conflicts with the merged color
        // spaces.

        self.0.merged_config.clear_named_transforms();

        // Make a temp copy to merge the input color spaces into (will reorder them later).
        let mut merge_config = self.0.merged_config.clone();

        let mut added_input_named_transforms: Vec<Vec<u8>> = Vec::new();

        // Loop over all active and inactive color spaces of all reference types in the input
        // config. Merge them into the temp config (which already contains the base color
        // spaces).

        // Merge from Base config.

        let base = self.0.base_config;
        for i in 0..base.num_named_transforms_with(NamedTransformVisibility::All) {
            let name = base.named_transform_name_by_index_with(NamedTransformVisibility::All, i);

            let from_base = true;
            Self::merge_from(
                self.0.params,
                &mut merge_config,
                base,
                name,
                from_base,
                &mut added_input_named_transforms,
            )?;
        }

        // Merge from Input config.

        let input = self.0.input_config;
        for i in 0..input.num_named_transforms_with(NamedTransformVisibility::All) {
            let name = input.named_transform_name_by_index_with(NamedTransformVisibility::All, i);

            // Doing this against the mergedConfig rather than the base config so that the most
            // recent state of any aliases that get added or color spaces that are removed are
            // considered by the duplicate consolidation process.

            // TODO: Handle duplicate named transforms.

            let from_base = false;
            Self::merge_from(
                self.0.params,
                &mut merge_config,
                input,
                name,
                from_base,
                &mut added_input_named_transforms,
            )?;
        }

        self.0.merged_config.clear_named_transforms();

        // Add the named transforms to the real merged config.

        if self.0.params.is_input_first() {
            // Add color spaces from the input config.
            for name in &added_input_named_transforms {
                if let Some(input_nt) = merge_config.named_transform(name) {
                    let mut e_input_nt = input_nt.clone();

                    // Add family prefix.
                    let from_base = false;
                    let mut family = e_input_nt.family().to_vec();
                    self.update_family(&mut family, from_base);
                    e_input_nt.set_family(&family);

                    self.0.merged_config.add_named_transform(&e_input_nt)?;

                    merge_config.remove_named_transform(name);
                }
            }

            // Add color spaces from the base config.
            for i in 0..merge_config.num_named_transforms_with(NamedTransformVisibility::All) {
                // Note that during the merge process, some of the color spaces from the base
                // config may be replaced if their aliases are edited. This should not change
                // their order in the config, but may want to account for this, if that ever
                // changes and they get moved to the end.
                let name = merge_config
                    .named_transform_name_by_index_with(NamedTransformVisibility::All, i);

                if let Some(base_nt) = merge_config.named_transform(name) {
                    let mut e_base_nt = base_nt.clone();

                    // Add family prefix.
                    let from_base = true;
                    let mut family = e_base_nt.family().to_vec();
                    self.update_family(&mut family, from_base);
                    e_base_nt.set_family(&family);

                    self.0.merged_config.add_named_transform(&e_base_nt)?;
                }
            }
        } else {
            // The color spaces should already be in the correct order.
            // Copy them into the real merged config and add family prefix.
            for i in 0..merge_config.num_named_transforms_with(NamedTransformVisibility::All) {
                let name = merge_config
                    .named_transform_name_by_index_with(NamedTransformVisibility::All, i);

                if let Some(nt) = merge_config.named_transform(name) {
                    let mut e_nt = nt.clone();

                    // Add family prefix.
                    let from_base = !added_input_named_transforms.iter().any(|n| n == name);
                    let mut family = e_nt.family().to_vec();
                    self.update_family(&mut family, from_base);
                    e_nt.set_family(&family);

                    self.0.merged_config.add_named_transform(&e_nt)?;
                }
            }
        }

        // Ensure the inactive_colorspaces doesn't contain anything that was removed.
        // TODO: Should move this to a higher level?
        clean_up_inactive_list(self.0.merged_config);

        // TODO: What if the environment contains a color space that was removed?
        Ok(())
    }

    /// The named transforms of `cfg` (the base's with `from_base`), each when `keep` holds for
    /// its name, merged into the merged config itself after removing its named transforms; the
    /// inactive color spaces cleaned up.
    ///
    /// The body of `NamedTransformsMerger::handleInputOnly`, `handleBaseOnly` and
    /// `handleRemove` (SectionMerger.cpp:3636-3728 @ v2.5.2).
    fn merge_into_merged(&mut self, from_base: bool, keep: impl Fn(&[u8]) -> bool) -> Result<()> {
        let m = &mut self.0;
        m.merged_config.clear_named_transforms();

        let cfg = if from_base {
            m.base_config
        } else {
            m.input_config
        };
        for i in 0..cfg.num_named_transforms_with(NamedTransformVisibility::All) {
            let name = cfg.named_transform_name_by_index_with(NamedTransformVisibility::All, i);
            if keep(name) {
                let mut added_input_named_transforms = Vec::new();
                Self::merge_from(
                    m.params,
                    m.merged_config,
                    cfg,
                    name,
                    from_base,
                    &mut added_input_named_transforms,
                )?;
            }
        }

        // Ensure the inactive_colorspaces doesn't contain anything that was removed.
        clean_up_inactive_list(m.merged_config);
        Ok(())
    }
}

impl<'o, 'a> Section<'o, 'a> for NamedTransformsMerger<'o, 'a> {
    fn state(&self) -> &SectionMerger<'o, 'a> {
        &self.0
    }

    /// Port of `NamedTransformsMerger::getName` (SectionMerger.h:440 @ v2.5.2).
    fn name(&self) -> &'static str {
        "Named Transforms"
    }

    /// Port of `NamedTransformsMerger::handlePreferInput` (SectionMerger.cpp:3626-3629 @
    /// v2.5.2).
    fn handle_prefer_input(&mut self) -> Result<()> {
        self.add_named_transforms()
    }

    /// Port of `NamedTransformsMerger::handlePreferBase` (SectionMerger.cpp:3631-3634 @
    /// v2.5.2).
    fn handle_prefer_base(&mut self) -> Result<()> {
        self.add_named_transforms()
    }

    /// The input's named transforms that may be merged.
    ///
    /// Port of `NamedTransformsMerger::handleInputOnly` (SectionMerger.cpp:3636-3665 @ v2.5.2).
    fn handle_input_only(&mut self) -> Result<()> {
        // Add the NamedTransforms from the input config.
        self.merge_into_merged(false, |_| true)
    }

    /// The base's named transforms that may be merged.
    ///
    /// Port of `NamedTransformsMerger::handleBaseOnly` (SectionMerger.cpp:3667-3696 @ v2.5.2).
    fn handle_base_only(&mut self) -> Result<()> {
        // Add the NamedTransforms from the base config.
        self.merge_into_merged(true, |_| true)
    }

    /// The base's named transforms that the input doesn't have (by name or alias).
    ///
    /// Port of `NamedTransformsMerger::handleRemove` (SectionMerger.cpp:3698-3728 @ v2.5.2).
    fn handle_remove(&mut self) -> Result<()> {
        let input = self.0.input_config;
        // Add the NamedTransforms from the base config if they do not exist in the INPUT config.
        self.merge_into_merged(true, |name| input.named_transform(name).is_none())
    }
}

#[cfg(test)]
#[path = "section_merger_oracle_tests.rs"]
mod oracle_tests;
