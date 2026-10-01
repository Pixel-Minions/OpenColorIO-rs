# Improvement candidates

Upstream bugs and limitations that the port copies, so that its results match OpenColorIO
2.5.2 byte for byte, but that could be fixed. Nothing here is fixed during the port. At the
end, the owner decides on each one: keep it, or fix it as a deviation (`docs/deviations.md`),
which changes the port's results for the inputs the entry names.

The last section lists places where upstream's behaviour is undefined because it reads or
writes memory it doesn't own. The port can't copy those. The owner's general rule
(2026-09-30): the port returns an error there instead, unless an entry says otherwise. Other
undefined behaviour that both wheels resolve the same predictable way, such as an integer
overflow that wraps, is copied like any other bug and listed with the `I-` entries.

**Adding an entry** (`CLAUDE.md`, rule 1). A chunk that ports an upstream bug or limitation,
or a platform difference that exists only because of how the wheels were built, adds an
entry here in the same chunk. Numbers are never changed or reused: take the next free one
in the series (`I-` or `U-`), whatever the section. An entry says:
- **Upstream:** what OCIO does, with its source (`file:line @ v2.5.2`) or the wheel probe that
  showed it;
- **Who notices:** the inputs or calls affected, in plain words;
- **A fix:** what the port would do instead, and what that changes;
- **Status:** matched (with the card), to be matched (with the phase), or open.

## Images and CPU processing

### I-1. Huge images fail on Windows

- **Upstream:** image sizes and positions are C `long`, which is 32 bits on Windows and 64 bits
  on Linux. On Windows, for images of 2^31 pixels or more (about 46,000 × 46,000), the paths
  that pack channel by channel count the pixels as `width * height` and start each scanline at
  pixel `y * width`, and both wrap (`ImagePacking.cpp:33-63, 103-133, 173-203, 243-273`,
  `ScanlineHelper.cpp:142-146, 169-173` @ v2.5.2). Where a scanline's wrapped start falls
  outside the wrapped count, reading it raises "Invalid output image position.", and writing
  it silently writes nothing, so the row is left as it was. Where it falls inside, the scanline
  is read or written from that pixel: for a top-down layout, that is the wrong pixels, from
  the middle of a row on into the next one; it can also reach outside the image's memory
  (U-15). Through the Windows wheel, a planar F32 image of 65,536 × 65,537 pixels processes its
  first row and then raises. Linux processes all of them (2^32 pixels of 65,536 × 65,536 in
  17 s).
- **Who notices:** applications that process single images of over 2 gigapixels on Windows.
- **A fix:** 64-bit sizes on every platform, so those images work on Windows too.
- **Status:** matched in `p1-bitdepth` (1.1d): the message and every buffer byte, including the
  partly written output, checked against the Windows wheel
  (`crates/ocio-ops/tests/image_packing_oracle.rs`, and through the scanline helper in
  `scanline_helper_oracle.rs`), and wrapped starts on small buffers
  (`crates/ocio-ops/src/image_packing_tests.rs`). The owner chose to match it on 2026-09-30.
  Where a wrapped scanline would reach outside the memory, the port returns an error (U-15).

### I-2. "Invalid x stride." is checked on Windows only

- **Upstream:** `PackedImageDesc` refuses an x stride equal to `AutoStride`, the value a stride
  has only after an overflow (`ImageDesc.cpp:310-313`). The check comes after
  `std::abs(m_xStrideBytes)`, which is undefined for that value, so GCC removed it. The Windows
  wheel raises "Invalid x stride." for every width. The Linux wheel, with an `AutoStride` y
  stride, raises "Invalid y stride." for odd widths and builds the image for even widths (x
  stride `INT64_MIN`, y stride 0), which then reads memory it doesn't own when applied.
- **Who notices:** only code that passes absurd strides.
- **A fix:** the same check on both platforms.
- **Status:** matched in `p1-bitdepth` (1.1b): the check on Windows; on Linux, the same
  messages, and D-2 refuses an image the Linux wheel would build.

### I-3. The RGBA fast path truncates the x stride to 32 bits

- **Upstream:** `PackedImageDesc` decides whether an image is tightly packed RGBA with
  `div((int)m_xStrideBytes, (int)m_chanStrideBytes)` (`ImageDesc.cpp:264`). An x stride of 4 GiB
  or more is truncated, so a layout whose pixels are 4 GiB plus 4 channels apart counts as
  tightly packed, and the fast path processes the wrong bytes. Through the Linux wheel, an
  RGBA F32 image with an x stride of 16 − 2^32 in a 4 GiB buffer had its pixels processed as
  if contiguous, and 16 bytes past the buffer were written.
- **Who notices:** nobody in practice: it needs pixels 4 GiB apart.
- **A fix:** compare the full 64-bit stride.
- **Status:** matched in `p1-bitdepth` (1.1b). D-2 refuses the image when the rows the fast
  path processes reach outside the buffer.

### I-4. Integer and half-float input drop a matrix's alpha offset

- **Upstream:** with any input bit depth except F32 and the default optimization, the optimizer
  bakes the leading separable ops into one Lut1D (`OptimizeSeparablePrefix`,
  `OpOptimizers.cpp:553-595`). That Lut1D scales alpha instead of offsetting it, so a matrix's
  alpha offset is lost. Seen through the wheel for UINT8, UINT10, UINT12, UINT16 and F16 input,
  with a Matrix then a Log; `OPTIMIZATION_NONE` keeps the offset.
- **Who notices:** non-F32 images through a matrix that offsets alpha, with the default
  optimization.
- **A fix:** keep alpha's offset through the bake.
- **Status:** to be matched in Phase 2 (WP 2.5, the Lut1D bake).

### I-18. Very large strides wrap around in the checks

- **Upstream:** the description checks multiply strides (for example `m_chanStrideBytes *
  m_numChannels`, `ImageDesc.cpp:305`) and take `std::abs` of them. For strides of 2^61 bytes or
  more the products overflow, and `std::abs(INT64_MIN)` is undefined; both wheels wrap.
- **Who notices:** nobody in practice.
- **A fix:** checked arithmetic that refuses such strides.
- **Status:** matched in `p1-bitdepth` (1.1b). The port wraps the same way, then D-2 refuses
  any such image that reaches outside its buffer.

### I-41. `applyRGB` and `applyRGBA` convert the pixel's bytes in place

- **Upstream:** `CPUProcessor::applyRGB` and `applyRGBA` (`CPUProcessor.cpp:433-465`) pass the
  one float pixel as both the input and the output of every op, the bit-depth conversions
  included. With an input bit depth other than F32, the conversion reads the pixel's first
  bytes as 8- or 16-bit values and writes four floats over the same 16 bytes, so it reads some
  values after it has overwritten them with floats. With an output bit depth other than F32,
  the result is the pixel's first 4 or 8 bytes, as that type, and the bytes after them keep the
  ops' floats. Which values are overwritten before they are read depends on the compiler, so
  the wheels differ for 10-, 12-, 16-bit and half input: MSVC (Windows) reads each value after
  storing the float before it; GCC (Linux) reads each one step ahead, so only alpha is read
  after a store. 8-bit input is read in order on both. Seen in each wheel's machine code
  (`BitDepthCast<inBD, BIT_DEPTH_F32>::apply`: Windows 0x180089c30, 0x18008b220, 0x18008c4c0;
  Linux 0x1df730, 0x1dcad8, 0x1d9dc0, 0x1d7650, 0x1e2ed0) and through the oracle: UINT16 codes
  1000, 2000, 3000, 4000 give different floats on the two platforms. The conversions from F32
  read every float before storing over it, on both.
