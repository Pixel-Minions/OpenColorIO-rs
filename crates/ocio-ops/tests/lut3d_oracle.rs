// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The forward Lut3D renderers (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp,
//! Lut3DOpCPU_SSE2.cpp, _AVX, _AVX2, _AVX512 @ v2.5.2) against the wheel, bit for bit.
//!
//! - **The battery** ([`Lut3D`]): random, identity, extreme and NaN/Inf LUTs of 2 to 33 entries
//!   per side, tetrahedral and trilinear, with the generated cases of the battery (extreme
//!   finite, NaN and infinite values in chosen entries), on the tier's probes and on rows of 1
//!   to 33 pixels: a tetrahedral row of more than one pixel runs the SIMD kernel `CPUInfo`
//!   picks, a row of one pixel the scalar path, and a trilinear row the build's SSE2 path.
//!   The oracle builds a `Lut3DTransform` from the values, passed as a blob (`setData`), so
//!   NaNs and infinities reach it; OCIO sanitizes them when it builds the renderer. Every
//!   renderer moves alpha without arithmetic, so alpha passes through, and the profiles this
//!   machine doesn't dispatch to (the other tetrahedral kernels, the generic tetrahedral and
//!   trilinear paths) are compared on it.
//! - **Large grids** ([`large_grids_match_the_wheel`]): sizes 32, 64, 65 and 129, whose LUTs
//!   are too large to send with each of the battery's buffers, on the probes of spike S4 in
//!   one call.
//! - [`fractional_tie_signed_zero`]: a zero whose sign depends on the path.
//!
//! These tests depend on the CPU (`cpu-tests`): under SDE the wheel and the port both dispatch
//! to the emulated CPU's kernel.

use core::ffi::c_ulong;
use std::sync::Arc;

use ocio_ops::cpu_info::{
    BuildConfig, CpuInfo, X86_CPU_FLAG_AVX, X86_CPU_FLAG_AVX2, X86_CPU_FLAG_AVX512,
    X86_CPU_FLAG_SSE2,
};
use ocio_ops::op::CpuOp;
use ocio_ops::ops::lut3d::lut3d_op_cpu::{
    ForwardLut3DRenderer, get_forward_lut3d_renderer, get_lut3d_renderer,
};
use ocio_ops::ops::lut3d::lut3d_op_data::{Interpolation, Lut3DOpData};
use ocio_testkit::battery::params::Slot;
use ocio_testkit::battery::params::{A, Case, Channels as BatteryChannels, LutEntries, Params};
use ocio_testkit::battery::{self, Combo, Direction, Family, Port, Spec, Tier};
use ocio_testkit::compare::assert_pixels_bits_eq;
use ocio_testkit::oracle::f32_to_bytes;
use ocio_testkit::probe::{self, ProbeSet};
use ocio_testkit::{Oracle, probe::Rng};
use serde_json::{Value, json};

/// The name of `interp` in PyOpenColorIO.
fn interp_name(interp: Interpolation) -> &'static str {
    match interp {
        Interpolation::Tetrahedral => "INTERP_TETRAHEDRAL",
        Interpolation::Linear => "INTERP_LINEAR",
        other => panic!("no name for {other:?}"),
    }
}

