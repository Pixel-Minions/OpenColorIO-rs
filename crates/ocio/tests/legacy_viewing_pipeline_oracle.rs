// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `LegacyViewingPipeline` against the wheel, through the oracle's `object_calls`.
//!
//! Generated pipelines over upstream's app helpers test config, a config made for the
//! pipeline's paths (shared views of the display's own color space, a view transform, a named
//! transform, data color spaces, looks with fall-backs, the scene-linear and color timing
//! roles), upstream's `context_test1` config (a look whose file depends on the context) and
//! every built-in config. Each case sets a display view transform (or none), the CCs, the
//! channel view and the looks override, then compares the pipeline's text, `getProcessor`'s
//! processor (cache ID, flags, metadata, `createGroupTransform()`) or exception, the log, and
//! the pixels of its default CPU processor and of its CPU processor without optimization, at
//! extreme values.
//!
//! The pixels go through the binding's `applyRGBA(list)`, which converts each value from and
//! to a Python float: signalling NaNs can't pass that way unchanged, so the probe values have
//! quiet NaNs only (each with a payload of its own).
//!
//! The results depend on the CPU (the processors' kernels): this target is in the
//! `cpu-tests` alias.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use common::transforms::compare_processor;
use ocio::{
    BuiltinConfigRegistry, CdlTransform, Config, Context, DisplayViewTransform, ExponentTransform,
    ExposureContrastStyle, ExposureContrastTransform, FixedFunctionStyle, FixedFunctionTransform,
    LegacyViewingPipeline, LogTransform, MatrixTransform, OptimizationFlags, Processor, Transform,
    TransformDirection,
};
use ocio_ops::image_desc::PackedImageDesc;
use ocio_ops::logging::{reset_to_default_logging_function, set_logging_function};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::Response;
use ocio_testkit::oracle_values::{bytes, log};
use ocio_testkit::processor_ops::ProcessorOpsReply;
use serde_json::{Value, json};

// ---------------------------------------------------------------------------------------------
// Configs

/// The config of upstream's app helpers tests (tests/cpu/apphelpers/configs.data:5-173 @
/// v2.5.2).
const CATEGORY_TEST_CONFIG: &str = include_str!("data/apphelpers_category_test_config.ocio");

/// A config for the pipeline's paths: a shared view whose display color space is the
/// display's own (`<USE_DISPLAY_NAME>`), a view with a view transform, a view of a named
/// transform, views of a data color space and of no color space, views with looks (one with a
/// fall-back, one with a look of no transform), and the scene-linear and color timing roles.
const PIPELINE_CONFIG: &str = r#"ocio_profile_version: 2.1

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  color_timing: log
  default: raw
  scene_linear: lin

file_rules:
  - !<Rule> {name: Default, colorspace: default}

shared_views:
  - !<View> {name: shared_vt, view_transform: vt_scene, display_colorspace: <USE_DISPLAY_NAME>, looks: +grade}

displays:
  d_srgb:
    - !<View> {name: plain, colorspace: srgb}
    - !<View> {name: graded, colorspace: srgb, looks: "+grade, -contrast"}
    - !<View> {name: fallback, colorspace: srgb, looks: missing|grade}
    - !<View> {name: noop_look, colorspace: srgb, looks: noop}
    - !<View> {name: vt, view_transform: vt_scene, display_colorspace: d_srgb, looks: contrast}
    - !<View> {name: named, colorspace: nt_view}
    - !<View> {name: data, colorspace: raw, looks: grade}
    - !<Views> [shared_vt]
  d_other:
    - !<View> {name: plain, colorspace: log}
    - !<Views> [shared_vt]

active_displays: []
active_views: []

looks:
  - !<Look>
    name: grade
    process_space: log
    transform: !<CDLTransform> {slope: [1.1, 1, 0.9], offset: [0.01, 0, -0.02], power: [1, 1.2, 1]}

  - !<Look>
    name: contrast
    process_space: lin
    transform: !<ExponentTransform> {value: [1.2, 1.2, 1.2, 1]}
    inverse_transform: !<ExponentTransform> {value: [1.3, 1.3, 1.3, 1]}

  - !<Look>
    name: noop
    process_space: srgb

view_transforms:
  - !<ViewTransform>
    name: vt_scene
    from_scene_reference: !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1], direction: inverse}

display_colorspaces:
  - !<ColorSpace>
    name: d_srgb
    from_display_reference: !<ExponentTransform> {value: [2.4, 2.4, 2.4, 1], direction: inverse}

  - !<ColorSpace>
    name: d_other
    from_display_reference: !<MatrixTransform> {offset: [0.05, 0, 0, 0]}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true

  - !<ColorSpace>
    name: lin

  - !<ColorSpace>
    name: log
    to_scene_reference: !<LogTransform> {base: 2, direction: inverse}

  - !<ColorSpace>
    name: srgb
    from_scene_reference: !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1], direction: inverse}

  - !<ColorSpace>
    name: cam
    isdata: false
    to_scene_reference: !<MatrixTransform> {matrix: [0.9, 0.1, 0, 0, 0.05, 0.9, 0.05, 0, 0, 0.1, 0.9, 0, 0, 0, 0, 1]}

