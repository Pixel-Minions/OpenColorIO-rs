// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `PyOpenColorIO` module (Phase 6). Its bindings follow upstream's pybind11
//! definitions in `src/bindings/python/` @ v2.5.2.
//!
//! The bindings are behind the `python` feature (and `extension-module` for the module that
//! Python imports): `cargo xtask py-build` builds them with PyO3 against the interpreter of
//! `target/py-port` and lays the module out as upstream's package (`PyOpenColorIO/__init__.py`
//! next to the extension `PyOpenColorIO/PyOpenColorIO.*`).
//!
//! A binding's Rust doc comment is its Python `__doc__`, which must be the wheel's (P6-7):
//! pybind11's signature line. Citations of the upstream definitions are therefore `//`
//! comments above each binding.
#![deny(unsafe_code)]

#[cfg(feature = "python")]
pub mod dispatch;
#[cfg(feature = "python")]
pub mod embed;
#[cfg(feature = "python")]
pub mod py_open_color_io;
