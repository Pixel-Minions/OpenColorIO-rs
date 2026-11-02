// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `cargo xtask wheel-check`: the wheel `opencolorio-rs` as a user gets it. It builds the wheel
//! (`wheel.rs`), or takes one with `--wheel`, and then:
//! 1. reads the wheel's files (`tools/wheel-check/read_wheel.py`) and holds them to
//!    [`problems`]: exactly the expected files, the file name, `WHEEL`, `METADATA` (a plain
//!    version, a `Summary` that names the matched OCIO version), `RECORD`, the entry points
//!    against the official wheel's, and the license files against their sources;
//! 2. on Linux, proves the platform tag from the binaries (`wheel_audit.rs`);
//! 3. creates a new `uv venv` outside the source tree and installs only the wheel, then proves
//!    from inside it that the package imported is the installed one, byte for byte the wheel's
//!    (`tools/wheel-check/installed.py`, [`installed_problems`]);
//! 4. adds the pinned NumPy (`py-port/uv.lock`), which upstream's tests need and the module
//!    doesn't, and runs what `cargo xtask py-suite` runs: upstream's Python suite unmodified,
//!    every test function of which must pass, its original driver, and the package launcher
//!    compared with the official wheel;
//! 5. runs every console script the environment got, compared with the official wheel's.
//!
//! Neither source tree nor `target/py` is on the environment's path. On failure the environment
//! stays in the system's temporary directory, and the message says where.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use ocio_testkit::fixtures::sha256_hex;
use serde::Deserialize;

use crate::py::{self, Layout};
use crate::py_tools::TOOLS;
use crate::{wheel, wheel_audit};

pub(crate) const USAGE: &str = "\
cargo xtask wheel-check [--wheel <file>]
    Builds the wheel (wheel-build; --wheel: checks that file instead), checks its files and
    metadata, installs only it in a new `uv venv` outside the source tree, proves that the
    package imported is the installed one, and runs upstream's Python suite, its original
    driver, and the launcher and every console script against the official wheel. Fails if a
    required file is missing or a test fails. Never publishes anything.
";

/// The distribution name (PLAN.md D14), and its normalized form in file names.
const NAME: &str = "opencolorio-rs";
const FILE_NAME: &str = "opencolorio_rs";

/// `Requires-Python` and the wheel's Python tags: CPython 3.13 only, not `abi3` (P6-5, D14).
const REQUIRES_PYTHON: &str = "==3.13.*";
const PYTHON_TAGS: &str = "cp313-cp313";

/// The `.pth` file a new `uv venv` holds. Anything else could put a directory on the path.
const VENV_PTH: &str = "_virtualenv.pth";

/// One file of the wheel (`read_wheel.py`).
#[derive(Debug, Clone, Deserialize)]
struct Member {
    size: u64,
    mode: u32,
    sha256: String,
    record_hash: String,
}

/// What a wheel holds (`read_wheel.py`).
#[derive(Debug, Clone, Deserialize)]
struct Contents {
    /// Every file, by its path.
    members: BTreeMap<String, Member>,
    /// The `.dist-info` files directly in it, by name.
    texts: BTreeMap<String, String>,
}

/// What the wheel must be.
#[derive(Debug)]
struct Expected {
    /// The workspace's version without its local segment.
    version: String,
    /// The OCIO version the workspace's version names.
    ocio_version: String,
    /// `EXT_SUFFIX` of the interpreter.
    ext_suffix: String,
    /// `.exe` or nothing.
    exe_suffix: &'static str,
    /// Windows: no execute bits, and `win_amd64`.
    windows: bool,
    /// `__init__.py` and `command_line.py`: their SHA-256.
    sources: BTreeMap<String, String>,
    /// Each license file's path under `licenses/`, and its SHA-256.
    licenses: BTreeMap<String, String>,
    /// The official wheel's `entry_points.txt`, `console_scripts`.
    official_scripts: BTreeMap<String, String>,
}

/// The files of the package: `lay_out_package`'s, without the extension, which maturin adds.
pub(crate) fn package_files(exe_suffix: &str) -> BTreeSet<String> {
    let mut files: BTreeSet<String> = ["__init__.py", "command_line.py"]
        .map(String::from)
        .into_iter()
        .collect();
    files.extend(TOOLS.iter().map(|t| format!("bin/{t}{exe_suffix}")));
    files
}

/// The workspace's version split at its local segment: `("0.0.0", "2.5.2")`.
fn plain_and_ocio(version: &str) -> Option<(&str, &str)> {
    let (plain, local) = version.split_once('+')?;
    Some((plain, local.strip_prefix("ocio.")?))
}

fn workspace_version(root: &Path) -> Result<String, String> {
    let path = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let manifest: toml::Value = toml::from_str(&text).map_err(|e| e.to_string())?;
    manifest["workspace"]["package"]["version"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "the workspace has no version".to_string())
}

/// A release, with no pre-release, post-release, dev or local segment: what the owner's decision
/// of 2026-11-02 allows on PyPI.
fn is_plain_version(version: &str) -> bool {
    !version.is_empty()
        && version
            .split('.')
            .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The header fields of a `METADATA` or `WHEEL` file.
fn fields(text: &str) -> Vec<(&str, &str)> {
    text.lines()
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_once(": "))
        .collect()
}

