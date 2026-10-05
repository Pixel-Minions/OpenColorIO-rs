// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of `parse_utils.rs`: the ported tests of `tests/cpu/ParseUtils_tests.cpp` @ v2.5.2 that
//! cover what is ported so far. The XML, int, float and string-vector tests come with the file
//! formats (Phase 4).

use super::*;
use crate::open_color_types::{
    bit_depth_to_string, combine_transform_directions, get_inverse_transform_direction,
    transform_direction_to_string,
};
use ocio_testkit::upstream::{check_equal, check_throw_what};

/// Port of `OCIO_ADD_TEST(ParseUtils, bool_string)` @ v2.5.2.
#[test]
fn bool_string() {
    let res_str = bool_to_string(true);
    check_equal("true", res_str);

    let res_str = bool_to_string(false);
    check_equal("false", res_str);

    for (s, expected) in [
        ("yes", true),
        ("Yes", true),
        ("YES", true),
        ("YeS", true),
        ("yEs", true),
        ("true", true),
        ("TRUE", true),
        ("True", true),
        ("tRUe", true),
        ("tRUE", true),
        ("yes ", false),
        (" true ", false),
        ("false", false),
        ("", false),
        ("no", false),
        ("valid", false),
        ("success", false),
        ("anything", false),
    ] {
        let res_bool = bool_from_string(Some(s.as_bytes()));
        check_equal(expected, res_bool);
    }
}

/// Port of `OCIO_ADD_TEST(ParseUtils, transform_direction)` @ v2.5.2.
#[test]
fn transform_direction() {
    let res_str = transform_direction_to_string(TransformDirection::Forward);
    check_equal("forward", res_str);
    let res_str = transform_direction_to_string(TransformDirection::Inverse);
    check_equal("inverse", res_str);

    for (s, expected) in [
        ("forward", TransformDirection::Forward),
        ("inverse", TransformDirection::Inverse),
        ("Forward", TransformDirection::Forward),
        ("Inverse", TransformDirection::Inverse),
        ("FORWARD", TransformDirection::Forward),
        ("INVERSE", TransformDirection::Inverse),
    ] {
        let res_dir = transform_direction_from_string(Some(s.as_bytes())).expect("no throw");
        check_equal(expected, res_dir);
    }
    check_throw_what(
        transform_direction_from_string(Some(b"unknown")),
        "Unrecognized transform direction: 'unknown'",
    );
    check_throw_what(
        transform_direction_from_string(Some(b"")),
        "Unrecognized transform direction: ''",
    );
    check_throw_what(
        transform_direction_from_string(Some(b"anything")),
        "Unrecognized transform direction: 'anything'",
    );
    check_throw_what(
        transform_direction_from_string(None),
        "Unrecognized transform direction: ''",
    );

    let res_dir =
        combine_transform_directions(TransformDirection::Inverse, TransformDirection::Inverse);
    check_equal(TransformDirection::Forward, res_dir);
    let res_dir =
        combine_transform_directions(TransformDirection::Forward, TransformDirection::Forward);
    check_equal(TransformDirection::Forward, res_dir);
    let res_dir =
        combine_transform_directions(TransformDirection::Inverse, TransformDirection::Forward);
    check_equal(TransformDirection::Inverse, res_dir);
    let res_dir =
        combine_transform_directions(TransformDirection::Forward, TransformDirection::Inverse);
    check_equal(TransformDirection::Inverse, res_dir);

    let res_dir = get_inverse_transform_direction(TransformDirection::Inverse);
    check_equal(TransformDirection::Forward, res_dir);
    let res_dir = get_inverse_transform_direction(TransformDirection::Forward);
    check_equal(TransformDirection::Inverse, res_dir);
}

/// Port of `OCIO_ADD_TEST(ParseUtils, bitdepth)` @ v2.5.2.
#[test]
fn bitdepth() {
    for (bd, expected) in [
        (BitDepth::Uint8, "8ui"),
        (BitDepth::Uint10, "10ui"),
        (BitDepth::Uint12, "12ui"),
        (BitDepth::Uint14, "14ui"),
        (BitDepth::Uint16, "16ui"),
        (BitDepth::Uint32, "32ui"),
        (BitDepth::F16, "16f"),
        (BitDepth::F32, "32f"),
        (BitDepth::Unknown, "unknown"),
    ] {
        let res_str = bit_depth_to_string(bd);
        check_equal(expected, res_str);
    }

    for (s, expected) in [
        ("8ui", BitDepth::Uint8),
        ("8Ui", BitDepth::Uint8),
        ("8UI", BitDepth::Uint8),
        ("8uI", BitDepth::Uint8),
        ("10ui", BitDepth::Uint10),
        ("12ui", BitDepth::Uint12),
        ("14ui", BitDepth::Uint14),
        ("16ui", BitDepth::Uint16),
        ("32ui", BitDepth::Uint32),
        ("16f", BitDepth::F16),
        ("32f", BitDepth::F32),
        ("7ui", BitDepth::Unknown),
        ("unknown", BitDepth::Unknown),
        ("", BitDepth::Unknown),
    ] {
        let res_bd = bit_depth_from_string(Some(s.as_bytes()));
        check_equal(expected, res_bd);
    }

    for (bd, expected) in [
        (BitDepth::F16, true),
        (BitDepth::F32, true),
        (BitDepth::Uint8, false),
        (BitDepth::Uint10, false),
        (BitDepth::Uint12, false),
        (BitDepth::Uint14, false),
        (BitDepth::Uint16, false),
        (BitDepth::Uint32, false),
        (BitDepth::Unknown, false),
    ] {
        let res_bool = bit_depth_is_float(bd);
        check_equal(expected, res_bool);
    }

    for (bd, expected) in [
        (BitDepth::Uint8, 8),
        (BitDepth::Uint10, 10),
        (BitDepth::Uint12, 12),
        (BitDepth::Uint14, 14),
        (BitDepth::Uint16, 16),
        (BitDepth::Uint32, 32),
        (BitDepth::F16, 0),
        (BitDepth::F32, 0),
        (BitDepth::Unknown, 0),
    ] {
        let res_int = bit_depth_to_int(bd);
        check_equal(expected, res_int);
    }
}

