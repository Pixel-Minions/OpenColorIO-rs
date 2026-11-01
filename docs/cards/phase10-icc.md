# Phase 10: ICC output of `ociobakelut`

Port `src/apps/ociobakelut/ocioicc.cpp` and the part of `main.cpp` that calls it. The owner
decided on 2026-10-31 to translate the subset of lcms2 they reach to Rust, in full, including
`--displayicc`: no C crate, no new dependency.

The expected result is what the wheel's `ociobakelut` does on the same platform: file bytes,
stdout, stderr and exit code. `upstream/Little-CMS` is the source, pinned at the tag `lcms2.17`
(commit 51763476), the version OCIO 2.5.2 names in `RECOMMENDED_VERSION`. The wheel's bytes are
the reference, never lcms2's source by itself.

## Where it goes

- `crates/ocio-tools/src/lcms2/`: a tree that mirrors lcms2's `src/`, one Rust file for each C
  file that is reached. `types.rs` holds what those share of `lcms2.h`, `lcms2_plugin.h` and
  `lcms2_internal.h`. Function names are the C ones in snake case (`_cmsReadInputLUT` becomes
  `cms_read_input_lut`), and each cites its source as `src/cmsxxx.c:a-b @ lcms2.17`.
- `crates/ocio-tools/src/ocioicc.rs`: `ocioicc.cpp`.
- `upstream-map.toml` is OCIO's: it lists `ocioicc.cpp` and `ocioicc.h`. lcms2's files are
  tracked here.
- No public API: `ocio-tools` isn't published, and `lcms2` is a module of it because its tools
  and tests use it.

The context parameter is gone, as every call passes null: the global context, with no plug-ins.
Stages and pipelines own what they hold, in a `Vec`, so the `cms*Free` calls are gone, and
`Clone` is `cmsStageDup` and `cmsPipelineDup`: their only ways to fail are size limits the
original has passed.

## What is reached

The calls of `ocioicc.cpp` with what `main.cpp` passes, followed through lcms2. Evaluated with
what can be true at each point, not just with what the C compiler keeps.

The facts that cut the most:
- Both transforms take `TYPE_RGB_16` and `TYPE_LabV2_16`, so `IsProperColorSpace` turns away any
  profile that isn't RGB on the device side. A gray, CMYK or named color profile gets no further
  in the transform; the tool then dies in the sampler (U-652). `_cmsReadInputLUT` and
  `_cmsReadOutputLUT` only ever see RGB device spaces. `_cmsReadDevicelinkLUT` sees any.
- All transforms have `cmsFLAGS_NOOPTIMIZE|cmsFLAGS_NOCACHE`: `_cmsOptimizePipeline` ends after
  `PreOptimize`, and the worker is `PrecalculatedXFORM`, or `FloatXFORM` for `TYPE_Lab_DBL`.
- Black point compensation is forced only for v4 profiles in the perceptual intent.
  `cmsDetectDestinationBlackPoint` sees a v4 one or the abstract Lab one, so it ends at its first
  test: no Adobe algorithm, no round trip transform. `BlackPointUsingPerceptualBlack` needs CMYK.

