// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The public real-world corpora (PLAN.md section 8, card p10-corpus): their committed manifest,
//! and what the environment asks of them.
//!
//! `corpus/manifest.sha256` lists every input file with its SHA-256, in `sha256sum`'s text form,
//! relative to the corpus root: `<hex>  <corpus>/<path>`. The files themselves are never
//! committed. [`Corpus::verify`] refuses a corpus root that doesn't hold exactly those bytes, so
//! a run names the inputs it tested.
//!
//! The environment:
//! - `OCIO_RS_CORPUS`: the public corpora to run, comma-separated (`aces,imageworks`). A corpus
//!   named here that the root doesn't hold, in full and with these hashes, fails the test.
//! - `OCIO_RS_CORPUS_ROOT`: the directory holding each corpus's directory.
//! - `OCIO_RS_STUDIO_CORPUS`: directories of private configs, separated as `PATH` is. It is
//!   read only by the studio sweep, and only an explicit local run sets it.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::paths::workspace_root;

/// The variable naming the public corpora to run.
pub const CORPUS_VAR: &str = "OCIO_RS_CORPUS";
/// The variable naming the directory that holds the public corpora.
pub const ROOT_VAR: &str = "OCIO_RS_CORPUS_ROOT";
/// The variable naming the directories of private configs.
pub const STUDIO_VAR: &str = "OCIO_RS_STUDIO_CORPUS";

/// The public corpora the manifest may name.
pub const CORPORA: [&str; 2] = ["aces", "imageworks"];

/// A file of the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its SHA-256, lower-case hex.
    pub sha256: String,
    /// Its path below the corpus root, with `/`.
    pub path: String,
}

/// The committed manifest's text, parsed; panics where it isn't in the exact form the module
/// describes (sorted by path, no duplicates, known corpora, no `..`).
pub fn parse_manifest(text: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let number = i + 1;
        let (sha256, path) = line
            .split_once("  ")
            .unwrap_or_else(|| panic!("manifest line {number}: not `<sha256>  <path>`"));
        assert!(
            sha256.len() == 64
                && sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "manifest line {number}: {sha256:?} is no SHA-256"
        );
        let first = path.split('/').next().unwrap_or_default();
        assert!(
            CORPORA.contains(&first) && path.split('/').all(|p| !p.is_empty() && p != ".."),
            "manifest line {number}: {path:?} is no path below a known corpus"
        );
        entries.push(Entry {
            sha256: sha256.to_string(),
            path: path.to_string(),
        });
    }
    assert!(
        entries.windows(2).all(|w| w[0].path < w[1].path),
        "the manifest must be sorted by path, without duplicates"
    );
    entries
}

/// The committed manifest.
pub fn manifest() -> Vec<Entry> {
    let path = workspace_root().join("corpus").join("manifest.sha256");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    parse_manifest(&text)
}