named_transforms:
  - !<NamedTransform>
    name: nt_view
    transform: !<MatrixTransform> {offset: [0.1, 0, 0, 0]}
"#;

/// Where a config comes from, on both sides.
#[derive(Clone)]
enum Source {
    Yaml(&'static str),
    File(String),
    Builtin(String),
    Raw,
}

impl Source {
    fn port(&self) -> Arc<Config> {
        match self {
            Source::Yaml(text) => Config::create_from_stream(text.as_bytes()).unwrap(),
            Source::File(path) => Config::create_from_file(path).unwrap(),
            Source::Builtin(name) => Config::create_from_builtin_config(name).unwrap(),
            Source::Raw => Config::create_raw().unwrap(),
        }
    }

    fn wheel(&self) -> Value {
        match self {
            Source::Yaml(text) => json!({"yaml": text}),
            Source::File(path) => json!({"file": path}),
            Source::Builtin(name) => json!({"builtin": name}),
            Source::Raw => json!("raw"),
        }
    }
}

fn context_test1() -> String {
    ocio_testkit::paths::upstream_dir()
        .join("tests/data/files/configs/context_test1/config.ocio")
        .to_string_lossy()
        .replace('\\', "/")
}

// ---------------------------------------------------------------------------------------------
// Transforms, on both sides

/// A transform: the oracle's spec, and the port's.
#[derive(Clone)]
struct Tr {
    label: &'static str,
    spec: Value,
    port: Transform,
}

fn f(v: f64) -> Value {
    json!({"f64": v.to_bits()})
}

fn fs(values: &[f64]) -> Value {
    values.iter().map(|&v| f(v)).collect()
}

fn dir_name(dir: TransformDirection) -> &'static str {
    match dir {
        TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
        TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
    }
}

fn matrix(label: &'static str, m: [f64; 16], offset: [f64; 4]) -> Tr {
    let mut t = MatrixTransform::new();
    t.set_matrix(&m);
    t.set_offset(&offset);
    Tr {
        label,
        spec: json!({"class": "MatrixTransform", "calls": [["setMatrix", fs(&m)], ["setOffset", fs(&offset)]]}),
        port: t.into(),
    }
}

fn exponent(label: &'static str, value: [f64; 4], dir: TransformDirection) -> Tr {
    let mut t = ExponentTransform::new();
    t.set_value(&value);
    t.set_direction(dir);
    Tr {
        label,
        spec: json!({"class": "ExponentTransform", "calls": [["setValue", fs(&value)],
            ["setDirection", {"enum": dir_name(dir)}]]}),
        port: t.into(),
    }
}

fn log_tr(label: &'static str, base: f64) -> Tr {
    let mut t = LogTransform::new();
    t.set_base(base);
    Tr {
        label,
        spec: json!({"class": "LogTransform", "calls": [["setBase", f(base)]]}),
        port: t.into(),
    }
}

fn cdl(label: &'static str, slope: [f64; 3], sat: f64) -> Tr {
    let mut t = CdlTransform::new();
    t.set_slope(&slope);
    t.set_sat(sat);
    Tr {
        label,
        spec: json!({"class": "CDLTransform", "calls": [["setSlope", fs(&slope)], ["setSat", f(sat)]]}),
        port: t.into(),
    }
}

fn fixed(label: &'static str, style: FixedFunctionStyle, name: &str) -> Tr {
    let t = FixedFunctionTransform::new(style, &[]).unwrap();
    Tr {
        label,
        spec: json!({"class": "FixedFunctionTransform", "args": {"style": {"enum": name}}}),
        port: t.into(),
    }
}

/// An ExposureContrastTransform: style, exposure, contrast, gamma, pivot, and which of the
/// three are dynamic.
#[allow(clippy::too_many_arguments)]
fn ec(
    label: &'static str,
    style: (ExposureContrastStyle, &str),
    exposure: f64,
    contrast: f64,
    gamma: f64,
    pivot: f64,
    dynamic: [bool; 3],
) -> Tr {
    let mut t = ExposureContrastTransform::new();
    t.set_style(style.0);
    t.set_exposure(exposure);
    t.set_contrast(contrast);
    t.set_gamma(gamma);
    t.set_pivot(pivot);
    let mut calls = vec![
        json!(["setStyle", {"enum": style.1}]),
        json!(["setExposure", f(exposure)]),
        json!(["setContrast", f(contrast)]),
        json!(["setGamma", f(gamma)]),
        json!(["setPivot", f(pivot)]),
    ];
    if dynamic[0] {
        t.make_exposure_dynamic();
        calls.push(json!(["makeExposureDynamic"]));
    }
    if dynamic[1] {
        t.make_contrast_dynamic();
        calls.push(json!(["makeContrastDynamic"]));
    }
    if dynamic[2] {
        t.make_gamma_dynamic();
        calls.push(json!(["makeGammaDynamic"]));
    }
    Tr {
        label,
        spec: json!({"class": "ExposureContrastTransform", "calls": calls}),
        port: t.into(),
    }
}

