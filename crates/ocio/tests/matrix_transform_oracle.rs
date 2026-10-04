// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `MatrixTransform` against the wheel (`common::transforms`):
//! - its text (`repr()`, `str()`), with 16 significant digits, every bit depth's name, NaNs as
//!   each platform spells them (I-7), and groups of them; its validation, an inverse matrix
//!   that can't be inverted included; `equals()` between transforms that differ in each part;
//! - the raw config's processor of each, in both directions: `BuildMatrixOp`, then
//!   `CreateMatrixTransform` through `createGroupTransform()`;
//! - the static functions `Fit`, `Identity`, `Sat`, `Scale` and `View` (the binding's, which
//!   build a transform from them), their values bit for bit and `Fit`'s error.

mod common;

use common::transforms::{
    BIT_DEPTHS, Case, bit_depth_spec, check_processors, check_text, direction_spec, group,
    special_doubles,
};
use ocio::{BitDepth, MatrixTransform, TransformDirection};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::processor_ops::{ProcessorOpsReply, ProcessorOpsRequest};
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

use TransformDirection::{Forward, Inverse};

/// Doubles as spec values.
fn f64s(values: &[f64]) -> Value {
    Value::Array(values.iter().map(|&v| f64_spec(v)).collect())
}

/// A matrix transform's parameters.
#[derive(Debug, Clone, Copy)]
struct Params {
    m44: [f64; 16],
    offset4: [f64; 4],
    dir: TransformDirection,
    depths: (BitDepth, BitDepth),
}

impl Default for Params {
    fn default() -> Self {
        let (m44, offset4) = MatrixTransform::identity();
        Params {
            m44,
            offset4,
            dir: Forward,
            depths: (BitDepth::Unknown, BitDepth::Unknown),
        }
    }
}

/// The case of `params`, built through the setters (the binding's constructor validates).
fn matrix_case(label: impl Into<String>, params: Params) -> Case {
    let mut port = MatrixTransform::new();
    port.set_matrix(&params.m44);
    port.set_offset(&params.offset4);
    port.set_direction(params.dir);
    port.set_file_input_bit_depth(params.depths.0);
    port.set_file_output_bit_depth(params.depths.1);
    let spec = json!({"class": "MatrixTransform", "calls": [
        ["setMatrix", f64s(&params.m44)],
        ["setOffset", f64s(&params.offset4)],
        ["setDirection", direction_spec(params.dir)],
        ["setFileInputBitDepth", bit_depth_spec(params.depths.0)],
        ["setFileOutputBitDepth", bit_depth_spec(params.depths.1)],
    ]});
    Case::new(label, spec, port)
}

/// A diagonal matrix.
fn diagonal(d: [f64; 4]) -> [f64; 16] {
    MatrixTransform::scale(&d).0
}

/// The matrix of `tests/cpu/transforms/MatrixTransform_tests.cpp:47-70 @ v2.5.2`.
fn upstream_params() -> Params {
    Params {
        m44: [
            1.0, 1.01, 1.02, 1.03, 1.04, 1.05, 1.06, 1.07, 1.08, 1.09, 1.10, 1.11, 1.12, 1.13,
            1.14, 1.15,
        ],
        offset4: [1.0, 1.1, 1.2, 1.3],
        dir: Forward,
        depths: (BitDepth::Uint8, BitDepth::Uint10),
    }
}

