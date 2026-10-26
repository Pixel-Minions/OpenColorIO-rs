# Running OpenColorIO-rs: the orchestrator's handbook

This handbook lets **any** AI coding agent (or person) take over the day-to-day running of the
port, with no access to earlier conversations. It describes every step the previous orchestrator
took, the rules behind them, the state at handoff, and what comes next. It assumes only a shell
(Git Bash and PowerShell on Windows), `git`, `cargo`, `gh` and Docker Desktop on this machine.

Read, in this order: this file; `CLAUDE.md` (the rules for anyone who writes code here: its name
is historical, it applies to every agent); `PLAN.md` §3 (definition of done), §7 (how the work is
done) and §10 (roadmap); the card file of the work you pick up (`docs/cards/phaseN.md`).

---

## 0. Roles

- **Owner:** Jose Enriquez (GitHub org Pixel-Minions). Decides only: waivers, deviations, new
  dependencies, new API shapes, downloads, changes to `CLAUDE.md`, the `unsafe` allowlist,
  pausing and resuming. Does not review code.
- **Orchestrator (you):** plans the next cards, does or delegates the implementation, runs
  verification, stacks finished work, lands it on `phase0`, opens pull requests, merges `main`,
  keeps the machine healthy, and reports to the owner.
- **Implementer / verifier / helper:** roles, described in §4. If your platform can run several
  independent agent sessions, give each role to a fresh session with the prompts in §4. If it
  can't, do the implementer work yourself, one card at a time, and do verification as a separate,
  deliberately adversarial pass (a fresh session if possible) before landing.
- **Reviewer:** the previous orchestrator (a Claude agent) reviews your pull requests and
  reports (§10). Send the owner a short summary after each land so they can forward it.

### How to talk to the owner

- Short, direct, numbers first. They often ask "status?", "progress?", "how much is left?":
  answer with a small table (overall %, the counts from `docs/parity.md`, what is running, what
  is next, a time estimate).
