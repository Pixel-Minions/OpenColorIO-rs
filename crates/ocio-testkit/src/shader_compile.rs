// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Compile checks of generated shaders (owner decision P7-5, `docs/cards/phase7.md` WP 7.4).
//!
//! `cargo xtask compile-shaders` runs two phases:
//! 1. **Capture.** With [`CORPUS_ENV`] set, the family oracle tests
//!    (`crates/ocio-gpu/tests/*_oracle.rs`) and upstream's GPU suite, in its text-only build
//!    (`ocio-gpu-exec`'s `shader-corpus` feature), write the wheel's shaders under that
//!    directory. Both assert the port's text equal to the wheel's first, so a captured shader
//!    is the port's as well, byte for byte.
//! 2. **Compile.** `compile_glsl.rs` (Rocky Linux 9) and `compile_hlsl.rs` (Windows) wrap each
//!    captured shader in a minimal shader of its language ([`wrap`]) and give it to the
//!    platform's compiler ([`check`]). The compiler's verdict is the expected value.
//!
//! There is no list of shaders that may fail. Upstream's own text doesn't compile for four
//! reasons, which the owner accepted as upstream's limits (P7-9, 2026-11-02): the [`Cause`]s. A
//! shader that fails passes the check only when [`explain`] shows why, three ways: its text holds
//! the form of a cause that applies to this corpus and language; the compiler's first error is that
//! cause's, on a line that holds the form; and with every form it holds repaired, the shader
//! compiles. The last is what stops a real bug: a compiler stops at the first syntax error, so a
//! bug behind a cause's literal shows only once the literal is gone. Any other failure fails the
//! check, and so does a cause that no longer explains any shader of a check it applies to.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use crate::fixtures::sha256_hex;
use crate::gpu::{GpuLanguage, Shader};

/// The directory that receives the corpus. When it is unset nothing is recorded.
pub const CORPUS_ENV: &str = "OCIO_RS_SHADER_CORPUS";

/// Upstream's GPU tests (`tests/gpu/*_test.cpp` @ v2.5.2, `docs/cards/phase7.md` WP 7.3).
pub const SUITE_TESTS: usize = 264;

/// The GLSL languages glslang checks (P7-5).
pub const GLSL_LANGUAGES: [GpuLanguage; 6] = [
    GpuLanguage::Glsl12,
    GpuLanguage::Glsl13,
    GpuLanguage::Glsl40,
    GpuLanguage::GlslEs10,
    GpuLanguage::GlslEs30,
    GpuLanguage::GlslVk46,
];

/// Every language with a compiler on a reference platform (P7-5). The other three (MSL 2.0,
/// OSL 1, Cg) have none: text parity only.
pub const COMPILED_LANGUAGES: [GpuLanguage; 7] = [
    GpuLanguage::Glsl12,
    GpuLanguage::Glsl13,
    GpuLanguage::Glsl40,
    GpuLanguage::GlslEs10,
    GpuLanguage::GlslEs30,
    GpuLanguage::GlslVk46,
    GpuLanguage::HlslSm50,
];

/// The family oracle targets that must contribute to every compiled language. Each extracts
/// real shaders; the description tests drive `GpuShaderDesc` with arbitrary text instead.
pub const FAMILY_TARGETS: [&str; 15] = [
    "cdl_op_gpu_oracle",
    "exponent_op_gpu_oracle",
    "exposure_contrast_op_gpu_oracle",
    "fixed_function_op_gpu_oracle",
    "gamma_op_gpu_oracle",
    "gpu_processor_oracle",
    "grading_hue_curve_op_gpu_oracle",
    "grading_primary_op_gpu_oracle",
    "grading_rgb_curve_op_gpu_oracle",
    "grading_tone_op_gpu_oracle",
    "log_op_gpu_oracle",
    "lut1d_op_gpu_oracle",
    "lut3d_op_gpu_oracle",
    "matrix_op_gpu_oracle",
    "range_op_gpu_oracle",
];

/// Where a captured shader comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corpus {
    /// Upstream's 264 GPU tests, keyed `<group>.<name>`.
    Suite,
    /// The family oracle tests, keyed by test target.
    Family,
}

impl Corpus {
    fn directory(self) -> &'static str {
        match self {
            Corpus::Suite => "suite",
            Corpus::Family => "family",
        }
    }
}

/// Why a wheel shader can't compile: a limit of upstream's text, which the port copies byte for
/// byte. Each is an entry of `docs/improvements.md`, known by what the shader's text holds, never
/// by which shader it is (P7-9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cause {
    /// A whole number past `max_digits10`, written `1e+30.` (I-810).
    LargeWholeNumber,
    /// A non-finite parameter, written `nan`, `inf`, `-nan(ind)` (I-811).
    NonFinite,
    /// An array constructor, which GLSL ES 1.00 doesn't have (I-812).
    EsArrayConstructor,
    /// A literal with an `f` suffix, which GLSL ES 1.00 doesn't have (I-812).
    EsFloatSuffix,
    /// A function name holding `__`, which GLSL ES reserves (I-812).
    EsReservedName,
    /// A function or pixel name that can't be one: not an ASCII identifier, or the `main` of the
    /// shader that calls it (I-813).
    Name,
}

/// The line upstream writes before the function's declaration (`GpuShaderText`).
const DECLARATION: &str = "// Declaration of the OCIO shader function";

/// The name a repaired shader's function gets.
const REPAIRED_NAME: &str = "ocioRepairedName";

fn is_identifier_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn after_identifier(text: &[u8], at: usize) -> bool {
    at > 0 && (is_identifier_byte(text[at - 1]) || text[at - 1] >= 0x80)
}

fn digits_end(text: &[u8], mut at: usize) -> usize {
    while text.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
    }
    at
}

fn find(text: &[u8], what: &[u8]) -> Option<usize> {
    text.windows(what.len()).position(|w| w == what)
}

/// Where `text` holds I-810's literals, numbers with a `.` after their exponent.
fn large_whole_numbers(text: &[u8]) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    for at in 1..text.len().saturating_sub(2) {
        if text[at] == b'e' && text[at + 1] == b'+' && text[at - 1].is_ascii_digit() {
            let end = digits_end(text, at + 2);
            if end > at + 2
                && text.get(end) == Some(&b'.')
                && !text.get(end + 1).is_some_and(u8::is_ascii_digit)
            {
                let mut start = at;
                while start > 0 && (text[start - 1].is_ascii_digit() || text[start - 1] == b'.') {
                    start -= 1;
                }
                found.push(start..end + 1);
            }
        }
    }
    found
}

/// Where `text` holds I-811's literals: the words `nan` and `inf`, with the `(ind)` or `(snan)` a
/// `nan` can carry (`std::ostream`'s text on each platform, I-7).
fn non_finite_literals(text: &[u8]) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut at = 0;
    while at + 3 <= text.len() {
        let word = &text[at..at + 3];
        let whole = (word == b"nan" || word == b"inf")
            && !after_identifier(text, at)
            && (at == 0 || text[at - 1] != b'.')
            && !text
                .get(at + 3)
                .is_some_and(|b| is_identifier_byte(*b) || *b >= 0x80);
        if whole {
            let mut end = at + 3;
            for detail in [&b"(ind)"[..], b"(snan)"] {
                if word == b"nan" && text[end..].starts_with(detail) {
                    end += detail.len();
                }
            }
            found.push(at..end);
            at = end;
        } else {
            at += 1;
        }
    }
    found
}

