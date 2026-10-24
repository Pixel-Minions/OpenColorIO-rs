// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's cache keys, scoped to the sources a command can reach.
//!
//! A cached response is keyed by the request and by every oracle source that can change the
//! command's answer, so that changing one oracle module asks the wheel again only for the
//! commands that can reach it. A stale answer would let a wrong port pass, so whatever can't
//! be resolved statically gets the key of the whole oracle ([`Scopes::key`]).
//!
//! `oracle_scope.py`, run in the oracle's Python, reads the package with `ast` (it imports
//! none of it) and reports, per top-level module: the modules it reaches statically, the
//! commands it defines, those that run other commands by name, why it can't be resolved
//! statically ("fallback"), and why its top level may act on every command's process
//! ("global"). Its module docstring lists the rules. From those facts, the key of a request
//! covers:
//!
//! - the command's own module and every module it reaches, transitively;
//! - the shared modules: `__init__.py`, `__main__.py` and `commands.py` (every process runs
//!   them: the package, the framed dispatcher, and the registry with the commands it hosts),
//!   every module whose top level does more than define names (`commands.py` imports every
//!   module of the package in every process), and every module those reach;
//! - for `batch` and `with_files`, which run other commands named in their arguments, the
//!   modules of every command they run, at any depth ([`nested`]);
//! - as the whole oracle's key does: `pyproject.toml`, `uv.lock` and `.python-version`, every
//!   file of the package that isn't a top-level module (data a module could read), upstream's
//!   test data, the allowed variables and this machine (`environment` in `oracle.rs`), and
//!   `oracle_scope.py` itself.
//!
//! The whole oracle's key is the fallback: for a command the analysis doesn't know (or that two
//! modules define), for a command that can reach a module with a fallback reason, for a command
//! that runs other commands by name but isn't one of [`nested`]'s, for a `batch` or
//! `with_files` whose arguments don't name their commands, and for every command when the
//! analysis fails or reports modules other than the package's.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;
use xxhash_rust::xxh3::{Xxh3, xxh3_128};

/// The analysis, run by the oracle's Python.
pub(crate) const ANALYZER: &str = include_str!("oracle_scope.py");

/// The modules every process of the oracle runs, whatever its command.
pub(crate) const ROOTS: [&str; 3] = ["__init__.py", "__main__.py", "commands.py"];

/// The files of the oracle's directory that its environment is made from.
pub(crate) const ENVIRONMENT_FILES: [&str; 3] = ["pyproject.toml", "uv.lock", ".python-version"];

/// How deep [`nested`] commands are followed before the request gets the whole oracle's key.
const MAX_DEPTH: usize = 4;

/// The commands that run other commands named in their arguments, and those commands with
/// their arguments: `batch`'s `calls[i].cmd` and `.args`, and `with_files`' `command` and
/// `args`. `None` for any other command; `Some(Err(()))` for arguments that don't name their
/// commands.
pub(crate) fn nested<'a>(
    cmd: &str,
    args: &'a Value,
) -> Option<Result<Vec<(&'a str, &'a Value)>, ()>> {
    static NULL: Value = Value::Null;
    match cmd {
        "batch" => Some(
            args.get("calls")
                .and_then(Value::as_array)
                .ok_or(())
                .and_then(|calls| {
                    calls
                        .iter()
                        .map(|call| {
                            let cmd = call.get("cmd").and_then(Value::as_str).ok_or(())?;
                            Ok((cmd, call.get("args").unwrap_or(&NULL)))
                        })
                        .collect()
                }),
        ),
        "with_files" => Some(
            args.get("command")
                .and_then(Value::as_str)
                .map(|cmd| vec![(cmd, args.get("args").unwrap_or(&NULL))])
                .ok_or(()),
        ),
        _ => None,
    }
}

/// What the key of one command covers.
#[derive(Debug)]
struct Scope {
    /// The module that defines the command (`None` when two do), for the tests.
    #[cfg(test)]
    module: Option<usize>,
    /// Indices into [`Scopes::modules`]: the command's module, what it reaches, and the
    /// shared modules.
    modules: Vec<usize>,
    /// Whether the command gets the whole oracle's key.
    fallback: bool,
    /// Whether the analysis found the command may run other commands by name.
    dispatcher: bool,
}

/// The cache keys of the oracle's commands.
#[derive(Debug)]
pub(crate) struct Scopes {
    /// The whole oracle's key, the fallback.
    full: u128,
    /// The hash of what every scoped key covers besides modules.
    base: u128,
    /// Every top-level module of the package, by name, with the hash of its bytes.
    modules: Vec<(String, u128)>,
    commands: HashMap<String, Scope>,
}

