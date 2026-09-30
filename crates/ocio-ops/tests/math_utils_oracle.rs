// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `MathUtils`' tolerance tests against the wheel, through the code that calls them. Upstream
//! has no dedicated test for the `double` versions, which compare after converting to `float`
//! (src/OpenColorIO/MathUtils.cpp:17-71, 162-191 @ v2.5.2):
//! - `IsScalarEqualToZero<double>` decides whether `LogOpData` refuses a linear or log side
//!   slope ("... slope cannot be 0", src/OpenColorIO/ops/log/LogOpData.cpp:45-60 @ v2.5.2).
//!   The Python binding's constructors validate the transform.
//! - `IsVecEqualToOne<double>` decides whether the `ExponentOp` of a version 1 config's
//!   ExponentTransform is a no-op, which the optimizer removes
//!   (src/OpenColorIO/ops/exponent/ExponentOp.cpp:62-70 and
//!   src/OpenColorIO/ops/gamma/GammaOp.cpp:195-206 @ v2.5.2); its CPU processor's cache ID
//!   names the ops that remain.
//! - `IsM44Identity<double>` and `IsVecEqualToZero<double>` decide whether a serialized config
//!   writes a MatrixTransform's matrix and offset (src/OpenColorIO/OCIOYaml.cpp:3069-3083
//!   @ v2.5.2).
//!
//! They compare within 2 float ULPs, so the probes are the doubles around the float roundings
//! of 0 and 1: the float neighbours, the ties between them and the doubles next to the ties.

use ocio_ops::math_utils::{
    is_m44_identity, is_scalar_equal_to_zero, is_vec_equal_to_one, is_vec_equal_to_zero,
};
use ocio_testkit::Oracle;
use ocio_testkit::battery::yaml_number;
use ocio_testkit::oracle::{BatchCall, f32_to_bytes};
use serde_json::{Value, json};

/// `x` and the two doubles next to it.
fn with_neighbours(x: f64) -> [f64; 3] {
    [x.next_down(), x, x.next_up()]
}

/// Doubles around the floats `k` steps of `step` from `reference`, for `k` in 0..=`steps`, and
/// around the ties halfway between them.
fn probes_around(reference: f64, step: f64, steps: u32) -> Vec<f64> {
    let mut probes = Vec::new();
    for k in 0..=steps {
        let k = f64::from(k);
        probes.extend(with_neighbours(reference + k * step));
        probes.extend(with_neighbours(reference + (k + 0.5) * step));
    }
    probes
}

/// The oracle's `cpu_apply` of each call, on one pixel.
fn run(args: Vec<Value>) -> Vec<Value> {
    let pixel = f32_to_bytes(&[0.5, 0.25, 0.125, 1.0]);
    let calls: Vec<BatchCall<'_>> = args
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

/// Whether the wheel refused the call with `refusal` in its message; any other exception fails.
fn refused(result: &Value, refusal: &str, label: &str) -> bool {
    match result.get("exception") {
        None => false,
        Some(exception) => {
            let message = exception["message"].as_str().unwrap_or_default();
            assert!(message.contains(refusal), "{label}: {result}");
            true
        }
    }
}

/// `IsScalarEqualToZero<double>` is the wheel's test for a zero log slope: at 0 and every
/// double that rounds to a float within 2 ULPs of it, of either sign, and away from it. NaN and
/// the infinities, which JSON can't hold, go through a config.
#[test]
fn is_scalar_equal_to_zero_is_the_wheels_zero_slope_test() {
    // The float denormals step by 2^-149.
    let step = f64::from(f32::from_bits(1));
    let mut values = probes_around(0.0, step, 5);
    values.extend(values.clone().iter().map(|v| -v));
    values.extend([
        f64::from_bits(1),
        f64::MIN_POSITIVE,
        1e-300,
        1e-40,
        f64::from(f32::MIN_POSITIVE),
        1.0,
        f64::from(f32::MAX),
        f64::MAX,
    ]);

    let mut cases: Vec<(String, f64, Value, &str)> = Vec::new();
    for &v in &values {
        for (arg, refusal) in [
            ("linSideSlope", "linear side slope cannot be 0"),
            ("logSideSlope", "log side slope cannot be 0"),
        ] {
            let transform = json!({"class": "LogAffineTransform", "args": {arg: [v, 1.0, 1.0]}});
            cases.push((
                format!("{arg} {v:e}"),
                v,
                json!({ "transform": transform }),
                refusal,
            ));
        }
    }
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let yaml = format!(
            "ocio_profile_version: 2.1\nroles:\n  default: raw\ncolorspaces:\n  - !<ColorSpace>\n    \
             name: raw\n  - !<ColorSpace>\n    name: cs\n    from_scene_reference: \
             !<LogAffineTransform> {{lin_side_slope: [{}, 1, 1]}}\n",
            yaml_number(v)
        );
        let args = json!({"config": {"yaml": yaml}, "src": "raw", "dst": "cs"});
        cases.push((
            format!("lin_side_slope {v}"),
            v,
            args,
            "linear side slope cannot be 0",
        ));
    }

    let results = run(cases.iter().map(|(_, _, args, _)| args.clone()).collect());
    let mut zeros = 0;
    for ((label, v, _, refusal), result) in cases.iter().zip(&results) {
        let wheel = refused(result, refusal, label);
        assert_eq!(is_scalar_equal_to_zero(*v), wheel, "{label}: {result}");
        zeros += usize::from(wheel);
    }
    // Both answers occur, on both sides of the boundary.
    assert!(
        zeros > 0 && zeros < cases.len(),
        "{zeros} of {}",
        cases.len()
    );
}