- When you need a decision: one plain sentence on what changes for them (often "nothing
  visible"), a concrete example, then 2-3 options with **your recommendation first** and its
  trade-off. Avoid numerics jargon. They usually pick the recommendation when it's explained
  plainly. Batch questions (up to 4 at once).
- Owner's standing direction (2026-10-25): **wrap the project as fast as possible.** Progress is
  measured by merges to `main`; avoid tooling detours.

---

## 1. State at handoff (2026-10-25, evening)

| Item | State |
|---|---|
| `main` (GitHub) | `4161ee6c`: milestones M0, M1, M2, M3 done |
| `phase0` (local integration branch; main checkout `D:/Projects/OpenColorIO-rs` is always on it) | `f2538401` = main + land `fea753dc` (oracle cache keyed per command) + land `2cae5f65` (Python processors and baker) + README (about 80%) |
| PR #70 (`card/group-p13a`) | the oracle-cache land; contained in PR #71 |
| PR #71 (`card/p6-baker`, head `f2538401`) | waiting for CI. When its three required checks pass: `git push origin f2538401:refs/heads/main` (this merges #70 and #71; close #70 by hand if GitHub doesn't) |
| Land waiting to run | branch `card/group-p15`, worktree `D:/Projects/OpenColorIO-rs-wt/stack5`: 9 commits on `f2538401` (config merging 9.2d-9.2j: cards `p9-merge-2`, `p9-merge-3`; `docs/cards/phase7.md` with the owner's decisions) + this handbook. Paused by the owner just after starting; relaunch it (§6) |
| Counts on phase0 (`docs/parity.md`) | C++ tests ported 1,158 / 1,191; GPU tests 0 / 264; Python tests passing unmodified 375 / 384; upstream files 511 done + 230 n/a of 913 |
| Agents running | none |

### Next work, in order

1. Relaunch the `card/group-p15` land (§6). Merge PR #71 when green. Then push p15's result, open
   its PR (§7), merge when green.
2. **Card 6.8a** (`docs/cards/phase6.md`, row 6.8a; see also `docs/cards/phase9.md` around line
   82): bind `ConfigMerger`, `ConfigMergingParameters`, `ConfigMergingHelpers` and the ConfigUtils
   statics (`GetProcessorToBuiltinColorSpace`, ...) in `crates/ocio-py`. That makes the last 9
   upstream Python tests pass (8 in `MergeConfigsTest.py`, plus
   `ColorSpaceTest.test_processor_to_known_colorspace`) and completes **M4**. The SystemMonitors
   binding part of 6.8a needs card `p9-monitors` (9.3a) first; it isn't needed by those 9 tests.
   Rust API for merging (owner-approved): `merge_configs(params: Option<&ConfigMergingParameters>,
   base: Option<&Config>, input: Option<&Config>)` and `merge_color_space(params:
   &ConfigMergingParameters, base: Option<&Config>, cs: Option<&ColorSpace>)`.
3. A verification pass owed on `p9-merge-2`/`p9-merge-3` (they landed without one, under the
   wrap-fast rule): §4's verifier, HIGH findings only.
4. **Phase 7** (`docs/cards/phase7.md`, decisions P7-1..P7-8 all taken as recommended): wave 1 is
   three parallel cards: (A) the GPU execution harness (critical path), (B) the legacy GPU path,
   then the HLSL compile checks, (C) the oracle chunks O7.1/O7.2. The Rocky image rebuild may
   download Mesa llvmpipe (EGL, GL) and glslang once: approved (P7-4). Nothing else may be
   downloaded.
5. Phase 9's remaining cards (`docs/cards/phase9.md`: `p9-monitors`, `p9-catchup`,
   `p9-dashboard`), Phase 8 (SIMD and threading), Phase 10 (release).

### Recorded follow-ups (not scheduled yet)

- **Python processors** (notes: `D:/Projects/OpenColorIO-rs-wt/_handoffs-2026-10-24/p6-processors.HANDOFF.md`):
  - MEDIUM: a Python logging function that raises during `extractGpuShaderInfo`/`finalize`
    leaks the shader description (`crates/ocio-py/src/py_gpu_shader_creator.rs`,
    `with_desc_unlocked`): add drop guards that restore the outer hook and put the description
    back, and have the hook hold a weak handle.
  - MEDIUM: a logging function that extracts with the same `GPUProcessor`. Upstream holds
    `m_mutex` (`GPUProcessor.cpp:102`). The Windows wheel raises "resource deadlock would occur:
    resource deadlock would occur"; the Linux wheel deadlocks. Follow the U-181/U-182 precedent
    (`docs/improvements.md`): on Windows raise MSVC's text through `ExceptionKind::SystemError`
    (`Exception::system_error`); on Linux raise the port's own error under a new `U-` entry.
    **Owner decision needed** for a *second thread* waiting on that mutex: recommend waiting with
    the Python GIL released (a deviation, like D-9).
  - LOW: tests for mutants V1-V3 (RGB-curve, hue-curve, tone, grading-primary OSL warnings through
    the logging hook), V6 (two `DynamicProperty` holders), V7-V9 (U-196 on `getVectorInt`,
    `getBool`, `getFloat3`), V11-V13 (D-15 boundary values and `getOptimizedCPUProcessor`), V16
    (Rust unit test of `Texture::interpolation()` for 254).
- **Python baker:** on Linux, `Baker.setFormat(None)` may give libstdc++'s `std::logic_error` as a
  RuntimeError rather than a crash (handle like U-123); tests for non-UTF-8 bake output and for
  "file written only on success".
- **Container-script guard** (`xtask/src/guards.rs`): it wrongly flags `"$CARGO" run`-style lines
  and misses `exec docker`, `podman-remote`. Fix only if a real script trips it.

### Register numbers

Highest in use on `card/group-p15`: **I-437, U-201, D-15**; waivers W0001-W0002. Hand out fresh
ranges from **I-451 and U-221 upward**, a separate range per card (ranges given out earlier and
left unused stay unused).

---

## 2. The project in one page

An exact Rust port of OpenColorIO 2.5.2, accepted only on byte-for-byte parity with the official
`opencolorio==2.5.2` wheel on Windows x86-64 and Rocky Linux 9. The C++ source is the submodule
`upstream/OpenColorIO` (pinned at v2.5.2). Crates: `ocio-ops` (ops, CPU renderers), `ocio-gpu`
(shaders), `ocio-formats`, `ocio` (config, processors, file formats, app helpers), `ocio-py`
(the `PyOpenColorIO`-compatible module) and its `ffi`, `ocio-testkit` (oracle client, battery,
fixtures), `xtask` (gates, land, guards, parity).

- **The oracle:** `oracle/ocio_oracle/` runs the real wheel. Expected values come only from it,
  from upstream's tests, or from the platform (CLAUDE.md rule 2). Never from the port's output.
- **Registers:** `docs/improvements.md` lists every upstream bug ported (`I-n`) and every place
  the wheel has undefined behaviour where the port raises its own error instead (`U-n`); the owner
  decides at the end which to fix. `docs/deviations.md` lists approved deviations (`D-n`).
  `waivers.toml` holds owner-signed waivers.
- **Milestones:** M0-M3 done; M4 (Python module: all 384 upstream Python tests pass unmodified)
  next; then Phase 7 (GPU), 9, 8, 10.

### Standing owner decisions (all in the repository; the short list)

- The general rule for undefined behaviour in the wheel: the port raises an error (U- entry).
- **Platform rule (PLAN.md D12):** where the Windows wheel's behaviour is defined (e.g. MSVC's
  "resource deadlock would occur"), match it per platform; where Linux hangs or aborts, the port
  raises its own error with a U- entry.
