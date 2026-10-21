// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The workspace's packages, their directories and their targets, from
//! `cargo metadata --no-deps` (`cargo xtask test-shard`, `cargo xtask gate --touched`).

use std::path::Path;
use std::process::{Command, Stdio};

/// A workspace member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Package {
    pub(crate) name: String,
    /// Its directory relative to the workspace root, with `/` (`crates/ocio-ops`), or `""` for
    /// a package at the root.
    pub(crate) dir: String,
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
    let root = slashes(&str_of(&value, "workspace_root")?);
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
        let manifest = slashes(&str_of(package, "manifest_path")?);
        let dir = manifest
            .strip_suffix("/Cargo.toml")
            .and_then(|d| strip_dir_prefix(d, &root))
            .ok_or_else(|| {
                format!("cargo metadata: {name}'s manifest {manifest} is not under {root}")
            })?;
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
        packages.push(Package { name, dir, targets });
    }
    Ok(packages)
}

/// `path` with `/` separators.
fn slashes(path: &str) -> String {
    path.replace('\\', "/")
}

/// `dir` relative to `root` (`""` for the root itself), when it is `root` or inside it.
fn strip_dir_prefix(dir: &str, root: &str) -> Option<String> {
    let root = root.trim_end_matches('/');
    let rest = dir.strip_prefix(root)?;
    if rest.is_empty() {
        Some(String::new())
    } else {
        rest.strip_prefix('/').map(String::from)
    }
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
        assert_eq!(packages[0].dir, "crates/ocio-ops");
        assert_eq!(packages[1].dir, "xtask");
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
    fn package_directories() {
        assert_eq!(
            strip_dir_prefix("/w/crates/a", "/w"),
            Some("crates/a".into())
        );
        assert_eq!(strip_dir_prefix("/w", "/w/"), Some(String::new()));
        assert_eq!(strip_dir_prefix("/wx/a", "/w"), None);
        assert_eq!(strip_dir_prefix("/v/a", "/w"), None);
    }

    #[test]
    fn reads_this_workspace() {
        let packages = workspace(&crate::root()).unwrap();
        let xtask = packages.iter().find(|p| p.name == "xtask").unwrap();
        assert_eq!(xtask.dir, "xtask");
        assert!(xtask.targets.iter().any(|t| t.kinds == ["bin"] && t.test));
        assert!(
            packages
                .iter()
                .any(|p| p.name == "ocio-ops" && p.dir == "crates/ocio-ops")
        );
    }
}
