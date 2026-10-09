// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-ins' LUT helpers: a port of `src/OpenColorIO/transforms/builtins/OpHelpers.cpp`
//! @ v2.5.2. The entries whose curves have no closed-form op sample them into 1D LUTs, linearly
//! interpolated: a standard domain of `lutDimension` entries over [0, 1], or a half domain of
//! every half code. The `CreateLut` overload whose generator fills three `double`s
//! (OpHelpers.cpp:76-104) has no caller in v2.5.2, in the library or its tests, and is not
//! ported.

use std::os::raw::c_ulong;

use ocio_ops::exception::{Exception, Result};
use ocio_ops::imath_half::half_to_float;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::lut1d::lut1d_op::create_lut1d_op;
use ocio_ops::ops::lut1d::lut1d_op_data::{HalfFlags, Lut1DOpData};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;

/// `HALF_MAX` (Imath's `half.h`), as `float`.
const HALF_MAX: f32 = 65504.0;

/// The non-uniform 1D linear interpolation of `in_` through `lut_values`, `lut_size` pairs
/// `{in[n], out[n]}`: the first or last output outside the domain. "Invalid interpolation
/// value." where no pair brackets `in_` (a NaN).
///
/// Port of `Interpolate1D` (OpHelpers.cpp:15-52 @ v2.5.2).
pub(crate) fn interpolate_1d(lut_size: usize, lut_values: &[f64], in_: f64) -> Result<f64> {
    // The LUT values are organized that way:
    // { in[0], out[0],
    //   in[1], out[1],
    //   ...
    //   in[N], out[N] }
    //
    // where N is lutSize, and
    // in[n] is lutValues[n * 2] & out[n] is lutValues[n * 2 + 1].

    // Clamp if values are outside the domain of the LUT.
    if in_ < lut_values[0] {
        return Ok(lut_values[1]);
    } else if in_ >= lut_values[2 * (lut_size - 1)] {
        return Ok(lut_values[2 * (lut_size - 1) + 1]);
    }

    for idx in 1..lut_size {
        if in_ < lut_values[2 * idx] {
            let min_idx = 2 * (idx - 1);
            let max_idx = 2 * idx;

            let in_coeff =
                (in_ - lut_values[min_idx]) / (lut_values[max_idx] - lut_values[min_idx]);

            return Ok(
                lut_values[min_idx + 1] * (1.0 - in_coeff) + lut_values[max_idx + 1] * in_coeff
            );
        }
    }

    Err(Exception::new("Invalid interpolation value."))
}

/// `idx / (lutDimension - 1.)`, the standard domain's input of entry `idx`, in `double`.
fn domain_value(idx: c_ulong, lut_dimension: c_ulong) -> f64 {
    idx as f64 / (lut_dimension as f64 - 1.0)
}

/// Appends a forward 1D LUT of `lut_dimension` entries over [0, 1], linear, whose three
/// channels are `lut_value_generator` of the entry's input.
///
/// Port of `CreateLut(OpRcPtrVec &, unsigned long, std::function<float(double)>)`
/// (OpHelpers.cpp:54-74 @ v2.5.2).
pub(crate) fn create_lut(
    ops: &mut OpVec,
    lut_dimension: c_ulong,
    lut_value_generator: impl Fn(f64) -> f32,
) -> Result<()> {
    let mut lut = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, lut_dimension, false)?;
    lut.set_interpolation(Interpolation::Linear);
    lut.set_direction(TransformDirection::Forward);

    let values = lut.get_array_mut().get_values_mut();

    for idx in 0..lut_dimension {
        let i = idx as usize;
        values[i * 3] = lut_value_generator(domain_value(idx, lut_dimension));
        values[i * 3 + 1] = lut_value_generator(domain_value(idx, lut_dimension));
        values[i * 3 + 2] = lut_value_generator(domain_value(idx, lut_dimension));
    }

    create_lut1d_op(ops, lut, TransformDirection::Forward);
    Ok(())
}

/// Appends a forward half-domain 1D LUT (65536 entries, one per half code), linear, whose
/// three channels are `lut_value_generator` of the code's value: 0 for a NaN code, and
/// `HALF_MAX` with the sign of an infinite one.
///
/// Port of `CreateHalfLut` (OpHelpers.cpp:106-139 @ v2.5.2).
pub(crate) fn create_half_lut(
    ops: &mut OpVec,
    lut_value_generator: impl Fn(f64) -> f32,
) -> Result<()> {
    try_create_half_lut(ops, |value| Ok(lut_value_generator(value)))
}

/// [`create_half_lut`] with a generator that can fail, as upstream's can throw: its first
/// error is returned, and nothing is appended to `ops`.
///
/// Port of `CreateHalfLut` (OpHelpers.cpp:106-139 @ v2.5.2).
pub(crate) fn try_create_half_lut(
    ops: &mut OpVec,
    lut_value_generator: impl Fn(f64) -> Result<f32>,
) -> Result<()> {
    let mut lut = Lut1DOpData::with_half_flags(HalfFlags::INPUT_HALF_CODE, 65536, true)?;
    lut.set_interpolation(Interpolation::Linear);
    lut.set_direction(TransformDirection::Forward);

    let lut_dimension = lut.get_array().get_length();
    let values = lut.get_array_mut().get_values_mut();
    for idx in 0..lut_dimension {
        let bits = idx as u16;

        let mut value = f64::from(half_to_float(bits));

        let exponent_all_ones = bits & 0x7c00 == 0x7c00;
        if exponent_all_ones && bits & 0x03ff != 0 {
            // halfValue.isNan()
            value = 0.0;
        } else if exponent_all_ones {
            // halfValue.isInfinity()
            value = f64::from(if bits & 0x8000 != 0 {
                -HALF_MAX
            } else {
                HALF_MAX
            });
        }

        let i = idx as usize;
        values[i * 3] = lut_value_generator(value)?;
        values[i * 3 + 1] = lut_value_generator(value)?;
        values[i * 3 + 2] = lut_value_generator(value)?;
    }

    create_lut1d_op(ops, lut, TransformDirection::Forward);
    Ok(())
}

#[cfg(test)]
#[path = "op_helpers_tests.rs"]
mod tests;
