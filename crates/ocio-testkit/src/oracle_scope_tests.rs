// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the scoped cache keys, on small packages laid out as the oracle's is, analysed by
//! the oracle's Python; and on the oracle itself, against its registry.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{ANALYZER, Scopes};
use crate::Oracle;
use crate::paths;

/// The whole oracle's key in these tests.
const FULL: u128 = 0xF011_F011;
/// The environment's hash in these tests.
const ENV: u128 = 0xE0E0;

/// The modules of the test package: the shared ones as the oracle has them (a registry that
/// imports every module, a dispatcher, a helper the registry imports), a chain `c` -> `b` ->
/// `a`, an unrelated `d`, and `with_files`. The shared modules' top levels only define names
/// (the oracle's call `_import_package_modules()` and `sys.exit(...)`), so that what makes
/// them shared is their being the modules every process runs, not what their top level does.
const PACKAGE: &[(&str, &str)] = &[
    ("__init__.py", "\"\"\"The package.\"\"\"\n"),
    (
        "__main__.py",
        "import sys\n\nfrom . import commands, regen\n\n\ndef serve_one():\n    \
         return commands.COMMANDS.get(sys.argv[1])\n\n\ndef main():\n    return serve_one()\n",
    ),
    ("regen.py", "GROUPS = {}\n"),
    (
        "commands.py",
        r#"import importlib
import os
import pkgutil

from . import spec

COMMANDS = {}


def command(fn):
    COMMANDS[fn.__name__] = fn
    return fn


@command
def info(args, blobs):
    return spec.version(), []


@command
def batch(args, blobs):
    return [COMMANDS[call["cmd"]](call.get("args") or {}, []) for call in args["calls"]], []


def _import_package_modules():
    package_dir = os.path.dirname(os.path.abspath(__file__))
    for module in pkgutil.iter_modules([package_dir]):
        importlib.import_module(f"{__package__}.{module.name}")
"#,
    ),
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
    (
        "files.py",
        "from .commands import COMMANDS, command\n\n\n@command\n\
         def with_files(args, blobs):\n    \
         return COMMANDS[args[\"command\"]](args.get(\"args\", {}), blobs)\n",
    ),
];

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

/// A new oracle directory for `test`, holding [`PACKAGE`] and the environment's files.
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
    Package(dir)
}

/// Writes the package's file `name`.
fn write(dir: &Path, name: &str, text: &str) {
    let file = dir.join("ocio_oracle").join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

/// Adds a comment to the package's file `name`, which changes its bytes and nothing else.
fn touch(dir: &Path, name: &str) {
    let file = dir.join("ocio_oracle").join(name);
    let mut text = std::fs::read_to_string(&file).unwrap();
    text.push_str("# changed\n");
    std::fs::write(file, text).unwrap();
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

/// From an analysis alone: each of the modules every process runs is in every key, even with
/// no edge to it; an edge to a module the package doesn't have, or an analysis of other
/// modules than the package's, can't be resolved.
#[test]
fn the_roots_are_shared_and_unknown_modules_unresolved() {
    let module = |edges: &[&str], commands: &[&str]| {
        json!({"edges": edges, "commands": commands, "dispatchers": [], "fallback": [],
               "global": []})
    };
    let modules = |names: &[&str]| -> Vec<(String, u128)> {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.to_string(), i as u128))
            .collect()
    };
    let names = ["__init__.py", "__main__.py", "commands.py", "x.py", "y.py"];
    let manifest = json!({"format": 1, "modules": {
        "__init__.py": module(&[], &[]),
        "__main__.py": module(&[], &[]),
        "commands.py": module(&[], &["info"]),
        "x.py": module(&[], &["x_cmd"]),
        "y.py": module(&["missing.py"], &["y_cmd"]),
    }});
    let scopes = Scopes::from_manifest(FULL, ENV, modules(&names), &manifest).unwrap();
    assert_eq!(
        scopes.modules_of("x_cmd").unwrap(),
        ["__init__.py", "__main__.py", "commands.py", "x.py"]
    );
    assert_ne!(scopes.key("x_cmd", &json!({})), FULL);
    assert_eq!(scopes.key("y_cmd", &json!({})), FULL);
    assert!(Scopes::from_manifest(FULL, ENV, modules(&names[..4]), &manifest).is_err());
    assert!(Scopes::from_manifest(FULL, ENV, modules(&names), &json!({"format": 2})).is_err());
}

