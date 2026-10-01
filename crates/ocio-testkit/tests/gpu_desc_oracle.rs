// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `gpu_shader_desc` command (`oracle/ocio_oracle/gpu_desc.py`, card
//! p1-gpu-infra) against the wheel itself:
//! - upstream's `GpuShader` `generic_shader` test holds through the command, but for its
//!   uniform, which Python can't add;
//! - every call's outcome and the description's state agree with an independent read of the
//!   wheel;
//! - the command refuses exactly a finalize in MSL that would read past a line, one it can't
//!   check, and a texture whose float count would wrap, and nothing next to them; and requests
//!   it can't read exactly.

use ocio_testkit::Oracle;
use ocio_testkit::gpu::{GpuLanguage, Texture, Texture3d};
use ocio_testkit::gpu_desc::{
    DescCall, DescOutcome, GpuShaderDescReply, GpuShaderDescRequest, Returned,
};
use serde_json::json;

use DescCall::*;

/// Runs `requests` in one oracle process; each must succeed.
fn run(requests: &[GpuShaderDescRequest]) -> Vec<GpuShaderDescReply> {
    let calls: Vec<_> = requests.iter().map(GpuShaderDescRequest::call).collect();
    Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            GpuShaderDescReply::from_response(r.unwrap_or_else(|e| panic!("request {i}: {e}")))
        })
        .collect()
}

fn text(outcome: &DescOutcome) -> &str {
    match outcome {
        DescOutcome::Returned(Returned::Text(s)) => s,
        other => panic!("not a text: {other:?}"),
    }
}

fn number(outcome: &DescOutcome) -> u64 {
    match outcome {
        DescOutcome::Returned(Returned::Number(n)) => *n,
        other => panic!("not a number: {other:?}"),
    }
}

fn raised(outcome: &DescOutcome) -> &str {
    match outcome {
        DescOutcome::Raised(r) => {
            assert_eq!(r.kind, "Exception");
            &r.message
        }
        other => panic!("nothing raised: {other:?}"),
    }
}

fn texture(outcome: &DescOutcome) -> &Texture {
    match outcome {
        DescOutcome::Returned(Returned::Texture(t)) => t,
        other => panic!("not a texture: {other:?}"),
    }
}

fn texture_3d(outcome: &DescOutcome) -> &Texture3d {
    match outcome {
        DescOutcome::Returned(Returned::Texture3d(t)) => t,
        other => panic!("not a 3D texture: {other:?}"),
    }
}