/// Where `text` holds I-812's tables, `const float n[3] = float[3](...);`, each with the end of
/// its name's `[3]`.
fn array_constructors(text: &[u8]) -> Vec<(Range<usize>, usize)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(start) = find(&text[from..], b"const ").map(|at| at + from) {
        from = start + 6;
        if after_identifier(text, start) {
            continue;
        }
        let Some(equals) = find(&text[start..], b"] = ").map(|at| at + start) else {
            break;
        };
        // Only this statement.
        if text[start..equals].contains(&b';') {
            continue;
        }
        let rest = &text[equals + 4..];
        if !(rest.starts_with(b"float[") || rest.starts_with(b"int[")) {
            continue;
        }
        let Some(end) = find(rest, b");").map(|at| at + equals + 4 + 2) else {
            break;
        };
        found.push((start..end, equals + 1));
        from = end;
    }
    found
}

/// Where `text` holds I-812's literals: the `f` of a number with that suffix.
fn float_suffixes(text: &[u8]) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let number = text[at].is_ascii_digit()
            || (text[at] == b'.' && text.get(at + 1).is_some_and(u8::is_ascii_digit));
        if !number || after_identifier(text, at) {
            at += 1;
            continue;
        }
        let mut end = digits_end(text, at);
        if text.get(end) == Some(&b'.') {
            end = digits_end(text, end + 1);
        }
        if text.get(end) == Some(&b'e') {
            let sign = usize::from(matches!(text.get(end + 1), Some(b'+' | b'-')));
            end = digits_end(text, end + 1 + sign);
        }
        if text.get(end) == Some(&b'f')
            && !text.get(end + 1).copied().is_some_and(is_identifier_byte)
        {
            found.push(end..end + 1);
        }
        at = end.max(at + 1);
    }
    found
}

/// Whether `name` can name the function a shader's `main` calls.
fn is_usable_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && !bytes[0].is_ascii_digit()
        && bytes.iter().copied().all(is_identifier_byte)
        && name != "main"
}

impl Cause {
    /// Every cause.
    pub const ALL: [Cause; 6] = [
        Cause::LargeWholeNumber,
        Cause::NonFinite,
        Cause::EsArrayConstructor,
        Cause::EsFloatSuffix,
        Cause::EsReservedName,
        Cause::Name,
    ];

    /// Its entry of `docs/improvements.md`.
    pub fn entry(self) -> &'static str {
        match self {
            Cause::LargeWholeNumber => "I-810",
            Cause::NonFinite => "I-811",
            Cause::EsArrayConstructor | Cause::EsFloatSuffix | Cause::EsReservedName => "I-812",
            Cause::Name => "I-813",
        }
    }

    /// Whether the cause can explain a failure of `corpus` in `language`, and must explain one:
    /// [`check`] fails when such a cause explains none. Upstream's 264 tests hold no non-finite
    /// or very large parameter and no such name, so outside GLSL ES 1.0 no cause applies to them.
    pub fn applies(self, corpus: Corpus, language: GpuLanguage) -> bool {
        match self {
            Cause::LargeWholeNumber | Cause::NonFinite | Cause::Name => corpus == Corpus::Family,
            Cause::EsArrayConstructor | Cause::EsFloatSuffix => language == GpuLanguage::GlslEs10,
            Cause::EsReservedName => corpus == Corpus::Suite && language == GpuLanguage::GlslEs10,
        }
    }

    /// The places in `text` that hold the cause's form.
    fn spans(self, text: &str) -> Vec<Range<usize>> {
        let bytes = text.as_bytes();
        match self {
            Cause::LargeWholeNumber => large_whole_numbers(bytes),
            Cause::NonFinite => non_finite_literals(bytes),
            Cause::EsArrayConstructor => array_constructors(bytes)
                .into_iter()
                .map(|(span, _)| span)
                .collect(),
            Cause::EsFloatSuffix => float_suffixes(bytes),
            Cause::EsReservedName => Vec::new(),
            Cause::Name => text
                .char_indices()
                .filter(|(_, c)| !c.is_ascii())
                .map(|(at, c)| at..at + c.len_utf8())
                .collect(),
        }
    }

    /// Whether a shader of this function name and text holds the cause's form.
    fn holds(self, function_name: &str, text: &str) -> bool {
        match self {
            Cause::EsReservedName => function_name.contains("__"),
            Cause::Name => !is_usable_name(function_name) || !self.spans(text).is_empty(),
            _ => !self.spans(text).is_empty(),
        }
    }

    /// Whether `line` of the wrapped shader can be where the compiler meets the cause's form.
    fn is_on(self, function_name: &str, line: &str) -> bool {
        match self {
            Cause::EsReservedName => line.contains(function_name),
            Cause::Name => !line.is_ascii() || line.contains('('),
            _ => !self.spans(line).is_empty(),
        }
    }

    /// The compiler's words for the cause's form, as a part of its message.
    fn words(self, tool: Tool) -> &'static [&'static str] {
        match (self, tool) {
            (Cause::LargeWholeNumber, Tool::Fxc) => &["syntax error: unexpected token"],
            (Cause::LargeWholeNumber, Tool::Dxc) => {
                &["expected unqualified-id", "invalid suffix '.' on floating"]
            }
            (Cause::NonFinite, _) => &["undeclared identifier"],
            (Cause::Name, Tool::Fxc) => &[
                "Illegal character",
                "unexpected token '('",
                "redefinition of 'main'",
            ],
            (Cause::Name, Tool::Dxc) => &[
                "non-ASCII characters",
                "expected ')'",
                "redefinition of 'main'",
            ],
            (cause, Tool::Glslang) => cause.glslang_words(),
            (_, Tool::Fxc | Tool::Dxc) => &[],
        }
    }

    fn glslang_words(self) -> &'static [&'static str] {
        match self {
            Cause::LargeWholeNumber => &["syntax error, unexpected"],
            Cause::NonFinite => &["undeclared identifier"],
            Cause::EsArrayConstructor => &["'arrayed constructor' : not supported"],
            Cause::EsFloatSuffix => &["'floating-point suffix' : not supported"],
            Cause::EsReservedName => &["consecutive underscores"],
            Cause::Name => &[
                "syntax error, unexpected LEFT_PAREN",
                "'main' : function cannot take any parameter(s)",
                "unexpected token",
            ],
        }
    }
}

/// `text` with each span replaced by what `by` makes of it.
fn replaced(text: &str, spans: &[Range<usize>], by: impl Fn(&Range<usize>) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for span in spans {
        out.push_str(&text[at..span.start]);
        out.push_str(&by(span));
        at = span.end;
    }
    out.push_str(&text[at..]);
    out
}

/// `text` with its function renamed where upstream declares it, on the line after
/// [`DECLARATION`].
fn renamed(text: &str, function_name: &str) -> String {
    let open = text
        .find(DECLARATION)
        .and_then(|at| text[at..].find('(').map(|open| open + at))
        .unwrap_or_else(|| panic!("no function's declaration in:\n{text}"));
    let start = open - function_name.len();
    assert_eq!(&text[start..open], function_name, "in:\n{text}");
    format!("{}{REPAIRED_NAME}{}", &text[..start], &text[open..])
}

