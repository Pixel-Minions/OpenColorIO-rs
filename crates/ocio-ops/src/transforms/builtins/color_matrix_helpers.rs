// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Chromaticities, primaries and the matrices between RGB spaces: a port of
//! `src/OpenColorIO/transforms/builtins/ColorMatrixHelpers.cpp/.h` @ v2.5.2.
//!
//! Upstream keeps these with the built-in transforms, but the ACES 2.0 fixed functions use
//! them too (`ops/fixedfunction/ACES2/ColorLib.h`), so the port puts them in `ocio-ops`,
//! below both; `ocio` re-exports them for the built-in transforms.
//!
//! Everything is computed in `double`, in the source's order. The matrix products and the
//! inverse are `MatrixArray`'s ([`MatrixArray::inner`], [`MatrixArray::inverse`]).

use crate::exception::Result;
use crate::math_utils::{sse_add, sse_mul};
use crate::ops::matrix::matrix_op_data::{MatrixArray, Offsets};

/// CIE xy chromaticity coordinates.
///
/// Port of `Chromaticities` (src/OpenColorIO/transforms/builtins/ColorMatrixHelpers.h:17-45 @
/// v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chromaticities {
    /// `m_xy`.
    pub xy: [f64; 2],
}

impl Chromaticities {
    /// Port of `Chromaticities::Chromaticities(double, double)` (ColorMatrixHelpers.h:22-26 @
    /// v2.5.2).
    pub const fn new(x: f64, y: f64) -> Self {
        Chromaticities { xy: [x, y] }
    }
}

/// The chromaticities of a set of RGB primaries and of its white.
///
/// Port of `Primaries` (src/OpenColorIO/transforms/builtins/ColorMatrixHelpers.h:47-83 @
/// v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Primaries {
    /// `m_red`: CIE xy chromaticity coordinates for red primary.
    pub red: Chromaticities,
    /// `m_grn`: CIE xy chromaticity coordinates for green primary.
    pub grn: Chromaticities,
    /// `m_blu`: CIE xy chromaticity coordinates for blue primary.
    pub blu: Chromaticities,
    /// `m_wht`: CIE xy chromaticities for white (or gray).
    pub wht: Chromaticities,
}

impl Primaries {
    /// Port of `Primaries::Primaries(red, grn, blu, wht)` (ColorMatrixHelpers.h:52-58 @
    /// v2.5.2).
    pub const fn new(
        red: Chromaticities,
        grn: Chromaticities,
        blu: Chromaticities,
        wht: Chromaticities,
    ) -> Self {
        Primaries { red, grn, blu, wht }
    }
}

/// `const Primaries primaries(red_xy, grn_xy, blu_xy, wht_xy)` from four chromaticities.
const fn primaries(red: [f64; 2], grn: [f64; 2], blu: [f64; 2], wht: [f64; 2]) -> Primaries {
    Primaries::new(
        Chromaticities::new(red[0], red[1]),
        Chromaticities::new(grn[0], grn[1]),
        Chromaticities::new(blu[0], blu[1]),
        Chromaticities::new(wht[0], wht[1]),
    )
}

/// The CIE XYZ space with an equal-energy white.
///
/// Port of `CIE_XYZ_ILLUM_E` (ColorMatrixHelpers.cpp:13-21 @ v2.5.2).
pub mod cie_xyz_illum_e {
    use super::{Primaries, primaries};
    /// `CIE_XYZ_ILLUM_E::primaries`.
    pub const PRIMARIES: Primaries = primaries([1., 0.], [0., 1.], [0., 0.], [1. / 3., 1. / 3.]);
}

/// Port of `ACES_AP0` (ColorMatrixHelpers.cpp:24-32 @ v2.5.2).
pub mod aces_ap0 {
    use super::{Primaries, primaries};
    /// `ACES_AP0::primaries`.
    pub const PRIMARIES: Primaries = primaries(
        [0.7347, 0.2653],
        [0.0000, 1.0000],
        [0.0001, -0.0770],
        [0.32168, 0.33767],
    );
}

/// Port of `ACES_AP1` (ColorMatrixHelpers.cpp:34-42 @ v2.5.2).
pub mod aces_ap1 {
    use super::{Primaries, primaries};
    /// `ACES_AP1::primaries`.
    pub const PRIMARIES: Primaries = primaries(
        [0.713, 0.293],
        [0.165, 0.830],
        [0.128, 0.044],
        [0.32168, 0.33767],
    );
}

