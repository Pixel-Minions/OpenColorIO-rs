// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! OCIO 2.6.0's cycle guards (PR #2318): `BuildOps` (src/OpenColorIO/Transform.cpp:52-71 @
//! v2.6.0) and `CollectContextVariables` (ContextVariableUtils.cpp:182-204 @ v2.6.0) each count
//! their nesting in a thread-local depth and throw once it passes 32, cycle or not. Chains a
//! few levels either side of the limit, of each kind of level, against the wheel: whether
//! `getProcessor` and `isColorSpaceLinear` (which builds ops without collecting context
//! variables first) succeed, with the processor cache on and off, and the exact message where
//! they throw.

use ocio::{Config, ProcessorCacheFlags, ReferenceSpaceType};
use ocio_ops::platform::{MapEnv, set_thread_env_provider};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::bytes;
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::sync::Arc;

/// The chain lengths tried: the limit is crossed between them for every kind below.
const LENGTHS: std::ops::RangeInclusive<usize> = 12..=38;

/// A config of `n` color spaces after `cs0`, each one's `from_scene_reference` the link
/// `link(i)` to the one before; `extra` comes after the color spaces.
fn chain(n: usize, link: impl Fn(usize) -> String, extra: &str) -> String {
    let mut yaml = String::from(
        "ocio_profile_version: 2\nroles:\n  default: cs0\ncolorspaces:\n  - !<ColorSpace>\n    name: cs0\n",
    );
    for i in 1..=n {
        let _ = write!(
            yaml,
            "  - !<ColorSpace>\n    name: cs{i}\n    from_scene_reference: {}\n",
            link(i)
        );
    }
    yaml.push_str(extra);
    yaml
}

/// The kinds of chain: a color space transform to the previous color space; the same inside
/// a group; a look transform whose look holds it; a color space whose transform is `n`
/// groups around a matrix.
fn kinds(n: usize) -> Vec<(&'static str, String)> {
    let cst = |i: usize| format!("!<ColorSpaceTransform> {{src: cs0, dst: cs{}}}", i - 1);
    let grouped = |i: usize| {
        format!(
            "!<GroupTransform> {{children: [!<ColorSpaceTransform> {{src: cs0, dst: cs{}}}]}}",
            i - 1
        )
    };
    let mut looks = String::from("looks:\n");
    for i in 1..=n {
        let _ = write!(
            looks,
            "  - !<Look>\n    name: l{i}\n    process_space: cs0\n    transform: !<ColorSpaceTransform> {{src: cs0, dst: cs{}}}\n",
            i - 1
        );
    }
    let look = |i: usize| format!("!<LookTransform> {{src: cs0, dst: cs0, looks: l{i}}}");
    let mut groups = String::from("!<MatrixTransform> {offset: [0.1, 0, 0, 0]}");
    for _ in 0..n {
        groups = format!("!<GroupTransform> {{children: [{groups}]}}");
    }
    let nested = format!(
        "ocio_profile_version: 2\nroles:\n  default: cs0\ncolorspaces:\n  - !<ColorSpace>\n    name: cs0\n  - !<ColorSpace>\n    name: cs{n}\n    from_scene_reference: {groups}\n"
    );
    vec![
        ("color space transforms", chain(n, cst, "")),
        ("color space transforms in groups", chain(n, grouped, "")),
        ("look transforms", chain(n, look, &looks)),
        ("groups", nested),
    ]
}

/// What a call gave: `Ok` with the result (`isColorSpaceLinear`'s boolean, or null for a
/// processor), or `Err` with the exception's message.
fn wheel_outcome(call: &Value) -> Result<Value, Vec<u8>> {
    match call.get("exception") {
        Some(e) => Err(bytes(&e["message"])),
        None => Ok(if call["result"].is_boolean() {
            call["result"].clone()
        } else {
            Value::Null
        }),
    }
}

#[test]
fn chains_past_the_limit_throw_as_the_wheel_does() {
    set_thread_env_provider(Some(Arc::new(MapEnv::from_entries::<&str, &str>(&[]))));
    let mut cases = Vec::new();
    for n in LENGTHS {
        for (kind, yaml) in kinds(n) {
            cases.push((n, kind, yaml));
        }
    }
    let target = |n: usize| format!("cs{n}");
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(n, _, yaml)| BatchCall {
            cmd: "config_calls",
            args: json!({"config": {"yaml": yaml}, "calls": [
                {"call": "getProcessor", "args": ["cs0", target(*n)]},
                {"call": "isColorSpaceLinear",
                 "args": [target(*n), {"enum": "REFERENCE_SPACE_SCENE"}]},
                {"call": "setProcessorCacheFlags", "args": [{"enum": "PROCESSOR_CACHE_OFF"}]},
                {"call": "getProcessor", "args": ["cs0", target(*n)]},
            ]}),
            blobs: Vec::new(),
        })
        .collect();
    let wheel = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    let mut thrown = std::collections::BTreeMap::<&str, (usize, usize)>::new();
    for ((n, kind, yaml), w) in cases.iter().zip(wheel) {
        let w = w.expect("config_calls").result;
        assert!(
            w["config"].is_null(),
            "{kind} {n}: the wheel read the config: {w}"
        );
        let calls = w["calls"].as_array().unwrap();
        assert_eq!(calls.len(), 4, "{kind} {n}: {w}");
        assert!(calls[2].get("exception").is_none(), "{kind} {n}: {w}");
        let config = Config::create_from_stream(yaml.as_bytes()).unwrap();
        let port_processor = config
            .processor_between("cs0", target(*n))
            .map(|_| Value::Null)
            .map_err(|e| e.message().as_bytes().to_vec());
        let port_linear = config
            .is_color_space_linear(target(*n), ReferenceSpaceType::Scene)
            .map(Value::Bool)
            .map_err(|e| e.message().as_bytes().to_vec());
        config.set_processor_cache_flags(ProcessorCacheFlags::OFF);
        let port_uncached = config
            .processor_between("cs0", target(*n))
            .map(|_| Value::Null)
            .map_err(|e| e.message().as_bytes().to_vec());
        for (what, wheel, port) in [
            ("getProcessor", wheel_outcome(&calls[0]), port_processor),
            ("isColorSpaceLinear", wheel_outcome(&calls[1]), port_linear),
            (
                "getProcessor without the cache",
                wheel_outcome(&calls[3]),
                port_uncached,
            ),
        ] {
            let entry = thrown.entry(kind).or_default();
            if wheel.is_err() {
                entry.0 += 1;
            } else {
                entry.1 += 1;
            }
            if wheel != port {
                failures.push(format!(
                    "{kind} {n} {what}: wheel {:?}, port {:?}",
                    wheel.map_err(|m| String::from_utf8_lossy(&m).into_owned()),
                    port.map_err(|m| String::from_utf8_lossy(&m).into_owned())
                ));
            }
        }
    }
    // Not vacuous: for each kind the wheel both builds some chains and refuses others.
    assert_eq!(thrown.len(), 4, "{thrown:?}");
    for (kind, (refused, built)) in &thrown {
        assert!(
            *refused > 0 && *built > 0,
            "{kind}: {refused} refused, {built} built"
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