/// The function name and text of a shader with every form of `causes` written so that it can
/// compile. What the literals stood for doesn't matter: only whether the rest compiles.
fn repaired(causes: &[Cause], function_name: &str, text: &str) -> (String, String) {
    let mut name = function_name.to_string();
    let mut text = text.to_string();
    if causes.contains(&Cause::Name) || causes.contains(&Cause::EsReservedName) {
        text = renamed(&text, &name);
        name = REPAIRED_NAME.to_string();
    }
    for cause in causes {
        let spans = cause.spans(&text);
        text = match cause {
            // Not the number itself: past a float's range, it fails for that (as a `log` base,
            // dxc's validation refuses the constant).
            Cause::LargeWholeNumber | Cause::NonFinite => {
                replaced(&text, &spans, |_| "2.0".to_string())
            }
            Cause::EsFloatSuffix => replaced(&text, &spans, |_| String::new()),
            Cause::Name => replaced(&text, &spans, |_| "x".to_string()),
            Cause::EsArrayConstructor => {
                let tables = array_constructors(text.as_bytes());
                replaced(&text, &spans, |span| {
                    let (_, name_end) = tables.iter().find(|(s, _)| s == span).unwrap();
                    format!("{};", &text[span.start + "const ".len()..*name_end])
                })
            }
            Cause::EsReservedName => text,
        };
    }
    (name, text)
}

/// A compiler's first error: its line of the wrapped shader, and its message.
fn first_error(tool: Tool, output: &str) -> Option<(usize, String)> {
    output.lines().find_map(|line| {
        let (number, message) = match tool {
            // `ERROR: <file>:12: 'inf' : undeclared identifier`.
            Tool::Glslang => {
                let (file, message) = line.strip_prefix("ERROR: ")?.split_once(": ")?;
                (file.rsplit(':').next()?, message)
            }
            // `<file>(12,34-36): error X3004: undeclared identifier 'nan'`.
            Tool::Fxc => {
                let (position, message) = line.split_once("): error ")?;
                (position.rsplit_once('(')?.1.split(',').next()?, message)
            }
            // `<file>:12:34: error: use of undeclared identifier 'nan'`.
            Tool::Dxc => {
                let (position, message) = line.split_once(": error: ")?;
                let mut parts = position.rsplit(':');
                parts.next()?;
                (parts.next()?, message)
            }
        };
        Some((number.parse().ok()?, message.to_string()))
    })
}

/// Why `entry` failed with `output` in `language`, or why that is no accepted reason (module
/// docs). `compile` compiles a wrapped shader as the check does.
pub fn explain(
    corpus: Corpus,
    language: GpuLanguage,
    tool: Tool,
    entry: &Entry,
    output: &str,
    compile: impl FnOnce(&str) -> Result<(), String>,
) -> Result<Vec<Cause>, String> {
    let name = &entry.function_name;
    let causes: Vec<Cause> = Cause::ALL
        .into_iter()
        .filter(|cause| cause.applies(corpus, language) && cause.holds(name, &entry.text))
        .collect();
    if causes.is_empty() {
        return Err("its text holds the form of no cause".to_string());
    }
    let wrapped = wrap(language, name, &entry.text);
    let Some((number, message)) = first_error(tool, output) else {
        return Err("the compiler's output holds no error".to_string());
    };
    let line = wrapped.lines().nth(number.wrapping_sub(1)).unwrap_or("");
    let is_theirs = |cause: &Cause| {
        cause.is_on(name, line) && cause.words(tool).iter().any(|w| message.contains(w))
    };
    if !causes.iter().any(is_theirs) {
        return Err(format!(
            "its first error is not {causes:?}'s: line {number}: {message}"
        ));
    }
    let (name, text) = repaired(&causes, name, &entry.text);
    compile(&wrap(language, &name, &text))
        .map_err(|error| format!("with {causes:?} repaired it still fails: {}", cause(&error)))?;
    Ok(causes)
}

/// A compiler (P7-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// `glslangValidator`, `-V` for Vulkan GLSL.
    Glslang,
    /// The Windows SDK's `fxc.exe /T ps_5_0`.
    Fxc,
    /// The Windows SDK's `dxc.exe -T ps_6_0`.
    Dxc,
}

fn language_of(name: &str) -> GpuLanguage {
    GpuLanguage::ALL
        .into_iter()
        .find(|language| language.oracle_name() == name)
        .unwrap_or_else(|| panic!("unknown shader language {name}"))
}

/// The configured corpus directory.
pub fn corpus_dir() -> Option<PathBuf> {
    std::env::var_os(CORPUS_ENV).map(PathBuf::from)
}

/// Writes one shader under `dir`. Atomic: tests of one target share the directory.
pub fn record(
    dir: &Path,
    corpus: Corpus,
    source: &str,
    language: GpuLanguage,
    function_name: &str,
    text: &str,
) {
    assert!(
        !source.is_empty()
            && source
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
        "a corpus source must be a plain file name: {source:?}"
    );
    let target = dir
        .join(corpus.directory())
        .join(source)
        .join(language.oracle_name());
    std::fs::create_dir_all(&target).unwrap();
    let body = format!("{function_name}\n{text}");
    let name = sha256_hex(body.as_bytes());
    let path = target.join(format!("{name}.txt"));
    if path.exists() {
        return;
    }
    let scratch = target.join(format!(
        "{name}.{}.{:?}.tmp",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&scratch, body).unwrap();
    if let Err(error) = std::fs::rename(&scratch, &path) {
        // Another thread of this target can finish the same shader first.
        std::fs::remove_file(&scratch).unwrap();
        assert!(path.exists(), "{}: {error}", path.display());
    }
}

/// The test target of this process, from `<target>-<hash>[.exe]`.
fn test_target() -> String {
    let exe = std::env::current_exe().unwrap();
    let stem = exe.file_stem().unwrap().to_str().unwrap();
    stem.rsplit_once('-')
        .map_or(stem, |(target, _)| target)
        .to_string()
}

/// [`crate::gpu::GpuShaderReply::from_response`]'s hook: records a family oracle test's
/// wheel shader. Only `*_oracle` targets record; those tests compare the port with it.
pub(crate) fn record_family(shader: &Shader) {
    let Some(dir) = corpus_dir() else {
        return;
    };
    let target = test_target();
    if !target.ends_with("_oracle") {
        return;
    }
    let getter = |key: &str| {
        shader.getters[key]
            .as_str()
            .unwrap_or_else(|| panic!("{key} in {}", shader.getters))
    };
    record(
        &dir,
        Corpus::Family,
        &target,
        language_of(getter("language")),
        getter("function_name"),
        &shader.text,
    );
}

/// One distinct captured shader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `sha256(function name, newline, text)`.
    pub digest: String,
    /// The shader's function, which the wrapper calls.
    pub function_name: String,
    /// The shader's text.
    pub text: String,
    /// The tests or targets that produced it, sorted.
    pub sources: Vec<String>,
}

