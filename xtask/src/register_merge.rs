// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Entry-by-entry merges of `docs/improvements.md`, and the rebuild `xtask land` checks a
//! landed register against.
//!
//! Git merges the register as text. Cards that run in parallel add entries at the same places
//! (the end of a section, the end of the file), and a line-based merge then attaches text to
//! the wrong entry: a fix that removes lines after one entry leaves them in place once another
//! card has appended an entry after them, and a union merge keeps both sides' lines. Here the
//! register is a list of sections, each a list of entries keyed by their id (`I-81`, `U-31`),
//! and a merge applies the other side's changes entry by entry:
//! - an entry only one side changed (added, edited, removed or moved to another section) takes
//!   that side's version;
//! - an entry both sides changed the same way is kept; changed differently, it is a conflict;
//! - an added entry goes into its section before the first entry of its series with a larger
//!   number (or after the last one of its series), so the order `xtask guards` checks holds;
//! - the text before the first section, and each section's text before its first entry, merge
//!   the same way, as one block each.
//!
//! `cargo xtask merge-register <base> <ours> <theirs>` is the git merge driver
//! (`.gitattributes`: `docs/improvements.md merge=ocio-register`); `xtask land` configures it
//! for its replays, and agents pass it to their own cherry-picks
//! (`git -c merge.ocio-register.driver="cargo xtask merge-register %O %A %B" cherry-pick ...`).
//! Without the driver configured, git falls back to its text merge, which stops on a conflict
//! instead of guessing.

use std::path::Path;

/// The register this module merges, relative to the workspace root.
pub(crate) const REGISTER: &str = "docs/improvements.md";

/// An entry's series and number: `('U', 31)` for `U-31`.
type Id = (char, u32);

/// One entry: its heading and the lines up to the next heading, without trailing blank lines.
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    id: Id,
    lines: Vec<String>,
}

/// One `## ` section: its heading, its text before the first entry (without trailing blank
/// lines), and its entries in file order.
#[derive(Debug, Clone, PartialEq)]
struct Section {
    heading: String,
    intro: Vec<String>,
    entries: Vec<Entry>,
}

/// `docs/improvements.md` as sections of entries.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Register {
    preamble: Vec<String>,
    sections: Vec<Section>,
}

fn without_blank_tail(mut lines: Vec<String>) -> Vec<String> {
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines
}

fn name(id: Id) -> String {
    format!("{}-{}", id.0, id.1)
}

impl Register {
    /// Reads a register in its usual form: one blank line before every heading, LF line
    /// endings, one final newline. Text in that form reads back exactly ([`Register::render`]);
    /// anything else is refused, so that a merge never rewrites what it doesn't understand.
    pub(crate) fn parse(text: &str) -> Result<Register, String> {
        if text.contains('\r') {
            return Err("it has carriage returns".into());
        }
        enum Owner {
            Preamble,
            Intro,
            Entry,
        }
        let mut preamble = Vec::new();
        let mut sections: Vec<Section> = Vec::new();
        let mut owner = Owner::Preamble;
        let mut in_fence = false;
        for line in text.split('\n') {
            let heading = !in_fence && line.starts_with('#');
            if line.trim_start().starts_with("```") {
                in_fence = !in_fence;
            }
            if heading && line.starts_with("## ") {
                sections.push(Section {
                    heading: line.to_string(),
                    intro: Vec::new(),
                    entries: Vec::new(),
                });
                owner = Owner::Intro;
                continue;
            }
            if heading && line.starts_with("### ") {
                let id = crate::registers::entry_id(line)
                    .ok_or_else(|| format!("`{line}` is not an entry heading"))?;
                let section = sections
                    .last_mut()
                    .ok_or_else(|| format!("`{line}` is outside every `## ` section"))?;
                section.entries.push(Entry {
                    id,
                    lines: vec![line.to_string()],
                });
                owner = Owner::Entry;
                continue;
            }
            let line = line.to_string();
            match owner {
                Owner::Preamble => preamble.push(line),
                Owner::Intro => sections.last_mut().expect("a section").intro.push(line),
                Owner::Entry => sections
                    .last_mut()
                    .and_then(|s| s.entries.last_mut())
                    .expect("an entry")
                    .lines
                    .push(line),
            }
        }
        let register = Register {
            preamble: without_blank_tail(preamble),
            sections: sections
                .into_iter()
                .map(|s| Section {
                    heading: s.heading,
                    intro: without_blank_tail(s.intro),
                    entries: s
                        .entries
                        .into_iter()
                        .map(|e| Entry {
                            id: e.id,
                            lines: without_blank_tail(e.lines),
                        })
                        .collect(),
                })
                .collect(),
        };
        let mut seen = std::collections::BTreeSet::new();
        for entry in register.sections.iter().flat_map(|s| &s.entries) {
            if !seen.insert(entry.id) {
                return Err(format!("{} appears twice", name(entry.id)));
            }
        }
        if register.render() != text {
            return Err(
                "it is not in the register's form (one blank line before every \
                        heading, no blank lines at the end of a block, one final newline)"
                    .into(),
            );
        }
        Ok(register)
    }

