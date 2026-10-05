// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of `context.rs`: the ported tests of `tests/cpu/Context_tests.cpp` @ v2.5.2.
//! `OCIO_SOURCE_DIR` is the upstream checkout, `upstream/OpenColorIO`.

use super::*;
use ocio_testkit::paths::upstream_dir;

/// `ociodir`: the upstream source directory, as a plain path: without Windows' `\\?\`
/// prefix, whose `?` `_wstat` refuses as a wildcard, as the port does.
fn ociodir() -> Vec<u8> {
    let dir = upstream_dir();
    let dir = dir.to_str().expect("a UTF-8 path");
    dir.strip_prefix(r"\\?\").unwrap_or(dir).as_bytes().to_vec()
}

/// Port of `SanitizePath` (Context_tests.cpp:24-28 @ v2.5.2): `normpath`.
fn sanitize_path(path: &[u8]) -> Vec<u8> {
    os_path::normpath(path)
}

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
#[test]
fn abs_path() {
    let _lock = crate::path_utils::hash_function_lock();
    let contextpath = cat(&[&ociodir(), b"/src/OpenColorIO/Context.cpp"]);

    let mut con = Context::new();
    con.add_search_path(ociodir());
    con.set_string_var("non_abs", Some(b"src/OpenColorIO/Context.cpp"));
    con.set_string_var("is_abs", Some(&contextpath));

    con.resolve_file_location("${non_abs}").expect("no throw");

    assert_eq!(
        sanitize_path(&con.resolve_file_location("${non_abs}").unwrap()),
        sanitize_path(&contextpath)
    );

    con.resolve_file_location("${is_abs}").expect("no throw");
    assert_eq!(
        con.resolve_file_location("${is_abs}").unwrap(),
        sanitize_path(&contextpath)
    );
}

/// Port of `OCIO_ADD_TEST(Context, var_search_path)` @ v2.5.2.
#[test]
fn var_search_path() {
    let _lock = crate::path_utils::hash_function_lock();
    let mut context = Context::new();
    let contextpath = cat(&[&ociodir(), b"/src/OpenColorIO/Context.cpp"]);

    context.set_string_var("SOURCE_DIR", Some(&ociodir()));
    context.add_search_path("${SOURCE_DIR}/src/OpenColorIO");

    let resolved_source = context
        .resolve_file_location("Context.cpp")
        .expect("no throw");
    assert_eq!(sanitize_path(&resolved_source), sanitize_path(&contextpath));
}

/// Port of `OCIO_ADD_TEST(Context, use_searchpaths)` @ v2.5.2.
#[test]
fn use_searchpaths() {
    let _lock = crate::path_utils::hash_function_lock();
    let mut context = Context::new();

    // Add 2 absolute search paths.
    let search_path1 = cat(&[&ociodir(), b"/src/OpenColorIO"]);
    let search_path2 = cat(&[&ociodir(), b"/tests/gpu"]);
    context.add_search_path(&search_path1);
    context.add_search_path(&search_path2);

    let resolved_source = context
        .resolve_file_location("Context.cpp")
        .expect("no throw");
    let res1 = cat(&[&search_path1, b"/Context.cpp"]);
    assert_eq!(sanitize_path(&resolved_source), sanitize_path(&res1));
    let resolved_source = context
        .resolve_file_location("GPUUnitTest.h")
        .expect("no throw");
    let res2 = cat(&[&search_path2, b"/GPUUnitTest.h"]);
    assert_eq!(sanitize_path(&resolved_source), sanitize_path(&res2));
}

/// Port of `OCIO_ADD_TEST(Context, use_searchpaths_workingdir)` @ v2.5.2.
#[test]
fn use_searchpaths_workingdir() {
    let _lock = crate::path_utils::hash_function_lock();
    let mut context = Context::new();

    // Set working directory and add 2 relative search paths.
    let search_path1: &[u8] = b"src/OpenColorIO";
    let search_path2: &[u8] = b"tests/gpu";
    context.set_working_dir(ociodir());
    context.add_search_path(search_path1);
    context.add_search_path(search_path2);

    let resolved_source = context
        .resolve_file_location("Context.cpp")
        .expect("no throw");
    let res1 = cat(&[&ociodir(), b"/", search_path1, b"/Context.cpp"]);
    assert_eq!(sanitize_path(&resolved_source), sanitize_path(&res1));
    let resolved_source = context
        .resolve_file_location("GPUUnitTest.h")
        .expect("no throw");
    let res2 = cat(&[&ociodir(), b"/", search_path2, b"/GPUUnitTest.h"]);
    assert_eq!(sanitize_path(&resolved_source), sanitize_path(&res2));
}

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
