# P73: qualify the original Python suite driver

Own xtask/src/py.rs, py_driver.rs, its module declaration and the driver's source-map entry.
Run the pinned, unchanged tests/python/OpenColorIOTestSuite.py after the existing per-module
qualification. Both the official wheel and the port execute its no-argument single-process
suite, its --run_only ConstantsTest selection and its unknown-module diagnostic.

The wheel's existing py_suite inventory supplies every expected test ID and multiplicity.
The complete driver's successful exit, exact inventory and unittest OK footer must agree;
the port also preserves the wheel driver's execution order. Selection must contain exactly
the wheel-inventoried ConstantsTest runs. Unknown-module exit status and diagnostic bytes
must match. No test source, expectation, threshold, exclusion or oracle command changes.

Each process gets a separate cwd/temp directory and a cleared environment. The CLI's empty
build-directory shape supplies its normal search paths without installing another package;
ConstantsTest needs no data. The full driver reads the original upstream data directory.
Keep each actual stdout/stderr/returncode in target/py-driver.json, including failures.

Native reporting controls reject an incomplete count, missing/duplicated/wrong IDs and a
skipped-test footer. These are harness controls; both-platform original-driver runtime
qualification remains required in normal CI before merging. No SDE invocation is needed.
