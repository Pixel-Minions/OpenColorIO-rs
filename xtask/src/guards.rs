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

/// Where `unsafe` may be allowed (PLAN.md D8): SIMD modules (and the test file of one of
/// them, `sse_tests.rs`, coordinator's decision 2026-10-21), the test kit's FFI to the
/// platform C runtime (the reference for C formatting and parsing), the one file that calls
/// `CreateSymbolicLinkW` and lists directories with `FindFirstFileW` as minizip-ng does (owner
/// decisions 2026-10-17), and, for the Python binding, the crate `ocio-py-ffi`, one file that
/// calls `PyType_FromMetaclass`, `PyInstanceMethod_New` (owner decision 2026-10-20, option A),
/// `PyCFunction_NewEx` and `PySequence_Check` (2026-10-21), `PyType_GetSlot` for four number
/// slots (2026-10-21, p6-core re-check round 4), and the views of Python buffers (P6-10), each
/// named here by its file;
/// [`check_py_ffi`] checks that file's calls and [`PY_FFI_SHA256`] pins it. The keyword
/// `unsafe` appears nowhere else in the workspace's Rust sources ([`unsafe_tokens`]), and these
/// files pull in no other source ([`source_problems`]).
fn unsafe_allowed(rel: &str) -> bool {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    rel == PY_FFI
        || rel == "crates/ocio-testkit/src/crt.rs"
        || rel == "crates/ocio/src/minizip/os_windows_ffi.rs"
        || rel == "crates/ocio-ops/src/sse_tests.rs"
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

/// The workspace's members, as the root manifest's `[workspace] members` lists them (paths
/// relative to the root; globs are refused, so that the list is the members).
fn workspace_members(root_manifest: &str) -> Result<Vec<String>, String> {
    let doc = root_manifest
        .parse::<toml::Table>()
        .map_err(|e| format!("Cargo.toml: {e}"))?;
    let members = doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .ok_or("Cargo.toml: no [workspace] members")?;
    members
        .iter()
        .map(|m| match m.as_str() {
            Some(m)
                if !m.is_empty()
                    && !m.contains(['*', '?', '[', '\\'])
                    && !m.split('/').any(|c| c.is_empty() || c == "." || c == "..") =>
            {
                Ok(m.to_string())
            }
            _ => Err(format!(
                "Cargo.toml: workspace member `{m}`: the guards take plain paths below the root                  (no globs, `.` or `..`)"
            )),
        })
        .collect()
}

/// The files below the members' directories whose names satisfy `keep`, relative to the root,
/// each once (a member may hold another), without a member's own `target` directory.
fn member_files(root: &Path, members: &[String], keep: impl Fn(&str) -> bool) -> Vec<String> {
    let mut files = std::collections::BTreeSet::new();
    for member in members {
        for rel in crate::walk(&root.join(member)) {
            let rel = format!("{member}/{rel}");
            let in_target = members
                .iter()
                .any(|m| rel.starts_with(&format!("{m}/target/")));
            if keep(&rel) && !in_target {
                files.insert(rel);
            }
        }
    }
    files.into_iter().collect()
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
    problems.extend(check_dependency_pins(&manifests));
    problems.extend(crate::registers::check(root));
    problems.extend(crate::upstream::validate(&crate::upstream::load_map()?));
    check_submodule(root, &mut problems);
    check_oracle_cache_off(root, &mut problems);

    if problems.is_empty() {
        println!("guards: ok");
        Ok(())
    } else {
        Err(format!("guards failed:\n  {}", problems.join("\n  ")))
    }
}

/// Files as (path relative to the workspace root, text).
type Files = Vec<(String, String)>;

/// The workspace's Rust sources and manifests, the files below its members' directories
/// ([`workspace_members`]; the root manifest first): what `run` hands `check_files`.
fn workspace_files(root: &Path) -> Result<(Files, Files), String> {
    let read = |rel: String| {
        std::fs::read_to_string(root.join(&rel))
            .map(|text| (rel.clone(), text))
            .map_err(|e| format!("{rel}: {e}"))
    };
    let (_, root_manifest) = read("Cargo.toml".to_string())?;
    let members = workspace_members(&root_manifest)?;
    let sources = member_files(root, &members, |rel| rel.ends_with(".rs"))
        .into_iter()
        .map(read)
        .collect::<Result<_, _>>()?;
    let manifests = std::iter::once(Ok(("Cargo.toml".to_string(), root_manifest)))
        .chain(
            member_files(root, &members, |rel| {
                rel.rsplit('/').next() == Some("Cargo.toml")
            })
            .into_iter()
            .map(read),
        )
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
    let scanned: std::collections::BTreeSet<&str> =
        sources.iter().map(|(rel, _)| rel.as_str()).collect();
    for (rel, text) in sources {
        let rel = rel.as_str();
        if !text.starts_with("// SPDX-License-Identifier: BSD-3-Clause\n// Copyright Contributors to the OpenColorIO Project.\n") {
            problems.push(format!("{rel}: missing the SPDX / copyright header"));
        }
        problems.extend(check_internals_uses(rel, text));
        if rel == PY_FFI {
            problems.extend(check_py_ffi(rel, text));
            problems.extend(py_ffi_pin_problem(text));
        } else if rel.starts_with(PY_FFI_DIR) {
            problems.push(format!(
                "{rel}: the Python binding's unsafe code is the one file {PY_FFI}"
            ));
        }
        if rel == PY_ROOT && !forbids_unsafe_code(text) {
            problems.push(format!(
                "{rel}: the Python binding's crate root must hold `#![forbid(unsafe_code)]`, a \
                 top-level inner attribute"
            ));
        }
        if rel == PY_BUILD_SCRIPT {
            problems.push(format!(
                "{rel}: the Python binding's crates have no build script"
            ));
        }
        if !unsafe_allowed(rel) {
            for line in unsafe_code_relaxed(text) {
                problems.push(format!(
                    "{rel}:{line}: `unsafe` is allowed only in SIMD modules and the files the \
                     allowlist names (PLAN.md D8)"
                ));
            }
            for line in unsafe_tokens(text) {
                problems.push(format!(
                    "{rel}:{line}: the keyword `unsafe` appears only in SIMD modules and the \
                     files the allowlist names (PLAN.md D8), not even in a macro"
                ));
            }
        }
        if rel != PY_FFI {
            problems.extend(source_problems(rel, text, &scanned));
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
        problems.extend(check_py_manifest(rel, text));
        problems.extend(check_manifest_sources(rel, text, manifests));
    }
    problems
}

/// The build script `ocio-py` may not have: Cargo builds and runs a `build.rs` at a crate's
/// root without any key in the manifest. (The ffi crate's, like any other file of that crate
/// but [`PY_FFI`], is refused already.)
const PY_BUILD_SCRIPT: &str = "crates/ocio-py/build.rs";

/// The lines of the keyword `unsafe` in `text`'s code (comments and literals aside; a raw
/// identifier `r#unsafe` too). Outside the allowlisted files ([`unsafe_allowed`]) there is
/// none: a macro defined elsewhere that expands to `unsafe` code would otherwise put it where
/// no file holds an `allow(unsafe_code)` (rustc doesn't lint `unsafe` from a macro of another
/// crate of the workspace; p6-core's re-check round 4, M1).
fn unsafe_tokens(text: &str) -> Vec<usize> {
    tokenize(text)
        .tokens
        .iter()
        .filter(|t| t.kind == TokenKind::Ident && t.text == "unsafe")
        .map(|t| t.line)
        .collect()
}

/// A source file that Rust code pulls in by name: `path = "<file>"` inside an attribute
/// (`#[path = ..]`, in a `cfg_attr` too), or a macro `include!`, `include_str!` or
/// `include_bytes!`.
struct SourceRef {
    /// The index of the token `path` or of the macro's name.
    at: usize,
    line: usize,
    /// `path`, `include`, `include_str` or `include_bytes`.
    what: String,
    /// The plain string literal that names the file, if that's what names it.
    value: Option<String>,
    /// Outside any `{ }` (an inline module, a function, a macro's braces): where rustc
    /// resolves a `#[path]` relative to the file's directory.
    top_level: bool,
}

/// The [`SourceRef`]s of `toks`.
fn source_refs(toks: &[Token]) -> Vec<SourceRef> {
    let text_at = |k: usize| toks.get(k).map_or("", |t| t.text.as_str());
    let string_at = |k: usize| toks.get(k).and_then(plain_string).map(str::to_string);
    // The open groups: (opening token, an attribute's `[`).
    let mut groups: Vec<(&str, bool)> = Vec::new();
    let mut refs = Vec::new();
    for (k, t) in toks.iter().enumerate() {
        let top_level = !groups.iter().any(|g| g.0 == "{");
        match t.text.as_str() {
            "(" | "[" | "{" if t.kind == TokenKind::Punct => {
                let attribute = t.text == "["
                    && (text_at(k.wrapping_sub(1)) == "#"
                        || (text_at(k.wrapping_sub(1)) == "!"
                            && text_at(k.wrapping_sub(2)) == "#"));
                groups.push((t.text.as_str(), attribute));
            }
            ")" | "]" | "}" if t.kind == TokenKind::Punct => {
                groups.pop();
            }
            "path"
                if t.kind == TokenKind::Ident
                    && text_at(k + 1) == "="
                    && groups.iter().any(|g| g.1) =>
            {
                refs.push(SourceRef {
                    at: k,
                    line: t.line,
                    what: t.text.clone(),
                    value: string_at(k + 2),
                    top_level,
                });
            }
            "include" | "include_str" | "include_bytes"
                if t.kind == TokenKind::Ident && text_at(k + 1) == "!" =>
            {
                let enclosed =
                    ["(", "[", "{"].contains(&text_at(k + 2)) && matching(toks, k + 2) == k + 4;
                refs.push(SourceRef {
                    at: k,
                    line: t.line,
                    what: t.text.clone(),
                    value: if enclosed { string_at(k + 3) } else { None },
                    top_level,
                });
            }
            _ => {}
        }
    }
    refs
}

/// The text of a plain string literal (`"..."`, without escapes).
fn plain_string(t: &Token) -> Option<&str> {
    let inner = t.text.strip_prefix('"')?.strip_suffix('"')?;
    (t.kind == TokenKind::Literal && !inner.contains(['\\', '"'])).then_some(inner)
}

/// The path, relative to the root, that `value` names from the file `rel`: relative to `rel`'s
/// directory, which is where rustc resolves an `include!`, and a `#[path]` outside any inline
/// module. `None` for an absolute path, one with a backslash or a drive, or one that leaves the
/// root.
fn resolve(rel: &str, value: &str) -> Option<String> {
    if value.starts_with('/') || value.contains(['\\', ':']) {
        return None;
    }
    let mut parts: Vec<&str> = rel.split('/').collect();
    parts.pop();
    for part in value.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// The guards on the source files `text` (at `rel`) pulls in by name ([`SourceRef`]), so that
/// every Rust file the workspace compiles is one of the files the guards scan (`scanned`):
/// - everywhere, a `#[path]` outside any inline module and an `include!` name, by a plain
///   string literal, a file among `scanned` (the `.rs` files below the members);
/// - an allowlisted file ([`unsafe_allowed`]; the ffi crate's has its own rules) declares no
///   out-of-line module and pulls in no file, but for its own test module in exactly the form
///   `#[cfg(test)] #[path = "<its stem>_tests.rs"] (pub(crate))? mod <name>;` (coordinator's
///   decision 2026-10-21): no other `mod x;`, `#[path]` or `include!`, `include_str!` or
///   `include_bytes!` (p6-core's re-check round 4: a `mod child;` in crt.rs compiled a child
///   with bare `unsafe` code).
fn source_problems(
    rel: &str,
    text: &str,
    scanned: &std::collections::BTreeSet<&str>,
) -> Vec<String> {
    let toks = tokenize(text).tokens;
    let refs = source_refs(&toks);
    let mut problems = Vec::new();
    let names_scanned = |r: &SourceRef| {
        r.value
            .as_deref()
            .and_then(|v| resolve(rel, v))
            .is_some_and(|p| scanned.contains(p.as_str()))
    };
    for r in &refs {
        let code = r.what == "path" || r.what == "include";
        if code && !(names_scanned(r) && (r.what == "include" || r.top_level)) {
            problems.push(format!(
                "{rel}:{}: `{}` of {}: Rust code pulled in by name is one of the workspace's \
                 sources the guards scan, named by a plain string literal (a `#[path]` outside \
                 any inline module)",
                r.line,
                r.what,
                r.value
                    .as_deref()
                    .unwrap_or("something other than a plain string literal"),
            ));
        }
    }
    if !unsafe_allowed(rel) {
        return problems;
    }
    let text_at = |k: usize| toks.get(k).map_or("", |t| t.text.as_str());
    let stem = rel
        .rsplit('/')
        .next()
        .unwrap_or(rel)
        .trim_end_matches(".rs");
    let test_file = format!("\"{stem}_tests.rs\"");
    let mut approved = Vec::new();
    for (k, t) in toks.iter().enumerate() {
        let declaration = t.kind == TokenKind::Ident
            && t.text == "mod"
            && toks.get(k + 1).is_some_and(|t| t.kind == TokenKind::Ident)
            && text_at(k + 2) == ";";
        if !declaration {
            continue;
        }
        let visibility = if k >= 4 && (k - 4..k).map(text_at).eq(["pub", "(", "crate", ")"]) {
            4
        } else {
            0
        };
        // `#[allow(unsafe_code)]` between the two only for a test file that is itself
        // allowlisted (sse.rs's sse_tests.rs, which has no `allow` of its own).
        let sibling_allowed =
            resolve(rel, &format!("{stem}_tests.rs")).is_some_and(|p| unsafe_allowed(&p));
        let cfg = ["#", "[", "cfg", "(", "test", ")", "]"];
        let allow = ["#", "[", "allow", "(", "unsafe_code", ")", "]"];
        let path_attr = ["#", "[", "path", "=", &test_file, "]"];
        let forms = [
            cfg.iter().chain(&path_attr).copied().collect::<Vec<&str>>(),
            cfg.iter()
                .chain(&allow)
                .chain(&path_attr)
                .copied()
                .collect(),
        ];
        let fitting = forms
            .iter()
            .take(1 + usize::from(sibling_allowed))
            .find(|form| {
                k.checked_sub(visibility + form.len())
                    .is_some_and(|s| (s..s + form.len()).map(text_at).eq(form.iter().copied()))
            });
        // The token `path`, four before the declaration (or its visibility).
        let path = fitting.map(|_| k - visibility - 4);
        let fits = path.is_some()
            && refs
                .iter()
                .any(|r| Some(r.at) == path && r.top_level && names_scanned(r));
        if fits {
            approved.extend(path);
        } else {
            problems.push(format!(
                "{rel}:{}: `mod {};`: an allowlisted file declares no out-of-line module but its \
                 own tests, `#[cfg(test)] #[path = {test_file}] mod <name>;`",
                t.line,
                text_at(k + 1)
            ));
        }
    }
    for r in &refs {
        if !approved.contains(&r.at) {
            problems.push(format!(
                "{rel}:{}: `{}`: an allowlisted file pulls in no other file (but its own tests, \
                 `#[cfg(test)] #[path = {test_file}] mod <name>;`)",
                r.line, r.what
            ));
        }
    }
    problems
}

/// The manifests' part of the rule that every Rust file the workspace compiles is one the
/// guards scan ([`workspace_files`]: the files below the members' directories):
/// - a target's `path` and a `build` script stay below the member's directory;
/// - no crate is a procedural macro (`proc-macro = true`), whose code would run in rustc and
///   could make `unsafe` code no source holds;
/// - the root manifest's path dependencies are members (Cargo makes a path dependency below the
///   root a member without listing it), each member has a manifest the guards read, and the
///   root has no `[patch]` or `[replace]`.
fn check_manifest_sources(rel: &str, text: &str, manifests: &[(String, String)]) -> Vec<String> {
    let Ok(doc) = text.parse::<toml::Table>() else {
        // check_dependency_pins reports it.
        return Vec::new();
    };
    let mut problems = Vec::new();
    if rel == "Cargo.toml" {
        let members = match workspace_members(text) {
            Ok(members) => members,
            Err(e) => return vec![e],
        };
        for key in ["patch", "replace"] {
            if doc.contains_key(key) {
                problems.push(format!(
                    "{rel}: [{key}]: the workspace builds its members and pinned crates only"
                ));
            }
        }
        for (name, spec) in doc
            .get("workspace")
            .and_then(|w| w.get("dependencies"))
            .and_then(|d| d.as_table())
            .into_iter()
            .flatten()
        {
            if let Some(path) = spec.get("path").and_then(|p| p.as_str())
                && !members.iter().any(|m| m == path)
            {
                problems.push(format!(
                    "{rel}: workspace dependency `{name}`'s path `{path}` isn't a workspace member"
                ));
            }
        }
        for member in &members {
            let manifest = format!("{member}/Cargo.toml");
            if !manifests.iter().any(|(m, _)| *m == manifest) {
                problems.push(format!("{rel}: member `{member}` has no {manifest}"));
            }
        }
        return problems;
    }
    let dir = rel.trim_end_matches("Cargo.toml");
    let mut paths: Vec<(String, &toml::Value)> = Vec::new();
    if let Some(build) = doc.get("package").and_then(|p| p.get("build")) {
        paths.push(("package.build".to_string(), build));
    }
    if let Some(lib) = doc.get("lib") {
        if lib.get("proc-macro").is_some() || lib.get("proc_macro").is_some() {
            problems.push(format!(
                "{rel}: [lib] proc-macro: the workspace has no procedural macro crates"
            ));
        }
        if let Some(path) = lib.get("path") {
            paths.push(("lib.path".to_string(), path));
        }
    }
    for kind in ["bin", "test", "example", "bench"] {
        for target in doc
            .get(kind)
            .and_then(|t| t.as_array())
            .into_iter()
            .flatten()
        {
            if let Some(path) = target.get("path") {
                paths.push((format!("{kind}.path"), path));
            }
        }
    }
    for (key, value) in paths {
        let inside = match value {
            toml::Value::String(v) => {
                resolve(rel, v).is_some_and(|p| p.starts_with(dir) && p.len() > dir.len())
            }
            toml::Value::Boolean(_) => key == "package.build",
            _ => false,
        };
        if !inside {
            problems.push(format!(
                "{rel}: {key} = {value}: a crate's sources stay below its directory {dir}"
            ));
        }
    }
    problems
}

/// The Python binding's manifests: `ocio-py`'s and its ffi crate's targets are the default
/// ones (no `path`, no `build` script, no `[[...]]` target), so that [`PY_ROOT`] is the
/// binding's crate root and [`PY_FFI`] the ffi crate's.
fn check_py_manifest(rel: &str, text: &str) -> Vec<String> {
    if rel != "crates/ocio-py/Cargo.toml" && rel != format!("{PY_FFI_DIR}Cargo.toml") {
        return Vec::new();
    }
    let lines = py_manifest_lines(rel, text);
    if !lines.is_empty() {
        return lines;
    }
    // The same keys in forms no line starts with: dotted (`package.build = ..`) or inline
    // (`lib = { path = .. }`, `bin = [..]`).
    let Ok(doc) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let keys = [
        doc.get("package")
            .and_then(|p| p.get("build"))
            .map(|_| "package.build"),
        doc.get("lib")
            .and_then(|l| l.get("path"))
            .map(|_| "lib.path"),
    ];
    keys.into_iter()
        .flatten()
        .chain(
            ["bin", "test", "example", "bench"]
                .into_iter()
                .filter(|k| doc.contains_key(*k)),
        )
        .map(|key| {
            format!(
                "{rel}: `{key}`: the Python binding's crates keep their default targets \
                 ({PY_ROOT}, {PY_FFI})"
            )
        })
        .collect()
}

/// [`check_py_manifest`]'s lines: those that start a `[[...]]` target or set a `path` or `build`.
fn py_manifest_lines(rel: &str, text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .map(|(n, line)| (n + 1, line.trim()))
        .filter(|(_, line)| {
            line.starts_with("[[")
                || ["path", "build"].iter().any(|key| {
                    line.strip_prefix(key)
                        .is_some_and(|rest| rest.trim_start().starts_with('='))
                })
        })
        .map(|(n, line)| {
            format!(
                "{rel}:{n}: `{line}`: the Python binding's crates keep their default targets \
                 ({PY_ROOT}, {PY_FFI})"
            )
        })
        .collect()
}

/// The Python binding's only `unsafe` code: the crate `ocio-py-ffi`, which is this one file
/// (owner decisions 2026-10-20 and 2026-10-21; a crate of its own, which `ocio-py` reaches
/// through safe functions only, since p6-core's re-check round 3).
const PY_FFI: &str = "crates/ocio-py/ffi/src/lib.rs";
/// The ffi crate's directory: [`PY_FFI`] is its only Rust source.
const PY_FFI_DIR: &str = "crates/ocio-py/ffi/";
/// SHA-256 of [`PY_FFI`]'s bytes. Changing that file requires updating this pin, so that every
/// change to the Python binding's `unsafe` code shows in review as a change of this line (the
/// coordinator's decision, p6-core re-check round 3).
const PY_FFI_SHA256: &str = "3fb244f63643e64de8a7358f858398c2a2339852af2a5944e8d28048406b6168";
/// The Python binding's crate root, which must forbid `unsafe_code`: no attribute or macro
/// inside the crate can relax a `forbid`, so the compiler holds the binding to safe code.
const PY_ROOT: &str = "crates/ocio-py/src/lib.rs";

/// The owner's calls that return a new reference (or null with an exception set), each the
/// whole of an `unsafe` block bound by `let <name> = unsafe { <call> };`, whose result the next
/// statement wraps ([`PY_FFI_WRAP`]).
const PY_FFI_NEW: [&str; 3] = [
    "ffi::PyType_FromMetaclass",
    "PyInstanceMethod_New",
    "ffi::PyCFunction_NewEx",
];
/// The owner's other calls, each the whole of an `unsafe` block anywhere: `PySequence_Check`,
/// which returns an int; and the views of Python buffers (owner decision P6-10, card
/// p6-buffers): `PyObject_CheckBuffer`, `PyObject_GetBuffer` and `PyBuffer_Release`, the copy of
/// the view the exporter filled and of its format, shape and strides, the slices over a
/// buffer's memory, and the freeing of the view's allocation.
const PY_FFI_CALLS: [&str; 9] = [
    "ffi::PySequence_Check",
    "ffi::PyObject_CheckBuffer",
    "ffi::PyObject_GetBuffer",
    "ffi::PyBuffer_Release",
    "std::ptr::read",
    "CStr::from_ptr",
    "std::slice::from_raw_parts",
    "std::slice::from_raw_parts_mut",
    "Box::from_raw",
];
/// The owner's call that reads a type's slot (owner decision 2026-10-21, p6-core re-check
/// round 4), whose result, a function pointer, is only compared against null: the whole of an
/// `unsafe` block bound by `let <name> = unsafe { <call> };`, followed by `!<name>.is_null()`,
/// the end of the function's block, the pointer's only use.
const PY_FFI_SLOT: &str = "ffi::PyType_GetSlot";
/// The wrapping of a new reference, `Bound::from_owned_ptr_or_err(py, <name>)`: the whole of
/// the `unsafe` block of the statement right after the binding of `<name>`.
const PY_FFI_WRAP: &str = "Bound::from_owned_ptr_or_err";
/// Identifiers [`PY_FFI`] may not hold: other source files (`mod`, `include!`, `#[path]`),
/// macros that could hide code, assembly, and the attributes that name or export a symbol.
const PY_FFI_BANNED: [&str; 15] = [
    "mod",
    "include",
    "include_str",
    "include_bytes",
    "macro_rules",
    "macro",
    "path",
    "asm",
    "global_asm",
    "naked_asm",
    "link_name",
    "link_ordinal",
    "export_name",
    "no_mangle",
    "link_section",
];
/// The one declaration [`PY_FFI`]'s `unsafe extern "C"` block holds (its parameter's name
/// aside, `_` here), token by token.
const PY_FFI_EXTERN: [&str; 18] = [
    "fn",
    "PyInstanceMethod_New",
    "(",
    "_",
    ":",
    "*",
    "mut",
    "ffi",
    "::",
    "PyObject",
    ")",
    "->",
    "*",
    "mut",
    "ffi",
    "::",
    "PyObject",
    ";",
];

/// The guards of the Python binding's `unsafe` code, beyond its pin ([`py_ffi_pin_problem`]),
/// on [`PY_FFI`]'s tokens (comments and literals are never code):
/// - none of [`PY_FFI_BANNED`];
/// - `ffi` is pyo3's: `use pyo3::ffi;` once, and every other `ffi` (but `std::ffi`) starts a path
///   `ffi::...`;
/// - each `unsafe` starts a block or the one `unsafe extern "C"` block ([`PY_FFI_EXTERN`]);
///   nothing else (`unsafe fn`, `unsafe impl`, `unsafe trait`, `#[unsafe(...)]`);
/// - each `unsafe` block is exactly one call whose arguments are plain values
///   ([`plain_argument`]): `let <name> = unsafe { <one of PY_FFI_NEW> };`, followed by the
///   statement `unsafe { Bound::from_owned_ptr_or_err(<py>, <name>) }` (alone or bound by
///   `let`), its only wrapping; a [`PY_FFI_SLOT`] bound by `let <name> = unsafe { .. };` and
///   followed by `!<name>.is_null() }` only; or one of [`PY_FFI_CALLS`] anywhere.
fn check_py_ffi(rel: &str, text: &str) -> Vec<String> {
    let toks = tokenize(text).tokens;
    let mut problems = Vec::new();
    let mut problem = |line: usize, why: String| problems.push(format!("{rel}:{line}: {why}"));
    let text_at = |k: usize| toks.get(k).map_or("", |t| t.text.as_str());
    for t in toks.iter().filter(|t| t.kind == TokenKind::Ident) {
        if PY_FFI_BANNED.contains(&t.text.as_str()) {
            problem(
                t.line,
                format!(
                    "`{}`: the Python binding's unsafe code is this one file, all in sight",
                    t.text
                ),
            );
        }
    }
    let mut uses = 0;
    for (k, t) in toks.iter().enumerate() {
        if t.kind != TokenKind::Ident || t.text != "ffi" {
            continue;
        }
        if k >= 3
            && [
                text_at(k - 3),
                text_at(k - 2),
                text_at(k - 1),
                text_at(k + 1),
            ] == ["use", "pyo3", "::", ";"]
        {
            uses += 1;
        } else if k >= 2 && text_at(k - 1) == "::" && ["std", "core"].contains(&text_at(k - 2)) {
            // The standard library's `std::ffi`.
        } else if text_at(k + 1) != "::" || (k > 0 && text_at(k - 1) == "::") {
            problem(
                t.line,
                "`ffi` must be pyo3's (`use pyo3::ffi;`)".to_string(),
            );
        }
    }
    if uses != 1 {
        problems.push(format!(
            "{rel}: `use pyo3::ffi;` {uses} times (once is required)"
        ));
    }
    // The wrappings due: the index of the `unsafe` of the statement after a binding, the
    // binding's name and line.
    let mut due: Vec<(usize, String, usize)> = Vec::new();
    let mut externs = 0;
    for (k, t) in toks.iter().enumerate() {
        if t.kind != TokenKind::Ident || t.text != "unsafe" {
            continue;
        }
        let mut problem = |why: String| problems.push(format!("{rel}:{}: {why}", t.line));
        if text_at(k + 1) == "extern" {
            externs += 1;
            if let Some(why) = extern_block_problem(&toks[k + 2..]) {
                problem(why);
            }
            continue;
        }
        if text_at(k + 1) != "{" {
            problem(format!("`unsafe {}`", text_at(k + 1)));
            continue;
        }
        let close = matching(&toks, k + 1);
        let Some((name, args)) = single_call(&toks[k + 2..close]) else {
            problem("an `unsafe` block that isn't exactly one call".to_string());
            continue;
        };
        if let Some(bad) = args.iter().find(|a| !plain_argument(a)) {
            let bad: Vec<&str> = bad.iter().map(|t| t.text.as_str()).collect();
            problem(format!("`{name}`'s argument `{}`", bad.join(" ")));
            continue;
        }
        if PY_FFI_NEW.contains(&name.as_str()) {
            let bound = k >= 3
                && text_at(k - 3) == "let"
                && toks[k - 2].kind == TokenKind::Ident
                && text_at(k - 1) == "="
                && text_at(close + 1) == ";";
            if !bound {
                problem(format!(
                    "`{name}` isn't bound alone by `let <name> = unsafe {{ ... }};`"
                ));
                continue;
            }
            let next = close + 2;
            let wrap = if text_at(next) == "unsafe" {
                next
            } else if text_at(next) == "let"
                && toks
                    .get(next + 1)
                    .is_some_and(|t| t.kind == TokenKind::Ident)
                && text_at(next + 2) == "="
            {
                next + 3
            } else {
                next
            };
            due.push((wrap, toks[k - 2].text.clone(), t.line));
        } else if name == PY_FFI_WRAP {
            let Some(at) = due.iter().position(|(w, _, _)| *w == k) else {
                problem(format!(
                    "`{name}` that isn't the statement right after the binding of an owner's \
                     call's result"
                ));
                continue;
            };
            let (_, binding, _) = due.remove(at);
            let wraps = args.len() == 2 && args[1].len() == 1 && args[1][0].text == binding;
            if !wraps {
                problem(format!("`{name}` of something else than `{binding}`"));
            }
        } else if name == PY_FFI_SLOT {
            let binding = (k >= 3
                && text_at(k - 3) == "let"
                && toks[k - 2].kind == TokenKind::Ident
                && text_at(k - 1) == "=")
                .then(|| toks[k - 2].text.as_str());
            let compared = binding.is_some_and(|b| {
                (close + 1..close + 8)
                    .map(text_at)
                    .eq([";", "!", b, ".", "is_null", "(", ")"])
                    && text_at(close + 8) == "}"
            });
            if !compared {
                problem(format!(
                    "`{name}` whose result isn't bound by `let <name> = unsafe {{ ... }};` and \
                     only compared against null, `!<name>.is_null()` ending the function"
                ));
            }
        } else if !PY_FFI_CALLS.contains(&name.as_str()) {
            problem(format!("an `unsafe` call of `{name}`"));
        }
    }
    for (_, binding, line) in due {
        problems.push(format!(
            "{rel}:{line}: `{binding}`, a new reference, isn't wrapped by the next statement"
        ));
    }
    if externs > 1 {
        problems.push(format!(
            "{rel}: {externs} `unsafe extern` blocks (one is allowed)"
        ));
    }
    problems
}

/// Why [`PY_FFI`]'s pin is wrong, if it is: the SHA-256 of `text` isn't [`PY_FFI_SHA256`].
fn py_ffi_pin_problem(text: &str) -> Option<String> {
    let digest = ocio_testkit::fixtures::sha256_hex(text.as_bytes());
    (digest != PY_FFI_SHA256).then(|| {
        format!(
            "{PY_FFI}: its SHA-256 is {digest}, not the pinned {PY_FFI_SHA256} \
             (xtask/src/guards.rs, PY_FFI_SHA256): a change of the Python binding's unsafe code \
             updates the pin, in sight of review"
        )
    })
}

/// Whether the tokens of `text` hold the attribute `#![forbid(unsafe_code)]` outside every group
/// (a top-level inner attribute, the crate's when `text` is its root: not one of a nested
/// module, a `cfg_attr`'s or a macro's).
fn forbids_unsafe_code(text: &str) -> bool {
    let toks = tokenize(text).tokens;
    let want = ["#", "!", "[", "forbid", "(", "unsafe_code", ")", "]"];
    let mut depth = 0usize;
    for (k, t) in toks.iter().enumerate() {
        if depth == 0
            && toks
                .get(k..k + want.len())
                .is_some_and(|w| w.iter().map(|t| t.text.as_str()).eq(want))
        {
            return true;
        }
        match t.text.as_str() {
            "(" | "[" | "{" if t.kind == TokenKind::Punct => depth += 1,
            ")" | "]" | "}" if t.kind == TokenKind::Punct => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    false
}

/// The index of the token that closes the group `toks[open]` opens (`toks.len()` if none).
fn matching(toks: &[Token], open: usize) -> usize {
    let mut depth = 0usize;
    for (k, t) in toks.iter().enumerate().skip(open) {
        match t.text.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                depth -= 1;
                if depth == 0 {
                    return k;
                }
            }
            _ => {}
        }
    }
    toks.len()
}

/// The path and arguments of `expr` when it is exactly one call `a::b(args)` (a trailing comma
/// allowed).
fn single_call(expr: &[Token]) -> Option<(String, Vec<&[Token]>)> {
    let mut path = Vec::new();
    let mut k = 0;
    loop {
        let t = expr.get(k)?;
        if t.kind != TokenKind::Ident {
            return None;
        }
        path.push(t.text.as_str());
        k += 1;
        if expr.get(k)?.text != "::" {
            break;
        }
        k += 1;
    }
    if expr.get(k)?.text != "(" || matching(expr, k) != expr.len() - 1 {
        return None;
    }
    let inner = &expr[k + 1..expr.len() - 1];
    let mut args = Vec::new();
    let (mut depth, mut start) = (0usize, 0);
    for (at, t) in inner.iter().enumerate() {
        match t.text.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth = depth.saturating_sub(1),
            "," if depth == 0 => {
                args.push(&inner[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    if start < inner.len() {
        args.push(&inner[start..]);
    }
    Some((path.join("::"), args))
}

/// An argument the owner's calls may take: a variable, a pointer or the interpreter token
/// of one (`x.as_ptr()`, `x.as_type_ptr()`, `x.py()`), `null_mut()`, or
/// `std::ptr::from_mut(x)`; nothing that dereferences, calls into the C API or runs code.
fn plain_argument(arg: &[Token]) -> bool {
    let variable = |t: &Token| {
        t.kind == TokenKind::Ident
            && t.text
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
            && t.text
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    let texts: Vec<&str> = arg.iter().map(|t| t.text.as_str()).collect();
    match texts[..] {
        [_] => variable(&arg[0]),
        [_, ".", method, "(", ")"] => {
            variable(&arg[0]) && ["as_ptr", "as_type_ptr", "py"].contains(&method)
        }
        ["null_mut", "(", ")"] => true,
        ["std", "::", "ptr", "::", "from_mut", "(", _, ")"] => variable(&arg[6]),
        _ => false,
    }
}

/// Why the `unsafe extern` block whose tokens after `extern` start `toks` isn't allowed, if it
/// isn't: `extern "C" { <PY_FFI_EXTERN> }`, nothing else.
fn extern_block_problem(toks: &[Token]) -> Option<String> {
    if toks.first().map(|t| t.text.as_str()) != Some("\"C\"") {
        return Some("an `unsafe extern` block without the ABI \"C\"".to_string());
    }
    if toks.get(1).map(|t| t.text.as_str()) != Some("{") {
        return Some("an `unsafe extern` without a block".to_string());
    }
    let close = matching(toks, 1);
    let items = &toks[2..close.min(toks.len())];
    let declared = items.len() == PY_FFI_EXTERN.len()
        && items.iter().zip(PY_FFI_EXTERN).all(|(t, want)| {
            if want == "_" {
                t.kind == TokenKind::Ident
            } else {
                t.text == want
            }
        });
    (!declared).then(|| {
        let items: Vec<&str> = items.iter().map(|t| t.text.as_str()).collect();
        format!(
            "an `unsafe extern` block that declares more or less than PyInstanceMethod_New: {}",
            items.join(" ")
        )
    })
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

/// The lines where `text` may relax the `unsafe_code` lint: each identifier `unsafe_code` in
/// code (comments and literals aside) other than a lint of an attribute `#[deny(...)]`,
/// `#![deny(...)]`, `#[forbid(...)]` or `#![forbid(...)]` outside any macro invocation or
/// definition. That refuses an `allow`, `expect` or `warn` of it, alone or among other lints,
/// in an inner or outer attribute, on one line or several, inside a `cfg_attr`, and the name
/// handed to a macro that could make any of them (`allow_it!(unsafe_code)`, even
/// `m!(#[deny(unsafe_code)])`): the identifier is reserved for denying the lint. The line is
/// that of the attribute's `#` when the identifier is inside one, else its own.
fn unsafe_code_relaxed(text: &str) -> Vec<usize> {
    let toks = tokenize(text).tokens;
    let text_at = |k: usize| toks.get(k).map_or("", |t| t.text.as_str());
    let is_ident = |k: usize| toks.get(k).is_some_and(|t| t.kind == TokenKind::Ident);
    // The open groups: (index of the opening token, an attribute's `[`, a macro's group).
    let mut groups: Vec<(usize, bool, bool)> = Vec::new();
    let mut lines = Vec::new();
    for (k, t) in toks.iter().enumerate() {
        match t.text.as_str() {
            "(" | "[" | "{" if t.kind == TokenKind::Punct => {
                let attribute = t.text == "["
                    && (text_at(k.wrapping_sub(1)) == "#"
                        || (text_at(k.wrapping_sub(1)) == "!"
                            && text_at(k.wrapping_sub(2)) == "#"));
                let invocation = k >= 2 && text_at(k - 1) == "!" && is_ident(k - 2);
                let definition = k >= 3
                    && is_ident(k - 1)
                    && text_at(k - 2) == "!"
                    && text_at(k - 3) == "macro_rules";
                groups.push((k, attribute, invocation || definition));
            }
            ")" | "]" | "}" if t.kind == TokenKind::Punct => {
                groups.pop();
            }
            "unsafe_code" if t.kind == TokenKind::Ident => {
                let in_macro = groups.iter().any(|g| g.2);
                let denied = match groups[..] {
                    [.., (attr, true, _), (lints, false, _)] => {
                        lints == attr + 2
                            && ["deny", "forbid"].contains(&text_at(attr + 1))
                            && text_at(lints) == "("
                    }
                    _ => false,
                };
                if in_macro || !denied {
                    let line = groups
                        .iter()
                        .find(|g| g.1)
                        .map_or(t.line, |g| toks[g.0 - 1].line);
                    let hash = groups.iter().find(|g| g.1).map(|g| {
                        if text_at(g.0 - 1) == "!" {
                            toks[g.0 - 2].line
                        } else {
                            toks[g.0 - 1].line
                        }
                    });
                    lines.push(hash.unwrap_or(line));
                }
            }
            _ => {}
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
    let Tokenized { tokens, comments } = tokenize(text);
    let mut code: Vec<char> = text.chars().collect();
    for (from, to) in comments {
        for ch in &mut code[from..to] {
            if *ch != '\n' {
                *ch = ' ';
            }
        }
    }
    Lexed {
        idents: tokens
            .into_iter()
            .filter(|t| t.kind == TokenKind::Ident)
            .map(|t| (t.text, t.line))
            .collect(),
        code: code.into_iter().collect(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    /// An identifier or keyword (a raw identifier `r#x` is `x`).
    Ident,
    /// A string, byte-string, raw-string, C-string, character or byte literal (its text the
    /// whole literal), or a number.
    Literal,
    /// A lifetime or label (`'a`).
    Lifetime,
    /// Punctuation: `::`, `->` and `=>` as one token, any other character alone.
    Punct,
}

#[derive(Debug, Clone)]
struct Token {
    text: String,
    kind: TokenKind,
    /// 1-based.
    line: usize,
}

/// Rust source as tokens, and where its comments are.
struct Tokenized {
    tokens: Vec<Token>,
    /// The character ranges `[from, to)` of its comments (nested block comments as one).
    comments: Vec<(usize, usize)>,
}

/// Rust's lexical structure, enough for the guards: comments (`//` to the end of the line,
/// nested `/* */`) are no tokens, and a literal is one token whatever it holds (a `//` or `/*`
/// in a string is no comment, a `"` in a character literal or a raw string ends nothing).
fn tokenize(text: &str) -> Tokenized {
    let c: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut comments = Vec::new();
    let mut line = 1;
    let mut i = 0;
    let push = |tokens: &mut Vec<Token>, from: usize, to: usize, kind, line| {
        tokens.push(Token {
            text: c[from..to].iter().collect(),
            kind,
            line,
        });
    };
    while i < c.len() {
        let ch = c[i];
        let next = c.get(i + 1).copied();
        let start = i;
        let start_line = line;
        if ch == '\n' {
            line += 1;
            i += 1;
        } else if ch.is_whitespace() {
            i += 1;
        } else if ch == '/' && next == Some('/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            comments.push((start, i));
        } else if ch == '/' && next == Some('*') {
            let mut depth = 0;
            while i < c.len() {
                if c[i] == '/' && c.get(i + 1) == Some(&'*') {
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
            comments.push((start, i.min(c.len())));
        } else if ch == '"' {
            i = skip_string(&c, i + 1, None, &mut line);
            push(
                &mut tokens,
                start,
                i.min(c.len()),
                TokenKind::Literal,
                start_line,
            );
        } else if ch == '\'' {
            if next == Some('\\') {
                // A character literal with an escape ('\n', '\'', '\u{1F600}').
                i += 3;
                while i < c.len() && c[i] != '\'' {
                    i += 1;
                }
                i += 1;
                push(
                    &mut tokens,
                    start,
                    i.min(c.len()),
                    TokenKind::Literal,
                    start_line,
                );
            } else if c.get(i + 2) == Some(&'\'') {
                i += 3;
                push(&mut tokens, start, i, TokenKind::Literal, start_line);
            } else {
                // A lifetime or label.
                i += 1;
                while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                    i += 1;
                }
                push(&mut tokens, start, i, TokenKind::Lifetime, start_line);
            }
        } else if ch.is_ascii_digit() {
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            push(&mut tokens, start, i, TokenKind::Literal, start_line);
        } else if ch.is_alphabetic() || ch == '_' {
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
                    push(
                        &mut tokens,
                        start,
                        i.min(c.len()),
                        TokenKind::Literal,
                        start_line,
                    );
                }
                // b"..", c".."
                "b" | "c" if hashes == 0 && after == Some('"') => {
                    i = skip_string(&c, i + 1, None, &mut line);
                    push(
                        &mut tokens,
                        start,
                        i.min(c.len()),
                        TokenKind::Literal,
                        start_line,
                    );
                }
                // b'x', b'\n'
                "b" if hashes == 0 && after == Some('\'') => {
                    // Past the quote, then the character (escaped after a backslash).
                    i += 1;
                    i += if c.get(i) == Some(&'\\') { 2 } else { 1 };
                    while i < c.len() && c[i] != '\'' {
                        i += 1;
                    }
                    i += 1;
                    push(
                        &mut tokens,
                        start,
                        i.min(c.len()),
                        TokenKind::Literal,
                        start_line,
                    );
                }
                // r#ident: the identifier follows.
                "r" if hashes == 1 && after.is_some_and(|a| a.is_alphabetic() || a == '_') => {
                    let from = i + 1;
                    i = from;
                    while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                        i += 1;
                    }
                    push(&mut tokens, from, i, TokenKind::Ident, start_line);
                }
                _ => push(&mut tokens, start, i, TokenKind::Ident, start_line),
            }
        } else {
            let two: String = c[i..(i + 2).min(c.len())].iter().collect();
            let len = if ["::", "->", "=>"].contains(&two.as_str()) {
                2
            } else {
                1
            };
            i += len;
            push(&mut tokens, start, i, TokenKind::Punct, start_line);
        }
    }
    Tokenized { tokens, comments }
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

/// Third-party versions are exact (`=x.y.z`) in the workspace, and crates inherit them:
/// `manifests` as [`workspace_files`] reads them, the root's first.
fn check_dependency_pins(manifests: &[(String, String)]) -> Vec<String> {
    let mut problems = Vec::new();
    for (rel, text) in manifests {
        let Ok(doc) = text.parse::<toml::Table>() else {
            problems.push(format!("{rel}: does not parse"));
            continue;
        };
        if rel == "Cargo.toml" {
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
            continue;
        }
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

/// The variable that turns the oracle's response cache off (`crates/ocio-testkit/src/oracle.rs`).
const ORACLE_NO_CACHE: &str = "OCIO_RS_ORACLE_NO_CACHE";

/// The directory of GitHub's workflows, and the directory of the scripts they and the gate run.
const WORKFLOWS_DIR: &str = ".github/workflows";
const SCRIPTS_DIR: &str = "scripts";

/// CI's safety net for the oracle's cache keys: every job of every workflow asks the wheel,
/// never the response cache, so that an answer cached under a key that missed a change of the
/// oracle (`crates/ocio-testkit/src/oracle_scope.rs`) can't pass CI. Every workflow (`.yml` or
/// `.yaml` in [`WORKFLOWS_DIR`]: one that runs tests, one that doesn't yet, and a reusable one,
/// whose jobs don't get its caller's `env`) sets [`ORACLE_NO_CACHE`] in its top-level `env`, and
/// every script in [`SCRIPTS_DIR`] that starts a container passes it in (`scripts/rocky9.sh`).
fn check_oracle_cache_off(root: &Path, problems: &mut Vec<String>) {
    let mut workflows = Vec::new();
    match std::fs::read_dir(root.join(WORKFLOWS_DIR)) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        let lower = name.to_ascii_lowercase();
                        if lower.ends_with(".yml") || lower.ends_with(".yaml") {
                            workflows.push(format!("{WORKFLOWS_DIR}/{name}"));
                        }
                    }
                    Err(e) => problems.push(format!("{WORKFLOWS_DIR}: {e}")),
                }
            }
        }
        Err(e) => problems.push(format!("{WORKFLOWS_DIR}: {e}")),
    }
    if workflows.is_empty() {
        problems.push(format!("{WORKFLOWS_DIR}: no workflow found"));
    }
    workflows.sort();
    for rel in &workflows {
        match std::fs::read_to_string(root.join(rel)) {
            Ok(text) => problems.extend(workflow_cache_problems(rel, &text)),
            Err(e) => problems.push(format!("{rel}: {e}")),
        }
    }
    let mut scripts = Vec::new();
    collect_scripts(&root.join(SCRIPTS_DIR), SCRIPTS_DIR, &mut scripts, problems);
    if scripts.is_empty() {
        problems.push(format!("{SCRIPTS_DIR}: no script found"));
    }
    scripts.sort();
    for rel in &scripts {
        match std::fs::read(root.join(rel)) {
            Ok(bytes) => {
                problems.extend(script_cache_problems(rel, &String::from_utf8_lossy(&bytes)))
            }
            Err(e) => problems.push(format!("{rel}: {e}")),
        }
    }
}

/// Every file under `dir` (whose path from the root is `rel`), at any depth.
fn collect_scripts(dir: &Path, rel: &str, out: &mut Vec<String>, problems: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            problems.push(format!("{rel}: {e}"));
            return;
        }
    };
    for entry in entries {
        match entry {
            Ok(entry) => {
                let name = format!("{rel}/{}", entry.file_name().to_string_lossy());
                if entry.path().is_dir() {
                    collect_scripts(&entry.path(), &name, out, problems);
                } else {
                    out.push(name);
                }
            }
            Err(e) => problems.push(format!("{rel}: {e}")),
        }
    }
}

/// The problems of workflow `rel`: its top-level `env:` must set [`ORACLE_NO_CACHE`] to `"1"`,
/// and nothing else in it may name the variable (a job or a step that set it otherwise, or a
/// script that unset it).
fn workflow_cache_problems(rel: &str, text: &str) -> Vec<String> {
    let expected = format!("  {ORACLE_NO_CACHE}: \"1\"");
    let mut in_env = false;
    let mut set = false;
    let mut problems = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let top_level = !line.is_empty() && !line.starts_with([' ', '#']);
        if top_level {
            in_env = line == "env:";
        }
        if !line.contains(ORACLE_NO_CACHE) {
            continue;
        }
        if in_env && line == expected && !set {
            set = true;
        } else {
            problems.push(format!(
                "{rel}:{}: {ORACLE_NO_CACHE} is named other than once, in the top-level env",
                i + 1
            ));
        }
    }
    if !set {
        problems.push(format!(
            "{rel}: the top-level env must set `{ORACLE_NO_CACHE}: \"1\"`, so that every test asks \
             the wheel, never the oracle's response cache"
        ));
    }
    problems
}

/// The subcommands of a container engine that start no container. Any other (`run`, `create`,
/// `exec`, `start`, `compose`, `container ...`, an option, a word the check can't read) may.
const NO_CONTAINER: [&str; 17] = [
    "build", "image", "images", "inspect", "volume", "ps", "pull", "push", "rm", "rmi", "info",
    "version", "system", "login", "logout", "tag", "network",
];

/// The problems of script `rel`: every command that names a container engine (a word whose last
/// path component starts with `docker` or `podman`, in any case, outside a comment line) and may
/// start a container (its subcommand isn't one of [`NO_CONTAINER`]) must pass
/// [`ORACLE_NO_CACHE`] into it: `-e OCIO_RS_ORACLE_NO_CACHE` (or `--env`), on the command's own
/// line or on one of the lines its trailing `\`s continue it onto.
fn script_cache_problems(rel: &str, text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let words_of = |line: &str| -> Vec<String> {
        if line.trim_start().starts_with('#') {
            return Vec::new();
        }
        line.split_whitespace()
            .map(|w| w.trim_matches(|c| c == '"' || c == '\'').to_string())
            .collect()
    };
    let mut problems = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let words = words_of(line);
        for (j, word) in words.iter().enumerate() {
            let program = word
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !(program.starts_with("docker") || program.starts_with("podman")) {
                continue;
            }
            let subcommand = words.get(j + 1).map(String::as_str);
            if program.len() == 6 && subcommand.is_some_and(|s| NO_CONTAINER.contains(&s)) {
                continue;
            }
            // The command: this line and those its trailing `\`s continue it onto.
            let mut end = i;
            while end + 1 < lines.len() && lines[end].trim_end().ends_with('\\') {
                end += 1;
            }
            let forwards = lines[i..=end].iter().any(|l| {
                words_of(l)
                    .windows(2)
                    .any(|w| (w[0] == "-e" || w[0] == "--env") && w[1] == ORACLE_NO_CACHE)
            });
            if !forwards {
                problems.push(format!(
                    "{rel}:{}: `{word}` may start a container without passing {ORACLE_NO_CACHE} \
                     into it (`-e {ORACLE_NO_CACHE} \\`)",
                    i + 1
                ));
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The workflows that run tests turn the oracle's response cache off for every job, and
    /// the Rocky Linux 9 script passes that on: the repository's files, and the cases the check
    /// refuses.
    #[test]
    fn ci_never_reads_the_oracle_cache() {
        let root = paths::workspace_root();
        let mut problems = Vec::new();
        check_oracle_cache_off(root, &mut problems);
        assert_eq!(problems, Vec::<String>::new());

        let good = "name: CI\n\nenv:\n  CARGO_TERM_COLOR: always\n  # The cache.\n  \
                    OCIO_RS_ORACLE_NO_CACHE: \"1\"\n\njobs:\n  a:\n    steps:\n      - run: x\n";
        assert_eq!(workflow_cache_problems("w", good), Vec::<String>::new());
        for bad in [
            // Not set, set to another value, set in a job only, set after the top-level env.
            "env:\n  CARGO_TERM_COLOR: always\njobs:\n  a:\n    steps: []\n",
            "env:\n  OCIO_RS_ORACLE_NO_CACHE: \"0\"\n",
            "env:\n  OCIO_RS_ORACLE_NO_CACHE: 1\n",
            "jobs:\n  a:\n    env:\n      OCIO_RS_ORACLE_NO_CACHE: \"1\"\n",
            "env:\n  A: b\njobs:\n  OCIO_RS_ORACLE_NO_CACHE: \"1\"\n",
            // Set, then named again: a step's env, or a script that unsets it.
            &good.replace(
                "      - run: x\n",
                "      - run: unset OCIO_RS_ORACLE_NO_CACHE\n",
            ),
            &good.replace(
                "      - run: x\n",
                "      - env:\n          OCIO_RS_ORACLE_NO_CACHE: \"\"\n",
            ),
            &format!("{good}env:\n  OCIO_RS_ORACLE_NO_CACHE: \"1\"\n"),
        ] {
            assert!(!workflow_cache_problems("w", bad).is_empty(), "{bad}");
        }

        let rocky = "docker run --rm \\\n    -e OCIO_RS_TIER \\\n    -e OCIO_RS_ORACLE_NO_CACHE \\\n    \
                     \"$image\" \"$@\"\n";
        let quiet = "# Builds the image with docker run's help.\nif ! docker image inspect x; then\n    \
                     docker build -t x .\nfi\ndocker volume create \\\n    v\n";
        for good in [rocky, quiet, "set -e\necho hello\n", ""] {
            assert_eq!(
                script_cache_problems("r", good),
                Vec::<String>::new(),
                "{good}"
            );
        }
        assert_eq!(
            script_cache_problems(
                "r",
                &rocky.replace(" -e OCIO_RS_ORACLE", " --env OCIO_RS_ORACLE")
            ),
            Vec::<String>::new()
        );
        for bad in [
            rocky.replace("    -e OCIO_RS_ORACLE_NO_CACHE \\\n", ""),
            rocky.replace("-e OCIO_RS_ORACLE_NO_CACHE", "-e OCIO_RS_ORACLE_NO_CACHE=0"),
            rocky.replace(
                "    -e OCIO_RS_ORACLE_NO_CACHE \\\n",
                "    # -e OCIO_RS_ORACLE_NO_CACHE\n",
            ),
            // A second container, without the variable.
            format!("{rocky}docker run --rm \"$image\" true\n"),
            // Other engines, spellings and subcommands that start a container.
            "podman run --rm x cargo test\n".to_string(),
            "docker container run --rm x cargo test\n".to_string(),
            "docker exec box cargo test\n".to_string(),
            "docker start -a box\n".to_string(),
            "docker create --name box x\n".to_string(),
            "docker --context remote run x\n".to_string(),
            "docker compose run tests\n".to_string(),
            "docker-compose up\n".to_string(),
            "/usr/bin/docker run x\n".to_string(),
            "\"$root/bin/Docker\" run x\n".to_string(),
            "docker \\\n    run x\n".to_string(),
            "if true; then docker run x; fi\n".to_string(),
            // Only the engines themselves have subcommands known to start nothing.
            "docker-compose build\n".to_string(),
        ] {
            assert!(!script_cache_problems("r", &bad).is_empty(), "{bad}");
        }

        // Every workflow of the directory, a reusable one too, and every script, at any depth.
        let root = paths::target_dir().join(format!("xtask-cache-off-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".github/workflows")).unwrap();
        std::fs::create_dir_all(root.join("scripts/ci")).unwrap();
        std::fs::write(root.join(".github/workflows/a.yml"), good).unwrap();
        std::fs::write(root.join(".github/workflows/README.md"), "docs\n").unwrap();
        std::fs::write(root.join("scripts/rocky9.sh"), rocky).unwrap();
        let mut problems = Vec::new();
        check_oracle_cache_off(&root, &mut problems);
        assert_eq!(problems, Vec::<String>::new());
        let reusable = "on:\n  workflow_call:\njobs:\n  t:\n    steps:\n      - run: cargo test\n";
        std::fs::write(root.join(".github/workflows/reusable.YAML"), reusable).unwrap();
        std::fs::write(
            root.join("scripts/ci/shard.sh"),
            "docker run x cargo test\n",
        )
        .unwrap();
        let mut problems = Vec::new();
        check_oracle_cache_off(&root, &mut problems);
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            problems
                .iter()
                .any(|p| p.starts_with(".github/workflows/reusable.YAML: ")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p.starts_with("scripts/ci/shard.sh:1: ")),
            "{problems:?}"
        );
        assert_eq!(problems.len(), 2, "{problems:?}");

        // No workflow, or no script, fails closed.
        std::fs::create_dir_all(root.join(".github/workflows")).unwrap();
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        let mut problems = Vec::new();
        check_oracle_cache_off(&root, &mut problems);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            problems,
            [
                ".github/workflows: no workflow found",
                "scripts: no script found"
            ],
        );
    }

    #[test]
    fn unsafe_code_relaxed_in_any_lint_attribute() {
        let cases: [(&str, &[usize]); 18] = [
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
            ("#[deny(dead_code, unsafe_code)]\nfn f() {}\n", &[]),
            (
                "// #[allow(unsafe_code)]\n/// #![allow(unsafe_code)]\n",
                &[],
            ),
            (
                "const A: &str = \"#[allow(unsafe_code)]\";\nconst B: &str = r#\"a \"q\" #[allow(unsafe_code)]\"#;\n",
                &[],
            ),
            // The identifier is reserved for denying the lint (it was allowed as a name before
            // p6-core's re-check round 3, which a macro could forward into an `allow`).
            ("#[allow(dead_code)]\nfn unsafe_code() {}\n", &[2]),
            ("fn r#unsafe_code() {}\n", &[1]),
            // Not the lint list of `deny` itself (p6-core re-check round 4, G16c).
            ("#[deny::x(unsafe_code)]\nfn f() {}\n", &[1]),
            // The verifier's bypasses of the line scan (p6-core re-check round 3): a `/*` in a
            // string on the attribute's line, a `//` in a string before it, and a macro that
            // forwards the name into an `allow`.
            (
                "const S: &str = \"/*\"; #[allow(unsafe_code)]\nmod m {}\n",
                &[1],
            ),
            ("const S: &str = \"//\"; #![allow(unsafe_code)]\n", &[1]),
            (
                "macro_rules! allow_it {\n    ($l:ident) => {\n        #[allow($l)]\n        mod m {}\n    };\n}\nallow_it!(unsafe_code);\n",
                &[7],
            ),
            (
                "macro_rules! m {\n    (#[deny($l:ident)]) => { #[allow($l)] mod n {} };\n}\nm!(#[deny(unsafe_code)]);\n",
                &[4],
            ),
            (
                "macro_rules! m {\n    () => { #![deny(unsafe_code)] };\n}\n",
                &[2],
            ),
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

    /// `check_py_ffi`'s problems with `text`, after `use pyo3::ffi;`.
    fn ffi_problems(text: &str) -> Vec<String> {
        check_py_ffi(PY_FFI, &format!("use pyo3::ffi;\n{text}"))
    }

    #[test]
    fn the_python_binding_s_ffi_crate_makes_only_the_owners_calls() {
        let good = "#![allow(unsafe_code)]\n\
                    #[cfg_attr(windows, link(name = \"python313\", kind = \"raw-dylib\"))]\n\
                    unsafe extern \"C\" {\n    /// A doc comment: unsafe { x }.\n    \
                    fn PyInstanceMethod_New(func: *mut ffi::PyObject) -> *mut ffi::PyObject;\n}\n\
                    fn a() {\n    // SAFETY: unsafe { anything } in a comment.\n    \
                    let s = \"unsafe { *p } // /* mod m; include!\";\n    \
                    let p = unsafe {\n        ffi::PyType_FromMetaclass(\n            \
                    m.as_type_ptr(),\n            null_mut(),\n            \
                    std::ptr::from_mut(s),\n            b.as_ptr(),\n        )\n    };\n    \
                    let cls = unsafe { Bound::from_owned_ptr_or_err(py, p) }?;\n    \
                    let q = unsafe { PyInstanceMethod_New(f.as_ptr()) };\n    \
                    unsafe { Bound::from_owned_ptr_or_err(f.py(), q) };\n    \
                    let c = unsafe { ffi::PySequence_Check(o.as_ptr()) };\n    \
                    let r = unsafe { ffi::PyCFunction_NewEx(d, s, m) };\n    \
                    unsafe { Bound::from_owned_ptr_or_err(record.py(), r) }\n}\n\
                    fn b(ty: &Bound<PyType>) -> bool {\n    \
                    let pointer = unsafe { ffi::PyType_GetSlot(ty.as_type_ptr(), id) };\n    \
                    !pointer.is_null()\n}\n\
                    fn c(o: &Bound<PyAny>) {\n    \
                    let k = unsafe { ffi::PyObject_CheckBuffer(o.as_ptr()) };\n    \
                    let s = unsafe { ffi::PyObject_GetBuffer(o.as_ptr(), view, flags) };\n    \
                    let raw = unsafe { std::ptr::read(view) };\n    \
                    let f = unsafe { CStr::from_ptr(format_ptr) }.to_bytes();\n    \
                    let a = unsafe { std::slice::from_raw_parts(shape_ptr, count) };\n    \
                    let b = unsafe { std::slice::from_raw_parts_mut(start, len) };\n    \
                    unsafe { ffi::PyBuffer_Release(view) };\n    \
                    let boxed = unsafe { Box::from_raw(view) };\n}\n";
        assert_eq!(ffi_problems(good), Vec::<String>::new());
        for bad in [
            "fn a() { let x = unsafe { *p }; }",
            "fn a() { unsafe { ffi::PyType_GetSlot(t, 1) } }",
            "unsafe extern \"C\" {\n    fn PyInstanceMethod_New(f: *mut ffi::PyObject) -> *mut ffi::PyObject;\n    fn Py_Other() -> i32;\n}",
            "unsafe extern \"C\" {\n    fn Py_Other() -> i32;\n}",
            "unsafe fn f() {}",
            "unsafe impl Send for X {}",
            "unsafe trait T {}",
            // Only the C ABI, only the one declaration, without attributes or qualifiers.
            "unsafe extern \"C-unwind\" {\n    fn PyInstanceMethod_New(f: *mut ffi::PyObject) -> *mut ffi::PyObject;\n}",
            "unsafe extern \"system\" {\n    fn PyInstanceMethod_New(f: *mut ffi::PyObject) -> *mut ffi::PyObject;\n}",
            "unsafe extern {\n    fn PyInstanceMethod_New(f: *mut ffi::PyObject) -> *mut ffi::PyObject;\n}",
            "unsafe extern \"C\" {\n    fn PyInstanceMethod_New(f: *mut ffi::PyObject) -> *mut ffi::PyObject;\n    static X: i32;\n}",
            "unsafe extern \"C\" {\n    safe fn PyInstanceMethod_New(f: *mut ffi::PyObject) -> *mut ffi::PyObject;\n}",
            "unsafe extern \"C\" {\n    pub fn PyInstanceMethod_New(f: *mut ffi::PyObject) -> *mut ffi::PyObject;\n}",
            "unsafe extern \"C\" {\n    fn PyInstanceMethod_New(f: *mut ffi::PyObject, g: i32) -> *mut ffi::PyObject;\n}",
            // One call per block, nothing before or after it.
            "fn a() { let p = unsafe { ffi::PyType_FromMetaclass(m, n, s, b); *q }; }",
            "fn a() { let p = unsafe { other(); ffi::PyType_FromMetaclass(m, n, s, b) }; }",
            "fn a() { let p = unsafe { ffi::PyType_FromMetaclass(m, n, s, b).add(1) }; }",
            "fn a() { let p = unsafe { ffi::PyType_FromMetaclass(m, n, s, b)(1) }; }",
            // Plain arguments only.
            "fn a() { let c = unsafe { ffi::PySequence_Check(*f) }; }",
            "fn a() { let c = unsafe { ffi::PySequence_Check(ffi::Py_None()) }; }",
            "fn a() { let c = unsafe { ffi::PySequence_Check({ x() }) }; }",
            "fn a() { let c = unsafe { ffi::PySequence_Check(f.as_ptr().add(1)) }; }",
            "fn a() { let c = unsafe { ffi::PySequence_Check(F) }; }",
            "fn a() { let c = unsafe { ffi::PySequence_Check(f.read()) }; }",
            "fn a() { let c = unsafe { ffi::PySequence_Check(std::ptr::read(f)) }; }",
            // A name that merely contains an owner's call (N17b), or another path to it.
            "fn a() { let c = unsafe { ffi::PySequence_CheckX(o) }; }",
            "fn a() { let c = unsafe { PySequence_Check(o) }; }",
            "fn a() { let c = unsafe { xffi::PySequence_Check(o) }; }",
            // `ffi` is pyo3's.
            "use other as ffi;",
            "struct ffi;",
            // The verifier's bypasses of the line scan (p6-core re-check round 3): a `//` in a
            // string hid the rest of the line; a child module, an include or a path inherited
            // the file's `allow(unsafe_code)`.
            "fn a() { let s = \"//\"; let x = unsafe { *p }; }",
            "fn a() { let s = '\"'; let x = unsafe { *p }; }",
            "mod m;",
            "#[path = \"x.rs\"]\nfn f() {}",
            "include!(\"x.rs\");",
            "const S: &str = include_str!(\"x.rs\");",
            "macro_rules! m { () => {} }",
            "core::arch::global_asm!(\"nop\");",
            "#[link_name = \"Py_DecRef\"]\nfn f() {}",
            // Every entry of the ban list (p6-core re-check round 4: the ones no case refused).
            "fn f() { core::arch::asm!(\"nop\"); }",
            "fn f() { core::arch::naked_asm!(\"ret\"); }",
            "const B: &[u8] = include_bytes!(\"x.bin\");",
            "#[link_ordinal(1)]\nfn f() {}",
            "#[export_name = \"f\"]\nfn f() {}",
            "#[link_section = \".x\"]\nfn f() {}",
            "macro m() {}",
            // The buffer calls (P6-10): plain arguments only, no other path to them, and no
            // other call of the standard library's.
            "fn a() { let b = unsafe { std::slice::from_raw_parts_mut(p.add(1), n) }; }",
            "fn a() { let b = unsafe { std::slice::from_raw_parts(p, n + 1) }; }",
            "fn a() { let b = unsafe { core::slice::from_raw_parts(p, n) }; }",
            "fn a() { let b = unsafe { slice::from_raw_parts(p, n) }; }",
            "fn a() { let b = unsafe { std::mem::transmute(p) }; }",
            "fn a() { let b = unsafe { std::ptr::write(p, v) }; }",
            "fn a() { let b = unsafe { Box::from_raw(p.cast()) }; }",
            "fn a() { let b = unsafe { ffi::PyObject_GetBuffer(o.as_ptr(), &mut v, f) }; }",
            "fn a() { unsafe { ffi::PyBuffer_Release(std::ptr::from_mut(v).add(1)) }; }",
            // A `from_mut` of something else than a variable.
            "fn a() { let c = unsafe { ffi::PySequence_Check(std::ptr::from_mut(F)) }; }",
            // The wrapping takes the previous statement's owner's call result, once.
            "fn a(raw: *mut O) { unsafe { Bound::from_owned_ptr_or_err(py, raw) } }",
            "fn a() { let p = g(); unsafe { Bound::from_owned_ptr_or_err(py, p) } }",
            "fn a() { let p = unsafe { ffi::PySequence_Check(o) }; unsafe { Bound::from_owned_ptr_or_err(py, p) } }",
            "fn a(ptr: *mut O) { let q = unsafe { PyInstanceMethod_New(f) }; unsafe { Bound::from_owned_ptr_or_err(py, ptr) } }",
            "fn a() { let p = unsafe { PyInstanceMethod_New(f) }; unsafe { Bound::from_owned_ptr_or_err(py, p, q) } }",
            "fn a() { let p = unsafe { PyInstanceMethod_New(f) }; unsafe { Bound::from_owned_ptr_or_err(p) } }",
            // A slot's pointer used otherwise than compared against null (p6-core re-check round
            // 4), or not bound.
            "fn b() -> bool { let p = unsafe { ffi::PyType_GetSlot(t, id) }; p.is_null() }",
            "fn b() -> bool { let p = unsafe { ffi::PyType_GetSlot(t, id) }; !p.is_null() && x }",
            "fn b() -> bool { let p = unsafe { ffi::PyType_GetSlot(t, id) }; !q.is_null() }",
            "fn b() -> bool { let p = unsafe { ffi::PyType_GetSlot(t, id) }; let q = p; !q.is_null() }",
            "fn b() -> bool { let p = unsafe { ffi::PyType_GetSlot(t, id) }; !p.is_null(); call(p) }",
            "fn b() -> bool { !unsafe { ffi::PyType_GetSlot(t, id) }.is_null() }",
            "fn b() { let p = unsafe { ffi::PyType_GetSlot(t, id) }; }",
            "fn b() { let mut p = unsafe { ffi::PyType_GetSlot(t, id) }; !p.is_null() }",
            "fn b() { p = unsafe { ffi::PyType_GetSlot(t, id) }; !p.is_null() }",
            "fn b() { let p; unsafe { ffi::PyType_GetSlot(t, id) }; !p.is_null() }",
            // Not the next statement: a closure (callable twice), a use in between.
            "fn a() { let (p, _) = (unsafe { PyInstanceMethod_New(f) }, 1); }",
            // An earlier binding of the name elsewhere in the file, or a parameter of its name.
            "fn x() { let ptr = unsafe { PyInstanceMethod_New(f) }; unsafe { Bound::from_owned_ptr_or_err(py, ptr) } }\n\
             fn y(ptr: *mut O) { unsafe { Bound::from_owned_ptr_or_err(py, ptr) } }",
            // Unwrapped.
            "fn a() { let p = unsafe { PyInstanceMethod_New(f) }; drop(p); }",
        ] {
            assert_eq!(ffi_problems(bad).len(), 1, "{bad}: {:?}", ffi_problems(bad));
        }
        // Two problems each (the first one's text has the fragment): the attribute
        // `no_mangle` and `unsafe(`; a path to another `ffi` and the call it makes; a call
        // that isn't the wrapping (G3: a two-argument call of something else on an owner's
        // call's result; a name that merely contains an owner's call, N17b) or a wrapping that
        // isn't the next statement (a closure, callable twice; a copy in between; `let mut`;
        // the result used before the statement ends), and the new reference left unwrapped.
        for (bad, first) in [
            (
                "#[unsafe(no_mangle)]\nextern \"C\" fn f() {}",
                "`no_mangle`",
            ),
            (
                "fn a() { let c = unsafe { my::ffi::PySequence_Check(o) }; }",
                "`ffi` must be pyo3's",
            ),
            (
                "fn a() { let c = unsafe { self::ffi::PySequence_Check(o) }; }",
                "`ffi` must be pyo3's",
            ),
            (
                "fn a() { let p = unsafe { PyInstanceMethod_New(f) }; unsafe { Bound::from_owned_ptr(py, p) } }",
                "call of `Bound::from_owned_ptr`",
            ),
            (
                "fn a() { let p = unsafe { ffi::PyCFunction_NewExt(d, s, m) }; unsafe { Bound::from_owned_ptr_or_err(py, p) } }",
                "call of `ffi::PyCFunction_NewExt`",
            ),
            (
                "fn a() { let p = unsafe { PyInstanceMethod_New(f) }; let g = || unsafe { Bound::from_owned_ptr_or_err(py, p) }; }",
                "isn't the statement right after",
            ),
            (
                "fn a() { let p = unsafe { PyInstanceMethod_New(f) }; let ptr = p; unsafe { Bound::from_owned_ptr_or_err(py, ptr) } }",
                "isn't the statement right after",
            ),
            (
                "fn a() { let mut p = unsafe { PyInstanceMethod_New(f) }; unsafe { Bound::from_owned_ptr_or_err(py, p) } }",
                "isn't bound alone",
            ),
            (
                "fn a() { let p = unsafe { PyInstanceMethod_New(f) }.wrapping_add(1); unsafe { Bound::from_owned_ptr_or_err(py, p) } }",
                "isn't bound alone",
            ),
        ] {
            let problems = ffi_problems(bad);
            assert_eq!(problems.len(), 2, "{bad}: {problems:?}");
            assert!(problems[0].contains(first), "{bad}: {problems:?}");
        }
        // Which problem each is (p6-core re-check round 4: G11, G23, G29): a block with a call
        // and more after it (whose tokens a call's argument scan would take for arguments), an
        // `unsafe` item, an `unsafe extern` without a block.
        for (bad, why) in [
            (
                "fn a() { let c = unsafe { ffi::PySequence_Check(x); ffi::PySequence_Check(y) }; }",
                "isn't exactly one call",
            ),
            (
                "fn a() { unsafe { ffi::PySequence_Check(x); g(y) } }",
                "isn't exactly one call",
            ),
            ("unsafe fn f() {}", "`unsafe fn`"),
            (
                "unsafe extern \"C\" fn PyInstanceMethod_New() {}",
                "without a block",
            ),
        ] {
            let problems = ffi_problems(bad);
            assert_eq!(problems.len(), 1, "{bad}: {problems:?}");
            assert!(problems[0].contains(why), "{bad}: {problems:?}");
        }
        // Wrapped twice: the second wrapping isn't the statement after the binding.
        let twice = "fn a() { let p = unsafe { PyInstanceMethod_New(f) }; \
                     let x = unsafe { Bound::from_owned_ptr_or_err(py, p) }; \
                     let y = unsafe { Bound::from_owned_ptr_or_err(py, p) }; }";
        assert_eq!(ffi_problems(twice).len(), 1, "{:?}", ffi_problems(twice));
        // Two extern blocks, each fine on its own.
        let two = "unsafe extern \"C\" {\n    fn PyInstanceMethod_New(f: *mut ffi::PyObject) -> *mut ffi::PyObject;\n}\n\
                   unsafe extern \"C\" {\n    fn PyInstanceMethod_New(f: *mut ffi::PyObject) -> *mut ffi::PyObject;\n}\n";
        assert_eq!(ffi_problems(two).len(), 1);
        // `use pyo3::ffi;` once.
        assert_eq!(check_py_ffi(PY_FFI, "fn a() {}").len(), 1);
        assert_eq!(
            check_py_ffi(PY_FFI, "use pyo3::ffi;\nuse pyo3::ffi;\n").len(),
            1
        );
        // The file itself.
        let text = std::fs::read_to_string(paths::workspace_root().join(PY_FFI)).unwrap();
        assert_eq!(check_py_ffi(PY_FFI, &text), Vec::<String>::new());
    }

    #[test]
    fn the_python_binding_s_ffi_file_is_pinned() {
        let text = std::fs::read_to_string(paths::workspace_root().join(PY_FFI)).unwrap();
        assert_eq!(py_ffi_pin_problem(&text), None);
        let changed = text.replacen("check == 1", "check != 0", 1);
        assert_ne!(changed, text);
        assert!(py_ffi_pin_problem(&changed).is_some());
        assert!(py_ffi_pin_problem(&format!("{text}\n")).is_some());
    }

    #[test]
    fn the_python_binding_s_unsafe_code_is_one_file_and_its_root_forbids_unsafe_code() {
        let file = |rel: &str, text: &str| (rel.to_string(), text.to_string());
        let header = "// SPDX-License-Identifier: BSD-3-Clause\n\
                      // Copyright Contributors to the OpenColorIO Project.\n";
        let ffi = std::fs::read_to_string(paths::workspace_root().join(PY_FFI)).unwrap();
        let root = format!("{header}#![forbid(unsafe_code)]\n");
        let ok = [file(PY_FFI, &ffi), file(PY_ROOT, &root)];
        assert_eq!(check_files(&ok, &[], &|_, _| false), Vec::<String>::new());
        for (rel, text) in [
            (
                "crates/ocio-py/ffi/src/other.rs",
                format!("{header}fn f() {{}}\n"),
            ),
            (
                "crates/ocio-py/ffi/build.rs",
                format!("{header}fn main() {{}}\n"),
            ),
            (
                "crates/ocio-py/ffi/tests/t.rs",
                format!("{header}fn f() {{}}\n"),
            ),
            (PY_ROOT, format!("{header}#![deny(unsafe_code)]\n")),
            (PY_ROOT, format!("{header}// #![forbid(unsafe_code)]\n")),
        ] {
            let mut files = ok.to_vec();
            files.retain(|(r, _)| r != rel);
            files.push(file(rel, &text));
            assert_eq!(
                check_files(&files, &[], &|_, _| false).len(),
                1,
                "{rel}: {text}"
            );
        }
        // A change of the pinned file.
        let files = [file(PY_FFI, &format!("{ffi}\n")), file(PY_ROOT, &root)];
        assert_eq!(check_files(&files, &[], &|_, _| false).len(), 1);
        // One that check_py_ffi refuses, which check_files runs (p6-core re-check round 4, G33).
        let files = [
            file(PY_FFI, &format!("{ffi}fn z() {{ unsafe {{ other() }} }}\n")),
            file(PY_ROOT, &root),
        ];
        let problems = check_files(&files, &[], &|_, _| false);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(
            problems[0].contains("an `unsafe` call of `other`"),
            "{problems:?}"
        );
        assert!(problems[1].contains("not the pinned"), "{problems:?}");
        // The root's `forbid` is a top-level inner attribute (p6-core re-check round 4).
        for text in [
            "mod m {\n    #![forbid(unsafe_code)]\n}\n",
            "#[cfg(any())]\nmod m {\n    #![forbid(unsafe_code)]\n}\n",
            "#![cfg_attr(feature = \"x\", forbid(unsafe_code))]\n",
            "m! { #![forbid(unsafe_code)] }\n",
            "m!(#![forbid(unsafe_code)]);\n",
        ] {
            assert!(!forbids_unsafe_code(text), "{text}");
            let files = [
                file(PY_FFI, &ffi),
                file(PY_ROOT, &format!("{header}{text}")),
            ];
            let problems = check_files(&files, &[], &|_, _| false);
            // (A `forbid` in a `cfg_attr` or a macro is also a relaxing use of the name.)
            assert!(
                problems
                    .iter()
                    .any(|p| p.contains("top-level inner attribute")),
                "{text}: {problems:?}"
            );
        }
        assert!(forbids_unsafe_code(
            "//! Docs.\n#![deny(missing_docs)]\n#![forbid(unsafe_code)]\nmod m {}\n"
        ));
        // A build script, which Cargo runs without any key in the manifest.
        let files = [
            file(PY_FFI, &ffi),
            file(PY_ROOT, &root),
            file(
                "crates/ocio-py/build.rs",
                &format!("{header}fn main() {{}}\n"),
            ),
        ];
        let problems = check_files(&files, &[], &|_, _| false);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("no build script"), "{problems:?}");
        // Targets other than the default ones.
        for manifest in [
            "[package]
name = \"ocio-py\"
build = \"build.rs\"
",
            "[lib]
name = \"ocio_py\"
path = \"src/other.rs\"
",
            "[package]
name = \"ocio-py\"

[[bin]]
name = \"x\"
",
        ] {
            for rel in ["crates/ocio-py/Cargo.toml", "crates/ocio-py/ffi/Cargo.toml"] {
                let manifests = [file(rel, manifest)];
                let problems = check_files(&ok, &manifests, &|_, _| false);
                // Its lints aren't the workspace's either.
                assert_eq!(problems.len(), 2, "{rel}: {manifest}: {problems:?}");
                assert!(problems[1].contains("default targets"), "{problems:?}");
            }
        }
        // The same keys, dotted or inline (p6-core re-check round 4).
        for (manifest, key) in [
            ("package.build = \"x.rs\"\n", "`package.build`"),
            ("lib = { path = \"src/x.rs\" }\n", "`lib.path`"),
            ("bin = [{ name = \"x\" }]\n", "`bin`"),
            ("test = [{ name = \"x\" }]\n", "`test`"),
            ("example = [{ name = \"x\" }]\n", "`example`"),
            ("bench = [{ name = \"x\" }]\n", "`bench`"),
        ] {
            let manifest = format!("{manifest}\n[lints]\nworkspace = true\n");
            for rel in ["crates/ocio-py/Cargo.toml", "crates/ocio-py/ffi/Cargo.toml"] {
                let problems = check_py_manifest(rel, &manifest);
                assert_eq!(problems.len(), 1, "{rel}: {manifest}: {problems:?}");
                assert!(problems[0].contains(key), "{problems:?}");
            }
            assert!(check_py_manifest("crates/ocio/Cargo.toml", &manifest).is_empty());
        }
        let fine = "[package]
name = \"ocio-py\"

[dependencies]
ocio-py-ffi.workspace = true

[lints]
workspace = true
";
        let manifests = [file("crates/ocio-py/Cargo.toml", fine)];
        assert_eq!(
            check_files(&ok, &manifests, &|_, _| false),
            Vec::<String>::new()
        );
        // An `allow(unsafe_code)` in the binding's other files.
        let files = [
            file(PY_FFI, &ffi),
            file(PY_ROOT, &root),
            file(
                "crates/ocio-py/src/x.rs",
                &format!("{header}#![allow(unsafe_code)]\n"),
            ),
        ];
        assert_eq!(check_files(&files, &[], &|_, _| false).len(), 1);
    }

    #[test]
    fn unsafe_allowed_only_in_the_allowlisted_files() {
        // The Python binding: its ffi crate's one file.
        assert!(unsafe_allowed(PY_FFI));
        for rel in [
            "crates/ocio-py/src/lib.rs",
            "crates/ocio-py/src/pybind11.rs",
            "crates/ocio-py/src/type_object.rs",
            "crates/ocio-py/ffi/src/x.rs",
            "crates/ocio-py/ffi/src/lib/x.rs",
            "crates/ocio-py/ffi/build.rs",
            "crates/ocio-py/tests/t.rs",
        ] {
            assert!(!unsafe_allowed(rel), "{rel}");
        }
        // As before p6-core's re-check round 3: the SIMD modules, the test kit's C runtime and
        // the Windows file of minizip-ng's port.
        for rel in [
            "crates/ocio-testkit/src/crt.rs",
            "crates/ocio/src/minizip/os_windows_ffi.rs",
            "crates/ocio-ops/src/sse.rs",
            "crates/ocio-ops/src/sse2.rs",
            "crates/ocio-ops/src/avx.rs",
            "crates/ocio-ops/src/avx2.rs",
            "crates/ocio-ops/src/avx512.rs",
            "crates/ocio-ops/src/cpu_info.rs",
            "crates/ocio-ops/src/ops/simd/x.rs",
            "crates/ocio-ops/src/ops/lut1d/lut1d_op_cpu_sse2.rs",
            "crates/ocio-ops/src/ops/lut3d/lut3d_op_cpu_avx2.rs",
            "crates/ocio-ops/src/half_f16c.rs",
            // The SIMD module's own test file (coordinator's decision 2026-10-21).
            "crates/ocio-ops/src/sse_tests.rs",
        ] {
            assert!(unsafe_allowed(rel), "{rel}");
        }
        for rel in [
            "crates/ocio-ops/src/lib.rs",
            "crates/ocio-gpu/src/sse2.rs",
            "crates/ocio/src/minizip/os_windows.rs",
            "crates/ocio-testkit/src/lib.rs",
            "xtask/src/guards.rs",
            "crates/ocio-ops/src/sse2_tests.rs",
            "crates/ocio-ops/src/avx_tests.rs",
            "crates/ocio-testkit/src/crt_tests.rs",
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

    const HEADER: &str = "// SPDX-License-Identifier: BSD-3-Clause\n\
                          // Copyright Contributors to the OpenColorIO Project.\n";

    /// `check_files`' problems with these sources (each text after the header) and no manifest.
    fn source_file_problems(files: &[(&str, &str)]) -> Vec<String> {
        let files: Vec<(String, String)> = files
            .iter()
            .map(|(rel, text)| (rel.to_string(), format!("{HEADER}{text}")))
            .collect();
        check_files(&files, &[], &|_, _| false)
    }

    #[test]
    fn the_keyword_unsafe_only_in_the_allowlisted_files() {
        // M1 (p6-core re-check round 4): a macro of another crate expands `unsafe` code into
        // ocio-py, which rustc and the lint-based guards accepted.
        let macro_m1 = "#[macro_export]\nmacro_rules! raw {\n    ($e:expr) => {\n        unsafe { $e }\n    };\n}\n";
        let problems = source_file_problems(&[("crates/ocio-gpu/src/lib.rs", macro_m1)]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].starts_with("crates/ocio-gpu/src/lib.rs:6: the keyword `unsafe`"),
            "{problems:?}"
        );
        // Any use, in any file but the allowlisted ones: a block, an item, a raw identifier.
        for (text, lines) in [
            ("fn f() { unsafe { g() } }\n", vec![3]),
            ("unsafe fn f() {}\nunsafe impl Send for X {}\n", vec![3, 4]),
            ("fn r#unsafe() {}\n", vec![3]),
            (
                "// unsafe { g() }\n/// unsafe\nconst S: &str = \"unsafe { g() }\";\nconst R: &str = r#\"unsafe\"#;\nfn unsafe_x() {}\n",
                vec![],
            ),
        ] {
            for rel in [
                "crates/ocio-py/src/lib.rs",
                "crates/ocio-py/tests/t.rs",
                "crates/ocio-ops/src/sse2_tests.rs",
                "xtask/src/main.rs",
                "tools/x/src/lib.rs",
            ] {
                let problems: Vec<usize> = source_file_problems(&[(rel, text)])
                    .iter()
                    .filter(|p| p.contains("the keyword `unsafe`"))
                    .map(|p| {
                        p[rel.len() + 1..]
                            .split(':')
                            .next()
                            .unwrap()
                            .parse()
                            .unwrap()
                    })
                    .collect();
                assert_eq!(problems, lines, "{rel}: {text}");
            }
            for rel in [
                "crates/ocio-testkit/src/crt.rs",
                "crates/ocio-ops/src/sse_tests.rs",
                "crates/ocio-ops/src/ops/lut1d/lut1d_op_cpu_avx2.rs",
            ] {
                assert_eq!(source_file_problems(&[(rel, text)]), Vec::<String>::new());
            }
        }
    }

    #[test]
    fn the_guards_scan_every_member_s_files() {
        // N1 (p6-core re-check round 4): a member under tools/ wasn't scanned.
        let root_manifest = "[workspace]\nmembers = [\"crates/a\", \"tools/x\", \"tools/x/y\"]\n";
        assert_eq!(
            workspace_members(root_manifest).unwrap(),
            ["crates/a", "tools/x", "tools/x/y"]
        );
        for members in [
            "\"crates/*\"",
            "\"../x\"",
            "\"a/./b\"",
            "\"\"",
            "\"a\\\\b\"",
            "1",
        ] {
            let text = format!("[workspace]\nmembers = [{members}]\n");
            assert!(workspace_members(&text).is_err(), "{text}");
        }
        assert!(workspace_members("[package]\nname = \"a\"\n").is_err());
        // A scratch tree: only a member's own `target` directory is left out, each file once.
        let dir = std::env::temp_dir().join(format!("ocio-guards-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for rel in [
            "crates/a/src/lib.rs",
            "crates/a/target/debug/build.rs",
            "crates/a/src/target/kept.rs",
            "tools/x/src/lib.rs",
            "tools/x/Cargo.toml",
            "tools/x/y/src/lib.rs",
            "tools/x/y/target/z.rs",
            "tools/other/src/lib.rs",
        ] {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "").unwrap();
        }
        let members = workspace_members(root_manifest).unwrap();
        let sources = member_files(&dir, &members, |rel| rel.ends_with(".rs"));
        let manifests = member_files(&dir, &members, |rel| rel.ends_with("Cargo.toml"));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            sources,
            [
                "crates/a/src/lib.rs",
                "crates/a/src/target/kept.rs",
                "tools/x/src/lib.rs",
                "tools/x/y/src/lib.rs",
            ]
        );
        assert_eq!(manifests, ["tools/x/Cargo.toml"]);
    }

    #[test]
    fn allowlisted_files_pull_in_no_other_file_but_their_tests() {
        let tests_sse = ("crates/ocio-ops/src/sse_tests.rs", "fn t() {}\n");
        let tests_sse2 = ("crates/ocio-ops/src/sse2_tests.rs", "fn t() {}\n");
        // The form the coordinator approved (2026-10-21), with `pub(crate)` or not; the `allow`
        // only before a test file that is itself allowlisted.
        for (rel, text, sibling) in [
            (
                "crates/ocio-ops/src/sse2.rs",
                "#[cfg(test)]\n#[path = \"sse2_tests.rs\"]\npub(crate) mod tests;\n",
                tests_sse2,
            ),
            (
                "crates/ocio-ops/src/sse2.rs",
                "#[cfg(test)]\n#[path = \"sse2_tests.rs\"]\nmod simd_tests;\n",
                tests_sse2,
            ),
            (
                "crates/ocio-ops/src/sse.rs",
                "#[cfg(test)]\n#[allow(unsafe_code)] // A comment.\n#[path = \"sse_tests.rs\"]\nmod tests;\n",
                tests_sse,
            ),
        ] {
            assert_eq!(
                source_file_problems(&[(rel, text), sibling]),
                Vec::<String>::new(),
                "{rel}: {text}"
            );
        }
        // The repro (p6-core re-check round 4): a child module of crt.rs with bare `unsafe`.
        let problems = source_file_problems(&[
            ("crates/ocio-testkit/src/crt.rs", "mod child;\n"),
            (
                "crates/ocio-testkit/src/child.rs",
                "pub fn f() { unsafe { g() } }\n",
            ),
        ]);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].contains("`mod child;`"), "{problems:?}");
        assert!(problems[1].contains("the keyword `unsafe`"), "{problems:?}");
        // Anything else, each a problem per declaration and per file named.
        let child = ("crates/ocio-testkit/src/x.rs", "fn f() {}\n");
        for (text, count) in [
            ("pub mod child;\n", 1),
            ("#[path = \"x.rs\"]\nmod m;\n", 2),
            ("#[cfg(test)]\n#[path = \"x.rs\"]\nmod tests;\n", 2),
            ("#[cfg(test)]\n#[path = \"crt_tests.rs\"]\nmod tests;\n", 3),
            ("#[path = \"x.rs\"]\nfn f() {}\n", 1),
            ("include!(\"x.rs\");\n", 1),
            ("const S: &str = include_str!(\"x.rs\");\n", 1),
            ("const B: &[u8] = include_bytes!(\"x.rs\");\n", 1),
            ("#[cfg_attr(test, path = \"x.rs\")]\nmod m;\n", 2),
            ("mod a {\n    mod b;\n}\n", 1),
        ] {
            let problems = source_file_problems(&[("crates/ocio-testkit/src/crt.rs", text), child]);
            assert_eq!(problems.len(), count, "{text}: {problems:?}");
            assert!(
                problems
                    .iter()
                    .all(|p| p.starts_with("crates/ocio-testkit/src/crt.rs:")),
                "{problems:?}"
            );
        }
        // The approved form, but: another stem, no sibling, `cfg` other than `test`, an
        // `allow` before a test file that isn't allowlisted, inside an inline module, other
        // visibility.
        for (rel, text, count) in [
            (
                "crates/ocio-ops/src/sse.rs",
                "#[cfg(test)]\n#[path = \"sse2_tests.rs\"]\nmod tests;\n",
                2,
            ),
            (
                "crates/ocio-ops/src/avx.rs",
                "#[cfg(test)]\n#[path = \"avx_tests.rs\"]\nmod tests;\n",
                3,
            ),
            (
                "crates/ocio-ops/src/sse.rs",
                "#[cfg(not(test))]\n#[path = \"sse_tests.rs\"]\nmod tests;\n",
                2,
            ),
            (
                "crates/ocio-ops/src/sse2.rs",
                "#[cfg(test)]\n#[allow(unsafe_code)]\n#[path = \"sse2_tests.rs\"]\nmod tests;\n",
                2,
            ),
            (
                "crates/ocio-ops/src/sse.rs",
                "mod a {\n    #[cfg(test)]\n    #[path = \"sse_tests.rs\"]\n    mod tests;\n}\n",
                3,
            ),
            (
                "crates/ocio-ops/src/sse.rs",
                "#[cfg(test)]\n#[path = \"sse_tests.rs\"]\npub mod tests;\n",
                2,
            ),
        ] {
            let problems = source_file_problems(&[(rel, text), tests_sse, tests_sse2]);
            assert_eq!(problems.len(), count, "{rel}: {text}: {problems:?}");
        }
    }

    #[test]
    fn rust_code_pulled_in_by_name_is_scanned() {
        let common = ("crates/ocio-ops/tests/common/cdl.rs", "pub fn f() {}\n");
        // As the GPU tests share the ops tests' helpers, and a sibling by `include!`.
        for text in [
            "#[allow(dead_code)]\n#[path = \"../../ocio-ops/tests/common/cdl.rs\"]\nmod cdl;\n",
            "#[path = \"../../ocio-ops/tests/./common/cdl.rs\"]\nmod cdl;\n",
            "#[cfg_attr(test, path = \"../../ocio-ops/tests/common/cdl.rs\")]\nmod cdl;\n",
            "fn f() { include!(\"../../ocio-ops/tests/common/cdl.rs\"); }\n",
            "const S: &str = include_str!(\"data.txt\");\n",
            "const B: &[u8] = include_bytes!(\"../../../../x.bin\");\n",
        ] {
            assert_eq!(
                source_file_problems(&[("crates/ocio-gpu/tests/t.rs", text), common]),
                Vec::<String>::new(),
                "{text}"
            );
        }
        // Code no guard reads: a file outside the members, of another extension, one that
        // leaves the root, an absolute path, a computed one, a `#[path]` inside an inline
        // module (resolved from elsewhere).
        for text in [
            "#[path = \"../../../tools/evil.rs\"]\nmod evil;\n",
            "#[path = \"../../ocio-ops/tests/common/cdl.txt\"]\nmod cdl;\n",
            "#[path = \"../../../../outside.rs\"]\nmod m;\n",
            "#[path = \"/abs/x.rs\"]\nmod m;\n",
            "#[path = \"C:\\\\x.rs\"]\nmod m;\n",
            "#[cfg_attr(test, path = \"evil.txt\")]\nmod m;\n",
            "include!(concat!(env!(\"OUT_DIR\"), \"/x.rs\"));\n",
            "include!(\"evil.txt\");\n",
            "include!(\"../../ocio-ops/tests/common/cdl.rs\", \"x\");\n",
            "macro_rules! inc {\n    ($p:literal) => {\n        include!($p);\n    };\n}\n",
            "mod a {\n    #[path = \"../../ocio-ops/tests/common/cdl.rs\"]\n    mod cdl;\n}\n",
        ] {
            let problems = source_file_problems(&[("crates/ocio-gpu/tests/t.rs", text), common]);
            assert_eq!(problems.len(), 1, "{text}: {problems:?}");
            assert!(
                problems[0].contains("Rust code pulled in by name"),
                "{problems:?}"
            );
        }
        assert_eq!(
            resolve("a/b/c.rs", "../x/./y.rs").as_deref(),
            Some("a/x/y.rs")
        );
        assert_eq!(resolve("a/c.rs", "../../y.rs"), None);
    }

    #[test]
    fn manifests_build_scanned_sources_only() {
        let problems = |rel: &str, text: &str| check_manifest_sources(rel, text, &[]);
        let member = "crates/a/Cargo.toml";
        // Inside the crate's directory: fine.
        for text in [
            "[package]\nname = \"a\"\nbuild = \"build.rs\"\n",
            "[package]\nbuild = false\n",
            "[lib]\npath = \"src/x.rs\"\n",
            "[[bin]]\nname = \"b\"\npath = \"src/bin/b.rs\"\n",
            "[[test]]\nname = \"t\"\npath = \"tests/./t.rs\"\n",
        ] {
            assert_eq!(problems(member, text), Vec::<String>::new(), "{text}");
        }
        // Outside it, or not a path; a procedural macro crate.
        for text in [
            "[package]\nbuild = \"../b/build.rs\"\n",
            "[package]\nbuild = 1\n",
            "[lib]\npath = \"../../tools/x.rs\"\n",
            "[lib]\npath = \"/abs/x.rs\"\n",
            "[[bin]]\nname = \"b\"\npath = \"../b.rs\"\n",
            "[[test]]\nname = \"t\"\npath = \"../t.rs\"\n",
            "[[example]]\nname = \"e\"\npath = \"../e.rs\"\n",
            "[[bench]]\nname = \"b\"\npath = \"../b.rs\"\n",
            "[lib]\nproc-macro = true\n",
            "[lib]\nproc_macro = true\n",
            "lib = { proc-macro = true }\n",
            "lib.proc-macro = true\n",
        ] {
            assert_eq!(problems(member, text).len(), 1, "{text}");
        }
        assert_eq!(
            problems("crates/ocio-py/Cargo.toml", "[lib]\nproc-macro = true\n").len(),
            1
        );
        // The root: path dependencies are members, every member has a manifest the guards
        // read, no `[patch]` or `[replace]`.
        let root = "[workspace]\nmembers = [\"crates/a\"]\n\n[workspace.dependencies]\n\
                    a = { path = \"crates/a\" }\nserde = \"=1.0.0\"\n";
        let manifests = [("crates/a/Cargo.toml".to_string(), String::new())];
        assert_eq!(
            check_manifest_sources("Cargo.toml", root, &manifests),
            Vec::<String>::new()
        );
        assert_eq!(check_manifest_sources("Cargo.toml", root, &[]).len(), 1);
        for extra in [
            "b = { path = \"tools/b\" }\n",
            "c = { path = \"crates/a/\" }\n",
        ] {
            let text = format!("{root}{extra}");
            assert_eq!(
                check_manifest_sources("Cargo.toml", &text, &manifests).len(),
                1,
                "{text}"
            );
        }
        for extra in [
            "\n[patch.crates-io]\nserde = { path = \"x\" }\n",
            "\n[replace]\n\"serde:1.0.0\" = { path = \"x\" }\n",
        ] {
            let text = format!("{root}{extra}");
            assert_eq!(
                check_manifest_sources("Cargo.toml", &text, &manifests).len(),
                1,
                "{text}"
            );
        }
        assert_eq!(
            check_manifest_sources("Cargo.toml", "[workspace]\nmembers = [\"c/*\"]\n", &[]).len(),
            1
        );
    }

    /// `tokenize`'s tokens of `text`, as (text, kind).
    fn tokens(text: &str) -> Vec<(String, TokenKind)> {
        tokenize(text)
            .tokens
            .into_iter()
            .map(|t| (t.text, t.kind))
            .collect()
    }

    #[test]
    fn the_tokenizer_takes_every_literal_whole() {
        use TokenKind::{Ident, Lifetime, Literal, Punct};
        let tok = |text: &str, kind| (text.to_string(), kind);
        // Character literals with an escape, a quote among them (G17c), byte literals (G17f),
        // a raw identifier (G17h), lifetimes.
        let cases: [(&str, Vec<(String, TokenKind)>); 9] = [
            (
                "'\\\"' x \"",
                vec![tok("'\\\"'", Literal), tok("x", Ident), tok("\"", Literal)],
            ),
            ("'\\'' x", vec![tok("'\\''", Literal), tok("x", Ident)]),
            ("'\\\\' x", vec![tok("'\\\\'", Literal), tok("x", Ident)]),
            (
                "'\\u{1F600}' x",
                vec![tok("'\\u{1F600}'", Literal), tok("x", Ident)],
            ),
            ("'\"' x", vec![tok("'\"'", Literal), tok("x", Ident)]),
            (
                "b'x' b'\\'' y",
                vec![
                    tok("b'x'", Literal),
                    tok("b'\\''", Literal),
                    tok("y", Ident),
                ],
            ),
            (
                "r#unsafe r#a_1 x",
                vec![tok("unsafe", Ident), tok("a_1", Ident), tok("x", Ident)],
            ),
            (
                "<'a, 'static> 'b: x",
                vec![
                    tok("<", Punct),
                    tok("'a", Lifetime),
                    tok(",", Punct),
                    tok("'static", Lifetime),
                    tok(">", Punct),
                    tok("'b", Lifetime),
                    tok(":", Punct),
                    tok("x", Ident),
                ],
            ),
            (
                "r#\"a \"b\"# br\"c\\\" x",
                vec![
                    tok("r#\"a \"b\"#", Literal),
                    tok("br\"c\\\"", Literal),
                    tok("x", Ident),
                ],
            ),
        ];
        for (text, want) in cases {
            assert_eq!(tokens(text), want, "{text}");
        }
        // What a hidden `unsafe` would need: a quote in a character literal starts no string.
        assert_eq!(
            unsafe_tokens("let q = '\\\"'; unsafe { x } let s = \"\";"),
            [1]
        );
        assert_eq!(
            unsafe_tokens("let q = b'\"'; unsafe { x } let s = \"\";"),
            [1]
        );
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
