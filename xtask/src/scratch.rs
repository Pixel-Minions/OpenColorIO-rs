// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `cargo xtask clean-scratch [--yes] [--unlabelled]`: lists, and with `--yes` deletes, scratch
//! that piles up as cards land:
//! - verifier and probe output, `target/verify*`, in the main checkout and every worktree,
//!   including worktrees still in use (it is output, never input);
//! - worktrees whose branch has landed on `phase0`, when they have nothing uncommitted;
//! - Docker volumes `ocio-rs-target-<id>` (the Rocky Linux 9 build directories of
//!   `scripts/rocky9.sh`) labelled with this repository whose checkout is gone.
//!
//! `scripts/rocky9.sh` labels each volume with its checkout and repository. Volumes of other
//! repositories, and volumes without those labels (made by an older `scripts/rocky9.sh`, which
//! can't be attributed), are kept; `--unlabelled` also deletes the unlabelled ones that no
//! checkout of this repository still uses. It never deletes anything else.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::land::{LANDED_FROM, same_path};

const INTEGRATION: &str = "phase0";
const VOLUME_PREFIX: &str = "ocio-rs-target-";
/// Labels `scripts/rocky9.sh` gives the volumes it creates.
const CHECKOUT_LABEL: &str = "ocio-rs.checkout";
const REPOSITORY_LABEL: &str = "ocio-rs.git-common-dir";

pub(crate) const USAGE: &str = "usage: cargo xtask clean-scratch [--yes] [--unlabelled]";

pub(crate) fn run(args: &[&str]) -> Result<(), String> {
    let mut yes = false;
    let mut unlabelled = false;
    for &arg in args {
        match arg {
            "--yes" => yes = true,
            "--unlabelled" => unlabelled = true,
            _ => return Err(USAGE.into()),
        }
    }
    let root = crate::root();
    let worktrees = worktrees(&root)?;
    if yes {
        println!("clean-scratch: deleting what is marked `delete`");
    } else {
        println!("clean-scratch: dry run; nothing is deleted without --yes");
    }
    let mut freed = 0u64;
    let mut failures = Vec::new();

    // Which worktrees go: those whose branch landed and that have nothing uncommitted.
    let landed = Landed::load(&root)?;
    if !landed.available {
        println!("clean-scratch: no {INTEGRATION} branch here, so no worktree counts as landed");
    }
    let decisions: Vec<Result<String, String>> = worktrees
        .iter()
        .enumerate()
        .map(|(i, wt)| {
            if i == 0 {
                Err("the main checkout".to_string())
            } else {
                removable(&root, wt, &landed)
            }
        })
        .collect();
    let goes = |wt: &Worktree| {
        worktrees
            .iter()
            .zip(&decisions)
            .any(|(w, d)| w.path == wt.path && d.is_ok())
    };

    // 1. Verifier and probe output, in the checkouts that stay (the others go whole below).
    println!("\ntarget/verify* (verifier and probe output), in every checkout:");
    let mut any = false;
    for wt in worktrees
        .iter()
        .filter(|w| Path::new(&w.path).is_dir() && !goes(w))
    {
        let target = Path::new(&wt.path).join("target");
        let Ok(entries) = std::fs::read_dir(&target) else {
            continue;
        };
        let mut found: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("verify"))
            .map(|e| e.path())
            .collect();
        found.sort();
        for path in found {
            any = true;
            let size = disk_size(&path);
            println!("  delete {:>10}  {}", bytes(size), shown(&path));
            freed += size;
            if yes {
                let removed = if path.is_dir() {
                    std::fs::remove_dir_all(&path)
                } else {
                    std::fs::remove_file(&path)
                };
                if let Err(e) = removed {
                    failures.push(format!("{}: {e}", shown(&path)));
                }
            }
        }
    }
    if !any {
        println!("  (none)");
    }

    // 2. Worktrees of landed branches.
    println!("\nworktrees whose branch landed on {INTEGRATION}:");
    let mut gone: Vec<&Worktree> = Vec::new();
    for (wt, decision) in worktrees.iter().zip(&decisions) {
        let label = wt.branch.as_deref().unwrap_or("(detached)");
        match decision {
            Ok(why) => {
                let size = disk_size(Path::new(&wt.path));
                println!("  delete {:>10}  {}  {label}: {why}", bytes(size), wt.path);
                freed += size;
                if !yes {
                    gone.push(wt);
                    continue;
                }
                // --force: every worktree here holds the upstream submodule, which git
                // refuses to remove without it. It was checked clean just above.
                match crate::git(&root, &["worktree", "remove", "--force", &wt.path]) {
                    Ok(_) => gone.push(wt),
                    Err(e) => failures.push(e),
                }
            }
            Err(why) => println!("  keep   {:>10}  {}  {label}: {why}", "", wt.path),
        }
    }

    // 3. Docker volumes: this repository's, whose checkout is gone.
    println!("\nDocker volumes {VOLUME_PREFIX}* (scripts/rocky9.sh build directories):");
    match volumes() {
        Err(e) => println!("  not checked: {e}"),
        Ok(volumes) => {
            let owners = Owners::new(&root, &worktrees, &gone)?;
            for v in &volumes {
                let (delete, why) = owners.verdict(v, unlabelled);
                if !delete {
                    println!("  keep   {:>10}  {}  {why}", v.size, v.name);
                    continue;
                }
                let in_use = if v.links > 0 {
                    format!(" (used by {} container(s): deleting it will fail)", v.links)
                } else {
                    String::new()
                };
                println!("  delete {:>10}  {}  {why}{in_use}", v.size, v.name);
                freed += v.bytes;
                if yes && let Err(e) = docker(&["volume", "rm", &v.name]) {
                    failures.push(format!("{}: {e}", v.name));
                }
            }
        }
    }

    println!();
    if yes {
        println!("clean-scratch: freed about {}", bytes(freed));
    } else {
        println!(
            "clean-scratch: would free about {}. Run `cargo xtask clean-scratch --yes` to delete.",
            bytes(freed)
        );
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "some deletions failed:\n  {}",
            failures.join("\n  ")
        ))
    }
}

