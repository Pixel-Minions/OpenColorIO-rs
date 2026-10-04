// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `LogUtil::ConvertLogParameters` against the wheel, bit for bit, through CTF files.
//!
//! The wheel reads a CTF `Log` element's style and legacy `LogParams` (gamma, refWhite,
//! refBlack, highlight, shadow, per channel or for all), converts them in
//! `CTFReaderLogElt::end` (src/OpenColorIO/fileformats/ctf/CTFReaderHelper.cpp:3587-3619 @
//! v2.5.2): `GetLogDirection`, `ConvertLogParameters` (its errors wrapped as "Parameters are
//! not valid: '<message>'."), then the op data's `validate` (wrapped as "Log is not valid:
//! '<message>'."). The oracle's `processor_ops` writes out the processor's
//! `createGroupTransform()`, whose Log transform carries the op data's base, direction and
//! parameters as exact doubles. The port converts the same parameters with
//! `convert_log_parameters` and validates the data.
//!
//! The CTF reader itself is WP 1.6, so the test takes the message out of the reader's wrapping
//! (and the file loader's around it) by its two prefixes.

use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::log::log_op_data::{LogOpData, Params};
use ocio_ops::ops::log::log_utils::{
    CtfChannel, CtfParams, LogStyle, convert_log_parameters, convert_style_to_string, ctf_values,
    get_log_direction,
};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::probe::Rng;
use serde_json::{Value, json};

/// One channel's legacy parameters: gamma, refWhite, refBlack, highlight, shadow.
type Legacy = [f64; 5];

/// The data's base, direction and parameters, or the reader's message.
type Outcome = Result<(f64, TransformDirection, [Params; 3]), String>;

#[derive(Debug, Clone)]
struct Case {
    style: LogStyle,
    /// Red, green, blue; `None` for the styles without parameters.
    channels: Option<[Legacy; 3]>,
}

impl Case {
    /// The CTF file: one `LogParams` for all channels when they are equal, else one per
    /// channel. Numbers are written in Rust's shortest round-trip form.
    fn ctf(&self) -> String {
        let mut params = String::new();
        if let Some(channels) = &self.channels {
            let one = |chan: &str, p: &Legacy| {
                format!(
                    "    <LogParams{chan} gamma=\"{}\" refWhite=\"{}\" refBlack=\"{}\" \
                     highlight=\"{}\" shadow=\"{}\" />\n",
                    p[0], p[1], p[2], p[3], p[4]
                )
            };
            if channels[0] == channels[1] && channels[0] == channels[2] {
                params += &one("", &channels[0]);
            } else {
                for (chan, p) in ["R", "G", "B"].iter().zip(channels) {
                    params += &one(&format!(" channel=\"{chan}\""), p);
                }
            }
        }
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <ProcessList version=\"1.3\" id=\"a\">\n  \
             <Log inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"{}\">\n{params}  </Log>\n\
             </ProcessList>\n",
            convert_style_to_string(self.style)
        )
    }

    /// The port: the reader's conversion and validation, with its messages.
    fn port(&self) -> Outcome {
        let mut ctf = CtfParams::default();
        ctf.style = self.style;
        if let Some(channels) = &self.channels {
            for (chan, p) in [CtfChannel::Red, CtfChannel::Green, CtfChannel::Blue]
                .into_iter()
                .zip(channels)
            {
                let values = ctf.get_mut(chan);
                values[ctf_values::GAMMA] = p[0];
                values[ctf_values::REF_WHITE] = p[1];
                values[ctf_values::REF_BLACK] = p[2];
                values[ctf_values::HIGHLIGHT] = p[3];
                values[ctf_values::SHADOW] = p[4];
            }
        }
        let mut base = 2.0;
        let (mut r, mut g, mut b) = (Params::new(), Params::new(), Params::new());
        let dir = get_log_direction(self.style);
        convert_log_parameters(&ctf, &mut base, &mut r, &mut g, &mut b)
            .map_err(|e| format!("Parameters are not valid: '{}'.", e.message()))?;
        let data = LogOpData::from_channel_params(base, r.clone(), g.clone(), b.clone(), dir)
            .map_err(|e| e.message().to_string())?;
        data.validate()
            .map_err(|e| format!("Log is not valid: '{}'.", e.message()))?;
        Ok((base, dir, [r, g, b]))
    }
}

