// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `Processor` and `GPUProcessor` (chunk 6.3h) against the wheel:
//! - the two classes' surface, the iterator class's included;
//! - through the same step scripts on both sides: a processor's getters (no-op, crosstalk,
//!   cache ID, dynamic), the objects it returns and their identity (the config's cached
//!   processor, its optimized, CPU and GPU processors, which its caches give again; its
//!   `ProcessorMetadata`, shared with the processors made from it, a change through one seen by
//!   the others), its format metadata and its ops' (`TransformFormatMetadataIterator`: length,
//!   indices past the end and negative, `next` to the end), `createGroupTransform`, its dynamic
//!   properties (of a type outside the enumerators too), the overloads of
//!   `getOptimizedProcessor` and `getOptimizedCPUProcessor`;
//! - the GPU processors' getters and `extractGpuShaderInfo`: shader texts in several languages,
//!   uniforms (doubles, and the int and float vectors of a dynamic RGB curve) whose values
//!   follow the dynamic properties, the 1D and 3D textures of a LUT file with Vulkan bindings;
//! - and, where the port deviates or the wheel's behaviour is undefined: a change through a
//!   processor's format metadata, which the wheel makes and the port refuses (D-13); a bit depth
//!   outside the enumerators (U-112); `extractGpuShaderInfo(None)` (U-195).
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, compare, f, i, n, none, r, ra, s};
use ocio_testkit::oracle_values::bytes;
use pyo3::prelude::*;
use serde_json::{Value, json};

/// Each class has the wheel's surface, member for member, its iterator class's included.
#[test]
fn the_classes_have_the_wheels_surface() {
    let report = common::wheel_surface();
    for c in ["Processor", "GPUProcessor"] {
        let theirs = common::wheel_member(&report, c);
        let ours = common::port_member(c);
        assert!(
            theirs["members"].as_array().is_some_and(|m| m.len() > 5),
            "{c}: {theirs}"
        );
        assert_eq!(ours, theirs, "{c}");
    }
}

/// A file of upstream's test data, as an absolute path both sides read.
fn data(file: &str) -> String {
    let path = ocio_testkit::paths::upstream_dir()
        .join("tests/data/files")
        .join(file);
    let path = path.to_string_lossy().replace('\\', "/");
    path.strip_prefix("//?/").unwrap_or(&path).to_string()
}

const STATE: [&str; 4] = ["isNoOp", "hasChannelCrosstalk", "getCacheID", "isDynamic"];
const GPU_STATE: [&str; 3] = ["isNoOp", "hasChannelCrosstalk", "getCacheID"];
const CPU_STATE: [&str; 4] = [
    "isNoOp",
    "getCacheID",
    "getInputBitDepth",
    "getOutputBitDepth",
];

