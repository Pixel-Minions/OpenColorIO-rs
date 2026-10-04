// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `cargo xtask land <branch> [--no-rocky]`: lands a card branch on `phase0`.
//!
//! From the main checkout, on `phase0`, with a clean tree:
//! 1. replays the branch's commits onto `phase0` in a temporary worktree (`target/land/wt`)
//!    with `git rebase --exec "cargo xtask gate"`, which gates every commit in debug on this
//!    platform (release and Rocky Linux 9 run once, on the merge commit, in step 4); a branch
//!    with merge commits is first replayed without gates, to check that the replay drops
//!    nothing they carry;
//! 2. merges the result with `--no-ff`, listing each chunk's subject;
//! 3. regenerates `docs/parity.md` and `docs/ratchet.toml` into that merge commit;
//! 4. runs the full gate on it (debug and release, `xtask ci --main`, cargo-deny, Rocky) and
//!    `xtask oracle check-all`;
//! 5. fast-forwards `phase0` to it and prints the push steps. It never pushes.
//!
//! `phase0` only moves once every step has passed. On a failure the temporary worktree stays
//! as it stopped, for inspection; the next run removes it.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::gate::{self, Kind, Runner};

/// The local integration branch; `main` on GitHub fast-forwards to it.
const INTEGRATION: &str = "phase0";
const CO_AUTHOR: &str = "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>";
/// Trailer naming the branch and the commit a merge landed (`xtask clean-scratch` reads it).
pub(crate) const LANDED_FROM: &str = "Landed-From:";
/// Environment variable naming the running land's lock file, for the gates it starts.
pub(crate) const LAND_LOCK_ENV: &str = "OCIO_RS_LAND_LOCK";
/// Environment variable naming the running land itself (a nonce it writes to `lock.id` next to
/// its lock): a later land holds the same lock path.
pub(crate) const LAND_ID_ENV: &str = "OCIO_RS_LAND_ID";

pub(crate) const USAGE: &str = "\
cargo xtask land <branch> [--no-rocky]

Lands <branch> on phase0, from the main checkout on phase0 with a clean tree:
  1. replays its commits onto phase0 in target/land/wt with
     `git rebase --exec \"cargo xtask gate\"`: every commit is gated in debug on this
     platform (each chunk already passed its own gate; release and Rocky Linux 9 run on
     the merge commit, step 4); commits already on top of phase0 keep their hashes. For a branch with merge commits, a first
     replay without gates must give the tree that merging the branch as it is gives (a
     replay drops merge commits, and what only they carry); otherwise it stops at once.
  2. merges the result with --no-ff; the message lists each chunk's subject
  3. regenerates docs/parity.md and docs/ratchet.toml into the merge commit
  4. runs `cargo xtask gate --full --release --main --rocky` (with cargo-deny) and
     `xtask oracle check-all` on the merge commit
  5. fast-forwards phase0 to it, and prints the push steps. It never pushes.

  --no-rocky   skip Rocky Linux 9 everywhere (Docker unavailable); CI still runs it

One land runs at a time (an OS lock on target/land/lock). If a land is killed, the rebase and
gate it started carry on until the gate's next step, which sees that land has exited and
stops; a new land waits for no one, but refuses to start while such a gate still runs.
";

