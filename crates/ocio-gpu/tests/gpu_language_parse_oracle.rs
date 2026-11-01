// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! GPU-language string conversions against the existing live module-call oracle.
//! Inputs follow ParseUtils.cpp:258-296 @ v2.5.2; outputs and exception bytes come
//! only from the official wheel, including nullable and non-UTF8 C strings.

use ocio_gpu::open_color_types::{GpuLanguage, gpu_language_from_string, gpu_language_to_string};
use ocio_testkit::Oracle;
use ocio_testkit::gpu::GpuLanguage as OracleLanguage;
use ocio_testkit::oracle_values::{exception, hex, result_bytes};
use serde_json::{Value, json};

fn inputs() -> Vec<Option<Vec<u8>>> {
    let mut values = vec![None, Some(Vec::new())];
    // Names from GpuLanguageToString, plus unsupported spellings as probe inputs.
    for word in [
        "cg",
        "glsl_1.2",
        "glsl_1.3",
        "glsl_4.0",
        "glsl_vk_4.6",
        "glsl_es_1.0",
        "glsl_es_3.0",
        "hlsl_sm_5.0",
        "osl_1",
        "msl_2",
        "hlsl_dx11",
        "msl_2.0",
        "unknown",
    ] {
        values.push(Some(word.as_bytes().to_vec()));
        values.push(Some(word.to_ascii_uppercase().into_bytes()));
        let mut mixed = word.as_bytes().to_vec();
        mixed[0] = mixed[0].to_ascii_uppercase();
        values.push(Some(mixed));
        values.push(Some(format!(" {word}").into_bytes()));
        values.push(Some(format!("{word}\t").into_bytes()));
        values.push(Some([word.as_bytes(), b"\0ignored"].concat()));
    }
    values.extend([
        Some(b"\0glsl_1.2".to_vec()),
        Some(b"glsl_1.2\0\xff".to_vec()),
        Some(b"glsl_1.2\xff".to_vec()),
        Some(b"\xc3\x89".to_vec()),
        Some(b"'\"\\".to_vec()),
    ]);
    values.extend((0..=u8::MAX).map(|byte| Some(vec![byte])));
    values
}

#[test]
fn language_conversions_match_the_wheel_for_nullable_and_byte_inputs() {
    let inputs = inputs();
    let mut calls = inputs
        .iter()
        .map(|input| {
            let argument = input
                .as_ref()
                .map_or(Value::Null, |bytes| json!({"bytes": hex(bytes)}));
            json!({"call": "GpuLanguageFromString", "on": "OCIO", "args": [argument]})
        })
        .collect::<Vec<_>>();
    calls.extend(OracleLanguage::ALL.iter().map(|language| {
        json!({"call": "GpuLanguageToString", "on": "OCIO", "args": [{"enum": language.oracle_name()}]})
    }));
    let response = Oracle::get().call(
        "config_calls",
        json!({"config": "raw", "calls": calls}),
        &[],
    );
    let results = response.result["calls"]
        .as_array()
        .expect("one result per module call");
    assert_eq!(results.len(), inputs.len() + GpuLanguage::ALL.len());
    for (input, expected) in inputs.iter().zip(results) {
        let label = format!("GpuLanguageFromString({input:02x?}): {expected}");
        match gpu_language_from_string(input.as_deref()) {
            Ok(language) => assert_eq!(
                expected["result"]["enum"].as_str(),
                Some(OracleLanguage::ALL[language as usize].oracle_name()),
                "{label}"
            ),
            Err(error) => {
                if let Some(bytes) = expected["undecodable"].as_str() {
                    assert_eq!(hex(error.what()), bytes, "{label}");
                } else {
                    let (kind, message) = exception(expected);
                    assert_eq!(kind, "Exception", "{label}");
                    assert_eq!(error.what(), message, "{label}");
                }
            }
        }
    }
    for (language, expected) in GpuLanguage::ALL.iter().zip(&results[inputs.len()..]) {
        assert_eq!(
            gpu_language_to_string(*language).as_bytes(),
            result_bytes(expected)
        );
    }
}
