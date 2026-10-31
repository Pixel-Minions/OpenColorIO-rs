# Phase 8: SIMD speed

Goal (PLAN.md Phase 8): SIMD kernels for every numeric profile, SIMD pixel packing, and a speed
within 10% of C++ OCIO on the same machine. Parallel apply is a separate owner decision (below).
Bit parity with the wheel stays the acceptance rule: nothing here may change an output bit.

## How the speed is measured

`tools/perf-compare/perf_compare.py` and its Rust side `tools/perf-compare/src/main.rs` run the same cases
on both sides: one config (written by the wheel's own `serialize`), one image (ocioperf's
synthetic pattern, 1920x1080), one `apply(src, dst)` per iteration.

```sh
cargo build --release --manifest-path tools/perf-compare/Cargo.toml --target-dir target/perf-port
<venv python> tools/perf-compare/perf_compare.py --port-exe target/perf-port/release/perf-port.exe --out <dir>
<venv python> tools/perf-compare/perf_compare.py --port-exe ... --out <dir> --ocioperf --filter '^(matrix|lut1d)$'
```

- **The C++ side** is timed from Python, with `perf_counter_ns` around one `cpu.apply` call. Python
  costs one pybind11 call per image, not per pixel, and the GIL is released inside: the script
  measures that call on a 1x1 image (0.6 us against 1,000 us and more per image), and the image
  descriptions are built outside the timed region.
- **Cross-check against the plan's tool.** `--ocioperf` runs the wheel's `ocioperf.exe` itself on
  its 4K image against the port's copy of its four loops. Its in-place and two-buffer ratios agree
  with the table below (for example the matrix: 8.50 and 7.62 against 9.03; `lut3d_tetra_33`:
  5.06 and 4.94 against 5.09).
- **Noise.** Each process is pinned to one logical CPU, each case is timed 5 times in each of 2
  processes that alternate wheel and port, and the minimum is reported.
- **Same work.** The output digests of the two sides are compared. Every case below gave the same
  bits, so the ratios compare the same work.
- **Machine.** Ryzen 9 9950X3D on Windows 11. The wheel dispatches its AVX-512 kernels here, and
  so does the port. Times on another CPU or on Linux will differ.
- **Build.** The workspace's release profile. Fat LTO with one codegen unit changed nothing
  useful (`gt_log` 2.85 to 2.74, `lut1d` 7.07 to 8.04, `chain_aces2_srgb` 1.24 to 1.30), so the
  profile stays as it is.

## Where the port stands (baseline, bad41047)

Port time over wheel time (1.00 = same speed; the target is 1.10), RGBA F32 unless named. A row
that groups cases shows the first case's times and the range of ratios.

| Family | Case | Wheel ns/px | Port ns/px | Ratio |
|---|---|---:|---:|---:|
| Matrix | `matrix` (full) | 0.62 | 5.59 | 9.03 |
| | `matrix_diag` | 0.80 | 1.13 | 1.41 |
| Lut1D | `lut1d` (kernel) | 1.45 | 10.22 | 7.07 |
| | `lut1d/f32-u8` | 2.98 | 13.36 | 4.49 |
| | `lut1d_half`, `lut1d_inv` | 23.79 | 16.72 | 0.70 |
| GradingPrimary | `gp_log`, `gp_lin`, `gp_video` | 9.98 | 52.99 | 5.05-5.39 |
| | `gp_log_inv` | 10.01 | 69.81 | 6.98 |
| Lut3D | `lut3d_tetra_33` | 3.03 | 15.41 | 5.09 |
| | `lut3d_linear_33` | 7.80 | 28.00 | 3.59 |
| CDL | `cdl_no_clamp`, `cdl_asc` | 7.21 | 29.68 | 4.12-4.24 |
| FixedFunction | `ff_lin_to_pq` | 16.55 | 57.62 | 3.48 |
| | `ff_xyz_to_luv` | 4.15 | 9.06 | 2.18 |
| | `ff_rgb_to_hsy_log` | 2.22 | 3.24 | 1.46 |
| | `ff_aces2_sdr` | 265.71 | 342.48 | 1.29 |
| | red mod, dark to dim | 5.18 | 5.87 | 1.04-1.14 |
| | glow, gamut comp, RGB to HSV | 4.32 | 2.97 | 0.69-0.97 |
| GradingTone | `gt_log`, `gt_lin`, `gt_log_inv` | 136.75 | 389.77 | 2.25-2.85 |
| GradingHueCurve | `hc_log` | 27.57 | 61.93 | 2.25 |
| GradingRGBCurve | `rc_log`, `rc_lin`, `rc_log_inv` | 16.19 | 32.85 | 1.64-2.11 |
| Log | `log_affine`, `log_camera` and inverses | 1.90 | 3.87 | 1.85-2.41 |
| | `log_base2` | 1.45 | 1.61 | 1.12 |
| Gamma | moncurve, exponent, mirrors | 5.47 | 11.84 | 1.94-2.17 |
| ExposureContrast | `ec_linear`, `ec_video`, `ec_log` | 5.34 | 8.82 | 1.65-2.12 |
| Range | `range` | 1.72 | 1.20 | 0.69 |
| Bit depths, no op | `noop/u8-u8` (and 10, 12, 16 bit, F16) | 3.09 | 4.22 | 1.32-1.37 |
| | `noop/f32-u8` (and u10, u16) | 2.31 | 4.26 | 1.76-1.85 |
| | `noop/f16-f32` | 2.78 | 5.86 | 2.10 |
| | `noop/u8-f32` (and u10, u16), `noop/f32-f16` | 1.64 | 1.14 | 0.70-0.87 |
| A real view | `chain_aces2_srgb` (studio config, ACEScg to sRGB, ACES 2.0) | 270.48 | 335.88 | 1.24 |

