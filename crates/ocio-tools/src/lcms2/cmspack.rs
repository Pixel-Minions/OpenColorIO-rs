// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the formatters of `src/cmspack.c` @ lcms2.17 that three pixel formats reach:
//! `TYPE_RGB_16`, `TYPE_LabV2_16` and, from the black point detection, an RGB format that it
//! builds and `TYPE_Lab_DBL`.

use super::cmsio0::Profile;
use super::cmspcs::{cms_channels_of_color_space, cms_lcms_color_space};
use super::types::*;

/// Port of `FomLabV2ToLabV4` (src/cmspack.c:45-50 @ lcms2.17).
fn fom_lab_v2_to_lab_v4(x: u16) -> u16 {
    let x = x as i32;
    let a = ((x << 8) | x) >> 8; // * 257 / 256
    if a > 0xffff {
        return 0xffff;
    }
    a as u16
}

/// Port of `FomLabV4ToLabV2` (src/cmspack.c:53-56 @ lcms2.17).
fn fom_lab_v4_to_lab_v2(x: u16) -> u16 {
    ((((x as i32) << 8) + 0x80) / 257) as u16
}

/// Port of `UnrollLabV2_16` (src/cmspack.c:420-434 @ lcms2.17).
pub fn unroll_lab_v2_16(w_in: &mut [u16], accum: &[u16; 3]) {
    w_in[0] = fom_lab_v2_to_lab_v4(accum[0]); // L
    w_in[1] = fom_lab_v2_to_lab_v4(accum[1]); // a
    w_in[2] = fom_lab_v2_to_lab_v4(accum[2]); // b
}

/// Port of `Unroll3Words` (src/cmspack.c:780-794 @ lcms2.17).
pub fn unroll_3_words(w_in: &mut [u16], accum: &[u16; 3]) {
    w_in[0] = accum[0]; // C R
    w_in[1] = accum[1]; // M G
    w_in[2] = accum[2]; // Y B
}

/// Port of `Unroll16ToFloat` (src/cmspack.c:1282-1330 @ lcms2.17), for three channels with no swaps,
/// extra channels or planes: what `cmsFormatterForColorspaceOfProfile` gives for RGB.
pub fn unroll_16_to_float(w_in: &mut [f32], accum: &[u16; 3]) {
    for i in 0..3 {
        let v = accum[i] as f32 / 65535.0f32;
        w_in[i] = v;
    }
}

/// Port of `PackLabV2_16` (src/cmspack.c:2219-2236 @ lcms2.17).
pub fn pack_lab_v2_16(w_out: &[u16], output: &mut [u16; 3]) {
    output[0] = fom_lab_v4_to_lab_v2(w_out[0]);
    output[1] = fom_lab_v4_to_lab_v2(w_out[1]);
    output[2] = fom_lab_v4_to_lab_v2(w_out[2]);
}

/// Port of `Pack3Words` (src/cmspack.c:2303-2320 @ lcms2.17).
pub fn pack_3_words(w_out: &[u16], output: &mut [u16; 3]) {
    output[0] = w_out[0];
    output[1] = w_out[1];
    output[2] = w_out[2];
}

/// Port of `PackLabDoubleFromFloat` (src/cmspack.c:3189-3216 @ lcms2.17), chunky with no extra channels.
pub fn pack_lab_double_from_float(w_out: &[f32]) -> CieLab {
    CieLab {
        l: w_out[0] as f64 * 100.0,
        a: w_out[1] as f64 * 255.0 - 128.0,
        b: w_out[2] as f64 * 255.0 - 128.0,
    }
}

/// Port of `cmsFormatterForColorspaceOfProfile` (src/cmspack.c:4025-4041 @ lcms2.17), not float.
pub fn cms_formatter_for_color_space_of_profile(profile: &Profile, n_bytes: u32) -> u32 {
    let color_space = profile.color_space;
    let color_space_bits = cms_lcms_color_space(color_space);
    let n_output_chans = cms_channels_of_color_space(color_space);

    // Unsupported color space?
    if n_output_chans < 0 {
        return 0;
    }

    // Fix float spaces
    let n_bytes = n_bytes & 7;

    // Create a fake formatter for result
    (color_space_bits << 16) | n_bytes | ((n_output_chans as u32) << 3)
}
