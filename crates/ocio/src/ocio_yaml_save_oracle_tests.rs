// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The transform savers against the wheel, through the oracle's `config_calls`.
//!
//! Each case is the YAML text of a transform, the transform of the only color space of a
//! config of a profile version ([`config_text`]). The wheel loads the config and serializes it
//! (`Config::serialize`); the port loads the same config (`Config::CreateFromStream`) and saves
//! the color space's transform ([`save_transform`]) where the config writer puts it: the
//! value of the color space's `to_scene_reference` (`to_reference` in version 1) key, in the
//! color space's block map, in the `colorspaces` sequence of the document's map, with the
//! writer's precisions. Compared byte for byte: that key's lines in both texts (the key's line
//! and the lines indented under it), and the warnings logged while loading and saving.

use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::{bytes, hex, log};
use serde_json::json;

use super::*;
use crate::test_env::capture_log;

/// The color space key of a transform in a config of the profile version `version`.
fn transform_key(version: &str) -> &'static str {
    if version.starts_with('1') {
        "to_reference"
    } else {
        "to_scene_reference"
    }
}

/// The config of a case: its transform is the only color space's.
fn config_text(version: &str, case: &str) -> String {
    format!(
        "ocio_profile_version: {version}\nroles: {{default: raw}}\ncolorspaces:\n  - \
         !<ColorSpace>\n    name: raw\n    {}: {case}\n",
        transform_key(version)
    )
}

/// The transform's lines of `text`: from the line of its key to the last line indented under
/// it.
fn transform_lines(text: &[u8], version: &str) -> Option<Vec<u8>> {
    let key = format!("    {}: ", transform_key(version));
    let lines: Vec<&[u8]> = text.split(|&c| c == b'\n').collect();
    let start = lines.iter().position(|l| l.starts_with(key.as_bytes()))?;
    let mut out = lines[start].to_vec();
    for l in &lines[start + 1..] {
        if !l.starts_with(b"      ") {
            break;
        }
        out.push(b'\n');
        out.extend_from_slice(l);
    }
    Some(out)
}

/// The port's text of a transform where the config writer puts it.
fn port_save(t: &Transform, major: u32) -> ocio_ops::exception::Result<Vec<u8>> {
    let mut out = Emitter::new();
    out.set_double_precision(15);
    out.set_float_precision(7);
    out.put(BeginMap);
    out.put(Key).put("colorspaces").put(Value).put(BeginSeq);
    out.put(verbatim_tag("ColorSpace")).put(BeginMap);
    out.put(Key).put("name").put(Value).put("raw");
    let key = if major < 2 {
        "to_reference"
    } else {
        "to_scene_reference"
    };
    out.put(Key).put(key).put(Value);
    save_transform(&mut out, t, major)?;
    out.put(EndMap).put(EndSeq).put(EndMap);
    Ok(out.c_str().to_vec())
}

/// The port's lines of a case's transform (or why it has none), and what it logged loading
/// and saving it.
fn port_lines(text: &str, version: &str) -> (std::result::Result<Vec<u8>, String>, Vec<Vec<u8>>) {
    capture_log(|| {
        let config = Config::create_from_stream(text.as_bytes())
            .map_err(|e| format!("the port refuses it: {}", String::from_utf8_lossy(e.what())))?;
        let t = config
            .color_space("raw")
            .and_then(|cs| cs.transform(ColorSpaceDirection::ToReference))
            .ok_or("the port loads no transform")?;
        let saved = port_save(t, config.major_version()).map_err(|e| {
            format!(
                "the port can't save it: {}",
                String::from_utf8_lossy(e.what())
            )
        })?;
        transform_lines(&saved, version).ok_or_else(|| "no transform lines".to_string())
    })
}

fn lossy(log: &[Vec<u8>]) -> Vec<String> {
    log.iter()
        .map(|m| String::from_utf8_lossy(m).into_owned())
        .collect()
}

