# P58: reconcile builder and YAML entry-point declarations

OpBuilders.h declares29builder overloads (25distinctnames). Each has an existing
production Rust function with its source citation. The repeated names are
ColorSpace(two overloads), Look(two), and Log(three sharing LogOpData/build_log_op).
All per-family builders are included in the map targets.

OCIOYaml.h declares only Read and Write. Both already exist as read/write in
ocio_yaml.rs, are called by Config, and have the existing YAML oracle suites.

This source audit updates exactly those two map entries and this card. No function,
API, production code, test, fixture, oracle, dependency, waiver, SIMD or tool changes.
The repository tool regenerates bookkeeping only at integration. Normal CI still
validates the final merge head; source preparation is not a runtime completion claim.
