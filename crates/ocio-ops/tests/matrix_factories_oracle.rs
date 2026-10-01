// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Matrix op's factories against the wheel, through the transforms and files whose ops
//! they build, and the Matrix ops' cache IDs, which hash the bits of the matrix and offsets
//! (MatrixOpData.cpp:846-869 @ v2.5.2):
//! - `CreateFitOp` (`MatrixTransform::Fit`): an `AllocationTransform` of the uniform
//!   allocation is one fit from its two variables to [0, 1] on RGB (`CreateAllocationOps`,
//!   src/OpenColorIO/ops/allocation/AllocationOp.cpp:42-60), in the transform's direction
//!   (`BuildAllocationOp`, src/OpenColorIO/transforms/AllocationTransform.cpp:189-205). Its
//!   variables are `float`s (`AllocationData::vars`). Equal variables raise Fit's message;
//! - `CreateScaleOffsetOp` and `CreateSaturationOp` (`MatrixTransform::Sat`): a `CDLTransform`
//!   in a version 1 config is a scale and offset, an exponent, then a saturation, in reverse
//!   order and inverted for the inverse direction (`BuildCDLOp`, src/OpenColorIO/ops/cdl/
//!   CDLOp.cpp:200-252); the binding's slope, offset and power fill RGB, alpha keeps 1, 0 and
//!   1, and the saturation's luma coefficients default to Rec. 709's;
//! - `CreateMinMaxOp(float, float)`: an `.spi1d` file's `From` range, before the LUT forward and
//!   after it inverse (src/OpenColorIO/fileformats/FileFormatSpi1D.cpp:430-440); an equal range
//!   raises, in the file transform's words.
//!
//! The wheel's processors also hold ops the port doesn't have yet (the exponent, the LUT), so
//! the test compares the Matrix ops' cache IDs, in order, from the CPU processor's cache ID,
//! without optimization. The processor finalizes the ops: an inverse Matrix op becomes its
//! forward equivalent.

use ocio_ops::exception::Result;
use ocio_ops::op::{Op, OpVec};
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::matrix::matrix_op::{
    create_fit_op, create_min_max_op_f32, create_saturation_op, create_scale_offset_op,
};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::probe::Rng;
use serde_json::{Value, json};

use TransformDirection::{Forward, Inverse};

/// A version 1 config with one color space.
const V1_CONFIG: &str = "ocio_profile_version: 1\n\
roles: {default: raw}\n\
colorspaces:\n  \
- !<ColorSpace> {name: raw}\n";

/// The cache IDs of the Matrix ops in a CPU processor's cache ID, in order.
fn matrix_ops(cpu_cache_id: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = cpu_cache_id;
    while let Some(start) = rest.find("<MatrixOffsetOp ") {
        let end = rest[start..].find(" >").expect("the op's end") + start + 2;
        out.push(rest[start..end].to_string());
        rest = &rest[end..];
    }
    out
}

/// The port's Matrix ops' cache IDs, after the processor's `finalize`.
fn port_matrix_ops(build: impl FnOnce(&mut OpVec) -> Result<()>) -> Result<Vec<String>> {
    let mut ops = OpVec::new();
    build(&mut ops)?;
    ops.finalize()?;
    Ok(ops
        .iter()
        .map(|op: &Op| String::from_utf8(op.get_cache_id()).unwrap())
        .collect())
}

/// The direction enum of a transform spec.
fn dir_enum(dir: TransformDirection) -> Value {
    match dir {
        Forward => json!({"enum": "TRANSFORM_DIR_FORWARD"}),
        Inverse => json!({"enum": "TRANSFORM_DIR_INVERSE"}),
    }
}

