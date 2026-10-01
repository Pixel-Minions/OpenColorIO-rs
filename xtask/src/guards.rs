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
