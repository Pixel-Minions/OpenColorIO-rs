// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0's node API for reading a loaded document: `YAML::Node`
//! (include/yaml-cpp/node/node.h, node/impl.h), its data (node/detail/node_data.h,
//! src/node_data.cpp), the memory that owns a document's nodes (node/detail/memory.h,
//! src/memory.cpp), the iterators (node/iterator.h, node/detail/iterator.h,
//! node/detail/node_iterator.h) and `NodeType` (node/type.h).
//!
//! OCIO only reads the nodes `YAML::Load` builds, through `const` nodes: their type, tag,
//! mark and scalar, their size, iteration, `operator[]` and `as<T>()`. So the port keeps a
//! document's nodes in an arena (`memory_holder`), and a [`Node`] is a handle: the arena and
//! an index. An alias is the same index as its anchor, as in yaml-cpp, where an alias is the
//! same `detail::node`. yaml-cpp's mutating API (assignment, `push_back`, `force_insert`,
//! `remove`, the non-`const` `operator[]` that inserts) is not ported. Every node of a loaded
//! document is defined, so the bookkeeping of undefined nodes (`m_undefinedPairs`,
//! dependencies) has nothing to do and is left out.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::convert::Convert;
use super::event_handler::EmitterStyle;
use super::exceptions::{Exception, KeyText, Result};
use super::mark::Mark;

/// `NodeType::value` (node/type.h:13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    Undefined,
    Null,
    Scalar,
    Sequence,
    Map,
}

/// Port of `detail::node_data` (node/detail/node_data.h:29-125) for a loaded node.
#[derive(Debug, Clone)]
pub(super) struct NodeData {
    pub(super) mark: Mark,
    pub(super) ty: NodeType,
    pub(super) tag: Vec<u8>,
    pub(super) style: EmitterStyle,
    pub(super) scalar: Vec<u8>,
    /// `m_sequence`: indices into the document's nodes.
    pub(super) sequence: Vec<usize>,
    /// `m_map`: (key, value) indices, in insertion order; a key can repeat.
    pub(super) map: Vec<(usize, usize)>,
}

impl Default for NodeData {
    /// `node_data()` (node_data.cpp:23-33): a null node with the null mark.
    fn default() -> Self {
        NodeData {
            mark: Mark::null_mark(),
            ty: NodeType::Null,
            tag: Vec::new(),
            style: EmitterStyle::Default,
            scalar: Vec::new(),
            sequence: Vec::new(),
            map: Vec::new(),
        }
    }
}

/// Port of `detail::memory_holder` (node/detail/memory.h): the nodes of one document.
#[derive(Debug, Default)]
pub struct Memory {
    pub(super) nodes: Vec<NodeData>,
}

/// A key for `operator[]` (node/impl.h:323-372): the key types OCIO and yaml-cpp's tests use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key<'k> {
    /// A `std::string` or C-string key, compared with each map key's `as<std::string>`.
    Str(&'k [u8]),
    /// An unsigned index (`std::size_t`): a sequence's element, or a map key that decodes to
    /// the same number.
    Index(u64),
    /// A signed index (`int`): as `Index` when not negative.
    Int(i64),
    /// `YAML::Null`: the map key that is null.
    Null,
    /// A `std::vector<std::string>`: the map key that is a sequence of these strings.
    StrSeq(&'k [&'k [u8]]),
    /// A `std::map<std::string, std::string>`: the map key that is a map of these pairs.
    StrMap(&'k [(&'k [u8], &'k [u8])]),
}

impl<'k> From<&'k str> for Key<'k> {
    fn from(key: &'k str) -> Key<'k> {
        Key::Str(key.as_bytes())
    }
}

impl<'k> From<&'k [u8]> for Key<'k> {
    fn from(key: &'k [u8]) -> Key<'k> {
        Key::Str(key)
    }
}

impl<'k, const N: usize> From<&'k [u8; N]> for Key<'k> {
    fn from(key: &'k [u8; N]) -> Key<'k> {
        Key::Str(key)
    }
}

impl From<usize> for Key<'_> {
    fn from(key: usize) -> Self {
        Key::Index(key as u64)
    }
}

impl From<i32> for Key<'_> {
    fn from(key: i32) -> Self {
        Key::Int(i64::from(key))
    }
}

