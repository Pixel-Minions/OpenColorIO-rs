// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The battery's engine: plans the jobs (case × combination × probe buffer), runs them against
//! the wheel in batches, and compares each response with the port.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use super::params::{Case, Channels, Comparison, Origin, mutations, sampled_mutations};
use super::{Combo, Family, Format, Mutations, Plan, Port, Spec, Summary, Validation};
use crate::compare::f32_bits_report;
use crate::oracle::{BatchCall, Oracle, Response, f32_to_bytes};
use crate::probe::{ALL_F32_PIXELS, ProbeSet, all_f32_chunk};

/// Most calls per oracle process, to keep the request's JSON header small.
const MAX_CALLS_PER_BATCH: usize = 20_000;

/// Failures reported in full; later ones keep their first line.
const FULL_REPORTS: usize = 100;

/// One RGBA probe buffer, shared by every job that uses it.
#[derive(Debug)]
struct Buffer {
    name: String,
    pixels: Vec<f32>,
    bytes: Vec<u8>,
}

impl Buffer {
    fn new(name: String, pixels: Vec<f32>) -> Arc<Buffer> {
        let bytes = f32_to_bytes(&pixels);
        Arc::new(Buffer {
            name,
            pixels,
            bytes,
        })
    }
}

fn buffers(sets: &[ProbeSet]) -> Vec<Arc<Buffer>> {
    sets.iter()
        .flat_map(ProbeSet::rgba_buffers)
        .map(|(name, pixels)| Buffer::new(name, pixels))
        .collect()
}

/// The input pixels of a job.
enum Input {
    /// A probe buffer.
    Buffer(Arc<Buffer>),
    /// A chunk of the sweep of every `f32`, made when its batch runs.
    Sweep {
        /// The chunk.
        index: u64,
        /// Its pixels.
        pixels: u64,
    },
}

impl Input {
    /// The input's size in bytes.
    fn bytes(&self) -> usize {
        match self {
            Input::Buffer(buffer) => buffer.bytes.len(),
            Input::Sweep { pixels, .. } => *pixels as usize * 16,
        }
    }

    fn buffer(&self) -> Arc<Buffer> {
        match self {
            Input::Buffer(buffer) => Arc::clone(buffer),
            Input::Sweep { index, pixels } => Buffer::new(
                format!(
                    "every f32, chunk {} of {}",
                    index + 1,
                    ALL_F32_PIXELS / pixels
                ),
                all_f32_chunk(*index, *pixels),
            ),
        }
    }
}

/// How a spec reaches the wheel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Route {
    /// A JSON transform spec (`Spec::Transform`).
    Transform,
    /// One transform in a config's YAML (`Spec::Yaml`).
    Yaml,
    /// One transform in a version 1 config's YAML (`Spec::YamlV1`).
    YamlV1,
}

impl Route {
    fn of(spec: &Spec) -> Route {
        match spec {
            Spec::Transform(_) => Route::Transform,
            Spec::Yaml(_) => Route::Yaml,
            Spec::YamlV1(_) => Route::YamlV1,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Route::Transform => "JSON transform",
            Route::Yaml => "YAML",
            Route::YamlV1 => "YAML in a version 1 config",
        }
    }
}

/// One oracle call and its comparison.
struct Job {
    case: usize,
    combo: usize,
    args: Arc<Value>,
    input: Input,
}

/// The port's renderers for one case and combination.
struct Ports {
    port: Result<Port, String>,
    others: Vec<(String, Port)>,
    pass_through: Channels,
}

