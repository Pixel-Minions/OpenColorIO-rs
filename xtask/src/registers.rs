// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The registers' structure, checked by `cargo xtask guards`: `docs/improvements.md` (the
//! `I-` and `U-` entries) and `docs/deviations.md` (the `D-` table).
//!
//! Merges and replays have moved register text silently: a status block under the wrong
//! entry, an entry twice, a heading glued to the text before it. Each of those breaks a rule
//! checked here, and the problem names the entry.
//!
//! `docs/improvements.md`:
//! - every `### ` heading is an entry, `### I-n. title` or `### U-n. title`, under a `## `
//!   section, and each number appears once;
//! - within each section, the `I-` entries are in ascending order (the sections group them by
//!   subject, so the numbers aren't in order across the file), and the `U-` entries are in
//!   ascending order across the file;
//! - every entry has exactly one `- **Status:**` line;
//! - a blank line precedes every heading.
//!
//! `docs/deviations.md` is a table, one row per deviation: every `D-n` appears once, and
//! every row has the header's number of cells, none of them empty. Its rows are kept in the
//! order the owner approved them, not by number, so the order isn't checked.

use std::collections::BTreeMap;
use std::path::Path;

/// Problems of both registers under `root`.
pub(crate) fn check(root: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    for (rel, check) in [
        (
            "docs/improvements.md",
            check_improvements as fn(&str) -> Vec<String>,
        ),
        ("docs/deviations.md", check_deviations),
    ] {
        match std::fs::read_to_string(root.join(rel)) {
            Ok(text) => problems.extend(check(&text).into_iter().map(|p| format!("{rel}:{p}"))),
            Err(e) => problems.push(format!("{rel}: {e}")),
        }
    }
    problems
}

/// An entry's series and number: `('U', 10)` for `U-10`.
fn entry_id(heading: &str) -> Option<(char, u32)> {
    let rest = heading.strip_prefix("### ")?;
    let mut chars = rest.chars();
    let series = chars.next().filter(|c| *c == 'I' || *c == 'U')?;
    let rest = chars.as_str().strip_prefix('-')?;
    let (number, title) = rest.split_once(". ")?;
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) || title.trim().is_empty() {
        return None;
    }
    Some((series, number.parse().ok()?))
}

/// Problems of `docs/improvements.md`, each `line: problem`.
pub(crate) fn check_improvements(text: &str) -> Vec<String> {
    struct Entry {
        id: (char, u32),
        line: usize,
        section: usize,
        statuses: Vec<usize>,
    }
    /// What the lines being read belong to.
    enum Owner {
        /// The text before the first section, which describes the entries' fields.
        Preamble,
        /// A section's text before its first entry.
        Section,
        /// The entry at this index of `entries`.
        Entry(usize),
        /// A heading already reported as no entry.
        Unknown,
    }
    let mut problems = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    let mut owner = Owner::Preamble;
    let mut section: Option<usize> = None;
    let mut sections = 0;
    let mut previous = "";
    let mut in_fence = false;
    for (n, line) in text.lines().enumerate() {
        let at = n + 1;
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
        if in_fence {
            previous = line;
            continue;
        }
        if line.starts_with('#') {
            if n > 0 && !previous.trim().is_empty() {
                problems.push(format!("{at}: no blank line before the heading `{line}`"));
            }
            if line.starts_with("## ") {
                section = Some(sections);
                sections += 1;
                owner = Owner::Section;
            } else if line.starts_with("### ") {
                owner = Owner::Unknown;
                match (entry_id(line), section) {
                    (Some(id), Some(section)) => {
                        entries.push(Entry {
                            id,
                            line: at,
                            section,
                            statuses: Vec::new(),
                        });
                        owner = Owner::Entry(entries.len() - 1);
                    }
                    (Some(_), None) => {
                        problems.push(format!("{at}: `{line}` is outside every `## ` section"))
                    }
                    (None, _) => problems.push(format!(
                        "{at}: `{line}` is not an entry heading (`### I-n. title` or `### U-n. title`)"
                    )),
                }
            } else if line.starts_with("# ") && n > 0 {
                problems.push(format!("{at}: a second title `{line}`"));
            }
        } else if line.starts_with("- **Status:**") {
            match owner {
                Owner::Entry(i) => entries[i].statuses.push(at),
                Owner::Section => problems.push(format!(
                    "{at}: a status line before the section's first entry"
                )),
                // The preamble describes the field; a bad heading is reported already.
                Owner::Preamble | Owner::Unknown => {}
            }
        }
        previous = line;
    }

    let name = |id: (char, u32)| format!("{}-{}", id.0, id.1);
    let mut seen: BTreeMap<(char, u32), Vec<usize>> = BTreeMap::new();
    for entry in &entries {
        seen.entry(entry.id).or_default().push(entry.line);
        if entry.statuses.len() != 1 {
            let lines: Vec<String> = entry.statuses.iter().map(|l| l.to_string()).collect();
            problems.push(format!(
                "{}: {} has {} `- **Status:**` lines, not 1{}",
                entry.line,
                name(entry.id),
                entry.statuses.len(),
                if lines.is_empty() {
                    String::new()
                } else {
                    format!(" (lines {})", lines.join(", "))
                }
            ));
        }
    }
    for (id, lines) in &seen {
        if lines.len() > 1 {
            let lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            problems.push(format!(
                "{}: {} appears {} times (lines {})",
                lines[1],
                name(*id),
                lines.len(),
                lines.join(", ")
            ));
        }
    }
    // Order: U- across the file, I- within each section.
    let mut last_u: Option<&Entry> = None;
    let mut last_i: BTreeMap<usize, &Entry> = BTreeMap::new();
    for entry in &entries {
        let last = match entry.id.0 {
            'U' => last_u.replace(entry),
            _ => last_i.insert(entry.section, entry),
        };
        if let Some(last) = last
            && last.id.1 >= entry.id.1
        {
            let scope = if entry.id.0 == 'U' {
                "in the file"
            } else {
                "in its section"
            };
            problems.push(format!(
                "{}: {} comes after {} (line {}) {scope}; they go in ascending order",
                entry.line,
                name(entry.id),
                name(last.id),
                last.line
            ));
        }
    }
    problems
}

