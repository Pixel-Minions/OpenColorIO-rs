// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `cargo xtask land <branch> [--rocky]`: lands a card branch on `phase0`.
//!
//! From the main checkout, on `phase0`, with a clean tree:
//! 1. replays the branch's commits onto `phase0` in a temporary worktree (`target/land/wt`)
//!    with `git rebase --exec "cargo xtask gate --auto"`, which gates every commit (release,
//!    and Rocky Linux 9 with `--rocky`, for commits that touch platform-sensitive files);
//! 2. merges the result with `--no-ff`, listing each chunk's subject;
//! 3. regenerates `docs/parity.md` and `docs/ratchet.toml` into that merge commit;
//! 4. runs the full gate on it (debug and release, `xtask ci --main`, plus Rocky with
//!    `--rocky`) and `xtask oracle check-all`;
//! 5. fast-forwards `phase0` to it and prints the push steps. It never pushes.
//!
//! `phase0` only moves once every step has passed. On a failure the temporary worktree stays
//! as it stopped, for inspection; the next run removes it.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::gate::{self, Kind, Runner};

/// The local integration branch; `main` on GitHub fast-forwards to it.
const INTEGRATION: &str = "phase0";
const CO_AUTHOR: &str = "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>";
/// Trailer naming the branch and the commit a merge landed (`xtask clean-scratch` reads it).
pub(crate) const LANDED_FROM: &str = "Landed-From:";

pub(crate) const USAGE: &str = "\
cargo xtask land <branch> [--rocky]

Lands <branch> on phase0, from the main checkout on phase0 with a clean tree:
  1. replays its commits onto phase0 in target/land/wt with
     `git rebase --exec \"cargo xtask gate --auto [--rocky]\"`: every commit is gated, in
     release too (and in Rocky Linux 9 with --rocky) when it touches platform-sensitive files;
     commits already on top of phase0 keep their hashes
  2. merges the result with --no-ff; the message lists each chunk's subject
  3. regenerates docs/parity.md and docs/ratchet.toml into the merge commit
  4. runs `cargo xtask gate --full --release --main [--rocky]` and `xtask oracle check-all`
     on the merge commit
  5. fast-forwards phase0 to it, and prints the push steps. It never pushes.
";

pub(crate) fn parse(args: &[&str]) -> Result<(String, bool), String> {
    let mut branch = None;
    let mut rocky = false;
    for &arg in args {
        match arg {
            "--rocky" => rocky = true,
            _ if arg.starts_with('-') => return Err(format!("unknown land option `{arg}`")),
            _ if branch.is_none() => branch = Some(arg.to_string()),
            _ => return Err(format!("land takes one branch\n\n{USAGE}")),
        }
    }
    let branch = branch.ok_or_else(|| format!("land needs a branch\n\n{USAGE}"))?;
    Ok((branch, rocky))
}

