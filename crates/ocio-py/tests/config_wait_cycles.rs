// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! D-9 across threads (owner decision 2026-10-23): threads whose callbacks use each other's
//! configs, each while its own call holds its own config. The wheel's calls run alongside
//! each other and all finish (upstream's `Config` takes no lock; the verifier's probe
//! `xdead2.py`); the port's would wait for each other forever, so the thread whose wait would
//! close the cycle gets D-9's RuntimeError instead, and the others finish. Without the wheel:
//! the step interpreter can't define the callbacks these need (a barrier, then a call on
//! another config). Python code runs in this test's embedded interpreter, on the port's module.
#![cfg(feature = "python")]

mod common;

use pyo3::prelude::*;
use pyo3::types::PyDict;

/// `cycle(uses)`: one thread per config, each calling `getProcessor` on its own config, whose
/// proxy's LUT loader (the first time) waits until every thread is there, then calls
/// `uses[i](configs)` and records what it returned or raised. Each config reads a LUT of its
/// own name and hash, so no file cache answers for the proxy. The threads are joined with a
/// timeout, and the barrier has one: a regression fails rather than hangs.
const CYCLE: &std::ffi::CStr = c"
import threading
Y = '''ocio_profile_version: 2
roles: {default: raw}
displays: {d: [!<View> {name: v, colorspace: raw}]}
colorspaces:
  - !<ColorSpace> {name: raw}
  - !<ColorSpace> {name: lut, from_scene_reference: !<FileTransform> {src: NAME.spi1d}}
'''
LUT = list(b'Version 1\\nFrom 0 1\\nLength 2\\nComponents 1\\n{\\n0\\n0.5\\n}\\n')
rounds = [0]

def cycle(uses):
    rounds[0] += 1
    n = len(uses)
    bar = threading.Barrier(n, timeout=20)
    cfgs = []
    out = {}
    class P(OCIO.PyConfigIOProxy):
        def __init__(s, i):
            super().__init__()
            s.i = i
            s.done = False
        def getConfigData(s):
            return Y.replace('NAME', 'r%d_%d' % (rounds[0], s.i))
        def getLutData(s, path):
            if not s.done:
                s.done = True
                bar.wait()
                try:
                    out[s.i] = ('ok', uses[s.i](cfgs))
                except Exception as e:
                    out[s.i] = (type(e).__name__, str(e))
            return LUT
        def getFastLutFileHash(s, path):
            return 'h%d_%d' % (rounds[0], s.i)
    # The proxies live as long as their configs are used (pybind11 keeps the C++ part only).
    proxies = [P(i) for i in range(n)]
    cfgs.extend(OCIO.Config.CreateFromConfigIOProxy(p) for p in proxies)
    done = {}
    def work(i):
        try:
            cfgs[i].getProcessor('raw', 'lut')
            done[i] = 'ok'
        except Exception as e:
            done[i] = repr(e)
    ts = [threading.Thread(target=work, args=(i,), daemon=True) for i in range(n)]
    for t in ts:
        t.start()
    for t in ts:
        t.join(30)
    return [t.is_alive() for t in ts], [done.get(i) for i in range(n)], \\
        [out.get(i) for i in range(n)], (cfgs, proxies)

def names(k):
    return lambda c: list(c[k].getColorSpaceNames())

spare = OCIO.Config.CreateFromStream(Y.replace('NAME', 'spare'))
def views_equal(k):
    return lambda c: OCIO.Config.AreViewsEqual(c[k], spare, 'd', 'v')
";

/// What `cycle` gave: whether each thread still runs, how each thread's call ended, what each
/// callback's use of another config returned or raised, and the configs (with their proxies).
struct Outcome {
    alive: Vec<bool>,
    done: Vec<Option<String>>,
    out: Vec<Option<(String, Py<PyAny>)>>,
    configs: Py<PyAny>,
}

