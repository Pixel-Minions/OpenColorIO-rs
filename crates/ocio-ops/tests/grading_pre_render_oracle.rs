// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The pre-render values of GradingPrimary and GradingTone against the wheel: a dynamic op's
//! GPU shader exposes them as uniforms (`gpu_shader`), which read the shader's copy of the
//! dynamic property (src/OpenColorIO/ops/gradingprimary/GradingPrimaryOpGPU.cpp,
//! src/OpenColorIO/ops/gradingtone/GradingToneOpGPU.cpp @ v2.5.2). Seeded random values of each
//! style and direction (the pow of 2 of the linear style's exposure and pivot among them), and
//! NaN values where two can meet: one NaN at a time, and random sets of NaNs each with its own
//! payload and sign, whose bits show which operand each wheel's machine code puts first
//! (`GradingPrimaryPreRender::update`'s per-platform orders).

use ocio_ops::open_color_types::{GradingStyle, TransformDirection};
use ocio_ops::ops::gradingprimary::grading_primary::{
    GradingPrimary, GradingPrimaryPreRender, GradingRgbm,
};
use ocio_ops::ops::gradingtone::grading_tone::{GradingRgbmsw, GradingTone, GradingTonePreRender};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::probe::Rng;
use serde_json::{Value, json};

fn f64_spec(v: f64) -> Value {
    json!({"f64": v.to_bits()})
}
fn rgbm_spec(v: &GradingRgbm) -> Value {
    json!({"object": {"class": "GradingRGBM",
                      "args": [f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue),
                               f64_spec(v.master)]}})
}
fn primary_spec(p: &GradingPrimary) -> Value {
    json!({"object": {"class": "GradingPrimary", "args": [{"enum": "GRADING_LOG"}], "attrs": [
        ["brightness", rgbm_spec(&p.brightness)],
        ["contrast", rgbm_spec(&p.contrast)],
        ["gamma", rgbm_spec(&p.gamma)],
        ["offset", rgbm_spec(&p.offset)],
        ["exposure", rgbm_spec(&p.exposure)],
        ["lift", rgbm_spec(&p.lift)],
        ["gain", rgbm_spec(&p.gain)],
        ["saturation", f64_spec(p.saturation)],
        ["pivot", f64_spec(p.pivot)],
        ["pivotBlack", f64_spec(p.pivot_black)],
        ["pivotWhite", f64_spec(p.pivot_white)],
        ["clampBlack", f64_spec(p.clamp_black)],
        ["clampWhite", f64_spec(p.clamp_white)],
    ]}})
}
fn rgbmsw_spec(v: &GradingRgbmsw) -> Value {
    json!({"object": {"class": "GradingRGBMSW",
                      "args": [f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue),
                               f64_spec(v.master), f64_spec(v.start), f64_spec(v.width)]}})
}
fn tone_spec(t: &GradingTone) -> Value {
    json!({"object": {"class": "GradingTone", "args": [{"enum": "GRADING_LOG"}], "attrs": [
        ["blacks", rgbmsw_spec(&t.blacks)],
        ["shadows", rgbmsw_spec(&t.shadows)],
        ["midtones", rgbmsw_spec(&t.midtones)],
        ["highlights", rgbmsw_spec(&t.highlights)],
        ["whites", rgbmsw_spec(&t.whites)],
        ["scontrast", f64_spec(t.scontrast)],
    ]}})
}

fn u(rng: &mut Rng, lo: f64, hi: f64) -> f64 {
    lo + (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64 * (hi - lo)
}

fn pick(rng: &mut Rng, lo: f64, hi: f64, specials: &[f64]) -> f64 {
    if rng.next_u64().is_multiple_of(6) {
        specials[(rng.next_u64() % specials.len() as u64) as usize]
    } else {
        u(rng, lo, hi)
    }
}

fn rgbm(rng: &mut Rng, lo: f64, hi: f64, specials: &[f64]) -> GradingRgbm {
    GradingRgbm::new(
        pick(rng, lo, hi, specials),
        pick(rng, lo, hi, specials),
        pick(rng, lo, hi, specials),
        pick(rng, lo, hi, specials),
    )
}

const STYLES: [(GradingStyle, &str); 3] = [
    (GradingStyle::Log, "GRADING_LOG"),
    (GradingStyle::Lin, "GRADING_LIN"),
    (GradingStyle::Video, "GRADING_VIDEO"),
];
const DIRS: [(TransformDirection, &str); 2] = [
    (TransformDirection::Forward, "TRANSFORM_DIR_FORWARD"),
    (TransformDirection::Inverse, "TRANSFORM_DIR_INVERSE"),
];

fn uni<'a>(result: &'a Value, suffix: &str) -> Option<&'a Value> {
    result["shader"]["uniforms"]
        .as_array()?
        .iter()
        .find(|u| u["name"].as_str().unwrap().ends_with(suffix))
        .map(|u| &u["value"])
}

