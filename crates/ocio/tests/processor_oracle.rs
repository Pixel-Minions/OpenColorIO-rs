// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The processor against the wheel, through the oracle's `processor_ops`: `Config::getProcessor`
//! and `getOptimizedProcessor` for groups of groups, nested, in both directions, and the
//! processors' cache ID, flags, metadata and `createGroupTransform()`.
//!
//! Here, groups of groups, whose processors have no ops, and a group whose optimized processor
//! depends on the output bit depth. The processors of every other class, and their
//! `createGroupTransform()`, are checked by each class's oracle test (`common/transforms.rs`,
//! `check_processors`).

mod common;

use common::transforms::{self, Case};
use ocio::{
    BitDepth, Config, FormatMetadata, GroupTransform, MatrixTransform, OptimizationFlags,
    Processor, RangeTransform, Transform, TransformDirection,
};
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as WheelBitDepth;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::processor_ops::{
    Dump, Dumped, ProcessorDump, ProcessorOpsReply, ProcessorOpsRequest,
};
use serde_json::{Value, json};

/// A group: its direction and its children.
#[derive(Debug, Clone)]
struct Group {
    dir: TransformDirection,
    children: Vec<Group>,
}

fn dir_name(dir: TransformDirection) -> &'static str {
    match dir {
        TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
        TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
    }
}

impl Group {
    fn spec(&self) -> Value {
        json!({
            "class": "GroupTransform",
            "calls": [["setDirection", {"enum": dir_name(self.dir)}]],
            "children": self.children.iter().map(Group::spec).collect::<Vec<_>>(),
        })
    }

    fn port(&self) -> Transform {
        let mut group = GroupTransform::new();
        group.set_direction(self.dir);
        for child in &self.children {
            group.append_transform(child.port());
        }
        group.into()
    }
}

fn groups() -> Vec<Group> {
    use TransformDirection::{Forward as F, Inverse as I};
    let leaf = |dir| Group {
        dir,
        children: Vec::new(),
    };
    let node = |dir, children| Group { dir, children };
    vec![
        leaf(F),
        leaf(I),
        node(F, vec![leaf(I)]),
        node(I, vec![leaf(F), leaf(I), leaf(F)]),
        node(
            F,
            vec![node(I, vec![leaf(F), node(F, vec![leaf(I)])]), leaf(F)],
        ),
    ]
}

/// A bit depth, as the oracle names it.
fn wheel_depth(depth: BitDepth) -> WheelBitDepth {
    match depth {
        BitDepth::Uint8 => WheelBitDepth::Uint8,
        BitDepth::Uint10 => WheelBitDepth::Uint10,
        BitDepth::Uint12 => WheelBitDepth::Uint12,
        BitDepth::Uint16 => WheelBitDepth::Uint16,
        BitDepth::F16 => WheelBitDepth::F16,
        BitDepth::F32 => WheelBitDepth::F32,
        other => panic!("no pixel bit depth: {other:?}"),
    }
}

/// What the optimized processor is asked for: bit depths and flags.
fn optimizations() -> Vec<(BitDepth, BitDepth, OptimizationFlags, Value)> {
    vec![
        (
            BitDepth::F32,
            BitDepth::F32,
            OptimizationFlags::DEFAULT,
            json!("OPTIMIZATION_DEFAULT"),
        ),
        (
            BitDepth::Uint8,
            BitDepth::F16,
            OptimizationFlags::NONE,
            json!("OPTIMIZATION_NONE"),
        ),
        (
            BitDepth::Uint16,
            BitDepth::Uint10,
            OptimizationFlags::ALL,
            json!("OPTIMIZATION_ALL"),
        ),
    ]
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("UTF-8 metadata")
}

/// A metadata tree, from the port: element name and value, attributes, children.
fn port_metadata(metadata: &FormatMetadata) -> Value {
    let attributes: Vec<Value> = metadata
        .get_attributes()
        .iter()
        .map(|(name, value)| json!([text(name), text(value)]))
        .collect();
    let children: Vec<Value> = metadata
        .get_children_elements()
        .iter()
        .map(port_metadata)
        .collect();
    json!({"name": text(metadata.get_element_name()), "value": text(metadata.get_element_value()),
        "attributes": attributes, "children": children})
}

