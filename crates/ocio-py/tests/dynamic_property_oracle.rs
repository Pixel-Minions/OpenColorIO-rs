// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `DynamicProperty` (chunk 6.7a) against the wheel: the class's surface, and its methods on
//! the dynamic properties of CPU processors of each dynamic grading transform: the type, each
//! getter and setter on a property of its kind (values with special bits, values refused) and
//! of another (the binding's "Invalid dynamic property type" errors), the values a getter
//! returns (copies for a primary and a tone; for the curves, the property's own object, which
//! keeps a change made through it until a setter replaces the curves), what a setter changes
//! in the pixels the processor renders, and the casters.
//!
//! The processors come from `Config.CreateRaw().getProcessor(t).getDefaultCPUProcessor()`,
//! stand-ins in the port's interpreter until the port binds those (`common::processors`).
#![cfg(feature = "python")]

mod common;

use common::processors::{cpu, start};
use common::script::{Expect, Script, b, compare, f, fb, fl, i, l, n, none, r, ra, special};

/// The property of type `ty` (`DYNAMIC_PROPERTY_...`) of the CPU processor `c`.
fn prop(sc: &mut Script, c: usize, ty: &str) -> usize {
    sc.call(
        ra(c, "getDynamicProperty"),
        vec![n(&format!("OCIO.DYNAMIC_PROPERTY_{ty}"))],
    )
}

/// A pixel the processor `c` renders now.
fn render(sc: &mut Script, c: usize) {
    sc.call(ra(c, "applyRGB"), vec![fl(&[0.25, 0.5, 0.75])]);
}

/// Every getter and setter of the property `p` that its kind (`kind`, a getter's suffix)
/// refuses: the binding's errors.
fn other_kinds(sc: &mut Script, p: usize, kind: &str, values: &[(usize, &str)]) {
    for (v, k) in values {
        if *k == kind {
            continue;
        }
        sc.raises(ra(p, &format!("get{k}")), vec![]);
        sc.raises(ra(p, &format!("set{k}")), vec![r(*v)]);
    }
}

#[test]
fn the_class_has_the_wheels_surface() {
    start();
    let report = common::wheel_surface();
    let theirs = common::wheel_member(&report, "DynamicProperty");
    let ours = common::port_member("DynamicProperty");
    assert!(
        theirs["members"].as_array().is_some_and(|m| m.len() > 10),
        "{theirs}"
    );
    assert_eq!(ours, theirs);
}

