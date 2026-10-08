// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms of Canon cameras: a port of
//! `src/OpenColorIO/transforms/builtins/CanonCameras.cpp` @ v2.5.2, each entry's style and
//! description in upstream's order. Canon Log 2 and 3 are 4096-entry 1D LUTs (the wheel is
//! built with `OCIO_LUT_SUPPORT`), Cinema Gamut to ACES a CAT02 matrix.

use std::sync::Arc;

use ocio_ops::exception::Result;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op_from_array;

use crate::transforms::builtins::builtin_transform_registry::{
    BuiltinTransformRegistry, OpCreator,
};
use crate::transforms::builtins::color_matrix_helpers::{
    AdaptationMethod, Chromaticities, Primaries, aces_ap0, build_conversion_matrix,
};
use crate::transforms::builtins::op_helpers::create_lut;

/// Canon Cinema Gamut.
///
/// Port of `CANON_CGAMUT::primaries` (CanonCameras.cpp:26-33 @ v2.5.2).
const CANON_CGAMUT: Primaries = Primaries::new(
    Chromaticities::new(0.7400, 0.2700),
    Chromaticities::new(0.1700, 1.1400),
    Chromaticities::new(0.0800, -0.1000),
    Chromaticities::new(0.3127, 0.3290),
);

/// `std::pow(10, x)`: the `int` promoted to `double`.
fn pow10(x: f64) -> f64 {
    10f64.powf(x)
}

/// Canon Log 2 to linear, as a 1D LUT of 4096 entries.
///
/// Port of `CANON_CLOG2::GenerateOpsToLinear` (CanonCameras.cpp:38-59 @ v2.5.2).
fn clog2_to_linear(ops: &mut OpVec) -> Result<()> {
    let generate_lut_values = |in_: f64| -> f32 {
        let out = if in_ < 0.092864125 {
            -(pow10((0.092864125 - in_) / 0.24136077) - 1.) / 87.099375
        } else {
            (pow10((in_ - 0.092864125) / 0.24136077) - 1.) / 87.099375
        };

        (out * 0.9) as f32
    };

    create_lut(ops, 4096, generate_lut_values)
}

/// Canon Log 3 to linear, as a 1D LUT of 4096 entries.
///
/// Port of `CANON_CLOG3::GenerateOpsToLinear` (CanonCameras.cpp:86-113 @ v2.5.2).
fn clog3_to_linear(ops: &mut OpVec) -> Result<()> {
    let generate_lut_values = |in_: f64| -> f32 {
        let out = if in_ < 0.097465473 {
            -(pow10((0.12783901 - in_) / 0.36726845) - 1.) / 14.98325
        } else if in_ <= 0.15277891 {
            (in_ - 0.12512219) / 1.9754798
        } else {
            (pow10((in_ - 0.12240537) / 0.36726845) - 1.) / 14.98325
        };

        (out * 0.9) as f32
    };

    create_lut(ops, 4096, generate_lut_values)
}

/// A Canon Log curve, then Cinema Gamut to ACES AP0 with CAT02 adaptation.
///
/// Port of `CANON_CLOG2_CGAMUT_to_ACES2065_1_Functor` and
/// `CANON_CLOG3_CGAMUT_to_ACES2065_1_Functor` (CanonCameras.cpp:150-157, 175-182 @ v2.5.2).
fn clog_then_cgamut(ops: &mut OpVec, to_linear: fn(&mut OpVec) -> Result<()>) -> Result<()> {
    to_linear(ops)?;

    let matrix =
        build_conversion_matrix(&CANON_CGAMUT, &aces_ap0::PRIMARIES, AdaptationMethod::Cat02)?;
    create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);
    Ok(())
}

/// An entry's op creator.
fn creator(f: fn(&mut OpVec) -> Result<()>) -> OpCreator {
    Arc::new(f)
}

/// Registers the 4 built-in transforms of Canon cameras.
///
/// Port of `CAMERA::CANON::RegisterAll` (CanonCameras.cpp:146-197 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // CanonCameras.cpp:158
    registry.add_builtin(
        b"CANON_CLOG2-CGAMUT_to_ACES2065-1",
        Some(b"Convert Canon Log 2 Cinema Gamut to ACES2065-1"),
        Arc::new(|ops: &mut OpVec| clog_then_cgamut(ops, clog2_to_linear)),
    );
    // CanonCameras.cpp:168
    registry.add_builtin(
        b"CURVE - CANON_CLOG2_to_LINEAR",
        Some(b"Convert Canon Log 2 to linear"),
        creator(clog2_to_linear),
    );
    // CanonCameras.cpp:183
    registry.add_builtin(
        b"CANON_CLOG3-CGAMUT_to_ACES2065-1",
        Some(b"Convert Canon Log 3 Cinema Gamut to ACES2065-1"),
        Arc::new(|ops: &mut OpVec| clog_then_cgamut(ops, clog3_to_linear)),
    );
    // CanonCameras.cpp:193
    registry.add_builtin(
        b"CURVE - CANON_CLOG3_to_LINEAR",
        Some(b"Convert Canon Log 3 to linear"),
        creator(clog3_to_linear),
    );
}
