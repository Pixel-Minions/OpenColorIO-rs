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

/// Whether the entry `meta` describes (from `symlink_metadata`, which doesn't follow links) is
/// a link. Rust's `FileType::is_symlink` is true for symbolic links and, on Windows, for every
/// name-surrogate reparse point: junctions (mount points) and symbolic links, to directories
/// or files. Other reparse points (cloud placeholders, deduplicated files, AF_UNIX sockets) are
/// the files and directories they hold, and a delete removes them as such.
pub(crate) fn is_link(meta: &std::fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}

/// Removes the link at `path` itself, leaving its target alone. On Windows a link to a
/// directory (a junction or a directory symbolic link, `is_symlink_dir`) is a directory entry
/// that `RemoveDirectoryW` (`remove_dir`) deletes without entering it; a link to a file, and
/// every symbolic link on Unix, is a file (`remove_file`).
fn unlink(path: &Path, meta: &std::fs::Metadata) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;
        if meta.file_type().is_symlink_dir() {
            return std::fs::remove_dir(path);
        }
    }
    #[cfg(not(windows))]
    let _ = meta;
    std::fs::remove_file(path)
}

/// `path` for messages, with `/`.
fn shown(path: &Path) -> String {
    crate::plain_path(path).to_string_lossy().replace('\\', "/")
}

/// Unlinks every link under `dir`, walking it without following links, and returns the links
/// it removed. `dir` itself must not be a link: deleting it would delete what it points to, so
/// that is refused (remove the link by hand).
pub(crate) fn unlink_all(dir: &Path) -> Result<Vec<PathBuf>, String> {
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
    let meta = match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("{}: {e}", shown(path))),
        Ok(meta) => meta,
    };
    if is_link(&meta) {
        return unlink(path, &meta).map_err(|e| format!("{}: {e}", shown(path)));
    }
    if !meta.is_dir() {
        return std::fs::remove_file(path).map_err(|e| format!("{}: {e}", shown(path)));
    }
    unlink_all(path)?;
    std::fs::remove_dir_all(path).map_err(|e| format!("{}: {e}", shown(path)))
}