/// Every source of `corpus`, with the distinct shaders of `language` it recorded.
fn read_sources(
    dir: &Path,
    corpus: Corpus,
    language: GpuLanguage,
) -> BTreeMap<String, Vec<PathBuf>> {
    let root = dir.join(corpus.directory());
    let mut sources = BTreeMap::new();
    for source in std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("{}: {e}: run `cargo xtask compile-shaders`", root.display()))
    {
        let source = source.unwrap();
        let name = source.file_name().into_string().unwrap();
        let mut shaders = Vec::new();
        if let Ok(files) = std::fs::read_dir(source.path().join(language.oracle_name())) {
            for file in files {
                let path = file.unwrap().path();
                assert_eq!(
                    path.extension().and_then(|e| e.to_str()),
                    Some("txt"),
                    "an unfinished corpus file: {}",
                    path.display()
                );
                shaders.push(path);
            }
        }
        sources.insert(name, shaders);
    }
    sources
}

/// The distinct shaders of `language`, after checking that the capture is complete: the suite
/// has exactly its [`SUITE_TESTS`] tests with one shader each, and every target of
/// [`FAMILY_TARGETS`] recorded at least one.
pub fn load(dir: &Path, corpus: Corpus, language: GpuLanguage) -> Vec<Entry> {
    let sources = read_sources(dir, corpus, language);
    match corpus {
        Corpus::Suite => {
            assert_eq!(sources.len(), SUITE_TESTS, "suite tests in the corpus");
            for (source, shaders) in &sources {
                assert_eq!(shaders.len(), 1, "{source} in {}", language.oracle_name());
            }
        }
        Corpus::Family => {
            for target in FAMILY_TARGETS {
                let count = sources.get(target).map_or(0, Vec::len);
                assert!(count > 0, "{target} recorded no {language:?} shader");
            }
        }
    }
    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    for (source, shaders) in sources {
        for path in shaders {
            let digest = path.file_stem().unwrap().to_str().unwrap().to_string();
            let entry = entries.entry(digest.clone()).or_insert_with(|| {
                let body = std::fs::read_to_string(&path).unwrap();
                assert_eq!(digest, sha256_hex(body.as_bytes()), "{}", path.display());
                let (function_name, text) = body.split_once('\n').unwrap();
                Entry {
                    digest,
                    function_name: function_name.to_string(),
                    text: text.to_string(),
                    sources: Vec::new(),
                }
            });
            entry.sources.push(source.clone());
        }
    }
    let entries: Vec<_> = entries.into_values().collect();
    assert!(!entries.is_empty(), "no {language:?} shader in the corpus");
    entries
}

/// `text` in a minimal fragment or pixel shader of its language: `main`, the input, the output.
///
/// The order is upstream's `OpenGLBuilder::buildProgram` (src/libutils/oglapphelpers/glsl.cpp:
/// 483-510 @ v2.5.2), with the version strings of `getGLSLVersionString` (:454-481): the
/// version, the shader, the client's `main`. GLSL 1.2's `main` is `OglApp::setShader`'s
/// (src/libutils/oglapphelpers/oglapp.cpp:206-215). GLSL ES needs a default float precision and
/// ES 1.0 the 3D texture extension before the shader's declarations; upstream has no GL ES
/// client to port, so these lines and the `main`s of the other languages are this check's own.
pub fn wrap(language: GpuLanguage, function_name: &str, text: &str) -> String {
    let (head, main) = match language {
        GpuLanguage::Glsl12 => (
            "#version 120\n",
            format!(
                "\nuniform sampler2D img;\n\nvoid main()\n{{\n    \
                 vec4 col = texture2D(img, gl_TexCoord[0].st);\n    \
                 gl_FragColor = {function_name}(col);\n}}\n"
            ),
        ),
        GpuLanguage::Glsl13 | GpuLanguage::Glsl40 | GpuLanguage::GlslEs30 => (
            match language {
                GpuLanguage::Glsl13 => "#version 130\n",
                GpuLanguage::Glsl40 => "#version 400 core\n",
                _ => "#version 300 es\nprecision highp float;\nprecision highp sampler3D;\n",
            },
            format!(
                "\nin vec4 ocioInput;\nout vec4 ocioOutput;\n\nvoid main()\n{{\n    \
                 ocioOutput = {function_name}(ocioInput);\n}}\n"
            ),
        ),
        GpuLanguage::GlslEs10 => (
            "#version 100\n#extension GL_OES_texture_3D : enable\n\
             precision highp float;\nprecision highp sampler3D;\n",
            format!(
                "\nvarying vec4 ocioInput;\n\nvoid main()\n{{\n    \
                 gl_FragColor = {function_name}(ocioInput);\n}}\n"
            ),
        ),
        GpuLanguage::GlslVk46 => (
            "#version 460 core\n",
            format!(
                "\nlayout(location = 0) in vec4 ocioInput;\n\
                 layout(location = 0) out vec4 ocioOutput;\n\nvoid main()\n{{\n    \
                 ocioOutput = {function_name}(ocioInput);\n}}\n"
            ),
        ),
        GpuLanguage::HlslSm50 => (
            "",
            format!(
                "\nfloat4 main(float4 ocioInput : COLOR0) : SV_TARGET\n{{\n    \
                 return {function_name}(ocioInput);\n}}\n"
            ),
        ),
        GpuLanguage::Cg | GpuLanguage::Osl1 | GpuLanguage::Msl20 => {
            panic!("{language:?} has no compile check (P7-5)")
        }
    };
    format!("{head}{text}\n{main}")
}

/// The numeric parts of a Windows SDK version directory, to sort `10.0.9` before `10.0.10`.
fn sdk_version(path: &Path) -> Option<Vec<u32>> {
    let name = path.file_name()?.to_str()?;
    name.split('.').map(|part| part.parse().ok()).collect()
}

/// The newest `Windows Kits/10/bin/<version>/x64` with both compilers, as P7-5 asks: located at
/// run time, never installed. No SDK is an error, not a skip.
pub fn windows_sdk_bin() -> PathBuf {
    let root = Path::new("C:/Program Files (x86)/Windows Kits/10/bin");
    let mut found: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(root)
        .unwrap_or_else(|e| panic!("no Windows SDK at {}: {e}", root.display()))
        .filter_map(|entry| {
            let version = entry.ok()?.path();
            let bin = version.join("x64");
            (bin.join("fxc.exe").is_file() && bin.join("dxc.exe").is_file())
                .then_some((sdk_version(&version)?, bin))
        })
        .collect();
    found.sort();
    found
        .pop()
        .unwrap_or_else(|| panic!("no fxc.exe and dxc.exe under {}", root.display()))
        .1
}

/// The command that compiles `source` (a file) into `output`.
fn command(tool: Tool, language: GpuLanguage, source: &Path, output: &Path) -> Command {
    match tool {
        Tool::Glslang => {
            let mut command = Command::new("glslangValidator");
            command.arg("-l");
            if language == GpuLanguage::GlslVk46 {
                command.arg("-V").arg("-o").arg(output);
            }
            command.arg(source);
            command
        }
        Tool::Fxc => {
            let mut command = Command::new(windows_sdk_bin().join("fxc.exe"));
            command
                .args(["/nologo", "/T", "ps_5_0", "/E", "main", "/Fo"])
                .arg(output)
                .arg(source);
            command
        }
        Tool::Dxc => {
            let mut command = Command::new(windows_sdk_bin().join("dxc.exe"));
            command
                .args(["-T", "ps_6_0", "-E", "main", "-Fo"])
                .arg(output)
                .arg(source);
            command
        }
    }
}

