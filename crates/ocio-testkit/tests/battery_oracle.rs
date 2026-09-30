// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The battery's machinery against the real oracle: a port that reproduces the wheel passes,
//! and wrong ports, refusals and pass-through channels that change are reported per case.
//!
//! The "port" of these tests is the wheel itself, called one buffer at a time outside the
//! battery's batch (`oracle_port`), or a deliberately wrong renderer. No port code is
//! involved: the op families' own battery runs live next to their renderers.

use std::panic::{AssertUnwindSafe, catch_unwind};

use ocio_testkit::Oracle;
use ocio_testkit::battery::params::{A, Case, Channels, Params, Precision, R, RGB, Slot};
use ocio_testkit::battery::{
    Combo, Direction, Family, Mutations, Plan, Port, Spec, Summary, Sweep, Validation, run_with,
    yaml_number,
};
use ocio_testkit::oracle::f32_to_bytes;
use ocio_testkit::probe::{ProbeSet, specials};
use serde_json::{Value, json};

/// A LogTransform's base.
#[derive(Debug, Clone, PartialEq)]
struct Base(f64);

impl Params for Base {
    fn slots(&self) -> Vec<Slot> {
        vec![Slot::new("base", Precision::F64AsF32, RGB)]
    }
    fn get(&self, _: usize) -> f64 {
        self.0
    }
    fn set(&mut self, _: usize, value: f64) {
        self.0 = value;
    }
}

fn spec(base: &Base, direction: Direction) -> Spec {
    if base.0.is_finite() {
        Spec::Transform(json!({
            "class": "LogTransform",
            "args": {"base": base.0, "direction": direction.oracle_enum()},
        }))
    } else {
        Spec::Yaml(format!(
            "!<LogTransform> {{base: {}, direction: {}}}",
            yaml_number(base.0),
            direction.yaml()
        ))
    }
}

/// The wheel's output for one buffer, from a single `cpu_apply` call outside any batch.
fn wheel(args: &Value, pixels: &[f32]) -> Result<Vec<f32>, String> {
    let resp = Oracle::get().call("cpu_apply", args.clone(), &[&f32_to_bytes(pixels)]);
    match resp.result.get("exception") {
        Some(e) => Err(e["message"].as_str().unwrap_or_default().to_string()),
        None => Ok(resp.blob_f32(0)),
    }
}

