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
//!   (`Config::CreateRaw()->getProcessor(transform, direction)`), written out by
//!   `createGroupTransform()`, against the port's ops: `validate`, [`build_ops`], `finalize`,
//!   then [`create_transform`] for each op (`Processor::Impl::setTransform` and
//!   `createGroupTransform`, src/OpenColorIO/Processor.cpp:300-316, 623-641 @ v2.5.2). Every
//!   getter the dump holds is compared, floats by their bits; or the error and its message.

use std::collections::BTreeMap;

use ocio::transform::{build_ops, create_transform};
use ocio::{Config, FormatMetadata, GroupTransform, Transform, TransformDirection};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::BitDepth;
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::processor_ops::{Dump, Dumped, ProcessorOpsReply, ProcessorOpsRequest};
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
        _ => None,
    }
}

/// What the port's processor of `transform` in the direction `dir` makes of it: the group
/// `createGroupTransform()` returns, or the error.
///
/// Port of `Processor::Impl::setTransform` and `createGroupTransform` (src/OpenColorIO/
/// Processor.cpp:300-316, 623-641 @ v2.5.2), for transforms without dynamic properties.
pub(crate) fn port_processor_group(
    transform: &Transform,
    dir: TransformDirection,
) -> ocio::Result<GroupTransform> {
    transform.validate()?;
    let config = Config::create_raw();
    let mut ops = OpVec::new();
    build_ops(&mut ops, &config, config.current_context(), transform, dir)?;
    ops.finalize()?;

    let mut group = GroupTransform::new();
    *group.format_metadata_mut() = ops.get_format_metadata().clone();
    for op in ops.iter() {
        create_transform(&mut group, op)?;
    }
    Ok(group)
}

/// The wheel's processors of `cases`, in both directions, against the port's.
pub(crate) fn check_processors(cases: &[Case]) {
    let dirs = [TransformDirection::Forward, TransformDirection::Inverse];
    let requests: Vec<(usize, TransformDirection, ProcessorOpsRequest)> = cases
        .iter()
        .enumerate()
        .flat_map(|(k, case)| {
            dirs.map(|dir| {
                let mut request = ProcessorOpsRequest::new(
                    json!({"transform": case.spec, "direction": direction_name(dir)}),
                );
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
        let port = port_processor_group(&case.port, *dir);
        let outcome = match (reply.raised(), &port) {
            (Some(raised), Err(e)) => {
                if raised.stage == "processor" && raised.message == e.message() {
                    None
                } else {
                    Some(format!("wheel {raised:?}\n  port  {e:?}"))
                }
            }
            (None, Ok(group)) => compare_group(&reply.processor().group, group)
                .err()
                .map(|e| format!("{e}\n  wheel {}", reply.result)),
            (wheel, port) => Some(format!(
                "wheel {wheel:?} {}\n  port  {port:?}",
                reply.result
            )),
        };
        if let Some(failure) = outcome {
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
        other => panic!("no dump for {other:?}"),
    };
    (class.to_string(), getters)
}
