// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Python callables the library calls (chunk 6.6a) against the wheel, through the same step
//! scripts on both sides (the step interpreter defines no functions: the callables are bound
//! methods of builtins, such as `list.append`, `"h%.0s".__mod__`, `"x".__mul__`, and builtins
//! such as `int`, `len`, `str`):
//! - the logging level, `SetLoggingFunction`, `ResetToDefaultLoggingFunction` and `LogMessage`:
//!   the messages a logging function receives, the exceptions it raises passed through the
//!   library (from `LogMessage` and from `Config.validate`), the library usable after them;
//! - `SetComputeHashFunction` and `ResetComputeHashFunction`, on a config read from files: the
//!   cache IDs and processors the hashes give, a result pybind11 can't cast, an exception, the
//!   empty function (`None`: `std::bad_function_call`, whose text is each C++ runtime's), the
//!   library usable after them;
//! - a `PyConfigIOProxy` the library calls (`CreateFromConfigIOProxy`, `getConfigIOProxy`,
//!   `setConfigIOProxy`): its config, its LUT data and hashes, the errors of its overrides as
//!   the library reports them (`error_already_set::what()` in the LUT loader's message, with
//!   the frames of an exception raised in Python code: the same up to the port's outermost
//!   frame, `run`, where the wheel's step interpreter has its script's `main` and `<module>`
//!   too);
//! - `error_already_set::what()` of a logging function's exceptions in the config reader's
//!   message: an empty message, notes (one not UTF-8, one not a `str`), a message not UTF-8,
//!   and a `str()` that raises, without a traceback or in a Python frame (the exceptions are
//!   raised in a stdlib generator's frame by `copy.copy`, so the frames `what()` lists are the
//!   same on both sides);
//! - where the library catches a callback's exception (the file loader, `GroupTransform::write`,
//!   `Config::serialize`) and where a proxy's LUT loader fails for an absolute path (the file
//!   system's file, or its error);
//! - D-9: a callback that uses the config the library is working on raises the port's
//!   RuntimeError, where the wheel's reads it; another thread's calls on the config while a
//!   callback of a call on it waits are the wheel's.
//!
//! And, without the wheel, where its process dies or it keeps a value outside an enum: `None`
//! for the proxy (U-124), for `LogMessage`'s message (U-123), a logging level outside the
//! enumerators (U-112).
//!
//! The logging and hash functions are the process's: one test runs every script in turn.
#![cfg(feature = "python")]

mod common;

use common::script::{
    Expect, Script, b, by, compare, compare_reports, d, f, i, n, none, r, ra, s, t,
};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};

/// A config whose color space `lut` reads `lut.spi1d` from its search path.
const CONFIG: &str = r#"ocio_profile_version: 2.1
search_path: luts
roles:
  default: raw
displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace> {name: raw, isdata: true}
  - !<ColorSpace> {name: lin}
  - !<ColorSpace>
    name: lut
    from_scene_reference: !<FileTransform> {src: lut.spi1d}
"#;

const LUT: &[u8] = b"Version 1\nFrom 0 1\nLength 2\nComponents 1\n{\n0\n0.5\n}\n";

/// A config of version 2.3 without the roles 2.2 requires: its validation logs errors.
const LOGGING_CONFIG: &str = r#"ocio_profile_version: 2.3
roles:
  default: raw
displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace> {name: raw}
"#;

fn logging_script() -> Script {
    let mut sc = Script::new();
    let level = sc.call(n("OCIO.GetLoggingLevel"), vec![]);
    for l in ["NONE", "WARNING", "INFO", "DEBUG", "UNKNOWN"] {
        sc.call(
            n("OCIO.SetLoggingLevel"),
            vec![n(&format!("OCIO.LOGGING_LEVEL_{l}"))],
        );
        sc.call(n("OCIO.GetLoggingLevel"), vec![]);
    }
    sc.raises(n("OCIO.SetLoggingLevel"), vec![i(1)]);
    sc.raises(n("OCIO.SetLoggingLevel"), vec![none()]);
    sc.call_kw(
        n("OCIO.SetLoggingLevel"),
        vec![],
        vec![("level", n("OCIO.LOGGING_LEVEL_INFO"))],
        Expect::Value,
    );
    // The messages a logging function receives, at each level.
    let got = sc.call(n("list"), vec![]);
    sc.call(n("OCIO.SetLoggingFunction"), vec![ra(got, "append")]);
    for (l, m) in [
        ("WARNING", "w"),
        ("INFO", "i"),
        ("DEBUG", "d"),
        ("WARNING", "two\nlines"),
        ("WARNING", "ends\n"),
        ("WARNING", ""),
        ("INFO", "caf\u{e9}"),
        ("NONE", "n"),
    ] {
        sc.call(
            n("OCIO.LogMessage"),
            vec![n(&format!("OCIO.LOGGING_LEVEL_{l}")), s(m)],
        );
    }
    sc.raises(
        n("OCIO.LogMessage"),
        vec![n("OCIO.LOGGING_LEVEL_UNKNOWN"), s("u")],
    );
    sc.call(
        n("OCIO.SetLoggingLevel"),
        vec![n("OCIO.LOGGING_LEVEL_DEBUG")],
    );
    sc.call(
        n("OCIO.LogMessage"),
        vec![n("OCIO.LOGGING_LEVEL_DEBUG"), s("d2")],
    );
    // A level outside the enumerators logs nothing.
    let odd = sc.call(n("OCIO.LoggingLevel"), vec![i(7)]);
    sc.call(n("OCIO.LogMessage"), vec![r(odd), s("odd")]);
    sc.call_kw(
        n("OCIO.LogMessage"),
        vec![],
        vec![
            ("level", n("OCIO.LOGGING_LEVEL_WARNING")),
            ("message", by(b"bytes")),
        ],
        Expect::Value,
    );
    sc.raises(n("OCIO.LogMessage"), vec![n("OCIO.LOGGING_LEVEL_WARNING")]);
    sc.call(n("list"), vec![r(got)]);
    sc.call(
        n("OCIO.SetLoggingLevel"),
        vec![n("OCIO.LOGGING_LEVEL_WARNING")],
    );
    // A logging function that raises: the exception passes through the library, which works
    // afterwards.
    sc.call(n("OCIO.SetLoggingFunction"), vec![n("int")]);
    sc.raises(
        n("OCIO.LogMessage"),
        vec![n("OCIO.LOGGING_LEVEL_WARNING"), s("x")],
    );
    let c = sc.call(n("OCIO.Config.CreateFromStream"), vec![s(LOGGING_CONFIG)]);
    sc.raises(ra(c, "validate"), vec![]);
    sc.call(n("OCIO.SetLoggingFunction"), vec![n("len")]);
    sc.call(
        n("OCIO.LogMessage"),
        vec![n("OCIO.LOGGING_LEVEL_WARNING"), s("x")],
    );
    sc.raises(ra(c, "validate"), vec![]);
    sc.call(n("OCIO.SetLoggingFunction"), vec![ra(got, "append")]);
    sc.raises(ra(c, "validate"), vec![]);
    sc.call(
        n("OCIO.LogMessage"),
        vec![n("OCIO.LOGGING_LEVEL_WARNING"), s("after")],
    );
    sc.call(n("list"), vec![r(got)]);
    // What the caster refuses.
    sc.raises(n("OCIO.SetLoggingFunction"), vec![none()]);
    sc.raises(n("OCIO.SetLoggingFunction"), vec![i(5)]);
    sc.raises(n("OCIO.SetLoggingFunction"), vec![]);
    sc.call_kw(
        n("OCIO.SetLoggingFunction"),
        vec![],
        vec![("logFunction", ra(got, "append"))],
        Expect::Value,
    );
    sc.call(n("OCIO.ResetToDefaultLoggingFunction"), vec![]);
    sc.raises(n("OCIO.ResetToDefaultLoggingFunction"), vec![i(1)]);
    sc.call(
        n("OCIO.LogMessage"),
        vec![n("OCIO.LOGGING_LEVEL_WARNING"), s("to stderr")],
    );
    sc.call(n("len"), vec![r(got)]);
    sc.call(n("OCIO.SetLoggingLevel"), vec![r(level)]);
    sc.call(n("OCIO.GetLoggingLevel"), vec![]);
    sc
}

