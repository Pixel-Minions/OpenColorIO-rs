// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The cineSpace `.csp` format: a per-channel prelut (resampled to a 65536-entry 1D LUT
//! through a cubic interpolator) and a 1D or a 3D LUT (red fastest). A port of
//! `src/OpenColorIO/fileformats/FileFormatCSP.cpp` @ v2.5.2.
//!
//! ```text
//! CSPLUTV100
//! 3D
//!
//! BEGIN METADATA
//! ...
//! END METADATA
//!
//! 2
//! 0.0 1.0
//! 0.0 1.0
//! ... (the prelut of each channel: a count, the inputs, the outputs)
//!
//! 2 2 2
//! 0.0 0.0 0.0
//! ...
//! ```
//!
//! The interpolator's arithmetic follows each wheel's machine code, whose operand order
//! decides which NaN a sum or a product of two NaNs returns: MSVC's and GCC's differ, from
//! each other and from the source (`FileFormatCSP.cpp:111-245`).

use std::any::Any;
use std::ffi::c_ulong;
use std::sync::Arc;

use ocio_ops::cfmt::{FloatField, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::image_desc::PackedImageDesc;
use ocio_ops::math_utils::{sse_add, sse_mul, sse_sub, std_max, vecs_equal_with_rel_error};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    Allocation, BitDepth, OptimizationFlags, TransformDirection, combine_transform_directions,
};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op::{create_lut1d_op, generate_identity_lut1d};
use ocio_ops::ops::lut3d::lut3d_op::{
    Lut3DOrder, create_lut3d_op, generate_identity_lut3d, get_lut3d_index_blue_fast,
};
use ocio_ops::ops::lut3d::lut3d_op_data::{Interpolation, Lut3DOpData, MAX_3D_LUT_LENGTH};
use ocio_ops::ops::matrix::matrix_op::create_min_max_op;
use ocio_ops::parse_utils::{string_to_int, string_vec_to_float_vec, string_vec_to_int_vec};
use ocio_ops::utils::string_utils::{split_by_white_spaces, starts_with, trim, upper};

use crate::baker::Baker;
use crate::baking_utils::{
    LinuxResize, bake_buffer, cube_values, get_input_to_target_processor,
    get_shaper_to_input_processor, get_shaper_to_target_processor,
};
use crate::config::Config;
use crate::context::Context;
use crate::fileformats::file_format_utils::{
    handle_lut1d, handle_lut3d, log_warning_interpolation_not_used,
};
use crate::fileformats::input_stream::InputStream;
use crate::lut_limits::MAX_1D_LUT_LENGTH;
use crate::parse_utils::nextline;
use crate::transform::Transform;
use crate::transforms::allocation_transform::AllocationTransform;
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FileFormat, FormatBakeCapabilities, FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;

/// The samples of the 1D LUT a prelut is resampled to: 2^16.
///
/// Port of `NUM_PRELUT_SAMPLES` (FileFormatCSP.cpp:32 @ v2.5.2).
const NUM_PRELUT_SAMPLES: u32 = 65536;

/// The interpolation of the resampled prelut, linear for the best precision.
///
/// Port of `PRELUT_INTERPOLATION` (FileFormatCSP.cpp:35 @ v2.5.2).
const PRELUT_INTERPOLATION: Interpolation = Interpolation::Linear;

/// The cubic interpolator of a prelut: its sample points, and per interval `1/delta, a, b, c,
/// d`, the curve over the interval being `y = a + b*z + c*z^2 + d*z^3` with
/// `z = (x - stims[i]) * (1/delta)`.
///
/// Port of `rsr_Interpolator1D_` (FileFormatCSP.cpp:59-74 @ v2.5.2).
struct Interpolator1D {
    /// `stims`.
    stims: Vec<f32>,
    /// `parameters`: 5 per interval.
    parameters: Vec<f32>,
    /// `minValue`, the value at the first sample.
    min_value: f32,
    /// `maxValue`, the value at the last sample.
    max_value: f32,
}

/// The interval `[low, high]` of `data` holding `x`, halved until it is one interval.
///
/// Port of `rsr_internal_I1D_refineSegment` (FileFormatCSP.cpp:76-89 @ v2.5.2), as a loop;
/// its asserts are compiled out of the wheels.
fn refine_segment(x: f32, data: &[f32], mut low: i32, mut high: i32) -> i32 {
    loop {
        if high - low == 1 {
            return low;
        }

        let mid_point = (low + high) / 2;
        if x < data[mid_point as usize] {
            high = mid_point;
        } else {
            low = mid_point;
        }
    }
}