/// Port of `REC709` (ColorMatrixHelpers.cpp:44-52 @ v2.5.2).
pub mod rec709 {
    use super::{Primaries, primaries};
    /// `REC709::primaries`.
    pub const PRIMARIES: Primaries =
        primaries([0.64, 0.33], [0.30, 0.60], [0.15, 0.06], [0.3127, 0.3290]);
}

/// Port of `REC709_D60` (ColorMatrixHelpers.cpp:54-62 @ v2.5.2).
pub mod rec709_d60 {
    use super::{Primaries, primaries};
    /// `REC709_D60::primaries`.
    pub const PRIMARIES: Primaries =
        primaries([0.64, 0.33], [0.30, 0.60], [0.15, 0.06], [0.32168, 0.33767]);
}

/// Port of `REC2020` (ColorMatrixHelpers.cpp:64-72 @ v2.5.2).
pub mod rec2020 {
    use super::{Primaries, primaries};
    /// `REC2020::primaries`.
    pub const PRIMARIES: Primaries = primaries(
        [0.708, 0.292],
        [0.170, 0.797],
        [0.131, 0.046],
        [0.3127, 0.3290],
    );
}

/// Port of `REC2020_D60` (ColorMatrixHelpers.cpp:74-82 @ v2.5.2).
pub mod rec2020_d60 {
    use super::{Primaries, primaries};
    /// `REC2020_D60::primaries`.
    pub const PRIMARIES: Primaries = primaries(
        [0.708, 0.292],
        [0.170, 0.797],
        [0.131, 0.046],
        [0.32168, 0.33767],
    );
}

/// Port of `P3_DCI` (ColorMatrixHelpers.cpp:84-92 @ v2.5.2).
pub mod p3_dci {
    use super::{Primaries, primaries};
    /// `P3_DCI::primaries`.
    pub const PRIMARIES: Primaries = primaries(
        [0.680, 0.320],
        [0.265, 0.690],
        [0.150, 0.060],
        [0.314, 0.351],
    );
}

/// Port of `P3_D65` (ColorMatrixHelpers.cpp:94-102 @ v2.5.2).
pub mod p3_d65 {
    use super::{Primaries, primaries};
    /// `P3_D65::primaries`.
    pub const PRIMARIES: Primaries = primaries(
        [0.680, 0.320],
        [0.265, 0.690],
        [0.150, 0.060],
        [0.3127, 0.3290],
    );
}

/// Port of `P3_D60` (ColorMatrixHelpers.cpp:104-112 @ v2.5.2).
pub mod p3_d60 {
    use super::{Primaries, primaries};
    /// `P3_D60::primaries`.
    pub const PRIMARIES: Primaries = primaries(
        [0.680, 0.320],
        [0.265, 0.690],
        [0.150, 0.060],
        [0.32168, 0.33767],
    );
}

/// White points in CIE XYZ.
///
/// Port of `WHITEPOINT` (ColorMatrixHelpers.cpp:114-119 @ v2.5.2).
pub mod whitepoint {
    use crate::ops::matrix::matrix_op_data::Offsets;
    /// `WHITEPOINT::D60_XYZ`.
    pub fn d60_xyz() -> Offsets {
        Offsets::new(0.95264607456985, 1., 1.00882518435159, 0.)
    }
    /// `WHITEPOINT::D65_XYZ`.
    pub fn d65_xyz() -> Offsets {
        Offsets::new(0.95045592705167, 1., 1.08905775075988, 0.)
    }
    /// `WHITEPOINT::DCI_XYZ`.
    pub fn dci_xyz() -> Offsets {
        Offsets::new(0.89458689458689, 1., 0.95441595441595, 0.)
    }
}