| File | Reached | Left out, and why |
|---|---|---|
| `cmsxform.c` | `cmsCreateTransform`, `cmsCreateTransformTHR`, `cmsCreateMultiprofileTransformTHR`, `cmsCreateExtendedTransform`, `AllocEmptyTransform`, `GetXFormColorSpaces`, `IsProperColorSpace`, `cmsDoTransform`, `PrecalculatedXFORM`, `FloatXFORM` | cache, gamut, proofing, null transforms, plug-ins, extra channels |
| `cmscnvrt.c` | `_cmsLinkProfiles`, `DefaultICCintents`, `ComputeConversion`, `ComputeBlackPointCompensation`, `AddConversion`, `IsEmptyLayer`, `ColorSpaceIsCompatible` | absolute intent (`ComputeAbsoluteIntent`, `CHAD2Temp`, `Temp2CHAD`), black-preserving intents, `cmsFLAGS_NONEGATIVES` |
| `cmssamp.c` | `cmsDetectBlackPoint`, `cmsDetectDestinationBlackPoint` up to its v4 test, `BlackPointAsDarkerColorant` | `BlackPointUsingPerceptualBlack`, `CreateRoundtripXForm`, the curve fitting |
| `cmsio0.c` | null and file `cmsIOHANDLER`s, `cmsCreateProfilePlaceholder`, `_cmsReadHeader`, `_cmsWriteHeader`, tag search, `cmsOpenProfileFromFile`, `cmsSaveProfileToFile`, `SaveTags`, `cmsReadTag`, `cmsWriteTag`, `cmsLinkTag`, version get and set, `_cmsGetTagTrueType` | memory streams, raw tags, `SetLinks` (nothing saved is linked), stream handlers |
| `cmsio1.c` | `_cmsReadInputLUT`, `_cmsReadOutputLUT`, `_cmsReadDevicelinkLUT`, RGB matrix-shapers, the float tags, `ChangeInterpolationToTrilinear`, `cmsIsMatrixShaper`, `cmsIsIntentSupported` as input | gray and named color branches, profile sequences, `_cmsReadCHAD`, `_cmsReadMediaWhitePoint` |
| `cmstypes.c` | read: `XYZ `, `curv`, `para`, `mft1`, `mft2`, `mAB `, `mBA `, `mpet` (`cvst`, `matf`, `clut`, `bACS`, `eACS`), `clrt`; write: `mluc`, `XYZ `, `sf32`, `mAB `, `mBA `, `para`; `SupportedTags` in full, for the links `_cmsReadHeader` looks for | every other type; writers of `curv` and `mft*`: the tool's curves are `cmsBuildGamma`'s, and its profile is v4.2 |
| `cmsvirt.c` | `cmsCreateRGBProfileTHR`, `cmsCreate_sRGBProfileTHR`, `cmsCreateLab2ProfileTHR`, `cmsCreateLab4ProfileTHR`, `SetTextTags` | every other virtual profile |
| `cmslut.c` | stages: identity, curves, matrix, CLUT 16-bit and float, Lab and XYZ conversions, Lab v2/v4, float normalizers; `cmsStageSampleCLut16bit`, pipelines | `_cmsStageClipNegatives`, `_cmsStageAllocLabPrelin`, `cmsSliceSpace*`, `cmsPipelineEvalReverseFloat`, unlinking |
| `cmsgamma.c` | `AllocateToneCurveStruct`, the parametric types 1 to 8 and the reverses of 1 to 5, `EvalSegmentedFn`, tabulated and segmented builders, `cmsReverseToneCurve`, evaluation | `cmsJoinToneCurve`, smoothing, linearity and gamma estimates, types 108 and 109 |
| `cmsintrp.c` | all of it but the plug-in entry | |
| `cmsopt.c` | `_cmsOptimizePipeline` up to `cmsFLAGS_NOOPTIMIZE`, `PreOptimize` and what it calls, `FastIdentity16` | the optimizers |
| `cmspack.c` | `Unroll3Words`, `UnrollLabV2_16`, `Unroll16ToFloat`, `Pack3Words`, `PackLabV2_16`, `PackLabDoubleFromFloat`, `cmsFormatterForColorspaceOfProfile` | the other formatters |
| `cmspcs.c` | `cmsXYZ2Lab`, `cmsLab2XYZ`, `cmsXYZ2xyY`, `cmsxyY2XYZ`, `_cmsLCMScolorSpace`, `cmsChannelsOf*`, `_cmsEndPointsBySpace` for RGB | the 16-bit encodings, which only unreached formatters use; delta E, LCh |
| `cmsnamed.c` | `cmsMLUsetASCII`, what it adds | the rest: nothing reads the colorant table it would hold |
| `cmswtpnt.c` | `cmsWhitePointFromTemp`, `_cmsAdaptationMatrix`, `_cmsBuildRGB2XYZtransferMatrix` | `cmsTempFromWhitePoint`, `cmsAdaptToIlluminant` |
| `cmsmtrx.c` | the 3x3 operations used | vector helpers `_cmsVEC3*` are inlined |
| `cmsplugin.c`, `cmserr.c` | number readers and writers, fixed point, `_cmsGetTime`, `cmsSignalError`, the allocator's size limits | contexts, plug-ins, sub-allocation |

