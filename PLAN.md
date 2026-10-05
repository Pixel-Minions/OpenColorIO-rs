# OpenColorIO-rs — Porting Plan

**Status:** v0.9, 2026-10-04. Phases 0, 0b and 1 are complete (M0, §15); Phase 2 comes next.
- **Changes in v0.9:**
  - Phase 1 results and its measured duration (§1, §11, §15);
  - owner decisions: the optimizer's generic core moved to 1.2d (Option A); Lut1D E–J move to Phase 2 (WP 2.1, 2.5); upstream `Processor` and `CPUProcessor` tests that need `Config::Create()` move to Phase 3;
  - W0002 extended to the NaN entries of the 1D LUT the optimizer bakes from NaN parameters, and the CPU cache ID that hashes it.
- **Changes in v0.8:** Phase 0 results; W0001, W0002 and D-1 approved; the tooling round before Phase 1.
- **Changes in v0.7:**
  - parity with OpenColorIO is the goal, and consumers align with the port (§1, §6);
  - milestones are complete sections of OCIO, byte-exact on CPU and GPU in all 10 languages;
  - GPU writers land with each op family, and the GPU infrastructure moves to Phase 1;
  - Ultravioleta's replay corpus is no longer a release criterion;
  - `LegacyViewingPipeline` joins the app helpers (M3);
  - the exact Lut3D inverse moves to Phase 2;
  - CLF reading in 2.5.2 has no SMPTE ST 2136-1 support.
- **Changes in v0.6:** D13 decided (`PyOpenColorIO`); the public repository is fine for now; work lands as small mergeable chunks (§7, `CLAUDE.md`).
- **Changes in v0.5:** Rust 1.98.1 pinned (D7); exact upstream test counts; the Phase 0 progress and findings; repository visibility added to the open questions.
- **Changes in v0.4:**
  - our own version numbers, each stating the OCIO version it matches;
  - a Python module compatible with PyOpenColorIO is back in scope, because users will script Ultravioleta in Python;
  - planned for 2–3 agents at a time;
  - Rocky Linux 9 as the Linux reference.
- **Changes in v0.3:** skip the audit of the existing port; keep the crate private; Windows and Linux only; match OCIO on each platform.
- **Changes in v0.2:** pinned to OCIO 2.5.2; a byte-exact definition of done; an operating model for agents; Ultravioleta as the first consumer.

