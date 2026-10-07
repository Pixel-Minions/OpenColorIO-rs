// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transform of Panasonic cameras: a port of
//! `src/OpenColorIO/transforms/builtins/PanasonicCameras.cpp` @ v2.5.2: the V-Log curve (a Log
//! op) then V-Gamut to ACES AP0 (a Matrix op).

use std::sync::Arc;

use ocio_ops::exception::Result;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::log::log_op::create_log_op;
use ocio_ops::ops::log::log_op_data::LogOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op_from_array;

use crate::transforms::builtins::builtin_transform_registry::BuiltinTransformRegistry;
use crate::transforms::builtins::color_matrix_helpers::{
    AdaptationMethod, Chromaticities, Primaries, aces_ap0, build_conversion_matrix,
};

/// Panasonic V-Gamut.
///
/// Port of `PANASONIC_VLOG_VGAMUT` (PanasonicCameras.cpp:21-29 @ v2.5.2).
const PANASONIC_VLOG_VGAMUT: Primaries = Primaries::new(
    Chromaticities::new(0.730, 0.280),
    Chromaticities::new(0.165, 0.840),
    Chromaticities::new(0.100, -0.030),
    Chromaticities::new(0.3127, 0.3290),
);

/// V-Log to linear: an inverse camera log with a linear break.
///
/// Port of `PANASONIC_VLOG_VGAMUT_to_LINEAR` (PanasonicCameras.cpp:31-49 @ v2.5.2).
fn panasonic_vlog_vgamut_to_linear() -> Result<LogOpData> {
    const CUT1: f64 = 0.01;
    const B: f64 = 0.00873;
    const C: f64 = 0.241514;
    const D: f64 = 0.598206;

    const LIN_SIDE_SLOPE: f64 = 1.;
    const LIN_SIDE_OFFSET: f64 = B;
    const LOG_SIDE_SLOPE: f64 = C;
    const LOG_SIDE_OFFSET: f64 = D;
    const LIN_SIDE_BREAK: f64 = CUT1;
    const BASE: f64 = 10.;

    let params = vec![
        LOG_SIDE_SLOPE,
        LOG_SIDE_OFFSET,
        LIN_SIDE_SLOPE,
        LIN_SIDE_OFFSET,
        LIN_SIDE_BREAK,
    ];
    LogOpData::from_channel_params(
        BASE,
        params.clone(),
        params.clone(),
        params,
        TransformDirection::Inverse,
    )
}

/// Registers the built-in transform of Panasonic cameras.
///
/// Port of `CAMERA::PANASONIC::RegisterAll` (PanasonicCameras.cpp:58-75 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // PanasonicCameras.cpp:60-74
    registry.add_builtin(
        b"PANASONIC_VLOG-VGAMUT_to_ACES2065-1",
        Some(b"Convert Panasonic Varicam V-Log V-Gamut to ACES2065-1"),
        Arc::new(|ops: &mut OpVec| {
            create_log_op(
                ops,
                panasonic_vlog_vgamut_to_linear()?,
                TransformDirection::Forward,
            )?;

            let matrix = build_conversion_matrix(
                &PANASONIC_VLOG_VGAMUT,
                &aces_ap0::PRIMARIES,
                AdaptationMethod::Bradford,
            )?;
            create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);
            Ok(())
        }),
    );
}