/// Upstream's `GpuShader` test `generic_shader` (tests/cpu/GpuShader_tests.cpp:16-253
/// @ v2.5.2), run on the wheel through the command, with upstream's expected values. Python
/// has no `addUniform`, so the test's uniform and its buffer size are left out; nothing else
/// depends on them.
#[test]
fn upstream_generic_shader_holds() {
    // tests/cpu/GpuShader_tests.cpp:50-51, 56-58 and 135-137 @ v2.5.2.
    let values: Vec<f32> = [0.1f32, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9].repeat(2);
    let values_3d: Vec<f32> = [
        0.1f32, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.7, 0.8, 0.9,
    ]
    .repeat(2);
    let add_texture = |name: &str, dimensions: &str| AddTexture {
        name: name.into(),
        sampler_name: format!("{name}Sampler"),
        width: 3,
        height: 2,
        channel: "TEXTURE_RGB_CHANNEL".into(),
        dimensions: dimensions.into(),
        interpolation: "INTERP_TETRAHEDRAL".into(),
        values: values.clone(),
    };
    let add_3d_texture = |name: &str, edge_len: u32| Add3dTexture {
        name: name.into(),
        sampler_name: format!("{name}Sampler"),
        edge_len,
        interpolation: "INTERP_TETRAHEDRAL".into(),
        values: if edge_len == 2 {
            values_3d.clone()
        } else {
            vec![0.0; (edge_len * edge_len * edge_len * 3) as usize]
        },
    };
    let calls = vec![
        SetLanguage(GpuLanguage::Glsl13),
        SetFunctionName("1sd234_".into()),
        SetPixelName("pxl_1sd234_".into()),
        SetResourcePrefix("res_1sd234_".into()),
        Finalize,
        GetCacheId, // 5
        SetResourcePrefix("res_1".into()),
        Finalize,
        GetCacheId, // 8
        add_texture("lut1", "TEXTURE_2D"),
        GetTexture(0), // 10
        GetTexture(1),
        add_texture("lut2", "TEXTURE_2D"),
        GetTexture(2), // 13
        add_3d_texture("lut1", 2),
        Get3dTexture(0), // 15
        Get3dTexture(1),
        add_3d_texture("lut2", 2),
        add_3d_texture("lut1", 130), // 18
        AddToParameterDeclareShaderCode("vec2 coords;\n".into()),
        AddToHelperShaderCode("vec2 helpers() {}\n\n".into()),
        AddToFunctionHeaderShaderCode("void func() {\n".into()),
        AddToFunctionShaderCode("  int i;\n".into()),
        AddToFunctionFooterShaderCode("}\n".into()),
        Finalize,
        GetShaderText, // 25
        SetLanguage(GpuLanguage::GlslVk46),
        SetDescriptorSetIndex(123, 0), // 27
        SetDescriptorSetIndex(123, 456),
        AddToTextureDeclareShaderCode(
            "layout(set=123, binding = 456) uniform sampler2D samplerName; \n".into(),
        ),
        Finalize,
        GetShaderText, // 31
    ];
    let reply = GpuShaderDescRequest::new(calls).run();
    let c = &reply.calls;

    assert_eq!(
        text(&c[5]),
        "glsl_1.3 1sd234_ res_1sd234_ pxl_1sd234_ 0 0 1 6001c324468d497f99aa06d3014798d8"
    );
    assert_ne!(text(&c[8]), text(&c[5]));

    assert_eq!(number(&c[9]), 1);
    let t = texture(&c[10]);
    assert_eq!(
        (t.name.as_str(), t.sampler_name.as_str(), t.width, t.height),
        ("lut1", "lut1Sampler", 3, 2)
    );
    assert_eq!(
        (t.channel.as_str(), t.interpolation.as_str()),
        ("TEXTURE_RGB_CHANNEL", "INTERP_TETRAHEDRAL")
    );
    assert_eq!(t.values, values);
    assert!(raised(&c[11]).contains("1D LUT access error"));
    assert_eq!(number(&c[12]), 2);
    assert!(raised(&c[13]).contains("1D LUT access error"));

    assert_eq!(number(&c[14]), 3);
    let t = texture_3d(&c[15]);
    assert_eq!(
        (t.name.as_str(), t.sampler_name.as_str(), t.edge_len),
        ("lut1", "lut1Sampler", 2)
    );
    assert_eq!(t.interpolation, "INTERP_TETRAHEDRAL");
    assert_eq!(t.values, values_3d);
    assert!(raised(&c[16]).contains("3D LUT access error"));
    assert_eq!(number(&c[17]), 4);
    raised(&c[18]);

    let mut frag_text = String::new();
    frag_text += "\n";
    frag_text += "// Declaration of all variables\n";
    frag_text += "\n";
    frag_text += "vec2 coords;\n";
    frag_text += "\n";
    frag_text += "// Declaration of all helper methods\n";
    frag_text += "\n";
    frag_text += "vec2 helpers() {}\n\n";
    frag_text += "void func() {\n";
    frag_text += "  int i;\n";
    frag_text += "}\n";
    assert_eq!(text(&c[25]), frag_text);

    assert!(raised(&c[27]).contains("Texture binding start index must be greater than 0."));
    assert_eq!(reply.shader.getters["descriptor_set_index"], json!(123));
    assert_eq!(reply.shader.getters["texture_binding_start"], json!(456));

    let mut frag_text = String::new();
    frag_text += "layout (set = 123, binding = 0) uniform 1sd234__Parameters\n";
    frag_text += "{\n";
    frag_text += "\n";
    frag_text += "// Declaration of all variables\n";
    frag_text += "\n";
    frag_text += "vec2 coords;\n";
    frag_text += "\n";
    frag_text += "};\n";
    frag_text += "\n";
    frag_text += "// Declaration of all textures\n";
    frag_text += "\n";
    frag_text += "layout(set=123, binding = 456) uniform sampler2D samplerName; \n";
    frag_text += "\n";
    frag_text += "// Declaration of all helper methods\n";
    frag_text += "\n";
    frag_text += "vec2 helpers() {}\n\n";
    frag_text += "void func() {\n";
    frag_text += "  int i;\n";
    frag_text += "}\n";
    assert_eq!(text(&c[31]), frag_text);
}

/// Drives a GpuShaderDesc through each case's calls (the command's own call list) and prints
/// what each call returned or raised, then the description's state, as JSON: written
/// independently of the command, to check it.
const DESC_READ: &str = r#"
import json, struct, sys
import numpy as np
import PyOpenColorIO as OCIO

