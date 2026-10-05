// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The RGB to XYZ matrices of ACES 2.0, from the color matrix helpers, in `float`: a port of
//! `src/OpenColorIO/ops/fixedfunction/ACES2/ColorLib.h` @ v2.5.2.

use super::matrix_lib::{M33f, m33_from_ocio_matrix_array, mult_f33_f33};
use crate::exception::Result;
use crate::transforms::builtins::color_matrix_helpers::{
    AdaptationMethod, Primaries, build_conversion_matrix, cie_xyz_illum_e,
};

/// The matrix from the RGB of `c` to CIE XYZ (equal-energy white, no adaptation), narrowed to
/// `float`.
///
/// Port of `RGBtoXYZ_f33` (src/OpenColorIO/ops/fixedfunction/ACES2/ColorLib.h:17-22 @ v2.5.2).
pub fn rgb_to_xyz_f33(c: &Primaries) -> Result<M33f> {
    Ok(m33_from_ocio_matrix_array(&build_conversion_matrix(
        c,
        &cie_xyz_illum_e::PRIMARIES,
        AdaptationMethod::None,
    )?))
}

/// The inverse of [`rgb_to_xyz_f33`]'s matrix, inverted in `double` before the narrowing.
///
/// Port of `XYZtoRGB_f33` (src/OpenColorIO/ops/fixedfunction/ACES2/ColorLib.h:24-29 @ v2.5.2).
pub fn xyz_to_rgb_f33(c: &Primaries) -> Result<M33f> {
    Ok(m33_from_ocio_matrix_array(
        &build_conversion_matrix(c, &cie_xyz_illum_e::PRIMARIES, AdaptationMethod::None)?
            .inverse()?,
    ))
}

/// The matrix from the RGB of `csrc` to the RGB of `cdst`, as the product of the two `float`
/// matrices.
///
/// Port of `RGBtoRGB_f33` (src/OpenColorIO/ops/fixedfunction/ACES2/ColorLib.h:31-34 @ v2.5.2).
pub fn rgb_to_rgb_f33(csrc: &Primaries, cdst: &Primaries) -> Result<M33f> {
    Ok(mult_f33_f33(&xyz_to_rgb_f33(cdst)?, &rgb_to_xyz_f33(csrc)?))
}

/// `Identity_M33`.
#[rustfmt::skip]
pub const IDENTITY_M33: M33f = [
    1., 0., 0.,
    0., 1., 0.,
    0., 0., 1.,
];
