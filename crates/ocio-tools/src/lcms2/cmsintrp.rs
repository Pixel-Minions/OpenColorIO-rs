// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/cmsintrp.c` @ lcms2.17: the interpolators, for 1 to 15 input channels.
//!
//! C keeps the table in the parameters and shifts it for the recursion over dimensions. Here the
//! caller gives the table, and the recursion a `base` offset into it.
//!
//! The fixed-point sums overflow `int` for extreme tables (the source marks those functions
//! `CMS_NO_SANITIZE`); the wheels' machine code wraps, so this wraps.

use super::cmserr::{cms_signal_error, mark_out_of_bounds};
use super::types::*;

/// Which function `DefaultInterpolatorsFactory` chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolator {
    /// `LinLerp1D`, `LinLerp1Dfloat`.
    Lin1D,
    /// `Eval1Input`, `Eval1InputFloat`.
    Eval1,
    /// `BilinearInterp16`, `BilinearInterpFloat`.
    Bilinear,
    /// `TrilinearInterp16`, `TrilinearInterpFloat`.
    Trilinear,
    /// `TetrahedralInterp16`, `TetrahedralInterpFloat`.
    Tetrahedral,
    /// `Eval4Inputs` to `Eval15Inputs`, and their float versions.
    EvalN,
}

/// `cmsInterpParams`, without the table.
#[derive(Debug, Clone)]
pub struct InterpParams {
    pub dw_flags: u32,
    pub n_inputs: u32,
    pub n_outputs: u32,
    pub n_samples: [u32; MAX_INPUT_DIMENSIONS],
    pub domain: [u32; MAX_INPUT_DIMENSIONS],
    pub opta: [u32; MAX_INPUT_DIMENSIONS],
    pub interpolator: Interpolator,
}

/// `LutTable[i]`. Outside the table the wheel reads memory it doesn't own (U-653).
#[inline(always)]
fn at<T: Copy + Default>(table: &[T], i: usize) -> T {
    match table.get(i) {
        Some(value) => *value,
        None => {
            mark_out_of_bounds();
            T::default()
        }
    }
}

/// Port of `DefaultInterpolatorsFactory` (src/cmsintrp.c:1178-1330 @ lcms2.17).
fn default_interpolators_factory(
    n_input_channels: u32,
    n_output_channels: u32,
    dw_flags: u32,
) -> Option<Interpolator> {
    let is_trilinear = dw_flags & CMS_LERP_FLAGS_TRILINEAR != 0;

    // Safety check
    if n_input_channels >= 4 && n_output_channels >= MAX_STAGE_CHANNELS as u32 {
        return None;
    }

    match n_input_channels {
        1 => Some(if n_output_channels == 1 {
            Interpolator::Lin1D
        } else {
            Interpolator::Eval1
        }),
        2 => Some(Interpolator::Bilinear),
        3 => Some(if is_trilinear {
            Interpolator::Trilinear
        } else {
            Interpolator::Tetrahedral
        }),
        4..=15 => Some(Interpolator::EvalN),
        _ => None,
    }
}

/// Port of `_cmsComputeInterpParamsEx` (src/cmsintrp.c:110-157 @ lcms2.17).
pub fn cms_compute_interp_params_ex(
    n_samples: &[u32],
    input_chan: u32,
    output_chan: u32,
    dw_flags: u32,
) -> Option<InterpParams> {
    // Check for maximum inputs
    if input_chan > MAX_INPUT_DIMENSIONS as u32 {
        cms_signal_error(
            CMS_ERROR_RANGE,
            format!("Too many input channels ({input_chan} channels, max={MAX_INPUT_DIMENSIONS})")
                .as_bytes(),
        );
        return None;
    }

    let mut samples = [0u32; MAX_INPUT_DIMENSIONS];
    let mut domain = [0u32; MAX_INPUT_DIMENSIONS];
    let mut opta = [0u32; MAX_INPUT_DIMENSIONS];

    // Fill samples per input direction and domain (which is number of nodes minus one)
    for i in 0..input_chan as usize {
        samples[i] = n_samples[i];
        domain[i] = n_samples[i].wrapping_sub(1);
    }

    // Compute factors to apply to each component to index the grid array
    opta[0] = output_chan;
    for i in 1..input_chan as usize {
        opta[i] = opta[i - 1].wrapping_mul(n_samples[input_chan as usize - i]);
    }

    let Some(interpolator) = default_interpolators_factory(input_chan, output_chan, dw_flags)
    else {
        cms_signal_error(
            CMS_ERROR_UNKNOWN_EXTENSION,
            format!("Unsupported interpolation ({input_chan}->{output_chan} channels)").as_bytes(),
        );
        return None;
    };

    // All seems ok
    Some(InterpParams {
        dw_flags,
        n_inputs: input_chan,
        n_outputs: output_chan,
        n_samples: samples,
        domain,
        opta,
        interpolator,
    })
}

