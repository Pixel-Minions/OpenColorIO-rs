// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Linux wheel's platform tag, worked out from its binaries with auditwheel's rules.
//!
//! maturin audits the extension and picks the lowest `manylinux` tag it fits, but it doesn't look
//! at the tools in `PyOpenColorIO/bin`, which come from another `cargo build`. So this reads all
//! of them with `readelf` (binutils, in the Rocky Linux 9 image), and the wheel's tag must be
//! exactly what they need:
//! - `manylinux_2_N`, where `N` is the newest `GLIBC_2.N` symbol version any of them needs;
//! - only the libraries of [`ALLOWED_LIBRARIES`]. They are in every auditwheel policy, and every
//!   system has them. Anything else would have to be bundled, and nothing is: the wheel has no
//!   C or C++ code (no `build.rs`, no `cc` in `Cargo.lock`), so `libstdc++` is not among them;
//! - the only other symbol versions are libgcc's `GCC_x`, up to [`NEWEST_GCC`]: a bound well
//!   below what a glibc 2.34 system has, since the binaries need `GCC_4.2.0` at most. maturin's
//!   own audit applies auditwheel's exact table to the extension.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::{Command, Stdio};

/// The shared libraries a binary of the wheel may need.
const ALLOWED_LIBRARIES: [&str; 4] = [
    "ld-linux-x86-64.so.2",
    "libc.so.6",
    "libgcc_s.so.1",
    "libm.so.6",
];

/// The newest `GCC_x` symbol version allowed, as numbers.
const NEWEST_GCC: [u32; 3] = [7, 0, 0];

/// What one binary needs from the system.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Needs {
    /// `DT_NEEDED`.
    libraries: BTreeSet<String>,
    /// The symbol versions of `.gnu.version_r`.
    versions: BTreeSet<String>,
}

/// Reads `readelf -W -d -V`.
fn needs_of(readelf: &str) -> Needs {
    let mut needs = Needs::default();
    let mut in_version_needs = false;
    for line in readelf.lines() {
        if let Some((_, rest)) = line.split_once("(NEEDED)")
            && let Some((_, library)) = rest.split_once('[')
        {
            needs
                .libraries
                .insert(library.trim_end_matches(']').to_string());
        }
        if line.starts_with("Version needs section") {
            in_version_needs = true;
        } else if line.starts_with("Version ") {
            in_version_needs = false;
        }
        if in_version_needs
            && let Some((_, rest)) = line.split_once("Name: ")
            && let Some(version) = rest.split_whitespace().next()
        {
            needs.versions.insert(version.to_string());
        }
    }
    needs
}

/// The numbers of `GCC_4.2.0`.
fn numbers(version: &str) -> Option<Vec<u32>> {
    version.split('.').map(|n| n.parse().ok()).collect()
}

/// The platform tag the binaries need, or what stops them from having one.
fn true_platform(binaries: &BTreeMap<String, Needs>) -> Result<String, Vec<String>> {
    let mut problems = Vec::new();
    let mut newest = None;
    for (name, needs) in binaries {
        for library in &needs.libraries {
            if !ALLOWED_LIBRARIES.contains(&library.as_str()) {
                problems.push(format!("{name} needs {library}, which nothing bundles"));
            }
        }
        for version in &needs.versions {
            if let Some(glibc) = version.strip_prefix("GLIBC_") {
                match numbers(glibc).as_deref() {
                    Some([2, minor]) => newest = newest.max(Some(*minor)),
                    Some([2, minor, _]) => newest = newest.max(Some(*minor)),
                    _ => problems.push(format!("{name}: unknown symbol version {version}")),
                }
            } else if let Some(gcc) = version.strip_prefix("GCC_")
                && numbers(gcc).is_some_and(|n| n.as_slice() <= NEWEST_GCC.as_slice())
            {
            } else {
                problems.push(format!("{name}: symbol version {version} is not allowed"));
            }
        }
    }
    match newest {
        Some(minor) if problems.is_empty() => Ok(format!("manylinux_2_{minor}_x86_64")),
        None if problems.is_empty() => Err(vec!["no binary needs any GLIBC_2.N".to_string()]),
        _ => Err(problems),
    }
}

/// The wheel's platform tag, the last part of its file name.
pub(crate) fn platform_of(wheel: &Path) -> Option<&str> {
    wheel
        .file_name()?
        .to_str()?
        .strip_suffix(".whl")?
        .rsplit('-')
        .next()
}