/// A subclass of `PyConfigIOProxy` and an instance of it whose overrides are the instance's
/// own attributes `attrs`: the instance's step.
fn proxy(sc: &mut Script, attrs: Vec<(&str, Value)>) -> usize {
    let cls = sc.call(
        n("type"),
        vec![
            s("P"),
            t(vec![n("OCIO.PyConfigIOProxy")]),
            d(vec![(s("__module__"), s("probe"))]),
        ],
    );
    let obj = sc.call(r(cls), vec![]);
    for (name, value) in attrs {
        sc.call(n("setattr"), vec![r(obj), s(name), value]);
    }
    obj
}

/// `iter([text]).__next__`: a callable without arguments that returns `text` (once).
fn returns(sc: &mut Script, text: &str) -> Value {
    let it = sc.call(n("iter"), vec![common::script::l(vec![s(text)])]);
    ra(it, "__next__")
}

/// A proxy whose LUT loader raises in a Python frame (`copy.replace`): `what()` lists the
/// frames, from the deepest outwards. The script and its last step's index.
fn frames_script() -> (Script, usize) {
    let mut sc = Script::new();
    let data = returns(&mut sc, CONFIG);
    let hash = ra_lit(&mut sc, "h5%.0s", "__mod__");
    let p5 = proxy(
        &mut sc,
        vec![
            ("getConfigData", data),
            ("getLutData", n("copy.replace")),
            ("getFastLutFileHash", hash),
        ],
    );
    sc.call(n("OCIO.ClearAllCaches"), vec![]);
    let c5 = sc.call(n("OCIO.Config.CreateFromConfigIOProxy"), vec![r(p5)]);
    let k = sc.raises(ra(c5, "getProcessor"), vec![s("lin"), s("lut")]);
    (sc, k)
}

/// A config whose reading logs a warning (an unknown key).
const WARNING_CONFIG: &str = "ocio_profile_version: 2\nroles: {default: raw}\n\
     displays: {d: [!<View> {name: v, colorspace: raw}]}\n\
     colorspaces: [!<ColorSpace> {name: raw}]\nbogus: 1\n";

/// A class (a step) of instances that raise `exc` (a step) when `copy.copy` copies them:
/// their `__reduce_ex__` (a dict's `__getitem__`, called without the instance) gives
/// `(gen.throw, (exc,))`, which `copy._reconstruct` calls, `gen` a stdlib generator
/// (`WeakSet.__iter__`) not started. The exception's traceback runs from `copy` through
/// `_reconstruct` to the generator's frame, the deepest, which once ended has no `f_back`:
/// `what()` lists that frame only, the same on both sides. `base` and `slot` (a method name)
/// make `copy.copy` (a function: bound to the instance) a method of the class.
fn raising_class(sc: &mut Script, name: &str, base: Vec<Value>, slot: &str, exc: usize) -> usize {
    let set = sc.call(n("threading.WeakSet"), vec![]);
    let generator = sc.call(ra(set, "__iter__"), vec![]);
    let reduce = sc.call(
        n("dict"),
        vec![d(vec![(
            i(4),
            t(vec![ra(generator, "throw"), t(vec![r(exc)])]),
        )])],
    );
    sc.call(
        n("type"),
        vec![
            s(name),
            t(base),
            d(vec![
                (s("__module__"), s("probe")),
                (s(slot), n("copy.copy")),
                (s("__reduce_ex__"), ra(reduce, "__getitem__")),
            ]),
        ],
    )
}

