# Third-party notices

This package is part of OpenColorIO-rs, a port of OpenColorIO 2.5.2 under BSD-3-Clause (see
[`LICENSE`](LICENSE) and [`NOTICE`](NOTICE)). Its sources also translate code from the projects
below. The sections are those of the repository's `THIRD-PARTY-NOTICES.md`, which lists every
project the port takes code from. The crates this package depends on ship their own licenses.

## expat 2.7.2

<https://github.com/libexpat/libexpat>, the XML parser OpenColorIO 2.5.2 is built with (both
wheels bundle it). The parts OpenColorIO's readers reach are ported in
`crates/ocio-formats/src/expat/`, together with its own tests. Its source is pinned as the
submodule `upstream/expat` (tag R_2_7_2, commit 7643f96bd5b9f5d3b26ea112519e05e17ef91cfb), so
that the citations can be checked. Its `COPYING`:

```text
Copyright (c) 1998-2000 Thai Open Source Software Center Ltd and Clark Cooper
Copyright (c) 2001-2025 Expat maintainers

Permission is hereby granted, free of charge, to any person obtaining
a copy of this software and associated documentation files (the
"Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so, subject to
the following conditions:

The above copyright notice and this permission notice shall be included
in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```
