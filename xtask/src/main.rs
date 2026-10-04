// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Repository automation (PLAN.md §7, WP 0.2–0.4). Run `cargo xtask help`.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use parity::{Baseline, RatchetMode};

mod fixtures;
mod gate;
mod guards;
mod land;
mod parity;
mod registers;
mod scratch;
mod upstream;

const USAGE: &str = "\
cargo xtask <command>

Checking and landing chunks:
  gate [--staged] [--crates a,b] [--release] [--rocky] [--quick|--full]
                              fmt, clippy, ci and tests, stopping at the first failure;
                              logs in target/gate-logs/ (`cargo xtask gate --help`)
  land <branch> [--no-rocky]  replay <branch> onto phase0 gating every commit (debug, this
                              platform), merge it with --no-ff, regenerate the generated
                              files, gate the result fully (release, Rocky Linux 9) and
                              fast-forward phase0; never pushes
                              (`cargo xtask land --help`)
  clean-scratch [--yes] [--unlabelled]
                              list, and with --yes delete: target/verify* in every
                              checkout (in use or not), worktrees of landed branches, and
                              Rocky build volumes (ocio-rs-target-*) that scripts/rocky9.sh
                              labelled with this repository and whose checkout is gone;
                              --unlabelled: also unlabelled volumes no checkout still uses

Oracle and fixtures (fixtures/ is written only by these commands):
  oracle info                 versions and platform of the pinned oracle wheel
  oracle regen <group>        regenerate fixtures/<group>/ and its manifest entries
  oracle check <group>        regenerate into a temporary directory and compare with the
                              committed fixtures (proves they are platform-independent)
  oracle check-all            `oracle check` for every committed group
  fixtures verify            every fixture matches fixtures/MANIFEST.toml, and vice versa

Guardrails:
  guards                      forbidden patterns, unsafe allowlist, waivers, headers,
                              the registers' structure (docs/improvements.md,
                              docs/deviations.md), ocio::internals and its feature only
                              for crates/ocio/tests
  ratchet [--update] [--base <rev>]
                              ported upstream tests may only increase (--update records
                              the current counts; `xtask land` does it at merge time)
  ci [--main] [--base <rev> | --base-file <path>]
                              guards + fixtures verify + ported tests >= the baseline;
                              --main (main and land commits): docs/ratchet.toml and
                              docs/parity.md are current
  The ratchet's baseline is docs/ratchet.toml at <rev> (the base: a PR's base, or where the
  branch left phase0) or in <path>; without either, this checkout's own, which a branch
  could lower.

Upstream:
  upstream-map update         add new upstream files to upstream-map.toml (keeps edits)
  upstream-status             summary of upstream-map.toml
  upstream-tests              list upstream tests and whether each is ported
  parity [--check]            write docs/parity.md (--check: fail if it is stale); `xtask land`
                              writes it at merge time
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["gate", "--help" | "-h"] => {
            print!("{}", gate::USAGE);
            Ok(())
        }
        ["gate", rest @ ..] => gate::parse(rest).and_then(gate::run),
        ["land", "--help" | "-h"] => {
            print!("{}", land::USAGE);
            Ok(())
        }
        ["land", rest @ ..] => {
            land::parse(rest).and_then(|(branch, rocky)| land::run(&branch, rocky))
        }
        ["clean-scratch", rest @ ..] => scratch::run(rest),
        ["oracle", "info"] => fixtures::oracle_info(),
        ["oracle", "regen", group] => fixtures::regen(group),
        ["oracle", "check", group] => fixtures::check(group),
        ["oracle", "check-all"] => fixtures::check_all(),
        ["fixtures", "verify"] => fixtures::verify(),
        ["guards"] => guards::run(),
        ["ratchet", rest @ ..] => parse_base(rest, &["--update"]).and_then(|(flags, base)| {
            let mode = if flags.contains(&"--update") {
                RatchetMode::Update
            } else {
                RatchetMode::AtLeast
            };
            parity::ratchet(mode, &base)
        }),
        ["upstream-map", "update"] => upstream::update_map(),
        ["upstream-status"] => upstream::status(),
        ["upstream-tests"] => parity::list_tests(),
        ["parity"] => parity::write_dashboard(false),
        ["parity", "--check"] => parity::write_dashboard(true),
        ["ci", rest @ ..] => parse_base(rest, &["--main"])
            .and_then(|(flags, base)| ci(flags.contains(&"--main"), &base)),
        ["help"] | [] => {
            print!("{USAGE}");
            Ok(())
        }
        _ => Err(format!("unknown command `{}`\n\n{USAGE}", args.join(" "))),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Splits `args` into the given `flags` and the ratchet baseline (`--base <rev>` or
/// `--base-file <path>`; neither: this checkout's `docs/ratchet.toml`).
fn parse_base<'a>(args: &[&'a str], flags: &[&str]) -> Result<(Vec<&'a str>, Baseline), String> {
    let mut found = Vec::new();
    let mut base = Baseline::Committed;
    let mut it = args.iter();
    while let Some(&arg) = it.next() {
        match arg {
            "--base" | "--base-file" => {
                let value = it.next().ok_or_else(|| format!("{arg} needs a value"))?;
                if base != Baseline::Committed {
                    return Err("give one of --base and --base-file".into());
                }
                base = if arg == "--base" {
                    Baseline::Rev(value.to_string())
                } else {
                    Baseline::File(PathBuf::from(value))
                };
            }
            _ if flags.contains(&arg) => found.push(arg),
            _ => return Err(format!("unknown option `{arg}`\n\n{USAGE}")),
        }
    }
    Ok((found, base))
}

