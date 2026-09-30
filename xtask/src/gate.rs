// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `cargo xtask gate`: the check a chunk passes before it is committed (CLAUDE.md "Chunks").
//!
//! Every step is a child process whose exit status is checked. Its full output goes to
//! `target/gate-logs/<UTC time>/<step>.log`; the gate prints one summary line per step and stops
//! at the first failure, with a non-zero exit code.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) const USAGE: &str = "\
cargo xtask gate [--staged] [--crates a,b] [--release] [--rocky] [--quick|--full] [--main]
                 [--auto]

Runs, in order, stopping at the first failure:
  fmt            cargo fmt --all --check
  clippy         cargo clippy --workspace --all-targets -- -D warnings
  ci             cargo xtask ci (branch mode; with --main, cargo xtask ci --main)
  deny           with --main: cargo deny check, CI's third check (not in the Rocky pass)
  test           cargo test --workspace --no-fail-fast (debug)
  test-release   the same in release, with --release
  rocky          with --rocky: this gate, same options, in Rocky Linux 9 (scripts/rocky9.sh)

  --staged       gate exactly what is staged (`git add`), leaving unstaged changes and
                 untracked files out: the index as a temporary commit on HEAD, checked out
                 in target/gate-staged/wt (kept, with its logs, until the next --staged run)
  --crates a,b   run the test steps for these packages only (fmt, clippy and ci always cover
                 the workspace)
  --quick        tests run the quick tier (OCIO_RS_TIER=quick): the default, per chunk
  --full         tests run the full tier (OCIO_RS_TIER=full), as `xtask land` does
  --main         check what `main` requires: the ci step also checks that
                 docs/ratchet.toml and docs/parity.md are current, and cargo-deny runs (it
                 must be installed: `cargo install cargo-deny --locked`); `xtask land` uses it
                 on the merge commit
  --auto         for `xtask land`: add --release, and keep --rocky, only when the HEAD commit
                 touches platform-sensitive files (Rust sources, the oracle, fixtures, scripts)

The ci step's ratchet baseline is docs/ratchet.toml where HEAD left phase0 (their merge base;
else origin/main's), saved as base-ratchet.toml next to the logs for the Rocky pass too.

Logs: target/gate-logs/<UTC time>/<step>.log (the Rocky steps in .../rocky/).
";

/// What the gate runs.
#[derive(Debug, Default, Clone)]
pub(crate) struct Options {
    /// Packages the test steps are limited to (empty: the whole workspace).
    pub(crate) crates: Vec<String>,
    pub(crate) release: bool,
    pub(crate) rocky: bool,
    /// The full test tier instead of the quick one.
    pub(crate) full: bool,
    /// `xtask ci --main` instead of branch mode.
    pub(crate) main: bool,
    /// Decide `release` and `rocky` from the files the HEAD commit touches.
    pub(crate) auto: bool,
    /// Write the step logs here instead of a new `target/gate-logs/<time>` (used for the Rocky
    /// pass, which writes next to the Windows logs). Relative paths are relative to the root.
    pub(crate) log_dir: Option<PathBuf>,
    /// The ratchet baseline's file (the Rocky pass gets the one the host gate saved).
    pub(crate) base_file: Option<PathBuf>,
    /// Gate the staged changes in a scratch worktree.
    pub(crate) staged: bool,
    /// The arguments other than `--staged`, for the gate `--staged` runs.
    pub(crate) others: Vec<String>,
}

