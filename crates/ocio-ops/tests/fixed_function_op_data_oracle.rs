// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `FixedFunctionStyle`'s names and `FixedFunctionOpData` against the wheel, through
//! `FixedFunctionTransform` and the oracle's `transform_text`:
//! - `FixedFunctionStyleToString`: the transform's `repr()` prints the style with it
//!   (`operator<<`, src/OpenColorIO/transforms/FixedFunctionTransform.cpp:153-176 @ v2.5.2);
//! - the two styles upstream doesn't implement: `FixedFunctionTransform::Create` converts the
//!   style with `FixedFunctionOpData::ConvertStyle`, which refuses them
//!   (FixedFunctionTransform.cpp:14-18, 38-41);
//! - `validate`: the transform's `validate` runs its data's and prefixes the message with
//!   `FixedFunctionTransform validation failed: ` (FixedFunctionTransform.cpp:86-99). A
//!   transform whose setters set the direction, the style (`ConvertStyle` in the current
//!   direction, FixedFunctionTransform.cpp:122-126) and the parameters is validated only
//!   there. The binding's constructor builds its data with the validating constructors
//!   (`FixedFunctionOpData(Style, const Params &)`, in the forward direction), whose messages
//!   have no prefix (src/bindings/python/transforms/PyFixedFunctionTransform.cpp:26-42);
//! - `equals`: the transform's compares its data with `FixedFunctionOpData::operator==`
//!   (FixedFunctionTransform.cpp:111-115).
//!
//! The cache ID and `isInverse` reach the wheel through the processors, with the op
//! (`crates/ocio/tests/fixed_function_transform_oracle.rs`, chunk 2.3e).

mod common;

use common::fixed_function::{ALL_STYLES, constructed, style_enum, unvalidated};
use ocio_ops::open_color_types::{
    FixedFunctionStyle, TransformDirection, fixed_function_style_to_string,
};
use ocio_ops::ops::fixedfunction::FixedFunctionOpStyle;
use ocio_ops::ops::fixedfunction::fixed_function_op_data::FixedFunctionOpData;
use ocio_testkit::transform_text::{Built, TransformTextRequest, f64_spec};
use serde_json::{Value, json};

const DIRECTIONS: [TransformDirection; 2] =
    [TransformDirection::Forward, TransformDirection::Inverse];

/// The transform's validation prefix (FixedFunctionTransform.cpp:86-99 @ v2.5.2).
const PREFIX: &str = "FixedFunctionTransform validation failed: ";

/// The style name in a FixedFunctionTransform's `repr()`: `<FixedFunction direction=forward,
/// style=ACES_Glow03>`.
fn repr_style(repr: &str) -> &str {
    let start = repr.find("style=").expect("a style") + "style=".len();
    let rest = &repr[start..];
    let end = rest.find([',', '>']).expect("the style's end");
    &rest[..end]
}

/// Every implemented style's name, as `repr()` prints it; the two unimplemented ones are
/// refused with upstream's message.
#[test]
fn style_names_match_the_wheel() {
    let transforms = ALL_STYLES
        .iter()
        .map(|&style| match style {
            FixedFunctionStyle::AcesGamutMap02 | FixedFunctionStyle::AcesGamutMap07 => json!({
                "class": "FixedFunctionTransform",
                "args": {"style": style_enum(style)},
            }),
            _ => unvalidated(style, json!([]), TransformDirection::Forward),
        })
        .collect();
    let reply = TransformTextRequest {
        transforms,
        pairs: Vec::new(),
    }
    .run();

    for (style, built) in ALL_STYLES.iter().zip(&reply.transforms) {
        match (fixed_function_style_to_string(*style), built) {
            (Ok(name), Built::Text(text)) => {
                assert_eq!(name, repr_style(&text.repr), "{style:?}");
            }
            (Err(port), Built::Raised(wheel)) => {
                assert_eq!(port.message(), wheel.message, "{style:?}");
                // `Create` refuses them through ConvertStyle, with the same text.
                let op =
                    FixedFunctionOpStyle::from_transform_style(*style, TransformDirection::Forward)
                        .unwrap_err();
                assert_eq!(op.message(), wheel.message, "{style:?}");
            }
            (port, wheel) => panic!("{style:?}: the port gives {port:?}, the wheel {wheel:?}"),
        }
    }
}

/// Parameters as a spec value: each double by its bits.
fn params_spec(params: &[f64]) -> Value {
    Value::Array(params.iter().map(|&v| f64_spec(v)).collect())
}