pub(crate) fn run(branch: &str, rocky: bool) -> Result<(), String> {
    let root = crate::root();
    let git = |args: &[&str]| crate::git(&root, args).map(|s| s.trim().to_string());

    // The main checkout, on phase0, with nothing uncommitted: the result lands there.
    let git_dir = git(&["rev-parse", "--path-format=absolute", "--git-dir"])?;
    let common_dir = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    if !same_path(&git_dir, &common_dir) {
        return Err(
            "run `cargo xtask land` from the main checkout, not from a linked worktree".into(),
        );
    }
    let head = git(&["symbolic-ref", "--quiet", "HEAD"]).unwrap_or_default();
    if head != format!("refs/heads/{INTEGRATION}") {
        return Err(format!(
            "the main checkout must be on {INTEGRATION} (it is on {})",
            if head.is_empty() {
                "a detached HEAD"
            } else {
                &head
            }
        ));
    }
    check_clean(&root)?;
    let base = git(&[
        "rev-parse",
        "--verify",
        &format!("refs/heads/{INTEGRATION}^{{commit}}"),
    ])?;
    let tip = git(&[
        "rev-parse",
        "--verify",
        &format!("refs/heads/{branch}^{{commit}}"),
    ])
    .map_err(|_| format!("there is no local branch `{branch}`"))?;
    let range = format!("{base}..{tip}");
    let commits = git(&["log", "--reverse", "--format=%h %s", &range])?;
    if commits.is_empty() {
        return Err(format!(
            "nothing to land: {INTEGRATION} already contains {branch}"
        ));
    }
    println!(
        "land: {branch} ({}) onto {INTEGRATION} ({}):",
        short(&tip),
        short(&base)
    );
    for line in commits.lines() {
        println!("    {line}");
    }
    let merges = git(&["rev-list", "--merges", "--count", &range])?;
    if merges != "0" {
        println!("land: the replay drops the branch's {merges} merge commit(s)");
    }

    let land_dir = root.join("target").join("land");
    let _lock = Lock::take(&land_dir)?;
    let wt = land_dir.join("wt");
    let build = land_dir.join("target");
    remove_worktree(&root, &wt)?;
    crate::git(
        &root,
        &[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            path_str(&wt)?,
            &tip,
        ],
    )?;
    init_submodule(&root, &wt)?;
    println!(
        "land: worktree {}, build directory {}",
        crate::display_path(&wt),
        crate::display_path(&build)
    );

    // 1. Replay, gating every commit.
    let exec = if rocky {
        "cargo xtask gate --auto --rocky"
    } else {
        "cargo xtask gate --auto"
    };
    println!("land: git rebase --exec \"{exec}\" {}", short(&base));
    let mut rebase = Command::new("git");
    rebase
        .current_dir(&wt)
        .args([
            "rebase",
            "--no-autosquash",
            "--no-update-refs",
            "--exec",
            exec,
        ])
        .arg(&base);
    let replayed = land_env(&mut rebase, &build)
        .status()
        .map_err(|e| format!("could not run git rebase: {e}"))?;
    if !replayed.success() {
        return Err(replay_failure(&wt));
    }
    let rebased = crate::git(&wt, &["rev-parse", "HEAD"])?.trim().to_string();
    if rebased == base {
        return Err(format!(
            "nothing to land: the replay dropped every commit of {branch}, whose changes are \
             already in {INTEGRATION}"
        ));
    }
    let kept = rebased == tip;
    let chunks = crate::git(
        &wt,
        &[
            "log",
            "--reverse",
            "--format=%s",
            &format!("{base}..{rebased}"),
        ],
    )?;
    let chunks: Vec<&str> = chunks.lines().collect();
    if kept {
        println!("land: every commit kept its hash (the branch sat on top of {INTEGRATION})");
    } else {
        println!(
            "land: the replay rewrote the commits onto {INTEGRATION}; {} chunk(s), tip {}",
            chunks.len(),
            short(&rebased)
        );
    }

    // 2.-4. The merge commit with the generated files, gated.
    let landing = Landing {
        wt: &wt,
        build: &build,
        message_file: land_dir.join("MERGE_MSG"),
        rocky,
    };
    let landed = landing
        .merge_and_gate(&base, &rebased, &merge_message(branch, &tip, &chunks))
        .map_err(|e| {
            format!(
                "{e}\nland stopped: {INTEGRATION} and {branch} are untouched; {} is left as it \
                 stopped, for inspection (the next `cargo xtask land` removes it)",
                crate::display_path(&wt)
            )
        })?;

    // 5. phase0 moves only now, and only if nothing else moved it meanwhile.
    let now = git(&[
        "rev-parse",
        "--verify",
        &format!("refs/heads/{INTEGRATION}^{{commit}}"),
    ])?;
    let head = git(&["symbolic-ref", "--quiet", "HEAD"]).unwrap_or_default();
    if now != base || head != format!("refs/heads/{INTEGRATION}") {
        return Err(format!(
            "{INTEGRATION} or the main checkout changed during the land; {INTEGRATION} is \
             untouched, and the gated result is {landed} in {}",
            crate::display_path(&wt)
        ));
    }
    check_clean(&root)?;
    crate::git(&root, &["merge", "--quiet", "--ff-only", &landed])?;
    remove_worktree(&root, &wt)?;

    println!(
        "land: {INTEGRATION} is now {} (was {}). Nothing was pushed.",
        short(&landed),
        short(&base)
    );
    println!("land: next steps:");
    println!("  1. Run CI on exactly this commit, on the PR branch:");
    if kept {
        println!("       git push origin {landed}:refs/heads/{branch}");
    } else {
        println!(
            "     The replay changed the commit hashes, so the PR branch needs --force-with-lease:"
        );
        println!(
            "       git push --force-with-lease=refs/heads/{branch}:{tip} origin {landed}:refs/heads/{branch}"
        );
    }
    println!("  2. Once CI has passed on {landed}, fast-forward main:");
    println!("       git push origin {landed}:refs/heads/main");
    println!(
        "     main accepts only commits whose Windows, Rocky Linux 9 and cargo-deny checks passed,"
    );
    println!("     so this works only after CI has passed on that exact SHA.");
    Ok(())
}

