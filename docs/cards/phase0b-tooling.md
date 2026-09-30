# Phase 0b: tooling round before Phase 1

**Why this round.** In Phase 0, roughly a quarter to a third of the effort went to overhead:
- agents couldn't commit, so work moved as patches and stashes;
- spikes had to be rebuilt into chunks after the fact;
- shared helpers were duplicated;
- generated files conflicted at merge time;
- chunks were gated and landed by hand.

Phase 1 has about 44 chunks, so removing this overhead first pays off quickly. Each card
below is a list of mergeable chunks (`CLAUDE.md` → "Chunks").

---

## T1: one command to check a chunk, one to land a branch (`xtask`)

| Chunk | What |
|---|---|
| T1a | **`cargo xtask gate [--crates a,b] [--release] [--rocky]`** runs the exact per-chunk check:<br>• `cargo fmt --all --check`<br>• `clippy --workspace --all-targets -D warnings`<br>• `xtask ci`<br>• `cargo test` in debug and, with `--release`, release<br>• with `--rocky`, the same inside `scripts/rocky9.sh`<br>It stops at the first failure, with a non-zero exit code and no pipes that can hide one. It prints a one-line summary per step. |
| T1b | **`cargo xtask land <branch>`**:<br>1. replays each commit of `<branch>` on top of `phase0`, running `gate` at each one (Windows; Rocky and release for commits touching numeric crates);<br>2. merges with `--no-ff`;<br>3. regenerates the generated files (T2);<br>4. runs the full gate on the merge;<br>5. commits.<br>It refuses to land if any step fails. Pushing stays a separate, explicit step. |

## T2: generated files that never conflict

| Chunk | What |
|---|---|
| T2a | **Chunks stop committing `docs/parity.md` and `docs/ratchet.toml`.**<br>• `xtask land` regenerates both at merge time.<br>• CI checks on `main` that they are current.<br>• In PRs, CI checks that the ported-test count did not go down relative to the base (the ratchet), without needing the committed file. |
| T2b | **Oracle commands register themselves.** `commands.py` imports every module of the package (`pkgutil`), so adding a command never edits a shared file. |

## T3: a standard oracle test battery (`ocio-testkit`)

| Chunk | What |
|---|---|
| T3a | **Probe sets** as one API:<br>• all half values;<br>• specials;<br>• seeded random values in several ranges;<br>• breakpoint neighbourhoods (±N ulp around given values);<br>• all channels cycled. |
| T3b | **Parameter generators:**<br>• typical values;<br>• extreme finite values (±1e38 for f32 and ±1e300 for f64 parameters, subnormals);<br>• NaN and ±Inf parameters.<br>The NaN/Inf cases automatically use the W0002 comparison, and nothing else does. |
| T3c | **`battery::run(spec_family, renderer)`** runs every combination against the oracle:<br>• transform spec × probe set × bit depths × packed/planar layouts × optimization flags (fast math on and off) × direction.<br>It reports failures by case. Every op family's card uses it, so coverage is the same everywhere and agents only supply the family-specific parts. |
| T3d | **Tiers:**<br>• `quick`, per chunk (sampled);<br>• `full`, per land;<br>• `exhaustive`, nightly (every f32, 10⁷ sweeps).<br>They are selected by an environment variable, and the gate picks the tier. |

## T4: shared machine-code inspection tools (`tools/wheel-inspect/`)

| Chunk | What |
|---|---|
| T4a | **The spikes' scripts, cleaned up and documented:**<br>• find an OCIO function in the Windows DLL (MSVC RTTI and vtables, `.pdata`) and in the Linux `.so` (symbols);<br>• dump its disassembly from both wheels;<br>• list its imports (libm, SVML).<br>A how-to in `docs/wheel-inspect.md`: "which operand order did the compiler use here?" in a few commands. |

## T5: PR-based CI on GitHub

| Chunk | What |
|---|---|
| T5a | **One PR per card**, on a `card/<id>` branch, with its chunk commits (merged with a merge commit, never squashed, so the chunks stay reviewable).<br>• CI runs the gate on GitHub's Windows and Rocky machines. Their CPUs differ from the local machine, so they exercise other SIMD kernels.<br>• A PR template and labels mark "needs owner" items (waivers, deviations, API shape).<br>• Agents never push: the orchestrator pushes branches and opens PRs. |
| T5b | **Nightly workflow:**<br>• the exhaustive tier;<br>• `cargo-mutants` on modules changed that day;<br>• Intel SDE emulation of older CPUs (`-p4p`, `-snb`, `-hsw`, `-skl`, `-skx`) for the oracle and the port.<br>SDE is downloaded from Intel's mirror, and its SHA256 is checked against the published value. |

## T6: housekeeping

| Chunk | What |
|---|---|
| T6a | **`cargo xtask clean-scratch`** removes:<br>• verifier and probe output under `target/verify*`;<br>• Docker volumes `ocio-rs-target-*` that belong to no existing worktree;<br>• worktrees of landed branches.<br>It lists everything first, and deletes only with `--yes`. |

## Owner decisions for this round

1. **Agent commits.** Background agents were refused `git add`/`git commit`. The proposal is a
   project permission rule allowing exactly those two commands (never `push`, `reset`,
   `rebase` or `stash`), for example in `.claude/settings.json`:
   `"permissions": { "allow": ["Bash(git add:*)", "Bash(git commit:*)"] }`.
   Only the owner changes permission settings.
2. **PRs.** The orchestrator pushes `card/*` branches and opens PRs on
   `Pixel-Minions/OpenColorIO-rs`. Optionally, protect `main` so it requires a green CI.
   That is a repository setting only the owner can change.
3. **SDE in CI.** The nightly job downloads Intel SDE from Intel's mirror under the license
   the owner accepted, and checks its SHA256 on every run.

## Order

T2b → T1a → T2a → T1b (the merge tooling). T3 and T4 run in parallel. Then T5 and T6. After
that, Phase 1 starts with its cards rewritten to use the gate, the battery and PRs.
