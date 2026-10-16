// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/reference/ReferenceOpData_tests.cpp` @ v2.5.2: the tests that load CTF
//! files referencing others, through the file transform. They need this crate (the config, the
//! file transform and the CTF reader), so they live here; the accessors test is in
//! `ocio-ops/src/ops/reference/reference_op_data_tests.rs`.

use std::sync::Arc;

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::test_utils::{build_ops_test, get_test_files_dir};
use crate::processor::Processor;
use crate::transform::Transform;
use crate::transforms::file_transform::FileTransform;
use ocio_ops::exception::Result;
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::noop::no_ops::NoOpKind;
use ocio_testkit::upstream::check_throw_what;

/// Use OCIO public interface to load a file transform. We can't validate what the processor
/// contains.
///
/// Port of `GetTransformFileProcessor` (ReferenceOpData_tests.cpp:37-54 @ v2.5.2).
fn get_transform_file_processor(file_name: &str) -> Result<Arc<Processor>> {
    // (Path will be normalized, so this works even on Windows.)
    let file_path = format!("{}/{file_name}", get_test_files_dir());

    let mut p_file_transform = FileTransform::new();
    p_file_transform.set_src(&file_path);

    let mut p_config = Config::new()?;

    // Use search paths to resolve references.
    p_config.add_search_path(get_test_files_dir().as_bytes());

    p_config.processor(&Transform::File(p_file_transform))
}

/// Port of `OCIO_ADD_TEST(Reference, load_path_resolve_failing)` @ v2.5.2.
#[test]
fn load_path_resolve_failing() {
    let file_name = "reference_path_missing_file.ctf";
    check_throw_what(
        get_transform_file_processor(file_name),
        "could not be located",
    );
}

/// Port of `OCIO_ADD_TEST(Reference, load_multiple_resolve)` @ v2.5.2.
#[test]
fn load_multiple_resolve() {
    let file_name = "references_some_inverted.ctf";
    get_transform_file_processor(file_name).unwrap();
}

/// Port of `OCIO_ADD_TEST(Reference, load_same_twice_resolve)` @ v2.5.2.
#[test]
fn load_same_twice_resolve() {
    let file_name = "references_same_twice.ctf";
    get_transform_file_processor(file_name).unwrap();
}

/// Port of `OCIO_ADD_TEST(Reference, load_nested_resolve)` @ v2.5.2.
#[test]
fn load_nested_resolve() {
    let file_name = "reference_nested.ctf";
    get_transform_file_processor(file_name).unwrap();
}

/// Port of `OCIO_ADD_TEST(Reference, load_cycle_itself_resolve_failing)` @ v2.5.2.
#[test]
fn load_cycle_itself_resolve_failing() {
    let file_name = "reference_cycle_itself.ctf";
    check_throw_what(
        get_transform_file_processor(file_name),
        "is creating a recursion",
    );
}

/// Port of `OCIO_ADD_TEST(Reference, load_cycle_2_resolve_failing)` @ v2.5.2.
#[test]
fn load_cycle_2_resolve_failing() {
    let file_name = "reference_cycle_2levels.ctf";
    check_throw_what(
        get_transform_file_processor(file_name),
        "is creating a recursion",
    );
}

/// Port of `OCIO_ADD_TEST(Reference, load_cycle_3_resolve_failing)` @ v2.5.2.
#[test]
fn load_cycle_3_resolve_failing() {
    let file_name = "reference_cycle_3levels.ctf";
    check_throw_what(
        get_transform_file_processor(file_name),
        "is creating a recursion",
    );
}

/// Port of `OCIO_ADD_TEST(Reference, load_cycle_rel_resolve_failing)` @ v2.5.2.
#[test]
fn load_cycle_rel_resolve_failing() {
    let file_name = "reference_cycle_path_not_equal.ctf";
    check_throw_what(
        get_transform_file_processor(file_name),
        "is creating a recursion",
    );
}

/// The path of a `FileNoOp`'s data, if `op` is one.
///
/// Port of `GetFilePath` (ReferenceOpData_tests.cpp:115-125 @ v2.5.2).
fn get_file_path(path: &mut Vec<u8>, op: &Op) -> bool {
    if let OpData::NoOp(data) = &**op.data()
        && let NoOpKind::File(file_data) = data.kind()
    {
        *path = file_data.get_path().to_vec();
        return true;
    }
    false
}

/// Whether `path` holds `name`.
fn find(path: &[u8], name: &str) -> bool {
    path.windows(name.len()).any(|w| w == name.as_bytes())
}

//
// The following tests load files using the non-public API in order to validate the
// referenced files are loaded correctly.
//

/// The context of the resolution tests: the test files' directory as its search path.
fn test_context() -> Context {
    let mut context = Context::new();
    context.add_search_path(get_test_files_dir().as_bytes());
    context
}

/// Port of `OCIO_ADD_TEST(Reference, load_one_reference)` @ v2.5.2.
#[test]
fn load_one_reference() {
    let context = test_context();
    let file_name = "reference_one_matrix.ctf";
    let mut ops = OpVec::new();
    build_ops_test(&mut ops, file_name, &context, TransformDirection::Forward).unwrap();

    // Ops contains [FileNoOp, FileNoOp, Matrix].
    assert_eq!(ops.len(), 3);

    let op = &ops[2];
    assert!(matches!(&**op.data(), OpData::Matrix(_)));
}

/// Port of `OCIO_ADD_TEST(Reference, load_multiple_resolve_internal)` @ v2.5.2.
#[test]
fn load_multiple_resolve_internal() {
    let context = test_context();
    let file_name = "references_some_inverted.ctf";
    let mut ops = OpVec::new();
    build_ops_test(&mut ops, file_name, &context, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 10);
    let mut path = Vec::new();
    assert!(get_file_path(&mut path, &ops[0]));
    assert!(find(&path, file_name));

    assert!(get_file_path(&mut path, &ops[1]));
    assert!(find(&path, "matrix_example_1_3_offsets.ctf"));

    assert!(matches!(&**ops[2].data(), OpData::Matrix(_)));

    assert!(get_file_path(&mut path, &ops[3]));
    assert!(find(&path, "xyz_to_rgb.clf"));

    let OpData::Lut1D(lut_data) = &**ops[4].data() else {
        panic!("not a Lut1D");
    };
    assert_eq!(lut_data.get_direction(), TransformDirection::Inverse);

    assert!(matches!(&**ops[5].data(), OpData::Range(_)));

    assert!(matches!(&**ops[6].data(), OpData::Matrix(_)));

    assert!(matches!(&**ops[7].data(), OpData::Range(_)));

    assert!(get_file_path(&mut path, &ops[8]));
    assert!(find(&path, "cdl_clamp_fwd.clf"));

    assert!(matches!(&**ops[9].data(), OpData::Cdl(_)));
}

/// Port of `OCIO_ADD_TEST(Reference, load_nested_resolve_internal)` @ v2.5.2.
#[test]
fn load_nested_resolve_internal() {
    let context = test_context();
    let file_name = "reference_nested.ctf";
    let mut ops = OpVec::new();
    build_ops_test(&mut ops, file_name, &context, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 4);

    let mut path = Vec::new();
    assert!(get_file_path(&mut path, &ops[0]));
    assert!(find(&path, "reference_nested.ctf"));

    assert!(get_file_path(&mut path, &ops[1]));
    assert!(find(&path, "reference_nested_2.ctf"));

    assert!(get_file_path(&mut path, &ops[2]));
    assert!(find(&path, "matrix_example.clf"));

    assert!(matches!(&**ops[3].data(), OpData::Matrix(_)));
}
