# ACES2 GPU failures on llvmpipe: findings

Branch `claude/aces2-gpu-debug`, 2026-10-30. Investigation only: no test, threshold, waiver or
port code was changed.

Scope: four of the five finite failures of the unmerged fixed-function GPU card on
Mesa 25.2.7 llvmpipe (LLVM 21.1.8, 256 bits), Rocky Linux 9, GLSL 1.20. GamutComp13 inverse
was already explained and is not repeated here.

## Method and trust chain

- **Inputs.** All inputs are the CI captures in
  `kept/gpu-next/2026-10-29/ci-40950573272/release/<test>/`. They were copied into
  `debug/work/`, which is git-ignored. The exact compiled shader texts come from
  `.../debug/compiled/`.
- **GPU side.** `native/probe.cpp` is an EGL/GL runner with no OCIO and no Rust linked. It
  runs in `ocio-rs-rocky9:1` through `native/docker.sh`. Replaying the four original shaders
  reproduces all four captured GPU images bit for bit (`native/replay_original.sh`).
- **Instrumented shaders.**
  - Every instrumented variant first re-checks that its normal output still equals the
    captured GPU image bits. All variants used below pass this check.
  - Injected values arrive as runtime uniforms or textures, so nothing is constant-folded.
- **CPU side.**
  - CPU values come from the official `opencolorio==2.5.2` wheel: the Linux wheel inside the
    container, installed offline from the shared uv cache into `/tmp`, and the Windows wheel
    in `oracle/.venv`. Both gave identical bits at every point compared.
  - Stage values come from the wheel's own component FixedFunctions (`RGB_TO_JMH_20`,
    `GAMUT_COMPRESS_20`, `TONESCALE_COMPRESS_20`) applied to the optimized op list.
  - `native/cpu_rgb_to_jmh.cpp` is a literal transcription of upstream's CPU `RGB_to_Aab` and
    `Aab_to_JMh`, compiled with g++ and linked against glibc. Its intermediates are used only
    because its J, M and h bits equal the wheel's for every pixel checked: 4 nan_bug
    neighbours and all 27 rgb_to_jmh pixels.
- **CPU reference.** `attrib/cpu_layout.py` recomputes each captured `cpu.bin` exactly the way
  upstream does (`GPUUnitTest.cpp:391-422`: `getDefaultCPUProcessor`, an in-place
  `PackedImageDesc` of `min(n,256)` by `ceil(n/256)`). The result is identical bits for all
  four tests.
- **Builtin emulations.** `native/acc_lib.glsl` holds fp64 emulations rounded once to float.
  `native/check_acc.py` validated them against glibc double math: 0 ULP on 4096 random inputs
  each for pow, atan2, log, sin and cos.
- **llvmpipe builtin accuracy on the same samples:**
  - `pow`: up to 23 ULP (mean 2.2). At `pow(x, 0.42f)` with x in [0.01, 100], up to 4 ULP.
  - `atan(y,x)`: up to 336 ULP (mean 20.5).
  - `log`: up to 296 ULP.
  - `sin`/`cos`: up to 1 ULP.

## 1. style_aces2_nan_bug

**Root cause: the input is ill-conditioned for the CPU itself.**

### Where it sits

- The input is gray 0.89942779 (`0x3f6640e6`), PQ-encoded P3-D65. In the optimized
  processor it becomes linear `(38.85, 40.47, 43.67)`. The `[0,40]` range clamps G and B to
  40, giving `(38.849670, 40, 40)`.
- The inverse output transform (4000 nit, P3 limiting primaries, D60 white) sees this as
  compressed JMh `(510.2052, 4.0742, 195.73)`. That is J within 1.6 of `limit_J_max` 511.81.
- Its M lies about 0.1 % outside the approximated gamut boundary `gamutBoundaryM = 4.0706`.
  It sits on the pole of the inverse Reinhard remap: `remap_M<true>` with `nd ~ 1`, where the
  inverse is `scale * nd/(1-nd)` and the `nd >= 1` branch caps it.
  - CPU: `Transform.cpp:1007-1040` @ v2.5.2.
  - GPU: `ocio_remap_M_inv1`, `FixedFunctionOpGPU.cpp:1118`, captured `ocio.glsl:131-152`.

