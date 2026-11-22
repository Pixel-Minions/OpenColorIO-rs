// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Pass records: what `cargo xtask gate` tested on a git tree, so that `cargo xtask land` need
//! not test the same tree again (owner, 2026-11-22).
//!
//! A top-level gate (not the nested Rocky pass) that passes every step records the pass, keyed
//! by the git tree it tested, in `<git common dir>/ocio-rs-gate-passes/<tree>.toml`, where every
//! worktree of the repository sees it. It records only a tree that is exactly what it tested:
//! HEAD's (with `--staged`, the temporary commit's) with nothing uncommitted or untracked, from
//! the gate's start to its end. A record holds what the tests covered and what the tree doesn't
//! pin: the compiler (`rustc -vV`'s version line) and this machine's CPU
//! ([`ocio_testkit::oracle::machine_description`], which also keys the oracle's cache). One file
//! holds every pass of its tree, as `[[pass]]` tables, oldest first.
//!
//! Writing a record never fails a gate: it warns instead.

use std::path::{Path, PathBuf};

/// The records' directory under the git common directory.
pub(crate) const DIR: &str = "ocio-rs-gate-passes";

/// The files `cargo xtask land` regenerates into its merge commit: a merge tree that differs
/// from the branch tip's only in these is the tree the branch's gate tested.
pub(crate) const GENERATED: [&str; 2] = ["docs/parity.md", "docs/ratchet.toml"];

/// Which packages' test steps ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Tested {
    /// No test step.
    Nothing,
    /// The whole workspace (`cargo test --workspace`, and the Python steps).
    Workspace,
    /// These packages only (`--crates`, `--touched`).
    Packages(Vec<String>),
}

impl Tested {
    /// Whether test steps that covered `self` cover `want` too.
    pub(crate) fn covers(&self, want: &Tested) -> bool {
        match (self, want) {
            (_, Tested::Nothing) | (Tested::Workspace, _) => true,
            (Tested::Packages(have), Tested::Packages(want)) => {
                want.iter().all(|p| have.contains(p))
            }
            (Tested::Nothing | Tested::Packages(_), Tested::Workspace) => false,
            (Tested::Nothing, Tested::Packages(_)) => false,
        }
    }

    pub(crate) fn describe(&self) -> String {
        match self {
            Tested::Nothing => "no test step".into(),
            Tested::Workspace => "the whole workspace".into(),
            Tested::Packages(names) => names.join(", "),
        }
    }
}

/// One passing gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Record {
    /// The git tree it tested.
    pub(crate) tree: String,
    /// The commit whose tree that is (with `--staged`, the temporary commit).
    pub(crate) commit: String,
    /// When it passed (`20261122T051209Z`, UTC).
    pub(crate) time: String,
    /// Its step logs.
    pub(crate) log_dir: String,
    /// The full test tier (else the quick one).
    pub(crate) full: bool,
    pub(crate) release: bool,
    pub(crate) rocky: bool,
    pub(crate) main: bool,
    pub(crate) tested: Tested,
    /// `rustc -vV`'s version line.
    pub(crate) rustc: String,
    /// [`ocio_testkit::oracle::machine_description`].
    pub(crate) machine: String,
}

impl Record {
    fn to_table(&self) -> toml::Table {
        let mut t = toml::Table::new();
        let mut put = |k: &str, v: toml::Value| {
            t.insert(k.into(), v);
        };
        put("tree", self.tree.clone().into());
        put("commit", self.commit.clone().into());
        put("time", self.time.clone().into());
        put("log_dir", self.log_dir.clone().into());
        put("tier", if self.full { "full" } else { "quick" }.into());
        put("release", self.release.into());
        put("rocky", self.rocky.into());
        put("main", self.main.into());
        put(
            "tested",
            match &self.tested {
                Tested::Nothing => "nothing".into(),
                Tested::Workspace => "workspace".into(),
                Tested::Packages(names) => toml::Value::Array(
                    names
                        .iter()
                        .map(|n| toml::Value::from(n.as_str()))
                        .collect(),
                ),
            },
        );
        put("rustc", self.rustc.clone().into());
        put("machine", self.machine.clone().into());
        t
    }

