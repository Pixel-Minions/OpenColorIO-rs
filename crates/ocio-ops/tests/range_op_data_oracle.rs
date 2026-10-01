// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `RangeOpData` against the wheel, through the `RangeTransform` that holds one
//! (src/OpenColorIO/transforms/RangeTransform.cpp @ v2.5.2):
//! - `validate`: the transform's `validate` runs the data's and prefixes its message with
//!   `RangeTransform validation failed: ` (RangeTransform.cpp:52-72). The binding's constructor
//!   with bounds validates (src/bindings/python/transforms/PyRangeTransform.cpp:19-38), which
//!   the oracle's `transform_text` reports;
//! - the scale and the offset (`fillScaleOffset`) and the inverse (`getAsForward`), bit for
//!   bit: a transform of the `noClamp` style builds `convertToMatrix()` as a Matrix op
//!   (`BuildRangeOp`, src/OpenColorIO/ops/range/RangeOp.cpp:263-281), whose data cache ID hashes
//!   the bits of the scale and the offset (MatrixOpData.cpp:846-869). The port builds the same
//!   ops and the same CPU processor, and the two CPU processor cache IDs must be equal;
//! - `equals`: `RangeTransform::equals` compares the data with it, and the styles
//!   (RangeTransform.cpp:102-107), through `transform_text` pairs.
//!
//! The bounds reach the wheel in JSON where they are finite and in a config's YAML where some
//! are infinite (as the battery's `Spec::Yaml` does).

use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::exception::Result;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_ops::ops::range::RangeOpData;
use ocio_testkit::Oracle;
use ocio_testkit::battery::{Combo, Direction, Format, Spec, yaml_number};
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::probe::Rng;
use ocio_testkit::transform_text::{Built, TransformTextRequest};
use serde_json::{Map, Value, json};

/// The prefix of `RangeTransformImpl::validate`'s messages (RangeTransform.cpp:67-72 @ v2.5.2).
const PREFIX: &str = "RangeTransform validation failed: ";

/// The bounds `[minIn, maxIn, minOut, maxOut]`; `None` is an empty bound.
type Bounds = [Option<f64>; 4];

/// The keys of the bounds in the binding and in YAML.
const KEYS: [(&str, &str); 4] = [
    ("minInValue", "min_in_value"),
    ("maxInValue", "max_in_value"),
    ("minOutValue", "min_out_value"),
    ("maxOutValue", "max_out_value"),
];

/// The port's data of a `RangeTransform` with `bounds`, in the direction `dir`, without
/// validating: the transform's constructor sets each bound, then the direction.
fn port_data(bounds: &Bounds, dir: TransformDirection) -> RangeOpData {
    let mut data = RangeOpData::new();
    let [min_in, max_in, min_out, max_out] =
        bounds.map(|b| b.unwrap_or(RangeOpData::empty_value()));
    data.set_min_in_value(min_in);
    data.set_max_in_value(max_in);
    data.set_min_out_value(min_out);
    data.set_max_out_value(max_out);
    data.set_direction(dir);
    data
}

/// The JSON spec of a `RangeTransform` with finite `bounds`.
fn transform(bounds: &Bounds, dir: TransformDirection) -> Value {
    let mut args = Map::new();
    for ((key, _), bound) in KEYS.iter().zip(bounds) {
        if let Some(value) = bound {
            args.insert(key.to_string(), json!(value));
        }
    }
    if dir == TransformDirection::Inverse {
        args.insert("direction".into(), json!({"enum": "TRANSFORM_DIR_INVERSE"}));
    }
    json!({"class": "RangeTransform", "args": args})
}

/// A `RangeTransform` of the `noClamp` style in the config's YAML syntax.
fn yaml_no_clamp(bounds: &Bounds, dir: TransformDirection) -> String {
    let mut fields: Vec<String> = KEYS
        .iter()
        .zip(bounds)
        .filter_map(|((_, key), bound)| bound.map(|v| format!("{key}: {}", yaml_number(v))))
        .collect();
    fields.push("style: noClamp".into());
    let dir = match dir {
        TransformDirection::Forward => "forward",
        TransformDirection::Inverse => "inverse",
    };
    fields.push(format!("direction: {dir}"));
    format!("!<RangeTransform> {{{}}}", fields.join(", "))
}

/// The port's CPU processor cache ID of a `noClamp` `RangeTransform`, F32 in and out, without
/// optimization: `BuildRangeOp` validates the data, then builds `convertToMatrix()` forward
/// (RangeOp.cpp:263-281 @ v2.5.2). The processor doesn't run the transform's own `validate`, so
/// the data's messages come without its prefix.
fn port_no_clamp(bounds: &Bounds, dir: TransformDirection) -> Result<String> {
    let data = port_data(bounds, dir);
    data.validate()?;
    let mut ops = OpVec::new();
    create_matrix_op(
        &mut ops,
        data.convert_to_matrix()?,
        TransformDirection::Forward,
    );
    ops.finalize()?;
    let cpu = CpuProcessor::new(&ops, BitDepth::F32, BitDepth::F32, OptimizationFlags::NONE)?;
    Ok(String::from_utf8(cpu.get_cache_id().to_vec()).unwrap())
}