/// A processor's getters, the objects it returns and their identity.
fn processor_script() -> Script {
    let mut sc = Script::new();
    let cfg = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let m = sc.call_kw(
        n("OCIO.MatrixTransform"),
        vec![],
        vec![("offset", common::script::fl(&[1.0, 1.0, 1.0, 0.0]))],
        Expect::Value,
    );
    let p = sc.call(ra(cfg, "getProcessor"), vec![r(m)]);
    sc.eval(r(p), &STATE);
    sc.call(n("type"), vec![r(p)]);
    // The config's cache gives the same processor.
    sc.call(ra(cfg, "getProcessor"), vec![r(m)]);
    let identity = sc.call(n("OCIO.MatrixTransform"), vec![]);
    let pi = sc.call(ra(cfg, "getProcessor"), vec![r(identity)]);
    sc.eval(r(pi), &STATE);

    // ProcessorMetadata: the same object, shared with the optimized processors.
    let pm = sc.call(ra(p, "getProcessorMetadata"), vec![]);
    sc.call(ra(p, "getProcessorMetadata"), vec![]);
    let files = sc.call(ra(pm, "getFiles"), vec![]);
    sc.call(n("list"), vec![r(files)]);
    let opt = sc.call(
        ra(p, "getOptimizedProcessor"),
        vec![n("OCIO.OPTIMIZATION_DEFAULT")],
    );
    sc.call(
        ra(p, "getOptimizedProcessor"),
        vec![n("OCIO.OPTIMIZATION_DEFAULT")],
    );
    sc.eval(r(opt), &STATE);
    let opm = sc.call(ra(opt, "getProcessorMetadata"), vec![]);
    sc.call(ra(pm, "addFile"), vec![s("x.clf")]);
    sc.call(ra(pm, "addLook"), vec![s("look")]);
    let files2 = sc.call(ra(opm, "getFiles"), vec![]);
    sc.call(n("list"), vec![r(files2)]);
    let looks = sc.call(ra(opm, "getLooks"), vec![]);
    sc.call(n("list"), vec![r(looks)]);
    let opt2 = sc.call(
        ra(p, "getOptimizedProcessor"),
        vec![n("OCIO.OPTIMIZATION_NONE")],
    );
    let opm2 = sc.call(ra(opt2, "getProcessorMetadata"), vec![]);
    let files3 = sc.call(ra(opm2, "getFiles"), vec![]);
    sc.call(n("list"), vec![r(files3)]);
    let pim = sc.call(ra(pi, "getProcessorMetadata"), vec![]);
    let files4 = sc.call(ra(pim, "getFiles"), vec![]);
    sc.call(n("list"), vec![r(files4)]);

    // Format metadata: the same object; a call that changes nothing.
    let fm = sc.call(ra(p, "getFormatMetadata"), vec![]);
    sc.call(ra(p, "getFormatMetadata"), vec![]);
    sc.repr(r(fm));
    sc.call(ra(fm, "clear"), vec![]);
    sc.repr(r(fm));
    let tfm = sc.call(ra(p, "getTransformFormatMetadata"), vec![]);
    sc.call(n("len"), vec![r(tfm)]);
    let t0 = sc.call(ra(tfm, "__getitem__"), vec![i(0)]);
    sc.call(ra(tfm, "__getitem__"), vec![i(0)]);
    sc.repr(r(t0));
    sc.raises(ra(tfm, "__getitem__"), vec![i(1)]);
    sc.raises(ra(tfm, "__getitem__"), vec![i(-1)]);
    sc.raises(ra(tfm, "__getitem__"), vec![s("0")]);
    sc.call(n("next"), vec![r(tfm)]);
    sc.raises(n("next"), vec![r(tfm)]);
    sc.call(n("iter"), vec![r(tfm)]);
    let it2 = sc.call(ra(pi, "getTransformFormatMetadata"), vec![]);
    sc.call(n("len"), vec![r(it2)]);
    sc.call(n("list"), vec![r(it2)]);
    sc.call(ra(it2, "__getitem__"), vec![i(0)]);
    sc.raises(ra(it2, "__getitem__"), vec![i(1)]);

    // createGroupTransform: a new group each time.
    let g = sc.call(ra(p, "createGroupTransform"), vec![]);
    sc.repr(r(g));
    sc.call(n("len"), vec![r(g)]);
    sc.call(ra(p, "createGroupTransform"), vec![]);
    let gi = sc.call(ra(pi, "createGroupTransform"), vec![]);
    sc.repr(r(gi));

    // Dynamic properties of a processor without any.
    for t in ["EXPOSURE", "CONTRAST", "GAMMA", "GRADING_PRIMARY"] {
        let ty = format!("OCIO.DYNAMIC_PROPERTY_{t}");
        sc.call(ra(p, "hasDynamicProperty"), vec![n(&ty)]);
        sc.raises(ra(p, "getDynamicProperty"), vec![n(&ty)]);
    }
    let odd = sc.call(n("OCIO.DynamicPropertyType"), vec![i(77)]);
    sc.call(ra(p, "hasDynamicProperty"), vec![r(odd)]);
    sc.raises(ra(p, "getDynamicProperty"), vec![r(odd)]);
    sc.raises(ra(p, "getDynamicProperty"), vec![i(0)]);

    // The overloads.
    let o3 = sc.call(
        ra(p, "getOptimizedProcessor"),
        vec![
            n("OCIO.BIT_DEPTH_UINT8"),
            n("OCIO.BIT_DEPTH_UINT10"),
            n("OCIO.OPTIMIZATION_LOSSLESS"),
        ],
    );
    sc.eval(r(o3), &STATE);
    sc.call_kw(
        ra(p, "getOptimizedProcessor"),
        vec![],
        vec![
            ("oFlags", n("OCIO.OPTIMIZATION_VERY_GOOD")),
            ("outBitDepth", n("OCIO.BIT_DEPTH_F16")),
            ("inBitDepth", n("OCIO.BIT_DEPTH_F32")),
        ],
        Expect::Value,
    );
    sc.raises(
        ra(p, "getOptimizedProcessor"),
        vec![n("OCIO.BIT_DEPTH_UINT8"), n("OCIO.BIT_DEPTH_UINT8")],
    );
    sc.raises(ra(p, "getOptimizedProcessor"), vec![i(0)]);
    sc.raises(ra(p, "getOptimizedProcessor"), vec![]);
    sc.raises(
        ra(p, "getOptimizedProcessor"),
        vec![
            n("OCIO.BIT_DEPTH_UINT14"),
            n("OCIO.BIT_DEPTH_UINT8"),
            n("OCIO.OPTIMIZATION_DEFAULT"),
        ],
    );

    // The GPU and CPU processors, which the processor's caches give again.
    let gpu = sc.call(ra(p, "getDefaultGPUProcessor"), vec![]);
    sc.call(ra(p, "getDefaultGPUProcessor"), vec![]);
    sc.eval(r(gpu), &GPU_STATE);
    sc.call(n("type"), vec![r(gpu)]);
    let gpu2 = sc.call(
        ra(p, "getOptimizedGPUProcessor"),
        vec![n("OCIO.OPTIMIZATION_NONE")],
    );
    sc.eval(r(gpu2), &GPU_STATE);
    sc.call(
        ra(p, "getOptimizedGPUProcessor"),
        vec![n("OCIO.OPTIMIZATION_DEFAULT")],
    );
    sc.raises(ra(p, "getOptimizedGPUProcessor"), vec![]);
    let gpui = sc.call(ra(pi, "getDefaultGPUProcessor"), vec![]);
    sc.eval(r(gpui), &GPU_STATE);
    let cpu = sc.call(ra(p, "getDefaultCPUProcessor"), vec![]);
    sc.call(ra(p, "getDefaultCPUProcessor"), vec![]);
    sc.eval(r(cpu), &CPU_STATE);
    let cpu2 = sc.call(
        ra(p, "getOptimizedCPUProcessor"),
        vec![n("OCIO.OPTIMIZATION_NONE")],
    );
    sc.eval(r(cpu2), &CPU_STATE);
    let cpu3 = sc.call(
        ra(p, "getOptimizedCPUProcessor"),
        vec![
            n("OCIO.BIT_DEPTH_UINT8"),
            n("OCIO.BIT_DEPTH_UINT16"),
            n("OCIO.OPTIMIZATION_DEFAULT"),
        ],
    );
    sc.eval(r(cpu3), &CPU_STATE);
    sc.raises(
        ra(p, "getOptimizedCPUProcessor"),
        vec![
            n("OCIO.BIT_DEPTH_UINT32"),
            n("OCIO.BIT_DEPTH_UINT8"),
            n("OCIO.OPTIMIZATION_DEFAULT"),
        ],
    );
    sc.raises(ra(p, "getOptimizedCPUProcessor"), vec![b(true)]);
    sc.raises(n("OCIO.Processor.isNoOp"), vec![i(1)]);
    sc.raises(n("OCIO.GPUProcessor.getCacheID"), vec![r(p)]);
    sc
}

