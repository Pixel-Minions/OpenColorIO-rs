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
- **Status:** matched in `p1-engine` (1.2d), each platform as its wheel compiled it. The 1D
  LUT lookups to F32 at the start of a processor convert in place the same way (`p1-optimizer`);
  with hue adjust (`p2-lut1d-fwd`, 2.1b) they read the three colour codes first, then store the
  floats before they read alpha, which is then a byte of red's float or half of green's, except
  for 10-, 12- and 16-bit input on Linux, which reads alpha first
  (`Lut1DRendererHueAdjust<inBD, F32>::apply`, `Lut1DRendererHalfCodeHueAdjust<F16, F32>`).

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
  ID on each platform. Through the wheels, the oracle's commands show the same split in
  more places, for a negative NaN, `-nan(ind)` on Windows and `-nan` on Linux:
  - the messages of `validate()` that print a parameter's value (`transform_text`, over 247
    transforms of 13 classes holding special values; `oracle/ocio_oracle/transform_text.py`,
    c49a36e);
  - shader text, whose literals print the parameter (`getFloatString`,
    `GpuShaderUtils.cpp:21-35`; `gpu_shader`, over 1230 shaders in all 10 languages;
    `oracle/ocio_oracle/gpu.py`, 470d0b7);
  - baked LUTs, whose ranges and entries print NaN results with each library's spelling
    (`-nan(ind)` on Windows and `-nan` on Linux for a negative NaN), and whose NaNs can even
    differ in sign: a spi1d baked over the shaper range of a color space whose `from_reference`
    offsets red by `.nan` writes `From nan nan` on Windows and `From -nan -nan` on Linux (the
    port matches each; `bake`, `crates/ocio/tests/bake_formats_oracle.rs`, p4-baker; probed
    2026-10-16);
  - the CLF/CTF writer (`fileformats/ctf/CTFTransform.cpp` @ v2.5.2), which streams most
    parameters as they are: a CDL's slope, offset and power and its saturation (`CDLWriter`,
    813-933), a gamma's or an exponent's parameters (`AddGammaParams`, 1181-1214), a log's
    (`AddLogParam`, 1967-1975) and a range's bounds (`WriteTag`, 2474-2480). A negative NaN
    that validation lets through (a CDL's offset, an exponent's alpha, a log's base or
    slopes) is written `-nan(ind)` on Windows and `-nan` on Linux; a range can't hold one, a
    NaN bound being an unset one. The FixedFunction params and the LUT arrays go through
    `WriteValue` (519-539 @ v2.5.2), which writes `nan` on both (`write_transform`, p4-ctf-read-3).
- **Who notices:** anyone comparing text, messages, shaders or cache IDs across platforms for
  transforms with NaN parameters.
- **A fix:** one spelling on both platforms.
- **Status:** matched in `cfmt` (WP 0.5); each op's text, validation messages and shader
  literals use it as the op lands (D12). The CLF/CTF writer's cases:
  `nan_parameters_write_as_in_the_wheel` (`crates/ocio/tests/file_format_ctf_write_oracle.rs`),
  checked on both platforms.

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
- **Who notices:** Windows configs with a drive letter in a string `search_path`. Such a path
  still resolves when the working directory is on the same drive: `C` names a directory that
  doesn't hold the file, and `/luts` is then `C:/luts`, a path relative to the current drive's
  root. Upstream's tests rely on it: 9 of them set a string search path holding the test files'
  absolute path (`GetTestFilesDir()`) and pass on Windows only from a working directory on the
  checkout's drive, in the port as in upstream's CI: `FileTransform` `interpolation_validity`,
  `cc_file_with_different_file_extension`; `Config` `look_fallback`,
  `context_variable_with_search_path_v1` and `_v2`, `is_colorspace_linear`; `CPUProcessor`
  `with_several_ops`, `image_desc`; `Caching` `processor_cache`.
- **A fix:** don't split after a drive letter.
- **Status:** matched in `p3-context` (3.5, `Context::set_search_path`,
  `crates/ocio/src/context.rs:169-178`).

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

### I-100. YAML positions wrap after 2^31 characters or lines

- **Upstream:** yaml-cpp counts the reader's position, line and column in C++ `int`s with `++`
  (`Stream::get`, `AdvanceCurrent`, yaml-cpp 0.8.0 `src/stream.cpp:262-303`), and prints the
  line and column plus one (`include/yaml-cpp/exceptions.h:173-181`). Past 2^31 characters or
  lines the counters overflow, undefined behaviour that a plain machine addition resolves by
  wrapping. The port wraps them: marks turn negative, and a mark of -1 everywhere reads as
  "no mark".
- **Who notices:** configs larger than 2 GiB, in the line numbers of their error messages and
  in yaml-cpp's 1024-character limit on simple keys, which compares positions.
- **A fix:** count in 64 bits.
- **Status:** matched in the YAML parser (`p3-yaml-parser`, `crates/ocio/src/yaml_cpp/`).

### I-101. A NUL byte in an unquoted or block scalar starts an escape sequence

- **Upstream:** yaml-cpp scans plain and block scalars with no escape character, which it
  stores as `0`, and still compares each character with it (yaml-cpp 0.8.0
  `src/scanscalar.cpp:69-75`). So a NUL byte and the character after it go through the
  escapes of double-quoted scalars (`src/exp.cpp:66-134`): NUL then `n` reads as a line
  break, NUL then `0` as a NUL, and NUL then another character fails with "unknown escape
  character: ...". Seen through the wheel: `ocio_profile_version: x<NUL>ny` reads the version
  `x`, line break, `y`; `ab<NUL>cd` fails at line 1, column 5 with "unknown escape character:
  c"; a NUL at the end fails with the end-of-input character 0x04 as the unknown one.
- **Who notices:** configs with NUL bytes outside quoted scalars.
- **A fix:** refuse the NUL, or keep it as it is.
- **Status:** matched in the YAML parser (`p3-yaml-parser`), and checked against the wheel in
  `crates/ocio/tests/yaml_cpp_parser_oracle.rs`.

### I-102. The control character 0x04 can end a YAML token

- **Upstream:** yaml-cpp's reader marks the end of the input with the character 0x04
  (`Stream::eof()`, yaml-cpp 0.8.0 `src/stream.h:40`), and passes the byte 0x04 of UTF-8
  input through unchanged. Its expressions that accept "the end of the input" test for that
  character (`RegEx::MatchOpEmpty`, `src/regeximpl.h:100-103`), so a 0x04 byte counts as the
  end where the scanner looks for one: `:` followed by 0x04 is a mapping indicator, and so are
  `-`, `---` and `...` followed by it. Seen through the wheel: `ocio_profile_version:<0x04>x`
  is a map whose version is `<0x04>x`, where `ocio_profile_version:x` is a scalar. Elsewhere
  the byte is an ordinary character.
- **Who notices:** configs with 0x04 bytes.
- **A fix:** decode UTF-8 so that an input byte can't be the end marker.
- **Status:** matched in the YAML parser (`p3-yaml-parser`), and checked against the wheel in
  `crates/ocio/tests/yaml_cpp_parser_oracle.rs`.

### I-103. Configs read some numbers differently on Windows and Linux

- **Upstream:** yaml-cpp reads a number with `std::stringstream >> value` (yaml-cpp 0.8.0
  `include/yaml-cpp/node/convert.h:160-201`), so each wheel's C++ library decides what a number
  is. MSVC's STL with the UCRT's `strtod` (Windows) reads hexadecimal floats (`0x1p3` is 8,
  `0x.8` is 0.5) and refuses a nonzero value that rounds to zero (`1e-400`, `2e-324`: "bad
  conversion"). libstdc++ with glibc's `strtod_l` (Linux) refuses hexadecimal floats (it reads
  the `0` and stops) and reads a value that rounds to zero as 0. Both refuse a value that
  overflows and keep subnormal values. A config with such a number loads with a different
  value, or fails, on one platform only. Seen through both wheels (`yaml_scalars`, O3.3).
  The Windows wheel doesn't ship its C++ library: it runs the `msvcp140.dll` of the machine
  (System32), 14.51.36247 on the reference machine, so its reading can change with a Visual
  C++ runtime update. The port translates the MSVC 14.44 headers' `num_get`, and is checked
  against the 14.51 runtime.
- **Who notices:** configs with hexadecimal floats or numbers below the smallest subnormal.
- **A fix:** one reader on both platforms (decimal only, and a value that rounds to zero
  read as 0, say).
- **Status:** matched in the YAML parser (`p3-yaml-parser`, `ocio_ops::utils::num_get`), and
  checked against both wheels in `crates/ocio/tests/yaml_cpp_convert_oracle.rs`.

### I-104. The escapes `\N` and `\_` give bytes that aren't UTF-8

- **Upstream:** in a double-quoted scalar, yaml-cpp turns `\N` (next line, U+0085) into the
  single byte 0x85 and `\_` (no-break space, U+00A0) into the single byte 0xA0 (yaml-cpp 0.8.0
  `src/exp.cpp:117-120`), where UTF-8 needs two bytes (C2 85, C2 A0); the other escapes,
  `\L`, `\P` and `\x`/`\u`/`\U`, give UTF-8. So a config string with these escapes holds
  bytes that aren't UTF-8. Seen through the wheel: the version `"x\N\_\L\P"` reads as `x`,
  0x85, 0xA0, then E2 80 A8 and E2 80 A9.
- **Who notices:** configs that write these two escapes, in names, descriptions or roles.
- **A fix:** write them as UTF-8.
- **Status:** matched in the YAML parser (`p3-yaml-parser`, `crates/ocio/src/yaml_cpp/exp.rs`).

### I-105. UTF-16 and UTF-32 configs are decoded leniently

- **Upstream:** yaml-cpp converts a UTF-16 or UTF-32 input (found by its byte order mark or
  its first bytes) to UTF-8 as it reads (yaml-cpp 0.8.0 `src/stream.cpp:161-182`,
  `336-445`), and never refuses a code unit:
  - a lone low surrogate, and a high surrogate that no low one follows, read as U+FFFD (the
    unit after a lone high surrogate is then read on its own);
  - U+0004, the reader's end-of-input character (I-102), reads as U+FFFD;
  - UTF-32 surrogates and values above U+10FFFF are written in UTF-8's form anyway, and above
    0x1FFFFF they lose their high bits (`Utf8Adjust` masks them).
  Seen through the wheel: `ocio_profile_version: x`, U+0004, `y` in UTF-16 reads the version
  `x`, U+FFFD, `y`; with a lone 0xDC00 in place of U+0004 too.
- **Who notices:** UTF-16 and UTF-32 configs with invalid code units.
- **A fix:** refuse invalid code units, and keep U+0004.
- **Status:** matched in the YAML parser (`p3-yaml-parser`, `crates/ocio/src/yaml_cpp/stream.rs`),
  and checked against the wheel in `crates/ocio/tests/yaml_cpp_node_oracle.rs`.

### I-130. Some setters keep a config's old cache ID

- **Upstream:** a config caches its cache ID (`Config::getCacheID`, `Config.cpp:5250-5315`),
  the hash of its `serialize()` text, and most setters reset it (`Impl::resetCacheIDs`). But
  `setName`, `setDescription`, `setFamilySeparator` and `setMinorVersion`
  (`Config.cpp:2115-2118, 2132-2145, 2154-2157, 1311-1324`) don't, though the text holds what
  they set. Seen through the wheel: after each of them, `getCacheID()` returns the ID it
  returned before, while `serialize()` changed (`ocio_profile_version: 2.3` after
  `setMinorVersion(3)`); a later setter that resets the ID (`setStrictParsingEnabled`) gives a
  new one. A config's validation result is kept the same way.
- **Who notices:** applications that change a config's name, description, family separator or
  minor version in code, and cache their results by the config's cache ID.
- **A fix:** reset the cache IDs in these setters too.
- **Status:** the setters are matched in `p3-config-1` (3.4d: they keep the cache IDs); the
  cache ID itself comes with 3.7d, which checks it against the wheel.

### I-131. `isInactiveColorSpace` reads its own list its own way

- **Upstream:** `Config::isInactiveColorSpace` (`Config.cpp:2561-2575`) doesn't ask the list
  the config uses (`getColorSpaceNames(..., COLORSPACE_INACTIVE)`, built from the API's, the
  environment's or the config's list, split on `,` and trimmed, by name, alias or role,
  `Config.cpp:5351-5407`). It splits the config's own list (`getInactiveColorSpaces`) on the
  two characters `", "` and compares each piece with the name, ignoring case. So:
  - `OCIO_INACTIVE_COLORSPACES` is ignored;
  - `"a,b"` is one piece, so `a` is not inactive, though the config makes it so;
  - aliases and roles don't count;
  - an empty list splits into one empty piece, so `isInactiveColorSpace("")` is true on a
    config without inactive color spaces.
  Seen through the wheel, as in `crates/ocio/tests/config_oracle.rs`.
- **Who notices:** applications that ask whether a color space is inactive by this function.
- **A fix:** look the name up in the config's inactive color spaces.
- **Status:** matched in `p3-config-1` (3.4g), checked against the wheel in
  `crates/ocio/tests/config_oracle.rs`.

### I-132. Removing a named transform keeps it in the lists of active ones

- **Upstream:** `Config::removeNamedTransform` (`Config.cpp:3300-3317`) returns as soon as it
  has erased the named transform, before it resets the cache IDs and rebuilds the lists of
  active and inactive named transforms; it does both only when no named transform has the
  name. So after a removal, `getNamedTransformNames()` (the active ones) and
  `getNamedTransformNames(NAMEDTRANSFORM_INACTIVE)` still give the removed name, and the
  config's cache ID and validation stay as they were. Seen in the source only: the wheel's
  Python module doesn't bind `removeNamedTransform`, and upstream's tests don't call it. The
  config merger calls it on its working copy of a config (`apphelpers/mergeconfigs/
  SectionMerger.cpp:3288, 3415, 3563`), but builds the merged config with
  `addNamedTransform`, which rebuilds the lists (`Config.cpp:3296-3297`), so a merged config
  doesn't show the stale ones.
- **Who notices:** C++ applications that remove named transforms and list them after.
- **A fix:** reset and rebuild after the removal, as `removeColorSpace` does.
- **Status:** matched in `p3-config-2` (3.4j, `Config::remove_named_transform`).

### I-134. Copying a config reads the environment again

- **Upstream:** `Config::createEditableCopy` (`Config.cpp:1352-1357`) makes a new config with
  `Config::Create`, whose `Impl` constructor (`Config.cpp:335-374`) reads `OCIO_ACTIVE_DISPLAYS`,
  `OCIO_ACTIVE_VIEWS` and `OCIO_INACTIVE_COLORSPACES` and splits the first two with
  `SplitStringEnvStyle`, then overwrites all three with the copied config's. So the copy reads
  the environment for nothing, and it throws (`SplitStringEnvStyle`'s error for a quote opened
  and not closed before a separator) when the environment has changed since the config was
  made and one of those lists is now malformed. The Python binding's `__deepcopy__`
  (`PyConfig.cpp:193-196`) goes through it.
- **Who notices:** a host that changes `OCIO_ACTIVE_DISPLAYS` or `OCIO_ACTIVE_VIEWS` to a
  malformed list while it holds a config, then copies the config.
- **A fix:** copy the state without constructing a new `Impl` from the environment.
- **Status:** not matched, by Rust's `Clone`: `Clone for Config` can't fail, and copies without
  reading the environment (`crates/ocio/src/config.rs`). A binding that must raise as the wheel
  does can call `Config::new()` first and drop its result, as upstream's construction does.
  Seen in the source; the oracle sets the environment once per process, before the config is
  made, so it can't show the case.

### I-135. A viewing rule's color space and encoding indices are cut to `int`

- **Upstream:** `ViewingRules::getColorSpace` and `getEncoding` (`ViewingRules.cpp:240-253,
  288-301`) check a `size_t` index after `static_cast<int>`, which both wheels compile as the
  low 32 bits, signed. So an index of 2^32 + n is index n, and an index whose low 32 bits are
  2^31 or more is negative: it passes the check, and `TokensManager::getToken` gives a null
  pointer. `removeColorSpace` and `removeEncoding` go through them: through the wheel,
  `removeColorSpace(0, 2**32)` removes the rule's first color space, and
  `removeColorSpace(0, 2**31)` removes nothing, without an error.
- **Who notices:** code and Python scripts that pass a wrong, large index.
- **A fix:** check the `size_t` index against the count, as `getName` does.
- **Status:** matched in `p3-rules` (3.9g, `crates/ocio/src/viewing_rules.rs`): the getters
  return `Option` for the null pointer; checked against the wheel
  (`crates/ocio/tests/viewing_rules_oracle.rs`, `large_indices_match_the_wheel`).

### I-136. Views for a color space miss rules on an alias, a role or an encoding's case

- **Upstream:** `Config::Impl::getFilteredViews` (`Config.cpp:838-901`), behind
  `getViews(display, colorSpaceName)`, finds the image's color space by any of its names, but
  compares the rules' color spaces (roles resolved) with the name it was given, in lower case:
  given an alias or a role, it misses the rules that name the color space, and a rule that
  names an alias matches only an image given by that alias. It compares each of a rule's
  encodings, in lower case, with the color space's encoding as the color space writes it: a
  color space whose encoding has an upper-case letter matches no rule.
- **Who notices:** applications that ask for the views of an image by an alias or a role, and
  configs whose color spaces write their encodings with capitals.
- **A fix:** compare with the found color space's name, and both encodings in lower case.
- **Status:** matched in `p3-rules` (3.9g, `Config::impl_filtered_views`); checked against the
  wheel (`crates/ocio/tests/config_oracle.rs`, `views_by_viewing_rules_match_the_wheel`).

### I-138. Upgrading a version 1 config gives the rule at index 1 the default color space

- **Upstream:** `UpdateFileRulesFromV1ToV2` (`FileRules.cpp:960-1047`), which
  `Config::upgradeToLatestVersion` calls for a version 1 config without a `default` role,
  gives its choice of color space (a data `raw`, the first data color space, the first active
  one, or the first one) to the rule at index 1 (`setColorSpace(1, ...)`), meant as the
  default rule after the path search rule it inserts first. When the config already has
  other rules, index 1 is one of them: through the wheel, rules `[ColorSpaceNamePathSearch,
  g (color space b), Default]` upgrade to `g` with the chosen color space and `Default` left
  as `default`; when it is the path search rule, the upgrade ends the program (U-55).
- **Who notices:** version 1 configs made in code with file rules, then upgraded.
- **A fix:** set the color space of the default rule (the last one).
- **Status:** matched in `p3-rules` (3.9f); checked against the wheel
  (`crates/ocio/tests/config_oracle.rs`, `upgrades_match_the_wheel`, "a rule at index 1").

### I-139. A regular-expression rule given a pattern or an extension becomes a partial glob

- **Upstream:** `FileRule::setPattern` and `setExtension` on a regular-expression rule
  (`FileRules.cpp:360-413`) make it a glob with the other part empty, a glob that
  `insertRule` and these setters can't make (they refuse an empty pattern or extension). An
  empty extension builds `(\..*)`: the rule takes any extension but not a name without one
  (`abc.x` and `abc.`, not `abc`), and its `repr()` shows no extension; an empty pattern
  builds `(.*)`.
- **Who notices:** code and Python scripts that turn regular-expression rules into globs one
  part at a time.
- **A fix:** refuse the change, or give the other part `*`.
- **Status:** matched in `p3-rules` (3.9e-f); checked against the wheel
  (`crates/ocio/tests/config_oracle.rs`, `regex_rules_made_globs_match_the_wheel`).

### I-140. The transform loaders word their messages unevenly

- **Upstream:** the loaders of a config's transforms (`OCIOYaml.cpp`) report the same things
  in different words:
  - an unknown key gives "At line N, unknown key 'key' in 'MatrixTransform'." in most
    classes, but "Unknown key in LogTransform: 'key'.", without its line, in the Log,
    ExponentWithLinear, FixedFunction and grading loaders (`OCIOYaml.cpp:1118, 1476, 1748,
    2056, 2239, 2443, 2920`, which call the other overload of `LogUnknownKeyWarning`);
  - a `base` that isn't one number gives "LogTransform parse error, base must be a  single
    double. Found 2." with two spaces, and no line (`OCIOYaml.cpp:2900-2901`);
  - a node that isn't a map gives "Unsupported Transform type encountered: (2) in OCIO
    profile.", the number of yaml-cpp's `NodeType` (2 a scalar, 3 a sequence) rather than its
    name (`OCIOYaml.cpp:3198-3205`).
  Seen through the wheel.
- **Who notices:** anyone reading the warnings and errors of a config with mistakes in it.
- **A fix:** one wording with the line for every unknown key; one space; the node type's
  name.
- **Status:** matched in `p3-yaml-load-1` (3.3g: the Log, Matrix and Range loaders; 3.3h: the
  ExponentWithLinear loader; 3.3i2: the FixedFunction loader; the grading loaders with Phase 5),
  checked against the wheel in `crates/ocio/src/ocio_yaml_oracle_tests.rs`.

### I-141. A transform an alias names is loaded again at each use

- **Upstream:** a GroupTransform loads each child node it lists (`OCIOYaml.cpp:2520-2525`),
  so a child given as an alias (`*m`) is loaded again for each use, and the config holds a
  transform per use. Nested aliases multiply: n levels of groups whose children are two
  aliases of the level below hold 2^n transforms, from a config of n lines.
- **Who notices:** configs that repeat a transform through aliases (they load as written), and
  machine-made configs that nest them, which take time and memory that grow exponentially.
- **A fix:** share the transform an alias names, or bound the number of transforms a config
  loads.
- **Status:** matched in `p3-yaml-load-1` (3.3h2), checked against the wheel with aliases of a
  matrix and of nested groups in `crates/ocio/src/ocio_yaml_oracle_tests.rs`.

### I-142. Two transform loaders take a repeated key

- **Upstream:** the loaders of the BuiltinTransform and the DisplayViewTransform don't call
  `CheckDuplicates` (`OCIOYaml.cpp:600-629, 840-891`), so a key given twice is read twice and
  the last value wins (`{view: a, view: b}` loads view `b`), where every other transform
  refuses it ("Key-value pair with key 'view' specified more than once. "). Seen through the
  wheel.
- **Who notices:** configs that repeat a key of these two transforms by mistake.
- **A fix:** check for repeated keys in both, as in the others.
- **Status:** matched in `p3-yaml-load-1` (3.3i), checked against the wheel in
  `crates/ocio/src/ocio_yaml_oracle_tests.rs`.

### I-143. A description of newlines only is read past its start

- **Upstream:** `SanitizeNewlines` removes a description's trailing newlines one by one, and
  reads the last character again after each (`OCIOYaml.cpp:50-55`). When every character was a
  newline, it reads `back()` of the string it has just emptied, an access out of range (the
  byte before its buffer); the loop then stops whatever the byte is, as the string is empty.
  Both wheels give the empty description (`description: "\n\n"`), seen through the wheel.
- **Who notices:** no one: the value read is never used. Decided (coordinator, 2026-10-07):
  match the result, as both wheels give it predictably.
- **A fix:** test the length before reading the last character.
- **Status:** matched in `p3-yaml-load-2` (3.3j), checked against the wheel in
  `crates/ocio/src/ocio_yaml_objects_oracle_tests.rs`.

### I-144. Named transforms' and views' descriptions keep their trailing newlines

- **Upstream:** the loaders of color spaces, looks, view transforms and the config read a
  description with `loadDescription`, which drops its trailing newlines (`SanitizeNewlines`);
  the named transform's loader (`OCIOYaml.cpp:3965-3969`) and the view's, for displays, shared
  views and the virtual display (`OCIOYaml.cpp:452-455`), read it as a plain string, so
  `"d\n\n"` stays `d` and two newlines. Seen through the wheel.
- **Who notices:** configs whose named transforms or views have descriptions ending in newlines
  (a literal block `|` adds one).
- **A fix:** read them with `loadDescription`, as the others.
- **Status:** matched in `p3-yaml-load-2` (3.3j; views in 3.3k), checked against the wheel in
  `crates/ocio/src/ocio_yaml_objects_oracle_tests.rs` and
  `crates/ocio/tests/config_load_oracle.rs`.

### I-145. A config's name loses its trailing newlines

- **Upstream:** the config loader reads the `name` key with `loadDescription`
  (`OCIOYaml.cpp:4531-4535`), so `SanitizeNewlines` drops its trailing newlines: `name: "n\n\n"`
  gives the name `n`. Names of color spaces, looks and the other objects are read as plain
  strings. Seen through the wheel.
- **Who notices:** configs whose name ends in a newline (a literal block `|` adds one); the name
  read back differs from the one written.
- **A fix:** read the name with the string loader, as the other names.
- **Status:** matched in `p3-yaml-load-2` (3.3l), checked against the wheel in
  `crates/ocio/tests/config_load_oracle.rs` ("name and description").

### I-146. Two misspelled version-consistency messages

- **Upstream:** the config's version check (`Config::Impl::checkVersionConsistency`) refuses a
  BuiltinTransform in a version 1 config with "Only config version 2 (or higher) can have
  BuiltinInTransform." (`Config.cpp:5594`), and a FileTransform's CDL style with "Only config
  version 2 (or higher) can use CDL style' for FileTransform." (`Config.cpp:5719`), a stray
  quote. Seen through the wheel.
- **Who notices:** anyone who reads the messages.
- **A fix:** "BuiltinTransform"; "CDL style".
- **Status:** matched in `p3-yaml-load-2`, checked against the wheel in
  `crates/ocio/tests/config_load_oracle.rs` ("v1 builtin", "v1 file cdl style").

### I-147. A config that failed validation can fail again with an empty message

- **Upstream:** `Config::validate` keeps its result: a failed config throws its kept message
  (`m_validationtext`) again on the next call (`Config.cpp:1361-1365`). Most failures set it
  before throwing, but four don't: an environment variable whose default names another variable
  (`Config.cpp:1396-1402`), a transform's own `validate()` (`Config.cpp:1818`), a view's looks
  that can't be parsed (`Config.cpp:670`), and the version check (`Config.cpp:2101`). For those,
  the second `validate()` throws an empty message. Seen through the wheel.
- **Who notices:** a caller that validates a config twice and reads the second message.
- **A fix:** set the kept message in those four places too.
- **Status:** matched in `p3-validate`, checked against the wheel in
  `crates/ocio/tests/config_load_oracle.rs` (`validating_twice_as_in_the_wheel`).

### I-148. The built-in configs' texts end their lines with CR LF on Windows

- **Upstream:** the build embeds each `builtinconfigs/configs/*.ocio` file byte for byte, as the
  build's checkout wrote it (`src/OpenColorIO/CMakeLists.txt:262-298`). The Windows wheel's
  checkout wrote them with CR LF line ends and the Linux wheel's with LF, so
  `BuiltinConfigRegistry::getBuiltinConfig` (Python: `BuiltinConfigRegistry()[name]`) gives
  different texts on the two platforms. Seen through the wheels. The configs read from them are
  the same: their `serialize()` and cache IDs match.
- **Who notices:** a caller that writes or hashes a built-in config's text as the registry gives
  it.
- **A fix:** embed the files with LF on every platform (a `.gitattributes` rule for them, or
  normalize them in the build).
- **Status:** matched in `p3-loading` (3.10a, `crates/ocio/src/builtinconfigs/mod.rs`), checked
  against the wheel live on each platform (`crates/ocio/tests/builtin_configs_oracle.rs`).

### I-149. An `ocio://` URI is found anywhere in a path

- **Upstream:** `ResolveConfigPath`, `Config::CreateFromFile` and
  `Config::CreateFromBuiltinConfig` look for a URI with `std::regex_search` of
  `ocio:\/\/([^\s]+)` (`BuiltinConfigRegistry.cpp:36-39`, `Config.cpp:1160-1164, 1246-1252`),
  which matches anywhere in the text, not only at its start: `x ocio://default` resolves to the
  default config's URI, and a file path with `ocio://` inside is read as a built-in config's name,
  whatever follows up to the next space. `CreateFromBuiltinConfig` adds the prefix only when the
  name doesn't start with `ocio://` in lowercase, so `OCIO://default` becomes the name
  `OCIO://default`, which no built-in config has. Seen through the wheel.
- **Who notices:** a caller that passes a path containing `ocio://`, or a URI in another case.
- **A fix:** match the URI at the start of the path only (`std::regex_match` or a prefix
  check), ignoring the prefix's case if wanted.
- **Status:** matched in `p3-loading` (3.10a,
  `crates/ocio/src/builtinconfigs/builtin_config_registry.rs`, `search_builtin_uri`), checked
  against the wheel (`crates/ocio/tests/builtin_configs_oracle.rs`).

### I-157. A config file of fewer than four bytes can read as empty on Windows

- **Upstream:** `Config::CreateFromFile` hands yaml-cpp an `std::ifstream` (`Config.cpp:1176-1206`).
  yaml-cpp's detection of the encoding reads up to four bytes and puts back those that aren't a
  byte order mark (yaml-cpp 0.8.0 `stream.cpp:189-245`). When the file ends first, it puts them
  back after the end: libstdc++'s file stream (Linux) seeks back for each, as a string stream
  does, but MSVC's (Windows) puts back one byte with the C runtime's `ungetc` and one in its own
  putback character, and fails on a third, which sets `badbit`: the stream then reads as an
  empty document. So on Windows the files `00 00 FE` and `61 00 00` give "does not appear to
  have a valid version", where Linux and `CreateFromStream` give yaml-cpp's errors. Seen
  through the wheels.
  A file that can't be read differs too: a directory, which Linux opens, makes libstdc++'s
  `basic_filebuf::underflow` throw, which reaches `OCIOYaml::Read` through yaml-cpp's
  `sgetn` ("... failed. basic_filebuf::underflow error reading the file: Is a directory"),
  where Windows can't open it ("Error could not read '<path>' OCIO profile.").