/// Port of `_cmsComputeInterpParams` (src/cmsintrp.c:161-173 @ lcms2.17).
pub fn cms_compute_interp_params(
    n_samples: u32,
    input_chan: u32,
    output_chan: u32,
    dw_flags: u32,
) -> Option<InterpParams> {
    // Fill the auxiliary array
    let samples = [n_samples; MAX_INPUT_DIMENSIONS];
    // Call the extended function
    cms_compute_interp_params_ex(&samples, input_chan, output_chan, dw_flags)
}

/// Port of `LinearInterp` (src/cmsintrp.c:184-189 @ lcms2.17).
#[inline(always)]
fn linear_interp(a: i32, l: i32, h: i32) -> u16 {
    let dif = (h.wrapping_sub(l) as u32)
        .wrapping_mul(a as u32)
        .wrapping_add(0x8000);
    let dif = (dif >> 16).wrapping_add(l as u32);
    dif as u16
}

/// Port of `LinLerp1D` (src/cmsintrp.c:193-221 @ lcms2.17).
fn lin_lerp_1d(value: &[u16], output: &mut [u16], p: &InterpParams, lut_table: &[u16]) {
    // if last value or just one point
    if value[0] == 0xffff || p.domain[0] == 0 {
        output[0] = at(lut_table, p.domain[0] as usize);
    } else {
        let val3 = p.domain[0].wrapping_mul(value[0] as u32) as i32;
        let val3 = cms_to_fixed_domain(val3); // To fixed 15.16

        let cell0 = fixed_to_int(val3); // Cell is 16 MSB bits
        let rest = fixed_rest_to_int(val3); // Rest is 16 LSB bits

        let y0 = at(lut_table, cell0 as isize as usize);
        let y1 = at(lut_table, (cell0 as isize + 1) as usize);

        output[0] = linear_interp(rest, y0 as i32, y1 as i32);
    }
}

/// Port of `fclamp` (src/cmsintrp.c:224-227 @ lcms2.17).
#[inline(always)]
fn fclamp(v: f32) -> f32 {
    if v < 1.0e-9f32 || v.is_nan() {
        0.0
    } else if v > 1.0 {
        1.0
    } else {
        v
    }
}

/// Port of `LinLerp1Dfloat` (src/cmsintrp.c:230-261 @ lcms2.17).
fn lin_lerp_1d_float(value: &[f32], output: &mut [f32], p: &InterpParams, lut_table: &[f32]) {
    let mut val2 = fclamp(value[0]);

    // if last value...
    if val2 == 1.0 || p.domain[0] == 0 {
        output[0] = at(lut_table, p.domain[0] as usize);
    } else {
        val2 *= p.domain[0] as f32;

        let cell0 = val2.floor() as i32;
        let cell1 = val2.ceil() as i32;

        // Rest is 16 LSB bits
        let rest = val2 - cell0 as f32;

        let y0 = at(lut_table, cell0 as usize);
        let y1 = at(lut_table, cell1 as usize);

        output[0] = y0 + (y1 - y0) * rest;
    }
}

/// Port of `Eval1Input` (src/cmsintrp.c:266-306 @ lcms2.17).
fn eval_1_input(input: &[u16], output: &mut [u16], p16: &InterpParams, lut_table: &[u16]) {
    let n_outputs = p16.n_outputs as usize;

    // if last value...
    if input[0] == 0xffff || p16.domain[0] == 0 {
        let y0 = p16.domain[0].wrapping_mul(p16.opta[0]) as usize;
        for out_chan in 0..n_outputs {
            output[out_chan] = at(lut_table, y0 + out_chan);
        }
    } else {
        let v = (input[0] as u32).wrapping_mul(p16.domain[0]) as i32;
        let fk = cms_to_fixed_domain(v);

        let k0 = fixed_to_int(fk);
        let rk = fixed_rest_to_int(fk) as u16 as i32;

        let k1 = k0 + i32::from(input[0] != 0xFFFF);

        let big_k0 = p16.opta[0].wrapping_mul(k0 as u32) as i32;
        let big_k1 = p16.opta[0].wrapping_mul(k1 as u32) as i32;

        for out_chan in 0..n_outputs {
            let l = at(lut_table, (big_k0 as isize + out_chan as isize) as usize);
            let h = at(lut_table, (big_k1 as isize + out_chan as isize) as usize);
            output[out_chan] = linear_interp(rk, l as i32, h as i32);
        }
    }
}

/// Port of `Eval1InputFloat` (src/cmsintrp.c:311-354 @ lcms2.17).
fn eval_1_input_float(value: &[f32], output: &mut [f32], p: &InterpParams, lut_table: &[f32]) {
    let mut val2 = fclamp(value[0]);
    let n_outputs = p.n_outputs as usize;

    // if last value...
    if val2 == 1.0 || p.domain[0] == 0 {
        let start = p.domain[0].wrapping_mul(p.opta[0]) as usize;
        for out_chan in 0..n_outputs {
            output[out_chan] = at(lut_table, start + out_chan);
        }
    } else {
        val2 *= p.domain[0] as f32;

        let cell0 = val2.floor() as i32;
        let cell1 = val2.ceil() as i32;

        // Rest is 16 LSB bits
        let rest = val2 - cell0 as f32;

        let cell0 = cell0.wrapping_mul(p.opta[0] as i32) as usize;
        let cell1 = cell1.wrapping_mul(p.opta[0] as i32) as usize;

        for out_chan in 0..n_outputs {
            let y0 = at(lut_table, cell0 + out_chan);
            let y1 = at(lut_table, cell1 + out_chan);
            output[out_chan] = y0 + (y1 - y0) * rest;
        }
    }
}

