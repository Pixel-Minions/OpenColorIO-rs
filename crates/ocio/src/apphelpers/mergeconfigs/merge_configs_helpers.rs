// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The parameters of a config merge (`ConfigMergingParameters`) and the merger that runs a
//! series of merges (`ConfigMerger`): a port of
//! `src/OpenColorIO/apphelpers/mergeconfigs/MergeConfigsHelpers.h` and
//! `MergeConfigsHelpers.cpp` @ v2.5.2, whose public API is
//! `include/OpenColorIO/OpenColorAppHelpers.h:536-853` (owner decision P9-2).

use std::sync::{Arc, Mutex, MutexGuard};

use ocio_ops::exception::{Exception, Result};
use ocio_ops::platform::{create_input_file_stream, strcasecmp};
use ocio_ops::utils::pystring::os_path;
use ocio_ops::utils::string_utils::{c_str, right_trim_char, split, trim};

use crate::apphelpers::mergeconfigs::ociom_yaml::{self, WriteError, enum_to_strategy_string};
use crate::config::Config;
use crate::yaml_cpp::stream::IStream;

/// How a section of the config is merged.
///
/// Port of `ConfigMergingParameters::MergeStrategies`
/// (include/OpenColorIO/OpenColorAppHelpers.h:546-561 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MergeStrategies {
    /// `STRATEGY_PREFER_INPUT`: combine the elements of the base and input configs, the
    /// input's taking priority.
    PreferInput = 0,
    /// `STRATEGY_PREFER_BASE`: combine the elements of the base and input configs, the base's
    /// taking priority.
    PreferBase,
    /// `STRATEGY_INPUT_ONLY`: only the input's elements for that section of the config.
    InputOnly,
    /// `STRATEGY_BASE_ONLY`: only the base's elements for that section of the config.
    BaseOnly,
    /// `STRATEGY_REMOVE`: the input config's elements are removed from the base config (by
    /// name, even when their contents differ).
    Remove,
    /// `STRATEGY_UNSPECIFIED`: no strategy set yet.
    Unspecified,
}

/// The state of a [`ConfigMergingParameters`].
///
/// Port of `ConfigMergingParameters::Impl` (MergeConfigsHelpers.h:21-127 @ v2.5.2).
#[derive(Debug)]
struct ParamsImpl {
    /// `m_baseConfig`: the name or path of the base config.
    base_config: Vec<u8>,
    /// `m_inputConfig`: the name or path of the input config.
    input_config: Vec<u8>,
    /// `m_outputName`: the name of the merged config (a later merge's base or input may name
    /// it).
    output_name: Vec<u8>,
    /// `m_name`: the override of the merged config's name.
    name: Vec<u8>,
    /// `m_description`: the override of the merged config's description.
    description: Vec<u8>,
    /// `m_overrideCfg`: a config that holds the overrides of the search path, the environment,
    /// the active displays and views and the inactive color spaces. Upstream's copies share it
    /// (a `ConfigRcPtr` copied by `Impl::operator=`; docs/improvements.md, I-386).
    override_cfg: Arc<Mutex<Config>>,
    /// `m_inputFamilyPrefix`.
    input_family_prefix: Vec<u8>,
    /// `m_baseFamilyPrefix`.
    base_family_prefix: Vec<u8>,
    /// `m_inputFirst`.
    input_first: bool,
    /// `m_errorOnConflict`.
    error_on_conflict: bool,
    /// `m_avoidDuplicates`.
    avoid_duplicates: bool,
    /// `m_adjustInputReferenceSpace`.
    adjust_input_reference_space: bool,
    /// `m_defaultStrategy`: the strategy of each section whose own is unspecified.
    default_strategy: MergeStrategies,
    /// `m_roles`.
    roles: MergeStrategies,
    /// `m_fileRules`.
    file_rules: MergeStrategies,
    /// `m_displayViews`: shared views, displays, viewing rules, the virtual display, the
    /// active displays and views.
    display_views: MergeStrategies,
    /// `m_viewTransforms`: the view transforms and the default view transform.
    view_transforms: MergeStrategies,
    /// `m_looks`.
    looks: MergeStrategies,
    /// `m_colorspaces`: the color spaces, the environment, the search path, the family
    /// separator and the inactive color spaces.
    colorspaces: MergeStrategies,
    /// `m_namedTransforms`.
    named_transforms: MergeStrategies,
}

impl ParamsImpl {
    /// The defaults: the output name `merged`, the input first, no error on conflict, avoiding
    /// duplicates, adjusting the input's reference space, the default strategy
    /// [`MergeStrategies::PreferInput`] and each section's unspecified; an override config
    /// made by `Config::Create` without its environment variables. Fails as [`Config::new`]
    /// does (an active display or view list of the environment with an unclosed quote).
    ///
    /// Port of `ConfigMergingParameters::Impl::Impl` (MergeConfigsHelpers.h:62-90 @ v2.5.2).
    fn new() -> Result<ParamsImpl> {
        let mut override_cfg = Config::new()?;
        override_cfg.clear_environment_vars();
        Ok(ParamsImpl {
            base_config: Vec::new(),
            input_config: Vec::new(),
            output_name: b"merged".to_vec(),

            // Overrides
            name: Vec::new(),
            description: Vec::new(),
            override_cfg: Arc::new(Mutex::new(override_cfg)),

            // Options
            input_family_prefix: Vec::new(),
            base_family_prefix: Vec::new(),
            input_first: true,
            error_on_conflict: false,
            avoid_duplicates: true,
            adjust_input_reference_space: true,

            default_strategy: MergeStrategies::PreferInput,
            roles: MergeStrategies::Unspecified,
            file_rules: MergeStrategies::Unspecified,
            display_views: MergeStrategies::Unspecified,
            view_transforms: MergeStrategies::Unspecified,
            looks: MergeStrategies::Unspecified,
            colorspaces: MergeStrategies::Unspecified,
            named_transforms: MergeStrategies::Unspecified,
        })
    }