fn f3(v: &[f32; 3]) -> Value {
    json!(v.iter().map(|x| x.to_bits()).collect::<Vec<_>>())
}

#[test]
fn primary_pre_render_matches_wheel_uniforms() {
    let mut rng = Rng::new(0x7e5f_0001);
    let sp = [
        0.0,
        -0.0,
        1.0,
        1.0e-300,
        150.0,
        -150.0,
        127.9,
        128.0,
        -149.0,
        1e308,
        f64::INFINITY,
        0.3,
        1.0 / 3.0,
    ];
    let mut cases = Vec::new();
    for i in 0..600 {
        let (style, _) = STYLES[i % 3];
        let dir = DIRS[(i / 3) % 2].0;
        let mut p = GradingPrimary::new(style);
        p.brightness = rgbm(&mut rng, -100.0, 100.0, &sp);
        p.contrast = rgbm(&mut rng, 0.01, 3.0, &sp);
        p.gamma = rgbm(&mut rng, 0.01, 3.0, &sp);
        p.offset = rgbm(&mut rng, -1.0, 1.0, &sp);
        p.exposure = rgbm(&mut rng, -10.0, 10.0, &sp);
        p.lift = rgbm(&mut rng, -1.0, 1.0, &sp);
        p.gain = rgbm(&mut rng, -2.0, 3.0, &sp);
        p.pivot = pick(
            &mut rng,
            -3.0,
            3.0,
            &[1100.0, -1100.0, 0.0, 1.0, -1.0, 0.5, 1e-310],
        );
        p.pivot_black = pick(&mut rng, -0.5, 0.5, &sp);
        p.pivot_white = pick(&mut rng, 0.5, 1.5, &sp);
        if rng.next_u64().is_multiple_of(4) {
            p = GradingPrimary::new(style);
            if rng.next_u64().is_multiple_of(2) {
                p.exposure.red = u(&mut rng, -3.0, 3.0);
            }
        }
        cases.push((style, dir, p));
    }
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(style, dir, p)| {
            let sn = STYLES.iter().find(|s| s.0 == *style).unwrap().1;
            let dn = DIRS.iter().find(|d| d.0 == *dir).unwrap().1;
            BatchCall {
                cmd: "gpu_shader",
                args: json!({
                    "transform": {"class": "GradingPrimaryTransform",
                                  "args": {"values": primary_spec(p), "style": {"enum": sn},
                                           "dynamic": true, "dir": {"enum": dn}}},
                    "shader": {"language": "GPU_LANGUAGE_GLSL_4_0"},
                }),
                blobs: vec![],
            }
        })
        .collect();
    let replies = Oracle::get().batch(&calls, true);
    let mut compared = 0;
    let mut mismatches = Vec::new();
    for (i, ((style, dir, p), reply)) in cases.iter().zip(replies).enumerate() {
        let reply = reply.unwrap_or_else(|e| panic!("case {i}: {e}"));
        let r = &reply.result;
        if r.get("exception").is_some() {
            continue;
        }
        let mut pr = GradingPrimaryPreRender::default();
        pr.update(*style, *dir, p);
        let mut check = |name: &str, port: Value| {
            if let Some(w) = uni(r, &format!("_{name}")) {
                if *w != port {
                    mismatches.push(format!(
                        "case {i} {style:?} {dir:?} {name}: wheel {w} port {port} ({p:?})"
                    ));
                }
            } else {
                mismatches.push(format!("case {i}: no uniform {name}"));
            }
        };
        match style {
            GradingStyle::Log => {
                check("brightness", f3(pr.get_brightness()));
                check("contrast", f3(pr.get_contrast()));
                check("gamma", f3(pr.get_gamma()));
                check("pivot", json!(pr.get_pivot().to_bits()));
            }
            GradingStyle::Lin => {
                check("offset", f3(pr.get_offset()));
                check("exposure", f3(pr.get_exposure()));
                check("contrast", f3(pr.get_contrast()));
                check("pivot", json!(pr.get_pivot().to_bits()));
            }
            GradingStyle::Video => {
                check("gamma", f3(pr.get_gamma()));
                check("offset", f3(pr.get_offset()));
                check("slope", f3(pr.get_slope()));
            }
        }
        check("localBypass", json!(pr.get_local_bypass()));
        compared += 1;
    }
    eprintln!("primary compared {compared} of {}", cases.len());
    assert!(
        mismatches.is_empty(),
        "{}\n{:#?}",
        mismatches.len(),
        &mismatches[..mismatches.len().min(20)]
    );
    assert!(compared > 300);
}

