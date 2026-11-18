// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Stale port markers (the 2.6.0 plan's M2, option (b), orchestrator 2026-11-18): a test
//! ported from 2.5.2 keeps its ` @ v2.5.2.` marker, and still counts, when its upstream body
//! did not change in 2.6.0. One whose body did change is stale until its card re-ports it
//! ` @ v2.6.0.`.
//!
//! The upstream tests whose body changed are committed as data
//! ([`CHANGED_LIST`]), since a checkout's submodule may lack the `v2.5.2` tag:
//! `cargo xtask upstream-changed` writes the list from the submodule's two tags, and
//! `--check` checks it (a missing tag skips with a message). A body runs from its macro at the
//! start of a line to the next such macro, trailing spaces and tabs ignored, as in
//! docs/ocio-2.6-plan.md's table.
//!
//! `cargo xtask parity` warns about the stale markers and lists them in docs/parity.md;
//! `cargo xtask ci --no-stale` (the release's step, docs/release-checklist.md) fails while
//! any remain. The ratchet counts them as before.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use ocio_testkit::paths;

/// The committed list of the upstream tests whose body differs between [`OLD_TAG`] and
/// [`NEW_TAG`], relative to the workspace root: one `<suite> <id>` a line (`cpu
/// OpOptimizers/combine_ops`), sorted, after `#` comment lines.
pub(crate) const CHANGED_LIST: &str = "xtask/data/upstream-tests-changed-2.6.0.txt";

/// The release the stale markers name.
const OLD_TAG: &str = "v2.5.2";
/// The pinned release.
const NEW_TAG: &str = "v2.6.0";

/// The marker version of a test ported before 2.6.0.
pub(crate) const OLD_MARKER: &str = "v2.5.2";

/// An upstream test: (suite, id), as `parity` names them (`cpu`, `OpOptimizers/combine_ops`).
pub(crate) type TestId = (String, String);

/// The test a macro line declares, and where its body starts: `OCIO_ADD_TEST(Group, name)`,
/// `OCIO_ADD_GPU_TEST(Group, name)` or `OCIO_ADD_TEST_<SIMD>(name)`, at the start of the line.
fn macro_at_line_start(line: &str) -> Option<TestId> {
    let rest = line.strip_prefix("OCIO_ADD_")?;
    let (suite, rest, simd) = if let Some(r) = rest.strip_prefix("TEST(") {
        ("cpu", r, None)
    } else if let Some(r) = rest.strip_prefix("GPU_TEST(") {
        ("gpu", r, None)
    } else {
        let r = rest.strip_prefix("TEST_")?;
        let open = r.find('(')?;
        let group = &r[..open];
        if group.is_empty()
            || !group
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            return None;
        }
        ("cpu", &r[open + 1..], Some(group))
    };
    let args = &rest[..rest.find(')')?];
    let args: Vec<&str> = args.split(',').map(str::trim).collect();
    let id = match simd {
        Some(group) => format!("{group}/{}", args[0]),
        None if args.len() >= 2 => format!("{}/{}", args[0], args[1]),
        None => return None,
    };
    Some((suite.to_string(), id))
}

/// Each test of one upstream test file's `text` and its body: from its macro line to the next
/// macro line, each line without its trailing spaces and tabs, the whole trimmed.
pub(crate) fn bodies(text: &str) -> BTreeMap<TestId, String> {
    let mut out = BTreeMap::new();
    let mut current: Option<(TestId, Vec<&str>)> = None;
    let mut finish = |current: Option<(TestId, Vec<&str>)>| {
        if let Some((id, lines)) = current {
            let body: Vec<&str> = lines
                .iter()
                .map(|l| l.trim_end_matches([' ', '\t']))
                .collect();
            out.insert(id, body.join("\n").trim().to_string());
        }
    };
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(id) = macro_at_line_start(line) {
            finish(current.take());
            current = Some((id, vec![line]));
        } else if let Some((_, lines)) = current.as_mut() {
            lines.push(line);
        }
    }
    finish(current);
    out
}

