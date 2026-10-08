// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The exact inverse of a 3D LUT (`InvLut3DRenderer`, src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp
//! @ v2.5.2) against the wheel, bit for bit.
//!
//! The default optimization's `OPTIMIZATION_LUT_INV_FAST` replaces an inverse 3D LUT with a
//! forward one (`ReplaceInverseLuts`, `MakeFastLut3DFromInverse`); without it, the processor
//! renders the inverse LUT itself, so only that flag is off here.
//!
//! - **The battery** ([`Inverse`]): LUTs of 2 to 33 entries per side, smooth and invertible,
//!   random, flat, rank-deficient, quantized (flat areas), extreme, and with NaNs and
//!   infinities, with the battery's generated cases. The renderer's search costs up to one
//!   factorization per cube of the LUT for a pixel no cube inverts, so the LUTs whose cubes'
//!   ranges overlap everywhere (random, extreme, NaN/Inf) are small.
//! - **Large grids** ([`large_grids_match_the_wheel`]): 32, and 65 and 127 in the full tier.
//! - **Grid sizes the renderer refuses** ([`grid_sizes_128_and_129_are_refused`]): the
//!   extrapolated LUT is over the 3D LUT's limit (I-153). A grid size of 1 never finishes in
//!   the wheel; the port refuses it (U-65, [`grid_size_1_is_refused`]).
//!
//! The renderer has no SIMD kernel and calls no math library, so these tests don't depend on
//! the CPU.

use core::ffi::c_ulong;

use ocio_ops::ops::lut3d::inv_lut3d::GRID_SIZE_1_INVERSE;
use ocio_ops::ops::lut3d::lut3d_op_cpu::get_lut3d_renderer;
use ocio_ops::ops::lut3d::lut3d_op_data::{Interpolation, Lut3DOpData};
use ocio_testkit::battery::params::{A, Case, Channels, LutEntries, Params, Slot};
use ocio_testkit::battery::{self, Combo, Direction, Family, Port, Spec, Tier};
use ocio_testkit::compare::assert_pixels_bits_eq;
use ocio_testkit::oracle::{BatchCall, f32_to_bytes};
use ocio_testkit::probe::{self, RandomRange, Rng};
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

/// The default optimization's flags but `OPTIMIZATION_LUT_INV_FAST` (fast math changes
/// nothing here).
const EXACT: [&str; 3] = [
    "OPTIMIZATION_LOSSLESS",
    "OPTIMIZATION_COMP_LUT1D",
    "OPTIMIZATION_COMP_SEPARABLE_PREFIX",
];

/// The name of `interp` in PyOpenColorIO.
fn interp_name(interp: Interpolation) -> &'static str {
    match interp {
        Interpolation::Tetrahedral => "INTERP_TETRAHEDRAL",
        Interpolation::Linear => "INTERP_LINEAR",
        other => panic!("no name for {other:?}"),
    }
}

/// An inverse `Lut3DTransform`: its grid size, interpolation (which the inverse ignores) and
/// values (blue fastest, the order of `setData`), with chosen entries as slots.
#[derive(Debug, Clone)]
struct Lut3D {
    grid_size: u32,
    interpolation: Interpolation,
    values: Vec<f32>,
    entries: LutEntries,
}

impl Lut3D {
    fn new(grid_size: u32, interpolation: Interpolation, values: Vec<f32>) -> Lut3D {
        let entries = (grid_size as usize).pow(3);
        Lut3D {
            grid_size,
            interpolation,
            values,
            entries: LutEntries::first_second_middle_last("lut", entries),
        }
    }

    /// The inverse transform, its values as blob 0.
    fn transform(&self) -> Value {
        json!({"class": "Lut3DTransform", "calls": [
            ["setData", {"blob": 0, "dtype": "float32"}],
            ["setInterpolation", {"enum": interp_name(self.interpolation)}],
            ["setDirection", {"enum": "TRANSFORM_DIR_INVERSE"}],
        ]})
    }

    /// The port's data, as `Lut3DTransform::setData`, `setInterpolation` and `setDirection`
    /// make it, and `BuildLut3DOp` validates and inverts it
    /// (src/OpenColorIO/ops/lut3d/Lut3DOp.cpp:251-261 and 222-249 @ v2.5.2).
    fn data(&self) -> Lut3DOpData {
        let mut lut =
            Lut3DOpData::with_interpolation(self.interpolation, c_ulong::from(self.grid_size))
                .expect("a valid LUT");
        let lut_values = lut.get_array_mut().get_values_mut();
        assert_eq!(lut_values.len(), self.values.len(), "the LUT's values");
        lut_values.copy_from_slice(&self.values);
        lut.validate().expect("a valid LUT");
        std::hint::black_box(lut.inverse())
    }

