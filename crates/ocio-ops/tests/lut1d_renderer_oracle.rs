// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The 1D LUT renderers for float input against the wheel, bit for bit
//! (src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp, Lut1DOpCPU_SSE2.cpp, _AVX, _AVX2, _AVX512 @
//! v2.5.2), through the oracle test battery (every case, with fast math on and off, on the
//! tier's probes, each buffer one renderer call):
//! - the standard domain's (`Lut1DRenderer<BIT_DEPTH_F32, outBD>`, [`StandardDomain`]), which
//!   renders a row of more than one pixel with the SIMD kernel the CPU dispatches to
//!   (`m_applyLutFunc`), and a row of one pixel with the scalar loop; rows of 1 to 33 pixels
//!   leave every remainder of the 4-, 8- and 16-pixel kernels. The kernels move alpha through
//!   their RGBA packs unchanged from F32 to F32, so it is a pass-through channel there, and
//!   the kernels this machine doesn't dispatch to are compared on it;
//! - the half domain's (`Lut1DRendererHalfCode`, [`HalfDomain`]) and the hue-adjust renderers
//!   of both domains (`Lut1DRendererHueAdjust`, `Lut1DRendererHalfCodeHueAdjust`,
//!   [`HueAdjust`]), which have no SIMD kernel;
//! - the inverse renderers of both domains, with and without hue adjust (`InvLut1DRenderer`,
//!   ..., [`Inverse`]), without `OPTIMIZATION_LUT_INV_FAST`, which would replace the inverse
//!   LUT with a forward one.
//!
//! And to every output bit depth: on rows of one pixel ([`single_pixel_rows_match_the_wheel`]:
//! the scalar code), and on whole rows ([`whole_rows_match_the_wheel`]: the dispatched kernel).
//! These tests are CPU-dependent (`cpu-tests`): under SDE the wheel and the port both dispatch
//! to the emulated CPU's kernel.
//!
//! The oracle builds a `Lut1DTransform` from the LUT's values, passed as a blob (`setData`).
//! For the other output bit depths than F32, the LUT is the processor's last op, which the CPU
//! engine renders from F32 to the output bit depth (`CreateCPUEngine`,
//! src/OpenColorIO/CPUProcessor.cpp:153-162): a `GroupTransform` of a `MatrixTransform` that
//! halves every channel, then the LUT, without optimization, so that both ops stay. The port
//! renders the same: the matrix op's renderer, then the LUT's.
//!
//! Without a kernel, a forward LUT's float renderers write alpha (`in[3] * m_alphaScaling`,
//! which quiets a signalling NaN), so no channel passes through; with one, a row of one pixel
//! still takes the scalar loop, so alpha passes through from rows of 2 pixels
//! (`Family::pass_through_min_pixels`).

mod common;

use common::image::{depth_name, port_depth};
use ocio_ops::Result;
use ocio_ops::cpu_info::CpuInfo;
use ocio_ops::imath_half::half_to_float;
use ocio_ops::op::{CpuOp, OpVec, Pixels, PixelsMut};
use ocio_ops::open_color_types::{BitDepth, Lut1DHueAdjust, TransformDirection};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op_cpu::{
    Lut1DKernel, get_lut1d_profile_renderer, get_lut1d_renderer, get_lut1d_scalar_renderer,
    lut1d_kernel,
};
use ocio_ops::ops::lut1d::lut1d_op_data::Lut3by1DArray;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op_from_m44;
use ocio_testkit::Oracle;
use ocio_testkit::battery::params::{A, Channels as BatteryChannels};
use ocio_testkit::battery::params::{Case, LutEntries, Params, Slot, mutations, sampled_mutations};
use ocio_testkit::battery::{self, BitDepth as Depth, Combo, Direction, Family, Port, Spec, Tier};
use ocio_testkit::image::{Buffer, Channels, Data, Packed, Request, Stride};
use ocio_testkit::oracle::{BatchCall, f32_to_bytes};
use ocio_testkit::probe::{self, ProbeSet, RandomRange};
use serde_json::{Value, json};

/// The entries of a half domain.
const HALF_ENTRIES: usize = 65536;