/// Port of `rsr_internal_I1D_findSegmentContaining` (FileFormatCSP.cpp:91-94 @ v2.5.2).
fn find_segment_containing(x: f32, data: &[f32], n: i32) -> i32 {
    refine_segment(x, data, 0, n - 1)
}

impl Interpolator1D {
    /// The curve at `x`: `x` itself when it is a NaN, the first or last value outside the
    /// samples, else the cubic of its interval, `a + z*(b + z*(c + d*z))` in the operand order
    /// of the wheel's machine code (MSVC: `((z*d + c)*z + b)*z + a`; GCC:
    /// `z*((d*z + c)*z + b) + a`).
    ///
    /// Port of `rsr_Interpolator1D_interpolate` (FileFormatCSP.cpp:111-143 @ v2.5.2).
    fn interpolate(&self, x: f32) -> f32 {
        // Is x in range?
        if x.is_nan() {
            return x;
        }

        let n = self.stims.len();
        if x < self.stims[0] {
            return self.min_value;
        }
        if x > self.stims[n - 1] {
            return self.max_value;
        }

        // Ok so its between the beginning and end .. lets find out where...
        let seg_id = find_segment_containing(x, &self.stims, n as i32) as usize;

        let segdata = &self.parameters[5 * seg_id..5 * seg_id + 5];

        let inv_delta = segdata[0];
        let a = segdata[1];
        let b = segdata[2];
        let c = segdata[3];
        let d = segdata[4];

        let z = sse_mul(x - self.stims[seg_id], inv_delta);

        if cfg!(windows) {
            let m1 = sse_mul(z, d);
            let s1 = sse_add(m1, c);
            let m2 = sse_mul(s1, z);
            let s2 = sse_add(m2, b);
            let m3 = sse_mul(s2, z);
            sse_add(m3, a)
        } else {
            let m1 = sse_mul(d, z);
            let s1 = sse_add(m1, c);
            let m2 = sse_mul(s1, z);
            let s2 = sse_add(m2, b);
            let m3 = sse_mul(z, s2);
            sse_add(m3, a)
        }
    }

