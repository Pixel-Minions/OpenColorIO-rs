// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The app helpers: a port of `src/OpenColorIO/apphelpers/` @ v2.5.2, whose public API is
//! `include/OpenColorIO/OpenColorAppHelpers.h` (PLAN.md, Phase 5; owner decision P5-3).

pub(crate) mod category_helpers;
pub mod color_space_helpers;
pub mod legacy_viewing_pipeline;

#[cfg(test)]
mod configs_data;
