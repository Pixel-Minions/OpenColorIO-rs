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
