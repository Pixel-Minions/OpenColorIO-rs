// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CCC and CDL readers against the wheel (`processor_ops`): the processors of file
//! transforms of generated CCC and CDL files and of upstream's, or their errors: the color
//! correction a cccid picks (by id, by index, empty, out of range, not a number, from a
//! context variable), the CDL style of the file transform, duplicate ids, the elements of
//! each schema, and files of one schema under another's extension.

mod common;

use common::lut_files::{file, file_case, write_files};
use common::transforms::{Case, check_processors, check_processors_in};
use ocio::{Config, FileTransform, Interpolation, TransformDirection};
use ocio_ops::open_color_types::CdlStyle;
use serde_json::{Value, json};

/// A file transform of `src` with the cccid `cccid`.
fn ccc_case(label: &str, src: &str, cccid: &str) -> Case {
    ccc_style_case(label, src, cccid, None)
}

/// A file transform of `src` with the cccid `cccid` and, if given, the CDL style `style`.
fn ccc_style_case(label: &str, src: &str, cccid: &str, style: Option<CdlStyle>) -> Case {
    let mut calls = vec![json!(["setSrc", src]), json!(["setCCCId", cccid])];
    let mut port = FileTransform::new();
    port.set_src(src);
    port.set_ccc_id(cccid);
    if let Some(style) = style {
        let name = match style {
            CdlStyle::Asc => "CDL_ASC",
            CdlStyle::NoClamp => "CDL_NO_CLAMP",
        };
        calls.push(json!(["setCDLStyle", {"enum": name}]));
        port.set_cdl_style(style);
    }
    Case::new(
        label,
        json!({"class": "FileTransform", "calls": Value::Array(calls)}),
        port,
    )
}

/// The path of upstream's test file `name`.
fn upstream_file(name: &str) -> String {
    let path = ocio_testkit::paths::upstream_dir()
        .join("tests/data/files")
        .join(name);
    let path = path.to_str().unwrap().replace('\\', "/");
    path.trim_start_matches("//?/").to_owned()
}

/// A color correction of the given id attribute (none for `None`) and slope.
fn cc(id: Option<&str>, slope: &str) -> String {
    let id = id.map_or(String::new(), |id| format!(" id=\"{id}\""));
    format!(
        "<ColorCorrection{id}>\n\
         \x20   <SOPNode>\n\
         \x20       <Slope>{slope}</Slope>\n\
         \x20       <Offset>0.01 0.02 0.03</Offset>\n\
         \x20       <Power>1.1 1.2 1.3</Power>\n\
         \x20   </SOPNode>\n\
         \x20   <SatNode>\n\
         \x20       <Saturation>0.9</Saturation>\n\
         \x20   </SatNode>\n\
         </ColorCorrection>\n"
    )
}

/// A CCC of `body`.
fn ccc(body: &str) -> String {
    format!(
        "<ColorCorrectionCollection xmlns=\"urn:ASC:CDL:v1.01\">\n\
         \x20   <Description>a collection</Description>\n\
         {body}</ColorCorrectionCollection>\n"
    )
}

/// A CDL of `body`.
fn cdl(body: &str) -> String {
    format!(
        "<ColorDecisionList xmlns=\"urn:ASC:CDL:v1.01\">\n\
         \x20   <InputDescription>a list</InputDescription>\n\
         {body}</ColorDecisionList>\n"
    )
}

/// A color decision of `body`.
fn decision(body: &str) -> String {
    format!("<ColorDecision>\n{body}</ColorDecision>\n")
}

#[test]
fn upstream_files_by_cccid_as_in_the_wheel() {
    let mut cases = Vec::new();
    for name in [
        "cdl_test1.ccc",
        "cdl_test1.cdl",
        "cdl_test1.cc",
        "cdl_test_cc_file_with_extension.ccc",
        "cdl_test_cc_file_with_extension.cdl",
    ] {
        let src = upstream_file(name);
        for cccid in [
            "",
            "cc0001",
            "cc0002",
            "cc0003",
            "foo",
            "CC0001",
            "0",
            "1",
            "3",
            "4",
            "5",
            "-1",
            "+1",
            "01",
            " 1",
            "1 ",
            "1x",
            "x",
            "0x1",
            "2147483648",
            "nope",
        ] {
            cases.push(ccc_case(&format!("{name} [{cccid}]"), &src, cccid));
        }
        for style in [CdlStyle::Asc, CdlStyle::NoClamp] {
            cases.push(ccc_style_case(
                &format!("{name} [cc0002] {style:?}"),
                &src,
                "cc0002",
                Some(style),
            ));
            cases.push(ccc_style_case(
                &format!("{name} [1] {style:?}"),
                &src,
                "1",
                Some(style),
            ));
        }
        cases.push(file_case(
            format!("{name}, inverse"),
            &src,
            Interpolation::Default,
            TransformDirection::Inverse,
        ));
    }
    check_processors(&cases);
}