pub(crate) fn parse(args: &[&str]) -> Result<Options, String> {
    let mut opts = Options::default();
    let mut tier = None;
    opts.others = args
        .iter()
        .filter(|a| **a != "--staged")
        .map(|a| a.to_string())
        .collect();
    let mut it = args.iter();
    while let Some(&arg) = it.next() {
        let mut value = |name: &str| -> Result<String, String> {
            match arg.strip_prefix(name).and_then(|v| v.strip_prefix('=')) {
                Some(v) => Ok(v.to_string()),
                None => it
                    .next()
                    .map(|v| v.to_string())
                    .ok_or_else(|| format!("{name} needs a value")),
            }
        };
        match arg {
            "--release" => opts.release = true,
            "--rocky" => opts.rocky = true,
            "--main" => opts.main = true,
            "--auto" => opts.auto = true,
            "--staged" => opts.staged = true,
            "--quick" | "--full" => {
                if tier.is_some_and(|t| t != arg) {
                    return Err("--quick and --full are exclusive".into());
                }
                tier = Some(arg);
                opts.full = arg == "--full";
            }
            _ if arg == "--crates" || arg.starts_with("--crates=") => {
                let list = value("--crates")?;
                opts.crates.extend(
                    list.split(',')
                        .map(str::trim)
                        .filter(|c| !c.is_empty())
                        .map(String::from),
                );
            }
            _ if arg == "--log-dir" || arg.starts_with("--log-dir=") => {
                opts.log_dir = Some(PathBuf::from(value("--log-dir")?));
            }
            _ if arg == "--base-file" || arg.starts_with("--base-file=") => {
                opts.base_file = Some(PathBuf::from(value("--base-file")?));
            }
            _ => return Err(format!("unknown gate option `{arg}`\n\n{USAGE}")),
        }
    }
    Ok(opts)
}

