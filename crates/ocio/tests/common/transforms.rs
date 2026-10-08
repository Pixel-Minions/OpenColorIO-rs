// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Checks of the port's transforms against the wheel's, shared by the transform classes'
//! oracle tests. Each case is a transform spec for the oracle and the port's transform built
//! the same way.
//!
//! - [`check_text`]: through `transform_text`, `repr()` and `str()` (upstream's `operator<<`)
//!   against `Display`, byte for byte; what `validate()` raises against `validate`'s message;
//!   and `equals()` for pairs of cases against the port's `equals`.
//! - [`check_processors`]: through `processor_ops`, the raw config's processor of each case
//!   (`Config::CreateRaw()->getProcessor(transform, direction)`) and its optimized processor,
//!   against the port's [`Config::processor_in_direction`] and
//!   [`Processor::optimized_processor_with_bit_depths`]: their cache IDs, `isNoOp`,
//!   `hasChannelCrosstalk`, `isDynamic`, the files and looks they read, and
//!   `createGroupTransform()`, every getter of every transform the dump holds, floats by their
//!   bits; or the error, its message and where it happened.
//! - [`check_optimized_processors`]: the same for the optimized processors of other bit
//!   depths and the default flags, where the optimizer bakes and folds LUTs.

use std::collections::BTreeMap;
use std::sync::Arc;

use ocio::{
    Config, FixedFunctionStyle, FormatMetadata, GroupTransform, Interpolation, Lut1DHueAdjust,
    NegativeStyle, OptimizationFlags, Processor, RangeStyle, Transform, TransformDirection,
};
use ocio_ops::open_color_types::{BitDepth, CdlStyle};
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::processor_ops::{
    Dump, Dumped, ProcessorDump, ProcessorOpsReply, ProcessorOpsRequest,
};
use ocio_testkit::transform_text::{Built, TransformTextReply, TransformTextRequest};
use serde_json::{Value, json};

/// A transform, as the oracle builds it and as the port does.
#[derive(Debug, Clone)]
pub(crate) struct Case {
    /// What the case is, for failure messages.
    pub(crate) label: String,
    /// The oracle's spec of the transform.
    pub(crate) spec: Value,
    /// The port's transform.
    pub(crate) port: Transform,
}

impl Case {
    pub(crate) fn new(label: impl Into<String>, spec: Value, port: impl Into<Transform>) -> Case {
        Case {
            label: label.into(),
            spec,
            port: port.into(),
        }
    }
}

/// A group of `cases`, in order, in the direction `dir`.
pub(crate) fn group(label: &str, dir: TransformDirection, cases: &[Case]) -> Case {
    let mut port = GroupTransform::new();
    port.set_direction(dir);
    for case in cases {
        port.append_transform(case.port.clone());
    }
    Case::new(
        label,
        json!({"class": "GroupTransform",
            "calls": [["setDirection", direction_spec(dir)]],
            "children": cases.iter().map(|c| c.spec.clone()).collect::<Vec<_>>()}),
        port,
    )
}

/// A direction, as the oracle's specs name it.
pub(crate) fn direction_name(dir: TransformDirection) -> &'static str {
    match dir {
        TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
        TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
    }
}

/// A direction, as a spec value.
pub(crate) fn direction_spec(dir: TransformDirection) -> Value {
    json!({"enum": direction_name(dir)})
}

/// A bit depth, as the binding names it.
pub(crate) fn bit_depth_name(depth: BitDepth) -> &'static str {
    match depth {
        BitDepth::Unknown => "BIT_DEPTH_UNKNOWN",
        BitDepth::Uint8 => "BIT_DEPTH_UINT8",
        BitDepth::Uint10 => "BIT_DEPTH_UINT10",
        BitDepth::Uint12 => "BIT_DEPTH_UINT12",
        BitDepth::Uint14 => "BIT_DEPTH_UINT14",
        BitDepth::Uint16 => "BIT_DEPTH_UINT16",
        BitDepth::Uint32 => "BIT_DEPTH_UINT32",
        BitDepth::F16 => "BIT_DEPTH_F16",
        BitDepth::F32 => "BIT_DEPTH_F32",
    }
}