/// A port that is the wheel itself.
fn oracle_port(args: Value) -> Port {
    Port::in_place(move |px| {
        let out = wheel(&args, px).expect("the wheel accepts it");
        px.copy_from_slice(&out);
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// The wheel itself, outside the batch; refusals give the wheel's own text.
    Oracle,
    /// Leaves the pixels unchanged.
    Identity,
    /// The wheel's output, then refusals with a different text.
    WrongRefusalText,
}

struct LogFamily {
    cases: Vec<Case<Base>>,
    bases: Vec<Case<Base>>,
    port: Kind,
    /// Other profiles: the wheel itself, and one that quiets signalling NaNs in alpha.
    other_profiles: bool,
    pass_through: Channels,
    validation: Validation,
}

impl LogFamily {
    fn new(cases: Vec<Case<Base>>) -> Self {
        LogFamily {
            cases,
            bases: Vec::new(),
            port: Kind::Oracle,
            other_profiles: false,
            pass_through: [false; 4],
            validation: Validation::Ported,
        }
    }
}

impl Family for LogFamily {
    type Params = Base;

    fn name(&self) -> String {
        "LogTransform (battery self-test)".to_string()
    }
    fn cases(&self) -> Vec<Case<Base>> {
        self.cases.clone()
    }
    fn mutation_bases(&self) -> Vec<Case<Base>> {
        self.bases.clone()
    }
    fn directions(&self) -> Vec<Direction> {
        vec![Direction::Forward]
    }
    fn spec(&self, params: &Base, direction: Direction) -> Spec {
        spec(params, direction)
    }
    fn port(&self, params: &Base, combo: &Combo) -> Result<Port, String> {
        let args = spec(params, combo.direction).cpu_apply_args(combo);
        match self.port {
            Kind::Identity => Ok(Port::in_place(|_| {})),
            Kind::Oracle | Kind::WrongRefusalText => {
                // The wheel's refusal comes from any buffer.
                match wheel(&args, &[0.5; 4]) {
                    Ok(_) => Ok(oracle_port(args)),
                    Err(text) if self.port == Kind::Oracle => Err(text),
                    Err(text) => Err(format!("{text} (reworded)")),
                }
            }
        }
    }
    fn other_profiles(&self, params: &Base, combo: &Combo) -> Vec<(String, Port)> {
        if !self.other_profiles {
            return Vec::new();
        }
        let args = spec(params, combo.direction).cpu_apply_args(combo);
        let quiet_args = args.clone();
        vec![
            ("the wheel".to_string(), oracle_port(args)),
            (
                "alpha arithmetic".to_string(),
                Port::in_place(move |px| {
                    let out = wheel(&quiet_args, px).expect("the wheel accepts it");
                    px.copy_from_slice(&out);
                    for alpha in px.iter_mut().skip(3).step_by(4) {
                        // Arithmetic on a pass-through channel quiets signalling NaNs.
                        *alpha = std::hint::black_box(*alpha) * std::hint::black_box(1.0f32);
                    }
                }),
            ),
        ]
    }
    fn pass_through(&self, _: &Base, _: &Combo) -> Channels {
        self.pass_through
    }
    fn validation(&self) -> Validation {
        self.validation
    }
}

/// The specials only, no generated cases: a few oracle calls.
fn small_plan() -> Plan {
    Plan {
        name: "self-test".to_string(),
        probes: vec![ProbeSet::Specials],
        generated_probes: vec![ProbeSet::Specials],
        breakpoint_ulps: 0,
        mutations: Mutations::None,
        sweep: None,
        batch_bytes: 256 << 20,
        cache: true,
    }
}

fn run_expecting_failure(family: &LogFamily, plan: &Plan) -> String {
    let result = catch_unwind(AssertUnwindSafe(|| run_with(family, plan)));
    let payload = result.expect_err("the battery should fail");
    payload
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_else(|| "(no message)".to_string())
}

#[test]
fn a_port_that_reproduces_the_wheel_passes() {
    let family = LogFamily::new(vec![
        Case::new("base 2", Base(2.0)),
        Case::new("base NaN", Base(f64::NAN)),
    ]);
    let summary: Summary = run_with(&family, &small_plan());
    assert_eq!(summary.failures, Vec::<String>::new());
    assert_eq!(summary.groups, 4);
    assert_eq!(summary.comparisons, 4);
    assert_eq!(summary.values, 4 * 4 * specials().len());
    // The NaN base's colour channels compare under W0002.
    assert_eq!(summary.w0002_comparisons, 2);
    assert_eq!(summary.oracle_batches, 1);
}

#[test]
fn a_wrong_port_fails_with_a_report_per_case() {
    let mut family = LogFamily::new(vec![Case::new("base 2", Base(2.0))]);
    family.port = Kind::Identity;
    let message = run_expecting_failure(&family, &small_plan());
    assert!(message.contains("2 failures"), "{message}");
    for combo in ["forward, fast math on", "forward, fast math off"] {
        let head = format!("case \"base 2\" ({combo}), probe \"specials\"");
        assert!(message.contains(&head), "{head} missing from:\n{message}");
    }
    assert!(message.contains("values differ bitwise"), "{message}");
}

#[test]
fn refusals_are_compared_or_left_out() {
    // Ported: identical texts pass, different texts fail.
    let family = LogFamily::new(vec![Case::new("base 1", Base(1.0))]);
    let summary = run_with(&family, &small_plan());
    assert_eq!(summary.refusals_compared, 2);
    assert_eq!(summary.comparisons, 0);

    let mut family = LogFamily::new(vec![Case::new("base 1", Base(1.0))]);
    family.port = Kind::WrongRefusalText;
    let message = run_expecting_failure(&family, &small_plan());
    assert!(
        message.contains("refuse it with different texts"),
        "{message}"
    );

    // Not ported: a generated case the wheel refuses is left out, an explicit one fails.
    let mut family = LogFamily::new(vec![Case::new("base 2", Base(2.0))]);
    family.bases = vec![Case::new("base 2", Base(2.0))];
    family.validation = Validation::NotPorted { card: "WP 1.3l1" };
    let plan = Plan {
        mutations: Mutations::All,
        ..small_plan()
    };
    let summary = run_with(&family, &plan);
    let left_out: Vec<&str> = summary.left_out.iter().map(|(l, _)| l.as_str()).collect();
    for label in ["base 2, base = -1e300", "base 2, base = -inf"] {
        assert!(left_out.contains(&label), "{label} not in {left_out:?}");
    }
    assert!(!left_out.contains(&"base 2, base = NaN"));
    assert_eq!(summary.validation_card, Some("WP 1.3l1"));

    let mut family = LogFamily::new(vec![Case::new("base 1", Base(1.0))]);
    family.validation = Validation::NotPorted { card: "WP 1.3l1" };
    let message = run_expecting_failure(&family, &small_plan());
    assert!(
        message.contains("the wheel refuses this explicit case"),
        "{message}"
    );
}

#[test]
fn pass_through_channels_are_checked_on_every_profile() {
    // The Log op passes alpha through: the wheel and a correct other profile agree, the
    // profile that does arithmetic on alpha quiets the specials' signalling NaNs.
    let mut family = LogFamily::new(vec![Case::new("base 2", Base(2.0))]);
    family.pass_through = A;
    family.other_profiles = true;
    let message = run_expecting_failure(&family, &small_plan());
    assert!(message.contains("2 failures"), "{message}");
    assert!(
        message.contains("profile alpha arithmetic: pass-through channels"),
        "{message}"
    );
    assert!(!message.contains("profile the wheel:"), "{message}");

    // A channel the wheel changes can't be declared pass-through.
    let mut family = LogFamily::new(vec![Case::new("base 2", Base(2.0))]);
    family.pass_through = R;
    let message = run_expecting_failure(&family, &small_plan());
    assert!(message.contains("the wheel changes channels"), "{message}");
}

/// A YAML spec and a JSON spec with the same parameters give identical pixels, for ordinary,
/// huge, tiny and subnormal values. (The Log renderers narrow parameters to `float`, so this
/// can't see differences below `float` precision. Reading the values back with
/// PyOpenColorIO's getters showed identical doubles on both platforms.)
#[test]
fn yaml_specs_read_numbers_back_exactly() {
    let values = [
        2.2,
        0.293255132,
        -0.0,
        1e-6,
        1e39,
        -1e300,
        1.7e308,
        1e-46,
        1e-310,
        f64::from_bits(1),
    ];
    let combo = Combo {
        direction: Direction::Inverse,
        fast_math: false,
        format: ocio_testkit::battery::Format::F32_RGBA,
    };
    let input = f32_to_bytes(&ocio_testkit::probe::to_rgba_cycled(&specials()));
    let mut calls = Vec::new();
    for v in values {
        let json = Spec::Transform(json!({
            "class": "LogAffineTransform",
            "args": {"linSideOffset": [v, 0.5, v], "direction": combo.direction.oracle_enum()},
        }));
        let yaml = Spec::Yaml(format!(
            "!<LogAffineTransform> {{lin_side_offset: [{}, 0.5, {}], direction: inverse}}",
            yaml_number(v),
            yaml_number(v)
        ));
        for spec in [json, yaml] {
            calls.push(ocio_testkit::oracle::BatchCall {
                cmd: "cpu_apply",
                args: spec.cpu_apply_args(&combo),
                blobs: vec![input.as_slice()],
            });
        }
    }
    let responses = Oracle::get().batch(&calls, true);
    for (v, pair) in values.iter().zip(responses.chunks(2)) {
        assert!(
            pair[0].result.get("exception").is_none(),
            "{}",
            pair[0].result
        );
        assert_eq!(
            pair[0].blobs, pair[1].blobs,
            "{v:e}: JSON and YAML specs give different pixels"
        );
    }
}

/// The sweep of every `f32` streams its chunks through the batch like any probe: here its
/// first two chunks, in both combinations, for the first case only.
#[test]
fn the_f32_sweep_runs_its_chunks() {
    let family = LogFamily::new(vec![
        Case::new("base 2", Base(2.0)),
        Case::new("base 10", Base(10.0)),
    ]);
    let plan = Plan {
        sweep: Some(Sweep {
            cases: 1,
            chunk_pixels: 1 << 12,
            chunks: Some(2),
        }),
        ..small_plan()
    };
    let summary = run_with(&family, &plan);
    assert_eq!(summary.failures, Vec::<String>::new());
    // 2 cases x 2 combinations of specials, then 2 combinations x 2 chunks for the first.
    assert_eq!(summary.comparisons, 4 + 4);
    assert_eq!(summary.values, 4 * 4 * specials().len() + 4 * 4 * (1 << 12));
    assert_eq!(summary.oracle_batches, 1);
}