impl Key<'_> {
    /// `key_to_string` (node/impl.h:318-321): the key as a stream writes it, or empty for a
    /// key that isn't streamable (`_Null`).
    fn to_text(self) -> Vec<u8> {
        match self {
            Key::Str(s) => s.to_vec(),
            Key::Index(i) => i.to_string().into_bytes(),
            Key::Int(i) => i.to_string().into_bytes(),
            Key::Null | Key::StrSeq(_) | Key::StrMap(_) => Vec::new(),
        }
    }

    /// The key in `ErrorMsg::BAD_SUBSCRIPT_WITH_KEY` (exceptions.h:123-149): strings and
    /// numbers are printed.
    fn subscript_text(self) -> KeyText {
        match self {
            Key::Str(_) | Key::Index(_) | Key::Int(_) => KeyText::Text(self.to_text()),
            Key::Null | Key::StrSeq(_) | Key::StrMap(_) => KeyText::Unprintable,
        }
    }
}

/// Port of `YAML::Node` (node/node.h:21-136), for reading.
#[derive(Debug, Clone)]
pub struct Node {
    is_valid: bool,
    invalid_key: Vec<u8>,
    /// `m_pMemory` and `m_pNode`: the document's nodes and this one's index.
    node: Option<(Arc<Memory>, usize)>,
}

impl Default for Node {
    fn default() -> Self {
        Node::new()
    }
}

/// `operator==(const Node&, const Node&)` (node/impl.h:382): `is()`, which throws for a
/// zombie; here a zombie equals nothing.
impl PartialEq for Node {
    fn eq(&self, other: &Node) -> bool {
        self.is(other).unwrap_or(false)
    }
}

impl Node {
    /// `Node()` (node/impl.h:19-20): a valid node with no data, which reads as null.
    pub fn new() -> Node {
        Node {
            is_valid: true,
            invalid_key: Vec::new(),
            node: None,
        }
    }

    /// `Node(Zombie, key)` (node/impl.h:47-51): what `operator[]` gives for a missing key.
    pub(super) fn zombie(key: Vec<u8>) -> Node {
        Node {
            is_valid: false,
            invalid_key: key,
            node: None,
        }
    }

    /// `Node(detail::node&, shared_memory_holder)` (node/impl.h:53-54).
    pub(super) fn from_index(memory: &Arc<Memory>, index: usize) -> Node {
        Node {
            is_valid: true,
            invalid_key: Vec::new(),
            node: Some((Arc::clone(memory), index)),
        }
    }

    fn data(&self) -> Option<&NodeData> {
        self.node
            .as_ref()
            .map(|(memory, index)| &memory.nodes[*index])
    }

    /// The `InvalidNode` a zombie throws.
    fn invalid(&self) -> Exception {
        Exception::invalid_node(&self.invalid_key)
    }

    /// `IsDefined()` (node/impl.h:68-73): a zombie isn't; every loaded node is.
    pub fn is_defined(&self) -> bool {
        self.is_valid
    }

    /// `Mark()` (node/impl.h:75-80): where the node starts, or the null mark for `Node()`.
    pub fn mark(&self) -> Result<Mark> {
        if !self.is_valid {
            return Err(self.invalid());
        }
        Ok(self.data().map_or(Mark::null_mark(), |d| d.mark))
    }

    /// `Type()` (node/impl.h:82-86).
    pub fn node_type(&self) -> Result<NodeType> {
        if !self.is_valid {
            return Err(self.invalid());
        }
        Ok(self.data().map_or(NodeType::Null, |d| d.ty))
    }

    /// `IsNull()` (node/node.h:52).
    pub fn is_null(&self) -> Result<bool> {
        Ok(self.node_type()? == NodeType::Null)
    }

    /// `IsScalar()` (node/node.h:53).
    pub fn is_scalar(&self) -> Result<bool> {
        Ok(self.node_type()? == NodeType::Scalar)
    }

    /// `IsSequence()` (node/node.h:54).
    pub fn is_sequence(&self) -> Result<bool> {
        Ok(self.node_type()? == NodeType::Sequence)
    }

    /// `IsMap()` (node/node.h:55).
    pub fn is_map(&self) -> Result<bool> {
        Ok(self.node_type()? == NodeType::Map)
    }

    /// `Scalar()` (node/impl.h:166-170): the scalar's text, empty for other nodes.
    pub fn scalar(&self) -> Result<&[u8]> {
        if !self.is_valid {
            return Err(self.invalid());
        }
        Ok(self.data().map_or(&[][..], |d| d.scalar.as_slice()))
    }

    /// `Tag()` (node/impl.h:172-176): the resolved tag: `?` for an untagged plain node, `!`
    /// for an untagged quoted scalar, the URI of a verbatim tag (`!<ColorSpace>` gives
    /// `ColorSpace`).
    pub fn tag(&self) -> Result<&[u8]> {
        if !self.is_valid {
            return Err(self.invalid());
        }
        Ok(self.data().map_or(&[][..], |d| d.tag.as_slice()))
    }

