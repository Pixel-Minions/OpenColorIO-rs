// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The transforms' pixels through the port's API at every bit depth, layout and optimization
//! level, against the wheel's, byte for byte, through the oracle's `image_apply`.
//!
//! Each case of each class (`common::api_cases`) goes in both directions through
//! `Config::CreateRaw()->getProcessor(transform)` and a CPU processor for an input and an
//! output bit depth (all 36 pairs of UINT8, UINT10, UINT12, UINT16, F16 and F32) and an
//! optimization level (`getDefaultCPUProcessor` for F32 to F32, otherwise
//! `getOptimizedCPUProcessor` with `OPTIMIZATION_DEFAULT`; and each of `OPTIMIZATION_NONE`,
//! `LOSSLESS`, `VERY_GOOD`, `GOOD`, `DRAFT` and `DEFAULT`), applied from an image of one
//! layout to another (packed RGBA, RGB and BGRA, planar RGBA and RGB, with the strides the
//! library derives), or in place when the bit depths match. The integer and half inputs take
//! the optimizer's separable-prefix bake at the levels that have it, so the 1D LUT it bakes
//! and its lookups are on the path.
//!
//! Every buffer must equal the wheel's after the call, the source's included, and so must the
//! processor's and the CPU processor's cache IDs and getters; or both must raise the same
//! message at the same stage.
//!
//! The quick tier runs each combination of bit depths, layout, level and apply three times per
//! class, spread over its cases and directions; the full and exhaustive tiers run every
//! combination for every case in both directions.
//!
//! Waiver W0002 applies to the cases with NaN parameters where their scope says, through the
//! battery's own comparisons only: the images, decoded into RGBA `f32`, with
//! `Case::compare_pixels`, where the pixels' renderer ran with fast math as [`w0002_combo`]
//! says; and where the CPU processors' cache IDs differ, the 1D LUTs the optimizer baked from
//! the ops, read back from both optimized processors, with `Case::compare_baked_luts` (the
//! owner's extension of W0002, 2026-10-04). Everything else compares bit for bit, and what
//! W0002 covers is counted. A test in `battery::params` pins the callers of both.
//!
//! The `Lut1DTransform`'s float renderers, composing LUTs, the inverse LUT and the hue
//! adjustment are Phase 2's (WP 2.1, 2.5). [`lut1d_deferral`] says, from the renderer upstream
//! picks for each combination, which ones the port must refuse with which "not ported yet"
//! message, while the wheel renders them: the test counts those as deferrals, pins how many
//! there are per message, and compares every other combination, the lookups of integer and
//! half inputs, which every tier runs in full.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use common::api::{Calls, LEVELS, port_transform};
use common::api_cases::{self, Cases};
use ocio::{BitDepth, Config, Exception, OptimizationFlags, Transform, TransformDirection};
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::image_desc::{
    AUTO_STRIDE, Bytes, ImageDesc, ImageDescMut, PackedImageDesc, PixelData, PlanarImageDesc,
};
use ocio_ops::open_color_types::ChannelOrdering;
use ocio_ops::ops::lut1d::lut1d_op::{NOT_PORTED_COMPOSE, NOT_PORTED_F32};
use ocio_testkit::Oracle;
use ocio_testkit::battery::params::Comparison;
use ocio_testkit::battery::{self, BitDepth as Depth, Direction, Tier};
use ocio_testkit::image::{
    Buffer, ChannelOrder, Channels, Data, Packed, Planar, Request, Stride, channel_bytes,
};
use ocio_testkit::probe::{Rng, specials};
use ocio_testkit::processor_ops::{ProcessorOpsReply, ProcessorOpsRequest};
use serde_json::{Value, json};

/// The images' width and height: an odd width, so that the engine's loops have a remainder.
const WIDTH: usize = 37;
const HEIGHT: usize = 2;

/// The bit depths the CPU processor takes.
const DEPTHS: [Depth; 6] = [
    Depth::Uint8,
    Depth::Uint10,
    Depth::Uint12,
    Depth::Uint16,
    Depth::F16,
    Depth::F32,
];

/// How many cases and directions each combination runs for in the quick tier.
const QUICK_TURNS: usize = 3;

/// What a destination buffer holds before an apply.
const PREFILL: [u8; 5] = [0xa5, 0x5a, 0xc3, 0x3c, 0x96];

/// A version 1 config with one color space, for the classes that build other ops there.
const V1_CONFIG: &str = "ocio_profile_version: 1
roles:
  default: raw
displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
    name: raw
";

/// An image layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    PackedRgba,
    PackedRgb,
    PackedBgra,
    PlanarRgba,
    PlanarRgb,
}

const LAYOUTS: [Layout; 5] = [
    Layout::PackedRgba,
    Layout::PackedRgb,
    Layout::PackedBgra,
    Layout::PlanarRgba,
    Layout::PlanarRgb,
];

impl Layout {
    fn channels(self) -> usize {
        match self {
            Layout::PackedRgba | Layout::PackedBgra | Layout::PlanarRgba => 4,
            Layout::PackedRgb | Layout::PlanarRgb => 3,
        }
    }

    fn planar(self) -> bool {
        matches!(self, Layout::PlanarRgba | Layout::PlanarRgb)
    }

