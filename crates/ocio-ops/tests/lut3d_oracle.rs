// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Spike S4: the forward Lut3D renderers against the wheel, bit for bit.
//!
//! The wheel builds each LUT with `Lut3DTransform.setValue` calls through the `cpu_apply`
//! command and applies it to one row of RGBA F32 pixels, so its renderers see every pixel in
//! one call (`numPixels > 1`): tetrahedral LUTs run the SIMD kernel `CPUInfo` picks, trilinear
//! LUTs the SSE2 code path. The port runs the same rows through the renderer
//! [`get_forward_lut3d_renderer`] builds for this machine.
//!
//! - Sizes 2, 3, 5, 6, 17, 32, 33, 64, 65 and 129; seeded random, identity and extreme LUTs
//!   (finite values: `cpu_apply` sends LUT values as JSON numbers, which have no NaN or
//!   infinity). For sizes 2, 3, 5, 17, 33, 65 and 129, `size - 1` is a power of two, so
//!   `in * m_step` is exact; sizes 6, 32 and 64 round it.
//! - Probes: all 65,536 half values, every combination of 20 special values per channel, and
//!   2^20 random values.
//! - A single-pixel call runs the scalar path instead (`Lut3DTetrahedralRenderer::apply`); the
//!   `generic_*` tests check it with one-pixel images, and the `width_1_*` tests through the
//!   wheel's own scanline loop, on images one pixel wide (the `lut3d_apply` command).
//! - The `nan_inf_*` tests use LUTs that hold NaNs and infinities, which only `lut3d_apply`
//!   can send (as raw float32); OCIO sanitizes them when it builds the renderer.
//! - [`partial_blocks`] checks pixel counts that leave a partial SIMD block.

use ocio_ops::cpu_info::CpuInfo;
use ocio_ops::ops::lut3d::lut3d_op_cpu::{ForwardLut3DRenderer, get_forward_lut3d_renderer};
use ocio_ops::ops::lut3d::lut3d_op_data::{Interpolation, Lut3DArray, Lut3DOpData};
use ocio_testkit::compare::assert_pixels_bits_eq;
use ocio_testkit::oracle::{bytes_to_f32, f32_to_bytes};
use ocio_testkit::{Oracle, probe};
use serde_json::{Value, json};

/// The name of `interp` in PyOpenColorIO.
fn interp_name(interp: Interpolation) -> &'static str {
    match interp {
        Interpolation::Tetrahedral => "INTERP_TETRAHEDRAL",
        Interpolation::Linear => "INTERP_LINEAR",
        other => panic!("no name for {other:?}"),
    }
}

/// The `cpu_apply` transform spec of a Lut3DTransform with `values` (blue fastest).
fn lut_spec(grid_size: u32, values: &[f32], interp: Interpolation) -> Value {
    let name = interp_name(interp);
    let gs = grid_size as usize;
    let mut calls = Vec::with_capacity(gs * gs * gs + 1);
    calls.push(json!(["setInterpolation", {"enum": name}]));
    for r in 0..gs {
        for g in 0..gs {
            for b in 0..gs {
                let at = 3 * ((r * gs + g) * gs + b);
                // f64 JSON numbers carry each f32 exactly.
                calls.push(json!([
                    "setValue",
                    r,
                    g,
                    b,
                    f64::from(values[at]),
                    f64::from(values[at + 1]),
                    f64::from(values[at + 2])
                ]));
            }
        }
    }
    json!({"class": "Lut3DTransform", "args": {"gridSize": grid_size}, "calls": calls})
}

/// The wheel's output for `pixels` (RGBA F32), all in one row.
fn wheel(spec: &Value, pixels: &[f32]) -> Vec<f32> {
    let resp = Oracle::get().call(
        "cpu_apply",
        json!({"transform": spec}),
        &[&f32_to_bytes(pixels)],
    );
    assert!(resp.result.get("exception").is_none(), "{}", resp.result);
    resp.blob_f32(0)
}

