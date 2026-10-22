// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Step scripts of the oracle's step interpreter (`py_eval_steps.py`), written with short
//! helpers, each step with the outcome the test expects of the wheel (a value or an exception),
//! so that a comparison of the two reports can't pass on scripts that went wrong on both sides
//! the same way (a typo raising AttributeError everywhere).

use serde_json::{Value, json};

/// A dotted name: `OCIO.X`, a builtin, or a module of the interpreter's list.
pub(crate) fn n(name: &str) -> Value {
    json!({ "name": name })
}

/// The result of step `i`.
pub(crate) fn r(i: usize) -> Value {
    json!({ "ref": i })
}

/// An attribute (dotted) of step `i`'s result.
pub(crate) fn ra(i: usize, attr: &str) -> Value {
    json!({ "ref": i, "attr": attr })
}

/// A Python `float` of these bits.
pub(crate) fn fb(bits: u64) -> Value {
    json!({ "float": format!("0x{bits:016x}") })
}

/// A Python `float`.
pub(crate) fn f(x: f64) -> Value {
    fb(x.to_bits())
}

/// A Python `int`.
pub(crate) fn i(v: i128) -> Value {
    json!({ "int": v.to_string() })
}

/// A Python `str`.
pub(crate) fn s(text: &str) -> Value {
    json!({ "str": ocio_testkit::oracle_values::hex(text.as_bytes()) })
}

/// A Python `bytes`.
pub(crate) fn by(bytes: &[u8]) -> Value {
    json!({ "bytes": ocio_testkit::oracle_values::hex(bytes) })
}

/// `None`.
pub(crate) fn none() -> Value {
    json!({ "none": null })
}

/// A `bool`.
pub(crate) fn b(v: bool) -> Value {
    json!({ "bool": v })
}

/// A list.
pub(crate) fn l(items: Vec<Value>) -> Value {
    json!({ "list": items })
}

/// A tuple.
pub(crate) fn t(items: Vec<Value>) -> Value {
    json!({ "tuple": items })
}

/// A list of floats.
pub(crate) fn fl(values: &[f64]) -> Value {
    l(values.iter().map(|&v| f(v)).collect())
}

/// What the test expects of a step in the wheel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Expect {
    /// A result.
    Value,
    /// An exception.
    Raised,
}

/// A script and the outcome expected of each step.
#[derive(Debug, Default)]
pub(crate) struct Script {
    pub(crate) steps: Vec<Value>,
    pub(crate) expect: Vec<Expect>,
    /// Steps whose value's repr isn't compared (a class whose `__repr__` a later chunk binds).
    pub(crate) unrepr: Vec<usize>,
    /// Steps whose log isn't compared (until the logging functions are bound, 6.6a, the port's
    /// messages don't reach the step interpreter).
    pub(crate) unlog: Vec<usize>,
}

impl Script {
    pub(crate) fn new() -> Script {
        Script::default()
    }

    /// Adds a step; its index.
    pub(crate) fn step(&mut self, step: Value, expect: Expect) -> usize {
        self.steps.push(step);
        self.expect.push(expect);
        self.steps.len() - 1
    }

    /// `target(*args)`, which succeeds.
    pub(crate) fn call(&mut self, target: Value, args: Vec<Value>) -> usize {
        self.step(json!({ "call": target, "args": args }), Expect::Value)
    }

    /// `target(*args)`, which raises.
    pub(crate) fn raises(&mut self, target: Value, args: Vec<Value>) -> usize {
        self.step(json!({ "call": target, "args": args }), Expect::Raised)
    }

    /// `target(*args, **kwargs)`.
    pub(crate) fn call_kw(
        &mut self,
        target: Value,
        args: Vec<Value>,
        kwargs: Vec<(&str, Value)>,
        expect: Expect,
    ) -> usize {
        let kwargs: Vec<Value> = kwargs.into_iter().map(|(k, v)| json!([k, v])).collect();
        self.step(
            json!({ "call": target, "args": args, "kwargs": kwargs }),
            expect,
        )
    }

    /// The value of `expr`, with getters read from it.
    pub(crate) fn eval(&mut self, expr: Value, getters: &[&str]) -> usize {
        self.step(json!({ "eval": expr, "getters": getters }), Expect::Value)
    }

    /// `expr` that raises.
    pub(crate) fn eval_raises(&mut self, expr: Value) -> usize {
        self.step(json!({ "eval": expr }), Expect::Raised)
    }

    /// `repr(expr)`.
    pub(crate) fn repr(&mut self, expr: Value) -> usize {
        self.call(n("repr"), vec![expr])
    }

    /// Leaves the repr of step `i`'s value out of the comparison.
    pub(crate) fn unrepr(&mut self, i: usize) {
        self.unrepr.push(i);
    }

    /// Leaves step `i`'s log out of the comparison.
    pub(crate) fn unlog(&mut self, i: usize) {
        self.unlog.push(i);
    }

    /// The script as the step interpreter reads it.
    pub(crate) fn json(&self) -> Value {
        json!({ "steps": self.steps })
    }
}

/// Runs `scripts` against the port and the wheel and compares the reports step by step; each
/// step of the wheel's report must have the outcome its script expects.
pub(crate) fn compare(scripts: &[Script]) {
    let json: Vec<Value> = scripts.iter().map(Script::json).collect();
    compare_with(scripts, &super::wheel(&json));
}