/// A curve per channel, in `f64`, of an entry's input value.
type Curve = fn(f64) -> [f64; 3];

/// Smooth curves, rising and falling, with values past `[0, 1]`.
fn mixed(x: f64) -> [f64; 3] {
    [x * x, 1.0 - 2.0 * x, x * 4.0 - 1.5]
}

/// Values near `FLT_MAX`, which overflow to infinities, and tiny ones.
fn extreme(x: f64) -> [f64; 3] {
    [x * 3.0e38, -x * 3.0e38, x * 1.0e-30]
}

/// Steps that go back and forth, so that neighbouring entries differ in sign and size.
fn jagged(x: f64) -> [f64; 3] {
    let k = (x * 7.0).floor();
    [
        if k as i64 % 2 == 0 { x } else { -x },
        x.sqrt() * 1.0e3,
        k * 0.125 - x,
    ]
}

/// A `Lut1DTransform`'s values, R, G and B per entry, in a standard or a half domain, with
/// chosen entries as slots.
#[derive(Debug, Clone)]
struct Lut {
    half_domain: bool,
    /// `HUE_DW3`, or `HUE_NONE`.
    hue_adjust: bool,
    values: Vec<f32>,
    entries: LutEntries,
}

impl Lut {
    /// A LUT of `length` entries (a half domain's are the 65,536 halfs) whose values are
    /// `curve` at each entry's input: `i / (length - 1)`, or the half of code `i`, NaN and
    /// infinite ones included.
    fn new(half_domain: bool, length: usize, curve: Curve) -> Lut {
        let values = (0..length)
            .flat_map(|i| {
                let x = if half_domain {
                    f64::from(half_to_float(i as u16))
                } else {
                    i as f64 / (length - 1) as f64
                };
                curve(x).map(|v| v as f32)
            })
            .collect();
        let entries = if half_domain {
            // +0, the smallest denormal, 1, the largest half, +Inf, a NaN, -0, -1, -HALF_MAX,
            // -Inf.
            LutEntries::new(
                "lut",
                vec![
                    0x0000, 0x0001, 0x3c00, 0x7bff, 0x7c00, 0x7e00, 0x8000, 0xbc00, 0xfbff, 0xfc00,
                ],
            )
        } else {
            LutEntries::first_second_middle_last("lut", length)
        };
        Lut {
            half_domain,
            hue_adjust: false,
            values,
            entries,
        }
    }

    /// The LUT with every entry infinite, -Inf and +Inf in turn: between two nodes, the
    /// renderers interpolate from `-FLT_MAX` to `FLT_MAX` (the infinities sanitized), which gives
    /// infinities, so that with hue adjust two infinities of one sign meet (`new_chroma` is
    /// NaN).
    fn alternating_infinities(half_domain: bool, length: usize) -> Lut {
        let mut lut = Lut::new(half_domain, length, mixed);
        for (i, v) in lut.values.iter_mut().enumerate() {
            *v = if (i / 3) % 2 == 0 {
                f32::NEG_INFINITY
            } else {
                f32::INFINITY
            };
        }
        lut
    }

    /// The LUT with hue adjust (`HUE_DW3`).
    fn with_hue_adjust(mut self) -> Lut {
        self.hue_adjust = true;
        self
    }

    fn length(&self) -> usize {
        self.values.len() / 3
    }

    /// The transform, its values as blob 0.
    fn transform(&self) -> Value {
        self.transform_in(Direction::Forward)
    }

    /// The transform in `direction`, its values as blob 0.
    fn transform_in(&self, direction: Direction) -> Value {
        let hue = if self.hue_adjust {
            "HUE_DW3"
        } else {
            "HUE_NONE"
        };
        json!({"class": "Lut1DTransform", "calls": [
            ["setData", {"blob": 0, "dtype": "float32"}],
            ["setInputHalfDomain", self.half_domain],
            ["setHueAdjust", {"enum": hue}],
            ["setDirection", direction.oracle_enum()],
        ]})
    }

    /// The port's data, as `Lut1DTransform::setData`, `setInputHalfDomain` and `setHueAdjust`
    /// make it, and
    /// `BuildLut1DOp` validates it and the processor finalizes it
    /// (src/OpenColorIO/ops/lut1d/Lut1DOp.cpp:244-253, src/OpenColorIO/Processor.cpp:623-641
    /// @ v2.5.2).
    fn port(&self) -> Result<Lut1DOpData> {
        self.port_in(Direction::Forward)
    }