/// The matrix cases: the default, upstream's, every bit depth, the special doubles in every
/// position, matrices that can't be inverted, and some that can.
fn matrix_cases() -> Vec<Case> {
    let mut cases = vec![Case::new(
        "the default",
        json!({"class": "MatrixTransform"}),
        MatrixTransform::new(),
    )];
    for dir in [Forward, Inverse] {
        cases.push(matrix_case(
            format!("upstream's, {dir:?}"),
            Params {
                dir,
                ..upstream_params()
            },
        ));
    }
    for (i, &depth) in BIT_DEPTHS.iter().enumerate() {
        cases.push(matrix_case(
            format!("bit depths {depth:?}"),
            Params {
                depths: (depth, BIT_DEPTHS[(i + 4) % BIT_DEPTHS.len()]),
                ..Params::default()
            },
        ));
    }
    let specials = special_doubles();
    for start in (0..specials.len()).step_by(4) {
        let mut params = Params::default();
        for (i, value) in params.m44.iter_mut().enumerate() {
            *value = specials[(start + i) % specials.len()];
        }
        for (i, value) in params.offset4.iter_mut().enumerate() {
            *value = specials[(start + 16 + i) % specials.len()];
        }
        for dir in [Forward, Inverse] {
            cases.push(matrix_case(
                format!("specials from {start}, {dir:?}"),
                Params { dir, ..params },
            ));
        }
    }
    // One special value on the diagonal of an invertible matrix, inverted.
    for (k, &value) in specials.iter().enumerate() {
        cases.push(matrix_case(
            format!("special {k} on the diagonal, inverse"),
            Params {
                m44: diagonal([2.0, value, 0.5, 1.0]),
                offset4: [0.1, value, -0.2, 0.0],
                dir: Inverse,
                ..Params::default()
            },
        ));
    }
    for (label, m44) in [
        ("zeros", [0.0; 16]),
        ("a zero on the diagonal", diagonal([1.0, 0.0, 1.0, 1.0])),
        (
            "two equal rows",
            [
                1.0, 2.0, 3.0, 0.0, 1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
        ),
        ("a tiny diagonal", diagonal([1e-300, 1.0, 1.0, 1.0])),
        ("a huge diagonal", diagonal([1e300, 1e300, 1.0, 1.0])),
    ] {
        for dir in [Forward, Inverse] {
            cases.push(matrix_case(
                format!("{label}, {dir:?}"),
                Params {
                    m44,
                    offset4: [0.5, 0.0, -0.5, 1.0],
                    dir,
                    ..Params::default()
                },
            ));
        }
    }
    cases
}

/// Groups of matrix transforms, nested, in both directions.
fn group_cases(matrices: &[Case]) -> Vec<Case> {
    let a = &matrices[1];
    let b = &matrices[matrices.len() - 1];
    let nested = group("a group of two", Inverse, &[a.clone(), b.clone()]);
    vec![
        group("a group of one", Forward, std::slice::from_ref(a)),
        nested.clone(),
        group(
            "a nested group",
            Forward,
            &[b.clone(), nested, matrices[0].clone()],
        ),
    ]
}

/// Pairs for `equals()`: each case with itself, with the next one, and with a copy (the same
/// spec built again); and pairs that differ in one part each.
fn pairs(n: usize) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for i in 0..n {
        pairs.extend([(i, i), (i, (i + 1) % n)]);
    }
    pairs
}

#[test]
fn text_validation_and_equality_match_the_wheel() {
    let mut cases = matrix_cases();
    let groups = group_cases(&cases);
    let n = cases.len();
    // Each matrix case built again: equal unless a NaN is in it.
    let copies: Vec<Case> = cases.clone();
    cases.extend(copies);
    let mut pairs = pairs(n);
    pairs.extend((0..n).map(|i| (i, i + n)));
    // Cases that differ from upstream's in one part each.
    let base = upstream_params();
    let first = cases.len();
    let mut variants = vec![matrix_case("upstream's again", base)];
    variants.push(matrix_case(
        "other file bit depths",
        Params {
            depths: (BitDepth::F32, BitDepth::F16),
            ..base
        },
    ));
    let mut m44 = base.m44;
    m44[7] += 1e-15;
    variants.push(matrix_case("a matrix value", Params { m44, ..base }));
    let mut offset4 = base.offset4;
    offset4[3] = 1.3000000000000003;
    variants.push(matrix_case("an offset", Params { offset4, ..base }));
    variants.push(matrix_case(
        "inverse",
        Params {
            dir: Inverse,
            ..base
        },
    ));
    let zero = Params {
        offset4: [0.0; 4],
        ..Params::default()
    };
    let mut negative_zero = zero;
    negative_zero.offset4[2] = -0.0;
    negative_zero.m44[1] = -0.0;
    variants.push(matrix_case("zero offsets", zero));
    variants.push(matrix_case("negative zeros", negative_zero));
    let k = variants.len();
    cases.extend(variants);
    for j in 1..k {
        pairs.push((first, first + j));
        pairs.push((first + j, first));
    }
    pairs.push((first + k - 2, first + k - 1));
    pairs.push((first + k - 1, first + k - 2));
    // Groups have no equals() in the binding, and a matrix compares only with a matrix.
    let g = cases.len();
    cases.extend(groups);
    pairs.extend([(g, g), (g, 0), (0, g)]);
    check_text(&cases, &pairs);
}

#[test]
fn processors_match_the_wheel() {
    let mut cases = matrix_cases();
    let groups = group_cases(&cases);
    cases.extend(groups);
    check_processors(&cases);
}

