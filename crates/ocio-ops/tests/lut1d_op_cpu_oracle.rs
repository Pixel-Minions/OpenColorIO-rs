// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The 1D LUT lookups against the wheel, bit for bit: a processor of one `Lut1DTransform`
//! (built empty and set, as in `lut1d_op_data_oracle.rs`) with one entry per code of its
//! input bit depth, or a half domain for half input, without optimization, so that the LUT is
//! the CPU engine's first op and its lookup the input's conversion (`CreateCPUEngine`,
//! src/OpenColorIO/CPUProcessor.cpp:140-146 @ v2.5.2; `Lut1DRenderer` and
//! `Lut1DRendererHalfCode`, src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp:501-536, 622-650; with hue
//! adjust, `Lut1DRendererHueAdjust` and `Lut1DRendererHalfCodeHueAdjust`, 753-894).
//!
//! - Every code of each input bit depth (8, 10, 12, 16 bits and half) on each channel, to F32,
//!   and through the conversion to 8- and 16-bit and half output, for LUTs of negative, large
//!   and tiny values and, in a half domain, the NaN and infinite entries `setLength` leaves
//!   (which the renderer sanitizes); with and without hue adjust;
//! - `applyRGB` and `applyRGBA`, which convert the pixel's own bytes (docs/improvements.md,
//!   I-41), through `apply(src, dst)` over one pixel's bytes, as
//!   `cpu_processor_apply_rgb_oracle.rs` checks them, for 8-, 16-bit and half input, with and
//!   without hue adjust, whose renderers read alpha in another order. With 10-
//!   and 12-bit input the lookups read past the tables there (U-1).
//!
//! And the same for inverse LUTs, which the CPU engine renders from the input bit depth too,
//! but never looks up (`InvLut1DRenderer`, `InvLut1DRendererHalfCode`,
//! `InvLut1DRendererHueAdjust`, `InvLut1DRendererHalfCodeHueAdjust`, Lut1DOpCPU.cpp:1137-1625):
//! every code, of LUTs of every length, a half domain from integer input included
//! (docs/improvements.md, I-69), and `applyRGB` and `applyRGBA` from every integer and half
//! input, standard and half domains, with and without hue adjust.

mod common;

use core::ffi::c_ulong;

use common::image::{depth_name, port_depth, port_image};
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::exception::Result;
use ocio_ops::image_desc::Bytes;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, Lut1DHueAdjust, OptimizationFlags, TransformDirection};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op::create_lut1d_op;
use ocio_ops::ops::lut1d::lut1d_op_data::Lut3by1DArray;
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::image::{Buffer, Channels, Data, Packed, Reply, Request, Stride};
use ocio_testkit::oracle::BatchCall;
use serde_json::{Value, json};

/// A `Lut1DTransform`: a half domain or not, its length, and each entry's RGB from the
/// identity's value (`None` keeps the identity).
#[derive(Debug, Clone, Copy)]
struct Lut {
    half_domain: bool,
    length: c_ulong,
    curve: Option<fn(f32) -> [f32; 3]>,
    /// `HUE_DW3`, or `HUE_NONE`.
    hue_adjust: bool,
    /// `TRANSFORM_DIR_INVERSE`, or `TRANSFORM_DIR_FORWARD`.
    inverse: bool,
}

impl Lut {
    fn identity(&self) -> Lut3by1DArray {
        let mut data = Lut1DOpData::new(2).unwrap();
        data.set_input_half_domain(self.half_domain);
        Lut3by1DArray::new(data.get_half_flags(), 3, self.length, false).unwrap()
    }

    /// The entries `setValue` sets: the curve's finite values.
    fn entries(&self) -> Vec<(usize, [f32; 3])> {
        let Some(curve) = self.curve else {
            return Vec::new();
        };
        let identity = self.identity();
        (0..self.length as usize)
            .map(|i| (i, curve(identity[3 * i])))
            .filter(|(_, rgb)| rgb.iter().all(|v| v.is_finite()))
            .collect()
    }

    fn spec(&self) -> Value {
        let mut calls = vec![
            json!(["setInputHalfDomain", self.half_domain]),
            json!(["setLength", self.length]),
        ];
        for (index, rgb) in self.entries() {
            calls.push(json!(["setValue", index, rgb[0], rgb[1], rgb[2]]));
        }
        if self.hue_adjust {
            calls.push(json!(["setHueAdjust", {"enum": "HUE_DW3"}]));
        }
        if self.inverse {
            calls.push(json!(["setDirection", {"enum": "TRANSFORM_DIR_INVERSE"}]));
        }
        json!({"class": "Lut1DTransform", "args": {}, "calls": calls})
    }