**Target:** OpenColorIO **2.5.2**, exactly: the OCIO line of the VFX Reference Platform CY2026. Later OCIO versions become new major versions of this crate (§2).

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
6. [Consumers](#6-consumers)
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
- **Parity first.**
  - The scope, the API and the order of work are OpenColorIO's.
  - Consumers align with the port, not the other way around. The first is Ultravioleta, which will replace its Python worker with the port section by section (§6).
  - Nothing in the port is tailored to one consumer.
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
  - Every module is checked against an *oracle*: the official `opencolorio==2.5.2` wheel.
  - Upstream's Python suite runs unmodified against our Python module.
  - Agents never write expected values, and CI enforces that (§7).
- **Byte-exact is achievable.**
  - OCIO 2.5.2 uses no hardware-approximate instructions.
  - FMA appears only in the AVX2/AVX-512 LUT kernels, which can be reproduced exactly.
  - MSVC's SVML math library is only used for PQ with fast math off. On Windows the port uses `powf` there, under waiver W0001: an invisible difference (§3).
  - On Windows and on Linux (Rocky 9), pixels will be bit-identical to the wheel on the same machine. Text is identical everywhere (§3).
- **Effort.** About 133 person-weeks of conventional work. With 2–3 agents at a time:
  - M0 (analytic transforms, CPU and GPU) in about 5–8 weeks;
  - M1 (LUTs, fixed functions, built-in configs) in about 3–4.5 months;
  - full 2.5.2 parity (`1.0.0`) in about 6–10 months.

  Measured: Phase 1, planned at 18 person-weeks, took 2026-09-30 to 2026-10-04 with 2–4 implementer agents plus verifiers. The re-estimate is in §11 (owner to confirm).

### Milestones: complete sections of OCIO

The goal is parity with OpenColorIO 2.5.2 as a whole. The port is not a subset tailored to
one application. It lands in **sections**, and each section is a complete slice of OCIO:
- it has OCIO's public API for that slice;
- it passes the upstream tests of the slice;
- **CPU:** byte-exact with the wheel at every bit depth, layout and optimization level;
- **GPU:** byte-exact shader text, uniforms and textures in all 10 shading languages.

A section is never "CPU now, GPU later", and never a partial feature: an op family lands whole,
forward and inverse. Every version below carries `+ocio.2.5.2`.

| Milestone | Section | Version |
|---|---|---|
| **M0** | **Analytic transforms.** The op engine and optimizer. Matrix, Range, Exponent, ExponentWithLinear, Log, LogAffine, LogCamera, CDL and Group transforms, with their ops. Processors from `Config::CreateRaw()`, CPU and GPU processors, and `GpuShaderDesc` for every language | `0.1.0` |
| **M1** | **LUTs, fixed functions and the built-in configs.** Lut1D, Lut3D, every fixed function (including ACES 2.0), the 98 built-in transforms, the config model with YAML read and write, color-space, display/view, look and named-transform processors, `createGroupTransform`, and the 8 built-in configs | `0.2.0` |
| **M2** | **Files.** All 24 file formats, `FileTransform`, config files and `.ocioz`, file rules, context variables and search paths, and the baker | `0.3.0` |
| **M3** | **Dynamic, grading and app helpers.** Dynamic properties, the grading ops, the app helpers (`LegacyViewingPipeline`, menus, display/view helpers, mixing), and the exact Lut3D inverse | `0.4.0` |
| **M4** | **Python.** The `PyOpenColorIO`-compatible module; upstream's Python suite passes unmodified | `0.5.0` |
| **M5** | **Everything else.** The remaining upstream tests, ConfigUtils, config merging, the legacy GPU path, the GPU execution harness, and SIMD speed | `1.0.0` |

Consumers adopt each section as it lands, through OCIO's API, and they align with the port. §6 covers Ultravioleta.

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
| Distribution | A git dependency; not published to crates.io | Consumers pin `tag = "v1.0.0"` |

If you ever publish to crates.io, this scheme works unchanged:
- every release has its own number;
- a new OCIO minor is always a major bump, so Cargo's default `^` requirement never crosses one.

---

## 3. Definition of done: byte by byte

A release (e.g. `1.0.0+ocio.2.5.2`) ships only when all four conditions hold.

**1. Upstream's C++ and GPU tests pass.** Every test in upstream's v2.5.2 C++ suite (1,191) and GPU suite (264) is ported and passes, or is on a reviewed not-applicable list. That list covers tests of C++-only mechanics, such as `shared_ptr` identity.

**2. Upstream's Python tests pass.** Upstream's Python suite (384 tests) passes **unmodified** against our Python module.

**3. Output is byte-exact against the oracle.**

| Output | Must be identical |
|---|---|
| Config YAML (`serialize`); CLF/CTF/CDL/CC/CCC files; baked LUT files; shader text and uniform layout; cache IDs; error and warning text; Python `repr` strings; menu and helper results; optimized op lists; `createGroupTransform` contents | Byte for byte, on every platform |
| CPU pixels, at all 6 bit depths, all optimization levels, and for every op; GPU texture data; LUT values computed while baking or composing | Bit for bit against the wheel on the same OS and CPU. The reference platforms are Windows x86-64 and Linux x86-64 (Rocky Linux 9) |

**4. No unapproved waivers.** Every exception is listed in `docs/waivers.md` with its root cause and your sign-off. CI fails on any mismatch that isn't listed there.

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
  - Replicate OCIO's CPU dispatch (`CPUInfo`) exactly, including its 2.5.2 quirk flags: SSE2_SLOW, SSE3_SLOW and SSSE3_SLOW on old AMD and Intel parts, AVX_SLOW on Bulldozer and Jaguar, and AVX2_SLOWGATHER on Zen 3 and earlier and on Haswell. (The EPYC 9V45 AVX-512 exception is 2.6 and later.) Then the same variant runs as in C++ on the same machine.
  - Implement each profile as exact scalar code first. `f32::mul_add` reproduces the AVX2/AVX-512 FMA results exactly.
  - Add SIMD later, with equality tests against the scalar profile.
- **Port OCIO's fast approximations bit-exactly:** `sseLog2`, `sseExp2`, `ssePower`, `sseAtan2` and `sseSinCos`. The default optimization level uses them.
- **Transcendentals use the platform's math library.** Rust's `std` calls the same CRT or glibc functions as C++ does on that platform.
- **What was checked in 2.5.2:**
  - no hardware-approximate instructions (`rcp`, `rsqrt`);
  - FMA only in `Lut1DOpCPU_AVX2/AVX512` and `Lut3DOpCPU_AVX2/AVX512`;
  - MSVC's SVML `_mm_pow_ps` only in PQ with fast math off (`FixedFunctionOpCPU.cpp:2133-2194`). **Decided (waiver W0001):** on Windows the port calls `powf`. 1.7% of values differ, at most about 4e-5 relative and only near black beyond that, so there is no visible difference. It is not tied to the MSVC toolset version. Linux stays bit-exact.

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
| **GPU shader infrastructure** | 3.0K | 3 | 10 targets |
| **SIMD** (SSE2/AVX/AVX2/AVX-512/F16C; NEON via sse2neon) | ~6.6K raw | 4 | Chooses kernels at runtime, with CPU-specific exceptions |
| **File formats:** 19 readers covering 24 formats; 12 can bake, 5 can write | 18.9K | 1–5 | CLF/CTF has a 6.5K-line reader and a 3.4K-line writer (raw lines) |
| **Built-ins:** 98 transforms and 8 configs | 3.2K + 377 KB YAML | 2 | Upstream has expected values for all 98 transforms |
| **App helpers:** menus, legacy viewing pipeline, mixing, display/view helpers | 2.5K | 2 | — |
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
| **D7** | Toolchain | Rust 1.98.1 pinned in `rust-toolchain.toml`; edition 2024; `rust-version = "1.98"` | Decided (Phase 0) |
| **D8** | `unsafe` policy | `#![forbid(unsafe_code)]` everywhere except the SIMD and Python-binding modules | Proposed |
| **D9** | License | BSD-3-Clause, keeping upstream's notice ("Copyright Contributors to the OpenColorIO Project") | Required (the port is a derivative work) |
| **D10** | Versioning | Our own semver; the matched OCIO version goes in build metadata (`1.0.0+ocio.2.5.2`) and in the release notes; tags `v1.0.0` (§2) | Decided |
| **D11** | Platforms | Windows x86-64 and Linux x86-64 (Rocky Linux 9, glibc 2.34), both bit-exact reference platforms from Phase 0. ARM and macOS come later | Decided |
| **D12** | Output across platforms | Do what OCIO does on each platform: where OCIO is byte-identical across Windows and Linux, so is the port; where OCIO differs, the port differs the same way | Decided |
| **D13** | Python module | Import name `PyOpenColorIO`, so existing scripts run unchanged. A standalone wheel, which Rust applications can also embed (`append_to_inittab`), sharing objects with the host. It can't share a Python environment with the official wheel, so the oracle gets its own | Decided |

---

## 6. Consumers

**The rule:** consumers align with the port, never the other way around.
- The port's scope, API and order of work are OpenColorIO 2.5.2's.
- A consumer calls OCIO's API (Rust, or `PyOpenColorIO` in Python) and adopts each section (§1) as it lands.
- Anything a consumer needs that OCIO 2.5.2 doesn't provide stays in the consumer.
- Nothing in this repository depends on a consumer: no consumer-specific APIs, tests or release criteria.

### Ultravioleta, the first consumer

Everything in this subsection is work in Ultravioleta, recorded here for context.

**How Ultravioleta uses OCIO today**, based on its `scripts/ocio_worker.py`, `src/color*.rs` and `planning/color-management.md`:

- **A Python worker.** `ColorManager` spawns a Python worker running `opencolorio==2.5.2`. They talk in length-prefixed JSON plus a binary payload, with six operations: `info`, `convert`, `display`, `gpu_shader`, `cpu_ops` and `gpu_convert`.
- **About 60 OCIO calls, in five groups:**
  - **Config:** file and built-in configs, `validate`; color spaces (name, aliases, encoding, `isData`); the `scene_linear` role; displays and views; the default display and view; looks and view looks.
  - **Processors:** `DisplayViewTransform`, color-space pairs, and `LegacyViewingPipeline` with a looks override.
  - **CPU:** the default CPU processor, and `applyRGBA` on f32 RGBA.
  - **Introspection:** `getOptimizedProcessor` + `createGroupTransform`, and the getters of Matrix, Exponent, ExponentWithLinear, LogCamera, Range and Lut1D.
  - **GPU:** `GLSL_VK_4_6` with 1D textures disallowed, descriptor set 1/1, a function name and a resource prefix; shader text; 2D/3D textures. Uniforms are refused today.
- **A native path.** `color_ops.rs` runs those six ops natively, taking ~28 ms per UHD frame instead of 612 ms through the bridge. It stays within 3e-5 of OCIO, but it isn't byte-exact.
- **Fallbacks.** Everything else goes through the bridge: 3D LUTs, CDLs, most fixed functions, and ADX's `LogTransform`.

**How Ultravioleta adopts the port:**
- **The same calls.** It replaces the worker's OCIO calls with the same OCIO calls on the port: `Config`, processors, `CPUProcessor::apply`, `GpuShaderDesc`. The worker's Python translates call for call.
- **Section by section.** Each section switches once Ultravioleta's own shadow mode (worker and port side by side) shows no byte differences. The worker stays as the fallback until the port covers everything Ultravioleta uses. `color_ops.rs` goes away as its ops' sections land.
- **Its plan items map onto OCIO APIs:**

| Ultravioleta plan item | OCIO API in the port |
|---|---|
| C1: canonical names and aliases | Config name resolution |
| C2: file rules and `interop_id` | `Config::getColorSpaceFromFilepath`, `ColorSpace::getInteropID` |
| C4: `$OCIO`, `ocio://`, built-in configs | `Config::CreateFromEnv`, `CreateFromFile` with `ocio://` URIs |
| C6: project variables | `Context` |
| C7: viewer dynamic properties | Dynamic properties |
| C8: transform nodes | `Transform` |
| C9: color space menus | `ColorSpaceMenuHelper` |

- **Scripting.** Users script Ultravioleta through the port's `PyOpenColorIO` module (M4), embedded in its interpreter. Existing PyOpenColorIO scripts, for example from Nuke, run unchanged within OCIO 2.5.2's API.

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
- **You own the checks.** CODEOWNERS assigns `fixtures/`, `oracle/`, `waivers.toml` and the vendored upstream Python tests to you. Agents never push: the orchestrator opens every PR, and a PR that touches them carries the `needs-owner` label until you approve it.
- **Exact by default.** Comparisons are exact unless a waiver in `waivers.toml` says otherwise, and every waiver needs your approval.
- **No hidden gaps.** The main branch may not contain `#[ignore]`, `todo!()` or `unimplemented!()` without a tracked waiver.
- **Coverage only goes up.** The count of ported upstream tests can only increase. Every upstream test and source file is either mapped or marked not-applicable with a reason.
- **Tests must catch changes.** `cargo-mutants` runs on the core modules to show that tests fail when the code is deliberately broken.
- **Standard checks:** clippy, fmt, `cargo-deny`, and `forbid(unsafe_code)` outside the SIMD and binding modules.

### Team shape: 2–3 agents at a time

| Agent | Role |
|---|---|
| Implementer A | The critical path: ops → transforms → config → processors |
| Implementer B | Independent branches of the graph as they open up: file formats, GPU writers, the Python module |
| Verifier C | Reviews every PR adversarially: reads the upstream code against the diff, hunts for divergences the tests miss, and adds oracle probes for them |

- With only 2 agents, run one implementer plus the verifier.
- Each agent works in its own git worktree and commits its card as a series of mergeable chunks. The orchestrator lands each card through a pull request, gated by CI (below).
- **Your review is narrow:** public API shape, waivers and deviations only.
- **Stuck on a mismatch.** An agent that can't explain a byte mismatch writes a minimal reproduction (an oracle script plus a failing test) and escalates. It never loosens a check.
- **One progress metric.** A generated parity dashboard (`docs/parity.md`) shows:
  - upstream tests ported and passing (C++, GPU and Python);
  - oracle checks passing, per subsystem;
  - upstream files mapped;
  - open waivers.

### Pull requests

Each card lands as one PR from its `card/<id>` branch, with its chunk commits. It lands as a
merge commit and is never squashed, so every chunk stays reviewable. Agents commit; only the
orchestrator pushes.

1. **Review.** The agent reports the card done. The orchestrator reviews it, and the verifier
   checks it.
2. **Open the PR.** The orchestrator pushes the branch and opens the PR with the template in
   `.github/`. CI runs on GitHub's Windows and Linux machines. Their CPUs differ from the
   workstation's, so they run SIMD kernels the workstation doesn't.
3. **Owner items.** Oracle and fixture changes, waivers, deviations, public API shape and new
   dependencies get their label plus `needs-owner`, and wait for your OK.
4. **Land.** `cargo xtask land` builds the merge commit locally and runs the full gate. It
   regenerates the generated files and replays the branch onto `main` first if other cards
   landed in the meantime. The orchestrator pushes that merge commit to the PR branch, so CI
   checks exactly what will land.
5. **Merge.** When CI passes, `main` moves forward to that commit with a fast-forward push, and
   GitHub marks the PR merged. The branch is then deleted.

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

- **What it is.** `oracle/` is a uv project pinned to `opencolorio==2.5.2`. It lives in its own Python environment, separate from our module. Its scripts drive the real library and record:
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
- Your studio configs, run locally only.

### GPU

- **Text and texture parity for all 10 targets.** If our shaders are identical to upstream's, they inherit upstream's validation on real GPUs.
- **Compile checks:**
  - glslang for GLSL;
  - DXC for HLSL, on Windows;
  - naga for Vulkan GLSL.
- **Execution:** headless wgpu on software GPU adapters, replaying upstream's 264 GPU test cases.

### Robustness and performance

- **Fuzzing.** `cargo-fuzz` targets for every parser, plus differential fuzzing that checks we accept and reject the same inputs as the oracle. Upstream's size limits apply: 1D LUTs up to 300,000 entries, 3D LUTs up to 129³.
- **Benchmarks.** criterion benchmarks, compared against:
  - the wheel's `ocioperf` on the same machine.

### CI

- **Platforms.**
  - Windows x86-64 runs on a GitHub runner. Spikes also run on your workstation.
  - Linux x86-64 runs in a Rocky Linux 9 container (glibc 2.34).
  - Both run the live oracle and Python 3.13.
  - ARM and macOS come later. Official wheels exist for both, so either can become a reference platform when needed.
- **Pull requests.** Every card lands through a PR (§7), and CI runs on the exact merge commit that will land. GitHub's runners have assorted CPUs, so over many runs they dispatch SIMD kernels the workstation doesn't (the first catch: an alpha bug in the Lut3D SSE2 and AVX kernels).
- **Forced numeric profiles.** Every profile (scalar, SSE2, AVX, AVX2, AVX-512) is exercised, just as upstream reruns its tests for each SIMD mode.
  - The port can run any profile on any CPU, but the wheel only runs the kernel its CPU selects.
  - So the CPU-dependent tests (`cargo cpu-tests`) also run under Intel SDE, on emulated Nehalem (SSE2 kernels), Sandy Bridge (AVX), Haswell (AVX2 with slow gather), Skylake (AVX2) and Skylake server (AVX-512), on both platforms (`scripts/sde.sh`, `.github/workflows/sde.yml`).
  - They run nightly and on PRs that touch the kernels, the test kit or the oracle.
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
│   ├── ocio-py/            PyO3 module "PyOpenColorIO": a wheel, also embeddable in Rust apps
│   ├── ocio-tools/         dev tools: ociocheck, ociochecklut, ociowrite, ociobakelut equivalents
│   └── ocio-testkit/       dev-only: fixture loading, exact comparators, replay runner
├── oracle/                 uv project pinned to opencolorio==2.5.2 (its own environment)
├── fixtures/               generated by the oracle; hash manifest; read-only for agents
├── corpus/               real-world configs (ACES releases, legacy v1), checked against the oracle
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

Rust (the same calls as OCIO's C++ API):

```rust
let config = ocio::Config::from_builtin("cg-config-v4.0.0_aces-v2.0_ocio-v2.5")?; // or from_file / from_env / from_uri
config.validate()?;
let view = ocio::DisplayViewTransform::new("ACEScg", "sRGB - Display", "ACES 2.0 - SDR 100 nits (Rec.709)");
let proc = config.processor(&view.into())?;                 // Arc<Processor>

let cpu = proc.default_cpu_processor()?;
cpu.apply_rgba(&mut strip)?;                                 // &mut [[f32; 4]]; bit-identical to applyRGBA; an error where OCIO reads past a LUT (U-1)

let mut desc = ocio::GpuShaderDesc::new(ocio::GpuLanguage::GlslVk4_6);
desc.set_allow_texture_1d(false);
desc.set_descriptor_set_index(1, 1);
desc.set_function_name("ocio_display");
desc.set_resource_prefix("ocio_");
proc.default_gpu_processor()?.extract_gpu_shader_info(&mut desc)?;
let text = desc.shader_text();                               // byte-identical to OCIO's
```

Python (the same API as PyOpenColorIO):

```python
import PyOpenColorIO as OCIO                  # the port's module
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
| GPU checks | naga + wgpu 30, glslang, DXC via `hassle-rs` | — |
| Tests, benchmarks, CLI | `proptest`, `cargo-fuzz`, `cargo-mutants`, `criterion`, `clap` | No tolerance crates: comparisons are exact |

---

## 10. Roadmap

The order follows OCIO's own layering, and each milestone is a complete section of OCIO (§1). Sizes are in person-weeks (pw) of conventional work; §11 translates them into agent time.

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

Every op family lands whole in one phase: op data, CPU renderers for every profile, and its
GPU writer for all 10 shading languages.

### Phase 1 — Op engine, analytic transforms and GPU infrastructure (~18 pw)

- **1.1** Pixel formats, bit depths, image descriptions, packing and scanlines.
- **1.2** The op model: `OpData`, `Op`, the op list, finalize, op cache IDs, `CpuOp`, and the CPU engine.
- **1.3** The analytic op families: Matrix, Range, Exponent, Gamma (10 styles), Log (all styles), CDL, and the no-op, allocation and reference ops. Each comes with op data, CPU renderers and GPU writers.
- **1.4** Fast math, plus min/max/clamp/rounding helpers that behave like C++.
- **1.5** Numeric profiles and dispatch.
- **1.6** The optimizer: every pass except the bit-depth bake, which needs Lut1D (2.5).
- **1.7** GPU infrastructure: `GpuShaderText` for all 10 languages, `GpuShaderCreator` and `GpuShaderDesc` (uniforms, textures, resource naming, descriptor sets), the class wrappers, and `GPUProcessor`.
- **1.8** Transforms for these families plus `GroupTransform`: validation, equality, and `operator<<` (the basis of Python `repr`). Also `BuildOps` and `CreateTransform` for them, `Config::CreateRaw()`, and `Processor`, `CPUProcessor` and `GPUProcessor`.

**Exit (M0):** through OCIO's API, the analytic transforms are byte-exact with the wheel:
- on the CPU, at every bit depth, layout, optimization level and profile;
- on the GPU, in all 10 languages.

Their upstream tests are ported.

### Phase 2 — LUTs and fixed functions (~16 pw)

- **2.1** Lut1D: forward (standard and half domain, integer lookups, hue adjust), inverse (exact and fast), and composition. Every profile, CPU and GPU.
- **2.2** Lut3D: forward in every profile, plus the fast and exact inverses. CPU and GPU.
- **2.3** Fixed functions: 2.5.2's 23 public styles, except ACES 2.0. CPU and GPU. On Windows, PQ with fast math off calls `powf` under waiver W0001, and the PQ chunk writes W0001's bound.
- **2.4** ACES 2.0 (output transform, JMh, tone scale, chroma and gamut compression, and the tables, which the GPU gets as textures). Also B-spline evaluation for the ACES 1.x tone scale.
- **2.5** The optimizer's LUT passes: `ReplaceInverseLuts` and the separable-prefix bit-depth bake.

**Exit:** every op family is complete on CPU and GPU.

### Phase 3 — Config, transforms and processors (~17 pw)

| WP | Scope |
|---|---|
| 3.1 | The remaining transforms (23 in all): validate, equality, `operator<<` |
| 3.2 | Op builders: the remaining leaf transforms, the 98 built-ins, and the color-space, display-view, look and named-transform paths |
| 3.3 | YAML reader: parse events into a tree that keeps tags and line numbers, then load; v1 and v2; yaml-cpp's scalar rules |
| 3.4 | Config model: roles, color spaces (aliases, categories, encodings, interop IDs, inactive), displays and views, view transforms, looks, named transforms |
| 3.5 | Context: variable resolution, environment modes, search paths |
| 3.6 | Processor API and caches: the `getProcessor` overloads, the processor cache and its fallback, `createGroupTransform`, processor metadata |
| 3.7 | YAML writer (byte-exact) and the config cache ID |
| 3.8 | `validate()` and the version-consistency checks, with verbatim messages |
| 3.9 | File rules (globs, ECMAScript regexes, upgrade from v1) and viewing rules |
| 3.10 | Built-in configs and `ocio://` URIs; loading from env, file or string |

**Exit (M1).**

### Phase 4 — File formats (~18 pw)

- **4.1** Format registry and `FileTransform`: upstream's probing order, the file cache, `cccid`.
- **4.2** spi1d, spi3d, spimtx, Iridas cube, Resolve cube, ITX.
- **4.3** 3DL, CSP, Houdini, Discreet 1DL, Truelight, Pandora, Nuke `.vf`.
- **4.4** XML base; CDL/CC/CCC read and write; Iridas `.look`.
- **4.5** CLF/CTF reader: CTF 1.2–2.5 and CLF up to 3.0. SMPTE ST 2136-1 support is 2.6 and later.
- **4.6** CLF/CTF writer, byte-exact.
- **4.7** ICC reader.
- **4.8** Baker: all 12 bake formats.
- **4.9** `.ocioz` archives.

**Exit (M2).**

### Phase 5 — Dynamic properties, grading ops and app helpers (~10 pw)

- **Dynamic properties** of every type: in the CPU renderers, and as GPU uniforms.
- **Grading ops:** ExposureContrast, GradingPrimary, GradingRGBCurve, GradingTone and GradingHueCurve. CPU and GPU.
- **App helpers:** `LegacyViewingPipeline`, `ColorSpaceMenuHelper`, the display/view helpers, and the mixing helpers.

**Exit (M3).**

### Phase 6 — Python module (~12 pw)

This is a parallel track for implementer B. It starts after M1, binding each part of the Rust API as it lands, and finishes after Phase 5.

| WP | Scope |
|---|---|
| 6.1 | The module skeleton; the embedding API for Rust applications (`append_to_inittab`); the standalone wheel; running upstream's Python suite in CI |
| 6.2 | 35 enums that behave like pybind11's: reachable both nested and at module level, with `__members__`, `.name` and `int()` |
| 6.3 | About 65 classes and 56 iterator classes (supporting `len`, indexing and iteration), with upstream's keyword names and defaults |
| 6.4 | Overload dispatch (for example, the 13 overloads of `Config.getProcessor`); pybind11's `TypeError` behavior; the `OCIO.Exception` and `OCIO.ExceptionMissingFile` types |
| 6.5 | NumPy and the buffer protocol for float32, float16, uint8 and uint16: in place, releasing the GIL |
| 6.6 | `ConfigIOProxy` subclassing from Python, and Python callbacks for logging and hashing |

**Exit (M4):** upstream's Python suite passes unmodified.

### Phase 7 — Legacy GPU path and GPU execution (~3 pw)

All 10 shading languages already come with each op family (Phases 1–5). This phase adds:
- the legacy GPU path, which bakes the whole chain into one 3D LUT;
- a GPU execution harness replaying upstream's 264 GPU tests: headless wgpu, plus glslang and DXC compile checks.

### Phase 8 — SIMD speed and threading (~6 pw)

- **SIMD kernels** for every numeric profile, each bit-identical to its scalar profile.
- **Pixel packing:** SIMD pack/unpack.
- **Parallel apply.**
- **Speed target:** within 10% of C++ OCIO on the same machine (`ocioperf`).

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
| 1 Op engine, analytic transforms, GPU infrastructure | 18 |
| 2 LUTs and fixed functions | 16 |
| 3 Config, transforms and processors | 17 |
| 4 File formats | 18 |
| 5 Dynamic properties, grading ops, app helpers | 10 |
| 6 Python module | 12 |
| 7 Legacy GPU path and GPU execution | 3 |
| 8 SIMD and threading | 6 |
| 9 Remaining library | 9 |
| 10 Release | 5 |
| Integration (~10%) | 12 |
| **Total** | **~133** (range 120–160) |

**With 2–3 agents at a time.**

The assumption: the team delivers 3–5 conventional person-weeks of *verified* work per calendar week. The agents work many hours a day, but verification, rework and waiting for review slow them down. That gives:

| Milestone | Cumulative person-weeks | Calendar (low confidence) |
|---|---:|---|
| M0 — analytic transforms, CPU and GPU | ~25 | 5–8 weeks |
| M1 — LUTs, fixed functions, built-in configs | ~58 | 3–4.5 months |
| M2 — files | ~76 | 4.5–6 months |
| M3 — dynamic properties, grading, app helpers | ~86 | 5–7 months |
| M4 — Python module | ~98 | 5.5–8 months |
| M5 — `1.0.0`, full 2.5.2 parity | ~133 | 6–10 months |

- **Re-estimate at the end of Phase 1,** from the measured person-weeks per calendar week.
  - **Measured:** Phases 0 and 0b (7 pw) took 2026-09-28 to 2026-09-30. Phase 1 (18 pw) took 2026-09-30 to 2026-10-04, with 2–4 implementer agents plus verifiers. M0 (~25 pw) was planned at 5–8 weeks.
  - **Proposed wording (owner to confirm):** "The calendar above is replaced by the measured rate: 18 pw in 5 days in Phase 1. The remaining phases are scaled from it and re-checked at the end of Phase 2, because LUT, config and file-format work is more text and parsing than Phase 1 was."
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
| Hidden behavior (env precedence, cache identity, error text) | Medium | Medium | Verbatim messages; behavior fixtures; real-world config corpora |
| Performance falls well below C++ OCIO | Medium | Medium | Benchmarks from Phase 1 against `ocioperf`; SIMD profiles; rayon over strips |
| 2–3 agents make the calendar long | Medium | Medium | Complete sections land in order, so consumers can adopt early; re-estimate after Phase 1 |
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

**Phase 0: complete (2026-09-30).** Everything is merged to `main` on GitHub and green in CI on Windows, Rocky Linux 9 and cargo-deny.
- **Harness.**
  - WP 0.1–0.4 and most of 0.6: workspace, upstream submodule and map, oracle and test kit, hash-locked fixtures, guardrails (`cargo xtask ci`), the Rocky Linux 9 image, CI, and the agent rules (`CLAUDE.md`).
  - Logging moved to Phase 1 chunk 1.2e; the I/O trait belongs with config loading in 3.10.
- **Spikes.** Each is bit-exact against the wheel on both platforms, in debug and release, and was reviewed independently. The reports are in `docs/spikes/`.
  - **S2 + S5:** fast math, and the Log and Gamma CPU renderers. S5 found SVML only in PQ with fast math off (waiver W0001, invisible).
  - **S4:** CPU dispatch identical to the wheel's `ociocpuinfo`; the Lut3D forward kernels in every SIMD profile, including FMA; three half-float conversions compared on every input.
  - **S1 + WP 0.5:** C/C++ number formatting and parsing; a yaml-cpp 0.8.0 emitter port that re-emits the 8 built-in configs byte for byte, with byte-string semantics.
- **Upstream tests ported:** 68 C++ (ratchet 68).
- **Findings that became rules:**
  - pin the NaN operand order the wheel compiled;
  - test in release builds too;
  - OCIO strings are bytes;
  - platform differences are reproduced per platform (D12): camera-log break in float vs double, NaN signs and text, the Windows wheel's CRLF built-ins, old glibc `log2`;
  - W0002 (NaN parameters) and D-1 (classic locale) are approved.
- **The one-time failure after the S1 merge** was almost certainly the oracle pipe problem found in Phase 0b (below), since fixed.
- **Actuals:** Phase 0 was budgeted at about 7 person-weeks and took about one day of wall-clock time with 3 agents plus reviewers. The calendar in §11 is likely pessimistic; it gets re-estimated after Phase 1 from measured throughput.

**Phase 0b, the tooling round: complete (2026-09-30).** Card `docs/cards/phase0b-tooling.md`.
- **Pull requests (T5a):**
  - every card lands through a PR, and CI runs on the exact commit that lands;
  - `main` is protected: all three CI checks are required, admins included, and force pushes are blocked;
  - labels mark the owner's items.
- **Merge tooling (T1, T2, T6):**
  - `cargo xtask gate` (with `--staged`), `cargo xtask land` and `cargo xtask clean-scratch`;
  - generated files are regenerated at landing, so chunks never conflict on them;
  - the ratchet's baseline comes from the base commit;
  - oracle commands register themselves.
- **The test battery (T3):** one engine tests every op family, with tiers (quick locally, full in CI), generated extreme, NaN and ±Inf parameters, and pass-through checks on every numeric profile. Log and Gamma moved onto it: 1.09 billion comparisons at full, in 27 oracle processes instead of 308.
- **Emulated CPUs (T5b, the SDE part):** Intel SDE runs the CPU-dependent tests on Nehalem, Sandy Bridge, Haswell, Skylake and Skylake server, on both platforms, nightly and on kernel PRs, so every kernel the wheel has is checked.
- **Machine-code inspection (T4):** `tools/wheel-inspect`.
- **Verification:** each tooling card got two or three adversarial verifier rounds (mutation testing in scratch clones), and every finding was fixed before landing.
- **Bugs found on the way, all fixed:**
  - the Lut3D SSE2/AVX kernels quieted signaling-NaN alphas in release builds. LLVM had turned a copy into arithmetic, which led to the "Channels that pass through" rule;
  - an oracle pipe failure under load: large requests are now written in pieces, and writer errors are reported;
  - the oracle cache mixing up emulated CPUs.
- **Still open from T5b:** the nightly exhaustive tier and `cargo-mutants` on changed modules. They are built during Phase 1, not before it.

**Phase 1: complete (2026-10-04).** Card file `docs/cards/phase1.md`. Milestone M0: the analytic transforms through `Config::CreateRaw()` processors, byte-exact on CPU and GPU.
- **Cards landed:** `p1-oracle-image`, `p1-math`, `p1-foundations` (and `p1-foundations-fix`), `p1-oracle-2`, `p1-bitdepth-2`, `p1-gpu-infra-3`, `p1-engine-3`, `tooling-1`, `p1-gpu-ops`, `p1-matrix-4` (Range and Matrix), `p1-gamma-2`, `p1-cdl-2`, `p1-exponent-4`, `p1-log-2`, `p1-gpu-gamma-2`, `p1-optimizer-2`, `p1-gpu-ops4`, `p1-transforms-fam4` (every transform, with Allocation and Lut1DTransform), `tooling-2`, `p1-processor-4`, `p1-tests-catchup-3`, `p1-api-parity-2`.
- **Through the public API:** `p1-api-parity-2` checks every analytic transform through the port's own processors against the wheel:
  - CPU at every bit depth (U8, U10, U12, U16, F16, F32) in and out, packed RGBA, RGB and BGRA, planar RGBA and RGB, and every optimization level;
  - GPU in all 10 languages at every optimization level;
  - AllocationTransform too. It found no parity bugs.
- **Upstream tests ported:** 262 of 1,191 C++ (ratchet 262); GPU 0 of 264 (Phase 7, with the pixel harness); Python 0 of 384.
- **Deferred by owner decisions:**
  - Lut1D E–J (float interpolation, hue adjust, SIMD, inverse): Phase 2, WP 2.1 and 2.5;
  - the GPU processor of a baked U8 or LUT processor returns "not ported yet" until Phase 2;
  - `multi_op_prefix` (Phase 2) and `opt_prefix_test1` (needs the CTF reader);
  - upstream `Processor` and `CPUProcessor` tests that need `Config::Create()`: Phase 3 (2026-10-04);
  - F5: the 10- and 12-bit in-place wheel test is skipped.
- **Waivers:** W0002 was extended on 2026-10-04 to the NaN entries of the 1D LUT the optimizer bakes from NaN parameters (integer and half-float input), and the CPU cache ID that hashes it. Everything else is exact.
- **Improvements register:** `docs/improvements.md` grew from 22 entries at the end of Phase 0 to 68 (54 `I-` and 14 `U-`).

**Then: Phase 2**, following `docs/cards/phase2.md`, in parallel with Phase 3 (owner decision, 2026-10-05). M1 is LUTs, fixed functions and the built-in configs.

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