/// Why `wt` may be removed, or why it stays.
fn removable(root: &Path, wt: &Worktree, landed: &Landed) -> Result<String, String> {
    if same_path(&wt.path, &root.to_string_lossy()) {
        return Err("the checkout running this command".into());
    }
    if wt.locked {
        return Err("locked (`git worktree lock`)".into());
    }
    if wt.prunable || !Path::new(&wt.path).is_dir() {
        return Err("its directory is missing (`git worktree prune` forgets it)".into());
    }
    let Some(branch) = &wt.branch else {
        return Err("detached HEAD: no branch to have landed".into());
    };
    let why = landed.why(branch, &wt.head).ok_or("not landed")?;
    if !crate::git(Path::new(&wt.path), crate::STATUS_ALL)?
        .trim()
        .is_empty()
    {
        return Err(format!(
            "{why}, but it has uncommitted changes or untracked files"
        ));
    }
    Ok(why)
}

/// Which branch tips have landed on phase0, and how. A worktree counts as landed only when its
/// branch's name and tip both match: a new branch that merely starts at a landed commit has
/// not landed.
struct Landed {
    /// Whether this repository has a phase0 branch at all.
    available: bool,
    /// (branch, tip) named by `Landed-From:` trailers of phase0's merges (`xtask land`
    /// replays branches, so their commits are not in phase0 when phase0 had moved).
    trailers: HashSet<(String, String)>,
    /// (tip merged, merge commit, merge subject) for every non-first parent of phase0's own
    /// merges (on its first-parent line).
    merges: Vec<(String, String, String)>,
}

impl Landed {
    fn load(root: &Path) -> Result<Landed, String> {
        let exists = crate::git(
            root,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{INTEGRATION}"),
            ],
        )
        .is_ok();
        if !exists {
            return Ok(Landed {
                available: false,
                trailers: HashSet::new(),
                merges: Vec::new(),
            });
        }
        let log = crate::git(
            root,
            &[
                "log",
                "--first-parent",
                "--merges",
                "--format=%H %P%x00%B%x01",
                INTEGRATION,
            ],
        )?;
        Ok(parse_landed(&log))
    }

    /// How `branch` at `tip` landed, if it did.
    fn why(&self, branch: &str, tip: &str) -> Option<String> {
        if self
            .trailers
            .contains(&(branch.to_string(), tip.to_string()))
        {
            return Some(format!("landed by `xtask land` ({LANDED_FROM} trailer)"));
        }
        self.merges
            .iter()
            .find(|(merged, _, subject)| merged == tip && names_branch(subject, branch))
            .map(|(_, merge, subject)| {
                format!(
                    "merged into {INTEGRATION} by {} \"{subject}\"",
                    &merge[..merge.len().min(10)]
                )
            })
    }
}