- `#[doc(hidden)]` library hooks added for the Python binding are pre-approved.
- `unsafe` only in SIMD modules, the `ocio-py` FFI file (`crates/ocio-py/ffi/src/lib.rs`, with an
  allowlist of CPython calls), `crates/ocio/src/minizip/os_windows_ffi.rs`, the planned
  `system_monitor_windows_ffi.rs` (P9-3), and Phase 7's two GL harness files in
  `crates/ocio-gpu-exec` (P7-2). `cargo xtask guards` enforces the list; a new entry needs the owner.
- Deviations D-7..D-15 (Python binding): read `docs/deviations.md` before touching `ocio-py`.
- Every merge to `main` updates `README.md` progress.
- Group finished cards into one land and one PR whenever it saves time.
- Oracle changes (`oracle/`, new fixture groups) are their own chunk and get an independent
  verification pass with mutation testing.
- Since 2026-10-25 (wrap fast): a card lands once its **HIGH** verification findings are fixed;
  MEDIUM/LOW become recorded follow-ups. No new tooling projects unless a land is blocked.

---

## 3. The machine

Windows 11 workstation: Ryzen 9 9950X3D (32 threads, AVX-512), **62 GB RAM**, D: (887 GB, the
repository and builds), C: (Docker's disk image). Git Bash and PowerShell 7. **Python is not on
`PATH`** (use the oracle's venv or `uv run`).

### Hard rules

- **Never download anything** without the owner's approval: no clones, no network submodule
  init, no new crates or PyPI packages. `uv` runs offline (`UV_OFFLINE=1`) unless told otherwise.
  Exception approved: Phase 7's Rocky image packages (P7-4).
- **New worktrees get submodules only from local copies, all five overridden in one command**
  (a plain `git submodule update --init` clones from GitHub, which happened twice):

  ```bash
  M=D:/Projects/OpenColorIO-rs/.git/worktrees/p5-values/modules/upstream
  git -c protocol.file.allow=always \
    -c submodule.upstream/OpenColorIO.url=$M/OpenColorIO -c submodule.upstream/pystring.url=$M/pystring \
    -c submodule.upstream/yaml-cpp.url=$M/yaml-cpp -c submodule.upstream/expat.url=$M/expat \
    -c submodule.upstream/minizip-ng.url=$M/minizip-ng submodule update --init
  ```

  **Never remove the worktree `D:/Projects/OpenColorIO-rs-wt/p5-values`**: it holds these local
  copies and the wheels used by `tools/wheel-inspect` (`target/wheel-inspect`).
- Never change `.git/config` (the submodule URLs must stay the GitHub ones).
- Only the orchestrator pushes. Never force-push `main` (branch protection forbids it anyway).
- Never print environment variable values (names only). Never put the owner's email or personal
  data into external requests.
- Ask before deleting anything that might hold uncommitted work (check `git status` first).
- Never use `git stash` (shared by all worktrees). Use `cargo xtask gate --staged` to check a
  partial change.

### Resources

- One workspace build at a time beside a land; `CARGO_BUILD_JOBS=8` alone, 6 or 4 when a land
  runs. Two big builds at once failed with Windows "os error 1450, Insufficient system resources".
- At most 3 parallel implementers/verifiers while a land runs (memory).
- Helper programs that may grow (fuzz drivers, probes, mutation loops) run under a memory cap:
  `powershell -File D:/Projects/OpenColorIO-rs-wt/tools/run-capped.ps1 -MaxGB 8 -TimeoutMin 20 -Log out.log -Exe <program> -ArgString "<args>"`
  (exit 137 = killed for memory, 124 = time). A runaway fuzz driver once crashed the machine.
- Probe wheel behaviour carefully: a GPU texture of width 1 makes the wheel allocate ~30 GB (U-5).

### Docker and Rocky Linux 9

- `scripts/rocky9.sh <cmd>` runs a command in the `ocio-rs-rocky9:1` image with the checkout at
  `/work`. Each checkout gets a build volume `ocio-rs-target-<md5 of the lowercased Windows path,
  first 12 hex>`. Docker Desktop must be running:
  `Start-Process "C:\Program Files\Docker\Docker\Docker Desktop.exe"`. If it won't start after a
  crash, run `C:\Users\alenr\Start-DockerClean.ps1`.
- **One Rocky run at a time.** Check `docker ps` for an `ocio-rs-rocky9` container first. A land
  needs Rocky for most of its gate.
- The owner's other containers and volumes (`bugslayers`, `pipeline-desk`, `atlas_*`,
  `flight_plan_*`, `avr-ci-*`) are never touched. Never quit Docker yourself.

### Disk (the most frequent outage)

- Watch C: and D: during every land (`tools/watch_land.sh` stops when D: < 25 GB or C: < 40 GB).
- Each worktree's `target/` grows to 15-45 GB, its `target/gate-staged` to ~35 GB more. Delete a
  finished worktree's `target/` (remove a nested `target/gate-staged/wt` worktree first with
  `git worktree remove --force`).
