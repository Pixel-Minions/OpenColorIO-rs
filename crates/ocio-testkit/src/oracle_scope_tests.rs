// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the scoped cache keys, on small packages laid out as the oracle's is, analysed by
//! the oracle's Python; and on the oracle itself, against its registry.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{ANALYZER, LOADER_PIN, NESTED, REGISTRY_PIN, Scopes};
use crate::Oracle;
use crate::paths;

/// The whole oracle's key in these tests.
const FULL: u128 = 0xF011_F011;
/// The environment's hash in these tests.
const ENV: u128 = 0xE0E0;

/// The block of the oracle's module `file` that starts with `start` (its top-level
/// statements are two blank lines apart).
fn oracle_block(file: &str, start: &str) -> String {
    let path = paths::oracle_dir().join("ocio_oracle").join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\r\n", "\n");
    let blocks: Vec<&str> = text
        .split("\n\n\n")
        .filter(|b| b.starts_with(start))
        .collect();
    assert_eq!(blocks.len(), 1, "{}: {start}", path.display());
    blocks[0].trim_end().to_string()
}

/// The test package's `commands.py`: the oracle's `command`, `batch` and
/// `_import_package_modules` (so that their pins are the oracle's), and `info`. Its top level
/// only defines names: it doesn't call `_import_package_modules()`, so that what makes it
/// shared is its being one of the modules every process runs.
fn commands_py() -> String {
    format!(
        "import importlib\nimport os\nimport pkgutil\nimport traceback\n\nfrom . import spec\n\n\
         COMMANDS = {{}}\n\n\n{}\n\n\n@command\ndef info(args, blobs):\n    \
         return spec.version(), []\n\n\n{}\n\n\n{}\n",
        oracle_block("commands.py", "def command(fn):"),
        oracle_block("commands.py", "@command\ndef batch("),
        oracle_block("commands.py", "def _import_package_modules():"),
    )
}

/// The test package's `files_api.py`: the oracle's `with_files`, and stubs of what it uses.
fn files_api_py() -> String {
    format!(
        "import contextlib\n\nfrom .commands import COMMANDS, command\n\nOCIO = None\n\n\n\
         class RequestError(Exception):\n    pass\n\n\ndef check_keys(what, args, allowed):\n    \
         return what\n\n\n@contextlib.contextmanager\ndef directory(contents):\n    \
         yield contents\n\n\ndef _substitute(value, root):\n    return value\n\n\n\
         def _restore(value, roots):\n    return value\n\n\ndef _roots(root):\n    \
         return [root]\n\n\n{}\n",
        oracle_block("files_api.py", "@command\ndef with_files("),
    )
}

/// The modules of the test package besides `commands.py` and `files_api.py`: a dispatcher,
/// a helper the registry imports, a chain `c` -> `b` -> `a`, and an unrelated `d`.
const PACKAGE: &[(&str, &str)] = &[
    ("__init__.py", "\"\"\"The package.\"\"\"\n"),
    (
        "__main__.py",
        "import sys\n\nfrom . import commands, regen\n\n\ndef serve_one():\n    \
         return commands.COMMANDS.get(sys.argv[1])\n\n\ndef main():\n    return serve_one()\n",
    ),
    ("regen.py", "GROUPS = {}\n"),
    ("spec.py", "def version():\n    return 1\n"),
    (
        "a.py",
        "from .commands import command\n\n\n@command\ndef a_cmd(args, blobs):\n    \
         return 1, []\n\n\ndef helper():\n    return 2\n",
    ),
    (
        "b.py",
        "from . import a\nfrom .commands import command\n\n\n@command\n\
         def b_cmd(args, blobs):\n    return a.helper(), []\n",
    ),
    (
        "c.py",
        "from .b import b_cmd\nfrom .commands import command\n\n\n@command\n\
         def c_cmd(args, blobs):\n    return b_cmd(args, blobs)\n",
    ),
    (
        "d.py",
        "from .commands import command\n\n\n@command\ndef d_cmd(args, blobs):\n    \
         return 4, []\n",
    ),
];

/// Files a case writes besides the module it is about: (name, text).
type Files = &'static [(&'static str, &'static str)];

/// The test package's commands that run no other command.
const COMMANDS: [&str; 5] = ["a_cmd", "b_cmd", "c_cmd", "d_cmd", "info"];

/// A test's oracle directory, deleted when the test ends, failed or not.
struct Package(PathBuf);

impl Drop for Package {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl std::ops::Deref for Package {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

/// A new oracle directory for `test`, holding the test package and the environment's files.
fn package(test: &str) -> Package {
    let dir = paths::target_dir().join(format!("testkit-scope-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("ocio_oracle")).unwrap();
    for (name, text) in [
        ("pyproject.toml", "p"),
        ("uv.lock", "l"),
        (".python-version", "3.13"),
    ] {
        std::fs::write(dir.join(name), text).unwrap();
    }
    for (name, text) in PACKAGE {
        write(&dir, name, text);
    }
    write(&dir, "commands.py", &commands_py());
    write(&dir, "files_api.py", &files_api_py());
    Package(dir)
}

/// Writes the package's file `name`.
fn write(dir: &Path, name: &str, text: &str) {
    let file = dir.join("ocio_oracle").join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

/// Removes the package's file `name`.
fn remove(dir: &Path, name: &str) {
    std::fs::remove_file(dir.join("ocio_oracle").join(name)).unwrap();
}

/// Adds a comment to the package's file `name`, which changes its bytes and nothing else.
fn touch(dir: &Path, name: &str) {
    let file = dir.join("ocio_oracle").join(name);
    let mut text = std::fs::read_to_string(&file).unwrap();
    text.push_str("# changed\n");
    std::fs::write(file, text).unwrap();
}

fn try_scopes(dir: &Path) -> Result<Scopes, String> {
    Scopes::compute(Oracle::get().python(), dir, FULL, ENV, None)
}

fn scopes_with(dir: &Path, environment: u128) -> Scopes {
    Scopes::compute(Oracle::get().python(), dir, FULL, environment, None)
        .unwrap_or_else(|e| panic!("{e}"))
}

fn scopes(dir: &Path) -> Scopes {
    scopes_with(dir, ENV)
}

/// The keys of `commands`, each requested without arguments.
fn keys(scopes: &Scopes, commands: &[&str]) -> Vec<u128> {
    commands.iter().map(|c| scopes.key(c, &json!({}))).collect()
}

/// The commands of `commands` whose keys differ between `before` and `after`.
fn changed<'a>(commands: &[&'a str], before: &[u128], after: &[u128]) -> Vec<&'a str> {
    (0..commands.len())
        .filter(|&i| before[i] != after[i])
        .map(|i| commands[i])
        .collect()
}

/// Changing a module changes the key of its own commands and of every command whose module
/// imports it, directly or through others, and of no other command.
#[test]
fn a_module_changes_the_keys_of_the_commands_that_reach_it() {
    let dir = package("reach");
    let before = keys(&scopes(&dir), &COMMANDS);
    assert!(before.iter().all(|&k| k != FULL), "{before:x?}");
    assert_eq!(
        scopes(&dir).modules_of("c_cmd").unwrap(),
        [
            "__init__.py",
            "__main__.py",
            "a.py",
            "b.py",
            "c.py",
            "commands.py",
            "regen.py",
            "spec.py"
        ]
    );

    touch(&dir, "a.py");
    let after_a = keys(&scopes(&dir), &COMMANDS);
    assert_eq!(
        changed(&COMMANDS, &before, &after_a),
        ["a_cmd", "b_cmd", "c_cmd"]
    );

    touch(&dir, "b.py");
    let after_b = keys(&scopes(&dir), &COMMANDS);
    assert_eq!(changed(&COMMANDS, &after_a, &after_b), ["b_cmd", "c_cmd"]);

    touch(&dir, "d.py");
    let after_d = keys(&scopes(&dir), &COMMANDS);
    assert_eq!(changed(&COMMANDS, &after_b, &after_d), ["d_cmd"]);

    // The same sources give the same keys.
    assert_eq!(keys(&scopes(&dir), &COMMANDS), after_d);
}

/// An analysis of the test package's modules, for [`Scopes::from_manifest`].
fn manifest(modules: Value) -> Value {
    let pins: serde_json::Map<String, Value> = [REGISTRY_PIN, LOADER_PIN]
        .into_iter()
        .map(|(name, pin)| (name.to_string(), json!(pin)))
        .chain(
            NESTED
                .iter()
                .map(|(name, file, pin)| (format!("{file}:{name}"), json!(pin))),
        )
        .collect();
    json!({"format": 2, "modules": modules, "pins": pins})
}

fn module(edges: &[&str], commands: &[&str]) -> Value {
    json!({"edges": edges, "commands": commands, "dispatchers": [], "fallback": [],
           "import_fallback": [], "global": []})
}

/// `names`, each with the hash `hashes` gives it.
fn hashed(names: &[&str], hashes: impl Fn(usize) -> u128) -> Vec<(String, u128)> {
    names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.to_string(), hashes(i)))
        .collect()
}

