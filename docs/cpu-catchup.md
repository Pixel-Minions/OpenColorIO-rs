# Deferred Config-backed CPU coverage

The four tests in `tests/cpu/ColorSpaceSet_tests.cpp` @ v2.5.2 were deferred until
Config's color-space APIs existed. They now run in
`crates/ocio/tests/color_space_set_tests.rs`: basic(lines12-100),
decoupled_sets(102-155), order_validation(157-203), and
operations_on_set(205-316). Every original category/filter request, count, name,
copy-lifetime and set-order assertion is preserved. Existing set-only extension
tests remain unchanged. The established Rust API represents the C++ null/empty
category with empty bytes; no API shape or deviation is introduced.

[CI40959291599](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/40959291599)
passed all four source cases, target clippy and workspace guardrails on Windows
and Rocky Linux9, in debug and release, at runtime sourcef2a5c578. Each job
regenerated parity/ratchet and passed main mode; the resulting files are
byte-identical across all four jobs. The [verified summary](cpu-color-space-set-validation.json)
records their source identities and hashes. Proposed coverage is1183/1191C++
and205/264GPU. This test-only change introduces no new expected values, oracle
command, runtime behavior, dependency or waiver.

P24 completes both original `Exception` tests (`basic`, `missing_file`) through the
existing Rust `Exception`, `ExceptionKind`, `std::error::Error`, and `Clone` APIs.
The source values and observations come from `tests/cpu/Exception_tests.cpp` at
v2.5.2. This changes test coverage only.

Focused CI [40961400013](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/40961400013)
passed Windows and Rocky Linux 9 in both debug and release, including guardrails,
target clippy and generated-file main mode. All four saved artifacts identify
runtime source `1e0a581b659ed1c5a29e50d32196ff9e7c4a8085` and validation workflow
`27548da7f6dbc97c6344d146eec3be04557c102c`. Independent verification found identical
generated files across all four profiles; see `cpu-exception-validation.json`.
The proposed cumulative coverage is 1,185/1,191 C++ tests and 205/264 GPU tests.
Normal landing CI remains required before merging this chunk.