    /// [`Lut::port`] in `direction`: `BuildLut1DOp` validates the forward data, then
    /// `CreateLut1DOp` inverts it (src/OpenColorIO/ops/lut1d/Lut1DOp.cpp:221-253 @ v2.5.2)
    /// before the processor finalizes it.
    fn port_in(&self, direction: Direction) -> Result<Lut1DOpData> {
        let mut data = Lut1DOpData::new(2)?;
        data.set_input_half_domain(self.half_domain);
        if self.hue_adjust {
            data.set_hue_adjust(Lut1DHueAdjust::Dw3)?;
        }
        let mut array = Lut3by1DArray::new(data.get_half_flags(), 3, self.length() as _, false)?;
        array.get_values_mut().copy_from_slice(&self.values);
        *data.get_array_mut() = array;
        data.validate()?;
        if direction == Direction::Inverse {
            data = data.inverse();
        }
        data.finalize()?;
        Ok(std::hint::black_box(data))
    }

    /// The inputs of the chosen entries that are finite floats.
    fn entry_inputs(&self) -> Vec<f32> {
        self.entries
            .indices()
            .iter()
            .map(|&i| {
                if self.half_domain {
                    half_to_float(i as u16)
                } else {
                    (i as f64 / (self.length() - 1) as f64) as f32
                }
            })
            .filter(|x| x.is_finite())
            .collect()
    }
}

impl Params for Lut {
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

/// The longest row the battery probes for the kernels: every remainder of 4, 8 and 16 pixels.
const ROWS: usize = 33;

/// The standard domain's renderer, F32 to F32, through the battery: the kernel this machine
/// dispatches to, and the others on alpha.
struct StandardDomain;

impl Family for StandardDomain {
    type Params = Lut;

    fn name(&self) -> String {
        "Lut1DTransform (standard domain)".to_string()
    }
    fn cases(&self) -> Vec<Case<Lut>> {
        let mut cases = Vec::new();
        for n in [2, 3, 17, 256, 4096] {
            for (name, curve) in [
                ("mixed", mixed as Curve),
                ("extreme", extreme),
                ("jagged", jagged),
            ] {
                cases.push(Case::new(
                    format!("{n} entries, {name}"),
                    Lut::new(false, n, curve),
                ));
            }
            cases.push(Case::new(
                format!("{n} entries, alternating infinities"),
                Lut::alternating_infinities(false, n),
            ));
        }
        cases
    }
    fn mutation_bases(&self) -> Vec<Case<Lut>> {
        // The 17 entries' and the 4096 entries' mixed curves.
        let cases = self.cases();
        vec![cases[8].clone(), cases[16].clone()]
    }
    fn directions(&self) -> Vec<Direction> {
        // The default optimization replaces an inverse LUT with a forward one, WP 2.1g's;
        // `Inverse` renders inverse LUTs.
        vec![Direction::Forward]
    }
    fn spec(&self, lut: &Lut, _direction: Direction) -> Spec {
        Spec::with_f32_blobs(lut.transform(), &[&lut.values])
    }
    fn port(&self, lut: &Lut, _combo: &Combo) -> std::result::Result<Port, String> {
        let renderer = lut
            .port()
            .and_then(|data| get_lut1d_renderer(&data, BitDepth::F32, BitDepth::F32))
            .map_err(|e| e.message().to_string())?;
        Ok(Port::in_place(move |px| renderer.apply(px)))
    }
    fn other_profiles(&self, lut: &Lut, _combo: &Combo) -> Vec<(String, Port)> {
        let data = lut.port().expect("the case's data");
        let dispatched = lut1d_kernel(CpuInfo::instance(), BitDepth::F32);
        [
            Lut1DKernel::Sse2,
            Lut1DKernel::Avx,
            Lut1DKernel::Avx2,
            Lut1DKernel::Avx512,
        ]
        .into_iter()
        .filter(|&kernel| Some(kernel) != dispatched)
        .map(|kernel| {
            let renderer = get_lut1d_profile_renderer(&data, BitDepth::F32, Some(kernel))
                .expect("a profile renderer");
            (
                format!("{kernel:?}"),
                Port::in_place(move |px| renderer.apply(px)),
            )
        })
        .collect()
    }
    fn pass_through(&self, _lut: &Lut, _combo: &Combo) -> BatteryChannels {
        A
    }
    /// A row of one pixel takes the scalar loop, which writes alpha times 1.
    fn pass_through_min_pixels(&self) -> usize {
        2
    }
    fn breakpoints(&self, lut: &Lut, _direction: Direction) -> Vec<f32> {
        probe::lut_domain_points(lut.length())
    }
    fn extra_probes(&self, _lut: &Lut, _direction: Direction) -> Vec<ProbeSet> {
        vec![ProbeSet::RowLengths { max_pixels: ROWS }]
    }
}

#[test]
fn standard_domain_matches_the_wheel() {
    battery::run(&StandardDomain);
}

/// The half domain's renderer, F32 to F32, through the battery.
struct HalfDomain;

impl Family for HalfDomain {
    type Params = Lut;

