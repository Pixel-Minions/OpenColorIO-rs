// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Deleting scratch directories without deleting through links.
//!
//! A scratch worktree may hold a link to a directory outside it: a verifier linked
//! `upstream/OpenColorIO` to the main checkout's submodule (a Windows junction) instead of
//! checking it out. `git worktree remove --force` (Git for Windows 2.53) deletes a junction's
//! target's contents along with the worktree: it removed the main checkout's submodule files.
//! So before a scratch directory is deleted, every link inside it is unlinked: the link itself
//! is removed, never what it points to.

use std::path::{Path, PathBuf};

/// Whether the entry `meta` describes (from `symlink_metadata`, which doesn't follow links)
/// is a link: a symbolic link, or on Windows any reparse point (junctions, directory and file
/// symbolic links, and the others, which a delete must not walk into either).
fn is_link(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}

/// Removes the link at `path` itself, leaving its target alone. On Windows a link to a
/// directory (a junction or a directory symbolic link) is a directory entry that
/// `RemoveDirectoryW` (`remove_dir`) deletes without entering it; on Unix every symbolic link is
/// a file (`remove_file`).
fn unlink(path: &Path, meta: &std::fs::Metadata) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
        if meta.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0 {
            return std::fs::remove_dir(path);
        }
    }
    #[cfg(not(windows))]
    let _ = meta;
    std::fs::remove_file(path)
}

/// Unlinks every link under `dir`, walking it without following links, and returns the links
/// it removed. `dir` itself must not be a link: deleting it would delete what it points to, so
/// that is refused (remove the link by hand).
pub(crate) fn unlink_all(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let shown = |p: &Path| crate::plain_path(p).to_string_lossy().replace('\\', "/");
    match std::fs::symlink_metadata(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", shown(dir))),
        Ok(meta) if is_link(&meta) => {
            return Err(format!(
                "{} is a link (a symbolic link or junction): not deleting it or what it points \
                 to; remove the link itself by hand",
                shown(dir)
            ));
        }
        Ok(_) => {}
    }
    let mut removed = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let entries =
            std::fs::read_dir(&current).map_err(|e| format!("{}: {e}", shown(&current)))?;
        for entry in entries {
            let path = entry
                .map_err(|e| format!("{}: {e}", shown(&current)))?
                .path();
            let meta =
                std::fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", shown(&path)))?;
            if is_link(&meta) {
                unlink(&path, &meta).map_err(|e| {
                    format!(
                        "{}: could not remove this link ({e}); nothing was deleted through it",
                        shown(&path)
                    )
                })?;
                removed.push(path);
            } else if meta.is_dir() {
                pending.push(path);
            }
        }
    }
    Ok(removed)
}

