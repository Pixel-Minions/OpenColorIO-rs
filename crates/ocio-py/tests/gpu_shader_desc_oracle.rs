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
//! - and where the wheel stores a dimension or an interpolation outside the enumerators, or a
//!   language, the port's error (U-112), the wheel running the same steps; where the wheel
//!   reads past a buffer (a reversed view), the port's error (U-148).
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, by, compare, i, l, n, none, r, ra, s};
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

/// A language, a texture dimension or an interpolation outside its enumerators, which the
/// wheel stores (and, for the language, reads through its `switch`es later: `getCacheID` kills
/// its process), is the port's error (U-112). The wheel runs the same steps without one.
#[test]
fn an_enum_value_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let mut refused = Vec::new();
    let d = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
    let lang = sc.call(n("OCIO.GpuLanguage"), vec![i(99)]);
    refused.push(sc.call(ra(d, "setLanguage"), vec![r(lang)]));
    refused.push(sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![r(lang)]));
    let buf = sc.eval(arr("<f4", &[6], &floats(6, 0)), &[]);
    let dims = sc.call(n("OCIO.GpuShaderDesc.TextureDimensions"), vec![i(7)]);
    let interp = sc.call(n("OCIO.Interpolation"), vec![i(42)]);
    refused.push(sc.call(
        ra(d, "addTexture"),
        vec![
            s("t"),
            s("s"),
            i(6),
            i(1),
            n("OCIO.GpuShaderDesc.TEXTURE_RED_CHANNEL"),
            r(dims),
            n("OCIO.INTERP_LINEAR"),
            r(buf),
        ],
    ));
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
    assert_eq!(refused.len(), 5);
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