def bits(values):
    return np.asarray(values, dtype=np.float32).view(np.uint32).tolist()

def texture(t):
    out = {"name": t.textureName, "sampler_name": t.samplerName,
           "interpolation": t.interpolation.name,
           "binding_index": t.textureShaderBindingIndex, "values": bits(t.getValues())}
    if hasattr(t, "edgeLen"):
        out["edge_len"] = t.edgeLen
    else:
        out.update(width=t.width, height=t.height, channel=t.channel.name,
                   dimensions=t.dimensions.name)
    return out

for case in json.load(open(sys.argv[1], encoding="utf-8")):
    calls, blobs = case["calls"], case["blobs"]
    desc = OCIO.GpuShaderDesc.CreateShaderDesc()
    outcomes = []
    for call in calls:
        name, args = call[0], call[1:]
        try:
            if name == "setLanguage":
                result = desc.setLanguage(OCIO.GpuLanguage.__members__[args[0]])
            elif name == "addTexture":
                result = desc.addTexture(
                    args[0], args[1], args[2], args[3],
                    OCIO.GpuShaderDesc.TextureType.__members__[args[4]],
                    OCIO.GpuShaderDesc.TextureDimensions.__members__[args[5]],
                    OCIO.Interpolation.__members__[args[6]],
                    np.asarray(blobs[args[7]], dtype=np.uint32).view(np.float32))
            elif name == "add3DTexture":
                result = desc.add3DTexture(
                    args[0], args[1], args[2], OCIO.Interpolation.__members__[args[3]],
                    np.asarray(blobs[args[4]], dtype=np.uint32).view(np.float32))
            elif name == "getTexture":
                result = texture(desc.getTextures()[args[0]])
            elif name == "get3DTexture":
                result = texture(desc.get3DTextures()[args[0]])
            elif name == "getNumDynamicProperties":
                result = len(desc.getDynamicProperties())
            elif name == "getDynamicProperty":
                prop = desc.getDynamicProperty(OCIO.DynamicPropertyType.__members__[args[0]])
                result = {"type": prop.getType().name}
            elif name == "getDynamicPropertyAt":
                result = {"type": desc.getDynamicProperties()[args[0]].getType().name}
            elif name == "clone":
                desc, result = desc.clone(), None
            else:
                result = getattr(desc, name)(*args)
            outcomes.append({"returned": result})
        except OCIO.Exception as e:
            outcomes.append({"raised": str(e)})
    state = {
        "cache_id": desc.getCacheID(), "text": desc.getShaderText(),
        "language": desc.getLanguage().name, "function_name": desc.getFunctionName(),
        "pixel_name": desc.getPixelName(), "resource_prefix": desc.getResourcePrefix(),
        "uid": desc.getUniqueID(), "descriptor_set_index": desc.getDescriptorSetIndex(),
        "texture_binding_start": desc.getTextureBindingStart(),
        "texture_max_width": desc.getTextureMaxWidth(),
        "allow_texture_1d": desc.getAllowTexture1D(),
        "uniform_buffer_size": desc.getUniformBufferSize(),
        "textures": [texture(t) for t in desc.getTextures()],
        "textures_3d": [texture(t) for t in desc.get3DTextures()],
    }
    print(json.dumps({"calls": outcomes, "state": state}))
"#;

fn f32_bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|v| v.to_bits()).collect()
}

fn texture_summary(t: &Texture) -> serde_json::Value {
    json!({"name": t.name, "sampler_name": t.sampler_name, "interpolation": t.interpolation,
           "binding_index": t.binding_index, "values": f32_bits(&t.values),
           "width": t.width, "height": t.height, "channel": t.channel,
           "dimensions": t.dimensions})
}

fn texture_3d_summary(t: &Texture3d) -> serde_json::Value {
    json!({"name": t.name, "sampler_name": t.sampler_name, "interpolation": t.interpolation,
           "binding_index": t.binding_index, "values": f32_bits(&t.values),
           "edge_len": t.edge_len})
}

