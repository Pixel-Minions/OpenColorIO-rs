// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `py_eval` command (`oracle/ocio_oracle/py_eval_api.py` and the interpreter
//! `py_eval_steps.py`, O6.2), against the wheel itself: a script of its own makes the same
//! calls in the oracle's environment (`Oracle::run_script`, independently of the command) and
//! prints one line per fact (each result's type, repr, value, identity with earlier results,
//! getters and log, or the exception's type, MRO and message); the command's report,
//! flattened the same way, must hold the same lines in the same order. The scripts' process
//! sees nothing of the caller's environment; a process that ends is reported as a crash; bad
//! requests are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes, hex};
use serde_json::{Value, json};

fn eval(scripts: Value) -> Value {
    Oracle::get()
        .call("py_eval", json!({ "scripts": scripts }), &[])
        .result
}

fn s(text: &str) -> Value {
    json!({ "str": hex(text.as_bytes()) })
}

fn call(target: Value, args: Value) -> Value {
    json!({ "call": target, "args": args })
}

fn on(step: usize, attr: &str) -> Value {
    json!({ "ref": step, "attr": attr })
}

fn name(n: &str) -> Value {
    json!({ "name": n })
}

/// The steps both sides run, in order (the independent script's `STEPS` mirror them).
fn steps() -> Value {
    let mut config = call(name("OCIO.Config.CreateRaw"), json!([]));
    config["getters"] = json!(["getName", "getMajorVersion"]);
    let mut cdl = call(name("OCIO.CDLTransform"), json!([]));
    cdl["getters"] = json!(["getSlope", "getDirection"]);
    json!([
        config,                                                              // 0
        call(on(0, "getColorSpace"), json!([s("raw")])),                     // 1
        call(on(0, "getColorSpace"), json!([s("raw")])),                     // 2
        cdl,                                                                 // 3
        call(name("OCIO.CDLTransform"), json!([])),                          // 4
        call(name("OCIO.ExposureContrastTransform"), json!([])),             // 5
        call(on(5, "setExposure"), json!([{"float": "0x3fb999999999999a"}])), // 6
        call(on(5, "getExposure"), json!([])),                               // 7
        call(name("OCIO.Lut1DTransform"), json!([])),                        // 8
        call(on(8, "getData"), json!([])),                                   // 9
        {"eval": name("OCIO.TRANSFORM_DIR_INVERSE")},                        // 10
        call(on(0, "getColorSpace"), json!([{"int": "1"}])),                 // 11
        call(on(0, "getProcessor"), json!([s("raw"), s("none")])),           // 12
        {"eval": on(0, "nope")},                                             // 13
        call(on(4, "setID"), json!([{"str": "eda080"}])),                    // 14
        call(on(0, "getProcessor"), json!([s("raw"), s("raw")])),            // 15
        call(on(15, "getDefaultCPUProcessor"), json!([])),                   // 16
        call(name("OCIO.LogMessage"),
             json!([name("OCIO.LOGGING_LEVEL_WARNING"), s("hello log")])),   // 17
        call(name("OCIO.IsEnvVariablePresent"), json!([s("PATH")])),        // 18
        {"call": name("OCIO.CDLTransform"),
         "kwargs": [["direction", name("OCIO.TRANSFORM_DIR_INVERSE")]],
         "getters": ["getDirection", "nope"]},                               // 19
        call(name("len"), json!([{"ref": 9}])),                              // 20
        {"setattr": {"ref": 4}, "name": "nope", "value": {"none": null}},    // 21
        call(on(3, "setSlope"),
             json!([{"list": [{"float": "0x3ff8000000000000"},
                              {"int": "2"},
                              {"float": "0x7ff0000000000000"}]}])),           // 22
        call(on(3, "getSlope"), json!([])),                                  // 23
        call(name("bytes"), json!([{"bytes": "00ff41"}])),                   // 24
        call(name("tuple"), json!([{"list": [{"bool": true}, {"none": null}]}])), // 25
    ])
}

