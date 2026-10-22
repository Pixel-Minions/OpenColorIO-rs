// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Keyword arguments as pybind11's dispatcher binds them, against the wheel, over the bound
//! classes' functions as the wheel's own surface describes them (py_surface: each function's
//! docstring, its overloads' signatures):
//! - an overload bound without argument names (its parameters `self`, `arg0`, `arg1`, ...) or
//!   with positional-only ones (`/`) takes no keyword: `Class.method(self=obj, arg0=...)` and
//!   `obj.method(arg0=...)` raise the wheel's "incompatible function arguments" (an operator
//!   returns `NotImplemented`), for every method, operator, iterator method, property getter and
//!   static method of the classes bound so far, and of every enum; the arguments are values of
//!   the parameters' types, which a positional call would take, so that binding them by name
//!   would run the function;
//! - an overload with names takes them, `self` included: each method whose parameters are text
//!   (or a `dict`) called as `Class.method(self=obj, name='a', ...)` and as
//!   `obj.method(name='a', ...)` runs as the wheel's.
//!
//! The wheel never runs the refused calls, so any value is safe there. Each step's report is
//! compared with the wheel's, and each step's outcome in the wheel is the one its overloads'
//! names call for.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, by, d, f, i, n, none, r, ra, s};
use ocio_testkit::oracle_values::bytes;
use serde_json::{Value, json};

/// The classes bound so far whose functions the test calls, with their nested classes (the
/// iterators), as the wheel's module path names them.
const CLASSES: [&str; 31] = [
    "Transform",
    "GroupTransform",
    "MatrixTransform",
    "RangeTransform",
    "AllocationTransform",
    "ExponentTransform",
    "ExponentWithLinearTransform",
    "LogTransform",
    "LogAffineTransform",
    "LogCameraTransform",
    "FixedFunctionTransform",
    "BuiltinTransform",
    "BuiltinTransformRegistry",
    "CDLTransform",
    "FileTransform",
    "FormatMetadata",
    "Context",
    "ProcessorMetadata",
    "FileRules",
    "ViewingRules",
    "ColorSpace",
    "ColorSpaceSet",
    "Look",
    "NamedTransform",
    "ViewTransform",
    "ColorSpaceTransform",
    "DisplayViewTransform",
    "LookTransform",
    "BuiltinConfigRegistry",
    "vector_of_uint8_t",
    "PyConfigIOProxy",
];

/// The nested classes no function of the module returns: no instance to call them with.
const UNREACHABLE: [&str; 1] = ["FormatMetadata.ConstChildElementIterator"];

/// Methods the named calls leave out: they write files or search the file system.
const NOT_RUN: [&str; 2] = ["write", "resolveFileLocation"];

/// The binary operators, which return `NotImplemented` for arguments no overload takes.
const OPERATORS: [&str; 5] = ["__eq__", "__ne__", "__sub__", "__or__", "__and__"];

