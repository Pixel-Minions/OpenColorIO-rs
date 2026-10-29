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
