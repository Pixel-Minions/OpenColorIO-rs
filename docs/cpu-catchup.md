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

P25/P26 finish two previously partial upstream cases using existing APIs:
`NamedTransform/alias` now includes the original Config copy, canonical-name,
replacement and collision checks; `FileTransform/context_variables` now includes
the original ExposureContrast processor build and complete CCC-ID case 4.
Original values, YAML, variable order and assertions are preserved. The unrelated
`NamedTransform/basic` null-argument case remains uncounted.

[Alias CI40964313761](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/40964313761)
passed all ten named-transform module tests on Windows/Rocky debug/release.
[Context CI40965288458](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/40965288458)
executed and passed the complete four-case context test on all four profiles.
Independent artifact verification checked both exact runtime/workflow identities,
actual required test names, guardrails, target clippy, main-mode generation and
identical generated files within each component's four profiles. See
`cpu-alias-context-validation.json` for the eight profile records.

The source-only replay onto the merged P24 parent has identical crate, map,
oracle, script, dependency and workflow files to validated context runtime
`ee787da8b4a1c2775cd68ae68219862d4d3dba4a`; only landing bookkeeping differs.
Proposed cumulative coverage is 1,187/1,191 C++ and 205/264 GPU tests. Full normal
CI on the exact combined landing remains required before merging.

PR82's first normal Windows check failed before the Rust comparison: the official
wheel's last reader deque was empty despite zero reader errors. Starting a thread
does not guarantee its first read precedes the main thread's stop signal.
The existing concurrency test now signals an Event only after getValues and the
sample selection succeed, waits for every reader in every round, and checks each
wait succeeded. It retains all 200 rounds, five readers, ten extractions per
round, zero-reader-error checks, exact final samples, the port's time limit and
the same script/report comparison on both sides. Production binding code and the
oracle are unchanged.

[Startup CI41004232105](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/41004232105)
passed five complete repetitions on Windows and Rocky Linux 9 in both debug and
release (20 repetitions total). Independent artifact review verified all source
identities, actual test executions, unchanged stress counts, target Python-feature
clippy, guardrails/main mode and identical generated files across all four
profiles; see `texture-reader-startup-validation.json`. Full normal CI on the
updated exact landing remains required. The prior failed candidate is preserved
in its history; no checks or tolerances were relaxed.

P27 completes the original `Builtins/read_write` test. It builds the original
2.4 YAML config with every registry style in its original order, validates the
config, checks serialization byte for byte, and creates the `ref` to `test`
processor. It uses existing APIs and copies the original YAML/assertions; no
production code, dependency, API, oracle, waiver or comparison changes.

[CI41070682176](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/41070682176)
passed the actual original case on Windows and Rocky Linux 9 debug/release,
including guards, target clippy and main-mode generation. Independent artifact
review checked all four source/workflow identities and identical generated files;
see `cpu-builtin-registry-validation.json`. Proposed C++ coverage is 1,188/1,191;
GPU remains 205/264. Full exact landing CI is required before merging.

This CPU test completion does not change the 59 held fixed-function GPU cases.
The separate ACES2 investigation remains with the owner's debugger; its checkout,
branch and the frozen prior GPU evidence were not accessed or modified.
