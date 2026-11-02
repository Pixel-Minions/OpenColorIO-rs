# P81: approved Rust-native private test support

The owner explicitly approved the seven-file boundary proposed in
kept/continuation/2026-11-01/OWNER-NATIVE-TEST-SUPPORT.md on 2026-11-01.
D-19 records the decision. This metadata-only chunk closes seven support rows
against existing Rust equivalents; it introduces no SDK implementation or test
exclusion. In particular, it does not claim to implement C++ debug recursive-lock
diagnostics. Valid locking and public callback/reentrancy/error behavior remain
in scope, as do every original OCIO case and all comparisons and tolerances.

| Upstream file | Existing equivalent and evidence |
|---|---|
| src/OpenColorIO/Mutex.h | caching.rs, context.rs, processor.rs and ocio-ops/logging.rs use std Mutex/MutexGuard for valid scoped locking. P79 records the separate debug diagnostic gap; D-19 now resolves that private-support boundary. Upstream try_lock has no callers outside its definition. |
| tests/cpu/UnitTestMain.cpp | Cargo/libtest, xtask/shards.rs and actual SDE profiles drive tests and CPU selection. The existing UnitTest/windows_debug P9-6 N/A is unchanged. |
| tests/testutils/UnitTest.cpp | ocio_testkit::upstream documents the existing first-failure adaptation and implements check_str_float_vec_close; libtest supplies registration and execution. |
| tests/testutils/UnitTest.h | Existing upstream assert/throw/float helpers and Rust #[test] keep the assertions; C++ macro/static registration and diagnostics are private machinery. |
| tests/utils/UnitTestMain.cpp | Cargo/libtest and xtask shards replace the banner and delegation only. |
| tests/cpu/UnitTestUtils.cpp | fileformats/test_utils.rs supplies GetTestFilesDir, LoadTestFile and BuildOpsTest; file_transform_tests.rs and file_format_ctf_tests.rs supply file-processor helpers; ocioz_archive_tests.rs uses per-test directories with original filesystem assertions. |
| tests/cpu/UnitTestUtils.h | upstream helpers, the same file helpers and test_env::EnvGuard (existing D-5); typed/scoped Rust inputs replace C++ templates/U8 casts and shared directories. |

No N/A row, waiver, test count, threshold, oracle, fixture, renderer, dependency,
unsafe or public API changes. All seven target sets point to existing files.
The candidate map has 682 done and 228 reviewed N/A rows out of 913, with only
the public header and Processor.cpp/.h open for legacy post-extraction dynamic
qualification. This is source accounting, not completion of CPU qualification.
P80's functional log helpers and P78's package launcher retain their own actual
runtime requirements in normal Windows/Rocky debug/release CI before landing.
