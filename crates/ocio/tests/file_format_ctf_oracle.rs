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
use common::transforms::{Case, check_processors, direction_name};
use ocio::{Config, Interpolation, TransformDirection};

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
        // Text of another format: the format check reads its lines and finds no ProcessList.
        case(
            "a file of another format",
            "lut.ctf",
            b"3DMESH\nMesh 4 12\n0 64 128 1023\n",
        ),
        // The same through the readers of its own extension, which fail, then every format.
        case(
            "a .3dl that fails, read by every format",
            "lut.3dl",
            b"3DMESH\nMesh 4 12\nLUT8\ngamma 1.0\n",
        ),
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

/// An operator `op` of the attributes `attrs` holding `body`.
fn op(op: &str, attrs: &str, body: &str) -> String {
    format!("    <{op} {attrs}>\n{body}    </{op}>\n")
}

/// The Gamma, Exponent, Log and ASC_CDL readers' files.
fn op_cases_e() -> Vec<Case> {
    let f32s = "inBitDepth=\"32f\" outBitDepth=\"32f\"";
    let mut cases = Vec::new();

    // Gamma and Exponent.
    for (label, version, tag, style, params) in [
        (
            "basicFwd",
            "1.4",
            "Gamma",
            "basicFwd",
            "<GammaParams gamma=\"2.2\"/>",
        ),
        (
            "basicRev by channel",
            "1.4",
            "Gamma",
            "basicRev",
            "<GammaParams channel=\"R\" gamma=\"2.2\"/><GammaParams channel=\"g\" gamma=\"2.4\"/>\
             <GammaParams channel=\"B\" gamma=\"2.6\"/>",
        ),
        (
            "alpha in CTF 1.4",
            "1.4",
            "Gamma",
            "basicFwd",
            "<GammaParams channel=\"A\" gamma=\"2.2\"/>",
        ),
        (
            "alpha in CTF 1.5",
            "1.5",
            "Gamma",
            "basicFwd",
            "<GammaParams gamma=\"2.2\"/><GammaParams channel=\"A\" gamma=\"1.5\"/>",
        ),
        (
            "an unknown channel",
            "1.5",
            "Gamma",
            "basicFwd",
            "<GammaParams channel=\"X\" gamma=\"2.2\"/>",
        ),
        (
            "moncurveFwd",
            "1.5",
            "Gamma",
            "moncurveFwd",
            "<GammaParams gamma=\"2.4\" offset=\"0.055\"/>",
        ),
        (
            "moncurveRev without its offset",
            "1.5",
            "Gamma",
            "moncurveRev",
            "<GammaParams gamma=\"2.4\"/>",
        ),
        (
            "basicFwd with an offset",
            "1.5",
            "Gamma",
            "basicFwd",
            "<GammaParams gamma=\"2.4\" offset=\"0.055\"/>",
        ),
        (
            "basicFwd without its gamma",
            "1.5",
            "Gamma",
            "basicFwd",
            "<GammaParams foo=\"1\"/>",
        ),
        (
            "an unknown style",
            "1.5",
            "Gamma",
            "foo",
            "<GammaParams gamma=\"2.2\"/>",
        ),
        (
            "basicMirrorFwd in CTF 1.8",
            "1.8",
            "Gamma",
            "basicMirrorFwd",
            "<GammaParams gamma=\"2.2\"/>",
        ),
        (
            "basicMirrorFwd in CTF 2.0",
            "2.0",
            "Gamma",
            "basicMirrorFwd",
            "<GammaParams gamma=\"2.2\"/>",
        ),
        (
            "monCurveMirrorRev in CTF 2.0",
            "2.0",
            "Exponent",
            "monCurveMirrorRev",
            "<ExponentParams exponent=\"3\" offset=\"0.16\"/>",
        ),
        (
            "basicPassThruFwd with alpha in CTF 2.0",
            "2.0",
            "Exponent",
            "basicPassThruFwd",
            "<ExponentParams exponent=\"2\"/><ExponentParams channel=\"a\" exponent=\"1.2\"/>",
        ),
        (
            "a gamma out of bounds",
            "2.0",
            "Gamma",
            "basicFwd",
            "<GammaParams gamma=\"0.001\"/>",
        ),
        (
            "a moncurve offset out of bounds",
            "2.0",
            "Gamma",
            "moncurveFwd",
            "<GammaParams gamma=\"2\" offset=\"0.95\"/>",
        ),
        (
            "a bad gamma",
            "2.0",
            "Gamma",
            "basicFwd",
            "<GammaParams gamma=\"x\"/>",
        ),
        (
            "two gammas",
            "2.0",
            "Gamma",
            "basicFwd",
            "<GammaParams gamma=\"1 2\"/>",
        ),
        (
            "GammaParams under an Exponent",
            "2.0",
            "Exponent",
            "basicFwd",
            "<GammaParams gamma=\"2\"/><ExponentParams exponent=\"2\"/>",
        ),
        (
            "ExponentParams under a Gamma",
            "2.0",
            "Gamma",
            "basicFwd",
            "<ExponentParams exponent=\"2\"/><GammaParams gamma=\"2\"/>",
        ),
        ("no params", "2.0", "Gamma", "basicFwd", ""),
        (
            "text in params",
            "2.0",
            "Gamma",
            "basicFwd",
            "<GammaParams gamma=\"2\">x</GammaParams>",
        ),
    ] {
        cases.push(ctf(
            &format!("a {tag}: {label}"),
            &format!("id=\"a\" version=\"{version}\""),
            &op(
                tag,
                &format!("{f32s} style=\"{style}\""),
                &format!("        {params}\n"),
            ),
        ));
    }
    cases.extend([
        ctf(
            "a Gamma without a style",
            "id=\"a\" version=\"2.0\"",
            &op("Gamma", f32s, "        <GammaParams gamma=\"2\"/>\n"),
        ),
        clf(
            "an Exponent in CLF 3 with alpha",
            "id=\"a\" compCLFversion=\"3\"",
            &op(
                "Exponent",
                &format!("{f32s} style=\"basicRev\""),
                "        <ExponentParams exponent=\"2.6\"/>\n\
                 \x20       <ExponentParams channel=\"A\" exponent=\"1.7\"/>\n",
            ),
        ),
        clf(
            "an Exponent in CLF 3",
            "id=\"a\" compCLFversion=\"3\"",
            &op(
                "Exponent",
                &format!("{f32s} style=\"monCurveFwd\" bypass=\"true\""),
                "        <ExponentParams channel=\"R\" exponent=\"2.4\" offset=\"0.055\"/>\n",
            ),
        ),
    ]);

    // Log.
    let cineon = "gamma=\"0.6\" refWhite=\"685\" refBlack=\"95\" highlight=\"1\" shadow=\"0\"";
    for (label, version, style, params) in [
        ("log10 in CTF 1.3", "1.3", "log10", String::new()),
        ("antiLog2 in CTF 1.3", "1.3", "antiLog2", String::new()),
        (
            "linToLog in CTF 1.3",
            "1.3",
            "linToLog",
            format!("<LogParams {cineon}/>"),
        ),
        (
            "logToLin by channel in CTF 1.3",
            "1.3",
            "logToLin",
            format!(
                "<LogParams channel=\"R\" {cineon}/><LogParams channel=\"G\" {cineon}/>\
                 <LogParams channel=\"B\" {cineon}/>"
            ),
        ),
        (
            "linToLog without its shadow in CTF 1.3",
            "1.3",
            "linToLog",
            "<LogParams gamma=\"0.6\" refWhite=\"685\" refBlack=\"95\" highlight=\"1\"/>"
                .to_owned(),
        ),
        (
            "linToLog with an OCIO parameter in CTF 1.3",
            "1.3",
            "linToLog",
            format!("<LogParams {cineon} base=\"10\"/>"),
        ),
        (
            "an unknown channel in CTF 1.3",
            "1.3",
            "linToLog",
            format!("<LogParams channel=\"A\" {cineon}/>"),
        ),
        (
            "LogParams of log10",
            "1.3",
            "log10",
            format!("<LogParams {cineon}/>"),
        ),
        ("a bad style", "2.0", "logarithm", String::new()),
        ("log2 in CTF 2.0", "2.0", "log2", String::new()),
        ("antiLog10 in CTF 2.0", "2.0", "antiLog10", String::new()),
        (
            "linToLog with OCIO parameters",
            "2.0",
            "linToLog",
            "<LogParams linSideSlope=\"1.1\" linSideOffset=\"0.1\" logSideSlope=\"0.9\" \
             logSideOffset=\"0.2\" base=\"10\"/>"
                .to_owned(),
        ),
        (
            "linToLog with legacy parameters in CTF 2.0",
            "2.0",
            "linToLog",
            format!("<LogParams {cineon}/>"),
        ),
        (
            "mixed parameters",
            "2.0",
            "linToLog",
            format!("<LogParams {cineon} base=\"10\"/>"),
        ),
        (
            "a base mismatch",
            "2.0",
            "logToLin",
            "<LogParams channel=\"R\" base=\"2\"/><LogParams channel=\"G\" base=\"2.5\"/>"
                .to_owned(),
        ),
        (
            "a camera style",
            "2.0",
            "cameraLinToLog",
            "<LogParams linSideBreak=\"0.1\" linearSlope=\"1.5\" base=\"10\" \
             logSideSlope=\"0.25\" logSideOffset=\"0.4\"/>"
                .to_owned(),
        ),
        (
            "a camera style without its break",
            "2.0",
            "cameraLogToLin",
            "<LogParams logSideSlope=\"0.25\"/>".to_owned(),
        ),
        (
            "a break for a non-camera style",
            "2.0",
            "linToLog",
            "<LogParams linSideBreak=\"0.1\"/>".to_owned(),
        ),
        (
            "a linear slope for a non-camera style",
            "2.0",
            "linToLog",
            "<LogParams linearSlope=\"0.1\"/>".to_owned(),
        ),
        (
            "legacy parameters for a camera style",
            "2.0",
            "cameraLinToLog",
            format!("<LogParams linSideBreak=\"0.1\" {cineon}/>"),
        ),
        (
            "a bad base",
            "2.0",
            "linToLog",
            "<LogParams base=\"x\"/>".to_owned(),
        ),
        (
            "a log side slope of 0",
            "2.0",
            "linToLog",
            "<LogParams logSideSlope=\"0\"/>".to_owned(),
        ),
    ] {
        cases.push(ctf(
            &format!("a Log: {label}"),
            &format!("id=\"a\" version=\"{version}\""),
            &op(
                "Log",
                &format!("{f32s} style=\"{style}\""),
                &format!("        {params}\n"),
            ),
        ));
    }
    cases.extend([
        ctf(
            "a Log without a style",
            "id=\"a\" version=\"2.0\"",
            &op("Log", f32s, ""),
        ),
        clf(
            "a Log with legacy parameters in CLF 3",
            "id=\"a\" compCLFversion=\"3\"",
            &op(
                "Log",
                &format!("{f32s} style=\"linToLog\""),
                &format!("        <LogParams {cineon} linSideSlope=\"1.2\"/>\n"),
            ),
        ),
        clf(
            "a camera Log in CLF 3",
            "id=\"a\" compCLFversion=\"3\"",
            &op(
                "Log",
                &format!("{f32s} style=\"cameraLogToLin\""),
                "        <LogParams channel=\"R\" linSideBreak=\"0.01\"/>\n\
                 \x20       <LogParams channel=\"G\" linSideBreak=\"0.02\"/>\n\
                 \x20       <LogParams channel=\"B\" linSideBreak=\"0.03\"/>\n",
            ),
        ),
    ]);

    // ASC_CDL.
    let sop = "        <SOPNode>\n\
               \x20           <Slope>1.35 1.1 0.71</Slope>\n\
               \x20           <Offset>0.05 -0.23 0.11</Offset>\n\
               \x20           <Power>0.93 0.81 1.27</Power>\n\
               \x20       </SOPNode>\n";
    let sat = "        <SatNode>\n            <Saturation>1.239</Saturation>\n        </SatNode>\n";
    for style in [
        "Fwd",
        "Rev",
        "FwdNoClamp",
        "RevNoClamp",
        "v1.2_Fwd",
        "v1.2_Rev",
        "noClampFwd",
        "noClampRev",
        "FWD",
        "foo",
    ] {
        cases.push(clf(
            &format!("an ASC_CDL of style {style}"),
            "id=\"a\" compCLFversion=\"3\"",
            &op(
                "ASC_CDL",
                &format!("{f32s} style=\"{style}\""),
                &format!("{sop}{sat}"),
            ),
        ));
    }
    let sop_of = |slope: &str, offset: &str, power: &str| {
        format!("        <SOPNode>\n{slope}{offset}{power}        </SOPNode>\n",)
    };
    let slope = "            <Slope>1.35 1.1 0.71</Slope>\n";
    let offset = "            <Offset>0.05 -0.23 0.11</Offset>\n";
    let power = "            <Power>0.93 0.81 1.27</Power>\n";
    for (label, version, body) in [
        ("no style", "2.0", format!("{sop}{sat}")),
        ("no SOPNode", "2.0", sat.to_owned()),
        ("no SatNode", "2.0", sop.to_owned()),
        ("nothing", "2.0", String::new()),
        ("no slope", "2.0", sop_of("", offset, power)),
        ("no offset", "2.0", sop_of(slope, "", power)),
        ("no power", "2.0", sop_of(slope, offset, "")),
        (
            "two slope values",
            "2.0",
            sop_of("            <Slope>1.35 1.1</Slope>\n", offset, power),
        ),
        (
            "a bad slope",
            "2.0",
            sop_of("            <Slope>1.35 x 1</Slope>\n", offset, power),
        ),
        (
            "a power of 0",
            "2.0",
            sop_of(slope, offset, "            <Power>0 1 1</Power>\n"),
        ),
        (
            "two saturations",
            "2.0",
            "        <SatNode><Saturation>1 2</Saturation></SatNode>\n".to_owned(),
        ),
        (
            "a SATNode",
            "2.0",
            "        <SATNode><Saturation>0.8</Saturation></SATNode>\n".to_owned(),
        ),
        (
            "descriptions",
            "2.0",
            format!(
                "        <Description>cdl</Description>\n\
                 \x20       <InputDescription>in</InputDescription>\n\
                 \x20       <ViewingDescription>view</ViewingDescription>\n\
                 \x20       <SOPNode>\n\
                 \x20           <Description>sop</Description>\n{slope}{offset}{power}\
                 \x20       </SOPNode>\n\
                 \x20       <SatNode>\n\
                 \x20           <Description>sat</Description>\n\
                 \x20           <Saturation>1.1</Saturation>\n\
                 \x20       </SatNode>\n"
            ),
        ),
        ("a Slope under the ASC_CDL", "2.0", format!("{slope}{sop}")),
        (
            "a Saturation under the SOPNode",
            "2.0",
            sop_of(
                slope,
                offset,
                &format!("{power}            <Saturation>1</Saturation>\n"),
            ),
        ),
        ("a SOPNode in CTF 1.3", "1.3", format!("{sop}{sat}")),
    ] {
        cases.push(ctf(
            &format!("an ASC_CDL: {label}"),
            &format!("id=\"a\" version=\"{version}\""),
            &op("ASC_CDL", &format!("{f32s} style=\"Fwd\""), &body),
        ));
    }
    cases.push(ctf(
        "an ASC_CDL without a style",
        "id=\"a\" version=\"2.0\"",
        &op("ASC_CDL", f32s, &format!("{sop}{sat}")),
    ));
    cases.push(ctf(
        "operators in a row",
        "id=\"a\" version=\"2.0\"",
        &format!(
            "{}{}{}",
            op("Log", &format!("{f32s} style=\"log10\""), ""),
            op(
                "Gamma",
                &format!("{f32s} style=\"basicFwd\""),
                "        <GammaParams gamma=\"2.2\"/>\n",
            ),
            op(
                "ASC_CDL",
                &format!("{f32s} style=\"Rev\""),
                &format!("{sop}{sat}")
            ),
        ),
    ));

    // Upstream's files.
    for name in [
        "gamma_test1.ctf",
        "gamma_test2.ctf",
        "gamma_test3.ctf",
        "gamma_test4.ctf",
        "gamma_test5.ctf",
        "gamma_test6.ctf",
        "gamma_alpha_test1.ctf",
        "gamma_alpha_test2.ctf",
        "gamma_alpha_test3.ctf",
        "gamma_alpha_test4.ctf",
        "gamma_alpha_test5.ctf",
        "gamma_alpha_test6.ctf",
        "clf/illegal/exponent_bad_value.clf",
        "clf/illegal/exponent_bad_param.clf",
        "clf/exponent_all_styles.clf",
        "clf/cdl_clamp_fwd.clf",
        "clf/cdl_missing_style.clf",
        "clf/cdl_all_styles.clf",
        "clf/illegal/cdl_bad_slope.clf",
        "clf/illegal/cdl_bad_sat.clf",
        "clf/illegal/cdl_bad_power.clf",
        "clf/illegal/cdl_missing_slope.clf",
        "clf/illegal/cdl_missing_offset.clf",
        "clf/illegal/cdl_missing_power.clf",
        "clf/illegal/cdl_bad_style.clf",
        "clf/cdl_missing_sop.clf",
        "clf/cdl_missing_sat.clf",
        "cdl_various.ctf",
        "clf/log_all_styles.clf",
        "log_logtolin.ctf",
        "log_logtolinv2.ctf",
        "log_lintolog_3chan.ctf",
        "clf/illegal/log_bad_style.clf",
        "clf/illegal/log_bad_version.clf",
        "clf/illegal/log_bad_param.clf",
        "clf/illegal/log_missing_breakpnt.clf",
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

/// The processors of the Gamma, Exponent, Log and ASC_CDL readers' files, in both
/// directions.
#[test]
fn gamma_log_and_cdl_files_read_as_in_the_wheel() {
    check_processors(&op_cases_e());
}

/// The warnings of reading the Gamma, Exponent, Log and ASC_CDL readers' files, message by
/// message.
#[test]
fn gamma_log_and_cdl_files_log_as_in_the_wheel() {
    cdl_common::check_logs(&op_cases_e());
}

/// A LUT1D (or another tag) of the attributes `attrs`, with an Array of `dim` and `values`
/// and `extra` elements before it.
fn lut(tag: &str, attrs: &str, extra: &str, dim: &str, values: &str) -> String {
    format!(
        "    <{tag} {attrs}>\n{extra}\
         \x20       <Array dim=\"{dim}\">\n{values}\n        </Array>\n\
         \x20   </{tag}>\n"
    )
}

/// `n` values from 0 to 1023, rising.
fn ramp(n: usize) -> String {
    (0..n)
        .map(|i| format!("{}", (i * 1023) / (n - 1)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The Lut1D and InverseLUT1D readers' files, and IndexMaps.
fn op_cases_f() -> Vec<Case> {
    let bd = "inBitDepth=\"10i\" outBitDepth=\"10i\"";
    let mut cases = Vec::new();
    for (label, version, tag, attrs, extra, dim, values) in [
        (
            "1 component",
            "1.3",
            "LUT1D",
            bd.to_owned(),
            "",
            "8 1",
            ramp(8),
        ),
        (
            "3 components",
            "1.3",
            "LUT1D",
            bd.to_owned(),
            "",
            "4 3",
            ramp(12),
        ),
        (
            "12i to 16f",
            "1.3",
            "LUT1D",
            "inBitDepth=\"12i\" outBitDepth=\"16f\"".to_owned(),
            "",
            "4 1",
            "0 0.25 0.5 1".to_owned(),
        ),
        (
            "linear interpolation",
            "1.3",
            "LUT1D",
            format!("{bd} interpolation=\"linear\""),
            "",
            "4 1",
            ramp(4),
        ),
        (
            "a bad interpolation",
            "1.3",
            "LUT1D",
            format!("{bd} interpolation=\"cubic\""),
            "",
            "4 1",
            ramp(4),
        ),
        (
            "hueAdjust in CTF 1.3",
            "1.3",
            "LUT1D",
            format!("{bd} hueAdjust=\"dw3\""),
            "",
            "4 3",
            ramp(12),
        ),
        (
            "hueAdjust in CTF 1.4",
            "1.4",
            "LUT1D",
            format!("{bd} hueAdjust=\"dw3\""),
            "",
            "4 3",
            ramp(12),
        ),
        (
            "a bad hueAdjust",
            "1.7",
            "LUT1D",
            format!("{bd} hueAdjust=\"dw2\""),
            "",
            "4 3",
            ramp(12),
        ),
        (
            "raw halfs",
            "1.7",
            "LUT1D",
            "inBitDepth=\"10i\" outBitDepth=\"16f\" rawHalfs=\"true\"".to_owned(),
            "",
            "4 1",
            "0 15360 48128 31744".to_owned(),
        ),
        (
            "raw halfs out of range",
            "1.7",
            "LUT1D",
            "inBitDepth=\"10i\" outBitDepth=\"16f\" rawHalfs=\"true\"".to_owned(),
            "",
            "12 1",
            "70000 -1 -65536.5 1e10 -3e9 nan inf -inf 65535.9 0.9 -0.5 131071".to_owned(),
        ),
        (
            "a bad halfDomain",
            "1.7",
            "LUT1D",
            format!("{bd} halfDomain=\"yes\""),
            "",
            "4 1",
            ramp(4),
        ),
        ("dim 4 2", "1.7", "LUT1D", bd.to_owned(), "", "4 2", ramp(8)),
        ("dim 1 1", "1.7", "LUT1D", bd.to_owned(), "", "1 1", ramp(2)),
        ("dim 4", "1.7", "LUT1D", bd.to_owned(), "", "4", ramp(4)),
        (
            "dim 300001 1",
            "1.7",
            "LUT1D",
            bd.to_owned(),
            "",
            "300001 1",
            ramp(2),
        ),
        (
            "too few values",
            "1.7",
            "LUT1D",
            bd.to_owned(),
            "",
            "4 3",
            ramp(8),
        ),
        (
            "too many values",
            "1.7",
            "LUT1D",
            bd.to_owned(),
            "",
            "4 3",
            ramp(13),
        ),
        (
            "too many values of 1 component",
            "1.7",
            "LUT1D",
            bd.to_owned(),
            "",
            "4 1",
            ramp(5),
        ),
        // IndexMaps.
        (
            "an IndexMap in CTF 1.7",
            "1.7",
            "LUT1D",
            bd.to_owned(),
            "        <IndexMap dim=\"2\">64.5@0 940 @ 7</IndexMap>\n",
            "8 1",
            ramp(8),
        ),
        (
            "an IndexMap in CTF 1.4",
            "1.4",
            "LUT1D",
            bd.to_owned(),
            "        <IndexMap dim=\"2\">64.5@0 940@7</IndexMap>\n",
            "8 1",
            ramp(8),
        ),
        (
            "an IndexMap in CTF 2.0",
            "2.0",
            "LUT1D",
            bd.to_owned(),
            "        <IndexMap dim=\"2\">64.5@0 940@7</IndexMap>\n",
            "8 1",
            ramp(8),
        ),
        (
            "an IndexMap of 3 entries",
            "1.7",
            "LUT1D",
            bd.to_owned(),
            "        <IndexMap dim=\"3\">0@0 64.5@1 940@7</IndexMap>\n",
            "8 1",
            ramp(8),
        ),
        (
            "two IndexMaps",
            "1.7",
            "LUT1D",
            bd.to_owned(),
            "        <IndexMap dim=\"2\">0@0 940@7</IndexMap>\n\
             \x20       <IndexMap dim=\"2\">0@0 940@7</IndexMap>\n",
            "8 1",
            ramp(8),
        ),
        (
            "an IndexMap's bad dim",
            "1.7",
            "LUT1D",
            bd.to_owned(),
            "        <IndexMap dim=\"2 2\">0@0 940@7</IndexMap>\n",
            "8 1",
            ramp(8),
        ),
        (
            "an IndexMap without dim",
            "1.7",
            "LUT1D",
            bd.to_owned(),
            "        <IndexMap foo=\"2\">0@0 940@7</IndexMap>\n",
            "8 1",
            ramp(8),
        ),
        (
            "an IndexMap's bad values",
            "1.7",
            "LUT1D",
            bd.to_owned(),
            "        <IndexMap dim=\"2\">0@0 x@7</IndexMap>\n",
            "8 1",
            ramp(8),
        ),
        (
            "an IndexMap's half pair",
            "1.7",
            "LUT1D",
            bd.to_owned(),
            "        <IndexMap dim=\"2\">0@0 940</IndexMap>\n",
            "8 1",
            ramp(8),
        ),
        (
            "an IndexMap's extra pair",
            "1.7",
            "LUT1D",
            bd.to_owned(),
            "        <IndexMap dim=\"2\">0@0 940@7 1000@7</IndexMap>\n",
            "8 1",
            ramp(8),
        ),
        (
            "an IndexMap's missing pair",
            "1.7",
            "LUT1D",
            bd.to_owned(),
            "        <IndexMap dim=\"2\">0@0</IndexMap>\n",
            "8 1",
            ramp(8),
        ),
        (
            "an IndexMap under a Matrix",
            "1.7",
            "Matrix",
            "inBitDepth=\"32f\" outBitDepth=\"32f\"".to_owned(),
            "        <IndexMap dim=\"2\">0@0 1@1</IndexMap>\n",
            "3 3 3",
            "1 0 0 0 1 0 0 0 1".to_owned(),
        ),
        // InverseLUT1D.
        (
            "an inverse",
            "1.3",
            "InverseLUT1D",
            bd.to_owned(),
            "",
            "8 1",
            ramp(8),
        ),
        (
            "an inverse from 16i",
            "1.7",
            "InverseLUT1D",
            "inBitDepth=\"16i\" outBitDepth=\"32f\" hueAdjust=\"dw3\"".to_owned(),
            "",
            "4 3",
            "0 0 0 1000 2000 3000 40000 50000 60000 65535 65535 65535".to_owned(),
        ),
        (
            "an inverse's bad rawHalfs",
            "1.7",
            "InverseLUT1D",
            format!("{bd} rawHalfs=\"false\""),
            "",
            "4 1",
            ramp(4),
        ),
        (
            "an inverse's bad hueAdjust",
            "1.7",
            "InverseLUT1D",
            format!("{bd} hueAdjust=\"x\""),
            "",
            "4 1",
            ramp(4),
        ),
        (
            "an inverse's raw halfs out of range",
            "2.0",
            "InverseLUT1D",
            "inBitDepth=\"16f\" outBitDepth=\"10i\" rawHalfs=\"true\"".to_owned(),
            "",
            "4 1",
            "0 15360 70000 15872".to_owned(),
        ),
        (
            "an inverse with too few values",
            "1.7",
            "InverseLUT1D",
            bd.to_owned(),
            "",
            "4 3",
            ramp(4),
        ),
        (
            "an inverse with an IndexMap",
            "1.7",
            "InverseLUT1D",
            bd.to_owned(),
            "        <IndexMap dim=\"2\">0@0 940@7</IndexMap>\n",
            "8 1",
            ramp(8),
        ),
    ] {
        cases.push(ctf(
            &format!("a {tag}: {label}"),
            &format!("id=\"a\" version=\"{version}\""),
            &lut(tag, &attrs, extra, dim, &values),
        ));
    }
    cases.push(clf(
        "a LUT1D in CLF 2 with an IndexMap",
        "id=\"a\" compCLFversion=\"2\"",
        &lut(
            "LUT1D",
            bd,
            "        <IndexMap dim=\"2\">64.5@0 940@7</IndexMap>\n",
            "8 1",
            &ramp(8),
        ),
    ));
    cases.push(clf(
        "a LUT1D with halfDomain",
        "id=\"a\" compCLFversion=\"3\"",
        &lut(
            "LUT1D",
            "inBitDepth=\"16f\" outBitDepth=\"16f\" halfDomain=\"true\" rawHalfs=\"true\"",
            "",
            "65536 1",
            &(0..65536u32)
                .map(|i| (i ^ 0x8000).to_string())
                .collect::<Vec<_>>()
                .join(" "),
        ),
    ));

    // Upstream's files.
    for name in [
        "lut1d_32_10i_10i.ctf",
        "lut1d_hue_adjust_test.ctf",
        "lut1d_hue_adjust_invalid_style.ctf",
        "lut3by1d_nan_infinity_example.clf",
        "clf/illegal/lut1d_half_domain_set_false.clf",
        "clf/illegal/lut1d_raw_half_set_false.clf",
        "clf/lut1d_half_domain_raw_half_set.clf",
        "clf/illegal/lut1d_half_domain_missing_values.clf",
        "clf/xyz_to_rgb.clf",
        "clf/lut1d_long.clf",
        "lut1d_inv.ctf",
        "lut1d_inverse_halfdom_slog_fclut.ctf",
        "clf/difficult_syntax.clf",
        "difficult_test1_v1.ctf",
        "indexMap_test.ctf",
        "indexMap_test1_clfv2.clf",
        "indexMap_test3.ctf",
        "indexMap_test4_clfv2.clf",
        "indexMap_test_clfv2.clf",
        "clf/illegal/transform_missing_inbitdepth.clf",
        "clf/illegal/transform_missing_outbitdepth.clf",
        "clf/illegal/transform_bitdepth_mismatch.clf",
        "clf/inverseOf_id_test.clf",
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

/// The processors of the Lut1D and InverseLUT1D readers' files, in both directions.
#[test]
fn lut1d_files_read_as_in_the_wheel() {
    check_processors(&op_cases_f());
}

/// The warnings of reading the Lut1D and InverseLUT1D readers' files, message by message.
#[test]
fn lut1d_files_log_as_in_the_wheel() {
    cdl_common::check_logs(&op_cases_f());
}

/// The Lut3D and InverseLUT3D readers' files.
fn op_cases_f2() -> Vec<Case> {
    let bd = "inBitDepth=\"10i\" outBitDepth=\"12i\"";
    let grid2 =
        "0 0 0 0 0 4095 0 4095 0 0 4095 4095 4095 0 0 4095 0 4095 4095 4095 0 4095 4095 4095";
    let mut cases = Vec::new();
    for (label, version, tag, attrs, extra, dim, values) in [
        (
            "a 2x2x2 grid",
            "1.6",
            "LUT3D",
            bd.to_owned(),
            "",
            "2 2 2 3",
            grid2.to_owned(),
        ),
        (
            "tetrahedral",
            "1.7",
            "LUT3D",
            format!("{bd} interpolation=\"tetrahedral\""),
            "",
            "2 2 2 3",
            grid2.to_owned(),
        ),
        (
            "trilinear",
            "2.0",
            "LUT3D",
            format!("{bd} interpolation=\"trilinear\""),
            "",
            "2 2 2 3",
            grid2.to_owned(),
        ),
        (
            "a bad interpolation",
            "2.0",
            "LUT3D",
            format!("{bd} interpolation=\"linear\""),
            "",
            "2 2 2 3",
            grid2.to_owned(),
        ),
        (
            "hueAdjust",
            "2.0",
            "LUT3D",
            format!("{bd} hueAdjust=\"dw3\""),
            "",
            "2 2 2 3",
            grid2.to_owned(),
        ),
        (
            "an IndexMap in CTF 1.7",
            "1.7",
            "LUT3D",
            bd.to_owned(),
            "        <IndexMap dim=\"2\">64@0 940@1</IndexMap>\n",
            "2 2 2 3",
            grid2.to_owned(),
        ),
        (
            "an IndexMap in CTF 1.6",
            "1.6",
            "LUT3D",
            bd.to_owned(),
            "        <IndexMap dim=\"2\">64@0 940@1</IndexMap>\n",
            "2 2 2 3",
            grid2.to_owned(),
        ),
        (
            "an IndexMap of 130 entries",
            "1.7",
            "LUT3D",
            bd.to_owned(),
            "        <IndexMap dim=\"130\">64@0 940@1</IndexMap>\n",
            "2 2 2 3",
            grid2.to_owned(),
        ),
        (
            "an IndexMap of 3 entries",
            "1.7",
            "LUT3D",
            bd.to_owned(),
            "        <IndexMap dim=\"3\">0@0 64@0 940@1</IndexMap>\n",
            "2 2 2 3",
            grid2.to_owned(),
        ),
        (
            "dim 2 2 2 4",
            "2.0",
            "LUT3D",
            bd.to_owned(),
            "",
            "2 2 2 4",
            grid2.to_owned(),
        ),
        (
            "dim 2 2 3 3",
            "2.0",
            "LUT3D",
            bd.to_owned(),
            "",
            "2 2 3 3",
            grid2.to_owned(),
        ),
        (
            "dim 1 1 1 3",
            "2.0",
            "LUT3D",
            bd.to_owned(),
            "",
            "1 1 1 3",
            "0 0 0".to_owned(),
        ),
        (
            "dim 130",
            "2.0",
            "LUT3D",
            bd.to_owned(),
            "",
            "130 130 130 3",
            grid2.to_owned(),
        ),
        (
            "dim 2 2 3",
            "2.0",
            "LUT3D",
            bd.to_owned(),
            "",
            "2 2 3",
            grid2.to_owned(),
        ),
        (
            "too few values",
            "2.0",
            "LUT3D",
            bd.to_owned(),
            "",
            "2 2 2 3",
            "0 0 0 0 0 4095".to_owned(),
        ),
        (
            "too many values",
            "2.0",
            "LUT3D",
            bd.to_owned(),
            "",
            "2 2 2 3",
            format!("{grid2} 1"),
        ),
        (
            "an inverse",
            "1.6",
            "InverseLUT3D",
            bd.to_owned(),
            "",
            "2 2 2 3",
            "0 0 0 0 0 1023 0 1023 0 0 1023 1023 1023 0 0 1023 0 1023 1023 1023 0 1023 1023 1023"
                .to_owned(),
        ),
        (
            "an inverse, tetrahedral",
            "2.0",
            "InverseLUT3D",
            "inBitDepth=\"32f\" outBitDepth=\"10i\" interpolation=\"tetrahedral\"".to_owned(),
            "",
            "2 2 2 3",
            "0 0 0 0 0 1 0 1 0 0 1 1 1 0 0 1 0 1 1 1 0 1 1 1".to_owned(),
        ),
        (
            "an inverse with too few values",
            "2.0",
            "InverseLUT3D",
            bd.to_owned(),
            "",
            "2 2 2 3",
            "0 0 0".to_owned(),
        ),
        (
            "an inverse with an IndexMap",
            "1.7",
            "InverseLUT3D",
            bd.to_owned(),
            "        <IndexMap dim=\"2\">64@0 940@1</IndexMap>\n",
            "2 2 2 3",
            grid2.to_owned(),
        ),
    ] {
        cases.push(ctf(
            &format!("a {tag}: {label}"),
            &format!("id=\"a\" version=\"{version}\""),
            &lut(tag, &attrs, extra, dim, &values),
        ));
    }

    // Upstream's files.
    for name in [
        "lut3d_example_Inv.ctf",
        "lut_inv_pairs.ctf",
        "opt_test2.ctf",
        "indexMap_test2_clfv2.clf",
        "clf/aces_to_video_with_look.clf",
        "clf/lut1d_lut3d_lut1d.clf",
        "clf/lut3d_17x17x17_10i_12i.clf",
        "clf/lut3d_as_matrix.clf",
        "clf/lut3d_bizarre.clf",
        "clf/lut3d_identity_12i_16f.clf",
        "clf/lut3d_preview_tier_test.clf",
        "clf/multiple_ops.clf",
        "clf/tabulation_support.clf",
        "clf/illegal/indexMap_test2.clf",
        "clf/illegal/lut3d_unequal_size.clf",
        "clf/illegal/unknown_elements.clf",
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

/// The processors of the Lut3D and InverseLUT3D readers' files, in both directions.
#[test]
fn lut3d_files_read_as_in_the_wheel() {
    check_processors(&op_cases_f2());
}

/// The warnings of reading the Lut3D and InverseLUT3D readers' files, message by message.
#[test]
fn lut3d_files_log_as_in_the_wheel() {
    cdl_common::check_logs(&op_cases_f2());
}

/// LUT files read with each FileTransform interpolation: a LUT without an interpolation of
/// its own takes the FileTransform's if it allows it (`HandleLUT`).
fn interpolation_cases() -> Vec<Case> {
    let b10 = "inBitDepth=\"10i\" outBitDepth=\"10i\"";
    let mut cases = Vec::new();
    let mut files = Vec::new();
    for (label, body) in [
        ("a LUT3D", lut("LUT3D", b10, "", "2 2 2 3", &ramp(24))),
        (
            "a tetrahedral LUT3D",
            lut(
                "LUT3D",
                &format!("{b10} interpolation=\"tetrahedral\""),
                "",
                "2 2 2 3",
                &ramp(24),
            ),
        ),
        (
            "a trilinear LUT3D",
            lut(
                "LUT3D",
                &format!("{b10} interpolation=\"trilinear\""),
                "",
                "2 2 2 3",
                &ramp(24),
            ),
        ),
        (
            "an InverseLUT3D",
            lut("InverseLUT3D", b10, "", "2 2 2 3", &ramp(24)),
        ),
        ("a LUT1D", lut("LUT1D", b10, "", "4 1", &ramp(4))),
        (
            "a linear LUT1D",
            lut(
                "LUT1D",
                &format!("{b10} interpolation=\"linear\""),
                "",
                "4 1",
                &ramp(4),
            ),
        ),
        (
            "an InverseLUT1D",
            lut("InverseLUT1D", b10, "", "4 1", &ramp(4)),
        ),
        (
            "a LUT1D and a LUT3D",
            format!(
                "{}{}",
                lut("LUT1D", b10, "", "4 1", &ramp(4)),
                lut("LUT3D", b10, "", "2 2 2 3", &ramp(24))
            ),
        ),
    ] {
        let dir = write_files(&[file(
            "lut.ctf",
            pl("id=\"a\" version=\"1.7\"", &body).as_bytes(),
        )]);
        files.push((label.to_owned(), format!("{dir}/lut.ctf")));
    }
    for name in [
        "clf/lut3d_17x17x17_10i_12i.clf",
        "clf/lut1d_example.clf",
        "lut3d_example_Inv.ctf",
    ] {
        files.push((name.to_owned(), upstream_file(name)));
    }
    for (label, path) in &files {
        for interp in [
            Interpolation::Nearest,
            Interpolation::Linear,
            Interpolation::Tetrahedral,
            Interpolation::Cubic,
            Interpolation::Best,
            Interpolation::Default,
        ] {
            cases.push(file_case(
                format!("{label}, {interp:?}"),
                path,
                interp,
                TransformDirection::Forward,
            ));
        }
    }
    cases
}

/// The processors of LUT files read with each FileTransform interpolation, in both
/// directions.
#[test]
fn file_interpolations_read_as_in_the_wheel() {
    check_processors(&interpolation_cases());
}

/// The warnings of reading LUT files with each FileTransform interpolation, message by
/// message.
#[test]
fn file_interpolations_log_as_in_the_wheel() {
    cdl_common::check_logs(&interpolation_cases());
}

/// Upstream's CLF examples, metadata text in parts, the versions that change a reader, and
/// the limits of LUT and IndexMap dimensions.
fn structure_cases() -> Vec<Case> {
    let b10 = "inBitDepth=\"10i\" outBitDepth=\"10i\"";
    let f32s = "inBitDepth=\"32f\" outBitDepth=\"32f\"";
    let lut1 = lut("LUT1D", b10, "", "4 1", &ramp(4));
    let im =
        |dim: &str, values: &str| format!("        <IndexMap dim=\"{dim}\">{values}</IndexMap>\n");
    let mut cases = Vec::new();

    // Upstream's files.
    for name in ["clf/lut1d_example.clf", "clf/matrix_3x4_example.clf"] {
        cases.push(file_case(
            name,
            &upstream_file(name),
            Interpolation::Default,
            TransformDirection::Forward,
        ));
    }

    // Text in parts: around an entity, on several lines, around an element.
    cases.push(ctf(
        "Info text in parts",
        "id=\"a\" version=\"1.7\"",
        &format!(
            "    <Info><R> a &amp; b </R><S>x<T/>y</S>\n        <U>line 1\n  line 2</U></Info>\n{lut1}"
        ),
    ));
    cases.push(ctf(
        "descriptors in parts",
        "id=\"a\" version=\"1.7\"",
        &format!(
            "    <InputDescriptor>a&amp;b</InputDescriptor>\n\
             \x20   <InputDescriptor> c </InputDescriptor>\n\
             \x20   <OutputDescriptor>d\ne</OutputDescriptor>\n\
             \x20   <OutputDescriptor>f&lt;</OutputDescriptor>\n{lut1}"
        ),
    ));

    // The Lut1D reader of each version: an IndexMap is a Range from CTF 1.5 on.
    for version in ["1.3", "1.4", "1.5", "1.6", "1.7"] {
        cases.push(ctf(
            &format!("a LUT1D with an IndexMap in CTF {version}"),
            &format!("id=\"a\" version=\"{version}\""),
            &lut("LUT1D", b10, &im("2", "64@0 940@3"), "4 1", &ramp(4)),
        ));
    }

    // The legacy Log reader up to CTF 1.8.
    for version in ["1.7", "1.8"] {
        cases.push(ctf(
            &format!("a Log in CTF {version}"),
            &format!("id=\"a\" version=\"{version}\""),
            &op("Log", &format!("{f32s} style=\"log2\""), ""),
        ));
    }

    // The limits of LUT dimensions.
    for (tag, dim, values) in [
        ("LUT1D", "300000 1", ramp(4)),
        ("LUT1D", "300001 1", ramp(4)),
        ("InverseLUT1D", "300000 1", ramp(4)),
        ("InverseLUT1D", "300001 1", ramp(4)),
        ("LUT3D", "129 129 129 3", ramp(24)),
        ("LUT3D", "130 130 130 3", ramp(24)),
        ("InverseLUT3D", "129 129 129 3", ramp(24)),
        ("InverseLUT3D", "130 130 130 3", ramp(24)),
    ] {
        cases.push(ctf(
            &format!("a {tag} of dimensions {dim}"),
            "id=\"a\" version=\"1.7\"",
            &lut(tag, f32s, "", dim, &values),
        ));
    }

    // IndexMaps of 1 and 2 entries.
    for (tag, dim, values) in [("LUT1D", "4 1", ramp(4)), ("LUT3D", "2 2 2 3", ramp(24))] {
        for (im_dim, im_values) in [("1", "0@0"), ("2", "0@0 1023@1")] {
            cases.push(ctf(
                &format!("a {tag} with an IndexMap of dimension {im_dim}"),
                "id=\"a\" version=\"1.7\"",
                &lut(tag, b10, &im(im_dim, im_values), dim, &values),
            ));
        }
    }
    cases
}

/// The processors of upstream's CLF examples, metadata text in parts, the versions that
/// change a reader and the limits of dimensions, in both directions.
#[test]
fn structure_files_read_as_in_the_wheel() {
    check_processors(&structure_cases());
}

/// The warnings of reading upstream's CLF examples, metadata text in parts, the versions
/// that change a reader and the limits of dimensions, message by message.
#[test]
fn structure_files_log_as_in_the_wheel() {
    cdl_common::check_logs(&structure_cases());
}

/// DynamicParameter elements under the ported operators.
fn dynamic_parameter_cases() -> Vec<Case> {
    let b10 = "inBitDepth=\"10i\" outBitDepth=\"10i\"";
    let f32s = "inBitDepth=\"32f\" outBitDepth=\"32f\"";
    let dp = |attrs: &str| format!("        <DynamicParameter {attrs}/>\n");
    let lut1 = |extra: &str| lut("LUT1D", b10, extra, "4 1", &ramp(4));
    let mut cases = Vec::new();

    // CTF files: (label, version, body).
    let mut files = vec![
        (
            "LOOK_SWITCH on a LUT1D".to_owned(),
            "2.0",
            lut1(&dp("param=\"LOOK_SWITCH\"")),
        ),
        (
            "LOOK_SWITCH of another case".to_owned(),
            "2.0",
            lut1(&dp("PARAM=\"look_switch\"")),
        ),
        (
            "LOOK_SWITCH after the Array".to_owned(),
            "2.0",
            format!(
                "    <LUT1D {b10}>\n        <Array dim=\"4 1\">{}</Array>\n{}    </LUT1D>\n",
                ramp(4),
                dp("param=\"LOOK_SWITCH\"")
            ),
        ),
        (
            "LOOK_SWITCH twice".to_owned(),
            "1.7",
            lut1(&format!(
                "{}{}",
                dp("param=\"LOOK_SWITCH\""),
                dp("param=\"LOOK_SWITCH\" Param=\"LOOK_SWITCH\"")
            )),
        ),
        (
            "LOOK_SWITCH then EXPOSURE".to_owned(),
            "2.0",
            lut1(&dp("param=\"LOOK_SWITCH\" Param=\"EXPOSURE\"")),
        ),
        ("no param".to_owned(), "2.0", lut1(&dp("other=\"1\""))),
        ("an empty param".to_owned(), "2.0", lut1(&dp("param=\"\""))),
        (
            "an unknown param".to_owned(),
            "2.0",
            lut1(&dp("param=\"FOO\"")),
        ),
        (
            "text".to_owned(),
            "2.0",
            lut1("        <DynamicParameter param=\"LOOK_SWITCH\">text</DynamicParameter>\n"),
        ),
        (
            "under an Array".to_owned(),
            "2.0",
            format!(
                "    <LUT1D {b10}>\n        <Array dim=\"4 1\">{}{}</Array>\n    </LUT1D>\n",
                ramp(4),
                dp("param=\"LOOK_SWITCH\"")
            ),
        ),
        (
            "under the ProcessList".to_owned(),
            "2.0",
            format!("{}{}", dp("param=\"LOOK_SWITCH\""), lut1("")),
        ),
        (
            "under a SOPNode".to_owned(),
            "2.0",
            op(
                "ASC_CDL",
                &format!("{f32s} style=\"Fwd\""),
                &format!(
                    "        <SOPNode>\n{}            <Slope>1 1 1</Slope>\n\
                     \x20           <Offset>0 0 0</Offset>\n\
                     \x20           <Power>1 1 1</Power>\n        </SOPNode>\n",
                    dp("param=\"LOOK_SWITCH\"")
                ),
            ),
        ),
    ];
    for param in [
        "EXPOSURE",
        "contrast",
        "GAMMA",
        "PRIMARY",
        "RGB_CURVE",
        "HUE_CURVE",
        "TONE",
    ] {
        files.push((
            format!("{param} on a LUT1D"),
            "2.0",
            lut1(&dp(&format!("param=\"{param}\""))),
        ));
    }
    for (tag, attrs, body) in [
        (
            "Matrix",
            f32s.to_owned(),
            "        <Array dim=\"3 3 3\">1 0 0 0 1 0 0 0 1</Array>\n".to_owned(),
        ),
        ("Range", f32s.to_owned(), String::new()),
        ("Log", format!("{f32s} style=\"log10\""), String::new()),
        (
            "Exponent",
            format!("{f32s} style=\"basicFwd\""),
            "        <ExponentParams exponent=\"2.2\"/>\n".to_owned(),
        ),
        ("ASC_CDL", format!("{f32s} style=\"Fwd\""), String::new()),
        (
            "LUT3D",
            b10.to_owned(),
            format!("        <Array dim=\"2 2 2 3\">{}</Array>\n", ramp(24)),
        ),
        (
            "InverseLUT1D",
            b10.to_owned(),
            format!("        <Array dim=\"4 1\">{}</Array>\n", ramp(4)),
        ),
    ] {
        for param in ["param=\"LOOK_SWITCH\"", "param=\"EXPOSURE\"", ""] {
            files.push((
                format!("[{param}] on a {tag}"),
                "2.0",
                op(tag, &attrs, &format!("{}{body}", dp(param))),
            ));
        }
    }
    for (label, version, body) in files {
        cases.push(ctf(
            &format!("a DynamicParameter: {label}"),
            &format!("id=\"a\" version=\"{version}\""),
            &body,
        ));
    }
    cases.push(clf(
        "a DynamicParameter in a CLF",
        "id=\"a\" compCLFversion=\"3\"",
        &lut1(&dp("param=\"LOOK_SWITCH\"")),
    ));
    cases
}

/// The processors of files with DynamicParameter elements, in both directions.
#[test]
fn dynamic_parameter_files_read_as_in_the_wheel() {
    check_processors(&dynamic_parameter_cases());
}

/// The warnings of reading files with DynamicParameter elements, message by message.
#[test]
fn dynamic_parameter_files_log_as_in_the_wheel() {
    cdl_common::check_logs(&dynamic_parameter_cases());
}

/// The ACES, ACESParams, FixedFunction and Function readers' files.
fn op_cases_g() -> Vec<Case> {
    let f32s = "inBitDepth=\"32f\" outBitDepth=\"32f\"";
    let i16 = "inBitDepth=\"16i\" outBitDepth=\"16i\"";
    let gamma = |g: &str| format!("        <ACESParams gamma=\"{g}\"/>\n");
    let mut cases = Vec::new();

    // CTF files: (label, version, operator, its attributes, its body).
    for (label, version, tag, attrs, body) in [
        // FixedFunction.
        (
            "a style with a parameter",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"Rec2100SurroundFwd\" params=\"0.8\""),
            String::new(),
        ),
        (
            "a style of another case",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"rec2100surroundrev\" params=\" 1.2 \""),
            String::new(),
        ),
        (
            "a style without parameters",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"hsv_to_rgb\""),
            String::new(),
        ),
        (
            "the gamut compression",
            "2.1",
            "FixedFunction",
            format!(
                "{f32s} style=\"GamutComp13Fwd\" params=\"1.147 1.264 1.312 0.815 0.803 0.88 1.2\""
            ),
            String::new(),
        ),
        (
            "a later style in CTF 2",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"Lin_TO_PQ\""),
            String::new(),
        ),
        (
            "the gamma log",
            "2.4",
            "FixedFunction",
            format!(
                "{f32s} style=\"Lin_TO_GammaLog\" \
                 params=\"0 0.25 0.5 1 0 2.718 0.17883277 0.807825590164 1 -0.07116723\""
            ),
            String::new(),
        ),
        (
            "an ACES 2 output transform",
            "2.4",
            "FixedFunction",
            format!(
                "{f32s} style=\"ACESOutputTransform20Inv\" \
                 params=\"1000 0.68 0.32 0.265 0.69 0.15 0.06 0.3127 0.329\""
            ),
            String::new(),
        ),
        (
            "an HSY style",
            "2.5",
            "FixedFunction",
            format!("{f32s} style=\"RGB_TO_HSY_LOG\""),
            String::new(),
        ),
        (
            "params before the style",
            "2",
            "FixedFunction",
            format!("{f32s} params=\"0.8\" style=\"Rec2100SurroundFwd\""),
            String::new(),
        ),
        (
            "two styles",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"HSV_to_RGB\" Style=\"RGB_to_HSV\""),
            String::new(),
        ),
        (
            "two params",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"Rec2100SurroundFwd\" params=\"0.8\" PARAMS=\"1.5\""),
            String::new(),
        ),
        (
            "too many parameters",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"Rec2100SurroundFwd\" params=\"0.8 2.0\""),
            String::new(),
        ),
        (
            "parameters a style doesn't take",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"RGB_TO_HSV\" params=\"0.8\""),
            String::new(),
        ),
        (
            "empty params",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"Rec2100SurroundFwd\" params=\"\""),
            String::new(),
        ),
        (
            "a parameter out of range",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"Rec2100SurroundFwd\" params=\"200\""),
            String::new(),
        ),
        (
            "a NaN parameter",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"Rec2100SurroundFwd\" params=\"nan\""),
            String::new(),
        ),
        (
            "an infinite parameter",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"Rec2100SurroundFwd\" params=\"inf\""),
            String::new(),
        ),
        (
            "illegal params",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"Rec2100SurroundFwd\" params=\"0.8 abc\""),
            String::new(),
        ),
        (
            "long illegal params",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"Rec2100SurroundFwd\" params=\"0.8 0.9 1.0 1.1 1.2 x\""),
            String::new(),
        ),
        (
            "a missing style",
            "2",
            "FixedFunction",
            format!("{f32s} params=\"0.8\""),
            String::new(),
        ),
        (
            "an unknown style",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"UnknownStyle\""),
            String::new(),
        ),
        (
            "an empty style",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"\""),
            String::new(),
        ),
        (
            "an unknown attribute",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"HSV_to_RGB\" gamma=\"2\""),
            String::new(),
        ),
        (
            "a description and an ACESParams",
            "2",
            "FixedFunction",
            format!("{f32s} style=\"Rec2100SurroundFwd\" params=\"0.8\""),
            format!(
                "        <Description>a surround</Description>\n{}",
                gamma("1.2")
            ),
        ),
        (
            "a FixedFunction in CTF 1.8",
            "1.8",
            "FixedFunction",
            format!("{f32s} style=\"HSV_to_RGB\""),
            String::new(),
        ),
        // ACES.
        (
            "a style without parameters",
            "1.5",
            "ACES",
            format!("{i16} style=\"RedMod03Rev\""),
            String::new(),
        ),
        (
            "a surround",
            "1.5",
            "ACES",
            format!("{i16} style=\"Surround\""),
            gamma("1.2"),
        ),
        (
            "a Rec.2100 surround",
            "2",
            "ACES",
            format!("{f32s} style=\"rec2100surroundfwd\""),
            gamma("0.8"),
        ),
        (
            "an inverse Rec.2100 surround",
            "1.5",
            "ACES",
            format!("{f32s} style=\"Rec2100SurroundRev\""),
            gamma("1.4"),
        ),
        (
            "a surround without ACESParams",
            "1.5",
            "ACES",
            format!("{i16} style=\"Surround\""),
            String::new(),
        ),
        (
            "two ACESParams",
            "1.5",
            "ACES",
            format!("{i16} style=\"Surround\""),
            format!("{}{}", gamma("1.2"), gamma("1.4")),
        ),
        (
            "an ACESParams without gamma",
            "1.5",
            "ACES",
            format!("{i16} style=\"Surround\""),
            "        <ACESParams wrongParam=\"1.2\"/>\n".to_owned(),
        ),
        (
            "an ACESParams without gamma, inverse",
            "1.5",
            "ACES",
            format!("{i16} style=\"Rec2100SurroundRev\""),
            "        <ACESParams/>\n".to_owned(),
        ),
        (
            "an ACESParams with an unknown attribute",
            "1.5",
            "ACES",
            format!("{i16} style=\"Surround\""),
            "        <ACESParams Gamma=\"1.1\" offset=\"2\"/>\n".to_owned(),
        ),
        (
            "a NaN gamma",
            "1.5",
            "ACES",
            format!("{i16} style=\"Surround\""),
            gamma("nan"),
        ),
        (
            "an illegal gamma",
            "1.5",
            "ACES",
            format!("{i16} style=\"Surround\""),
            gamma("1.2 1.3"),
        ),
        (
            "a gamma out of range",
            "1.5",
            "ACES",
            format!("{i16} style=\"Surround\""),
            gamma("0"),
        ),
        (
            "an ACESParams with text",
            "1.5",
            "ACES",
            format!("{i16} style=\"Surround\""),
            "        <ACESParams gamma=\"1.2\">some text</ACESParams>\n".to_owned(),
        ),
        (
            "an ACESParams a style doesn't take",
            "1.5",
            "ACES",
            format!("{i16} style=\"Glow03Fwd\""),
            gamma("1.2"),
        ),
        (
            "a second ACESParams without gamma",
            "1.5",
            "ACES",
            format!("{f32s} style=\"Rec2100SurroundFwd\""),
            format!("{}        <ACESParams/>\n", gamma("1.2")),
        ),
        (
            "an ACESParams before the style is known",
            "1.5",
            "ACES",
            format!("{i16} style=\"DarkToDim10\""),
            gamma("1.2"),
        ),
        (
            "a missing style",
            "1.5",
            "ACES",
            i16.to_owned(),
            String::new(),
        ),
        (
            "an unknown style",
            "1.5",
            "ACES",
            format!("{i16} style=\"Surround2\""),
            String::new(),
        ),
        (
            "params",
            "2",
            "ACES",
            format!("{i16} style=\"RedMod10Fwd\" params=\"1\""),
            String::new(),
        ),
        (
            "an ACES in CTF 1.4",
            "1.4",
            "ACES",
            format!("{i16} style=\"RedMod03Rev\""),
            String::new(),
        ),
        // Function.
        (
            "a style",
            "1.6",
            "Function",
            "inBitDepth=\"16i\" outBitDepth=\"32f\" style=\"HSV_to_RGB\"".to_owned(),
            String::new(),
        ),
        (
            "an ACES style",
            "2",
            "Function",
            format!("{f32s} style=\"GamutComp13Rev\""),
            String::new(),
        ),
        (
            "params",
            "2",
            "Function",
            format!("{f32s} style=\"Rec2100SurroundFwd\" params=\"0.8\""),
            String::new(),
        ),
        (
            "an ACESParams",
            "2",
            "Function",
            format!("{f32s} style=\"Rec2100SurroundFwd\""),
            gamma("0.8"),
        ),
        (
            "a missing style",
            "1.6",
            "Function",
            f32s.to_owned(),
            String::new(),
        ),
        (
            "an unknown style",
            "1.6",
            "Function",
            format!("{f32s} style=\"HSV\""),
            String::new(),
        ),
        (
            "a Function in CTF 1.5",
            "1.5",
            "Function",
            format!("{f32s} style=\"HSV_to_RGB\""),
            String::new(),
        ),
    ] {
        cases.push(ctf(
            &format!("a {tag}: {label}"),
            &format!("id=\"a\" version=\"{version}\""),
            &op(tag, &attrs, &body),
        ));
    }

    // CLF files.
    for (label, tag, attrs, body) in [
        (
            "a surround",
            "ACES",
            format!("{f32s} style=\"Surround\""),
            gamma("1.2"),
        ),
        (
            "a FixedFunction",
            "FixedFunction",
            format!("{f32s} style=\"HSV_to_RGB\""),
            String::new(),
        ),
        (
            "a Function",
            "Function",
            format!("{f32s} style=\"HSV_to_RGB\""),
            String::new(),
        ),
    ] {
        cases.push(clf(
            &format!("a CLF {tag}: {label}"),
            "id=\"a\" compCLFversion=\"3\"",
            &op(tag, &attrs, &body),
        ));
    }

    // Upstream's files.
    for name in ["deprecated_ops.ctf", "fixed_function.ctf"] {
        cases.push(file_case(
            name,
            &upstream_file(name),
            Interpolation::Default,
            TransformDirection::Forward,
        ));
    }
    cases
}

