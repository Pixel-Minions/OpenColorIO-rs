// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The regex engines don't overflow their caller's stack: expressions both wheels compile,
//! thousands of groups or quantifiers deep, are compiled, copied, printed, matched and dropped
//! on a thread of 1 MiB (a program's main thread on Windows). The engines recurse on threads of
//! their own, sized from the stack their recursion takes at opt-level 0 (where frames are
//! largest: measured 7.3 KiB per nesting level for libstdc++'s parser, 1.9 KiB for MSVC's,
//! 1.43 MiB for MSVC's matcher at its limit), and drop a tree without recursion.
//!
//! The results aren't checked here (the oracle tests do); only that each call returns.

use ocio_ops::std_regex::{Library, Regex, regex_match};

/// Runs `f` on a thread of 1 MiB; a stack overflow aborts the test process.
fn on_small_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

fn exercise(pattern: String, text: Vec<u8>, library: Library) {
    on_small_stack(move || {
        let re = Regex::new(pattern.as_bytes(), library);
        let copy = re.clone();
        let printed = format!("{re:?}");
        assert!(!printed.is_empty());
        if let Ok(re) = &re {
            let _ = regex_match(&text, re);
        }
        drop(re);
        drop(copy);
    });
}

/// 5,000 nested groups of each kind (the most both ports compile), in both libraries.
#[test]
fn deep_groups() {
    for library in [Library::Msvc, Library::Libstdcxx] {
        for open in ["(?:a|", "a(?:", "(?:[ab]", "(?=a|", "(?:", "(?="] {
            let pattern = format!("{}a{}", open.repeat(5000), ")".repeat(5000));
            exercise(pattern, b"a".to_vec(), library);
        }
    }
}

/// Quantifiers on quantifiers, which only libstdc++ accepts: up to 90,000 of them.
#[test]
fn stacked_quantifiers() {
    for count in [20_000, 90_000] {
        let pattern = format!("a{}", "*".repeat(count));
        exercise(pattern.clone(), b"a".to_vec(), Library::Libstdcxx);
        exercise(pattern, Vec::new(), Library::Libstdcxx);
    }
}

/// MSVC's matcher at its depth limit (600 nested matches).
#[test]
fn msvc_matcher_at_its_limit() {
    for (pattern, text) in [
        ("(a|b|c)*", vec![b'c'; 298]),
        ("(?:(a)|b)*", vec![b'a'; 300]),
        ("(?:(?=a|b).)*", vec![b'a'; 299]),
        ("((a)(b))*", b"ab".repeat(300)),
    ] {
        exercise(pattern.to_string(), text, Library::Msvc);
    }
}
