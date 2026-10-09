// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The registry of the built-in configs, and the `ocio://` URIs that name them:
//! `BuiltinConfigRegistry.cpp` and `BuiltinConfigRegistry.h` (src/OpenColorIO/builtinconfigs @
//! v2.5.2).

use std::sync::OnceLock;

use ocio_ops::exception::{Exception, Result};
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::string_utils::c_str;

/// `OUT_OF_RANGE_EXCEPTION_TEXT` (BuiltinConfigRegistry.cpp:19 @ v2.5.2).
const OUT_OF_RANGE_EXCEPTION_TEXT: &str = "Config index is out of range.";

/// `DEFAULT_BUILTIN_CONFIG_URI` (BuiltinConfigRegistry.cpp:21 @ v2.5.2).
const DEFAULT_BUILTIN_CONFIG_URI: &[u8] = b"ocio://cg-config-v4.0.0_aces-v2.0_ocio-v2.5";
/// `LATEST_CG_BUILTIN_CONFIG_URI` (BuiltinConfigRegistry.cpp:22 @ v2.5.2).
const LATEST_CG_BUILTIN_CONFIG_URI: &[u8] = b"ocio://cg-config-v4.0.0_aces-v2.0_ocio-v2.5";
/// `LATEST_STUDIO_BUILTIN_CONFIG_URI` (BuiltinConfigRegistry.cpp:23 @ v2.5.2).
const LATEST_STUDIO_BUILTIN_CONFIG_URI: &[u8] = b"ocio://studio-config-v4.0.0_aces-v2.0_ocio-v2.5";

/// `BUILTIN_DEFAULT_NAME` (BuiltinConfigRegistry.cpp:25 @ v2.5.2).
const BUILTIN_DEFAULT_NAME: &[u8] = b"default";
/// `BUILTIN_LATEST_CG_NAME` (BuiltinConfigRegistry.cpp:26 @ v2.5.2).
const BUILTIN_LATEST_CG_NAME: &[u8] = b"cg-config-latest";
/// `BUILTIN_LATEST_STUDIO_NAME` (BuiltinConfigRegistry.cpp:27 @ v2.5.2).
const BUILTIN_LATEST_STUDIO_NAME: &[u8] = b"studio-config-latest";

/// The prefix of the URIs of the built-in configs.
///
/// Port of `OCIO_BUILTIN_URI_PREFIX` (src/OpenColorIO/Config.cpp:63 @ v2.5.2).
pub const OCIO_BUILTIN_URI_PREFIX: &str = "ocio://";

/// Whether `b` is a space of the regular expressions' `\s` class: the C locale's `isspace`,
/// which both wheels' `std::regex_traits<char>` use (their global locale is the classic one).
fn is_regex_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

/// Upstream's `std::regex_search` of `ocio:\/\/([^\s]+)` in `text`: the capture of the
/// leftmost match, the longest run of non-space bytes that follows the first `ocio://` with at
/// least one.
///
/// Port of the `uriPattern` searches (BuiltinConfigRegistry.cpp:36-39, Config.cpp:1160-1164 and
/// 1246-1252 @ v2.5.2).
pub(crate) fn search_builtin_uri(text: &[u8]) -> Option<&[u8]> {
    let prefix = OCIO_BUILTIN_URI_PREFIX.as_bytes();
    (0..text.len()).find_map(|start| {
        let rest = text[start..].strip_prefix(prefix)?;
        let len = rest.iter().take_while(|&&b| !is_regex_space(b)).count();
        (len > 0).then(|| &rest[..len])
    })
}