    /// The port's data, as the transform's setters make it.
    fn port(&self) -> Lut1DOpData {
        let mut data = Lut1DOpData::new(2).unwrap();
        data.set_input_half_domain(self.half_domain);
        if self.hue_adjust {
            data.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
        }
        *data.get_array_mut() = self.identity();
        for (index, rgb) in self.entries() {
            for (c, v) in rgb.into_iter().enumerate() {
                data.get_array_mut()[3 * index + c] = v;
            }
        }
        data
    }

    /// The LUT for `depth`'s lookups.
    fn for_depth(depth: Depth, curve: Option<fn(f32) -> [f32; 3]>) -> Lut {
        let half_domain = depth == Depth::F16;
        let length = Lut1DOpData::get_lut_ideal_size(port_depth(depth)).unwrap();
        Lut {
            half_domain,
            length,
            curve,
            hue_adjust: false,
            inverse: false,
        }
    }

    /// A half domain, or a standard domain of 256 entries.
    fn of_domain(half_domain: bool, curve: fn(f32) -> [f32; 3]) -> Lut {
        Lut {
            half_domain,
            length: if half_domain { 65536 } else { 256 },
            curve: Some(curve),
            hue_adjust: false,
            inverse: false,
        }
    }

    /// The LUT with hue adjust.
    fn with_hue_adjust(mut self) -> Lut {
        self.hue_adjust = true;
        self
    }

    /// The LUT in the inverse direction.
    fn inverted(mut self) -> Lut {
        self.inverse = true;
        self
    }

    fn direction(&self) -> TransformDirection {
        if self.inverse {
            TransformDirection::Inverse
        } else {
            TransformDirection::Forward
        }
    }
}

fn mixed(x: f32) -> [f32; 3] {
    [x * x, 1.0 - 2.0 * x, x * 4.0 - 1.5]
}

fn extreme(x: f32) -> [f32; 3] {
    [x * 3.0e38, -x * 3.0e38, x * 1.0e-30]
}

/// The processor of `lut` from `input` to `output`, without optimization.
fn processor(lut: &Lut, input: Depth, output: Depth) -> Value {
    json!({
        "transform": {"class": "GroupTransform", "children": [lut.spec()]},
        "optimization": "OPTIMIZATION_NONE",
        "in_bitdepth": depth_name(port_depth(input)),
        "out_bitdepth": depth_name(port_depth(output)),
    })
}

/// The port's CPU processor of `lut`: `BuildLut1DOp` validates the data and makes the op, the
/// processor finalizes it (src/OpenColorIO/ops/lut1d/Lut1DOp.cpp:244-253,
/// src/OpenColorIO/Processor.cpp:623-641 @ v2.5.2).
fn port_processor(lut: &Lut, input: Depth, output: Depth) -> Result<CpuProcessor> {
    let data = lut.port();
    data.validate()?;
    let mut raw = OpVec::new();
    create_lut1d_op(&mut raw, data, lut.direction());
    raw.finalize()?;
    CpuProcessor::new(
        &raw,
        port_depth(input),
        port_depth(output),
        OptimizationFlags::NONE,
    )
}

/// The bytes of a channel of `depth`.
fn size(depth: Depth) -> usize {
    match depth {
        Depth::Uint8 => 1,
        Depth::F32 => 4,
        _ => 2,
    }
}

/// Every code of `depth` on each channel: red `i`, green the codes backwards, blue a
/// permutation, alpha `i`, in rows of 256 pixels.
fn ramp(depth: Depth) -> (Vec<u8>, i64) {
    let n = Lut1DOpData::get_lut_ideal_size(port_depth(depth)).unwrap() as usize;
    let mut bytes = Vec::with_capacity(4 * n * size(depth));
    for i in 0..n {
        for code in [i, n - 1 - i, (i * 97) % n, i] {
            if size(depth) == 1 {
                bytes.push(code as u8);
            } else {
                bytes.extend_from_slice(&(code as u16).to_ne_bytes());
            }
        }
    }
    (bytes, (n / 256).max(1) as i64)
}

fn packed(buffer: usize, width: i64, height: i64, depth: Depth) -> Packed {
    Packed::new(Data::at(buffer, 0), width, height, Channels::Count(4))
        .layout(depth, [Stride::Auto; 3])
}

