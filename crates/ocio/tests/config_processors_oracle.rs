// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The processors a config makes through each `getProcessor` overload (WP 3.2d), with and
//! without a context, against the wheel's, through the oracle's `config_processor`: from two
//! color spaces (found by `getColorSpace`, a name it doesn't find passing `None`), from two
//! names, from a color space to a (display, view) pair, and from a named transform (found by
//! `getNamedTransform`, likewise) or its name. Each processor's cache ID, flags, files and
//! looks, `createGroupTransform()` and its optimized processor's, or the error and where it
//! happened, compare exactly.
//!
//! The context passed is a copy of the config's current context that sets the context
//! variable a color space's transform names, so the processor differs from the one without.
//!
//! Also: the processor cache keyed by a context variable of the destination only; transforms
//! that refer to themselves, where the wheel's stack overflows and the port refuses them; and
//! the longest chains of transforms the port accepts below its nesting limit
//! (docs/improvements.md, U-58).

mod common;

use std::sync::Arc;

use common::transforms::compare_processor;
use ocio::{
    BitDepth, Config, Context, GroupTransform, OptimizationFlags, Processor, TransformDirection,
};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::{BatchCall, Response};
use ocio_testkit::oracle_values::bytes;
use ocio_testkit::processor_ops::ProcessorOpsReply;
use serde_json::{Value, json};

use TransformDirection::{Forward, Inverse};

/// A config of every kind of source and destination: scene-referred color spaces (one by
/// alias, one whose transform names a color space through a context variable), a data one, a
/// display one, named transforms with a forward transform, an inverse one or both (by alias
/// too), views of a color space, a named transform, the context variable's color space and a
/// look.
const CONFIG: &str = r#"ocio_profile_version: 2

environment: {OCIO_RS_CP_VAR: cs}

search_path: ""
strictparsing: false
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: lin
  scene_linear: lin

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: ntview, colorspace: ntf}
    - !<View> {name: var, colorspace: varcs}
    - !<View> {name: look, colorspace: cs, looks: +lk}

looks:
  - !<Look>
    name: lk
    process_space: lin
    transform: !<CDLTransform> {slope: [1.1, 1, 1]}

display_colorspaces:
  - !<ColorSpace>
    name: dcs
    aliases: [display color space]
    isdata: false
    allocation: uniform
    from_display_reference: !<RangeTransform> {min_in_value: 0, min_out_value: 0}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true

  - !<ColorSpace>
    name: lin

  - !<ColorSpace>
    name: cs
    aliases: [colorspace]
    to_scene_reference: !<RangeTransform> {max_in_value: 1, max_out_value: 1}

  - !<ColorSpace>
    name: varcs
    from_scene_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {offset: [0.01, 0.02, 0.03, 0]}
        - !<ColorSpaceTransform> {src: lin, dst: $OCIO_RS_CP_VAR}

named_transforms:
  - !<NamedTransform>
    name: forward
    aliases: [nt1, ntf]
    encoding: scene-linear
    transform: !<MatrixTransform> {name: forward, offset: [0.1, 0.2, 0.3, 0.4]}

  - !<NamedTransform>
    name: inverse
    aliases: [nt2, nti]
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}

  - !<NamedTransform>
    name: both
    aliases: [nt3, ntb]
    transform: !<MatrixTransform> {name: forward, offset: [0.1, 0.2, 0.3, 0.4]}
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}
"#;

/// The context variable the context passed sets, and its value there.
const VAR: (&str, &str) = ("OCIO_RS_CP_VAR", "lin");