/// `name = value` of `console_scripts` in an `entry_points.txt`, without spaces around `=`.
fn console_scripts(text: &str) -> BTreeMap<String, String> {
    let mut in_section = false;
    let mut scripts = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == "[console_scripts]";
        } else if in_section && let Some((name, value)) = line.split_once('=') {
            scripts.insert(name.trim().to_string(), value.trim().to_string());
        }
    }
    scripts
}

/// What is wrong with the wheel `file_name`, by its contents alone.
fn problems(file_name: &str, contents: &Contents, expected: &Expected) -> Vec<String> {
    let mut out = Vec::new();
    let version = &expected.version;
    let dist_info = format!("{FILE_NAME}-{version}.dist-info");
    let extension = format!("PyOpenColorIO{}", expected.ext_suffix);

    // The file name.
    let platform = wheel_audit::platform_of(Path::new(file_name)).unwrap_or_default();
    if file_name != format!("{FILE_NAME}-{version}-{PYTHON_TAGS}-{platform}.whl") {
        out.push(format!(
            "file name {file_name}: not {FILE_NAME}-{version}-{PYTHON_TAGS}-<platform>.whl"
        ));
    }
    let platform_ok = if expected.windows {
        platform == "win_amd64"
    } else {
        platform.starts_with("manylinux_2_") && platform.ends_with("_x86_64")
    };
    if !platform_ok {
        out.push(format!("platform tag {platform}"));
    }

    // Exactly these files.
    let mut required: BTreeSet<String> = package_files(expected.exe_suffix)
        .into_iter()
        .chain([extension.clone()])
        .map(|f| format!("PyOpenColorIO/{f}"))
        .collect();
    for name in ["METADATA", "WHEEL", "RECORD", "entry_points.txt"] {
        required.insert(format!("{dist_info}/{name}"));
    }
    for name in expected.licenses.keys() {
        required.insert(format!("{dist_info}/licenses/{name}"));
    }
    let present: BTreeSet<String> = contents.members.keys().cloned().collect();
    for missing in required.difference(&present) {
        out.push(format!("missing {missing}"));
    }
    for extra in present.difference(&required) {
        out.push(format!("unexpected {extra}"));
    }

    // Files with a known source.
    for (name, sha256) in &expected.sources {
        let path = format!("PyOpenColorIO/{name}");
        if contents
            .members
            .get(&path)
            .is_some_and(|m| m.sha256 != *sha256)
        {
            out.push(format!("{path} differs from its source"));
        }
    }
    for (name, sha256) in &expected.licenses {
        let path = format!("{dist_info}/licenses/{name}");
        if contents
            .members
            .get(&path)
            .is_some_and(|m| m.sha256 != *sha256)
        {
            out.push(format!("{path} differs from its source"));
        }
    }

    // What runs must be able to run.
    if !expected.windows {
        for tool in TOOLS {
            let path = format!("PyOpenColorIO/bin/{tool}");
            if contents
                .members
                .get(&path)
                .is_some_and(|m| m.mode & 0o111 == 0)
            {
                out.push(format!("{path} isn't executable"));
            }
        }
    }

    wheel_file_problems(contents, platform, &mut out);
    metadata_problems(contents, expected, &mut out);
    record_problems(contents, &dist_info, &mut out);

    // The entry points: the official wheel's, for exactly the four tools.
    let scripts = console_scripts(text_of(contents, "entry_points.txt"));
    let names: BTreeSet<&str> = scripts.keys().map(String::as_str).collect();
    if names != TOOLS.iter().copied().collect() {
        out.push(format!("console scripts {names:?}, not {TOOLS:?}"));
    }
    for tool in TOOLS {
        match expected.official_scripts.get(tool) {
            None => out.push(format!("the official wheel has no console script {tool}")),
            Some(official) if scripts.get(tool) != Some(official) => out.push(format!(
                "console script {tool}: {:?}, the official wheel's is {official}",
                scripts.get(tool)
            )),
            Some(_) => {}
        }
    }
    out
}

fn text_of<'a>(contents: &'a Contents, name: &str) -> &'a str {
    contents.texts.get(name).map_or("", String::as_str)
}

fn wheel_file_problems(contents: &Contents, platform: &str, out: &mut Vec<String>) {
    let wheel = fields(text_of(contents, "WHEEL"));
    let tags: Vec<&str> = wheel
        .iter()
        .filter(|(key, _)| *key == "Tag")
        .map(|(_, value)| *value)
        .collect();
    if tags != [format!("{PYTHON_TAGS}-{platform}")] {
        out.push(format!("WHEEL's tags {tags:?}, not the file name's"));
    }
    for required in [("Wheel-Version", "1.0"), ("Root-Is-Purelib", "false")] {
        if !wheel.contains(&required) {
            out.push(format!("WHEEL lacks {}: {}", required.0, required.1));
        }
    }
}