/// `LERP` of the float interpolators.
#[inline(always)]
fn lerp_f(a: f32, l: f32, h: f32) -> f32 {
    l + ((h - l) * a)
}

/// `LERP` of the 16-bit interpolators.
#[inline(always)]
fn lerp_16(a: i32, l: i32, h: i32) -> i32 {
    l.wrapping_add(round_fixed_to_int(h.wrapping_sub(l).wrapping_mul(a))) as u16 as i32
}

/// Port of `BilinearInterpFloat` (src/cmsintrp.c:357-407 @ lcms2.17).
fn bilinear_interp_float(input: &[f32], output: &mut [f32], p: &InterpParams, lut_table: &[f32]) {
    let total_out = p.n_outputs as usize;
    let px = fclamp(input[0]) * p.domain[0] as f32;
    let py = fclamp(input[1]) * p.domain[1] as f32;

    let x0 = cms_quick_floor(px as f64);
    let fx = px - x0 as f32;
    let y0 = cms_quick_floor(py as f64);
    let fy = py - y0 as f32;

    let big_x0 = p.opta[1].wrapping_mul(x0 as u32) as usize;
    let big_x1 = big_x0
        + if fclamp(input[0]) >= 1.0 {
            0
        } else {
            p.opta[1] as usize
        };

    let big_y0 = p.opta[0].wrapping_mul(y0 as u32) as usize;
    let big_y1 = big_y0
        + if fclamp(input[1]) >= 1.0 {
            0
        } else {
            p.opta[0] as usize
        };

    for out_chan in 0..total_out {
        let dens = |i: usize, j: usize| at(lut_table, i + j + out_chan);

        let d00 = dens(big_x0, big_y0);
        let d01 = dens(big_x0, big_y1);
        let d10 = dens(big_x1, big_y0);
        let d11 = dens(big_x1, big_y1);

        let dx0 = lerp_f(fx, d00, d10);
        let dx1 = lerp_f(fx, d01, d11);

        output[out_chan] = lerp_f(fy, dx0, dx1);
    }
}

/// Port of `BilinearInterp16` (src/cmsintrp.c:410-466 @ lcms2.17).
fn bilinear_interp_16(input: &[u16], output: &mut [u16], p: &InterpParams, lut_table: &[u16]) {
    let total_out = p.n_outputs as usize;

    let fx = cms_to_fixed_domain((input[0] as u32).wrapping_mul(p.domain[0]) as i32);
    let x0 = fixed_to_int(fx);
    let rx = fixed_rest_to_int(fx); // Rest in 0..1.0 domain

    let fy = cms_to_fixed_domain((input[1] as u32).wrapping_mul(p.domain[1]) as i32);
    let y0 = fixed_to_int(fy);
    let ry = fixed_rest_to_int(fy);

    let big_x0 = p.opta[1].wrapping_mul(x0 as u32) as usize;
    let big_x1 = big_x0
        + if input[0] == 0xFFFF {
            0
        } else {
            p.opta[1] as usize
        };

    let big_y0 = p.opta[0].wrapping_mul(y0 as u32) as usize;
    let big_y1 = big_y0
        + if input[1] == 0xFFFF {
            0
        } else {
            p.opta[0] as usize
        };

    for out_chan in 0..total_out {
        let dens = |i: usize, j: usize| at(lut_table, i + j + out_chan) as i32;

        let d00 = dens(big_x0, big_y0);
        let d01 = dens(big_x0, big_y1);
        let d10 = dens(big_x1, big_y0);
        let d11 = dens(big_x1, big_y1);

        let dx0 = lerp_16(rx, d00, d10);
        let dx1 = lerp_16(rx, d01, d11);

        output[out_chan] = lerp_16(ry, dx0, dx1) as u16;
    }
}

