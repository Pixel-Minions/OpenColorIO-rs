// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The optimizer's separable-prefix bake against the wheel, entry for entry: with integer and
//! half input, the default and the full flags (`OPTIMIZATION_COMP_SEPARABLE_PREFIX`) replace
//! the leading ops that don't mix channels, if one of them isn't a Matrix or a Range, with a
//! 1D LUT that looks the input's codes up (`OptimizeSeparablePrefix` and
//! `FindSeparablePrefix`, src/OpenColorIO/OpOptimizers.cpp:464-596 @ v2.5.2). The LUT is the
//! lookup domain (`Lut1DOpData::MakeLookupDomain`, src/OpenColorIO/ops/lut1d/
//! Lut1DOpData.cpp:508-526) rendered through copies of the prefix's ops at F32, without fast
//! math (`ComposeVec` and `EvalTransform`, Lut1DOpData.cpp:683-705, src/OpenColorIO/ops/
//! OpTools.cpp:11-46), so its entries follow each platform's math library.
//!
//! - The optimized processor (`getOptimizedProcessor`: finalize, optimize, optimizeForBitdepth,
//!   src/OpenColorIO/Processor.cpp:382-399), through the oracle's `processor_ops`: its cache
//!   ID, and its first transform's `getData()`, the baked LUT's values, bit for bit;
//! - the CPU processor's pixels, for every code of each input bit depth on each channel.
//!
//! The bake uses each platform's math library, so its entries can differ between Windows and
//! Linux (docs/improvements.md, I-24): the checks compare each platform's wheel with the port
//! on that platform. They belong to `cpu-tests`.
//!
//! The prefixes hold Log, LogAffine and LogCamera transforms, Gamma ops (ExponentTransform and
//! ExponentWithLinearTransform in a version 2 config), CDL ops, and Exponent ops (a version 1
//! config's ExponentTransform), with Matrix and Range transforms before and after them; a
//! matrix that mixes channels, or a CDL with a saturation, ends the prefix; a prefix of Matrix
//! and Range transforms alone isn't baked, and neither is a single forward 1D LUT.

mod common;

use common::cdl::Cdl;
use common::image::{depth_name, port_depth, port_image};
use common::log_chain::{Affine, T, port_raw_ops};
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::exception::Result;
use ocio_ops::hash_utils::cache_id_hash;
use ocio_ops::image_desc::Bytes;
use ocio_ops::op::OpVec;
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{CdlStyle, NegativeStyle, OptimizationFlags, TransformDirection};
use ocio_ops::ops::exponent::ExponentOpData;
use ocio_ops::ops::exponent::exponent_op::create_exponent_op;
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::battery::{Combo, Direction, Format, Spec, yaml_list};
use ocio_testkit::image::{Buffer, Channels, Data, Packed, Request, Stride};
use ocio_testkit::oracle::BatchCall;
use serde_json::{Value, json};

use TransformDirection::{Forward as F, Inverse as I};

/// The input bit depths the bake serves.
const INPUTS: [Depth; 5] = [
    Depth::Uint8,
    Depth::Uint10,
    Depth::Uint12,
    Depth::Uint16,
    Depth::F16,
];

/// The optimization levels that bake.
const FLAGS: [(&str, OptimizationFlags); 2] = [
    ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
    ("OPTIMIZATION_ALL", OptimizationFlags::ALL),
];

