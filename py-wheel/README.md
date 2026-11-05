# opencolorio-rs

A Rust port matching [OpenColorIO](https://opencolorio.org/) 2.5.2, with the Python module
`PyOpenColorIO` and the tools `ociocheck`, `ociochecklut`, `ociowrite` and `ociobakelut`.

**This is not the official OpenColorIO package**, and the OpenColorIO project doesn't make it.
The official package is [`opencolorio`](https://pypi.org/project/opencolorio/). Both install the
import name `PyOpenColorIO`, so they can't share a Python environment.

The port is checked byte by byte against the official `opencolorio==2.5.2` wheel, and
`PyOpenColorIO.__version__` is `2.5.2`. Exceptions are listed in the repository's
`docs/deviations.md` and `waivers.toml`. Python 3.13 on Windows x86-64 and Linux x86-64.

License: BSD-3-Clause for this project's own code, with the code it ports and links under their
own licenses, as `License-Expression` lists them; SampleICC's ICC Software License 0.2 has no SPDX
identifier and appears as `LicenseRef-ICC-0.2`. The wheel's `licenses` directory holds this
project's notices, every third-party notice in full, and those of the Rust crates it links.

This is an independent project. It isn't the official OpenColorIO package, and it isn't affiliated with or endorsed by the OpenColorIO project or the Academy Software Foundation.
