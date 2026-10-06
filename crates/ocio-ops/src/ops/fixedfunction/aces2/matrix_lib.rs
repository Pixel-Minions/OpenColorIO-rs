// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The small vector and matrix types of ACES 2.0 and their arithmetic, in `float`: a port of
//! `src/OpenColorIO/ops/fixedfunction/ACES2/MatrixLib.h` @ v2.5.2.
//!
//! The C++ functions are `inline`, so each caller's compiled code decides the operand order
//! where two NaNs meet; these keep the source's order, and the callers that need another one
//! say so where they call them.

use crate::exception::Result;
use crate::math_utils::{sse_add, sse_mul};
use crate::ops::matrix::matrix_op_data::MatrixArray;

/// `f2`: `std::array<float, 2>`.
pub type F2 = [f32; 2];
/// `f3`: `std::array<float, 3>`.
pub type F3 = [f32; 3];
/// `f4`: `std::array<float, 4>`.
pub type F4 = [f32; 4];
/// `m33f`: a 3x3 matrix, row by row (`std::array<float, 9>`).
pub type M33f = [f32; 9];

/// Port of `f3_from_f` (src/OpenColorIO/ops/fixedfunction/ACES2/MatrixLib.h:21-24 @ v2.5.2).
#[inline]
pub fn f3_from_f(v: f32) -> F3 {
    [v, v, v]
}

/// `v + f3[i]` for each component.
///
/// Port of `add_f_f3` (src/OpenColorIO/ops/fixedfunction/ACES2/MatrixLib.h:26-33 @ v2.5.2).
#[inline]
pub fn add_f_f3(v: f32, f3: &F3) -> F3 {
    [sse_add(v, f3[0]), sse_add(v, f3[1]), sse_add(v, f3[2])]
}

/// `v * f3[i]` for each component.
///
/// Port of `mult_f_f3` (src/OpenColorIO/ops/fixedfunction/ACES2/MatrixLib.h:35-42 @ v2.5.2).
#[inline]
pub fn mult_f_f3(v: f32, f3: &F3) -> F3 {
    [sse_mul(v, f3[0]), sse_mul(v, f3[1]), sse_mul(v, f3[2])]
}

/// The row vector `f3` times the rows of `mat33`: `f3[0] * m[3r] + f3[1] * m[3r + 1] +
/// f3[2] * m[3r + 2]` for row `r`, summed left to right.
///
/// Port of `mult_f3_f33` (src/OpenColorIO/ops/fixedfunction/ACES2/MatrixLib.h:44-51 @ v2.5.2).
#[inline]
pub fn mult_f3_f33(f3: &F3, mat33: &M33f) -> F3 {
    let row = |r: usize| {
        sse_add(
            sse_add(
                sse_mul(f3[0], mat33[3 * r]),
                sse_mul(f3[1], mat33[3 * r + 1]),
            ),
            sse_mul(f3[2], mat33[3 * r + 2]),
        )
    };
    [row(0), row(1), row(2)]
}

/// How a wheel's compiled copy of [`mult_f3_f33`] computes one row, with `p_i` the product of
/// `f3[i]` and `mat33[3r + i]`: which operand of the products comes first (`X`: the vector's,
/// `M`: the matrix's), and the order of the sums. The value is the source's in every order;
/// where two operands are NaN, the result is the first one's (`math_utils::sse_add`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// `(x0*m0 + x1*m1) + x2*m2`: the source's order.
    X012,
    /// `(x1*m1 + x0*m0) + x2*m2`.
    X102,
    /// `(m0*x0 + m1*x1) + m2*x2`.
    M012,
    /// `m2*x2 + (m0*x0 + m1*x1)`.
    M201,
}

