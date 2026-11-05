# opencolorio-rs

A Rust port of [OpenColorIO](https://github.com/AcademySoftwareFoundation/OpenColorIO) 2.5.2:
configs, transforms, processors and the file formats, with the output of the original checked
byte for byte against the official `opencolorio==2.5.2` wheel, on Windows x86-64 and Rocky
Linux 9.

The package is `opencolorio-rs`; the library is `ocio`:

```sh
cargo add opencolorio-rs
```

```rust
let version = ocio::version(); // "2.5.2", the OpenColorIO version this release matches
```

`ocio::PORT_VERSION` is this crate's own version; each version's build metadata states the
OpenColorIO version it matches.

Status, how the port is checked and the rest of the project (the `PyOpenColorIO`-compatible
Python module, the tools): <https://github.com/Pixel-Minions/OpenColorIO-rs>.

BSD-3-Clause, as OpenColorIO, for this project's own code, with the code it ports under its own
licenses: yaml-cpp (MIT), minizip-ng (Zlib) and SampleICC, which is under the ICC Software
License, Version 0.2. That license has no SPDX identifier, so the crate's `license` field,
`BSD-3-Clause AND MIT AND Zlib`, can't name it. See `LICENSE`, `NOTICE` and
`THIRD-PARTY-NOTICES.md`, which has every notice in full.

This is an independent project. It isn't the official OpenColorIO package, and it isn't affiliated with or endorsed by the OpenColorIO project or the Academy Software Foundation.
