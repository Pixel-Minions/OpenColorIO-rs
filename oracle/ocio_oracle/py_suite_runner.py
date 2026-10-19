# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Runs one module of upstream's Python suite (tests/python) and prints each test's outcome as
JSON. A plain script: it imports nothing of the oracle, so the port's environment runs the
same file against the port's module (card p6-skeleton, 6.1b), and the two runs compare.

usage: python -I -B -X utf8 py_suite_runner.py <tests/python dir> <module> <tests/data/files dir>

It sets TEST_DATAFILES_DIR and puts the tests' directory first on sys.path, as upstream's
OpenColorIOTestSuite.py does when it runs without arguments, then loads the module's tests
with unittest's loader and runs them in the loader's order. The tests are not modified.

Output, one JSON object on stdout, which nothing else writes to (the tests' own output, and
the library's, goes to stderr):
  {"module": name, "numpy": its version or null (without NumPy, several tests pass without
   checking anything), "import_error": null or {"type", "message"},
   "tests": [{"id": unittest id, "outcome": "pass" | "fail" | "error" | "skip"
              | "expected_failure" | "unexpected_success", "message": text or null}]}
A message is the exception's type and text (traceback.format_exception_only, without the
traceback and its paths); a skip's message is its reason. -B keeps the tests' bytecode out of
upstream's checkout.
"""

import json
import os
import sys
import traceback
import unittest


def _message(err):
    exc_type, exc, _tb = err
    return "".join(traceback.format_exception_only(exc_type, exc))


class _Result(unittest.TestResult):
    def __init__(self):
        super().__init__()
        self.outcomes = []

    def _add(self, test, outcome, message):
        self.outcomes.append({"id": test.id(), "outcome": outcome, "message": message})

    def addSuccess(self, test):
        self._add(test, "pass", None)

    def addFailure(self, test, err):
        self._add(test, "fail", _message(err))

    def addError(self, test, err):
        self._add(test, "error", _message(err))

    def addSkip(self, test, reason):
        self._add(test, "skip", reason)

    def addExpectedFailure(self, test, err):
        self._add(test, "expected_failure", _message(err))

    def addUnexpectedSuccess(self, test):
        self._add(test, "unexpected_success", None)

    def addSubTest(self, test, subtest, err):
        # A failing subtest fails its test, as unittest's own runner reports it.
        if err is not None:
            outcome = "fail" if issubclass(err[0], test.failureException) else "error"
            self._add(test, outcome, _message(err))


def _numpy_version():
    try:
        import numpy
    except ImportError:
        return None
    return numpy.__version__


def main():
    tests_dir, module_name, data_dir = sys.argv[1:4]
    # Only this script writes to stdout: the process's stdout (file descriptor 1, which the
    # library would write to as well) becomes stderr until the report.
    sys.stdout.flush()
    report = os.fdopen(os.dup(1), "w", encoding="utf-8")
    os.dup2(2, 1)
    sys.stdout = sys.stderr
    os.environ["TEST_DATAFILES_DIR"] = data_dir
    sys.path.insert(0, tests_dir)
    out = {"module": module_name, "numpy": _numpy_version(), "import_error": None, "tests": []}
    try:
        module = __import__(module_name)
    except BaseException as exc:  # noqa: BLE001 - an import that fails fails the module
        out["import_error"] = {"type": type(exc).__qualname__,
                               "message": "".join(traceback.format_exception_only(type(exc), exc))}
    else:
        suite = unittest.defaultTestLoader.loadTestsFromModule(module)
        result = _Result()
        suite.run(result)
        # In run order; a test with failing subtests appears once per failing subtest, and a
        # class whose setUpClass fails appears as unittest names it ("setUpClass (...)").
        out["tests"] = result.outcomes
    report.write(json.dumps(out))
    report.close()


if __name__ == "__main__":
    main()
