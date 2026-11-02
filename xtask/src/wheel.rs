// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The wheel `opencolorio-rs` (PLAN.md D14). `cargo xtask wheel-build` lays out the uv project
//! `py-wheel/` and runs `uv build`, which runs maturin:
//! - `py-wheel/python/PyOpenColorIO/`: the package as `cargo xtask py-build` lays it out, with
//!   the same function (`py::lay_out_package`), in a release build. maturin adds the extension.
//! - `py-wheel/LICENSE`, `NOTICE`, `THIRD-PARTY-NOTICES.md` and `crate-licenses/`: the license
//!   files. PEP 639 keeps `license-files` inside the project, and `THIRD-PARTY-NOTICES.md` says
//!   a binary release carries the license files of the crates it links, so they are copied here
//!   from Cargo's registry, at `Cargo.lock`'s versions.
//!
//! Those are generated, gitignored and removed again after the build. The wheel goes to
//! `<target>/wheel/`. On Linux, `wheel_audit` then proves its platform tag.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::py::{self, Layout};

pub(crate) const USAGE: &str = "\
cargo xtask wheel-build
    Builds the wheel opencolorio-rs into <target>/wheel/ in a release build: lays out py-wheel/
    (the package as py-build does, the license files) and runs `uv build`, which runs maturin
    at the pin of py-wheel/pyproject.toml and the hashes of py-wheel/build-constraints.txt.
    On Linux, its platform tag must be what all its binaries need (wheel_audit.rs). Never
    publishes anything.
";

/// The uv project of the wheel.
const PROJECT: &str = "py-wheel";

/// What [`lay_out`] writes in the project: the rest of it is committed.
const GENERATED: [&str; 6] = [
    "python",
    "LICENSE",
    "NOTICE",
    "THIRD-PARTY-NOTICES.md",
    "crate-licenses",
    "rust-library",
];

/// The project's own license files, at the repository's root.
const ROOT_LICENSES: [&str; 3] = ["LICENSE", "NOTICE", "THIRD-PARTY-NOTICES.md"];

/// What maturin's PEP 517 hook adds to its command: without this it passes
/// `--compatibility off`, which skips the tag's audit on Linux. `pypi`: the lowest `manylinux`
/// tag the extension fits, or an error if PyPI wouldn't take it. `check`: an error instead of
/// bundling a library.
const MATURIN_ARGS: &str = "maturin.build-args=--compatibility pypi --auditwheel check";

pub(crate) fn wheel_build(args: &[&str]) -> Result<(), String> {
    if !args.is_empty() {
        return Err(format!("unknown arguments {args:?}\n\n{USAGE}"));
    }
    let wheel = build(&Layout::new())?;
    println!("wheel-build: {}", crate::display_path(&wheel));
    Ok(())
}

/// Builds the wheel; its path.
pub(crate) fn build(layout: &Layout) -> Result<PathBuf, String> {
    py::sync_env(layout)?;
    let project = layout.root.join(PROJECT);
    remove_generated(&project)?;
    let built = lay_out(&layout.root, &project).and_then(|()| uv_build(layout, &project));
    remove_generated(&project)?;
    let wheel = built?;
    if !cfg!(windows) {
        let scratch = wheel.with_file_name("audit");
        let found = crate::wheel_audit::audit(&layout.python(), &wheel, &scratch)?;
        println!("wheel-build: {found}");
    }
    Ok(wheel)
}

