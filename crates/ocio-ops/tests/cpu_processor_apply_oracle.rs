// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CPU processor's image `apply` methods against the wheel's `CPUProcessor.apply`, through
//! the oracle's `image_apply`: from one image to another (`apply(src, dst)`), in place
//! (`apply(img)`), and from an image to itself (`apply(img, img)`, the port's `apply_same`),
//! for every pair of the bit depths of the pixel tests, RGBA-packed images and others, and
//! lists of Matrix transforms at two optimization levels. Every buffer must equal the wheel's,
//! byte for byte, or both must raise the same message.
//!
//! The port builds the processor's ops as the wheel does (see `cpu_processor_oracle.rs`): a
//! `GroupTransform` builds each child forward, `BuildMatrixOp` clones the transform's data
//! (src/OpenColorIO/ops/matrix/MatrixOp.cpp:395-404), and the processor finalizes them
//! (src/OpenColorIO/Processor.cpp:618-641 @ v2.5.2). The optimizer never bakes Matrix ops into
//! a Lut1D (`FindSeparablePrefix`, src/OpenColorIO/OpOptimizers.cpp:417-469), so every case
//! renders with the ported ops.

mod common;

use common::image::{
    GENERIC_SHAPES, PACKED_SHAPES, PAIRS, Shape, add_image, buffer_indices, depth_name, port_depth,
    port_image,
};
use ocio_ops::Result;
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::image_desc::Bytes;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::image::{Buffer, Image, Request};
use serde_json::{Value, json};

/// A matrix and its offsets.
type Matrix = ([f64; 16], [f64; 4]);

/// A list of Matrix transforms.
type Chain = Vec<(Matrix, TransformDirection)>;

/// A diagonal matrix.
fn diagonal(d: [f64; 4]) -> [f64; 16] {
    let mut m = [0.0; 16];
    for (i, v) in d.into_iter().enumerate() {
        m[i * 5] = v;
    }
    m
}

/// Lists of transforms: one that leaves no op (so the processor renders an identity matrix),
/// one with channel crosstalk, its inverse, and two that the optimizer combines unless it is
/// off (tests/cpu/ops/matrix/MatrixOp_tests.cpp:367-377 @ v2.5.2 for the matrix).
fn chains() -> Vec<Chain> {
    use TransformDirection::{Forward, Inverse};
    let upstream: Matrix = (
        [
            1.1, 0.2, 0.3, 0.4, 0.5, 1.6, 0.7, 0.8, 0.2, 0.1, 1.1, 0.2, 0.3, 0.4, 0.5, 1.6,
        ],
        [-0.5, -0.25, 0.25, 0.0],
    );
    let scale: Matrix = (diagonal([2.0, 0.5, 4.0, 1.0]), [0.0; 4]);
    let offset: Matrix = (diagonal([1.0; 4]), [0.1, -0.2, 0.3, 0.0]);
    vec![
        vec![(offset, Forward), (offset, Inverse)],
        vec![(upstream, Forward)],
        vec![(upstream, Inverse)],
        vec![(scale, Forward), (offset, Forward), (upstream, Forward)],
    ]
}

/// Optimization levels: none, and the default.
const FLAGS: [(&str, OptimizationFlags); 2] = [
    ("OPTIMIZATION_NONE", OptimizationFlags::NONE),
    ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
];

/// The spec of a `MatrixTransform`.
fn transform((m, o): &Matrix, dir: TransformDirection) -> Value {
    let dir = match dir {
        TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
        TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
    };
    json!({
        "class": "MatrixTransform",
        "args": {"matrix": m.to_vec(), "offset": o.to_vec(), "direction": {"enum": dir}},
    })
}

/// The processor of `chain`, as `image_apply` takes it.
fn processor(chain: &Chain, flags: &str, input: BitDepth, output: BitDepth) -> Value {
    let children: Vec<Value> = chain.iter().map(|(m, dir)| transform(m, *dir)).collect();
    json!({
        "transform": {"class": "GroupTransform", "children": children},
        "optimization": flags,
        "in_bitdepth": depth_name(input),
        "out_bitdepth": depth_name(output),
    })
}