#[test]
fn tone_pre_render_matches_wheel_uniforms() {
    let mut rng = Rng::new(0x7e5f_0002);
    let mut cases = Vec::new();
    for i in 0..600 {
        let (style, _) = STYLES[i % 3];
        let mut t = GradingTone::new(style);
        let r = |rng: &mut Rng, lo, hi| {
            if rng.next_u64().is_multiple_of(3) {
                1.0
            } else {
                u(rng, lo, hi)
            }
        };
        let k = rng.next_u64() % 4;
        for (j, v) in [&mut t.blacks, &mut t.midtones, &mut t.whites]
            .into_iter()
            .enumerate()
        {
            if k == 0 || j as u64 % 3 == rng.next_u64() % 3 {
                v.red = r(&mut rng, 0.1, 1.9);
                v.green = r(&mut rng, 0.1, 1.9);
                v.blue = r(&mut rng, 0.1, 1.9);
                v.master = r(&mut rng, 0.1, 1.9);
                v.width = if rng.next_u64().is_multiple_of(2) {
                    u(&mut rng, 0.01, 3.0)
                } else {
                    v.width
                };
                v.start = if rng.next_u64().is_multiple_of(2) {
                    v.start + u(&mut rng, -0.5, 0.5)
                } else {
                    v.start
                };
            }
        }
        for v in [&mut t.shadows, &mut t.highlights] {
            if k == 0 || rng.next_u64().is_multiple_of(2) {
                v.red = r(&mut rng, 0.2, 1.8);
                v.green = r(&mut rng, 0.2, 1.8);
                v.blue = r(&mut rng, 0.2, 1.8);
                v.master = r(&mut rng, 0.2, 1.8);
            }
        }
        if rng.next_u64().is_multiple_of(2) {
            t.shadows.start = t.shadows.width + u(&mut rng, 0.0, 1.0);
        }
        if rng.next_u64().is_multiple_of(2) {
            t.highlights.start = t.highlights.width - u(&mut rng, 0.0, 1.0);
        }
        if rng.next_u64().is_multiple_of(2) {
            t.scontrast = u(&mut rng, 0.01, 1.99);
        }
        cases.push((style, t));
    }
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(style, t)| {
            let sn = STYLES.iter().find(|s| s.0 == *style).unwrap().1;
            BatchCall {
                cmd: "gpu_shader",
                args: json!({
                    "transform": {"class": "GradingToneTransform",
                                  "args": {"values": tone_spec(t), "style": {"enum": sn},
                                           "dynamic": true}},
                    "shader": {"language": "GPU_LANGUAGE_GLSL_4_0"},
                }),
                blobs: vec![],
            }
        })
        .collect();
    let replies = Oracle::get().batch(&calls, true);
    let mut compared = 0;
    let mut mismatches = Vec::new();
    for (i, ((style, t), reply)) in cases.iter().zip(replies).enumerate() {
        let reply = reply.unwrap_or_else(|e| panic!("case {i}: {e}"));
        let r = &reply.result;
        if r.get("exception").is_some() {
            continue;
        }
        let mut pr = GradingTonePreRender::new(*style);
        pr.update(t);
        for (name, v) in [
            ("blacksStart", pr.blacks_start),
            ("blacksWidth", pr.blacks_width),
            ("shadowsStart", pr.shadows_start),
            ("shadowsWidth", pr.shadows_width),
            ("highlightsStart", pr.highlights_start),
            ("highlightsWidth", pr.highlights_width),
            ("whitesStart", pr.whites_start),
            ("whitesWidth", pr.whites_width),
        ] {
            match uni(r, &format!("_{name}")) {
                Some(w) if *w == json!(v.to_bits()) => {}
                w => mismatches.push(format!(
                    "case {i} {style:?} {name}: wheel {w:?} port {v} ({})",
                    t
                )),
            }
        }
        match uni(r, "_localBypass") {
            Some(w) if *w == json!(pr.local_bypass) => {}
            w => mismatches.push(format!("case {i} localBypass {w:?}")),
        }
        compared += 1;
    }
    eprintln!("tone compared {compared} of {}", cases.len());
    assert!(
        mismatches.is_empty(),
        "{}\n{:#?}",
        mismatches.len(),
        &mismatches[..mismatches.len().min(20)]
    );
    assert!(compared > 300);
}