fn chains() -> Vec<Vec<T>> {
    let up: Affine = [[0.18; 3], [1.0; 3], [2.0; 3], [0.1; 3]];
    let unequal: Affine = [[0.5, 1.0, 1.5], [1.0; 3], [2.0; 3], [-0.1, 0.0, 0.1]];
    let brk = [0.1, 0.2, 0.3];
    vec![
        vec![T::Log(2.0, F)],
        vec![T::Log(10.0, I)],
        vec![T::Log(std::f64::consts::E, F)],
        vec![T::Affine(10.0, up, F)],
        vec![T::Affine(10.0, unequal, I)],
        vec![T::Camera(2.0, up, brk, None, F)],
        vec![T::Camera(2.0, up, brk, Some([1.5, 1.25, 1.0]), I)],
        vec![T::Matrix, T::Log(2.0, F)],
        vec![T::Range, T::Log(10.0, F), T::Matrix],
        vec![T::Log(2.0, F), T::CrossMatrix, T::Log(2.0, I)],
        vec![T::CrossMatrix, T::Log(2.0, F)],
        vec![T::Range, T::Matrix],
        // `optimizeForBitdepth` removes an identity clamp at an integer input before the bake,
        // not at half input, and one at an integer output.
        vec![T::Range01, T::Log(2.0, F)],
        vec![T::Log(2.0, F), T::Range01],
        // Gamma ops (an ExponentTransform in a version 2 config, and an
        // ExponentWithLinearTransform), CDL ops, and Gamma or CDL ops with others; a CDL with a
        // saturation mixes channels and ends the prefix.
        vec![T::Gamma([2.2, 2.4, 1.8, 1.0], NegativeStyle::Clamp, F)],
        vec![T::Gamma([2.6, 2.6, 2.6, 1.0], NegativeStyle::Mirror, I)],
        vec![T::Gamma([2.2, 2.2, 2.2, 1.5], NegativeStyle::PassThru, F)],
        vec![T::Moncurve(
            [2.4, 2.2, 2.0, 1.0],
            [0.055, 0.09, 0.1, 0.0],
            NegativeStyle::Linear,
            F,
        )],
        vec![T::Moncurve(
            [2.4, 2.4, 2.4, 1.0],
            [0.055, 0.055, 0.055, 0.0],
            NegativeStyle::Mirror,
            I,
        )],
        vec![T::Cdl(cdl(1.0, CdlStyle::Asc), F)],
        vec![T::Cdl(cdl(1.0, CdlStyle::NoClamp), I)],
        vec![T::Cdl(cdl(1.2, CdlStyle::Asc), F), T::Log(2.0, F)],
        vec![
            T::Matrix,
            T::Gamma([2.2, 2.4, 1.8, 1.0], NegativeStyle::Clamp, F),
            T::Cdl(cdl(1.0, CdlStyle::NoClamp), F),
            T::Log(10.0, F),
        ],
        vec![
            T::Gamma([2.2, 2.4, 1.8, 1.0], NegativeStyle::Mirror, F),
            T::Cdl(cdl(1.2, CdlStyle::Asc), F),
        ],
    ]
}

/// A `CDLTransform` with upstream's `multi_op_prefix` slope, offset and power
/// (tests/cpu/OpOptimizers_tests.cpp:1447-1449 @ v2.5.2), the saturation `sat` and `style`.
fn cdl(sat: f64, style: CdlStyle) -> Cdl {
    Cdl {
        slope: [1.35, 1.1, 0.071],
        offset: [0.05, -0.23, 0.11],
        power: [1.27, 0.81, 0.2],
        sat,
        style,
    }
}

/// A list of an Exponent op's version 1 config: `ExponentTransform`s and scales.
#[derive(Debug, Clone, Copy)]
enum V1 {
    /// An `ExponentTransform`: the exponents and the direction.
    Exponent([f64; 4], TransformDirection),
    /// A `MatrixTransform` scaling RGB.
    Scale(f64),
}

/// What a processor is built from.
#[derive(Debug, Clone)]
enum Source {
    /// A `GroupTransform` of these transforms, in the raw config (version 2).
    Chain(Vec<T>),
    /// A colour space of a version 1 config whose `from_reference` is a `GroupTransform` of
    /// these transforms: only a version 1 config builds Exponent ops (`BuildExponentOp`,
    /// src/OpenColorIO/ops/gamma/GammaOp.cpp:190-215 @ v2.5.2).
    V1(Vec<V1>),
}

