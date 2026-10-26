// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The app helpers' bindings (src/bindings/python/apphelpers @ v2.5.2, chunk 6.7c): the
//! submodules `ColorSpaceHelpers`, `ConfigMergingHelpers` and `DisplayViewHelpers`, and the
//! classes of the color space menus, config merging (`PyMergeConfigs.cpp`, chunk 6.8a), the
//! legacy viewing pipeline and the mixing helpers, in upstream's registration order.

pub(crate) mod py_color_space_helpers;
pub(crate) mod py_display_view_helpers;
pub(crate) mod py_legacy_viewing_pipeline;
pub(crate) mod py_merge_configs;
pub(crate) mod py_mixing_helpers;
