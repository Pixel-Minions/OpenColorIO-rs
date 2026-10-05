// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms: a port of `src/OpenColorIO/transforms/builtins/` @ v2.5.2. So far
//! the registry and every entry's style and description; the ops come with WP 3.2e-g and
//! `p3-after-p2`.

pub mod builtin_transform_registry;

mod aces;
mod apple_cameras;
mod arri_cameras;
mod canon_cameras;
mod displays;
mod panasonic_cameras;
mod red_cameras;
mod sony_cameras;
