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
