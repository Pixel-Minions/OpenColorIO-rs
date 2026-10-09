// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of `YAML::Load` and `YAML::LoadAll` (include/yaml-cpp/node/parse.h, src/parse.cpp,
//! yaml-cpp 0.8.0) over the bytes of a stream. `LoadFile` and `LoadAllFromFile` open files;
//! OCIO opens its files itself, so they are not ported.

use super::exceptions::Result;
use super::node::Node;
use super::node_builder::NodeBuilder;
use super::parser::Parser;
use super::stream::IStream;

/// `Load(std::istream&)` (parse.cpp:22-30): the first document, or `Node()` if there is
/// none. OCIO reads a config with it (`OCIOYaml::Read`, OCIOYaml.cpp:5424 @ v2.5.2).
pub fn load<'a>(input: impl Into<IStream<'a>>) -> Result<Node> {
    let mut parser = Parser::new(input);
    let mut builder = NodeBuilder::new();
    if !parser.handle_next_document(&mut builder)? {
        return Ok(Node::new());
    }
    Ok(builder.root())
}

/// `LoadAll(std::istream&)` (parse.cpp:50-63): every document.
pub fn load_all<'a>(input: impl Into<IStream<'a>>) -> Result<Vec<Node>> {
    let mut docs = Vec::new();
    let mut parser = Parser::new(input);
    loop {
        let mut builder = NodeBuilder::new();
        if !parser.handle_next_document(&mut builder)? {
            break;
        }
        docs.push(builder.root());
    }
    Ok(docs)
}

#[cfg(test)]
#[path = "load_node_tests.rs"]
mod tests;
