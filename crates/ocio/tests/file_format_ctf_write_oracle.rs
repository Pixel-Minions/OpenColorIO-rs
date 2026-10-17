// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CLF/CTF writer against the wheel (`write_transform`): `GroupTransform::write` of groups
//! as CTF and CLF, byte for byte, or its error. The operators come with their writers (WP
//! 4.6a-4.6d): matrices of every form, bit depth and value; the ProcessList's version, id,
//! name, inverseOf, descriptions, descriptors and Info; and the processors' groups of read
//! files.

mod common;

use common::lut_files::{file, write_files};
use common::transforms::{BIT_DEPTHS, Case, bit_depth_spec, direction_name, direction_spec};
use ocio::TransformDirection::{self, Forward, Inverse};
use ocio::{BitDepth, Config, FileTransform, GroupTransform, MatrixTransform, Transform};
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
    port: GroupTransform,
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
        port,
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
        .and_then(|p| p.create_group_transform())
        .expect("the processor's group");
    WriteCase {
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
    let config = Config::create_raw().unwrap();
    let mut failures = Vec::new();
    for (case, response) in cases.iter().zip(Oracle::get().batch(&calls, true)) {
        let response = response.unwrap_or_else(|e| panic!("{}: {e}", case.label));
        let result = &response.result;
        let format = case.request["format"].as_str().unwrap();
        let mut written = Vec::new();
        let port = case.port.write(&config, format, &mut written);
        match (result.get("exception"), port) {
            (Some(exception), Err(e)) => {
                let message = exception["message"].as_str().unwrap();
                if result["stage"] != "write" || message.as_bytes() != e.what() {
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