`cmsmd5.c` isn't reached: the tool doesn't compute a profile ID, so the header's is zero.
`cmsalpha.c`, `cmscam02.c`, `cmscgats.c`, `cmsgmt.c`, `cmshalf.c`, `cmsps2.c`, `cmssm.c` aren't
either.

## The wheel's build

- Both tools link lcms2 statically, and strings of each linked object match 2.17's sources, on
  both platforms. Windows' `/OPT:REF` drops what the program doesn't call, so the plug-in check
  with the version number survives only on Linux, as `0x87A` (2170) beside its message.
- `Buildlcms2.cmake` passes no flag that touches arithmetic: x86-64 SSE2 with `-O3` (GCC) or
  `/O2` (MSVC), no FMA, no fast math. The math library is imported on both sides (`pow`, `log10`,
  `exp`, `log`, `floor`, `sqrt`): Rust's `std` calls the same ones.
- Two things depend on the compiler: `lcms2_internal.h` makes `vsnprintf` the C runtime's
  `_vsnprintf` under MSVC (U-650), and `long` has 32 bits on Windows (I-653).

## What a comparison may set aside

Only bytes 24 to 35 of a profile, the creation time (ICC.1 section 7.2.1), which
`_cmsWriteHeader` fills from `time()`. `creation_time_is_all_that_varies` runs the wheel twice,
over a second apart, and shows that nothing else changes. The port's field must still be a date
within the hour after the wheel's. The description defaults to the output's name, and the
sweeps use the same names on both sides.

## Chunks

| # | Chunk | Upstream | Checks |
|---|---|---|---|
| 1 | This card | | |
| 2 | Numbers, matrices, white points: `types`, `cmserr`, `cmsmtrx`, `cmspcs`, `cmswtpnt` and the notice | `lcms2.h`, `cmserr.c`, `cmsmtrx.c`, `cmspcs.c`, `cmswtpnt.c` | `testcms2.c` |
| 3 | Interpolation and curves: `cmsintrp`, `cmsgamma` | `cmsintrp.c`, `cmsgamma.c` | `testcms2.c` |
| 4 | Pipelines: `cmslut`, `cmsopt` | `cmslut.c`, `cmsopt.c` | `testcms2.c` |
| 5 | Profiles: `cmsplugin`, `cmsio0`, `cmsnamed`, `cmstypes` | those files | `testcms2.c` |
| 6 | Transforms: `cmsio1`, `cmsvirt`, `cmspack`, `cmscnvrt`, `cmssamp`, `cmsxform` | those files | `testcms2.c` |
| 7 | The tool: `ocioicc`, `ociobakelut`, the sweeps and the registers | `ocioicc.cpp`, `main.cpp` | sweeps against the wheel |

The wheel can only be asked through the tool, so the sweeps come with chunk 7, once there is
something to run. Chunks 2 to 6 carry lcms2's own tests.

## lcms2's test suite

Of `testbed/testcms2.c`, these cover the translated code and are ported, in the `_tests.rs` files
next to it: the fast floors, 15.16 fixed point, D50, 1D and 3D interpolation, 3D to 8D through a
stage, Lab to XYZ and to xyY, gamma curves (with `cmsEstimateGamma`, in the test), parametric
curves (6 to 8 forward only), reversal of a flat table, segmented curves, and LUT stages.

The others use something that isn't reached, so there is nothing of ours for them to cover: the
formatters (`TYPE_RGB_FLT`, `TYPE_RGB_8`, planar, alpha), `cmsBuildTabulatedToneCurveFloat`,
`cmsJoinToneCurve`, `cmsPipelineEvalReverseFloat`, named colors, gray and CMYK, proofing, gamut
checks, black preservation, CGATS, PostScript, MD5, plug-ins, contexts. What is left of transforms
and profiles the sweeps check against the wheel.