- **Who notices:** nobody with a real config: only files of three bytes or fewer differ, and
  directories given as configs.
- **A fix:** read the file into a string stream first.
- **Status:** matched in `p3-loading` (3.10b, `crates/ocio/src/yaml_cpp/stream.rs`,
  `IStream::file` and `MsvcFileBuf`), checked against the wheel on each platform for every file
  of up to three bytes of the bytes that matter and 256 of four
  (`crates/ocio/tests/config_files_oracle.rs`).

### I-158. Until Phase 4, an OCIOZ archive given to `CreateFromFile` is an error

- **Upstream:** `Config::CreateFromFile` reads a file that starts with `PK` as an OCIOZ archive,
  through `CIOPOciozArchive` and `CreateFromConfigIOProxy` (`Config.cpp:1188-1200`).
- **Who notices:** a caller that loads an `.ocioz` file, or any file starting with `PK`, before
  the archive reader is ported.
- **A fix:** none: this is the port's interim state, not upstream's. Until the archive reader
  lands (Phase 4, the archive card), the port returns "Config::CreateFromFile: reading an OCIOZ
  archive is not ported yet." (`crates/ocio/src/config.rs`, `Config::create_from_file`); that card
  replaces the error with the reader and removes this entry.
- **Status:** interim, in `p3-loading` (3.10b).

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

### I-64. 1D LUT lookups cast floats to integers by their low bits

- **Upstream:** the 1D LUT lookups write alpha to an integer output with a plain cast,
  `OutType(in[3] * m_alphaScaling)` (`Lut1DRendererHalfCode::apply` and
  `Lut1DRenderer::apply`, `ops/lut1d/Lut1DOpCPU.cpp:525, 646`), and so do the hue-adjust
  lookups for their colour values, `OutType(RGB2[c])` (790-793, 886-889). The cast truncates,
  where the other integer conversions add 0.5 first (`Converter<BD>::CastValue`), and
  converting a NaN or a value outside the type's range is undefined behaviour. Both wheels
  compile it as a 32-bit `cvttss2si` and keep the low 8 or 16 bits
  (`Lut1DRendererHalfCode<F16, UINT8>::apply`: Windows 0x180274138, Linux 0x414c15): a NaN,
  an infinity or a value past `INT_MAX` gives 0, and a value in range keeps its low bits, so
  256 gives 0 in 8 bits.
- **Who notices:** nothing through the API: the CPU engine renders a lookup to F32 only
  (`CreateCPUEngine`, `CPUProcessor.cpp:140-146`). Code that calls `GetLut1DRenderer` for half
  input to an integer output (upstream's unit tests do) gets 0 for a NaN or infinite alpha.
- **A fix:** convert with `Converter<outBD>::CastValue`, which rounds and clamps.
- **Status:** matched in `p2-lut1d-fwd` (2.1a for the lookups, 2.1b for the hue-adjust
  lookups).

### I-65. A 1D LUT's float results depend on the row length and the CPU

- **Upstream:** `Lut1DRenderer<BIT_DEPTH_F32, outBD>::apply` renders a row of more than one
  pixel with the SIMD kernel the CPU dispatches to (`ops/lut1d/Lut1DOpCPU.cpp:652-658`), and a
  row of one pixel with its scalar loop (659-720). The two interpolate differently: the kernels
  compute `p + (n - p) * d` from the lower node (`Lut1DOpCPU_SSE2.cpp:47-76`), with one
  rounding on AVX2 and AVX-512 CPUs (`_mm256_fmadd_ps`, `_mm512_fmadd_ps`) and two on SSE2 and
  AVX ones; the scalar loop computes `(low - high) * delta + high` from the upper node. So the
  same input can give different last bits by row length and by CPU. On a node between entries
  of `-FLT_MAX` and `FLT_MAX` (sanitized infinities), the kernels multiply an infinite
  difference by 0 and give NaN, where the scalar loop gives the node's value. The kernels also
  keep alpha's bits from F32 to F32, where the scalar loop multiplies it by 1, which quiets a
  signalling NaN. The integer outputs round to nearest even in the kernels' packs, and add 0.5
  and truncate in the scalar loop. On a CPU with AVX but not F16C, the AVX and AVX2 kernels
  have no half output and replace the SSE2 one with none (`Lut1DOpCPU_AVX.cpp:153-157`,
  `Lut1DOpCPU.cpp:289-294`), so every row to F16 takes the scalar loop.
- **Who notices:** anyone comparing a pixel rendered alone (`applyRGBA`, a one-pixel-wide
  image) with the same pixel in a row, or results across machines.
- **A fix:** one interpolation for every row length (the kernels' arithmetic in the scalar
  loop), and no FMA; that changes the scalar results, and the AVX2 and AVX-512 machines' ones.
- **Status:** matched in `p2-lut1d-simd`, every kernel and the scalar loop; checked against the
  wheel on rows of every length, and under SDE on each kernel's CPUs.

### I-66. An inverse half-domain 1D LUT keeps a reversal at ±Inf in green and blue

- **Upstream:** `Lut1DOpData::initializeFromForward` flattens the reversals of a half-domain
  LUT's positive half up to index `31744u * maxChannels` and of its negative half up to
  `64512u * maxChannels` (`ops/lut1d/Lut1DOpData.cpp:994`, `1013`), without the channel's
  offset `+ c` that its start indices have. For red the last entry flattened is +Inf's (and
  -Inf's); for green and blue the loop stops one entry before them, so a reversal at the +Inf
  or -Inf code stays in those channels.
- **Who notices:** inverse half-domain LUTs whose green or blue values at +Inf or -Inf go the
  wrong way: the processor's cache ID hashes the unflattened value. The inverse renderer's
  effective domain ends at 65504 and -65504 (`31743`, `64511`), so the pixels don't see it.
- **A fix:** end the loops at `31744u * maxChannels + c` and `64512u * maxChannels + c`. That
  changes the LUT's values and its cache ID for such LUTs.
- **Status:** matched in `p2-lut1d-inv` (2.1e); `tests/lut1d_op_oracle.rs` compares the cache
  IDs of inverse half-domain LUTs with such reversals with the wheel's.

### I-67. An inverse half-domain 1D LUT inverts blue's negative half with red's sign

- **Upstream:** `InvLut1DRendererHalfCode::apply` and `InvLut1DRendererHalfCodeHueAdjust::apply`
  invert a value on the negative half of a half-domain LUT with `-flipSign`, the channel's
  sign flipped; for blue they pass `-this->m_paramsR.flipSign`, red's
  (`ops/lut1d/Lut1DOpCPU.cpp:1519`, `1606`). Where blue rises and red falls, or the reverse,
  blue's values on its negative half are clamped and searched with the wrong sign.
- **Who notices:** inverse half-domain LUTs whose blue channel goes the other way from red,
  for blue values on the negative half (at or above the value at +0 for a falling blue,
  below it for a rising one).
- **A fix:** pass `-this->m_paramsB.flipSign`.
- **Status:** matched in `p2-lut1d-inv` (2.1f); `tests/lut1d_renderer_oracle.rs` compares
  the inverse half domain's "crossed" curves (blue falls, red rises) with the wheel's.

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

### I-69. An inverse half-domain 1D LUT splits integer input at an unscaled point

- **Upstream:** `InvLut1DRendererHalfCode::updateData` takes each channel's value at +0 as
  the point that splits the domain's positive and negative halves (`bisectPoint`,
  `ops/lut1d/Lut1DOpCPU.cpp:1393-1404`), as it is in the LUT, but scales the tables it
  searches by the input bit depth's maximum (`lutScale`, 1406-1437), and `apply` compares
  the input value, in the input bit depth's units, with the unscaled point (1460-1532). For
  float and half input the maximum is 1; for integer input the point is `maxValue` times too
  small, so the codes between it and the scaled point take the other half.
- **Who notices:** inverse half-domain LUTs rendered from 8-, 10-, 12- or 16-bit images
  (the CPU engine renders a processor's first 1D LUT from the input bit depth) whose value at
  +0 is not 0.
- **A fix:** scale `bisectPoint` by `lutScale`.
- **Status:** matched in `p2-lut1d-inv` (2.1f); the API format sweep
  (`crates/ocio/tests/api_formats_oracle.rs`) renders inverse half-domain LUTs from every
  bit depth and compares them with the wheel's.

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

### I-90. A linear-style RGB curve changes alpha when it isn't the first op

- **Upstream:** the linear style's renderers (`GradingRGBCurveLinearFwdOpCPU`,
  `GradingRGBCurveLinearRevOpCPU`, `ops/gradingrgbcurve/GradingRGBCurveOpCPU.cpp:237-347`)
  convert all four lanes to the grading log and back with the SSE2 `LinLog` and `LogLin`
  (179-235), then restore alpha with `out[3] = in[3]`. That restores it only when the
  renderer reads one buffer and writes another, as the CPU processor's first op does from a
  packed F32 image (`ScanlineHelper.cpp:130-137`). Every other op of a chain, and every op of
  `applyRGB`/`applyRGBA` and of an image processed in place, renders in place
  (`CPUProcessor.cpp:400`), where `in[3]` is the converted alpha: alpha comes out as
  `LogLin(LinLog(alpha))`, which isn't always alpha (the approximations round, and a
  signalling NaN comes out quiet).
- **Who notices:** anyone whose alpha goes through a GradingRGBCurveTransform of the linear
  style that isn't the first op of the processor, or is applied in place.
- **A fix:** leave alpha alone in the renderers, as the GPU shaders do.
- **Status:** matched in `p2-rgbcurve` (2.6d): `grading_rgb_curve_op_cpu.rs` converts alpha
  in `CpuOp::apply` and restores it in `CpuOp::apply_bit_depth`; the battery checks the first
  op's alpha against the wheel, and `in_place_the_linear_style_converts_alpha`
  (`crates/ocio-ops/tests/grading_rgb_curve_oracle.rs`) the alpha of `applyRGBA` in place.

### I-91. The RGB curves' NaNs differ between Windows and Linux

- **Upstream:** `KnotsCoefs::evalCurve` and `evalCurveRev`
  (`ops/gradingrgbcurve/GradingBSplineCurve.cpp:1258-1381`) are scalar `float` code that the
  two wheels compile with different operand orders (`docs/wheel-inspect.md`). MSVC computes the
  segments' quadratic as `((t * A) + B) * t + C`, the low line as `(x - knStart) * B + C` and
  the high line as `offs + slope * (x - knEnd)` (`sub_1801e87d0`); GCC computes `((A * t) +
  B) * t + C`, `B * (x - knStart) + C` and `offs + (x - knEnd) * slope` (`0x3bc930`). In the
  inverse, MSVC adds the knot after the quotient (`q + knStart`, as the source) and computes
  the root as `kn - C0 / B` and `kn - (C0 + C0) / denom`, where GCC adds the knot first
  (`knStart + q`) and keeps the source's `kn + (-C0) / B` and `kn + (C0 * -2) / denom`
  (`sub_1801e8970`, `0x3bcab0`). The values are the same; where two NaNs meet (a NaN pixel
  and the NaN coefficients of a degenerate curve, such as one with two control points at the
  same x), the result keeps a different NaN, and a NaN `C0` comes out with its sign flipped
  on Linux only.