fn remove_generated(project: &Path) -> Result<(), String> {
    for name in GENERATED {
        let path = project.join(name);
        let removed = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else if path.exists() {
            std::fs::remove_file(&path)
        } else {
            Ok(())
        };
        removed.map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

/// Writes the project's generated files.
fn lay_out(root: &Path, project: &Path) -> Result<(), String> {
    let package = project.join("python").join("PyOpenColorIO");
    std::fs::create_dir_all(&package).map_err(|e| format!("{}: {e}", package.display()))?;
    py::lay_out_package(root, &package, true)?;
    let laid_out = files_under(&package)?;
    let expected = crate::wheel_check::package_files(std::env::consts::EXE_SUFFIX);
    if laid_out != expected {
        return Err(format!(
            "the package holds {laid_out:?}, but the wheel's check expects {expected:?}"
        ));
    }
    if !cfg!(windows) {
        strip_tools(&package.join("bin"))?;
    }
    for (name, source) in license_files(root)? {
        let destination = project.join(name);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        py::copy(&source, &destination)?;
    }
    Ok(())
}

/// The files in `dir` and below, by their paths from it, with `/`.
fn files_under(dir: &Path) -> Result<BTreeSet<String>, String> {
    let mut files = BTreeSet::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(next) = dirs.pop() {
        for entry in std::fs::read_dir(&next).map_err(|e| format!("{}: {e}", next.display()))? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                dirs.push(path);
            } else if let Ok(relative) = path.strip_prefix(dir) {
                files.insert(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    Ok(files)
}

/// Strips the tools, as `[tool.maturin] strip` does the extension: a Linux tool is 30 MB with the
/// release profile's line tables, 5 MB without them. Windows keeps those in `.pdb` files.
fn strip_tools(bin: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(bin).map_err(|e| format!("{}: {e}", bin.display()))? {
        let tool = entry.map_err(|e| e.to_string())?.path();
        let status = Command::new("strip")
            .arg(&tool)
            .stdin(Stdio::null())
            .status()
            .map_err(|e| format!("could not run strip: {e}"))?;
        if !status.success() {
            return Err(format!("strip {}: {status}", tool.display()));
        }
    }
    Ok(())
}

/// Each license file the wheel carries, by its path in the project: the project's, and those of
/// the crates that can be linked into it, which must be the ones `THIRD-PARTY-NOTICES.md` lists.
pub(crate) fn license_files(root: &Path) -> Result<BTreeMap<String, PathBuf>, String> {
    let notices = root.join("THIRD-PARTY-NOTICES.md");
    let text =
        std::fs::read_to_string(&notices).map_err(|e| format!("{}: {e}", notices.display()))?;
    let listed = crates_in_notices(&text);
    let linked = crates_in_tree(&cargo(
        root,
        &[
            "tree",
            "--locked",
            "--package",
            "ocio-py",
            "--package",
            "ocio-tools",
            "--features",
            "ocio-py/extension-module",
            "--edges",
            "normal,no-proc-macro",
            "--target",
            "all",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ],
    )?);
    if linked.is_empty() || linked != listed {
        return Err(format!(
            "THIRD-PARTY-NOTICES.md lists other crates than can be linked into the wheel: \
             only listed {:?}, only linked {:?}",
            listed.difference(&linked).collect::<Vec<_>>(),
            linked.difference(&listed).collect::<Vec<_>>()
        ));
    }
    let metadata: serde_json::Value = serde_json::from_str(&cargo(
        root,
        &[
            "metadata",
            "--locked",
            "--all-features",
            "--format-version",
            "1",
        ],
    )?)
    .map_err(|e| format!("cargo metadata: {e}"))?;
    let mut files: BTreeMap<String, PathBuf> = ROOT_LICENSES
        .iter()
        .map(|name| ((*name).to_string(), root.join(name)))
        .collect();
    files.insert(
        "rust-library/COPYRIGHT-library.html".to_string(),
        rust_library_copyright(root)?,
    );
    for (name, version) in &linked {
        let manifest = metadata["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["name"] == name.as_str() && p["version"] == version.as_str())
            .and_then(|p| p["manifest_path"].as_str())
            .ok_or_else(|| format!("cargo metadata doesn't list {name} {version}"))?;
        let dir = Path::new(manifest)
            .parent()
            .ok_or_else(|| format!("{manifest} has no directory"))?;
        let found = license_names(dir)?;
        if found.is_empty() {
            return Err(format!(
                "{name} {version} has no license file in {}",
                dir.display()
            ));
        }
        for file in found {
            let source = dir.join(&file);
            files.insert(format!("crate-licenses/{name}-{version}/{file}"), source);
        }
    }
    Ok(files)
}

fn license_names(dir: &Path) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.path().is_file() && is_license_name(&name) {
            names.push(name);
        }
    }
    Ok(names)
}

/// The attribution of Rust's standard library, which every binary links statically, for the
/// toolchain `rust-toolchain.toml` pins: a toolchain change needs that toolchain's file.
fn rust_library_copyright(root: &Path) -> Result<PathBuf, String> {
    let manifest = root.join("rust-toolchain.toml");
    let text = std::fs::read_to_string(&manifest)
        .map_err(|e| format!("{}: {e}", manifest.display()))?;
    let channel = text
        .lines()
        .find_map(|line| line.strip_prefix("channel = "))
        .map(|value| value.trim().trim_matches('"'))
        .ok_or_else(|| format!("{} names no channel", manifest.display()))?;
    let path = root.join(format!("docs/licenses/rust-library-{channel}.html"));
    if !path.is_file() {
        return Err(format!(
            "{} is missing: copy it from that toolchain's share/doc/rust/COPYRIGHT-library.html",
            path.display()
        ));
    }
    Ok(path)
}

/// The names `LICEN[CS]E*`, `COPYING*`, `NOTICE*`, as PEP 639's default globs have them.
fn is_license_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["LICENSE", "LICENCE", "COPYING", "NOTICE"]
        .iter()
        .any(|prefix| upper.starts_with(prefix))
}

/// The table of crates that can be linked, in `THIRD-PARTY-NOTICES.md`'s last section.
fn crates_in_notices(text: &str) -> BTreeSet<(String, String)> {
    let section = text
        .split_once("\n## Rust crates in built artifacts\n")
        .map_or("", |(_, section)| section);
    let table = section
        .split_once("These run only at compile time")
        .map_or(section, |(table, _)| table);
    table
        .lines()
        .filter_map(|line| {
            let mut cells = line.strip_prefix('|')?.split('|').map(str::trim);
            let (name, version) = (cells.next()?, cells.next()?);
            version
                .starts_with(|c: char| c.is_ascii_digit())
                .then(|| (name.to_string(), version.to_string()))
        })
        .collect()
}

/// The registry crates of `cargo tree --prefix none --format {p}`: a workspace member's line
/// ends with its path, and one seen before with ` (*)`.
fn crates_in_tree(text: &str) -> BTreeSet<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_end().trim_end_matches(" (*)");
            let (name, version) = line.split_once(" v")?;
            (!version.contains(' ')).then(|| (name.to_string(), version.to_string()))
        })
        .collect()
}