    /// The wheel's `cpu_apply` request for `pixels`.
    fn request(&self, pixels: &[f32]) -> (Value, Vec<Vec<u8>>) {
        (
            json!({"transform": self.transform(), "optimization": EXACT}),
            vec![f32_to_bytes(pixels), f32_to_bytes(&self.values)],
        )
    }
}

impl Params for Lut3D {
    fn slots(&self) -> Vec<Slot> {
        self.entries.slots()
    }
    fn get(&self, index: usize) -> f64 {
        self.entries.get(&self.values, index)
    }
    fn set(&mut self, index: usize, value: f64) {
        self.entries.set(&mut self.values, index, value);
    }
}

/// A blue-fastest LUT of `grid_size` entries per side, each entry `f` of its grid position
/// scaled to [0, 1].
fn lut_of(grid_size: u32, f: impl Fn([f64; 3]) -> [f64; 3]) -> Vec<f32> {
    let gs = grid_size as usize;
    let scale = 1.0 / (gs - 1) as f64;
    let mut v = Vec::with_capacity(gs * gs * gs * 3);
    for r in 0..gs {
        for g in 0..gs {
            for b in 0..gs {
                let x = [r, g, b].map(|i| i as f64 * scale);
                v.extend(f(x).map(|c| c as f32));
            }
        }
    }
    v
}

/// Smooth, monotonic and invertible, with some crosstalk: the kind of LUT one inverts.
fn smooth(x: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = x;
    let curve = |c: f64| 0.06 + 0.88 * (0.6 * c * c + 0.4 * c);
    [
        curve(r) + 0.03 * (g - b),
        curve(g) + 0.03 * (b - r),
        curve(b) + 0.03 * (r - g),
    ]
}

/// Each channel rounded down to a quarter: flat areas, where the inverse is not unique.
fn quantized(x: [f64; 3]) -> [f64; 3] {
    x.map(|c| (c * 4.0).floor() / 4.0)
}

/// Red in red and green: a LUT of rank 2, whose cubes are flat.
fn rank_deficient(x: [f64; 3]) -> [f64; 3] {
    [x[0], x[0], x[2]]
}

/// Seeded random values in [-0.5, 1.5).
fn random_lut(grid_size: u32, seed: u64) -> Vec<f32> {
    probe::uniform(seed, (grid_size as usize).pow(3) * 3, -0.5, 1.5)
}

/// Values at the edges of the float range: huge, tiny, denormal, signed zeros.
fn extreme_lut(grid_size: u32, seed: u64) -> Vec<f32> {
    let pool = [
        f32::MAX,
        -f32::MAX,
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
        1.0,
        -1.0,
        0.5,
    ];
    let mut rng = Rng::new(seed);
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

/// A smooth LUT with NaNs (quiet and signaling, either sign, with payloads) and infinities in
/// a third of its values. The inverse uses them as they are (no `SanitizeFloat`).
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
    let mut rng = Rng::new(seed);
    lut_of(grid_size, smooth)
        .into_iter()
        .map(|v| {
            if rng.next_u64().is_multiple_of(3) {
                specials[(rng.next_u64() % specials.len() as u64) as usize]
            } else {
                v
            }
        })
        .collect()
}

/// Random values among 0, 0.25 and 0.5: cubes with coinciding corners, whose factorization
/// meets zero pivots and takes the rank-revealing path.
fn quarters_lut(grid_size: u32, seed: u64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    (0..(grid_size as usize).pow(3) * 3)
        .map(|_| (rng.next_u64() % 3) as f32 * 0.25)
        .collect()
}

/// A slightly irregular ramp with one NaN, in channel `seed % 3` of an entry chosen by `seed`:
/// the tree's ranges keep a NaN met first (C++'s `std::min` and `std::max`), so the cube
/// whose base entry it is never matches, and nor does the rest of the first sub-tree it
/// starts.
fn one_nan_lut(grid_size: u32, seed: u64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    let mut values = lut_of(grid_size, |x| x.map(|c| 0.05 + 0.9 * c))
        .into_iter()
        .map(|v| v + rng.uniform(-0.02, 0.02))
        .collect::<Vec<_>>();
    let entry = (rng.next_u64() % (grid_size as u64).pow(3)) as usize;
    values[entry * 3 + (seed % 3) as usize] = f32::NAN;
    values
}

