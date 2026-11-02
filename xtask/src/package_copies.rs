// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! What the published crates carry so that each package stands alone (PLAN.md D6): copies of
//! `LICENSE` and `NOTICE`, the sections of `THIRD-PARTY-NOTICES.md` its sources need, and
//! upstream's built-in configs. A package can't reach outside its directory, so these are
//! committed copies, and `cargo xtask guards` keeps them equal to what they copy.

use std::collections::BTreeSet;
use std::path::Path;

/// The directories of the crates published on crates.io; every other member is `publish = false`.
const PUBLISHED: [&str; 4] = [
    "crates/ocio",
    "crates/ocio-formats",
    "crates/ocio-gpu",
    "crates/ocio-ops",
];
/// Upstream's built-in configs, and the copies the library embeds.
const CONFIGS_UPSTREAM: &str = "upstream/OpenColorIO/src/OpenColorIO/builtinconfigs/configs";
const CONFIGS_COPY: &str = "crates/ocio/src/builtinconfigs/configs";
/// The file that lists every third-party source, and each crate's extract of it.
const NOTICES: &str = "THIRD-PARTY-NOTICES.md";

/// The problems of the checkout at `root`, given its members' manifests (path, text).
pub(crate) fn check(root: &Path, manifests: &[(String, String)]) -> Vec<String> {
    let mut problems = check_published(manifests);
    let read = |rel: &str| std::fs::read(root.join(rel)).ok();
    for dir in PUBLISHED {
        for file in ["LICENSE", "NOTICE"] {
            let copy = format!("{dir}/{file}");
            if read(&copy).is_none_or(|copy| Some(copy) != read(file)) {
                problems.push(format!("{copy}: not a copy of {file}"));
            }
        }
        let text = |rel: &str| read(rel).map(|t| String::from_utf8_lossy(&t).into_owned());
        let notices = format!("{dir}/{NOTICES}");
        problems.extend(check_notices(
            dir,
            text(&notices).as_deref(),
            &text(NOTICES).unwrap_or_default(),
        ));
    }
    let names = |dir: &str| -> BTreeSet<String> {
        std::fs::read_dir(root.join(dir))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect()
    };
    problems.extend(check_configs(
        &names(CONFIGS_UPSTREAM),
        &names(CONFIGS_COPY),
        &|name| read(&format!("{CONFIGS_UPSTREAM}/{name}")),
        &|name| read(&format!("{CONFIGS_COPY}/{name}")),
    ));
    problems
}

/// `publish = true` in exactly the [`PUBLISHED`] manifests, and each of them leaves out its
/// tests.
fn check_published(manifests: &[(String, String)]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut published = BTreeSet::new();
    for (rel, text) in manifests {
        let Some(dir) = rel.strip_suffix("/Cargo.toml") else {
            continue;
        };
        let Ok(doc) = text.parse::<toml::Table>() else {
            // check_dependency_pins reports it.
            continue;
        };
        let package = doc.get("package");
        if package
            .and_then(|p| p.get("publish"))
            .and_then(|p| p.as_bool())
            != Some(true)
        {
            continue;
        }
        published.insert(dir);
        let excludes = |pattern: &str| {
            package
                .and_then(|p| p.get("exclude"))
                .and_then(|e| e.as_array())
                .is_some_and(|e| e.iter().any(|v| v.as_str() == Some(pattern)))
        };
        for pattern in ["/tests/", "*_tests.rs"] {
            if !excludes(pattern) {
                problems.push(format!("{rel}: a published crate excludes `{pattern}`"));
            }
        }
    }
    if published != BTreeSet::from(PUBLISHED) {
        problems.push(format!(
            "`publish = true` must be exactly {PUBLISHED:?}, is {published:?}"
        ));
    }
    problems
}

