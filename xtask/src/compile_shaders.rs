// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `cargo xtask compile-shaders`: compile checks of generated shaders (P7-5,
//! `docs/cards/phase7.md` WP 7.4).
//!
//! Captures the wheel's shaders with the tests that assert the port's text equal to them, then
//! runs `compile_glsl.rs` (Linux) or `compile_hlsl.rs` (Windows) over them. Every check is
//! strict, and a check named on the command line that doesn't run is an error.

use std::path::{Path, PathBuf};
use std::process::Command;

use ocio_testkit::shader_compile::CORPUS_ENV;

pub(crate) const USAGE: &str = "\
cargo xtask compile-shaders <check>...

Compiles the wheel's shaders, which the capture asserts equal to the port's, with the
platform's compilers: glslang in Rocky Linux 9, the Windows SDK's fxc and dxc on Windows. Run
it on Rocky through `scripts/rocky9.sh`.

Each <check> is a prefix of the test names in crates/ocio-gpu-exec/tests/compile_glsl.rs or
compile_hlsl.rs: suite_* (upstream's 264 GPU tests) or family_* (crates/ocio-gpu's *_oracle
tests). The compilers' controls (control_*) always run. Only the corpora the checks name are
captured. Writes target/shader-corpus/report/<check>.json, and fails if a shader fails for
anything but a registered cause of ocio_testkit::shader_compile (P7-9), or if such a cause no
longer explains any shader of a check it applies to.
";

/// Upstream's GPU tests, as the suite's targets hold them.
const SUITE_TARGETS: [&str; 14] = [
    "cdl_op_test",
    "config_test",
    "ec_op_test",
    "fixed_function_op_test",
    "gamma_op_test",
    "grading_hue_curve_op_test",
    "grading_primary_op_test",
    "grading_rgb_curve_op_test",
    "grading_tone_op_test",
    "log_op_test",
    "lut1d_op_test",
    "lut3d_op_test",
    "matrix_op_test",
    "range_op_test",
];

const FEATURE: &str = "ocio-gpu-exec/shader-corpus";

/// The `cargo` argument lists of a run: the captures, then the compile target.
fn plan(checks: &[&str], compile_target: &str) -> Vec<Vec<String>> {
    let strings = |args: &[&str]| args.iter().map(ToString::to_string).collect::<Vec<_>>();
    let mut steps = Vec::new();
    if checks.iter().any(|check| check.starts_with("family")) {
        steps.push(strings(&[
            "test",
            "-p",
            "opencolorio-rs-gpu",
            "--tests",
            "--no-fail-fast",
        ]));
    }
    if checks.iter().any(|check| check.starts_with("suite")) {
        let mut step = strings(&["test", "-p", "ocio-gpu-exec", "--features", FEATURE]);
        step.push("--no-fail-fast".into());
        for target in SUITE_TARGETS {
            step.push("--test".into());
            step.push(target.into());
        }
        steps.push(step);
    }
    let mut compile = strings(&["test", "-p", "ocio-gpu-exec", "--features", FEATURE]);
    compile.extend(strings(&[
        "--no-fail-fast",
        "--test",
        compile_target,
        "--",
        "control_",
    ]));
    compile.extend(checks.iter().map(ToString::to_string));
    steps.push(compile);
    steps
}

/// Fails unless `check` is a corpus check's prefix.
fn validate(check: &str) -> Result<(), String> {
    if check.starts_with("suite") || check.starts_with("family") {
        Ok(())
    } else {
        Err(format!("unknown check `{check}`\n\n{USAGE}"))
    }
}

/// The reports `checks` name, each failing check's groups printed. An error when one of the
/// named checks left no report: it didn't run.
fn summarize(report: &Path, checks: &[&str]) -> Result<u64, String> {
    let mut names: Vec<_> = std::fs::read_dir(report)
        .map_err(|e| format!("{}: {e}", report.display()))?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter_map(|name| name.strip_suffix(".json").map(String::from))
        .collect();
    names.sort();
    for check in checks {
        if !names.iter().any(|name| name.starts_with(check)) {
            return Err(format!(
                "the check `{check}` left no report: it did not run"
            ));
        }
    }
    let mut failed = 0;
    for name in &names {
        let path = report.join(format!("{name}.json"));
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{name}: {e}"))?;
        println!(
            "compile-shaders: {name}: {} distinct shaders, {} passed, {} by a known cause, {} failed",
            value["compiled"], value["passed"], value["explained"], value["failed"]
        );
        for cause in value["causes"].as_array().into_iter().flatten() {
            println!(
                "    {:6} {} ({})",
                cause["count"], cause["cause"], cause["entry"]
            );
        }
        for group in value["groups"].as_array().into_iter().flatten() {
            println!("    {:6} no cause: {}", group["count"], group["cause"]);
        }
        failed += value["failed"].as_u64().unwrap_or(0);
    }
    Ok(failed)
}