    /// The interpolator of the samples `stims` (at least 2) and their `values`: a line for 2
    /// samples, else a cubic per interval whose slopes at its ends are the centered
    /// differences (the one-sided ones at the first and last samples). Each sum is written in
    /// the operand order of the wheel's machine code.
    ///
    /// Port of `rsr_Interpolator1D_createFromRaw` (FileFormatCSP.cpp:145-245 @ v2.5.2).
    fn create_from_raw(stims: &[f32], values: &[f32]) -> Interpolator1D {
        let length = stims.len();
        let retval_stims = stims.to_vec();
        let mut parameters = vec![0.0f32; 5 * (length - 1)];
        let min_value = values[0];
        let max_value = values[length - 1];

        // Now the fun part .. filling in the coefficients.
        if length == 2 {
            parameters[0] = 1.0f32 / (stims[1] - stims[0]);
            parameters[1] = values[0];
            parameters[2] = values[1] - values[0];
            parameters[3] = 0.0;
            parameters[4] = 0.0;
        } else {
            let zero = 0.0f32;
            let win = cfg!(windows);
            for i in 0..length - 1 {
                let params = &mut parameters[5 * i..5 * i + 5];
                let f0 = values[i];
                let f1 = values[i + 1];

                params[0] = 1.0f32 / (retval_stims[i + 1] - retval_stims[i]);

                if i == 0 {
                    let delta = stims[i + 1] - stims[i];
                    let delta2 = (stims[i + 2] - stims[i + 1]) / delta;
                    let f2 = values[i + 2];

                    let dfdx1 = (f2 - f0) / sse_add(delta2, 1.0);
                    // 1*f0 + 0*f1 + 0*dfdx1; -2*f0 + 2*f1 - 1*dfdx1; 1*f0 - 1*f1 + 1*dfdx1.
                    if win {
                        params[1] = sse_add(sse_add(sse_mul(f1, zero), f0), sse_mul(dfdx1, zero));
                        params[2] = sse_sub(sse_sub(sse_add(f1, f1), sse_add(f0, f0)), dfdx1);
                        params[3] = sse_add(sse_sub(f0, f1), dfdx1);
                    } else {
                        params[1] = sse_add(sse_mul(zero, dfdx1), sse_add(sse_mul(f1, zero), f0));
                        params[2] = sse_sub(sse_add(sse_mul(-2.0, f0), sse_add(f1, f1)), dfdx1);
                        params[3] = sse_add(dfdx1, sse_sub(f0, f1));
                    }
                    params[4] = 0.0;
                } else if i == length - 2 {
                    let delta = stims[i + 1] - stims[i];
                    let delta1 = (stims[i] - stims[i - 1]) / delta;
                    let fn1 = values[i - 1];
                    let dfdx0 = (f1 - fn1) / sse_add(delta1, 1.0);
                    // 1*f0 + 0*f1 + 0*dfdx0; 0*f0 + 0*f1 + 1*dfdx0; -1*f0 + 1*f1 - 1*dfdx0.
                    let zf0 = sse_mul(f0, zero);
                    let zf1 = sse_mul(f1, zero);
                    let zdfdx0 = sse_mul(dfdx0, zero);
                    if win {
                        params[1] = sse_add(sse_add(zf1, f0), zdfdx0);
                    } else {
                        params[1] = sse_add(zdfdx0, sse_add(zf1, f0));
                    }
                    params[2] = sse_add(sse_add(zf0, zf1), dfdx0);
                    params[3] = sse_sub(sse_sub(f1, f0), dfdx0);
                    params[4] = 0.0;
                } else {
                    let delta = stims[i + 1] - stims[i];
                    let fn1 = values[i - 1];
                    let delta1 = (stims[i] - stims[i - 1]) / delta;

                    let f2 = values[i + 2];
                    let delta2 = (stims[i + 2] - stims[i + 1]) / delta;

                    let dfdx0 = (f1 - fn1) / sse_add(delta1, 1.0);
                    let dfdx1 = (f2 - f0) / sse_add(delta2, 1.0);

                    // 1*f0 + 0*dfdx0 + 0*f1 + 0*dfdx1; 0*f0 + 1*dfdx0 + 0*f1 + 0*dfdx1;
                    // -3*f0 - 2*dfdx0 + 3*f1 - 1*dfdx1; 2*f0 + 1*dfdx0 - 2*f1 + 1*dfdx1.
                    let zf0 = sse_mul(f0, zero);
                    let zf1 = sse_mul(f1, zero);
                    let zdfdx0 = sse_mul(dfdx0, zero);
                    let zdfdx1 = sse_mul(dfdx1, zero);
                    params[1] = sse_add(sse_add(sse_add(zdfdx0, f0), zf1), zdfdx1);
                    params[2] = sse_add(sse_add(sse_add(zf0, dfdx0), zf1), zdfdx1);
                    params[3] = sse_sub(
                        sse_add(
                            sse_sub(sse_mul(f0, -3.0), sse_add(dfdx0, dfdx0)),
                            sse_mul(f1, 3.0),
                        ),
                        dfdx1,
                    );
                    let two_f0_plus_dfdx0 = if win {
                        sse_add(sse_add(f0, f0), dfdx0)
                    } else {
                        sse_add(dfdx0, sse_add(f0, f0))
                    };
                    params[4] = sse_add(sse_sub(two_f0_plus_dfdx0, sse_add(f1, f1)), dfdx1);
                }
            }
        }

        Interpolator1D {
            stims: retval_stims,
            parameters,
            min_value,
            max_value,
        }
    }
}

/// `lerpf(a, b, z)`, `(b - a) * z + a`, as both wheels compute it: `z * (b - a) + a`.
///
/// Port of `lerpf` (src/OpenColorIO/MathUtils.h:53-56 @ v2.5.2).
fn lerpf(a: f32, b: f32, z: f32) -> f32 {
    sse_add(sse_mul(z, b - a), a)
}

/// A `.csp` file's metadata, prelut and LUT.
///
/// Port of `CachedFileCSP` (FileFormatCSP.cpp:291-307 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct CachedFileCsp {
    /// `metadata`.
    pub metadata: Vec<u8>,
    /// `prelut_from_min`.
    pub prelut_from_min: [f64; 3],
    /// `prelut_from_max`.
    pub prelut_from_max: [f64; 3],
    /// `prelut`.
    pub prelut: Option<Lut1DOpData>,
    /// `lut1D`.
    pub lut1d: Option<Lut1DOpData>,
    /// `lut3D`.
    pub lut3d: Option<Lut3DOpData>,
}

impl CachedFile for CachedFileCsp {}