| (second `compressGamut` call) | `nd` | remapped M | final output |
|---|---|---|---|
| wheel CPU | 0.999713 (pole side: 3567.01 / 1.0214 = 1 / (1-nd) - 1) | 3567.0105 `0x455ef02b` | 974.2882, 568.0020, 5954.451 |
| llvmpipe GPU | 1.00022018 `0x3f800737` (cap branch) | 4.07439566 `0x40826173` | 275.40216, 814.3779, 963.6708 |

The CPU's huge M saturates `Ra_lim = 0.99` in `Aab_to_RGB`, and then the `[0, 5945.53]` range
clamps. That is why the CPU output is clamped garbage.

### First divergent stage, and the decisive one

1. **Lut1D (PQ decode), first but not decisive.**
   - The GPU gives R = 38.8497124 `0x421b661b`; the CPU gives 38.84967 `0x421b6610` (11 ULP).
   - The cause is the half-domain index arithmetic in `ocio_lut1d_0_computePos` plus linear
     texture filtering.
   - Injecting the CPU's LUT output leaves the GPU's JMh, and its final output, bit-identical
     to before. The difference is inconsequential.
2. **RGB to JMh, decisive.** With identical RGB input (CPU LUT value injected):
   - **lms**: identical bits on both sides (`41fb9172 41fcf3ab 41fde850`).
   - **`pow(abs(lms), 0.42)`**: the GPU gives `40882f39 40887fa3 4088b705`, while glibc and
     the double-rounded reference both give `40882f38 40887fa1 4088b704`. That is +1, +2 and
     +1 ULP. `rgb_a` inherits +1, +2 and +1 ULP.
   - **`Aab`**: the `a` row (15480 r - 16887.27 g + 1407.27 b) cancels terms of about 2100
     down to -3.92. The result is `a` = -3.92196655 on the GPU against -3.92149353 on the CPU,
     a relative difference of 1.2e-4.
   - **M**: 4.07462072 `0x4082634b` against 4.07415724 `0x40825f7f`, relative +1.1e-4. J
     differs by 4 ULP and h by 89 ULP.
3. **Substitution proof.**
   - Injecting only the CPU's `rgb_a` makes the GPU produce the wheel's exact final bits
     `44739272 440e0021 45ba139c`.
   - Injecting the CPU's compressed JMh does the same.
   - The gamut compression, tone scale and JMh-to-RGB code on the GPU is therefore not what
     separates the results. The input arrives on the other side of the pole.

### Ill-conditioned on the CPU itself

`nan_bug/sweep.py` and `nan_bug/cpu_neighbours.py` run the wheel on consecutive f32 inputs:

- The wheel's own output jumps between four families at **plus or minus 1 ULP** of the test
  input: 974/568/5954, 1810/5679/5978, 275/814/963 and 1224/2096/5961.
- Within inputs 0.89941686..0.89945340 (-183..+430 ULP around the test value), the CPU
  changes regime **101 times**. 302 of those 614 inputs land on the pole side.
- On llvmpipe, GPU against CPU passes upstream's 0.01 relative check on only 336 of 614
  inputs in that band. Across a wider window (0.8955..0.9033, 131072 inputs) 96.4 % pass.
- Upstream's commented-out neighbour 0.89944305 (`0x3f6641e6`) is in the band. llvmpipe
  passes there, giving 275.7114, 815.2339, 964.8170 against CPU 275.7026, 815.2327, 964.7594.
  Upstream's FIXME says their GPU failed there. Which side of the pole an implementation lands
  on is renderer luck.
- Even with every llvmpipe pow, atan, log, log2, sin and cos replaced by correctly rounded
  emulations, the test still fails (0.998). Changing `#version` or `GALLIUM_NOSSE` also does
  not help.
- Any GPU that is not bit-identical to the CPU in this chain can fail this input.

**Recommendation: (b).** No port or harness fix exists. The owner decides. The narrowest
exception is not a looser threshold. Pin the exact observed result for this one test, on this
renderer only:

- the test id `FixedFunction/style_aces2_nan_bug`;
- the renderer string;
- the GPU output bits `4389b37a 444b9830 4470eaee`;
- the wheel CPU bits `44739272 440e0021 45ba139c`.

The exception would keep the existing port-equals-wheel GPU bit check, so any change on
either side fails again.

## 2. style_aces2_4000nit_p3_rndtrip (0.018350264 against 0.018)

