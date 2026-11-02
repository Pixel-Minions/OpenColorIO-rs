// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Private CI transport for the two original ICC input sweeps. Cases are built in
//! their original order before selection, including every RNG draw. Comparisons
//! stay in common::sweep; the original aggregate assertions run after the union.

use std::path::Path;

use ocio_testkit::fixtures::sha256_hex;
use serde_json::{Value, json};

use crate::common::{Case, Summary, sweep};

pub(crate) const PARTS: usize = 4;
const GROUPS: [&str; 2] = ["ociobakelut_icc_generated", "ociobakelut_icc_corrupted"];

#[derive(Debug, Default)]
pub(crate) struct Audit {
    pub(crate) writing_files: usize,
    pub(crate) deaths: usize,
    pub(crate) port_stops: usize,
    pub(crate) errors: Vec<String>,
}

impl Audit {
    pub(crate) fn from_summary(summary: &Summary) -> Self {
        Self {
            writing_files: summary.writing_files(),
            deaths: summary
                .outcomes
                .iter()
                .filter(|o| !(0..=255).contains(&o.exit_code))
                .count(),
            port_stops: summary.port_stops,
            errors: summary
                .outcomes
                .iter()
                .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
                .collect(),
        }
    }
}

fn indices(total: usize, part: usize) -> Vec<usize> {
    assert!(part < PARTS);
    (part..total).step_by(PARTS).collect()
}

/// SHA256 covers every case input/control and each file's original bytes. Paths
/// and argv retain the original global case index on both wheel and port runs.
pub(crate) fn plan(cases: &[Case]) -> Vec<Value> {
    cases
        .iter()
        .enumerate()
        .map(|(index, case)| {
            assert!(case.original_index.is_none());
            case_record(case, index)
        })
        .collect()
}

fn case_record(case: &Case, index: usize) -> Value {
    let files: Vec<_> = case
        .files
        .iter()
        .map(|(name, bytes)| json!([name, bytes.len(), sha256_hex(bytes)]))
        .collect();
    let inputs = json!({"label":case.label, "argv":case.argv, "env":case.env,
            "files":files, "dies":case.dies, "attributes":case.attributes,
            "merged":case.merged, "closed_pipe":case.closed_pipe,
            "sigpipe_ignored":case.sigpipe_ignored, "head_bytes":case.head_bytes,
            "icc":case.icc, "stop":format!("{:?}",case.stop)});
    json!({"index":index, "label":case.label, "input_sha256":sha256_hex(&serde_json::to_vec(&inputs).unwrap())})
}

fn profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

fn source() -> String {
    let head = std::env::var("OCIO_RS_ICC_SOURCE").expect("ICC CI source must be recorded");
    assert!(
        head.len() == 40 && head.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid ICC CI source"
    );
    head
}

pub(crate) fn run(group: &str, tool: &str, program: &str, cases: &[Case]) -> Option<Audit> {
    let Ok(selection) = std::env::var("OCIO_RS_ICC_PART") else {
        return Some(Audit::from_summary(&sweep(group, tool, program, cases)));
    };
    assert_eq!(
        std::env::consts::OS,
        "linux",
        "ICC partition execution is Linux-only"
    );
    assert!(GROUPS.contains(&group));
    let part = (0..PARTS)
        .find(|p| selection == format!("{p}/{PARTS}"))
        .expect("invalid ICC partition selection");
    let original = plan(cases);
    let selected = indices(cases.len(), part);
    let chosen: Vec<_> = selected
        .iter()
        .map(|&index| {
            let mut case = cases[index].clone();
            case.original_index = Some(index);
            case
        })
        .collect();
    // Fingerprint the ACTUAL immutable cases passed to sweep, independently of
    // the declared indices. A wrong-case substitution must fail the union audit.
    let executed_inputs: Vec<_> = chosen
        .iter()
        .map(|case| case_record(case, case.original_index.expect("selected original index")))
        .collect();
    let summary = sweep(group, tool, program, &chosen);
    let outcomes: Vec<_> = summary
        .outcomes
        .iter()
        .map(|o| {
            json!({
                "exit_code":o.exit_code, "writing_files":!o.files.is_empty(),
                "stderr":String::from_utf8_lossy(&o.stderr),
            })
        })
        .collect();
    let report = json!({"schema":1, "source":source(), "profile":profile(),
        "tier":std::env::var("OCIO_RS_TIER").unwrap(), "group":group,
        "part":part, "parts":PARTS, "original":original, "indices":selected,
        "executed_inputs":executed_inputs, "outcomes":outcomes,
        "port_stops":summary.port_stops, "exact_comparisons_passed":true});
    let directory =
        std::env::var_os("OCIO_RS_ICC_REPORT_DIR").expect("ICC partition report directory");
    let directory = Path::new(&directory);
    std::fs::create_dir_all(directory).unwrap();
    let file = directory.join(format!("{group}.json"));
    // Never let a stale success survive a failed or interrupted rerun.
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(file)
        .unwrap();
    serde_json::to_writer(&mut out, &report).unwrap();
    println!(
        "ICC partition {group} {part}/{PARTS}: {} original cases compared",
        chosen.len()
    );
    None // Global original assertions are required by the separate complete-union job.
}