/// The independent script: the same calls as `steps()`, each result's facts as
/// `step<TAB>fact<TAB>value` lines.
const INDEPENDENT: &str = r#"
import re, struct
import PyOpenColorIO as OCIO

log = []
OCIO.SetLoggingFunction(log.append)
ADDRESS = re.compile(r" at 0x[0-9A-Fa-f]+")
IMMUTABLE = (type(None), bool, int, float, complex, str, bytes, tuple, frozenset)

def h(s):
    return s.encode("utf-8", "surrogatepass").hex()

def q(c):
    return c.__module__ + "." + c.__qualname__

def masked(s):
    return h(ADDRESS.sub(" at 0x?", s))

def out(i, fact, value):
    print(i, fact, value, sep="\t")

def describe(i, prefix, v):
    out(i, prefix + "type", q(type(v)))
    out(i, prefix + "repr", masked(repr(v)))
    if isinstance(v, bool):
        out(i, prefix + "bool", str(v))
    elif type(v) is int:
        out(i, prefix + "int", str(v))
    elif type(v) is float:
        out(i, prefix + "float", "0x" + struct.pack(">d", v).hex())
    elif type(v) is str:
        out(i, prefix + "str", h(v))
    elif type(v) is bytes:
        out(i, prefix + "bytes", v.hex())
    elif type(v) in (list, tuple):
        for k, x in enumerate(v):
            describe(i, f"{prefix}{k}.", x)
    elif type(v).__module__ == "numpy":
        out(i, prefix + "array", f"{v.dtype.str} {list(v.shape)} {list(v.strides)} {v.tobytes().hex()}")
    elif hasattr(type(v), "__members__"):
        out(i, prefix + "enum", f"{v.name}={int(v)}")

r = []

def step(i, f, getters=()):
    del log[:]
    try:
        v = f()
    except Exception as e:
        r.append(None)
        out(i, "raised", q(type(e)))
        out(i, "mro", ",".join(q(c) for c in type(e).__mro__))
        out(i, "message", masked(str(e)))
    else:
        same = [] if isinstance(v, IMMUTABLE) else [j for j, w in enumerate(r) if w is v]
        r.append(v)
        out(i, "same_as", ",".join(map(str, same)))
        describe(i, "", v)
        for g in getters:
            try:
                x = getattr(v, g)
                x = x() if callable(x) else x
            except Exception as e:
                out(i, "getter " + g + " raised", q(type(e)) + " " + masked(str(e)))
            else:
                describe(i, "getter " + g + " ", x)
    for m in log:
        out(i, "log", h(m))

def setattr_none(o):
    o.nope = None

def set_slope():
    r[3].setSlope([1.5, 2, float("inf")])

step(0, lambda: OCIO.Config.CreateRaw(), ["getName", "getMajorVersion"])
step(1, lambda: r[0].getColorSpace("raw"))
step(2, lambda: r[0].getColorSpace("raw"))
step(3, lambda: OCIO.CDLTransform(), ["getSlope", "getDirection"])
step(4, lambda: OCIO.CDLTransform())
step(5, lambda: OCIO.ExposureContrastTransform())
step(6, lambda: r[5].setExposure(struct.unpack(">d", bytes.fromhex("3fb999999999999a"))[0]))
step(7, lambda: r[5].getExposure())
step(8, lambda: OCIO.Lut1DTransform())
step(9, lambda: r[8].getData())
step(10, lambda: OCIO.TRANSFORM_DIR_INVERSE)
step(11, lambda: r[0].getColorSpace(1))
step(12, lambda: r[0].getProcessor("raw", "none"))
step(13, lambda: r[0].nope)
step(14, lambda: r[4].setID("\ud800"))
step(15, lambda: r[0].getProcessor("raw", "raw"))
step(16, lambda: r[15].getDefaultCPUProcessor())
step(17, lambda: OCIO.LogMessage(OCIO.LOGGING_LEVEL_WARNING, "hello log"))
step(18, lambda: OCIO.IsEnvVariablePresent("PATH"))
step(19, lambda: OCIO.CDLTransform(direction=OCIO.TRANSFORM_DIR_INVERSE), ["getDirection", "nope"])
step(20, lambda: len(r[9]))
step(21, lambda: setattr_none(r[4]))
step(22, set_slope)
step(23, lambda: r[3].getSlope())
step(24, lambda: bytes(bytes.fromhex("00ff41")))
step(25, lambda: tuple([True, None]))
# The wheel keeps the logging function in a static object, released after the interpreter
# has finalized: a process that ends with a Python function set crashes on its way out.
OCIO.ResetToDefaultLoggingFunction()
"#;

