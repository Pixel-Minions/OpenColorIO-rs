# Five finite GPU exact failure pins

The owner approved five individual pins on2026-10-30 after the debugger's report
covered the four ACES2 failures and the existing minimal pow proof covered GamutComp13
inverse. There is no port/harness defect. W0013 remains unchanged; no new class rule
or threshold is introduced. All59 original fixed-function cases run unchanged.

The [report](../kept/gpu-exact-pins/2026-10-30/aces2-debug/ACES2-GPU-FINDINGS.md)
records standalone replay, wheel stage matching, CPU-layout equivalence, builtin
accuracy checks and knockout experiments. The key causal controls are:

- nan_bug: identical lms inputs, pow differences of1/2/1ULP and Aab cancellation;
  injected wheel rgb_a or JMh gives exact CPU output. The CPU changes branch at
  neighbouring input floats, and even correctly rounded GPU builtins do not make
  this input stable. See reportsection1 and I-544.
- P3 round trip: atan-only or pow-only replacement passes0.018; all replacements
  reach0.0100052. The near-zero residual is amplified by PQ encoding.
- Rec709 round trip: atan replacement passes0.004; all reach0.0029326. Gamma2.4
  similarly amplifies the near-zero residual.
- RGB-to-JMh: pow or atan replacement passes0.0002; both reach3.05e-5. The low-chroma
  Aab cancellation amplifies small builtin errors.
- GamutComp13 inverse: minimal pow reproduces the original GPU output; replacing
  only pow word3f7a4c78 with platform3f7a4c75 brings error below3e-6. See the
  [existing proof](../kept/gpu-exact-pins/2026-10-30/GAMUT-ROOT-CAUSE-STATUS.md).

`waivers.toml` contains W0014-W0018 separately. The existing unpublished lookup keeps
its signature; its parser accepts only these five id/test pairs, exact recorded
renderer fields and required safeguards. Retired Matrix rules remain invalid.

The private execution guard always checks an active pin, including a newly passing
comparison. It requires original settings/error bits/component, no NaN/Inf-category
change, full live port/wheel GPU identity and recorded input/CPU/GPU hashes. Debug and
release original captures have identical hashes. nan_bug's six reported RGB words are
also checked directly. Renderer, language, native proof, sampling, output or result
changes fail closed. No expected pixel buffers are committed or substituted at runtime.

Rejection units cover a newly passing result, changed error/index/settings/renderer,
one-bit changes anywhere in input/CPU/GPU data, wrong nan_bug RGB pins, missing native
proof, wrong language/run and broken whole-image identity. Live pin records are saved
beside native GPU controls for independent CI verification.

Canonical full evidence is kept at `kept/gpu-exact-pins/2026-10-30/` in the primary
checkout. Raw debugger/CI evidence, source and hash provenance remain kept locally;
the approved code, waiver register and causal documentation are published.
No old protected evidence or debugger source is moved, edited or deleted. Full focused
GPU validation and final-head normal CI are still required before landing.
