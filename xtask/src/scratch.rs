// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `cargo xtask clean-scratch [--yes]`: lists, and with `--yes` deletes, scratch that piles up
//! as cards land:
//! - verifier and probe output, `target/verify*`, in the main checkout and every worktree;
//! - worktrees whose branch has landed on `phase0`, when they have nothing uncommitted;
//! - Docker volumes `ocio-rs-target-<id>` (the Rocky Linux 9 build directories of
//!   `scripts/rocky9.sh`) whose id matches no worktree that remains.
//!
//! It never deletes anything else.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::land::{LANDED_FROM, same_path};

const INTEGRATION: &str = "phase0";
const VOLUME_PREFIX: &str = "ocio-rs-target-";

pub(crate) fn run(args: &[&str]) -> Result<(), String> {
    let yes = match args {
        [] => false,
        ["--yes"] => true,
        _ => return Err("usage: cargo xtask clean-scratch [--yes]".into()),
    };
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

    // 3. Docker volumes of no remaining worktree.
    println!("\nDocker volumes {VOLUME_PREFIX}* (scripts/rocky9.sh build directories):");
    match volumes() {
        Err(e) => println!("  not checked: {e}"),
        Ok(mut volumes) => {
            // Volume id -> (worktree, whether it is gone: deleted in this run, or its
            // directory no longer exists).
            let mut ids: Vec<(String, &Worktree, bool)> = Vec::new();
            for wt in &worktrees {
                let going = gone.iter().any(|g| g.path == wt.path) || !Path::new(&wt.path).is_dir();
                for form in path_forms(&wt.path) {
                    ids.push((volume_id(&form), wt, going));
                }
            }
            volumes.sort_by(|a, b| a.name.cmp(&b.name));
            for v in volumes.iter().filter(|v| v.name.starts_with(VOLUME_PREFIX)) {
                let id = &v.name[VOLUME_PREFIX.len()..];
                let owner = ids.iter().find(|(i, _, going)| i == id && !going);
                if let Some((_, wt, _)) = owner {
                    println!("  keep   {:>10}  {}  {}", v.size, v.name, wt.path);
                    continue;
                }
                let why = match ids.iter().find(|(i, _, _)| i == id) {
                    Some((_, wt, _)) if gone.iter().any(|g| g.path == wt.path) => {
                        format!("its worktree {} is deleted above", wt.path)
                    }
                    Some((_, wt, _)) => format!("its worktree {} no longer exists", wt.path),
                    None => "no worktree of this repository has this id".to_string(),
                };
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
    if wt.branch.is_none() {
        return Err("detached HEAD: no branch to have landed".into());
    }
    let why = landed.why(&wt.head)?.ok_or("not landed")?;
    let status = crate::git(
        Path::new(&wt.path),
        &["status", "--porcelain", "--ignore-submodules=none"],
    )?;
    if !status.trim().is_empty() {
        return Err(format!(
            "{why}, but it has uncommitted changes or untracked files"
        ));
    }
    Ok(why)
}

/// What says a branch tip has landed on phase0.
struct Landed {
    root: PathBuf,
    /// Commits of phase0's own line (first parents): a branch tip there never landed as a
    /// card, it is just where a new branch starts.
    first_parent: HashSet<String>,
    /// Branch tips named by `Landed-From:` trailers of phase0's merges (`xtask land` replays
    /// branches, so their own commits are not in phase0 when phase0 had moved).
    trailers: HashSet<String>,
}

impl Landed {
    fn load(root: &Path) -> Result<Landed, String> {
        let first_parent = crate::git(root, &["rev-list", "--first-parent", INTEGRATION])?
            .lines()
            .map(str::to_string)
            .collect();
        let trailers = crate::git(
            root,
            &[
                "log",
                "--first-parent",
                "--merges",
                "--format=%B",
                INTEGRATION,
            ],
        )?
        .lines()
        .filter_map(|l| l.trim().strip_prefix(LANDED_FROM))
        .filter_map(|v| v.split_whitespace().nth(1))
        .map(str::to_string)
        .collect();
        Ok(Landed {
            root: root.to_path_buf(),
            first_parent,
            trailers,
        })
    }

    fn why(&self, tip: &str) -> Result<Option<String>, String> {
        if self.trailers.contains(tip) {
            return Ok(Some(format!(
                "landed by `xtask land` ({LANDED_FROM} trailer)"
            )));
        }
        if self.first_parent.contains(tip) {
            return Ok(None);
        }
        let merged = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["merge-base", "--is-ancestor", tip, INTEGRATION])
            .stdin(Stdio::null())
            .status()
            .map_err(|e| format!("could not run git: {e}"))?;
        Ok(merged
            .success()
            .then(|| format!("merged into {INTEGRATION}")))
    }
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

/// A Docker volume from `docker system df -v`.
#[derive(Debug)]
struct Volume {
    name: String,
    /// As Docker prints it, e.g. `2.103GB`.
    size: String,
    bytes: u64,
    /// Containers using it.
    links: u64,
}

fn volumes() -> Result<Vec<Volume>, String> {
    let out = docker(&["system", "df", "-v", "--format", "{{json .Volumes}}"])?;
    let list: Vec<serde_json::Value> =
        serde_json::from_str(out.trim()).map_err(|e| format!("docker system df: {e}"))?;
    Ok(list
        .iter()
        .map(|v| {
            let field = |k: &str| v[k].as_str().unwrap_or("").to_string();
            let size = field("Size");
            Volume {
                name: field("Name"),
                bytes: docker_bytes(&size),
                size,
                links: field("Links").parse().unwrap_or(0),
            }
        })
        .collect())
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

/// The forms of a worktree path that `scripts/rocky9.sh` may have hashed: as git lists it and
/// as the file system spells it (Windows paths are case-insensitive).
fn path_forms(path: &str) -> Vec<String> {
    let mut forms = vec![rocky_path(path)];
    if let Ok(canonical) = std::fs::canonicalize(path) {
        let canonical = rocky_path(&crate::plain_path(&canonical).to_string_lossy());
        if !forms.contains(&canonical) {
            forms.push(canonical);
        }
    }
    forms
}

/// A checkout path as `scripts/rocky9.sh` sees it: `pwd` in Git Bash on Windows
/// (`D:\Projects\x` -> `/d/Projects/x`), the path itself elsewhere.
fn rocky_path(path: &str) -> String {
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

/// `scripts/rocky9.sh`: `printf '%s' "$root" | md5sum | cut -c1-12`.
fn volume_id(rocky_path: &str) -> String {
    md5(rocky_path.as_bytes())
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
        // `printf '%s' /d/Projects/OpenColorIO-rs | md5sum | cut -c1-12` in Git Bash.
        assert_eq!(volume_id("/d/Projects/OpenColorIO-rs"), "b6a2d90b5ed0");
        if cfg!(windows) {
            assert_eq!(
                rocky_path(r"D:\Projects\OpenColorIO-rs-wt\commit-s1"),
                "/d/Projects/OpenColorIO-rs-wt/commit-s1"
            );
            assert_eq!(
                rocky_path("D:/Projects/OpenColorIO-rs"),
                "/d/Projects/OpenColorIO-rs"
            );
            assert_eq!(rocky_path(r"\\?\D:\Projects\x\"), "/d/Projects/x");
        } else {
            assert_eq!(rocky_path("/home/u/ocio/"), "/home/u/ocio");
        }
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