const LINEAR: (ExposureContrastStyle, &str) =
    (ExposureContrastStyle::Linear, "EXPOSURE_CONTRAST_LINEAR");
const VIDEO: (ExposureContrastStyle, &str) =
    (ExposureContrastStyle::Video, "EXPOSURE_CONTRAST_VIDEO");
const LOGARITHMIC: (ExposureContrastStyle, &str) = (
    ExposureContrastStyle::Logarithmic,
    "EXPOSURE_CONTRAST_LOGARITHMIC",
);

fn diag(d: [f64; 4]) -> [f64; 16] {
    let mut m = [0.; 16];
    for (i, v) in d.into_iter().enumerate() {
        m[i * 5] = v;
    }
    m
}

/// The color corrections (linear, color timing and display CCs).
fn ccs() -> Vec<Tr> {
    use TransformDirection::{Forward, Inverse};
    vec![
        matrix(
            "scale",
            diag([1.5, 0.75, 1.25, 1.]),
            [0.01, -0.02, 0.03, 0.],
        ),
        matrix("identity", diag([1., 1., 1., 1.]), [0.; 4]),
        exponent("gamma", [2., 1.8, 2.2, 1.], Forward),
        exponent("gamma inverse", [2., 1.8, 2.2, 1.], Inverse),
        exponent("unit gamma", [1., 1., 1., 1.], Forward),
        ec("exposure", LINEAR, 0.5, 1.2, 1., 0.18, [true, true, false]),
        ec("exposure static", LINEAR, -1.5, 0.8, 1., 0.18, [false; 3]),
        ec("gamma video", VIDEO, 0., 1., 1.4, 1., [false, false, true]),
        ec("log", LOGARITHMIC, 0.25, 1.1, 1., 0.18, [false; 3]),
        ec("noop static", LINEAR, 0., 1., 1., 0.18, [false; 3]),
        fixed(
            "rgb to hsv",
            FixedFunctionStyle::RgbToHsv,
            "FIXED_FUNCTION_RGB_TO_HSV",
        ),
        cdl("cdl", [1.1, 0.9, 1.05], 1.2),
        log_tr("log2", 2.),
    ]
}

/// The channel views: matrices that read the alpha into a color channel (or don't), and a
/// transform that isn't a matrix.
fn channel_views() -> Vec<Tr> {
    let mut alpha = [0.; 16];
    alpha[3] = 1.;
    alpha[7] = 1.;
    alpha[11] = 1.;
    alpha[15] = 1.;
    let mut green_alpha = [0.; 16];
    green_alpha[7] = 0.5;
    green_alpha[15] = 1.;
    let mut blue_alpha = diag([1., 1., 0., 1.]);
    blue_alpha[11] = 1e-30;
    let mut negative_alpha = diag([1., 1., 1., 1.]);
    negative_alpha[3] = -1.;
    let mut red = [0.; 16];
    red[0] = 1.;
    red[4] = 1.;
    red[8] = 1.;
    red[15] = 1.;
    vec![
        matrix("alpha", alpha, [0.; 4]),
        matrix("green alpha", green_alpha, [0.; 4]),
        matrix("blue alpha", blue_alpha, [0.; 4]),
        matrix("negative alpha", negative_alpha, [0.; 4]),
        matrix("red", red, [0.; 4]),
        matrix(
            "luma",
            [
                0.2126, 0.7152, 0.0722, 0., 0.2126, 0.7152, 0.0722, 0., 0.2126, 0.7152, 0.0722, 0.,
                0., 0., 0., 1.,
            ],
            [0.; 4],
        ),
        fixed(
            "hsv view",
            FixedFunctionStyle::RgbToHsv,
            "FIXED_FUNCTION_RGB_TO_HSV",
        ),
    ]
}

// ---------------------------------------------------------------------------------------------
// Cases

/// A small deterministic generator (xorshift64*), so the cases never change unnoticed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<T: Clone>(&mut self, list: &[T]) -> T {
        list[self.below(list.len())].clone()
    }

    fn maybe(&mut self, n: usize, of: usize) -> Option<usize> {
        // Present one time in `of`, else one of `n`.
        if self.below(of) == 0 {
            None
        } else {
            Some(self.below(n))
        }
    }
}