/// Port of `TrilinearInterpFloat` (src/cmsintrp.c:470-540 @ lcms2.17).
fn trilinear_interp_float(input: &[f32], output: &mut [f32], p: &InterpParams, lut_table: &[f32]) {
    let total_out = p.n_outputs as usize;

    // We need some clipping here
    let px = fclamp(input[0]) * p.domain[0] as f32;
    let py = fclamp(input[1]) * p.domain[1] as f32;
    let pz = fclamp(input[2]) * p.domain[2] as f32;

    let x0 = px.floor() as i32; // We need full floor functionality here
    let fx = px - x0 as f32;
    let y0 = py.floor() as i32;
    let fy = py - y0 as f32;
    let z0 = pz.floor() as i32;
    let fz = pz - z0 as f32;

    let big_x0 = p.opta[2].wrapping_mul(x0 as u32) as usize;
    let big_x1 = big_x0
        + if fclamp(input[0]) >= 1.0 {
            0
        } else {
            p.opta[2] as usize
        };

    let big_y0 = p.opta[1].wrapping_mul(y0 as u32) as usize;
    let big_y1 = big_y0
        + if fclamp(input[1]) >= 1.0 {
            0
        } else {
            p.opta[1] as usize
        };

    let big_z0 = p.opta[0].wrapping_mul(z0 as u32) as usize;
    let big_z1 = big_z0
        + if fclamp(input[2]) >= 1.0 {
            0
        } else {
            p.opta[0] as usize
        };

    for out_chan in 0..total_out {
        let dens = |i: usize, j: usize, k: usize| at(lut_table, i + j + k + out_chan);

        let d000 = dens(big_x0, big_y0, big_z0);
        let d001 = dens(big_x0, big_y0, big_z1);
        let d010 = dens(big_x0, big_y1, big_z0);
        let d011 = dens(big_x0, big_y1, big_z1);

        let d100 = dens(big_x1, big_y0, big_z0);
        let d101 = dens(big_x1, big_y0, big_z1);
        let d110 = dens(big_x1, big_y1, big_z0);
        let d111 = dens(big_x1, big_y1, big_z1);

        let dx00 = lerp_f(fx, d000, d100);
        let dx01 = lerp_f(fx, d001, d101);
        let dx10 = lerp_f(fx, d010, d110);
        let dx11 = lerp_f(fx, d011, d111);

        let dxy0 = lerp_f(fy, dx00, dx10);
        let dxy1 = lerp_f(fy, dx01, dx11);

        output[out_chan] = lerp_f(fz, dxy0, dxy1);
    }
}

/// Port of `TrilinearInterp16` (src/cmsintrp.c:543-617 @ lcms2.17).
fn trilinear_interp_16(input: &[u16], output: &mut [u16], p: &InterpParams, lut_table: &[u16]) {
    let total_out = p.n_outputs as usize;

    let fx = cms_to_fixed_domain((input[0] as u32).wrapping_mul(p.domain[0]) as i32);
    let x0 = fixed_to_int(fx);
    let rx = fixed_rest_to_int(fx); // Rest in 0..1.0 domain

    let fy = cms_to_fixed_domain((input[1] as u32).wrapping_mul(p.domain[1]) as i32);
    let y0 = fixed_to_int(fy);
    let ry = fixed_rest_to_int(fy);

    let fz = cms_to_fixed_domain((input[2] as u32).wrapping_mul(p.domain[2]) as i32);
    let z0 = fixed_to_int(fz);
    let rz = fixed_rest_to_int(fz);

    let big_x0 = p.opta[2].wrapping_mul(x0 as u32) as usize;
    let big_x1 = big_x0
        + if input[0] == 0xFFFF {
            0
        } else {
            p.opta[2] as usize
        };

    let big_y0 = p.opta[1].wrapping_mul(y0 as u32) as usize;
    let big_y1 = big_y0
        + if input[1] == 0xFFFF {
            0
        } else {
            p.opta[1] as usize
        };

    let big_z0 = p.opta[0].wrapping_mul(z0 as u32) as usize;
    let big_z1 = big_z0
        + if input[2] == 0xFFFF {
            0
        } else {
            p.opta[0] as usize
        };

    for out_chan in 0..total_out {
        let dens = |i: usize, j: usize, k: usize| at(lut_table, i + j + k + out_chan) as i32;

        let d000 = dens(big_x0, big_y0, big_z0);
        let d001 = dens(big_x0, big_y0, big_z1);
        let d010 = dens(big_x0, big_y1, big_z0);
        let d011 = dens(big_x0, big_y1, big_z1);

        let d100 = dens(big_x1, big_y0, big_z0);
        let d101 = dens(big_x1, big_y0, big_z1);
        let d110 = dens(big_x1, big_y1, big_z0);
        let d111 = dens(big_x1, big_y1, big_z1);

        let dx00 = lerp_16(rx, d000, d100);
        let dx01 = lerp_16(rx, d001, d101);
        let dx10 = lerp_16(rx, d010, d110);
        let dx11 = lerp_16(rx, d011, d111);

        let dxy0 = lerp_16(ry, dx00, dx10);
        let dxy1 = lerp_16(ry, dx01, dx11);

        output[out_chan] = lerp_16(rz, dxy0, dxy1) as u16;
    }
}