pub(super) fn run<F: Family>(family: &F, plan: &Plan) -> Summary {
    let start = Instant::now();
    let name = family.name();
    let mut summary = Summary {
        family: name.clone(),
        plan: plan.name.clone(),
        ..Summary::default()
    };
    if let super::Validation::NotPorted { card } = family.validation() {
        summary.validation_card = Some(card);
    }

    let mut cases = family.cases();
    assert!(!cases.is_empty(), "{name}: no cases");
    summary.explicit_cases = cases.len();
    for base in family.mutation_bases() {
        match plan.mutations {
            Mutations::None => {}
            Mutations::Sampled => cases.extend(sampled_mutations(&base)),
            Mutations::All => cases.extend(mutations(&base)),
        }
    }
    summary.generated_cases = cases.len() - summary.explicit_cases;

    let mut combos = Vec::new();
    for format in family.formats() {
        assert!(
            format == Format::F32_RGBA,
            "{name}: format {format:?} needs the port's CPU engine and packing (WP 1.1, 1.2d), \
             and planar layouts need the oracle's planar images (O1.2); the battery runs F32 \
             RGBA only for now"
        );
        for direction in family.directions() {
            for fast_math in [true, false] {
                combos.push(Combo {
                    direction,
                    fast_math,
                    format,
                });
            }
        }
    }
    assert!(!combos.is_empty(), "{name}: no combinations");
    summary.groups = cases.len() * combos.len();

    let explicit_buffers = buffers(&plan.probes);
    let generated_buffers = buffers(&plan.generated_probes);

    let mut jobs = Vec::new();
    let mut sweeps = Vec::new();
    // Per case and combination, how many buffers the plan asks for: counted here from the
    // plan, apart from the jobs, so that the checker can prove each one was compared.
    let mut expected: HashMap<(usize, usize), usize> = HashMap::new();
    // Per case, the spec routes (JSON transform or YAML) its combinations use.
    let mut routes: Vec<HashSet<Route>> = vec![HashSet::new(); cases.len()];
    for (c, case) in cases.iter().enumerate() {
        let shared = match case.origin() {
            Origin::Explicit => &explicit_buffers,
            Origin::Generated { .. } => &generated_buffers,
        };
        let mut neighbourhoods: HashMap<super::Direction, Option<Arc<Buffer>>> = HashMap::new();
        let mut specs = HashMap::new();
        for (k, combo) in combos.iter().enumerate() {
            let spec = specs
                .entry(combo.direction)
                .or_insert_with(|| family.spec(case.params(), combo.direction));
            routes[c].insert(Route::of(spec));
            let args = Arc::new(spec.cpu_apply_args(combo));
            let near = neighbourhoods
                .entry(combo.direction)
                .or_insert_with(|| {
                    let points = family.breakpoints(case.params(), combo.direction);
                    if plan.breakpoint_ulps == 0 || points.is_empty() {
                        return None;
                    }
                    let set = ProbeSet::Neighbourhoods {
                        points,
                        ulps: plan.breakpoint_ulps,
                    };
                    set.rgba_buffers()
                        .into_iter()
                        .next()
                        .map(|(name, pixels)| Buffer::new(name, pixels))
                })
                .clone();
            let mut buffers_asked = shared.len() + usize::from(near.is_some());
            for buffer in shared.iter().chain(near.iter()) {
                jobs.push(Job {
                    case: c,
                    combo: k,
                    args: Arc::clone(&args),
                    input: Input::Buffer(Arc::clone(buffer)),
                });
            }
            if let Some(sweep) = plan.sweep
                && c < sweep.cases.min(summary.explicit_cases)
            {
                let pixels = sweep.chunk_pixels;
                let chunks = sweep
                    .chunks
                    .unwrap_or(u64::MAX)
                    .min(ALL_F32_PIXELS / pixels);
                buffers_asked += chunks as usize;
                sweeps.extend((0..chunks).map(|index| Job {
                    case: c,
                    combo: k,
                    args: Arc::clone(&args),
                    input: Input::Sweep { index, pixels },
                }));
            }
            expected.insert((c, k), buffers_asked);
        }
    }
    // The sweeps run last, so the probe buffers' batches stay the same with or without them.
    jobs.extend(sweeps);

    let mut checker = Checker {
        family,
        cases: &cases,
        combos: &combos,
        current: None,
        refused: HashMap::new(),
        reported: HashSet::new(),
        waived: HashMap::new(),
        refusals: HashMap::new(),
        expected,
        routes,
        log_reported: HashSet::new(),
        compared: HashMap::new(),
        other_profiles_reported: false,
        summary: &mut summary,
    };
    let oracle = Oracle::get();
    let mut start_job = 0;
    while start_job < jobs.len() {
        let mut end = start_job;
        let mut bytes = 0;
        while end < jobs.len()
            && end - start_job < MAX_CALLS_PER_BATCH
            && (end == start_job || bytes + jobs[end].input.bytes() <= plan.batch_bytes)
        {
            bytes += jobs[end].input.bytes();
            end += 1;
        }
        let batch = &jobs[start_job..end];
        let inputs: Vec<Arc<Buffer>> = batch.iter().map(|job| job.input.buffer()).collect();
        let calls: Vec<BatchCall<'_>> = batch
            .iter()
            .zip(&inputs)
            .map(|(job, input)| BatchCall {
                cmd: "cpu_apply",
                args: (*job.args).clone(),
                blobs: vec![&input.bytes],
            })
            .collect();
        // A sweep's responses would only fill the disk.
        let sweeping = batch
            .iter()
            .any(|job| matches!(job.input, Input::Sweep { .. }));
        let responses = oracle.batch(&calls, plan.cache && !sweeping);
        checker.summary.oracle_calls += calls.len();
        checker.summary.oracle_batches += 1;
        drop(calls);
        for ((job, input), response) in batch.iter().zip(&inputs).zip(responses) {
            checker.check(job, input, response);
        }
        start_job = end;
    }
    checker.finish();
    summary.elapsed = start.elapsed();
    summary
}