/// The format metadata of a CLF file's processor (upstream's ProcessorTest
/// `test_format_meta_data`, read through every getter).
fn file_metadata_script() -> Script {
    let mut sc = Script::new();
    let cfg = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let ft = sc.call_kw(
        n("OCIO.FileTransform"),
        vec![],
        vec![("src", s(&data("clf/xyz_to_rgb.clf")))],
        Expect::Value,
    );
    let p0 = sc.call(ra(cfg, "getProcessor"), vec![r(ft)]);
    let pm = sc.call(ra(p0, "getProcessorMetadata"), vec![]);
    let files = sc.call(ra(pm, "getFiles"), vec![]);
    sc.call(n("list"), vec![r(files)]);
    let p = sc.call(
        ra(p0, "getOptimizedProcessor"),
        vec![n("OCIO.OPTIMIZATION_NONE")],
    );
    const MD: [&str; 6] = [
        "getID",
        "getName",
        "getElementName",
        "getElementValue",
        "__len__",
        "__repr__",
    ];
    let fm = sc.call(ra(p, "getFormatMetadata"), vec![]);
    sc.eval(r(fm), &MD);
    let children = sc.call(ra(fm, "getChildElements"), vec![]);
    sc.call(n("len"), vec![r(children)]);
    let c0 = sc.call(n("next"), vec![r(children)]);
    sc.eval(r(c0), &MD);
    let c0again = sc.call(ra(fm, "getChildElements"), vec![]);
    let c0b = sc.call(ra(c0again, "__getitem__"), vec![i(0)]);
    sc.call(n("operator.is_"), vec![r(c0), r(c0b)]);
    sc.raises(ra(c0again, "__getitem__"), vec![i(1)]);
    let attrs = sc.call(ra(fm, "getAttributes"), vec![]);
    sc.call(n("list"), vec![r(attrs)]);
    sc.call(n("list"), vec![r(fm)]);
    sc.call(ra(fm, "__getitem__"), vec![s("id")]);
    sc.call(ra(fm, "__contains__"), vec![s("name")]);
    sc.raises(ra(fm, "__getitem__"), vec![s("nope")]);
    // The library's errors of a ROOT element, which change nothing.
    sc.raises(ra(fm, "setElementName"), vec![s("Element")]);
    sc.raises(ra(fm, "setElementValue"), vec![s("value")]);
    sc.raises(ra(fm, "setName"), vec![none()]);
    // A set to the value it has, which changes nothing.
    sc.call(
        ra(c0, "setElementValue"),
        vec![s("Example with a Matrix and Lut1D")],
    );
    let tfm = sc.call(ra(p, "getTransformFormatMetadata"), vec![]);
    sc.call(n("len"), vec![r(tfm)]);
    for k in 0..3 {
        let t = sc.call(n("next"), vec![r(tfm)]);
        sc.eval(r(t), &MD);
        let ch = sc.call(ra(t, "getChildElements"), vec![]);
        sc.call(n("len"), vec![r(ch)]);
        if k != 1 {
            let c = sc.call(n("next"), vec![r(ch)]);
            sc.eval(r(c), &MD);
        }
    }
    sc.raises(n("next"), vec![r(tfm)]);
    let g = sc.call(ra(p, "createGroupTransform"), vec![]);
    sc.call(n("len"), vec![r(g)]);
    let gfm = sc.call(ra(g, "getFormatMetadata"), vec![]);
    sc.repr(r(gfm));
    let g0 = sc.call(n("operator.getitem"), vec![r(g), i(0)]);
    let g0fm = sc.call(ra(g0, "getFormatMetadata"), vec![]);
    sc.repr(r(g0fm));
    sc.call(n("operator.is_"), vec![r(gfm), r(fm)]);
    let t0 = sc.call(ra(p0, "getTransformFormatMetadata"), vec![]);
    sc.call(n("len"), vec![r(t0)]);
    sc.call(n("list"), vec![r(t0)]);
    sc
}