/// Port of `TetrahedralInterpFloat` (src/cmsintrp.c:622-716 @ lcms2.17). `domain` is `p->Domain`, or
/// what `Eval4InputsFloat` shifted it to; `base` is how far it moved `p->Table`.
fn tetrahedral_interp_float(
    input: &[f32],
    output: &mut [f32],
    p: &InterpParams,
    domain: &[u32],
    lut_table: &[f32],
    base: usize,
) {
    let total_out = p.n_outputs as usize;

    // We need some clipping here
    let px = fclamp(input[0]) * domain[0] as f32;
    let py = fclamp(input[1]) * domain[1] as f32;
    let pz = fclamp(input[2]) * domain[2] as f32;

    let x0 = px.floor() as i32; // We need full floor functionality here
    let rx = px - x0 as f32;
    let y0 = py.floor() as i32;
    let ry = py - y0 as f32;
    let z0 = pz.floor() as i32;
    let rz = pz - z0 as f32;

    let big_x0 = p.opta[2].wrapping_mul(x0 as u32) as usize;
    let big_x1 = big_x0
        + if fclamp(input[0]) >= 1.0 {
            0
        } else {
            p.opta[2] as usize
        };

    let big_y0 = p.opta[1].wrapping_mul(y0 as u32) as usize;
    let big_y1 = big_y0
        + if fclamp(input[1]) >= 1.0 {
            0
        } else {
            p.opta[1] as usize
        };

    let big_z0 = p.opta[0].wrapping_mul(z0 as u32) as usize;
    let big_z1 = big_z0
        + if fclamp(input[2]) >= 1.0 {
            0
        } else {
            p.opta[0] as usize
        };

    for out_chan in 0..total_out {
        let dens = |i: usize, j: usize, k: usize| at(lut_table, base + i + j + k + out_chan);

        // These are the 6 Tetrahedral

        let c0 = dens(big_x0, big_y0, big_z0);
        let (c1, c2, c3);

        if rx >= ry && ry >= rz {
            c1 = dens(big_x1, big_y0, big_z0) - c0;
            c2 = dens(big_x1, big_y1, big_z0) - dens(big_x1, big_y0, big_z0);
            c3 = dens(big_x1, big_y1, big_z1) - dens(big_x1, big_y1, big_z0);
        } else if rx >= rz && rz >= ry {
            c1 = dens(big_x1, big_y0, big_z0) - c0;
            c2 = dens(big_x1, big_y1, big_z1) - dens(big_x1, big_y0, big_z1);
            c3 = dens(big_x1, big_y0, big_z1) - dens(big_x1, big_y0, big_z0);
        } else if rz >= rx && rx >= ry {
            c1 = dens(big_x1, big_y0, big_z1) - dens(big_x0, big_y0, big_z1);
            c2 = dens(big_x1, big_y1, big_z1) - dens(big_x1, big_y0, big_z1);
            c3 = dens(big_x0, big_y0, big_z1) - c0;
        } else if ry >= rx && rx >= rz {
            c1 = dens(big_x1, big_y1, big_z0) - dens(big_x0, big_y1, big_z0);
            c2 = dens(big_x0, big_y1, big_z0) - c0;
            c3 = dens(big_x1, big_y1, big_z1) - dens(big_x1, big_y1, big_z0);
        } else if ry >= rz && rz >= rx {
            c1 = dens(big_x1, big_y1, big_z1) - dens(big_x0, big_y1, big_z1);
            c2 = dens(big_x0, big_y1, big_z0) - c0;
            c3 = dens(big_x0, big_y1, big_z1) - dens(big_x0, big_y1, big_z0);
        } else if rz >= ry && ry >= rx {
            c1 = dens(big_x1, big_y1, big_z1) - dens(big_x0, big_y1, big_z1);
            c2 = dens(big_x0, big_y1, big_z1) - dens(big_x0, big_y0, big_z1);
            c3 = dens(big_x0, big_y0, big_z1) - c0;
        } else {
            c1 = 0.0;
            c2 = 0.0;
            c3 = 0.0;
        }

        output[out_chan] = c0 + c1 * rx + c2 * ry + c3 * rz;
    }
}

