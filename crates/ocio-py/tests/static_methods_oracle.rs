// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `staticmethod` objects `def_static` puts in a class's dictionary, against the wheel's:
//! pybind11 makes them with `PyStaticMethod_New`, which copies nothing of the function into
//! the `staticmethod` (`staticmethod(f)` from Python copies `__doc__`, `__name__`, `__module__`
//! and `__qualname__` into its `__dict__`). For each static method of the bound classes, as
//! the wheel's surface lists them: its `__dict__`, its `__doc__` (the type's), its type, its
//! function's name, `__wrapped__` and `__isabstractmethod__`.
#![cfg(feature = "python")]

mod common;

use common::script::{Script, compare, n, r, ra, s};
use ocio_testkit::oracle_values::bytes;

/// The classes bound so far.
const CLASSES: [&str; 25] = [
    "Transform",
    "GroupTransform",
    "MatrixTransform",
    "RangeTransform",
    "AllocationTransform",
    "ExponentTransform",
    "ExponentWithLinearTransform",
    "LogTransform",
    "LogAffineTransform",
    "LogCameraTransform",
    "FixedFunctionTransform",
    "BuiltinTransform",
    "BuiltinTransformRegistry",
    "CDLTransform",
    "FileTransform",
    "FormatMetadata",
    "Context",
    "ProcessorMetadata",
    "FileRules",
    "ViewingRules",
    "ColorSpace",
    "ColorSpaceSet",
    "Look",
    "NamedTransform",
    "ViewTransform",
];

#[test]
fn static_methods_are_the_wheels() {
    let report = common::wheel_surface();
    let mut sc = Script::new();
    let mut count = 0;
    for c in CLASSES {
        let class = common::wheel_member(&report, c);
        for m in class["members"].as_array().unwrap() {
            if m["kind"] != "staticmethod" {
                continue;
            }
            let name = String::from_utf8(bytes(&m["name"])).unwrap();
            let sm = sc.call(n(&format!("OCIO.{c}.__dict__.get")), vec![s(&name)]);
            sc.call(n("type"), vec![r(sm)]);
            sc.eval(ra(sm, "__dict__"), &[]);
            sc.eval(ra(sm, "__doc__"), &[]);
            sc.eval(ra(sm, "__func__.__name__"), &[]);
            sc.eval(ra(sm, "__isabstractmethod__"), &[]);
            sc.call(
                n("operator.is_"),
                vec![ra(sm, "__wrapped__"), ra(sm, "__func__")],
            );
            count += 1;
        }
    }
    // CDLTransform (2), FileTransform (2), GroupTransform, MatrixTransform (5) and
    // NamedTransform, as the wheel's surface lists them.
    assert_eq!(count, 11);
    compare(&[sc]);
}
