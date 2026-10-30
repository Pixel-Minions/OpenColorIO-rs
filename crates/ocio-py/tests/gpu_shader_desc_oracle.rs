// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `GpuShaderCreator` and `GpuShaderDesc` with their nested classes (chunk 6.3h) against the
//! wheel:
//! - the two classes' surface, their nested classes' included;
//! - through the same step scripts on both sides: `CreateShaderDesc` (defaults, keywords, empty
//!   names left as they are), the creator's getters and setters (`None` as the empty string,
//!   bytes, text cut at a NUL, double underscores), the code sections, `createShaderText`,
//!   `finalize` (and the Metal wrapper's error), cache IDs, `clone` (of either class: a
//!   `GpuShaderDesc`), the descriptor set, the resource index, the dynamic properties of a
//!   description without any (and their iterator's errors), the 1D and 3D textures (values
//!   whose bits a copy would change, the binding's checks and their messages, a strided view
//!   read from its first item (I-328), the library's errors, the iterators, the `Texture`
//!   values and `getValues`), `UniformData` made in Python (its readwrite members, the getters
//!   it doesn't hold, its copy);
//! - a real Float3 uniform's getter before and after changing its retained GPU grading property;
//! - and where the wheel stores a dimension or an interpolation outside the enumerators, or a
//!   language, the port's error (U-112), the wheel running the same steps; where the wheel
//!   reads past a buffer (a reversed view), the port's error (U-148);
//! - textures read on other threads while a shader is extracted into their description (no
//!   error, the texture's values), within a time limit.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, by, compare, f, i, l, n, none, r, ra, s};
use pyo3::prelude::*;
use serde_json::{Value, json};

/// Each class has the wheel's surface, member for member, its nested classes' included.
#[test]
fn the_classes_have_the_wheels_surface() {
    let report = common::wheel_surface();
    for c in ["GpuShaderCreator", "GpuShaderDesc"] {
        let theirs = common::wheel_member(&report, c);
        let ours = common::port_member(c);
        assert!(
            theirs["members"].as_array().is_some_and(|m| m.len() > 15),
            "{c}: {theirs}"
        );
        assert_eq!(ours, theirs, "{c}");
    }
}

/// A NumPy array of `dtype`, `shape` and `bytes` (C order).
fn arr(dtype: &str, shape: &[usize], bytes: &[u8]) -> Value {
    json!({ "array": { "dtype": dtype, "shape": shape, "bytes": ocio_testkit::oracle_values::hex(bytes) } })
}

/// float32 bits that a copy through another type would change: NaN payloads (quiet and
/// signaling, both signs), infinities, signed zeros, subnormals, extremes, ordinary values.
const F32_BITS: [u32; 12] = [
    0x7fc0_0001,
    0x7fa0_0003,
    0xffc0_0005,
    0xff80_0007,
    0x7f80_0000,
    0xff80_0000,
    0x0000_0000,
    0x8000_0000,
    0x0000_0001,
    0x807f_ffff,
    0x3f80_0000,
    0x3dcc_cccd,
];

/// `n` float32 values, cycling through [`F32_BITS`] from `start`, as little-endian bytes.
fn floats(n: usize, start: usize) -> Vec<u8> {
    (0..n)
        .flat_map(|k| F32_BITS[(start + k) % F32_BITS.len()].to_le_bytes())
        .collect()
}

/// The creator's getters, which show a description's state.
const STATE: [&str; 10] = [
    "getUniqueID",
    "getLanguage",
    "getFunctionName",
    "getPixelName",
    "getResourcePrefix",
    "getDescriptorSetIndex",
    "getTextureBindingStart",
    "getTextureMaxWidth",
    "getAllowTexture1D",
    "getCacheID",
];