#[test]
fn primary_pre_render_nan_targets() {
    let neg_nan = f64::from_bits(0xfff8_0000_0000_0000);
    let pos_nan = f64::from_bits(0x7ff8_0000_0000_0000);
    let pay_nan = f64::from_bits(0x7ff8_0000_0000_1234);
    let mut cases = Vec::new();
    for (style, _) in STYLES {
        for (dir, _) in DIRS {
            for field in 0..14 {
                for nan in [pos_nan, neg_nan, pay_nan] {
                    let mut p = GradingPrimary::new(style);
                    match field {
                        0 => p.brightness.red = nan,
                        1 => p.brightness.master = nan,
                        2 => p.contrast.red = nan,
                        3 => p.contrast.master = nan,
                        4 => p.gamma.red = nan,
                        5 => p.offset.red = nan,
                        6 => p.offset.master = nan,
                        7 => p.exposure.red = nan,
                        8 => p.lift.red = nan,
                        9 => p.gain.red = nan,
                        10 => p.pivot = nan,
                        11 => p.pivot_black = nan,
                        12 => p.pivot_white = nan,
                        _ => {
                            p.brightness.red = pos_nan;
                            p.brightness.master = neg_nan;
                            p.offset.red = neg_nan;
                            p.offset.master = pay_nan;
                            p.lift.red = neg_nan;
                        }
                    }
                    cases.push((style, dir, p, field));
                }
            }
            // Random sets of NaN values, each with its own payload and sign.
            let mut rng = Rng::new(0x9a17_0bad);
            for field in 14..74 {
                let mut p = GradingPrimary::new(style);
                let mut k = 1u64;
                let mut values: Vec<&mut f64> = Vec::new();
                for v in [
                    &mut p.brightness,
                    &mut p.contrast,
                    &mut p.gamma,
                    &mut p.offset,
                    &mut p.exposure,
                    &mut p.lift,
                    &mut p.gain,
                ] {
                    values.extend([&mut v.red, &mut v.green, &mut v.blue, &mut v.master]);
                }
                values.extend([&mut p.pivot, &mut p.pivot_black, &mut p.pivot_white]);
                for v in values {
                    if rng.next_u64().is_multiple_of(2) {
                        let sign = if rng.next_u64().is_multiple_of(2) {
                            0
                        } else {
                            1u64 << 63
                        };
                        *v = f64::from_bits(0x7ff8_0000_0000_0000 | sign | (k << 32));
                    }
                    k += 1;
                }
                cases.push((style, dir, p, field));
            }
        }
    }
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(style, dir, p, _)| {
            let sn = STYLES.iter().find(|s| s.0 == *style).unwrap().1;
            let dn = DIRS.iter().find(|d| d.0 == *dir).unwrap().1;
            BatchCall {
                cmd: "gpu_shader",
                args: json!({
                    "transform": {"class": "GradingPrimaryTransform",
                                  "args": {"values": primary_spec(p), "style": {"enum": sn},
                                           "dynamic": true, "dir": {"enum": dn}}},
                    "shader": {"language": "GPU_LANGUAGE_GLSL_4_0"},
                }),
                blobs: vec![],
            }
        })
        .collect();
    let replies = Oracle::get().batch(&calls, true);
    let mut out = Vec::new();
    let mut refused = 0;
    for ((style, dir, p, field), reply) in cases.iter().zip(replies) {
        let r = &reply.unwrap().result;
        if r.get("exception").is_some() {
            refused += 1;
            continue;
        }
        let mut pr = GradingPrimaryPreRender::default();
        pr.update(*style, *dir, p);
        let checks: Vec<(&str, Value)> = vec![
            ("brightness", f3(pr.get_brightness())),
            ("contrast", f3(pr.get_contrast())),
            ("gamma", f3(pr.get_gamma())),
            ("pivot", json!(pr.get_pivot().to_bits())),
            ("offset", f3(pr.get_offset())),
            ("exposure", f3(pr.get_exposure())),
            ("slope", f3(pr.get_slope())),
            ("localBypass", json!(pr.get_local_bypass())),
        ];
        for (name, port) in checks {
            if let Some(w) = uni(r, &format!("_{name}"))
                && *w != port
            {
                out.push(format!(
                    "{style:?} {dir:?} field {field} {name}: wheel {w} port {port}"
                ));
            }
        }
    }
    eprintln!("refused {refused} of {}", cases.len());
    for l in &out {
        eprintln!("{l}");
    }
    assert!(out.is_empty(), "{} mismatches", out.len());
}