pub(crate) fn run(mut opts: Options) -> Result<(), String> {
    let root = crate::root();
    if opts.staged {
        return run_staged(&root, &opts.others);
    }
    let packages = workspace_packages(&root)?;
    for name in &opts.crates {
        if !packages.contains(name) {
            return Err(format!(
                "--crates: `{name}` is not a workspace package ({})",
                packages.join(", ")
            ));
        }
    }
    let nested = opts.log_dir.is_some();
    if opts.auto {
        resolve_auto(&root, &mut opts)?;
    }
    // cargo-deny is CI's third check; the Rocky image doesn't have it, and CI runs it once.
    let deny = opts.main && !nested;
    if deny {
        let found = cargo(&root, &["deny", "--version"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if !found {
            return Err(
                "cargo-deny is not installed, and `--main` runs it (it is CI's cargo-deny \
                 check): `cargo install cargo-deny --locked`"
                    .into(),
            );
        }
    }
    let log_dir = match &opts.log_dir {
        Some(dir) if dir.is_absolute() => dir.clone(),
        Some(dir) => root.join(dir),
        None => new_log_dir(&root.join("target").join("gate-logs"))?,
    };
    std::fs::create_dir_all(&log_dir).map_err(|e| format!("{}: {e}", log_dir.display()))?;
    if !nested {
        println!("gate: logs in {}", crate::display_path(&log_dir));
    }
    let base_file = match opts.base_file.clone() {
        Some(file) if file.is_absolute() => Some(file),
        Some(file) => Some(root.join(file)),
        None => save_ratchet_base(&root, &log_dir)?,
    };

    let mut runner = Runner::new(log_dir.clone());
    runner.watch = LandWatch::from_env()?;
    runner.step(
        "fmt",
        cargo(&root, &["fmt", "--all", "--check", "--", "--color=never"]),
        Kind::Plain,
    )?;
    runner.step(
        "clippy",
        cargo(
            &root,
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        Kind::Plain,
    )?;
    let mut ci = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    ci.arg("ci").current_dir(&root);
    if opts.main {
        ci.arg("--main");
    }
    if let Some(file) = &base_file {
        ci.arg("--base-file").arg(file);
    }
    runner.step("ci", ci, Kind::Plain)?;
    if deny {
        runner.step(
            "deny",
            cargo(&root, &["deny", "--color", "never", "check"]),
            Kind::Plain,
        )?;
    }
    let tier = if opts.full { "full" } else { "quick" };
    runner.step(
        "test",
        test_command(&root, &opts.crates, false, tier),
        Kind::Tests,
    )?;
    if opts.release {
        runner.step(
            "test-release",
            test_command(&root, &opts.crates, true, tier),
            Kind::Tests,
        )?;
    }
    if opts.rocky {
        let mut rocky = Command::new(bash()?);
        rocky
            .current_dir(&root)
            .args(["scripts/rocky9.sh", "cargo", "xtask", "gate"]);
        if !opts.crates.is_empty() {
            rocky.args(["--crates", &opts.crates.join(",")]);
        }
        if opts.release {
            rocky.arg("--release");
        }
        if opts.main {
            rocky.arg("--main");
        }
        rocky.arg(if opts.full { "--full" } else { "--quick" });
        rocky
            .arg("--log-dir")
            .arg(crate::display_path(&log_dir.join("rocky")));
        if let Some(file) = &base_file {
            // Relative to the checkout, which the container mounts at /work.
            rocky.arg("--base-file").arg(crate::display_path(file));
        }
        runner.step("rocky", rocky, Kind::Nested)?;
    }
    if !nested {
        println!(
            "gate: pass, {} steps in {}",
            runner.steps,
            duration(runner.started.elapsed())
        );
    }
    Ok(())
}

/// `--staged`: gates exactly the index. It becomes a temporary commit on HEAD (on no branch),
/// checked out in `target/gate-staged/wt`, where `cargo xtask gate <others>` runs; the checkout
/// itself, with its unstaged changes and untracked files, is not touched. Safe beside other
/// agents, unlike `git stash`, whose `refs/stash` every worktree shares. The worktree stays,
/// with its logs, until the next `--staged` run in this checkout.
fn run_staged(root: &Path, others: &[String]) -> Result<(), String> {
    let dir = root.join("target").join("gate-staged");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let lock = crate::land::lock_file(&dir.join("lock"))?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            return Err("another `cargo xtask gate --staged` is running in this checkout".into());
        }
        Err(std::fs::TryLockError::Error(e)) => return Err(format!("gate-staged lock: {e}")),
    }
    // `git write-tree` stores a cache tree in the index it reads (under index.lock): let it
    // read a copy, so that the checkout's own index is never written.
    let index = crate::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )?;
    let copy = dir.join("index");
    std::fs::copy(index.trim(), &copy).map_err(|e| format!("{}: {e}", index.trim()))?;
    let out = crate::clear_git_env(&mut Command::new("git"))
        .arg("-C")
        .arg(root)
        .arg("write-tree")
        .env("GIT_INDEX_FILE", &copy)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run git write-tree: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git write-tree failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let tree = String::from_utf8_lossy(&out.stdout).into_owned();
    let head = crate::git(root, &["rev-parse", "--verify", "HEAD"])?;
    let out = crate::clear_git_env(&mut Command::new("git"))
        .arg("-C")
        .arg(root)
        .args(["commit-tree", tree.trim(), "-p", head.trim()])
        .args(["-m", "cargo xtask gate --staged"])
        .env("GIT_AUTHOR_NAME", "cargo xtask gate")
        .env("GIT_AUTHOR_EMAIL", "gate@localhost")
        .env("GIT_COMMITTER_NAME", "cargo xtask gate")
        .env("GIT_COMMITTER_EMAIL", "gate@localhost")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run git commit-tree: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git commit-tree failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let commit = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let staged = crate::git(root, &["diff", "--cached", "--name-only"])?;
    let wt = dir.join("wt");
    crate::land::remove_worktree(root, &wt)?;
    crate::land::add_worktree(root, &wt, &commit)?;
    println!(
        "gate: the staged changes ({} files) on {}, checked out in {}",
        staged.lines().count(),
        &head.trim()[..head.trim().len().min(10)],
        crate::display_path(&wt)
    );
    let mut cmd = cargo(&wt, &["xtask", "gate"]);
    cmd.args(others)
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .env_remove("CARGO_TERM_COLOR");
    let status = crate::clear_git_env(&mut cmd)
        .status()
        .map_err(|e| format!("could not run the gate: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "the gate failed on the staged changes; its logs are in {}/target/gate-logs/",
            crate::display_path(&wt)
        ))
    }
}