/// `CreateShaderDesc`, the creator's getters and setters, the code, `clone`.
fn creator_script() -> Script {
    let mut sc = Script::new();
    let d = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
    sc.eval(r(d), &STATE);
    sc.call(n("type"), vec![r(d)]);
    let d2 = sc.call_kw(
        n("OCIO.GpuShaderDesc.CreateShaderDesc"),
        vec![],
        vec![
            ("language", n("OCIO.GPU_LANGUAGE_MSL_2_0")),
            ("functionName", s("")),
            ("pixelName", by(b"px__name\0tail")),
            ("resourcePrefix", s("pre__fix")),
            ("uid", s("u1")),
        ],
        Expect::Value,
    );
    sc.eval(r(d2), &STATE);
    let d3 = sc.call(
        n("OCIO.GpuShaderDesc.CreateShaderDesc"),
        vec![
            n("OCIO.GPU_LANGUAGE_GLSL_VK_4_6"),
            s("fn"),
            s(""),
            s(""),
            s(""),
        ],
    );
    sc.eval(r(d3), &STATE);
    sc.raises(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![i(1)]);
    sc.raises(
        n("OCIO.GpuShaderDesc.CreateShaderDesc"),
        vec![n("OCIO.GPU_LANGUAGE_CG"), none()],
    );
    sc.call_kw(
        n("OCIO.GpuShaderDesc.CreateShaderDesc"),
        vec![],
        vec![("nope", i(1))],
        Expect::Raised,
    );

    // The setters: text, bytes, None (the empty string), NUL, double underscores.
    for (setter, values) in [
        ("setUniqueID", vec![s("a__b"), by(b"x\0y"), none(), s("")]),
        ("setFunctionName", vec![s("f__n___x"), none(), by(b"g")]),
        ("setPixelName", vec![s("p__"), none(), s("q")]),
        ("setResourcePrefix", vec![s("__r"), none(), s("pfx")]),
    ] {
        for v in values {
            sc.call(ra(d, setter), vec![v]);
            sc.eval(r(d), &STATE);
        }
        sc.raises(ra(d, setter), vec![i(3)]);
    }
    sc.call(ra(d, "setLanguage"), vec![n("OCIO.GPU_LANGUAGE_GLSL_1_3")]);
    sc.raises(ra(d, "setLanguage"), vec![i(2)]);
    sc.eval(r(d), &STATE);
    sc.call(ra(d, "setDescriptorSetIndex"), vec![i(2), i(10)]);
    sc.raises(ra(d, "setDescriptorSetIndex"), vec![i(1), i(0)]);
    sc.raises(ra(d, "setDescriptorSetIndex"), vec![i(-1), i(1)]);
    sc.raises(ra(d, "setDescriptorSetIndex"), vec![i(4294967296), i(1)]);
    sc.call_kw(
        ra(d, "setDescriptorSetIndex"),
        vec![],
        vec![("index", i(4294967295)), ("textureBindingStart", i(3))],
        Expect::Value,
    );
    sc.eval(r(d), &STATE);
    sc.call(ra(d, "setTextureMaxWidth"), vec![i(17)]);
    sc.raises(ra(d, "setTextureMaxWidth"), vec![i(-1)]);
    sc.call(ra(d, "setAllowTexture1D"), vec![b(false)]);
    sc.call(ra(d, "setAllowTexture1D"), vec![none()]);
    sc.eval(r(d), &STATE);
    for _ in 0..3 {
        sc.call(ra(d, "getNextResourceIndex"), vec![]);
    }
    // The cache ID was kept: the resource count isn't in it until a setter clears it.
    sc.call(ra(d, "getCacheID"), vec![]);
    sc.call(ra(d, "setUniqueID"), vec![s("again")]);
    sc.call(ra(d, "getCacheID"), vec![]);
    sc.call(ra(d, "begin"), vec![s("uid")]);
    sc.call(ra(d, "begin"), vec![none()]);
    sc.raises(ra(d, "begin"), vec![]);
    sc.call(ra(d, "end"), vec![]);

    // The code sections and the text.
    for (f, code) in [
        ("addToParameterDeclareShaderCode", "uniform float a;\n"),
        ("addToTextureDeclareShaderCode", "uniform sampler1D t;\n"),
        ("addToHelperShaderCode", "float h() { return 1.; }\n"),
        ("addToFunctionHeaderShaderCode", "vec4 F(vec4 c) {\n"),
        ("addToFunctionShaderCode", "  c.r = h();\n"),
        ("addToFunctionFooterShaderCode", "  return c;\n}\n"),
    ] {
        sc.call(ra(d, f), vec![s(code)]);
        sc.call(ra(d, f), vec![none()]);
        sc.call(ra(d, f), vec![by(b"/*x*/\0hidden")]);
        sc.call_kw(ra(d, f), vec![], vec![("shaderCode", s(""))], Expect::Value);
    }
    sc.call(ra(d, "getShaderText"), vec![]);
    sc.call(ra(d, "finalize"), vec![]);
    sc.call(ra(d, "getShaderText"), vec![]);
    sc.call(ra(d, "getCacheID"), vec![]);
    sc.call(
        ra(d, "createShaderText"),
        vec![s("P"), s("T"), none(), s("FH"), by(b"FB\0x"), s("FF")],
    );
    sc.call(ra(d, "getShaderText"), vec![]);
    sc.call(ra(d, "getCacheID"), vec![]);
    sc.raises(ra(d, "createShaderText"), vec![s("P")]);
    // Vulkan: the parameters in a uniform block.
    sc.call(
        ra(d3, "createShaderText"),
        vec![s("float x;"), s(""), s(""), s(""), s(""), s("")],
    );
    sc.call(ra(d3, "getShaderText"), vec![]);
    sc.call(
        ra(d3, "createShaderText"),
        vec![none(), s("T"), s(""), s(""), s(""), s("")],
    );
    sc.call(ra(d3, "getShaderText"), vec![]);
    // Metal: the class wrapper, of a function name starting with a digit too.
    sc.call(ra(d2, "addToFunctionShaderCode"), vec![s("  return x;\n")]);
    sc.call(ra(d2, "finalize"), vec![]);
    sc.call(ra(d2, "getShaderText"), vec![]);
    let d4 = sc.call(
        n("OCIO.GpuShaderDesc.CreateShaderDesc"),
        vec![n("OCIO.GPU_LANGUAGE_MSL_2_0"), s("9fn")],
    );
    sc.call(ra(d4, "finalize"), vec![]);
    sc.call(ra(d4, "getShaderText"), vec![]);
    let d5 = sc.call(
        n("OCIO.GpuShaderDesc.CreateShaderDesc"),
        vec![n("OCIO.LANGUAGE_OSL_1")],
    );
    sc.call(ra(d5, "finalize"), vec![]);
    sc.call(ra(d5, "getShaderText"), vec![]);

    // clone, of either class: a GpuShaderDesc of the creator's state.
    let c = sc.call(ra(d, "clone"), vec![]);
    sc.call(n("type"), vec![r(c)]);
    sc.eval(r(c), &STATE);
    sc.call(ra(c, "getShaderText"), vec![]);
    sc.call(n("operator.is_"), vec![r(c), r(d)]);
    let c2 = sc.call(n("OCIO.GpuShaderCreator.clone"), vec![r(d2)]);
    sc.call(n("type"), vec![r(c2)]);
    sc.eval(r(c2), &STATE);
    sc.raises(n("OCIO.GpuShaderCreator.clone"), vec![i(1)]);
    sc.raises(n("OCIO.GpuShaderCreator.setUniqueID"), vec![none(), s("x")]);
    sc.raises(n("OCIO.GpuShaderCreator"), vec![]);

    // Dynamic properties of a description without any.
    sc.call(
        ra(d, "hasDynamicProperty"),
        vec![n("OCIO.DYNAMIC_PROPERTY_EXPOSURE")],
    );
    let odd = sc.call(n("OCIO.DynamicPropertyType"), vec![i(77)]);
    sc.call(ra(d, "hasDynamicProperty"), vec![r(odd)]);
    sc.raises(
        ra(d, "getDynamicProperty"),
        vec![n("OCIO.DYNAMIC_PROPERTY_GAMMA")],
    );
    sc.raises(ra(d, "getDynamicProperty"), vec![r(odd)]);
    sc.raises(ra(d, "getDynamicProperty"), vec![i(0)]);
    let props = sc.call(ra(d, "getDynamicProperties"), vec![]);
    sc.call(n("len"), vec![r(props)]);
    sc.call(n("iter"), vec![r(props)]);
    sc.raises(ra(props, "__getitem__"), vec![i(0)]);
    sc.raises(ra(props, "__getitem__"), vec![i(-1)]);
    sc.raises(ra(props, "__getitem__"), vec![i(2147483648)]);
    sc.raises(n("next"), vec![r(props)]);
    sc.call(n("list"), vec![r(props)]);
    let uniforms = sc.call(ra(d, "getUniforms"), vec![]);
    sc.call(n("len"), vec![r(uniforms)]);
    sc.raises(ra(uniforms, "__getitem__"), vec![i(0)]);
    sc.raises(ra(uniforms, "__getitem__"), vec![i(-3)]);
    sc.raises(n("next"), vec![r(uniforms)]);
    sc.call(ra(d, "getUniformBufferSize"), vec![]);
    sc
}

