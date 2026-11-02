# P94: register the verified legacy-update child process

PR104 is rebased onto a23e0aed, retaining main's real-config sweep, GPU compile
checks and release paperwork. The failing environment inventory omitted P87's
legacy_gpu_updates.py from STARTS_PROCESSES even though its subprocess.run already
supplies the allowlisted env explicitly.

Owned files: crates/ocio-testkit/tests/oracle_env_oracle.rs, README.md and this card.
Add only that module to the sorted exact process-starter inventory. Keep all
existing rejection checks for missing env, inherited os.environ, None and other
process-launch methods. No oracle source, environment allowlist or expected
output changes. The independently verified oracle source remains unchanged.

Also reconcile the M5 table's stale legacy/library TODO text with P88/P89's
qualified live updates and source accounting. Retain main's corpus and GPU
compile-check statements. Complete SDE and release checks remain pending; no
full matrix runs until explicit owner confirmation on the release/tagged commit.

Local validation: both existing oracle environment tests pass, including the
actual allowlisted interpreter/child environment check. Targeted clippy with
warnings denied and workspace formatting pass under the existing 1.5GiB cap.
