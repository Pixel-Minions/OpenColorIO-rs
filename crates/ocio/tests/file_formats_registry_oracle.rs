// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port's format registry against the wheel's (`file_formats`): the formats a file
//! transform reads (`FileTransform::getFormats`), those the baker bakes and those a group
//! writes, each in the registry's order, and which extensions are supported.

use ocio::FileTransform;
use ocio::transforms::file_format::{FormatCapabilities, FormatRegistry};
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::bytes;
use serde_json::{Value, json};

/// A list of the wheel's result as (name, extension) pairs.
fn pairs(list: &Value) -> Vec<(Vec<u8>, Vec<u8>)> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|p| (bytes(&p[0]), bytes(&p[1])))
        .collect()
}

/// The port's list of a capability, by index as upstream's getters give it.
fn port_list(capability: FormatCapabilities) -> Vec<(Vec<u8>, Vec<u8>)> {
    let registry = FormatRegistry::instance();
    (0..registry.num_formats(capability))
        .map(|i| {
            (
                registry
                    .format_name_by_index(capability, i)
                    .as_bytes()
                    .to_vec(),
                registry
                    .format_extension_by_index(capability, i)
                    .as_bytes()
                    .to_vec(),
            )
        })
        .collect()
}

/// The three lists are the wheel's, in order; so are the file transform's own getters.
#[test]
fn the_lists_are_the_wheels() {
    let result = Oracle::get().call("file_formats", json!({}), &[]).result;
    assert_eq!(port_list(FormatCapabilities::READ), pairs(&result["read"]));
    assert_eq!(port_list(FormatCapabilities::BAKE), pairs(&result["bake"]));
    assert_eq!(
        port_list(FormatCapabilities::WRITE),
        pairs(&result["write"])
    );
    let read: Vec<(Vec<u8>, Vec<u8>)> = (0..FileTransform::num_formats())
        .map(|i| {
            (
                FileTransform::format_name_by_index(i).to_vec(),
                FileTransform::format_extension_by_index(i).to_vec(),
            )
        })
        .collect();
    assert_eq!(read, pairs(&result["read"]));
    let infos: Vec<(Vec<u8>, Vec<u8>)> = FileTransform::formats()
        .iter()
        .map(|f| (f.name.as_bytes().to_vec(), f.extension.as_bytes().to_vec()))
        .collect();
    assert_eq!(infos, pairs(&result["read"]));
}

/// Extensions in every case, with and without a dot, empty, a dot alone, two dots, a NUL and
/// bytes past ASCII: supported as the wheel says.
#[test]
fn extensions_are_supported_as_in_the_wheel() {
    let mut extensions: Vec<Vec<u8>> = Vec::new();
    for e in [
        "3dl", "cc", "ccc", "cdl", "clf", "ctf", "csp", "lut", "icc", "icm", "pf", "cube", "itx",
        "look", "mga", "m3d", "spi1d", "spi3d", "spimtx", "cub", "vf", "ocio", "ocioz", "txt",
    ] {
        extensions.push(e.as_bytes().to_vec());
        extensions.push(format!(".{e}").into_bytes());
        extensions.push(e.to_uppercase().into_bytes());
        extensions.push(format!("..{e}").into_bytes());
    }
    for e in [
        &b""[..],
        b".",
        b"..",
        b"c\x00dl",
        b"cdl\x00x",
        b" cdl",
        b"\xff",
        b"CdL",
    ] {
        extensions.push(e.to_vec());
    }
    let args: Vec<Value> = extensions
        .iter()
        .map(|e| json!({ "bytes": e.iter().map(|b| format!("{b:02x}")).collect::<String>() }))
        .collect();
    let result = Oracle::get()
        .call("file_formats", json!({ "extensions": args }), &[])
        .result;
    let supported = result["supported"].as_array().unwrap();
    for (extension, wheel) in extensions.iter().zip(supported) {
        assert_eq!(
            json!({ "result": FileTransform::is_format_extension_supported(extension) }),
            *wheel,
            "{}",
            extension.escape_ascii()
        );
    }
}
