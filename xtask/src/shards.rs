// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `cargo xtask test-shard <shard>`: one CI job's share of `cargo test --workspace`.
//!
//! Each reference platform and profile runs disjoint groups: `formats`, `heavy`,
//! `grading`, `icc` and `rest`. The latter contains every new or otherwise unlisted target.
//! Windows keeps ICC in its existing formats job. Linux gives the two large ICC
//! input sweeps four parallel partitions, with their original aggregate assertions
//! required by a complete-union job in debug and release. Other ICC cases run once
//! in partition zero. Original input order, RNG draws and case directories remain.
//! The commands retain workspace-wide feature unification and include doctests. Unit tests
//! prove the complete target union exactly once and require the workflow matrices to match.
//!
//! Run 41126692114 (2026-10-30, source be369c75, live oracle, full tier) measured the old
//! Windows release `rest` job at 156.7 minutes, including 141.7 minutes in test binaries.
//! Grading tone, hue curve and primary took 36.7, 31.8 and 20.7 minutes respectively.
//! Move primary to `heavy`, and tone plus hue curve to `grading`: the old rest's measured
//! test time becomes 52.4 minutes. Debug's corresponding time becomes 42.1 minutes.
//! On Rocky the old rest test times fall from 108.6/116.0 to 43.5/45.7 minutes (release/debug).
//! These are replayed component timings, not a promised new job duration. Builds and runner
//! contention remain, and `formats` still has an approximately 81-minute Windows debug job.
//!
//! No individual test, input, tier, assertion, oracle call or comparison changes. Every group
//! uses target filters over `cargo test --workspace --no-fail-fast`, with doctests separate;
//! a request for all groups runs the original complete workspace command directly.

use std::path::Path;
use std::process::Command;

use crate::metadata::{self, Package};

pub(crate) const USAGE: &str = "\
cargo xtask test-shard <shard> [--dry-run]
cargo xtask test-shard --list

Runs one CI test job's share of `cargo test --workspace --no-fail-fast`, in the profile the
shard names (debug or release), on the platform it names (windows or linux: Rocky Linux 9,
through scripts/rocky9.sh). Every cargo run goes on after a failure, and the command fails
at the end if any did. The test tier comes from OCIO_RS_TIER, as for `cargo test`.

  --dry-run   print the cargo commands without running them (on any platform)
  --list      the shards, their groups, and the test targets of the listed groups
";

/// A share of the workspace's tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Group {
    Formats,
    Heavy,
    Grading,
    Icc,
    Rest,
}

impl Group {
    const ALL: [Group; 5] = [
        Group::Formats,
        Group::Heavy,
        Group::Grading,
        Group::Icc,
        Group::Rest,
    ];

    fn name(self) -> &'static str {
        match self {
            Group::Formats => "formats",
            Group::Heavy => "heavy",
            Group::Grading => "grading",
            Group::Icc => "icc",
            Group::Rest => "rest",
        }
    }

    /// The `(package, test target)` pairs of a listed group; `rest` is everything else.
    fn listed(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Group::Formats => FORMATS,
            Group::Heavy => HEAVY,
            Group::Grading => GRADING,
            Group::Icc => ICC,
            Group::Rest => &[],
        }
    }
}

/// The API format sweep. ICC is separate on Linux: the owner measured 170 of
/// 221 debug minutes and 76 of 115 release minutes in the old formats job.
const FORMATS: &[(&str, &str)] = &[("ocio", "api_formats_oracle")];

/// Kept whole on Windows; Linux partitions only the two large original input
/// sweeps. The required native union job preserves their aggregate assertions.
const ICC: &[(&str, &str)] = &[("ocio-tools", "ociobakelut_icc_oracle")];
const ICC_PARTS: u8 = 4;

/// Windows debug minutes (run 39641166482): 15.3, 9.0, 7.9 and 3.8.
const HEAVY: &[(&str, &str)] = &[
    ("ocio-ops", "lut3d_inv_oracle"),
    ("ocio-ops", "lut1d_renderer_oracle"),
    ("ocio", "api_battery_oracle"),
    ("ocio", "api_gpu_oracle"),
    ("ocio-ops", "grading_primary_oracle"),
];

