// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms of ARRI cameras: a port of
//! `src/OpenColorIO/transforms/builtins/ArriCameras.cpp` @ v2.5.2: each camera's log curve
//! (a Log op) then its primaries to ACES AP0 (a Matrix op).

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

/// ARRI ALEXA Wide Gamut.
///
/// Port of `ARRI_ALEXA_WIDE_GAMUT` (ArriCameras.cpp:18-26 @ v2.5.2).
const ARRI_ALEXA_WIDE_GAMUT: Primaries = Primaries::new(
    Chromaticities::new(0.68400, 0.31300),
    Chromaticities::new(0.22100, 0.84800),
    Chromaticities::new(0.08610, -0.10200),
    Chromaticities::new(0.31270, 0.32900),
);

/// ARRI Wide Gamut 4.
///
/// Port of `ARRI_WIDE_GAMUT_4` (ArriCameras.cpp:28-36 @ v2.5.2).
const ARRI_WIDE_GAMUT_4: Primaries = Primaries::new(
    Chromaticities::new(0.73470, 0.26530),
    Chromaticities::new(0.14240, 0.85760),
    Chromaticities::new(0.09910, -0.03080),
    Chromaticities::new(0.31270, 0.32900),
);

/// ARRI ALEXA LogC (EI800) to linear: an inverse camera log with a linear break.
///
/// Port of `ARRI_ALEXA_LOGC_EI800_to_LINEAR` (ArriCameras.cpp:38-51 @ v2.5.2).
fn arri_alexa_logc_ei800_to_linear() -> Result<LogOpData> {
    const LIN_SIDE_SLOPE: f64 = 1. / (0.18 * 0.005 * (800. / 400.) / 0.01);
    const LIN_SIDE_OFFSET: f64 = 0.0522722750;
    const LOG_SIDE_SLOPE: f64 = 0.2471896383;
    const LOG_SIDE_OFFSET: f64 = 0.3855369987;
    const LIN_SIDE_BREAK: f64 = ((1. / 9.) - LIN_SIDE_OFFSET) / LIN_SIDE_SLOPE;
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

/// ARRI LogC4 to linear: an inverse camera log with a linear break.
///
/// Port of `ARRI_LOGC4_to_LINEAR` (ArriCameras.cpp:53-66 @ v2.5.2).
fn arri_logc4_to_linear() -> Result<LogOpData> {
    const LIN_SIDE_SLOPE: f64 = 2231.82630906769;
    const LIN_SIDE_OFFSET: f64 = 64.0;
    const LOG_SIDE_SLOPE: f64 = 0.0647954196341293;
    const LOG_SIDE_OFFSET: f64 = -0.295908392682586;
    const LIN_SIDE_BREAK: f64 = -0.0180569961199113;
    const BASE: f64 = 2.;

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

/// Registers the 2 built-in transforms of ARRI cameras.
///
/// Port of `CAMERA::ARRI::RegisterAll` (ArriCameras.cpp:75-108 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // ArriCameras.cpp:77-91
    registry.add_builtin(
        b"ARRI_ALEXA-LOGC-EI800-AWG_to_ACES2065-1",
        Some(b"Convert ARRI ALEXA LogC (EI800) ALEXA Wide Gamut to ACES2065-1"),
        Arc::new(|ops: &mut OpVec| {
            create_log_op(
                ops,
                arri_alexa_logc_ei800_to_linear()?,
                TransformDirection::Forward,
            )?;

            let matrix = build_conversion_matrix(
                &ARRI_ALEXA_WIDE_GAMUT,
                &aces_ap0::PRIMARIES,
                AdaptationMethod::Cat02,
            )?;
            create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);
            Ok(())
        }),
    );
    // ArriCameras.cpp:93-107
    registry.add_builtin(
        b"ARRI_LOGC4_to_ACES2065-1",
        Some(b"Convert ARRI LogC4 to ACES2065-1"),
        Arc::new(|ops: &mut OpVec| {
            create_log_op(ops, arri_logc4_to_linear()?, TransformDirection::Forward)?;

            let matrix = build_conversion_matrix(
                &ARRI_WIDE_GAMUT_4,
                &aces_ap0::PRIMARIES,
                AdaptationMethod::Cat02,
            )?;
            create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);
            Ok(())
        }),
    );
}