    /// `Style()` (node/impl.h:183-187).
    pub fn style(&self) -> Result<EmitterStyle> {
        if !self.is_valid {
            return Err(self.invalid());
        }
        Ok(self.data().map_or(EmitterStyle::Default, |d| d.style))
    }

    /// `is(const Node&)` (node/impl.h:195-201): whether both are the same node. A zombie on
    /// either side throws, with this node's key.
    pub fn is(&self, rhs: &Node) -> Result<bool> {
        if !self.is_valid || !rhs.is_valid {
            return Err(self.invalid());
        }
        Ok(match (&self.node, &rhs.node) {
            (Some((a, i)), Some((b, j))) => Arc::ptr_eq(a, b) && i == j,
            _ => false,
        })
    }

    /// `reset(const Node&)` (node/impl.h:216-221): makes this handle refer to `rhs`'s node.
    /// A zombie on either side throws, with this node's key.
    pub fn reset(&mut self, rhs: &Node) -> Result<()> {
        if !self.is_valid || !rhs.is_valid {
            return Err(self.invalid());
        }
        self.node = rhs.node.clone();
        Ok(())
    }

    /// `size()` (node/impl.h:271-275, node_data.cpp:90-105): the number of elements or pairs.
    pub fn size(&self) -> Result<usize> {
        if !self.is_valid {
            return Err(self.invalid());
        }
        Ok(self.data().map_or(0, |d| match d.ty {
            NodeType::Sequence => d.sequence.len(),
            NodeType::Map => d.map.len(),
            _ => 0,
        }))
    }

    /// `begin()`/`end()` (node/impl.h:277-300) as an iterator: a sequence's elements, or a
    /// map's pairs, in order. Other nodes, `Node()` and zombies have none.
    pub fn iter(&self) -> NodeIter {
        let (memory, kind) = match (&self.node, self.is_valid) {
            (Some((memory, index)), true) => {
                let data = &memory.nodes[*index];
                let kind = match data.ty {
                    NodeType::Sequence => IterKind::Sequence(data.sequence.clone()),
                    NodeType::Map => IterKind::Map(data.map.clone()),
                    _ => IterKind::None,
                };
                (Some(Arc::clone(memory)), kind)
            }
            _ => (None, IterKind::None),
        };
        NodeIter {
            memory,
            kind,
            next: 0,
        }
    }

    /// The `const` `operator[](const Key&)` (node/impl.h:324-333, node/detail/impl.h:116-139):
    /// a map's value for the key (the first pair whose key equals it), a sequence's element
    /// at an index, or a zombie that remembers the key. A scalar throws `BadSubscript`; a
    /// zombie throws `InvalidNode`.
    pub fn get<'k>(&self, key: impl Into<Key<'k>>) -> Result<Node> {
        let key = key.into();
        // EnsureNodeExists: a zombie throws; `Node()` reads as a null node.
        if !self.is_valid {
            return Err(self.invalid());
        }
        let found = match &self.node {
            None => None,
            Some((memory, index)) => {
                let data = &memory.nodes[*index];
                match data.ty {
                    NodeType::Map => self.find_in_map(memory, data, key)?,
                    NodeType::Undefined | NodeType::Null => None,
                    NodeType::Sequence => get_idx(&data.sequence, key),
                    NodeType::Scalar => {
                        return Err(Exception::bad_subscript(data.mark, &key.subscript_text()));
                    }
                }
            }
        };
        Ok(match (found, &self.node) {
            (Some(value), Some((memory, _))) => Node::from_index(memory, value),
            _ => Node::zombie(key.to_text()),
        })
    }

    /// The map lookup of `node_data::get<Key>` (node/detail/impl.h:134-138): the first pair
    /// whose key `equals` the key (node/detail/impl.h:99-114), by its conversion to the key's
    /// type.
    fn find_in_map(
        &self,
        memory: &Arc<Memory>,
        data: &NodeData,
        key: Key<'_>,
    ) -> Result<Option<usize>> {
        for &(k, v) in &data.map {
            let key_node = Node::from_index(memory, k);
            let equal = match key {
                Key::Str(s) => Vec::<u8>::decode(&key_node)?.is_some_and(|lhs| lhs == s),
                Key::Index(i) => u64::decode(&key_node)?.is_some_and(|lhs| lhs == i),
                Key::Int(i) => i32::decode(&key_node)?.is_some_and(|lhs| i64::from(lhs) == i),
                Key::Null => key_node.is_null()?,
                Key::StrSeq(seq) => Vec::<Vec<u8>>::decode(&key_node)?
                    .is_some_and(|lhs| lhs.iter().map(Vec::as_slice).eq(seq.iter().copied())),
                Key::StrMap(pairs) => decode_string_map(&key_node)?.is_some_and(|lhs| {
                    lhs == pairs
                        .iter()
                        .map(|&(k, v)| (k.to_vec(), v.to_vec()))
                        .collect::<BTreeMap<_, _>>()
                }),
            };
            if equal {
                return Ok(Some(v));
            }
        }
        Ok(None)
    }

    /// `as<T>()` (node/impl.h:152-157, the `as_if<T, void>` helpers at 121-149): the node
    /// converted, or `TypedBadConversion<T>` at the node's mark. A zombie throws
    /// `InvalidNode`. `as<std::string>()` of a null node is `"null"`.
    pub fn as_<T: Convert>(&self) -> Result<T> {
        if !self.is_valid {
            return Err(self.invalid());
        }
        T::as_if(self)
    }

    /// `as<T>(fallback)` (node/impl.h:159-164, the `as_if<T, S>` helpers at 91-119): the
    /// node converted, or the fallback.
    pub fn as_or<T: Convert>(&self, fallback: T) -> Result<T> {
        if !self.is_valid {
            return Ok(fallback);
        }
        T::as_if_or(self, fallback)
    }

    /// Whether the node has data (`m_pNode` is set): `Node()` and zombies don't.
    pub(super) fn has_data(&self) -> bool {
        self.node.is_some()
    }
}

