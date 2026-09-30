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
  `getName()` still returns `a`.
- **Who notices:** code that spells an attribute name with different cases.
- **A fix:** match names the same way everywhere, so that setting an attribute replaces the one
  that reading returns.
- **Status:** matched in `p1-foundations` (1.2a).

### I-15. A misspelled error message

- **Upstream:** renaming a metadata element to `ROOT`, or adding a child element named `ROOT`,
  fails with "'ROOT' is reversed for root FormatMetadata elements."
  (`fileformats/FormatMetadata.cpp:241`): "reversed" for "reserved".
- **Who notices:** anyone who reads the message.
- **A fix:** "reserved".
- **Status:** matched in `p1-foundations` (1.2a).

## Logging

### I-16. Two messages bypass the logging function

- **Upstream:** the warning about an invalid `OCIO_LOGGING_LEVEL`, and the version line logged
  when that variable asks for debug messages, are written straight to stderr
  (`Logging.cpp:45-50, 57-61`), even when the application has set its own logging function.
- **Who notices:** applications that show or collect OCIO's log through a logging function.
- **A fix:** send them through the logging function, like every other message.
- **Status:** matched in `p1-foundations` (1.2e).

### I-17. A NUL in a logged message cuts its line

- **Upstream:** the logging function receives each line as a C string (`Logging.cpp:86`), so a
  line that holds a NUL byte stops there, and loses the rest of its text and its line break.
  A warning about a key or name with a NUL shows this (seen through the wheel in 1.2e); I-6
  describes one way such names arise.
- **Who notices:** applications that log messages about names with NUL bytes; the next line of
  their log continues on the same line.
- **A fix:** pass the whole line, with its length.
- **Status:** matched in `p1-foundations` (1.2e).

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
