// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CC, CCC and CDL writers against the wheel (`write_transform`): `GroupTransform::write`
//! of groups of CDLs in each format, byte for byte, or its error: ids and names with special
//! characters, descriptions of every kind (escaped twice, I-173), the group's descriptions,
//! values of many forms, the processors' groups of read CC, CCC and CDL files, and groups the
//! formats refuse (empty, more than one CDL as a CC, other transforms, unknown formats).

mod common;

use common::lut_files::{file, write_files};
use common::transforms::{Case, direction_name};
use ocio::TransformDirection;
use ocio::{CdlTransform, Config, FileTransform, GroupTransform, RangeTransform, Transform};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values;
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

/// The formats of the cases, as the user names them.
const FORMATS: [&str; 5] = [
    "ColorCorrection",
    "ColorCorrectionCollection",
    "ColorDecisionList",
    "colordecisionlist",
    "Not a format",
];

/// A CDL's spec and the port's CDL, from its parameters.
fn cdl(id: &str, desc: &str, values: [f64; 10]) -> Case {
    let mut port = CdlTransform::new();
    port.set_slope(&values[0..3].try_into().unwrap());
    port.set_offset(&values[3..6].try_into().unwrap());
    port.set_power(&values[6..9].try_into().unwrap());
    port.set_sat(values[9]);
    port.set_id(id.as_bytes());
    port.set_first_sop_description(desc.as_bytes());
    let f =
        |r: std::ops::Range<usize>| Value::Array(values[r].iter().map(|&v| f64_spec(v)).collect());
    Case::new(
        id,
        json!({"class": "CDLTransform", "calls": [
            ["setSlope", f(0..3)],
            ["setOffset", f(3..6)],
            ["setPower", f(6..9)],
            ["setSat", f64_spec(values[9])],
            ["setID", id],
            ["setFirstSOPDescription", desc],
        ]}),
        port,
    )
}

/// A group of `cases`, with the metadata calls `metadata` on the group.
struct WriteCase {
    label: String,
    request: Value,
    port: GroupTransform,
}