/// `git log --format=%H %P%x00%B%x01` of phase0's first-parent merges.
fn parse_landed(log: &str) -> Landed {
    let mut trailers = HashSet::new();
    let mut merges = Vec::new();
    for record in log.split('\u{1}') {
        let Some((commits, message)) = record.trim_start().split_once('\0') else {
            continue;
        };
        let mut shas = commits.split_whitespace();
        let Some(merge) = shas.next() else { continue };
        let subject = message.lines().next().unwrap_or("").trim().to_string();
        for merged in shas.skip(1) {
            merges.push((merged.to_string(), merge.to_string(), subject.clone()));
        }
        for line in message.lines() {
            let mut value = match line.trim().strip_prefix(LANDED_FROM) {
                Some(v) => v.split_whitespace(),
                None => continue,
            };
            if let (Some(branch), Some(tip)) = (value.next(), value.next()) {
                trailers.insert((branch.to_string(), tip.to_string()));
            }
        }
    }
    Landed {
        available: true,
        trailers,
        merges,
    }
}

/// Whether a merge subject names `branch` as the branch it merges: `Merge <branch>` (then the
/// end, a space, `:` or `(`), git's `Merge branch '<branch>'`, or GitHub's
/// `Merge pull request #<n> from <owner>/<branch>`.
fn names_branch(subject: &str, branch: &str) -> bool {
    if let Some(rest) = subject
        .strip_prefix("Merge ")
        .and_then(|s| s.strip_prefix(branch))
        && (rest.is_empty() || rest.starts_with([' ', ':', '(']))
    {
        return true;
    }
    if subject.starts_with(&format!("Merge branch '{branch}'")) {
        return true;
    }
    subject.starts_with("Merge pull request #")
        && subject
            .split_once(" from ")
            .and_then(|(_, from)| from.split_once('/'))
            .is_some_and(|(_, b)| b == branch)
}

/// An entry of `git worktree list --porcelain`.
#[derive(Debug, Default)]
struct Worktree {
    path: String,
    head: String,
    branch: Option<String>,
    locked: bool,
    prunable: bool,
}

fn worktrees(root: &Path) -> Result<Vec<Worktree>, String> {
    Ok(parse_worktrees(&crate::git(
        root,
        &["worktree", "list", "--porcelain"],
    )?))
}

fn parse_worktrees(list: &str) -> Vec<Worktree> {
    let mut out = Vec::new();
    for record in list.split("\n\n") {
        let mut wt = Worktree::default();
        for line in record.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                wt.path = path.to_string();
            } else if let Some(head) = line.strip_prefix("HEAD ") {
                wt.head = head.to_string();
            } else if let Some(branch) = line.strip_prefix("branch ") {
                wt.branch = Some(branch.trim_start_matches("refs/heads/").to_string());
            } else if line == "locked" || line.starts_with("locked ") {
                wt.locked = true;
            } else if line == "prunable" || line.starts_with("prunable ") {
                wt.prunable = true;
            }
        }
        if !wt.path.is_empty() {
            out.push(wt);
        }
    }
    out
}

/// A Docker volume `ocio-rs-target-*`, with its size and the labels `scripts/rocky9.sh` gives it.
#[derive(Debug, Default)]
struct Volume {
    name: String,
    /// As Docker prints it, e.g. `2.103GB`.
    size: String,
    bytes: u64,
    /// Containers using it.
    links: u64,
    /// The checkout it was created for (`ocio-rs.checkout`).
    checkout: Option<String>,
    /// That checkout's git common directory (`ocio-rs.git-common-dir`): the repository.
    repository: Option<String>,
}

