// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CC reader (the CDL parser and expat under it) against the wheel (`processor_ops`): the
//! processors of file transforms of generated CC files and of upstream's, or their errors,
//! with the parser's and expat's messages and the line they name: missing and malformed
//! nodes, values, descriptions at each level, unknown elements, character data where none
//! belongs, XML errors on each line, encodings, entities, line ends, numbers past the double
//! range (I-172), and a header past the 5 KiB the parser scans for the root element.

mod common;

use common::lut_files::{file, file_case, write_files};
use common::transforms::{Case, check_processors, check_processors_dirs};
use ocio::{Interpolation, TransformDirection};

/// The case of a set of one file, `lut.cc`, holding `bytes`.
fn case(label: &str, bytes: &[u8]) -> Case {
    let dir = write_files(&[file("lut.cc", bytes)]);
    file_case(
        label,
        &format!("{dir}/lut.cc"),
        Interpolation::Default,
        TransformDirection::Forward,
    )
}

/// A CC of the given SOP node's and Sat node's contents.
fn cc(id: &str, sop: &str, sat: &str) -> String {
    format!(
        "<ColorCorrection id=\"{id}\">\n\
         \x20   <SOPNode>\n{sop}    </SOPNode>\n\
         \x20   <SatNode>\n{sat}    </SatNode>\n\
         </ColorCorrection>\n"
    )
}

const SOP: &str = "        <Slope>1.1 1.2 1.3</Slope>\n\
                   \x20       <Offset>-0.01 0.02 0.03</Offset>\n\
                   \x20       <Power>1.25 1.0 0.9</Power>\n";
const SAT: &str = "        <Saturation>0.8</Saturation>\n";

