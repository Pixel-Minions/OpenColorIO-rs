// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms of Apple cameras: a port of
//! `src/OpenColorIO/transforms/builtins/AppleCameras.cpp` @ v2.6.0, each entry's style and
//! description in upstream's order. Apple Log is a half-domain 1D LUT (the wheel is built with
//! `OCIO_LUT_SUPPORT`), Rec.2020 or Apple Wide Gamut to ACES a Bradford matrix.

use std::sync::Arc;

use ocio_ops::exception::Result;
use ocio_ops::math_utils::std_pow;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op_from_array;

use crate::transforms::builtins::builtin_transform_registry::BuiltinTransformRegistry;
use crate::transforms::builtins::color_matrix_helpers::{
    AdaptationMethod, Chromaticities, Primaries, aces_ap0, build_conversion_matrix, rec2020,
};
use crate::transforms::builtins::op_helpers::create_half_lut;

/// Apple Wide Gamut primaries from the Apple Log 2 white paper.
///
/// Port of `APPLE_WIDE_GAMUT` (AppleCameras.cpp:27-36 @ v2.6.0).
const APPLE_WIDE_GAMUT: Primaries = Primaries::new(
    Chromaticities::new(0.725, 0.301),
    Chromaticities::new(0.221, 0.814),
    Chromaticities::new(0.068, -0.076),
    Chromaticities::new(0.3127, 0.3290),
);

/// Apple Log to linear, as a half-domain 1D LUT.
///
/// Port of `APPLE_LOG::GenerateAppleLogToLinearOps` (AppleCameras.cpp:41-97 @ v2.6.0).
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
            (std_pow(2.0, (in_ - DELTA) / GAMMA) - BETA) as f32
        } else if in_ < p_t && in_ >= 0.0 {
            ((in_ / C).sqrt() + R_0) as f32
        } else {
            R_0 as f32
        }
    };

    create_half_lut(ops, generate_lut_values)
}

/// Registers the 3 built-in transforms of Apple cameras.
///
/// Port of `CAMERA::APPLE::RegisterAll` (AppleCameras.cpp:107-149 @ v2.6.0).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // AppleCameras.cpp:119
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
    // AppleCameras.cpp:135: Apple Log 2 uses the same transfer function as Apple Log, but
    // with the wider Apple Wide Gamut primaries rather than Rec.2020.
    registry.add_builtin(
        b"APPLE_LOG-APPLEWG_to_ACES2065-1",
        Some(b"Convert Apple Log 2 Apple Wide Gamut to ACES2065-1"),
        Arc::new(|ops: &mut OpVec| {
            apple_log_to_linear(ops)?;

            let matrix = build_conversion_matrix(
                &APPLE_WIDE_GAMUT,
                &aces_ap0::PRIMARIES,
                AdaptationMethod::Bradford,
            )?;
            create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);
            Ok(())
        }),
    );
    // AppleCameras.cpp:145
    registry.add_builtin(
        b"CURVE - APPLE_LOG_to_LINEAR",
        Some(b"Convert Apple Log to linear"),
        Arc::new(apple_log_to_linear),
    );
}