/// Helpers for the tests of code that deletes scratch directories.
#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    /// A test's own directory in the system temporary directory, deleted when the test ends,
    /// however it ends: its links first, so that a failing test leaves no junction behind.
    pub(crate) struct Scratch(PathBuf);

    impl Scratch {
        pub(crate) fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir()
                .join(format!("ocio-rs-xtask-links-{name}-{}", std::process::id()));
            let scratch = Scratch(dir);
            scratch.clear();
            std::fs::create_dir_all(&scratch.0).unwrap();
            scratch
        }

        pub(crate) fn path(&self) -> &Path {
            &self.0
        }

        fn clear(&self) {
            let _ = unlink_all(&self.0);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            self.clear();
        }
    }

    /// Makes `link` point to the directory `target`: a junction on Windows (`mklink /J`, which
    /// needs no privilege, unlike a symbolic link), a symbolic link elsewhere.
    pub(crate) fn link_dir(target: &Path, link: &Path) {
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

    /// A directory with a file under `dir`, standing for the main checkout's submodule.
    pub(crate) fn precious(dir: &Path) -> PathBuf {
        let keep = dir.join("main").join("upstream").join("OpenColorIO");
        std::fs::create_dir_all(keep.join("src")).unwrap();
        std::fs::write(keep.join("src").join("file.cpp"), "precious").unwrap();
        keep
    }

    /// Whether `precious`'s directory (or a link to it) still holds its file.
    pub(crate) fn intact(keep: &Path) -> bool {
        std::fs::read_to_string(keep.join("src").join("file.cpp")).is_ok_and(|t| t == "precious")
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    #[test]
    fn removing_a_link_to_a_directory_keeps_its_target() {
        // The claims `is_link` and `unlink` rest on: a junction (Windows) or symbolic link
        // (Unix) is `is_symlink`, a junction `is_symlink_dir` too; remove_dir on a junction,
        // or remove_file on a symbolic link, removes the link only.
        let dir = Scratch::new("unlink");
        let keep = precious(dir.path());
        let link = dir.path().join("link");
        link_dir(&keep, &link);
        let meta = std::fs::symlink_metadata(&link).unwrap();
        assert!(meta.file_type().is_symlink());
        assert!(is_link(&meta));
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileTypeExt;
            assert!(meta.file_type().is_symlink_dir());
        }
        assert!(!meta.is_dir(), "a link is not walked into as a directory");
        assert!(intact(&link), "the link reaches its target");
        unlink(&link, &meta).unwrap();
        assert!(
            std::fs::symlink_metadata(&link).is_err(),
            "the link is gone"
        );
        assert!(intact(&keep), "its target is not");
    }

    #[test]
    fn a_scratch_worktree_goes_without_what_its_links_point_to() {
        // A scratch worktree whose upstream/OpenColorIO links to the main checkout's submodule,
        // with another link deeper down and a link to a file.
        let dir = Scratch::new("tree");
        let keep = precious(dir.path());
        let wt = dir.path().join("wt");
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
    }

    /// A git repository in `dir` with a registered worktree `dir/wt` whose
    /// upstream/OpenColorIO is a link to `keep`. Returns the repository and the worktree.
    fn repo_with_linked_worktree(dir: &Path, keep: &Path) -> (PathBuf, PathBuf) {
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| crate::git(&repo, args).unwrap();
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
            commit.trim(),
        ]);
        std::fs::create_dir_all(wt.join("upstream")).unwrap();
        link_dir(keep, &wt.join("upstream").join("OpenColorIO"));
        assert!(intact(&wt.join("upstream").join("OpenColorIO")));
        (repo, wt)
    }

    #[test]
    fn remove_worktree_keeps_what_a_junction_in_it_points_to() {
        // The case that lost the main checkout's submodule: a registered worktree whose
        // upstream/OpenColorIO links to it, removed as land, gate --staged and clean-scratch
        // remove theirs.
        let dir = Scratch::new("git");
        let keep = precious(dir.path());
        let (repo, wt) = repo_with_linked_worktree(dir.path(), &keep);
        crate::land::remove_worktree(&repo, &wt).unwrap();
        assert!(
            std::fs::symlink_metadata(&wt).is_err(),
            "the worktree is gone"
        );
        assert!(intact(&keep), "what its link pointed to is not");
        let list = crate::git(&repo, &["worktree", "list", "--porcelain"]).unwrap();
        assert_eq!(list.matches("worktree ").count(), 1, "{list}");
    }

    #[test]
    fn a_locked_worktree_is_refused_before_anything_is_touched() {
        let dir = Scratch::new("locked");
        let keep = precious(dir.path());
        let (repo, wt) = repo_with_linked_worktree(dir.path(), &keep);
        let wt_arg = wt.to_str().unwrap();
        crate::git(&repo, &["worktree", "lock", "--reason", "in use", wt_arg]).unwrap();
        let err = crate::land::remove_worktree(&repo, &wt).unwrap_err();
        assert!(err.contains("locked"), "{err}");
        let link = wt.join("upstream").join("OpenColorIO");
        assert!(
            std::fs::symlink_metadata(&link).is_ok_and(|m| is_link(&m)),
            "its link is still there"
        );
        assert!(intact(&keep));
        crate::git(&repo, &["worktree", "unlock", wt_arg]).unwrap();
        crate::land::remove_worktree(&repo, &wt).unwrap();
        assert!(intact(&keep));
    }

    #[test]
    fn a_scratch_directory_that_is_itself_a_link() {
        let dir = Scratch::new("root");
        let keep = precious(dir.path());
        let wt = dir.path().join("wt");
        link_dir(&keep, &wt);
        // A worktree that is a link is refused (remove_worktree unlinks first).
        let err = unlink_all(&wt).unwrap_err();
        assert!(err.contains("is a link"), "{err}");
        assert!(intact(&keep));
        assert!(std::fs::symlink_metadata(&wt).is_ok(), "the link stays too");
        // A directory that is a link: remove_tree removes only the link.
        remove_tree(&wt).unwrap();
        assert!(std::fs::symlink_metadata(&wt).is_err());
        assert!(intact(&keep));
    }

    #[test]
    fn the_scratch_guard_unlinks_before_it_deletes() {
        let outside = Scratch::new("guard-outside");
        let keep = precious(outside.path());
        let path = {
            let dir = Scratch::new("guard");
            link_dir(&keep, &dir.path().join("link"));
            dir.path().to_path_buf()
        };
        assert!(std::fs::symlink_metadata(&path).is_err(), "deleted on drop");
        assert!(intact(&keep));
    }
}