/// A `getProcessor` overload and its arguments, as the oracle's `config_processor` names them.
#[derive(Debug, Clone, Copy)]
enum Overload {
    ColorSpaces(&'static str, &'static str),
    Names(&'static str, &'static str),
    DisplayView(&'static str, &'static str, &'static str, TransformDirection),
    NamedTransform(&'static str, TransformDirection),
    NamedTransformName(&'static str, TransformDirection),
}

fn direction_name(dir: TransformDirection) -> &'static str {
    match dir {
        Forward => "TRANSFORM_DIR_FORWARD",
        Inverse => "TRANSFORM_DIR_INVERSE",
    }
}

impl Overload {
    fn spec(self) -> Value {
        match self {
            Overload::ColorSpaces(src, dst) => json!({"color_spaces": [src, dst]}),
            Overload::Names(src, dst) => json!({"names": [src, dst]}),
            Overload::DisplayView(src, display, view, dir) => {
                json!({"display_view": [src, display, view, direction_name(dir)]})
            }
            Overload::NamedTransform(name, dir) => {
                json!({"named_transform": [name, direction_name(dir)]})
            }
            Overload::NamedTransformName(name, dir) => {
                json!({"named_transform_name": [name, direction_name(dir)]})
            }
        }
    }

    /// The port's processor by the same overload, or its context variant with `context`.
    fn port(self, config: &Config, context: Option<&Context>) -> ocio::Result<Arc<Processor>> {
        match (self, context) {
            (Overload::ColorSpaces(src, dst), None) => config
                .processor_between_color_spaces(config.color_space(src), config.color_space(dst)),
            (Overload::ColorSpaces(src, dst), Some(ctx)) => config
                .processor_between_color_spaces_with_context(
                    ctx,
                    config.color_space(src),
                    config.color_space(dst),
                ),
            (Overload::Names(src, dst), None) => config.processor_between(src, dst),
            (Overload::Names(src, dst), Some(ctx)) => {
                config.processor_between_with_context(ctx, src, dst)
            }
            (Overload::DisplayView(src, display, view, dir), None) => {
                config.processor_for_display_view(src, display, view, dir)
            }
            (Overload::DisplayView(src, display, view, dir), Some(ctx)) => {
                config.processor_for_display_view_with_context(ctx, src, display, view, dir)
            }
            (Overload::NamedTransform(name, dir), None) => {
                config.processor_for_named_transform(config.named_transform(name), dir)
            }
            (Overload::NamedTransform(name, dir), Some(ctx)) => config
                .processor_for_named_transform_with_context(ctx, config.named_transform(name), dir),
            (Overload::NamedTransformName(name, dir), None) => {
                config.processor_for_named_transform_by_name(name, dir)
            }
            (Overload::NamedTransformName(name, dir), Some(ctx)) => {
                config.processor_for_named_transform_by_name_with_context(ctx, name, dir)
            }
        }
    }
}

/// Every overload over sources and destinations of each kind, those the config doesn't have
/// and names of the wrong kind.
fn overloads() -> Vec<Overload> {
    let mut list = Vec::new();
    for (src, dst) in [
        ("cs", "dcs"),
        ("colorspace", "display color space"),
        ("varcs", "cs"),
        ("lin", "varcs"),
        ("raw", "cs"),
        ("default", "varcs"),
        ("nope", "cs"),
        ("cs", "nope"),
        ("forward", "cs"),
        ("cs", ""),
    ] {
        list.push(Overload::ColorSpaces(src, dst));
        list.push(Overload::Names(src, dst));
    }
    for (src, dst) in [("dcs", "nti"), ("ntb", "forward"), ("lin", "inverse")] {
        list.push(Overload::Names(src, dst));
    }
    for dir in [Forward, Inverse] {
        for (src, display, view) in [
            ("cs", "sRGB", "var"),
            ("lin", "sRGB", "look"),
            ("varcs", "sRGB", "ntview"),
            ("dcs", "sRGB", "Raw"),
            ("raw", "sRGB", "look"),
            ("cs", "nope", "Raw"),
            ("cs", "sRGB", "nope"),
            ("", "sRGB", "Raw"),
            ("cs", "", "Raw"),
            ("cs", "sRGB", ""),
            ("nope", "sRGB", "Raw"),
        ] {
            list.push(Overload::DisplayView(src, display, view, dir));
        }
        for name in ["forward", "inverse", "both", "nt3", "nti", "nope", "", "cs"] {
            list.push(Overload::NamedTransform(name, dir));
            list.push(Overload::NamedTransformName(name, dir));
        }
    }
    list
}

/// A processor and its `createGroupTransform()`.
type WithGroup = (Arc<Processor>, GroupTransform);

/// The port's processor, its optimized processor (`OPTIMIZATION_NONE`, F32 in and out) and
/// each one's group, or the error and the stage that raised it, as `config_processor` names
/// it.
fn port_processors(
    processor: ocio::Result<Arc<Processor>>,
) -> Result<(WithGroup, WithGroup), (&'static str, ocio::Exception)> {
    let processor = processor.map_err(|e| ("processor", e))?;
    let group = processor
        .create_group_transform()
        .map_err(|e| ("group", e))?;
    let optimized = processor
        .optimized_processor_with_bit_depths(BitDepth::F32, BitDepth::F32, OptimizationFlags::NONE)
        .map_err(|e| ("optimize", e))?;
    let optimized_group = optimized
        .create_group_transform()
        .map_err(|e| ("optimized_group", e))?;
    Ok(((processor, group), (optimized, optimized_group)))
}

/// What the wheel raised, at a stage: (stage, message), from `{"exception": {"type",
/// "message": {"bytes"}}}`.
fn wheel_raised(stage: &str, part: &Value) -> Option<(String, Vec<u8>)> {
    let exception = part.get("exception")?;
    assert_eq!(exception["type"], "Exception", "{part}");
    Some((stage.to_string(), bytes(&exception["message"])))
}

/// The wheel's reply against the port's processors: `None` when they agree.
fn compare(
    response: &Response,
    port: &Result<(WithGroup, WithGroup), (&'static str, ocio::Exception)>,
) -> Option<String> {
    let result = &response.result;
    assert_eq!(result["config"], Value::Null, "{result}");
    let wheel_error = wheel_raised("processor", &result["processor"]).or_else(|| {
        let ops = &result["ops"];
        let stage = ops["stage"].as_str().unwrap_or_default();
        wheel_raised(stage, ops)
    });
    match (wheel_error, port) {
        (Some((stage, message)), Err((port_stage, e))) => {
            (stage != *port_stage || message != e.message().as_bytes()).then(|| {
                format!(
                    "wheel {stage} {:?}\n  port  {port_stage} {e:?}",
                    String::from_utf8_lossy(&message)
                )
            })
        }
        (None, Ok(((processor, group), (optimized, optimized_group)))) => {
            let cache_id = processor.cache_id().ok()?;
            let wheel_cache_id = bytes(&result["processor"]["cache_id"]);
            if wheel_cache_id != cache_id.as_bytes() {
                return Some(format!(
                    "cache ID: wheel {}, port {cache_id}",
                    String::from_utf8_lossy(&wheel_cache_id)
                ));
            }
            let reply = ProcessorOpsReply::from_response(Response {
                result: result["ops"].clone(),
                blobs: response.blobs.clone(),
            });
            compare_processor(reply.processor(), processor, group)
                .map_err(|e| format!("processor: {e}"))
                .and_then(|()| {
                    compare_processor(reply.optimized(), optimized, optimized_group)
                        .map_err(|e| format!("optimized processor: {e}"))
                })
                .err()
        }
        (wheel, port) => Some(format!("wheel {wheel:?} {result}\n  port  {port:?}")),
    }
}

/// Every overload, with and without a context, against the wheel.
#[test]
fn every_get_processor_overload_matches_the_wheel() {
    let config = Config::create_from_stream(CONFIG.as_bytes()).unwrap();
    let mut context = (*config.current_context().get()).clone();
    context.set_string_var(VAR.0, Some(VAR.1.as_bytes()));
    let context_spec = json!({"calls": [{"call": "setStringVar", "args": [VAR.0, VAR.1]}]});

    let requests: Vec<(Overload, bool, Value)> = overloads()
        .into_iter()
        .flat_map(|overload| {
            let context_spec = context_spec.clone();
            [false, true].into_iter().map(move |with_context| {
                let mut args = json!({
                    "config": {"yaml": CONFIG},
                    "overload": overload.spec(),
                    "ops": {"optimization": "OPTIMIZATION_NONE"},
                });
                if with_context {
                    args["context"] = context_spec.clone();
                }
                (overload, with_context, args)
            })
        })
        .collect();
    let calls: Vec<BatchCall<'_>> = requests
        .iter()
        .map(|(_, _, args)| BatchCall {
            cmd: "config_processor",
            args: args.clone(),
            blobs: vec![],
        })
        .collect();
    let mut failures = Vec::new();
    for ((overload, with_context, _), response) in
        requests.iter().zip(Oracle::get().batch(&calls, true))
    {
        let response = response.unwrap_or_else(|e| panic!("{e}"));
        // The oracle makes a config per request, so its cache is empty: so is a copy's.
        let config = (*config).clone();
        let port = port_processors(overload.port(&config, with_context.then_some(&context)));
        if let Some(failure) = compare(&response, &port) {
            failures.push(format!("{overload:?} (context: {with_context}): {failure}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The port's error for transforms nested deeper than its limit (docs/improvements.md, U-58).
const TOO_DEEP: &str = "levels deep: a color space, look, view transform or named transform may \
                        refer to itself.";

/// The port's error for transforms nested deeper than its limit weighing every level, groups
/// and CTF references included (docs/improvements.md, U-58).
const TOO_DEEP_TOTAL: &str = "The transforms nest too deep, counting groups and CTF references: \
                              upstream's stack overflows.";

/// Whether `message` is one of the port's errors for transforms nested too deep.
fn too_deep(message: &str) -> bool {
    message.ends_with(TOO_DEEP) || message.ends_with(TOO_DEEP_TOTAL)
}

/// A config whose color space `self` converts from itself, and whose color space `p`, the
/// process space of the look `k`, applies `k` to go to the reference: both nest without end.
const CYCLES: &str = r#"ocio_profile_version: 2
environment: {}
roles:
  default: lin
displays:
  d:
    - !<View> {name: v, colorspace: lin}
looks:
  - !<Look> {name: k, process_space: p, transform: !<MatrixTransform> {offset: [0.1, 0, 0, 0]}}
colorspaces:
  - !<ColorSpace>
    name: lin
  - !<ColorSpace>
    name: self
    to_scene_reference: !<ColorSpaceTransform> {src: self, dst: lin}
  - !<ColorSpace>
    name: p
    to_scene_reference: !<LookTransform> {src: lin, dst: lin, looks: k}
"#;

/// A color space that refers to itself, directly or through a look: the wheel overflows its
/// stack, which ends the process (the oracle reports no response); the port refuses it with an
/// error.
#[test]
fn transforms_that_refer_to_themselves_are_refused() {
    let config = Config::create_from_stream(CYCLES.as_bytes()).unwrap();
    for (src, dst) in [("self", "lin"), ("lin", "self"), ("p", "lin"), ("lin", "p")] {
        let wheel = Oracle::get().try_call(
            "config_processor",
            json!({"config": {"yaml": CYCLES}, "overload": {"names": [src, dst]}}),
            &[],
        );
        assert!(
            wheel.is_err(),
            "{src} -> {dst}: the wheel answered {:?}",
            wheel.map(|r| r.result)
        );

        let message = config
            .processor_between(src, dst)
            .err()
            .unwrap_or_else(|| panic!("{src} -> {dst}: no error"))
            .to_string();
        assert!(message.ends_with(TOO_DEEP), "{src} -> {dst}: {message}");
    }
}

/// A config of `n` color spaces `c1`..`cn`, each converting from the one before by `kind`: a
/// color space transform, a look transform, a group holding a color space transform, or a
/// display view transform of a view with a look, without or with a view transform; `lookopt`,
/// `dvopt` and `dvvtopt` with two look options (`k|j`) instead of one; with a suffix `:d`, that
/// transform inside `d` nested groups; with a suffix `:d@k:e`, inside `d` groups for the first
/// `k` links and `e` for the others.
fn chain(kind: &str, n: usize) -> String {
    let (kind, depths) = match kind.split_once(':') {
        Some((kind, depths)) => (kind, depths),
        None => (kind, "0"),
    };
    let depth_of = |i: usize| -> usize {
        match depths.split_once('@') {
            Some((first, rest)) => {
                let (k, other) = rest.split_once(':').unwrap();
                if i <= k.parse().unwrap() {
                    first.parse().unwrap()
                } else {
                    other.parse().unwrap()
                }
            }
            None => depths.parse().unwrap(),
        }
    };
    let mut s = String::from(
        "ocio_profile_version: 2
environment: {}
roles:
  default: c0
displays:
  d:
    - !<View> {name: v, colorspace: c0}
    - !<View> {name: lk, colorspace: c0, looks: k}
    - !<View> {name: vt, view_transform: t, display_colorspace: dc, looks: k}
    - !<View> {name: lkopt, colorspace: c0, looks: \"k|j\"}
    - !<View> {name: vtopt, view_transform: t, display_colorspace: dc, looks: \"k|j\"}
view_transforms:
  - !<ViewTransform> {name: t, from_scene_reference: !<MatrixTransform> {offset: [0, 0.1, 0, 0]}}
display_colorspaces:
  - !<ColorSpace> {name: dc, from_display_reference: !<MatrixTransform> {offset: [0, 0, 0.1, 0]}}
looks:
  - !<Look> {name: k, process_space: c0, transform: !<MatrixTransform> {offset: [0.1, 0, 0, 0]}}
  - !<Look> {name: j, process_space: c0, transform: !<MatrixTransform> {offset: [0.2, 0, 0, 0]}}
colorspaces:
  - !<ColorSpace>
    name: c0
",
    );
    for i in 1..=n {
        let prev = i - 1;
        let transform = match kind {
            "cst" => format!("!<ColorSpaceTransform> {{src: c{prev}, dst: c0}}"),
            "look" => format!("!<LookTransform> {{src: c{prev}, dst: c0, looks: k}}"),
            "lookopt" => format!("!<LookTransform> {{src: c{prev}, dst: c0, looks: \"k|j\"}}"),
            "group" => format!(
                "!<GroupTransform> {{children: [!<ColorSpaceTransform> {{src: c{prev}, dst: c0}}]}}"
            ),
            "dv" => format!("!<DisplayViewTransform> {{src: c{prev}, display: d, view: lk}}"),
            "dvvt" => format!("!<DisplayViewTransform> {{src: c{prev}, display: d, view: vt}}"),
            "dvopt" => format!("!<DisplayViewTransform> {{src: c{prev}, display: d, view: lkopt}}"),
            "dvvtopt" => {
                format!("!<DisplayViewTransform> {{src: c{prev}, display: d, view: vtopt}}")
            }
            _ => unreachable!("{kind}"),
        };
        let mut transform = transform;
        for _ in 0..depth_of(i) {
            transform = format!("!<GroupTransform> {{children: [{transform}]}}");
        }
        s += &format!("  - !<ColorSpace>\n    name: c{i}\n    to_scene_reference: {transform}\n");
    }
    s
}

/// The port's processor from the last color space of a chain of `n` to `c0`, or its error.
fn port_chain(kind: &str, n: usize) -> ocio::Result<Arc<Processor>> {
    let config = Config::create_from_stream(chain(kind, n).as_bytes())?;
    config.processor_between(format!("c{n}"), "c0")
}

/// The longest chain of each kind the port accepts ([`KINDS`]), alone and then the files, is
/// the wheel's processor, on the test thread's stack; one link more is the port's error, and
/// the wheel builds it too (it goes on to about twice as deep or more before its stack
/// overflows, docs/improvements.md U-58). A change to a weight or to the limit changes a
/// length.
#[test]
fn the_deepest_chains_below_the_limit_match_the_wheel() {
    let dir = reference_chain_files();
    let mut cases = Vec::new();
    for (kind, alone, then_files) in KINDS {
        cases.push((
            kind,
            false,
            alone,
            chain(kind, alone),
            chain(kind, alone + 1),
        ));
        cases.push((
            kind,
            true,
            then_files,
            chain_with_files(kind, &dir, then_files),
            chain_with_files(kind, &dir, then_files + 1),
        ));
    }
    let port = |yaml: &str, n: usize| {
        Config::create_from_stream(yaml.as_bytes())
            .and_then(|config| config.processor_between(format!("c{n}"), "c0"))
    };
    let mut calls = Vec::new();
    for (kind, files, n, longest, refused) in &cases {
        let label = format!("{kind} files={files} {n}");
        let error = port(refused, n + 1).map(|_| ()).unwrap_err().to_string();
        assert!(too_deep(&error), "{label}: one link more: {error}");
        for (yaml, n) in [(longest, *n), (refused, n + 1)] {
            calls.push(BatchCall {
                cmd: "config_processor",
                args: json!({
                    "config": {"yaml": yaml},
                    "overload": {"names": [format!("c{n}"), "c0"]},
                    "ops": {"optimization": "OPTIMIZATION_NONE"},
                }),
                blobs: vec![],
            });
        }
    }
    let replies = Oracle::get().batch(&calls, true);
    for (i, (kind, files, n, longest, _)) in cases.iter().enumerate() {
        let label = format!("{kind} files={files} {n}");
        let reply = |r: usize| {
            replies[2 * i + r]
                .as_ref()
                .unwrap_or_else(|e| panic!("{label}: {e}"))
        };
        // The deepest chain the port accepts is the wheel's processor.
        if let Some(failure) = compare(reply(0), &port_processors(port(longest, *n))) {
            panic!("{label}: {failure}");
        }
        // One link more, the wheel still makes a processor where the port refuses.
        assert!(
            reply(1).result["processor"]["cache_id"].is_object(),
            "{label}: {}",
            reply(1).result["processor"]
        );
    }
}

/// A context variable in the destination name only: the processors for each value of the
/// variable are the wheel's, through the port's one config and its processor cache (the cache
/// key holds the variable's value, `CollectContextVariables` of the destination).
#[test]
fn a_context_variable_of_the_destination_keys_the_cache() {
    const CONFIG: &str = "ocio_profile_version: 2
environment: {DST: a}
roles:
  default: lin
displays:
  d:
    - !<View> {name: v, colorspace: lin}
colorspaces:
  - !<ColorSpace>
    name: lin
  - !<ColorSpace>
    name: a
    from_scene_reference: !<MatrixTransform> {offset: [0.1, 0, 0, 0]}
  - !<ColorSpace>
    name: b
    from_scene_reference: !<MatrixTransform> {offset: [0, 0.2, 0, 0]}
";
    let config = Config::create_from_stream(CONFIG.as_bytes()).unwrap();
    let values = ["a", "b", "a"];
    let args: Vec<Value> = values
        .iter()
        .map(|value| {
            json!({"config": {"yaml": CONFIG}, "overload": {"names": ["lin", "$DST"]},
                   "context": {"calls": [{"call": "setStringVar", "args": ["DST", value]}]}})
        })
        .collect();
    let calls: Vec<BatchCall<'_>> = args
        .iter()
        .map(|args| BatchCall {
            cmd: "config_processor",
            args: args.clone(),
            blobs: vec![],
        })
        .collect();
    let wheel: Vec<String> = Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .map(|r| bytes(&r.unwrap_or_else(|e| panic!("{e}")).result["processor"]["cache_id"]))
        .map(|id| String::from_utf8(id).unwrap())
        .collect();
    let port: Vec<String> = values
        .iter()
        .map(|value| {
            let mut context = (*config.current_context().get()).clone();
            context.set_string_var("DST", Some(value.as_bytes()));
            let processor = config
                .processor_between_with_context(&context, "lin", "$DST")
                .unwrap();
            processor.cache_id().unwrap()
        })
        .collect();
    assert_eq!(port, wheel);
    assert_ne!(port[0], port[1]);
}

/// `f` on a thread of a 1 MiB stack, the smallest the port supports for its nesting limits
/// (docs/improvements.md, U-58 and U-76).
fn on_a_1_mib_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

/// The chain kinds ([`chain`]) of the tests of the limits, and the longest chain of each the
/// port accepts, alone and with its first link a file transform of 256 CTF files
/// ([`chain_with_files`]): each kind of link, links inside nested groups, which the total
/// counts, and two mixes of the heaviest chains of color spaces in groups, alone and then the
/// files (docs/improvements.md, U-58).
///
/// The lengths follow from the weights of the levels, which follow from the wheel's crashes
/// (U-58): the processor's own color space transform weighs 100, each link `w` (its transform
/// 100, 221 for a look transform, 238 for a display view transform, and 72 for each group
/// around it), and for the kinds of a look the look's matrix 100, at most 68,150 in all; then
/// files, `100 + w (n - 1) + 67 + 256 x 67` (the first link's file transform and its files).
/// And at most 256 transforms other than groups: the processor's, one a link, and the look's
/// matrix or the file transform. So a color space transform in 5 groups, `w` = 460, reaches
/// `100 + 460 n <= 68,150`: 147 links.
const KINDS: [(&str, usize, usize); 26] = [
    ("cst", 255, 255),
    ("look", 254, 231),
    ("group", 255, 255),
    ("dv", 254, 214),
    ("dvvt", 254, 214),
    ("cst:1", 255, 255),
    ("cst:2", 255, 209),
    ("cst:3", 215, 161),
    ("cst:5", 147, 111),
    ("cst:20", 44, 34),
    ("cst:200", 4, 4),
    ("look:2", 186, 140),
    ("group:3", 175, 132),
    ("group:20", 42, 32),
    ("dv:1", 219, 164),
    ("dv:2", 177, 134),
    ("dv:3", 149, 112),
    ("dvvt:10", 70, 54),
    ("lookopt", 254, 231),
    ("dvopt", 254, 214),
    ("dvvtopt", 254, 214),
    ("lookopt:1", 231, 174),
    ("dvopt:1", 219, 164),
    ("dvvtopt:2", 177, 134),
    // 255 links: the first 80 in 3 groups, the others in 2, 590 groups (68,080 alone; the
    // verifier's heaviest mix, 2026-10-17).
    ("cst:3@80:2", 255, 186),
    // Then files, 255 links: 2 groups around links 2 to 99, 1 around the others (68,063).
    ("cst:2@99:1", 255, 255),
];

/// The longest chain of each kind the port accepts ([`KINDS`]) builds on a 1 MiB thread, both
/// ways, and one link more is refused: the limits of nested transforms hold at the test
/// profile's optimization level, and at opt-level 0 (the figures in docs/improvements.md, U-58).
#[test]
fn the_deepest_chains_build_on_a_1_mib_thread() {
    for (kind, n, _) in KINDS {
        let refused = port_chain(kind, n + 1).map(|_| ()).unwrap_err().to_string();
        assert!(too_deep(&refused), "{kind} {}: {refused}", n + 1);
        let built = on_a_1_mib_thread(move || {
            let config = Config::create_from_stream(chain(kind, n).as_bytes())?;
            config.processor_between(format!("c{n}"), "c0")?;
            config.processor_between("c0", format!("c{n}")).map(|_| ())
        });
        assert!(built.is_ok(), "{kind} {n}: {built:?}");
    }
}

/// The chain of `kind` of `n` color spaces whose first link `c1` converts from `c0` with a file
/// transform of the CTF file `c0.ctf` of `dir`, the search path.
fn chain_with_files(kind: &str, dir: &str, n: usize) -> String {
    let yaml = chain(kind, n).replacen(
        "ocio_profile_version: 2\n",
        // A list, whose entries aren't split at colons (a Windows drive's, I-9).
        &format!("ocio_profile_version: 2\nsearch_path: [\"{dir}\"]\n"),
        1,
    );
    let start = yaml.find("    name: c1\n    to_scene_reference: ").unwrap();
    let end = start
        + yaml[start..]
            .find("\n  - !<ColorSpace>\n    name: c2\n")
            .unwrap_or(
                // A chain of one link.
                yaml.len() - start - 1,
            );
    format!(
        "{}    name: c1\n    to_scene_reference: !<FileTransform> {{src: c0.ctf}}{}",
        &yaml[..start],
        &yaml[end..]
    )
}

/// A config of color spaces `c1`..`cn`: `c1` converts from `c0` with a file transform of the
/// CTF file `c0.ctf` of `dir`, each other one from the one before with a color space
/// transform; `dir` is the search path.
fn mixed_chain(dir: &str, n: usize) -> String {
    chain_with_files("cst", dir, n)
}

/// A chain of `FILES` CTF files of `dir`, `c0.ctf` the first: each references the next, and
/// the last holds a matrix.
fn reference_chain_files() -> String {
    use common::lut_files::{file, write_files};

    let f32s = "inBitDepth=\"32f\" outBitDepth=\"32f\"";
    let mut files = Vec::new();
    for i in 0..FILES {
        let body = if i == FILES - 1 {
            format!(
                "    <Matrix {f32s}>\n        <Array dim=\"3 3 3\">2 0 0 0 3 0 0 0 4</Array>\n    \
                 </Matrix>\n"
            )
        } else {
            format!("    <Reference {f32s} path=\"c{}.ctf\"/>\n", i + 1)
        };
        files.push(file(
            &format!("c{i}.ctf"),
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <ProcessList id=\"a\" version=\"1.7\">\n{body}</ProcessList>\n"
            )
            .into_bytes(),
        ));
    }
    write_files(&files)
}

/// The longest chain of CTF references the port accepts (U-76).
const FILES: usize = 256;

/// The longest chain of each kind the port accepts whose first link is a file transform of the
/// longest chain of CTF references it accepts ([`KINDS`]) builds on a 1 MiB thread, both ways,
/// and one link more is refused: the chain of transforms comes first on the stack and the files
/// after it (docs/improvements.md, U-58 and U-76).
#[test]
fn the_deepest_chains_of_transforms_then_of_references_build_on_a_1_mib_thread() {
    let dir = reference_chain_files();
    for (kind, _, n) in KINDS {
        let refused = Config::create_from_stream(chain_with_files(kind, &dir, n + 1).as_bytes())
            .and_then(|config| config.processor_between(format!("c{}", n + 1), "c0"))
            .map(|_| ())
            .unwrap_err()
            .to_string();
        assert!(too_deep(&refused), "{kind} {}: {refused}", n + 1);
        let yaml = chain_with_files(kind, &dir, n);
        let built = on_a_1_mib_thread(move || {
            let config = Config::create_from_stream(yaml.as_bytes())?;
            let forward = config.processor_between(format!("c{n}"), "c0")?;
            config.processor_between("c0", format!("c{n}"))?;
            Ok::<_, ocio::Exception>(forward.processor_metadata().num_files())
        });
        assert_eq!(
            built.map_err(|e| e.to_string()),
            Ok(FILES as i32),
            "{kind} {n}"
        );
    }
}

/// The deepest stack of both limits: the longest chain of color space transforms the port
/// accepts, whose first link is a file transform of a chain of CTF references as long as the
/// port accepts (256 files): references never nest inside `BuildOps`, so the chain of color
/// spaces comes first on the stack and the files after it. It builds on a 1 MiB thread, and
/// its processor is the wheel's (docs/improvements.md, U-58 and U-76).
#[test]
fn a_chain_of_color_spaces_then_of_references_builds_on_a_1_mib_thread() {
    let dir = reference_chain_files();

    let port = |n: usize| {
        let yaml = mixed_chain(&dir, n);
        Config::create_from_stream(yaml.as_bytes())
            .and_then(|config| config.processor_between(format!("c{n}"), "c0"))
    };
    // The longest ([`KINDS`]).
    let n = 255;
    assert!(port(n + 1).is_err());

    let yaml = mixed_chain(&dir, n);
    let threaded = yaml.clone();
    let built = on_a_1_mib_thread(move || {
        Config::create_from_stream(threaded.as_bytes())
            .and_then(|config| config.processor_between(format!("c{n}"), "c0"))
            .map(|processor| processor.processor_metadata().num_files())
    });
    assert_eq!(built.map_err(|e| e.to_string()), Ok(FILES as i32));

    let reply = Oracle::get().call(
        "config_processor",
        json!({
            "config": {"yaml": yaml},
            "overload": {"names": [format!("c{n}"), "c0"]},
            "ops": {"optimization": "OPTIMIZATION_NONE"},
        }),
        &[],
    );
    if let Some(failure) = compare(&reply, &port_processors(port(n))) {
        panic!("mixed {n}: {failure}");
    }
}

/// Groups count in the limit of the total nesting: 255 color spaces, each a color space
/// transform from the one before inside 5 nested groups, are refused with the total's error,
/// where the wheel builds them (it crashes from 7 groups on Windows; docs/improvements.md,
/// U-58), and the same chain inside 2 groups builds.
#[test]
fn groups_count_in_the_total_nesting() {
    let message = port_chain("cst:5", 255).unwrap_err().to_string();
    assert!(message.ends_with(TOO_DEEP_TOTAL), "{message}");
    assert!(port_chain("cst:2", 213).is_ok());

    let reply = Oracle::get().call(
        "config_processor",
        json!({
            "config": {"yaml": chain("cst:5", 255)},
            "overload": {"names": ["c255", "c0"]},
            "ops": {"optimization": "OPTIMIZATION_NONE"},
        }),
        &[],
    );
    assert!(
        reply.result["processor"]["cache_id"].is_object(),
        "{}",
        reply.result["processor"]
    );
}

/// The groups count in the limit of the total nesting without the processor cache too: the
/// verifier's chain of 255 color spaces each inside 5 groups is refused. (The processor still
/// runs `CollectContextVariables` first, as upstream's does, `Config.cpp:4815`; the test of
/// `BuildOps` alone is `the_nesting_weight_limit_is_exact`, `crates/ocio/src/transform_tests.rs`.)
#[test]
fn groups_count_in_the_total_without_the_processor_cache() {
    let config = Config::create_from_stream(chain("cst:5", 255).as_bytes()).unwrap();
    config.set_processor_cache_flags(ocio::ProcessorCacheFlags::OFF);
    let message = config
        .processor_between("c255", "c0")
        .unwrap_err()
        .to_string();
    assert!(message.ends_with(TOO_DEEP_TOTAL), "{message}");
    let config = Config::create_from_stream(chain("cst:5", 100).as_bytes()).unwrap();
    config.set_processor_cache_flags(ocio::ProcessorCacheFlags::OFF);
    config.processor_between("c100", "c0").unwrap();
}
