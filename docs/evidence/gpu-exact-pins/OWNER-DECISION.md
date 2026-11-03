# Owner decision, 2026-10-30

The owner supplied `ACES2-GPU-FINDINGS.md` from local debugger commit370cd592 and
approved five individual exact-pin waivers for the four ACES2 failures and
GamutComp13 inverse. This supersedes the earlier five-test hold, without changing
W0013 or authorizing a new class rule.

Required: run original upstream cases/thresholds unchanged; complete live port/wheel
GPU bit identity; exact observed error words/component; nan_bug's reported GPU/CPU
RGB words; recorded Mesa25.2.7/LLVM21.1.8 scope, failing closed elsewhere; reject every
changed result, including a new pass. Record W0014-W0018, cite knockout experiments,
preserve evidence under kept, add nan_bug ill-conditioning to improvements, then
land all59 cases only after full normal CI.

Implementation also pins complete input/CPU/GPU hashes because debug and release
original captures are identical. These are measured waiver scope boundaries, not
replacement expected pixel images. Every actual reference remains live.

The debugger's branch/worktree is read only and never pushed. Complete canonical
evidence was copied to the primary checkout's kept/gpu-exact-pins/2026-10-30 before
implementation. Source/text/provenance may be committed; pixel buffers remain kept.
Original kept/gpu-next/2026-10-29 is never moved, edited or deleted.
