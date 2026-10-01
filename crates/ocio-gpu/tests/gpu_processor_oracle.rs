// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The function header and footer of the GPU processor against the wheel: a processor with no
//! op extracts only them, then finalizes, so the port's description, given the same settings,
//! the header, the footer and a finalize, must hold the wheel's shader.

use ocio_gpu::gpu_processor::{write_shader_footer, write_shader_header};
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_testkit::gpu::{self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings};
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::json;

/// In the 10 languages, with the default names, other names, an empty pixel name (an error,
/// but in OSL), an empty function name, and names with double underscores and non-ASCII bytes,
/// the port writes the wheel's shader for a processor with no op (an identity matrix, which the
/// optimizer removes): the same text, cache ID and settings, or the same error.
#[test]
fn a_processor_without_ops_writes_the_wheels_header_and_footer() {
    // (resource prefix, function name, pixel name); `None` keeps the default.
    type Names = (
        Option<&'static str>,
        Option<&'static str>,
        Option<&'static str>,
    );
    let names: [Names; 6] = [
        (None, None, None),
        (Some("R3"), Some("F1"), Some("P2")),
        (None, None, Some("")),
        (None, Some(""), None),
        (Some("a__b"), Some("f__\u{e9}"), Some("p__x")),
        (Some(""), Some("main"), Some("\u{3c0}")),
    ];
    let mut cases = Vec::new();
    for (language, oracle_language) in GpuLanguage::ALL
        .into_iter()
        .zip(oracle_gpu::GpuLanguage::ALL)
    {
        for (prefix, function_name, pixel_name) in names {
            cases.push((
                language,
                (prefix, function_name, pixel_name),
                GpuShaderRequest::new(
                    json!({"transform": {"class": "MatrixTransform"}}),
                    ShaderSettings {
                        language: Some(oracle_language),
                        resource_prefix: prefix.map(String::from),
                        function_name: function_name.map(String::from),
                        pixel_name: pixel_name.map(String::from),
                        ..ShaderSettings::default()
                    },
                ),
            ));
        }
    }
    let calls: Vec<_> = cases.iter().map(|(_, _, r)| r.call()).collect();
    for ((language, names, _), response) in cases.iter().zip(Oracle::get().batch(&calls, true)) {
        let label = format!("{language:?} {names:?}");
        let reply = GpuShaderReply::from_response(response.expect("the oracle"));
        assert_eq!(
            reply.result["gpu_processor"]["isNoOp"],
            json!(true),
            "{label}: the processor has no op"
        );

        let (prefix, function_name, pixel_name) = *names;
        let mut desc = GpuShaderDesc::new(*language);
        if let Some(p) = prefix {
            desc.set_resource_prefix(p);
        }
        if let Some(f) = function_name {
            desc.set_function_name(f);
        }
        if let Some(p) = pixel_name {
            desc.set_pixel_name(p);
        }
        // GPUProcessor.cpp:100-114 @ v2.5.2, with no op.
        let extracted = write_shader_header(&mut desc).and_then(|()| {
            write_shader_footer(&mut desc);
            desc.finalize()
        });

        match reply.raised() {
            Some(raised) => {
                let error = extracted.expect_err(&label);
                assert_eq!(error.message(), raised.message, "{label}");
            }
            None => {
                extracted.unwrap_or_else(|e| panic!("{label}: {e}"));
                let shader = reply.shader();
                assert_text_eq(&label, &shader.text, desc.shader_text_utf8().unwrap());
                assert_eq!(
                    String::from_utf8(desc.cache_id()).unwrap(),
                    shader.cache_id,
                    "{label}"
                );
                for (key, value) in [
                    ("function_name", desc.function_name()),
                    ("pixel_name", desc.pixel_name()),
                    ("resource_prefix", desc.resource_prefix()),
                ] {
                    assert_eq!(
                        shader.getters[key].as_str().map(str::as_bytes),
                        Some(value),
                        "{label}: {key}"
                    );
                }
            }
        }
    }
}