/// The wheel's conversion, from `processor_ops`: the Log transform's base, direction and
/// parameters, or the reader's message.
fn wheel(result: &Value) -> Outcome {
    if let Some(e) = result.get("exception") {
        let message = e["message"].as_str().unwrap();
        // "... failed with: At line N: <the reader's message>" (the file loader and
        // `ThrowM`'s line prefix).
        let at = message
            .find("Parameters are not valid: ")
            .or_else(|| message.find("Log is not valid: "))
            .unwrap_or_else(|| panic!("an unexpected message: {message}"));
        return Err(message[at..].to_string());
    }
    let children = result["processor"]["group"]["children"].as_array().unwrap();
    assert_eq!(children.len(), 1, "{children:?}");
    let getters = &children[0]["getters"];
    let f64_of = |v: &Value| f64::from_bits(v["f64"].as_u64().unwrap());
    let base = f64_of(&getters["getBase"]);
    let dir = match getters["getDirection"]["enum"].as_str().unwrap() {
        "TRANSFORM_DIR_FORWARD" => TransformDirection::Forward,
        "TRANSFORM_DIR_INVERSE" => TransformDirection::Inverse,
        other => panic!("{other}"),
    };
    let class = children[0]["class"].as_str().unwrap();
    let mut params: [Params; 3] = Default::default();
    if class == "LogTransform" {
        // Default parameters: the transform has no getters for them.
        for p in &mut params {
            *p = vec![1.0, 0.0, 1.0, 0.0];
        }
    } else {
        assert_eq!(class, "LogAffineTransform");
        // `LogAffineParameter` order: LOG_SIDE_SLOPE, LOG_SIDE_OFFSET, LIN_SIDE_SLOPE,
        // LIN_SIDE_OFFSET.
        for getter in [
            "getLogSideSlopeValue",
            "getLogSideOffsetValue",
            "getLinSideSlopeValue",
            "getLinSideOffsetValue",
        ] {
            let values = getters[getter].as_array().unwrap();
            for (p, v) in params.iter_mut().zip(values) {
                p.push(f64_of(v));
            }
        }
    }
    Ok((base, dir, params))
}