/// The SHA-256 of `bytes`, lower-case hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Every file below `dir`, recursively, skipping directories named `.git`.
fn files_below(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("reading {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("reading {}: {e}", dir.display()))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        if kind.is_dir() {
            if entry.file_name() != ".git" {
                files_below(&path, out)?;
            }
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// A public corpus, checked against the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corpus {
    /// The corpus's name, the first component of its manifest paths.
    pub name: String,
    /// Each config file's path below the root, with `/`, sorted.
    pub configs: Vec<String>,
    /// The directory holding the corpus's directory.
    pub root: PathBuf,
}

/// Whether a manifest path is a config file.
pub fn is_config(path: &str) -> bool {
    path.ends_with(".ocio") || path.ends_with(".ocioz")
}

impl Corpus {
    /// The corpus `name` below `root`, whose files must be exactly the manifest's: each one
    /// present with its hash, and none other. The error names what is wrong.
    pub fn verify(name: &str, root: &Path, entries: &[Entry]) -> Result<Corpus, String> {
        let prefix = format!("{name}/");
        let listed: Vec<&Entry> = entries
            .iter()
            .filter(|e| e.path.starts_with(&prefix))
            .collect();
        if listed.is_empty() {
            return Err(format!("the manifest has no corpus {name:?}"));
        }
        let top = root.join(name);
        if !top.is_dir() {
            return Err(format!(
                "corpus {name:?} was requested, and {} doesn't exist",
                top.display()
            ));
        }
        let mut found = Vec::new();
        files_below(&top, &mut found)?;
        let found: BTreeSet<String> = found
            .iter()
            .map(|p| {
                let relative = p.strip_prefix(root).expect("below the root");
                let parts: Vec<String> = relative
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                parts.join("/")
            })
            .collect();
        let expected: BTreeSet<String> = listed.iter().map(|e| e.path.clone()).collect();
        let missing: Vec<&String> = expected.difference(&found).collect();
        let extra: Vec<&String> = found.difference(&expected).collect();
        if !missing.is_empty() || !extra.is_empty() {
            return Err(format!(
                "corpus {name:?} in {} isn't the manifest's: {} files missing ({:?}...), {} not \
                 listed ({:?}...)",
                root.display(),
                missing.len(),
                missing.first(),
                extra.len(),
                extra.first()
            ));
        }
        for entry in &listed {
            let path = root.join(&entry.path);
            let bytes = fs::read(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
            let actual = sha256_hex(&bytes);
            if actual != entry.sha256 {
                return Err(format!(
                    "{}: SHA-256 {actual}, the manifest's {}",
                    entry.path, entry.sha256
                ));
            }
        }
        Ok(Corpus {
            name: name.to_string(),
            configs: listed
                .iter()
                .filter(|e| is_config(&e.path))
                .map(|e| e.path.clone())
                .collect(),
            root: root.to_path_buf(),
        })
    }
}

/// The public corpora `requested` (the value of [`CORPUS_VAR`]; `None` or empty for none) below
/// `root` (the value of [`ROOT_VAR`]). An error for a corpus the root doesn't hold exactly, for
/// a name the manifest doesn't know and for a request without a root.
pub fn requested(
    requested: Option<&str>,
    root: Option<&Path>,
    entries: &[Entry],
) -> Result<Vec<Corpus>, String> {
    let names: Vec<&str> = requested
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect();
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let root = root.ok_or_else(|| {
        format!("{CORPUS_VAR} asks for {names:?}, and {ROOT_VAR} doesn't name a directory")
    })?;
    names
        .iter()
        .map(|name| Corpus::verify(name, root, entries))
        .collect()
}

/// [`requested`], from the environment.
pub fn requested_from_env() -> Result<Vec<Corpus>, String> {
    let names = std::env::var(CORPUS_VAR).ok();
    let root = std::env::var_os(ROOT_VAR).map(PathBuf::from);
    requested(names.as_deref(), root.as_deref(), &manifest())
}

/// The private configs below the directories [`STUDIO_VAR`] names, each file once by content
/// (the first path, in sorted order), or nothing when it isn't set. An error for a directory
/// that isn't one: a sweep that was asked for must not pass empty.
pub fn studio_from_env() -> Result<Vec<PathBuf>, String> {
    let Some(value) = std::env::var_os(STUDIO_VAR) else {
        return Ok(Vec::new());
    };
    let mut files = Vec::new();
    for dir in std::env::split_paths(&value) {
        if !dir.is_dir() {
            return Err(format!(
                "{STUDIO_VAR} names something that isn't a directory"
            ));
        }
        // Not the error's text: it names the path.
        files_below(&dir, &mut files)
            .map_err(|_| format!("{STUDIO_VAR} names a directory that can't be read"))?;
    }
    files.retain(|p| p.extension().is_some_and(|e| e == "ocio" || e == "ocioz"));
    files.sort();
    let mut seen = BTreeSet::new();
    files.retain(|p| {
        fs::read(p)
            .map(|bytes| seen.insert(sha256_hex(&bytes)))
            .unwrap_or(true)
    });
    if files.is_empty() {
        return Err(format!("{STUDIO_VAR} names directories without a config"));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch corpus root below the target directory, new for `name`.
    fn scratch(name: &str) -> PathBuf {
        let dir = crate::paths::target_dir().join("corpus-tests").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("aces")).unwrap();
        fs::write(dir.join("aces").join("a.ocio"), b"abc").unwrap();
        dir
    }

    fn entries() -> Vec<Entry> {
        // SHA-256 of "abc" (FIPS 180-2, appendix B.1).
        let sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        parse_manifest(&format!("{sha256}  aces/a.ocio\n"))
    }

    #[test]
    fn a_corpus_with_the_manifests_bytes_verifies() {
        let root = scratch("good");
        let corpus = Corpus::verify("aces", &root, &entries()).unwrap();
        assert_eq!(corpus.configs, ["aces/a.ocio"]);
    }

    #[test]
    fn a_requested_corpus_that_is_absent_fails() {
        let root = scratch("absent");
        let error = requested(Some("imageworks"), Some(&root), &entries()).unwrap_err();
        assert!(error.contains("no corpus"), "{error}");
        fs::remove_dir_all(root.join("aces")).unwrap();
        let error = requested(Some("aces"), Some(&root), &entries()).unwrap_err();
        assert!(error.contains("was requested"), "{error}");
        let error = requested(Some("aces"), None, &entries()).unwrap_err();
        assert!(error.contains(ROOT_VAR), "{error}");
    }

    #[test]
    fn nothing_requested_is_no_corpus() {
        assert_eq!(requested(None, None, &entries()), Ok(Vec::new()));
        assert_eq!(requested(Some(" , "), None, &entries()), Ok(Vec::new()));
    }

    #[test]
    fn changed_missing_and_extra_files_fail() {
        let root = scratch("changed");
        fs::write(root.join("aces").join("a.ocio"), b"abd").unwrap();
        let error = Corpus::verify("aces", &root, &entries()).unwrap_err();
        assert!(error.contains("SHA-256"), "{error}");

        let root = scratch("extra");
        fs::write(root.join("aces").join("b.ocio"), b"abc").unwrap();
        let error = Corpus::verify("aces", &root, &entries()).unwrap_err();
        assert!(error.contains("1 not listed"), "{error}");

        let root = scratch("missing");
        fs::remove_file(root.join("aces").join("a.ocio")).unwrap();
        let error = Corpus::verify("aces", &root, &entries()).unwrap_err();
        assert!(error.contains("1 files missing"), "{error}");
    }

    #[test]
    fn the_manifest_form_is_exact() {
        let sha = "a".repeat(64);
        for bad in [
            format!("{sha} aces/a.ocio\n"),
            format!("{sha}  other/a.ocio\n"),
            format!("{sha}  aces/../a.ocio\n"),
            format!("{}  aces/a.ocio\n", "A".repeat(64)),
            format!("{sha}  aces/b.ocio\n{sha}  aces/a.ocio\n"),
            format!("{sha}  aces/a.ocio\n{sha}  aces/a.ocio\n"),
        ] {
            assert!(
                std::panic::catch_unwind(|| parse_manifest(&bad)).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn the_committed_manifest_lists_every_corpus() {
        let entries = manifest();
        for name in CORPORA {
            let configs = entries
                .iter()
                .filter(|e| e.path.starts_with(&format!("{name}/")) && is_config(&e.path))
                .count();
            assert!(configs > 0, "{name}");
        }
    }
}
