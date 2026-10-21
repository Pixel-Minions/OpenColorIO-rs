// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The ExposureContrast op family: a port of `src/OpenColorIO/ops/exposurecontrast/` @ v2.5.2.
//! The GPU shader comes with 5.1c, the transform with 5.5a.

pub mod exposure_contrast_op;
pub mod exposure_contrast_op_cpu;
pub mod exposure_contrast_op_data;
