# Architecture: how upstream's structure maps onto Rust

Binding for all porting cards. Where upstream and this note disagree, upstream's *behavior*
wins and this note is updated.

## Crates and layering

```
ocio-ops  ←  ocio-formats
    ↑    ←  ocio-gpu
    └──────  ocio  ←  ocio-py, ocio-tools
```

| Crate | Holds (upstream files) |
|---|---|
| `ocio-ops` | The bottom layer and CPU engine: `utils/` (StringUtils, NumberUtils), `cfmt` (C/iostream formatting), `exception`, `platform`, `hash_utils`, `logging`, `bit_depth_utils`, `math_utils`, `sse`/`sse2`/`avx*` (fast math, SIMD helpers), `cpu_info`, image descriptions and packing (`ImageDesc`, `ImagePacking`, `ScanlineHelper`), `FormatMetadataImpl`, dynamic properties, `op`, `op_data`, every `ops/<family>/` op data plus CPU renderers, `OpOptimizers`, the CPU processor engine |
| `ocio-formats` | `fileformats/**`: readers produce op data; writers consume it |
| `ocio-gpu` | `GpuShader*`, `GPUProcessor`, `GpuShaderUtils`, and every `ops/<family>/*GPU.cpp` shader writer |
| `ocio` | The public API: `transforms/**` (the transform classes, `BuildXxxOp` and `CreateXxxTransform`), `Config` and YAML I/O, `Context`, `Processor`/`CPUProcessor`/`GPUProcessor` wrappers, built-ins, `apphelpers/**`, the baker, `.ocioz` |

Upstream's `ops/<family>/<Family>Op.cpp` files mix op code with transform glue
(`BuildXxxOp`, `CreateXxxTransform`). The op part goes to `ocio-ops`; the transform glue goes
to `ocio` (`crates/ocio/src/transforms/<family>_glue.rs` or next to the transform). List both
targets in `upstream-map.toml`.

## Op data and ops

- `OpData` is an enum with one variant per `OpData::Type`:
  `Cdl, Exponent, ExposureContrast, FixedFunction, Gamma, GradingPrimary, GradingRgbCurve, GradingHueCurve, GradingTone, Log, Lut1D, Lut3D, Matrix, Range, Reference, NoOp`.
  Each variant's struct is a port of `<Family>OpData`, including its `FormatMetadataImpl`
  (id, name, descriptions) and its `validate`, `isNoOp`, `isIdentity`, `getCacheID`,
  `equals` and `hasChannelCrosstalk`. Matches over `OpData` are exhaustive: no wildcard arms.
- `Op` wraps `Arc<OpData>`. Upstream's `Op::clone()` is a deep copy; ours is `Arc::make_mut`
  (copy on write). `finalize()` mutates through `make_mut`, so shared data is never changed
  behind another op's back. This fixes upstream's `Lut1DOpData::finalize` sharing bug
  without changing outputs.
- The virtual `Op` methods (`isSameType`, `isInverse`, `canCombineWith`, `combineWith`,
  `getCacheID`, `getInfo`, `finalize`, the dynamic-property accessors and `getCPUOp`) become
  methods on `Op` that match on the variant and call the family's module.
- `OpRcPtrVec` becomes `OpVec`: a `Vec<Op>` plus `FormatMetadataImpl`, with the same methods
  (`finalize`, `optimize`, `optimizeForBitdepth`, `getCacheID`, `invert`, `validate`, ...).
- GPU: `ocio-gpu` has `extract_gpu_shader_info(op: &Op, creator)`, matching on the variant.
  It is the port of each op's `extractGpuShaderInfo`.

## CPU renderers and numeric profiles

- `OpCPU` becomes `trait CpuOp: Send + Sync`. Renderers are chosen when the CPU processor is
  built, exactly where upstream chooses (`GetXxxRenderer`, `getCPUOp(fastLogExpPow)`), and
  are held as `Arc<dyn CpuOp>`.
- Most renderers work in place on RGBA f32 scanlines (`&mut [f32]`). Renderers that
  upstream uses at the ends of the chain with other bit depths (Lut1D, the generic
  bit-depth helpers) take typed input and output buffers (`u8`, `u16`, `f16`, `f32`) through
  a small enum. No `unsafe` casts.
- **Numeric profiles.** Where upstream has SIMD kernels, each C++ kernel is its own
  renderer type (e.g. `Lut3DTetrahedralSse2`, `...Avx2`, `...Avx512`). Each first
  reproduces the kernel's per-lane arithmetic as exact scalar code (`f32::mul_add` only
  where the C++ uses FMA intrinsics). Real SIMD comes in Phase 8, with equality tests
  against the scalar profile.
- **Dispatch** is a port of upstream's: compile-time `OCIO_USE_*` settings as the official
  wheel was built (constants in `cpu_info.rs`), plus runtime `CPUInfo` flags and quirks.
