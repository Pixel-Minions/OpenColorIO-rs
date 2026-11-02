# P84: parallel ICC input sweeps in required Linux CI

Owner request, 2026-11-01: the old Linux debug formats job took221 minutes,
including170 in ociobakelut_icc_oracle (76 of115 release minutes). Split that
work without changing any input, comparison, tier or expected output; aim for
Linux jobs below about two hours. Bundle this chunk with the next source group,
not a separate CI round.

Windows retains its complete existing ICC test execution in the formats job.
Linux moves ICC into four parallel input partitions per profile, debug AND
release. Each partition builds both original complete input sequences before
selecting indices congruent to its part modulo4. The original RNG seed and every
draw stay in place. Both wheel and port keep the original global case directory
index. Part0 also runs all other ICC cases once; parts1..3 select only the two
original large tests. Workspace feature unification, full tier, raw comparison
logic, creation-time checks, numerical controls and reviewed stop rules remain.

Local native input verification against the pre-refactor6e3f8a4b source proves all
576 generated and3000 corrupted inputs, labels and original order identical.
It uses the actual generators and file bytes, without executing the SDK or tools.
The original generated sweep still requires at least150 files,100 deaths, its
original <=total/50 stop limit, and every original diagnostic fragment (including
the existing too-long-curve conditional). Corrupted still requires count/8 files
and deaths, and <=count/50 stops. No threshold is divided or weakened per part.

Each part emits a report only after common::sweep has passed every exact original
wheel/port comparison. Its original plan fingerprints every case/control and file
byte with SHA256, then records selected global indices and actual outcome facts.
P86 independently fingerprints the immutable cases actually passed into sweep;
the join requires these to equal the selected original input records, so declaring
the right indices while substituting different case bytes cannot satisfy the audit.
Separate REQUIRED debug/release joins regenerate the original full input plans,
require four matching source/profile/full-tier reports per sweep, verify all
input fingerprints and exactly one outcome per original index, and enforce the
ORIGINAL aggregate assertions on the full union. A failed/missing/cancelled part
or audit fails the required Linux check. The proof and reports are retained as CI
artifacts. Native controls reject missing/duplicate/foreign indices, changed
inputs, bad types, incomplete outcomes and source/profile/tier mismatches.

The sample workspace packages in xtask/src/shards.rs keep their existing indices;
ocio-tools remains last. No cpu-tests target, SDE selection/budget, oracle,
fixture, waiver, deviation or SDK source changes in this chunk. The separately
prepared P82 oracle extension is preserved outside target and excluded here.

With a balanced case cost, the170-minute ICC component becomes about43 minutes
per part, plus builds and the other ICC tests in part0. Formats loses that component;
the measured other jobs are about90 minutes or less. This is a scheduling estimate;
the next grouped normal run must establish the actual durations before landing.

Local validation: all102 xtask controls, including10 shard/workflow controls, and
xtask clippy with warnings denied pass. Actual full-tier input identity and native
audit rejection controls pass under1GB/1.5GB caps. The actual ICC test/audit modules
also pass isolated clippy with the existing workspace precision-lint policy.
Integrated Windows/Rocky debug/release normal CI remains
required on the final regenerated group SHA. No separate CI dispatch for this chunk.
