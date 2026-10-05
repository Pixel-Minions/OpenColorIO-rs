// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of `YAML::NodeBuilder` (src/nodebuilder.h, src/nodebuilder.cpp, yaml-cpp 0.8.0): the
//! event handler that builds a document's nodes. A collection's node is created at its start
//! event, so an alias inside it can refer to it; an alias is the anchored node itself.

use std::sync::Arc;

use super::event_handler::{Anchor, EmitterStyle, EventHandler};
use super::mark::Mark;
use super::node::{Memory, Node, NodeData, NodeType};

/// Port of `YAML::NodeBuilder` (nodebuilder.h:22-70).
#[derive(Debug)]
pub struct NodeBuilder {
    /// `m_pMemory`: the document's nodes.
    nodes: Vec<NodeData>,
    root: Option<usize>,
    stack: Vec<usize>,
    /// `m_anchors`: the node of each anchor number (0 is no anchor).
    anchors: Vec<Option<usize>>,
    /// `m_keys`: the key of each map being filled, and whether its value is next.
    keys: Vec<(usize, bool)>,
    map_depth: usize,
}

impl Default for NodeBuilder {
    fn default() -> Self {
        NodeBuilder::new()
    }
}

impl NodeBuilder {
    /// `NodeBuilder()` (nodebuilder.cpp:12-20).
    pub fn new() -> NodeBuilder {
        NodeBuilder {
            nodes: Vec::new(),
            root: None,
            stack: Vec::new(),
            // since the anchors start at 1
            anchors: vec![None],
            keys: Vec::new(),
            map_depth: 0,
        }
    }

    /// `Root()` (nodebuilder.cpp:24-29): the document's root node, or `Node()` if there was
    /// none.
    pub fn root(self) -> Node {
        match self.root {
            None => Node::new(),
            Some(root) => Node::from_index(&Arc::new(Memory { nodes: self.nodes }), root),
        }
    }

    /// `Push(const Mark&, anchor_t)` (nodebuilder.cpp:80-86): a new node (`create_node`, a
    /// null node with the null mark), at `mark`.
    fn push_new(&mut self, mark: Mark, anchor: Anchor) -> usize {
        self.nodes.push(NodeData {
            mark,
            ..NodeData::default()
        });
        let node = self.nodes.len() - 1;
        self.register_anchor(anchor, node);
        self.push(node);
        node
    }

    /// `Push(detail::node&)` (nodebuilder.cpp:88-96): a node that is a map's key gets an entry
    /// in `m_keys`.
    fn push(&mut self, node: usize) {
        let needs_key = self
            .stack
            .last()
            .is_some_and(|&top| self.nodes[top].ty == NodeType::Map)
            && self.keys.len() < self.map_depth;

        self.stack.push(node);
        if needs_key {
            self.keys.push((node, false));
        }
    }

    /// `Pop()` (nodebuilder.cpp:98-126): the finished node goes into its collection: appended
    /// to a sequence, or, in a map, kept as the key until its value comes.
    fn pop(&mut self) {
        let Some(node) = self.stack.pop() else {
            return;
        };
        let Some(&collection) = self.stack.last() else {
            self.root = Some(node);
            return;
        };

        match self.nodes[collection].ty {
            NodeType::Sequence => self.nodes[collection].sequence.push(node),
            NodeType::Map => {
                if let Some(key) = self.keys.last_mut() {
                    if key.1 {
                        let key_node = key.0;
                        self.nodes[collection].map.push((key_node, node));
                        self.keys.pop();
                    } else {
                        key.1 = true;
                    }
                }
            }
            // yaml-cpp asserts; a collection on the stack is a sequence or a map.
            _ => self.stack.clear(),
        }
    }

    /// `RegisterAnchor(anchor_t, detail::node&)` (nodebuilder.cpp:128-133). The parser numbers
    /// anchors in the order of their nodes' events, so the anchor is the next number.
    fn register_anchor(&mut self, anchor: Anchor, node: usize) {
        if anchor != 0 {
            self.anchors.push(Some(node));
        }
    }
}

impl EventHandler for NodeBuilder {
    /// `OnDocumentStart` (nodebuilder.cpp:31).
    fn on_document_start(&mut self, _mark: Mark) {}

    /// `OnDocumentEnd` (nodebuilder.cpp:33).
    fn on_document_end(&mut self) {}

    /// `OnNull` (nodebuilder.cpp:35-39).
    fn on_null(&mut self, mark: Mark, anchor: Anchor) {
        let node = self.push_new(mark, anchor);
        self.nodes[node].ty = NodeType::Null;
        self.pop();
    }

    /// `OnAlias` (nodebuilder.cpp:41-45): the anchored node itself, again. The parser only
    /// reports anchors it has numbered, whose nodes exist.
    fn on_alias(&mut self, _mark: Mark, anchor: Anchor) {
        let Some(&Some(node)) = self.anchors.get(anchor) else {
            return;
        };
        self.push(node);
        self.pop();
    }

    /// `OnScalar` (nodebuilder.cpp:47-53).
    fn on_scalar(&mut self, mark: Mark, tag: &[u8], anchor: Anchor, value: &[u8]) {
        let node = self.push_new(mark, anchor);
        let data = &mut self.nodes[node];
        data.ty = NodeType::Scalar;
        data.scalar = value.to_vec();
        data.tag = tag.to_vec();
        self.pop();
    }

    /// `OnSequenceStart` (nodebuilder.cpp:55-61).
    fn on_sequence_start(&mut self, mark: Mark, tag: &[u8], anchor: Anchor, style: EmitterStyle) {
        let node = self.push_new(mark, anchor);
        let data = &mut self.nodes[node];
        data.tag = tag.to_vec();
        data.ty = NodeType::Sequence;
        data.style = style;
    }

    /// `OnSequenceEnd` (nodebuilder.cpp:63).
    fn on_sequence_end(&mut self) {
        self.pop();
    }

    /// `OnMapStart` (nodebuilder.cpp:65-72).
    fn on_map_start(&mut self, mark: Mark, tag: &[u8], anchor: Anchor, style: EmitterStyle) {
        let node = self.push_new(mark, anchor);
        let data = &mut self.nodes[node];
        data.ty = NodeType::Map;
        data.tag = tag.to_vec();
        data.style = style;
        self.map_depth += 1;
    }

    /// `OnMapEnd` (nodebuilder.cpp:74-78).
    fn on_map_end(&mut self) {
        self.map_depth -= 1;
        self.pop();
    }
}