/// The URI of the built-in config that a URI names, for the three special names `default`,
/// `cg-config-latest` and `studio-config-latest` (ignoring case), wherever the URI is in
/// `original_path`; otherwise `original_path` itself. Its C string ends at the first NUL.
///
/// Port of `ResolveConfigPath` (src/OpenColorIO/builtinconfigs/BuiltinConfigRegistry.cpp:32-58
/// @ v2.5.2). Upstream's null pointer is the empty string.
#[doc(alias = "ResolveConfigPath")]
pub fn resolve_config_path(original_path: &[u8]) -> &[u8] {
    let uri = c_str(original_path);
    // Check if original path starts with "ocio://".
    if let Some(name) = search_builtin_uri(uri) {
        if strcasecmp(name, BUILTIN_DEFAULT_NAME).is_eq() {
            return DEFAULT_BUILTIN_CONFIG_URI;
        } else if strcasecmp(name, BUILTIN_LATEST_CG_NAME).is_eq() {
            return LATEST_CG_BUILTIN_CONFIG_URI;
        } else if strcasecmp(name, BUILTIN_LATEST_STUDIO_NAME).is_eq() {
            return LATEST_STUDIO_BUILTIN_CONFIG_URI;
        }
    }

    // Return originalPath if no special path was used.
    uri
}

/// A built-in config: its name, its name for user interfaces, its text and whether it is
/// recommended.
///
/// Port of `BuiltinConfigRegistryImpl::BuiltinConfigData` (BuiltinConfigRegistry.h:17-40 @
/// v2.5.2).
#[derive(Debug, Clone)]
struct BuiltinConfigData {
    /// `m_config`: the text (upstream keeps the pointer it is given).
    config: &'static [u8],
    /// `m_name`.
    name: Vec<u8>,
    /// `m_uiName`.
    ui_name: Vec<u8>,
    /// `m_isRecommended`.
    is_recommended: bool,
}

/// The registry of the built-in configs.
///
/// Port of `BuiltinConfigRegistry` (include/OpenColorIO/OpenColorIO.h:3993-4038 @ v2.5.2) and
/// `BuiltinConfigRegistryImpl` (BuiltinConfigRegistry.h:15-97 @ v2.5.2), its only
/// implementation. Upstream deletes the copy constructor and assignment: the type has neither
/// `Clone` nor `Default`.
#[derive(Debug)]
pub struct BuiltinConfigRegistry {
    /// `m_builtinConfigs`.
    builtin_configs: Vec<BuiltinConfigData>,
}

