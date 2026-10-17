// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingRGBCurve writer against the wheel (`write_transform`), byte for byte.
//!
//! The wheel writes the group of the processor of a CTF file holding GradingRGBCurves
//! (`Processor::createGroupTransform`, then `GroupTransform::write`). The port can't build that
//! group yet: `GradingRGBCurveTransform` comes with Phase 3 (WP 3.1). So it writes the
//! processor's ops directly: the file transform's ops, finalized as `Processor::setTransform`
//! leaves them (`Processor.cpp:623-641` @ v2.5.2), through [`write_ops`] with the ops' format
//! metadata, which is what the processor's group carries. Upstream's round trip from these ops
//! to transforms and back to ops copies each op's data (`CreateGradingRGBCurveTransform`,
//! `BuildGradingRGBCurveOp`, `CreateMatrixTransform`, `BuildMatrixOp`), so both write the same
//! ops. When the transform is ported, `tests/file_format_ctf_write_oracle.rs` takes these
//! cases through `GroupTransform::write` too.

use std::path::PathBuf;

use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values;
use serde_json::json;

use super::*;
use crate::transform::{Transform, build_ops};
use crate::transforms::file_transform::FileTransform;

/// The CTF format's name, and the CLF's.
const FORMATS: [&str; 2] = [FILEFORMAT_CTF, FILEFORMAT_CLF];

/// The path of a CTF file of `body`, written once under `<target>/ctf-write-files/`, named
/// after a hash of its bytes, with `/` separators.
fn ctf_file(body: &str) -> String {
    let text = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <ProcessList id=\"pl\" name=\"curves\" version=\"2\">\n\
         \x20   <Description>a ProcessList</Description>\n{body}</ProcessList>\n"
    );
    let hash = xxhash_rust::xxh3::xxh3_64(text.as_bytes());
    let dir: PathBuf = ocio_testkit::paths::target_dir().join("ctf-write-files");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{hash:016x}.ctf"));
    if std::fs::read(&path).ok().as_deref() != Some(text.as_bytes()) {
        let temp = dir.join(format!(
            "{hash:016x}.{}.{:?}.tmp",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&temp, &text).unwrap();
        std::fs::rename(&temp, &path).unwrap();
    }
    let path = path.to_str().unwrap().replace('\\', "/");
    path.trim_start_matches("//?/").to_owned()
}

/// A GradingRGBCurve element of `attrs` holding `body`.
fn curves(attrs: &str, body: &str) -> String {
    format!(
        "    <GradingRGBCurve inBitDepth=\"32f\" outBitDepth=\"32f\" {attrs}>\n{body}    \
         </GradingRGBCurve>\n"
    )
}

/// The curve element `tag` of the control points `points` and, when given, the slopes
/// `slopes`.
fn curve(tag: &str, points: &str, slopes: Option<&str>) -> String {
    let slopes = slopes
        .map(|s| format!("            <Slopes>{s}</Slopes>\n"))
        .unwrap_or_default();
    format!(
        "        <{tag}>\n            <ControlPoints>{points}</ControlPoints>\n{slopes}        \
         </{tag}>\n"
    )
}

/// The dynamic parameter element.
const DYNAMIC: &str = "        <DynamicParameter param=\"RGB_CURVE\"/>\n";

/// The port's write of the processor's ops of the CTF file `path` in `dir`, in `format`.
fn port_write(path: &str, dir: TransformDirection, format: &str) -> Result<Vec<u8>> {
    let config = Config::create_raw()?;
    let context = Context::new();
    let mut file_transform = FileTransform::new();
    file_transform.set_src(path);
    let transform = Transform::File(file_transform);

    // The processor's ops (`Processor::Impl::setTransform`).
    transform.validate()?;
    let mut ops = OpVec::new();
    build_ops(&mut ops, &config, &context, &transform, dir)?;
    ops.finalize()?;
    ops.validate_dynamic_properties()?;

    // Its group's metadata, and the group's write (`LocalFileFormat::write`).
    let metadata = ops.get_format_metadata().clone();
    let is_clf = strcasecmp(format.as_bytes(), FILEFORMAT_CLF.as_bytes()).is_eq();
    let mut ostream = OStringStream::new(Crt::NATIVE);
    if let Err(e) = write_ops(&mut ops, &metadata, is_clf, &mut ostream) {
        // `GroupTransform::write`'s message (transforms/GroupTransform.cpp:128-138 @ v2.5.2).
        let mut err = OStringStream::new(Crt::NATIVE);
        err.put_str("Error writing format '");
        err.put_str(format);
        err.put_str("': ");
        err.put_c_str(e.what());
        return Err(Exception::new(err.into_bytes()));
    }
    Ok(ostream.into_bytes())
}

