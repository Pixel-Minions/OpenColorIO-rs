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
        {"eval": {"dict": [[s("k"), {"int": "1"}]]}},                        // 26
        {"setitem": {"ref": 26}, "index": s("k2"), "value": {"int": "2"}},   // 27
        {"eval": {"ref": 26}},                                               // 28
        {"setattr": name("OCIO"), "name": "Leak", "value": {"int": "1"}},    // 29
        {"delattr": name("OCIO"), "name": "Leak"},                           // 30
        call(name("hasattr"), json!([name("OCIO"), s("Leak")])),             // 31
        {"eval": {"array": {"dtype": "<f4", "shape": [2, 2],
                            "bytes": "0000803f000000400000404000008040"}}},  // 32
        {"eval": {"list": [{"list": [{"list": [{"int": "1"}]}]}]}},          // 33
        {"eval": {"scalar": {"dtype": "<f4", "bytes": "0100807f"}}},         // 34
        {"call": name("OCIO.GradingRGBM"), "getters": ["red", "__repr__"]},  // 35
        {"eval": {"str": "61eda080"}},                                       // 36
        call(name("operator.getitem"), json!([{"ref": 26}, s("missing")])),  // 37
        call(name("tuple"), json!([])),                                      // 38
        call(name("tuple"), json!([])),                                      // 39
        {"eval": {"bytes": "00ff"}},                                         // 40
        call(name("OCIO.ColorSpace.__new__"), json!([name("OCIO.ColorSpace")])), // 41
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
    try:
        text = repr(v)
    except Exception as e:
        out(i, prefix + "repr_raised", q(type(e)) + " " + masked(str(e)))
    else:
        out(i, prefix + "repr", masked(text))
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
    elif type(v) is dict:
        for k, (key, x) in enumerate(v.items()):
            describe(i, f"{prefix}{k}.0.", key)
            describe(i, f"{prefix}{k}.1.", x)
    elif type(v).__module__ == "numpy" and type(v).__name__ == "ndarray":
        out(i, prefix + "array", f"{v.dtype.str} {list(v.shape)} {list(v.strides)} {v.tobytes().hex()}")
    elif type(v).__module__ == "numpy":
        out(i, prefix + "scalar", f"{v.dtype.str} {v.tobytes().hex()}")
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
step(26, lambda: {"k": 1})
def set_item():
    r[26]["k2"] = 2
step(27, set_item)
step(28, lambda: r[26])
def set_leak():
    OCIO.Leak = 1
step(29, set_leak)
def del_leak():
    del OCIO.Leak