/// Compiles a wrapped shader in `work`. `Err` is the compiler's output.
pub fn compile(
    tool: Tool,
    language: GpuLanguage,
    work: &Path,
    name: &str,
    wrapped: &str,
) -> Result<(), String> {
    std::fs::create_dir_all(work).unwrap();
    let extension = if tool == Tool::Glslang {
        "frag"
    } else {
        "hlsl"
    };
    let source = work.join(format!("{name}.{extension}"));
    let output = work.join(format!("{name}.out"));
    std::fs::write(&source, wrapped).unwrap();
    let run = command(tool, language, &source, &output)
        .output()
        .unwrap_or_else(|e| panic!("could not run {tool:?}: {e}; it is required, not optional"));
    std::fs::remove_file(&source).unwrap();
    if output.exists() {
        std::fs::remove_file(&output).unwrap();
    }
    if run.status.success() {
        return Ok(());
    }
    let mut out = String::from_utf8_lossy(&run.stdout).into_owned();
    out.push_str(&String::from_utf8_lossy(&run.stderr));
    Err(out.replace(&work.display().to_string(), ""))
}

/// The first error of a compiler's output, with its numbers masked, to group failures.
pub fn cause(output: &str) -> String {
    let error = output
        .lines()
        .find(|line| {
            let line = line.to_ascii_lowercase();
            line.contains("error") && !line.contains("compilation errors")
        })
        .unwrap_or(output);
    // Past the file and position, which name the shader rather than the cause.
    let message = error
        .rsplit_once(": error")
        .map_or(error, |(_, message)| message)
        .trim_start_matches("ERROR: ");
    let message = message.split_once(".frag:").map_or(message, |(_, m)| m);
    let mut out = String::new();
    let mut digits = false;
    for c in message.trim().chars().take(100) {
        if c.is_ascii_digit() {
            if !digits {
                out.push('#');
            }
            digits = true;
        } else {
            digits = false;
            out.push(c);
        }
    }
    out
}

/// The compiler threads per check: other work shares the machine, and the tool is a process.
const THREADS: usize = 6;

/// Compiles every entry; the failures, in entry order.
fn compile_entries(
    entries: &[Entry],
    tool: Tool,
    language: GpuLanguage,
    work: &Path,
) -> Vec<(usize, String)> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let failures = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(entry) = entries.get(index) else {
                        return;
                    };
                    let wrapped = wrap(language, &entry.function_name, &entry.text);
                    if let Err(output) = compile(tool, language, work, &entry.digest, &wrapped) {
                        failures.lock().unwrap().push((index, output));
                    }
                }
            });
        }
    });
    let mut failures = failures.into_inner().unwrap();
    failures.sort();
    failures
}

/// What [`explain`] says of each failure, in the failures' order.
fn explain_failures(
    entries: &[Entry],
    failures: &[(usize, String)],
    (corpus, language, tool): (Corpus, GpuLanguage, Tool),
    work: &Path,
) -> Vec<Result<Vec<Cause>, String>> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            scope.spawn(|| {
                loop {
                    let at = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((index, output)) = failures.get(at) else {
                        return;
                    };
                    let entry = &entries[*index];
                    let name = format!("{}-repaired", entry.digest);
                    let explained = explain(corpus, language, tool, entry, output, |wrapped| {
                        compile(tool, language, work, &name, wrapped)
                    });
                    results.lock().unwrap().push((at, explained));
                }
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|(at, _)| *at);
    results.into_iter().map(|(_, result)| result).collect()
}

/// The causes that apply to `corpus` in `language` and explained no failure: `explained` holds
/// what each explained failure was put down to.
fn causes_unused(
    corpus: Corpus,
    language: GpuLanguage,
    explained: &BTreeMap<Cause, usize>,
) -> Vec<Cause> {
    Cause::ALL
        .into_iter()
        .filter(|cause| cause.applies(corpus, language) && !explained.contains_key(cause))
        .collect()
}

/// Compiles every captured shader of `corpus` in `language` with `tool`, writes the verdict to
/// `<dir>/report/<check>.json`, and panics if a shader failed that [`explain`] can't put down to a
/// [`Cause`], with those failures grouped by the compiler's first error, or if a cause that
/// applies here explains no failure.
pub fn check(check: &str, corpus: Corpus, language: GpuLanguage, tool: Tool) {
    let dir = corpus_dir()
        .unwrap_or_else(|| panic!("{CORPUS_ENV} is not set: run `cargo xtask compile-shaders`"));
    let entries = load(&dir, corpus, language);
    let work = dir.join("work").join(check);
    let failures = compile_entries(&entries, tool, language, &work);
    let verdicts = explain_failures(&entries, &failures, (corpus, language, tool), &work);
    let mut explained: BTreeMap<Cause, usize> = BTreeMap::new();
    let mut unexplained = Vec::new();
    for (failure, verdict) in failures.iter().zip(verdicts) {
        match verdict {
            Ok(causes) => {
                for cause in causes {
                    *explained.entry(cause).or_default() += 1;
                }
            }
            Err(why) => unexplained.push((failure, why)),
        }
    }
    let unused = causes_unused(corpus, language, &explained);
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (at, (failure, _)) in unexplained.iter().enumerate() {
        groups.entry(cause(&failure.1)).or_default().push(at);
    }
    let group_json: Vec<Value> = groups
        .iter()
        .map(|(cause, members)| {
            let (failure, why) = &unexplained[members[0]];
            let first = &entries[failure.0];
            json!({
                "cause": cause,
                "count": members.len(),
                "why": why,
                "example": first.digest,
                "example_sources": first.sources,
                "example_output": failure.1,
            })
        })
        .collect();
    let failed: Vec<Value> = unexplained
        .iter()
        .map(|(failure, why)| {
            let entry = &entries[failure.0];
            json!({"digest": entry.digest, "sources": entry.sources, "why": why})
        })
        .collect();
    let cause_json: Vec<Value> = explained
        .iter()
        .map(|(cause, count)| {
            json!({"cause": format!("{cause:?}"), "entry": cause.entry(), "count": count})
        })
        .collect();
    let report = dir.join("report");
    std::fs::create_dir_all(&report).unwrap();
    std::fs::write(
        report.join(format!("{check}.json")),
        serde_json::to_vec_pretty(&json!({
            "check": check,
            "language": language.oracle_name(),
            "tool": format!("{tool:?}"),
            "compiled": entries.len(),
            "passed": entries.len() - failures.len(),
            "explained": failures.len() - unexplained.len(),
            "failed": unexplained.len(),
            "causes": cause_json,
            "unused_causes": unused.iter().map(|c| format!("{c:?}")).collect::<Vec<_>>(),
            "groups": group_json,
            "failures": failed,
        }))
        .unwrap(),
    )
    .unwrap();
    let mut summary = String::new();
    for (cause, members) in &groups {
        summary.push_str(&format!(
            "\n  {:6} {cause}: {}",
            members.len(),
            unexplained[members[0]].1
        ));
    }
    assert!(
        unexplained.is_empty(),
        "{check}: {} of {} distinct shaders failed in {tool:?} with no cause:{summary}",
        unexplained.len(),
        entries.len()
    );
    assert_every_cause_used(check, &unused);
}

