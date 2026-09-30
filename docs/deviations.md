# Deviations from OpenColorIO 2.5.2

Where the port intentionally behaves differently from upstream. Each entry needs the
owner's approval. A deviation must never change an output covered by the byte-exact
definition of done (PLAN.md §3) unless it also has a waiver in `waivers.toml`.

Upstream bugs that don't affect outputs (crashes, data races, deadlocks) are fixed in the
port and listed here. Upstream bugs that do affect outputs are copied, and listed in
`docs/improvements.md` for the owner to decide on at the end.

**General rule (approved by the owner in chat, 2026-09-30):** where upstream reads or writes
memory it doesn't own (a crash, or corrupted data), the port returns an error instead. Each
case is a `U-` entry in `docs/improvements.md`, where an entry can make an exception (U-1).
The first case is D-2: image layouts that reach outside their buffer.

| Id | Upstream behavior | Port behavior | Affects outputs? | Approved |
|---|---|---|---|---|
| D-1 | iostream and yaml-cpp number formatting use the process's C++ global locale (`std::locale::global`) | Always formats with the classic "C" locale | Only in a C++ host that changes the global C++ locale. Python and Rust hosts can't: Python's `locale.setlocale` changes the C locale, not the C++ one, and the wheel always formats with "C" | Approved by the owner in chat, 2026-09-29 |
| D-2 | An image description holds pointers and strides and is never checked against a buffer (`src/OpenColorIO/ImageDesc.cpp`); a layout that reaches outside its memory makes `CPUProcessor::apply` read or write outside it, which is undefined behaviour. The Python binding checks only a buffer's entry count, not the strides | After upstream's checks, the constructors refuse a layout that would make the CPU engine touch a byte outside its memory: "PackedImageDesc Error: The strides and dimensions reach outside the image buffer." (and "PlanarImageDesc Error: ..."). The check covers exactly the bytes the engine touches: whole rows of `4 * width` channels for an RGBA-packed image, each channel at `start + x * x_stride + y * y_stride` otherwise (`crates/ocio-ops/src/image_desc.rs`, `reaches_outside`) | Only for layouts whose use is undefined behaviour upstream. Every layout whose pixels are all inside its memory is accepted and processed as upstream does | Approved by the owner, 2026-09-30 (the image-description API proposal) |
