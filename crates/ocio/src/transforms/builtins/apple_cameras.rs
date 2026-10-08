// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms of Apple cameras: a port of
//! `src/OpenColorIO/transforms/builtins/AppleCameras.cpp` @ v2.5.2, each entry's style and
//! description in upstream's order. Apple Log is a half-domain 1D LUT (the wheel is built with
//! `OCIO_LUT_SUPPORT`), Rec.2020 to ACES a Bradford matrix.

use std::sync::Arc;

use ocio_ops::exception::Result;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op_from_array;

use crate::transforms::builtins::builtin_transform_registry::BuiltinTransformRegistry;
use crate::transforms::builtins::color_matrix_helpers::{
    AdaptationMethod, aces_ap0, build_conversion_matrix, rec2020,
};
use crate::transforms::builtins::op_helpers::create_half_lut;

/// Apple Log to linear, as a half-domain 1D LUT.
///
/// Port of `APPLE_LOG::GenerateAppleLogToLinearOps` (AppleCameras.cpp:30-58 @ v2.5.2).
fn apple_log_to_linear(ops: &mut OpVec) -> Result<()> {
    const R_0: f64 = -0.05641088;
    const R_T: f64 = 0.01;
    const C: f64 = 47.28711236;
    const BETA: f64 = 0.00964052;
    const GAMMA: f64 = 0.08550479;
    const DELTA: f64 = 0.69336945;

    let p_t = C * (R_T - R_0).powf(2.0);
    let generate_lut_values = |in_: f64| -> f32 {
        if in_ >= p_t {
            (2.0f64.powf((in_ - DELTA) / GAMMA) - BETA) as f32
        } else if in_ < p_t && in_ >= 0.0 {
            ((in_ / C).sqrt() + R_0) as f32
        } else {
            R_0 as f32
        }
    };

    create_half_lut(ops, generate_lut_values)
}

/// Registers the 2 built-in transforms of Apple cameras.
///
/// Port of `CAMERA::APPLE::RegisterAll` (AppleCameras.cpp:96-122 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // AppleCameras.cpp:108
    registry.add_builtin(
        b"APPLE_LOG_to_ACES2065-1",
        Some(b"Convert Apple Log to ACES2065-1"),
        Arc::new(|ops: &mut OpVec| {
            apple_log_to_linear(ops)?;

            let matrix = build_conversion_matrix(
                &rec2020::PRIMARIES,
                &aces_ap0::PRIMARIES,
                AdaptationMethod::Bradford,
            )?;
            create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);
            Ok(())
        }),
    );
    // AppleCameras.cpp:118
    registry.add_builtin(
        b"CURVE - APPLE_LOG_to_LINEAR",
        Some(b"Convert Apple Log to linear"),
        Arc::new(apple_log_to_linear),
    );
}
