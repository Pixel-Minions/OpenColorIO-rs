// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The section mergers of a config merge: a port of
//! `src/OpenColorIO/apphelpers/mergeconfigs/SectionMerger.h` and `SectionMerger.cpp` @ v2.5.2.
//!
//! A merge copies the base config, then lets one merger per section of the config change that
//! copy, each by its strategy (`ConfigMerger::mergeConfigs`, `ConfigMergingHelpers::MergeConfigs`,
//! MergeConfigsHelpers.cpp:681-760): [`GeneralMerger`] and [`RolesMerger`] (9.2d), then the file
//! rules, the displays and views, the view transforms, the looks, the color spaces and the named
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

use ocio_ops::exception::{Exception, Result};
use ocio_ops::logging::log_warning;
use ocio_ops::platform::strcasecmp;

use crate::apphelpers::mergeconfigs::merge_configs_helpers::{
    ConfigMergingParameters, MergeStrategies,
};
use crate::config::Config;

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

#[cfg(test)]
#[path = "section_merger_oracle_tests.rs"]
mod oracle_tests;