    /// Copies `rhs` into `self`: every field, and the override config's pointer, so both share
    /// one override config (docs/improvements.md, I-386).
    ///
    /// Port of `ConfigMergingParameters::Impl::operator=` (MergeConfigsHelpers.h:95-126 @
    /// v2.5.2).
    fn assign(&mut self, rhs: &ParamsImpl) {
        self.base_config.clone_from(&rhs.base_config);
        self.input_config.clone_from(&rhs.input_config);
        self.output_name.clone_from(&rhs.output_name);

        // Overrides
        self.name.clone_from(&rhs.name);
        self.description.clone_from(&rhs.description);
        self.override_cfg = Arc::clone(&rhs.override_cfg);

        // Options
        self.input_family_prefix
            .clone_from(&rhs.input_family_prefix);
        self.base_family_prefix.clone_from(&rhs.base_family_prefix);
        self.input_first = rhs.input_first;
        self.error_on_conflict = rhs.error_on_conflict;
        self.avoid_duplicates = rhs.avoid_duplicates;
        self.adjust_input_reference_space = rhs.adjust_input_reference_space;

        self.default_strategy = rhs.default_strategy;
        self.roles = rhs.roles;
        self.file_rules = rhs.file_rules;
        self.display_views = rhs.display_views;
        self.view_transforms = rhs.view_transforms;
        self.looks = rhs.looks;
        self.colorspaces = rhs.colorspaces;
        self.named_transforms = rhs.named_transforms;
    }
}

/// The options of one merge: the names of its base and input configs and of its output, its
/// options, the overrides of the merged config, and a strategy per section of the config. In
/// an OCIOM file, one of the merges of its `merge` section.
///
/// A shared handle, as upstream's `ConfigMergingParametersRcPtr` (owner decision P9-2): a
/// clone is the same object (a merger's parameters and the handle added to it change
/// together), and the setters take `&self`. [`create_editable_copy`] makes
/// another object, which still shares the overrides of the search path, the environment, the
/// active displays and views and the inactive color spaces with this one, as upstream's
/// (docs/improvements.md, I-386).
///
/// Strings end at their first NUL, as the `const char *` upstream takes; getters return owned
/// bytes.
///
/// Port of `ConfigMergingParameters` (include/OpenColorIO/OpenColorAppHelpers.h:542-709,
/// src/OpenColorIO/apphelpers/mergeconfigs/MergeConfigsHelpers.cpp:25-335 @ v2.5.2).
///
/// [`create_editable_copy`]: ConfigMergingParameters::create_editable_copy
#[derive(Debug, Clone)]
pub struct ConfigMergingParameters {
    inner: Arc<Mutex<ParamsImpl>>,
}

/// Where a section's own strategy is unspecified, the default strategy.
fn or_default(strategy: MergeStrategies, default: MergeStrategies) -> MergeStrategies {
    if strategy == MergeStrategies::Unspecified {
        return default;
    }
    strategy
}

