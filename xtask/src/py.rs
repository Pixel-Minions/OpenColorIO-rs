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
//!
//! `cargo xtask py-suite` (chunk 6.1b) runs upstream's Python suite, unmodified, against that
//! module: each module of upstream's driver (`OpenColorIOTestSuite.py`) in a new process,
//! through the runner the oracle runs it with against the wheel
//! (`oracle/ocio_oracle/py_suite_runner.py`, O6.3), in the same conditions. Each run's
//! outcome goes to `<target>/py-suite.json`. A test function passes when the port makes every
//! run of it that the wheel makes (the oracle's py_suite of the same modules: a test a class
//! inherits runs once per class), and each passes. `py-port/passing.txt` lists the functions
//! that pass: a listed function that doesn't fails the command; `--update` rewrites the list.
//! The list only grows: `cargo xtask ci` holds its length to the base's docs/ratchet.toml.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(crate) const USAGE: &str = "\
cargo xtask py-build [--release]
    Builds the port's PyOpenColorIO (ocio-py, feature extension-module) against the Python of
    target/py-port (made from py-port/ with `uv sync` if needed), lays it out as upstream's
    package in target/py/PyOpenColorIO/, and puts target/py on target/py-port's path
    (ocio-rs-port.pth). --release: an optimized build.

cargo xtask py-suite [--update] [--release] [--no-build]
    Builds the module (as py-build; --no-build: the one in target/py), runs upstream's Python
    suite against it (each module of tests/python/OpenColorIOTestSuite.py in a new process of
    target/py-port), writes each test's outcome to target/py-suite.json, and fails when a test
    function py-port/passing.txt lists doesn't pass. --update: rewrite the list with the
    functions that pass (it may only grow).

cargo xtask py-check
    CI's step for the Python module: clippy (-D warnings) and the tests of ocio-py with its
    python feature, against target/py-port's Python, then py-suite.
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

pub(crate) fn py_suite(args: &[&str]) -> Result<(), String> {
    let (mut update, mut release, mut no_build) = (false, false, false);
    for arg in args {
        match *arg {
            "--update" => update = true,
            "--release" => release = true,
            "--no-build" => no_build = true,
            _ => return Err(format!("unknown argument `{arg}`\n\n{USAGE}")),
        }
    }
    let layout = Layout::new();
    if no_build {
        sync_env(&layout)?;
    } else {
        build(&layout, release)?;
    }
    let tests = layout.root.join("upstream/OpenColorIO/tests");
    let modules = driver_modules(&tests)?;
    let scratch = layout.py.with_file_name("py-suite-tmp");
    let mut reports = Vec::new();
    for module in &modules {
        reports.push(run_module(&layout, &tests, module, &scratch)?);
    }
    let _ = std::fs::remove_dir_all(&scratch);
    let json = layout.py.with_file_name("py-suite.json");
    let text = serde_json::to_string_pretty(&serde_json::json!({ "modules": reports }))
        .map_err(|e| e.to_string())?;
    std::fs::write(&json, text).map_err(|e| format!("{}: {e}", json.display()))?;

    let runs = runs(&reports);
    let expected = wheel_runs(&tests, &modules)?;
    let passing = passing_functions(&expected, &runs);
    let (listed, problems) = crate::parity::passing_python()?;
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    let failing: Vec<&String> = listed.iter().filter(|f| !passing.contains(*f)).collect();
    let unlisted = passing.iter().filter(|f| !listed.contains(*f)).count();
    let total = crate::parity::upstream_python_tests().len();
    println!(
        "py-suite: {} runs of {} modules; {} of {total} test functions pass, {} listed in {}; \
         outcomes in {}",
        runs.len(),
        modules.len(),
        passing.len(),
        listed.len(),
        crate::parity::PASSING_PYTHON,
        crate::display_path(&json)
    );
    if !failing.is_empty() {
        let mut why = format!(
            "{} test functions {} lists don't pass:",
            failing.len(),
            crate::parity::PASSING_PYTHON
        );
        for f in failing {
            let _ = std::fmt::Write::write_fmt(&mut why, format_args!("\n    {f}"));
            for (defined, id, outcome, message) in &runs {
                if defined.as_deref() == Some(f.as_str()) && outcome != "pass" {
                    let _ = std::fmt::Write::write_fmt(
                        &mut why,
                        format_args!("\n        {id}: {outcome} {}", message.trim_end()),
                    );
                }
            }
        }
        return Err(why);
    }
    if update {
        let path = layout.root.join(crate::parity::PASSING_PYTHON);
        let text: String = passing.iter().map(|f| format!("{f}\n")).collect();
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        println!(
            "py-suite: {} now lists {} test functions",
            crate::parity::PASSING_PYTHON,
            passing.len()
        );
    } else if unlisted > 0 {
        println!(
            "py-suite: {unlisted} test functions pass that {} doesn't list; \
             `cargo xtask py-suite --update` adds them",
            crate::parity::PASSING_PYTHON
        );
    }
    Ok(())
}

