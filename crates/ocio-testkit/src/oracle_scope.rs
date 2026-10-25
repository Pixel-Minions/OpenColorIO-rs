// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's cache keys, scoped to the sources a command can reach.
//!
//! A cached response is keyed by the request and by every oracle source that can change the
//! command's answer, so that changing one oracle module asks the wheel again only for the
//! commands that can reach it. A stale answer would let a wrong port pass, so whatever can't
//! be proved harmless statically gets the key of the whole oracle ([`Scopes::key`]).
//!
//! `oracle_scope.py`, run in the oracle's Python, reads the package with `ast`, then imports
//! every module of it (a module that fails to import fails every process of the oracle, and
//! the analysis with it). It reports, per top-level module: the modules it reaches
//! statically, the commands it defines, those that run other commands by name, why its code
//! can't be resolved statically ("fallback"), which of those reasons can act when it is
//! imported ("import_fallback"), and why its top level may act on every command's process
//! ("global"); and the syntax-tree pins of the functions the keys rely on. It is an
//! allowlist, and its module docstring lists the rules. From those facts, the key of a request
//! covers:
//!
//! - the command's own module and every module it reaches, transitively;
//! - the shared modules: `__init__.py`, `__main__.py` and `commands.py` (every process runs
//!   them: the package, the framed dispatcher, and the registry with the commands it hosts),
//!   every module whose top level does more than define names (`commands.py` imports every
//!   module of the package in every process), and every module those reach;
//! - for `batch` and `with_files`, which run other commands named in their arguments, the
//!   modules of every command they run, at any depth up to [`MAX_DEPTH`] ([`nested`]);
//! - as the whole oracle's key does: `pyproject.toml`, `uv.lock` and `.python-version`, every
//!   file of the package that isn't a top-level module (data a module could read), upstream's
//!   test data, the allowed variables and this machine (`environment` in `oracle.rs`), and
//!   `oracle_scope.py` itself.
//!
//! The whole oracle's key is the fallback:
//! - for a command the analysis doesn't know (or that two modules define);
//! - for a command whose own module, or a module it reaches, has a fallback reason or an edge
//!   to a module the package doesn't have;
//! - for every command, when a shared module has an import fallback reason (or such an edge);
//! - for a command that runs other commands by name, unless it is `batch` in `commands.py` or
//!   `with_files` in `files_api.py` with the pin [`NESTED`] names (whose arguments [`nested`]
//!   reads), and for a `batch` or `with_files` whose arguments don't name their commands;
//! - for every command, when the analysis fails, reports modules other than the package's,
//!   or reports pins of `commands.py`'s `command` and `_import_package_modules` other than
//!   [`REGISTRY_PIN`] and [`LOADER_PIN`] (the code the analysis's rules were written for).

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

/// The pin (`oracle_scope.py`'s `pin`: the SHA-256 of the function's syntax tree, without its
/// docstring) of `commands.py`'s `command`, which registers every command by its function's
/// name and runs nothing else. Another pin fails the analysis.
pub(crate) const REGISTRY_PIN: (&str, &str) = (
    "commands.py:command",
    "d86f843013fb3dd2a36c9178a9dd870352ff091761a3e9fc73c627e15101596a",
);

/// The pin of `commands.py`'s `_import_package_modules`, the one function the analysis's rules
/// exempt: it imports every module of the package, which the "global" reasons model.
pub(crate) const LOADER_PIN: (&str, &str) = (
    "commands.py:_import_package_modules",
    "f9c28888ffe28999a665abc3782e5245453a8b261d77865428a15309261b84fe",
);

/// The commands [`nested`] follows: each with its module and the pin of the function whose
/// arguments [`nested`] reads. Another pin (`batch` running something more than its
/// `calls`) gives the command the whole oracle's key until [`nested`] is reviewed and the pin
/// updated.
pub(crate) const NESTED: [(&str, &str, &str); 2] = [
    (
        "batch",
        "commands.py",
        "4781aa74d77f910df863b6d5db567028822478f90893897261e33a23dad45c07",
    ),
    (
        "with_files",
        "files_api.py",
        "b3fa2e798f91a600c651a2958ad5713ea0d45e7a9712bf75a3463e7b22457b26",
    ),
];

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
    /// Whether [`nested`] may follow the commands it runs: one of [`NESTED`], with its pin.
    followed: bool,
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

/// What [`fingerprint`] reads: the hash of what the analysis and every key cover besides the
/// modules, the modules with their hashes, and the hash of both (the cached analysis's name).
struct Fingerprint {
    base: u128,
    modules: Vec<(String, u128)>,
    sources: u128,
}

