// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The battery's LUT support (card T2.1) against the real oracle: a family whose spec passes
//! the LUT's values as a blob ([`Spec::TransformWithBlobs`]), whose LUT entries are slots
//! ([`LutEntries`]), and which probes rows of every length ([`ProbeSet::RowLengths`]) and the
//! LUT's nodes and midpoints ([`lut_domain_points`]).
//!
//! The "port" of these tests is the wheel itself: for each case and combination, one oracle
//! process applies the same spec, with the same blob, to every buffer the plan probes, outside
//! the battery's batch (`WheelPort`). No port code is involved.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use ocio_testkit::Oracle;
use ocio_testkit::battery::params::{Case, LutEntries, Params, Slot};
use ocio_testkit::battery::{
    Combo, Direction, Family, Mutations, Plan, Port, Spec, Summary, run_with,
};
use ocio_testkit::oracle::{BatchCall, f32_to_bytes};
use ocio_testkit::probe::{ProbeSet, lut_domain_points};
use serde_json::{Value, json};

/// Entries in the test's 1D LUT.
const ENTRIES: usize = 17;

/// The longest row the test probes.
const ROWS: usize = 33;

/// A Lut1DTransform's values, red, green and blue per entry, with chosen entries as slots.
#[derive(Debug, Clone, PartialEq)]
struct Lut {
    values: Vec<f32>,
    entries: LutEntries,
}

impl Lut {
    /// A curve per channel: `x^1.5`, `x^2` and `x^0.75` at the entries' inputs.
    fn curves() -> Lut {
        let values = (0..ENTRIES)
            .flat_map(|i| {
                let x = i as f64 / (ENTRIES - 1) as f64;
                [x.powf(1.5), x * x, x.powf(0.75)].map(|v| v as f32)
            })
            .collect();
        Lut {
            values,
            entries: LutEntries::first_second_middle_last("lut", ENTRIES),
        }
    }

    /// The curves with `value` in component `component` of entry `entry`.
    fn with(entry: usize, component: usize, value: f32) -> Lut {
        let mut lut = Lut::curves();
        lut.values[3 * entry + component] = value;
        lut
    }
}

impl Params for Lut {
    fn slots(&self) -> Vec<Slot> {
        self.entries.slots()
    }
    fn get(&self, index: usize) -> f64 {
        self.entries.get(&self.values, index)
    }
    fn set(&mut self, index: usize, value: f64) {
        self.entries.set(&mut self.values, index, value);
    }
}

/// The LUT as a transform spec, its values as blob 0.
fn spec(lut: &Lut, direction: Direction) -> Spec {
    Spec::with_f32_blobs(
        json!({"class": "Lut1DTransform", "calls": [
            ["setData", {"blob": 0, "dtype": "float32"}],
            ["setDirection", direction.oracle_enum()],
        ]}),
        &[&lut.values],
    )
}

/// Every buffer the test's plan probes for the LUT: the specials, the neighbourhoods of the
/// LUT's nodes and midpoints, and the rows.
fn planned_buffers() -> Vec<Vec<f32>> {
    [
        ProbeSet::Specials,
        ProbeSet::Neighbourhoods {
            points: lut_domain_points(ENTRIES),
            ulps: 1,
        },
        ProbeSet::RowLengths { max_pixels: ROWS },
    ]
    .iter()
    .flat_map(ProbeSet::rgba_buffers)
    .map(|(_, pixels)| pixels)
    .collect()
}

/// The bits of some pixels, as a key.
fn key(pixels: &[f32]) -> Vec<u32> {
    pixels.iter().map(|v| v.to_bits()).collect()
}

/// The wheel's outputs for every planned buffer, from one oracle process, by input; or its
/// refusal.
fn wheel_outputs(spec: &Spec, combo: &Combo) -> Result<HashMap<Vec<u32>, Vec<f32>>, String> {
    let args = spec.cpu_apply_args(combo);
    let buffers = planned_buffers();
    let bytes: Vec<Vec<u8>> = buffers.iter().map(|b| f32_to_bytes(b)).collect();
    let calls: Vec<BatchCall<'_>> = bytes
        .iter()
        .map(|b| BatchCall {
            cmd: "cpu_apply",
            args: args.clone(),
            blobs: std::iter::once(b.as_slice())
                .chain(spec.blobs().iter().map(Vec::as_slice))
                .collect(),
        })
        .collect();
    let mut out = HashMap::new();
    for (buffer, response) in buffers.iter().zip(Oracle::get().batch(&calls, true)) {
        let response = response.expect("the oracle runs the call");
        if let Some(e) = response.result.get("exception") {
            return Err(e["message"].as_str().unwrap_or_default().to_string());
        }
        out.insert(key(buffer), response.blob_f32(0));
    }
    Ok(out)
}

/// What the port does with the wheel's output.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Bug {
    /// Nothing: the port is the wheel.
    None,
    /// Adds one to the red channel of rows of this many pixels.
    OnRowsOf(usize),
}

struct LutFamily {
    cases: Vec<Case<Lut>>,
    bases: Vec<Case<Lut>>,
    directions: Vec<Direction>,
    bug: Bug,
}

impl Family for LutFamily {
    type Params = Lut;

