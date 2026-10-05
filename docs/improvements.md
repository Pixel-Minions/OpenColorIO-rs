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
    `transforms/Lut3DTransform.cpp:174-218`). The Lut1DTransform's smallest and largest values
    are `std::min` and `std::max` from `FLT_MAX` and `-FLT_MAX`, which skip a NaN entry, so a
    channel of only NaNs prints `minrgb` 3.40282e+38 and `maxrgb` -3.40282e+38, whatever else
    the LUT holds (seen through the wheel);
  - it prints numbers with 9 significant digits, so two `MatrixTransform`s one ULP apart have
    the same text (seen through the wheel: the second gets the first one's processor, and with
    `PROCESSOR_CACHE_OFF` they differ).

  Two such transforms get the same processor from one config: the second applies the first.
- **Who notices:** applications that build transforms in code, not from files, and get several
  processors from one config.
- **A fix:** put every value, or a hash of it, in the key.
- **Status:** the cache and its key are matched (`p1-processor`, WP 1.8g); each class's text
  as the class lands. The Lut1DTransform's text, NaN channels included, is matched in
  `p1-transforms-fam4` and checked against the wheel in
  `crates/ocio/tests/lut1d_transform_oracle.rs` (the "NaNs only" case).

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

### I-53. Processor caches key by a hash, which differs between Windows and Linux

- **Upstream:** the config's cache of processors, and each processor's caches of optimized and
  CPU processors, key their entries by `std::hash<std::string>` of a text
  (`Config.cpp:4836-4841`, `Processor.cpp:410-413, 563-566` @ v2.5.2), not by the text. The
  hash is the C++ library's: FNV-1a on Windows (MSVC), `_Hash_bytes` with the seed
  `0xc70f6907` on Linux (libstdc++). So:
  - two texts with the same 64-bit hash share an entry: the second gets the first one's
    processor;
  - the config's fallback (`Config.cpp:4849-4873`) reuses the first cached processor with the
    same cache ID in the order of the keys, so when several cached processors share a cache ID
    (cached while `OCIO_DISABLE_CACHE_FALLBACK` was set), which one comes back differs between
    the platforms.
- **Who notices:** practically no one for a collision; for the fallback, applications that
  toggle `OCIO_DISABLE_CACHE_FALLBACK` and compare processors by identity.
- **A fix:** key by the text itself, and make the fallback's choice not depend on the hash.
- **Status:** matched (`p1-processor`, WP 1.8g: `caching::std_hash_string`), checked through
  the wheels' fallback (`crates/ocio/tests/processor_cache_oracle.rs`).

### I-54. `OCIO_OPTIMIZATION_FLAGS` reads differently on Windows and Linux