/// The wheel's output for `pixels`, one single-pixel image per pixel.
fn wheel_one_pixel_calls(spec: &Value, pixels: &[f32]) -> Vec<f32> {
    let blobs: Vec<Vec<u8>> = pixels.chunks(4).map(f32_to_bytes).collect();
    let calls: Vec<Value> = (0..blobs.len())
        .map(|i| json!({"cmd": "cpu_apply", "args": {"transform": spec}, "blobs": [i]}))
        .collect();
    let refs: Vec<&[u8]> = blobs.iter().map(Vec::as_slice).collect();
    let resp = Oracle::get().call("batch", json!({"calls": calls}), &refs);
    let mut out = Vec::with_capacity(pixels.len());
    for call in resp.result.as_array().expect("batch results") {
        assert!(call["result"].get("exception").is_none(), "{call}");
        let blob = call["blobs"][0].as_u64().expect("an output blob") as usize;
        out.extend(bytes_to_f32(&resp.blobs[blob]));
    }
    out
}

/// The wheel's output for `pixels` through the `lut3d_apply` command, which sends the LUT as
/// raw float32 values (NaNs and infinities included) and lays the pixels out in rows of
/// `width` pixels (by default one row). OCIO's renderers get one row per call.
fn wheel_lut3d_apply(
    values: &[f32],
    interp: Interpolation,
    pixels: &[f32],
    width: Option<usize>,
) -> Vec<f32> {
    let mut args = json!({"interpolation": interp_name(interp)});
    if let Some(width) = width {
        args["width"] = json!(width);
    }
    let resp = Oracle::get().call(
        "lut3d_apply",
        args,
        &[&f32_to_bytes(values), &f32_to_bytes(pixels)],
    );
    assert!(resp.result.get("exception").is_none(), "{}", resp.result);
    // The processor runs the LUT itself: the optimizer neither removed nor replaced it.
    assert_eq!(
        resp.result["optimized_ops"],
        json!(["Lut3DTransform"]),
        "{}",
        resp.result
    );
    resp.blob_f32(0)
}

/// The port's renderer for `values` on `cpu`.
fn renderer(
    grid_size: u32,
    values: &[f32],
    interp: Interpolation,
    cpu: &CpuInfo,
) -> ForwardLut3DRenderer {
    let array = Lut3DArray::from_values(grid_size, values.to_vec()).expect("a valid LUT");
    get_forward_lut3d_renderer(&Lut3DOpData::from_array(interp, array), cpu)
}

/// Applies `renderer` to all `pixels` in one call, as one image row.
fn port(renderer: &ForwardLut3DRenderer, pixels: &[f32]) -> Vec<f32> {
    let mut out = vec![0f32; pixels.len()];
    renderer.apply(pixels, &mut out);
    out
}

/// Seeded random LUT values in [-0.5, 1.5).
fn random_lut(grid_size: u32, seed: u64) -> Vec<f32> {
    probe::uniform(seed, (grid_size as usize).pow(3) * 3, -0.5, 1.5)
}

/// An identity LUT: each entry is its (r, g, b) grid position scaled to [0, 1].
fn identity_lut(grid_size: u32) -> Vec<f32> {
    let gs = grid_size as usize;
    let scale = 1.0 / (gs - 1) as f64;
    let mut v = Vec::with_capacity(gs * gs * gs * 3);
    for r in 0..gs {
        for g in 0..gs {
            for b in 0..gs {
                v.extend([r, g, b].map(|i| (i as f64 * scale) as f32));
            }
        }
    }
    v
}

/// LUT values at the edges of the float range: huge, tiny, denormal, signed zeros.
fn extreme_lut(grid_size: u32, seed: u64) -> Vec<f32> {
    let pool = [
        f32::MAX,
        -f32::MAX,
        f32::MAX / 2.0,
        -f32::MAX / 3.0,
        1e30,
        -1e30,
        1e-30,
        -1e-30,
        f32::from_bits(1),
        -f32::from_bits(1),
        f32::MIN_POSITIVE,
        0.0,
        -0.0,
        65504.0,
        -65504.0,
        1.0,
        -1.0,
        0.5,
    ];
    let mut rng = probe::Rng::new(seed);
    (0..(grid_size as usize).pow(3) * 3)
        .map(|_| {
            if rng.next_u64().is_multiple_of(4) {
                rng.finite_bits()
            } else {
                pool[(rng.next_u64() % pool.len() as u64) as usize]
            }
        })
        .collect()
}