/// Checks the cases of a profile version against the wheel; panics listing every difference.
/// The wheel loads each config and serializes it (`config_calls`), so its log of loading and
/// its log of serializing come apart.
fn check_save_v(version: &str, cases: &[&str]) {
    let configs: Vec<String> = cases.iter().map(|c| config_text(version, c)).collect();
    let calls: Vec<BatchCall<'_>> = configs
        .iter()
        .map(|text| BatchCall {
            cmd: "config_calls",
            args: json!({"config": {"yaml": {"bytes": hex(text.as_bytes())}},
                         "calls": [{"call": "serialize"}]}),
            blobs: Vec::new(),
        })
        .collect();
    let wheel = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for ((case, w), text) in cases.iter().zip(wheel).zip(&configs) {
        let w = w.expect("config_calls").result;
        if !w["config"].is_null() {
            failures.push(format!("{case}: the wheel refuses it: {}", w["config"]));
            continue;
        }
        let serialize = &w["calls"][0];
        let wheel_lines = transform_lines(&bytes(&serialize["result"]), version);
        let (port, port_log) = port_lines(text, version);
        match port {
            Ok(port) => {
                if wheel_lines.as_ref() != Some(&port) {
                    failures.push(format!(
                        "{case}:\n  wheel {:?}\n  port  {:?}",
                        wheel_lines.map(|l| String::from_utf8_lossy(&l).into_owned()),
                        String::from_utf8_lossy(&port)
                    ));
                }
            }
            Err(e) => failures.push(format!("{case}: {e}")),
        }
        let wheel_log = [log(&w["config_log"]), log(&serialize["log"])].concat();
        if wheel_log != port_log {
            failures.push(format!(
                "{case}: log\n  wheel {:?}\n  port  {:?}",
                lossy(&wheel_log),
                lossy(&port_log)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

/// Each saver's keys, defaults and values, in version 2 configs.
#[test]
fn transforms_save_as_in_the_wheel() {
    check_save_v(
        "2",
        &[
            // AllocationTransform: the variables as floats, at precision 7.
            "!<AllocationTransform> {}",
            "!<AllocationTransform> {allocation: lg2, vars: [-8, 5, 0.003]}",
            "!<AllocationTransform> {allocation: uniform, vars: [0.1, 0.9], direction: inverse}",
            "!<AllocationTransform> {vars: [0.123456789, 1e-10, 3.4e38, -0.30000001]}",
            // BuiltinTransform.
            "!<BuiltinTransform> {style: UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD}",
            "!<BuiltinTransform> {style: ACEScct_to_ACES2065-1, direction: inverse}",
            // CDLTransform: defaults compared in float precision.
            "!<CDLTransform> {}",
            "!<CDLTransform> {name: n, slope: [1, 2, 3], offset: [0.1, 0, 0], power: [1, 1, \
             1.5], sat: 1.5, style: noclamp}",
            "!<CDLTransform> {slope: [1.0000001, 1, 1], offset: [1e-46, 0, 0], power: \
             [0.99999995, 1, 1], sat: 1.0000001}",
            "!<CDLTransform> {slope: [1.000001, 1, 1], offset: [1e-30, 0, 0], sat: 0.999999}",
            "!<CDLTransform> {style: asc, direction: inverse}",
            "!<CDLTransform> {sat: 0.30000000000000004, power: [1e-300, 2, 3]}",
            // ColorSpaceTransform.
            "!<ColorSpaceTransform> {src: a, dst: b}",
            "!<ColorSpaceTransform> {src: a, dst: b, data_bypass: false, direction: inverse}",
            "!<ColorSpaceTransform> {src: \"\", dst: \"x: y\"}",
            // DisplayViewTransform.
            "!<DisplayViewTransform> {src: a, display: d, view: v}",
            "!<DisplayViewTransform> {src: a, display: d, view: v, looks_bypass: true, \
             data_bypass: false, direction: inverse}",
            // ExponentTransform: one value when the RGB values are equal and alpha is 1.
            "!<ExponentTransform> {value: 2.2}",
            "!<ExponentTransform> {value: [1, 2, 3, 4]}",
            "!<ExponentTransform> {value: [2, 2, 2, 1], style: mirror}",
            "!<ExponentTransform> {value: [2, 2, 2, 1.5], style: pass_thru, name: e}",
            "!<ExponentTransform> {value: [.nan, .nan, .nan, 1]}",
            "!<ExponentTransform> {value: [.inf, .inf, .inf, 1], direction: inverse}",
            // ExponentWithLinearTransform.
            "!<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055}",
            "!<ExponentWithLinearTransform> {gamma: [2.4, 2.4, 2.4, 1.1], offset: [0.1, 0.1, \
             0.1, 0.2], style: mirror}",
            "!<ExponentWithLinearTransform> {name: x, gamma: [1, 2, 3, 1], offset: 0.1, \
             direction: inverse}",
            "!<ExponentWithLinearTransform> {gamma: 1.2, offset: [0, 0, 0, 0.5], style: linear}",
            // FileTransform.
            "!<FileTransform> {src: a.lut}",
            "!<FileTransform> {src: a.cc, cccid: x, cdl_style: asc, interpolation: \
             tetrahedral}",
            "!<FileTransform> {src: a.lut, interpolation: default, direction: inverse}",
            "!<FileTransform> {src: a.lut, interpolation: foo}",
            "!<FileTransform> {src: a.cc, cdl_style: noclamp}",
            // GroupTransform: a block map.
            "!<GroupTransform> {children: []}",
            "!<GroupTransform> {name: g, direction: inverse, children: [!<LogTransform> {base: \
             10}, !<GroupTransform> {children: [!<MatrixTransform> {offset: [1, 2, 3, 0]}]}]}",
            "!<GroupTransform> {children: [!<GroupTransform> {children: [!<GroupTransform> \
             {name: deep, children: [!<RangeTransform> {}]}]}, !<CDLTransform> {}]}",
            // LogAffineTransform: a parameter as one number when its three are equal.
            "!<LogAffineTransform> {}",
            "!<LogAffineTransform> {base: 10, log_side_slope: [1, 2, 3], lin_side_offset: 0.5}",
            "!<LogAffineTransform> {log_side_slope: 1, log_side_offset: 0, lin_side_slope: \
             [1, 1, 1], lin_side_offset: [0, 0, 0], name: la}",
            "!<LogAffineTransform> {base: .nan, log_side_slope: .nan, direction: inverse}",
            "!<LogAffineTransform> {base: 2.0000000000000004, lin_side_slope: \
             0.30000000000000004}",
            // LogCameraTransform: its break and linear slope have no default.
            "!<LogCameraTransform> {lin_side_break: 0.1}",
            "!<LogCameraTransform> {lin_side_break: [0.1, 0.2, 0.3], linear_slope: [1, 1, 1], \
             base: 10}",
            "!<LogCameraTransform> {lin_side_break: [0, 0, 0], linear_slope: 1.5, \
             log_side_offset: [1, 1, 2], name: lc, direction: inverse}",
            // LogTransform.
            "!<LogTransform> {}",
            "!<LogTransform> {base: 10, name: l}",
            "!<LogTransform> {base: 2, direction: inverse}",
            // LookTransform.
            "!<LookTransform> {src: a, dst: b, looks: \"+c, -d\"}",
            "!<LookTransform> {src: a, dst: b, direction: inverse}",
            // MatrixTransform: the identity and zeros in float precision.
            "!<MatrixTransform> {}",
            "!<MatrixTransform> {name: m, matrix: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, \
             14, 15, 16], offset: [0.1, 0.2, 0.3, 0.4]}",
            "!<MatrixTransform> {matrix: [1.0000001, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 1e-46, \
             1], offset: [1e-46, 0, 0, 0]}",
            "!<MatrixTransform> {matrix: [1.000001, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, \
             1], direction: inverse}",
            "!<MatrixTransform> {offset: [0, 0, 0, 1e-40]}",
            // RangeTransform.
            "!<RangeTransform> {}",
            "!<RangeTransform> {min_in_value: 0, max_in_value: 1, min_out_value: 0.1, \
             max_out_value: 0.9, style: noClamp}",
            "!<RangeTransform> {min_in_value: 0.1, max_in_value: 1e-300, name: r}",
            "!<RangeTransform> {max_out_value: 123456789.123456789, direction: inverse}",
            "!<RangeTransform> {min_in_value: -0.0, max_in_value: 1e+300, style: clamp}",
        ],
    );
}

/// The fixed functions: their styles, parameters, and the warning saving the experimental
/// ACES 2 styles logs (version 2.5: the config refuses the styles of later versions in older
/// ones).
#[test]
fn fixed_functions_save_as_in_the_wheel() {
    check_save_v(
        "2.5",
        &[
            "!<FixedFunctionTransform> {style: ACES_RedMod03}",
            "!<FixedFunctionTransform> {style: ACES_GamutComp13, params: [1.147, 1.264, 1.312, \
             0.815, 0.803, 0.880, 1.2], name: f}",
            "!<FixedFunctionTransform> {style: REC2100_Surround, params: [0.78], direction: \
             inverse}",
            "!<FixedFunctionTransform> {style: RGB_TO_HSV}",
            "!<FixedFunctionTransform> {style: XYZ_TO_LUV, direction: inverse}",
            "!<FixedFunctionTransform> {style: Lin_TO_PQ}",
            "!<FixedFunctionTransform> {style: RGB_TO_HSY_LOG}",
            "!<FixedFunctionTransform> {style: ACES2_OutputTransform, params: [100, 0.7347, \
             0.2653, 0, 1, 0.0001, -0.077, 0.3127, 0.329]}",
            "!<FixedFunctionTransform> {style: ACES2_RGB_TO_JMh, params: [100, 0.7347, 0.2653, \
             0, 1, 0.0001, -0.077, 0.3127], direction: inverse}",
        ],
    );
}

/// Version 1: no names, the exponent's four values, the log's base always, and a FileTransform's
/// `default` interpolation written `linear`.
#[test]
fn transforms_save_in_version_1_as_in_the_wheel() {
    check_save_v(
        "1",
        &[
            "!<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}",
            "!<ExponentTransform> {value: [1, 2, 3, 1]}",
            "!<LogTransform> {}",
            "!<LogTransform> {base: 10}",
            "!<FileTransform> {src: a.lut}",
            "!<FileTransform> {src: a.lut, interpolation: best}",
            "!<FileTransform> {src: a.lut, interpolation: default}",
            "!<MatrixTransform> {matrix: [2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1]}",
            "!<CDLTransform> {slope: [2, 2, 2]}",
            "!<GroupTransform> {children: [!<LogTransform> {base: 10}, !<CDLTransform> {sat: \
             2}]}",
        ],
    );
}

/// A class without a saver: serializing a config whose color space holds a `Lut1DTransform`
/// fails with the dispatch's message, as `Config::serialize` wraps it, in the wheel too.
#[test]
fn a_config_with_a_lut1d_transform_is_not_serialized() {
    let _env = crate::test_env::EnvGuard::new();
    let mut config = (*Config::create_raw().unwrap()).clone();
    let mut cs = ColorSpace::new();
    cs.set_name("lut");
    let t = Transform::from(crate::transforms::lut1d_transform::Lut1DTransform::new());
    cs.set_transform(Some(&t), ColorSpaceDirection::ToReference)
        .unwrap();
    config.add_color_space(&cs).unwrap();
    let e = config.serialize().unwrap_err();

    let r = Oracle::get()
        .call(
            "config_calls",
            json!({"config": "raw", "calls": [
                {"new": "Lut1DTransform", "as": "t"},
                {"new": "ColorSpace", "as": "cs"},
                {"call": "setName", "on": "cs", "args": ["lut"]},
                {"call": "setTransform", "on": "cs",
                 "args": [{"ref": "t"}, {"enum": "COLORSPACE_DIR_TO_REFERENCE"}]},
                {"call": "addColorSpace", "args": [{"ref": "cs"}]},
                {"call": "serialize"},
            ]}),
            &[],
        )
        .result;
    let wheel = bytes(&r["calls"][5]["exception"]["message"]);
    assert_eq!(
        String::from_utf8_lossy(e.what()),
        String::from_utf8_lossy(&wheel)
    );
}

/// Checks that the wheel and the port refuse each case's config of a profile version, with the
/// same message.
fn check_refused_v(version: &str, cases: &[&str]) {
    let configs: Vec<String> = cases.iter().map(|c| config_text(version, c)).collect();
    let calls: Vec<BatchCall<'_>> = configs
        .iter()
        .map(|text| BatchCall {
            cmd: "config_calls",
            args: json!({"config": {"yaml": {"bytes": hex(text.as_bytes())}}, "calls": []}),
            blobs: Vec::new(),
        })
        .collect();
    let wheel = Oracle::get().batch(&calls, true);
    assert_eq!(wheel.len(), cases.len());
    let mut failures = Vec::new();
    for ((case, w), text) in cases.iter().zip(wheel).zip(&configs) {
        let w = w.expect("config_calls").result;
        let wheel_message = bytes(&w["config"]["exception"]["message"]);
        let port = Config::create_from_stream(text.as_bytes());
        match port {
            Ok(_) => failures.push(format!("{case}: the port loads it, the wheel: {w}")),
            Err(e) => {
                if wheel_message.is_empty() || e.what() != wheel_message.as_slice() {
                    failures.push(format!(
                        "{case}:\n  wheel {}\n  port  {}",
                        String::from_utf8_lossy(&wheel_message),
                        String::from_utf8_lossy(e.what())
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The ExposureContrastTransform's saver: the dynamic values left out, the pivot always, the
/// log values unless they are the defaults, the name, and special values.
#[test]
fn exposure_contrast_transforms_save_as_in_the_wheel() {
    check_save_v(
        "2",
        &[
            "!<ExposureContrastTransform> {}",
            "!<ExposureContrastTransform> {style: video, exposure: 1.5, contrast: 0.5, gamma: \
             1.1, pivot: 0.18}",
            "!<ExposureContrastTransform> {style: LOG, exposure: -1.4, contrast: 0.6, gamma: 1.2, \
             pivot: 0.2, direction: inverse, name: ec}",
            "!<ExposureContrastTransform> {direction: inverse, style: Video, contrast: 2}",
            "!<ExposureContrastTransform> {log_exposure_step: 0.088, log_midway_gray: 0.435}",
            "!<ExposureContrastTransform> {log_exposure_step: 0.0880000001, log_midway_gray: \
             0.43500000000000005}",
            "!<ExposureContrastTransform> {log_exposure_step: 0.1, log_midway_gray: 0.5, \
             exposure: 0}",
            "!<ExposureContrastTransform> {exposure: .nan, contrast: .inf, gamma: -.inf, pivot: \
             -0.0}",
            "!<ExposureContrastTransform> {exposure: 1e-30, contrast: 1e300, gamma: 1e-320, \
             pivot: 4.9e-324}",
            "!<ExposureContrastTransform> {exposure: 0.12345678901234567, contrast: \
             1.0000000000000002, gamma: 123456789012345678, pivot: 0.30000000000000004}",
            "!<ExposureContrastTransform> {log_exposure_step: .nan, log_midway_gray: .nan}",
            "!<ExposureContrastTransform> {exposure: 3.4028234663852886e38, contrast: \
             1.1754943508222875e-38, gamma: 1.7976931348623157e308}",
            "!<GroupTransform> {children: [!<ExposureContrastTransform> {style: linear, \
             exposure: 1}, !<ExposureContrastTransform> {name: x, gamma: 2}]}",
        ],
    );
}

/// Version 1 configs refuse an ExposureContrastTransform (`checkVersionConsistency`), in a
/// group too.
#[test]
fn exposure_contrast_transforms_are_refused_in_version_1() {
    check_refused_v(
        "1",
        &[
            "!<ExposureContrastTransform> {}",
            "!<ExposureContrastTransform> {style: video, exposure: 1}",
            "!<GroupTransform> {children: [!<ExposureContrastTransform> {}]}",
        ],
    );
}

/// The GradingPrimaryTransform's saver: a flow map for default values, a block map otherwise;
/// each style's controls, the contrast pivot forced by a contrast or pivot that isn't the
/// default, the other styles' controls dropped, the clamps; and special values and values
/// with more digits than the writer's precision.
#[test]
fn grading_primary_transforms_save_as_in_the_wheel() {
    check_save_v(
        "2",
        &[
            "!<GradingPrimaryTransform> {style: log}",
            "!<GradingPrimaryTransform> {style: linear, direction: inverse, name: gp}",
            "!<GradingPrimaryTransform> {style: video}",
            "!<GradingPrimaryTransform> {style: log, brightness: {rgb: [0.1, 0.12345678, 0], \
             master: 0.1}, contrast: {rgb: [1.1, 1, 1], master: 1.1}, gamma: {rgb: [1.1, 1.2, \
             1.3], master: 1}, saturation: 0.9, pivot: {black: 0.1, white: 1.1}, clamp: {black: \
             0.1, white: 1.1}}",
            "!<GradingPrimaryTransform> {style: log, pivot: {contrast: -0.2}}",
            "!<GradingPrimaryTransform> {style: log, pivot: {contrast: -0.1}}",
            "!<GradingPrimaryTransform> {style: log, contrast: {rgb: [1, 1, 1], master: 1.5}}",
            "!<GradingPrimaryTransform> {style: log, offset: {rgb: [1, 1, 1], master: 1}, lift: \
             {rgb: [1, 1, 1], master: 1}, exposure: {rgb: [1, 1, 1], master: 1}, gain: {rgb: [2, \
             2, 2], master: 2}}",
            "!<GradingPrimaryTransform> {style: linear, offset: {rgb: [0.1, 0.12345678, 0], \
             master: 0.1}, exposure: {rgb: [-1.1, 0.9, -0.01], master: 1.1}, contrast: {rgb: \
             [1.1, 1, 1], master: 1.1}, saturation: 1.1, direction: inverse}",
            "!<GradingPrimaryTransform> {style: linear, pivot: {contrast: 0.18, black: 0.5, \
             white: 2}}",
            "!<GradingPrimaryTransform> {style: linear, pivot: {black: 0.5}, clamp: {white: 1.1}}",
            "!<GradingPrimaryTransform> {style: linear, gamma: {rgb: [2, 2, 2], master: 2}}",
            "!<GradingPrimaryTransform> {style: video, lift: {rgb: [0.1, 0, 0], master: 0.1}, \
             gamma: {rgb: [1.1, 1, 1], master: 1.1}, gain: {rgb: [1.1, 1, 1], master: 1.1}, \
             offset: {rgb: [0, 0, 0], master: 0.5}, pivot: {contrast: 0.3, white: 1.1}, clamp: \
             {black: 0.1}}",
            "!<GradingPrimaryTransform> {style: video, contrast: {rgb: [2, 2, 2], master: 2}, \
             brightness: {rgb: [1, 1, 1], master: 1}}",
            "!<GradingPrimaryTransform> {style: log, brightness: {rgb: [.nan, .inf, -.inf], \
             master: -0.0}, saturation: .nan}",
            "!<GradingPrimaryTransform> {style: log, gamma: {rgb: [.nan, 1, 1], master: 1}, \
             pivot: {contrast: .nan, black: .nan}, clamp: {black: .nan, white: .nan}}",
            "!<GradingPrimaryTransform> {style: linear, offset: {rgb: [1e-30, 1e300, \
             4.9e-324], master: 0.12345678901234567}, saturation: 1.0000000000000002, pivot: \
             {contrast: 0.30000000000000004}}",
            "!<GradingPrimaryTransform> {style: video, clamp: {black: -1.7976931348623157e308, \
             white: 1.7976931348623157e308}}",
            "!<GradingPrimaryTransform> {style: video, clamp: {black: -1e308, white: 1e308}}",
            "!<GradingPrimaryTransform> {style: log, clamp: {black: -.inf, white: .inf}}",
            "!<GradingPrimaryTransform> {style: log, saturation: 3.4028234663852886e38, \
             brightness: {rgb: [1.1754943508222875e-38, 0, 0], master: 0}}",
            "!<GroupTransform> {children: [!<GradingPrimaryTransform> {style: log}, \
             !<GradingPrimaryTransform> {style: video, saturation: 2}]}",
        ],
    );
}

/// Version 1 configs refuse a GradingPrimaryTransform (`checkVersionConsistency`), in a group
/// too.
#[test]
fn grading_primary_transforms_are_refused_in_version_1() {
    check_refused_v(
        "1",
        &[
            "!<GradingPrimaryTransform> {style: log}",
            "!<GroupTransform> {children: [!<GradingPrimaryTransform> {style: video}]}",
        ],
    );
}

/// The GradingRGBCurveTransform's saver: a flow map when every curve is the default, a block
/// map otherwise; the bypass; the curves that aren't the default or whose slopes aren't, as
/// floats; special values and coordinates with more digits than the writer's precision.
#[test]
fn grading_rgb_curve_transforms_save_as_in_the_wheel() {
    check_save_v(
        "2",
        &[
            "!<GradingRGBCurveTransform> {style: log}",
            "!<GradingRGBCurveTransform> {style: linear, lintolog_bypass: true, direction: \
             inverse, name: gc}",
            "!<GradingRGBCurveTransform> {style: video, red: {control_points: [0, 0, 0.5, 0.5, \
             1, 1.123456]}}",
            "!<GradingRGBCurveTransform> {style: linear, red: {control_points: [-7, -7, 0, 0, 7, \
             7], slopes: [1, 1, 1]}}",
            "!<GradingRGBCurveTransform> {style: log, red: {control_points: [-0.2, 0, 0.5, 0.5, \
             1.2, 1.5]}, green: {control_points: [0, 0, 0.2, 0.5, 1, 1.5]}, blue: \
             {control_points: [0, 0, 0.1, 0.5, 1, 1.5], slopes: [0, 1, 1.1]}, master: \
             {control_points: [-1, -1, 0, 0.1, 0.5, 0.6, 1, 1.1]}, direction: inverse}",
            "!<GradingRGBCurveTransform> {style: log, green: {control_points: [0, .nan, 1, \
             .inf], slopes: [-0.0, 1e-30]}}",
            "!<GradingRGBCurveTransform> {style: log, green: {control_points: [0, \
             1.1754944e-38, 0.30000001, 0.123456789, 1, 3.4028234e38], slopes: [1.0000001, \
             0.99999994, 1e-45]}}",
            "!<GradingRGBCurveTransform> {style: video, master: {control_points: [0, 0, 1, 1], \
             slopes: [0, -0.0]}}",
            "!<GroupTransform> {children: [!<GradingRGBCurveTransform> {style: log}, \
             !<GradingRGBCurveTransform> {style: video, red: {control_points: [0, 0, 1, 2]}}]}",
        ],
    );
}

/// Version 1 configs refuse a GradingRGBCurveTransform (`checkVersionConsistency`), in a
/// group too.
#[test]
fn grading_rgb_curve_transforms_are_refused_in_version_1() {
    check_refused_v(
        "1",
        &[
            "!<GradingRGBCurveTransform> {style: log}",
            "!<GroupTransform> {children: [!<GradingRGBCurveTransform> {style: video}]}",
        ],
    );
}