/// Compares the jobs' responses with the port, in job order.
struct Checker<'a, F: Family> {
    family: &'a F,
    cases: &'a [Case<F::Params>],
    combos: &'a [Combo],
    /// The port of the case and combination of the last job.
    current: Option<((usize, usize), Ports)>,
    /// Per case and combination: whether the wheel refused it (and with which text).
    refused: HashMap<(usize, usize), Option<String>>,
    /// Case and combination pairs whose refusal or port failure is already reported.
    reported: HashSet<(usize, usize)>,
    /// NaN values W0002 covered, per case and combination.
    waived: HashMap<(usize, usize), usize>,
    /// How many case and combination pairs the wheel refused with each text.
    refusals: HashMap<String, usize>,
    /// Per case and combination, how many buffers the plan asks for.
    expected: HashMap<(usize, usize), usize>,
    /// Per case, the spec routes its combinations use.
    routes: Vec<HashSet<Route>>,
    /// Case and combination pairs whose unexpected OCIO log messages are already reported.
    log_reported: HashSet<(usize, usize)>,
    /// Per case and combination, how many buffers were compared.
    compared: HashMap<(usize, usize), usize>,
    /// Whether other profiles without pass-through channels are already reported.
    other_profiles_reported: bool,
    summary: &'a mut Summary,
}