- **Upstream, the fitting:** `EstimateRGBSlopes` and `FitRGBSpline` (335-446) differ the same
  way where a NaN y coordinate or slope meets the default NaN of an invalid operation. MSVC
  computes a slope's numerator as `secantLen[k - 1] * secantSlope[k - 1] + secantLen[k] *
  secantSlope[k]`, the middle segment's `(ksi - xi) * (s1 - s0)` and the second segment's
  constant as `((ksi - xi) * s0 + yi) + q` (`sub_1801e5df0`, `sub_1801e6c80`); GCC computes
  `secantSlope[k] * secantLen[k] + secantSlope[k - 1] * secantLen[k - 1]`, `(s1 - s0) * (ksi -
  xi)` and `q + (yi + s0 * (ksi - xi))` (inlined in `computeKnotsAndCoefsForRGBCurve`,
  `0x3bef40`, and `0x3be970`). Both add the knots and `s_bar`'s terms in the other order from
  the source. Such a curve's coefficients keep a different NaN on each platform: the GPU
  shader's constants (`nan` against `-nan(ind)`) and a dynamic op's uniform values.
- **Who notices:** anyone comparing NaN pixels of a GradingRGBCurveTransform, or an ACES 1.x
  output built-in, between a Windows and a Linux machine; and the shader text or uniforms of
  a curve with a NaN y coordinate or slope.
- **A fix:** one operand order on every platform, which changes NaN bits on at least one.
- **Status:** matched in `p2-rgbcurve` (2.6b), `KnotsCoefs::eval_curve` and
  `eval_curve_rev`, read from both wheels' machine code; the GradingRGBCurve battery (2.6d,
  `crates/ocio-ops/tests/grading_rgb_curve_oracle.rs`, the "x repeated" case) checks it on
  both platforms. The fitting is matched in `p2-rgbcurve`'s fix-forward chunk,
  `estimate_rgb_slopes` and `fit_rgb_spline`, read from both wheels' machine code;
  `nan_curves_shaders_match_the_wheel`
  (`crates/ocio-gpu/tests/grading_rgb_curve_op_gpu_oracle.rs`) checks it on both platforms.
  `AdjustRGBSlopes` differs too (MSVC's unrolled loop multiplies `slopes[i + 1] * adjust` or
  `adjust * slopes[i + 1]` by the knot's index), but no two different NaNs meet there.

### I-150. A hue-adjust 1D LUT resampled on a domain renders green with red's curve

- **Upstream:** `Lut1DOpData::Compose` evaluates the second LUT, hue adjust included, on the
  entries of a lookup domain, whose three channels are equal (`ComposeVec`, `EvalTransform`;
  `ops/lut1d/Lut1DOpData.cpp:683-830`), and gives the result that LUT's hue adjust
  (`setHueAdjust(lut2->getHueAdjust())`). On equal channels `GamutMapUtils::Order3` names red
  the minimum and green the middle, and the hue factor is 0, so the hue adjust sets each
  entry's green to its red (`ops/lut1d/Lut1DOpCPU.cpp:723-745`, `771-788`): the new LUT's
  green column is red's curve. That happens where the CPU renderers resample a hue-adjust LUT
  for a lookup (`BaseLut1DRenderer::updateData`, 388-406) and where the optimizer replaces an
  inverse hue-adjust LUT with its fast forward LUT (`MakeFastLut1DFromInverse`, 841-867). The
  hue adjust after the lookup recomputes the middle channel of each pixel, so green is wrong
  where it is a pixel's minimum or maximum. Through the wheel, the fast LUT of an inverse
  hue-adjust LUT of three different curves has green equal to red in all of its 4096 entries.
- **Who notices:** hue-adjust (`HUE_DW3`) LUTs whose green curve differs from red's, applied
  to 8-, 10-, 12- or 16-bit or half images whose bit depth the LUT has no entry per code for,
  or inverted with the default optimization (`OPTIMIZATION_LUT_INV_FAST`).
- **A fix:** compose without the hue adjust and set it on the result afterwards, as a
  comment in `Compose` suggests (overriding the hue adjust temporarily).
- **Status:** matched in `p2-lut1d-inv` (2.1g); the API format sweep
  (`crates/ocio/tests/api_formats_oracle.rs`, "256 entries, hue adjust" from 10-, 12- and
  16-bit input) and the battery's inverse hue-adjust cases (`tests/lut1d_renderer_oracle.rs`)
  compare them with the wheel.

### I-151. A hue-adjust 1D LUT misses its nodes' values from float input

- **Upstream:** the hue-adjust renderer of a standard-domain LUT from float input
  (`Lut1DRendererHueAdjust::apply`, its `OCIO_USE_SSE2` branch, `ops/lut1d/Lut1DOpCPU.cpp:909-941`,
  which every x86-64 wheel compiles) truncates the index, takes the next index up as the high
  one, and interpolates down from the high node with `delta = highIdx - idx`:
  `lerpf(lut[high], lut[low], delta)`. An input that lands exactly on a node gets
  `high + 1 * (low - high)`, not the node's value: through the wheel, a pixel of 0 gets
  0.10000002 from the LUT `[0.1, 0.7, 0.3]`; the LUT `[-FLT_MAX, FLT_MAX]` gives -Inf for 0
  (`FLT_MAX + (-FLT_MAX - FLT_MAX)`) and a NaN green for a grey pixel. The other renderers
  interpolate up from the low node.
- **Who notices:** hue-adjust (`HUE_DW3`) LUTs of a standard domain applied to float images:
  values on the nodes are off by a rounding error, and LUTs with values near `FLT_MAX` give
  infinities and NaNs.
- **A fix:** interpolate up from the low node, as the forward renderer without hue adjust
  does (`lerpf(lut[low], lut[high], idx - lowIdx)`).
- **Status:** matched in `p2-lut1d-fwd` (2.1b); `tests/lut1d_renderer_oracle.rs`
  (`hue_adjust_on_nodes_matches_the_wheel`) compares those LUTs' nodes with the wheel's.

### I-152. A 3D LUT of one entry per side starts as NaNs

- **Upstream:** a new 3D LUT is filled with the identity, each entry its grid position times
  `1 / (length - 1)` (`Lut3DArray::fill`, `ops/lut3d/Lut3DOpData.cpp:174-192` @ v2.5.2). For
  a grid size of 1 that step is `1 / 0`, infinity, and the only entry is `0 * inf`: the
  default NaN in each channel (`getData()` gives `0xffc00000` three times on both
  platforms). The forward renderers sanitize NaNs to 0, so such a LUT maps every color to
  black; its text prints `FLT_MAX` and `-FLT_MAX` for each channel's range.
- **Who notices:** anyone who makes a `Lut3DTransform` of grid size 1 (`setGridSize(1)`, or
  `Lut3DTransform(gridSize=1)`) and reads its values, or renders it without setting them.
- **A fix:** fill a grid of one entry with 0 (or refuse a grid size under 2); that changes
  its values, its cache ID and its text, not its forward pixels.
- **Status:** matched in `p2-lut3d` (2.2a), `Lut3DArray::fill`; `processors_match_the_wheel`
  (`crates/ocio/tests/lut3d_transform_oracle.rs`, the "1 entry" case) compares the values bit
  for bit, and `lut3d_transform_through_the_api_matches_the_wheel`
  (`crates/ocio/tests/api_battery_oracle.rs`) the pixels.

### I-153. A 3D LUT of 128 or 129 entries per side can't be inverted

- **Upstream:** the exact inverse renderer grows the LUT by one entry on each side
  (`InvLut3DRenderer::extrapolate3DArray`, `ops/lut3d/Lut3DOpCPU.cpp:1461-1470` @ v2.5.2) into
  a `Lut3DArray`, which refuses more than 129 entries per side (`Lut3DOpData.cpp:194-204`).
  So creating the CPU processor of an inverse LUT of 128 or 129 entries throws "LUT 3D: Grid
  size '130' must not be greater than '129'." (or '131'), with or without
  `OPTIMIZATION_LUT_INV_FAST`: the fast path builds its forward LUT through the same renderer
  (`MakeFastLut3DFromInverse`, `Lut3DOpData.cpp:29-58`). A forward LUT of those sizes works.
- **Who notices:** anyone inverting a 3D LUT of 128 or 129 entries per side on the CPU (129 is
  a common size for high-quality LUTs).
- **A fix:** keep the extrapolated values in a plain array, not limited to 129 entries; such
  LUTs then invert.
- **Status:** matched in `p2-lut3d-inv` (2.2d2), `InvLut3DRenderer::new`;
  `grid_sizes_128_and_129_are_refused` (`crates/ocio-ops/tests/lut3d_inv_oracle.rs`) checks
  the message against the wheel.

### I-154. The exact 3D LUT inverse can undo its own column pivot

- **Upstream:** when `invert_hypercube` meets a (nearly) zero pivot column, its rank-revealing
  search looks through the later columns for the largest entry and swaps that column in; but
  the swap is inside the loop over those columns, so each later column whose entries are no
  larger swaps the same pair again (`ops/lut3d/Lut3DOpCPU.cpp:953-972` @ v2.5.2). With three
  channels, a column found first is swapped back out when the next column is no larger, and
  the factorization goes on with the zero pivot. The cube is then judged by a factorization
  the search meant to avoid, so a color can be found in another cube, or in none (giving 0).
- **Who notices:** inverses of 3D LUTs with coinciding or nearly coinciding corners (flat or
  quantized areas), for colors on those areas, without `OPTIMIZATION_LUT_INV_FAST`, and with
  it through the fast LUT built from the exact inverse.
- **A fix:** swap once, after the search over all later columns; that changes the inverse of
  such colors.
- **Status:** matched in `p2-lut3d-inv` (2.2d2), `invert_hypercube`;
  `degenerate_cubes_match_the_wheel` (`crates/ocio-ops/tests/lut3d_inv_oracle.rs`) includes
  LUTs where the swap back changes the result.

### I-155. The exact 3D LUT inverse clamps its input to [0, 1] and gives 0 when it finds none

- **Upstream:** `InvLut3DRenderer::apply` clamps each channel to [0, 1] before searching,
  though a LUT's values may lie outside it ("TODO: Should improve this based on actual LUT
  contents", `ops/lut3d/Lut3DOpCPU.cpp:1655-1663` @ v2.5.2), and returns 0 in every channel
  when no cube of the extrapolated LUT holds the color ("For now, if no result is found,
  return 0", 1672). It reads the LUT's values as they are, where the forward renderers
  sanitize them (`SanitizeFloat`): a NaN at a cube's base entry stays in the tree's ranges
  (C++'s `std::min` and `std::max` keep a NaN met first, and a parent's range starts from its
  first child's), so that cube, and the rest of the sub-tree it starts, never match; a NaN at
  another entry is skipped, and a cube with it gives a NaN inverse, which the output clamp
  turns into 0.
- **Who notices:** inverses of LUTs whose output range isn't [0, 1] (scene-referred or
  extended-range LUTs) and colors outside the LUT's gamut, which come out as 0 rather than
  near the closest color; LUTs with NaN values.
- **A fix:** clamp to the LUT's own range; return the closest point on the LUT's surface; and
  sanitize the values as the forward renderers do. Each changes those outputs.
- **Status:** matched in `p2-lut3d-inv` (2.2d2), `InvLut3DRenderer::apply` and `RangeTree`;
  the battery of `crates/ocio-ops/tests/lut3d_inv_oracle.rs` checks random LUTs (values in
  [-0.5, 1.5)), LUTs with NaNs and infinities, and LUTs with one NaN.

### I-156. The fast inverse of a 3D LUT is a trilinear cube of 48 entries over [0, 1]

- **Upstream:** with `OPTIMIZATION_LUT_INV_FAST` (in every level from `VERY_GOOD` down, the
  default included), an inverse 3D LUT becomes a forward one: the exact inverse sampled on an
  identity of 48 entries per side, or of the LUT's own size when larger
  (`MakeFastLut3DFromInverse`, `ops/lut3d/Lut3DOpData.cpp:29-58` @ v2.5.2, and `Compose`,
  60-153). The cube keeps the identity's `INTERP_DEFAULT`, so it renders trilinear, though
  the exact inverse inverts the tetrahedral forward LUT (the code that would make it
  tetrahedral is commented out, "it does not seem to help accuracy"); and its domain is
  [0, 1], so values outside it clamp ("TODO: The FastLut will limit inputs to [0,1]").
  Upstream's own test needs a tolerance of 0.015 for a round trip through it
  (`Lut3DOp_tests.cpp`, `cpu_renderer_inverse`).
- **Who notices:** every inverse 3D LUT at the default optimization (CPU and GPU), whose
  results differ from the exact inverse by up to about a hundredth, and more for colors
  outside [0, 1].
- **A fix:** a larger or tetrahedral cube, or the LUT's own output range as its domain; each
  changes those results.
- **Status:** matched in `p2-lut3d-inv` (2.2e), `make_fast_lut3d_from_inverse`;
  `optimized_luts_match_the_wheel` (`crates/ocio/tests/lut3d_transform_oracle.rs`) compares
  the cubes entry for entry, and the API battery and format sweep the pixels.

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

### I-80. Some fixed function styles ignore the direction when they are set

- **Upstream:** `FixedFunctionOpData::ConvertStyle(FixedFunctionStyle, TransformDirection)` gives
  the forward op style of `FIXED_FUNCTION_RGB_TO_HSV`, `XYZ_TO_xyY`, `XYZ_TO_uvY` and
  `XYZ_TO_LUV` whatever the direction (`ops/fixedfunction/FixedFunctionOpData.cpp:433-436, 452-463`),
  where every other style takes the inverse op style in the inverse direction.
  `FixedFunctionTransform::setStyle` converts the style in the transform's current direction
  (`transforms/FixedFunctionTransform.cpp:122-126`), so setting one of these styles on an
  inverse transform makes it forward: `getDirection()` then says forward, and the processor
  renders RGB to HSV instead of HSV to RGB. The config reader calls `setStyle` and
  `setDirection` in the order of the YAML keys (`OCIOYaml.cpp:1421-1483`), so
  `{direction: inverse, style: RGB_TO_HSV}` loads a forward transform where
  `{style: RGB_TO_HSV, direction: inverse}` loads an inverse one.
- **Who notices:** code that sets the style of an inverse transform to one of these four, and
  configs that write `direction` before `style`.
- **A fix:** give those styles their inverse op style in the inverse direction, as the others.
- **Status:** matched in `p2-ff-cpu` (2.3a1, `FixedFunctionOpStyle::from_transform_style`; the
  transform's `set_style` in 2.3e); `fixed_function_op_data_oracle.rs` checks the styles the
  setters give in both directions against the wheel's validation messages.

### I-81. Fixed function renderers order their NaNs per platform

- **Upstream:** the fixed function renderers mix a pixel's channels in sums and products
  (`ops/fixedfunction/FixedFunctionOpCPU.cpp`). Where two NaNs meet, x86 returns the first
  operand's, and the two compilers ordered the operands differently:
  - the glows' YC (`rgbToYC`, 759-766): MSVC adds the green term to the blue one, GCC the
    blue term to the green one (Windows wheel `0x18018b37e`, Linux wheel `0x355195`), and the
    glows' gain is the first factor of red, green and blue with MSVC but the second of blue
    with GCC (`0x18018b4c5`, `0x35513f`);
  - the Rec.2100 surround (1419-1454): MSVC computes `channel * factor`, GCC `factor *
    channel` for red and green (`0x18018da66`, `0x3548b8`);
  - XYZ to xyY (1830-1854): MSVC `(Y + X) + Z` and `d * X`, GCC `(X + Y) + Z` and `X * d`
    (`0x18018e1f9`, `0x353a7e`); xyY to XYZ (1861-1884): MSVC `(Y * x) * d`, GCC
    `(x * Y) * d` (`0x18018e703`, `0x353b40`);
  - XYZ to CIELUV (1956-1987): MSVC `(9 * Y) * d`, GCC `d * (9 * Y)` (`0x18018ddbd`,
    `0x35464f`); CIELUV to XYZ (1994-2026): MSVC `d * u*` and `(... * Y) * dd`, GCC `u* * d`
    and `dd * (... * Y)` (`0x18018d5ac`, `0x353e2f`);
  - RGB to HSY and back (`applyRGBToHSY`, `applyHSYToRGB`, 1591-1763): MSVC compiles one
    function for the three styles, GCC a loop per style, and the luma's and the distance's
    sums, the scaling by `luma / currY`, the linear style's quadratic and the result's
    product each order their operands per compiler and, with GCC, per style (`0x18018ed10`
    and `0x18018e8e0`; `0x358880` and `0x357ce0`; the port's `apply_rgb_to_hsy`,
    `apply_hsy_to_rgb` and `hsy_lin_gain` list them).
  - ACES 2.0's JMh model and chroma compression (`ACES2/Transform.cpp`, 175-250 and
    335-462), which the ACES 2.0 renderers share: MSVC sums the matrices' rows as
    `(p1 + p0) + p2` and GCC as `(p0 + p1) + p2`, except the last two rows of RGB to Aab's
    second matrix, which MSVC sums `(p0 + p1) + p2`, and the middle row of both directions'
    second matrices, which GCC sums `p2 + (p0 + p1)` (`sub_1802f9170` and `0x34b5f0`,
    `sub_1802f8c00` and `0x34ba60`, `sub_1802f8ed0` and `0x34bcd0`); the chroma compression's
    limit is `pow(..) * reachMaxM` with MSVC and `reachMaxM * pow(..)` with GCC, its
    colourfulness `pow(..) * M` with MSVC and `M * pow(..)` with GCC, and the inverse's
    `pow(..) * M` with MSVC and `M * pow(..)` with GCC (`sub_1802fa060` and `0x34c4d0`,
    `sub_1802fa250` and `0x34c8b0`). Both compilers compute `a` as `cos_hr * M` in the copy
    of `JMh_to_Aab` that JMh to RGB inlines, against the source's `M * cos_hr`. The toes,
    `JMh_to_Aab`'s `b` and the inverse's `Mnorm * M` (GCC) order more operands per compiled
    copy (the port's `ToeFwdOrder`, `toe_inv`, `jmh_to_aab_with` and `chroma_compress_inv`
    list them), where the renderers can't show it: a NaN that reaches them has already
    reached the operand that decides the result.
  - ACES 2.0's gamut compression (`ACES2/Transform.cpp`, 932-1070): the compressed J is
    `remapped_M * slope + J_intersect` with MSVC and `slope * remapped_M + J_intersect` with
    GCC; the inverse Reinhard curve is `-(nd / (nd - 1)) * scale` with MSVC and `scale *
    -(nd / (nd - 1))` with GCC, and GCC adds it to the threshold in the inverse, MSVC the
    threshold to it (`sub_1802f88a0` and `sub_1802f8620`, `0x34ee70` and `0x34ac80`). GCC also
    folded the source's negations in `solve_J_intersect` above the focus J (`-2c / (-s -
    root)` with `s = -b`, where MSVC computes `-2c / (b - root)`), which can change a NaN's
    sign. The other operations where two NaNs can meet (the J intersect, the hulls'
    boundaries, the smooth minimum, the threshold) order their operands per compiler too (the
    port's `solve_j_intersect`, `EstimateOrder`, `smin_scaled` and `remap_m` list them),
    where the renderers can't show it.
  Both compilers also reorder some of the source's operations the same way (for example
  `13 * L* * (u - u'n)` as `(u - u'n) * (13 * L*)`), which only matters to the port.
- **Who notices:** images whose pixels have NaNs of different signs or payloads in two or
  more channels, through these styles: the output NaN's sign and payload differ between
  Windows and Linux. For example, through a glow, a pixel with a NaN A in green, a NaN B in
  blue and a finite red comes out with A in red and blue on Windows, B on Linux.
  Through ACES 2.0's RGB to JMh (of AP0), a pixel with one NaN in red and blue and another
  in green comes out with green's NaN in J, M and h on Windows, red's on Linux.
- **A fix:** one operand order for both platforms.
- **Status:** matched in `p2-ff-cpu` (the glows, 2.3b) and `p2-ff-cpu-2` (2.3c1, the HSYs in
  2.3c2), and `p2-aces2-cpu` (ACES 2.0's JMh model and chroma compression, 2.4e1; its gamut
  compression, 2.4e2), each wheel's order per platform (`cfg(target_os)`). The battery's NaN
  buffers and `fixed_function_oracle.rs`'s `nan_combinations_match_the_wheel` (every
  combination of NaNs, finite values and infinities in red, green and blue) compare them with
  the wheel on both platforms.

### I-82. RGB to HSV gives an all-negative-infinity pixel's saturation a different sign per platform

- **Upstream:** for extended-range input, `Renderer_RGB_TO_HSV::apply` computes the saturation
  as `(rgb_max - rgb_min) / -rgb_min` (`ops/fixedfunction/FixedFunctionOpCPU.cpp:1521-1524`).
  MSVC divides by the negated minimum; GCC computes `-(rgb_max - rgb_min) / rgb_min` (Linux
  wheel `0x35398c`: `subss`, `xorps` with the sign bit, `divss`), the same value for every
  input but a NaN difference, whose sign it flips. The difference is a NaN when the maximum
  and the minimum are the same infinity: for a pixel whose three channels are -Inf (where
  `-rgb_min > rgb_max` holds), the saturation is the x86 default NaN (`0xffc00000`) on Windows
  and its positive twin (`0x7fc00000`) on Linux.
- **Who notices:** images with all-negative-infinity pixels through `RGB_TO_HSV`.
- **A fix:** one formula for both platforms.
- **Status:** matched in `p2-ff-cpu-2` (2.3c1, `RendererRgbToHsv`, `cfg(target_os)`); the
  battery's specials and `nan_combinations_match_the_wheel` compare it on both platforms.

### I-83. The gamma-log styles give a NaN a different sign per platform

- **Upstream:** `Renderer_LIN_TO_GAMMA_LOG::apply` and `Renderer_GAMMA_LOG_TO_LIN::apply`
  restore the sign below the mirror point with `value * std::copysign(1.0f, mirrorin)`
  (`ops/fixedfunction/FixedFunctionOpCPU.cpp:2271, 2314`), where `value` comes from
  `|mirrorin|`, so for a NaN pixel it is a positive NaN. MSVC builds `±1.0f` and multiplies
  (Windows wheel `0x18018d39e`, `0x18018cd22`): the NaN keeps its sign. GCC turns the product
  into its `xorsign` pattern, `value ^ signbit(mirrorin)` (Linux wheel `0x354dbb`,
  `0x355077`): the NaN's sign flips where the input's is set, so a NaN pixel's output takes
  the pixel's sign. Every value other than a NaN gives the same bits on both. It is the mirror
  Gamma styles' difference (I-60) in another renderer.
- **Who notices:** images with negative NaNs (the sign bit set) through a
  `FIXED_FUNCTION_LIN_TO_GAMMA_LOG` transform, in either direction: the output NaN is positive
  on Windows and negative on Linux.
- **A fix:** one rule for both platforms, e.g. always the input's sign.
- **Status:** matched in `p2-ff-cpu-2` (2.3d1, `times_copysign_one`, `cfg(target_os)`); the
  battery's specials and NaN buffers compare it bit for bit on both platforms.

### I-84. PQ without fast math differs between Windows and Linux

- **Upstream:** with fast math off, `GetFixedFunctionCPURenderer` picks the SSE PQ renderers
  with `_mm_pow_ps` where the compiler is MSVC 2019 or later with AVX enabled, and the scalar
  `Renderer_LIN_TO_PQ<float>` and `Renderer_PQ_TO_LIN<float>` with `powf` elsewhere
  (`ops/fixedfunction/FixedFunctionOpCPU.cpp:2557-2590`). MSVC compiles `_mm_pow_ps` to
  SVML's `__vdecl_powf4`, so the Windows wheel computes PQ with SVML and the Linux wheel with
  glibc's `powf`: different results in about 1.7% of the values (every `f32` measured): up to
  339 ulp for `LIN_TO_PQ`; for `PQ_TO_LIN`, up to 3,356,700 ulp up to 1 in magnitude (near
  black, where the outputs are subnormal), and without bound above 1, through its pole near
  1.992, where the two give infinities and NaNs at different inputs (W0001).
- **Who notices:** `FIXED_FUNCTION_LIN_TO_PQ` transforms, in either direction, with fast math
  off: Windows and Linux give different values.
- **A fix:** one renderer and one `pow` on every platform.
- **Status:** matched in `p2-ff-cpu-2` (2.3d2) on Linux, bit for bit; on Windows the port
  calls `powf` where the wheel calls SVML (W0001: within its bound, and for `PQ_TO_LIN` above 1
  in magnitude any difference, split by range in 2.3d3).

### I-120. A color space transform's text runs the data bypass into the destination

- **Upstream:** `operator<<(std::ostream &, const ColorSpaceTransform &)` prints
  `dataBypass=0` straight after the destination's name, without a separator
  (`transforms/ColorSpaceTransform.cpp:138-151`):
  `<ColorSpaceTransform direction=forward, src=a, dst=bdataBypass=0>`. The flag prints only
  when it is off, as a C++ `bool` (`0`).
- **Who notices:** anyone reading `repr()` of a transform that processes data color spaces; a
  destination whose name ends in `dataBypass=0` prints the same as one without the flag.
- **A fix:** `, dataBypass=false`.
- **Status:** matched in `p3-transforms` (3.1a), checked against the wheel in
  `crates/ocio/tests/config_transforms_oracle.rs`.

### I-121. A display view transform's text doubles its separators

- **Upstream:** `operator<<(std::ostream &, const DisplayViewTransform &)` ends the view with
  `", "`, and each bypass it prints starts with another `", "`
  (`transforms/DisplayViewTransform.cpp:154-171`): `... view=v, >` without bypasses, and
  `... view=v, , looksBypass=1, dataBypass=0>` with both. The flags print as C++ `bool`s
  (`1`, `0`).
- **Who notices:** anyone reading `repr()` of a display view transform.
- **A fix:** one separator before each field, and none before `>`.
- **Status:** matched in `p3-transforms` (3.1a), checked against the wheel in
  `crates/ocio/tests/config_transforms_oracle.rs`.

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

### I-85. The gamma-log shader subtracts the gamma segment's offset

- **Upstream:** the CPU renderer of `LIN_TO_GAMMA_LOG` computes the gamma segment as
  `slope * pow(E + offset, power)` (`ops/fixedfunction/FixedFunctionOpCPU.cpp:2265`, as the
  comment in the shader writer says too), and its inverse subtracts the offset after the
  power. The shader writes `slope * pow(E - offset, power)`
  (`ops/fixedfunction/FixedFunctionOpGPU.cpp:2024-2025`), while its inverse matches the CPU's.
- **Who notices:** `FIXED_FUNCTION_LIN_TO_GAMMA_LOG` transforms whose gamma segment has an
  offset (the fifth parameter; the Rec.2100 HLG curve's is 0), forward, on the GPU: the shader
  gives other values than the CPU, and its inverse doesn't undo it.
- **A fix:** `E + offset` in the shader.
- **Status:** matched in `p2-ff-gpu` (2.3g2); `fixed_function_op_gpu_oracle.rs` compares a
  curve with an offset with the wheel's shader.

### I-86. The ACES 2.0 gamut compression shader compresses a J at or below 0

- **Upstream:** the CPU's `gamut_compress_fwd` and `gamut_compress_inv` return `(0, 0, h)` for
  a J at or below 0 (`ops/fixedfunction/ACES2/Transform.cpp:1099, 1119`). The shader's
  compression function only returns early for an M at or below 0 or a J above the limit
  (`ops/fixedfunction/FixedFunctionOpGPU.cpp:1175`), so it compresses such a JMh.
- **Who notices:** `FIXED_FUNCTION_ACES_GAMUT_COMPRESS_20` and
  `FIXED_FUNCTION_ACES_OUTPUT_TRANSFORM_20` transforms on the GPU, for JMh values with a
  negative or zero J and a positive M: the GPU's output differs from the CPU's.
- **A fix:** the CPU's test in the shader.
- **Status:** matched in `p2-aces2-gpu` (2.4g); `fixed_function_op_gpu_oracle.rs` compares the
  shader with the wheel's.

### I-87. The inverse double-log shader's break points are NaNs of each platform's sign

- **Upstream:** `Add_DOUBLE_LOG_TO_LIN` computes the break points in log space as
  `logSlope * std::log(linSlope * break + linOff) + logOff` in `double`
  (`ops/fixedfunction/FixedFunctionOpGPU.cpp:2180-2181`), and validation allows a negative
  argument (it checks only the base and the breaks' order). The Windows wheel calls the
  UCRT's `log`, which returns the negative x86 default NaN for it; the Linux wheel links
  `log@GLIBC_2.2.5` (`Add_DOUBLE_LOG_TO_LIN` at 0x3711f0), whose compatibility wrapper returns
  a positive NaN. The shader then compares the pixel with `vec3(-nan, -nan, -nan)` on Windows
  and `vec3(nan, nan, nan)` on Linux. The CPU renderer computes its breaks with `logf`, which
  both wheels link in its current version, and the other `log` calls of the gamma-log and
  double-log writers and renderers take the log base, which validation keeps positive. The
  gamma-log inverse shader's `std::pow(double, double)` (its break and mirror,
  `FixedFunctionOpGPU.cpp:2055-2056`) is `pow@GLIBC_2.2.5` on Linux too, but its wrapper gives
  a negative base with a non-integer power the same NaN as the current symbol.
- **Who notices:** anyone comparing the shader text of such a `FIXED_FUNCTION_LIN_TO_DOUBLE_LOG`
  inverse between a Windows and a Linux machine (the shader's results don't depend on the sign).
- **A fix:** refuse a negative log argument at a break in `validate`, or one `log` on every
  platform; either changes the port's results on at least one of them.
- **Status:** matched in `p2-ff-gpu` (`log_as_linked` in `fixed_function_op_gpu.rs`, as
  `log2_glibc_2_2_5` does for I-70); `fixed_function_op_gpu_oracle.rs`,
  `double_log_break_points_match_the_wheel`, and for the `pow`
  `gamma_log_negative_gamma_bases_match_the_wheel`, on both platforms.

### I-137. The GPU has only the fast inverse of a 1D LUT, and no OSL

- **Upstream:** `Lut1DOp::extractGpuShaderInfo` replaces an inverse 1D LUT with its fast
  forward LUT (`MakeFastLut1DFromInverse`) whatever the optimization flags say
  (`ops/lut1d/Lut1DOp.cpp:157-175`, with a TODO for an exact GPU inverse), so a GPU processor
  made with `OPTIMIZATION_NONE` (no `OPTIMIZATION_LUT_INV_FAST`) still writes the fast LUT, a
  resampling of the inverse on a lookup domain, where the CPU processor inverts exactly.
  `GetLut1DGPUShaderProgram` refuses the OSL translation ("The Lut1DOp is not yet supported
  by the 'Open Shading language (OSL)' translation", `ops/lut1d/Lut1DOpGPU.cpp:148-151`).
- **Who notices:** GPU renders of inverse 1D LUTs compared with the CPU's, and OSL users of
  1D LUTs.
- **A fix:** an exact inverse renderer on the GPU, and an OSL translation.
- **Status:** matched in `p2-lut1d-gpu` (2.1h, `crates/ocio-gpu/src/ops/lut1d/lut1d_op_gpu.rs`);
  checked against the wheel (`crates/ocio-gpu/tests/lut1d_op_gpu_oracle.rs`, the inverse LUTs
  at `OPTIMIZATION_NONE`, and OSL in every case).

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

### I-223. Python makes enum values that no enumerator has

- **Upstream:** pybind11 constructs an enum from any integer its underlying type holds
  (`OCIO.TransformDirection(5)`, `OCIO.BitDepth(99)`): `static_cast` of a value outside an
  unscoped enum's range, undefined in C++17 for an enum without a fixed type, which both
  wheels keep as the integer. The value reads as `???` (`<TransformDirection.???: 5>`), and
  the string-conversion functions give what their last line gives
  (`ParseUtils.cpp:113-519` @ v2.5.2): `TransformDirectionToString` "inverse",
  `GetInverseTransformDirection` `TRANSFORM_DIR_FORWARD`, `CombineTransformDirections`
  `TRANSFORM_DIR_INVERSE`, `BitDepthToString` "unknown", `BitDepthToInt` 0, `CDLStyleToString`
  "asc", `RangeStyleToString` "Clamp"; `GpuLanguageToString`, `FixedFunctionStyleToString`,
  `GradingStyleToString`, `ExposureContrastStyleToString` and `NegativeStyleToString` raise
  (seen through the wheel in `conversions_oracle.rs`).
- **Who notices:** Python code that makes an enum from a number outside its values.
- **A fix:** refuse the number when the enum is constructed, as a `ValueError`.
- **Status:** matched in `p6-core` (6.2b); `enums_oracle.rs`, `conversions_oracle.rs`. I-31
  is one consequence of it.

### I-225. Python takes negative values for an enum on Windows only

- **Upstream:** OCIO's enums without a fixed type (all but `OptimizationFlags`,
  `ProcessorCacheFlags` and `GpuShaderCreator.TextureDimensions`) have no negative
  enumerators, so their underlying type is the compiler's choice: MSVC's is `int`, GCC's
  `unsigned int`. pybind11 converts the `int` passed to an enum's constructor (`__init__`,
  `__setstate__`) to that type, so `OCIO.LoggingLevel(-1)` makes a value (`???`, -1) in the
  Windows wheel and raises the constructor's TypeError in the Linux one, and
  `OCIO.LoggingLevel(4294967295)` the other way round (seen through both wheels in
  `enums_oracle.rs`).
- **Who notices:** Python code that makes an enum from a number outside its values (I-223)
  and runs on both platforms.
- **A fix:** refuse every number that isn't one of the enum's values (I-223's fix), which
  removes the difference.
- **Status:** matched in `p6-core` (6.2b follow-up); `enums_oracle.rs`.

### I-236. A huge `len()` makes a list argument fail with the C++ runtime's texts

- **Upstream:** pybind11's `std::vector` caster reserves the sequence's `len()` elements before
  reading any (`list_caster::reserve_maybe`), and its `std::array` caster reserves a second
  `len()` for its items. Through the wheels, a sequence whose `__len__` returns more than the
  vector's `max_size()` makes the call raise `ValueError` "vector too long" on Windows (MSVC's
  STL) and "vector::reserve" on Linux (libstdc++); one the allocator can't serve raises
  `MemoryError` "bad allocation" or "std::bad_alloc"; and on Windows only, a size within 47
  bytes of the address space raises `MemoryError` "bad array new length" (MSVC adds 47 bytes
  of alignment to large allocations). The limits differ too: MSVC's `max_size()` is
  `SIZE_MAX / sizeof(T)` capped at `PTRDIFF_MAX`, libstdc++'s `PTRDIFF_MAX / sizeof(T)`, so
  `2**61` floats are "vector::reserve" on Linux and "bad allocation" on Windows (seen through
  the wheel in `dispatch_oracle.rs`).
- **Who notices:** Python code that passes a sequence whose `__len__` lies (or is huge) to a
  function taking a list.
- **A fix:** read the items without reserving, or refuse a `len()` beyond the items read, with
  one text on both platforms.
- **Status:** matched in `p6-core` (`crates/ocio-py/src/dispatch.rs`, `reserve`, which reserves
  the same bytes through the same allocator and raises the platform's texts);
  `dispatch_oracle.rs`, `vectors_load_sequences_as_the_wheels`,
  `arrays_load_sequences_as_the_wheels`.

### I-237. A NUL in an argument's `repr()` cuts pybind11's error short

- **Upstream:** pybind11's dispatcher builds the TypeError of a call no overload takes as a C++
  string ("... Invoked with: " and each argument's `repr()`, each keyword's name) and sets it
  with `PyErr_SetString`, which reads it up to its first NUL: through the wheel,
  `GetEnvVariable(x)` with a `repr(x)` of `'a\x00b'` says "Invoked with: a", and
  `GetEnvVariable(**{'a\x00b': 1})` "Invoked with: kwargs: a" (`dispatch_oracle.rs`).
- **Who notices:** Python code that reads the error of a refused call whose arguments' reprs or
  keyword names hold a NUL.
- **A fix:** set the whole message.
- **Status:** matched in `p6-core` (`crates/ocio-py/src/dispatch.rs`, `incompatible`);
  `dispatch_oracle.rs`, `module_functions_answer_as_the_wheels`.

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
  replacement), and an unpaired surrogate in the environment becomes U+FFFD: a variable set
  with such bytes reads back with U+FFFD, and two names that differ only there are one
  variable. Linux passes the bytes. The C runtimes set variables by their own rules:
  `_wputenv_s` (Windows) builds `name=value` and splits it at its first `=`, so
  `SetEnvVariable("A=B", "C")` sets `A` to `B=C` and `UnsetEnvVariable("Q=R")` sets `Q` to `R=`,
  refuses a name that starts with `=`, and removes a variable set to ""; Linux's `setenv`
  refuses a name holding `=` and keeps an empty value. Windows compares names without case
  twice over: the system (`GetEnvironmentVariableW`, which `GetEnvVariable` reads) folds every
  code unit by its own table (I-117), the C runtime's list (`_wenviron`, which a context's
  `loadEnvironment` reads) folds ASCII only, so after setting a lowercase "e acute" name and
  then its uppercase, the first reads the second's value and the context loads both. Linux
  compares exactly.
- **Who notices:** configs and environments with names or paths that aren't UTF-8, names that
  differ only in case or hold `=`, used on both platforms.
- **A fix:** none: these are the platforms' rules.
- **Status:** matched in p3-context (3.5a, and its fix chunk after the verifier's review): the
  conversions checked against the system for every string of up to 4 bytes or units over each
  class, the C runtime's list against `_wenviron`/`environ`
  (`crates/ocio-ops/tests/platform_crt.rs`), and the environment functions against the wheel
  (`crates/ocio/tests/env_oracle.rs`).

### I-114. Windows device names as files

- **Upstream:** `CreateFileContentHash` asks `_wstat` whether a file exists. On Windows it finds
  some device names, in any directory that exists: `nul`, `aux`, `com1`, `conin$`, `nul:` (with
  `st_dev` -1), but not `con` or `lpt1` (as probed on Windows 11). `_wstat` also opens verbatim
  (`\\?\`) and device (`\\.\`) paths and UNC paths (`st_dev` then the current drive's), and
  refuses a drive letter alone (`C:`).
- **Who notices:** a config whose file references name such devices; `resolveFileLocation` then
  finds them.
- **A fix:** treat device names as missing on Windows.
- **Status:** matched (p3-context 3.5a, corrected in a fix chunk after the verifier's review):
  the port opens the path as `_wstat` does, and takes a file whose information can't be read for
  a device; checked against `_wstat` in `crates/ocio-ops/tests/platform_crt.rs` and against the
  wheel in `crates/ocio/tests/context_oracle.rs`.

### I-115. pystring's indices are `int`

- **Upstream:** pystring computes positions and lengths as `int` (`(int) str.size()` and the
  `ADJUST_INDICES` arithmetic, pystring.cpp @ v1.1.4), so for a path of 2^31 bytes or more the
  length wraps.
- **Who notices:** nobody in practice: OCIO passes it file paths and names.
- **A fix:** none needed. The port computes with the true lengths (`i64`).
- **Status:** not matched (p3-context 3.5b), for the owner: matching would mean computing every
  pystring index as a wrapping `int`, whose results then index outside the string (undefined
  behaviour, which the general rule turns into errors), for inputs no one passes.

### I-116. Windows reads the working directory in the ANSI code page

- **Upstream:** `GetCwd` (`PathUtils.cpp:131-150` @ v2.5.2) calls `_getcwd` on Windows, which
  gives the path in the process's ANSI code page (cp1252 on most Western systems), not UTF-8,
  and with `?` for characters the code page lacks. `AbsPath` joins a relative file name to it;
  OCIO's other paths are UTF-8 and go to the system through `Utf8ToUtf16`. Linux gives the
  bytes.
- **Who notices:** a config loaded by a relative path (`Config::CreateFromFile("x.ocio")`) from
  a working directory whose name isn't ASCII, on Windows: the config's working directory is the
  ANSI path, which `Utf8ToUtf16` then misreads.
- **A fix:** `_wgetcwd` and `Utf16ToUtf8`.
- **Status:** not matched (p3-context 3.5f, `crates/ocio/src/path_utils.rs`, `get_cwd`): the
  port gives the path as UTF-8, as the rest of OCIO's paths are; the ANSI conversion needs a
  system call the port's crates can't make (`unsafe`). ASCII paths are the same on both. For
  the owner.

### I-117. Windows folds environment variable names by the system's table

- **Upstream:** the Windows wheel reads a variable with `GetEnvironmentVariableW`, which finds
  its name ignoring case as `RtlUpcaseUnicodeChar` folds each UTF-16 code unit, from the NLS
  data of the Windows it runs on (`Platform.cpp:48-97` @ v2.5.2). That table is Unicode's
  simple uppercase mapping of an older Unicode version: on the reference machine (Windows 11,
  build 26300) it differs from Unicode's current mapping at 252 code units (225 it leaves
  alone, such as the dotless i, the micro sign and the long s, whose mapping Unicode added
  later; 27 Greek letters with iota subscript, which it maps to their titlecase form). Another
  Windows version can have another table, so which names are the same variable can change with
  a Windows update.
- **Who notices:** names that differ only in the case of such characters.
- **A fix:** none: it is the system's rule.
- **Status:** matched on the reference machine (p3-context, the fix chunk after the verifier's
  review): the port folds with Rust's simple uppercase mapping and the reference machine's
  252 exceptions (`crates/ocio-ops/src/platform_nls_upcase.rs`); `tests/platform_crt.rs`
  checks every code unit against `RtlUpcaseUnicodeChar`, so a machine with another table fails
  there. The alternative, calling the system (FFI, `unsafe`), is the owner's decision.

### I-122. A file rule's regular expression means different things on Windows and Linux

- **Upstream:** OCIO compiles and matches a file rule's regular expression with the C++
  library's `std::regex` (ECMAScript): MSVC's STL in the Windows wheel, GCC 14's libstdc++ in
  the Linux wheel. They accept different expressions, read some differently, and word their
  errors differently. In the Linux wheel:
  - a quantifier may follow a quantifier (`a**`, `a{2}{3}`); `]` and `}` are ordinary
    characters; `\cX` is `X` itself; `\0` is NUL and the digits after it ordinary characters;
    `\uNNNN` keeps the low byte; a range compares its ends as signed `char`s
    (`[\x7f-\x81]` is refused);
  - `^` matches only at the start of the expression being matched and `$` only at the end of
    the text, never at a `\n` (the Windows wheel also matches them after and before a `\n`);
    a lookahead is matched as an expression of its own that starts where it stands, so `^`
    matches there and `\b`/`\B` see no character before it (`a(?=^b)b` and `a(?=\bb)b`
    match `ab`);
  - a back reference to a group that matched nothing fails (Windows: matches empty); a
    repetition keeps its groups' captures from one iteration to the next; an iteration may
    match empty (at most twice at a position); a brace copies its body (`{n,m}`: `n` copies
    then `m - n` optional ones; `{n,}`: `n` copies then a loop), and each copy's loops count
    those entries apart (`(?:(?:()|()|())?){3}\1\2\3` matches the empty text);
  - collating elements and equivalence classes take POSIX names (`[[.space.]]`); the errors are
    libstdc++'s texts, or `regex_error` alone: the text that the system's `libstdc++.so`
    gives a `regex_error` built from a code only, at run time (the wheel links it
    dynamically).
  In the Windows wheel, a collating element matches when `_Lookup_coll`'s comparison, which
  stops after the first character that differs, ends at the end of the text: `[[.a.]]` matches
  any one character that ends the text (`b[[.a.]]` matches `bz`), and `[[.ab.]]` matches `z`
  but not `a`; and a match gives up with `error_stack` past 600 nested matches and
  `error_complexity` past ten million steps.
- **Who notices:** configs whose regex file rules use these constructs, shared between Windows
  and Linux.
- **Decided** (D12): the port does what each wheel does.
- **Status:** matched in `p3-regex`: the MSVC parser (3.9a) and matcher (3.9b), the libstdc++
  parser and its NFA state limit (3.9c) and matcher (3.9d, with the lookahead and brace-copy
  rules of the fix chunk after its verifier).

### I-133. A glob rule's expression is "sanitized" differently on Windows and Linux

- **Upstream:** a glob rule's pattern and extension become a regular expression, which
  `SanitizeRegularExpression` (`FileRules.cpp:30-49`) rewrites with two `regex_replace`
  calls, to turn `*?`, `?*` and `**` into one `.*`. The first expression,
  `(\.\*\.^\*)+|(^\\\.\.\*)+`, doesn't do what its comments say: its first alternative
  requires a `^` in the middle of the text, and its second matches a literal `\..*` at the
  start of the text, which the built expression never has (it starts with `^(`). So it never
  changes an expression in the Linux wheel. In the Windows wheel, MSVC's `^` also matches
  after a line feed (I-122), so a glob holding a line feed then `.*` (the pattern `\n.*`,
  converted to `\n\..*`) has its `\..*` replaced by `.*`, which changes what the rule matches
  (`\nfoo.exr` matches the pattern `\n.*` and extension `exr` on Windows, not on Linux). The
  second expression, `(\.\*)+`, merges runs of `.*` on both; after a backslash, which escapes
  the first `.`, that changes what the glob matches: the pattern `\**` builds `\.*.*` (dots,
  then anything), merged into `\.*` (dots only), so with the extension `exr` the rule takes
  `...exr` but not `abc.exr`, in both wheels.
- **Who notices:** glob rules with a line feed in their pattern, shared between Windows and
  Linux.
- **A fix:** drop the first expression, or anchor nothing in it.
- **Decided** (D12): the port does what each wheel does.
- **Status:** matched in `p3-rules` (3.9e, `std_regex::regex_replace` and
  `crates/ocio/src/file_rules.rs`); the messages of globs whose sanitized expression doesn't
  compile show it, checked against both wheels in `crates/ocio/tests/file_rules_oracle.rs`;
  the paths such globs send to each rule in `crates/ocio/tests/config_oracle.rs`
  (`sanitized_globs_on_a_config_match_the_wheel`).

### I-160. A LUT header's `%d` out of `int`'s range

- **Upstream:** the spi1d, Iridas cube and Discreet 1DL readers read their header numbers with
  `sscanf`'s `%d` (`FileFormatSpi1D.cpp`, `FileFormatIridasCube.cpp`, `FileFormatDiscreet1DL.cpp`),
  and C leaves a number outside `int`'s range undefined (C17 7.21.6.2p10). Both wheels' C
  runtimes do the same: they read the number as `strtoll` would (saturating to the 64-bit
  range) and store its low 32 bits, so `Length 4294967297` is a length of 1, and
  `Length 9223372036854775808` one of -1.
- **Who notices:** files with absurd sizes: they read as other sizes rather than failing.
- **A fix:** read the numbers with a range check.
- **Status:** matched in `p4-parse` (4.0b, `ocio-ops/src/utils/cscan.rs`), compared with both C
  runtimes over generated scans (`crates/ocio-ops/tests/cscan_crt.rs`).

### I-161. `sscanf` reads a lone sign and a 0xFF byte differently on Windows and Linux

- **Upstream:** the same readers call `sscanf_s` on Windows (UCRT) and `sscanf` on Linux
  (glibc). For `%d`:
  - a `+` or `-` at the end of the line is an input failure in the UCRT (the call returns
    `EOF`, -1, when nothing was stored before) and a matching failure in glibc (it returns 0);
  - the UCRT reads a 0xFF byte as `EOF` (a signed `char` of -1) and consumes it: where a digit
    or sign is needed it fails there (an input failure if it was the last byte), and after
    digits it ends the number and is lost, so `12\xff34` reads as 12 then 34. glibc reads 0xFF
    as any other byte.
  The readers compare the count with the one they expect, so valid files, and most malformed
  ones, read alike on both wheels; the port follows each runtime, so the rest do too. Both
  scan in the "C" locale here; the Windows wheel under Python scans in code page 1252's, where
  0xA0 is white space too (deviation D-1).
- **Who notices:** malformed LUT files, read on both platforms.
- **A fix:** none needed for valid files; a hand-written header parser would remove the
  difference.
- **Decided** (D12): the port does what each wheel does.
- **Status:** matched in `p4-parse` (4.0b, `ocio-ops/src/utils/cscan.rs`), compared with each
  platform's C runtime (`crates/ocio-ops/tests/cscan_crt.rs`).

### I-162. Text LUT files read differently on Windows and Linux

- **Upstream:** `getLutData` (`FileTransform.cpp:218-220`) opens the file of every format that
  isn't binary in text mode (`std::ios_base::in`). On Windows, MSVC's `std::ifstream` reads
  through the C runtime's text-mode `FILE`: `CR LF` reads as `LF`, a lone `CR` is kept (so
  `CR CR LF` reads as `CR LF`), and a `0x1A` byte ends the file, whatever follows. On Linux the
  reader gets the file's bytes, `CR` included. Through a `ConfigIOProxy` (a `std::stringstream`)
  it gets the bytes on both. So a file with Windows line ends, or with a `0x1A` in it, can read
  differently on the two platforms; a directory, which glibc opens, fails to read on Linux and
  fails to open on Windows.
- **Who notices:** LUT files with `CR LF` line ends whose readers keep the `CR` (in a name or a
  last token), and files with a `0x1A` byte, shared between Windows and Linux.
- **A fix:** open text formats in binary mode and treat `CR LF` as a line end on both platforms.
- **Decided** (P4-6, D12): the port does what each wheel does.
- **Status:** matched in `p4-registry` (`crates/ocio/src/fileformats/input_stream.rs`, the open
  of 4.0c), compared with the platform's C runtime (`fopen` and `fread`) on generated files in
  `crates/ocio/tests/input_stream_crt.rs`.

### I-163. A file's LUT changes in the file cache once a processor uses it

- **Upstream:** the file cache keeps what a reader read (`GetCachedFileAndFormat`,
  `FileTransform.cpp:801-888`), and the spi1d reader's `buildFileOps` hands the cached
  `Lut1DOpDataRcPtr` itself to the forward LUT op (`HandleLUT1D` never clones a 1D LUT, and
  `CreateLut1DOp` shares the pointer forward). The processor then finalizes its ops in place:
  `Lut1DOpData::finalize` calls `adjustColorComponentNumber`, which marks the cached LUT as one
  component when its three channels are equal. A later inverse processor of the same file
  clones that LUT, and `initializeFromForward` makes only its first channel monotonic; from a
  fresh cache it makes all three. So the inverse of a non-monotonic LUT holds other values in
  green and blue, and has another cache ID, once a forward processor of the file was made in
  the process (probed 2026-10-08: for the entries `0, 1, 0.5, 1`, green and blue keep `0.5`
  after a forward processor and become `1` from a fresh cache; the inverse processor's cache
  ID is `91c3bdd33553011dfc45941c59ca1de5` after a forward processor and
  `8caceefb8952f416f8747d8a34fc639a` from a fresh cache; the pixels agree, as the one-component
  LUT renders from red). Every reader that builds its 1D LUT ops through `HandleLUT1D` hands
  over the cached LUT the same way: the Discreet 1D LUT (`FileFormatDiscreet1DL.cpp:776-783`),
  ICC (`FileFormatICC.cpp:793-862`), Iridas `.cube` (`FileFormatIridasCube.cpp:538-572`),
  Resolve `.cube` (`FileFormatResolveCube.cpp:767-800`), `.3dl` shaper
  (`FileFormat3DL.cpp:612-639`), Truelight shaper (`FileFormatTruelight.cpp:380-409`),
  Houdini 1D and prelut (`FileFormatHDL.cpp:849-896`) and cineSpace prelut and 1D LUT
  (`FileFormatCSP.cpp:875-918`) readers. The CTF and CLF readers, which share op data with
  the cache too, likely do the same.
- **Who notices:** applications that make processors of a non-monotonic LUT file in both
  directions: the inverse's cache ID and data depend on the order.
- **A fix:** clone the cached op data when building ops, or finalize copies.
- **Decided** (owner, 2026-10-08, as for I-174): the port keeps copies. Its processors never
  change the cached file, so they give what the wheel gives from a fresh cache, and differ from
  the wheel's inverse processors made after a forward one in the same process (their cache ID
  and the inverse LUT's green and blue entries). The oracle tests run every request after
  `ClearAllCaches` (`crates/ocio/tests/common/lut_files.rs`).
- **Status:** not reproduced (p4-registry 4.2a2).

### I-164. The Discreet 1D LUT header check reads past the line's end

- **Upstream:** `IMLutGet` checks the header with `std::string subStr(InString, 5)`
  (`FileFormatDiscreet1DL.cpp:435`): the first 5 bytes of the 200-byte line buffer, whatever
  the line's length. For a line shorter than 5 bytes it reads past the line's NUL, into bytes
  of earlier lines or never written (an uninitialized `char` array, whose read is undefined).
  Both wheels give the same result whatever those bytes are: the copy holds the line's NUL
  among its 5 bytes, so it is never `"lut: "` in lower case, and the line is a syntax error.
- **Who notices:** nobody: a header line shorter than 5 bytes is an error either way.
- **A fix:** compare the line itself (`StartsWith`).
- **Status:** matched in `p4-registry` (4.3d, `crates/ocio/src/fileformats/file_format_discreet1dl.rs`):
  a line shorter than 5 bytes never matches; checked against the wheel with headers around
  that length (`crates/ocio/tests/file_format_discreet1dl_oracle.rs`).

### I-165. An ICC `mluc` string of 2^30 characters or more wraps its byte count

- **Upstream:** SampleICC's `mluc` reader reads a record's string with
  `Read16(istream, &unicodeStr[0], nNumChar)` (`ext/sampleicc/src/include/iccProfileReader.h:
  366-369` @ OCIO v2.5.2), whose byte count is `num << 1` on an `icInt32Number`: for 2^30
  characters or more the doubled count no longer fits. C++17 defines the shift there (the
  value in the unsigned type, converted to `int`, which both compilers wrap), so it is a
  negative count, which reads nothing (I-166). `Read16` then returns half that negative
  count, never the length the record claims, and the tag's reader fails.
- **Who notices:** nobody: such a record claims a string of 2 GiB or more.
- **A fix:** check the length against the tag's size before reading.
- **Status:** matched in `p4-readers` (4.7a, `crates/ocio/src/fileformats/icc/
  icc_profile_reader.rs`): the count wraps as the wheels' does.

### I-166. SampleICC reads a negative count differently on Windows and Linux

- **Upstream:** `SampleICC::Read8` (`iccProfileReader.h:144-156` @ OCIO v2.5.2) passes its
  `icInt32Number` count to `istream::read`, and the `desc` reader passes a 32-bit text size
  of 2^31 or more there as a negative count (`iccProfileReader.h:259-264`), as the `mluc`
  reader does a wrapped one (I-165). MSVC's `istream::read` reads nothing for a count below 1
  and leaves the stream good, so `Read8` returns the count; libstdc++'s asks the buffer for no
  bytes, finds it didn't get the count, and fails the stream. So on Windows a `desc` tag of
  such a size reads as an empty description (after allocating up to 4 GiB, U-70), and the
  profile reads on; on Linux the tag's reader fails, and so does every later read of the
  profile (a failed stream doesn't seek), which gives `Illegal matrix tag in ICC profile.`
  An `mluc` string of 2^30 characters or more fails its reader on both, but leaves the
  stream good on Windows only.
- **Who notices:** nobody but a corrupt profile: the size claims 2 GiB or more.
- **A fix:** check the size against the tag's size before reading.
- **Status:** matched in `p4-readers` (4.7a, `crates/ocio/src/fileformats/icc/
  icc_profile_reader.rs`), per platform; checked against the wheel on both
  (`crates/ocio/tests/file_format_icc_oracle.rs`, "a desc of size 2^32 - 1").

### I-167. ICC parametric curves past a `long`'s range check differently on Windows and Linux

- **Upstream:** `ValidateParametricCurve`'s `QuantizeF` (`src/OpenColorIO/fileformats/
  FileFormatICC.cpp:298-302` @ v2.5.2) rounds with `std::lround(float)`, the C runtime's
  `lroundf`, to a `long`: 32 bits on Windows, 64 on Linux. For a NaN (the power law of a
  negative argument, checked only after the discontinuity check) or a value out of range,
  the UCRT returns 0 (`LONG_MIN` for exactly 2^31) and glibc `LONG_MIN`. So a type 3 or 4
  curve whose power law has a negative argument, and whose linear segment quantizes to 0 or
  less at `d` (`c * d` of type 3, `c * d + f` of type 4: `c = 0`, or `d < 0`), fails with
  `Expecting no negative arguments to the power law.` on Windows and `Expecting no negative
  discontinuity at linear segment boundary.` on Linux; where the segment quantizes above 0
  (`{2.4, 0.5, -0.6, 0.1, 0.5}`), both give the discontinuity error. A curve with a segment
  past 2^31 / 1023 checks (and warns) differently too.
- **Who notices:** nobody but malformed profiles: their curves are errors or out of any
  display's range either way.
- **A fix:** check the parameters before computing, and quantize in a type that holds them.
- **Status:** matched in `p4-readers` (4.7b, `lround` in
  `crates/ocio/src/fileformats/file_format_icc.rs`, checked against the platform's `lroundf`);
  checked against the wheel on both (`crates/ocio/tests/file_format_icc_oracle.rs`).

### I-224. `GetEnvVariable` of an empty name returns the previous call's value

- **Upstream:** `GetEnvVariable` (`Platform.cpp:23-28` @ v2.5.2) returns a `thread_local`
  string that `Platform::Getenv` fills, and `Getenv` returns before touching it when the name
  is null or empty (`Platform.cpp:50-53`). So `GetEnvVariable(nullptr)` or `("")` returns what
  the last call on that thread returned: in Python, `OCIO.GetEnvVariable(None)` right after
  `OCIO.GetEnvVariable("X")` gives the value of `X` (seen through the wheel in
  `dispatch_oracle.rs`).
- **Who notices:** code that asks for an empty variable name, such as a name read from
  somewhere that turned out empty.
- **A fix:** clear the string first, so an empty name gives "".
- **Status:** matched in `p6-core` (6.2b follow-up); `dispatch_oracle.rs`.

## File formats

### I-170. expat counts a start tag's attribute values twice

- **Upstream:** expat 2.7.2's billion laughs protection counts the bytes of each token it
  reads, once. A start tag (`<e a='x&amp;y'>`, not an empty element) is counted as one token
  in `doContent`, then `storeAtts` passes the same accounting class (`XML_ACCOUNT_DIRECT` in
  the document) to `storeAttributeValue`, which counts the tokens of each value that needs
  normalizing (a reference, or white space other than single spaces) again
  (`expat/lib/xmlparse.c:3321-3326, 3508-3509, 3956-3958, 6567-6571` @ R_2_7_2). An empty
  element's tag passes `XML_ACCOUNT_NONE` there ("token spans whole start tag"). The direct
  count, the denominator of the amplification factor, grows by those values' bytes once more.
- **Who notices:** documents over the protection's 8 MiB activation threshold whose
  amplification by internal entities is near the limit of 100: start tags with such values
  lower the factor, so a document that the limit would refuse can pass.
- **A fix:** pass `XML_ACCOUNT_NONE` for start tags too, as for empty elements.
- **Status:** matched in `p4-xml` (4.4b, `crates/ocio-formats/src/expat/xmlparse.rs`,
  `do_content`).

### I-171. The XML parser's memory limit is 40 bytes apart on Windows and Linux

- **Upstream:** expat 2.7.2's allocation tracker fails an allocation, and with it the parse
  (`XML_ERROR_NO_MEMORY`, "out of memory"), once the heap expat allocated reaches 64 MiB and
  more than 100 times the document's direct bytes (`expat_heap_increase_tolerable`,
  `expat_malloc`, `expat_realloc`, `expat/lib/xmlparse.c:812-1005` @ R_2_7_2; the defaults in
  `internal.h:152-154`). It counts the sizes of expat's own blocks: its parser structure,
  pools, hash tables, tags, attribute arrays, open entities and default attributes, each
  with an 8-byte header. They are the same in both wheels but the parser structure's, which
  holds `long` and `unsigned long` fields: 1,048 bytes as MSVC lays it out and 1,088 with GCC.
  So a document within 40 bytes of the limit (a pool block of 32 MiB, say, filled to within
  20 bytes of it) is refused on Linux and read on Windows.
- **Who notices:** only documents at the limit: tens of MiB of parser memory from a document a
  hundred times smaller, which no LUT or CDL file comes near.
- **A fix:** none needed; dropping the parser structure from the count would make the
  platforms agree.
- **Status:** matched in `p4-cdl` (`crates/ocio-formats/src/expat/heap.rs`, which counts what
  expat allocates block for block, with each platform's structure size;
  `crates/ocio/tests/xml_memory_oracle.rs` finds where families of documents start being
  refused, and both wheels agree to the byte). Before that chunk the port counted its own
  structures (the owner's decision of 2026-10-08, superseded once the exact count worked).

### I-172. The XML readers read a number out of range as 0 on Linux, and as infinity on Windows

- **Upstream:** `ParseNumber` (`fileformats/xmlutils/XMLReaderUtils.h:150-205` @ v2.5.2)
  checks `NumberUtils::from_chars`'s `invalid_argument` only: a `result_out_of_range` passes,
  with the value `from_chars` left. On Windows (`std::from_chars`) that is the overflow's
  infinity, the underflow's 0, and a subnormal read as such; on Linux (`strtod_l`, which sets
  `ERANGE` for an overflow, an underflow and a subnormal result) the value is not stored, and
  `ParseNumber`'s local stays 0. So a CTF matrix entry of `1e999` reads as `inf` on Windows
  and `0` on Linux, `-1e999` as `-inf` and `0`, and `1e-310` as `1e-310` and `0` (wheel
  probes of a CTF `Matrix` read through `FileTransform`, 2026-10-08).
- **Who notices:** CTF, CLF and CDL files with numbers past the double range, or subnormal.
- **A fix:** refuse `result_out_of_range`, or read it the same way on both platforms.
- **Status:** matched in `p4-xml` (4.4c, `crates/ocio-formats/src/fileformats/xmlutils/
  xml_reader_utils.rs`, `parse_number`); its oracle tests come with the CTF reader (4.5).

### I-173. The CDL writers escape descriptions twice

- **Upstream:** the CC, CCC and CDL writers collect the descriptions of the metadata already
  escaped for XML (`ExtractCDLMetadata`, `fileformats/cdl/CDLWriter.cpp:24-56` @ v2.5.2), and
  the formatter escapes them again as it writes them (`XmlFormatter::writeContentTag`,
  `XMLWriterUtils.cpp:68-83, 124-127`). A description "x < y & z" is written
  `x &amp;lt; y &amp;amp; z`, which reads back as "x &lt; y &amp; z" (probed 2026-10-08 with
  `GroupTransform.write` in the three formats). The id and name attributes, and the numbers,
  are escaped once.
- **Who notices:** CC, CCC and CDL files written from transforms whose descriptions hold `<`,
  `>`, `&`, `"` or `'`: each write and read adds a level of escaping.
- **A fix:** collect the descriptions as they are, and let the formatter escape them; written
  files of such descriptions change.
- **Status:** matched in `p4-cdl` (4.4e, `crates/ocio/src/fileformats/cdl/cdl_writer.rs`).

### I-174. A CDL read from a file is shared with the file cache

- **Upstream:** `CDLTransform::CreateFromFile` and `CreateGroupFromFile` return the
  transforms the file cache holds, not copies (`getCDLGroup` of the CC, CCC and CDL formats,
  `FileFormatCC.cpp:29-34`, `FileFormatCCC.cpp:32-41`, `FileFormatCDL.cpp:55-64`;
  `CDLTransform.cpp:34-118` @ v2.5.2). Editing one edits the cached file: later reads of the
  file give the edited values, and so do the processors of file transforms of it, until
  `ClearAllCaches` (probed 2026-10-08: `setSlope` on the CDL `CreateFromFile` gave, and a
  second `CreateFromFile`, `CreateGroupFromFile` and a `FileTransform`'s processor all give
  the new slope).
- **Who notices:** applications that edit a CDL they read with `CreateFromFile` or
  `CreateGroupFromFile`, and then read the file again or process it through a file
  transform.
- **A fix:** the port's `CdlTransform::from_file` and `group_from_file` return copies (the
  owner's API, `docs/cards/phase4.md` P4-5 d), so the cache never changes, as a fix would
  have it. Matching upstream would need transforms shared by handle; the Python binding
  (`ocio-py`) decides whether its objects share the cached ones.
- **Decided** (the owner, 2026-10-08): the port keeps copies; the Python binding decides later
  whether its objects share the cached ones.
- **Status:** not reproduced: the port gives copies (`p4-cdl` 4.4e,
  `crates/ocio/src/transforms/cdl_transform.rs`), which differs from the wheel after such an
  edit.

### I-175. The CDL reader drops content in four places

- **Upstream:** the CDL, CCC and CC parser (`fileformats/cdl/CDLParser.cpp` @ v2.5.2):
  - ignores character data that is a single line feed (`CharacterDataHandler`, 896-897),
    wherever it is: a description, a value or text where none belongs, which elsewhere is an
    error ("... not allowed ...") or part of the value, as long as expat hands it over alone
    (a line feed alone on its line);
  - lets a second root element inside the first, while the first has no correction yet,
    take the parsing info over (`HandleColorDecisionListStartElement`,
    `HandleColorCorrectionCollectionStartElement`, 496-580): the outer root's descriptions
    and the corrections after the inner root ends are lost (probed 2026-10-08: a collection
    holding a collection with one correction, then a second correction, reads as the first
    correction alone, with the inner collection's description);
  - drops the descriptions of a `ColorDecision` (its element keeps none);
  - does not know `ColorCorrectionRef`: a `ColorDecision` that refers to a correction holds
    none, with an "Unknown element" warning (probed).
- **Who notices:** CDL files with such content: corrections and descriptions go missing
  without an error.
- **A fix:** keep the line feed as data where the element takes data; refuse a nested root;
  keep the decision's descriptions; resolve `ColorCorrectionRef` by id.
- **Status:** matched in `p4-cdl` (4.4d, `crates/ocio/src/fileformats/cdl/cdl_parser.rs`,
  `cdl_reader_helper.rs`); the oracle cases of `crates/ocio/tests/file_format_cc_oracle.rs`
  and `file_format_ccc_cdl_oracle.rs`.

### I-176. A CDL file's root element must be named in its first 5 KiB

- **Upstream:** the CDL parser reads the lines of the first 5 KiB of the file to find the
  root element's name ("<ColorDecisionList", "<ColorCorrectionCollection" or
  "<ColorCorrection", in that order, anywhere in that text) and picks the schema from it
  (`loadHeader`, `initializeHandlers`, `fileformats/cdl/CDLParser.cpp:177-195, 286-325` @
  v2.5.2); a file whose root starts later (after a long comment or DOCTYPE) fails with
  "Missing CDL tag".
- **Who notices:** valid CDL, CCC and CC files with more than 5 KiB before their root.
- **A fix:** pick the schema from the root element expat reports.
- **Status:** matched in `p4-cdl` (4.4d, `crates/ocio/src/fileformats/cdl/cdl_parser.rs`);
  the "past 5 KiB" cases of `crates/ocio/tests/file_format_cc_oracle.rs`.

### I-177. The XML readers parse a line of 4 GiB or more modulo 2^32

- **Upstream:** the CDL, Iridas look and CTF readers give expat each line with
  `XML_Parse(parser, line.c_str(), (int)line.size(), done)` (`CDLParser.cpp:241-279`,
  `FileFormatIridasLook.cpp:173-191`, `FileFormatCTF.cpp:238-274` @ v2.5.2). The cast keeps
  the low 32 bits: a line of 2 GiB to 4 GiB has a negative length, which expat refuses
  ("invalid argument"); one of 4 GiB or more is parsed as its first `size mod 2^32` bytes, and
  the rest of the line is skipped.
- **Who notices:** files with lines of 2 GiB or more (hostile files).
- **A fix:** refuse lines longer than `INT_MAX`, or give expat long lines in pieces.
- **Status:** matched in `p4-cdl` (`crates/ocio-formats/src/expat/xmlparse.rs`, `parse` and
  `c_int_length`; `a_line_of_4_gib_or_more_parses_its_length_mod_2_32`).

### I-178. `GroupTransform::write` ignores a failing output stream

- **Upstream:** the writers write to the caller's `std::ostream`, whose failures set its
  state bits without throwing; `GroupTransformImpl::write` (`transforms/GroupTransform.cpp:
  114-139` @ v2.5.2) never checks them, so a write to a full disk or a closed stream
  "succeeds".
- **Who notices:** C++ callers that write to a file stream and don't check it, and the
  port's callers, whose `std::io::Write` errors are dropped the same way.
- **A fix:** raise an error when the stream fails.
- **Status:** matched in `p4-cdl` (`crates/ocio/src/transforms/group_transform.rs`, `write`;
  `a_failing_output_stream_is_not_an_error`).

### I-180. The CTF format's info has no bake capabilities

- **Upstream:** `LocalFileFormat::getFormatInfo` (`fileformats/FileFormatCTF.cpp:147-170` @
  v2.5.2) sets the bake capabilities of its second info, "Color Transform Format", on its
  first, the CLF one, a second time (`info.bake_capabilities = ...` after
  `info2.capabilities`), so the CTF info has none.
- **Who notices:** no one through the API: the baker reads a format's first info
  (`Baker.cpp:249-251`), the CLF one, which has its capabilities, and no other code reads
  them.
- **A fix:** set `info2.bake_capabilities`.
- **Status:** matched in `p4-ctf-read-1` (`crates/ocio/src/fileformats/file_format_ctf.rs`,
  `format_info`).

### I-181. A CTF LUT's raw halfs outside [0, 65536) keep the low 16 bits of their `int`

- **Upstream:** with `rawHalfs="true"`, `CTFReaderLut1DElt::endArray` and
  `CTFReaderInvLut1DElt::endArray` (`fileformats/ctf/CTFReaderHelper.cpp:4201-4246,
  3378-3429` @ v2.5.2) read each value as `ConvertHalfBitsToFloat((unsigned short)value)`. The
  cast of a `float` outside `[0, 65536)` (negative, too large, NaN) is undefined. Both wheels
  compile it as `cvttss2si` to a 32-bit `int` (Windows `0x1800a76f3` and `0x1800a7d06`; Linux
  `0x209ad0` and `0x205750`, the Lut1D and InverseLUT1D sites), then keep its low 16 bits:
  `70000` reads as half `0x1170`, `-1` as `0xffff`, NaN, infinities and values past the `int`
  range as `0x0000`.
- **Who notices:** a CLF whose raw half values aren't half bit patterns, which no valid file
  has.
- **A fix:** reject values that aren't integers in `[0, 65535]`.
- **Status:** matched in `p4-ctf-read-2` (`crates/ocio/src/fileformats/ctf/ctf_reader_helper.rs`,
  `lut1d_end_array`; `crates/ocio/tests/file_format_ctf_oracle.rs` checks such values with
  the wheel).

### I-182. A `.3dl` line over 4095 bytes ends the file there

- **Upstream:** the `.3dl` reader reads lines with `istream.getline(lineBuffer, 4096)`
  (`src/OpenColorIO/fileformats/FileFormat3DL.cpp:247-258` @ v2.5.2). A longer line fills the
  buffer with its first 4095 bytes and fails the stream: the reader parses those bytes, so the
  value they cut is read cut (`1023` as `10`), and `while (istream.good())` stops, so nothing
  after that line is read. Its bit-depth errors also lack a space: "Error parsing .3dl
  file.The maximum 3D LUT value, ..." (`FileFormat3DL.cpp:438-457`; the shaper's messages
  have it, 376-395).
