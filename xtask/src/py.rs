// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port's Python module (Phase 6, `docs/cards/phase6.md`, chunk 6.1a): `cargo xtask
//! py-build` builds `ocio-py` with its `extension-module` feature and lays it out as upstream's
//! package, in the port's own Python environment (P6-3, P6-4):
//! - `target/py-port`: the environment of the uv project `py-port/` (Python 3.13 and NumPy,
//!   without the wheel), made with `uv sync`;
//! - `target/py/PyOpenColorIO/`: the package, `__init__.py` (upstream's,
//!   `crates/ocio-py/python/PyOpenColorIO/__init__.py`) and the extension
//!   `PyOpenColorIO<EXT_SUFFIX>` (`.cp313-win_amd64.pyd`, `.cpython-313-x86_64-linux-gnu.so`);
//! - `ocio-rs-port.pth` in the environment's site-packages, which puts `target/py` on its
//!   path, so its interpreter imports `PyOpenColorIO` from there.
//!
//! PyO3 builds against the environment's interpreter (`PYO3_PYTHON`). `target` is this
//! platform's target directory (`CARGO_TARGET_DIR`; Rocky Linux 9 has its own).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(crate) const USAGE: &str = "\
cargo xtask py-build [--release]
    Builds the port's PyOpenColorIO (ocio-py, feature extension-module) against the Python of
    target/py-port (made from py-port/ with `uv sync` if needed), lays it out as upstream's
    package in target/py/PyOpenColorIO/, and puts target/py on target/py-port's path
    (ocio-rs-port.pth). --release: an optimized build.
";

/// The name of the `.pth` file that puts the package's directory on the environment's path.
const PTH: &str = "ocio-rs-port.pth";

/// Where things go.
#[derive(Debug)]
pub(crate) struct Layout {
    /// The workspace root.
    pub(crate) root: PathBuf,
    /// The port's Python environment, `<target>/py-port`.
    pub(crate) env: PathBuf,
    /// The directory on the environment's path, `<target>/py`.
    pub(crate) py: PathBuf,
}

impl Layout {
    pub(crate) fn new() -> Layout {
        let target = crate::plain_path(ocio_testkit::paths::target_dir());
        Layout {
            root: crate::root(),
            env: target.join("py-port"),
            py: target.join("py"),
        }
    }

    /// The environment's interpreter.
    pub(crate) fn python(&self) -> PathBuf {
        python_in(&self.env)
    }

    /// The package's directory.
    pub(crate) fn package(&self) -> PathBuf {
        self.py.join("PyOpenColorIO")
    }
}

/// The interpreter of the virtual environment `env`.
fn python_in(env: &Path) -> PathBuf {
    if cfg!(windows) {
        env.join("Scripts").join("python.exe")
    } else {
        env.join("bin").join("python")
    }
}

pub(crate) fn py_build(args: &[&str]) -> Result<(), String> {
    let release = match args {
        [] => false,
        ["--release"] => true,
        _ => return Err(format!("unknown arguments {args:?}\n\n{USAGE}")),
    };
    let layout = Layout::new();
    build(&layout, release)?;
    println!(
        "py-build: {} imports PyOpenColorIO from {}",
        crate::display_path(&layout.python()),
        crate::display_path(&layout.py)
    );
    Ok(())
}

/// Makes the environment, builds the module and lays the package out.
pub(crate) fn build(layout: &Layout, release: bool) -> Result<(), String> {
    sync_env(layout)?;
    let python = layout.python();
    let library = build_extension(&layout.root, &python, release)?;
    let suffix = query(
        &python,
        "import sysconfig; print(sysconfig.get_config_var('EXT_SUFFIX'))",
    )?;
    let site = query(
        &python,
        "import sysconfig; print(sysconfig.get_path('purelib'))",
    )?;
    let package = layout.package();
    std::fs::create_dir_all(&package).map_err(|e| format!("{}: {e}", package.display()))?;
    // A module built for another interpreter (another suffix) would shadow nothing, but is
    // stale: only this build's extension stays.
    for entry in std::fs::read_dir(&package)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("PyOpenColorIO.") && name != format!("PyOpenColorIO{suffix}") {
            std::fs::remove_file(entry.path()).map_err(|e| format!("{name}: {e}"))?;
        }
    }
    copy(
        &layout
            .root
            .join("crates/ocio-py/python/PyOpenColorIO/__init__.py"),
        &package.join("__init__.py"),
    )?;
    copy(&library, &package.join(format!("PyOpenColorIO{suffix}")))?;
    let pth = Path::new(&site).join(PTH);
    std::fs::write(&pth, pth_contents(&layout.py)).map_err(|e| format!("{}: {e}", pth.display()))
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("copying {} to {}: {e}", from.display(), to.display()))
}

/// The `.pth` file's text: the directory to put on the path, on a line of its own.
fn pth_contents(dir: &Path) -> String {
    format!("{}\n", dir.display())
}