/// `convert<std::map<std::string, std::string>>::decode` (convert.h:239-258): a map, each
/// key and value through `as<std::string>()`; a key that repeats keeps its last value.
fn decode_string_map(node: &Node) -> Result<Option<BTreeMap<Vec<u8>, Vec<u8>>>> {
    if !node.is_map()? {
        return Ok(None);
    }
    let mut rhs = BTreeMap::new();
    for element in node.iter() {
        rhs.insert(
            element.first.as_::<Vec<u8>>()?,
            element.second.as_::<Vec<u8>>()?,
        );
    }
    Ok(Some(rhs))
}

/// `get_idx<Key>::get` (node/detail/impl.h:18-59), the `const` versions: an element of the
/// sequence for an index in range; other keys find nothing.
fn get_idx(sequence: &[usize], key: Key<'_>) -> Option<usize> {
    let index = match key {
        Key::Index(i) => i,
        Key::Int(i) => u64::try_from(i).ok()?,
        Key::Str(_) | Key::Null | Key::StrSeq(_) | Key::StrMap(_) => return None,
    };
    usize::try_from(index)
        .ok()
        .and_then(|i| sequence.get(i).copied())
}

#[derive(Debug, Clone)]
enum IterKind {
    None,
    Sequence(Vec<usize>),
    Map(Vec<(usize, usize)>),
}

/// `const_iterator` (node/detail/iterator.h, node/detail/node_iterator.h).
#[derive(Debug, Clone)]
pub struct NodeIter {
    memory: Option<Arc<Memory>>,
    kind: IterKind,
    next: usize,
}

/// `detail::iterator_value` (node/iterator.h:19-29): a sequence element is the value itself,
/// with zombie `first` and `second`; a map pair is `first` and `second`, and the value
/// itself is a zombie.
#[derive(Debug, Clone)]
pub struct IteratorValue {
    pub node: Node,
    pub first: Node,
    pub second: Node,
}

impl Iterator for NodeIter {
    type Item = IteratorValue;

    fn next(&mut self) -> Option<IteratorValue> {
        let memory = self.memory.as_ref()?;
        let item = match &self.kind {
            IterKind::None => return None,
            IterKind::Sequence(items) => IteratorValue {
                node: Node::from_index(memory, *items.get(self.next)?),
                first: Node::zombie(Vec::new()),
                second: Node::zombie(Vec::new()),
            },
            IterKind::Map(pairs) => {
                let &(k, v) = pairs.get(self.next)?;
                IteratorValue {
                    node: Node::zombie(Vec::new()),
                    first: Node::from_index(memory, k),
                    second: Node::from_index(memory, v),
                }
            }
        };
        self.next += 1;
        Some(item)
    }
}

#[cfg(test)]
#[path = "node_spec_tests.rs"]
mod node_spec_tests;

#[cfg(test)]
#[path = "error_messages_tests.rs"]
mod error_messages_tests;