/// `git -C upstream/OpenColorIO <args>`, its stdout; `None` when git fails.
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(paths::upstream_dir())
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Every test of `tests/cpu` and `tests/gpu` at `tag` and its body; `None` when the submodule
/// doesn't have the tag.
fn tests_at(tag: &str) -> Option<BTreeMap<TestId, String>> {
    git(&[
        "rev-parse",
        "--verify",
        "--quiet",
        &format!("{tag}^{{commit}}"),
    ])?;
    let files = git(&[
        "ls-tree",
        "-r",
        "--name-only",
        tag,
        "tests/cpu",
        "tests/gpu",
    ])?;
    let mut out = BTreeMap::new();
    for file in files.lines().filter(|f| f.ends_with(".cpp")) {
        let text = git(&["show", &format!("{tag}:{file}")])?;
        out.extend(bodies(&text));
    }
    Some(out)
}

/// The tests whose body differs between two releases' tests (new and removed ones aside).
pub(crate) fn changed(
    old: &BTreeMap<TestId, String>,
    new: &BTreeMap<TestId, String>,
) -> BTreeSet<TestId> {
    new.iter()
        .filter(|(id, body)| old.get(*id).is_some_and(|b| b != *body))
        .map(|(id, _)| id.clone())
        .collect()
}

/// The text of [`CHANGED_LIST`] for `changed`.
fn list_text(changed: &BTreeSet<TestId>) -> String {
    let mut text = format!(
        "# Upstream tests whose body differs between {OLD_TAG} and {NEW_TAG} (trailing spaces\n\
         # ignored): `cargo xtask upstream-changed` writes this file, `--check` checks it.\n"
    );
    for (suite, id) in changed {
        text.push_str(&format!("{suite} {id}\n"));
    }
    text
}

/// Parses [`CHANGED_LIST`]'s text.
pub(crate) fn parse_list(text: &str) -> Result<BTreeSet<TestId>, String> {
    let mut out = BTreeSet::new();
    for line in text.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let (suite, id) = line
            .split_once(' ')
            .ok_or_else(|| format!("{CHANGED_LIST}: bad line `{line}`"))?;
        if !matches!(suite, "cpu" | "gpu") || id.is_empty() || id.contains(' ') {
            return Err(format!("{CHANGED_LIST}: bad line `{line}`"));
        }
        out.insert((suite.to_string(), id.to_string()));
    }
    if out.is_empty() {
        return Err(format!("{CHANGED_LIST} lists no test"));
    }
    Ok(out)
}

/// The committed list of changed tests.
pub(crate) fn changed_list() -> Result<BTreeSet<TestId>, String> {
    let path = paths::workspace_root().join(CHANGED_LIST);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{CHANGED_LIST}: {e}"))?;
    parse_list(&text.replace("\r\n", "\n"))
}

/// `cargo xtask upstream-changed [--check]`.
pub(crate) fn upstream_changed(check: bool) -> Result<(), String> {
    let (Some(old), Some(new)) = (tests_at(OLD_TAG), tests_at(NEW_TAG)) else {
        let message = format!(
            "upstream-changed: the submodule lacks the tag {OLD_TAG} or {NEW_TAG}; {CHANGED_LIST} \
             is used as committed"
        );
        if check {
            println!("{message}");
            return Ok(());
        }
        return Err(message);
    };
    let text = list_text(&changed(&old, &new));
    let path = paths::workspace_root().join(CHANGED_LIST);
    if check {
        let current = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        if current != text {
            return Err(format!(
                "{CHANGED_LIST} is stale; run `cargo xtask upstream-changed`"
            ));
        }
        println!("upstream-changed: {CHANGED_LIST} is current");
        return Ok(());
    }
    std::fs::create_dir_all(path.parent().expect("xtask/data")).map_err(|e| e.to_string())?;
    std::fs::write(&path, &text).map_err(|e| e.to_string())?;
    println!(
        "upstream-changed: {} tests written to {CHANGED_LIST}",
        text.lines().filter(|l| !l.starts_with('#')).count()
    );
    Ok(())
}