/// A negative style, as the binding names it.
pub(crate) fn negative_style_name(style: NegativeStyle) -> &'static str {
    match style {
        NegativeStyle::Clamp => "NEGATIVE_CLAMP",
        NegativeStyle::Mirror => "NEGATIVE_MIRROR",
        NegativeStyle::PassThru => "NEGATIVE_PASS_THRU",
        NegativeStyle::Linear => "NEGATIVE_LINEAR",
    }
}

/// A negative style, as a spec value.
pub(crate) fn negative_style_spec(style: NegativeStyle) -> Value {
    json!({"enum": negative_style_name(style)})
}

/// Every bit depth.
pub(crate) const BIT_DEPTHS: [BitDepth; 9] = [
    BitDepth::Unknown,
    BitDepth::Uint8,
    BitDepth::Uint10,
    BitDepth::Uint12,
    BitDepth::Uint14,
    BitDepth::Uint16,
    BitDepth::Uint32,
    BitDepth::F16,
    BitDepth::F32,
];

/// A bit depth, as a spec value.
pub(crate) fn bit_depth_spec(depth: BitDepth) -> Value {
    json!({"enum": bit_depth_name(depth)})
}

/// Doubles that print or compare in their own way: NaNs of both signs (a quiet one, a
/// signalling one, one with a payload), the infinities, both zeros, the smallest subnormal,
/// the extremes, values at the edges of 6 and 16 significant digits, and ordinary ones.
pub(crate) fn special_doubles() -> Vec<f64> {
    let mut values: Vec<f64> = [
        0x7ff8_0000_0000_0000u64,
        0xfff8_0000_0000_0000,
        0x7ff0_0000_0000_0001,
        0xfff0_0000_0000_0001,
        0x7ff8_0000_dead_beef,
    ]
    .map(f64::from_bits)
    .to_vec();
    values.extend([
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.0,
        -0.0,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::MIN,
        1e300,
        -1e-300,
        f64::from(f32::MAX),
        f64::from(f32::MIN_POSITIVE),
        0.1,
        1.0 / 3.0,
        -2.0 / 3.0,
        1e-7,
        123_456.5,
        1_234_567.0,
        999_999.5,
        9_999_999_999_999_998.0,
        1e15 + 0.3,
        1e16,
        123_456_789_012_345_680.0,
        0.5,
        -1.0,
        2.0,
    ]);
    values
}

/// The port's text, validation and equality, against the wheel's.
pub(crate) fn check_text(cases: &[Case], pairs: &[(usize, usize)]) {
    let reply = TransformTextRequest {
        transforms: cases.iter().map(|c| c.spec.clone()).collect(),
        pairs: pairs.to_vec(),
    }
    .run();
    compare_text(cases, pairs, &reply);
}