/// Whether `str`, trimmed and in upper case, starts with `prefix`.
///
/// Port of `startswithU` (FileFormatCSP.cpp:310-314 @ v2.5.2).
fn startswith_u(str: &[u8], prefix: &[u8]) -> bool {
    starts_with(&upper(trim(str)), prefix)
}

/// The cineSpace format.
///
/// Port of `LocalFileFormat` (FileFormatCSP.cpp:316-338 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

/// `bytes` and a message's `parts` as one message.
fn message(parts: &[&[u8]]) -> Exception {
    Exception::new(parts.concat())
}

impl LocalFileFormat {
    /// Reads a `.csp` file: `CSPLUTV100`, `1D` or `3D`, an optional metadata block, the
    /// prelut of each channel, then the 1D or the 3D LUT.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatCSP.cpp:361-699 @ v2.5.2). A metadata block
    /// the file never closes is an error, where upstream reads past the file's end forever
    /// (docs/improvements.md, U-72).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<CachedFileCsp> {
        let mut lut1d_ptr: Option<Lut1DOpData> = None;
        let mut lut3d_ptr: Option<Lut3DOpData> = None;

        // Try and read the LUT header.
        let mut line: Vec<u8> = Vec::new();
        let not_empty = nextline(istream, &mut line);

        if !not_empty {
            return Err(message(&[
                b"File ",
                file_name,
                b": file stream empty when trying to read csp LUT.",
            ]));
        }

        if !startswith_u(&line, b"CSPLUTV100") {
            return Err(message(&[
                b"File ",
                file_name,
                b" doesn't seem to be a csp LUT, expected 'CSPLUTV100'. First line: '",
                &line,
                b"'.",
            ]));
        }

        // Next line tells us if we are reading a 1D or 3D LUT.
        nextline(istream, &mut line);
        if !startswith_u(&line, b"1D") && !startswith_u(&line, b"3D") {
            return Err(message(&[
                b"Unsupported CSP LUT type. Require 1D or 3D. Found, '",
                &line,
                b"' in ",
                file_name,
                b".",
            ]));
        }
        let csptype = line.clone();

        // Read meta data block.
        let mut metadata: Vec<u8> = Vec::new();
        let mut line_update_needed = false;
        nextline(istream, &mut line);
        if startswith_u(&line, b"BEGIN METADATA") {
            // `while (!startswithU(line, "END METADATA") || !istream)`: once the stream
            // fails, every line is empty and upstream never leaves the loop (U-72).
            while !startswith_u(&line, b"END METADATA") || istream.fail() {
                if !nextline(istream, &mut line) {
                    return Err(message(&[
                        b"Metadata block of csp LUT ",
                        file_name,
                        b" is not closed. Expected 'END METADATA' before the end of the file.",
                    ]));
                }
                if !startswith_u(&line, b"END METADATA") {
                    metadata.extend_from_slice(&line);
                    metadata.push(b'\n');
                }
            }
            line_update_needed = true;
        } // Else line update not needed.

        // Make 3 vectors of prelut inputs + output values.
        let mut prelut_in: [Vec<f32>; 3] = Default::default();
        let mut prelut_out: [Vec<f32>; 3] = Default::default();
        let mut useprelut = [false, false, false];

        // Parse the prelut block.
        for c in 0..3 {
            // How many points do we have for this channel.
            if line_update_needed {
                nextline(istream, &mut line);
            }

            let mut cpoints: i32 = 0;

            if !string_to_int(&mut cpoints, &line, false) || cpoints < 0 {
                return Err(message(&[
                    format!("Prelut does not specify valid dimension size on channel '{c}: '")
                        .as_bytes(),
                    &line,
                    b"' in ",
                    file_name,
                    b".",
                ]));
            }

            if cpoints >= 2 {
                nextline(istream, &mut line);
                let inputparts = split_by_white_spaces(trim(&line));

                nextline(istream, &mut line);
                let outputparts = split_by_white_spaces(trim(&line));

                if inputparts.len() as i32 != cpoints || outputparts.len() as i32 != cpoints {
                    return Err(message(&[
                        format!(
                            "Prelut does not specify the expected number of data points. \
                             Expected: {cpoints}.Found: {}, {}. In ",
                            inputparts.len(),
                            outputparts.len()
                        )
                        .as_bytes(),
                        file_name,
                        b".",
                    ]));
                }

                if !string_vec_to_float_vec(&mut prelut_in[c], &inputparts)
                    || !string_vec_to_float_vec(&mut prelut_out[c], &outputparts)
                {
                    return Err(message(&[
                        b"Prelut data is malformed, cannot convert to float array. In ",
                        file_name,
                        b".",
                    ]));
                }

                useprelut[c] = !vecs_equal_with_rel_error(&prelut_in[c], &prelut_out[c], 1e-6f32);
            } else {
                // Even though it's probably not part of the spec, why not allow for a size 0
                // in a channel to be specified?  It should be synonymous with identity, and
                // allows the code lower down to assume all 3 channels exist.

                prelut_in[c].push(0.0);
                prelut_in[c].push(1.0);
                prelut_out[c].push(0.0);
                prelut_out[c].push(1.0);
                useprelut[c] = false;
            }
            line_update_needed = true;
        }