    fn name(&self) -> String {
        "Lut1DTransform (half domain)".to_string()
    }
    fn cases(&self) -> Vec<Case<Lut>> {
        vec![
            Case::new("mixed", Lut::new(true, HALF_ENTRIES, mixed)),
            Case::new("extreme", Lut::new(true, HALF_ENTRIES, extreme)),
            Case::new("jagged", Lut::new(true, HALF_ENTRIES, jagged)),
            Case::new(
                "alternating infinities",
                Lut::alternating_infinities(true, HALF_ENTRIES),
            ),
        ]
    }
    fn mutation_bases(&self) -> Vec<Case<Lut>> {
        self.cases()[..1].to_vec()
    }
    fn directions(&self) -> Vec<Direction> {
        // The default optimization replaces an inverse LUT with a forward one, WP 2.1g's;
        // `Inverse` renders inverse LUTs.
        vec![Direction::Forward]
    }
    fn spec(&self, lut: &Lut, _direction: Direction) -> Spec {
        Spec::with_f32_blobs(lut.transform(), &[&lut.values])
    }
    fn port(&self, lut: &Lut, _combo: &Combo) -> std::result::Result<Port, String> {
        let renderer = lut
            .port()
            .and_then(|data| get_lut1d_renderer(&data, BitDepth::F32, BitDepth::F32))
            .map_err(|e| e.message().to_string())?;
        Ok(Port::in_place(move |px| renderer.apply(px)))
    }
    fn breakpoints(&self, lut: &Lut, _direction: Direction) -> Vec<f32> {
        lut.entry_inputs()
    }
}

#[test]
fn half_domain_matches_the_wheel() {
    battery::run(&HalfDomain);
}

/// The hue-adjust renderers, F32 to F32, through the battery: standard domains of a few lengths
/// and the half domain.
struct HueAdjust;

impl Family for HueAdjust {
    type Params = Lut;