/// The 1D and 3D textures.
fn textures_script() -> Script {
    let mut sc = Script::new();
    let d = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
    let red = n("OCIO.GpuShaderDesc.TEXTURE_RED_CHANNEL");
    let rgb = n("OCIO.GpuShaderDesc.TEXTURE_RGB_CHANNEL");
    let d1 = n("OCIO.GpuShaderDesc.TEXTURE_1D");
    let d2 = n("OCIO.GpuShaderDesc.TEXTURE_2D");
    let buf6 = sc.eval(arr("<f4", &[6], &floats(6, 0)), &[]);
    let buf2x9 = sc.eval(arr("<f4", &[2, 9], &floats(18, 3)), &[]);
    let added = |sc: &mut Script, args: Vec<Value>, expect: Expect| {
        sc.step(json!({ "call": ra(d, "addTexture"), "args": args }), expect)
    };
    added(
        &mut sc,
        vec![
            s("tex"),
            s("sampler"),
            i(2),
            i(3),
            red.clone(),
            d2.clone(),
            n("OCIO.INTERP_DEFAULT"),
            r(buf6),
        ],
        Expect::Value,
    );
    sc.call_kw(
        ra(d, "addTexture"),
        vec![],
        vec![
            ("textureName", by(b"t2\0x")),
            ("samplerName", s("s2")),
            ("width", i(3)),
            ("height", i(2)),
            ("channel", rgb.clone()),
            ("dimensions", d1.clone()),
            ("interpolation", n("OCIO.INTERP_NEAREST")),
            ("values", r(buf2x9)),
        ],
        Expect::Value,
    );
    // A view of every other value: read one after another from its first item (I-328).
    let every_other = sc.call(n("slice"), vec![none(), none(), i(2)]);
    let buf12 = sc.eval(arr("<f4", &[12], &floats(12, 5)), &[]);
    let view = sc.call(n("operator.getitem"), vec![r(buf12), r(every_other)]);
    added(
        &mut sc,
        vec![
            s("t3"),
            s("s3"),
            i(6),
            i(1),
            red.clone(),
            d1.clone(),
            n("OCIO.INTERP_LINEAR"),
            r(view),
        ],
        Expect::Value,
    );
    // The binding's checks, then the library's.
    let buf6_f8 = sc.eval(arr("<f8", &[6], &[0u8; 48]), &[]);
    let odd_channel = sc.call(n("OCIO.GpuShaderDesc.TextureType"), vec![i(5)]);
    for args in [
        // The channel's type, then the buffer's type and size.
        vec![
            s("x"),
            s("y"),
            i(2),
            i(3),
            r(odd_channel),
            d2.clone(),
            n("OCIO.INTERP_DEFAULT"),
            r(buf6),
        ],
        vec![
            s("x"),
            s("y"),
            i(2),
            i(3),
            red.clone(),
            d2.clone(),
            n("OCIO.INTERP_DEFAULT"),
            r(buf6_f8),
        ],
        vec![
            s("x"),
            s("y"),
            i(2),
            i(2),
            red.clone(),
            d2.clone(),
            n("OCIO.INTERP_DEFAULT"),
            r(buf6),
        ],
        vec![
            s("x"),
            s("y"),
            i(2),
            i(3),
            rgb.clone(),
            d2.clone(),
            n("OCIO.INTERP_DEFAULT"),
            r(buf6),
        ],
        // The library's: empty names, a size of 0, the width limit.
        vec![
            s(""),
            s("y"),
            i(2),
            i(3),
            red.clone(),
            d2.clone(),
            n("OCIO.INTERP_DEFAULT"),
            r(buf6),
        ],
        vec![
            s("x"),
            by(b"\0y"),
            i(2),
            i(3),
            red.clone(),
            d2.clone(),
            n("OCIO.INTERP_DEFAULT"),
            r(buf6),
        ],
        vec![
            s("x"),
            s("y"),
            i(5000),
            i(0),
            red.clone(),
            d2.clone(),
            n("OCIO.INTERP_DEFAULT"),
            r(buf6),
        ],
        vec![
            s("x"),
            s("y"),
            i(6),
            i(0),
            red.clone(),
            d2.clone(),
            n("OCIO.INTERP_DEFAULT"),
            r(buf6),
        ],
        // Not a buffer, and arguments missing.
        vec![
            s("x"),
            s("y"),
            i(2),
            i(3),
            red.clone(),
            d2.clone(),
            n("OCIO.INTERP_DEFAULT"),
            l(vec![]),
        ],
        vec![
            s("x"),
            s("y"),
            i(2),
            i(3),
            red.clone(),
            d2.clone(),
            n("OCIO.INTERP_DEFAULT"),
        ],
        vec![
            s("x"),
            s("y"),
            i(-2),
            i(3),
            red.clone(),
            d2.clone(),
            n("OCIO.INTERP_DEFAULT"),
            r(buf6),
        ],
    ] {
        added(&mut sc, args, Expect::Raised);
    }
    let empty = sc.eval(arr("<f4", &[0], &[]), &[]);
    added(
        &mut sc,
        vec![
            s("x"),
            s("y"),
            i(0),
            i(5),
            red.clone(),
            d2.clone(),
            n("OCIO.INTERP_DEFAULT"),
            r(empty),
        ],
        Expect::Raised,
    );
    sc.call(ra(d, "setTextureMaxWidth"), vec![i(2)]);
    added(
        &mut sc,
        vec![
            s("x"),
            s("y"),
            i(3),
            i(2),
            red.clone(),
            d2.clone(),
            n("OCIO.INTERP_DEFAULT"),
            r(buf6),
        ],
        Expect::Raised,
    );
    added(
        &mut sc,
        vec![
            s("x"),
            s("y"),
            i(2),
            i(3),
            red.clone(),
            d2.clone(),
            n("OCIO.INTERP_DEFAULT"),
            r(buf6),
        ],
        Expect::Value,
    );

    let textures = sc.call(ra(d, "getTextures"), vec![]);
    sc.call(n("len"), vec![r(textures)]);
    const TEXTURE: [&str; 9] = [
        "textureName",
        "samplerName",
        "width",
        "height",
        "channel",
        "dimensions",
        "interpolation",
        "textureShaderBindingIndex",
        "getValues",
    ];
    for k in [0, 1, 2, 3] {
        let t = sc.call(ra(textures, "__getitem__"), vec![i(k)]);
        sc.eval(r(t), &TEXTURE);
        sc.call(n("type"), vec![r(t)]);
    }
    sc.raises(ra(textures, "__getitem__"), vec![i(4)]);
    sc.raises(ra(textures, "__getitem__"), vec![i(-1)]);
    let t0 = sc.call(n("next"), vec![r(textures)]);
    sc.eval(r(t0), &TEXTURE);
    let t0again = sc.call(ra(textures, "__getitem__"), vec![i(0)]);
    sc.call(n("operator.is_"), vec![r(t0), r(t0again)]);
    sc.call(n("list"), vec![r(textures)]);
    sc.raises(n("next"), vec![r(textures)]);
    sc.call(n("iter"), vec![r(textures)]);
    sc.call(ra(t0, "getValues"), vec![]);
    sc.raises(n("setattr"), vec![r(t0), s("width"), i(1)]);
    sc.raises(n("OCIO.GpuShaderDesc.Texture"), vec![]);
    // The binding index follows the description's binding start.
    sc.call(ra(d, "setDescriptorSetIndex"), vec![i(1), i(7)]);
    let t1 = sc.call(ra(textures, "__getitem__"), vec![i(1)]);
    sc.eval(r(t1), &TEXTURE);
    sc.eval(r(t0), &TEXTURE);

    // 3D textures.
    let cube = sc.eval(arr("<f4", &[8, 3], &floats(24, 7)), &[]);
    let cube3 = sc.eval(arr("<f4", &[81], &floats(81, 2)), &[]);
    sc.call(
        ra(d, "add3DTexture"),
        vec![
            s("lut"),
            s("lutSampler"),
            i(2),
            n("OCIO.INTERP_TETRAHEDRAL"),
            r(cube),
        ],
    );
    sc.call_kw(
        ra(d, "add3DTexture"),
        vec![],
        vec![
            ("textureName", s("lut2")),
            ("samplerName", s("s")),
            ("edgeLen", i(3)),
            ("interpolation", n("OCIO.INTERP_LINEAR")),
            ("values", r(cube3)),
        ],
        Expect::Value,
    );
    for args in [
        vec![s("x"), s("y"), i(3), n("OCIO.INTERP_LINEAR"), r(cube)],
        vec![s("x"), s("y"), i(2), n("OCIO.INTERP_LINEAR"), r(buf6_f8)],
        vec![s(""), s("y"), i(2), n("OCIO.INTERP_LINEAR"), r(cube)],
        vec![s("x"), s(""), i(2), n("OCIO.INTERP_LINEAR"), r(cube)],
        vec![s("x"), s("y"), i(0), n("OCIO.INTERP_LINEAR"), r(empty)],
        vec![s("x"), s("y"), i(130), n("OCIO.INTERP_LINEAR"), r(cube)],
    ] {
        sc.raises(ra(d, "add3DTexture"), args);
    }
    let cubes = sc.call(ra(d, "get3DTextures"), vec![]);
    sc.call(n("len"), vec![r(cubes)]);
    const TEXTURE_3D: [&str; 6] = [
        "textureName",
        "samplerName",
        "edgeLen",
        "interpolation",
        "textureShaderBindingIndex",
        "getValues",
    ];
    for k in [0, 1] {
        let t = sc.call(ra(cubes, "__getitem__"), vec![i(k)]);
        sc.eval(r(t), &TEXTURE_3D);
        sc.call(n("type"), vec![r(t)]);
    }
    sc.raises(ra(cubes, "__getitem__"), vec![i(2)]);
    sc.raises(ra(cubes, "__getitem__"), vec![i(-2)]);
    let c0 = sc.call(n("next"), vec![r(cubes)]);
    sc.eval(r(c0), &TEXTURE_3D);
    sc.call(n("next"), vec![r(cubes)]);
    sc.raises(n("next"), vec![r(cubes)]);
    sc.call(ra(d, "getCacheID"), vec![]);
    sc.call(ra(d, "getNextResourceIndex"), vec![]);
    // A clone has none of the textures.
    let c = sc.call(ra(d, "clone"), vec![]);
    let ct = sc.call(ra(c, "getTextures"), vec![]);
    sc.call(n("len"), vec![r(ct)]);
    let c3 = sc.call(ra(c, "get3DTextures"), vec![]);
    sc.call(n("len"), vec![r(c3)]);
    sc
}