/// From an analysis alone: each of the modules every process runs is in every key, even with
/// no edge to it; an edge to a module the package doesn't have, or an analysis of other
/// modules than the package's, can't be resolved; the pins of the registry and the loader must
/// be the ones the rules were written for.
#[test]
fn the_roots_are_shared_and_unknown_modules_unresolved() {
    let names = ["__init__.py", "__main__.py", "commands.py", "x.py", "y.py"];
    let modules = |names: &[&str]| hashed(names, |i| i as u128);
    let analysis = manifest(json!({
        "__init__.py": module(&[], &[]),
        "__main__.py": module(&[], &[]),
        "commands.py": module(&[], &["info"]),
        "x.py": module(&[], &["x_cmd"]),
        "y.py": module(&["missing.py"], &["y_cmd"]),
    }));
    let scopes = Scopes::from_manifest(FULL, ENV, modules(&names), &analysis).unwrap();
    assert_eq!(
        scopes.modules_of("x_cmd").unwrap(),
        ["__init__.py", "__main__.py", "commands.py", "x.py"]
    );
    assert_ne!(scopes.key("x_cmd", &json!({})), FULL);
    assert_eq!(scopes.key("y_cmd", &json!({})), FULL);
    assert!(Scopes::from_manifest(FULL, ENV, modules(&names[..4]), &analysis).is_err());
    let mut format = analysis.clone();
    format["format"] = json!(1);
    assert!(Scopes::from_manifest(FULL, ENV, modules(&names), &format).is_err());
    for (pin, _) in [REGISTRY_PIN, LOADER_PIN] {
        let mut other = analysis.clone();
        other["pins"][pin] = json!("0");
        assert!(
            Scopes::from_manifest(FULL, ENV, modules(&names), &other).is_err(),
            "{pin}"
        );
    }
}

/// A key names the modules it covers: two modules with the same bytes aren't the same module.
#[test]
fn the_key_names_the_modules_it_covers() {
    let names = [
        "__init__.py",
        "__main__.py",
        "commands.py",
        "p.py",
        "q.py",
        "x.py",
    ];
    // `p.py` and `q.py` have the same hash.
    let modules = hashed(&names, |i| if i == 4 { 3 } else { i as u128 });
    let key = |edge: &str| {
        let analysis = manifest(json!({
            "__init__.py": module(&[], &[]),
            "__main__.py": module(&[], &[]),
            "commands.py": module(&[], &[]),
            "p.py": module(&[], &[]),
            "q.py": module(&[], &[]),
            "x.py": module(&[edge], &["x_cmd"]),
        }));
        Scopes::from_manifest(FULL, ENV, modules.clone(), &analysis)
            .unwrap()
            .key("x_cmd", &json!({}))
    };
    assert_ne!(key("p.py"), FULL);
    assert_ne!(key("p.py"), key("q.py"));
}

/// A shared module whose code may act when it is imported, in a way the analysis can't
/// resolve, gives every command the whole oracle's key; a fallback reason elsewhere in it
/// (in a function nothing runs at import) only gives that to the commands that reach it.
#[test]
fn a_shared_module_unresolved_at_import_gives_every_key_the_full_one() {
    let names = ["__init__.py", "__main__.py", "commands.py", "g.py", "x.py"];
    let modules = hashed(&names, |i| i as u128);
    let scopes = |g: Value| {
        let analysis = manifest(json!({
            "__init__.py": module(&[], &[]),
            "__main__.py": module(&[], &[]),
            "commands.py": module(&[], &[]),
            "g.py": g,
            "x.py": module(&[], &["x_cmd"]),
        }));
        Scopes::from_manifest(FULL, ENV, modules.clone(), &analysis).unwrap()
    };
    let g = |fallback: &[&str], at_import: &[&str], global: &[&str]| {
        json!({"edges": [], "commands": ["g_cmd"], "dispatchers": [], "fallback": fallback,
               "import_fallback": at_import, "global": global})
    };
    // Global, unresolved only in a function.
    let quiet = scopes(g(&["f"], &[], &["top"]));
    assert_ne!(quiet.key("x_cmd", &json!({})), FULL);
    assert!(quiet.modules_of("x_cmd").unwrap().contains(&"g.py"));
    assert_eq!(quiet.key("g_cmd", &json!({})), FULL);
    // Global, unresolved at import.
    let loud = scopes(g(&["f"], &["f"], &["top"]));
    assert_eq!(loud.key("x_cmd", &json!({})), FULL);
    // Not shared: what it does at import is no one's but its importers'.
    let private = scopes(g(&["f"], &["f"], &[]));
    assert_ne!(private.key("x_cmd", &json!({})), FULL);
    assert!(!private.modules_of("x_cmd").unwrap().contains(&"g.py"));
}