/// A metadata tree, from the wheel's dump.
fn wheel_metadata(dump: &Dump) -> Value {
    let string = |d: &Dumped| match d {
        Dumped::Str(s) => s.clone(),
        other => panic!("a string: {other:?}"),
    };
    let list = |name: &str| match dump.getter(name) {
        Dumped::List(items) => items.clone(),
        other => panic!("{name}: {other:?}"),
    };
    let attributes: Vec<Value> = list("getAttributes")
        .iter()
        .map(|pair| match pair {
            Dumped::List(kv) => json!(kv.iter().map(string).collect::<Vec<_>>()),
            other => panic!("an attribute: {other:?}"),
        })
        .collect();
    let children: Vec<Value> = list("getChildElements")
        .iter()
        .map(|child| wheel_metadata(child.object()))
        .collect();
    json!({"name": string(dump.getter("getElementName")),
        "value": string(dump.getter("getElementValue")),
        "attributes": attributes, "children": children})
}

/// A processor, from the port.
fn port_summary(processor: &Processor) -> Value {
    let metadata = processor.processor_metadata();
    let files: Vec<String> = (0..metadata.num_files())
        .map(|i| text(metadata.file(i)))
        .collect();
    let looks: Vec<String> = (0..metadata.num_looks())
        .map(|i| text(metadata.look(i)))
        .collect();
    let group = processor.create_group_transform().expect("a group");
    json!({"cache_id": processor.cache_id().expect("a cache ID"),
        "isNoOp": processor.is_no_op().expect("isNoOp"),
        "hasChannelCrosstalk": processor.has_channel_crosstalk(),
        "isDynamic": processor.is_dynamic(),
        "files": files, "looks": looks,
        "group": {"direction": dir_name(group.direction()),
            "num_transforms": group.num_transforms(),
            "metadata": port_metadata(group.format_metadata())}})
}

/// A processor, from the wheel.
fn wheel_summary(p: &ProcessorDump) -> Value {
    let strings = |name: &str| match p.processor_metadata.getter(name) {
        Dumped::List(items) => items
            .iter()
            .map(|s| match s {
                Dumped::Str(s) => s.clone(),
                other => panic!("{name}: {other:?}"),
            })
            .collect::<Vec<_>>(),
        other => panic!("{name}: {other:?}"),
    };
    // The binding has no getNumTransforms: a group iterates over its transforms.
    let num_transforms = p.group.children.len();
    json!({"cache_id": p.cache_id, "isNoOp": p.is_no_op,
        "hasChannelCrosstalk": p.has_channel_crosstalk, "isDynamic": p.is_dynamic,
        "files": strings("getFiles"), "looks": strings("getLooks"),
        "group": {"direction": p.group.getter("getDirection").name(),
            "num_transforms": num_transforms,
            "metadata": wheel_metadata(p.group.getter("getFormatMetadata").object())}})
}