    /// The buffers of an image: one, or one per plane.
    fn buffers(self) -> usize {
        if self.planar() { self.channels() } else { 1 }
    }
}

/// The port's bit depth for the test kit's.
fn port_depth(depth: Depth) -> BitDepth {
    match depth {
        Depth::Uint8 => BitDepth::Uint8,
        Depth::Uint10 => BitDepth::Uint10,
        Depth::Uint12 => BitDepth::Uint12,
        Depth::Uint16 => BitDepth::Uint16,
        Depth::F16 => BitDepth::F16,
        Depth::F32 => BitDepth::F32,
    }
}

/// `count` channel values of `depth`, seeded, as bytes: every code of 8 bits, codes up to the
/// maximum with both ends for 10, 12 and 16 bits, half specials and any half bits, and float
/// specials and values from -0.5 to 2.
fn source_bytes(depth: Depth, count: usize, seed: u64) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let floats = specials();
    let half_specials: [u16; 10] = [
        0x0000, 0x8000, 0x0001, 0x03ff, 0x3c00, 0x7bff, 0x7c00, 0xfc00, 0x7e00, 0x7d00,
    ];
    let mut out = Vec::with_capacity(count * 4);
    for k in 0..count {
        let bits = rng.next_u64();
        match depth {
            Depth::Uint8 => out.push((k as u64 * 167 + seed) as u8),
            Depth::Uint10 | Depth::Uint12 | Depth::Uint16 => {
                let max: u64 = match depth {
                    Depth::Uint10 => 1023,
                    Depth::Uint12 => 4095,
                    _ => 65535,
                };
                let code = match k % 7 {
                    0 => 0,
                    1 => max,
                    _ => bits % (max + 1),
                };
                out.extend_from_slice(&(code as u16).to_ne_bytes());
            }
            Depth::F16 => {
                let half = if k % 3 == 0 {
                    half_specials[(k / 3) % half_specials.len()]
                } else {
                    bits as u16
                };
                out.extend_from_slice(&half.to_ne_bytes());
            }
            Depth::F32 => {
                let value = if k % 2 == 0 {
                    floats[(k / 2) % floats.len()]
                } else {
                    rng.uniform(-0.5, 2.0)
                };
                out.extend_from_slice(&value.to_ne_bytes());
            }
        }
    }
    out
}

/// Adds an image of `layout` and `depth` to `request`, over new buffers: the source's seeded
/// values, or a destination's prefill. Returns the image's index.
fn add_image(request: &mut Request, layout: Layout, depth: Depth, seed: Option<u64>) -> usize {
    let values = WIDTH
        * HEIGHT
        * if layout.planar() {
            1
        } else {
            layout.channels()
        };
    let buffer = |request: &mut Request, k: u64| {
        request.buffer(match seed {
            Some(seed) => Buffer::Bytes(source_bytes(depth, values, seed * 8 + k)),
            None => Buffer::fill(values * channel_bytes(depth), &PREFILL),
        })
    };
    let w = WIDTH as i64;
    let h = HEIGHT as i64;
    if layout.planar() {
        let planes: Vec<Data> = (0..layout.channels())
            .map(|k| Data::at(buffer(request, k as u64), 0))
            .collect();
        request.image(Planar::new(planes, w, h).layout(depth, [Stride::Auto; 2]))
    } else {
        let channels = match layout {
            Layout::PackedBgra => Channels::Order(ChannelOrder::Bgra),
            _ => Channels::Count(layout.channels() as i64),
        };
        let data = Data::at(buffer(request, 0), 0);
        request.image(Packed::new(data, w, h, channels).layout(depth, [Stride::Auto; 3]))
    }
}

/// A port description of either kind.
enum PortImage<B: AsRef<[u8]>> {
    Packed(PackedImageDesc<B>),
    Planar(PlanarImageDesc<B>),
}