/// Where the merge commit is made and gated.
struct Landing<'a> {
    wt: &'a Path,
    build: &'a Path,
    message_file: PathBuf,
    rocky: bool,
}

impl Landing<'_> {
    /// Merges `rebased` into `base` with `--no-ff`, regenerates the generated files into the
    /// merge commit, runs the full gate and `oracle check-all` on it, and returns its hash.
    fn merge_and_gate(&self, base: &str, rebased: &str, message: &str) -> Result<String, String> {
        let wt = self.wt;
        crate::git(wt, &["checkout", "--quiet", "--detach", base])?;
        std::fs::write(&self.message_file, message)
            .map_err(|e| format!("{}: {e}", self.message_file.display()))?;
        let mut merge = Command::new("git");
        merge
            .current_dir(wt)
            .args(["merge", "--quiet", "--no-ff", "--no-edit", "-F"])
            .arg(&self.message_file)
            .arg(rebased);
        let merged = self.run(&mut merge, "git merge --no-ff");
        let _ = std::fs::remove_file(&self.message_file);
        merged?;

        // 3. The generated files, regenerated into the merge commit.
        for args in [&["parity"][..], &["ratchet", "--update"][..]] {
            let mut cmd = Command::new(cargo());
            cmd.current_dir(wt).arg("xtask").args(args);
            self.run(&mut cmd, &format!("cargo xtask {}", args.join(" ")))?;
        }
        let changed = crate::git(wt, &["status", "--porcelain", "--ignore-submodules=none"])?;
        let unexpected: Vec<&str> = changed
            .lines()
            .filter(|l| !l.ends_with(" docs/parity.md") && !l.ends_with(" docs/ratchet.toml"))
            .collect();
        if !unexpected.is_empty() {
            return Err(format!(
                "regenerating the generated files changed other files:\n  {}",
                unexpected.join("\n  ")
            ));
        }
        if changed.trim().is_empty() {
            println!("land: docs/parity.md and docs/ratchet.toml were already current");
        } else {
            crate::git(wt, &["add", "docs/parity.md", "docs/ratchet.toml"])?;
            let mut amend = Command::new("git");
            amend
                .current_dir(wt)
                .args(["commit", "--quiet", "--amend", "--no-edit"]);
            self.run(&mut amend, "git commit --amend")?;
            println!(
                "land: regenerated docs/parity.md and docs/ratchet.toml into the merge commit"
            );
        }
        let landed = crate::git(wt, &["rev-parse", "HEAD"])?.trim().to_string();
        println!("land: merge commit {}", short(&landed));

        // 4. The full gate on the merge commit, and the fixtures on each platform.
        let rocky = if self.rocky { " --rocky" } else { "" };
        println!(
            "land: cargo xtask gate --full --release --main{rocky} on {}",
            short(&landed)
        );
        let mut full = Command::new(cargo());
        full.current_dir(wt)
            .args(["xtask", "gate", "--full", "--release", "--main"]);
        if self.rocky {
            full.arg("--rocky");
        }
        self.run(&mut full, "the full gate on the merge commit")?;
        let logs = wt.join("target").join("gate-logs").join(format!(
            "land-{}",
            gate::utc_stamp(std::time::SystemTime::now())
        ));
        let mut runner = Runner::new(logs);
        runner.label = "land";
        let mut check = Command::new(cargo());
        check
            .current_dir(wt)
            .args(["xtask", "oracle", "check-all"])
            .env("CARGO_TARGET_DIR", self.build);
        runner.step("oracle-check-all", check, Kind::Plain)?;
        if self.rocky {
            let mut check = Command::new(gate::bash()?);
            check.current_dir(wt).args([
                "scripts/rocky9.sh",
                "cargo",
                "xtask",
                "oracle",
                "check-all",
            ]);
            runner.step("rocky/oracle-check-all", check, Kind::Plain)?;
        }
        Ok(landed)
    }

    /// Runs `cmd` with land's environment and its output on the console.
    fn run(&self, cmd: &mut Command, what: &str) -> Result<(), String> {
        let status = land_env(cmd, self.build)
            .status()
            .map_err(|e| format!("could not run {what}: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("{what} failed ({status})"))
        }
    }
}