/// Runs `script` in the oracle's environment as the command runs its scripts: isolated,
/// without bytecode, in UTF-8 mode, with the command's environment (nothing of the caller's
/// but, on Windows, SYSTEMROOT). The lines it prints.
fn run_independent(script: &str) -> Vec<String> {
    let mut command = Oracle::get().command();
    command.env_clear();
    if cfg!(windows) {
        command.env("SYSTEMROOT", std::env::var_os("SYSTEMROOT").unwrap());
    }
    command.args(["-I", "-B", "-X", "utf8", "-c", script]);
    let output = command.output().expect("the oracle's Python runs");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        output.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout.lines().map(str::to_string).collect()
}

fn text_hex(v: &Value) -> String {
    hex(&bytes(v))
}

fn describe(i: usize, prefix: &str, v: &Value, out: &mut Vec<String>) {
    let mut line = |fact: &str, value: String| out.push(format!("{i}\t{prefix}{fact}\t{value}"));
    line("type", v["type"].as_str().unwrap().to_string());
    line("repr", text_hex(&v["repr"]));
    if let Some(b) = v.get("bool") {
        line(
            "bool",
            if b.as_bool().unwrap() {
                "True"
            } else {
                "False"
            }
            .into(),
        );
    } else if let Some(n) = v.get("int") {
        line("int", n.as_str().unwrap().into());
    } else if let Some(f) = v.get("float") {
        line("float", f.as_str().unwrap().into());
    } else if let Some(t) = v.get("str") {
        line("str", text_hex(t));
    } else if let Some(b) = v.get("bytes") {
        line("bytes", b.as_str().unwrap().into());
    } else if let Some(a) = v.get("array") {
        line(
            "array",
            format!(
                "{} {} {} {}",
                a["dtype"].as_str().unwrap(),
                list(&a["shape"]),
                list(&a["strides"]),
                a["bytes"].as_str().unwrap()
            ),
        );
    } else if let Some(e) = v.get("enum") {
        line("enum", format!("{}={}", e[0].as_str().unwrap(), e[1]));
    }
    if let Some(items) = v.get("items") {
        for (k, item) in items.as_array().unwrap().iter().enumerate() {
            describe(i, &format!("{prefix}{k}."), item, out);
        }
    }
}

/// A JSON list of integers as Python prints a list: `[a, b]`.
fn list(v: &Value) -> String {
    let items: Vec<String> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.to_string())
        .collect();
    format!("[{}]", items.join(", "))
}

