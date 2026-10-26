// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! PyConfig.cpp:355-384, 810-963 @ v2.5.2: all ConfigUtils statics, every overload,
//! nullable contexts, keyword dispatch, processor text/IDs and rendered RGB against the wheel.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, compare, f, l, n, none, r, ra, s};

const STATICS: &[&str] = &[
    "IdentifyBuiltinColorSpace",
    "IdentifyInterchangeSpace",
    "GetProcessorToBuiltinColorSpace",
    "GetProcessorFromBuiltinColorSpace",
    "GetProcessorFromConfigs",
];

#[test]
fn the_config_utils_statics_have_the_wheels_surface() {
    let report = common::wheel_surface();
    let theirs = common::wheel_member(&report, "Config");
    let ours = common::port_member("Config");
    // Each method is compared below by the surface walk's bytes-encoded name.
    let select = |value: &serde_json::Value| {
        value["members"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|member| {
                let name = ocio_testkit::oracle_values::bytes(&member["name"]);
                STATICS.iter().any(|expected| expected.as_bytes() == name)
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    let theirs = select(&theirs);
    assert_eq!(theirs.len(), STATICS.len());
    assert_eq!(select(&ours), theirs);
}

fn processor(sc: &mut Script, p: usize) {
    sc.eval(r(p), &["getCacheID", "isNoOp", "hasChannelCrosstalk"]);
    let group = sc.call(ra(p, "createGroupTransform"), vec![]);
    sc.repr(r(group));
    let cpu = sc.call(ra(p, "getDefaultCPUProcessor"), vec![]);
    sc.call(
        ra(cpu, "applyRGB"),
        vec![l(vec![f(0.125), f(0.25), f(0.5)])],
    );
}

#[test]
fn identification_and_builtin_processors_are_the_wheels() {
    let mut sc = Script::new();
    let cfg = sc.call(
        n("OCIO.Config.CreateFromBuiltinConfig"),
        vec![s("cg-config-latest")],
    );
    for name in ["ACEScg", "ACES2065-1", "Raw"] {
        sc.call(
            n("OCIO.Config.IdentifyBuiltinColorSpace"),
            vec![r(cfg), r(cfg), s(name)],
        );
        sc.call(
            n("OCIO.Config.IdentifyInterchangeSpace"),
            vec![r(cfg), s("ACEScg"), r(cfg), s(name)],
        );
        let p = sc.call(
            n("OCIO.Config.GetProcessorToBuiltinColorSpace"),
            vec![r(cfg), s("ACEScg"), s(name)],
        );
        processor(&mut sc, p);
        let p = sc.call(
            n("OCIO.Config.GetProcessorFromBuiltinColorSpace"),
            vec![s(name), r(cfg), s("ACEScg")],
        );
        processor(&mut sc, p);
    }
    sc.raises(
        n("OCIO.Config.IdentifyBuiltinColorSpace"),
        vec![r(cfg), r(cfg), s("missing")],
    );
    sc.raises(
        n("OCIO.Config.IdentifyBuiltinColorSpace"),
        vec![none(), r(cfg), s("missing")],
    );
    sc.raises(
        n("OCIO.Config.IdentifyInterchangeSpace"),
        vec![none(), s("ACEScg"), r(cfg), s("missing")],
    );
    sc.raises(
        n("OCIO.Config.GetProcessorToBuiltinColorSpace"),
        vec![none(), s("ACEScg"), s("missing")],
    );
    sc.raises(
        n("OCIO.Config.GetProcessorFromBuiltinColorSpace"),
        vec![s("missing"), none(), s("ACEScg")],
    );
    compare(&[sc]);
}

#[test]
fn every_from_configs_overload_is_the_wheels() {
    let mut sc = Script::new();
    let cfg = sc.call(
        n("OCIO.Config.CreateFromBuiltinConfig"),
        vec![s("cg-config-latest")],
    );
    let ctx = sc.call(ra(cfg, "getCurrentContext"), vec![]);
    let display = sc.call(ra(cfg, "getDefaultDisplay"), vec![]);
    let view = sc.call(ra(cfg, "getDefaultView"), vec![r(display)]);
    let forward = n("OCIO.TRANSFORM_DIR_FORWARD");
    for args in [
        vec![r(cfg), s("ACEScg"), r(cfg), s("ACES2065-1")],
        vec![r(ctx), r(cfg), s("ACEScg"), r(ctx), r(cfg), s("ACES2065-1")],
        vec![
            r(cfg),
            s("ACEScg"),
            s("ACES2065-1"),
            r(cfg),
            s("ACES2065-1"),
            s("ACES2065-1"),
        ],
        vec![
            r(ctx),
            r(cfg),
            s("ACEScg"),
            s("ACES2065-1"),
            r(ctx),
            r(cfg),
            s("ACES2065-1"),
            s("ACES2065-1"),
        ],
        vec![
            r(cfg),
            s("ACEScg"),
            r(cfg),
            r(display),
            r(view),
            forward.clone(),
        ],
        vec![
            r(ctx),
            r(cfg),
            s("ACEScg"),
            r(ctx),
            r(cfg),
            r(display),
            r(view),
            forward.clone(),
        ],
        vec![
            r(cfg),
            s("ACEScg"),
            s("ACES2065-1"),
            r(cfg),
            r(display),
            r(view),
            s("ACES2065-1"),
            forward.clone(),
        ],
        vec![
            r(ctx),
            r(cfg),
            s("ACEScg"),
            s("ACES2065-1"),
            r(ctx),
            r(cfg),
            r(display),
            r(view),
            s("ACES2065-1"),
            forward.clone(),
        ],
    ] {
        let p = sc.call(n("OCIO.Config.GetProcessorFromConfigs"), args);
        processor(&mut sc, p);
    }
    // The sixth overload's binding names the display slot dstView and the view slot
    // dstDisplay, so these reversed keywords have precisely the positional call's result.
    let p = sc.call_kw(
        n("OCIO.Config.GetProcessorFromConfigs"),
        vec![],
        vec![
            ("srcContext", r(ctx)),
            ("srcConfig", r(cfg)),
            ("srcColorSpaceName", s("ACEScg")),
            ("dstContext", r(ctx)),
            ("dstConfig", r(cfg)),
            ("dstView", r(display)),
            ("dstDisplay", r(view)),
            ("direction", forward),
        ],
        Expect::Value,
    );
    processor(&mut sc, p);
    for args in [
        vec![none(), r(cfg), s("ACEScg"), r(ctx), r(cfg), s("ACES2065-1")],
        vec![r(ctx), r(cfg), s("ACEScg"), none(), r(cfg), s("ACES2065-1")],
        vec![
            none(),
            r(cfg),
            s("missing"),
            r(ctx),
            r(cfg),
            s("ACES2065-1"),
        ],
        vec![
            none(),
            r(cfg),
            s("ACEScg"),
            s("missing"),
            none(),
            r(cfg),
            s("ACES2065-1"),
            s("ACES2065-1"),
        ],
    ] {
        sc.raises(n("OCIO.Config.GetProcessorFromConfigs"), args);
    }
    sc.raises(n("OCIO.Config.GetProcessorFromConfigs"), vec![]);
    // pybind11 converts every argument before the function body inspects null names.
    // A later incompatible argument therefore wins over the deliberate null-name error.
    sc.raises(
        n("OCIO.Config.GetProcessorFromConfigs"),
        vec![r(cfg), none(), common::script::i(2), s("ACEScg")],
    );
    sc.raises(
        n("OCIO.Config.IdentifyInterchangeSpace"),
        vec![r(cfg), none(), common::script::i(2), s("ACEScg")],
    );
    sc.raises(
        n("OCIO.Config.GetProcessorFromBuiltinColorSpace"),
        vec![none(), common::script::i(2), s("ACEScg")],
    );
    compare(&[sc]);
}

#[test]
fn defined_errors_before_a_null_config_are_the_wheels() {
    let mut sc = Script::new();
    let cfg = sc.call(
        n("OCIO.Config.CreateFromBuiltinConfig"),
        vec![s("cg-config-latest")],
    );
    let ctx = sc.call(ra(cfg, "getCurrentContext"), vec![]);
    let raw = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let direction = n("OCIO.TRANSFORM_DIR_FORWARD");
    for args in [
        vec![r(ctx), none(), s("ACEScg"), r(ctx), r(cfg), s("missing")],
        vec![
            r(ctx),
            r(cfg),
            s("missing"),
            s("ACES2065-1"),
            r(ctx),
            none(),
            s("ACEScg"),
            s("ACES2065-1"),
        ],
        vec![
            r(ctx),
            r(cfg),
            s("missing"),
            r(ctx),
            none(),
            s("display"),
            s("view"),
            direction.clone(),
        ],
        vec![
            r(ctx),
            r(raw),
            s("raw"),
            r(ctx),
            none(),
            s("display"),
            s("view"),
            direction.clone(),
        ],
        vec![
            none(),
            r(cfg),
            s("ACEScg"),
            s("ACES2065-1"),
            r(ctx),
            none(),
            s("display"),
            s("view"),
            s("ACES2065-1"),
            direction,
        ],
    ] {
        sc.raises(n("OCIO.Config.GetProcessorFromConfigs"), args);
    }
    compare(&[sc]);
}