/// A compact set of special channel values.
fn special_channels() -> Vec<f32> {
    vec![
        0.0,
        -0.0,
        f32::NAN,
        -f32::NAN,
        f32::from_bits(0x7f80_0001),
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(1),
        -1e-10,
        0.25,
        0.5,
        f32::from_bits(0.5f32.to_bits() - 1),
        1.0,
        f32::from_bits(1.0f32.to_bits() - 1),
        f32::from_bits(1.0f32.to_bits() + 1),
        2.0,
        -1.0,
        1e30,
        -f32::MAX,
        f32::MAX,
    ]
}

/// All probe pixels: half values, special combinations and random values.
fn probe_pixels(seed: u64) -> Vec<f32> {
    let mut pixels = probe::to_rgba_cycled(&probe::all_half_values());

    let specials = special_channels();
    let mut k = 0;
    for &r in &specials {
        for &g in &specials {
            for &b in &specials {
                pixels.extend([r, g, b, specials[k % specials.len()]]);
                k += 1;
            }
        }
    }

    let mut rng = probe::Rng::new(seed);
    for i in 0..(1 << 18) {
        for _ in 0..4 {
            let v = if i % 4 == 3 {
                rng.any_bits()
            } else {
                rng.uniform(-0.25, 1.25)
            };
            pixels.push(v);
        }
    }
    pixels
}

/// Every LUT kind for `grid_size`, against the wheel, for both interpolations.
fn check_size(grid_size: u32) {
    let pixels = probe_pixels(0x4c55_5433 + u64::from(grid_size));
    let luts = [
        ("random", random_lut(grid_size, u64::from(grid_size))),
        ("identity", identity_lut(grid_size)),
        (
            "extreme",
            extreme_lut(grid_size, 0x4558 + u64::from(grid_size)),
        ),
    ];
    for (kind, values) in &luts {
        for interp in [Interpolation::Tetrahedral, Interpolation::Linear] {
            let expected = wheel(&lut_spec(grid_size, values, interp), &pixels);
            let r = renderer(grid_size, values, interp, CpuInfo::instance());
            let actual = port(&r, &pixels);
            assert_pixels_bits_eq(
                &format!("{kind} {grid_size}^3 LUT, {interp:?}"),
                &pixels,
                4,
                &expected,
                &actual,
            );
        }
    }
}

#[test]
fn size_2() {
    check_size(2);
}

#[test]
fn size_3() {
    check_size(3);
}

#[test]
fn size_5() {
    check_size(5);
}

#[test]
fn size_6() {
    check_size(6);
}

#[test]
fn size_17() {
    check_size(17);
}

#[test]
fn size_32() {
    check_size(32);
}

#[test]
fn size_33() {
    check_size(33);
}

#[test]
fn size_64() {
    check_size(64);
}

#[test]
fn size_65() {
    check_size(65);
}

#[test]
fn size_129() {
    check_size(129);
}

/// Pixels for the single-pixel checks: specials, exact grid positions (ties) and random values.
fn one_pixel_probes(count: usize, seed: u64) -> Vec<f32> {
    let specials = special_channels();
    let mut rng = probe::Rng::new(seed);
    let mut pixels = Vec::new();
    for i in 0..count {
        let pick = |rng: &mut probe::Rng| match i % 4 {
            0 => specials[(rng.next_u64() % specials.len() as u64) as usize],
            // Exact multiples of 1/8 give equal deltas and integer indices.
            1 => (rng.next_u64() % 9) as f32 / 8.0,
            2 => rng.uniform(-0.25, 1.25),
            _ => rng.any_bits(),
        };
        let px = [
            pick(&mut rng),
            pick(&mut rng),
            pick(&mut rng),
            pick(&mut rng),
        ];
        pixels.extend(px);
    }
    pixels
}

/// The scalar path, which OCIO runs for a single pixel, against one-pixel images.
fn check_one_pixel_calls(grid_size: u32, count: usize) {
    let pixels = one_pixel_probes(count, 0x3150 + u64::from(grid_size));
    for (kind, values) in [
        ("random", random_lut(grid_size, u64::from(grid_size))),
        (
            "extreme",
            extreme_lut(grid_size, 0x4558 + u64::from(grid_size)),
        ),
    ] {
        for interp in [Interpolation::Tetrahedral, Interpolation::Linear] {
            let expected = wheel_one_pixel_calls(&lut_spec(grid_size, &values, interp), &pixels);
            let r = renderer(grid_size, &values, interp, CpuInfo::instance());
            let actual: Vec<f32> = pixels.chunks(4).flat_map(|px| port(&r, px)).collect();
            assert_pixels_bits_eq(
                &format!("{kind} {grid_size}^3 LUT, {interp:?}, one pixel per call"),
                &pixels,
                4,
                &expected,
                &actual,
            );
        }
    }
}