/// Makes the environment and gives the variables a cargo command needs to build `ocio-py`'s
/// `python` feature against its interpreter (`PYO3_PYTHON`) and to run what links the
/// interpreter's library: its directory on `PATH` (Windows, `python313.dll`) or
/// `LD_LIBRARY_PATH` (Linux, `libpython3.13.so`), and `PYTHONHOME`, so that an interpreter
/// embedded in a test binary finds its standard library. An error, never a silent skip, when
/// the environment can't be made.
pub(crate) fn cargo_vars(
    layout: &Layout,
) -> Result<Vec<(&'static str, std::ffi::OsString)>, String> {
    sync_env(layout).map_err(|e| {
        format!(
            "ocio-py's `python` feature needs the port's Python environment {}, which \
             `uv sync` of py-port/ makes (Python 3.13 and NumPy, from uv's cache or PyPI): {e}",
            crate::display_path(&layout.env)
        )
    })?;
    let python = layout.python();
    let home = PathBuf::from(query(&python, "import sys; print(sys.base_prefix)")?);
    let (var, dir) = if cfg!(windows) {
        ("PATH", home.clone())
    } else {
        ("LD_LIBRARY_PATH", home.join("lib"))
    };
    let mut dirs = vec![dir];
    dirs.extend(std::env::var_os(var).iter().flat_map(std::env::split_paths));
    let joined = std::env::join_paths(dirs).map_err(|e| format!("{var}: {e}"))?;
    Ok(vec![
        ("PYO3_PYTHON", python.into_os_string()),
        (var, joined),
        ("PYTHONHOME", home.into_os_string()),
    ])
}

/// `uv sync` of the project `py-port/` into the environment (from uv's cache when it holds
/// Python 3.13 and NumPy already, as the oracle's environment leaves them).
fn sync_env(layout: &Layout) -> Result<(), String> {
    let status = Command::new(uv())
        .arg("sync")
        .arg("--project")
        .arg(layout.root.join("py-port"))
        .args(["--frozen", "--quiet", "--no-install-project"])
        .env("UV_PROJECT_ENVIRONMENT", &layout.env)
        .env_remove("VIRTUAL_ENV")
        .stdin(Stdio::null())
        .status()
        .map_err(|e| format!("could not run `uv` ({e}); install it: https://docs.astral.sh/uv/"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("uv sync of py-port/ failed: {status}"))
    }
}

fn uv() -> std::ffi::OsString {
    std::env::var_os("OCIO_RS_UV").unwrap_or_else(|| "uv".into())
}

/// What `code` prints in the environment's interpreter, without its line end.
fn query(python: &Path, code: &str) -> Result<String, String> {
    let out = Command::new(python)
        .args(["-I", "-c", code])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", python.display()))?;
    if !out.status.success() {
        return Err(format!(
            "{} -c {code:?}: {}",
            python.display(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

/// Builds `ocio-py` with `extension-module` against `python`; the shared library's path.
fn build_extension(root: &Path, python: &Path, release: bool) -> Result<PathBuf, String> {
    let mut cmd = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    cmd.current_dir(root)
        .args([
            "build",
            "--package",
            "ocio-py",
            "--features",
            "extension-module",
        ])
        .args(["--message-format", "json-render-diagnostics"])
        .env("PYO3_PYTHON", python)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit());
    if release {
        cmd.arg("--release");
    }
    let out = cmd.output().map_err(|e| format!("cargo build: {e}"))?;
    if !out.status.success() {
        return Err(format!("cargo build of ocio-py failed: {}", out.status));
    }
    cdylib_from_messages(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| "cargo build of ocio-py reported no shared library".to_string())
}

/// The shared library of `ocio_py` in Cargo's JSON messages (`--message-format json`): the
/// last `compiler-artifact` of that target with a `.dll` (Windows) or `.so` (Linux) file.
fn cdylib_from_messages(messages: &str) -> Option<PathBuf> {
    let mut found = None;
    for line in messages.lines() {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if message["reason"] != "compiler-artifact" || message["target"]["name"] != "ocio_py" {
            continue;
        }
        for file in message["filenames"].as_array().into_iter().flatten() {
            let Some(file) = file.as_str() else { continue };
            if file.ends_with(".dll") || file.ends_with(".so") {
                found = Some(PathBuf::from(file));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_library_is_found_in_cargos_messages() {
        let messages = [
            r#"{"reason":"compiler-artifact","target":{"name":"ocio","kind":["lib"]},"filenames":["/t/libocio.rlib"]}"#,
            "not json",
            r#"{"reason":"compiler-artifact","target":{"name":"ocio_py","kind":["cdylib","rlib"]},"filenames":["C:\\t\\ocio_py.dll","C:\\t\\ocio_py.dll.lib","C:\\t\\libocio_py.rlib"]}"#,
            r#"{"reason":"build-finished","success":true}"#,
        ]
        .join("\n");
        assert_eq!(
            cdylib_from_messages(&messages),
            Some(PathBuf::from("C:\\t\\ocio_py.dll"))
        );
        let linux = r#"{"reason":"compiler-artifact","target":{"name":"ocio_py"},"filenames":["/t/libocio_py.so","/t/libocio_py.rlib"]}"#;
        assert_eq!(
            cdylib_from_messages(linux),
            Some(PathBuf::from("/t/libocio_py.so"))
        );
        assert_eq!(
            cdylib_from_messages(&messages.replace("ocio_py", "x")),
            None
        );
    }

    #[test]
    fn the_pth_file_names_the_directory_on_a_line() {
        assert_eq!(pth_contents(Path::new("/w/target/py")), "/w/target/py\n");
    }

    #[test]
    fn the_environments_interpreter() {
        let python = python_in(Path::new("env"));
        if cfg!(windows) {
            assert_eq!(python, Path::new("env").join("Scripts").join("python.exe"));
        } else {
            assert_eq!(python, Path::new("env").join("bin").join("python"));
        }
    }
}
