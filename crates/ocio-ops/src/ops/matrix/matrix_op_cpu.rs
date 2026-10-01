// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Matrix op's CPU renderers: a port of `src/OpenColorIO/ops/matrix/MatrixOpCPU.h` and
//! `MatrixOpCPU.cpp` @ v2.5.2.
//!
//! [`get_matrix_renderer`] picks one by the matrix's shape: a diagonal matrix scales each
//! channel ([`ScaleRenderer`], or [`ScaleWithOffsetRenderer`] with offsets), any other
//! multiplies the pixel by the matrix ([`MatrixRenderer`], [`MatrixWithOffsetRenderer`]). They
//! compute in `float`: the constructors round the data's `double` values to `float`. They write
//! every channel, alpha included.
//!
//! # The wheels' operand orders
//!
//! Where two NaNs meet in a product or a sum, x86 returns the first operand's, so the order
//! decides which NaN comes out. Neither compiler kept the source's everywhere:
//! - The scale renderers are plain C++, `in * scale` (`+ offset`). GCC (Linux) keeps that order.
//!   MSVC (Windows) unrolls the loop four pixels at a time and keeps it there, but in the loop
//!   that finishes the last `numPixels % 4` pixels computes blue as `scale * in`
//!   (`ScaleRenderer::apply` at 0x1802b29c2, `ScaleWithOffsetRenderer::apply` at 0x1802b2be4).
//! - The matrix renderers use SSE2 intrinsics: `(m0 * r + m1 * g) + (m2 * b + m3 * a)`, then
//!   `+ offset`, in the source. Both compilers multiply the pixel's value by the column
//!   (`r * m0`), and order the sums each their own way. GCC computes
//!   `(b*m2 + a*m3) + (r*m0 + g*m1)` (`MatrixRenderer::apply` at 0x4e4140,
//!   `MatrixWithOffsetRenderer::apply` at 0x4e40c0). MSVC computes
//!   `(a*m3 + b*m2) + (g*m1 + r*m0)` in its four-pixel loop (0x1802b24f4, 0x1802b26b4) and
//!   `(g*m1 + r*m0) + (a*m3 + b*m2)` in the loop that finishes the last `numPixels % 4` pixels
//!   (0x1802b260b, 0x1802b27db). Both add the offset last.
//!
//! The port computes each operation with [`sse_mul`] and [`sse_add`] in those orders, per
//! platform (PLAN.md D12) and, on Windows, per loop.

use std::sync::Arc;

use super::matrix_op_data::MatrixOpData;
use crate::exception::{Exception, Result};
use crate::math_utils::{sse_add, sse_mul};
use crate::op::CpuOp;
use crate::open_color_types::TransformDirection;

/// Whether the wheel's `apply` of `num_pixels` pixels processes the one at `index` in MSVC's
/// loop that finishes the last `num_pixels % 4` pixels, after its four-pixel loop. Always
/// `false` on Linux, where GCC's loops all compute the same way.
fn in_msvc_remainder(index: usize, num_pixels: usize) -> bool {
    cfg!(target_os = "windows") && index >= num_pixels & !3
}

/// A diagonal matrix without offsets: each channel times its scale.
///
/// Port of `ScaleRenderer` (src/OpenColorIO/ops/matrix/MatrixOpCPU.cpp:17-28 @ v2.5.2).
#[derive(Debug)]
pub struct ScaleRenderer {
    /// `m_scale`.
    scale: [f32; 4],
}

impl ScaleRenderer {
    /// Port of `ScaleRenderer::ScaleRenderer` (MatrixOpCPU.cpp:79-88 @ v2.5.2).
    pub fn new(mat: &MatrixOpData) -> Self {
        let m = mat.get_array().get_values();
        ScaleRenderer {
            scale: [m[0] as f32, m[5] as f32, m[10] as f32, m[15] as f32],
        }
    }
}

impl CpuOp for ScaleRenderer {
    /// Port of `ScaleRenderer::apply` (MatrixOpCPU.cpp:90-105 @ v2.5.2), with the wheels'
    /// operand orders (module docs).
    fn apply(&self, rgba: &mut [f32]) {
        let num_pixels = rgba.len() / 4;
        let s = &self.scale;
        for (index, px) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            px[0] = sse_mul(px[0], s[0]);
            px[1] = sse_mul(px[1], s[1]);
            px[2] = if in_msvc_remainder(index, num_pixels) {
                sse_mul(s[2], px[2])
            } else {
                sse_mul(px[2], s[2])
            };
            px[3] = sse_mul(px[3], s[3]);
        }
    }
}