    fn name(&self) -> String {
        "Lut1DTransform (hue adjust)".to_string()
    }
    fn cases(&self) -> Vec<Case<Lut>> {
        let mut cases = Vec::new();
        for (half, n) in [(false, 17), (true, HALF_ENTRIES), (false, 2), (false, 4096)] {
            for (name, curve) in [
                ("mixed", mixed as Curve),
                ("extreme", extreme),
                ("jagged", jagged),
            ] {
                let domain = if half {
                    "half domain".to_string()
                } else {
                    format!("{n} entries")
                };
                cases.push(Case::new(
                    format!("{domain}, {name}"),
                    Lut::new(half, n, curve).with_hue_adjust(),
                ));
            }
        }
        for (half, n) in [(false, 2), (false, 17), (true, HALF_ENTRIES)] {
            let domain = if half {
                "half domain".to_string()
            } else {
                format!("{n} entries")
            };
            cases.push(Case::new(
                format!("{domain}, alternating infinities"),
                Lut::alternating_infinities(half, n).with_hue_adjust(),
            ));
        }
        cases
    }
    fn mutation_bases(&self) -> Vec<Case<Lut>> {
        // The 17 entries' and the half domain's mixed curves.
        vec![self.cases()[0].clone(), self.cases()[3].clone()]
    }
    fn directions(&self) -> Vec<Direction> {
        // The default optimization replaces an inverse LUT with a forward one, WP 2.1g's;
        // `Inverse` renders inverse LUTs.
        vec![Direction::Forward]
    }
    fn spec(&self, lut: &Lut, _direction: Direction) -> Spec {
        Spec::with_f32_blobs(lut.transform(), &[&lut.values])
    }
    fn port(&self, lut: &Lut, _combo: &Combo) -> std::result::Result<Port, String> {
        let renderer = lut
            .port()
            .and_then(|data| get_lut1d_renderer(&data, BitDepth::F32, BitDepth::F32))
            .map_err(|e| e.message().to_string())?;
        Ok(Port::in_place(move |px| renderer.apply(px)))
    }
    fn breakpoints(&self, lut: &Lut, _direction: Direction) -> Vec<f32> {
        if lut.half_domain {
            lut.entry_inputs()
        } else {
            probe::lut_domain_points(lut.length())
        }
    }
}

#[test]
fn hue_adjust_matches_the_wheel() {
    battery::run(&HueAdjust);
}

/// Rising in red and green, falling in blue: a half domain's blue then takes the negative half
/// with red's sign (docs/improvements.md, I-67).
fn crossed(x: f64) -> [f64; 3] {
    [x * 2.0 - 0.25, x * x * x, 0.5 - x]
}

/// Flat at both ends and reversing in between: what an inverse LUT's set-up flattens and
/// leaves out of its effective domain.
fn flat_ends(x: f64) -> [f64; 3] {
    let w = (x * 9.0).sin();
    [w.clamp(-0.5, 0.5), -w, (x - 0.5).abs()]
}

/// The inverse renderers (`InvLut1DRenderer`, `InvLut1DRendererHalfCode`,
/// `InvLut1DRendererHueAdjust`, `InvLut1DRendererHalfCodeHueAdjust`), F32 to F32, through the
/// battery: standard domains of several lengths and the half domain, with and without hue
/// adjust. The default optimization's `OPTIMIZATION_LUT_INV_FAST` replaces an inverse LUT with
/// a forward one (`ReplaceInverseLuts`); without it, the processor renders the inverse LUT
/// itself, and only that flag is off here ([`Family::optimization_off`]). They write alpha
/// (`in[3] * m_alphaScaling`), so no channel passes through.
struct Inverse;

impl Inverse {
    /// The curves of the inverse cases.
    const CURVES: [(&str, Curve); 5] = [
        ("mixed", mixed),
        ("extreme", extreme),
        ("jagged", jagged),
        ("crossed", crossed),
        ("flat ends", flat_ends),
    ];
}

impl Family for Inverse {
    type Params = Lut;