/// The LUTs of `grid_size`: the kinds whose search stays cheap at any size, and the others up
/// to 9 entries per side.
fn luts(grid_size: u32) -> Vec<(&'static str, Vec<f32>)> {
    let seed = u64::from(grid_size);
    let mut luts = vec![
        ("smooth", lut_of(grid_size, smooth)),
        ("identity", lut_of(grid_size, |x| x)),
        ("flat", lut_of(grid_size, |_| [0.5, 0.25, 0.75])),
        ("rank 2", lut_of(grid_size, rank_deficient)),
        ("quantized", lut_of(grid_size, quantized)),
    ];
    if grid_size <= 9 {
        luts.extend([
            ("random", random_lut(grid_size, seed)),
            ("NaN/Inf", nan_inf_lut(grid_size, 0x4e41_4e49 + seed)),
            ("quarters", quarters_lut(grid_size, 0x5175 + seed)),
            ("one NaN", one_nan_lut(grid_size, 0x314e + seed)),
            (
                "one NaN, another",
                one_nan_lut(grid_size, 0x314e_0001 + seed),
            ),
        ]);
    }
    if grid_size <= 5 {
        luts.push(("extreme", extreme_lut(grid_size, 0x4558 + seed)));
    }
    luts
}

/// The exact inverse renderer, F32 to F32, through the battery. It writes red, green and blue
/// only, so alpha passes through; it has one profile.
struct Inverse;

impl Family for Inverse {
    type Params = Lut3D;

    fn name(&self) -> String {
        "Lut3DTransform (inverse, without OPTIMIZATION_LUT_INV_FAST)".to_string()
    }
    fn cases(&self) -> Vec<Case<Lut3D>> {
        let mut cases = Vec::new();
        for grid_size in [2, 3, 4, 5, 9, 17, 33] {
            for (kind, values) in luts(grid_size) {
                cases.push(Case::new(
                    format!("{kind} {grid_size}^3"),
                    Lut3D::new(grid_size, Interpolation::Tetrahedral, values),
                ));
            }
        }
        // The inverse ignores the interpolation.
        cases.push(Case::new(
            "smooth 5^3, trilinear",
            Lut3D::new(5, Interpolation::Linear, lut_of(5, smooth)),
        ));
        cases
    }
    fn mutation_bases(&self) -> Vec<Case<Lut3D>> {
        self.cases()
            .into_iter()
            .filter(|case| case.label() == "smooth 5^3")
            .collect()
    }
    fn directions(&self) -> Vec<Direction> {
        vec![Direction::Inverse]
    }
    fn optimization_off(&self) -> Vec<&'static str> {
        vec!["OPTIMIZATION_LUT_INV_FAST"]
    }
    fn spec(&self, lut: &Lut3D, _direction: Direction) -> Spec {
        Spec::with_f32_blobs(lut.transform(), &[&lut.values])
    }
    fn port(&self, lut: &Lut3D, _combo: &Combo) -> Result<Port, String> {
        let renderer = get_lut3d_renderer(&lut.data()).map_err(|e| e.message().to_string())?;
        Ok(Port::in_place(move |px| renderer.apply(px)))
    }
    fn pass_through(&self, _lut: &Lut3D, _combo: &Combo) -> Channels {
        A
    }
    fn breakpoints(&self, lut: &Lut3D, _direction: Direction) -> Vec<f32> {
        // The LUT's values the clamped input can reach, and the clamp's ends.
        let mut points: Vec<f32> = lut
            .values
            .iter()
            .copied()
            .filter(|v| (0.0..=1.0).contains(v))
            .chain([0.0, 1.0])
            .collect();
        points.sort_by(f32::total_cmp);
        points.dedup_by(|a, b| a.to_bits() == b.to_bits());
        let step = points.len().div_ceil(48).max(1);
        points.into_iter().step_by(step).collect()
    }
}

#[test]
fn inverse_matches_the_wheel() {
    battery::run(&Inverse);
}

/// Pixels for the large grids: the specials and random values in [0, 1] and around it.
fn probe_pixels() -> Vec<f32> {
    let mut values = probe::specials();
    values.extend(probe::random(
        0x1a7_3d1f,
        &[(RandomRange::Unit, 768), (RandomRange::Overshoot, 128)],
    ));
    probe::to_rgba_cycled(&values)
}

/// The port's output for `pixels` through `lut`'s renderer, in one call.
fn port(lut: &Lut3D, pixels: &[f32]) -> Result<Vec<f32>, String> {
    let renderer = get_lut3d_renderer(&lut.data()).map_err(|e| e.message().to_string())?;
    let mut out = pixels.to_vec();
    renderer.apply(&mut out);
    Ok(out)
}