fn number(value: &Value) -> usize {
    value
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .expect("nonnegative ICC audit integer")
}

fn validate(
    reports: &[Value],
    group: &str,
    original: &[Value],
    head: &str,
    profile: &str,
    tier: &str,
) -> Audit {
    assert_eq!(reports.len(), PARTS, "every partition report is required");
    let mut audit = Audit::default();
    let mut union = Vec::new();
    for (part, report) in reports.iter().enumerate() {
        assert_eq!(report["schema"], 1);
        assert_eq!(report["source"], head);
        assert_eq!(report["profile"], profile);
        assert_eq!(report["tier"], tier);
        assert_eq!(report["group"], group);
        assert_eq!(number(&report["part"]), part);
        assert_eq!(number(&report["parts"]), PARTS);
        assert_eq!(report["exact_comparisons_passed"], true);
        assert_eq!(
            report["original"],
            json!(original),
            "case inputs or original order changed"
        );
        let selected = indices(original.len(), part);
        assert_eq!(
            report["indices"],
            json!(selected),
            "missing/duplicate/foreign original case index"
        );
        let expected_inputs: Vec<_> = selected.iter().map(|&index| &original[index]).collect();
        assert_eq!(
            report["executed_inputs"],
            json!(expected_inputs),
            "actual executed case inputs differ from the original selection"
        );
        let outcomes = report["outcomes"].as_array().expect("actual outcomes");
        assert_eq!(
            outcomes.len(),
            selected.len(),
            "every selected case must finish"
        );
        let stops = number(&report["port_stops"]);
        assert!(stops <= outcomes.len());
        audit.port_stops += stops;
        for outcome in outcomes {
            let code = outcome["exit_code"].as_i64().expect("wheel exit code");
            audit.deaths += usize::from(!(0..=255).contains(&code));
            audit.writing_files +=
                usize::from(outcome["writing_files"].as_bool().expect("file outcome"));
            audit.errors.push(
                outcome["stderr"]
                    .as_str()
                    .expect("wheel stderr")
                    .to_string(),
            );
        }
        union.extend(selected);
    }
    union.sort_unstable();
    assert_eq!(
        union,
        (0..original.len()).collect::<Vec<_>>(),
        "original case union must run exactly once"
    );
    audit
}

pub(crate) fn joined(root: &Path, group: &str, cases: &[Case]) -> Audit {
    assert!(GROUPS.contains(&group));
    let profile = profile();
    let reports: Vec<Value> = (0..PARTS)
        .map(|part| {
            let path = root
                .join(format!("linux-{profile}-icc-{part}"))
                .join(format!("{group}.json"));
            serde_json::from_slice(
                &std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
            )
            .unwrap()
        })
        .collect();
    let tier = std::env::var("OCIO_RS_TIER").expect("audit must use the original tier");
    assert_eq!(tier, "full", "required CI keeps its full tier");
    let audit = validate(&reports, group, &plan(cases), &source(), profile, &tier);
    println!(
        "ICC exact-once audit {group} {profile}: {} original cases",
        cases.len()
    );
    audit
}

/// Small protocol controls run without the SDK/oracle. They prove rejection, not
/// ICC output coverage; the actual native plans/outcomes above are mandatory.
#[cfg(test)]
mod controls {
    use super::*;