/// Steps that make an instance of each class (by its path): the index of the instance's step.
fn instances(sc: &mut Script) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut put = |name: &str, step: usize| out.push((name.to_string(), step));
    for c in [
        "MatrixTransform",
        "RangeTransform",
        "AllocationTransform",
        "ExponentTransform",
        "ExponentWithLinearTransform",
        "LogTransform",
        "LogAffineTransform",
        "BuiltinTransform",
        "CDLTransform",
        "FileTransform",
        "FileRules",
        "ColorSpaceTransform",
        "DisplayViewTransform",
        "LookTransform",
    ] {
        let k = sc.call(n(&format!("OCIO.{c}")), vec![]);
        put(c, k);
        if c == "MatrixTransform" {
            put("Transform", k);
        }
    }
    let lc = sc.call_kw(
        n("OCIO.LogCameraTransform"),
        vec![],
        vec![("linSideBreak", json!({ "list": [f(0.1), f(0.1), f(0.1)] }))],
        Expect::Value,
    );
    put("LogCameraTransform", lc);
    let ff = sc.call(
        n("OCIO.FixedFunctionTransform"),
        vec![n("OCIO.FixedFunctionStyle.FIXED_FUNCTION_ACES_GLOW_03")],
    );
    put("FixedFunctionTransform", ff);
    let group = sc.call(n("OCIO.GroupTransform"), vec![]);
    let child = sc.call(n("OCIO.MatrixTransform"), vec![]);
    sc.call(ra(group, "appendTransform"), vec![r(child)]);
    put("GroupTransform", group);
    put(
        "GroupTransform.TransformIterator",
        sc.call(ra(group, "__iter__"), vec![]),
    );
    put(
        "GroupTransform.WriteFormatIterator",
        sc.call(n("OCIO.GroupTransform.GetWriteFormats"), vec![]),
    );
    put(
        "FileTransform.FormatIterator",
        sc.call(n("OCIO.FileTransform.getFormats"), vec![]),
    );
    let bytes = sc.call(
        n("OCIO.vector_of_uint8_t"),
        vec![json!({ "list": [i(1), i(2)] })],
    );
    put("vector_of_uint8_t", bytes);
    let proxy = sc.call(n("OCIO.PyConfigIOProxy"), vec![]);
    put("PyConfigIOProxy", proxy);
    let configs = sc.call(n("OCIO.BuiltinConfigRegistry"), vec![]);
    put("BuiltinConfigRegistry", configs);
    put(
        "BuiltinConfigRegistry.BuiltinConfigNameIterator",
        sc.call(ra(configs, "__iter__"), vec![]),
    );
    put(
        "BuiltinConfigRegistry.BuiltinConfigIterator",
        sc.call(ra(configs, "getBuiltinConfigs"), vec![]),
    );
    let registry = sc.call(n("OCIO.BuiltinTransformRegistry"), vec![]);
    put("BuiltinTransformRegistry", registry);
    put(
        "BuiltinTransformRegistry.BuiltinStyleIterator",
        sc.call(ra(registry, "__iter__"), vec![]),
    );
    put(
        "BuiltinTransformRegistry.BuiltinIterator",
        sc.call(ra(registry, "getBuiltins"), vec![]),
    );
    let cdl = sc.call(n("OCIO.CDLTransform"), vec![]);
    let fm = sc.call(ra(cdl, "getFormatMetadata"), vec![]);
    sc.call(ra(fm, "__setitem__"), vec![s("attr"), s("v")]);
    sc.call(ra(fm, "addChildElement"), vec![s("Child"), s("c")]);
    put("FormatMetadata", fm);
    put(
        "FormatMetadata.AttributeNameIterator",
        sc.call(ra(fm, "__iter__"), vec![]),
    );
    put(
        "FormatMetadata.AttributeIterator",
        sc.call(ra(fm, "getAttributes"), vec![]),
    );
    put(
        "FormatMetadata.ChildElementIterator",
        sc.call(ra(fm, "getChildElements"), vec![]),
    );
    let ctx = sc.call(n("OCIO.Context"), vec![]);
    sc.call(ra(ctx, "setStringVar"), vec![s("A"), s("b")]);
    sc.call(ra(ctx, "addSearchPath"), vec![s("p")]);
    put("Context", ctx);
    put(
        "Context.StringVarNameIterator",
        sc.call(ra(ctx, "__iter__"), vec![]),
    );
    put(
        "Context.StringVarIterator",
        sc.call(ra(ctx, "getStringVars"), vec![]),
    );
    put(
        "Context.SearchPathIterator",
        sc.call(ra(ctx, "getSearchPaths"), vec![]),
    );
    let pm = sc.call(n("OCIO.ProcessorMetadata"), vec![]);
    sc.call(ra(pm, "addFile"), vec![s("f")]);
    sc.call(ra(pm, "addLook"), vec![s("l")]);
    put("ProcessorMetadata", pm);
    put(
        "ProcessorMetadata.FileIterator",
        sc.call(ra(pm, "getFiles"), vec![]),
    );
    put(
        "ProcessorMetadata.LookIterator",
        sc.call(ra(pm, "getLooks"), vec![]),
    );
    let vr = sc.call(n("OCIO.ViewingRules"), vec![]);
    sc.call(ra(vr, "insertRule"), vec![i(0), s("rule")]);
    sc.call(ra(vr, "addColorSpace"), vec![i(0), s("cs")]);
    sc.call(ra(vr, "insertRule"), vec![i(1), s("other")]);
    sc.call(ra(vr, "addEncoding"), vec![i(1), s("enc")]);
    put("ViewingRules", vr);
    put(
        "ViewingRules.ViewingRuleColorSpaceIterator",
        sc.call(ra(vr, "getColorSpaces"), vec![i(0)]),
    );
    put(
        "ViewingRules.ViewingRuleEncodingIterator",
        sc.call(ra(vr, "getEncodings"), vec![i(1)]),
    );
    let cs = sc.call_kw(
        n("OCIO.ColorSpace"),
        vec![],
        vec![("name", s("cs")), ("aliases", json!({ "list": [s("x")] }))],
        Expect::Value,
    );
    sc.call(ra(cs, "addCategory"), vec![s("c")]);
    put("ColorSpace", cs);
    put(
        "ColorSpace.ColorSpaceCategoryIterator",
        sc.call(ra(cs, "getCategories"), vec![]),
    );
    put(
        "ColorSpace.ColorSpaceAliasIterator",
        sc.call(ra(cs, "getAliases"), vec![]),
    );
    let css = sc.call(n("OCIO.ColorSpaceSet"), vec![]);
    sc.call(ra(css, "addColorSpace"), vec![r(cs)]);
    put("ColorSpaceSet", css);
    put(
        "ColorSpaceSet.ColorSpaceNameIterator",
        sc.call(ra(css, "getColorSpaceNames"), vec![]),
    );
    put(
        "ColorSpaceSet.ColorSpaceIterator",
        sc.call(ra(css, "getColorSpaces"), vec![]),
    );
    let look = sc.call_kw(
        n("OCIO.Look"),
        vec![],
        vec![("name", s("l"))],
        Expect::Value,
    );
    put("Look", look);
    let nt = sc.call_kw(
        n("OCIO.NamedTransform"),
        vec![],
        vec![("name", s("nt"))],
        Expect::Value,
    );
    sc.call(ra(nt, "addAlias"), vec![s("y")]);
    sc.call(ra(nt, "addCategory"), vec![s("c")]);
    put("NamedTransform", nt);
    put(
        "NamedTransform.NamedTransformCategoryIterator",
        sc.call(ra(nt, "getCategories"), vec![]),
    );
    put(
        "NamedTransform.NamedTransformAliasIterator",
        sc.call(ra(nt, "getAliases"), vec![]),
    );
    let vt = sc.call_kw(
        n("OCIO.ViewTransform"),
        vec![],
        vec![("name", s("vt"))],
        Expect::Value,
    );
    sc.call(ra(vt, "addCategory"), vec![s("c")]);
    put("ViewTransform", vt);
    put(
        "ViewTransform.ViewTransformCategoryIterator",
        sc.call(ra(vt, "getCategories"), vec![]),
    );
    out
}

