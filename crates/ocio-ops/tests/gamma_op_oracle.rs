// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Lists of Gamma ops against the wheel's CPU processors: the cache ID, which names the ops
//! the optimizer leaves with their data's cache ID (`GammaOpData::getCacheID`, 7 significant
//! digits), and the pixels, which show the composed parameters bit for bit. Through them: the
//! optimizer's Gamma steps (src/OpenColorIO/OpOptimizers.cpp @ v2.5.2): no-ops removed,
//! identities replaced with a Range or a Matrix (`GammaOpData::getIdentityReplacement`, under
//! `OPTIMIZATION_IDENTITY_GAMMA`), inverse pairs removed (`isInverse`, under
//! `OPTIMIZATION_PAIR_IDENTITY_GAMMA`), and neighbours combined (`mayCompose`, `compose`, under
//! `OPTIMIZATION_COMP_GAMMA`).
//!
//! The wheel builds a `GroupTransform` of `ExponentTransform`s and
//! `ExponentWithLinearTransform`s in a raw config, a version 2 config: each child builds a Gamma
//! op with a copy of its data (`BuildExponentOp`, `BuildExponentWithLinearOp`,
//! src/OpenColorIO/ops/gamma/GammaOp.cpp:179-216), forward, and the processor finalizes them
//! (src/OpenColorIO/Processor.cpp:618-641). The port builds the same data (`common::gamma`),
//! the ops with `create_gamma_op`, and the same processor. Finite parameters go through JSON
//! (the bindings' constructors validate each child, with its transform's prefix), NaN and
//! infinite ones through a config's YAML (no prefix: `BuildExponentOp` validates).
//!
//! The input is F32: for integer and half input, the optimizer bakes the separable ops into a
//! Lut1D, which comes with WP 2.5.

mod common;

use common::gamma::{
    direction_enum, exponent_op, exponent_with_linear_op, negative_style_enum, yaml_direction,
    yaml_style,
};
use common::image::{add_image, depth_name, port_depth, port_image};
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::exception::Result;
use ocio_ops::image_desc::Bytes;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    NegativeStyle, OptimizationFlags, TransformDirection, get_inverse_transform_direction,
};
use ocio_ops::ops::gamma::gamma_op::create_gamma_op;
use ocio_ops::ops::gamma::gamma_op_data::GammaOpData;
use ocio_testkit::Oracle;
use ocio_testkit::battery::{BitDepth as Depth, yaml_list};
use ocio_testkit::image::{Buffer, Request};
use ocio_testkit::oracle::BatchCall;
use serde_json::{Value, json};

use NegativeStyle::{Clamp, Linear, Mirror, PassThru};
use TransformDirection::{Forward as F, Inverse as I};

/// A transform of the lists.
#[derive(Debug, Clone, Copy)]
enum T {
    /// `ExponentTransform(value, negativeStyle, direction)`.
    Exp([f64; 4], NegativeStyle, TransformDirection),
    /// `ExponentWithLinearTransform(gamma, offset, negativeStyle, direction)`.
    Lin([f64; 4], [f64; 4], NegativeStyle, TransformDirection),
}

impl T {
    /// Whether every parameter is finite: then JSON can hold them.
    fn finite(&self) -> bool {
        match self {
            T::Exp(v, ..) => v.iter().all(|x| x.is_finite()),
            T::Lin(g, o, ..) => g.iter().chain(o).all(|x| x.is_finite()),
        }
    }

    /// The JSON spec.
    fn spec(&self) -> Value {
        match *self {
            T::Exp(value, neg, dir) => json!({"class": "ExponentTransform", "args": {
                "value": value, "negativeStyle": negative_style_enum(neg),
                "direction": direction_enum(dir)}}),
            T::Lin(gamma, offset, neg, dir) => {
                json!({"class": "ExponentWithLinearTransform", "args": {
                "gamma": gamma, "offset": offset, "negativeStyle": negative_style_enum(neg),
                "direction": direction_enum(dir)}})
            }
        }
    }