/// `error_already_set::what()` of a logging function's exceptions, in the message of the
/// config reader that catches them: an empty message, notes (one not UTF-8), a message not
/// UTF-8, and a `str()` that raises (`__str__` a dict's `popitem`, without a traceback, or
/// raising in a Python frame), notes too; a `str` subclass whose `encode` answers otherwise (a
/// note, and what `str()` gives: `str`'s own `encode` reads them), and an exception whose
/// `__notes__` raises KeyError (no mark, no notes; the wheel's report of it on stderr is I-302).
/// The logging function is `operator.methodcaller("join", y)`, whose `iter(y)` copies `y` and
/// raises.
fn what_script() -> Script {
    let mut sc = Script::new();
    let popitem = sc.call(n("dict"), vec![]);
    let e_class = sc.call(
        n("type"),
        vec![
            s("E"),
            t(vec![n("Exception")]),
            d(vec![
                (s("__module__"), s("probe")),
                (s("__str__"), ra(popitem, "popitem")),
            ]),
        ],
    );
    let inner = sc.call(n("RuntimeError"), vec![s("no str")]);
    let e2_class = raising_class(&mut sc, "E2", vec![n("Exception")], "__str__", inner);
    let empty = sc.call(n("ValueError"), vec![]);
    let noted = sc.call(n("ValueError"), vec![s("n")]);
    sc.call(ra(noted, "add_note"), vec![s("note1")]);
    // Lone surrogates (UTF-8 with surrogatepass, as the step interpreter reads text).
    sc.call(ra(noted, "add_note"), vec![json!({ "str": "edb3bf" })]);
    let surrogate = sc.call(n("ValueError"), vec![json!({ "str": "61edb3bf62" })]);
    let no_str = sc.call(r(e_class), vec![s("x")]);
    let no_str_noted = sc.call(r(e_class), vec![s("y")]);
    sc.call(ra(no_str_noted, "add_note"), vec![s("nn")]);
    let no_str_framed = sc.call(r(e2_class), vec![s("z")]);
    // Notes set as a list holding a note that isn't a `str`.
    let bad_note = sc.call(n("ValueError"), vec![s("t")]);
    sc.step(
        json!({ "setattr": r(bad_note), "name": "__notes__",
                "value": common::script::l(vec![s("a"), i(5)]) }),
        Expect::Value,
    );
    // A `str` subclass whose `encode` answers otherwise: pybind11 encodes with `str`'s own
    // (`PyUnicode_AsEncodedString`), a note of it and a `str()` giving one.
    let encodes = sc.call(n("dict"), vec![d(vec![(s("utf-8"), by(b"OVERRIDDEN"))])]);
    let s_class = sc.call(
        n("type"),
        vec![
            s("S"),
            t(vec![n("str")]),
            d(vec![
                (s("__module__"), s("probe")),
                (s("encode"), ra(encodes, "get")),
            ]),
        ],
    );
    let n1 = sc.call(r(s_class), vec![s("n1")]);
    let sub_noted = sc.call(n("ValueError"), vec![s("v")]);
    sc.step(
        json!({ "setattr": r(sub_noted), "name": "__notes__",
                "value": common::script::l(vec![r(n1), s("n2")]) }),
        Expect::Value,
    );
    let strsub = sc.call(r(s_class), vec![s("strsub")]);
    let strsubs = sc.call(
        n("iter"),
        vec![common::script::l(vec![
            r(strsub),
            r(strsub),
            r(strsub),
            r(strsub),
        ])],
    );
    let ss_class = sc.call(
        n("type"),
        vec![
            s("SS"),
            t(vec![n("Exception")]),
            d(vec![
                (s("__module__"), s("probe")),
                (s("__str__"), ra(strsubs, "__next__")),
            ]),
        ],
    );
    let sub_str = sc.call(r(ss_class), vec![s("v")]);
    // Reading `__notes__` raises KeyError (a `__getattr__` that is a dict's `__getitem__`): no
    // mark and no notes (the wheel's report of it as unraisable, on stderr: I-302).
    let missing = sc.call(n("dict"), vec![]);
    let r_class = sc.call(
        n("type"),
        vec![
            s("R"),
            t(vec![n("Exception")]),
            d(vec![
                (s("__module__"), s("probe")),
                (s("__getattr__"), ra(missing, "__getitem__")),
            ]),
        ],
    );
    let notes_raise = sc.call(r(r_class), vec![s("v")]);
    for exc in [
        empty,
        noted,
        surrogate,
        no_str,
        no_str_noted,
        no_str_framed,
        bad_note,
        sub_noted,
        sub_str,
        notes_raise,
    ] {
        let class = raising_class(&mut sc, "Y", vec![], "__iter__", exc);
        let y = sc.call(r(class), vec![]);
        let f = sc.call(n("operator.methodcaller"), vec![s("join"), r(y)]);
        sc.call(n("OCIO.SetLoggingFunction"), vec![r(f)]);
        sc.raises(n("OCIO.Config.CreateFromStream"), vec![s(WARNING_CONFIG)]);
        sc.call(n("OCIO.ResetToDefaultLoggingFunction"), vec![]);
    }
    sc
}

