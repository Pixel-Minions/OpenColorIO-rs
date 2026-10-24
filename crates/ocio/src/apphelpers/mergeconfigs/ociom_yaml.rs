// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The OCIOM file format, which holds a config merger's parameters: a port of
//! `src/OpenColorIO/apphelpers/mergeconfigs/OCIOMYaml.h` and `OCIOMYaml.cpp` @ v2.5.2.

use crate::apphelpers::mergeconfigs::merge_configs_helpers::MergeStrategies;

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