#[test]
fn cc_files_read_as_in_the_wheel() {
    let valid = cc("a", SOP, SAT);
    let mut cases = vec![
        case("valid", valid.as_bytes()),
        case("valid, CR LF", valid.replace('\n', "\r\n").as_bytes()),
        case("valid, lone CRs", valid.replace('\n', "\r").as_bytes()),
        case("valid, no final line end", valid.trim_end().as_bytes()),
        case(
            "XML declaration",
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{valid}").as_bytes(),
        ),
        case(
            "UTF-8 BOM",
            format!("\u{feff}{valid}").as_bytes(),
        ),
        case(
            "descriptions at every level",
            cc(
                "d",
                &format!(
                    "        <Description>sop &amp; &lt;desc&gt;</Description>\n{SOP}"
                ),
                &format!("        <Description>sat</Description>\n{SAT}"),
            )
            .replace(
                "    <SOPNode>",
                "    <Description>cc desc</Description>\n    <InputDescription>in</InputDescription>\n    <ViewingDescription>view</ViewingDescription>\n    <SOPNode>",
            )
            .as_bytes(),
        ),
        case(
            "an input description in the SOP node",
            cc(
                "e",
                &format!("        <InputDescription>x</InputDescription>\n{SOP}"),
                SAT,
            )
            .as_bytes(),
        ),
        case(
            "a description over several lines",
            cc(
                "f",
                &format!("        <Description>line 1\n  line 2\n\tline 3 </Description>\n{SOP}"),
                SAT,
            )
            .as_bytes(),
        ),
        case(
            "SATNode",
            cc("g", SOP, SAT).replace("SatNode", "SATNode").as_bytes(),
        ),
        case("no Sat node", cc("h", SOP, "").replace("    <SatNode>\n    </SatNode>\n", "").as_bytes()),
        case("an empty id", cc("", SOP, SAT).as_bytes()),
        case(
            "no id, another attribute",
            cc("i", SOP, SAT)
                .replace("id=\"i\"", "name=\"n\"")
                .as_bytes(),
        ),
        case(
            "an unknown element",
            cc("j", &format!("        <Foo>bar</Foo>\n{SOP}"), SAT).as_bytes(),
        ),
        case(
            "a SOP node under a SOP node",
            cc("k", &format!("        <SOPNode/>\n{SOP}"), SAT).as_bytes(),
        ),
        case(
            "values with commas and line ends",
            cc(
                "l",
                "        <Slope>1.1,1.2\n1.3</Slope>\n        <Offset> 0 0 0 </Offset>\n        <Power>1 1 1</Power>\n",
                SAT,
            )
            .as_bytes(),
        ),
        case(
            "number forms",
            cc(
                "m",
                "        <Slope>1e0 .5 +2</Slope>\n        <Offset>-0 0x10 1e-3</Offset>\n        <Power>1. 1E0 1</Power>\n",
                SAT,
            )
            .as_bytes(),
        ),
        case(
            "a slope past the double range",
            cc(
                "n",
                &SOP.replace("1.1 1.2 1.3", "1e999 1 1"),
                SAT,
            )
            .as_bytes(),
        ),
        case(
            "a subnormal offset",
            cc("o", &SOP.replace("-0.01 0.02 0.03", "1e-310 0 0"), SAT).as_bytes(),
        ),
        case(
            "latin-1",
            format!(
                "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n{}",
                cc("p", &format!("        <Description>caf\u{e9}</Description>\n{SOP}"), SAT)
            )
            .chars()
            .map(|c| c as u32 as u8)
            .collect::<Vec<u8>>()
            .as_slice(),
        ),
        case(
            "an internal entity",
            format!(
                "<!DOCTYPE ColorCorrection [<!ENTITY one \"1.0\">]>\n{}",
                cc("q", &SOP.replace("1.25 1.0 0.9", "&one; &one; &one;"), SAT)
            )
            .as_bytes(),
        ),
        // Errors of the elements.
        case(
            "no Slope",
            cc("r", &SOP.replace("        <Slope>1.1 1.2 1.3</Slope>\n", ""), SAT).as_bytes(),
        ),
        case(
            "no Offset",
            cc("s", &SOP.replace("-0.01 0.02 0.03", "").replace("<Offset></Offset>", ""), SAT)
                .as_bytes(),
        ),
        case(
            "no Power",
            cc("t", &SOP.replace("        <Power>1.25 1.0 0.9</Power>\n", ""), SAT).as_bytes(),
        ),
        case(
            "two slope values",
            cc("u", &SOP.replace("1.1 1.2 1.3", "1.1 1.2"), SAT).as_bytes(),
        ),
        case(
            "an empty slope",
            cc("v", &SOP.replace("1.1 1.2 1.3", ""), SAT).as_bytes(),
        ),
        case(
            "a word in the slope",
            cc("w", &SOP.replace("1.1 1.2 1.3", "1.1 abc 1.3"), SAT).as_bytes(),
        ),
        case(
            "a long illegal slope",
            cc("x", &SOP.replace("1.1 1.2 1.3", "1.1 1.2 1.3 abcdefghijklmnopqrstuvwxyz"), SAT)
                .as_bytes(),
        ),
        case(
            "two saturations",
            cc("y", SOP, "        <Saturation>0.8 0.9</Saturation>\n").as_bytes(),
        ),
        case(
            "a word in the saturation",
            cc("z", SOP, "        <Saturation>high</Saturation>\n").as_bytes(),
        ),
        case(
            "text in the SOP node",
            cc("aa", &format!("        text\n{SOP}"), SAT).as_bytes(),
        ),
        case(
            "a slope outside the SOP node",
            cc("ab", SOP, &format!("{SAT}        <Slope>1 1 1</Slope>\n")).as_bytes(),
        ),
        // Errors of expat.
        case(
            "a mismatched tag",
            cc("ac", &SOP.replace("</Power>", "</Slope>"), SAT).as_bytes(),
        ),
        case(
            "an invalid token",
            cc("ad", &SOP.replace("<Offset>", "<<Offset>"), SAT).as_bytes(),
        ),
        case(
            "no end tag",
            cc("ae", SOP, SAT)
                .replace("</ColorCorrection>\n", "")
                .as_bytes(),
        ),
        case(
            "an undefined entity",
            cc("af", &SOP.replace("1.25", "&x;"), SAT).as_bytes(),
        ),
        case(
            "junk after the root",
            format!("{valid}<ColorCorrection/>\n").as_bytes(),
        ),
        case(
            "a duplicate attribute",
            cc("ag", SOP, SAT)
                .replace("id=\"ag\"", "id=\"ag\" id=\"ah\"")
                .as_bytes(),
        ),
        case(
            "an unknown encoding",
            format!("<?xml version=\"1.0\" encoding=\"klingon\"?>\n{valid}").as_bytes(),
        ),
        // The root element.
        case("no CC tag", b"<Foo>\n</Foo>\n".as_slice()),
        case(
            "a comment past 5 KiB before the root",
            format!("<!--\n{}\n-->\n{valid}", "x".repeat(6000)).as_bytes(),
        ),
        case(
            "lines past 5 KiB before the root",
            format!(
                "<!--\n{}-->\n{valid}",
                format!("{}\n", "y".repeat(100)).repeat(60)
            )
            .as_bytes(),
        ),
        case(
            "lines just under 5 KiB before the root",
            format!(
                "<!--\n{}-->\n{valid}",
                format!("{}\n", "y".repeat(100)).repeat(50)
            )
            .as_bytes(),
        ),
        case(
            "a line longer than the header's buffer",
            format!("<!-- {} -->{valid}", "z".repeat(5200)).as_bytes(),
        ),
    ];

    // Upstream's files.
    for name in [
        "cdl_test1.cc",
        "cdl_test2.cc",
        "cdl_test_SATNode.cc",
        "cdl_test_ASC_SAT.cc",
        "cdl_test_ASC_SOP.cc",
    ] {
        let path = ocio_testkit::paths::upstream_dir()
            .join("tests/data/files")
            .join(name);
        let path = path.to_str().unwrap().replace('\\', "/");
        let path = path.trim_start_matches("//?/");
        cases.push(file_case(
            name,
            path,
            Interpolation::Default,
            TransformDirection::Forward,
        ));
    }

    check_processors(&cases);
}

/// A file transform's direction combines with the processor's (`buildFileOps`), and an
/// inverse transform of a CC is an inverse CDL.
#[test]
fn cc_file_transforms_in_both_directions() {
    let dir = write_files(&[file("lut.cc", cc("a", SOP, SAT).as_bytes())]);
    let cases = vec![
        file_case(
            "inverse",
            &format!("{dir}/lut.cc"),
            Interpolation::Default,
            TransformDirection::Inverse,
        ),
        file_case(
            "forward, nearest",
            &format!("{dir}/lut.cc"),
            Interpolation::Nearest,
            TransformDirection::Forward,
        ),
    ];
    check_processors_dirs(
        &cases,
        &[TransformDirection::Forward, TransformDirection::Inverse],
    );
}
