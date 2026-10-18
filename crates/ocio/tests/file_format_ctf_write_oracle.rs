// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CLF/CTF writer against the wheel (`write_transform`): `GroupTransform::write` of groups
//! as CTF and CLF, byte for byte, or its error. The operators come with their writers (WP
//! 4.6a-4.6d): matrices of every form, bit depth and value; the ProcessList's version, id,
//! name, inverseOf, descriptions, descriptors and Info; CDLs, gammas and exponents (a version 1
//! config's Exponent ops too), logs and ranges of each style, direction and bit depth; 1D and 3D
//! LUTs of each bit depth, interpolation and direction; FixedFunctions of each style; and the
//! processors' groups of read files. The GradingRGBCurve writer's cases are
//! `src/fileformats/file_format_ctf_write_oracle_tests.rs`.

mod common;

use common::lut_files::{file, write_files};
use common::transforms::{
    BIT_DEPTHS, Case, bit_depth_spec, direction_name, direction_spec, fixed_function_style_name,
    interpolation_name, negative_style_spec,
};
use ocio::TransformDirection::{self, Forward, Inverse};
use ocio::{
    BitDepth, CdlStyle, CdlTransform, ColorSpaceTransform, Config, ExponentTransform,
    ExponentWithLinearTransform, FileTransform, FixedFunctionStyle, FixedFunctionTransform,
    GroupTransform, Interpolation, LogAffineTransform, LogCameraTransform, LogTransform,
    Lut1DHueAdjust, Lut1DTransform, Lut3DTransform, MatrixTransform, NegativeStyle, RangeStyle,
    RangeTransform, Transform,
};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values;
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

/// The CTF and CLF format names, also in another case, and a format that doesn't exist.
const CTF: &str = "Color Transform Format";
const CLF: &str = "Academy/ASC Common LUT Format";

/// The JSON of `values`.
fn f64s(values: &[f64]) -> Value {
    Value::Array(values.iter().map(|&v| f64_spec(v)).collect())
}

/// A matrix transform's case.
fn matrix(
    label: &str,
    m44: [f64; 16],
    offset4: [f64; 4],
    dir: TransformDirection,
    depths: (BitDepth, BitDepth),
) -> Case {
    let mut port = MatrixTransform::new();
    port.set_matrix(&m44);
    port.set_offset(&offset4);
    port.set_direction(dir);
    port.set_file_input_bit_depth(depths.0);
    port.set_file_output_bit_depth(depths.1);
    let spec = json!({"class": "MatrixTransform", "calls": [
        ["setMatrix", f64s(&m44)],
        ["setOffset", f64s(&offset4)],
        ["setDirection", direction_spec(dir)],
        ["setFileInputBitDepth", bit_depth_spec(depths.0)],
        ["setFileOutputBitDepth", bit_depth_spec(depths.1)],
    ]});
    Case::new(label, spec, port)
}

/// The identity matrix.
const IDENTITY: [f64; 16] = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];

/// A write of a group to the wheel and the port.
struct WriteCase {
    label: String,
    request: Value,
    /// The port's group, or the error of building the processor it comes from.
    port: ocio::Result<GroupTransform>,
    /// The config the port writes with, when not the raw config (the request's "config").
    config: Option<std::sync::Arc<Config>>,
}

/// The metadata calls the oracle and the port make on the group: `[method, args...]`.
type MetadataCall<'a> = (&'a str, &'a str, &'a str);