/// `Merge <branch> (<n> chunks)`, the chunk subjects, and the trailers.
fn merge_message(branch: &str, tip: &str, chunks: &[&str]) -> String {
    let mut message = format!(
        "Merge {branch} ({} chunk{})\n\n",
        chunks.len(),
        if chunks.len() == 1 { "" } else { "s" }
    );
    for subject in chunks {
        message.push_str(&format!("- {subject}\n"));
    }
    message.push_str(&format!("\n{LANDED_FROM} {branch} {tip}\n{CO_AUTHOR}\n"));
    message
}

/// What stopped the replay, read from the rebase's `done` list.
fn replay_failure(wt: &Path) -> String {
    let done = crate::git(
        wt,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "rebase-merge/done",
        ],
    )
    .ok()
    .and_then(|p| std::fs::read_to_string(p.trim()).ok())
    .unwrap_or_default();
    let lines: Vec<&str> = done.lines().filter(|l| !l.trim().is_empty()).collect();
    let last_pick = lines
        .iter()
        .rev()
        .find_map(|l| l.strip_prefix("pick "))
        .map(|l| {
            let sha = l.split_whitespace().next().unwrap_or("");
            crate::git(wt, &["log", "-1", "--format=%h %s", sha])
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| l.to_string())
        });
    let conflicts = crate::git(wt, &["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default();
    let place = crate::display_path(wt);
    let why = match (lines.last(), last_pick) {
        (Some(line), Some(commit)) if line.starts_with("exec ") => {
            format!("the gate failed on {commit}; its logs are in {place}/target/gate-logs/")
        }
        (_, Some(commit)) if !conflicts.trim().is_empty() => format!(
            "{commit} does not apply on {INTEGRATION}: conflicts in {}",
            conflicts.lines().collect::<Vec<_>>().join(", ")
        ),
        (_, Some(commit)) => format!("the replay stopped at {commit}"),
        _ => "git rebase failed before replaying any commit".to_string(),
    };
    format!(
        "land stopped: {why}.\n{INTEGRATION} and the branch are untouched; {place} is left as the \
         replay stopped, for inspection (the next `cargo xtask land` removes it)"
    )
}

/// The environment of land's git and cargo commands: a build directory shared by every land
/// (third-party crates and the oracle cache stay warm), and no editor.
fn land_env<'a>(cmd: &'a mut Command, build: &Path) -> &'a mut Command {
    gate::clear_git_env(cmd);
    cmd.env("CARGO_TARGET_DIR", build)
        .env("GIT_EDITOR", "true")
        .stdin(Stdio::null())
}