- **Who notices:** C++ and Rust callers of `applyRGB` or `applyRGBA` on a CPU processor whose
  input or output bit depth isn't F32. Python's `applyRGB` and `applyRGBA` build an image and
  call `apply`, so they don't see this.
- **A fix:** convert through a separate pixel, as `apply` does; or refuse other bit depths.
  Either changes the results for those processors, and makes them the same on both platforms.
- **Status:** matched in `p1-engine` (1.2d), each platform as its wheel compiled it.

## Configs and cache IDs

### I-5. Different transforms can share a cached processor

- **Upstream:** `Config::getProcessor` caches processors under a hash of the transform's text
  (`Config.cpp:4830-4841`), and the cache is on by default. That text leaves things out:
  - for a `Lut1DTransform` or `Lut3DTransform` that holds its values in memory, it gives only the
    size, settings and the smallest and largest values (`transforms/Lut1DTransform.cpp:184-224`,
    `transforms/Lut3DTransform.cpp:174-218`);
  - it prints numbers with 9 significant digits, so two `MatrixTransform`s one ULP apart have
    the same text (seen through the wheel: the second gets the first one's processor, and with
    `PROCESSOR_CACHE_OFF` they differ).

  Two such transforms get the same processor from one config: the second applies the first.
- **Who notices:** applications that build transforms in code, not from files, and get several
  processors from one config.
- **A fix:** put every value, or a hash of it, in the key.
- **Status:** to be matched in Phase 3 (the processor cache).

### I-6. A malformed UTF-8 sequence truncates a config's text and cache ID

- **Upstream:** yaml-cpp decodes text leniently: the overlong sequence `C0 80` becomes a NUL byte,
  and `serialize()` and the config's cache ID stop at that byte (`OCIOYaml.cpp:5447`). Configs
  that differ only after it share a cache ID.
- **Who notices:** configs that contain malformed UTF-8.
- **A fix:** keep or refuse such bytes instead of truncating.
- **Status:** to be matched in Phase 3 (YAML).

### I-7. NaN parameters print differently on Windows and Linux

- **Upstream:** a transform's text (`repr()`), error messages and op cache IDs print NaN as the
  platform's C++ library does: Windows writes `nan`, `-nan(ind)`, `nan(snan)`, `-nan(snan)` or
  `-nan`; Linux writes `nan` or `-nan`. A processor with a NaN parameter has a different cache
  ID on each platform. Through the wheels, the oracle's commands show the same split in two
  more places, for a negative NaN, `-nan(ind)` on Windows and `-nan` on Linux:
  - the messages of `validate()` that print a parameter's value (`transform_text`, over 247
    transforms of 13 classes holding special values; `oracle/ocio_oracle/transform_text.py`,
    c49a36e);
  - shader text, whose literals print the parameter (`getFloatString`,
    `GpuShaderUtils.cpp:21-35`; `gpu_shader`, over 1230 shaders in all 10 languages;
    `oracle/ocio_oracle/gpu.py`, 470d0b7).
- **Who notices:** anyone comparing text, messages, shaders or cache IDs across platforms for
  transforms with NaN parameters.
- **A fix:** one spelling on both platforms.
- **Status:** matched in `cfmt` (WP 0.5); each op's text, validation messages and shader
  literals use it as the op lands (D12).

### I-8. Built-in configs have Windows line endings on Windows

- **Upstream:** the Windows wheel embeds the built-in configs' YAML with CRLF line endings, a side
  effect of how its build checked out the sources. The Linux wheel and upstream's files use LF.
  `BuiltinConfigRegistry()[name]` returns different text on each platform; parsed configs,
  `serialize()` and cache IDs are the same.
- **Who notices:** code that reads a built-in config's text directly.
- **A fix:** LF on both platforms.
- **Status:** to be matched in Phase 3 (built-in configs), per D12.

### I-9. A search path string is split at every colon, even on Windows

- **Upstream:** `Context::setSearchPath` splits a string at each `:` (`Context.cpp:202`), so a
  Windows path with a drive letter, `C:/luts`, becomes `C` and `/luts`. A config can avoid it
  by listing its paths as a YAML sequence.
- **Who notices:** Windows configs with a drive letter in a string `search_path`.
- **A fix:** don't split after a drive letter.
- **Status:** to be matched in Phase 3.

### I-10. Some descriptions don't read back the same

- **Upstream:** `serialize()` writes a description whose first line starts with a space, or that
  holds a carriage return, in a form that reads back differently (the yaml-cpp emitter).
- **Who notices:** configs with such descriptions that are saved and loaded again.
- **A fix:** write those descriptions in a form that round-trips.
- **Status:** matched in the YAML emitter (WP 0.5); used from Phase 3.

### I-19. Saving a config drops values close to their defaults

- **Upstream:** `serialize()` leaves out a `MatrixTransform`'s matrix when it is within the
  equality tolerance of identity (I-20), and offsets within that tolerance of 0
  (`OCIOYaml.cpp:3071-3083`). It does the same for a CDL's slope, offset, power and saturation
  (`OCIOYaml.cpp:740-762`). Through the wheel, a matrix entry of 1 + 2e-7 with an offset of
  2e-45 saves as an empty `!<MatrixTransform> {}`; 1 + 4e-7 and 5e-45 are written.
- **Who notices:** anyone who saves and reloads a config with such values: they come back as
  the defaults.
- **A fix:** leave a value out only when it equals its default exactly.
- **Status:** to be matched in Phase 3 (the YAML writer), with `p1-math`'s helpers.

## Numeric helpers

### I-20. Double values are compared to 0 and 1 in float precision

- **Upstream:** `IsScalarEqualToZero<double>` and `IsScalarEqualToOne<double>` convert the value
  to float and allow 2 float ULPs (`MathUtils.cpp:17-39`). So:
  - a value with |x| ≤ 2.5·2⁻¹⁴⁹ (about 3.50e-45) counts as 0: a `LogAffineTransform` slope of
    2.1e-45 is refused as "cannot be 0", and 3.6e-45 is accepted;
  - a value in [1 − 2.5·2⁻²⁴, 1 + 2.5·2⁻²³] (about 1 − 1.49e-7 to 1 + 2.98e-7) counts as 1: a
    v1 exponent in that band is dropped as a no-op.
- **Who notices:** configs with tiny slopes, or with exponents and gains within 3e-7 of 1.
- **A fix:** compare in double.
- **Status:** matched in `p1-math` (1.4a).

