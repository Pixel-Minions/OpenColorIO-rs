// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Lists of CDL ops against the wheel's CPU processors: the cache ID, which names the ops the
//! optimizer leaves with their data's cache ID (`CDLOpData::getCacheID`, 7 significant digits),
//! and the pixels. Through them: the optimizer's CDL steps (src/OpenColorIO/OpOptimizers.cpp @
//! v2.5.2): no-ops removed, identities replaced with a [0, 1] Range or the identity matrix
//! (`CDLOpData::getIdentityReplacement`, under `OPTIMIZATION_IDENTITY`), inverse pairs replaced
//! (`isInverse`, with its 1e-9 tolerance, under `OPTIMIZATION_PAIR_IDENTITY_CDL`), and a CDL
//! whose power is 1 broken into matrices and clamps (`getSimplerReplacement`, under
//! `OPTIMIZATION_SIMPLIFY_OPS`), which the optimizer then combines.
//!
//! The wheel builds a `GroupTransform` of `CDLTransform`s in a raw config, a version 2 config:
//! each child builds a CDL op with a copy of its data (`BuildCDLOp`,
//! src/OpenColorIO/ops/cdl/CDLOp.cpp:200-265), forward, and the processor finalizes them
//! (src/OpenColorIO/Processor.cpp:618-641). The port builds the same data (`common::cdl`), the
//! ops with `create_cdl_op`, and the same processor. Finite parameters go through JSON (the
//! binding's constructor validates each child, with its prefix), NaN and infinite ones through
//! a config's YAML (no prefix: `BuildCDLOp` validates).
//!
//! The input is F32: for integer and half input, the optimizer bakes separable ops into a
//! Lut1D, which comes with WP 2.5.

mod common;

use common::cdl::{Cdl, yaml_style};
use common::image::{add_image, depth_name, port_depth, port_image};
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::exception::{Exception, Result};
use ocio_ops::image_desc::Bytes;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{CdlStyle, OptimizationFlags, TransformDirection};
use ocio_ops::ops::cdl::cdl_op::create_cdl_op;
use ocio_testkit::Oracle;
use ocio_testkit::battery::{BitDepth as Depth, yaml_list, yaml_number};
use ocio_testkit::image::{Buffer, Request};
use ocio_testkit::oracle::BatchCall;
use serde_json::{Value, json};

use CdlStyle::{Asc, NoClamp};
use TransformDirection::{Forward as F, Inverse as I};

/// A transform of the lists: a CDL and its direction.
type T = (Cdl, TransformDirection);

/// Whether every parameter is finite: then JSON can hold them.
fn finite(c: &Cdl) -> bool {
    c.slope
        .iter()
        .chain(&c.offset)
        .chain(&c.power)
        .chain([&c.sat])
        .all(|v| v.is_finite())
}

/// The config YAML of a CDL.
fn yaml((c, dir): &T) -> String {
    let dir = match dir {
        F => "forward",
        I => "inverse",
    };
    format!(
        "!<CDLTransform> {{slope: {}, offset: {}, power: {}, sat: {}, style: {}, direction: {}}}",
        yaml_list(&c.slope),
        yaml_list(&c.offset),
        yaml_list(&c.power),
        yaml_number(c.sat),
        yaml_style(c.style),
        dir
    )
}

/// A CDL.
fn cdl(slope: [f64; 3], offset: [f64; 3], power: [f64; 3], sat: f64, style: CdlStyle) -> Cdl {
    Cdl {
        slope,
        offset,
        power,
        sat,
        style,
    }
}