#[test]
fn generic_size_2() {
    check_one_pixel_calls(2, 4000);
}

#[test]
fn generic_size_3() {
    check_one_pixel_calls(3, 4000);
}

#[test]
fn generic_size_5() {
    check_one_pixel_calls(5, 2000);
}

#[test]
fn generic_size_6() {
    check_one_pixel_calls(6, 2000);
}

#[test]
fn generic_size_17() {
    check_one_pixel_calls(17, 60);
}

/// The scalar path through the wheel's own scanline loop: on an image one pixel wide, every
/// renderer call gets one pixel.
fn check_width_1(grid_size: u32) {
    let pixels = [
        probe_pixels(0x5749_4431 + u64::from(grid_size)),
        one_pixel_probes(4000, 0x5749_4432 + u64::from(grid_size)),
    ]
    .concat();
    for (kind, values) in [
        ("random", random_lut(grid_size, u64::from(grid_size))),
        (
            "extreme",
            extreme_lut(grid_size, 0x4558 + u64::from(grid_size)),
        ),
    ] {
        for interp in [Interpolation::Tetrahedral, Interpolation::Linear] {
            let expected = wheel_lut3d_apply(&values, interp, &pixels, Some(1));
            let r = renderer(grid_size, &values, interp, CpuInfo::instance());
            let actual: Vec<f32> = pixels.chunks(4).flat_map(|px| port(&r, px)).collect();
            assert_pixels_bits_eq(
                &format!("{kind} {grid_size}^3 LUT, {interp:?}, an image one pixel wide"),
                &pixels,
                4,
                &expected,
                &actual,
            );
        }
    }
}

#[test]
fn width_1_size_2() {
    check_width_1(2);
}

#[test]
fn width_1_size_3() {
    check_width_1(3);
}

#[test]
fn width_1_size_5() {
    check_width_1(5);
}

#[test]
fn width_1_size_6() {
    check_width_1(6);
}

#[test]
fn width_1_size_17() {
    check_width_1(17);
}

#[test]
fn width_1_size_32() {
    check_width_1(32);
}

#[test]
fn width_1_size_33() {
    check_width_1(33);
}

#[test]
fn width_1_size_64() {
    check_width_1(64);
}

#[test]
fn width_1_size_65() {
    check_width_1(65);
}

#[test]
fn width_1_size_129() {
    check_width_1(129);
}

