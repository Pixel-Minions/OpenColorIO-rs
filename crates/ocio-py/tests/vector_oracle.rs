// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `vector_of_uint8_t` (chunk 6.6a, pybind11's `bind_vector` as the wheel binds it) against the
//! wheel: the class's surface (py_types_oracle.rs compares it member for member), and its
//! behaviour through the same step scripts on both sides:
//! - the constructors (none, a copy, an iterable: lists, tuples, bytes, a `bytearray`, a set, a
//!   dict, a range, an iterator, a NumPy array and scalar, booleans) and the items the caster
//!   refuses (out of range, negative, a float, `None`, text), with pybind11's `RuntimeError`;
//! - indexing and slicing (negative, out of range, past 64 bits, steps of every sign, a zero
//!   step), assignment (an index, a slice from a vector, a list or itself, sizes that differ),
//!   deletion (an index, slices whose step pybind11 handles and the right ones);
//! - `append`, `insert` (the size itself, negative, out of range), `extend` (a vector, itself,
//!   an iterable, an item refused: the appended ones erased), `pop`, `remove`, `count`, `in`,
//!   `clear`, `len`, `bool`, `==` and `!=` (with the implicit conversions from a `list` and a
//!   `bytearray`, `NotImplemented` otherwise), `repr` (bytes written as characters, and the
//!   UnicodeDecodeError of those that aren't UTF-8), `hash`;
//! - iteration (`iter`, `next` to `StopIteration` and past it, the iterator class, an element
//!   changed under it, a vector grown with room in its buffer), keyword arguments.
//!
//! And, without the wheel, which reads, writes or erases memory the vector doesn't hold there:
//! a slice deletion whose loop leaves the vector, a slice whose bound's `__index__` shrank it
//! (U-132), and an iterator read after its vector reallocated or shrank under it (U-133).
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, by, compare, d, f, i, l, n, none, r, ra, s, t};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};

/// A list of ints.
fn ints(values: &[i128]) -> Value {
    l(values.iter().map(|&v| i(v)).collect())
}

/// `slice(start, stop, step)` of the step interpreter (`None` for a missing bound).
fn slice(sc: &mut Script, start: Option<i128>, stop: Option<i128>, step: Option<i128>) -> usize {
    let v = |x: Option<i128>| x.map_or_else(none, i);
    sc.call(n("slice"), vec![v(start), v(stop), v(step)])
}

/// A new vector of `values`, its step.
fn vector(sc: &mut Script, values: &[i128]) -> usize {
    sc.call(n("OCIO.vector_of_uint8_t"), vec![ints(values)])
}

/// The vector's items as a list, and its repr.
fn show(sc: &mut Script, v: usize) {
    sc.call(n("list"), vec![r(v)]);
    sc.call(n("len"), vec![r(v)]);
}