/// `cycle(<uses>)`.
fn run_cycle(py: Python<'_>, globals: &Bound<'_, PyDict>, uses: &str) -> Outcome {
    let code = std::ffi::CString::new(format!("cycle([{uses}])")).unwrap();
    let result = py
        .eval(&code, Some(globals), None)
        .unwrap_or_else(|e| panic!("{uses}: {e}"));
    let item = |i: usize| result.get_item(i).unwrap();
    Outcome {
        alive: item(0).extract().unwrap(),
        done: item(1).extract().unwrap(),
        out: item(2).extract().unwrap(),
        configs: item(3).unbind(),
    }
}

/// Runs `cycle(<uses>)` and checks that every thread ended and every call succeeded, and that
/// exactly one callback got D-9's error, the others what `expected(i)` (the same call made
/// once the threads ended) gives.
fn check_cycle(
    py: Python<'_>,
    globals: &Bound<'_, PyDict>,
    uses: &str,
    expected: impl Fn(usize) -> String,
) {
    let outcome = run_cycle(py, globals, uses);
    let configs = outcome.configs;
    let n = outcome.alive.len();
    assert!(n >= 2, "{uses}");
    assert_eq!(
        outcome.alive,
        vec![false; n],
        "{uses}: threads still waiting"
    );
    assert_eq!(outcome.done, vec![Some("ok".to_string()); n], "{uses}");
    globals
        .set_item("cfgs", configs.bind(py).get_item(0).unwrap())
        .unwrap();
    globals.set_item("kept", configs).unwrap();
    let mut errors = 0;
    for (i, out) in outcome.out.iter().enumerate() {
        let (kind, value) = out
            .as_ref()
            .unwrap_or_else(|| panic!("{uses}: callback {i} didn't run"));
        if kind == "RuntimeError" {
            errors += 1;
            let text: String = value.extract(py).unwrap();
            assert!(
                text.starts_with("Config: a call waits for this config")
                    && text.ends_with("(docs/deviations.md, D-9)"),
                "{uses}: {text}"
            );
        } else {
            assert_eq!(kind, "ok", "{uses}: callback {i}");
            let again = std::ffi::CString::new(expected(i)).unwrap();
            let again = py.eval(&again, Some(globals), None).unwrap();
            assert!(
                value.bind(py).eq(&again).unwrap(),
                "{uses}: callback {i}: {} vs {}",
                value.bind(py).repr().unwrap(),
                again.repr().unwrap()
            );
        }
    }
    assert_eq!(errors, 1, "{uses}: {:?}", outcome.out);
}

/// Two threads (the verifier's `xdead2.py`), three threads round, and two threads one of which
/// takes two configs at once (`AreViewsEqual`, `with_two`): one D-9 error, the rest finish.
/// Then a thread whose callback uses a config another thread holds, without a cycle, waits
/// for it and gets its answer.
#[test]
fn threads_waiting_for_each_others_configs() {
    common::start_python();
    Python::attach(|py| {
        let globals = PyDict::new(py);
        globals
            .set_item("OCIO", py.import("PyOpenColorIO").unwrap())
            .unwrap();
        py.run(CYCLE, Some(&globals), None)
            .unwrap_or_else(|e| panic!("{e}"));
        let names = |k: usize| move |_: usize| format!("list(cfgs[{k}].getColorSpaceNames())");
        check_cycle(py, &globals, "names(1), names(0)", |i| names(1 - i)(i));
        check_cycle(py, &globals, "names(1), names(2), names(0)", |i| {
            names((i + 1) % 3)(i)
        });
        check_cycle(py, &globals, "views_equal(1), names(0)", |i| match i {
            0 => "OCIO.Config.AreViewsEqual(cfgs[1], spare, 'd', 'v')".to_string(),
            _ => names(0)(i),
        });

        // No cycle: the second thread's callback waits for the first thread's config, whose
        // callback uses the spare config only.
        let outcome = run_cycle(
            py,
            &globals,
            "lambda c: list(spare.getColorSpaceNames()), names(0)",
        );
        assert_eq!(outcome.alive, [false, false]);
        assert_eq!(
            outcome.done,
            [Some("ok".to_string()), Some("ok".to_string())]
        );
        for out in &outcome.out {
            let (kind, value) = out.as_ref().unwrap();
            assert_eq!(kind, "ok", "{value:?}");
        }
    });
}