/// The `ocio-rs-target-*` volumes, sorted by name: sizes from `docker system df -v`, labels
/// from `docker volume inspect`.
fn volumes() -> Result<Vec<Volume>, String> {
    let out = docker(&["system", "df", "-v", "--format", "{{json .Volumes}}"])?;
    let list: Vec<serde_json::Value> =
        serde_json::from_str(out.trim()).map_err(|e| format!("docker system df: {e}"))?;
    let mut volumes: Vec<Volume> = list
        .iter()
        .filter(|v| v["Name"].as_str().unwrap_or("").starts_with(VOLUME_PREFIX))
        .map(|v| {
            let field = |k: &str| v[k].as_str().unwrap_or("").to_string();
            let size = field("Size");
            Volume {
                name: field("Name"),
                bytes: docker_bytes(&size),
                size,
                links: field("Links").parse().unwrap_or(0),
                ..Volume::default()
            }
        })
        .collect();
    if volumes.is_empty() {
        return Ok(volumes);
    }
    let mut args = vec!["volume", "inspect"];
    let names: Vec<String> = volumes.iter().map(|v| v.name.clone()).collect();
    args.extend(names.iter().map(String::as_str));
    let inspected: Vec<serde_json::Value> = serde_json::from_str(docker(&args)?.trim())
        .map_err(|e| format!("docker volume inspect: {e}"))?;
    for info in &inspected {
        let label = |k: &str| info["Labels"][k].as_str().map(str::to_string);
        if let Some(v) = volumes.iter_mut().find(|v| info["Name"] == v.name.as_str()) {
            v.checkout = label(CHECKOUT_LABEL);
            v.repository = label(REPOSITORY_LABEL);
        }
    }
    volumes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(volumes)
}

/// What decides whose a volume is.
struct Owners {
    /// This repository's git common directory, canonical.
    repository: String,
    /// Canonical paths of the worktrees that stay.
    remaining: HashSet<String>,
    /// Canonical paths of the worktrees deleted in this run.
    gone: HashSet<String>,
    /// `xtask land`'s worktree: its volume stays warm between lands, while the worktree is gone.
    land: String,
    /// Legacy volume ids (see `legacy_forms`) of the worktrees whose `scripts/rocky9.sh` still
    /// makes unlabelled volumes, and those worktrees.
    legacy_users: Vec<(String, String)>,
}