/// Tone and hue curve together took 68.5 minutes on Windows release in run 41126692114.
const GRADING: &[(&str, &str)] = &[
    ("ocio-ops", "grading_tone_oracle"),
    ("ocio-ops", "grading_hue_curve_oracle"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Platform {
    Windows,
    Linux,
}

impl Platform {
    fn name(self) -> &'static str {
        match self {
            Platform::Windows => "windows",
            Platform::Linux => "linux",
        }
    }

    fn this() -> Option<Platform> {
        if cfg!(windows) {
            Some(Platform::Windows)
        } else if cfg!(target_os = "linux") {
            Some(Platform::Linux)
        } else {
            None
        }
    }
}

/// One CI test job.
#[derive(Debug)]
pub(crate) struct Shard {
    pub(crate) id: &'static str,
    platform: Platform,
    release: bool,
    groups: &'static [Group],
    icc_part: Option<u8>,
}

/// Every CI test job. The ids name the platform and the profile; ci.yml's `windows-tests`
/// and `rocky9-tests` matrices list them, in this order.
pub(crate) const SHARDS: &[Shard] = &[
    Shard {
        id: "windows-debug-formats",
        platform: Platform::Windows,
        release: false,
        groups: &[Group::Formats, Group::Icc],
        icc_part: None,
    },
    Shard {
        id: "windows-debug-heavy",
        platform: Platform::Windows,
        release: false,
        groups: &[Group::Heavy],
        icc_part: None,
    },
    Shard {
        id: "windows-debug-grading",
        platform: Platform::Windows,
        release: false,
        groups: &[Group::Grading],
        icc_part: None,
    },
    Shard {
        id: "windows-debug-rest",
        platform: Platform::Windows,
        release: false,
        groups: &[Group::Rest],
        icc_part: None,
    },
    Shard {
        id: "windows-release-formats",
        platform: Platform::Windows,
        release: true,
        groups: &[Group::Formats, Group::Icc],
        icc_part: None,
    },
    Shard {
        id: "windows-release-heavy",
        platform: Platform::Windows,
        release: true,
        groups: &[Group::Heavy],
        icc_part: None,
    },
    Shard {
        id: "windows-release-grading",
        platform: Platform::Windows,
        release: true,
        groups: &[Group::Grading],
        icc_part: None,
    },
    Shard {
        id: "windows-release-rest",
        platform: Platform::Windows,
        release: true,
        groups: &[Group::Rest],
        icc_part: None,
    },
    Shard {
        id: "linux-debug-formats",
        platform: Platform::Linux,
        release: false,
        groups: &[Group::Formats],
        icc_part: None,
    },
    // Debug builds of the heavy and the other targets in one job outgrew the hosted runner's
    // disk (PR #65), as on Windows they run apart.
    Shard {
        id: "linux-debug-heavy",
        platform: Platform::Linux,
        release: false,
        groups: &[Group::Heavy],
        icc_part: None,
    },
    Shard {
        id: "linux-debug-grading",
        platform: Platform::Linux,
        release: false,
        groups: &[Group::Grading],
        icc_part: None,
    },
    Shard {
        id: "linux-debug-rest",
        platform: Platform::Linux,
        release: false,
        groups: &[Group::Rest],
        icc_part: None,
    },
    Shard {
        id: "linux-release-formats",
        platform: Platform::Linux,
        release: true,
        groups: &[Group::Formats],
        icc_part: None,
    },
    // Release builds of the heavy and the other targets in one job outgrew the hosted
    // runner's disk (PR #66, run 39933842156), as on Windows they run apart.
    Shard {
        id: "linux-release-heavy",
        platform: Platform::Linux,
        release: true,
        groups: &[Group::Heavy],
        icc_part: None,
    },
    Shard {
        id: "linux-release-grading",
        platform: Platform::Linux,
        release: true,
        groups: &[Group::Grading],
        icc_part: None,
    },
    Shard {
        id: "linux-release-rest",
        platform: Platform::Linux,
        release: true,
        groups: &[Group::Rest],
        icc_part: None,
    },
    Shard {
        id: "linux-debug-icc-0",
        platform: Platform::Linux,
        release: false,
        groups: &[Group::Icc],
        icc_part: Some(0),
    },
    Shard {
        id: "linux-debug-icc-1",
        platform: Platform::Linux,
        release: false,
        groups: &[Group::Icc],
        icc_part: Some(1),
    },
    Shard {
        id: "linux-debug-icc-2",
        platform: Platform::Linux,
        release: false,
        groups: &[Group::Icc],
        icc_part: Some(2),
    },
    Shard {
        id: "linux-debug-icc-3",
        platform: Platform::Linux,
        release: false,
        groups: &[Group::Icc],
        icc_part: Some(3),
    },
    Shard {
        id: "linux-release-icc-0",
        platform: Platform::Linux,
        release: true,
        groups: &[Group::Icc],
        icc_part: Some(0),
    },
    Shard {
        id: "linux-release-icc-1",
        platform: Platform::Linux,
        release: true,
        groups: &[Group::Icc],
        icc_part: Some(1),
    },
    Shard {
        id: "linux-release-icc-2",
        platform: Platform::Linux,
        release: true,
        groups: &[Group::Icc],
        icc_part: Some(2),
    },
    Shard {
        id: "linux-release-icc-3",
        platform: Platform::Linux,
        release: true,
        groups: &[Group::Icc],
        icc_part: Some(3),
    },
];

/// What `cargo test --workspace` runs, one piece at a time.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Unit {
    /// A target's tests: `cargo test -p <package> <flag> [<name>]`, the flag one of `--lib`,
    /// `--bin` and `--test` (`--lib` takes no name).
    Target {
        package: String,
        flag: &'static str,
        name: String,
    },
    /// A library's documentation tests: `cargo test -p <package> --doc`.
    Doc { package: String },
}