/// The port's CPU processor of `chain`.
fn port_processor(
    chain: &Chain,
    flags: OptimizationFlags,
    input: BitDepth,
    output: BitDepth,
) -> Result<CpuProcessor> {
    let mut raw = OpVec::new();
    for ((m, o), dir) in chain {
        let mut data = MatrixOpData::new();
        data.set_rgba(m);
        data.set_rgba_offsets(o);
        data.set_direction(*dir);
        data.validate()?;
        create_matrix_op(&mut raw, data, TransformDirection::Forward);
    }
    raw.finalize()?;
    CpuProcessor::new(&raw, input, output, flags)
}

/// The images of an apply.
#[derive(Debug, Clone)]
enum Apply {
    /// `apply(src, dst)`.
    SrcDst(Image, Image),
    /// `apply(img)`, in place.
    InPlace(Image),
    /// `apply(img, img)`.
    Same(Image),
}

/// The port applies `cpu` to the images of `apply` in `buffers`.
fn port_apply(cpu: &CpuProcessor, apply: &Apply, buffers: &mut [Vec<u8>]) -> Result<()> {
    match apply {
        Apply::SrcDst(src, dst) => {
            // The destination's buffers come after the source's.
            let first_dst = buffer_indices(dst).into_iter().min().expect("a buffer");
            let (src_buffers, dst_buffers) = buffers.split_at_mut(first_dst);
            let src = port_image(src, |b| Bytes(&src_buffers[b][..]))?;
            let mut slots: Vec<Option<&mut [u8]>> =
                dst_buffers.iter_mut().map(|b| Some(&mut b[..])).collect();
            let mut dst = port_image(dst, |b| Bytes(slots[b - first_dst].take().expect("once")))?;
            cpu.apply_src_dst(src.desc(), dst.desc_mut())
        }
        Apply::InPlace(img) | Apply::Same(img) => {
            let mut slots: Vec<Option<&mut [u8]>> =
                buffers.iter_mut().map(|b| Some(&mut b[..])).collect();
            let mut img = port_image(img, |b| Bytes(slots[b].take().expect("once")))?;
            match apply {
                Apply::InPlace(_) => cpu.apply(img.desc_mut()),
                _ => cpu.apply_same(img.desc_mut()),
            }
        }
    }
}

/// A case: the chain, the flags, the bit depths, the images, and the request.
struct Case {
    chain: usize,
    flags: usize,
    depths: (Depth, Depth),
    apply: Apply,
    request: Request,
}

/// Adds a case of `apply` (`f` adds its images to the request).
fn add_case(
    cases: &mut Vec<Case>,
    (chain, flags): (usize, usize),
    (input, output): (Depth, Depth),
    f: impl FnOnce(&mut Request) -> (Apply, Vec<usize>),
) {
    let chains = chains();
    let (flags_name, _) = FLAGS[flags];
    let mut request = Request::new(processor(
        &chains[chain],
        flags_name,
        port_depth(input),
        port_depth(output),
    ));
    let (apply, indices) = f(&mut request);
    request.apply = indices;
    cases.push(Case {
        chain,
        flags,
        depths: (input, output),
        apply,
        request,
    });
}