/// `UniformData` made in Python.
fn uniform_data_script() -> Script {
    let mut sc = Script::new();
    let u = sc.call(n("OCIO.GpuShaderDesc.UniformData"), vec![]);
    const STATE: [&str; 2] = ["type", "bufferOffset"];
    sc.eval(r(u), &STATE);
    for g in [
        "getDouble",
        "getBool",
        "getFloat3",
        "getVectorFloat",
        "getVectorInt",
    ] {
        sc.raises(ra(u, g), vec![]);
    }
    sc.step(
        json!({ "setattr": r(u), "name": "type", "value": n("OCIO.UNIFORM_BOOL") }),
        Expect::Value,
    );
    sc.step(
        json!({ "setattr": r(u), "name": "bufferOffset", "value": i(18446744073709551615) }),
        Expect::Value,
    );
    sc.eval(r(u), &STATE);
    let odd = sc.call(n("OCIO.UniformDataType"), vec![i(42)]);
    sc.step(
        json!({ "setattr": r(u), "name": "type", "value": r(odd) }),
        Expect::Value,
    );
    sc.eval(r(u), &STATE);
    sc.raises(ra(u, "getBool"), vec![]);
    for (name, value) in [
        ("type", i(1)),
        ("type", none()),
        ("bufferOffset", i(-1)),
        ("bufferOffset", i(18446744073709551616)),
        ("bufferOffset", s("1")),
    ] {
        sc.step(
            json!({ "setattr": r(u), "name": name, "value": value }),
            Expect::Raised,
        );
    }
    let copy = sc.call(n("OCIO.GpuShaderDesc.UniformData"), vec![r(u)]);
    sc.eval(r(copy), &STATE);
    let copy2 = sc.call_kw(
        n("OCIO.GpuShaderDesc.UniformData"),
        vec![],
        vec![("data", r(copy))],
        Expect::Value,
    );
    sc.step(
        json!({ "setattr": r(copy2), "name": "bufferOffset", "value": i(4) }),
        Expect::Value,
    );
    sc.eval(r(copy2), &STATE);
    sc.eval(r(copy), &STATE);
    sc.raises(n("OCIO.GpuShaderDesc.UniformData"), vec![none()]);
    sc.raises(n("OCIO.GpuShaderDesc.UniformData"), vec![i(1)]);
    sc.raises(n("OCIO.GpuShaderDesc.UniformData"), vec![r(u), r(u)]);
    let t1 = sc.eval(ra(u, "type"), &[]);
    let t2 = sc.eval(ra(u, "type"), &[]);
    sc.call(n("operator.is_"), vec![r(t1), r(t2)]);
    sc
}

/// Each class behaves as the wheel's.
#[test]
fn the_classes_behave_as_the_wheels() {
    compare(&[creator_script(), textures_script(), uniform_data_script()]);
}

/// A real Float3 getter reads its shader description's live property. GradingPrimary's LOG
/// shader adds brightness, contrast and gamma Float3 uniforms first, then six double uniforms
/// and its bypass bool (GradingPrimaryOpGPU.cpp:107-137 @ v2.5.2). The asymmetric brightness
/// input is GPTest1::brightness (tests/gpu/GradingPrimaryOp_test.cpp:10-13 @ v2.5.2); every
/// expected getter value still comes from the wheel, not from the port's computed values.
#[test]
fn a_float3_uniform_reads_the_live_property() {
    let checkpoint = |script: &Script| {
        let theirs = common::wheel(&[script.json()]);
        common::script::compare_with(std::slice::from_ref(script), &theirs);
        theirs[0]["steps"].clone()
    };
    let mut sc = Script::new();
    let cfg = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let tr = sc.call_kw(
        n("OCIO.GradingPrimaryTransform"),
        vec![n("OCIO.GRADING_LOG")],
        vec![("dynamic", b(true))],
        Expect::Value,
    );
    let desc = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
    // Compare the constructors before any step needs their returned objects.
    checkpoint(&sc);
    let processor = sc.call(ra(cfg, "getProcessor"), vec![r(tr)]);
    let gpu = sc.call(ra(processor, "getDefaultGPUProcessor"), vec![]);
    sc.call(ra(gpu, "extractGpuShaderInfo"), vec![r(desc)]);
    sc.eval(r(desc), &STATE);
    let text_before = sc.call(ra(desc, "getShaderText"), vec![]);
    let cache_before = sc.call(ra(desc, "getCacheID"), vec![]);
    let layout_before = sc.call(ra(desc, "getUniformBufferSize"), vec![]);
    sc.call(ra(desc, "getNextResourceIndex"), vec![]);
    let uniforms = sc.call(ra(desc, "getUniforms"), vec![]);
    let count_before = sc.call(n("len"), vec![r(uniforms)]);
    let setup = checkpoint(&sc);
    // Ten additions in AddGPLogProperties: three Float3, six double and one bool.
    assert_eq!(setup[count_before]["value"]["int"], "10");

    let first = sc.call(ra(uniforms, "__getitem__"), vec![i(0)]);
    sc.call(n("operator.getitem"), vec![r(first), i(0)]);
    let data = sc.call(n("operator.getitem"), vec![r(first), i(1)]);
    let metadata_before = sc.eval(r(data), &["type", "bufferOffset"]);
    let is_float3 = sc.call(
        n("operator.eq"),
        vec![ra(data, "type"), n("OCIO.UNIFORM_FLOAT3")],
    );
    for other in ["getDouble", "getBool", "getVectorFloat", "getVectorInt"] {
        sc.raises(ra(data, other), vec![]);
    }
    let initial = sc.call(ra(data, "getFloat3"), vec![]);
    // Do not reference a possibly failed getter result in a later transport step: first
    // require equality and its positive Value outcome at this complete prefix.
    let before = checkpoint(&sc);
    assert_eq!(before[is_float3]["value"]["bool"], true);
    assert_eq!(
        before[initial]["value"]["items"].as_array().unwrap().len(),
        3
    );

    let property = sc.call(
        ra(desc, "getDynamicProperty"),
        vec![n("OCIO.DYNAMIC_PROPERTY_GRADING_PRIMARY")],
    );
    let value = sc.call(ra(property, "getGradingPrimary"), vec![]);
    let brightness = sc.call(
        n("OCIO.GradingRGBM"),
        vec![f(-10.0), f(45.0), f(-5.0), f(50.0)],
    );
    sc.step(
        json!({ "setattr": r(value), "name": "brightness", "value": r(brightness) }),
        Expect::Value,
    );
    sc.call(ra(property, "setGradingPrimary"), vec![r(value)]);
    // Keep the original UniformData: replacing it could hide a stale captured getter.
    let updated = sc.call(ra(data, "getFloat3"), vec![]);
    let metadata_after = sc.eval(r(data), &["type", "bufferOffset"]);
    sc.eval(r(desc), &STATE);
    let text_after = sc.call(ra(desc, "getShaderText"), vec![]);
    let cache_after = sc.call(ra(desc, "getCacheID"), vec![]);
    let layout_after = sc.call(ra(desc, "getUniformBufferSize"), vec![]);
    sc.call(ra(desc, "getNextResourceIndex"), vec![]);
    let count_after = sc.call(n("len"), vec![r(uniforms)]);
    let after = checkpoint(&sc);
    let components = after[updated]["value"]["items"].as_array().unwrap();
    assert_eq!(components.len(), 3);
    assert_ne!(before[initial]["value"], after[updated]["value"]);
    assert_ne!(components[0], components[1]);
    assert_ne!(components[1], components[2]);
    assert_ne!(components[0], components[2]);
    // Only a dynamic value changed: compiled text/cache, layout, count and own UniformData
    // fields stay as the wheel reported them before the update. Full port/wheel reports,
    // including resource allocation, all creator state and wrong getters, were compared.
    for (old, new) in [
        (text_before, text_after),
        (cache_before, cache_after),
        (layout_before, layout_after),
        (count_before, count_after),
    ] {
        assert_eq!(before[old]["value"], after[new]["value"]);
    }
    assert_eq!(
        before[metadata_before]["getters"],
        after[metadata_after]["getters"]
    );
}

