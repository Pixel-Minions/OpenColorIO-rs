// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The forward 1D LUT renderers for float input against the wheel, bit for bit
//! (src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp @ v2.5.2):
//! - the half domain's (`Lut1DRendererHalfCode<BIT_DEPTH_F32, outBD>`), which has no SIMD
//!   kernel, through the oracle test battery: every case, with fast math on and off, on the
//!   tier's probes, each buffer one renderer call ([`HalfDomain`]);
//! - the scalar code of the standard domain's (`Lut1DRenderer<BIT_DEPTH_F32, outBD>`), and the
//!   half domain's, to every output bit depth, on rows of one pixel: upstream renders a row of
//!   more than one pixel of a standard domain with a SIMD kernel (`m_applyLutFunc`, on every
//!   x86-64 CPU), which waits for WP 2.1c and 2.1d, but a row of one pixel with the scalar
//!   loop ([`single_pixel_rows_match_the_wheel`]).
//!
//! The oracle builds a `Lut1DTransform` from the LUT's values, passed as a blob (`setData`).
//! For the other output bit depths than F32, the LUT is the processor's last op, which the CPU
//! engine renders from F32 to the output bit depth (`CreateCPUEngine`,
//! src/OpenColorIO/CPUProcessor.cpp:153-162): a `GroupTransform` of a `MatrixTransform` that
//! halves every channel, then the LUT, without optimization, so that both ops stay. The port
//! renders the same: the matrix op's renderer, then the LUT's.
//!
//! A forward LUT's float renderers write alpha (`in[3] * m_alphaScaling`, which quiets a
//! signalling NaN), so no channel passes through.

mod common;

use common::image::{depth_name, port_depth};
use ocio_ops::Result;
use ocio_ops::imath_half::half_to_float;
use ocio_ops::op::{CpuOp, OpVec, Pixels, PixelsMut};
use ocio_ops::open_color_types::{BitDepth, TransformDirection};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op_cpu::{get_lut1d_renderer, get_lut1d_scalar_renderer};
use ocio_ops::ops::lut1d::lut1d_op_data::Lut3by1DArray;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op_from_m44;
use ocio_testkit::Oracle;
use ocio_testkit::battery::params::{Case, LutEntries, Params, Slot, mutations, sampled_mutations};
use ocio_testkit::battery::{self, BitDepth as Depth, Combo, Direction, Family, Port, Spec, Tier};
use ocio_testkit::image::{Buffer, Channels, Data, Packed, Request, Stride};
use ocio_testkit::oracle::{BatchCall, f32_to_bytes};
use ocio_testkit::probe::{self, RandomRange};
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
            values,
            entries,
        }
    }

    fn length(&self) -> usize {
        self.values.len() / 3
    }

    /// The transform, its values as blob 0.
    fn transform(&self) -> Value {
        json!({"class": "Lut1DTransform", "calls": [
            ["setData", {"blob": 0, "dtype": "float32"}],
            ["setInputHalfDomain", self.half_domain],
        ]})
    }

    /// The port's data, as `Lut1DTransform::setData` and `setInputHalfDomain` make it, and
    /// `BuildLut1DOp` validates it and the processor finalizes it
    /// (src/OpenColorIO/ops/lut1d/Lut1DOp.cpp:244-253, src/OpenColorIO/Processor.cpp:623-641
    /// @ v2.5.2).
    fn port(&self) -> Result<Lut1DOpData> {
        let mut data = Lut1DOpData::new(2)?;
        data.set_input_half_domain(self.half_domain);
        let mut array = Lut3by1DArray::new(data.get_half_flags(), 3, self.length() as _, false)?;
        array.get_values_mut().copy_from_slice(&self.values);
        *data.get_array_mut() = array;
        data.validate()?;
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
        ]
    }
    fn mutation_bases(&self) -> Vec<Case<Lut>> {
        self.cases()[..1].to_vec()
    }
    fn directions(&self) -> Vec<Direction> {
        // The inverse renderers are WP 2.1f's.
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

/// One single-pixel-row comparison: a LUT, with the matrix before it or not, to an output
/// bit depth.
struct RowCase {
    label: String,
    lut: Lut,
    output: Depth,
}

impl RowCase {
    fn with_matrix(&self) -> bool {
        self.output != Depth::F32
    }

    /// The `image_apply` request: the probes as a column of single-pixel rows, F32 RGBA, into
    /// a column of the output bit depth.
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
        let src = request.buffer(Buffer::Bytes(f32_to_bytes(pixels)));
        let dst = request.buffer(Buffer::Bytes(vec![
            0;
            pixels.len()
                * ocio_testkit::image::channel_bytes(
                    self.output
                )
        ]));
        request.image(
            Packed::new(Data::at(src, 0), 1, n, Channels::Count(4))
                .layout(Depth::F32, [Stride::Auto; 3]),
        );
        request.image(
            Packed::new(Data::at(dst, 0), 1, n, Channels::Count(4))
                .layout(self.output, [Stride::Auto; 3]),
        );
        request.apply = vec![0, 1];
        request
    }

    /// The port's output bytes: the matrix op's renderer if any, then the LUT's renderer from
    /// F32 to the output bit depth, the scalar profile for a standard domain.
    fn port(&self, pixels: &[f32]) -> Result<Vec<u8>> {
        let data = self.lut.port()?;
        let out = port_depth(self.output);
        let renderer = if data.is_input_half_domain() {
            get_lut1d_renderer(&data, BitDepth::F32, out)?
        } else {
            get_lut1d_scalar_renderer(&data, out)?
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

/// The LUTs of the single-pixel-row test: standard domains of several lengths and the half
/// domain, with the explicit curves, to every output bit depth; and the generated cases of
/// the mixed curves (extreme finite, NaN and infinite values in chosen entries: all of them
/// beyond the quick tier, a sample in it), to F32 and 16-bit output.
fn row_cases() -> Vec<RowCase> {
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
        .collect();
    for (label, lut) in &luts {
        for output in common::image::DEPTHS {
            cases.push(RowCase {
                label: format!("{label}, to {output:?}"),
                lut: lut.clone(),
                output,
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
    let cases = row_cases();
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