impl Source {
    /// The oracle's arguments that name the processor.
    fn processor_args(&self) -> Value {
        match self {
            Source::Chain(chain) => {
                let children: Vec<Value> = chain.iter().map(common::log_chain::transform).collect();
                json!({"transform": {"class": "GroupTransform", "children": children}})
            }
            Source::V1(items) => {
                let children: Vec<String> = items
                    .iter()
                    .map(|item| match item {
                        V1::Exponent(v, dir) => format!(
                            "!<ExponentTransform> {{value: {}, direction: {}}}",
                            yaml_list(v),
                            match dir {
                                F => "forward",
                                I => "inverse",
                            }
                        ),
                        V1::Scale(s) => format!(
                            "!<MatrixTransform> {{matrix: {}}}",
                            yaml_list(&[
                                *s, 0., 0., 0., 0., *s, 0., 0., 0., 0., *s, 0., 0., 0., 0., 1.
                            ])
                        ),
                    })
                    .collect();
                let group = format!("!<GroupTransform> {{children: [{}]}}", children.join(", "));
                Spec::YamlV1(group).cpu_apply_args(&Combo {
                    direction: Direction::Forward,
                    fast_math: true,
                    format: Format::F32_RGBA,
                })
            }
        }
    }

    /// The oracle's arguments of the processor with `flags`, from `input` to `output`.
    fn args(&self, flags: &str, input: Depth, output: Depth) -> Value {
        let mut args = self.processor_args();
        args["optimization"] = json!(flags);
        args["in_bitdepth"] = json!(depth_name(port_depth(input)));
        args["out_bitdepth"] = json!(depth_name(port_depth(output)));
        args
    }

    /// The processor's ops, as it builds and finalizes them.
    fn raw_ops(&self) -> Result<OpVec> {
        match self {
            Source::Chain(chain) => port_raw_ops(chain),
            Source::V1(items) => {
                let mut raw = OpVec::new();
                for item in items {
                    match item {
                        // BuildExponentOp: the transform's direction (GammaOp.cpp:190-215).
                        V1::Exponent(v, dir) => {
                            create_exponent_op(&mut raw, ExponentOpData::from_values(v), *dir)?;
                        }
                        // BuildMatrixOp: the transform's data, validated (MatrixOp.cpp:395-404).
                        V1::Scale(s) => {
                            let mut data = MatrixOpData::create_diagonal_matrix(*s);
                            data.set_array_value(15, 1.0);
                            data.validate()?;
                            create_matrix_op(&mut raw, data, F);
                        }
                    }
                }
                raw.finalize()?;
                Ok(raw)
            }
        }
    }
}

/// The port's optimized ops: `getOptimizedProcessor`'s steps on the processor's ops.
fn port_optimized(
    source: &Source,
    flags: OptimizationFlags,
    input: Depth,
    output: Depth,
) -> Result<OpVec> {
    let mut ops = source.raw_ops()?;
    ops.finalize()?;
    ops.optimize(flags)?;
    ops.optimize_for_bitdepth(port_depth(input), port_depth(output), flags)?;
    Ok(ops)
}