- The land's build `target/land/target`: its `oracle-cache` (one full land with a cold cache
  writes ~118 GB) stays valid for unchanged commands since PR #70 (per-command keys); delete it
  only when D: is short.
- Rocky volumes (`docker volume ls | grep ocio-rs-target-`): the land's volume reached 295 GB once
  and filled C:. Delete stale ones with `docker volume rm`; keep `ocio-rs-cargo-registry` and
  `ocio-rs-uv-cache`. Docker's disk image (`docker_data.vhdx`) shrinks only when the owner runs
  `C:\Users\alenr\Compact-DockerDisk.ps1` as admin (it stops Docker): ask them.
- Long paths: delete with PowerShell and a **literal** path:
  `Remove-Item -LiteralPath '\\?\D:\Projects\OpenColorIO-rs-wt\<card>\target' -Recurse -Force`.

### Pitfalls that cost hours

- Never set `CARGO_TARGET_DIR` to another checkout's target for a **gate** or for `xtask`:
  `ocio-testkit` and `xtask` bake the workspace root in at compile time, and the gate then checks
  the wrong tree. Borrowing a target for plain `cargo test` is fine.
- Never `cd` into `target/land/wt` from a long-lived shell; inspect with absolute paths or
  `git -C`. A shell parked there blocks the next land from deleting it.
- Never pipe a gate into `tail` inside an `&&` chain (the exit status is lost).
- `sed` treats `\u` in a replacement as "uppercase"; write non-ASCII data as escapes and check
  with `od -c`.