    /// The config YAML.
    fn yaml(&self) -> String {
        match *self {
            T::Exp(value, neg, dir) => format!(
                "!<ExponentTransform> {{value: {}, style: {}, direction: {}}}",
                yaml_list(&value),
                yaml_style(neg),
                yaml_direction(dir)
            ),
            T::Lin(gamma, offset, neg, dir) => format!(
                "!<ExponentWithLinearTransform> {{gamma: {}, offset: {}, style: {}, \
                 direction: {}}}",
                yaml_list(&gamma),
                yaml_list(&offset),
                yaml_style(neg),
                yaml_direction(dir)
            ),
        }
    }

    /// The op data, and the transform's validation prefix.
    fn data(&self) -> (GammaOpData, &'static str) {
        match *self {
            T::Exp(value, neg, dir) => (
                exponent_op(value, neg, dir),
                "ExponentTransform validation failed: ",
            ),
            T::Lin(gamma, offset, neg, dir) => (
                exponent_with_linear_op(gamma, offset, neg, dir),
                "ExponentWithLinearTransform validation failed: ",
            ),
        }
    }
}

/// Lists of transforms: every style alone, at 7 significant digits; identities of each style
/// (no-ops, a Range, a Matrix); inverse pairs; compositions whose product rounds to 1 or falls
/// below 1, of every combinable pair of styles, and pairs that don't combine; NaN and infinite
/// parameters; and refusals.
fn chains() -> Vec<Vec<T>> {
    let v = [2.2, 1.0 / 0.45, 1.23456789, 1.0];
    let lin = ([2.4, 1.0 / 0.45, 3.0, 1.0], [0.055, 0.099, 0.16, 0.0]);
    let one = [1.0; 4];
    let lin_one = ([1.0; 4], [0.0; 4]);
    let mut chains = Vec::new();
    for neg in [Clamp, Mirror, PassThru] {
        for dir in [F, I] {
            chains.push(vec![T::Exp(v, neg, dir)]);
            chains.push(vec![T::Exp(one, neg, dir)]);
            chains.push(vec![
                T::Exp(v, neg, dir),
                T::Exp(v, neg, get_inverse_transform_direction(dir)),
            ]);
        }
    }
    for neg in [Linear, Mirror] {
        for dir in [F, I] {
            chains.push(vec![T::Lin(lin.0, lin.1, neg, dir)]);
            chains.push(vec![T::Lin(lin_one.0, lin_one.1, neg, dir)]);
            chains.push(vec![
                T::Lin(lin.0, lin.1, neg, dir),
                T::Lin(lin.0, lin.1, neg, get_inverse_transform_direction(dir)),
            ]);
            // Moncurves never combine, with a basic style either.
            chains.push(vec![
                T::Lin(lin.0, lin.1, neg, dir),
                T::Lin(lin.0, lin.1, neg, dir),
            ]);
            chains.push(vec![T::Exp(v, Clamp, dir), T::Lin(lin.0, lin.1, neg, dir)]);
        }
    }
    // Every pair of basic styles and directions: combined where `mayCompose` allows it.
    let a = [1.0 / 0.45, 2.0, 0.5, 1.25];
    let b = [0.45, 3.0, 0.8, 1.0];
    for neg1 in [Clamp, Mirror, PassThru] {
        for neg2 in [Clamp, Mirror, PassThru] {
            for (d1, d2) in [(F, F), (F, I), (I, F), (I, I)] {
                chains.push(vec![T::Exp(a, neg1, d1), T::Exp(b, neg2, d2)]);
            }
        }
    }
    chains.extend([
        // Products within 1e-6 of 1 (RoundAround1), and all below 1.
        vec![
            T::Exp([0.5, 0.25, 0.8, 1.0], Clamp, F),
            T::Exp([0.4, 0.5, 0.5, 0.3], Clamp, F),
        ],
        vec![
            T::Exp([2.0, 2.0, 2.0, 1.0], Clamp, F),
            T::Exp([0.5000004, 0.4999996, 0.5000006, 1.0], Clamp, F),
        ],
        vec![
            T::Exp([0.5, 0.5, 1.5, 1.0], Mirror, F),
            T::Exp([0.5, 0.5, 0.5, 1.0], Mirror, F),
        ],
        // Red and green below 1 but blue above: no inversion (all three must be below 1).
        vec![
            T::Exp([0.5, 0.5, 2.0, 1.0], Clamp, F),
            T::Exp([0.5, 0.5, 1.0, 1.0], Clamp, F),
        ],
        // A forward/inverse pair that differs only in alpha: not an inverse pair.
        vec![
            T::Exp([2.2, 2.2, 2.2, 1.0], Clamp, F),
            T::Exp([2.2, 2.2, 2.2, 1.5], Clamp, I),
        ],
        // A moncurve gamma of 1 with an offset: not an identity (`IsMonCurveIdentity` reads
        // the offset after the gamma).
        vec![T::Lin([1.0; 4], [0.1; 4], Linear, F)],
        vec![T::Lin([1.0; 4], [0.1; 4], Mirror, I)],
        // Three in a row, then an identity left behind.
        vec![
            T::Exp(v, Clamp, F),
            T::Exp(v, Clamp, I),
            T::Exp(v, PassThru, F),
        ],
        vec![
            T::Exp([2.0; 4], PassThru, F),
            T::Exp([3.0; 4], PassThru, I),
            T::Exp([1.5; 4], PassThru, F),
        ],
        // NaN and infinite parameters (YAML).
        vec![T::Exp([2.2, f64::NAN, 1.8, 1.0], Clamp, F)],
        vec![
            T::Exp([2.2, f64::NAN, 1.8, 1.0], Mirror, F),
            T::Exp([2.0; 4], Mirror, I),
        ],
        vec![T::Lin(
            [2.4, f64::NAN, 2.2, 1.0],
            [0.055, 0.1, f64::NAN, 0.0],
            Linear,
            F,
        )],
        vec![T::Exp([f64::INFINITY, 2.2, 1.8, 1.0], Clamp, F)],
        vec![T::Lin(
            [2.4, 2.2, 2.2, 1.0],
            [0.055, f64::NEG_INFINITY, 0.1, 0.0],
            Mirror,
            I,
        )],
        // Refused.
        vec![
            T::Exp(v, Clamp, F),
            T::Exp([0.001, 1.0, 1.0, 1.0], Clamp, F),
        ],
        vec![T::Lin(
            [2.4, 2.2, 2.2, 1.0],
            [0.055, 0.95, 0.1, 0.0],
            Linear,
            F,
        )],
    ]);
    chains
}