fn proxy_script() -> Script {
    let mut sc = Script::new();
    // A proxy whose LUT loader raises (TypeError from `str.__mul__`, without a Python frame);
    // its hash says the file is there.
    let data = returns(&mut sc, CONFIG);
    let (lut, hash) = (
        ra_lit(&mut sc, "x", "__mul__"),
        ra_lit(&mut sc, "h%.0s", "__mod__"),
    );
    let p = proxy(
        &mut sc,
        vec![
            ("getConfigData", data),
            ("getLutData", lut),
            ("getFastLutFileHash", hash),
        ],
    );
    let c = sc.call(n("OCIO.Config.CreateFromConfigIOProxy"), vec![r(p)]);
    sc.call(ra(c, "getConfigIOProxy"), vec![]);
    sc.call(ra(c, "getCacheID"), vec![]);
    sc.call(ra(c, "serialize"), vec![]);
    sc.raises(ra(c, "getProcessor"), vec![s("lin"), s("lut")]);
    // The file cache keeps the failure under the file's hash.
    sc.raises(ra(c, "getProcessor"), vec![s("lin"), s("lut")]);
    // LUT data pybind11 can't cast to a vector.
    let data = returns(&mut sc, CONFIG);
    let (lut, hash) = (
        ra_lit(&mut sc, "", "__add__"),
        ra_lit(&mut sc, "h2%.0s", "__mod__"),
    );
    let p2 = proxy(
        &mut sc,
        vec![
            ("getConfigData", data),
            ("getLutData", lut),
            ("getFastLutFileHash", hash),
        ],
    );
    // The file cache keeps the earlier proxies' LUTs under their paths.
    sc.call(n("OCIO.ClearAllCaches"), vec![]);
    let c2 = sc.call(n("OCIO.Config.CreateFromConfigIOProxy"), vec![r(p2)]);
    sc.raises(ra(c2, "getProcessor"), vec![s("lin"), s("lut")]);
    // A hash that raises: the file isn't there.
    let data = returns(&mut sc, CONFIG);
    let (lut, hash) = (
        ra_lit(&mut sc, "x", "__mul__"),
        ra_lit(&mut sc, "x", "__mul__"),
    );
    let p3 = proxy(
        &mut sc,
        vec![
            ("getConfigData", data),
            ("getLutData", lut),
            ("getFastLutFileHash", hash),
        ],
    );
    // The file cache keeps the earlier proxies' LUTs under their paths.
    sc.call(n("OCIO.ClearAllCaches"), vec![]);
    let c3 = sc.call(n("OCIO.Config.CreateFromConfigIOProxy"), vec![r(p3)]);
    sc.call(ra(c3, "getCacheID"), vec![]);
    sc.raises(ra(c3, "getProcessor"), vec![s("lin"), s("lut")]);
    // The config's text: what the caster refuses, an exception, an empty one.
    for (attr, expect) in [
        (n("int"), Expect::Raised),
        (n("str"), Expect::Raised),
        (n("list"), Expect::Raised),
        (n("bytes"), Expect::Raised),
    ] {
        let p = proxy(&mut sc, vec![("getConfigData", attr)]);
        sc.step(
            json!({ "call": n("OCIO.Config.CreateFromConfigIOProxy"), "args": [r(p)] }),
            expect,
        );
    }
    let mul = ra_lit(&mut sc, "x", "__mul__");
    let p4 = proxy(&mut sc, vec![("getConfigData", mul)]);
    sc.raises(n("OCIO.Config.CreateFromConfigIOProxy"), vec![r(p4)]);
    let bare = sc.call(n("OCIO.PyConfigIOProxy"), vec![]);
    sc.raises(n("OCIO.Config.CreateFromConfigIOProxy"), vec![r(bare)]);
    sc.raises(n("OCIO.Config.CreateFromConfigIOProxy"), vec![i(1)]);
    sc.call_kw(
        n("OCIO.Config.CreateFromConfigIOProxy"),
        vec![],
        vec![("arg0", r(p))],
        Expect::Raised,
    );
    // Setting and getting a config's proxy.
    let raw = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    sc.call(ra(raw, "getConfigIOProxy"), vec![]);
    sc.call(ra(raw, "setConfigIOProxy"), vec![r(p)]);
    sc.call(ra(raw, "getConfigIOProxy"), vec![]);
    sc.call(ra(raw, "setConfigIOProxy"), vec![none()]);
    sc.call(ra(raw, "getConfigIOProxy"), vec![]);
    sc.call(ra(raw, "setConfigIOProxy"), vec![r(bare)]);
    sc.call(ra(raw, "getConfigIOProxy"), vec![]);
    sc.raises(ra(raw, "setConfigIOProxy"), vec![i(1)]);
    sc.call_kw(
        ra(raw, "setConfigIOProxy"),
        vec![],
        vec![("ciop", r(p2))],
        Expect::Value,
    );
    sc.call(ra(raw, "getConfigIOProxy"), vec![]);
    sc
}

/// A bound method `m` of the `str` `text`: its expression (a step making the `str`).
fn ra_lit(sc: &mut Script, text: &str, m: &str) -> Value {
    let k = sc.call(n("str"), vec![s(text)]);
    ra(k, m)
}

/// The steps of a config read from the files, under the hash function `hash` (or the default,
/// `None`): its processor, then its cache ID (but the default's, which holds the file's path
/// and time, different on the two sides); with `id_first`, the cache ID first. The library
/// marks a file's hash as known before it calls the hash function, so a hash that fails is
/// raised by the first use only, and later uses see an empty hash (a missing file).
fn hash_steps(sc: &mut Script, hash: Option<Value>, processor: Expect, id_first: bool) {
    sc.call(n("OCIO.ClearAllCaches"), vec![]);
    let default = hash.is_none();
    match hash {
        Some(h) => sc.call(n("OCIO.SetComputeHashFunction"), vec![h]),
        None => sc.call(n("OCIO.ResetComputeHashFunction"), vec![]),
    };
    let c = sc.call(n("OCIO.Config.CreateFromFile"), vec![s("config.ocio")]);
    if id_first && !default {
        sc.call(ra(c, "getCacheID"), vec![]);
    }
    sc.step(
        json!({ "call": ra(c, "getProcessor"), "args": [s("lin"), s("lut")] }),
        processor,
    );
    if !default {
        sc.call(ra(c, "getCacheID"), vec![]);
    }
    sc.step(
        json!({ "call": ra(c, "getProcessor"), "args": [s("lin"), s("lut")] }),
        processor,
    );
}

fn hash_script() -> Script {
    let mut sc = Script::new();
    for id_first in [false, true] {
        let constant = ra_lit(&mut sc, "h%.0s", "__mod__");
        hash_steps(&mut sc, Some(constant), Expect::Value, id_first);
        // A result pybind11 can't cast to a std::string, an exception, the empty function
        // (I-301).
        hash_steps(&mut sc, Some(n("len")), Expect::Raised, id_first);
        let mul = ra_lit(&mut sc, "x", "__mul__");
        hash_steps(&mut sc, Some(mul), Expect::Raised, id_first);
        hash_steps(&mut sc, Some(none()), Expect::Raised, id_first);
    }
    sc.raises(n("OCIO.SetComputeHashFunction"), vec![i(1)]);
    sc.raises(n("OCIO.SetComputeHashFunction"), vec![]);
    sc.raises(n("OCIO.ResetComputeHashFunction"), vec![i(1)]);
    // The default again.
    hash_steps(&mut sc, None, Expect::Value, false);
    sc
}

