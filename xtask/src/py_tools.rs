// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Upstream's package launcher with the four in-scope Rust developer executables.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const TOOLS: [&str; 4] = ["ociobakelut", "ociocheck", "ociochecklut", "ociowrite"];

/// Lay out the native tools where upstream's `package/command_line.py` looks for them.
pub(crate) fn stage(layout: &crate::py::Layout, release: bool) -> Result<(), String> {
    let mut cmd = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    cmd.current_dir(&layout.root)
        .args([
            "build",
            "--package",
            "ocio-tools",
            "--bins",
            "--message-format",
            "json-render-diagnostics",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::inherit());
    if release {
        cmd.arg("--release");
    }
    let out = cmd
        .output()
        .map_err(|e| format!("building packaged developer tools: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "building packaged developer tools failed: {}",
            out.status
        ));
    }
    let artifacts = tool_artifacts(&String::from_utf8_lossy(&out.stdout))?;
    let bin = layout.package().join("bin");
    std::fs::create_dir_all(&bin).map_err(|e| format!("{}: {e}", bin.display()))?;
    for (name, source) in artifacts {
        let destination = bin.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        std::fs::copy(&source, &destination)
            .map_err(|e| format!("{} -> {}: {e}", source.display(), destination.display()))?;
    }
    let source = layout
        .root
        .join("crates/ocio-py/python/PyOpenColorIO/command_line.py");
    let destination = layout.package().join("command_line.py");
    std::fs::copy(&source, &destination)
        .map_err(|e| format!("{} -> {}: {e}", source.display(), destination.display()))?;
    Ok(())
}

fn tool_artifacts(messages: &str) -> Result<BTreeMap<String, PathBuf>, String> {
    let mut tools = BTreeMap::new();
    for line in messages.lines() {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if message["reason"] != "compiler-artifact"
            || message["target"]["kind"] != serde_json::json!(["bin"])
        {
            continue;
        }
        let name = message["target"]["name"]
            .as_str()
            .ok_or("missing native tool name")?;
        let executable = message["executable"]
            .as_str()
            .ok_or("missing native tool executable")?;
        if !TOOLS.contains(&name)
            || tools
                .insert(name.to_owned(), PathBuf::from(executable))
                .is_some()
        {
            return Err(format!("unexpected or duplicate packaged tool {name}"));
        }
    }
    if tools.len() != TOOLS.len() {
        return Err("Cargo did not build all four packaged developer tools".into());
    }
    Ok(tools)
}

/// Compare both launcher entry paths with the official wheel's unchanged launcher.
pub(crate) fn check(layout: &crate::py::Layout) -> Result<(), String> {
    const CALL: &str = "import sys; from PyOpenColorIO.command_line import call_program; raise SystemExit(call_program(sys.argv[1], sys.argv[2:]))";
    const MAIN: &str = "import os,sys; from PyOpenColorIO.command_line import main; sys.argv=[os.path.join('launcher-dir',sys.argv[1]),*sys.argv[2:]]; main()";
    let oracle = ocio_testkit::Oracle::get();
    let port_python = layout.python();
    let mut reports = Vec::new();
    for name in TOOLS {
        let executable = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        for (mode, script) in [("call_program", CALL), ("main", MAIN)] {
            for args in [["--help"], ["--no-such-ocio-launcher-option"]] {
                let mut outputs = Vec::new();
                for python in [oracle.python(), port_python.as_path()] {
                    let mut cmd = Command::new(python);
                    cmd.args(["-I", "-B", "-X", "utf8", "-c", script])
                        .arg(&executable)
                        .args(args)
                        .current_dir(&layout.root)
                        .env_clear()
                        .stdin(Stdio::null());
                    for variable in ["SYSTEMROOT", "TEMP", "TMP", "TMPDIR"] {
                        if let Some(value) = std::env::var_os(variable) {
                            cmd.env(variable, value);
                        }
                    }
                    let out = cmd
                        .output()
                        .map_err(|e| format!("packaged {name}/{mode}: {e}"))?;
                    outputs.push(out);
                }
                let reference = &outputs[0];
                let port = &outputs[1];
                let matches = reference.status.code() == port.status.code()
                    && reference.status.code().is_some()
                    && reference.stdout == port.stdout
                    && reference.stderr == port.stderr;
                reports.push(serde_json::json!({ "tool": name, "mode": mode, "args": args, "matches": matches,
                    "wheel": { "returncode": reference.status.code(), "stdout": reference.stdout, "stderr": reference.stderr },
                    "port": { "returncode": port.status.code(), "stdout": port.stdout, "stderr": port.stderr } }));
                let path = layout.py.with_file_name("py-tools.json");
                std::fs::write(
                    &path,
                    serde_json::to_vec_pretty(&reports).map_err(|e| e.to_string())?,
                )
                .map_err(|e| format!("{}: {e}", path.display()))?;
                if !matches {
                    return Err(format!(
                        "packaged {name}/{mode} {args:?} differs from the wheel; {}\n{reference:?}\n{port:?}",
                        path.display()
                    ));
                }
            }
        }
    }
    println!("py-tools: both launcher paths for all four tools match the wheel (16 comparisons)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_launcher_is_upstreams_file_verbatim() {
        // The submodule may be checked out with CRLF on Windows. Compare with
        // its pinned Git blob, without normalising or weakening either side.
        let upstream = crate::git(
            &crate::root().join("upstream/OpenColorIO"),
            &["show", "HEAD:src/bindings/python/package/command_line.py"],
        )
        .expect("the pinned upstream package launcher blob");
        assert_eq!(
            include_bytes!("../../crates/ocio-py/python/PyOpenColorIO/command_line.py").as_slice(),
            upstream.as_bytes()
        );
    }

    #[test]
    fn incomplete_or_duplicate_binary_manifests_are_rejected() {
        let lines = TOOLS.map(|name| {
            serde_json::json!({ "reason": "compiler-artifact",
            "target": { "name": name, "kind": ["bin"] }, "executable": format!("target/{name}") })
            .to_string()
        });
        assert_eq!(tool_artifacts(&lines.join("\n")).unwrap().len(), 4);
        assert!(tool_artifacts(&lines[..3].join("\n")).is_err());
        assert!(tool_artifacts(&[lines.as_slice(), &lines[..1]].concat().join("\n")).is_err());
        assert!(tool_artifacts(&lines.join("\n").replace("ociowrite", "another-tool")).is_err());
    }
}