    fn name(&self) -> String {
        "Lut1DTransform (inverse, without OPTIMIZATION_LUT_INV_FAST)".to_string()
    }
    fn cases(&self) -> Vec<Case<Lut>> {
        let mut cases = Vec::new();
        for (half, n) in [
            (false, 17),
            (true, HALF_ENTRIES),
            (false, 2),
            (false, 3),
            (false, 256),
            (false, 4096),
        ] {
            let domain = if half {
                "half domain".to_string()
            } else {
                format!("{n} entries")
            };
            for (name, curve) in Self::CURVES {
                cases.push(Case::new(
                    format!("{domain}, {name}"),
                    Lut::new(half, n, curve),
                ));
            }
            cases.push(Case::new(
                format!("{domain}, alternating infinities"),
                Lut::alternating_infinities(half, n),
            ));
        }
        for (half, n) in [(false, 17), (true, HALF_ENTRIES), (false, 2)] {
            let domain = if half {
                "half domain".to_string()
            } else {
                format!("{n} entries")
            };
            for (name, curve) in Self::CURVES {
                cases.push(Case::new(
                    format!("{domain}, {name}, hue adjust"),
                    Lut::new(half, n, curve).with_hue_adjust(),
                ));
            }
        }
        cases
    }
    fn mutation_bases(&self) -> Vec<Case<Lut>> {
        // The mixed curves of 17 entries and of the half domain, without and with hue adjust.
        let cases = self.cases();
        let base = |label: &str| {
            cases
                .iter()
                .find(|case| case.label() == label)
                .unwrap_or_else(|| panic!("no case {label}"))
                .clone()
        };
        vec![
            base("17 entries, mixed"),
            base("half domain, mixed"),
            base("17 entries, mixed, hue adjust"),
            base("half domain, mixed, hue adjust"),
        ]
    }
    fn directions(&self) -> Vec<Direction> {
        vec![Direction::Inverse]
    }
    fn optimization_off(&self) -> Vec<&'static str> {
        vec!["OPTIMIZATION_LUT_INV_FAST"]
    }
    fn spec(&self, lut: &Lut, direction: Direction) -> Spec {
        Spec::with_f32_blobs(lut.transform_in(direction), &[&lut.values])
    }
    fn port(&self, lut: &Lut, combo: &Combo) -> std::result::Result<Port, String> {
        let renderer = lut
            .port_in(combo.direction)
            .and_then(|data| get_lut1d_renderer(&data, BitDepth::F32, BitDepth::F32))
            .map_err(|e| e.message().to_string())?;
        Ok(Port::in_place(move |px| renderer.apply(px)))
    }
    /// The inverse's breakpoints are the LUT's values: every finite one of a standard domain,
    /// the chosen entries' of a half domain.
    fn breakpoints(&self, lut: &Lut, _direction: Direction) -> Vec<f32> {
        let mut points: Vec<f32> = if lut.half_domain {
            lut.entries
                .indices()
                .iter()
                .flat_map(|&i| lut.values[i * 3..i * 3 + 3].to_vec())
                .collect()
        } else {
            lut.values.clone()
        };
        points.retain(|v| v.is_finite());
        points.sort_by(f32::total_cmp);
        points.dedup_by(|a, b| a.to_bits() == b.to_bits());
        points
    }
}

#[test]
fn inverse_matches_the_wheel() {
    battery::run(&Inverse);
}

/// The matrix the processor applies before the LUT for the other output bit depths: halves
/// every channel.
#[rustfmt::skip]
const HALVE: [f64; 16] = [
    0.5, 0.0, 0.0, 0.0,
    0.0, 0.5, 0.0, 0.0,
    0.0, 0.0, 0.5, 0.0,
    0.0, 0.0, 0.0, 0.5,
];

/// The probe values for a LUT: the specials, the neighbourhoods of a standard domain's nodes
/// and midpoints (or of the chosen entries' halfs), and random values in `[0, 1)` and
/// `[-1, 2)`; with the matrix, each value twice, as is and doubled, so that the LUT sees it.
fn probe_pixels(lut: &Lut, with_matrix: bool) -> Vec<f32> {
    let points = if lut.half_domain {
        lut.entry_inputs()
    } else {
        probe::lut_domain_points(lut.length())
    };
    let mut values = probe::specials();
    values.extend(probe::neighbourhoods(&points, 2));
    values.extend(probe::random(
        0x0b47_7e27_21a0,
        &[(RandomRange::Unit, 384), (RandomRange::Overshoot, 128)],
    ));
    if with_matrix {
        let doubled: Vec<f32> = values.iter().map(|v| v * 2.0).collect();
        values.extend(doubled);
    }
    probe::to_rgba_cycled(&values)
}

/// One comparison of rows: a LUT, with the matrix before it or not, to an output bit depth, on
/// rows of one pixel or on one row.
struct RowCase {
    label: String,
    lut: Lut,
    output: Depth,
    single_pixel_rows: bool,
}

impl RowCase {
    fn with_matrix(&self) -> bool {
        self.output != Depth::F32
    }

