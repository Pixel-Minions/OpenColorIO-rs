# Moving from PyOpenColorIO to the `opencolorio-rs` wheel

The `opencolorio-rs` wheel installs the same Python module as the official `opencolorio` wheel,
under the same import name, with the port's Rust code behind it. It matches OpenColorIO 2.5.2:
upstream's 384 Python tests run unmodified against it, installed, on both platforms. Nothing is
published yet; this page describes 1.0.0.

## What stays the same

- **The import name.** Scripts keep `import PyOpenColorIO as OCIO`, and need no change.
- **The version.** `PyOpenColorIO.__version__` is `"2.5.2"`, the OpenColorIO release the port
  matches. The wheel's own version, `1.0.0`, is the port's.
- **The results.** Pixels, text output, errors and cache IDs are those of the official wheel on
  the same machine, except where `docs/deviations.md` and `waivers.toml` list a difference.

## What changes

- **One package or the other.** Both wheels install `PyOpenColorIO`, so they can't share a Python
  environment. Remove `opencolorio` first:

  ```sh
  pip uninstall opencolorio
  pip install opencolorio-rs
  ```

- **Python 3.13 only** (`cp313`), on Windows x86-64 and on Linux x86-64. There is no wheel for
  other Python versions, macOS or ARM.
- **glibc 2.34 or newer on Linux.** The wheel is `manylinux_2_34`, built on Rocky Linux 9. It
  doesn't install on older systems, such as Rocky Linux 8, which the official wheels support.
- **Four tools.** The wheel ships `ociocheck`, `ociochecklut`, `ociowrite` and `ociobakelut`, as
  console scripts and in `PyOpenColorIO/bin`, with the official wheel's entry points. The official
  wheel's other tools aren't in it: `ocioarchive`, `ocioconvert`, `ociocpuinfo`, `ociodisplay`,
  `ociolutimage`, `ociomakeclf`, `ociomergeconfigs`, `ocioperf`, `pyocioamf` and
  `pyociodisplay`.
- **No native C++ interop.** The official module's classes hold C++ objects that other C++
  extensions can exchange with it. The port's hold Rust objects and don't offer that exchange
  (deviation D-16). Pure Python use isn't affected.

## Embedding the module in a Rust application

A Rust application that runs Python in its own process can register the module itself, so that
its Python code imports `PyOpenColorIO` from the application and shares the process's OCIO state
with the Rust side: caches, the logging level, environment variables and the current config.

The embedding API is `ocio_py::embed`, in the `ocio-py` crate with its `python` feature. That crate
isn't published on crates.io in 1.0.0, so it comes from the repository as a path dependency.

```rust
// Before the interpreter starts.
ocio_py::embed::append_to_inittab();
pyo3::Python::initialize();
// Python code can now `import PyOpenColorIO`.
```

`append_to_inittab` panics if an interpreter already runs in the process. The module docs of
`crates/ocio-py/src/embed.rs` say how the current config is shared, and the conversions of
processors and transforms between Rust and Python. A config passed with `config_to_python` or
`config_from_python` is a copy, not shared (deviation D-11 covers how the host reads the current
config while Python changes it). `crates/ocio-py/tests/embed.rs` is a working example.
