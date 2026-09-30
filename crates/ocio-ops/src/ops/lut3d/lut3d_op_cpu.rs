// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Forward 3D LUT CPU renderers: a port of `BaseLut3DRenderer`, `Lut3DTetrahedralRenderer`,
//! `Lut3DRenderer` and `GetForwardLut3DRenderer`
//! (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:28-820 and 1736-1747 @ v2.5.2).
//!
//! **Numeric profiles.** Every C++ kernel is its own renderer here, written as exact scalar code
//! that reproduces the kernel's arithmetic lane by lane:
//!
//! | Profile | C++ | Runs when |
//! |---|---|---|
//! | Generic tetrahedral | `Lut3DTetrahedralRenderer::apply`, scalar branch | no SIMD kernel, or a call with at most one pixel |
//! | SSE2 tetrahedral | `applyTetrahedralSSE2` | `hasSSE2()` |
//! | AVX tetrahedral | `applyTetrahedralAVX` | `hasAVX() && !AVXSlow()` |
//! | AVX2 tetrahedral | `applyTetrahedralAVX2` (FMA) | `hasAVX2() && !AVX2SlowGather()` |
//! | AVX-512 tetrahedral | `applyTetrahedralAVX512` (FMA) | `hasAVX512()` |
//! | SSE2 trilinear | `Lut3DRenderer::apply`, `OCIO_USE_SSE2` branch | builds with SSE2 (every x86-64 wheel) |
//! | Generic trilinear | `Lut3DRenderer::apply`, `#else` branch | builds without SSE2 |
//!
//! The later tetrahedral kernels win: the dispatch overwrites the choice in that order.
//!
//! **NaN operand order** (CLAUDE.md). The tetrahedral paths never create a NaN: the inputs are
//! clamped (NaN becomes 0), the LUT values are sanitized, and the weights are in `[0, 1]`, so a
//! sum can at most overflow to an infinity. The trilinear paths use [`sse_add`] and [`sse_mul`]
//! in upstream's source order. The generic one computes `b - a`, which overflows when
//! neighboring values are huge and of opposite signs, and can then compute `inf * 0` or
//! `inf - inf`. Every NaN there is the x86 default NaN, so the order cannot change the bits.
//!
//! Not ported yet: `InvLut3DRenderer` (the exact inverse).

use super::lut3d_op_cpu_avx::apply_tetrahedral_avx;
use super::lut3d_op_cpu_avx2::apply_tetrahedral_avx2;
use super::lut3d_op_cpu_avx512::apply_tetrahedral_avx512;
use super::lut3d_op_cpu_sse2::apply_tetrahedral_sse2;
use super::lut3d_op_data::{Interpolation, Lut3DOpData};
use crate::cpu_info::CpuInfo;
use crate::math_utils::{clamp, sse_add, sse_cvttps_epi32, sse_max, sse_min, sse_mul};

/// A tetrahedral SIMD kernel: the functions `m_applyLutFunc` can point to
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:386-416 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TetrahedralKernel {
    /// `applyTetrahedralSSE2`
    Sse2,
    /// `applyTetrahedralAVX`
    Avx,
    /// `applyTetrahedralAVX2`
    Avx2,
    /// `applyTetrahedralAVX512`
    Avx512,
}

/// The kernel a tetrahedral renderer picks on `cpu`. Port of the
/// `Lut3DTetrahedralRenderer` constructor (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:386-416 @ v2.5.2).
/// The `#if OCIO_USE_*` guards are part of `CpuInfo::has_*`.
pub fn tetrahedral_kernel(cpu: &CpuInfo) -> Option<TetrahedralKernel> {
    let mut kernel = None;
    if cpu.has_sse2() {
        kernel = Some(TetrahedralKernel::Sse2);
    }
    if cpu.has_avx() && !cpu.avx_slow() {
        kernel = Some(TetrahedralKernel::Avx);
    }
    if cpu.has_avx2() && !cpu.avx2_slow_gather() {
        kernel = Some(TetrahedralKernel::Avx2);
    }
    if cpu.has_avx512() {
        kernel = Some(TetrahedralKernel::Avx512);
    }
    kernel
}