/// The matrix that converts RGB tristimulus values of `primaries` to CIE XYZ, scaled to take
/// RGB on [0,1] to XYZ on [0,1] (the white's Y is 1), as the 4x4 matrix of a `MatrixArray`.
/// The matrix of the primaries' chromaticities is inverted, and its inverse applied to the
/// white's XYZ gives each primary's gain. A singular matrix (degenerate primaries) is refused
/// with `MatrixArray::inverse`'s exception.
///
/// Port of `rgb2xyz_from_xy` (src/OpenColorIO/transforms/builtins/ColorMatrixHelpers.cpp:
/// 184-227 @ v2.5.2).
pub fn rgb2xyz_from_xy(primaries: &Primaries) -> Result<MatrixArray> {
    let mut wht_xyz = [0.0f64; 3];
    let mut gains = [0.0f64; 3];

    // `1. - x - y`.
    let z = |c: &Chromaticities| (1. - c.xy[0]) - c.xy[1];

    // Create a 4x4 identity matrix.
    let mut matrix = MatrixArray::new();
    {
        let m = matrix.get_values_mut();
        m[0] = primaries.red.xy[0];
        m[4] = primaries.red.xy[1];
        m[8] = z(&primaries.red);

        m[1] = primaries.grn.xy[0];
        m[5] = primaries.grn.xy[1];
        m[9] = z(&primaries.grn);

        m[2] = primaries.blu.xy[0];
        m[6] = primaries.blu.xy[1];
        m[10] = z(&primaries.blu);
    }

    // 'matrix' is always well-conditioned, forming inverse is okay.
    let inv_matrix = matrix.inverse()?;

    wht_xyz[0] = primaries.wht.xy[0] / primaries.wht.xy[1];
    wht_xyz[1] = 1.0; // Set scaling of XYZ values to [0, 1].
    wht_xyz[2] = z(&primaries.wht) / primaries.wht.xy[1];

    // Tristimulus value conversion matrix, initialized to a 4x4 identity matrix for now.
    let mut rgb2xyz = MatrixArray::new();

    let inv = inv_matrix.get_values();
    let m = matrix.get_values();
    for i in 0..3 {
        gains[i] = sse_add(
            sse_add(
                sse_mul(wht_xyz[0], inv[i * 4]),
                sse_mul(wht_xyz[1], inv[i * 4 + 1]),
            ),
            sse_mul(wht_xyz[2], inv[i * 4 + 2]),
        );

        for j in 0..3 {
            rgb2xyz.get_values_mut()[j * 4 + i] = sse_mul(gains[i], m[j * 4 + i]);
        }
    }

    Ok(rgb2xyz)
}

/// How [`build_vonkries_adapt`] adapts white points.
///
/// Port of `AdaptationMethod` (src/OpenColorIO/transforms/builtins/ColorMatrixHelpers.h:
/// 152-157 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaptationMethod {
    /// `ADAPTATION_NONE`.
    None,
    /// `ADAPTATION_BRADFORD`.
    Bradford,
    /// `ADAPTATION_CAT02`.
    Cat02,
}

/// A von Kries chromatic adaptation matrix from the white `src_xyz` to the white `dst_xyz`,
/// with the Bradford or CAT02 cone responses (Bradford for [`AdaptationMethod::None`], as
/// upstream).
///
/// Port of `build_vonkries_adapt` (src/OpenColorIO/transforms/builtins/ColorMatrixHelpers.cpp:
/// 229-274 @ v2.5.2).
pub fn build_vonkries_adapt(
    src_xyz: &Offsets,
    dst_xyz: &Offsets,
    method: AdaptationMethod,
) -> Result<MatrixArray> {
    #[rustfmt::skip]
    const CONE_RESP_MAT_BRADFORD: [f64; 16] = [
         0.8951,  0.2664, -0.1614,  0.,
        -0.7502,  1.7135,  0.0367,  0.,
         0.0389, -0.0685,  1.0296,  0.,
         0.,      0.,      0.,      1.,
    ];

    #[rustfmt::skip]
    const CONE_RESP_MAT_CAT02: [f64; 16] = [
         0.7328,  0.4296, -0.1624,  0.,
        -0.7036,  1.6975,  0.0061,  0.,
         0.0030,  0.0136,  0.9834,  0.,
         0.,      0.,      0.,      1.,
    ];

    let mut xyz2rgb = MatrixArray::new();
    if method == AdaptationMethod::Cat02 {
        xyz2rgb.set_rgba(&CONE_RESP_MAT_CAT02);
    } else {
        xyz2rgb.set_rgba(&CONE_RESP_MAT_BRADFORD);
    }

    let rgb2xyz = xyz2rgb.inverse()?;

    // Convert white point XYZ values to cone primary RGBs.
    let src_rgb = xyz2rgb.inner_offsets(src_xyz);
    let dst_rgb = xyz2rgb.inner_offsets(dst_xyz);
    let scale_factor = [
        dst_rgb[0] / src_rgb[0],
        dst_rgb[1] / src_rgb[1],
        dst_rgb[2] / src_rgb[2],
        1.,
    ];

    // Make a diagonal matrix with the scale factors.
    let mut scale_mat = MatrixArray::new();
    {
        let s = scale_mat.get_values_mut();
        s[0] = scale_factor[0];
        s[5] = scale_factor[1];
        s[10] = scale_factor[2];
        s[15] = scale_factor[3];
    }

    // Compose into the adaptation matrix.
    Ok(rgb2xyz.inner(&scale_mat.inner(&xyz2rgb)))
}

