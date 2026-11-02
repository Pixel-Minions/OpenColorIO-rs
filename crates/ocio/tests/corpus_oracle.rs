// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Real-world configs against the wheel (PLAN.md section 8, card p10-corpus).
//!
//! Per config, read from its file: the load result or the exact error, what loading logged, its
//! text (`serialize`), its cache ID, `validate` and what it logged, every listing (color spaces,
//! roles, displays, views, looks, view transforms, named transforms, with their text, and the
//! viewing rules' filters); then a processor per color space to and from the scene-linear role,
//! per display and view from that role in both directions, and per named transform in both
//! directions, with fast math on and off: the processor's cache ID, its
//! `createGroupTransform()`, the optimized processor's, the CPU processor's cache ID and pixels
//! bit for bit over a compact probe set, the GPU processor's cache ID and shader (text,
//! uniforms' count, textures bit for bit), what OCIO logged.
//!
//! Which configs:
//! - upstream's test configs (`tests/data/files/configs`), always, so the sweep itself never
//!   goes unexercised;
//! - the public corpora `OCIO_RS_CORPUS` names, below `OCIO_RS_CORPUS_ROOT`
//!   ([`ocio_testkit::corpus`]): a corpus requested and absent or different fails;
//! - the private directories `OCIO_RS_STUDIO_CORPUS` names, only when it is set. Their report
//!   holds anonymous counts and kinds, keyed by a hash of the path: never a name, a path or a
//!   message from the config. Nothing of them reaches the oracle's cache.
//!
//! The reports are written below `<target>/corpus-report/`. Pixel data stays in memory.
//!
//! `OCIO_RS_CORPUS_JOBS` sets how many configs run at a time (default 4),
//! `OCIO_RS_CORPUS_CACHE=1` keeps the public corpora's oracle answers in the oracle's cache,
//! `OCIO_RS_CORPUS_ONLY` keeps the configs whose path holds that text, and
//! `OCIO_RS_CORPUS_REPORT` names the report file.

mod common;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Once, PoisonError};
use std::time::Instant;

use common::transforms::compare_processor;
use ocio::{
    BitDepth, ColorSpaceVisibility, Config, EnvironmentMode, NamedTransformVisibility,
    OptimizationFlags, Processor, ReferenceSpaceType, SearchReferenceSpaceType, TransformDirection,
    ViewType,
};
use ocio_gpu::gpu_shader::{TextureDimensions, TextureType};
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::image_desc::PackedImageDesc;
use ocio_ops::logging::set_logging_function;
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::platform::{MapEnv, set_thread_env_provider};
use ocio_testkit::oracle::{BatchCall, Response, bytes_to_f32, f32_to_bytes};
use ocio_testkit::oracle_values::{bytes, hex, log};
use ocio_testkit::processor_ops::ProcessorDump;
use ocio_testkit::{Oracle, corpus, paths, probe};
use serde_json::{Value, json};

use ocio_testkit::gpu::{GpuShaderReply, Texture, Texture3d};

use TransformDirection::{Forward, Inverse};

thread_local! {
    /// What OCIO logged on this thread: each config runs on one thread.
    static LOG: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };
}

/// Sends what OCIO logs to the logging thread's [`LOG`], once for the process.
fn install_logging() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        set_logging_function(Some(Arc::new(|m: &[u8]| {
            LOG.with(|l| l.borrow_mut().push(m.to_vec()));
        })))
        .expect("a logging function");
    });
}

