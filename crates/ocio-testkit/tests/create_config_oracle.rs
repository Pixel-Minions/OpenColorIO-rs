// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Contract of O7.2: `spec.config` builds the config of upstream's GPU harness
//! (`tests/gpu/GPUUnitTest.cpp:137-142 @ v2.5.2`), including its disabled processor cache.

use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle::f32_to_bytes;
use serde_json::{Value, json};

/// The new form is Config::Create (Python's Config constructor), not CreateRaw, and disabling
/// its cache changes repeated getProcessor calls from one held processor to two. Every field
/// is compared with an independent direct call of the wheel in the same process.
#[test]
fn create_config_is_the_gpu_harness_config() {
    let script = r#"
import json
import PyOpenColorIO as OCIO
from ocio_oracle import spec

def report(config):
    transform = OCIO.MatrixTransform(offset=[0.25, 0.0, 0.0, 0.0])
    first = config.getProcessor(transform)
    second = config.getProcessor(transform)
    return {"serialize": config.serialize(), "colorspaces": list(config.getColorSpaceNames()),
            "roles": list(config.getRoles()), "same_processor": first is second,
            "processor_id": first.getCacheID()}

actual = [report(spec.config(value)) for value in
          [None, {"create": {}}, {"create": {"processor_cache": "off"}}]]
direct = [OCIO.Config.CreateRaw(), OCIO.Config(), OCIO.Config()]
direct[2].setProcessorCacheFlags(OCIO.PROCESSOR_CACHE_OFF)
print(json.dumps({"actual": actual, "direct": [report(c) for c in direct]}))
"#;
    let lines = Oracle::get().run_script(script, &[]);
    assert_eq!(lines.len(), 1);
    let result: Value = serde_json::from_str(&lines[0]).expect("a JSON report");
    let actual = result["actual"].as_array().expect("three config forms");
    let direct = result["direct"]
        .as_array()
        .expect("three direct wheel configs");
    assert_eq!(actual.len(), 3);
    assert_eq!(direct.len(), 3);
    assert_eq!(actual, direct);
    assert_ne!(direct[0]["serialize"], direct[1]["serialize"]);
    assert_ne!(direct[1]["same_processor"], direct[2]["same_processor"]);
    assert_eq!(direct[1]["same_processor"], json!(true));
    assert_eq!(direct[2]["same_processor"], json!(false));
}

/// cpu_apply, processor_ops and gpu_shader all reach spec.config through _processor. Their
/// results for an explicit Create/cache-off config equal independent direct wheel calls.
#[test]
fn processor_commands_accept_create_configs() {
    let transform = json!({"class": "MatrixTransform", "args": {"offset": [0.25, 0.0, 0.0, 0.0]}});
    let pixels = f32_to_bytes(&[0.125, 0.25, 0.5, 1.0]);
    let args = json!({"config": {"create": {"processor_cache": "off"}}, "transform": transform});
    let calls = [
        BatchCall {
            cmd: "cpu_apply",
            args: args.clone(),
            blobs: vec![&pixels],
        },
        BatchCall {
            cmd: "processor_ops",
            args: args.clone(),
            blobs: vec![],
        },
        BatchCall {
            cmd: "gpu_shader",
            args,
            blobs: vec![],
        },
    ];
    let responses = Oracle::get().batch(&calls, false);
    assert_eq!(responses.len(), calls.len());
    let responses: Vec<_> = responses
        .into_iter()
        .map(|r| r.expect("the command succeeds"))
        .collect();
    for response in &responses {
        assert!(
            response.result.get("exception").is_none(),
            "{}",
            response.result
        );
    }
    let script = r#"
import json
import numpy as np
import PyOpenColorIO as OCIO
from ocio_oracle.checks import dump
config = OCIO.Config()
config.setProcessorCacheFlags(OCIO.PROCESSOR_CACHE_OFF)
proc = config.getProcessor(OCIO.MatrixTransform(offset=[0.25, 0.0, 0.0, 0.0]))
cpu = proc.getDefaultCPUProcessor()
image = np.array([0.125, 0.25, 0.5, 1.0], dtype=np.float32)
cpu.applyRGBA(image)
gpu = proc.getDefaultGPUProcessor()
desc = OCIO.GpuShaderDesc.CreateShaderDesc()
gpu.extractGpuShaderInfo(desc)
arrays = []
processor = {"cache_id": proc.getCacheID(), "isNoOp": proc.isNoOp(),
             "hasChannelCrosstalk": proc.hasChannelCrosstalk(), "isDynamic": proc.isDynamic(),
             "processor_metadata": dump(proc.getProcessorMetadata(), arrays),
             "group": dump(proc.createGroupTransform(), arrays)}
assert not arrays
print(json.dumps({"processor_id": proc.getCacheID(), "cpu_id": cpu.getCacheID(),
                  "pixels": image.view(np.uint32).tolist(), "gpu_id": gpu.getCacheID(),
                  "shader": desc.getShaderText(), "processor": processor}))
"#;
    let lines = Oracle::get().run_script(script, &[]);
    assert_eq!(lines.len(), 1);
    let direct: Value = serde_json::from_str(&lines[0]).expect("direct wheel report");
    assert_eq!(
        responses[0].result["processor_cache_id"],
        direct["processor_id"]
    );
    assert_eq!(responses[0].result["cpu_cache_id"], direct["cpu_id"]);
    assert_eq!(responses[0].blobs.len(), 1);
    let bits: Vec<_> = responses[0].blobs[0]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_le_bytes(*b))
        .collect();
    assert_eq!(json!(bits), direct["pixels"]);
    assert_eq!(
        responses[2].result["processor_cache_id"],
        direct["processor_id"]
    );
    assert_eq!(responses[2].result["gpu_cache_id"], direct["gpu_id"]);
    assert_eq!(
        responses[2].blobs[0],
        direct["shader"].as_str().expect("shader text").as_bytes()
    );
    assert!(direct["processor"].is_object());
    assert_eq!(responses[1].result["processor"], direct["processor"]);
    assert!(responses[1].blobs.is_empty());
}

