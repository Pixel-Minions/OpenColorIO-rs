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
  on Linux. On Windows, the non-packed paths fail on images of 2^31 pixels or more (about
  46,000 × 46,000) with "Invalid output image position." (`ImagePacking.cpp:35-39, 105-109`,
  `ScanlineHelper.cpp` @ v2.5.2). The failure can come after the first rows are written:
  through the Windows wheel, a planar F32 image of 65,536 × 65,537 pixels processes its first
  row and then raises. Linux processes all of them (2^32 pixels of 65,536 × 65,536 in 17 s).
- **Who notices:** applications that process single images of over 2 gigapixels on Windows.
- **A fix:** 64-bit sizes on every platform, so those images work on Windows too.
- **Status:** to be matched in `p1-bitdepth` (1.1d): the message and every buffer byte,
  including the partly written output. The owner chose to match it on 2026-09-30.

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
  ID on each platform.
- **Who notices:** anyone comparing text or cache IDs across platforms for transforms with NaN
  parameters.
- **A fix:** one spelling on both platforms.
- **Status:** matched in `cfmt` (WP 0.5); each op's text uses it as the op lands (D12).

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

### I-32. `repr()` of a GradingRGBCurve prints an address

- **Upstream:** the binding gives GradingHueCurve's class its repr twice, and GradingRGBCurve's
  none: `PyGradingData.cpp:470` calls `defRepr(clsGradingHueCurve)` where `clsGradingRGBCurve`
  was meant. So a GradingRGBCurve's `repr()` is pybind11's default,
  `<PyOpenColorIO.PyOpenColorIO.GradingRGBCurve object at 0x...>`, whose address changes from
  run to run (seen through the wheel in the p1-oracle review).
- **Who notices:** Python users who print a GradingRGBCurve.
- **A fix:** the repr its values give, like the other grading classes'.
- **Status:** to be matched in Phase 6 (D13).
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
- **Decided** (general rule): the port gives the wheel's messages where the wheel raises, and an
  error where it would overrun.
- **Status:** to be matched in `p1-bitdepth` (1.1e).
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
  line. `p1-gpu-infra` settles the scope: 1.7d for the wrapper, 1.7e for the uid.
- **Status:** the wrapper's part matched in `p1-gpu-infra` (1.7d, U-10); the uid's to be matched
  in 1.7e. The oracle refuses these MSL prefixes whatever the processor (`gpu_shader`,
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
  them into lines (U-6). A texture declared last, with nothing after it, is enough: the next
  line is empty.
- **Who notices:** MSL shaders whose added declaration code declares a texture without a sampler
  after it, or whose resource prefix holds line feeds like U-6's.
- **Decided** (general rule): the port returns an error exactly where the read would pass the
  line's end, and otherwise parses as upstream does: a line of 6 bytes is read up to its
  terminating NUL, and a non-ASCII byte is neither white space nor a digit, as in both wheels
  (U-6). This settles the class wrapper's part of U-6.
- **Status:** matched in `p1-gpu-infra` (1.7d).