/// Deletes the directory or file `path`, after unlinking every link inside it (`unlink_all`).
/// When `path` is itself a link, only the link goes.
pub(crate) fn remove_tree(path: &Path) -> Result<(), String> {
    let shown = crate::plain_path(path).to_string_lossy().replace('\\', "/");
    let meta = match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("{shown}: {e}")),
        Ok(meta) => meta,
    };
    if is_link(&meta) {
        return unlink(path, &meta).map_err(|e| format!("{shown}: {e}"));
    }
    if !meta.is_dir() {
        return std::fs::remove_file(path).map_err(|e| format!("{shown}: {e}"));
    }
    unlink_all(path)?;
    std::fs::remove_dir_all(path).map_err(|e| format!("{shown}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory for one test, in the system temporary directory.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ocio-rs-xtask-links-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Makes `link` point to the directory `target`: a junction on Windows (`mklink /J`, which
    /// needs no privilege, unlike a symbolic link), a symbolic link elsewhere.
    fn link_dir(target: &Path, link: &Path) {
        #[cfg(windows)]
        {
            let out = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "mklink /J: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
    }

    /// A directory with a file, standing for the main checkout's submodule.
    fn precious(dir: &Path) -> PathBuf {
        let keep = dir.join("main").join("upstream").join("OpenColorIO");
        std::fs::create_dir_all(keep.join("src")).unwrap();
        std::fs::write(keep.join("src").join("file.cpp"), "precious").unwrap();
        keep
    }

    fn intact(keep: &Path) -> bool {
        std::fs::read_to_string(keep.join("src").join("file.cpp")).is_ok_and(|t| t == "precious")
    }

    #[test]
    fn removing_a_link_to_a_directory_keeps_its_target() {
        // The claim `unlink` rests on: on Windows, remove_dir on a junction removes the
        // junction only; on Unix, remove_file on a symbolic link removes the link only.
        let dir = scratch("unlink");
        let keep = precious(&dir);
        let link = dir.join("link");
        link_dir(&keep, &link);
        let meta = std::fs::symlink_metadata(&link).unwrap();
        assert!(is_link(&meta));
        assert!(intact(&link), "the link reaches its target");
        unlink(&link, &meta).unwrap();
        assert!(
            std::fs::symlink_metadata(&link).is_err(),
            "the link is gone"
        );
        assert!(intact(&keep), "its target is not");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_scratch_worktree_goes_without_what_its_links_point_to() {
        // A scratch worktree whose upstream/OpenColorIO links to the main checkout's submodule,
        // with another link deeper down and a link to a file.
        let dir = scratch("tree");
        let keep = precious(&dir);
        let wt = dir.join("wt");
        std::fs::create_dir_all(wt.join("upstream")).unwrap();
        std::fs::create_dir_all(wt.join("target").join("verify-x")).unwrap();
        std::fs::write(wt.join("Cargo.toml"), "scratch").unwrap();
        link_dir(&keep, &wt.join("upstream").join("OpenColorIO"));
        link_dir(&keep, &wt.join("target").join("verify-x").join("deep"));
        #[cfg(unix)]
        std::os::unix::fs::symlink(keep.join("src").join("file.cpp"), wt.join("file-link"))
            .unwrap();

        let mut removed = unlink_all(&wt).unwrap();
        removed.sort();
        let mut expected = vec![
            wt.join("target").join("verify-x").join("deep"),
            wt.join("upstream").join("OpenColorIO"),
        ];
        if cfg!(unix) {
            expected.push(wt.join("file-link"));
        }
        expected.sort();
        assert_eq!(removed, expected);
        assert!(wt.join("Cargo.toml").exists(), "only links are removed");
        assert!(intact(&keep));

        link_dir(&keep, &wt.join("upstream").join("OpenColorIO"));
        remove_tree(&wt).unwrap();
        assert!(!wt.exists());
        assert!(intact(&keep));
        // Nothing left to remove is not an error.
        remove_tree(&wt).unwrap();
        assert_eq!(unlink_all(&wt).unwrap(), Vec::<PathBuf>::new());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn remove_worktree_keeps_what_a_junction_in_it_points_to() {
        // The case that lost the main checkout's submodule: a registered worktree whose
        // upstream/OpenColorIO links to it, removed as land, gate --staged and clean-scratch
        // remove theirs.
        let dir = scratch("git");
        let keep = precious(&dir);
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| crate::git(&repo, args).unwrap().trim().to_string();
        git(&["init", "--quiet"]);
        // The empty tree, which git always has.
        let commit = git(&[
            "-c",
            "user.name=xtask test",
            "-c",
            "user.email=test@localhost",
            "commit-tree",
            "4b825dc642cb6eb9a060e54bf8d69288fbee4904",
            "-m",
            "empty",
        ]);
        let wt = dir.join("wt");
        git(&[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            wt.to_str().unwrap(),
            &commit,
        ]);
        std::fs::create_dir_all(wt.join("upstream")).unwrap();
        link_dir(&keep, &wt.join("upstream").join("OpenColorIO"));
        assert!(intact(&wt.join("upstream").join("OpenColorIO")));

        crate::land::remove_worktree(&repo, &wt).unwrap();
        assert!(
            std::fs::symlink_metadata(&wt).is_err(),
            "the worktree is gone"
        );
        assert!(intact(&keep), "what its link pointed to is not");
        let list = git(&["worktree", "list", "--porcelain"]);
        assert_eq!(list.matches("worktree ").count(), 1, "{list}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_scratch_directory_that_is_itself_a_link() {
        let dir = scratch("root");
        let keep = precious(&dir);
        let wt = dir.join("wt");
        link_dir(&keep, &wt);
        // A worktree that is a link is refused (remove_worktree unlinks first).
        let err = unlink_all(&wt).unwrap_err();
        assert!(err.contains("is a link"), "{err}");
        assert!(intact(&keep));
        assert!(std::fs::symlink_metadata(&wt).is_ok(), "the link stays too");
        // A scratch directory (target/verify*) that is a link: only the link goes.
        remove_tree(&wt).unwrap();
        assert!(std::fs::symlink_metadata(&wt).is_err());
        assert!(intact(&keep));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