/// The command's report as the independent script's lines.
fn flatten(report: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for (i, step) in report["steps"].as_array().unwrap().iter().enumerate() {
        if let Some(e) = step.get("raised") {
            out.push(format!("{i}\traised\t{}", e["type"].as_str().unwrap()));
            let mro: Vec<&str> = e["mro"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap())
                .collect();
            out.push(format!("{i}\tmro\t{}", mro.join(",")));
            out.push(format!("{i}\tmessage\t{}", text_hex(&e["message"])));
        } else {
            let same: Vec<String> = step["same_as"]
                .as_array()
                .unwrap()
                .iter()
                .map(|j| j.to_string())
                .collect();
            out.push(format!("{i}\tsame_as\t{}", same.join(",")));
            describe(i, "", &step["value"], &mut out);
            for getter in step["getters"].as_array().unwrap() {
                let g = getter[0].as_str().unwrap();
                if let Some(e) = getter[1].get("raised") {
                    out.push(format!(
                        "{i}\tgetter {g} raised\t{} {}",
                        e["type"].as_str().unwrap(),
                        text_hex(&e["message"])
                    ));
                } else {
                    describe(i, &format!("getter {g} "), &getter[1]["value"], &mut out);
                }
            }
        }
        for m in step["log"].as_array().unwrap() {
            out.push(format!("{i}\tlog\t{}", text_hex(m)));
        }
    }
    out
}

/// Every step's result, exception, identity, getters and log, as an independent script sees
/// the same calls: values of each kind (text, integers, floats by their bits, a list, a NumPy
/// array, an enum value, bytes, a tuple), objects returned twice (the same object for a
/// config's color space, a new one for each constructor call), TypeErrors of rejected
/// arguments (an int for a str, a lone surrogate), OCIO's exception, an AttributeError, a
/// repr with an address, a keyword argument, the log.
#[test]
fn reports_each_step_as_an_independent_script_sees_it() {
    let report = eval(json!([{ "steps": steps() }]));
    let ours = flatten(&report["scripts"][0]);
    let theirs = run_independent(INDEPENDENT);
    assert!(ours.len() > 60, "{ours:#?}");
    let first_difference = ours.iter().zip(&theirs).position(|(a, b)| a != b);
    assert!(
        first_difference.is_none() && ours.len() == theirs.len(),
        "first difference at line {first_difference:?} of {} reported, {} independent: {:?} vs {:?}",
        ours.len(),
        theirs.len(),
        first_difference.map(|i| &ours[i]),
        first_difference.map(|i| &theirs[i]),
    );

    // The facts the comparison relies on, where an error in both scripts could hide them.
    let steps = report["scripts"][0]["steps"].as_array().unwrap();
    assert_eq!(steps[2]["same_as"], json!([1]));
    assert_eq!(steps[4]["same_as"], json!([]));
    assert_eq!(steps[16]["value"]["address"], json!(true));
    assert!(steps[11]["raised"]["type"] == "builtins.TypeError");
    assert!(steps[14]["raised"]["type"] == "builtins.TypeError");
    assert_eq!(steps[17]["log"].as_array().unwrap().len(), 1);
    assert_eq!(steps[18]["value"]["bool"], json!(false), "{}", steps[18]);
    assert!(steps[9]["value"]["array"]["dtype"] == "<f4");
}

/// Scripts run one after another, each with its own results: a reference reaches only its
/// own script's steps.
#[test]
fn scripts_have_their_own_results() {
    let report = eval(json!([
        {"steps": [call(name("OCIO.CDLTransform"), json!([]))]},
        {"steps": [call(name("OCIO.CDLTransform"), json!([])), {"eval": {"ref": 0}}]},
    ]));
    let scripts = report["scripts"].as_array().unwrap();
    assert_eq!(scripts.len(), 2);
    assert_eq!(scripts[0]["steps"].as_array().unwrap().len(), 1);
    assert_eq!(scripts[1]["steps"][1]["same_as"], json!([0]));

    // A script's first step has no earlier result, whatever ran before it.
    let args = json!({ "scripts": [
        {"steps": [call(name("OCIO.CDLTransform"), json!([]))]},
        {"steps": [{"eval": {"ref": 0}}]},
    ] });
    assert!(Oracle::get().try_call("py_eval", args, &[]).is_err());
}