/// A double in about [-2^scale, 2^scale], of every sign.
fn value(rng: &mut Rng, scale: i32) -> f64 {
    let unit = (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
    (unit * 2.0 - 1.0) * 2f64.powi(scale)
}

/// Bounds with both ends set: upstream's tests' (RangeOpData_tests.cpp, RangeOpCPU_tests.cpp
/// @ v2.5.2), input bounds about and exactly 1e-6 apart (the "too close" check), constants,
/// values past the float range, overflowing differences, infinities, and random ones at several
/// magnitudes.
fn two_sided() -> Vec<[f64; 4]> {
    let mut out = vec![
        [0.0, 1.0, 0.5, 1.5],
        [-0.05432, 1.05432, 0.05432, 2.05432],
        [0.064, 0.940, 0.032, 0.235],
        [0.1, 1.2, -0.5, 2.],
        [-0.1, 1.1, -0.1, 1.1],
        [0., 1., 0.01, 1.],
        [16., 235., 0., 1.],
        [0., 1., 0., 0.],
        [0.5, 0.5 + 1e-6, 0., 1.],
        [0.5, 0.5 + 9.9e-7, 0., 1.],
        [0.5, 0.5 + 1.01e-6, 0., 1.],
        [1e5, 1e5 + 1e-6, 0., 1.],
        [0.0, 1e-6, 0., 1.],
        [-1e-6, 0.0, 0., 1.],
        [-1e39, 1e39, -1e39, 1e39],
        [-1e308, 1e308, -1e308, 1e308],
        [-1e308, 1e308, 0., 1.],
        [0., 1., -1e308, 1e308],
        [-1e-300, 1e-300, 0., 1.],
        [f64::NEG_INFINITY, f64::INFINITY, 0., 1.],
        [0., 1., f64::NEG_INFINITY, f64::INFINITY],
        [0., f64::INFINITY, 0., f64::INFINITY],
        [f64::NEG_INFINITY, 1., f64::NEG_INFINITY, 1.],
        [f64::MAX, f64::INFINITY, 0., 1.],
    ];
    let mut rng = Rng::new(0x5241_4e47);
    for scale in [-20, -3, 0, 3, 20, 60, 127] {
        for _ in 0..8 {
            let mut v = [0.0; 4].map(|_| value(&mut rng, scale));
            if v[0] > v[1] {
                v.swap(0, 1);
            }
            if v[2] > v[3] {
                v.swap(2, 3);
            }
            out.push(v);
        }
    }
    out
}

/// The oracle's `cpu_apply` of each spec, on one pixel.
fn run(specs: Vec<Value>) -> Vec<Value> {
    let pixel: Vec<u8> = [0.5f32, 0.25, 0.125, 1.0]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
    let calls: Vec<BatchCall<'_>> = specs
        .into_iter()
        .map(|args| BatchCall {
            cmd: "cpu_apply",
            args,
            blobs: vec![&pixel],
        })
        .collect();
    Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .map(|r| r.unwrap_or_else(|e| panic!("{e}")).result)
        .collect()
}

#[test]
fn the_scale_offset_and_inverse_match_the_wheel() {
    let mut cases = Vec::new();
    for bounds in two_sided() {
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            cases.push((bounds.map(Some), dir));
        }
    }
    let combo = Combo {
        direction: Direction::Forward,
        fast_math: true,
        format: Format::F32_RGBA,
    };
    let specs = cases
        .iter()
        .map(|(bounds, dir)| {
            let mut args = Spec::Yaml(yaml_no_clamp(bounds, *dir)).cpu_apply_args(&combo);
            args["optimization"] = json!("OPTIMIZATION_NONE");
            args
        })
        .collect();
    let results = run(specs);

    let mut failures = Vec::new();
    for ((bounds, dir), result) in cases.iter().zip(&results) {
        let wheel = match result.get("exception") {
            Some(exception) => Err(exception["message"].as_str().unwrap().to_string()),
            None => Ok(result["cpu_cache_id"].as_str().unwrap().to_string()),
        };
        let port = port_no_clamp(bounds, *dir).map_err(|e| e.message().to_string());
        if port != wheel {
            failures.push(format!(
                "{bounds:?} {dir:?}\n  wheel {wheel:?}\n  port  {port:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

/// Bounds for validation and equality, finite: every combination of empty ends, matching and
/// mismatched one-sided clamps (at the tolerances of `FloatsDiffer`, absolute near 0 and
/// relative elsewhere), reversed bounds, and close input bounds.
fn finite_bounds() -> Vec<Bounds> {
    let mut out: Vec<Bounds> = Vec::new();
    for mask in 0..16u32 {
        let full = [0.25, 0.75, 0.125, 1.5];
        out.push(std::array::from_fn(|k| {
            (mask & (1 << k) != 0).then_some(full[k])
        }));
    }
    for (a, b) in [
        (0.0, 1e-6),
        (0.0, 1.0001e-6),
        (0.0, 0.9999e-6),
        (0.0, 0.00001),
        (9e-4, 9e-4 + 1e-6),
        (9e-4, 9e-4 + 1.01e-6),
        (1e-3, 1e-3 * (1.0 + 1e-6)),
        (1e-3, 1e-3 * (1.0 + 1.01e-6)),
        (16.0, 16.0 * (1.0 + 0.99e-6)),
        (16.0, 16.0 * (1.0 + 1.01e-6)),
        (-16.0, -16.0 * (1.0 + 1.01e-6)),
        (1e39, 1e39 * (1.0 + 0.5e-6)),
    ] {
        out.push([Some(a), None, Some(b), None]);
        out.push([None, Some(a), None, Some(b)]);
        out.push([Some(b), None, Some(a), None]);
    }
    // `FloatsDiffer` picks its tolerance from its first argument (docs/improvements.md, I-51):
    // one-sided bounds near 1e-3 in each order, and ranges that are equal one way only.
    for (a, b) in [(1e-3, 9.995e-4), (9.995e-4, 1e-3)] {
        out.push([Some(a), None, Some(b), None]);
        out.push([None, Some(a), None, Some(b)]);
        out.push([Some(a), None, Some(a), None]);
        out.push([None, Some(a), None, Some(a)]);
    }
    out.extend([
        [Some(1.0), Some(0.0), Some(0.0), Some(1.0)],
        [Some(0.0), Some(1.0), Some(1.0), Some(0.0)],
        [Some(0.0), Some(0.0), Some(0.0), Some(1.0)],
        [Some(0.0), Some(5e-7), Some(0.0), Some(1.0)],
        [Some(0.0), Some(1.0), Some(0.5), Some(0.5)],
        [Some(16.0), Some(235.0), None, Some(2.0)],
        [Some(1e39), Some(-1e39), Some(0.0), Some(1.0)],
    ]);
    out
}

#[test]
fn validation_matches_the_wheel() {
    let bounds = finite_bounds();
    let mut transforms = Vec::new();
    for b in &bounds {
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            transforms.push(((b, dir), transform(b, dir)));
        }
    }
    let reply = TransformTextRequest {
        transforms: transforms.iter().map(|(_, spec)| spec.clone()).collect(),
        pairs: Vec::new(),
    }
    .run();

    let mut failures = Vec::new();
    for (((b, dir), _), built) in transforms.iter().zip(&reply.transforms) {
        let wheel = match built {
            Built::Raised(e) => Err(e.message.clone()),
            Built::Text(text) => match &text.validate {
                Some(e) => Err(e.message.clone()),
                None => Ok(()),
            },
        };
        let port = port_data(b, *dir)
            .validate()
            .map_err(|e| format!("{PREFIX}{}", e.message()));
        if port != wheel {
            failures.push(format!(
                "{b:?} {dir:?}\n  wheel {wheel:?}\n  port  {port:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn equality_matches_the_wheel() {
    // The valid bounds, each in both directions.
    let mut data = Vec::new();
    for b in finite_bounds() {
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            if port_data(&b, dir).validate().is_ok() {
                data.push((b, dir));
            }
        }
    }
    // Every pair of the ones with the same empty ends, and some others.
    let mut pairs = Vec::new();
    for i in 0..data.len() {
        for j in 0..data.len() {
            let shape = |b: &Bounds| b.map(|v| v.is_some());
            if shape(&data[i].0) == shape(&data[j].0) || (i + j) % 7 == 0 {
                pairs.push((i, j));
            }
        }
    }
    let reply = TransformTextRequest {
        transforms: data.iter().map(|(b, dir)| transform(b, *dir)).collect(),
        pairs: pairs.clone(),
    }
    .run();

    let mut failures = Vec::new();
    let mut wheel_equals = std::collections::HashMap::new();
    for ((i, j), wheel) in pairs.iter().zip(&reply.pairs) {
        let wheel = wheel.unwrap_or_else(|| panic!("no equals for {:?} {:?}", data[*i], data[*j]));
        wheel_equals.insert((*i, *j), wheel);
        let a = port_data(&data[*i].0, data[*i].1);
        let b = port_data(&data[*j].0, data[*j].1);
        if a.equals(&b) != wheel {
            failures.push(format!("{:?} == {:?}: wheel {wheel}", data[*i], data[*j]));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} pairs differ:\n{}",
        failures.len(),
        pairs.len(),
        failures.join("\n")
    );
    // Some pairs are equal in one order only (docs/improvements.md, I-51).
    let asymmetric = wheel_equals
        .iter()
        .filter(|((i, j), equal)| wheel_equals.get(&(*j, *i)).is_some_and(|e| e != *equal))
        .count();
    assert!(asymmetric > 0);
}
