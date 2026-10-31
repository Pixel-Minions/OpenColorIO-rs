# P57: reconcile three implemented private headers

Audit each declaration against the pinned2.5.2 source and existing Rust users.

| Upstream header | Existing implementation |
|---|---|
| TransformBuilder.h | Its only CreateTransform declaration maps to transform::create_transform in crates/ocio/src/transform.rs. The previous target file did not exist. |
| ops/allocation/AllocationOp.h | AllocationData/default/cache ID/Display, CreateGpuAllocationNoOp and CreateAllocationOps already exist in allocation_op.rs/no_ops.rs and have their original tests and live oracle coverage. |
| PrivateTypes.h | Four used private container aliases are represented in Config/Processor by BTreeMap, BTreeSet, Vec<Arc<Look>> and Vec<&Transform>. The other two aliases are unused throughout pinned upstream. |

Every declaration is accounted for. No new public or private alias needs to be
introduced for an unused C++ typedef. This changes exactly three map entries and
this card; the repository tool regenerates the dashboard at integration.
No production source, test, comparison, fixture, oracle, dependency or waiver changes.
No SIMD, app or apputils work is included. No SDE-matching changed path.

The README's test table also retained205 GPU tests after all264 were merged. Correct
that table, show the inherited P56 C++ accounting as1189ported+2reviewedN/A, and
remove the stale remaining-GPU-execution/null-review text. Overall effort stays84%;
these accounting corrections do not imply additional implementation work.