fn metadata_problems(contents: &Contents, expected: &Expected, out: &mut Vec<String>) {
    let metadata = fields(text_of(contents, "METADATA"));
    let one = |key: &str| -> Option<&str> {
        let mut values = metadata.iter().filter(|(k, _)| *k == key).map(|(_, v)| *v);
        let value = values.next();
        if values.next().is_some() { None } else { value }
    };
    for (key, value) in [
        ("Name", NAME),
        ("Version", expected.version.as_str()),
        ("Requires-Python", REQUIRES_PYTHON),
        ("License-Expression", "BSD-3-Clause"),
    ] {
        if one(key) != Some(value) {
            out.push(format!("METADATA {key}: {:?}, not {value}", one(key)));
        }
    }
    // Owner decision 2026-11-02: no local segment, whatever the workspace's version says; the
    // matched OCIO version is in Summary.
    let version = one("Version").unwrap_or_default();
    if !is_plain_version(version) {
        out.push(format!("METADATA Version {version} isn't a plain release"));
    }
    let summary = one("Summary").unwrap_or_default();
    for phrase in [
        "Rust port matching OpenColorIO",
        expected.ocio_version.as_str(),
        "Not the official",
    ] {
        if !summary.contains(phrase) {
            out.push(format!(
                "METADATA Summary {summary:?} doesn't say {phrase:?}"
            ));
        }
    }
    if metadata.iter().any(|(key, _)| *key == "Requires-Dist") {
        out.push("METADATA has dependencies; the official wheel has none".to_string());
    }
    let listed: BTreeSet<&str> = metadata
        .iter()
        .filter(|(key, _)| *key == "License-File")
        .map(|(_, value)| *value)
        .collect();
    let licenses: BTreeSet<&str> = expected.licenses.keys().map(String::as_str).collect();
    if listed != licenses {
        out.push(format!(
            "METADATA License-File lists {} files, the wheel carries {}",
            listed.len(),
            licenses.len()
        ));
    }
}

fn record_problems(contents: &Contents, dist_info: &str, out: &mut Vec<String>) {
    let own = format!("{dist_info}/RECORD");
    let mut recorded = BTreeSet::new();
    for line in text_of(contents, "RECORD").lines() {
        let mut parts = line.split(',');
        let (Some(path), Some(hash), Some(size)) = (parts.next(), parts.next(), parts.next())
        else {
            out.push(format!("RECORD line {line:?}"));
            continue;
        };
        recorded.insert(path.to_string());
        if path == own {
            continue;
        }
        match contents.members.get(path) {
            None => out.push(format!("RECORD lists {path}, which isn't in the wheel")),
            Some(member) => {
                if hash.strip_prefix("sha256=") != Some(&member.record_hash)
                    || size != member.size.to_string()
                {
                    out.push(format!("RECORD's hash or size of {path} is wrong"));
                }
            }
        }
    }
    for path in contents.members.keys() {
        if !recorded.contains(path) {
            out.push(format!("RECORD doesn't list {path}"));
        }
    }
}

/// What `installed.py` reports, with paths in [`normal`] form.
#[derive(Debug, Clone, Deserialize)]
struct Installed {
    site: String,
    package: String,
    extension: String,
    command_line: String,
    bin_dir: String,
    sys_path: Vec<String>,
    pth: Vec<String>,
    distributions: Vec<String>,
    version: String,
    python: Vec<u32>,
    ocio_version: String,
}

