// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the op data model. Upstream's one test of it, `OpData equality`
//! (tests/cpu/Op_tests.cpp:275-314 @ v2.5.2), needs the Matrix and Range families.

use super::*;
use crate::ops::noop::{FileNoOpData, NoOpData, NoOpKind};

const ALL_TYPES: [OpDataType; 16] = [
    OpDataType::Cdl,
    OpDataType::Exponent,
    OpDataType::ExposureContrast,
    OpDataType::FixedFunction,
    OpDataType::Gamma,
    OpDataType::GradingPrimary,
    OpDataType::GradingRgbCurve,
    OpDataType::GradingHueCurve,
    OpDataType::GradingTone,
    OpDataType::Log,
    OpDataType::Lut1D,
    OpDataType::Lut3D,
    OpDataType::Matrix,
    OpDataType::Range,
    OpDataType::Reference,
    OpDataType::NoOp,
];

fn file_data(path: &[u8]) -> OpData {
    OpData::NoOp(NoOpData::new(NoOpKind::File(FileNoOpData::new(path))))
}

fn look_data(look: &[u8]) -> OpData {
    OpData::NoOp(NoOpData::new(NoOpKind::Look(look.to_vec())))
}

#[test]
fn the_reference_and_no_op_types_have_no_name() {
    for op_type in ALL_TYPES {
        let unnamed = matches!(op_type, OpDataType::Reference | OpDataType::NoOp);
        assert_eq!(get_type_name(op_type).is_err(), unnamed, "{op_type:?}");
    }
    // The names differ.
    let names: Vec<&str> = ALL_TYPES
        .iter()
        .filter_map(|&t| get_type_name(t).ok())
        .collect();
    for (i, a) in names.iter().enumerate() {
        assert!(!a.is_empty());
        assert!(names[i + 1..].iter().all(|b| a != b), "{a} twice");
    }
}

#[test]
fn no_op_data() {
    // NoOpData (src/OpenColorIO/ops/noop/NoOps.h:39-51 @ v2.5.2), for the data of a FileNoOp
    // and of a LookNoOp.
    for data in [file_data(b"file.clf"), look_data(b"look")] {
        assert_eq!(data.get_type(), OpDataType::NoOp);
        assert!(data.validate().is_ok());
        assert!(data.is_no_op());
        assert!(data.is_identity());
        assert!(!data.has_channel_crosstalk());
        assert_eq!(data.get_cache_id().unwrap(), b"");

        let mut ops = OpDataVec::new();
        data.get_simpler_replacement(&mut ops).unwrap();
        assert!(ops.is_empty());
    }
}

#[test]
fn no_op_data_are_equal_whatever_their_class() {
    // NoOpData keeps OpData::equals, which compares the types only.
    let a = file_data(b"a.clf");
    let b = file_data(b"b.clf");
    let c = look_data(b"look");
    assert!(a.equals(&a));
    assert!(a.equals(&b));
    assert!(a.equals(&c));
    assert!(c.equals(&a));
    assert!(a == c);
}

#[test]
fn id_and_name_live_in_the_metadata() {
    let mut data = look_data(b"look");
    assert_eq!(data.get_id(), b"");
    assert_eq!(data.get_name(), b"");

    data.set_id(b"the id");
    data.set_name(b"the name");
    assert_eq!(data.get_id(), b"the id");
    assert_eq!(data.get_name(), b"the name");

    let metadata = data.get_format_metadata();
    assert_eq!(metadata.get_id(), b"the id");
    assert_eq!(metadata.get_name(), b"the name");

    // The setters pass the string on as a C string, which ends at the first NUL.
    data.set_id(b"first\0second");
    assert_eq!(data.get_id(), b"first");

    // The getters find the attributes ignoring case, as the metadata does.
    data.get_format_metadata_mut().clear();
    data.get_format_metadata_mut()
        .add_attribute(Some(b"NAME"), Some(b"upper"))
        .unwrap();
    assert_eq!(data.get_name(), b"upper");
}

#[test]
fn clone_copies_the_metadata() {
    let mut data = look_data(b"look");
    data.set_name(b"name");
    let copy = data.clone();
    assert_eq!(copy.get_name(), b"name");
    assert_eq!(copy.get_format_metadata(), data.get_format_metadata());
}
