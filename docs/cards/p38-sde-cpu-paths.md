# P38: trigger SDE for every CPU-test library package

The CPU-dependent Cargo alias selects the ocio, ocio-ops and ocio-gpu library packages,
but the SDE pull-request path filter included only ocio-ops. Add ocio and ocio-gpu paths
so their implementation/test changes trigger the existing emulated-CPU matrix.

Own this card and `.github/workflows/sde.yml`. Change only the two trigger patterns;
preserve every existing path, job, CPU/platform row, timeout, tier, filter and runtime
command. No test coverage or comparison changes. Normal diagnostic ci.yml stays excluded.

Statically verify all three alias packages have trigger coverage, the former test-only
legacy follow-on path is now selected, and the rest of the workflow is byte-identical.
The final follow-on PR must register the existing complete SDE matrix on its exact head.

Keep PR85/86 frozen and all owner GPU restrictions/protected paths unchanged.
