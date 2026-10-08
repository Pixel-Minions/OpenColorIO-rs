// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the built-in transform registry:
//! `tests/cpu/transforms/builtins/BuiltinTransformRegistry_tests.cpp` @ v2.5.2. `aces` builds
//! the ops of three entries and `read_write` every entry's processor: they come with the
//! builders (WP 3.2e-g, `p3-after-p2`); the `version_*_validation` tests with the config's
//! validation (WP 3.8c). The registry's styles are compared with the wheel's in
//! `tests/config_transforms_oracle.rs`.

use super::*;
use ocio_ops::platform::strcasecmp;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(Builtins, basic)` @ v2.5.2.
#[test]
fn basic() {
    // Create an empty built-in transform registry.

    let mut registry = BuiltinTransformRegistry::new();
    assert_eq!(registry.num_builtins(), 0);
    check_throw_what(registry.builtin_style(0), "Invalid index.");

    let mut ops = OpVec::new();
    check_throw_what(registry.create_ops(0, &mut ops), "Invalid index.");

    // Add a built-in transform.

    let empty_functor: OpCreator = Arc::new(|_ops: &mut OpVec| Ok(()));
    registry.add_builtin(b"trans1", None, empty_functor.clone());

    assert_eq!(registry.num_builtins(), 1);
    assert!(strcasecmp(registry.builtin_style(0).unwrap(), "trans1").is_eq());

    // Add an existing built-in transform i.e. replace the existing one.

    registry.add_builtin(b"trans1", None, empty_functor);
    assert_eq!(registry.num_builtins(), 1);
    assert!(strcasecmp(registry.builtin_style(0).unwrap(), "trans1").is_eq());

    assert!(registry.create_ops(0, &mut ops).is_ok());
}

/// A style that differs only in case replaces the entry, which takes the new spelling and
/// description; a description stops at its first NUL, and a null one is empty.
#[test]
fn replacing_ignores_case() {
    let mut registry = BuiltinTransformRegistry::new();
    let creator: OpCreator = Arc::new(|_ops: &mut OpVec| Ok(()));
    registry.add_builtin(b"Trans1", Some(b"first"), creator.clone());
    registry.add_builtin(b"other", Some(b"two\0three"), creator.clone());
    registry.add_builtin(b"tRANS1", None, creator);
    assert_eq!(registry.num_builtins(), 2);
    assert_eq!(registry.builtin_style(0).unwrap(), b"tRANS1");
    assert_eq!(registry.builtin_description(0).unwrap(), b"");
    assert_eq!(registry.builtin_description(1).unwrap(), b"two");
    check_throw_what(registry.builtin_description(2), "Invalid index.");
}

/// The global registry's ops: an index past its entries is upstream's error, and an entry whose
/// ops are not ported yet (the last one, Rec.2100 HLG, a fixed function of `p3-after-p2`) says
/// so, in both directions.
#[test]
fn ops_of_the_global_registry() {
    let registry = BuiltinTransformRegistry::get();
    let last = registry.num_builtins() - 1;
    let mut ops = OpVec::new();
    for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
        check_throw_what(
            create_builtin_transform_ops(&mut ops, registry.num_builtins(), dir),
            "Invalid built-in transform name.",
        );
        check_throw_what(
            create_builtin_transform_ops(&mut ops, last, dir),
            concat!(
                "BuiltinTransform: the ops of 'DISPLAY - CIE-XYZ-D65_to_REC.2100-HLG-1000nit' ",
                "are not ported yet."
            ),
        );
    }
    assert!(ops.is_empty());
}