/// One transform: its spec for the wheel and the port's validation of the same data.
struct Case {
    label: String,
    spec: Value,
    /// The port's validation, with the message the wheel shows.
    port: Result<(), String>,
    /// The data, where the port builds it.
    data: Option<FixedFunctionOpData>,
}

/// A transform set up with its setters, which don't validate: the data that
/// `FixedFunctionTransform(ACES_GLOW_03)`, `setDirection`, `setStyle` and `setParams` give
/// (FixedFunctionTransform.cpp:38-41, 80-84, 122-126, 133-145 @ v2.5.2), validated by the
/// transform's `validate`.
fn setters(style: FixedFunctionStyle, dir: TransformDirection, params: &[f64]) -> Case {
    let mut data = FixedFunctionOpData::new(FixedFunctionOpStyle::AcesGlow03Fwd).unwrap();
    data.set_direction(dir);
    let op_style = FixedFunctionOpStyle::from_transform_style(style, data.direction()).unwrap();
    data.set_style(op_style);
    data.set_params(params.to_vec());
    Case {
        label: format!("setters {style:?} {dir:?} {params:?}"),
        spec: unvalidated(style, params_spec(params), dir),
        port: data
            .validate()
            .map_err(|e| format!("{PREFIX}{}", e.message())),
        data: Some(data),
    }
}

/// A transform built by the binding's constructor: the forward style with its parameters,
/// through the validating constructor (no prefix), then `setDirection` and `validate`
/// (PyFixedFunctionTransform.cpp:26-42 @ v2.5.2).
fn constructor(style: FixedFunctionStyle, dir: TransformDirection, params: &[f64]) -> Case {
    let port = FixedFunctionOpStyle::from_transform_style(style, TransformDirection::Forward)
        .and_then(|s| FixedFunctionOpData::with_params(s, params.to_vec()))
        .map_err(|e| e.message().to_string())
        .and_then(|mut data| {
            data.set_direction(dir);
            data.validate()
                .map_err(|e| format!("{PREFIX}{}", e.message()))
                .map(|()| data)
        });
    Case {
        label: format!("constructor {style:?} {dir:?} {params:?}"),
        spec: constructed(style, params_spec(params), dir),
        port: port.as_ref().map(|_| ()).map_err(Clone::clone),
        data: port.ok(),
    }
}

/// Valid parameters of each style that takes some: upstream's test values
/// (tests/cpu/ops/fixedfunction/FixedFunctionOpData_tests.cpp:71, 169, 219-220, 270-271,
/// FixedFunctionOpCPU_tests.cpp:564-569, 773, 866 @ v2.5.2).
fn valid_params(style: FixedFunctionStyle) -> Vec<f64> {
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
        AcesOutputTransform20 | AcesGamutCompress20 => {
            vec![100.0, 0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329]
        }
        AcesRgbToJmh20 => vec![
            0.7347, 0.2653, 0.0000, 1.0000, 0.0001, -0.0770, 0.32168, 0.33767,
        ],
        AcesTonescaleCompress20 => vec![1000.0],
        _ => Vec::new(),
    }
}

/// `params` with `value` at `i`.
fn with(params: &[f64], i: usize, value: f64) -> Vec<f64> {
    let mut p = params.to_vec();
    p[i] = value;
    p
}

/// Values that print with every kind of text and fall on, inside and outside every bound.
fn probe_values() -> Vec<f64> {
    let next_up = |v: f64| f64::from_bits(v.to_bits() + 1);
    let next_down = |v: f64| f64::from_bits(v.to_bits() - 1);
    vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.01,
        next_down(0.01),
        0.00999999,
        100.0,
        next_up(100.0),
        100.00001,
        1.001,
        next_down(1.001),
        65504.0,
        next_up(65504.0),
        0.9995,
        next_up(0.9995),
        10000.0,
        10000.5,
        10001.0,
        1.5,
        0.5,
        f64::from_bits(1),
        1e-300,
        1e300,
        f64::MAX,
        123456789.0,
        0.000123456789,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        -f64::NAN,
    ]
}