/// A pipeline's settings.
#[derive(Debug, Clone)]
struct Case {
    /// The display view transform: none, or its source, display, view, direction, looks
    /// bypass and data bypass.
    dvt: Option<(String, String, String, TransformDirection, bool, bool)>,
    linear: Option<usize>,
    timing: Option<usize>,
    channel: Option<usize>,
    display_cc: Option<usize>,
    looks_override_enabled: bool,
    looks_override: Option<String>,
    /// Whether `getProcessor` gets a context (the config's own, or one with a variable
    /// changed), or none.
    context: Option<usize>,
}

/// The names a config's cases take: its color spaces and roles, its displays and views, and
/// its looks, with names it doesn't have.
struct Tokens {
    sources: Vec<String>,
    display_views: Vec<(String, String)>,
    looks: Vec<String>,
}

fn text(v: &[u8]) -> String {
    String::from_utf8(v.to_vec()).expect("UTF-8 names")
}

fn tokens(config: &Config) -> Tokens {
    let mut sources: Vec<String> = (0..config.num_color_spaces())
        .map(|i| text(config.color_space_name_by_index(i)))
        .collect();
    sources.extend((0..config.num_roles()).map(|i| text(config.role_name(i))));
    sources.push("no_such_space".to_string());
    let mut display_views = Vec::new();
    for d in 0..config.num_displays() {
        let display = text(config.display(d));
        for v in 0..config.num_views(&display) {
            display_views.push((display.clone(), text(config.view(&display, v))));
        }
        display_views.push((display.clone(), "no_such_view".to_string()));
    }
    display_views.push(("no_such_display".to_string(), "no_such_view".to_string()));
    let mut looks: Vec<String> = (0..config.num_looks())
        .map(|i| text(config.look_name_by_index(i)))
        .collect();
    let names = looks.clone();
    for (k, a) in names.iter().enumerate() {
        looks.push(format!("-{a}"));
        if let Some(b) = names.get(k + 1) {
            looks.push(format!("+{a}, -{b}"));
            looks.push(format!("no_such_look|{b}"));
        }
    }
    looks.push("no_such_look".to_string());
    looks.push(String::new());
    Tokens {
        sources,
        display_views,
        looks,
    }
}