/// A diagonal matrix with offsets: each channel times its scale, plus its offset.
///
/// Port of `ScaleWithOffsetRenderer` (src/OpenColorIO/ops/matrix/MatrixOpCPU.cpp:30-42 @
/// v2.5.2).
#[derive(Debug)]
pub struct ScaleWithOffsetRenderer {
    /// `m_scale`.
    scale: [f32; 4],
    /// `m_offset`.
    offset: [f32; 4],
}

impl ScaleWithOffsetRenderer {
    /// Port of `ScaleWithOffsetRenderer::ScaleWithOffsetRenderer` (MatrixOpCPU.cpp:107-123 @
    /// v2.5.2).
    pub fn new(mat: &MatrixOpData) -> Self {
        let m = mat.get_array().get_values();
        let o = mat.get_offsets();
        ScaleWithOffsetRenderer {
            scale: [m[0] as f32, m[5] as f32, m[10] as f32, m[15] as f32],
            offset: [o[0] as f32, o[1] as f32, o[2] as f32, o[3] as f32],
        }
    }
}

impl CpuOp for ScaleWithOffsetRenderer {
    /// Port of `ScaleWithOffsetRenderer::apply` (MatrixOpCPU.cpp:125-140 @ v2.5.2), with the
    /// wheels' operand orders (module docs).
    fn apply(&self, rgba: &mut [f32]) {
        let num_pixels = rgba.len() / 4;
        let (s, o) = (&self.scale, &self.offset);
        for (index, px) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            px[0] = sse_add(sse_mul(px[0], s[0]), o[0]);
            px[1] = sse_add(sse_mul(px[1], s[1]), o[1]);
            let blue = if in_msvc_remainder(index, num_pixels) {
                sse_mul(s[2], px[2])
            } else {
                sse_mul(px[2], s[2])
            };
            px[2] = sse_add(blue, o[2]);
            px[3] = sse_add(sse_mul(px[3], s[3]), o[3]);
        }
    }
}

/// The matrix's columns as the renderers keep them: `columns[j][i]` is `m[i][j]`, the
/// multiplier of input channel `j` in output channel `i`, rounded to `float`.
///
/// Port of the column setup of `MatrixWithOffsetRenderer::MatrixWithOffsetRenderer` and
/// `MatrixRenderer::MatrixRenderer` (MatrixOpCPU.cpp:144-173, 292-320 @ v2.5.2).
fn columns(mat: &MatrixOpData) -> [[f32; 4]; 4] {
    let dim = mat.get_array().get_length() as usize;
    let m = mat.get_array().get_values();
    let mut columns = [[0.0f32; 4]; 4];
    for (j, column) in columns.iter_mut().enumerate() {
        for (i, value) in column.iter_mut().enumerate() {
            *value = m[i * dim + j] as f32;
        }
    }
    columns
}

/// One output channel of the matrix renderers: the sum of the four products `rm0` (red times
/// its multiplier), `gm1`, `bm2` and `am3`, in the order the wheel's loop sums them (module
/// docs).
fn matrix_sum(rm0: f32, gm1: f32, bm2: f32, am3: f32, msvc_remainder: bool) -> f32 {
    if !cfg!(target_os = "windows") {
        sse_add(sse_add(bm2, am3), sse_add(rm0, gm1))
    } else if msvc_remainder {
        sse_add(sse_add(gm1, rm0), sse_add(am3, bm2))
    } else {
        sse_add(sse_add(am3, bm2), sse_add(gm1, rm0))
    }
}

/// The products of one pixel's channels with one column each, for output channel `i`.
fn products(px: [f32; 4], columns: &[[f32; 4]; 4], i: usize) -> [f32; 4] {
    [
        sse_mul(px[0], columns[0][i]),
        sse_mul(px[1], columns[1][i]),
        sse_mul(px[2], columns[2][i]),
        sse_mul(px[3], columns[3][i]),
    ]
}