/// The transforms whose validation the test compares: every style in both directions with a
/// wrong number of parameters (each style's message names it in its direction), and each
/// checked parameter at the probe values, through the setters and the constructor.
fn validation_cases() -> Vec<Case> {
    use FixedFunctionStyle::*;
    let mut out = Vec::new();
    for style in ALL_STYLES {
        if matches!(style, AcesGamutMap02 | AcesGamutMap07) {
            continue;
        }
        let valid = valid_params(style);
        for dir in DIRECTIONS {
            for n in [0, 1, 2, 6, 7, 8, 9, 10, 13, 20] {
                out.push(setters(style, dir, &vec![1.5; n]));
            }
            out.push(setters(style, dir, &valid));
            out.push(constructor(style, dir, &valid));
            out.push(constructor(style, dir, &[0.5, 1.0]));
        }
        // The parameters each style checks.
        let checked: &[usize] = match style {
            AcesGamutComp13 => &[0, 1, 2, 3, 4, 5, 6],
            AcesOutputTransform20 | AcesTonescaleCompress20 | AcesGamutCompress20 => &[0],
            Rec2100Surround => &[0],
            LinToDoubleLog => &[0, 1, 2],
            LinToGammaLog => &[0, 1, 2, 5],
            _ => &[],
        };
        for &i in checked {
            for v in probe_values() {
                out.push(setters(
                    style,
                    TransformDirection::Inverse,
                    &with(&valid, i, v),
                ));
                out.push(constructor(
                    style,
                    TransformDirection::Forward,
                    &with(&valid, i, v),
                ));
            }
        }
    }
    // Several parameters out of bounds: the first one checked is reported.
    let gc = valid_params(AcesGamutComp13);
    out.push(setters(
        AcesGamutComp13,
        TransformDirection::Forward,
        &with(&with(&gc, 6, 0.), 2, 1.),
    ));
    out.push(setters(
        AcesGamutComp13,
        TransformDirection::Forward,
        &with(&with(&gc, 5, 2.), 0, 1e9),
    ));
    let dl = valid_params(LinToDoubleLog);
    out.push(setters(
        LinToDoubleLog,
        TransformDirection::Forward,
        &with(&with(&dl, 1, 1.0), 0, -1.),
    ));
    let gl = valid_params(LinToGammaLog);
    out.push(setters(
        LinToGammaLog,
        TransformDirection::Forward,
        &with(&with(&gl, 2, 0.0), 0, 1.),
    ));
    out.push(setters(
        LinToGammaLog,
        TransformDirection::Forward,
        &with(&with(&gl, 0, 1.0), 5, 0.),
    ));
    out
}