        if csptype == b"1D" {
            // How many 1D LUT points do we have.
            nextline(istream, &mut line);
            let mut points1d: i32 = 0;
            if !string_to_int(&mut points1d, &line, false) {
                return Err(message(&[
                    b"A csp 1D LUT with invalid number of entries (",
                    &line,
                    b") in ",
                    file_name,
                    b".",
                ]));
            }

            if points1d <= 0 || i64::from(points1d) > MAX_1D_LUT_LENGTH as i64 {
                return Err(message(&[
                    format!("A csp 1D LUT with invalid number of entries ({points1d}): ")
                        .as_bytes(),
                    &line,
                    b" . In ",
                    file_name,
                    b".",
                ]));
            }

            let mut lut1d = Lut1DOpData::new(points1d as c_ulong)?;
            if Lut1DOpData::is_valid_interpolation(interp) {
                lut1d.set_interpolation(interp);
            }
            lut1d.set_file_output_bit_depth(BitDepth::F32);

            for i in 0..points1d as usize {
                // Scan for the three floats.
                nextline(istream, &mut line);

                let line_parts = split_by_white_spaces(&line);
                let mut float_array: Vec<f32> = Vec::new();

                if !string_vec_to_float_vec(&mut float_array, &line_parts) || float_array.len() != 3
                {
                    return Err(message(&[
                        b"Malformed 1D csp LUT. Each line of LUT values must contain three \
                          numbers. Line: '",
                        &line,
                        b"'. File: ",
                        file_name,
                        b".",
                    ]));
                }

                // Store each channel.
                let lut_array = lut1d.get_array_mut().get_values_mut();
                lut_array[i * 3] = float_array[0];
                lut_array[i * 3 + 1] = float_array[1];
                lut_array[i * 3 + 2] = float_array[2];
            }
            lut1d_ptr = Some(lut1d);
        } else if csptype == b"3D" {
            // Read the cube size.
            nextline(istream, &mut line);

            let line_parts = split_by_white_spaces(&line);

            let mut cube_size: Vec<i32> = Vec::new();

            if !string_vec_to_int_vec(&mut cube_size, &line_parts) || cube_size.len() != 3 {
                return Err(message(&[
                    b"Malformed 3D csp in LUT file, couldn't read cube size. '",
                    &line,
                    b"'. In file: ",
                    file_name,
                    b".",
                ]));
            }

            // TODO: Support nonuniform cube sizes.
            let lut_size = cube_size[0];
            if lut_size != cube_size[1] || lut_size != cube_size[2] {
                return Err(message(&[
                    format!(
                        "A csp 3D LUT with nonuniform cube sizes is not supported ({}, {}, {}): ",
                        cube_size[0], cube_size[1], cube_size[2]
                    )
                    .as_bytes(),
                    &line,
                    b" .",
                ]));
            }

            if lut_size <= 0 || i64::from(lut_size) > i64::from(MAX_3D_LUT_LENGTH) {
                return Err(message(&[
                    format!("A csp 3D LUT with invalid cube size ({lut_size}): ").as_bytes(),
                    &line,
                    b"' in ",
                    file_name,
                    b".",
                ]));
            }

            let mut lut3d = Lut3DOpData::new(lut_size as c_ulong)?;
            if Lut3DOpData::is_valid_interpolation(interp) {
                lut3d.set_interpolation(interp);
            }
            lut3d.set_file_output_bit_depth(BitDepth::F32);

            let num3dentries = lut_size * lut_size * lut_size;

            let (mut r, mut g, mut b) = (0i32, 0i32, 0i32);

            for i in 0..num3dentries {
                // Load the cube.
                nextline(istream, &mut line);

                // OpData::Lut3D Array index, b changes fastest.
                let array_idx =
                    get_lut3d_index_blue_fast(r, g, b, lut_size, lut_size, lut_size) as usize;

                let line_parts = split_by_white_spaces(&line);
                let mut float_array: Vec<f32> = Vec::new();

                if !string_vec_to_float_vec(&mut float_array, &line_parts) || float_array.len() != 3
                {
                    return Err(message(&[
                        format!("Malformed 3D csp LUT, couldn't read cube row ({i}): ").as_bytes(),
                        &line,
                        b"' in ",
                        file_name,
                        b".",
                    ]));
                }

                let lut_array = lut3d.get_array_mut().get_values_mut();
                lut_array[array_idx] = float_array[0];
                lut_array[array_idx + 1] = float_array[1];
                lut_array[array_idx + 2] = float_array[2];

                // CSP stores the LUT in red-fastest order.
                r += 1;
                if r == lut_size {
                    r = 0;
                    g += 1;
                    if g == lut_size {
                        g = 0;
                        b += 1;
                    }
                }
            }
            lut3d_ptr = Some(lut3d);
        }