/// Maps special values into the float domain: -Inf to -FLT_MAX, Inf to FLT_MAX, NaN to 0.
/// Port of `SanitizeFloat` (src/OpenColorIO/MathUtils.cpp:145-160 @ v2.5.2).
fn sanitize_float(f: f32) -> f32 {
    if f == f32::NEG_INFINITY {
        -f32::MAX
    } else if f == f32::INFINITY {
        f32::MAX
    } else if f.is_nan() {
        0.0
    } else {
        f
    }
}

/// Port of `GetLut3DIndexBlueFast` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:302-305 @ v2.5.2).
fn index_blue_fast(index_r: i32, index_g: i32, index_b: i32, dim: i32, components: i32) -> usize {
    (components * (index_b + dim * (index_g + dim * index_r))) as usize
}

/// The renderer's copy of the LUT and its invariants. Port of `BaseLut3DRenderer`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:30-57 and 307-384 @ v2.5.2).
#[derive(Debug, Clone)]
struct BaseLut3D {
    /// `m_optLut`: the sanitized values, RGB plus a zero when the build has SSE2 (so the SIMD
    /// code can load 4 floats per entry), RGB otherwise.
    opt_lut: Vec<f32>,
    /// `m_dim`
    dim: u32,
    /// `m_step`
    step: f32,
    /// `m_components`
    components: usize,
}

impl BaseLut3D {
    /// Port of `BaseLut3DRenderer::updateData` and both `createOptLut`
    /// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:327-384 @ v2.5.2).
    fn new(lut: &Lut3DOpData, use_sse2: bool) -> BaseLut3D {
        let dim = lut.array().length();
        let step = dim as f32 - 1.0f32;
        let components = if use_sse2 { 4 } else { 3 };

        let values = lut.array().values();
        let max_entries = dim as usize * dim as usize * dim as usize;
        let mut opt_lut = Vec::with_capacity(max_entries * components);
        for idx in 0..max_entries {
            opt_lut.push(sanitize_float(values[idx * 3]));
            opt_lut.push(sanitize_float(values[idx * 3 + 1]));
            opt_lut.push(sanitize_float(values[idx * 3 + 2]));
            if use_sse2 {
                opt_lut.push(0.0);
            }
        }
        BaseLut3D {
            opt_lut,
            dim,
            step,
            components,
        }
    }
}

/// Tetrahedral interpolation. Port of `Lut3DTetrahedralRenderer`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:59-66 and 386-624 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct Lut3DTetrahedralRenderer {
    base: BaseLut3D,
    kernel: Option<TetrahedralKernel>,
}

impl Lut3DTetrahedralRenderer {
    /// The renderer OCIO builds for `lut` on `cpu`.
    pub fn new(lut: &Lut3DOpData, cpu: &CpuInfo) -> Lut3DTetrahedralRenderer {
        let kernel = tetrahedral_kernel(cpu);
        // The kernels read 4 floats per entry; every build with a SIMD kernel has SSE2.
        assert!(
            kernel.is_none() || cpu.build().use_sse2,
            "a SIMD Lut3D kernel needs a build with OCIO_USE_SSE2"
        );
        Lut3DTetrahedralRenderer {
            base: BaseLut3D::new(lut, cpu.build().use_sse2),
            kernel,
        }
    }

    /// The SIMD kernel used for calls with more than one pixel.
    pub fn kernel(&self) -> Option<TetrahedralKernel> {
        self.kernel
    }

    /// Applies the LUT to packed RGBA pixels. Port of `Lut3DTetrahedralRenderer::apply`
    /// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:422-624 @ v2.5.2): the SIMD kernel runs only
    /// when there is one and the call has more than one pixel.
    pub fn apply(&self, input: &[f32], output: &mut [f32]) {
        assert_eq!(input.len(), output.len());
        assert_eq!(input.len() % 4, 0);
        let num_pixels = input.len() / 4;
        let lut = &self.base.opt_lut;
        let dim = self.base.dim as i32;
        match self.kernel {
            Some(kernel) if num_pixels > 1 => {
                let count = i32::try_from(num_pixels).expect("pixel count fits in an int");
                match kernel {
                    TetrahedralKernel::Sse2 => {
                        apply_tetrahedral_sse2(lut, dim, input, output, count)
                    }
                    TetrahedralKernel::Avx => apply_tetrahedral_avx(lut, dim, input, output, count),
                    TetrahedralKernel::Avx2 => {
                        apply_tetrahedral_avx2(lut, dim, input, output, count)
                    }
                    TetrahedralKernel::Avx512 => {
                        apply_tetrahedral_avx512(lut, dim, input, output, count)
                    }
                }
            }
            _ => self.apply_generic(input, output),
        }
    }