/// Saves `docs/ratchet.toml` as it is where HEAD left phase0 (their merge base), or else
/// origin/main, to `<log_dir>/base-ratchet.toml`: the ci step's ratchet baseline, which a
/// branch can't lower by editing its own copy. None, with a note, when neither exists.
fn save_ratchet_base(root: &Path, log_dir: &Path) -> Result<Option<PathBuf>, String> {
    for (branch, what) in [
        ("refs/heads/phase0", "where HEAD left phase0"),
        ("refs/remotes/origin/main", "where HEAD left origin/main"),
    ] {
        if crate::git(root, &["rev-parse", "--verify", "--quiet", branch]).is_err() {
            continue;
        }
        let base = crate::git(root, &["merge-base", "HEAD", branch])?;
        let base = base.trim();
        let Ok(text) = crate::git(root, &["show", &format!("{base}:docs/ratchet.toml")]) else {
            println!("gate: {base} ({what}) has no docs/ratchet.toml");
            continue;
        };
        let file = log_dir.join("base-ratchet.toml");
        std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
        println!(
            "gate: ratchet baseline: docs/ratchet.toml at {} ({what})",
            &base[..base.len().min(10)]
        );
        return Ok(Some(file));
    }
    println!(
        "gate: no phase0 or origin/main here, so the ratchet baseline is this checkout's own \
         docs/ratchet.toml"
    );
    Ok(None)
}

/// Sets `release`, and keeps `rocky`, only when HEAD touches platform-sensitive files.
fn resolve_auto(root: &Path, opts: &mut Options) -> Result<(), String> {
    let files = crate::git(
        root,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            "--root",
            "-m",
            "--first-parent",
            "HEAD",
        ],
    )?;
    let subject = crate::git(root, &["log", "-1", "--format=%h %s", "HEAD"])?;
    let sensitive: Vec<&str> = files
        .lines()
        .filter(|f| platform_sensitive(f))
        .take(3)
        .collect();
    if sensitive.is_empty() {
        opts.rocky = false;
        println!(
            "gate: {}: no platform-sensitive files; debug on this platform only",
            subject.trim()
        );
    } else {
        opts.release = true;
        println!(
            "gate: {}: touches {}{}; adding --release{}",
            subject.trim(),
            sensitive.join(", "),
            if files.lines().filter(|f| platform_sensitive(f)).count() > 3 {
                ", ..."
            } else {
                ""
            },
            if opts.rocky { " --rocky" } else { "" }
        );
    }
    Ok(())
}

/// Files whose changes can change what builds or what the tests see on each platform.
pub(crate) fn platform_sensitive(path: &str) -> bool {
    const DIRS: &[&str] = &[
        "crates/",
        "xtask/",
        "oracle/",
        "fixtures/",
        "corpus/",
        "scripts/",
        "docker/",
        "upstream/",
        ".cargo/",
    ];
    // .gitattributes decides line endings, and with them the bytes of fixtures and sources.
    const FILES: &[&str] = &[
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        ".gitattributes",
    ];
    DIRS.iter().any(|d| path.starts_with(d)) || FILES.contains(&path)
}

fn cargo(root: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    cmd.args(args)
        .current_dir(root)
        .env("CARGO_TERM_COLOR", "never");
    cmd
}

fn test_command(root: &Path, crates: &[String], release: bool, tier: &str) -> Command {
    let mut cmd = cargo(root, &["test"]);
    if crates.is_empty() {
        cmd.arg("--workspace");
    }
    for name in crates {
        cmd.args(["-p", name]);
    }
    if release {
        cmd.arg("--release");
    }
    cmd.arg("--no-fail-fast").env("OCIO_RS_TIER", tier);
    cmd
}