    /// The register's text.
    pub(crate) fn render(&self) -> String {
        let mut out: Vec<&str> = self.preamble.iter().map(String::as_str).collect();
        for section in &self.sections {
            out.push("");
            out.push(&section.heading);
            out.extend(section.intro.iter().map(String::as_str));
            for entry in &section.entries {
                out.push("");
                out.extend(entry.lines.iter().map(String::as_str));
            }
        }
        out.push("");
        out.join("\n")
    }

    /// Every entry with its section's heading.
    fn entries(&self) -> std::collections::BTreeMap<Id, (&str, &Entry)> {
        self.sections
            .iter()
            .flat_map(|s| {
                s.entries
                    .iter()
                    .map(move |e| (e.id, (s.heading.as_str(), e)))
            })
            .collect()
    }

    fn section_mut(&mut self, heading: &str) -> Option<&mut Section> {
        self.sections.iter_mut().find(|s| s.heading == heading)
    }

    fn remove(&mut self, id: Id) {
        for section in &mut self.sections {
            section.entries.retain(|e| e.id != id);
        }
    }

    /// Puts `entry` into the section `heading`, in its series' order: before the first entry
    /// of its series with a larger number, else after the last one of its series, else at the
    /// end. `false` without such a section.
    fn insert(&mut self, heading: &str, entry: Entry) -> bool {
        let Some(section) = self.section_mut(heading) else {
            return false;
        };
        let series = entry.id.0;
        let at = section
            .entries
            .iter()
            .position(|e| e.id.0 == series && e.id.1 > entry.id.1)
            .or_else(|| {
                section
                    .entries
                    .iter()
                    .rposition(|e| e.id.0 == series)
                    .map(|i| i + 1)
            })
            .unwrap_or(section.entries.len());
        section.entries.insert(at, entry);
        true
    }
}

/// What a three-way merge gives.
#[derive(Debug, PartialEq)]
pub(crate) enum Merged {
    /// The merged register.
    Clean(String),
    /// The merge with git's conflict markers around each conflicting block, and what conflicts.
    Conflict {
        text: String,
        conflicts: Vec<String>,
    },
}

/// `ours` and `theirs` side by side between git's conflict markers.
fn markers(ours: &[String], theirs: &[String]) -> Vec<String> {
    let mut out = vec!["<<<<<<< ours".to_string()];
    out.extend(ours.iter().cloned());
    out.push("=======".into());
    out.extend(theirs.iter().cloned());
    out.push(">>>>>>> theirs".into());
    out
}

/// The three-way choice for one block: the side that changed it, or `None` when both changed
/// it differently.
fn pick<'a, T: PartialEq>(base: &T, ours: &'a T, theirs: &'a T) -> Option<&'a T> {
    if theirs == base || ours == theirs {
        Some(ours)
    } else if ours == base {
        Some(theirs)
    } else {
        None
    }
}

