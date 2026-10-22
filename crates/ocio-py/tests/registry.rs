// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The identity registries of the port's module (pybind11's registered instances, which the
//! wheel's deallocation of an object leaves): they hold the objects that live and no others,
//! whatever was made and dropped before (plain objects, a group's children, objects of a Python
//! subclass in a reference cycle, `FormatMetadata` objects), and a lookup reads the entries of
//! its own node only, however many other objects live. (Identity itself, `is`, is compared with
//! the wheel's in the classes' oracle tests.)
//!
//! One test in its own binary: the registries are the process's, so no other test may make or
//! drop objects meanwhile.
#![cfg(feature = "python")]

mod common;

use ocio_py::{RegistryStats, registry_stats};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Runs `code` in `globals`.
fn run(py: Python<'_>, globals: &Bound<'_, PyDict>, code: &str) {
    let code = std::ffi::CString::new(code).unwrap();
    py.run(&code, Some(globals), None)
        .unwrap_or_else(|e| panic!("{e}"));
}

/// The registries after a full collection.
fn stats(py: Python<'_>, globals: &Bound<'_, PyDict>) -> RegistryStats {
    run(py, globals, "gc.collect()");
    registry_stats()
}

#[test]
fn the_registries_hold_the_live_objects_only_and_look_up_by_node() {
    const N: usize = 5000;
    common::start_python();
    Python::attach(|py| {
        let globals = PyDict::new(py);
        run(
            py,
            &globals,
            &format!("import gc\nimport PyOpenColorIO as OCIO\nN = {N}"),
        );
        let base = stats(py, &globals);

        // Objects made and dropped leave nothing.
        run(
            py,
            &globals,
            "for _ in range(4 * N): OCIO.MatrixTransform()\n\
             for _ in range(N): OCIO.RangeTransform().getFormatMetadata()",
        );
        let after = stats(py, &globals);
        assert_eq!(after.transforms, base.transforms);
        assert_eq!(after.format_metadata, base.format_metadata);

        // A group's children: registered when Python gets them, found again by node.
        run(
            py,
            &globals,
            "g = OCIO.GroupTransform([OCIO.MatrixTransform() for _ in range(N)])\n\
             keep = [OCIO.LogTransform() for _ in range(N)]",
        );
        let made = stats(py, &globals);
        assert_eq!(made.transforms, base.transforms + 1 + N);
        run(py, &globals, "kids = list(g)");
        let first = stats(py, &globals);
        assert_eq!(first.transforms, base.transforms + 1 + 2 * N);
        // No child had an object: each lookup read nothing.
        assert_eq!(first.examined, made.examined);
        run(
            py,
            &globals,
            "again = [g[i] for i in range(N)]\n\
             assert all(a is b for a, b in zip(kids, again))",
        );
        let second = stats(py, &globals);
        // Each lookup read its own node's one entry, whatever else lives.
        assert_eq!(second.examined - first.examined, N);
        assert_eq!(second.transforms, first.transforms);

        // FormatMetadata: one object per transform while it lives.
        run(
            py,
            &globals,
            "fms = [kids[0].getFormatMetadata() for _ in range(N)]\n\
             assert all(f is fms[0] for f in fms)",
        );
        let fm = stats(py, &globals);
        assert_eq!(fm.format_metadata, base.format_metadata + 1);
        assert_eq!(fm.examined - second.examined, N - 1);

        // Objects in a reference cycle, which only the collector frees. (Not one through
        // getFormatMetadata's keep-alive, which the collector can't see, and which the wheel
        // never frees either.)
        run(
            py,
            &globals,
            "Sub = type('Sub', (OCIO.MatrixTransform,), {})\n\
             for _ in range(N):\n    s = Sub(); s.me = s; s.fm = s\n\
             del s",
        );
        let cycles = stats(py, &globals);
        assert_eq!(cycles.transforms, second.transforms);
        assert_eq!(cycles.format_metadata, base.format_metadata + 1);

        run(py, &globals, "del g, keep, kids, again, fms, Sub");
        let end = stats(py, &globals);
        assert_eq!(end.transforms, base.transforms);
        assert_eq!(end.format_metadata, base.format_metadata);
    });
}
