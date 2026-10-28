// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Argument refusal and incompatible-ABI fallback for the shared conduit.
//! Compatible native C++ negotiation and pointer exchange are outside the port's scope
//! (PLAN.md section 4, D-16); Python refusals and this fallback remain in scope.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, compare, d, i, n, none, r, ra, s, t};

/// Shared conduit signature refusal with a valid Config self. No capsule or
/// native pointer is made: every argument vector has a deliberately wrong
/// Python type. Correctly typed fallback and matching C++ ABI remain separate.
#[test]
fn conduit_rejects_wrong_python_argument_types() {
    let mut sc = Script::new();
    let cfg = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let bytes = common::script::by(b"deliberately-incompatible-abi");
    for args in [
        vec![i(7), none(), bytes.clone()],
        vec![none(), none(), bytes.clone()],
        vec![bytes.clone(), none(), bytes.clone()],
        vec![bytes.clone(), r(cfg), bytes.clone()],
        vec![bytes.clone(), none(), i(7)],
        vec![],
        vec![bytes.clone(), none()],
        vec![bytes.clone(), none(), bytes.clone(), i(7)],
    ] {
        sc.raises(ra(cfg, "_pybind11_conduit_v1_"), args);
    }
    // The wheel checks capsule identity, not a Python class's displayed name.
    let fake_class = sc.call(
        n("type"),
        vec![
            s("PyCapsule"),
            t(vec![]),
            d(vec![(s("__module__"), s("probe"))]),
        ],
    );
    let spoof = sc.call(r(fake_class), vec![]);
    sc.raises(
        ra(cfg, "_pybind11_conduit_v1_"),
        vec![bytes.clone(), r(spoof), bytes.clone()],
    );
    sc.raises(
        n("OCIO.Config._pybind11_conduit_v1_"),
        vec![r(cfg), bytes.clone(), none(), bytes.clone()],
    );
    sc.call_kw(
        ra(cfg, "_pybind11_conduit_v1_"),
        vec![],
        vec![("arg0", bytes.clone()), ("arg1", none()), ("arg2", bytes)],
        Expect::Raised,
    );
    sc.call(ra(cfg, "serialize"), vec![]);
    compare(&[sc]);
}

/// The official wheel returns None for this incompatible-ABI request with NumPy's
/// real owned capsule. This is the exact three-step request observed on Windows
/// (2026-10-27); the full report, including its result, is obtained from the wheel
/// again on each platform. No capsule or pointer is created, and its native contents
/// are not inspected.
#[test]
fn a_real_capsule_uses_the_wheels_incompatible_abi_fallback() {
    let mut sc = Script::new();
    let cfg = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let capsule = sc.eval(n("numpy._core._multiarray_umath._ARRAY_API"), &[]);
    sc.call(
        ra(cfg, "_pybind11_conduit_v1_"),
        vec![
            common::script::by(b"OpenColorIO-rs review deliberately incompatible ABI"),
            r(capsule),
            common::script::by(b"raw_pointer_ephemeral"),
        ],
    );
    compare(&[sc]);
}