/// The optimization levels, and the Gamma flags alone.
fn flags() -> Vec<(Value, OptimizationFlags)> {
    let mut out: Vec<(Value, OptimizationFlags)> = [
        ("OPTIMIZATION_NONE", OptimizationFlags::NONE),
        ("OPTIMIZATION_LOSSLESS", OptimizationFlags::LOSSLESS),
        ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
        ("OPTIMIZATION_GOOD", OptimizationFlags::GOOD),
        ("OPTIMIZATION_ALL", OptimizationFlags::ALL),
        (
            "OPTIMIZATION_IDENTITY_GAMMA",
            OptimizationFlags::IDENTITY_GAMMA,
        ),
        (
            "OPTIMIZATION_PAIR_IDENTITY_GAMMA",
            OptimizationFlags::PAIR_IDENTITY_GAMMA,
        ),
        ("OPTIMIZATION_COMP_GAMMA", OptimizationFlags::COMP_GAMMA),
    ]
    .into_iter()
    .map(|(name, flags)| (json!(name), flags))
    .collect();
    out.push((
        json!(["OPTIMIZATION_IDENTITY_GAMMA", "OPTIMIZATION_COMP_GAMMA"]),
        OptimizationFlags::IDENTITY_GAMMA | OptimizationFlags::COMP_GAMMA,
    ));
    out
}