impl BuiltinConfigRegistry {
    /// The global registry, with the CG and Studio configs registered the first time it is
    /// asked for.
    ///
    /// Port of `BuiltinConfigRegistry::Get` and `BuiltinConfigRegistryImpl::init`
    /// (BuiltinConfigRegistry.cpp:60-84 @ v2.5.2).
    #[doc(alias = "Get")]
    pub fn get() -> &'static BuiltinConfigRegistry {
        static GLOBAL_REGISTRY: OnceLock<BuiltinConfigRegistry> = OnceLock::new();
        GLOBAL_REGISTRY.get_or_init(|| {
            let mut registry = BuiltinConfigRegistry::new();
            registry.init();
            registry
        })
    }

    /// An empty registry.
    ///
    /// Port of `BuiltinConfigRegistryImpl::BuiltinConfigRegistryImpl`
    /// (BuiltinConfigRegistry.h:45 @ v2.5.2).
    pub(crate) fn new() -> BuiltinConfigRegistry {
        BuiltinConfigRegistry {
            builtin_configs: Vec::new(),
        }
    }

    /// Registers the CG and Studio configs, if the registry is empty.
    ///
    /// Port of `BuiltinConfigRegistryImpl::init` (BuiltinConfigRegistry.cpp:75-84 @ v2.5.2).
    fn init(&mut self) {
        if self.builtin_configs.is_empty() {
            self.builtin_configs.clear();

            super::cg_config::register(self);
            super::studio_config::register(self);
        }
    }

    /// Adds a built-in config, or replaces the one whose name is the same ignoring case. The
    /// strings end at their first NUL, as upstream's C strings.
    ///
    /// Port of `BuiltinConfigRegistryImpl::addBuiltin` (BuiltinConfigRegistry.cpp:86-101 @
    /// v2.5.2).
    pub(crate) fn add_builtin(
        &mut self,
        name: &[u8],
        ui_name: &[u8],
        config: &'static [u8],
        is_recommended: bool,
    ) {
        let data = BuiltinConfigData {
            config: c_str(config),
            name: c_str(name).to_vec(),
            ui_name: c_str(ui_name).to_vec(),
            is_recommended,
        };

        for builtin in &mut self.builtin_configs {
            // Overwrite data if the config name is the same.
            if strcasecmp(&data.name, &builtin.name).is_eq() {
                *builtin = data;
                return;
            }
        }

        self.builtin_configs.push(data);
    }

    /// The number of built-in configs.
    ///
    /// Port of `BuiltinConfigRegistryImpl::getNumBuiltinConfigs`
    /// (BuiltinConfigRegistry.cpp:103-106 @ v2.5.2).
    #[doc(alias = "getNumBuiltinConfigs")]
    pub fn num_builtin_configs(&self) -> usize {
        self.builtin_configs.len()
    }

    /// The built-in config at `config_index`, or "Config index is out of range.".
    fn at(&self, config_index: usize) -> Result<&BuiltinConfigData> {
        self.builtin_configs
            .get(config_index)
            .ok_or_else(|| Exception::new(OUT_OF_RANGE_EXCEPTION_TEXT))
    }

    /// The name of the config at `config_index`.
    ///
    /// Port of `BuiltinConfigRegistryImpl::getBuiltinConfigName`
    /// (BuiltinConfigRegistry.cpp:108-116 @ v2.5.2).
    #[doc(alias = "getBuiltinConfigName")]
    pub fn builtin_config_name(&self, config_index: usize) -> Result<&[u8]> {
        Ok(&self.at(config_index)?.name)
    }

    /// The name of the config at `config_index` for user interfaces.
    ///
    /// Port of `BuiltinConfigRegistryImpl::getBuiltinConfigUIName`
    /// (BuiltinConfigRegistry.cpp:118-126 @ v2.5.2).
    #[doc(alias = "getBuiltinConfigUIName")]
    pub fn builtin_config_ui_name(&self, config_index: usize) -> Result<&[u8]> {
        Ok(&self.at(config_index)?.ui_name)
    }

    /// The YAML text of the config at `config_index`.
    ///
    /// Port of `BuiltinConfigRegistryImpl::getBuiltinConfig` (BuiltinConfigRegistry.cpp:128-136
    /// @ v2.5.2).
    #[doc(alias = "getBuiltinConfig")]
    pub fn builtin_config(&self, config_index: usize) -> Result<&'static [u8]> {
        Ok(self.at(config_index)?.config)
    }

    /// The YAML text of the config named `config_name` (ignoring case, up to its first NUL),
    /// or "Could not find '<name>' in the built-in configurations.".
    ///
    /// Port of `BuiltinConfigRegistryImpl::getBuiltinConfigByName`
    /// (BuiltinConfigRegistry.cpp:138-152 @ v2.5.2).
    #[doc(alias = "getBuiltinConfigByName")]
    pub fn builtin_config_by_name(&self, config_name: &[u8]) -> Result<&'static [u8]> {
        let config_name = c_str(config_name);
        // Search for config name.
        for builtin in &self.builtin_configs {
            if strcasecmp(config_name, &builtin.name).is_eq() {
                return Ok(builtin.config);
            }
        }

        let mut os = b"Could not find '".to_vec();
        os.extend_from_slice(config_name);
        os.extend_from_slice(b"' in the built-in configurations.");
        Err(Exception::new(os))
    }

    /// Whether the config at `config_index` is recommended.
    ///
    /// Port of `BuiltinConfigRegistryImpl::isBuiltinConfigRecommended`
    /// (BuiltinConfigRegistry.cpp:154-162 @ v2.5.2).
    #[doc(alias = "isBuiltinConfigRecommended")]
    pub fn is_builtin_config_recommended(&self, config_index: usize) -> Result<bool> {
        Ok(self.at(config_index)?.is_recommended)
    }
}