fn constructors_script() -> Script {
    let mut sc = Script::new();
    let empty = sc.call(n("OCIO.vector_of_uint8_t"), vec![]);
    sc.repr(r(empty));
    sc.call(n("bool"), vec![r(empty)]);
    let v = vector(&mut sc, &[1, 2, 3, 250]);
    show(&mut sc, v);
    sc.raises(n("repr"), vec![r(v)]);
    sc.raises(n("str"), vec![r(v)]);
    let copy = sc.call(n("OCIO.vector_of_uint8_t"), vec![r(v)]);
    sc.call(n("operator.is_"), vec![r(copy), r(v)]);
    show(&mut sc, copy);
    sc.call(ra(copy, "append"), vec![i(7)]);
    show(&mut sc, v);
    let ascii = vector(&mut sc, &[65, 66, 10, 0]);
    sc.repr(r(ascii));
    sc.call(n("str"), vec![r(ascii)]);
    for arg in [
        t(vec![i(1), i(2)]),
        by(b"ab\xff"),
        ints(&[0, 255]),
        l(vec![b(true), b(false)]),
    ] {
        let k = sc.call(n("OCIO.vector_of_uint8_t"), vec![arg]);
        show(&mut sc, k);
    }
    let ba = sc.call(n("bytearray"), vec![by(b"\x01\x02")]);
    let k = sc.call(n("OCIO.vector_of_uint8_t"), vec![r(ba)]);
    show(&mut sc, k);
    let set = sc.call(n("set"), vec![ints(&[3, 1, 2])]);
    let k = sc.call(n("OCIO.vector_of_uint8_t"), vec![r(set)]);
    show(&mut sc, k);
    let range = sc.call(n("range"), vec![i(250), i(256)]);
    let k = sc.call(n("OCIO.vector_of_uint8_t"), vec![r(range)]);
    show(&mut sc, k);
    let it = sc.call(n("iter"), vec![ints(&[4, 5])]);
    let k = sc.call(n("OCIO.vector_of_uint8_t"), vec![r(it)]);
    show(&mut sc, k);
    let d = sc.call(n("dict"), vec![l(vec![t(vec![i(9), i(8)])])]);
    let k = sc.call(n("OCIO.vector_of_uint8_t"), vec![r(d)]);
    show(&mut sc, k);
    let arr = json!({ "array": { "dtype": "|u1", "shape": [2], "bytes": "0102" } });
    let k = sc.call(n("OCIO.vector_of_uint8_t"), vec![arr]);
    show(&mut sc, k);
    let scalar = json!({ "scalar": { "dtype": "<i8", "bytes": "0300000000000000" } });
    let k = sc.call(n("OCIO.vector_of_uint8_t"), vec![l(vec![scalar])]);
    show(&mut sc, k);
    // Refused items: pybind11's cast_error; refused arguments: no overload.
    for arg in [
        ints(&[256]),
        ints(&[-1]),
        l(vec![f(1.0)]),
        l(vec![none()]),
        l(vec![i(1), s("a")]),
        s("ab"),
    ] {
        sc.raises(n("OCIO.vector_of_uint8_t"), vec![arg]);
    }
    for arg in [i(5), none(), f(1.0)] {
        sc.raises(n("OCIO.vector_of_uint8_t"), vec![arg]);
    }
    sc.raises(n("OCIO.vector_of_uint8_t"), vec![ints(&[1]), ints(&[2])]);
    sc.call_kw(
        n("OCIO.vector_of_uint8_t"),
        vec![],
        vec![("arg0", ints(&[1]))],
        Expect::Raised,
    );
    sc.raises(n("hash"), vec![r(v)]);
    sc
}