/// The cells of a Markdown table row, trimmed: `| a | b |` gives `["a", "b"]`.
fn cells(line: &str) -> Vec<&str> {
    let inner = line.trim().trim_start_matches('|').trim_end_matches('|');
    inner.split('|').map(str::trim).collect()
}

/// Problems of `docs/deviations.md`, each `line: problem`.
pub(crate) fn check_deviations(text: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let mut columns: Option<usize> = None;
    let mut seen: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    let mut rows = 0;
    for (n, line) in text.lines().enumerate() {
        let at = n + 1;
        if !line.trim_start().starts_with('|') {
            continue;
        }
        let row = cells(line);
        if columns.is_none() {
            if row.first() != Some(&"Id") {
                problems.push(format!("{at}: the table's first column is not `Id`"));
            }
            columns = Some(row.len());
            continue;
        }
        if row
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'))
        {
            continue;
        }
        rows += 1;
        let id = row.first().copied().unwrap_or("");
        let number = id
            .strip_prefix("D-")
            .filter(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|d| d.parse::<u32>().ok());
        let Some(number) = number else {
            problems.push(format!("{at}: `{id}` is not a deviation id (`D-n`)"));
            continue;
        };
        seen.entry(number).or_default().push(at);
        let want = columns.unwrap_or(0);
        if row.len() != want {
            problems.push(format!(
                "{at}: D-{number} has {} cells, not the header's {want}",
                row.len()
            ));
        } else if let Some(empty) = row.iter().position(|c| c.is_empty()) {
            problems.push(format!(
                "{at}: D-{number} has an empty cell (column {})",
                empty + 1
            ));
        }
    }
    if rows == 0 {
        problems.push("1: no deviation table".to_string());
    }
    for (number, lines) in &seen {
        if lines.len() > 1 {
            let lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            problems.push(format!(
                "{}: D-{number} appears {} times (lines {})",
                lines[1],
                lines.len(),
                lines.join(", ")
            ));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "\
# Improvement candidates

Preamble, with `- **Status:**` named in a sentence.

## Images

### I-1. One

- **Upstream:** text.
- **Status:** open.

### I-18. Two

- **Status:** matched.

## Configs

### I-5. Three

- **Status:** open.

## Undefined behaviour upstream

### U-1. Four

- **Status:** open.

### U-15. Five

- **Status:** open.
";

    fn problems(text: &str) -> Vec<String> {
        check_improvements(text)
    }

    /// One problem, which contains every fragment.
    fn one(text: &str, fragments: &[&str]) {
        let p = problems(text);
        assert_eq!(p.len(), 1, "{p:#?}");
        for f in fragments {
            assert!(p[0].contains(f), "{:?} lacks {f:?}", p[0]);
        }
    }

    #[test]
    fn a_well_formed_register_passes() {
        assert_eq!(problems(GOOD), Vec::<String>::new());
    }

    #[test]
    fn an_entry_twice_is_named() {
        let text = GOOD.replace(
            "### U-15. Five",
            "### U-1. Four\n\n- **Status:** open.\n\n### U-15. Five",
        );
        // The second U-1 also breaks the order; the duplicate is reported by name.
        let p = problems(&text);
        assert!(
            p.iter()
                .any(|p| p.contains("U-1 appears 2 times (lines 24, 28)")),
            "{p:#?}"
        );
    }

    #[test]
    fn a_misplaced_status_is_named() {
        // A status block moved under the next entry: one entry without, one with two.
        let text = GOOD.replace(
            "- **Status:** open.\n\n### U-15. Five\n\n- **Status:** open.",
            "\n### U-15. Five\n\n- **Status:** open.\n- **Status:** open.",
        );
        let p = problems(&text);
        assert!(
            p.iter()
                .any(|p| p.contains("U-1 has 0 `- **Status:**` lines")),
            "{p:#?}"
        );
        assert!(
            p.iter()
                .any(|p| p.contains("U-15 has 2 `- **Status:**` lines, not 1 (lines")),
            "{p:#?}"
        );
    }

    #[test]
    fn a_missing_blank_line_is_named() {
        let text = GOOD.replace(
            "- **Status:** matched.\n\n## Configs",
            "- **Status:** matched.\n## Configs",
        );
        one(&text, &["no blank line before the heading `## Configs`"]);
        let text = GOOD.replace(
            "- **Status:** open.\n\n### I-18.",
            "- **Status:** open.\n### I-18.",
        );
        one(&text, &["no blank line before the heading `### I-18. Two`"]);
    }

    #[test]
    fn u_entries_out_of_order_are_named() {
        let text = GOOD
            .replace("### U-1. Four", "### U-X")
            .replace("### U-15. Five", "### U-1. Four")
            .replace("### U-X", "### U-15. Five");
        one(&text, &["U-1 comes after U-15", "in the file"]);
    }

    #[test]
    fn i_entries_out_of_order_in_a_section_are_named() {
        let text = GOOD
            .replace("### I-1. One", "### I-X")
            .replace("### I-18. Two", "### I-1. One")
            .replace("### I-X", "### I-18. Two");
        one(&text, &["I-1 comes after I-18", "in its section"]);
        // Across sections, I- numbers go down (I-18, then I-5): that is the file's layout.
        assert!(GOOD.find("### I-18.") < GOOD.find("### I-5."));
    }

    #[test]
    fn a_heading_that_is_no_entry_is_named() {
        let text = GOOD.replace("### I-5. Three", "### I5. Three");
        one(&text, &["`### I5. Three` is not an entry heading"]);
        let text = GOOD.replace("### I-5. Three", "### I-5.");
        one(&text, &["`### I-5.` is not an entry heading"]);
    }

    #[test]
    fn an_entry_outside_a_section_is_named() {
        let text = GOOD.replace(
            "Preamble, with",
            "### I-9. Early\n\n- **Status:** open.\n\nPreamble, with",
        );
        let p = problems(&text);
        assert!(
            p.iter()
                .any(|p| p.contains("`### I-9. Early` is outside every `## ` section")),
            "{p:#?}"
        );
    }

    const DEVIATIONS: &str = "\
# Deviations

| Id | Upstream | Port | Approved |
|---|---|---|---|
| D-1 | a | b | yes |
| D-3 | c | d | yes |
| D-2 | e | f | yes |
";

    #[test]
    fn well_formed_deviations_pass_in_any_order() {
        assert_eq!(check_deviations(DEVIATIONS), Vec::<String>::new());
    }

    #[test]
    fn deviation_problems_are_named() {
        let p = check_deviations(&DEVIATIONS.replace("| D-2 |", "| D-1 |"));
        assert_eq!(p, ["7: D-1 appears 2 times (lines 5, 7)"]);
        let p = check_deviations(&DEVIATIONS.replace("| e | f |", "| e |"));
        assert_eq!(p, ["7: D-2 has 3 cells, not the header's 4"]);
        let p = check_deviations(&DEVIATIONS.replace("| c | d |", "| c |  |"));
        assert_eq!(p, ["6: D-3 has an empty cell (column 3)"]);
        let p = check_deviations(&DEVIATIONS.replace("| D-3 |", "| X-3 |"));
        assert_eq!(p, ["6: `X-3` is not a deviation id (`D-n`)"]);
        let p = check_deviations("# Deviations\n\nNo table.\n");
        assert_eq!(p, ["1: no deviation table"]);
    }

    /// The registers as they are in this checkout pass.
    #[test]
    fn the_registers_pass() {
        let root = ocio_testkit::paths::workspace_root();
        assert_eq!(check(root), Vec::<String>::new());
    }
}