/// A case of the optimized processor: what it's built from, the input and output bit depths,
/// and the flags.
type BakeCase = (Source, Depth, Depth, &'static str, OptimizationFlags);

/// `sources` at every input bit depth, to F32 and 16-bit output, at the levels that bake.
fn bake_cases(sources: &[Source]) -> Vec<BakeCase> {
    let mut cases = Vec::new();
    for source in sources {
        for input in INPUTS {
            for output in [Depth::F32, Depth::Uint16] {
                for (name, flags) in FLAGS {
                    cases.push((source.clone(), input, output, name, flags));
                }
            }
        }
    }
    cases
}

#[test]
fn the_baked_luts_match_the_wheel() {
    let sources: Vec<Source> = chains().into_iter().map(Source::Chain).collect();
    let cases = bake_cases(&sources);
    let baked = check_bakes(&cases);
    // Most cases bake; some don't.
    assert!(baked > cases.len() / 2 && baked < cases.len(), "{baked}");
}

/// Lists of Exponent ops (`ExponentOp`, a version 1 config's), alone, in pairs, and with a
/// scale: the bake renders them with the Exponent renderer and the math library.
fn exponent_lists() -> Vec<Source> {
    let a = [1.037289, 1.019015, 0.966082, 1.0];
    let b = [2.0, 2.1, 3.0, 3.1];
    vec![
        vec![V1::Exponent(b, F)],
        vec![V1::Exponent(b, I)],
        vec![V1::Exponent(a, F), V1::Exponent(b, F)],
        vec![V1::Scale(2.0), V1::Exponent(b, F)],
        vec![V1::Exponent(a, F), V1::Scale(0.5), V1::Exponent(b, I)],
    ]
    .into_iter()
    .map(Source::V1)
    .collect()
}

#[test]
fn exponent_bakes_match_the_wheel() {
    let cases = bake_cases(&exponent_lists());
    let baked = check_bakes(&cases);
    assert!(baked > 0, "{baked}");
}

#[test]
fn exponent_bakes_render_every_code_as_the_wheel() {
    every_code_of(&exponent_lists());
}

/// The optimized processors of `cases` against the wheel: their cache IDs, and the baked LUT
/// entry for entry. Returns how many baked.
fn check_bakes(cases: &[BakeCase]) -> usize {
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(source, input, output, name, _)| {
            let mut args = source.processor_args();
            args["in_bitdepth"] = json!(depth_name(port_depth(*input)));
            args["out_bitdepth"] = json!(depth_name(port_depth(*output)));
            args["optimization"] = json!(name);
            BatchCall {
                cmd: "processor_ops",
                args,
                blobs: vec![],
            }
        })
        .collect();
    let responses = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    let mut baked = 0;
    for ((source, input, output, name, flags), response) in cases.iter().zip(responses) {
        let what = format!("{source:?} {input:?}->{output:?} {name}");
        let response = response.unwrap_or_else(|e| panic!("{what}: {e}"));
        let result = &response.result;
        assert!(result.get("exception").is_none(), "{what}: {result}");
        let optimized = &result["optimized"];
        let ops = match port_optimized(source, *flags, *input, *output) {
            Ok(ops) => ops,
            Err(e) => {
                failures.push(format!("{what}: the port raised {}", e.message()));
                continue;
            }
        };

        // The cache ID: the hash of the ops' cache IDs.
        let port_id = if ops.is_empty() {
            "<NOOP>".to_string()
        } else {
            cache_id_hash(&ops.get_cache_id().unwrap())
        };
        if port_id != optimized["cache_id"].as_str().unwrap() {
            failures.push(format!("{what}: the cache IDs differ"));
        }

        // The baked LUT, entry for entry.
        let first = &optimized["group"]["children"][0];
        let wheel_lut = first["class"] == "Lut1DTransform";
        let port_lut = ops.first().and_then(|op| match &**op.data() {
            OpData::Lut1D(lut) => Some(lut.clone()),
            _ => None,
        });
        match (wheel_lut, port_lut) {
            (false, None) => {}
            (true, Some(lut)) => {
                baked += 1;
                let getters = &first["getters"];
                let blob = &response.blobs[getters["getData"]["blob"].as_u64().unwrap() as usize];
                let wheel: Vec<u32> = blob
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| u32::from_ne_bytes(*b))
                    .collect();
                let port: Vec<u32> = lut
                    .get_array()
                    .get_values()
                    .iter()
                    .map(|v| v.to_bits())
                    .collect();
                if getters["getInputHalfDomain"] != lut.is_input_half_domain()
                    || getters["getLength"] != lut.get_array().get_length()
                {
                    failures.push(format!("{what}: the LUTs' domains differ"));
                } else if port != wheel {
                    let k = port
                        .iter()
                        .zip(&wheel)
                        .position(|(p, w)| p != w)
                        .unwrap_or(0);
                    failures.push(format!(
                        "{what}: {} of {} values differ, first at {k}: wheel {:#010x} port \
                         {:#010x}",
                        port.iter().zip(&wheel).filter(|(p, w)| p != w).count(),
                        wheel.len(),
                        wheel.get(k).copied().unwrap_or(0),
                        port.get(k).copied().unwrap_or(0)
                    ));
                }
            }
            (wheel, port) => failures.push(format!(
                "{what}: the wheel baked {wheel}, the port {}",
                port.is_some()
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
    baked
}

/// `FindSeparablePrefix` leaves a prefix that is a single forward Lut1D as it is ("nothing to
/// optimize", src/OpenColorIO/OpOptimizers.cpp:493-507 @ v2.5.2): a 256-entry Lut1DTransform
/// at 8-bit input. (A prefix with a LUT and other ops renders the LUT on floats to bake it,
/// which waits for the float renderers, Phase 2.)
#[test]
fn a_single_lut_isnt_baked_again() {
    let mut cases = Vec::new();
    for output in [Depth::F32, Depth::Uint16] {
        for (name, flags) in FLAGS {
            cases.push((
                Source::Chain(vec![T::Lut8]),
                Depth::Uint8,
                output,
                name,
                flags,
            ));
        }
    }
    check_bakes(&cases);
}

/// Every code of `depth` on each channel: red `i`, green the codes backwards, blue a
/// permutation, alpha `i`, in rows of 256 pixels.
fn ramp(depth: Depth) -> (Vec<u8>, i64) {
    let n = Lut1DOpData::get_lut_ideal_size(port_depth(depth)).unwrap() as usize;
    let mut bytes = Vec::new();
    for i in 0..n {
        for code in [i, n - 1 - i, (i * 97) % n, i] {
            if depth == Depth::Uint8 {
                bytes.push(code as u8);
            } else {
                bytes.extend_from_slice(&(code as u16).to_ne_bytes());
            }
        }
    }
    (bytes, (n / 256) as i64)
}

fn packed(buffer: usize, height: i64, depth: Depth) -> Packed {
    Packed::new(Data::at(buffer, 0), 256, height, Channels::Count(4))
        .layout(depth, [Stride::Auto; 3])
}

/// The CPU processors' pixels, for every code, with the default flags, to F32 and to 16-bit
/// output.
#[test]
fn every_code_matches_the_wheel() {
    let sources: Vec<Source> = chains().into_iter().map(Source::Chain).collect();
    every_code_of(&sources);
}

/// The CPU processors of `sources`, with the default flags, from every code of each input bit
/// depth, to F32 and to 16-bit output, against the wheel.
fn every_code_of(sources: &[Source]) {
    let mut cases = Vec::new();
    for source in sources {
        for input in INPUTS {
            for output in [Depth::F32, Depth::Uint16] {
                cases.push((source.clone(), input, output));
            }
        }
    }
    let requests: Vec<Request> = cases
        .iter()
        .map(|(source, input, output)| {
            let (bytes, height) = ramp(*input);
            let out_size = if *output == Depth::F32 { 4 } else { 2 };
            let mut request = Request::new(source.args(FLAGS[0].0, *input, *output));
            let src = request.buffer(Buffer::Bytes(bytes));
            let dst = request.buffer(Buffer::Bytes(vec![0; 4 * 256 * height as usize * out_size]));
            request.image(packed(src, height, *input));
            request.image(packed(dst, height, *output));
            request.apply = vec![0, 1];
            request
        })
        .collect();
    let calls: Vec<BatchCall<'_>> = requests.iter().map(Request::call).collect();
    let responses = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for (((source, input, output), request), response) in cases.iter().zip(&requests).zip(responses)
    {
        let what = format!("{source:?} {input:?}->{output:?}");
        let reply = request.reply(response.unwrap_or_else(|e| panic!("{what}: {e}")));
        assert!(reply.raised().is_none(), "{what}: {:?}", reply.raised());
        let mut buffers: Vec<Vec<u8>> = request.buffers.iter().map(Buffer::bytes).collect();
        let port = (|| -> Result<()> {
            let cpu = CpuProcessor::new(
                &source.raw_ops()?,
                port_depth(*input),
                port_depth(*output),
                FLAGS[0].1,
            )?;
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
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}