/// A static function's call, as the binding takes it, and the port's result.
struct Static {
    label: String,
    factory: Value,
    port: ocio::Result<([f64; 16], [f64; 4])>,
}

/// The static functions' cases.
fn statics() -> Vec<Static> {
    let mut out = Vec::new();
    let mut fit = |old_min: [f64; 4], old_max: [f64; 4], new_min: [f64; 4], new_max: [f64; 4]| {
        out.push(Static {
            label: format!("Fit {old_min:?} {old_max:?} {new_min:?} {new_max:?}"),
            factory: json!([
                "Fit",
                f64s(&old_min),
                f64s(&old_max),
                f64s(&new_min),
                f64s(&new_max)
            ]),
            port: MatrixTransform::fit(&old_min, &old_max, &new_min, &new_max),
        });
    };
    let specials = special_doubles();
    // tests/python/MatrixTransformTest.py:26-29 @ v2.5.2.
    fit([0.1; 4], [0.9; 4], [0.0; 4], [1.1; 4]);
    fit([0.0; 4], [1.0; 4], [0.0; 4], [1.0; 4]);
    fit(
        [-0.5, 0.0, 1.0, 0.0],
        [2.0, 1.0 / 3.0, 3.0, 1.0],
        [0.1, 0.2, -1.0, 0.0],
        [0.7, 0.9, 5.0, 1.0],
    );
    // A range of 0 in each channel, and one that is 0 only in float precision.
    for i in 0..4 {
        let mut old_max = [1.0; 4];
        old_max[i] = 0.0;
        fit([0.0; 4], old_max, [0.0; 4], [1.0; 4]);
        let mut old_min = [0.0; 4];
        let mut old_max = [1.0; 4];
        old_min[i] = 1.0 / 3.0;
        old_max[i] = 1.0 / 3.0 + 1e-12;
        fit(old_min, old_max, [0.0; 4], [1.0; 4]);
    }
    fit([1e300; 4], [1e300; 4], [0.0; 4], [1.0; 4]);
    // Two different NaNs in one product, the other product finite: each product's order shows.
    let nans = [
        f64::NAN,
        -f64::NAN,
        f64::from_bits(0x7ff0_0000_0000_0001),
        f64::from_bits(0xfff8_0000_dead_beef),
    ];
    for (a, b) in [(0, 1), (1, 0), (2, 3), (3, 2), (0, 2)] {
        let (a, b) = (nans[a], nans[b]);
        fit([a; 4], [1.0; 4], [0.0; 4], [b; 4]);
        fit([0.0; 4], [a; 4], [b; 4], [1.0; 4]);
    }
    for (k, &v) in specials.iter().enumerate() {
        let rotate = |s: usize| [0, 1, 2, 3].map(|i| specials[(k + s + i) % specials.len()]);
        fit([0.0, v, 0.0, 0.0], [1.0; 4], [0.0; 4], [1.0; 4]);
        fit([0.0; 4], [1.0, 1.0, v, 1.0], [0.0; 4], [1.0; 4]);
        fit([0.0; 4], [1.0; 4], [v; 4], [2.0, v, 0.5, 1.0]);
        fit(rotate(0), rotate(1), rotate(2), rotate(3));
    }

    out.push(Static {
        label: "Identity".to_string(),
        factory: json!(["Identity"]),
        port: Ok(MatrixTransform::identity()),
    });

    let rec709 = [0.2126, 0.7152, 0.0722];
    let sat = |sat: f64, luma: [f64; 3]| Static {
        label: format!("Sat {sat} {luma:?}"),
        factory: json!(["Sat", f64_spec(sat), f64s(&luma)]),
        port: Ok(MatrixTransform::sat(sat, &luma)),
    };
    // tests/python/MatrixTransformTest.py:38 @ v2.5.2.
    let mut sats = vec![sat(0.5, rec709)];
    for &v in &specials {
        sats.push(sat(v, rec709));
        sats.push(sat(0.3, [v, 0.5, -v]));
    }
    // NaNs of both signs meeting in the products and the sums.
    let snan = f64::from_bits(0x7ff0_0000_0000_0001);
    for (s, luma) in [
        (f64::NAN, [-f64::NAN, snan, f64::NAN]),
        (-f64::NAN, [f64::NAN, -snan, 0.5]),
        (snan, [-f64::NAN, f64::NAN, -snan]),
    ] {
        sats.push(sat(s, luma));
    }
    out.extend(sats);

    let scale = |scale: [f64; 4]| Static {
        label: format!("Scale {scale:?}"),
        factory: json!(["Scale", f64s(&scale)]),
        port: Ok(MatrixTransform::scale(&scale)),
    };
    // tests/python/MatrixTransformTest.py:42 @ v2.5.2.
    out.push(scale([0.9, 0.8, 0.7, 1.]));
    for start in (0..specials.len()).step_by(4) {
        out.push(scale(
            [0, 1, 2, 3].map(|i| specials[(start + i) % specials.len()]),
        ));
    }

    let view = |hot: [i32; 4], luma: [f64; 3]| Static {
        label: format!("View {hot:?} {luma:?}"),
        factory: json!(["View", hot, f64s(&luma)]),
        port: Ok(MatrixTransform::view(&hot, &luma)),
    };
    // Every channel combination; non-zero values other than 1 count as hot.
    for bits in 0..16 {
        let hot = [0, 1, 2, 3].map(|i| (bits >> i) & 1);
        out.push(view(hot, rec709));
    }
    out.push(view([2, -1, 0, 0], rec709));
    out.push(view([0, 7, 0, -3], rec709));
    // Sums of 0, and about 0 in float precision: not normalized.
    out.push(view([1, 1, 1, 0], [1.0, -1.0, 0.0]));
    out.push(view([1, 1, 0, 0], [1e-9, 0.0, 0.5]));
    out.push(view([1, 0, 1, 0], [-0.0, 0.5, -0.0]));
    // Sums that are 0 only once converted to float, and ones just past it.
    for tiny in [1e-300, -1e-310, 1.4e-45, 2.9e-45, 4.3e-45, 1e-44, -1e-44] {
        out.push(view([1, 1, 0, 0], [tiny, 0.0, 0.5]));
    }
    // NaNs of both signs meeting in the sum, in every order (I-74), and other specials.
    let (nan, neg_nan) = (f64::NAN, -f64::NAN);
    for luma in [
        [nan, neg_nan, 0.5],
        [neg_nan, nan, 0.5],
        [0.5, nan, neg_nan],
        [0.5, neg_nan, nan],
        [nan, 0.5, neg_nan],
        [neg_nan, 0.5, nan],
        [f64::from_bits(0x7ff0_0000_0000_0001), neg_nan, nan],
    ] {
        out.push(view([1, 1, 1, 0], luma));
        out.push(view([1, 0, 1, 0], luma));
    }
    for (k, &v) in specials.iter().enumerate() {
        out.push(view(
            [1, 1, 1, 0],
            [v, specials[(k + 7) % specials.len()], 0.25],
        ));
        out.push(view([0, 1, 1, 0], [0.25, v, -v]));
    }
    out
}