impl Owners {
    fn new(root: &Path, worktrees: &[Worktree], gone: &[&Worktree]) -> Result<Owners, String> {
        let repository = crate::git(
            root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        let is_gone = |w: &Worktree| gone.iter().any(|g| g.path == w.path);
        let stays = |w: &&Worktree| !is_gone(w) && Path::new(&w.path).is_dir();
        let mut legacy_users = Vec::new();
        for wt in worktrees.iter().filter(stays) {
            let script = std::fs::read_to_string(Path::new(&wt.path).join("scripts/rocky9.sh"))
                .unwrap_or_default();
            if !script.is_empty() && !script.contains(CHECKOUT_LABEL) {
                for form in legacy_forms(&wt.path) {
                    legacy_users.push((volume_id(&form), wt.path.clone()));
                }
            }
        }
        Ok(Owners {
            repository: canonical_path(repository.trim()),
            remaining: worktrees
                .iter()
                .filter(stays)
                .map(|w| canonical_path(&w.path))
                .collect(),
            gone: gone.iter().map(|w| canonical_path(&w.path)).collect(),
            land: canonical_path(&format!(
                "{}/target/land/wt",
                worktrees.first().map_or("", |w| w.path.as_str())
            )),
            legacy_users,
        })
    }

    /// Whether to delete `v`, and why.
    fn verdict(&self, v: &Volume, unlabelled: bool) -> (bool, String) {
        match (&v.checkout, &v.repository) {
            (Some(checkout), Some(repo)) if canonical_path(repo) == self.repository => {
                let c = canonical_path(checkout);
                if c == self.land {
                    (false, format!("xtask land's build volume ({checkout})"))
                } else if self.remaining.contains(&c) {
                    (false, checkout.clone())
                } else if self.gone.contains(&c) {
                    (true, format!("its worktree {checkout} is deleted above"))
                } else if !Path::new(checkout).is_dir() {
                    (true, format!("its checkout {checkout} no longer exists"))
                } else {
                    (
                        false,
                        format!("{checkout} exists but is not a worktree of this repository"),
                    )
                }
            }
            (Some(checkout), Some(repo)) if !repo.is_empty() => {
                let note = if Path::new(repo).exists() {
                    ""
                } else {
                    "; that repository no longer exists (`docker volume rm` it if it is not coming back)"
                };
                (false, format!("another repository's: {checkout}{note}"))
            }
            (Some(checkout), _) => (
                false,
                format!("labelled without a repository, not attributable: {checkout}"),
            ),
            (None, _) => {
                let id = &v.name[VOLUME_PREFIX.len()..];
                if let Some((_, user)) = self.legacy_users.iter().find(|(i, _)| i == id) {
                    (
                        false,
                        format!("unlabelled: {user}'s older scripts/rocky9.sh still uses it"),
                    )
                } else if unlabelled {
                    (
                        true,
                        "unlabelled, and no checkout of this repository still uses it \
                         (a volume of another clone would look the same)"
                            .to_string(),
                    )
                } else {
                    (
                        false,
                        "unlabelled (an older scripts/rocky9.sh made it), so not attributable; \
                         no checkout of this repository still uses it: --unlabelled deletes it"
                            .to_string(),
                    )
                }
            }
        }
    }
}

fn docker(args: &[&str]) -> Result<String, String> {
    let out = Command::new("docker")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("docker is not available ({e})"))?;
    if !out.status.success() {
        return Err(format!(
            "`docker {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Docker's decimal sizes (`416.2MB`, `2.103GB`, `0B`) in bytes.
fn docker_bytes(size: &str) -> u64 {
    let split = size
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(size.len());
    let (number, unit) = size.split_at(split);
    let scale = match unit.trim() {
        "B" | "" => 1.0,
        "kB" | "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        _ => 0.0,
    };
    (number.parse::<f64>().unwrap_or(0.0) * scale) as u64
}

/// A path in the canonical form `scripts/rocky9.sh` hashes into volume ids: forward slashes, no
/// trailing slash, and Windows paths (a drive letter or `//`) in ASCII lower case, since Windows
/// ignores case.
pub(crate) fn canonical_path(path: &str) -> String {
    let path = crate::plain_path(Path::new(path))
        .to_string_lossy()
        .replace('\\', "/");
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic();
    let mut path = path.as_str();
    if !(drive && path.len() == 3) {
        path = path.trim_end_matches('/');
    }
    if drive || path.starts_with("//") {
        path.to_ascii_lowercase()
    } else {
        path.to_string()
    }
}

/// The forms of a worktree path that an older `scripts/rocky9.sh` hashed into unlabelled volume
/// ids (Git Bash's `pwd`): as git lists it and as the file system spells it.
fn legacy_forms(path: &str) -> Vec<String> {
    let mut forms = vec![legacy_path(path)];
    if let Ok(canonical) = std::fs::canonicalize(path) {
        let canonical = legacy_path(&crate::plain_path(&canonical).to_string_lossy());
        if !forms.contains(&canonical) {
            forms.push(canonical);
        }
    }
    forms
}

/// A checkout path as an older `scripts/rocky9.sh` hashed it: `pwd` in Git Bash on Windows
/// (`D:\Projects\x` -> `/d/Projects/x`), the path itself elsewhere.
fn legacy_path(path: &str) -> String {
    let path = crate::plain_path(Path::new(path))
        .to_string_lossy()
        .replace('\\', "/");
    let path = path.trim_end_matches('/');
    let bytes = path.as_bytes();
    if cfg!(windows) && bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        format!(
            "/{}{}",
            char::from(bytes[0].to_ascii_lowercase()),
            &path[2..]
        )
    } else {
        path.to_string()
    }
}

/// `scripts/rocky9.sh`: `printf '%s' "$path" | md5sum | cut -c1-12`.
fn volume_id(path: &str) -> String {
    md5(path.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()[..12]
        .to_string()
}

/// MD5 (RFC 1321), for the volume ids only.
fn md5(input: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];
    let mut state: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    let mut message = input.to_vec();
    let bit_len = (input.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_le_bytes());
    for block in message.as_chunks::<64>().0 {
        let m: [u32; 16] = std::array::from_fn(|i| {
            u32::from_le_bytes([
                block[4 * i],
                block[4 * i + 1],
                block[4 * i + 2],
                block[4 * i + 3],
            ])
        });
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(K[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        for (s, v) in state.iter_mut().zip([a, b, c, d]) {
            *s = s.wrapping_add(v);
        }
    }
    let mut out = [0u8; 16];
    for (chunk, s) in out.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        *chunk = s.to_le_bytes();
    }
    out
}

/// Bytes on disk under `path`, without following links.
fn disk_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    std::fs::read_dir(path)
        .map(|entries| entries.flatten().map(|e| disk_size(&e.path())).sum())
        .unwrap_or(0)
}

fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn shown(path: &Path) -> String {
    crate::plain_path(path).to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: [u8; 16]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn md5_matches_rfc_1321() {
        // The test suite of RFC 1321, appendix A.5.
        assert_eq!(hex(md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(md5(b"a")), "0cc175b9c0f1b6a831c399e269772661");
        assert_eq!(hex(md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hex(md5(b"message digest")),
            "f96b697d7cb7938d525a2f31aaf161d0"
        );
        assert_eq!(
            hex(md5(b"abcdefghijklmnopqrstuvwxyz")),
            "c3fcd3d76192e4007dfb496cca67e13b"
        );
        assert_eq!(
            hex(md5(
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890"
            )),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }

    #[test]
    fn volume_ids_match_rocky9_sh() {
        // scripts/rocky9.sh in Git Bash, from D:\Projects\OpenColorIO-rs-wt\tooling-xtask, typed
        // in either case: `pwd -W` is D:/Projects/... (or d:/projects/...), its canonical form
        // d:/projects/opencolorio-rs-wt/tooling-xtask, and md5sum gives 76d19c61b038.
        for typed in [
            r"D:\Projects\OpenColorIO-rs-wt\tooling-xtask",
            "d:/projects/opencolorio-rs-wt/tooling-xtask",
            r"\\?\D:\Projects\OpenColorIO-rs-wt\tooling-xtask\",
        ] {
            assert_eq!(
                canonical_path(typed),
                "d:/projects/opencolorio-rs-wt/tooling-xtask"
            );
        }
        assert_eq!(
            volume_id(&canonical_path(
                "D:/Projects/OpenColorIO-rs-wt/tooling-xtask"
            )),
            "76d19c61b038"
        );
        assert_eq!(canonical_path("D:/"), "d:/");
        assert_eq!(canonical_path("//Server/Share/x/"), "//server/share/x");
        assert_eq!(canonical_path("/home/U/ocio/"), "/home/U/ocio");
        // The older scripts/rocky9.sh hashed Git Bash's `pwd`: `printf '%s'
        // /d/Projects/OpenColorIO-rs | md5sum | cut -c1-12` is b6a2d90b5ed0.
        assert_eq!(volume_id("/d/Projects/OpenColorIO-rs"), "b6a2d90b5ed0");
        if cfg!(windows) {
            assert_eq!(
                legacy_path(r"D:\Projects\OpenColorIO-rs-wt\commit-s1"),
                "/d/Projects/OpenColorIO-rs-wt/commit-s1"
            );
            assert_eq!(legacy_path(r"\\?\D:\Projects\x\"), "/d/Projects/x");
        } else {
            assert_eq!(legacy_path("/home/u/ocio/"), "/home/u/ocio");
        }
    }

    #[test]
    fn a_branch_landed_only_when_its_name_and_tip_match() {
        // The shape of `git log --first-parent --merges --format=%H %P%x00%B%x01`.
        let log = "m1 p1 t1\0Merge card/a (2 chunks)\n\n- one\n\nLanded-From: card/a o1\n\
                   Co-Authored-By: x\n\u{1}\n\
                   m2 p2 t2\0Merge card/sde-ci: the CPU tests\n\u{1}\n\
                   m3 p3 t3\0Merge branch 'fix/x' into phase0\n\u{1}\n\
                   m4 p4 t4\0Merge pull request #7 from Pixel-Minions/card/pr\n\u{1}\n\
                   m5 p5 t5\0Merge S4: CPU dispatch\n\u{1}\n";
        let landed = parse_landed(log);
        let landed_as = |branch: &str, tip: &str| landed.why(branch, tip).is_some();
        // land's trailer names the branch and its original tip.
        assert!(landed_as("card/a", "o1"));
        assert!(!landed_as("vt/chain", "o1"));
        assert!(!landed_as("card/a", "t9"));
        // A merge commit's non-first parent, and a subject naming that branch.
        assert!(landed_as("card/sde-ci", "t2"));
        assert!(!landed_as("vt/chain", "t2"));
        assert!(!landed_as("card/sde", "t2"));
        assert!(landed_as("fix/x", "t3"));
        assert!(landed_as("card/pr", "t4"));
        // A subject that doesn't name the branch, or the first parent: not landed.
        assert!(!landed_as("chunks/s4", "t5"));
        assert!(!landed_as("card/sde-ci", "p2"));
    }

    #[test]
    fn volumes_are_deleted_only_when_this_repository_labels_them_and_their_checkout_is_gone() {
        let owners = Owners {
            repository: canonical_path("D:/r/.git"),
            remaining: [canonical_path("D:/r"), canonical_path("D:/wt/a")].into(),
            gone: [canonical_path("D:/wt/landed")].into(),
            land: canonical_path("D:/r/target/land/wt"),
            legacy_users: vec![("0123456789ab".into(), "D:/wt/old".into())],
        };
        let volume = |name: &str, checkout: Option<&str>, repository: Option<&str>| Volume {
            name: name.into(),
            checkout: checkout.map(String::from),
            repository: repository.map(String::from),
            ..Volume::default()
        };
        let delete = |v: &Volume, unlabelled: bool| owners.verdict(v, unlabelled).0;
        let ours = |checkout: &str| volume("ocio-rs-target-x", Some(checkout), Some("d:/R/.GIT"));
        // This repository's: kept while the checkout is a worktree, and for land's worktree.
        assert!(!delete(&ours("D:/R"), false));
        assert!(!delete(&ours("d:/wt/A"), false));
        assert!(!delete(&ours("D:/r/target/land/wt"), false));
        // Deleted with its worktree, or once the checkout no longer exists.
        assert!(delete(&ours("D:/wt/landed"), false));
        assert!(delete(&ours("Z:/no/such/checkout/ocio-rs-test"), false));
        // Another repository's, unlabelled or unattributable: kept.
        let other = volume("ocio-rs-target-y", Some("Z:/gone"), Some("Z:/gone/.git"));
        assert!(!delete(&other, true));
        assert!(!delete(
            &volume("ocio-rs-target-z", Some("Z:/gone"), None),
            true
        ));
        let legacy = volume("ocio-rs-target-fedcba987654", None, None);
        assert!(!delete(&legacy, false));
        // --unlabelled deletes unlabelled volumes, except one an older script still uses.
        assert!(delete(&legacy, true));
        assert!(!delete(
            &volume("ocio-rs-target-0123456789ab", None, None),
            true
        ));
    }

    #[test]
    fn docker_sizes() {
        assert_eq!(docker_bytes("0B"), 0);
        assert_eq!(docker_bytes("416.2MB"), 416_200_000);
        assert_eq!(docker_bytes("2.103GB"), 2_103_000_000);
        assert_eq!(docker_bytes("12kB"), 12_000);
    }

    #[test]
    fn parses_worktree_list() {
        // The shape of `git worktree list --porcelain`: one record per worktree.
        let out = parse_worktrees(
            "worktree D:/p/main\nHEAD aaa\nbranch refs/heads/phase0\n\n\
             worktree D:/p/wt\nHEAD bbb\ndetached\nlocked reason\n\n\
             worktree D:/p/gone\nHEAD ccc\nbranch refs/heads/card/x\nprunable gitdir file points to non-existent location\n",
        );
        assert_eq!(out.len(), 3);
        assert_eq!(
            (out[0].path.as_str(), out[0].head.as_str()),
            ("D:/p/main", "aaa")
        );
        assert_eq!(out[0].branch.as_deref(), Some("phase0"));
        assert!(out[1].branch.is_none() && out[1].locked && !out[1].prunable);
        assert_eq!(out[2].branch.as_deref(), Some("card/x"));
        assert!(out[2].prunable && !out[2].locked);
    }

    #[test]
    fn human_sizes() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1536), "1.5 KiB");
        assert_eq!(bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
    }
}