/// A proxy config whose color space `lut` reads the file `SRC` (replaced by an absolute path).
const ABSOLUTE_LUT_CONFIG: &str = "ocio_profile_version: 2\nroles: {default: raw}\n\
     displays: {d: [!<View> {name: v, colorspace: raw}]}\n\
     colorspaces:\n  - !<ColorSpace> {name: raw}\n  \
     - !<ColorSpace> {name: lut, from_scene_reference: !<FileTransform> {src: 'SRC'}}\n";

/// Where the library catches a callback's exception and goes on with its own (on the files of
/// [`hash_script`]): a debug message the logging function (`int`, whose ValueError has no
/// traceback) refuses while a LUT loads (`GetCachedFileAndFormat`'s catch) and while a group
/// is written as CTF; the hash function refusing the LUT's path while the CTF writer resolves
/// it (`GroupTransform::write`'s catch); a warning the logging function refuses while a config
/// is serialized (`Config::serialize`'s); a proxy whose LUT loader raises for an absolute path,
/// which the library then reads from the file system (`getLutData`'s fallback), there or
/// missing.
fn catches_script() -> Script {
    let mut sc = Script::new();
    let c = sc.call(n("OCIO.Config.CreateFromFile"), vec![s("config.ocio")]);
    sc.call(n("OCIO.ClearAllCaches"), vec![]);
    sc.call(
        n("OCIO.SetLoggingLevel"),
        vec![n("OCIO.LOGGING_LEVEL_DEBUG")],
    );
    sc.call(n("OCIO.SetLoggingFunction"), vec![n("int")]);
    sc.raises(ra(c, "getProcessor"), vec![s("lin"), s("lut")]);
    sc.call(n("OCIO.ClearAllCaches"), vec![]);
    let g = sc.call(n("OCIO.GroupTransform"), vec![]);
    let ft = sc.call(n("OCIO.FileTransform"), vec![s("lut.spi1d")]);
    sc.call(ra(g, "appendTransform"), vec![r(ft)]);
    sc.call_kw(
        ra(g, "write"),
        vec![],
        vec![
            ("formatName", s("Color Transform Format")),
            ("config", r(c)),
        ],
        Expect::Raised,
    );
    sc.call(
        n("OCIO.SetLoggingLevel"),
        vec![n("OCIO.LOGGING_LEVEL_INFO")],
    );
    sc.call(n("OCIO.ResetToDefaultLoggingFunction"), vec![]);
    // The hash function (`int`) refusing the LUT's path while the writer resolves it, outside
    // the file loader.
    // (A config whose context hasn't resolved the file yet.)
    sc.call(n("OCIO.ClearAllCaches"), vec![]);
    let fresh = sc.call(n("OCIO.Config.CreateFromFile"), vec![s("config.ocio")]);
    sc.call(n("OCIO.SetComputeHashFunction"), vec![n("int")]);
    sc.call_kw(
        ra(g, "write"),
        vec![],
        vec![
            ("formatName", s("Color Transform Format")),
            ("config", r(fresh)),
        ],
        Expect::Raised,
    );
    sc.call(n("OCIO.ResetComputeHashFunction"), vec![]);
    // A config of version 2.4 whose fixed function style is deprecated there: its writer warns.
    let c2 = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    sc.call(ra(c2, "setVersion"), vec![i(2), i(4)]);
    let cs = sc.call_kw(
        n("OCIO.ColorSpace"),
        vec![],
        vec![("name", s("ff"))],
        Expect::Value,
    );
    let ff = sc.call(
        n("OCIO.FixedFunctionTransform"),
        vec![
            n("OCIO.FIXED_FUNCTION_ACES_GAMUT_COMPRESS_20"),
            common::script::fl(&[100.0, 0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329]),
        ],
    );
    sc.call(
        ra(cs, "setTransform"),
        vec![r(ff), n("OCIO.COLORSPACE_DIR_TO_REFERENCE")],
    );
    sc.call(ra(c2, "addColorSpace"), vec![r(cs)]);
    sc.call(n("OCIO.SetLoggingFunction"), vec![n("int")]);
    sc.raises(ra(c2, "serialize"), vec![]);
    sc.raises(n("str"), vec![r(c2)]);
    sc.call(n("OCIO.ResetToDefaultLoggingFunction"), vec![]);
    // The proxy's absolute paths (the config file's directory, "/" joined), the failures
    // above forgotten.
    sc.call(n("OCIO.ClearAllCaches"), vec![]);
    let dir = sc.call(ra(c, "getWorkingDir"), vec![]);
    for (file, expect) in [
        ("/luts/lut.spi1d", Expect::Value),
        ("/luts/missing.spi1d", Expect::Raised),
    ] {
        let path = sc.call(n("str.__add__"), vec![r(dir), s(file)]);
        let text = sc.call(
            n("str.replace"),
            vec![s(ABSOLUTE_LUT_CONFIG), s("SRC"), r(path)],
        );
        let data = sc.call(n("iter"), vec![common::script::l(vec![r(text)])]);
        let lut = ra_lit(&mut sc, "x", "__mul__");
        let p = proxy(
            &mut sc,
            vec![
                ("getConfigData", ra(data, "__next__")),
                ("getLutData", lut),
                ("getFastLutFileHash", n("str")),
            ],
        );
        let c3 = sc.call(n("OCIO.Config.CreateFromConfigIOProxy"), vec![r(p)]);
        sc.step(
            json!({ "call": ra(c3, "getProcessor"), "args": [s("raw"), s("lut")] }),
            expect,
        );
    }
    sc
}

fn d9_script() -> Script {
    let mut sc = Script::new();
    let c = sc.call(n("OCIO.Config.CreateFromStream"), vec![s(LOGGING_CONFIG)]);
    sc.call(n("OCIO.SetLoggingFunction"), vec![ra(c, "getColorSpace")]);
    // The validation logs its errors and succeeds, in the wheel.
    sc.call(ra(c, "validate"), vec![]);
    let other = sc.call(n("OCIO.Config.CreateFromStream"), vec![s(LOGGING_CONFIG)]);
    sc.call(ra(other, "validate"), vec![]);
    sc.call(n("OCIO.ResetToDefaultLoggingFunction"), vec![]);
    sc
}

