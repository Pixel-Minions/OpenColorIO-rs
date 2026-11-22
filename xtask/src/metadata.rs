// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The workspace's packages, their directories and their targets, from
//! `cargo metadata --no-deps` (`cargo xtask test-shard`, `cargo xtask gate --touched`), and
//! which of them depend on which (`cargo xtask land`'s test steps, `gate`'s Python steps).

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
    parse(&metadata(root)?)
}

/// The workspace members and their dependencies on each other ([`dependencies`]), from one
/// `cargo metadata` run.
pub(crate) fn workspace_graph(root: &Path) -> Result<(Vec<Package>, Graph), String> {
    let json = metadata(root)?;
    Ok((parse(&json)?, dependencies(&json)?))
}

/// `cargo metadata --no-deps --format-version 1` of the workspace at `root`.
fn metadata(root: &Path) -> Result<String, String> {
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
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Each workspace member, in `cargo metadata`'s order, with the workspace members it depends on
/// (normal, build and dev dependencies: any of them can change what its tests see).
pub(crate) type Graph = Vec<(String, Vec<String>)>;

/// The [`Graph`] of the workspace members in `cargo metadata --format-version 1` output. A
/// dependency is a member's when it names a member and comes from a path (not a registry
/// crate of the same name).
pub(crate) fn dependencies(json: &str) -> Result<Graph, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("cargo metadata: {e}"))?;
    let members: Vec<&str> = value
        .get("workspace_members")
        .and_then(|m| m.as_array())
        .ok_or("cargo metadata: no `workspace_members`")?
        .iter()
        .filter_map(|m| m.as_str())
        .collect();
    let packages: Vec<&serde_json::Value> = value
        .get("packages")
        .and_then(|p| p.as_array())
        .ok_or("cargo metadata: no `packages`")?
        .iter()
        .filter(|p| {
            p.get("id")
                .and_then(|id| id.as_str())
                .is_some_and(|id| members.contains(&id))
        })
        .collect();
    let name = |v: &serde_json::Value| v.get("name").and_then(|n| n.as_str()).map(String::from);
    let names: Vec<String> = packages.iter().filter_map(|p| name(p)).collect();
    let mut graph = Vec::new();
    for package in packages {
        let Some(own) = name(package) else {
            return Err("cargo metadata: a package without a `name`".into());
        };
        let mut deps: Vec<String> = Vec::new();
        for dep in package
            .get("dependencies")
            .and_then(|d| d.as_array())
            .into_iter()
            .flatten()
        {
            let from_path = dep.get("path").is_some_and(|p| p.is_string());
            if let Some(dep) = name(dep)
                && from_path
                && names.contains(&dep)
                && !deps.contains(&dep)
            {
                deps.push(dep);
            }
        }
        graph.push((own, deps));
    }
    Ok(graph)
}