    fn name(&self) -> String {
        "Lut1DTransform (battery LUT self-test)".to_string()
    }
    fn cases(&self) -> Vec<Case<Lut>> {
        self.cases.clone()
    }
    fn mutation_bases(&self) -> Vec<Case<Lut>> {
        self.bases.clone()
    }
    fn directions(&self) -> Vec<Direction> {
        self.directions.clone()
    }
    fn spec(&self, lut: &Lut, direction: Direction) -> Spec {
        spec(lut, direction)
    }
    fn port(&self, lut: &Lut, combo: &Combo) -> Result<Port, String> {
        let outputs = wheel_outputs(&spec(lut, combo.direction), combo)?;
        let bug = self.bug;
        Ok(Port::in_place(move |px| {
            let out = outputs.get(&key(px)).unwrap_or_else(|| {
                panic!("a buffer of {} pixels the plan doesn't have", px.len() / 4)
            });
            px.copy_from_slice(out);
            if bug == Bug::OnRowsOf(px.len() / 4) {
                px[0] += 1.0;
            }
        }))
    }
    fn breakpoints(&self, _: &Lut, _: Direction) -> Vec<f32> {
        lut_domain_points(ENTRIES)
    }
    fn extra_probes(&self, _: &Lut, _: Direction) -> Vec<ProbeSet> {
        vec![ProbeSet::RowLengths { max_pixels: ROWS }]
    }
}

/// The specials and the LUT's own probes; no generated cases.
fn plan() -> Plan {
    Plan {
        name: "LUT self-test".to_string(),
        probes: vec![ProbeSet::Specials],
        generated_probes: vec![ProbeSet::Specials],
        breakpoint_ulps: 1,
        mutations: Mutations::None,
        sweep: None,
        batch_bytes: 256 << 20,
        cache: true,
    }
}

/// The curves, a NaN in the second entry's red, and +Inf in the middle entry's green.
fn cases() -> Vec<Case<Lut>> {
    vec![
        Case::new("curves", Lut::curves()),
        Case::new("NaN lut[1].r", Lut::with(1, 0, f32::NAN)),
        Case::new("inf lut[8].g", Lut::with(ENTRIES / 2, 1, f32::INFINITY)),
    ]
}

/// A LUT whose values reach the wheel as a blob passes against a port that reproduces the
/// wheel: every case in every combination on the specials, the neighbourhoods of the LUT's
/// nodes and midpoints, and one row of each length from 1 to 33 pixels. Its NaN entry is
/// compared bit for bit: W0002 doesn't cover LUT entries.
#[test]
fn a_lut_given_as_a_blob_passes() {
    let family = LutFamily {
        cases: cases(),
        bases: Vec::new(),
        directions: Direction::BOTH.to_vec(),
        bug: Bug::None,
    };
    let summary: Summary = run_with(&family, &plan());
    assert_eq!(summary.failures, Vec::<String>::new());
    assert_eq!(summary.groups, 3 * 4);
    // Per group: the specials, the neighbourhoods, and the 33 rows.
    assert_eq!(summary.comparisons, 3 * 4 * (2 + ROWS));
    assert_eq!(summary.w0002_comparisons, 0);
    assert_eq!(summary.oracle_batches, 1);
}

/// A port wrong only on rows of one length fails there, and only there.
#[test]
fn a_port_wrong_on_one_row_length_fails() {
    let family = LutFamily {
        cases: cases()[..1].to_vec(),
        bases: Vec::new(),
        directions: Direction::BOTH.to_vec(),
        bug: Bug::OnRowsOf(7),
    };
    let result = catch_unwind(AssertUnwindSafe(|| run_with(&family, &plan())));
    let payload = result.expect_err("the battery should fail");
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(message.contains("4 failures"), "{message}");
    assert!(
        message.contains("probe \"row of 7 pixels\" (7 pixels)"),
        "{message}"
    );
    assert!(!message.contains("row of 6 pixels"), "{message}");
}

/// The generated cases of a LUT put each value in one component of one chosen entry; the
/// wheel takes every one of them (the port, the wheel itself, compares them all).
#[test]
fn generated_cases_change_one_chosen_entry() {
    let family = LutFamily {
        cases: cases()[..1].to_vec(),
        bases: cases()[..1].to_vec(),
        directions: vec![Direction::Forward],
        bug: Bug::None,
    };
    let plan = Plan {
        mutations: Mutations::Sampled,
        ..plan()
    };
    let summary = run_with(&family, &plan);
    assert_eq!(summary.failures, Vec::<String>::new());
    // Four chosen entries (0, 1, 8, 16), three components, two values each.
    assert_eq!(summary.generated_cases, 4 * 3 * 2);
    assert_eq!(summary.left_out, Vec::<(String, String)>::new());
    assert_eq!(summary.w0002_comparisons, 0);
}

/// The spec's blob is what `cpu_apply` gets after the pixels: an explicit check of the
/// arguments and blobs a [`Spec::TransformWithBlobs`] gives.
#[test]
fn a_spec_with_blobs_gives_cpu_apply_its_blobs() {
    let lut = Lut::curves();
    let spec = spec(&lut, Direction::Inverse);
    assert_eq!(spec.blobs(), [f32_to_bytes(&lut.values)]);
    let combo = Combo {
        direction: Direction::Inverse,
        fast_math: false,
        format: ocio_testkit::battery::Format::F32_RGBA,
    };
    let args = spec.cpu_apply_args(&combo);
    let Spec::TransformWithBlobs(transform, _) = &spec else {
        panic!("not a spec with blobs");
    };
    assert_eq!(args["transform"], *transform);
    assert!(args.get("optimization").is_some_and(Value::is_array));
}
