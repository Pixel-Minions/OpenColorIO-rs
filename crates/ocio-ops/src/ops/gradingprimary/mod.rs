// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingPrimary op family: a port of `src/OpenColorIO/ops/gradingprimary/` @ v2.5.2.
//! Its values and their pre-render form (5.0a), the op data and the op (5.2a), the renderers
//! (5.2b, 5.2c); the GPU shader comes with 5.2d, the transform is in `ocio`
//! (crates/ocio/src/transforms/grading_primary_transform.rs).

pub mod grading_primary;
pub mod grading_primary_op;
pub mod grading_primary_op_cpu;
#[cfg(target_arch = "x86_64")]
mod grading_primary_op_cpu_sse2;
pub mod grading_primary_op_data;