/// What this thread logged since the last call.
fn take_log() -> Vec<Vec<u8>> {
    LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

fn s(v: &[u8]) -> Value {
    json!({"bytes": hex(v)})
}

fn list(items: impl Iterator<Item = Vec<u8>>) -> Value {
    Value::Array(items.map(|v| s(&v)).collect())
}

fn log_json(messages: &[Vec<u8>]) -> Value {
    Value::Array(messages.iter().map(|m| s(m)).collect())
}

const VIEW_TYPES: [(ViewType, &str); 2] = [
    (ViewType::Shared, "VIEW_SHARED"),
    (ViewType::DisplayDefined, "VIEW_DISPLAY_DEFINED"),
];

/// A getter call on the config: its name, its arguments, and the port's value as the oracle
/// writes it (an object by its text alone).
type Getter = (String, Vec<Value>, Value);

fn g(name: &str, args: Vec<Value>, port: Value) -> Getter {
    (name.to_string(), args, port)
}

const SPACE_TYPES: [(SearchReferenceSpaceType, &str); 3] = [
    (
        SearchReferenceSpaceType::Scene,
        "SEARCH_REFERENCE_SPACE_SCENE",
    ),
    (
        SearchReferenceSpaceType::Display,
        "SEARCH_REFERENCE_SPACE_DISPLAY",
    ),
    (SearchReferenceSpaceType::All, "SEARCH_REFERENCE_SPACE_ALL"),
];

const VISIBILITIES: [(ColorSpaceVisibility, &str); 3] = [
    (ColorSpaceVisibility::Active, "COLORSPACE_ACTIVE"),
    (ColorSpaceVisibility::Inactive, "COLORSPACE_INACTIVE"),
    (ColorSpaceVisibility::All, "COLORSPACE_ALL"),
];

const NT_VISIBILITIES: [(NamedTransformVisibility, &str); 3] = [
    (NamedTransformVisibility::Active, "NAMEDTRANSFORM_ACTIVE"),
    (
        NamedTransformVisibility::Inactive,
        "NAMEDTRANSFORM_INACTIVE",
    ),
    (NamedTransformVisibility::All, "NAMEDTRANSFORM_ALL"),
];

/// The port's result as the oracle writes it, or the exception's message.
fn or_exception<T>(value: ocio::Result<T>, out: impl FnOnce(T) -> Value) -> Value {
    match value {
        Ok(v) => out(v),
        Err(e) => json!({"exception": s(e.what())}),
    }
}

/// The names of the config's color spaces.
fn color_space_names(c: &Config) -> Vec<Vec<u8>> {
    let (kind, vis) = (SearchReferenceSpaceType::All, ColorSpaceVisibility::All);
    (0..c.num_color_spaces_with(kind, vis))
        .map(|i| c.color_space_name_by_index_with(kind, vis, i).to_vec())
        .collect()
}

/// The views of `display` (or the shared views, for `""`), each once.
fn all_views(c: &Config, display: &[u8]) -> Vec<Vec<u8>> {
    let mut views: Vec<Vec<u8>> = Vec::new();
    for (t, _) in VIEW_TYPES {
        for j in 0..c.num_views_of_type(t, display) {
            let view = c.view_of_type(t, display, j).to_vec();
            if !views.contains(&view) {
                views.push(view);
            }
        }
    }
    views
}

/// The getters of the config and the port's values; what serializing and validating logged
/// joins their results, in the order the oracle runs them.
fn getters(c: &Config) -> Vec<Getter> {
    let mut out = vec![
        g("getMajorVersion", vec![], json!(c.major_version())),
        g("getMinorVersion", vec![], json!(c.minor_version())),
        g("getName", vec![], s(c.name())),
        g("getDescription", vec![], s(c.description())),
        g("getFamilySeparator", vec![], s(&[c.family_separator()])),
        g("getSearchPath", vec![], s(&c.search_path())),
        g("getWorkingDir", vec![], s(&c.working_dir())),
        g(
            "getEnvironmentMode",
            vec![],
            json!({"enum": match c.environment_mode() {
                EnvironmentMode::Unknown => "ENV_ENVIRONMENT_UNKNOWN",
                EnvironmentMode::LoadPredefined => "ENV_ENVIRONMENT_LOAD_PREDEFINED",
                EnvironmentMode::LoadAll => "ENV_ENVIRONMENT_LOAD_ALL",
            }}),
        ),
        g(
            "getEnvironmentVarNames",
            vec![],
            list(
                (0..c.num_environment_vars()).map(|i| c.environment_var_name_by_index(i).to_vec()),
            ),
        ),
        g(
            "isStrictParsingEnabled",
            vec![],
            json!(c.is_strict_parsing_enabled()),
        ),
        g(
            "getDefaultLumaCoefs",
            vec![],
            Value::Array(
                c.default_luma_coefs()
                    .iter()
                    .map(|v| json!({"f64": v.to_bits()}))
                    .collect(),
            ),
        ),
        g(
            "getRoleNames",
            vec![],
            list((0..c.num_roles()).map(|i| c.role_name(i).to_vec())),
        ),
        g(
            "getActiveDisplays",
            vec![],
            list(
                (0..c.num_active_displays())
                    .map(|i| c.active_display(i).unwrap_or_default().to_vec()),
            ),
        ),
        g(
            "getActiveViews",
            vec![],
            list((0..c.num_active_views()).map(|i| c.active_view(i).unwrap_or_default().to_vec())),
        ),
        g(
            "getInactiveColorSpaces",
            vec![],
            s(c.inactive_color_spaces()),
        ),
        g(
            "getLookNames",
            vec![],
            list((0..c.num_looks()).map(|i| c.look_name_by_index(i).to_vec())),
        ),
        g(
            "getViewTransformNames",
            vec![],
            list((0..c.num_view_transforms()).map(|i| c.view_transform_name_by_index(i).to_vec())),
        ),
        g(
            "getDefaultViewTransformName",
            vec![],
            s(c.default_view_transform_name()),
        ),
        g("getFileRules", vec![], s(&c.file_rules().get().to_bytes())),
        g(
            "getViewingRules",
            vec![],
            s(&c.viewing_rules().get().to_bytes()),
        ),
        g(
            "getDisplaysAll",
            vec![],
            list((0..c.num_displays_all()).map(|i| c.display_all(i).to_vec())),
        ),
        g("getDefaultDisplay", vec![], s(c.default_display())),
    ];
    for (kind, kind_name) in SPACE_TYPES {
        for (vis, vis_name) in VISIBILITIES {
            out.push(g(
                "getColorSpaceNames",
                vec![json!({"enum": kind_name}), json!({"enum": vis_name})],
                list(
                    (0..c.num_color_spaces_with(kind, vis))
                        .map(|i| c.color_space_name_by_index_with(kind, vis, i).to_vec()),
                ),
            ));
        }
    }
    for (vis, name) in NT_VISIBILITIES {
        out.push(g(
            "getNamedTransformNames",
            vec![json!({"enum": name})],
            list(
                (0..c.num_named_transforms_with(vis))
                    .map(|i| c.named_transform_name_by_index_with(vis, i).to_vec()),
            ),
        ));
    }
    take_log();
    let text = c.serialize();
    let serialize_log = take_log();
    out.push(g(
        "serialize",
        vec![],
        json!({
            "result": or_exception(text, |t| s(&t)),
            "log": log_json(&serialize_log),
        }),
    ));
    let cache_id = c.cache_id();
    take_log();
    out.push(g(
        "getCacheID",
        vec![],
        or_exception(cache_id, |id| s(id.as_bytes())),
    ));
    // The second call gives the result the config kept, and logs nothing (I-147).
    for _ in 0..2 {
        let valid = c.validate();
        let validate_log = take_log();
        out.push(g(
            "validate",
            vec![],
            json!({
                "result": or_exception(valid, |()| Value::Null),
                "log": log_json(&validate_log),
            }),
        ));
    }
    for i in 0..c.num_roles() {
        let role = c.role_name(i);
        out.push(g(
            "getRoleColorSpace",
            vec![s(role)],
            s(c.role_color_space(role)),
        ));
    }
    let names = color_space_names(c);
    for name in &names {
        let args = vec![s(name)];
        out.push(g(
            "getColorSpace",
            args.clone(),
            match c.color_space(name) {
                Some(cs) => s(cs.to_string().as_bytes()),
                None => Value::Null,
            },
        ));
        out.push(g(
            "getCanonicalName",
            args.clone(),
            s(c.canonical_name(name)),
        ));
        out.push(g(
            "isInactiveColorSpace",
            args.clone(),
            json!(c.is_inactive_color_space(name)),
        ));
        out.push(g(
            "isColorSpaceUsed",
            args.clone(),
            json!(c.is_color_space_used(name)),
        ));
        for (t, t_name) in [
            (ReferenceSpaceType::Scene, "REFERENCE_SPACE_SCENE"),
            (ReferenceSpaceType::Display, "REFERENCE_SPACE_DISPLAY"),
        ] {
            let linear = c.is_color_space_linear(name, t);
            take_log();
            out.push(g(
                "isColorSpaceLinear",
                vec![s(name), json!({"enum": t_name})],
                or_exception(linear, |v| json!(v)),
            ));
        }
    }
    for i in 0..c.num_looks() {
        let name = c.look_name_by_index(i).to_vec();
        out.push(g(
            "getLook",
            vec![s(&name)],
            match c.look(&name) {
                Some(look) => s(look.to_string().as_bytes()),
                None => Value::Null,
            },
        ));
    }
    for i in 0..c.num_view_transforms() {
        let name = c.view_transform_name_by_index(i).to_vec();
        out.push(g(
            "getViewTransform",
            vec![s(&name)],
            match c.view_transform(&name) {
                Some(vt) => s(vt.to_string().as_bytes()),
                None => Value::Null,
            },
        ));
    }
    let all = NamedTransformVisibility::All;
    for i in 0..c.num_named_transforms_with(all) {
        let name = c.named_transform_name_by_index_with(all, i).to_vec();
        out.push(g(
            "getNamedTransform",
            vec![s(&name)],
            match c.named_transform(&name) {
                Some(nt) => s(nt.to_string().as_bytes()),
                None => Value::Null,
            },
        ));
    }
    let view_getters = |out: &mut Vec<Getter>, display: &[u8], view: &[u8]| {
        let args = vec![s(display), s(view)];
        out.push(g("hasView", args.clone(), json!(c.has_view(display, view))));
        out.push(g(
            "isViewShared",
            args.clone(),
            json!(c.is_view_shared(display, view)),
        ));
        out.push(g(
            "getDisplayViewTransformName",
            args.clone(),
            s(c.display_view_transform_name(display, view)),
        ));
        out.push(g(
            "getDisplayViewColorSpaceName",
            args.clone(),
            s(c.display_view_color_space_name(display, view)),
        ));
        out.push(g(
            "getDisplayViewLooks",
            args.clone(),
            s(c.display_view_looks(display, view)),
        ));
        out.push(g(
            "getDisplayViewRule",
            args.clone(),
            s(c.display_view_rule(display, view)),
        ));
        out.push(g(
            "getDisplayViewDescription",
            args,
            s(c.display_view_description(display, view)),
        ));
    };
    for i in 0..c.num_displays_all() {
        let display = c.display_all(i).to_vec();
        out.push(g(
            "getDefaultView",
            vec![s(&display)],
            s(c.default_view(&display)),
        ));
        out.push(g(
            "getViews",
            vec![s(&display)],
            list((0..c.num_views(&display)).map(|j| c.view(&display, j).to_vec())),
        ));
        for (t, t_name) in VIEW_TYPES {
            out.push(g(
                "getViews",
                vec![json!({"enum": t_name}), s(&display)],
                list(
                    (0..c.num_views_of_type(t, &display))
                        .map(|j| c.view_of_type(t, &display, j).to_vec()),
                ),
            ));
        }
        for view in all_views(c, &display) {
            view_getters(&mut out, &display, &view);
        }
        for name in &names {
            let args = vec![s(&display), s(name)];
            let views = c.num_views_for_color_space(&display, name).and_then(|n| {
                (0..n)
                    .map(|j| {
                        c.view_for_color_space(&display, name, j)
                            .map(<[u8]>::to_vec)
                    })
                    .collect::<ocio::Result<Vec<_>>>()
            });
            out.push(g(
                "getViews",
                args.clone(),
                or_exception(views, |v| list(v.into_iter())),
            ));
            out.push(g(
                "getDefaultView",
                args,
                or_exception(c.default_view_for_color_space(&display, name), s),
            ));
            take_log();
        }
    }
    for view in all_views(c, b"") {
        view_getters(&mut out, b"", &view);
    }
    for (t, t_name) in VIEW_TYPES {
        out.push(g(
            "getVirtualDisplayViews",
            vec![json!({"enum": t_name})],
            list(
                (0..c.virtual_display_num_views(t)).map(|j| c.virtual_display_view(t, j).to_vec()),
            ),
        ));
    }
    out
}

/// A result as the port writes it: a string the binding couldn't decode as its bytes, an
/// object by its text.
fn normalized(call: &Value) -> Value {
    match (call.get("result"), call.get("undecodable")) {
        (Some(result), _) => match result.get("repr") {
            Some(repr) => match repr.get("undecodable") {
                Some(h) => json!({"bytes": h}),
                None => repr.clone(),
            },
            None => result.clone(),
        },
        (None, Some(h)) => json!({"bytes": h}),
        _ => match call.get("exception") {
            Some(e) => json!({"exception": e["message"]}),
            None => call.clone(),
        },
    }
}

/// The compact probe set: the battery's special values cycled over the channels, a neutral
/// ramp, a coarse color grid, and random colors in three ranges. RGBA, F32.
fn probes() -> Vec<f32> {
    let mut pixels = probe::to_rgba_cycled(&probe::specials());
    for v in probe::ramp(256, -0.125, 1.5) {
        pixels.extend([v, v, v, 1.0]);
    }
    let grid = [-0.01f32, 0.0, 0.002, 0.018, 0.18, 0.5, 0.75, 1.0, 4.0];
    for r in grid {
        for g in grid {
            for b in grid {
                pixels.extend([r, g, b, 0.5]);
            }
        }
    }
    for (seed, lo, hi) in [(1, 0.0, 1.0), (2, -1.0, 16.0), (3, 0.0, 1024.0)] {
        pixels.extend(probe::to_rgba_cycled(&probe::uniform(
            seed,
            4 * 128,
            lo,
            hi,
        )));
    }
    pixels
}

/// What a processor comes from.
#[derive(Debug, Clone)]
enum Source {
    /// `getProcessor(src, dst)`.
    Names(Vec<u8>, Vec<u8>),
    /// `getProcessor(src, display, view, direction)`.
    DisplayView(Vec<u8>, Vec<u8>, Vec<u8>, TransformDirection),
    /// `getProcessor(name, direction)`.
    NamedTransform(Vec<u8>, TransformDirection),
}

fn direction_name(dir: TransformDirection) -> &'static str {
    match dir {
        Forward => "TRANSFORM_DIR_FORWARD",
        Inverse => "TRANSFORM_DIR_INVERSE",
    }
}