#[test]
fn generated_files_as_in_the_wheel() {
    let two = format!("{}{}", cc(Some("a"), "1.1 1.2 1.3"), cc(Some("b"), "2 2 2"));
    let files: Vec<(&str, String)> = vec![
        ("two.ccc", ccc(&two)),
        ("empty.ccc", ccc("")),
        (
            "no ids.ccc",
            ccc(&format!("{}{}", cc(None, "1 1 1"), cc(None, "3 3 3"))),
        ),
        (
            "duplicate ids.ccc",
            ccc(&format!(
                "{}{}",
                cc(Some("a"), "1 1 1"),
                cc(Some("a"), "3 3 3")
            )),
        ),
        (
            "an empty id.ccc",
            ccc(&format!(
                "{}{}",
                cc(Some(""), "1 1 1"),
                cc(Some(""), "3 3 3")
            )),
        ),
        (
            "an id that is a number.ccc",
            ccc(&format!(
                "{}{}",
                cc(Some("1"), "1 1 1"),
                cc(Some("0"), "3 3 3")
            )),
        ),
        (
            "an id with an entity.ccc",
            ccc(&cc(Some("a &amp; b"), "1 1 1")),
        ),
        ("a color correction error.ccc", ccc(&cc(Some("a"), "1 1"))),
        (
            "a decision in a collection.ccc",
            ccc(&decision(&cc(Some("a"), "1 1 1"))),
        ),
        (
            "a collection in a collection.ccc",
            ccc(&ccc(&cc(Some("a"), "1 1 1"))),
        ),
        (
            "an unknown element.ccc",
            ccc(&format!("<Foo/>\n{}", cc(Some("a"), "1 1 1"))),
        ),
        ("text in the collection.ccc", ccc(&format!("text\n{two}"))),
        ("a CC as .ccc.ccc", cc(Some("a"), "1 1 1")),
        ("a CCC as .cc.cc", ccc(&two)),
        ("a CDL as .ccc.ccc", cdl(&decision(&two))),
        (
            "two decisions.cdl",
            cdl(&format!(
                "{}{}",
                decision(&cc(Some("a"), "1 1 1")),
                decision(&cc(Some("b"), "2 2 2"))
            )),
        ),
        ("two corrections in a decision.cdl", cdl(&decision(&two))),
        (
            "an empty decision.cdl",
            cdl(&format!(
                "{}{}",
                decision(""),
                decision(&cc(Some("b"), "2 2 2"))
            )),
        ),
        ("empty.cdl", cdl("")),
        ("a correction outside a decision.cdl", cdl(&two)),
        (
            "a decision with descriptions.cdl",
            cdl(&decision(&format!(
                "<Description>d</Description>\n<InputDescription>i</InputDescription>\n<ViewingDescription>v</ViewingDescription>\n<MediaRef ref=\"x\"/>\n{}",
                cc(Some("a"), "1 1 1")
            ))),
        ),
        (
            "a list in a decision.cdl",
            cdl(&decision(&cdl(&decision(&cc(Some("a"), "1 1 1"))))),
        ),
        ("a CCC as .cdl.cdl", ccc(&two)),
        ("a CDL as .cc.cc", cdl(&decision(&two))),
        ("a CDL as .txt.txt", cdl(&decision(&two))),
        ("a CCC as .txt.txt", ccc(&two)),
        ("a CC as .txt.txt", cc(Some("a"), "1 1 1")),
    ];
    let mut entries = Vec::new();
    let mut names = Vec::new();
    for (k, (label, text)) in files.iter().enumerate() {
        // The extension is the label's last one.
        let ext = label.rsplit('.').next().unwrap();
        let name = format!("f{k}.{ext}");
        entries.push(file(&name, text.as_bytes()));
        names.push(name);
    }
    let dir = write_files(&entries);
    let mut cases = Vec::new();
    for ((label, _), name) in files.iter().zip(&names) {
        let src = format!("{dir}/{name}");
        for cccid in ["", "a", "b", "0", "1", "2", "a & b"] {
            cases.push(ccc_case(&format!("{label} [{cccid}]"), &src, cccid));
        }
    }
    check_processors(&cases);
}

/// The cccid is resolved in the config's context.
#[test]
fn a_cccid_from_a_context_variable_as_in_the_wheel() {
    const CONFIG: &str = "ocio_profile_version: 2\n\
environment: {CCC: cc0002, IDX: \"3\"}\n\
roles: {default: raw}\n\
colorspaces:\n  \
- !<ColorSpace> {name: raw}\n";
    let mut config = Config::create_raw().unwrap();
    let config_mut = std::sync::Arc::get_mut(&mut config).unwrap();
    config_mut.add_environment_var("CCC", Some(b"cc0002"));
    config_mut.add_environment_var("IDX", Some(b"3"));
    let src = upstream_file("cdl_test1.ccc");
    let cases = vec![
        ccc_case("$CCC", &src, "$CCC"),
        ccc_case("${IDX}", &src, "${IDX}"),
        ccc_case("$NOPE", &src, "$NOPE"),
    ];
    check_processors_in(&cases, Some(&json!({"yaml": CONFIG})), &config);
}