/// A language or an interpolation outside its enum's values (`GpuLanguage`, 0 to 15;
/// `Interpolation`, 0 to 255), which the wheel stores (and, for the language, reads through
/// its `switch`es later: `getCacheID` kills its process), is the port's error (U-112). The
/// wheel runs the same steps without one. (Inside the values, see
/// [`texture_values_inside_the_enums_are_the_wheels`].)
#[test]
fn an_enum_value_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let mut refused = Vec::new();
    let d = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
    let lang = sc.call(n("OCIO.GpuLanguage"), vec![i(99)]);
    refused.push(sc.call(ra(d, "setLanguage"), vec![r(lang)]));
    refused.push(sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![r(lang)]));
    let buf = sc.eval(arr("<f4", &[6], &floats(6, 0)), &[]);
    let interp = sc.call(n("OCIO.Interpolation"), vec![i(256)]);
    refused.push(sc.call(
        ra(d, "addTexture"),
        vec![
            s("t"),
            s("s"),
            i(6),
            i(1),
            n("OCIO.GpuShaderDesc.TEXTURE_RED_CHANNEL"),
            n("OCIO.GpuShaderDesc.TEXTURE_1D"),
            r(interp),
            r(buf),
        ],
    ));
    let cube = sc.eval(arr("<f4", &[24], &floats(24, 1)), &[]);
    refused.push(sc.call(
        ra(d, "add3DTexture"),
        vec![s("t"), s("s"), i(2), r(interp), r(cube)],
    ));
    assert!(sc.expect.iter().all(|e| *e == Expect::Value));
    let theirs = common::wheel(&[sc.json()]);
    let theirs = theirs[0]["steps"].as_array().unwrap();
    let ours = common::port(&sc.json());
    let ours = ours["steps"].as_array().unwrap();
    assert_eq!(theirs.len(), sc.steps.len());
    assert_eq!(ours.len(), sc.steps.len());
    assert_eq!(refused.len(), 4);
    for (k, (step, wheel_step)) in ours.iter().zip(theirs).enumerate() {
        assert!(wheel_step.get("value").is_some(), "step {k}: {wheel_step}");
        if refused.contains(&k) {
            let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
                &step["raised"]["message"],
            ))
            .unwrap();
            assert!(message.contains("U-112"), "step {k}: {step}");
            assert_eq!(step["raised"]["type"], "builtins.ValueError", "step {k}");
        } else {
            assert!(step.get("value").is_some(), "step {k}: {step}");
        }
    }
}

/// Where the wheel reads a texture's values past a buffer (a reversed view, read forward from
/// its last item), the port's error (U-148); the library's own checks come first, as the
/// wheel's do before it reads. The description stays usable.
#[test]
fn textures_past_the_buffer_are_the_ports_error() {
    let _turn = common::turn();
    pyo3::Python::attach(|py| {
        let code = std::ffi::CString::new(
            r#"
import PyOpenColorIO as OCIO, numpy as np

def refused(f, kind, entry):
    try:
        f()
    except kind as e:
        assert entry in str(e), str(e)
    else:
        raise AssertionError("no error")

d = OCIO.GpuShaderDesc.CreateShaderDesc()
rev = np.arange(6, dtype=np.float32)[::-1]
red, d2 = OCIO.GpuShaderDesc.TEXTURE_RED_CHANNEL, OCIO.GpuShaderDesc.TEXTURE_2D
refused(lambda: d.addTexture('t', 's', 2, 3, red, d2, OCIO.INTERP_LINEAR, rev),
        BufferError, "U-148")
refused(lambda: d.addTexture('', 's', 2, 3, red, d2, OCIO.INTERP_LINEAR, rev),
        OCIO.Exception, "The texture name is invalid.")
refused(lambda: d.add3DTexture('t', 's', 2, OCIO.INTERP_LINEAR,
                               np.arange(24, dtype=np.float32)[::-1]),
        BufferError, "U-148")
assert d.addTexture('t', 's', 2, 3, red, d2, OCIO.INTERP_LINEAR,
                    np.arange(6, dtype=np.float32)) == 1
assert len(d.getTextures()) == 1
assert list(d.getTextures()[0].getValues()) == [0, 1, 2, 3, 4, 5]
"#,
        )
        .unwrap();
        py.run(&code, None, None).unwrap_or_else(|e| panic!("{e}"));
    });
}

// ---------------------------------------------------------------------------------------------
// A UniformData whose property is gone (verifier 6.3h, H3; docs/improvements.md, U-196)

/// The wheel's steps, every object alive: a description of a dynamic exposure, its dynamic
/// properties and its clone's (none: `clone` copies no property), its uniform's `getDouble`
/// (the exposure), through a copy of the `UniformData` too, then after the description's
/// property is set through a `DynamicProperty` object of it.
fn uniform_lifetime_script() -> Script {
    let mut sc = Script::new();
    let cfg = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let ec = sc.call_kw(
        n("OCIO.ExposureContrastTransform"),
        vec![],
        vec![("exposure", f(0.75)), ("dynamicExposure", b(true))],
        Expect::Value,
    );
    let p = sc.call(ra(cfg, "getProcessor"), vec![r(ec)]);
    let gpu = sc.call(ra(p, "getDefaultGPUProcessor"), vec![]);
    let d = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
    sc.call(ra(gpu, "extractGpuShaderInfo"), vec![r(d)]);
    let dps = sc.call(ra(d, "getDynamicProperties"), vec![]);
    sc.call(n("len"), vec![r(dps)]);
    let c = sc.call(ra(d, "clone"), vec![]);
    let cdps = sc.call(ra(c, "getDynamicProperties"), vec![]);
    sc.call(n("len"), vec![r(cdps)]);
    let cu = sc.call(ra(c, "getUniforms"), vec![]);
    sc.call(n("len"), vec![r(cu)]);
    let u = sc.call(ra(d, "getUniforms"), vec![]);
    sc.call(n("len"), vec![r(u)]);
    let item = sc.call(ra(u, "__getitem__"), vec![i(0)]);
    let second = sc.call(n("operator.itemgetter"), vec![i(1)]);
    let data = sc.call(r(second), vec![r(item)]);
    let copy = sc.call(n("OCIO.GpuShaderDesc.UniformData"), vec![r(data)]);
    sc.call(ra(data, "getDouble"), vec![]);
    sc.call(ra(copy, "getDouble"), vec![]);
    let w = sc.call(
        ra(d, "getDynamicProperty"),
        vec![n("OCIO.DYNAMIC_PROPERTY_EXPOSURE")],
    );
    sc.call(ra(w, "setDouble"), vec![f(2.5)]);
    sc.call(ra(data, "getDouble"), vec![]);
    sc.call(ra(copy, "getDouble"), vec![]);
    // A dynamic exposure and a dynamic contrast: the second uniform reads the contrast.
    let ec2 = sc.call_kw(
        n("OCIO.ExposureContrastTransform"),
        vec![],
        vec![
            ("exposure", f(0.75)),
            ("contrast", f(1.25)),
            ("dynamicExposure", b(true)),
            ("dynamicContrast", b(true)),
        ],
        Expect::Value,
    );
    let p2 = sc.call(ra(cfg, "getProcessor"), vec![r(ec2)]);
    let gpu2 = sc.call(ra(p2, "getDefaultGPUProcessor"), vec![]);
    let d2 = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
    sc.call(ra(gpu2, "extractGpuShaderInfo"), vec![r(d2)]);
    let u2 = sc.call(ra(d2, "getUniforms"), vec![]);
    let item2 = sc.call(ra(u2, "__getitem__"), vec![i(1)]);
    let data2 = sc.call(r(second), vec![r(item2)]);
    sc.call(ra(data2, "getDouble"), vec![]);
    sc
}

/// The port's Python of the same steps, with the description gone before the reads, and kept
/// (or not) by what keeps its property in the wheel: a `DynamicProperty` object of it, the
/// uniform iterator (which holds the description); or by what doesn't: its clone, nothing.
/// Each read's outcome: `('ok', value)` or the exception's type and text.
const UNIFORM_LIFETIME: &std::ffi::CStr = c"
import gc
import PyOpenColorIO as OCIO

def outcome(f):
    try:
        return ('ok', f())
    except Exception as e:
        return (type(e).__name__, str(e))

