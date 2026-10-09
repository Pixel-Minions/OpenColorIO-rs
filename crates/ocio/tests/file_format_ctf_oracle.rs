// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CLF/CTF parser against the wheel (`processor_ops`): the errors and the warnings of
//! reading generated files and upstream's that the wheel refuses before their first
//! operator, or that have none: the ProcessList's attributes and versions (CTF and CLF, by
//! attribute and by extension), the elements each parent allows, metadata, Info and its
//! version, descriptors, descriptions, the operators each version supports, XML errors, and
//! the 5 KiB the format check reads. The operators' readers come with WP 4.5c on.

mod cdl_common;
mod common;

use common::lut_files::{file, file_case, write_files};
use common::transforms::{Case, check_processors};
use ocio::{Interpolation, TransformDirection};

/// The case of a set of one file, `name`, holding `bytes`.
fn case(label: &str, name: &str, bytes: &[u8]) -> Case {
    let dir = write_files(&[file(name, bytes)]);
    file_case(
        label,
        &format!("{dir}/{name}"),
        Interpolation::Default,
        TransformDirection::Forward,
    )
}

/// A ProcessList of the attributes `attrs` holding `body`.
fn pl(attrs: &str, body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <ProcessList {attrs}>\n{body}</ProcessList>\n"
    )
}

/// A CTF case: `lut.ctf` holding a ProcessList of `attrs` and `body`.
fn ctf(label: &str, attrs: &str, body: &str) -> Case {
    case(label, "lut.ctf", pl(attrs, body).as_bytes())
}

/// A CLF case: `lut.clf` holding a ProcessList of `attrs` and `body`.
fn clf(label: &str, attrs: &str, body: &str) -> Case {
    case(label, "lut.clf", pl(attrs, body).as_bytes())
}

/// The path of upstream's test file `name`.
fn upstream_file(name: &str) -> String {
    let path = ocio_testkit::paths::upstream_dir()
        .join("tests/data/files")
        .join(name);
    let path = path.to_str().unwrap().replace('\\', "/");
    path.trim_start_matches("//?/").to_owned()
}

/// An Info element's contents.
const INFO_BODY: &str = "        <Release>2017.0.63</Release>\n\
                         \x20       <Category a=\"1\" b=\"\">\n\
                         \x20           <Tags>\n\
                         \x20               <Maya />\n\
                         \x20               <Flame>  some text  </Flame>\n\
                         \x20           </Tags>\n\
                         \x20       </Category>\n";

