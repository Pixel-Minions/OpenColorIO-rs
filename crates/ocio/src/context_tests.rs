// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of `context.rs`: the ported tests of `tests/cpu/Context_tests.cpp` @ v2.5.2.
//! So far the tests that don't look for files.

use super::*;

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

/// Port of `OCIO_ADD_TEST(Context, search_paths)` @ v2.5.2.
#[test]
fn search_paths() {
    let mut con = Context::new();
    assert_eq!(con.num_search_paths(), 0);
    let empty: &[u8] = b"";
    assert_eq!(con.search_path(), empty);
    assert_eq!(con.search_path_with_index(42), empty);

    con.add_search_path(empty);
    assert_eq!(con.num_search_paths(), 0);

    let first: &[u8] = b"First";
    con.add_search_path(first);
    assert_eq!(con.num_search_paths(), 1);
    assert_eq!(con.search_path(), first);
    assert_eq!(con.search_path_with_index(0), first);
    con.clear_search_paths();
    assert_eq!(con.num_search_paths(), 0);
    assert_eq!(con.search_path(), empty);

    let second: &[u8] = b"Second";
    let first_second = cat(&[first, b":", second]);
    con.add_search_path(first);
    con.add_search_path(second);
    assert_eq!(con.num_search_paths(), 2);
    assert_eq!(con.search_path(), first_second);
    assert_eq!(con.search_path_with_index(0), first);
    assert_eq!(con.search_path_with_index(1), second);
    con.add_search_path(empty);
    assert_eq!(con.num_search_paths(), 2);

    con.set_search_path(first);
    assert_eq!(con.num_search_paths(), 1);
    assert_eq!(con.search_path(), first);
    assert_eq!(con.search_path_with_index(0), first);

    con.set_search_path(&first_second);
    assert_eq!(con.num_search_paths(), 2);
    assert_eq!(con.search_path(), first_second);
    assert_eq!(con.search_path_with_index(0), first);
    assert_eq!(con.search_path_with_index(1), second);
}

/// Port of `OCIO_ADD_TEST(Context, abs_path)` @ v2.5.2.
/// Port of `OCIO_ADD_TEST(Context, string_vars)` @ v2.5.2.
#[test]
fn string_vars() {
    // Test Context::addStringVars().

    let mut ctx1 = Context::new();
    ctx1.set_string_var("var1", Some(b"val1"));
    ctx1.set_string_var("var2", Some(b"val2"));

    let mut ctx2 = Context::new();
    ctx2.set_string_var("var1", Some(b"val11"));
    ctx2.set_string_var("var3", Some(b"val3"));

    let const_ctx2 = &ctx2;
    ctx1.add_string_vars(const_ctx2);
    assert_eq!(3, ctx1.num_string_vars());

    assert_eq!(b"var1", ctx1.string_var_name_by_index(0));
    assert_eq!(b"val11", ctx1.string_var_by_index(0));

    assert_eq!(b"var2", ctx1.string_var_name_by_index(1));
    assert_eq!(b"val2", ctx1.string_var_by_index(1));

    assert_eq!(b"var3", ctx1.string_var_name_by_index(2));
    assert_eq!(b"val3", ctx1.string_var_by_index(2));
}