fn cases(tokens: &Tokens, seed: u64, n: usize) -> Vec<Case> {
    let mut rng = Rng(seed);
    let num_ccs = ccs().len();
    let num_views = channel_views().len();
    (0..n)
        .map(|_| {
            let dvt = (rng.below(40) != 0).then(|| {
                let (display, view) = rng.pick(&tokens.display_views);
                let src = match rng.below(30) {
                    0 => String::new(),
                    _ => rng.pick(&tokens.sources),
                };
                let display = if rng.below(40) == 0 {
                    String::new()
                } else {
                    display
                };
                let dir = if rng.below(3) == 0 {
                    TransformDirection::Inverse
                } else {
                    TransformDirection::Forward
                };
                (
                    src,
                    display,
                    view,
                    dir,
                    rng.below(4) == 0,
                    rng.below(4) != 0,
                )
            });
            let looks_override_enabled = rng.below(3) == 0;
            Case {
                dvt,
                linear: rng.maybe(num_ccs, 2),
                timing: (rng.below(3) == 0).then(|| rng.below(num_ccs)),
                channel: rng.maybe(num_views, 2),
                display_cc: rng.maybe(num_ccs, 3),
                looks_override_enabled,
                looks_override: (rng.below(2) == 0).then(|| rng.pick(&tokens.looks)),
                context: rng.maybe(2, 3),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The port's side

/// The port's log while `f` runs: OCIO's logging function is the process's, so the cases of
/// this file log one at a time.
static LOGGING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn capture<T>(f: impl FnOnce() -> T) -> (T, Vec<Vec<u8>>) {
    let _lock = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let me = std::thread::current().id();
    let messages = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = Arc::clone(&messages);
    set_logging_function(Some(Arc::new(move |m: &[u8]| {
        if std::thread::current().id() == me {
            sink.lock().unwrap().push(m.to_vec());
        } else {
            eprint!("{}", String::from_utf8_lossy(m));
        }
    })))
    .unwrap();
    let out = f();
    reset_to_default_logging_function();
    let log = messages.lock().unwrap().clone();
    (out, log)
}

/// The probe pixels, packed RGBA: zeros, ordinary values, mid grey, the extremes of `f32`,
/// subnormals, the infinities and quiet NaNs with payloads of their own, each in every
/// channel.
fn pixels() -> Vec<f32> {
    let specials: Vec<f32> = vec![
        0.0,
        -0.0,
        0.18,
        1.0,
        -1.0,
        0.5,
        2.0,
        100.0,
        65504.0,
        -0.25,
        f32::from_bits(1),
        -1e-40,
        f32::MAX,
        -f32::MAX,
        f32::MIN_POSITIVE,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0x7fc0_0001),
        f32::from_bits(0xffc0_0002),
        f32::from_bits(0x7fe0_1003),
        1e20,
        3.4,
        0.001,
    ];
    let n = specials.len();
    let mut out = Vec::new();
    for i in 0..n {
        out.extend([
            specials[i],
            specials[(i + 5) % n],
            specials[(i + 11) % n],
            specials[(i + 3) % n],
        ]);
    }
    // Every special as a grey too, alpha 1.
    for &v in &specials {
        out.extend([v, v, v, 1.0]);
    }
    out
}

/// `cpu.applyRGBA(list)`: the binding's list overload applies a packed RGBA image in place.
fn apply_rgba(proc: &Processor, flags: Option<OptimizationFlags>) -> ocio::Result<Vec<f32>> {
    let cpu = match flags {
        None => proc.default_cpu_processor()?,
        Some(flags) => proc.optimized_cpu_processor(flags)?,
    };
    let mut data = pixels();
    let width = data.len() / 4;
    {
        let mut img = PackedImageDesc::new(&mut data[..], width, 1, 4)?;
        cpu.apply(&mut img)?;
    }
    Ok(data)
}

/// The port's pipeline of `case`.
fn port_pipeline(case: &Case) -> LegacyViewingPipeline {
    let ccs = ccs();
    let views = channel_views();
    let mut vp = LegacyViewingPipeline::new();
    if let Some((src, display, view, dir, looks_bypass, data_bypass)) = &case.dvt {
        let mut dt = DisplayViewTransform::new();
        dt.set_src(src);
        dt.set_display(display);
        dt.set_view(view);
        dt.set_direction(*dir);
        dt.set_looks_bypass(*looks_bypass);
        dt.set_data_bypass(*data_bypass);
        vp.set_display_view_transform(Some(&dt));
    }
    vp.set_linear_cc(case.linear.map(|i| &ccs[i].port)).unwrap();
    vp.set_color_timing_cc(case.timing.map(|i| &ccs[i].port))
        .unwrap();
    vp.set_channel_view(case.channel.map(|i| &views[i].port))
        .unwrap();
    vp.set_display_cc(case.display_cc.map(|i| &ccs[i].port))
        .unwrap();
    vp.set_looks_override_enabled(case.looks_override_enabled);
    if let Some(looks) = &case.looks_override {
        vp.set_looks_override(looks);
    }
    vp
}

/// The wheel's calls that make the pipeline `vp` of `case`.
fn wheel_pipeline(case: &Case, vp: &str) -> Vec<Value> {
    let ccs = ccs();
    let views = channel_views();
    let mut calls = vec![json!({"new": "LegacyViewingPipeline", "as": vp})];
    if let Some((src, display, view, dir, looks_bypass, data_bypass)) = &case.dvt {
        let spec = json!({"class": "DisplayViewTransform", "calls": [
            ["setSrc", src], ["setDisplay", display], ["setView", view],
            ["setDirection", {"enum": dir_name(*dir)}],
            ["setLooksBypass", looks_bypass], ["setDataBypass", data_bypass]]});
        calls.push(json!({"call": "setDisplayViewTransform", "on": vp,
            "args": [{"transform": spec}]}));
    }
    for (setter, index, list) in [
        ("setLinearCC", case.linear, &ccs),
        ("setColorTimingCC", case.timing, &ccs),
        ("setChannelView", case.channel, &views),
        ("setDisplayCC", case.display_cc, &ccs),
    ] {
        if let Some(i) = index {
            calls.push(json!({"call": setter, "on": vp, "args": [{"transform": list[i].spec}]}));
        }
    }
    calls.push(json!({"call": "setLooksOverrideEnabled", "on": vp,
        "args": [case.looks_override_enabled]}));
    if let Some(looks) = &case.looks_override {
        calls.push(json!({"call": "setLooksOverride", "on": vp, "args": [looks]}));
    }
    calls
}

// ---------------------------------------------------------------------------------------------
// Comparison

/// What the cases of a run reached.
#[derive(Default)]
struct Reached {
    made: usize,
    refused: usize,
    messages: BTreeSet<String>,
    groups: BTreeSet<Vec<String>>,
    with_context: usize,
    pixels: usize,
}

/// The context variables a request sets on a copy of the config's context, for the cases
/// whose `context` is `Some(1)`.
const CONTEXT_VARS: [(&str, &str); 2] = [("CCCID", "look-01"), ("SHOT", "shot2")];

/// Runs `cases` on the config of `source`, `per_request` cases a request, and compares them;
/// the mismatches go to `failures`.
fn check(
    test: &str,
    source: &Source,
    cases: &[Case],
    per_request: usize,
    failures: &mut Vec<String>,
) -> Reached {
    let mut reached = Reached::default();
    let pixel_values: Vec<Value> = pixels().iter().map(|&v| f(f64::from(v))).collect();
    for (chunk_index, chunk) in cases.chunks(per_request).enumerate() {
        let label = format!("{test} #{chunk_index}");
        // A config per request, as the wheel's (its processor cache starts empty).
        let config = source.port();
        let own_context = config.current_context().get();
        let mut changed_context: Context = (*own_context).clone();
        for (name, value) in CONTEXT_VARS {
            changed_context.set_string_var(name, Some(value.as_bytes()));
        }
        let mut calls = vec![
            json!({"call": "getCurrentContext", "as": "ctx0"}),
            json!({"call": "getCurrentContext", "as": "ctx_own"}),
            json!({"copy": "ctx_own", "as": "ctx1"}),
        ];
        for (name, value) in CONTEXT_VARS {
            calls.push(json!({"call": "setStringVar", "on": "ctx1", "args": [name, value]}));
        }
        // Per case: the index of its repr call, of its getProcessor call, and the port's
        // outcomes.
        let mut plan = Vec::new();
        for (k, case) in chunk.iter().enumerate() {
            let vp = format!("vp{k}");
            let proc = format!("proc{k}");
            calls.extend(wheel_pipeline(case, &vp));
            let repr_at = calls.len();
            calls.push(json!({"call": "__repr__", "on": vp}));
            let mut get = json!({"call": "getProcessor", "on": vp, "args": [{"ref": "config"}],
                "as": proc});
            if let Some(c) = case.context {
                get["kwargs"] = json!({"context": {"ref": format!("ctx{c}")}});
                reached.with_context += 1;
            }
            let proc_at = calls.len();
            calls.push(get);

            let pipeline = port_pipeline(case);
            let context = case.context.map(|c| {
                if c == 0 {
                    &*own_context
                } else {
                    &changed_context
                }
            });
            let (made, port_log) = capture(|| pipeline.processor(&config, context));
            let mut applies = Vec::new();
            if let Ok(made) = &made {
                for (cpu, flags, extra) in [
                    ("getDefaultCPUProcessor", None, json!([])),
                    (
                        "getOptimizedCPUProcessor",
                        Some(OptimizationFlags::NONE),
                        json!([{"enum": "OPTIMIZATION_NONE"}]),
                    ),
                ] {
                    let cpu_name = format!("{proc}_{cpu}");
                    calls.push(json!({"call": cpu, "on": proc, "args": extra, "as": cpu_name}));
                    let apply_at = calls.len();
                    calls.push(json!({"call": "applyRGBA", "on": cpu_name,
                        "args": [pixel_values.clone()]}));
                    applies.push((cpu, apply_at, capture(|| apply_rgba(made, flags))));
                }
            }
            plan.push((
                k,
                case,
                repr_at,
                proc_at,
                pipeline.to_bytes(),
                made,
                port_log,
                applies,
            ));
        }

        let response = Oracle::get().call(
            "object_calls",
            json!({"config": source.wheel(), "calls": calls}),
            &[],
        );
        let result = &response.result;
        assert_eq!(result["config"], Value::Null, "{label}: {result}");
        let results = result["calls"].as_array().expect("calls");
        assert_eq!(results.len(), calls.len(), "{label}");
        for (k, case, repr_at, proc_at, repr, made, port_log, applies) in plan {
            let (ccs, views) = (ccs(), channel_views());
            let name = format!(
                "{label} case {k} {case:?} (linear {:?}, timing {:?}, channel {:?}, display {:?})",
                case.linear.map(|i| ccs[i].label),
                case.timing.map(|i| ccs[i].label),
                case.channel.map(|i| views[i].label),
                case.display_cc.map(|i| ccs[i].label),
            );
            let mut problems = Vec::new();

            // The pipeline's text.
            let wheel_repr = &results[repr_at];
            if wheel_repr
                .get("result")
                .and_then(Value::as_str)
                .map(str::as_bytes)
                != Some(repr.as_slice())
            {
                problems.push(format!(
                    "repr: wheel {wheel_repr}, port {:?}",
                    String::from_utf8_lossy(&repr)
                ));
            }

            // The processor, or the exception.
            let wheel = &results[proc_at];
            match (wheel.get("exception"), &made) {
                (Some(e), Err(port)) => {
                    reached.refused += 1;
                    let message = bytes(&e["message"]);
                    reached
                        .messages
                        .insert(String::from_utf8_lossy(&message).into_owned());
                    if message != port.what() {
                        problems.push(format!(
                            "messages {:?} != {:?}",
                            String::from_utf8_lossy(&message),
                            String::from_utf8_lossy(port.what())
                        ));
                    }
                }
                (None, Ok(port)) => {
                    reached.made += 1;
                    let dump = &wheel["result"]["processor"];
                    if dump.is_null() {
                        problems.push(format!("the wheel gave {wheel}"));
                    } else {
                        let reply = ProcessorOpsReply::from_response(Response {
                            result: json!({"processor": dump}),
                            blobs: response.blobs.clone(),
                        });
                        reached.groups.insert(
                            reply
                                .processor()
                                .classes()
                                .into_iter()
                                .map(String::from)
                                .collect(),
                        );
                        let group = port.create_group_transform().unwrap();
                        if let Err(e) = compare_processor(reply.processor(), port, &group) {
                            problems.push(format!("processor: {e}"));
                        }
                    }
                }
                (wheel_e, port) => problems.push(format!(
                    "wheel {} {wheel}, port {:?}",
                    if wheel_e.is_some() { "raised" } else { "made" },
                    port.as_ref()
                        .map(|_| "a processor")
                        .map_err(|e| e.message())
                )),
            }
            let wheel_log = log(&wheel["log"]);
            if wheel_log != port_log {
                problems.push(format!(
                    "log {:?} != {:?}",
                    wheel_log
                        .iter()
                        .map(|m| String::from_utf8_lossy(m))
                        .collect::<Vec<_>>(),
                    port_log
                        .iter()
                        .map(|m| String::from_utf8_lossy(m))
                        .collect::<Vec<_>>()
                ));
            }

            // The pixels.
            for (cpu, apply_at, (port_pixels, apply_log)) in applies {
                let mut wheel_apply_log = log(&results[apply_at - 1]["log"]);
                wheel_apply_log.extend(log(&results[apply_at]["log"]));
                if wheel_apply_log != apply_log {
                    problems.push(format!("{cpu} log {wheel_apply_log:?} != {apply_log:?}"));
                }
                let wheel_pixels = &results[apply_at];
                let port_pixels = match port_pixels {
                    Ok(p) => p,
                    Err(e) => {
                        problems.push(format!(
                            "{cpu}: the port raised {e:?}, wheel {wheel_pixels}"
                        ));
                        continue;
                    }
                };
                let Some(list) = wheel_pixels["result"].as_array() else {
                    problems.push(format!("{cpu}: the wheel gave {wheel_pixels}"));
                    continue;
                };
                let wheel_bits: Vec<u64> = list
                    .iter()
                    .map(|v| v["f64"].as_u64().expect("an f64"))
                    .collect();
                let port_bits: Vec<u64> = port_pixels
                    .iter()
                    .map(|&v| f64::from(v).to_bits())
                    .collect();
                assert_eq!(port_bits.len(), pixels().len());
                if wheel_bits != port_bits {
                    let diffs: Vec<String> = wheel_bits
                        .iter()
                        .zip(&port_bits)
                        .enumerate()
                        .filter(|(_, (w, p))| w != p)
                        .take(8)
                        .map(|(i, (w, p))| {
                            format!(
                                "[{i}] in {:?}: wheel {:?} ({w:#x}), port {:?} ({p:#x})",
                                pixels()[i],
                                f64::from_bits(*w),
                                f64::from_bits(*p)
                            )
                        })
                        .collect();
                    problems.push(format!("{cpu} pixels: {}", diffs.join("; ")));
                } else {
                    reached.pixels += 1;
                }
            }

            if !problems.is_empty() {
                failures.push(format!("{name}: {}", problems.join("\n    ")));
            }
        }
    }
    reached
}

/// Writes `failures` to a file under the target directory and fails with their number and
/// the first few.
fn report(test: &str, failures: &[String]) {
    if failures.is_empty() {
        return;
    }
    let path = ocio_testkit::paths::target_dir().join(format!("{test}-failures.txt"));
    std::fs::write(&path, failures.join("\n\n")).unwrap();
    panic!(
        "{} mismatches (all in {}):\n{}",
        failures.len(),
        path.display(),
        failures
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}

// ---------------------------------------------------------------------------------------------
// The tests

/// Generated pipelines of upstream's test config, the pipeline config, `context_test1` and the
/// raw config (which has no scene-linear role); each with the processors it makes at least.
#[test]
fn pipelines_match_the_wheel() {
    let mut failures = Vec::new();
    let mut all = Reached::default();
    for (test, source, seed, n, min_made) in [
        (
            "category",
            Source::Yaml(CATEGORY_TEST_CONFIG),
            0x1e9a_0001,
            160,
            40,
        ),
        (
            "pipeline",
            Source::Yaml(PIPELINE_CONFIG),
            0x1e9a_0002,
            320,
            120,
        ),
        (
            "context_test1",
            Source::File(context_test1()),
            0x1e9a_0003,
            80,
            10,
        ),
        ("raw", Source::Raw, 0x1e9a_0004, 80, 2),
    ] {
        let config = source.port();
        let r = check(
            test,
            &source,
            &cases(&tokens(&config), seed, n),
            40,
            &mut failures,
        );
        assert!(
            r.made >= min_made && r.refused >= n / 20,
            "{test}: {} made, {} refused",
            r.made,
            r.refused
        );
        all.made += r.made;
        all.refused += r.refused;
        all.messages.extend(r.messages);
        all.groups.extend(r.groups);
        all.with_context += r.with_context;
        all.pixels += r.pixels;
    }
    report("legacy_viewing_pipeline", &failures);

    // Every error of the pipeline, and many shapes of groups.
    for fragment in [
        "LegacyViewingPipeline: can't create a processor without a display transform.",
        "LegacyViewingPipeline is not valid: DisplayViewTransform: empty source color space name.",
        "LegacyViewingPipeline is not valid: DisplayViewTransform: empty display name.",
        "LegacyViewingPipeline error: Cannot find inputColorSpace, named 'no_such_space'.",
        "DisplayViewTransform error: LinearCC requires 'scene_linear' role to be defined.",
        "DisplayViewTransform error: ColorTimingCC requires 'color_timing' role to be defined.",
        "The specified look, 'no_such_look', cannot be found.",
    ] {
        assert!(
            all.messages.iter().any(|m| m.contains(fragment)),
            "no case raised {fragment:?}: {:#?}",
            all.messages
        );
    }
    assert!(all.groups.len() >= 60, "{} group shapes", all.groups.len());
    assert!(
        all.with_context >= 200,
        "{} cases with a context",
        all.with_context
    );
    assert_eq!(all.pixels, 2 * all.made, "pixels compared");
}

/// Generated pipelines of every built-in config.
#[test]
fn builtin_config_pipelines_match_the_wheel() {
    let mut failures = Vec::new();
    let registry = BuiltinConfigRegistry::get();
    let mut made = 0;
    for i in 0..registry.num_builtin_configs() {
        let name = text(registry.builtin_config_name(i).unwrap());
        let source = Source::Builtin(name.clone());
        let config = source.port();
        let r = check(
            &name,
            &source,
            &cases(&tokens(&config), 0x1e9a_0100 + i as u64, 40),
            40,
            &mut failures,
        );
        assert!(r.made >= 10, "{name}: {} made", r.made);
        assert_eq!(r.pixels, 2 * r.made, "{name}: pixels compared");
        made += r.made;
    }
    report("legacy_viewing_pipeline_builtin", &failures);
    assert!(made >= 40, "{made} processors");
}

/// U-151: the pipeline's transform setters refuse a FixedFunctionTransform whose parameters were
/// made invalid, with the error the wheel's FixedFunctionTransform raises for those parameters
/// (its constructor validates them as the copy the setters make does), where the wheel's setter
/// ends its process (`noexcept`); the pipeline keeps the transform it had.
#[test]
fn setters_refuse_an_invalid_fixed_function() {
    let valid_params = [1.1, 1.2, 1.3, 0.8, 0.9, 0.95, 1.2];
    let response = Oracle::get().call(
        "object_calls",
        json!({"calls": [
            {"new": "FixedFunctionTransform",
             "args": [{"enum": "FIXED_FUNCTION_ACES_GAMUT_COMP_13"}],
             "kwargs": {"params": [f(1.0)]}},
            {"new": "FixedFunctionTransform",
             "args": [{"enum": "FIXED_FUNCTION_ACES_GAMUT_COMP_13"}],
             "kwargs": {"params": fs(&valid_params)}},
        ]}),
        &[],
    );
    let results = response.result["calls"].as_array().expect("calls");
    assert_eq!(results.len(), 2);
    let message = bytes(&results[0]["exception"]["message"]);
    assert!(!message.is_empty(), "{}", results[0]);
    assert!(results[1].get("result").is_some(), "{}", results[1]);

    let valid: Transform =
        FixedFunctionTransform::new(FixedFunctionStyle::AcesGamutComp13, &valid_params)
            .unwrap()
            .into();
    let mut invalid =
        FixedFunctionTransform::new(FixedFunctionStyle::AcesGamutComp13, &valid_params).unwrap();
    invalid.set_params(&[1.0]);
    let invalid: Transform = invalid.into();

    type Setter = fn(&mut LegacyViewingPipeline, Option<&Transform>) -> ocio::Result<()>;
    type Getter = fn(&LegacyViewingPipeline) -> Option<&Transform>;
    let accessors: [(&str, Setter, Getter); 4] = [
        (
            "linear CC",
            LegacyViewingPipeline::set_linear_cc,
            LegacyViewingPipeline::linear_cc,
        ),
        (
            "color timing CC",
            LegacyViewingPipeline::set_color_timing_cc,
            LegacyViewingPipeline::color_timing_cc,
        ),
        (
            "channel view",
            LegacyViewingPipeline::set_channel_view,
            LegacyViewingPipeline::channel_view,
        ),
        (
            "display CC",
            LegacyViewingPipeline::set_display_cc,
            LegacyViewingPipeline::display_cc,
        ),
    ];
    for (name, set, get) in accessors {
        let mut vp = LegacyViewingPipeline::new();
        set(&mut vp, Some(&valid)).unwrap();
        let e = set(&mut vp, Some(&invalid)).expect_err(name);
        assert_eq!(
            String::from_utf8_lossy(e.what()),
            String::from_utf8_lossy(&message),
            "{name}"
        );
        assert_eq!(
            get(&vp).map(Transform::to_bytes),
            Some(valid.to_bytes()),
            "{name}: the pipeline keeps its transform"
        );
    }
}
