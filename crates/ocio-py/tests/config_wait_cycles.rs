// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! D-9 across threads (owner decision 2026-10-23): threads whose callbacks use each other's
//! configs, each while its own call holds its own config. The wheel's calls run alongside
//! each other and all finish (upstream's `Config` takes no lock; the verifier's probe
//! `xdead2.py`); the port's would wait for each other forever, so the thread whose wait would
//! close the cycle gets D-9's RuntimeError instead, and the others finish. Without the wheel:
//! the step interpreter can't define the callbacks these need (a barrier, then a call on
//! another config). Python code runs in this test's embedded interpreter, on the port's module.
//!
//! The bookkeeping of the waits has its own cases: a wait that ended leaves no trace (a stale
//! entry would make a later wait that closes no cycle fail), a thread that waited and then got
//! the config is its owner (its callback's use of the config is D-9's busy error, not a wait
//! for itself), and the two-thread cycle is repeated.
//!
//! A regression can leave a thread waiting on a lock with the GIL held, which no Python
//! timeout interrupts: every test runs its body on a thread of its own, waited for at most
//! [`LIMIT`], so it fails rather than hangs.
#![cfg(feature = "python")]

mod common;

use std::sync::mpsc;
use std::time::Duration;

use pyo3::prelude::*;
use pyo3::types::PyDict;

/// How long a test may take (a pass takes seconds).
const LIMIT: Duration = Duration::from_secs(120);

/// Runs `test` on a thread of its own and fails if it panics or takes longer than [`LIMIT`].
fn guarded(test: impl FnOnce() + Send + 'static) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));
        let _ = tx.send(result);
    });
    match rx.recv_timeout(LIMIT) {
        Ok(Ok(())) => {}
        Ok(Err(panic)) => std::panic::resume_unwind(panic),
        Err(_) => panic!("no end within {LIMIT:?}: threads wait for each other"),
    }
}