impl Source {
    fn overload(&self) -> Value {
        match self {
            Source::Names(src, dst) => json!({"names": [s(src), s(dst)]}),
            Source::DisplayView(src, display, view, dir) => {
                json!({"display_view": [s(src), s(display), s(view), direction_name(*dir)]})
            }
            Source::NamedTransform(name, dir) => {
                json!({"named_transform_name": [s(name), direction_name(*dir)]})
            }
        }
    }

    fn port(&self, config: &Config) -> ocio::Result<Arc<Processor>> {
        match self {
            Source::Names(src, dst) => config.processor_between(src, dst),
            Source::DisplayView(src, display, view, dir) => {
                config.processor_for_display_view(src, display, view, *dir)
            }
            Source::NamedTransform(name, dir) => {
                config.processor_for_named_transform_by_name(name, *dir)
            }
        }
    }

    /// The kind of processor and its direction, without a name.
    fn kind(&self) -> String {
        match self {
            Source::Names(..) => "color space".to_string(),
            Source::DisplayView(.., dir) => format!("display view {}", direction_name(*dir)),
            Source::NamedTransform(_, dir) => format!("named transform {}", direction_name(*dir)),
        }
    }

    fn label(&self) -> String {
        let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
        match self {
            Source::Names(src, dst) => format!("{} -> {}", text(src), text(dst)),
            Source::DisplayView(src, display, view, dir) => format!(
                "{} -> {} / {} {}",
                text(src),
                text(display),
                text(view),
                direction_name(*dir)
            ),
            Source::NamedTransform(name, dir) => {
                format!("{} {}", text(name), direction_name(*dir))
            }
        }
    }
}

/// The default flags without fast math.
const NO_FAST_MATH: OptimizationFlags =
    OptimizationFlags(OptimizationFlags::DEFAULT.0 & !OptimizationFlags::FAST_LOG_EXP_POW.0);

/// A setting of a processor: its name, its flags, its GPU language, in the port's and the
/// oracle's names.
type Setting = (&'static str, OptimizationFlags, GpuLanguage, &'static str);

/// The two settings of a processor: fast math on and off, each with its GPU language.
const SETTINGS: [Setting; 2] = [
    (
        "fast math",
        OptimizationFlags::DEFAULT,
        GpuLanguage::Glsl4_0,
        "GPU_LANGUAGE_GLSL_4_0",
    ),
    (
        "no fast math",
        NO_FAST_MATH,
        GpuLanguage::HlslSm5_0,
        "GPU_LANGUAGE_HLSL_DX11",
    ),
];

/// A difference from the wheel.
#[derive(Debug, Clone)]
struct Mismatch {
    /// What differs: a short name of the comparison.
    kind: String,
    /// The processor (empty for the config's own checks), by name.
    task: String,
    /// The processor's kind, with no name in it.
    task_kind: String,
    /// The settings' name.
    setting: String,
    /// The optimized processor's transform classes, where known.
    classes: Vec<String>,
    /// What the wheel and the port gave.
    detail: String,
    /// What of it holds no text from the config: positions and counts.
    anonymous: String,
}

/// What a config's sweep compared.
#[derive(Debug, Default)]
struct Outcome {
    /// Whether the wheel loaded the config.
    loaded: bool,
    getters: usize,
    processors: usize,
    /// Processors both sides refused, with the same message.
    refused: usize,
    /// CPU comparisons.
    pixels: usize,
    /// Shader comparisons.
    shaders: usize,
    /// How many optimized processors held each transform class.
    classes: BTreeMap<String, usize>,
    /// Why both sides refused processors, by [`refusal_kind`].
    refusals: BTreeMap<&'static str, usize>,
    mismatches: Vec<Mismatch>,
}

impl Outcome {
    fn config_mismatch(&mut self, kind: &str, detail: String) {
        self.mismatches.push(Mismatch {
            kind: kind.to_string(),
            task: String::new(),
            task_kind: String::new(),
            setting: String::new(),
            classes: Vec::new(),
            detail,
            anonymous: String::new(),
        });
    }
}

/// Where two byte strings first differ: positions and lengths, no text.
fn difference_position(wheel: &[u8], port: &[u8]) -> (usize, String) {
    let at = wheel
        .iter()
        .zip(port)
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| wheel.len().min(port.len()));
    let line = 1 + wheel[..at].iter().filter(|&&b| b == b'\n').count();
    let position = format!(
        "at byte {at} (line {line}), wheel {} bytes, port {} bytes",
        wheel.len(),
        port.len()
    );
    (at, position)
}

/// The kind of a refusal both sides gave, by upstream's own words, so that a report says how
/// much of a config couldn't be exercised here without quoting a name from it.
fn refusal_kind(message: &[u8]) -> &'static str {
    let text = String::from_utf8_lossy(message);
    if text.contains("could not be located") {
        if text.contains('$') {
            "file not located, with a context variable"
        } else {
            "file not located"
        }
    } else if text.contains("could not be found") {
        "name not found"
    } else if text.contains("context variable") {
        "context variable"
    } else {
        "other"
    }
}

/// The first place two byte strings differ, with some context.
fn first_difference(wheel: &[u8], port: &[u8]) -> String {
    let (at, position) = difference_position(wheel, port);
    let piece = |b: &[u8]| {
        let start = at.saturating_sub(60);
        String::from_utf8_lossy(&b[start.min(b.len())..(at + 60).min(b.len())]).into_owned()
    };
    format!(
        "{position}\n  wheel {:?}\n  port  {:?}",
        piece(wheel),
        piece(port)
    )
}

