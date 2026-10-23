// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The ExposureContrast op family: a port of `src/OpenColorIO/ops/exposurecontrast/` @ v2.5.2.
//! The GPU shader is in `ocio-gpu`, the transform in `ocio`
//! (crates/ocio/src/transforms/exposure_contrast_transform.rs).

pub mod exposure_contrast_op;
pub mod exposure_contrast_op_cpu;
pub mod exposure_contrast_op_data;