/// Applies the changes from `base` to `theirs` onto `ours`, entry by entry (the module's
/// rules). An error when one of the three isn't a register in its usual form.
pub(crate) fn merge3(base: &str, ours: &str, theirs: &str) -> Result<Merged, String> {
    let read = |what: &str, text: &str| {
        Register::parse(text).map_err(|e| format!("{what}: {REGISTER} can't be merged: {e}"))
    };
    let b = read("the base", base)?;
    let o = read("ours", ours)?;
    let t = read("theirs", theirs)?;
    let mut out = o.clone();
    let mut conflicts = Vec::new();

    match pick(&b.preamble, &o.preamble, &t.preamble) {
        Some(p) => out.preamble = p.clone(),
        None => {
            conflicts.push("the text before the first section".to_string());
            out.preamble = markers(&o.preamble, &t.preamble);
        }
    }

    // Sections: theirs' new ones (after the section before them in theirs), and the intros.
    let find = |r: &Register, h: &str| r.sections.iter().position(|s| s.heading == h);
    for (k, section) in t.sections.iter().enumerate() {
        if find(&b, &section.heading).is_some() || find(&out, &section.heading).is_some() {
            continue;
        }
        let at = k
            .checked_sub(1)
            .and_then(|p| find(&out, &t.sections[p].heading))
            .map_or(out.sections.len(), |i| i + 1);
        out.sections.insert(
            at,
            Section {
                heading: section.heading.clone(),
                intro: section.intro.clone(),
                entries: Vec::new(),
            },
        );
    }
    for section in &mut out.sections {
        let (Some(bi), Some(ti)) = (find(&b, &section.heading), find(&t, &section.heading)) else {
            continue;
        };
        let (base_intro, their_intro) = (&b.sections[bi].intro, &t.sections[ti].intro);
        match pick(base_intro, &section.intro, their_intro) {
            Some(intro) => section.intro = intro.clone(),
            None => {
                conflicts.push(format!("the text of `{}`", section.heading));
                section.intro = markers(&section.intro, their_intro);
            }
        }
    }

    // Entries.
    let (eb, eo, et) = (b.entries(), o.entries(), t.entries());
    let ids: std::collections::BTreeSet<Id> = eb
        .keys()
        .chain(eo.keys())
        .chain(et.keys())
        .copied()
        .collect();
    for id in ids {
        let (vb, vo, vt) = (eb.get(&id), eo.get(&id), et.get(&id));
        if vt == vb || vo == vt {
            continue;
        }
        if vo == vb {
            out.remove(id);
            if let Some((heading, entry)) = vt
                && !out.insert(heading, (*entry).clone())
            {
                conflicts.push(format!(
                    "{}: its section `{heading}` is gone from ours",
                    name(id)
                ));
            }
            continue;
        }
        conflicts.push(format!("{} changed on both sides", name(id)));
        let theirs_lines = vt.map(|(_, e)| e.lines.clone()).unwrap_or_default();
        let ours_lines = vo.map(|(_, e)| e.lines.clone()).unwrap_or_default();
        let marked = Entry {
            id,
            lines: markers(&ours_lines, &theirs_lines),
        };
        match (vo, vt) {
            (Some((heading, _)), _) | (None, Some((heading, _))) => {
                let heading = heading.to_string();
                out.remove(id);
                if !out.insert(&heading, marked) {
                    conflicts.push(format!("{}: no section `{heading}`", name(id)));
                }
            }
            (None, None) => {}
        }
    }

    // Sections theirs removed go when ours left them as they were, emptied by the entries.
    for section in &b.sections {
        if find(&t, &section.heading).is_some() {
            continue;
        }
        if let Some(i) = find(&out, &section.heading) {
            if out.sections[i].entries.is_empty() && out.sections[i].intro == section.intro {
                out.sections.remove(i);
            } else {
                conflicts.push(format!(
                    "the section `{}`: theirs removes it, ours changed it",
                    section.heading
                ));
            }
        }
    }

    let text = out.render();
    Ok(if conflicts.is_empty() {
        Merged::Clean(text)
    } else {
        Merged::Conflict { text, conflicts }
    })
}

