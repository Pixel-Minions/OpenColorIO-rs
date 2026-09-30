// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! A test-only YAML reader: `saphyr-parser` events into a tree that keeps what the emitter
//! needs to be driven the way OCIO drives it: each collection's requested style (flow or
//! block), verbatim tags, and each scalar's text and style.
//!
//! `saphyr-parser` reports no collection style, but a flow collection's span starts at its
//! `[` or `{` and a block collection's at its first entry. An empty collection is always
//! written `[]`/`{}`; yaml-cpp puts the bracket on a line of its own only when block style
//! was requested (`EmitEndSeq`/`EmitEndMap` indent to the group, while a flow group opens on
//! the key's line), which is how the requested style is recovered.

use saphyr_parser::{Event, Parser, ScalarStyle, Tag};

/// The style the writer asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Style {
    Block,
    Flow,
}

/// A YAML node.
#[derive(Debug, Clone)]
pub(crate) enum Node {
    Scalar {
        value: String,
        style: ScalarStyle,
        tag: Option<String>,
    },
    Seq {
        tag: Option<String>,
        style: Style,
        items: Vec<Node>,
    },
    Map {
        tag: Option<String>,
        style: Style,
        entries: Vec<(String, Node)>,
    },
}

impl Node {
    /// The verbatim tag (`!<Name>` gives `Name`).
    pub(crate) fn tag(&self) -> Option<&str> {
        match self {
            Node::Scalar { tag, .. } | Node::Seq { tag, .. } | Node::Map { tag, .. } => {
                tag.as_deref()
            }
        }
    }

    /// A scalar's text; panics on a collection.
    pub(crate) fn text(&self) -> &str {
        match self {
            Node::Scalar { value, .. } => value,
            other => panic!("expected a scalar, found {other:?}"),
        }
    }

    /// A map's entries; panics otherwise.
    pub(crate) fn entries(&self) -> &[(String, Node)] {
        match self {
            Node::Map { entries, .. } => entries,
            other => panic!("expected a map, found {other:?}"),
        }
    }

    /// A sequence's items; panics otherwise.
    pub(crate) fn items(&self) -> &[Node] {
        match self {
            Node::Seq { items, .. } => items,
            other => panic!("expected a sequence, found {other:?}"),
        }
    }

    /// A collection's requested style.
    pub(crate) fn style(&self) -> Style {
        match self {
            Node::Seq { style, .. } | Node::Map { style, .. } => *style,
            Node::Scalar { .. } => panic!("a scalar has no collection style"),
        }
    }

    /// The value of `key` in a map.
    pub(crate) fn get(&self, key: &str) -> Option<&Node> {
        self.entries()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }
}

fn tag_name(tag: Option<std::borrow::Cow<'_, Tag>>) -> Option<String> {
    tag.map(|t| {
        assert!(
            t.handle.is_empty(),
            "only verbatim tags are expected, found {t:?}"
        );
        t.suffix.clone()
    })
}

/// A collection being read.
enum Open {
    Seq {
        tag: Option<String>,
        style: Style,
        items: Vec<Node>,
    },
    Map {
        tag: Option<String>,
        style: Style,
        entries: Vec<(String, Node)>,
        key: Option<String>,
    },
}

/// Parses one YAML document into a tree.
pub(crate) fn parse(text: &str) -> Node {
    let chars: Vec<char> = text.chars().collect();
    // The style a collection starting at `index` was requested with.
    let style_at = |index: usize, empty_probe: bool| -> Style {
        let c = chars.get(index).copied();
        if c != Some('[') && c != Some('{') {
            return Style::Block;
        }
        if !empty_probe {
            return Style::Flow;
        }
        // An empty collection: block style if its bracket starts its line.
        let line_start = chars[..index]
            .iter()
            .rposition(|&c| c == '\n')
            .map_or(0, |i| i + 1);
        if chars[line_start..index].iter().all(|&c| c == ' ') {
            Style::Block
        } else {
            Style::Flow
        }
    };
    let is_empty_at = |index: usize| {
        let mut i = index + 1;
        while chars.get(i) == Some(&' ') {
            i += 1;
        }
        matches!(chars.get(i), Some(']') | Some('}'))
    };

    let mut stack: Vec<Open> = Vec::new();
    let mut root = None;
    let push = |stack: &mut Vec<Open>, node: Node, root: &mut Option<Node>| match stack.last_mut() {
        None => *root = Some(node),
        Some(Open::Seq { items, .. }) => items.push(node),
        Some(Open::Map { entries, key, .. }) => match key.take() {
            None => match node {
                Node::Scalar { value, .. } => *key = Some(value),
                other => panic!("non-scalar key {other:?}"),
            },
            Some(k) => entries.push((k, node)),
        },
    };

    for event in Parser::new_from_str(text) {
        let (event, span) = event.unwrap_or_else(|e| panic!("YAML error: {e}"));
        let index = span.start.index();
        match event {
            Event::Scalar(value, style, _, tag) => {
                let node = Node::Scalar {
                    value: value.into_owned(),
                    style,
                    tag: tag_name(tag),
                };
                push(&mut stack, node, &mut root);
            }
            Event::SequenceStart(_, tag) => stack.push(Open::Seq {
                tag: tag_name(tag),
                style: style_at(index, is_empty_at(index)),
                items: Vec::new(),
            }),
            Event::MappingStart(_, tag) => stack.push(Open::Map {
                tag: tag_name(tag),
                style: style_at(index, is_empty_at(index)),
                entries: Vec::new(),
                key: None,
            }),
            Event::SequenceEnd => match stack.pop() {
                Some(Open::Seq { tag, style, items }) => {
                    push(&mut stack, Node::Seq { tag, style, items }, &mut root);
                }
                _ => panic!("unbalanced sequence"),
            },
            Event::MappingEnd => match stack.pop() {
                Some(Open::Map {
                    tag,
                    style,
                    entries,
                    key: None,
                }) => {
                    push(
                        &mut stack,
                        Node::Map {
                            tag,
                            style,
                            entries,
                        },
                        &mut root,
                    );
                }
                _ => panic!("unbalanced mapping"),
            },
            Event::Alias(_) => panic!("aliases are not expected"),
            Event::StreamStart
            | Event::StreamEnd
            | Event::DocumentStart(_)
            | Event::DocumentEnd
            | Event::Nothing => {}
        }
    }
    root.expect("a document")
}