/// LUT values with NaNs (quiet and signaling, either sign, with payloads) and infinities among
/// random values. OCIO sanitizes them when it builds the renderer (`SanitizeFloat`: NaN to 0,
/// infinities to +-FLT_MAX).
fn nan_inf_lut(grid_size: u32, seed: u64) -> Vec<f32> {
    let specials = [
        f32::NAN,
        -f32::NAN,
        f32::from_bits(0x7fc0_1234),
        f32::from_bits(0x7f80_0001),
        f32::from_bits(0xffbf_ffff),
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut rng = probe::Rng::new(seed);
    (0..(grid_size as usize).pow(3) * 3)
        .map(|_| {
            if rng.next_u64().is_multiple_of(3) {
                specials[(rng.next_u64() % specials.len() as u64) as usize]
            } else {
                rng.uniform(-0.5, 1.5)
            }
        })
        .collect()
}

/// A LUT with NaNs and infinities, all pixels in one call and one pixel per call, for both
/// interpolations.
fn check_nan_inf(grid_size: u32) {
    let values = nan_inf_lut(grid_size, 0x4e41_4e49 + u64::from(grid_size));
    let pixels = [
        probe_pixels(0x4e49_4e46 + u64::from(grid_size)),
        one_pixel_probes(4000, 0x4e49_4e47 + u64::from(grid_size)),
    ]
    .concat();
    for interp in [Interpolation::Tetrahedral, Interpolation::Linear] {
        let r = renderer(grid_size, &values, interp, CpuInfo::instance());
        assert_pixels_bits_eq(
            &format!("NaN/Inf {grid_size}^3 LUT, {interp:?}, all pixels in one call"),
            &pixels,
            4,
            &wheel_lut3d_apply(&values, interp, &pixels, None),
            &port(&r, &pixels),
        );
        let actual: Vec<f32> = pixels.chunks(4).flat_map(|px| port(&r, px)).collect();
        assert_pixels_bits_eq(
            &format!("NaN/Inf {grid_size}^3 LUT, {interp:?}, an image one pixel wide"),
            &pixels,
            4,
            &wheel_lut3d_apply(&values, interp, &pixels, Some(1)),
            &actual,
        );
    }
}

#[test]
fn nan_inf_size_2() {
    check_nan_inf(2);
}

#[test]
fn nan_inf_size_6() {
    check_nan_inf(6);
}

#[test]
fn nan_inf_size_33() {
    check_nan_inf(33);
}

/// Calls whose pixel count leaves a partial SIMD block: the AVX-512 kernel's masked last block
/// (and the zero-padded last block of the SSE2, AVX and AVX2 kernels).
#[test]
fn partial_blocks() {
    let grid_size = 17;
    for (kind, values) in [
        ("random", random_lut(grid_size, 0x5041_5254)),
        ("extreme", extreme_lut(grid_size, 0x5041_5255)),
    ] {
        for interp in [Interpolation::Tetrahedral, Interpolation::Linear] {
            let spec = lut_spec(grid_size, &values, interp);
            let r = renderer(grid_size, &values, interp, CpuInfo::instance());
            let counts = [2usize, 3, 5, 7, 15, 17, 31, 33, 47, 63, 65];
            let blobs: Vec<Vec<f32>> = counts
                .iter()
                .map(|&n| one_pixel_probes(n, 0x5041 + n as u64))
                .collect();
            let bytes: Vec<Vec<u8>> = blobs.iter().map(|b| f32_to_bytes(b)).collect();
            let calls: Vec<Value> = (0..bytes.len())
                .map(|i| json!({"cmd": "cpu_apply", "args": {"transform": spec}, "blobs": [i]}))
                .collect();
            let refs: Vec<&[u8]> = bytes.iter().map(Vec::as_slice).collect();
            let resp = Oracle::get().call("batch", json!({"calls": calls}), &refs);
            for ((call, pixels), n) in resp
                .result
                .as_array()
                .expect("batch results")
                .iter()
                .zip(&blobs)
                .zip(counts)
            {
                assert!(call["result"].get("exception").is_none(), "{call}");
                let blob = call["blobs"][0].as_u64().expect("an output blob") as usize;
                let expected = bytes_to_f32(&resp.blobs[blob]);
                assert_pixels_bits_eq(
                    &format!("{kind} LUT, {interp:?}, {n} pixels in one call"),
                    pixels,
                    4,
                    &expected,
                    &port(&r, pixels),
                );
            }
        }
    }
}

/// A zero result whose sign depends on the path: on a 2^3 LUT, the input (0.5, 0.25, 0.5) ties
/// `fx == fz > fy`. The scalar path gives zero weight to n001
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:533-552 @ v2.5.2) and the SIMD kernels to n100
/// (Lut3DOpCPU_SSE2.cpp:140-161), so with these LUT values a single-pixel call gives -0 and a
/// two-pixel call +0, in the wheel and in the port. Found by the verifier
/// (`target/verify/zero_tie.py`).
#[test]
fn fractional_tie_signed_zero() {
    let grid_size = 2;
    let mut values = vec![0.5f32; 2 * 2 * 2 * 3];
    // Red of v000, v001, v100, v101 and v111 (index 3 * ((r * 2 + g) * 2 + b)).
    values[0] = -0.0;
    values[3] = -1.0;
    values[12] = 1.0;
    values[15] = -0.0;
    values[21] = -0.0;
    let spec = lut_spec(grid_size, &values, Interpolation::Tetrahedral);
    let r = renderer(
        grid_size,
        &values,
        Interpolation::Tetrahedral,
        CpuInfo::instance(),
    );

    let one = [0.5f32, 0.25, 0.5, 1.0];
    let two = [one, one].concat();
    assert_pixels_bits_eq(
        "one pixel per call (scalar path)",
        &one,
        4,
        &wheel(&spec, &one),
        &port(&r, &one),
    );
    assert_pixels_bits_eq(
        "two pixels in one call (SIMD kernel)",
        &two,
        4,
        &wheel(&spec, &two),
        &port(&r, &two),
    );
}