/// The command's reply as `DESC_READ` prints its own read.
fn summary(reply: &GpuShaderDescReply) -> serde_json::Value {
    let calls: Vec<serde_json::Value> = reply
        .calls
        .iter()
        .map(|c| match c {
            DescOutcome::Raised(r) => json!({ "raised": r.message }),
            DescOutcome::Returned(r) => json!({ "returned": match r {
                Returned::Nothing => serde_json::Value::Null,
                Returned::Text(s) => json!(s),
                Returned::Number(n) => json!(n),
                Returned::Texture(t) => texture_summary(t),
                Returned::Texture3d(t) => texture_3d_summary(t),
                Returned::DynamicProperty(t) => json!({ "type": t }),
            }}),
        })
        .collect();
    let s = &reply.shader;
    let mut state = s.getters.clone();
    state["cache_id"] = json!(s.cache_id);
    state["text"] = json!(s.text);
    state["textures"] = s.textures.iter().map(texture_summary).collect();
    state["textures_3d"] = s.textures_3d.iter().map(texture_3d_summary).collect();
    for key in ["uniforms", "dynamic_properties"] {
        if let Some(object) = state.as_object_mut() {
            object.remove(key);
        }
    }
    json!({ "calls": calls, "state": state })
}

/// Every call, its outcome and the description's state agree with `DESC_READ`'s independent
/// read, bit for bit: the names with a NUL and double underscores, the cache ID before and
/// after `getNextResourceIndex`, the code sections in the 10 languages with
/// `createShaderText` and `finalize` (the OSL and Metal class wrappers included), a clone,
/// and textures with every error, the binding start added after the textures.
#[test]
fn calls_match_an_independent_read() {
    let red = |name: &str, width: u32, height: u32, values: Vec<f32>| AddTexture {
        name: name.into(),
        sampler_name: format!("{name}Sampler"),
        width,
        height,
        channel: "TEXTURE_RED_CHANNEL".into(),
        dimensions: "TEXTURE_1D".into(),
        interpolation: "INTERP_LINEAR".into(),
        values,
    };
    let ramp = |n: usize| -> Vec<f32> { (0..n).map(|i| i as f32 * 0.25 - 1.0).collect() };
    let code = |language: GpuLanguage| {
        vec![
            SetLanguage(language),
            SetFunctionName("F__1".into()),
            AddToParameterDeclareShaderCode("float p;\n".into()),
            AddToTextureDeclareShaderCode(
                "texture2d<float> t;\nsampler s;\n  // a comment\nint u[4];\n".into(),
            ),
            AddToHelperShaderCode("float h() { return 1.; }\n".into()),
            AddToFunctionHeaderShaderCode("float4 F_1(float4 inPixel)\n{\n".into()),
            AddToFunctionShaderCode("  inPixel.r = h();\n".into()),
            AddToFunctionFooterShaderCode("  return inPixel;\n}\n".into()),
            Finalize,
            GetShaderText,
            GetCacheId,
            CreateShaderText([
                "a\0b".into(),
                "t".into(),
                "h".into(),
                "fh".into(),
                "fb".into(),
                "ff".into(),
            ]),
            GetShaderText,
            GetCacheId,
        ]
    };
    let mut cases: Vec<Vec<DescCall>> = vec![
        vec![
            SetUniqueId("u\0v__w".into()),
            SetFunctionName("a__b___c\0d".into()),
            SetPixelName("p__x".into()),
            SetResourcePrefix("r__".into()),
            GetCacheId,
            GetNextResourceIndex,
            GetNextResourceIndex,
            GetCacheId,
            SetDescriptorSetIndex(7, 9),
            GetCacheId,
            Begin("x".into()),
            End,
            SetTextureMaxWidth(17),
            SetAllowTexture1d(false),
            Clone,
            GetCacheId,
            GetNextResourceIndex,
            GetCacheId,
            GetNumDynamicProperties,
            GetDynamicProperty("DYNAMIC_PROPERTY_GAMMA".into()),
            GetDynamicPropertyAt(0),
            GetDynamicPropertyAt(7),
        ],
        vec![
            red("r1", 5, 1, ramp(5)),
            red("r2", 4, 3, ramp(12)),
            red("wide", 4097, 1, ramp(4097)),
            red("", 1, 1, ramp(1)),
            AddTexture {
                name: "rgb".into(),
                sampler_name: String::new(),
                width: 2,
                height: 2,
                channel: "TEXTURE_RGB_CHANNEL".into(),
                dimensions: "TEXTURE_2D".into(),
                interpolation: "INTERP_NEAREST".into(),
                values: ramp(12),
            },
            red("zero", 0, 2, Vec::new()),
            red("high", 2, 0, Vec::new()),
            Add3dTexture {
                name: "c".into(),
                sampler_name: "cS".into(),
                edge_len: 3,
                interpolation: "INTERP_BEST".into(),
                values: ramp(81),
            },
            Add3dTexture {
                name: "e".into(),
                sampler_name: "eS".into(),
                edge_len: 0,
                interpolation: "INTERP_LINEAR".into(),
                values: Vec::new(),
            },
            SetDescriptorSetIndex(1, 40),
            GetTexture(0),
            GetTexture(1),
            GetTexture(2),
            Get3dTexture(0),
            Get3dTexture(1),
            SetTextureMaxWidth(3),
            red("narrow", 4, 1, ramp(4)),
            Clone,
            GetTexture(0),
        ],
    ];
    for language in GpuLanguage::ALL {
        cases.push(code(language));
    }

    let requests: Vec<GpuShaderDescRequest> =
        cases.into_iter().map(GpuShaderDescRequest::new).collect();
    // The independent read takes each request's calls, and its texture values as float bits,
    // in the order of the calls, as the request's blobs are.
    let script_cases: Vec<serde_json::Value> = requests
        .iter()
        .map(|r| {
            let blobs: Vec<Vec<u32>> = r
                .calls
                .iter()
                .filter_map(|c| match c {
                    AddTexture { values, .. } | Add3dTexture { values, .. } => {
                        Some(f32_bits(values))
                    }
                    _ => None,
                })
                .collect();
            json!({ "calls": r.args()["calls"], "blobs": blobs })
        })
        .collect();
    // The cases go in a file: a command line can't hold them on Windows.
    let path = ocio_testkit::paths::target_dir()
        .join(format!("gpu_desc_read_{}.json", std::process::id()));
    std::fs::write(&path, serde_json::to_string(&script_cases).expect("JSON"))
        .expect("the cases' file");
    let lines = Oracle::get().run_script(DESC_READ, &[path.display().to_string()]);
    std::fs::remove_file(&path).expect("the cases' file");
    assert_eq!(lines.len(), requests.len(), "{lines:?}");
    for (i, (reply, line)) in run(&requests).iter().zip(&lines).enumerate() {
        let expected: serde_json::Value = serde_json::from_str(line).expect("the script's JSON");
        assert_eq!(summary(reply), expected, "case {i}");
    }
}