The script's format rows for `matrix`, `lut1d` and `lut3d_tetra_33` (u8, u10, u12, u16 and F16,
each way) follow from the rows above: the op's own gap dominates (`matrix/u8-u8` 3.01,
`lut3d_tetra_33/u8-u8` 3.94), so those formats move with the op's chunk.

Two things ocioperf shows that the table doesn't:
- Pixel by pixel (`applyRGBA`), the port takes 11 ns/px on the matrix against 4.3 (2.61).
- Line by line, the matrix ratio stays near the image's, but `lut3d_tetra_33` went from 5.06 to
  8.57 in one run. To check in the Lut3D chunk: it may be a per-call cost or noise.

## Progress

Same machine and method, at 3afae8dc. Ratios are port time over wheel time.

| Case | Baseline | Now |
|---|---:|---:|
| `matrix`, two buffers | 9.03 | 1.36 |
| `matrix`, ocioperf in place / line by line | 8.50 / 6.79 | 1.05 / 1.03 |
| `matrix_diag` | 1.41 | 0.55 |
| `lut1d`, two buffers | 7.07 | 1.23 |
| `lut1d`, ocioperf in place / line by line | 7.11 / 6.70 | 1.13 / 1.06 |
| `lut1d/f32-u8` | 4.49 | 1.65 |
| `gp_log`, `gp_lin`, `gp_video` | 5.05-5.39 | 0.98-1.00 |
| `gp_log_inv` | 6.98 | 0.99 |
| `matrix/u8-u8` | 3.01 | 1.36 |
| `chain_aces2_srgb` | 1.24 | 1.22 |

What the kernels showed about the rest of the path:
- **Two buffers.** Upstream's first op reads the source and writes the destination. The port copies
  the row and renders in place, which costs about 0.25 ns/px (`matrix`: 1.05 in place, 1.36 and up
  with two buffers). It needs a `CpuOp` method that renders from one buffer to another, with a
  copy as its default: a new chunk, 12b below.
- **Pixel by pixel.** `gp_log` takes 47 ns per `apply_rgba` against 14.5 (3.26): the state and
  block set-up run per call. Chunk 14.
- **Lut3D line by line.** `lut3d_tetra_33` costs twice as much per row in ocioperf's line-by-line
  loop as in a whole image, again in the second run (10.07 against 4.91). So it isn't noise.
  Chunk 5 looks for what runs per call.

## Why the port is slow

Every "kernel" in the port is scalar code that reproduces one lane, including the Lut1D and Lut3D
SSE2, AVX, AVX2 and AVX-512 files. `crates/ocio-ops/src/{sse,sse2,avx,avx2,avx512}.rs` hold
one-lane helpers; their real intrinsics exist only to prove those helpers against the hardware.
That scalar code is exact, and it stays: see the next section.

The per-lane NaN helpers (`sse_add`, `sse_mul`) branch on every operand, and that is most of the
matrix's cost.

## How to get real SIMD and keep every bit

The hard part is NaN operand order. In C++ `_mm_add_ps(a, b)` becomes `addps`, which returns the
first operand's NaN. In Rust it is LLVM's `fadd <4 x float>`, which is commutative: LLVM may swap
the operands, so the intrinsic alone doesn't say which NaN comes out. The wheels' orders even
differ per platform and, on Windows, per loop (`matrix_op_cpu.rs`). Values other than NaN don't
depend on the order, and Rust never fuses or reassociates.