- In PowerShell, `bash.exe` resolves to WSL's bash: always use
  `C:\Program Files\Git\bin\bash.exe`.
- Long commands started from an agent tool may be killed by the tool's time limit: start lands
  detached (§6).
- Windows Update restarts kill lands; after a reboot, start Docker and relaunch.
- Test races appear when tests share one process (the embedded Python interpreter, the logging
  function, caches, file renames on Windows). Fix the harness, never the comparison.

---

## 4. Doing a card

### 4.1 Card lifecycle

1. Pick the card (§1 "Next work"); read its row and the "How this phase works" section of its
   card file; note dependencies and the owner decisions it relies on.
2. Create its worktree from the current `phase0` tip (or from a card it builds on):
   `git -C D:/Projects/OpenColorIO-rs worktree add -b card/<card> D:/Projects/OpenColorIO-rs-wt/<card> <sha>`,
   then initialise the submodules (§3).
3. Implement chunk by chunk (§4.2). Each chunk is one commit, gated.
4. Verify (§4.3). Fix HIGH findings in new commits.
5. Stack with other finished cards and land (§5, §6). Push, PR, merge (§7). Clean up (§3 Disk).

### 4.2 Implementer instructions

Give these to an implementer session verbatim (filled in), or follow them yourself:

```
You are an implementer on OpenColorIO-rs (D:\Projects\OpenColorIO-rs), an exact Rust port of
OpenColorIO 2.5.2 accepted only on byte-by-byte parity with the opencolorio==2.5.2 wheel.
Card: <card> (<work packages>), in docs/cards/<phaseN>.md (<rows>).

Read first: CLAUDE.md (all of it), PLAN.md §3 and §7, the card, docs/deviations.md, and the code
patterns <list>. Work only in your worktree <path> on branch card/<card>.

Scope: <upstream files with line ranges; Rust targets; upstream tests to port or make pass>.

Rules:
- Port, don't reinvent: every ported function cites its source
  (`Port of `Foo::bar` (src/OpenColorIO/Foo.cpp:120-188 @ v2.5.2).`). Error and log text verbatim.
- Expected values only from the oracle (the wheel), upstream's tests, or the platform. Never from
  the port's own output, not even in "wiring" tests.
- Comparisons exact (bitwise floats, byte-for-byte text); only upstream's own tolerances in
  ported upstream tests. Never weaken a check; no #[ignore], todo!(), unimplemented!().
- No vacuous comparisons: every oracle comparison must fail if the wheel's answer changes. An
  exception entry also has a null result, an empty set compares equal, a skipped case proves
  nothing: assert exception/no-exception state, non-empty case counts and per-case identity.
- NaN operand order: wherever two NaNs can meet, read both wheels' machine code with
  tools/wheel-inspect (docs/wheel-inspect.md) before committing; MSVC and GCC often differ.
- Linux early: for numeric or platform-sensitive chunks, run the chunk's own test targets in
  Rocky (scripts/rocky9.sh env CARGO_BUILD_JOBS=8 cargo test -p <crate> --no-fail-fast --test <t>)
  after the orchestrator gives you the Rocky slot (check docker ps first).
- Undefined behaviour in the wheel (memory it doesn't own, aborts): the port returns a deliberate
  error and the case gets a U- entry; never a panic or a CPython side effect.
- Mutation-test your own chunk: at least the obvious mutants (a constant, an operand order, a
  boundary, a default) must fail a test; list them in the commit message.
- Register numbers: I-<a>..I-<b>, U-<c>..U-<d>. A chunk that edits docs/improvements.md or
  docs/deviations.md also runs `cargo test -p xtask register_merge` (one blank line before every
  heading, none at the end of a block, exactly one final newline).
- Report test-race fixes so the orchestrator can move them to the front of a land.
- A new API shape, deviation, dependency, download, or unsafe code beyond the allowlist: stop and
  report to the orchestrator before building on it (the owner decides).
- Chunks: one coherent unit, under ~600 non-test lines, tests in the same chunk, upstream-map.toml
  updated in the same chunk. Never commit docs/parity.md or docs/ratchet.toml.
- Gates: `git add <exactly the chunk>`, then `cargo xtask gate --staged --crates <crates>,xtask`
  while iterating and a full `cargo xtask gate --staged` before the commit, on your worktree's own
  target; CARGO_BUILD_JOBS=<n>; one cargo process at a time.
- Commit message: first line `<card>: <what> (<WP>)`; body: upstream files and lines ported, the
  tests, the evidence (which checks ran, on which platforms), the mutants; last line
  `Co-Authored-By: <your model/agent name>` (the previous agents used
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`; use your own identity).
- Never download, push, rewrite earlier commits, touch target/land, other worktrees or branches,
  git config, CLAUDE.md or permission settings. Never git stash.