def uniform_after(keep):
    t = OCIO.ExposureContrastTransform(exposure=0.75, dynamicExposure=True)
    gpu = OCIO.Config.CreateRaw().getProcessor(t).getDefaultGPUProcessor()
    d = OCIO.GpuShaderDesc.CreateShaderDesc()
    gpu.extractGpuShaderInfo(d)
    data = d.getUniforms()[0][1]
    copy = OCIO.GpuShaderDesc.UniformData(data)
    kept = {'wrapper': lambda: d.getDynamicProperty(OCIO.DYNAMIC_PROPERTY_EXPOSURE),
            'iterator': d.getUniforms, 'clone': d.clone, 'nothing': lambda: None,
            'wrapper gone': lambda: d.getDynamicProperty(OCIO.DYNAMIC_PROPERTY_EXPOSURE)}[keep]()
    if keep == 'wrapper gone':
        kept = None
    del d
    gc.collect()
    reads = [outcome(data.getDouble), outcome(copy.getDouble)]
    if keep == 'wrapper':
        kept.setDouble(2.5)
        reads += [outcome(data.getDouble), outcome(copy.getDouble)]
    return reads, data.type == OCIO.UNIFORM_DOUBLE, outcome(OCIO.GpuShaderDesc.UniformData().getDouble)[0]

def contrast_after():
    t = OCIO.ExposureContrastTransform(exposure=0.75, contrast=1.25, dynamicExposure=True,
                                       dynamicContrast=True)
    gpu = OCIO.Config.CreateRaw().getProcessor(t).getDefaultGPUProcessor()
    d = OCIO.GpuShaderDesc.CreateShaderDesc()
    gpu.extractGpuShaderInfo(d)
    exposure, contrast = d.getUniforms()[0][1], d.getUniforms()[1][1]
    kept = d.getDynamicProperty(OCIO.DYNAMIC_PROPERTY_CONTRAST)
    del d
    gc.collect()
    return outcome(contrast.getDouble), outcome(exposure.getDouble)[0]
";