pub(crate) fn parse(args: &[&str]) -> Result<(String, bool), String> {
    let mut branch = None;
    let mut rocky = true;
    for &arg in args {
        match arg {
            "--rocky" => rocky = true,
            "--no-rocky" => rocky = false,
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
    // The generated files are land's to write: a branch that edits them could, for one, lower
    // the ratchet (the changes it makes since it left phase0).
    let generated = git(&[
        "diff",
        "--name-only",
        &format!("{base}...{tip}"),
        "--",
        "docs/ratchet.toml",
        "docs/parity.md",
    ])?;
    if !generated.is_empty() {
        return Err(format!(
            "{branch} edits the generated files ({}): chunks never commit them, and \
             `cargo xtask land` regenerates them. Drop those edits from the branch",
            generated.lines().collect::<Vec<_>>().join(", ")
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
    let lock = Lock::take(&land_dir)?;
    let wt = land_dir.join("wt");
    let build = land_dir.join("target");
    remove_worktree(&root, &wt)?;
    add_worktree(&root, &wt, &tip)?;
    println!(
        "land: worktree {}, build directory {}",
        crate::display_path(&wt),
        crate::display_path(&build)
    );

    // A replay drops merge commits, and with them anything only they carry. Before any gate
    // runs, replay once without gates and compare that tree with merging the branch as it is.
    // (Only with merge commits: for a linear branch the check has nothing to find, and it would
    // refuse a follow-up land whose earlier chunks landed already, rewritten.)
    if merges != "0" {
        let mut plain = Command::new("git");
        plain
            .current_dir(&wt)
            .args(["rebase", "--quiet", "--no-autosquash", "--no-update-refs"])
            .arg(&base);
        let replayed = land_env(&mut plain, &build, &lock)
            .status()
            .map_err(|e| format!("could not run git rebase: {e}"))?;
        if !replayed.success() {
            return Err(replay_failure(&wt));
        }
        let rebased = crate::git(&wt, &["rev-parse", "HEAD"])?;
        check_replay(&root, &base, &tip, rebased.trim()).map_err(|e| {
            format!(
                "{e}\nland stopped before any gate ran: {INTEGRATION} and {branch} are \
                 untouched; the plain replay is in {}",
                crate::display_path(&wt)
            )
        })?;
        crate::git(&wt, &["checkout", "--quiet", "--detach", &tip])?;
        println!("land: replaying drops nothing the branch's merge commits carry");
    }

    // 1. Replay, gating every commit lightly: fmt, clippy, ci and the debug tests on this
    // platform. Each chunk already passed its own `gate --staged` (with --release --rocky when
    // numeric), and step 4 runs the full gate, release and Rocky Linux 9 included, on the
    // merged result (owner decision, 2026-10-01: the per-commit Rocky and release passes made
    // lands the slowest step).
    let exec = "cargo xtask gate";
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
    let replayed = land_env(&mut rebase, &build, &lock)
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
        lock: &lock,
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

    // phase0 has moved: report that first, whatever happens next.
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
    if let Err(e) = remove_worktree(&root, &wt) {
        println!(
            "land: warning: could not remove {} ({e}); the next `cargo xtask land` removes it",
            crate::display_path(&wt)
        );
    }
    Ok(())
}

/// Stops when the replayed tree differs from merging the branch as it is: content that only
/// the branch's merge commits carry, which the replay drops, or a replay that resolved
/// something differently.
fn check_replay(root: &Path, base: &str, tip: &str, rebased: &str) -> Result<(), String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["merge-tree", "--write-tree", base, tip])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run git merge-tree: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let merged = text.lines().next().unwrap_or("").trim().to_string();
    if !out.status.success() || merged.is_empty() {
        return Err(format!(
            "merging the branch as it is ({}) conflicts with {INTEGRATION}, although replaying \
             its commits did not, so the replay can't be checked: rebase the branch onto \
             {INTEGRATION} yourself\n{}",
            short(tip),
            text.trim()
        ));
    }
    let replayed = crate::git(root, &["rev-parse", &format!("{rebased}^{{tree}}")])?;
    if merged == replayed.trim() {
        return Ok(());
    }
    let stat = crate::git(root, &["diff", "--stat", &merged, replayed.trim()]).unwrap_or_default();
    Err(format!(
        "the replayed branch differs from merging it as it is: the replay would drop what \
         these files carry in the branch's merge commits (or resolve them differently):\n{}",
        stat.trim_end()
    ))
}

/// Where the merge commit is made and gated.
struct Landing<'a> {
    wt: &'a Path,
    build: &'a Path,
    message_file: PathBuf,
    rocky: bool,
    lock: &'a Lock,
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

        // 3. The generated files, regenerated into the merge commit. The ratchet's baseline is
        // phase0's committed docs/ratchet.toml.
        for args in [
            &["parity"][..],
            &["ratchet", "--update", "--base", base][..],
        ] {
            let mut cmd = Command::new(cargo());
            cmd.current_dir(wt).arg("xtask").args(args);
            self.run(&mut cmd, &format!("cargo xtask {}", args.join(" ")))?;
        }
        let changed = crate::git(wt, crate::STATUS_ALL)?;
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
        let status = land_env(cmd, self.build, self.lock)
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
/// (third-party crates and the oracle cache stay warm), no editor, and the lock the gates
/// watch.
fn land_env<'a>(cmd: &'a mut Command, build: &Path, lock: &Lock) -> &'a mut Command {
    crate::clear_git_env(cmd);
    cmd.env("CARGO_TARGET_DIR", build)
        .env("GIT_EDITOR", "true")
        .env(LAND_LOCK_ENV, &lock.path)
        .env(LAND_ID_ENV, &lock.id)
        .stdin(Stdio::null())
}

fn cargo() -> std::ffi::OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

fn check_clean(root: &Path) -> Result<(), String> {
    let status = crate::git(root, crate::STATUS_ALL)?;
    if status.trim().is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the main checkout has uncommitted changes or untracked files:\n{status}"
        ))
    }
}

