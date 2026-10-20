// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `py_suite` command (`oracle/ocio_oracle/py_suite_api.py`, O6.3), against
//! upstream's own driver: `OpenColorIOTestSuite.py`, run unmodified in the oracle's
//! environment, runs every module of its list in one process; the command, one module at a
//! time, must report the same tests with the same outcomes. A synthetic module of each
//! outcome unittest has checks the report's shape, its environment (the data directory, the
//! module path, the flags, an empty working directory) and that nothing the tests print
//! reaches the report. A module that fails to import, one that ends its process, and bad
//! requests are reported or refused.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::text;
use ocio_testkit::paths;
use serde_json::{Value, json};

/// Upstream's tests directory, as a plain path: on Windows, the workspace root is canonical,
/// a verbatim path (`\\?\D:\...`), where `/` doesn't separate names, and a test that joins
/// `shot1/lut1.clf` to a search path (`OCIOZArchiveTest`) finds no file under it.
fn upstream_tests() -> PathBuf {
    let dir = paths::upstream_dir().join("tests");
    let text = dir.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(plain) if plain.as_bytes().get(1) == Some(&b':') => PathBuf::from(plain),
        _ => dir,
    }
}

/// The modules of upstream's driver, in its order: the entries of `get_test_modules`
/// (`OpenColorIOTestSuite.py`), `("Name", Name),`.
fn driver_modules() -> Vec<String> {
    let driver = std::fs::read_to_string(
        upstream_tests()
            .join("python")
            .join("OpenColorIOTestSuite.py"),
    )
    .unwrap();
    let modules: Vec<String> = driver
        .lines()
        .filter_map(|line| {
            let entry = line.trim().strip_prefix("(\"")?.strip_suffix("),")?;
            let (quoted, name) = entry.split_once("\", ")?;
            (quoted == name).then(|| name.to_string())
        })
        .collect();
    assert_eq!(modules.len(), 48, "{modules:?}");
    modules
}