#[test]
fn validation_matches_the_wheel() {
    let cases = validation_cases();
    let reply = TransformTextRequest {
        transforms: cases.iter().map(|c| c.spec.clone()).collect(),
        pairs: Vec::new(),
    }
    .run();

    let mut failures = Vec::new();
    let mut refused = 0;
    for (case, built) in cases.iter().zip(&reply.transforms) {
        let wheel = match built {
            Built::Raised(e) => Err(e.message.clone()),
            Built::Text(text) => match &text.validate {
                Some(e) => Err(e.message.clone()),
                None => Ok(()),
            },
        };
        refused += usize::from(wheel.is_err());
        if case.port != wheel {
            failures.push(format!(
                "{}\n  wheel {wheel:?}\n  port  {:?}",
                case.label, case.port
            ));
        }
    }
    println!("{} transforms, {refused} refused by the wheel", cases.len());
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

#[test]
fn equality_matches_the_wheel() {
    use FixedFunctionStyle::*;
    let next_up = |v: f64| f64::from_bits(v.to_bits() + 1);
    let gc = valid_params(AcesGamutComp13);
    let mut cases = Vec::new();
    for dir in DIRECTIONS {
        cases.push(constructor(AcesGlow03, dir, &[]));
        cases.push(constructor(AcesGlow10, dir, &[]));
        cases.push(constructor(RgbToHsv, dir, &[]));
        cases.push(constructor(Rec2100Surround, dir, &[2.0]));
        cases.push(constructor(Rec2100Surround, dir, &[0.5]));
        cases.push(constructor(AcesGamutComp13, dir, &gc));
        cases.push(constructor(
            AcesGamutComp13,
            dir,
            &with(&gc, 6, next_up(1.2)),
        ));
        cases.push(setters(AcesGamutComp13, dir, &with(&gc, 6, f64::NAN)));
        cases.push(setters(AcesGamutComp13, dir, &with(&gc, 3, 0.0)));
        cases.push(setters(AcesGamutComp13, dir, &with(&gc, 3, -0.0)));
        cases.push(setters(AcesGlow03, dir, &[1.0]));
    }
    // The setters' style in the inverse direction: RGB_TO_HSV ignores it.
    cases.push(setters(RgbToHsv, TransformDirection::Inverse, &[]));

    let mut pairs = Vec::new();
    for i in 0..cases.len() {
        for j in 0..cases.len() {
            pairs.push((i, j));
        }
    }
    let reply = TransformTextRequest {
        transforms: cases.iter().map(|c| c.spec.clone()).collect(),
        pairs: pairs.clone(),
    }
    .run();

    let mut failures = Vec::new();
    let mut equal = 0;
    for ((i, j), wheel) in pairs.iter().zip(&reply.pairs) {
        let (a, b) = (&cases[*i], &cases[*j]);
        let (Some(da), Some(db)) = (&a.data, &b.data) else {
            panic!("{} or {} isn't built", a.label, b.label);
        };
        let wheel = wheel.unwrap_or_else(|| panic!("no equals for {} {}", a.label, b.label));
        equal += usize::from(wheel);
        if i == j {
            // The transform's `equals` is true for itself before it compares the data
            // (`if (this == &other) return true;`, FixedFunctionTransform.cpp:113 @ v2.5.2),
            // which a NaN parameter makes unequal to itself.
            if !wheel {
                failures.push(format!("{} == itself: wheel {wheel}", a.label));
            }
            continue;
        }
        if da.equals(db) != wheel || (da == db) != wheel {
            failures.push(format!("{} == {}: wheel {wheel}", a.label, b.label));
        }
    }
    println!("{} pairs, {equal} equal", pairs.len());
    assert!(
        failures.is_empty(),
        "{} of {} pairs differ:\n{}",
        failures.len(),
        pairs.len(),
        failures.join("\n")
    );
}

/// Every op style, with parameters its `validate` accepts (the CTF reader validates the op at
/// the element's end, src/OpenColorIO/fileformats/ctf/CTFReaderHelper.cpp:1482-1487 @ v2.5.2).
fn op_styles() -> Vec<(FixedFunctionOpStyle, Vec<f64>)> {
    use FixedFunctionOpStyle::*;
    let gamut_comp_13 = vec![1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];
    let output_20 = vec![
        100.0, 0.708, 0.292, 0.17, 0.797, 0.131, 0.046, 0.3127, 0.329,
    ];
    let gamma_log = vec![
        0.0,
        0.25,
        0.5,
        1.0,
        0.0,
        2.718281828459045,
        0.17883277,
        0.807825590164,
        1.0,
        -0.07116723,
    ];
    let double_log = vec![
        10.0, 0.25, 0.5, -1.0, 0.0, -1.0, 1.25, 1.0, 1.0, 1.0, 0.5, 1.0, 0.0,
    ];
    vec![
        (AcesRedMod03Fwd, vec![]),
        (AcesRedMod03Inv, vec![]),
        (AcesRedMod10Fwd, vec![]),
        (AcesRedMod10Inv, vec![]),
        (AcesGlow03Fwd, vec![]),
        (AcesGlow03Inv, vec![]),
        (AcesGlow10Fwd, vec![]),
        (AcesGlow10Inv, vec![]),
        (AcesDarkToDim10Fwd, vec![]),
        (AcesDarkToDim10Inv, vec![]),
        (AcesGamutComp13Fwd, gamut_comp_13.clone()),
        (AcesGamutComp13Inv, gamut_comp_13),
        (AcesOutputTransform20Fwd, output_20.clone()),
        (AcesOutputTransform20Inv, output_20.clone()),
        (AcesRgbToJmh20, output_20[1..].to_vec()),
        (AcesJmhToRgb20, output_20[1..].to_vec()),
        (AcesTonescaleCompress20Fwd, vec![100.0]),
        (AcesTonescaleCompress20Inv, vec![100.0]),
        (AcesGamutCompress20Fwd, output_20.clone()),
        (AcesGamutCompress20Inv, output_20),
        (Rec2100SurroundFwd, vec![0.78]),
        (Rec2100SurroundInv, vec![0.78]),
        (RgbToHsv, vec![]),
        (HsvToRgb, vec![]),
        (XyzToXyy, vec![]),
        (XyyToXyz, vec![]),
        (XyzToUvy, vec![]),
        (UvyToXyz, vec![]),
        (XyzToLuv, vec![]),
        (LuvToXyz, vec![]),
        (LinToPq, vec![]),
        (PqToLin, vec![]),
        (LinToGammaLog, gamma_log.clone()),
        (GammaLogToLin, gamma_log),
        (LinToDoubleLog, double_log.clone()),
        (DoubleLogToLin, double_log),
        (RgbToHsyLin, vec![]),
        (HsyLinToRgb, vec![]),
        (RgbToHsyLog, vec![]),
        (HsyLogToRgb, vec![]),
        (RgbToHsyVid, vec![]),
        (HsyVidToRgb, vec![]),
    ]
}

/// `GetStyle` against the wheel's CTF reader, which calls it on the `style` attribute of a
/// `FixedFunction` element (CTFReaderHelper.cpp:1427-1473 @ v2.5.2): every style's CTF name
/// as `to_str(false)` writes it, in upper and lower case (`GetStyle` compares with
/// `Platform::Strcasecmp`), the `Surround` alias, and names it refuses. The wheel reads each
/// file through a `FileTransform`; its processor's `FixedFunctionTransform` gives the style
/// and the direction, which the port's op style converts to (`ConvertStyle`, `getDirection`).
#[test]
fn ctf_style_names_match_the_wheel() {
    use ocio_testkit::oracle::{BatchCall, Oracle};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("ctf_style_names");
    std::fs::create_dir_all(&dir).unwrap();

    let mut cases: Vec<(String, Vec<f64>)> = Vec::new();
    for (style, params) in op_styles() {
        let name = style.to_str(false);
        for text in [
            name.to_string(),
            name.to_ascii_uppercase(),
            name.to_ascii_lowercase(),
        ] {
            cases.push((text, params.clone()));
        }
    }
    for text in ["Surround", "SURROUND", "surround"] {
        cases.push((text.to_string(), vec![0.78]));
    }
    for text in [
        "RedMod03Inv",
        "Rec2100Surround",
        "Unknown",
        "RGB_TO_HSV_",
        " RGB_TO_HSV",
    ] {
        cases.push((text.to_string(), vec![]));
    }

    let mut calls = Vec::new();
    for (i, (text, params)) in cases.iter().enumerate() {
        let params = if params.is_empty() {
            String::new()
        } else {
            let list: Vec<String> = params.iter().map(|p| format!("{p:?}")).collect();
            format!(" params=\"{}\"", list.join(" "))
        };
        let ctf = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ProcessList version=\"2\" id=\"ABCD\">\n    \
             <FixedFunction inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"{text}\"{params}/>\n\
             </ProcessList>\n"
        );
        let path = dir.join(format!("style_{i}.ctf"));
        std::fs::write(&path, ctf).unwrap();
        let src = path
            .to_str()
            .unwrap()
            .replace(std::path::MAIN_SEPARATOR, "/");
        calls.push(BatchCall {
            cmd: "processor_ops",
            args: json!({"transform": {"class": "FileTransform", "args": {"src": src}}}),
            blobs: vec![],
        });
    }
    // The files' contents aren't part of the cache key: never cache these calls.
    let responses = Oracle::get().batch(&calls, false);

    let mut failures = Vec::new();
    for ((text, params), response) in cases.iter().zip(responses) {
        let response = response.unwrap_or_else(|e| panic!("{text:?}: the oracle failed: {e}"));
        let port = FixedFunctionOpStyle::from_name(Some(text));
        match (response.result.get("exception"), port) {
            (Some(exception), Err(e)) => {
                let message = exception["message"].as_str().unwrap_or_default();
                if !message.contains(e.message()) {
                    failures.push(format!(
                        "{text:?}: wheel {message:?}, port {:?}",
                        e.message()
                    ));
                }
            }
            (Some(exception), Ok(style)) => {
                failures.push(format!("{text:?}: wheel {exception}, port {style:?}"));
            }
            (None, Err(e)) => {
                failures.push(format!(
                    "{text:?}: wheel {}, port {:?}",
                    response.result,
                    e.message()
                ));
            }
            (None, Ok(style)) => {
                let data = FixedFunctionOpData::with_params(style, params.clone()).unwrap();
                let child = &response.result["processor"]["group"]["children"][0]["getters"];
                let wheel = (child["getStyle"].clone(), child["getDirection"].clone());
                let port = (
                    style_enum(style.to_transform_style()),
                    common::fixed_function::direction_enum(data.direction()),
                );
                if wheel != port {
                    failures.push(format!("{text:?}: wheel {wheel:?}, port {port:?}"));
                }
            }
        }
    }
    println!("{} names", cases.len());
    assert!(
        failures.is_empty(),
        "{} of {} names differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}