/// Port of `TetrahedralInterp16` (src/cmsintrp.c:720-850 @ lcms2.17).
fn tetrahedral_interp_16(input: &[u16], output: &mut [u16], p: &InterpParams, lut_table: &[u16]) {
    let total_out = p.n_outputs as usize;

    let fx = cms_to_fixed_domain((input[0] as u32).wrapping_mul(p.domain[0]) as i32);
    let fy = cms_to_fixed_domain((input[1] as u32).wrapping_mul(p.domain[1]) as i32);
    let fz = cms_to_fixed_domain((input[2] as u32).wrapping_mul(p.domain[2]) as i32);

    let x0 = fixed_to_int(fx);
    let y0 = fixed_to_int(fy);
    let z0 = fixed_to_int(fz);

    let rx = fixed_rest_to_int(fx);
    let ry = fixed_rest_to_int(fy);
    let rz = fixed_rest_to_int(fz);

    let big_x0 = p.opta[2].wrapping_mul(x0 as u32);
    let mut big_x1 = if input[0] == 0xFFFF { 0 } else { p.opta[2] };

    let big_y0 = p.opta[1].wrapping_mul(y0 as u32);
    let mut big_y1 = if input[1] == 0xFFFF { 0 } else { p.opta[1] };

    let big_z0 = p.opta[0].wrapping_mul(z0 as u32);
    let mut big_z1 = if input[2] == 0xFFFF { 0 } else { p.opta[0] };

    let mut base = big_x0.wrapping_add(big_y0).wrapping_add(big_z0) as usize;

    // Which of the six tetrahedra, and how `c1`, `c2` and `c3` are differenced in it.
    #[derive(Clone, Copy)]
    enum Shape {
        XyZ,
        XzY,
        ZxY,
        YxZ,
        YzX,
        ZyX,
    }

    let shape = if rx >= ry {
        if ry >= rz {
            big_y1 = big_y1.wrapping_add(big_x1);
            big_z1 = big_z1.wrapping_add(big_y1);
            Shape::XyZ
        } else if rz >= rx {
            big_x1 = big_x1.wrapping_add(big_z1);
            big_y1 = big_y1.wrapping_add(big_x1);
            Shape::ZxY
        } else {
            big_z1 = big_z1.wrapping_add(big_x1);
            big_y1 = big_y1.wrapping_add(big_z1);
            Shape::XzY
        }
    } else if rx >= rz {
        big_x1 = big_x1.wrapping_add(big_y1);
        big_z1 = big_z1.wrapping_add(big_x1);
        Shape::YxZ
    } else if ry >= rz {
        big_z1 = big_z1.wrapping_add(big_y1);
        big_x1 = big_x1.wrapping_add(big_z1);
        Shape::YzX
    } else {
        big_y1 = big_y1.wrapping_add(big_z1);
        big_x1 = big_x1.wrapping_add(big_y1);
        Shape::ZyX
    };

    for slot in output.iter_mut().take(total_out) {
        let mut c1 = at(lut_table, base + big_x1 as usize) as i32;
        let mut c2 = at(lut_table, base + big_y1 as usize) as i32;
        let mut c3 = at(lut_table, base + big_z1 as usize) as i32;
        let c0 = at(lut_table, base) as i32;
        base += 1;
        match shape {
            Shape::XyZ => {
                c3 -= c2;
                c2 -= c1;
                c1 -= c0;
            }
            Shape::ZxY => {
                c2 -= c1;
                c1 -= c3;
                c3 -= c0;
            }
            Shape::XzY => {
                c2 -= c3;
                c3 -= c1;
                c1 -= c0;
            }
            Shape::YxZ => {
                c3 -= c1;
                c1 -= c2;
                c2 -= c0;
            }
            Shape::YzX => {
                c1 -= c3;
                c3 -= c2;
                c2 -= c0;
            }
            Shape::ZyX => {
                c1 -= c2;
                c2 -= c3;
                c3 -= c0;
            }
        }
        let rest = c1
            .wrapping_mul(rx)
            .wrapping_add(c2.wrapping_mul(ry))
            .wrapping_add(c3.wrapping_mul(rz))
            .wrapping_add(0x8001);
        *slot = (c0 as u16 as i32).wrapping_add(rest.wrapping_add(rest >> 16) >> 16) as u16;
    }
}

/// One of the two tetrahedral blocks of `Eval4Inputs` (src/cmsintrp.c:854-1032 @ lcms2.17).
#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn eval_4_inputs_block(
    tmp: &mut [u16; MAX_STAGE_CHANNELS],
    n_outputs: usize,
    lut_table: &[u16],
    base: usize,
    (big_x0, big_x1): (i32, i32),
    (big_y0, big_y1): (i32, i32),
    (big_z0, big_z1): (i32, i32),
    (rx, ry, rz): (i32, i32, i32),
) {
    for out_chan in 0..n_outputs {
        let dens =
            |i: i32, j: i32, k: i32| at(lut_table, base + (i + j + k) as usize + out_chan) as i32;

        let c0 = dens(big_x0, big_y0, big_z0);
        let (c1, c2, c3);

        if rx >= ry && ry >= rz {
            c1 = dens(big_x1, big_y0, big_z0) - c0;
            c2 = dens(big_x1, big_y1, big_z0) - dens(big_x1, big_y0, big_z0);
            c3 = dens(big_x1, big_y1, big_z1) - dens(big_x1, big_y1, big_z0);
        } else if rx >= rz && rz >= ry {
            c1 = dens(big_x1, big_y0, big_z0) - c0;
            c2 = dens(big_x1, big_y1, big_z1) - dens(big_x1, big_y0, big_z1);
            c3 = dens(big_x1, big_y0, big_z1) - dens(big_x1, big_y0, big_z0);
        } else if rz >= rx && rx >= ry {
            c1 = dens(big_x1, big_y0, big_z1) - dens(big_x0, big_y0, big_z1);
            c2 = dens(big_x1, big_y1, big_z1) - dens(big_x1, big_y0, big_z1);
            c3 = dens(big_x0, big_y0, big_z1) - c0;
        } else if ry >= rx && rx >= rz {
            c1 = dens(big_x1, big_y1, big_z0) - dens(big_x0, big_y1, big_z0);
            c2 = dens(big_x0, big_y1, big_z0) - c0;
            c3 = dens(big_x1, big_y1, big_z1) - dens(big_x1, big_y1, big_z0);
        } else if ry >= rz && rz >= rx {
            c1 = dens(big_x1, big_y1, big_z1) - dens(big_x0, big_y1, big_z1);
            c2 = dens(big_x0, big_y1, big_z0) - c0;
            c3 = dens(big_x0, big_y1, big_z1) - dens(big_x0, big_y1, big_z0);
        } else if rz >= ry && ry >= rx {
            c1 = dens(big_x1, big_y1, big_z1) - dens(big_x0, big_y1, big_z1);
            c2 = dens(big_x0, big_y1, big_z1) - dens(big_x0, big_y0, big_z1);
            c3 = dens(big_x0, big_y0, big_z1) - c0;
        } else {
            c1 = 0;
            c2 = 0;
            c3 = 0;
        }

        let rest = c1
            .wrapping_mul(rx)
            .wrapping_add(c2.wrapping_mul(ry))
            .wrapping_add(c3.wrapping_mul(rz));

        tmp[out_chan] = c0.wrapping_add(round_fixed_to_int(cms_to_fixed_domain(rest))) as u16;
    }
}

