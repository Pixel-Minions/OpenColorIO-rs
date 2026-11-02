# P88: qualify Processor's legacy dynamic updates

Close the remaining qualification gap in Processor.cpp and Processor.h without
changing their existing implementation. Processor.cpp:447-523 @ v2.5.2 creates the
legacy processor through the shared flags-only cache; OpenColorIO.h:2707-2709 and
3476-3483 expose the legacy entry point and dynamic properties. P87's separate
native reference chunk and P90's independent-review corrections precede this
consumer. The verifier approved 54c9311a after 180 direct-wheel comparisons,
12 rejection controls and private native/Python mutation tests; the evidence is
kept/continuation/2026-11-01/p87-verifier/RECHECK-54c9311a.txt.

Owned files: crates/ocio/tests/legacy_gpu_live_oracle.rs, its private
legacy_gpu_live/values.rs helper, .cargo/config.toml, the two Processor rows of
upstream-map.toml and this card. No SDK, fixture, waiver or tolerance changes.

The live oracle compares 5,880 requests: seven property types, three positions
before/inside/after the legacy LUT region, edges 2 and 3, seven named optimization
levels, ten shader languages, and both cache orders (fresh legacy versus an
ordinary GPU request first). The lattice input follows NoOps_tests.cpp:32-52.
Each successful history observes the initial descriptor, a source-processor
setter, a descriptor setter, then resets through both routes. It rereads the same
descriptor after each update. Missing-property and initial-extraction refusals
must match stage, type, exact message, log and completed prefix.

Every snapshot compares the complete shader text, texture bits, uniforms and live
getters, dynamic-property values, source sharing, caches and GPU metadata. Values
are typed/bit-exact through the existing gpu::Shader and Dumped representations;
all expected outputs come from the verified official-library client. Batches are
live, with no response-cache acceptance. The ordinary-cache-first/NONE witness
must complete all four setters for every property type.

Twenty-eight libtest parts cover the original plan exactly once (210 requests
each), retaining bounded fresh-process execution in the existing SDE inventory.
The plan audit proves the complete index union. The cpu-tests alias adds the new
target while retaining every previous package and target. No SDE qualification
pass is inferred from this registration or from a native pass.

Existing processor_oracle, processor_cache_oracle, processor_gpu_oracle,
dynamic_properties_oracle, legacy_gpu_oracle and family sweeps retain their
coverage of every other Processor API. The private setColorSpaceConversion has
no caller in upstream 2.5.2, as already recorded by Phase 9; this adds no exclusion.
Both-platform debug/release normal CI on the regenerated land head remains the
merge requirement. Complete SDE qualification and release acceptance remain open;
the owner's explicit confirmation is required before the final full matrix.

Local Windows validation: all 28 parts plus the original-plan union audit pass
(29 tests, 5,880 live requests, 349.74s), under the existing 1.5GiB cap with one
test thread. The separate seven-type witness also passes. Targeted clippy with
warnings denied, workspace formatting, source guards, 72 fixture hashes and the
unchanged upstream-test baseline ratchet pass. These local results do not replace
the required exact-head normal CI in both profiles and reference platforms.