/// Legacy parameters: upstream's tests' (LogOpCPU_tests.cpp, LogUtils_tests.cpp @ v2.5.2),
/// gammas at the 0.01 limit (compared with a `float` 0.01), refBlack close to refWhite (the
/// -0.0001 clamp), equal ends, reversed ends, large and tiny values, and random valid ones.
fn legacy_sets() -> Vec<[Legacy; 3]> {
    let up = [
        [0.5, 685., 93., 0.8, 0.0004],
        [0.6, 684., 94., 0.9, 0.0005],
        [0.65, 683., 95., 1.0, 0.0003],
    ];
    let mut out = vec![
        up,
        [up[0]; 3],
        [
            [4.6, 758., 30., 0.7, 0.4],
            [2.6, 300., 42., 0.8, 0.1],
            [4.6, 758., 30., 0.7, 0.4],
        ],
        [
            [2.4, 410., 256., 0.2, 0.1],
            [3.5, 620., 485., 0.7, 0.6],
            [4.6, 730., 558., 0.9, 0.7],
        ],
    ];
    let mut singles: Vec<Legacy> = vec![
        // Gamma at the limit: 0.01 is above 0.01f, the double of 0.01f isn't.
        [0.01, 685., 95., 1.0, 0.0],
        [f64::from(0.01f32), 685., 95., 1.0, 0.0],
        [f64::from(0.01f32).next_up(), 685., 95., 1.0, 0.0],
        [0.005, 685., 95., 1.0, 0.0],
        [0.0, 685., 95., 1.0, 0.0],
        [-0.6, 685., 95., 1.0, 0.0],
        // refBlack close to refWhite: the exponent is clamped at -0.0001.
        [0.6, 685., 684.99, 1.0, 0.0],
        [0.6, 685., 684.9999, 1.0, 0.0],
        [0.6, 685., 685. - 1e-9, 1.0, 0.0],
        [0.6, 685., 685., 1.0, 0.0],
        [0.6, 95., 685., 1.0, 0.0],
        [0.6, -0.0, 0.0, 1.0, 0.0],
        // Highlight and shadow.
        [0.6, 685., 95., 1.0, 1.0],
        [0.6, 685., 95., 0.0, 1.0],
        [0.6, 685., 95., 1.0, -1.0],
        [0.6, 685., 95., 1e-300, 0.0],
        [0.6, 685., 95., 1e300, -1e300],
        [0.6, 685., 95., 1.0 + 1e-15, 1.0],
        // Large and tiny values.
        [1e300, 685., 95., 1.0, 0.0],
        [1e-300, 685., 95., 1.0, 0.0],
        [0.6, 1e300, -1e300, 1.0, 0.0],
        [0.6, 1e300, 1e299, 1.0, 0.0],
        [0.6, 1e-300, 0.0, 1.0, 0.0],
        [1e5, 1023., 0., 1.0, 0.0],
        [0.6, 1023., 1022., 1e-30, 0.0],
    ];
    let mut rng = Rng::new(0x4c4f_4743);
    let unit = |rng: &mut Rng| (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
    for _ in 0..48 {
        let gamma = 0.02 + 4.0 * unit(&mut rng);
        let ref_white = 300.0 + 723.0 * unit(&mut rng);
        let ref_black = ref_white * unit(&mut rng);
        let shadow = unit(&mut rng) - 0.5;
        let highlight = shadow + 2.0 * unit(&mut rng) + 1e-6;
        singles.push([gamma, ref_white, ref_black, highlight, shadow]);
    }
    for (k, single) in singles.iter().enumerate() {
        out.push([*single; 3]);
        // The same on one channel only.
        let mut mixed = up;
        mixed[k % 3] = *single;
        out.push(mixed);
    }
    out
}

#[test]
fn ctf_conversion_matches_the_wheel() {
    let mut cases = Vec::new();
    for style in [LogStyle::LogToLin, LogStyle::LinToLog] {
        for channels in legacy_sets() {
            cases.push(Case {
                style,
                channels: Some(channels),
            });
        }
    }
    for style in [
        LogStyle::Log10,
        LogStyle::Log2,
        LogStyle::AntiLog10,
        LogStyle::AntiLog2,
    ] {
        cases.push(Case {
            style,
            channels: None,
        });
    }

    let dir = std::env::temp_dir().join(format!("ocio-rs-log-ctf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut specs = Vec::new();
    for (k, case) in cases.iter().enumerate() {
        let path = dir.join(format!("log{k}.ctf"));
        std::fs::write(&path, case.ctf()).unwrap();
        specs.push(json!({
            "transform": {"class": "FileTransform", "args": {"src": path.to_str().unwrap()}},
            "optimization": "OPTIMIZATION_NONE",
        }));
    }
    let calls: Vec<BatchCall<'_>> = specs
        .into_iter()
        .map(|args| BatchCall {
            cmd: "processor_ops",
            args,
            blobs: vec![],
        })
        .collect();
    let results: Vec<Value> = Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .map(|r| r.unwrap_or_else(|e| panic!("{e}")).result)
        .collect();
    std::fs::remove_dir_all(&dir).ok();

    let bits = |r: &Outcome| {
        r.clone().map(|(base, dir, params)| {
            (
                base.to_bits(),
                dir,
                params.map(|p| p.iter().map(|v| v.to_bits()).collect::<Vec<_>>()),
            )
        })
    };
    let mut failures = Vec::new();
    let mut refused = 0;
    for (case, result) in cases.iter().zip(&results) {
        let wheel = wheel(result);
        refused += usize::from(wheel.is_err());
        let port = case.port();
        if bits(&port) != bits(&wheel) {
            failures.push(format!("{case:?}\n  wheel {wheel:?}\n  port  {port:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
    // Both outcomes are covered.
    assert!(refused > 0 && refused < cases.len(), "{refused}");
}