/// `cargo xtask compile-shaders`.
pub(crate) fn run(args: &[&str]) -> Result<(), String> {
    if args.is_empty() {
        return Err(USAGE.to_string());
    }
    for check in args {
        validate(check)?;
    }
    let compile_target = if cfg!(windows) {
        "compile_hlsl"
    } else {
        "compile_glsl"
    };
    let root = crate::root();
    let corpus: PathBuf =
        crate::plain_path(ocio_testkit::paths::target_dir()).join("shader-corpus");
    if corpus.exists() {
        std::fs::remove_dir_all(&corpus).map_err(|e| format!("{}: {e}", corpus.display()))?;
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut errors = Vec::new();
    for step in plan(args, compile_target) {
        println!("compile-shaders: cargo {}", step.join(" "));
        let status = Command::new(&cargo)
            .args(&step)
            .current_dir(&root)
            .env(CORPUS_ENV, &corpus)
            .status()
            .map_err(|e| format!("could not run cargo: {e}"))?;
        if !status.success() {
            errors.push(format!("cargo {}: {status}", step.join(" ")));
        }
    }
    let failed = summarize(&corpus.join("report"), args)?;
    if errors.is_empty() && failed == 0 {
        Ok(())
    } else {
        Err(format!(
            "compile-shaders: {failed} shaders failed to compile with no cause; failed steps:\n  {}",
            errors.join("\n  ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cargo_args<'a>(steps: &'a [Vec<String>], wanted: &str) -> Vec<&'a Vec<String>> {
        steps
            .iter()
            .filter(|step| step.iter().any(|arg| arg == wanted))
            .collect()
    }

    #[test]
    fn suite_targets_are_exactly_the_files_that_opt_in() {
        let tests = crate::root().join("crates/ocio-gpu-exec/tests");
        let mut opted_in = Vec::new();
        for entry in std::fs::read_dir(&tests).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                if text.contains("#![cfg(any(target_os = \"linux\", feature = \"shader-corpus\"))]")
                {
                    let stem = path.file_stem().unwrap().to_str().unwrap();
                    opted_in.push(stem.to_string());
                }
            }
        }
        opted_in.sort();
        assert_eq!(opted_in, SUITE_TARGETS);
    }

    #[test]
    fn a_plan_captures_what_its_checks_read_before_compiling() {
        let steps = plan(&["suite_glsl_1_2"], "compile_glsl");
        assert_eq!(steps.len(), 2);
        assert!(steps[0].contains(&"lut3d_op_test".to_string()));
        assert!(steps[0].iter().any(|arg| arg == FEATURE));
        assert!(cargo_args(&steps, "opencolorio-rs-gpu").is_empty());
        let compile = steps.last().unwrap();
        assert_eq!(
            compile[compile.len() - 2..],
            ["control_".to_string(), "suite_glsl_1_2".to_string()]
        );

        let steps = plan(&["family_"], "compile_hlsl");
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0][..3], ["test", "-p", "opencolorio-rs-gpu"]);

        assert_eq!(plan(&["family_", "suite_"], "compile_hlsl").len(), 3);
    }

    #[test]
    fn a_named_check_without_a_report_is_an_error() {
        let dir = std::env::temp_dir().join(format!("xtask-shaders-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("suite_a.json"),
            r#"{"compiled": 3, "passed": 2, "failed": 1, "groups": []}"#,
        )
        .unwrap();
        assert_eq!(summarize(&dir, &["suite_"]), Ok(1));
        let error = summarize(&dir, &["suite_", "family_"]).unwrap_err();
        assert!(error.contains("family_"), "{error}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The CI steps that make these checks real. Each named check is a test of its target.
    #[test]
    fn ci_runs_every_check() {
        let ci = std::fs::read_to_string(crate::root().join(".github/workflows/ci.yml")).unwrap();
        let tests = crate::root().join("crates/ocio-gpu-exec/tests");
        let glsl = std::fs::read_to_string(tests.join("compile_glsl.rs")).unwrap();
        let hlsl = std::fs::read_to_string(tests.join("compile_hlsl.rs")).unwrap();
        let flat = ci.replace(
            "
          ",
            " ",
        );
        // No step may fail: P7-9 is decided, and its causes are the checks' own (the checks of one
        // CI command, its target's source).
        assert!(!ci.contains("continue-on-error"));
        for (checks, source) in [
            ("suite_hlsl_fxc suite_hlsl_dxc", &hlsl),
            ("family_", &hlsl),
            (
                "suite_glsl_1_2 suite_glsl_1_3 suite_glsl_4_0 suite_glsl_es_3_0 suite_glsl_vk_4_6",
                &glsl,
            ),
            ("suite_glsl_es_1_0 family_", &glsl),
        ] {
            let command = format!(
                "cargo xtask compile-shaders {checks}
"
            );
            assert!(flat.contains(&command), "ci.yml doesn't run `{command}`");
            for check in checks.split(' ') {
                assert!(source.contains(&format!("fn {check}")), "{check}");
            }
        }
        // Every check of both targets is in one of the commands above.
        for source in [&glsl, &hlsl] {
            for line in source.lines() {
                let name = line.strip_prefix("fn ").and_then(|l| l.split('(').next());
                if let Some(name) = name.filter(|n| n.starts_with("suite_")) {
                    assert!(flat.contains(name), "{name} is not in ci.yml");
                }
            }
        }
    }

    #[test]
    fn only_corpus_checks_are_accepted() {
        assert!(validate("suite_hlsl_fxc").is_ok());
        assert!(validate("family_").is_ok());
        assert!(validate("control_").is_err());
        assert!(validate("--anything").is_err());
    }
}