/// Changing a shared source changes every key: the modules every process runs and what they
/// import, a module whose top level runs code, a file of the package that isn't a top-level
/// module, the environment's files, and the environment's hash.
#[test]
fn a_shared_source_changes_every_key() {
    let dir = package("shared");
    // `g` sets a variable when it is imported, which every process does; `h` only defines.
    write(&dir, "g.py", "import os\n\nos.environ[\"G\"] = \"1\"\n");
    write(
        &dir,
        "h.py",
        "import re\n\nPATTERN = re.compile(\"x\")\n\n\ndef helper():\n    return PATTERN\n",
    );
    write(&dir, "table.json", "[1]");
    write(&dir, "sub/inner.py", "X = 1\n");

    for file in [
        "__init__.py",
        "__main__.py",
        "commands.py",
        "spec.py",
        "regen.py",
        "g.py",
        "table.json",
        "sub/inner.py",
    ] {
        let before = keys(&scopes(&dir), &COMMANDS);
        touch(&dir, file);
        let after = keys(&scopes(&dir), &COMMANDS);
        assert_eq!(changed(&COMMANDS, &before, &after), COMMANDS, "{file}");
    }
    for file in ["pyproject.toml", "uv.lock", ".python-version"] {
        let before = keys(&scopes(&dir), &COMMANDS);
        std::fs::write(dir.join(file), "changed").unwrap();
        let after = keys(&scopes(&dir), &COMMANDS);
        assert_eq!(changed(&COMMANDS, &before, &after), COMMANDS, "{file}");
    }
    let before = keys(&scopes(&dir), &COMMANDS);
    let after = keys(&scopes_with(&dir, ENV + 1), &COMMANDS);
    assert_eq!(changed(&COMMANDS, &before, &after), COMMANDS, "environment");

    // A module that only defines names is no one's but its importers'.
    let before = keys(&scopes(&dir), &COMMANDS);
    touch(&dir, "h.py");
    assert_eq!(keys(&scopes(&dir), &COMMANDS), before, "h.py");
}

/// The top-level statements that make a module shared (each, with the files it needs), and
/// some that don't: a module that nothing imports changes every key, or none.
#[test]
fn a_module_whose_top_level_runs_code_is_shared() {
    // A base class whose hook runs when another module subclasses it.
    const REGISTRY: (&str, &str) = (
        "reg.py",
        "class Base:\n    def __init_subclass__(cls, **kw):\n        PLUGINS.append(cls)\n\n\n\
         PLUGINS = []\n",
    );
    let shared: &[(&str, Files)] = &[
        ("import os\n\nos.environ[\"G\"] = \"1\"\n", &[]),
        ("from . import spec\n\nspec.version = lambda: 2\n", &[]),
        ("for i in range(3):\n    pass\n", &[]),
        ("def f(x=print(\"x\")):\n    return x\n", &[]),
        ("def f(x: print(\"x\")):\n    return x\n", &[]),
        ("def f() -> print(\"x\"):\n    return 1\n", &[]),
        ("X: print(\"x\") = 1\n", &[]),
        ("F = lambda x=print(\"x\"): x\n", &[]),
        (
            "import functools\n\n\n@functools.singledispatch\ndef f(x):\n    return x\n",
            &[],
        ),
        (
            "import functools\n\n\n@functools.total_ordering\nclass C:\n    \
             def __lt__(self, other):\n        return True\n",
            &[],
        ),
        ("class C:\n    print(\"x\")\n", &[]),
        ("class C:\n    X = print(\"x\")\n", &[]),
        ("class C(metaclass=type):\n    pass\n", &[]),
        (
            "from . import reg\n\n\nclass P(reg.Base):\n    pass\n",
            &[REGISTRY],
        ),
        (
            "class B:\n    def __init_subclass__(cls):\n        pass\n\n\nclass P(B):\n    pass\n",
            &[],
        ),
        (
            "class Hook:\n    pass\n\n\nHOOK = Hook()\n\n\nclass C:\n    hook = HOOK\n",
            &[],
        ),
        (
            "from . import regen\n\nX = [0 for regen.GROUPS[\"x\"] in [1]]\n",
            &[],
        ),
        ("X = (Y := 1)\n", &[]),
        ("X = [1]\nX += [2]\n", &[]),
        ("from .regen import GROUPS\n\nGROUPS.update({1: 2})\n", &[]),
        (
            "from .regen import GROUPS\n\nX = GROUPS.setdefault(1, 2)\n",
            &[],
        ),
        ("import os\n\nX = os.environ.copy()\n", &[]),
        ("import os\n\nX = os.getcwd()\n", &[]),
        (
            "def main():\n    pass\n\n\nif __name__ == \"__main__\":\n    main()\nelse:\n    \
             main()\n",
            &[],
        ),
        (
            "try:\n    import json\nexcept ImportError:\n    pass\n",
            &[],
        ),
        ("X = type(\"X\", (), {})\n", &[]),
        ("def len(x):\n    return print(x)\n\n\nX = len(1)\n", &[]),
        // A sibling's class: its metaclass could be the package's.
        (
            "from . import reg\n\n\nclass P:\n    base = reg.Base\n",
            &[REGISTRY],
        ),
    ];
    let private: &[(&str, Files)] = &[
        ("import re\n\nPATTERN = re.compile(\"x\")\n", &[]),
        (
            "KEYS = {\"a\": 1, \"b\": (2, 3)}\nNAMES = frozenset(KEYS)\n",
            &[],
        ),
        (
            "import contextlib\n\n\n@contextlib.contextmanager\ndef f():\n    yield\n",
            &[],
        ),
        (
            "import functools\n\n\n@functools.lru_cache(maxsize=None)\ndef f():\n    \
             return 1\n",
            &[],
        ),
        (
            "class C(Exception):\n    \"\"\"Doc.\"\"\"\n\n    def m(self, x=1):\n        \
             return print(x)\n",
            &[],
        ),
        (
            "class B:\n    pass\n\n\nclass C(B):\n    X = 1\n\n    @property\n    \
             def p(self):\n        return 1\n\n    @p.setter\n    def p(self, v):\n        \
             pass\n\n    @staticmethod\n    def s():\n        return print(1)\n",
            &[],
        ),
        ("TABLE = {\"f\": lambda x: print(x)}\n", &[]),
        (
            "def main():\n    print(\"x\")\n\n\nif __name__ == \"__main__\":\n    main()\n",
            &[],
        ),
        (
            "from .regen import GROUPS\n\nSIZES = {k: len(v) for k, v in GROUPS.items()}\n",
            &[],
        ),
        (
            "from . import regen\n\n\nclass C:\n    groups = regen.GROUPS\n",
            &[],
        ),
        (
            "import unittest\n\n\nclass R(unittest.TestResult):\n    pass\n",
            &[],
        ),
        ("X = 2 ** 32 - 1\nY = (type(None), int)\n", &[]),
        (
            "import builtins\n\nNAMES = [n for n in vars(builtins) if n.startswith(\"A\")]\n",
            &[],
        ),
        ("def f(x: int = 1) -> str:\n    return str(x)\n", &[]),
    ];
    let dir = package("top-level");
    for (cases, is_shared) in [(shared, true), (private, false)] {
        for (source, files) in cases {
            for (name, text) in *files {
                write(&dir, name, text);
            }
            write(&dir, "g.py", source);
            let before = keys(&scopes(&dir), &COMMANDS);
            assert!(before.iter().all(|&k| k != FULL), "{source}");
            touch(&dir, "g.py");
            let after = keys(&scopes(&dir), &COMMANDS);
            let expected: &[&str] = if is_shared { &COMMANDS } else { &[] };
            assert_eq!(changed(&COMMANDS, &before, &after), expected, "{source}");
            for (name, _) in *files {
                remove(&dir, name);
            }
        }
    }
}