fn short(value: &Value) -> String {
    let mut text = value.to_string();
    if text.len() > 300 {
        let mut end = 300;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("...");
    }
    text
}

/// The wheel's text of a value: its bytes, or the JSON.
fn describe(value: &Value) -> String {
    match value.get("bytes").and_then(Value::as_str) {
        Some(_) => String::from_utf8_lossy(&bytes(value)).into_owned(),
        None => short(value),
    }
}

/// The config's load, text, cache ID, validation and listings: the wheel's of the file `path`
/// against the port's of `port_path`, the same file but for the harness's own test. Returns
/// the port's config where both sides loaded it.
fn check_config(
    path: &str,
    port_path: &str,
    out: &mut Outcome,
    cache: bool,
) -> Option<Arc<Config>> {
    take_log();
    let port = Config::create_from_file(port_path);
    let port_log = take_log();
    let port_getters = port.as_ref().ok().map(|c| getters(c)).unwrap_or_default();
    let calls: Vec<Value> = port_getters
        .iter()
        .map(|(name, args, _)| json!({"call": name, "args": args}))
        .collect();
    let args = json!({"config": {"file": path}, "calls": calls});
    let replies = Oracle::get().batch(
        &[BatchCall {
            cmd: "config_calls",
            args,
            blobs: Vec::new(),
        }],
        cache,
    );
    let wheel = replies
        .into_iter()
        .next()
        .expect("a reply")
        .map_err(|e| out.config_mismatch("oracle", e))
        .ok()?
        .result;
    let wheel_log = log(&wheel["config_log"]);
    if wheel_log != port_log {
        out.config_mismatch(
            "load log",
            format!(
                "wheel {:?}\n  port  {:?}",
                wheel_log
                    .iter()
                    .map(|m| String::from_utf8_lossy(m).into_owned())
                    .collect::<Vec<_>>(),
                port_log
                    .iter()
                    .map(|m| String::from_utf8_lossy(m).into_owned())
                    .collect::<Vec<_>>()
            ),
        );
    }
    let loaded = wheel["config"].is_null();
    out.loaded = loaded;
    let config = match (port, loaded) {
        (Ok(config), true) => config,
        (Err(e), false) => {
            let message = match wheel["config"].get("undecodable") {
                Some(h) => bytes(&json!({"bytes": h})),
                None => bytes(&wheel["config"]["exception"]["message"]),
            };
            if message != e.what() {
                out.config_mismatch(
                    "load error",
                    format!(
                        "wheel {:?}\n  port  {:?}",
                        String::from_utf8_lossy(&message),
                        String::from_utf8_lossy(e.what())
                    ),
                );
            }
            return None;
        }
        (Ok(_), false) => {
            out.config_mismatch(
                "load outcome",
                format!(
                    "the wheel refuses it, the port loads it: {}",
                    wheel["config"]
                ),
            );
            return None;
        }
        (Err(e), true) => {
            out.config_mismatch(
                "load outcome",
                format!(
                    "the wheel loads it, the port refuses it: {}",
                    String::from_utf8_lossy(e.what())
                ),
            );
            return None;
        }
    };
    let results = wheel["calls"].as_array().expect("the calls' results");
    assert_eq!(results.len(), port_getters.len());
    for ((name, args, port_value), call) in port_getters.iter().zip(results) {
        let wheel_value = if name == "serialize" || name == "validate" {
            json!({"result": normalized(call), "log": call["log"]})
        } else {
            normalized(call)
        };
        out.getters += 1;
        if &wheel_value == port_value {
            continue;
        }
        let args: Vec<String> = args.iter().map(describe).collect();
        let texts = |a: &Value| match a.pointer("/result/bytes") {
            Some(_) => bytes(&a["result"]),
            None if a.get("bytes").is_some() => bytes(a),
            None => a.to_string().into_bytes(),
        };
        let (wheel_text, port_text) = (texts(&wheel_value), texts(port_value));
        let detail = format!("{args:?}: {}", first_difference(&wheel_text, &port_text));
        out.config_mismatch(name, detail);
        let last = out.mismatches.last_mut().expect("just added");
        last.anonymous = difference_position(&wheel_text, &port_text).1;
    }
    Some(config)
}

/// The tasks of a config.
fn sources(config: &Config) -> Vec<Source> {
    // A config without the role is compared on its default role's color space, or its first.
    let names = color_space_names(config);
    let scene: Vec<u8> = ["scene_linear", "default"]
        .iter()
        .find(|role| config.has_role(role))
        .map(|role| role.as_bytes().to_vec())
        .or_else(|| names.first().cloned())
        .unwrap_or_default();
    let mut out = Vec::new();
    for name in &names {
        out.push(Source::Names(name.clone(), scene.clone()));
        out.push(Source::Names(scene.clone(), name.clone()));
    }
    for i in 0..config.num_displays_all() {
        let display = config.display_all(i).to_vec();
        for view in all_views(config, &display) {
            for dir in [Forward, Inverse] {
                out.push(Source::DisplayView(
                    scene.clone(),
                    display.clone(),
                    view.clone(),
                    dir,
                ));
            }
        }
    }
    let all = NamedTransformVisibility::All;
    for i in 0..config.num_named_transforms_with(all) {
        let name = config.named_transform_name_by_index_with(all, i).to_vec();
        for dir in [Forward, Inverse] {
            out.push(Source::NamedTransform(name.clone(), dir));
        }
    }
    out
}

/// What raised in a part of the reply, as `(stage, message)`.
fn raised(part: &Value) -> Option<(String, Vec<u8>)> {
    let exception = part.get("exception")?;
    let message = match exception.get("undecodable") {
        Some(h) => bytes(&json!({"bytes": h})),
        None => bytes(&exception["message"]),
    };
    Some((
        part["stage"].as_str().unwrap_or_default().to_string(),
        message,
    ))
}

/// The port's texture `index`, as the oracle reports the wheel's.
fn port_texture(desc: &GpuShaderDesc, index: u32) -> Texture {
    let utf8 = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    let t = desc.texture(index).expect("a texture");
    Texture {
        name: utf8(t.texture_name()),
        sampler_name: utf8(t.sampler_name()),
        width: u64::from(t.width()),
        height: u64::from(t.height()),
        channel: match t.channel() {
            TextureType::RedChannel => "TEXTURE_RED_CHANNEL",
            TextureType::RgbChannel => "TEXTURE_RGB_CHANNEL",
        }
        .into(),
        dimensions: match t.dimensions() {
            TextureDimensions::D1 => "TEXTURE_1D",
            TextureDimensions::D2 => "TEXTURE_2D",
        }
        .into(),
        interpolation: interpolation_name(t.interpolation()).into(),
        binding_index: u64::from(desc.texture_shader_binding_index(index).expect("a binding")),
        values: t.values().to_vec(),
    }
}

fn interpolation_name(interpolation: Interpolation) -> &'static str {
    match interpolation {
        Interpolation::Unknown => "INTERP_UNKNOWN",
        Interpolation::Nearest => "INTERP_NEAREST",
        Interpolation::Linear => "INTERP_LINEAR",
        Interpolation::Tetrahedral => "INTERP_TETRAHEDRAL",
        Interpolation::Cubic => "INTERP_CUBIC",
        Interpolation::Default => "INTERP_DEFAULT",
        Interpolation::Best => "INTERP_BEST",
    }
}

/// The port's 3D texture `index`, as the oracle reports the wheel's.
fn port_texture_3d(desc: &GpuShaderDesc, index: u32) -> Texture3d {
    let utf8 = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    let t = desc.texture_3d(index).expect("a 3D texture");
    Texture3d {
        name: utf8(t.texture_name()),
        sampler_name: utf8(t.sampler_name()),
        edge_len: u64::from(t.edge_len()),
        interpolation: interpolation_name(t.interpolation()).into(),
        binding_index: u64::from(
            desc.texture_3d_shader_binding_index(index)
                .expect("a binding"),
        ),
        values: t.values().to_vec(),
    }
}