/// The write of a group of `cases` in `format`, after the metadata calls `metadata` on the
/// group (`__setitem__`, `addChildElement`, `setName` and `setID`).
fn group_case(
    label: &str,
    cases: &[Case],
    metadata: &[MetadataCall<'_>],
    format: &str,
) -> WriteCase {
    let mut port = GroupTransform::new();
    for case in cases {
        port.append_transform(case.port.clone());
    }
    let mut calls = Vec::new();
    for &(method, a, b) in metadata {
        let md = port.format_metadata_mut();
        match method {
            "__setitem__" => {
                md.add_attribute(Some(a.as_bytes()), Some(b.as_bytes()))
                    .unwrap();
                calls.push(json!(["__setitem__", a, b]));
            }
            "addChildElement" => {
                md.add_child_element(Some(a.as_bytes()), Some(b.as_bytes()))
                    .unwrap();
                calls.push(json!(["addChildElement", a, b]));
            }
            "setName" => {
                md.set_name(Some(a.as_bytes()));
                calls.push(json!(["setName", a]));
            }
            "setID" => {
                md.set_id(Some(a.as_bytes()));
                calls.push(json!(["setID", a]));
            }
            _ => unreachable!("{method}"),
        }
    }
    WriteCase {
        label: format!("{label} as {format}"),
        request: json!({
            "group": {"class": "GroupTransform",
                      "children": cases.iter().map(|c| c.spec.clone()).collect::<Vec<_>>()},
            "format": format,
            "metadata": calls,
        }),
        port: Ok(port),
        config: None,
    }
}

/// The write of the group of the raw config's processor of a file transform of `src` (an
/// upstream test file), in `dir`, in `format`.
fn processor_case(src: &str, dir: TransformDirection, format: &str) -> WriteCase {
    let path = ocio_testkit::paths::upstream_dir()
        .join("tests/data/files")
        .join(src);
    let path = path.to_str().unwrap().replace('\\', "/");
    let path = path.trim_start_matches("//?/").to_owned();
    path_case(src, &path, dir, format)
}

/// The write of the group of the raw config's processor of a file transform of `path`, in
/// `dir`, in `format`.
fn path_case(src: &str, path: &str, dir: TransformDirection, format: &str) -> WriteCase {
    let mut transform = FileTransform::new();
    transform.set_src(path);
    let config = Config::create_raw().unwrap();
    let port = config
        .processor_in_direction(&Transform::from(transform), dir)
        .and_then(|p| p.create_group_transform());
    WriteCase {
        config: None,
        label: format!("{src}, {dir:?}, as {format}"),
        request: json!({
            "processor": {
                "transform": {"class": "FileTransform", "calls": [["setSrc", path]]},
                "direction": direction_name(dir),
            },
            "format": format,
        }),
        port,
    }
}

/// Each case's write in the wheel against the port's: the text, or the error.
fn check_writes(cases: &[WriteCase]) {
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|c| BatchCall {
            cmd: "write_transform",
            args: c.request.clone(),
            blobs: Vec::new(),
        })
        .collect();
    let raw = Config::create_raw().unwrap();
    let mut failures = Vec::new();
    for (case, response) in cases.iter().zip(Oracle::get().batch(&calls, true)) {
        let config = case.config.as_ref().unwrap_or(&raw);
        let response = response.unwrap_or_else(|e| panic!("{}: {e}", case.label));
        let result = &response.result;
        let format = case.request["format"].as_str().unwrap();
        let mut written = Vec::new();
        let (stage, port) = match &case.port {
            Ok(group) => ("write", group.write(config, format, &mut written)),
            Err(e) => ("processor", Err(e.clone())),
        };
        match (result.get("exception"), port) {
            (Some(exception), Err(e)) => {
                let message = exception["message"].as_str().unwrap();
                if result["stage"] != stage || message.as_bytes() != e.what() {
                    failures.push(format!(
                        "{}: wheel {result}\n  port  {:?}",
                        case.label,
                        String::from_utf8_lossy(e.what())
                    ));
                }
            }
            (None, Ok(())) => {
                let wheel = oracle_values::bytes(&result["text"]);
                if wheel != written {
                    failures.push(format!(
                        "{}:\n  wheel {:?}\n  port  {:?}",
                        case.label,
                        String::from_utf8_lossy(&wheel),
                        String::from_utf8_lossy(&written)
                    ));
                }
            }
            (wheel, port) => {
                failures.push(format!("{}: wheel {wheel:?}, port {port:?}", case.label))
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The matrices: each form (3x3, offsets, alpha, both), inverses, every bit depth in and out,
/// values that need many digits, and special values.
fn matrix_cases() -> Vec<Case> {
    let f32s = (BitDepth::F32, BitDepth::F32);
    let upstream = [
        1.0, 1.01, 1.02, 1.03, 1.04, 1.05, 1.06, 1.07, 1.08, 1.09, 1.10, 1.11, 1.12, 1.13, 1.14,
        1.15,
    ];
    let rgb_only = [
        0.4123908, 0.35758434, 0.18048079, 0., 0.21263901, 0.71516868, 0.07219232, 0., 0.01933082,
        0.01191948, 0.95053215, 0., 0., 0., 0., 1.,
    ];
    let mut cases = vec![
        matrix("the identity", IDENTITY, [0.; 4], Forward, f32s),
        matrix("3x3", rgb_only, [0.; 4], Forward, f32s),
        matrix(
            "3x3 and offsets",
            rgb_only,
            [0.1, 1.2, 2.3456789123456, 0.],
            Forward,
            f32s,
        ),
        matrix(
            "alpha offset",
            rgb_only,
            [0.1, 0.2, 0.3, 0.9],
            Forward,
            f32s,
        ),
        matrix("4x4", upstream, [0.; 4], Forward, f32s),
        matrix(
            "4x4 and offsets",
            upstream,
            [1.0, 1.1, 1.2, 1.3],
            Forward,
            f32s,
        ),
        matrix(
            "4x4 and offsets, inverse",
            upstream,
            [1.0, 1.1, 1.2, 1.3],
            Inverse,
            f32s,
        ),
        matrix(
            "a diagonal, inverse",
            [
                2., 0., 0., 0., 0., 4., 0., 0., 0., 0., 8., 0., 0., 0., 0., 1.,
            ],
            [0.1, 0.2, 0.3, 1.0],
            Inverse,
            (BitDepth::F16, BitDepth::F32),
        ),
        matrix(
            "thirds",
            [
                1. / 3.,
                10. / 3.,
                100. / 3.,
                0.,
                3.,
                4.,
                5.,
                0.,
                6.,
                7.,
                8.,
                0.,
                0.,
                0.,
                0.,
                1.,
            ],
            [0.; 4],
            Forward,
            (BitDepth::Uint10, BitDepth::Uint10),
        ),
        matrix(
            "many digits and exponents",
            [
                0.1 + 0.2,
                1e-7,
                123_456_789.125,
                1e21,
                -0.0,
                2.5e-310,
                1e300,
                -1e-300,
                f64::MAX,
                f64::MIN_POSITIVE,
                -123.456,
                1e15,
                1e16,
                0.000123456789012345678,
                0.,
                1.,
            ],
            [1e-5, -1e-5, 1e17, 0.],
            Forward,
            f32s,
        ),
        matrix(
            "special values",
            [
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
                0.,
                -f64::NAN,
                1.,
                0.,
                0.,
                0.,
                0.,
                1.,
                0.,
                0.,
                0.,
                0.,
                1.,
            ],
            [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.],
            Forward,
            f32s,
        ),
    ];
    for &depth_in in &BIT_DEPTHS {
        for &depth_out in &[
            BitDepth::Unknown,
            BitDepth::Uint8,
            BitDepth::Uint12,
            BitDepth::F16,
            BitDepth::Uint32,
        ] {
            cases.push(matrix(
                &format!("bit depths {depth_in:?} to {depth_out:?}"),
                upstream,
                [0.01, 0.02, 0.03, 0.001],
                Forward,
                (depth_in, depth_out),
            ));
        }
    }
    cases
}

#[test]
fn matrices_write_as_in_the_wheel() {
    let mut cases = Vec::new();
    for case in matrix_cases() {
        for format in [CTF, CLF] {
            cases.push(group_case(
                &case.label,
                std::slice::from_ref(&case),
                &[],
                format,
            ));
        }
    }
    check_writes(&cases);
}

#[test]
fn groups_of_matrices_write_as_in_the_wheel() {
    let m = matrix_cases();
    let pick = |i: usize| m[i].clone();
    // Bit depths that meet between the ops: each op's preferred file depths.
    let depths = |label: &str, a: (BitDepth, BitDepth), b: (BitDepth, BitDepth)| {
        vec![
            matrix(
                &format!("{label} first"),
                IDENTITY,
                [0.1, 0.2, 0.3, 0.],
                Forward,
                a,
            ),
            matrix(&format!("{label} second"), IDENTITY, [0.; 4], Forward, b),
        ]
    };
    let groups: Vec<(&str, Vec<Case>)> = vec![
        ("empty", Vec::new()),
        ("two matrices", vec![pick(1), pick(5)]),
        ("three matrices", vec![pick(2), pick(6), pick(8)]),
        (
            "10i to 12i, then 12i to 8i",
            depths(
                "a",
                (BitDepth::Uint10, BitDepth::Uint12),
                (BitDepth::Uint12, BitDepth::Uint8),
            ),
        ),
        (
            "10i to unknown, then 16i to 16f",
            depths(
                "b",
                (BitDepth::Uint10, BitDepth::Unknown),
                (BitDepth::Uint16, BitDepth::F16),
            ),
        ),
        (
            "unknown to 8i, then unknown to unknown",
            depths(
                "c",
                (BitDepth::Unknown, BitDepth::Uint8),
                (BitDepth::Unknown, BitDepth::Unknown),
            ),
        ),
    ];
    let metadata: &[(&str, &[MetadataCall<'_>])] = &[
        ("no metadata", &[]),
        ("an id", &[("setID", "UID42", "")]),
        (
            "an id, a name and inverseOf",
            &[
                ("__setitem__", "id", "a < b & \"c\""),
                ("setName", "the 'group'", ""),
                ("__setitem__", "inverseOf", "inv"),
                ("__setitem__", "Unknown", "not saved"),
            ],
        ),
        (
            "descriptions and descriptors",
            &[
                ("addChildElement", "Description", "first & <one>"),
                ("addChildElement", "Description", "second"),
                ("addChildElement", "InputDescriptor", "in"),
                ("addChildElement", "OutputDescriptor", "out \"quoted\""),
                ("addChildElement", "InputDescriptor", "in, again"),
                ("addChildElement", "Unknown", "not saved"),
            ],
        ),
        (
            "Info",
            &[
                ("addChildElement", "Info", "Preserved"),
                ("addChildElement", "Info", ""),
            ],
        ),
    ];
    let mut cases = Vec::new();
    for (label, children) in &groups {
        for (md_label, md) in metadata {
            for format in [
                CTF,
                CLF,
                "color transform format",
                "academy/asc common lut format",
                "flame",
                "Not a format",
            ] {
                cases.push(group_case(
                    &format!("{label}, {md_label}"),
                    children,
                    md,
                    format,
                ));
            }
        }
    }
    check_writes(&cases);
}

#[test]
fn processors_of_matrix_files_write_as_in_the_wheel() {
    let mut cases = Vec::new();
    for src in [
        "clf/matrix_example.clf",
        "clf/matrix_3x4_example.clf",
        "matrix_example4x4.ctf",
        "matrix_offsets_example.ctf",
        "clf/info_example.clf",
    ] {
        for dir in [Forward, Inverse] {
            for format in [CTF, CLF] {
                cases.push(processor_case(src, dir, format));
            }
        }
    }
    check_writes(&cases);
}

#[test]
fn info_metadata_writes_as_in_the_wheel() {
    let matrix = "    <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\">\n\
                  \x20       <Array dim=\"3 3\">2 0 0 0 3 0 0 0 4</Array>\n    </Matrix>\n";
    let mut cases = Vec::new();
    for (label, info) in [
        ("Info with an attribute only", "    <Info a=\"1\"/>\n"),
        (
            "Info with attributes and a value",
            "    <Info a=\"1\" b=\"&amp;\">text</Info>\n",
        ),
        ("an empty Info", "    <Info/>\n"),
        ("Info with a value only", "    <Info>value</Info>\n"),
        (
            "Info with nested children",
            "    <Info c=\"x\">top<Child d=\"y\"><Inner>deep</Inner></Child><Empty/></Info>\n",
        ),
    ] {
        let text = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <ProcessList id=\"a\" version=\"2.0\">\n{info}{matrix}</ProcessList>\n"
        );
        let dir = write_files(&[file("info.ctf", text.into_bytes())]);
        for format in [CTF, CLF] {
            cases.push(path_case(
                label,
                &format!("{dir}/info.ctf"),
                Forward,
                format,
            ));
        }
    }
    check_writes(&cases);
}

/// A CDL transform's case.
fn cdl(
    label: &str,
    sop: [[f64; 3]; 3],
    sat: f64,
    style: CdlStyle,
    dir: TransformDirection,
) -> Case {
    let mut port = CdlTransform::new();
    port.set_slope(&sop[0]);
    port.set_offset(&sop[1]);
    port.set_power(&sop[2]);
    port.set_sat(sat);
    port.set_style(style);
    port.set_direction(dir);
    let style_name = match style {
        CdlStyle::Asc => "CDL_ASC",
        CdlStyle::NoClamp => "CDL_NO_CLAMP",
    };
    let spec = json!({"class": "CDLTransform", "calls": [
        ["setSlope", f64s(&sop[0])],
        ["setOffset", f64s(&sop[1])],
        ["setPower", f64s(&sop[2])],
        ["setSat", f64_spec(sat)],
        ["setStyle", {"enum": style_name}],
        ["setDirection", direction_spec(dir)],
    ]});
    Case::new(label, spec, port)
}

/// An exponent transform's case.
fn exponent(
    label: &str,
    value: [f64; 4],
    negative: NegativeStyle,
    dir: TransformDirection,
) -> Case {
    let mut port = ExponentTransform::new();
    port.set_value(&value);
    port.set_negative_style(negative).unwrap();
    port.set_direction(dir);
    let spec = json!({"class": "ExponentTransform", "calls": [
        ["setValue", f64s(&value)],
        ["setNegativeStyle", negative_style_spec(negative)],
        ["setDirection", direction_spec(dir)],
    ]});
    Case::new(label, spec, port)
}

/// An exponent with linear transform's case.
fn exponent_linear(
    label: &str,
    gamma: [f64; 4],
    offset: [f64; 4],
    negative: NegativeStyle,
    dir: TransformDirection,
) -> Case {
    let mut port = ExponentWithLinearTransform::new();
    port.set_gamma(&gamma);
    port.set_offset(&offset);
    port.set_negative_style(negative).unwrap();
    port.set_direction(dir);
    let spec = json!({"class": "ExponentWithLinearTransform", "calls": [
        ["setGamma", f64s(&gamma)],
        ["setOffset", f64s(&offset)],
        ["setNegativeStyle", negative_style_spec(negative)],
        ["setDirection", direction_spec(dir)],
    ]});
    Case::new(label, spec, port)
}

/// A log transform's case.
fn log(label: &str, base: f64, dir: TransformDirection) -> Case {
    let mut port = LogTransform::new();
    port.set_base(base);
    port.set_direction(dir);
    let spec = json!({"class": "LogTransform", "calls": [
        ["setBase", f64_spec(base)],
        ["setDirection", direction_spec(dir)],
    ]});
    Case::new(label, spec, port)
}

/// A log affine transform's case: the base, then the log side's slope and offset and the
/// linear side's.
fn log_affine(label: &str, base: f64, params: [[f64; 3]; 4], dir: TransformDirection) -> Case {
    let mut port = LogAffineTransform::new();
    port.set_base(base);
    port.set_log_side_slope_value(&params[0]);
    port.set_log_side_offset_value(&params[1]);
    port.set_lin_side_slope_value(&params[2]);
    port.set_lin_side_offset_value(&params[3]);
    port.set_direction(dir);
    let spec = json!({"class": "LogAffineTransform", "calls": [
        ["setBase", f64_spec(base)],
        ["setLogSideSlopeValue", f64s(&params[0])],
        ["setLogSideOffsetValue", f64s(&params[1])],
        ["setLinSideSlopeValue", f64s(&params[2])],
        ["setLinSideOffsetValue", f64s(&params[3])],
        ["setDirection", direction_spec(dir)],
    ]});
    Case::new(label, spec, port)
}

/// A log camera transform's case: as [`log_affine`], with the linear side's break and, when
/// given, the linear slope.
fn log_camera(
    label: &str,
    base: f64,
    params: [[f64; 3]; 4],
    lin_break: [f64; 3],
    linear_slope: Option<[f64; 3]>,
    dir: TransformDirection,
) -> Case {
    let mut port = LogCameraTransform::new(&lin_break);
    port.set_base(base);
    port.set_log_side_slope_value(&params[0]);
    port.set_log_side_offset_value(&params[1]);
    port.set_lin_side_slope_value(&params[2]);
    port.set_lin_side_offset_value(&params[3]);
    port.set_direction(dir);
    let mut calls = vec![
        json!(["setBase", f64_spec(base)]),
        json!(["setLogSideSlopeValue", f64s(&params[0])]),
        json!(["setLogSideOffsetValue", f64s(&params[1])]),
        json!(["setLinSideSlopeValue", f64s(&params[2])]),
        json!(["setLinSideOffsetValue", f64s(&params[3])]),
        json!(["setDirection", direction_spec(dir)]),
    ];
    if let Some(slope) = linear_slope {
        port.set_linear_slope_value(&slope).unwrap();
        calls.push(json!(["setLinearSlopeValue", f64s(&slope)]));
    }
    let spec = json!({"class": "LogCameraTransform", "args": [f64s(&lin_break)], "calls": calls});
    Case::new(label, spec, port)
}

/// A setter of a range transform's bound.
type RangeSetter = fn(&mut RangeTransform, f64);

/// A range transform's case: the bounds that are given (min in, max in, min out, max out).
fn range(
    label: &str,
    bounds: [Option<f64>; 4],
    style: RangeStyle,
    depths: (BitDepth, BitDepth),
    dir: TransformDirection,
) -> Case {
    let mut port = RangeTransform::new();
    port.set_style(style);
    port.set_file_input_bit_depth(depths.0);
    port.set_file_output_bit_depth(depths.1);
    let style_name = match style {
        RangeStyle::NoClamp => "RANGE_NO_CLAMP",
        RangeStyle::Clamp => "RANGE_CLAMP",
    };
    let mut calls = vec![
        json!(["setStyle", {"enum": style_name}]),
        json!(["setFileInputBitDepth", bit_depth_spec(depths.0)]),
        json!(["setFileOutputBitDepth", bit_depth_spec(depths.1)]),
    ];
    let setters: [(&str, RangeSetter); 4] = [
        ("setMinInValue", RangeTransform::set_min_in_value),
        ("setMaxInValue", RangeTransform::set_max_in_value),
        ("setMinOutValue", RangeTransform::set_min_out_value),
        ("setMaxOutValue", RangeTransform::set_max_out_value),
    ];
    for (bound, (name, set)) in bounds.iter().zip(setters) {
        if let Some(value) = bound {
            set(&mut port, *value);
            calls.push(json!([name, f64_spec(*value)]));
        }
    }
    port.set_direction(dir);
    calls.push(json!(["setDirection", direction_spec(dir)]));
    Case::new(
        label,
        json!({"class": "RangeTransform", "calls": calls}),
        port,
    )
}

/// The log affine parameters of many digits.
const DIGITS_4: [[f64; 3]; 4] = [
    [1.0 / 3.0, 0.1 + 0.2, 123_456_789.125],
    [1e-7, -2.0 / 3.0, 1e15 + 0.3],
    [1e16, 0.000123456789012345678, 7.0 / 9.0],
    [0.0, -0.0, 1e-300],
];

/// The CDL, gamma, exponent, log and range transforms: each style and direction, values that
/// need many digits, and special values.
fn op_cases_b() -> Vec<Case> {
    let f32s = (BitDepth::F32, BitDepth::F32);
    let sop = [[1.1, 1.2, 1.3], [0.1, 0.2, 0.3], [0.9, 1.0, 1.25]];
    let digits = [
        [1.0 / 3.0, 0.1 + 0.2, 123_456_789.125],
        [1e-7, -2.0 / 3.0, 1e15 + 0.3],
        [1e16, 0.000123456789012345678, 2.5e-310],
    ];
    let affine = [
        [0.5, 0.6, 0.7],
        [0.1, 0.2, 0.3],
        [1.1, 1.2, 1.3],
        [0.01, 0.02, 0.03],
    ];
    let same = [[0.5; 3], [0.1; 3], [1.1; 3], [0.01; 3]];
    let mut cases = Vec::new();
    for (style, dir) in [
        (CdlStyle::Asc, Forward),
        (CdlStyle::Asc, Inverse),
        (CdlStyle::NoClamp, Forward),
        (CdlStyle::NoClamp, Inverse),
    ] {
        cases.push(cdl(
            &format!("a CDL, {style:?} {dir:?}"),
            sop,
            0.8,
            style,
            dir,
        ));
    }
    cases.push(cdl(
        "a CDL of many digits",
        digits,
        1.0 / 7.0,
        CdlStyle::Asc,
        Forward,
    ));
    cases.push(cdl(
        "a CDL of extreme values",
        [
            [1e300, f64::MIN_POSITIVE, 0.0],
            [-0.0, f64::INFINITY, -1e-300],
            [2.5e-310, 1e16, 1.0 / 3.0],
        ],
        1e-300,
        CdlStyle::Asc,
        Inverse,
    ));
    cases.push(cdl(
        "a CDL of NaNs",
        [
            [f64::NAN, f64::INFINITY, -0.0],
            [f64::NEG_INFINITY, f64::MAX, f64::MIN_POSITIVE],
            [1e300, -1e-300, 0.0],
        ],
        f64::NAN,
        CdlStyle::NoClamp,
        Forward,
    ));
    for negative in [
        NegativeStyle::Clamp,
        NegativeStyle::Mirror,
        NegativeStyle::PassThru,
    ] {
        for dir in [Forward, Inverse] {
            for (what, value) in [
                ("", [2.2, 2.2, 2.2, 1.0]),
                (" per channel", [2.2, 2.4, 2.6, 1.0]),
                (" with alpha", [2.2, 2.2, 2.2, 1.5]),
            ] {
                cases.push(exponent(
                    &format!("an exponent{what}, {negative:?} {dir:?}"),
                    value,
                    negative,
                    dir,
                ));
            }
        }
    }
    cases.push(exponent(
        "an exponent of many digits",
        [1.0 / 3.0, 0.1 + 0.2, 12.345_678_901_234_5, 1.0],
        NegativeStyle::Clamp,
        Forward,
    ));
    for negative in [NegativeStyle::Linear, NegativeStyle::Mirror] {
        for dir in [Forward, Inverse] {
            for (what, gamma, offset) in [
                ("", [2.4, 2.4, 2.4, 1.0], [0.055, 0.055, 0.055, 0.0]),
                (
                    " per channel",
                    [2.4, 2.2, 2.0, 1.0],
                    [0.055, 0.09, 0.1, 0.0],
                ),
                (
                    " and alpha",
                    [2.4, 2.4, 2.4, 1.8],
                    [0.055, 0.055, 0.055, 0.01],
                ),
            ] {
                cases.push(exponent_linear(
                    &format!("an exponent with linear{what}, {negative:?} {dir:?}"),
                    gamma,
                    offset,
                    negative,
                    dir,
                ));
            }
        }
    }
    for dir in [Forward, Inverse] {
        for base in [2.0, 10.0, std::f64::consts::E, 1.0 / 3.0] {
            cases.push(log(&format!("a log of base {base}, {dir:?}"), base, dir));
        }
        cases.push(log_affine(
            &format!("a log affine, {dir:?}"),
            10.0,
            affine,
            dir,
        ));
        cases.push(log_affine(
            &format!("a log affine of equal channels, {dir:?}"),
            2.0,
            same,
            dir,
        ));
        cases.push(log_camera(
            &format!("a log camera, {dir:?}"),
            10.0,
            affine,
            [0.1, 0.2, 0.3],
            None,
            dir,
        ));
        cases.push(log_camera(
            &format!("a log camera of equal channels and a linear slope, {dir:?}"),
            2.0,
            same,
            [0.1; 3],
            Some([1.5; 3]),
            dir,
        ));
        cases.push(log_camera(
            &format!("a log camera with linear slopes, {dir:?}"),
            2.0,
            affine,
            [0.1, 0.2, 0.3],
            Some([1.5, 1.6, 1.7]),
            dir,
        ));
    }
    cases.push(log_affine(
        "a log affine of many digits",
        1.0 / 3.0,
        DIGITS_4,
        Forward,
    ));
    let bounds = [Some(0.1), Some(0.9), Some(-0.1), Some(1.2)];
    for style in [RangeStyle::Clamp, RangeStyle::NoClamp] {
        for dir in [Forward, Inverse] {
            cases.push(range(
                &format!("a range, {style:?} {dir:?}"),
                bounds,
                style,
                f32s,
                dir,
            ));
        }
    }
    for (label, bounds) in [
        ("only a minimum", [Some(0.1), None, Some(0.1), None]),
        ("only a maximum", [None, Some(0.9), None, Some(0.9)]),
        (
            "different minimums only",
            [Some(0.1), None, Some(0.2), None],
        ),
        ("no bounds", [None; 4]),
        (
            "many digits",
            [
                Some(1.0 / 3.0),
                Some(123_456.789_012_345),
                Some(-1e-7),
                Some(1e15 + 0.3),
            ],
        ),
    ] {
        for dir in [Forward, Inverse] {
            cases.push(range(
                &format!("a range of {label}, {dir:?}"),
                bounds,
                RangeStyle::Clamp,
                f32s,
                dir,
            ));
        }
    }
    for &depth_in in &BIT_DEPTHS {
        for &depth_out in &[
            BitDepth::Unknown,
            BitDepth::Uint8,
            BitDepth::Uint12,
            BitDepth::F16,
        ] {
            cases.push(range(
                &format!("a range from {depth_in:?} to {depth_out:?}"),
                [Some(0.05), Some(0.95), Some(0.0), Some(1.0)],
                RangeStyle::Clamp,
                (depth_in, depth_out),
                Forward,
            ));
        }
    }
    cases
}

/// The CDL, gamma, exponent, log and range transforms, alone in a group, as CTF and CLF.
#[test]
fn cdl_gamma_log_and_range_transforms_write_as_in_the_wheel() {
    let mut cases = Vec::new();
    for case in op_cases_b() {
        for format in [CTF, CLF] {
            cases.push(group_case(
                &case.label,
                std::slice::from_ref(&case),
                &[],
                format,
            ));
        }
    }
    check_writes(&cases);
}

/// Groups of these transforms with matrices: the file bit depths the ranges and matrices
/// carry from one op to the next, and the version the ops need.
#[test]
fn groups_of_cdl_gamma_log_and_range_transforms_write_as_in_the_wheel() {
    let b = op_cases_b();
    let find = |label: &str| {
        b.iter()
            .find(|c| c.label == label)
            .unwrap_or_else(|| panic!("{label}"))
            .clone()
    };
    let m = |label: &str, depths: (BitDepth, BitDepth)| {
        matrix(label, IDENTITY, [0.1, 0.2, 0.3, 0.], Forward, depths)
    };
    let groups: Vec<(&str, Vec<Case>)> = vec![
        (
            "a range from 10i to 12i, then a log",
            vec![
                find("a range from Uint10 to Uint12"),
                find("a log affine, Forward"),
            ],
        ),
        (
            "a matrix from 8i to 16i, then a range, then a CDL",
            vec![
                m("m", (BitDepth::Uint8, BitDepth::Uint16)),
                find("a range from Unknown to Uint12"),
                find("a CDL, Asc Forward"),
            ],
        ),
        (
            "an exponent, a range from 16f to 8i, an exponent with linear",
            vec![
                find("an exponent per channel, Clamp Forward"),
                find("a range from F16 to Uint8"),
                find("an exponent with linear, Linear Forward"),
            ],
        ),
        (
            "a mirrored exponent and a log camera",
            vec![
                find("an exponent, Mirror Inverse"),
                find("a log camera with linear slopes, Inverse"),
            ],
        ),
    ];
    let mut cases = Vec::new();
    for (label, children) in &groups {
        for format in [CTF, CLF] {
            cases.push(group_case(label, children, &[("setID", "UID", "")], format));
        }
    }
    check_writes(&cases);
}

/// The processors of upstream's CDL, gamma, exponent, log and range files, in both directions,
/// as CTF and CLF: their metadata (the CDL's SOP, Sat, input and viewing descriptions), the
/// ops' ids, names and descriptions, and the version the file needs.
#[test]
fn processors_of_cdl_gamma_log_and_range_files_write_as_in_the_wheel() {
    let mut cases = Vec::new();
    for src in [
        "clf/cdl_all_styles.clf",
        "clf/cdl_clamp_fwd.clf",
        "clf/cdl_missing_sat.clf",
        "clf/cdl_missing_sop.clf",
        "clf/cdl_missing_style.clf",
        "cdl_various.ctf",
        "clf/exponent_all_styles.clf",
        "gamma_alpha_test1.ctf",
        "gamma_alpha_test2.ctf",
        "gamma_alpha_test3.ctf",
        "gamma_alpha_test4.ctf",
        "gamma_alpha_test5.ctf",
        "gamma_alpha_test6.ctf",
        "gamma_comp_test.ctf",
        "gamma_comp_test2.ctf",
        "gamma_test1.ctf",
        "gamma_test2.ctf",
        "gamma_test3.ctf",
        "gamma_test4.ctf",
        "gamma_test5.ctf",
        "gamma_test6.ctf",
        "clf/log_all_styles.clf",
        "log_identities.ctf",
        "log_lintolog_3chan.ctf",
        "log_logtolin.ctf",
        "log_logtolinv2.ctf",
        "clf/range.clf",
        "clf/range_test1_clamp.clf",
        "clf/range_test1_noclamp.clf",
        "clf/range_test2.clf",
    ] {
        for dir in [Forward, Inverse] {
            for format in [CTF, CLF] {
                cases.push(processor_case(src, dir, format));
            }
        }
    }
    check_writes(&cases);
}

/// A version 1 config: there, exponent and CDL transforms build Exponent ops
/// (`BuildExponentOp`, `BuildCDLOp`), which the writer writes as gammas.
const V1_CONFIG: &str = "ocio_profile_version: 1
roles: {default: raw}
colorspaces:
  - !<ColorSpace> {name: raw}
";

/// The exponents and CDLs of a version 1 config, alone and in groups: Exponent ops, written as
/// gammas, or refused in CLF when the alpha exponent isn't 1.
#[test]
fn exponents_of_a_version_1_config_write_as_in_the_wheel() {
    let config = Config::create_from_stream(V1_CONFIG.as_bytes()).unwrap();
    let b = op_cases_b();
    let find = |label: &str| {
        b.iter()
            .find(|c| c.label == label)
            .unwrap_or_else(|| panic!("{label}"))
            .clone()
    };
    let mut groups: Vec<(String, Vec<Case>)> = Vec::new();
    for label in [
        "an exponent, Clamp Forward",
        "an exponent, Mirror Inverse",
        "an exponent per channel, PassThru Forward",
        "an exponent per channel, Clamp Inverse",
        "an exponent with alpha, Clamp Forward",
        "an exponent with alpha, Clamp Inverse",
        "an exponent of many digits",
        "a CDL, Asc Forward",
        "a CDL, NoClamp Inverse",
    ] {
        groups.push((label.to_owned(), vec![find(label)]));
    }
    groups.push((
        "a range from 10i to 12i, then an exponent".to_owned(),
        vec![
            find("a range from Uint10 to Uint12"),
            find("an exponent per channel, Clamp Forward"),
        ],
    ));
    let mut cases = Vec::new();
    for (label, children) in &groups {
        for format in [CTF, CLF] {
            let mut case = group_case(
                &format!("{label} in a version 1 config"),
                children,
                &[],
                format,
            );
            case.request["config"] = json!({"yaml": V1_CONFIG});
            case.config = Some(config.clone());
            cases.push(case);
        }
    }

    // Named and described exponents of a version 1 config's color spaces: the Exponent op
    // keeps the transform's metadata (`BuildExponentOp`), which the writer copies into the
    // gamma it writes. The group is the processor's, from each color space to `raw`, written
    // with the version 1 config.
    let named = "ocio_profile_version: 1
roles: {default: raw}
colorspaces:
  - !<ColorSpace> {name: raw}
  - !<ColorSpace>
    name: exp
    to_reference: !<ExponentTransform> {value: [2, 2.2, 2.4, 1], name: \"exp <1> & q\"}
  - !<ColorSpace>
    name: exp_alpha
    to_reference: !<ExponentTransform> {value: [2, 2, 2, 1.5], name: expalpha}
";
    let named_config = Config::create_from_stream(named.as_bytes()).unwrap();
    for src in ["exp", "exp_alpha"] {
        for dir in [Forward, Inverse] {
            for format in [CTF, CLF] {
                let mut transform = ColorSpaceTransform::new();
                transform.set_src(src);
                transform.set_dst("raw");
                let port = named_config
                    .processor_in_direction(&Transform::from(transform), dir)
                    .and_then(|p| p.create_group_transform());
                cases.push(WriteCase {
                    label: format!("{src} to raw in a version 1 config, {dir:?}, as {format}"),
                    request: json!({
                        "processor": {
                            "config": {"yaml": named},
                            "transform": {"class": "ColorSpaceTransform",
                                          "calls": [["setSrc", src], ["setDst", "raw"]]},
                            "direction": direction_name(dir),
                        },
                        "config": {"yaml": named},
                        "format": format,
                    }),
                    port,
                    config: Some(named_config.clone()),
                });
            }
        }
    }
    check_writes(&cases);
}

/// The values the LUT cases cycle through: fractions of the bit depths' ranges, values that
/// need many digits, values outside [0, 1], and special values.
const LUT_VALUES: [f32; 24] = [
    0.0,
    1.0,
    0.5,
    0.25,
    1.0 / 3.0,
    2.0 / 3.0,
    511.0 / 1023.0,
    511.5 / 1023.0,
    4011.123 / 1023.0,
    -24.10297 / 1023.0,
    0.1,
    0.000_123_456_79,
    1.0e-7,
    -0.5,
    1.5,
    123_456.79,
    -0.0,
    0.999_999_9,
    f32::MIN_POSITIVE,
    f32::MAX,
    f32::INFINITY,
    f32::NEG_INFINITY,
    f32::NAN,
    65504.0,
];

/// The `i`th value of the cycle starting at `start`.
fn lut_value(start: usize, i: usize) -> f32 {
    LUT_VALUES[(start + i) % LUT_VALUES.len()]
}

/// The settings of a LUT case.
#[derive(Clone, Copy)]
struct LutSettings {
    /// The file output bit depth.
    depth: BitDepth,
    /// The interpolation.
    interpolation: Interpolation,
    /// The direction.
    dir: TransformDirection,
    /// Where the values start in [`LUT_VALUES`], or none for equal channels from the start.
    start: usize,
    /// Whether the channels are equal (a 1D LUT written with one component).
    equal: bool,
}

/// A 1D LUT transform's case of `length` entries, with the hue adjust and raw halfs given.
fn lut1d(
    label: &str,
    length: usize,
    settings: LutSettings,
    hue_dw3: bool,
    raw_halfs: bool,
) -> Case {
    let mut port = Lut1DTransform::new();
    port.set_length(length as std::ffi::c_ulong).unwrap();
    let mut calls = vec![json!(["setLength", length])];
    for i in 0..length {
        let rgb = if settings.equal {
            [lut_value(settings.start, i); 3]
        } else {
            [
                lut_value(settings.start, 3 * i),
                lut_value(settings.start, 3 * i + 1),
                lut_value(settings.start, 3 * i + 2),
            ]
        };
        port.set_value(i as std::ffi::c_ulong, rgb[0], rgb[1], rgb[2])
            .unwrap();
        calls.push(json!([
            "setValue",
            i,
            f64_spec(f64::from(rgb[0])),
            f64_spec(f64::from(rgb[1])),
            f64_spec(f64::from(rgb[2]))
        ]));
    }
    port.set_file_output_bit_depth(settings.depth);
    port.set_interpolation(settings.interpolation);
    port.set_direction(settings.dir);
    calls.extend([
        json!(["setFileOutputBitDepth", bit_depth_spec(settings.depth)]),
        json!(["setInterpolation", {"enum": interpolation_name(settings.interpolation)}]),
        json!(["setDirection", direction_spec(settings.dir)]),
    ]);
    if hue_dw3 {
        port.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
        calls.push(json!(["setHueAdjust", {"enum": "HUE_DW3"}]));
    }
    if raw_halfs {
        port.set_output_raw_halfs(true);
        calls.push(json!(["setOutputRawHalfs", true]));
    }
    Case::new(
        label,
        json!({"class": "Lut1DTransform", "calls": calls}),
        port,
    )
}

/// A 3D LUT transform's case of a grid of `size`.
fn lut3d(label: &str, size: usize, settings: LutSettings) -> Case {
    let mut port = Lut3DTransform::new();
    port.set_grid_size(size as std::ffi::c_ulong).unwrap();
    let mut calls = vec![json!(["setGridSize", size])];
    let mut i = 0;
    for r in 0..size {
        for g in 0..size {
            for b in 0..size {
                let rgb = [
                    lut_value(settings.start, 3 * i),
                    lut_value(settings.start, 3 * i + 1),
                    lut_value(settings.start, 3 * i + 2),
                ];
                port.set_value(
                    r as std::ffi::c_ulong,
                    g as std::ffi::c_ulong,
                    b as std::ffi::c_ulong,
                    rgb[0],
                    rgb[1],
                    rgb[2],
                )
                .unwrap();
                calls.push(json!([
                    "setValue",
                    r,
                    g,
                    b,
                    f64_spec(f64::from(rgb[0])),
                    f64_spec(f64::from(rgb[1])),
                    f64_spec(f64::from(rgb[2]))
                ]));
                i += 1;
            }
        }
    }
    port.set_file_output_bit_depth(settings.depth);
    port.set_interpolation(settings.interpolation);
    port.set_direction(settings.dir);
    calls.extend([
        json!(["setFileOutputBitDepth", bit_depth_spec(settings.depth)]),
        json!(["setInterpolation", {"enum": interpolation_name(settings.interpolation)}]),
        json!(["setDirection", direction_spec(settings.dir)]),
    ]);
    Case::new(
        label,
        json!({"class": "Lut3DTransform", "calls": calls}),
        port,
    )
}

/// The 1D and 3D LUTs: every file output bit depth and interpolation, both directions, equal
/// and different channels, hue adjust, raw halfs, and the values of [`LUT_VALUES`] (special
/// values included) from several starts.
fn lut_cases() -> Vec<Case> {
    let base = LutSettings {
        depth: BitDepth::F32,
        interpolation: Interpolation::Default,
        dir: Forward,
        start: 0,
        equal: false,
    };
    let mut cases = Vec::new();
    for &depth in &BIT_DEPTHS {
        for dir in [Forward, Inverse] {
            for equal in [false, true] {
                for start in [0, 7] {
                    let settings = LutSettings {
                        depth,
                        dir,
                        equal,
                        start,
                        ..base
                    };
                    cases.push(lut1d(
                        &format!("a 1D LUT, {depth:?} {dir:?}, equal {equal}, from {start}"),
                        9,
                        settings,
                        false,
                        false,
                    ));
                }
            }
            cases.push(lut3d(
                &format!("a 3D LUT, {depth:?} {dir:?}"),
                3,
                LutSettings { depth, dir, ..base },
            ));
            cases.push(lut3d(
                &format!("a 3D LUT of 2, {depth:?} {dir:?}, from 5"),
                2,
                LutSettings {
                    depth,
                    dir,
                    start: 5,
                    ..base
                },
            ));
        }
    }
    for interpolation in [
        Interpolation::Default,
        Interpolation::Linear,
        Interpolation::Best,
        Interpolation::Nearest,
        Interpolation::Cubic,
        Interpolation::Tetrahedral,
    ] {
        for dir in [Forward, Inverse] {
            let settings = LutSettings {
                interpolation,
                dir,
                ..base
            };
            cases.push(lut1d(
                &format!("a 1D LUT, {interpolation:?} {dir:?}"),
                3,
                settings,
                false,
                false,
            ));
            cases.push(lut3d(
                &format!("a 3D LUT, {interpolation:?} {dir:?}"),
                2,
                settings,
            ));
        }
    }
    for dir in [Forward, Inverse] {
        cases.push(lut1d(
            &format!("a 1D LUT with hue adjust, {dir:?}"),
            5,
            LutSettings { dir, ..base },
            true,
            false,
        ));
        for depth in [BitDepth::F16, BitDepth::F32, BitDepth::Uint10] {
            for equal in [false, true] {
                cases.push(lut1d(
                    &format!("a 1D LUT of raw halfs, {depth:?} {dir:?}, equal {equal}"),
                    9,
                    LutSettings {
                        depth,
                        dir,
                        equal,
                        ..base
                    },
                    false,
                    true,
                ));
            }
        }
    }
    cases
}

/// The 1D and 3D LUT transforms, alone in a group, as CTF and CLF.
#[test]
fn lut_transforms_write_as_in_the_wheel() {
    let mut cases = Vec::new();
    for case in lut_cases() {
        for format in [CTF, CLF] {
            cases.push(group_case(
                &case.label,
                std::slice::from_ref(&case),
                &[],
                format,
            ));
        }
    }
    check_writes(&cases);
}

/// Groups of LUTs with matrices and ranges: the file bit depths an inverse LUT takes from the
/// op before it, and those it hands on.
#[test]
fn groups_of_lut_transforms_write_as_in_the_wheel() {
    let l = lut_cases();
    let find = |label: &str| {
        l.iter()
            .find(|c| c.label == label)
            .unwrap_or_else(|| panic!("{label}"))
            .clone()
    };
    let m = |label: &str, depths: (BitDepth, BitDepth)| {
        matrix(label, IDENTITY, [0.1, 0.2, 0.3, 0.], Forward, depths)
    };
    let groups: Vec<(&str, Vec<Case>)> = vec![
        (
            "a matrix to 12i, then an inverse 1D LUT of 10i",
            vec![
                m("m", (BitDepth::Uint8, BitDepth::Uint12)),
                find("a 1D LUT, Uint10 Inverse, equal false, from 0"),
            ],
        ),
        (
            "a 1D LUT of 16i, an inverse 3D LUT of 8i, a 1D LUT of 16f",
            vec![
                find("a 1D LUT, Uint16 Forward, equal true, from 7"),
                find("a 3D LUT, Uint8 Inverse"),
                find("a 1D LUT, F16 Forward, equal false, from 7"),
            ],
        ),
        (
            "two inverse 1D LUTs of 12i and 10i, then a 3D LUT of 10i",
            vec![
                find("a 1D LUT, Uint12 Inverse, equal true, from 0"),
                find("a 1D LUT, Uint10 Inverse, equal true, from 7"),
                find("a 3D LUT, Uint10 Forward"),
            ],
        ),
    ];
    let mut cases = Vec::new();
    for (label, children) in &groups {
        for format in [CTF, CLF] {
            cases.push(group_case(label, children, &[("setID", "UID", "")], format));
        }
    }
    check_writes(&cases);
}

/// The processors of upstream's 1D and 3D LUT files (half domains, raw halfs, hue adjust,
/// inverses, 10i and 12i arrays), in both directions, as CTF and CLF.
#[test]
fn processors_of_lut_files_write_as_in_the_wheel() {
    let mut cases = Vec::new();
    for src in [
        "clf/lut1d_32f_example.clf",
        "clf/lut1d_comp.clf",
        "clf/lut1d_example.clf",
        "clf/lut1d_half_domain_raw_half_set.clf",
        "clf/lut1d_long.clf",
        "clf/lut1d_lut3d_lut1d.clf",
        "clf/lut3d_17x17x17_10i_12i.clf",
        "clf/lut3d_as_matrix.clf",
        "clf/lut3d_bizarre.clf",
        "clf/lut3d_identity_12i_16f.clf",
        "clf/lut3d_preview_tier_test.clf",
        "lut1d_1024_hue_adjust_test.ctf",
        "lut1d_32_10i_10i.ctf",
        "lut1d_green.ctf",
        "lut1d_halfdom.ctf",
        "lut1d_hd_16f_16i_1chan.ctf",
        "lut1d_hd_hue_adjust.ctf",
        "lut1d_hue_adjust_test.ctf",
        "lut1d_identity_test.ctf",
        "lut1d_infinity.ctf",
        "lut1d_inv.ctf",
        "lut1d_inverse_gpu.ctf",
        "lut1d_inverse_halfdom_slog_fclut.ctf",
        "lut1d_inverse_hd_hueAdjust.ctf",
        "lut3d_example_Inv.ctf",
        "lut_inv_pairs.ctf",
    ] {
        for dir in [Forward, Inverse] {
            for format in [CTF, CLF] {
                cases.push(processor_case(src, dir, format));
            }
        }
    }
    check_writes(&cases);
}

/// Valid parameters of each FixedFunction style that takes some: upstream's test values
/// (tests/cpu/ops/fixedfunction/FixedFunctionOpData_tests.cpp:71, 169, 219-220, 270-271,
/// FixedFunctionOpCPU_tests.cpp:564-569, 773, 866 @ v2.5.2), as
/// `tests/fixed_function_transform_oracle.rs` takes them.
fn ff_valid_params(style: FixedFunctionStyle) -> Vec<f64> {
    use FixedFunctionStyle::*;
    match style {
        AcesGamutComp13 => vec![1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2],
        Rec2100Surround => vec![2.0],
        LinToDoubleLog => vec![
            10.0, 0.25, 0.5, -1.0, 0.0, -1.0, 1.25, 1.0, 1.0, 1.0, 0.5, 1.0, 0.0,
        ],
        LinToGammaLog => vec![
            0.0,
            0.25,
            0.5,
            1.0,
            0.0,
            2.718,
            0.17883277,
            0.807825590164,
            1.0,
            -0.07116723,
        ],
        AcesOutputTransform20 | AcesGamutCompress20 => vec![
            100.0, 0.6400, 0.3300, 0.3000, 0.6000, 0.1500, 0.0600, 0.3127, 0.3290,
        ],
        AcesRgbToJmh20 => vec![
            0.7347, 0.2653, 0.0000, 1.0000, 0.0001, -0.0770, 0.32168, 0.33767,
        ],
        AcesTonescaleCompress20 => vec![1000.0],
        _ => Vec::new(),
    }
}

/// Every FixedFunction style.
const FF_STYLES: [FixedFunctionStyle; 23] = {
    use FixedFunctionStyle::*;
    [
        AcesRedMod03,
        AcesRedMod10,
        AcesGlow03,
        AcesGlow10,
        AcesDarkToDim10,
        Rec2100Surround,
        RgbToHsv,
        XyzToXyy,
        XyzToUvy,
        XyzToLuv,
        AcesGamutMap02,
        AcesGamutMap07,
        AcesGamutComp13,
        LinToPq,
        LinToGammaLog,
        LinToDoubleLog,
        AcesOutputTransform20,
        AcesRgbToJmh20,
        AcesTonescaleCompress20,
        AcesGamutCompress20,
        RgbToHsyLin,
        RgbToHsyLog,
        RgbToHsyVid,
    ]
};

/// A FixedFunction transform's case: the binding's constructor of `ACES_GLOW_03`, then
/// `setStyle`, `setParams` and `setDirection`, none of which validates (the writer does).
/// None when the port refuses the style (one upstream doesn't implement).
fn fixed_function(
    label: &str,
    style: FixedFunctionStyle,
    params: &[f64],
    dir: TransformDirection,
) -> Option<Case> {
    let mut port = FixedFunctionTransform::new(FixedFunctionStyle::AcesGlow03, &[]).unwrap();
    port.set_style(style).ok()?;
    port.set_params(params);
    port.set_direction(dir);
    let style_spec = |s| json!({"enum": fixed_function_style_name(s)});
    Some(Case::new(
        label,
        json!({"class": "FixedFunctionTransform",
            "args": {"style": style_spec(FixedFunctionStyle::AcesGlow03)},
            "calls": [["setStyle", style_spec(style)], ["setParams", f64s(params)],
                ["setDirection", direction_spec(dir)]]}),
        port,
    ))
}

/// The FixedFunction transforms: every style in both directions with valid parameters, and
/// parameters that need many digits or are special values, which the writer validates.
fn ff_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for style in FF_STYLES {
        for dir in [Forward, Inverse] {
            cases.extend(fixed_function(
                &format!("{style:?} {dir:?}"),
                style,
                &ff_valid_params(style),
                dir,
            ));
        }
    }
    for (label, params) in [
        ("a surround of many digits", vec![1.0 / 3.0]),
        ("a surround of 1e-7", vec![1e-7]),
        ("a surround of 17 digits", vec![0.123_456_789_012_345_68]),
        ("a surround just under 100", vec![99.999_999_999_999_99]),
        ("a surround of 0.01", vec![0.01]),
        ("a surround of 123456.789012345", vec![123_456.789_012_345]),
        ("a NaN surround", vec![f64::NAN]),
        ("a negative NaN surround", vec![-f64::NAN]),
        ("an infinite surround", vec![f64::INFINITY]),
        ("two surrounds", vec![1.5, 2.5]),
    ] {
        cases.extend(fixed_function(
            label,
            FixedFunctionStyle::Rec2100Surround,
            &params,
            Forward,
        ));
    }
    let mut digits = ff_valid_params(FixedFunctionStyle::LinToGammaLog);
    digits[5] = std::f64::consts::E;
    digits[6] = 1.0 / 7.0;
    cases.extend(fixed_function(
        "a gamma log of many digits",
        FixedFunctionStyle::LinToGammaLog,
        &digits,
        Forward,
    ));
    cases
}

/// The FixedFunction transforms, alone in a group and after a matrix, as CTF and CLF (which
/// refuses them).
#[test]
fn fixed_function_transforms_write_as_in_the_wheel() {
    let mut cases = Vec::new();
    let m = matrix(
        "m",
        IDENTITY,
        [0.1, 0.2, 0.3, 0.],
        Forward,
        (BitDepth::Uint10, BitDepth::Uint12),
    );
    for case in ff_cases() {
        for format in [CTF, CLF] {
            cases.push(group_case(
                &case.label,
                std::slice::from_ref(&case),
                &[],
                format,
            ));
            cases.push(group_case(
                &format!("{}, after a matrix", case.label),
                &[m.clone(), case.clone()],
                &[("setID", "UID", "")],
                format,
            ));
        }
    }
    check_writes(&cases);
}

/// The processors of upstream's FixedFunction file, in both directions, as CTF and CLF.
#[test]
fn processors_of_fixed_function_files_write_as_in_the_wheel() {
    let mut cases = Vec::new();
    for dir in [Forward, Inverse] {
        for format in [CTF, CLF] {
            cases.push(processor_case("fixed_function.ctf", dir, format));
        }
    }
    check_writes(&cases);
}

/// The write formats' names and extensions, in order: the wheel's (`config_calls`:
/// `GroupTransform.GetWriteFormats()`, the binding's iterator over `GetNumWriteFormats`,
/// `GetFormatNameByIndex` and `GetFormatExtensionByIndex`) against the port's.
#[test]
fn write_formats_are_the_wheels() {
    let calls = vec![json!({"call": "GetWriteFormats", "on": "GroupTransform"})];
    let reply = Oracle::get()
        .call(
            "config_calls",
            json!({"config": "raw", "calls": calls}),
            &[],
        )
        .result;
    let result = &reply["calls"][0];
    let wheel: Vec<(Vec<u8>, Vec<u8>)> = result["result"]
        .as_array()
        .unwrap_or_else(|| panic!("{result}"))
        .iter()
        .map(|pair| {
            (
                oracle_values::bytes(&pair[0]),
                oracle_values::bytes(&pair[1]),
            )
        })
        .collect();
    let port: Vec<(Vec<u8>, Vec<u8>)> = (0..GroupTransform::num_write_formats())
        .map(|i| {
            (
                GroupTransform::format_name_by_index(i).to_vec(),
                GroupTransform::format_extension_by_index(i).to_vec(),
            )
        })
        .collect();
    assert_eq!(wheel, port);
}

/// NaN parameters, negative and with a payload, and infinities, in each writer: the CDL,
/// gamma, exponent, log and range writers stream a `double` as it is (a negative NaN is
/// `-nan(ind)` on Windows and `-nan` on Linux), while the FixedFunction params and the LUT
/// arrays write NaN through `WriteValue`, `nan` on both (docs/improvements.md, I-7). The cases
/// a writer's validation refuses compare their messages.
#[test]
fn nan_parameters_write_as_in_the_wheel() {
    let neg_nan = f64::from_bits(0xfff8_0000_0000_0000);
    let pay_nan = f64::from_bits(0x7ff8_0000_0000_1234);
    let mut ops = vec![
        cdl(
            "a CDL of NaNs and infinities",
            [
                [neg_nan, 1., 1.],
                [pay_nan, f64::NEG_INFINITY, 0.],
                [1., neg_nan, f64::INFINITY],
            ],
            neg_nan,
            CdlStyle::NoClamp,
            Forward,
        ),
        cdl(
            "a CDL of a negative NaN offset",
            [[1.0, 1.0, 1.0], [neg_nan, 0.0, 0.0], [1.0, 1.0, 1.0]],
            1.0,
            CdlStyle::NoClamp,
            Forward,
        ),
        exponent(
            "an exponent of NaNs",
            [neg_nan, pay_nan, f64::INFINITY, 1.0],
            NegativeStyle::PassThru,
            Forward,
        ),
        exponent(
            "an exponent of a NaN alpha",
            [2.0, 2.0, 2.0, neg_nan],
            NegativeStyle::Mirror,
            Forward,
        ),
        exponent_linear(
            "an exponent with linear of NaNs",
            [neg_nan, 2.4, 2.4, 1.],
            [pay_nan, 0.05, f64::NEG_INFINITY, 0.],
            NegativeStyle::Linear,
            Forward,
        ),
        log("a log of a NaN base", neg_nan, Forward),
        log("a log of an infinite base", f64::INFINITY, Inverse),
        log_affine(
            "a log affine of NaNs",
            10.0,
            [
                [neg_nan, 1., 1.],
                [pay_nan, 0., 0.],
                [1., f64::INFINITY, 1.],
                [0., 0., f64::NEG_INFINITY],
            ],
            Forward,
        ),
        range(
            "a range of NaNs",
            [Some(neg_nan), Some(1.0), Some(pay_nan), Some(f64::INFINITY)],
            RangeStyle::Clamp,
            (BitDepth::Uint10, BitDepth::F16),
            Forward,
        ),
        range(
            "a range of a negative NaN maximum",
            [Some(0.0), Some(neg_nan), Some(0.0), Some(neg_nan)],
            RangeStyle::NoClamp,
            (BitDepth::F32, BitDepth::F32),
            Forward,
        ),
        range(
            "a range of a negative NaN output minimum",
            [Some(0.0), Some(1.0), Some(neg_nan), Some(1.0)],
            RangeStyle::Clamp,
            (BitDepth::F32, BitDepth::F32),
            Forward,
        ),
        range(
            "a range of a minimum only",
            [Some(-0.0), None, Some(5e-324), None],
            RangeStyle::Clamp,
            (BitDepth::Uint16, BitDepth::Uint8),
            Forward,
        ),
    ];
    ops.extend(fixed_function(
        "a FixedFunction of a negative NaN power",
        FixedFunctionStyle::AcesGamutComp13,
        &[1.147, 1.264, 1.312, 0.815, 0.803, 0.880, neg_nan],
        Forward,
    ));
    ops.extend(fixed_function(
        "a FixedFunction of NaNs",
        FixedFunctionStyle::AcesGamutComp13,
        &[
            neg_nan,
            pay_nan,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -0.0,
            5e-324,
            1.0,
        ],
        Forward,
    ));
    // The NaNs by their bits on both sides: an `f32` to `f64` conversion of a NaN the
    // optimizer may fold (in release) doesn't keep the sign.
    let neg_nan_f32 = f32::from_bits(0xffc0_0000);
    let mut values = Lut1DTransform::new();
    values.set_length(2).unwrap();
    values
        .set_value(0, neg_nan_f32, f32::from_bits(0x7fc0_0000), f32::INFINITY)
        .unwrap();
    let spec = json!({"class": "Lut1DTransform", "calls": [
        ["setLength", 2],
        ["setValue", 0, {"f64": 0xfff8_0000_0000_0000u64}, {"f64": 0x7ff8_0000_0000_0000u64},
         f64_spec(f64::INFINITY)],
    ]});
    ops.push(Case::new("a 1D LUT of NaNs", spec, values));
    let mut cases = Vec::new();
    for case in &ops {
        for format in [CTF, CLF] {
            cases.push(group_case(
                &case.label,
                std::slice::from_ref(case),
                &[],
                format,
            ));
        }
    }
    check_writes(&cases);
}