/// A key names the modules it covers: two modules with the same bytes aren't the same module.
#[test]
fn the_key_names_the_modules_it_covers() {
    let module = |edges: &[&str], commands: &[&str]| {
        json!({"edges": edges, "commands": commands, "dispatchers": [], "fallback": [],
               "global": []})
    };
    let names = [
        "__init__.py",
        "__main__.py",
        "commands.py",
        "p.py",
        "q.py",
        "x.py",
    ];
    // `p.py` and `q.py` have the same hash.
    let modules: Vec<(String, u128)> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.to_string(), if i == 4 { 3 } else { i as u128 }))
        .collect();
    let key = |edge: &str| {
        let manifest = json!({"format": 1, "modules": {
            "__init__.py": module(&[], &[]),
            "__main__.py": module(&[], &[]),
            "commands.py": module(&[], &[]),
            "p.py": module(&[], &[]),
            "q.py": module(&[], &[]),
            "x.py": module(&[edge], &["x_cmd"]),
        }});
        Scopes::from_manifest(FULL, ENV, modules.clone(), &manifest)
            .unwrap()
            .key("x_cmd", &json!({}))
    };
    assert_ne!(key("p.py"), FULL);
    assert_ne!(key("p.py"), key("q.py"));
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

/// The top-level statements that make a module shared, and some that don't.
#[test]
fn a_module_whose_top_level_runs_code_is_shared() {
    let shared = [
        "import os\n\nos.environ[\"G\"] = \"1\"\n",
        "import PyOpenColorIO as OCIO\n\nOCIO.SetLoggingLevel(OCIO.LOGGING_LEVEL_NONE)\n",
        "import PyOpenColorIO as OCIO\n\nLEVEL = OCIO.SetLoggingLevel(0)\n",
        "from . import spec\n\nspec.version = lambda: 2\n",
        "for i in range(3):\n    pass\n",
        "def f(x=print(\"x\")):\n    return x\n",
        "import functools\n\n\n@functools.singledispatch\ndef f(x):\n    return x\n",
        "class C:\n    print(\"x\")\n",
        "try:\n    import foo\nexcept ImportError:\n    pass\n",
    ];
    let private = [
        "import re\n\nPATTERN = re.compile(\"x\")\n",
        "KEYS = {\"a\": 1, \"b\": (2, 3)}\nNAMES = frozenset(KEYS)\n",
        "import contextlib\n\n\n@contextlib.contextmanager\ndef f():\n    yield\n",
        "class C(Exception):\n    \"\"\"Doc.\"\"\"\n\n    def m(self, x=1):\n        \
         return print(x)\n",
        "TABLE = {\"f\": lambda x: print(x)}\n",
        "def main():\n    print(\"x\")\n\n\nif __name__ == \"__main__\":\n    main()\n",
    ];
    let dir = package("top-level");
    for (source, is_shared) in shared
        .iter()
        .map(|s| (s, true))
        .chain(private.iter().map(|s| (s, false)))
    {
        write(&dir, "g.py", source);
        let before = keys(&scopes(&dir), &COMMANDS);
        touch(&dir, "g.py");
        let after = keys(&scopes(&dir), &COMMANDS);
        let expected: &[&str] = if is_shared { &COMMANDS } else { &[] };
        assert_eq!(changed(&COMMANDS, &before, &after), expected, "{source}");
    }
}