/// Removes a scratch worktree (land's, `gate --staged`'s, or one `clean-scratch` deletes),
/// registered or not. `--force`: it holds the upstream submodule and may be stopped
/// mid-rebase, and git refuses to remove either without it.
///
/// First every link inside it is unlinked (`links::unlink_all`): `git worktree remove --force`
/// deletes what a junction points to (Git for Windows 2.53), such as the main checkout's
/// submodule when a worktree links `upstream/OpenColorIO` to it. A worktree that is itself a
/// link is refused, and so is a locked one (`git worktree lock`), before anything in it is
/// touched.
pub(crate) fn remove_worktree(root: &Path, wt: &Path) -> Result<(), String> {
    let list = crate::git(root, &["worktree", "list", "--porcelain"])?.replace("\r\n", "\n");
    // `git worktree list --porcelain`: one record per worktree, separated by an empty line.
    let record = list.split("\n\n").find(|record| {
        record
            .lines()
            .filter_map(|l| l.strip_prefix("worktree "))
            .any(|p| same_path(p, &wt.to_string_lossy()))
    });
    if let Some(record) = record
        && record
            .lines()
            .any(|l| l == "locked" || l.starts_with("locked "))
    {
        return Err(format!(
            "{} is locked (`git worktree lock`): not removing it; `git worktree unlock` it first",
            crate::display_path(wt)
        ));
    }
    for link in crate::links::unlink_all(wt)? {
        println!(
            "removed the link {} (not what it pointed to)",
            crate::plain_path(&link)
                .to_string_lossy()
                .replace('\\', "/")
        );
    }
    if record.is_some() {
        crate::git(
            root,
            &[
                LONG_PATHS[0],
                LONG_PATHS[1],
                "worktree",
                "remove",
                "--force",
                path_str(wt)?,
            ],
        )?;
    }
    crate::links::remove_tree(wt)
}

/// Nested under a long checkout path, OCIO's longest file names (84 characters inside the
/// submodule) pass Windows' 260: git needs `core.longpaths` to create or delete them.
const LONG_PATHS: [&str; 2] = ["-c", "core.longpaths=true"];

/// Checks out `commit`, detached, in a new scratch worktree at `wt`, with the upstream
/// submodule.
pub(crate) fn add_worktree(root: &Path, wt: &Path, commit: &str) -> Result<(), String> {
    crate::git(
        root,
        &[
            LONG_PATHS[0],
            LONG_PATHS[1],
            "worktree",
            "add",
            "--quiet",
            "--detach",
            path_str(wt)?,
            commit,
        ],
    )?;
    init_submodule(root, wt)
}