/// `luts` through the wheel and the port on `pixels`, in one batch: the outputs bit for bit,
/// or the same exception.
fn check(luts: &[(String, Lut3D)], pixels: &[f32]) {
    let requests: Vec<(Value, Vec<Vec<u8>>)> =
        luts.iter().map(|(_, lut)| lut.request(pixels)).collect();
    let calls: Vec<BatchCall<'_>> = requests
        .iter()
        .map(|(args, blobs)| BatchCall {
            cmd: "cpu_apply",
            args: args.clone(),
            blobs: blobs.iter().map(Vec::as_slice).collect(),
        })
        .collect();
    let responses = Oracle::get().batch(&calls, true);
    for ((label, lut), response) in luts.iter().zip(responses) {
        let response = response.unwrap_or_else(|e| panic!("{label}: {e}"));
        match (response.result.get("exception"), port(lut, pixels)) {
            (None, Ok(out)) => assert_pixels_bits_eq(label, pixels, 4, &response.blob_f32(0), &out),
            (Some(exception), Err(message)) => assert_text_eq(
                &format!("{label}: the exception"),
                exception["message"].as_str().expect("a message"),
                &message,
            ),
            (wheel, port) => panic!(
                "{label}: the wheel gives {wheel:?}, the port {:?}",
                port.map(|_| "pixels")
            ),
        }
    }
}

/// Grids too large to send with each of the battery's buffers, smooth and quantized, on a set
/// of pixels in one call. In the full and exhaustive tiers only, but for 32^3.
#[test]
fn large_grids_match_the_wheel() {
    let sizes: &[u32] = if Tier::current() == Tier::Quick {
        &[32]
    } else {
        &[32, 65, 127]
    };
    let mut luts = Vec::new();
    for &grid_size in sizes {
        for (kind, f) in [
            ("smooth", smooth as fn([f64; 3]) -> [f64; 3]),
            ("quantized", quantized),
        ] {
            luts.push((
                format!("{kind} {grid_size}^3"),
                Lut3D::new(grid_size, Interpolation::Tetrahedral, lut_of(grid_size, f)),
            ));
        }
    }
    check(&luts, &probe_pixels());
}

/// LUTs of random quarters (cubes with coinciding corners) on pixels whose channels are on and
/// near the quarters, where such cubes' ranges are: each pixel of the 5^3 grid of quarters,
/// and of it moved by up to 0.2 in each channel.
#[test]
fn degenerate_cubes_match_the_wheel() {
    let mut values = Vec::new();
    for r in 0..5 {
        for g in 0..5 {
            for b in 0..5 {
                values.extend([r, g, b].map(|q| q as f32 * 0.25));
                values.push(1.0);
            }
        }
    }
    let mut rng = Rng::new(0x5175_0000);
    let grid = values.clone();
    for _ in 0..4 {
        values.extend(grid.iter().enumerate().map(|(i, v)| {
            if i % 4 == 3 {
                *v
            } else {
                v + rng.uniform(-0.2, 0.2)
            }
        }));
    }
    // Seeds 0 to 3 of each size, and seeds whose LUTs have cubes where the rank-revealing loop
    // of `invert_hypercube` swaps a column in, then back out (Lut3DOpCPU.cpp:953-972 @ v2.5.2),
    // with these pixels: found by a search of the port, and checked here against the wheel.
    let mut luts = Vec::new();
    for (grid_size, swapped_back) in [
        (2, [420, 475, 514, 604]),
        (3, [56, 77, 100, 216]),
        (4, [33, 52, 61, 211]),
    ] {
        for seed in (0..4u64).chain(swapped_back) {
            luts.push((
                format!("quarters {grid_size}^3, seed {seed}"),
                Lut3D::new(
                    grid_size,
                    Interpolation::Tetrahedral,
                    quarters_lut(grid_size, 0x5175_0100 + seed),
                ),
            ));
        }
    }
    check(&luts, &values);
}

/// LUTs of 128 and 129 entries per side: the renderer's extrapolated LUT has 130 or 131,
/// over the 3D LUT's limit, and its constructor throws the 3D LUT's error (I-153).
#[test]
fn grid_sizes_128_and_129_are_refused() {
    let luts: Vec<(String, Lut3D)> = [128, 129]
        .into_iter()
        .map(|grid_size| {
            (
                format!("identity {grid_size}^3"),
                Lut3D::new(
                    grid_size,
                    Interpolation::Tetrahedral,
                    lut_of(grid_size, |x| x),
                ),
            )
        })
        .collect();
    check(&luts, &probe::to_rgba_cycled(&[0.25, 0.5, 0.75]));
}

/// A LUT of 1 entry per side: upstream's extrapolation steps by `dim - 1` and never ends, in
/// both wheels (U-65); the port refuses it instead. Not compared with the wheel, which would
/// not return.
#[test]
fn grid_size_1_is_refused() {
    let lut = Lut3D::new(1, Interpolation::Tetrahedral, vec![0.25, 0.5, 0.75]);
    assert_eq!(
        port(&lut, &[0.5, 0.5, 0.5, 1.0]),
        Err(GRID_SIZE_1_INVERSE.to_string())
    );
}