**Root cause: a near-zero channel's round-trip residual, amplified by the PQ encode.**

- The worst pixel is 4862, input `(0, 0.7897488, 0.90257)`, channel R.
- Before the final `[0,40]` range and PQ Lut1D, the CPU has R = -1.049e-5, which clamps to 0
  and encodes as PQ 7.3e-7. The GPU has R = +7.32e-5, which encodes as 0.01835.
- PQ's slope is unbounded at 0, so the test passes only if the GPU residual on a zero channel
  stays below about 7.05e-5 (in 0..40 units, about 1.8e-6 of peak). llvmpipe's 7.32e-5 is
  about 4 % over.

**First divergence.** The GPU is off right after the input PQ Lut1D: G by -1.08e-6 relative,
B by -2.19e-6 relative (half-domain index or filter precision). Every ACES2 stage then adds a
signed residual.

`attrib/attrib.py` measured each stage's contribution to R before the final range, by
injecting the wheel's stage value:

| Stage | Contribution to R |
|---|---|
| Lut1D decode | +6.8e-5 |
| RGB to JMh (inverse) | +0.8e-5 |
| Gamut inverse | -2.6e-5 |
| Tone scale inverse | -4.7e-5 |
| JMh to RGB (inverse) | +1.9e-5 |
| RGB to JMh (forward) | +5.6e-5 |
| Tone scale forward | -4.5e-5 |
| Gamut forward | +0.1e-5 |
| JMh to RGB (forward) | +4.8e-5 |
| **Sum** | **+8.4e-5** |

**Dominant builtin.** `native/knockout.py` replaces one builtin class at a time with its
correctly rounded emulation:

| Variant | Max error | Against 0.018 |
|---|---|---|
| Original | 0.0183503 | fail |
| `atan` only | 0.0109110 | pass |
| `pow` only | 0.0170756 | pass |
| sin/cos only | unchanged | fail |
| log/log2 only | unchanged | fail |
| All | 0.0100052 | pass |

`atan` (hue, about 20 ULP mean on llvmpipe) is the largest single contributor and `pow` the
second. The remaining 0.010 comes mostly from the GPU Lut1D index precision.

The other agent's `precise` result fits this picture: it perturbs the same residual. It is not
a fix.

**Recommendation: (b).** This is renderer precision. Upstream itself marks this limit "TODO:
Investigate why this is not closer". The narrowest exception is the same exact-pin form as
nan_bug, for this test on this renderer: max_diff `0.018350264` at index 19448.

## 3. style_aces2_rec709_rndtrip (0.004482085 against 0.004)

**Root cause:** the same mechanism, with gamma 2.4 in place of PQ.

- The worst pixel is 272, input `(0, 1, 0)`, channel B.
- The CPU has B = -3.13e-7 before the range, which encodes to 0. The GPU has B = +2.31e-6,
  which encodes as `x^(1/2.4)` = 0.004482.
- The budget is `0.004^2.4`, about 1.76e-6 (about 15 ULP of 1.0). llvmpipe's residual is
  2.62e-6, spread over all ACES2 stages in steps of about 1e-6 (table in
  `work/attrib_709.txt`).

**Builtin knock-out:**

| Variant | Max error | Against 0.004 |
|---|---|---|
| Original | 0.00448208 | fail |
| `atan` only | 0.00353245 | pass |
| `pow` only | 0.00413853 | fail |
| All | 0.00293260 | pass |

`atan` is the dominant builtin.

**Recommendation: (b),** with an exact pin as above (max_diff `0.004482085` at index 1090).

## 4. style_aces2_rgb_to_jmh_fwd (0.00022888184 against 0.0002)

**Root cause:** `pow(lms, 0.42)` ULP error, amplified by the Aab cancellation, plus llvmpipe
`atan`.

- The worst pixel is 11, input `(0.18476, 0.20398, 0.31311)`, channel h (low chroma,
  M = 11.84).
- lms is bit-identical on GPU and CPU for all 27 pixels. GPU `rgb_a` is off by up to
  3 ULP (pixel 11: 0, +1, +1 ULP). Aab differs by up to 4.2e-5 relative in `a`.
- At low chroma the hue absorbs this: 249.10678 on the GPU against 249.10701 on the CPU.