/// A shared module's fallback reasons give every command the whole oracle's key when they
/// can act at import: in a top-level statement that runs code, or in what such a statement
/// uses, here or in another module; not in a function nothing runs at import, nor in an
/// inert statement.
#[test]
fn what_runs_at_import_must_be_resolved() {
    let cases: &[(&str, Files, bool)] = &[
        (
            "import os\n\nos.environ[\"G\"] = \"1\"\n\n\ndef f(n):\n    return __import__(n)\n",
            &[],
            false,
        ),
        (
            "def f():\n    return __import__(\"json\")\n\n\nf()\n",
            &[],
            true,
        ),
        (
            "import builtins\n\nIMP = vars(builtins)[\"__im\" + \"port__\"]\nIMP(\"json\")\n",
            &[],
            true,
        ),
        (
            "from . import h\n\nh.f()\n",
            &[("h.py", "def f():\n    return __import__(\"json\")\n")],
            true,
        ),
        (
            "from . import h\n\nh.g()\n",
            &[(
                "h.py",
                "def f():\n    return __import__(\"json\")\n\n\ndef g():\n    return 1\n",
            )],
            false,
        ),
        ("class C:\n    X = __import__(\"json\")\n", &[], true),
        (
            "class C:\n    X = print(1)\n\n    def f(self):\n        return __import__(\"json\")\n",
            &[],
            false,
        ),
        (
            "import builtins\n\nNAMES = [n for n in vars(builtins) if n.startswith(\"A\")]\n",
            &[],
            false,
        ),
        ("import xml.dom\n", &[], true),
        ("from xml import dom\n", &[], true),
    ];
    let dir = package("import-time");
    for (source, files, full) in cases {
        for (name, text) in *files {
            write(&dir, name, text);
        }
        write(&dir, "g.py", source);
        let scopes = scopes(&dir);
        for command in COMMANDS {
            assert_eq!(
                scopes.key(command, &json!({})) == FULL,
                *full,
                "{source}: {command}"
            );
        }
        for (name, _) in *files {
            remove(&dir, name);
        }
    }
}