/// Git for Windows' bash on Windows (the `bash` on PATH there is often WSL's, which can't run
/// `scripts/rocky9.sh` against Docker Desktop); `bash` elsewhere. `OCIO_RS_BASH` overrides.
pub(crate) fn bash() -> Result<PathBuf, String> {
    if let Some(bash) = std::env::var_os("OCIO_RS_BASH") {
        return Ok(bash.into());
    }
    if !cfg!(windows) {
        return Ok("bash".into());
    }
    let exec_path = crate::git(Path::new("."), &["--exec-path"])?;
    Path::new(exec_path.trim())
        .ancestors()
        .map(|dir| dir.join("bin").join("bash.exe"))
        .find(|bash| bash.is_file())
        .ok_or_else(|| {
            format!(
                "Git Bash not found above `git --exec-path` ({}); set OCIO_RS_BASH",
                exec_path.trim()
            )
        })
}

/// Package names of the workspace members.
fn workspace_packages(root: &Path) -> Result<Vec<String>, String> {
    let read = |path: PathBuf| -> Result<toml::Table, String> {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        text.parse::<toml::Table>()
            .map_err(|e| format!("{}: {e}", path.display()))
    };
    let manifest = read(root.join("Cargo.toml"))?;
    let members = manifest
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .ok_or("Cargo.toml has no [workspace] members")?;
    let mut names = Vec::new();
    for member in members.iter().filter_map(|m| m.as_str()) {
        let package = read(root.join(member).join("Cargo.toml"))?;
        if let Some(name) = package
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
        {
            names.push(name.to_string());
        }
    }
    Ok(names)
}

/// A new, empty `<parent>/<UTC time>` directory.
fn new_log_dir(parent: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    let stamp = utc_stamp(SystemTime::now());
    let mut dir = parent.join(&stamp);
    let mut n = 1;
    loop {
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                n += 1;
                dir = parent.join(format!("{stamp}-{n}"));
            }
            Err(e) => return Err(format!("{}: {e}", dir.display())),
        }
    }
}

/// `20260930T051209Z`: the UTC date and time, to the second.
pub(crate) fn utc_stamp(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    let s = secs % 86_400;
    format!(
        "{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

/// Days since 1970-01-01 to (year, month, day) in the proleptic Gregorian calendar
/// (H. Hinnant, "chrono-Compatible Low-Level Date Algorithms", `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub(crate) fn duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{:.1}s", d.as_secs_f64())
    } else if secs < 3600 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h{:02}m", secs / 3600, secs % 3600 / 60)
    }
}

/// How a step's output is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Everything goes to the log.
    Plain,
    /// `cargo test`: also count the results.
    Tests,
    /// Another gate (the Rocky pass): its summary lines are shown as they arrive.
    Nested,
}

/// A gate that `cargo xtask land` started (it names its lock in `OCIO_RS_LAND_LOCK`): the
/// gate holds `gate.lock` next to that lock while it runs, and before each step checks that
/// land still holds its lock. A killed land leaves its rebase and gate running; they stop here,
/// at the gate's next step.
#[derive(Debug)]
pub(crate) struct LandWatch {
    land_lock: PathBuf,
    _gate_lock: File,
}

impl LandWatch {
    fn from_env() -> Result<Option<LandWatch>, String> {
        let Some(path) = std::env::var_os(crate::land::LAND_LOCK_ENV) else {
            return Ok(None);
        };
        let land_lock = PathBuf::from(path);
        let gate_lock = crate::land::lock_file(&land_lock.with_file_name("gate.lock"))?;
        gate_lock.lock().map_err(|e| format!("gate.lock: {e}"))?;
        Ok(Some(LandWatch {
            land_lock,
            _gate_lock: gate_lock,
        }))
    }

    /// An error once the land that started this gate has exited.
    fn check(&self) -> Result<(), String> {
        let alive = std::fs::OpenOptions::new()
            .read(true)
            .open(&self.land_lock)
            .is_ok_and(|file| matches!(file.try_lock(), Err(std::fs::TryLockError::WouldBlock)));
        if alive {
            Ok(())
        } else {
            Err(
                "the `cargo xtask land` that started this gate has exited, so the gate stops \
                 here (and the replay with it)"
                    .into(),
            )
        }
    }
}

