// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The workspace's packages and their targets, from `cargo metadata --no-deps`
//! (`cargo xtask test-shard`).

use std::path::Path;
use std::process::{Command, Stdio};

/// A workspace member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Package {
    pub(crate) name: String,
    pub(crate) targets: Vec<Target>,
}

/// One of a package's targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) name: String,
    /// Cargo's kinds: `lib`, `rlib`, `cdylib`, `proc-macro`, `bin`, `test`, `example`,
    /// `bench`, `custom-build`, ...
    pub(crate) kinds: Vec<String>,
    /// Whether `cargo test` runs it by default (`test = true` in the manifest).
    pub(crate) test: bool,
    /// Whether `cargo test` runs its documentation tests.
    pub(crate) doctest: bool,
}

/// The workspace members of the workspace at `root`, in `cargo metadata`'s order.
pub(crate) fn workspace(root: &Path) -> Result<Vec<Package>, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = Command::new(cargo)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    parse(&String::from_utf8_lossy(&out.stdout))
}

/// The workspace members in `cargo metadata --format-version 1` output.
pub(crate) fn parse(json: &str) -> Result<Vec<Package>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("cargo metadata: {e}"))?;
    let str_of = |v: &serde_json::Value, key: &str| -> Result<String, String> {
        v.get(key)
            .and_then(|s| s.as_str())
            .map(String::from)
            .ok_or_else(|| format!("cargo metadata: no `{key}`"))
    };
    let members: Vec<&str> = value
        .get("workspace_members")
        .and_then(|m| m.as_array())
        .ok_or("cargo metadata: no `workspace_members`")?
        .iter()
        .filter_map(|m| m.as_str())
        .collect();
    let mut packages = Vec::new();
    for package in value
        .get("packages")
        .and_then(|p| p.as_array())
        .ok_or("cargo metadata: no `packages`")?
    {
        if !members.contains(&str_of(package, "id")?.as_str()) {
            continue;
        }
        let name = str_of(package, "name")?;
        let mut targets = Vec::new();
        for target in package
            .get("targets")
            .and_then(|t| t.as_array())
            .ok_or_else(|| format!("cargo metadata: {name} has no `targets`"))?
        {
            let flag = |key: &str| target.get(key).and_then(|b| b.as_bool()).unwrap_or(false);
            targets.push(Target {
                name: str_of(target, "name")?,
                kinds: target
                    .get("kind")
                    .and_then(|k| k.as_array())
                    .map(|k| {
                        k.iter()
                            .filter_map(|k| k.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                test: flag("test"),
                doctest: flag("doctest"),
            });
        }
        packages.push(Package { name, targets });
    }
    Ok(packages)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of `cargo metadata --no-deps --format-version 1` (cargo 1.98.1 on Windows),
    /// trimmed to the fields read here.
    const SAMPLE: &str = r#"{
        "packages": [
            {"name": "ocio-ops", "id": "path+file:///D:/w/crates/ocio-ops#0.0.0",
             "manifest_path": "D:\\w\\crates\\ocio-ops\\Cargo.toml",
             "targets": [
                {"kind": ["lib"], "name": "ocio_ops", "test": true, "doctest": true},
                {"kind": ["test"], "name": "log_oracle", "test": true, "doctest": false}
             ]},
            {"name": "xtask", "id": "path+file:///D:/w/xtask#0.0.0",
             "manifest_path": "D:\\w\\xtask\\Cargo.toml",
             "targets": [
                {"kind": ["bin"], "name": "xtask", "test": true, "doctest": false}
             ]}
        ],
        "workspace_members": [
            "path+file:///D:/w/crates/ocio-ops#0.0.0",
            "path+file:///D:/w/xtask#0.0.0"
        ],
        "workspace_root": "D:\\w"
    }"#;

    #[test]
    fn reads_packages_and_targets() {
        let packages = parse(SAMPLE).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "ocio-ops");
        assert_eq!(packages[1].name, "xtask");
        assert_eq!(
            packages[0].targets[1],
            Target {
                name: "log_oracle".into(),
                kinds: vec!["test".into()],
                test: true,
                doctest: false
            }
        );
        assert!(packages[0].targets[0].doctest);
    }

    #[test]
    fn reads_this_workspace() {
        let packages = workspace(&crate::root()).unwrap();
        let xtask = packages.iter().find(|p| p.name == "xtask").unwrap();
        assert!(xtask.targets.iter().any(|t| t.kinds == ["bin"] && t.test));
        assert!(packages.iter().any(|p| p.name == "ocio-ops"));
    }
}
