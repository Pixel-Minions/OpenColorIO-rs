# OpenColorIO-rs: rules for agents

An exact Rust port of OpenColorIO **2.5.2**, built by agents and accepted only on
byte-by-byte parity with the official `opencolorio==2.5.2` wheel. `PLAN.md` is the source of
truth: read §3 (definition of done), §7 (how agents work) and your card before writing code.

## The rules

1. **Port, don't reinvent.**
   - Read the upstream code in `upstream/OpenColorIO` (submodule pinned at `v2.5.2`) and translate it.
   - Every ported function's doc comment cites its source: `` Port of `Foo::bar` (src/OpenColorIO/Foo.cpp:120-188 @ v2.5.2). ``
   - Don't guess behavior, simplify it, or "fix" it. Upstream bugs that change outputs are ported too.
2. **Expected values come only from the oracle, upstream's tests, or the platform itself.**
   - The oracle is the real OCIO 2.5.2 (`ocio_testkit::Oracle`, `fixtures/`).
   - Upstream tests are copied verbatim, with a citation. This includes the own tests of a library the wheel is built with, at the version it uses (yaml-cpp 0.8.0, Imath 3.2.1).
   - The platform C runtime is the reference for C and C++ runtime behavior (`ocio-testkit` `crt.rs`), and the CPU for instruction semantics.
   - Never use the port's own output as an expected value. Never type a number into a test that didn't come from one of those places.
3. **Comparisons are exact.**
   - Floats compare bitwise (`assert_f32_bits_eq`, `assert_pixels_bits_eq`); text compares byte for byte (`assert_text_eq`).
   - The only tolerances allowed are upstream's own, in ported upstream tests, through `ocio_testkit::upstream` helpers.
4. **Never weaken a check.**
   - No `#[ignore]`, `todo!()` or `unimplemented!()`, and no loosened comparison.
   - Never edit `fixtures/`, `oracle/uv.lock`, `waivers.toml` or `docs/ratchet.toml` to get green.
   - If you can't explain a mismatch, stop. Write a minimal reproduction (an oracle probe plus a failing test), leave it failing, and report it.
5. **Match upstream's arithmetic exactly.** See "Bit-exact porting" below.
6. **Stay in scope.** Change only the files your card names. Other agents are working in parallel.
7. **Work in small, mergeable chunks.** See "Chunks" below.
   - Commit on your own branch only.
   - Never push, rewrite earlier commits, touch other branches, or change git config.
   - The orchestrator reviews and merges chunk by chunk.

## Chunks

Every card lands as a series of chunks. Each chunk is one commit that can be reviewed and merged on its own.

