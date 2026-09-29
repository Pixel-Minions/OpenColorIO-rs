// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `upstream-map.toml`: every upstream file the port accounts for, and where it goes.

use std::collections::BTreeMap;

use ocio_testkit::paths;
use serde::{Deserialize, Serialize};

pub(crate) const UPSTREAM_TAG: &str = "v2.5.2";
pub(crate) const UPSTREAM_COMMIT: &str = "c52966a6677723d5bd2dbef0ccec3fed9cbc3790";

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct UpstreamMap {
    pub(crate) upstream: UpstreamPin,
    #[serde(default, rename = "file")]
    pub(crate) files: Vec<MapEntry>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct UpstreamPin {
    pub(crate) tag: String,
    pub(crate) commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MapEntry {
    /// Path in the upstream repository.
    pub(crate) path: String,
    /// api | src | binding | app | test-cpu | test-gpu | test-python | test-support | build
    pub(crate) kind: String,
    /// Rust files (or "run unmodified") that port this file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) target: Vec<String>,
    /// todo | partial | done | n/a
    pub(crate) status: String,
    /// Why the file is n/a, or notes on a partial port.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) note: String,
}

const STATUSES: &[&str] = &["todo", "partial", "done", "n/a"];

fn map_path() -> std::path::PathBuf {
    paths::workspace_root().join("upstream-map.toml")
}

pub(crate) fn load_map() -> Result<UpstreamMap, String> {
    let path = map_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(UpstreamMap::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// `CamelCase` file stem to `snake_case`, keeping `1D`/`3D` and SIMD suffixes readable:
/// `Lut1DOpCPU_AVX2` -> `lut1d_op_cpu_avx2`, `OCIOYaml` -> `ocio_yaml`.
pub(crate) fn snake(stem: &str) -> String {
    if stem == "FileFormat3DL" {
        return "file_format_3dl".to_string();
    }
    let chars: Vec<char> = stem.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' || c == '-' || c == '.' {
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            continue;
        }
        if c.is_ascii_uppercase() && i > 0 {
            let prev = chars[i - 1];
            let next = chars.get(i + 1).copied();
            let next_lower = next.is_some_and(|n| n.is_ascii_lowercase());
            let boundary = prev.is_ascii_lowercase()
                || (prev.is_ascii_uppercase() && next_lower)
                || (prev.is_ascii_digit() && (c != 'D' || next_lower));
            if boundary && !out.ends_with('_') {
                out.push('_');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

fn split(path: &str) -> (Vec<&str>, &str, &str) {
    let mut parts: Vec<&str> = path.split('/').collect();
    let file = parts.pop().unwrap_or_default();
    let (stem, ext) = file.rsplit_once('.').unwrap_or((file, ""));
    (parts, stem, ext)
}

fn rs_path(dirs: &[&str], stem: &str) -> String {
    let mut p: Vec<String> = dirs.iter().map(|d| snake(d)).collect();
    p.push(format!("{}.rs", snake(stem)));
    p.join("/")
}

/// Files of `src/OpenColorIO/` that belong to the op engine rather than the public API crate.
const OPS_CRATE_FILES: &[&str] = &[
    "AVX",
    "AVX2",
    "AVX512",
    "SSE",
    "SSE2",
    "BitDepthUtils",
    "CPUInfo",
    "CPUInfoConfig",
    "CPUProcessor",
    "ImagePacking",
    "MathUtils",
    "Op",
    "OpBuilders",
    "OpOptimizers",
    "ScanlineHelper",
    "HashUtils",
    "Platform",
    "Logging",
    "Mutex",
    "PrivateTypes",
];
const GPU_CRATE_FILES: &[&str] = &[
    "GPUProcessor",
    "GpuShader",
    "GpuShaderClassWrapper",
    "GpuShaderDesc",
    "GpuShaderUtils",
];

/// Default kind, target and status for a new upstream file.
fn classify(path: &str) -> Option<MapEntry> {
    let (dirs, stem, ext) = split(path);
    let file = path.rsplit('/').next().unwrap_or(path);
    let entry = |kind: &str, target: Vec<String>, status: &str, note: &str| MapEntry {
        path: path.to_string(),
        kind: kind.to_string(),
        target,
        status: status.to_string(),
        note: note.to_string(),
    };
    if file == "CMakeLists.txt" || (ext == "in" && !path.contains("/builtinconfigs/")) {
        return Some(entry("build", vec![], "n/a", "build system"));
    }
    let code = matches!(ext, "cpp" | "h" | "in");
    let top = dirs.first().copied().unwrap_or("");
    match (top, dirs.get(1).copied().unwrap_or("")) {
        ("include", _) if ext == "h" => Some(entry(
            "api",
            vec!["crates/ocio/src/lib.rs".into()],
            "todo",
            "",
        )),
        ("src", "OpenColorIO") if code || ext == "ocio" => {
            let rest = &dirs[2..];
            let target = if ext == "ocio" {
                format!(
                    "crates/ocio/src/{}/{file}",
                    rest.iter().map(|d| snake(d)).collect::<Vec<_>>().join("/")
                )
            } else if rest.first() == Some(&"ops") && stem.contains("GPU") {
                format!("crates/ocio-gpu/src/{}", rs_path(rest, stem))
            } else if rest.first() == Some(&"ops")
                || (rest.is_empty() && OPS_CRATE_FILES.contains(&stem))
            {
                format!("crates/ocio-ops/src/{}", rs_path(rest, stem))
            } else if rest.first() == Some(&"fileformats") {
                format!("crates/ocio-formats/src/{}", rs_path(rest, stem))
            } else if rest.is_empty() && GPU_CRATE_FILES.contains(&stem) {
                format!("crates/ocio-gpu/src/{}", rs_path(rest, stem))
            } else {
                format!(
                    "crates/ocio/src/{}",
                    rs_path(rest, stem.trim_end_matches(".cpp"))
                )
            };
            Some(entry("src", vec![target], "todo", ""))
        }
        ("src", "utils") if code => Some(entry(
            "src",
            vec![format!("crates/ocio-ops/src/utils/{}", rs_path(&[], stem))],
            "todo",
            "",
        )),
        ("src", "bindings") => match dirs.get(2).copied() {
            Some("python") if code || ext == "py" => Some(entry(
                "binding",
                vec![format!("crates/ocio-py/src/{}", rs_path(&dirs[3..], stem))],
                "todo",
                "",
            )),
            Some("python") => None,
            _ => Some(entry(
                "binding",
                vec![],
                "n/a",
                "out of scope: only the Python binding is ported (PLAN.md §4)",
            )),
        },
        ("src", "apps") => {
            let app = dirs.get(2).copied().unwrap_or("");
            if matches!(
                app,
                "ociocheck" | "ociochecklut" | "ociowrite" | "ociobakelut"
            ) && code
            {
                Some(entry(
                    "app",
                    vec![format!("crates/ocio-tools/src/bin/{app}.rs")],
                    "todo",
                    "",
                ))
            } else if code || ext == "py" {
                Some(entry(
                    "app",
                    vec![],
                    "n/a",
                    "app not in scope (PLAN.md §4; dev tools are ociocheck, ociochecklut, ociowrite, ociobakelut)",
                ))
            } else {
                None
            }
        }
        ("src", "apputils") if code => Some(entry(
            "app",
            vec![format!("crates/ocio-tools/src/{}", rs_path(&[], stem))],
            "todo",
            "",
        )),
        ("src", "libutils") if code || ext == "mm" => Some(entry(
            "app",
            vec![],
            "n/a",
            "OpenGL/Metal/image-IO helpers for upstream's apps",
        )),
        ("tests", "cpu" | "utils") if code => {
            let rest = if dirs[1] == "cpu" {
                &dirs[2..]
            } else {
                &dirs[1..]
            };
            let subject = stem.trim_end_matches("_tests");
            if subject == stem {
                // Test support code (UnitTestUtils, UnitTestMain, ...).
                return Some(entry(
                    "test-support",
                    vec![format!(
                        "crates/ocio-testkit/src/upstream/{}",
                        rs_path(&[], stem)
                    )],
                    "todo",
                    "",
                ));
            }
            let src = format!(
                "src/OpenColorIO/{}{subject}.cpp",
                rest.iter().map(|d| format!("{d}/")).collect::<String>()
            );
            let target = classify(&src)
                .and_then(|e| e.target.first().cloned())
                .map(|t| format!("{}_tests.rs", t.trim_end_matches(".rs")))
                .unwrap_or_default();
            Some(entry("test-cpu", vec![target], "todo", ""))
        }
        ("tests", "gpu") if code => Some(entry(
            "test-gpu",
            vec![format!("crates/ocio-gpu/tests/{}", rs_path(&[], stem))],
            "todo",
            "",
        )),
        ("tests", "python") if ext == "py" || ext == "txt" => Some(entry(
            "test-python",
            vec!["run unmodified against ocio-py".into()],
            "todo",
            "",
        )),
        ("tests", "testutils") if code => Some(entry(
            "test-support",
            vec![format!(
                "crates/ocio-testkit/src/upstream/{}",
                rs_path(&[], stem)
            )],
            "todo",
            "",
        )),
        ("tests", "java" | "osl" | "cmake-consumer") => Some(entry(
            "test-support",
            vec![],
            "n/a",
            "tests of out-of-scope components",
        )),
        _ => None,
    }
}

const ROOTS: &[&str] = &["include", "src", "tests"];

pub(crate) fn update_map() -> Result<(), String> {
    let upstream = paths::upstream_dir();
    if !upstream.join("src").is_dir() {
        return Err(format!(
            "{} is missing; run `git submodule update --init`",
            upstream.display()
        ));
    }
    let mut map = load_map()?;
    let mut existing: BTreeMap<String, MapEntry> =
        map.files.drain(..).map(|e| (e.path.clone(), e)).collect();

    let mut files = Vec::new();
    for root in ROOTS {
        for rel in crate::walk(&upstream.join(root)) {
            let path = format!("{root}/{rel}");
            if path.starts_with("tests/data/") {
                continue;
            }
            if let Some(entry) = classify(&path) {
                files.push(existing.remove(&path).unwrap_or(entry));
            }
        }
    }
    for gone in existing.keys() {
        println!("removed (no longer upstream): {gone}");
    }
    let added = files.len();
    map.upstream = UpstreamPin {
        tag: UPSTREAM_TAG.into(),
        commit: UPSTREAM_COMMIT.into(),
    };
    map.files = files;
    let text = toml::to_string_pretty(&map).map_err(|e| e.to_string())?;
    let header = "\
# Every upstream OpenColorIO file the port accounts for (PLAN.md §7).
# Created by `cargo xtask upstream-map update`, which keeps existing entries; agents update
# `target`, `status` and `note` as they port. status: todo | partial | done | n/a.
# An n/a entry needs a note; n/a for anything other than build files, out-of-scope
# components or apps needs the owner's approval in review.

";
    std::fs::write(map_path(), format!("{header}{text}")).map_err(|e| e.to_string())?;
    println!("upstream-map.toml: {added} files");
    Ok(())
}

pub(crate) fn validate(map: &UpstreamMap) -> Vec<String> {
    let mut problems = Vec::new();
    if map.upstream.tag != UPSTREAM_TAG || map.upstream.commit != UPSTREAM_COMMIT {
        problems.push(format!(
            "upstream-map.toml pins {} {}, expected {UPSTREAM_TAG} {UPSTREAM_COMMIT}",
            map.upstream.tag, map.upstream.commit
        ));
    }
    for e in &map.files {
        if !STATUSES.contains(&e.status.as_str()) {
            problems.push(format!("{}: unknown status `{}`", e.path, e.status));
        }
        if e.status == "n/a" && e.note.is_empty() {
            problems.push(format!("{}: n/a needs a note", e.path));
        }
        if e.status != "n/a" && e.target.is_empty() {
            problems.push(format!("{}: needs a target", e.path));
        }
    }
    problems
}

pub(crate) fn status() -> Result<(), String> {
    let map = load_map()?;
    let problems = validate(&map);
    let mut table: BTreeMap<&str, BTreeMap<&str, usize>> = BTreeMap::new();
    for e in &map.files {
        *table
            .entry(e.kind.as_str())
            .or_default()
            .entry(e.status.as_str())
            .or_default() += 1;
    }
    println!(
        "upstream {} ({})",
        map.upstream.tag,
        &map.upstream.commit[..map.upstream.commit.len().min(7)]
    );
    println!(
        "{:<14} {:>6} {:>8} {:>6} {:>6} {:>6}",
        "kind", "todo", "partial", "done", "n/a", "total"
    );
    for (kind, counts) in &table {
        let get = |s: &str| counts.get(s).copied().unwrap_or(0);
        let total: usize = counts.values().sum();
        println!(
            "{kind:<14} {:>6} {:>8} {:>6} {:>6} {total:>6}",
            get("todo"),
            get("partial"),
            get("done"),
            get("n/a")
        );
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::snake;

    #[test]
    fn snake_names() {
        assert_eq!(snake("Lut1DOpCPU_AVX2"), "lut1d_op_cpu_avx2");
        assert_eq!(snake("CPUInfo"), "cpu_info");
        assert_eq!(snake("OCIOYaml"), "ocio_yaml");
        assert_eq!(snake("OCIOZArchive"), "ocioz_archive");
        assert_eq!(snake("GradingRGBCurveOpCPU"), "grading_rgb_curve_op_cpu");
        assert_eq!(snake("CTFReaderHelper"), "ctf_reader_helper");
        assert_eq!(snake("ACES2"), "aces2");
        assert_eq!(snake("FileFormat3DL"), "file_format_3dl");
        assert_eq!(snake("PyGpuShaderDesc"), "py_gpu_shader_desc");
        assert_eq!(snake("Lut3DOpCPU_AVX512"), "lut3d_op_cpu_avx512");
    }
}
