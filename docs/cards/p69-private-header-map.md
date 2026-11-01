# P69: existing parse, range and file-loader mappings

Six stale partial entries, no runtime changes:

- ParseUtils.h: existing ocio-ops parsing/numeric/XML/vector functions and ocio's
  stream nextline cover the private declarations. Public GPU-language parsing is
  declared in OpenColorTypes.h and accounted for by P66.
- RangeOpData.cpp/.h: the supposed missing CTF IndexMapping constructor is already
  range_from_index_mapping in ctf_reader_helper.rs:3031; add that existing target.
- CDLTransform.h: class/metadata behavior exists, with map/vector/shared handles in
  the existing CDL reader modules; include their targets.
- FileTransform.cpp/.h: model/format/default/CDL/cache/loading/context/build APIs
  through upstream FileTransform.cpp:970 are implemented in the existing transform,
  format and CDL-reader modules.

Local source/map/whitespace checks pass. The count becomes656done+228reviewed=884/913,
leaving23partial+6todo. Original test counts, comparisons and SDE debt are unchanged.
Normal CI remains required on the final regenerated candidate before merging.