/// `cargo xtask ci`. Branch mode (the default) checks that the ported-test count is at least
/// the baseline; chunks never edit `docs/ratchet.toml` or `docs/parity.md`. `--main` checks
/// that both generated files are current, as `main` and land commits must be.
fn ci(main: bool, base: &Baseline) -> Result<(), String> {
    if main {
        println!("ci: main mode: docs/ratchet.toml and docs/parity.md must be current");
    } else {
        println!(
            "ci: branch mode: docs/parity.md is not checked (`cargo xtask ci --main` checks it)"
        );
    }
    guards::run()?;
    fixtures::verify()?;
    parity::ratchet(
        if main {
            RatchetMode::Current
        } else {
            RatchetMode::AtLeast
        },
        base,
    )?;
    if main {
        parity::write_dashboard(true)?;
    }
    Ok(())
}

/// Files under `dir` (recursively), as paths relative to `dir` with `/` separators, sorted.
pub(crate) fn walk(dir: &std::path::Path) -> Vec<String> {
    fn inner(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                inner(root, &path, out);
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    inner(dir, dir, &mut out);
    out.sort();
    out
}

/// The workspace root, without the `\\?\` prefix `canonicalize` adds on Windows, which
/// other programs (bash, git, cmd) don't all accept.
pub(crate) fn root() -> PathBuf {
    plain_path(ocio_testkit::paths::workspace_root())
}

/// `path` without a Windows verbatim (`\\?\`) prefix.
pub(crate) fn plain_path(path: &Path) -> PathBuf {
    match path.to_str() {
        Some(s) if s.starts_with(r"\\?\UNC\") => PathBuf::from(format!(r"\\{}", &s[8..])),
        Some(s) if s.starts_with(r"\\?\") => PathBuf::from(&s[4..]),
        _ => path.to_path_buf(),
    }
}

/// `path` for messages: relative to the workspace root when it is inside it, with `/`.
pub(crate) fn display_path(path: &Path) -> String {
    let path = plain_path(path);
    let shown = path.strip_prefix(root()).unwrap_or(&path);
    shown.to_string_lossy().replace('\\', "/")
}

/// `git status` arguments that list every uncommitted change and every untracked file, whatever
/// `status.showUntrackedFiles` says.
pub(crate) const STATUS_ALL: &[&str] = &[
    "status",
    "--porcelain",
    "--untracked-files=all",
    "--ignore-submodules=none",
];

/// Git's repository-local variables (`git rev-parse --local-env-vars`, git 2.53).
/// `git rebase --exec`, which `xtask land` runs the gate under, exports GIT_DIR, which would
/// point every git command at that one repository: xtask's git commands and the steps it runs
/// find each repository, the upstream submodule too, from their directory, as a fresh shell
/// does.
const LOCAL_GIT_ENV: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// Removes git's repository-local variables from `cmd`'s environment.
pub(crate) fn clear_git_env(cmd: &mut Command) -> &mut Command {
    for var in LOCAL_GIT_ENV {
        cmd.env_remove(var);
    }
    cmd
}

/// Runs `git <args>` in `dir` and returns its standard output.
pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = clear_git_env(&mut Command::new("git"))
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