/// Runs `cases` on the wheel and the port: each gives the same buffers, or the same error.
fn check(cases: &[Case]) {
    let calls: Vec<_> = cases.iter().map(|case| case.request.call()).collect();
    let responses = Oracle::get().batch(&calls, true);
    let chains = chains();

    let mut failures = Vec::new();
    for (case, response) in cases.iter().zip(responses) {
        let (input, output) = case.depths;
        let what = format!(
            "chain {} {} {input:?} to {output:?}: {:?}",
            case.chain, FLAGS[case.flags].0, case.apply
        );
        let reply = case
            .request
            .reply(response.unwrap_or_else(|e| panic!("{what}: {e}")));

        let mut buffers: Vec<Vec<u8>> = case.request.buffers.iter().map(Buffer::bytes).collect();
        let port = port_processor(
            &chains[case.chain],
            FLAGS[case.flags].1,
            port_depth(input),
            port_depth(output),
        )
        .and_then(|cpu| port_apply(&cpu, &case.apply, &mut buffers));
        match (reply.raised(), port) {
            (None, Ok(())) => {
                for (k, (port, wheel)) in buffers.iter().zip(&reply.buffers).enumerate() {
                    if port != wheel {
                        let first = port.iter().zip(wheel).position(|(a, b)| a != b);
                        failures.push(format!("{what}\n  buffer {k} differs at byte {first:?}"));
                    }
                }
            }
            (Some(raised), Err(e)) if raised.message == e.message() => {}
            (raised, port) => failures.push(format!(
                "{what}\n  wheel {:?}\n  port  {:?}",
                raised.map(|r| (r.stage, r.message)),
                port.map_err(|e| e.message().to_string())
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} applies differ:\n{}",
        failures.len(),
        cases.len(),
        failures[..failures.len().min(10)].join("\n")
    );
}

/// The layouts: the RGBA-packed ones, and some the scanline helper packs channel by channel.
fn shapes() -> Vec<Shape> {
    let mut shapes = PACKED_SHAPES.to_vec();
    shapes.extend([GENERIC_SHAPES[1], GENERIC_SHAPES[9], GENERIC_SHAPES[0]]);
    shapes
}

/// From one image to another: each layout to itself and to the next one, at every pair of bit
/// depths, with each list of transforms and both optimization levels in turn.
#[test]
fn apply_src_dst_matches_the_wheel() {
    let shapes = shapes();
    let mut cases = Vec::new();
    for (pair, depths) in PAIRS.into_iter().enumerate() {
        for size in [(1, 1), (17, 3)] {
            for (k, &shape) in shapes.iter().enumerate() {
                for (d, dst_shape) in [shape, shapes[(k + 1) % shapes.len()]]
                    .into_iter()
                    .enumerate()
                {
                    let n = pair + k + d;
                    let seed = Some((pair * 1000 + k) as u64);
                    add_case(&mut cases, (n % 4, n / 4 % 2), depths, |request| {
                        let src = add_image(request, shape, depths.0, size, seed);
                        let dst = add_image(request, dst_shape, depths.1, size, None);
                        (Apply::SrcDst(src, dst), vec![0, 1])
                    });
                }
            }
        }
    }
    check(&cases);
}

/// In place, and from an image to itself: every layout, at every bit depth, as input and
/// output. From an image to itself, upstream reads each row through the source's description
/// and writes it through the destination's, the same image (`GenericScanlineHelper::init(src,
/// dst)`, src/OpenColorIO/ScanlineHelper.cpp:50-82 @ v2.5.2).
#[test]
fn apply_in_place_and_to_itself_match_the_wheel() {
    let mut cases = Vec::new();
    for (pair, depths) in PAIRS.into_iter().enumerate() {
        if depths.0 != depths.1 {
            continue;
        }
        for size in [(1, 1), (3, 2), (17, 3)] {
            for (k, &shape) in shapes().iter().enumerate() {
                for same in [false, true] {
                    let n = pair + k + usize::from(same);
                    let seed = Some((pair * 1000 + k) as u64);
                    add_case(&mut cases, (n % 4, n / 4 % 2), depths, |request| {
                        let img = add_image(request, shape, depths.0, size, seed);
                        if same {
                            (Apply::Same(img), vec![0, 0])
                        } else {
                            (Apply::InPlace(img), vec![0])
                        }
                    });
                }
            }
        }
    }
    check(&cases);
}

/// The errors, with the wheel's messages: in place and from an image to itself, with an image
/// whose bit depth is the input's but not the output's; from one image to another, with images
/// of different sizes, and a source whose bit depth isn't the input's.
#[test]
fn apply_errors_match_the_wheel() {
    let rgba = PACKED_SHAPES[0];
    let planar = GENERIC_SHAPES[9];
    let mut cases = Vec::new();
    for shape in [rgba, planar] {
        let depths = (Depth::Uint8, Depth::F32);
        for same in [false, true] {
            add_case(&mut cases, (1, 1), depths, |request| {
                let img = add_image(request, shape, Depth::Uint8, (2, 2), Some(8));
                if same {
                    (Apply::Same(img), vec![0, 0])
                } else {
                    (Apply::InPlace(img), vec![0])
                }
            });
        }
        let depths = (Depth::F32, Depth::F16);
        for (src_size, dst_size) in [((3, 2), (2, 3)), ((3, 2), (3, 1))] {
            add_case(&mut cases, (3, 0), depths, |request| {
                let src = add_image(request, shape, Depth::F32, src_size, Some(9));
                let dst = add_image(request, shape, Depth::F16, dst_size, None);
                (Apply::SrcDst(src, dst), vec![0, 1])
            });
        }
        add_case(&mut cases, (2, 1), depths, |request| {
            let src = add_image(request, shape, Depth::F16, (2, 2), Some(10));
            let dst = add_image(request, shape, Depth::F16, (2, 2), None);
            (Apply::SrcDst(src, dst), vec![0, 1])
        });
    }
    check(&cases);
}
