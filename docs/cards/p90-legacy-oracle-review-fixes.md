# P90: resolve the independent P87 oracle review

The owner-authorized independent verifier reviewed frozen7441063d and found two
MEDIUM issues, no HIGH: primary/tone donors changed only one field, allowing partial
copy mutants to survive; failed results bypassed completed-prefix cardinality.
Evidence is kept/continuation/2026-11-01/p87-verifier/REVIEW.txt and its raw logs.

Change the donor inputs to exercise primary exposure/offset/pivot/clamps and tone
blacks/whites as well as the existing scalar fields. The same complete direct-wheel
comparison now covers those fields. Add40 IEEE-special, mixed-source, empty-history
and late-failure comparisons. A failed native result must be at the update stage
with1..N completed states (initial included); success still requires N+1. Add three
malformed-failure controls,12 total. Real late-failure prefixes remain accepted.
Serialize the two Rust contracts with a private test mutex to bound native memory
and overlapping first-build work; every original assertion/case remains required.

Both integrated Rust oracle contracts pass under1.5GB on Windows. The same verifier
must recheck this forward commit and kill the two surviving native mutants before
the consumer lands. No SDK, fixture, waiver, tolerance, dependency or map closure.
