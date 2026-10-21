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
/// decisions 2026-10-17), and, for the Python binding, the crate `ocio-py-ffi`, one file that
/// calls `PyType_FromMetaclass`, `PyInstanceMethod_New` (owner decision 2026-10-20, option A),
/// `PyCFunction_NewEx` and `PySequence_Check` (2026-10-21), each named here by its file;
/// [`check_py_ffi`] checks that file's calls and [`PY_FFI_SHA256`] pins it.
fn unsafe_allowed(rel: &str) -> bool {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    rel == PY_FFI
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
                "{rel}: the Python binding's crate root must hold `#![forbid(unsafe_code)]`"
            ));
        }
        if !unsafe_allowed(rel) {
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
        problems.extend(check_py_manifest(rel, text));
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
const PY_FFI_SHA256: &str = "10ffda1bb6637b1d4a056566e779e8ee354775788c5f0e864085893d9f15667e";
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
/// The owner's other call, which returns an int.
const PY_FFI_CHECK: &str = "ffi::PySequence_Check";
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
///   `let`), its only wrapping; or a [`PY_FFI_CHECK`] anywhere.
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
        } else if name != PY_FFI_CHECK {
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

/// Whether the tokens of `text` hold the attribute `#![forbid(unsafe_code)]`.
fn forbids_unsafe_code(text: &str) -> bool {
    let toks = tokenize(text).tokens;
    let want = ["#", "!", "[", "forbid", "(", "unsafe_code", ")", "]"];
    toks.windows(want.len())
        .any(|w| w.iter().map(|t| t.text.as_str()).eq(want))
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
        let cases: [(&str, &[usize]); 17] = [
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
                    unsafe { Bound::from_owned_ptr_or_err(record.py(), r) }\n}\n";
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
            // The wrapping takes the previous statement's owner's call result, once.
            "fn a(raw: *mut O) { unsafe { Bound::from_owned_ptr_or_err(py, raw) } }",
            "fn a() { let p = g(); unsafe { Bound::from_owned_ptr_or_err(py, p) } }",
            "fn a() { let p = unsafe { ffi::PySequence_Check(o) }; unsafe { Bound::from_owned_ptr_or_err(py, p) } }",
            "fn a(ptr: *mut O) { let q = unsafe { PyInstanceMethod_New(f) }; unsafe { Bound::from_owned_ptr_or_err(py, ptr) } }",
            "fn a() { let p = unsafe { PyInstanceMethod_New(f) }; unsafe { Bound::from_owned_ptr_or_err(py, p, q) } }",
            "fn a() { let p = unsafe { PyInstanceMethod_New(f) }; unsafe { Bound::from_owned_ptr_or_err(p) } }",
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
        ] {
            assert!(unsafe_allowed(rel), "{rel}");
        }
        for rel in [
            "crates/ocio-ops/src/lib.rs",
            "crates/ocio-gpu/src/sse2.rs",
            "crates/ocio/src/minizip/os_windows.rs",
            "crates/ocio-testkit/src/lib.rs",
            "xtask/src/guards.rs",
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