impl ConfigMergingParameters {
    /// New parameters with the defaults (see `ParamsImpl::new`). Fails as [`Config::new`]
    /// does, for the override config.
    ///
    /// Port of `ConfigMergingParameters::Create` (MergeConfigsHelpers.cpp:25-39 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> Result<ConfigMergingParameters> {
        Ok(ConfigMergingParameters {
            inner: Arc::new(Mutex::new(ParamsImpl::new()?)),
        })
    }

    /// Another object with the same parameters, which shares this one's override config
    /// (docs/improvements.md, I-386). Fails as [`ConfigMergingParameters::new`] does.
    ///
    /// Port of `ConfigMergingParameters::createEditableCopy` (MergeConfigsHelpers.cpp:46-51 @
    /// v2.5.2).
    #[doc(alias = "createEditableCopy")]
    pub fn create_editable_copy(&self) -> Result<ConfigMergingParameters> {
        let params = ConfigMergingParameters::new()?;
        params.lock().assign(&self.lock());
        Ok(params)
    }

    /// Whether `a` and `b` are the same object (upstream's pointer equality).
    pub fn ptr_eq(a: &ConfigMergingParameters, b: &ConfigMergingParameters) -> bool {
        Arc::ptr_eq(&a.inner, &b.inner)
    }

    fn lock(&self) -> MutexGuard<'_, ParamsImpl> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The override config, shared with the copies.
    fn override_cfg(&self) -> Arc<Mutex<Config>> {
        Arc::clone(&self.lock().override_cfg)
    }

    /// Runs `f` on the override config.
    fn with_override_cfg<T>(&self, f: impl FnOnce(&mut Config) -> T) -> T {
        let cfg = self.override_cfg();
        let mut cfg = cfg.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut cfg)
    }

    /// Sets the name or path of the base config, which the merger looks for along its search
    /// paths.
    ///
    /// Port of `ConfigMergingParameters::setBaseConfigName` (MergeConfigsHelpers.cpp:53-56 @
    /// v2.5.2).
    #[doc(alias = "setBaseConfigName")]
    pub fn set_base_config_name(&self, base_config: impl AsRef<[u8]>) {
        self.lock().base_config = c_str(base_config.as_ref()).to_vec();
    }

    /// Port of `ConfigMergingParameters::getBaseConfigName` (MergeConfigsHelpers.cpp:58-61 @
    /// v2.5.2).
    #[doc(alias = "getBaseConfigName")]
    pub fn base_config_name(&self) -> Vec<u8> {
        self.lock().base_config.clone()
    }

    /// Sets the name or path of the input config, which the merger looks for along its search
    /// paths.
    ///
    /// Port of `ConfigMergingParameters::setInputConfigName` (MergeConfigsHelpers.cpp:63-66 @
    /// v2.5.2).
    #[doc(alias = "setInputConfigName")]
    pub fn set_input_config_name(&self, input_config: impl AsRef<[u8]>) {
        self.lock().input_config = c_str(input_config.as_ref()).to_vec();
    }

    /// Port of `ConfigMergingParameters::getInputConfigName` (MergeConfigsHelpers.cpp:68-71 @
    /// v2.5.2).
    #[doc(alias = "getInputConfigName")]
    pub fn input_config_name(&self) -> Vec<u8> {
        self.lock().input_config.clone()
    }

    /// Overrides the name of the merged config.
    ///
    /// Port of `ConfigMergingParameters::setName` (MergeConfigsHelpers.cpp:73-76 @ v2.5.2).
    #[doc(alias = "setName")]
    pub fn set_name(&self, merged_config_name: impl AsRef<[u8]>) {
        self.lock().name = c_str(merged_config_name.as_ref()).to_vec();
    }

    /// Port of `ConfigMergingParameters::getName` (MergeConfigsHelpers.cpp:78-81 @ v2.5.2).
    #[doc(alias = "getName")]
    pub fn name(&self) -> Vec<u8> {
        self.lock().name.clone()
    }

    /// Port of `ConfigMergingParameters::getDescription` (MergeConfigsHelpers.cpp:83-86 @
    /// v2.5.2).
    #[doc(alias = "getDescription")]
    pub fn description(&self) -> Vec<u8> {
        self.lock().description.clone()
    }

    /// Overrides the description of the merged config.
    ///
    /// Port of `ConfigMergingParameters::setDescription` (MergeConfigsHelpers.cpp:88-91 @
    /// v2.5.2).
    #[doc(alias = "setDescription")]
    pub fn set_description(&self, merged_config_desc: impl AsRef<[u8]>) {
        self.lock().description = c_str(merged_config_desc.as_ref()).to_vec();
    }

    /// Overrides a context variable of the merged config: the override config's
    /// [`Config::add_environment_var`] (`None`, upstream's null pointer, removes it; an empty
    /// name is ignored).
    ///
    /// Port of `ConfigMergingParameters::addEnvironmentVar` (MergeConfigsHelpers.cpp:93-96 @
    /// v2.5.2).
    #[doc(alias = "addEnvironmentVar")]
    pub fn add_environment_var(&self, name: impl AsRef<[u8]>, default_value: Option<&[u8]>) {
        self.with_override_cfg(|cfg| cfg.add_environment_var(name, default_value));
    }

    /// Port of `ConfigMergingParameters::getNumEnvironmentVars` (MergeConfigsHelpers.cpp:98-101
    /// @ v2.5.2).
    #[doc(alias = "getNumEnvironmentVars")]
    pub fn num_environment_vars(&self) -> i32 {
        self.with_override_cfg(|cfg| cfg.num_environment_vars())
    }

    /// The name of the context variable override at `index`, in byte order; `""` outside the
    /// list.
    ///
    /// Port of `ConfigMergingParameters::getEnvironmentVar` (MergeConfigsHelpers.cpp:103-106 @
    /// v2.5.2).
    #[doc(alias = "getEnvironmentVar")]
    pub fn environment_var(&self, index: i32) -> Vec<u8> {
        self.with_override_cfg(|cfg| cfg.environment_var_name_by_index(index).to_vec())
    }

    /// The value of the context variable override at `index`; `""` outside the list.
    ///
    /// Port of `ConfigMergingParameters::getEnvironmentVarValue`
    /// (MergeConfigsHelpers.cpp:108-112 @ v2.5.2).
    #[doc(alias = "getEnvironmentVarValue")]
    pub fn environment_var_value(&self, index: i32) -> Vec<u8> {
        self.with_override_cfg(|cfg| {
            let name = cfg.environment_var_name_by_index(index).to_vec();
            cfg.environment_var_default(name).to_vec()
        })
    }

    /// Overrides the search path of the merged config: the override config's
    /// [`Config::set_search_path`] (split on `:`).
    ///
    /// Port of `ConfigMergingParameters::setSearchPath` (MergeConfigsHelpers.cpp:114-117 @
    /// v2.5.2).
    #[doc(alias = "setSearchPath")]
    pub fn set_search_path(&self, path: impl AsRef<[u8]>) {
        self.with_override_cfg(|cfg| cfg.set_search_path(path));
    }

    /// Adds a path to the override of the search path (an empty one is ignored).
    ///
    /// Port of `ConfigMergingParameters::addSearchPath` (MergeConfigsHelpers.cpp:119-122 @
    /// v2.5.2).
    #[doc(alias = "addSearchPath")]
    pub fn add_search_path(&self, path: impl AsRef<[u8]>) {
        self.with_override_cfg(|cfg| cfg.add_search_path(path));
    }

    /// The override of the search path, its paths joined with `:`.
    ///
    /// Port of `ConfigMergingParameters::getSearchPath` (MergeConfigsHelpers.cpp:124-127 @
    /// v2.5.2).
    #[doc(alias = "getSearchPath")]
    pub fn search_path(&self) -> Vec<u8> {
        self.with_override_cfg(|cfg| cfg.search_path())
    }

    /// Overrides the active displays of the merged config: the override config's
    /// [`Config::set_active_displays`], which fails for a list that opens a quote it doesn't
    /// close.
    ///
    /// Port of `ConfigMergingParameters::setActiveDisplays` (MergeConfigsHelpers.cpp:129-132 @
    /// v2.5.2).
    #[doc(alias = "setActiveDisplays")]
    pub fn set_active_displays(&self, displays: impl AsRef<[u8]>) -> Result<()> {
        self.with_override_cfg(|cfg| cfg.set_active_displays(c_str(displays.as_ref())))
    }

    /// The override of the active displays, joined with `", "`.
    ///
    /// Port of `ConfigMergingParameters::getActiveDisplays` (MergeConfigsHelpers.cpp:134-137 @
    /// v2.5.2).
    #[doc(alias = "getActiveDisplays")]
    pub fn active_displays(&self) -> Vec<u8> {
        self.with_override_cfg(|cfg| cfg.active_displays())
    }

    /// Overrides the active views of the merged config: the override config's
    /// [`Config::set_active_views`], which fails for a list that opens a quote it doesn't
    /// close.
    ///
    /// Port of `ConfigMergingParameters::setActiveViews` (MergeConfigsHelpers.cpp:139-142 @
    /// v2.5.2).
    #[doc(alias = "setActiveViews")]
    pub fn set_active_views(&self, views: impl AsRef<[u8]>) -> Result<()> {
        self.with_override_cfg(|cfg| cfg.set_active_views(c_str(views.as_ref())))
    }

    /// The override of the active views, joined with `", "`.
    ///
    /// Port of `ConfigMergingParameters::getActiveViews` (MergeConfigsHelpers.cpp:144-147 @
    /// v2.5.2).
    #[doc(alias = "getActiveViews")]
    pub fn active_views(&self) -> Vec<u8> {
        self.with_override_cfg(|cfg| cfg.active_views())
    }

    /// Overrides the inactive color spaces of the merged config: the override config's
    /// [`Config::set_inactive_color_spaces`] (trimmed).
    ///
    /// Port of `ConfigMergingParameters::setInactiveColorSpaces`
    /// (MergeConfigsHelpers.cpp:149-152 @ v2.5.2).
    #[doc(alias = "setInactiveColorSpaces")]
    pub fn set_inactive_color_spaces(&self, colorspaces: impl AsRef<[u8]>) {
        self.with_override_cfg(|cfg| cfg.set_inactive_color_spaces(colorspaces));
    }

    /// The override of the inactive color spaces, as set.
    ///
    /// Port of `ConfigMergingParameters::getInactiveColorSpaces`
    /// (MergeConfigsHelpers.cpp:154-157 @ v2.5.2).
    #[doc(alias = "getInactiveColorSpaces")]
    pub fn inactive_color_spaces(&self) -> Vec<u8> {
        self.with_override_cfg(|cfg| cfg.inactive_color_spaces().to_vec())
    }

    /// Sets the name of the merged config, which a later merge's base or input may name.
    ///
    /// Port of `ConfigMergingParameters::setOutputName` (MergeConfigsHelpers.cpp:159-162 @
    /// v2.5.2).
    #[doc(alias = "setOutputName")]
    pub fn set_output_name(&self, output_name: impl AsRef<[u8]>) {
        self.lock().output_name = c_str(output_name.as_ref()).to_vec();
    }

    /// Port of `ConfigMergingParameters::getOutputName` (MergeConfigsHelpers.cpp:164-167 @
    /// v2.5.2).
    #[doc(alias = "getOutputName")]
    pub fn output_name(&self) -> Vec<u8> {
        self.lock().output_name.clone()
    }

    /// Sets the strategy of the sections whose own is unspecified, also used for the config's
    /// basic attributes such as its description.
    ///
    /// Port of `ConfigMergingParameters::setDefaultStrategy` (MergeConfigsHelpers.cpp:169-172 @
    /// v2.5.2).
    #[doc(alias = "setDefaultStrategy")]
    pub fn set_default_strategy(&self, strategy: MergeStrategies) {
        self.lock().default_strategy = strategy;
    }

    /// Port of `ConfigMergingParameters::getDefaultStrategy` (MergeConfigsHelpers.cpp:174-177 @
    /// v2.5.2).
    #[doc(alias = "getDefaultStrategy")]
    pub fn default_strategy(&self) -> MergeStrategies {
        self.lock().default_strategy
    }

    /// Sets a prefix for the families of the input config's items (with `/` as the separator).
    ///
    /// Port of `ConfigMergingParameters::setInputFamilyPrefix` (MergeConfigsHelpers.cpp:179-182
    /// @ v2.5.2).
    #[doc(alias = "setInputFamilyPrefix")]
    pub fn set_input_family_prefix(&self, prefix: impl AsRef<[u8]>) {
        self.lock().input_family_prefix = c_str(prefix.as_ref()).to_vec();
    }

    /// Port of `ConfigMergingParameters::getInputFamilyPrefix` (MergeConfigsHelpers.cpp:184-187
    /// @ v2.5.2).
    #[doc(alias = "getInputFamilyPrefix")]
    pub fn input_family_prefix(&self) -> Vec<u8> {
        self.lock().input_family_prefix.clone()
    }

    /// Sets a prefix for the families of the base config's items (with `/` as the separator).
    ///
    /// Port of `ConfigMergingParameters::setBaseFamilyPrefix` (MergeConfigsHelpers.cpp:189-192
    /// @ v2.5.2).
    #[doc(alias = "setBaseFamilyPrefix")]
    pub fn set_base_family_prefix(&self, prefix: impl AsRef<[u8]>) {
        self.lock().base_family_prefix = c_str(prefix.as_ref()).to_vec();
    }

    /// Port of `ConfigMergingParameters::getBaseFamilyPrefix` (MergeConfigsHelpers.cpp:194-197
    /// @ v2.5.2).
    #[doc(alias = "getBaseFamilyPrefix")]
    pub fn base_family_prefix(&self) -> Vec<u8> {
        self.lock().base_family_prefix.clone()
    }

    /// Whether the input config's items come before the base's in the merged config.
    ///
    /// Port of `ConfigMergingParameters::setInputFirst` (MergeConfigsHelpers.cpp:199-202 @
    /// v2.5.2).
    #[doc(alias = "setInputFirst")]
    pub fn set_input_first(&self, enabled: bool) {
        self.lock().input_first = enabled;
    }

    /// Port of `ConfigMergingParameters::isInputFirst` (MergeConfigsHelpers.cpp:204-207 @
    /// v2.5.2).
    #[doc(alias = "isInputFirst")]
    pub fn is_input_first(&self) -> bool {
        self.lock().input_first
    }

    /// Whether a conflict fails the merge, rather than logging a warning.
    ///
    /// Port of `ConfigMergingParameters::setErrorOnConflict` (MergeConfigsHelpers.cpp:209-212 @
    /// v2.5.2).
    #[doc(alias = "setErrorOnConflict")]
    pub fn set_error_on_conflict(&self, enabled: bool) {
        self.lock().error_on_conflict = enabled;
    }

    /// Port of `ConfigMergingParameters::isErrorOnConflict` (MergeConfigsHelpers.cpp:214-217 @
    /// v2.5.2).
    #[doc(alias = "isErrorOnConflict")]
    pub fn is_error_on_conflict(&self) -> bool {
        self.lock().error_on_conflict
    }

    /// Whether an input color space equivalent to a base one adds its name and aliases to the
    /// base's instead of being added.
    ///
    /// Port of `ConfigMergingParameters::setAvoidDuplicates` (MergeConfigsHelpers.cpp:219-222 @
    /// v2.5.2).
    #[doc(alias = "setAvoidDuplicates")]
    pub fn set_avoid_duplicates(&self, enabled: bool) {
        self.lock().avoid_duplicates = enabled;
    }

    /// Port of `ConfigMergingParameters::isAvoidDuplicates` (MergeConfigsHelpers.cpp:224-227 @
    /// v2.5.2).
    #[doc(alias = "isAvoidDuplicates")]
    pub fn is_avoid_duplicates(&self) -> bool {
        self.lock().avoid_duplicates
    }

    /// Whether the input config's color spaces are adjusted to the base config's reference
    /// space.
    ///
    /// Port of `ConfigMergingParameters::setAdjustInputReferenceSpace`
    /// (MergeConfigsHelpers.cpp:229-232 @ v2.5.2).
    #[doc(alias = "setAdjustInputReferenceSpace")]
    pub fn set_adjust_input_reference_space(&self, enabled: bool) {
        self.lock().adjust_input_reference_space = enabled;
    }

    /// Port of `ConfigMergingParameters::isAdjustInputReferenceSpace`
    /// (MergeConfigsHelpers.cpp:234-237 @ v2.5.2).
    #[doc(alias = "isAdjustInputReferenceSpace")]
    pub fn is_adjust_input_reference_space(&self) -> bool {
        self.lock().adjust_input_reference_space
    }

    /// Port of `ConfigMergingParameters::setRoles` (MergeConfigsHelpers.cpp:239-242 @ v2.5.2).
    #[doc(alias = "setRoles")]
    pub fn set_roles(&self, strategy: MergeStrategies) {
        self.lock().roles = strategy;
    }

    /// The roles' strategy, or the default strategy where it is unspecified.
    ///
    /// Port of `ConfigMergingParameters::getRoles` (MergeConfigsHelpers.cpp:244-251 @ v2.5.2).
    #[doc(alias = "getRoles")]
    pub fn roles(&self) -> MergeStrategies {
        let p = self.lock();
        or_default(p.roles, p.default_strategy)
    }

    /// Port of `ConfigMergingParameters::setFileRules` (MergeConfigsHelpers.cpp:253-256 @
    /// v2.5.2).
    #[doc(alias = "setFileRules")]
    pub fn set_file_rules(&self, strategy: MergeStrategies) {
        self.lock().file_rules = strategy;
    }

    /// The file rules' strategy, or the default strategy where it is unspecified.
    ///
    /// Port of `ConfigMergingParameters::getFileRules` (MergeConfigsHelpers.cpp:258-265 @
    /// v2.5.2).
    #[doc(alias = "getFileRules")]
    pub fn file_rules(&self) -> MergeStrategies {
        let p = self.lock();
        or_default(p.file_rules, p.default_strategy)
    }

    /// Sets the strategy of the displays and views: shared views, displays, viewing rules, the
    /// virtual display, the active displays and views.
    ///
    /// Port of `ConfigMergingParameters::setDisplayViews` (MergeConfigsHelpers.cpp:267-270 @
    /// v2.5.2).
    #[doc(alias = "setDisplayViews")]
    pub fn set_display_views(&self, strategy: MergeStrategies) {
        self.lock().display_views = strategy;
    }

    /// The displays and views' strategy, or the default strategy where it is unspecified.
    ///
    /// Port of `ConfigMergingParameters::getDisplayViews` (MergeConfigsHelpers.cpp:272-279 @
    /// v2.5.2).
    #[doc(alias = "getDisplayViews")]
    pub fn display_views(&self) -> MergeStrategies {
        let p = self.lock();
        or_default(p.display_views, p.default_strategy)
    }

    /// Sets the strategy of the view transforms and the default view transform.
    ///
    /// Port of `ConfigMergingParameters::setViewTransforms` (MergeConfigsHelpers.cpp:281-284 @
    /// v2.5.2).
    #[doc(alias = "setViewTransforms")]
    pub fn set_view_transforms(&self, strategy: MergeStrategies) {
        self.lock().view_transforms = strategy;
    }

    /// The view transforms' strategy, or the default strategy where it is unspecified.
    ///
    /// Port of `ConfigMergingParameters::getViewTransforms` (MergeConfigsHelpers.cpp:286-293 @
    /// v2.5.2).
    #[doc(alias = "getViewTransforms")]
    pub fn view_transforms(&self) -> MergeStrategies {
        let p = self.lock();
        or_default(p.view_transforms, p.default_strategy)
    }

    /// Port of `ConfigMergingParameters::setLooks` (MergeConfigsHelpers.cpp:295-298 @ v2.5.2).
    #[doc(alias = "setLooks")]
    pub fn set_looks(&self, strategy: MergeStrategies) {
        self.lock().looks = strategy;
    }

    /// The looks' strategy, or the default strategy where it is unspecified.
    ///
    /// Port of `ConfigMergingParameters::getLooks` (MergeConfigsHelpers.cpp:300-307 @ v2.5.2).
    #[doc(alias = "getLooks")]
    pub fn looks(&self) -> MergeStrategies {
        let p = self.lock();
        or_default(p.looks, p.default_strategy)
    }

    /// Sets the strategy of the color spaces, the environment, the search path, the family
    /// separator and the inactive color spaces.
    ///
    /// Port of `ConfigMergingParameters::setColorspaces` (MergeConfigsHelpers.cpp:309-312 @
    /// v2.5.2).
    #[doc(alias = "setColorspaces")]
    pub fn set_colorspaces(&self, strategy: MergeStrategies) {
        self.lock().colorspaces = strategy;
    }

    /// The color spaces' strategy, or the default strategy where it is unspecified.
    ///
    /// Port of `ConfigMergingParameters::getColorspaces` (MergeConfigsHelpers.cpp:314-321 @
    /// v2.5.2).
    #[doc(alias = "getColorspaces")]
    pub fn colorspaces(&self) -> MergeStrategies {
        let p = self.lock();
        or_default(p.colorspaces, p.default_strategy)
    }

    /// Port of `ConfigMergingParameters::setNamedTransforms` (MergeConfigsHelpers.cpp:323-326 @
    /// v2.5.2).
    #[doc(alias = "setNamedTransforms")]
    pub fn set_named_transforms(&self, strategy: MergeStrategies) {
        self.lock().named_transforms = strategy;
    }

    /// The named transforms' strategy, or the default strategy where it is unspecified.
    ///
    /// Port of `ConfigMergingParameters::getNamedTransforms` (MergeConfigsHelpers.cpp:328-335 @
    /// v2.5.2).
    #[doc(alias = "getNamedTransforms")]
    pub fn named_transforms(&self) -> MergeStrategies {
        let p = self.lock();
        or_default(p.named_transforms, p.default_strategy)
    }

    /// The parameters as text, `<label: value, ...>`: the strings that aren't empty, the flags,
    /// and each strategy (a section's resolved by the default), then the environment
    /// overrides, `environment: [name=value, ...]` (`=value` only for a value that isn't
    /// empty), when there are any. The Python binding's `repr()`.
    ///
    /// Port of `operator<<(std::ostream&, const ConfigMergingParameters&)`
    /// (MergeConfigsHelpers.cpp:337-405 @ v2.5.2).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut os = b"<".to_vec();
        let mut first = true;

        let mut print_str = |os: &mut Vec<u8>, label: &str, value: &[u8]| {
            if !value.is_empty() {
                if !first {
                    os.extend_from_slice(b", ");
                }
                os.extend_from_slice(label.as_bytes());
                os.extend_from_slice(b": ");
                os.extend_from_slice(value);
                first = false;
            }
        };

        print_str(&mut os, "base", &self.base_config_name());
        print_str(&mut os, "input", &self.input_config_name());
        print_str(&mut os, "output_name", &self.output_name());
        print_str(&mut os, "input_family_prefix", &self.input_family_prefix());
        print_str(&mut os, "base_family_prefix", &self.base_family_prefix());
        print_str(
            &mut os,
            "input_first",
            bool_text(self.is_input_first()).as_bytes(),
        );
        print_str(
            &mut os,
            "error_on_conflict",
            bool_text(self.is_error_on_conflict()).as_bytes(),
        );
        print_str(
            &mut os,
            "default_strategy",
            enum_to_strategy_string(self.default_strategy()).as_bytes(),
        );
        print_str(
            &mut os,
            "avoid_duplicates",
            bool_text(self.is_avoid_duplicates()).as_bytes(),
        );
        print_str(
            &mut os,
            "adjust_input_reference_space",
            bool_text(self.is_adjust_input_reference_space()).as_bytes(),
        );
        print_str(&mut os, "name", &self.name());
        print_str(&mut os, "description", &self.description());
        print_str(&mut os, "search_path", &self.search_path());
        print_str(&mut os, "active_displays", &self.active_displays());
        print_str(&mut os, "active_views", &self.active_views());
        print_str(
            &mut os,
            "inactive_colorspaces",
            &self.inactive_color_spaces(),
        );
        let strategies = [
            ("roles", self.roles()),
            ("file_rules", self.file_rules()),
            ("display-views", self.display_views()),
            ("view_transforms", self.view_transforms()),
            ("looks", self.looks()),
            ("colorspaces", self.colorspaces()),
            ("named_transforms", self.named_transforms()),
        ];
        for (label, strategy) in strategies {
            print_str(&mut os, label, enum_to_strategy_string(strategy).as_bytes());
        }

        // Environment vars.
        let num_env = self.num_environment_vars();
        if num_env > 0 {
            if !first {
                os.extend_from_slice(b", ");
            }
            os.extend_from_slice(b"environment: [");
            for i in 0..num_env {
                if i > 0 {
                    os.extend_from_slice(b", ");
                }
                os.extend_from_slice(&self.environment_var(i));
                let val = self.environment_var_value(i);
                if !val.is_empty() {
                    os.push(b'=');
                    os.extend_from_slice(&val);
                }
            }
            os.push(b']');
        }

        os.push(b'>');
        os
    }
}