/// The GradingRGBCurve files: each style, bypassLinToLog, curves that differ from the
/// style's default by their points or their slopes, slopes that are the default ones,
/// dynamic curves, values that need many digits, the op's id, name and descriptions, and a
/// matrix before the curves.
fn curve_files() -> Vec<(&'static str, String)> {
    let red = curve("Red", "0 0 0.5 0.7 1 1", None);
    let lin_red = curve("Red", "-7 -6.015625 0 0 7 7", None);
    let master_slopes = curve("Master", "-7 -7 0 0 7 7 16 10", Some("1 0.75 1.1 1"));
    let digits = curve(
        "Green",
        "0 0.333333343 0.123456789 0.987654321 1.5 1.00001252",
        Some("0.333333343 1.00000012 2.5e-07"),
    );
    let default_slopes = curve("Blue", "0 0 0.5 0.5 1 1", Some("0 0 0"));
    let all = format!(
        "{}{}{}{}",
        curve("Red", "0 0.1 0.5 0.6 1 1", None),
        curve("Green", "0 0 0.4 0.5 1 0.9", None),
        curve("Blue", "0 0 0.5 0.5 0.8 0.7 1 1", Some("1 1 0.5 1")),
        curve("Master", "0 0 0.5 0.45 1 1", None),
    );
    let matrix = "    <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\">\n        <Array dim=\"3 3\">\
                  2 0 0 0 3 0 0 0 4</Array>\n    </Matrix>\n";
    vec![
        ("log", curves("style=\"log\"", &red)),
        ("logRev", curves("style=\"logRev\"", &red)),
        ("linear", curves("style=\"linear\"", &lin_red)),
        ("linearRev", curves("style=\"linearRev\"", &lin_red)),
        ("video", curves("style=\"video\"", &red)),
        ("videoRev", curves("style=\"videoRev\"", &red)),
        (
            "bypassLinToLog",
            curves("style=\"linear\" bypassLinToLog=\"true\"", &lin_red),
        ),
        ("master slopes", curves("style=\"linear\"", &master_slopes)),
        ("many digits", curves("style=\"log\"", &digits)),
        ("default slopes", curves("style=\"log\"", &default_slopes)),
        ("all curves", curves("style=\"video\"", &all)),
        ("dynamic default", curves("style=\"log\"", DYNAMIC)),
        (
            "dynamic",
            curves("style=\"linearRev\"", &format!("{lin_red}{DYNAMIC}")),
        ),
        (
            "an id, a name and descriptions",
            curves(
                "id=\"c1\" name=\"the curves\" style=\"log\"",
                &format!(
                    "        <Description>first</Description>\n        \
                     <Description>second</Description>\n{red}"
                ),
            ),
        ),
        (
            "after a matrix",
            format!("{matrix}{}", curves("style=\"video\"", &master_slopes)),
        ),
    ]
}

/// Each GradingRGBCurve file's processor in both directions, written as CTF and CLF by the
/// wheel and the port: the text, or the error.
#[test]
fn grading_rgb_curves_write_as_in_the_wheel() {
    let mut cases = Vec::new();
    for (label, body) in curve_files() {
        let path = ctf_file(&body);
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            for format in FORMATS {
                cases.push((label, path.clone(), dir, format));
            }
        }
    }
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(_, path, dir, format)| BatchCall {
            cmd: "write_transform",
            args: json!({
                "processor": {
                    "transform": {"class": "FileTransform", "calls": [["setSrc", path]]},
                    "direction": match dir {
                        TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
                        TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
                    },
                },
                "format": format,
            }),
            blobs: Vec::new(),
        })
        .collect();
    let mut failures = Vec::new();
    let mut texts = 0;
    for ((label, path, dir, format), response) in
        cases.iter().zip(Oracle::get().batch(&calls, true))
    {
        let response = response.unwrap_or_else(|e| panic!("{label}: {e}"));
        let result = &response.result;
        let port = port_write(path, *dir, format);
        let same = match (result.get("exception"), &port) {
            (Some(exception), Err(e)) => {
                result["stage"] == "write"
                    && exception["message"].as_str().unwrap().as_bytes() == e.what()
            }
            (None, Ok(written)) => {
                texts += 1;
                oracle_values::bytes(&result["text"]) == *written
            }
            _ => false,
        };
        if !same {
            failures.push(format!(
                "{label} ({dir:?}, {format}):\n  wheel {result}\n  port  {:?}",
                port.map(|w| String::from_utf8_lossy(&w).into_owned())
                    .map_err(|e| String::from_utf8_lossy(e.what()).into_owned())
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // Every file is written as CTF in both directions.
    assert_eq!(texts, 2 * curve_files().len());
}