    /// The `image_apply` request: the probes as a column of single-pixel rows, or as one row,
    /// F32 RGBA, into an image of the output bit depth of the same shape.
    fn request(&self, pixels: &[f32]) -> Request {
        let transform = if self.with_matrix() {
            json!({"class": "GroupTransform", "children": [
                {"class": "MatrixTransform", "args": {"matrix": HALVE}},
                self.lut.transform(),
            ]})
        } else {
            self.lut.transform()
        };
        let mut request = Request::new(json!({
            "transform": transform,
            "optimization": "OPTIMIZATION_NONE",
            "in_bitdepth": "BIT_DEPTH_F32",
            "out_bitdepth": depth_name(port_depth(self.output)),
        }));
        let n = (pixels.len() / 4) as i64;
        let (width, height) = if self.single_pixel_rows {
            (1, n)
        } else {
            (n, 1)
        };
        let src = request.buffer(Buffer::Bytes(f32_to_bytes(pixels)));
        let dst = request.buffer(Buffer::Bytes(vec![
            0;
            pixels.len()
                * ocio_testkit::image::channel_bytes(
                    self.output
                )
        ]));
        request.image(
            Packed::new(Data::at(src, 0), width, height, Channels::Count(4))
                .layout(Depth::F32, [Stride::Auto; 3]),
        );
        request.image(
            Packed::new(Data::at(dst, 0), width, height, Channels::Count(4))
                .layout(self.output, [Stride::Auto; 3]),
        );
        request.apply = vec![0, 1];
        request
    }

    /// The port's output bytes: the matrix op's renderer if any, then the LUT's renderer from
    /// F32 to the output bit depth, on the whole row; for single-pixel rows, the scalar profile
    /// of a standard domain without hue adjust (the others have no kernel).
    fn port(&self, pixels: &[f32]) -> Result<Vec<u8>> {
        let data = self.lut.port()?;
        let out = port_depth(self.output);
        let scalar = self.single_pixel_rows
            && !data.is_input_half_domain()
            && data.get_hue_adjust() == Lut1DHueAdjust::None;
        let renderer = if scalar {
            get_lut1d_scalar_renderer(&data, out)?
        } else {
            get_lut1d_renderer(&data, BitDepth::F32, out)?
        };
        let mut input = pixels.to_vec();
        if self.with_matrix() {
            let mut ops = OpVec::new();
            create_matrix_op_from_m44(&mut ops, &HALVE, TransformDirection::Forward);
            ops.finalize()?;
            let matrix = ops[0].get_cpu_op(false)?.expect("a matrix renderer");
            matrix.apply(&mut input);
        }
        Ok(render(&*renderer, &input, out))
    }
}

/// `renderer`'s output for F32 `input`, as `out`'s bytes in the machine's order.
fn render(renderer: &dyn CpuOp, input: &[f32], out: BitDepth) -> Vec<u8> {
    let n = input.len();
    match out {
        BitDepth::Uint8 => {
            let mut o = vec![0u8; n];
            renderer.apply_bit_depth(Pixels::F32(input), PixelsMut::U8(&mut o));
            o
        }
        BitDepth::Uint10 | BitDepth::Uint12 | BitDepth::Uint16 => {
            let mut o = vec![0u16; n];
            renderer.apply_bit_depth(Pixels::F32(input), PixelsMut::U16(&mut o));
            o.iter().flat_map(|v| v.to_ne_bytes()).collect()
        }
        BitDepth::F16 => {
            let mut o = vec![half::f16::ZERO; n];
            renderer.apply_bit_depth(Pixels::F32(input), PixelsMut::F16(&mut o));
            o.iter().flat_map(|v| v.to_bits().to_ne_bytes()).collect()
        }
        BitDepth::F32 => {
            let mut o = vec![0.0f32; n];
            renderer.apply_bit_depth(Pixels::F32(input), PixelsMut::F32(&mut o));
            f32_to_bytes(&o)
        }
        BitDepth::Uint14 | BitDepth::Uint32 | BitDepth::Unknown => unreachable!("{out:?}"),
    }
}

