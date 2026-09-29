// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Upstream test inventory, ported-test markers, the ratchet and `docs/parity.md`.
//!
//! A Rust test counts as a port of an upstream test when a comment directly above it names
//! the upstream test exactly as upstream's macro does, for example:
//!
//! ```text
//! /// Port of `OCIO_ADD_TEST(Lut1DOpCPU, apply_half)` @ v2.5.2.
//! #[test]
//! fn apply_half() { ... }
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use ocio_testkit::paths;
use serde::{Deserialize, Serialize};

/// An upstream test: suite ("cpu" or "gpu"), id ("Group/Name") and the file defining it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct UpstreamTest {
    pub(crate) suite: &'static str,
    pub(crate) id: String,
    pub(crate) file: String,
}

/// Removes comments, `#define` bodies and `#if 0` blocks, keeping line structure.
fn preprocess(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_block = false;
    while let Some(c) = chars.next() {
        if in_block {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block = false;
            } else if c == '\n' {
                out.push('\n');
            }
            continue;
        }
        match c {
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                in_block = true;
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            _ => out.push(c),
        }
    }
    let mut result = String::with_capacity(out.len());
    let mut depth_disabled = 0usize;
    let mut depth = 0usize;
    let mut continuation = false;
    for line in out.lines() {
        let t = line.trim_start();
        let was_continuation = continuation;
        continuation = t.ends_with('\\');
        if was_continuation || t.starts_with("#define") {
            result.push('\n');
            continue;
        }
        if t.starts_with("#if") {
            depth += 1;
            if depth_disabled == 0 && (t == "#if 0" || t.starts_with("#if 0 ")) {
                depth_disabled = depth;
            }
        } else if t.starts_with("#endif") {
            if depth_disabled == depth {
                depth_disabled = 0;
            }
            depth = depth.saturating_sub(1);
        } else if (t.starts_with("#else") || t.starts_with("#elif"))
            && depth_disabled == depth
            && depth > 0
        {
            depth_disabled = 0;
        }
        if depth_disabled == 0 {
            result.push_str(line);
        }
        result.push('\n');
    }
    result
}

/// Test ids declared by the upstream test macros in `text` (already preprocessed).
fn macro_ids(text: &str) -> Vec<(&'static str, String)> {
    const SIMD: &[(&str, &str)] = &[
        ("OCIO_ADD_TEST_SSE2(", "SSE2"),
        ("OCIO_ADD_TEST_AVX512(", "AVX512"),
        ("OCIO_ADD_TEST_AVX2(", "AVX2"),
        ("OCIO_ADD_TEST_AVX(", "AVX"),
    ];
    let mut ids = Vec::new();
    for line in text.lines() {
        let mut rest = line;
        while let Some(pos) = rest.find("OCIO_ADD_") {
            let tail = &rest[pos..];
            let (suite, args_start) = if tail.starts_with("OCIO_ADD_TEST(") {
                ("cpu", "OCIO_ADD_TEST(".len())
            } else if tail.starts_with("OCIO_ADD_GPU_TEST(") {
                ("gpu", "OCIO_ADD_GPU_TEST(".len())
            } else if let Some((m, group)) = SIMD.iter().find(|(m, _)| tail.starts_with(m)) {
                if let Some(end) = tail[m.len()..].find(')') {
                    let name = tail[m.len()..m.len() + end].trim();
                    if is_ident(name) {
                        ids.push(("cpu", format!("{group}/{name}")));
                    }
                }
                rest = &tail[m.len()..];
                continue;
            } else {
                rest = &tail["OCIO_ADD_".len()..];
                continue;
            };
            if let Some(end) = tail[args_start..].find(')') {
                let args = &tail[args_start..args_start + end];
                if let Some((group, name)) = args.split_once(',') {
                    let (group, name) = (group.trim(), name.trim());
                    if is_ident(group) && is_ident(name) {
                        ids.push((suite, format!("{group}/{name}")));
                    }
                }
            }
            rest = &tail[args_start..];
        }
    }
    ids
}

fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Every upstream C++ and GPU test, from the pinned submodule.
pub(crate) fn upstream_tests() -> Result<Vec<UpstreamTest>, String> {
    let upstream = paths::upstream_dir();
    let mut tests = BTreeSet::new();
    for dir in ["tests/cpu", "tests/utils", "tests/gpu"] {
        for rel in crate::walk(&upstream.join(dir)) {
            if !rel.ends_with(".cpp") {
                continue;
            }
            let file = format!("{dir}/{rel}");
            let text = std::fs::read_to_string(upstream.join(&file))
                .map_err(|e| format!("{file}: {e}"))?;
            for (suite, id) in macro_ids(&preprocess(&text)) {
                tests.insert(UpstreamTest {
                    suite,
                    id,
                    file: file.clone(),
                });
            }
        }
    }
    if tests.is_empty() {
        return Err("no upstream tests found; is upstream/OpenColorIO checked out?".into());
    }
    Ok(tests.into_iter().collect())
}