/// The generated files and upstream's.
fn ctf_cases() -> Vec<Case> {
    let d = "    <Description>no operator</Description>\n";
    let mut cases = vec![
        // The ProcessList's attributes.
        ctf("no operator", "id=\"a\" version=\"2.5\"", d),
        ctf("no operator, no body", "id=\"a\" version=\"2.5\"", ""),
        ctf("no id", "version=\"2.5\"", d),
        ctf("an empty id", "id=\"\" version=\"2.5\"", d),
        ctf("an id in upper case", "ID=\"a\" version=\"2.5\"", d),
        ctf("an empty name", "id=\"a\" name=\"\"", d),
        ctf("a name", "id=\"a\" name=\"n\"", d),
        ctf("an empty inverseOf", "id=\"a\" inverseOf=\"\"", d),
        ctf(
            "an unknown attribute",
            "id=\"a\" foo=\"bar\" version=\"1.3\"",
            d,
        ),
        ctf("xmlns", "id=\"a\" xmlns=\"urn:x\"", d),
        // CTF versions.
        ctf("no version", "id=\"a\"", d),
        ctf("an empty version", "id=\"a\" version=\"\"", d),
        ctf("version abc", "id=\"a\" version=\"abc\"", d),
        ctf("version 1.2.3.4", "id=\"a\" version=\"1.2.3.4\"", d),
        ctf("version 1.", "id=\"a\" version=\"1.\"", d),
        ctf("version 1.3.10", "id=\"a\" version=\"1.3.10\"", d),
        ctf("version 2.5.1", "id=\"a\" version=\"2.5.1\"", d),
        ctf("version 2.6", "id=\"a\" version=\"2.6\"", d),
        ctf("version 3", "id=\"a\" version=\"3\"", d),
        ctf(
            "version and Version",
            "id=\"a\" version=\"1.3\" Version=\"1.4\"",
            d,
        ),
        ctf(
            "version and compCLFversion",
            "id=\"a\" version=\"1.3\" compCLFversion=\"3\"",
            d,
        ),
        ctf(
            "compCLFversion and version",
            "id=\"a\" compCLFversion=\"3\" version=\"1.3\"",
            d,
        ),
        ctf(
            "compCLFversion twice",
            "id=\"a\" compCLFversion=\"3\" compclfversion=\"3\"",
            d,
        ),
        // CLF versions.
        ctf(
            "compCLFversion in a .ctf",
            "id=\"a\" compCLFversion=\"2\"",
            d,
        ),
        clf("compCLFversion 3", "id=\"a\" compCLFversion=\"3\"", d),
        clf(
            "compCLFversion 3.0.1",
            "id=\"a\" compCLFversion=\"3.0.1\"",
            d,
        ),
        clf("compCLFversion 3.1", "id=\"a\" compCLFversion=\"3.1\"", d),
        clf("an empty compCLFversion", "id=\"a\" compCLFversion=\"\"", d),
        clf("compCLFversion x", "id=\"a\" compCLFversion=\"x\"", d),
        clf("a .clf without a version", "id=\"a\"", d),
        clf("a .clf with a CTF version", "id=\"a\" version=\"1.5\"", d),
        case(
            "a .CLF without a version",
            "lut.CLF",
            pl("id=\"a\"", d).as_bytes(),
        ),
        // The elements each parent allows.
        ctf(
            "a nested ProcessList",
            "id=\"a\"",
            "    <ProcessList id=\"b\"></ProcessList>\n",
        ),
        ctf(
            "an unknown element",
            "id=\"a\"",
            "    <Foo a=\"1\">x</Foo>\n",
        ),
        ctf(
            "an Array under the ProcessList",
            "id=\"a\"",
            "    <Array dim=\"3 3\"/>\n",
        ),
        ctf(
            "a DynamicParameter under the ProcessList",
            "id=\"a\"",
            "    <DynamicParameter param=\"EXPOSURE\"/>\n",
        ),
        ctf(
            "a Saturation under the ProcessList",
            "id=\"a\"",
            "    <Saturation>1</Saturation>\n",
        ),
        ctf(
            "ControlPoints under the ProcessList",
            "id=\"a\"",
            "    <ControlPoints dim=\"2\"/>\n",
        ),
        ctf(
            "ControlPoints under an unknown element",
            "id=\"a\"",
            "    <Red><ControlPoints dim=\"2\"/></Red>\n",
        ),
        ctf(
            "a Matrix in a Description",
            "id=\"a\"",
            "    <Description>a <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\"/> b</Description>\n",
        ),
        ctf(
            "a Matrix in an unknown element",
            "id=\"a\"",
            "    <Foo><Matrix inBitDepth=\"32f\" outBitDepth=\"32f\"/></Foo>\n",
        ),
        ctf(
            "a Matrix in a nested ProcessList",
            "id=\"a\"",
            "    <ProcessList><Matrix inBitDepth=\"32f\" outBitDepth=\"32f\"/></ProcessList>\n",
        ),
        ctf(
            "a Description in a Description",
            "id=\"a\"",
            "    <Description>a <Description>b</Description></Description>\n",
        ),
        ctf(
            "descriptions",
            "id=\"a\"",
            "    <Description>  one  </Description>\n    <description>two</description>\n",
        ),
        ctf(
            "descriptors",
            "id=\"a\" version=\"1.3\"",
            "    <InputDescriptor> in </InputDescriptor>\n\
             \x20   <OutputDescriptor>out<!-- c -->put</OutputDescriptor>\n\
             \x20   <inputdescriptor>2</inputdescriptor>\n",
        ),
        ctf(
            "a descriptor in an unknown element",
            "id=\"a\"",
            "    <Foo><InputDescriptor>in</InputDescriptor></Foo>\n",
        ),
        ctf("text in the ProcessList", "id=\"a\"", "    some text\n"),
        ctf("blanks in the ProcessList", "id=\"a\"", "    \t  \n"),
        // Info.
        ctf(
            "an Info",
            "id=\"a\" version=\"1.3\"",
            &format!("    <Info>\n{INFO_BODY}    </Info>\n"),
        ),
        ctf(
            "an Info in a CLF",
            "id=\"a\" compCLFversion=\"3\"",
            &format!("    <Info>\n{INFO_BODY}    </Info>\n"),
        ),
        ctf(
            "a nested Info",
            "id=\"a\"",
            "    <Foo><Info version=\"2\"/></Foo>\n",
        ),
        ctf(
            "an Info with a Matrix and a ProcessList inside",
            "id=\"a\"",
            "    <Info><Matrix inBitDepth=\"32f\"><ProcessList/></Matrix></Info>\n",
        ),
    ];
    for (label, version) in [
        ("1", "version=\"1\""),
        ("2.9", "version=\"2.9\""),
        ("3.0", "version=\"3.0\""),
        ("-5", "version=\"-5\""),
        ("abc", "version=\"abc\""),
        ("empty", "version=\"\""),
        ("blank", "version=\" \""),
        ("leading blanks", "version=\"  2\""),
        ("2x", "version=\"2x\""),
        ("past int", "version=\"4294967298\""),
        ("in upper case", "VERSION=\"5\""),
        ("second", "a=\"1\" version=\"5\""),
    ] {
        cases.push(ctf(
            &format!("an Info version {label}"),
            "id=\"a\"",
            &format!("    <Info {version}>\n{INFO_BODY}    </Info>\n"),
        ));
    }

    // The operators a version doesn't support.
    for (label, attrs, op) in [
        ("ASC_CDL in CTF 1.2", "version=\"1.2\"", "ASC_CDL"),
        ("ACES in CTF 1.4", "version=\"1.4\"", "ACES"),
        ("Function in CTF 1.5", "version=\"1.5\"", "Function"),
        (
            "FixedFunction in CTF 1.9",
            "version=\"1.9\"",
            "FixedFunction",
        ),
        (
            "GradingHueCurve in CTF 2.4",
            "version=\"2.4\"",
            "GradingHueCurve",
        ),
        ("Log in CTF 1.2", "version=\"1.2\"", "Log"),
        ("InverseLUT1D in CTF 1.2", "version=\"1.2\"", "InverseLUT1D"),
        ("InverseLUT3D in CTF 1.5", "version=\"1.5\"", "InverseLUT3D"),
        (
            "ExposureContrast in CLF 3",
            "compCLFversion=\"3\"",
            "ExposureContrast",
        ),
        (
            "FixedFunction in CLF 3",
            "compCLFversion=\"3\"",
            "FixedFunction",
        ),
        ("Function in CLF 3", "compCLFversion=\"3\"", "Function"),
        (
            "GradingPrimary in CLF 3",
            "compCLFversion=\"3\"",
            "GradingPrimary",
        ),
        (
            "GradingRGBCurve in CLF 3",
            "compCLFversion=\"3\"",
            "GradingRGBCurve",
        ),
        (
            "GradingTone in CLF 3",
            "compCLFversion=\"3\"",
            "GradingTone",
        ),
        (
            "GradingHueCurve in CLF 3",
            "compCLFversion=\"3\"",
            "GradingHueCurve",
        ),
        (
            "InverseLUT1D in CLF 3",
            "compCLFversion=\"3\"",
            "InverseLUT1D",
        ),
        (
            "InverseLUT3D in CLF 3",
            "compCLFversion=\"3\"",
            "InverseLUT3D",
        ),
        ("Gamma in CLF 2", "compCLFversion=\"2\"", "Gamma"),
        ("Exponent in CLF 2", "compCLFversion=\"2\"", "Exponent"),
        ("Log in CLF 2", "compCLFversion=\"2\"", "log"),
    ] {
        cases.push(ctf(
            label,
            &format!("id=\"a\" {attrs}"),
            &format!("    <{op} inBitDepth=\"32f\" outBitDepth=\"32f\"/>\n"),
        ));
    }

    cases.extend([
        // An operator as the root.
        case(
            "an operator as the root",
            "lut.ctf",
            b"<!-- <ProcessList -->\n<Matrix inBitDepth=\"32f\" outBitDepth=\"32f\"/>\n",
        ),
        case(
            "an unknown root",
            "lut.ctf",
            b"<!-- <ProcessList -->\n<Foo><ProcessList id=\"a\"/></Foo>\n",
        ),
        // XML errors.
        ctf("a tag mismatch", "id=\"a\"", "    <Description>x</Desc>\n"),
        case(
            "the ProcessList not closed",
            "lut.ctf",
            b"<ProcessList id=\"a\">\n<Description>x</Description>\n",
        ),
        case(
            "junk after the ProcessList",
            "lut.ctf",
            b"<ProcessList id=\"a\">\n</ProcessList>\n<Foo/>\n",
        ),
        ctf(
            "a bad token",
            "id=\"a\"",
            "    <Description>&bad;</Description>\n",
        ),
        case("an empty file", "lut.ctf", b""),
        // The format check's 5 KiB.
        case(
            "a ProcessList after 5 KiB of lines",
            "lut.ctf",
            format!(
                "<!--\n{}-->\n{}",
                format!("{}\n", "y".repeat(100)).repeat(60),
                pl("id=\"a\"", d)
            )
            .as_bytes(),
        ),
        case(
            "a ProcessList just under 5 KiB of lines",
            "lut.ctf",
            format!(
                "<!--\n{}-->\n{}",
                format!("{}\n", "y".repeat(100)).repeat(49),
                pl("id=\"a\"", d)
            )
            .as_bytes(),
        ),
        case(
            "a ProcessList on a line past the format check's buffer",
            "lut.ctf",
            format!(
                "<ProcessList id=\"a\" name=\"{}\">\n{d}</ProcessList>\n",
                "n".repeat(6000)
            )
            .as_bytes(),
        ),
        case(
            "a ProcessList at the end of a line past the format check's buffer",
            "lut.ctf",
            format!(
                "<!-- {} --><ProcessList id=\"a\">\n{d}</ProcessList>\n",
                "c".repeat(6000)
            )
            .as_bytes(),
        ),
        case(
            "no line end",
            "lut.ctf",
            b"<ProcessList id=\"a\"><Description>x</Description></ProcessList>",
        ),
        case(
            "CR LF",
            "lut.ctf",
            pl("id=\"a\"", d).replace('\n', "\r\n").as_bytes(),
        ),
    ]);

    // Upstream's files that the wheel refuses before an operator.
    for name in [
        "clf/illegal/image_png.clf",
        "process_list_invalid_version.ctf",
        "clf/illegal/process_list_bad_version.clf",
        "process_list_higher_version.ctf",
        "clf/illegal/process_list_higher_version.clf",
        "clf/illegal/process_list_missing.clf",
        "clf/illegal/transform_missing.clf",
        "clf/illegal/transform_missing_id.clf",
        "clf/illegal/transform_empty.clf",
        "clf/illegal/transform_id_empty.clf",
        "info_version_invalid.ctf",
        "info_version_unsupported.ctf",
        "info_version_empty.ctf",
    ] {
        cases.push(file_case(
            name,
            &upstream_file(name),
            Interpolation::Default,
            TransformDirection::Forward,
        ));
    }
    cases
}