        let mut cached_file = CachedFileCsp {
            metadata,
            prelut_from_min: [0.0, 0.0, 0.0],
            prelut_from_max: [1.0, 1.0, 1.0],
            prelut: None,
            lut1d: None,
            lut3d: None,
        };

        if useprelut[0] || useprelut[1] || useprelut[2] {
            let mut prelut_ptr = Lut1DOpData::new(c_ulong::from(NUM_PRELUT_SAMPLES))?;
            prelut_ptr.set_file_output_bit_depth(BitDepth::F32);

            for c in 0..3 {
                let prelut_numpts = prelut_in[c].len();
                let from_min = prelut_in[c][0];
                let from_max = prelut_in[c][prelut_numpts - 1];

                // Create interpolater, to resample to simple 1D LUT.
                let interpolater = Interpolator1D::create_from_raw(&prelut_in[c], &prelut_out[c]);

                // Resample into 1D LUT.
                // TODO: Fancy spline analysis to determine required number of samples.
                cached_file.prelut_from_min[c] = f64::from(from_min);
                cached_file.prelut_from_max[c] = f64::from(from_max);
                let prelut_array = prelut_ptr.get_array_mut().get_values_mut();

                for i in 0..NUM_PRELUT_SAMPLES {
                    let interpo = i as f32 / (NUM_PRELUT_SAMPLES - 1) as f32;
                    let srcval = lerpf(from_min, from_max, interpo);
                    let newval = interpolater.interpolate(srcval);
                    prelut_array[i as usize * 3 + c] = newval;
                }
            }

            prelut_ptr.set_interpolation(PRELUT_INTERPOLATION);
            cached_file.prelut = Some(prelut_ptr);
        }

        if csptype == b"1D" {
            cached_file.lut1d = lut1d_ptr;
        } else if csptype == b"3D" {
            cached_file.lut3d = lut3d_ptr;
        }
        // If file contains neither it throws earlier.