    /// The scalar branch of `Lut3DTetrahedralRenderer::apply`
    /// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:431-623 @ v2.5.2).
    fn apply_generic(&self, input: &[f32], output: &mut [f32]) {
        let BaseLut3D {
            opt_lut: lut,
            dim,
            step,
            components,
        } = &self.base;
        let dim_minus_one = *dim as f32 - 1.0f32;
        let (dim, components) = (*dim as i32, *components as i32);

        for (inp, out) in input
            .as_chunks::<4>()
            .0
            .iter()
            .zip(output.as_chunks_mut::<4>().0.iter_mut())
        {
            let new_alpha = inp[3];

            // NaNs become 0.
            let idx = [
                clamp(inp[0] * step, 0.0, dim_minus_one),
                clamp(inp[1] * step, 0.0, dim_minus_one),
                clamp(inp[2] * step, 0.0, dim_minus_one),
            ];

            let low = idx.map(|v| v.floor() as i32);
            // When idx is exactly an index, high is wrong, but the delta is then 0.
            let high = idx.map(|v| v.ceil() as i32);

            let fx = idx[0] - low[0] as f32;
            let fy = idx[1] - low[1] as f32;
            let fz = idx[2] - low[2] as f32;

            // Index into the LUT for the surrounding corners.
            let at = |r: i32, g: i32, b: i32| index_blue_fast(r, g, b, dim, components);
            let n000 = at(low[0], low[1], low[2]);
            let n100 = at(high[0], low[1], low[2]);
            let n010 = at(low[0], high[1], low[2]);
            let n001 = at(low[0], low[1], high[2]);
            let n110 = at(high[0], high[1], low[2]);
            let n101 = at(high[0], low[1], high[2]);
            let n011 = at(low[0], high[1], high[2]);
            let n111 = at(high[0], high[1], high[2]);

            // `w * lut[a] + w * lut[b] + w * lut[c] + w * lut[d]`, left to right, per channel.
            let blend = |w: [f32; 4], n: [usize; 4], c: usize| {
                w[0] * lut[n[0] + c]
                    + w[1] * lut[n[1] + c]
                    + w[2] * lut[n[2] + c]
                    + w[3] * lut[n[3] + c]
            };
            let (w, n) = if fx > fy {
                if fy > fz {
                    ([1.0 - fx, fx - fy, fy - fz, fz], [n000, n100, n110, n111])
                } else if fx > fz {
                    ([1.0 - fx, fx - fz, fz - fy, fy], [n000, n100, n101, n111])
                } else {
                    ([1.0 - fz, fz - fx, fx - fy, fy], [n000, n001, n101, n111])
                }
            } else if fz > fy {
                ([1.0 - fz, fz - fy, fy - fx, fx], [n000, n001, n011, n111])
            } else if fz > fx {
                ([1.0 - fy, fy - fz, fz - fx, fx], [n000, n010, n011, n111])
            } else {
                ([1.0 - fy, fy - fx, fx - fz, fz], [n000, n010, n110, n111])
            };
            out[0] = blend(w, n, 0);
            out[1] = blend(w, n, 1);
            out[2] = blend(w, n, 2);
            out[3] = new_alpha;
        }
    }
}

/// Trilinear interpolation. Port of `Lut3DRenderer`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:68-76 and 626-820 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct Lut3DRenderer {
    base: BaseLut3D,
    use_sse2: bool,
}

impl Lut3DRenderer {
    /// The renderer OCIO builds for `lut` on `cpu`. Its code path is chosen at compile time
    /// (`OCIO_USE_SSE2`), not by the CPU flags.
    pub fn new(lut: &Lut3DOpData, cpu: &CpuInfo) -> Lut3DRenderer {
        let use_sse2 = cpu.build().use_sse2;
        Lut3DRenderer {
            base: BaseLut3D::new(lut, use_sse2),
            use_sse2,
        }
    }