/// A class of the wheel's surface: its path and its members' entries.
struct Class {
    path: String,
    members: Vec<Value>,
    /// An enum's first entry's name.
    first_entry: Option<String>,
}

fn text(v: &Value) -> String {
    String::from_utf8(bytes(v)).unwrap()
}

/// The classes of the wheel's surface under `members` (a module's or a class's), recursively,
/// each with its path from `prefix`.
fn classes(members: &Value, prefix: &str, out: &mut Vec<Class>) {
    for m in members.as_array().unwrap() {
        if m["kind"] != "class" {
            continue;
        }
        let name = text(&m["name"]);
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}.{name}")
        };
        let first_entry = m["entries"]
            .as_array()
            .and_then(|e| e.first())
            .map(|e| e[0].as_str().unwrap().to_string());
        out.push(Class {
            path: path.clone(),
            members: m["members"].as_array().unwrap().clone(),
            first_entry,
        });
        classes(&m["members"], &path, out);
    }
}

/// `text` split at the commas outside brackets, quotes and `<...>` (a signature's parameters).
fn split_params(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            cur.push(c);
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '[' | '(' | '<' | '{' => depth += 1,
            ']' | ')' | '>' | '}' => depth -= 1,
            ',' if depth == 0 && chars.peek() == Some(&' ') => {
                chars.next();
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// A parameter of a signature: its name, its type, whether it has a default.
#[derive(Debug)]
struct Param {
    name: String,
    ty: String,
    default: bool,
}

/// An overload's signature as its docstring prints it.
#[derive(Debug)]
struct Signature {
    params: Vec<Param>,
    /// It has a `/` (`py::pos_only()`).
    pos_only: bool,
}

impl Signature {
    /// Its first parameter is a method's `self`.
    fn method(&self) -> bool {
        self.params.first().is_some_and(|p| p.name == "self")
    }

    /// Bound without argument names: each parameter after a method's `self` named `arg` and
    /// its position, as pybind11 names a parameter it has no record of.
    fn unnamed(&self) -> bool {
        let skip = usize::from(self.method());
        !self.pos_only
            && self
                .params
                .iter()
                .skip(skip)
                .enumerate()
                .all(|(k, p)| p.name == format!("arg{k}"))
    }
}

/// The signatures of a function's docstring `doc`: one line `name(params) -> ret`, or
/// "Overloaded function." and each numbered (a property getter's name may be empty).
fn signatures(doc: &str) -> Vec<Signature> {
    let mut out = Vec::new();
    for line in doc.lines() {
        let rest = match line.split_once(". ") {
            Some((num, rest)) if num.chars().all(|c| c.is_ascii_digit()) => rest,
            _ => line,
        };
        let Some(open) = rest.find('(') else {
            continue;
        };
        if !rest[..open]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            continue;
        }
        let after = &rest[open + 1..];
        if after.starts_with("*args, **kwargs)") {
            continue;
        }
        // The parameters end at the `)` that closes the first `(`.
        let mut depth = 1;
        let mut end = None;
        for (k, c) in after.char_indices() {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(k);
                        break;
                    }
                }
                _ => {}
            }
        }
        let inner = &after[..end.unwrap_or_else(|| panic!("{line}"))];
        assert!(after[end.unwrap()..].starts_with(") -> "), "{line}");
        let mut params = Vec::new();
        let mut pos_only = false;
        for p in split_params(inner) {
            if p == "/" {
                pos_only = true;
                continue;
            }
            let (pname, rest) = p.split_once(": ").unwrap_or_else(|| panic!("{line}: {p}"));
            let (ty, default) = match rest.split_once(" = ") {
                Some((ty, _)) => (ty, true),
                None => (rest, false),
            };
            params.push(Param {
                name: pname.to_string(),
                ty: ty.to_string(),
                default,
            });
        }
        out.push(Signature { params, pos_only });
    }
    out
}