- **Forced profiles.** For tests, the flags can be overridden (`cpu_info::with_flags(...)`,
  test-only). That is how upstream reruns its suite per SIMD mode. Live oracle checks
  always use the real flags, because the wheel does.

## Processor pipeline

The port follows upstream's order exactly: build ops → validate → finalize → optimize
(`OpOptimizers.cpp`, up to 80 passes, the same pass order) → `optimizeForBitdepth` → CPU
engine (`CreateCPUEngine`: the first and last ops absorb the bit-depth conversion, and Lut1D
is special-cased) → `ScanlineHelper` (packing and unpacking per scanline, with a scratch
buffer of `m_width` pixels).

## Image descriptions

`ImageDesc`, `PackedImageDesc` and `PlanarImageDesc` are public API in `ocio-ops`
(`image_desc.rs`), re-exported by `ocio`. The owner approved this design on 2026-09-30.
- **Memory.** A description borrows its memory as bytes: typed slices (`&[T]` or `&mut [T]`,
  `T` one of `u8`, `u16`, `half::f16`, `f32`, and `&Vec<T>`) are viewed as bytes without
  copying, through `zerocopy`; `Bytes(..)` takes raw bytes of any bit depth; `At(data, offset)`
  puts the first pixel `offset` bytes into the memory, as a C++ pointer into an array does.
  `PackedImageDesc<B>` is generic over the byte borrow: `&[u8]` describes a source, `&mut [u8]` an
  image the CPU processor can write (`ImageDescMut`).
- **Upstream's layouts.** Strides are `isize` bytes, `AUTO_STRIDE` (`isize::MIN`) or explicit,
  negative ones included. A channel's position is a byte offset in its buffer (`ChannelPos`)
  instead of a pointer. Offsets and strides use upstream's integer arithmetic, and wrap where C++
  overflows. Values are read and written with `from_ne_bytes`/`to_ne_bytes`, so any stride works
  for any buffer.
- **Checks.** The constructors make upstream's checks in upstream's order, with its messages.
  Before them, a typed slice must hold the bit depth's channel type (the Python binding's
  `checkBufferType` and its message). After them, the bounds check of deviation D-2
  (`docs/deviations.md`) refuses a layout that would make the CPU engine touch a byte outside its
  memory: for an RGBA-packed image, whole rows of `4 * width` channels; for any other, each
  channel at `start + x * x_stride + y * y_stride`.
- **Sizes** are `usize` in the API and C++'s `long` (`c_long`) inside, as the CPU engine computes
  with them. A size beyond `long` (Windows) becomes an invalid size, which upstream's checks
  refuse.
- **No `unsafe`.** The crate stays `#![deny(unsafe_code)]` outside the SIMD modules. `ocio-py`
  keeps a description's layout (`ImageLayout`, plain data) with the NumPy buffers, and borrows the
  buffers only while `apply` runs.

## Strings are bytes

OCIO's strings are C byte strings (`std::string`, `const char *`). They are usually UTF-8, but
not always. The S1 review showed this through the wheel:
- a config loaded from a Latin-1 file keeps and re-serializes its raw bytes;
- Python `bytes` arguments are accepted as they are;
- yaml-cpp's lenient decoding turns an overlong `C0 80` into a NUL, and `serialize()` and the
  config cache ID are truncated at that NUL.

So OCIO string data (names, descriptions, families, categories, paths, environment values,
metadata) is stored as bytes with no interior NUL, never as Rust `String`. The emitter and
hashing take bytes. Public Rust getters and setters work on bytes, with `&str` conveniences.
The exact API shape is decided with the owner in Phase 3, where the config model is built.
Python follows pybind11: `str` arguments become UTF-8 bytes, and `bytes` arguments pass through
unchanged.

## Errors, logging and environment

- `ocio_ops::Exception` holds upstream's message verbatim and whether it is
  `ExceptionMissingFile`. Every fallible port function returns `ocio_ops::Result`.
- Logging is a port of `Logging.cpp`: the same prefixes (`[OpenColorIO Warning]: `), the
  same line splitting, and the same level rules (`OCIO_LOGGING_LEVEL` read once). The
  callback is called *outside* the global lock (upstream calls it under the lock and can
  deadlock): deviation D-3, which changes nothing that one thread logs.
- The environment is read through `ocio_ops::platform::getenv`, which uses an injectable
  provider. Tests never mutate the process environment.

## Unsafe

`#![forbid(unsafe_code)]` everywhere except:
- `ocio-ops` SIMD modules (`sse*.rs`, `avx*.rs`, `*_sse*.rs`, `*_avx*.rs`, `*_f16c*.rs`, `simd/`) and `cpu_info.rs`;
- `ocio-py`;
- `ocio-testkit/src/crt.rs` (FFI to the C runtime as a test reference).

`cargo xtask guards` enforces this.
