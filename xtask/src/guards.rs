// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `cargo xtask guards`: checks that keep anyone from passing by weakening a check
//! (PLAN.md §7).

use std::path::Path;

use ocio_testkit::paths;
use serde::Deserialize;

/// Owner-approved exceptions (`waivers.toml`).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Waivers {
    #[serde(default, rename = "waiver")]
    pub(crate) waivers: Vec<Waiver>,
    #[serde(default)]
    pub(crate) not_applicable: Vec<NotApplicable>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Waiver {
    pub(crate) id: String,
    pub(crate) scope: String,
    pub(crate) check: String,
    pub(crate) reason: String,
    pub(crate) approved_by: String,
    pub(crate) date: String,
    /// Source files allowed to contain an otherwise forbidden pattern, e.g. `#[ignore`.
    #[serde(default)]
    pub(crate) allow_patterns: Vec<AllowPattern>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AllowPattern {
    pub(crate) file: String,
    pub(crate) pattern: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NotApplicable {
    /// Upstream test id, as `cargo xtask upstream-tests` prints it.
    pub(crate) test: String,
    pub(crate) reason: String,
    pub(crate) approved_by: String,
}

pub(crate) fn load_waivers() -> Result<Waivers, String> {
    let path = paths::workspace_root().join("waivers.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Patterns that may not appear in Rust sources without a waiver.
const FORBIDDEN: &[(&str, &str)] = &[
    ("#[ignore", "tests may not be skipped"),
    ("todo!(", "unfinished code may not be merged"),
    ("unimplemented!(", "unfinished code may not be merged"),
];

/// Tolerance comparisons: allowed only through `ocio_testkit::upstream` helpers, which port
/// upstream's own tolerance checks.
const TOLERANCE: &[&str] = &[".abs() < ", ".abs() <= ", "f32::EPSILON", "f64::EPSILON"];

/// Where `unsafe` may be allowed (PLAN.md D8): SIMD modules, the Python binding, and the
/// test kit's FFI to the platform C runtime (the reference for C formatting and parsing).
fn unsafe_allowed(rel: &str) -> bool {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    rel.starts_with("crates/ocio-py/")
        || rel == "crates/ocio-testkit/src/crt.rs"
        || (rel.starts_with("crates/ocio-ops/")
            && (rel.contains("/simd/")
                || file.contains("_sse")
                || file.contains("_avx")
                || file.contains("_f16c")
                || matches!(
                    file,
                    "sse.rs" | "sse2.rs" | "avx.rs" | "avx2.rs" | "avx512.rs" | "cpu_info.rs"
                )))
}

fn rust_sources() -> Vec<String> {
    let root = paths::workspace_root();
    let mut files = Vec::new();
    for dir in ["crates", "xtask"] {
        for rel in crate::walk(&root.join(dir)) {
            if rel.ends_with(".rs") && !rel.contains("/target/") {
                files.push(format!("{dir}/{rel}"));
            }
        }
    }
    files
}

pub(crate) fn run() -> Result<(), String> {
    let root = paths::workspace_root();
    let waivers = load_waivers()?;
    let mut problems = Vec::new();

    for w in &waivers.waivers {
        if w.id.is_empty()
            || w.reason.is_empty()
            || w.approved_by.is_empty()
            || w.date.is_empty()
            || w.scope.is_empty()
            || w.check.is_empty()
        {
            problems.push(format!(
                "waivers.toml: waiver `{}` has an empty field",
                w.id
            ));
        }
    }
    for na in &waivers.not_applicable {
        if na.reason.is_empty() || na.approved_by.is_empty() {
            problems.push(format!(
                "waivers.toml: not_applicable `{}` needs a reason and approved_by",
                na.test
            ));
        }
    }
    let waived = |file: &str, pattern: &str| {
        waivers
            .waivers
            .iter()
            .flat_map(|w| &w.allow_patterns)
            .any(|a| a.file == file && a.pattern == pattern)
    };

    for rel in rust_sources() {
        let text = std::fs::read_to_string(root.join(&rel)).map_err(|e| format!("{rel}: {e}"))?;
        if !text.starts_with("// SPDX-License-Identifier: BSD-3-Clause\n// Copyright Contributors to the OpenColorIO Project.\n") {
            problems.push(format!("{rel}: missing the SPDX / copyright header"));
        }
        problems.extend(check_internals_uses(&rel, &text));
        let is_test_file = rel.ends_with("_tests.rs") || rel.contains("/tests/");
        let is_xtask = rel.starts_with("xtask/");
        for (n, line) in text.lines().enumerate() {
            let code = line.trim_start();
            let at = || format!("{rel}:{}", n + 1);
            // The guard's own pattern table is not a use of the patterns.
            if is_xtask && rel.ends_with("guards.rs") {
                continue;
            }
            for (pattern, why) in FORBIDDEN {
                if code.contains(pattern) && !code.starts_with("//") && !waived(&rel, pattern) {
                    problems.push(format!("{}: `{pattern}`: {why}", at()));
                }
            }
            if code.contains("allow(unsafe_code)") && !unsafe_allowed(&rel) {
                problems.push(format!(
                    "{}: `unsafe` is allowed only in SIMD modules and ocio-py (PLAN.md D8)",
                    at()
                ));
            }
            if is_test_file && !rel.starts_with("crates/ocio-testkit/src/upstream/") {
                for pattern in TOLERANCE {
                    if code.contains(pattern) && !code.starts_with("//") && !waived(&rel, pattern) {
                        problems.push(format!(
                            "{}: `{pattern}`: comparisons are exact; upstream tolerance checks go through ocio_testkit::upstream",
                            at()
                        ));
                    }
                }
            }
        }
    }

    for rel in manifests(root) {
        let text = std::fs::read_to_string(root.join(&rel)).map_err(|e| format!("{rel}: {e}"))?;
        problems.extend(check_internals_feature(&rel, &text));
    }
    problems.extend(check_dependency_pins(root));
    problems.extend(crate::registers::check(root));
    problems.extend(crate::upstream::validate(&crate::upstream::load_map()?));
    check_submodule(root, &mut problems);

    if problems.is_empty() {
        println!("guards: ok");
        Ok(())
    } else {
        Err(format!("guards failed:\n  {}", problems.join("\n  ")))
    }
}

/// The module of `ocio`'s test-only internals (`ocio::internals`, under the crate's `internals`
/// feature), and the feature's name.
const INTERNALS: &str = "internals";
/// Where `ocio::internals` is declared, and the attribute that must gate the declaration.
const INTERNALS_HOME: &str = "crates/ocio/src/lib.rs";
const INTERNALS_GATE: &str = "#[cfg(feature = \"internals\")]";
/// The only sources that may use `ocio::internals`: the crate's own integration tests.
const INTERNALS_USERS: &str = "crates/ocio/tests/";
/// The only manifest that may enable the feature, in its dev-dependency on itself.
const INTERNALS_MANIFEST: &str = "crates/ocio/Cargo.toml";

/// `ocio::internals` is for the crate's own integration tests only. `cargo clippy --workspace
/// --all-targets` and `cargo test --workspace` unify the feature that ocio's dev-dependency on
/// itself enables into every crate's build, so a use elsewhere (or a re-export from the crate)
/// would build there, and fail only in a plain `cargo build`.
///
/// Outside `crates/ocio/tests/`, the identifier `internals` may appear in code (not comments or
/// literals) only as `crates/ocio/src/lib.rs`'s `mod internals`, gated by
/// `#[cfg(feature = "internals")]`. Any other identifier `internals`, whatever path or alias
/// leads to it (`ocio::internals`, `use ocio::{internals, ..}`, `use ocio as o; o::internals`,
/// `use ocio::*; internals::..`, `pub use crate::internals::..`), is refused.
fn check_internals_uses(rel: &str, text: &str) -> Vec<String> {
    if rel.starts_with(INTERNALS_USERS) {
        return Vec::new();
    }
    let idents = identifiers(text);
    let mut problems = Vec::new();
    for (k, (word, line)) in idents.iter().enumerate() {
        if word != INTERNALS {
            continue;
        }
        let declaration = rel == INTERNALS_HOME && k > 0 && idents[k - 1].0 == "mod";
        if !declaration {
            problems.push(format!(
                "{rel}:{line}: `{INTERNALS}`: ocio's test-only internals (`ocio::internals`) may \
                 be used only by {INTERNALS_USERS}, and the crate may not re-export them (the \
                 identifier is reserved for them outside {INTERNALS_USERS})"
            ));
        } else if !gated(text, *line) {
            problems.push(format!(
                "{rel}:{line}: `mod {INTERNALS}` must be gated by `{INTERNALS_GATE}`, so that a \
                 build of the library never has it"
            ));
        }
    }
    problems
}

/// Whether the item declared on `line` (1-based) carries `INTERNALS_GATE`, on that line or among
/// the attributes and doc comments directly above it.
fn gated(text: &str, line: usize) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    let Some(own) = lines.get(line - 1) else {
        return false;
    };
    own.contains(INTERNALS_GATE)
        || lines[..line - 1]
            .iter()
            .rev()
            .map(|l| l.trim())
            .take_while(|l| l.starts_with("#[") || l.starts_with("//"))
            .any(|l| l.starts_with(INTERNALS_GATE))
}

/// The identifiers in Rust source `text`, with their 1-based lines, outside comments and
/// string, byte-string, raw-string and character literals. A raw identifier `r#x` is `x`.
fn identifiers(text: &str) -> Vec<(String, usize)> {
    let c: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut line = 1;
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        let next = c.get(i + 1).copied();
        if ch == '\n' {
            line += 1;
            i += 1;
        } else if ch == '/' && next == Some('/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '/' && next == Some('*') {
            // Block comments nest.
            let mut depth = 0;
            loop {
                if i >= c.len() {
                    break;
                } else if c[i] == '/' && c.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    line += usize::from(c[i] == '\n');
                    i += 1;
                }
            }
        } else if ch == '"' {
            i = skip_string(&c, i + 1, None, &mut line);
        } else if ch == '\'' {
            // A character literal ('x', '\n', '\u{1F600}'), or a lifetime or label ('a).
            if next == Some('\\') {
                i += 3;
                while i < c.len() && c[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if c.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1;
            }
        } else if ch.is_ascii_digit() {
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
        } else if ch.is_alphabetic() || ch == '_' {
            let start = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            let word: String = c[start..i].iter().collect();
            let mut hashes = 0;
            while c.get(i + hashes) == Some(&'#') {
                hashes += 1;
            }
            let after = c.get(i + hashes).copied();
            match word.as_str() {
                // r"..", r#".."#, br"..", cr#".."#
                "r" | "br" | "cr" if after == Some('"') => {
                    i = skip_string(&c, i + hashes + 1, Some(hashes), &mut line);
                }
                // b"..", c".."
                "b" | "c" if hashes == 0 && after == Some('"') => {
                    i = skip_string(&c, i + 1, None, &mut line);
                }
                // r#ident: the identifier follows.
                "r" if hashes == 1 && after.is_some_and(|a| a.is_alphabetic() || a == '_') => {
                    i += 1;
                }
                _ => out.push((word, line)),
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Skips a string literal whose body starts at `i`: a raw string (`raw` holds its number of
/// `#`, without escapes) up to its closing `"` and that many `#`, or another string (with `\`
/// escapes) up to its closing `"`. Returns the index past the literal.
fn skip_string(c: &[char], mut i: usize, raw: Option<usize>, line: &mut usize) -> usize {
    let hashes = raw.unwrap_or(0);
    while i < c.len() {
        match c[i] {
            '\\' if raw.is_none() => {
                *line += usize::from(c.get(i + 1) == Some(&'\n'));
                i += 2;
            }
            '"' if (0..hashes).all(|h| c.get(i + 1 + h) == Some(&'#')) => {
                return i + 1 + hashes;
            }
            ch => {
                *line += usize::from(ch == '\n');
                i += 1;
            }
        }
    }
    i
}

/// The workspace's manifests: the root `Cargo.toml`, the crates' and xtask's.
fn manifests(root: &Path) -> Vec<String> {
    let mut out = vec!["Cargo.toml".to_string()];
    for dir in ["crates", "xtask"] {
        for rel in crate::walk(&root.join(dir)) {
            if rel.ends_with("Cargo.toml") && !rel.contains("target/") {
                out.push(format!("{dir}/{rel}"));
            }
        }
    }
    out
}

/// Only `crates/ocio/Cargo.toml`'s dev-dependency on itself may enable ocio's `internals`
/// feature: no other dependency on ocio (a crate's, or the workspace's), no feature of another
/// crate (`ocio/internals`), and no other feature of ocio itself (`default` included).
fn check_internals_feature(rel: &str, text: &str) -> Vec<String> {
    let Ok(doc) = text.parse::<toml::Table>() else {
        // check_dependency_pins reports it.
        return Vec::new();
    };
    let mut problems = Vec::new();
    // (section, table) of every dependency table: the crate's, its targets', the workspace's.
    let mut tables: Vec<(String, &toml::Table)> = Vec::new();
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(t) = doc.get(section).and_then(|d| d.as_table()) {
            tables.push((section.to_string(), t));
        }
        for (cfg, target) in doc
            .get("target")
            .and_then(|t| t.as_table())
            .into_iter()
            .flatten()
        {
            if let Some(t) = target.get(section).and_then(|d| d.as_table()) {
                tables.push((format!("target.{cfg}.{section}"), t));
            }
        }
    }
    if let Some(t) = doc
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        tables.push(("workspace.dependencies".to_string(), t));
    }
    let mut ocio_names = vec!["ocio".to_string()];
    for (section, table) in &tables {
        for (name, spec) in *table {
            let package = spec.get("package").and_then(|p| p.as_str()).unwrap_or(name);
            if package != "ocio" {
                continue;
            }
            ocio_names.push(name.clone());
            let enables = spec
                .get("features")
                .and_then(|f| f.as_array())
                .is_some_and(|f| f.iter().any(|v| v.as_str() == Some(INTERNALS)));
            if enables && !(rel == INTERNALS_MANIFEST && section == "dev-dependencies") {
                problems.push(format!(
                    "{rel}: [{section}] `{name}` enables ocio's `{INTERNALS}` feature; only \
                     {INTERNALS_MANIFEST}'s dev-dependency on itself may (for {INTERNALS_USERS})"
                ));
            }
        }
    }
    for (feature, list) in doc
        .get("features")
        .and_then(|f| f.as_table())
        .into_iter()
        .flatten()
    {
        for entry in list
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
        {
            let of_ocio = entry.split_once('/').is_some_and(|(dep, f)| {
                f == INTERNALS && ocio_names.iter().any(|n| n == dep.trim_end_matches('?'))
            });
            let own = rel == INTERNALS_MANIFEST && entry == INTERNALS;
            if of_ocio || own {
                problems.push(format!(
                    "{rel}: feature `{feature}` enables ocio's `{INTERNALS}` feature (`{entry}`); \
                     only {INTERNALS_MANIFEST}'s dev-dependency on itself may"
                ));
            }
        }
    }
    problems
}

/// Third-party versions are exact (`=x.y.z`) in the workspace, and crates inherit them.
fn check_dependency_pins(root: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let Ok(text) = std::fs::read_to_string(root.join("Cargo.toml")) else {
        return vec!["Cargo.toml is missing".into()];
    };
    let Ok(doc) = text.parse::<toml::Table>() else {
        return vec!["Cargo.toml does not parse".into()];
    };
    if let Some(deps) = doc
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        for (name, spec) in deps {
            let version = match spec {
                toml::Value::String(v) => Some(v.as_str()),
                toml::Value::Table(t) => t.get("version").and_then(|v| v.as_str()),
                _ => None,
            };
            if let Some(v) = version
                && !v.starts_with('=')
            {
                problems.push(format!(
                    "Cargo.toml: workspace dependency `{name}` must be pinned exactly (`={v}`)"
                ));
            }
        }
    }
    for rel in crate::walk(&root.join("crates")).into_iter().chain(
        crate::walk(&root.join("xtask"))
            .into_iter()
            .map(|r| format!("../xtask/{r}")),
    ) {
        if !rel.ends_with("Cargo.toml") {
            continue;
        }
        let path = root.join("crates").join(&rel);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(doc) = text.parse::<toml::Table>() else {
            problems.push(format!("{}: does not parse", path.display()));
            continue;
        };
        for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
            if let Some(deps) = doc.get(section).and_then(|d| d.as_table()) {
                for (name, spec) in deps {
                    let inherits = spec
                        .as_table()
                        .and_then(|t| t.get("workspace"))
                        .and_then(|w| w.as_bool())
                        == Some(true);
                    if !inherits {
                        problems.push(format!("{rel}: `{name}` must use `workspace = true` (versions live in the root Cargo.toml)"));
                    }
                }
            }
        }
    }
    problems
}

fn check_submodule(root: &Path, problems: &mut Vec<String>) {
    let submodule = root.join("upstream").join("OpenColorIO");
    if !submodule.join(".git").exists() {
        problems.push("upstream/OpenColorIO is missing; run `git submodule update --init`".into());
        return;
    }
    // `--work-tree=.` keeps git from following the submodule's `core.worktree`, a path relative
    // to its git directory that doesn't resolve in the Rocky Linux 9 container when this
    // checkout is a linked worktree (`scripts/rocky9.sh` mounts the git directory itself).
    // Without git's repository-local variables: under `git rebase --exec`, GIT_DIR names the
    // superproject, and git would report its commit instead of the submodule's.
    let head = crate::clear_git_env(&mut std::process::Command::new("git"))
        .arg("-C")
        .arg(&submodule)
        .args(["--work-tree=.", "rev-parse", "HEAD"])
        .output();
    match head {
        Ok(out) if out.status.success() => {
            let commit = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if commit != crate::upstream::UPSTREAM_COMMIT {
                problems.push(format!(
                    "upstream/OpenColorIO is at {commit}, expected {}",
                    crate::upstream::UPSTREAM_COMMIT
                ));
            }
        }
        Ok(out) => problems.push(format!(
            "upstream/OpenColorIO: `git rev-parse HEAD` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(e) => problems.push(format!("upstream/OpenColorIO: could not run git: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lines `check_internals_uses` refuses in `text` at `rel`.
    fn refused(rel: &str, text: &str) -> Vec<usize> {
        check_internals_uses(rel, text)
            .iter()
            .map(|p| {
                let at = p.strip_prefix(&format!("{rel}:")).unwrap();
                at[..at.find(':').unwrap()].parse().unwrap()
            })
            .collect()
    }

    #[test]
    fn ocio_internals_only_in_ocio_s_integration_tests() {
        let uses = "use ocio::internals::build_ops;\n\
                    fn f() { ocio::internals::create_transform(g, op); }\n\
                    use ocio::{config::Config, internals};\n\
                    use ::ocio :: internals as i;\n\
                    use ocio as o;\nfn g() { o::internals::build_ops(); }\n\
                    use ocio::*;\nfn h() { r#internals::build_ops(); }\n";
        // Allowed in ocio's integration tests, wherever they put them.
        assert!(refused("crates/ocio/tests/common/transforms.rs", uses).is_empty());
        assert!(refused("crates/ocio/tests/a/b.rs", uses).is_empty());
        // Anywhere else, every use is refused, whatever path leads to it.
        for rel in [
            "crates/ocio-tools/src/main.rs",
            "crates/ocio-py/src/lib.rs",
            "crates/ocio-py/tests/t.rs",
            "crates/ocio/src/transform.rs",
            "crates/ocio/benches/b.rs",
            "crates/ocio/src/lib.rs",
            "xtask/src/main.rs",
        ] {
            assert_eq!(refused(rel, uses), [1, 2, 3, 4, 6, 8], "{rel}");
        }
        // A re-export from the crate itself.
        assert_eq!(
            refused(
                "crates/ocio/src/lib.rs",
                "pub use crate::internals::build_ops;\npub use self::internals as x;\n"
            ),
            [1, 2]
        );
    }

    #[test]
    fn ocio_internals_declared_only_in_lib_rs_and_gated() {
        let gated = "/// Docs.\n#[cfg(feature = \"internals\")]\n#[doc(hidden)]\npub mod internals {\n    \
                     use crate::config::Config;\n}\n";
        assert!(refused("crates/ocio/src/lib.rs", gated).is_empty());
        assert!(
            refused(
                "crates/ocio/src/lib.rs",
                "#[cfg(feature = \"internals\")] mod internals;\n"
            )
            .is_empty()
        );
        // Ungated, gated by something else, or gated away from the declaration.
        for text in [
            "pub mod internals {}\n",
            "#[cfg(not(feature = \"internals\"))]\npub mod internals {}\n",
            "#[cfg(any(feature = \"internals\", test))]\npub mod internals {}\n",
            "#[cfg(feature = \"internals\")]\nfn f() {}\n\npub mod internals {}\n",
        ] {
            assert_eq!(refused("crates/ocio/src/lib.rs", text).len(), 1, "{text}");
        }
        // Declared anywhere else.
        assert_eq!(refused("crates/ocio/src/transform.rs", gated), [4]);
        assert_eq!(refused("crates/ocio-ops/src/lib.rs", gated), [4]);
    }

    #[test]
    fn comments_and_literals_are_not_uses() {
        let text = "// ocio::internals in a comment\n\
                    /// [`ocio::internals`]\n\
                    /* ocio::internals /* nested */ internals */\n\
                    #[cfg_attr(not(feature = \"internals\"), allow(dead_code))]\n\
                    const A: &str = \"ocio::internals \\\" internals\";\n\
                    const B: &str = r#\"ocio::internals \" internals\"#;\n\
                    const C: &[u8] = b\"internals\";\n\
                    const D: char = '\"'; const E: char = '\''; fn l<'a>(x: &'a str) {}\n\
                    const F: &str = \"internals\";\n\
                    fn internals_x() { let internals_y = 1; }\n\
                    fn after() { ocio::internals::build_ops(); }\n";
        assert_eq!(refused("crates/ocio-tools/src/main.rs", text), [11]);
    }

    #[test]
    fn only_ocio_s_dev_dependency_on_itself_enables_internals() {
        let own = "[package]\nname = \"ocio\"\n\n[features]\ninternals = []\n\n\
                   [dev-dependencies]\nocio = { workspace = true, features = [\"internals\"] }\n";
        assert!(check_internals_feature(INTERNALS_MANIFEST, own).is_empty());
        // A manifest without the feature at all (before the feature existed).
        assert!(
            check_internals_feature(
                INTERNALS_MANIFEST,
                "[dependencies]\nocio-ops.workspace = true\n"
            )
            .is_empty()
        );
        let refused = |rel: &str, text: &str| check_internals_feature(rel, text).len();
        // Another crate's dependency on ocio, plain, renamed, per target or as a feature.
        let tools = "crates/ocio-tools/Cargo.toml";
        for text in [
            "[dependencies]\nocio = { workspace = true, features = [\"internals\"] }\n",
            "[dev-dependencies]\nocio = { workspace = true, features = [\"internals\"] }\n",
            "[dependencies]\nx = { package = \"ocio\", workspace = true, features = [\"internals\"] }\n",
            "[target.'cfg(windows)'.dev-dependencies]\nocio = { workspace = true, features = [\"internals\"] }\n",
            "[features]\nt = [\"ocio/internals\"]\n",
            "[features]\nt = [\"ocio?/internals\"]\n",
            "[dependencies]\nx = { package = \"ocio\", workspace = true }\n[features]\nt = [\"x/internals\"]\n",
        ] {
            assert_eq!(refused(tools, text), 1, "{text}");
        }
        // Another crate's feature named `internals`, or a feature of another dependency: fine.
        assert_eq!(
            refused(
                tools,
                "[features]\ninternals = []\nt = [\"serde/internals\"]\n"
            ),
            0
        );
        // The workspace's dependency, ocio's own regular dependency, and any other feature of
        // ocio (default included) that turns it on.
        assert_eq!(
            refused(
                "Cargo.toml",
                "[workspace.dependencies]\nocio = { path = \"crates/ocio\", features = [\"internals\"] }\n"
            ),
            1
        );
        assert_eq!(
            refused(
                INTERNALS_MANIFEST,
                "[dependencies]\nocio = { workspace = true, features = [\"internals\"] }\n"
            ),
            1
        );
        for features in [
            "default = [\"internals\"]",
            "x = [\"internals\"]",
            "x = [\"ocio/internals\"]",
        ] {
            assert_eq!(
                refused(
                    INTERNALS_MANIFEST,
                    &format!("[features]\ninternals = []\n{features}\n")
                ),
                1,
                "{features}"
            );
        }
    }

    #[test]
    fn this_workspace_passes() {
        let root = paths::workspace_root();
        for rel in rust_sources() {
            let text = std::fs::read_to_string(root.join(&rel)).unwrap();
            assert_eq!(check_internals_uses(&rel, &text), Vec::<String>::new());
        }
        for rel in manifests(root) {
            let text = std::fs::read_to_string(root.join(&rel)).unwrap();
            assert_eq!(check_internals_feature(&rel, &text), Vec::<String>::new());
        }
    }
}