impl Unit {
    fn group(&self) -> Group {
        let Unit::Target {
            package,
            flag: "--test",
            name,
        } = self
        else {
            return Group::Rest;
        };
        [Group::Formats, Group::Heavy, Group::Grading, Group::Icc]
            .into_iter()
            .find(|g| g.listed().contains(&(package.as_str(), name.as_str())))
            .unwrap_or(Group::Rest)
    }
}

/// Library crate types: `cargo test --lib` covers each.
const LIB_KINDS: &[&str] = &["lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"];

/// Every unit of `cargo test --workspace`, after checking what the shards' commands rely on:
///
/// - the shards run `cargo test --workspace` with target filters, which select targets by
///   name in every package (so features unify across the workspace as without filters). A
///   listed test target's name must then be no other package's test target, and every
///   library and test target must be tested (`test = true`): `--lib` and `--test <name>`
///   would also run one that `cargo test` skips;
/// - the listed groups name test targets that exist, once;
/// - there is no example or bench: `cargo test` builds examples and runs some, and no shard
///   places them yet.
fn units(packages: &[Package]) -> Result<Vec<Unit>, String> {
    let mut units = Vec::new();
    for package in packages {
        for target in &package.targets {
            let unit = |flag: &'static str| Unit::Target {
                package: package.name.clone(),
                flag,
                name: target.name.clone(),
            };
            let kinds: Vec<&str> = target.kinds.iter().map(String::as_str).collect();
            let lib = kinds.iter().any(|k| LIB_KINDS.contains(k));
            if (lib || kinds == ["test"]) && !target.test {
                return Err(format!(
                    "{}'s target `{}` sets `test = false`, which the test shards don't \
                     support: `cargo test --workspace --lib` or `--test {}` would run it, and \
                     `cargo test --workspace` doesn't (xtask/src/shards.rs)",
                    package.name, target.name, target.name
                ));
            }
            if lib {
                units.push(unit("--lib"));
                if target.doctest {
                    units.push(Unit::Doc {
                        package: package.name.clone(),
                    });
                }
            } else if kinds == ["bin"] {
                if target.test {
                    units.push(unit("--bin"));
                }
            } else if kinds == ["test"] {
                units.push(unit("--test"));
            } else if kinds != ["custom-build"] {
                return Err(format!(
                    "{} has a target `{}` of kind {kinds:?}, which no test shard places yet: \
                     extend xtask/src/shards.rs",
                    package.name, target.name
                ));
            }
        }
    }
    let mut seen = Vec::new();
    for group in Group::ALL {
        for &(package, name) in group.listed() {
            let owners: Vec<&str> = units
                .iter()
                .filter_map(|u| match u {
                    Unit::Target {
                        package: p,
                        flag: "--test",
                        name: n,
                    } if n == name => Some(p.as_str()),
                    _ => None,
                })
                .collect();
            if !owners.contains(&package) {
                return Err(format!(
                    "test shard group `{}` lists `{package}` test target `{name}`, which the \
                     workspace doesn't have: update xtask/src/shards.rs",
                    group.name()
                ));
            }
            if owners.len() > 1 {
                return Err(format!(
                    "test shard group `{}` lists test target `{name}`, which {} packages \
                     have ({}): `cargo test --workspace --test {name}` runs each",
                    group.name(),
                    owners.len(),
                    owners.join(", ")
                ));
            }
            if seen.contains(&name) {
                return Err(format!("test target `{name}` is in two test shard groups"));
            }
            seen.push(name);
        }
    }
    Ok(units)
}