/// A matrix from the primaries `src_prims` to `dst_prims`, which maps RGB [1,1,1] to [1,1,1].
/// Zero offsets for `src_wht_xyz` or `dst_wht_xyz` take that white from the primaries; the
/// whites are adapted with `method` unless they are equal or `method` is
/// [`AdaptationMethod::None`].
///
/// Port of `build_conversion_matrix(src_prims, dst_prims, src_wht_XYZ, dst_wht_XYZ, method)`
/// (src/OpenColorIO/transforms/builtins/ColorMatrixHelpers.cpp:276-328 @ v2.5.2).
pub fn build_conversion_matrix_with_whites(
    src_prims: &Primaries,
    dst_prims: &Primaries,
    src_wht_xyz: &Offsets,
    dst_wht_xyz: &Offsets,
    method: AdaptationMethod,
) -> Result<MatrixArray> {
    let ones = Offsets::new(1., 1., 1., 0.);

    // Calculate the primary conversion matrices.
    let src_rgb2xyz = rgb2xyz_from_xy(src_prims)?;
    let dst_rgb2xyz = rgb2xyz_from_xy(dst_prims)?;
    let dst_xyz2rgb = dst_rgb2xyz.inverse()?;

    // Return the composed matrix if no white point adaptation is needed.
    if !src_wht_xyz.is_not_null()
        && !dst_wht_xyz.is_not_null()
        // If the white points are equal, don't need to adapt.
        && src_prims.wht.xy[0] == dst_prims.wht.xy[0]
        && src_prims.wht.xy[1] == dst_prims.wht.xy[1]
    {
        return Ok(dst_xyz2rgb.inner(&src_rgb2xyz));
    }
    if method == AdaptationMethod::None {
        return Ok(dst_xyz2rgb.inner(&src_rgb2xyz));
    }

    // Calculate src and dst white XYZ.
    let dst_wht = if dst_wht_xyz.is_not_null() {
        *dst_wht_xyz
    } else {
        dst_rgb2xyz.inner_offsets(&ones)
    };
    let src_wht = if src_wht_xyz.is_not_null() {
        *src_wht_xyz
    } else {
        src_rgb2xyz.inner_offsets(&ones)
    };

    // Build the adaptation matrix (may be an identity).
    let vkmat = build_vonkries_adapt(&src_wht, &dst_wht, method)?;

    // Compose the adaptation into the conversion matrix.
    Ok(dst_xyz2rgb.inner(&vkmat.inner(&src_rgb2xyz)))
}

/// [`build_conversion_matrix_with_whites`] with both whites taken from the primaries.
///
/// Port of `build_conversion_matrix(src_prims, dst_prims, method)`
/// (src/OpenColorIO/transforms/builtins/ColorMatrixHelpers.cpp:330-336 @ v2.5.2).
pub fn build_conversion_matrix(
    src_prims: &Primaries,
    dst_prims: &Primaries,
    method: AdaptationMethod,
) -> Result<MatrixArray> {
    let zero = Offsets::new(0., 0., 0., 0.);
    build_conversion_matrix_with_whites(src_prims, dst_prims, &zero, &zero, method)
}

/// A matrix from `src_prims` to CIE XYZ with the D65 white.
///
/// Port of `build_conversion_matrix_to_XYZ_D65` (src/OpenColorIO/transforms/builtins/
/// ColorMatrixHelpers.cpp:338-343 @ v2.5.2).
pub fn build_conversion_matrix_to_xyz_d65(
    src_prims: &Primaries,
    method: AdaptationMethod,
) -> Result<MatrixArray> {
    let zero = Offsets::new(0., 0., 0., 0.);
    build_conversion_matrix_with_whites(
        src_prims,
        &cie_xyz_illum_e::PRIMARIES,
        &zero,
        &whitepoint::d65_xyz(),
        method,
    )
}

/// A matrix from CIE XYZ with the D65 white to `dst_prims`.
///
/// Port of `build_conversion_matrix_from_XYZ_D65` (src/OpenColorIO/transforms/builtins/
/// ColorMatrixHelpers.cpp:345-350 @ v2.5.2).
pub fn build_conversion_matrix_from_xyz_d65(
    dst_prims: &Primaries,
    method: AdaptationMethod,
) -> Result<MatrixArray> {
    let zero = Offsets::new(0., 0., 0., 0.);
    build_conversion_matrix_with_whites(
        &cie_xyz_illum_e::PRIMARIES,
        dst_prims,
        &whitepoint::d65_xyz(),
        &zero,
        method,
    )
}

#[cfg(test)]
#[path = "color_matrix_helpers_tests.rs"]
mod tests;