/// Checks out the upstream submodule in `wt` from the main checkout's copy (a local clone:
/// no network).
fn init_submodule(root: &Path, wt: &Path) -> Result<(), String> {
    let source = root.join("upstream").join("OpenColorIO");
    let url = format!("submodule.upstream/OpenColorIO.url={}", path_str(&source)?);
    let out = crate::clear_git_env(&mut Command::new("git"))
        .current_dir(wt)
        .args(["-c", "protocol.file.allow=always", "-c", &url])
        .args(LONG_PATHS)
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

/// One land at a time: an OS lock on `target/land/lock`, held for the whole run and released
/// by the OS when the process ends, however it ends, so a lock is never stale.
///
/// A killed land's rebase and gate aren't killed with it (that needs a job object or `unsafe`
/// code, which xtask may not use). Instead the gates a land starts watch its lock before each
/// step, and stop at the first step after it is released (`gate::LandWatch`); while they run
/// they hold `target/land/gate.lock`, and a new land doesn't start under them.
#[derive(Debug)]
pub(crate) struct Lock {
    path: PathBuf,
    /// This land's nonce, also in `lock.id`: the gates it starts compare the two, since a land
    /// started after a killed one holds the same lock path.
    id: String,
    _file: File,
}

impl Lock {
    fn take(dir: &Path) -> Result<Lock, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join("lock");
        let who = dir.join("lock.pid");
        let file = lock_file(&path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(format!(
                    "another `cargo xtask land` is running ({})",
                    std::fs::read_to_string(&who).unwrap_or_default().trim()
                ));
            }
            Err(TryLockError::Error(e)) => return Err(format!("{}: {e}", path.display())),
        }
        let gate = lock_file(&dir.join("gate.lock"))?;
        match gate.try_lock() {
            Ok(()) => drop(gate),
            Err(TryLockError::WouldBlock) => {
                return Err(format!(
                    "a gate that an earlier `cargo xtask land` started is still running in {} \
                     (that land was killed); it stops at its next step: wait for it, then try \
                     again",
                    crate::display_path(&dir.join("wt"))
                ));
            }
            Err(TryLockError::Error(e)) => return Err(format!("gate.lock: {e}")),
        }
        let _ = std::fs::write(&who, format!("pid {}\n", std::process::id()));
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let id = format!("{}-{nanos:x}", std::process::id());
        let id_file = dir.join("lock.id");
        std::fs::write(&id_file, &id).map_err(|e| format!("{}: {e}", id_file.display()))?;
        Ok(Lock {
            path,
            id,
            _file: file,
        })
    }
}

/// Whether the land whose lock is `lock` and whose nonce is `id` still runs: some process holds
/// that lock, and `lock.id` beside it names this land, not a later one.
pub(crate) fn land_alive(lock: &Path, id: &str) -> bool {
    let held = OpenOptions::new()
        .read(true)
        .open(lock)
        .is_ok_and(|file| matches!(file.try_lock(), Err(TryLockError::WouldBlock)));
    held && std::fs::read_to_string(lock.with_file_name("lock.id")).is_ok_and(|s| s.trim() == id)
}

/// Opens (creating it) a file used only for its OS lock.
pub(crate) fn lock_file(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))
}

pub(crate) fn path_str(path: &Path) -> Result<&str, String> {
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
        // Rocky Linux 9 is the default; --no-rocky opts out.
        assert_eq!(parse(&["card/x"]).unwrap(), ("card/x".to_string(), true));
        assert_eq!(
            parse(&["--rocky", "card/x"]).unwrap(),
            ("card/x".to_string(), true)
        );
        assert_eq!(
            parse(&["card/x", "--no-rocky"]).unwrap(),
            ("card/x".to_string(), false)
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