/// One comparison of a task: mismatches gathered with the task's labels.
struct Comparison<'a> {
    source: &'a Source,
    setting: &'a str,
    classes: Vec<String>,
    found: Vec<Mismatch>,
}

impl Comparison<'_> {
    fn add(&mut self, kind: &str, detail: String) {
        self.found.push(Mismatch {
            kind: kind.to_string(),
            task: self.source.label(),
            task_kind: self.source.kind(),
            setting: self.setting.to_string(),
            classes: self.classes.clone(),
            detail,
            anonymous: String::new(),
        });
    }

    /// [`Comparison::add`], of what holds no text from the config.
    fn add_anonymous(&mut self, kind: &str, detail: String) {
        self.add(kind, detail.clone());
        self.found.last_mut().expect("just added").anonymous = detail;
    }
}

/// The GPU part of a reply against the port's.
fn compare_gpu(
    c: &mut Comparison<'_>,
    wheel: GpuShaderReply,
    processor: &Processor,
    flags: OptimizationFlags,
    language: GpuLanguage,
) -> bool {
    let wheel_raised = raised(&wheel.result);
    let gpu = match processor.optimized_gpu_processor(flags) {
        Ok(gpu) => gpu,
        Err(e) => {
            if wheel_raised.as_ref().map(|(_, m)| m.as_slice()) != Some(e.what()) {
                c.add(
                    "gpu processor",
                    format!("wheel {wheel_raised:?}\n  port  {e}"),
                );
            }
            return false;
        }
    };
    let wheel_id = wheel.result["gpu_cache_id"].as_str().unwrap_or_default();
    let port_id = String::from_utf8_lossy(gpu.get_cache_id()).into_owned();
    if wheel_id != port_id {
        c.add(
            "gpu cache id",
            format!("wheel {wheel_id}\n  port  {port_id}"),
        );
    }
    let flags_of = &wheel.result["gpu_processor"];
    let port_flags = json!({"isNoOp": gpu.is_no_op(),
        "hasChannelCrosstalk": gpu.has_channel_crosstalk()});
    if flags_of != &port_flags {
        c.add(
            "gpu flags",
            format!("wheel {flags_of}\n  port  {port_flags}"),
        );
    }
    let mut desc = GpuShaderDesc::new(language);
    match (gpu.extract_gpu_shader_info(&mut desc), wheel_raised) {
        (Err(e), Some((_, message))) => {
            if message != e.what() {
                c.add(
                    "gpu error",
                    format!(
                        "wheel {:?}\n  port  {:?}",
                        String::from_utf8_lossy(&message),
                        String::from_utf8_lossy(e.what())
                    ),
                );
            }
            false
        }
        (Ok(()), None) => {
            let shader = wheel.shader();
            let text = String::from_utf8_lossy(desc.shader_text()).into_owned();
            if shader.text != text {
                let (wheel_text, port_text) = (shader.text.as_bytes(), text.as_bytes());
                c.add("shader text", first_difference(wheel_text, port_text));
                let position = difference_position(wheel_text, port_text).1;
                c.found.last_mut().expect("just added").anonymous = position;
            }
            let id = String::from_utf8_lossy(&desc.cache_id()).into_owned();
            if shader.cache_id != id {
                c.add(
                    "shader cache id",
                    format!("wheel {}\n  port  {id}", shader.cache_id),
                );
            }
            if shader.uniforms.len() != desc.num_uniforms() as usize
                || shader.dynamic_properties.len() != desc.num_dynamic_properties() as usize
            {
                c.add("shader uniforms", String::new());
            }
            let textures: Vec<Texture> = (0..desc.num_textures())
                .map(|i| port_texture(&desc, i))
                .collect();
            let textures_3d: Vec<Texture3d> = (0..desc.num_textures_3d())
                .map(|i| port_texture_3d(&desc, i))
                .collect();
            if shader.textures != textures {
                let at = shader
                    .textures
                    .iter()
                    .zip(&textures)
                    .position(|(a, b)| a != b);
                c.add(
                    "shader textures",
                    format!(
                        "wheel {} textures, port {}, first different: {at:?}",
                        shader.textures.len(),
                        textures.len()
                    ),
                );
            }
            if shader.textures_3d != textures_3d {
                c.add(
                    "shader 3D textures",
                    format!(
                        "wheel {}, port {}",
                        shader.textures_3d.len(),
                        textures_3d.len()
                    ),
                );
            }
            true
        }
        (port, wheel) => {
            c.add(
                "gpu outcome",
                format!(
                    "wheel {:?}\n  port  {port:?}",
                    wheel.map(|(_, m)| String::from_utf8_lossy(&m).into_owned())
                ),
            );
            false
        }
    }
}