/// [`check_text`]'s comparison of a reply.
pub(crate) fn compare_text(cases: &[Case], pairs: &[(usize, usize)], reply: &TransformTextReply) {
    let mut failures = Vec::new();
    for (case, built) in cases.iter().zip(&reply.transforms) {
        let Built::Text(text) = built else {
            failures.push(format!(
                "{}: the wheel raised building it: {built:?}",
                case.label
            ));
            continue;
        };
        let port_text = case.port.to_string();
        let port_validate = case.port.validate().err().map(|e| e.message().to_string());
        let wheel_validate = text.validate.as_ref().map(|e| e.message.clone());
        if port_text != text.repr || port_text != text.str || port_validate != wheel_validate {
            failures.push(format!(
                "{}\n  wheel {:?}\n        {wheel_validate:?}\n  port  {port_text:?}\n        \
                 {port_validate:?}",
                case.label, text.repr
            ));
        }
    }
    for (&(i, j), wheel) in pairs.iter().zip(&reply.pairs) {
        let port = port_equals(&cases[i].port, &cases[j].port);
        if port != *wheel {
            failures.push(format!(
                "{} equals {}: wheel {wheel:?}, port {port:?}",
                cases[i].label, cases[j].label
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `a.equals(b)` where the binding has it: for two transforms of a class that has `equals`.
fn port_equals(a: &Transform, b: &Transform) -> Option<bool> {
    match (a, b) {
        (Transform::Matrix(a), Transform::Matrix(b)) => Some(a.equals(b)),
        (Transform::Range(a), Transform::Range(b)) => Some(a.equals(b)),
        (Transform::Lut1D(a), Transform::Lut1D(b)) => Some(a.equals(b)),
        (Transform::Cdl(a), Transform::Cdl(b)) => Some(a.equals(b)),
        (Transform::Log(a), Transform::Log(b)) => Some(a.equals(b)),
        (Transform::LogAffine(a), Transform::LogAffine(b)) => Some(a.equals(b)),
        (Transform::LogCamera(a), Transform::LogCamera(b)) => Some(a.equals(b)),
        (Transform::Exponent(a), Transform::Exponent(b)) => Some(a.equals(b)),
        (Transform::ExponentWithLinear(a), Transform::ExponentWithLinear(b)) => Some(a.equals(b)),
        (Transform::FixedFunction(a), Transform::FixedFunction(b)) => Some(a.equals(b)),
        _ => None,
    }
}

/// A processor and the group its `createGroupTransform()` returns.
pub(crate) type ProcessorAndGroup = (Arc<Processor>, GroupTransform);

/// The port's processor of `transform` in the direction `dir`, and its optimized processor for
/// the bit depths `input` and `output` and the flags `flags`, each with its
/// `createGroupTransform()`; or the error, and the step that raised it, as `processor_ops`
/// names it ("processor", "group", "optimize" or "optimized_group").
///
/// `config` is copied first: the oracle makes a config per request, so its cache is empty.
pub(crate) fn port_processors(
    config: &Config,
    transform: &Transform,
    dir: TransformDirection,
    (input, output, flags): (BitDepth, BitDepth, OptimizationFlags),
) -> Result<(ProcessorAndGroup, ProcessorAndGroup), (&'static str, ocio::Exception)> {
    let config = config.clone();
    let processor = config
        .processor_in_direction(transform, dir)
        .map_err(|e| ("processor", e))?;
    let group = processor
        .create_group_transform()
        .map_err(|e| ("group", e))?;
    let optimized = processor
        .optimized_processor_with_bit_depths(input, output, flags)
        .map_err(|e| ("optimize", e))?;
    let optimized_group = optimized
        .create_group_transform()
        .map_err(|e| ("optimized_group", e))?;
    Ok(((processor, group), (optimized, optimized_group)))
}

/// The wheel's reply to a `processor_ops` request against the port's processors: `None` when
/// they agree.
pub(crate) fn compare_reply(
    reply: &ProcessorOpsReply,
    port: &Result<(ProcessorAndGroup, ProcessorAndGroup), (&'static str, ocio::Exception)>,
) -> Option<String> {
    match (reply.raised(), port) {
        (Some(raised), Err((stage, e))) => {
            if raised.stage == *stage && raised.message == e.message() {
                None
            } else {
                Some(format!("wheel {raised:?}\n  port  {stage} {e:?}"))
            }
        }
        (None, Ok(((processor, group), (optimized, optimized_group)))) => {
            compare_processor(reply.processor(), processor, group)
                .map_err(|e| format!("processor: {e}"))
                .and_then(|()| {
                    compare_processor(reply.optimized(), optimized, optimized_group)
                        .map_err(|e| format!("optimized processor: {e}"))
                })
                .err()
                .map(|e| format!("{e}\n  wheel {}", reply.result))
        }
        (wheel, port) => Some(format!(
            "wheel {wheel:?} {}\n  port  {port:?}",
            reply.result
        )),
    }
}

/// The wheel's processor, as `processor_ops` writes it out, against the port's.
fn compare_processor(
    wheel: &ProcessorDump,
    port: &Processor,
    group: &GroupTransform,
) -> Result<(), String> {
    let cache_id = port.cache_id().map_err(|e| e.to_string())?;
    let is_no_op = port.is_no_op().map_err(|e| e.to_string())?;
    let port_flags = (
        cache_id.as_str(),
        is_no_op,
        port.has_channel_crosstalk(),
        port.is_dynamic(),
    );
    let wheel_flags = (
        wheel.cache_id.as_str(),
        wheel.is_no_op,
        wheel.has_channel_crosstalk,
        wheel.is_dynamic,
    );
    if port_flags != wheel_flags {
        return Err(format!(
            "(cache ID, isNoOp, hasChannelCrosstalk, isDynamic): wheel {wheel_flags:?}, port \
             {port_flags:?}"
        ));
    }
    let metadata = port.processor_metadata();
    let files: Vec<Dumped> = (0..metadata.num_files())
        .map(|i| dumped_str(metadata.file(i)))
        .collect();
    let looks: Vec<Dumped> = (0..metadata.num_looks())
        .map(|i| dumped_str(metadata.look(i)))
        .collect();
    for (name, port_list) in [("getFiles", files), ("getLooks", looks)] {
        let port_list = Dumped::List(port_list);
        if wheel.processor_metadata.getter(name) != &port_list {
            return Err(format!(
                "{name}: wheel {:?}, port {port_list:?}",
                wheel.processor_metadata.getter(name)
            ));
        }
    }
    compare_group(&wheel.group, group)
}

/// The raw config's processors of `cases`, in both directions, the wheel's against the port's.
pub(crate) fn check_processors(cases: &[Case]) {
    check_processors_in(cases, None, &Config::create_raw().unwrap());
}

/// The processors of `cases` in a config, in both directions, the wheel's against the port's:
/// `config_spec` is the oracle's config (see `spec.config`; `None` for the raw config) and
/// `config` the port's, which must be the same.
pub(crate) fn check_processors_in(cases: &[Case], config_spec: Option<&Value>, config: &Config) {
    let dirs = [TransformDirection::Forward, TransformDirection::Inverse];
    check_processors_in_dirs(cases, config_spec, config, &dirs);
}

/// The raw config's processors of `cases` in the directions `dirs`, the wheel's against the
/// port's.
pub(crate) fn check_processors_dirs(cases: &[Case], dirs: &[TransformDirection]) {
    check_processors_in_dirs(cases, None, &Config::create_raw().unwrap(), dirs);
}

/// The processors of `cases` in a config, in the directions `dirs`, the wheel's against the
/// port's (see [`check_processors_in`]).
pub(crate) fn check_processors_in_dirs(
    cases: &[Case],
    config_spec: Option<&Value>,
    config: &Config,
    dirs: &[TransformDirection],
) {
    let requests: Vec<(usize, TransformDirection, ProcessorOpsRequest)> = cases
        .iter()
        .enumerate()
        .flat_map(|(k, case)| {
            dirs.iter().map(move |&dir| {
                let mut processor =
                    json!({"transform": case.spec, "direction": direction_name(dir)});
                if let Some(config_spec) = config_spec {
                    processor["config"] = config_spec.clone();
                }
                let mut request = ProcessorOpsRequest::new(processor);
                request.optimization = Some(json!("OPTIMIZATION_NONE"));
                (k, dir, request)
            })
        })
        .collect();
    let calls: Vec<BatchCall<'_>> = requests.iter().map(|(_, _, r)| r.call()).collect();
    let mut failures = Vec::new();
    for ((k, dir, _), response) in requests.iter().zip(Oracle::get().batch(&calls, true)) {
        let reply = ProcessorOpsReply::from_response(response.unwrap_or_else(|e| panic!("{e}")));
        let case = &cases[*k];
        let port = port_processors(
            config,
            &case.port,
            *dir,
            (BitDepth::F32, BitDepth::F32, OptimizationFlags::NONE),
        );
        if let Some(failure) = compare_reply(&reply, &port) {
            failures.push(format!("{} ({dir:?}): {failure}", case.label));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The wheel's group, as `processor_ops` writes it out, against the port's.
fn compare_group(wheel: &Dump, port: &GroupTransform) -> Result<(), String> {
    if wheel.class != "GroupTransform" {
        return Err(format!("the wheel's group is a {}", wheel.class));
    }
    let metadata = dump_metadata(port.format_metadata());
    if wheel.getter("getFormatMetadata") != &metadata {
        return Err(format!(
            "group metadata: wheel {:?}, port {metadata:?}",
            wheel.getter("getFormatMetadata")
        ));
    }
    let n = port.num_transforms() as usize;
    if wheel.children.len() != n {
        return Err(format!(
            "{} transforms in the wheel's group, {n} in the port's",
            wheel.children.len()
        ));
    }
    for (i, child) in wheel.children.iter().enumerate() {
        let port_child = port.transform(i as i32).map_err(|e| e.to_string())?;
        let (class, getters) = dump_transform(port_child);
        if child.class != class || child.getters != getters {
            return Err(format!(
                "transform {i}:\n  wheel {} {:?}\n  port  {class} {getters:?}",
                child.class, child.getters
            ));
        }
    }
    Ok(())
}

/// A string as the dump holds it.
fn dumped_str(bytes: &[u8]) -> Dumped {
    Dumped::Str(String::from_utf8(bytes.to_vec()).expect("UTF-8 metadata"))
}

/// The port's metadata, as `checks.dump` writes out a `FormatMetadata`.
pub(crate) fn dump_metadata(metadata: &FormatMetadata) -> Dumped {
    let attributes = (0..metadata.get_num_attributes())
        .map(|i| {
            Dumped::List(vec![
                dumped_str(metadata.get_attribute_name(i)),
                dumped_str(metadata.get_attribute_value(i)),
            ])
        })
        .collect();
    let children = (0..metadata.get_num_children_elements())
        .map(|i| dump_metadata(metadata.get_child_element(i).expect("a child element")))
        .collect();
    let getters = BTreeMap::from([
        ("getAttributes".to_string(), Dumped::List(attributes)),
        ("getChildElements".to_string(), Dumped::List(children)),
        (
            "getElementName".to_string(),
            dumped_str(metadata.get_element_name()),
        ),
        (
            "getElementValue".to_string(),
            dumped_str(metadata.get_element_value()),
        ),
        ("getID".to_string(), dumped_str(metadata.get_id())),
        ("getName".to_string(), dumped_str(metadata.get_name())),
    ]);
    Dumped::Object(Dump {
        class: "FormatMetadata".to_string(),
        getters,
        properties: BTreeMap::new(),
        uncalled: Vec::new(),
        children: Vec::new(),
    })
}

/// An enum value, as the dump holds it.
pub(crate) fn dumped_enum(name: &str) -> Dumped {
    Dumped::Enum(name.to_string())
}

/// Doubles, as the dump holds them.
pub(crate) fn dumped_f64s(values: &[f64]) -> Dumped {
    Dumped::List(values.iter().map(|&v| Dumped::F64(v)).collect())
}

/// The port's transform, as `checks.dump` writes it out: its class and every getter that takes
/// no argument.
pub(crate) fn dump_transform(transform: &Transform) -> (String, BTreeMap<String, Dumped>) {
    let mut getters = BTreeMap::new();
    let mut put = |name: &str, value: Dumped| {
        getters.insert(name.to_string(), value);
    };
    put(
        "getDirection",
        dumped_enum(direction_name(transform.direction())),
    );
    let class = match transform {
        Transform::Matrix(t) => {
            put("getTransformType", dumped_enum("TRANSFORM_TYPE_MATRIX"));
            put(
                "getFileInputBitDepth",
                dumped_enum(bit_depth_name(t.file_input_bit_depth())),
            );
            put(
                "getFileOutputBitDepth",
                dumped_enum(bit_depth_name(t.file_output_bit_depth())),
            );
            put("getFormatMetadata", dump_metadata(t.format_metadata()));
            put("getMatrix", dumped_f64s(&t.matrix()));
            put("getOffset", dumped_f64s(&t.offset()));
            "MatrixTransform"
        }
        Transform::Exponent(t) => {
            put("getTransformType", dumped_enum("TRANSFORM_TYPE_EXPONENT"));
            put("getFormatMetadata", dump_metadata(t.format_metadata()));
            put(
                "getNegativeStyle",
                dumped_enum(negative_style_name(t.negative_style())),
            );
            put("getValue", dumped_f64s(&t.value()));
            "ExponentTransform"
        }
        Transform::ExponentWithLinear(t) => {
            put(
                "getTransformType",
                dumped_enum("TRANSFORM_TYPE_EXPONENT_WITH_LINEAR"),
            );
            put("getFormatMetadata", dump_metadata(t.format_metadata()));
            put(
                "getNegativeStyle",
                dumped_enum(negative_style_name(t.negative_style())),
            );
            put("getGamma", dumped_f64s(&t.gamma()));
            put("getOffset", dumped_f64s(&t.offset()));
            "ExponentWithLinearTransform"
        }
        Transform::Log(t) => {
            put("getTransformType", dumped_enum("TRANSFORM_TYPE_LOG"));
            put("getFormatMetadata", dump_metadata(t.format_metadata()));
            put("getBase", Dumped::F64(t.base()));
            "LogTransform"
        }
        Transform::LogAffine(t) => {
            put("getTransformType", dumped_enum("TRANSFORM_TYPE_LOG_AFFINE"));
            put("getFormatMetadata", dump_metadata(t.format_metadata()));
            put("getBase", Dumped::F64(t.base()));
            put(
                "getLogSideSlopeValue",
                dumped_f64s(&t.log_side_slope_value()),
            );
            put(
                "getLogSideOffsetValue",
                dumped_f64s(&t.log_side_offset_value()),
            );
            put(
                "getLinSideSlopeValue",
                dumped_f64s(&t.lin_side_slope_value()),
            );
            put(
                "getLinSideOffsetValue",
                dumped_f64s(&t.lin_side_offset_value()),
            );
            "LogAffineTransform"
        }
        Transform::LogCamera(t) => {
            put("getTransformType", dumped_enum("TRANSFORM_TYPE_LOG_CAMERA"));
            put("getFormatMetadata", dump_metadata(t.format_metadata()));
            put("getBase", Dumped::F64(t.base()));
            put(
                "getLogSideSlopeValue",
                dumped_f64s(&t.log_side_slope_value()),
            );
            put(
                "getLogSideOffsetValue",
                dumped_f64s(&t.log_side_offset_value()),
            );
            put(
                "getLinSideSlopeValue",
                dumped_f64s(&t.lin_side_slope_value()),
            );
            put(
                "getLinSideOffsetValue",
                dumped_f64s(&t.lin_side_offset_value()),
            );
            put(
                "getLinSideBreakValue",
                dumped_f64s(&t.lin_side_break_value()),
            );
            // The binding returns three quiet NaNs where the slope isn't set
            // (src/bindings/python/transforms/PyLogCameraTransform.cpp:156-166 @ v2.5.2).
            let slope = t.linear_slope_value();
            put(
                "getLinearSlopeValue",
                dumped_f64s(&slope.unwrap_or([f64::NAN; 3])),
            );
            put("isLinearSlopeValueSet", Dumped::Bool(slope.is_some()));
            "LogCameraTransform"
        }
        Transform::Cdl(t) => {
            put("getTransformType", dumped_enum("TRANSFORM_TYPE_CDL"));
            put("getFormatMetadata", dump_metadata(t.format_metadata()));
            put("getSlope", dumped_f64s(&t.slope()));
            put("getOffset", dumped_f64s(&t.offset()));
            put("getPower", dumped_f64s(&t.power()));
            put("getSOP", dumped_f64s(&t.sop()));
            put("getSat", Dumped::F64(t.sat()));
            put("getSatLumaCoefs", dumped_f64s(&t.sat_luma_coefs()));
            put(
                "getStyle",
                dumped_enum(match t.style() {
                    CdlStyle::Asc => "CDL_ASC",
                    CdlStyle::NoClamp => "CDL_NO_CLAMP",
                }),
            );
            put("getID", dumped_str(t.id()));
            put(
                "getFirstSOPDescription",
                dumped_str(t.first_sop_description()),
            );
            "CDLTransform"
        }
        Transform::Range(t) => {
            put("getTransformType", dumped_enum("TRANSFORM_TYPE_RANGE"));
            put(
                "getFileInputBitDepth",
                dumped_enum(bit_depth_name(t.file_input_bit_depth())),
            );
            put(
                "getFileOutputBitDepth",
                dumped_enum(bit_depth_name(t.file_output_bit_depth())),
            );
            put("getFormatMetadata", dump_metadata(t.format_metadata()));
            put(
                "getStyle",
                dumped_enum(match t.style() {
                    RangeStyle::Clamp => "RANGE_CLAMP",
                    RangeStyle::NoClamp => "RANGE_NO_CLAMP",
                }),
            );
            put("getMinInValue", Dumped::F64(t.min_in_value()));
            put("getMaxInValue", Dumped::F64(t.max_in_value()));
            put("getMinOutValue", Dumped::F64(t.min_out_value()));
            put("getMaxOutValue", Dumped::F64(t.max_out_value()));
            put("hasMinInValue", Dumped::Bool(t.has_min_in_value()));
            put("hasMaxInValue", Dumped::Bool(t.has_max_in_value()));
            put("hasMinOutValue", Dumped::Bool(t.has_min_out_value()));
            put("hasMaxOutValue", Dumped::Bool(t.has_max_out_value()));
            "RangeTransform"
        }
        Transform::Lut1D(t) => {
            put("getTransformType", dumped_enum("TRANSFORM_TYPE_LUT1D"));
            put(
                "getFileOutputBitDepth",
                dumped_enum(bit_depth_name(t.file_output_bit_depth())),
            );
            put("getFormatMetadata", dump_metadata(t.format_metadata()));
            put("getHueAdjust", dumped_enum(hue_adjust_name(t.hue_adjust())));
            put("getInputHalfDomain", Dumped::Bool(t.input_half_domain()));
            put("getOutputRawHalfs", Dumped::Bool(t.output_raw_halfs()));
            put(
                "getInterpolation",
                dumped_enum(interpolation_name(t.interpolation())),
            );
            put("getLength", Dumped::Int(t.length() as i64));
            // The binding's getData(): the values of each entry, in a float32 array.
            let values: Vec<f32> = (0..t.length())
                .flat_map(|i| t.value(i).expect("an entry"))
                .collect();
            put(
                "getData",
                Dumped::Array {
                    dtype: "float32".to_string(),
                    shape: vec![values.len() as u64],
                    bytes: values.iter().flat_map(|v| v.to_le_bytes()).collect(),
                },
            );
            "Lut1DTransform"
        }
        Transform::FixedFunction(t) => {
            put(
                "getTransformType",
                dumped_enum("TRANSFORM_TYPE_FIXED_FUNCTION"),
            );
            put("getFormatMetadata", dump_metadata(t.format_metadata()));
            put(
                "getStyle",
                dumped_enum(fixed_function_style_name(t.style())),
            );
            put("getParams", dumped_f64s(&t.params()));
            "FixedFunctionTransform"
        }
        other => panic!("no dump for {other:?}"),
    };
    (class.to_string(), getters)
}

/// The raw config's processors of `cases`, forward, and their optimized processors for each
/// `(input, output)` bit depth pair with the default flags, the wheel's (`getProcessor`,
/// `getOptimizedProcessor`, then `createGroupTransform()`) against the port's
/// ([`port_processors`]): the processors' cache IDs and flags, and every getter, a LUT's values
/// bit for bit. Returns, per case, how many of its optimized processors hold a Lut1DTransform.
pub(crate) fn check_optimized_processors(cases: &[Case], depths: &[(Depth, Depth)]) -> Vec<usize> {
    let requests: Vec<(usize, Depth, Depth, ProcessorOpsRequest)> = cases
        .iter()
        .enumerate()
        .flat_map(|(k, case)| {
            depths.iter().map(move |&(input, output)| {
                let mut request = ProcessorOpsRequest::new(
                    json!({"transform": case.spec, "direction": direction_name(TransformDirection::Forward)}),
                );
                request.in_bitdepth = Some(input);
                request.out_bitdepth = Some(output);
                request.optimization = Some(json!("OPTIMIZATION_DEFAULT"));
                (k, input, output, request)
            })
        })
        .collect();
    let calls: Vec<BatchCall<'_>> = requests.iter().map(|(.., r)| r.call()).collect();
    let config = Config::create_raw().unwrap();
    let mut failures = Vec::new();
    let mut luts = vec![0; cases.len()];
    for ((k, input, output, _), response) in requests.iter().zip(Oracle::get().batch(&calls, true))
    {
        let reply = ProcessorOpsReply::from_response(response.unwrap_or_else(|e| panic!("{e}")));
        if let Some(optimized) = &reply.optimized
            && optimized.classes().contains(&"Lut1DTransform")
        {
            luts[*k] += 1;
        }
        let case = &cases[*k];
        let port = port_processors(
            &config,
            &case.port,
            TransformDirection::Forward,
            (
                port_depth(*input),
                port_depth(*output),
                OptimizationFlags::DEFAULT,
            ),
        );
        let outcome = compare_reply(&reply, &port);
        if let Some(failure) = outcome {
            failures.push(format!(
                "{} ({input:?} -> {output:?}): {failure}",
                case.label
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    luts
}

/// Transforms whose building raises in the wheel (`transform_text`'s `Raised`: a constructor or
/// a setter of the spec), against the port's error for the same call, by message.
pub(crate) fn setter_errors(checks: &[(&str, Value, ocio::Result<()>)]) {
    let reply = TransformTextRequest {
        transforms: checks.iter().map(|(_, spec, _)| spec.clone()).collect(),
        pairs: Vec::new(),
    }
    .run();
    let mut failures = Vec::new();
    for ((label, _, port), built) in checks.iter().zip(&reply.transforms) {
        let wheel = match built {
            Built::Raised(e) => Some(e.message.clone()),
            Built::Text(_) => None,
        };
        let port = port.as_ref().err().map(|e| e.message().to_string());
        if wheel.is_none() || wheel != port {
            failures.push(format!("{label}: wheel {wheel:?}, port {port:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The port's bit depth of a battery bit depth.
pub(crate) fn port_depth(depth: Depth) -> BitDepth {
    match depth {
        Depth::Uint8 => BitDepth::Uint8,
        Depth::Uint10 => BitDepth::Uint10,
        Depth::Uint12 => BitDepth::Uint12,
        Depth::Uint16 => BitDepth::Uint16,
        Depth::F16 => BitDepth::F16,
        Depth::F32 => BitDepth::F32,
    }
}

/// An interpolation, as the binding names it.
pub(crate) fn interpolation_name(interp: Interpolation) -> &'static str {
    match interp {
        Interpolation::Unknown => "INTERP_UNKNOWN",
        Interpolation::Nearest => "INTERP_NEAREST",
        Interpolation::Linear => "INTERP_LINEAR",
        Interpolation::Tetrahedral => "INTERP_TETRAHEDRAL",
        Interpolation::Cubic => "INTERP_CUBIC",
        Interpolation::Default => "INTERP_DEFAULT",
        Interpolation::Best => "INTERP_BEST",
    }
}

/// A hue adjustment, as the binding names it.
pub(crate) fn hue_adjust_name(hue: Lut1DHueAdjust) -> &'static str {
    match hue {
        Lut1DHueAdjust::None => "HUE_NONE",
        Lut1DHueAdjust::Dw3 => "HUE_DW3",
        Lut1DHueAdjust::Wypn => "HUE_WYPN",
    }
}

/// A `FixedFunctionStyle`, as PyOpenColorIO names it.
pub(crate) fn fixed_function_style_name(style: FixedFunctionStyle) -> &'static str {
    use FixedFunctionStyle::*;
    match style {
        AcesRedMod03 => "FIXED_FUNCTION_ACES_RED_MOD_03",
        AcesRedMod10 => "FIXED_FUNCTION_ACES_RED_MOD_10",
        AcesGlow03 => "FIXED_FUNCTION_ACES_GLOW_03",
        AcesGlow10 => "FIXED_FUNCTION_ACES_GLOW_10",
        AcesDarkToDim10 => "FIXED_FUNCTION_ACES_DARK_TO_DIM_10",
        Rec2100Surround => "FIXED_FUNCTION_REC2100_SURROUND",
        RgbToHsv => "FIXED_FUNCTION_RGB_TO_HSV",
        XyzToXyy => "FIXED_FUNCTION_XYZ_TO_xyY",
        XyzToUvy => "FIXED_FUNCTION_XYZ_TO_uvY",
        XyzToLuv => "FIXED_FUNCTION_XYZ_TO_LUV",
        AcesGamutMap02 => "FIXED_FUNCTION_ACES_GAMUTMAP_02",
        AcesGamutMap07 => "FIXED_FUNCTION_ACES_GAMUTMAP_07",
        AcesGamutComp13 => "FIXED_FUNCTION_ACES_GAMUT_COMP_13",
        LinToPq => "FIXED_FUNCTION_LIN_TO_PQ",
        LinToGammaLog => "FIXED_FUNCTION_LIN_TO_GAMMA_LOG",
        LinToDoubleLog => "FIXED_FUNCTION_LIN_TO_DOUBLE_LOG",
        AcesOutputTransform20 => "FIXED_FUNCTION_ACES_OUTPUT_TRANSFORM_20",
        AcesRgbToJmh20 => "FIXED_FUNCTION_ACES_RGB_TO_JMH_20",
        AcesTonescaleCompress20 => "FIXED_FUNCTION_ACES_TONESCALE_COMPRESS_20",
        AcesGamutCompress20 => "FIXED_FUNCTION_ACES_GAMUT_COMPRESS_20",
        RgbToHsyLin => "FIXED_FUNCTION_RGB_TO_HSY_LIN",
        RgbToHsyLog => "FIXED_FUNCTION_RGB_TO_HSY_LOG",
        RgbToHsyVid => "FIXED_FUNCTION_RGB_TO_HSY_VID",
    }
}