#[test]
fn static_functions_match_the_wheel() {
    let statics = statics();
    let requests: Vec<ProcessorOpsRequest> = statics
        .iter()
        .map(|s| {
            let mut request = ProcessorOpsRequest::new(
                json!({"transform": {"class": "MatrixTransform", "factory": s.factory}}),
            );
            request.optimization = Some(json!("OPTIMIZATION_NONE"));
            request
        })
        .collect();
    let calls: Vec<BatchCall<'_>> = requests.iter().map(ProcessorOpsRequest::call).collect();
    let mut failures = Vec::new();
    let bits = |values: &[f64]| values.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
    for (s, response) in statics.iter().zip(Oracle::get().batch(&calls, true)) {
        let reply = ProcessorOpsReply::from_response(response.unwrap_or_else(|e| panic!("{e}")));
        match (reply.raised(), &s.port) {
            (Some(raised), Err(e))
                if raised.stage == "transform" && raised.message == e.message() => {}
            (None, Ok((m44, offset4))) => {
                let child = &reply.processor().group.children[0];
                let (wheel_m44, wheel_offset4) = (
                    child.getter("getMatrix").f64s(),
                    child.getter("getOffset").f64s(),
                );
                if bits(&wheel_m44) != bits(m44) || bits(&wheel_offset4) != bits(offset4) {
                    failures.push(format!(
                        "{}:\n  wheel {:x?} {:x?}\n  port  {:x?} {:x?}",
                        s.label,
                        bits(&wheel_m44),
                        bits(&wheel_offset4),
                        bits(m44),
                        bits(offset4)
                    ));
                }
            }
            (raised, port) => failures.push(format!(
                "{}:\n  wheel {raised:?}\n  port  {port:?}",
                s.label
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
