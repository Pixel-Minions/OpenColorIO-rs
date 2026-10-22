// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingTone op family: a port of `src/OpenColorIO/ops/gradingtone/` @ v2.5.2. Its values
//! and their pre-render form (5.0b), the op data and the op (5.3a), the log and video styles'
//! renderers (5.3b); the linear style's renderers come with 5.3c, the GPU shader with 5.3d and
//! 5.3e, the transform is in `ocio`
//! (crates/ocio/src/transforms/grading_tone_transform.rs).

pub mod grading_tone;
pub mod grading_tone_op;
pub mod grading_tone_op_cpu;
pub mod grading_tone_op_data;
