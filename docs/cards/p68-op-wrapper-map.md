# P68: existing op wrapper, factory and data mappings

Ten source-map corrections from the retained60-file audit; no runtime changes.

| Entry | Existing implementation or precise boundary |
|---|---|
| OpArray.h | ArrayBase is represented by typed arrays/CTF dispatch; value indexing, resize, set and validation exist. ColorMatrixHelpers indexes the typed values directly; setMaxColorComponents has no upstream caller. |
| CDLOp.cpp / CDLOp.h | op/data/factory behavior and CDL transform reconstruction exist. The value overload taking FormatMetadataImpl is declared but has no C++ definition; cdl_op.rs documents the actual signature. |
| ExponentOp.h | factories and existing exponent transform reconstruction; add the transform-side target. |
| GammaOp.cpp | wrapper/data/factories plus Exponent/ExponentWithLinear reconstruction and GPU extraction. |
| GradingBSplineCurve.h | spline/control-point/KnotsCoefs types and forward/reverse/hue/fit behavior; stale note predates hue completion. |
| LogOp.cpp | wrapper/data, all three factory overloads, reconstruction/build and GPU extraction. |
| Lut1DOp.cpp | wrapper, composition, identity/linear-scale factories, reconstruction/build and GPU extraction. |
| Lut3DOpData.h | Compose and MakeFastLut3DFromInverse already exist in lut3d_op_data.rs. |
| MatrixOp.h | all defined factories, including the one-argument CreateIdentityMatrixOp, exist. Only float-pointer CreateFitOp/CreateMatrixOffsetOp lack upstream definitions; add the existing matrix transform target. |

Renderer translation is accounted for in the separately mapped CPU files. Complete
SDE qualification remains open in P63; these closures assert source mappings only.
Add missing existing transform targets and replace obsolete incremental notes.
The source count becomes650done+228reviewed=878/913, leaving29partial+6todo.
Local source/map/whitespace checks pass; normal CI remains required before merging.