/// [`compare`] with the wheel's reports `theirs` of `scripts` (run through `with_files`, say).
pub(crate) fn compare_with(scripts: &[Script], theirs: &[Value]) {
    assert_eq!(theirs.len(), scripts.len());
    for (k, (script, theirs)) in scripts.iter().zip(theirs).enumerate() {
        let ours = super::port(&script.json());
        let (ours, theirs) = (
            ours["steps"].as_array().unwrap(),
            theirs["steps"].as_array().unwrap(),
        );
        assert!(!script.steps.is_empty(), "script {k} is empty");
        assert_eq!(theirs.len(), script.steps.len(), "script {k}");
        // Every step whose outcome isn't the expected one, at once.
        let unexpected: Vec<String> = theirs
            .iter()
            .enumerate()
            .filter(|(j, b)| (b.get("raised").is_some()) != (script.expect[*j] == Expect::Raised))
            .map(|(j, b)| {
                format!(
                    "step {j}: {}
  {b}",
                    script.steps[j]
                )
            })
            .collect();
        assert!(
            unexpected.is_empty(),
            "script {k}: the wheel's outcomes are not the expected ones:
{}",
            unexpected.join(
                "
"
            )
        );
        for (j, (a, b)) in ours.iter().zip(theirs).enumerate() {
            let raised = b.get("raised").is_some();
            assert_eq!(
                raised,
                script.expect[j] == Expect::Raised,
                "script {k} step {j}: the wheel's outcome is not the expected one: {b}\n{}",
                script.steps[j]
            );
            let (mut a, mut b) = (a.clone(), b.clone());
            if script.unlog.contains(&j) {
                for v in [&mut a, &mut b] {
                    if let Some(o) = v.as_object_mut() {
                        o.remove("log");
                    }
                }
            }
            if script.unrepr.contains(&j) {
                for v in [&mut a, &mut b] {
                    if let Some(value) = v.get_mut("value").and_then(Value::as_object_mut) {
                        value.remove("repr");
                        value.remove("repr_raised");
                        value.remove("address");
                    }
                }
            }
            assert!(
                a == b,
                "script {k} step {j}: {}
where they differ: {}",
                script.steps[j],
                differences(&a, &b, "").join(
                    "
"
                )
            );
        }
        assert_eq!(ours.len(), theirs.len(), "script {k}");
    }
}

/// A dict.
pub(crate) fn d(items: Vec<(Value, Value)>) -> Value {
    let items: Vec<Value> = items.into_iter().map(|(k, v)| json!([k, v])).collect();
    json!({ "dict": items })
}

/// The base class's methods on an instance `obj` of a class: type, direction, validate, repr,
/// deepcopy.
pub(crate) fn base_methods(sc: &mut Script, obj: usize, validates: bool) {
    sc.call(ra(obj, "getTransformType"), vec![]);
    sc.call(ra(obj, "getDirection"), vec![]);
    let expect = if validates {
        Expect::Value
    } else {
        Expect::Raised
    };
    sc.step(json!({ "call": ra(obj, "validate"), "args": [] }), expect);
    sc.repr(r(obj));
    sc.call(n("OCIO.Transform.getDirection"), vec![r(obj)]);
    sc.call(n("OCIO.Transform.__repr__"), vec![r(obj)]);
    let copy = sc.call(n("copy.deepcopy"), vec![r(obj)]);
    sc.call(n("operator.is_"), vec![r(copy), r(obj)]);
    sc.repr(r(copy));
    sc.raises(ra(obj, "__deepcopy__"), vec![l(vec![])]);
    sc.raises(ra(obj, "__deepcopy__"), vec![by(b"")]);
    let copy2 = sc.call(ra(obj, "__deepcopy__"), vec![d(vec![])]);
    sc.call(n("type"), vec![r(copy2)]);
    sc.call(
        ra(obj, "setDirection"),
        vec![n("OCIO.TRANSFORM_DIR_INVERSE")],
    );
    sc.call(ra(obj, "getDirection"), vec![]);
    sc.repr(r(obj));
    sc.call(ra(copy, "getDirection"), vec![]);
    sc.raises(ra(obj, "setDirection"), vec![none()]);
    sc.raises(ra(obj, "setDirection"), vec![i(1)]);
    sc.raises(ra(obj, "setDirection"), vec![]);
    sc.call_kw(
        ra(obj, "setDirection"),
        vec![],
        vec![("direction", n("OCIO.TRANSFORM_DIR_FORWARD"))],
        Expect::Value,
    );
    sc.call(ra(obj, "getDirection"), vec![]);
}

/// Values whose bits a getter must give back: signed zeros, a NaN with a payload, both
/// infinities, the extremes of `double`, a value `float` rounds.
pub(crate) const SPECIAL_BITS: [u64; 8] = [
    0x8000_0000_0000_0000,
    0x0000_0000_0000_0000,
    0x7ff8_0000_0000_1234,
    0xfff0_0000_0000_0000,
    0x7ff0_0000_0000_0000,
    0x7fef_ffff_ffff_ffff,
    0x0000_0000_0000_0001,
    0x3fb9_9999_9999_999a,
];

pub(crate) fn special(k: usize) -> Value {
    fb(SPECIAL_BITS[k % SPECIAL_BITS.len()])
}

/// Where two reports differ: each path and the two values there.
fn differences(a: &Value, b: &Value, path: &str) -> Vec<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            keys.into_iter()
                .flat_map(|k| {
                    differences(
                        x.get(k).unwrap_or(&Value::Null),
                        y.get(k).unwrap_or(&Value::Null),
                        &format!("{path}.{k}"),
                    )
                })
                .collect()
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .flat_map(|(i, (p, q))| differences(p, q, &format!("{path}[{i}]")))
            .collect(),
        _ if a == b => Vec::new(),
        _ => vec![format!("{path}: port {a} wheel {b}")],
    }
}
