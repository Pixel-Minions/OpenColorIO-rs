# Native GPU retest references

The upstream harness extracts the GPU processor and shader description once,
then updates the CPU processor's property and the shader description's separate
property before each retest. `dynamic_apply` already exposes both targets and
actual uniform/property reads; no new oracle command is needed.

The Suite4 reference helper sends an original CPU stream, a complete finite CPU
stream when required, and a retained native GPU-description stream. Each stream
replays the same source updates. Every frame's uniforms and dynamic-property
values come directly from the wheel's recorded reads. Shader text, resources,
layout and property values are independently checked against fresh per-state
wheel extraction too. Original GPU processor flags come from the initial actual
processor. All GPU execution, finite-footprint, full finite-companion and native
readback checks remain exact as before.

A fresh processor is a different metadata reference. An original Primary GPU
processor can retain hasChannelCrosstalk=true after its separate shader property
is reset to identity, while a fresh identity processor reports false. Tone's
identity reset can preserve unused cached coefficient values, while a fresh
identity description starts with zeroed coefficients. A minimal official-wheel
2.5.2 probe reproduces both observations without rendering or port code; its
sources and results are kept under kept/gpu-next/2026-10-29/dynamic-target-probe.

The live regressions compare original and complete finite CPU frames with
independent fresh wheel CPU processors, check retained processor flags, and
check native Tone uniform history on reset. These controls fail with the earlier
Processor-property CPU binding or fresh-per-frame GPU metadata transport.
