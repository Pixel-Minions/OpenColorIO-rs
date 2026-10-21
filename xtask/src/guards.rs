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

impl Waivers {
    /// Whether a waiver lets `file` contain the otherwise forbidden `pattern`.
    fn allow(&self, file: &str, pattern: &str) -> bool {
        self.waivers
            .iter()
            .flat_map(|w| &w.allow_patterns)
            .any(|a| a.file == file && a.pattern == pattern)
    }
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
    /// A bound in ulp per function (W0001), which the test kit's comparison reads
    /// (`ocio_testkit::battery::params`).
    #[serde(default)]
    pub(crate) bound_ulp: std::collections::BTreeMap<String, u64>,
    /// When and over what the bound was measured; required with `bound_ulp`.
    #[serde(default)]
    pub(crate) bound_measured: String,
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

/// Where `unsafe` may be allowed (PLAN.md D8): SIMD modules, the test kit's FFI to the
/// platform C runtime (the reference for C formatting and parsing), the one file that calls
/// `CreateSymbolicLinkW` and lists directories with `FindFirstFileW` as minizip-ng does (owner
/// decisions 2026-10-17), and, in the Python binding, the one file that calls
/// `PyType_FromMetaclass`, `PyInstanceMethod_New` (owner decision 2026-10-20, option A) and
/// `PyCFunction_NewEx` (2026-10-21), each named here by its file; [`check_type_object`] pins
/// that file's calls.
fn unsafe_allowed(rel: &str) -> bool {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    rel == "crates/ocio-py/src/type_object.rs"
        || rel == "crates/ocio-testkit/src/crt.rs"
        || rel == "crates/ocio/src/minizip/os_windows_ffi.rs"
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
        if !w.bound_ulp.is_empty() && w.bound_measured.is_empty() {
            problems.push(format!(
                "waivers.toml: waiver `{}` has a bound_ulp without bound_measured",
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
    let waived = |file: &str, pattern: &str| waivers.allow(file, pattern);

    let (sources, manifests) = workspace_files(root)?;
    problems.extend(check_files(&sources, &manifests, &waived));
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

/// Files as (path relative to the workspace root, text).
type Files = Vec<(String, String)>;

/// The workspace's Rust sources and manifests: what `run` hands `check_files`.
fn workspace_files(root: &Path) -> Result<(Files, Files), String> {
    let read = |rel: String| {
        std::fs::read_to_string(root.join(&rel))
            .map(|text| (rel.clone(), text))
            .map_err(|e| format!("{rel}: {e}"))
    };
    let sources = rust_sources()
        .into_iter()
        .map(read)
        .collect::<Result<_, _>>()?;
    let manifests = manifests(root)
        .into_iter()
        .map(read)
        .collect::<Result<_, _>>()?;
    Ok((sources, manifests))
}

/// The guards over the files' contents, the entry point `run` checks the workspace's Rust
/// sources and manifests through. `waived(file, pattern)`: whether a waiver lets `file`
/// contain the forbidden `pattern`.
fn check_files(
    sources: &[(String, String)],
    manifests: &[(String, String)],
    waived: &dyn Fn(&str, &str) -> bool,
) -> Vec<String> {
    let mut problems = Vec::new();
    for (rel, text) in sources {
        let rel = rel.as_str();
        if !text.starts_with("// SPDX-License-Identifier: BSD-3-Clause\n// Copyright Contributors to the OpenColorIO Project.\n") {
            problems.push(format!("{rel}: missing the SPDX / copyright header"));
        }
        problems.extend(check_internals_uses(rel, text));
        if rel == TYPE_OBJECT {
            problems.extend(check_type_object(rel, text));
        }
        if !(rel.starts_with("xtask/") && rel.ends_with("guards.rs")) && !unsafe_allowed(rel) {
            for line in unsafe_code_relaxed(text) {
                problems.push(format!(
                    "{rel}:{line}: `unsafe` is allowed only in SIMD modules and the files the \
                     allowlist names (PLAN.md D8)"
                ));
            }
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
                if code.contains(pattern) && !code.starts_with("//") && !waived(rel, pattern) {
                    problems.push(format!("{}: `{pattern}`: {why}", at()));
                }
            }
            if is_test_file && !rel.starts_with("crates/ocio-testkit/src/upstream/") {
                for pattern in TOLERANCE {
                    if code.contains(pattern) && !code.starts_with("//") && !waived(rel, pattern) {
                        problems.push(format!(
                            "{}: `{pattern}`: comparisons are exact; upstream tolerance checks go through ocio_testkit::upstream",
                            at()
                        ));
                    }
                }
            }
        }
    }

    for (rel, text) in manifests {
        problems.extend(check_internals_feature(rel, text));
        problems.extend(check_unsafe_lint(rel, text));
    }
    problems
}

/// The Python binding's one file with `unsafe` (owner decisions 2026-10-20 and 2026-10-21).
const TYPE_OBJECT: &str = "crates/ocio-py/src/type_object.rs";

/// What each `unsafe { ... }` block of [`TYPE_OBJECT`] may be: one of the owner's three calls,
/// or the wrapping of a call's result.
const TYPE_OBJECT_CALLS: [&str; 4] = [
    "ffi::PyType_FromMetaclass(",
    "PyInstanceMethod_New(",
    "ffi::PyCFunction_NewEx(",
    "Bound::from_owned_ptr_or_err(",
];

/// [`TYPE_OBJECT`]'s `unsafe`: blocks that each make one of [`TYPE_OBJECT_CALLS`], one
/// `unsafe extern "C"` block declaring `PyInstanceMethod_New` only, and no `unsafe fn`,
/// `unsafe impl` or `unsafe trait`.
fn check_type_object(rel: &str, text: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let code: String = text
        .lines()
        .map(|l| l.split("//").next().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    let line_of = |at: usize| code[..at].matches('\n').count() + 1;
    let mut i = 0;
    while let Some(found) = code[i..].find("unsafe") {
        let at = i + found;
        i = at + "unsafe".len();
        let rest = code[i..].trim_start();
        if let Some(body) = rest.strip_prefix('{') {
            let mut depth = 1;
            let end = body
                .char_indices()
                .find(|&(_, c)| {
                    match c {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    }
                    depth == 0
                })
                .map_or(body.len(), |(e, _)| e);
            let block = body[..end].trim();
            if !TYPE_OBJECT_CALLS.iter().any(|c| block.starts_with(c)) {
                problems.push(format!(
                    "{rel}:{}: an `unsafe` block that isn't one of the owner's calls: {block}",
                    line_of(at)
                ));
            }
        } else if let Some(body) = rest.strip_prefix("extern \"C\"") {
            let body = body.trim_start();
            let end = body.find('}').unwrap_or(body.len());
            let declared: Vec<&str> = body[..end]
                .split("fn ")
                .skip(1)
                .map(|d| d.split('(').next().unwrap_or_default().trim())
                .collect();
            if declared != ["PyInstanceMethod_New"] {
                problems.push(format!(
                    "{rel}:{}: an `unsafe extern` block declaring {declared:?}",
                    line_of(at)
                ));
            }
        } else if rest.starts_with("fn ") || rest.starts_with("impl") || rest.starts_with("trait") {
            problems.push(format!("{rel}:{}: `unsafe` {rest:.20}", line_of(at)));
        }
    }
    problems
}

/// The manifests' part of the `unsafe` rule: the workspace denies the `unsafe_code` lint, and
/// every crate and xtask takes the workspace's lints, so that `unsafe` anywhere (integration
/// tests, benches, xtask included) needs an `allow`, which only the allowlisted files may hold
/// ([`unsafe_allowed`]).
fn check_unsafe_lint(rel: &str, text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let has_after = |header: &str, line: &str| {
        lines
            .iter()
            .position(|l| *l == header)
            .is_some_and(|start| {
                lines[start + 1..]
                    .iter()
                    .take_while(|l| !l.starts_with('['))
                    .any(|l| *l == line)
            })
    };
    if rel == "Cargo.toml" {
        if has_after("[workspace.lints.rust]", "unsafe_code = \"deny\"") {
            return vec![];
        }
        return vec![format!(
            "{rel}: [workspace.lints.rust] must deny `unsafe_code` (PLAN.md D8)"
        )];
    }
    if has_after("[lints]", "workspace = true") {
        return vec![];
    }
    vec![format!(
        "{rel}: [lints] must be `workspace = true`, the workspace's (they deny `unsafe_code`)"
    )]
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

/// The lines of the attributes in `text` that relax the `unsafe_code` lint: an `allow`,
/// `expect` or `warn` of it, alone or among other lints, in an inner or outer attribute, on
/// one line or several, or inside a `cfg_attr`. Attributes in comments don't count.
fn unsafe_code_relaxed(text: &str) -> Vec<usize> {
    let mut lines = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while let Some(found) = text[i..].find('#') {
        let start = i + found;
        i = start + 1;
        let mut at = start + 1;
        if bytes.get(at) == Some(&b'!') {
            at += 1;
        }
        if bytes.get(at) != Some(&b'[') {
            continue;
        }
        // Skip attributes in a comment: the line up to the `#` starts a comment.
        let line_start = text[..start].rfind('\n').map_or(0, |n| n + 1);
        let before = text[line_start..start].trim_start();
        if before.starts_with("//") || before.contains("/*") {
            continue;
        }
        // The attribute's text, to its matching `]`.
        let mut depth = 0usize;
        let mut end = at;
        for (k, &c) in bytes[at..].iter().enumerate() {
            match c {
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = at + k;
                        break;
                    }
                }
                _ => {}
            }
        }
        let attribute: String = text[at..=end.max(at)]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if attribute.contains("unsafe_code")
            && ["allow(", "expect(", "warn("]
                .iter()
                .any(|level| attribute.contains(level))
        {
            lines.push(text[..start].matches('\n').count() + 1);
        }
    }
    lines
}

/// `ocio::internals` is for the crate's own integration tests only. `cargo clippy --workspace
/// --all-targets` and `cargo test --workspace` unify the feature that ocio's dev-dependency on
/// itself enables into every crate's build, so a use elsewhere (or a re-export from the crate)
/// would build there, and fail only in a plain `cargo build`.
///
/// Outside `crates/ocio/tests/`, the identifier `internals` may appear in code (not comments or
/// literals) only as `crates/ocio/src/lib.rs`'s `mod internals`, gated by
/// `#[cfg(feature = "internals")]` (`gated`). Any other identifier `internals`, whatever path or
/// alias leads to it (`ocio::internals`, `use ocio::{internals, ..}`, `use ocio as o;
/// o::internals`, `use ocio::*; internals::..`, `pub use crate::internals::..`, even gated), is
/// refused. Other identifiers that contain the word (`_internals`, `internals_x`) can't name
/// the module, and are not.
fn check_internals_uses(rel: &str, text: &str) -> Vec<String> {
    if rel.starts_with(INTERNALS_USERS) {
        return Vec::new();
    }
    let Lexed { idents, code } = lex(text);
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
        } else if !gated(&code, *line) {
            problems.push(format!(
                "{rel}:{line}: `mod {INTERNALS}` must be gated by `{INTERNALS_GATE}`, so that a \
                 build of the library never has it"
            ));
        }
    }
    problems
}

/// Whether the item declared on `line` (1-based) of `code` (the source with its comments
/// blanked, `Lexed::code`) carries `INTERNALS_GATE`: a line that is exactly that attribute,
/// among the attribute lines directly above the declaration (blank and comment lines between
/// them are skipped). rustfmt, which the gate runs, puts every outer attribute on a line of its
/// own, so an attribute on the declaration's own line doesn't count, nor one of another item.
fn gated(code: &str, line: usize) -> bool {
    let lines: Vec<&str> = code.lines().collect();
    for above in lines[..line.saturating_sub(1).min(lines.len())]
        .iter()
        .rev()
    {
        let above = above.trim();
        if above == INTERNALS_GATE {
            return true;
        }
        if !(above.is_empty() || (above.starts_with("#[") && above.ends_with(']'))) {
            return false;
        }
    }
    false
}

/// Rust source, lexed.
struct Lexed {
    /// Its identifiers, with their 1-based lines, outside comments and string, byte-string,
    /// raw-string and character literals. A raw identifier `r#x` is `x`.
    idents: Vec<(String, usize)>,
    /// The source with every comment's characters (doc comments too) blanked to spaces, its
    /// lines kept.
    code: String,
}

/// Lexes `text` just enough for the guards: see `Lexed`.
fn lex(text: &str) -> Lexed {
    let c: Vec<char> = text.chars().collect();
    let mut code = c.clone();
    let mut blank = |from: usize, to: usize| {
        for ch in &mut code[from..to.min(c.len())] {
            if *ch != '\n' {
                *ch = ' ';
            }
        }
    };
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
            let start = i;
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            blank(start, i);
        } else if ch == '/' && next == Some('*') {
            // Block comments nest.
            let start = i;
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
            blank(start, i);
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
    Lexed {
        idents: out,
        code: code.into_iter().collect(),
    }
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

    #[test]
    fn unsafe_code_relaxed_in_any_lint_attribute() {
        let cases: [(&str, &[usize]); 9] = [
            ("#![allow(unsafe_code)]\n", &[1]),
            ("#[allow(dead_code, unsafe_code)]\nfn f() {}\n", &[1]),
            ("#[expect(unsafe_code)]\nfn f() {}\n", &[1]),
            ("#![warn(unsafe_code)]\n", &[1]),
            (
                "fn f() {}\n#[allow(\n    dead_code,\n    unsafe_code\n)]\nfn g() {}\n",
                &[2],
            ),
            ("#[cfg_attr(windows, allow(unsafe_code))]\nmod m;\n", &[1]),
            ("#![deny(unsafe_code)]\n#![forbid(unsafe_code)]\n", &[]),
            (
                "// #[allow(unsafe_code)]\n/// #![allow(unsafe_code)]\n",
                &[],
            ),
            ("#[allow(dead_code)]\nfn unsafe_code() {}\n", &[]),
        ];
        for (text, lines) in cases {
            assert_eq!(unsafe_code_relaxed(text), lines, "{text}");
        }
    }

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
    fn manifests_take_the_workspace_lints_which_deny_unsafe_code() {
        let root = "[workspace.lints.rust]\nunsafe_code = \"deny\"\nunreachable_pub = \"warn\"\n";
        assert!(check_unsafe_lint("Cargo.toml", root).is_empty());
        for text in [
            "[workspace.lints.rust]\nunreachable_pub = \"warn\"\n",
            "[workspace.lints.rust]\nunsafe_code = \"warn\"\n",
            "[workspace.lints.clippy]\nunsafe_code = \"deny\"\n",
        ] {
            assert_eq!(check_unsafe_lint("Cargo.toml", text).len(), 1, "{text}");
        }
        let krate = "[package]\nname = \"a\"\n\n[lints]\nworkspace = true\n";
        assert!(check_unsafe_lint("crates/a/Cargo.toml", krate).is_empty());
        for text in [
            "[package]\nname = \"a\"\n",
            "[lints]\n\n[dependencies]\nworkspace = true\n",
            "[lints]\nworkspace = false\n",
        ] {
            assert_eq!(
                check_unsafe_lint("xtask/Cargo.toml", text).len(),
                1,
                "{text}"
            );
        }
    }

    #[test]
    fn type_object_rs_makes_only_the_owners_calls() {
        let good = "#[cfg_attr(windows, link(name = \"python313\", kind = \"raw-dylib\"))]\n\
                    unsafe extern \"C\" {\n    fn PyInstanceMethod_New(f: *mut ffi::PyObject) \
                    -> *mut ffi::PyObject;\n}\n\
                    fn a() {\n    // SAFETY: unsafe { anything } in a comment.\n    \
                    let p = unsafe { ffi::PyType_FromMetaclass(m, n, s, b) };\n    \
                    let q = unsafe { PyInstanceMethod_New(f.as_ptr()) };\n    \
                    let r = unsafe {\n        ffi::PyCFunction_NewEx(d, s, m)\n    };\n    \
                    unsafe { Bound::from_owned_ptr_or_err(py, p) }\n}\n";
        assert!(
            check_type_object(TYPE_OBJECT, good).is_empty(),
            "{:?}",
            check_type_object(TYPE_OBJECT, good)
        );
        for bad in [
            "fn a() { let x = unsafe { *p }; }",
            "fn a() { unsafe { ffi::PyType_GetSlot(t, 1) } }",
            "unsafe extern \"C\" {\n    fn PyInstanceMethod_New(f: *mut O) -> *mut O;\n    fn Py_Other() -> i32;\n}",
            "unsafe extern \"C\" {\n    fn Py_Other() -> i32;\n}",
            "unsafe fn f() {}",
            "unsafe impl Send for X {}",
        ] {
            assert_eq!(check_type_object(TYPE_OBJECT, bad).len(), 1, "{bad}");
        }
        // The file itself.
        let text = std::fs::read_to_string(paths::workspace_root().join(TYPE_OBJECT)).unwrap();
        assert!(check_type_object(TYPE_OBJECT, &text).is_empty());
    }

    #[test]
    fn unsafe_allowed_in_the_python_binding_only_in_type_object_rs() {
        assert!(unsafe_allowed("crates/ocio-py/src/type_object.rs"));
        for rel in [
            "crates/ocio-py/src/lib.rs",
            "crates/ocio-py/src/pybind11.rs",
            "crates/ocio-py/src/type_object/x.rs",
            "crates/ocio-py/tests/t.rs",
        ] {
            assert!(!unsafe_allowed(rel), "{rel}");
        }
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
        // Blank lines and comments between the attributes and the declaration don't detach
        // them.
        assert!(
            refused(
                "crates/ocio/src/lib.rs",
                "#[cfg(feature = \"internals\")]\n\n// Note.\n/* More. */\n#[doc(hidden)]\n\
                 mod internals;\n"
            )
            .is_empty()
        );
        // Ungated, gated by something else, or the gate another item's, a comment's, or on the
        // declaration's own line (which rustfmt never leaves there).
        for text in [
            "pub mod internals {}\n",
            "#[cfg(not(feature = \"internals\"))]\npub mod internals {}\n",
            "#[cfg(any(feature = \"internals\", test))]\npub mod internals {}\n",
            "#[cfg(feature = \"internals\")]\nfn f() {}\n\npub mod internals {}\n",
            "#[cfg(feature = \"internals\")] fn f() {}\npub mod internals {}\n",
            "#[cfg(feature = \"internals\")]\n#[doc(hidden)] fn f() {}\npub mod internals {}\n",
            "pub mod internals {} // #[cfg(feature = \"internals\")]\n",
            "// #[cfg(feature = \"internals\")]\npub mod internals {}\n",
            "/// #[cfg(feature = \"internals\")]\npub mod internals {}\n",
            "/* #[cfg(feature = \"internals\")] */\npub mod internals {}\n",
            "#[cfg(feature = \"internals\")] pub mod internals {}\n",
            "#[cfg(feature = \"internals\")] mod internals;\n",
        ] {
            assert_eq!(refused("crates/ocio/src/lib.rs", text).len(), 1, "{text}");
        }
        // Gated or not, nothing else in lib.rs may name it: no re-export.
        assert_eq!(
            refused(
                "crates/ocio/src/lib.rs",
                "#[cfg(feature = \"internals\")]\npub mod internals {}\n\
                 #[cfg(feature = \"internals\")]\npub use internals::build_ops;\n"
            ),
            [4]
        );
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
                    const G: &core::ffi::CStr = cr#\"ocio::internals \" internals\"#;\n\
                    const H: &core::ffi::CStr = c\"internals\"; const I: &[u8] = br\"internals\";\n\
                    fn _internals() { let _internals = 1; }\n\
                    fn after() { ocio::internals::build_ops(); }\n";
        // Identifiers that merely contain the word (`internals_x`, `_internals`) can't name the
        // module: allowed.
        assert_eq!(refused("crates/ocio-tools/src/main.rs", text), [14]);
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
            "[build-dependencies]\nocio = { workspace = true, features = [\"internals\"] }\n",
            "[dependencies]\nocio = { workspace = true, features = [\"x\", \"internals\"] }\n",
            "[dependencies]\nx = { package = \"ocio\", workspace = true, features = [\"internals\"] }\n",
            "[target.'cfg(windows)'.dev-dependencies]\nocio = { workspace = true, features = [\"internals\"] }\n",
            "[features]\nt = [\"ocio/internals\"]\n",
            "[features]\nt = [\"ocio?/internals\"]\n",
            "[dependencies]\nx = { package = \"ocio\", workspace = true }\n[features]\nt = [\"x/internals\"]\n",
        ] {
            assert_eq!(refused(tools, text), 1, "{text}");
            // xtask's manifest is no exception.
            assert_eq!(refused("xtask/Cargo.toml", text), 1, "{text}");
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
    fn run_checks_sources_and_manifests_for_internals() {
        // Through check_files, the entry point `run` checks the workspace's files through.
        let file = |rel: &str, text: &str| (rel.to_string(), text.to_string());
        let header = "// SPDX-License-Identifier: BSD-3-Clause\n\
                      // Copyright Contributors to the OpenColorIO Project.\n";
        let sources = [
            file(
                "crates/ocio/tests/t.rs",
                &format!("{header}use ocio::internals::build_ops;\n"),
            ),
            file(
                "crates/ocio-tools/src/main.rs",
                &format!("{header}use ocio::internals::build_ops;\n"),
            ),
        ];
        let manifests = [
            file(
                INTERNALS_MANIFEST,
                "[dev-dependencies]\nocio = { workspace = true, features = [\"internals\"] }\n\n\
                 [lints]\nworkspace = true\n",
            ),
            file(
                "crates/ocio-py/Cargo.toml",
                "[dependencies]\nocio = { workspace = true, features = [\"internals\"] }\n\n\
                 [lints]\nworkspace = true\n",
            ),
        ];
        let problems = check_files(&sources, &manifests, &|_, _| false);
        assert_eq!(problems.len(), 2, "{problems:#?}");
        assert!(problems[0].starts_with("crates/ocio-tools/src/main.rs:3: `internals`"));
        assert!(problems[1].starts_with("crates/ocio-py/Cargo.toml: [dependencies] `ocio`"));
    }

    #[test]
    fn this_workspace_passes() {
        let waivers = load_waivers().unwrap();
        let waived = |file: &str, pattern: &str| waivers.allow(file, pattern);
        let (sources, manifests) = workspace_files(paths::workspace_root()).unwrap();
        assert!(sources.iter().any(|(rel, _)| rel == INTERNALS_HOME));
        assert!(manifests.iter().any(|(rel, _)| rel == INTERNALS_MANIFEST));
        assert!(manifests.iter().any(|(rel, _)| rel == "Cargo.toml"));
        assert_eq!(
            check_files(&sources, &manifests, &waived),
            Vec::<String>::new()
        );
    }
}