/// The stale markers among `markers` ((suite, id, version) -> files): each `@ v2.5.2` marker
/// of a test in `changed`, with the files that hold it.
pub(crate) fn stale<'a>(
    markers: &'a crate::parity::VersionedMarkers,
    changed: &BTreeSet<TestId>,
) -> Vec<(&'a str, &'a str, &'a [String])> {
    markers
        .iter()
        .filter(|((suite, id, version), _)| {
            *version == OLD_MARKER && changed.contains(&(suite.to_string(), id.clone()))
        })
        .map(|((suite, id, _), files)| (*suite, id.as_str(), files.as_slice()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies_run_to_the_next_macro_at_a_line_start() {
        let text = "// header\nOCIO_ADD_TEST(A, b)\n{\n    x;  \n}\n\n\
                    OCIO_ADD_GPU_TEST(C, d)\r\n{\r\n  // OCIO_ADD_TEST(E, f) in a comment\r\n}\r\n\
                    OCIO_ADD_TEST_AVX2(g)\n{}\n  OCIO_ADD_TEST(H, i) indented\n";
        let b = bodies(text);
        let keys: Vec<_> = b.keys().cloned().collect();
        assert_eq!(
            keys,
            [
                ("cpu".to_string(), "A/b".to_string()),
                ("cpu".to_string(), "AVX2/g".to_string()),
                ("gpu".to_string(), "C/d".to_string()),
            ]
        );
        assert_eq!(b[&keys[0]], "OCIO_ADD_TEST(A, b)\n{\n    x;\n}");
        assert_eq!(
            b[&keys[1]],
            "OCIO_ADD_TEST_AVX2(g)\n{}\n  OCIO_ADD_TEST(H, i) indented"
        );
        assert_eq!(
            b[&keys[2]],
            "OCIO_ADD_GPU_TEST(C, d)\n{\n  // OCIO_ADD_TEST(E, f) in a comment\n}"
        );
    }

    #[test]
    fn only_changed_bodies_of_tests_in_both_count() {
        let old = bodies(
            "OCIO_ADD_TEST(A, same)\n{}\nOCIO_ADD_TEST(A, edit)\n{ 1; }\nOCIO_ADD_TEST(A, gone)\n{}\n",
        );
        let new = bodies(
            "OCIO_ADD_TEST(A, same)\n{}   \nOCIO_ADD_TEST(A, edit)\n{ 2; }\nOCIO_ADD_TEST(A, new)\n{}\n",
        );
        let c = changed(&old, &new);
        assert_eq!(
            c.into_iter().collect::<Vec<_>>(),
            [("cpu".to_string(), "A/edit".to_string())]
        );
    }

    #[test]
    fn the_list_round_trips_and_refuses_bad_lines() {
        let set: BTreeSet<TestId> = [
            ("cpu".to_string(), "A/b".to_string()),
            ("gpu".to_string(), "C/d".to_string()),
        ]
        .into();
        assert_eq!(parse_list(&list_text(&set)).unwrap(), set);
        assert!(parse_list("# only a comment\n").is_err());
        assert!(parse_list("cpu\n").is_err());
        assert!(parse_list("xyz A/b\n").is_err());
    }

    #[test]
    fn only_old_markers_of_changed_tests_are_stale() {
        let mut markers = BTreeMap::new();
        let f = |s: &str| vec![s.to_string()];
        markers.insert(("cpu", "A/edit".to_string(), "v2.5.2"), f("a.rs"));
        markers.insert(("cpu", "A/edit2".to_string(), "v2.6.0"), f("b.rs"));
        markers.insert(("cpu", "A/same".to_string(), "v2.5.2"), f("c.rs"));
        markers.insert(("gpu", "A/edit".to_string(), "v2.5.2"), f("d.rs"));
        let changed: BTreeSet<TestId> = [
            ("cpu".to_string(), "A/edit".to_string()),
            ("cpu".to_string(), "A/edit2".to_string()),
        ]
        .into();
        let s = stale(&markers, &changed);
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].0, s[0].1, s[0].2), ("cpu", "A/edit", &f("a.rs")[..]));
    }

    /// The committed list is the submodule's (skipped, with a message, without both tags).
    #[test]
    fn the_committed_list_is_current() {
        upstream_changed(true).unwrap();
        assert!(changed_list().unwrap().len() > 100);
    }
}