/// The units of `groups`, in workspace order.
fn selected(units: &[Unit], groups: &[Group]) -> Vec<Unit> {
    units
        .iter()
        .filter(|u| groups.contains(&u.group()))
        .cloned()
        .collect()
}

/// The `cargo` argument lists that run `groups` of the workspace's tests: `cargo test
/// --workspace` itself for all three, else with the groups' targets (`--lib`, `--bin <name>`,
/// `--test <name>`), and the doctests in a run of their own (`--doc` takes no other target).
fn commands(
    packages: &[Package],
    groups: &[Group],
    release: bool,
) -> Result<Vec<Vec<String>>, String> {
    let units = units(packages)?;
    let head = |doc: bool| -> Vec<String> {
        let mut v = vec!["test".to_string(), "--workspace".to_string()];
        if doc {
            v.push("--doc".into());
        }
        if release {
            v.push("--release".into());
        }
        v.push("--no-fail-fast".into());
        v
    };
    if Group::ALL.iter().all(|g| groups.contains(g)) {
        return Ok(vec![head(false)]);
    }
    let chosen = selected(&units, groups);
    let mut filters: Vec<(&str, &str)> = Vec::new();
    for unit in &chosen {
        if let Unit::Target { flag, name, .. } = unit {
            let filter = (*flag, if *flag == "--lib" { "" } else { name.as_str() });
            if !filters.contains(&filter) {
                filters.push(filter);
            }
        }
    }
    let mut out = Vec::new();
    if !filters.is_empty() {
        let mut args = head(false);
        for (flag, name) in filters {
            args.push(flag.into());
            if !name.is_empty() {
                args.push(name.into());
            }
        }
        out.push(args);
    }
    if chosen.iter().any(|u| matches!(u, Unit::Doc { .. })) {
        out.push(head(true));
    }
    if out.is_empty() {
        return Err("this test shard selects no tests".into());
    }
    Ok(out)
}

/// Part zero runs every other ICC test once. The other parts select only the
/// two original large sweeps; each receives a disjoint case-index selection.
fn shard_commands(packages: &[Package], shard: &Shard) -> Result<Vec<Vec<String>>, String> {
    let mut runs = commands(packages, shard.groups, shard.release)?;
    if let Some(part) = shard.icc_part {
        if part >= ICC_PARTS || shard.platform != Platform::Linux || shard.groups != [Group::Icc] {
            return Err("invalid ICC partition shard".into());
        }
        assert_eq!(runs.len(), 1);
        runs[0].push("--".into());
        if part != 0 {
            runs[0].extend(
                [
                    "display_profiles_generated",
                    "corrupted_profiles",
                    "--exact",
                ]
                .map(str::to_string),
            );
        }
        runs[0].push("--nocapture".into());
    }
    Ok(runs)
}

pub(crate) fn run(args: &[&str]) -> Result<(), String> {
    let root = crate::root();
    match args {
        ["--list"] => list(&root),
        [id] | [id, "--dry-run"] | ["--dry-run", id] if !id.starts_with('-') => {
            let dry_run = args.contains(&"--dry-run");
            let shard = SHARDS.iter().find(|s| s.id == *id).ok_or_else(|| {
                format!(
                    "no test shard `{id}`; the shards: {}",
                    SHARDS.iter().map(|s| s.id).collect::<Vec<_>>().join(", ")
                )
            })?;
            if !dry_run && Platform::this() != Some(shard.platform) {
                return Err(format!(
                    "test shard `{id}` runs on {}, not here",
                    shard.platform.name()
                ));
            }
            let packages = metadata::workspace(&root)?;
            let commands = shard_commands(&packages, shard)?;
            println!(
                "test-shard {id}: groups {} ({} cargo run{})",
                group_names(shard.groups),
                commands.len(),
                if commands.len() == 1 { "" } else { "s" }
            );
            for args in &commands {
                println!("    cargo {}", args.join(" "));
            }
            if let Some(part) = shard.icc_part {
                println!(
                    "    ICC original-input partition {part}/{ICC_PARTS}; complete-union audit required"
                );
            }
            if dry_run {
                return Ok(());
            }
            run_all(&root, &commands, shard)
        }
        _ => Err(USAGE.into()),
    }
}