/// Dynamic properties: a processor's, its CPU processor's and a shader description's, whose
/// uniforms follow them.
fn dynamic_script() -> Script {
    let mut sc = Script::new();
    let cfg = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let ec = sc.call_kw(
        n("OCIO.ExposureContrastTransform"),
        vec![],
        vec![("exposure", f(1.25)), ("contrast", f(1.1))],
        Expect::Value,
    );
    let p = sc.call(ra(cfg, "getProcessor"), vec![r(ec)]);
    sc.eval(r(p), &STATE);
    sc.call(ra(ec, "makeContrastDynamic"), vec![]);
    let p = sc.call(ra(cfg, "getProcessor"), vec![r(ec)]);
    sc.eval(r(p), &STATE);
    sc.call(
        ra(p, "hasDynamicProperty"),
        vec![n("OCIO.DYNAMIC_PROPERTY_CONTRAST")],
    );
    sc.call(
        ra(p, "hasDynamicProperty"),
        vec![n("OCIO.DYNAMIC_PROPERTY_EXPOSURE")],
    );
    let dp = sc.call(
        ra(p, "getDynamicProperty"),
        vec![n("OCIO.DYNAMIC_PROPERTY_CONTRAST")],
    );
    sc.call(
        ra(p, "getDynamicProperty"),
        vec![n("OCIO.DYNAMIC_PROPERTY_CONTRAST")],
    );
    sc.eval(r(dp), &["getType", "getDouble"]);
    sc.raises(
        ra(p, "getDynamicProperty"),
        vec![n("OCIO.DYNAMIC_PROPERTY_EXPOSURE")],
    );
    sc.call(ra(dp, "setDouble"), vec![f(1.15)]);
    let dp2 = sc.call(
        ra(p, "getDynamicProperty"),
        vec![n("OCIO.DYNAMIC_PROPERTY_CONTRAST")],
    );
    sc.eval(r(dp2), &["getDouble"]);
    sc.raises(ra(dp, "getGradingPrimary"), vec![]);
    let cpu = sc.call(ra(p, "getDefaultCPUProcessor"), vec![]);
    sc.eval(r(cpu), &CPU_STATE);
    let opt = sc.call(
        ra(p, "getOptimizedProcessor"),
        vec![n("OCIO.OPTIMIZATION_NO_DYNAMIC_PROPERTIES")],
    );
    sc.eval(r(opt), &STATE);

    // Uniforms of a dynamic exposure and gamma (upstream's GpuShaderDescTest test_uniform).
    let ec2 = sc.call_kw(
        n("OCIO.ExposureContrastTransform"),
        vec![],
        vec![("dynamicExposure", b(true)), ("dynamicGamma", b(true))],
        Expect::Value,
    );
    let p2 = sc.call(ra(cfg, "getProcessor"), vec![r(ec2)]);
    let gpu = sc.call(ra(p2, "getDefaultGPUProcessor"), vec![]);
    sc.eval(r(gpu), &GPU_STATE);
    for lang in [
        "GLSL_1_2",
        "GLSL_4_0",
        "HLSL_DX11",
        "MSL_2_0",
        "GLSL_VK_4_6",
        "CG",
    ] {
        let desc = sc.call(
            n("OCIO.GpuShaderDesc.CreateShaderDesc"),
            vec![n(&format!("OCIO.GPU_LANGUAGE_{lang}"))],
        );
        sc.call(ra(gpu, "extractGpuShaderInfo"), vec![r(desc)]);
        sc.eval(
            r(desc),
            &["getShaderText", "getCacheID", "getUniformBufferSize"],
        );
    }
    let desc = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
    sc.call_kw(
        ra(gpu, "extractGpuShaderInfo"),
        vec![],
        vec![("shaderDesc", r(desc))],
        Expect::Value,
    );
    let uniforms = sc.call(ra(desc, "getUniforms"), vec![]);
    sc.call(n("len"), vec![r(uniforms)]);
    let u0 = sc.call(ra(uniforms, "__getitem__"), vec![i(0)]);
    const UD: [&str; 3] = ["type", "bufferOffset", "getDouble"];
    let ud0 = sc.call(n("operator.getitem"), vec![r(u0), i(1)]);
    sc.eval(r(ud0), &UD);
    sc.raises(ra(ud0, "getBool"), vec![]);
    sc.raises(ra(ud0, "getVectorFloat"), vec![]);
    let u1 = sc.call(ra(uniforms, "__getitem__"), vec![i(1)]);
    let ud1 = sc.call(n("operator.getitem"), vec![r(u1), i(1)]);
    sc.eval(r(ud1), &UD);
    sc.raises(ra(uniforms, "__getitem__"), vec![i(2)]);
    sc.call(n("list"), vec![r(uniforms)]);
    let props = sc.call(ra(desc, "getDynamicProperties"), vec![]);
    sc.call(n("len"), vec![r(props)]);
    let pr0 = sc.call(ra(props, "__getitem__"), vec![i(0)]);
    sc.eval(r(pr0), &["getType", "getDouble"]);
    sc.call(
        ra(desc, "hasDynamicProperty"),
        vec![n("OCIO.DYNAMIC_PROPERTY_GAMMA")],
    );
    let dexp = sc.call(
        ra(desc, "getDynamicProperty"),
        vec![n("OCIO.DYNAMIC_PROPERTY_EXPOSURE")],
    );
    sc.call(ra(dexp, "setDouble"), vec![f(2.0)]);
    sc.eval(r(ud0), &UD);
    let dgam = sc.call(
        ra(desc, "getDynamicProperty"),
        vec![n("OCIO.DYNAMIC_PROPERTY_GAMMA")],
    );
    sc.call(ra(dgam, "setDouble"), vec![f(0.5)]);
    sc.eval(r(ud1), &UD);
    // The processor's own property isn't the description's.
    let pexp = sc.call(
        ra(p2, "getDynamicProperty"),
        vec![n("OCIO.DYNAMIC_PROPERTY_EXPOSURE")],
    );
    sc.eval(r(pexp), &["getDouble"]);
    let copy = sc.call(n("OCIO.GpuShaderDesc.UniformData"), vec![r(ud0)]);
    sc.call(ra(dexp, "setDouble"), vec![f(-3.5)]);
    sc.eval(r(copy), &UD);
    // A second extraction into the same description.
    sc.raises(ra(gpu, "extractGpuShaderInfo"), vec![r(desc)]);

    // The vector uniforms of a dynamic RGB curve (upstream's test_vector_uniform).
    let curve = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![common::script::fl(&[1.0, 2.0, 3.0, 4.0])],
    );
    let rgb = sc.call(
        n("OCIO.GradingRGBCurve"),
        vec![r(curve), r(curve), r(curve), r(curve)],
    );
    let tr = sc.call_kw(
        n("OCIO.GradingRGBCurveTransform"),
        vec![],
        vec![("values", r(rgb)), ("dynamic", b(true))],
        Expect::Value,
    );
    let p3 = sc.call(ra(cfg, "getProcessor"), vec![r(tr)]);
    let gpu3 = sc.call(ra(p3, "getDefaultGPUProcessor"), vec![]);
    let desc3 = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
    sc.call(ra(gpu3, "extractGpuShaderInfo"), vec![r(desc3)]);
    sc.eval(r(desc3), &["getShaderText", "getUniformBufferSize"]);
    let us = sc.call(ra(desc3, "getUniforms"), vec![]);
    sc.call(n("len"), vec![r(us)]);
    let mut datas = Vec::new();
    for k in 0..5 {
        let u = sc.call(ra(us, "__getitem__"), vec![i(k)]);
        let ud = sc.call(n("operator.getitem"), vec![r(u), i(1)]);
        sc.call(n("operator.getitem"), vec![r(u), i(0)]);
        sc.step(
            json!({ "eval": r(ud), "getters": ["type", "bufferOffset"] }),
            Expect::Value,
        );
        datas.push(ud);
    }
    sc.call(ra(datas[0], "getVectorInt"), vec![]);
    sc.call(ra(datas[1], "getVectorFloat"), vec![]);
    sc.raises(ra(datas[1], "getVectorInt"), vec![]);
    sc.raises(ra(datas[0], "getVectorFloat"), vec![]);
    sc.call(ra(datas[2], "getVectorInt"), vec![]);
    sc.call(ra(datas[3], "getVectorFloat"), vec![]);
    sc.call(ra(datas[4], "getBool"), vec![]);
    sc.raises(ra(datas[4], "getDouble"), vec![]);
    let curve2 = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![common::script::fl(&[5.0, 6.0, 7.0, 8.0])],
    );
    let rgb2 = sc.call(
        n("OCIO.GradingRGBCurve"),
        vec![r(curve2), r(curve2), r(curve2), r(curve2)],
    );
    let dyn_curve = sc.call(
        ra(desc3, "getDynamicProperty"),
        vec![n("OCIO.DYNAMIC_PROPERTY_GRADING_RGBCURVE")],
    );
    sc.call(ra(dyn_curve, "setGradingRGBCurve"), vec![r(rgb2)]);
    sc.call(ra(datas[1], "getVectorFloat"), vec![]);
    sc.call(ra(datas[0], "getVectorInt"), vec![]);
    sc
}

