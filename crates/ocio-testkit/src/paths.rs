// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Locations of the workspace, the upstream submodule and the per-platform target directory.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The workspace root (the directory holding the top-level `Cargo.toml`).
pub fn workspace_root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        // This crate lives at <root>/crates/ocio-testkit.
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = manifest.join("..").join("..");
        assert!(
            root.join("rust-toolchain.toml").is_file(),
            "no workspace root at {}",
            root.display()
        );
        root.canonicalize().unwrap_or(root)
    })
}

/// The upstream OpenColorIO checkout (`upstream/OpenColorIO`, pinned to the matched tag).
pub fn upstream_dir() -> PathBuf {
    workspace_root().join("upstream").join("OpenColorIO")
}

/// The committed fixtures directory.
pub fn fixtures_dir() -> PathBuf {
    workspace_root().join("fixtures")
}

/// The oracle's uv project directory.
pub fn oracle_dir() -> PathBuf {
    workspace_root().join("oracle")
}

/// The Cargo target directory of the running test binary.
///
/// Windows and the Rocky Linux 9 container share one checkout, so each platform must use
/// its own target directory; the oracle's virtual environment and cache live inside it.
pub fn target_dir() -> &'static Path {
    static TARGET: OnceLock<PathBuf> = OnceLock::new();
    TARGET.get_or_init(|| {
        if let Some(dir) = std::env::var_os("CARGO_TARGET_DIR") {
            let dir = PathBuf::from(dir);
            return if dir.is_absolute() {
                dir
            } else {
                workspace_root().join(dir)
            };
        }
        // <target>/<profile>/deps/<test binary> or <target>/<profile>/<binary>.
        if let Ok(exe) = std::env::current_exe() {
            let mut dir = exe.parent();
            if dir
                .and_then(Path::file_name)
                .is_some_and(|n| n == "deps" || n == "examples")
            {
                dir = dir.and_then(Path::parent);
            }
            if let Some(target) = dir.and_then(Path::parent) {
                return target.to_path_buf();
            }
        }
        workspace_root().join("target")
    })
}