/// How `operator<<`'s `print_bool` writes a flag.
fn bool_text(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

/// The error where upstream dereferences the null parameters a merger holds (`addParams` given
/// `None`): the wheel's process dies there (docs/improvements.md, U-176).
fn null_params(call: &str, index: usize) -> Exception {
    Exception::new(format!(
        "{call}: the merge parameters at index {index} are None, a null pointer the wheel \
         dereferences (its process dies; docs/improvements.md, U-176)"
    ))
}

/// The `std::out_of_range` that `std::vector::at(n)` throws for a vector of `size` elements, as
/// each wheel's C++ library words it: MSVC's "invalid vector subscript", libstdc++'s
/// `vector::_M_range_check` message (docs/improvements.md, I-387).
fn vector_at_out_of_range(n: usize, size: usize) -> Exception {
    if cfg!(windows) {
        Exception::out_of_range("invalid vector subscript")
    } else {
        Exception::out_of_range(format!(
            "vector::_M_range_check: __n (which is {n}) >= this->size() (which is {size})"
        ))
    }
}

/// The controller of a series of merges: the search paths and working directory where it finds
/// the base and input configs, one [`ConfigMergingParameters`] per merge, and, once merged,
/// each merge's config (a later merge's base or input may name an earlier one's output). An
/// OCIOM file holds one.
///
/// A value (owner decision P9-2): [`create_editable_copy`](ConfigMerger::create_editable_copy)
/// is upstream's deep copy. The merge parameters it holds are shared handles, as upstream's
/// pointers; `None` stands for upstream's null pointer, which `addParams` takes (the wheel
/// accepts `None`), and which the copy, the merge and the writer refuse where upstream
/// dereferences it (docs/improvements.md, U-176).
///
/// Port of `ConfigMerger` (include/OpenColorIO/OpenColorAppHelpers.h:711-853) and
/// `ConfigMerger::Impl` (src/OpenColorIO/apphelpers/mergeconfigs/MergeConfigsHelpers.h:129-194,
/// MergeConfigsHelpers.cpp:449-680 @ v2.5.2).
#[derive(Debug)]
pub struct ConfigMerger {
    /// `m_searchPaths`: where the base and input configs are looked for.
    search_paths: Vec<Vec<u8>>,
    /// `m_workingDir`: the directory relative search paths start from, and the search path
    /// when there is none.
    working_dir: Vec<u8>,
    /// `m_majorVersion`: the OCIOM format's.
    major_version: u32,
    /// `m_minorVersion`.
    minor_version: u32,
    /// `m_mergeParams`: one per merge; `None` for upstream's null pointer.
    merge_params: Vec<Option<ConfigMergingParameters>>,
    /// `m_mergedConfigs`: each merge's output.
    merged_configs: Vec<Arc<Config>>,
}

impl Default for ConfigMerger {
    fn default() -> Self {
        ConfigMerger::new()
    }
}

impl ConfigMerger {
    /// A merger without merges, search paths or working directory, of OCIOM version 1.0.
    ///
    /// Port of `ConfigMerger::Create` and `ConfigMerger::Impl::Impl`
    /// (MergeConfigsHelpers.cpp:521-535, MergeConfigsHelpers.h:139-151 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> ConfigMerger {
        ConfigMerger {
            search_paths: Vec::new(),
            working_dir: Vec::new(),
            major_version: 1,
            minor_version: 0,
            merge_params: Vec::new(),
            merged_configs: Vec::new(),
        }
    }

    /// A deep copy: the search paths, working directory and version, a copy of each merge's
    /// parameters ([`ConfigMergingParameters::create_editable_copy`], which still shares their
    /// overrides, docs/improvements.md, I-386), and a copy of each merged config. Fails where a
    /// copy of the parameters fails, and for parameters that are `None` (after making the new
    /// parameters, as upstream does, so the environment's error comes first), which upstream
    /// dereferences (docs/improvements.md, U-176).
    ///
    /// Port of `ConfigMerger::createEditableCopy` and `ConfigMerger::Impl::operator=`
    /// (MergeConfigsHelpers.cpp:560-565, MergeConfigsHelpers.h:156-180 @ v2.5.2).
    #[doc(alias = "createEditableCopy")]
    pub fn create_editable_copy(&self) -> Result<ConfigMerger> {
        let mut merger = ConfigMerger::new();
        merger.search_paths.clone_from(&self.search_paths);
        merger.working_dir.clone_from(&self.working_dir);
        merger.major_version = self.major_version;
        merger.minor_version = self.minor_version;

        merger.merge_params.reserve(self.merge_params.len());
        for (i, param) in self.merge_params.iter().enumerate() {
            // `param->createEditableCopy()` makes the new parameters (`Create`, so a
            // `Config::Create`, which can fail on the environment) before it reads `*this`: for
            // a null pointer too, so a failing environment is reported before the null.
            let copy = ConfigMergingParameters::new()?;
            let param = param
                .as_ref()
                .ok_or_else(|| null_params("createEditableCopy", i))?;
            copy.lock().assign(&param.lock());
            merger.merge_params.push(Some(copy));
        }

        merger.merged_configs.reserve(self.merged_configs.len());
        for config in &self.merged_configs {
            merger.merged_configs.push(Arc::new(Config::clone(config)));
        }
        Ok(merger)
    }

    /// The config `value` names: the first file found at `value` along the search paths
    /// (relative ones from the working directory, trimmed of spaces and trailing `/`; the
    /// working directory alone without search paths), else the built-in config of that name,
    /// else the output of a merge whose output name is `value`, ignoring case, else `None`. A
    /// file or built-in config that exists but fails to load is skipped silently
    /// (docs/improvements.md, I-388). A merge whose output doesn't exist yet (a later merge)
    /// fails with the C++ library's `std::out_of_range` (I-387), and parameters that are `None`
    /// are refused where upstream dereferences them (U-176).
    ///
    /// Port of `ConfigMerger::Impl::loadConfig` (MergeConfigsHelpers.cpp:449-516 @ v2.5.2).
    // `mergeConfigs` (9.2j, p9-merge-3) calls it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn load_config(&self, value: &[u8]) -> Result<Option<Arc<Config>>> {
        // Get the absolute path.
        let mut searchpaths: Vec<Vec<u8>> = Vec::new();

        if self.search_paths.is_empty() {
            searchpaths.push(self.working_dir.clone());
        }

        for path in &self.search_paths {
            // Remove trailing "/", and spaces.
            let mut dirname = right_trim_char(trim(path), b'/').to_vec();

            if !os_path::isabs(&dirname) {
                dirname = os_path::join(&self.working_dir, &dirname);
            }

            searchpaths.push(os_path::normpath(&dirname));
        }

        for searchpath in &searchpaths {
            // Try to load the provided config using the search paths. Return as soon as a
            // valid path is found. Normalize the path to prevent directory traversal via '../'
            // sequences. A config that can't be loaded, for whatever reason, is skipped
            // (upstream's `catch(...)`).
            let resolvedfullpath = os_path::normpath(&os_path::join(searchpath, value));
            if let Ok(config) = Config::create_from_file(&resolvedfullpath) {
                return Ok(Some(config));
            }
        }

        // Try to load the provided base config name as a built-in config.
        if let Ok(config) = Config::create_from_builtin_config(value) {
            return Ok(Some(config));
        }

        // Must be a reference to a config from a previous merge.
        for (i, params) in self.merge_params.iter().enumerate() {
            let params = params
                .as_ref()
                .ok_or_else(|| null_params("mergeConfigs", i))?;
            if strcasecmp(params.output_name(), value).is_eq() {
                // Use the config from the index.
                return match self.merged_configs.get(i) {
                    Some(config) => Ok(Some(Arc::clone(config))),
                    None => Err(vector_at_out_of_range(i, self.merged_configs.len())),
                };
            }
        }

        Ok(None)
    }

    /// The number of search paths.
    ///
    /// Port of `ConfigMerger::getNumSearchPaths` (MergeConfigsHelpers.cpp:572-575 @ v2.5.2).
    #[doc(alias = "getNumSearchPaths")]
    pub fn num_search_paths(&self) -> i32 {
        self.search_paths.len() as i32
    }

    /// The search path at `index`; `""` outside the list.
    ///
    /// Port of `ConfigMerger::getSearchPath` (MergeConfigsHelpers.cpp:577-581 @ v2.5.2).
    #[doc(alias = "getSearchPath")]
    pub fn search_path(&self, index: i32) -> Vec<u8> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.search_paths.get(i))
            .cloned()
            .unwrap_or_default()
    }

    /// Sets the search paths: `path` split on `:` (`""` gives one empty path; a Windows drive
    /// letter is split off too, docs/improvements.md, I-389).
    ///
    /// Port of `ConfigMerger::setSearchPath` (MergeConfigsHelpers.cpp:583-586 @ v2.5.2).
    #[doc(alias = "setSearchPath")]
    pub fn set_search_path(&mut self, path: impl AsRef<[u8]>) {
        self.search_paths = split(c_str(path.as_ref()), b':');
    }

    /// Adds a search path; an empty one is ignored.
    ///
    /// Port of `ConfigMerger::addSearchPath` (MergeConfigsHelpers.cpp:588-594 @ v2.5.2).
    #[doc(alias = "addSearchPath")]
    pub fn add_search_path(&mut self, path: impl AsRef<[u8]>) {
        let path = c_str(path.as_ref());
        if !path.is_empty() {
            self.search_paths.push(path.to_vec());
        }
    }

    /// Sets the directory that relative search paths start from, and the search path when
    /// there is none (an OCIOM file's directory, by default).
    ///
    /// Port of `ConfigMerger::setWorkingDir` (MergeConfigsHelpers.cpp:596-599 @ v2.5.2).
    #[doc(alias = "setWorkingDir")]
    pub fn set_working_dir(&mut self, dirname: impl AsRef<[u8]>) {
        self.working_dir = c_str(dirname.as_ref()).to_vec();
    }

    /// Port of `ConfigMerger::getWorkingDir` (MergeConfigsHelpers.cpp:601-604 @ v2.5.2).
    #[doc(alias = "getWorkingDir")]
    pub fn working_dir(&self) -> &[u8] {
        &self.working_dir
    }

    /// The parameters of the merge at `index` (the same object, a shared handle); `None`
    /// outside the list, and for parameters added as `None`.
    ///
    /// Port of `ConfigMerger::getParams` (MergeConfigsHelpers.cpp:606-613 @ v2.5.2).
    #[doc(alias = "getParams")]
    pub fn params(&self, index: i32) -> Option<ConfigMergingParameters> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.merge_params.get(i))
            .cloned()
            .flatten()
    }

    /// The number of merges, `None` parameters included.
    ///
    /// Port of `ConfigMerger::getNumConfigMergingParameters` (MergeConfigsHelpers.cpp:615-618 @
    /// v2.5.2).
    #[doc(alias = "getNumConfigMergingParameters")]
    pub fn num_config_merging_parameters(&self) -> i32 {
        self.merge_params.len() as i32
    }

    /// Adds a merge's parameters (the handle itself, shared with the caller). `None`, which
    /// the wheel accepts, is kept as upstream's null pointer (docs/improvements.md, U-176).
    ///
    /// Port of `ConfigMerger::addParams` (MergeConfigsHelpers.cpp:620-623 @ v2.5.2).
    #[doc(alias = "addParams")]
    pub fn add_params(&mut self, params: Option<ConfigMergingParameters>) {
        self.merge_params.push(params);
    }

    /// The merger as an OCIOM file. Fails as the writer does: "Error building YAML: " and the
    /// message of a list of active displays, active views or inactive color spaces that opens a
    /// quote it doesn't close; and for parameters that are `None`, which upstream dereferences
    /// (docs/improvements.md, U-176). The Python binding's `serialize()` and `repr()`.
    ///
    /// Port of `ConfigMerger::serialize` and `operator<<(std::ostream&, const ConfigMerger&)`
    /// (MergeConfigsHelpers.cpp:625-644 @ v2.5.2).
    pub fn serialize(&self) -> Result<Vec<u8>> {
        ociom_yaml::write(self).map_err(|e| match e {
            WriteError::Exception(e) => {
                let mut error = b"Error building YAML: ".to_vec();
                error.extend_from_slice(e.what());
                Exception::new(error)
            }
            WriteError::NullParams(index) => null_params("serialize", index),
        })
    }

    /// Port of `ConfigMerger::getMajorVersion` (MergeConfigsHelpers.cpp:646-649 @ v2.5.2).
    #[doc(alias = "getMajorVersion")]
    pub fn major_version(&self) -> u32 {
        self.major_version
    }

    /// Port of `ConfigMerger::getMinorVersion` (MergeConfigsHelpers.cpp:651-654 @ v2.5.2).
    #[doc(alias = "getMinorVersion")]
    pub fn minor_version(&self) -> u32 {
        self.minor_version
    }

    /// Sets the version of the OCIOM format.
    ///
    /// Port of `ConfigMerger::setVersion` (MergeConfigsHelpers.cpp:656-660 @ v2.5.2).
    #[doc(alias = "setVersion")]
    pub fn set_version(&mut self, major: u32, minor: u32) {
        self.major_version = major;
        self.minor_version = minor;
    }

    /// The number of merged configs.
    ///
    /// Port of `ConfigMerger::getNumMergedConfigs` (MergeConfigsHelpers.cpp:662-665 @ v2.5.2).
    #[doc(alias = "getNumMergedConfigs")]
    pub fn num_merged_configs(&self) -> i32 {
        self.merged_configs.len() as i32
    }

    /// The last merge's config; `None` before any merge.
    ///
    /// Port of `ConfigMerger::getMergedConfig()` (MergeConfigsHelpers.cpp:667-670 @ v2.5.2).
    #[doc(alias = "getMergedConfig")]
    pub fn last_merged_config(&self) -> Option<Arc<Config>> {
        self.merged_config(self.merged_configs.len() as i32 - 1)
    }

    /// The config of the merge at `index`; `None` outside the list.
    ///
    /// Port of `ConfigMerger::getMergedConfig(int)` (MergeConfigsHelpers.cpp:672-679 @ v2.5.2).
    #[doc(alias = "getMergedConfig")]
    pub fn merged_config(&self, index: i32) -> Option<Arc<Config>> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.merged_configs.get(i))
            .cloned()
    }
}