- **Who notices:** `.3dl` files with a line of 4096 bytes or more (a shaper of about 800
  values or more on one line): their LUT is cut or refused with a misleading error.
- **A fix:** read lines of any length; add the space.
- **Status:** matched in `p4-readers` (4.3a, `crates/ocio/src/fileformats/file_format_3dl.rs`);
  the oracle cases "a shaper line of 4094 bytes" to "4097 bytes" and "a line past the buffer"
  (`crates/ocio/tests/file_format_3dl_oracle.rs`).

### I-183. Four Houdini LUT messages and blocks that mislead

- **Upstream:** the Houdini reader (`src/OpenColorIO/fileformats/FileFormatHDL.cpp` @ v2.5.2):
  - reports a type `C` file without an `RGB` block as "3D+1D LUT should contain Pre{} LUT
    section" (565-572);
  - swaps the words of "Malformed LUT - Unknown word '<LUT name>' after LUT name '<next
    word>'" (165-175);
  - quotes the first size of the `Length` line when a later one doesn't parse: "Invalid
    integer on 'Length' line: '<first size>'" (443-453);
  - drops the rest of the `LUT:` line (`readHeaders`, 55-78), so `LUT: {` or `LUT: RGB {`
    loses its brace or block name, and the block then reads as malformed.
- **Who notices:** authors of malformed Houdini LUTs, whose errors name the wrong block or
  word.
- **A fix:** name the `RGB` block, swap the words back, quote the size that doesn't parse, and
  read a block that starts on the `LUT:` line.
