// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! What CI may do with the corpora (`corpus/README.md`): the public ACES configs on both
//! platforms, from their release tags and without a token, and nothing else.

use std::collections::BTreeSet;

fn read(path: &str) -> String {
    std::fs::read_to_string(crate::root().join(path)).unwrap()
}

/// The directories of `aces/` in the committed manifest.
fn manifest_directories() -> BTreeSet<String> {
    read("corpus/manifest.sha256")
        .lines()
        .filter_map(|line| line.split_once("  aces/"))
        .map(|(_, path)| path.split('/').next().unwrap().to_string())
        .collect()
}

/// The text of one job of `ci.yml`.
fn job<'a>(ci: &'a str, name: &str) -> &'a str {
    let start = ci
        .find(&format!("\n  {name}:\n"))
        .unwrap_or_else(|| panic!("no job {name}"));
    let rest = &ci[start + name.len() + 5..];
    let end = rest
        .match_indices("\n  ")
        .find(|(at, _)| {
            let line = &rest[at + 3..];
            !line.starts_with(' ') && !line.starts_with('#') && !line.starts_with('\n')
        })
        .map_or(rest.len(), |(at, _)| at);
    &rest[..end]
}

#[test]
fn each_platform_runs_the_aces_corpus_from_the_manifest() {
    let ci = read(".github/workflows/ci.yml");
    for name in ["windows-checks", "rocky9-checks"] {
        let job = job(&ci, name);
        for text in [
            "uses: actions/cache@v4",
            "key: aces-${{ hashFiles('corpus/manifest.sha256') }}",
            "scripts/fetch-aces-corpus.sh \"$RUNNER_TEMP/ocio-corpus\"",
            "OCIO_RS_CORPUS: aces\n",
            "OCIO_RS_CORPUS_ROOT: ${{ runner.temp }}/ocio-corpus",
            "--test corpus_oracle public_corpora",
        ] {
            assert!(job.contains(text), "{name} doesn't hold `{text}`");
        }
        // Fetched before it runs.
        let fetch = job.find("scripts/fetch-aces-corpus.sh").unwrap();
        assert!(fetch < job.find("corpus_oracle").unwrap(), "{name}");
    }
}

#[test]
fn nothing_but_aces_reaches_ci() {
    let ci = read(".github/workflows/ci.yml");
    let script = read("scripts/fetch-aces-corpus.sh");
    assert!(!ci.contains("imageworks"));
    assert!(!ci.contains("OCIO_RS_STUDIO_CORPUS"));
    assert_eq!(ci.matches("OCIO_RS_CORPUS:").count(), 2);
    assert_eq!(ci.matches("OCIO_RS_CORPUS: aces\n").count(), 2);
    for word in [
        "imageworks",
        "STUDIO",
        "token:",
        "Authorization",
        "GITHUB_TOKEN",
    ] {
        assert!(!script.contains(word), "{word}");
    }
    // Only the aces lines of the manifest, and only the upstream ACES repository.
    assert!(script.contains("grep '  aces/' \"$here/corpus/manifest.sha256\""));
    assert!(script.contains(
        "https://github.com/AcademySoftwareFoundation/OpenColorIO-Config-ACES/releases/download"
    ));
}

/// The script's pinned tags are the manifest's directories, and it counts their files.
#[test]
fn the_tags_are_the_manifests_directories() {
    let script = read("scripts/fetch-aces-corpus.sh");
    let line = script
        .lines()
        .find(|line| line.trim_start().starts_with("v1.0.0 |"))
        .unwrap();
    let tags: BTreeSet<String> = line
        .trim()
        .split(')')
        .next()
        .unwrap()
        .split('|')
        .map(|tag| tag.trim().to_string())
        .collect();
    assert_eq!(tags, manifest_directories());
    let files = read("corpus/manifest.sha256")
        .lines()
        .filter(|line| line.contains("  aces/"))
        .count();
    assert_eq!(files, 37);
    assert!(script.contains("-ne 37"));
}