/// The `## ` sections of a notices file, trimmed, by heading. A `## ` inside a code block is
/// license text.
fn sections(text: &str) -> Vec<(&str, &str)> {
    let mut starts = Vec::new();
    let mut fenced = false;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.starts_with("```") {
            fenced = !fenced;
        } else if !fenced && line.starts_with("## ") {
            starts.push(offset);
        }
        offset += line.len();
    }
    starts.push(text.len());
    starts
        .windows(2)
        .map(|w| {
            let section = text[w[0]..w[1]].trim_end();
            (section.lines().next().unwrap_or(""), section)
        })
        .collect()
}

/// A crate's extract (`extract`, when it has one) of the notices file `all`: the sections that
/// name a path of the crate, each one verbatim, and no others.
fn check_notices(dir: &str, extract: Option<&str>, all: &str) -> Vec<String> {
    let path = format!("{dir}/");
    let needed: Vec<_> = sections(all)
        .into_iter()
        .filter(|(_, section)| section.contains(&path))
        .collect();
    let found = sections(extract.unwrap_or(""));
    let rel = format!("{dir}/{NOTICES}");
    let mut problems = Vec::new();
    for (heading, section) in &needed {
        match found.iter().find(|(h, _)| h == heading) {
            Some((_, copy)) if copy == section => {}
            Some(_) => problems.push(format!("{rel}: `{heading}` differs from {NOTICES}'s")),
            None => problems.push(format!("{rel}: `{heading}` is missing")),
        }
    }
    for (heading, _) in &found {
        if !needed.iter().any(|(h, _)| h == heading) {
            problems.push(format!("{rel}: `{heading}` names no path of this crate"));
        }
    }
    problems
}

/// `bytes` with each CR LF as LF: the library reads its config files so, and a checkout of the
/// submodule may have either.
fn lf(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for (i, &byte) in bytes.iter().enumerate() {
        if byte != b'\r' || bytes.get(i + 1) != Some(&b'\n') {
            out.push(byte);
        }
    }
    out
}