fn indexing_script() -> Script {
    let mut sc = Script::new();
    let v = vector(&mut sc, &[1, 2, 3, 4, 5, 6]);
    for k in [0, 5, -1, -6, 6, -7] {
        let step = json!({ "call": ra(v, "__getitem__"), "args": [i(k)] });
        let expect = if (-6..6).contains(&k) {
            Expect::Value
        } else {
            Expect::Raised
        };
        sc.step(step, expect);
    }
    sc.raises(ra(v, "__getitem__"), vec![i(1 << 63)]);
    sc.raises(ra(v, "__getitem__"), vec![s("0")]);
    sc.call(ra(v, "__getitem__"), vec![b(true)]);
    for (start, stop, step) in [
        (None, None, None),
        (Some(1), Some(4), None),
        (None, None, Some(-1)),
        (None, None, Some(-2)),
        (Some(4), Some(0), Some(-2)),
        (Some(10), None, None),
        (Some(-100), Some(2), None),
        (Some(1), None, Some(3)),
    ] {
        let sl = slice(&mut sc, start, stop, step);
        let part = sc.call(ra(v, "__getitem__"), vec![r(sl)]);
        show(&mut sc, part);
    }
    let zero = slice(&mut sc, None, None, Some(0));
    sc.raises(ra(v, "__getitem__"), vec![r(zero)]);
    let two = slice(&mut sc, None, Some(2), None);
    let part = sc.call_kw(
        ra(v, "__getitem__"),
        vec![],
        vec![("s", r(two))],
        Expect::Value,
    );
    show(&mut sc, part);
    sc.call_kw(
        ra(v, "__getitem__"),
        vec![],
        vec![("arg0", i(0))],
        Expect::Raised,
    );
    // __setitem__: an index; a slice from a vector, a list (converted), a tuple (refused),
    // itself, a size that differs; a value out of range.
    sc.call(ra(v, "__setitem__"), vec![i(-1), i(9)]);
    sc.raises(ra(v, "__setitem__"), vec![i(6), i(9)]);
    sc.raises(ra(v, "__setitem__"), vec![i(0), i(300)]);
    let sl = slice(&mut sc, Some(0), Some(2), None);
    let src = vector(&mut sc, &[7, 8]);
    sc.call(ra(v, "__setitem__"), vec![r(sl), r(src)]);
    sc.call(ra(v, "__setitem__"), vec![r(sl), ints(&[17, 18])]);
    sc.raises(ra(v, "__setitem__"), vec![r(sl), t(vec![i(1), i(2)])]);
    sc.raises(ra(v, "__setitem__"), vec![r(sl), ints(&[1])]);
    sc.raises(ra(v, "__setitem__"), vec![r(sl), ints(&[1, 300])]);
    show(&mut sc, v);
    let rev = slice(&mut sc, None, None, Some(-1));
    sc.call(ra(v, "__setitem__"), vec![r(rev), r(v)]);
    show(&mut sc, v);
    let all = slice(&mut sc, None, None, None);
    sc.call(ra(v, "__setitem__"), vec![r(all), r(v)]);
    show(&mut sc, v);
    sc.call_kw(
        ra(v, "__setitem__"),
        vec![],
        vec![("arg0", i(0)), ("arg1", i(1))],
        Expect::Raised,
    );
    // __delitem__: an index; slices with positive steps, and negative ones whose pybind11 loop
    // stays inside the vector.
    sc.call(ra(v, "__delitem__"), vec![i(-1)]);
    sc.raises(ra(v, "__delitem__"), vec![i(9)]);
    show(&mut sc, v);
    for (values, start, stop, step) in [
        (&[1, 2, 3, 4][..], None, None, Some(2)),
        (&[1, 2, 3, 4][..], Some(1), None, Some(2)),
        (&[1, 2, 3, 4][..], None, None, Some(-2)),
        (&[1, 2, 3, 4, 5, 6][..], Some(4), Some(0), Some(-2)),
        (&[1, 2, 3, 4][..], Some(3), Some(2), Some(-1)),
        (&[1, 2, 3, 4][..], Some(1), Some(3), None),
        (&[1, 2, 3, 4][..], Some(5), None, None),
    ] {
        let w = vector(&mut sc, values);
        let sl = slice(&mut sc, start, stop, step);
        sc.call(ra(w, "__delitem__"), vec![r(sl)]);
        show(&mut sc, w);
    }
    // A slice whose bound's `__index__` reads the vector (its `__len__`, called without the
    // instance: a bound method isn't a descriptor), for each method taking a slice.
    let w = vector(&mut sc, &[1, 2, 3, 4]);
    let half = sc.call(
        n("type"),
        vec![
            s("Half"),
            t(vec![]),
            d(vec![
                (s("__module__"), s("probe")),
                (s("__index__"), ra(w, "__len__")),
            ]),
        ],
    );
    let h = sc.call(r(half), vec![]);
    let from = sc.call(n("slice"), vec![r(h), none(), none()]);
    let upto = sc.call(n("slice"), vec![none(), r(h), none()]);
    let part = sc.call(ra(w, "__getitem__"), vec![r(from)]);
    show(&mut sc, part);
    let part = sc.call(ra(w, "__getitem__"), vec![r(upto)]);
    show(&mut sc, part);
    sc.call(ra(w, "__setitem__"), vec![r(upto), ints(&[5, 6, 7, 8])]);
    show(&mut sc, w);
    sc.call(ra(w, "__delitem__"), vec![r(from)]);
    show(&mut sc, w);
    sc.call(ra(w, "__delitem__"), vec![r(upto)]);
    show(&mut sc, w);
    sc
}