/// A call on a config from the script's thread while another thread's call on it waits in
/// its logging function: the wheel's calls run alongside it, the port's wait for its end, with
/// the GIL released, and give the wheel's answers. The logging function is
/// `operator.methodcaller("join", z)`, `z` a `zip` of `range(1)` (first, so that `join`, which
/// reads `z` whole, stops after one round), `iter(barrier.wait, -1)` and
/// `iter(sem.acquire, False)`: at the first message the worker meets the script's thread at
/// the barrier (which the script waits at through `Thread(target=barrier.wait).run()`, whose
/// result is `None` where `wait`'s is the order of arrival), so the script's calls begin while
/// the worker is in its callback; it then waits on the semaphore, which a timer releases two
/// seconds after the script's calls began. The join then refuses the pair `zip` gives (a
/// TypeError, which ends the validation in the worker).
fn threads_script() -> Script {
    let mut sc = Script::new();
    let c = sc.call(n("OCIO.Config.CreateFromStream"), vec![s(LOGGING_CONFIG)]);
    let barrier = sc.call(n("threading.Barrier"), vec![i(2), none(), f(30.0)]);
    let sem = sc.call(n("threading.Semaphore"), vec![i(0)]);
    let meets = sc.call(n("iter"), vec![ra(barrier, "wait"), i(-1)]);
    let waits = sc.call(n("iter"), vec![ra(sem, "acquire"), b(false)]);
    let once = sc.call(n("range"), vec![i(1)]);
    let both = sc.call(n("zip"), vec![r(once), r(meets), r(waits)]);
    let logging = sc.call(n("operator.methodcaller"), vec![s("join"), r(both)]);
    sc.call(n("OCIO.SetLoggingFunction"), vec![r(logging)]);
    let worker = sc.call_kw(
        n("threading.Thread"),
        vec![],
        vec![
            ("target", ra(c, "validate")),
            ("name", s("worker")),
            ("daemon", b(false)),
        ],
        Expect::Value,
    );
    let timer = sc.call(
        n("threading.Timer"),
        vec![f(2.0), ra(sem, "release"), t(vec![i(1000)])],
    );
    sc.step(
        json!({ "setattr": r(timer), "name": "name", "value": s("timer") }),
        Expect::Value,
    );
    // Threads made by the port's test thread are daemons by default (a thread Python didn't
    // start), the wheel's script's aren't; the timer's repr (its default name and daemon flag)
    // isn't compared until both are set.
    sc.unrepr(timer);
    sc.step(
        json!({ "setattr": r(timer), "name": "daemon", "value": b(false) }),
        Expect::Value,
    );
    let meet = sc.call_kw(
        n("threading.Thread"),
        vec![],
        vec![
            ("target", ra(barrier, "wait")),
            ("name", s("meet")),
            ("daemon", b(false)),
        ],
        Expect::Value,
    );
    sc.call(ra(worker, "start"), vec![]);
    // The worker's first message meets this thread at the barrier: it is in its callback now.
    sc.call(ra(meet, "run"), vec![]);
    sc.call(ra(timer, "start"), vec![]);
    let names = sc.call(ra(c, "getColorSpaceNames"), vec![]);
    sc.call(n("list"), vec![r(names)]);
    sc.call(ra(c, "getCacheID"), vec![]);
    sc.call(ra(worker, "join"), vec![]);
    sc.call(ra(timer, "join"), vec![]);
    sc.call(n("OCIO.ResetToDefaultLoggingFunction"), vec![]);
    sc.call(ra(c, "getCacheID"), vec![]);
    sc
}

/// D-10: exceptions whose `__notes__` isn't a list (a tuple, a `str`, `None`), raised by a
/// logging function in the config reader's message, as `what_script` does for lists.
fn bad_notes_script() -> Script {
    let mut sc = Script::new();
    for notes in [t(vec![s("a"), s("b")]), s("abc"), none()] {
        let exc = sc.call(n("ValueError"), vec![s("t")]);
        sc.step(
            json!({ "setattr": r(exc), "name": "__notes__", "value": notes }),
            Expect::Value,
        );
        let class = raising_class(&mut sc, "Y", vec![], "__iter__", exc);
        let y = sc.call(r(class), vec![]);
        let f = sc.call(n("operator.methodcaller"), vec![s("join"), r(y)]);
        sc.call(n("OCIO.SetLoggingFunction"), vec![r(f)]);
        sc.raises(n("OCIO.Config.CreateFromStream"), vec![s(WARNING_CONFIG)]);
        sc.call(n("OCIO.ResetToDefaultLoggingFunction"), vec![]);
    }
    sc
}

/// The start of the line the wheel's `what()` holds for a `__notes__` that isn't a list
/// (`PyList_Size`'s `PyErr_BadInternalCall`: its text names the interpreter's build path), and
/// the end of that line.
const BAD_NOTES_LINE: &str = "\nFAILURE obtaining len(__notes__): SystemError: ";
const BAD_NOTES_TAIL: &str = "listobject.c:342: bad argument to internal function";

/// The wheel's message without [`BAD_NOTES_LINE`] and its text, which it must hold once.
fn without_bad_notes_line(wheel: &str) -> String {
    assert_eq!(wheel.matches(BAD_NOTES_LINE).count(), 1, "{wheel:?}");
    let start = wheel.find(BAD_NOTES_LINE).unwrap();
    let end = start + 1 + wheel[start + 1..].find('\n').expect(wheel);
    let line = &wheel[start..end];
    assert!(line.ends_with(BAD_NOTES_TAIL), "{line:?}");
    format!("{}{}", &wheel[..start], &wheel[end..])
}

/// The text of a report's exception message.
fn message(step: &Value) -> String {
    String::from_utf8(ocio_testkit::oracle_values::bytes(
        &step["raised"]["message"],
    ))
    .unwrap()
}

