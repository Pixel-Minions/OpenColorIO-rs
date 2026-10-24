// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The process's current config as an embedding Rust host reads it while Python holds it
//! (`ocio::get_current_config`, the Python module's hook, `crates/ocio-py/src/embed.rs`):
//! - the same `Arc` while nothing changed, its processors cached, whatever Python read
//!   meanwhile (upstream's `GetCurrentConfig` returns the same `shared_ptr`);
//! - a new `Arc` after each kind of change Python makes in place (the config, a color space and
//!   a transform of it, its context, its file rules; then through every collection of the
//!   config: a look and both its transforms, a named transform and both of its, a view
//!   transform, a color space's `to_scene_reference` transform, the children of a group and
//!   those of a group in it), with the change: the serialization of the `Arc` is the one Python
//!   gives (`serialize()`), and has the text the edit wrote;
//! - from a thread of its own while the host's thread holds the GIL (S1), while another
//!   thread's Python call on the config waits in its callback (S2), and from a callback of a
//!   Python call on the config (S3): an answer at once, the config as last published, never a
//!   wait for the GIL or for the Python call.
//!
//! Every read runs on a thread of its own, joined with a timeout: a deadlock fails the test.
#![cfg(feature = "python")]

mod common;

use std::ffi::CString;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use ocio::{ColorSpaceDirection, Config, Transform};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// A config of version 2.3 without the roles 2.2 requires (its validation logs errors), with a
/// color space holding a matrix, one holding only a `to_scene_reference` transform, one holding
/// groups in a group, a look and a named transform with an inverse, and a view transform.
const CONFIG: &str = "ocio_profile_version: 2.3
roles:
  default: raw
displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace> {name: raw}
  - !<ColorSpace> {name: m, from_scene_reference: !<MatrixTransform> {offset: [0.5, 0, 0, 0]}}
  - !<ColorSpace> {name: tr, to_scene_reference: !<MatrixTransform> {offset: [0.5, 0, 0, 0]}}
  - !<ColorSpace> {name: grp, from_scene_reference: !<GroupTransform> {children: [!<MatrixTransform> {offset: [0.5, 0, 0, 0]}, !<ExponentTransform> {value: [2, 2, 2, 1]}, !<GroupTransform> {children: [!<MatrixTransform> {offset: [0.25, 0, 0, 0]}]}]}}
looks:
  - !<Look> {name: lk, process_space: raw, transform: !<MatrixTransform> {offset: [0.5, 0, 0, 0]}, inverse_transform: !<MatrixTransform> {offset: [-0.5, 0, 0, 0]}}
view_transforms:
  - !<ViewTransform> {name: vt, from_scene_reference: !<MatrixTransform> {offset: [0.5, 0, 0, 0]}}
named_transforms:
  - !<NamedTransform> {name: nt, transform: !<MatrixTransform> {offset: [0.5, 0, 0, 0]}, inverse_transform: !<MatrixTransform> {offset: [-0.5, 0, 0, 0]}}
";

/// `ocio::get_current_config()` on a thread of its own, waited for at most a minute.
fn read() -> Arc<Config> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(ocio::get_current_config().unwrap());
    });
    rx.recv_timeout(Duration::from_secs(60))
        .expect("get_current_config: no answer within a minute")
}

fn run(py: Python<'_>, globals: &Bound<'_, PyDict>, code: &str) {
    py.run(&CString::new(code).unwrap(), Some(globals), None)
        .unwrap_or_else(|e| panic!("{code}: {e}"));
}

/// The current configs [`read_from_rust`] read.
static IN_CALLBACK: Mutex<Vec<Arc<Config>>> = Mutex::new(Vec::new());

/// A logging function that reads the current config from Rust, on the thread of the call that
/// logs.
#[pyfunction]
fn read_from_rust(_message: &str) {
    IN_CALLBACK
        .lock()
        .unwrap()
        .push(ocio::get_current_config().unwrap());
}