fn properties_script() -> Script {
    let mut sc = Script::new();
    // A value of each kind, for the setters.
    let double = sc.call(n("float"), vec![f(0.5)]);
    let primary = sc.call(n("OCIO.GradingPrimary"), vec![n("OCIO.GRADING_LOG")]);
    let rgb = sc.call(n("OCIO.GradingRGBCurve"), vec![n("OCIO.GRADING_LOG")]);
    let hue = sc.call(n("OCIO.GradingHueCurve"), vec![n("OCIO.GRADING_LOG")]);
    let tone = sc.call(n("OCIO.GradingTone"), vec![n("OCIO.GRADING_LOG")]);
    let kinds = [
        (double, "Double"),
        (primary, "GradingPrimary"),
        (rgb, "GradingRGBCurve"),
        (hue, "GradingHueCurve"),
        (tone, "GradingTone"),
    ];
    // Exposure, contrast and gamma.
    let ec = sc.call_kw(
        n("OCIO.ExposureContrastTransform"),
        vec![],
        vec![
            ("dynamicExposure", b(true)),
            ("dynamicContrast", b(true)),
            ("dynamicGamma", b(true)),
            ("style", n("OCIO.EXPOSURE_CONTRAST_VIDEO")),
        ],
        Expect::Value,
    );
    let c = cpu(&mut sc, ec);
    render(&mut sc, c);
    for (k, ty) in ["EXPOSURE", "CONTRAST", "GAMMA"].into_iter().enumerate() {
        let p = prop(&mut sc, c, ty);
        sc.call(ra(p, "getType"), vec![]);
        sc.call(ra(p, "getDouble"), vec![]);
        for v in [special(k), special(k + 3), f(1.25), i(2)] {
            sc.call(ra(p, "setDouble"), vec![v]);
            sc.call(ra(p, "getDouble"), vec![]);
        }
        render(&mut sc, c);
        let again = prop(&mut sc, c, ty);
        sc.call(ra(again, "getDouble"), vec![]);
        sc.call(n("operator.is_"), vec![r(p), r(again)]);
        sc.raises(ra(p, "setDouble"), vec![none()]);
        sc.raises(ra(p, "setDouble"), vec![l(vec![])]);
        sc.raises(ra(p, "setDouble"), vec![]);
        sc.call_kw(
            ra(p, "setDouble"),
            vec![],
            vec![("val", f(0.75))],
            Expect::Value,
        );
        other_kinds(&mut sc, p, "Double", &kinds);
    }
    // A GradingPrimary: copies both ways; values refused by its style.
    let gp = sc.call(
        n("OCIO.GradingPrimaryTransform"),
        vec![n("OCIO.GRADING_LOG"), b(true)],
    );
    let c = cpu(&mut sc, gp);
    render(&mut sc, c);
    let p = prop(&mut sc, c, "GRADING_PRIMARY");
    sc.call(ra(p, "getType"), vec![]);
    let v1 = sc.call(ra(p, "getGradingPrimary"), vec![]);
    let v2 = sc.call(ra(p, "getGradingPrimary"), vec![]);
    sc.call(n("operator.is_"), vec![r(v1), r(v2)]);
    let gamma = sc.call(n("OCIO.GradingRGBM"), vec![f(1.2), f(0.8), f(1.0), f(1.1)]);
    sc.step(
        serde_json::json!({ "setattr": r(v1), "name": "gamma", "value": r(gamma) }),
        Expect::Value,
    );
    sc.call(ra(p, "getGradingPrimary"), vec![]);
    sc.call(ra(p, "setGradingPrimary"), vec![r(v1)]);
    sc.call(ra(p, "getGradingPrimary"), vec![]);
    render(&mut sc, c);
    sc.step(
        serde_json::json!({ "setattr": r(v1), "name": "pivot", "value": fb(0x7ff8_0000_0000_0042) }),
        Expect::Value,
    );
    sc.call(ra(p, "setGradingPrimary"), vec![r(v1)]);
    sc.call(ra(p, "getGradingPrimary"), vec![]);
    render(&mut sc, c);
    let zero = sc.call(n("OCIO.GradingRGBM"), vec![f(0.0); 4]);
    sc.step(
        serde_json::json!({ "setattr": r(v1), "name": "gamma", "value": r(zero) }),
        Expect::Value,
    );
    sc.raises(ra(p, "setGradingPrimary"), vec![r(v1)]);
    sc.call(ra(p, "getGradingPrimary"), vec![]);
    sc.raises(ra(p, "setGradingPrimary"), vec![none()]);
    sc.raises(ra(p, "setGradingPrimary"), vec![r(tone)]);
    other_kinds(&mut sc, p, "GradingPrimary", &kinds);
    // A GradingTone.
    let gt = sc.call(
        n("OCIO.GradingToneTransform"),
        vec![n("OCIO.GRADING_LOG"), b(true)],
    );
    let c = cpu(&mut sc, gt);
    let p = prop(&mut sc, c, "GRADING_TONE");
    sc.call(ra(p, "getType"), vec![]);
    let v1 = sc.call(ra(p, "getGradingTone"), vec![]);
    let v2 = sc.call(ra(p, "getGradingTone"), vec![]);
    sc.call(n("operator.is_"), vec![r(v1), r(v2)]);
    sc.step(
        serde_json::json!({ "setattr": r(v1), "name": "scontrast", "value": f(1.5) }),
        Expect::Value,
    );
    sc.call(ra(p, "getGradingTone"), vec![]);
    sc.call(ra(p, "setGradingTone"), vec![r(v1)]);
    sc.call(ra(p, "getGradingTone"), vec![]);
    render(&mut sc, c);
    sc.step(
        serde_json::json!({ "setattr": r(v1), "name": "scontrast", "value": f(3.0) }),
        Expect::Value,
    );
    sc.raises(ra(p, "setGradingTone"), vec![r(v1)]);
    sc.raises(ra(p, "setGradingTone"), vec![none()]);
    other_kinds(&mut sc, p, "GradingTone", &kinds);
    sc
}