/// A command whose module, or a module it reaches, does what the analysis can't follow gets
/// the whole oracle's key, and so do its importers; other commands keep theirs.
#[test]
fn an_unresolvable_module_gets_the_whole_oracles_key() {
    let cases = [
        "import importlib\n\n\ndef load(name):\n    return importlib.import_module(name)\n",
        "from importlib import import_module\n\n\ndef load(name):\n    \
         return import_module(name)\n",
        "def load(name):\n    return __import__(name)\n",
        "def run(code):\n    exec(code)\n",
        "def run(code):\n    return eval(code)\n",
        "import sys\n\n\ndef load(name):\n    return sys.modules[name]\n",
        "import sys\n\n\ndef add(path):\n    sys.path.insert(0, path)\n",
        "import builtins\n\n\ndef load(name):\n    return getattr(builtins, name)\n",
        "def load(name):\n    return __builtins__[name]\n",
        "def load(d):\n    return d[\"__import__\"]\n",
        "from .missing import thing\n",
        "from .. import other\n",
        "from .a import *\n",
        "import ocio_oracle.missing\n",
        "import ocio_oracle\n\n\ndef reach():\n    return ocio_oracle.d\n",
        "from . import a\n\n\ndef reach(name):\n    return getattr(a, name)\n",
        "import os\n\nHERE = os.path.dirname(__file__)\n",
        "import os\n\nDATA = os.path.join(os.path.dirname(os.path.abspath(__file__)), \
         \"table.json\")\n",
        "ARGV = [\"-m\", \"ocio_oracle.nowhere\"]\n",
        "def argv(name):\n    return [\"-m\", f\"ocio_oracle.{name}\"]\n",
        "CHILD = \"import importlib\\nimportlib.import_module('x')\\n\"\n",
        "CHILD = \"import ocio_oracle.nowhere\\n\"\n",
        "from .commands import command\n\n\ndef make(f):\n    return command(f)\n",
        "import functools\n\nfrom .commands import command\n\n\n@command\n\
         @functools.lru_cache\ndef cached(args, blobs):\n    return 0, []\n",
        // Running the package, a script that runs commands by name, or a child that does.
        "ARGV = [\"-m\", \"ocio_oracle\"]\n",
        "COMMAND = \"python -m ocio_oracle\"\n",
        "import os\n\nSCRIPT = os.path.join(os.path.dirname(os.path.abspath(__file__)), \
         \"runner.py\")\n",
        "ARGV = [\"-m\", \"ocio_oracle.runner\"]\n",
        "CHILD = \"from ocio_oracle import commands\\ncommands.COMMANDS['x']({}, [])\\n\"\n",
        "CHILD = \"from ocio_oracle.commands import batch\\nbatch({}, [])\\n\"\n",
    ];
    let dir = package("fallback");
    write(
        &dir,
        "runner.py",
        "from ocio_oracle.commands import COMMANDS\n\n\ndef main():\n    \
         return COMMANDS[\"x\"]({}, [])\n\n\nif __name__ == \"__main__\":\n    main()\n",
    );
    write(
        &dir,
        "f.py",
        "from . import e\nfrom .commands import command\n\n\n@command\n\
         def f_cmd(args, blobs):\n    return 0, []\n",
    );
    for snippet in cases {
        let source = format!(
            "from .commands import command\n{snippet}\n\n@command\n\
             def e_cmd(args, blobs):\n    return 0, []\n"
        );
        write(&dir, "e.py", &source);
        let scopes = scopes(&dir);
        assert_eq!(scopes.key("e_cmd", &json!({})), FULL, "{snippet}");
        assert_eq!(scopes.key("f_cmd", &json!({})), FULL, "{snippet}");
        for command in COMMANDS {
            assert_ne!(
                scopes.key(command, &json!({})),
                FULL,
                "{snippet}: {command}"
            );
        }
    }

    // An unknown command, and one that two modules define.
    write(
        &dir,
        "e.py",
        "from .commands import command\n\n\n@command\ndef d_cmd(args, blobs):\n    return 0, []\n",
    );
    let scopes = scopes(&dir);
    assert_eq!(scopes.key("no_such_command", &json!({})), FULL);
    assert_eq!(scopes.key("d_cmd", &json!({})), FULL);
    assert_ne!(scopes.key("a_cmd", &json!({})), FULL);

    // A module that doesn't parse: no one knows what importing it does.
    write(&dir, "e.py", "def broken(:\n");
    let scopes = self::scopes(&dir);
    for command in COMMANDS {
        assert_eq!(scopes.key(command, &json!({})), FULL, "{command}");
    }
}