/// The processors of the ACES, FixedFunction and Function readers' files, in both
/// directions.
#[test]
fn fixed_function_files_read_as_in_the_wheel() {
    check_processors(&op_cases_g());
}

/// The warnings of reading the ACES, FixedFunction and Function readers' files, message by
/// message.
#[test]
fn fixed_function_files_log_as_in_the_wheel() {
    cdl_common::check_logs(&op_cases_g());
}

/// A curve element `tag` holding `body`.
fn curve(tag: &str, body: &str) -> String {
    format!("        <{tag}>\n{body}        </{tag}>\n")
}

/// A ControlPoints element of `values`.
fn points(values: &str) -> String {
    format!("            <ControlPoints>{values}</ControlPoints>\n")
}

/// A Slopes element of `values`.
fn slopes(values: &str) -> String {
    format!("            <Slopes>{values}</Slopes>\n")
}

/// The GradingRGBCurve reader's files.
fn op_cases_g2() -> Vec<Case> {
    let f32s = "inBitDepth=\"32f\" outBitDepth=\"32f\"";
    let style = |s: &str| format!("{f32s} style=\"{s}\"");
    let mut cases = Vec::new();

    // CTF files: (label, version, the GradingRGBCurve's attributes, its body).
    for (label, version, attrs, body) in [
        ("the log defaults", "2", style("log"), String::new()),
        ("the linear defaults", "2", style("linear"), String::new()),
        (
            "the video defaults, inverse",
            "2",
            style("videoRev"),
            String::new(),
        ),
        (
            "linear curves",
            "2",
            style("linear"),
            format!(
                "{}{}",
                curve("Red", &points(" -7 -6\n 0 0\n 7 7 ")),
                curve("Master", &points("-7 -7 0 0 7 7 16 10"))
            ),
        ),
        (
            "video curves with slopes",
            "2",
            style("video"),
            curve(
                "Master",
                &format!("{}{}", points("0 0 1 1 1.5 1.4"), slopes(" 0 0.7 1.1 ")),
            ),
        ),
        (
            "every log curve, inverse",
            "2",
            style("logRev"),
            format!(
                "{}{}{}{}",
                curve("Red", &points("0.015625 0 0.5 0.5 2 2")),
                curve("Green", &points("0.015625 0.1 2.5 0.5 3.5 1.5")),
                curve("Blue", &points("-4 -4 4.5 0.5 5 3")),
                curve("Master", &points("11 11 12.5 11.5 13.5 12.5 26.5 15"))
            ),
        ),
        (
            "curve names of another case",
            "2.5",
            style("LOG"),
            format!(
                "{}{}",
                curve("red", &points("0 0 1 2")),
                curve("MASTER", &points("0 0 1 0.5"))
            ),
        ),
        (
            "bypassLinToLog",
            "2",
            format!("{} bypassLinToLog=\"true\"", style("linear")),
            curve("Green", &points("-1 -1 0 0.5 1 1")),
        ),
        (
            "bypassLinToLog of another case",
            "2",
            format!("{} BypassLinToLog=\"TRUE\"", style("linearRev")),
            String::new(),
        ),
        (
            "bypassLinToLog false",
            "2",
            format!("{} bypassLinToLog=\"false\"", style("linear")),
            String::new(),
        ),
        (
            "bypassLinToLog before the style",
            "2",
            format!("{f32s} bypassLinToLog=\"true\" style=\"video\""),
            String::new(),
        ),
        ("an invalid style", "2", style("logx"), String::new()),
        ("an empty style", "2", style(""), String::new()),
        ("a missing style", "2", f32s.to_owned(), String::new()),
        (
            "two styles",
            "2",
            format!("{} Style=\"video\"", style("linear")),
            curve("Master", &points("0 0 0.5 0.3 1 1")),
        ),
        (
            "an unknown attribute",
            "2",
            format!("{} gamma=\"2\"", style("log")),
            String::new(),
        ),
        (
            "an odd number of values",
            "2",
            style("linear"),
            curve("Red", &points("-7 -6 0 0 7")),
        ),
        (
            "one control point",
            "2",
            style("linear"),
            curve("Red", &points("0 1")),
        ),
        (
            "no control point",
            "2",
            style("linear"),
            curve("Red", "            <ControlPoints/>\n"),
        ),
        (
            "decreasing x",
            "2",
            style("linear"),
            curve("Red", &points("-7 -6 0 0 -1 7")),
        ),
        (
            "decreasing y",
            "2",
            style("video"),
            curve("Blue", &points("0 0.5 0.5 0.2 1 1")),
        ),
        // A NaN x coordinate is refused where upstream would read past the control points
        // (`docs/improvements.md` U-35), so the files' tests leave it out.
        (
            "a NaN y coordinate",
            "2",
            style("video"),
            curve("Blue", &points("0 0 0.5 nan 1 1")),
        ),
        (
            "too many slopes",
            "2",
            style("linear"),
            curve(
                "Red",
                &format!("{}{}", points("-7 -6 0 0 1 7"), slopes("1 1 1 1")),
            ),
        ),
        (
            "slopes before the control points",
            "2",
            style("log"),
            curve(
                "Red",
                &format!("{}{}", slopes("1 1.5 2"), points("0 0 0.5 0.5 1 1 2 2")),
            ),
        ),
        (
            "slopes of the default curve",
            "2",
            style("video"),
            curve("Red", &slopes("1 1")),
        ),
        (
            "control points twice",
            "2",
            style("video"),
            curve(
                "Red",
                &format!("{}{}", points("0 0 1 1 2 3"), points("0 0 2 1")),
            ),
        ),
        (
            "control points in two parts",
            "2",
            style("video"),
            curve(
                "Red",
                "            <ControlPoints>0 0 1 1<!-- c -->1.5 1.6</ControlPoints>\n",
            ),
        ),
        (
            "a curve twice",
            "2",
            style("video"),
            format!(
                "{}{}",
                curve("Red", &points("0 0 1 1 2 3")),
                curve("Red", &slopes("1 1 1"))
            ),
        ),
        (
            "illegal values",
            "2",
            style("video"),
            curve("Red", &points("0 0 1 x")),
        ),
        (
            "long illegal values",
            "2",
            style("video"),
            curve("Red", &points("0 0 0.25 0.25 0.5 0.5 1 1 x")),
        ),
        (
            "illegal slopes",
            "2",
            style("video"),
            curve("Red", &format!("{}{}", points("0 0 1 1"), slopes("1 1e"))),
        ),
        (
            "an unknown curve",
            "2",
            style("linear"),
            curve("Grn", &points("0 1 1 2 2 3")),
        ),
        (
            "a hue curve",
            "2",
            style("linear"),
            curve("HueHue", &points("0 1 1 2 2 3")),
        ),
        (
            "control points in the operator",
            "2",
            style("linear"),
            points("0 1 1 2 2 3"),
        ),
        (
            "slopes in the operator",
            "2",
            style("linear"),
            slopes("1 1"),
        ),
        (
            "attributes and descriptions",
            "2",
            style("video"),
            format!(
                "        <Description>curves</Description>\n{}",
                curve(
                    "Red",
                    "            <Description>red</Description>\n\
                     \x20           <ControlPoints a=\"1\">0 0 1 1</ControlPoints>\n\
                     \x20           <Slopes b=\"2\">1 1</Slopes>\n"
                )
            ),
        ),
        (
            "text in a curve",
            "2",
            style("video"),
            curve("Red", "            text\n"),
        ),
        (
            "an element in the control points",
            "2",
            style("video"),
            curve(
                "Red",
                "            <ControlPoints>0 0 1 1<Slopes>1 1</Slopes></ControlPoints>\n",
            ),
        ),
        (
            "a GradingRGBCurve in CTF 1.8",
            "1.8",
            style("video"),
            String::new(),
        ),
        // Dynamic curves: their cache IDs leave the curves out, so the identity's comes first,
        // and the next ones share it.
        (
            "dynamic identity curves",
            "2",
            style("log"),
            "        <DynamicParameter param=\"RGB_CURVE\"/>\n".to_owned(),
        ),
        (
            "dyn after curves",
            "2",
            style("log"),
            format!(
                "{}        <DynamicParameter param=\"RGB_CURVE\"/>\n",
                curve("Red", &points("0 0 0.5 0.7 1 1"))
            ),
        ),
        (
            "dyn before curves",
            "2",
            style("linearRev"),
            format!(
                "        <DynamicParameter param=\"rgb_curve\"/>\n{}",
                curve("Master", &points("-1 -1 0 0.3 1 1.5"))
            ),
        ),
        (
            "dynamic twice, bypassLinToLog",
            "2.5",
            format!("{} bypassLinToLog=\"true\"", style("linear")),
            format!(
                "        <DynamicParameter param=\"RGB_CURVE\"/>\n{}\
                 \x20       <DynamicParameter PARAM=\"RGB_CURVE\"/>\n",
                curve("Green", &points("-1 -1 0 0.3 1 1.5"))
            ),
        ),
        (
            "another dynamic parameter",
            "2",
            style("video"),
            "        <DynamicParameter param=\"PRIMARY\"/>\n".to_owned(),
        ),
    ] {
        cases.push(ctf(
            &format!("a GradingRGBCurve: {label}"),
            &format!("id=\"a\" version=\"{version}\""),
            &op("GradingRGBCurve", &attrs, &body),
        ));
    }

    cases.push(clf(
        "a CLF GradingRGBCurve",
        "id=\"a\" compCLFversion=\"3\"",
        &op("GradingRGBCurve", &style("video"), ""),
    ));
    cases
}

