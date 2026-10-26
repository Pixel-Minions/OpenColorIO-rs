// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Config merging: a port of `src/OpenColorIO/apphelpers/mergeconfigs/` @ v2.5.2, whose
//! public API is `include/OpenColorIO/OpenColorAppHelpers.h:536-890`
//! (`ConfigMergingParameters`, `ConfigMerger`, `ConfigMergingHelpers`; owner decision P9-2).
//!
//! - [`merge_configs_helpers`]: the parameters of a merge and the merger
//!   (`MergeConfigsHelpers.cpp/.h`);
//! - `ociom_yaml`: the OCIOM file format, which holds a merger's parameters (`OCIOMYaml.cpp/.h`);
//! - `section_merger`: the mergers of each section of a config (`SectionMerger.cpp/.h`).

pub mod merge_configs_helpers;
pub(crate) mod ociom_yaml;
pub(crate) mod section_merger;