/// Code the analysis can't resolve: the command whose module holds it, or reaches it, gets
/// the whole oracle's key; other commands keep theirs.
const UNRESOLVED: &[&str] = &[
    // The import machinery, and modules that reach modules by name.
    "def load(name):\n    import importlib\n    return importlib.import_module(name)\n",
    "from importlib import import_module\n\n\ndef load(name):\n    return import_module(name)\n",
    "import inspect\n\n\ndef f():\n    return inspect.stack()\n",
    "import gc\n\n\ndef f():\n    return gc.get_objects()\n",
    "import runpy\n\n\ndef f(n):\n    return runpy.run_module(n)\n",
    "import pkgutil\n\n\ndef f():\n    return list(pkgutil.iter_modules())\n",
    "import builtins\n\n\ndef load(name):\n    return getattr(builtins, name)\n",
    "def f():\n    import xml.dom\n    return xml.dom\n",
    "def load(name):\n    return __import__(name)\n",
    "def run(code):\n    exec(code)\n",
    "def run(code):\n    return eval(code)\n",
    "def run(code):\n    return compile(code, \"x\", \"exec\")\n",
    "def f():\n    return globals()\n",
    "def f():\n    return vars()\n",
    "def load(name):\n    return __builtins__[name]\n",
    "import sys\n\n\ndef load(name):\n    return sys.modules[name]\n",
    "import sys\n\n\ndef add(path):\n    sys.path.insert(0, path)\n",
    "import sys\n\n\ndef f():\n    return sys.settrace\n",
    "import sys\n\n\ndef f():\n    return sys._getframe(1)\n",
    "def f(frame):\n    return frame.f_globals\n",
    "def f():\n    try:\n        raise ValueError\n    except ValueError as e:\n        \
     return e.__traceback__\n",
    "def f():\n    return object.__subclasses__()\n",
    "from . import spec\n\n\ndef f():\n    return spec.version.__globals__\n",
    "import json\n\n\ndef f():\n    return json.sys\n",
    "import os\n\n\ndef f():\n    return os.system(\"x\")\n",
    "def f(o):\n    return o.system(\"x\")\n",
    "def f(o):\n    return o.execv(\"x\", [])\n",
    "import os\n\n\ndef f():\n    return os.__dict__[\"x\"]\n",
    "def f(d):\n    return d[\"os\"]\n",
    "import json\n\n\ndef f():\n    return vars(json)[\"system\"]\n",
    "from os import system\n\n\ndef f():\n    return system(\"x\")\n",
    "import sys\n\n\ndef f(x):\n    return x(sys)\n",
    // Strings naming the import machinery, as an argument, an index or a keyword argument.
    "def load(d):\n    return d[\"__import__\"]\n",
    "def f(g):\n    return g(name=\"__globals__\")\n",
    "def f(g):\n    return getattr(g, \"__globals__\")\n",
    "def f():\n    import operator\n    return operator.attrgetter(\"a.__globals__\")\n",
    // Imports that can't be resolved, and two imports of one name.
    "from .a import *\n",
    "import json as m\nimport os as m\n",
    // The package, and a sibling module other than as `<module>.<name>` of a definition.
    "import ocio_oracle\n\n\ndef reach():\n    return ocio_oracle.d\n",
    "from . import a\n\n\ndef reach(name):\n    return getattr(a, name)\n",
    "from . import a\n\n\ndef put(name):\n    setattr(a, name, 1)\n",
    "from . import a\n\n\ndef take():\n    delattr(a, \"helper\")\n",
    "from . import commands\n\n\ndef f():\n    return getattr(commands, \"COMMANDS\")\n",
    "from . import commands\n\n\ndef f():\n    return vars(commands)\n",
    "from . import a\n\n\ndef f():\n    m = a\n    return m\n",
    "from . import commands\n\n\ndef f(n):\n    return commands.importlib.import_module(n)\n",
    "from .commands import importlib\n\n\ndef f(n):\n    return importlib.import_module(n)\n",
    "import os\n\nfrom . import spec\n\n\ndef f():\n    return os.path.dirname(spec.__file__)\n",
    // `__file__` other than as the package's directory joined with a sibling's file, or the
    // oracle's directory passed on.
    "import os\n\nHERE = os.path.dirname(__file__)\n",
    "import os\n\nDATA = os.path.join(os.path.dirname(os.path.abspath(__file__)), \
     \"table.json\")\n",
    "import os\n\n\ndef f(d):\n    return os.path.join(d, os.path.dirname(os.path.abspath(\
     __file__)))\n",
    "import os\n\nORACLE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))\n\n\n\
     def f(m):\n    return os.path.join(ORACLE, m)\n",
    "import os\n\n\ndef f(m):\n    oracle = os.path.dirname(os.path.dirname(os.path.abspath(\
     __file__)))\n    return g(oracle, m)\n\n\ndef g(d, m):\n    return d + m\n",
    // Running a module by name, and strings that name a module of the package unresolved.
    "ARGV = [\"-m\", \"ocio_oracle.nowhere\"]\n",
    "ARGV = [\"-m\", \"ocio_oracle.d\"]\n",
    "ARGV = [\"-m\", \"ocio_oracle\"]\n",
    "COMMAND = \"python -m ocio_oracle\"\n",
    "NAME = \"ocio_oracle.nowhere\"\n",
    "def argv(name):\n    return f\"ocio_oracle.{name}\"\n",
    "def argv(name):\n    return \"ocio_oracle.\" + name\n",
    "def f(n):\n    return f\"{n}.py\"\n",
    "def f(n):\n    return n + \".py\"\n",
    "def f(x):\n    return \"ocio_oracle/d\" + x\n",
    "def f(m):\n    return m.__file__\n",
    "import os\n\nX = os.path.join(os.path.dirname(os.path.abspath(__file__)), \"missing.py\")\n",
    "DATA = b\"from ocio_oracle import d\"\n",
    "DATA = b\"ocio_oracle.d\"\n",
    // Children's sources that can't be resolved, or that use what can't be.
    "CHILD = \"import importlib\\nimportlib.import_module('x')\\n\"\n",
    "CHILD = \"import ocio_oracle.nowhere\\n\"\n",
    "CHILD = \"import sys\\nsys.path.insert(0, %r)\\nfrom ocio_oracle import d\\n\"\n",
    "CHILD = \"from ocio_oracle import {}\\n\"\n",
    "def f(o):\n    return f\"import sys\\nsys.path.insert(0, {o!r})\\nfrom ocio_oracle import \
     d\\n\"\n",
    "CHILD = \"import sys\\nprint(sys.modules)\\n\"\n",
    "CHILD = \"import os\\nprint(open('ocio_oracle/d.py').read())\\n\"\n",
    // Processes the analysis can't follow.
    "import subprocess\n\n\ndef f(c):\n    return subprocess.run(c)\n",
    "import subprocess\n\n\ndef f():\n    return subprocess.run([\"python\", \"-c\", \"x\"])\n",
    "import subprocess\n\n\ndef f():\n    return subprocess.run([\"ls\"], shell=True)\n",
    "import subprocess\n\n\ndef f():\n    return subprocess.getoutput(\"x\")\n",
    "import subprocess\n\nRUN = subprocess.run\n",
    "import sys\n\nEXE = sys.executable\n",
    "import subprocess\nimport sys\n\nCHILD = \"__import__('json')\"\n\n\ndef f():\n    \
     return subprocess.run([sys.executable, \"-c\", CHILD])\n",
    "import subprocess\nimport sys\n\n\ndef f(p):\n    return subprocess.run([sys.executable, \
     p])\n",
    "import subprocess\nimport sys\n\n\ndef f():\n    src = \"import json\"\n    \
     return subprocess.run([sys.executable, \"-c\", src])\n",
    "import subprocess\nimport sys\n\nCHILD = \"import json\"\nCHILD = CHILD\n\n\ndef f():\n    \
     return subprocess.run([sys.executable, \"-c\", CHILD])\n",
    "import subprocess\nimport sys\n\n\ndef f():\n    return subprocess.run([sys.executable, \
     \"-c\", \"import json\" + \"\"])\n",
    // The command decorator out of place, and the exempt loader anywhere else.
    "from .commands import command\n\n\ndef make(f):\n    return command(f)\n",
    "import functools\n\nfrom .commands import command\n\n\n@command\n\
     @functools.lru_cache\ndef cached(args, blobs):\n    return 0, []\n",
    "from .commands import _import_package_modules as load\n\n\ndef f():\n    load()\n",
    "from . import commands\n\n\ndef f():\n    commands._import_package_modules()\n",
    "import importlib\n\n\ndef _import_package_modules():\n    \
     importlib.import_module(\"json\")\n",
    // A script, or a child, that may run a command by name.
    "import os\nimport subprocess\nimport sys\n\nSCRIPT = os.path.join(os.path.dirname(\
     os.path.abspath(__file__)), \"runner.py\")\n\n\ndef f():\n    \
     return subprocess.run([sys.executable, SCRIPT])\n",
    "import os\nimport subprocess\nimport sys\n\nSCRIPT = os.path.join(os.path.dirname(\
     os.path.abspath(__file__)), \"top_runner.py\")\n\n\ndef f():\n    \
     return subprocess.run([sys.executable, SCRIPT])\n",
    "CHILD = \"from ocio_oracle import commands\\ncommands.COMMANDS['x']({}, [])\\n\"\n",
    "CHILD = \"from ocio_oracle.commands import batch\\nbatch({}, [])\\n\"\n",
];