/// The LUTs of the row tests: standard domains of several lengths and the half domain, with
/// the explicit curves, and two with hue adjust, to every output bit depth; and the generated
/// cases of the mixed curves (extreme finite, NaN and infinite values in chosen entries: all of
/// them beyond the quick tier, a sample in it), to F32 and 16-bit output.
fn row_cases(single_pixel_rows: bool) -> Vec<RowCase> {
    let tier = Tier::current();
    let mut cases = Vec::new();
    let luts: Vec<(String, Lut)> = [2usize, 3, 17, 256, 4096]
        .into_iter()
        .map(|n| (false, n))
        .chain([(true, HALF_ENTRIES)])
        .flat_map(|(half, n)| {
            [
                ("mixed", mixed as Curve),
                ("extreme", extreme),
                ("jagged", jagged),
            ]
            .map(move |(name, curve)| {
                let domain = if half {
                    "half domain".into()
                } else {
                    format!("{n} entries")
                };
                (format!("{domain}, {name}"), Lut::new(half, n, curve))
            })
        })
        .chain([(false, 17), (true, HALF_ENTRIES)].map(|(half, n)| {
            let domain = if half {
                "half domain".to_string()
            } else {
                format!("{n} entries")
            };
            (
                format!("{domain}, mixed, hue adjust"),
                Lut::new(half, n, mixed).with_hue_adjust(),
            )
        }))
        .collect();
    for (label, lut) in &luts {
        for output in common::image::DEPTHS {
            cases.push(RowCase {
                label: format!("{label}, to {output:?}"),
                lut: lut.clone(),
                output,
                single_pixel_rows,
            });
        }
    }
    for (label, lut) in luts.iter().filter(|(label, _)| label.ends_with("mixed")) {
        let base = Case::new(label.clone(), lut.clone());
        let generated = if tier == Tier::Quick {
            sampled_mutations(&base)
        } else {
            mutations(&base)
        };
        for case in generated {
            for output in [Depth::F32, Depth::Uint16] {
                cases.push(RowCase {
                    label: format!("{}, to {output:?}", case.label()),
                    lut: case.params().clone(),
                    output,
                    single_pixel_rows,
                });
            }
        }
    }
    cases
}

/// Every row case, applied by the wheel to a column of single-pixel rows (one renderer call
/// per pixel, the scalar code), equals the port's renderer byte for byte.
#[test]
fn single_pixel_rows_match_the_wheel() {
    check_rows(true);
}

/// Every row case, applied by the wheel to one row (one renderer call, the dispatched kernel
/// for a standard domain without hue adjust), equals the port's renderer byte for byte.
#[test]
fn whole_rows_match_the_wheel() {
    check_rows(false);
}

/// The row cases, on single-pixel rows or one row.
fn check_rows(single_pixel_rows: bool) {
    let cases = row_cases(single_pixel_rows);
    let inputs: Vec<Vec<f32>> = cases
        .iter()
        .map(|case| probe_pixels(&case.lut, case.with_matrix()))
        .collect();
    let requests: Vec<Request> = cases
        .iter()
        .zip(&inputs)
        .map(|(case, pixels)| case.request(pixels))
        .collect();
    let blobs: Vec<Vec<u8>> = cases.iter().map(|c| f32_to_bytes(&c.lut.values)).collect();
    let calls: Vec<BatchCall<'_>> = requests
        .iter()
        .zip(&blobs)
        .map(|(request, lut)| {
            let mut call = request.call();
            call.blobs.push(lut.as_slice());
            call
        })
        .collect();
    let responses = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for ((case, (request, pixels)), response) in cases
        .iter()
        .zip(requests.iter().zip(&inputs))
        .zip(responses)
    {
        let reply = request.reply(response.unwrap_or_else(|e| panic!("{}: {e}", case.label)));
        if let Some(raised) = reply.raised() {
            failures.push(format!("{}: the wheel raised {raised:?}", case.label));
            continue;
        }
        match case.port(pixels) {
            Err(e) => failures.push(format!("{}: the port raised {}", case.label, e.message())),
            Ok(port) if port != reply.buffers[1] => {
                let size = ocio_testkit::image::channel_bytes(case.output);
                let first = port
                    .chunks(size)
                    .zip(reply.buffers[1].chunks(size))
                    .position(|(p, w)| p != w)
                    .expect("a difference");
                failures.push(format!(
                    "{}: value {first} (input {:e}) differs: port {:02x?}, wheel {:02x?}",
                    case.label,
                    pixels[first],
                    &port[first * size..(first + 1) * size],
                    &reply.buffers[1][first * size..(first + 1) * size],
                ));
            }
            Ok(_) => {}
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures[..failures.len().min(20)].join("\n")
    );
}