/// The callbacks behave as the wheel's (see the module's documentation); D-9 where it differs.
#[test]
fn callbacks_as_the_wheels() {
    compare(&[logging_script(), proxy_script(), what_script()]);
    // The what() cases reach the branches they are for, in the wheel (the port's are equal).
    let what = what_script();
    let theirs = common::wheel(&[what.json()]).remove(0);
    let messages: Vec<String> = theirs["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|st| st.get("raised").is_some())
        .map(message)
        .collect();
    assert_eq!(messages.len(), 10, "{theirs}");
    let at = "\n\nAt:\n  ";
    for (m, parts) in messages.iter().zip([
        &["ValueError: <EMPTY MESSAGE>", at][..],
        &[
            "ValueError[WITH __notes__]: n\n__notes__ (len=2):\nnote1\n\\udcff",
            at,
        ],
        &["ValueError: a\\udcffb", at],
        &[
            "E: <MESSAGE UNAVAILABLE DUE TO ANOTHER EXCEPTION>",
            at,
            "\nMESSAGE UNAVAILABLE DUE TO EXCEPTION: KeyError: 'popitem(): dictionary is empty'",
        ],
        &[
            "E[WITH __notes__]: <MESSAGE UNAVAILABLE DUE TO ANOTHER EXCEPTION>\n__notes__",
            "\nMESSAGE UNAVAILABLE DUE TO EXCEPTION: KeyError",
        ],
        &[
            "E2: <MESSAGE UNAVAILABLE DUE TO ANOTHER EXCEPTION>",
            "\nMESSAGE UNAVAILABLE DUE TO EXCEPTION: RuntimeError: no str\n\nAt:\n  ",
        ],
        &[
            "ValueError[WITH __notes__]: t\n__notes__ (len=2):\na\nFAILURE obtaining __notes__[1]: \
           TypeError: bad argument type for built-in operation\n\nAt:\n  ",
        ],
        &["ValueError[WITH __notes__]: v\n__notes__ (len=2):\nn1\nn2\n\nAt:\n  "],
        &["SS: strsub\n\nAt:\n  "],
        &["R: v\n\nAt:\n  "],
    ]) {
        for part in parts {
            assert!(m.contains(part), "{part:?} in {m:?}");
        }
    }

    // D-10: a `__notes__` that isn't a list. The wheel's what() has the line of CPython's own
    // error for it, with the interpreter's build path; the port's lists no notes, and is the
    // wheel's message without that line, in every other byte.
    let bad = bad_notes_script();
    let theirs = common::wheel(&[bad.json()]).remove(0);
    let ours = common::port(&bad.json());
    let raised = |report: &Value| -> Vec<String> {
        report["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|st| st.get("raised").is_some())
            .map(message)
            .collect()
    };
    let (wheel, port) = (raised(&theirs), raised(&ours));
    assert_eq!(wheel.len(), 3, "{theirs}");
    assert_eq!(port.len(), 3, "{ours}");
    for (w, p) in wheel.iter().zip(&port) {
        let head = "ValueError[WITH __notes__]: t\n";
        assert!(
            w.contains(&format!("{head}{}", &BAD_NOTES_LINE[1..])),
            "{w:?}"
        );
        assert!(p.contains(&format!("{head}\nAt:\n  ")), "{p:?}");
        assert!(
            !p.contains("FAILURE") && !p.contains("__notes__ (len"),
            "{p:?}"
        );
        assert_eq!(&without_bad_notes_line(w), p);
    }
    // The rest of the two reports is equal: the other steps, and the raised types.
    let (theirs_steps, ours_steps) = (
        theirs["steps"].as_array().unwrap(),
        ours["steps"].as_array().unwrap(),
    );
    assert_eq!(theirs_steps.len(), ours_steps.len());
    for (j, (a, b)) in theirs_steps.iter().zip(ours_steps).enumerate() {
        if a.get("raised").is_some() {
            assert_eq!(a["raised"]["type"], b["raised"]["type"], "step {j}");
        } else {
            assert_eq!(a, b, "step {j}");
        }
    }

    // The frames of what(): the wheel's step interpreter runs as a script (`main`, `<module>`),
    // the port's is called by this test (`run` is its outermost frame); the frames up to `run`
    // are the same, and the wheel's has exactly those two more.
    let (sc, k) = frames_script();
    let theirs = common::wheel(&[sc.json()]).remove(0);
    let ours = common::port(&sc.json());
    for j in 0..k {
        assert_eq!(theirs["steps"][j], ours["steps"][j], "step {j}");
    }
    let (wheel, port) = (message(&theirs["steps"][k]), message(&ours["steps"][k]));
    assert!(
        port.contains("\n\nAt:\n") && port.contains("copy.py("),
        "{port}"
    );
    assert!(port.ends_with(": run\n"), "{port}");
    let extra = wheel
        .strip_prefix(port.as_str())
        .unwrap_or_else(|| panic!("{wheel}\n{port}"));
    let lines: Vec<&str> = extra.lines().collect();
    assert_eq!(lines.len(), 2, "{extra}");
    for (line, function) in lines.iter().zip(["main", "<module>"]) {
        assert!(
            line.starts_with("  ")
                && line.contains("py_eval_steps.py(")
                && line.ends_with(&format!("): {function}")),
            "{line}"
        );
    }
    assert_eq!(
        theirs["steps"][k]["raised"]["type"],
        ours["steps"][k]["raised"]["type"]
    );

    // The hash function, on files.
    let files = [
        ("config.ocio", CONFIG.as_bytes().to_vec()),
        ("luts/lut.spi1d", LUT.to_vec()),
    ];
    let scripts = [hash_script(), catches_script()];
    let json: Vec<Value> = scripts.iter().map(Script::json).collect();
    let theirs = common::wheel_with_files(&json, &files);
    let dir = std::env::temp_dir().join(format!("ocio-py-callbacks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (name, bytes) in &files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    std::env::set_current_dir(&dir).unwrap();
    let ours: Vec<Value> = scripts
        .iter()
        .map(|sc| common::restore_dir(&common::port(&sc.json()), &dir))
        .collect();
    compare_reports(&scripts, &ours, &theirs);
    // The library's catches are reached, in the wheel (the port's are equal): its own
    // exceptions with the callback's what() in them, the proxy's fallback reading the file.
    let raised: Vec<(String, String)> = theirs[1]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|st| st.get("raised").is_some())
        .map(|st| {
            (
                st["raised"]["type"].as_str().unwrap().to_string(),
                message(st),
            )
        })
        .collect();
    assert_eq!(raised.len(), 6, "{}", theirs[1]);
    for ((ty, m), prefix) in raised.iter().zip([
        "ValueError: invalid literal for int() with base 10: '[OpenColorIO Debug]",
        "Error writing format 'Color Transform Format': ValueError: invalid literal",
        "Error writing format 'Color Transform Format': ValueError: invalid literal",
        "Error building YAML: ValueError: invalid literal",
        "Error building YAML: ValueError: invalid literal",
        "The specified transform file '$FILES",
    ]) {
        assert_eq!(ty, "PyOpenColorIO.PyOpenColorIO.Exception", "{m}");
        assert!(m.starts_with(prefix), "{prefix:?} in {m:?}");
    }
    std::env::set_current_dir(std::env::temp_dir()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);

    // D-9: the wheel's logging function reads the config during its validation, which logs its
    // errors and succeeds; the port's raises its RuntimeError, which ends the validation.
    let sc = d9_script();
    let theirs = common::wheel(&[sc.json()]).remove(0);
    let ours = common::port(&sc.json());
    let (wheel, port) = (&theirs["steps"][2], &ours["steps"][2]);
    assert_eq!(wheel["value"]["type"], "builtins.NoneType", "{wheel}");
    assert_eq!(port["raised"]["type"], "builtins.RuntimeError", "{port}");
    assert!(
        message(port).starts_with("Config: a callback of a call on this config uses it")
            && message(port).ends_with("(docs/deviations.md, D-9)"),
        "{port}"
    );
    // Another config is free to use, as the wheel's.
    assert_eq!(theirs["steps"][4], ours["steps"][4]);
    assert_eq!(theirs["steps"][5], ours["steps"][5]);

    // Another thread's call on the config: it waits, and is the wheel's.
    compare(&[threads_script()]);

    // A menu's logging function changes the menu's parameters (chunk 6.7c); on a thread of its
    // own, so that a regression (the parameters' lock held while the library logs) fails.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(|| compare(&[menu_logging_script()]));
        let _ = tx.send(result);
    });
    match rx.recv_timeout(std::time::Duration::from_secs(120)) {
        Ok(Ok(())) => {}
        Ok(Err(panic)) => std::panic::resume_unwind(panic),
        Err(_) => panic!("the menu's logging function waits for the parameters' lock"),
    }
}

/// A config whose view `v` shows a color space it doesn't have: it doesn't validate.
const MENU_CONFIG: &str = "ocio_profile_version: 2\ndescription: menu logging\n\
     roles: {default: raw, scene_linear: lin}\n\
     displays: {d: [!<View> {name: v, colorspace: missing}]}\n\
     colorspaces: [!<ColorSpace> {name: raw}, !<ColorSpace> {name: lin}]\n";

/// A menu of a config that doesn't validate logs a warning first (`ColorSpaceMenuHelper::Create`,
/// ColorSpaceHelpers.cpp:385-394 @ v2.5.2), whose logging function includes the roles in the
/// menu's parameters (a class of `str` whose `__del__` is the parameters' `setIncludeRoles`,
/// its default `True`; the step interpreter defines no functions): the wheel reads the
/// parameters after the warning, and its menu has the roles; so has the port's, and its
/// parameters' lock isn't held while the library logs.
fn menu_logging_script() -> Script {
    let mut sc = Script::new();
    let cfg = sc.call(n("OCIO.Config.CreateFromStream"), vec![s(MENU_CONFIG)]);
    let p = sc.call(n("OCIO.ColorSpaceMenuParameters"), vec![r(cfg)]);
    sc.eval(r(p), &["getIncludeRoles"]);
    let class = sc.call(
        n("type"),
        vec![
            s("Logged"),
            t(vec![n("str")]),
            d(vec![
                (s("__module__"), s("probe")),
                (s("__del__"), ra(p, "setIncludeRoles")),
            ]),
        ],
    );
    sc.call(n("OCIO.SetLoggingFunction"), vec![r(class)]);
    let h = sc.call(n("OCIO.ColorSpaceMenuHelper"), vec![r(p)]);
    sc.call(n("OCIO.ResetToDefaultLoggingFunction"), vec![]);
    sc.eval(r(p), &["getIncludeRoles"]);
    sc.call(ra(h, "getNumColorSpaces"), vec![]);
    for k in 0..4 {
        sc.call(ra(h, "getName"), vec![i(k)]);
    }
    sc
}

/// Where the wheel's process dies (U-123, U-124) or it keeps a level outside the enumerators
/// (U-112), the port's error; the controls run.
#[test]
fn where_the_wheel_fails_the_port_raises() {
    common::start_python();
    Python::attach(|py| {
        for (code, entry) in [
            ("OCIO.Config.CreateFromConfigIOProxy(None)", "U-124"),
            ("OCIO.LogMessage(OCIO.LOGGING_LEVEL_WARNING, None)", "U-123"),
            ("OCIO.SetLoggingLevel(OCIO.LoggingLevel(7))", "U-112"),
        ] {
            let globals = PyDict::new(py);
            let src =
                std::ffi::CString::new(format!("import PyOpenColorIO as OCIO\n{code}")).unwrap();
            let err = py.run(&src, Some(&globals), None).expect_err(code);
            let text = err.to_string();
            assert!(
                text.starts_with("ValueError")
                    && text.contains(&format!("docs/improvements.md, {entry})")),
                "{code}: {text}"
            );
        }
        let src = std::ffi::CString::new(
            "import PyOpenColorIO as OCIO\nl = OCIO.GetLoggingLevel()\n\
             OCIO.SetLoggingLevel(l)\nassert OCIO.GetLoggingLevel() == l\n\
             OCIO.Config.CreateRaw().setConfigIOProxy(None)",
        )
        .unwrap();
        py.run(&src, None, None).unwrap_or_else(|e| panic!("{e}"));
    });
}