/// A value of the parameter type `ty`, which a positional call takes: an instance of a class
/// (the step of `instances`), or `None` for a type the test makes no value of.
fn value_of(ty: &str, instances: &[(String, usize)]) -> Value {
    match ty {
        "str" => s("a"),
        "typing.SupportsInt" => i(0),
        "typing.SupportsFloat" => f(0.5),
        "bool" => b(false),
        "bytes" => by(b""),
        "dict" => d(vec![]),
        _ => ty
            .strip_prefix("PyOpenColorIO.PyOpenColorIO.")
            .and_then(|c| instances.iter().find(|(p, _)| p == c))
            .map_or_else(none, |(_, k)| r(*k)),
    }
}

/// What a generated step expects of the wheel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// The keywords bind no overload: "incompatible function arguments" (`NotImplemented` for
    /// an operator).
    Refused,
    /// The keywords bind: whatever the function does, not that error.
    Bound,
}

/// The script: the instances' steps, then the generated calls, each listed in `expect` with
/// its outcome in the wheel and the function it calls.
struct Generated {
    sc: Script,
    expect: Vec<(usize, Outcome, String)>,
}

/// The calls of each function of the classes (and enums) of the wheel's surface `report`.
fn generate(report: &Value) -> Generated {
    let mut all = Vec::new();
    classes(&report["module"]["members"], "", &mut all);
    let mut sc = Script::new();
    let mut objects = instances(&mut sc);
    let mut expect = Vec::new();
    let mut covered = Vec::new();
    for class in &all {
        let top = class.path.split('.').next().unwrap();
        // The module's enums, and those of the classes above (the classes of later cards
        // aren't bound yet, nor their enums).
        let enum_bound = !class.path.contains('.') || CLASSES.contains(&top);
        let instance = if let Some(entry) = class.first_entry.as_ref().filter(|_| enum_bound) {
            let k = sc.eval(n(&format!("OCIO.{}.{entry}", class.path)), &[]);
            objects.push((class.path.clone(), k));
            k
        } else if class.first_entry.is_none() && CLASSES.contains(&top) {
            if UNREACHABLE.contains(&class.path.as_str()) {
                continue;
            }
            objects
                .iter()
                .find(|(p, _)| *p == class.path)
                .unwrap_or_else(|| panic!("no instance of {}", class.path))
                .1
        } else {
            continue;
        };
        covered.push(class.path.clone());
        let target = |m: &str| n(&format!("OCIO.{}.{m}", class.path));
        for m in &class.members {
            let name = text(&m["name"]);
            let kind = m["kind"].as_str().unwrap();
            if ["__init__", "__setstate__", "__new__"].contains(&name.as_str()) {
                continue;
            }
            let (doc, call) = match kind {
                "instancemethod" | "builtin_function_or_method" | "staticmethod" => {
                    (&m["doc"], target(&name))
                }
                "property" if m["getter"] == true => {
                    (&m["getter_doc"], target(&format!("{name}.fget")))
                }
                _ => continue,
            };
            if doc.is_null() {
                continue;
            }
            let sigs = signatures(&text(doc));
            assert!(!sigs.is_empty(), "{}.{name}: {}", class.path, text(doc));
            let refused = sigs.iter().all(|o| o.unnamed() || o.pos_only);
            let what = format!("{}.{name}", class.path);
            if refused {
                for o in &sigs {
                    let method = o.method();
                    let kwargs: Vec<Value> = o
                        .params
                        .iter()
                        .enumerate()
                        .map(|(k, p)| {
                            let v = if method && k == 0 {
                                r(instance)
                            } else {
                                value_of(&p.ty, &objects)
                            };
                            json!([p.name, v])
                        })
                        .collect();
                    if !kwargs.is_empty() {
                        let k = sc.step(
                            json!({ "call": call.clone(), "kwargs": kwargs }),
                            Expect::Value,
                        );
                        expect.push((k, Outcome::Refused, what.clone()));
                    }
                    // `self` positional, the others by keyword.
                    if method && kind != "property" && kwargs.len() > 1 {
                        let k = sc.step(
                            json!({
                                "call": ra(instance, &name),
                                "kwargs": kwargs[1..].to_vec(),
                            }),
                            Expect::Value,
                        );
                        expect.push((k, Outcome::Refused, what.clone()));
                    }
                }
            } else if sigs[0].method()
                && kind != "property"
                && !NOT_RUN.contains(&name.as_str())
                && sigs.len() == 1
            {
                // A named overload whose parameters are text (or a dict): by keyword, `self`
                // too.
                let o = &sigs[0];
                let rest: Vec<&Param> = o.params.iter().skip(1).filter(|p| !p.default).collect();
                if !rest.is_empty() && rest.iter().all(|p| p.ty == "str" || p.ty == "dict") {
                    let kwargs: Vec<Value> = rest
                        .iter()
                        .map(|p| json!([p.name, value_of(&p.ty, &objects)]))
                        .collect();
                    let mut with_self = vec![json!(["self", r(instance)])];
                    with_self.extend(kwargs.iter().cloned());
                    let k = sc.step(
                        json!({ "call": call.clone(), "kwargs": with_self }),
                        Expect::Value,
                    );
                    expect.push((k, Outcome::Bound, what.clone()));
                    let k = sc.step(
                        json!({ "call": ra(instance, &name), "kwargs": kwargs }),
                        Expect::Value,
                    );
                    expect.push((k, Outcome::Bound, what.clone()));
                }
            }
        }
    }
    for c in CLASSES {
        assert!(covered.iter().any(|p| p == c), "{c} not in the surface");
    }
    Generated { sc, expect }
}