/// Code the analysis resolves, near what it can't: the command keeps a key of its own.
const RESOLVED: &[&str] = &[
    "import os\n\nSCRIPT = os.path.join(os.path.dirname(os.path.abspath(__file__)), \"d.py\")\n",
    "import os\n\n\ndef f():\n    oracle = os.path.dirname(os.path.dirname(os.path.abspath(\
     __file__)))\n    return {\"oracle_path\": oracle}\n",
    "import os\n\n\ndef f(m):\n    oracle = os.path.dirname(os.path.dirname(os.path.abspath(\
     __file__)))\n    return g(oracle, m)\n\n\ndef g(d, m):\n    return {\"d\": d, \"m\": m}\n",
    "import os\n\nORACLE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))\n\n\n\
     def f():\n    return os.path.samefile(os.getcwd(), ORACLE)\n",
    "from . import a\n\n\ndef f(a):\n    return a.anything\n",
    "def f():\n    import json\n    return json.dumps(1)\n\n\ndef g(json):\n    return json\n",
    "def f():\n    import os\n    return os.name\n\n\ndef g():\n    os = 1\n    return os\n",
    "def f(x):\n    return x.ctypes.data\n",
    "def f(cls):\n    return cls.__dict__, cls.executable\n",
    "import json\nimport os\n\n\ndef f():\n    return os.path.dirname(json.__file__)\n",
    "import json\n\n\ndef f(n):\n    return getattr(json, n)\n",
    "B = b\"[OpenColorIO \"\n",
    "def f(name):\n    return f\"ocio_oracle regen {name}\"\n",
    "PREFIX = \"ocio_oracle_blobs_\"\n",
    "import subprocess\nimport sys\n\nCHILD = \"import json\\nimport sys\\nsys.path.insert(0, \
     sys.argv[1])\\nfrom ocio_oracle import a\\nprint(a.helper())\\n\"\n\n\ndef f():\n    \
     return subprocess.run([sys.executable, \"-I\", \"-c\", CHILD, \"x\"])\n",
    "import os\nimport subprocess\nimport sys\n\nTOOL = os.path.join(os.path.dirname(\
     os.path.abspath(__file__)), \"tool.py\")\n\n\ndef f():\n    \
     return subprocess.run([sys.executable, \"-I\", \"-X\", \"utf8\", TOOL])\n",
    "import subprocess\n\n\ndef f(exe):\n    return subprocess.run([exe], check=True)\n",
    "def f(d):\n    return d[\"__name__\"], getattr(d, \"__doc__\")\n",
];

/// Each of [`UNRESOLVED`] gets the whole oracle's key, and so does its importer; each of
/// [`RESOLVED`] doesn't.
#[test]
fn an_unresolvable_module_gets_the_whole_oracles_key() {
    let dir = package("fallback");
    write(
        &dir,
        "runner.py",
        "from ocio_oracle.commands import COMMANDS\n\n\ndef main():\n    \
         return COMMANDS[\"x\"]({}, [])\n\n\nif __name__ == \"__main__\":\n    main()\n",
    );
    write(
        &dir,
        "top_runner.py",
        "def main():\n    pass\n\n\nif __name__ == \"__main__\":\n    \
         from ocio_oracle.commands import COMMANDS\n    COMMANDS[\"x\"]({}, [])\n",
    );
    write(
        &dir,
        "tool.py",
        "import json\n\n\ndef main():\n    print(json.dumps(1))\n\n\n\
         if __name__ == \"__main__\":\n    main()\n",
    );
    write(
        &dir,
        "f.py",
        "from . import e\nfrom .commands import command\n\n\n@command\n\
         def f_cmd(args, blobs):\n    return 0, []\n",
    );
    for (snippets, unresolved) in [(UNRESOLVED, true), (RESOLVED, false)] {
        for snippet in snippets {
            let source = format!(
                "from .commands import command\n{snippet}\n\n@command\n\
                 def e_cmd(args, blobs):\n    return 0, []\n"
            );
            write(&dir, "e.py", &source);
            let scopes = scopes(&dir);
            for command in ["e_cmd", "f_cmd"] {
                assert_eq!(
                    scopes.key(command, &json!({})) == FULL,
                    unresolved,
                    "{snippet}: {command}"
                );
            }
            for command in COMMANDS {
                assert_ne!(
                    scopes.key(command, &json!({})),
                    FULL,
                    "{snippet}: {command}"
                );
            }
        }
    }

    // An unknown command. (One that two modules define fails the import.)
    assert_eq!(self::scopes(&dir).key("no_such_command", &json!({})), FULL);
}

/// A module that fails to import fails every process of the oracle: the analysis fails, so
/// that every key is the whole oracle's.
#[test]
fn a_module_that_fails_to_import_fails_the_analysis() {
    let dir = package("import");
    assert!(try_scopes(&dir).is_ok());
    for source in [
        "X = {}[\"missing\"]\n",
        "def f(x=undefined_name):\n    return x\n",
        "import no_such_module_anywhere\n",
        "from .spec import no_such_name\n",
        "from .missing import thing\n",
        "import ocio_oracle.missing\n",
        "import sys\n\nsys.exit(0)\n",
        "def broken(:\n",
        "from .commands import command\n\n\n@command\ndef d_cmd(args, blobs):\n    \
         return 0, []\n",
    ] {
        write(&dir, "e.py", source);
        let result = try_scopes(&dir);
        assert!(result.is_err(), "{source}");
    }
}