- **Upstream:** `EnvironmentOverride` reads the variable with `std::stoul(value, nullptr, 0)`
  (`Processor.cpp:354-374` @ v2.5.2), whose `unsigned long` is 32 bits on Windows and 64 bits
  on Linux, and whose messages are the C++ library's. So, between the wheels:
  - a value above 2^32 - 1 (`4294967296`) is an error on Windows ("Illegal value for
    OCIO_OPTIMIZATION_FLAGS: stoul argument out of range") and flags on Linux;
  - a negative value wraps at a different width (`-1` is `0xFFFFFFFF` on Windows, 2^64 - 1 on
    Linux), which changes the cache keys of the optimized and CPU processors;
  - `0x` with no hexadecimal digit after it is an error on Windows ("invalid stoul argument";
    the Windows C runtime converts nothing) and 0 on Linux (glibc converts the `0`);
  - the messages of the errors differ: "invalid stoul argument" and "stoul argument out of
    range" on Windows, "stoul" for both on Linux.
  Any text after the digits is ignored (`0x1Fzz` reads as 31).
- **Who notices:** users who set `OCIO_OPTIMIZATION_FLAGS` to a value out of the flags' range,
  or a malformed one.
- **A fix:** read the variable as a 32-bit value on both platforms, refuse trailing text, and
  give one message.
- **Status:** matched (`p1-processor`, WP 1.8h1: `processor::stoul`, checked against each C
  runtime's `strtoul`, and through both wheels in `crates/ocio/tests/processor_cache_oracle.rs`).

## Numeric helpers

### I-20. Double values are compared to 0 and 1 in float precision

- **Upstream:** `IsScalarEqualToZero<double>` and `IsScalarEqualToOne<double>` convert the value
  to float and allow 2 float ULPs (`MathUtils.cpp:17-39`). So:
  - a value with |x| ≤ 2.5·2⁻¹⁴⁹ (about 3.50e-45) counts as 0: a `LogAffineTransform` slope of
    2.1e-45 is refused as "cannot be 0", and 3.6e-45 is accepted;
  - a value in [1 − 2.5·2⁻²⁴, 1 + 2.5·2⁻²³] (about 1 − 1.49e-7 to 1 + 2.98e-7) counts as 1: a
    v1 exponent in that band is dropped as a no-op.

  `LogUtil::ValidateLegacyParams` (`ops/log/LogUtils.cpp:139-147`) compares a CTF Log's
  double gamma with the float `0.01f` (0.009999999776482582): a gamma equal to the float's
  value is refused, but the next double up is accepted, although it is below 0.01 and the
  message says it "should be greater than 0.01".
- **Who notices:** configs with tiny slopes, or with exponents and gains within 3e-7 of 1;
  CTF files with a legacy Log gamma within 2.3e-10 below 0.01.
- **A fix:** compare in double.
- **Status:** matched in `p1-math` (1.4a); the gamma in `p1-log` (1.3l1), `log_utils_oracle.rs`
  against the wheel.

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
    and so does the optimized processor's cache ID, which hashes them;
  - so do the ones it bakes from CDLTransforms (`powf`), at every one of those input depths:
    an ASC CDL and an inverse no-clamp one (the p1-optimizer verifier's probe of both wheels;
    at UINT8 the optimized cache ID is 4edb2d4f on Windows and 3210436f on Linux).

  Log, LogAffine, ExposureContrast, and the LUT and built-in bakes came out the same on both,
  over 3309 cases (the p1-oracle review's survey). LogCamera did too in that survey, but its
  break on the log side is computed differently on each platform, and other parameters show
  it (I-70).
- **Who notices:** anyone comparing GPU textures, SDR 2.0 shaders, or renders of exponents or
  CDLs at 8 to 16 bits between a Windows and a Linux machine.
- **A fix:** one math library on every platform, which changes the port's results on at least
  one of them.
- **Status:** the bake is matched in `p1-optimizer` (D, `OptimizeSeparablePrefix`): it renders the
  prefix with each op's renderer and the platform's math library, and
  `tests/lut1d_bake_oracle.rs` compares the baked LUTs with each platform's wheel, entry for
  entry. The ACES 2 tables are to be matched in Phase 2 (D12: the port calls the platform's
  functions, as OCIO does). The values may also depend on the CPU (SIMD renderers, glibc's
  ifunc variants), so their checks belong in `cpu-tests` (the bake's are there).

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

### I-52. The unknown Log style message is written over its own start

- **Upstream:** `LogUtil::ConvertStringToStyle` (`ops/log/LogUtils.cpp:54-59`) builds its
  error in `std::stringstream ss("Unknown Log style: '"); ss << str << "'.";`. A stream
  constructed with text starts writing at its beginning, so the style name and `'.` overwrite
  "Unknown Log style: '" instead of following it: "foo" gives "foo'.wn Log style: '", and a
  name of 18 characters or more replaces it all. `ConvertStyleToString`'s message for a value
  outside the enum (`LogUtils.cpp:87-90`) has the same bug, but a Rust enum can't hold such a
  value. The only caller, the CTF/CLF reader (`fileformats/ctf/CTFReaderHelper.cpp:3564-3571`),
  replaces the message with its own ("Required attribute 'style' 'foo' is invalid."), so no
  output shows it.
- **Who notices:** nobody through the library; code calling the function directly.
- **A fix:** `std::ostringstream ss; ss << "Unknown Log style: '" << str << "'.";`.
- **Status:** matched in `p1-log` (1.3l1); `log_utils.rs`, `overwritten`.

### I-55. Exponents that differ past 7 digits share a cache ID

- **Upstream:** an Exponent op's cache ID writes each exponent with 7 significant digits
  (`ExponentOpData::getCacheID`, `ops/exponent/ExponentOp.cpp:72-90`), and the processors'
  cache IDs are made of their ops'. Exponents that differ past the 7th digit give the same
  cache IDs though they give different pixels. Through the wheel, ExponentTransforms of
  `[2.0000001, 2, 2, 1]` and `[2.0000003, 2, 2, 1]` in a version 1 config share the
  processor's, the CPU processor's and the GPU processor's cache IDs (`<ExponentOp 2 2 2 1 >`);
  their CPU outputs differ (0.7 becomes 0.48999998 and 0.48999995), and so do their shaders
  (`vec4(2.0000000999999998, ...)` and `vec4(2.0000003, ...)`).
- **Who notices:** applications that cache processors or shaders by these cache IDs, with
  version 1 configs (where CDLs build Exponent ops too).
- **A fix:** write the exponents with all their digits (17), or hash them.
- **Status:** matched in `p1-exponent` (1.3e1), checked against the wheel in
  `crates/ocio-ops/tests/exponent_oracle.rs`.

### I-56. A tiny exponent can't be inverted

- **Upstream:** inverting an Exponent op refuses an exponent that is 0, "Cannot apply
  ExponentOp op, Cannot apply 0.0 exponent in the inverse." (`CreateExponentOp`,
  `ops/exponent/ExponentOp.cpp:307-337`). The test is `IsScalarEqualToZero`, which converts the
  `double` to `float` and compares within 2 ULPs (`MathUtils.cpp:17-27`), so a nonzero exponent
  up to 2 float ULPs (about 2.8e-45), or one that underflows a float, is refused too, though
  `1.0 / e` is finite. Through the wheel, the inverses of exponents of 1e-46 and 1e-300 are
  refused, and 5e-45 is inverted (2e+44). A NaN exponent isn't 0 and is inverted to NaN.
- **Who notices:** inverse ExponentTransforms (version 1 configs) and inverse CDLs with such
  powers.
- **A fix:** test the `double` against 0.
- **Status:** matched in `p1-exponent` (1.3e1), checked against the wheel in
  `crates/ocio-ops/tests/exponent_oracle.rs`.

### I-60. The mirror Gamma styles give a negative NaN pixel a different sign per platform

- **Upstream:** without fast math, `GammaBasicMirrorOpCPU::apply` and the two
  `GammaMoncurveMirrorOpCPU` renderers compute `std::copysign(1.0f, in) * value`
  (`ops/gamma/GammaOpCPU.cpp:394-414, 707-741, 803-838`), where `value` comes from `|in|`, so
  for a NaN pixel it is a positive NaN. MSVC builds `±1.0f` and multiplies (Windows wheel
  `0x1801bd3d0`, the moncurve mirror loop at `0x1801bde60`): the NaN keeps its positive sign.
  GCC turns the product into its `xorsign` pattern, `value ^ signbit(in)` (Linux wheel
  `GammaBasicMirrorOpCPU::apply` at `0x384be0`, `GammaMoncurveMirrorOpCPUFwd::apply` at
  `0x384d30`, `...Rev::apply` at `0x385030`): the NaN takes the input's sign, except in the
  alpha channel of the two moncurve mirror renderers, where GCC builds `±1.0f` and multiplies
  too (`docs/spikes/s2-s5.md`, "Windows and Linux differences" 2). Every value other than a
  NaN gives the same bits on both. The fast-math renderers OR the sign bit back on both
  platforms.
- **Who notices:** images with negative NaNs (the sign bit set) through an `ExponentTransform`
  or `ExponentWithLinearTransform` of the mirror style, with ordinary parameters and fast math
  off: the output NaN is positive on Windows, negative on Linux (but positive in a moncurve
  mirror's alpha).
- **A fix:** one rule for every platform and channel, e.g. always the input's sign.
- **Status:** matched in `p1-gamma` (S2, 1.3g2): `gamma_op_cpu.rs` reproduces each wheel per
  renderer and channel (`BASIC_MIRROR_SIGN`, `MONCURVE_MIRROR_SIGN`), and the battery's NaN
  probes compare it bit for bit on both platforms.

### I-61. With fast math, a no-clamp CDL lets an infinite or NaN alpha change the colour

- **Upstream:** the fast-math CDL renderers process a pixel as one four-lane vector, alpha
  included, and the saturation's luma sums all four lanes, alpha's with a weight of 0
  (`ops/cdl/CDLOpCPU.cpp:104, 157-171`). The clamping styles clamp the lanes to [0, 1] before
  the luma, so alpha's lane is a number; the no-clamp styles don't. Forward
  (`CDLRendererFwdSSE<false>`, `CDLOpCPU.cpp:346-376`), an infinite alpha passes the power
  step, `inf * 0` makes the luma NaN, and red, green and blue come out NaN. Reverse
  (`CDLRendererRevSSE<false>`, `CDLOpCPU.cpp:409-440`), the saturation comes first, so an
  infinite or NaN alpha makes the luma and every channel NaN; the power step then turns the
  NaNs into 0, and the output is `-offset / slope` whatever the colour. The scalar renderers
  (fast math off) never read alpha. Seen through the wheel, the same on both platforms, for
  CDL_DATA_1 (`tests/cpu/ops/cdl/CDLOp_tests.cpp:60-66`) in the no-clamp style and the pixel
  `(0.5, 0.5, 0.5, A)`: forward, `(0.80900, 0.38568, 0.0032947)` without fast math for any
  `A`, but NaN in every channel at `OPTIMIZATION_DEFAULT` for `A = ±inf`; inverse,
  `(0.31451, 0.59543, 6.61108)` without fast math, but `(-0.037037, 0.209091, -1.549296)` at
  `OPTIMIZATION_DEFAULT` for `A = ±inf` or NaN.
- **Who notices:** images with infinite (or, inverse, NaN) alpha through a no-clamp
  `CDLTransform` whose power isn't 1, at the default optimization: the colour is lost, and it
  differs from the result without fast math.
- **A fix:** leave alpha's lane out of the luma (a 0 lane before the multiply, or a three-lane
  sum).
- **Status:** matched in `p1-cdl` (1.3c2): `cdl_op_cpu.rs` computes alpha's lane, and the
  battery's probes (infinite and NaN alphas) compare it bit for bit on both platforms.

### I-62. A CDL with a power of 1 depends on the optimization flags

- **Upstream:** with `OPTIMIZATION_SIMPLIFY_OPS` (part of `OPTIMIZATION_LOSSLESS`, `_DEFAULT`,
  `_GOOD` and `_ALL`; `OpOptimizers.cpp:673`), a CDL op whose power is 1 is replaced with a
  slope and offset matrix, a saturation matrix and clamps (`CDLOpData::getSimplerReplacement`,
  `ops/cdl/CDLOpData.cpp:325-394`); an inverse one with the matrices inverted exactly
  (`MatrixOpData`'s inverse). The CDL renderers instead floor a reverse slope, power and
  saturation at 0.01 before taking the reciprocal (`Reciprocal`, `ops/cdl/CDLOpCPU.cpp:18-23,
  62-99`). So an inverse CDL with a power of 1 and a slope or saturation of 0 fails with
  "Singular Matrix can't be inverted." (`ops/matrix/MatrixOpData.cpp:218, 259`) under those
  flags, but renders under `OPTIMIZATION_NONE`, `_IDENTITY` or `_PAIR_IDENTITY_CDL`; and a
  slope or saturation below 0.01, such as 0.005, divides by 0.005 (200) in the matrices but by
  0.01 (100) in the renderer.
- **Who notices:** inverse CDLs with a power of 1 and a zero or tiny slope or saturation: the
  processor refuses them, or renders them differently, depending on the optimization flags.
- **A fix:** floor the reciprocals in the simplified matrices as the renderer does (or not in
  either), and give the zero cases one outcome.
- **Status:** matched in `p1-cdl` (1.3c1, 1.3c3); `cdl_op_oracle.rs` checks the zero and 0.005
  cases against the wheel under every flag setting.

### I-63. The CDL's validation messages misname their bounds and their class

- **Upstream:** `validateGreaterEqual` refuses a slope or saturation below 0 with "CDL:
  Invalid 'slope' -0.9 should be greater than 0.", though 0 is accepted ("or equal to" is
  missing), and `validateGreaterThan` refuses a power with the prefix "CDLOpData: Invalid
  'power'" where the other two say "CDL:" (`ops/cdl/CDLOpData.cpp:221-253`).
- **Who notices:** anyone who reads the messages.
- **A fix:** "should be greater than or equal to 0." for the slope and saturation, and one
  prefix for all three.
- **Status:** matched in `p1-cdl` (1.3c1), checked against the wheel in
  `crates/ocio-ops/tests/cdl_op_data_oracle.rs` and the battery.

### I-68. Two half-domain 1D LUTs are never equal

- **Upstream:** `Lut1DTransform::setLength` fills a half-domain LUT with each half code's
  value, NaN codes included ("Use NaNs for the 2048 NaN values in the domain.",
  `transforms/Lut1DTransform.cpp:101-106`). `Lut1DOpData::equals` compares the values with
  `std::vector<float>::operator==` (`ops/lut1d/Lut1DOpData.cpp:528-550`, `ops/OpArray.h:182-188`),
  where a NaN equals nothing. So two half-domain LUTs with the same values are unequal, unless
  they are the same object, and so are the `Lut1DTransform`s that hold them; seen through the
  wheel. `isInverse` uses the same comparison, so the optimizer never removes such a pair of
  inverse LUTs as an identity.
- **Who notices:** code that compares half-domain `Lut1DTransform`s, and processors with a
  half-domain LUT followed by its inverse.
- **A fix:** compare the values bit for bit, or fill the NaN codes with a value that compares
  (as the lookup domains do, with `filterNANs`).
- **Status:** matched in `p1-optimizer` (chunk A); `lut1d_op_data_oracle.rs` checks equality
  against the wheel, and `crates/ocio/tests/lut1d_transform_oracle.rs` checks it on the
  `Lut1DTransform`s (each half-domain case against a copy of itself, `p1-transforms-fam4`).

### I-70. A camera log's break differs between Windows and Linux

- **Upstream:** `LogUtil::GetLogSideBreak` (`ops/log/LogUtils.cpp:270-281`) computes the
  break on the log side of a LogCameraTransform as `float logSideBreak =
  log2((float)(...)); logSideBreak *= (float)logSlope / log2((float)base);`. The wheels
  compile it differently:
  - Windows (MSVC) calls `log2f` and computes every step in `float`;
  - Linux (GCC, libstdc++) calls `double log2(double)` on the promoted arguments, so the
    quotient is a `double` and `*=` multiplies in `double` before rounding back to `float`.
    It links `log2@GLIBC_2.2.5`, whose compatibility wrapper returns a positive NaN for a
    negative argument, where the UCRT's `log2f` returns the negative x86 default NaN.

  Finite, valid parameters then give breaks one ULP apart: the "per channel, base 10" case
  of `log_oracle.rs` gets 0x3e16fa6b on Windows and 0x3e16fa6c on Linux on its red channel,
  and differs on all three channels. A break whose argument `linSlope * linBreak + linOffset`
  is negative is a NaN of each platform's sign. The break feeds the offset of the linear
  segment (`GetLinearOffset`) and the inverse's choice of segment, so the pixels at and
  below the break differ, and so do the NaNs the linear segment produces.
  `LogUtil::GetLinearSlope` (`LogUtils.cpp:255-268`), when no linear slope is set, also
  multiplies its numerator `logSlope * linSlope` in a different order on each: MSVC computes
  `linSlope * logSlope` (0x18021abde), GCC the source's order (0x40176d). With both slopes NaN
  the slope keeps the linear side's NaN on Windows and the log side's on Linux; with NaNs of
  opposite signs (reachable through the API's setters, not through a config), the renderers'
  NaNs and the shader's literal of the linear segment's slope (`linear_segment_slope`, or
  `linear_segment_slopeinv` in the inverse) differ in sign between the platforms.
- **Who notices:** anyone comparing renders of a LogCameraTransform, or a camera-style CTF
  Log, between a Windows and a Linux machine; ARRI LogC3 (EI 800) happens to give the same
  break on both.
- **A fix:** one computation and one `log2` on every platform (`log2f` in `float`, say),
  which changes the port's results on at least one of them.
- **Status:** matched in `p1-log` (1.3l1, the S2 spike's variants); `log_utils.rs`,
  `get_log_side_break_msvc` and `get_log_side_break_libstdcxx`; `log_oracle.rs`,
  `camera_cases_distinguish_the_log_side_break_variants` and the camera battery, on both
  platforms. The GPU writer (1.3l4) writes the break (`log_break`) and the linear segment's
  offset (`linear_segment_offset`) as `float` literals, so a camera log's shader text
  differs between the platforms too: `crates/ocio-gpu/tests/log_op_gpu_oracle.rs` compares
  it with the wheel live on each. The linear slope's numerator: `get_linear_slope_msvc` and
  `get_linear_slope_libstdcxx` (p1-gpu-ops4, from the review of 1.3l4), read from both wheels'
  machine code; the wheel can't be given NaNs of opposite signs until the specs carry a
  double's bits, so no oracle test checks them yet.

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

### I-73. A matrix transform's text changes how a group prints what follows it

- **Upstream:** `operator<<(std::ostream &, const MatrixTransform &)` sets the stream's
  precision to 16 and leaves it there (`transforms/MatrixTransform.cpp:348`). A group prints its
  children on one stream (`transforms/GroupTransform.cpp:156-169`), so every transform after a
  MatrixTransform prints its numbers with 16 significant digits instead of the default 6: in a
  group of a RangeTransform, a MatrixTransform and the same RangeTransform, the wheel prints
  `minInValue=0.123457` the first time and `minInValue=0.1234567891234` the second.
- **Who notices:** anyone reading `repr()` of a group, or comparing the texts of groups that
  hold the same transforms in another order.
- **A fix:** restore the stream's precision at the end of the MatrixTransform's text.
- **Status:** matched in `p1-transforms-fam4` (1.8b): the transforms write their text on one
  stream (`Transform::write_text`), checked against the wheel in
  `crates/ocio/tests/matrix_transform_oracle.rs` and, with the transforms that print numbers,
  in their own oracle tests.

### I-74. The matrix transform's static functions order their NaNs per platform

- **Upstream:** `MatrixTransform::Fit`, `Sat` and `View` (`transforms/MatrixTransform.cpp:
  162-334`) combine their arguments with products and sums. Where two NaNs meet, x86 keeps the
  first operand's, and the wheels' compilers ordered the operands differently: in `Fit`'s
  offsets, `newmin * oldmax - newmax * oldmin`, the Windows wheel multiplies in the source's
  order and the Linux wheel computes `oldmax * newmin`; in `Sat`, `(1 - sat) * luma`, Windows
  multiplies in the source's order and Linux with the luma first, except for the last
  diagonal value; in `View`, `values[0] + values[1] + values[2]`, Windows adds the first two
  in the other order. Seen through each wheel's `MatrixTransform.Fit`, `Sat` and `View`.
  The ops that use `Fit` and `Sat` (an AllocationTransform's, a version 1 CDL's saturation)
  pass them constants with the one variable, so no two NaNs meet there.
- **Who notices:** matrices built from NaN arguments of different signs or payloads: their
  NaNs, in the cache IDs, the pixels and the shaders, differ between Windows and Linux.
- **A fix:** one operand order for both platforms.
- **Status:** matched in `p1-transforms-fam4` (1.8b), each wheel's order, checked in
  `crates/ocio/tests/matrix_transform_oracle.rs`.

### I-75. A range that doesn't clamp names its class twice when it lacks a bound

- **Upstream:** `RangeTransformImpl::validate` throws "RangeTransform validation failed:
  non clamping range must have min and max values defined." for a range that doesn't clamp
  and lacks a bound, inside the `try` whose `catch` prefixes every message with
  "RangeTransform validation failed: " (`transforms/RangeTransform.cpp:52-73`). The message
  comes out with the prefix twice.
- **Who notices:** anyone who reads the message.
- **A fix:** throw the message without its prefix.
- **Status:** matched in `p1-transforms-fam4` (1.8c), checked against the wheel in
  `crates/ocio/tests/range_transform_oracle.rs`.

### I-76. An allocation without variables doesn't print its allocation

- **Upstream:** `operator<<(std::ostream &, const AllocationTransform &)` prints the allocation
  only together with the variables, when there are some
  (`transforms/AllocationTransform.cpp:159-183`). Without variables, a uniform and a log2
  allocation (and an unknown one) both print `<AllocationTransform direction=forward>`.
- **Who notices:** anyone reading `repr()` of such a transform, which hides how it allocates.
- **A fix:** print the allocation always.
- **Status:** matched in `p1-transforms-fam4`, checked against the wheel in
  `crates/ocio/tests/allocation_transform_oracle.rs`.

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
  `vec4(inf, -inf, 0., 0.)`; NaN parameters give `nan`. In Cg, ±inf and values beyond the
  half range are clamped to ±65504 first (e.g. `half4(65504., -65504., 0., 0.)`), so those
  shaders compile; only `nan` stays invalid.
- **Who notices:** shaders for transforms with NaN or infinite parameters, or diagonal matrices
  and offsets beyond the float range: the shader doesn't compile. The CPU renders them.
- **A fix:** write non-finite values in a form each language accepts (`1.0/0.0`,
  `uintBitsToFloat(...)`), or refuse such parameters on the GPU.
- **Status:** matched in `p1-gpu-ops` (1.3m4), checked against the wheel in
  `crates/ocio-gpu/tests/matrix_op_gpu_oracle.rs` (`extreme_parameters_write_the_wheels_shader`).
  The Gamma writer (1.3g4) writes a NaN parameter, which its validation lets through, as
  a NaN literal too: `crates/ocio-gpu/tests/gamma_op_gpu_oracle.rs`. Valid, finite
  moncurve parameters can still give `inf`, since `ComputeParamsRev` narrows the reverse
  slope to `float` (`ops/gamma/GammaOpUtils.cpp:90-97, 119-127`): a gamma above about 7.6
  with an offset of 0, e.g. `ExponentWithLinearTransform([10, 2.4, 2.4, 1],
  [0, 0.055, 0.055, 0], NEGATIVE_LINEAR, INVERSE)`, gives
  `vec4 slope = vec4(inf, 12.9232101, ...)`; the same test checks it.
  The Range writer (1.3r3): `RangeOpData::validate` accepts infinite bounds, and the scale
  and offset it computes from them are infinite or NaN. Through the wheel, in GLSL 4.0, a
  maxOut of inf gives `outColor.rgb * vec3(inf, inf, inf) + vec3(-inf, -inf, -inf)` and
  `min(vec3(inf, inf, inf), ...)`; a minIn of -inf gives an offset of `-nan(ind)` on
  Windows: `crates/ocio-gpu/tests/range_op_gpu_oracle.rs`
  (`extreme_bounds_write_the_wheels_shader`). The CDL writer (1.3c4) writes the renderers'
  `float` parameters, which its validation lets be NaN or infinite, and which a `double`
  beyond `FLT_MAX` overflows: through the wheel, a slope of 1e39 and an offset of NaN give
  `vec3 slope = vec3(inf, ...)` and `vec3 offset = vec3(nan, ...)`; an infinite saturation
  goes through `declareVar`, which writes `3.40282347e+38` instead:
  `crates/ocio-gpu/tests/cdl_op_gpu_oracle.rs` (`extreme_parameters_write_the_wheels_shader`).
  The Log writer (1.3l4) writes some parameters as `double`s and others as `float`s it
  computes from them, and its validation lets the base and the parameters be NaN or
  infinite. Through the wheel, in GLSL 4.0, an affine log in base 10 with a log side slope
  and a linear side slope of 1e39 gives `vec3 lin_slope = vec3(9.9999999999999994e+38., ...)`
  and `vec3 log_slope = vec3(inf, ...)` (the slope divided by `log(base)` in `double`, then
  narrowed), and its inverse `vec3 log_slopeinv = vec3(0., ...)`; a `LogTransform` with a
  NaN base gives `vec3 log_slope = vec3(nan, nan, nan)`:
  `crates/ocio-gpu/tests/log_op_gpu_oracle.rs` (`extreme_parameters_write_the_wheels_shader`).

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

## Platform, environment and paths

### I-110. `splitext` compares the rest of the name with "."

- **Upstream:** pystring's `splitext_generic`, which OCIO calls through `os::path::splitext` to
  find a file's extension, skips a name's leading dots by comparing `slice(p, filenameIndex)`,
  the rest of the path, with "." (pystring.cpp:1597-1608 @ v1.1.4), where Python's
  `ntpath._splitext` compares the one character `p[filenameIndex]`. So a name that starts with
  dots and has another dot later (`..b`, `.a.` after a separator) is split at its last dot, where
  Python leaves it whole.
- **Who notices:** file names made of leading dots and an extension, such as `..cube`, whose
  extension Python's `splitext` wouldn't find.
- **A fix:** compare the one byte, as Python does.
- **Status:** matched in p3-context (3.5b, `crates/ocio-ops/src/utils/pystring.rs`).

### I-111. A context's copy forgets its environment mode

- **Upstream:** `Context::createEditableCopy` copies the context through `Context::Impl::
  operator=` (`Context.cpp:60-80, 156-161` @ v2.5.2), which copies the search paths, working
  directory, variables, caches, cache ID and I/O proxy, but not `m_envmode`: the copy has the
  default, `ENV_ENVIRONMENT_LOAD_PREDEFINED`. The copied cache ID was computed with the original
  mode, so the copy's `getCacheID` describes a mode it doesn't have until a setter clears it.
- **Who notices:** code that copies a context in `ENV_ENVIRONMENT_LOAD_ALL` mode (and a config's
  copy, which copies its context) and then calls `loadEnvironment`: the copy updates its own
  variables instead of loading all of them.
- **A fix:** copy `m_envmode` in `operator=`.
- **Status:** matched in p3-context (3.5e, `crates/ocio/src/context.rs`, `Clone for Context`);
  checked against the wheel (`crates/ocio/tests/context_oracle.rs`, "environment").

### I-112. Temporary file names

- **Upstream:** `Platform::CreateTempFilename` names a file `/tmp/ocio_<n>` on Linux, `<n>` drawn
  with `std::uniform_int_distribution<int>` from a default-seeded `std::mt19937`, and takes
  `tmpnam_s`'s name on Windows (`<temp dir>\u<id>.<k>`) (`Platform.cpp:210-259` @ v2.5.2). Only
  upstream's tests call it.
- **Who notices:** nobody through the API: no library code calls it.
- **A fix:** none needed. The port keeps the forms (`/tmp/ocio_<n>`, a name in the temporary
  directory) with its own numbers: the distribution's algorithm is the C++ library's, which the
  standard leaves open, and `tmpnam_s`'s names come from the UCRT.
- **Status:** not matched, by design (p3-context 3.5a, `crates/ocio-ops/src/platform.rs`).

### I-113. Windows converts names, values and paths through UTF-16

- **Upstream:** on Windows, the wheel converts environment variable names and values, and file
  paths, between UTF-8 and UTF-16 with `MultiByteToWideChar` and `WideCharToMultiByte`
  (`Platform.cpp:48-142, 261-322, 333-357` @ v2.5.2). Bytes that aren't UTF-8 become U+FFFD,
  with Windows' own rule (a lead byte and a continuation byte outside its range are one
  replacement), and an unpaired surrogate in the environment becomes U+FFFD. Linux passes the
  bytes. Names compare without case on Windows (`GetEnvironmentVariableW`), exactly on Linux;
  and setting a variable to "" removes it on Windows, keeps it empty on Linux.
- **Who notices:** configs and environments with names or paths that aren't UTF-8, or that
  differ only in case, used on both platforms.
- **A fix:** none: these are the platforms' rules.
- **Status:** matched in p3-context (3.5a): the conversions checked against the system for every
  string of up to 4 bytes or units over each class (`crates/ocio-ops/tests/platform_crt.rs`).

### I-114. Windows device names as files

- **Upstream:** `CreateFileContentHash` asks `_wstat` whether a file exists. On Windows it finds
  some device names, in any directory that exists: `nul`, `aux`, `com1`, `conin$`, `nul:` (with
  `st_dev` -1), but not `con` or `lpt1` (as probed on Windows 11).
- **Who notices:** a config whose file references name such devices; `resolveFileLocation` then
  finds them.
- **A fix:** treat device names as missing on Windows, as the port does.
- **Status:** not matched (p3-context 3.5a): the port asks `std::fs::metadata`, which finds no
  device. For the owner to decide, with D-5's kind of limitation.

### I-115. pystring's indices are `int`

- **Upstream:** pystring computes positions and lengths as `int` (`(int) str.size()` and the
  `ADJUST_INDICES` arithmetic, pystring.cpp @ v1.1.4), so for a path of 2^31 bytes or more the
  length wraps.
- **Who notices:** nobody in practice: OCIO passes it file paths and names.
- **A fix:** none needed. The port computes with the true lengths (`i64`).
- **Status:** not matched (p3-context 3.5b), for the owner: matching would mean computing every
  pystring index as a wrapping `int`, whose results then index outside the string (undefined
  behaviour, which the general rule turns into errors), for inputs no one passes.

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
  `applyRGB` and `applyRGBA` look up the pixel's own bytes as codes (I-41), so with 10- or
  12-bit input they read past the table whenever those bytes exceed the maximum.
- **Decided** (owner, 2026-10-01): the port returns an error: "Lut1D: a 10ui value above 1023
  can't be looked up: upstream reads past the 1D LUT's 1024 entries." (and the 12-bit one).
  The CPU processor checks the codes before the lookup (`CpuOp::check_input`), and
  `CpuProcessor::apply_rgb` and `apply_rgba` return a `Result` for it (an API change the
  owner approved), leaving the pixel as it was.
- **Future improvement** (owner, 2026-10-01): a candidate for the end-of-port review, which
  picks one of the alternatives considered: clamp the code to the largest one, or mask the
  extra high bits.
- **Status:** matched with an error in `p1-optimizer` (chunk C); `lut1d_op_cpu_tests.rs`
  checks the errors.

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

### U-20. A Log op's data with short channels

- **Upstream:** `LogOpData`'s constructor from three parameter vectors accepts channels that
  all have fewer than 4 parameters (`ops/log/LogOpData.cpp:86-107`), and `setRedParams`,
  `setGreenParams` and `setBlueParams` set any vector. Only upstream's tests and the CTF/CLF
  reader use them, and validation refuses both cases ("Log: expecting at least 4
  parameters.", "Log: Red, green & blue parameters must have the same size."), but
  `CreateLogOp` doesn't validate a forward op's data (`ops/log/LogOp.cpp:139-150`). Where the
  data isn't validated:
  - `setValue` writes past a channel too short for the parameter (`LogOpData.cpp:120-148`);
  - `getValue`, and so `getParameters`, reads past a green or blue channel shorter than the
    red one (`LogOpData.cpp:160-196`);
  - `getIdentityReplacement` reads the red channel's linear offset and slope
    (`LogOpData.cpp:268-277`);
  - the parameter strings of the cache ID read past a green or blue channel shorter than the
    red one (`LogOpData.cpp:389-414`);
  - the renderers' `updateData` reads the first 4 parameters of each channel, and the first 5
    for the camera style (`Log2LinRenderer`, `Lin2LogRenderer` and `CameraL2LBaseRenderer`,
    `ops/log/LogOpCPU.cpp:530-545, 630-645, 728-742`), so `LogOp::getCPUOp` and `Op::apply`
    read past a shorter channel.
- **Decided** (general rule): the port returns an error instead: "Log: the channels have
  fewer parameters than this needs: upstream accesses past them." from
  `LogOpData::{set_value, value, get_parameters, get_identity_replacement}`, the parameter
  strings (so `get_cache_id`) and `get_log_renderer` (so `Op::{get_cpu_op, apply,
  apply_in_out}`). `get_parameters` leaves an array the red channel has no parameter for as
  it is, as upstream's does, and raises for one the red channel has and the green or blue
  one doesn't, after setting the arrays before it. A CPU processor is never affected: its
  `finalize` validates the ops.
- **Status:** matched in `p1-log` (1.3l1, and the verifier's fixes);
  `log_op_data_tests.rs`, `short_channels_are_errors`; `log_op_tests.rs`,
  `renderers_of_short_channels_raise`. The transforms in `p1-transforms-fam4`: upstream's
  `CreateLogTransform` copies such data into a log affine or log camera transform, whose
  getters and setters read past the short channels; the port's `create_log_transform` (so
  `CreateTransform`, which the processors' `createGroupTransform` calls) returns the error
  and adds no transform, for a channel too short for the four affine parameters, or for a
  camera's break or linear slope (`log_transform_tests.rs`, `short_channels_are_refused`).

### U-24. Queries of a Gamma op whose channels have too few parameters

- **Upstream:** a basic Gamma style uses one parameter per channel and a moncurve style two,
  but the setters take any number, and only `validate` checks it
  (`ops/gamma/GammaOpData.cpp:366-436`). Before that, the queries read the values they need
  without a check: `isIdentity` (and so `isNoOp`) reads red's first value when the four
  channels are equal, and a moncurve style's second one when the first is 1
  (`GammaOpData.cpp:28-38, 512-549`); `getCacheID` prints each channel's first value
  (`GammaOpData.cpp:40-50, 798-816`); `compose` reads the first value of each channel of both
  ops (`GammaOpData.cpp:707-745`); the CPU renderers read each channel's first value, and a
  moncurve style's second (`ops/gamma/GammaOpCPU.cpp:295-318`, `GammaOpUtils.cpp:32-120`). On
  an empty or too short vector, these read past its end. The processors validate their ops
  first, so only code that queries such an op directly gets there.
- **Decided** (general rule): the port returns an error from those queries instead, where
  upstream would read past the end and only there: "GammaOp: a channel has fewer parameters
  than its style uses: upstream reads past them." `GammaOpData::{is_identity, is_no_op,
  get_cache_id, compose}`, `get_gamma_renderer` and `compute_params_fwd`/`_rev` return it,
  and so do `Op::{is_no_op, is_identity, get_cache_id, get_cpu_op}` for a Gamma op (1.3g3),
  which `CreateGammaOp` doesn't validate either. The GPU writer reads the same values in
  each style's block (`ops/gamma/GammaOpGPU.cpp:16-314`): `get_gamma_gpu_shader_program`
  (`ocio-gpu`, 1.3g4) returns the error too.
- **Status:** matched in `p1-gamma` (1.3g1, the renderers in 1.3g2, the op in 1.3g3,
  `compute_params_fwd`/`_rev` after it); `gamma_op_data_tests.rs`, `gamma_op_cpu_tests.rs`,
  `gamma_op_utils_tests.rs` and `gamma_op_tests.rs` check the errors,
  and that the reads upstream doesn't make (a moncurve gamma other than 1, channels that
  differ) are answered. The GPU writer in `p1-gpu-gamma` (1.3g4): `gamma_op_gpu.rs`'s test
  checks the error for each style and channel. The transforms in `p1-transforms-fam4`:
  upstream's `CreateGammaTransform` copies such data into an exponent or exponent with linear
  transform, whose getters, setters and text read each channel's first parameter; the port's
  `create_gamma_transform` (so `CreateTransform`) returns the error for an empty channel and
  adds no transform (`exponent_with_linear_transform_tests.rs`, `empty_channels_are_refused`).
  checks the error for each style and channel.

### U-27. A config's processor cache disabled while a thread uses it

- **Upstream:** `Config::getProcessor` checks `m_processorCache.isEnabled()` before it takes the
  cache's lock (`Config.cpp:4830-4832`), and `Config::setProcessorCacheFlags` is `const` and
  doesn't take that lock (`Config.cpp:929-933`). When another thread disables the cache
  (`PROCESSOR_CACHE_OFF`) between the check and `m_processorCache[key]`, `operator[]` checks
  again and returns a function-static `dummy` entry (`Caching.h:71-77`), one per cache type,
  shared by every config: the call reads and writes it unlocked against other threads, a data
  race, and can return a processor that another call left there, of another transform. Reading
  `m_enabled` while another thread writes it is a data race too. A processor's own caches can't
  get there: its flags are set before it is shared.
- **Who notices:** applications that change a config's cache flags while other threads get
  processors from it.
- **Decided** (general rule): the port's `GenericCache::lock` checks that the cache is enabled
  under the lock and gives no entries otherwise, and `processor_with_context` then makes an
  uncached processor of the transform it was given.
- **Status:** matched in `p1-processor` (`caching.rs`, `config.rs`), found by its verifier.
