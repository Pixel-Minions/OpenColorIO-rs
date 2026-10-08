// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The config's model objects against the wheel, through the oracle's `config_calls`: random
//! sequences of calls on `ColorSpace`, `NamedTransform`, `ViewTransform` and `Look` with odd
//! byte strings (empty, blank, cased, NUL, non-UTF-8), each call's result or exception, then
//! the object's getters and `repr()`; random `ColorSpaceSet` operations; and the cases the
//! p3-model-objects verifier found unchecked. Written by the verifier (its killing tests).
//!
//! Strings are compared as bytes: a text the binding can't decode is compared with the port's
//! bytes too (`to_bytes()`, `Exception::what()`).

use ocio::{ColorSpace, Look, NamedTransform, ReferenceSpaceType, ViewTransform};
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes, bytes_arg};
use serde_json::{Value, json};

#[derive(Debug, PartialEq, Clone)]
enum R {
    Unit,
    Bool(bool),
    Err(Vec<u8>),
    Undecodable(Vec<u8>),
    Text(Vec<u8>),
}

fn unhex(h: &str) -> Vec<u8> {
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
        .collect()
}

fn wheel_r(v: &Value) -> R {
    if let Some(e) = v.get("exception") {
        return R::Err(bytes(&e["message"]));
    }
    if let Some(u) = v.get("undecodable") {
        return R::Undecodable(unhex(u.as_str().unwrap()));
    }
    match &v["result"] {
        Value::Null => R::Unit,
        Value::Bool(b) => R::Bool(*b),
        o if o.get("bytes").is_some() => R::Text(bytes(o)),
        _ => R::Unit,
    }
}

const STRS: &[&[u8]] = &[
    b"",
    b" ",
    b"  \t ",
    b"\x01x\x1f",
    b"A",
    b"a",
    b" a ",
    b"\xc3\xa9",
    b"\xa0",
    b"\xff\xfe",
    b"x\0y",
    b"Linear",
    b"LINEAR ",
    b"\xa0lin\xa0",
    b"a, b",
    b"[x]",
    b"Z",
    b"z\x7f",
    b"@",
    b"\x60",
    b"\x81",
    b"b",
    b"B\t",
];

const IDS: &[&[u8]] = &[
    b"",
    b"a",
    b"a:b",
    b"a::b",
    b":",
    b"a:",
    b":b",
    b"a:b:c",
    b"A",
    b"a b",
    b"a\0B",
    b"\xc3\xa9",
    b"\xff",
    b"ok|()[]^%#*~/+-._",
    b"x:y|z",
    b"0:0",
    b"::",
    b"a:b:",
    b"{",
    b"\\",
    b"\x60",
    b"@",
];