    /// Whether the build's SSE2 code path runs.
    pub fn uses_sse2(&self) -> bool {
        self.use_sse2
    }

    /// Applies the LUT to packed RGBA pixels. Port of `Lut3DRenderer::apply`
    /// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:635-820 @ v2.5.2).
    pub fn apply(&self, input: &[f32], output: &mut [f32]) {
        assert_eq!(input.len(), output.len());
        assert_eq!(input.len() % 4, 0);
        if self.use_sse2 {
            self.apply_sse2(input, output);
        } else {
            self.apply_generic(input, output);
        }
    }

    /// The `OCIO_USE_SSE2` branch of `Lut3DRenderer::apply`
    /// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:640-739 @ v2.5.2), one lane at a time, with
    /// `GetLut3DIndices` and `LookupNearest4` (Lut3DOpCPU.cpp:204-264).
    fn apply_sse2(&self, input: &[f32], output: &mut [f32]) {
        let lut = &self.base.opt_lut;
        let step = self.base.step;
        let max_idx = (self.base.dim - 1) as f32;
        let dim = self.base.dim;

        // 4 * (idxB + sizesB * (idxG + sizesG * idxR)), in 32-bit integer lanes.
        let lookup = |r: i32, g: i32, b: i32| -> [f32; 4] {
            let offset = (b as u32).wrapping_add(
                dim.wrapping_mul((g as u32).wrapping_add(dim.wrapping_mul(r as u32))),
            ) << 2;
            let at = offset as usize;
            [lut[at], lut[at + 1], lut[at + 2], lut[at + 3]]
        };

        for (inp, out) in input
            .as_chunks::<4>()
            .0
            .iter()
            .zip(output.as_chunks_mut::<4>().0.iter_mut())
        {
            let new_alpha = inp[3];

            let data = [inp[0], inp[1], inp[2], inp[3]];

            // NaNs become 0.
            let idx = data.map(|v| sse_min(sse_max(v * step, 0.0), max_idx));

            // lowIdx = floor(idx), with lowIdx in [0, maxIdx].
            let low_int = idx.map(sse_cvttps_epi32);
            let low = low_int.map(|v| v as f32);

            // highIdx = ceil(idx), with highIdx in [1, maxIdx].
            let high_int: [i32; 4] = std::array::from_fn(|k| {
                if low[k] < max_idx {
                    low_int[k] + 1
                } else {
                    low_int[k]
                }
            });

            let delta: [f32; 4] = std::array::from_fn(|k| idx[k] - low[k]);

            let (l0, l1, l2) = (low_int[0], low_int[1], low_int[2]);
            let (h0, h1, h2) = (high_int[0], high_int[1], high_int[2]);

            // Look up the 8 corners of the cube.
            let v = [
                lookup(l0, l1, l2),
                lookup(l0, l1, h2),
                lookup(l0, h1, l2),
                lookup(l0, h1, h2),
                lookup(h0, l1, l2),
                lookup(h0, l1, h2),
                lookup(h0, h1, l2),
                lookup(h0, h1, h2),
            ];

            // Perform the trilinear interpolation.
            let (wr, wg, wb) = (delta[0], delta[1], delta[2]);
            let one_minus_wr = 1.0 - wr;
            let one_minus_wg = 1.0 - wg;
            let one_minus_wb = 1.0 - wb;

            let lerp = |a: [f32; 4], b: [f32; 4], one_minus_w: f32, w: f32| -> [f32; 4] {
                std::array::from_fn(|k| sse_add(sse_mul(a[k], one_minus_w), sse_mul(b[k], w)))
            };

            // Along the blue axis, then green, then red.
            let blue1 = lerp(v[0], v[1], one_minus_wb, wb);
            let blue2 = lerp(v[2], v[3], one_minus_wb, wb);
            let blue3 = lerp(v[4], v[5], one_minus_wb, wb);
            let blue4 = lerp(v[6], v[7], one_minus_wb, wb);

            let green1 = lerp(blue1, blue2, one_minus_wg, wg);
            let green2 = lerp(blue3, blue4, one_minus_wg, wg);

            let result = lerp(green1, green2, one_minus_wr, wr);

            out[0] = result[0];
            out[1] = result[1];
            out[2] = result[2];
            out[3] = new_alpha;
        }
    }

