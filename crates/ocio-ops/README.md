# opencolorio-rs-ops

The OpenColorIO ops: op data, CPU renderers for each SIMD profile, the optimizer, pixel formats and CPU detection. It is part of
[`opencolorio-rs`](https://github.com/Pixel-Minions/OpenColorIO-rs), a Rust port of OpenColorIO
2.5.2. Applications use that crate; this one is published so that it can depend on it.

This crate is a building block of [`opencolorio-rs`](https://crates.io/crates/opencolorio-rs): use that crate. Its API follows OpenColorIO's internals and may change in minor releases.

The package is `opencolorio-rs-ops`; the library is `ocio_ops`.

BSD-3-Clause, as OpenColorIO. See `LICENSE`, `NOTICE` and `THIRD-PARTY-NOTICES.md`.

This is an independent project. It isn't the official OpenColorIO package, and it isn't affiliated with or endorsed by the OpenColorIO project or the Academy Software Foundation.
