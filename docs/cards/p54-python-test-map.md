# P54: reconcile upstream Python test-file status

The Python suite already passes all 384 upstream test functions, but all 53 non-build
entries under `tests/python/` still said `todo`. Reconcile them against the pinned
upstream sources, `py-port/passing.txt`, and `xtask/src/py.rs`.

- The driver lists 48 test modules. Their 383 functions and the inherited
  `TransformsBaseTest.py` function account for all 384 recorded functions.
- `UnitTestUtils.py` is reused unmodified; `__init__.py` has no executable statements.
- The upstream NumPy requirement is supplied by the existing pinned oracle environment.
- The suite-driver entry remains **partial**: its inventory is consumed, but its
  standalone CLI and single-process path have not been separately validated.
- `CMakeLists.txt` keeps its existing build-system `n/a` status.

Result: 52 Python entries done, one partial, one existing build entry not applicable.
No test, module, assertion, dependency, fixture, oracle source, or passing-function list
changes. No new exclusion is introduced. All other map entries are untouched.

Full normal CI on PR89 (run 41150861803, head bad4104799) records 384/384 functions
passing on Windows and Rocky Linux 9. The local audit in
`kept/continuation/2026-10-31/p54-python-map-audit.json` pins both actual log hashes,
original source hashes and per-entry changes. Normal repository checks also run for
this chunk; the SDE workflow has no matching changed path.