impl Scopes {
    /// The scopes of the package in `oracle_dir`, analysed by `python`. `full` is the whole
    /// oracle's key, and `environment` the hash of what isn't in `oracle_dir` (upstream's test
    /// data, the allowed variables, this machine). With `cache_dir`, the analysis is kept in a
    /// file keyed by everything it reads, and read back from it.
    pub(crate) fn compute(
        python: &Path,
        oracle_dir: &Path,
        full: u128,
        environment: u128,
        cache_dir: Option<&Path>,
    ) -> Result<Scopes, String> {
        let package = oracle_dir.join("ocio_oracle");
        let mut files = Vec::new();
        crate::oracle::collect_files(&package, &mut files);
        files.sort();

        // `base`: what the analysis and every scoped key cover besides the modules.
        let mut modules = Vec::new();
        let mut base = Xxh3::new();
        base.update(b"oracle scope v1");
        hash_named(&mut base, "oracle_scope.py", ANALYZER.as_bytes());
        for name in ENVIRONMENT_FILES {
            match std::fs::read(oracle_dir.join(name)) {
                Ok(bytes) => hash_named(&mut base, name, &bytes),
                Err(_) => hash_named(&mut base, name, b"\0missing"),
            }
        }
        base.update(b"package files");
        for file in &files {
            let rel = file
                .strip_prefix(&package)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
            if rel.ends_with(".py") && !rel.contains('/') {
                modules.push((rel, xxh3_128(&bytes)));
            } else {
                hash_named(&mut base, &rel, &bytes);
            }
        }
        let base = base.digest128();

        // The analysis reads the modules, with the environment's Python; it doesn't depend on
        // upstream's test data or the machine, so emulated CPUs share it.
        let mut sources = Xxh3::new();
        sources.update(&base.to_le_bytes());
        for (name, hash) in &modules {
            sources.update(name.as_bytes());
            sources.update(&hash.to_le_bytes());
        }
        let cached =
            cache_dir.map(|dir| dir.join(format!("scope-{:032x}.json", sources.digest128())));
        // A cached analysis that doesn't parse (a write cut short) is made again.
        let read = cached.as_deref().and_then(|f| std::fs::read(f).ok());
        let manifest = match read.and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()) {
            Some(manifest) => manifest,
            None => {
                let bytes = analyze(python, &package)?;
                let manifest: Value =
                    serde_json::from_slice(&bytes).map_err(|e| format!("analysis: {e}"))?;
                if let Some(file) = &cached {
                    crate::oracle::write_atomically(file, &bytes);
                }
                manifest
            }
        };
        let mut keyed = Xxh3::new();
        keyed.update(&base.to_le_bytes());
        keyed.update(&environment.to_le_bytes());
        Scopes::from_manifest(full, keyed.digest128(), modules, &manifest)
    }

    /// The scopes that `manifest`, the analysis of `modules`, gives.
    fn from_manifest(
        full: u128,
        base: u128,
        modules: Vec<(String, u128)>,
        manifest: &Value,
    ) -> Result<Scopes, String> {
        if manifest.get("format") != Some(&Value::from(1)) {
            return Err(format!("analysis format {:?}", manifest.get("format")));
        }
        let reported = manifest
            .get("modules")
            .and_then(Value::as_object)
            .ok_or("analysis without modules")?;
        let index: HashMap<&str, usize> = modules
            .iter()
            .enumerate()
            .map(|(i, (name, _))| (name.as_str(), i))
            .collect();
        let mut names: Vec<&str> = reported.keys().map(String::as_str).collect();
        names.sort_unstable();
        let mut ours: Vec<&str> = index.keys().copied().collect();
        ours.sort_unstable();
        if names != ours {
            return Err(format!(
                "the analysis read {names:?}, the package has {ours:?}"
            ));
        }

        let strings = |module: &Value, key: &str| -> Result<Vec<String>, String> {
            module
                .get(key)
                .and_then(Value::as_array)
                .ok_or(format!("analysis without {key}"))?
                .iter()
                .map(|v| v.as_str().map(str::to_string).ok_or(format!("{key}: {v}")))
                .collect()
        };
        let n = modules.len();
        let mut edges = vec![Vec::new(); n];
        let mut unresolved = vec![false; n];
        let mut global = vec![false; n];
        let mut defined: HashMap<String, Vec<(usize, bool)>> = HashMap::new();
        for (name, module) in reported {
            let i = index[name.as_str()];
            for edge in strings(module, "edges")? {
                match index.get(edge.as_str()) {
                    Some(&j) => edges[i].push(j),
                    None => unresolved[i] = true,
                }
            }
            unresolved[i] |= !strings(module, "fallback")?.is_empty();
            global[i] = !strings(module, "global")?.is_empty();
            let dispatchers = strings(module, "dispatchers")?;
            for command in strings(module, "commands")? {
                let dispatcher = dispatchers.contains(&command);
                defined.entry(command).or_default().push((i, dispatcher));
            }
        }

        let roots: Vec<usize> = ROOTS
            .iter()
            .filter_map(|r| index.get(r).copied())
            .chain((0..n).filter(|&i| global[i]))
            .collect();
        let shared = closure(&edges, &roots);
        let mut commands = HashMap::new();
        for (command, definitions) in defined {
            let scope = match definitions.as_slice() {
                [(module, dispatcher)] => {
                    let mut reach = closure(&edges, &[*module]);
                    for (i, s) in shared.iter().enumerate() {
                        reach[i] |= s;
                    }
                    Scope {
                        #[cfg(test)]
                        module: Some(*module),
                        fallback: (0..n).any(|i| reach[i] && unresolved[i]),
                        modules: (0..n).filter(|&i| reach[i]).collect(),
                        dispatcher: *dispatcher,
                    }
                }
                // The registry refuses a name defined twice.
                _ => Scope {
                    #[cfg(test)]
                    module: None,
                    modules: Vec::new(),
                    fallback: true,
                    dispatcher: true,
                },
            };
            commands.insert(command, scope);
        }
        Ok(Scopes {
            full,
            base,
            modules,
            commands,
        })
    }

    /// The key of a request of `cmd` with `args`.
    pub(crate) fn key(&self, cmd: &str, args: &Value) -> u128 {
        let mut reach = vec![false; self.modules.len()];
        if !self.reach(cmd, args, &mut reach, 0) {
            return self.full;
        }
        let mut h = Xxh3::new();
        h.update(b"oracle scoped key v1");
        h.update(&self.base.to_le_bytes());
        for (i, (name, hash)) in self.modules.iter().enumerate() {
            if reach[i] {
                h.update(&(name.len() as u64).to_le_bytes());
                h.update(name.as_bytes());
                h.update(&hash.to_le_bytes());
            }
        }
        h.digest128()
    }

    /// Marks in `reach` the modules a request of `cmd` with `args` can reach; false when its
    /// scope can't be resolved.
    fn reach(&self, cmd: &str, args: &Value, reach: &mut [bool], depth: usize) -> bool {
        let Some(scope) = self.commands.get(cmd) else {
            return false;
        };
        if scope.fallback || depth > MAX_DEPTH {
            return false;
        }
        for &i in &scope.modules {
            reach[i] = true;
        }
        match nested(cmd, args) {
            None => !scope.dispatcher,
            Some(Err(())) => false,
            Some(Ok(calls)) => calls
                .into_iter()
                .all(|(cmd, args)| self.reach(cmd, args, reach, depth + 1)),
        }
    }

    /// The modules the key of `cmd` covers (without nested commands), or `None` when it gets
    /// the whole oracle's key.
    #[cfg(test)]
    pub(crate) fn modules_of(&self, cmd: &str) -> Option<Vec<&str>> {
        let scope = self.commands.get(cmd)?;
        (!scope.fallback && !scope.dispatcher).then(|| {
            scope
                .modules
                .iter()
                .map(|&i| self.modules[i].0.as_str())
                .collect()
        })
    }

    /// Each command the analysis found, with the module that defines it.
    #[cfg(test)]
    pub(crate) fn command_modules(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .commands
            .iter()
            .map(|(c, s)| {
                let module = s.module.map_or("", |i| self.modules[i].0.as_str());
                (c.clone(), module.to_string())
            })
            .collect();
        out.sort();
        out
    }
}