/// The curves of the curve properties: the property's own object, a change through it shown
/// by the getter (not in the pixels) until a setter replaces the curves.
fn curves_script() -> Script {
    let mut sc = Script::new();
    let double = sc.call(n("float"), vec![f(0.5)]);
    let primary = sc.call(n("OCIO.GradingPrimary"), vec![n("OCIO.GRADING_LOG")]);
    let rgb = sc.call(n("OCIO.GradingRGBCurve"), vec![n("OCIO.GRADING_LOG")]);
    let hue = sc.call(n("OCIO.GradingHueCurve"), vec![n("OCIO.GRADING_LOG")]);
    let tone = sc.call(n("OCIO.GradingTone"), vec![n("OCIO.GRADING_LOG")]);
    let kinds = [
        (double, "Double"),
        (primary, "GradingPrimary"),
        (rgb, "GradingRGBCurve"),
        (hue, "GradingHueCurve"),
        (tone, "GradingTone"),
    ];
    for (class, kind, ty, curve, getters) in [
        (
            "GradingRGBCurveTransform",
            "GradingRGBCurve",
            "GRADING_RGBCURVE",
            "red",
            &["isIdentity", "red", "green"][..],
        ),
        (
            "GradingHueCurveTransform",
            "GradingHueCurve",
            "GRADING_HUECURVE",
            "sat_sat",
            &["isIdentity", "getDrawCurveOnly", "sat_sat", "__repr__"][..],
        ),
    ] {
        let t = sc.call(
            n(&format!("OCIO.{class}")),
            vec![n("OCIO.GRADING_LOG"), b(true)],
        );
        let c = cpu(&mut sc, t);
        render(&mut sc, c);
        let p = prop(&mut sc, c, ty);
        sc.call(ra(p, "getType"), vec![]);
        let v = sc.call(ra(p, &format!("get{kind}")), vec![]);
        sc.eval(r(v), getters);
        let v2 = sc.call(ra(p, &format!("get{kind}")), vec![]);
        sc.call(n("operator.is_"), vec![r(v), r(v2)]);
        let p2 = prop(&mut sc, c, ty);
        let v3 = sc.call(ra(p2, &format!("get{kind}")), vec![]);
        sc.call(n("operator.is_"), vec![r(v), r(v3)]);
        // Changed through the object: the getter shows it, the pixels don't.
        let it = sc.call(ra(v, &format!("{curve}.getControlPoints")), vec![]);
        let point = sc.call(n("OCIO.GradingControlPoint"), vec![f(0.5), f(0.7)]);
        sc.call(ra(it, "__setitem__"), vec![i(1), r(point)]);
        sc.eval(r(v2), getters);
        render(&mut sc, c);
        // Set back: a copy, fitted; the object given isn't the property's.
        sc.call(ra(p, &format!("set{kind}")), vec![r(v)]);
        render(&mut sc, c);
        let v4 = sc.call(ra(p, &format!("get{kind}")), vec![]);
        sc.call(n("operator.is_"), vec![r(v), r(v4)]);
        sc.eval(r(v4), getters);
        // Curves refused (x decreasing): the property's object stays.
        let bad = sc.call(n(&format!("OCIO.{kind}")), vec![n("OCIO.GRADING_LOG")]);
        let it = sc.call(ra(bad, &format!("{curve}.getControlPoints")), vec![]);
        let point = sc.call(n("OCIO.GradingControlPoint"), vec![f(2.0), f(0.7)]);
        sc.call(ra(it, "__setitem__"), vec![i(1), r(point)]);
        sc.raises(ra(p, &format!("set{kind}")), vec![r(bad)]);
        let v5 = sc.call(ra(p, &format!("get{kind}")), vec![]);
        sc.call(n("operator.is_"), vec![r(v4), r(v5)]);
        render(&mut sc, c);
        // Another kind's curves, other types.
        let other = if kind == "GradingRGBCurve" { hue } else { rgb };
        sc.raises(ra(p, &format!("set{kind}")), vec![r(other)]);
        sc.raises(ra(p, &format!("set{kind}")), vec![i(0)]);
        sc.raises(ra(p, &format!("set{kind}")), vec![]);
        other_kinds(&mut sc, p, kind, &kinds);
        // None on a property of another kind: the binding's error, before the pointer.
        let ec = sc.call_kw(
            n("OCIO.ExposureContrastTransform"),
            vec![],
            vec![("dynamicGamma", b(true))],
            Expect::Value,
        );
        let ce = cpu(&mut sc, ec);
        let pe = prop(&mut sc, ce, "GAMMA");
        sc.raises(ra(pe, &format!("set{kind}")), vec![none()]);
    }
    sc
}