/// The globals the scripts run in: the module, `RUN` (a number of this call's own), and `cycle`
/// and its helpers.
fn cycle_globals(py: Python<'_>) -> Bound<'_, PyDict> {
    let globals = PyDict::new(py);
    globals
        .set_item("OCIO", py.import("PyOpenColorIO").unwrap())
        .unwrap();
    static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    globals
        .set_item(
            "RUN",
            RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        )
        .unwrap();
    py.run(CYCLE, Some(&globals), None)
        .unwrap_or_else(|e| panic!("{e}"));
    globals
}

/// `cycle(uses)`: one thread per config, each calling `getProcessor` on its own config, whose
/// proxy's LUT loader (the first time) waits until every thread is there, then calls
/// `uses[i](configs)` and records what it returned or raised. Each config reads a LUT of its
/// own name and hash (`RUN` tells this test's from another's, which run alongside in this
/// process), so no file cache answers for the proxy. The threads are joined with a
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
            return Y.replace('NAME', 'r%d_%d_%d' % (RUN, rounds[0], s.i))
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
            return 'h%d_%d_%d' % (RUN, rounds[0], s.i)
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
    guarded(|| {
        common::start_python();
        Python::attach(|py| {
            let globals = cycle_globals(py);
            let names = |k: usize| move |_: usize| format!("list(cfgs[{k}].getColorSpaceNames())");
            check_cycle(py, &globals, "names(1), names(0)", |i| names(1 - i)(i));
            check_cycle(py, &globals, "names(1), names(2), names(0)", |i| {
                names((i + 1) % 3)(i)
            });
            check_cycle(py, &globals, "views_equal(1), names(0)", |i| match i {
                0 => "OCIO.Config.AreViewsEqual(cfgs[1], spare, 'd', 'v')".to_string(),
                _ => names(0)(i),
            });

            // No cycle: the second thread's callback waits for the first thread's config,
            // whose callback uses the spare config only.
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
    });
}

/// The two-thread cycle again and again (the verifier's repeat loop): each round has one
/// thread get D-9's error and the other its answer, whichever thread's wait closes the cycle.
#[test]
fn the_two_cycle_repeats() {
    guarded(|| {
        common::start_python();
        Python::attach(|py| {
            let globals = cycle_globals(py);
            for _ in 0..60 {
                check_cycle(py, &globals, "names(1), names(0)", |i| {
                    format!("list(cfgs[{}].getColorSpaceNames())", 1 - i)
                });
            }
        });
    });
}

/// The scripts of the two cases below: configs read through a proxy whose LUT loader runs
/// `hooks[(config, path)]` (a callback of the call that loads the LUT). Every wait of Python's
/// has a timeout (a thread that waits on a lock with the GIL held is caught by [`guarded`]).
const WAITS: &std::ffi::CStr = c"
import threading, queue, time
Y = '''ocio_profile_version: 2
name: NAME
roles: {default: raw}
displays: {d: [!<View> {name: v, colorspace: raw}]}
colorspaces:
  - !<ColorSpace> {name: raw}
  - !<ColorSpace> {name: l1, from_scene_reference: !<FileTransform> {src: NAME_1.spi1d}}
  - !<ColorSpace> {name: l2, from_scene_reference: !<FileTransform> {src: NAME_2.spi1d}}
  - !<ColorSpace> {name: l3, from_scene_reference: !<FileTransform> {src: NAME_3.spi1d}}
'''
LUT = list(b'Version 1\\nFrom 0 1\\nLength 2\\nComponents 1\\n{\\n0\\n0.5\\n}\\n')
hooks = {}

class Proxy(OCIO.PyConfigIOProxy):
    def __init__(s, name):
        super().__init__()
        s.name = name
    def getConfigData(s):
        return Y.replace('NAME', s.name)
    def getLutData(s, path):
        hook = hooks.get((s.name, path))
        if hook:
            hook()
        return LUT
    def getFastLutFileHash(s, path):
        return s.name + path

def outcome(f):
    try:
        return ('ok', f())
    except Exception as e:
        return (type(e).__name__, str(e))

def in_thread(f, results, key):
    t = threading.Thread(target=lambda: results.__setitem__(key, outcome(f)), daemon=True)
    t.start()
    return t

def load(c, name):
    c.getProcessor('raw', name)
    return 'done'

# A stale 'waiting' entry would make a later wait, with no cycle, fail: thread A waits for c1
# (held by B, in its callback), gets it after B, and holds c2 in a callback of its own;
# U holds c1 in a callback that uses c2, which only waits for A.
def stale_wait():
    p1, p2 = Proxy('c1'), Proxy('c2')
    c1 = OCIO.Config.CreateFromConfigIOProxy(p1)
    c2 = OCIO.Config.CreateFromConfigIOProxy(p2)
    free_b, free_a, b_in, a_in, u_in = (threading.Event() for _ in range(5))
    r = {}
    jobs = queue.Queue()
    def worker():
        while True:
            f = jobs.get()
            if f is None:
                return
            f()
    a = threading.Thread(target=worker, daemon=True)
    a.start()
    # B holds c1 (blocked in a LUT callback); A waits for c1 with a plain call.
    hooks[('c1', 'c1_1.spi1d')] = lambda: (b_in.set(), free_b.wait(30))
    b = in_thread(lambda: load(c1, 'l1'), r, 'B')
    assert b_in.wait(30)
    jobs.put(lambda: r.__setitem__('A_wait', outcome(c1.getName)))
    time.sleep(1.0)
    r['A_waited'] = 'A_wait' not in r
    free_b.set()
    b.join(30)
    deadline = time.time() + 30
    while 'A_wait' not in r and time.time() < deadline:
        time.sleep(0.01)
    # A holds c2 (blocked in a LUT callback).
    hooks[('c2', 'c2_2.spi1d')] = lambda: (a_in.set(), free_a.wait(30))
    jobs.put(lambda: r.__setitem__('A_hold', outcome(lambda: load(c2, 'l2'))))
    assert a_in.wait(30)
    # U holds c1; its callback uses c2, held by A, which waits for nothing.
    hooks[('c1', 'c1_3.spi1d')] = lambda: (u_in.set(), r.__setitem__('U_cb', outcome(c2.getName)))
    u = in_thread(lambda: load(c1, 'l3'), r, 'U')
    assert u_in.wait(30)
    time.sleep(1.0)
    r['U_waited'] = 'U_cb' not in r
    free_a.set()
    u.join(30)
    jobs.put(None)
    a.join(30)
    r['alive'] = [t.is_alive() for t in (a, b, u)]
    return r, (p1, p2, c1, c2)

# A thread that waited for a config and got it is its owner: its own callback's use of the
# config is D-9's busy error, not a wait for itself.
def waiter_then_owner():
    p = Proxy('c')
    c = OCIO.Config.CreateFromConfigIOProxy(p)
    free_b, b_in = threading.Event(), threading.Event()
    r = {}
    hooks[('c', 'c_1.spi1d')] = lambda: (b_in.set(), free_b.wait(30))
    hooks[('c', 'c_2.spi1d')] = lambda: r.__setitem__('cb', outcome(c.getName))
    b = in_thread(lambda: load(c, 'l1'), r, 'B')
    assert b_in.wait(30)
    a = in_thread(lambda: load(c, 'l2'), r, 'A')
    time.sleep(1.0)
    r['A_waited'] = 'A' not in r
    free_b.set()
    a.join(30)
    b.join(30)
    r['alive'] = [t.is_alive() for t in (a, b)]
    return r, (p, c)
";

/// The globals of [`WAITS`].
fn waits_globals(py: Python<'_>) -> Bound<'_, PyDict> {
    let globals = PyDict::new(py);
    globals
        .set_item("OCIO", py.import("PyOpenColorIO").unwrap())
        .unwrap();
    py.run(WAITS, Some(&globals), None)
        .unwrap_or_else(|e| panic!("{e}"));
    globals
}

/// The `(kind, value)` pair `outcome` made, in the entry `key` of the dict `r`.
fn entry(r: &Bound<'_, PyAny>, key: &str) -> (String, String) {
    let value = r
        .get_item(key)
        .unwrap_or_else(|_| panic!("{key} missing in {}", r.repr().unwrap()));
    value.extract().unwrap()
}

/// Whether the entry `key` of the dict `r` is true.
fn flag(r: &Bound<'_, PyAny>, key: &str) -> bool {
    r.get_item(key).unwrap().extract().unwrap()
}

/// A wait that ended leaves no trace (`stale_wait`): thread A waited for c1 (B's) and got it;
/// later U, holding c1, waits for c2 (A's), and A waits for nothing: U gets c2's name once A
/// is done, not D-9's error for a cycle that isn't there.
#[test]
fn a_wait_that_ended_leaves_no_trace() {
    guarded(|| {
        common::start_python();
        Python::attach(|py| {
            let globals = waits_globals(py);
            let r = py
                .eval(c"stale_wait()[0]", Some(&globals), None)
                .unwrap_or_else(|e| panic!("{e}"));
            let shown = r.repr().unwrap().to_string();
            // Each wait really waited (without waiting the case proves nothing).
            assert!(flag(&r, "A_waited"), "{shown}");
            assert!(flag(&r, "U_waited"), "{shown}");
            let alive: Vec<bool> = r.get_item("alive").unwrap().extract().unwrap();
            assert_eq!(alive, [false; 3], "{shown}");
            let done = ("ok".to_string(), "done".to_string());
            for key in ["B", "U", "A_hold"] {
                assert_eq!(entry(&r, key), done, "{shown}");
            }
            // The configs' names, as their texts say (`name: c1`, `name: c2`).
            assert_eq!(entry(&r, "A_wait"), ("ok".into(), "c1".into()), "{shown}");
            assert_eq!(entry(&r, "U_cb"), ("ok".into(), "c2".into()), "{shown}");
        });
    });
}

/// A thread that waited for a config and got it is its owner (`waiter_then_owner`): its own
/// callback's call on the config gets D-9's busy error. (Not recorded as the owner, the
/// callback would lock the config again, with the GIL held: [`guarded`] ends the test.)
#[test]
fn a_woken_waiter_is_the_owner() {
    guarded(|| {
        common::start_python();
        Python::attach(|py| {
            let globals = waits_globals(py);
            let r = py
                .eval(c"waiter_then_owner()[0]", Some(&globals), None)
                .unwrap_or_else(|e| panic!("{e}"));
            let shown = r.repr().unwrap().to_string();
            assert!(flag(&r, "A_waited"), "{shown}");
            let alive: Vec<bool> = r.get_item("alive").unwrap().extract().unwrap();
            assert_eq!(alive, [false; 2], "{shown}");
            let done = ("ok".to_string(), "done".to_string());
            assert_eq!(entry(&r, "A"), done, "{shown}");
            assert_eq!(entry(&r, "B"), done, "{shown}");
            assert_eq!(
                entry(&r, "cb"),
                (
                    "RuntimeError".to_string(),
                    "Config: a callback of a call on this config uses it while the library \
                     works on it, which the port can't share (docs/deviations.md, D-9)"
                        .to_string()
                ),
                "{shown}"
            );
        });
    });
}