    fn from_table(t: &toml::Table) -> Result<Record, String> {
        let text = |k: &str| -> Result<String, String> {
            t.get(k)
                .and_then(|v| v.as_str())
                .map(String::from)
                .ok_or_else(|| format!("a pass without `{k}`"))
        };
        let flag = |k: &str| -> Result<bool, String> {
            t.get(k)
                .and_then(|v| v.as_bool())
                .ok_or_else(|| format!("a pass without `{k}`"))
        };
        let full = match text("tier")?.as_str() {
            "full" => true,
            "quick" => false,
            other => return Err(format!("a pass of tier `{other}`")),
        };
        let tested = match t.get("tested") {
            Some(toml::Value::String(s)) if s == "nothing" => Tested::Nothing,
            Some(toml::Value::String(s)) if s == "workspace" => Tested::Workspace,
            Some(toml::Value::Array(names)) => Tested::Packages(
                names
                    .iter()
                    .map(|n| {
                        n.as_str()
                            .map(String::from)
                            .ok_or("`tested` lists a non-string")
                    })
                    .collect::<Result<_, _>>()?,
            ),
            _ => return Err("a pass without a valid `tested`".into()),
        };
        Ok(Record {
            tree: text("tree")?,
            commit: text("commit")?,
            time: text("time")?,
            log_dir: text("log_dir")?,
            full,
            release: flag("release")?,
            rocky: flag("rocky")?,
            main: flag("main")?,
            tested,
            rustc: text("rustc")?,
            machine: text("machine")?,
        })
    }
}

/// A records file: every pass of one tree, as `[[pass]]` tables.
pub(crate) fn render(records: &[Record]) -> String {
    let mut doc = toml::Table::new();
    doc.insert(
        "pass".into(),
        toml::Value::Array(
            records
                .iter()
                .map(|r| toml::Value::Table(r.to_table()))
                .collect(),
        ),
    );
    toml::to_string(&doc).unwrap_or_default()
}

/// The records of a records file ([`render`]).
pub(crate) fn parse(text: &str) -> Result<Vec<Record>, String> {
    let doc: toml::Table = text.parse().map_err(|e| format!("{e}"))?;
    match doc.get("pass") {
        None => Ok(Vec::new()),
        Some(toml::Value::Array(passes)) => passes
            .iter()
            .map(|p| {
                p.as_table()
                    .ok_or_else(|| "a `pass` that is not a table".to_string())
                    .and_then(Record::from_table)
            })
            .collect(),
        Some(_) => Err("`pass` is not an array of tables".into()),
    }
}

/// What a land would test on `tree`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Need {
    pub(crate) tree: String,
    pub(crate) tested: Tested,
    pub(crate) rocky: bool,
    pub(crate) rustc: String,
    pub(crate) machine: String,
}

/// Why `record` doesn't cover `need`; None when it does: the same tree, compiler and machine,
/// the full tier in debug and release, Rocky Linux 9 when the land runs it, and the same or a
/// larger set of packages. (`--main` isn't needed: the land runs `ci --main` and cargo-deny
/// itself.)
pub(crate) fn not_covered(record: &Record, need: &Need) -> Option<&'static str> {
    if record.tree != need.tree {
        Some("another tree")
    } else if !record.full {
        Some("the quick tier")
    } else if !record.release {
        Some("no release tests")
    } else if need.rocky && !record.rocky {
        Some("no Rocky Linux 9 pass")
    } else if record.rustc != need.rustc {
        Some("another rustc")
    } else if record.machine != need.machine {
        Some("another machine or CPU")
    } else if !record.tested.covers(&need.tested) {
        Some("fewer packages")
    } else {
        None
    }
}

/// The newest of `records` that covers `need`, or why none does (the newest record's reason).
pub(crate) fn covering<'a>(records: &'a [Record], need: &Need) -> Result<&'a Record, String> {
    let mut why = None;
    for record in records.iter().rev() {
        match not_covered(record, need) {
            None => return Ok(record),
            Some(reason) => {
                why.get_or_insert(reason);
            }
        }
    }
    Err(match why {
        Some(reason) => format!(
            "{} recorded pass(es) of this tree, none covering it (the newest: {reason})",
            records.len()
        ),
        None => "no recorded pass of this tree".into(),
    })
}