/// Keyword arguments bind as the wheel's dispatcher binds them (see the module's
/// documentation).
#[test]
fn keywords_bind_as_the_wheels() {
    let g = generate(&common::wheel_surface());
    let script = g.sc.json();
    let theirs = common::wheel(std::slice::from_ref(&script));
    let theirs = theirs[0]["steps"].as_array().unwrap();
    let ours = common::port(&script);
    let ours = ours["steps"].as_array().unwrap();
    assert_eq!(theirs.len(), g.sc.steps.len());
    assert_eq!(ours.len(), g.sc.steps.len());
    // The setup's steps succeed in the wheel.
    let first = g.expect.first().unwrap().0;
    for (k, step) in theirs[..first].iter().enumerate() {
        assert!(step.get("value").is_some(), "setup step {k}: {step}");
    }
    let (mut refused, mut not_implemented, mut bound, mut bound_values) = (0, 0, 0, 0);
    let mut differ = Vec::new();
    for &(k, outcome, ref what) in &g.expect {
        let step = &theirs[k];
        let incompatible = step["raised"]["type"] == "builtins.TypeError"
            && step["raised"]["message"]
                .get("bytes")
                .is_some_and(|_| text(&step["raised"]["message"]).contains("(): incompatible "));
        match outcome {
            Outcome::Refused => {
                let name = what.rsplit('.').next().unwrap();
                let ni = step["value"]["type"] == "builtins.NotImplementedType";
                assert!(
                    incompatible || (ni && OPERATORS.contains(&name)),
                    "{what}: {}\n{step}",
                    g.sc.steps[k]
                );
                refused += 1;
                not_implemented += usize::from(ni);
            }
            Outcome::Bound => {
                assert!(!incompatible, "{what}: {}\n{step}", g.sc.steps[k]);
                bound += 1;
                bound_values += usize::from(step.get("value").is_some());
            }
        }
        if ours[k] != *step {
            differ.push(format!(
                "step {k} ({what}): {}\n  port  {}\n  wheel {step}",
                g.sc.steps[k], ours[k]
            ));
        }
    }
    assert!(
        differ.is_empty(),
        "{} of {} steps differ:\n{}",
        differ.len(),
        g.expect.len(),
        differ.join("\n")
    );
    assert!(refused > 400, "{refused}");
    assert!(not_implemented >= 10, "{not_implemented}");
    assert!(bound > 100, "{bound}");
    assert!(bound_values > 80, "{bound_values}");
}