const ATTRS: &[&[u8]] = &[
    b"amf_transform_ids",
    b"AMF_TRANSFORM_IDS",
    b"icc_profile_name",
    b"Icc_Profile_Name",
    b"",
    b"unknown",
    b"amf_transform_ids\0x",
    b"amf_transform_ids ",
    b"\xff",
];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as usize
    }
    fn pick(&mut self, xs: &[&'static [u8]]) -> &'static [u8] {
        xs[self.next() % xs.len()]
    }
}

/// An object's `repr()`, as bytes.
trait Repr {
    fn repr(&self) -> Vec<u8>;
}

macro_rules! repr {
    ($($T:ty),*) => {$(
        impl Repr for $T {
            fn repr(&self) -> Vec<u8> {
                self.to_bytes()
            }
        }
    )*};
}
repr!(ColorSpace, Look, NamedTransform, ViewTransform);

type PortOp<T> = Box<dyn Fn(&mut T) -> R>;

#[allow(clippy::too_many_arguments)]
fn run<T: Repr>(
    class: &str,
    new_args: Value,
    make: impl Fn() -> T,
    generate: impl Fn(&mut Rng) -> (Value, PortOp<T>),
    getters: impl Fn(&T, &Value) -> Vec<String>,
    seed: u64,
    cases: usize,
    ops_per_case: usize,
) -> (Vec<String>, Vec<String>) {
    let mut rng = Rng(seed);
    let mut calls = Vec::new();
    let mut plan = Vec::new();
    for k in 0..cases {
        let name = format!("o{k}");
        calls.push(json!({"new": class, "args": new_args, "as": name}));
        let mut ops = Vec::new();
        for _ in 0..ops_per_case {
            let (mut call, port) = generate(&mut rng);
            call["on"] = json!(name);
            calls.push(call.clone());
            ops.push((call, port));
        }
        calls.push(json!({"dump": name}));
        plan.push(ops);
    }
    let response = Oracle::get()
        .call(
            "config_calls",
            json!({"config": "new", "calls": calls}),
            &[],
        )
        .result;
    let results = response["calls"].as_array().unwrap();
    let mut failures = Vec::new();
    let mut lossy = Vec::new();
    let mut i = 0;
    for (k, ops) in plan.iter().enumerate() {
        let mut obj = make();
        assert!(results[i].get("result").is_some(), "{}", results[i]);
        i += 1;
        for (call, port) in ops {
            let w = wheel_r(&results[i]);
            let p = port(&mut obj);
            let log = &results[i]["log"];
            if log.as_array().is_some_and(|l| !l.is_empty()) {
                failures.push(format!("{class} {k}: {call}: log {log}"));
            }
            match (&w, &p) {
                (R::Undecodable(a), R::Err(b) | R::Text(b)) => {
                    if a != b {
                        lossy.push(format!(
                            "{class} {k}: {call}: wheel msg {:?} port msg {:?}",
                            String::from_utf8_lossy(a),
                            String::from_utf8_lossy(b)
                        ));
                    }
                }
                _ => {
                    if w != p {
                        failures.push(format!("{class} {k}: {call}: wheel {w:?} port {p:?}"));
                    }
                }
            }
            i += 1;
        }
        let dump = &results[i]["result"];
        i += 1;
        let port_repr = obj.repr();
        if let Some(r) = dump.get("repr") {
            let (wb, undecodable) = if r.get("bytes").is_some() {
                (bytes(r), false)
            } else {
                (unhex(r["undecodable"].as_str().unwrap()), true)
            };
            if wb != port_repr {
                let msg = format!(
                    "{class} {k}: repr\n   wheel {:?}\n   port  {:?}",
                    String::from_utf8_lossy(&wb),
                    String::from_utf8_lossy(&port_repr)
                );
                if undecodable {
                    lossy.push(msg)
                } else {
                    failures.push(msg)
                }
            }
        } else {
            failures.push(format!("{class} {k}: no repr: {dump}"));
        }
        for d in getters(&obj, &dump["getters"]) {
            failures.push(format!("{class} {k}: {d}"));
        }
    }
    (failures, lossy)
}

fn text_or_raw(v: &Value) -> Vec<u8> {
    if v.get("bytes").is_some() {
        bytes(v)
    } else {
        unhex(v["undecodable"].as_str().unwrap_or_else(|| panic!("{v}")))
    }
}

fn texts(v: &Value) -> Vec<Vec<u8>> {
    v.as_array()
        .unwrap_or_else(|| panic!("not a list: {v}"))
        .iter()
        .map(text_or_raw)
        .collect()
}

fn err(r: ocio::Result<()>) -> R {
    match r {
        Ok(()) => R::Unit,
        Err(e) => R::Err(e.what().to_vec()),
    }
}

macro_rules! setter {
    ($rng:expr, $call:literal, $m:ident, $T:ty) => {{
        let s = $rng.pick(STRS);
        (
            json!({"call": $call, "args": [bytes_arg(s)]}),
            Box::new(move |o: &mut $T| {
                o.$m(s);
                R::Unit
            }) as PortOp<$T>,
        )
    }};
}
macro_rules! query {
    ($rng:expr, $call:literal, $m:ident, $T:ty) => {{
        let s = $rng.pick(STRS);
        (
            json!({"call": $call, "args": [bytes_arg(s)]}),
            Box::new(move |o: &mut $T| R::Bool(o.$m(s))) as PortOp<$T>,
        )
    }};
}

fn cs_gen(rng: &mut Rng) -> (Value, PortOp<ColorSpace>) {
    match rng.next() % 16 {
        0 => setter!(rng, "setName", set_name, ColorSpace),
        1 | 2 => setter!(rng, "addAlias", add_alias, ColorSpace),
        3 => setter!(rng, "removeAlias", remove_alias, ColorSpace),
        4 => query!(rng, "hasAlias", has_alias, ColorSpace),
        5 | 6 => setter!(rng, "addCategory", add_category, ColorSpace),
        7 => setter!(rng, "removeCategory", remove_category, ColorSpace),
        8 => query!(rng, "hasCategory", has_category, ColorSpace),
        9 => setter!(rng, "setFamily", set_family, ColorSpace),
        10 => setter!(rng, "setEncoding", set_encoding, ColorSpace),
        11 => setter!(rng, "setEqualityGroup", set_equality_group, ColorSpace),
        12 => setter!(rng, "setDescription", set_description, ColorSpace),
        13 => {
            let s = rng.pick(IDS);
            (
                json!({"call": "setInteropID", "args": [bytes_arg(s)]}),
                Box::new(move |o: &mut ColorSpace| err(o.set_interop_id(s))),
            )
        }
        14 => {
            let a = rng.pick(ATTRS);
            let v = rng.pick(STRS);
            (
                json!({"call": "setInterchangeAttribute", "args": [bytes_arg(a), bytes_arg(v)]}),
                Box::new(move |o: &mut ColorSpace| err(o.set_interchange_attribute(a, v))),
            )
        }
        _ => {
            let a = rng.pick(ATTRS);
            (
                json!({"call": "getInterchangeAttribute", "args": [bytes_arg(a)]}),
                Box::new(move |o: &mut ColorSpace| match o.interchange_attribute(a) {
                    Ok(v) => R::Text(v.to_vec()),
                    Err(e) => R::Err(e.what().to_vec()),
                }),
            )
        }
    }
}

fn list_diff(out: &mut Vec<String>, what: &str, wheel: Vec<Vec<u8>>, port: Vec<Vec<u8>>) {
    if wheel != port {
        out.push(format!("{what} wheel {wheel:?} port {port:?}"));
    }
}

fn cs_getters(c: &ColorSpace, g: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let aliases: Vec<Vec<u8>> = (0..c.num_aliases()).map(|i| c.alias(i).to_vec()).collect();
    list_diff(&mut out, "aliases", texts(&g["getAliases"]), aliases);
    let cats: Vec<Vec<u8>> = (0..c.num_categories())
        .map(|i| c.category(i).unwrap().to_vec())
        .collect();
    list_diff(&mut out, "categories", texts(&g["getCategories"]), cats);
    for (name, v) in [
        ("getName", c.name()),
        ("getFamily", c.family()),
        ("getEncoding", c.encoding()),
        ("getInteropID", c.interop_id()),
        ("getDescription", c.description()),
        ("getEqualityGroup", c.equality_group()),
    ] {
        if text_or_raw(&g[name]) != v {
            out.push(format!(
                "{name} wheel {:?} port {:?}",
                text_or_raw(&g[name]),
                v
            ));
        }
    }
    out
}

fn nt_gen(rng: &mut Rng) -> (Value, PortOp<NamedTransform>) {
    match rng.next() % 11 {
        0 => setter!(rng, "setName", set_name, NamedTransform),
        1 | 2 => setter!(rng, "addAlias", add_alias, NamedTransform),
        3 => setter!(rng, "removeAlias", remove_alias, NamedTransform),
        4 => query!(rng, "hasAlias", has_alias, NamedTransform),
        5 | 6 => setter!(rng, "addCategory", add_category, NamedTransform),
        7 => setter!(rng, "removeCategory", remove_category, NamedTransform),
        8 => query!(rng, "hasCategory", has_category, NamedTransform),
        9 => setter!(rng, "setFamily", set_family, NamedTransform),
        _ => setter!(rng, "setEncoding", set_encoding, NamedTransform),
    }
}

fn nt_getters(c: &NamedTransform, g: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let aliases: Vec<Vec<u8>> = (0..c.num_aliases()).map(|i| c.alias(i).to_vec()).collect();
    list_diff(&mut out, "aliases", texts(&g["getAliases"]), aliases);
    let cats: Vec<Vec<u8>> = (0..c.num_categories())
        .map(|i| c.category(i).unwrap().to_vec())
        .collect();
    list_diff(&mut out, "categories", texts(&g["getCategories"]), cats);
    out
}

fn vt_gen(rng: &mut Rng) -> (Value, PortOp<ViewTransform>) {
    match rng.next() % 8 {
        0 => setter!(rng, "setName", set_name, ViewTransform),
        1 | 2 => setter!(rng, "addCategory", add_category, ViewTransform),
        3 => setter!(rng, "removeCategory", remove_category, ViewTransform),
        4 => query!(rng, "hasCategory", has_category, ViewTransform),
        5 => setter!(rng, "setFamily", set_family, ViewTransform),
        6 => setter!(rng, "setDescription", set_description, ViewTransform),
        _ => {
            let a = rng.pick(ATTRS);
            let v = rng.pick(STRS);
            (
                json!({"call": "setInterchangeAttribute", "args": [bytes_arg(a), bytes_arg(v)]}),
                Box::new(move |o: &mut ViewTransform| err(o.set_interchange_attribute(a, v))),
            )
        }
    }
}

fn vt_getters(c: &ViewTransform, g: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let cats: Vec<Vec<u8>> = (0..c.num_categories())
        .map(|i| c.category(i).unwrap().to_vec())
        .collect();
    list_diff(&mut out, "categories", texts(&g["getCategories"]), cats);
    out
}

fn look_gen(rng: &mut Rng) -> (Value, PortOp<Look>) {
    match rng.next() % 5 {
        0 => setter!(rng, "setName", set_name, Look),
        1 => setter!(rng, "setProcessSpace", set_process_space, Look),
        2 => setter!(rng, "setDescription", set_description, Look),
        3 => {
            let a = rng.pick(ATTRS);
            (
                json!({"call": "getInterchangeAttribute", "args": [bytes_arg(a)]}),
                Box::new(move |o: &mut Look| match o.interchange_attribute(a) {
                    Ok(v) => R::Text(v.to_vec()),
                    Err(e) => R::Err(e.what().to_vec()),
                }),
            )
        }
        _ => {
            let a = rng.pick(ATTRS);
            let v = rng.pick(STRS);
            (
                json!({"call": "setInterchangeAttribute", "args": [bytes_arg(a), bytes_arg(v)]}),
                Box::new(move |o: &mut Look| err(o.set_interchange_attribute(a, v))),
            )
        }
    }
}

fn report(name: &str, (failures, lossy): (Vec<String>, Vec<String>)) {
    println!(
        "{name}: {} failures, {} lossy (non-UTF-8) differences",
        failures.len(),
        lossy.len()
    );
    for f in failures.iter().take(60) {
        println!("  FAIL {f}");
    }
    for f in lossy.iter().take(6) {
        println!("  LOSSY {f}");
    }
    assert!(
        failures.is_empty() && lossy.is_empty(),
        "{name}: {} failures, {} non-UTF-8 differences",
        failures.len(),
        lossy.len()
    );
}

#[test]
fn color_space_random() {
    report(
        "ColorSpace",
        run(
            "ColorSpace",
            json!([]),
            ColorSpace::new,
            cs_gen,
            cs_getters,
            1,
            300,
            14,
        ),
    );
}

#[test]
fn named_transform_random() {
    report(
        "NamedTransform",
        run(
            "NamedTransform",
            json!(["", [], "", "", {"transform": {"class": "MatrixTransform"}}]),
            || {
                let mut n = NamedTransform::new();
                let m: ocio::Transform = ocio::MatrixTransform::new().into();
                n.set_transform(Some(&m), ocio::TransformDirection::Forward)
                    .unwrap();
                n
            },
            nt_gen,
            nt_getters,
            2,
            300,
            12,
        ),
    );
}

#[test]
fn view_transform_random() {
    report(
        "ViewTransform",
        run(
            "ViewTransform",
            json!([{"enum": "REFERENCE_SPACE_SCENE"}]),
            || ViewTransform::new(ReferenceSpaceType::Scene),
            vt_gen,
            vt_getters,
            3,
            200,
            10,
        ),
    );
}

#[test]
fn look_random() {
    report(
        "Look",
        run(
            "Look",
            json!([]),
            Look::new,
            look_gen,
            |_, _| Vec::new(),
            4,
            200,
            8,
        ),
    );
}

// ---------------------------------------------------------------------------------------------
// Color space sets.

use ocio::ColorSpaceSet;

const SET_NAMES: &[&[u8]] = &[b"a", b"b", b"c", b"A", b"d", b"x"];
const SET_ALIASES: &[&[u8]] = &[b"x", b"y", b"a", b"B", b"z"];

/// A color space of a random set: its name and aliases.
type SetSpace = (&'static [u8], Vec<&'static [u8]>);

fn random_sets(rng: &mut Rng, count: usize) -> Vec<Vec<SetSpace>> {
    (0..count)
        .map(|_| {
            (0..rng.next() % 4)
                .map(|_| {
                    let name = rng.pick(SET_NAMES);
                    let aliases = (0..rng.next() % 3).map(|_| rng.pick(SET_ALIASES)).collect();
                    (name, aliases)
                })
                .collect()
        })
        .collect()
}

#[test]
fn color_space_set_operations_random() {
    let mut rng = Rng(77);
    let sets = random_sets(&mut rng, 24);
    let mut calls = Vec::new();
    let mut port_sets = Vec::new();
    let mut failures = Vec::new();
    let mut add_checks = Vec::new();
    for (s, spaces) in sets.iter().enumerate() {
        calls.push(json!({"new": "ColorSpaceSet", "as": format!("s{s}")}));
        let mut css = ColorSpaceSet::new();
        for (k, (name, aliases)) in spaces.iter().enumerate() {
            let cs = format!("s{s}c{k}");
            calls.push(json!({"new": "ColorSpace", "as": cs}));
            calls.push(json!({"call": "setName", "on": cs, "args": [bytes_arg(name)]}));
            let mut c = ColorSpace::new();
            c.set_name(name);
            for alias in aliases {
                calls.push(json!({"call": "addAlias", "on": cs, "args": [bytes_arg(alias)]}));
                c.add_alias(alias);
            }
            add_checks.push((calls.len(), err(css.add_color_space(&c))));
            calls.push(
                json!({"call": "addColorSpace", "on": format!("s{s}"), "args": [{"ref": cs}]}),
            );
        }
        port_sets.push(css);
    }
    let ops = ["__or__", "__and__", "__sub__"];
    let mut op_calls = Vec::new();
    for a in 0..sets.len() {
        for b in 0..sets.len() {
            for op in ops {
                op_calls.push((a, b, op, calls.len()));
                calls.push(json!({"call": op, "on": format!("s{a}"), "args": [{"ref": format!("s{b}")}], "as": format!("{op}{a}_{b}")}));
            }
            op_calls.push((a, b, "__eq__", calls.len()));
            calls.push(json!({"call": "__eq__", "on": format!("s{a}"), "args": [{"ref": format!("s{b}")}]}));
            op_calls.push((a, b, "__ne__", calls.len()));
            calls.push(json!({"call": "__ne__", "on": format!("s{a}"), "args": [{"ref": format!("s{b}")}]}));
        }
    }
    let response = Oracle::get()
        .call(
            "config_calls",
            json!({"config": "new", "calls": calls}),
            &[],
        )
        .result;
    let results = response["calls"].as_array().unwrap().clone();
    for (i, port) in &add_checks {
        let w = wheel_r(&results[*i]);
        if &w != port {
            failures.push(format!("add {}: wheel {w:?} port {port:?}", calls[*i]));
        }
    }
    // Phase 2: the names of each result the wheel produced.
    let mut names_calls = calls.clone();
    let mut name_checks = Vec::new();
    for &(a, b, op, i) in &op_calls {
        let w = &results[i];
        let pa = &port_sets[a];
        let pb = &port_sets[b];
        match op {
            "__eq__" | "__ne__" => {
                let p = if op == "__eq__" { pa == pb } else { pa != pb };
                if wheel_r(w) != R::Bool(p) {
                    failures.push(format!("s{a} {op} s{b}: wheel {w} port {p}"));
                }
            }
            _ => {
                let port = match op {
                    "__or__" => ColorSpaceSet::union(pa, pb),
                    "__and__" => ColorSpaceSet::intersection(pa, pb),
                    _ => ColorSpaceSet::difference(pa, pb),
                };
                match (w.get("exception"), port) {
                    (Some(_), Err(e)) => {
                        if wheel_r(w) != R::Err(e.what().to_vec()) {
                            failures.push(format!(
                                "s{a} {op} s{b}: wheel {w} port {}",
                                String::from_utf8_lossy(e.what())
                            ));
                        }
                    }
                    (None, Ok(css)) => {
                        let names: Vec<Vec<u8>> = (0..css.num_color_spaces())
                            .map(|k| css.color_space_name_by_index(k).unwrap().to_vec())
                            .collect();
                        name_checks.push((names_calls.len(), a, b, op, names));
                        names_calls.push(
                            json!({"call": "getColorSpaceNames", "on": format!("{op}{a}_{b}")}),
                        );
                    }
                    (w, p) => failures.push(format!(
                        "s{a} {op} s{b}: wheel {w:?} port {:?}",
                        p.map(|_| ())
                            .map_err(|e| String::from_utf8_lossy(e.what()).into_owned())
                    )),
                }
            }
        }
    }
    let response = Oracle::get()
        .call(
            "config_calls",
            json!({"config": "new", "calls": names_calls}),
            &[],
        )
        .result;
    let results = response["calls"].as_array().unwrap();
    for (i, a, b, op, port) in name_checks {
        let wheel = texts(&results[i]["result"]);
        if wheel != port {
            failures.push(format!("s{a} {op} s{b}: wheel {wheel:?} port {port:?}"));
        }
    }
    println!(
        "set ops: {} op calls, {} failures",
        op_calls.len(),
        failures.len()
    );
    for f in failures.iter().take(40) {
        println!("  FAIL {f}");
    }
    assert!(failures.is_empty());
}

// ---------------------------------------------------------------------------------------------
// Killing tests for the survivors.

fn i73_pair() -> (Value, ocio::Transform, Value, ocio::Transform) {
    let mut matrix = ocio::MatrixTransform::new();
    matrix.set_offset(&[0.1, 1.0 / 3.0, 0.0, 0.0]);
    let mut range = ocio::RangeTransform::new();
    range.set_min_in_value(0.123_456_789_123_4);
    (
        json!({"class": "MatrixTransform", "calls": [["setOffset", [0.1, 1.0 / 3.0, 0.0, 0.0]]]}),
        matrix.into(),
        json!({"class": "RangeTransform", "calls": [["setMinInValue", 0.123_456_789_123_4]]}),
        range.into(),
    )
}

fn repr_of(calls: Vec<Value>, name: &str) -> Vec<u8> {
    let mut calls = calls;
    calls.push(json!({"call": "__repr__", "on": name}));
    let response = Oracle::get()
        .call(
            "config_calls",
            json!({"config": "new", "calls": calls}),
            &[],
        )
        .result;
    let results = response["calls"].as_array().unwrap();
    for r in results {
        assert!(r.get("result").is_some(), "{r}");
    }
    bytes(&results.last().unwrap()["result"])
}

#[test]
fn view_transform_shares_the_stream_between_its_transforms() {
    let (ms, mp, rs, rp) = i73_pair();
    let wheel = repr_of(
        vec![
            json!({"new": "ViewTransform", "args": [{"enum": "REFERENCE_SPACE_SCENE"}], "as": "v"}),
            json!({"call": "setTransform", "on": "v", "args": [{"transform": ms}, {"enum": "VIEWTRANSFORM_DIR_TO_REFERENCE"}]}),
            json!({"call": "setTransform", "on": "v", "args": [{"transform": rs}, {"enum": "VIEWTRANSFORM_DIR_FROM_REFERENCE"}]}),
        ],
        "v",
    );
    let mut v = ViewTransform::new(ReferenceSpaceType::Scene);
    v.set_transform(Some(&mp), ocio::ViewTransformDirection::ToReference)
        .unwrap();
    v.set_transform(Some(&rp), ocio::ViewTransformDirection::FromReference)
        .unwrap();
    println!("{}", String::from_utf8_lossy(&wheel));
    assert_eq!(wheel, v.to_bytes());
}

#[test]
fn named_transform_shares_the_stream_between_its_transforms() {
    let (ms, mp, rs, rp) = i73_pair();
    let wheel = repr_of(
        vec![
            json!({"new": "NamedTransform", "as": "n"}),
            json!({"call": "setTransform", "on": "n", "args": [{"transform": ms}, {"enum": "TRANSFORM_DIR_FORWARD"}]}),
            json!({"call": "setTransform", "on": "n", "args": [{"transform": rs}, {"enum": "TRANSFORM_DIR_INVERSE"}]}),
        ],
        "n",
    );
    let mut n = NamedTransform::new();
    n.set_transform(Some(&mp), ocio::TransformDirection::Forward)
        .unwrap();
    n.set_transform(Some(&rp), ocio::TransformDirection::Inverse)
        .unwrap();
    println!("{}", String::from_utf8_lossy(&wheel));
    assert_eq!(wheel, n.to_bytes());
}

#[test]
fn one_allocation_variable_prints_the_allocation() {
    let yaml = "ocio_profile_version: 2\nroles: {default: x}\ncolorspaces:\n  - !<ColorSpace> {name: x, allocation: lg2, allocationvars: [0.5]}\n";
    let response = Oracle::get()
        .call(
            "config_calls",
            json!({"config": {"yaml": yaml}, "calls": [{"call": "getColorSpace", "args": ["x"]}]}),
            &[],
        )
        .result;
    let r = &response["calls"][0]["result"];
    let wheel = bytes(&r["repr"]);
    let mut c = ColorSpace::new();
    c.set_name("x");
    c.set_allocation(ocio::Allocation::Lg2);
    c.set_allocation_vars(&[0.5]);
    println!("{}", String::from_utf8_lossy(&wheel));
    assert_eq!(wheel, c.to_bytes());
}

#[test]
fn whitespace_only_category() {
    let response = Oracle::get()
        .call(
            "config_calls",
            json!({"config": "new", "calls": [
                {"new": "ColorSpace", "as": "c"},
                {"call": "addCategory", "on": "c", "args": ["  "]},
                {"call": "hasCategory", "on": "c", "args": [""]},
                {"call": "removeCategory", "on": "c", "args": [""]},
                {"call": "getCategories", "on": "c"},
                {"call": "__repr__", "on": "c"},
            ]}),
            &[],
        )
        .result;
    let r = response["calls"].as_array().unwrap();
    let mut c = ColorSpace::new();
    c.add_category("  ");
    assert_eq!(r[2]["result"].as_bool().unwrap(), c.has_category(""));
    c.remove_category("");
    assert_eq!(texts(&r[4]["result"]).len(), c.num_categories() as usize);
    assert_eq!(bytes(&r[5]["result"]), c.to_bytes());
    println!("{}", String::from_utf8_lossy(&bytes(&r[5]["result"])));
}