/// A task's reply against the port.
fn compare_task(
    source: &Source,
    (setting, flags, language, _): &Setting,
    response: Response,
    config: &Config,
    pixels: &[f32],
    out: &mut Outcome,
) {
    let Response { result, blobs } = response;
    let mut c = Comparison {
        source,
        setting,
        classes: Vec::new(),
        found: Vec::new(),
    };
    take_log();
    let port = source.port(config);
    if !result["config"].is_null() {
        c.add("config", short(&result["config"]));
    }
    let wheel_processor = &result["processor"];
    let processor = match (port, wheel_processor.get("exception")) {
        (Err(e), Some(_)) => {
            let message = raised(wheel_processor).expect("raised").1;
            if message != e.what() {
                c.add(
                    "processor error",
                    format!(
                        "wheel {:?}\n  port  {:?}",
                        String::from_utf8_lossy(&message),
                        String::from_utf8_lossy(e.what())
                    ),
                );
            }
            out.refused += 1;
            *out.refusals.entry(refusal_kind(&message)).or_default() += 1;
            out.mismatches.append(&mut c.found);
            return;
        }
        (Err(e), None) => {
            c.add(
                "processor outcome",
                format!(
                    "the wheel makes it, the port refuses it: {}",
                    String::from_utf8_lossy(e.what())
                ),
            );
            out.mismatches.append(&mut c.found);
            return;
        }
        (Ok(_), Some(_)) => {
            c.add(
                "processor outcome",
                format!(
                    "the port makes it, the wheel refuses it: {}",
                    short(wheel_processor)
                ),
            );
            out.mismatches.append(&mut c.found);
            return;
        }
        (Ok(processor), None) => processor,
    };
    out.processors += 1;
    match processor.cache_id() {
        Ok(id) => {
            let wheel_id = bytes(&wheel_processor["cache_id"]);
            if wheel_id != id.as_bytes() {
                c.add(
                    "processor cache id",
                    format!("wheel {}\n  port  {id}", String::from_utf8_lossy(&wheel_id)),
                );
            }
        }
        Err(e) => c.add("processor cache id", format!("port error {e}")),
    }

    // The ops, as the wheel's `ops` part stages them.
    let ops = &result["ops"];
    let wheel_stage = raised(ops);
    let port_ops = (|| {
        let group = processor
            .create_group_transform()
            .map_err(|e| ("group", e))?;
        let optimized = processor
            .optimized_processor_with_bit_depths(BitDepth::F32, BitDepth::F32, *flags)
            .map_err(|e| ("optimize", e))?;
        let optimized_group = optimized
            .create_group_transform()
            .map_err(|e| ("optimized_group", e))?;
        Ok::<_, (&str, ocio::Exception)>((group, optimized, optimized_group))
    })();
    match (wheel_stage, port_ops) {
        (Some((stage, message)), Err((port_stage, e))) => {
            if stage != port_stage || message != e.what() {
                c.add(
                    "ops error",
                    format!(
                        "wheel {stage} {:?}\n  port  {port_stage} {:?}",
                        String::from_utf8_lossy(&message),
                        String::from_utf8_lossy(e.what())
                    ),
                );
            }
        }
        (None, Ok((group, optimized, optimized_group))) => {
            let wheel_dump = ProcessorDump::parse(&ops["processor"], &blobs);
            let wheel_optimized = ProcessorDump::parse(&ops["optimized"], &blobs);
            c.classes = wheel_optimized
                .classes()
                .iter()
                .map(ToString::to_string)
                .collect();
            for class in &c.classes {
                *out.classes.entry(class.clone()).or_default() += 1;
            }
            if let Err(e) = compare_processor(&wheel_dump, &processor, &group) {
                c.add("processor ops", e);
            }
            if let Err(e) = compare_processor(&wheel_optimized, &optimized, &optimized_group) {
                c.add("optimized ops", e);
            }
        }
        (wheel, port) => c.add(
            "ops outcome",
            format!(
                "wheel {:?}\n  port  {:?}",
                wheel.map(|(s, m)| (s, String::from_utf8_lossy(&m).into_owned())),
                port.err().map(|(s, e)| format!("{s} {e}"))
            ),
        ),
    }

    // The CPU processor and the pixels.
    let cpu_part = &result["cpu"][0];
    let wheel_cpu = raised(cpu_part);
    let port_cpu = processor
        .optimized_cpu_processor_with_bit_depths(BitDepth::F32, BitDepth::F32, *flags)
        .map_err(|e| ("cpu_processor", e))
        .and_then(|cpu| {
            let npix = pixels.len() / 4;
            let mut dst = vec![0f32; pixels.len()];
            let src = PackedImageDesc::new(pixels, npix, 1, 4).map_err(|e| ("apply", e))?;
            let mut dst_desc =
                PackedImageDesc::new(&mut dst[..], npix, 1, 4).map_err(|e| ("apply", e))?;
            cpu.apply_src_dst(&src, &mut dst_desc)
                .map_err(|e| ("apply", e))?;
            Ok((cpu, dst))
        });
    match (wheel_cpu, port_cpu) {
        (Some((stage, message)), Err((port_stage, e))) => {
            if stage != port_stage || message != e.what() {
                c.add(
                    "cpu error",
                    format!(
                        "wheel {stage} {:?}\n  port  {port_stage} {:?}",
                        String::from_utf8_lossy(&message),
                        String::from_utf8_lossy(e.what())
                    ),
                );
            }
        }
        (None, Ok((cpu, dst))) => {
            out.pixels += 1;
            let wheel_id = cpu_part["cpu_cache_id"].as_str().unwrap_or_default();
            let port_id = String::from_utf8_lossy(cpu.get_cache_id()).into_owned();
            if wheel_id != port_id {
                c.add(
                    "cpu cache id",
                    format!("wheel {wheel_id}\n  port  {port_id}"),
                );
            }
            let index = cpu_part["pixels"].as_u64().expect("a blob") as usize;
            let wheel_pixels = bytes_to_f32(&blobs[index]);
            assert_eq!(wheel_pixels.len(), dst.len(), "the wheel's pixels");
            let different: Vec<usize> = (0..dst.len())
                .filter(|&i| wheel_pixels[i].to_bits() != dst[i].to_bits())
                .collect();
            if let Some(&first) = different.first() {
                c.add_anonymous(
                    "pixels",
                    format!(
                        "{} of {} values differ; first at channel {first}: input {:#010x}, wheel \
                         {:#010x}, port {:#010x}",
                        different.len(),
                        dst.len(),
                        pixels[first].to_bits(),
                        wheel_pixels[first].to_bits(),
                        dst[first].to_bits()
                    ),
                );
            }
        }
        (wheel, port) => c.add(
            "cpu outcome",
            format!(
                "wheel {:?}\n  port  {:?}",
                wheel.map(|(s, m)| (s, String::from_utf8_lossy(&m).into_owned())),
                port.err().map(|(s, e)| format!("{s} {e}"))
            ),
        ),
    }

    // The shader.
    let gpu = GpuShaderReply::from_response(Response {
        result: result["gpu"].clone(),
        blobs,
    });
    if compare_gpu(&mut c, gpu, &processor, *flags, *language) {
        out.shaders += 1;
    }

    let port_log = take_log();
    let wheel_log = log(&result["log"]);
    if wheel_log != port_log {
        c.add(
            "log",
            format!(
                "wheel {:?}\n  port  {:?}",
                wheel_log
                    .iter()
                    .map(|m| String::from_utf8_lossy(m).into_owned())
                    .collect::<Vec<_>>(),
                port_log
                    .iter()
                    .map(|m| String::from_utf8_lossy(m).into_owned())
                    .collect::<Vec<_>>()
            ),
        );
    }
    out.mismatches.append(&mut c.found);
}

/// How many oracle calls one process makes.
const CALLS_PER_PROCESS: usize = 48;

/// The processors of the file `path` in the wheel, each against the port's of `config`.
fn check_processors(path: &str, config: &Config, out: &mut Outcome, cache: bool) {
    let pixels = probes();
    let pixel_bytes = f32_to_bytes(&pixels);
    let sources = sources(config);
    let tasks: Vec<(&Source, &Setting)> = sources
        .iter()
        .flat_map(|source| SETTINGS.iter().map(move |setting| (source, setting)))
        .collect();
    for batch in tasks.chunks(CALLS_PER_PROCESS) {
        let calls: Vec<BatchCall<'_>> = batch
            .iter()
            .map(|(source, (_, flags, _, language))| BatchCall {
                cmd: "config_processor",
                args: json!({
                    "config": {"file": path},
                    // A forward processor changes the wheel's file cache for later inverse
                    // ones (I-163): each request starts from a fresh cache, as the port's
                    // always is.
                    "context": {"calls": [{"call": "ClearAllCaches", "on": "OCIO"}]},
                    "overload": source.overload(),
                    "ops": {"optimization": flags.0},
                    "cpu": [{"optimization": flags.0, "blob": 0}],
                    "gpu": {"optimization": flags.0, "shader": {"language": language}},
                }),
                blobs: vec![&pixel_bytes[..]],
            })
            .collect();
        for ((source, setting), reply) in batch.iter().zip(Oracle::get().batch(&calls, cache)) {
            match reply {
                Ok(response) => {
                    // The oracle makes a config per request: its cache is empty, as a copy's.
                    let copy = (*config).clone();
                    compare_task(source, setting, response, &copy, &pixels, out);
                }
                Err(e) => out.mismatches.push(Mismatch {
                    kind: "oracle".to_string(),
                    task: source.label(),
                    task_kind: source.kind(),
                    setting: setting.0.to_string(),
                    classes: Vec::new(),
                    detail: e,
                    anonymous: String::new(),
                }),
            }
        }
    }
    // Each task compared a processor, or the same refusal; a task left out is reported.
    if out.processors + out.refused != tasks.len() {
        out.config_mismatch(
            "coverage",
            format!(
                "{} tasks, {} processors, {} refused",
                tasks.len(),
                out.processors,
                out.refused
            ),
        );
        let last = out.mismatches.last_mut().expect("just added");
        last.anonymous = last.detail.clone();
    }
}

/// The sweep of one config.
fn check_file(path: &str, cache: bool) -> Outcome {
    set_thread_env_provider(Some(Arc::new(MapEnv::from_entries::<&str, &str>(&[]))));
    let mut out = Outcome::default();
    if let Some(config) = check_config(path, path, &mut out, cache) {
        check_processors(path, &config, &mut out, cache);
    }
    set_thread_env_provider(None);
    out
}

/// A config of a sweep.
struct Input {
    /// How the report names it.
    id: String,
    path: PathBuf,
}

/// Whether a sweep's report may hold names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Privacy {
    Public,
    /// Anonymous counts and kinds only.
    Private,
}

/// A mismatch's line of the report.
fn report_line(id: &str, m: &Mismatch, privacy: Privacy) -> String {
    let line = match privacy {
        Privacy::Public => json!({
            "config": id, "kind": m.kind, "task": m.task, "setting": m.setting,
            "classes": m.classes, "detail": m.detail,
        }),
        Privacy::Private => json!({
            "config": id, "kind": m.kind, "task_kind": m.task_kind, "setting": m.setting,
            "classes": m.classes, "anonymous": m.anonymous,
        }),
    };
    line.to_string()
}

/// What a sweep compared in all.
#[derive(Debug, Default)]
struct Totals {
    configs: usize,
    loaded: usize,
    getters: usize,
    processors: usize,
    refused: usize,
    pixels: usize,
    shaders: usize,
    mismatches: usize,
    classes: BTreeMap<String, usize>,
    refusals: BTreeMap<&'static str, usize>,
    kinds: BTreeMap<String, usize>,
}