/// The C library's `strerror` of an I/O error: its text without Rust's " (os error N)".
fn strerror(e: &std::io::Error) -> String {
    let text = e.to_string();
    match (e.raw_os_error(), text.rfind(" (os error ")) {
        (Some(_), Some(at)) => text[..at].to_string(),
        _ => text,
    }
}

impl ConfigMerger {
    /// The merger an OCIOM file describes (`filepath` up to its first NUL). The errors: "The
    /// merge options filepath is missing." (a missing-file exception) for an empty path, "Error
    /// could not read '<filepath>' merge options." for a file that can't be opened, and the
    /// reader's, "Error: Loading the OCIOM Merge parameters '<filepath>' failed. ..."
    /// (`ociom_yaml::read`). The file's bytes are read as the `std::ifstream` gives them to
    /// yaml-cpp ([`IStream::file`]), as [`Config::create_from_file`] reads a config.
    ///
    /// Port of `ConfigMerger::CreateFromFile` (MergeConfigsHelpers.cpp:537-558 @ v2.5.2).
    #[doc(alias = "CreateFromFile")]
    pub fn from_file(filepath: impl AsRef<[u8]>) -> Result<Arc<ConfigMerger>> {
        let filepath = c_str(filepath.as_ref());
        if filepath.is_empty() {
            return Err(Exception::missing_file(
                "The merge options filepath is missing.",
            ));
        }

        let mut ifstream = create_input_file_stream(filepath).map_err(|_| {
            let mut os = b"Error could not read '".to_vec();
            os.extend_from_slice(filepath);
            os.extend_from_slice(b"' merge options.");
            Exception::new(os)
        })?;

        // The stream's bytes, up to a read that fails (a directory, which Linux opens).
        let mut data = Vec::new();
        let read = std::io::Read::read_to_end(&mut ifstream, &mut data);
        drop(ifstream);

        // A read that fails ends MSVC's file stream, as the end of the file does. libstdc++'s
        // throws `std::ios_base::failure` from `basic_filebuf::underflow` as yaml-cpp reads the
        // stream buffer, inside `Impl::Read`'s handler, which wraps its `what()` (as
        // `Config::create_from_file`, which Rocky Linux 9 checked for a directory).
        if let Err(e) = read
            && !cfg!(windows)
        {
            let mut what = b"basic_filebuf::underflow error reading the file: ".to_vec();
            what.extend_from_slice(strerror(&e).as_bytes());
            return Err(ociom_yaml::failed(filepath, &what));
        }

        Ok(Arc::new(ociom_yaml::read(IStream::file(&data), filepath)?))
    }
}

#[cfg(test)]
#[path = "merge_configs_helpers_oracle_tests.rs"]
mod oracle_tests;

#[cfg(test)]
#[path = "merge_configs_helpers_tests.rs"]
mod tests;