/// The 1D and 3D textures of a LUT file, with Vulkan bindings (upstream's test_vulkan).
fn textures_script() -> Script {
    let mut sc = Script::new();
    let cfg = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let ft = sc.call_kw(
        n("OCIO.FileTransform"),
        vec![],
        vec![("src", s(&data("clf/lut1d_lut3d_lut1d.clf")))],
        Expect::Value,
    );
    let p = sc.call(ra(cfg, "getProcessor"), vec![r(ft)]);
    let desc = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
    sc.call(ra(desc, "setDescriptorSetIndex"), vec![i(2), i(10)]);
    let gpu = sc.call(ra(p, "getDefaultGPUProcessor"), vec![]);
    sc.eval(r(gpu), &GPU_STATE);
    sc.call(ra(gpu, "extractGpuShaderInfo"), vec![r(desc)]);
    sc.eval(
        r(desc),
        &["getShaderText", "getCacheID", "getNextResourceIndex"],
    );
    const T1: [&str; 9] = [
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
    let textures = sc.call(ra(desc, "getTextures"), vec![]);
    sc.call(n("len"), vec![r(textures)]);
    for _ in 0..2 {
        let t = sc.call(n("next"), vec![r(textures)]);
        sc.eval(r(t), &T1);
    }
    let cubes = sc.call(ra(desc, "get3DTextures"), vec![]);
    sc.call(n("len"), vec![r(cubes)]);
    let c = sc.call(n("next"), vec![r(cubes)]);
    sc.eval(
        r(c),
        &[
            "textureName",
            "samplerName",
            "edgeLen",
            "interpolation",
            "textureShaderBindingIndex",
            "getValues",
        ],
    );
    // 2D textures only, a width limit that fits the LUTs (one that doesn't is U-5), and
    // other languages.
    let desc2 = sc.call(n("OCIO.GpuShaderDesc.CreateShaderDesc"), vec![]);
    sc.call(ra(desc2, "setAllowTexture1D"), vec![b(false)]);
    sc.call(ra(desc2, "setTextureMaxWidth"), vec![i(8192)]);
    sc.call(ra(gpu, "extractGpuShaderInfo"), vec![r(desc2)]);
    sc.eval(r(desc2), &["getShaderText", "getCacheID"]);
    let t2 = sc.call(ra(desc2, "getTextures"), vec![]);
    let first = sc.call(n("next"), vec![r(t2)]);
    sc.eval(r(first), &["width", "height", "dimensions"]);
    for lang in ["HLSL_DX11", "MSL_2_0", "GLSL_ES_3_0"] {
        let d = sc.call(
            n("OCIO.GpuShaderDesc.CreateShaderDesc"),
            vec![n(&format!("OCIO.GPU_LANGUAGE_{lang}"))],
        );
        sc.call(ra(gpu, "extractGpuShaderInfo"), vec![r(d)]);
        sc.eval(r(d), &["getShaderText", "getCacheID"]);
    }
    sc.raises(ra(gpu, "extractGpuShaderInfo"), vec![i(1)]);
    sc.raises(ra(gpu, "extractGpuShaderInfo"), vec![]);
    sc.raises(ra(gpu, "extractGpuShaderInfo"), vec![r(gpu)]);
    sc
}

/// Each class behaves as the wheel's.
#[test]
fn the_classes_behave_as_the_wheels() {
    common::start_python();
    compare(&[
        processor_script(),
        file_metadata_script(),
        dynamic_script(),
        textures_script(),
    ]);
}

/// The text of a step's exception, from its report.
fn message(step: &Value) -> String {
    String::from_utf8(bytes(&step["raised"]["message"])).unwrap()
}

/// A change through a processor's format metadata: the wheel makes it (the metadata reads
/// otherwise after), the port raises D-13's error and the metadata reads as before, as the
/// wheel's did. Each change of each `FormatMetadata` setter, on the processor's metadata, an
/// op's, and a child element.
#[test]
fn a_change_of_a_processors_metadata_is_d13() {
    let changes: [(&str, Vec<Value>); 7] = [
        ("setName", vec![s("renamed")]),
        ("setID", vec![s("an-id")]),
        ("setElementName", vec![s("Element")]),
        ("setElementValue", vec![s("value")]),
        ("__setitem__", vec![s("attr"), s("v")]),
        ("addChildElement", vec![s("Child"), s("text")]),
        ("clear", vec![]),
    ];
    let mut scripts = Vec::new();
    // (the reference's step, the change's step) of each script.
    let mut marks = Vec::new();
    for (method, args) in &changes {
        for target in ["processor", "op", "child"] {
            // A ROOT element can't be renamed nor have a value (the library's errors, as the
            // wheel's: `file_metadata_script`).
            if (*method == "setElementName" || *method == "setElementValue") && target != "child" {
                continue;
            }
            let mut sc = Script::new();
            let cfg = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
            let ft = sc.call_kw(
                n("OCIO.FileTransform"),
                vec![],
                vec![("src", s(&data("clf/xyz_to_rgb.clf")))],
                Expect::Value,
            );
            let p0 = sc.call(ra(cfg, "getProcessor"), vec![r(ft)]);
            let p = sc.call(
                ra(p0, "getOptimizedProcessor"),
                vec![n("OCIO.OPTIMIZATION_NONE")],
            );
            let md = match target {
                "processor" => sc.call(ra(p, "getFormatMetadata"), vec![]),
                "op" => {
                    let it = sc.call(ra(p, "getTransformFormatMetadata"), vec![]);
                    sc.call(ra(it, "__getitem__"), vec![i(0)])
                }
                _ => {
                    let fm = sc.call(ra(p, "getFormatMetadata"), vec![]);
                    let ch = sc.call(ra(fm, "getChildElements"), vec![]);
                    sc.call(ra(ch, "__getitem__"), vec![i(0)])
                }
            };
            let before = sc.repr(r(md));
            let change = sc.call(ra(md, method), args.clone());
            let after = sc.repr(r(md));
            let again = match target {
                "processor" => sc.call(ra(p, "getFormatMetadata"), vec![]),
                "op" => {
                    let it = sc.call(ra(p, "getTransformFormatMetadata"), vec![]);
                    sc.call(ra(it, "__getitem__"), vec![i(0)])
                }
                _ => {
                    let fm = sc.call(ra(p, "getFormatMetadata"), vec![]);
                    let ch = sc.call(ra(fm, "getChildElements"), vec![]);
                    sc.call(ra(ch, "__getitem__"), vec![i(0)])
                }
            };
            let again_repr = sc.repr(r(again));
            scripts.push(sc);
            marks.push((before, change, after, again_repr));
        }
    }
    assert_eq!(scripts.len(), 17);
    let json: Vec<Value> = scripts.iter().map(Script::json).collect();
    let theirs = common::wheel(&json);
    for (k, (sc, (before, change, after, again))) in scripts.iter().zip(&marks).enumerate() {
        let ours = common::port(&sc.json());
        let (ours, theirs) = (
            ours["steps"].as_array().unwrap(),
            theirs[k]["steps"].as_array().unwrap(),
        );
        assert_eq!(theirs.len(), sc.steps.len(), "script {k}");
        assert_eq!(ours.len(), sc.steps.len(), "script {k}");
        // Every step succeeds in the wheel, and the change shows.
        assert!(
            theirs.iter().all(|t| t.get("value").is_some()),
            "script {k}: {theirs:?}"
        );
        assert_ne!(
            theirs[*before]["value"]["str"], theirs[*after]["value"]["str"],
            "script {k}: the wheel's change"
        );
        assert_eq!(
            theirs[*after]["value"]["str"], theirs[*again]["value"]["str"],
            "script {k}"
        );
        // The port reads as the wheel up to the change, refuses it, and reads as before.
        for j in 0..*change {
            assert_eq!(ours[j], theirs[j], "script {k} step {j}");
        }
        assert!(
            message(&ours[*change]).contains("D-13"),
            "script {k}: {}",
            ours[*change]
        );
        assert_eq!(ours[*change]["raised"]["type"], "builtins.RuntimeError");
        assert_eq!(
            ours[*after]["value"], theirs[*before]["value"],
            "script {k}"
        );
        assert_eq!(
            ours[*again]["value"], theirs[*before]["value"],
            "script {k}"
        );
    }
}

/// An op's metadata read through a processor and its optimized processor, which share the op:
/// the same object in the wheel, two objects in the port, which read the same (D-13).
#[test]
fn an_ops_metadata_through_two_processors_is_two_objects() {
    let mut sc = Script::new();
    let cfg = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let ft = sc.call_kw(
        n("OCIO.FileTransform"),
        vec![],
        vec![("src", s(&data("clf/xyz_to_rgb.clf")))],
        Expect::Value,
    );
    let p0 = sc.call(ra(cfg, "getProcessor"), vec![r(ft)]);
    let p = sc.call(
        ra(p0, "getOptimizedProcessor"),
        vec![n("OCIO.OPTIMIZATION_NONE")],
    );
    let it0 = sc.call(ra(p0, "getTransformFormatMetadata"), vec![]);
    let a = sc.call(ra(it0, "__getitem__"), vec![i(1)]);
    let it = sc.call(ra(p, "getTransformFormatMetadata"), vec![]);
    let b = sc.call(ra(it, "__getitem__"), vec![i(0)]);
    let same = sc.call(n("operator.is_"), vec![r(a), r(b)]);
    let ra_ = sc.repr(r(a));
    let rb = sc.repr(r(b));
    let theirs = common::wheel(&[sc.json()]).remove(0);
    let ours = common::port(&sc.json());
    let (ours, theirs) = (
        ours["steps"].as_array().unwrap(),
        theirs["steps"].as_array().unwrap(),
    );
    assert_eq!(
        theirs[same]["value"]["bool"],
        json!(true),
        "{}",
        theirs[same]
    );
    assert_eq!(ours[same]["value"]["bool"], json!(false), "{}", ours[same]);
    assert_eq!(theirs[b]["same_as"], json!([a]));
    assert_eq!(ours[b]["same_as"], json!([]));
    for k in [ra_, rb] {
        assert_eq!(ours[k], theirs[k], "step {k}");
    }
    assert_eq!(ours[ra_]["value"], ours[rb]["value"]);
}

/// A bit depth outside the enumerators, which the wheel hands the library (an optimized
/// processor without ops makes nothing of it, and is cached under the number), is the port's
/// error (U-112).
#[test]
fn a_bit_depth_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let cfg = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let group = sc.call(n("OCIO.GroupTransform"), vec![]);
    let pg = sc.call(ra(cfg, "getProcessor"), vec![r(group)]);
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    let pm = sc.call(ra(cfg, "getProcessor"), vec![r(m)]);
    let bd = sc.call(n("OCIO.BitDepth"), vec![i(42)]);
    let refused = [
        sc.call(
            ra(pg, "getOptimizedProcessor"),
            vec![r(bd), n("OCIO.BIT_DEPTH_F32"), n("OCIO.OPTIMIZATION_NONE")],
        ),
        sc.call(
            ra(pg, "getOptimizedProcessor"),
            vec![
                n("OCIO.BIT_DEPTH_F32"),
                r(bd),
                n("OCIO.OPTIMIZATION_DEFAULT"),
            ],
        ),
        // An identity, which the default optimization removes.
        sc.call(
            ra(pm, "getOptimizedProcessor"),
            vec![r(bd), r(bd), n("OCIO.OPTIMIZATION_DEFAULT")],
        ),
    ];
    let theirs = common::wheel(&[sc.json()]).remove(0);
    let ours = common::port(&sc.json());
    let (ours, theirs) = (
        ours["steps"].as_array().unwrap(),
        theirs["steps"].as_array().unwrap(),
    );
    assert_eq!(ours.len(), sc.steps.len());
    for (k, (o, t)) in ours.iter().zip(theirs).enumerate() {
        assert!(t.get("value").is_some(), "step {k}: {t}");
        if refused.contains(&k) {
            assert!(message(o).contains("U-112"), "step {k}: {o}");
            assert_eq!(o["raised"]["type"], "builtins.ValueError");
        } else {
            assert!(o.get("value").is_some(), "step {k}: {o}");
        }
    }
}