/// `batch` and `with_files` are keyed by the commands they run, at any depth up to the limit;
/// a command that runs commands by name in any other way, or a request that doesn't name
/// them, gets the whole oracle's key.
#[test]
fn the_dispatchers_are_keyed_by_the_commands_they_run() {
    let dir = package("dispatch");
    write(
        &dir,
        "z.py",
        "from .commands import COMMANDS, command\n\n\n@command\n\
         def run_named(args, blobs):\n    return COMMANDS[args[\"name\"]](args, blobs)\n",
    );
    write(
        &dir,
        "w.py",
        "from .commands import COMMANDS as REGISTRY, command\n\n\n@command\n\
         def run_aliased(args, blobs):\n    return REGISTRY[args[\"name\"]](args, blobs)\n\n\n\
         @command\ndef run_local(args, blobs):\n    \
         from .commands import COMMANDS as table\n    return table[args[\"name\"]](args, blobs)\n",
    );
    write(
        &dir,
        "y.py",
        "from .commands import batch, command\n\n\n@command\n\
         def via_batch(args, blobs):\n    return batch(args, blobs)\n",
    );
    write(
        &dir,
        "x.py",
        "from . import commands\nfrom .alias import run\nfrom .commands import command\n\n\n\
         @command\ndef via_alias(args, blobs):\n    return run(args, blobs)\n\n\n\
         @command\ndef via_module(args, blobs):\n    return commands.batch(args, blobs)\n",
    );
    // A module-level name bound to a dispatcher, used from another module.
    write(
        &dir,
        "alias.py",
        "from .commands import batch\n\nrun = batch\n",
    );
    // `serve_one` and `main` dispatch in `__main__.py`, not here; a local `COMMANDS`-free
    // table shadows nothing.
    write(
        &dir,
        "m.py",
        "from .commands import command\n\n\ndef serve_one():\n    return 1\n\n\n\
         def main():\n    return serve_one()\n\n\n@command\ndef m_cmd(args, blobs):\n    \
         batch = {}\n    return main(), batch\n",
    );
    assert_ne!(scopes(&dir).key("m_cmd", &json!({})), FULL);
    let call = |cmd: &str| json!({"cmd": cmd, "args": {}});
    let requests = [
        ("batch", json!({"calls": [call("a_cmd"), call("d_cmd")]})),
        ("batch", json!({"calls": [call("a_cmd")]})),
        ("with_files", json!({"command": "d_cmd", "args": {}})),
        (
            "batch",
            json!({"calls": [{"cmd": "with_files", "args": {"command": "d_cmd"}}]}),
        ),
    ];
    let keys = |scopes: &Scopes| -> Vec<u128> {
        requests
            .iter()
            .map(|(cmd, args)| scopes.key(cmd, args))
            .collect()
    };
    let before = keys(&scopes(&dir));
    assert!(before.iter().all(|&k| k != FULL), "{before:x?}");
    touch(&dir, "d.py");
    let after_d = keys(&scopes(&dir));
    assert_eq!(
        (0..4).map(|i| before[i] != after_d[i]).collect::<Vec<_>>(),
        [true, false, true, true]
    );
    touch(&dir, "b.py");
    assert_eq!(keys(&scopes(&dir)), after_d, "b.py");
    touch(&dir, "a.py");
    let after_a = keys(&scopes(&dir));
    assert_eq!(
        (0..4).map(|i| after_d[i] != after_a[i]).collect::<Vec<_>>(),
        [true, true, false, false]
    );
    touch(&dir, "files_api.py");
    let after_files = keys(&scopes(&dir));
    assert_eq!(
        (0..4)
            .map(|i| after_a[i] != after_files[i])
            .collect::<Vec<_>>(),
        [false, false, true, true]
    );

    let scopes = scopes(&dir);
    for (cmd, args) in [
        ("batch", json!({})),
        ("batch", json!({"calls": "a_cmd"})),
        ("batch", json!({"calls": [{"cmd": 3}]})),
        ("batch", json!({"calls": [call("no_such_command")]})),
        ("with_files", json!({"args": {}})),
        ("with_files", json!({"command": "no_such_command"})),
        ("run_named", json!({"name": "a_cmd"})),
        ("run_aliased", json!({"name": "a_cmd"})),
        ("run_local", json!({"name": "a_cmd"})),
        ("via_batch", json!({"calls": [call("a_cmd")]})),
        ("via_alias", json!({"calls": [call("a_cmd")]})),
        ("via_module", json!({"calls": [call("a_cmd")]})),
    ] {
        assert_eq!(scopes.key(cmd, &args), FULL, "{cmd} {args}");
    }
    // Commands nested to the depth limit, 4 below the request, and past it.
    let nest = |depth: usize| {
        let mut request = call("a_cmd");
        for _ in 0..depth {
            request = json!({"cmd": "batch", "args": {"calls": [request]}});
        }
        request
    };
    assert_ne!(scopes.key("batch", &nest(4)["args"]), FULL);
    assert_eq!(scopes.key("batch", &nest(5)["args"]), FULL);
}

/// The functions the keys rely on are pinned: the oracle's pins are the ones
/// `oracle_scope.rs` names; a `batch` that runs more than its `calls` (or a `with_files`
/// that changed) gets the whole oracle's key; a registry or a loader that changed fails the
/// analysis.
#[test]
fn the_functions_the_keys_rely_on_are_pinned() {
    let oracle = Oracle::get();
    let analysis: Value = serde_json::from_slice(
        &super::analyze(ANALYZER, oracle.python(), &paths::oracle_dir()).unwrap(),
    )
    .unwrap();
    for (name, pin) in [REGISTRY_PIN, LOADER_PIN] {
        assert_eq!(analysis["pins"][name], json!(pin), "{name}");
    }
    for (command, file, pin) in NESTED {
        assert_eq!(
            analysis["pins"][format!("{file}:{command}")],
            json!(pin),
            "{command}"
        );
    }

    let dir = package("pins");
    let request = json!({"calls": [{"cmd": "a_cmd"}], "then": "d_cmd"});
    assert_ne!(scopes(&dir).key("batch", &request), FULL);
    let commands = commands_py();
    let then = commands.replace(
        "        out.extend(call_blobs)\n    return results, out",
        "        out.extend(call_blobs)\n    if \"then\" in args:\n        \
         COMMANDS[args[\"then\"]]({}, [])\n    return results, out",
    );
    assert_ne!(then, commands);
    write(&dir, "commands.py", &then);
    assert_eq!(scopes(&dir).key("batch", &request), FULL);
    assert_ne!(scopes(&dir).key("a_cmd", &json!({})), FULL);
    write(&dir, "commands.py", &commands);

    let files = files_api_py();
    let other = files.replace("ClearAllCaches()", "ClearAllCaches(1)");
    assert_ne!(other, files);
    write(&dir, "files_api.py", &other);
    let request = json!({"command": "d_cmd", "args": {}});
    assert_eq!(scopes(&dir).key("with_files", &request), FULL);
    write(&dir, "files_api.py", &files);
    assert_ne!(scopes(&dir).key("with_files", &request), FULL);

    for (old, new) in [
        (
            "    COMMANDS[fn.__name__] = fn\n",
            "    COMMANDS[fn.__name__] = fn\n    fn.registered = True\n",
        ),
        (
            "        importlib.import_module(",
            "        print(name)\n        importlib.import_module(",
        ),
    ] {
        let changed = commands.replace(old, new);
        assert_ne!(changed, commands, "{old}");
        write(&dir, "commands.py", &changed);
        assert!(try_scopes(&dir).is_err(), "{new}");
    }
}