    fn fixture() -> (Vec<Value>, Vec<Value>) {
        let cases: Vec<_> = (0..17)
            .map(|i| {
                Case::new(format!("case-{i}"), &["--input"]).file("input", format!("bytes-{i}"))
            })
            .collect();
        let original = plan(&cases);
        let reports = (0..PARTS).map(|part| {
            let selected = indices(original.len(),part);
            let outcomes: Vec<_> = selected.iter().map(|_| json!({"exit_code":0,"writing_files":true,"stderr":""})).collect();
            json!({"schema":1,"source":"source","profile":"debug","tier":"full", "group":GROUPS[0],
                "part":part,"parts":PARTS,"original":original,"indices":selected,
                "executed_inputs":selected.iter().map(|&index|case_record(&cases[index],index)).collect::<Vec<_>>(),"outcomes":outcomes,
                "port_stops":0,"exact_comparisons_passed":true})
        }).collect();
        (original, reports)
    }

    #[test]
    fn complete_partition_union_and_rejection_controls() {
        let (original, reports) = fixture();
        assert_eq!(
            validate(&reports, GROUPS[0], &original, "source", "debug", "full").writing_files,
            original.len()
        );
        for field in [
            "schema",
            "source",
            "profile",
            "tier",
            "group",
            "part",
            "parts",
            "original",
            "indices",
            "executed_inputs",
            "outcomes",
            "port_stops",
            "exact_comparisons_passed",
        ] {
            let mut changed = reports.clone();
            changed[2].as_object_mut().unwrap().remove(field);
            assert!(
                std::panic::catch_unwind(|| validate(
                    &changed, GROUPS[0], &original, "source", "debug", "full"
                ))
                .is_err(),
                "accepted missing {field}"
            );
        }
        let mut changed = reports.clone();
        changed[1]["indices"][0] = changed[0]["indices"][0].clone();
        assert!(
            std::panic::catch_unwind(|| validate(
                &changed, GROUPS[0], &original, "source", "debug", "full"
            ))
            .is_err()
        );
        let mut changed = reports.clone();
        changed[3]["original"][0]["input_sha256"] = json!("changed-file");
        assert!(
            std::panic::catch_unwind(|| validate(
                &changed, GROUPS[0], &original, "source", "debug", "full"
            ))
            .is_err()
        );
        assert!(
            std::panic::catch_unwind(|| validate(
                &reports[..3],
                GROUPS[0],
                &original,
                "source",
                "debug",
                "full"
            ))
            .is_err()
        );
        for replacement in [json!(false), json!(0.0), json!(17), json!(-1)] {
            let mut changed = reports.clone();
            changed[0]["indices"][0] = replacement;
            assert!(
                std::panic::catch_unwind(|| validate(
                    &changed, GROUPS[0], &original, "source", "debug", "full"
                ))
                .is_err()
            );
        }
        for field in ["source", "profile", "tier", "group"] {
            let mut changed = reports.clone();
            changed[0][field] = json!("foreign");
            assert!(
                std::panic::catch_unwind(|| validate(
                    &changed, GROUPS[0], &original, "source", "debug", "full"
                ))
                .is_err()
            );
        }
        let mut changed = reports.clone();
        changed[0]["exact_comparisons_passed"] = json!(false);
        assert!(
            std::panic::catch_unwind(|| validate(
                &changed, GROUPS[0], &original, "source", "debug", "full"
            ))
            .is_err()
        );
        let mut changed = reports.clone();
        changed[0]["outcomes"].as_array_mut().unwrap().pop();
        assert!(
            std::panic::catch_unwind(|| validate(
                &changed, GROUPS[0], &original, "source", "debug", "full"
            ))
            .is_err()
        );
        let mut changed = reports.clone();
        // Declared index0 remains valid, but the actual executed input is foreign.
        changed[0]["executed_inputs"][0] = case_record(
            &Case::new("wrong-case", &["--input"]).file("input", "foreign bytes"),
            0,
        );
        assert!(
            std::panic::catch_unwind(|| validate(
                &changed, GROUPS[0], &original, "source", "debug", "full"
            ))
            .is_err()
        );
    }
}