fn cargo() -> std::ffi::OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

fn check_clean(root: &Path) -> Result<(), String> {
    let status = crate::git(root, &["status", "--porcelain", "--ignore-submodules=none"])?;
    if status.trim().is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the main checkout has uncommitted changes or untracked files:\n{status}"
        ))
    }
}

/// Removes land's temporary worktree, registered or not. `--force`: it holds the upstream
/// submodule and may be stopped mid-rebase, and git refuses to remove either without it.
fn remove_worktree(root: &Path, wt: &Path) -> Result<(), String> {
    let list = crate::git(root, &["worktree", "list", "--porcelain"])?;
    let registered = list
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .any(|p| same_path(p, &wt.to_string_lossy()));
    if registered {
        crate::git(root, &["worktree", "remove", "--force", path_str(wt)?])?;
    }
    if wt.exists() {
        std::fs::remove_dir_all(wt).map_err(|e| format!("{}: {e}", wt.display()))?;
    }
    Ok(())
}

/// Checks out the upstream submodule in `wt` from the main checkout's copy (a local clone:
/// no network).
fn init_submodule(root: &Path, wt: &Path) -> Result<(), String> {
    let source = root.join("upstream").join("OpenColorIO");
    let url = format!("submodule.upstream/OpenColorIO.url={}", path_str(&source)?);
    let out = Command::new("git")
        .current_dir(wt)
        .args(["-c", "protocol.file.allow=always", "-c", &url])
        .args(["submodule", "--quiet", "update", "--init"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run git submodule update: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "could not check out upstream/OpenColorIO in {}: {}",
            crate::display_path(wt),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

/// A lock file, so that two lands don't share the temporary worktree.
#[derive(Debug)]
struct Lock(PathBuf);

impl Lock {
    fn take(dir: &Path) -> Result<Lock, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join("lock");
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let _ = writeln!(file, "pid {}", std::process::id());
                Ok(Lock(path))
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(format!(
                "another `cargo xtask land` is running ({}), or one was killed; if none is \
                 running, delete {}",
                std::fs::read_to_string(&path).unwrap_or_default().trim(),
                crate::display_path(&path)
            )),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn path_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("{} is not valid UTF-8", path.display()))
}

/// Whether two paths, as git or the OS print them, name the same directory.
pub(crate) fn same_path(a: &str, b: &str) -> bool {
    let norm = |p: &str| {
        let p = crate::plain_path(Path::new(p))
            .to_string_lossy()
            .replace('\\', "/");
        let p = p.trim_end_matches('/').to_string();
        if cfg!(windows) { p.to_lowercase() } else { p }
    };
    norm(a) == norm(b)
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(10)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_arguments() {
        assert_eq!(parse(&["card/x"]).unwrap(), ("card/x".to_string(), false));
        assert_eq!(
            parse(&["--rocky", "card/x"]).unwrap(),
            ("card/x".to_string(), true)
        );
        assert!(parse(&[]).is_err());
        assert!(parse(&["a", "b"]).is_err());
        assert!(parse(&["card/x", "--bogus"]).is_err());
    }

    #[test]
    fn merge_message_lists_chunks_and_ends_with_the_co_author() {
        let m = merge_message("card/x", "abc123", &["T1: one", "T1: two"]);
        assert_eq!(
            m,
            "Merge card/x (2 chunks)\n\n- T1: one\n- T1: two\n\n\
             Landed-From: card/x abc123\n\
             Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n"
        );
        assert!(merge_message("b", "c", &["only"]).starts_with("Merge b (1 chunk)\n"));
    }

    #[test]
    fn compares_paths_as_git_prints_them() {
        if cfg!(windows) {
            assert!(same_path("D:/Projects/x", r"D:\Projects\x\"));
            assert!(same_path(r"\\?\D:\Projects\X", "d:/projects/x"));
        }
        assert!(same_path("/work/a", "/work/a/"));
        assert!(!same_path("/work/a", "/work/b"));
    }
}