/// Whether two trees that differ in `changed` (`git diff --name-only`) differ only in the
/// generated files.
pub(crate) fn only_generated(changed: &[&str]) -> bool {
    changed.iter().all(|f| GENERATED.contains(f))
}

/// The tree and commit a gate tests, when it is exactly HEAD's: None when `status` (`git
/// status --porcelain`, every untracked file included) lists anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreeState {
    pub(crate) tree: String,
    pub(crate) commit: String,
}

pub(crate) fn clean_state(status: &str, tree: &str, commit: &str) -> Option<TreeState> {
    status.trim().is_empty().then(|| TreeState {
        tree: tree.trim().to_string(),
        commit: commit.trim().to_string(),
    })
}

/// The [`TreeState`] of the checkout at `root`: None with uncommitted changes or untracked
/// files.
pub(crate) fn tree_state(root: &Path) -> Result<Option<TreeState>, String> {
    let status = crate::git(root, crate::STATUS_ALL)?;
    let commit = crate::git(root, &["rev-parse", "--verify", "HEAD"])?;
    let tree = crate::git(root, &["rev-parse", "--verify", "HEAD^{tree}"])?;
    Ok(clean_state(&status, &tree, &commit))
}

/// `<git common dir>/ocio-rs-gate-passes` of the repository at `root`.
pub(crate) fn dir(root: &Path) -> Result<PathBuf, String> {
    let common = crate::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    Ok(PathBuf::from(common.trim()).join(DIR))
}

/// `rustc -vV`'s version line (`rustc 1.98.1 (...)`), for the toolchain `root` selects.
pub(crate) fn rustc_version(root: &Path) -> Result<String, String> {
    let out = std::process::Command::new("rustc")
        .arg("-vV")
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("could not run rustc -vV: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    match text.lines().next() {
        Some(line) if out.status.success() && line.starts_with("rustc ") => {
            Ok(line.trim().to_string())
        }
        _ => Err(format!("rustc -vV failed ({})", out.status)),
    }
}

/// The records of `tree` in `dir` (none when it has no file).
pub(crate) fn read(dir: &Path, tree: &str) -> Result<Vec<Record>, String> {
    let file = dir.join(format!("{tree}.toml"));
    match std::fs::read_to_string(&file) {
        Ok(text) => parse(&text).map_err(|e| format!("{}: {e}", file.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

/// Appends `record` to its tree's file in `dir`, under `dir/lock`; the file is replaced whole
/// (a new file renamed over it), so a reader never sees half of it. A file that doesn't parse
/// is replaced.
pub(crate) fn write(dir: &Path, record: &Record) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let lock = crate::land::lock_file(&dir.join("lock"))?;
    lock.lock()
        .map_err(|e| format!("{}: {e}", dir.join("lock").display()))?;
    let mut records = read(dir, &record.tree).unwrap_or_default();
    records.push(record.clone());
    let file = dir.join(format!("{}.toml", record.tree));
    let new = dir.join(format!("{}.toml.{}", record.tree, std::process::id()));
    std::fs::write(&new, render(&records)).map_err(|e| format!("{}: {e}", new.display()))?;
    std::fs::rename(&new, &file).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(file)
}

/// What [`record`] did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Recorded {
    Written(PathBuf),
    /// The tree had uncommitted changes or untracked files when the gate started or ended.
    Dirty,
    /// HEAD moved during the gate.
    Moved,
}

/// Records a pass of the checkout at `root` in `dir`, when it was `start` (its state when the
/// gate started) and still is; `fill` gives the record's other fields.
pub(crate) fn record(
    root: &Path,
    dir: &Path,
    start: Option<&TreeState>,
    fill: impl FnOnce(&TreeState) -> Result<Record, String>,
) -> Result<Recorded, String> {
    let Some(start) = start else {
        return Ok(Recorded::Dirty);
    };
    let Some(end) = tree_state(root)? else {
        return Ok(Recorded::Dirty);
    };
    if &end != start {
        return Ok(Recorded::Moved);
    }
    let record = fill(start)?;
    write(dir, &record).map(Recorded::Written)
}

#[cfg(test)]
#[path = "passes_tests.rs"]
mod tests;
