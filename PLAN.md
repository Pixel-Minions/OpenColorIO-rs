# OpenColorIO-rs — Porting Plan

**Status:** v0.6, 2026-09-29. Phase 0 is in progress (§15).
- **Changes in v0.6:** D13 decided (`PyOpenColorIO`); the public repository is fine for now; work lands as small mergeable chunks (§7, `CLAUDE.md`).
- **Changes in v0.5:** Rust 1.98.1 pinned (D7); exact upstream test counts; the Phase 0 progress and findings; repository visibility added to the open questions.
- **Changes in v0.4:**
  - our own version numbers, each stating the OCIO version it matches;
  - a Python module compatible with PyOpenColorIO is back in scope, because users will script Ultravioleta in Python;
  - planned for 2–3 agents at a time;
  - Rocky Linux 9 as the Linux reference.
- **Changes in v0.3:** skip the audit of the existing port; keep the crate private; Windows and Linux only; match OCIO on each platform.
- **Changes in v0.2:** pinned to OCIO 2.5.2; a byte-exact definition of done; an operating model for agents; Ultravioleta as the first consumer.

**Target:** OpenColorIO **2.5.2**, exactly. This is the version Ultravioleta pins (`requirements-color.txt`). Later OCIO versions become new major versions of this crate (§2).

**Sources:**
- the upstream `v2.5.2` tag and `main` (source, tests, release notes, CI);
- the Rust ecosystem, with crate versions as of 2026-09-29;
- Ultravioleta's OCIO usage.

