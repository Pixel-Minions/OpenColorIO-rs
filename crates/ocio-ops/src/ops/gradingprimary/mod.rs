// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingPrimary op family: a port of `src/OpenColorIO/ops/gradingprimary/` @ v2.5.2.
//! Its values and their pre-render form (5.0a), the op data and the op (5.2a); the renderers
//! come with 5.2b and 5.2c, the GPU shader with 5.2d, the transform with 5.5b.

pub mod grading_primary;
pub mod grading_primary_op;
pub mod grading_primary_op_data;
