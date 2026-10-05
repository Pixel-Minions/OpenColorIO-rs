// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of `context_variable_utils.rs`: the ported tests of
//! `tests/cpu/ContextVariableUtils_tests.cpp` @ v2.5.2. `LoadEnvironment` is checked against the
//! wheel through the context (`tests/context_oracle.rs`).

use super::*;

/// Port of `OCIO_ADD_TEST(ContextVariableUtils, env_check)` @ v2.5.2.
#[test]
fn env_check() {
    // Test the detection of the context variables.
    assert!(!contains_context_variable_token(b"1234"));
    assert!(contains_context_variable_token(b"${1234}"));
    assert!(contains_context_variable_token(b"${1234"));
    assert!(contains_context_variable_token(b"$1234"));
    assert!(!contains_context_variable_token(b"{1234}"));
    assert!(contains_context_variable_token(b"1234%"));
    assert!(contains_context_variable_token(b"12%34"));
    assert!(contains_context_variable_token(b"123%4%"));

    assert!(!contains_context_variables(b"1234"));
    assert!(contains_context_variables(b"${1234}"));
    assert!(!contains_context_variables(b"%1234"));
    assert!(contains_context_variables(b"%1234%"));
    // Test succeeds even if '{1234' is a suspicious name for a context variable.
    assert!(contains_context_variables(b"${1234"));
}

fn insert(map: &mut EnvMap, name: &str, value: &str) {
    map.entry(EnvMapKey(name.as_bytes().to_vec()))
        .or_insert_with(|| value.as_bytes().to_vec());
}

/// Port of `OCIO_ADD_TEST(ContextVariableUtils, env_expand)` @ v2.5.2.
#[test]
fn env_expand() {
    // Test the resolution of the context variables.

    // Build env by hand for unit test.
    let mut env_map = EnvMap::new();

    // Add some fake context variables so the test runs.
    insert(&mut env_map, "TEST1", "foo.bar");
    insert(&mut env_map, "TEST1NG", "bar.foo");
    insert(&mut env_map, "FOO_foo.bar", "cheese");

    // The string to test.
    let foo: &[u8] = b"/a/b/${TEST1}/${TEST1NG}/%TEST1%/$TEST1NG/${FOO_${TEST1}}/";
    let foo_result: &[u8] = b"/a/b/foo.bar/bar.foo/foo.bar/bar.foo/cheese/";

    let mut used_envs = UsedEnvs::new();
    {
        // Resolve the string.
        let testresult = resolve_context_variables(foo, &env_map, &mut used_envs);

        // Check the resulting string.
        assert_eq!(testresult, foo_result);
        assert!(!contains_context_variables(&testresult));

        // Check the used context variables.
        assert_eq!(3, used_envs.len());
        let mut iter = used_envs.iter();
        assert_eq!(
            iter.next(),
            Some((&b"FOO_foo.bar".to_vec(), &b"cheese".to_vec()))
        );
        assert_eq!(
            iter.next(),
            Some((&b"TEST1".to_vec(), &b"foo.bar".to_vec()))
        );
        assert_eq!(
            iter.next(),
            Some((&b"TEST1NG".to_vec(), &b"bar.foo".to_vec()))
        );
    }

    // Now, test some faulty cases.

    env_map.clear();
    insert(&mut env_map, "TEST1", "foo.bar");
    insert(&mut env_map, "TEST1NG", "bar.foo");

    used_envs.clear();
    {
        // That's a right context variable syntax but the env does not contain one of the vars.
        let testresult = resolve_context_variables(foo, &env_map, &mut used_envs);

        // Check the resulting string.
        assert_eq!(
            testresult,
            b"/a/b/foo.bar/bar.foo/foo.bar/bar.foo/${FOO_foo.bar}/"
        );
        assert!(contains_context_variables(&testresult));

        // Check the used context variables.
        assert_eq!(2, used_envs.len());
        let mut iter = used_envs.iter();
        assert_eq!(
            iter.next(),
            Some((&b"TEST1".to_vec(), &b"foo.bar".to_vec()))
        );
        assert_eq!(
            iter.next(),
            Some((&b"TEST1NG".to_vec(), &b"bar.foo".to_vec()))
        );
    }

    used_envs.clear();
    {
        // That's also a right context variable syntax but it still does not exist.
        let testresult = resolve_context_variables(b"$TEST2", &env_map, &mut used_envs);
        assert_eq!(testresult, b"$TEST2");
        assert!(contains_context_variables(&testresult));
        assert_eq!(0, used_envs.len());
    }

    used_envs.clear();
    {
        // That's not a context variable because of a wrong syntax. But a context variable named
        // TEST1 exists so it means that %TEST1% would have succeeded.
        let testresult = resolve_context_variables(b"%TEST1", &env_map, &mut used_envs);
        assert_eq!(testresult, b"%TEST1");
        assert!(!contains_context_variables(&testresult));
        assert_eq!(0, used_envs.len());
    }

    used_envs.clear();
    {
        // That's still not a context variable because of a wrong syntax.
        let testresult = resolve_context_variables(b"TEST1%", &env_map, &mut used_envs);
        assert_eq!(testresult, b"TEST1%");
        assert!(!contains_context_variables(&testresult));
        assert_eq!(0, used_envs.len());
    }

    used_envs.clear();
    {
        // That's an ambiguous context variable as the syntax is right but the name is '{TEST1'
        // which does not exist (but 'TEST1' exists).
        let testresult = resolve_context_variables(b"${TEST1", &env_map, &mut used_envs);
        assert_eq!(testresult, b"${TEST1");
        assert!(contains_context_variables(&testresult));
        assert_eq!(0, used_envs.len());
    }
}
