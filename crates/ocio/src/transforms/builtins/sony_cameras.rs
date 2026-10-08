// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms of Sony cameras: a port of
//! `src/OpenColorIO/transforms/builtins/SonyCameras.cpp` @ v2.5.2: the S-Log3 curve (a Log op)
//! then each gamut to ACES AP0 (a Matrix op, from primaries or, for the Venice, as given).

use std::sync::Arc;

use ocio_ops::exception::Result;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::log::log_op::create_log_op;
use ocio_ops::ops::log::log_op_data::LogOpData;
use ocio_ops::ops::matrix::matrix_op::{create_matrix_op_from_array, create_matrix_op_from_m44};

use crate::transforms::builtins::builtin_transform_registry::BuiltinTransformRegistry;
use crate::transforms::builtins::color_matrix_helpers::{
    AdaptationMethod, Chromaticities, Primaries, aces_ap0, build_conversion_matrix,
};

/// Sony S-Gamut3.
///
/// Port of `SONY_SGAMUT3` (SonyCameras.cpp:20-28 @ v2.5.2).
const SONY_SGAMUT3: Primaries = Primaries::new(
    Chromaticities::new(0.730, 0.280),
    Chromaticities::new(0.140, 0.855),
    Chromaticities::new(0.100, -0.050),
    Chromaticities::new(0.3127, 0.3290),
);

/// Sony S-Gamut3.Cine.
///
/// Port of `SONY_SGAMUT3_CINE` (SonyCameras.cpp:30-38 @ v2.5.2).
const SONY_SGAMUT3_CINE: Primaries = Primaries::new(
    Chromaticities::new(0.766, 0.275),
    Chromaticities::new(0.225, 0.800),
    Chromaticities::new(0.089, -0.087),
    Chromaticities::new(0.3127, 0.3290),
);

/// S-Log3 to linear: an inverse camera log with a linear break and a linear slope.
///
/// Port of `SONY_SLOG3_to_LINEAR` (SonyCameras.cpp:41-55 @ v2.5.2).
fn sony_slog3_to_linear() -> Result<LogOpData> {
    const LIN_SIDE_SLOPE: f64 = 1. / (0.18 + 0.01);
    const LIN_SIDE_OFFSET: f64 = 0.01 / (0.18 + 0.01);
    const LOG_SIDE_SLOPE: f64 = 261.5 / 1023.;
    const LOG_SIDE_OFFSET: f64 = 420. / 1023.;
    const LIN_SIDE_BREAK: f64 = 0.01125000;
    const LINEAR_SLOPE: f64 = ((171.2102946929 - 95.) / 0.01125000) / 1023.;
    const BASE: f64 = 10.;

    let params = vec![
        LOG_SIDE_SLOPE,
        LOG_SIDE_OFFSET,
        LIN_SIDE_SLOPE,
        LIN_SIDE_OFFSET,
        LIN_SIDE_BREAK,
        LINEAR_SLOPE,
    ];
    LogOpData::from_channel_params(
        BASE,
        params.clone(),
        params.clone(),
        params,
        TransformDirection::Inverse,
    )
}

/// Appends S-Log3 to linear, then `primaries` to ACES AP0 with the CAT02 adaptation.
fn slog3_then(ops: &mut OpVec, primaries: &Primaries) -> Result<()> {
    create_log_op(ops, sony_slog3_to_linear()?, TransformDirection::Forward)?;

    let matrix = build_conversion_matrix(primaries, &aces_ap0::PRIMARIES, AdaptationMethod::Cat02)?;
    create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);
    Ok(())
}

/// Appends S-Log3 to linear, then the matrix `m44`: for the Venice, upstream was given the
/// matrix only, not primaries.
fn slog3_then_matrix(ops: &mut OpVec, m44: &[f64; 16]) -> Result<()> {
    create_log_op(ops, sony_slog3_to_linear()?, TransformDirection::Forward)?;

    create_matrix_op_from_m44(ops, m44, TransformDirection::Forward);
    Ok(())
}

/// Registers the 4 built-in transforms of Sony cameras.
///
/// Port of `CAMERA::SONY::RegisterAll` (SonyCameras.cpp:64-143 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // SonyCameras.cpp:66-80
    registry.add_builtin(
        b"SONY_SLOG3-SGAMUT3_to_ACES2065-1",
        Some(b"Convert Sony S-Log3 S-Gamut3 to ACES2065-1"),
        Arc::new(|ops: &mut OpVec| slog3_then(ops, &SONY_SGAMUT3)),
    );
    // SonyCameras.cpp:82-96
    registry.add_builtin(
        b"SONY_SLOG3-SGAMUT3.CINE_to_ACES2065-1",
        Some(b"Convert Sony S-Log3 S-Gamut3.Cine to ACES2065-1"),
        Arc::new(|ops: &mut OpVec| slog3_then(ops, &SONY_SGAMUT3_CINE)),
    );
    // SonyCameras.cpp:98-119
    registry.add_builtin(
        b"SONY_SLOG3-SGAMUT3-VENICE_to_ACES2065-1",
        Some(b"Convert Sony S-Log3 S-Gamut3 for the Venice camera to ACES2065-1"),
        Arc::new(|ops: &mut OpVec| {
            // Note that in CTL, the matrices are stored transposed.
            const SGAMUT3_VENICE: [f64; 16] = [
                0.7933297411,
                0.0890786256,
                0.1175916333,
                0., //
                0.0155810585,
                1.0327123069,
                -0.0482933654,
                0., //
                -0.0188647478,
                0.0127694121,
                1.0060953358,
                0., //
                0.,
                0.,
                0.,
                1.,
            ];
            slog3_then_matrix(ops, &SGAMUT3_VENICE)
        }),
    );
    // SonyCameras.cpp:121-142
    registry.add_builtin(
        b"SONY_SLOG3-SGAMUT3.CINE-VENICE_to_ACES2065-1",
        Some(b"Convert Sony S-Log3 S-Gamut3.Cine for the Venice camera to ACES2065-1"),
        Arc::new(|ops: &mut OpVec| {
            // Note that in CTL, the matrices are stored transposed.
            const SGAMUT3_CINE_VENICE: [f64; 16] = [
                0.6742570921,
                0.2205717359,
                0.1051711720,
                0., //
                -0.0093136061,
                1.1059588614,
                -0.0966452553,
                0., //
                -0.0382090673,
                -0.0179383766,
                1.0561474439,
                0., //
                0.,
                0.,
                0.,
                1.,
            ];
            slog3_then_matrix(ops, &SGAMUT3_CINE_VENICE)
        }),
    );
}
