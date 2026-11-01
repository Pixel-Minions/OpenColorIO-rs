# P71: finish the former Phase8 source-map dependency

Six source-accounting closures using merged main4bc8b1f6, without renderer changes:

- CPUProcessor.cpp: typed conversion ends, CPU engine, scanline/image/RGB/RGBA apply,
  cache/dynamic queries and physical renderer dispatch are present in cpu_processor.rs.
- Op.cpp/.h: CpuOp defaults and typed/in-place/dynamic interfaces, all family arms,
  op/data/vector behavior and CPU/GPU/processor-metadata hand-off are implemented.
- Platform.cpp/.h: keep the existing P9-6 decision for AlignedMalloc/AlignedFree.
  Lut3D still has Vec<f32> storage (lut3d_op_cpu.rs:115); its SSE2/AVX paths use
  unaligned loads (lut3d_op_cpu_sse2.rs:241 and lut3d_op_cpu_avx.rs:222). The storage
  condition has not reopened the C++ allocation helper. Other platform functions exist.
- Lut1DOpCPU.h: get_lut1d_renderer and get_lut1d_renderer_for_cpu implement the
  declared factory across supported bit depths and dispatched physical kernels.

Source completion is separate from complete CPU-profile qualification. All ten
profiles, the execution findings and post-extraction legacy updates remain in P63.
No new exclusion, waiver, unsafe code or numerical change is introduced.

## The retained60-file audit after this series

P65/P67-P71 correct44 already implemented source mappings in small commits. P66 adds
the missing public GPU-language conversion and resolves two additional source entries.
The final proposed map has671done+228reviewed=899/913, leaving10partial+4todo:

| Open files | Actual work still retained |
|---|---|
| OpenColorIO.h, Processor.cpp/.h | legacy post-extraction dynamic qualification and the complete CPU-profile proof |
| Mutex.h | explicit std::sync lock/debug-diagnostic accounting |
| package/command_line.py | package executable lookup/argv/exit launcher |
| UnitTestLogUtils.cpp/.h | account for or complete the shared log-capture support |
| tests/cpu/UnitTestMain.cpp | native runner/CPU-override CLI replacement accounting |
| UnitTestUtils.cpp/.h | remaining environment/temp-directory helper mapping |
| OpenColorIOTestSuite.py | standalone original driver/CLI qualification |
| tests/testutils/UnitTest.cpp/.h | explicit libtest registry/reporting/macro replacement accounting |
| tests/utils/UnitTestMain.cpp | native banner/delegation replacement accounting |

No additional missing OCIO_ADD_TEST case is claimed:1189ported+2previously reviewed
still account for all1191. All264 GPU markers and384 original Python functions remain.
Source-map counts include bookkeeping, and do not raise an implementation percentage
or assert a complete SDE profile. The grouped exact-head normal gate, including the
new parser's live-wheel test and original Python suite, is still required before merging.