impl<F: Family> Checker<'_, F> {
    fn label(&self, group: (usize, usize)) -> String {
        format!(
            "case \"{}\" ({})",
            self.cases[group.0].label(),
            self.combos[group.1]
        )
    }

    fn ports(&mut self, group: (usize, usize)) -> &Ports {
        if self.current.as_ref().is_none_or(|(g, _)| *g != group) {
            let params = self.cases[group.0].params();
            let combo = &self.combos[group.1];
            let ports = Ports {
                port: self.family.port(params, combo),
                others: self.family.other_profiles(params, combo),
                pass_through: self.family.pass_through(params, combo),
            };
            if ports.pass_through.iter().any(|&p| p) {
                let non_finite = self.cases[group.0].non_finite_channels();
                assert!(
                    !(0..4).any(|c| ports.pass_through[c] && non_finite[c]),
                    "{}: a channel can't pass through and have a non-finite parameter",
                    self.label(group)
                );
            } else if !ports.others.is_empty() && !self.other_profiles_reported {
                // Only pass-through channels of other profiles are compared: with none, the
                // family would think its other profiles are checked when nothing is.
                self.other_profiles_reported = true;
                let message = format!(
                    "{}: the family gives {} other profiles but no pass-through channels, so \
                     none of them would be compared; declare `pass_through`, or give no other \
                     profiles",
                    self.label(group),
                    ports.others.len()
                );
                self.fail(message);
            }
            self.current = Some((group, ports));
        }
        &self.current.as_ref().expect("just set").1
    }

    /// Records a failure: its whole report for the first [`FULL_REPORTS`], its first line
    /// after that.
    fn fail(&mut self, message: String) {
        if self.summary.failures.len() < FULL_REPORTS {
            self.summary.failures.push(message);
        } else {
            let head = message.lines().next().unwrap_or_default().to_string();
            self.summary.failures.push(head);
        }
    }

    fn check(&mut self, job: &Job, buffer: &Buffer, response: Result<Response, String>) {
        let group = (job.case, job.combo);
        let probe = format!(
            "probe \"{}\" ({} pixels)",
            buffer.name,
            buffer.pixels.len() / 4
        );
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                // The oracle raised on this call: a bug in the family's spec or the harness.
                if self.reported.insert(group) {
                    let message = format!("{}, {probe}: {error}", self.label(group));
                    self.fail(message);
                }
                return;
            }
        };
        // OCIO logging a warning usually means the spec isn't what the family meant: a
        // misspelled optional key is ignored with a warning, and the case runs the default.
        let unexpected: Vec<String> = response
            .result
            .get("log")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|message| !self.cases[job.case].allows_log(message))
            .map(|message| message.trim_end().to_string())
            .collect();
        if !unexpected.is_empty() && self.log_reported.insert(group) {
            let message = format!(
                "{}, {probe}: OCIO logged {unexpected:?}; if the case expects it, allow it with \
                 `Case::allow_log`",
                self.label(group)
            );
            self.fail(message);
        }
        let refusal = response.result.get("exception").map(|e| {
            e.get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        });
        if let Some(message) = &refusal {
            // The wheel refuses parameters while building the transform (the Python bindings
            // validate it), the processor or the CPU processor. OCIO raising while loading the
            // config or applying the processor is a bug in the spec or the harness.
            let stage = response.result.get("stage").and_then(Value::as_str);
            if !matches!(stage, Some("transform" | "processor" | "cpu_processor")) {
                if self.reported.insert(group) {
                    let when = match stage {
                        Some("config") => "while loading the config".to_string(),
                        Some("apply") => "while applying the processor".to_string(),
                        Some(other) => format!("at stage {other:?}"),
                        None => "without a stage".to_string(),
                    };
                    let label = self.label(group);
                    self.fail(format!(
                        "{label}, {probe}: OCIO raised {when}, so the spec or the harness is \
                         wrong: {message}"
                    ));
                }
                return;
            }
        }
        match self.refused.get(&group) {
            Some(before) if before.is_some() != refusal.is_some() => {
                if self.reported.insert(group) {
                    let message = format!(
                        "{}: the wheel refused some probes and accepted others",
                        self.label(group)
                    );
                    self.fail(message);
                }
                return;
            }
            Some(_) => {}
            None => {
                self.refused.insert(group, refusal.clone());
            }
        }
        if let Some(message) = refusal {
            self.check_refusal(group, message);
            return;
        }

        let case = &self.cases[job.case];
        let combo = self.combos[job.combo];
        let label = self.label(group);
        let input = &buffer.pixels;
        let expected = response.blob_f32(0);
        let ports = self.ports(group);
        let port = match &ports.port {
            Ok(port) => port,
            Err(text) => {
                let text = text.clone();
                if self.reported.insert(group) {
                    self.fail(format!(
                        "{label}: the wheel accepts it, the port refuses it: {text}"
                    ));
                }
                return;
            }
        };
        let actual = port.apply(input);
        let comparison = case.compare(&combo, input, &expected, &actual);
        let pass_through = ports.pass_through;
        let mut failures = Vec::new();
        let mut pass_through_checks = 0;
        if pass_through.iter().any(|&p| p) {
            if let Some(report) = channels_report(pass_through, input, input, &expected) {
                failures.push(format!(
                    "{label}, {probe}: the wheel changes channels {pass_through:?}, which the \
                     family says pass through:\n{report}"
                ));
            }
            for (profile, other) in &ports.others {
                let other_actual = other.apply(input);
                pass_through_checks += 1;
                if let Some(report) = channels_report(pass_through, input, &expected, &other_actual)
                {
                    failures.push(format!(
                        "{label}, {probe}, profile {profile}: pass-through channels \
                         {pass_through:?} differ from the wheel:\n{report}"
                    ));
                }
            }
        }
        self.summary.comparisons += 1;
        *self.compared.entry(group).or_default() += 1;
        self.summary.values += input.len();
        self.summary.pass_through_checks += pass_through_checks;
        if case.w0002_applies(&combo) {
            self.summary.w0002_comparisons += 1;
            // Listed in the summary even when nothing needs the waiver.
            self.waived.entry(group).or_default();
        }
        match comparison {
            Comparison::Exact => {}
            Comparison::W0002 { waived } => *self.waived.entry(group).or_default() += waived,
            Comparison::Mismatch(report) => {
                failures.insert(0, format!("{label}, {probe}:\n{report}"));
            }
        }
        for failure in failures {
            self.fail(failure);
        }
    }

    fn check_refusal(&mut self, group: (usize, usize), wheel: String) {
        if !self.reported.insert(group) {
            return;
        }
        *self.refusals.entry(wheel.clone()).or_default() += 1;
        let label = self.label(group);
        let case = &self.cases[group.0];
        match self.family.validation() {
            Validation::NotPorted { .. } => {
                if matches!(case.origin(), Origin::Generated { .. }) {
                    let text = case.label().to_string();
                    if !self.summary.left_out.iter().any(|(l, _)| *l == text) {
                        self.summary.left_out.push((text, wheel));
                    }
                } else {
                    self.fail(format!(
                        "{label}: the wheel refuses this explicit case: {wheel}"
                    ));
                }
            }
            Validation::Ported => match &self.ports(group).port {
                Err(port) if *port == wheel => self.summary.refusals_compared += 1,
                Err(port) => {
                    let port = port.clone();
                    self.fail(format!(
                        "{label}: the wheel and the port refuse it with different texts:\n  \
                         wheel: {wheel}\n  port:  {port}"
                    ));
                }
                Ok(_) => self.fail(format!(
                    "{label}: the wheel refuses it, the port accepts it: {wheel}"
                )),
            },
        }
    }

    fn finish(mut self) {
        // Every case and combination was compared on every buffer the plan asks for, or its
        // refusal handled, or its failure reported; and something was compared at all.
        let mut groups: Vec<((usize, usize), usize)> =
            self.expected.iter().map(|(&g, &n)| (g, n)).collect();
        groups.sort();
        for (group, asked) in groups {
            let compared = self.compared.get(&group).copied().unwrap_or(0);
            if !self.reported.contains(&group) && compared != asked {
                let message = format!(
                    "{}: compared {compared} of the {asked} probe buffers the plan asks for",
                    self.label(group)
                );
                self.fail(message);
            }
        }
        if self.summary.comparisons + self.summary.refusals_compared == 0 {
            self.fail(
                "nothing was compared: every case was left out or failed, or the plan has no \
                 probes"
                    .to_string(),
            );
        }
        // Every spec route the generated cases take has an explicit case on it that the wheel
        // accepted and the battery compared. Otherwise a bug in that route's spec (for
        // example two parameters swapped in the YAML) only shows as refusals, and those are
        // left out while the family's validation isn't ported.
        let generated_routes: HashSet<Route> = self
            .cases
            .iter()
            .zip(&self.routes)
            .filter(|(case, _)| matches!(case.origin(), Origin::Generated { .. }))
            .flat_map(|(_, routes)| routes.iter().copied())
            .collect();
        let mut generated_routes: Vec<Route> = generated_routes.into_iter().collect();
        generated_routes.sort();
        for route in generated_routes {
            let covered = (0..self.cases.len()).any(|c| {
                self.cases[c].origin() == Origin::Explicit
                    && self.routes[c].contains(&route)
                    && (0..self.combos.len())
                        .any(|k| self.compared.get(&(c, k)).is_some_and(|&n| n > 0))
            });
            if !covered {
                self.fail(format!(
                    "generated cases use {} specs, but no explicit case on that route was \
                     compared: a bug in the family's {} spec would only show as refusals. Add \
                     an explicit case the wheel accepts on that route",
                    route.name(),
                    route.name()
                ));
            }
        }
        let mut waived: Vec<((usize, usize), usize)> = self.waived.into_iter().collect();
        waived.sort();
        for ((case, combo), n) in waived {
            let label = self.cases[case].label().to_string();
            self.summary
                .w0002_waived
                .push((label, self.combos[combo].to_string(), n));
        }
        let mut refusals: Vec<(String, usize)> = self.refusals.into_iter().collect();
        refusals.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        self.summary.refusals = refusals;
    }
}