/// The scripts' process sees nothing of the caller's environment: not this test's (`PATH`,
/// Cargo's `CARGO`), nor the oracle's own (its temporary directory); on Windows, only
/// `SYSTEMROOT`, which Python needs.
#[test]
fn the_scripts_see_nothing_of_the_callers_environment() {
    assert!(std::env::var_os("PATH").is_some() && std::env::var_os("CARGO").is_some());
    let names = [
        "PATH",
        "CARGO",
        "TEMP",
        "TMP",
        "TMPDIR",
        "HOME",
        "SYSTEMROOT",
    ];
    let steps: Vec<Value> = names
        .iter()
        .map(|n| call(name("OCIO.IsEnvVariablePresent"), json!([s(n)])))
        .collect();
    let report = eval(json!([{ "steps": steps }]));
    for (n, step) in names
        .iter()
        .zip(report["scripts"][0]["steps"].as_array().unwrap())
    {
        let present = step["value"]["bool"].as_bool().unwrap();
        assert_eq!(present, *n == "SYSTEMROOT" && cfg!(windows), "{n}");
    }
}

/// A step that ends the process (`exit(3)`, a `SystemExit`) is reported as a crash.
#[test]
fn reports_a_crash() {
    let report = eval(json!([{ "steps": [call(name("exit"), json!([{"int": "3"}]))] }]));
    assert_eq!(report["crashed"]["returncode"], 3, "{report}");
}

/// Requests it can't read are refused: the request's, a script's and a step's keys,
/// expressions of an unknown kind or name, bad literals, and references to a step that is
/// later, missing or raised.
#[test]
fn bad_requests_are_refused() {
    let ok = call(name("OCIO.CDLTransform"), json!([]));
    let raised = call(name("OCIO.Config.CreateRaw"), json!([{ "int": "1" }]));
    let bad_scripts = [
        json!([{ "steps": [ok], "extra": 1 }]),
        json!([{ "steps": [{ "eval": name("OCIO"), "args": [] }] }]),
        json!([{ "steps": [{ "eval": name("OCIO"), "call": name("OCIO") }] }]),
        json!([{ "steps": [{ "setattr": { "ref": 0 }, "name": "x" }] }]),
        json!([{ "steps": [{ "eval": name("os.system") }] }]),
        json!([{ "steps": [{ "eval": { "int": "0x10" } }] }]),
        json!([{ "steps": [{ "eval": { "int": 1 } }] }]),
        json!([{ "steps": [{ "eval": { "float": "1.0" } }] }]),
        json!([{ "steps": [{ "eval": { "str": "zz" } }] }]),
        json!([{ "steps": [{ "eval": { "bool": 1 } }] }]),
        json!([{ "steps": [{ "eval": { "what": 1 } }] }]),
        json!([{ "steps": [{ "eval": { "ref": 0 } }] }]),
        json!([{ "steps": [ok, { "eval": { "ref": 1 } }] }]),
        json!([{ "steps": [raised, { "eval": { "ref": 0 } }] }]),
        json!([{ "steps": [{ "eval": { "array": { "dtype": "<f4", "bytes": "00" } } }] }]),
        json!([{ "steps": { "eval": name("OCIO") } }]),
        json!([{ "steps": [{ "eval": name("OCIO"), "getters": "getName" }] }]),
        json!([{ "steps": [{ "call": name("OCIO.CDLTransform"), "args": {} }] }]),
        json!([{ "steps": [{ "call": name("OCIO.CDLTransform"), "kwargs": [["a"]] }] }]),
        json!([{ "steps": [{ "call": name("OCIO.CDLTransform"), "kwargs": [[1, { "none": null }]] }] }]),
        json!({ "steps": [] }),
    ];
    for scripts in bad_scripts {
        let args = json!({ "scripts": scripts });
        assert!(
            Oracle::get()
                .try_call("py_eval", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
    for args in [json!({}), json!({ "scripts": [], "extra": 1 })] {
        assert!(
            Oracle::get()
                .try_call("py_eval", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
}