- **Status:** matched in `p4-readers` (4.3b, `crates/ocio/src/fileformats/file_format_hdl.rs`);
  the oracle cases "no RGB block", "a name not followed by {", "a second Length that doesn't
  parse" and "the 3D block's brace on the LUT: line" (`crates/ocio/tests/
  file_format_hdl_oracle.rs`).

### I-184. Two cineSpace messages with a missing space and a stray quote

- **Upstream:** the `.csp` reader's "Prelut does not specify the expected number of data
  points. Expected: <n>.Found: <a>, <b>. In <file>." has no space after the period
  (`src/OpenColorIO/fileformats/FileFormatCSP.cpp:449-455` @ v2.5.2), and "A csp 3D LUT with
  invalid cube size (<n>): <line>' in <file>." closes a quote it never opened (573-579).
- **Who notices:** readers of those errors.
- **A fix:** add the space; quote the line on both sides.
- **Status:** matched in `p4-readers` (4.3e, `crates/ocio/src/fileformats/file_format_csp.rs`);
  the oracle cases of prelut counts and cube sizes (`crates/ocio/tests/
  file_format_csp_oracle.rs`).

### I-185. The line readers read some numbers differently on Windows and Linux

- **Upstream:** `NumberUtils::from_chars` (`src/utils/NumberUtils.h:104-256` @ v2.5.2) is
  `std::from_chars` where the compiler declares it (`__cpp_lib_to_chars`, 18-21), and
  `strtod_l`, `strtof_l` and `strtol_l` in the C locale otherwise. The Windows wheel (MSVC)
  compiled the first, the Linux wheel (GCC 14, whose `<charconv>` isn't included there) the
  second. The two agree on ordinary numbers and differ on edge cases: a sign before `0x`, a
  hex float, subnormal and out-of-range values, NaN payloads, octal integers, and the width of
  `long`. The line readers parse their values with it (`StringToFloat`, `StringToInt`,
  `ParseUtils.cpp:575-608`), so a file holding such a number reads differently: in an Iridas
  `.cube`, `-0x1p-2` reads as `-0` on Windows and `-0.25` on Linux, and `1e-40` reads as that
  subnormal on Windows while Linux refuses the file (wheel probes, 2026-10-15). The `.look`
  reader parses its size as a `long`: `"4294967298"` is out of range on Windows ("Invalid LUT
  size value: '"4294967298"'. Expected quoted integer") and too large on Linux ("... Expected
  value between 2 and 129") (wheel probes, 2026-10-15), and a zero-padded `"010"` is a size of
  10 on Windows and 8 (octal) on Linux, so the same file reads as a LUT of 10 per side on one
  and fails its entry count on the other (the `.look` verifier's probes, 2026-10-16).
- **Who notices:** LUT files with hex floats, subnormal values, or other edge-case numbers,
  shared between Windows and Linux.
- **A fix:** one parser on both platforms (`std::from_chars` where it exists, or a port of
  it).
- **Decided** (PLAN.md D12): the port does what each wheel does.
- **Status:** matched (`crates/ocio-ops/src/utils/number_utils.rs`, `Flavor`);
  checked in every line reader against the wheel of each platform
  (`crates/ocio/tests/file_format_number_forms_oracle.rs`), and in the `.look` reader's size
  (`crates/ocio/tests/file_format_iridas_look_oracle.rs`).

### I-186. A `.look` file's closed mask still refuses later skipped elements

- **Upstream:** the `.look` parser sets `m_inMask` when a `<mask>` element starts, and skips
  its content (`StartElementHandler`, `src/OpenColorIO/fileformats/FileFormatIridasLook.cpp:
  303-315` @ v2.5.2). The mask's end only lowers the skip depth: `m_inMask` is cleared by the
  `else if (pImpl->m_inMask)` branch of `EndElementHandler` (385-393), which the skip branch
  before it always takes (343-346). So after any mask, empty or not, an element nested in a
  later skipped element (`<shaders><base/></shaders>`) fails with "Cannot load .look LUT
  containing mask" (272-276), though that element is no mask (wheel probes, Windows and Linux,
  2026-10-15: `<mask/>` then `<shaders><base/></shaders>` is refused, `<mask/>` then
  `<shaders/>` reads).
- **Who notices:** `.look` files with an empty mask before shaders or other sections with
  child elements: they are refused as files with a mask.
- **A fix:** clear `m_inMask` when the mask element ends.
- **Status:** matched in `p4-look` (4.4f, `crates/ocio/src/fileformats/
  file_format_iridas_look.rs`); the "empty mask" oracle cases
  (`crates/ocio/tests/file_format_iridas_look_oracle.rs`).

### I-187. A CTF ACESParams without a gamma names the attribute without a space

- **Upstream:** `CTFReaderACESParamsElt::start` (`fileformats/ctf/CTFReaderHelper.cpp:1272-1323`
  @ v2.5.2) refuses a Rec.2100 surround ACES element whose ACESParams has no `gamma` with
  `"Missing required parameter ", ATTR_GAMMA, "for ACES FixedFunction element with style "`:
  the message reads "Missing required parameter gammafor ACES FixedFunction element with
  style Rec2100SurroundFwd.".
- **Who notices:** a reader of the error of a deprecated CTF 1.x ACES element with a wrong
  parameter.
- **A fix:** add the space (`"for ACES ..."` becomes `" for ACES ..."`).
- **Status:** matched in `p4-ctf-read-3` (`crates/ocio/src/fileformats/ctf/ctf_reader_helper.rs`,
  `CtfReaderAcesParamsElt::start`; `crates/ocio/tests/file_format_ctf_oracle.rs` checks the
  message with the wheel).

### I-188. One config gives two files' dynamic RGB curves the same processor

- **Upstream:** `Config::getProcessor` (`Config.cpp:4849-4873` @ v2.5.2) reuses a cached
  processor whose cache ID equals the new one's (the fallback that
  `OCIO_DISABLE_CACHE_FALLBACK` turns off). A dynamic GradingRGBCurve op leaves its curves
  out of its cache ID (`GradingRGBCurveOpData::getCacheID`,
  `ops/gradingrgbcurve/GradingRGBCurveOpData.cpp:127-150`: only a static one writes them), so
  two CTF files whose GradingRGBCurves are dynamic (`<DynamicParameter param="RGB_CURVE"/>`),
  with the same style and direction but different curves, get the same cache ID, and the
  second file's processor is the first's, with the first file's curves (and, being the same
  object, the first file's path in `getProcessorMetadata().getFiles()`). With two such files,
  identity curves (`a`) and a bent red curve (`b`), reading `a` then `b` in one raw config
  gives `b` the identity: `applyRGBA([0.25, 0.25, 0.25, 1])` is `0.25` for both, against
  `0.37524959` for `b` read first (the verifier's `dyn_cache.py`, 2026-10-16).
- **Who notices:** an application that reads two such files through one config, without
  setting the curves through the dynamic property afterwards.
- **A fix:** keep the fallback to processors without dynamic properties, or put the dynamic
  values in the cache ID.
- **Status:** matched in `p4-ctf-read-3`: the port's processor cache does the same
  (`crates/ocio/src/config.rs`, `processor_with_context`);
  `dynamic_curves_share_a_processor_as_in_the_wheel`
  (`crates/ocio/tests/file_format_ctf_oracle.rs`) checks which processors the wheel and the
  port share, and their cache IDs, with the oracle's `processor_cache`.

### I-189. A GradingRGBCurve's bypassLinToLog accepts only "true"

- **Upstream:** `CTFReaderGradingRGBCurveElt::start`
  (`fileformats/ctf/CTFReaderHelper.cpp:2592-2640` @ v2.5.2) refuses any `bypassLinToLog`
  value other than `true` (ignoring case), `false` included, with "Unknown bypassLinToLog
  value: 'false' while parsing RGBCurve.", where the attribute is a boolean.
- **Who notices:** a CTF writer that spells out the default, `bypassLinToLog="false"`.
- **A fix:** accept `false` as the default.
- **Status:** matched in `p4-ctf-read-3`
  (`crates/ocio/src/fileformats/ctf/ctf_reader_helper.rs`, `grading_rgb_curve_start`);
  `op_cases_g2`'s "bypassLinToLog false" checks it with the wheel.

### I-190. A curve's Slopes before its ControlPoints count the default curve's points

- **Upstream:** `CTFReaderGradingCurveSlopesElt::end` (`CTFReaderHelper.cpp:2940-2954` @
  v2.5.2) checks the number of slopes against the number of control points the curve has when
  the Slopes element ends. Before the ControlPoints element, that is the style's default curve
  (3 points for the log and video styles), so the same elements in the other order are
  refused or accepted differently, and a later ControlPoints keeps the slopes it doesn't
  replace (`setNumControlPoints` resizes them).
- **Who notices:** a CTF file that writes a curve's Slopes before its ControlPoints.
- **A fix:** keep the slopes until the curve ends, and check them against its final points.
- **Status:** matched in `p4-ctf-read-3` (`ctf_reader_helper.rs`,
  `CtfReaderGradingCurveSlopesElt`); `op_cases_g2`'s "slopes before the control points" and
  "slopes of the default curve" check it with the wheel.

### I-191. The ACES reader's missing-style message starts with a lower-case letter

- **Upstream:** `CTFReaderACESElt::start` (`CTFReaderHelper.cpp:1209-1239` @ v2.5.2) refuses an
  ACES element without a style with "style parameter for FixedFunction is missing.", where the
  FixedFunction and Function readers (1427-1473, 1506-1534) say "Style parameter ...".
- **Who notices:** a reader of the error of a deprecated CTF 1.x ACES element.
- **A fix:** capitalize it as the others do.
- **Status:** matched in `p4-ctf-read-3` (`ctf_reader_helper.rs`, `fixed_function_start`);
  `op_cases_g`'s "a missing style" ACES case checks it with the wheel.

### I-192. A file reference's recursion check ignores case

- **Upstream:** `BuildFileTransformOps` (`transforms/FileTransform.cpp:909-929` @ v2.5.2)
  refuses a file it is still loading by comparing the resolved paths with
  `Platform::Strcasecmp`, which ignores ASCII case on every platform. On Windows the file
  system ignores case too, so a CTF file `CaseA.ctf` that references `casea.ctf` is a real
  cycle there. On Linux the two names are two files: `Pair.ctf` referencing a separate
  `pair.ctf` is refused with "Reference to: <dir>/pair.ctf is creating a recursion.", though
  nothing recurses.
- **Who notices:** a Linux user whose CTF references a file whose name differs from the
  referencing file's only in case.
- **A fix:** compare the paths as the file system does (case-sensitively on Linux).
- **Status:** matched in `p4-ctf-read-3` (`crates/ocio/src/transforms/file_transform.rs`,
  `build_file_transform_ops`); `references_resolve_as_in_the_wheel`'s "names that differ in
  case" group (`crates/ocio/tests/file_format_ctf_oracle.rs`) checks a self-reference in
  another case on both platforms, and the two files on Linux, with the wheel.

### I-193. The processor cache ignores the context variables a CTF reference uses

- **Upstream:** `Config::getProcessor` (`Config.cpp:4791-4873` @ v2.5.2) keys its processor
  cache on the context variables `CollectContextVariables` finds in the transform. For a file
  transform those are the variables of its `src`, of the search path that finds it and of its
  CCC ID (`transforms/FileTransform.cpp:224-300`), not those of the references inside the CTF
  file, which `BuildFileTransformOps` resolves with the same context. So with one config, a CTF
  file whose reference is `path="$FILEVAR"` gets one processor for every value of `FILEVAR`:
  the first one built (`FILEVAR=m.ctf`, a matrix) is returned for `FILEVAR=lut.spi1d` too,
  with the same cache ID and the first file's `getFiles()` (the verifier's `ctxcache.py`,
  2026-10-16).
- **Who notices:** a config whose CTF files pick their references through context variables,
  per shot for instance, used through one config object.
- **A fix:** collect the context variables of the references when the file is read, or leave
  file transforms whose files reference others out of the cache.
- **Status:** matched in `p4-ctf-read-3`: the port's processor cache and its context variable
  collection do the same (`crates/ocio/src/config.rs`, `processor_with_context`);
  `context_variables_in_references_share_a_processor_as_in_the_wheel`
  (`crates/ocio/tests/file_format_ctf_oracle.rs`) checks the cache IDs and files of both orders
  with the oracle's `config_calls`.

### I-194. A Reference's basePath is read and ignored

- **Upstream:** `CTFReaderReferenceElt::start` (`fileformats/ctf/CTFReaderHelper.cpp:5025-5091`
  @ v2.5.2) accepts a `basePath` attribute and drops its value ("Ignore BasePath for now"): the
  path resolves through the context's search paths alone. Its only effect is the error when an
  alias comes with it.
- **Who notices:** a CTF file from another application that relies on `basePath` to find its
  references.
- **A fix:** resolve the path against the base path first, or warn that it is ignored.
- **Status:** matched in `p4-ctf-read-3` (`crates/ocio/src/fileformats/ctf/ctf_reader_helper.rs`,
  `reference_start`); `op_cases_h`'s "a path and a basePath" and "only a basePath" check it with
  the wheel.

### I-195. A `.look` size is read from each piece of its text alone

- **Upstream:** the `.look` parser reads the size in `CharacterDataHandler`
  (`src/OpenColorIO/fileformats/FileFormatIridasLook.cpp:416-440` @ v2.5.2), once for each
  piece of text expat delivers: each piece must parse on its own (but a lone line feed), and
  the last one sets the size. expat delivers the text
  of an element in pieces at line ends (the reader gives it a line at a time), around
  comments and processing instructions, and at each character or entity reference. So a
  pretty-printed `<size>` (the value on its own line between indented lines) is refused at its
  white space ("Invalid LUT size value: '    '. Expected quoted integer"), `"1<!-- -->7"` is
  refused as `'"1'` (between 2 and 129), and `&quot;2&quot;` as `'"'` (the `.look` verifier's
  probes, both wheels, 2026-10-16).
- **Who notices:** `.look` files written by tools that indent or split the size.
- **A fix:** collect the size's text until the element ends, then parse it once.
- **Status:** matched in `p4-look` (4.4f, `crates/ocio/src/fileformats/
  file_format_iridas_look.rs`); the oracle cases of sizes on their own line, split by a
  comment, and by `&quot;` (`crates/ocio/tests/file_format_iridas_look_oracle.rs`).

### I-196. A `.look` file's data may not hold tabs or carriage returns

- **Upstream:** the `.look` parser removes spaces, quotes and line feeds from the data
  (`CharacterDataHandler`, `src/OpenColorIO/fileformats/FileFormatIridasLook.cpp:441-457` @
  v2.5.2), though the format's description says "all whitespace and quotes" (52-53). A tab or
  a carriage return (written `&#13;`, since expat turns CR LF into LF) stays in the data and
  is refused: tab-indented data lines give "Number of characters in 'data' must be multiple
  of 8. 193 elements found." (the `.look` verifier's probes, both wheels, 2026-10-16).
- **Who notices:** `.look` files whose data lines are indented with tabs.
- **A fix:** remove all XML white space from the data.
- **Status:** matched in `p4-look` (4.4f, `crates/ocio/src/fileformats/
  file_format_iridas_look.rs`); the oracle cases of tab-indented data and of `&#9;` and
  `&#13;` in the data (`crates/ocio/tests/file_format_iridas_look_oracle.rs`).

### I-197. Two `.look` data messages that mislead

- **Upstream:** `getLut` (`src/OpenColorIO/fileformats/FileFormatIridasLook.cpp:193-235` @
  v2.5.2) says "Number of characters in 'data' must be multiple of 8. N elements found."
  where N counts characters (195-203), and "Non-hex characters found in 'data' block at index
  'I'." where I is the start of the 8-character group holding the bad character, not its own
  position (211-221).
- **Who notices:** authors of malformed `.look` files.
- **A fix:** say "characters", and give the bad character's index.
- **Status:** matched in `p4-look` (4.4f, `crates/ocio/src/fileformats/
  file_format_iridas_look.rs`); the oracle cases "a character missing" and "a character that
  isn't hex" (`crates/ocio/tests/file_format_iridas_look_oracle.rs`).

### I-198. A `.look` LUT without a size or data fails after the reader

- **Upstream:** `getLut` expects `3 * size^3` values, with a size of 0 when no `<size>`
  element was read (`src/OpenColorIO/fileformats/FileFormatIridasLook.cpp:205-233` @ v2.5.2),
  so a `<LUT>` without size and data passes it, and `read` makes a 3D LUT of 0 entries per
  side (538-548). The file reads, and the error comes when its op is validated: "Lut3D
  content array issue: Array content is empty." (`Lut3DOpData::validate`,
  `ops/lut3d/Lut3DOpData.cpp:371-381`; `OpArray.h:168`), with no file name.
- **Who notices:** authors of `.look` files without a LUT section's content.
- **A fix:** refuse a missing size in `getLut`, naming the file.
- **Status:** matched in `p4-look` (4.4f, `crates/ocio/src/fileformats/
  file_format_iridas_look.rs`); the oracle cases "no size and no data" and "no LUT"
  (`crates/ocio/tests/file_format_iridas_look_oracle.rs`).

### I-199. A format named in other case is accepted by the baker, then refused by the bake