        Ok(cached_file)
    }
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatCSP.cpp:351-359 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo::new(
            "cinespace",
            "csp",
            FormatCapabilities::READ | FormatCapabilities::BAKE,
            FormatBakeCapabilities::LUT_3D | FormatBakeCapabilities::LUT_1D_3D,
        )]
    }

    /// Port of `LocalFileFormat::read` (FileFormatCSP.cpp:361-699 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, file_name, interp)?))
    }

    /// Bakes `baker`'s transform into a cube (32 per side by default, 2 at least), red
    /// fastest, after a prelut: the shaper space's (1024 entries by default) when there is
    /// one, else one made from the input space's allocation (2 entries for a uniform one).
    /// The metadata's children go in the metadata block. The format name isn't checked.
    ///
    /// Port of `LocalFileFormat::bake` (FileFormatCSP.cpp:701-849 @ v2.5.2).
    fn bake(&self, baker: &Baker, _format_name: &[u8], ostream: &mut OStringStream) -> Result<()> {
        const DEFAULT_CUBE_SIZE: i32 = 32;
        const DEFAULT_SHAPER_SIZE: i32 = 1024;

        let config = baker
            .config()
            .expect("Baker::bake checks the config before a format bakes");

        // TODO: Add 1D/3D LUT writing switch, using hasChannelCrosstalk.
        let mut cube_size = baker.cube_size();
        if cube_size == -1 {
            cube_size = DEFAULT_CUBE_SIZE;
        }
        cube_size = std_max(2, cube_size); // smallest cube is 2x2x2

        let mut cube_data = bake_buffer(cube_values(cube_size), LinuxResize::Checked)?;
        let num_entries = cube_size * cube_size * cube_size;
        generate_identity_lut3d(&mut cube_data, cube_size, 3, Lut3DOrder::FastRed)?;

        let mut shaper_in_data: Vec<f32>;
        let mut shaper_out_data: Vec<f32>;

        // Use an explicitly shaper space.
        // TODO: Use the optional allocation for the shaper space,
        //       instead of the implied 0-1 uniform allocation.
        let shaper_space = baker.shaper_space();
        if !shaper_space.is_empty() {
            let mut shaper_size = baker.shaper_size();
            if shaper_size == -1 {
                shaper_size = DEFAULT_SHAPER_SIZE;
            }

            shaper_out_data = bake_buffer(i128::from(shaper_size) * 3, LinuxResize::Checked)?;
            shaper_in_data = bake_buffer(i128::from(shaper_size) * 3, LinuxResize::Checked)?;
            generate_identity_lut1d(&mut shaper_out_data, shaper_size, 3);
            generate_identity_lut1d(&mut shaper_in_data, shaper_size, 3);

            let shaper_to_input = get_shaper_to_input_processor(baker)?;
            let mut shaper_in_img =
                PackedImageDesc::new(&mut shaper_in_data[..], shaper_size as usize, 1, 3)?;
            shaper_to_input.apply(&mut shaper_in_img)?;

            let shaper_to_target = get_shaper_to_target_processor(baker)?;
            let mut cube_img =
                PackedImageDesc::new(&mut cube_data[..], num_entries as usize, 1, 3)?;
            shaper_to_target.apply(&mut cube_img)?;
        } else {
            // A shaper is not specified, let's fake one, using the input space allocation as
            // our guide.

            let input_color_space = config
                .color_space(baker.input_space())
                .expect("Baker::bake checks the input space");

            // Let's make an allocation transform for this colorspace.
            let mut allocation_transform = AllocationTransform::new();
            allocation_transform.set_allocation(input_color_space.allocation());

            // numVars may be '0'.
            let num_vars = input_color_space.allocation_num_vars();
            if num_vars > 0 {
                let vars = input_color_space.allocation_vars().to_vec();
                allocation_transform.set_vars(&vars);
            } else {
                allocation_transform.set_vars(&[]);
            }

            // What size shaper should we make?
            let mut shaper_size = baker.shaper_size();
            if shaper_size == -1 {
                shaper_size = DEFAULT_SHAPER_SIZE;
            }
            shaper_size = std_max(2, shaper_size);
            if input_color_space.allocation() == Allocation::Uniform {
                // This is an awesome optimization.
                // If we know it's a uniform scaling, only 2 points will suffice!
                shaper_size = 2;
            }
            shaper_out_data = bake_buffer(i128::from(shaper_size) * 3, LinuxResize::Checked)?;
            shaper_in_data = bake_buffer(i128::from(shaper_size) * 3, LinuxResize::Checked)?;
            generate_identity_lut1d(&mut shaper_out_data, shaper_size, 3);
            generate_identity_lut1d(&mut shaper_in_data, shaper_size, 3);

            // Apply the forward to the allocation to the output shaper y axis, and the cube
            let shaper_to_input = config
                .processor_in_direction(
                    &Transform::from(allocation_transform),
                    TransformDirection::Inverse,
                )?
                .optimized_cpu_processor(OptimizationFlags::LOSSLESS)?;

            let mut shaper_in_img =
                PackedImageDesc::new(&mut shaper_in_data[..], shaper_size as usize, 1, 3)?;
            shaper_to_input.apply(&mut shaper_in_img)?;
            let mut cube_img =
                PackedImageDesc::new(&mut cube_data[..], num_entries as usize, 1, 3)?;
            shaper_to_input.apply(&mut cube_img)?;

            // Apply the 3D LUT to the remainder (from the input to the output).
            let input_to_target = get_input_to_target_processor(baker)?;
            input_to_target.apply(&mut cube_img)?;
        }

        // Write out the file.
        ostream.put_str("CSPLUTV100\n");
        ostream.put_str("3D\n");
        ostream.put_str("\n");
        ostream.put_str("BEGIN METADATA\n");
        let metadata = baker.format_metadata();
        let nb = metadata.get_num_children_elements();
        for i in 0..nb {
            let child = metadata.get_child_element(i)?;
            ostream.put_bytes(child.get_element_value());
            ostream.put_str("\n");
        }
        ostream.put_str("END METADATA\n");
        ostream.put_str("\n");

        // Write out the 1D Prelut.
        ostream.float_field = FloatField::Fixed;
        ostream.precision = 6;

        if shaper_out_data.len() != shaper_in_data.len() {
            return Err(Exception::new("Internal shaper size exception."));
        }

        if !shaper_in_data.is_empty() {
            let points = shaper_in_data.len() / 3;
            for c in 0..3 {
                ostream.put_u64(points as u64);
                ostream.put_str("\n");
                for i in 0..points {
                    if i != 0 {
                        ostream.put_str(" ");
                    }
                    ostream.put_f32(shaper_in_data[3 * i + c]);
                }
                ostream.put_str("\n");

                for i in 0..points {
                    if i != 0 {
                        ostream.put_str(" ");
                    }
                    ostream.put_f32(shaper_out_data[3 * i + c]);
                }
                ostream.put_str("\n");
            }
        }
        ostream.put_str("\n");

        // Write out the 3D Cube.
        ostream.put_i32(cube_size);
        ostream.put_str(" ");
        ostream.put_i32(cube_size);
        ostream.put_str(" ");
        ostream.put_i32(cube_size);
        ostream.put_str("\n");
        for i in 0..num_entries as usize {
            ostream.put_f32(cube_data[3 * i]);
            ostream.put_str(" ");
            ostream.put_f32(cube_data[3 * i + 1]);
            ostream.put_str(" ");
            ostream.put_f32(cube_data[3 * i + 2]);
            ostream.put_str("\n");
        }
        ostream.put_str("\n");
        Ok(())
    }

    /// Appends, forward, the prelut's min/max op and the prelut, then the 1D or the 3D LUT;
    /// inverse, the LUT, then the prelut and its inverse min/max op.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatCSP.cpp:851-927 @ v2.5.2).
    fn build_file_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        untyped_cached_file: &CachedFileRcPtr,
        file_transform: &FileTransform,
        dir: TransformDirection,
    ) -> Result<()> {
        let any: &dyn Any = untyped_cached_file.as_ref();
        // This should never happen.
        let cached_file = match any.downcast_ref::<CachedFileCsp>() {
            Some(file) if file.prelut.is_some() || file.lut1d.is_some() || file.lut3d.is_some() => {
                file
            }
            _ => return Err(Exception::new("Cannot build CSP Op. Invalid cache type.")),
        };

        let new_dir = combine_transform_directions(dir, file_transform.direction());

        let file_interp = file_transform.interpolation();

        let mut file_interp_used = false;
        let prelut = handle_lut1d(
            cached_file.prelut.as_ref(),
            file_interp,
            &mut file_interp_used,
        );
        let lut1d = handle_lut1d(
            cached_file.lut1d.as_ref(),
            file_interp,
            &mut file_interp_used,
        );
        let lut3d = handle_lut3d(
            cached_file.lut3d.as_ref(),
            file_interp,
            &mut file_interp_used,
        );

        if !file_interp_used {
            log_warning_interpolation_not_used(file_interp, file_transform);
        }

        match new_dir {
            TransformDirection::Forward => {
                if let Some(prelut) = prelut {
                    create_min_max_op(
                        ops,
                        &cached_file.prelut_from_min,
                        &cached_file.prelut_from_max,
                        new_dir,
                    )?;
                    create_lut1d_op(ops, prelut, new_dir);
                }
                if let Some(lut1d) = lut1d {
                    create_lut1d_op(ops, lut1d, new_dir);
                } else if let Some(lut3d) = lut3d {
                    create_lut3d_op(ops, lut3d, new_dir);
                }
            }
            TransformDirection::Inverse => {
                if let Some(lut1d) = lut1d {
                    create_lut1d_op(ops, lut1d, new_dir);
                } else if let Some(lut3d) = lut3d {
                    create_lut3d_op(ops, lut3d, new_dir);
                }
                if let Some(prelut) = prelut {
                    create_lut1d_op(ops, prelut, new_dir);
                    create_min_max_op(
                        ops,
                        &cached_file.prelut_from_min,
                        &cached_file.prelut_from_max,
                        new_dir,
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "file_format_csp_tests.rs"]
mod tests;