/// The write of a group of `cases` in `format`, after the group's metadata calls `metadata`
/// (`addChildElement` and `setName`).
fn group_case(
    label: &str,
    cases: &[Case],
    metadata: &[(&str, &str, &str)],
    format: &str,
) -> WriteCase {
    let mut port = GroupTransform::new();
    for case in cases {
        port.append_transform(case.port.clone());
    }
    let mut calls = Vec::new();
    for &(method, a, b) in metadata {
        match method {
            "addChildElement" => {
                port.format_metadata_mut()
                    .add_child_element(Some(a.as_bytes()), Some(b.as_bytes()))
                    .unwrap();
                calls.push(json!(["addChildElement", a, b]));
            }
            "setName" => {
                port.format_metadata_mut().set_name(Some(a.as_bytes()));
                calls.push(json!(["setName", a]));
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

/// The write of the group of the raw config's processor of `transform` (a file transform of
/// `src` with the cccid `cccid`) in `format`.
fn processor_case(label: &str, src: &str, cccid: &str, format: &str) -> WriteCase {
    let mut transform = FileTransform::new();
    transform.set_src(src);
    transform.set_ccc_id(cccid);
    let config = Config::create_raw().unwrap();
    let port = config
        .processor_in_direction(&Transform::from(transform), TransformDirection::Forward)
        .and_then(|p| p.create_group_transform())
        .expect("the processor's group");
    WriteCase {
        label: format!("{label} as {format}"),
        request: json!({
            "processor": {
                "transform": {"class": "FileTransform",
                              "calls": [["setSrc", src], ["setCCCId", cccid]]},
                "direction": direction_name(TransformDirection::Forward),
            },
            "format": format,
        }),
        port,
    }
}

/// Each case's write in the wheel against the port's.
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

#[test]
fn groups_of_cdls_write_as_in_the_wheel() {
    let plain = cdl("", "", [1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
    let upstream = cdl(
        "Test look: 01-A.",
        "SOP Desc",
        [1.35, 1.1, 0.071, 0.05, -0.23, 0.11, 0.93, 0.81, 1.27, 1.23],
    );
    let special = cdl(
        "a < b & \"c\" 'd' > e",
        "x < y & \"z\" 'w' > v",
        [
            0.1 + 0.2,
            1.0 / 3.0,
            1e-7,
            -0.0,
            123_456_789.125,
            1e21,
            2.5e-310,
            0.5,
            7.0,
            0.0,
        ],
    );
    let extremes = cdl(
        "extremes",
        "",
        [
            f64::MAX,
            f64::MIN_POSITIVE,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
            -1.0,
            1e300,
            1e-300,
            100.0,
            -2.0,
        ],
    );
    let range = Case::new(
        "range",
        json!({"class": "RangeTransform"}),
        RangeTransform::new(),
    );
    let descriptions: &[(&str, &str, &str)] = &[
        ("addChildElement", "Description", "main & <first>"),
        ("addChildElement", "InputDescription", "input 'quoted'"),
        ("addChildElement", "description", "main, lower case"),
        ("addChildElement", "ViewingDescription", "viewing \"x\""),
        ("addChildElement", "SOPDescription", "the group's SOP"),
        ("addChildElement", "SATDescription", "the group's Sat"),
        ("addChildElement", "Other", "ignored"),
        ("addChildElement", "INPUTDESCRIPTION", "input, upper case"),
        ("setName", "the group", ""),
    ];

    let mut cases = Vec::new();
    for format in FORMATS {
        cases.push(group_case(
            "one plain CDL",
            std::slice::from_ref(&plain),
            &[],
            format,
        ));
        cases.push(group_case(
            "upstream's CDL",
            std::slice::from_ref(&upstream),
            &[],
            format,
        ));
        cases.push(group_case(
            "special characters",
            std::slice::from_ref(&special),
            &[],
            format,
        ));
        cases.push(group_case(
            "extreme values",
            std::slice::from_ref(&extremes),
            &[],
            format,
        ));
        cases.push(group_case(
            "two CDLs",
            &[upstream.clone(), special.clone()],
            descriptions,
            format,
        ));
        cases.push(group_case(
            "descriptions",
            std::slice::from_ref(&plain),
            descriptions,
            format,
        ));
        cases.push(group_case("an empty group", &[], &[], format));
        cases.push(group_case(
            "a range",
            std::slice::from_ref(&range),
            &[],
            format,
        ));
        cases.push(group_case(
            "a CDL then a range",
            &[plain.clone(), range.clone()],
            &[],
            format,
        ));
    }
    check_writes(&cases);
}

#[test]
fn groups_of_read_files_write_as_in_the_wheel() {
    let ccc = "<ColorCorrectionCollection xmlns=\"urn:ASC:CDL:v1.01\">\n\
        <Description>collection &amp; &lt;desc&gt;</Description>\n\
        <ColorCorrection id=\"one &amp; two\" name=\"n\">\n\
        <Description>cc &quot;desc&quot;</Description>\n\
        <InputDescription>in</InputDescription>\n\
        <ViewingDescription>view</ViewingDescription>\n\
        <SOPNode>\n<Description>sop &lt; desc</Description>\n\
        <Slope>1.1 1.2 1.3</Slope><Offset>-0.01 0.02 0.03</Offset><Power>1.25 1 0.9</Power>\n\
        </SOPNode>\n\
        <SatNode><Description>sat &gt; desc</Description><Saturation>0.8</Saturation></SatNode>\n\
        </ColorCorrection>\n\
        </ColorCorrectionCollection>\n";
    let dir = write_files(&[file("descs.ccc", ccc.as_bytes())]);
    let upstream = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    let upstream = upstream.to_str().unwrap().replace('\\', "/");
    let upstream = upstream.trim_start_matches("//?/");

    let mut cases = Vec::new();
    for format in FORMATS {
        cases.push(processor_case(
            "a CCC with descriptions",
            &format!("{dir}/descs.ccc"),
            "",
            format,
        ));
        for (name, cccid) in [
            ("cdl_test1.cc", ""),
            ("cdl_test2.cc", ""),
            ("cdl_test1.ccc", "cc0002"),
            ("cdl_test1.ccc", "3"),
            ("cdl_test1.cdl", "cc0001"),
        ] {
            cases.push(processor_case(
                &format!("{name} [{cccid}]"),
                &format!("{upstream}/{name}"),
                cccid,
                format,
            ));
        }
    }
    check_writes(&cases);
}