/// `name`, its length and `bytes`, into `h`.
fn hash_named(h: &mut Xxh3, name: &str, bytes: &[u8]) {
    h.update(&(name.len() as u64).to_le_bytes());
    h.update(name.as_bytes());
    h.update(&(bytes.len() as u64).to_le_bytes());
    h.update(bytes);
}

/// The modules `from` reach through `edges`, `from` included.
fn closure(edges: &[Vec<usize>], from: &[usize]) -> Vec<bool> {
    let mut reach = vec![false; edges.len()];
    let mut stack: Vec<usize> = from.to_vec();
    while let Some(i) = stack.pop() {
        if !std::mem::replace(&mut reach[i], true) {
            stack.extend(&edges[i]);
        }
    }
    reach
}

/// Runs [`ANALYZER`] on `package` with `python`, isolated as the oracle is, and returns what it
/// prints.
fn analyze(python: &Path, package: &Path) -> Result<Vec<u8>, String> {
    let mut command = Command::new(python);
    crate::oracle::isolate(&mut command);
    command
        .args(["-I", "-X", "utf8", "-"])
        .arg(package)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| format!("{}: {e}", python.display()))?;
    let written = child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(ANALYZER.as_bytes());
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() || written.is_err() {
        return Err(format!(
            "the analysis of {} failed ({}): {}",
            package.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

#[cfg(test)]
#[path = "oracle_scope_tests.rs"]
mod tests;