/// `batch` and `with_files` are keyed by the commands they run, at any depth; a command that
/// runs commands by name in any other way, or a request that doesn't name them, gets the whole
/// oracle's key.
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
    // `serve_one` and `main` dispatch in `__main__.py`, not here.
    write(
        &dir,
        "m.py",
        "from .commands import command\n\n\ndef serve_one():\n    return 1\n\n\n\
         def main():\n    return serve_one()\n\n\n@command\ndef m_cmd(args, blobs):\n    \
         return main(), []\n",
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
    touch(&dir, "files.py");
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

/// Strings that name a module are edges: a sibling's file a module starts as a script, the
/// imports of a child process's source, and a module named for `python -m`.
#[test]
fn strings_that_name_a_module_are_edges() {
    let dir = package("strings");
    write(
        &dir,
        "tool.py",
        "import json\n\n\ndef main():\n    print(json.dumps(1))\n\n\n\
         if __name__ == \"__main__\":\n    main()\n",
    );
    write(
        &dir,
        "s.py",
        "import os\n\nfrom .commands import command\n\n\
         SCRIPT = os.path.join(os.path.dirname(os.path.abspath(__file__)), \"tool.py\")\n\
         ORACLE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))\n\n\n\
         @command\ndef s_cmd(args, blobs):\n    return SCRIPT, []\n",
    );
    write(
        &dir,
        "t.py",
        "from .commands import command\n\n\
         CHILD = \"import sys\\nsys.path.insert(0, sys.argv[1])\\nfrom ocio_oracle import d\\n\"\n\n\n\
         @command\ndef t_cmd(args, blobs):\n    return CHILD, []\n",
    );
    write(
        &dir,
        "u.py",
        "from .commands import command\n\nARGV = [\"-m\", \"ocio_oracle.d\"]\n\n\n\
         @command\ndef u_cmd(args, blobs):\n    return ARGV, []\n",
    );
    let commands = ["s_cmd", "t_cmd", "u_cmd", "a_cmd"];
    let before = keys(&scopes(&dir), &commands);
    assert!(before.iter().all(|&k| k != FULL), "{before:x?}");
    touch(&dir, "tool.py");
    let after_tool = keys(&scopes(&dir), &commands);
    assert_eq!(changed(&commands, &before, &after_tool), ["s_cmd"]);
    touch(&dir, "d.py");
    let after_d = keys(&scopes(&dir), &commands);
    assert_eq!(
        changed(&commands, &after_tool, &after_d),
        ["t_cmd", "u_cmd"]
    );
}

/// The analysis is kept in the cache directory, keyed by what it reads: the same sources read
/// it back; changed sources (here, `b` no longer imports `a`), another analyzer or other
/// environment files are analysed again. A cached analysis that doesn't parse, or gives no
/// scopes, is made again. An analysis whose sources changed while it ran is refused, and not
/// kept.
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
        &b"{\"format\": 1, \"mod"[..],
        &b"{\"format\": 2, \"modules\": {}}"[..],
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
/// registers it, and nothing else; and the battery's command, `cpu_apply`, gets a key of its
/// own, which a change to `config_api.py` (most commands' helper) leaves alone.
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
}