/// Runs the `cpu_apply` of each processor spec without optimization; each gives its Matrix
/// ops' cache IDs, or the exception's message.
fn run(specs: Vec<Value>) -> Vec<std::result::Result<Vec<String>, String>> {
    let pixel: Vec<u8> = [0.5f32, 0.25, 0.125, 1.0]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
    let calls: Vec<BatchCall<'_>> = specs
        .into_iter()
        .map(|mut args| {
            args["optimization"] = json!("OPTIMIZATION_NONE");
            BatchCall {
                cmd: "cpu_apply",
                args,
                blobs: vec![&pixel],
            }
        })
        .collect();
    Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .map(|r| {
            let result = r.unwrap_or_else(|e| panic!("{e}")).result;
            match result.get("exception") {
                Some(exception) => Err(exception["message"].as_str().unwrap().to_string()),
                None => Ok(matrix_ops(result["cpu_cache_id"].as_str().unwrap())),
            }
        })
        .collect()
}

/// Compares the wheel's and the port's results, case by case.
fn check<C: std::fmt::Debug>(
    cases: &[C],
    wheel: Vec<std::result::Result<Vec<String>, String>>,
    port: impl Fn(&C) -> Result<Vec<String>>,
) {
    let mut failures = Vec::new();
    for (case, wheel) in cases.iter().zip(wheel) {
        let port = port(case).map_err(|e| e.message().to_string());
        if port != wheel {
            failures.push(format!("{case:?}\n  wheel {wheel:?}\n  port  {port:?}"));
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

/// A double in about [-2^scale, 2^scale], of every sign.
fn value(rng: &mut Rng, scale: i32) -> f64 {
    let unit = (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
    (unit * 2.0 - 1.0) * 2f64.powi(scale)
}

#[test]
fn fit_matches_the_wheel() {
    let mut vars: Vec<[f64; 2]> = vec![
        [0.0, 1.0],
        [-0.125, 2.5],
        [0.1, 0.9],
        [2.0, -3.0],
        [1.0, 1.0],
        [1e-30, 2e-30],
        [-1e30, 1e30],
        [0.0, 1e-45],
    ];
    let mut rng = Rng::new(0xf17);
    for scale in [-8, 0, 8, 60] {
        for _ in 0..6 {
            vars.push([value(&mut rng, scale), value(&mut rng, scale)]);
        }
    }
    let mut cases = Vec::new();
    for v in vars {
        for dir in [Forward, Inverse] {
            cases.push((v, dir));
        }
    }
    let specs = cases
        .iter()
        .map(|(v, dir)| {
            json!({"transform": {"class": "AllocationTransform", "args": {
                "allocation": {"enum": "ALLOCATION_UNIFORM"},
                "vars": v.to_vec(),
                "direction": dir_enum(*dir),
            }}})
        })
        .collect();
    let wheel = run(specs);
    check(&cases, wheel, |(v, dir)| {
        // `AllocationData::vars` holds `float`s.
        let [min, max] = v.map(|x| f64::from(x as f32));
        port_matrix_ops(|ops| {
            create_fit_op(
                ops,
                &[min, min, min, 0.0],
                &[max, max, max, 1.0],
                &[0.0; 4],
                &[1.0; 4],
                *dir,
            )
        })
    });
}

/// The CDL parameters: slope, offset, power (RGB) and saturation.
type Cdl = ([f64; 3], [f64; 3], [f64; 3], f64);

#[test]
fn scale_offset_and_saturation_match_the_wheel() {
    let mut cdls: Vec<Cdl> = vec![
        ([1.0; 3], [0.0; 3], [1.0; 3], 1.0),
        ([1.1, 1.2, 0.9], [0.01, -0.02, 0.03], [1.0; 3], 0.8),
        ([2.0, 0.5, 4.0], [0.0; 3], [1.0; 3], 0.0),
        ([0.0, 1.0, 1.0], [0.1; 3], [1.0; 3], 1.5),
    ];
    let mut rng = Rng::new(0xcd1);
    for _ in 0..12 {
        let r = |rng: &mut Rng, s| value(rng, s);
        cdls.push((
            [
                r(&mut rng, 1).abs(),
                r(&mut rng, 1).abs(),
                r(&mut rng, 1).abs(),
            ],
            [r(&mut rng, -2), r(&mut rng, -2), r(&mut rng, -2)],
            [1.0; 3],
            r(&mut rng, 1).abs(),
        ));
    }
    let mut cases = Vec::new();
    for cdl in cdls {
        for dir in [Forward, Inverse] {
            cases.push((cdl, dir));
        }
    }
    let specs = cases
        .iter()
        .map(|((slope, offset, power, sat), dir)| {
            json!({
                "config": {"yaml": V1_CONFIG},
                "transform": {"class": "CDLTransform", "args": {
                    "slope": slope.to_vec(),
                    "offset": offset.to_vec(),
                    "power": power.to_vec(),
                    "sat": sat,
                    "direction": dir_enum(*dir),
                }},
            })
        })
        .collect();
    let wheel = run(specs);
    // The default luma coefficients (src/OpenColorIO/ops/cdl/CDLOpData.cpp, Rec. 709).
    let luma = [0.2126, 0.7152, 0.0722];
    check(&cases, wheel, |((slope, offset, _, sat), dir)| {
        let slope4 = [slope[0], slope[1], slope[2], 1.0];
        let offset4 = [offset[0], offset[1], offset[2], 0.0];
        port_matrix_ops(|ops| {
            match dir {
                Forward => {
                    create_scale_offset_op(ops, &slope4, &offset4, Forward);
                    create_saturation_op(ops, *sat, &luma, Forward);
                }
                Inverse => {
                    create_saturation_op(ops, *sat, &luma, Inverse);
                    create_scale_offset_op(ops, &slope4, &offset4, Inverse);
                }
            }
            Ok(())
        })
    });
}

#[test]
fn min_max_matches_the_wheel() {
    let dir = std::env::temp_dir().join(format!("ocio-rs-min-max-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ranges: Vec<[f32; 2]> = vec![
        [0.0, 1.0],
        [-0.125, 2.5],
        [0.1, 0.9],
        [-4.0, 4.0],
        [0.5, 0.5],
        [1e-3, 3.0e3],
    ];
    // Random ranges: where `-min * (1 / range)` and `-min / range` round differently.
    let mut rng = Rng::new(0x5b1d);
    let mut ranges = ranges;
    for scale in [-4, 0, 4] {
        for _ in 0..8 {
            let a = value(&mut rng, scale) as f32;
            let b = value(&mut rng, scale) as f32;
            ranges.push([a.min(b), a.max(b)]);
        }
    }
    let mut cases = Vec::new();
    let mut specs = Vec::new();
    let mut paths = Vec::new();
    for (k, range) in ranges.iter().enumerate() {
        let path = dir.join(format!("r{k}.spi1d"));
        std::fs::write(
            &path,
            format!(
                "Version 1\nFrom {} {}\nLength 2\nComponents 1\n{{\n 0.0\n 1.0\n}}\n",
                range[0], range[1]
            ),
        )
        .unwrap();
        for d in [Forward, Inverse] {
            cases.push((*range, d, paths.len()));
            specs.push(json!({"transform": {"class": "FileTransform", "args": {
                "src": path.to_str().unwrap(),
                "direction": dir_enum(d),
            }}}));
        }
        paths.push(path);
    }
    let wheel = run(specs);
    check(&cases, wheel, |([min, max], d, k)| {
        port_matrix_ops(|ops| create_min_max_op_f32(ops, *min, *max, *d)).map_err(|e| {
            // `BuildFileTransformOps` wraps what building the ops raised
            // (src/OpenColorIO/transforms/FileTransform.cpp:962-970 @ v2.5.2).
            ocio_ops::exception::Exception::new(format!(
                "The transform file: {} failed while building ops with this error: {}",
                paths[*k].to_str().unwrap(),
                e.message()
            ))
        })
    });
    std::fs::remove_dir_all(&dir).ok();
}