/// [`mult_f3_f33`] with each row computed as `rows` says: the orders of a wheel's compiled
/// copy, which its caller has read from that wheel's machine code.
#[inline]
pub fn mult_f3_f33_rows(f3: &F3, mat33: &M33f, rows: [Row; 3]) -> F3 {
    let row = |r: usize| {
        let m = &mat33[3 * r..3 * r + 3];
        let p = |i: usize| match rows[r] {
            Row::X012 | Row::X102 => sse_mul(f3[i], m[i]),
            Row::M012 | Row::M201 => sse_mul(m[i], f3[i]),
        };
        match rows[r] {
            Row::X012 | Row::M012 => sse_add(sse_add(p(0), p(1)), p(2)),
            Row::X102 => sse_add(sse_add(p(1), p(0)), p(2)),
            Row::M201 => sse_add(p(2), sse_add(p(0), p(1))),
        }
    };
    [row(0), row(1), row(2)]
}

/// The product `a * b` of two 3x3 matrices, each entry summed left to right.
///
/// Port of `mult_f33_f33` (src/OpenColorIO/ops/fixedfunction/ACES2/MatrixLib.h:53-68 @ v2.5.2).
pub fn mult_f33_f33(a: &M33f, b: &M33f) -> M33f {
    let entry = |r: usize, c: usize| {
        sse_add(
            sse_add(sse_mul(a[3 * r], b[c]), sse_mul(a[3 * r + 1], b[3 + c])),
            sse_mul(a[3 * r + 2], b[6 + c]),
        )
    };
    [
        entry(0, 0),
        entry(0, 1),
        entry(0, 2),
        entry(1, 0),
        entry(1, 1),
        entry(1, 2),
        entry(2, 0),
        entry(2, 1),
        entry(2, 2),
    ]
}

/// The transpose of `mat33` with its diagonal scaled by `scale`. (Upstream's name says only
/// the scale; the transpose is part of its result.)
///
/// Port of `scale_f33` (src/OpenColorIO/ops/fixedfunction/ACES2/MatrixLib.h:70-77 @ v2.5.2).
pub fn scale_f33(mat33: &M33f, scale: &F3) -> M33f {
    [
        sse_mul(mat33[0], scale[0]),
        mat33[3],
        mat33[6],
        mat33[1],
        sse_mul(mat33[4], scale[1]),
        mat33[7],
        mat33[2],
        mat33[5],
        sse_mul(mat33[8], scale[2]),
    ]
}

/// The 3x3 part of a 4x4 `MatrixArray`, each value narrowed to `float`.
///
/// Port of `m33_from_ocio_matrix_array` (src/OpenColorIO/ops/fixedfunction/ACES2/
/// MatrixLib.h:79-87 @ v2.5.2).
pub fn m33_from_ocio_matrix_array(array: &MatrixArray) -> M33f {
    let v = array.get_values();
    [
        v[0] as f32,
        v[1] as f32,
        v[2] as f32,
        v[4] as f32,
        v[5] as f32,
        v[6] as f32,
        v[8] as f32,
        v[9] as f32,
        v[10] as f32,
    ]
}

/// The inverse of `mat33`, through `MatrixArray::inverse` in `double` (the 4x4 identity with
/// `mat33` in its upper left), narrowed back to `float`. A singular matrix is refused with
/// `MatrixArray::inverse`'s exception.
///
/// Port of `invert_f33` (src/OpenColorIO/ops/fixedfunction/ACES2/MatrixLib.h:89-108 @ v2.5.2).
pub fn invert_f33(mat33: &M33f) -> Result<M33f> {
    let mut array = MatrixArray::new();
    {
        let v = array.get_values_mut();
        v[0] = f64::from(mat33[0]);
        v[1] = f64::from(mat33[1]);
        v[2] = f64::from(mat33[2]);

        v[4] = f64::from(mat33[3]);
        v[5] = f64::from(mat33[4]);
        v[6] = f64::from(mat33[5]);

        v[8] = f64::from(mat33[6]);
        v[9] = f64::from(mat33[7]);
        v[10] = f64::from(mat33[8]);
    }

    let inverse = array.inverse()?;
    Ok(m33_from_ocio_matrix_array(&inverse))
}