#[test]
fn every_code_matches_the_wheel() {
    const IN: [Depth; 5] = [
        Depth::Uint8,
        Depth::Uint10,
        Depth::Uint12,
        Depth::Uint16,
        Depth::F16,
    ];
    const OUT: [Depth; 4] = [Depth::F32, Depth::Uint16, Depth::Uint8, Depth::F16];
    let mut cases = Vec::new();
    for input in IN {
        for curve in [None, Some(mixed as fn(f32) -> [f32; 3]), Some(extreme)] {
            for output in OUT {
                cases.push((Lut::for_depth(input, curve), input, output));
            }
        }
        for curve in [Some(mixed as fn(f32) -> [f32; 3]), Some(extreme)] {
            for output in OUT {
                let lut = Lut::for_depth(input, curve).with_hue_adjust();
                cases.push((lut, input, output));
                // The inverse LUT, with and without hue adjust.
                let lut = Lut::for_depth(input, curve).inverted();
                cases.push((lut, input, output));
                cases.push((lut.with_hue_adjust(), input, output));
            }
        }
    }
    // Inverse LUTs whose domain isn't the input's: a half domain from integer input, a
    // standard domain from half input.
    for (half_domain, input) in [
        (true, Depth::Uint8),
        (true, Depth::Uint16),
        (false, Depth::F16),
    ] {
        for curve in [mixed as fn(f32) -> [f32; 3], extreme] {
            let lut = Lut::of_domain(half_domain, curve).inverted();
            cases.push((lut, input, Depth::F32));
            cases.push((lut.with_hue_adjust(), input, Depth::F32));
        }
    }
    let mut failures = Vec::new();
    // A few cases at a time: a LUT of 65536 entries is a long spec.
    for cases in cases.chunks(16) {
        let requests: Vec<Request> = cases
            .iter()
            .map(|(lut, input, output)| {
                let (bytes, height) = ramp(*input);
                let mut request = Request::new(processor(lut, *input, *output));
                let src = request.buffer(Buffer::Bytes(bytes));
                let dst =
                    request.buffer(Buffer::Bytes(vec![
                        0;
                        4 * 256 * height as usize * size(*output)
                    ]));
                let width = 256;
                request.image(packed(src, width, height, *input));
                request.image(packed(dst, width, height, *output));
                request.apply = vec![0, 1];
                request
            })
            .collect();
        let calls: Vec<BatchCall<'_>> = requests.iter().map(Request::call).collect();
        let responses = Oracle::get().batch(&calls, true);

        for (((lut, input, output), request), response) in
            cases.iter().zip(&requests).zip(responses)
        {
            let what = format!("{lut:?} {input:?}->{output:?}");
            let reply: Reply = request.reply(response.unwrap_or_else(|e| panic!("{what}: {e}")));
            assert!(reply.raised().is_none(), "{what}: {:?}", reply.raised());
            let mut buffers: Vec<Vec<u8>> = request.buffers.iter().map(Buffer::bytes).collect();
            let port = (|| -> Result<()> {
                let cpu = port_processor(lut, *input, *output)?;
                let (src_buffers, dst_buffers) = buffers.split_at_mut(1);
                let src = port_image(&request.images[0], |_| Bytes(&src_buffers[0][..]))?;
                let mut slot = Some(&mut dst_buffers[0][..]);
                let mut dst = port_image(&request.images[1], |_| {
                    Bytes(slot.take().expect("one buffer"))
                })?;
                cpu.apply_src_dst(src.desc(), dst.desc_mut())
            })();
            match port {
                Err(e) => failures.push(format!("{what}: the port raised {}", e.message())),
                Ok(()) if buffers[1] != reply.buffers[1] => {
                    failures.push(format!("{what}: the pixels differ"));
                }
                Ok(()) => {}
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

/// `applyRGB` and `applyRGBA` with 8-, 16-bit and half input to F32, through `apply(src, dst)`
/// over one pixel's 16 bytes: the lookup reads the pixel's first bytes as codes and writes four
/// floats over them, in the order each wheel compiled (module docs).
///
/// The pixels have no zero channel, so that every code the lookup reads, before or after the
/// store of the float before it, is a byte of a non-zero float: the order of the reads and
/// stores shows in the result, at 8 bits on red too.
///
/// The inverse LUTs' renderers, standard and half domains, with and without hue adjust, from
/// 8-, 10-, 12-, 16-bit and half input, each in the order its wheel compiled
/// (`InvLut1DRenderer::in_place_order`): they never index by the code, so 10- and 12-bit
/// codes past the LUT are values like any other.
#[test]
fn the_in_place_lookup_follows_the_wheel() {
    /// The RGBA pixel's bytes, and the RGB pixel's {r, g, b, 0.0f}.
    fn pixels(seed: u16) -> [Vec<u8>; 2] {
        let s = f32::from(seed);
        let rgba: Vec<u8> = [0.13 + 0.071 * s, 0.61 - 0.043 * s, 0.37 + 0.05 * s, 0.9]
            .iter()
            .flat_map(|v: &f32| v.to_ne_bytes())
            .collect();
        let mut rgb = rgba[..12].to_vec();
        rgb.extend_from_slice(&0.0f32.to_ne_bytes());
        [rgba, rgb]
    }
    let mut cases = Vec::new();
    for input in [Depth::Uint8, Depth::Uint16, Depth::F16] {
        for curve in [Some(mixed as fn(f32) -> [f32; 3]), Some(extreme)] {
            for seed in 0..8u16 {
                let lut = Lut::for_depth(input, curve);
                cases.push((lut, input, pixels(seed)));
                cases.push((lut.with_hue_adjust(), input, pixels(seed)));
            }
        }
    }
    for input in [
        Depth::Uint8,
        Depth::Uint10,
        Depth::Uint12,
        Depth::Uint16,
        Depth::F16,
    ] {
        for half_domain in [false, true] {
            for curve in [mixed as fn(f32) -> [f32; 3], extreme] {
                for seed in 0..4u16 {
                    let lut = Lut::of_domain(half_domain, curve).inverted();
                    cases.push((lut, input, pixels(seed)));
                    cases.push((lut.with_hue_adjust(), input, pixels(seed)));
                }
            }
        }
    }
    let floats = |bytes: &[u8]| -> [f32; 4] {
        std::array::from_fn(|k| f32::from_ne_bytes(bytes[4 * k..4 * k + 4].try_into().unwrap()))
    };
    let bytes_of =
        |values: &[f32]| -> Vec<u8> { values.iter().flat_map(|v| v.to_ne_bytes()).collect() };
    let mut failures = Vec::new();
    // A few cases at a time: a LUT of 65536 entries is a long spec.
    for cases in cases.chunks(8) {
        let requests: Vec<Request> = cases
            .iter()
            .flat_map(|(lut, input, pixels)| {
                pixels.iter().map(move |bytes| {
                    let mut request = Request::new(processor(lut, *input, Depth::F32));
                    let buffer = request.buffer(Buffer::Bytes(bytes.clone()));
                    request.image(packed(buffer, 1, 1, *input));
                    request.image(packed(buffer, 1, 1, Depth::F32));
                    request.apply = vec![0, 1];
                    request
                })
            })
            .collect();
        let calls: Vec<BatchCall<'_>> = requests.iter().map(Request::call).collect();
        let responses = Oracle::get().batch(&calls, true);
        let replies: Vec<Reply> = responses
            .into_iter()
            .zip(&requests)
            .map(|(r, request)| request.reply(r.unwrap_or_else(|e| panic!("{e}"))))
            .collect();

        for ((lut, input, [rgba, rgb]), wheel) in cases.iter().zip(replies.chunks(2)) {
            let cpu = port_processor(lut, *input, Depth::F32).unwrap();
            let mut px = floats(rgba);
            cpu.apply_rgba(&mut px).unwrap();
            let rgb4 = floats(rgb);
            let mut px3 = [rgb4[0], rgb4[1], rgb4[2]];
            cpu.apply_rgb(&mut px3).unwrap();
            for (k, (port, wheel)) in [bytes_of(&px), bytes_of(&px3)]
                .iter()
                .zip(wheel)
                .enumerate()
            {
                assert!(wheel.raised().is_none());
                let n = port.len();
                if port[..] != wheel.buffers[0][..n] {
                    failures.push(format!(
                        "{} {input:?} {:02x?}\n  wheel {:02x?}\n  port  {:02x?}",
                        ["applyRGBA", "applyRGB"][k],
                        [rgba, rgb][k],
                        &wheel.buffers[0][..n],
                        port
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        2 * cases.len(),
        failures[..failures.len().min(10)].join("\n")
    );
    let _ = BitDepth::F32;
}