/// Lists of transforms: each style and direction alone (with a power other than 1), with a
/// power of 1 (simplified into matrices and clamps, with or without saturation), as an
/// identity, as an inverse pair (and pairs that are not, past the tolerance), next to each
/// other; NaN and infinite parameters; and refusals.
fn chains() -> Vec<Vec<T>> {
    // tests/cpu/ops/cdl/CDLOp_tests.cpp:60-66 @ v2.5.2.
    let data_1 = |style| {
        cdl(
            [1.35, 1.1, 0.071],
            [0.05, -0.23, 0.11],
            [0.93, 0.81, 1.27],
            1.23,
            style,
        )
    };
    let power_1 = |sat, style| cdl([1.2, 0.8, 1.1], [0.05, -0.1, 0.0], [1.0; 3], sat, style);
    let identity = |style| cdl([1.0; 3], [0.0; 3], [1.0; 3], 1.0, style);
    let mut chains = Vec::new();
    for style in [Asc, NoClamp] {
        for dir in [F, I] {
            let other = match dir {
                F => I,
                I => F,
            };
            chains.push(vec![(data_1(style), dir)]);
            chains.push(vec![(power_1(1.0, style), dir)]);
            chains.push(vec![(power_1(0.7, style), dir)]);
            chains.push(vec![(identity(style), dir)]);
            // An inverse pair, and pairs that differ: by a saturation, and by a power just past
            // the tolerance, and just within it.
            chains.push(vec![(data_1(style), dir), (data_1(style), other)]);
            let mut sat = data_1(style);
            sat.sat = 1.3;
            chains.push(vec![(data_1(style), dir), (sat, other)]);
            let mut past = data_1(style);
            past.power[1] += 2e-9;
            chains.push(vec![(data_1(style), dir), (past, other)]);
            let mut within = data_1(style);
            within.power[1] += 0.5e-9;
            chains.push(vec![(data_1(style), dir), (within, other)]);
            // Powers of 1 within the tolerance: simplified.
            let mut near = power_1(0.7, style);
            near.power = [1.0 + 0.5e-9, 1.0 - 0.5e-9, 1.0];
            chains.push(vec![(near, dir)]);
            // Two simplified CDLs whose matrices combine.
            chains.push(vec![(power_1(0.7, style), dir), (power_1(1.0, style), dir)]);
            // Saturation only: a saturation matrix (and clamps) when simplified.
            for sat in [0.7, 1.3] {
                chains.push(vec![(cdl([1.0; 3], [0.0; 3], [1.0; 3], sat, style), dir)]);
            }
            // A power of 1 with a zero or tiny slope or saturation (I-62): the simplified
            // inverse matrices are singular, or divide by 0.005 where the renderer floors the
            // reciprocal at 0.01.
            for (slope, sat) in [
                ([0.0, 1.0, 1.2], 0.9),
                ([1.2, 1.0, 0.9], 0.0),
                ([0.005, 1.0, 1.2], 0.9),
                ([1.2, 1.0, 0.9], 0.005),
            ] {
                chains.push(vec![(
                    cdl(slope, [0.1, 0.0, -0.1], [1.0; 3], sat, style),
                    dir,
                )]);
            }
        }
    }
    let nan = f64::NAN;
    let inf = f64::INFINITY;
    chains.extend([
        // Parameters that need 7 significant digits, and an 8th.
        vec![(
            cdl(
                [1.234567, 0.1234567, 12.34567],
                [0.001234565, -1.2345678, 1234567.8],
                [1.1234567, 0.98765432, 2.5],
                0.9876543,
                Asc,
            ),
            F,
        )],
        // Both styles, one after the other.
        vec![(data_1(Asc), F), (data_1(NoClamp), I)],
        // NaN and infinite parameters (YAML).
        vec![(cdl([1.2, 1.0, 0.9], [0.1, nan, 0.0], [1.1; 3], 0.9, Asc), F)],
        vec![(
            cdl([1.2, 1.0, 0.9], [nan, 0.0, -0.1], [0.8; 3], 1.3, NoClamp),
            I,
        )],
        vec![(
            cdl([1.2, 1.0, 0.9], [nan, 0.0, -0.1], [1.0; 3], 1.3, NoClamp),
            F,
        )],
        vec![(
            cdl([inf, 1.0, 0.9], [0.1, 0.0, 0.0], [1.1, inf, 0.9], 0.9, Asc),
            F,
        )],
        // Refused: a negative slope, a zero power, a NaN power.
        vec![(cdl([-0.5, 1.0, 1.0], [0.0; 3], [1.2; 3], 1.0, Asc), F)],
        vec![(cdl([1.0; 3], [0.0; 3], [1.2, 0.0, 1.2], 1.0, NoClamp), I)],
        vec![(cdl([1.0; 3], [0.0; 3], [1.2, nan, 1.2], 1.0, NoClamp), F)],
    ]);
    chains
}