/// Port of `Eval4Inputs` (src/cmsintrp.c:854-1032 @ lcms2.17).
fn eval_4_inputs(
    input: &[u16],
    output: &mut [u16],
    p16: &InterpParams,
    domain: &[u32],
    lut_table: &[u16],
    base: usize,
) {
    let mut tmp1 = [0u16; MAX_STAGE_CHANNELS];
    let mut tmp2 = [0u16; MAX_STAGE_CHANNELS];
    let n_outputs = p16.n_outputs as usize;

    let fk = cms_to_fixed_domain((input[0] as u32).wrapping_mul(domain[0]) as i32);
    let fx = cms_to_fixed_domain((input[1] as u32).wrapping_mul(domain[1]) as i32);
    let fy = cms_to_fixed_domain((input[2] as u32).wrapping_mul(domain[2]) as i32);
    let fz = cms_to_fixed_domain((input[3] as u32).wrapping_mul(domain[3]) as i32);

    let k0 = fixed_to_int(fk);
    let x0 = fixed_to_int(fx);
    let y0 = fixed_to_int(fy);
    let z0 = fixed_to_int(fz);

    let rk = fixed_rest_to_int(fk);
    let rx = fixed_rest_to_int(fx);
    let ry = fixed_rest_to_int(fy);
    let rz = fixed_rest_to_int(fz);

    let big_k0 = p16.opta[3].wrapping_mul(k0 as u32) as i32;
    let big_k1 = big_k0
        + if input[0] == 0xFFFF {
            0
        } else {
            p16.opta[3] as i32
        };

    let big_x0 = p16.opta[2].wrapping_mul(x0 as u32) as i32;
    let big_x1 = big_x0
        + if input[1] == 0xFFFF {
            0
        } else {
            p16.opta[2] as i32
        };

    let big_y0 = p16.opta[1].wrapping_mul(y0 as u32) as i32;
    let big_y1 = big_y0
        + if input[2] == 0xFFFF {
            0
        } else {
            p16.opta[1] as i32
        };

    let big_z0 = p16.opta[0].wrapping_mul(z0 as u32) as i32;
    let big_z1 = big_z0
        + if input[3] == 0xFFFF {
            0
        } else {
            p16.opta[0] as i32
        };

    let xs = (big_x0, big_x1);
    let ys = (big_y0, big_y1);
    let zs = (big_z0, big_z1);
    let rs = (rx, ry, rz);

    eval_4_inputs_block(
        &mut tmp1,
        n_outputs,
        lut_table,
        base + big_k0 as usize,
        xs,
        ys,
        zs,
        rs,
    );
    eval_4_inputs_block(
        &mut tmp2,
        n_outputs,
        lut_table,
        base + big_k1 as usize,
        xs,
        ys,
        zs,
        rs,
    );

    for i in 0..n_outputs {
        output[i] = linear_interp(rk, tmp1[i] as i32, tmp2[i] as i32);
    }
}

/// Port of `Eval4InputsFloat` (src/cmsintrp.c:1038-1078 @ lcms2.17).
fn eval_4_inputs_float(
    input: &[f32],
    output: &mut [f32],
    p: &InterpParams,
    domain: &[u32],
    lut_table: &[f32],
    base: usize,
) {
    let mut tmp1 = [0f32; MAX_STAGE_CHANNELS];
    let mut tmp2 = [0f32; MAX_STAGE_CHANNELS];

    let pk = fclamp(input[0]) * domain[0] as f32;
    let k0 = cms_quick_floor(pk as f64);
    let rest = pk - k0 as f32;

    let big_k0 = p.opta[3].wrapping_mul(k0 as u32) as usize;
    let big_k1 = big_k0
        + if fclamp(input[0]) >= 1.0 {
            0
        } else {
            p.opta[3] as usize
        };

    let inner = &domain[1..];
    tetrahedral_interp_float(&input[1..], &mut tmp1, p, inner, lut_table, base + big_k0);
    tetrahedral_interp_float(&input[1..], &mut tmp2, p, inner, lut_table, base + big_k1);

    for i in 0..p.n_outputs as usize {
        let y0 = tmp1[i];
        let y1 = tmp2[i];
        output[i] = y0 + (y1 - y0) * rest;
    }
}