/// Strings that name a module are edges: a sibling's file a module starts as a script, the
/// imports of a child process's source (which binds its names in a scope of its own), and the
/// bare imports of a script; a script's and a child's own reach is followed.
#[test]
fn strings_that_name_a_module_are_edges() {
    let dir = package("strings");
    write(
        &dir,
        "tool.py",
        "import json\n\n\ndef main():\n    import helper\n    from helper2 import value\n    \
         print(json.dumps(helper.value() + value()))\n\n\nif __name__ == \"__main__\":\n    \
         main()\n",
    );
    write(&dir, "helper.py", "def value():\n    return 1\n");
    write(&dir, "helper2.py", "def value():\n    return 2\n");
    write(
        &dir,
        "s.py",
        "import os\nimport subprocess\nimport sys\n\nfrom .commands import command\n\n\
         SCRIPT = os.path.join(os.path.dirname(os.path.abspath(__file__)), \"tool.py\")\n\n\n\
         @command\ndef s_cmd(args, blobs):\n    \
         return subprocess.run([sys.executable, SCRIPT]).returncode, []\n",
    );
    write(
        &dir,
        "t.py",
        "import subprocess\nimport sys\n\nfrom .commands import command\n\n\
         CHILD = \"import sys\\nsys.path.insert(0, sys.argv[1])\\nfrom ocio_oracle import d\\n\"\n\n\n\
         @command\ndef t_cmd(args, blobs):\n    \
         return subprocess.run([sys.executable, \"-c\", CHILD, \".\"]).returncode, []\n",
    );
    // The child binds `a` to `d`; the module's own `a` is still `a.py`.
    write(
        &dir,
        "u.py",
        "from . import a\nfrom .commands import command\n\n\
         NOTE = \"from ocio_oracle import d as a\"\n\n\n@command\ndef u_cmd(args, blobs):\n    \
         return a.helper(), []\n",
    );
    let commands = ["s_cmd", "t_cmd", "u_cmd", "a_cmd"];
    let before = keys(&scopes(&dir), &commands);
    assert!(before.iter().all(|&k| k != FULL), "{before:x?}");
    touch(&dir, "tool.py");
    let after_tool = keys(&scopes(&dir), &commands);
    assert_eq!(changed(&commands, &before, &after_tool), ["s_cmd"]);
    touch(&dir, "helper.py");
    let after_helper = keys(&scopes(&dir), &commands);
    assert_eq!(changed(&commands, &after_tool, &after_helper), ["s_cmd"]);
    touch(&dir, "helper2.py");
    let after_helper2 = keys(&scopes(&dir), &commands);
    assert_eq!(changed(&commands, &after_helper, &after_helper2), ["s_cmd"]);
    touch(&dir, "d.py");
    let after_d = keys(&scopes(&dir), &commands);
    assert_eq!(
        changed(&commands, &after_helper2, &after_d),
        ["t_cmd", "u_cmd"]
    );
    touch(&dir, "a.py");
    let after_a = keys(&scopes(&dir), &commands);
    assert_eq!(changed(&commands, &after_d, &after_a), ["u_cmd", "a_cmd"]);
}

/// The analysis is kept in the cache directory, keyed by what it reads: the same sources read
/// it back; changed sources, another analyzer or other environment files are analysed again.
/// A cached analysis that doesn't parse, or gives no scopes, is made again. An analysis whose
/// sources changed while it ran is refused, and not kept.
#[test]
fn the_cached_analysis_follows_the_sources() {
    let dir = package("cached");
    let cache = dir.join("cache");
    let python = Oracle::get().python();
    let compute_with = |analyzer: &str| {
        Scopes::compute_with(analyzer, python, &dir, FULL, ENV, Some(&cache))
            .unwrap_or_else(|e| panic!("{e}"))
    };
    let compute = || compute_with(ANALYZER);
    let first = keys(&compute(), &COMMANDS);
    let entries = || -> Vec<PathBuf> {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&cache)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        entries
    };
    assert_eq!(entries().len(), 1);
    assert_eq!(keys(&compute(), &COMMANDS), first);
    assert_eq!(
        keys(&scopes(&dir), &COMMANDS),
        first,
        "the cache changed the keys"
    );

    // Cut short, or of another format: made again, in place.
    for broken in [
        &b"{\"format\": 2, \"mod"[..],
        &b"{\"format\": 1, \"modules\": {}}"[..],
    ] {
        std::fs::write(&entries()[0], broken).unwrap();
        assert_eq!(keys(&compute(), &COMMANDS), first);
        assert_eq!(entries().len(), 1);
        assert_ne!(std::fs::read(&entries()[0]).unwrap(), broken);
    }

    // Another analyzer, and another environment file: analysed again.
    let changed_analyzer = format!("{ANALYZER}# changed\n");
    assert_eq!(keys(&compute_with(&changed_analyzer), &COMMANDS).len(), 5);
    assert_eq!(entries().len(), 2);
    std::fs::write(dir.join("uv.lock"), "changed").unwrap();
    compute();
    assert_eq!(entries().len(), 3);

    write(
        &dir,
        "b.py",
        "from .commands import command\n\n\n@command\ndef b_cmd(args, blobs):\n    \
         return 2, []\n",
    );
    assert!(!compute().modules_of("b_cmd").unwrap().contains(&"a.py"));
    assert_eq!(entries().len(), 4);
    let before = keys(&compute(), &COMMANDS);
    touch(&dir, "a.py");
    let after = keys(&compute(), &COMMANDS);
    assert_eq!(changed(&COMMANDS, &before, &after), ["a_cmd"]);

    // An analyzer that changes a module while it runs: refused, and not kept.
    let count = entries().len();
    let touching = format!(
        "import os, sys\nwith open(os.path.join(sys.argv[1], 'd.py'), 'a') as f:\n    \
         f.write('# touched\\n')\n{ANALYZER}"
    );
    let result = Scopes::compute_with(&touching, python, &dir, FULL, ENV, Some(&cache));
    assert!(result.is_err());
    assert_eq!(entries().len(), count);
}

/// The analysis of the oracle finds every command of its registry, in the module that
/// registers it, and nothing else; the battery's command, `cpu_apply`, gets a key of its own,
/// which a change to `config_api.py` (most commands' helper) leaves alone; and most commands
/// get keys of their own.
#[test]
fn the_analysis_of_the_oracle_follows_its_registry() {
    let oracle = Oracle::get();
    let scopes = Scopes::compute(oracle.python(), &paths::oracle_dir(), FULL, ENV, None)
        .unwrap_or_else(|e| panic!("{e}"));
    let registry = oracle.run_script(
        "import json\n\
         from ocio_oracle import commands\n\
         print(json.dumps(sorted([name, fn.__module__.rpartition('.')[2] + '.py']\n\
                                 for name, fn in commands.COMMANDS.items())))\n",
        &[],
    );
    let registry: Vec<(String, String)> = serde_json::from_str::<Value>(&registry.join("\n"))
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| {
            (
                pair[0].as_str().unwrap().to_string(),
                pair[1].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(registry.len() > 30, "{registry:?}");
    assert_eq!(scopes.command_modules(), registry);

    let cpu_apply = scopes.modules_of("cpu_apply").expect("cpu_apply is scoped");
    assert!(cpu_apply.contains(&"commands.py"), "{cpu_apply:?}");
    assert!(!cpu_apply.contains(&"config_api.py"), "{cpu_apply:?}");
    // A guard against an analysis that silently gives up: three in four commands are scoped.
    let scoped = registry
        .iter()
        .filter(|(c, _)| scopes.modules_of(c).is_some())
        .count();
    assert!(
        scoped * 4 >= registry.len() * 3,
        "{scoped} of {}",
        registry.len()
    );
}
