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
//!    platform's compiler ([`check`]). The compiler's verdict is the expected value: there is
//!    no list of expected failures here, and a shader that fails fails the check.

use std::collections::BTreeMap;
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

/// Compiles every captured shader of `corpus` in `language` with `tool`, writes the verdict to
/// `<dir>/report/<check>.json`, and panics if any shader failed, with the failures grouped by
/// the compiler's first error.
pub fn check(check: &str, corpus: Corpus, language: GpuLanguage, tool: Tool) {
    let dir = corpus_dir()
        .unwrap_or_else(|| panic!("{CORPUS_ENV} is not set: run `cargo xtask compile-shaders`"));
    let entries = load(&dir, corpus, language);
    let work = dir.join("work").join(check);
    let failures = compile_entries(&entries, tool, language, &work);
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, output) in &failures {
        groups.entry(cause(output)).or_default().push(*index);
    }
    let group_json: Vec<Value> = groups
        .iter()
        .map(|(cause, members)| {
            let first = &entries[members[0]];
            let first_output = failures.iter().find(|f| f.0 == members[0]).unwrap();
            json!({
                "cause": cause,
                "count": members.len(),
                "example": first.digest,
                "example_sources": first.sources,
                "example_output": first_output.1,
            })
        })
        .collect();
    let failed: Vec<Value> = failures
        .iter()
        .map(|(index, _)| {
            json!({"digest": entries[*index].digest, "sources": entries[*index].sources})
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
            "failed": failures.len(),
            "groups": group_json,
            "failures": failed,
        }))
        .unwrap(),
    )
    .unwrap();
    let mut summary = String::new();
    for (cause, members) in &groups {
        summary.push_str(&format!("\n  {:6} {cause}", members.len()));
    }
    assert!(
        failures.is_empty(),
        "{check}: {} of {} distinct shaders failed in {tool:?}:{summary}",
        failures.len(),
        entries.len()
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

#[cfg(test)]
mod tests {
    use super::*;

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