fn group_names(groups: &[Group]) -> String {
    groups
        .iter()
        .map(|g| g.name())
        .collect::<Vec<_>>()
        .join(", ")
}

fn list(root: &Path) -> Result<(), String> {
    let packages = metadata::workspace(root)?;
    let units = units(&packages)?;
    for shard in SHARDS {
        println!("{:<24} {}", shard.id, group_names(shard.groups));
    }
    for group in Group::ALL {
        let chosen = selected(&units, &[group]);
        println!("\ngroup {} ({} units):", group.name(), chosen.len());
        if group.listed().is_empty() {
            println!("    every unit the other groups don't list");
            continue;
        }
        for unit in chosen {
            if let Unit::Target { package, name, .. } = unit {
                println!("    {package} --test {name}");
            }
        }
    }
    Ok(())
}

/// Runs every command, going on after failures; an error naming the failed ones.
fn run_all(root: &Path, commands: &[Vec<String>], shard: &Shard) -> Result<(), String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut failed = Vec::new();
    for (i, args) in commands.iter().enumerate() {
        println!("test-shard: cargo run {} of {}", i + 1, commands.len());
        let mut command = Command::new(&cargo);
        command.args(args).current_dir(root);
        command.env_remove("OCIO_RS_ICC_AUDIT_DIR");
        if let Some(part) = shard.icc_part {
            let head = crate::git(root, &["rev-parse", "--verify", "HEAD"])?;
            command
                .env("OCIO_RS_ICC_PART", format!("{part}/{ICC_PARTS}"))
                .env(
                    "OCIO_RS_ICC_REPORT_DIR",
                    root.join("kept/icc-ci").join(shard.id),
                )
                .env("OCIO_RS_ICC_SOURCE", head.trim());
        } else {
            command
                .env_remove("OCIO_RS_ICC_PART")
                .env_remove("OCIO_RS_ICC_REPORT_DIR");
        }
        let status = command
            .status()
            .map_err(|e| format!("could not run cargo: {e}"))?;
        if !status.success() {
            println!("test-shard: failed ({status}): cargo {}", args.join(" "));
            failed.push(format!("cargo {}", args.join(" ")));
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} of {} cargo runs failed:\n    {}",
            failed.len(),
            commands.len(),
            failed.join("\n    ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::Target;

    fn target(name: &str, kind: &str, doctest: bool) -> Target {
        Target {
            name: name.into(),
            kinds: vec![kind.into()],
            test: true,
            doctest,
        }
    }

    /// A workspace with the listed test targets, and others the listed groups don't name.
    fn sample() -> Vec<Package> {
        vec![
            Package {
                name: "ocio-ops".into(),
                dir: "crates/ocio-ops".into(),
                targets: vec![
                    target("ocio_ops", "lib", true),
                    target("lut3d_inv_oracle", "test", false),
                    target("lut1d_renderer_oracle", "test", false),
                    target("log_oracle", "test", false),
                    target("grading_primary_oracle", "test", false),
                    target("grading_tone_oracle", "test", false),
                    target("grading_hue_curve_oracle", "test", false),
                    target("build-script-build", "custom-build", false),
                ],
            },
            Package {
                name: "ocio".into(),
                dir: "crates/ocio".into(),
                targets: vec![
                    target("ocio", "lib", true),
                    target("api_formats_oracle", "test", false),
                    target("api_battery_oracle", "test", false),
                    target("api_gpu_oracle", "test", false),
                    target("config_version_oracle", "test", false),
                ],
            },
            Package {
                name: "ocio-testkit".into(),
                dir: "crates/ocio-testkit".into(),
                targets: vec![
                    target("ocio_testkit", "lib", true),
                    // The same name as a test of `ocio`: shards select by package.
                    target("config_version_oracle", "test", false),
                ],
            },
            Package {
                name: "xtask".into(),
                dir: "xtask".into(),
                targets: vec![target("xtask", "bin", false)],
            },
            Package {
                name: "ocio-tools".into(),
                dir: "crates/ocio-tools".into(),
                targets: vec![target("ociobakelut_icc_oracle", "test", false)],
            },
        ]
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn each_platform_runs_every_group_once_per_profile() {
        for platform in [Platform::Windows, Platform::Linux] {
            for release in [false, true] {
                let mut groups: Vec<Group> = SHARDS
                    .iter()
                    .filter(|s| {
                        s.platform == platform
                            && s.release == release
                            && s.icc_part.is_none_or(|p| p == 0)
                    })
                    .flat_map(|s| s.groups.iter().copied())
                    .collect();
                groups.sort();
                assert_eq!(groups, Group::ALL, "{platform:?}, release {release}");
            }
        }
    }

    #[test]
    fn shard_ids_name_their_platform_and_profile() {
        for (i, shard) in SHARDS.iter().enumerate() {
            let prefix = format!(
                "{}-{}-",
                shard.platform.name(),
                if shard.release { "release" } else { "debug" }
            );
            assert!(shard.id.starts_with(&prefix), "{}", shard.id);
            assert!(
                SHARDS[..i].iter().all(|s| s.id != shard.id),
                "{} twice",
                shard.id
            );
        }
    }

    #[test]
    fn linux_icc_partitions_keep_original_filters_and_other_cases_once() {
        let packages = sample();
        for release in [false, true] {
            let shards: Vec<_> = SHARDS
                .iter()
                .filter(|s| s.release == release && s.icc_part.is_some())
                .collect();
            assert_eq!(shards.len(), usize::from(ICC_PARTS));
            assert_eq!(
                shards
                    .iter()
                    .map(|s| s.icc_part.unwrap())
                    .collect::<Vec<_>>(),
                (0..ICC_PARTS).collect::<Vec<_>>()
            );
            for shard in shards {
                assert_eq!(shard.platform, Platform::Linux);
                assert_eq!(shard.groups, [Group::Icc]);
                let runs = shard_commands(&packages, shard).unwrap();
                assert_eq!(runs.len(), 1);
                let args = &runs[0];
                assert!(
                    args.windows(2)
                        .any(|v| v == ["--test", "ociobakelut_icc_oracle"])
                );
                assert_eq!(args.iter().any(|s| s == "--release"), release);
                let tail = &args[args.iter().position(|v| v == "--").unwrap() + 1..];
                if shard.icc_part == Some(0) {
                    assert_eq!(tail, strings(&["--nocapture"]));
                } else {
                    assert_eq!(
                        tail,
                        strings(&[
                            "display_profiles_generated",
                            "corrupted_profiles",
                            "--exact",
                            "--nocapture"
                        ])
                    );
                }
            }
        }
        let workflow =
            std::fs::read_to_string(crate::root().join(".github/workflows/ci.yml")).unwrap();
        assert!(workflow.contains("profile: [debug, release]"));
        assert!(workflow.contains("name: Linux ICC exact-once audit"));
        assert!(workflow.contains("icc_partition_audit -- --exact --nocapture"));
        assert!(workflow.contains("OCIO_RS_TIER: full"));
    }

    #[test]
    fn commands_filter_the_workspace_by_target() {
        let packages = sample();
        assert_eq!(
            commands(&packages, &[Group::Formats], false).unwrap(),
            [strings(&[
                "test",
                "--workspace",
                "--no-fail-fast",
                "--test",
                "api_formats_oracle"
            ])]
        );
        assert_eq!(
            commands(&packages, &[Group::Heavy], true).unwrap(),
            [strings(&[
                "test",
                "--workspace",
                "--release",
                "--no-fail-fast",
                "--test",
                "lut3d_inv_oracle",
                "--test",
                "lut1d_renderer_oracle",
                "--test",
                "grading_primary_oracle",
                "--test",
                "api_battery_oracle",
                "--test",
                "api_gpu_oracle"
            ])]
        );
        assert_eq!(
            commands(&packages, &[Group::Grading], false).unwrap(),
            [strings(&[
                "test",
                "--workspace",
                "--no-fail-fast",
                "--test",
                "grading_tone_oracle",
                "--test",
                "grading_hue_curve_oracle"
            ])]
        );
        // `config_version_oracle` is a test of two packages; `--test` names it once.
        assert_eq!(
            commands(&packages, &[Group::Rest], false).unwrap(),
            [
                strings(&[
                    "test",
                    "--workspace",
                    "--no-fail-fast",
                    "--lib",
                    "--test",
                    "log_oracle",
                    "--test",
                    "config_version_oracle",
                    "--bin",
                    "xtask"
                ]),
                strings(&["test", "--workspace", "--doc", "--no-fail-fast"]),
            ]
        );
        // All four groups: the whole workspace, as one cargo run.
        assert_eq!(
            commands(&packages, &Group::ALL, true).unwrap(),
            [strings(&[
                "test",
                "--workspace",
                "--release",
                "--no-fail-fast"
            ])]
        );
    }

    #[test]
    fn new_packages_and_targets_land_in_rest() {
        let mut packages = sample();
        packages[1]
            .targets
            .push(target("new_oracle", "test", false));
        packages.push(Package {
            name: "ocio-new".into(),
            dir: "crates/ocio-new".into(),
            targets: vec![target("ocio_new", "lib", false)],
        });
        let units = units(&packages).unwrap();
        let rest = selected(&units, &[Group::Rest]);
        assert!(rest.contains(&Unit::Target {
            package: "ocio".into(),
            flag: "--test",
            name: "new_oracle".into()
        }));
        assert!(rest.contains(&Unit::Target {
            package: "ocio-new".into(),
            flag: "--lib",
            name: "ocio_new".into()
        }));
        check_cover(&packages);
    }

    #[test]
    fn unplaced_untested_or_stale_targets_are_errors() {
        let mut packages = sample();
        packages[3].targets.push(target("demo", "example", false));
        assert!(units(&packages).unwrap_err().contains("kind [\"example\"]"));
        let mut packages = sample();
        packages[1]
            .targets
            .retain(|t| t.name != "api_formats_oracle");
        assert!(
            units(&packages)
                .unwrap_err()
                .contains("`ocio` test target `api_formats_oracle`")
        );
        // `--test lut3d_inv_oracle` would run a test target that `cargo test` skips.
        let mut packages = sample();
        packages[0].targets[1].test = false;
        assert!(units(&packages).unwrap_err().contains("test = false"));
        // So would `--lib`, for a library.
        let mut packages = sample();
        packages[2].targets[0].test = false;
        assert!(units(&packages).unwrap_err().contains("test = false"));
        // `--test api_formats_oracle` would run both.
        let mut packages = sample();
        packages[2]
            .targets
            .push(target("api_formats_oracle", "test", false));
        assert!(
            units(&packages)
                .unwrap_err()
                .contains("2 packages have (ocio, ocio-testkit)")
        );
        // A bin with `test = false` is no unit; the others select bins by name.
        let mut packages = sample();
        packages[3].targets[0].test = false;
        assert!(
            !units(&packages)
                .unwrap()
                .iter()
                .any(|u| matches!(u, Unit::Target { flag: "--bin", .. }))
        );
    }

    /// The units a `cargo test` run selects, as cargo selects them: no target filter runs
    /// everything, doctests included; `--doc` runs only doctests; filters select targets by
    /// name in every package.
    fn cargo_selects(args: &[String], units: &[Unit]) -> Vec<Unit> {
        assert!(args.iter().any(|a| a == "--workspace"), "{args:?}");
        let doc = args.iter().any(|a| a == "--doc");
        let mut filters = Vec::new();
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--lib" => filters.push(("--lib", None)),
                "--bin" | "--test" => filters.push((arg.as_str(), it.next())),
                "-p" | "--package" | "--exclude" | "--bins" | "--tests" | "--all-targets" => {
                    panic!("unexpected {arg} in {args:?}")
                }
                _ => {}
            }
        }
        units
            .iter()
            .filter(|u| match u {
                Unit::Doc { .. } => doc || filters.is_empty(),
                Unit::Target { flag, name, .. } => {
                    !doc && (filters.is_empty()
                        || filters
                            .iter()
                            .any(|(f, n)| f == flag && n.is_none_or(|n| n == name)))
                }
            })
            .cloned()
            .collect()
    }

    /// Each platform's shards, in each profile, run every unit exactly once.
    fn check_cover(packages: &[Package]) {
        let mut all = units(packages).unwrap();
        all.sort();
        for platform in [Platform::Windows, Platform::Linux] {
            for release in [false, true] {
                let mut run = Vec::new();
                for shard in SHARDS
                    .iter()
                    .filter(|s| s.platform == platform && s.release == release)
                {
                    let commands = shard_commands(packages, shard).unwrap();
                    assert!(!commands.is_empty(), "{}", shard.id);
                    for args in &commands {
                        assert_eq!(args.iter().any(|a| a == "--release"), release, "{args:?}");
                        // This logical target is partitioned at the original case
                        // level, proven separately by the required ICC union audit.
                        let selected = cargo_selects(args, &all);
                        run.extend(selected.into_iter().filter(|unit| {
                            shard.icc_part.is_none_or(|p| p == 0) || !matches!(unit, Unit::Target {name,..} if name == "ociobakelut_icc_oracle")
                        }));
                    }
                }
                run.sort();
                assert_eq!(run, all, "{platform:?}, release {release}");
            }
        }
    }

    #[test]
    fn shards_cover_the_sample_once() {
        check_cover(&sample());
    }

    /// The shards of each platform and profile run every unit of this workspace's
    /// `cargo test --workspace` exactly once.
    #[test]
    fn shards_cover_this_workspace_once() {
        let packages = metadata::workspace(&crate::root()).unwrap();
        let units = units(&packages).unwrap();
        assert!(units.iter().any(|u| matches!(u, Unit::Doc { .. })));
        assert!(units.iter().any(|u| u.group() == Group::Rest));
        check_cover(&packages);
    }

    /// The `shard:` matrix lists in a workflow, per job: `(job id, shards)`.
    fn workflow_shards(text: &str) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        let mut in_jobs = false;
        let mut job = String::new();
        let mut in_list = false;
        for line in text.lines() {
            let line = line.trim_end();
            if line == "jobs:" {
                in_jobs = true;
                continue;
            }
            if in_jobs
                && let Some(name) = line.strip_prefix("  ").and_then(|l| l.strip_suffix(':'))
                && !name.starts_with([' ', '#'])
            {
                job = name.to_string();
                in_list = false;
                continue;
            }
            if line.trim_start() == "shard:" {
                in_list = true;
                out.push((job.clone(), Vec::new()));
                continue;
            }
            if in_list {
                if let Some(item) = line.trim_start().strip_prefix("- ") {
                    if let Some(last) = out.last_mut() {
                        last.1.push(item.trim().to_string());
                    }
                    continue;
                }
                in_list = false;
            }
        }
        out
    }

    #[test]
    fn reads_workflow_matrices() {
        let text = "on: push\njobs:\n  a:\n    strategy:\n      matrix:\n        shard:\n          \
                    - x\n          - y\n    steps:\n      - run: z\n  b:\n    steps: []\n";
        assert_eq!(
            workflow_shards(text),
            [("a".to_string(), vec!["x".to_string(), "y".to_string()])]
        );
    }

    /// ci.yml's test matrices list every shard once, each on its own platform, and the
    /// required checks wait for every job of their platform.
    #[test]
    fn ci_workflow_runs_every_shard_once() {
        let path = crate::root().join(".github/workflows/ci.yml");
        let text = std::fs::read_to_string(&path).unwrap();
        let ids = |platform| {
            SHARDS
                .iter()
                .filter(|s| s.platform == platform)
                .map(|s| s.id.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            workflow_shards(&text),
            [
                ("windows-tests".to_string(), ids(Platform::Windows)),
                ("rocky9-tests".to_string(), ids(Platform::Linux)),
            ]
        );
        assert_eq!(
            text.matches("cargo xtask test-shard ${{ matrix.shard }}")
                .count(),
            2
        );
        assert!(text.contains("needs: [windows-checks, windows-tests]"));
        assert!(text.contains("needs: [rocky9-checks, rocky9-tests, rocky9-icc-audit]"));
    }
}