/// A cause no shader needs any more must leave the rule, or its text check has gone wrong.
fn assert_every_cause_used(check: &str, unused: &[Cause]) {
    assert!(
        unused.is_empty(),
        "{check}: {unused:?} explain no failure: the shaders that held them are gone or compile \
         now, so the rule (docs/cards/phase7.md, P7-9) and docs/improvements.md need a change"
    );
}

/// The checks every compile target runs on each language's own tiny, valid shader and its
/// broken forms: a compiler that accepts everything, or a wrapper that hides the shader, fails
/// here.
pub fn check_controls(check: &str, tool: Tool, language: GpuLanguage, valid: &str) {
    let dir = corpus_dir()
        .unwrap_or_else(|| panic!("{CORPUS_ENV} is not set: run `cargo xtask compile-shaders`"));
    let work = dir.join("work").join(check);
    let good = wrap(language, "OCIOMain", valid);
    if let Err(output) = compile(tool, language, &work, "valid", &good) {
        panic!("{tool:?} refused a valid {language:?} shader:\n{good}\n{output}");
    }
    // The shader's own text is compiled: a syntax error, an unknown identifier and a type error
    // in it, and a shader that doesn't define the function the wrapper calls.
    let broken = [
        valid.replace("return", "retrun"),
        valid.replace("return outColor", "return unknownIdentifier"),
        valid.replace("return outColor", "return outColor.xyz"),
        String::new(),
    ];
    for (index, text) in broken.iter().enumerate() {
        assert_ne!(text, valid, "control {index} changed nothing");
        let wrapped = wrap(language, "OCIOMain", text);
        let verdict = compile(tool, language, &work, &format!("broken{index}"), &wrapped);
        assert!(
            verdict.is_err(),
            "{tool:?} accepted a broken {language:?} shader:\n{wrapped}"
        );
    }
    // The wrapper must call the function the description names.
    let renamed = wrap(language, "otherName", valid);
    assert!(
        compile(tool, language, &work, "renamed", &renamed).is_err(),
        "{tool:?} accepted a call of an undefined function:\n{renamed}"
    );
}