/// Config::Create has no colorspaces. A transform-only processor would also succeed if a
/// consumer accidentally ignored its requested config, so use CreateRaw's real raw space.
#[test]
fn processor_commands_use_the_requested_create_config() {
    let script = r#"
import json
import PyOpenColorIO as OCIO
raw = OCIO.Config.CreateRaw()
name = list(raw.getColorSpaceNames())[0]
control = raw.getProcessor(name, name)
config = OCIO.Config()
config.setProcessorCacheFlags(OCIO.PROCESSOR_CACHE_OFF)
log = []
OCIO.SetLoggingFunction(log.append)
try:
    config.getProcessor(name, name)
except OCIO.Exception as error:
    result = {"exception": {"type": type(error).__name__, "message": str(error)},
              "stage": "processor", "log": log}
else:
    raise AssertionError("Config::Create unexpectedly contains the raw colorspace")
finally:
    OCIO.ResetToDefaultLoggingFunction()
print(json.dumps({"name": name, "expected": result, "raw_id": control.getCacheID()}))
"#;
    let lines = Oracle::get().run_script(script, &[]);
    assert_eq!(lines.len(), 1);
    let direct: Value =
        serde_json::from_str(&lines[0]).expect("direct wheel config-sensitive report");
    assert!(direct["expected"]["exception"].is_object());
    let pixels = f32_to_bytes(&[0.125, 0.25, 0.5, 1.0]);
    for command in ["cpu_apply", "processor_ops", "gpu_shader"] {
        let blobs = if command == "cpu_apply" {
            vec![pixels.as_slice()]
        } else {
            vec![]
        };
        let args = json!({"config": {"create": {"processor_cache": "off"}},
                         "src": direct["name"], "dst": direct["name"]});
        let control = json!({"src": direct["name"], "dst": direct["name"]});
        let responses = Oracle::get().batch(
            &[
                BatchCall {
                    cmd: command,
                    args,
                    blobs: blobs.clone(),
                },
                BatchCall {
                    cmd: command,
                    args: control,
                    blobs,
                },
            ],
            false,
        );
        let mut responses = responses.into_iter();
        let requested = responses
            .next()
            .unwrap()
            .expect("the rejected config is reported");
        let raw = responses.next().unwrap().expect("the raw config succeeds");
        assert_eq!(requested.result, direct["expected"], "{command}");
        assert!(requested.blobs.is_empty(), "{command}");
        assert!(
            raw.result.get("exception").is_none(),
            "{command}: {}",
            raw.result
        );
        let raw_id = if command == "processor_ops" {
            &raw.result["processor"]["cache_id"]
        } else {
            &raw.result["processor_cache_id"]
        };
        assert_eq!(raw_id, &direct["raw_id"], "{command}");
    }
}

/// Misspellings and values that would silently change the requested config are refused.
#[test]
fn create_config_rejects_unknown_settings() {
    let script = r#"
import json
from ocio_oracle import spec
cases = [{"create": None}, {"create": []}, {"create": {"processor_cache": False}},
         {"create": {"processor_cache": "on"}}, {"create": {"processor_cashe": "off"}},
         {"create": {}, "builtin": "ocio://cg-config-latest"}]
results = []
for case in cases:
    try:
        spec.config(case)
    except ValueError as error:
        results.append({"type": type(error).__name__, "message": str(error)})
    else:
        results.append(None)
print(json.dumps(results))
"#;
    let lines = Oracle::get().run_script(script, &[]);
    assert_eq!(lines.len(), 1);
    let results: Vec<Value> = serde_json::from_str(&lines[0]).expect("refused settings");
    assert_eq!(results.len(), 6);
    for result in results {
        assert_eq!(result["type"], json!("ValueError"));
        assert!(
            result["message"]
                .as_str()
                .expect("an error")
                .starts_with("a create config")
        );
    }
}