fn vec_of(items: &[&str]) -> StringVec {
    items.iter().map(|s| s.as_bytes().to_vec()).collect()
}

/// Port of `OCIO_ADD_TEST(ParseUtils, split_string_env_style)` @ v2.5.2.
#[test]
fn split_string_env_style_test() {
    // For look parsing, the split needs to always return a result, even if empty.
    let outputvec = split_string_env_style(b"").unwrap();
    check_equal(1, outputvec.len());

    let outputvec = split_string_env_style(b"This:is:a:test").unwrap();
    check_equal(vec_of(&["This", "is", "a", "test"]), outputvec);

    let outputvec = split_string_env_style(b"   \"This\"  : is   :   a:   test  ").unwrap();
    check_equal(vec_of(&["This", "is", "a", "test"]), outputvec);

    let outputvec = split_string_env_style(b"   This  , is   ,   a,   test  ").unwrap();
    check_equal(vec_of(&["This", "is", "a", "test"]), outputvec);

    let outputvec = split_string_env_style(b"This:is   ,   a:test  ").unwrap();
    check_equal(vec_of(&["This:is", "a:test"]), outputvec);

    let outputvec = split_string_env_style(b",,").unwrap();
    check_equal(vec_of(&["", "", ""]), outputvec);

    let outputvec = split_string_env_style(b"   \"This  : is   \":   a:   test  ").unwrap();
    check_equal(vec_of(&["This  : is   ", "a", "test"]), outputvec);

    check_throw_what(
        split_string_env_style(b"   This  : is   \":   a:   test  "),
        "The string 'This  : is   \":   a:   test' is not correctly formatted. It is missing a \
         closing quote.",
    );

    check_throw_what(
        split_string_env_style(b"   This  : is   :   a:   test  \""),
        "The string 'This  : is   :   a:   test  \"' is not correctly formatted. It is missing a \
         closing quote.",
    );

    let outputvec = split_string_env_style(b"   This  : is   \":   a:   test  \"").unwrap();
    check_equal(vec_of(&["This", "is   \":   a:   test  \""]), outputvec);

    let outputvec = split_string_env_style(b"   \"This  : is   \",   a,   test  ").unwrap();
    check_equal(vec_of(&["This  : is   ", "a", "test"]), outputvec);

    // If the string contains a comma, it is chosen as the separator character rather than
    // the colon (even if it is within quotes and therefore not used as such).
    let outputvec = split_string_env_style(b"   \"This  , is   \":   a:   test  ").unwrap();
    check_equal(vec_of(&["\"This  , is   \":   a:   test"]), outputvec);

    let outputvec = split_string_env_style(b"   \"This  , is   \":   a,   test  ").unwrap();
    check_equal(vec_of(&["\"This  , is   \":   a", "test"]), outputvec);
}

/// Port of `OCIO_ADD_TEST(ParseUtils, join_string_env_style)` @ v2.5.2.
#[test]
fn join_string_env_style_test() {
    let outputvec = vec_of(&["This", "is", "a", "test"]);
    check_equal(
        b"This, is, a, test".to_vec(),
        join_string_env_style(&outputvec),
    );

    check_equal(Vec::<u8>::new(), join_string_env_style(&[]));

    let outputvec = vec_of(&["This:is", "a:test"]);
    check_equal(
        b"\"This:is\", \"a:test\"".to_vec(),
        join_string_env_style(&outputvec),
    );

    let outputvec = vec_of(&["", "", ""]);
    check_equal(b", , ".to_vec(), join_string_env_style(&outputvec));

    let outputvec = vec_of(&["This  : is", "a: test"]);
    check_equal(
        b"\"This  : is\", \"a: test\"".to_vec(),
        join_string_env_style(&outputvec),
    );

    let outputvec = vec_of(&["This", "is   \":   a:   test"]);
    check_equal(
        b"This, \"is   \":   a:   test\"".to_vec(),
        join_string_env_style(&outputvec),
    );

    let outputvec = vec_of(&["\"This, is, a, string\"", "this, one, too"]);
    check_equal(
        b"\"This, is, a, string\", \"this, one, too\"".to_vec(),
        join_string_env_style(&outputvec),
    );

    let outputvec = vec_of(&[
        "This",
        "is: ",
        "\"a very good,\"",
        " fine, helpful, and useful ",
        "test",
    ]);
    check_equal(
        b"This, \"is: \", \"a very good,\", \" fine, helpful, and useful \", test".to_vec(),
        join_string_env_style(&outputvec),
    );
}

/// Port of `OCIO_ADD_TEST(ParseUtils, intersect_string_vecs_case_ignore)` @ v2.5.2.
#[test]
fn intersect_string_vecs_case_ignore_test() {
    let source1 = vec_of(&["111", "This", "is", "222", "a", "test"]);
    let source2 = vec_of(&["333", "TesT", "this", "444", "a", "IS"]);

    let res_inter = intersect_string_vecs_case_ignore(&source1, &source2);
    check_equal(vec_of(&["This", "is", "a", "test"]), res_inter);
}