/// What `cargo <args>` prints.
fn cargo(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .current_dir(root)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| format!("cargo {}: {e}", args[0]))?;
    if !out.status.success() {
        return Err(format!("cargo {} failed: {}", args[0], out.status));
    }
    String::from_utf8(out.stdout).map_err(|e| format!("cargo {}: {e}", args[0]))
}

/// `uv build` of the project into `<target>/wheel/`; the wheel's path.
fn uv_build(layout: &Layout, project: &Path) -> Result<PathBuf, String> {
    let out = crate::plain_path(ocio_testkit::paths::target_dir()).join("wheel");
    match std::fs::remove_dir_all(&out) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("{}: {e}", out.display()));
        }
        _ => {}
    }
    let status = Command::new(py::uv())
        .arg("build")
        .arg(project)
        .arg("--wheel")
        .arg("--out-dir")
        .arg(&out)
        .arg("--python")
        .arg(layout.python())
        .arg("--build-constraints")
        .arg(project.join("build-constraints.txt"))
        .arg("--require-hashes")
        .args(["--config-setting", MATURIN_ARGS])
        .env_remove("VIRTUAL_ENV")
        .stdin(Stdio::null())
        .status()
        .map_err(|e| format!("could not run `uv` ({e})"))?;
    if !status.success() {
        return Err(format!("uv build of {PROJECT}/ failed: {status}"));
    }
    the_wheel_in(&out)
}

/// The directory's one wheel.
fn the_wheel_in(dir: &Path) -> Result<PathBuf, String> {
    let mut wheels = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|e| e == "whl") {
            wheels.push(path);
        }
    }
    match wheels.len() {
        1 => Ok(wheels.remove(0)),
        n => Err(format!("{n} wheels in {}, not one", dir.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(name: &str, version: &str) -> (String, String) {
        (name.to_string(), version.to_string())
    }

    #[test]
    fn the_notices_table_of_linked_crates_is_read_without_the_compile_time_one() {
        let text = "# x\n\n| a | 1 | no |\n\n## Rust crates in built artifacts\n\nText.\n\n\
                    | Crate | Version | License |\n|---|---|---|\n| cfg-if | 1.0.5 | MIT |\n\
                    | numpy | 0.29.0 | BSD-2-Clause |\n\nThese run only at compile time, for \
                    macros:\n\n| syn | 2.0.119 | MIT |\n";
        assert_eq!(
            crates_in_notices(text),
            BTreeSet::from([pair("cfg-if", "1.0.5"), pair("numpy", "0.29.0")])
        );
        assert!(crates_in_notices("| a | 1 | no |\n").is_empty());
    }

    #[test]
    fn the_tree_lists_registry_crates_once_and_no_workspace_member() {
        let text = "ocio-py v0.0.0+ocio.2.5.2 (D:\\w\\crates\\ocio-py)\nhalf v2.7.1\n\
                    half v2.7.1 (*)\nocio v0.0.0+ocio.2.5.2 (/work/crates/ocio) (*)\n\
                    libc v0.2.189\n";
        assert_eq!(
            crates_in_tree(text),
            BTreeSet::from([pair("half", "2.7.1"), pair("libc", "0.2.189")])
        );
    }

    #[test]
    fn license_names_are_recognised() {
        for name in ["LICENSE", "LICENSE-MIT", "license.txt", "COPYING", "NOTICE"] {
            assert!(is_license_name(name), "{name}");
        }
        for name in ["README.md", "Cargo.toml", "src"] {
            assert!(!is_license_name(name), "{name}");
        }
    }

    #[test]
    fn the_generated_files_are_gitignored_and_the_committed_ones_are_not() {
        let text = std::fs::read_to_string(crate::root().join(".gitignore")).unwrap();
        assert!(text.lines().any(|l| l == "/py-wheel/*"));
        for kept in ["build-constraints.txt", "pyproject.toml", "README.md"] {
            assert!(text.lines().any(|l| l == format!("!/py-wheel/{kept}")));
            assert!(!GENERATED.contains(&kept));
        }
    }

    /// `pyproject.toml`'s version is the workspace's without its local segment: PyPI refuses
    /// one (owner decision 2026-11-02), and maturin would take the workspace's as it stands.
    #[test]
    fn the_wheels_version_is_the_workspaces_without_its_local_segment() {
        let read = |path: &str| -> toml::Value {
            toml::from_str(&std::fs::read_to_string(crate::root().join(path)).unwrap()).unwrap()
        };
        let workspace = read("Cargo.toml");
        let workspace = workspace["workspace"]["package"]["version"]
            .as_str()
            .unwrap();
        let wheel = read("py-wheel/pyproject.toml");
        assert_eq!(
            Some(wheel["project"]["version"].as_str().unwrap()),
            workspace.split('+').next()
        );
    }
}