**Contents:**
1. [Summary](#1-summary)
2. [Target and versioning](#2-target-and-versioning)
3. [Definition of done: byte by byte](#3-definition-of-done-byte-by-byte)
4. [What we are porting](#4-what-we-are-porting-ocio-252)
5. [Decisions](#5-decisions)
6. [First consumer: Ultravioleta](#6-first-consumer-ultravioleta)
7. [How agents do the work](#7-how-agents-do-the-work)
8. [Verification](#8-verification)
9. [Architecture](#9-architecture)
10. [Roadmap](#10-roadmap)
11. [Effort and calendar](#11-effort-and-calendar)
12. [Porting later OCIO versions](#12-porting-later-ocio-versions)
13. [Risks](#13-risks)
14. [Open questions](#14-open-questions)
15. [Next steps](#15-next-steps)
16. [Appendices](#appendix-a--environment-variables)

---

## 1. Summary

- **What.** A Rust port of OCIO 2.5.2 that behaves exactly like it: the same results, text output, errors and accepted configs, from Rust and from Python. Coding agents build it. A release is accepted only when it passes byte-by-byte checks against the real OCIO 2.5.2, plus upstream's own C++, GPU and Python tests.
- **Versioning.**
  - We use our own semantic version, with the OCIO version it matches attached: `1.0.0+ocio.2.5.2`.
  - Tags are ours (`v1.0.0`).
  - `ocio::version()` still reports `2.5.2`, exactly as OCIO does.
  - Porting a later OCIO version is a diff from one upstream tag to the next, released as a new major version (§12).
- **First consumer: Ultravioleta.**
  - The port replaces its Python worker (`scripts/ocio_worker.py`). That removes the Python runtime from the color path and the bridge costing 612 ms per UHD frame. Every op becomes byte-exact; today only six ops run natively, within 3e-5 of OCIO.
  - Users' Python scripts get a module compatible with `PyOpenColorIO`, backed by the same Rust code, so scripts and the app always agree.
- **Size of OCIO 2.5.2:**
  - 96K lines of C++ in the core library, plus an 11K-line Python binding;
  - 66 public classes (~1,060 methods);
  - 23 transform types and 14 op families;
  - 24 file formats;
  - 98 built-in transforms and 8 built-in configs;
  - 10 GPU shading targets.
  - Upstream's tests: 1,191 C++ unit tests, 264 GPU tests, 384 Python tests (counted by `cargo xtask upstream-tests`).
- **How.**
  - A clean port from source into idiomatic Rust, one module at a time, mirroring upstream's layout.
  - Every module is checked against an *oracle*: the official `opencolorio==2.5.2` wheel, the same build Ultravioleta uses today.
  - Upstream's Python suite runs unmodified against our Python module.
  - Agents never write expected values, and CI enforces that (§7).
- **Byte-exact is achievable.**
  - OCIO 2.5.2 uses no hardware-approximate instructions.
  - FMA appears only in the AVX2/AVX-512 LUT kernels, which can be reproduced exactly.
  - MSVC's SVML math library is only used for PQ with fast math off.
  - On Windows and on Linux (Rocky 9), pixels will be bit-identical to the wheel on the same machine. Text is identical everywhere (§3).
- **Effort.** About 133 person-weeks of conventional work. With 2–3 agents at a time:
  - Ultravioleta runs natively on the built-in configs in about 3–4 months;
  - the Python worker is gone in about 4–6 months;
  - full 2.5.2 parity (`1.0.0`) arrives in about 6–10 months.

  Confidence is low until Phase 1 measures real velocity (§11).

### Milestones

Every version below carries `+ocio.2.5.2`.

| Milestone | What works | Version |
|---|---|---|
| **M0** | `ocio://default`: config queries, CPU color-space and display/view conversions, all byte-exact | `0.1.0` |
| **M1** | **Ultravioleta runs natively on the 8 built-in configs.** All six worker operations are byte-exact: `info`, `convert`, `display`, `gpu_shader`, `cpu_ops`, `gpu_convert` | `0.2.0` |
| **M2** | **Ultravioleta drops the Python worker.** Studio configs with LUT files, file rules, context variables, and CDL/look/file transforms all work | `0.3.0` |
| **M3** | Compositor features: dynamic properties (exposure/gamma uniforms in the viewer), grading ops, color space menus, mixing | `0.4.0` |
| **M4** | **Python scripting:** the `PyOpenColorIO`-compatible module is embedded in Ultravioleta, and upstream's Python suite passes | `0.5.0` |
| **M5** | **Full OCIO 2.5.2 parity:** every applicable upstream test, all GPU targets, config merging, SIMD performance | `1.0.0` |

---

## 2. Target and versioning

**One OCIO release per major version of this crate, ported exactly.**
- Behavior, results, accepted config versions, error text and API surface all match that release. That includes its bugs, unless a deviation is approved and listed in `docs/deviations.md`.
- Features added upstream after 2.5.2 are not in 1.x. Two examples from 2.6: `ACES_RGB_TO_HMJ_20` and the Apple Log / Apple Wide Gamut built-in.

| Item | Scheme | Example |
|---|---|---|
| Our version | Our own semver: `0.x` while porting; `1.0.0` is the complete port of OCIO 2.5.2 | `0.2.0`, `1.0.0` |
| Matched OCIO version | Build metadata on every version, so `cargo tree` and `Cargo.lock` show it | `1.0.0+ocio.2.5.2` |
| Git tags | Our version. Each release note starts with the OCIO version it matches | `v1.0.0`: "matches OCIO 2.5.2" |
| Version bumps | A new OCIO minor is a major bump; a new OCIO patch is a minor bump; port-only fixes are a patch bump; Rust-only additions are a minor bump | OCIO 2.6.0 → `2.0.0+ocio.2.6.0`; a fix → `1.0.1+ocio.2.5.2` |
| Maintenance branches | One per matched OCIO minor | `ocio-2.5` carries 1.x; `main` moves on to 2.6 |
| Upstream pin | Submodule at the matched tag; oracle wheel of the same version | `upstream/OpenColorIO` @ `v2.5.2`; `opencolorio==2.5.2` |
| Runtime version | `ocio::version()` and Python's `__version__` report OCIO's version, as OCIO does; `ocio::PORT_VERSION` reports ours | `"2.5.2"` and `"1.0.0"` |
| Accepted configs | Exactly the matched release's | `ocio_profile_version` ≤ 2.5; a 2.6 config fails with 2.5.2's error |
| Distribution | A private git dependency | Ultravioleta pins `tag = "v1.0.0"` |

If you ever publish to crates.io, this scheme works unchanged:
- every release has its own number;
- a new OCIO minor is always a major bump, so Cargo's default `^` requirement never crosses one.

---

## 3. Definition of done: byte by byte

A release (e.g. `1.0.0+ocio.2.5.2`) ships only when all five conditions hold.

**1. Upstream's C++ and GPU tests pass.** Every test in upstream's v2.5.2 C++ suite (1,191) and GPU suite (264) is ported and passes, or is on a reviewed not-applicable list. That list covers tests of C++-only mechanics, such as `shared_ptr` identity.

**2. Upstream's Python tests pass.** Upstream's Python suite (384 tests) passes **unmodified** against our Python module.

**3. Output is byte-exact against the oracle.**

| Output | Must be identical |
|---|---|
| Config YAML (`serialize`); CLF/CTF/CDL/CC/CCC files; baked LUT files; shader text and uniform layout; cache IDs; error and warning text; Python `repr` strings; menu and helper results; optimized op lists; `createGroupTransform` contents | Byte for byte, on every platform |
| CPU pixels, at all 6 bit depths, all optimization levels, and for every op; GPU texture data; LUT values computed while baking or composing | Bit for bit against the wheel on the same OS and CPU. The reference platforms are Windows x86-64 and Linux x86-64 (Rocky Linux 9) |

**4. No unapproved waivers.** Every exception is listed in `docs/waivers.md` with its root cause and your sign-off. CI fails on any mismatch that isn't listed there.

**5. Ultravioleta's replay corpus passes.** The corpus of captured real requests (§6) replays byte-identically.

### Why "same OS and CPU"

C++ OCIO 2.5.2 itself gives different bits on different machines:
- it picks SIMD kernels by CPU (SSE2, or AVX2/AVX-512 with FMA);
- it calls the platform's math library;
- ARM compilers fuse multiply-adds.

So with OCIO today, a frame rendered on a Windows workstation and the same frame rendered on a Linux farm node can already differ in the last bit, wherever OCIO calls the math library per pixel. The port does exactly what OCIO does (D12):
- where OCIO gives the same bytes on both platforms, so does the port;
- where OCIO differs, the port differs in the same way.

On Linux, OCIO calls glibc's math functions at runtime. So the Linux reference runner uses Rocky Linux 9 (glibc 2.34), the farm's target. That is also the VFX Reference Platform CY2027 Linux baseline, so the future 2.6 line lines up with the platform completely.

### How we get bit-exact results

These are the rules the agents port by.
- **Port each CPU kernel's arithmetic exactly:**
  - the same operation and summation order;
  - the same f32/f64 boundaries, including C++ overload promotions (`std::pow(float, double)` computes in double);
  - the NaN behavior of C++ `std::min`/`max` and SSE `min`/`max`;
  - the same rounding: add 0.5 and truncate in scalar casts, round to nearest-even in SIMD stores.
- **Kernel variants are numeric profiles.**
  - Replicate OCIO's CPU dispatch (`CPUInfo`) exactly, including its exceptions for CPUs where AVX is slow, gathers are slow, or AVX-512 is blocked. Then the same variant runs as in C++ on the same machine.
  - Implement each profile as exact scalar code first. `f32::mul_add` reproduces the AVX2/AVX-512 FMA results exactly.
  - Add SIMD later, with equality tests against the scalar profile.
- **Port OCIO's fast approximations bit-exactly:** `sseLog2`, `sseExp2`, `ssePower`, `sseAtan2` and `sseSinCos`. The default optimization level uses them.
- **Transcendentals use the platform's math library.** Rust's `std` calls the same CRT or glibc functions as C++ does on that platform.
- **What was checked in 2.5.2:**
  - no hardware-approximate instructions (`rcp`, `rsqrt`);
  - FMA only in `Lut1DOpCPU_AVX2/AVX512` and `Lut3DOpCPU_AVX2/AVX512`;
  - MSVC's SVML `_mm_pow_ps` only in PQ with fast math off (`FixedFunctionOpCPU.cpp:2133-2194`). Spike S5 decides whether to call the same MSVC routine or grant a waiver.

---

## 4. What we are porting (OCIO 2.5.2)

Line counts are non-blank, non-comment lines at `v2.5.2`. Difficulty runs from 1 (mechanical) to 5 (subtle).

| Subsystem | C++ lines | Difficulty | Notes |
|---|---:|:---:|---|
| **Config model:** color spaces, roles, displays/views, view transforms, looks, named transforms, context, env vars, search paths, file and viewing rules, validation, ConfigUtils | 11.2K | 3–4 | `Config.cpp` alone is ~6K raw lines. It has ~550 throw/log sites, and tests pin their text |
| **YAML config I/O** | 4.8K | 4 | Verbatim `!<Tag>` tags; a byte-exact writer (yaml-cpp `%.7g`/`%.15g`); v1 and v2 formats; profile-version gates 2.1–2.5 |
| **Processor pipeline:** op model, optimizer, caches, pixel packing, baker, `.ocioz`, logging | 6.9K | 4 | An 80-pass optimizer; tests pin XXH3-128 cache IDs |
| **Transforms** (23 types) and op builders | 5.9K | 2–3 | — |
| **Ops** (14 families), with CPU renderers and GPU writers | 33.6K | 2–5 | Hardest: Lut1D (half-domain, inversion), ACES 2.0, Lut3D inversion, grading curves |
| **GPU shader infrastructure** | 3.0K | 3 | 10 targets, including `GLSL_VK_4_6`, which Ultravioleta uses |
| **SIMD** (SSE2/AVX/AVX2/AVX-512/F16C; NEON via sse2neon) | ~6.6K raw | 4 | Chooses kernels at runtime, with CPU-specific exceptions |
| **File formats:** 19 readers covering 24 formats; 12 can bake, 5 can write | 18.9K | 1–5 | CLF/CTF has a 6.5K-line reader and a 3.4K-line writer (raw lines) |
| **Built-ins:** 98 transforms and 8 configs | 3.2K + 377 KB YAML | 2 | Upstream has expected values for all 98 transforms |
| **App helpers:** menus, legacy viewing pipeline, mixing, display/view helpers | 2.5K | 2 | Ultravioleta uses `LegacyViewingPipeline` today |
| **Config merging** (a preview feature in 2.5) | 4.3K | 4 | — |
| **Python binding** (pybind11) | ~10.8K raw | 3 | 1,414 `.def` calls. Our module must match its API and behavior |
| **Tests** | C++ 70K · GPU 5.2K · Python 13.8K raw | — | 1,191 / 264 / 384 tests; 261 data files (34 MB) |

**Out of scope:**
- the Java binding and the vendor plugins;
- upstream's Python apps;
- the Sphinx docs site;
- a C ABI, unless something later needs one.

---

## 5. Decisions

| # | Decision | Choice | Status |
|---|---|---|---|
| **D1** | Porting strategy | A clean port from source, checked against the oracle. Swapping Rust in piece by piece inside the C++ library (as the fish shell did) doesn't fit: OCIO's internals aren't exposed through its API, `autocxx` is archived, and `c2rust` only handles C | Proposed |
| **D2** | API surface | A Rust API plus a Python module compatible with PyOpenColorIO 2.5.2. The C ABI is deferred | Decided |
| **D3** | Target | OCIO 2.5.2 exactly; later OCIO versions become new major versions | Decided |
| **D4** | Equivalence | Byte-exact as defined in §3; waivers only with your sign-off | Decided |
| **D5** | Existing port (`doubleailes/ocio-rs`) | Skip the audit for now and port independently. Note that it is MIT-licensed although derived from BSD-3 code | Decided (skipped for now) |
| **D6** | Distribution | Not published to crates.io; consumed as a git dependency. The GitHub repository is public, which is fine for now | Decided |
| **D7** | Toolchain | Rust 1.98.1 pinned in `rust-toolchain.toml`, the same as Ultravioleta; edition 2024; `rust-version = "1.98"` | Decided (Phase 0) |
| **D8** | `unsafe` policy | `#![forbid(unsafe_code)]` everywhere except the SIMD and Python-binding modules | Proposed |
| **D9** | License | BSD-3-Clause, keeping upstream's notice ("Copyright Contributors to the OpenColorIO Project") | Required (the port is a derivative work) |
| **D10** | Versioning | Our own semver; the matched OCIO version goes in build metadata (`1.0.0+ocio.2.5.2`) and in the release notes; tags `v1.0.0` (§2) | Decided |
| **D11** | Platforms | Windows x86-64 and Linux x86-64 (Rocky Linux 9, glibc 2.34), both bit-exact reference platforms from Phase 0. ARM and macOS come later | Decided |
| **D12** | Output across platforms | Do what OCIO does on each platform: where OCIO is byte-identical across Windows and Linux, so is the port; where OCIO differs, the port differs the same way | Decided |
| **D13** | Python module | Import name `PyOpenColorIO`, so existing studio scripts run unchanged. It is built into Ultravioleta's embedded interpreter and shares the app's config and caches. A private standalone wheel serves pipeline scripts and CI. It can't share a Python environment with the official wheel, so the oracle gets its own | Decided |

---

## 6. First consumer: Ultravioleta

### How Ultravioleta uses OCIO today

Based on `scripts/ocio_worker.py`, `src/color*.rs` and `planning/color-management.md`.

- **A Python worker.** `ColorManager` spawns a Python worker running `opencolorio==2.5.2`. They talk in length-prefixed JSON plus a binary payload, with six operations: `info`, `convert`, `display`, `gpu_shader`, `cpu_ops` and `gpu_convert`.
- **About 60 OCIO calls, in five groups:**
  - **Config:** file and built-in configs, `validate`; color spaces (name, aliases, encoding, `isData`); the `scene_linear` role; displays and views; the default display and view; looks and view looks.
  - **Processors:** `DisplayViewTransform`, color-space pairs, and `LegacyViewingPipeline` with a looks override.
  - **CPU:** the default CPU processor, and `applyRGBA` on f32 RGBA.
  - **Introspection:** `getOptimizedProcessor` + `createGroupTransform`, and the getters of Matrix, Exponent, ExponentWithLinear, LogCamera, Range and Lut1D.
  - **GPU:** `GLSL_VK_4_6` with 1D textures disallowed, descriptor set 1/1, a function name and a resource prefix; shader text; 2D/3D textures. Uniforms are refused today.
- **A native path.** `color_ops.rs` runs those six ops natively, taking ~28 ms per UHD frame instead of 612 ms through the bridge. It stays within 3e-5 of OCIO, but it isn't byte-exact.
- **Fallbacks.** Everything else goes through the bridge: 3D LUTs, CDLs, most fixed functions, and ADX's `LogTransform`.

### What the port changes

- **No worker.** One in-process call replaces it, with no Python runtime to find for color processing. Your plan item C5 ("a portable bridge") becomes unnecessary.
- **`color_ops.rs` becomes redundant.** The port's `CpuProcessor` is byte-exact for every op. It must also be at least as fast: rayon over strips, then SIMD.
- **The viewer is unchanged.** Shader text is byte-identical, so `gpu_viewer.rs` keeps working. Dynamic properties later unlock uniforms (C7).
- **Your color-management plan maps directly onto port APIs:**

| Your plan item | Port API |
|---|---|
| C1: canonical names and aliases | Config name resolution |
| C2: file rules and `interop_id` | `color_space_from_filepath`, `ColorSpace::interop_id` |
| C4: `$OCIO`, `ocio://`, built-in configs | `Config::from_env`, `Config::from_uri` |
| C6: project variables | `Context` |
| C7: viewer dynamic properties | Dynamic properties |
| C8: transform nodes | `Transform` |
| C9: color space menus | `ColorSpaceMenuHelper` |

### Python scripting

- **Registration.** Ultravioleta embeds Python 3.13 and registers our module as `PyOpenColorIO` before the interpreter starts. So `import PyOpenColorIO as OCIO` finds it ahead of any installed copy.
- **Shared state.** Scripts and the app share the same Rust objects. `OCIO.GetCurrentConfig()` is the project's config, and a script's processor gives the same bytes as the app's.
- **Performance.** `applyRGBA` works on NumPy arrays in place and releases the GIL while it processes.
- **Compatibility.** Existing PyOpenColorIO scripts (for example, from Nuke) run unchanged, within OCIO 2.5.2's API.

### Cut-over path

This is work in Ultravioleta. The first three steps can start before the port is complete.
1. Put `ColorManager` behind a backend trait with two implementations: `PythonWorker` and `Native`.
2. **Capture mode:** record real worker requests and responses (config, operation, payload) into a corpus. OpenColorIO-rs replays that corpus in CI as byte-exact regression tests.
3. **Shadow mode**, in dev builds: run both backends and diff the bytes. Each mismatch becomes a new replay case.
4. Switch operations to `Native` as each reaches parity, keeping the worker as a fallback. This is the same native/bridge split you have today.
5. Remove the worker at M2. Scripting arrives at M4.

---

## 7. How agents do the work

### Principles

- **Port, don't reinvent.**
  - Every Rust function translates a named upstream function, and its doc comment cites `file:line @ v2.5.2`.
  - Every Python binding follows upstream's pybind11 definition, with the same names, keyword arguments and defaults.
  - The module tree mirrors upstream's source tree, recorded in `upstream-map.toml`. That keeps upstream diffs mapping one-to-one onto our code, which is what makes later versions cheap (§12).
- **The oracle decides.** Expected values come only from:
  - upstream test sources, ported verbatim;
  - fixtures generated by `oracle/` from the pinned wheel.

  Agents never use their own output as the expected value, and never "accept" snapshots.
- **Small, mergeable chunks.** There are about 75 porting cards (§10). Each card lands as a series of chunks: one commit or PR each, usually one upstream file or module with its tests, under ~600 lines of non-test code, and green on its own (fmt, clippy, `cargo xtask ci`, tests; plus Rocky Linux 9 for platform-sensitive code). Oracle changes are separate chunks. The rules are in `CLAUDE.md`.

### Guardrails enforced by CI

These exist so that no one can pass by weakening a check.
- **Fixtures are locked.** Only `xtask oracle regen` can regenerate them, from the pinned wheel. A manifest records each fixture's hash, oracle version and CPU flags, and CI verifies it.
- **You own the checks.** CODEOWNERS assigns `fixtures/`, `oracle/`, `waivers.toml` and the vendored upstream Python tests to you, so agent PRs can't change them.
- **Exact by default.** Comparisons are exact unless a waiver in `waivers.toml` says otherwise, and every waiver needs your approval.
- **No hidden gaps.** The main branch may not contain `#[ignore]`, `todo!()` or `unimplemented!()` without a tracked waiver.
- **Coverage only goes up.** The count of ported upstream tests can only increase. Every upstream test and source file is either mapped or marked not-applicable with a reason.
- **Tests must catch changes.** `cargo-mutants` runs on the core modules to show that tests fail when the code is deliberately broken.
- **Standard checks:** clippy, fmt, `cargo-deny`, and `forbid(unsafe_code)` outside the SIMD and binding modules.

### Team shape: 2–3 agents at a time

| Agent | Role |
|---|---|
| Implementer A | The critical path: ops → config → Ultravioleta's calls |
| Implementer B | Independent branches of the graph as they open up: file formats, GPU writers, the Python module |
| Verifier C | Reviews every PR adversarially: reads the upstream code against the diff, hunts for divergences the tests miss, and adds oracle probes for them |

- With only 2 agents, run one implementer plus the verifier.
- Each agent works in its own git worktree and commits its card as a series of mergeable chunks. The orchestrator merges them chunk by chunk, gated by CI.
- **Your review is narrow:** public API shape, waivers and deviations only.
- **Stuck on a mismatch.** An agent that can't explain a byte mismatch writes a minimal reproduction (an oracle script plus a failing test) and escalates. It never loosens a check.
- **One progress metric.** A generated parity dashboard (`docs/parity.md`) shows:
  - upstream tests ported and passing (C++, GPU and Python);
  - oracle checks passing, per subsystem;
  - upstream files mapped;
  - open waivers.

### Porting card template

```markdown
### WP 2.3 — Lut3D forward
Upstream:     ops/lut3d/Lut3DOpData.cpp, Lut3DOpCPU.cpp, Lut3DOpCPU_{SSE2,AVX,AVX2,AVX512}.cpp @ v2.5.2
Rust target:  ocio-ops::ops::lut3d
Depends on:   1.1, 1.2, 1.5
Port tests:   tests/cpu/ops/lut3d/Lut3DOpData_tests.cpp, Lut3DOpCPU_tests.cpp (all SIMD variants)
Oracle:       fixtures/lut3d/* (sizes 2..129, tetrahedral/trilinear, every bit depth)
Byte-exact:   pixels bit-identical for each numeric profile; cache IDs and text byte-identical
Pitfalls:     FMA only in the AVX2/AVX-512 profiles; dispatch must match CPUInfo
Done when:    tests ported and tagged; oracle green on both reference platforms; no waivers; no new unsafe
```

---

## 8. Verification

### The oracle

- **What it is.** `oracle/` is a uv project pinned to `opencolorio==2.5.2`, the same wheel Ultravioleta runs. It lives in its own Python environment, separate from our module. Its scripts drive the real library and record:
  - pixels from probe images;
  - serialized configs and cache IDs;
  - CLF/CTF and baked-LUT text;
  - shader text and textures;
  - optimized op lists and group transforms;
  - Python `repr` strings;
  - errors and warnings.
- **Text is committed; pixels run live.** Platform-independent text is committed as fixtures. Pixel and texture checks run live on each CI runner, with the oracle and the port on the same machine, because the kernel choice and the math library belong to the machine. Every fixture records the CPU flags it was generated with.
- **Internals are visible through public APIs.**
  - Optimized op lists: `getOptimizedProcessor().createGroupTransform()`, written as CTF.
  - ACES 2.0 tables and fast-inverse LUT bakes: both are exposed as GPU textures.

### Upstream's Python suite

Upstream's `tests/python` (384 tests) runs unmodified against our standalone wheel. It is a black-box suite that needs only NumPy.

### Probe inputs

- Ramps and random values.
- All 65,536 half-float bit patterns.
- NaN, ±Inf and negative values.
- Every bit depth and pixel layout.
- Every optimization level.

### Corpora

- The 8 built-in configs.
- OpenColorIO-Config-ACES releases v1.0.0 to v4.0.0.
- Legacy v1 configs: spi-vfx, spi-anim, nuke-default and aces_1.x.
- Upstream's `tests/data/files` (261 files).
- Transform chains and configs generated with `proptest`, run against the oracle.
- Ultravioleta's replay corpus.
- Your studio configs, run locally only.

### GPU

- **Text and texture parity for all 10 targets.** If our shaders are identical to upstream's, they inherit upstream's validation on real GPUs.
- **Compile checks:**
  - glslang for GLSL;
  - DXC for HLSL, on Windows;
  - naga for Vulkan GLSL, which is how Ultravioleta already compiles `GLSL_VK_4_6`.
- **Execution:** headless wgpu on software GPU adapters, replaying upstream's 264 GPU test cases.

### Robustness and performance

- **Fuzzing.** `cargo-fuzz` targets for every parser, plus differential fuzzing that checks we accept and reject the same inputs as the oracle. Upstream's size limits apply: 1D LUTs up to 300,000 entries, 3D LUTs up to 129³.
- **Benchmarks.** criterion benchmarks, compared against:
  - the wheel's `ocioperf` on the same machine;
  - Ultravioleta's current native path (28 ms per UHD frame).

### CI

- **Platforms.**
  - Windows x86-64 runs on a GitHub runner. Spikes also run on your workstation.
  - Linux x86-64 runs in a Rocky Linux 9 container (glibc 2.34).
  - Both run the live oracle and Python 3.13.
  - ARM and macOS come later. Official wheels exist for both, so either can become a reference platform when needed.
- **Forced numeric profiles.** Every profile (scalar, SSE2, AVX, AVX2, AVX-512) is exercised, just as upstream reruns its tests for each SIMD mode.
- **Also in CI:**
  - the minimum Rust version;
  - Miri (Rust's undefined-behavior checker) on `unsafe` modules;
  - quick fuzz runs and nightly sweeps;
  - the Python wheel build and upstream's Python suite.

---

## 9. Architecture

### Workspace

```
OpenColorIO-rs/
├── Cargo.toml              workspace: lints, minimum Rust version, profiles
├── crates/
│   ├── ocio-ops/           op data, CPU kernels (numeric profiles), optimizer, pixel formats, CPUInfo
│   ├── ocio-formats/       LUT and transform file readers/writers
│   ├── ocio-gpu/           shader generation per language, textures, uniforms
│   ├── ocio/               public API: transforms, Config (YAML), Context, processors, built-ins,
│   │                       baker, .ocioz, app helpers, config merging
│   ├── ocio-py/            PyO3 module "PyOpenColorIO": embedded by Ultravioleta, also built as a wheel
│   ├── ocio-tools/         dev tools: ociocheck, ociochecklut, ociowrite, ociobakelut equivalents
│   └── ocio-testkit/       dev-only: fixture loading, exact comparators, replay runner
├── oracle/                 uv project pinned to opencolorio==2.5.2 (its own environment)
├── fixtures/               generated by the oracle; hash manifest; read-only for agents
├── corpus/ultravioleta/    captured requests, replayed byte-exactly
├── upstream/OpenColorIO/   git submodule @ v2.5.2 (also supplies tests/python)
├── xtask/                  oracle regen, parity dashboard, upstream diff → porting cards
├── upstream-map.toml       each upstream file → its Rust module (or not-applicable)
└── docs/                   parity dashboard, waivers, deviations, porting cards
```

- The crates follow upstream's real layering: `ocio-ops` ← `ocio-formats`, `ocio-gpu` ← `ocio` ← `ocio-py`.
- Inside each crate, files mirror upstream's paths. For example, `ops/lut1d/Lut1DOpCPU.cpp` becomes `ocio-ops/src/ops/lut1d/lut1d_op_cpu.rs`.

### C++ idioms → Rust

| C++ (upstream) | Rust (port) |
|---|---|
| `ConstConfigRcPtr` everywhere; `createEditableCopy()` | Owned values: `Arc<Config>` to share, `Clone` to edit (upstream's setters already deep-copy) |
| `Transform` hierarchy with `DynamicPtrCast` dispatch (23 types) | `enum Transform` with exhaustive `match` |
| `OpData` hierarchy (14 types plus markers) | `enum OpData` behind `Arc` |
| `OpCPU` renderers per style × bit depth × instruction set | `Box<dyn CpuOp + Send + Sync>`, chosen at finalize; each numeric profile is its own renderer |
| `void*` with byte strides | Typed, stride-aware image views |
| `Exception` / `ExceptionMissingFile` | `Result<T, Error>` with upstream's message text verbatim; mapped to `OCIO.Exception` / `OCIO.ExceptionMissingFile` in Python |
| Mutex-guarded caches; unguarded lazy state in `const` methods | `Mutex<HashMap<key, Arc<V>>>`, `OnceLock`; lazy state computed when the config changes |
| Globals: current config, logging callback, registries | `ArcSwap`; the `log` crate plus a callback called outside locks; `LazyLock` |
| `getenv` in 10 places | An injectable `Env` trait, so tests never call `unsafe set_var` |
| `std::regex` (ECMAScript) | `regex` for glob rules, `fancy-regex` for user-written regexes |
| `DynamicProperty` shared mutable values | Handles backed by atomics or `ArcSwap`, read once per `apply` |

### API sketches

Rust, as Ultravioleta will call it:

```rust
let config = ocio::Config::from_builtin("cg-config-v4.0.0_aces-v2.0_ocio-v2.5")?; // or from_file / from_env / from_uri
config.validate()?;
let view = ocio::DisplayViewTransform::new("ACEScg", "sRGB - Display", "ACES 2.0 - SDR 100 nits (Rec.709)");
let proc = config.processor(&view.into())?;                 // Arc<Processor>

let cpu = proc.default_cpu_processor()?;
cpu.apply_rgba(&mut strip);                                  // &mut [[f32; 4]]; bit-identical to applyRGBA

let mut desc = ocio::GpuShaderDesc::new(ocio::GpuLanguage::GlslVk4_6);
desc.set_allow_texture_1d(false);
desc.set_descriptor_set_index(1, 1);
desc.set_function_name("ocio_display");
desc.set_resource_prefix("ocio_");
proc.default_gpu_processor()?.extract_gpu_shader_info(&mut desc)?;
let text = desc.shader_text();                               // byte-identical to OCIO's
```

Python, as a user script in Ultravioleta:

```python
import PyOpenColorIO as OCIO                  # Ultravioleta's built-in module
config = OCIO.GetCurrentConfig()              # the project's config, shared with the app
proc = config.getProcessor("ACEScg", "sRGB - Display").getDefaultCPUProcessor()
proc.applyRGBA(pixels)                        # NumPy float32, in place; same bytes as the app
print(OCIO.__version__)                       # "2.5.2"
```

### Dependencies

| Need | Pick | Notes |
|---|---|---|
| YAML reading | `saphyr-parser` 0.1 (parse events with source spans) | We build our own ordered tree, keeping verbatim tags and line numbers. No serde: it loses tags, line numbers and duplicate-key errors |
| YAML writing | A hand-written emitter (~1K lines) | Must reproduce yaml-cpp's layout and float formatting byte for byte |
| XML | `quick-xml` ≥ 0.41 | Needed for the RUSTSEC-2026-0194/0195 fixes. We track line numbers ourselves |
| Half floats | `half` 2.7 | F16C and software conversions; spike S4 checks that they are equal |
| SIMD | `core::arch` plus runtime detection | Each SIMD kernel must equal its scalar numeric profile |
| Cache IDs | `xxhash-rust` (xxh3) | XXH3-128, printed low 64 bits then high 64 bits |
| `.ocioz` archives | `zip` 8.x (deflate only), version pinned | — |
| Regex | `regex`, `fancy-regex` | — |
| ICC | A hand-written reader (~400 lines) | OCIO reads only matrix/TRC profiles |
| Python | `pyo3` 0.29, `numpy` 0.29, `maturin` 1.15 | Embedded via `append_to_inittab`; a standalone wheel for Python 3.13 |
| GPU checks | naga + wgpu 30 (same as Ultravioleta), glslang, DXC via `hassle-rs` | — |
| Tests, benchmarks, CLI | `proptest`, `cargo-fuzz`, `cargo-mutants`, `criterion`, `clap` | No tolerance crates: comparisons are exact |

---

## 10. Roadmap

The order serves Ultravioleta first, and the library still reaches full 2.5.2 parity. Sizes are in person-weeks (pw) of conventional work; §11 translates them into agent time.

### Phase 0 — Harness and guardrails, before any porting (~7 pw)

| WP | Scope |
|---|---|
| 0.1 | Workspace; CI on Windows x86-64 and Rocky Linux 9; `cargo-deny`; LICENSE and NOTICE |
| 0.2 | The `upstream/` submodule at v2.5.2; `upstream-map.toml` listing every upstream file; `xtask upstream-status` |
| 0.3 | The oracle and `ocio-testkit`: exact comparators and bit-level diff reports |
| 0.4 | Guardrails: fixture manifest, CODEOWNERS, `waivers.toml`, test-count ratchet, parity dashboard |
| 0.5 | `cfmt`: float formatting matching C, iostream and yaml-cpp, checked on ~10⁷ values |
| 0.6 | Plumbing: `Error` with verbatim messages, logging, `Env`, the I/O trait, XXH3 cache IDs |
| S1 | Spike (1 week): write the 8 built-in configs back out byte-identically |
| S2 | Spike (3 days): port `sseLog2`/`sseExp2`/`ssePower` and prove them bit-exact |
| ~~S3~~ | Skipped: auditing `doubleailes/ocio-rs` (D5) |
| S4 | Spike (1 week), on your Windows workstation and on Rocky Linux 9: replicate `CPUInfo`; prove the SSE2 and AVX2 profiles of Lut3D tetrahedral bit-exact against the wheel; check that F16C and software half conversion give identical results |
| S5 | Spike (3 days), on Windows: compare the exact-math paths (fast math off) against the MSVC wheel, looking for SVML or auto-vectorized math-library differences |
| S6 | Deferred until ARM or macOS is in scope: map where C++ compilers fused multiply-adds, and where sse2neon's emulation differs |

### Phase 1 — Op engine and analytic ops (~10 pw)

- **1.1** Pixel formats, bit depths, packing and scanlines.
- **1.2** `enum OpData`, the op list, finalize, op cache IDs, and the `CpuOp` trait.
- **1.3** Matrix, Range, Exponent, Gamma (10 styles), Log (all styles), CDL, and the marker ops.
- **1.4** Fast math, plus min/max/clamp/rounding helpers that behave like C++.
- **1.5** Numeric profiles and dispatch.
- **1.6** The optimizer.

**Exit:** every op is bit-exact against the oracle at every bit depth, optimization level and profile.

### Phase 2 — LUTs and fixed functions (~13 pw)

- **2.1** Lut1D forward: standard and half-domain, integer lookups, hue adjust, composition.
- **2.2** Lut1D inverse, both exact and fast.
- **2.3** Lut3D forward in every profile, plus the fast inverse.
- **2.4** Fixed functions: 2.5.2's 23 public styles, except ACES 2.0.
- **2.5** ACES 2.0 (output transform, JMh, tone scale, chroma and gamut compression, tables).
- **2.6** B-spline evaluation for the ACES 1.x tone scale.

**Exit:** every op the built-in transforms use is bit-exact.

### Phase 3 — Config, transforms and Ultravioleta's calls (~25 pw)

| WP | Scope |
|---|---|
| 3.1 | `enum Transform` with 23 variants: validate, equality, and text identical to upstream's `operator<<` (the basis of Python `repr`) |
| 3.2 | Op builders: leaf transforms, groups, the 98 built-ins, and the color-space / display-view / look / named-transform paths |
| 3.3 | YAML reader: parse events into a tree that keeps tags and line numbers, then load; v1 and v2; yaml-cpp's scalar rules |
| 3.4 | Config model: roles, color spaces (aliases, categories, encodings, interop IDs, inactive), displays and views, view transforms, looks, named transforms |
| 3.5 | Context: variable resolution, environment modes, search paths |
| 3.6 | Processor API and caches: `getProcessor`, default CPU and GPU processors |
| 3.7 | YAML writer (byte-exact) and the config cache ID |
| 3.8 | `validate()` and the version-consistency checks, with verbatim messages |
| 3.9 | File rules (globs, ECMAScript regexes, upgrade from v1) and viewing rules |
| 3.10 | Built-in configs and `ocio://` URIs; loading from env, file or string |
| **3.11** | **`LegacyViewingPipeline`** (moved up because Ultravioleta uses it) |
| **3.12** | **`createGroupTransform` and the transform getters** (Ultravioleta's `cpu_ops`) |
| **3.13** | **GPU for `GLSL_VK_4_6`:** the shader-description model (descriptor sets, resource prefix, no 1D textures), shader writers for every op the built-in configs use, and textures |

**M0** lands mid-phase. **M1** (Ultravioleta running natively on the built-in configs) lands at the end.

### Phase 4 — File formats (~18 pw)

- **4.1** Format registry and `FileTransform`: upstream's probing order, the file cache, `cccid`.
- **4.2** spi1d, spi3d, spimtx, Iridas cube, Resolve cube, ITX.
- **4.3** 3DL, CSP, Houdini, Discreet 1DL, Truelight, Pandora, Nuke `.vf`.
- **4.4** XML base; CDL/CC/CCC read and write; Iridas `.look`.
- **4.5** CLF/CTF reader: CTF 1.2–2.5, CLF up to 3.0, SMPTE ST 2136-1.
- **4.6** CLF/CTF writer, byte-exact.
- **4.7** ICC reader.
- **4.8** Baker: all 12 bake formats.
- **4.9** `.ocioz` archives.

**M2:** Ultravioleta drops the Python worker.

### Phase 5 — Compositor features (~10 pw)

- **Dynamic properties.** Viewer exposure, contrast and gamma become uniforms (C7), so Ultravioleta no longer has to refuse uniforms.
- **Grading ops:** ExposureContrast, GradingPrimary, GradingRGBCurve, GradingTone, GradingHueCurve.
- **Helpers:** `ColorSpaceMenuHelper` (C9), the display/view helpers, and the mixing helpers (for the color picker).
- **The exact Lut3D inverse.**

**Exit (M3):** all of the above work.

### Phase 6 — Python module (~12 pw)

This is a parallel track for implementer B. It starts after M1, binding each part of the Rust API as it lands, and finishes after Phase 5.

| WP | Scope |
|---|---|
| 6.1 | The module skeleton; embedding in Ultravioleta (`append_to_inittab`); the standalone wheel; running upstream's Python suite in CI |
| 6.2 | 35 enums that behave like pybind11's: reachable both nested and at module level, with `__members__`, `.name` and `int()` |
| 6.3 | About 65 classes and 56 iterator classes (supporting `len`, indexing and iteration), with upstream's keyword names and defaults |
| 6.4 | Overload dispatch (for example, the 13 overloads of `Config.getProcessor`); pybind11's `TypeError` behavior; the `OCIO.Exception` and `OCIO.ExceptionMissingFile` types |
| 6.5 | NumPy and the buffer protocol for float32, float16, uint8 and uint16: in place, releasing the GIL |
| 6.6 | `ConfigIOProxy` subclassing from Python, and Python callbacks for logging and hashing |

**Exit (M4):** upstream's Python suite passes unmodified, and Ultravioleta scripts can `import PyOpenColorIO`.

### Phase 7 — Remaining GPU targets (~6 pw)

- GLSL 1.2, 1.3 and 4.0.
- GLSL ES 1.0 and 3.0.
- HLSL, MSL, OSL and Cg.
- The legacy GPU path that bakes the whole chain into one 3D LUT.
- A GPU execution harness replaying upstream's 264 GPU tests.

### Phase 8 — SIMD speed and threading (~6 pw)

- **SIMD kernels** for every numeric profile, each bit-identical to its scalar profile.
- **Pixel packing:** SIMD pack/unpack.
- **Parallel apply.**
- **Speed targets:**
  - at least as fast as Ultravioleta's current native path;
  - within 10% of C++.

### Phase 9 — Remaining library surface (~9 pw)

- **ConfigUtils:** identifying interchange spaces, and processors that span two configs.
- **Config merging:** `ConfigMerger` and the OCIOM format.
- **Everything still open on the parity dashboard.**

### Phase 10 — Release (~5 pw)

- The dev tools.
- The docs.
- The release: `v1.0.0` (`1.0.0+ocio.2.5.2`) is tagged when §3 holds.

---

## 11. Effort and calendar

| Phase | Person-weeks (conventional) |
|---|---:|
| 0 Harness and guardrails | 7 |
| 1 Op engine | 10 |
| 2 LUTs and fixed functions | 13 |
| 3 Config, transforms, Ultravioleta's calls | 25 |
| 4 File formats | 18 |
| 5 Compositor features | 10 |
| 6 Python module | 12 |
| 7 Remaining GPU targets | 6 |
| 8 SIMD and threading | 6 |
| 9 Remaining library | 9 |
| 10 Release | 5 |
| Integration (~10%) | 12 |
| **Total** | **~133** (range 120–160) |

**With 2–3 agents at a time.**

The assumption: the team delivers 3–5 conventional person-weeks of *verified* work per calendar week. The agents work many hours a day, but verification, rework and waiting for review slow them down. That gives:

| Milestone | Cumulative person-weeks | Calendar (low confidence) |
|---|---:|---|
| M0 — default config | ~30 | 6–10 weeks |
| M1 — Ultravioleta native on built-in configs | ~55 | 3–4 months |
| M2 — Python worker removed | ~73 | 4–6 months |
| M3 — compositor features | ~83 | 4.5–6.5 months |
| M4 — Python scripting | ~95 | 5–7.5 months |
| M5 — `1.0.0`, full 2.5.2 parity | ~133 | 6–10 months |

- **Re-estimate at the end of Phase 1,** from the measured person-weeks per calendar week.
- **The critical path is not code generation.** It is:
  - Phase 0 (the harness);
  - exact YAML and float formatting;
  - bit-exact kernels;
  - debugging mismatches.

---

## 12. Porting later OCIO versions

Here is the process for moving from one OCIO release to the next (for example, 2.5.2 → 2.6.0):
1. Branch `ocio-2.5` off for 1.x maintenance. On `main`, bump the submodule to `v2.6.0` and the oracle to `opencolorio==2.6.0`.
2. `xtask upstream-diff v2.5.2 v2.6.0` lists the changed upstream files. `upstream-map.toml` maps them to Rust modules, and one porting card is generated per change.
3. Port the new and changed upstream tests first (C++, GPU and Python), then regenerate the fixtures. Everything that fails is the work list.
4. Agents port until §3 holds for 2.6.0. Then tag `v2.0.0`, with release notes saying "matches OCIO 2.6.0". The crate version is `2.0.0+ocio.2.6.0`, and Python's `__version__` becomes `2.6.0`.

**Cost, calibrated on real release diffs** (in `src/` and `include/`):

| Diff | Files | Lines | Expected agent effort |
|---|---:|---|---|
| 2.5.0 → 2.5.2 (patch) | 70 | +0.8K / −0.25K | Days |
| 2.5.2 → 2.6-dev (small minor) | 67 | +2.4K / −1.2K | 1–3 weeks |
| 2.4.2 → 2.5.0 (large minor: config merging, hue curves, Vulkan) | 99 | +16.4K / −0.7K | 1–2 months |

- **The 2.6 line fits the platform.** OCIO 2.6 is VFX Reference Platform CY2027's OCIO, and Rocky 9 / glibc 2.34 is that year's Linux baseline, so the 2.6 line lines up with the platform completely.
- **Older lines work in reverse.** For example, 2.4.x for pipelines on CY2025, if a client needs it.
- **Fixes** land on the newest line and are backported only when needed.
- **Why the layout matters.** Mirroring upstream's layout keeps every version port a mechanical diff.

---

## 13. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Bit-exactness proves impossible on some path (MSVC SVML, half conversion, glibc differences) | Medium | High | Spikes S4 and S5 in Phase 0, on both reference platforms; a waiver only with your sign-off |
| Agents game the checks (editing expected values, loosening comparisons, skipping tests) | High | High | Oracle-only expected values; locked fixtures; CODEOWNERS; test-count ratchet; mutation testing; the verifier agent |
| Byte-exact text (yaml-cpp's emitter, iostream floats, Python `repr`) is harder than expected | Medium | High | Spike S1 and WP 0.5 come before porting; reference-output tests over every corpus |
| The Python surface is large (1,414 bindings; pybind11's enum, overload and `TypeError` behavior) | Medium | Medium | Upstream's Python suite is the gate; port each binding from upstream's pybind11 source |
| Different CPUs pick different kernels, so committed pixel fixtures don't travel between machines | Certain | Medium | Live checks on each runner; fixtures record CPU flags; forced-profile runs |
| Hidden behavior (env precedence, cache identity, error text) | Medium | Medium | Verbatim messages; behavior fixtures; the Ultravioleta replay corpus |
| Performance falls below Ultravioleta's current native path | Medium | Medium | Benchmarks from Phase 1; SIMD profiles; rayon over strips |
| 2–3 agents make the calendar long | Medium | Medium | Ultravioleta-first ordering, so value arrives at M1 and M2; re-estimate after Phase 1 |
| Review load | Low | Medium | You asked not to plan around your review time. Review stays narrow (API, waivers, deviations); the verifier agent reviews every chunk; chunks are small |
| Licensing, if the code is ever shared | Low | Medium | BSD-3 with upstream's notice from day one (D9) |
| Malicious configs or LUT files | Medium | High | Fuzzing, size limits, no `unsafe` in parsers |

---

## 14. Open questions

None right now. Answered on 2026-09-29:
- D13: the import name is `PyOpenColorIO`.
- The public GitHub repository is fine for now.
- Don't plan around the owner's review time.
- Work is split into mergeable chunks (§7).

---

## 15. Next steps

**Phase 0 progress (2026-09-29):**
- **Harness done.** WP 0.1–0.4 and most of 0.6 (logging and the I/O trait are left) are done on the local `phase0` branch:
  - workspace;
  - upstream submodule and map;
  - oracle and test kit;
  - fixtures with manifest;
  - guardrails (`cargo xtask ci`);
  - Rocky Linux 9 image;
  - CI workflow;
  - agent rules (`CLAUDE.md`).
- **Fixtures travel.** The first fixtures (the built-in configs' `serialize()` output and cache IDs) are byte-identical on Windows and Rocky Linux 9.
- **Spikes running.** S1/WP 0.5, S2/S5 and S4 run in parallel worktrees. Their reports land in `docs/spikes/`.

**Then:**
1. Verify and merge the spikes chunk by chunk. Decide on any waivers they propose.
2. Generate the Phase 1 porting cards, each split into mergeable chunks. Start implementer A and verifier C; implementer B joins when the dependency graph branches (formats, then Python).
3. In Ultravioleta, independently of the port: add the backend trait and capture mode, so the replay corpus starts growing now.

---

## Appendix A — Environment variables

OCIO 2.5.2 reads these 10 environment variables:

| Variable | Notes |
|---|---|
| `OCIO` | — |
| `OCIO_ACTIVE_DISPLAYS` | — |
| `OCIO_ACTIVE_VIEWS` | — |
| `OCIO_INACTIVE_COLORSPACES` | — |
| `OCIO_OPTIMIZATION_FLAGS` | Overrides the flags even when a caller passes them explicitly |
| `OCIO_USER_CATEGORIES` | — |
| `OCIO_LOGGING_LEVEL` | — |
| `OCIO_DISABLE_ALL_CACHES` | — |
| `OCIO_DISABLE_PROCESSOR_CACHES` | — |
| `OCIO_DISABLE_CACHE_FALLBACK` | — |

A config with no `environment:` section also loads the whole process environment into its context, and into the context's cache ID.

## Appendix B — Upstream behaviors to preserve

**Platform differences found in the wheel**
- The Windows wheel embeds the built-in configs' YAML with CRLF line endings: its CI checked the sources out with autocrlf. Upstream's files and the Linux wheel use LF. So `BuiltinConfigRegistry()[name]` returns different bytes on each platform, and the port must match per platform (D12). Parsed configs, `serialize()` and cache IDs are identical on both.

**Loading configs**
- `CreateFromFile` recognizes `.ocioz` archives by the zip "PK" signature, and it accepts `ocio://` URIs.
- Creating a `Config` reads the active display/view and inactive color space environment variables. This happens even in `Config::Create()`.
- A string `search_path` is split on `:`, even on Windows.

**Context variables**
- Three syntaxes: `$V`, `${V}` and `%V%`.
- The longest variable name is substituted first.
- Recursion stops at 32 levels.

**Loading LUT files**
- The loader tries the formats registered for the file's extension first, then every other format. The first reader that succeeds wins.
- `.lut` tries Discreet 1DL, then Houdini.
- `.cube` tries Iridas, then Resolve.

**Config versions in 2.5.2**
- Profile-version gates run from 2.1 (ACES gamut compression) to 2.5 (MIRROR NEGS displays, HSY, `GradingHueCurveTransform`, interop attributes).
- Versions above 2.5 are rejected.
- v1 configs change CDL and exponent clamping, and upgrade their file rules.

**YAML output**
- The top level is a block map.
- Transforms are flow maps, except Group.
- Floats use yaml-cpp's `%.7g` / `%.15g` formatting.
- Roles and environment entries are sorted.
- The minor version is written only if it is non-zero.

**Cache IDs**
- XXH3-128, printed low 64 bits then high 64 bits.
- The UUID form is high 64 bits then low 64 bits.

**Python**
- Enum values are reachable both nested and at module level.
- `__repr__` is the C++ `operator<<` text.
- `OCIO.Exception` and `OCIO.ExceptionMissingFile` are plain `Exception` subclasses.

## Appendix C — Sources

- **OCIO:** <https://github.com/AcademySoftwareFoundation/OpenColorIO>. Tag `v2.5.2`, plus `main` at `e7b7461` for the 2.6 differences.
- **Reference wheel:** <https://pypi.org/project/opencolorio/2.5.2/>.
- **Ultravioleta:** `scripts/ocio_worker.py`, `src/color.rs`, `src/color_ops.rs`, `src/gpu_viewer.rs`, `planning/color-management.md`.
- **VFX Reference Platform:** <https://vfxplatform.com/>. CY2026 specifies OCIO 2.5.x; CY2027 specifies OCIO 2.6.x, Python 3.13 and glibc 2.34.
- **Port precedents:**
  - fish: <https://fishshell.com/blog/rustport/>
  - rav1d: <https://www.memorysafety.org/blog/porting-c-to-rust-for-av1/>
  - zlib-rs: <https://trifectatech.org/blog/zlib-rs-is-faster-than-c/>
  - Fontations: <https://developer.chrome.com/blog/memory-safety-fonts>