/// Runs steps, logging each to `<log_dir>/<name>.log`.
#[derive(Debug)]
pub(crate) struct Runner {
    log_dir: PathBuf,
    pub(crate) started: Instant,
    pub(crate) steps: usize,
    /// Prefix of the summary lines.
    pub(crate) label: &'static str,
    /// Set when `cargo xtask land` runs this gate.
    watch: Option<LandWatch>,
}

impl Runner {
    pub(crate) fn new(log_dir: PathBuf) -> Runner {
        Runner {
            log_dir,
            started: Instant::now(),
            steps: 0,
            label: "gate",
            watch: None,
        }
    }

    /// Runs `cmd` as step `name`; an error once it has printed why the step failed.
    pub(crate) fn step(&mut self, name: &str, mut cmd: Command, kind: Kind) -> Result<(), String> {
        if let Some(watch) = &self.watch {
            watch.check()?;
        }
        self.steps += 1;
        crate::clear_git_env(&mut cmd);
        let log_path = self.log_dir.join(format!("{name}.log"));
        if let Some(dir) = log_path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let shown = crate::display_path(&log_path);
        let mut log =
            File::create(&log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
        let _ = writeln!(log, "$ {}\n", describe(&cmd));
        if kind != Kind::Nested {
            print!("{}: {name:<NAME_WIDTH$} ", self.label);
            let _ = std::io::stdout().flush();
        }
        let start = Instant::now();
        let status = run_logged(&mut cmd, &log, kind == Kind::Nested, name, self.label);
        let elapsed = duration(start.elapsed());
        drop(log);
        let text = std::fs::read(&log_path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let counts = if kind == Kind::Tests {
            test_counts(&text)
                .map(|(p, f, i)| format!("{p} passed, {f} failed, {i} ignored  "))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let (ok, why) = match &status {
            Ok(s) if s.success() => (true, String::new()),
            Ok(s) => (false, format!("exited with {s}")),
            Err(e) => (false, e.clone()),
        };
        let verdict = if ok { "pass" } else { "FAIL" };
        if kind == Kind::Nested {
            print!("{}: {name:<NAME_WIDTH$} ", self.label);
        }
        println!("{verdict} {elapsed:>7}  {counts}{shown}");
        if ok {
            return Ok(());
        }
        let failed = failed_tests(&text);
        if !failed.is_empty() {
            println!("{}: failed tests:", self.label);
            for t in failed.iter().take(30) {
                println!("    {t}");
            }
            if failed.len() > 30 {
                println!("    ... and {} more", failed.len() - 30);
            }
        }
        let targets = failed_targets(&text);
        if !targets.is_empty() {
            // A test binary that crashes (an abort, running out of memory) reports no test.
            println!("{}: failed test binaries:", self.label);
            for t in &targets {
                println!("    {t}");
            }
        }
        println!("{}: last lines of {shown}:", self.label);
        let lines: Vec<&str> = text.lines().collect();
        for line in &lines[lines.len().saturating_sub(25)..] {
            println!("    | {line}");
        }
        Err(format!(
            "{} failed at step `{name}` ({why}); full log: {shown}",
            self.label
        ))
    }
}

/// Width of the step names in summary lines (`rocky/test-release` fits).
const NAME_WIDTH: usize = 18;

/// Runs `cmd` with stdout and stderr in `log`. For a nested gate, stdout is read line by line
/// and its step summary lines are printed as they arrive, renamed `<step>/<its step>`.
fn run_logged(
    cmd: &mut Command,
    log: &File,
    nested: bool,
    step: &str,
    label: &str,
) -> Result<ExitStatus, String> {
    let handle = |log: &File| log.try_clone().map_err(|e| e.to_string());
    cmd.stdin(Stdio::null()).stderr(handle(log)?);
    if !nested {
        cmd.stdout(handle(log)?);
        return cmd
            .status()
            .map_err(|e| format!("could not start {}: {e}", describe(cmd)));
    }
    cmd.stdout(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not start {}: {e}", describe(cmd)))?;
    let mut copy = handle(log)?;
    let stdout = child.stdout.take().expect("piped stdout");
    for line in BufReader::new(stdout).split(b'\n') {
        let Ok(line) = line else { break };
        let _ = copy.write_all(&line);
        let _ = copy.write_all(b"\n");
        let text = String::from_utf8_lossy(&line);
        if let Some((name, rest)) = summary_line(&text, label) {
            println!("{label}: {:<NAME_WIDTH$} {rest}", format!("{step}/{name}"));
        }
    }
    child.wait().map_err(|e| e.to_string())
}

/// `(step, "pass ...")` when `line` is a step summary line of a gate labelled `label`.
fn summary_line<'a>(line: &'a str, label: &str) -> Option<(&'a str, &'a str)> {
    let rest = line
        .trim_end_matches('\r')
        .strip_prefix(label)?
        .strip_prefix(": ")?;
    let (name, rest) = rest.split_once(' ')?;
    let rest = rest.trim_start();
    (rest.starts_with("pass ") || rest.starts_with("FAIL ")).then_some((name, rest))
}

/// The command line and the variables it sets, for the log header and errors.
fn describe(cmd: &Command) -> String {
    let mut s = String::new();
    for (key, value) in cmd.get_envs() {
        if let Some(value) = value {
            s.push_str(&format!(
                "{}={} ",
                key.to_string_lossy(),
                value.to_string_lossy()
            ));
        }
    }
    s.push_str(&cmd.get_program().to_string_lossy());
    for arg in cmd.get_args() {
        s.push(' ');
        s.push_str(&arg.to_string_lossy());
    }
    s
}

/// Passed, failed and ignored tests over every `test result:` line of a `cargo test` log.
pub(crate) fn test_counts(log: &str) -> Option<(u64, u64, u64)> {
    let mut found = false;
    let (mut passed, mut failed, mut ignored) = (0, 0, 0);
    for line in log.lines() {
        let line = strip_ansi(line);
        let Some(rest) = line.trim_start().strip_prefix("test result:") else {
            continue;
        };
        found = true;
        for part in rest.split(';') {
            let mut words = part.split_whitespace().rev();
            let (Some(kind), Some(n)) = (words.next(), words.next()) else {
                continue;
            };
            let Ok(n) = n.parse::<u64>() else { continue };
            match kind {
                "passed" => passed += n,
                "failed" => failed += n,
                "ignored" => ignored += n,
                _ => {}
            }
        }
    }
    found.then_some((passed, failed, ignored))
}

/// Names of the tests a `cargo test` log reports as failed.
fn failed_tests(log: &str) -> Vec<String> {
    log.lines()
        .map(strip_ansi)
        .filter_map(|l| {
            l.trim_start()
                .strip_prefix("test ")?
                .strip_suffix(" ... FAILED")
                .map(String::from)
        })
        .collect()
}

/// The test binaries `cargo test` reports as failed (`to rerun pass `-p x --test y``), with how
/// the process ended when it did not exit normally.
fn failed_targets(log: &str) -> Vec<String> {
    let lines: Vec<String> = log.lines().map(strip_ansi).collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(rest) = line.split("to rerun pass `").nth(1) else {
            continue;
        };
        let target = rest.split('`').next().unwrap_or(rest);
        let ended = lines[i + 1..]
            .iter()
            .take(4)
            .find_map(|l| l.split("didn't exit successfully: ").nth(1))
            .and_then(|l| l.rsplit_once(" ("))
            .map(|(_, how)| format!(" ({how}"))
            .unwrap_or_default();
        out.push(format!("{target}{ended}"));
    }
    out
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // CSI: ESC [ parameters final-byte
            if chars.next() == Some('[') {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_options() {
        let o = parse(&[
            "--crates",
            "ocio-ops,ocio",
            "--release",
            "--rocky",
            "--full",
        ])
        .unwrap();
        assert_eq!(o.crates, ["ocio-ops", "ocio"]);
        assert!(o.release && o.rocky && o.full && !o.auto && !o.main);
        assert!(parse(&["--main", "--auto"]).is_ok_and(|o| o.main && o.auto));
        let o = parse(&["--staged", "--release", "--crates", "xtask"]).unwrap();
        assert!(o.staged && o.release);
        assert_eq!(o.others, ["--release", "--crates", "xtask"]);
        let o = parse(&["--crates=ocio-ops", "--log-dir=target/x", "--quick"]).unwrap();
        assert_eq!(o.crates, ["ocio-ops"]);
        assert_eq!(o.log_dir, Some(PathBuf::from("target/x")));
        assert!(!o.full);
        assert!(parse(&["--quick", "--full"]).is_err());
        assert!(parse(&["--crates"]).is_err());
        assert!(parse(&["--bogus"]).is_err());
    }

    #[test]
    fn counts_tests_across_binaries() {
        let log = "running 2 tests\n\
            test a ... ok\n\
            test b ... FAILED\n\
            test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n\
            \u{1b}[0mtest result: \u{1b}[32mok\u{1b}[0m. 143 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.06s\n";
        assert_eq!(test_counts(log), Some((144, 1, 2)));
        assert_eq!(failed_tests(log), ["b"]);
        assert_eq!(test_counts("error: could not compile"), None);
        // The shape of a test binary that aborts (cargo 1.98.1 on Windows).
        let crashed = "running 3 tests\n\
            memory allocation of 17052710 bytes failed\n\
            error: test failed, to rerun pass `-p ocio-ops --test gamma_oracle`\n\
            \n\
            Caused by:\n  process didn't exit successfully: `D:\\t\\gamma_oracle.exe` (exit code: 0xc0000409, STATUS_STACK_BUFFER_OVERRUN)\n\
            error: test failed, to rerun pass `-p xtask --bin xtask`\n";
        assert_eq!(
            failed_targets(crashed),
            [
                "-p ocio-ops --test gamma_oracle (exit code: 0xc0000409, STATUS_STACK_BUFFER_OVERRUN)",
                "-p xtask --bin xtask"
            ]
        );
    }

    #[test]
    fn utc_stamps() {
        // 2000-02-29 00:00:00 UTC is 951782400 s after the epoch; 2026-09-30 05:12:09 UTC is
        // 1790745129 s.
        let at = |s| utc_stamp(UNIX_EPOCH + Duration::from_secs(s));
        assert_eq!(at(0), "19700101T000000Z");
        assert_eq!(at(951_782_400), "20000229T000000Z");
        assert_eq!(at(1_790_745_129), "20260930T051209Z");
        assert_eq!(at(1_790_745_129 - 86_400 * 273), "20251231T051209Z");
    }

    #[test]
    fn nested_summary_lines() {
        let line =
            "gate: test-release pass   29.8s  328 passed, 0 failed, 0 ignored  target/x.log\r";
        assert_eq!(
            summary_line(line, "gate"),
            Some((
                "test-release",
                "pass   29.8s  328 passed, 0 failed, 0 ignored  target/x.log"
            ))
        );
        assert_eq!(
            summary_line("gate: fmt          FAIL    0.3s  f.log", "gate"),
            Some(("fmt", "FAIL    0.3s  f.log"))
        );
        assert_eq!(summary_line("gate: failed tests:", "gate"), None);
        assert_eq!(summary_line("gate: last lines of x.log:", "gate"), None);
        assert_eq!(summary_line("    | gate: fmt pass 1s", "gate"), None);
    }

    #[test]
    fn platform_sensitive_paths() {
        assert!(platform_sensitive("crates/ocio-ops/src/lib.rs"));
        assert!(platform_sensitive("oracle/ocio_oracle/text.py"));
        assert!(platform_sensitive("Cargo.lock"));
        assert!(platform_sensitive(".gitattributes"));
        assert!(platform_sensitive("upstream/OpenColorIO"));
        assert!(!platform_sensitive("docs/parity.md"));
        assert!(!platform_sensitive("CLAUDE.md"));
        assert!(!platform_sensitive(".github/workflows/ci.yml"));
    }
}