- Keep HANDOFF.md (uncommitted) in your worktree current after every chunk: what's done (hashes),
  what's next, decisions, pitfalls, so a fresh session can continue. If you run low on context,
  stop at a chunk boundary and report.

Report: each commit hash (copied from git output), the tests ported or now passing, the register
entries, the gates run and their results, open questions.
```

### 4.3 Verifier instructions

```
You are an independent, adversarial verifier on OpenColorIO-rs. Review card <card>: commits
<shas> on branch card/<card> (worktree <path>). Never edit that worktree or branch.

Read CLAUDE.md, PLAN.md §3, the card, and the diff. Compare every ported function with its cited
upstream source (upstream/OpenColorIO @ v2.5.2). Probe the wheel through the oracle for cases the
tests miss (edge values, errors, None/null, threads, platform differences). Mutate the port in a
scratch copy (git worktree add --detach D:/Projects/OpenColorIO-rs-wt/verify-<card> <sha>,
submodules from local copies) to find checks that don't catch real changes.

Severity (owner's rule, 2026-10-25): FIX FIRST only for HIGH: output that differs from the wheel,
a hang, a crash, undefined behaviour reaching memory, or a vacuous test hiding a bug. MEDIUM and
LOW are listed as follow-ups and do not block.

Build with CARGO_TARGET_DIR=<the card's target> for plain cargo test only; never run a gate with
a borrowed target. Run growing probes through tools/run-capped.ps1. No Rocky without asking.
Never download, push, or touch other worktrees. Remove your scratch worktree when done.

Report: findings by severity, each with a reproduction (script path, wheel output vs port output);
mutants run and which survived; verdict LAND or FIX FIRST.
```

### 4.4 Fix rounds

- HIGH findings are fixed in **new commits** on the card branch (never amend), each with a test
  that fails before the fix.
- MEDIUM/LOW go into `HANDOFF.md` and the PR body as follow-ups.
- No full re-verification after a HIGH fix unless the fix is invasive; then a short check of the
  fix commits only. Oracle changes always get independent verification.

### 4.5 Questions that come up

- Owner decisions: ask (§0), record the answer in the card file or `docs/deviations.md` through
  the next chunk, and pass it to the implementer.
- The Rocky slot: grant it when `docker ps` shows no `ocio-rs-rocky9` container and no land is in
  its gate.

---

## 5. Stacking finished cards

Cards land as **groups**: a branch `card/group-<n>` built by cherry-picking the cards' commits
onto the current `phase0` tip. Tools are in `D:/Projects/OpenColorIO-rs-wt/tools/`:

- `regpick.sh <commit>`: cherry-picks one commit with the register merge driver; if
  `docs/improvements.md` conflicts because an old base wasn't in register form, merges it with a
  normalised base. Prints `ok`, `ok*`, `CONFLICT <files>` or `REGISTER MERGE FAILED`.
- `check_register_form.sh <base>`: checks every commit's register files (a land stops on these).

```bash
cd D:/Projects/OpenColorIO-rs
git worktree add -b card/group-<n> D:/Projects/OpenColorIO-rs-wt/stack<n> <phase0 sha>
cd D:/Projects/OpenColorIO-rs-wt/stack<n>
for c in $(git rev-list --reverse <card base>..card/<card>) $(git rev-list --reverse <base2>..card/<card2>); do
  bash ../tools/regpick.sh $c || break
done
bash ../tools/check_register_form.sh <phase0 sha>
```

- On `CONFLICT`: open the files, resolve (usually both sides added different items: keep both;
  for `upstream-map.toml` keep the newer row), make sure no `<<<<<<<`/`=======`/`>>>>>>>` remain
  (`grep -rn '^<<<<<<<' crates docs`), `git add`, `GIT_EDITOR=true git cherry-pick --continue`,
  then continue the loop from the next commit. Never commit conflict markers.
- Put test-race fixes first. Never land a branch stacked on a card that already landed
  (cherry-pick only the new commits onto `phase0`).
- A small fix needed only for the land (e.g. a Linux-only clippy lint) is a new commit on the
  group branch.

---

## 6. Landing

`cargo xtask land <branch>` runs from the main checkout (`D:/Projects/OpenColorIO-rs`, on
`phase0`, clean tree). It replays the branch onto `phase0` (no per-commit gates;
`--per-commit` restores them), merges `--no-ff`, regenerates `docs/parity.md` and
`docs/ratchet.toml`, then runs the full gate on the merge commit: fmt, clippy, ci, cargo-deny,
the test binaries built, then Rocky's whole gate **in parallel** with Windows' debug and release
tests, then `oracle check-all` on both platforms. Only then does `phase0` move. It never pushes.
About 2.5 h with a warm oracle cache, 3.5-4 h cold.

Start it detached (agent tools often kill long commands), from PowerShell:

```powershell
Start-Process -WindowStyle Hidden -WorkingDirectory D:\Projects\OpenColorIO-rs "C:\Program Files\Git\bin\bash.exe" -ArgumentList "D:/Projects/OpenColorIO-rs-wt/tools/run_land.sh","card/group-<n>","D:/Projects/OpenColorIO-rs-wt/tools/land-<n>.log"
```

Check the log appears within 20 s. `run_land.sh` appends `LAND EXIT <code>` at the end. Watch:

```bash
bash D:/Projects/OpenColorIO-rs-wt/tools/watch_land.sh D:/Projects/OpenColorIO-rs-wt/tools/land-<n>.log
grep -E "^gate: |^land: " D:/Projects/OpenColorIO-rs-wt/tools/land-<n>.log     # progress
```

Step logs: `D:/Projects/OpenColorIO-rs/target/land/wt/target/gate-logs/<time>/` (Rocky's in
`rocky/`). On success the log ends with the two push commands (§7) and `LAND EXIT 0`.

### When a land fails

`phase0` and the branch are untouched; `target/land/wt` stays for inspection. Seen in October:

| Symptom | Cause | Fix |
|---|---|---|
| A test fails in one build or platform only and passes alone | a test race (shared interpreter, logging function, `ClearAllCaches`, `copy.copy` adding `__slotnames__` to a class, rename onto an open file on Windows) | fix the harness; keep the comparison exact; put the fix first in the stack |
| clippy fails only in Rocky | a type is bigger on Linux (`large_enum_variant`) | box the variant |
| "os error 1450" | two big builds at once | stop other builds, relaunch |
| disk full / os error 112 | caches (§3) | clean, relaunch |
| CI fails before tests: "target/py-port ... not a valid Python environment" | the CI build cache restored a broken venv | fixed in `4161ee6c` (xtask rebuilds it) |

Fix with a new commit in the stack worktree, then relaunch the land. You can test a fix in the
land's warm build: apply the diff in `target/land/wt`, run the one target there
(`CARGO_TARGET_DIR=D:/Projects/OpenColorIO-rs/target/land/target cargo test ...` and
`scripts/rocky9.sh cargo ...` from that directory), then `git checkout -- <files>` to revert.

### Pausing (owner says "pause")

Stop agents; then stop the land: kill (PowerShell `taskkill /PID <id> /T /F`) every process whose
command line matches `run_land.sh|xtask land|xtask gate|target\land|rocky9.sh|ocio_oracle`
(`Get-CimInstance Win32_Process`), and `docker stop` the `ocio-rs-rocky9` container. Write down
what to relaunch. Nothing is lost except the land's elapsed time.

---

## 7. After a land

1. **README** (every merge to `main` updates it): take the counts from `docs/parity.md`'s
   Summary; update the status line ("about N% done"), the Progress paragraph ("Status as of
   <date>", the counts, what is in main, what's next) and the Milestones table. Commit on `phase0`
   (`README: progress after <group> (...)`).
2. **Push and open the PR** (the land prints the exact push; use its `--force-with-lease` form
   if the branch exists remotely):

   ```bash
   git -C D:/Projects/OpenColorIO-rs push origin <phase0 sha>:refs/heads/card/group-<n>
   gh pr create --repo Pixel-Minions/OpenColorIO-rs --base main --head card/group-<n> \
     --title "<what>" --body "<cards and what they bring; register entries; follow-ups; progress>"
   ```

3. **Merge** when the three required checks pass on that exact commit: **`Windows x86-64`**,
   **`Linux x86-64 (Rocky Linux 9)`**, **`cargo-deny`** (aggregators of the test shards; SDE jobs
   are advisory). Check with `gh pr checks <n> --repo Pixel-Minions/OpenColorIO-rs`. Then:
   `git -C D:/Projects/OpenColorIO-rs push origin <sha>:refs/heads/main` (fast-forward; the PR
   shows as merged). A later PR whose branch contains an earlier open PR merges both. A CI-only
   failure is fixed by a commit on `phase0`, pushed to the PR branch.
4. **Clean up:** archive each landed card's `HANDOFF.md` to
   `D:/Projects/OpenColorIO-rs-wt/_handoffs-<date>/`, `git worktree remove --force` the landed
   worktrees, `git worktree prune`, delete stale Rocky volumes, watch disk (§3).
5. **Report** to the owner (§0) and send the reviewer the PR link (§10).

---

## 8. Command reference

| Command | Use |
|---|---|
| `cargo xtask gate --staged [--crates a,b] [--release] [--rocky]` | gate exactly the staged chunk |
| `cargo xtask land <branch>` | land a group onto `phase0` (§6) |
| `cargo xtask ci` / `cargo xtask guards` | quick checks (no tests) |
| `cargo xtask parity` | regenerate `docs/parity.md` locally (never commit it from a chunk) |
| `cargo xtask upstream-tests` | every upstream test and whether it is ported |
| `cargo xtask py-suite` | upstream's Python suite against the port (`py-port/passing.txt`) |
| `cargo xtask oracle info` / `check-all` / `regen <group>` | the oracle |
| `cargo xtask clean-scratch [--yes]` | delete verifier scratch and landed worktrees' leftovers |
| `scripts/rocky9.sh <cmd>` | run in Rocky Linux 9 |
| `scripts/sde.sh <cpu> <cmd>` | run on an emulated CPU (nhm, snb, hsw, skl, skx) |
| `cargo test -p xtask register_merge` | register form check |
| `gh pr checks <n> --repo Pixel-Minions/OpenColorIO-rs` | CI status |

---

## 9. Time and effort, for planning

- A small binding card (one upstream file): ~2-4 h of implementation, ~1 h verification.
- A library card (~1,200 lines in 3 chunks): ~4-8 h.
- A land: 2.5-4 h; CI on the PR: ~1-1.5 h.
- M4 needs 6.8a (one card) after the p15 land: about 1-2 days end to end.
- Phase 7: 17 chunks in 9 cards; with 3 parallel implementers the critical path is 5 chunks.

---

## 10. Working with the reviewer

The previous orchestrator reviews your work. After each land (or each card, if you prefer), send:
the PR link; the cards and commit hashes; the land log's gate summary (`grep -E "^gate: |^land: "`);
any owner decisions asked and their answers; new register entries; follow-ups recorded. The
reviewer checks that: every chunk cites its upstream source; expected values come from the oracle
or upstream tests; no comparison was loosened and no check weakened; register and deviation rows
are accurate; owner decisions were asked, not assumed; the README matches `docs/parity.md`.
Findings come back as a short list; fix HIGH ones before merging `main`.