/// Upstream Python tests: "File.py::Class.test_name".
pub(crate) fn upstream_python_tests() -> Vec<String> {
    let dir = paths::upstream_dir().join("tests").join("python");
    let mut tests = Vec::new();
    for rel in crate::walk(&dir) {
        if !rel.ends_with(".py") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(dir.join(&rel)) else {
            continue;
        };
        let mut class = String::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("class ") {
                class = rest
                    .split(['(', ':'])
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
            } else if let Some(rest) = line.trim_start().strip_prefix("def test")
                && line.starts_with("    def ")
            {
                let name = format!("test{}", rest.split('(').next().unwrap_or(""));
                tests.push(format!("{rel}::{class}.{name}"));
            }
        }
    }
    tests
}

/// Ported-test markers in Rust sources: (suite, id) -> Rust files mentioning it.
pub(crate) fn ported_markers() -> Result<BTreeMap<(&'static str, String), Vec<String>>, String> {
    let root = paths::workspace_root();
    let mut found: BTreeMap<(&'static str, String), Vec<String>> = BTreeMap::new();
    for rel in crate::walk(&root.join("crates")) {
        if !rel.ends_with(".rs") {
            continue;
        }
        let file = format!("crates/{rel}");
        let text = std::fs::read_to_string(root.join(&file)).map_err(|e| format!("{file}: {e}"))?;
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim_start();
            if !t.starts_with("//") {
                continue;
            }
            // The marker must be in the comment block directly above a #[test] function.
            let attached = lines[i + 1..]
                .iter()
                .map(|l| l.trim_start())
                .take_while(|l| l.starts_with("//") || l.starts_with("#["))
                .any(|l| l.starts_with("#[test]"));
            if !attached {
                continue;
            }
            for (suite, id) in macro_ids(t) {
                found.entry((suite, id)).or_default().push(file.clone());
            }
        }
    }
    Ok(found)
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Ratchet {
    ported_cpu: usize,
    ported_gpu: usize,
}

struct Counts {
    upstream: Vec<UpstreamTest>,
    ported: BTreeSet<(&'static str, String)>,
    not_applicable: BTreeSet<String>,
    unknown_markers: Vec<String>,
}

fn counts() -> Result<Counts, String> {
    let upstream = upstream_tests()?;
    let markers = ported_markers()?;
    let known: BTreeSet<(&str, &str)> = upstream.iter().map(|t| (t.suite, t.id.as_str())).collect();
    let mut unknown_markers = Vec::new();
    let mut ported = BTreeSet::new();
    for ((suite, id), files) in &markers {
        if known.contains(&(*suite, id.as_str())) {
            ported.insert((*suite, id.clone()));
        } else {
            unknown_markers.push(format!(
                "{suite} test {id} (in {}) does not exist upstream",
                files.join(", ")
            ));
        }
    }
    let waivers = crate::guards::load_waivers()?;
    let not_applicable = waivers
        .not_applicable
        .iter()
        .map(|n| n.test.clone())
        .collect();
    Ok(Counts {
        upstream,
        ported,
        not_applicable,
        unknown_markers,
    })
}

pub(crate) fn list_tests() -> Result<(), String> {
    let c = counts()?;
    for t in &c.upstream {
        let state = if c.ported.contains(&(t.suite, t.id.clone())) {
            "ported"
        } else if c.not_applicable.contains(&t.id) {
            "n/a"
        } else {
            "todo"
        };
        println!("{:<4} {:<7} {:<60} {}", t.suite, state, t.id, t.file);
    }
    Ok(())
}

fn ratchet_path() -> std::path::PathBuf {
    paths::workspace_root().join("docs").join("ratchet.toml")
}

pub(crate) fn ratchet(update: bool) -> Result<(), String> {
    let c = counts()?;
    if !c.unknown_markers.is_empty() {
        return Err(c.unknown_markers.join("\n"));
    }
    let now = Ratchet {
        ported_cpu: c.ported.iter().filter(|(s, _)| *s == "cpu").count(),
        ported_gpu: c.ported.iter().filter(|(s, _)| *s == "gpu").count(),
    };
    let base: Ratchet = match std::fs::read_to_string(ratchet_path()) {
        Ok(text) => toml::from_str(&text).map_err(|e| format!("docs/ratchet.toml: {e}"))?,
        Err(_) => Ratchet::default(),
    };
    if now.ported_cpu < base.ported_cpu || now.ported_gpu < base.ported_gpu {
        return Err(format!(
            "ported upstream tests went down: cpu {} -> {}, gpu {} -> {}",
            base.ported_cpu, now.ported_cpu, base.ported_gpu, now.ported_gpu
        ));
    }
    if update {
        let text = format!(
            "# Ported upstream tests may only go up (PLAN.md §7). Raise with `cargo xtask ratchet --update`.\n{}",
            toml::to_string(&now).map_err(|e| e.to_string())?
        );
        std::fs::write(ratchet_path(), text).map_err(|e| e.to_string())?;
    } else if now.ported_cpu > base.ported_cpu || now.ported_gpu > base.ported_gpu {
        println!("ratchet: more tests ported than recorded; run `cargo xtask ratchet --update`");
    }
    println!(
        "ratchet: cpu {} (baseline {}), gpu {} (baseline {})",
        now.ported_cpu, base.ported_cpu, now.ported_gpu, base.ported_gpu
    );
    Ok(())
}

pub(crate) fn write_dashboard(check: bool) -> Result<(), String> {
    let c = counts()?;
    let map = crate::upstream::load_map()?;
    let waivers = crate::guards::load_waivers()?;
    let python = upstream_python_tests();

    let mut per_file: BTreeMap<&str, (&str, usize, usize, usize)> = BTreeMap::new();
    for t in &c.upstream {
        let row = per_file
            .entry(t.file.as_str())
            .or_insert((t.suite, 0, 0, 0));
        row.1 += 1;
        if c.ported.contains(&(t.suite, t.id.clone())) {
            row.2 += 1;
        } else if c.not_applicable.contains(&t.id) {
            row.3 += 1;
        }
    }
    let total = |suite: &str| c.upstream.iter().filter(|t| t.suite == suite).count();
    let ported = |suite: &str| c.ported.iter().filter(|(s, _)| *s == suite).count();
    let na = |suite: &str| {
        c.upstream
            .iter()
            .filter(|t| t.suite == suite && c.not_applicable.contains(&t.id))
            .count()
    };

    let mut md = String::new();
    let _ = writeln!(md, "# Parity dashboard\n");
    let _ = writeln!(
        md,
        "Generated by `cargo xtask parity` from the upstream submodule ({}), the ported-test markers, `upstream-map.toml` and `waivers.toml`. Do not edit.\n",
        map.upstream.tag
    );
    let _ = writeln!(md, "## Summary\n");
    let _ = writeln!(md, "| Measure | Done | Not applicable | Total |");
    let _ = writeln!(md, "|---|---:|---:|---:|");
    let _ = writeln!(
        md,
        "| Upstream C++ tests ported | {} | {} | {} |",
        ported("cpu"),
        na("cpu"),
        total("cpu")
    );
    let _ = writeln!(
        md,
        "| Upstream GPU tests ported | {} | {} | {} |",
        ported("gpu"),
        na("gpu"),
        total("gpu")
    );
    let _ = writeln!(
        md,
        "| Upstream Python tests passing (run unmodified) | 0 | 0 | {} |",
        python.len()
    );
    let files_done = map.files.iter().filter(|e| e.status == "done").count();
    let files_na = map.files.iter().filter(|e| e.status == "n/a").count();
    let _ = writeln!(
        md,
        "| Upstream files ported | {files_done} | {files_na} | {} |",
        map.files.len()
    );
    let _ = writeln!(md, "| Open waivers | {} | | |", waivers.waivers.len());
    let _ = writeln!(md, "\n## Upstream test files\n");
    let _ = writeln!(md, "| File | Suite | Ported | N/A | Tests |");
    let _ = writeln!(md, "|---|---|---:|---:|---:|");
    for (file, (suite, n, p, a)) in &per_file {
        let _ = writeln!(md, "| `{file}` | {suite} | {p} | {a} | {n} |");
    }
    let _ = writeln!(md, "\n## Upstream files by kind\n");
    let mut kinds: BTreeMap<&str, [usize; 4]> = BTreeMap::new();
    for e in &map.files {
        let row = kinds.entry(e.kind.as_str()).or_default();
        let i = match e.status.as_str() {
            "todo" => 0,
            "partial" => 1,
            "done" => 2,
            _ => 3,
        };
        row[i] += 1;
    }
    let _ = writeln!(md, "| Kind | To do | Partial | Done | N/A |");
    let _ = writeln!(md, "|---|---:|---:|---:|---:|");
    for (kind, r) in &kinds {
        let _ = writeln!(md, "| {kind} | {} | {} | {} | {} |", r[0], r[1], r[2], r[3]);
    }

    let path = paths::workspace_root().join("docs").join("parity.md");
    if check {
        let current = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        if current != md {
            return Err("docs/parity.md is stale; run `cargo xtask parity`".into());
        }
        println!("parity: docs/parity.md is current");
        return Ok(());
    }
    std::fs::create_dir_all(path.parent().expect("docs dir")).map_err(|e| e.to_string())?;
    std::fs::write(&path, md).map_err(|e| e.to_string())?;
    println!(
        "parity: cpu {}/{}, gpu {}/{}, python 0/{}",
        ported("cpu"),
        total("cpu"),
        ported("gpu"),
        total("gpu"),
        python.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_skips_disabled_code() {
        let src = "OCIO_ADD_TEST(A, b)\n#if 0\nOCIO_ADD_TEST(A, c)\n#endif\n// OCIO_ADD_TEST(A, d)\n#define X(n) \\\n OCIO_ADD_TEST(A, n)\nOCIO_ADD_TEST_SSE2(e)\n";
        let ids: Vec<String> = macro_ids(&preprocess(src))
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        assert_eq!(ids, ["A/b", "SSE2/e"]);
    }
}