/// `extractGpuShaderInfo(None)`: the wheel calls through the null description; the port raises
/// (U-195). Python's other calls go on.
#[test]
fn extracting_into_none_is_the_ports_error() {
    let _turn = common::turn();
    pyo3::Python::attach(|py| {
        let code = std::ffi::CString::new(
            r#"
import PyOpenColorIO as OCIO
gpu = OCIO.Config.CreateRaw().getProcessor(OCIO.MatrixTransform()).getDefaultGPUProcessor()
try:
    gpu.extractGpuShaderInfo(None)
except ValueError as e:
    assert "U-195" in str(e), str(e)
else:
    raise AssertionError("no error")
d = OCIO.GpuShaderDesc.CreateShaderDesc()
gpu.extractGpuShaderInfo(d)
assert "OCIOMain" in d.getShaderText()
"#,
        )
        .unwrap();
        py.run(&code, None, None).unwrap_or_else(|e| panic!("{e}"));
    });
}

/// The embedding API shares the processors: the Python object of a Rust processor (the one
/// Python holds, while it lives), and the Rust processor of a Python one.
#[test]
fn the_embedding_api_shares_processors() {
    let _turn = common::turn();
    pyo3::Python::attach(|py| {
        let config = ocio::Config::create_raw().unwrap();
        let p = config
            .processor(&ocio::Transform::Matrix(ocio::MatrixTransform::new()))
            .unwrap();
        let obj = ocio_py::embed::processor_to_python(py, &p).unwrap();
        let again = ocio_py::embed::processor_to_python(py, &p).unwrap();
        assert!(obj.bind(py).is(again.bind(py)));
        let back = ocio_py::embed::processor_from_python(obj.bind(py)).unwrap();
        assert!(std::sync::Arc::ptr_eq(&back, &p));
        let gpu = p.default_gpu_processor().unwrap();
        let gobj = ocio_py::embed::gpu_processor_to_python(py, &gpu).unwrap();
        let from_python = obj.bind(py).call_method0("getDefaultGPUProcessor").unwrap();
        assert!(gobj.bind(py).is(&from_python));
        let gback = ocio_py::embed::gpu_processor_from_python(&from_python).unwrap();
        assert!(std::sync::Arc::ptr_eq(&gback, &gpu));
        assert!(ocio_py::embed::gpu_processor_from_python(obj.bind(py)).is_err());
    });
}