/// The same config files in both directories, with the same text.
fn check_configs(
    upstream: &BTreeSet<String>,
    copies: &BTreeSet<String>,
    read_upstream: &dyn Fn(&str) -> Option<Vec<u8>>,
    read_copy: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Vec<String> {
    if upstream.is_empty() || upstream != copies {
        return vec![format!(
            "{CONFIGS_COPY}: must hold the files of {CONFIGS_UPSTREAM}: {copies:?} and {upstream:?}"
        )];
    }
    upstream
        .iter()
        .filter(|name| {
            read_upstream(name).map(|t| lf(&t)) != read_copy(name).map(|t| lf(&t))
                || read_copy(name).is_none()
        })
        .map(|name| format!("{CONFIGS_COPY}/{name}: not a copy of upstream's"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(publish: &str, exclude: &str) -> String {
        format!("[package]\nname = \"a\"\n{publish}\n{exclude}\n")
    }

    const EXCLUDE: &str = "exclude = [\"/tests/\", \"*_tests.rs\"]";

    fn published() -> Vec<(String, String)> {
        let mut files = vec![("Cargo.toml".to_string(), "[workspace]\n".to_string())];
        files.extend(PUBLISHED.map(|dir| {
            (
                format!("{dir}/Cargo.toml"),
                manifest("publish = true", EXCLUDE),
            )
        }));
        files.push((
            "crates/ocio-tools/Cargo.toml".to_string(),
            manifest("publish.workspace = true", ""),
        ));
        files
    }

    #[test]
    fn exactly_the_four_crates_are_published() {
        assert_eq!(check_published(&published()), Vec::<String>::new());
        // One more, one fewer.
        let mut files = published();
        files[5].1 = manifest("publish = true", EXCLUDE);
        assert_eq!(check_published(&files).len(), 1, "{files:?}");
        let mut files = published();
        files[1].1 = manifest("publish = false", EXCLUDE);
        assert_eq!(check_published(&files).len(), 1, "{files:?}");
        // `publish.workspace` is the workspace's `false`, not a published crate.
        let mut files = published();
        files[1].1 = manifest("publish.workspace = true", EXCLUDE);
        assert_eq!(check_published(&files).len(), 1, "{files:?}");
    }

    #[test]
    fn a_published_crate_leaves_out_its_tests() {
        for exclude in ["", "exclude = [\"/tests/\"]", "exclude = [\"*_tests.rs\"]"] {
            let mut files = published();
            files[1].1 = manifest("publish = true", exclude);
            assert!(!check_published(&files).is_empty(), "{exclude}");
        }
    }

    const ALL: &str = "# Notices\n\nintro crates/a/\n\n## One\n\nin `crates/a/x.rs`\n\n```text\n\
                       ## license heading\n```\n\n## Two\n\nin `crates/a-b/y.rs`\n\n\
                       ## Three\n\nin `crates/b/z.rs`\n";

    #[test]
    fn sections_stop_at_headings_outside_code_blocks() {
        let found: Vec<&str> = sections(ALL).iter().map(|(h, _)| *h).collect();
        assert_eq!(found, ["## One", "## Two", "## Three"]);
        assert!(sections(ALL)[0].1.ends_with("```"));
    }

    #[test]
    fn an_extract_holds_the_crates_sections_verbatim() {
        let two = "## Two\n\nin `crates/a-b/y.rs`\n";
        let three = "## Three\n\nin `crates/b/z.rs`\n";
        let one = "## One\n\nin `crates/a/x.rs`\n\n```text\n## license heading\n```\n";
        assert!(check_notices("crates/a-b", Some(two), ALL).is_empty());
        assert!(check_notices("crates/a", Some(&format!("# A\n\n{one}")), ALL).is_empty());
        // A path of the crate, not of a crate whose name starts the same.
        assert_eq!(check_notices("crates/a", Some(two), ALL).len(), 2);
        // Nothing needed, nothing there.
        assert!(check_notices("crates/c", None, ALL).is_empty());
        // Missing, altered, or an extract with a section it doesn't need.
        assert_eq!(check_notices("crates/b", None, ALL).len(), 1);
        let altered = three.replace("z.rs", "z.rs!");
        assert_eq!(check_notices("crates/b", Some(&altered), ALL).len(), 1);
        assert_eq!(
            check_notices("crates/b", Some(&format!("{three}\n{two}")), ALL).len(),
            1
        );
    }

    #[test]
    fn config_copies_match_whatever_the_line_ends() {
        let names = BTreeSet::from(["a.ocio".to_string()]);
        let text = |t: &'static [u8]| move |_: &str| Some(t.to_vec());
        let run = |up: &'static [u8], copy: &'static [u8]| {
            check_configs(&names, &names, &text(up), &text(copy)).len()
        };
        assert_eq!(run(b"a\nb\n", b"a\nb\n"), 0);
        assert_eq!(run(b"a\r\nb\r\n", b"a\nb\n"), 0);
        assert_eq!(run(b"a\nb\n", b"a\nc\n"), 1);
        assert_eq!(run(b"a\nb\n", b"a\nb"), 1);
        // A CR alone is text.
        assert_eq!(run(b"a\rb\n", b"ab\n"), 1);
        // A copy that can't be read, a file on one side only, nothing at all.
        let none = |_: &str| None;
        assert_eq!(check_configs(&names, &names, &none, &none).len(), 1);
        let other = BTreeSet::from(["b.ocio".to_string()]);
        assert_eq!(check_configs(&names, &other, &none, &none).len(), 1);
        let empty = BTreeSet::new();
        assert_eq!(check_configs(&empty, &empty, &none, &none).len(), 1);
    }

    #[test]
    fn this_checkout_passes() {
        let root = crate::root();
        let read = |rel: &str| {
            let text = std::fs::read_to_string(root.join(rel)).unwrap();
            (rel.to_string(), text)
        };
        let mut manifests = vec![read("Cargo.toml")];
        manifests.extend(PUBLISHED.map(|dir| read(&format!("{dir}/Cargo.toml"))));
        assert_eq!(check(&root, &manifests), Vec::<String>::new());
    }
}