pub(crate) fn py_check(args: &[&str]) -> Result<(), String> {
    if !args.is_empty() {
        return Err(format!("unknown arguments {args:?}\n\n{USAGE}"));
    }
    let layout = Layout::new();
    let vars = cargo_vars(&layout)?;
    for step in [
        &[
            "clippy",
            "--package",
            "ocio-py",
            "--features",
            "python",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ][..],
        &[
            "test",
            "--package",
            "ocio-py",
            "--features",
            "python",
            "--no-fail-fast",
        ][..],
    ] {
        let status = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(&layout.root)
            .args(step)
            .envs(vars.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .status()
            .map_err(|e| format!("cargo {}: {e}", step[0]))?;
        if !status.success() {
            return Err(format!(
                "cargo {} of ocio-py's python feature failed: {status}",
                step[0]
            ));
        }
    }
    py_suite(&[])
}

/// The modules upstream's driver runs, in its order: the entries `("Name", Name),` of
/// `get_test_modules` in `tests/python/OpenColorIOTestSuite.py`.
fn driver_modules(tests: &Path) -> Result<Vec<String>, String> {
    let path = tests.join("python").join("OpenColorIOTestSuite.py");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let modules = modules_of_driver(&text);
    if modules.is_empty() {
        return Err(format!("{}: no test modules found", path.display()));
    }
    Ok(modules)
}

fn modules_of_driver(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let entry = line.trim().strip_prefix("(\"")?.strip_suffix("),")?;
            let (quoted, name) = entry.split_once("\", ")?;
            (quoted == name).then(|| name.to_string())
        })
        .collect()
}

