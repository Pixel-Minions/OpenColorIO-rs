// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms: a port of `src/OpenColorIO/transforms/builtins/` @ v2.5.2: the
//! registry and every entry's style and description, and the ops of the identity and of the
//! ARRI, Panasonic, RED and Sony cameras. The other entries' ops come with WP 3.2g and
//! `p3-after-p2`.

pub mod builtin_transform_registry;

// `ColorMatrixHelpers` (transforms/builtins/ColorMatrixHelpers.cpp/.h), which Phase 2 ported
// into `ocio-ops` for the ACES 2 op data.
pub(crate) use ocio_ops::transforms::builtins::color_matrix_helpers;

mod aces;
mod apple_cameras;
mod arri_cameras;
mod canon_cameras;
mod displays;
mod panasonic_cameras;
mod red_cameras;
mod sony_cameras;