impl Totals {
    fn add(&mut self, outcome: &Outcome) {
        self.configs += 1;
        self.loaded += usize::from(outcome.loaded);
        self.getters += outcome.getters;
        self.processors += outcome.processors;
        self.refused += outcome.refused;
        self.pixels += outcome.pixels;
        self.shaders += outcome.shaders;
        self.mismatches += outcome.mismatches.len();
        for (class, count) in &outcome.classes {
            *self.classes.entry(class.clone()).or_default() += count;
        }
        for (kind, count) in &outcome.refusals {
            *self.refusals.entry(kind).or_default() += count;
        }
        for m in &outcome.mismatches {
            *self.kinds.entry(m.kind.clone()).or_default() += 1;
        }
    }

    fn summary(&self, name: &str, seconds: f64) -> String {
        format!(
            "{name}: {} configs ({} loaded by the wheel), {} getters, {} processors compared, {} \
             refused by both, {} pixel sets, {} shaders, {} mismatches, {seconds:.0}s\nkinds: \
             {:?}\nclasses: {:?}\nrefusals: {:?}\n",
            self.configs,
            self.loaded,
            self.getters,
            self.processors,
            self.refused,
            self.pixels,
            self.shaders,
            self.mismatches,
            self.kinds,
            self.classes,
            self.refusals,
        )
    }

    /// Whether every config loaded, and every processor was compared in pixels and shaders.
    fn complete(&self) -> bool {
        self.configs > 0
            && self.loaded == self.configs
            && self.processors > 0
            && self.pixels == self.processors
            && self.shaders == self.processors
    }
}

/// Runs the configs, a few at a time, and writes the report. Returns what it compared and its
/// summary.
fn sweep(name: &str, inputs: &[Input], privacy: Privacy, cache: bool) -> (Totals, String) {
    install_logging();
    let jobs = std::env::var("OCIO_RS_CORPUS_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4usize);
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, Outcome, f64)>> = Mutex::new(Vec::new());
    let started = Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..jobs.min(inputs.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(input) = inputs.get(i) else { break };
                    let t = Instant::now();
                    let path = input.path.to_str().expect("a UTF-8 path");
                    let outcome = catch_unwind(AssertUnwindSafe(|| check_file(path, cache)))
                        .unwrap_or_else(|_| {
                            let mut outcome = Outcome::default();
                            outcome.config_mismatch("panic", String::new());
                            outcome
                        });
                    if privacy == Privacy::Public {
                        eprintln!(
                            "{}: {} mismatches, {:.1}s",
                            input.id,
                            outcome.mismatches.len(),
                            t.elapsed().as_secs_f64()
                        );
                    }
                    results
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push((i, outcome, t.elapsed().as_secs_f64()));
                }
            });
        }
    });
    let mut results = results.into_inner().unwrap_or_else(PoisonError::into_inner);
    results.sort_by_key(|r| r.0);
    let mut lines = String::new();
    let mut totals = Totals::default();
    let mut per_config = String::new();
    for (i, outcome, seconds) in &results {
        let id = &inputs[*i].id;
        for m in &outcome.mismatches {
            lines.push_str(&report_line(id, m, privacy));
            lines.push('\n');
        }
        totals.add(outcome);
        writeln!(
            per_config,
            "{id}: loaded {}, getters {}, processors {}, refused {}, pixels {}, shaders {}, \
             mismatches {}, {seconds:.1}s",
            outcome.loaded,
            outcome.getters,
            outcome.processors,
            outcome.refused,
            outcome.pixels,
            outcome.shaders,
            outcome.mismatches.len()
        )
        .unwrap();
    }
    let summary = totals.summary(name, started.elapsed().as_secs_f64());
    let dir = paths::target_dir().join("corpus-report");
    fs::create_dir_all(&dir).expect("the report's directory");
    let file = std::env::var("OCIO_RS_CORPUS_REPORT").unwrap_or_else(|_| name.to_string());
    fs::write(dir.join(format!("{file}.jsonl")), &lines).expect("the report");
    fs::write(
        dir.join(format!("{file}.txt")),
        format!("{summary}{per_config}"),
    )
    .expect("the report");
    (totals, summary)
}

/// The configs `OCIO_RS_CORPUS_ONLY` keeps: none is an error, not an empty pass.
fn only(inputs: Vec<Input>) -> Vec<Input> {
    let Ok(filter) = std::env::var("OCIO_RS_CORPUS_ONLY") else {
        return inputs;
    };
    let kept: Vec<Input> = inputs
        .into_iter()
        .filter(|i| i.id.contains(&filter))
        .collect();
    assert!(
        !kept.is_empty(),
        "OCIO_RS_CORPUS_ONLY={filter} keeps no config"
    );
    kept
}

/// Every `.ocio` file below `dir`.
fn configs_below(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("a directory") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            configs_below(&path, out);
        } else if path.extension().is_some_and(|e| e == "ocio") {
            out.push(path);
        }
    }
}

/// Upstream's test configs, always: the sweep compares things on every run.
#[test]
fn upstream_test_configs_match_the_wheel() {
    // The workspace root is canonical, which is a verbatim path on Windows. `_wstat` gives such
    // a path the current drive's device (I-114), and the oracle runs in another directory.
    let dir = paths::upstream_dir()
        .join("tests")
        .join("data")
        .join("files")
        .join("configs");
    let dir = PathBuf::from(dir.to_str().expect("UTF-8").trim_start_matches("\\\\?\\"));
    let mut files = Vec::new();
    configs_below(&dir, &mut files);
    files.sort();
    assert!(!files.is_empty(), "no config in {}", dir.display());
    let inputs: Vec<Input> = files
        .into_iter()
        .map(|path| Input {
            id: path
                .strip_prefix(&dir)
                .expect("below the directory")
                .display()
                .to_string(),
            path,
        })
        .collect();
    let (totals, summary) = sweep("upstream", &only(inputs), Privacy::Public, true);
    println!("{summary}");
    assert_eq!(totals.mismatches, 0, "{summary}");
    assert!(totals.complete(), "{summary}");
}

/// The public corpora `OCIO_RS_CORPUS` asks for. One that is absent or differs from the
/// manifest fails here.
#[test]
fn public_corpora_match_the_wheel() {
    let corpora = corpus::requested_from_env().unwrap_or_else(|e| panic!("{e}"));
    let cache = std::env::var("OCIO_RS_CORPUS_CACHE").as_deref() == Ok("1");
    let mut failed = String::new();
    for corpus in &corpora {
        let inputs: Vec<Input> = corpus
            .configs
            .iter()
            .map(|path| Input {
                id: path.clone(),
                path: corpus.root.join(path),
            })
            .collect();
        assert!(!inputs.is_empty(), "{} holds no config", corpus.name);
        let (totals, summary) = sweep(&corpus.name, &only(inputs), Privacy::Public, cache);
        println!("{summary}");
        if totals.mismatches != 0 || !totals.complete() {
            failed.push_str(&summary);
        }
    }
    assert!(failed.is_empty(), "{failed}");
}

/// The private configs `OCIO_RS_STUDIO_CORPUS` names. Everything it reports is anonymous: its
/// report, and its failure message. A panic's message, which may quote a config, isn't printed
/// while this runs.
#[test]
fn studio_configs_match_the_wheel() {
    let files = corpus::studio_from_env().unwrap_or_else(|e| panic!("{e}"));
    if files.is_empty() {
        return;
    }
    let inputs: Vec<Input> = files
        .into_iter()
        .map(|path| Input {
            id: corpus::sha256_hex(path.as_os_str().as_encoded_bytes())[..12].to_string(),
            path,
        })
        .collect();
    std::panic::set_hook(Box::new(|_| {}));
    let (totals, summary) = sweep("studio", &inputs, Privacy::Private, false);
    let _ = std::panic::take_hook();
    println!("{summary}");
    assert_eq!(totals.mismatches, 0, "{summary}");
}