/// Runs one module of the suite as the oracle's py_suite does (py_suite_api.py): the runner in
/// a new process (`-I -B -X utf8`), in an empty working directory, with an environment of only
/// its own temporary directory (TEMP, TMP, TMPDIR) and, on Windows, SYSTEMROOT. The runner's
/// report, or {"module", "crashed": {"returncode", "stderr"}} when the process died.
fn run_module(
    layout: &Layout,
    tests: &Path,
    module: &str,
    scratch: &Path,
) -> Result<serde_json::Value, String> {
    let cwd = scratch.join(module).join("cwd");
    let tmp = scratch.join(module).join("tmp");
    for dir in [&cwd, &tmp] {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut cmd = Command::new(layout.python());
    cmd.args(["-I", "-B", "-X", "utf8"])
        .arg(layout.root.join("oracle/ocio_oracle/py_suite_runner.py"))
        .arg(tests.join("python"))
        .arg(module)
        .arg(tests.join("data").join("files"))
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
        .map_err(|e| format!("{}: {e}", layout.python().display()))?;
    let _ = std::fs::remove_dir_all(scratch.join(module));
    if out.status.success()
        && let Ok(report) = serde_json::from_slice::<serde_json::Value>(&out.stdout)
    {
        return Ok(report);
    }
    Ok(serde_json::json!({
        "module": module,
        "crashed": {
            "returncode": out.status.code(),
            "stderr": String::from_utf8_lossy(&out.stderr),
        },
    }))
}

/// Each run of the reports: where its test is defined, its id, its outcome and its message.
type Run = (Option<String>, String, String, String);

fn runs(reports: &[serde_json::Value]) -> Vec<Run> {
    let mut out = Vec::new();
    for report in reports {
        for t in report["tests"].as_array().into_iter().flatten() {
            out.push((
                t["defined"].as_str().map(str::to_string),
                t["id"].as_str().unwrap_or_default().to_string(),
                t["outcome"].as_str().unwrap_or_default().to_string(),
                t["message"].as_str().unwrap_or_default().to_string(),
            ));
        }
    }
    out
}

/// The runs the wheel makes of each test function (the oracle's py_suite, O6.3, of the same
/// modules): `defined` -> the runs' ids. With the wheel, every one of upstream's 384
/// functions runs at least once.
fn wheel_runs(
    tests: &Path,
    modules: &[String],
) -> Result<std::collections::BTreeMap<String, Vec<String>>, String> {
    let tests_dir = tests.to_string_lossy().into_owned();
    let calls: Vec<ocio_testkit::oracle::BatchCall<'_>> = modules
        .iter()
        .map(|m| ocio_testkit::oracle::BatchCall {
            cmd: "py_suite",
            args: serde_json::json!({ "tests": tests_dir, "module": m }),
            blobs: Vec::new(),
        })
        .collect();
    let mut out: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (module, response) in modules
        .iter()
        .zip(ocio_testkit::Oracle::get().batch(&calls, true))
    {
        let report = response
            .map_err(|e| format!("the wheel's {module}: {e}"))?
            .result;
        if report.get("crashed").is_some() || !report["import_error"].is_null() {
            return Err(format!("the wheel's {module} didn't run: {report}"));
        }
        for t in report["tests"].as_array().into_iter().flatten() {
            if let (Some(defined), Some(id)) = (t["defined"].as_str(), t["id"].as_str()) {
                out.entry(defined.to_string())
                    .or_default()
                    .push(id.to_string());
            }
        }
    }
    let functions = crate::parity::upstream_python_tests();
    if out.len() != functions.len() || functions.iter().any(|f| !out.contains_key(f)) {
        return Err(format!(
            "the wheel's runs cover {} test functions, not upstream's {}",
            out.len(),
            functions.len()
        ));
    }
    Ok(out)
}

/// The test functions that pass: each of `expected`'s (the wheel's runs of each function),
/// every run of which the port makes, and passes, and no other run of which fails. A function
/// the port doesn't run at all, or runs fewer times (a class whose setUpClass failed, a
/// module that didn't import), doesn't pass.
fn passing_functions(
    expected: &std::collections::BTreeMap<String, Vec<String>>,
    runs: &[Run],
) -> std::collections::BTreeSet<String> {
    let mut outcomes: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
    let mut failed = std::collections::BTreeSet::new();
    for (defined, id, outcome, _) in runs {
        outcomes
            .entry(id.as_str())
            .or_default()
            .push(outcome.as_str());
        if outcome != "pass"
            && let Some(defined) = defined
        {
            failed.insert(defined.as_str());
        }
    }
    expected
        .iter()
        .filter(|(f, ids)| {
            !ids.is_empty()
                && !failed.contains(f.as_str())
                && ids.iter().all(|id| {
                    outcomes
                        .get(id.as_str())
                        .is_some_and(|o| o.iter().all(|o| *o == "pass"))
                })
        })
        .map(|(f, _)| f.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drivers_modules_are_its_entries() {
        let text = "def get_test_modules():\n    return [\n        (\"ATest\", ATest),\n        \
                    (\"BTest\", BTest),\n        (\"C\", D),\n    ]\nimport ATest\n";
        assert_eq!(modules_of_driver(text), ["ATest", "BTest"]);
    }

    #[test]
    fn a_function_passes_when_every_run_the_wheel_makes_does() {
        let run = |defined: Option<&str>, id: &str, outcome: &str| {
            (
                defined.map(str::to_string),
                id.to_string(),
                outcome.to_string(),
                String::new(),
            )
        };
        let expected: std::collections::BTreeMap<String, Vec<String>> = [
            ("A.py::A.test_a", &["A.A.test_a"][..]),
            ("B.py::Base.test_b", &["B.X.test_b", "B.Y.test_b"]),
            ("C.py::C.test_c", &["C.C.test_c"]),
            ("D.py::D.test_d", &["D.D.test_d", "D.E.test_d"]),
            // Its class's setUpClass failed: no run.
            ("F.py::F.test_f", &["F.F.test_f"]),
            // Run in one class only, where the wheel runs it in two.
            ("G.py::Base.test_g", &["G.X.test_g", "G.Y.test_g"]),
            // Never run (a module that didn't import).
            ("H.py::H.test_h", &["H.H.test_h"]),
        ]
        .into_iter()
        .map(|(f, ids)| (f.to_string(), ids.iter().map(|i| i.to_string()).collect()))
        .collect();
        let runs = [
            run(Some("A.py::A.test_a"), "A.A.test_a", "pass"),
            run(Some("B.py::Base.test_b"), "B.X.test_b", "pass"),
            run(Some("B.py::Base.test_b"), "B.Y.test_b", "fail"),
            run(Some("C.py::C.test_c"), "C.C.test_c", "skip"),
            run(Some("D.py::D.test_d"), "D.D.test_d", "pass"),
            run(Some("D.py::D.test_d"), "D.E.test_d", "pass"),
            run(None, "setUpClass (F.F)", "error"),
            run(Some("G.py::Base.test_g"), "G.X.test_g", "pass"),
        ];
        let passing: Vec<String> = passing_functions(&expected, &runs).into_iter().collect();
        assert_eq!(passing, ["A.py::A.test_a", "D.py::D.test_d"]);
    }

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
