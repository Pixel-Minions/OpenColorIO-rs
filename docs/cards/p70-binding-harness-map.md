# P70: qualified binding and GPU-harness source accounting

Nine stale entries; no test, comparison, renderer or binding behavior changes.

- PyOpenColorIO.cpp/.h: module initialization registers every binder/global and
  complete metadata. Keep lib.rs/registration/polymorphic dispatch targets and D-16.
- PyTransform.cpp/.h: base and all23 concrete per-transform binding modules exist;
  add their existing targets instead of retaining the early-subset partial flag.
- UnitTestOptimFlags.cpp/.h: the existing test_env::EnvGuard thread overlay and
  processor_tests.rs checks supply the scoped optimization-environment behavior.
  The process environment stays intact and Drop restores the default provider
  under D-5. The proposed separate helper file was stale; no test is dropped.
- GPUUnitTest.cpp/.h: input/defaults/comparison/run/retests, complete wheel-GPU image
  identity, native readback regression and rejection controls are implemented and
  qualified. Phase7 card7.2c already excludes the standalone main registry/CLI.
- MatrixOp_test.cpp: all10 original cases, including8legacy, already run with the
  original settings/thresholds and wheel image identity; native readback and the
  separate ten-language text checks remain intact.

The existing normal suite runs384 original Python functions and all264 original GPU
cases on the approved Rocky llvmpipe renderer. Preserve the Phase7 platform boundary,
all approved exact pins/class checks and D-16; no new exclusion or waiver is added.

The count becomes665done+228reviewed=893/913, leaving16partial+4todo. Source/map checks
pass locally; normal CI is still required before merging. Complete CPU-profile and
post-extraction legacy qualification remain open in P63.