/// The raw config with a color space `cs` whose `from_scene_reference` is `transform`, as the
/// battery's YAML specs build it.
fn yaml_config(transform: &str) -> String {
    format!(
        "ocio_profile_version: 2.1\nroles:\n  default: raw\nfile_rules:\n  - !<Rule> {{name: \
         Default, colorspace: raw}}\ndisplays:\n  sRGB:\n    - !<View> {{name: Raw, colorspace: \
         raw}}\ncolorspaces:\n  - !<ColorSpace>\n    name: raw\n  - !<ColorSpace>\n    name: \
         cs\n    from_scene_reference: {transform}\n"
    )
}

/// The processor of `chain`, as `cpu_apply` and `image_apply` take it.
fn processor(chain: &[T], flags: &Value, output: Depth) -> Value {
    let mut args = if chain.iter().all(T::finite) {
        let children: Vec<Value> = chain.iter().map(T::spec).collect();
        json!({"transform": {"class": "GroupTransform", "children": children}})
    } else {
        let children: Vec<String> = chain.iter().map(T::yaml).collect();
        let group = format!("!<GroupTransform> {{children: [{}]}}", children.join(", "));
        json!({"config": {"yaml": yaml_config(&group)}, "src": "raw", "dst": "cs"})
    };
    args["optimization"] = flags.clone();
    args["in_bitdepth"] = json!("BIT_DEPTH_F32");
    args["out_bitdepth"] = json!(depth_name(port_depth(output)));
    args
}

/// The port's CPU processor of `chain`, or the wheel's refusal.
fn port_processor(chain: &[T], flags: OptimizationFlags, output: Depth) -> Result<CpuProcessor> {
    let json = chain.iter().all(T::finite);
    let mut raw = OpVec::new();
    for t in chain {
        let (data, prefix) = t.data();
        if json {
            // The binding's constructor validates each child.
            data.validate().map_err(|e| {
                ocio_ops::exception::Exception::new(format!("{prefix}{}", e.message()))
            })?;
        }
    }
    for t in chain {
        let (data, _) = t.data();
        // BuildExponentOp, BuildExponentWithLinearOp.
        data.validate()?;
        create_gamma_op(&mut raw, data, F);
    }
    raw.finalize()?;
    CpuProcessor::new(&raw, port_depth(Depth::F32), port_depth(output), flags)
}

#[test]
fn the_cache_id_matches_the_wheel() {
    let flags = flags();
    let mut cases = Vec::new();
    for chain in chains() {
        for f in 0..flags.len() {
            for output in [Depth::F32, Depth::Uint16, Depth::F16] {
                cases.push((chain.clone(), f, output));
            }
        }
    }
    let pixel = vec![0u8; 16];
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(chain, f, output)| BatchCall {
            cmd: "cpu_apply",
            args: processor(chain, &flags[*f].0, *output),
            blobs: vec![&pixel],
        })
        .collect();
    let results = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    let mut refused = 0;
    for ((chain, f, output), result) in cases.iter().zip(results) {
        let result = result.unwrap_or_else(|e| panic!("{e}")).result;
        let wheel = match result.get("exception") {
            Some(exception) => Err(exception["message"].as_str().unwrap().to_string()),
            None => Ok(result["cpu_cache_id"].as_str().unwrap().to_string()),
        };
        refused += usize::from(wheel.is_err());
        let port = port_processor(chain, flags[*f].1, *output)
            .map(|cpu| String::from_utf8(cpu.get_cache_id().to_vec()).unwrap())
            .map_err(|e| e.message().to_string());
        if port != wheel {
            failures.push(format!(
                "{chain:?} {} F32->{output:?}\n  wheel {wheel:?}\n  port  {port:?}",
                flags[*f].0
            ));
        }
    }
    println!("{} cases, {refused} refused by the wheel", cases.len());
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

