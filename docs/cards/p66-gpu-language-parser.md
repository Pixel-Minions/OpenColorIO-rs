# P66: the public GPU-language string conversion

The audit found GpuLanguageFromString only in the private Python binding.
Move the existing translation of ParseUtils.cpp:277-296 @v2.5.2 into
ocio_gpu::open_color_types::gpu_language_from_string and share it with the binding.

Use the established ParseUtils signature pattern:
`fn gpu_language_from_string(s: Option<&[u8]>) -> ocio_ops::Result<GpuLanguage>`.
Other FromString conversions already use nullable bytes and Result. None is the
upstream null pointer, bytes end at the first NUL, ASCII folding uses the existing
StringUtils translation, and errors retain the original input bytes. This introduces
no new ownership model, dependency, enum exception or deviation.

Close the OpenColorTypes.h and ParseUtils.cpp map entries after the conversion's
live oracle test passes on both platforms. Their other public constants, enums and
conversions were accounted for in the60-file audit; the map now includes the GPU
library implementation instead of relying on the binding's private copy.

One integration test uses the existing config_calls oracle command to compare
all ten languages, case variants, whitespace, null/empty strings, embedded NULs,
unsupported aliases, UTF8/non-UTF8 and every single-byte input. It compares enum
results and exception bytes only with the wheel, and checks ToString as well.
No oracle or fixture change and no new CPU-dependent/SDE target is needed.
The original384-function Python suite also qualifies the shared binding path.

Local format and source guards pass. Actual Rust compilation, live oracle runtime
and both-platform Python checks are pending normal CI. Do not claim the chunk landed
or the new behavior verified until that exact-head gate passes.