### I-21. The matrix inverse's singularity test is absolute

- **Upstream:** `GetM44Inverse` calls a matrix singular when its determinant, as a float, is
  within 2 ULPs of 0 (`MathUtils.cpp:217`). A matrix of 1e-12 times identity is refused while
  1e-11 is inverted; a nearly singular matrix is inverted into huge values; a matrix with a NaN
  "inverts" to all NaN and reports success.
- **Who notices:** nobody in 2.5.2: nothing calls it (the matrix op has its own inverse).
- **A fix:** a test relative to the matrix's scale, and refusing NaN.
- **Status:** matched in `p1-math` (1.4b).

### I-22. The half-float limits are rounded literals

- **Upstream:** `GetHalfMin` and `GetHalfNormMin` are decimal literals, not exactly 2⁻²⁴ and
  2⁻¹⁴ (`MathUtils.h:105-113`). So `ClampToNormHalf`, which Cg shader literals go through,
  flushes to 0 only below the double nearest 6.10351562e-05, and prints doubles in the narrow
  band just under 2⁻¹⁴ instead of flushing them. Confirmed through the Windows wheel's Cg
  shader text. No float value falls in the band.
- **Who notices:** Cg shaders with double parameters in that band.
- **A fix:** the exact constants.
- **Status:** matched in `p1-math` (1.4a).

### I-24. Values computed with the math library differ between Windows and Linux