impl<B: AsRef<[u8]>> PortImage<B> {
    fn desc(&self) -> &dyn ImageDesc {
        match self {
            PortImage::Packed(d) => d,
            PortImage::Planar(d) => d,
        }
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> PortImage<B> {
    fn desc_mut(&mut self) -> &mut dyn ImageDescMut {
        match self {
            PortImage::Packed(d) => d,
            PortImage::Planar(d) => d,
        }
    }
}

/// The port's description of an image of `layout` and `depth` over `buffers` (one, or one per
/// plane), as [`add_image`] describes it to the oracle.
fn port_image<B: AsRef<[u8]>>(
    layout: Layout,
    depth: Depth,
    buffers: Vec<B>,
) -> Result<PortImage<B>, Exception>
where
    Bytes<B>: PixelData<Bytes = B>,
{
    let depth = port_depth(depth);
    let mut buffers = buffers.into_iter().map(Bytes);
    let mut next = || buffers.next().expect("a buffer");
    if layout.planar() {
        let (r, g, b) = (next(), next(), next());
        let a = (layout.channels() == 4).then(&mut next);
        let desc = PlanarImageDesc::with_strides(
            r,
            g,
            b,
            a,
            WIDTH,
            HEIGHT,
            depth,
            AUTO_STRIDE,
            AUTO_STRIDE,
        )?;
        return Ok(PortImage::Planar(desc));
    }
    let desc = match layout {
        Layout::PackedBgra => PackedImageDesc::with_channel_order_and_strides(
            next(),
            WIDTH,
            HEIGHT,
            ChannelOrdering::Bgra,
            depth,
            AUTO_STRIDE,
            AUTO_STRIDE,
            AUTO_STRIDE,
        )?,
        _ => PackedImageDesc::with_strides(
            next(),
            WIDTH,
            HEIGHT,
            layout.channels(),
            depth,
            AUTO_STRIDE,
            AUTO_STRIDE,
            AUTO_STRIDE,
        )?,
    };
    Ok(PortImage::Packed(desc))
}

/// A class of the sweep: its cases, the config, and whether it is the `Lut1DTransform`.
struct Class {
    name: &'static str,
    cases: Cases,
    /// A version 1 config instead of the raw config.
    v1: bool,
    /// The `Lut1DTransform`, whose Phase 2 deferrals [`lut1d_deferral`] lists.
    lut1d: bool,
}

impl Class {
    fn new(name: &'static str, cases: Cases) -> Class {
        Class {
            name,
            cases,
            v1: false,
            lut1d: false,
        }
    }
}

/// One combination: bit depths, layout, level (`None`: no flags given) and whether in place.
#[derive(Debug, Clone, Copy)]
struct Combo {
    input: Depth,
    output: Depth,
    layout: Layout,
    level: Option<usize>,
    in_place: bool,
}

impl Combo {
    /// The flags of the CPU processor: the level's, `OPTIMIZATION_DEFAULT` without one.
    fn flags(&self) -> OptimizationFlags {
        self.level
            .map_or(OptimizationFlags::DEFAULT, |level| LEVELS[level].1)
    }
}

/// Every combination: from one image to another at every pair of bit depths, and in place at
/// every bit depth, with every layout and level.
fn combos() -> Vec<Combo> {
    let mut out = Vec::new();
    for input in DEPTHS {
        for output in DEPTHS {
            for in_place in [false, true] {
                if in_place && input != output {
                    continue;
                }
                for layout in LAYOUTS {
                    for level in std::iter::once(None).chain((0..LEVELS.len()).map(Some)) {
                        out.push(Combo {
                            input,
                            output,
                            layout,
                            level,
                            in_place,
                        });
                    }
                }
            }
        }
    }
    out
}

/// The battery's combination for W0002's scope at `combo` in `dir`: whether the pixels were
/// rendered with fast math. An integer or half input at a level with
/// `OPTIMIZATION_COMP_SEPARABLE_PREFIX` looks the pixels up in the 1D LUT the optimizer bakes
/// from the ops, which renders them without fast math (`EvalTransform`, ops/OpTools.cpp,
/// through `Op::apply`, src/OpenColorIO/Op.h:232-241 @ v2.5.2); otherwise the level's
/// `OPTIMIZATION_FAST_LOG_EXP_POW` decides.
fn w0002_combo(dir: Direction, combo: &Combo) -> battery::Combo {
    let flags = combo.flags();
    let baked =
        combo.input != Depth::F32 && flags.has_flag(OptimizationFlags::COMP_SEPARABLE_PREFIX);
    battery::Combo {
        direction: dir,
        fast_math: !baked && flags.has_flag(OptimizationFlags::FAST_LOG_EXP_POW),
        format: battery::Format::F32_RGBA,
    }
}

/// An exact `f32` for half bits: the same value, and for a NaN the same sign and payload.
fn half_to_f32(h: u16) -> f32 {
    let sign = u32::from(h >> 15) << 31;
    let exponent = u32::from((h >> 10) & 0x1f);
    let mantissa = u32::from(h & 0x3ff);
    let magnitude = match exponent {
        0x1f => 0x7f80_0000 | (mantissa << 13),
        // Subnormal: mantissa * 2^-24, exact in `f32`.
        0 => (mantissa as f32 * f32::from_bits(0x3380_0000)).to_bits(),
        _ => ((exponent + 112) << 23) | (mantissa << 13),
    };
    f32::from_bits(sign | magnitude)
}

/// An image of `layout` and `depth` over `buffers` (one, or one per plane), as RGBA `f32`
/// pixels: every value exact (half NaNs keep their sign and payload), and 0 for the alpha of
/// an RGB image.
fn decode(layout: Layout, depth: Depth, buffers: &[Vec<u8>]) -> Vec<f32> {
    let item = channel_bytes(depth);
    let value = |buffer: &[u8], index: usize| -> f32 {
        let b = &buffer[index * item..(index + 1) * item];
        match depth {
            Depth::Uint8 => f32::from(b[0]),
            Depth::Uint10 | Depth::Uint12 | Depth::Uint16 => {
                f32::from(u16::from_ne_bytes([b[0], b[1]]))
            }
            Depth::F16 => half_to_f32(u16::from_ne_bytes([b[0], b[1]])),
            Depth::F32 => f32::from_ne_bytes([b[0], b[1], b[2], b[3]]),
        }
    };
    let positions: [Option<usize>; 4] = match layout {
        Layout::PackedRgba | Layout::PlanarRgba => [Some(0), Some(1), Some(2), Some(3)],
        Layout::PackedRgb | Layout::PlanarRgb => [Some(0), Some(1), Some(2), None],
        Layout::PackedBgra => [Some(2), Some(1), Some(0), Some(3)],
    };
    let channels = layout.channels();
    let mut out = Vec::with_capacity(WIDTH * HEIGHT * 4);
    for pixel in 0..WIDTH * HEIGHT {
        for position in positions {
            out.push(match position {
                None => 0.0,
                Some(c) if layout.planar() => value(&buffers[c], pixel),
                Some(c) => value(&buffers[0], pixel * channels + c),
            });
        }
    }
    out
}

/// The layout of the destination image of a job that applies from one image to another.
fn destination_layout(job: &Job) -> Layout {
    match &job.request.images[1] {
        ocio_testkit::image::Image::Packed(p) => match p.channels {
            Channels::Order(ChannelOrder::Bgra) => Layout::PackedBgra,
            Channels::Count(4) => Layout::PackedRgba,
            _ => Layout::PackedRgb,
        },
        ocio_testkit::image::Image::Planar(p) => {
            if p.planes.len() == 4 {
                Layout::PlanarRgba
            } else {
                Layout::PlanarRgb
            }
        }
    }
}

/// Compares the buffers after a job, the wheel's and the port's: the source's (when it isn't
/// the destination) byte for byte, and the destination image byte for byte or, where they
/// differ, through the case's comparison ([`battery::params::Case::compare_pixels`]), which applies
/// W0002 where it covers the case. Returns how many NaN values W0002 waived.
fn compare_images(
    case: &battery::params::Case<Calls>,
    job: &Job,
    wheel: &[Vec<u8>],
    port: &[Vec<u8>],
) -> Result<usize, String> {
    let sources = job.combo.layout.buffers();
    let output = if job.combo.in_place {
        0..sources
    } else {
        for k in 0..sources {
            if wheel[k] != port[k] {
                return Err(format!("source buffer {k} differs"));
            }
        }
        sources..wheel.len()
    };
    if wheel[output.clone()] == port[output.clone()] {
        return Ok(0);
    }
    let (layout, depth) = if job.combo.in_place {
        (job.combo.layout, job.combo.input)
    } else {
        (destination_layout(job), job.combo.output)
    };
    let source: Vec<Vec<u8>> = job.request.buffers[..sources]
        .iter()
        .map(Buffer::bytes)
        .collect();
    let inputs = decode(job.combo.layout, job.combo.input, &source);
    let expected = decode(layout, depth, &wheel[output.clone()]);
    let actual = decode(layout, depth, &port[output]);
    match case.compare_pixels(
        &w0002_combo(job.dir, &job.combo),
        &inputs,
        &expected,
        &actual,
    ) {
        Comparison::W0002 { waived } => Ok(waived),
        Comparison::Exact => Err("the destination's bytes differ, its values don't".into()),
        Comparison::Mismatch(report) => Err(report),
    }
}

/// The values of the Lut1DTransforms of the port's optimized processor of `job`, three per
/// entry, in order: the 1D LUTs its CPU processor renders.
fn port_luts(class: &Class, job: &Job, calls: &Calls) -> Result<Vec<Vec<f32>>, Exception> {
    let transform = port_transform(&calls.spec(job.dir))?;
    let mut config = (*Config::create_raw()).clone();
    if class.v1 {
        config.set_major_version(1).expect("version 1");
    }
    let processor = config.processor_in_direction(&transform, TransformDirection::Forward)?;
    let group = processor
        .optimized_processor_with_bit_depths(
            port_depth(job.combo.input),
            port_depth(job.combo.output),
            job.combo.flags(),
        )?
        .create_group_transform()?;
    let mut luts = Vec::new();
    for k in 0..group.num_transforms() {
        if let Transform::Lut1D(lut) = group.transform(k)? {
            let mut values = Vec::new();
            for i in 0..lut.length() {
                values.extend(lut.value(i)?);
            }
            luts.push(values);
        }
    }
    Ok(luts)
}

/// The values of the Lut1DTransforms of the wheel's optimized processor, three per entry.
fn wheel_luts(reply: &ProcessorOpsReply) -> Vec<Vec<f32>> {
    reply
        .optimized()
        .group
        .children
        .iter()
        .filter(|t| t.class == "Lut1DTransform")
        .map(|t| t.getter("getData").f32s())
        .collect()
}

/// What a `Lut1DTransform`'s renderer depends on: its length, whether its domain is the half
/// codes, and whether it adjusts the hue, as its spec sets them.
#[derive(Debug, Clone, Copy)]
struct Lut1D {
    length: u64,
    half_domain: bool,
    hue_adjust: bool,
}

impl Lut1D {
    fn of(calls: &Calls) -> Lut1D {
        let spec = calls.spec(Direction::Forward);
        let mut lut = Lut1D {
            length: 2,
            half_domain: false,
            hue_adjust: false,
        };
        for call in spec["calls"].as_array().expect("calls") {
            match call[0].as_str() {
                Some("setLength") => lut.length = call[1].as_u64().expect("a length"),
                Some("setInputHalfDomain") => lut.half_domain = call[1] == json!(true),
                Some("setHueAdjust") => lut.hue_adjust = call[1]["enum"] != "HUE_NONE",
                _ => {}
            }
        }
        lut
    }

    /// The bit depth whose codes look the LUT up without resampling it, if any
    /// (`Lut1DOpData::mayLookup`, src/OpenColorIO/ops/lut1d/Lut1DOpData.cpp:491-506 @ v2.5.2).
    fn lookup_depth(&self) -> Option<Depth> {
        if self.half_domain {
            return Some(Depth::F16);
        }
        match self.length {
            256 => Some(Depth::Uint8),
            1024 => Some(Depth::Uint10),
            4096 => Some(Depth::Uint12),
            65536 => Some(Depth::Uint16),
            _ => None,
        }
    }

    /// Whether the spec is large (a setter per entry): the sweep runs it at its lookup's input
    /// only, from and to packed RGBA images, to F32 and to its own bit depth, at
    /// `OPTIMIZATION_NONE`, `LOSSLESS` and `DEFAULT`.
    fn runs(&self, combo: &Combo) -> bool {
        self.length < 4096
            || (Some(combo.input) == self.lookup_depth()
                && combo.layout == Layout::PackedRgba
                && (combo.output == Depth::F32 || combo.output == combo.input)
                && matches!(combo.level, Some(0 | 1 | 5)))
    }
}

/// The inverse LUT's deferral (its set-up in the processor's `finalize`, WP 2.1).
const NOT_PORTED_INVERSE: &str = "Lut1D: the inverse 1D LUT is not ported yet (WP 2.1).";

/// Where the port refuses a `Lut1DTransform` at `combo` because the renderer upstream picks is
/// Phase 2's, with the stage and message; `None` where the renderer is a lookup, which the port
/// has, and which must match the wheel.
///
/// Upstream (src/OpenColorIO @ v2.5.2):
/// - an inverse LUT is set up when the processor finalizes it (`Lut1DOpData::finalize`, the
///   inverse's domain); the port refuses there;
/// - the optimizer leaves a single forward LUT alone: `FindSeparablePrefix` gives no prefix to
///   bake for it (OpOptimizers.cpp:473-509), and a hue adjustment has crosstalk anyway;
/// - `GetLut1DRenderer` (ops/lut1d/Lut1DOpCPU.cpp:1657-1754) picks the hue-adjust renderer, the
///   lookup where `mayLookup(inBD)` (one entry per integer code, or a half domain for half
///   codes), and otherwise the float renderer (F32 input) or one that interpolates the codes.
///   The port has the lookup; the others are Phase 2's (WP 2.5), and the codes' interpolation
///   is its "composing 1D LUTs" refusal.
fn lut1d_deferral(
    lut: &Lut1D,
    dir: Direction,
    combo: &Combo,
) -> Option<(&'static str, &'static str)> {
    if dir == Direction::Inverse {
        return Some(("processor", NOT_PORTED_INVERSE));
    }
    if lut.hue_adjust {
        return Some(("cpu_processor", NOT_PORTED_F32));
    }
    if combo.input == Depth::F32 {
        return Some(("cpu_processor", NOT_PORTED_F32));
    }
    if lut.lookup_depth() == Some(combo.input) {
        return None;
    }
    Some(("cpu_processor", NOT_PORTED_COMPOSE))
}

/// The deferrals of the `Lut1DTransform`'s plan, per message, in the quick tier and in the
/// others: a digest of the plan the test generates, so that it can't change unnoticed.
const LUT1D_DEFERRALS_QUICK: [(&str, usize); 3] = [
    (NOT_PORTED_COMPOSE, 1277),
    (NOT_PORTED_F32, 674),
    (NOT_PORTED_INVERSE, 2205),
];
const LUT1D_DEFERRALS_FULL: [(&str, usize); 3] = [
    (NOT_PORTED_COMPOSE, 5145),
    (NOT_PORTED_F32, 2695),
    (NOT_PORTED_INVERSE, 8847),
];

/// One apply: a case in a direction, a combination, and its request.
struct Job {
    case: usize,
    dir: Direction,
    combo: Combo,
    request: Request,
    /// For the `Lut1DTransform`: its Phase 2 deferral at this combination, if any.
    deferral: Option<(&'static str, &'static str)>,
}

/// The jobs of `class`: in the quick tier, each combination for [`QUICK_TURNS`] of the cases and
/// directions, in turn, and for the `Lut1DTransform` also every case and direction the port
/// looks up; otherwise every combination for every case in both directions. Less for the
/// `Lut1DTransform`'s large specs ([`Lut1D::runs`]).
fn jobs(class: &Class, tier: Tier) -> Vec<Job> {
    let combos = combos();
    let cases = &class.cases.cases;
    let luts: Vec<Option<Lut1D>> = cases
        .iter()
        .map(|c| class.lut1d.then(|| Lut1D::of(c.params())))
        .collect();
    let turns: Vec<(usize, Direction)> = (0..cases.len())
        .flat_map(|c| [(c, Direction::Forward), (c, Direction::Inverse)])
        .collect();
    let mut jobs = Vec::new();
    for (k, combo) in combos.iter().enumerate() {
        let runs: Vec<(usize, Direction)> = turns
            .iter()
            .copied()
            .filter(|&(c, _)| luts[c].is_none_or(|lut| lut.runs(combo)))
            .collect();
        let deferral =
            |(c, dir): (usize, Direction)| luts[c].and_then(|lut| lut1d_deferral(&lut, dir, combo));
        let picked: Vec<(usize, Direction)> = if tier == Tier::Quick && !runs.is_empty() {
            let mut picked: Vec<(usize, Direction)> = (0..QUICK_TURNS.min(runs.len()))
                .map(|t| runs[(k * QUICK_TURNS + t) % runs.len()])
                .collect();
            for &turn in &runs {
                if class.lut1d && deferral(turn).is_none() && !picked.contains(&turn) {
                    picked.push(turn);
                }
            }
            picked
        } else {
            runs.clone()
        };
        for (case, dir) in picked {
            let spec = cases[case].params().spec(dir);
            let mut processor = json!({
                "transform": spec,
                "in_bitdepth": combo.input.oracle_name(),
                "out_bitdepth": combo.output.oracle_name(),
            });
            if let Some(level) = combo.level {
                processor["optimization"] = json!(LEVELS[level].0);
            }
            if class.v1 {
                processor["config"] = json!({"yaml": V1_CONFIG});
            }
            let mut request = Request::new(processor);
            let seed = k as u64;
            let src = add_image(&mut request, combo.layout, combo.input, Some(seed));
            request.apply = if combo.in_place {
                vec![src]
            } else {
                let next = if luts[case].is_some_and(|lut| lut.length >= 4096) {
                    combo.layout
                } else {
                    LAYOUTS[(k / 7 + 1) % LAYOUTS.len()]
                };
                let dst = add_image(&mut request, next, combo.output, None);
                vec![src, dst]
            };
            jobs.push(Job {
                case,
                dir,
                combo: *combo,
                request,
                deferral: deferral((case, dir)),
            });
        }
    }
    jobs
}

/// A bit depth's name in the Python API.
fn depth_name(depth: BitDepth) -> &'static str {
    match depth {
        BitDepth::Uint8 => "BIT_DEPTH_UINT8",
        BitDepth::Uint10 => "BIT_DEPTH_UINT10",
        BitDepth::Uint12 => "BIT_DEPTH_UINT12",
        BitDepth::Uint16 => "BIT_DEPTH_UINT16",
        BitDepth::F16 => "BIT_DEPTH_F16",
        BitDepth::F32 => "BIT_DEPTH_F32",
        other => panic!("no CPU processor of {other:?}"),
    }
}

/// The stage and message of an error, or the cache IDs and getters and the buffers.
type PortOutcome = Result<(Value, Vec<Vec<u8>>), (String, String)>;

/// What the port does for a job: the stage and message of its error, or the cache IDs and
/// getters as the oracle reports them, and every buffer after the apply.
fn port(class: &Class, job: &Job, calls: &Calls) -> PortOutcome {
    let fail =
        |stage: &'static str| move |e: Exception| (stage.to_string(), e.message().to_string());
    let transform = port_transform(&calls.spec(job.dir)).map_err(fail("transform"))?;
    let mut config = (*Config::create_raw()).clone();
    if class.v1 {
        config.set_major_version(1).expect("version 1");
    }
    let processor = config
        .processor_in_direction(&transform, TransformDirection::Forward)
        .map_err(fail("processor"))?;
    let (input, output) = (port_depth(job.combo.input), port_depth(job.combo.output));
    let cpu: Arc<CpuProcessor> = match job.combo.level {
        None if input == BitDepth::F32 && output == BitDepth::F32 => {
            processor.default_cpu_processor()
        }
        None => processor.optimized_cpu_processor_with_bit_depths(
            input,
            output,
            OptimizationFlags::DEFAULT,
        ),
        Some(level) => {
            processor.optimized_cpu_processor_with_bit_depths(input, output, LEVELS[level].1)
        }
    }
    .map_err(fail("cpu_processor"))?;
    let result = json!({
        "processor_cache_id": processor.cache_id().map_err(fail("processor"))?,
        "cpu_cache_id": String::from_utf8(cpu.get_cache_id().to_vec()).expect("UTF-8"),
        "cpu_processor": {
            "getInputBitDepth": depth_name(cpu.get_input_bit_depth()),
            "getOutputBitDepth": depth_name(cpu.get_output_bit_depth()),
            "isNoOp": cpu.is_no_op(),
            "isIdentity": cpu.is_identity(),
            "hasChannelCrosstalk": cpu.has_channel_crosstalk(),
        },
    });

    let mut buffers: Vec<Vec<u8>> = job.request.buffers.iter().map(Buffer::bytes).collect();
    let src_buffers = job.combo.layout.buffers();
    let (src, dst) = buffers.split_at_mut(src_buffers);
    let src_refs: Vec<&mut [u8]> = src.iter_mut().map(Vec::as_mut_slice).collect();
    if job.combo.in_place {
        let mut img =
            port_image(job.combo.layout, job.combo.input, src_refs).map_err(fail("image"))?;
        cpu.apply(img.desc_mut()).map_err(fail("apply"))?;
    } else {
        let next = match &job.request.images[1] {
            ocio_testkit::image::Image::Packed(p) => match p.channels {
                Channels::Order(ChannelOrder::Bgra) => Layout::PackedBgra,
                Channels::Count(4) => Layout::PackedRgba,
                _ => Layout::PackedRgb,
            },
            ocio_testkit::image::Image::Planar(p) => {
                if p.planes.len() == 4 {
                    Layout::PlanarRgba
                } else {
                    Layout::PlanarRgb
                }
            }
        };
        let src_shared: Vec<&[u8]> = src_refs.into_iter().map(|b| &*b).collect();
        let src_img =
            port_image(job.combo.layout, job.combo.input, src_shared).map_err(fail("image"))?;
        let dst_refs: Vec<&mut [u8]> = dst.iter_mut().map(Vec::as_mut_slice).collect();
        let mut dst_img = port_image(next, job.combo.output, dst_refs).map_err(fail("image"))?;
        cpu.apply_src_dst(src_img.desc(), dst_img.desc_mut())
            .map_err(fail("apply"))?;
    }
    Ok((result, buffers))
}

/// What W0002 covered in a class's jobs.
#[derive(Debug, Default)]
struct Waived {
    /// Destination images with NaN values that differ in sign or payload bits only.
    images: usize,
    /// Those values.
    values: usize,
    /// CPU processors' cache IDs that differ in the hashes of baked 1D LUTs only.
    cache_ids: usize,
    /// The NaN entries of those LUTs that differ in sign or payload bits only.
    lut_entries: usize,
}

/// Runs every job of `class` against the wheel; panics with a report if any differs.
fn check(class: &Class) {
    let tier = Tier::current();
    let jobs = jobs(class, tier);
    if class.lut1d {
        let mut planned: BTreeMap<&str, usize> = BTreeMap::new();
        for job in &jobs {
            if let Some((_, message)) = job.deferral {
                *planned.entry(message).or_default() += 1;
            }
        }
        let pinned = if tier == Tier::Quick {
            LUT1D_DEFERRALS_QUICK
        } else {
            LUT1D_DEFERRALS_FULL
        };
        let pinned: BTreeMap<&str, usize> = pinned.into_iter().collect();
        assert_eq!(
            planned,
            pinned,
            "the {} tier's plan of {} applies has other deferrals than pinned",
            tier.name(),
            jobs.len()
        );
    }
    let mut failures = Vec::new();
    let mut deferred: BTreeMap<String, usize> = BTreeMap::new();
    let mut compared = 0;
    let mut refusals = 0;
    let mut waived = Waived::default();
    // The jobs whose CPU processors' cache IDs differ, with both IDs.
    let mut baked: Vec<(&Job, String, String)> = Vec::new();
    // Batches of a bounded size, to keep each request's JSON small.
    for batch in jobs.chunks(500) {
        let calls: Vec<_> = batch.iter().map(|job| job.request.call()).collect();
        let responses = Oracle::get().batch(&calls, true);
        for (job, response) in batch.iter().zip(responses) {
            let case = &class.cases.cases[job.case];
            let what = format!(
                "{} \"{}\" {:?} {:?}",
                class.name,
                case.label(),
                job.dir,
                job.combo
            );
            let reply = job
                .request
                .reply(response.unwrap_or_else(|e| panic!("{what}: {e}")));
            if !reply.log().is_empty() {
                failures.push(format!("{what}: OCIO logged {:?}", reply.log()));
            }
            let port = port(class, job, case.params());
            if let Some((stage, message)) = job.deferral {
                // The wheel renders it; the port refuses it, there, with that message.
                match (reply.raised(), &port) {
                    (None, Err((s, m))) if s == stage && m == message => {
                        *deferred.entry(message.to_string()).or_default() += 1;
                    }
                    (raised, port) => failures.push(format!(
                        "{what}: a Phase 2 deferral ({stage}: {message}) was expected\n  \
                         wheel {:?}\n  port  {:?}",
                        raised.map(|r| (r.stage, r.message)),
                        port.as_ref().map(|_| ())
                    )),
                }
                continue;
            }
            match (reply.raised(), port) {
                (Some(raised), Err((stage, message)))
                    if raised.stage == stage && raised.message == message =>
                {
                    refusals += 1;
                }
                (None, Ok((result, buffers))) => {
                    let wheel = json!({
                        "processor_cache_id": reply.result["processor_cache_id"],
                        "cpu_cache_id": reply.result["cpu_cache_id"],
                        "cpu_processor": reply.result["cpu_processor"],
                    });
                    compared += 1;
                    match compare_images(case, job, &reply.buffers, &buffers) {
                        Ok(0) => {}
                        Ok(n) => {
                            waived.images += 1;
                            waived.values += n;
                        }
                        Err(report) => failures.push(format!("{what}\n  {report}")),
                    }
                    if wheel == result {
                        continue;
                    }
                    // Only the CPU processor's cache ID may differ, where it hashes 1D LUTs
                    // baked from NaN parameters: the second round reads them back.
                    let mut others = result.clone();
                    others["cpu_cache_id"] = wheel["cpu_cache_id"].clone();
                    if others == wheel {
                        let id = |v: &Value| v["cpu_cache_id"].as_str().expect("an ID").to_string();
                        baked.push((job, id(&wheel), id(&result)));
                    } else {
                        failures.push(format!("{what}\n  wheel {wheel}\n  port  {result}"));
                    }
                }
                (raised, port) => failures.push(format!(
                    "{what}\n  wheel {:?}\n  port  {:?}",
                    raised.map(|r| (r.stage, r.message)),
                    port.map(|_| ())
                )),
            }
        }
    }
    // The cache IDs that differ: the optimized processors' 1D LUTs, from both sides.
    let requests: Vec<ProcessorOpsRequest> = baked
        .iter()
        .map(|(job, _, _)| {
            let mut processor = json!({"transform": job.request.processor["transform"]});
            if class.v1 {
                processor["config"] = json!({"yaml": V1_CONFIG});
            }
            let mut request = ProcessorOpsRequest::new(processor);
            request.in_bitdepth = Some(job.combo.input);
            request.out_bitdepth = Some(job.combo.output);
            request.optimization = Some(json!(
                job.combo
                    .level
                    .map_or("OPTIMIZATION_DEFAULT", |l| LEVELS[l].0)
            ));
            request
        })
        .collect();
    let calls: Vec<_> = requests.iter().map(ProcessorOpsRequest::call).collect();
    for ((job, wheel_id, port_id), response) in baked.iter().zip(Oracle::get().batch(&calls, true))
    {
        let case = &class.cases.cases[job.case];
        let what = format!(
            "{} \"{}\" {:?} {:?}",
            class.name,
            case.label(),
            job.dir,
            job.combo
        );
        let reply = ProcessorOpsReply::from_response(response.unwrap_or_else(|e| panic!("{e}")));
        let port = match port_luts(class, job, case.params()) {
            Ok(luts) => luts,
            Err(e) => {
                failures.push(format!("{what}: the port's LUTs: {}", e.message()));
                continue;
            }
        };
        let combo = w0002_combo(job.dir, &job.combo);
        match case.compare_baked_luts(&combo, wheel_id, port_id, &wheel_luts(&reply), &port) {
            Comparison::W0002 { waived: n } => {
                waived.cache_ids += 1;
                waived.lut_entries += n;
            }
            Comparison::Exact => {}
            Comparison::Mismatch(report) => failures.push(format!("{what}\n  {report}")),
        }
    }
    println!(
        "{}: {} applies ({} tier): {compared} compared, {refusals} refusals compared, {} \
         deferred to Phase 2{}\n  W0002: {} images with {} NaN values differing in sign or \
         payload bits only; {} cache IDs of 1D LUTs baked with {} such NaN entries",
        class.name,
        jobs.len(),
        tier.name(),
        deferred.values().sum::<usize>(),
        deferred
            .iter()
            .map(|(m, n)| format!("\n  {n}: {m}"))
            .collect::<String>(),
        waived.images,
        waived.values,
        waived.cache_ids,
        waived.lut_entries
    );
    assert!(
        failures.is_empty(),
        "{} of {} applies differ:\n{}",
        failures.len(),
        jobs.len(),
        failures[..failures.len().min(20)].join("\n")
    );
    assert!(compared > 0, "{}: nothing compared", class.name);
}

#[test]
fn matrix_transform_matches_the_wheel_at_every_format_and_level() {
    check(&Class::new("MatrixTransform", api_cases::matrix()));
}

#[test]
fn range_transform_matches_the_wheel_at_every_format_and_level() {
    check(&Class::new("RangeTransform", api_cases::range()));
}

#[test]
fn cdl_transform_matches_the_wheel_at_every_format_and_level() {
    check(&Class::new("CDLTransform", api_cases::cdl()));
}

#[test]
fn log_transform_matches_the_wheel_at_every_format_and_level() {
    check(&Class::new("LogTransform", api_cases::log()));
}

#[test]
fn log_affine_transform_matches_the_wheel_at_every_format_and_level() {
    check(&Class::new("LogAffineTransform", api_cases::log_affine()));
}

#[test]
fn log_camera_transform_matches_the_wheel_at_every_format_and_level() {
    check(&Class::new("LogCameraTransform", api_cases::log_camera()));
}

#[test]
fn exponent_transform_matches_the_wheel_at_every_format_and_level() {
    check(&Class::new("ExponentTransform", api_cases::exponent()));
    check(&Class {
        v1: true,
        ..Class::new("ExponentTransform (version 1)", api_cases::exponent_v1())
    });
}

#[test]
fn exponent_with_linear_transform_matches_the_wheel_at_every_format_and_level() {
    check(&Class::new(
        "ExponentWithLinearTransform",
        api_cases::exponent_with_linear(),
    ));
}

#[test]
fn allocation_transform_matches_the_wheel_at_every_format_and_level() {
    check(&Class::new("AllocationTransform", api_cases::allocation()));
}

#[test]
fn group_transform_matches_the_wheel_at_every_format_and_level() {
    check(&Class::new("GroupTransform", api_cases::group()));
}

#[test]
fn lut1d_transform_matches_the_wheel_at_every_format_and_level() {
    let mut cases = api_cases::lut1d();
    cases.cases.extend(api_cases::lut1d_lookups().cases);
    check(&Class {
        lut1d: true,
        ..Class::new("Lut1DTransform", cases)
    });
}

#[test]
fn fixed_function_transform_matches_the_wheel_at_every_format_and_level() {
    check(&Class::new(
        "FixedFunctionTransform",
        api_cases::fixed_function(),
    ));
}