/// That the platform tag of the Linux wheel `wheel` is what its binaries need: extracts it with
/// `python` into `scratch`, which it replaces and removes. A line that says what they need.
pub(crate) fn audit(python: &Path, wheel: &Path, scratch: &Path) -> Result<String, String> {
    let _ = std::fs::remove_dir_all(scratch);
    std::fs::create_dir_all(scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    let out = Command::new(python)
        .args([
            "-I",
            "-c",
            "import sys, zipfile; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])",
        ])
        .arg(wheel)
        .arg(scratch)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", python.display()))?;
    if !out.status.success() {
        return Err(format!(
            "extracting {}: {}",
            wheel.display(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let package = scratch.join("PyOpenColorIO");
    let mut files = Vec::new();
    for dir in [package.clone(), package.join("bin")] {
        for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let path = entry.map_err(|e| e.to_string())?.path();
            let elf = std::fs::read(&path).is_ok_and(|bytes| bytes.starts_with(b"\x7fELF"));
            if elf {
                files.push(path);
            }
        }
    }
    let mut binaries = BTreeMap::new();
    for file in &files {
        let out = Command::new("readelf")
            .args(["-W", "-d", "-V"])
            .arg(file)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("could not run readelf: {e}"))?;
        if !out.status.success() {
            return Err(format!("readelf {}: {}", file.display(), out.status));
        }
        let name = file
            .strip_prefix(scratch)
            .unwrap_or(file)
            .to_string_lossy()
            .into_owned();
        binaries.insert(name, needs_of(&String::from_utf8_lossy(&out.stdout)));
    }
    let _ = std::fs::remove_dir_all(scratch);
    let needed = true_platform(&binaries).map_err(|p| p.join("\n"))?;
    let tagged = platform_of(wheel).unwrap_or_default();
    if tagged != needed {
        return Err(format!(
            "{} is tagged {tagged}, but its binaries need {needed}",
            crate::display_path(wheel)
        ));
    }
    let libraries: BTreeSet<&str> = binaries
        .values()
        .flat_map(|needs| needs.libraries.iter().map(String::as_str))
        .collect();
    Ok(format!(
        "{} binaries need {needed} and only {}",
        binaries.len(),
        libraries.into_iter().collect::<Vec<_>>().join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `readelf -W -d -V`'s form, from a binary of the Rocky Linux 9 build.
    const READELF: &str = "\n\
Dynamic section at offset 0x4dc000 contains 30 entries:\n\
  Tag        Type                         Name/Value\n\
 0x0000000000000001 (NEEDED)             Shared library: [libgcc_s.so.1]\n\
 0x0000000000000001 (NEEDED)             Shared library: [libc.so.6]\n\
 0x000000000000000c (INIT)               0x1000\n\
\n\
Version symbols section '.gnu.version' contains 3 entries:\n\
 Addr: 0x0000000000000d3e  Offset: 0x000d3e  Link: 6 (.dynsym)\n\
  000:   0 (*local*)       2 (GLIBC_2.2.5)   3 (GCC_3.0)\n\
\n\
Version needs section '.gnu.version_r' contains 2 entries:\n\
 Addr: 0x0000000000000dc0  Offset: 0x000dc0  Link: 7 (.dynstr)\n\
  000000: Version: 1  File: libgcc_s.so.1  Cnt: 1\n\
  0x0010:   Name: GCC_4.2.0  Flags: none  Version: 3\n\
  0x0020: Version: 1  File: libc.so.6  Cnt: 2\n\
  0x0030:   Name: GLIBC_2.34  Flags: none  Version: 2\n\
  0x0040:   Name: GLIBC_2.2.5  Flags: none  Version: 4\n";

    fn binaries(needs: &[(&str, Needs)]) -> BTreeMap<String, Needs> {
        needs
            .iter()
            .map(|(name, n)| {
                (
                    name.to_string(),
                    Needs {
                        libraries: n.libraries.clone(),
                        versions: n.versions.clone(),
                    },
                )
            })
            .collect()
    }

    fn needs(libraries: &[&str], versions: &[&str]) -> Needs {
        Needs {
            libraries: libraries.iter().map(|s| s.to_string()).collect(),
            versions: versions.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn readelf_is_read_from_its_needed_lines_and_version_needs_section() {
        assert_eq!(
            needs_of(READELF),
            needs(
                &["libc.so.6", "libgcc_s.so.1"],
                &["GCC_4.2.0", "GLIBC_2.2.5", "GLIBC_2.34"]
            )
        );
    }

    #[test]
    fn the_tag_is_the_newest_glibc_of_any_binary() {
        let got = true_platform(&binaries(&[
            (
                "a.so",
                needs(&["libc.so.6"], &["GLIBC_2.17", "GLIBC_2.3.4"]),
            ),
            ("bin/b", needs(&["libm.so.6"], &["GLIBC_2.34", "GCC_7.0.0"])),
            ("bin/c", needs(&["libc.so.6"], &["GLIBC_2.9"])),
        ]));
        assert_eq!(got, Ok("manylinux_2_34_x86_64".to_string()));
        // A three-part version counts by its minor too.
        let got = true_platform(&binaries(&[("a", needs(&[], &["GLIBC_2.3.4"]))]));
        assert_eq!(got, Ok("manylinux_2_3_x86_64".to_string()));
    }

    #[test]
    fn what_a_tag_cant_promise_is_refused() {
        for (library, version) in [
            ("libstdc++.so.6", "GLIBC_2.34"),
            ("libpython3.13.so.1.0", "GLIBC_2.34"),
            ("libc.so.6", "GLIBCXX_3.4.29"),
            ("libc.so.6", "CXXABI_1.3.13"),
            ("libc.so.6", "GCC_7.1.0"),
            ("libc.so.6", "GLIBC_PRIVATE"),
        ] {
            // With a real GLIBC version too, so that only this entry can be the reason.
            let versions = [version, "GLIBC_2.34"];
            let got = true_platform(&binaries(&[("a", needs(&[library], &versions))]));
            assert!(
                got.as_ref().is_err_and(
                    |p| p.len() == 1 && (p[0].contains(library) || p[0].contains(version))
                ),
                "{library} {version}: {got:?}"
            );
        }
        assert!(true_platform(&BTreeMap::new()).is_err());
        assert!(true_platform(&binaries(&[("a", needs(&["libc.so.6"], &[]))])).is_err());
    }

    #[test]
    fn the_platform_is_the_file_names_last_part() {
        for (name, platform) in [
            (
                "opencolorio_rs-0.0.0-cp313-cp313-manylinux_2_34_x86_64.whl",
                Some("manylinux_2_34_x86_64"),
            ),
            (
                "opencolorio_rs-0.0.0-cp313-cp313-win_amd64.whl",
                Some("win_amd64"),
            ),
            ("not-a-wheel.zip", None),
        ] {
            assert_eq!(platform_of(Path::new(name)), platform);
        }
    }
}