/// The optimization levels, and the CDL flags alone.
fn flags() -> Vec<(Value, OptimizationFlags)> {
    [
        ("OPTIMIZATION_NONE", OptimizationFlags::NONE),
        ("OPTIMIZATION_LOSSLESS", OptimizationFlags::LOSSLESS),
        ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
        ("OPTIMIZATION_GOOD", OptimizationFlags::GOOD),
        ("OPTIMIZATION_ALL", OptimizationFlags::ALL),
        ("OPTIMIZATION_IDENTITY", OptimizationFlags::IDENTITY),
        (
            "OPTIMIZATION_PAIR_IDENTITY_CDL",
            OptimizationFlags::PAIR_IDENTITY_CDL,
        ),
        ("OPTIMIZATION_SIMPLIFY_OPS", OptimizationFlags::SIMPLIFY_OPS),
    ]
    .into_iter()
    .map(|(name, flags)| (json!(name), flags))
    .collect()
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
    let mut args = if chain.iter().all(|(c, _)| finite(c)) {
        let children: Vec<Value> = chain.iter().map(|(c, dir)| c.spec(*dir)).collect();
        json!({"transform": {"class": "GroupTransform", "children": children}})
    } else {
        let children: Vec<String> = chain.iter().map(yaml).collect();
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
    let json = chain.iter().all(|(c, _)| finite(c));
    for (c, dir) in chain {
        if json {
            // The binding's constructor validates each child.
            c.op_data(*dir).validate().map_err(|e| {
                Exception::new(format!("CDLTransform validation failed: {}", e.message()))
            })?;
        }
    }
    let mut raw = OpVec::new();
    for (c, dir) in chain {
        let data = c.op_data(*dir);
        // BuildCDLOp.
        data.validate()?;
        create_cdl_op(&mut raw, data, F);
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

#[test]
fn the_pixels_match_the_wheel() {
    let flags = flags();
    let mut cases = Vec::new();
    for chain in chains() {
        for f in 0..flags.len() {
            cases.push((chain.clone(), f));
        }
    }
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

/// An op's ID heads its data's cache ID (`CDLOpData::getCacheID`, CDLOpData.cpp:492-511 @
/// v2.5.2): a `CDLTransform` with `setID` (CDLTransform.cpp:319-322), which sets its data's
/// ID, alone and next to one without, under every flag setting.
#[test]
fn the_cache_id_with_an_op_id_matches_the_wheel() {
    let with_id = cdl([1.2, 1.0, 0.9], [0.1, 0.0, 0.0], [1.1; 3], 0.4321, Asc);
    let chains: Vec<Vec<(Cdl, TransformDirection, Option<&str>)>> = vec![
        vec![(with_id, F, Some("abc"))],
        vec![(with_id, I, Some("abc"))],
        vec![(with_id, F, Some("abc")), (with_id, F, None)],
    ];
    let flags = flags();
    let mut cases = Vec::new();
    for chain in &chains {
        for f in 0..flags.len() {
            cases.push((chain, f));
        }
    }
    let pixel = vec![0u8; 16];
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(chain, f)| {
            let children: Vec<Value> = chain
                .iter()
                .map(|(c, dir, id)| {
                    let mut spec = c.spec(*dir);
                    if let Some(id) = id {
                        spec["calls"]
                            .as_array_mut()
                            .expect("the spec's calls")
                            .push(json!(["setID", id]));
                    }
                    spec
                })
                .collect();
            BatchCall {
                cmd: "cpu_apply",
                args: json!({
                    "transform": {"class": "GroupTransform", "children": children},
                    "optimization": flags[*f].0,
                }),
                blobs: vec![&pixel],
            }
        })
        .collect();
    let results = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for ((chain, f), result) in cases.iter().zip(results) {
        let result = result.unwrap_or_else(|e| panic!("{e}")).result;
        let wheel = result["cpu_cache_id"]
            .as_str()
            .unwrap_or_else(|| panic!("{result}"))
            .to_string();
        let port = (|| -> Result<String> {
            let mut raw = OpVec::new();
            for (c, dir, id) in chain.iter() {
                let mut data = c.op_data(*dir);
                if let Some(id) = id {
                    data.set_id(id.as_bytes());
                }
                data.validate()?;
                create_cdl_op(&mut raw, data, F);
            }
            raw.finalize()?;
            let cpu = CpuProcessor::new(
                &raw,
                port_depth(Depth::F32),
                port_depth(Depth::F32),
                flags[*f].1,
            )?;
            Ok(String::from_utf8(cpu.get_cache_id().to_vec()).unwrap())
        })()
        .unwrap_or_else(|e| e.message().to_string());
        if port != wheel {
            failures.push(format!(
                "{chain:?} {}\n  wheel {wheel:?}\n  port  {port:?}",
                flags[*f].0
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
