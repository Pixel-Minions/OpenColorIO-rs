// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! S1 (PLAN.md §3): the yaml-cpp emitter port writes each built-in config exactly as
//! OCIO 2.5.2 serializes it.
//!
//! Each `serialize.ocio` fixture (the wheel's `Config.serialize()`) is read into a tree with
//! its styles and tags, then written again through `ocio::yaml_cpp::Emitter` with the calls
//! `OCIOYaml.cpp` makes (`common::ocio_writer`), and the output must equal the fixture byte
//! for byte.

mod common;

use common::{ocio_writer::OcioWriter, yaml_tree};
use ocio_testkit::{assert_text_eq, fixtures};

/// The `serialize.ocio` fixtures of the built-in configs.
fn serialized_configs() -> Vec<String> {
    let paths: Vec<String> = fixtures::list("builtin_configs/")
        .into_iter()
        .filter(|p| p.ends_with("/serialize.ocio"))
        .collect();
    assert_eq!(paths.len(), 8, "the eight built-in configs: {paths:?}");
    paths
}

#[test]
fn builtin_configs_reemit_byte_identically() {
    for path in serialized_configs() {
        let expected = fixtures::read_text(&path);
        let tree = yaml_tree::parse(&expected);
        let mut writer = OcioWriter::new();
        writer.save_config(&tree);
        assert_text_eq(&path, &expected, writer.text());
        // The luma coefficients and the transforms' parameters went through the emitter's
        // double formatting, not through as text.
        assert!(writer.numbers > 3, "{path}: {} numbers", writer.numbers);
    }
}