/// Upstream's driver, run as cibuildwheel runs it (no arguments), from an empty directory:
/// each test's id and outcome from unittest's verbose output, in run order, and the count of
/// its "Ran N tests" line.
fn run_driver() -> (Vec<(String, String)>, usize) {
    let cwd = paths::target_dir().join(format!("py-suite-driver-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cwd);
    std::fs::create_dir_all(&cwd).unwrap();
    let mut command = Oracle::get().command();
    command
        .current_dir(&cwd)
        .args(["-I", "-B", "-X", "utf8"])
        .arg(
            upstream_tests()
                .join("python")
                .join("OpenColorIOTestSuite.py"),
        );
    let output = command.output().expect("the oracle's Python runs");
    let left: Vec<_> = std::fs::read_dir(&cwd).unwrap().flatten().collect();
    std::fs::remove_dir_all(&cwd).unwrap();
    assert!(left.is_empty(), "the driver left {left:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);

    // "test_name (Module.Class.test_name)", then on the same line or after the test's
    // docstring, " ... ok" (or "FAIL", "ERROR", "skipped '...'", ...).
    let mut tests = Vec::new();
    let mut current: Option<String> = None;
    let mut ran = None;
    for line in stderr.lines() {
        if let Some(rest) = line.strip_prefix("Ran ") {
            ran = rest.split(' ').next().and_then(|n| n.parse().ok());
        }
        if line.starts_with("test")
            && let Some((name, rest)) = line.split_once(" (")
            && let Some(id) = rest.split(')').next()
            && id.ends_with(&format!(".{name}"))
        {
            current = Some(id.to_string());
        }
        if let Some(id) = current.as_ref()
            && let Some((_, outcome)) = line.rsplit_once(" ... ")
        {
            let outcome = match outcome {
                "ok" => "pass",
                "FAIL" => "fail",
                "ERROR" => "error",
                "expected failure" => "expected_failure",
                "unexpected success" => "unexpected_success",
                o if o.starts_with("skipped ") => "skip",
                o => panic!("{id}: outcome {o:?}"),
            };
            tests.push((id.clone(), outcome.to_string()));
            current = None;
        }
    }
    let passed = tests.iter().all(|(_, o)| o != "fail" && o != "error");
    assert_eq!(output.status.success(), passed, "{stderr}");
    (tests, ran.expect("a \"Ran N tests\" line"))
}

/// Every module of upstream's driver, through the command, reports the tests the driver runs,
/// with the same outcomes: the wheel's baseline.
#[test]
fn reports_each_module_as_upstreams_driver_runs_it() {
    let modules = driver_modules();
    let tests_dir = upstream_tests().to_string_lossy().into_owned();
    let calls: Vec<BatchCall<'_>> = modules
        .iter()
        .map(|m| BatchCall {
            cmd: "py_suite",
            args: json!({"tests": tests_dir, "module": m}),
            blobs: Vec::new(),
        })
        .collect();
    let mut reported = BTreeMap::new();
    let mut defined = BTreeSet::new();
    for (module, response) in modules.iter().zip(Oracle::get().batch(&calls, true)) {
        let report = response.unwrap().result;
        assert_eq!(report["module"], json!(module), "{report}");
        assert!(report["import_error"].is_null(), "{module}: {report}");
        assert!(report["numpy"].is_string(), "{module}: no NumPy");
        let tests = report["tests"].as_array().unwrap();
        assert!(!tests.is_empty(), "{module}: no tests");
        for t in tests {
            let id = t["id"].as_str().unwrap().to_string();
            assert!(id.starts_with(&format!("{module}.")), "{module}: {id}");
            let outcome = t["outcome"].as_str().unwrap().to_string();
            assert_eq!(
                t["message"].is_null(),
                outcome == "pass" || outcome == "unexpected_success",
                "{id}: {t}"
            );
            assert!(reported.insert(id.clone(), outcome).is_none(), "{id} twice");
            defined.insert(t["defined"].as_str().unwrap().to_string());
        }
    }
    // Where each run's method is defined: together, every test function of upstream's files.
    assert_eq!(defined, upstream_test_functions());

    let (driven, ran) = run_driver();
    assert_eq!(ran, driven.len());
    let driven: BTreeMap<String, String> = driven.into_iter().collect();
    assert_eq!(driven.len(), ran, "a test ran twice");
    assert_eq!(reported, driven);
}

/// Upstream's test functions, "File.py::Class.test_name": each `    def test...` line of
/// tests/python/*.py, in the class whose `class` line comes before it.
fn upstream_test_functions() -> BTreeSet<String> {
    let dir = upstream_tests().join("python");
    let mut out = BTreeSet::new();
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        if !file.ends_with(".py") {
            continue;
        }
        let text = std::fs::read_to_string(entry.path()).unwrap();
        let mut class = "";
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("class ") {
                class = rest.split(['(', ':']).next().unwrap().trim();
            } else if let Some(rest) = line.strip_prefix("    def test") {
                let name = rest.split('(').next().unwrap();
                out.insert(format!("{file}::{class}.test{name}"));
            }
        }
    }
    assert_eq!(out.len(), 384, "the suite's test functions");
    out
}

/// A tests directory of synthetic modules, under the target directory.
struct Synthetic {
    dir: PathBuf,
}

impl Synthetic {
    fn new(modules: &[(&str, &str)]) -> Synthetic {
        let dir = paths::target_dir().join(format!(
            "py-suite-synthetic-{}-{}",
            std::process::id(),
            modules[0].0
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("python")).unwrap();
        std::fs::create_dir_all(dir.join("data").join("files")).unwrap();
        for (name, source) in modules {
            std::fs::write(dir.join("python").join(format!("{name}.py")), source).unwrap();
        }
        Synthetic { dir }
    }

    fn args(&self, module: &str) -> Value {
        json!({"tests": self.dir.to_string_lossy(), "module": module})
    }

    /// Uncached: the cache key doesn't cover these files.
    fn run(&self, module: &str) -> Value {
        Oracle::get()
            .call_uncached("py_suite", self.args(module), &[])
            .result
    }

    fn python_dir(&self) -> PathBuf {
        self.dir.join("python")
    }
}

impl Drop for Synthetic {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A module with each of unittest's outcomes, a test with failing subtests, a class whose
/// setUpClass fails, and a test that checks the environment it runs in and prints.
const OUTCOMES: &str = r#"
import os, sys, unittest

HERE = os.path.dirname(os.path.abspath(__file__))

class A(unittest.TestCase):
    def test_a_pass(self):
        pass

    def test_b_fail(self):
        self.assertEqual("left", "right")

    def test_c_error(self):
        raise KeyError("k\u00e9y")

    @unittest.skip("not today")
    def test_d_skip(self):
        pass

    @unittest.expectedFailure
    def test_e_expected_failure(self):
        self.assertTrue(False, "as expected")

    @unittest.expectedFailure
    def test_f_unexpected_success(self):
        pass

    def test_g_subtests(self):
        for i in range(4):
            with self.subTest(i=i):
                if i == 1:
                    self.fail("subtest one")
                if i == 3:
                    raise ValueError("subtest three")

    def test_h_environment(self):
        print("printed to stdout")
        sys.stdout.flush()
        os.write(1, b"written to file descriptor 1\n")
        self.assertTrue(os.path.samefile(os.environ["TEST_DATAFILES_DIR"],
                                         os.path.join(os.path.dirname(HERE), "data", "files")))
        self.assertTrue(os.path.samefile(sys.path[0], HERE))
        self.assertEqual((sys.flags.isolated, sys.flags.dont_write_bytecode, sys.flags.utf8_mode),
                         (1, 1, 1))
        self.assertEqual(os.listdir("."), [])
        with open("left-behind.txt", "w") as f:
            f.write("x")
        # Only what the command sets, and what the interpreter adds itself: uv's launcher on
        # Windows, LC_CTYPE on Linux, where Python coerces the C locale (PEP 538).
        expected = {"TEMP", "TMP", "TMPDIR", "TEST_DATAFILES_DIR"}
        if os.name == "nt":
            expected |= {"SYSTEMROOT"}
            added = {"PYTHONHOME", "UV_INTERNAL__PYTHONHOME"}
        else:
            added = {"LC_CTYPE"}
        self.assertEqual(set(os.environ) - added, expected)
        import tempfile
        self.assertTrue(os.path.samefile(tempfile.gettempdir(), os.environ["TMPDIR"]))
        self.assertFalse(os.path.samefile(tempfile.gettempdir(), "."))
        tempfile.NamedTemporaryFile(delete=False).close()
        import SyntheticHelper
        self.assertEqual(SyntheticHelper.VALUE, 7)

class B(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        raise RuntimeError("no class today")

    def test_never(self):
        pass
"#;

/// Each outcome is reported in run order, with its message; failing subtests once each; a
/// class whose setUpClass fails as unittest names it. The tests run with the data directory
/// and the module path upstream's driver sets, isolated, without bytecode, in UTF-8 mode, in
/// an empty directory, with a temporary directory of their own and nothing else of the
/// caller's environment; what they print doesn't reach the report.
#[test]
fn reports_each_outcome_of_a_synthetic_module() {
    let synthetic = Synthetic::new(&[
        ("SyntheticTest", OUTCOMES),
        ("SyntheticHelper", "VALUE = 7\n"),
    ]);
    let report = synthetic.run("SyntheticTest");
    assert_eq!(report["module"], "SyntheticTest");
    assert!(report["import_error"].is_null(), "{report}");
    assert!(report["numpy"].is_string(), "{report}");
    let tests: Vec<(String, String, Option<String>)> = report["tests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["id"].as_str().unwrap().to_string(),
                t["outcome"].as_str().unwrap().to_string(),
                (!t["message"].is_null()).then(|| text(&t["message"])),
            )
        })
        .collect();
    let ids_outcomes: Vec<(&str, &str)> = tests
        .iter()
        .map(|(id, outcome, _)| (id.as_str(), outcome.as_str()))
        .collect();
    assert_eq!(
        ids_outcomes,
        [
            ("SyntheticTest.A.test_a_pass", "pass"),
            ("SyntheticTest.A.test_b_fail", "fail"),
            ("SyntheticTest.A.test_c_error", "error"),
            ("SyntheticTest.A.test_d_skip", "skip"),
            (
                "SyntheticTest.A.test_e_expected_failure",
                "expected_failure"
            ),
            (
                "SyntheticTest.A.test_f_unexpected_success",
                "unexpected_success"
            ),
            ("SyntheticTest.A.test_g_subtests", "fail"),
            ("SyntheticTest.A.test_g_subtests", "error"),
            ("SyntheticTest.A.test_h_environment", "pass"),
            ("setUpClass (SyntheticTest.B)", "error"),
        ],
        "{report}"
    );
    let message = |i: usize| tests[i].2.clone().unwrap_or_default();
    assert!(tests[0].2.is_none() && tests[5].2.is_none());
    for (i, needles) in [
        (1, ["AssertionError", "'left' != 'right'"]),
        (2, ["KeyError", "'k\u{e9}y'"]),
        (3, ["not today", "not today"]),
        (4, ["AssertionError", "as expected"]),
        (6, ["AssertionError", "subtest one"]),
        (7, ["ValueError", "subtest three"]),
        (9, ["RuntimeError", "no class today"]),
    ] {
        for needle in needles {
            assert!(message(i).contains(needle), "{i}: {:?}", message(i));
        }
        assert!(
            !message(i).contains("Traceback") && !message(i).contains("SyntheticTest.py"),
            "{i}: {:?}",
            message(i)
        );
    }
    let left: Vec<_> = std::fs::read_dir(synthetic.python_dir())
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(left.len(), 2, "{left:?}");
}

/// Each run says where its method is defined: in its own class, in a base class of another
/// file (a test a class inherits), in a nested class; a class whose setUpClass fails, which
/// is no test method, says null.
#[test]
fn reports_where_each_test_is_defined() {
    let synthetic = Synthetic::new(&[
        (
            "DefinedTest",
            "import unittest\nfrom DefinedBase import Mixin\n\n\
             class C(Mixin, unittest.TestCase):\n    def test_own(self):\n        pass\n\n\
             class Outer:\n    class Inner(unittest.TestCase):\n        def test_nested(self):\n            pass\n\n\
             Inner = Outer.Inner\n\n\
             class D(unittest.TestCase):\n    @classmethod\n    def setUpClass(cls):\n        raise RuntimeError('no')\n\n    def test_never(self):\n        pass\n",
        ),
        (
            "DefinedBase",
            "class Mixin:\n    def test_inherited(self):\n        pass\n",
        ),
    ]);
    let report = synthetic.run("DefinedTest");
    let runs: Vec<(String, Value)> = report["tests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| (t["id"].as_str().unwrap().to_string(), t["defined"].clone()))
        .collect();
    assert_eq!(
        runs,
        [
            (
                "DefinedTest.C.test_inherited".to_string(),
                json!("DefinedBase.py::Mixin.test_inherited")
            ),
            (
                "DefinedTest.C.test_own".to_string(),
                json!("DefinedTest.py::C.test_own")
            ),
            ("setUpClass (DefinedTest.D)".to_string(), Value::Null),
            (
                "DefinedTest.Outer.Inner.test_nested".to_string(),
                json!("DefinedTest.py::Outer.Inner.test_nested")
            ),
        ],
        "{report}"
    );
}

/// A module that fails to import is reported with its exception; one whose process ends is
/// reported as a crash, with its exit status, even when the runner wrote its report first (a
/// handler that runs at exit).
#[test]
fn reports_an_import_error_and_a_crash() {
    let synthetic = Synthetic::new(&[
        ("BrokenTest", "raise ImportError('missing piece')\n"),
        ("CrashTest", "import os\nos._exit(3)\n"),
        ("LateCrashTest", LATE_CRASH),
    ]);
    let broken = synthetic.run("BrokenTest");
    assert_eq!(broken["import_error"]["type"], "ImportError", "{broken}");
    let message = text(&broken["import_error"]["message"]);
    assert!(message.contains("missing piece"), "{message:?}");
    assert_eq!(broken["tests"], json!([]));

    let crashed = synthetic.run("CrashTest");
    assert_eq!(crashed["crashed"]["returncode"], 3, "{crashed}");

    let late = synthetic.run("LateCrashTest");
    assert_eq!(late["crashed"]["returncode"], 9, "{late}");
}

/// A module whose test passes, and whose process then ends with status 9, after the runner
/// has written its report.
const LATE_CRASH: &str = r#"
import atexit, os, unittest

atexit.register(os._exit, 9)

class T(unittest.TestCase):
    def test_passes(self):
        pass
"#;

/// Requests it can't read are refused: missing or extra arguments, arguments of the wrong
/// type, and modules that aren't a file of the tests' python directory.
#[test]
fn bad_requests_are_refused() {
    let synthetic = Synthetic::new(&[("RefusedTest", "")]);
    let tests = synthetic.dir.to_string_lossy().into_owned();
    let nested = Path::new("python").join("RefusedTest");
    for args in [
        json!({}),
        json!({"tests": tests}),
        json!({"module": "RefusedTest"}),
        json!({"tests": tests, "module": "RefusedTest", "extra": 1}),
        json!({"tests": 1, "module": "RefusedTest"}),
        json!({"tests": tests, "module": ["RefusedTest"]}),
        json!({"tests": tests, "module": "NoSuchTest"}),
        json!({"tests": tests, "module": "RefusedTest.py"}),
        json!({"tests": tests, "module": nested.to_string_lossy()}),
    ] {
        assert!(
            Oracle::get()
                .try_call("py_suite", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
    assert!(synthetic.run("RefusedTest")["tests"] == json!([]));
}
