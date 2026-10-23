// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingHueCurve op family: a port of `src/OpenColorIO/ops/gradinghuecurve/` @ v2.5.2.
//! Its curves (5.0c), the op data and the op (5.4a), the renderers (5.4b); the GPU shader comes
//! with 5.4c, the transform is in `ocio`
//! (crates/ocio/src/transforms/grading_hue_curve_transform.rs).

pub mod grading_hue_curve;
pub mod grading_hue_curve_op;
pub mod grading_hue_curve_op_cpu;
pub mod grading_hue_curve_op_data;
