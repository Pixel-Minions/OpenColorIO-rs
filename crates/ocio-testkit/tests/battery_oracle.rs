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
    wheel_port(args, |_| {})
}

/// The wheel's output, then `after`.
fn wheel_port(args: Value, after: fn(&mut [f32])) -> Port {
    Port::in_place(move |px| {
        let out = wheel(&args, px).expect("the wheel accepts it");
        px.copy_from_slice(&out);
        after(px);
    })
}

/// Arithmetic on alpha, which quiets signalling NaNs.
fn quiet_alpha(px: &mut [f32]) {
    for alpha in px.iter_mut().skip(3).step_by(4) {
        *alpha = std::hint::black_box(*alpha) * std::hint::black_box(1.0f32);
    }
}

/// Flips the sign of every NaN in the channels `channels` selects.
fn flip_nan_signs(px: &mut [f32], channels: [bool; 4]) {
    for (i, v) in px.iter_mut().enumerate() {
        if channels[i % 4] && v.is_nan() {
            *v = -*v;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// The wheel itself, outside the batch; refusals give the wheel's own text.
    Oracle,
    /// Leaves the pixels unchanged.
    Identity,
    /// The wheel's output, then refusals with a different text.
    WrongRefusalText,
    /// The wheel's output, then arithmetic on alpha.
    QuietAlpha,
    /// The wheel's output with the signs of its NaNs flipped in the colour channels.
    FlipColourNans,
    /// The wheel's output with the signs of its NaNs flipped in alpha.
    FlipAlphaNans,
}

/// Deliberate bugs in the family's spec.
#[derive(Debug, Clone, Copy, PartialEq)]
enum SpecBug {
    None,
    /// Non-finite parameters written as Rust prints them (`inf`, `NaN`): not YAML numbers.
    RustNumbers,
    /// The case with this base names a transform class that doesn't exist.
    BadClassFor(f64),
    /// Non-finite parameters' YAML has a key LogTransform doesn't know (`bse`).
    UnknownKey,
}

struct LogFamily {
    cases: Vec<Case<Base>>,
    bases: Vec<Case<Base>>,
    port: Kind,
    spec_bug: SpecBug,
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
            spec_bug: SpecBug::None,
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
        match self.spec_bug {
            SpecBug::RustNumbers if !params.0.is_finite() => Spec::Yaml(format!(
                "!<LogTransform> {{base: {}, direction: {}}}",
                params.0,
                direction.yaml()
            )),
            SpecBug::UnknownKey if !params.0.is_finite() => Spec::Yaml(format!(
                "!<LogTransform> {{base: {}, bse: 2, direction: {}}}",
                yaml_number(params.0),
                direction.yaml()
            )),
            SpecBug::BadClassFor(base) if params.0 == base => {
                Spec::Transform(json!({"class": "NoSuchTransform", "args": {}}))
            }
            _ => spec(params, direction),
        }
    }
    fn port(&self, params: &Base, combo: &Combo) -> Result<Port, String> {
        let args = spec(params, combo.direction).cpu_apply_args(combo);
        match self.port {
            Kind::Identity => Ok(Port::in_place(|_| {})),
            Kind::QuietAlpha => Ok(wheel_port(args, quiet_alpha)),
            Kind::FlipColourNans => Ok(wheel_port(args, |px| flip_nan_signs(px, RGB))),
            Kind::FlipAlphaNans => Ok(wheel_port(args, |px| flip_nan_signs(px, A))),
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
        vec![
            ("the wheel".to_string(), oracle_port(args.clone())),
            // Arithmetic on a pass-through channel quiets signalling NaNs.
            (
                "alpha arithmetic".to_string(),
                wheel_port(args, quiet_alpha),
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
    // The NaN base's colour channels compare under W0002. The summary lists both combinations
    // with what the waiver covered, nothing here, so that a count moving is visible.
    assert_eq!(summary.w0002_comparisons, 2);
    let listed: Vec<(&str, &str, usize)> = summary
        .w0002_waived
        .iter()
        .map(|(case, combo, n)| (case.as_str(), combo.as_str(), *n))
        .collect();
    assert_eq!(
        listed,
        [
            ("base NaN", "forward, fast math on", 0),
            ("base NaN", "forward, fast math off", 0)
        ]
    );
    assert!(
        summary
            .to_string()
            .contains("base NaN: forward, fast math on 0; forward, fast math off 0"),
        "{summary}"
    );
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

    // Not ported: a generated case the wheel refuses is left out, an explicit one fails. (The
    // NaN base is an explicit case on the YAML spec the generated NaN and ±Inf cases take.)
    let mut family = LogFamily::new(vec![
        Case::new("base 2", Base(2.0)),
        Case::new("base NaN", Base(f64::NAN)),
    ]);
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
    // The summary counts the refusal texts: every left-out case in both combinations.
    let refused: usize = summary.refusals.iter().map(|(_, n)| n).sum();
    assert_eq!(refused, 2 * summary.left_out.len());
    for (label, text) in &summary.left_out {
        assert!(
            summary.refusals.iter().any(|(t, _)| t == text),
            "{label}: {text} is not among {:?}",
            summary.refusals
        );
    }
    assert!(summary.to_string().contains("refused by the wheel"));

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

/// A YAML spec loads, and reads its numbers back as a JSON spec does:
/// - ordinary, huge, tiny and subnormal values give the JSON spec's pixels;
/// - `.inf` and `-.inf` give the pixels of JSON's ±1e300, which the Log renderers also narrow
///   to ±Inf;
/// - `.nan` makes its channels NaN for every pixel and leaves the others as a finite spec does.
///
/// A number yaml-cpp can't parse (`inf` instead of `.inf`) fails while loading the config. (The
/// Log renderers narrow parameters to `float`, so pixels can't show differences below `float`
/// precision; reading the values back with PyOpenColorIO's getters showed identical doubles on
/// both platforms.)
#[test]
fn yaml_specs_read_numbers_back_exactly() {
    // (the YAML spec's value, the JSON spec's value)
    let mut values: Vec<(f64, f64)> = [
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
    ]
    .map(|v| (v, v))
    .to_vec();
    values.extend([
        (f64::INFINITY, 1e300),
        (f64::NEG_INFINITY, -1e300),
        (f64::NAN, 0.0),
    ]);
    let combo = Combo {
        direction: Direction::Inverse,
        fast_math: false,
        format: ocio_testkit::battery::Format::F32_RGBA,
    };
    let input = f32_to_bytes(&ocio_testkit::probe::to_rgba_cycled(&specials()));
    let mut calls = Vec::new();
    for &(yaml_value, json_value) in &values {
        let json = Spec::Transform(json!({
            "class": "LogAffineTransform",
            "args": {
                "linSideOffset": [json_value, 0.5, json_value],
                "direction": combo.direction.oracle_enum(),
            },
        }));
        let yaml = Spec::Yaml(format!(
            "!<LogAffineTransform> {{lin_side_offset: [{}, 0.5, {}], direction: inverse}}",
            yaml_number(yaml_value),
            yaml_number(yaml_value)
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
    for (&(yaml_value, _), pair) in values.iter().zip(responses.chunks(2)) {
        let label = yaml_number(yaml_value);
        let [json, yaml] = [&pair[0], &pair[1]].map(|r| match r {
            Ok(response) if response.result.get("exception").is_none() => response,
            Ok(response) => panic!("{label}: the wheel refused it: {}", response.result),
            Err(e) => panic!("{label}: {e}"),
        });
        if !yaml_value.is_nan() {
            assert_eq!(
                json.blobs, yaml.blobs,
                "{label}: JSON and YAML specs give different pixels"
            );
            continue;
        }
        let (finite, nan) = (json.blob_f32(0), yaml.blob_f32(0));
        assert_eq!(finite.len(), nan.len());
        for (i, (f, n)) in finite.iter().zip(&nan).enumerate() {
            if i % 4 == 0 || i % 4 == 2 {
                assert!(n.is_nan(), "{label}: value {i} is {n:e}, not NaN");
            } else {
                assert_eq!(f.to_bits(), n.to_bits(), "{label}: value {i}");
            }
        }
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

/// A spec the wheel can't load is a bug, not a refusal. The verifier's B6 wrote `inf` for
/// `.inf`, and the generated ±Inf cases vanished from the comparisons as "left out"; each of
/// them now fails.
#[test]
fn a_spec_the_wheel_cant_load_fails_instead_of_being_left_out() {
    let mut family = LogFamily::new(vec![Case::new("base 2", Base(2.0))]);
    family.bases = vec![Case::new("base 2", Base(2.0))];
    family.validation = Validation::NotPorted { card: "WP 1.3l1" };
    family.spec_bug = SpecBug::RustNumbers;
    let plan = Plan {
        mutations: Mutations::All,
        ..small_plan()
    };
    let message = run_expecting_failure(&family, &plan);
    // NaN, +Inf and -Inf, each in both combinations, and no explicit case on the YAML route.
    assert!(message.contains("7 failures"), "{message}");
    assert!(
        message.contains("generated cases use YAML specs, but no explicit case"),
        "{message}"
    );
    assert!(
        message.contains("OCIO raised while loading the config"),
        "{message}"
    );
    for label in ["base = NaN", "base = inf", "base = -inf"] {
        assert!(
            message.contains(&format!("case \"base 2, {label}\"")),
            "{label}: {message}"
        );
    }
}

/// A call the oracle can't run (a misspelled transform class) fails its own case, labelled
/// with its combination and probe; the other cases are still compared.
#[test]
fn a_failing_oracle_call_fails_its_case_only() {
    let mut family = LogFamily::new(vec![
        Case::new("base 2", Base(2.0)),
        Case::new("base 3", Base(3.0)),
    ]);
    family.spec_bug = SpecBug::BadClassFor(3.0);
    let message = run_expecting_failure(&family, &small_plan());
    assert!(message.contains("2 failures"), "{message}");
    assert!(
        message.contains("case \"base 3\" (forward, fast math on), probe \"specials\""),
        "{message}"
    );
    assert!(message.contains("NoSuchTransform"), "{message}");
    // Base 2 in both combinations.
    assert!(message.contains("comparisons: 2 buffers"), "{message}");
}

/// Every buffer of the plan is compared, not only the first. A port that quiets signalling
/// NaNs in alpha passes the halves (the plan's first buffer: their NaNs are quiet) and fails
/// the specials; a correct port's comparisons add up to every case and combination times every
/// buffer of the plan.
#[test]
fn every_probe_buffer_is_compared() {
    let plan = Plan {
        probes: vec![ProbeSet::Halves { stride: 61 }, ProbeSet::Specials],
        ..small_plan()
    };
    let buffers = plan.probes.iter().flat_map(ProbeSet::rgba_buffers).count();
    let family = LogFamily::new(vec![Case::new("base 2", Base(2.0))]);
    let summary = run_with(&family, &plan);
    assert_eq!(summary.comparisons, summary.groups * buffers);

    let mut family = LogFamily::new(vec![Case::new("base 2", Base(2.0))]);
    family.port = Kind::QuietAlpha;
    let message = run_expecting_failure(&family, &plan);
    assert!(message.contains("2 failures"), "{message}");
    assert!(message.contains("probe \"specials\""), "{message}");
    assert!(!message.contains("probe \"every 61st half\""), "{message}");
}

/// W0002 doesn't hide a wrong port: on a NaN-parameter case, the identity port fails in the
/// channels W0002 covers (a NaN from the wheel must stay NaN).
#[test]
fn a_wrong_port_fails_under_w0002_too() {
    let mut family = LogFamily::new(vec![Case::new("base NaN", Base(f64::NAN))]);
    family.port = Kind::Identity;
    let message = run_expecting_failure(&family, &small_plan());
    assert!(message.contains("2 failures"), "{message}");
    assert!(
        message.contains("NaN bits waived by W0002 in channels [true, true, true, false]"),
        "{message}"
    );
}

/// W0002 lets NaN bits differ in the channels of a NaN parameter and nowhere else: with a NaN
/// base, NaNs with flipped signs pass in the colour channels (and are counted), and fail in
/// alpha.
#[test]
fn w0002_waives_nan_bits_in_the_nan_parameter_channels_only() {
    let mut family = LogFamily::new(vec![Case::new("base NaN", Base(f64::NAN))]);
    family.port = Kind::FlipColourNans;
    let summary = run_with(&family, &small_plan());
    assert_eq!(summary.w0002_comparisons, 2);
    assert!(summary.w0002_waived.iter().all(|(_, _, n)| *n > 0));
    assert_eq!(summary.w0002_waived.len(), 2);

    let mut family = LogFamily::new(vec![Case::new("base NaN", Base(f64::NAN))]);
    family.port = Kind::FlipAlphaNans;
    let message = run_expecting_failure(&family, &small_plan());
    assert!(message.contains("2 failures"), "{message}");
}

/// A run that compares nothing fails: here the plan has no probes.
#[test]
fn a_run_that_compares_nothing_fails() {
    let plan = Plan {
        probes: Vec::new(),
        generated_probes: Vec::new(),
        ..small_plan()
    };
    let family = LogFamily::new(vec![Case::new("base 2", Base(2.0))]);
    let message = run_expecting_failure(&family, &plan);
    assert!(message.contains("nothing was compared"), "{message}");
}

/// Other profiles are compared on the pass-through channels only; a family that gives other
/// profiles without pass-through channels fails, rather than think they are checked.
#[test]
fn other_profiles_without_pass_through_channels_fail() {
    let mut family = LogFamily::new(vec![Case::new("base 2", Base(2.0))]);
    family.other_profiles = true;
    let message = run_expecting_failure(&family, &small_plan());
    assert!(message.contains("1 failures"), "{message}");
    assert!(
        message.contains("gives 2 other profiles but no pass-through channels"),
        "{message}"
    );
}

/// Every spec route the generated cases take needs an explicit case on that route that the
/// wheel accepts and the battery compares: otherwise a bug in that spec (the verifier's F2a,
/// gamma and offset swapped in a YAML template) only shows as refusals, which are left out
/// while validation isn't ported. The generated NaN and ±Inf cases take the YAML route here.
#[test]
fn generated_cases_need_a_compared_explicit_case_on_their_spec_route() {
    let plan = Plan {
        mutations: Mutations::Sampled,
        ..small_plan()
    };
    let mut family = LogFamily::new(vec![Case::new("base 2", Base(2.0))]);
    family.bases = vec![Case::new("base 2", Base(2.0))];
    family.validation = Validation::NotPorted { card: "WP 1.3l1" };
    let message = run_expecting_failure(&family, &plan);
    assert!(message.contains("1 failures"), "{message}");
    assert!(
        message.contains("generated cases use YAML specs, but no explicit case on that route"),
        "{message}"
    );

    family.cases.push(Case::new("base NaN", Base(f64::NAN)));
    let summary = run_with(&family, &plan);
    assert!(summary.generated_cases > 0);
}

/// OCIO logging a warning fails the case, unless the case allows it: a misspelled optional
/// key is ignored with a warning, and the case would silently run the default.
#[test]
fn an_ocio_warning_fails_its_case_unless_allowed() {
    let mut family = LogFamily::new(vec![Case::new("base NaN", Base(f64::NAN))]);
    family.spec_bug = SpecBug::UnknownKey;
    let message = run_expecting_failure(&family, &small_plan());
    assert!(message.contains("2 failures"), "{message}");
    assert!(message.contains("OCIO logged"), "{message}");
    assert!(message.contains("'bse'"), "{message}");

    let mut family = LogFamily::new(vec![
        Case::new("base NaN", Base(f64::NAN)).allow_log("Unknown key in LogTransform: 'bse'"),
    ]);
    family.spec_bug = SpecBug::UnknownKey;
    let summary = run_with(&family, &small_plan());
    assert_eq!(summary.comparisons, 2);
}

/// `Case::allow_log` takes only a fragment specific to the message. An empty fragment, or
/// OCIO's `[OpenColorIO Warning]` prefix, would allow every warning, a misspelled key's
/// included, and generated cases inherit it (the verifier's repro: with `allow_log("")`, a
/// misspelled key in a case whose pixels match the default's passed). A fragment of the
/// wheel's message, here with its prefix, still allows it.
#[test]
fn allow_log_needs_a_fragment_specific_to_the_wheels_message() {
    for fragment in ["", "[OpenColorIO Warning]"] {
        let result = catch_unwind(|| Case::new("base NaN", Base(f64::NAN)).allow_log(fragment));
        let Err(payload) = result else {
            panic!("allow_log({fragment:?}) took the fragment");
        };
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_default();
        assert!(message.contains("is too broad"), "{fragment:?}: {message}");
    }

    let mut family =
        LogFamily::new(vec![Case::new("base NaN", Base(f64::NAN)).allow_log(
            "[OpenColorIO Warning]: Unknown key in LogTransform: 'bse'",
        )]);
    family.spec_bug = SpecBug::UnknownKey;
    let summary = run_with(&family, &small_plan());
    assert_eq!(summary.comparisons, 2);
}