/// Each group, in each direction, with each optimization: the processor and the optimized
/// processor, as the wheel's.
#[test]
fn group_processors_match_the_wheel() {
    let config = Config::create_raw();
    let mut requests = Vec::new();
    let mut ports = Vec::new();
    for group in groups() {
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            for (in_bd, out_bd, flags, flags_name) in optimizations() {
                let mut request = ProcessorOpsRequest::new(json!({
                    "transform": group.spec(),
                    "direction": dir_name(dir),
                }));
                request.in_bitdepth = Some(wheel_depth(in_bd));
                request.out_bitdepth = Some(wheel_depth(out_bd));
                request.optimization = Some(flags_name);
                requests.push(request);

                let processor = config
                    .processor_in_direction(&group.port(), dir)
                    .expect("a processor");
                let optimized = processor
                    .optimized_processor_with_bit_depths(in_bd, out_bd, flags)
                    .expect("an optimized processor");
                ports.push((
                    format!("{group:?} {dir:?} {in_bd:?} {out_bd:?} {flags:?}"),
                    port_summary(&processor),
                    port_summary(&optimized),
                ));
            }
        }
    }

    let calls: Vec<_> = requests.iter().map(ProcessorOpsRequest::call).collect();
    let responses = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for ((case, port, port_optimized), response) in ports.iter().zip(responses) {
        let reply = ProcessorOpsReply::from_response(response.expect("an oracle reply"));
        assert!(reply.raised().is_none(), "{case}: {:?}", reply.raised());
        assert_eq!(reply.result["log"], json!([]), "{case}");
        let wheel = wheel_summary(reply.processor());
        let wheel_optimized = wheel_summary(reply.optimized());
        if *port != wheel || *port_optimized != wheel_optimized {
            failures.push(format!(
                "{case}\n  wheel {wheel} {wheel_optimized}\n  port  {port} {port_optimized}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A group with format metadata, for `processor_metadata`.
#[derive(Debug, Clone)]
struct MetaGroup {
    dir: TransformDirection,
    name: Option<&'static str>,
    id: Option<&'static str>,
    attributes: Vec<(&'static str, &'static str)>,
    children: Vec<MetaGroup>,
}

impl MetaGroup {
    fn spec(&self) -> Value {
        json!({
            "direction": dir_name(self.dir),
            "name": self.name,
            "id": self.id,
            "attributes": self.attributes.iter().map(|(n, v)| json!([n, v])).collect::<Vec<_>>(),
            "children": self.children.iter().map(MetaGroup::spec).collect::<Vec<_>>(),
        })
    }

    /// The group, its metadata set in the oracle's order: name, ID, attributes.
    fn port(&self) -> Transform {
        let mut group = GroupTransform::new();
        group.set_direction(self.dir);
        let metadata = group.format_metadata_mut();
        if let Some(name) = self.name {
            metadata.set_name(Some(name.as_bytes()));
        }
        if let Some(id) = self.id {
            metadata.set_id(Some(id.as_bytes()));
        }
        for (name, value) in &self.attributes {
            metadata
                .add_attribute(Some(name.as_bytes()), Some(value.as_bytes()))
                .expect("an attribute");
        }
        for child in &self.children {
            group.append_transform(child.port());
        }
        group.into()
    }
}

/// A metadata tree in the command's form.
fn metadata_tree(metadata: &FormatMetadata) -> Value {
    json!({
        "element_name": text(metadata.get_element_name()),
        "element_value": text(metadata.get_element_value()),
        "attributes": metadata
            .get_attributes()
            .iter()
            .map(|(name, value)| json!([text(name), text(value)]))
            .collect::<Vec<_>>(),
        "children": metadata
            .get_children_elements()
            .iter()
            .map(metadata_tree)
            .collect::<Vec<_>>(),
    })
}

/// A `processor_metadata` reply, from the port: `files` and `looks` in a `ProcessorMetadata`,
/// and the processors `config` gives `groups`, one after the other.
fn port_processor_metadata(
    files: &[&str],
    looks: &[&str],
    groups: &[MetaGroup],
    config: &Config,
) -> Value {
    let mut metadata = ocio::ProcessorMetadata::new();
    for file in files {
        metadata.add_file(file.as_bytes());
    }
    for look in looks {
        metadata.add_look(look.as_bytes());
    }
    let port_groups: Vec<Value> = groups
        .iter()
        .map(|group| {
            let processor = config.processor(&group.port()).expect("a processor");
            let created = processor.create_group_transform().expect("a group");
            json!({
                "cache_id": processor.cache_id().expect("a cache ID"),
                "metadata": metadata_tree(processor.format_metadata()),
                "group_metadata": metadata_tree(created.format_metadata()),
                "group_size": created.num_transforms(),
            })
        })
        .collect();
    json!({
        "metadata": {
            "files": (0..metadata.num_files()).map(|i| text(metadata.file(i))).collect::<Vec<_>>(),
            "looks": (0..metadata.num_looks()).map(|i| text(metadata.look(i))).collect::<Vec<_>>(),
        },
        "groups": port_groups,
    })
}

/// A `ProcessorMetadata`'s files (a set, in byte order) and looks (a list), from C strings;
/// and the format metadata of groups' processors and of their `createGroupTransform()`.
/// `BuildGroupOps` copies a group's metadata into the ops while they are still empty, so with
/// groups of groups, which add no ops, the processor holds the metadata of the last group
/// visited (transforms/GroupTransform.cpp:178-183 @ v2.5.2): "inner" over "outer", and in an
/// inverse group, whose children are visited last to first, "first" over "outer" and the
/// second child's. `createGroupTransform` copies the processor's (Processor.cpp:305 @ v2.5.2).
/// Against the wheel (`processor_metadata`).
///
/// Each group's processor comes from a config of its own, in the wheel (a request per group)
/// and in the port. A config's processor cache returns the processor it already holds with the
/// same cache ID (`Config::Impl::getProcessor`), and every processor here has the cache ID
/// `<NOOP>`, so with one config every group would get the first group's processor. The last
/// request pins that fallback on purpose: every group with one config, each getting the first
/// group's processor, which has no metadata.
#[test]
fn processor_metadata_matches_the_wheel() {
    use TransformDirection::{Forward as F, Inverse as I};
    let files = ["b", "a", "b", "c\u{0}d", "", "\u{e9}", "B", "a/b.clf"];
    let looks = ["y", "x", "y\u{0}z", "", "y"];
    let leaf = |dir, name, id, attributes| MetaGroup {
        dir,
        name,
        id,
        attributes,
        children: Vec::new(),
    };
    let groups = [
        leaf(F, None, None, Vec::new()),
        leaf(
            I,
            Some("one"),
            Some("UID42"),
            vec![("k", "v"), ("name", "two")],
        ),
        MetaGroup {
            dir: F,
            name: Some("outer"),
            id: Some("UID42"),
            attributes: vec![("k", "v")],
            children: vec![leaf(I, Some("inner"), None, vec![("a", "1")])],
        },
        MetaGroup {
            dir: I,
            name: Some("outer"),
            id: None,
            attributes: Vec::new(),
            children: vec![
                leaf(F, Some("first"), None, Vec::new()),
                leaf(F, None, Some("second"), Vec::new()),
            ],
        },
    ];

    // A request per group, each with a config of its own; then every group with one config.
    let mut cases: Vec<(String, Value, Value)> = groups
        .iter()
        .map(|group| {
            let args = json!({"files": files, "looks": looks, "groups": [group.spec()]});
            let port = port_processor_metadata(
                &files,
                &looks,
                std::slice::from_ref(group),
                &Config::create_raw(),
            );
            (format!("{group:?}"), args, port)
        })
        .collect();
    cases.push((
        "every group, one config".to_string(),
        json!({"files": files, "looks": looks,
            "groups": groups.iter().map(MetaGroup::spec).collect::<Vec<_>>()}),
        port_processor_metadata(&files, &looks, &groups, &Config::create_raw()),
    ));

    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(_, args, _)| BatchCall {
            cmd: "processor_metadata",
            args: args.clone(),
            blobs: Vec::new(),
        })
        .collect();
    let responses = Oracle::get().batch(&calls, true);
    let mut failures = Vec::new();
    for ((case, _, port), response) in cases.iter().zip(responses) {
        let wheel = response.unwrap_or_else(|e| panic!("{case}: {e}")).result;
        if *port != wheel {
            failures.push(format!("{case}\n  wheel {wheel}\n  port  {port}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The optimized processor's output bit depth: a matrix, then a range that only clamps to
/// [0, 1], optimized from F32 to UINT16 and to F32 with the default flags. The trailing clamp
/// goes only when the output is an integer depth (`OpRcPtrVec::optimizeForBitdepth`,
/// OpOptimizers.cpp:768-771 @ v2.5.2, through `Processor::Impl::getOptimizedProcessor`,
/// Processor.cpp:382-433). The wheel's processors and the port's compare getter for getter
/// (`check_optimized_processors`); and the wheel's two optimized processors must differ, or
/// the case would not tell the output depth from the input's.
#[test]
fn optimized_processor_output_depth_matches_the_wheel() {
    let m44 = [
        2.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.5, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let mut matrix = MatrixTransform::new();
    matrix.set_matrix(&m44);
    let mut range = RangeTransform::new();
    range.set_min_in_value(0.0);
    range.set_max_in_value(1.0);
    range.set_min_out_value(0.0);
    range.set_max_out_value(1.0);
    let case = transforms::group(
        "a matrix, then an identity range",
        TransformDirection::Forward,
        &[
            Case::new(
                "a matrix",
                json!({"class": "MatrixTransform", "calls": [["setMatrix", m44]]}),
                matrix,
            ),
            Case::new(
                "an identity range",
                json!({"class": "RangeTransform", "calls": [
                    ["setMinInValue", 0.0], ["setMaxInValue", 1.0],
                    ["setMinOutValue", 0.0], ["setMaxOutValue", 1.0]]}),
                range,
            ),
        ],
    );
    let depths = [
        (WheelBitDepth::F32, WheelBitDepth::Uint16),
        (WheelBitDepth::F32, WheelBitDepth::F32),
    ];
    transforms::check_optimized_processors(std::slice::from_ref(&case), &depths);

    let requests: Vec<ProcessorOpsRequest> = depths
        .iter()
        .map(|&(input, output)| {
            let mut request = ProcessorOpsRequest::new(
                json!({"transform": case.spec, "direction": dir_name(TransformDirection::Forward)}),
            );
            request.in_bitdepth = Some(input);
            request.out_bitdepth = Some(output);
            request.optimization = Some(json!("OPTIMIZATION_DEFAULT"));
            request
        })
        .collect();
    let calls: Vec<BatchCall<'_>> = requests.iter().map(ProcessorOpsRequest::call).collect();
    let classes: Vec<Vec<String>> = Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .map(|response| {
            let reply = ProcessorOpsReply::from_response(response.expect("an oracle reply"));
            let optimized = reply.optimized().classes();
            optimized.iter().map(|c| c.to_string()).collect()
        })
        .collect();
    assert_ne!(classes[0], classes[1], "{classes:?}");
}
