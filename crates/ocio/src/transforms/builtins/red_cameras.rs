// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms of RED cameras: a port of
//! `src/OpenColorIO/transforms/builtins/RedCameras.cpp` @ v2.5.2: each camera's log curve (a
//! Log op) then RED Wide Gamut RGB to ACES AP0 (a Matrix op).

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

/// RED Wide Gamut RGB.
///
/// Port of `RED_WIDE_GAMUT_RGB` (RedCameras.cpp:20-28 @ v2.5.2).
const RED_WIDE_GAMUT_RGB: Primaries = Primaries::new(
    Chromaticities::new(0.780308, 0.304253),
    Chromaticities::new(0.121595, 1.493994),
    Chromaticities::new(0.095612, -0.084589),
    Chromaticities::new(0.3127, 0.3290),
);

/// RED LogFilm to linear: an inverse log without a linear break. Its gain is a `static const`
/// upstream computes with `std::pow` (the C runtime's `pow`, as `f64::powf` calls it).
///
/// Port of `RED_REDLOGFILM_RWG_to_LINEAR` (RedCameras.cpp:30-53 @ v2.5.2).
fn red_redlogfilm_rwg_to_linear() -> Result<LogOpData> {
    const REF_WHITE: f64 = 685. / 1023.;
    const REF_BLACK: f64 = 95. / 1023.;
    const RANGE: f64 = 0.002 * 1023.;
    const GAMMA: f64 = 0.6;
    const HIGHLIGHT: f64 = 1.;
    const SHADOW: f64 = 0.;
    const MULTI_FACTOR: f64 = RANGE / GAMMA;
    let gain = (HIGHLIGHT - SHADOW) / (1. - 10f64.powf(MULTI_FACTOR * (REF_BLACK - REF_WHITE)));
    let offset = gain - (HIGHLIGHT - SHADOW);
    const LOG_SIDE_SLOPE: f64 = 1. / MULTI_FACTOR;
    const LOG_SIDE_OFFSET: f64 = REF_WHITE;
    let lin_side_slope = 1. / gain;
    let lin_side_offset = (offset - SHADOW) / gain;
    const BASE: f64 = 10.;

    let params = vec![
        LOG_SIDE_SLOPE,
        LOG_SIDE_OFFSET,
        lin_side_slope,
        lin_side_offset,
    ];
    LogOpData::from_channel_params(
        BASE,
        params.clone(),
        params.clone(),
        params,
        TransformDirection::Inverse,
    )
}

/// RED Log3G10 to linear: an inverse camera log with a linear break.
///
/// Port of `RED_LOG3G10_RWG_to_LINEAR` (RedCameras.cpp:55-68 @ v2.5.2).
fn red_log3g10_rwg_to_linear() -> Result<LogOpData> {
    const LIN_SIDE_SLOPE: f64 = 155.975327;
    const LIN_SIDE_OFFSET: f64 = 0.01 * LIN_SIDE_SLOPE + 1.0;
    const LOG_SIDE_SLOPE: f64 = 0.224282;
    const LOG_SIDE_OFFSET: f64 = 0.0;
    const LIN_SIDE_BREAK: f64 = -0.01;
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

/// Appends a camera's log op, then RED Wide Gamut RGB to ACES AP0 with the Bradford adaptation.
fn log_then_rwg(ops: &mut OpVec, log: LogOpData) -> Result<()> {
    create_log_op(ops, log, TransformDirection::Forward)?;

    let rwg = build_conversion_matrix(
        &RED_WIDE_GAMUT_RGB,
        &aces_ap0::PRIMARIES,
        AdaptationMethod::Bradford,
    )?;
    create_matrix_op_from_array(ops, &rwg, TransformDirection::Forward);
    Ok(())
}

/// Registers the 2 built-in transforms of RED cameras.
///
/// Port of `CAMERA::RED::RegisterAll` (RedCameras.cpp:77-113 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // RedCameras.cpp:79-95
    registry.add_builtin(
        b"RED_REDLOGFILM-RWG_to_ACES2065-1",
        Some(b"Convert RED LogFilm RED Wide Gamut to ACES2065-1"),
        Arc::new(|ops: &mut OpVec| log_then_rwg(ops, red_redlogfilm_rwg_to_linear()?)),
    );
    // RedCameras.cpp:96-112
    registry.add_builtin(
        b"RED_LOG3G10-RWG_to_ACES2065-1",
        Some(b"Convert RED Log3G10 RED Wide Gamut to ACES2065-1"),
        Arc::new(|ops: &mut OpVec| log_then_rwg(ops, red_log3g10_rwg_to_linear()?)),
    );
}