/// Whether the pixels of `chain` rendered with `flags` fall under waiver W0002, and so are left
/// out here: exactly the cases with a NaN parameter in a forward linear moncurve
/// (`ExponentWithLinearTransform`, `NEGATIVE_LINEAR`, forward) rendered without fast math
/// (`OPTIMIZATION_FAST_LOG_EXP_POW` off). There GCC multiplies `scale * pixel` and MSVC
/// `pixel * scale` (`GammaMoncurveOpCPUFwd`), so a NaN pixel meeting a NaN coefficient comes
/// out differently on Linux and Windows. Those cases are compared in the battery of
/// `tests/gamma_oracle.rs` under W0002, the only place W0002 may apply. The same chains with
/// fast math, and their cache IDs, are compared here bit for bit.
fn under_w0002(chain: &[T], flags: OptimizationFlags) -> bool {
    !flags.has_flag(OptimizationFlags::FAST_LOG_EXP_POW)
        && chain.iter().any(|t| {
            matches!(t, T::Lin(g, o, Linear, F)
            if g.iter().chain(o).any(|v| v.is_nan()))
        })
}

#[test]
fn the_pixels_match_the_wheel() {
    let flags = flags();
    let mut cases = Vec::new();
    let mut waived = 0;
    for chain in chains() {
        for (f, (_, flag)) in flags.iter().enumerate() {
            if under_w0002(&chain, *flag) {
                waived += 1;
            } else {
                cases.push((chain.clone(), f));
            }
        }
    }
    // One chain (a NaN gamma and a NaN offset, linear, forward) at the 6 settings without fast
    // math: NONE, LOSSLESS, and the Gamma flags alone and together.
    assert_eq!(waived, 6, "the W0002 cases left out");
    let shape = common::image::PACKED_SHAPES[0];
    let requests: Vec<Request> = cases
        .iter()
        .enumerate()
        .map(|(k, (chain, f))| {
            let mut request = Request::new(processor(chain, &flags[*f].0, Depth::F32));
            add_image(&mut request, shape, Depth::F32, (37, 2), Some(k as u64));
            add_image(&mut request, shape, Depth::F32, (37, 2), None);
            request.apply = vec![0, 1];
            request
        })
        .collect();
    let calls: Vec<BatchCall<'_>> = requests.iter().map(Request::call).collect();
    let responses = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for (((chain, f), request), response) in cases.iter().zip(&requests).zip(responses) {
        let what = format!("{chain:?} {}", flags[*f].0);
        let reply = request.reply(response.unwrap_or_else(|e| panic!("{what}: {e}")));
        let mut buffers: Vec<Vec<u8>> = request.buffers.iter().map(Buffer::bytes).collect();
        let port = (|| -> Result<()> {
            let cpu = port_processor(chain, flags[*f].1, Depth::F32)?;
            let (src_buffers, dst_buffers) = buffers.split_at_mut(1);
            let src = port_image(&request.images[0], |_| Bytes(&src_buffers[0][..]))?;
            let mut slot = Some(&mut dst_buffers[0][..]);
            let mut dst = port_image(&request.images[1], |_| {
                Bytes(slot.take().expect("one buffer"))
            })?;
            cpu.apply_src_dst(src.desc(), dst.desc_mut())
        })();
        match (reply.raised(), port) {
            (None, Ok(())) if buffers == reply.buffers => {}
            (None, Ok(())) => failures.push(format!("{what}: the pixels differ")),
            (Some(raised), Err(e)) if raised.message == e.message() => {}
            (raised, port) => failures.push(format!(
                "{what}\n  wheel {:?}\n  port  {:?}",
                raised.map(|r| r.message),
                port.map_err(|e| e.message().to_string())
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
}