step(30, del_leak)
step(31, lambda: hasattr(OCIO, "Leak"))
import numpy
step(32, lambda: numpy.array([[1.0, 2.0], [3.0, 4.0]], dtype="<f4"))
step(33, lambda: [[[1]]])
step(34, lambda: numpy.frombuffer(bytes.fromhex("0100807f"), dtype="<f4")[0])
step(35, lambda: OCIO.GradingRGBM(), ["red", "__repr__"])
step(36, lambda: "a" + chr(0xD800))
import operator
step(37, lambda: operator.getitem(r[26], "missing"))
step(38, lambda: tuple())
step(39, lambda: tuple())
step(40, lambda: bytes.fromhex("00ff"))
step(41, lambda: OCIO.ColorSpace.__new__(OCIO.ColorSpace))
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
    if let Some(e) = v.get("repr_raised") {
        line(
            "repr_raised",
            format!(
                "{} {}",
                e["type"].as_str().unwrap(),
                text_hex(&e["message"])
            ),
        );
    } else {
        line("repr", text_hex(&v["repr"]));
    }
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
    } else if let Some(a) = v.get("scalar") {
        line(
            "scalar",
            format!(
                "{} {}",
                a["dtype"].as_str().unwrap(),
                a["bytes"].as_str().unwrap()
            ),
        );
    } else if let Some(e) = v.get("enum") {
        line("enum", format!("{}={}", e[0].as_str().unwrap(), e[1]));
    }
    if let Some(items) = v.get("items") {
        for (k, item) in items.as_array().unwrap().iter().enumerate() {
            if let Some(pair) = item.as_array() {
                describe(i, &format!("{prefix}{k}.0."), &pair[0], out);
                describe(i, &format!("{prefix}{k}.1."), &pair[1], out);
            } else {
                describe(i, &format!("{prefix}{k}."), item, out);
            }
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
    assert_eq!(steps[28]["same_as"], json!([26]));
    assert_eq!(steps[31]["value"]["bool"], json!(false));
    assert_eq!(steps[38]["same_as"], json!([]));
    assert_eq!(steps[39]["same_as"], json!([]));
    assert!(
        steps[41]["value"].get("repr_raised").is_some(),
        "{}",
        steps[41]
    );
    assert_eq!(
        report["scripts"][0]["interpreter"],
        json!({"isolated": 1, "dont_write_bytecode": 1, "utf8_mode": 1})
    );
}

/// Each script has its own results: a reference reaches only its own script's steps.
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

/// What a script changes in its process doesn't reach the next script: the second script of a
/// request after one that sets the current config, the logging level, an environment
/// variable, attributes of OCIO and of a module, and NumPy's print options reports what it
/// reports alone.
#[test]
fn scripts_run_in_processes_of_their_own() {
    let mut raw = call(name("OCIO.Config.CreateRaw"), json!([]));
    raw["getters"] = json!([]);
    let changes = json!({"steps": [
        raw,
        call(on(0, "setName"), json!([s("leaked")])),
        call(name("OCIO.SetCurrentConfig"), json!([{"ref": 0}])),
        call(name("OCIO.SetLoggingLevel"), json!([name("OCIO.LOGGING_LEVEL_NONE")])),
        call(name("OCIO.SetEnvVariable"), json!([s("PY_EVAL_LEAK"), s("1")])),
        {"setattr": name("OCIO"), "name": "Leak", "value": {"int": "1"}},
        {"setattr": name("math"), "name": "pi", "value": {"float": "0x4008000000000000"}},
        {"call": name("numpy.set_printoptions"), "kwargs": [["precision", {"int": "1"}]]},
        call(name("OCIO.SetComputeHashFunction"), json!([name("str.upper")])),
    ]});
    let mut current = call(name("OCIO.GetCurrentConfig"), json!([]));
    current["getters"] = json!(["getName"]);
    let observe = json!({"steps": [
        current,
        call(name("OCIO.GetLoggingLevel"), json!([])),
        call(name("OCIO.IsEnvVariablePresent"), json!([s("PY_EVAL_LEAK")])),
        call(name("hasattr"), json!([name("OCIO"), s("Leak")])),
        {"eval": name("math.pi")},
        call(name("numpy.get_printoptions"), json!([])),
    ]});
    let after = eval(json!([changes, observe.clone()]));
    let alone = eval(json!([observe]));
    assert!(after["scripts"][0].get("crashed").is_none(), "{after}");
    assert_eq!(after["scripts"][1], alone["scripts"][0]);
    // The changes happened in the first script's process.
    let first = &after["scripts"][0]["steps"];
    assert!(
        first[2]["raised"].is_null() && first[8]["raised"].is_null(),
        "{first}"
    );
}

/// The scripts' process sees nothing of the caller's environment: the environment a context
/// loads holds only SYSTEMROOT on Windows (which Python needs) and what the interpreter adds
/// itself (uv's launcher on Windows, LC_CTYPE on Linux, PEP 538). Names only: the values are
/// never reported.
#[test]
fn the_scripts_see_nothing_of_the_callers_environment() {
    assert!(std::env::var_os("PATH").is_some() && std::env::var_os("CARGO").is_some());
    let report = eval(json!([{"steps": [
        call(name("OCIO.Context"), json!([])),
        call(on(0, "setEnvironmentMode"), json!([name("OCIO.ENV_ENVIRONMENT_LOAD_ALL")])),
        call(on(0, "loadEnvironment"), json!([])),
        call(name("list"), json!([{"ref": 0}])),
    ]}]));
    let names: Vec<String> = report["scripts"][0]["steps"][3]["value"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| String::from_utf8(bytes(&n["str"])).unwrap())
        .collect();
    let (required, added): (&[&str], &[&str]) = if cfg!(windows) {
        (&["SYSTEMROOT"], &["PYTHONHOME", "UV_INTERNAL__PYTHONHOME"])
    } else {
        (&[], &["LC_CTYPE"])
    };
    for n in &names {
        assert!(
            required.contains(&n.as_str()) || added.contains(&n.as_str()),
            "{n} reached the scripts: {names:?}"
        );
    }
    for n in required {
        assert!(names.iter().any(|x| x == n), "no {n}: {names:?}");
    }
}

/// A script's process that exits with a status other than 0 is reported as a crash, even
/// after it wrote its report (here a logging function set again at exit, which the wheel
/// releases after the interpreter has finalized); the other scripts still report. A script
/// that leaves a logging or hash function set doesn't crash: the interpreter resets both.
#[test]
fn reports_a_crash() {
    let ok = json!({"steps": [call(name("OCIO.CDLTransform"), json!([]))]});
    let crash = json!({"steps": [call(
        name("atexit.register"),
        json!([name("OCIO.SetLoggingFunction"), name("str.upper")]),
    )]});
    let report = eval(json!([ok, crash, ok]));
    let scripts = report["scripts"].as_array().unwrap();
    let crashed = &scripts[1]["crashed"];
    assert!(
        crashed["returncode"].as_i64().is_some_and(|c| c != 0),
        "{report}"
    );
    assert!(scripts[1].get("steps").is_none());
    for i in [0, 2] {
        assert_eq!(scripts[i]["steps"].as_array().unwrap().len(), 1, "{report}");
    }

    let left_set = eval(json!([{"steps": [
        call(name("OCIO.SetLoggingFunction"), json!([name("str.upper")])),
        call(name("OCIO.SetComputeHashFunction"), json!([name("str.upper")])),
    ]}]));
    assert!(
        left_set["scripts"][0].get("crashed").is_none(),
        "{left_set}"
    );
    assert_eq!(left_set["scripts"][0]["steps"].as_array().unwrap().len(), 2);
}

/// Requests it can't read are refused: the request's, a script's and a step's keys,
/// expressions of an unknown kind or name (the builtins that reach files, imports or code,
/// modules besides those listed), bad literals, and references to a step that is later,
/// missing or raised.
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
        json!([{ "steps": [{ "eval": name("open") }] }]),
        json!([{ "steps": [{ "eval": name("__import__") }] }]),
        json!([{ "steps": [{ "eval": name("exec") }] }]),
        json!([{ "steps": [{ "eval": name("eval") }] }]),
        json!([{ "steps": [{ "eval": name("compile") }] }]),
        json!([{ "steps": [{ "eval": name("globals") }] }]),
        json!([{ "steps": [{ "eval": name("exit") }] }]),
        json!([{ "steps": [{ "eval": name("pickle") }] }]),
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