/// A scratch directory of this test binary's, new for `name`.
fn scratch(name: &str) -> PathBuf {
    let dir = paths::target_dir().join("corpus-tests").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a scratch directory");
    // As the workspace root's: never a verbatim path (see above).
    PathBuf::from(dir.to_str().expect("UTF-8").trim_start_matches("\\\\?\\"))
}

/// A config of a scene-linear color space, a color space from it by a LUT file (not monotonic,
/// so that I-163 shows), one by a matrix of `offset`, a display view of each, and a named
/// transform.
fn harness_config(offset: f64) -> String {
    format!(
        "ocio_profile_version: 2.1
environment: {{}}
search_path: .
roles:
  default: lin
  scene_linear: lin
file_rules:
  - !<Rule> {{name: Default, colorspace: lin}}
displays:
  d:
    - !<View> {{name: curve, colorspace: curve}}
    - !<View> {{name: matrix, colorspace: matrix}}
colorspaces:
  - !<ColorSpace>
    name: lin
  - !<ColorSpace>
    name: curve
    to_scene_reference: !<FileTransform> {{src: curve.spi1d, interpolation: linear}}
  - !<ColorSpace>
    name: matrix
    to_scene_reference: !<MatrixTransform> {{offset: [{offset}, 0, 0, 0]}}
named_transforms:
  - !<NamedTransform>
    name: nt
    transform: !<MatrixTransform> {{offset: [{offset}, 0.1, 0, 0]}}
"
    )
}

/// Writes the harness's LUT and `configs` in a new directory; returns the configs' paths.
fn harness_files(name: &str, configs: &[(&str, &str)]) -> Vec<String> {
    let dir = scratch(name);
    let lut = "Version 1\nFrom 0 1\nLength 4\nComponents 1\n{\n0\n1\n0.5\n1\n}\n";
    fs::write(dir.join("curve.spi1d"), lut).expect("the LUT");
    configs
        .iter()
        .map(|(file, text)| {
            let path = dir.join(file);
            fs::write(&path, text).expect("the config");
            path.to_str().expect("UTF-8").to_string()
        })
        .collect()
}

fn kinds(out: &Outcome) -> Vec<&str> {
    let mut kinds: Vec<&str> = out.mismatches.iter().map(|m| m.kind.as_str()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
}

/// The same config on both sides: nothing differs, and everything was compared: 3 color spaces
/// to and from the role, 2 views and a named transform in both directions, each with 2
/// settings. The LUT's inverse comes after its forward processor, so without the fresh cache
/// the requests start from, I-163 shows here.
#[test]
fn the_sweep_finds_nothing_in_the_same_config() {
    install_logging();
    let config = harness_config(0.25);
    let paths = harness_files("same", &[("config.ocio", &config)]);
    let out = check_file(&paths[0], false);
    assert!(out.mismatches.is_empty(), "{:?}", out.mismatches);
    assert!(out.loaded);
    assert!(out.getters > 50, "{}", out.getters);
    assert_eq!(out.processors, (3 * 2 + 2 * 2 + 2) * SETTINGS.len());
    assert_eq!(out.refused, 0);
    assert_eq!(out.pixels, out.processors);
    assert_eq!(out.shaders, out.processors);
    assert!(
        out.classes.contains_key("Lut1DTransform"),
        "{:?}",
        out.classes
    );
}

/// The comparisons see differences: another config on the port's side, of the same names, fails
/// each kind of comparison that depends on it.
#[test]
fn the_sweep_reports_each_kind_of_difference() {
    install_logging();
    let (wheel, port) = (harness_config(0.25), harness_config(0.5));
    let paths = harness_files("different", &[("wheel.ocio", &wheel), ("port.ocio", &port)]);
    set_thread_env_provider(Some(Arc::new(MapEnv::from_entries::<&str, &str>(&[]))));
    let mut out = Outcome::default();
    let config = check_config(&paths[0], &paths[1], &mut out, false).expect("both load");
    let found = kinds(&out);
    for kind in [
        "getCacheID",
        "getColorSpace",
        "getNamedTransform",
        "serialize",
    ] {
        assert!(found.contains(&kind), "{kind}: {found:?}");
    }

    let mut out = Outcome::default();
    check_processors(&paths[0], &config, &mut out, false);
    set_thread_env_provider(None);
    let found = kinds(&out);
    for kind in [
        "cpu cache id",
        "gpu cache id",
        "optimized ops",
        "pixels",
        "processor cache id",
        "processor ops",
        "shader cache id",
        "shader text",
    ] {
        assert!(found.contains(&kind), "{kind}: {found:?}");
    }
    let pixels = out
        .mismatches
        .iter()
        .find(|m| m.kind == "pixels")
        .expect("pixels");
    assert!(
        pixels.anonymous.contains("values differ"),
        "{}",
        pixels.anonymous
    );
}

/// A config one side refuses, and two different refusals.
#[test]
fn the_sweep_reports_a_different_load() {
    install_logging();
    let good = harness_config(0.25);
    let paths = harness_files(
        "load",
        &[
            ("good.ocio", &good),
            ("bad.ocio", "ocio_profile_version: 99\n"),
        ],
    );
    set_thread_env_provider(Some(Arc::new(MapEnv::from_entries::<&str, &str>(&[]))));
    for (wheel, port) in [(0, 1), (1, 0)] {
        let mut out = Outcome::default();
        assert!(check_config(&paths[wheel], &paths[port], &mut out, false).is_none());
        assert_eq!(kinds(&out), ["load outcome"]);
    }
    let missing = format!("{}.missing", paths[1]);
    let mut out = Outcome::default();
    assert!(check_config(&paths[1], &missing, &mut out, false).is_none());
    assert_eq!(kinds(&out), ["load error"]);
    // Both refuse the same way.
    let mut out = Outcome::default();
    assert!(check_config(&paths[1], &paths[1], &mut out, false).is_none());
    assert!(out.mismatches.is_empty(), "{:?}", out.mismatches);
    assert!(!out.loaded);
    set_thread_env_provider(None);
}

/// A refusal's kind comes from upstream's words alone, whatever the message's names say.
#[test]
fn refusals_are_told_apart_by_upstreams_words() {
    for (message, kind) in [
        (
            "The specified file reference 'a.lut' could not be located.",
            "file not located",
        ),
        (
            "The specified file reference '$X/a.lut' could not be located.",
            "file not located, with a context variable",
        ),
        ("Color space 'a' could not be found.", "name not found"),
        ("A context variable is missing.", "context variable"),
        ("could not parse a", "other"),
    ] {
        assert_eq!(refusal_kind(message.as_bytes()), kind, "{message}");
    }
}

/// A private sweep's report holds no text from the config.
#[test]
fn a_private_report_holds_no_names() {
    let m = Mismatch {
        kind: "pixels".to_string(),
        task: "secret".to_string(),
        task_kind: "color space".to_string(),
        setting: "fast math".to_string(),
        classes: vec!["Lut1DTransform".to_string()],
        detail: "secret".to_string(),
        anonymous: "1 of 2".to_string(),
    };
    assert!(!report_line("id", &m, Privacy::Private).contains("secret"));
    assert!(report_line("id", &m, Privacy::Public).contains("secret"));
}

/// The fast-math settings differ in that flag alone, and the probes hold what they should. The
/// digest pins the probes, which this file generates.
#[test]
fn the_settings_and_the_probes_are_what_they_say() {
    assert_eq!(SETTINGS[0].1.0, OptimizationFlags::DEFAULT.0);
    assert_eq!(
        SETTINGS[0].1.0 ^ SETTINGS[1].1.0,
        OptimizationFlags::FAST_LOG_EXP_POW.0
    );
    let pixels = probes();
    assert_eq!(pixels.len() % 4, 0);
    assert!(pixels.iter().any(|v| v.is_nan()));
    assert!(pixels.iter().any(|v| v.is_infinite()));
    assert_eq!(
        corpus::sha256_hex(&f32_to_bytes(&pixels)),
        "128dfba97f231364b98406a9ab0f95bc9f96558cdd153877d753b366576320d2"
    );
}