fn methods_script() -> Script {
    let mut sc = Script::new();
    let v = vector(&mut sc, &[2, 5, 3]);
    sc.call(ra(v, "append"), vec![i(0)]);
    sc.raises(ra(v, "append"), vec![i(256)]);
    sc.raises(ra(v, "append"), vec![f(1.0)]);
    sc.call_kw(ra(v, "append"), vec![], vec![("x", i(4))], Expect::Value);
    show(&mut sc, v);
    sc.call(ra(v, "insert"), vec![i(-1), i(6)]);
    sc.call(ra(v, "insert"), vec![i(6), i(7)]);
    sc.raises(ra(v, "insert"), vec![i(8), i(7)]);
    sc.raises(ra(v, "insert"), vec![i(-9), i(7)]);
    sc.call_kw(
        ra(v, "insert"),
        vec![],
        vec![("x", i(1)), ("i", i(0))],
        Expect::Value,
    );
    show(&mut sc, v);
    sc.call(ra(v, "pop"), vec![]);
    sc.call(ra(v, "pop"), vec![i(0)]);
    sc.call(ra(v, "pop"), vec![i(-2)]);
    sc.raises(ra(v, "pop"), vec![i(9)]);
    sc.raises(ra(v, "pop"), vec![i(-9)]);
    sc.call_kw(ra(v, "pop"), vec![], vec![("i", i(0))], Expect::Value);
    show(&mut sc, v);
    let empty = sc.call(n("OCIO.vector_of_uint8_t"), vec![]);
    sc.raises(ra(empty, "pop"), vec![]);
    sc.raises(ra(empty, "remove"), vec![i(1)]);
    // extend: a vector, itself, an iterable, an item refused (the appended ones erased), a
    // tuple and text.
    let w = vector(&mut sc, &[1, 2, 3]);
    sc.call(ra(w, "extend"), vec![r(w)]);
    show(&mut sc, w);
    sc.call(ra(w, "extend"), vec![ints(&[9, 8])]);
    sc.call(ra(w, "extend"), vec![by(b"\x05")]);
    let other = vector(&mut sc, &[4]);
    sc.call(ra(w, "extend"), vec![r(other)]);
    sc.raises(ra(w, "extend"), vec![l(vec![i(1), i(300)])]);
    sc.raises(ra(w, "extend"), vec![s("ab")]);
    sc.raises(ra(w, "extend"), vec![i(3)]);
    sc.call(ra(w, "extend"), vec![t(vec![])]);
    sc.call_kw(
        ra(w, "extend"),
        vec![],
        vec![("L", ints(&[6]))],
        Expect::Value,
    );
    show(&mut sc, w);
    // count, remove, in.
    for x in [5, 9, 77] {
        sc.call(ra(w, "count"), vec![i(x)]);
        sc.call(ra(w, "__contains__"), vec![i(x)]);
    }
    sc.raises(ra(w, "count"), vec![i(300)]);
    sc.raises(ra(w, "__contains__"), vec![s("a")]);
    sc.call(ra(w, "remove"), vec![i(9)]);
    sc.raises(ra(w, "remove"), vec![i(77)]);
    show(&mut sc, w);
    // Comparisons, with the implicit conversions.
    let one = vector(&mut sc, &[1]);
    let ba = sc.call(n("bytearray"), vec![by(b"\x01")]);
    for other in [
        r(one),
        ints(&[1]),
        r(ba),
        t(vec![i(1)]),
        ints(&[300]),
        by(b"\x01"),
        i(1),
    ] {
        sc.call(n("operator.eq"), vec![r(one), other.clone()]);
        sc.call(n("operator.ne"), vec![r(one), other.clone()]);
        sc.call(ra(one, "__eq__"), vec![other]);
    }
    sc.call(n("operator.eq"), vec![ints(&[1]), r(one)]);
    sc.call(ra(w, "clear"), vec![]);
    show(&mut sc, w);
    sc.call(n("bool"), vec![r(w)]);
    sc.raises(ra(w, "__len__"), vec![i(1)]);
    sc
}