/// The command refuses a request where the wheel would do something undefined, and nothing
/// next to it:
/// - a finalize in MSL whose declarations make the Metal class wrapper read past a line: a
///   texture declared last with a line feed after it, or followed by a line without `sampler`
///   shorter than 6 bytes, and a texture line past white space. A line of 6 bytes, a last line
///   without a line feed, a line with `sampler`, `texture` in a comment or not at a line's
///   start, and the same declarations in the other languages, go through;
/// - a finalize in MSL after one in MSL or OSL, whose declarations the command doesn't follow;
///   after one in another language, or a second finalize in OSL, it goes through;
/// - a 1D LUT's texture whose float count would wrap in a C unsigned;
/// - calls it doesn't know, arguments of the wrong kind or number, and texture values of the
///   wrong count.
#[test]
fn refusals_and_the_requests_next_to_them() {
    const READS_PAST: &str = "which the wheel's Metal class wrapper reads past";
    const NOT_FOLLOWED: &str = "after a finalize in MSL or OSL";
    const WRAPS: &str = "which wraps";
    let finalize = |language: GpuLanguage, declarations: &str| {
        vec![
            SetLanguage(language),
            AddToTextureDeclareShaderCode(declarations.into()),
            Finalize,
        ]
    };
    let msl = |declarations: &str| finalize(GpuLanguage::Msl20, declarations);
    let mut cases: Vec<(&str, Vec<DescCall>, Option<&str>)> = vec![
        (
            "a texture declared last",
            msl("texture2d<float> t;\n"),
            Some(READS_PAST),
        ),
        (
            "5 bytes after a texture",
            msl("texture1d<float> t;\n12345\n"),
            Some(READS_PAST),
        ),
        (
            "6 bytes after a texture",
            msl("texture1d<float> t;\n123456\n"),
            None,
        ),
        (
            "a texture past white space before an empty line",
            msl(" \t\u{b}\u{c}\rtexture3d<float> t;\n\n"),
            Some(READS_PAST),
        ),
        (
            "a last line without a line feed",
            msl("texture1d<float> t;"),
            None,
        ),
        (
            "a sampler after a texture",
            msl("texture2d<float> t;\nsampler s;\n"),
            None,
        ),
        (
            "a short sampler line",
            msl("texture2d<float> t;\nsampler\n"),
            None,
        ),
        (
            "texture in a comment",
            msl("// texture2d<float> t;\n"),
            None,
        ),
        ("texture after the start", msl("float texture2d;\n"), None),
        (
            "a texture line after a NUL, which ends the code",
            msl("float a;\0\ntexture2d<float> t;\n"),
            None,
        ),
        (
            "two finalizes in MSL",
            [msl("float p;\n"), vec![Finalize]].concat(),
            Some(NOT_FOLLOWED),
        ),
        (
            "a finalize in MSL after one in OSL",
            [
                finalize(GpuLanguage::Osl1, "float p;\n"),
                vec![SetLanguage(GpuLanguage::Msl20), Finalize],
            ]
            .concat(),
            Some(NOT_FOLLOWED),
        ),
        (
            "a finalize in MSL after one in GLSL",
            [
                finalize(GpuLanguage::Glsl40, "float p;\n"),
                vec![SetLanguage(GpuLanguage::Msl20), Finalize],
            ]
            .concat(),
            None,
        ),
        (
            "two finalizes in OSL",
            [finalize(GpuLanguage::Osl1, "float p;\n"), vec![Finalize]].concat(),
            None,
        ),
        (
            "a texture of 2^32 floats",
            vec![AddTexture {
                name: "t".into(),
                sampler_name: "s".into(),
                width: 65536,
                height: 65536,
                channel: "TEXTURE_RED_CHANNEL".into(),
                dimensions: "TEXTURE_2D".into(),
                interpolation: "INTERP_LINEAR".into(),
                values: Vec::new(),
            }],
            Some(WRAPS),
        ),
        (
            "texture values of the wrong count",
            vec![AddTexture {
                name: "t".into(),
                sampler_name: "s".into(),
                width: 2,
                height: 1,
                channel: "TEXTURE_RGB_CHANNEL".into(),
                dimensions: "TEXTURE_1D".into(),
                interpolation: "INTERP_LINEAR".into(),
                values: vec![0.5; 2],
            }],
            Some("needs 6 floats, not 2"),
        ),
    ];
    // The MSL declarations refused above, in the other languages.
    for language in GpuLanguage::ALL {
        if language != GpuLanguage::Msl20 {
            cases.push((
                "a texture declared last, not in MSL",
                finalize(language, "texture2d<float> t;\n"),
                None,
            ));
        }
    }

    let requests: Vec<GpuShaderDescRequest> = cases
        .iter()
        .map(|(_, calls, _)| GpuShaderDescRequest::new(calls.clone()))
        .collect();
    let mut calls: Vec<_> = requests.iter().map(GpuShaderDescRequest::call).collect();
    // Requests the typed builder can't write.
    let raw: [(&str, serde_json::Value, &str); 6] = [
        (
            "an unknown call",
            json!({"calls": [["getUniforms"]]}),
            "unknown call",
        ),
        (
            "a missing argument",
            json!({"calls": [["setFunctionName"]]}),
            "takes 1 arguments",
        ),
        (
            "a bool for an unsigned",
            json!({"calls": [["setTextureMaxWidth", true]]}),
            "must be an integer",
        ),
        (
            "a number for a string",
            json!({"calls": [["setPixelName", 3]]}),
            "must be a string",
        ),
        (
            "an unknown language",
            json!({"calls": [["setLanguage", "GPU_LANGUAGE_GLSL_9"]]}),
            "unknown GpuLanguage",
        ),
        (
            "a blob that isn't there",
            json!({"calls": [["add3DTexture", "t", "s", 1, "INTERP_LINEAR", 0]]}),
            "must be an integer",
        ),
    ];
    for (_, args, _) in &raw {
        calls.push(ocio_testkit::oracle::BatchCall {
            cmd: "gpu_shader_desc",
            args: args.clone(),
            blobs: Vec::new(),
        });
    }
    let expected = cases
        .iter()
        .map(|(label, _, refusal)| (*label, *refusal))
        .chain(
            raw.iter()
                .map(|(label, _, fragment)| (*label, Some(*fragment))),
        );
    for ((label, refusal), result) in expected.zip(Oracle::get().batch(&calls, false)) {
        match (refusal, result) {
            (Some(fragment), Err(e)) => assert!(e.contains(fragment), "{label}: {e}"),
            (None, Ok(response)) => {
                let reply = GpuShaderDescReply::from_response(response);
                for outcome in &reply.calls {
                    assert!(
                        matches!(outcome, DescOutcome::Returned(_)),
                        "{label}: {outcome:?}"
                    );
                }
            }
            (refusal, result) => panic!("{label}: expected {refusal:?}, got {result:?}"),
        }
    }
}