- **Upstream:** OCIO computes some values with the platform's math library, the UCRT on Windows
  and glibc on Linux, which round some results differently. Through the wheels:
  - the ACES 2 output transform's GPU tables differ, and for its SDR 2.0 preset the shader text
    too, which prints some of those values (the hues);
  - the 1D LUTs that the optimizer bakes from ExponentTransform and ExponentWithLinearTransform
    for UINT8, UINT10, UINT12, UINT16 and F16 input, at the default and DRAFT flags, differ,
    and so does the optimized processor's cache ID, which hashes them.

  Log, LogAffine, LogCamera, ExposureContrast, and the LUT and built-in bakes came out the same
  on both, over 3309 cases (the p1-oracle review's survey).
- **Who notices:** anyone comparing GPU textures, SDR 2.0 shaders, or renders of exponents at 8
  to 16 bits between a Windows and a Linux machine.
- **A fix:** one math library on every platform, which changes the port's results on at least
  one of them.
- **Status:** to be matched in Phase 2, with the Lut1D bakes and the ACES 2 tables (D12: the
  port calls the platform's functions, as OCIO does). The review suspects these values may also
  depend on the CPU (SIMD renderers, glibc's ifunc variants). If so, their checks belong in
  `cpu-tests`.

## Ops

### I-40. A file no-op has no cache ID

- **Upstream:** `FileNoOp::getCacheID` returns the op's `m_fileReference`, which the
  constructor never sets: it gives the path to the op's `FileNoOpData` instead
  (`ops/noop/NoOps.cpp:300-304, 358-361`). So the cache ID of the op that marks a loaded file
  is empty. Processor cache IDs skip no-ops (`Op.cpp:448-465`), but `SerializeOpVec` prints
  each op's cache ID (`Op.cpp:473-489`), in the optimizer's debug log
  (`OpOptimizers.cpp:618-625, 636-646, 737-754`): the line of a file no-op has no file name,
  while a look no-op's line names the look.
- **Who notices:** people reading OCIO's debug log.
- **A fix:** return the path, as `LookNoOp` returns the look.
- **Status:** matched in `p1-engine` (1.2c).

### I-42. Matrix renderers' NaNs depend on the platform and, on Windows, on the pixel's position

- **Upstream:** the Matrix renderers (`ops/matrix/MatrixOpCPU.cpp`) multiply and add the
  pixel's values with the matrix's. Where two NaNs meet in a product or a sum, x86 returns the
  first operand's, and the two compilers ordered the operands differently: GCC computes
  `(b*m2 + a*m3) + (r*m0 + g*m1)` (Linux wheel 0x4e4140, 0x4e40c0), MSVC
  `(a*m3 + b*m2) + (g*m1 + r*m0)` in its four-pixel loop (Windows wheel 0x1802b24f4,
  0x1802b26b4) and `(g*m1 + r*m0) + (a*m3 + b*m2)` in the loop that finishes the last
  `numPixels % 4` pixels (0x1802b260b, 0x1802b27db). `ScaleRenderer` and
  `ScaleWithOffsetRenderer` compute blue as `in * scale` everywhere except MSVC's remainder
  loop, which computes `scale * in` (0x1802b29c2, 0x1802b2be4). So the NaN a pixel gets depends
  on the platform and, on Windows, on where the pixel sits in its row.
- **Who notices:** images whose pixels have NaNs of different signs or payloads in two or
  more channels, through any non-diagonal matrix (GCC's `r*m0 + g*m1` keeps red's NaN, MSVC's
  four-pixel loop `g*m1 + r*m0` green's); and NaN pixels through a matrix with NaN
  coefficients (blue, for diagonal matrices). The NaN's sign and payload differ.
- **A fix:** one operand order for every platform and loop.
- **Status:** matched in `p1-engine` (1.3m2), each wheel's order per platform and loop.

### I-43. Inverting a matrix flips a NaN offset's sign on Linux only

- **Upstream:** `MatrixOpData::getAsForward` negates the inverse's offsets with
  `invOffsets.scale(-1.)` (`ops/matrix/MatrixOpData.cpp`). MSVC multiplies by -1 (Windows
  wheel 0x1802b3b5f, `mulpd`), which keeps a NaN's sign; GCC folds `x * -1.0` into a negation
  (Linux wheel 0x4e8303, `xorpd` with -0.0), which flips it. Every other value gives the same
  bits.
- **Who notices:** inverse matrices whose offsets come out NaN: the NaN's sign, in the cache
  ID, the pixels and the shader text, differs between Windows and Linux. In shaders, the
  flipped sign adds Linux's `-nan` where Windows writes `nan` (I-7, I-35).
- **A fix:** negate on both, or multiply on both.
- **Status:** matched in `p1-engine` (1.3m1), each wheel's operation.

### I-44. Inverting a matrix orders its NaNs per platform

- **Upstream:** `MatrixArray::inverse` (Imath's Gauss-Jordan elimination,
  `ops/matrix/MatrixOpData.cpp`) subtracts `f * t[..]` in each step. MSVC keeps the source's
  `f * x` (Windows wheel 0x1802b4758); GCC computes `x * f`, except for the last product of
  each step, whose register it reuses (Linux wheel, its 9 unrolled steps, e.g. 0x4e673e and
  0x4e67de). When both are NaN, the first operand's NaN comes out.
- **Who notices:** inverse matrices with NaN coefficients: the inverse's NaNs, in the cache
  ID, the pixels and the shader text, differ between Windows and Linux.
- **A fix:** one operand order for both platforms.
- **Status:** matched in `p1-engine` (1.3m1), each wheel's order.

### I-45. Optimization flags are 32 bits on Windows and 64 on Linux

- **Upstream:** `OptimizationFlags` is an `enum : unsigned long`
  (`include/OpenColorIO/OpenColorTypes.h:634`), 32 bits with MSVC and 64 with GCC. The binding
  converts a Python integer to it: on Linux `OptimizationFlags(2**32 + 1)` is accepted, and the
  CPU processor's cache ID prints `oFlags 4294967297`; on Windows the same call raises
  `TypeError`. The GPU processor's cache ID prints them whole too (`GPU Processor: oFlags
  <flags> ops : ...`), which `crates/ocio-gpu/tests/matrix_op_gpu_oracle.rs` checks on Linux
  against the wheel with `(1 << 32) | OPTIMIZATION_DEFAULT`.
- **Who notices:** callers passing flags above bit 31, which no flag uses.
- **A fix:** a 32-bit type on every platform, or refusing unknown bits.
- **Status:** matched in `p1-engine` (1.2d): the port's `OptimizationFlags` holds a
  `c_ulong`.

### I-46. The optimizer stops after 81 passes, and logs its cap at exactly 80

- **Upstream:** `OpRcPtrVec::optimize` (`OpOptimizers.cpp:628-735`) loops
  `while (passes <= MAX_OPTIMIZATION_PASSES)`, with `MAX_OPTIMIZATION_PASSES = 80`, so it makes
  up to 81 passes, and then logs "The max number of passes, 80, was reached" only when
  `passes == 80`: when the 81st pass found nothing left to do, not when the cap stopped it.
  Seen through the wheel with the default optimization: lists of 80, 81 and 82 Matrix ops
  keep one op, 83 keep two, 84 three and 90 nine, and the message appears at 81 ops only.
- **Who notices:** very long lists of ops that combine one pair per pass; people reading the
  debug log.
- **A fix:** stop at 80 passes, and log when the cap stops the loop.
- **Status:** matched in `p1-engine` (1.2d).

### I-50. A max-only range followed by a min-only one can't be optimized

- **Upstream:** `RangeOpData::compose` (`ops/range/RangeOpData.cpp:352-431`) keeps the first
  range's input bounds when the composition outputs a constant. For a range with only a
  maximum followed by one with only a minimum at or above it (or the other way round), the
  result has an output bound set where its input bound is empty, and its constructor's
  `validate` raises ("In and out minimum limits must be both set or both missing in Range.",
  or the maximum's). So the optimizer, which combines neighbouring Range ops
  (`RangeOp::combineWith`, `ops/range/RangeOp.cpp:150-174`), can't build the CPU processor,
  at every optimization level that combines ranges. Seen through the wheel, for
  `[-, 0.5] -> [-, 0.5]` then `[0.5, -] -> [0.5, -]`, and the reverse.
- **Who notices:** a processor with such a pair of ranges, which raises instead of clamping
  every value to 0.5.
- **A fix:** when the composition outputs a constant, make a range that outputs it for every
  input: two distinct input bounds, and the constant as both output bounds (such as
  `[0, 1] -> [0.5, 0.5]`, which the wheel accepts: it clamps every value below 0 and above 1 to
  0.5 too). The input bounds of the two ranges don't do: here they combine into `[0.5, 0.5]`,
  which `validate` refuses as too close, or into one-sided bounds.
- **Status:** matched in `p1-range` (1.3r2).

### I-51. Range bounds and ranges compare differently in each order

- **Upstream:** `RangeOpData::FloatsDiffer(x1, x2)` (`ops/range/RangeOpData.cpp:313-330`)
  compares with an absolute tolerance of 1e-6 when `|x1| < 1e-3` and a relative one otherwise,
  chosen by the first argument only. So the result depends on the order: `FloatsDiffer(9.995e-4,
  1e-3)` is false (absolute, 5e-7 apart) and `FloatsDiffer(1e-3, 9.995e-4)` true (relative,
  5e-4). `validate` compares a one-sided range's output bound with its input bound
  (`FloatsDiffer(minOut, minIn)`, `RangeOpData.cpp:244-258`): the wheel accepts
  `[1e-3, -] -> [9.995e-4, -]` and refuses `[9.995e-4, -] -> [1e-3, -]` ("In and out minimum
  limits must be equal"), and likewise for the maximum-only pair. `equals` compares each bound
  with the other range's (`RangeOpData.cpp:515-546`), so it isn't symmetric: a range with
  bounds 9.995e-4 equals one with bounds 1e-3, but not the other way round, and so do the
  `RangeTransform`s that hold them.
- **Who notices:** one-sided ranges with bounds near 1e-3 that differ by less than 1e-6, and
  code that compares ranges or `RangeTransform`s with `equals`.
- **A fix:** choose the tolerance from both values (for example, absolute when both are below
  1e-3), so that the comparison is symmetric.
- **Status:** matched in `p1-range` (1.3r1); `range_op_data_oracle.rs` checks both orders of
  validation and equality against the wheel.

## Transforms

### I-11. Copying a group transform shares its children

- **Upstream:** `GroupTransform::createEditableCopy` copies the list of child transforms, not the
  children (`transforms/GroupTransform.cpp:36-44`). Editing a child of the copy edits the
  original's child too.
- **Who notices:** code and Python scripts that copy a group and then change its children.
- **A fix:** copy the children as well.
- **Status:** to be matched when `GroupTransform` is ported (1.8a).

### I-14. Metadata attribute names match exactly when set, but ignoring case when read

- **Upstream:** in the metadata of transforms, ops and LUT files (`FormatMetadata`),
  `addAttribute`, `setName` and `setID` replace an attribute only when its name is exactly the
  same (`fileformats/FormatMetadata.cpp:94-112`), while `getAttributeValue(name)`, `getName`,
  `getID` and `combine` use the first attribute whose name matches ignoring ASCII case
  (`fileformats/FormatMetadata.cpp:140-179, 219-231, 303-332`). After
  `addAttribute("Name", "a")`, `setName("b")` adds a second attribute, `name="b"`, and
  `getName()` still returns `a`. Under Python on Windows, the lookups also match names that
  differ in the case of non-ASCII bytes, by the ANSI code page; the port folds `A`-`Z` only
  (D-4), so there `combine` keeps apart attributes that the Windows wheel joins.
- **Who notices:** code that spells an attribute name with different cases.
- **A fix:** match names the same way everywhere, so that setting an attribute replaces the one
  that reading returns.
- **Status:** matched in `p1-foundations` (1.2a), with ASCII case folding (D-4); checked against
  the wheel in `crates/ocio-ops/tests/format_metadata_oracle.rs`.

### I-15. A misspelled error message

- **Upstream:** renaming a metadata element to `ROOT`, or adding a child element named `ROOT`,
  fails with "'ROOT' is reversed for root FormatMetadata elements."
  (`fileformats/FormatMetadata.cpp:241`): "reversed" for "reserved".
- **Who notices:** anyone who reads the message.
- **A fix:** "reserved".
- **Status:** matched in `p1-foundations` (1.2a), checked against the wheel in
  `crates/ocio-ops/tests/format_metadata_oracle.rs`.

## Logging

### I-16. Two messages bypass the logging function

- **Upstream:** the warning about an invalid `OCIO_LOGGING_LEVEL`, and the version line logged
  when that variable asks for debug messages, are written straight to stderr
  (`Logging.cpp:45-50, 57-61`), even when the application has set its own logging function.
- **Who notices:** applications that show or collect OCIO's log through a logging function.
- **A fix:** send them through the logging function, like every other message.
- **Status:** matched in `p1-foundations` (1.2e), checked against the wheel (stderr bytes
  included) in `crates/ocio-ops/tests/logging_oracle.rs`.

### I-17. A NUL in a logged message cuts its line

- **Upstream:** the logging function receives each line as a C string (`Logging.cpp:86`), so a
  line that holds a NUL byte stops there, and loses the rest of its text and its line break.
  A warning about a key or name with a NUL shows this (seen through the wheel in 1.2e); I-6
  describes one way such names arise.
- **Who notices:** applications that log messages about names with NUL bytes; the next line of
  their log continues on the same line.
- **A fix:** pass the whole line, with its length.
- **Status:** matched in `p1-foundations` (1.2e), checked against the wheel in
  `crates/ocio-ops/tests/logging_oracle.rs`.

## GPU shaders

### I-30. Large whole numbers become invalid shader literals

- **Upstream:** `getFloatString` writes a `float` with 9 significant digits (`%.9g`) or a
  `double` with 17 (`%.17g`). It then adds a `.` after any finite whole number, so that the shader
  reads it as floating point (`GpuShaderUtils.cpp:21-35`). From 1e9 (`float`) or 1e17 (`double`)
  up, `%g` switches to exponent notation, and the `.` lands after the exponent. Through the wheel,
  a matrix offset of 1e10 is written `vec4(1e+10., ...)` in GLSL, and the same way in every
  language but Cg, which clamps to the half range first.
- **Who notices:** shaders for transforms with a whole-number parameter of a billion or more. In
  C-style shading languages, a literal's `.` must come before its exponent.
- **A fix:** put the `.` in the mantissa (`1.e+10`), or leave it out when there is an exponent.
- **Status:** matched in `p1-gpu-infra` (1.7a).

### I-33. A texture of 2^32 floats or more keeps a wrapped count

- **Upstream:** a texture's float count, `w * h * d` times 1 or 3 channels, is taken in C
  `unsigned` arithmetic, which wraps at 2^32, and that many floats are copied
  (`CreateArray`, `GpuShader.cpp:24-37`). A 1D LUT's texture as wide as the width limit allows
  (`setTextureMaxWidth` takes up to 2^32 - 1) and high enough keeps fewer values than it has
  texels. Python's binding counts `width * height` in `unsigned` too, but times the channels in
  64 bits (`PyGpuShaderDesc.cpp:116-160, 259-283`): `getValues` then reads past the values
  kept, which is undefined. A 3D texture can't reach the count: the wheel refuses an edge of
  130 texels or more before copying anything. A count that wraps to exactly 0 leaves the vector
  empty, and `std::memcpy(&res[0], buf, 0)` then takes the address of its element 0
  (`GpuShader.cpp:36`), which is undefined for an empty vector, though the copy is of 0 bytes.
- **Who notices:** textures of 2^32 floats (16 GiB) or more.
- **A fix:** count in 64 bits, and refuse a texture that doesn't fit.
- **Status:** matched in `p1-gpu-infra` (1.7c): the port copies the wrapped count, and keeps no
  values when it wraps to 0. The oracle refuses these textures (`gpu_shader_desc`,
  `_check_texture_size`); Phase 6 decides for Python's `getValues`.

### I-34. The Metal class wrapper misreads declarations that line feeds split

- **Upstream:** in MSL, the class wrapper reads the shader's declarations back line by line to
  build its class (`GpuShaderClassWrapper.cpp:285-372`). It takes the line after each line that
  starts with `texture` for that texture's sampler, and reads the sampler's name from
  `find("sampler") + 7` (line 332): without `sampler`, from offset 6 of that line. Every other
  line becomes a parameter, its first word the type and the next one the name (lines 342-353).
  Line feeds in the resource prefix, or in declaration code a caller adds
  (`addToParameterDeclareShaderCode`, `addToTextureDeclareShaderCode`), cut declarations into
  such lines: the class gets a sampler with a wrong name, and parameters that are pieces of
  declarations. A texture declared on the last line, without a line feed, does the same:
  `std::getline` then fails at the end of the text and leaves the texture's line in the buffer,
  which is read as the sampler's line. Through the wheel, `texture1d<float> t;` alone gives the
  class a sampler named `e1d<float>`. A line shorter than 6 bytes there is read past its end
  (U-10).
- **Who notices:** MSL shaders whose resource prefix holds line feeds, or whose added
  declaration code declares a texture without a sampler on the next line. The class doesn't
  compile.
- **A fix:** read declarations, not lines: refuse line feeds in names, and a texture without
  its sampler.
- **Status:** matched in `p1-gpu-infra` (1.7d), checked against the wheel in
  `crates/ocio-gpu/src/gpu_shader_class_wrapper_tests.rs` and
  `crates/ocio-gpu/tests/gpu_shader_desc_oracle.rs`.

### I-35. Non-finite and float-overflowing parameters become invalid shader literals

- **Upstream:** `getFloatString` writes a literal with the C++ library's `%g`
  (`GpuShaderUtils.cpp:21-35`), so infinities and NaNs come out as `inf`, `-inf`, `nan`, and
  `-nan(ind)` on Windows or `-nan` on Linux (I-7), which no shading language reads as numbers.
  The Matrix writer also rounds a diagonal's values and the offsets to `float` first
  (`ops/matrix/MatrixOpGPU.cpp:37-62`), so a finite `double` above `FLT_MAX` becomes `inf` there,
  while a full matrix keeps it as a 17-digit `double`. Through the wheel, in GLSL: a diagonal
  of 1e39 gives `vec4(inf, 1., 1., 1.) * res`; the same value in a full matrix gives
  `mat4(9.9999999999999994e+38., ...)` (I-30's `.`); offsets of 1e39 and -1e39 give
  `vec4(inf, -inf, 0., 0.)`; NaN parameters give `nan`.
- **Who notices:** shaders for transforms with NaN or infinite parameters, or diagonal matrices
  and offsets beyond the float range: the shader doesn't compile. The CPU renders them.
- **A fix:** write non-finite values in a form each language accepts (`1.0/0.0`,
  `uintBitsToFloat(...)`), or refuse such parameters on the GPU.
- **Status:** matched in `p1-gpu-ops` (1.3m4), checked against the wheel in
  `crates/ocio-gpu/tests/matrix_op_gpu_oracle.rs` (`extreme_parameters_write_the_wheels_shader`).

## Python module (`ocio-py`)

### I-12. A channel order passed without its keyword is misread

- **Upstream:** `PackedImageDesc(buffer, width, height, CHANNEL_ORDERING_BGR)` takes the channel
  order as a channel count: the image becomes 4-channel RGBA, and RGBA, BGRA and ABGR raise an
  error. Only `chanOrder=...` reaches the channel-order constructor (seen through the wheel in
  O1.2).
- **Who notices:** Python scripts that pass the channel order positionally.
- **A fix:** choose the constructor by the argument's type.
- **Status:** to be matched in Phase 6 (D13).

### I-13. Read-only NumPy arrays are written to

- **Upstream:** the binding's `apply` writes into NumPy arrays marked read-only (seen through the
  wheel in O1.2).
- **Who notices:** scripts that rely on a read-only array staying unchanged.
- **A fix:** refuse read-only destinations.
- **Status:** to be matched in Phase 6 (D13).

### I-23. The buffer-type error names integer types oddly

- **Upstream:** when a NumPy array has the wrong type, the binding says, for example, "expected
  'u' (16-bit), but received float32", not "expected uint16". It names the expected type from
  the dtype's kind character, which its helper doesn't recognize (`PyUtils.cpp:26-54, 171`).
  Float types print correctly.
- **Who notices:** Python users who read the message.
- **A fix:** name the expected type as `uint16`, like the received one.
- **Status:** to be matched in Phase 6 (D13). The Rust API's own error for a slice of the wrong
  type uses the clean names.

### I-31. A shading language made from a number can abort the process

- **Upstream:** Python turns any integer into a `GpuLanguage` (`OCIO.GpuLanguage(42)`), and
  `GpuShaderDesc.setLanguage` accepts it. Extraction then raises "Unknown GPU shader language.".
  `getCacheID()`, however, ends the process. It is `noexcept` (`OpenColorIO.h:3410`), and the
  `GpuLanguageToString` it calls throws "Unsupported GPU shader language."
  (`GpuShaderDesc.cpp:263-282`, `ParseUtils.cpp:258-275`), so C++ calls `std::terminate`.
  Through the wheels, the Python process ends with SIGABRT (exit 134) on Rocky Linux 9 and with
  `0xC0000409` on Windows.
- **Who notices:** Python code that makes a language from a number outside 0-9.
- **A fix:** refuse the number when the language is set, with "Unsupported GPU shader language.".
- **Status:** open; decided in Phase 6. The Rust `GpuLanguage` holds only upstream's languages,
  so only the Python module can meet it.

### I-32. `repr()` of a GradingRGBCurve prints an address

- **Upstream:** the binding gives GradingHueCurve's class its repr twice, and GradingRGBCurve's
  none: `PyGradingData.cpp:470` calls `defRepr(clsGradingHueCurve)` where `clsGradingRGBCurve`
  was meant. So a GradingRGBCurve's `repr()` is pybind11's default,
  `<PyOpenColorIO.PyOpenColorIO.GradingRGBCurve object at 0x...>`, whose address changes from
  run to run (seen through the wheel in the p1-oracle review).
- **Who notices:** Python users who print a GradingRGBCurve.
- **A fix:** the repr its values give, like the other grading classes'.
- **Status:** to be matched in Phase 6 (D13).

## Undefined behaviour upstream

Out-of-bounds image layouts are decided: the port returns an error (D-2, approved on
2026-09-30). The general rule above covers the rest, except where an entry says otherwise.

### U-1. 10- and 12-bit values above their maximum

- **Upstream:** when the first op the optimizer leaves is a forward Lut1D, as the default
  optimization makes for integer input, the CPU looks up each R, G and B code in a table of
  1,024 (10-bit) or 4,096 (12-bit) entries without a bounds check (`CPUProcessor.cpp:140-146`,
  `ops/lut1d/Lut1DOpCPU.cpp:58-64, 635-650`). A 16-bit value above the maximum reads past the
  table: garbage, or a crash (it crashed the oracle). Alpha is scaled, not looked up
  (`Lut1DOpCPU.cpp:646`), and other processors convert codes with a multiply, which is well
  defined: a UINT10 red of 2000 with `OPTIMIZATION_NONE` gives 1023.
- **Options:** clamp to the largest code, ignore the extra bits, or return an error.
- **Status:** open; decided in Phase 2, with the Lut1D bake. The general rule doesn't apply
  here until then.

### U-2. `getAData()` without an alpha plane

- **Upstream:** in Python, `PlanarImageDesc.getAData()` on an image without an alpha plane
  returns uninitialized memory (seen through the wheel in O1.2).
- **Options:** return `None`, or an empty array.
- **Status:** open; decided in Phase 6.

### U-3. Very wide or very tall images

- **Upstream:**
  - On Windows, `4 * width` in `ScanlineHelper.cpp:72, 78, 104` overflows a 32-bit `long`: a
    width of 2^29 + 1 raises "vector too long", 2^30 raises "Invalid output image buffer.", and
    2^30 + 1 overruns the heap (a crash).
  - On both platforms, the row index is an `int` (`ScanlineHelper.h:92`), so an image of 2^31
    rows or more overflows it.
  - The rows are sized with `std::vector::resize` (`ScanlineHelper.cpp:69-81, 99-109`), which
    raises `std::length_error` past `max_size()` and `std::bad_alloc` for memory it can't have:
    through the Linux wheel, a planar F32 image of 2^60 × 1 pixels raises `ValueError`
    "vector::_M_default_append", and one of 2^32 × 1 under `ulimit -v 4000000` raises
    `MemoryError` "std::bad_alloc". Upstream sizes no row for an RGBA-packed F32 image in place,
    which it processes in its own memory.
- **Decided** (general rule): the port gives the wheel's messages where the wheel raises, and an
  error where it would overrun.
- **Status:** matched in `p1-bitdepth` (1.1e), in `crates/ocio-ops/src/scanline_helper.rs`:
  - where upstream's resize gets a negative size, `init` raises the C++ library's
    `std::length_error`: "vector too long" on Windows, "vector::_M_default_append" on Linux
    (where a C `long` wraps from a width of 2^61);
  - where upstream's RGBA row is empty and the source is packed channel by channel, the first
    row raises "Invalid output image buffer" (with a period for F32 sources);
  - where upstream would write outside its rows, the first row returns "ScanlineHelper Error:
    The image is too wide: 4 * width overflows the scanline buffers.";
  - after row 2^31 - 1 (Linux only: a Windows `long` can't count more rows), a source packed
    channel by channel raises "Invalid output image position.", as upstream's does, and an
    RGBA-packed one returns "ScanlineHelper Error: The image is too tall: the scanline index
    overflows.". With a y stride of 0, upstream reads the same row again instead, and never
    stops, since the negative index never reaches the height; the error is right there too;
  - the port sizes the rows upstream sizes, in its order, and raises `std::length_error` past
    libstdc++'s `max_size()` ("vector::_M_default_append" on Linux; a Windows `long` can't
    reach MSVC's, whose message is "vector too long") and `std::bad_alloc` where the memory
    can't be had ("std::bad_alloc" on Linux, "bad allocation" in MSVC's library, which both
    modules of the Windows wheel hold), instead of aborting. It sizes rows of its own only for
    the rows of RGBA-packed images that aren't aligned for their channel type, which upstream
    reads and writes in place; so there, and only there, it may raise `std::bad_alloc` where
    upstream wouldn't.
  The oracle refuses these sizes, so `scanline_helper_tests.rs` defines the behaviour.

### U-4. A Python logging function crashes the interpreter's exit

- **Upstream:** a logging function set from Python is held in a C++ global
  (`Logging.cpp:71`), which outlives the Python interpreter. A process that exits with one
  still set crashes (a segmentation fault on both platforms, seen through the wheel in
  `p1-foundations`); `ResetToDefaultLoggingFunction()` before exit avoids it, and the oracle's
  commands do so.
- **Options:** release the function when Python shuts down, or keep it and never release it;
  either way the process exits cleanly.
- **Status:** open; decided in Phase 6 (the Python module).

### U-5. A 1D LUT that doesn't fit its GPU texture width

- **Upstream:** a 1D LUT of L entries goes in a texture min(L, W) wide and L / W + 1 high, W being
  the description's texture width limit (4096 by default). In more than one row, each row's
  last entry is repeated at the start of the next (`ops/lut1d/Lut1DOpGPU.cpp:19-141, 153-177`).
  - A width of 0 divides by zero.
  - A width of 1 never advances along the LUT, and repeats entries forever.
  - Otherwise the padded entries can outnumber the texture's texels. The count of texels left
    to fill, an unsigned difference, then wraps, and the wheel appends entries until memory runs
    out. Through the Linux wheel, capped, that ends in `std::bad_alloc`; the Windows wheel grew
    to 23 GB before it was stopped.
- **Who notices:** GPU shaders of 1D LUTs that don't fit the width limit: 8191, 12286 or 12287
  entries at the default width (32,640 lengths up to 2^20 in all), and most lengths at small
  widths.
- **Options:** an error where the padding doesn't fit, or a layout that fits.
- **Status:** open; decided in Phase 2, with the Lut1D GPU writer. The oracle refuses these
  requests (`gpu_shader`, `_padding_fits`).

### U-6. Resource prefixes the Metal class wrapper reads past

- **Upstream:** in MSL, a class wrapper reads the shader's declarations back to build its class
  (`GpuShaderClassWrapper.cpp:285-372`), and their names start with the resource prefix.
  - After a line that starts with `texture` past white space, it takes the next line for the
    texture's sampler and reads from `find("sampler") + 7` (lines 330-335). Without `sampler`,
    that wraps past `npos` to 6, past the end of a shorter line. A line feed in the prefix cuts
    each declaration into lines, so this happens when the prefix's first segment between two
    line feeds is shorter than 6 bytes (it is the line after a texture's declaration), or when
    a segment after a line feed starts with `texture` past white space (the wrapper takes its
    line for a texture's declaration, and the line after the last declaration is empty).
  - The wrapper also passes the declarations' bytes to `std::isspace`, and the class name's
    first byte to `std::isdigit` (lines 157, 226, 307, 325, 333, 348). The C++ standard leaves
    them undefined for a non-ASCII byte, a negative `char`, but both wheels define them and give
    such a byte neither class, so there is no undefined behaviour there, and no D12 split:
    - Windows: the UCRT's `isspace` and `isdigit` return 0 below -1 in a single-byte locale
      (`ucrt/convert/_ctype.cpp:28-56`, Windows SDK 10.0.22000.0), and classify the byte
      through the code page in a multibyte one (`_isctype_l`).
    - Linux: glibc's `isspace` reads the locale's table, which covers -128 to 255. GCC inlines
      `isdigit` as `(unsigned)(c - '0') <= 9` (wheel-inspect, `generateClassWrapperHeader`).
    - Through `ctypes`, no byte from 0x80 to 0xFE gets either class: in the UCRT under the C,
      single-byte (874, 1251 to 1256) and multibyte (932, 936, 949, 950, UTF-8) locales, and in
      glibc under every locale of the Rocky Linux 9 image (C, POSIX, C.UTF-8). Python starts in
      the user's locale (`English_United States.1252` here) and in C.UTF-8 there.
  - The uid goes through `std::isalpha` and `std::isalnum` (`GPUProcessor.cpp:180-188`), but
    only in the `GpuShaderCreator` overload of `extractGpuShaderInfo`. Python takes the
    `GpuShaderDesc` overload (lines 151-155), which skips it: through Python, the uid changes
    nothing, not even the cache ID.
- **Who notices:** MSL shaders whose resource prefix holds line feeds like those; C++ callers of
  the creator overload with a non-ASCII uid. Other names, and every name in the other
  languages, are only written out, so they are well defined.
- **Decided** (general rule): the port returns an error where the wrapper would read past a
  line. `p1-gpu-infra` settles the scope: 1.7d for the wrapper, 1.7e for the uid. The uid's
  key reaches only the creator's `begin`, which does nothing in the one description there is,
  so it changes no output in C++ either: the port doesn't compute it (1.7e).
- **Status:** the wrapper's part matched in `p1-gpu-infra` (1.7d, U-10); the uid's key not
  ported (1.7e). The oracle refuses these MSL prefixes whatever the processor (`gpu_shader`,
  `_check_names`): being exact would need the declarations, which only the extraction makes.

### U-10. A texture declared without a sampler after it, in MSL

- **Upstream:** in MSL, the class wrapper reads the shader's declarations back to build its
  class (`GpuShaderClassWrapper.cpp:285-372`). It takes the line after each line that starts
  with `texture` for that texture's sampler, and reads the sampler's name from
  `find("sampler") + 7` (lines 330-335). When that line has no `sampler`, `npos + 7` wraps to 6,
  and a line shorter than 6 bytes is read past its end; `substr` then throws
  `std::out_of_range`. OCIO's own writers always declare a sampler after its texture, but the
  declarations also hold the code a caller adds (`addToParameterDeclareShaderCode`,
  `addToTextureDeclareShaderCode`, both in Python) and the resource prefix, whose line feeds cut
  them into lines (U-6). A texture declared last is enough when a line feed ends it: the next
  line is then empty. (Without that line feed, `std::getline` fails at the end of the text and
  leaves the texture's line in the buffer, which is long enough: I-34.)

  The error is `std::out_of_range`, not an OCIO `Exception`. Python sees an `IndexError`, whose
  message is the C++ library's, so it differs by platform:
  - through the Windows wheel, `invalid string position`;
  - through the Rocky Linux 9 wheel (the verifier's probe), `basic_string::substr: __pos (which
    is 6) > this->size() (which is 0)` for the declarations `"texture2d<float> t;\n"`, and
    `(which is 2)` for `"texture2d<float> t;\nab\n"`. `__pos` can exceed 6: before `substr`
    throws, the loop that skips spaces (line 333) reads the bytes past the line's end, stale
    ones of the string's buffer, and steps over any that are white space.

  It also escapes the `catch (const Exception &)` of `GPUProcessor.cpp:197-201`, so an
  extraction that meets it doesn't call the creator's `end()`.
- **Who notices:** MSL shaders whose added declaration code declares a texture without a sampler
  after it, or whose resource prefix holds line feeds like U-6's.
- **Decided** (general rule): the port returns an error exactly where the read would pass the
  line's end, and otherwise parses as upstream does: a line of 6 bytes is read up to its
  terminating NUL, and a non-ASCII byte is neither white space nor a digit, as in both wheels
  (U-6). This settles the class wrapper's part of U-6.
- **Status:** matched in `p1-gpu-infra` (1.7d): the port's error is an OCIO `Exception` with
  its own message. Phase 6 decides what Python raises there (an `IndexError`, as the wheel
  does, or the `Exception`).

### U-11. Texture values shorter than the texture

- **Upstream:** `addTexture` and `add3DTexture` take the values as a `const float *`, and copy
  the texture's float count from it (`CreateArray`, `GpuShader.cpp:24-37`), so a shorter buffer
  is read past its end. Python's binding checks the buffer's length first
  (`PyGpuShaderDesc.cpp:116-202`).
- **Who notices:** callers of the Rust API, which takes the values as a slice: a slice shorter
  than the texture.
- **Decided** (the owner, 2026-09-30: the general rule, with a clear message): after upstream's
  own checks (the width limit, the names, a size of 0), the port returns the error "The texture
  'NAME' needs N values, but only M were given.". A longer slice is read up to the count, as
  upstream reads the buffer.
- **Status:** matched in `p1-gpu-infra` (1.7c).

### U-12. Constant arrays written from a count and a pointer

- **Upstream:** `declareFloatArrayConst` and `declareIntArrayConst` take a count and a pointer
  (`int size, const float * v`) and write `size` values from it (`GpuShaderUtils.cpp:520-648`).
  Their callers pass a count kept apart from the values: the grading curves' `getNumKnots()`
  with `getKnotsArray()`, `getNumCoefs()` with `getCoefsArray()`
  (`ops/gradingrgbcurve/GradingRGBCurveOpGPU.cpp:227-230`,
  `ops/gradinghuecurve/GradingHueCurveOpGPU.cpp:267-270`), and ACES 2 its table's
  `total_size` with its data (`ops/fixedfunction/FixedFunctionOpGPU.cpp:864`). A count past
  the values reads past their end.
- **Who notices:** no one yet: none of the writers ported so far calls these helpers. The
  port's helpers take the values as a slice, so a writer ported later slices its values by its
  count, and a count past their end would panic there.
- **Decided** (general rule): a writer that ports one of these calls checks that the count
  fits before slicing, and returns an error where it doesn't. The helpers' doc comments say so
  (`crates/ocio-gpu/src/gpu_shader_utils.rs`).
- **Status:** the helpers ported in `p1-gpu-infra` (1.7a); each caller's check comes with its
  writer (2.4 for ACES 2, Phase 5 for the grading curves).

### U-15. A wrapped scanline reaches outside the image

- **Upstream:** on Windows, a scanline whose wrapped start (I-1) falls inside the image starts in
  the middle of a row, and its `width` pixels run past the row's end
  (`ImagePacking.cpp:65-85, 135-155, 208-228, 278-298`). For the last rows, and for right-to-left
  or bottom-up layouts, that is outside the image's memory: row 87,382 of a right-to-left UINT8
  plane of 49,152 × 87,383 pixels starts at pixel 32,768 of row 0, and is written up to 32,768
  bytes before the plane.
- **Decided** (general rule): the port returns an error instead, before reading or writing any
  pixel of the scanline: "ImagePacking Error: The image has too many pixels: the scanline's
  pixel index overflows." Scanlines that stay inside the buffers are read and written where
  upstream reads and writes them (I-1).
- **Status:** matched in `p1-bitdepth`, in `crates/ocio-ops/src/image_packing.rs`;
  `image_packing_tests.rs` checks it on small buffers, with that row's wrapped start on Windows.

### U-16. Queries of a 3x3 matrix before it is validated

- **Upstream:** a CLF or CTF file gives a Matrix op a 3x3 array, 9 values, which
  `MatrixArray::validate` turns into the canonical 4x4 form (`ops/matrix/MatrixOpData.cpp:
  413-436`). Before that, `MatrixOpData::hasAlpha` (and so `isIdentity` and `isNoOp`) reads
  the values at the 4x4 positions 3 to 15, `getCacheID` hashes 16 values
  (`MatrixOpData.cpp:529-614, 846-869`), and the op's `getCPUOp` builds a renderer from 16
  values (`GetMatrixRenderer`, `ops/matrix/MatrixOpCPU.cpp:400-428`): reads past the 9 values.
  `isIdentity` returns false first when the matrix has offsets (`MatrixOpData.cpp:534-539`),
  without the read. The processors validate their ops first, so only code that queries or
  renders such an op directly gets there.
- **Decided** (general rule): the port returns an error from those queries instead: "Matrix: a
  3x3 matrix has to be validated before this query: upstream reads past its 9 values."
  `MatrixOpData::{has_alpha, is_identity, is_no_op, get_cache_id}`, `get_matrix_renderer`,
  `OpData::{is_no_op, is_identity}`, `Op::{is_no_op, is_identity, get_cache_id, get_cpu_op,
  apply, apply_in_out}`, `OpVec::{is_no_op, get_cache_id}` and `serialize_op_vec` return
  `Result`s for it. `is_identity` and `is_no_op` answer false for a matrix with offsets, as
  upstream does.
- **Status:** matched in `p1-range` (before 1.3r1), the renderer in `p1-matrix`;
  `matrix_op_tests.rs` checks the errors, the answers with offsets, and that validating clears
  them.

### U-24. Queries of a Gamma op whose channels have too few parameters

- **Upstream:** a basic Gamma style uses one parameter per channel and a moncurve style two,
  but the setters take any number, and only `validate` checks it
  (`ops/gamma/GammaOpData.cpp:366-436`). Before that, the queries read the values they need
  without a check: `isIdentity` (and so `isNoOp`) reads red's first value when the four
  channels are equal, and a moncurve style's second one when the first is 1
  (`GammaOpData.cpp:28-38, 512-549`); `getCacheID` prints each channel's first value
  (`GammaOpData.cpp:40-50, 798-816`); `compose` reads the first value of each channel of both
  ops (`GammaOpData.cpp:707-745`). On an empty or too short vector, these read past its end.
  The processors validate their ops first, so only code that queries such an op directly gets
  there.
- **Decided** (general rule): the port returns an error from those queries instead, where
  upstream would read past the end and only there: "GammaOp: a channel has fewer parameters
  than its style uses: upstream reads past them." `GammaOpData::{is_identity, is_no_op,
  get_cache_id, compose}` return it.
- **Status:** matched in `p1-gamma` (1.3g1); `gamma_op_data_tests.rs` checks the errors, and
  that the reads upstream doesn't make (a moncurve gamma other than 1, channels that differ)
  are answered.