- **One coherent unit.** A chunk is usually one upstream file or module, with the tests that cover it. Aim for under ~600 lines of non-test code. Tests travel with the code they test, never in a later chunk.
- **Green on its own.** Before committing, all of these must be clean on Windows:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets` (0 warnings)
  - `cargo xtask ci`
  - `cargo test --workspace`

  Chunks with platform-sensitive behavior (numerics, formatting, parsing) must also pass `scripts/rocky9.sh cargo test -p <crate>`. Numeric chunks must pass their oracle tests in release builds too (`cargo test --release -p <crate>`, on both platforms). Optimization can change NaN results and other bits the debug build doesn't show.
- **Bookkeeping travels with the port.** Update `upstream-map.toml`, `docs/parity.md` and `docs/ratchet.toml` in the same chunk as the port they describe.
- **Oracle changes stand alone.** Changes to `oracle/` and new fixture groups get their own chunk, before the chunk that first uses them. The owner reviews them separately.
- **Order and fixes.** Chunks are ordered by dependency. A later fix is a new chunk; never rewrite an earlier commit.
- **Checking a chunk in isolation** while other work is in progress:
  1. `git add <files>`
  2. `git stash push --keep-index --include-untracked`
  3. Run the checks.
  4. `git commit`
  5. `git stash pop`
- **Commit messages.** The first line is `<card>: <what>`. The body lists the upstream files and line ranges ported, the tests, and the evidence (which checks ran and on which platforms). End with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Bit-exact porting

- **Precision and order.**
  - Keep every f32/f64 boundary and the operation order exactly as in C++, including overload promotions: `std::pow(float, float)` is `powf`, but `std::pow(float, double)` computes in double.
  - Keep casts where C++ has them: `static_cast<float>(double_expr)` becomes `as f32` at the same place.
- **FMA.** Rust never fuses multiply-adds. Use `f32::mul_add` only where upstream calls an FMA intrinsic (`Lut1DOpCPU_AVX2/AVX512`, `Lut3DOpCPU_AVX2/AVX512`), or where a spike proved that a compiler fused the operation in the wheel.
- **min, max and clamp.**
  - C++ `std::min(a, b)` is `(b < a) ? b : a`, and `std::max(a, b)` is `(a < b) ? b : a`.
  - SSE `_mm_min_ps(a, b)` is `a < b ? a : b`, and `_mm_max_ps(a, b)` is `a > b ? a : b`.
  - They treat NaN differently from each other and from Rust's `f32::min`/`f32::max`. Use the helpers that match the C++ you are porting.
- **NaN operand order.** When both operands of one operation can be NaN, the result's bits depend on operand order.
  - x86 `addps`/`mulps`/`addss`/`mulss` return the first operand's NaN, quieted. So SSE code follows the C++ source order.
  - C++ compilers may commute scalar code (MSVC and GCC differ; check the wheel's machine code).
  - LLVM freely commutes Rust's `+` and `*`, and the order can change between debug and release builds.
  - Use `math_utils::sse_add`/`sse_mul` wherever two NaNs can meet, in the operand order the wheel's *machine code* uses, per platform where MSVC and GCC differ. That is usually, but not always, upstream's source order: both wheels compile `m_linsinv[i] * (in[i] + m_minuslino[i])` (`LogOpCPU.cpp:787`) as `(in + minuslino) * linsinv`. Check the disassembly when finite parameters can create a NaN coefficient. NaN parameters are covered by waiver W0002.
- **Channels that pass through.** Never copy a channel the C++ passes through unchanged (`out[3] = in[3]`) in code that computes the other channels.
  - In optimized builds, LLVM's SLP vectorizer may compute the copy as identity arithmetic (`a - 0.0`, `a * 1.0`) in a vector lane next to the computed channels. That quiets a signaling NaN, which the C++ copy keeps. It happened in the Lut3D SSE2 and AVX kernels with rustc 1.98.1, and only in release builds.
  - Renderers work in place (`CpuOp::apply`) and never write a channel they pass through.
  - Tests check pass-through channels on every numeric profile, not only the one this machine dispatches to.
- **Rounding.**
  - Scalar integer conversions add 0.5 and truncate (`BitDepthUtils.h`).
  - SIMD stores round to nearest-even.
  - Port whichever the upstream path uses.
- **Math library.**
  - Use the same function the C++ calls: `logf`/`log2f`/`expf`/`powf` on f32, or `log`/`pow` on f64, through Rust `std`. Rust `std` calls the same CRT or glibc routine as C++ on that platform.
  - Compilers can fold constant math calls at compile time with a different implementation. When upstream computes a constant with a math function, verify it against the oracle.
- **Fast math.** OCIO's default optimization uses its own approximations (`SSE.h`: `sseLog2`, `sseExp2`, `ssePower`, ...), not libm. Port them operation for operation.
- **SIMD.**
  - Scalar code comes first. Each profile (SSE2, AVX, AVX2, AVX-512) is its own renderer and replicates the C++ kernel's arithmetic.
  - `unsafe` is allowed only in SIMD modules and `ocio-py`; `cargo xtask guards` enforces this.
- **Platform differences.** Where OCIO gives different bytes on Windows and on Linux, the port must too (PLAN.md D12). Check both platforms.
- **Reading the wheel's machine code.** `docs/wheel-inspect.md` shows, in a few commands of `tools/wheel-inspect`, which operand order either wheel compiled, where it computes in float or double, and whether a path calls libm, SVML or an FMA.

## The oracle

- `ocio_testkit::Oracle::get().call(cmd, json_args, &[blobs])` runs a command of `oracle/ocio_oracle/` in the pinned wheel on this machine.
- Responses are cached under `<target>/oracle-cache`. The cache key covers the oracle sources, the lock file and this machine's CPU.
- Pixel checks always run live, because the kernel choice and the math library belong to the machine. Never commit pixel data.
- Text that is identical on every platform is committed with `cargo xtask oracle regen <group>` (groups live in `oracle/ocio_oracle/regen.py`). Tests read it with `ocio_testkit::fixtures::read_text`.
- **New oracle commands.**
  - Add them in your own module under `oracle/ocio_oracle/`, registered in `commands.py`.
  - A command reports what the library does and never computes expected values.
  - The owner reviews every oracle change.
- **Both reference platforms:**
  - Windows: `cargo test ...`
  - Rocky Linux 9: `scripts/rocky9.sh cargo test ...`. It uses the `docker/rocky9` image, with the checkout mounted at `/work`.
- **Emulated CPUs.** This machine and GitHub's runners only run the SIMD kernels their own CPUs select. `scripts/sde.sh <cpu> <cmd>` runs a command on a CPU emulated by Intel SDE, and the wheel and the port then both pick that CPU's kernels.
  - Tests whose results depend on the CPU (SIMD kernels, CPUInfo, libm calls) belong to a test target in the `cpu-tests` alias (`.cargo/config.toml`). Add new targets of that kind there.
  - CI runs `cargo cpu-tests --release` on `nhm` (SSE2 kernels), `snb` (AVX), `hsw` (AVX2 with slow gather), `skl` (AVX2) and `skx` (AVX-512), on both platforms.
  - A chunk that adds or changes a SIMD kernel runs it locally too, on the CPUs whose kernel it touches: `scripts/sde.sh snb cargo cpu-tests --release`, and `scripts/rocky9.sh scripts/sde.sh snb cargo cpu-tests --release`.

## Oracle tests: use `ocio_testkit::battery`

- **Every op family's renderers are checked through the battery.** Implement `battery::Family` (parameter cases, the oracle's `Spec`, the port's `Port`) and call `battery::run` in a test. The how-to is at the top of `crates/ocio-testkit/src/battery.rs`; `crates/ocio-ops/tests/log_oracle.rs` is a complete example.
- **What it runs.** Every case in both directions, with fast math on and off, on the probe sets of the tier that `OCIO_RS_TIER` names:
  - `quick` (default, per chunk): sampled, seconds per family;
  - `full` (per land): all halves, the specials, 1.2 million random values, every generated case;
  - `exhaustive` (nightly): more random values, and every `f32` bit pattern.

  It sends its oracle calls in batches, a handful of processes per test. Hand-written oracle checks outside the battery batch their calls too (`Oracle::batch`).
- **Generated cases.** Give the battery a typical case per code path (`mutation_bases`), and it generates extreme finite, NaN and ±Inf parameters. Until the op data's `validate` is ported, declare `Validation::NotPorted`: generated cases the wheel refuses are then left out and listed.
- **W0002 only through the battery.** It applies automatically to the channels of NaN parameters (the scope the owner approved), and a case may narrow it but not widen it. Infinite and extreme finite parameters compare bit for bit. Nothing else may use the W0002 comparison; a test enforces this.
- **Pass-through channels.** Declare the channels your renderers never write (`pass_through`), and hand the battery the renderers of numeric profiles this machine doesn't dispatch (`other_profiles`). Their pass-through channels are then compared with the wheel on every profile; other profiles without pass-through channels are an error.

## Layout and conventions

- **Where code goes.**
  - Files mirror upstream: `src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp` becomes `crates/ocio-ops/src/ops/lut1d/lut1d_op_cpu.rs`.
  - `upstream-map.toml` records each upstream file's target and status (`todo | partial | done | n/a`). Update it as you port.
- **Ported upstream tests.**
  - A `tests/cpu/.../Foo_tests.cpp` file becomes `foo_tests.rs` next to `foo.rs`, included from `foo.rs` with `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;`.
  - Each ported test carries a marker directly above `#[test]`: ``/// Port of `OCIO_ADD_TEST(Group, name)` @ v2.5.2.``
  - `cargo xtask parity` and the ratchet count these markers.