/// The processors of the GradingRGBCurve reader's files, in both directions: their cache IDs
/// and pixels (its transform, which `check_processors` compares, comes with Phase 3).
#[test]
fn grading_rgb_curve_files_read_as_in_the_wheel() {
    check_applied(&op_cases_g2());
}

/// The warnings of reading the GradingRGBCurve reader's files, message by message.
#[test]
fn grading_rgb_curve_files_log_as_in_the_wheel() {
    cdl_common::check_logs(&op_cases_g2());
}

/// RGBA probes for the processors' pixels: values across the log, linear and video ranges,
/// distinct per channel, with two alphas.
fn rgba_probes() -> Vec<f32> {
    let values: [f32; 12] = [
        -0.5, 0.0, 0.01, 0.1, 0.18, 0.5, 0.9, 1.0, 1.5, 4.0, 16.0, 100.0,
    ];
    let mut probes = Vec::new();
    for (i, &v) in values.iter().enumerate() {
        probes.extend_from_slice(&[
            v,
            values[(i + 4) % values.len()],
            values[(i + 8) % values.len()],
            if i % 2 == 0 { 1.0 } else { 0.5 },
        ]);
    }
    probes
}

/// The processors of `cases` in both directions, the wheel's (`cpu_apply`, live) against the
/// port's: the error and its stage, or the processor's and the CPU processor's cache IDs and
/// the probes' pixels, bit for bit. For operators whose transform isn't ported yet
/// (`createGroupTransform`), such as GradingRGBCurve's (Phase 3, WP 3.1), which
/// `check_processors` compares.
fn check_applied(cases: &[Case]) {
    use ocio::Config;
    use ocio_ops::image_desc::PackedImageDesc;
    use ocio_testkit::Oracle;
    use ocio_testkit::oracle::BatchCall;

    let probes = rgba_probes();
    let bytes: Vec<u8> = probes.iter().flat_map(|v| v.to_le_bytes()).collect();
    let dirs = [TransformDirection::Forward, TransformDirection::Inverse];
    let requests: Vec<(usize, TransformDirection)> = (0..cases.len())
        .flat_map(|k| dirs.iter().map(move |&dir| (k, dir)))
        .collect();
    let calls: Vec<BatchCall<'_>> = requests
        .iter()
        .map(|&(k, dir)| BatchCall {
            cmd: "cpu_apply",
            args: serde_json::json!({
                "transform": cases[k].spec,
                "direction": direction_name(dir),
            }),
            blobs: vec![&bytes],
        })
        .collect();
    // Pixels always run live: the kernels and the math library belong to the machine.
    let responses = Oracle::get().batch(&calls, false);

    let mut failures = Vec::new();
    for (&(k, dir), response) in requests.iter().zip(responses) {
        let case = &cases[k];
        // A config per case, as `cpu_apply` makes one per call: one config would hand a case
        // the processor of an earlier one with the same cache ID (`docs/improvements.md`
        // I-188).
        let config = Config::create_raw().unwrap();
        let response = response.unwrap_or_else(|e| panic!("{e}"));
        let wheel = match response.result.get("exception") {
            Some(exception) => Err((
                response.result["stage"].as_str().unwrap().to_owned(),
                exception["message"].as_str().unwrap().to_owned(),
            )),
            None => Ok((
                response.result["processor_cache_id"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
                response.result["cpu_cache_id"].as_str().unwrap().to_owned(),
                response.blob_f32(0),
            )),
        };
        let port = (|| {
            let processor = config
                .processor_in_direction(&case.port, dir)
                .map_err(|e| ("processor".to_owned(), e.message().to_owned()))?;
            let processor_cache_id = processor
                .cache_id()
                .map_err(|e| ("processor".to_owned(), e.message().to_owned()))?;
            let cpu = processor
                .default_cpu_processor()
                .map_err(|e| ("cpu_processor".to_owned(), e.message().to_owned()))?;
            // From one buffer to another, as `cpu_apply` does: in place, the linear style
            // converts alpha (`docs/improvements.md` I-90).
            let mut pixels = vec![0.0f32; probes.len()];
            {
                let src = PackedImageDesc::new(&probes[..], probes.len() / 4, 1, 4)
                    .map_err(|e| ("apply".to_owned(), e.message().to_owned()))?;
                let mut dst = PackedImageDesc::new(&mut pixels[..], probes.len() / 4, 1, 4)
                    .map_err(|e| ("apply".to_owned(), e.message().to_owned()))?;
                cpu.apply_src_dst(&src, &mut dst)
                    .map_err(|e| ("apply".to_owned(), e.message().to_owned()))?;
            }
            let cpu_cache_id = String::from_utf8(cpu.get_cache_id().to_vec()).unwrap();
            Ok((processor_cache_id, cpu_cache_id, pixels))
        })();
        let same = match (&wheel, &port) {
            (Err(w), Err(p)) => w == p,
            (Ok((wp, wc, wpix)), Ok((pp, pc, ppix))) => {
                wp == pp
                    && wc == pc
                    && wpix.len() == ppix.len()
                    && wpix
                        .iter()
                        .zip(ppix)
                        .all(|(a, b)| a.to_bits() == b.to_bits())
            }
            _ => false,
        };
        if !same {
            failures.push(format!(
                "{} ({dir:?}):\n  wheel {wheel:?}\n  port  {port:?}",
                case.label
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The Reference reader's files. Every path is absolute: a relative one resolves against the
/// process's working directory with the raw config, which the oracle's and the port's don't
/// share (relative references and their resolution are R2's, with a config's search path).
fn op_cases_h() -> Vec<Case> {
    let f32s = "inBitDepth=\"32f\" outBitDepth=\"32f\"";
    // The referenced files.
    let dir = write_files(&[
        file(
            "matrix.ctf",
            pl(
                "id=\"m\" version=\"1.7\"",
                &op(
                    "Matrix",
                    f32s,
                    "        <Array dim=\"3 3 3\">2 0 0 0 3 0 0 0 4</Array>\n",
                ),
            )
            .as_bytes(),
        ),
        file(
            "range.clf",
            pl(
                "id=\"r\" compCLFversion=\"3\"",
                &op(
                    "Range",
                    f32s,
                    "        <minInValue>0.1</minInValue>\n        <maxInValue>0.9</maxInValue>\n\
                     \x20       <minOutValue>0</minOutValue>\n        <maxOutValue>1</maxOutValue>\n",
                ),
            )
            .as_bytes(),
        ),
    ]);
    let matrix = format!("{dir}/matrix.ctf");
    let range = format!("{dir}/range.clf");
    let reference = |attrs: &str| format!("    <Reference {f32s} {attrs}/>\n");
    let mut cases = Vec::new();

    // CTF files: (label, version, body).
    for (label, version, body) in [
        ("a path", "1.7", reference(&format!("path=\"{matrix}\""))),
        (
            "a path to a CLF, inverted",
            "1.7",
            reference(&format!("path=\"{range}\" inverted=\"true\"")),
        ),
        (
            "inverted of another case",
            "2",
            reference(&format!("PATH=\"{matrix}\" Inverted=\"TRUE\"")),
        ),
        (
            "inverted other than true",
            "2",
            reference(&format!("path=\"{matrix}\" inverted=\"yes\"")),
        ),
        (
            "inverted false",
            "2",
            reference(&format!("path=\"{matrix}\" inverted=\"false\"")),
        ),
        // Only "true" sets the direction: a later "false" leaves it inverse (I-202).
        (
            "inverted true, then false",
            "2",
            reference(&format!(
                "path=\"{matrix}\" inverted=\"true\" Inverted=\"false\""
            )),
        ),
        (
            "a path and a basePath",
            "1.7",
            reference(&format!("path=\"{matrix}\" basePath=\"Shared\"")),
        ),
        (
            "two paths",
            "1.7",
            reference(&format!("path=\"{matrix}\" Path=\"{range}\"")),
        ),
        (
            "the same file twice, between other operators",
            "1.7",
            format!(
                "{}{}{}",
                reference(&format!("path=\"{matrix}\"")),
                op(
                    "Matrix",
                    f32s,
                    "        <Array dim=\"3 3 3\">1 0 0 0 1 0 0 0 1</Array>\n",
                ),
                reference(&format!("path=\"{matrix}\" inverted=\"true\""))
            ),
        ),
        (
            "an id, a name and descriptions",
            "1.7",
            format!(
                "    <Reference id=\"uuid\" name=\"name\" {f32s} path=\"{matrix}\">\n\
                 \x20       <Description>a reference</Description>\n    </Reference>\n"
            ),
        ),
        ("an alias", "1.7", reference("alias=\"alias\"")),
        (
            "an alias, inverted",
            "1.7",
            reference("alias=\"alias\" inverted=\"true\""),
        ),
        (
            "an alias and a path",
            "1.7",
            reference(&format!("alias=\"alias\" path=\"{matrix}\"")),
        ),
        (
            "a path and an alias",
            "1.7",
            reference(&format!("path=\"{matrix}\" alias=\"alias\"")),
        ),
        (
            "an alias and a basePath",
            "1.7",
            reference("alias=\"alias\" basePath=\"Shared\""),
        ),
        (
            "an empty alias and a path",
            "1.7",
            reference(&format!("alias=\"\" path=\"{matrix}\"")),
        ),
        (
            "the currentMonitor alias",
            "1.7",
            reference("alias=\"currentMonitor\""),
        ),
        (
            "the currentMonitor alias of another case",
            "1.7",
            reference(&format!("path=\"{matrix}\" ALIAS=\"CURRENTMONITOR\"")),
        ),
        ("no path", "1.7", reference("")),
        ("an empty path", "1.7", reference("path=\"\"")),
        ("only a basePath", "1.7", reference("basePath=\"Shared\"")),
        (
            "a missing file",
            "1.7",
            reference(&format!("path=\"{dir}/missing.ctf\"")),
        ),
        (
            "an unknown attribute",
            "1.7",
            reference(&format!("path=\"{matrix}\" style=\"x\"")),
        ),
        (
            "a Reference in CTF 1.2",
            "1.2",
            reference(&format!("path=\"{matrix}\"")),
        ),
    ] {
        cases.push(ctf(
            &format!("a Reference: {label}"),
            &format!("id=\"a\" version=\"{version}\""),
            &body,
        ));
    }
    cases.push(clf(
        "a CLF Reference",
        "id=\"a\" compCLFversion=\"3\"",
        &reference(&format!("path=\"{matrix}\"")),
    ));

    // Upstream's files that don't resolve a relative path.
    for name in ["reference_alias.ctf", "reference_alias_path.ctf"] {
        cases.push(file_case(
            name,
            &upstream_file(name),
            Interpolation::Default,
            TransformDirection::Forward,
        ));
    }
    cases
}

/// The processors of the Reference reader's files, in both directions.
#[test]
fn reference_files_read_as_in_the_wheel() {
    check_processors(&op_cases_h());
}

/// The warnings of reading the Reference reader's files, message by message.
#[test]
fn reference_files_log_as_in_the_wheel() {
    cdl_common::check_logs(&op_cases_h());
}

/// One config's processors of two CTF files whose dynamic GradingRGBCurves differ, and of a
/// static one, the wheel's (`processor_cache`) against the port's: which are the same object,
/// and their cache IDs. A dynamic op's cache ID leaves its curves out, so the second file gets
/// the first's processor (`docs/improvements.md` I-188).
#[test]
fn dynamic_curves_share_a_processor_as_in_the_wheel() {
    use std::sync::Arc;

    use ocio::Config;
    use ocio_testkit::Oracle;

    let body = |extra: &str| {
        op(
            "GradingRGBCurve",
            "inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"log\"",
            extra,
        )
    };
    let dynamic = "        <DynamicParameter param=\"RGB_CURVE\"/>\n";
    let red = curve("Red", &points("0 0 0.5 0.7 1 1"));
    let files = [
        ("identity", body(dynamic)),
        ("bent", body(&format!("{red}{dynamic}"))),
        ("static", body(&red)),
    ];
    let cases: Vec<(&str, Case)> = files
        .iter()
        .map(|(name, body)| (*name, ctf(name, "id=\"a\" version=\"2\"", body)))
        .collect();
    let mut failures = Vec::new();
    for order in [[0, 1, 2], [1, 0, 2], [2, 1, 0]] {
        let mut steps = vec![serde_json::json!(["config"])];
        for &i in &order {
            let (name, case) = &cases[i];
            steps.push(serde_json::json!([
                "processor",
                name,
                case.spec,
                "TRANSFORM_DIR_FORWARD"
            ]));
        }
        let response = Oracle::get().call(
            "processor_cache",
            serde_json::json!({"cases": [{"env": {}, "steps": steps}]}),
            &[],
        );
        let result = &response.result[0];
        assert_eq!(result["returncode"], serde_json::json!(0), "{result}");
        let wheel = &result["report"];

        // The port, in the same order, on a raw config.
        let config = Config::create_raw().unwrap();
        let mut made: Vec<(&str, Arc<ocio::Processor>)> = Vec::new();
        let mut port_steps = vec![serde_json::Value::Null];
        let mut same = serde_json::Map::new();
        let mut ids = serde_json::Map::new();
        for &i in &order {
            let (name, case) = &cases[i];
            let processor = config
                .processor_in_direction(&case.port, TransformDirection::Forward)
                .unwrap();
            port_steps.push(serde_json::Value::Null);
            let first = made
                .iter()
                .find(|(_, p)| Arc::ptr_eq(p, &processor))
                .map_or(*name, |(n, _)| *n);
            same.insert((*name).to_owned(), serde_json::json!(first));
            ids.insert(
                (*name).to_owned(),
                serde_json::json!(processor.cache_id().unwrap()),
            );
            made.push((name, processor));
        }
        let port = serde_json::json!({"steps": port_steps, "same": same, "cache_ids": ids});
        if *wheel != port {
            failures.push(format!("{order:?}\n  wheel {wheel}\n  port  {port}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A config of the search paths `paths` and the environment `env`, as the oracle's YAML and
/// as the port's raw config with the same search paths and variables.
fn search_path_config(
    paths: &[&str],
    env: &[(&str, &str)],
) -> (serde_json::Value, std::sync::Arc<Config>) {
    let mut yaml = String::from("ocio_profile_version: 2\n");
    if !env.is_empty() {
        let vars: Vec<String> = env.iter().map(|(k, v)| format!("{k}: \"{v}\"")).collect();
        yaml.push_str(&format!("environment: {{{}}}\n", vars.join(", ")));
    }
    let quoted: Vec<String> = paths.iter().map(|p| format!("\"{p}\"")).collect();
    yaml.push_str(&format!("search_path: [{}]\n", quoted.join(", ")));
    yaml.push_str("roles: {default: raw}\ncolorspaces:\n  - !<ColorSpace> {name: raw}\n");
    let mut config = Config::create_raw().unwrap();
    let config_mut = std::sync::Arc::get_mut(&mut config).unwrap();
    for (k, v) in env {
        config_mut.add_environment_var(k, Some(v.as_bytes()));
    }
    for p in paths {
        config_mut.add_search_path(p);
    }
    (serde_json::json!({ "yaml": yaml }), config)
}

/// The CTF files that reference others, through a config's search path: upstream's, and
/// written ones (subdirectories, two search paths, context variables, nested inversions,
/// other formats, cycles).
fn resolution_cases() -> Vec<(
    &'static str,
    Vec<Case>,
    serde_json::Value,
    std::sync::Arc<Config>,
)> {
    let f32s = "inBitDepth=\"32f\" outBitDepth=\"32f\"";
    let reference = |attrs: &str| format!("    <Reference {f32s} {attrs}/>\n");
    let ctf_file = |body: &str| pl("id=\"a\" version=\"1.7\"", body).into_bytes();
    let matrix = op(
        "Matrix",
        f32s,
        "        <Array dim=\"3 3 3\">2 0 0 0 3 0 0 0 4</Array>\n",
    );
    let range = op(
        "Range",
        f32s,
        "        <minInValue>0.1</minInValue>\n        <maxInValue>0.9</maxInValue>\n\
         \x20       <minOutValue>0</minOutValue>\n        <maxOutValue>1</maxOutValue>\n",
    );
    let dir = write_files(&[
        file(
            "c.clf",
            pl("id=\"c\" compCLFversion=\"3\"", &range).into_bytes(),
        ),
        file(
            "a.ctf",
            ctf_file(&format!(
                "{}{matrix}",
                reference("path=\"sub/b.ctf\" inverted=\"true\"")
            )),
        ),
        file(
            "sub/b.ctf",
            ctf_file(&format!(
                "{}{}",
                reference("path=\"c.clf\" inverted=\"true\""),
                reference("path=\"d.ctf\"")
            )),
        ),
        file("sub/d.ctf", ctf_file(&matrix)),
        file("env.ctf", ctf_file(&reference("path=\"$SUBDIR/d.ctf\""))),
        file(
            "spi.ctf",
            ctf_file(&format!(
                "{}{}",
                reference("path=\"lut.spi1d\""),
                reference("path=\"alias.ctf\"")
            )),
        ),
        file(
            "lut.spi1d",
            b"Version 1\nFrom 0.0 1.0\nLength 3\nComponents 1\n{\n0.0\n0.25\n1.0\n}\n".to_vec(),
        ),
        file("alias.ctf", ctf_file(&reference("alias=\"a\""))),
        file("self.ctf", ctf_file(&reference("path=\"./self.ctf\""))),
        file("x.ctf", ctf_file(&reference("path=\"sub/y.ctf\""))),
        file("sub/y.ctf", ctf_file(&reference("path=\"x.ctf\""))),
        file(
            "twice.ctf",
            ctf_file(&format!(
                "{}{}",
                reference("path=\"c.clf\""),
                reference("path=\"c.clf\" inverted=\"true\"")
            )),
        ),
    ]);
    let written = |name: &str| {
        file_case(
            name,
            &format!("{dir}/{name}"),
            Interpolation::Default,
            TransformDirection::Forward,
        )
    };
    let upstream = |name: &str| {
        file_case(
            name,
            &upstream_file(name),
            Interpolation::Default,
            TransformDirection::Forward,
        )
    };
    let upstream_dir = upstream_file("");
    let upstream_dir = upstream_dir.trim_end_matches('/');
    let sub = format!("{dir}/sub");

    let mut groups = Vec::new();
    // Upstream's files, with their directory as the search path (ReferenceOpData_tests.cpp).
    let (spec, config) = search_path_config(&[upstream_dir], &[]);
    groups.push((
        "upstream's files",
        [
            "reference_path_missing_file.ctf",
            "references_some_inverted.ctf",
            "references_same_twice.ctf",
            "reference_nested.ctf",
            "reference_one_matrix.ctf",
            "reference_cycle_itself.ctf",
            "reference_cycle_2levels.ctf",
            "reference_cycle_3levels.ctf",
            "reference_cycle_path_not_equal.ctf",
            "reference_utf8.ctf",
            "reference_alias.ctf",
        ]
        .into_iter()
        .map(upstream)
        .collect(),
        spec,
        config,
    ));
    // The written files, with their directory as the search path.
    let written_files = [
        "a.ctf",
        "sub/b.ctf",
        "env.ctf",
        "spi.ctf",
        "self.ctf",
        "x.ctf",
        "twice.ctf",
    ];
    let (spec, config) = search_path_config(&[&dir], &[("SUBDIR", "sub")]);
    groups.push((
        "one search path",
        written_files.into_iter().map(written).collect(),
        spec,
        config,
    ));
    // Its subdirectory too: d.ctf resolves from both.
    let (spec, config) = search_path_config(&[&dir, &sub], &[("SUBDIR", "sub")]);
    groups.push((
        "two search paths",
        written_files.into_iter().map(written).collect(),
        spec,
        config,
    ));
    // No variable: $SUBDIR stays unresolved.
    let (spec, config) = search_path_config(&[&dir], &[]);
    groups.push(("no variable", vec![written("env.ctf")], spec, config));

    // References to 3D LUTs without an interpolation: the reference's file transform asks for
    // the default one (trilinear), which a CLF LUT without its own takes, and the other
    // formats' readers too (`FileFormatCTF.cpp:1233`).
    let lut3d = op(
        "LUT3D",
        f32s,
        "        <Array dim=\"2 2 2 3\">\n0 0 0\n0 0 0.9\n0 0.7 0.1\n0.1 0.6 0.8\n\
         0.8 0.1 0\n0.6 0.2 1\n0.9 0.9 0.2\n1 1 1\n</Array>\n",
    );
    let luts = write_files(&[
        file(
            "lut3d.clf",
            pl("id=\"l\" compCLFversion=\"3\"", &lut3d).into_bytes(),
        ),
        file("clf3d.ctf", ctf_file(&reference("path=\"lut3d.clf\""))),
        file(
            "spi3d.ctf",
            ctf_file(&reference("path=\"lut3d_bizarre.spi3d\"")),
        ),
        file("3dl.ctf", ctf_file(&reference("path=\"crosstalk.3dl\""))),
    ]);
    let (spec, config) = search_path_config(&[&luts, upstream_dir], &[]);
    groups.push((
        "references to 3D LUTs",
        ["clf3d.ctf", "spi3d.ctf", "3dl.ctf"]
            .into_iter()
            .map(|name| {
                file_case(
                    name,
                    &format!("{luts}/{name}"),
                    Interpolation::Default,
                    TransformDirection::Forward,
                )
            })
            .collect(),
        spec,
        config,
    ));

    // The recursion check compares the resolved paths ignoring case, on every platform
    // (I-192): a reference to the file itself in another case is a recursion on Windows and a
    // missing file on Linux, and on Linux, where `Pair.ctf` and `pair.ctf` are two files, a
    // reference from one to the other is refused as a recursion.
    let mut entries = vec![file("Self.ctf", ctf_file(&reference("path=\"SELF.ctf\"")))];
    let mut names = vec!["Self.ctf"];
    if !cfg!(windows) {
        entries.push(file("Pair.ctf", ctf_file(&reference("path=\"pair.ctf\""))));
        entries.push(file("pair.ctf", ctf_file(&matrix)));
        names.push("Pair.ctf");
    }
    let cases_dir = write_files(&entries);
    let (spec, config) = search_path_config(&[&cases_dir], &[]);
    groups.push((
        "names that differ in case",
        names
            .into_iter()
            .map(|name| {
                file_case(
                    name,
                    &format!("{cases_dir}/{name}"),
                    Interpolation::Default,
                    TransformDirection::Forward,
                )
            })
            .collect(),
        spec,
        config,
    ));

    // The longest chain of references the port follows (U-76): `c0.ctf` references `c1.ctf`,
    // and so on to a matrix in the last file.
    let mut chain = Vec::new();
    for i in 0..CHAIN_LIMIT {
        let body = if i == CHAIN_LIMIT - 1 {
            matrix.clone()
        } else {
            reference(&format!("path=\"c{}.ctf\"", i + 1))
        };
        chain.push(file(&format!("c{i}.ctf"), ctf_file(&body)));
    }
    let chain_dir = write_files(&chain);
    let (spec, config) = search_path_config(&[&chain_dir], &[]);
    groups.push((
        "the longest chain",
        vec![file_case(
            "c0.ctf",
            &format!("{chain_dir}/c0.ctf"),
            Interpolation::Default,
            TransformDirection::Forward,
        )],
        spec,
        config,
    ));
    groups
}

/// The number of files in the longest chain of CTF references the port follows
/// (`MAX_FILE_NESTING`, docs/improvements.md U-76).
const CHAIN_LIMIT: usize = 1000;

/// The processors of CTF files that reference others, through a config's search path, in
/// both directions.
#[test]
fn references_resolve_as_in_the_wheel() {
    for (label, cases, spec, config) in resolution_cases() {
        println!("{label}");
        common::transforms::check_processors_in(&cases, Some(&spec), &config);
    }
}

/// One config's processors of a CTF file whose reference is a context variable, `$FILEVAR`,
/// with two contexts that set it to two files, in both orders: the wheel's (`config_calls`)
/// against the port's cache IDs and files. The processor cache doesn't key on the variables
/// of a file's references, so the second context gets the first one's processor
/// (`docs/improvements.md` I-193).
#[test]
fn context_variables_in_references_share_a_processor_as_in_the_wheel() {
    use ocio::{Context, FileTransform, Transform};
    use ocio_testkit::Oracle;
    use serde_json::json;

    let f32s = "inBitDepth=\"32f\" outBitDepth=\"32f\"";
    let dir = write_files(&[
        file(
            "r_var.ctf",
            pl(
                "id=\"a\" version=\"1.7\"",
                &format!("    <Reference {f32s} path=\"$FILEVAR\"/>\n"),
            )
            .into_bytes(),
        ),
        file(
            "m.ctf",
            pl(
                "id=\"m\" version=\"1.7\"",
                &op(
                    "Matrix",
                    f32s,
                    "        <Array dim=\"3 3 3\">2 0 0 0 3 0 0 0 4</Array>\n",
                ),
            )
            .into_bytes(),
        ),
        file(
            "lut.spi1d",
            b"Version 1\nFrom 0.0 1.0\nLength 3\nComponents 1\n{\n0.0\n0.25\n1.0\n}\n".to_vec(),
        ),
    ]);
    let src = format!("{dir}/r_var.ctf");
    let case = file_case(
        "r_var.ctf",
        &src,
        Interpolation::Default,
        TransformDirection::Forward,
    );
    let hex = |bytes: &[u8]| json!({"bytes": bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()});
    let mut failures = Vec::new();
    for order in [["m.ctf", "lut.spi1d"], ["lut.spi1d", "m.ctf"]] {
        let (spec, config) = search_path_config(&[&dir], &[("FILEVAR", "m.ctf")]);
        let mut calls = vec![json!({"call": "ClearAllCaches", "on": "OCIO"})];
        for (k, value) in order.iter().enumerate() {
            let (ctx, p, m) = (format!("ctx{k}"), format!("p{k}"), format!("m{k}"));
            calls.extend([
                json!({"new": "Context", "as": ctx}),
                json!({"call": "addSearchPath", "on": ctx, "args": [dir]}),
                json!({"call": "__setitem__", "on": ctx, "args": ["FILEVAR", value]}),
                json!({"call": "getProcessor",
                       "args": [{"ref": ctx}, {"transform": case.spec},
                                {"enum": "TRANSFORM_DIR_FORWARD"}],
                       "as": p}),
                json!({"call": "getCacheID", "on": p}),
                json!({"call": "getProcessorMetadata", "on": p, "as": m}),
                json!({"call": "getFiles", "on": m}),
            ]);
        }
        let reply = Oracle::get()
            .call("config_calls", json!({"config": spec, "calls": calls}), &[])
            .result;
        let results = reply["calls"].as_array().expect("calls");
        let mut wheel = Vec::new();
        for (k, value) in order.iter().enumerate() {
            let at = |i: usize| {
                let r = &results[1 + 7 * k + i];
                assert!(r.get("result").is_some(), "{value}: {r}");
                r["result"].clone()
            };
            wheel.push(json!([value, at(4), at(6)]));
        }

        // The port, on one config, with the same contexts.
        ocio::clear_all_caches();
        let mut file_transform = FileTransform::new();
        file_transform.set_src(&src);
        let transform = Transform::File(file_transform);
        let mut port = Vec::new();
        for value in order {
            let mut ctx = Context::new();
            ctx.add_search_path(dir.as_bytes());
            ctx.set_string_var("FILEVAR", Some(value.as_bytes()));
            let processor = config
                .processor_with_context(&ctx, &transform, TransformDirection::Forward)
                .unwrap();
            let metadata = processor.processor_metadata();
            let files: Vec<serde_json::Value> = (0..metadata.num_files())
                .map(|i| hex(metadata.file(i)))
                .collect();
            port.push(json!([
                value,
                hex(processor.cache_id().unwrap().as_bytes()),
                files
            ]));
        }
        let (wheel, port) = (json!(wheel), json!(port));
        if wheel != port {
            failures.push(format!("{order:?}\n  wheel {wheel}\n  port  {port}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