/// A `UniformData` reads its description's dynamic property, which the wheel's getters reach
/// through a raw pointer (`std::bind(&DynamicPropertyDouble::getValue, prop.get())`,
/// ExposureContrastOpGPU.cpp:24-29 @ v2.5.2): defined while something holds the property, the
/// description (its iterator holds it) or a `DynamicProperty` object, and then the port's
/// reads are the wheel's (its steps, every object alive, give the values; a value set through
/// the `DynamicProperty` object shows). Once nothing does (the description's clone holds no
/// property: the wheel's own report), the wheel reads freed memory (U-196), and the port
/// raises its error, from the copy too; the value's own `type` stays.
#[test]
fn a_uniform_whose_property_is_gone_is_u196() {
    common::start_python();
    let sc = uniform_lifetime_script();
    let theirs = common::wheel(&[sc.json()]);
    common::script::compare_with(std::slice::from_ref(&sc), &theirs);
    let steps = theirs[0]["steps"].as_array().unwrap();
    let int = |k: usize| {
        steps[k]["value"]["int"]
            .as_str()
            .unwrap()
            .parse::<i64>()
            .unwrap()
    };
    let float = |k: usize| {
        let bits = steps[k]["value"]["float"].as_str().unwrap();
        f64::from_bits(u64::from_str_radix(bits.trim_start_matches("0x"), 16).unwrap())
    };
    // The description holds a property, its clone none; the uniform's reads.
    let n_steps = steps.len();
    assert_eq!(n_steps, 34);
    assert!(int(7) > 0, "the description's properties");
    assert_eq!(int(10), 0, "the clone's properties");
    assert_eq!(int(12), 0, "the clone's uniforms");
    assert!(int(14) > 0, "the description's uniforms");
    let (before, after) = (float(19), float(23));
    assert_eq!(float(20).to_bits(), before.to_bits(), "the copy's read");
    assert_eq!(
        float(24).to_bits(),
        after.to_bits(),
        "the copy's read after the set"
    );
    assert_ne!(before.to_bits(), after.to_bits());
    let _turn = common::turn();
    Python::attach(|py| {
        let globals = pyo3::types::PyDict::new(py);
        py.run(UNIFORM_LIFETIME, Some(&globals), None)
            .unwrap_or_else(|e| panic!("{e}"));
        // Only the contrast's object is kept: the contrast's uniform reads it, the exposure's
        // reads freed memory in the wheel.
        let got = py
            .eval(c"contrast_after()", Some(&globals), None)
            .unwrap_or_else(|e| panic!("{e}"));
        let ((kind, value), exposure): ((String, f64), String) = got.extract().unwrap();
        assert_eq!(kind, "ok");
        assert_eq!(
            value.to_bits(),
            float(33).to_bits(),
            "the contrast's uniform"
        );
        assert_eq!(exposure, "RuntimeError", "the exposure's uniform");
        for keep in ["wrapper", "iterator", "clone", "nothing", "wrapper gone"] {
            let code = std::ffi::CString::new(format!("uniform_after('{keep}')")).unwrap();
            let got = py
                .eval(&code, Some(&globals), None)
                .unwrap_or_else(|e| panic!("{keep}: {e}"));
            let (reads, double, default): (Vec<(String, Bound<'_, PyAny>)>, bool, String) =
                got.extract().unwrap();
            assert!(double, "{keep}: the value's own type");
            assert_eq!(default, "RuntimeError", "{keep}: a default UniformData");
            match keep {
                "wrapper" | "iterator" => {
                    let values: Vec<f64> = reads
                        .iter()
                        .map(|(kind, v)| {
                            assert_eq!(kind, "ok", "{keep}");
                            v.extract().unwrap()
                        })
                        .collect();
                    let expected: &[f64] = if keep == "wrapper" {
                        &[before, before, after, after]
                    } else {
                        &[before, before]
                    };
                    assert_eq!(values.len(), expected.len(), "{keep}");
                    for (v, e) in values.iter().zip(expected) {
                        assert_eq!(v.to_bits(), e.to_bits(), "{keep}: {values:?}");
                    }
                }
                _ => {
                    assert_eq!(reads.len(), 2, "{keep}");
                    for (kind, message) in &reads {
                        assert_eq!(kind, "RuntimeError", "{keep}");
                        let message: String = message.extract().unwrap();
                        assert!(message.contains("U-196"), "{keep}: {message}");
                        assert!(message.starts_with("GpuShaderDesc.UniformData.getDouble()"));
                    }
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------------------------
// The enums' values that aren't enumerators but are defined (verifier 6.3h, M1)

/// A texture's dimensions and interpolation outside the enumerators but inside the enums'
/// values (`TextureDimensions : uint8_t`, 0 to 255; `Interpolation`, without a fixed type, 0 to
/// 255): the wheel stores them, `getTextures` gives the interpolation as `???`, the dimensions
/// 0 too, and refuses more than two with the library's error ("1D LUT cannot have more than two
/// dimensions"), as does the port. A texture type outside its enumerators fails first, with the
/// binding's error, whatever the buffer (a float64 one here).
#[test]
fn texture_values_inside_the_enums_are_the_wheels() {
    let mut sc = Script::new();
    let buf = sc.eval(arr("<f4", &[4], &floats(4, 0)), &[]);
    let cube = sc.eval(arr("<f4", &[24], &floats(24, 1)), &[]);
    let getters = ["textureName", "width", "dimensions", "interpolation"];
    for dim in [0, 3, 255] {
        let d = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
        let dims = sc.call(n("OCIO.GpuShaderDesc.TextureDimensions"), vec![i(dim)]);
        sc.call(
            ra(d, "addTexture"),
            vec![
                s("t"),
                s("s"),
                i(4),
                i(1),
                n("OCIO.GpuShaderDesc.TEXTURE_RED_CHANNEL"),
                r(dims),
                n("OCIO.INTERP_LINEAR"),
                r(buf),
            ],
        );
        let it = sc.call(ra(d, "getTextures"), vec![]);
        sc.call(n("len"), vec![r(it)]);
        if dim > 2 {
            sc.raises(ra(it, "__getitem__"), vec![i(0)]);
            sc.raises(ra(it, "__next__"), vec![]);
        } else {
            let t = sc.call(ra(it, "__getitem__"), vec![i(0)]);
            sc.eval(r(t), &getters);
        }
        sc.call(ra(d, "getCacheID"), vec![]);
    }
    for interp in [5, 100, 253] {
        let d = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
        let value = sc.call(n("OCIO.Interpolation"), vec![i(interp)]);
        sc.call(
            ra(d, "addTexture"),
            vec![
                s("t"),
                s("s"),
                i(4),
                i(1),
                n("OCIO.GpuShaderDesc.TEXTURE_RED_CHANNEL"),
                n("OCIO.GpuShaderDesc.TEXTURE_2D"),
                r(value),
                r(buf),
            ],
        );
        sc.call(
            ra(d, "add3DTexture"),
            vec![s("t3"), s("s3"), i(2), r(value), r(cube)],
        );
        let it = sc.call(ra(d, "getTextures"), vec![]);
        let t = sc.call(ra(it, "__getitem__"), vec![i(0)]);
        sc.eval(r(t), &getters);
        let it = sc.call(ra(d, "get3DTextures"), vec![]);
        let t = sc.call(ra(it, "__getitem__"), vec![i(0)]);
        sc.eval(r(t), &["textureName", "edgeLen", "interpolation"]);
    }
    // A texture type outside the enumerators, with a float64 buffer: the binding's error.
    let d = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
    let ty = sc.call(n("OCIO.GpuShaderDesc.TextureType"), vec![i(2)]);
    let doubles = sc.eval(arr("<f8", &[4], &[0u8; 32]), &[]);
    sc.raises(
        ra(d, "addTexture"),
        vec![
            s("t"),
            s("s"),
            i(4),
            i(1),
            r(ty),
            n("OCIO.GpuShaderDesc.TEXTURE_1D"),
            n("OCIO.INTERP_LINEAR"),
            r(doubles),
        ],
    );
    let refused = sc.steps.len() - 1;
    let theirs = common::wheel(&[sc.json()]);
    let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
        &theirs[0]["steps"][refused]["raised"]["message"],
    ))
    .unwrap();
    assert_eq!(message, "Error: Unsupported texture type");
    common::script::compare_with(std::slice::from_ref(&sc), &theirs);
}

/// A language inside the enum's values (`GpuLanguage`: no fixed type, enumerators 0 to 9,
/// values 0 to 15) that is no enumerator: the wheel stores it (its `getLanguage` gives it
/// back; its `getCacheID` would end its process), the port raises D-15's error and keeps the
/// language it had.
#[test]
fn a_language_inside_the_enums_values_is_d15() {
    let mut sc = Script::new();
    let mut refused = Vec::new();
    let mut read = Vec::new();
    for v in [10, 15] {
        let d = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
        let lang = sc.call(n("OCIO.GpuLanguage"), vec![i(v)]);
        refused.push(sc.call(ra(d, "setLanguage"), vec![r(lang)]));
        read.push((v, sc.call(ra(d, "getLanguage"), vec![])));
        refused.push(sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![r(lang)]));
    }
    let theirs = common::wheel(&[sc.json()]);
    let theirs = theirs[0]["steps"].as_array().unwrap();
    let ours = common::port(&sc.json());
    let ours = ours["steps"].as_array().unwrap();
    assert_eq!(theirs.len(), sc.steps.len());
    assert_eq!(ours.len(), sc.steps.len());
    for (k, (step, wheel_step)) in ours.iter().zip(theirs).enumerate() {
        assert!(wheel_step.get("value").is_some(), "step {k}: {wheel_step}");
        if refused.contains(&k) {
            let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
                &step["raised"]["message"],
            ))
            .unwrap();
            assert!(message.contains("D-15"), "step {k}: {step}");
            assert_eq!(step["raised"]["type"], "builtins.ValueError", "step {k}");
        } else {
            assert!(step.get("value").is_some(), "step {k}: {step}");
        }
    }
    for (v, k) in read {
        // The wheel's description holds the value; the port's the one it had.
        assert_eq!(theirs[k]["value"]["enum"][1], json!(v), "{}", theirs[k]);
        assert_ne!(ours[k]["value"]["enum"][1], json!(v), "{}", ours[k]);
    }
}

// ---------------------------------------------------------------------------------------------
// Textures read on other threads while a shader is extracted into their description (verifier
// 6.3h, round 2)

/// The time a threaded script gets before the test calls it stuck.
const THREADED_LIMIT: std::time::Duration = std::time::Duration::from_secs(300);

/// The port's report of `script`, run on a thread of its own: a panic if it doesn't end within
/// [`THREADED_LIMIT`].
fn port_within_limit(script: Value) -> Value {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(|| common::port(&script));
        let _ = tx.send(result);
    });
    match rx.recv_timeout(THREADED_LIMIT) {
        Ok(Ok(report)) => report,
        Ok(Err(panic)) => std::panic::resume_unwind(panic),
        Err(_) => panic!("no end within {THREADED_LIMIT:?}"),
    }
}

/// The rounds of [`texture_readers_script`], and the extractions in each.
const READER_ROUNDS: usize = 200;
const EXTRACTIONS_PER_ROUND: i128 = 10;

/// The verifier's reproduction as a script, in [`READER_ROUNDS`] rounds. In each, a new
/// description holds a 2D texture (256 x 256 RGB texels) and a 3D one (48 texels a side), whose
/// copies take long enough for the other threads to wait for the port's description; three
/// threads read the 2D texture's values and two the 3D one's, in loops (`zip` of `iter(done.is_set, True)`
/// and the values' sixth float, `itemgetter(5)`, of each `getValues` call, into a `deque` that
/// keeps the last) that end when the round's event `done` is set; meanwhile the main thread
/// extracts a GPU processor without LUTs (it adds no texture) into the description
/// [`EXTRACTIONS_PER_ROUND`] times, through `json.dumps(..., indent=1,
/// default=extractGpuShaderInfo)` over a list of the description: the pure-Python encoder that
/// `indent` selects is a bytecode loop, so the interpreter switches threads between extractions
/// (every 10 us, `setswitchinterval`), as in the verifier's `for` loop. Each `getValues`
/// releases the GIL. An event signaled after the first successful value read makes startup
/// explicit in every round; Thread.start alone does not guarantee a sample before done.
/// A reader's exception goes to `threading.excepthook`, here a list's
/// `append`. The process's switch interval and excepthook are restored last (the port's
/// scripts share their test binary's interpreter). The script, the step that reads the errors'
/// count, and the steps that read the last round's readers' last values, each with the step
/// that made its texture's values.
///
/// The race is timing's: the port's readers that locked the description (before this fix)
/// failed in some rounds' first extractions. Measured on this machine (2026-10-25) with that
/// code put back in one reader kind at a time: the 2D texture's, 3 runs of the test failed out
/// of 4 (the readers' errors counted, 34 in one); the 3D texture's, 4 of 4.
fn texture_readers_script() -> (Script, usize, Vec<(usize, usize)>, Vec<usize>) {
    let mut sc = Script::new();
    let modules = n("threading._sys.modules");
    let module = |sc: &mut Script, name: &str| {
        sc.call(n("operator.getitem"), vec![modules.clone(), s(name)])
    };
    let itertools = module(&mut sc, "itertools");
    let collections = module(&mut sc, "collections");
    let json_module = module(&mut sc, "json");
    let interval = sc.call(n("threading._sys.getswitchinterval"), vec![]);
    sc.call(n("threading._sys.setswitchinterval"), vec![f(1e-5)]);
    let errors = sc.call(n("list"), vec![]);
    sc.step(
        json!({ "setattr": n("threading"), "name": "excepthook", "value": ra(errors, "append") }),
        Expect::Value,
    );

    let cfg = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let m = sc.call_kw(
        n("OCIO.MatrixTransform"),
        vec![],
        vec![("offset", l(vec![f(0.01), f(0.0), f(0.0), f(0.0)]))],
        Expect::Value,
    );
    let e = sc.call(
        n("OCIO.ExponentTransform"),
        vec![l(vec![f(1.1), f(1.2), f(1.3), f(1.0)])],
    );
    let lg = sc.call(n("OCIO.LogTransform"), vec![f(2.0)]);
    let g = sc.call(n("OCIO.GroupTransform"), vec![l(vec![r(m), r(e), r(lg)])]);
    let p = sc.call(
        ra(cfg, "getProcessor"),
        vec![r(g), n("OCIO.TRANSFORM_DIR_FORWARD")],
    );
    let gpu = sc.call(
        ra(p, "getOptimizedGPUProcessor"),
        vec![n("OCIO.OPTIMIZATION_NONE")],
    );
    let values = |sc: &mut Script, count: i128| {
        sc.call_kw(
            n("numpy.arange"),
            vec![i(count)],
            vec![("dtype", s("float32"))],
            Expect::Value,
        )
    };
    let buf = values(&mut sc, 256 * 256 * 3);
    let cube = values(&mut sc, 48 * 48 * 48 * 3);
    let no_args = sc.call(ra(itertools, "repeat"), vec![common::script::t(vec![])]);
    sc.unrepr(no_args);
    let sixth = sc.call(n("operator.itemgetter"), vec![i(5)]);
    let first = sc.call(n("operator.itemgetter"), vec![i(0)]);
    let mut ready_steps = Vec::new();

    let mut lasts = Vec::new();
    for _ in 0..READER_ROUNDS {
        let d = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
        sc.call(
            ra(d, "addTexture"),
            vec![
                s("t"),
                s("s"),
                i(256),
                i(256),
                n("OCIO.GpuShaderDesc.TEXTURE_RGB_CHANNEL"),
                n("OCIO.GpuShaderDesc.TEXTURE_2D"),
                n("OCIO.INTERP_LINEAR"),
                r(buf),
            ],
        );
        sc.call(
            ra(d, "add3DTexture"),
            vec![
                s("t3"),
                s("s3"),
                i(48),
                n("OCIO.INTERP_TETRAHEDRAL"),
                r(cube),
            ],
        );
        let textures = sc.call(ra(d, "getTextures"), vec![]);
        let tex = sc.call(n("operator.getitem"), vec![r(textures), i(0)]);
        let textures_3d = sc.call(ra(d, "get3DTextures"), vec![]);
        let tex3 = sc.call(n("operator.getitem"), vec![r(textures_3d), i(0)]);

        let done = sc.call(n("threading.Event"), vec![]);
        sc.unrepr(done);
        lasts.clear();
        let mut threads = Vec::new();
        let mut first_reads = Vec::new();
        for texture in [tex, tex, tex, tex3, tex3] {
            let first_read = sc.call(n("threading.Event"), vec![]);
            sc.unrepr(first_read);
            first_reads.push(first_read);
            let running = sc.call(n("iter"), vec![ra(done, "is_set"), b(true)]);
            sc.unrepr(running);
            let reads = sc.call(
                ra(itertools, "starmap"),
                vec![ra(texture, "getValues"), r(no_args)],
            );
            sc.unrepr(reads);
            let each = sc.call(n("zip"), vec![r(reads)]);
            sc.unrepr(each);
            let floats = sc.call(ra(itertools, "starmap"), vec![r(sixth), r(each)]);
            sc.unrepr(floats);
            // zip evaluates floats first: signal only after getValues and itemgetter succeed.
            let signals = sc.call(
                ra(itertools, "starmap"),
                vec![ra(first_read, "set"), r(no_args)],
            );
            sc.unrepr(signals);
            let after_read = sc.call(n("zip"), vec![r(floats), r(signals)]);
            sc.unrepr(after_read);
            let each_read = sc.call(n("zip"), vec![r(after_read)]);
            sc.unrepr(each_read);
            let floats = sc.call(ra(itertools, "starmap"), vec![r(first), r(each_read)]);
            sc.unrepr(floats);
            let reading = sc.call(n("zip"), vec![r(running), r(floats)]);
            sc.unrepr(reading);
            let last = sc.call_kw(
                ra(collections, "deque"),
                vec![],
                vec![("maxlen", i(1))],
                Expect::Value,
            );
            let thread = sc.call_kw(
                n("threading.Thread"),
                vec![],
                vec![
                    ("target", ra(last, "extend")),
                    ("args", common::script::t(vec![r(reading)])),
                ],
                Expect::Value,
            );
            sc.unrepr(thread);
            lasts.push(last);
            threads.push(thread);
        }
        for &thread in &threads {
            sc.call(ra(thread, "start"), vec![]);
        }
        // Thread.start does not promise a read. Every round must observe all five readers
        // before extraction can finish and set done; all original overlap/error/value checks stay.
        for first_read in first_reads {
            ready_steps.push(sc.call(ra(first_read, "wait"), vec![f(10.0)]));
        }
        let descs = sc.call(
            n("operator.mul"),
            vec![l(vec![r(d)]), i(EXTRACTIONS_PER_ROUND)],
        );
        sc.call_kw(
            ra(json_module, "dumps"),
            vec![r(descs)],
            vec![
                ("indent", i(1)),
                ("default", ra(gpu, "extractGpuShaderInfo")),
            ],
            Expect::Value,
        );
        sc.call(ra(done, "set"), vec![]);
        for &thread in &threads {
            sc.call(ra(thread, "join"), vec![]);
        }
    }
    sc.step(
        json!({
            "setattr": n("threading"),
            "name": "excepthook",
            "value": n("threading.__excepthook__"),
        }),
        Expect::Value,
    );
    sc.call(n("threading._sys.setswitchinterval"), vec![r(interval)]);
    let count = sc.call(n("len"), vec![r(errors)]);
    let read: Vec<(usize, usize)> = lasts
        .iter()
        .zip([buf, buf, buf, cube, cube])
        .map(|(&last, values)| (sc.call(n("list"), vec![r(last)]), values))
        .collect();
    (sc, count, read, ready_steps)
}

/// Upstream's `Texture.getValues` and `Texture3D.getValues` read the description with the GIL
/// released (PyGpuShaderDesc.cpp:259-285, 336-349 @ v2.5.2), while the main thread extracts a
/// shader into it; they read only its textures, which that extraction doesn't touch, and the
/// wheel's reads all succeed. The port's extraction holds the description outside its lock for
/// the call (so that a logging function can read it, H1 of round 1), and its readers, which
/// locked the description, saw an empty one: "1D LUT access error: index = 0 where size = 0".
/// Now each texture keeps its values. The same script on both sides, within a time limit: no
/// reader's error, the last round's readers' last value (the texture's sixth float).
#[test]
fn textures_read_during_an_extraction_are_the_wheels() {
    let (sc, count, read, ready_steps) = texture_readers_script();
    let theirs = common::wheel(&[sc.json()]).remove(0);
    let steps = theirs["steps"].as_array().unwrap();
    assert_eq!(steps.len(), sc.steps.len(), "{theirs}");
    for step in ready_steps {
        assert_eq!(
            steps[step]["value"]["bool"], true,
            "reader never started: {}",
            steps[step]
        );
    }
    assert_eq!(steps[count]["value"]["int"], "0", "{}", steps[count]);
    assert_eq!(read.len(), 5);
    for &(k, values) in &read {
        // `[(False, x)]`, x the sixth float of the values the texture was given: each reader
        // read at least once, the texture's values.
        let items = steps[k]["value"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 1, "step {k}: {}", steps[k]);
        let given = steps[values]["value"]["array"]["bytes"].as_str().unwrap();
        assert_eq!(
            items[0]["items"][1]["scalar"]["bytes"],
            given[40..48],
            "step {k}: {}",
            steps[k]
        );
    }
    let ours = port_within_limit(sc.json());
    common::script::compare_reports(std::slice::from_ref(&sc), &[ours], &[theirs]);
}