fn iteration_script() -> Script {
    let mut sc = Script::new();
    let v = vector(&mut sc, &[1, 2, 3]);
    let it = sc.call(n("iter"), vec![r(v)]);
    let ty = sc.call(n("type"), vec![r(it)]);
    sc.repr(r(ty));
    let same = sc.call(ra(it, "__iter__"), vec![]);
    sc.call(n("operator.is_"), vec![r(same), r(it)]);
    sc.call(n("next"), vec![r(it)]);
    // An element changed under the iterator shows.
    sc.call(ra(v, "__setitem__"), vec![i(1), i(9)]);
    sc.call(n("list"), vec![r(it)]);
    sc.raises(n("next"), vec![r(it)]);
    sc.raises(n("next"), vec![r(it)]);
    sc.raises(ra(it, "__next__"), vec![i(1)]);
    // Grown with room in its buffer (a vector reserved by a slice of itself: none here), and
    // an empty one grown: the iterator's end is the one it was made with.
    let empty = sc.call(n("OCIO.vector_of_uint8_t"), vec![]);
    let it2 = sc.call(n("iter"), vec![r(empty)]);
    sc.call(ra(empty, "append"), vec![i(1)]);
    sc.call(n("list"), vec![r(it2)]);
    let w = vector(&mut sc, &[1, 2]);
    let it3 = sc.call(n("iter"), vec![r(w)]);
    sc.call(n("next"), vec![r(it3)]);
    sc.call(n("next"), vec![r(it3)]);
    sc.call(ra(w, "append"), vec![i(3)]);
    sc.call(n("list"), vec![r(it3)]);
    sc.raises(r(ty), vec![]);
    sc.raises(ra(ty, "__next__"), vec![i(5)]);
    sc
}

/// The class behaves as the wheel's.
#[test]
fn the_class_behaves_as_the_wheels() {
    compare(&[
        constructors_script(),
        indexing_script(),
        methods_script(),
        iteration_script(),
    ]);
}

/// Runs `code` in the port's module with `V` the class; the error it raises.
fn port_error(py: Python<'_>, code: &str) -> PyErr {
    let globals = PyDict::new(py);
    py.run(
        c"import PyOpenColorIO as OCIO\nV = OCIO.vector_of_uint8_t",
        Some(&globals),
        None,
    )
    .unwrap();
    let code = std::ffi::CString::new(code).unwrap();
    py.run(&code, Some(&globals), None)
        .expect_err("the port raises")
}

/// Where the wheel reads or erases memory the vector doesn't hold, the port's error: a slice
/// deletion whose loop leaves the vector (U-132), an iterator read after its vector reallocated
/// or shrank under it (U-133).
#[test]
fn where_the_wheel_leaves_the_vector_the_port_raises() {
    common::start_python();
    Python::attach(|py| {
        for (code, entry) in [
            ("v = V([1, 2, 3, 4])\ndel v[::-1]", "U-132"),
            ("v = V([1, 2, 3])\ndel v[2::-1]", "U-132"),
            // A slice whose `__index__` shrinks the vector after pybind11 read its size.
            (
                "v = V([1, 2, 3, 4])\nP = type('P', (), {'__index__': v.pop})\nv[:P()]",
                "U-132",
            ),
            (
                "v = V([1, 2, 3, 4])\nP = type('P', (), {'__index__': v.pop})\n\
                 v[:P()] = V([5, 6, 7, 8])",
                "U-132",
            ),
            (
                "v = V([1, 2, 3, 4])\nP = type('P', (), {'__index__': v.pop})\ndel v[:P()]",
                "U-132",
            ),
            (
                "v = V([1, 2, 3])\nit = iter(v)\nv.append(4)\nnext(it)",
                "U-133",
            ),
            (
                "v = V([1, 2, 3])\nit = iter(v)\nv.clear()\nnext(it)",
                "U-133",
            ),
            (
                "v = V([1, 2, 3])\nit = iter(v)\nnext(it)\nv.pop()\nv.pop()\nnext(it)",
                "U-133",
            ),
            (
                "v = V([1, 2])\nit = iter(v)\nv.extend(range(9))\nnext(it)",
                "U-133",
            ),
        ] {
            let err = port_error(py, code);
            let text = err.to_string();
            assert!(
                text.starts_with("ValueError")
                    && text.contains(&format!("docs/improvements.md, {entry}")),
                "{code}: {text}"
            );
        }
        // The controls: the vector unchanged by the refused deletion; the same iterations
        // without the change under them.
        let globals = PyDict::new(py);
        py.run(
            c"import PyOpenColorIO as OCIO\nV = OCIO.vector_of_uint8_t\n\
              v = V([1, 2, 3, 4])\ntry:\n    del v[::-1]\nexcept ValueError:\n    pass\n\
              assert list(v) == [1, 2, 3, 4]\n\
              w = V([1, 2, 3])\nit = iter(w)\nw[0] = 5\nassert list(it) == [5, 2, 3]\n\
              e = V()\nit = iter(e)\ne.append(1)\nassert list(it) == []",
            Some(&globals),
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"));
    });
}
