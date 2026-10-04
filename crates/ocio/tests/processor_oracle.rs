// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The processor against the wheel, through the oracle's `processor_ops`: `Config::getProcessor`
//! and `getOptimizedProcessor` for groups of groups, nested, in both directions, and the
//! processors' cache ID, flags, metadata and `createGroupTransform()`.
//!
//! So far the group transform, the one class in the port; its processors have no ops. Each
//! class adds its cases as it lands.

use ocio::{
    BitDepth, Config, FormatMetadata, GroupTransform, OptimizationFlags, Processor, Transform,
    TransformDirection,
};
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as WheelBitDepth;
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

/// A `ProcessorMetadata`'s files (a set, in byte order) and looks (a list), from C strings;
/// and the format metadata of groups' processors and of their `createGroupTransform()`: the
/// metadata of the groups built while the ops are still empty, the last of them winning
/// (`BuildGroupOps`, transforms/GroupTransform.cpp:178-183 @ v2.5.2). Against the wheel
/// (`processor_metadata`).
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

    let response = Oracle::get().call(
        "processor_metadata",
        json!({"files": files, "looks": looks,
            "groups": groups.iter().map(MetaGroup::spec).collect::<Vec<_>>()}),
        &[],
    );

    let mut metadata = ocio::ProcessorMetadata::new();
    for file in files {
        metadata.add_file(file.as_bytes());
    }
    for look in looks {
        metadata.add_look(look.as_bytes());
    }
    let config = Config::create_raw();
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
    let port = json!({
        "metadata": {
            "files": (0..metadata.num_files()).map(|i| text(metadata.file(i))).collect::<Vec<_>>(),
            "looks": (0..metadata.num_looks()).map(|i| text(metadata.look(i))).collect::<Vec<_>>(),
        },
        "groups": port_groups,
    });
    assert_eq!(port, response.result);
}