| Variant | Max error | Against 0.0002 |
|---|---|---|
| Original | 2.29e-4 | fail |
| Wheel `rgb_a` injected | 9.16e-5 | pass |
| Wheel Aab injected | 9.16e-5 | pass |
| `pow` knocked out | 9.16e-5 | pass |
| `atan` knocked out | 1.53e-4 | pass |
| Both | 3.05e-5 | pass |

`pow` is the dominant builtin at the worst pixel, contributing about 1.4e-4 of 2.3e-4. `atan`
contributes up to about 9e-5.

**Recommendation: (b),** with an exact pin (max_diff `0.00022888184` at index 46).

## 5. Harness against upstream

No difference changes these results. Checked:

- **GLSL language.** Upstream uses `GPU_LANGUAGE_GLSL_1_2` (`GPUUnitTest.cpp:664-669`). Ours
  is the same: shader cache id `glsl_1.2`.
- **Processors.** Upstream uses `getDefaultGPUProcessor` (`GPUUnitTest.cpp:339-347`) and
  `getDefaultCPUProcessor` (`:391`). Ours matches: `gpu_unit_test.rs:328-335` and the oracle's
  `cpu_apply` without optimization (`oracle/ocio_oracle/commands.py:188-193`).
- **CPU image layout.** Upstream applies in place on `min(n,256)` by `ceil(n/256)`
  (`GPUUnitTest.cpp:396-422`). Our oracle applies source to destination on one row of
  `entries/4` pixels (`commands.py:196-203`). That is a formal difference only: re-running
  upstream's exact layout on the Linux wheel reproduces every captured `cpu.bin` bit for bit
  (`attrib/cpu_layout.py`). The compared rectangle, including the padded last row and alpha,
  matches `comparison_entries` (`gpu_unit_test.rs:271-277`).
- **Input texture.** Upstream uses RGBA32F, `GL_LINEAR`, `GL_CLAMP`, and a 256 by 256 ortho
  quad (`oglapp.cpp:65-80, 130-145`). Ours matches (`oglapp.rs:395-407`). LINEAR and NEAREST
  give identical finite results at texel centres; the replay uses NEAREST and matches.
- **LUT and table textures.** Upstream uses R32F or RGB32F, a filter from the interpolation,
  and `CLAMP_TO_EDGE` (`glsl.cpp:60-140`). Our captured settings match, and the uploads read
  back exactly.
- **Not applicable:** `setTestNaN`, `setTestInfinity` and wide range do not apply to custom
  values. Color clamping is disabled on both sides (`oglapp.cpp:257-259`,
  `oglapp.rs:268-270`).
- **Renderer controls.** `GALLIUM_NOSSE=1` (no AVX or FMA) changes the images but all four
  still fail. `LP_NATIVE_VECTOR_WIDTH=128` changes nothing (`native/controls.py`).

## Still unproven

- No hardware GPU was available. "Upstream's GPUs pass" is inferred from the thresholds and
  their TODO comments, not measured.
- For nan_bug, the exact upstream-GPU behaviour at 0.89942779 is unknown. The sweep shows only
  that the CPU flips within plus or minus 1 ULP.
- The round-trip stage table is a first-order attribution. The contributions are signed and
  interact, so "dominant" rests on the builtin knock-out, not on the table.

## Files

- `native/probe.cpp`: the GL runner. `native/nonfinite.cpp`: helpers copied from
  `origin/codex/p21-gpu-fixed-functions`.
- `native/docker.sh`: the container wrapper.
- `native/replay_original.sh`: replays the four original shaders.
- `native/cpu_rgb_to_jmh.cpp`: the CPU replica, verified against the wheel.
- `native/acc_lib.glsl`, `native/check_acc.py`, `native/knockout.py`: builtin emulation,
  validation and knock-out.
- `native/controls.py`: renderer-control experiments.
- `nan_bug/cpu_stages.py`, `nan_bug/cpu_neighbours.py`, `nan_bug/sweep.py`: wheel stage
  values and conditioning.
- `nan_bug/instrument.py`, `nan_bug/run_gpu.sh`: GPU intermediates and injection.
- `attrib/attrib.py`: stage attribution for the round trips.
- `attrib/cpu_layout.py`: the upstream-layout CPU reference check.
- `rgb_to_jmh/probe.py`: the rgb_to_jmh decomposition.
- `work/`: captures and raw outputs. It is git-ignored.