/// A report of the differences between `expected` and `actual` in `channels` (RGBA) only.
fn channels_report(
    channels: Channels,
    inputs: &[f32],
    expected: &[f32],
    actual: &[f32],
) -> Option<String> {
    let masked: Vec<f32> = actual
        .iter()
        .zip(expected)
        .enumerate()
        .map(|(i, (a, e))| if channels[i % 4] { *a } else { *e })
        .collect();
    if masked.len() != expected.len() {
        return f32_bits_report(expected, actual, Some(inputs), 4);
    }
    f32_bits_report(expected, &masked, Some(inputs), 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_reports_look_only_at_their_channels() {
        let inputs = [0.0f32; 8];
        let expected = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let mut actual = expected;
        actual[0] = 0.5;
        actual[4] = 0.5;
        assert!(channels_report([false, true, true, true], &inputs, &expected, &actual).is_none());
        let report = channels_report([true, false, false, false], &inputs, &expected, &actual)
            .expect("red differs");
        assert!(
            report.starts_with("2 of 8 values differ bitwise"),
            "{report}"
        );
        actual[7] = f32::from_bits(0x7fc0_0000);
        let report = channels_report([false, false, false, true], &inputs, &expected, &actual)
            .expect("alpha differs");
        assert!(
            report.starts_with("1 of 8 values differ bitwise"),
            "{report}"
        );
    }
}