/// The rule itself on each compiler, on a shader of `language`'s valid text with one thing added:
/// each form of a cause passes, and a real bug doesn't, alone, before such a form or behind it.
pub fn check_cause_controls(
    check: &str,
    corpus: Corpus,
    tool: Tool,
    language: GpuLanguage,
    valid: &str,
) {
    let dir = corpus_dir()
        .unwrap_or_else(|| panic!("{CORPUS_ENV} is not set: run `cargo xtask compile-shaders`"));
    let work = dir.join("work").join(check);
    let verdict = |label: &str, function_name: &str, text: &str| {
        let entry = Entry {
            digest: sha256_hex(text.as_bytes()),
            function_name: function_name.to_string(),
            text: format!("{DECLARATION}\n\n{text}"),
            sources: Vec::new(),
        };
        let wrapped = wrap(language, function_name, &entry.text);
        let name = format!("cause-{label}");
        let Err(output) = compile(tool, language, &work, &name, &wrapped) else {
            panic!("{tool:?} accepted {label}:\n{wrapped}");
        };
        let repaired = format!("{name}-repaired");
        explain(corpus, language, tool, &entry, &output, |wrapped| {
            compile(tool, language, &work, &repaired, wrapped)
        })
    };
    let with =
        |lines: &str| valid.replace("return outColor", &format!("{lines}\n  return outColor"));
    const BUG: &str = "outColor.r = unknownIdentifier;";
    const TYPE_BUG: &str = "outColor = outColor.xyz;";
    let mut cases: Vec<(String, &str, String, Option<Cause>)> = vec![
        ("bug".to_string(), "OCIOMain", with(BUG), None),
        ("type-bug".to_string(), "OCIOMain", with(TYPE_BUG), None),
    ];
    let mut forms: Vec<(&str, Cause)> = Vec::new();
    if corpus == Corpus::Family {
        forms.push(("outColor.r = nan;", Cause::NonFinite));
        forms.push(("outColor.r = -inf;", Cause::NonFinite));
        forms.push(("outColor.r = 1e+30.;", Cause::LargeWholeNumber));
        forms.push(("outColor.r = 1.5e+30. + 2.;", Cause::LargeWholeNumber));
    }
    if language == GpuLanguage::GlslEs10 {
        forms.push(("outColor.r = 1.5f;", Cause::EsFloatSuffix));
        forms.push((
            "const float t[2] = float[2](1., 2.);",
            Cause::EsArrayConstructor,
        ));
    }
    for (index, (form, cause)) in forms.into_iter().enumerate() {
        let mut case = |label: &str, lines: String, expected| {
            let label = format!("{label}-{index}");
            cases.push((label, "OCIOMain", with(&lines), expected));
        };
        case("form", form.to_string(), Some(cause));
        case("bug-after", format!("{form}\n  {BUG}"), None);
        case("bug-before", format!("{BUG}\n  {form}"), None);
        case("type-after", format!("{form}\n  {TYPE_BUG}"), None);
        case("type-before", format!("{TYPE_BUG}\n  {form}"), None);
    }
    let renamed = |name: &str, text: &str| text.replace("OCIOMain", name);
    if corpus == Corpus::Family {
        let name = Some(Cause::Name);
        cases.push(("main".to_string(), "main", renamed("main", valid), name));
        let bug = renamed("main", &with(BUG));
        cases.push(("main-bug".to_string(), "main", bug, None));
        // The newer dxc of `windows-2025` takes it.
        if tool != Tool::Dxc {
            let e_acute = "f_\u{e9}";
            let text = renamed(e_acute, valid);
            cases.push(("non-ascii".to_string(), e_acute, text, name));
        }
        cases.push(("empty".to_string(), "", renamed("", valid), name));
    }
    if corpus == Corpus::Suite && language == GpuLanguage::GlslEs10 {
        let reserved = Some(Cause::EsReservedName);
        let text = renamed("a__b", valid);
        cases.push(("reserved".to_string(), "a__b", text, reserved));
    }
    for (label, function_name, text, expected) in &cases {
        let verdict = verdict(label, function_name, text);
        match expected {
            Some(cause) => assert_eq!(verdict, Ok(vec![*cause]), "{label}:\n{text}"),
            None => assert!(verdict.is_err(), "{label} passed: {verdict:?}\n{text}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(function_name: &str, text: &str) -> Entry {
        Entry {
            digest: "d".to_string(),
            function_name: function_name.to_string(),
            text: text.to_string(),
            sources: Vec::new(),
        }
    }

    /// fxc's error for a NaN literal on `line` of the wrapped shader.
    fn undeclared(line: usize) -> String {
        format!("\\a.hlsl({line},7-9): error X3004: undeclared identifier 'nan'\n")
    }

    /// `explain` of a family HLSL shader whose compiler's output is `output`, and the wrapped
    /// repaired text it asked to compile.
    fn explained(
        text: &str,
        output: &str,
        repaired: Result<(), String>,
    ) -> Result<Vec<Cause>, String> {
        explain(
            Corpus::Family,
            GpuLanguage::HlslSm50,
            Tool::Fxc,
            &entry("OCIOMain", text),
            output,
            |_| repaired,
        )
    }

    #[test]
    fn the_forms_are_found_by_their_text() {
        let spans = |cause: Cause, text: &str| -> Vec<String> {
            cause
                .spans(text)
                .into_iter()
                .map(|s| text[s].to_string())
                .collect()
        };
        assert_eq!(
            spans(Cause::LargeWholeNumber, "a(1.00000002e+30., 1e+09.);"),
            ["1.00000002e+30.", "1e+09."]
        );
        assert!(spans(Cause::LargeWholeNumber, "1e+30, 1e-30., 1e+3.5, e+1.").is_empty());
        assert_eq!(
            spans(Cause::NonFinite, "f(nan, -nan(ind), inf, -inf, nan(snan));"),
            ["nan", "nan(ind)", "inf", "inf", "nan(snan)"]
        );
        assert!(
            spans(
                Cause::NonFinite,
                "ocio_nan, inf_x, x.inf, nanny, in, \u{e9}inf"
            )
            .is_empty()
        );
        assert_eq!(
            spans(Cause::EsFloatSuffix, "1.f + 0.85f - .5f * 1e-3f + 2f"),
            ["f", "f", "f", "f", "f"]
        );
        assert!(spans(Cause::EsFloatSuffix, "x1.f, a2f, 1.0, 0xf, f(1.)").is_empty());
        let table = "const float a[2] = float[2](1., 2.);";
        assert_eq!(
            spans(Cause::EsArrayConstructor, &format!("x\n{table}\ny")),
            [table]
        );
        assert_eq!(
            spans(Cause::EsArrayConstructor, "const int b[1] = int[1](3);"),
            ["const int b[1] = int[1](3);"]
        );
        assert!(
            spans(
                Cause::EsArrayConstructor,
                "const float a = 1.;\nfloat b[2] = c[2];\nconst vec3 d = vec3(1.);"
            )
            .is_empty()
        );
        assert_eq!(spans(Cause::Name, "vec4 \u{3c0} = a;"), ["\u{3c0}"]);
    }

    #[test]
    fn which_names_are_causes() {
        for name in ["", "main", "f_\u{e9}", "1a", "a b"] {
            assert!(Cause::Name.holds(name, "text"), "{name:?}");
        }
        for name in ["OCIOMain", "_a1", "mainly"] {
            assert!(!Cause::Name.holds(name, "text"), "{name:?}");
        }
        assert!(Cause::EsReservedName.holds("a__b", ""));
        assert!(!Cause::EsReservedName.holds("a_b", ""));
    }

    #[test]
    fn the_repairs_leave_no_form() {
        let text = format!(
            "{DECLARATION}\n\nvec4 f__\u{e9}(vec4 inPixel)\n{{\n  const float t[2] = \
             float[2](1e+30., nan);\n  x = -inf + 1e+09. * 0.5f + float(1);\n}}\n"
        );
        let (name, repaired) = repaired(&Cause::ALL, "f__\u{e9}", &text);
        assert_eq!(name, REPAIRED_NAME);
        assert_eq!(
            repaired,
            format!(
                "{DECLARATION}\n\nvec4 {REPAIRED_NAME}(vec4 inPixel)\n{{\n  float t[2];\n  x = \
                 -2.0 + 2.0 * 0.5 + float(1);\n}}\n"
            )
        );
        let empty = repaired_name_of("vec4 (vec4 inPixel)");
        assert_eq!(empty, format!("vec4 {REPAIRED_NAME}(vec4 inPixel)"));
    }

    fn repaired_name_of(declaration: &str) -> String {
        let text = format!("{DECLARATION}\n\n{declaration}");
        let (_, repaired) = repaired(&[Cause::Name], "", &text);
        repaired[DECLARATION.len() + 2..].to_string()
    }

    #[test]
    fn first_errors_of_each_compiler() {
        assert_eq!(
            first_error(
                Tool::Fxc,
                "\\a.hlsl(22,98): error X3000: syntax error: unexpected token ','\n\
                 \\a.hlsl(22,27-150): error X3013: 'max'\n"
            ),
            Some((22, "X3000: syntax error: unexpected token ','".to_string()))
        );
        assert_eq!(
            first_error(
                Tool::Dxc,
                "\\a.hlsl:12:39: warning: w\n\\a.hlsl:12:39: error: invalid suffix '.'\n"
            ),
            Some((12, "invalid suffix '.'".to_string()))
        );
        assert_eq!(
            first_error(
                Tool::Glslang,
                "a.frag\nERROR: /a.frag:12: 'inf' : undeclared identifier \n\
                 ERROR: 1 compilation errors.\n"
            ),
            Some((12, "'inf' : undeclared identifier ".to_string()))
        );
        assert_eq!(first_error(Tool::Fxc, "nothing here\n"), None);
    }

    /// A failure with no cause, the mutant that a new upstream or port bug makes.
    #[test]
    fn a_failure_whose_text_holds_no_cause_is_not_explained() {
        let error = explained("float4 OCIOMain();", &undeclared(1), Ok(())).unwrap_err();
        assert!(error.contains("no cause"), "{error}");
    }

    /// The right text, and another first error, or none.
    #[test]
    fn a_failure_needs_its_causes_first_error_on_the_line_of_the_form() {
        let text = "float4 OCIOMain();\nfloat a = nan;\nfloat b = 1.;";
        assert_eq!(
            explained(text, &undeclared(2), Ok(())),
            Ok(vec![Cause::NonFinite])
        );
        // Another line.
        let error = explained(text, &undeclared(3), Ok(())).unwrap_err();
        assert!(error.contains("first error"), "{error}");
        // Other words on that line.
        let output = "\\a.hlsl(2,1): error X3018: invalid subscript 'r'\n";
        let error = explained(text, output, Ok(())).unwrap_err();
        assert!(error.contains("first error"), "{error}");
        let error = explained(text, "no error line\n", Ok(())).unwrap_err();
        assert!(error.contains("no error"), "{error}");
    }

    /// The compiler stopped at the literal: a real bug behind it shows only in the repair.
    #[test]
    fn a_real_bug_behind_a_cause_is_not_explained() {
        let text = "float a = nan;";
        let error = explained(
            text,
            &undeclared(1),
            Err("\\a.hlsl(9,1): error X3004: undeclared identifier 'b'".to_string()),
        )
        .unwrap_err();
        assert!(error.contains("still fails"), "{error}");
    }

    /// A cause explains only the corpora and languages it applies to.
    #[test]
    fn a_form_outside_its_scope_is_not_explained() {
        let suite = explain(
            Corpus::Suite,
            GpuLanguage::HlslSm50,
            Tool::Fxc,
            &entry("OCIOMain", "float a = nan;"),
            &undeclared(1),
            |_| Ok(()),
        );
        assert!(suite.unwrap_err().contains("no cause"));
        let es_form_elsewhere = explain(
            Corpus::Family,
            GpuLanguage::GlslEs30,
            Tool::Glslang,
            &entry("OCIOMain", "float a = 1.f;"),
            "ERROR: /a.frag:4: 'f' : syntax\n",
            |_| Ok(()),
        );
        assert!(es_form_elsewhere.unwrap_err().contains("no cause"));
    }

    /// A cause that explains nothing where it applies is reported, and only there.
    #[test]
    fn a_cause_that_explains_nothing_is_reported() {
        let mut explained = BTreeMap::new();
        explained.insert(Cause::NonFinite, 1);
        explained.insert(Cause::Name, 1);
        assert_eq!(
            causes_unused(Corpus::Family, GpuLanguage::HlslSm50, &explained),
            [Cause::LargeWholeNumber]
        );
        explained.insert(Cause::LargeWholeNumber, 1);
        assert_eq!(
            causes_unused(Corpus::Family, GpuLanguage::HlslSm50, &explained),
            []
        );
        assert_eq!(
            causes_unused(Corpus::Family, GpuLanguage::GlslEs10, &explained),
            [Cause::EsArrayConstructor, Cause::EsFloatSuffix]
        );
        assert_eq!(
            causes_unused(Corpus::Suite, GpuLanguage::GlslEs10, &BTreeMap::new()),
            [
                Cause::EsArrayConstructor,
                Cause::EsFloatSuffix,
                Cause::EsReservedName
            ]
        );
        for language in COMPILED_LANGUAGES {
            if language != GpuLanguage::GlslEs10 {
                assert_eq!(
                    causes_unused(Corpus::Suite, language, &BTreeMap::new()),
                    [],
                    "{language:?}: no failure is accepted in upstream's tests"
                );
            }
        }
    }

    #[test]
    #[should_panic(expected = "explain no failure")]
    fn an_unused_cause_fails_its_check() {
        assert_every_cause_used("a check", &[Cause::NonFinite]);
    }

    /// The register's numbers, which the card and the entries must agree with.
    #[test]
    fn each_cause_names_its_entry() {
        let register =
            std::fs::read_to_string(crate::paths::workspace_root().join("docs/improvements.md"))
                .unwrap();
        for (cause, title) in [
            (Cause::LargeWholeNumber, "Whole numbers from 1e9 or 1e17 up"),
            (Cause::NonFinite, "A NaN or infinite parameter"),
            (Cause::EsArrayConstructor, "GLSL ES 1.0 doesn't accept"),
            (Cause::EsFloatSuffix, "GLSL ES 1.0 doesn't accept"),
            (Cause::EsReservedName, "GLSL ES 1.0 doesn't accept"),
            (Cause::Name, "names go into the shader"),
        ] {
            let heading = register
                .lines()
                .find(|line| line.starts_with(&format!("### {}.", cause.entry())))
                .unwrap_or_else(|| panic!("no entry {}", cause.entry()));
            assert!(heading.contains(title), "{cause:?}: {heading}");
        }
    }

    /// A name that can't clash with another test: tests share `std::env::temp_dir`.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ocio-shader-corpus-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn wrap_keeps_the_text_and_calls_the_named_function() {
        for language in COMPILED_LANGUAGES {
            let wrapped = wrap(language, "myFunction", "TEXT");
            assert!(wrapped.contains("\nTEXT\n") || wrapped.starts_with("TEXT\n"));
            assert!(wrapped.contains("myFunction("), "{language:?}");
            assert!(!wrapped.contains("OCIOMain"), "{language:?}");
        }
    }

    #[test]
    fn wrap_starts_with_upstreams_version_line() {
        // glsl.cpp:454-481 @ v2.5.2.
        for (language, version) in [
            (GpuLanguage::Glsl12, "#version 120\n"),
            (GpuLanguage::Glsl13, "#version 130\n"),
            (GpuLanguage::Glsl40, "#version 400 core\n"),
            (GpuLanguage::GlslVk46, "#version 460 core\n"),
            (GpuLanguage::GlslEs10, "#version 100\n"),
            (GpuLanguage::GlslEs30, "#version 300 es\n"),
        ] {
            assert!(wrap(language, "f", "").starts_with(version), "{language:?}");
        }
    }

    #[test]
    #[should_panic(expected = "no compile check")]
    fn languages_without_a_compiler_are_refused() {
        wrap(GpuLanguage::Msl20, "f", "");
    }

    #[test]
    fn cause_masks_what_names_the_shader() {
        assert_eq!(
            cause(
                "ERROR: /tmp/abc.frag:12: 'inf' : undeclared identifier\nERROR: 1 compilation errors."
            ),
            "#: 'inf' : undeclared identifier"
        );
        assert_eq!(
            cause(r"C:\w\a.hlsl(22,98): error X3000: syntax error: unexpected token ','"),
            "X#: syntax error: unexpected token ','"
        );
        assert_eq!(
            cause("a.hlsl:3:1: error: expected expression"),
            ": expected expression"
        );
    }

    #[test]
    fn sdk_versions_sort_by_number() {
        assert_eq!(
            sdk_version(Path::new("x/10.0.22000.0")),
            Some(vec![10, 0, 22000, 0])
        );
        assert!(sdk_version(Path::new("x/10.0.9.0")) < sdk_version(Path::new("x/10.0.10.0")));
        assert_eq!(sdk_version(Path::new("x/arm64")), None);
    }

    #[test]
    fn record_and_load_round_trip_with_sources() {
        let dir = scratch("round-trip");
        for target in FAMILY_TARGETS {
            record(
                &dir,
                Corpus::Family,
                target,
                GpuLanguage::Glsl12,
                "f",
                "same",
            );
        }
        record(
            &dir,
            Corpus::Family,
            "matrix_op_gpu_oracle",
            GpuLanguage::Glsl12,
            "f",
            "other",
        );
        record(
            &dir,
            Corpus::Family,
            "matrix_op_gpu_oracle",
            GpuLanguage::Glsl12,
            "g",
            "same",
        );
        let entries = load(&dir, Corpus::Family, GpuLanguage::Glsl12);
        assert_eq!(entries.len(), 3);
        let same = entries
            .iter()
            .find(|e| e.text == "same" && e.function_name == "f")
            .unwrap();
        assert_eq!(same.sources.len(), FAMILY_TARGETS.len());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[should_panic(expected = "recorded no")]
    fn a_family_target_without_shaders_fails_the_load() {
        let dir = scratch("missing-target");
        for target in &FAMILY_TARGETS[1..] {
            record(&dir, Corpus::Family, target, GpuLanguage::Glsl12, "f", "t");
        }
        load(&dir, Corpus::Family, GpuLanguage::Glsl12);
    }

    #[test]
    #[should_panic(expected = "suite tests in the corpus")]
    fn a_suite_with_a_test_missing_fails_the_load() {
        let dir = scratch("suite-count");
        for index in 0..SUITE_TESTS - 1 {
            record(
                &dir,
                Corpus::Suite,
                &format!("Group.test{index}"),
                GpuLanguage::Glsl12,
                "f",
                "t",
            );
        }
        load(&dir, Corpus::Suite, GpuLanguage::Glsl12);
    }

    #[test]
    #[should_panic(expected = "Group.test0")]
    fn a_suite_test_with_two_shaders_fails_the_load() {
        let dir = scratch("suite-two");
        for index in 0..SUITE_TESTS {
            record(
                &dir,
                Corpus::Suite,
                &format!("Group.test{index}"),
                GpuLanguage::Glsl12,
                "f",
                "t",
            );
        }
        record(
            &dir,
            Corpus::Suite,
            "Group.test0",
            GpuLanguage::Glsl12,
            "f",
            "u",
        );
        load(&dir, Corpus::Suite, GpuLanguage::Glsl12);
    }

    #[test]
    fn test_target_strips_the_hash() {
        // This binary is `ocio_testkit-<hash>`.
        assert_eq!(test_target(), "ocio_testkit");
    }
}
