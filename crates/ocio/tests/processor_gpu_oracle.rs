// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The processor's GPU processors against the wheel, through the oracle's `gpu_shader`:
//! `getDefaultGPUProcessor` and `getOptimizedGPUProcessor` of `Config::getProcessor`'s
//! processors, their cache IDs and flags, and the shader they write in the 10 languages.
//!
//! Here, empty groups: their processors have no ops, so the shader is the function's header
//! and footer. Every class's shaders through the API, at every level and in every language,
//! are in `api_gpu_oracle.rs`.

use ocio::{Config, GroupTransform, OptimizationFlags, Transform, TransformDirection};
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_testkit::gpu::{self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings};
use ocio_testkit::{Oracle, assert_text_eq};
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
    vec![
        leaf(F),
        Group {
            dir: I,
            children: vec![leaf(F), leaf(I)],
        },
    ]
}

/// Each group, in each direction, in each language, from the default GPU processor and from
/// three optimizations: the port's processor and GPU processor give the wheel's cache IDs and
/// flags, and the description the wheel's shader text and cache ID.
#[test]
fn group_gpu_processors_match_the_wheel() {
    // `None` is getDefaultGPUProcessor.
    let optimizations: [Option<(OptimizationFlags, &str)>; 4] = [
        None,
        Some((OptimizationFlags::NONE, "OPTIMIZATION_NONE")),
        Some((OptimizationFlags::DEFAULT, "OPTIMIZATION_DEFAULT")),
        Some((OptimizationFlags::ALL, "OPTIMIZATION_ALL")),
    ];
    let config = Config::create_raw().unwrap();
    let mut cases = Vec::new();
    for group in groups() {
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            for optimization in optimizations {
                for (language, oracle_language) in GpuLanguage::ALL
                    .into_iter()
                    .zip(oracle_gpu::GpuLanguage::ALL)
                {
                    let mut processor =
                        json!({"transform": group.spec(), "direction": dir_name(dir)});
                    if let Some((_, name)) = optimization {
                        processor["optimization"] = json!(name);
                    }
                    cases.push((
                        group.clone(),
                        dir,
                        optimization,
                        language,
                        GpuShaderRequest::new(processor, ShaderSettings::language(oracle_language)),
                    ));
                }
            }
        }
    }

    let calls: Vec<_> = cases.iter().map(|case| case.4.call()).collect();
    let mut failures = Vec::new();
    for ((group, dir, optimization, language, _), response) in
        cases.iter().zip(Oracle::get().batch(&calls, true))
    {
        let label = format!("{group:?} {dir:?} {optimization:?} {language:?}");
        let reply = GpuShaderReply::from_response(response.expect("the oracle"));
        assert!(reply.raised().is_none(), "{label}: {:?}", reply.raised());

        let processor = config
            .processor_in_direction(&group.port(), *dir)
            .expect("a processor");
        let gpu = match optimization {
            None => processor.default_gpu_processor(),
            Some((flags, _)) => processor.optimized_gpu_processor(*flags),
        }
        .expect("a GPU processor");
        let mut desc = GpuShaderDesc::new(*language);
        gpu.extract_gpu_shader_info(&mut desc)
            .expect("the extraction");

        let port = json!({
            "processor_cache_id": processor.cache_id().expect("a cache ID"),
            "gpu_cache_id": String::from_utf8(gpu.get_cache_id().to_vec()).expect("UTF-8"),
            "gpu_processor": {"isNoOp": gpu.is_no_op(),
                "hasChannelCrosstalk": gpu.has_channel_crosstalk()},
            "shader_cache_id": String::from_utf8(desc.cache_id()).expect("UTF-8"),
        });
        let shader = reply.shader();
        let wheel = json!({
            "processor_cache_id": reply.result["processor_cache_id"],
            "gpu_cache_id": reply.result["gpu_cache_id"],
            "gpu_processor": reply.result["gpu_processor"],
            "shader_cache_id": shader.cache_id,
        });
        if port != wheel {
            failures.push(format!("{label}\n  wheel {wheel}\n  port  {port}"));
            continue;
        }
        assert_text_eq(
            &label,
            &shader.text,
            desc.shader_text_utf8().expect("UTF-8"),
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