/// `cargo xtask merge-register <base> <ours> <theirs>`: git's merge driver (`%O %A %B`). Writes
/// the merge into `ours`; an error (git then reports a conflict) when both sides changed an
/// entry differently, with the conflict markers in `ours`, or when a side isn't a register in
/// its usual form, leaving `ours` as it was.
pub(crate) fn driver(base: &Path, ours: &Path, theirs: &Path) -> Result<(), String> {
    let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    let merged = merge3(&read(base)?, &read(ours)?, &read(theirs)?)?;
    let (text, conflicts) = match merged {
        Merged::Clean(text) => (text, Vec::new()),
        Merged::Conflict { text, conflicts } => (text, conflicts),
    };
    std::fs::write(ours, text).map_err(|e| format!("{}: {e}", ours.display()))?;
    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{REGISTER}: conflicts, marked in the file: {}",
            conflicts.join("; ")
        ))
    }
}

/// `git show <rev>:docs/improvements.md` in `dir`.
fn register_at(dir: &Path, rev: &str) -> Result<String, String> {
    crate::git(dir, &["show", &format!("{rev}:{REGISTER}")])
}

/// The register `base` gets from the changes of `originals` (commits, in order), each applied
/// entry by entry as the merge driver applies it: what a land of those commits must give.
pub(crate) fn rebuild(dir: &Path, base: &str, originals: &[String]) -> Result<String, String> {
    let mut register = register_at(dir, base)?;
    for commit in originals {
        let before = register_at(dir, &format!("{commit}^"))?;
        let after = register_at(dir, commit)?;
        register = match merge3(&before, &register, &after)? {
            Merged::Clean(text) => text,
            Merged::Conflict { conflicts, .. } => {
                return Err(format!(
                    "{REGISTER}: {commit}'s changes conflict with the register before it: {}",
                    conflicts.join("; ")
                ));
            }
        };
    }
    Ok(register)
}

/// Whether `commit` changes the register.
pub(crate) fn touches_register(dir: &Path, commit: &str) -> Result<bool, String> {
    let files = crate::git(
        dir,
        &["diff-tree", "--no-commit-id", "--name-only", "-r", commit],
    )?;
    Ok(files.lines().any(|l| l.trim() == REGISTER))
}

/// The entries in which two registers differ, for messages: `U-31`, `the text of ## X`, ...
pub(crate) fn differences(expected: &str, actual: &str) -> Vec<String> {
    let (Ok(e), Ok(a)) = (Register::parse(expected), Register::parse(actual)) else {
        return vec!["the whole file (one side isn't in the register's form)".into()];
    };
    let mut out = Vec::new();
    if e.preamble != a.preamble {
        out.push("the text before the first section".into());
    }
    let (ee, ea) = (e.entries(), a.entries());
    let ids: std::collections::BTreeSet<Id> = ee.keys().chain(ea.keys()).copied().collect();
    for id in ids {
        if ee.get(&id) != ea.get(&id) {
            out.push(name(id));
        }
    }
    let intros = |r: &Register| -> Vec<(String, Vec<String>)> {
        r.sections
            .iter()
            .map(|s| (s.heading.clone(), s.intro.clone()))
            .collect()
    };
    if intros(&e) != intros(&a) {
        out.push("the sections or their text".into());
    }
    if out.is_empty() && expected != actual {
        out.push("the order of the entries".into());
    }
    out
}

#[cfg(test)]
#[path = "register_merge_tests.rs"]
mod tests;