/// The fingerprint of the package in `oracle_dir` and of the analyzer `analyzer`.
fn fingerprint(oracle_dir: &Path, analyzer: &str) -> Result<Fingerprint, String> {
    let package = oracle_dir.join("ocio_oracle");
    let mut files = Vec::new();
    crate::oracle::collect_files(&package, &mut files);
    files.sort();

    let mut modules = Vec::new();
    let mut base = Xxh3::new();
    base.update(b"oracle scope v2");
    hash_named(&mut base, "oracle_scope.py", analyzer.as_bytes());
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
        hash_named(&mut sources, name, &hash.to_le_bytes());
    }
    Ok(Fingerprint {
        base,
        modules,
        sources: sources.digest128(),
    })
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
        Scopes::compute_with(ANALYZER, python, oracle_dir, full, environment, cache_dir)
    }

    /// [`Scopes::compute`], with the analyzer's source `analyzer`.
    pub(crate) fn compute_with(
        analyzer: &str,
        python: &Path,
        oracle_dir: &Path,
        full: u128,
        environment: u128,
        cache_dir: Option<&Path>,
    ) -> Result<Scopes, String> {
        let keyed = |base: u128| {
            let mut h = Xxh3::new();
            h.update(&base.to_le_bytes());
            h.update(&environment.to_le_bytes());
            h.digest128()
        };
        let read = fingerprint(oracle_dir, analyzer)?;
        let cached = cache_dir.map(|dir| dir.join(format!("scope-{:032x}.json", read.sources)));
        // A cached analysis that doesn't parse (a write cut short), or that gives no scopes,
        // is made again.
        if let Some(manifest) = cached
            .as_deref()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            && let Ok(scopes) =
                Scopes::from_manifest(full, keyed(read.base), read.modules.clone(), &manifest)
        {
            return Ok(scopes);
        }
        let bytes = analyze(analyzer, python, oracle_dir)?;
        // The analysis read the files again: they must be the bytes hashed above, or the
        // analysis would be kept under the wrong name.
        if fingerprint(oracle_dir, analyzer)?.sources != read.sources {
            return Err("the oracle's package changed while it was analysed".to_string());
        }
        let manifest: Value =
            serde_json::from_slice(&bytes).map_err(|e| format!("analysis: {e}"))?;
        let scopes = Scopes::from_manifest(full, keyed(read.base), read.modules, &manifest)?;
        if let Some(file) = &cached {
            crate::oracle::write_atomically(file, &bytes);
        }
        Ok(scopes)
    }

    /// The scopes that `manifest`, the analysis of `modules`, gives.
    fn from_manifest(
        full: u128,
        base: u128,
        modules: Vec<(String, u128)>,
        manifest: &Value,
    ) -> Result<Scopes, String> {
        if manifest.get("format") != Some(&Value::from(2)) {
            return Err(format!("analysis format {:?}", manifest.get("format")));
        }
        let reported = manifest
            .get("modules")
            .and_then(Value::as_object)
            .ok_or("analysis without modules")?;
        let pins = manifest
            .get("pins")
            .and_then(Value::as_object)
            .ok_or("analysis without pins")?;
        let pin = |name: &str| pins.get(name).and_then(Value::as_str);
        for (name, expected) in [REGISTRY_PIN, LOADER_PIN] {
            if pin(name) != Some(expected) {
                return Err(format!(
                    "{name} changed (pin {:?}, oracle_scope.rs expects {expected}): review the \
                     analysis's rules in oracle_scope.py, then the pin",
                    pin(name)
                ));
            }
        }
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
        // Whether a module's code, or the code it runs when imported, can't be resolved.
        let mut unresolved = vec![false; n];
        let mut unresolved_at_import = vec![false; n];
        let mut global = vec![false; n];
        let mut defined: HashMap<String, Vec<(usize, bool)>> = HashMap::new();
        for (name, module) in reported {
            let i = index[name.as_str()];
            let mut missing = false;
            for edge in strings(module, "edges")? {
                match index.get(edge.as_str()) {
                    Some(&j) => edges[i].push(j),
                    None => missing = true,
                }
            }
            unresolved[i] = missing || !strings(module, "fallback")?.is_empty();
            unresolved_at_import[i] = missing || !strings(module, "import_fallback")?.is_empty();
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
        // Every process imports the shared modules: what they run then can't be resolved.
        let poisoned = (0..n).any(|i| shared[i] && unresolved_at_import[i]);
        let mut commands = HashMap::new();
        for (command, definitions) in defined {
            let scope = match definitions.as_slice() {
                [(module, dispatcher)] => {
                    let own = closure(&edges, &[*module]);
                    let followed = NESTED.iter().any(|&(name, file, expected)| {
                        name == command
                            && modules[*module].0 == file
                            && pin(&format!("{file}:{name}")) == Some(expected)
                    });
                    Scope {
                        #[cfg(test)]
                        module: Some(*module),
                        fallback: poisoned || (0..n).any(|i| own[i] && unresolved[i]),
                        modules: (0..n).filter(|&i| own[i] || shared[i]).collect(),
                        dispatcher: *dispatcher,
                        followed,
                    }
                }
                // The registry refuses a name defined twice.
                _ => Scope {
                    #[cfg(test)]
                    module: None,
                    modules: Vec::new(),
                    fallback: true,
                    dispatcher: true,
                    followed: false,
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
        h.update(b"oracle scoped key v2");
        h.update(&self.base.to_le_bytes());
        for (i, (name, hash)) in self.modules.iter().enumerate() {
            if reach[i] {
                hash_named(&mut h, name, &hash.to_le_bytes());
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
            Some(_) if !scope.followed => false,
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
        (!scope.fallback && (!scope.dispatcher || scope.followed)).then(|| {
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

/// Runs `analyzer` on the package in `oracle_dir` with `python`, from that directory and
/// isolated as the oracle is, and returns what it prints.
fn analyze(analyzer: &str, python: &Path, oracle_dir: &Path) -> Result<Vec<u8>, String> {
    let package = oracle_dir.join("ocio_oracle");
    let mut command = Command::new(python);
    crate::oracle::isolate(&mut command);
    command
        .current_dir(oracle_dir)
        .args(["-I", "-X", "utf8", "-"])
        .arg(&package)
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
        .write_all(analyzer.as_bytes());
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