/// The serialization of `config`, which is the one Python's `g.serialize()` gives (the config
/// Python holds, with the change).
fn serialization_of(py: Python<'_>, g: &Bound<'_, PyDict>, config: &Config) -> String {
    let python: String = py
        .eval(c"g.serialize()", Some(g), None)
        .unwrap()
        .extract()
        .unwrap();
    let host = String::from_utf8(config.serialize().unwrap()).unwrap();
    assert_eq!(host, python);
    host
}

/// The offset of the matrix the color space `m` holds.
fn offset(c: &Config) -> [f64; 4] {
    match c
        .color_space("m")
        .unwrap()
        .transform(ColorSpaceDirection::FromReference)
    {
        Some(Transform::Matrix(m)) => m.offset(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn rust_reads_the_current_config_python_holds() {
    ocio::set_env_variable("OCIO", None).unwrap();
    common::start_python();
    Python::attach(|py| {
        let g = PyDict::new(py);
        g.set_item("OCIO", py.import("PyOpenColorIO").unwrap())
            .unwrap();
        g.set_item("CONFIG", CONFIG).unwrap();
        g.set_item(
            "read_from_rust",
            wrap_pyfunction!(read_from_rust, py).unwrap(),
        )
        .unwrap();
        run(
            py,
            &g,
            "import threading, time\n\
             OCIO.SetCurrentConfig(OCIO.Config.CreateFromStream(CONFIG))\n\
             g = OCIO.GetCurrentConfig()",
        );

        // S1 (each read): this thread holds the GIL. Nothing changed: the same Arc, and its
        // processors cached.
        let first = read();
        assert!(Arc::ptr_eq(&first, &read()));
        let processor = first.processor_between("raw", "m").unwrap();
        assert!(Arc::ptr_eq(
            &processor,
            &read().processor_between("raw", "m").unwrap()
        ));
        // What Python reads changes nothing.
        run(
            py,
            &g,
            "g.getName(); list(g.getColorSpaceNames())\n\
             cs = g.getColorSpace('m'); cs.getDescription()\n\
             t = cs.getTransform(OCIO.COLORSPACE_DIR_FROM_REFERENCE); t.getOffset()\n\
             ctx = g.getCurrentContext(); ctx.getSearchPath()\n\
             fr = g.getFileRules(); fr.getNumEntries()\n\
             FROM = OCIO.COLORSPACE_DIR_FROM_REFERENCE\n\
             lk = g.getLook('lk'); lt = lk.getTransform(); lki = lk.getInverseTransform()\n\
             nt = g.getNamedTransform('nt')\n\
             ntt = nt.getTransform(OCIO.TRANSFORM_DIR_FORWARD)\n\
             nti = nt.getTransform(OCIO.TRANSFORM_DIR_INVERSE)\n\
             vt = g.getViewTransform('vt')\n\
             vtt = vt.getTransform(OCIO.VIEWTRANSFORM_DIR_FROM_REFERENCE)\n\
             trt = g.getColorSpace('tr').getTransform(OCIO.COLORSPACE_DIR_TO_REFERENCE)\n\
             gt = g.getColorSpace('grp').getTransform(FROM)\n\
             inner = gt[0]; sub = gt[2]; subin = sub[0]",
        );
        assert!(Arc::ptr_eq(&first, &read()));

        // Each kind of change Python makes in place: a new Arc with the change, then the same
        // one again.
        type Check = fn(&Config) -> bool;
        let edits: [(&str, Check); 5] = [
            ("g.setDescription('edited')", |c| {
                c.description() == b"edited"
            }),
            ("cs.setDescription('cs edited')", |c| {
                c.color_space("m").unwrap().description() == b"cs edited"
            }),
            ("t.setOffset([0.25, 0, 0, 0])", |c| {
                offset(c) == [0.25, 0.0, 0.0, 0.0]
            }),
            ("ctx.setSearchPath('luts')", |c| c.search_path() == b"luts"),
            ("fr.setDefaultRuleColorSpace('m')", |c| {
                let rules = c.file_rules().get();
                rules.color_space(rules.num_entries() - 1).unwrap() == b"m"
            }),
        ];
        let mut last = first;
        for (edit, check) in edits {
            assert!(!check(&last), "{edit}: before");
            run(py, &g, edit);
            let now = read();
            assert!(!Arc::ptr_eq(&last, &now), "{edit}");
            assert!(check(&now), "{edit}");
            serialization_of(py, &g, &now);
            assert!(Arc::ptr_eq(&now, &read()), "{edit}: again");
            last = now;
        }
        assert_eq!(offset(&last), [0.25, 0.0, 0.0, 0.0]);

        // An edit through each collection of the config, and of the transforms in them: a new
        // Arc, whose serialization is Python's and has the text the edit wrote (a value of
        // its own, in none of the texts before it: the config's first serialization lacks it
        // and so does each edit's before).
        let edits = [
            ("lk.setDescription('look desc')", "look desc"),
            ("lt.setOffset([0.8125, 0, 0, 0])", "0.8125"),
            ("lki.setOffset([0.4375, 0, 0, 0])", "0.4375"),
            ("nt.setDescription('named desc')", "named desc"),
            ("ntt.setOffset([0.3125, 0, 0, 0])", "0.3125"),
            ("nti.setOffset([0.6875, 0, 0, 0])", "0.6875"),
            ("vt.setDescription('view desc')", "view desc"),
            ("vtt.setOffset([0.1875, 0, 0, 0])", "0.1875"),
            ("trt.setOffset([0.9375, 0, 0, 0])", "0.9375"),
            ("inner.setOffset([0.34375, 0, 0, 0])", "0.34375"),
            ("subin.setOffset([0.15625, 0, 0, 0])", "0.15625"),
            (
                "sub.appendTransform(OCIO.ExponentTransform([1.5, 1.5, 1.5, 1]))",
                "1.5",
            ),
            (
                "g.addColorSpace(OCIO.ColorSpace(name='gradecs', \
                 fromReference=OCIO.GradingRGBCurveTransform()))",
                "gradecs",
            ),
            // A curve of a curve transform's value, changed in place: the transform's node
            // doesn't change, its value's curve does.
            (
                "gcv = g.getColorSpace('gradecs').getTransform(FROM).getValue()\n\
                 gcv.red.getControlPoints()[1] = OCIO.GradingControlPoint(0.40625, 0.40625)",
                "0.40625",
            ),
        ];
        let mut text = serialization_of(py, &g, &last);
        for (edit, needle) in edits {
            assert!(!text.contains(needle), "{edit}: before");
            run(py, &g, edit);
            let now = read();
            assert!(!Arc::ptr_eq(&last, &now), "{edit}");
            text = serialization_of(py, &g, &now);
            assert!(text.contains(needle), "{edit}: {text}");
            assert!(Arc::ptr_eq(&now, &read()), "{edit}: again");
            last = now;
        }

        // S2: another thread's call on the config waits in its logging function, with a change
        // made before it: the host gets the config as last published at once; the change once
        // the call ended.
        run(
            py,
            &g,
            "sem = threading.Semaphore(0)\n\
             OCIO.SetLoggingFunction(sem.acquire)\n\
             g.setName('pending')\n\
             w = threading.Thread(target=g.validate)\n\
             w.start()\n\
             while not sem._cond._waiters:\n    time.sleep(0.01)",
        );
        let during = read();
        assert!(Arc::ptr_eq(&last, &during));
        assert_ne!(during.name(), b"pending");
        run(
            py,
            &g,
            "sem.release(1000)\nw.join()\nOCIO.ResetToDefaultLoggingFunction()",
        );
        let after = read();
        assert_eq!(after.name(), b"pending");

        // S3: from a callback of a Python call on the config, with a change made before it
        // (and one that makes the validation run again): the config as last published, not
        // D-9's error; the change once the call ended.
        run(
            py,
            &g,
            "g.setName('in callback')\n\
             g.addEnvironmentVar('S3', 'x')\n\
             OCIO.SetLoggingFunction(read_from_rust)\n\
             g.validate()\n\
             OCIO.ResetToDefaultLoggingFunction()",
        );
        let seen = std::mem::take(&mut *IN_CALLBACK.lock().unwrap());
        assert!(!seen.is_empty());
        for c in &seen {
            assert!(Arc::ptr_eq(c, &after));
        }
        assert_eq!(read().name(), b"in callback");
    });
}