So each kernel has two paths:

1. **The exact path** is today's renderer, untouched: one lane at a time, every wheel's operand
   order, already proven against the oracle.
2. **The fast path** uses real intrinsics, for blocks of pixels where the order can't show.

A block goes to the fast path only if every value in it is finite and the renderer's coefficients
are finite (checked in its constructor). With a finite block and finite coefficients, the only
NaNs a kernel can make are x86's default NaN (`Inf * 0`, `Inf - Inf`), and two default NaNs are the
same bits in either order. Any block with a NaN or an infinity takes the exact path, so a kernel's
handling of the specials doesn't change at all.

Each family's chunk must show that its kernel can't turn a default NaN into another NaN before
two can meet (sign flips, `abs`, bit manipulation of a NaN); where it can, the chunk must say what
it does instead.

A real kernel that clears NaN first (Lut1D and Lut3D clamp their input, and NaN becomes 0) needs no
such check: no NaN reaches its arithmetic.

Other rules for the fast paths:
- `f32::mul_add` or `_mm*_fmadd_ps` only where upstream has an FMA intrinsic.
- `_mm_min_ps` and `_mm_max_ps` keep their operand order in Rust (they are LLVM x86 intrinsics, not
  `fadd`); Rust's `f32::min` doesn't apply.
- Pass-through channels are never written.

Tests, in each kernel's chunk:
- The op's oracle battery on the dispatched profile, as before. The battery's probes mix finite
  and non-finite values in one call, so both paths run. Its quick tier is the chunk's gate; the
  full tier runs per land.
- For the profiles this machine doesn't dispatch: an equality test of the SIMD kernel against its
  exact scalar lanes (PLAN.md, "equality tests against the scalar profile"), on a CPU that can run
  it, on top of the wheel comparison that SDE gives (`scripts/sde.sh`).
- A mutation list in the commit message: at least a constant, an operand order, a boundary, and
  the block check removed.

The gates can survive mutants: where a build's operand order happens to be the wheel's, or the
op's clamp removes the NaN, dropping a gate changes no bit on that platform. They stay, as LLVM
promises nothing about the order. Say so in the commit where it happens.

## Chunks, ranked by the measured gap

Stop adding kernels once a family is within 10%.

| # | Chunk | Ratio today | Upstream |
|---|---|---:|---|
| 1 | This card and the speed comparison | | |
| 2 | Matrix: SSE2 fast path, block check and load/store helpers | 9.03 | `MatrixOpCPU.cpp` |
| 3 | Lut1D: real SSE2, AVX, AVX2 and AVX-512 kernels and their RGBA packs | 7.07 | `Lut1DOpCPU_*.cpp`, `SSE2.h`, `AVX*.h` |
| 4 | GradingPrimary: SSE2 fast paths | 5.05-6.98 | `GradingPrimaryOpCPU.cpp` |
| 5 | Lut3D: real tetrahedral kernels, linear SSE2 path | 5.09, 3.59 | `Lut3DOpCPU*.cpp` |
| 6 | CDL: SSE2 fast paths, with `ssePower` | 4.24 | `CDLOpCPU.cpp`, `SSE.h` |
| 7 | FixedFunction: PQ, then LUV and HSY | 3.48, 2.18 | `FixedFunctionOpCPU.cpp` |
| 8 | GradingTone | 2.85 | `GradingToneOpCPU.cpp` |
| 9 | GradingHueCurve and GradingRGBCurve | 2.25 | `GradingHueCurveOpCPU.cpp`, `GradingRGBCurveOpCPU.cpp` |
| 10 | Log | 2.41 | `LogOpCPU.cpp` |
| 11 | Gamma | 2.17 | `GammaOpCPU.cpp` |
| 12 | ExposureContrast | 2.12 | `ExposureContrastOpCPU.cpp` |
| 12b | Two buffers: render from the source row to the destination row | 1.36, 1.23 | `ScanlineHelper.cpp`, `OpCPU` |
| 13 | Pixel packing and bit-depth casts | 1.32-2.10 | `ImagePacking.cpp`, `CPUProcessor.cpp`, `BitDepthUtils.h` |
| 14 | `apply_rgba` per pixel | 2.61 | `CPUProcessor.cpp` |
| 15 | ACES 2 (scalar C++ in both builds, so this is about compiled code, not SIMD) | 1.29 | `ACES2/Transform.cpp` |

## Parallel apply: an owner decision

PLAN.md lists it, and upstream's CPU apply isn't threaded. It needs a new API shape or a new
dependency (rayon), so nothing is added here without the owner's decision. The options are in
the Phase 8 report.
