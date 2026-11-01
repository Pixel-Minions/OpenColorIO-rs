// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Qualify the original driver, without modifying it or its unittest runner.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Stdio};

/// Run `OpenColorIOTestSuite.py`'s standalone and CLI paths against both packages.
/// The driver itself remains upstream's (tests/python/OpenColorIOTestSuite.py @ v2.5.2).
pub(crate) fn run(
    layout: &crate::py::Layout,
    tests: &Path,
    expected: &BTreeMap<String, Vec<(String, String)>>,
) -> Result<(), String> {
    let scratch = layout
        .py
        .with_file_name("py-driver-tmp")
        .join(std::process::id().to_string());
    let build = scratch.join("build");
    // The CLI accepts a CMake build directory. These directories only provide its
    // search-path shape: both interpreters still import their own installed package.
    // ConstantsTest uses no data files. The no-argument full suite uses upstream's data.
    for dir in [
        build.join("src/OpenColorIO"),
        build.join("src/bindings/python"),
        build.join("testdata"),
    ] {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let full_ids: Vec<String> = expected
        .values()
        .flatten()
        .map(|(id, _)| id.clone())
        .collect();
    let selected_ids: Vec<String> = full_ids
        .iter()
        .filter(|id| id.starts_with("ConstantsTest."))
        .cloned()
        .collect();
    if selected_ids.is_empty() || full_ids.is_empty() {
        return Err("the original driver's wheel inventory is empty".into());
    }
    let build_arg = build.to_string_lossy().into_owned();
    let mut reports = Vec::new();
    let oracle = ocio_testkit::Oracle::get();
    let port_python = layout.python();
    for (name, args, ids) in [
        ("standalone", Vec::<&str>::new(), Some(&full_ids)),
        (
            "run-only",
            vec![build_arg.as_str(), "--run_only", "ConstantsTest"],
            Some(&selected_ids),
        ),
        (
            "unknown-module",
            vec![build_arg.as_str(), "--run_only", "NoSuchOCIOTestModule"],
            None,
        ),
    ] {
        let mut pair = Vec::new();
        let mut reference_ids = None;
        for (package, python) in [("wheel", oracle.python()), ("port", port_python.as_path())] {
            let cwd = scratch.join(name).join(package).join("cwd");
            let tmp = scratch.join(name).join(package).join("tmp");
            for dir in [&cwd, &tmp] {
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            let mut cmd = Command::new(python);
            // Script-directory imports are necessary for the original CLI. env_clear
            // prevents PYTHONPATH/OCIO from selecting a different package or config.
            cmd.args(["-B", "-X", "utf8"])
                .arg(tests.join("python/OpenColorIOTestSuite.py"))
                .args(&args)
                .current_dir(&cwd)
                .env_clear()
                .env("TEMP", &tmp)
                .env("TMP", &tmp)
                .env("TMPDIR", &tmp)
                .stdin(Stdio::null());
            if cfg!(windows)
                && let Some(root) = std::env::var_os("SYSTEMROOT")
            {
                cmd.env("SYSTEMROOT", root);
            }
            let out = cmd
                .output()
                .map_err(|e| format!("the {package}'s original driver ({name}): {e}"))?;
            pair.push(serde_json::json!({
                "package": package,
                "returncode": out.status.code(),
                "stdout": String::from_utf8_lossy(&out.stdout),
                "stderr": String::from_utf8_lossy(&out.stderr),
            }));
            reports.push(serde_json::json!({ "mode": name, "runs": pair }));
            // Keep even a failing run's actual output before checking its result.
            let report_path = layout.py.with_file_name("py-driver.json");
            std::fs::write(
                &report_path,
                serde_json::to_vec_pretty(&reports).map_err(|e| e.to_string())?,
            )
            .map_err(|e| format!("{}: {e}", report_path.display()))?;
            reports.pop();
            if let Some(ids) = ids {
                if !out.status.success() {
                    return Err(format!(
                        "the {package}'s original driver ({name}) failed: {}; see {}\n{}{}",
                        out.status,
                        report_path.display(),
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr),
                    ));
                }
                let observed =
                    successful_ids(&String::from_utf8_lossy(&out.stderr), ids).map_err(|e| {
                        format!(
                            "the {package}'s original driver ({name}): {e}\n{}",
                            String::from_utf8_lossy(&out.stderr)
                        )
                    })?;
                if let Some(reference_ids) = &reference_ids {
                    if &observed != reference_ids {
                        return Err(format!("the original driver's {name} test order differs"));
                    }
                } else {
                    reference_ids = Some(observed);
                }
            } else if out.status.code() != Some(1) {
                return Err(format!(
                    "the {package}'s original driver accepted an unknown module"
                ));
            }
        }
        if ids.is_none()
            && (pair[0]["stdout"] != pair[1]["stdout"]
                || pair[0]["stderr"] != pair[1]["stderr"]
                || pair[0]["stdout"].as_str().is_none_or(str::is_empty))
        {
            return Err("the original driver's unknown-module diagnostic differs".into());
        }
        reports.push(serde_json::json!({ "mode": name, "runs": pair }));
        println!(
            "py-driver: {name}: wheel and port match ({} runs)",
            ids.map_or(0, Vec::len)
        );
    }
    Ok(())
}

/// Validate unittest's standard verbose transcript, keeping duplicate inherited runs.
fn successful_ids(stderr: &str, expected: &[String]) -> Result<Vec<String>, String> {
    let text = stderr.replace("\r\n", "\n");
    let count = text
        .lines()
        .find_map(|line| line.strip_prefix("Ran "))
        .and_then(|line| line.split_whitespace().next())
        .and_then(|n| n.parse::<usize>().ok());
    let observed: Vec<String> = text
        .lines()
        .filter_map(|line| {
            let (method, rest) = line.split_once(" (")?;
            method.starts_with("test").then_some(())?;
            Some(rest.split_once(')')?.0.to_owned())
        })
        .collect();
    let mut sorted = observed.clone();
    sorted.sort();
    let mut expected = expected.to_vec();
    expected.sort();
    if count != Some(expected.len()) || sorted != expected || !text.trim_end().ends_with("\nOK") {
        return Err("did not pass every wheel-inventoried run with unittest's OK footer".into());
    }
    Ok(observed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_or_skipped_driver_runs_cannot_pass() {
        // unittest's verbose format; ids are inputs to this reporting control.
        let transcript =
            "test_a (Example.C.test_a)\nDescription ... ok\n\nRan 1 test in 0.001s\n\nOK\n";
        let ids = vec!["Example.C.test_a".to_owned()];
        assert_eq!(successful_ids(transcript, &ids).unwrap(), ids);
        assert!(successful_ids(&transcript.replace("Ran 1", "Ran 0"), &ids).is_err());
        assert!(successful_ids(&transcript.replace("\nOK\n", "\nOK (skipped=1)\n"), &ids).is_err());
        assert!(successful_ids(transcript, &[ids[0].clone(), ids[0].clone()]).is_err());
        assert!(successful_ids(&transcript.replace("Example.C", "Other.C"), &ids).is_err());
    }
}
