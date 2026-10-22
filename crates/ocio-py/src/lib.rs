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
//!
//! The binding has no `unsafe` code (the crate forbids it, which nothing inside it can relax):
//! the C API calls it needs that no safe API makes are the safe functions of `ocio-py-ffi`
//! (`crates/ocio-py/ffi`), whose one file `cargo xtask guards` pins.
#![forbid(unsafe_code)]

#[cfg(feature = "python")]
pub(crate) mod callback;
#[cfg(feature = "python")]
pub mod dispatch;
#[cfg(feature = "python")]
pub mod embed;
#[cfg(feature = "python")]
pub mod enums;
#[cfg(feature = "python")]
pub(crate) mod py_builtin_config_registry;
#[cfg(feature = "python")]
pub(crate) mod py_builtin_transform_registry;
#[cfg(feature = "python")]
pub(crate) mod py_color_space;
#[cfg(feature = "python")]
pub(crate) mod py_color_space_set;
#[cfg(feature = "python")]
pub(crate) mod py_config;
#[cfg(feature = "python")]
pub(crate) mod py_config_io_proxy;
#[cfg(feature = "python")]
pub(crate) mod py_context;
#[cfg(feature = "python")]
pub(crate) mod py_cpu_processor;
#[cfg(feature = "python")]
pub(crate) mod py_dynamic_property;
#[cfg(feature = "python")]
pub(crate) mod py_file_rules;
#[cfg(feature = "python")]
pub(crate) mod py_format_metadata;
#[cfg(feature = "python")]
pub(crate) mod py_gpu_shader_creator;
#[cfg(feature = "python")]
pub(crate) mod py_image_desc;
#[cfg(feature = "python")]
pub(crate) mod py_look;
#[cfg(feature = "python")]
pub(crate) mod py_named_transform;
#[cfg(feature = "python")]
pub mod py_open_color_io;
#[cfg(feature = "python")]
pub(crate) mod py_packed_image_desc;
#[cfg(feature = "python")]
pub(crate) mod py_planar_image_desc;
#[cfg(feature = "python")]
pub(crate) mod py_processor;
#[cfg(feature = "python")]
pub(crate) mod py_processor_metadata;
#[cfg(feature = "python")]
pub(crate) mod py_transform;
/// What the identity registries hold, for the tests of their size and cost.
#[cfg(feature = "python")]
#[doc(hidden)]
pub use py_transform::{RegistryStats, registry_stats};
#[cfg(feature = "python")]
pub(crate) mod py_types;
#[cfg(feature = "python")]
pub(crate) mod py_utils;
#[cfg(feature = "python")]
pub(crate) mod py_view_transform;
#[cfg(feature = "python")]
pub(crate) mod py_viewing_rules;
#[cfg(feature = "python")]
pub mod pybind11;
#[cfg(feature = "python")]
pub(crate) mod shared;
#[cfg(feature = "python")]
pub(crate) mod transforms;