/// Port of the `Eval##N##Inputs` functions of `EVAL_FNS` (src/cmsintrp.c:1080-1116 @ lcms2.17), for `n`
/// from 5 to 15 inputs, and of `Eval4Inputs` for 4.
fn eval_n_inputs(
    n: usize,
    input: &[u16],
    output: &mut [u16],
    p16: &InterpParams,
    domain: &[u32],
    lut_table: &[u16],
    base: usize,
) {
    if n == 4 {
        eval_4_inputs(input, output, p16, domain, lut_table, base);
        return;
    }
    let nm = n - 1;
    let mut tmp1 = [0u16; MAX_STAGE_CHANNELS];
    let mut tmp2 = [0u16; MAX_STAGE_CHANNELS];

    let fk = cms_to_fixed_domain((input[0] as u32).wrapping_mul(domain[0]) as i32);
    let k0 = fixed_to_int(fk);
    let rk = fixed_rest_to_int(fk);

    let big_k0 = p16.opta[nm].wrapping_mul(k0 as u32) as usize;
    let big_k1 = p16.opta[nm].wrapping_mul((k0 + i32::from(input[0] != 0xFFFF)) as u32) as usize;

    let inner = &domain[1..];
    eval_n_inputs(
        nm,
        &input[1..],
        &mut tmp1,
        p16,
        inner,
        lut_table,
        base + big_k0,
    );
    eval_n_inputs(
        nm,
        &input[1..],
        &mut tmp2,
        p16,
        inner,
        lut_table,
        base + big_k1,
    );

    for i in 0..p16.n_outputs as usize {
        output[i] = linear_interp(rk, tmp1[i] as i32, tmp2[i] as i32);
    }
}

/// Port of the `Eval##N##InputsFloat` functions of `EVAL_FNS` (src/cmsintrp.c:1080-1116 @ lcms2.17), for
/// `n` from 5 to 15 inputs, and of `Eval4InputsFloat` for 4.
fn eval_n_inputs_float(
    n: usize,
    input: &[f32],
    output: &mut [f32],
    p: &InterpParams,
    domain: &[u32],
    lut_table: &[f32],
    base: usize,
) {
    if n == 4 {
        eval_4_inputs_float(input, output, p, domain, lut_table, base);
        return;
    }
    let nm = n - 1;
    let mut tmp1 = [0f32; MAX_STAGE_CHANNELS];
    let mut tmp2 = [0f32; MAX_STAGE_CHANNELS];

    let pk = fclamp(input[0]) * domain[0] as f32;
    let k0 = cms_quick_floor(pk as f64);
    let rest = pk - k0 as f32;

    let big_k0 = p.opta[nm].wrapping_mul(k0 as u32) as usize;
    let big_k1 = big_k0
        + if fclamp(input[0]) >= 1.0 {
            0
        } else {
            p.opta[nm] as usize
        };

    let inner = &domain[1..];
    eval_n_inputs_float(
        nm,
        &input[1..],
        &mut tmp1,
        p,
        inner,
        lut_table,
        base + big_k0,
    );
    eval_n_inputs_float(
        nm,
        &input[1..],
        &mut tmp2,
        p,
        inner,
        lut_table,
        base + big_k1,
    );

    for i in 0..p.n_outputs as usize {
        let y0 = tmp1[i];
        let y1 = tmp2[i];
        output[i] = y0 + (y1 - y0) * rest;
    }
}

impl InterpParams {
    /// `p->Interpolation.Lerp16(input, output, p)`.
    #[inline]
    pub fn lerp16(&self, table: &[u16], input: &[u16], output: &mut [u16]) {
        match self.interpolator {
            Interpolator::Lin1D => lin_lerp_1d(input, output, self, table),
            Interpolator::Eval1 => eval_1_input(input, output, self, table),
            Interpolator::Bilinear => bilinear_interp_16(input, output, self, table),
            Interpolator::Trilinear => trilinear_interp_16(input, output, self, table),
            Interpolator::Tetrahedral => tetrahedral_interp_16(input, output, self, table),
            Interpolator::EvalN => {
                let n = self.n_inputs as usize;
                eval_n_inputs(n, input, output, self, &self.domain, table, 0)
            }
        }
    }

    /// `p->Interpolation.LerpFloat(input, output, p)`.
    #[inline]
    pub fn lerp_float(&self, table: &[f32], input: &[f32], output: &mut [f32]) {
        match self.interpolator {
            Interpolator::Lin1D => lin_lerp_1d_float(input, output, self, table),
            Interpolator::Eval1 => eval_1_input_float(input, output, self, table),
            Interpolator::Bilinear => bilinear_interp_float(input, output, self, table),
            Interpolator::Trilinear => trilinear_interp_float(input, output, self, table),
            Interpolator::Tetrahedral => {
                tetrahedral_interp_float(input, output, self, &self.domain, table, 0)
            }
            Interpolator::EvalN => {
                let n = self.n_inputs as usize;
                eval_n_inputs_float(n, input, output, self, &self.domain, table, 0)
            }
        }
    }
}

/// Port of `_cmsSetInterpolationRoutine` (src/cmsintrp.c:85-106 @ lcms2.17).
pub fn cms_set_interpolation_routine(p: &mut InterpParams) -> bool {
    match default_interpolators_factory(p.n_inputs, p.n_outputs, p.dw_flags) {
        Some(interpolator) => {
            p.interpolator = interpolator;
            true
        }
        None => false,
    }
}

#[cfg(test)]
#[path = "cmsintrp_tests.rs"]
mod tests;
