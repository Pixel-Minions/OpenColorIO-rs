# Improvement candidates

Upstream bugs and limitations that the port copies, so that its results match OpenColorIO
2.5.2 byte for byte, but that could be fixed. Nothing here is fixed during the port. At the
end, the owner decides on each one: keep it, or fix it as a deviation (`docs/deviations.md`),
which changes the port's results for the inputs the entry names.

The last section lists places where upstream's behaviour is undefined: it reads or writes
memory it doesn't own. The port can't copy those, so each needs a decision before its card
lands.

**Adding an entry** (`CLAUDE.md`, rule 1). A chunk that ports an upstream bug or limitation,
or a platform difference that exists only because of how the wheels were built, adds an
entry here in the same chunk. Take the next free number in its section's series (`I-` or
`U-`). An entry says:
- **Upstream:** what OCIO does, with its source (`file:line @ v2.5.2`) or the wheel probe that
  showed it;
- **Who notices:** the inputs or calls affected, in plain words;
- **A fix:** what the port would do instead, and what that changes;
- **Status:** matched (with the card), to be matched (with the phase), or open.

## Images and CPU processing

### I-1. Huge images fail on Windows

- **Upstream:** image sizes and positions are C `long`, which is 32 bits on Windows and 64 bits
  on Linux. On Windows, the non-packed paths fail on images of 2^31 pixels or more (about
  46,000 × 46,000) with "Invalid output image position.", where Linux works
  (`ImagePacking.cpp`, `ScanlineHelper.cpp` @ v2.5.2; read from the source, to be confirmed
  with the wheel).
- **Who notices:** applications that process single images of over 2 gigapixels on Windows.
- **A fix:** 64-bit sizes on every platform, so those images work on Windows too.
- **Status:** to be matched in `p1-bitdepth` (1.1d), once the wheel confirms it. The owner chose
  to match it on 2026-09-30.

### I-2. "Invalid x stride." is checked on Windows only

- **Upstream:** `PackedImageDesc` refuses an x stride equal to `AutoStride`, the value a stride
  has only after an overflow (`ImageDesc.cpp:310-313`). The check comes after
  `std::abs(m_xStrideBytes)`, which is undefined for that value, so GCC removed it: the Linux
  wheel accepts such an image, and applying it reads memory it doesn't own. The Windows wheel
  refuses it.
- **Who notices:** only code that passes absurd strides.
- **A fix:** the same check on both platforms.
- **Status:** to be matched in `p1-bitdepth` (1.1b): the check on Windows; on Linux, D-2 refuses
  the image because it reaches outside its buffer.

### I-3. The RGBA fast path truncates the x stride to 32 bits

- **Upstream:** `PackedImageDesc` decides whether an image is tightly packed RGBA with
  `div((int)m_xStrideBytes, (int)m_chanStrideBytes)` (`ImageDesc.cpp:264`). An x stride of 4 GiB
  or more is truncated, so a layout whose pixels are 4 GiB plus 4 channels apart counts as
  tightly packed, and the fast path processes the wrong bytes.
- **Who notices:** nobody in practice: it needs pixels 4 GiB apart.
- **A fix:** compare the full 64-bit stride.
- **Status:** to be matched in `p1-bitdepth` (1.1b).

### I-4. Integer input can drop a matrix's alpha offset

- **Upstream:** with an integer input bit depth and the default optimization, the optimizer bakes
  the leading ops into one Lut1D. With 8-bit input, a Matrix then Log lost the matrix's alpha
  offset in that bake; `OPTIMIZATION_NONE` keeps it (seen through the wheel in O1.2).
- **Who notices:** integer images through a matrix that offsets alpha, with the default
  optimization.
- **A fix:** keep alpha through the bake.
- **Status:** to be matched in Phase 2 (WP 2.5, the Lut1D bake), once confirmed with the wheel.

## Configs and cache IDs

### I-5. Different inline LUTs can share a cached processor

- **Upstream:** `Config::getProcessor` caches processors under a hash of the transform's text
  (`Config.cpp:4830-4841`), and the cache is on by default. For a `Lut1DTransform` or
  `Lut3DTransform` that holds its values in memory, that text gives only its size, settings and
  the smallest and largest values (`transforms/Lut1DTransform.cpp:184-224`,
  `transforms/Lut3DTransform.cpp:174-218`). Two different in-memory LUTs with the same size,
  settings and range therefore get the same processor from one config: the second applies the
  first one's LUT.
- **Who notices:** applications that build LUT transforms in code, not from files, and get
  several processors from one config.
- **A fix:** put the LUT's values, or a hash of them, in the key.
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

## Transforms

### I-11. Copying a group transform shares its children

- **Upstream:** `GroupTransform::createEditableCopy` copies the list of child transforms, not the
  children (`transforms/GroupTransform.cpp:36-44`). Editing a child of the copy edits the
  original's child too.
- **Who notices:** code and Python scripts that copy a group and then change its children.
- **A fix:** copy the children as well.
- **Status:** to be matched when `GroupTransform` is ported (1.8a).

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

## Undefined behaviour upstream: decisions needed

Out-of-bounds image layouts are already decided: the port returns an error (D-2, approved on
2026-09-30).

### U-1. 10- and 12-bit values above their maximum

- **Upstream:** with an integer input bit depth and the default optimization, each input code
  indexes a table of 1,024 (10-bit) or 4,096 (12-bit) entries without a bounds check
  (`CPUProcessor.cpp:140-146`, `ops/lut1d/Lut1DOpCPU.cpp:58-64, 635-650`). A 16-bit value above
  the maximum reads past the table: garbage, or a crash. It crashed the oracle.
- **Options:** clamp to the largest code, ignore the extra bits, return an error, or panic.
- **Status:** open; decided in Phase 2, with the Lut1D bake.

### U-2. `getAData()` without an alpha plane

- **Upstream:** in Python, `PlanarImageDesc.getAData()` on an image without an alpha plane
  returns uninitialized memory (seen through the wheel in O1.2).
- **Options:** return `None`, or an empty array.
- **Status:** open; decided in Phase 6.