/// A `Lut3DTransform`: its grid size, interpolation and values (blue fastest, the order of
/// `setData`), with chosen entries as slots.
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

    /// The transform, its values as blob 0.
    fn transform(&self) -> Value {
        json!({"class": "Lut3DTransform", "calls": [
            ["setData", {"blob": 0, "dtype": "float32"}],
            ["setInterpolation", {"enum": interp_name(self.interpolation)}],
        ]})
    }

    /// The port's data, as `Lut3DTransform::setData` and `setInterpolation` make it, and
    /// `BuildLut3DOp` validates it (src/OpenColorIO/ops/lut3d/Lut3DOp.cpp:251-261 @ v2.5.2).
    fn data(&self) -> Lut3DOpData {
        let mut lut =
            Lut3DOpData::with_interpolation(self.interpolation, c_ulong::from(self.grid_size))
                .expect("a valid LUT");
        let lut_values = lut.get_array_mut().get_values_mut();
        assert_eq!(lut_values.len(), self.values.len(), "the LUT's values");
        lut_values.copy_from_slice(&self.values);
        lut.validate().expect("a valid LUT");
        std::hint::black_box(lut)
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
    let mut rng = Rng::new(seed);
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

/// The LUTs of `grid_size` of each kind: random, identity, extreme, NaN/Inf.
fn luts(grid_size: u32) -> [(&'static str, Vec<f32>); 4] {
    let seed = u64::from(grid_size);
    [
        ("random", random_lut(grid_size, seed)),
        ("identity", identity_lut(grid_size)),
        ("extreme", extreme_lut(grid_size, 0x4558 + seed)),
        ("NaN/Inf", nan_inf_lut(grid_size, 0x4e41_4e49 + seed)),
    ]
}

/// The longest row the battery probes for the kernels: every remainder of 4, 8 and 16 pixels.
const ROWS: usize = 33;

/// The forward renderers, F32 to F32, through the battery.
struct Lut3DFamily;

impl Family for Lut3DFamily {
    type Params = Lut3D;

    fn name(&self) -> String {
        "Lut3DTransform".to_string()
    }
    fn cases(&self) -> Vec<Case<Lut3D>> {
        let mut cases = Vec::new();
        for grid_size in [2, 3, 5, 6, 17, 33] {
            for (kind, values) in luts(grid_size) {
                for interp in [Interpolation::Tetrahedral, Interpolation::Linear] {
                    cases.push(Case::new(
                        format!("{kind} {grid_size}^3, {interp:?}"),
                        Lut3D::new(grid_size, interp, values.clone()),
                    ));
                }
            }
        }
        cases
    }
    fn mutation_bases(&self) -> Vec<Case<Lut3D>> {
        // The random 17^3 LUTs, tetrahedral and trilinear.
        self.cases()
            .into_iter()
            .filter(|case| case.label().starts_with("random 17^3"))
            .collect()
    }
    fn directions(&self) -> Vec<Direction> {
        // The inverse LUT's renderer and fast forward LUT are WP 2.2d's and 2.2e's.
        vec![Direction::Forward]
    }
    fn spec(&self, lut: &Lut3D, _direction: Direction) -> Spec {
        Spec::with_f32_blobs(lut.transform(), &[&lut.values])
    }
    fn port(&self, lut: &Lut3D, _combo: &Combo) -> Result<Port, String> {
        let renderer = get_lut3d_renderer(&lut.data()).map_err(|e| e.message().to_string())?;
        Ok(Port::in_place(move |px| renderer.apply(px)))
    }
    fn other_profiles(&self, lut: &Lut3D, _combo: &Combo) -> Vec<(String, Port)> {
        let data = lut.data();
        let cpu = CpuInfo::instance();
        let sse2 = X86_CPU_FLAG_SSE2;
        let profiles: Vec<(&str, CpuInfo)> = match lut.interpolation {
            Interpolation::Tetrahedral => vec![
                ("generic tetrahedral", cpu.with_flags(0)),
                ("SSE2 tetrahedral", cpu.with_flags(sse2)),
                ("AVX tetrahedral", cpu.with_flags(sse2 | X86_CPU_FLAG_AVX)),
                (
                    "AVX2 tetrahedral",
                    cpu.with_flags(sse2 | X86_CPU_FLAG_AVX | X86_CPU_FLAG_AVX2),
                ),
                (
                    "AVX-512 tetrahedral",
                    cpu.with_flags(
                        sse2 | X86_CPU_FLAG_AVX | X86_CPU_FLAG_AVX2 | X86_CPU_FLAG_AVX512,
                    ),
                ),
            ],
            _ => vec![("generic trilinear", cpu.with_build(BuildConfig::NO_SIMD))],
        };
        let dispatched = kernel_of(&get_forward_lut3d_renderer(&data, cpu));
        profiles
            .into_iter()
            .map(|(name, profile_cpu)| {
                (
                    name,
                    Arc::new(get_forward_lut3d_renderer(&data, &profile_cpu)),
                )
            })
            .filter(|(_, renderer)| kernel_of(renderer) != dispatched)
            .map(|(name, renderer)| {
                (
                    name.to_string(),
                    Port::in_place(move |px| CpuOp::apply(&*renderer, px)),
                )
            })
            .collect()
    }
    fn pass_through(&self, _lut: &Lut3D, _combo: &Combo) -> BatteryChannels {
        A
    }
    fn breakpoints(&self, lut: &Lut3D, _direction: Direction) -> Vec<f32> {
        probe::lut_domain_points(lut.grid_size as usize)
    }
    fn extra_probes(&self, _lut: &Lut3D, _direction: Direction) -> Vec<ProbeSet> {
        vec![ProbeSet::RowLengths { max_pixels: ROWS }]
    }
}

/// What tells two renderers apart: the tetrahedral kernel (`None` for the scalar path), or the
/// trilinear code path.
fn kernel_of(renderer: &ForwardLut3DRenderer) -> String {
    match renderer {
        ForwardLut3DRenderer::Tetrahedral(t) => format!("tetrahedral {:?}", t.kernel()),
        ForwardLut3DRenderer::Trilinear(t) => format!("trilinear, SSE2 {}", t.uses_sse2()),
    }
}

#[test]
fn renderers_match_the_wheel() {
    battery::run(&Lut3DFamily);
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

/// Spike S4's probe pixels: half values, special combinations and random values.
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

    let mut rng = Rng::new(seed);
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

/// The wheel's output for `pixels` (RGBA F32) through `lut`, all in one row.
fn wheel(lut: &Lut3D, pixels: &[f32]) -> Vec<f32> {
    let resp = Oracle::get().call(
        "cpu_apply",
        json!({"transform": lut.transform()}),
        &[&f32_to_bytes(pixels), &f32_to_bytes(&lut.values)],
    );
    assert!(resp.result.get("exception").is_none(), "{}", resp.result);
    resp.blob_f32(0)
}

/// The port's output for `pixels` through `lut`'s renderer, in one call.
fn port(lut: &Lut3D, pixels: &[f32]) -> Vec<f32> {
    let renderer = get_lut3d_renderer(&lut.data()).expect("a renderer");
    let mut out = pixels.to_vec();
    renderer.apply(&mut out);
    out
}

/// The grids too large to send with each of the battery's buffers, each LUT kind, both
/// interpolations, against the wheel on spike S4's probes in one call (the dispatched kernel,
/// or the build's trilinear path). In the full and exhaustive tiers only, but for 32^3.
#[test]
fn large_grids_match_the_wheel() {
    let sizes: &[u32] = if Tier::current() == Tier::Quick {
        &[32]
    } else {
        &[32, 64, 65, 129]
    };
    for &grid_size in sizes {
        let pixels = probe_pixels(0x4c55_5433 + u64::from(grid_size));
        for (kind, values) in luts(grid_size) {
            for interp in [Interpolation::Tetrahedral, Interpolation::Linear] {
                let lut = Lut3D::new(grid_size, interp, values.clone());
                assert_pixels_bits_eq(
                    &format!("{kind} {grid_size}^3 LUT, {interp:?}"),
                    &pixels,
                    4,
                    &wheel(&lut, &pixels),
                    &port(&lut, &pixels),
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
    let lut = Lut3D::new(grid_size, Interpolation::Tetrahedral, values);

    let one = [0.5f32, 0.25, 0.5, 1.0];
    let two = [one, one].concat();
    assert_pixels_bits_eq(
        "one pixel per call (scalar path)",
        &one,
        4,
        &wheel(&lut, &one),
        &port(&lut, &one),
    );
    assert_pixels_bits_eq(
        "two pixels in one call (SIMD kernel)",
        &two,
        4,
        &wheel(&lut, &two),
        &port(&lut, &two),
    );
}