/// The class's methods behave as the wheel's.
#[test]
fn properties_behave_as_the_wheels() {
    start();
    compare(&[properties_script(), curves_script()]);
}

/// The class can't be made from Python, and its methods take only its instances.
#[test]
fn the_class_and_its_casters() {
    start();
    let mut sc = Script::new();
    sc.raises(n("OCIO.DynamicProperty"), vec![]);
    sc.raises(n("OCIO.DynamicProperty.getType"), vec![i(0)]);
    sc.raises(n("OCIO.DynamicProperty.getDouble"), vec![]);
    let rgbm = sc.call(n("OCIO.GradingRGBM"), vec![]);
    sc.raises(n("OCIO.DynamicProperty.setDouble"), vec![r(rgbm), f(1.0)]);
    compare(&[sc]);
}

/// `None` for the curves of a curve property of that kind, which the wheel validates through
/// the null pointer (its process dies), is the port's error (U-167). Each script runs in a
/// process of its own in the wheel, which dies at its last step, and only there.
#[test]
fn null_curves_are_the_ports_error() {
    start();
    let mut scripts = Vec::new();
    for (class, kind, ty) in [
        (
            "GradingRGBCurveTransform",
            "GradingRGBCurve",
            "GRADING_RGBCURVE",
        ),
        (
            "GradingHueCurveTransform",
            "GradingHueCurve",
            "GRADING_HUECURVE",
        ),
    ] {
        let mut sc = Script::new();
        let t = sc.call(
            n(&format!("OCIO.{class}")),
            vec![n("OCIO.GRADING_LOG"), b(true)],
        );
        let c = cpu(&mut sc, t);
        let p = prop(&mut sc, c, ty);
        sc.call(ra(p, &format!("set{kind}")), vec![none()]);
        scripts.push((sc, kind));
    }
    let json: Vec<serde_json::Value> = scripts.iter().map(|(sc, _)| sc.json()).collect();
    let result = ocio_testkit::Oracle::get()
        .call("py_eval", serde_json::json!({ "scripts": json }), &[])
        .result;
    let reports = result["scripts"].as_array().unwrap();
    assert_eq!(reports.len(), 2);
    let mut shortened = Vec::new();
    for (k, report) in reports.iter().enumerate() {
        let code = report["crashed"]["returncode"].as_i64();
        assert!(code.is_some_and(|c| c != 0), "script {k}: {report}");
        let mut steps = scripts[k].0.json();
        steps["steps"].as_array_mut().unwrap().pop();
        shortened.push(steps);
    }
    for report in common::wheel(&shortened) {
        for step in report["steps"].as_array().unwrap() {
            assert!(step.get("value").is_some(), "{step}");
        }
    }
    for (sc, kind) in &scripts {
        let ours = common::port(&sc.json());
        let steps = ours["steps"].as_array().unwrap();
        let last = steps.last().unwrap();
        for step in &steps[..steps.len() - 1] {
            assert!(step.get("value").is_some(), "{step}");
        }
        assert_eq!(last["raised"]["type"], "builtins.ValueError", "{last}");
        let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
            &last["raised"]["message"],
        ))
        .unwrap();
        assert_eq!(
            message,
            format!(
                "PyOpenColorIO.PyOpenColorIO.{kind}: None for a curve, a null pointer the wheel \
                 dereferences (its process dies; docs/improvements.md, U-167)"
            )
        );
    }
}