    /// The `#else` (no SSE2) branch of `Lut3DRenderer::apply`
    /// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:741-818 @ v2.5.2), with `lerp_rgb`
    /// (Lut3DOpCPU.cpp:268-299).
    fn apply_generic(&self, input: &[f32], output: &mut [f32]) {
        let BaseLut3D {
            opt_lut: lut,
            dim,
            step,
            components,
        } = &self.base;
        let dim_minus_one = *dim as f32 - 1.0f32;
        let (dim, components) = (*dim as i32, *components as i32);

        // lerp_rgb(out, a, b, z): (b - a) * z + a, per channel.
        let lerp = |a: [f32; 3], b: [f32; 3], z: f32| -> [f32; 3] {
            std::array::from_fn(|c| sse_add(sse_mul(b[c] - a[c], z), a[c]))
        };

        for (inp, out) in input
            .as_chunks::<4>()
            .0
            .iter()
            .zip(output.as_chunks_mut::<4>().0.iter_mut())
        {
            let new_alpha = inp[3];

            // NaNs become 0.
            let idx = [
                clamp(inp[0] * step, 0.0, dim_minus_one),
                clamp(inp[1] * step, 0.0, dim_minus_one),
                clamp(inp[2] * step, 0.0, dim_minus_one),
            ];

            let low = idx.map(|v| v.floor() as i32);
            // When idx is exactly an index, high is wrong, but the delta is then 0.
            let high = idx.map(|v| v.ceil() as i32);

            let delta: [f32; 3] = std::array::from_fn(|k| idx[k] - low[k] as f32);

            let rgb = |r: i32, g: i32, b: i32| -> [f32; 3] {
                let at = index_blue_fast(r, g, b, dim, components);
                [lut[at], lut[at + 1], lut[at + 2]]
            };
            let n000 = rgb(low[0], low[1], low[2]);
            let n100 = rgb(high[0], low[1], low[2]);
            let n010 = rgb(low[0], high[1], low[2]);
            let n001 = rgb(low[0], low[1], high[2]);
            let n110 = rgb(high[0], high[1], low[2]);
            let n101 = rgb(high[0], low[1], high[2]);
            let n011 = rgb(low[0], high[1], high[2]);
            let n111 = rgb(high[0], high[1], high[2]);

            // lerp_rgb(out, n000, n001, n010, n011, n100, n101, n110, n111, x, y, z).
            let (x, y, z) = (delta[0], delta[1], delta[2]);
            let v1 = lerp(lerp(n000, n001, z), lerp(n010, n011, z), y);
            let v2 = lerp(lerp(n100, n101, z), lerp(n110, n111, z), y);
            let result = lerp(v1, v2, x);

            out[0] = result[0];
            out[1] = result[1];
            out[2] = result[2];
            out[3] = new_alpha;
        }
    }
}

/// A forward 3D LUT renderer.
#[derive(Debug, Clone)]
pub enum ForwardLut3DRenderer {
    /// Tetrahedral interpolation.
    Tetrahedral(Lut3DTetrahedralRenderer),
    /// Trilinear interpolation.
    Trilinear(Lut3DRenderer),
}

impl ForwardLut3DRenderer {
    /// Applies the LUT to packed RGBA pixels (`input.len() / 4` pixels in one call).
    pub fn apply(&self, input: &[f32], output: &mut [f32]) {
        match self {
            ForwardLut3DRenderer::Tetrahedral(r) => r.apply(input, output),
            ForwardLut3DRenderer::Trilinear(r) => r.apply(input, output),
        }
    }
}

/// The renderer OCIO uses for a forward LUT. Port of `GetForwardLut3DRenderer`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:1736-1747 @ v2.5.2).
pub fn get_forward_lut3d_renderer(lut: &Lut3DOpData, cpu: &CpuInfo) -> ForwardLut3DRenderer {
    if lut.concrete_interpolation() == Interpolation::Tetrahedral {
        ForwardLut3DRenderer::Tetrahedral(Lut3DTetrahedralRenderer::new(lut, cpu))
    } else {
        ForwardLut3DRenderer::Trilinear(Lut3DRenderer::new(lut, cpu))
    }
}

#[cfg(test)]
#[path = "lut3d_op_cpu_tests.rs"]
mod tests;
