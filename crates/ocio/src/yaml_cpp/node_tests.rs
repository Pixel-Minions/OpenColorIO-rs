// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0 `test/node/node_test.cpp`: the tests of the API for reading nodes
//! that `Node()` can run on its own. The other 67 need APIs the port leaves out, since OCIO
//! only reads loaded documents: the 60 other `NodeTest` tests build or change nodes
//! (assignment and `encode` of values and containers, `push_back`, `remove`, `force_insert`,
//! the inserting `operator[]`) or clone them (`CloneNull`), and the 7 `NodeEmitterTest`
//! tests emit them.

use crate::yaml_cpp::event_handler::EmitterStyle;
use crate::yaml_cpp::node::Node;

/// Port of yaml-cpp 0.8.0 `TEST(NodeTest, UndefinedConstNodeWithFallback)`.
#[test]
fn undefined_const_node_with_fallback() {
    let node = Node::new();
    let cn = &node;
    assert_eq!(cn.get("undefined").unwrap().as_or::<i32>(3).unwrap(), 3);
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeTest, ConstIteratorOnConstUndefinedNode)`.
#[test]
fn const_iterator_on_const_undefined_node() {
    let node = Node::new();
    let cn = &node;
    let undefined_cn = cn.get("undefined").unwrap();

    let count = undefined_cn.iter().count();
    assert_eq!(0, count);
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeTest, DefaultNodeStyle)`.
#[test]
fn default_node_style() {
    let node = Node::new();
    assert_eq!(EmitterStyle::Default, node.style().unwrap());
}