#[test]
fn ctf_files_read_as_in_the_wheel() {
    check_processors(&ctf_cases());
}

/// The warnings of the reads (unknown elements, misplaced ones, attributes), message by
/// message.
#[test]
fn ctf_files_log_as_in_the_wheel() {
    cdl_common::check_logs(&ctf_cases());
}

/// A Matrix of the attributes `attrs`, with an Array of the dimensions `dim` and `values`.
fn matrix(attrs: &str, dim: &str, values: &str) -> String {
    format!(
        "    <Matrix {attrs}>\n\
         \x20       <Array dim=\"{dim}\">\n{values}\n        </Array>\n\
         \x20   </Matrix>\n"
    )
}

/// A Range of the attributes `attrs` holding `body`.
fn range(attrs: &str, body: &str) -> String {
    format!("    <Range {attrs}>\n{body}    </Range>\n")
}

/// The values of a matrix of `n` values: 0.1, 0.2, ... and a few exact ones.
fn values(n: usize) -> String {
    (0..n)
        .map(|i| format!("{}", 0.1 * (i as f64 + 1.0) - 0.35))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The Matrix and Range readers' files.
fn op_cases() -> Vec<Case> {
    let f32s = "inBitDepth=\"32f\" outBitDepth=\"32f\"";
    let mut cases = Vec::new();

    // Matrix dimensions, by version.
    for (version, dim, n) in [
        ("1.2", "3 3 3", 9),
        ("1.2", "4 4 3", 16),
        ("1.2", "4 4 4", 16),
        ("1.2", "3 3", 9),
        ("1.2", "3 4 3", 12),
        ("1.2", "4 4 3", 12),
        ("1.2", "5 5 3", 25),
        ("1.3", "3 3 3", 9),
        ("1.3", "3 3", 9),
        ("1.3", "3 4 3", 12),
        ("1.3", "3 4", 12),
        ("1.3", "4 4 4", 16),
        ("1.3", "4 4", 16),
        ("1.3", "4 5 4", 20),
        ("1.3", "4 5", 20),
        ("1.3", "4 4 3", 16),
        ("1.3", "3 4 4", 12),
        ("1.3", "3", 3),
        ("1.3", "3 3 3 3", 9),
        ("1.3", "5 5", 25),
        ("2.0", "3 3 3", 9),
        ("2.0", "3 4", 12),
        ("2.5", "4 5", 20),
        ("1.3", "3 3", 8),
        ("1.3", "3 4", 16),
        ("1.3", "4 4", 15),
        ("1.3", "4 5", 19),
        ("1.2", "3 3 3", 8),
        ("1.2", "4 4 3", 15),
        ("1.3", "3 3", 10),
        ("1.2", "4 4 3", 17),
        ("1.3", "4 5", 21),
    ] {
        cases.push(ctf(
            &format!("a matrix {dim} in CTF {version} with {n} values"),
            &format!("id=\"a\" version=\"{version}\""),
            &matrix(f32s, dim, &values(n)),
        ));
    }
    cases.push(clf(
        "a matrix 3 4 in CLF 3",
        "id=\"a\" compCLFversion=\"3\"",
        &matrix(f32s, "3 4", &values(12)),
    ));
    cases.push(clf(
        "a matrix 3 3 3 in CLF 2",
        "id=\"a\" compCLFversion=\"2\"",
        &matrix(f32s, "3 3 3", &values(9)),
    ));

    // Bit depths.
    for (bd_in, bd_out) in [
        ("8i", "8i"),
        ("10i", "12i"),
        ("12i", "16i"),
        ("16i", "16f"),
        ("16f", "32f"),
        ("32f", "10I"),
        ("32F", "8i"),
        ("8u", "32f"),
        ("32f", "10ui"),
        ("", "32f"),
    ] {
        cases.push(ctf(
            &format!("a matrix from {bd_in} to {bd_out}"),
            "id=\"a\" version=\"1.3\"",
            &matrix(
                &format!("inBitDepth=\"{bd_in}\" outBitDepth=\"{bd_out}\""),
                "3 4",
                &values(12),
            ),
        ));
    }
    cases.extend([
        ctf(
            "a matrix without an inBitDepth",
            "id=\"a\"",
            &matrix("outBitDepth=\"32f\"", "3 3 3", &values(9)),
        ),
        ctf(
            "a matrix without bit depths",
            "id=\"a\"",
            &matrix("id=\"m\"", "3 3 3", &values(9)),
        ),
        ctf(
            "two matrices whose bit depths don't match",
            "id=\"a\"",
            &format!(
                "{}{}",
                matrix(
                    "inBitDepth=\"32f\" outBitDepth=\"10i\"",
                    "3 3 3",
                    &values(9)
                ),
                matrix(
                    "inBitDepth=\"12i\" outBitDepth=\"32f\"",
                    "3 3 3",
                    &values(9)
                )
            ),
        ),
        ctf(
            "two matrices whose bit depths match",
            "id=\"a\"",
            &format!(
                "{}{}",
                matrix(
                    "inBitDepth=\"32f\" outBitDepth=\"10i\"",
                    "3 3 3",
                    &values(9)
                ),
                matrix(
                    "inBitDepth=\"10i\" outBitDepth=\"32f\"",
                    "3 3 3",
                    &values(9)
                )
            ),
        ),
        // The operator's attributes.
        ctf(
            "a matrix's id, name, bypass and an unknown attribute",
            "id=\"a\" version=\"2.0\"",
            &matrix(
                "id=\"m1\" name=\"the matrix\" bypass=\"true\" foo=\"1\" inBitDepth=\"32f\" \
                 outBitDepth=\"32f\"",
                "3 3",
                &values(9),
            ),
        ),
        clf(
            "a matrix's bypass in a CLF",
            "id=\"a\" compCLFversion=\"3\"",
            &matrix(
                "bypass=\"true\" inBitDepth=\"32f\" outBitDepth=\"32f\"",
                "3 3",
                &values(9),
            ),
        ),
        ctf(
            "a matrix's descriptions",
            "id=\"a\" version=\"2.0\"",
            "    <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\">\n\
             \x20       <Description>one</Description>\n\
             \x20       <Description>two</Description>\n\
             \x20       <Array dim=\"3 3\">1 0 0 0 1 0 0 0 1</Array>\n\
             \x20       <Foo/>\n\
             \x20   </Matrix>\n",
        ),
        // The array element.
        ctf(
            "a matrix without an array",
            "id=\"a\" version=\"2.0\"",
            "    <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\"/>\n",
        ),
        ctf(
            "a matrix with two arrays",
            "id=\"a\" version=\"2.0\"",
            "    <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\">\n\
             \x20       <Array dim=\"3 3\">1 0 0 0 1 0 0 0 1</Array>\n\
             \x20       <Array dim=\"3 3\">1 0 0 0 1 0 0 0 1</Array>\n\
             \x20   </Matrix>\n",
        ),
        ctf(
            "an array without dim",
            "id=\"a\" version=\"2.0\"",
            "    <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\">\n\
             \x20       <Array foo=\"3 3\">1 0 0 0 1 0 0 0 1</Array>\n\
             \x20   </Matrix>\n",
        ),
        ctf(
            "an array's unknown attribute",
            "id=\"a\" version=\"2.0\"",
            "    <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\">\n\
             \x20       <Array foo=\"1\" dim=\"3 3\">1 0 0 0 1 0 0 0 1</Array>\n\
             \x20   </Matrix>\n",
        ),
        ctf(
            "an array's dim in upper case",
            "id=\"a\" version=\"2.0\"",
            "    <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\">\n\
             \x20       <Array DIM=\"3 3\">1 0 0 0 1 0 0 0 1</Array>\n\
             \x20   </Matrix>\n",
        ),
        ctf(
            "an array's dim twice",
            "id=\"a\" version=\"2.0\"",
            "    <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\">\n\
             \x20       <Array dim=\"4 4\" Dim=\"3 3\">1 0 0 0 1 0 0 0 1</Array>\n\
             \x20   </Matrix>\n",
        ),
        ctf(
            "an array's bad dim",
            "id=\"a\" version=\"2.0\"",
            &matrix(f32s, "3 x", &values(9)),
        ),
        ctf(
            "an array's negative dim",
            "id=\"a\" version=\"2.0\"",
            &matrix(f32s, "-3 3", &values(9)),
        ),
        ctf(
            "an array's empty dim",
            "id=\"a\" version=\"2.0\"",
            &matrix(f32s, "", &values(9)),
        ),
        ctf(
            "an array's long bad dim",
            "id=\"a\" version=\"2.0\"",
            &matrix(f32s, &"3 ".repeat(100), &values(9)),
        ),
        ctf(
            "an array's bad value",
            "id=\"a\" version=\"2.0\"",
            &matrix(f32s, "3 3", "1 0 0 0 1 x 0 0 1"),
        ),
        ctf(
            "an array's values with inf and nan",
            "id=\"a\" version=\"2.0\"",
            &matrix(f32s, "3 3", "1 0 0 0 inf 0 0 0 nan"),
        ),
        ctf(
            "an array's values over several lines and tabs",
            "id=\"a\" version=\"2.0\"",
            &matrix(f32s, "3 3", "1\t0 0\n\n 0 1e0 0\n0 0\t\t1\n"),
        ),
        ctf(
            "an array's long bad values",
            "id=\"a\" version=\"2.0\"",
            &matrix(
                f32s,
                "3 3",
                &format!("1 0 0 0 1 0 0 0 1{}", " x".repeat(200)),
            ),
        ),
        ctf(
            "an array of a non-invertible inverse",
            "id=\"a\" version=\"2.0\"",
            &matrix(f32s, "3 3", "0 0 0 0 0 0 0 0 0"),
        ),
        ctf(
            "an Array under a Range",
            "id=\"a\" version=\"2.0\"",
            &range(f32s, "        <Array dim=\"3 3\"/>\n"),
        ),
        ctf(
            "an Array under an Array",
            "id=\"a\" version=\"2.0\"",
            "    <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\">\n\
             \x20       <Array dim=\"3 3\">1 0 0 0 1 0 0 0 1<Array/></Array>\n\
             \x20   </Matrix>\n",
        ),
        ctf(
            "text in a matrix",
            "id=\"a\" version=\"2.0\"",
            "    <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\">x\n\
             \x20       <Array dim=\"3 3\">1 0 0 0 1 0 0 0 1</Array>\n\
             \x20   </Matrix>\n",
        ),
        // Ranges.
        ctf(
            "a range",
            "id=\"a\" version=\"1.7\"",
            &range(
                "inBitDepth=\"10i\" outBitDepth=\"12i\"",
                "        <minInValue>64</minInValue>\n\
                 \x20       <maxInValue>940</maxInValue>\n\
                 \x20       <minOutValue>256</minOutValue>\n\
                 \x20       <maxOutValue>3760</maxOutValue>\n",
            ),
        ),
        ctf(
            "a range with its minimums only",
            "id=\"a\" version=\"2.0\"",
            &range(
                f32s,
                "        <minInValue>0.1</minInValue>\n        <minOutValue>0.2</minOutValue>\n",
            ),
        ),
        ctf(
            "a range with its maximums only",
            "id=\"a\" version=\"2.0\"",
            &range(
                "inBitDepth=\"16f\" outBitDepth=\"8i\"",
                "        <maxInValue>0.9</maxInValue>\n        <maxOutValue>200</maxOutValue>\n",
            ),
        ),
        ctf(
            "a range without values",
            "id=\"a\" version=\"2.0\"",
            &range(f32s, ""),
        ),
        ctf(
            "a range with one minimum",
            "id=\"a\" version=\"2.0\"",
            &range(f32s, "        <minInValue>0.1</minInValue>\n"),
        ),
        ctf(
            "a noClamp range",
            "id=\"a\" version=\"2.0\"",
            &range(
                "inBitDepth=\"8i\" outBitDepth=\"32f\" style=\"noClamp\"",
                "        <minInValue>16</minInValue>\n\
                 \x20       <maxInValue>235</maxInValue>\n\
                 \x20       <minOutValue>-0.05</minOutValue>\n\
                 \x20       <maxOutValue>1.05</maxOutValue>\n",
            ),
        ),
        ctf(
            "a noClamp range in CTF 1.6",
            "id=\"a\" version=\"1.6\"",
            &range(
                "inBitDepth=\"8i\" outBitDepth=\"32f\" style=\"noClamp\"",
                "        <minInValue>16</minInValue>\n\
                 \x20       <maxInValue>235</maxInValue>\n\
                 \x20       <minOutValue>-0.05</minOutValue>\n\
                 \x20       <maxOutValue>1.05</maxOutValue>\n",
            ),
        ),
        ctf(
            "a NOCLAMP range",
            "id=\"a\" version=\"2.0\"",
            &range(
                "inBitDepth=\"32f\" outBitDepth=\"32f\" STYLE=\"NOCLAMP\"",
                "        <minInValue>0</minInValue>\n\
                 \x20       <maxInValue>1</maxInValue>\n\
                 \x20       <minOutValue>0.5</minOutValue>\n\
                 \x20       <maxOutValue>2</maxOutValue>\n",
            ),
        ),
        ctf(
            "a noClamp range without maximums",
            "id=\"a\" version=\"2.0\"",
            &range(
                "inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"noClamp\"",
                "        <minInValue>0</minInValue>\n        <minOutValue>0.5</minOutValue>\n",
            ),
        ),
        ctf(
            "a clamp style range",
            "id=\"a\" version=\"2.0\"",
            &range(
                "inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"Clamp\"",
                "        <minInValue>0</minInValue>\n        <minOutValue>0.5</minOutValue>\n",
            ),
        ),
        ctf(
            "an unknown range style",
            "id=\"a\" version=\"2.0\"",
            &range(
                "inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"foo\"",
                "        <minInValue>0</minInValue>\n        <minOutValue>0.5</minOutValue>\n",
            ),
        ),
        ctf(
            "a range value of two numbers",
            "id=\"a\" version=\"2.0\"",
            &range(f32s, "        <minInValue>0 1</minInValue>\n"),
        ),
        ctf(
            "a bad range value",
            "id=\"a\" version=\"2.0\"",
            &range(f32s, "        <minInValue>abc</minInValue>\n"),
        ),
        ctf(
            "an empty range value",
            "id=\"a\" version=\"2.0\"",
            &range(
                f32s,
                "        <minInValue></minInValue>\n        <minOutValue>0.5</minOutValue>\n",
            ),
        ),
        ctf(
            "a range value's attributes",
            "id=\"a\" version=\"2.0\"",
            &range(
                f32s,
                "        <minInValue a=\"1\" b=\"2\">0</minInValue>\n\
                 \x20       <MINOUTVALUE>0.5</MINOUTVALUE>\n",
            ),
        ),
        ctf(
            "a range value twice",
            "id=\"a\" version=\"2.0\"",
            &range(
                f32s,
                "        <minInValue>0</minInValue>\n        <minInValue>0.25</minInValue>\n\
                 \x20       <minOutValue>0.5</minOutValue>\n",
            ),
        ),
        ctf(
            "a range value under the ProcessList",
            "id=\"a\" version=\"2.0\"",
            "    <minInValue>0</minInValue>\n",
        ),
        ctf(
            "a range value under a matrix",
            "id=\"a\" version=\"2.0\"",
            "    <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\">\n\
             \x20       <minInValue>0</minInValue>\n\
             \x20       <Array dim=\"3 3\">1 0 0 0 1 0 0 0 1</Array>\n\
             \x20   </Matrix>\n",
        ),
        ctf(
            "a range then a matrix",
            "id=\"a\" version=\"2.0\"",
            &format!(
                "{}{}",
                range(
                    "inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"noClamp\"",
                    "        <minInValue>0</minInValue>\n\
                     \x20       <maxInValue>1</maxInValue>\n\
                     \x20       <minOutValue>0.5</minOutValue>\n\
                     \x20       <maxOutValue>2</maxOutValue>\n",
                ),
                matrix(f32s, "3 4", &values(12))
            ),
        ),
        ctf(
            "a range in CTF 1.2",
            "id=\"a\" version=\"1.2\"",
            &range(
                "inBitDepth=\"16i\" outBitDepth=\"16i\" style=\"noClamp\"",
                "        <minInValue>0</minInValue>\n\
                 \x20       <maxInValue>65535</maxInValue>\n\
                 \x20       <minOutValue>100</minOutValue>\n\
                 \x20       <maxOutValue>200</maxOutValue>\n",
            ),
        ),
    ]);

    // Upstream's files of matrices and ranges.
    for name in [
        "matrix_example4x4.ctf",
        "matrix_offsets_example.ctf",
        "matrix_offsets_example_1_3.ctf",
        "matrix_example_1_3_3x3.ctf",
        "matrix_example_1_3_4x4.ctf",
        "matrix_example_1_3_offsets.ctf",
        "matrix_example_1_3_alpha_offsets.ctf",
        "clf/matrix_windows.clf",
        "clf/matrix_no_newlines.clf",
        "clf/matrix_example_utf8.clf",
        "clf/info_example.clf",
        "process_list_valid_version.ctf",
        "process_list_version_revision.ctf",
        "process_list_no_version.ctf",
        "info_version_without.ctf",
        "info_version_valid_minor.ctf",
        "clf/illegal/transform_element_end_missing.clf",
        "clf/illegal/transform_corrupted_tag.clf",
        "clf/illegal/array_missing_values.clf",
        "clf/illegal/array_bad_value.clf",
        "clf/illegal/array_bad_dimension.clf",
        "clf/illegal/array_too_many_values.clf",
        "clf/illegal/matrix_end_missing.clf",
        "clf/illegal/transform_bad_outdepth.clf",
        "clf/range.clf",
        "clf/range_test1_clamp.clf",
        "clf/range_test1_noclamp.clf",
        "clf/range_test2.clf",
        "clf/illegal/range_nonmatching_clamp.clf",
        "clf/illegal/range_empty.clf",
        "clf/illegal/range_bad_noclamp.clf",
        "clf/illegal/range_bad_values.clf",
    ] {
        cases.push(file_case(
            name,
            &upstream_file(name),
            Interpolation::Default,
            TransformDirection::Forward,
        ));
    }
    cases
}

/// The processors of the Matrix and Range readers' files, in both directions.
#[test]
fn matrix_and_range_files_read_as_in_the_wheel() {
    check_processors(&op_cases());
}

/// The warnings of reading the Matrix and Range readers' files, message by message.
#[test]
fn matrix_and_range_files_log_as_in_the_wheel() {
    cdl_common::check_logs(&op_cases());
}