- **Headers.** Every `.rs` file starts with `// SPDX-License-Identifier: BSD-3-Clause` and `// Copyright Contributors to the OpenColorIO Project.`
- **Dependencies.** Pin them exactly in the root `Cargo.toml` `[workspace.dependencies]` (`=x.y.z`); crates use `workspace = true`. New dependencies need a reason in your report.
- **Errors.** Upstream exception text is part of the output, so copy it verbatim.
- **Non-ASCII data.** Write it as escapes in source (`\u{feff}` in Rust, `\ufeff` in Python), never as raw characters. The file-editing tools can turn a `\uXXXX` typed in their input into the raw, often invisible, character, and `sed` treats `\u` in a replacement as "uppercase the next letter". Check such files with a byte dump (`od -c`) before committing.
- **Before reporting done:** `cargo fmt --all`, `cargo clippy --workspace --all-targets` (no warnings), `cargo xtask ci`, and `cargo test --workspace`, on Windows and in Rocky Linux 9.

## Commands

| Command | Purpose |
|---|---|
| `cargo xtask ci` | Guards, fixture hashes, ratchet, parity dashboard freshness |
| `cargo xtask guards` | Forbidden patterns, the `unsafe` allowlist, headers, dependency pins, the upstream map |
| `cargo xtask parity` | Regenerate `docs/parity.md` after porting tests |
| `cargo xtask ratchet --update` | Record the new number of ported upstream tests |
| `cargo xtask upstream-tests` | Every upstream test and whether it is ported |
| `cargo xtask oracle info` | The oracle's versions and this machine's CPU features |
| `cargo xtask oracle regen <group>` | Regenerate committed fixtures (owner-reviewed) |
| `cargo xtask oracle check-all` | Prove committed fixtures are identical on this platform |
| `scripts/rocky9.sh <cmd>` | Run a command on the Linux reference platform |
| `cargo cpu-tests` | The tests whose results depend on the CPU |
| `scripts/sde.sh <cpu> <cmd>` | Run a command on an emulated CPU (`nhm`, `snb`, `hsw`, `skl`, `skx`, ...) |