/// Any other matrix, with offsets: the pixel times the matrix, plus the offsets.
///
/// Port of `MatrixWithOffsetRenderer` (src/OpenColorIO/ops/matrix/MatrixOpCPU.cpp:44-61 @
/// v2.5.2).
#[derive(Debug)]
pub struct MatrixWithOffsetRenderer {
    /// `m_column1` to `m_column4`.
    columns: [[f32; 4]; 4],
    /// `m_offset`.
    offset: [f32; 4],
}

impl MatrixWithOffsetRenderer {
    /// Port of `MatrixWithOffsetRenderer::MatrixWithOffsetRenderer` (MatrixOpCPU.cpp:142-181
    /// @ v2.5.2).
    pub fn new(mat: &MatrixOpData) -> Self {
        let o = mat.get_offsets();
        MatrixWithOffsetRenderer {
            columns: columns(mat),
            offset: [o[0] as f32, o[1] as f32, o[2] as f32, o[3] as f32],
        }
    }
}

impl CpuOp for MatrixWithOffsetRenderer {
    /// Port of `MatrixWithOffsetRenderer::apply` (MatrixOpCPU.cpp:209-288 @ v2.5.2), its SSE2
    /// branch (`OCIO_USE_SSE2`, set in both wheels), with the wheels' operand orders (module
    /// docs).
    fn apply(&self, rgba: &mut [f32]) {
        let num_pixels = rgba.len() / 4;
        for (index, px) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let remainder = in_msvc_remainder(index, num_pixels);
            let input = *px;
            for (i, out) in px.iter_mut().enumerate() {
                let [rm0, gm1, bm2, am3] = products(input, &self.columns, i);
                *out = sse_add(matrix_sum(rm0, gm1, bm2, am3, remainder), self.offset[i]);
            }
        }
    }
}

/// Any other matrix, without offsets: the pixel times the matrix.
///
/// Port of `MatrixRenderer` (src/OpenColorIO/ops/matrix/MatrixOpCPU.cpp:63-77 @ v2.5.2).
#[derive(Debug)]
pub struct MatrixRenderer {
    /// `m_column1` to `m_column4`.
    columns: [[f32; 4]; 4],
}

impl MatrixRenderer {
    /// Port of `MatrixRenderer::MatrixRenderer` (MatrixOpCPU.cpp:290-321 @ v2.5.2).
    pub fn new(mat: &MatrixOpData) -> Self {
        MatrixRenderer {
            columns: columns(mat),
        }
    }
}

impl CpuOp for MatrixRenderer {
    /// Port of `MatrixRenderer::apply` (MatrixOpCPU.cpp:323-396 @ v2.5.2), its SSE2 branch, with
    /// the wheels' operand orders (module docs).
    fn apply(&self, rgba: &mut [f32]) {
        let num_pixels = rgba.len() / 4;
        for (index, px) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let remainder = in_msvc_remainder(index, num_pixels);
            let input = *px;
            for (i, out) in px.iter_mut().enumerate() {
                let [rm0, gm1, bm2, am3] = products(input, &self.columns, i);
                *out = matrix_sum(rm0, gm1, bm2, am3, remainder);
            }
        }
    }
}

/// The renderer for `mat`, a forward matrix: "Op::finalize has to be called." for an inverse
/// one.
///
/// Port of `GetMatrixRenderer` (src/OpenColorIO/ops/matrix/MatrixOpCPU.cpp:400-428 @ v2.5.2).
pub fn get_matrix_renderer(mat: &MatrixOpData) -> Result<Arc<dyn CpuOp>> {
    if mat.get_direction() == TransformDirection::Inverse {
        return Err(Exception::new("Op::finalize has to be called."));
    }
    Ok(if mat.is_diagonal() {
        if mat.has_offsets() {
            Arc::new(ScaleWithOffsetRenderer::new(mat))
        } else {
            Arc::new(ScaleRenderer::new(mat))
        }
    } else if mat.has_offsets() {
        Arc::new(MatrixWithOffsetRenderer::new(mat))
    } else {
        Arc::new(MatrixRenderer::new(mat))
    })
}

#[cfg(test)]
#[path = "matrix_op_cpu_tests.rs"]
mod tests;