/// `IsVecEqualToOne<double>` is the wheel's no-op test for a version 1 config's
/// ExponentTransform: the ExponentOp is removed exactly when each of its four exponents rounds
/// to a float within 2 ULPs of 1. Each channel in turn holds the probes, the others 1.
#[test]
fn is_vec_equal_to_one_is_the_wheels_exponent_no_op_test() {
    // Above 1 the floats step by 2^-23, below it by 2^-24.
    let above = f64::from(1.0f32.next_up()) - 1.0;
    let mut values = probes_around(1.0, above, 4);
    values.extend(probes_around(1.0, -above / 2.0, 4));
    values.extend([0.0, 2.0, 1e-300, 1e300]);
    let exponents: Vec<(usize, [f64; 4])> = (0..4)
        .flat_map(|channel| {
            values.iter().map(move |&v| {
                let mut exponent = [1.0; 4];
                exponent[channel] = v;
                (channel, exponent)
            })
        })
        .collect();

    let args = exponents
        .iter()
        .map(|(_, exponent)| {
            let list: Vec<String> = exponent.iter().map(|&v| yaml_number(v)).collect();
            let yaml = format!(
                "ocio_profile_version: 1\nroles:\n  default: raw\ncolorspaces:\n  - !<ColorSpace>\n    \
                 name: raw\n  - !<ColorSpace>\n    name: cs\n    from_reference: \
                 !<ExponentTransform> {{value: [{}]}}\n",
                list.join(", ")
            );
            json!({"config": {"yaml": yaml}, "src": "raw", "dst": "cs"})
        })
        .collect();
    let results = run(args);
    let mut no_ops = [0; 4];
    for ((channel, exponent), result) in exponents.iter().zip(&results) {
        let label = format!("channel {channel} {:e}", exponent[*channel]);
        assert!(result.get("exception").is_none(), "{label}: {result}");
        let kept = result["cpu_cache_id"]
            .as_str()
            .expect("a cache ID")
            .contains("<ExponentOp");
        assert_eq!(is_vec_equal_to_one(exponent), !kept, "{label}: {result}");
        no_ops[*channel] += usize::from(!kept);
    }
    // Both answers occur in every channel.
    for (channel, &count) in no_ops.iter().enumerate() {
        assert!(
            count > 0 && count < values.len(),
            "channel {channel}: {count} of {}",
            values.len()
        );
    }
}

/// `IsM44Identity<double>` and `IsVecEqualToZero<double>` are the wheel's tests for leaving a
/// MatrixTransform's matrix and offset out of a serialized config: each entry of the identity
/// and of a zero offset is replaced in turn by the doubles around the roundings of its value.
#[test]
fn is_m44_identity_is_the_wheels_identity_matrix_test() {
    let identity: [f64; 16] = std::array::from_fn(|i| if i % 5 == 0 { 1.0 } else { 0.0 });
    let one_step = f64::from(1.0f32.next_up()) - 1.0;
    let zero_step = f64::from(f32::from_bits(1));
    let around_one: Vec<f64> = [
        probes_around(1.0, one_step, 3),
        probes_around(1.0, -one_step / 2.0, 3),
    ]
    .concat();
    let mut around_zero = probes_around(0.0, zero_step, 3);
    around_zero.extend(around_zero.clone().iter().map(|v| -v));

    let mut cases: Vec<([f64; 16], [f64; 4])> = Vec::new();
    for i in 0..16 {
        let values = if i % 5 == 0 {
            &around_one
        } else {
            &around_zero
        };
        for &v in values {
            let mut matrix = identity;
            matrix[i] = v;
            cases.push((matrix, [0.0; 4]));
        }
    }
    for i in 0..4 {
        for &v in &around_zero {
            let mut offset = [0.0; 4];
            offset[i] = v;
            cases.push((identity, offset));
        }
    }

    let list = |values: &[f64]| {
        let items: Vec<String> = values.iter().map(|&v| yaml_number(v)).collect();
        format!("[{}]", items.join(", "))
    };
    let yamls: Vec<String> = cases
        .iter()
        .map(|(matrix, offset)| {
            format!(
                "ocio_profile_version: 2.1\nroles:\n  default: raw\ncolorspaces:\n  - !<ColorSpace>\n    \
                 name: raw\n  - !<ColorSpace>\n    name: cs\n    from_scene_reference: \
                 !<MatrixTransform> {{matrix: {}, offset: {}}}\n",
                list(matrix),
                list(offset)
            )
        })
        .collect();
    let calls: Vec<BatchCall<'_>> = yamls
        .iter()
        .map(|yaml| BatchCall {
            cmd: "config_serialize",
            args: json!({"config": {"yaml": yaml}}),
            blobs: Vec::new(),
        })
        .collect();

    let (mut identities, mut zeros) = (0, 0);
    for ((matrix, offset), response) in cases.iter().zip(Oracle::get().batch(&calls, true)) {
        let response = response.unwrap_or_else(|e| panic!("{e}"));
        assert!(
            response.result.get("exception").is_none(),
            "{}",
            response.result
        );
        let written = response
            .blob_text(0)
            .lines()
            .find(|line| line.contains("!<MatrixTransform>"))
            .expect("the MatrixTransform")
            .to_string();
        let label = format!("{matrix:?} {offset:?}: {written}");
        assert_eq!(
            is_m44_identity(matrix),
            !written.contains("matrix:"),
            "{label}"
        );
        assert_eq!(
            is_vec_equal_to_zero(offset),
            !written.contains("offset:"),
            "{label}"
        );
        identities += usize::from(is_m44_identity(matrix));
        zeros += usize::from(is_vec_equal_to_zero(offset));
    }
    // Both answers occur for the matrices and for the offsets.
    assert!(identities > 0 && identities < cases.len());
    assert!(zeros > 0 && zeros < cases.len());
}
