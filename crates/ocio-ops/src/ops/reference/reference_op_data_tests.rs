// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/reference/ReferenceOpData_tests.cpp` @ v2.5.2: the accessors. The
//! other tests load CTF files that reference others, which needs the file transform and the
//! CTF reader (Phase 4).

use super::*;
use crate::op_data::{OpData, OpDataType};

/// Port of `OCIO_ADD_TEST(Reference, accessors)` @ v2.5.2.
#[test]
fn accessors() {
    let mut r = ReferenceOpData::new();

    assert_eq!(r.get_reference_style(), ReferenceStyle::Path);
    assert_eq!(r.get_path(), b"");

    let alias = b"Alias";
    r.set_alias(alias);
    assert_eq!(r.get_reference_style(), ReferenceStyle::Alias);
    assert_eq!(r.get_alias(), alias);

    let file = b"TestPath.txt";
    r.set_path(file);
    assert_eq!(r.get_reference_style(), ReferenceStyle::Path);
    assert_eq!(r.get_path(), file);
}

#[test]
fn equality_compares_the_style_the_direction_and_what_the_style_names() {
    let mut a = ReferenceOpData::new();
    let mut b = ReferenceOpData::new();
    assert!(a == b);

    a.set_path(b"a.ctf");
    assert!(a != b);
    b.set_path(b"a.ctf");
    assert!(a == b);

    // A path reference ignores the alias.
    a.set_alias(b"alias");
    a.set_path(b"a.ctf");
    assert!(a == b);

    b.set_direction(TransformDirection::Inverse);
    assert!(a != b);
    a.set_direction(TransformDirection::Inverse);
    assert!(a == b);

    // An alias reference ignores the path.
    a.set_alias(b"x");
    b.set_alias(b"x");
    b.set_path(b"b.ctf");
    b.set_alias(b"x");
    assert!(a == b);
    b.set_alias(b"y");
    assert!(a != b);

    // Different styles.
    let mut c = a.clone();
    c.set_path(a.get_path());
    assert!(a != c);
}

#[test]
fn as_op_data() {
    let mut reference = ReferenceOpData::new();
    reference.set_path(b"other.ctf");
    let data = OpData::Reference(reference.clone());
    assert_eq!(data.get_type(), OpDataType::Reference);
    assert!(data.validate().is_ok());
    assert!(!data.is_no_op().unwrap());
    assert!(!data.is_identity().unwrap());
    assert!(data.has_channel_crosstalk());
    assert!(data.get_cache_id().is_err());

    let mut ops = crate::op_data::OpDataVec::new();
    data.get_simpler_replacement(&mut ops).unwrap();
    assert!(ops.is_empty());

    // Equal data of the same type only.
    assert!(data == OpData::Reference(reference.clone()));
    let mut other = reference.clone();
    other.set_path(b"else.ctf");
    assert!(data != OpData::Reference(other));
    let no_op = OpData::NoOp(crate::ops::noop::NoOpData::new(
        crate::ops::noop::NoOpKind::Look(b"look".to_vec()),
    ));
    assert!(data != no_op);
    assert!(no_op != data);

    // The id and name live in the metadata.
    let mut data = data;
    data.set_id(b"id");
    assert_eq!(data.get_id(), b"id");
    assert_eq!(data.get_format_metadata().get_id(), b"id");
}
