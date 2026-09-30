// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The battery's engine: plans the jobs (case × combination × probe buffer), runs them against
//! the wheel in batches, and compares each response with the port.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use super::params::{Case, Channels, Comparison, Origin, mutations, sampled_mutations};
use super::{Combo, Family, Format, Mutations, Plan, Port, Summary, Validation};
use crate::compare::f32_bits_report;
use crate::oracle::{BatchCall, Oracle, Response, f32_to_bytes};
use crate::probe::ProbeSet;

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

/// One oracle call and its comparison.
struct Job {
    case: usize,
    combo: usize,
    args: Arc<Value>,
    buffer: Arc<Buffer>,
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
            for buffer in shared.iter().chain(near.iter()) {
                jobs.push(Job {
                    case: c,
                    combo: k,
                    args: Arc::clone(&args),
                    buffer: Arc::clone(buffer),
                });
            }
        }
    }

    let mut checker = Checker {
        family,
        cases: &cases,
        combos: &combos,
        current: None,
        refused: HashMap::new(),
        reported: HashSet::new(),
        waived: HashMap::new(),
        summary: &mut summary,
    };
    let oracle = Oracle::get();
    let mut start_job = 0;
    while start_job < jobs.len() {
        let mut end = start_job;
        let mut bytes = 0;
        while end < jobs.len()
            && end - start_job < MAX_CALLS_PER_BATCH
            && (end == start_job || bytes + jobs[end].buffer.bytes.len() <= plan.batch_bytes)
        {
            bytes += jobs[end].buffer.bytes.len();
            end += 1;
        }
        let batch = &jobs[start_job..end];
        let calls: Vec<BatchCall<'_>> = batch
            .iter()
            .map(|job| BatchCall {
                cmd: "cpu_apply",
                args: (*job.args).clone(),
                blobs: vec![&job.buffer.bytes],
            })
            .collect();
        let responses = oracle.batch(&calls, plan.cache);
        checker.summary.oracle_calls += calls.len();
        checker.summary.oracle_batches += 1;
        drop(calls);
        for (job, response) in batch.iter().zip(responses) {
            checker.check(job, response);
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

    fn check(&mut self, job: &Job, response: Response) {
        let group = (job.case, job.combo);
        let refusal = response.result.get("exception").map(|e| {
            e.get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        });
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
        let input = &job.buffer.pixels;
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
        let probe = format!("probe \"{}\" ({} pixels)", job.buffer.name, input.len() / 4);
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
        self.summary.values += input.len();
        self.summary.pass_through_checks += pass_through_checks;
        if case.w0002_applies(&combo) {
            self.summary.w0002_comparisons += 1;
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

    fn finish(self) {
        let mut waived: Vec<((usize, usize), usize)> =
            self.waived.into_iter().filter(|(_, n)| *n > 0).collect();
        waived.sort();
        for (group, n) in waived {
            let label = format!("{} ({})", self.cases[group.0].label(), self.combos[group.1]);
            self.summary.w0002_waived.push((label, n));
        }
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