/// `names` and every workspace member that depends on one of them, directly or through others,
/// in the graph's order.
pub(crate) fn with_dependents(names: &[String], graph: &Graph) -> Vec<String> {
    let mut found: Vec<&str> = names.iter().map(String::as_str).collect();
    loop {
        let before = found.len();
        for (package, deps) in graph {
            if !found.contains(&package.as_str())
                && deps.iter().any(|d| found.contains(&d.as_str()))
            {
                found.push(package);
            }
        }
        if found.len() == before {
            break;
        }
    }
    let mut out: Vec<String> = graph
        .iter()
        .filter(|(p, _)| found.contains(&p.as_str()))
        .map(|(p, _)| p.clone())
        .collect();
    // Names the graph doesn't know stay, at the end.
    out.extend(
        names
            .iter()
            .filter(|n| !graph.iter().any(|(p, _)| p == *n))
            .cloned(),
    );
    out
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
            {"name": "opencolorio-rs-ops", "id": "path+file:///D:/w/crates/ocio-ops#opencolorio-rs-ops@0.0.0",
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
            "path+file:///D:/w/crates/ocio-ops#opencolorio-rs-ops@0.0.0",
            "path+file:///D:/w/xtask#0.0.0"
        ],
        "workspace_root": "D:\\w"
    }"#;

    #[test]
    fn reads_packages_and_targets() {
        let packages = parse(SAMPLE).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "opencolorio-rs-ops");
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

    /// The shape of a package's `dependencies` in `cargo metadata --no-deps --format-version 1`
    /// (cargo 1.98.1 on Windows), trimmed to the fields read here: a renamed path dependency, a
    /// dev-dependency, a registry crate, and a registry crate named like a member.
    const DEPENDENCIES: &str = r#"{
        "packages": [
            {"name": "a", "id": "path+file:///D:/w/a#0.0.0", "dependencies": [
                {"name": "half", "source": "registry+https://github.com/rust-lang/crates.io-index",
                 "kind": null, "rename": null}
            ]},
            {"name": "b", "id": "path+file:///D:/w/b#0.0.0", "dependencies": [
                {"name": "a", "kind": null, "rename": "aa", "path": "D:\\w\\a"},
                {"name": "kit", "kind": "dev", "rename": null, "path": "D:\\w\\kit"},
                {"name": "a", "kind": "build", "rename": null, "path": "D:\\w\\a"}
            ]},
            {"name": "kit", "id": "path+file:///D:/w/kit#0.0.0", "dependencies": []},
            {"name": "c", "id": "path+file:///D:/w/c#0.0.0", "dependencies": [
                {"name": "kit", "source": "registry+https://github.com/rust-lang/crates.io-index",
                 "kind": null, "rename": null}
            ]},
            {"name": "half", "id": "registry+https://github.com/rust-lang/crates.io-index#half@2.7.1",
             "dependencies": []}
        ],
        "workspace_members": [
            "path+file:///D:/w/a#0.0.0",
            "path+file:///D:/w/b#0.0.0",
            "path+file:///D:/w/kit#0.0.0",
            "path+file:///D:/w/c#0.0.0"
        ],
        "workspace_root": "D:\\w"
    }"#;

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reads_dependencies_between_members() {
        let graph = dependencies(DEPENDENCIES).unwrap();
        assert_eq!(
            graph,
            vec![
                ("a".to_string(), Vec::new()),
                ("b".to_string(), strings(&["a", "kit"])),
                ("kit".to_string(), Vec::new()),
                ("c".to_string(), Vec::new()),
            ]
        );
    }

    #[test]
    fn dependents_close_transitively() {
        // d -> c -> b -> a, and a dev-dependency cycle a -> e -> a; f stands alone.
        let graph: Graph = vec![
            ("a".into(), strings(&["e"])),
            ("b".into(), strings(&["a"])),
            ("c".into(), strings(&["b"])),
            ("d".into(), strings(&["c", "f"])),
            ("e".into(), strings(&["a"])),
            ("f".into(), Vec::new()),
        ];
        let of = |names: &[&str]| with_dependents(&strings(names), &graph);
        assert_eq!(of(&["c"]), strings(&["c", "d"]));
        assert_eq!(of(&["a"]), strings(&["a", "b", "c", "d", "e"]));
        assert_eq!(of(&["f"]), strings(&["d", "f"]));
        assert_eq!(of(&["d"]), strings(&["d"]));
        // In the graph's order, once each; an unknown name stays, at the end.
        assert_eq!(of(&["d", "c", "c"]), strings(&["c", "d"]));
        assert_eq!(of(&["zz", "f"]), strings(&["d", "f", "zz"]));
        assert_eq!(of(&[]), Vec::<String>::new());
        // A dependent listed before what it depends on through another package.
        let reversed: Graph = vec![
            ("x".into(), strings(&["y"])),
            ("y".into(), strings(&["z"])),
            ("z".into(), Vec::new()),
        ];
        assert_eq!(
            with_dependents(&strings(&["z"]), &reversed),
            strings(&["x", "y", "z"])
        );
    }

    /// The test steps of a change to one package of this workspace cover the packages that
    /// depend on it.
    #[test]
    fn dependents_in_this_workspace() {
        let (packages, graph) = workspace_graph(&crate::root()).unwrap();
        assert_eq!(
            graph.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
            packages.iter().map(|p| p.name.as_str()).collect::<Vec<_>>()
        );
        let of = |name: &str| with_dependents(&[name.to_string()], &graph);
        let ops = of("opencolorio-rs-ops");
        for name in [
            "opencolorio-rs-ops",
            "opencolorio-rs-formats",
            "opencolorio-rs-gpu",
            "opencolorio-rs",
            "ocio-py",
            "ocio-tools",
            "ocio-gpu-exec",
        ] {
            assert!(ops.contains(&name.to_string()), "{name}: {ops:?}");
        }
        for name in ["xtask", "ocio-testkit", "ocio-py-ffi"] {
            assert!(!ops.contains(&name.to_string()), "{name}: {ops:?}");
        }
        assert_eq!(of("xtask"), ["xtask"]);
        assert_eq!(of("ocio-py-ffi"), ["ocio-py", "ocio-py-ffi"]);
        // A dev-dependency counts.
        let kit = of("ocio-testkit");
        assert!(kit.contains(&"opencolorio-rs-ops".to_string()), "{kit:?}");
        assert!(kit.contains(&"xtask".to_string()), "{kit:?}");
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
                .any(|p| p.name == "opencolorio-rs-ops" && p.dir == "crates/ocio-ops")
        );
    }
}