- **Upstream:** `Baker::setFormat` finds the format by its name ignoring case
  (`FormatRegistry::getFileFormatByName`, `transforms/FileTransform.cpp:358-365` @ v2.5.2) and
  keeps the name as given (`Baker.cpp:118-139`); `Baker::bake` hands that name to the format
  (`Baker.cpp:393-403`), and eight formats' bakes compare it with their own names, case
  included: `spi1d` (`FileFormatSpi1D.cpp:323-329`), `spi3d` (`FileFormatSpi3D.cpp:274-280`),
  `iridas_cube`, `iridas_itx` (whose message says "Unknown 3dl format name",
  `FileFormatIridasItx.cpp:257-263`), `resolve_cube`, `houdini`, `flame` and `lustre`
  (`FileFormat3DL.cpp:509-522`), and the CLF and CTF ("Unknown CLF/CTF file format name,
  '<name>'.", `FileFormatCTF.cpp:1358-1364`). So `setFormat("SPI1D")` succeeds and `bake` fails with
  "Error baking SPI1D:Unknown spi format name, 'SPI1D'." (wheel probes, Windows and Linux,
  2026-10-16). The cineSpace and Truelight bakes take the name in any case.
- **Who notices:** callers that name a format in other case than the registry's.
- **A fix:** keep the registry's name in `setFormat`, or compare ignoring case in the bakes.
- **Status:** matched in `p4-baker` (4.8b1: spi1d and spi3d; 4.8b2: Iridas cube, itx and
  Resolve cube; 4.8c: 3DL and Houdini; in their `crates/ocio/src/fileformats/` files) and
  `p4-ctf-bake` (4.8d: CLF and CTF, `file_format_ctf.rs`); the "named in capitals" cases of
  `crates/ocio/tests/bake_formats_oracle.rs` and the "format named" cases of
  `crates/ocio/tests/ctf_bake_oracle.rs`.

### I-200. A `.3dl` bake with a shaper size below 1

- **Upstream:** the `.3dl` bake makes its shaper with `std::vector<float> shaperData(shaperSize)`
  and fills it through `GenerateIdentityLut1D(&shaperData[0], shaperSize, 1)`
  (`src/OpenColorIO/fileformats/FileFormat3DL.cpp:552-553` @ v2.5.2); the baker checks the
  shaper size only when a shaper space is set (`Baker.cpp:369-375`), which the 3DL formats
  refuse.
  - A size of 0 makes `&shaperData[0]` index an empty vector: undefined behaviour, which both
    wheels resolve the same way. An empty vector's storage is a null pointer, so the index
    gives a null pointer, `GenerateIdentityLut1D` returns at its `if(!img) return;`
    (`ops/lut1d/Lut1DOp.cpp:196`), and the shaper line is empty. A size of 1 writes a lone `0`
    (a NaN step, clamped to 0).
  - A negative size converts to a `size_t` past the vector's `max_size()`, and the constructor
    throws `std::length_error`, which `Baker::bake` reports with each C++ library's text:
    "Error baking flame:vector too long" on Windows (MSVC's STL) and "Error baking
    flame:cannot create std::vector larger than max_size()" on Linux (libstdc++), after a
    lustre file's first two lines are written (wheel probes, 2026-10-16).
- **Who notices:** callers of the `.3dl` bakes that set a shaper size below 1.
- **A fix:** check the shaper size whatever the shaper space, or ignore it for formats without
  a shaper.
- **Status:** matched in `p4-baker` (4.8c, `crates/ocio/src/fileformats/file_format_3dl.rs`,
  `VECTOR_CONSTRUCTOR_TOO_LONG`; an empty shaper writes an empty line); the "shaper size"
  cases of `crates/ocio/tests/bake_formats_oracle.rs`, whose partial files are compared too.

### I-201. A Reference by an alias other than currentMonitor adds no ops

- **Upstream:** the Reference reader keeps an alias (`CTFReaderHelper.cpp:5047-5056` @ v2.5.2),
  refusing only `currentMonitor`, and `BuildOp` (`fileformats/FileFormatCTF.cpp:1218-1248`)
  builds only references by path: a reference by any other alias, inverted or not, silently
  adds nothing to the processor.
- **Who notices:** a CTF file whose aliases name transforms the application is expected to
  supply.
- **A fix:** refuse unknown aliases, or log that they are skipped.
- **Status:** matched in `p4-ctf-read-3` (`ctf_reader_helper.rs`, `reference_start`;
  `crates/ocio/src/fileformats/file_format_ctf.rs`, `build_op`); `op_cases_h`'s "an alias" and
  "an alias, inverted", and upstream's `reference_alias.ctf`, check it with the wheel.

### I-202. A Reference's inverted attribute only ever sets the inverse

- **Upstream:** the Reference reader sets the inverse direction when `inverted` is `true`,
  ignoring case (`CTFReaderHelper.cpp:5058-5064` @ v2.5.2), and ignores any other value,
  without a warning: `inverted="1"` or `"yes"` leaves the reference forward, and a later
  `inverted="false"` (the attribute again, in another case) doesn't undo an earlier `"true"`.
- **Who notices:** a CTF writer that spells booleans another way.
- **A fix:** parse the value as a boolean and refuse what isn't one.
- **Status:** matched in `p4-ctf-read-3` (`ctf_reader_helper.rs`, `reference_start`);
  `op_cases_h`'s "inverted other than true" and "inverted true, then false" check it with the
  wheel.

### I-203. The CLF/CTF writer pads its arrays with a width that grows

- **Upstream:** `WriteValues` (`fileformats/ctf/CTFTransform.cpp:561-671` @ v2.5.2) formats each
  array value in an `ostringstream` whose width it set once, for the bit depth (3 for 8i, 4 for
  10i and 12i, 5 for 16i, 11 for 16f and 32f floats). A stream's width resets to 0 after each
  insertion, but the padding goes into the value's text first, and when a value's text is
  longer than the width, the width becomes that length for the values after it. So each value
  is padded to the longest text written before it in the array, not to the bit depth's width,
  and one long value shifts every later column. Upstream's own expected text shows it: in
  `OCIO_ADD_TEST(CTFTransform, lut1d_10i_ctf)` (`FileFormatCTF_tests.cpp:7894-7925`) the third
  row is `   1023    1023    1023`, padded to the 7 characters of `4011.12` in the row above.
  The GradingRGBCurve writer's `writeCurve` (`CTFTransform.cpp:1559-1603`) sets a width of 11
  with `SetOStream` and then streams `x << " " << y`: only the first number of each control
  point line, and only the first slope, is padded.
- **Who notices:** readers of written CLF and CTF files, whose arrays don't line up in columns
  once a value needs more digits.
- **A fix:** set the width before each value, or pad all values to the widest one.
- **Status:** matched in `p4-ctf-read-3` (`crates/ocio/src/fileformats/ctf/ctf_writer.rs`,
  `write_values` and `GradingRgbCurveWriter::write_curve`); the ported `lut1d_10i_ctf` and
  `bitdepth_ctf` check it against upstream's expected text, and the LUT and RGB curve cases of
  `crates/ocio/tests/file_format_ctf_write_oracle.rs` and
  `crates/ocio/src/fileformats/file_format_ctf_write_oracle_tests.rs`, check it with the
  wheel.

### I-204. The CLF/CTF bake fails on a shaper space whose range makes an invalid Range

- **Upstream:** with a shaper space, the CLF/CTF bake's 1D LUT (no crosstalk) and its shaper
  of a set size (crosstalk) come after a Range from the shaper space's range
  (`FileFormatCTF.cpp:1445, 1498, 1517-1532` @ v2.5.2). The range is the smallest start and
  the largest end of the three channels (`GetSrcRange`, `BakingUtils.cpp:53-73`), through
  `std::min` and `std::max`, which pass a NaN in red on and drop one in green or blue. A
  range that decreases (an inverted shaper space), whose ends are too close, or that starts
  with a NaN makes a Range that the op refuses (`RangeOpData.cpp:236, 471`), and the bake
  fails: "Error baking Academy/ASC Common LUT Format:Range maximum input value is less than
  minimum input value", "...Range maxInValue is too close to minInValue", "...In and out
  minimum limits must be both set or both missing in Range." (wheel probes, 2026-10-17). The
  half-domain shaper, without a shaper size, bakes those spaces; and a NaN in green or blue
  is ignored.
- **Who notices:** bakes with a shaper space that inverts, narrows to almost nothing or gives
  NaN.
- **A fix:** order the range's ends, refuse an unusable shaper space with a message that names
  it, and take a NaN in any channel alike.
- **Status:** matched in `p4-ctf-bake` (4.8d, `crates/ocio/src/fileformats/file_format_ctf.rs`);
  `invalid_shaper_ranges_as_in_the_wheel` (`crates/ocio/tests/ctf_bake_oracle.rs`).

### I-205. Chains of looks through the color space before double their work per link

- **Upstream:** a look transform converts to each look's process space and, after the looks,
  to its destination (`transforms/LookTransform.cpp:291-297, 355-360` @ v2.5.2). In a chain of
  color spaces where `c<i>` converts to the reference with a look transform from and to `c0`
  through a look whose process space is `c<i-1>`, building `c<i>` builds `c<i-1>`'s ops twice,
  once into the process space and once out of it: `T(n) = 2 T(n-1) + 1` ops, 2^n - 1 for `n`
  links. The wheel's processor of 20 links holds 1,048,575 ops (3.9 s); of 21, 7.6 s; of 22,
  more than 4 GB (the verifier's probes, 2026-10-17, under a memory cap). A chain where each
  color space's transforms to and from the reference both name the space before does the same
  in `CollectContextVariables`, in time only. The nesting limits (U-58) don't bound it: the
  depth stays `n`.
- **Who notices:** a machine-made config of such a chain, a few dozen links long; nothing
  hand-written.
- **A fix:** build each color space's conversion once per processor (a cache by color space and
  direction), or refuse chains whose op count passes a bound.
- **Status:** matched (the port takes the same time and memory, 8.1 s for 21 links): the owner
  decides whether to bound it.

### I-206. A LUT baked to a file has Windows line endings on Windows

- **Upstream:** the Python binding's `Baker.bake(fileName)` writes through a text-mode
  `std::ofstream`, so on Windows each line feed of the baked text is written as CR LF; on Linux
  the file holds the text as it is (`bake()` returns it with line feeds on both). A bake that
  fails leaves what it wrote before the failure in the file: lustre with a shaper size of -3
  leaves `3DMESH` CR LF `Mesh 0 12` CR LF on Windows (the baker verifier's probe,
  2026-10-16).
- **Who notices:** applications that bake LUT files on Windows and compare them with files
  baked on Linux, or read them with tools that keep the CR.
- **A fix:** open the file in binary mode.
- **Status:** the port's `Baker::bake` writes to any `std::io::Write` with line feeds, as
  `bake()`'s text; the text-mode file belongs to the Python binding (`ocio-py`). The oracle's
  `bake` reports the file and a failed bake's partial file, which the bake tests compare with
  the port's text in text mode (`crates/ocio/tests/bake_common/mod.rs`).

### I-207. An archive entry's name is a wildcard

- **Upstream:** a config read from an `.ocioz` archive finds its files by comparing names with
  minizip-ng's `mz_path_compare_wc`, where a `*` matches any run of bytes, `/` matches `\`,
  and case is ignored. `getFileBufferByPath` passes the entry's name as the wildcard and the
  path asked for as the path (`src/OpenColorIO/OCIOZArchive.cpp:417-427` @ v2.5.2), and takes
  the first entry that matches in the archive's order; `getFastLutFileHash` passes them the
  other way round (`OCIOZArchive.cpp:622-639`) and takes the last match in the order of the
  names. So an entry named `*` serves every file asked for (the config's `config.ocio`
  included), and where an entry `luts/*.spi1d` comes before `luts/a.spi1d`, the data of
  `luts/a.spi1d` comes from the first and its hash from the second.
- **Who notices:** archives with a `*` in an entry's name, or entries whose names differ only
  in case or in the kind of slash.
- **A fix:** compare the names as paths, the same way in both functions.
- **Status:** matched in `p4-baker` (4.9a, `crates/ocio/src/ocioz_archive.rs` and
  `crates/ocio/src/minizip/mz_os.rs`); `crates/ocio/tests/ocioz_read_oracle.rs`, the
  "wildcard names", "a wildcard before the name", "a lone star", "names in other cases" and
  "backslashes" archives.

### I-208. Archive entries that can't be read are read as zeros, and empty ones not at all

- **Upstream:** `readEntryBuffer` (`OCIOZArchive.cpp:390-403` @ v2.5.2) sizes the buffer from
  the central directory's uncompressed size, refuses a size of 0 or past 256 MB with "OCIOZ
  archive entry size is invalid or exceeds maximum allowed size.", and ignores what
  `mz_zip_reader_entry_save_buffer` returns. So:
  - an empty file in the archive, a LUT or the config itself, fails with that message;
  - an entry minizip-ng can't read leaves the buffer zeros, or zeros after what it read:
    compressed with a method other than deflate (OpenColorIO builds minizip-ng with zlib
    only), encrypted with AES, on another disk of a split archive, with corrupt deflate data
    (minizip-ng drops the 64 KiB read it fails in; zlib decodes the next block's header in
    the read that fills the buffer, so a bad block right after exactly 65,535 bytes fails
    that read too, and a deflate stream that ends without a final block reads on into the
    bytes after the entry, which minizip-ng doesn't limit for deflate), or with a size larger
    than its data; the file's reader then fails on the zeros, with a message about the
    file's format;
  - an entry flagged as encrypted is read as it is stored, without a password (OpenColorIO
    builds minizip-ng without encryption, `MZ_PKCRYPT=OFF`), and a wrong CRC goes unnoticed.
- **Who notices:** applications reading archives with empty files, or broken or unusual ones,
  which get a misleading error or a config read from zeros instead of an error about the
  archive.
- **A fix:** accept empty entries, and fail with minizip-ng's error when an entry can't be
  read whole.
- **Status:** matched in `p4-baker` (4.9a, `crates/ocio/src/ocioz_archive.rs` and
  `crates/ocio/src/minizip/`); `crates/ocio/tests/ocioz_read_oracle.rs`,
  `entries_read_their_own_way` and the "an empty LUT" and "an empty config" archives,
  `inflate_at_read_boundaries` and `entry_size_limit`.

### I-209. A second archive's LUT files can be the first one's

- **Upstream:** a config read from an archive asks its I/O proxy for its files by the paths
  they have in the archive, relative ones such as `luts/a.spi1d`. OpenColorIO caches the hash
  of a file by that path for the whole process (`GetFastFileHash`, `PathUtils.cpp:64-115` @
  v2.5.2), whatever proxy answered, and its file cache by that hash. So once a config from one
  archive has read `luts/a.spi1d`, a config from another archive that holds a different
  `luts/a.spi1d` gets the first archive's LUT, until `ClearAllCaches`.
- **Who notices:** applications that read several archives in one process whose files share
  paths, such as two versions of the same config.
- **A fix:** key the hash cache with the proxy as well as the path.
- **Status:** matched in `p4-baker` (4.9a): the port's caches are upstream's;
  `crates/ocio/tests/ocioz_read_oracle.rs`, `two_archives_in_one_process`.

### I-210. An archive rebuilt from its local headers has other names on Windows

- **Upstream:** where minizip-ng can't read an archive's central directory (a truncated
  `.ocioz`, say), OpenColorIO's reader rebuilds it from the local headers (`mz_zip_recover_cd`,
  minizip-ng mz_zip.c:1236-1406 @ 4.0.10, recovery being the reader's default), and the
  archive still loads. The rebuilt headers are written by `mz_zip_entry_write_header`, which on
  Windows only (`#if defined(_WIN32)`, mz_zip.c:792-805) writes each `\` of a name as `/`. The
  names are compared ignoring the kind of slash, but the file hashes OpenColorIO builds from them
  (`getFastLutFileHash`: the name and the CRC) differ, and with them the config's cache ID. Once
  one entry's local header has a zip64 field, the data descriptors of all later entries are
  read as zip64 (the flag is never reset in the loop).
- **Who notices:** applications comparing cache IDs of configs read from damaged archives with
  backslashes in their names across platforms.
- **A fix:** keep the names as stored, and reset the zip64 flag for each entry.
- **Status:** matched in `p4-baker` (4.9a2, `crates/ocio/src/minizip/mz_zip.rs`);
  `crates/ocio/tests/ocioz_read_oracle.rs`, `recovered_archives` ("backslashes") and
  `zip64_descriptors_after_a_zip64_entry`, on both platforms.

### I-211. An archive leaves out linked LUT files, and takes every LUT file around the config

- **Upstream:** `Config::archive` adds every file under the config's working directory whose
  extension a LUT format reads (`addSupportedFiles`, `src/OpenColorIO/OCIOZArchive.cpp:149-202`
  @ v2.5.2), whether the config uses it or not, following links to directories. It adds each
  through minizip-ng's `mz_zip_writer_add_path` (mz_zip_rw.c:1649-1729 @ 4.0.10), which skips a
  symbolic link to a file (neither storing nor following links, as OpenColorIO sets it up)
  without an error: a config whose LUT is a link archives without it, and the archived config
  fails to find it.
- **Who notices:** applications archiving configs whose LUT files are links (common where LUTs
  are shared between shows), or whose directories hold unrelated LUT files.
- **A fix:** archive the files the config references, following links to files.
- **Status:** matched in `p4-baker` (4.9b1, `crates/ocio/src/ocioz_archive.rs` and
  `crates/ocio/src/minizip/mz_zip_rw.rs`); `crates/ocio/tests/ocioz_archive_oracle.rs`, the
  generated directory's links (Linux).

### I-212. On Linux, extracting names with backslashes writes files whose names hold them

- **Upstream:** `ExtractOCIOZArchive` saves each entry at its name joined to the destination
  (`mz_zip_reader_save_all`, minizip-ng `mz_zip_rw.c:818-907` @ 4.0.10). On Linux minizip-ng
  is built with `MZ_PRESERVE_NATIVE_STRUCTURE` (its default, `mz_os_posix.c:40-42`), so only
  `/` separates directories (`mz_os_is_dir_separator`, `mz_os_posix.c:292-302`) and a
  backslash is part of a name: `shot1\lut1.clf` becomes a file of that name in the
  destination, and no `shot1` directory is made. On Windows both are separators, and the file
  is `lut1.clf` in the directory `shot1`. Archives written on Windows by older minizip
  versions hold such names: upstream's own `missing_config.ocioz` extracts that way on Linux
  (the Linux wheel, probed 2026-10-17).
- **Who notices:** applications extracting on Linux archives whose names use backslashes.
- **A fix:** convert the separators of the name before saving the file.
- **Status:** matched in `p4-baker` (`crates/ocio/src/minizip/mz_os.rs`);
  `crates/ocio/tests/ocioz_extract_oracle.rs`, `missing_config` and the name
  `dir\back.txt`, on both platforms.

### I-213. A zip extra field shorter than 4 bytes wraps minizip-ng's search limit

- **Upstream:** `mz_zip_extrafield_find` (minizip-ng `mz_zip.c:2583-2612` @ 4.0.10) counts its
  search limit down by each field's length less 4 (`max_seek -= field_length - 4`), so a field
  shorter than 4 bytes counts it up. Its callers that read a zipped central directory and an
  entry's hashes pass `INT32_MAX` (`mz_zip_rw.c:272, 487, 527`), and a short field then
  overflows the `int`: undefined behaviour, which both wheels resolve the same way, wrapping
  it to a negative limit and ending the search (`MZ_EXIST_ERROR`): Linux `sub ebx, eax; js`
  (`libOpenColorIO.so` 0x5f81f2), Windows `add edi, ecx; js` (`OpenColorIO_2_5.dll`
  0x180340248).
- **Who notices:** applications reading archives whose zipped central directory entry has a
  short extra field before its `CDCD` field.
- **A fix:** compute the limit in 64 bits, or refuse a field shorter than its header.
- **Status:** matched in `p4-baker` (`crates/ocio/src/minizip/mz_zip.rs`, `wrapping_sub`);
  `crates/ocio/tests/ocioz_read_oracle.rs`, `more_zipped_central_directories`.

### I-214. A seek past the end of an archive leaves minizip-ng's buffered position behind

- **Upstream:** minizip-ng reads an archive through a buffered stream that keeps its own idea
  of the file position (`mz_strm_buf.c` @ 4.0.10). A seek forwards past what it has read adds
  the offset to that position (`buffered->position += offset`, `mz_strm_buf.c:301`) before it
  seeks the file; where the file's seek fails, the buffer is emptied and the next read reads
  on from where the file is, at the end of the last 32,767 bytes read, not where the seek
  was asked to go. The file's seek fails where the position would pass what the file system
  holds: on Linux `fseeko`'s `lseek` refuses a position past the largest file (16 TiB on
  ext4, other limits on other file systems), so whether a zip64 size of, say, 2^45 bytes
  makes it fail depends on the file system; Windows' `SetFilePointerEx` takes it.
  `mz_zip_recover_cd` seeks past an entry by its local header's size, and a zip64 size near
  `INT64_MAX` makes the sum overflow the 64-bit position: undefined
  behaviour, which both wheels resolve the same way, wrapping it (Linux
  `add qword ptr [rdi + 0x10038], r12` at `libOpenColorIO.so` 0x5f6894, Windows
  `add qword ptr [rbx + 0x10038], rdi` at `OpenColorIO_2_5.dll` 0x18033d179).
- **Who notices:** applications reading damaged archives that minizip-ng recovers.
- **A fix:** check the seek's result, and the sum for overflow.
- **Status:** matched in `p4-baker` (`crates/ocio/src/minizip/mz_strm.rs`, the buffered
  stream, `wrapping_add`, and the file's own seek); `crates/ocio/tests/ocioz_read_oracle.rs`,
  `recovery_after_failed_seeks`, `recovery_after_seeks_past_the_file_system` and its
  `_long_tail`.

### I-215. Extracting a link, then a file of the same name, writes outside the destination

- **Upstream:** `ExtractOCIOZArchive` makes a link entry's link as its target says
  (`mz_zip_reader_entry_save_file`, minizip-ng `mz_zip_rw.c:696-724` @ 4.0.10), with no check
  of the target: `../outside.txt` and absolute targets are taken. A later file entry of the
  same name opens the link and writes through it (`mz_stream_os_open`, `fopen(path, "wb")` on
  Linux, `CreateFile` on Windows): on Linux an archive with a link `l.txt` to
  `../outside.txt` and then a file `l.txt` writes `outside.txt` next to the destination (both
  wheels where they make links, probed 2026-10-17).
- **Who notices:** applications extracting archives from untrusted sources.
- **A fix:** refuse link targets outside the destination, or don't write through links.
- **Status:** matched in `p4-baker` (`crates/ocio/src/minizip/mz_zip_rw.rs`);
  `crates/ocio/tests/ocioz_extract_oracle.rs`, `links` (Linux).

### I-216. Iterating a mixing manager's encodings raises a TypeError in Python

- **Upstream:** the binding's `MixingEncodingIterator` binds `__next__` with a
  `MixingSpaceIterator &` parameter (`src/bindings/python/apphelpers/PyMixingHelpers.cpp:147-151`
  @ v2.5.2), a class the encoding iterator isn't. So `list(manager.getMixingEncodings())` or a
  `for` loop over it raises pybind11's TypeError ("incompatible function arguments"), while
  `len()` and indexing read the encodings (wheel probe, Windows, 2026-10-18: a raw config's
  manager has 2 encodings, `[e[0], e[1]]` is `['RGB', 'HSV']`, `list(e)` raises).
- **Who notices:** Python scripts that iterate the encodings; C++ callers don't go through the
  binding.
- **A fix:** bind `__next__` with `MixingEncodingIterator &`.
- **Status:** open: the Python module (Phase 6) matches it. The oracle's `checks.dump` reads
  the binding's iterators by index, so `object_calls` can write a manager out (`p5-oracle`).

### I-217. The reverse log ExposureContrast ignores the log exposure step

- **Upstream:** each ExposureContrast renderer sets its pivot in `updateData`, and the forward
  log renderer also its exposure step; `ECLogarithmicRevRenderer::updateData` doesn't, so the
  reverse log style keeps the member's default, 0.088, whatever the op's log exposure step
  (`src/OpenColorIO/ops/exposurecontrast/ExposureContrastOpCPU.cpp:39-40, 651-657, 661-662` @
  v2.5.2). An op with a step other than 0.088 and a non-zero exposure isn't undone by its
  inverse: the forward style adds `exposure * step`, the reverse subtracts
  `exposure * 0.088`. Its pivot does use the step.
- **Who notices:** applications using a logarithmic ExposureContrastTransform with a log
  exposure step of their own, inverted (or the inverse direction).
- **A fix:** set `m_logExposureStep` in `ECLogarithmicRevRenderer::updateData` too.
- **Status:** matched in `p5-ec` (5.1b, `crates/ocio-ops/src/ops/exposurecontrast/
  exposure_contrast_op_cpu.rs`, `EcLogarithmicRevRenderer`); the battery's log style cases
  with a step of 0.07 or 0.1 (`crates/ocio-ops/tests/exposure_contrast_oracle.rs`), inverse.

### I-218. A GradingPrimary's NaN values differ between Windows and Linux

- **Upstream:** `GradingPrimaryPreRender::update` (`ops/gradingprimary/GradingPrimary.cpp:
  108-269` @ v2.5.2) combines each RGBM's master value with each channel's (`b.m_master +
  b.m_red`, `c.m_master * c.m_red`, ...) in double precision. Both wheels compile it with
  their own operand orders, different per channel and per platform (Windows `sub_1801d34a0`,
  Linux `0x3a67b0`, read with `tools/wheel-inspect`): for the log style's forward brightness
  MSVC adds `master + red`, `green + master`, `blue + master`, GCC `red + master`, `green +
  master`, `master + blue`; and so on for the contrast, gamma, offset, exposure, gain and the
  inverse video slope. Where a master and a channel value are both NaN, the pre-render value
  keeps a different NaN on each platform. MSVC also folds the inverse log brightness'
  negation into the constant (`x * -6.25 / 1023.`), which keeps a NaN's sign, where GCC flips
  it.
- **Who notices:** anyone comparing the uniforms of a dynamic GradingPrimaryTransform with NaN
  values between a Windows and a Linux machine (and the pixels, where the NaN reaches them).
- **A fix:** one operand order on every platform, which changes NaN bits on at least one.
- **Status:** matched in `p5-ec` (the fix of `p5-values` 5.0a,
  `crates/ocio-ops/src/ops/gradingprimary/grading_primary.rs`, `ORDERS`); the NaN cases of
  `crates/ocio-ops/tests/grading_pre_render_oracle.rs` (random sets of NaN values, each with
  its own payload and sign) check it on both platforms.

### I-219. The hue curves' NaN knots and coefficients differ between Windows and Linux

- **Upstream:** the hue curves' fitting (`PrepHueCurveData`, `CalcKsi`, `FitHueSpline` and
  `EstimateHueSlopes`, `ops/gradingrgbcurve/GradingBSplineCurve.cpp:22-326` @ v2.5.2) is scalar
  `float` code that the two wheels compile with different operand orders: of its 31 additions
  and multiplications of the curve's values (where a NaN y coordinate, x coordinate or slope
  can meet another NaN, or the default NaN of an invalid operation; seven of them show in no
  output: six have an operand that is never NaN, namely `dx * K`, `k * (...)` with a constant,
  `alpha * (...)`, `blend * (...)` and `scale * slopes[i]`, `scale * slopes[i + 1]`, which are
  `KSI_DX_K`, `EST_NEAR_MIN_MUL`, `KSI_BOUND_MUL`, `KSI_BLEND_MUL`, `EST_SCALE0` and
  `EST_SCALE1` in `hue_site` (`grading_b_spline_curve.rs`); and the seventh, the denominator
  `len[k] + len[k - 1]` of the diagonal slopes, `EST_DIAG_DEN`, is NaN only where the
  quotient's numerator is too, and `divss` keeps the numerator's NaN), MSVC swaps the
  source's operands at 17 of them and GCC at 13, not the same ones (4 and 2 of those where
  nothing shows; Windows `sub_1801e7490`, `sub_1801e55d0`, `sub_1801e4cb0`,
  `sub_1801e64d0`; Linux inlined in `computeKnotsAndCoefsForHueCurve`, `0x3bff10`, read with
  `tools/wheel-inspect`). For instance the second segment's constant `p0.y + slopes[i] *
  (ksi - p0.x) + ...` is `((ksi - x0) * s0 + y0) + ...` on Windows and `(s0 * (ksi - x0) +
  y0) + ...` on Linux, and a diagonal curve's slope numerator `len[k] * s[k] + len[k - 1] *
  s[k - 1]` keeps the source's products on Windows but adds them the other way, where Linux
  swaps the products' operands. A curve with NaN values gets knots and coefficients that keep
  a different NaN on each platform: a dynamic op's uniforms, and the pixels where the NaN
  reaches them.
- **Who notices:** anyone comparing a GradingHueCurveTransform with NaN control points or
  slopes between a Windows and a Linux machine.
- **A fix:** one operand order on every platform, which changes NaN bits on at least one.
- **Status:** matched in `p5-ec` (the fix of `p5-values` 5.0d,
  `crates/ocio-ops/src/ops/gradingrgbcurve/grading_b_spline_curve.rs`, `HUE_SWAPS`, every
  site from both wheels' machine code); `crates/ocio-ops/tests/hue_curve_nan_oracle.rs` (y,
  x and slope NaNs each with its own payload and sign, and the verifier's specials) checks it
  on both platforms.

### I-221. Archiving lists a directory whose path has 318 bytes or more by the wrong pattern

- **Upstream:** `Config::archive` lists the config's directory and its subdirectories with
  minizip-ng's `mz_os_open_dir` (`OCIOZArchive.cpp:150-201` @ v2.5.2; minizip-ng
  `mz_os_win32.c:362-399` @ 4.0.10), which builds `FindFirstFileW`'s pattern in a buffer of
  320 bytes: the path cut to 319 bytes (`strncpy`), a separator only where
  `mz_path_append_slash` has room (a path of 317 bytes or fewer), and the `*` only where
  `mz_path_combine` has room. A path of 318 bytes gives `<path>*`, which lists the parent's
  entries whose long or 8.3 short name starts with the directory's own name; a longer one
  gives the path cut to 319 bytes (a character the cut splits becomes U+FFFD), which lists
  the parent's entry of that name. OpenColorIO then looks for each name found in the
  directory itself: a directory of that name is listed in turn; a file with a LUT extension
  is archived where the directory holds one, and otherwise the archive fails ("Could not
  write LUT file <path> to in-memory archive."). The directory's own files aren't archived.
  Which 8.3 names match depends on the volume (whether it makes them, and their form).
- **Who notices:** applications archiving configs in deep directories, on Windows.
- **A fix:** size the pattern's buffer from the path.
- **Status:** matched in `p4-baker` (`crates/ocio/src/minizip/mz_os.rs`, `mz_os_list_dir`,
  through `FindFirstFileW` in `os_windows_ffi.rs`, owner decision 2026-10-17, which makes
  the same 8.3 matches on every volume); `crates/ocio/tests/ocioz_archive_oracle.rs`,
  `long_directory_paths` and `long_directory_patterns` (Windows). Past MAX_PATH the port
  gives the system the pattern's long form (`\\?\`), as the wheel's call works under a
  long-path-aware Python.

### I-222. Archiving reads a long name with the bytes earlier names left, on Windows

- **Upstream:** `Config::archive` lists the config's directory with minizip-ng's
  `mz_os_read_dir` (`OCIOZArchive.cpp:150-201` @ v2.5.2; minizip-ng `mz_os_win32.c:401-421` @
  4.0.10), which converts each name into the same 256 bytes of `d_name`. A name longer than
  255 UTF-8 bytes doesn't fit with its NUL: `WideCharToMultiByte` writes the whole characters
  that fit and fails, and OpenColorIO reads `d_name` up to its first NUL, past the cut into
  what earlier names of the listing wrote, or to the `end` flag's 0 where the 256 bytes hold
  no NUL (U-79 covers the reads that reach memory nothing wrote). The name read isn't the
  file's: a cut of 256 bytes is read as itself, and a shorter one (it falls inside a
  character) with an earlier name's bytes after it. The name read can be another entry's:
  a directory of that name is listed again, and its LUT files are archived again (U+20AC x84
  `.ab`, a directory holding `lut.spi1d`, then U+20AC x84 `.` U+1F600 `.spi1d`: the
  archive holds U+20AC x84 `.ab/lut.spi1d` twice, the Windows wheel, probed 2026-10-18), and
  a file of that name is taken as its own entry is. Where no entry has the name read and it
  has a LUT extension, OpenColorIO looks for that file and the archive fails ("Could not
  write LUT file <dir>\<name read> to in-memory archive."); any other is left out. A
  directory with U+20AC x84 `-3dlzz`, then U+20AC x84 `.` U+1F600 `.spi1d` (cut to 253
  bytes, then the first name's `3dl`) fails with the LUT file U+20AC x84 `.3dl` (the
  Windows wheel, 3 runs of 3, probed 2026-10-18).
- **Who notices:** applications archiving configs next to files with very long names, on
  Windows.
- **A fix:** convert each name into a buffer sized from it, or refuse a name whose conversion
  fails.
- **Status:** matched in `p4-baker` (`crates/ocio/src/minizip/mz_os.rs`, `mz_os_list_dir`);
  `crates/ocio/tests/ocioz_archive_oracle.rs`, `long_names` (Windows, against the wheel).

### I-226. An infinite or NaN alpha makes a GradingPrimary's colors NaN

- **Upstream:** with a saturation other than 1, the GradingPrimary renderers compute the luma
  as the dot product of all four lanes of the pixel with the weights `{0.2126, 0.7152,
  0.0722, 0.0}` (`ops/gradingprimary/GradingPrimaryOpCPU.cpp:155-170` @ v2.5.2). Alpha's
  weight is 0, but an infinite or NaN alpha (after the steps before the saturation, which
  alpha's lane goes through with identity values) times 0 is NaN, so the luma, and every
  color channel, become NaN. Alpha itself is restored. The scalar code (without SSE2) sums the
  three color channels only.
- **Who notices:** images with infinite or NaN alpha through a GradingPrimaryTransform with
  saturation: the colors of those pixels become NaN (and then the black clamp, I-227).
- **A fix:** a weight of 0 that leaves alpha out of the sum (mask alpha's lane before the
  sums), which changes those pixels' colors to the graded values.
- **Status:** matched in `p5-primary` (5.2b, 5.2c); the battery's probes with infinite and NaN
  alphas check it.

### I-227. A GradingPrimary turns NaN colors into -Inf

- **Upstream:** the GradingPrimary renderers end (forward) or start (inverse) with
  `ApplyClamp`, `_mm_min_ps(_mm_max_ps(pix, blackClamp), whiteClamp)`
  (`ops/gradingprimary/GradingPrimaryOpCPU.cpp:172-175` @ v2.5.2), whose `maxps` returns its
  second operand for a NaN first one. Without a clamp, the black clamp is `NoClampBlack`,
  `-DBL_MAX`, which is -Inf as a float: a NaN color becomes -Inf (or the black clamp when there
  is one), whenever the op isn't an identity. The scalar code (`std::max`) keeps the NaN; its
  comment says that the default values that should not clamp change clamping.
- **Who notices:** images with NaN colors through a GradingPrimaryTransform that changes
  anything: the NaN become -Inf, or the black clamp, in forward; in inverse the clamp comes
  first, and the -Inf goes through the rest.
- **A fix:** a clamp that keeps NaN (the operands of `maxps` swapped), which keeps NaN pixels
  NaN.
- **Status:** matched in `p5-primary` (5.2b, 5.2c); the battery's NaN probes check it.

### I-228. A GradingPrimary's NaN pixels can give zeros of another sign on Linux

- **Upstream:** the forward GradingPrimary renderers start with `_mm_add_ps(pixel, offset)`
  (or `brightness`; `ops/gradingprimary/GradingPrimaryOpCPU.cpp:304, 655, 999` and the other
  loops @ v2.5.2). MSVC computes `pixel + offset`, GCC `offset + pixel` (`tools/wheel-inspect`:
  Windows `GradingPrimaryLinFwdOpCPU::apply` 0x1801d4730, Linux 0x3a8500, and the log and video
  ones), so where a NaN pixel meets a NaN offset (an offset whose master and channel are +Inf
  and -Inf), Windows keeps the pixel's NaN and Linux the offset's. The linear style's contrast
  then keeps the NaN's sign while `ssePower` makes it 0 (`ApplyLinContrast`, 135-142): the
  output is a zero whose sign differs between the platforms.
- **Who notices:** NaN pixels through a linear GradingPrimaryTransform with infinite offsets
  of opposite signs, compared between a Windows and a Linux machine.
- **A fix:** one operand order on every platform, which changes those zeros' signs on one.
- **Status:** matched in `p5-primary` (5.2b, 5.2c:
  `crates/ocio-ops/src/ops/gradingprimary/grading_primary_op_cpu.rs`, `LoopOrders`); the
  battery's infinite-sum cases with NaN probes check it on both platforms.

### I-241. A channel's inverse midtones above the curve use the line below it

- **Upstream:** the inverse midtones of the red, green or blue component
  (`GradingToneRevOpCPU::mids`, `ops/gradingtone/GradingToneOpCPU.cpp:381-384` @ v2.5.2) give a
  value at or above the curve's top, `t >= y5`, `x0 + (t - y0) / m0`: the line below the curve's
  bottom, where the forward curve continues above its top with `y5 + (t - x5) * m5`, and the
  master component's code uses the top line (`outR4 = x5 + (t - y5) / m5`, 441). The
  pre-render makes `y0 = x0` and `m0 = m5 = 1`, so the bottom line gives the value back
  unchanged, the top line `t - y5 + x5`; the curve's area-preserving slopes make `y5` equal to
  `x5` only up to their rounding, so a value above the top comes back off by that rounding
  (or exactly, where it rounds to nothing).
- **Who notices:** nobody visibly: inverting a GradingToneTransform with a red, green or blue
  midtones value other than 1, on values above the curve's top, gives the last bits of the
  bottom line's answer.
- **A fix:** `x5 + (t - y5) / m5`, as the master component's code.
- **Status:** matched in `p5-tone` (5.3b, `crates/ocio-ops/src/ops/gradingtone/
  grading_tone_op_cpu.rs`, `mids_rev_scalar`); the battery's midtones cases, inverse, with
  values above 1 (`crates/ocio-ops/tests/grading_tone_oracle.rs`).

### I-242. A channel and the master component take different midtones segments on a knot

- **Upstream:** the forward midtones of the red, green or blue component choose the segment
  with strict comparisons (`if (t > x2) res = fR;`, `GradingToneOpCPU.cpp:310-315` @ v2.5.2),
  the master component's code with `setOnLimit` (`t < x2 ? res : fR`, 337-342): a value on
  the knot `x2`, `x3` or `x4` takes the segment below it in the first, above it in the second
  (the same value, up to the segments' rounding), and a NaN takes the middle segment `fM` in the
  first and the line above the top in the second. The inverse midtones differ the same way
  (an if/else chain on `t >= y5, y4, ...` for the components, `setOnLimit` for the master).
- **Who notices:** nobody visibly: the segments meet at the knots, so only the last bits and a
  NaN's payload differ.
- **A fix:** one choice of segment for both.
- **Status:** matched in `p5-tone` (5.3b, `grading_tone_op_cpu.rs`, `mids_fwd_scalar`,
  `mids_fwd_f3`, `mids_rev_scalar`, `mids_rev_f3`); the battery probes the knots' neighbourhoods
  (`breakpoints`).

### I-243. A GradingTone's NaN pixels and NaN coefficients differ between Windows and Linux

- **Upstream:** the log and video renderers (`GradingToneOpCPU.cpp:269-1061` @ v2.5.2) are
  scalar `float` templates instantiated for a `float` and a `float3`, which the two wheels
  compile with different operand orders: per platform, per instantiation, and with GCC per
  lane of the `float3` (Windows `sub_1801f38f0` to `sub_1801f9b10`, Linux `ComputeHSFwd<float>`
  and its siblings, read with `tools/wheel-inspect`). MSVC also computes some roots as `x0 -
  ((c + c) / (d + b)) * w` for the source's `((-2.f * c) / (d + b)) * w + x0`. Where two NaNs
  meet (a pixel's NaN and a NaN control point, which starts and widths that overflow a `float`
  give, or the default NaN of an infinity's arithmetic and a control point the pre-render
  negated), the result keeps a different NaN on each platform, and between the red, green,
  blue and the master component on one.
- **Who notices:** anyone comparing a GradingToneTransform with extreme starts or widths on NaN
  or infinite pixels between a Windows and a Linux machine.
- **A fix:** one operand order on every platform, which changes NaN bits on at least one.
- **Status:** matched in `p5-tone` (5.3b, `grading_tone_op_cpu.rs`, the `HS_FWD` ... `SC_REV`
  tables); the battery's overflowing cases with the distinct-NaN probes check it on both
  platforms.

### I-244. The linear GradingTone converts alpha to its log space and back

- **Upstream:** the linear style's renderers grade in a log space: `LinLog` before the tones,
  `LogLin` after (`ops/gradingtone/GradingToneOpCPU.cpp:1086-1145, 1147-1259` @ v2.5.2). Their
  `OCIO_USE_SSE2` branches, which every x86-64 wheel compiles, load and store the whole
  pixel, so alpha goes through both conversions too: `LogLin(LinLog(alpha))`, with OCIO's
  `sseLog2` and `ssePower` approximations, which gives alpha back only approximately (and a
  NaN alpha as the line's NaN, an infinite one as whatever the approximations make of it).
  The scalar branch (`#else`) copies alpha, as the log and video styles leave it.
- **Who notices:** anyone relying on a linear GradingToneTransform to keep alpha exactly,
  such as a compositing pipeline comparing alphas.
- **A fix:** keep the input's alpha, as the other grading renderers do (`out[3] = in[3]`).
- **Status:** matched in `p5-tone` (5.3c, `crates/ocio-ops/src/ops/gradingtone/
  grading_tone_op_cpu.rs`, `lin_log_pixel`, `log_lin_pixel`); the battery compares alpha bit
  for bit for the linear style (`crates/ocio-ops/tests/grading_tone_oracle.rs`).

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
- **Decided** (the owner's general rule, `docs/deviations.md`): the port refuses the LUT with
  an error where the padding doesn't fit, the limit is 0, or a texture 1 texel wide has more
  than one row ("The Lut1DOp of N entries doesn't fit in a texture at most W texels wide.").
- **Status:** matched in `p2-lut1d-gpu` (2.1h, `padding_fits` in
  `crates/ocio-gpu/src/ops/lut1d/lut1d_op_gpu.rs`, and its test of the lengths around the
  limits). The oracle refuses these requests (`gpu_shader`, `_padding_fits`).

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

### U-30. A FixedFunction style from a null name

- **Upstream:** `FixedFunctionOpData::GetStyle` refuses a null or empty name with "Unknown
  FixedFunction style: " followed by the name, appending the `const char *` to a `std::string`
  (`ops/fixedfunction/FixedFunctionOpData.cpp:189-368`). For a null pointer that is undefined
  behaviour. The CTF reader calls it with an attribute's value, which is never null.
- **Decided** (general rule): `FixedFunctionOpStyle::from_name(None)` refuses it as an empty
  name: "Unknown FixedFunction style: ".
- **Status:** matched in `p2-ff-cpu` (2.3a1).

### U-31. Queries of a FixedFunction op whose style has too few parameters

- **Upstream:** the setters take any number of parameters, and only `validate` checks how many
  the style takes (`ops/fixedfunction/FixedFunctionOpData.cpp:617-840`). Before that,
  `isInverse` of two Rec.2100 surrounds of the same style reads both first parameters without
  a check (`FixedFunctionOpData.cpp:842-854`), and these renderers read their parameters
  without one: the ACES 1.3 gamut compression's seven
  (`ops/fixedfunction/FixedFunctionOpCPU.cpp:984-1002`), the Rec.2100 surround's one
  (`FixedFunctionOpCPU.cpp:1406-1417`), the gamma-log's ten and the double-log's 13
  (`FixedFunctionOpCPU.cpp:2230-2246, 2322-2344`). On a shorter vector, they read past its
  end. The processors validate their ops first (`OpRcPtrVec::finalize`), so only code that
  queries such data directly gets there.
- **Decided** (general rule): `FixedFunctionOpData::is_inverse` and those renderers
  (`RendererAcesGamutComp13Fwd::new`, `RendererRec2100Surround::new`,
  `RendererLinToGammaLog::new`, `RendererLinToDoubleLog::new`, so
  `get_fixed_function_cpu_renderer`) return an error there, and only there: "FixedFunctionOp:
  the style has fewer parameters than it uses: upstream reads past them."
- **Status:** matched in `p2-ff-cpu` (2.3a2); `fixed_function_op_data_tests.rs` checks the
  error, and that the comparisons upstream makes without reading (another style, or an inverse
  that validation refuses) give upstream's answers. The renderers in 2.3b, and in 2.3c1 and
  2.3d1 (`p2-ff-cpu-2`), and ACES 2.0's (`p2-aces2-cpu`, 2.4e1, 2.4e2, through `param_f32`):
  `fixed_function_op_cpu_tests.rs` checks their errors.
  The GPU writer reads the ACES 1.3 gamut compression's parameters the same way in `p2-ff-gpu`
  (2.3f, `fixed_function_op_gpu.rs`), and the Rec.2100 surround's, the gamma-log's and the
  double-log's (2.3g1, 2.3g2), and ACES 2.0's (`p2-aces2-gpu`, 2.4f1-2.4h);
  `fixed_function_op_gpu.rs` checks their errors.

### U-32. ACES 2.0's hue table past its arrays

- **Upstream:** the ACES 2.0 fixed functions build their hue table from the corners of the
  limiting and the reach gamuts (`ops/fixedfunction/ACES2/Transform.cpp:646-736`).
  `extract_sorted_cube_hues` merges the two sorted lists of corner hues with comparisons that
  NaN hues never satisfy, so it reads past the 8 corners and writes past the 12 sorted hues;
  `build_hue_table` writes `samples` entries into the 363-entry table from counts that, as
  its own `BUG` notes say, can overrun it. Limiting primaries with a NaN coordinate, which
  `FixedFunctionOpData::validate` accepts (it checks only the peak luminance), get there: the
  wheel's process stops on Windows with `0xc0000409` (the stack cookie check) when it builds
  the processor.
- **Decided** (general rule): the port refuses such parameters where upstream would read or
  write past an array: "ACES 2.0: the gamut's corner hues make the hue table read or write
  past its arrays: upstream's behaviour is undefined." (`aces2::transform::CORNERS_OVERRUN`).
- **Status:** matched in `p2-aces2-cpu` (2.4c); `aces2/transform_tests.rs` checks the error.
  The GPU writer refuses the same parameters before it writes anything (`p2-aces2-gpu`,
  2.4f1-2.4h); `fixed_function_op_gpu.rs` checks it for the output transform and the gamut
  compression, both directions.

### U-35. An RGB curve with a NaN x coordinate

- **Upstream:** `AdjustRGBSlopes` (`ops/gradingrgbcurve/GradingBSplineCurve.cpp:450-488`) walks
  the fitted knots and pairs each knot that isn't a control point's x with the control
  points `i` and `i + 1`, counting on every control point's x to match its own knot. A NaN x
  matches nothing (`NaN != NaN`), so the walk counts that knot as a middle knot too, and
  reaches `ctrlPnts[i + 1]` past the last control point: it reads memory it doesn't own. A
  NaN x passes `GradingBSplineCurve::validate` (`x < lastX` is false), so a
  GradingRGBCurveTransform with one renders after reading it.
- **Who notices:** applications that set a NaN x coordinate on an RGB curve.
- **Decided** (general rule): `adjust_rgb_slopes` returns "RGB curve: fitting the curve would
  read past its control points." where upstream would read past them, so fitting the curves
  (the dynamic property's `precompute`, building the op data) fails there.
- **Status:** matched in `p2-rgbcurve` (2.6b), `grading_b_spline_curve.rs`; the GradingRGBCurve
  battery (2.6d) doesn't vary x coordinates (only y and the slopes), so it never sends
  such a curve to the wheel; `grading_b_spline_curve_tests.rs` checks the error.

### U-45. The working directory when `_getcwd` fails

- **Upstream:** `GetCwd` (`PathUtils.cpp:131-150` @ v2.5.2) on Windows calls
  `_getcwd(path, MAXPATHLEN)` into an uninitialized `char path[4096]` and returns `path` without
  checking the result. When the call fails (a working directory of 4096 bytes or more, or one
  the system can't give), the buffer is read uninitialized, up to whatever NUL follows.
- **Who notices:** `AbsPath` callers (the config loader, for a relative path) on Windows with a
  very long or removed working directory.
- **Decided** (the owner's general rule, with a clear message): the port returns the error "The
  current working directory could not be read (_getcwd failed)." from `abs_path`. Linux needs
  nothing: its buffer is zeroed and grown until the path fits, and another failure gives "".
- **Status:** p3-context 3.5f (`crates/ocio/src/path_utils.rs`, `get_cwd`).

### U-46. An environment variable of 32,767 UTF-16 code units

- **Upstream:** on Windows, `Setenv` and `Unsetenv` call `_wputenv_s` (`Platform.cpp:99-142` @
  v2.5.2), whose parameter validation refuses a name or a value of `_MAX_ENV` (32,767) UTF-16
  code units or more by calling the invalid parameter handler, which ends the wheel's process
  (status 0xC0000409). 32,766 works. Linux's `setenv` takes any length.
- **Who notices:** a caller of `SetEnvVariable` or `UnsetEnvVariable` with such a name or value,
  on Windows.
- **Decided** (the owner's general rule, with a clear message): `set_env_variable`,
  `unset_env_variable` (and `platform::setenv`, `unsetenv`) return a `Result`, the error
  "Environment variable names and values must be shorter than 32767 UTF-16 code units
  (_MAX_ENV)." on Windows, and change nothing.
- **Status:** p3-context, the fix chunk after the verifier's review (`crates/ocio-ops/src/
  platform.rs`, `put`); checked against the wheel, whose process ends, in
  `crates/ocio/tests/env_oracle.rs`.

### U-50. A look's transform set to null

- **Upstream:** `Look::setTransform` and `Look::setInverseTransform` call
  `transform->createEditableCopy()` without checking the pointer (`Look.cpp:116-129`), so a
  null transform dereferences null. Through the wheel, `Look().setTransform(None)` ends the
  Python process with an access violation on both platforms: exit code 0xC0000005
  (-1073741819) on Windows, a segmentation fault (a shell reports 139, 128 + SIGSEGV) on Linux.
  The color space's, view transform's and named transform's setters check for null and remove
  the transform instead.
- **Who notices:** code and Python scripts that clear a look's transform by setting it to null.
- **Decided** (general rule): the Rust setters take a `&Transform`, which can't be null; the
  Python module (Phase 6) refuses `None` with an error instead of crashing.
- **Status:** matched in `p3-model-objects` (3.4b, `look.rs`); the Python part is Phase 6's.

### U-51. A color space set given a null color space or a null set

- **Upstream:** `ColorSpaceSet` dereferences the pointers it is given without checking them:
  `addColorSpace(nullptr)` (`cs->getName()`, `ColorSpaceSet.cpp:122`), `addColorSpaces(nullptr)`
  and `removeColorSpaces(nullptr)` (`*css->m_impl`, 298, 308), and the operators `||`, `&&` and
  `-` with a null right-hand set (`ColorSpaceSet.cpp:320, 329, 350`). Through the wheel,
  `s.addColorSpace(None)`, `s.addColorSpaces(None)`, `s.removeColorSpaces(None)`, `s | None`,
  `s & None` and `s - None` end the Python process with an access violation (0xC0000005 on
  Windows, a segmentation fault on Linux). `s == None` is `False` (Python's own fallback).
- **Who notices:** code and Python scripts that pass a missing color space or set.
- **Decided** (general rule): the Rust methods take `&ColorSpace` and `&ColorSpaceSet`, which
  can't be null; the Python module (Phase 6) refuses `None` with an error instead of crashing.
- **Status:** matched in `p3-model-objects` (3.4b, `color_space_set.rs`); the Python part is
  Phase 6's.

### U-52. The transform of a named transform that has none

- **Upstream:** `NamedTransform::GetTransform(nt, dir)` (`NamedTransform.cpp:182-221`) inverts
  the other direction's transform when `dir`'s is null, without checking that one: for a named
  transform with neither transform it dereferences null (`inverseTransform->createEditableCopy()`,
  194, or `forwardTransform->...`, 209). Through the wheel,
  `NamedTransform.GetTransform(NamedTransform(), TRANSFORM_DIR_FORWARD)` ends the Python process
  with an access violation; so does any processor of such a named transform.
- **Who notices:** code that asks a named transform without transforms for its transform.
- **Decided** (general rule): return an error instead (its text is decided with the port).
- **Status:** done in `p3-builders` (3.2a, `crates/ocio/src/named_transform.rs`,
  `NamedTransform::get_transform`): "Named transform: it has neither a forward nor an inverse
  transform."; found by the `p3-model-objects` verifier.

### U-53. Regular expressions nested too deep for the wheel's stack

- **Upstream:** each wheel's `std::regex` parser recurses once per nested group. OCIO compiles
  a file rule's regular expression when it is set (`ValidateRegularExpression`,
  `FileRules.cpp:263-282`) and again for each match, so a rule nested deep enough ends the
  process with a stack overflow. The Windows wheel overflows at about 7,800 nested lookaheads
  (`(?=`) or 8,400 nested non-capture groups (`(?:`), measured in a Python process of its own;
  the depth varies between runs and with the caller's stack. Capture groups stop earlier: MSVC
  refuses a thousandth with `error_stack`.
- **Who notices:** configs and applications with machine-made rules; no real rule nests so
  deep.
- **Decided** (general rule, the wheel crashes): the port refuses groups nested deeper than
  5,000 with `error_stack`, the error MSVC gives its own limit, and compiles a deeply nested
  expression on a thread with the stack it needs. Between 5,000 levels and the wheel's limit
  the port refuses what the wheel accepts; the limit is a choice for the owner.
- **Status:** matched in `p3-regex` (3.9a, `std_regex/msvc.rs`). The Linux wheel's limits, in
  compiling (3.9c) and in matching (3.9d), are U-54.

### U-54. Regular expressions and texts too large for the Linux wheel's stack

- **Upstream:** libstdc++'s regex compiler recurses once per term of an alternative and once
  per nested group, and its matcher once per state it goes through. The Linux wheel's stack
  overflows (the process ends):
  - compiling about 75,000 terms in a row or about 14,100 to 14,500 nested groups;
  - matching, about 74,900 frames deep. Measured (2026-10-05/06) as the path length of `a`s at
    which a match crashes it: `a*` 32,719, `.*` 32,727, `(a)*` 17,460, `((a))*` 11,903,
    `((((a))))*` 7,272, `(((((a)))))*` 6,085, `(a|b)*` 14,150, `(?:a|b)*` 22,769, `(?:b|a)*`
    32,726, `(?:(a)|b)*` 14,125, `a*?` 58,197, `(a)*?` 22,771, `(?:a?)*` 20,943, `(?:aa)*`
    22,773 (pairs), `((a)(b))*` 8,041 (pairs), `(?:a{2,3})*` 13,361 (triples); `a` followed by
    19,353 `*` on `a`, by 28,993 on the empty text. Every one is about 74,900 frames when a
    character, assertion or back reference takes 1, a capture group 2, an alternative other than
    the last 1 (the last none), a greedy loop iteration 1.29, a lazy one 0.29, and non-capture
    groups, lookaheads (their frames go when they end) and a brace's copies none. The depths
    vary a little between runs.
- **Who notices:** configs and applications with machine-made rules, or very long paths. Real
  file rules (a few groups, paths under 4 KB) are far from the limits.
- **Decided** (owner, 2026-10-05: fixed limits below the crash points, and for matching a
  conservative cost bound at most half the lowest crash; each error the closest existing one):
  - compiling: an expression of more than 50,000 terms is refused with the state limit's
    `error_space` ("Number of NFA states exceeds limit. ..."); groups nested deeper than 5,000
    are refused with `error_stack`, whose `what()` is `regex_error` (a code without a message);
  - matching: a text longer than 8,192 bytes is refused with that `error_stack`; and so is a
    match whose estimated depth, along the path it tries (the weights above), would pass 37,000
    frames: under half of the lowest crash measured (74,829 frames, `((((a))))*`). So
    `(a|b)*` is refused from 6,993 `a`s (the wheel crashes at 14,150), `((((a))))*` from 3,595
    (7,272), `a` with 9,561 stars on `a` (19,353);
  - and a match the port's own recursion would take past 192 MiB of its thread's stack
    (thousands of nested groups that cost the wheel nothing) is refused the same way.
  Between these limits and the wheel's crashes the port refuses what the wheel accepts.
- **Status:** matched in `p3-regex`: the compile limits in 3.9c (`std_regex/libstdcxx.rs`), the
  match limits in the fix chunk after 3.9d's verifier (`std_regex/libstdcxx_match.rs`). Tests:
  the exact bound, the wheel surviving (in a process of its own) where the port starts
  refusing, and typical file rules on 4,096-byte paths never refused.

### U-55. Upgrading a version 1 config without a scene color space for the default rule

- **Upstream:** `Config::upgradeToLatestVersion` is `noexcept` (`Config.cpp:1332-1350`). For a
  version 1 config, it calls `UpdateFileRulesFromV1ToV2` (`FileRules.cpp:960-1047`), which,
  when the config has no `default` role, no data `raw` color space, no data scene color space
  and no active color space, gives the default rule the first scene color space: `""` when
  there is none (no color space, or only display ones). `FileRules::setColorSpace` throws for
  an empty name, and the exception leaving a `noexcept` function ends the program
  (`std::terminate`). The same happens when the rule at index 1, which gets that color space
  (I-138), is the path search rule (rules `[g, ColorSpaceNamePathSearch, Default]` on a
  version 1 config without a `default` role): `setColorSpace` throws "File rules:
  ColorSpaceNamePathSearch rule does not accept any color space."; through the Windows wheel
  the process exits with 127. Seen in the source and in that probe; the oracle can't run a
  case whose process ends.
- **Who notices:** applications that upgrade a version 1 config made in code before giving it
  color spaces.
- **Decided** (the owner's general rule, `docs/deviations.md`): the port returns that error
  from `Config::upgrade_to_latest_version`, and leaves the config as it was.
- **Status:** matched in `p3-rules` (3.9f; both triggers tested in
  `crates/ocio/src/config_tests.rs`); the other paths of the upgrade are checked against the
  wheel in `crates/ocio/tests/config_oracle.rs`.

### U-56. Null rules, and a null rule name for empty viewing rules

- **Upstream:** `Config::setFileRules(nullptr)` and `setViewingRules(nullptr)` call
  `createEditableCopy` on the null pointer (`Config.cpp:4648-4654, 3337-3343`), and
  `ViewingRules::getIndexForRule(nullptr)` on rules without a rule writes the null pointer to a
  stream for its message (`ViewingRules.cpp:212-226`). Through the Windows wheel,
  `config.setFileRules(None)`, `config.setViewingRules(None)` and
  `OCIO.ViewingRules().getIndexForRule(None)` end the Python process with an access violation.
  With a rule, `getIndexForRule(None)` raises `Platform::Strcasecmp`'s "String pointer for
  comparison must not be null." (`Platform.cpp:155-160`), as `FileRules.getIndexForRule(None)`
  always does (its rules have the default rule).
- **Who notices:** code and Python scripts that pass a missing rules object or rule name.
- **Decided** (general rule): the Rust methods take `&FileRules`, `&ViewingRules` and byte
  strings, which can't be null; the Python module (Phase 6) refuses `None` with an error
  instead of crashing, and with Strcasecmp's message where the wheel raises it.
- **Status:** matched in `p3-rules` (3.9g); the Python part is Phase 6's.

### U-57. An inverse view transform after looks that name no look

- **Upstream:** inverse, `BuildDisplayOps` finds the color space the view transform starts from
  as the result of the view's looks (`LooksResultColorSpace`,
  `transforms/DisplayViewTransform.cpp:486-491`). When the first option of the looks names no
  look, no look runs and the result is `""`, which no color space has. That happens for a list
  of empty names (`looks: ", "`, `"+"`, `"-"`, `'""'`, `"+, -"`), also when fallback options
  follow it (`"|x"`, `" | x"`, `" | lk0|lk0"`). The source color space is then null, and
  `BuildDisplayToSource` reads its reference space type through it
  (`DisplayViewTransform.cpp:256-258`). Through the wheel,
  `getProcessor(DisplayViewTransform(src, display, view, direction=TRANSFORM_DIR_INVERSE))`
  ends the Python process (an access violation, 0xC0000005, for each of these lists on
  Windows, 2026-10-09). Without a view transform the same null color
  space gives the defined "BuildColorSpaceOps failed, null dstColorSpace.", which the port
  matches.
- **Who notices:** a config with such a view, inverted.
- **Decided** (general rule): return an error instead: "DisplayViewTransform error. The looks
  leave no color space to apply the view transform from."
- **Status:** done in `p3-builders` (3.2c, `crates/ocio/src/transforms/
  display_view_transform.rs`, `source_reference_space`). Tests: each of these lists, inverse
  refused and forward built.

### U-58. Transforms that refer to themselves, or nest deep enough to overflow the stack

- **Upstream:** building a processor follows the transforms a transform names, recursively, in
  `CollectContextVariables` and in `BuildOps`: a color space transform builds its color spaces'
  transforms (`transforms/ColorSpaceTransform.cpp:165-346`), a look transform its looks' and
  their process spaces' (`transforms/LookTransform.cpp:203-305`), a display view transform its
  view's (`transforms/DisplayViewTransform.cpp:177-528`). Nothing bounds the depth, so a color
  space whose transform names itself (`to_scene_reference: !<ColorSpaceTransform> {src: self,
  dst: lin}`), or reaches itself through a look's process space or a default view transform,
  recurses until the stack overflows and the process ends (0xC00000FD on Windows). A long
  legal chain of color spaces, each converting from the one before, does the same. The wheel's
  `getProcessor` crashes at these chain lengths (capped binary searches, 2026-10-09):

  | Each link | Windows | Linux |
  |---|---|---|
  | `ColorSpaceTransform` | 1,363 | 8,058 |
  | `LookTransform` with a look | 618 | 4,154 |
  | `GroupTransform` holding a `ColorSpaceTransform` | 794 | 5,185 |
  | `DisplayViewTransform`, a view with a look | 573 | 3,609 |
  | `DisplayViewTransform`, a view transform and a look | 573 | 3,611 |

- **Who notices:** a config with a cycle (a mistake: upstream crashes on it), or a
  machine-made chain hundreds of color spaces deep.
- **Decided** (general rule, precedent U-60: refuse at about half the lowest measured crash;
  coordinator, 2026-10-09): `BuildOps` and `CollectContextVariables` each refuse to nest more
  than 256 levels (`MAX_TRANSFORM_NESTING`), with "The transforms nest more than 256 levels
  deep: a color space, look, view transform or named transform may refer to itself." Group
  transforms don't count: they name nothing, so they can't refer to themselves, and the loader
  refuses groups nested more than 590 deep (U-60), whose processors the port builds as the
  wheel does. The port accepts chains of 255 color space transforms (in groups or not) and of
  254 look or display view transforms. Between that and the wheel's crash the port refuses
  what the wheel builds.

  Groups multiply that depth, though: 255 color spaces, each a color space transform inside 5
  nested groups, needed 1,156 KiB of the port's stack at opt-level 1 (the verifier,
  2026-10-17), and 20 groups 3.7 MiB. And each kind of level costs the stack its own amount.
  The shortest chains that crash the wheel's `getProcessor`, by the kind of each link (Windows,
  Python's main thread, one process per length under a memory cap, 2026-10-17; "then files":
  the first link's transform a file transform starting a chain of 256 CTF references, U-76):

  | Each link | Crashes at | Half | A link's cost, in color space transforms |
  |---|---|---|---|
  | `ColorSpaceTransform` | 1,363 | 681 | 1 |
  | `LookTransform` with a look, one option (`k`) or two (`k\|j`) | 618 | 309 | 2.21 |
  | `DisplayViewTransform`, a view with a look, without or with a view transform, one option or two | 573 | 286 | 2.38 |
  | `ColorSpaceTransform` in 1 group | 794 | 397 | 1.72 |
  | `DisplayViewTransform` (a view with a look) in 1 group | 440 | 220 | 3.10 |
  | `ColorSpaceTransform`, then files | 1,193 | 596 | |
  | `LookTransform`, then files | 541 | 270 | |
  | `DisplayViewTransform`, then files | 502 | 251 | |
  | `ColorSpaceTransform` of a named transform holding a `ColorSpaceTransform` | 721 | 360 | 1.89 |
  | `LookTransform` of a look holding a `ColorSpaceTransform` | 452 | 226 | 3.02 |
  | `DisplayViewTransform` of a view whose look holds a `DisplayViewTransform` | 298 | 149 | 4.57 |

  and with groups around each link, a group counting as a level (the verifier's measures, the
  same day; the depth varies by a level with the caller's stack):

  | Groups around each link | Longest chain the wheel builds | Levels, groups included |
  |---|---|---|
  | 0 | 1,363 | 1,363 |
  | 1 | 794 | 1,588 |
  | 2 | 559 | 1,677 |
  | 3 | 432 | 1,728 |
  | 5 | 297 | 1,782 |
  | 10 | 166 | 1,826 |
  | 20 | 88 | 1,848 |
  | 50 | 36 | 1,836 |
  | 100 | 18 | 1,818 |
  | 200 | 9 | 1,809 |

  A chain of CTF files alone crashes at 2,125 files (U-76).

  So `BuildOps` and `CollectContextVariables` also weigh every level they nest, groups
  included, and refuse a total past 68,150 (`MAX_NESTING_WEIGHT`), with "The transforms nest
  too deep, counting groups and CTF references: upstream's stack overflows." (the verifier's
  finding, 2026-10-17: a plain count of 640 levels, the first total, let a display view
  transform in 1 group reach 254 links where the wheel crashes at 440). Each level weighs
  what it costs the wheel's stack, from the crashes above, in hundredths of a color space
  transform's, rounded up: a color space transform (and any transform that names nothing, which
  ends the chain) 100, a look transform 221 (1,363 / 618), a display view transform 238
  (1,363 / 573), a group 72 (1,363 / 794 - 1), a file transform and each CTF file of a chain of
  references 67 (`(1,363 - 1,193) / 256`; 2,125 files alone give 0.64). The total is half the
  wheel's 1,363 color space transforms: every kind of chain then stops at about half the wheel's
  crash, or before. A file transform reached through `BuildOps` counts twice, as a transform and
  as the first file of its chain. The 256 transforms and the 256 files still count apart too.
  The deepest group the loader reads (590 groups and the transform they hold, 591 levels, U-60)
  weighs 42,580, and its processor builds as in the wheel. The longest chains the port accepts:

  | Each link | Port | Wheel |
  |---|---|---|
  | `ColorSpaceTransform`, alone or in 1 group | 255 (the 256 transforms) | crashes at 1,363, 794 |
  | `ColorSpaceTransform` in 2 groups | 255 (the 256 transforms) | builds 559 |
  | `ColorSpaceTransform` in 5 groups | 147 | builds 297 |
  | `LookTransform`, one option or two | 254 (the 256 transforms) | crashes at 618 |
  | `DisplayViewTransform`, any view | 254 (the 256 transforms) | crashes at 573 |
  | `DisplayViewTransform` in 1 group, one option or two | 219 | crashes at 440 |
  | `ColorSpaceTransform`, then files | 255 | crashes at 1,193 |
  | `LookTransform`, then files | 231 | crashes at 541 |
  | `DisplayViewTransform`, then files | 214 | crashes at 502 |
  | A named transform, a look or a view whose transform links to the next | at most 128 (two transforms a link) | crashes at 721, 452, 298 |

  The limits must also hold on the port's own stack, at every optimization level, on a thread
  of 1 MiB (the main thread's on Windows). At opt-level 0 the first port didn't: an
  unoptimized frame keeps a slot for every temporary (each `?` holds several copies of the
  56-byte result), and a display view transform took about 11.7 KB a level, so only 86
  levels fit. The functions the build recurses through now keep their frames small (their
  lookups, errors and copies out of line, in `#[inline(never)]` and `#[cold]` helpers, the
  transforms that name nothing in their own function, the op list of a look's option boxed and
  that option's loop inline, and the recursive calls' results returned as they are, without
  `?`; the same for a chain of CTF references, U-76). What the longest chain of each kind
  takes, both ways, alone and with its first link a chain of 256 CTF references, which
  shortens the chain to what the total allows beside them (Windows, 2026-10-17; Windows rounds
  a thread's stack to 64 KiB, so each figure is the smallest such stack that holds the chain,
  plus 1 KiB):

  | Each link | n | opt-level 0 | n, then files | opt-level 0 | opt-level 1 | then files |
  |---|---|---|---|---|---|---|
  | `ColorSpaceTransform` | 255 | 577 KiB | 255 | 641 KiB | 257 KiB | 321 KiB |
  | `LookTransform` with a look | 254 | 641 KiB | 231 | 769 KiB | 344 KiB | 449 KiB |
  | `LookTransform`, two options | 254 | 705 KiB | 231 | 897 KiB | 324 KiB | 449 KiB |
  | `GroupTransform` holding a `ColorSpaceTransform` | 255 | 705 KiB | 255 | 769 KiB | 321 KiB | 385 KiB |
  | `DisplayViewTransform`, a view with a look | 254 | 769 KiB | 214 | 769 KiB | 385 KiB | 449 KiB |
  | `DisplayViewTransform`, a view transform and a look | 254 | 769 KiB | 214 | 769 KiB | 385 KiB | 449 KiB |
  | `DisplayViewTransform`, a view with two options | 254 | 769 KiB | 214 | 897 KiB | 385 KiB | 449 KiB |
  | `DisplayViewTransform`, a view transform and two options | 254 | 769 KiB | 214 | 897 KiB | 385 KiB | 449 KiB |
  | `ColorSpaceTransform` in 1 group | 255 | 705 KiB | 255 | 769 KiB | 321 KiB | 385 KiB |
  | `ColorSpaceTransform` in 2 groups | 255 | 897 KiB | 209 | 833 KiB | 385 KiB | 449 KiB |
  | `ColorSpaceTransform` in 5 groups | 147 | 833 KiB | 111 | 833 KiB | 385 KiB | 385 KiB |
  | `ColorSpaceTransform` in 20 groups | 44 | 705 KiB | 34 | 769 KiB | 321 KiB | 385 KiB |
  | `ColorSpaceTransform` in 200 groups | 4 | 577 KiB | 4 | 705 KiB | 257 KiB | 321 KiB |
  | `LookTransform` in 2 groups | 186 | 705 KiB | 140 | 769 KiB | 385 KiB | 449 KiB |
  | `LookTransform` in 1 group, two options | 231 | 769 KiB | 174 | 897 KiB | 385 KiB | 449 KiB |
  | Group of a `ColorSpaceTransform` in 3 groups | 175 | 833 KiB | 132 | 833 KiB | 385 KiB | 385 KiB |
  | `DisplayViewTransform` in 1 group | 219 | 769 KiB | 164 | 769 KiB | 448 KiB | 449 KiB |
  | `DisplayViewTransform` in 1 group, two options | 219 | 769 KiB | 164 | 897 KiB | 392 KiB | 449 KiB |
  | `DisplayViewTransform` in 3 groups | 149 | 769 KiB | 112 | 769 KiB | 385 KiB | 449 KiB |
  | `DisplayViewTransform` (view transform) in 10 groups | 70 | 651 KiB | 54 | 769 KiB | 322 KiB | 385 KiB |
  | `DisplayViewTransform` (view transform, two options) in 2 groups | 177 | 769 KiB | 134 | 839 KiB | 385 KiB | 449 KiB |
  | `ColorSpaceTransform`, the first 80 links in 3 groups, the others in 2 | 255 | 961 KiB | 186 | 833 KiB | 385 KiB | 449 KiB |
  | `ColorSpaceTransform`, links 2 to 99 in 2 groups, the others in 1 | 255 | 769 KiB | 255 | 833 KiB | 321 KiB | 449 KiB |

  The heaviest is the verifier's mix (2026-10-17): 255 color space transforms, 80 in 3 groups
  and 175 in 2, 590 groups in all, weigh 68,080 and take 961 KiB at opt-level 0; the second
  mix is the heaviest then files (68,063). On Rocky Linux 9 at opt-level 0, where a thread's
  stack isn't rounded, the heaviest take 902 KiB (that mix), 851 KiB (color space transforms
  in 2 groups) and 829 KiB (display view transforms of two options, then files); the two mixes
  then files 754 and 762 KiB.
- **Status:** done in `p3-builders` (3.2d follow-up; `crates/ocio/src/transform.rs`, `Nesting`;
  `build_ops`, and `collect_context_variables` in `crates/ocio/src/context_variable_utils.rs`).
  The small frames in `group-5` (`transform.rs`, `transforms/color_space_transform.rs`,
  `look_transform.rs`, `display_view_transform.rs`, `group_transform.rs`,
  `file_transform.rs`, `fileformats/file_format_ctf.rs`), and for two look options in
  `p4-ctf-bake` (`look_transform.rs`, `build_first_look_option`); the total in `p4-ctf-bake`
  (`transform.rs`, `MAX_NESTING_WEIGHT`, `Level` and `Nesting`; the files' level in
  `file_transform.rs`). Tests (`crates/ocio/tests/config_processors_oracle.rs`): a color space
  that names itself and one that reaches itself through a look are refused in both directions
  where the wheel crashes; `the_deepest_chains_below_the_limit_match_the_wheel`: the longest
  chain of each kind the port accepts, alone and then 256 CTF files (`KINDS`: groups around the
  links, two look options and the two mixes of the table included), has the length the weights
  give, written in the test, and is the wheel's processor (on a test thread's stack, debug and
  release), and one link more the wheel still builds while the port refuses;
  `groups_count_in_the_total_nesting` (the verifier's 5 groups) and
  `groups_count_in_the_total_without_the_processor_cache` (the same with the processor cache
  off; the processor still collects the context variables first, as upstream's does);
  `the_deepest_chains_build_on_a_1_mib_thread` and
  `the_deepest_chains_of_transforms_then_of_references_build_on_a_1_mib_thread` build the
  longest chain of each kind of the table both ways on a 1 MiB thread, and
  `a_chain_of_color_spaces_then_of_references_builds_on_a_1_mib_thread` checks that mixed
  chain's processor with the wheel; `the_nesting_weight_limit_is_exact`
  (`crates/ocio/src/transform_tests.rs`) pins the limit and the weights of groups, file
  transforms and files: 766 groups around a file transform of 193 CTF files weigh 68,150 and
  build, 726 groups around one of 236 files weigh 68,151 and are refused, through `BuildOps`
  alone and through the processor with its cache on and off, on a 1 MiB thread. A nightly job of `.github/workflows/sde.yml` runs them, with
  `chains_of_references_stop_at_the_limit` (U-76) and `deep_groups_fit_a_small_stack` (U-60),
  at opt-level 0 on Windows and Rocky Linux 9.

### U-60. GroupTransforms nested deep enough to overflow the wheel's stack

- **Upstream:** a GroupTransform loads each child through `load(const YAML::Node&,
  TransformRcPtr&)`, which loads a child group the same way: the loader recurses once per
  nested group (`OCIOYaml.cpp:2520-2525, 3196-3351`). YAML aliases let a short config nest
  groups to any depth (each anchor a group of the one before), and a group that holds itself
  through an alias (`&t !<GroupTransform> {children: [*t]}`) nests forever. The wheel's stack
  then overflows (the process ends): at 1,182 levels in Python's main thread on Windows and
  5,129 on Linux (an anchor chain, measured 2026-10-06), and on a group that holds itself on
  both. The depth varies with the caller's stack. Without aliases, yaml-cpp refuses the flow
  text of more than 247 nested groups first.
- **Who notices:** machine-made configs; real configs nest groups a few levels deep.
- **Decided** (owner's precedent for the regular expressions, U-54: refuse at about half the
  lowest measured crash; coordinator, 2026-10-06): the port refuses a group that 590 groups
  hold, with "At line N, 'GroupTransform' parsing failed: GroupTransforms nested more than
  590 deep can't be loaded: upstream's stack overflows.", which a group that holds itself
  reaches too. The port loads and copies nested groups without recursion; printing,
  validating and dropping a group recurse in small frames, and at opt-level 0 they fit 2,360
  levels (four times the limit) on a 1 MiB thread; saving a group (`p3-yaml-save`) recurses
  too, as upstream's writer does, and fits 1,200 levels (twice the limit). Between 591 levels
  and the wheel's crash the port refuses what the wheel loads.
- **Status:** matched in `p3-yaml-load-1` (3.3h2, raised from 100 to 590 in a later chunk;
  `crates/ocio/src/ocio_yaml.rs`, the copy in `crates/ocio/src/transforms/group_transform.rs`).
  Tests: 590 levels load as in the wheel, the wheel loads 591 where the port refuses them, groups
  that hold themselves are refused, and loading, copying, printing, validating, building a CPU
  processor and dropping 590 levels fit a 1 MiB thread (also at opt-level 0).

### U-65. The inverse of a 3D LUT of one entry per side never finishes

- **Upstream:** the exact inverse renderer grows the LUT by one entry on each side, and its
  loops over the two ends of a side step by `dim - 1` (`InvLut3DRenderer::extrapolate3DArray`,
  `ops/lut3d/Lut3DOpCPU.cpp:1461-1591` @ v2.5.2). For a grid size of 1 that step is 0, and the
  first such loop never ends: creating the CPU processor of an inverse `Lut3DTransform` of grid
  size 1 never returns, with or without `OPTIMIZATION_LUT_INV_FAST` (the fast path builds its
  forward LUT through the same renderer, `MakeFastLut3DFromInverse`, `Lut3DOpData.cpp:29-58`),
  and neither do its GPU processor (whose shader bakes that fast forward LUT) nor
  `getOptimizedProcessor` with `OPTIMIZATION_LUT_INV_FAST`.
  Both wheels were still running after 30 s (probed 2026-10-07, under Python with a time
  limit); a grid size of 2 returns at once. The oracle can't run a case that never returns.
- **Who notices:** applications that invert a 3D LUT of one entry per side, on the CPU or the
  GPU.
- **Decided** (the owner's general rule, `docs/deviations.md`, as for U-5's endless loops):
  the port refuses it with "Lut3D: the exact inverse of a 3D LUT needs a grid size of at
  least 2.", when the renderer is made.
- **Status:** matched in `p2-lut3d-inv` (2.2d2), `InvLut3DRenderer::new`
  (`crates/ocio-ops/src/ops/lut3d/inv_lut3d.rs`, `GRID_SIZE_1_INVERSE`); `grid_size_1_is_refused`
  (`crates/ocio-ops/tests/lut3d_inv_oracle.rs`).

### U-70. ICC description strings longer than the file

- **Upstream:** SampleICC's `desc` and `mluc` readers allocate the string a tag claims before
  reading it (`mText.resize(textSize + 1)`, `std::vector<icUInt16Number> unicodeStr(nNumChar)`,
  `iccProfileReader.h:261-264, 366-369` @ OCIO v2.5.2): up to 4 GiB for a size the file can't
  hold. Where the allocation succeeds, the read then fails and so does the tag's reader; where
  it doesn't, `std::bad_alloc` escapes the reader and the load fails with its message, which
  depends on the machine's memory.
- **Who notices:** hostile or corrupt profiles.
- **A fix:** check the size against the tag's size first.
- **Decided** (P4-7, the general rule): the port never allocates what the tag claims. A
  string `Read8` is asked for with a positive count (a `desc` size below 2^31 bytes, an
  `mluc` string below 2^30 characters) is read in pieces as far as the file goes, and the
  tag's reader fails as it does in the wheel when the allocation succeeds. A larger one is
  read with a negative count, which reads nothing (I-165, I-166): the port allocates nothing
  and gives the reader's result per platform.
- **Status:** matched in `p4-readers` (4.7a, `read_bytes` in `crates/ocio/src/fileformats/icc/
  icc_profile_reader.rs`); the wheel's results for such profiles are compared in 4.7b.

### U-71. A Houdini `3D+1D` LUT without a prelut size

- **Upstream:** the Houdini reader takes a `Length` line of one or two sizes, and for a
  `3D+1D` LUT reads the second, `lut_sizes[1]` (`src/OpenColorIO/fileformats/
  FileFormatHDL.cpp:488-490` @ v2.5.2), whether the line gave it or not: with one size, past
  the end of the vector it read the sizes into.
  The wheel reads whatever follows the vector's storage: four runs of the same file
  (`Type 3D+1D`, `Length 2`) gave "Prelut size must be between 2 and 300000, found: 0" three
  times and "found: 3241057" once (Windows, the p4-readers verifier, 2026-10-09).
- **Who notices:** malformed files: a `3D+1D` LUT always has a prelut.
- **A fix:** require two sizes for `3D+1D`.
- **Decided** (the general rule): the port's error, "A 3D+1D Houdini LUT needs a prelut size
  on the 'Length' line, after the cube size (...)".
- **Status:** matched with an error in `p4-readers` (4.3b, `crates/ocio/src/fileformats/
  file_format_hdl.rs`).

### U-72. A cineSpace metadata block never closed

- **Upstream:** the `.csp` reader reads its metadata block with
  `while (!startswithU(line, "END METADATA") || !istream) { nextline(istream, line); ... }`
  (`src/OpenColorIO/fileformats/FileFormatCSP.cpp:403-413` @ v2.5.2). At the end of the file
  `nextline` returns an empty line, so a block without `END METADATA` never ends, and each turn
  appends a line feed to the metadata string. Reading such a file runs until that allocation
  fails or the system ends the process: the wheel's memory grew from 61 to 158 MB in 6 seconds
  on a file of the lines `CSPLUTV100`, `3D`, an empty line, `BEGIN METADATA` and `foo`
  (Windows, the p4-readers verifier, 2026-10-09).
- **Who notices:** applications that read a truncated or malformed `.csp` file.
- **A fix:** stop at the end of the file with an error.
- **Decided** (the owner's general rule for endless loops, as U-5 and U-65): the port's error
  "Metadata block of csp LUT <file> is not closed. Expected 'END METADATA' before the end of
  the file.", raised where `nextline` finds the end of the file inside the block: the wheel
  reads no file that gets there (it runs out of memory instead), and reads every other as
  before.
- **Status:** matched with an error in `p4-readers` (4.3e, `crates/ocio/src/fileformats/
  file_format_csp.rs`); `a_metadata_block_never_closed_is_an_error`
  (`file_format_csp_tests.rs`).

### U-75. A CDL or CCC file whose root element never starts

- **Upstream:** the CDL parser picks the schema from the first 5 KiB of the file: the first
  of "<ColorDecisionList", "<ColorCorrectionCollection" and "<ColorCorrection" found anywhere
  in it (`FindRootElement`, `fileformats/cdl/CDLParser.cpp:286-325` @ v2.5.2). A CDL or CCC
  binds its parsing info when its root element starts (`CDLParser.cpp:496-580`); when the
  text is found but the element never starts (in a comment, an attribute, a misspelled or
  other root), `validateParsing` dereferences the null parsing info (`CDLParser.cpp:337`),
  and the wheel crashes (Windows, probed 2026-10-08: `<!-- <ColorDecisionList --><Foo/>` as a
  .cdl, and as a .ccc with the collection's name, end the process on a file transform's
  processor). A CC creates its parsing info up front, but the schema is the first name found,
  whatever the extension: a .cc whose first 5 KiB mention "<ColorDecisionList" or
  "<ColorCorrectionCollection" (in a comment, say) before its "<ColorCorrection" root is
  parsed as a CDL or a CCC, and takes the same path.
- **Who notices:** malformed or hostile CDL, CCC and CC files.
- **Decided** (the owner's general rule): the port refuses the file with the parser's message
  "Error parsing <root> (<file>). Error is: CDL parsing error: the root element '<root>' is
  missing. At line (<n>)", as its `validateParsing` (and its transform getters) find no
  parsing info.
- **Status:** matched in `p4-cdl` (4.4d, `crates/ocio/src/fileformats/cdl/cdl_parser.rs`,
  `missing_root_error`; `a_cdl_or_ccc_without_its_root_element_is_refused`).

### U-76. Chains of CTF references long enough to overflow the stack

- **Upstream:** a CTF Reference builds its file through `BuildFileTransformOps`
  (`fileformats/FileFormatCTF.cpp:1218-1248` @ v2.5.2), which builds the referenced CTF file's
  ops, references included (`transforms/FileTransform.cpp:895-970`): the build recurses once
  per file of a chain. The recursion check stops cycles (a reference resolves to the same path
  each time, so a cycle repeats a path within one turn), but nothing bounds a legal chain, each
  file referencing the next. The wheel's `getProcessor` of a chain of CTF files crashes (the
  process ends) at these lengths, in Python's main thread (the verifier's `deep.py`,
  2026-10-16):

  | | Builds | Crashes |
  |---|---|---|
  | Windows | 2,100 files | 2,125 files |
  | Linux | 9,000 files | 10,000 files |

  The depth varies with the caller's stack.
- **Who notices:** a machine-made chain of hundreds of CTF files; nothing hand-written.
- **Decided** (the owner's general rule, refusing where the port's own stack still holds the
  chain; owner, 2026-10-17, after the coordinator's first limit of 1,000 of 2026-10-16): the
  port refuses a chain of more than 256 files (`MAX_FILE_NESTING`, U-58's limit) with "The CTF
  references nest more than 256 files deep: upstream's stack overflows." Between that and the
  wheel's crash the port refuses what the wheel builds. The port's frames per file (its
  loading, its errors, the reference's file transform and the LUTs' copies are kept out of
  line, and the recursive calls' results returned without `?`, as for U-58) decide what a
  thread holds; a 1 MiB thread on Windows (measured 2026-10-17, `group-5`):

  | Build | Builds | Overflows |
  |---|---|---|
  | opt-level 0 | 759 files | 766 files (about 1.4 KB a file) |
  | opt-level 1, the test profile | 1,647 files | 1,658 files (about 630 bytes a file) |

  So 256 files fit at every optimization level, with room for the caller's frames. The
  references recurse through the file transform alone, never through `BuildOps`, so the two
  limits count apart: U-58's 256 levels of nested transforms can only come before a chain of
  references on the stack, never between its files. That mixed chain (U-58's longest chain of
  color spaces, the last one's transform a file transform starting a chain of 256 files)
  builds on a 1 MiB thread at opt-level 0 in 641 KiB (U-58). The files also count in U-58's
  weighted total (`MAX_NESTING_WEIGHT`, 2026-10-17), 67 each, with the transforms and the
  groups before them: one budget bounds every mix (U-58's tables).
- **Status:** done in `p4-ctf-read-3` (`crates/ocio/src/transforms/file_transform.rs`,
  `FileNesting`, in `build_file_transform_ops`). Tests: `chains_of_references_stop_at_the_limit`
  (`file_transform_tests.rs`) builds a chain of 256 files on a 1 MiB thread, in debug and
  release, and refuses 257; `references_resolve_as_in_the_wheel`'s "the longest chain"
  (`crates/ocio/tests/file_format_ctf_oracle.rs`) checks the 256-file processor with the
  wheel; `a_chain_of_color_spaces_then_of_references_builds_on_a_1_mib_thread`
  (`crates/ocio/tests/config_processors_oracle.rs`, `group-5`) builds the mixed chain on a
  1 MiB thread and checks its processor with the wheel.

### U-77. Bakes whose sizes overflow an `int`

- **Upstream:** the bakes size their buffers with `int` products of the baker's sizes:
  `cubeData.resize(cubeSize*cubeSize*cubeSize*3)` in every 3D bake, and `onedSize * 3` and
  `shaperSize * 3` for the 1D LUTs and shapers (`FileFormatSpi3D.cpp:289`,
  `FileFormatSpi1D.cpp:350`, `FileFormatResolveCube.cpp:609, 625, 651`,
  `FileFormatHDL.cpp:680, 695, 720`, `FileFormatCSP.cpp:716, 732-733, 778-779`,
  `FileFormatCTF.cpp:1469, 1494` @ v2.5.2, and the cubes of the Iridas cube, itx, 3DL and
  Truelight bakes).
  From a cube size of 895 (`895^3 * 3 = 2,150,752,125`) or a 1D or shaper size of 715,827,883,
  the product passes `INT_MAX`: undefined behaviour. Both wheels wrap it to 32 bits (MSVC;
  GCC multiplies 32-bit registers and sign-extends the result, `imul`, `movsxd` at
  `libOpenColorIO.so` 0x2825df-0x2825eb in the Truelight bake), and `resize` takes the wrapped
  `int` as a `size_t`.
  - A negative result is past `max_size()`, and nothing is allocated or written. Windows:
    "vector too long" ("Error baking spi3d:vector too long" for 895 and 1291, probed
    2026-10-16). Linux: libstdc++'s "vector::_M_default_append" where `resize` checks the
    size (the Resolve cube, Houdini, Cinespace and CLF/CTF bakes, their shapers, and the
    Spi1D, Resolve, Houdini and CLF/CTF 1D LUTs; the CLF/CTF bake at `libOpenColorIO.so`
    0x245914-0x24592f and 0x245e08-0x245e14, its errors probed 2026-10-18), and
    "std::bad_alloc" where GCC removed the check (the Spi3D, Iridas cube and itx, Flame 3DL
    and Truelight cubes: their cube size is at least 2, so without overflow the product can't
    pass `max_size()`) and `operator new` is asked for the wrapped size times 4 bytes (probed
    2026-10-17).
  - A positive result (the cube size 1290: `6,440,067,000` wraps to `2,145,099,704`; the 1D
    size `2^32 / 3 + 1`: wraps to 2, and the buffer is resized to 2 floats) allocates fewer
    floats than the bake then writes: upstream writes past its buffer, on both platforms.
- **Who notices:** callers that set a cube, 1D or shaper size in the hundreds or millions.
- **Decided** (the owner's general rule, 2026-09-30, as U-5 and U-65; confirmed for this case
  2026-10-16): the port refuses every size whose product passes an `int` before it allocates.
  Where the wrapped size is negative it gives the wheel's error ("vector too long" on Windows;
  on Linux "vector::_M_default_append" or "std::bad_alloc", bake by bake as above); where it
  is positive, on both platforms, its own "The LUT's <n> values overflow the int that sizes
  its buffer.". Sizes that fit are allocated as upstream does, and a failed allocation is the
  C++ library's `std::bad_alloc` too, where Rust would abort.
- **Status:** matched with errors in `p4-baker` (`crates/ocio/src/baking_utils.rs`,
  `bake_buffer`, used by every bake); `overflowing_sizes_fail_as_in_the_wheel`
  (`crates/ocio/tests/bake_formats_oracle.rs`, both platforms), and `baking_utils_tests.rs`.
  The sizes one below the overflow (894, 715,827,882) allocate 8.6 GB in both wheels and the
  positive wraps write past the buffer, so no test runs them. The CLF/CTF bake
  (`p4-ctf-bake`) takes `LinuxResize::Checked` at both sites;
  `overflowing_sizes_fail_as_in_the_wheel` (`crates/ocio/tests/ctf_bake_oracle.rs`, both
  platforms) checks its cube, its cube after a shaper and its 1D LUT.

### U-78. An archive entry named `..` reads uninitialized memory when extracted

- **Upstream:** `ExtractOCIOZArchive` resolves each entry's name with minizip-ng's
  `mz_path_resolve` (`mz_os.c:129-200` @ 4.0.10, called by `mz_zip_reader_save_all`,
  `mz_zip_rw.c:818-907`) into a buffer it `realloc`s from entry to entry. A `..` at the start
  of a name makes it test the buffer's first byte (`mz_os_is_dir_separator(*target)`), which
  nothing may have written yet: on the first entry it is memory `malloc` left uninitialized.
  Where the name goes on after the `..` the byte's value doesn't matter. Where the name is
  only `..`s (`..`, `../`, `../..`, and on Windows `..\`), it decides whether the resolved
  name is empty or a separator, and the entry is saved at the destination with one slash or
  two after it, which extracts the same way: a directory entry (`../`, `..\`) makes the
  destination, and the extraction goes on (the wheels extract the entries after it, probed
  2026-10-17); a file entry (`..`, `../..`) can't be opened as a file, and both wheels fail
  ("Could not extract: <archive>").
- **Who notices:** applications extracting archives with an entry named `..`.
- **Decided** (the owner's general rule, 2026-09-30, as the outcome doesn't depend on the
  byte): the port's `mz_path_resolve` takes a byte nothing wrote for one that isn't a
  separator, and extracts as both wheels do; where an earlier entry wrote the byte, the port
  uses it, as `realloc` keeps it.
- **Status:** matched in `p4-baker` (`crates/ocio/src/minizip/mz_os.rs`);
  `crates/ocio/tests/ocioz_extract_oracle.rs`, the names `..`, `../`, `../..` and `..\` first
  in their archives.

### U-79. A directory entry whose name is longer than 255 UTF-8 bytes, on Windows

- **Upstream:** `Config::archive` lists the config's directory with minizip-ng's
  `mz_os_read_dir` (`OCIOZArchive.cpp:150-201` @ v2.5.2; minizip-ng `mz_os_win32.c:401-421` @
  4.0.10), which converts each name of the listing (`.` and `..` first) from UTF-16 into the
  same 256 bytes of `d_name` (`WideCharToMultiByte`); the byte after `d_name` is the listing's
  `end` flag (0, and 1 once the last entry is read; `DIR_int`, `mz_os_win32.c:30-35`), then
  the structure's padding. `malloc` left all of it uninitialized. A name longer than 255
  bytes doesn't fit with its NUL: the conversion writes the whole characters that fit in the
  256 bytes and fails, and the bytes after them stay as they were. OpenColorIO reads
  `d_name` as a C string: the cut, then what earlier entries wrote after it, up to a NUL, or
  up to `end`'s 0 where the 256 bytes hold none (I-222). Where that read reaches a byte of
  `d_name` no entry wrote, or, for the last entry, `end`'s 1 and the padding, the name read
  depends on memory nothing wrote (the Windows wheel left such files out, probed
  2026-10-17).
- **Who notices:** applications archiving configs next to files with very long names.
- **Decided** (the owner's general rule, 2026-09-30): the port reads each name as minizip-ng
  leaves it in `d_name`, earlier entries' bytes included (I-222); where the read reaches a
  byte nothing wrote, or the padding after the last entry, `Config::archive` fails with
  "Could not archive <path>: its name is longer than minizip-ng reads.".
- **Status:** matched in `p4-baker`, with an error where upstream reads memory nothing wrote
  (`crates/ocio/src/minizip/mz_os.rs`, `mz_os_list_dir`, which keeps `d_name` across the
  listing; `crates/ocio/src/ocioz_archive.rs`); `crates/ocio/tests/ocioz_archive_oracle.rs`,
  `long_names` (Windows: against the wheel where the name read is fixed, the port alone where
  it isn't).

### U-80. Archiving a config next to a dangling link whose name holds `*` overflows the stack

- **Upstream:** `Config::archive` adds each LUT file it lists with minizip-ng's
  `mz_zip_writer_add_path` (`OCIOZArchive.cpp:183-187` @ v2.5.2; `mz_zip_rw.c:1649-1729` @
  4.0.10). A path with a `*` anywhere (in a directory's name too) that doesn't exist is taken
  for a wildcard: the function lists the path's directory and adds each entry whose name
  matches the path's last component. On Linux a dangling link with a LUT extension in a
  directory whose name holds a `*`, or whose own name holds one, is such a path, and it
  matches itself, so the function calls itself on it again without end, each call holding a
  directory open, until the stack overflows: the Linux wheel crashes (SIGSEGV, probed
  2026-10-17). Where the process may open few files (a low `ulimit -n`), the listing fails
  first and the wheel throws "Could not write LUT file <path> to in-memory archive." instead.
  On Windows names can't hold `*`.
- **Who notices:** applications archiving a config whose directory holds such a link.
- **Decided** (the owner's general rule, 2026-09-30): the port's `add_path` returns an error
  for a path with a `*` anywhere that doesn't exist, and `Config::archive` fails with "Could
  not write LUT file <path> to in-memory archive." (the low-limit outcome).
- **Status:** matched with an error in `p4-baker` (`crates/ocio/src/minizip/mz_zip_rw.rs`,
  `add_path_of_file`); `crates/ocio/tests/ocioz_archive_oracle.rs`, `dangling_link_with_a_star`
  (Linux, the port only).

### U-81. Setting a grading value while another thread applies the processor

- **Upstream:** a grading op's dynamic property holds its value and the pre-render values
  computed from it, and the renderers read both at each `apply`; `setValue` writes them with no
  lock (`DynamicPropertyGradingPrimaryImpl::setValue`, `src/OpenColorIO/DynamicProperty.cpp:
  173-178` @ v2.5.2, and the RGB curve, tone and hue curve properties alike). An application
  that sets a value through `getDynamicProperty` while another thread applies the processor has
  a data race: the renderer can read a value half written, or pre-render values of one value
  and the rest of another.
- **Who notices:** applications that change a grading control while rendering on other
  threads, such as a UI slider over a playing image.
- **Decided** (owner decision P5-1, 2026-10-17): the port keeps each grading property's value
  and its pre-render values under one lock; a set validates, computes and swaps them together,
  and a renderer copies them once per `apply`. A single thread sees upstream's values exactly;
  a set during an apply gives the whole old value or the whole new one.
- **Status:** matched in `p5-values` (5.0a, `crates/ocio-ops/src/dynamic_property.rs`:
  `DynamicPropertyGradingPrimaryImpl`; the RGB curve property, 2.6b, already held its value and
  knots under a lock); `DynamicPropertyGradingToneImpl` in 5.0b;
  `DynamicPropertyGradingHueCurveImpl` in 5.0c.

### U-82. A drawn identity hue curve past nearly full knot arrays writes past their ends

- **Upstream:** with `drawCurveOnly` set, `computeKnotsAndCoefsForHueCurve` gives each identity
  curve two knots and three coefficients after those of the curves before it, with no check
  against the arrays' sizes (`GradingBSplineCurve.cpp:890-928` @ v2.5.2; 120 knots and 360
  coefficients for the eight curves, `GradingBSplineCurve.h:103-105`). The fitted curves check
  ("Hue curve: maximum number of control points reached."), the identities don't: when the
  curves before take 119 or 120 knots (a HUE_SAT curve of about 50 points with the six
  identities before HUE_FX), the identity's writes go past the end of `m_knotsArray` (and of
  `m_coefsArray` from 358 coefficients). The wheel then reports HUE_FX at knot offset 119,
  count 2 (probed 2026-10-20), having written into memory it doesn't own.
- **Who notices:** UIs drawing a hue curve of many control points.
- **Decided** (the owner's general rule, 2026-09-30): the port refuses the identity with the
  fitted curves' message before it changes the curve's offsets, so building the property or
  setting the value fails, as it does for a fitted curve that doesn't fit.
- **Status:** matched with an error in `p5-ec` (the fix of `p5-values` 5.0d,
  `crates/ocio-ops/src/ops/gradingrgbcurve/grading_b_spline_curve.rs`,
  `compute_knots_and_coefs_for_hue_curve`); `crates/ocio-ops/tests/hue_curve_knots_oracle.rs`,
  `draw_curve_only_identities_past_the_full_arrays_are_refused` (the port only).

### U-86. An enum instance no constructor constructed reads uninitialized memory

- **Upstream:** `OCIO.TransformDirection.__new__(OCIO.TransformDirection)` allocates an
  instance whose value pybind11 never initializes (no `__init__` or `__setstate__` runs), and
  its methods read it: through the wheel, `int(t)` gave 3 and `repr(t)` printed whatever the
  memory held (the verifier's probes of p6-core).
- **Who notices:** Python code that calls `__new__` of an enum directly, without `__init__`.
- **A fix:** none needed beyond the port's error.
- **Status:** the port raises RuntimeError "PyOpenColorIO.PyOpenColorIO.ViewType: the instance
  was never constructed (...)" from every method that reads the value, in `p6-core`
  (`crates/ocio-py/src/pybind11.rs`, `read_value`); `constructors_oracle.rs`,
  `undefined_behaviour_raises_the_ports_errors` (the port only).

### U-87. Constructing `pybind11_object` itself aborts the process

- **Upstream:** `pybind11_builtins.pybind11_object()` makes an instance of a type pybind11 never
  registered, and the wheel's process aborts. `OCIO.TRANSFORM_DIR_INVERSE.__reduce__()` does the
  same: copyreg's `_reduce_ex` calls the first base whose `__new__` is its own,
  `pybind11_object` (the verifier's probes of p6-core).
- **Who notices:** Python code that pickles an enum with protocol 0 or 1, or calls
  `__reduce__()` (protocol 2 and up, `copy` and `pickle`'s default, work).
- **A fix:** none needed beyond the port's error.
- **Status:** the port raises TypeError "pybind11_builtins.pybind11_object: pybind11's base
  class has no instances (the wheel's process aborts)", in `p6-core`
  (`crates/ocio-py/src/pybind11.rs`, `Pybind11Object::new`); `constructors_oracle.rs`,
  `undefined_behaviour_raises_the_ports_errors` (the port only).

### U-96. A sequence whose third `len()` raises is read with the exception set

- **Upstream:** pybind11's `std::array` caster calls the sequence's `len()` three times: to
  check the size, to reserve, and for the end of its index loop (`sequence::end()`, which
  doesn't check `PySequence_Size`'s error). When the third raises, it reads the items with
  that exception still set, which CPython doesn't allow: through the wheel,
  `MatrixTransform().setOffset(x)` raised `SystemError` "<method 'append' of 'list' objects>
  returned a result with an exception set" from whatever the item's `__getitem__` called
  first (the p6-core-2 probes).
- **Who notices:** Python code that passes a sequence whose `__len__` raises on its third call
  to a function taking a fixed-size array.
- **A fix:** none needed beyond the port's error.
- **Status:** the port raises RuntimeError "the sequence's third len() raised, and pybind11
  reads its items with that exception set (...)", the `len()` error as its cause, in
  `p6-core` (`crates/ocio-py/src/dispatch.rs`, the `[T; N]` caster); `dispatch_oracle.rs`,
  `arrays_refuse_what_the_wheel_reads_undefined` (the port only).

### U-97. A sequence whose third `len()` shrinks leaves an array partly unread

- **Upstream:** the same caster reads the items up to the third `len()` into a vector and then
  copies the array's `N` values from it (`vector_to_array`), whether or not it holds `N`: a
  third `len()` under `N` copies memory the vector reserved but never initialized (through
  the wheel, `setOffset` of a sequence whose `len()`s were 4, 4, 2 gave `[0.0, 1.0, 0.0,
  0.0]`, the last two whatever the memory held).
- **Who notices:** Python code that passes a sequence whose `__len__` changes between calls to
  a function taking a fixed-size array.
- **A fix:** none needed beyond the port's error.
- **Status:** the port raises RuntimeError "the sequence's third len() gave 2, and pybind11
  reads the 4 values of the array from memory it didn't initialize (...)", in `p6-core`
  (`crates/ocio-py/src/dispatch.rs`, the `[T; N]` caster); `dispatch_oracle.rs`,
  `arrays_refuse_what_the_wheel_reads_undefined` (the port only).

### U-98. pybind11's function record aborts the process when Python makes, initializes or fails to pickle one

- **Upstream:** the type of every function's `__self__`, pybind11's function record
  (`pybind11_builtins.pybind11_detail_function_record_v1_...`), has a `__new__` and an
  `__init__` that pybind11 never means Python to call: `type(OCIO.GetVersion.__self__)()`,
  `type(r).__new__(type(r))` and `r.__init__()` abort the wheel's process (exit 127 on
  Windows). Its `__reduce_ex__` raises nothing Python can catch either: when the module's
  `__module__` or `__name__` isn't a str (`"..." + 5`), or `eval` is gone from the builtins,
  the Python error leaves the C function as a C++ exception and the process aborts (the
  verifier's probes of p6-core, and p6-core-2's).
- **Who notices:** Python code that instantiates the type of a function's `__self__`, or
  pickles a module function after renaming its module to something that isn't a str.
- **A fix:** none needed beyond the port's error.
- **Status:** the port raises TypeError "<record type>: pybind11's function records are made
  (initialized) by pybind11 only (the wheel's process aborts; ...)", and "<record type>:
  pickling a function record raised an error, which aborts the wheel's process (...)" with
  the error as its cause, in `p6-core` (`crates/ocio-py/src/pybind11.rs`, `FunctionRecord`);
  `constructors_oracle.rs`, `undefined_behaviour_raises_the_ports_errors` (the port only).

### U-99. A keyword name that isn't UTF-8 in a refused call aborts the process

- **Upstream:** when no overload takes a call, pybind11's dispatcher writes each keyword's name
  into its error (`"{}=".format(name)` made a C++ string) outside the `try` that guards the
  arguments' reprs, so the exception a name with a lone surrogate raises leaves the C
  function: `OCIO.GetEnvVariable(**{chr(0xd800): 1})` aborts the wheel's process (exit 127
  on Windows; the verifier's probes of p6-core).
- **Who notices:** Python code that passes a keyword argument whose name holds a lone surrogate
  to a call no overload takes.
- **A fix:** none needed beyond the port's error.
- **Status:** the port raises TypeError "GetEnvVariable(): a keyword argument's name pybind11
  can't write in its error (the wheel's process aborts; ...)", the formatting error as its
  cause, in `p6-core` (`crates/ocio-py/src/dispatch.rs`, `incompatible`);
  `constructors_oracle.rs`, `undefined_behaviour_raises_the_ports_errors` (the port only).