/// `path` as this check compares paths: real, `/`-separated, and on Windows in lower case.
fn normal(path: &str) -> String {
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    let text = crate::plain_path(&real)
        .to_string_lossy()
        .replace('\\', "/");
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

fn is_under(path: &str, dir: &str) -> bool {
    path == dir || path.starts_with(&format!("{dir}/"))
}

/// What is wrong with what the environment `env` imports: `forbidden` are the directories that
/// must not provide it, `ocio_version` is the official wheel's `__version__`.
fn installed_problems(
    report: &Installed,
    env: &str,
    forbidden: &[String],
    version: &str,
    ocio_version: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    if !is_under(&report.site, env) {
        out.push(format!("site-packages {} isn't in {env}", report.site));
    }
    let package = format!("{}/pyopencolorio", report.site.to_lowercase());
    for (what, path) in [
        ("__init__.py", &report.package),
        ("the extension", &report.extension),
        ("command_line.py", &report.command_line),
        ("bin", &report.bin_dir),
    ] {
        if !is_under(&path.to_lowercase(), &package) {
            out.push(format!("{what} comes from {path}, not from {package}"));
        }
    }
    for dir in forbidden {
        for path in report.sys_path.iter().chain([
            &report.package,
            &report.extension,
            &report.command_line,
            &report.bin_dir,
        ]) {
            if is_under(path, dir) {
                out.push(format!("{path} is in {dir}"));
            }
        }
    }
    if report.pth.iter().any(|name| name != VENV_PTH) {
        out.push(format!("site-packages has .pth files {:?}", report.pth));
    }
    if report.distributions != [NAME] {
        out.push(format!(
            "installed: {:?}, not only {NAME}",
            report.distributions
        ));
    }
    if report.version != version {
        out.push(format!(
            "installed version {}, not {version}",
            report.version
        ));
    }
    if report.python != [3, 13] {
        out.push(format!("Python {:?}, not 3.13", report.python));
    }
    if report.ocio_version != ocio_version {
        out.push(format!(
            "__version__ {}, the official wheel's is {ocio_version}",
            report.ocio_version
        ));
    }
    out
}

pub(crate) fn wheel_check(args: &[&str]) -> Result<(), String> {
    let given = match args {
        [] => None,
        ["--wheel", file] => Some(PathBuf::from(file)),
        _ => return Err(format!("unknown arguments {args:?}\n\n{USAGE}")),
    };
    let layout = Layout::new();
    let wheel = match given {
        Some(file) => {
            py::sync_env(&layout)?;
            file
        }
        None => wheel::build(&layout)?,
    };
    let scratch = std::env::temp_dir().join(format!("ocio-rs-wheel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    match check(&layout, &wheel, &scratch) {
        Ok(()) => {
            let _ = std::fs::remove_dir_all(&scratch);
            println!("wheel-check: {} passes", crate::display_path(&wheel));
            Ok(())
        }
        Err(why) => Err(format!("{why}\nwheel-check: kept {}", scratch.display())),
    }
}

fn check(layout: &Layout, wheel: &Path, scratch: &Path) -> Result<(), String> {
    let root = &layout.root;
    let env = Layout {
        root: root.clone(),
        env: scratch.join("env"),
        py: scratch.join("py"),
    };
    let forbidden = [
        normal(&root.to_string_lossy()),
        normal(&layout.py.to_string_lossy()),
    ];
    let scratch_text = normal(&scratch.to_string_lossy());
    if let Some(dir) = forbidden.iter().find(|dir| is_under(&scratch_text, dir)) {
        return Err(format!(
            "the temporary directory {scratch_text} is in {dir}"
        ));
    }

    // 1. The wheel's files.
    let contents = read_wheel(layout, wheel)?;
    let expected = expected(layout)?;
    let found = problems(
        wheel
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default(),
        &contents,
        &expected,
    );
    if !found.is_empty() {
        return Err(format!(
            "{} is not the wheel it must be:\n    {}",
            wheel.display(),
            found.join("\n    ")
        ));
    }
    println!(
        "wheel-check: {} files, {} license files, version {}",
        contents.members.len(),
        expected.licenses.len(),
        expected.version
    );

    // 2. The platform tag.
    if !cfg!(windows) {
        let audited = wheel_audit::audit(&layout.python(), wheel, &scratch.join("audit"))?;
        println!("wheel-check: {audited}");
    }

    // 3. Only the wheel, in a new environment.
    let uv = |args: &[&std::ffi::OsStr]| -> Result<(), String> {
        let status = Command::new(py::uv())
            .args(args)
            .env_remove("VIRTUAL_ENV")
            .env_remove("PYTHONPATH")
            .stdin(Stdio::null())
            .status()
            .map_err(|e| format!("could not run `uv` ({e})"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("uv {:?} failed: {status}", args[0]))
        }
    };
    uv(&[
        "venv".as_ref(),
        env.env.as_os_str(),
        "--no-project".as_ref(),
        "--python".as_ref(),
        layout.python().as_os_str(),
    ])?;
    let python = env.python();
    uv(&[
        "pip".as_ref(),
        "install".as_ref(),
        "--python".as_ref(),
        python.as_os_str(),
        "--no-index".as_ref(),
        "--no-deps".as_ref(),
        "--no-cache".as_ref(),
        wheel.as_os_str(),
    ])?;
    let installed = installed_report(&env, scratch)?;
    let oracle = ocio_testkit::Oracle::get();
    let official = py::query(
        oracle.python(),
        "import PyOpenColorIO; print(PyOpenColorIO.__version__)",
    )?;
    let found = installed_problems(
        &installed,
        &normal(&env.env.to_string_lossy()),
        &forbidden,
        &expected.version,
        &official,
    );
    if !found.is_empty() {
        return Err(format!(
            "the installed package isn't the wheel's:\n    {}",
            found.join("\n    ")
        ));
    }
    let site = Path::new(&installed.site);
    for (name, member) in contents
        .members
        .iter()
        .filter(|(name, _)| name.starts_with("PyOpenColorIO/"))
    {
        let path = site.join(name);
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if sha256_hex(&bytes) != member.sha256 {
            return Err(format!("the installed {name} isn't the wheel's"));
        }
    }
    println!(
        "wheel-check: the package is the wheel's, in {}, and imports without NumPy",
        installed.site
    );

    // 4. Upstream's suite and the launcher, as `cargo xtask py-suite`.
    let status = Command::new(py::uv())
        .arg("sync")
        .arg("--project")
        .arg(root.join("py-port"))
        .args(["--frozen", "--quiet", "--no-install-project", "--inexact"])
        .env("UV_PROJECT_ENVIRONMENT", &env.env)
        .env_remove("VIRTUAL_ENV")
        .stdin(Stdio::null())
        .status()
        .map_err(|e| format!("could not run `uv` ({e})"))?;
    if !status.success() {
        return Err(format!(
            "uv sync of NumPy into the environment failed: {status}"
        ));
    }
    let suite = py::run_suite(&env, "py-suite.json", Some("release"))?;
    py::check_listed(&suite)?;
    let total = crate::parity::upstream_python_tests().len();
    if total == 0 || suite.passing.len() != total {
        return Err(format!(
            "{} of upstream's {total} test functions pass against the installed wheel; \
             outcomes in {}",
            suite.passing.len(),
            env.py.with_file_name("py-suite.json").display()
        ));
    }
    crate::py_driver::run(&env, &suite.tests, &suite.expected)?;
    crate::py_tools::check(&env)?;

    // 5. The console scripts.
    crate::py_tools::check_console_scripts(&env)?;
    println!(
        "wheel-check: {} of {total} test functions pass against the installed wheel",
        suite.passing.len()
    );
    Ok(())
}

fn read_wheel(layout: &Layout, wheel: &Path) -> Result<Contents, String> {
    let script = layout.root.join("tools/wheel-check/read_wheel.py");
    let out = Command::new(layout.python())
        .arg("-I")
        .arg(&script)
        .arg(wheel)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", script.display()))?;
    if !out.status.success() {
        return Err(format!(
            "{}: {}",
            wheel.display(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("{}: {e}", wheel.display()))
}

fn expected(layout: &Layout) -> Result<Expected, String> {
    let root = &layout.root;
    let workspace = workspace_version(root)?;
    let (version, ocio_version) = plain_and_ocio(&workspace)
        .ok_or_else(|| format!("the workspace's version {workspace} names no OCIO version"))?;
    let sha256_of = |path: &Path| -> Result<String, String> {
        std::fs::read(path)
            .map(|bytes| sha256_hex(&bytes))
            .map_err(|e| format!("{}: {e}", path.display()))
    };
    let mut sources = BTreeMap::new();
    for name in ["__init__.py", "command_line.py"] {
        let path = root.join("crates/ocio-py/python/PyOpenColorIO").join(name);
        sources.insert(name.to_string(), sha256_of(&path)?);
    }
    let mut licenses = BTreeMap::new();
    for (name, path) in wheel::license_files(root)? {
        licenses.insert(name, sha256_of(&path)?);
    }
    let oracle = ocio_testkit::Oracle::get();
    let official = py::query(
        oracle.python(),
        "import importlib.metadata as m; \
         print(m.distribution('opencolorio').read_text('entry_points.txt'))",
    )?;
    Ok(Expected {
        version: version.to_string(),
        ocio_version: ocio_version.to_string(),
        ext_suffix: py::query(
            &layout.python(),
            "import sysconfig; print(sysconfig.get_config_var('EXT_SUFFIX'))",
        )?,
        exe_suffix: std::env::consts::EXE_SUFFIX,
        windows: cfg!(windows),
        sources,
        licenses,
        official_scripts: console_scripts(&official),
    })
}

/// Runs `installed.py` in the environment, as isolated as the suite's own runs.
fn installed_report(env: &Layout, scratch: &Path) -> Result<Installed, String> {
    let cwd = scratch.join("import-cwd");
    std::fs::create_dir_all(&cwd).map_err(|e| format!("{}: {e}", cwd.display()))?;
    let mut cmd = Command::new(env.python());
    cmd.arg("-I")
        .arg("-B")
        .arg(env.root.join("tools/wheel-check/installed.py"))
        .current_dir(&cwd)
        .env_clear()
        .stdin(Stdio::null());
    if let Some(root) = std::env::var_os("SYSTEMROOT") {
        cmd.env("SYSTEMROOT", root);
    }
    let out = cmd.output().map_err(|e| format!("{e}"))?;
    if !out.status.success() {
        return Err(format!(
            "importing the installed wheel failed: {}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let mut report: Installed =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("installed.py: {e}"))?;
    report.site = normal(&report.site);
    for path in [
        &mut report.package,
        &mut report.extension,
        &mut report.command_line,
        &mut report.bin_dir,
    ] {
        *path = normal(path);
    }
    report.sys_path = report.sys_path.iter().map(|p| normal(p)).collect();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIST_INFO: &str = "opencolorio_rs-0.0.0.dist-info";

    fn sha(name: &str) -> String {
        sha256_hex(name.as_bytes())
    }

    fn expected() -> Expected {
        Expected {
            version: "0.0.0".to_string(),
            ocio_version: "2.5.2".to_string(),
            ext_suffix: ".cpython-313-x86_64-linux-gnu.so".to_string(),
            exe_suffix: "",
            windows: false,
            sources: ["__init__.py", "command_line.py"]
                .map(|n| (n.to_string(), sha(n)))
                .into_iter()
                .collect(),
            licenses: ["LICENSE", "crate-licenses/half-2.7.1/LICENSE-MIT"]
                .map(|n| (n.to_string(), sha(n)))
                .into_iter()
                .collect(),
            official_scripts: TOOLS
                .map(|t| (t.to_string(), "PyOpenColorIO.command_line:main".to_string()))
                .into_iter()
                .collect(),
        }
    }

    const NAME_OK: &str = "opencolorio_rs-0.0.0-cp313-cp313-manylinux_2_34_x86_64.whl";

    /// A wheel with nothing wrong: [`expected`]'s, in the form `read_wheel.py` gives.
    fn good() -> Contents {
        let mut files: Vec<(String, u32)> = vec![
            ("PyOpenColorIO/__init__.py".into(), 0o644),
            ("PyOpenColorIO/command_line.py".into(), 0o644),
            (
                "PyOpenColorIO/PyOpenColorIO.cpython-313-x86_64-linux-gnu.so".into(),
                0o755,
            ),
            (format!("{DIST_INFO}/METADATA"), 0o644),
            (format!("{DIST_INFO}/WHEEL"), 0o644),
            (format!("{DIST_INFO}/entry_points.txt"), 0o644),
            (format!("{DIST_INFO}/licenses/LICENSE"), 0o644),
            (
                format!("{DIST_INFO}/licenses/crate-licenses/half-2.7.1/LICENSE-MIT"),
                0o644,
            ),
        ];
        files.extend(TOOLS.map(|t| (format!("PyOpenColorIO/bin/{t}"), 0o755)));
        let members: BTreeMap<String, Member> = files
            .into_iter()
            .map(|(path, mode)| {
                let hash = path.split_once("/licenses/").map_or_else(
                    || path.rsplit('/').next().unwrap().to_string(),
                    |(_, name)| name.to_string(),
                );
                let member = Member {
                    size: 1,
                    mode,
                    sha256: sha(&hash),
                    record_hash: format!("hash-of-{path}"),
                };
                (path, member)
            })
            .collect();
        let mut contents = Contents {
            members,
            texts: BTreeMap::new(),
        };
        rewrite_record(&mut contents);
        contents.members.insert(
            format!("{DIST_INFO}/RECORD"),
            Member {
                size: 1,
                mode: 0o644,
                sha256: sha("record"),
                record_hash: String::new(),
            },
        );
        for (name, text) in [
            (
                "METADATA",
                "Metadata-Version: 2.4\nName: opencolorio-rs\nVersion: 0.0.0\n\
                 License-File: LICENSE\nLicense-File: crate-licenses/half-2.7.1/LICENSE-MIT\n\
                 Summary: A Rust port matching OpenColorIO 2.5.2, with its PyOpenColorIO \
                 module. Not the official OpenColorIO package.\n\
                 License-Expression: BSD-3-Clause\nRequires-Python: ==3.13.*\n\n# readme\n\
                 Version: not a field\n",
            ),
            (
                "WHEEL",
                "Wheel-Version: 1.0\nGenerator: maturin (1.15.0)\nRoot-Is-Purelib: false\n\
                 Tag: cp313-cp313-manylinux_2_34_x86_64\n",
            ),
            (
                "entry_points.txt",
                "[console_scripts]\nociobakelut=PyOpenColorIO.command_line:main\n\
                 ociocheck=PyOpenColorIO.command_line:main\n\
                 ociochecklut=PyOpenColorIO.command_line:main\n\
                 ociowrite=PyOpenColorIO.command_line:main\n",
            ),
        ] {
            contents.texts.insert(name.to_string(), text.to_string());
        }
        contents
    }

    fn rewrite_record(contents: &mut Contents) {
        let mut record = String::new();
        for (path, member) in &contents.members {
            if path != &format!("{DIST_INFO}/RECORD") {
                record.push_str(&format!(
                    "{path},sha256={},{}\n",
                    member.record_hash, member.size
                ));
            }
        }
        record.push_str(&format!("{DIST_INFO}/RECORD,,\n"));
        contents.texts.insert("RECORD".to_string(), record);
    }

    /// The first problem of `good()` after `change`, which must have exactly the problems that
    /// contain `fragment`, at least one.
    fn assert_problem(change: impl Fn(&mut Contents), fragment: &str) {
        let mut contents = good();
        change(&mut contents);
        let found = problems(NAME_OK, &contents, &expected());
        assert!(
            found.iter().any(|p| p.contains(fragment)),
            "{fragment}: {found:?}"
        );
    }

    #[test]
    fn a_good_wheel_has_no_problems() {
        assert_eq!(
            problems(NAME_OK, &good(), &expected()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_required_file_may_not_go_missing() {
        let contents = good();
        for path in contents.members.keys() {
            let mut without = contents.clone();
            without.members.remove(path);
            rewrite_record(&mut without);
            let found = problems(NAME_OK, &without, &expected());
            assert!(
                found.contains(&format!("missing {path}")),
                "{path}: {found:?}"
            );
        }
        assert_eq!(contents.members.len(), 13);
    }

    #[test]
    fn an_unexpected_file_is_refused() {
        assert_problem(
            |c| {
                let member = c.members["PyOpenColorIO/__init__.py"].clone();
                c.members
                    .insert("PyOpenColorIO/bin/ocioconvert".to_string(), member);
            },
            "unexpected PyOpenColorIO/bin/ocioconvert",
        );
    }

    #[test]
    fn the_file_name_must_be_the_wheels() {
        for (name, fragment) in [
            (
                "opencolorio_rs-0.0.0-cp313-cp313-linux_x86_64.whl",
                "platform tag",
            ),
            (
                "opencolorio_rs-0.0.0-cp313-abi3-manylinux_2_34_x86_64.whl",
                "file name",
            ),
            (
                "opencolorio_rs-0.0.0-cp314-cp314-manylinux_2_34_x86_64.whl",
                "file name",
            ),
            (
                "opencolorio-0.0.0-cp313-cp313-manylinux_2_34_x86_64.whl",
                "file name",
            ),
            (
                "opencolorio_rs-0.0.0+ocio.2.5.2-cp313-cp313-manylinux_2_34_x86_64.whl",
                "file name",
            ),
            (
                "opencolorio_rs-0.0.0-cp313-cp313-win_amd64.whl",
                "platform tag",
            ),
        ] {
            let found = problems(name, &good(), &expected());
            assert!(
                found.iter().any(|p| p.contains(fragment)),
                "{name}: {found:?}"
            );
        }
        // And on Windows only win_amd64.
        let windows = Expected {
            windows: true,
            ..expected()
        };
        let found = problems(NAME_OK, &good(), &windows);
        assert!(
            found.iter().any(|p| p.contains("platform tag")),
            "{found:?}"
        );
    }

    #[test]
    fn wheel_files_tags_must_be_the_file_names() {
        assert_problem(
            |c| {
                c.texts.insert(
                    "WHEEL".into(),
                    "Wheel-Version: 1.0\nRoot-Is-Purelib: false\nTag: py3-none-any\n".into(),
                );
            },
            "WHEEL's tags",
        );
        assert_problem(
            |c| {
                let text =
                    c.texts["WHEEL"].replace("Root-Is-Purelib: false", "Root-Is-Purelib: true");
                c.texts.insert("WHEEL".into(), text);
            },
            "Root-Is-Purelib",
        );
    }

    fn with_metadata(from: &'static str, to: &'static str) -> impl Fn(&mut Contents) {
        move |c: &mut Contents| {
            let text = c.texts["METADATA"].replace(from, to);
            assert_ne!(text, c.texts["METADATA"], "{from}");
            c.texts.insert("METADATA".into(), text);
        }
    }

    #[test]
    fn the_version_has_no_local_segment_and_summary_names_the_ocio_version() {
        // The check doesn't lean on `expected`: a local segment fails even if it were expected.
        let local = Expected {
            version: "0.0.0+ocio.2.5.2".to_string(),
            ..expected()
        };
        let mut contents = good();
        with_metadata("Version: 0.0.0", "Version: 0.0.0+ocio.2.5.2")(&mut contents);
        let found = problems(NAME_OK, &contents, &local);
        assert!(
            found.iter().any(|p| p.contains("isn't a plain release")),
            "{found:?}"
        );
        assert_problem(
            with_metadata("OpenColorIO 2.5.2", "OpenColorIO"),
            "doesn't say \"2.5.2\"",
        );
        assert_problem(
            with_metadata("OpenColorIO 2.5.2", "OpenColorIO 2.5.1"),
            "doesn't say \"2.5.2\"",
        );
        assert_problem(
            with_metadata("Not the official", "The official"),
            "doesn't say \"Not the official\"",
        );
        assert_problem(
            with_metadata("A Rust port matching", "A build of"),
            "Rust port",
        );
    }

    #[test]
    fn metadata_fields_are_held_to_their_values() {
        for (from, to, key) in [
            ("Name: opencolorio-rs", "Name: opencolorio", "METADATA Name"),
            ("Version: 0.0.0\n", "Version: 0.0.1\n", "METADATA Version"),
            (
                "Requires-Python: ==3.13.*",
                "Requires-Python: >=3.13",
                "METADATA Requires-Python",
            ),
            (
                "License-Expression: BSD-3-Clause",
                "License-Expression: MIT",
                "METADATA License-Expression",
            ),
            ("License-File: LICENSE\n", "", "License-File lists"),
            (
                "Name: opencolorio-rs\n",
                "Name: opencolorio-rs\nRequires-Dist: numpy\n",
                "dependencies",
            ),
        ] {
            assert_problem(with_metadata(from, to), key);
        }
    }

    #[test]
    fn plain_versions() {
        for good in ["0.0.0", "1.0.0", "1.0.1", "12.34"] {
            assert!(is_plain_version(good), "{good}");
        }
        for bad in [
            "",
            "1.0.0+ocio.2.5.2",
            "1.0.0rc1",
            "1.0.0.post1",
            "1.0.dev0",
            "1..0",
            "v1",
        ] {
            assert!(!is_plain_version(bad), "{bad}");
        }
    }

    #[test]
    fn the_workspace_version_splits_at_its_local_segment() {
        assert_eq!(plain_and_ocio("0.0.0+ocio.2.5.2"), Some(("0.0.0", "2.5.2")));
        assert_eq!(plain_and_ocio("1.0.0+ocio.2.6.1"), Some(("1.0.0", "2.6.1")));
        assert_eq!(plain_and_ocio("1.0.0"), None);
        assert_eq!(plain_and_ocio("1.0.0+other"), None);
    }

    #[test]
    fn record_must_match_the_wheel() {
        assert_problem(
            |c| {
                let first = "PyOpenColorIO/__init__.py";
                c.members.get_mut(first).unwrap().record_hash = "other".to_string();
            },
            "hash or size of PyOpenColorIO/__init__.py",
        );
        assert_problem(
            |c| {
                let first = "PyOpenColorIO/__init__.py";
                c.members.get_mut(first).unwrap().size = 2;
            },
            "hash or size of PyOpenColorIO/__init__.py",
        );
        assert_problem(
            |c| {
                let text = c.texts["RECORD"]
                    .lines()
                    .filter(|l| !l.starts_with("PyOpenColorIO/bin/ociocheck,"))
                    .map(|l| format!("{l}\n"))
                    .collect();
                c.texts.insert("RECORD".into(), text);
            },
            "RECORD doesn't list PyOpenColorIO/bin/ociocheck",
        );
        assert_problem(
            |c| {
                let text = format!("{}nothing/here,sha256=x,1\n", c.texts["RECORD"]);
                c.texts.insert("RECORD".into(), text);
            },
            "RECORD lists nothing/here",
        );
    }

    #[test]
    fn files_with_a_source_must_be_identical_to_it() {
        assert_problem(
            |c| {
                c.members
                    .get_mut("PyOpenColorIO/command_line.py")
                    .unwrap()
                    .sha256 = sha("edited");
            },
            "PyOpenColorIO/command_line.py differs",
        );
        assert_problem(
            |c| {
                let license = format!("{DIST_INFO}/licenses/LICENSE");
                c.members.get_mut(&license).unwrap().sha256 = sha("edited");
            },
            "licenses/LICENSE differs",
        );
    }

    #[test]
    fn a_tool_without_execute_bits_is_refused_on_linux_only() {
        let change = |c: &mut Contents| {
            c.members
                .get_mut("PyOpenColorIO/bin/ociowrite")
                .unwrap()
                .mode = 0o644;
        };
        assert_problem(change, "PyOpenColorIO/bin/ociowrite isn't executable");
    }

    #[test]
    fn the_console_scripts_are_the_official_wheels_four() {
        let entry = |text: &'static str| {
            move |c: &mut Contents| {
                c.texts.insert("entry_points.txt".into(), text.into());
            }
        };
        assert_problem(
            entry(
                "[console_scripts]\nociobakelut=PyOpenColorIO.command_line:main\n\
                 ociocheck=PyOpenColorIO.command_line:main\n\
                 ociochecklut=PyOpenColorIO.command_line:main\n",
            ),
            "console scripts",
        );
        assert_problem(
            entry(
                "[console_scripts]\nociobakelut=PyOpenColorIO.command_line:main\n\
                 ociocheck=PyOpenColorIO.command_line:other\n\
                 ociochecklut=PyOpenColorIO.command_line:main\n\
                 ociowrite=PyOpenColorIO.command_line:main\n",
            ),
            "the official wheel's is",
        );
        // The check can't pass on an official side that is empty.
        let empty = Expected {
            official_scripts: BTreeMap::new(),
            ..expected()
        };
        let found = problems(NAME_OK, &good(), &empty);
        assert!(
            found.iter().any(|p| p.contains("has no console script")),
            "{found:?}"
        );
    }

    #[test]
    fn console_scripts_are_read_from_their_section_with_spaces_or_without() {
        let scripts = console_scripts(
            "[gui_scripts]\nb = x:y\n\n[console_scripts]\na = PyOpenColorIO.command_line:main\n\
             c=d:e\n",
        );
        assert_eq!(scripts.len(), 2);
        assert_eq!(scripts["a"], "PyOpenColorIO.command_line:main");
        assert_eq!(scripts["c"], "d:e");
    }

    #[test]
    fn fields_stop_at_the_blank_line() {
        let found = fields("Name: a\nVersion: 1\n\nVersion: 2\n");
        assert_eq!(found, [("Name", "a"), ("Version", "1")]);
    }

    fn installed() -> Installed {
        let site = "/tmp/x/env/lib/python3.13/site-packages".to_string();
        Installed {
            package: format!("{site}/PyOpenColorIO/__init__.py"),
            extension: format!("{site}/PyOpenColorIO/PyOpenColorIO.so"),
            command_line: format!("{site}/PyOpenColorIO/command_line.py"),
            bin_dir: format!("{site}/PyOpenColorIO/bin"),
            sys_path: vec!["/opt/python/lib".to_string(), site.clone()],
            pth: vec![VENV_PTH.to_string()],
            distributions: vec![NAME.to_string()],
            version: "0.0.0".to_string(),
            python: vec![3, 13],
            ocio_version: "2.5.2".to_string(),
            site,
        }
    }

    fn installed_found(report: &Installed) -> Vec<String> {
        installed_problems(
            report,
            "/tmp/x/env",
            &["/work".to_string(), "/work/target/py".to_string()],
            "0.0.0",
            "2.5.2",
        )
    }

    #[test]
    fn the_installed_package_must_come_from_the_new_environment() {
        assert_eq!(installed_found(&installed()), Vec::<String>::new());
        type Case = (fn(&mut Installed), &'static str);
        let cases: [Case; 10] = [
            (
                |r| r.package = "/work/target/py/PyOpenColorIO/__init__.py".into(),
                "__init__.py comes",
            ),
            (
                |r| r.extension = "/work/target/py/PyOpenColorIO/x.so".into(),
                "the extension comes",
            ),
            (
                |r| r.command_line = "/elsewhere/command_line.py".into(),
                "command_line.py comes",
            ),
            (|r| r.bin_dir = "/elsewhere/bin".into(), "bin comes"),
            (|r| r.sys_path.push("/work/target/py".into()), "is in /work"),
            (|r| r.pth.push("ocio-rs-port.pth".into()), ".pth files"),
            (|r| r.distributions.push("numpy".into()), "not only"),
            (
                |r| r.version = "0.0.0+ocio.2.5.2".into(),
                "installed version",
            ),
            (|r| r.python = vec![3, 14], "not 3.13"),
            (|r| r.ocio_version = "0.0.0".into(), "official wheel's is"),
        ];
        for (change, fragment) in cases {
            let mut report = installed();
            change(&mut report);
            let found = installed_found(&report);
            assert!(
                found.iter().any(|p| p.contains(fragment)),
                "{fragment}: {found:?}"
            );
        }
        let mut report = installed();
        report.site = "/work/site".into();
        assert!(!installed_found(&report).is_empty());
    }

    /// Both required checks wait for the wheel's job, and no workflow can publish it.
    #[test]
    fn ci_checks_the_wheel_on_both_platforms_and_publishes_nothing() {
        let workflows = crate::root().join(".github/workflows");
        let mut wheel_checks = 0;
        for entry in std::fs::read_dir(&workflows).unwrap() {
            let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
            wheel_checks += text.matches("cargo xtask wheel-check").count();
            for command in [
                "uv publish",
                "maturin publish",
                "twine",
                "PYPI_",
                "pypi-token",
            ] {
                assert!(!text.contains(command), "{command}");
            }
        }
        assert_eq!(wheel_checks, 2);
    }

    #[test]
    fn paths_are_under_a_directory_only_at_a_separator() {
        assert!(is_under("/work/a", "/work"));
        assert!(is_under("/work", "/work"));
        assert!(!is_under("/workspace/a", "/work"));
    }
}