/// Scratch git repositories for the tests of the merge driver and of `xtask land`'s checks.
#[cfg(test)]
pub(crate) mod testing {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    /// A register whose last entry ends with two stray lines, as phase0's did on 2026-10-06.
    pub(crate) const STRAY: &str = "\
# Improvement candidates

## Undefined behaviour upstream

### U-31. Thirty-one

- **Status:** matched in 2.3b, and in 2.3c1.

### U-46. Forty-six

- **Status:** p3-context.
  stray line one
  stray line two
";

    /// [`STRAY`] without its stray lines.
    pub(crate) fn fixed() -> String {
        STRAY.replace("  stray line one\n  stray line two\n", "")
    }

    /// `text` with `U-52` appended at its end.
    pub(crate) fn appended(text: &str) -> String {
        format!("{text}\n### U-52. Fifty-two\n\n- **Status:** to port.\n")
    }

    /// The `git -c` option that makes this test binary the `ocio-register` merge driver: git
    /// runs it with `OCIO_RS_TEST_MERGE_DRIVER` naming the driver's three files, and only
    /// `register_merge::tests::merge_driver_of_the_end_to_end_tests`, which then is `cargo
    /// xtask merge-register %O %A %B`. (The xtask executable itself can't serve: a gate runs
    /// it from the target directory the tests build into, and Windows can't replace a
    /// running executable.)
    fn driver() -> String {
        let exe = std::env::current_exe().unwrap();
        let exe = crate::plain_path(&exe).to_string_lossy().replace('\\', "/");
        format!(
            "merge.ocio-register.driver=OCIO_RS_TEST_MERGE_DRIVER='%O|%A|%B' '{exe}' --exact \
             register_merge::tests::merge_driver_of_the_end_to_end_tests --test-threads=1 \
             --nocapture --quiet"
        )
    }

    /// A scratch repository on `main`, whose `.gitattributes` merges the register with
    /// `merge` (`ocio-register`, or `union` as before), deleted at the end.
    pub(crate) struct Repo(PathBuf);

    impl Repo {
        pub(crate) fn new(name: &str, merge: &str) -> Repo {
            let dir = std::env::temp_dir().join(format!(
                "ocio-rs-xtask-register-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("docs")).unwrap();
            std::fs::write(
                dir.join(".gitattributes"),
                format!("{} merge={merge}\n", super::REGISTER),
            )
            .unwrap();
            let repo = Repo(dir);
            repo.git(&["init", "--quiet"]).unwrap();
            repo.git(&["checkout", "--quiet", "-b", "main"]).unwrap();
            repo
        }

        pub(crate) fn path(&self) -> &Path {
            &self.0
        }

        /// `git <args>` with an identity of its own, the merge driver, and none of the user's
        /// signing, hooks or line-ending settings.
        pub(crate) fn git(&self, args: &[&str]) -> Result<String, String> {
            let out = crate::clear_git_env(&mut Command::new("git"))
                .current_dir(&self.0)
                .args(["-c", "user.name=xtask test", "-c", "user.email=xtask@test"])
                .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
                .args(["-c", "core.hooksPath=no-hooks", "-c", &driver()])
                .args(args)
                .stdin(Stdio::null())
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if out.status.success() {
                Ok(stdout)
            } else {
                Err(format!(
                    "{stdout}\n{}",
                    String::from_utf8_lossy(&out.stderr)
                ))
            }
        }

        /// Commits `register` (and anything else changed) as `subject`; its hash.
        pub(crate) fn commit(&self, register: &str, subject: &str) -> String {
            std::fs::write(self.0.join(super::REGISTER), register).unwrap();
            self.git(&["add", "-A"]).unwrap();
            self.git(&["commit", "--quiet", "-m", subject]).unwrap();
            self.git(&["rev-parse", "HEAD"]).unwrap()
        }

        pub(crate) fn register(&self) -> String {
            std::fs::read_to_string(self.0.join(super::REGISTER)).unwrap()
        }
    }

    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
