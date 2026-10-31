# P56: account for the remaining three C++ tests

P9-6 was explicitly decided by the owner on2026-10-23 (Phase9 card, Owner decisions).
Record its two precise not-applicable tests in the reviewed list: Platform's C++
allocator test and the Windows debug CRT harness test. Preserve its condition to
reopen the allocator case if Phase8 introduces applicable aligned storage.

Translate every assertion of NamedTransform/basic (tests/cpu/NamedTransform_tests.cpp:
12-62@v2.5.2) through the existing production Python binding. Its NamedTransform,
MatrixTransform, clone identity, static GetTransform, stream text and Config errors
already exist; Config.addNamedTransform(None) reaches the live production Rust
binding's null-argument branch. The expected stream text and exception fragments
come directly from the pinned C++ test. No new API or unpublished helper is needed.
The prior direct Rust checks remain and the full test carries one C++ marker.

Expected accounting after full validation:1189 ported plus2 reviewed not-applicable
out of1191 C++ tests; GPU264 and Python384 unchanged. The native C++ test-runner CLI
file stays partial rather than claiming its remaining infrastructure was ported.
Only tests, map/register bookkeeping and documentation change. All original output
waivers, thresholds, fixtures, oracle and production code remain unchanged.
