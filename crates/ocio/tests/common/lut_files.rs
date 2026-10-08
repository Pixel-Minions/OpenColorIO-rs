// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! LUT files for the readers' oracle tests: each set of files is written once, under
//! `<target>/lut-files/<hash of the set>/`, where the wheel and the port read the same paths
//! (the oracle runs on this machine). The directory's name covers the files' names and bytes,
//! so the oracle's cached replies, which key on the paths, always describe these bytes.

use std::path::PathBuf;

use ocio::{FileTransform, Interpolation, TransformDirection};
use serde_json::{Value, json};

use super::transforms::{Case, direction_name, interpolation_name};

/// One entry of a set of files.
#[derive(Debug, Clone)]
pub(crate) enum Entry {
    /// A file and its bytes.
    File(String, Vec<u8>),
    /// A directory.
    Dir(String),
}

/// A file of `name` holding `bytes`.
pub(crate) fn file(name: &str, bytes: impl Into<Vec<u8>>) -> Entry {
    Entry::File(name.to_owned(), bytes.into())
}

/// The directory of the set `entries`, written if needed, as a path with `/` separators (not
/// a Windows verbatim path).
pub(crate) fn write_files(entries: &[Entry]) -> String {
    let mut key = Vec::new();
    for entry in entries {
        match entry {
            Entry::File(name, bytes) => {
                key.extend_from_slice(b"F");
                key.extend_from_slice(&(name.len() as u64).to_le_bytes());
                key.extend_from_slice(name.as_bytes());
                key.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
                key.extend_from_slice(bytes);
            }
            Entry::Dir(name) => {
                key.extend_from_slice(b"D");
                key.extend_from_slice(&(name.len() as u64).to_le_bytes());
                key.extend_from_slice(name.as_bytes());
            }
        }
    }
    let hash = xxhash_rust::xxh3::xxh3_128(&key);
    let dir: PathBuf = ocio_testkit::paths::target_dir()
        .join("lut-files")
        .join(format!("{hash:032x}"));
    for entry in entries {
        match entry {
            Entry::File(name, bytes) => {
                let path = dir.join(name);
                std::fs::create_dir_all(path.parent().expect("a parent")).unwrap();
                if std::fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
                    // Written aside then renamed, as parallel tests may write the same set.
                    let temp = dir.join(format!(
                        "{name}.{}.{:?}.tmp",
                        std::process::id(),
                        std::thread::current().id()
                    ));
                    std::fs::write(&temp, bytes).unwrap();
                    std::fs::rename(&temp, &path).unwrap();
                }
            }
            Entry::Dir(name) => std::fs::create_dir_all(dir.join(name)).unwrap(),
        }
    }
    let dir = dir.to_str().expect("a UTF-8 path").replace('\\', "/");
    dir.trim_start_matches("//?/").to_owned()
}

/// A case of the file transform of `src`, with the interpolation `interp` and the direction
/// `dir`, as the oracle builds it and as the port does.
pub(crate) fn file_case(
    label: impl Into<String>,
    src: &str,
    interp: Interpolation,
    dir: TransformDirection,
) -> Case {
    let spec: Value = json!({
        "class": "FileTransform",
        "calls": [
            ["setSrc", src],
            ["setInterpolation", {"enum": interpolation_name(interp)}],
            ["setDirection", {"enum": direction_name(dir)}],
        ],
    });
    let mut port = FileTransform::new();
    port.set_src(src);
    port.set_interpolation(interp);
    port.set_direction(dir);
    Case::new(label, spec, port)
}
