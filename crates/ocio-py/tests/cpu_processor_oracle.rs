// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `CPUProcessor` (chunk 6.5b) against the wheel:
//! - the class's surface (its methods' docstrings, overload by overload, as the wheel's);
//! - its getters, for processors of several transforms and bit depths, and its dynamic
//!   property queries;
//! - `applyRGB` and `applyRGBA` on buffers of each bit depth's type (values whose bits differ:
//!   NaN payloads, infinities, signed zeros, subnormals), in any shape, on `bytearray`s and
//!   memoryviews, and on lists, which return new ones; the binding's checks and their messages
//!   (type, C order, divisibility) and the library's (bit depth, empty buffers);
//! - `apply` of image descriptions: packed and planar, in place and from a source to a
//!   destination of another bit depth, with channel orders and strides (negative ones, padded
//!   pixels), one image as both source and destination, two descriptions of the same memory,
//!   planes in one array, a read-only source;
//! - without the wheel, whose process dies there or writes memory it may not, the port's
//!   errors: an `ImageDesc()` applied (U-146), a read-only destination (U-156), a source and a
//!   destination that partly overlap, and planes some of which share memory (U-150).
//!
//! The processors come from `Config.CreateRaw().getProcessor(t)` and its
//! `getDefaultCPUProcessor()` or `getOptimizedCPUProcessor(in, out, flags)`. Until the port binds
//! those (cards p6-config and p6-processors), this test's interpreter gets stand-ins for them,
//! which make the same processor with the Rust API and hand it to Python through the embedding
//! API (`ocio_py::embed::cpu_processor_to_python`); the transform comes from the script, as the
//! wheel's. The steps that make the config and the processor aren't compared, only the CPU
//! processor they give.
#![cfg(feature = "python")]

mod common;

use std::ffi::c_ulong;
use std::sync::Once;

use common::script::{Expect, Script, compare, f, fl, i, l, n, none, r, ra, s};
use ocio_ops::open_color_types::{BitDepth, OptimizationFlags};
use pyo3::prelude::*;
use pyo3::types::{PyCFunction, PyDict, PyTuple};
use serde_json::{Value, json};

/// The Rust side of the stand-in: `_make(transform, inBitDepth, outBitDepth, flags)`, the CPU
/// processor `Config::create_raw()` makes of `transform`, as a Python `CPUProcessor`.
fn make_cpu(args: &Bound<'_, PyTuple>) -> PyResult<Py<PyAny>> {
    let py = args.py();
    let error = |e: ocio::Exception| pyo3::exceptions::PyRuntimeError::new_err(format!("{e:?}"));
    let transform = ocio_py::embed::transform_from_python(&args.get_item(0)?)?;
    let depth = |k: usize| -> PyResult<BitDepth> {
        let v: i64 = args.get_item(k)?.extract()?;
        use BitDepth::*;
        [
            Unknown, Uint8, Uint10, Uint12, Uint14, Uint16, Uint32, F16, F32,
        ]
        .into_iter()
        .find(|b| *b as i64 == v)
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("a bit depth"))
    };
    let flags: c_ulong = args.get_item(3)?.extract()?;
    let config = ocio::Config::create_raw().map_err(error)?;
    let processor = config.processor(&transform).map_err(error)?;
    let cpu = processor
        .optimized_cpu_processor_with_bit_depths(depth(1)?, depth(2)?, OptimizationFlags(flags))
        .map_err(error)?;
    ocio_py::embed::cpu_processor_to_python(py, &cpu)
}

/// The stand-ins' Python side, installed while the module lacks
/// `Processor.getDefaultCPUProcessor`.
const STAND_IN: &str = r#"
import PyOpenColorIO as OCIO

class _Processor:
    def __init__(self, transform):
        self.transform = transform

    def getDefaultCPUProcessor(self):
        return _make(self.transform, int(OCIO.BIT_DEPTH_F32), int(OCIO.BIT_DEPTH_F32),
                     int(OCIO.OPTIMIZATION_DEFAULT))

    def getOptimizedCPUProcessor(self, inBitDepth, outBitDepth, oFlags):
        return _make(self.transform, int(inBitDepth), int(outBitDepth), int(oFlags))

class Config:
    @staticmethod
    def CreateRaw():
        return Config()

    def getProcessor(self, transform):
        return _Processor(transform)

if not hasattr(getattr(OCIO, "Processor", None), "getDefaultCPUProcessor"):
    OCIO.Config = Config
"#;

static STAND_IN_ONCE: Once = Once::new();

/// Starts the interpreter with the stand-ins, if the module needs them.
fn start() {
    common::start_python();
    STAND_IN_ONCE.call_once(|| {
        Python::attach(|py| {
            let make = PyCFunction::new_closure(py, Some(c"_make"), None, |args, _| make_cpu(args))
                .unwrap();
            let globals = PyDict::new(py);
            globals.set_item("_make", make).unwrap();
            let code = std::ffi::CString::new(STAND_IN).unwrap();
            py.run(&code, Some(&globals), None)
                .unwrap_or_else(|e| panic!("{e}"));
        });
    });
}

/// A NumPy array of `dtype` (NumPy's `str`, `<f4`), `shape` and `bytes` (C order).
fn arr(dtype: &str, shape: &[usize], bytes: &[u8]) -> Value {
    json!({ "array": { "dtype": dtype, "shape": shape, "bytes": ocio_testkit::oracle_values::hex(bytes) } })
}

fn le32(bits: &[u32]) -> Vec<u8> {
    bits.iter().flat_map(|b| b.to_le_bytes()).collect()
}

fn le16(bits: &[u16]) -> Vec<u8> {
    bits.iter().flat_map(|b| b.to_le_bytes()).collect()
}

/// float32 bits whose values differ in every way processing could treat differently: NaN
/// payloads (quiet and signaling, both signs), infinities, signed zeros, subnormals, extremes,
/// ordinary values.
const F32_BITS: [u32; 16] = [
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
    0x7f7f_ffff,
    0xff7f_ffff,
    0x3f80_0000,
    0x3e4c_cccd,
    0xbf00_0000,
    0x4049_0fdb,
];

/// float16 bits, likewise.
const F16_BITS: [u16; 12] = [
    0x7e01, 0x7d03, 0xfe05, 0x7c00, 0xfc00, 0x8000, 0x0001, 0x3c00, 0x3555, 0xbc00, 0x7bff, 0x0400,
];

/// uint16 and uint8 values from one end to the other.
const U16S: [u16; 12] = [0, 1, 2, 3, 255, 256, 1023, 4095, 32767, 32768, 65534, 65535];
const U8S: [u8; 12] = [0, 1, 2, 3, 63, 64, 127, 128, 129, 200, 254, 255];

/// The CPU processor of the transform of step `t`: `getDefaultCPUProcessor()`, or
/// `getOptimizedCPUProcessor(in, out, OPTIMIZATION_DEFAULT)` of the bit depths named.
fn cpu(sc: &mut Script, t: usize, depths: Option<(&str, &str)>) -> usize {
    let config = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    sc.uncompare(config);
    let processor = sc.call(ra(config, "getProcessor"), vec![r(t)]);
    sc.uncompare(processor);
    match depths {
        None => sc.call(ra(processor, "getDefaultCPUProcessor"), vec![]),
        Some((i, o)) => sc.call(
            ra(processor, "getOptimizedCPUProcessor"),
            vec![
                n(&format!("OCIO.BIT_DEPTH_{i}")),
                n(&format!("OCIO.BIT_DEPTH_{o}")),
                n("OCIO.OPTIMIZATION_DEFAULT"),
            ],
        ),
    }
}

/// `MatrixTransform.Scale([0.5, 0.5, 0.5, 1.0])` (upstream's CPUProcessorTest), with an
/// offset, so that every channel changes.
fn scale(sc: &mut Script) -> usize {
    let m = sc.call(
        n("OCIO.MatrixTransform.Scale"),
        vec![fl(&[0.5, 0.5, 0.5, 1.0])],
    );
    sc.call(ra(m, "setOffset"), vec![fl(&[0.25, -0.125, 0.0, 0.5])]);
    m
}

/// The class has the wheel's surface, its docstrings included.
#[test]
fn the_class_has_the_wheels_surface() {
    start();
    let report = common::wheel_surface();
    let theirs = common::wheel_member(&report, "CPUProcessor");
    assert!(
        theirs["members"].as_array().is_some_and(|m| m.len() > 10),
        "{theirs}"
    );
    assert_eq!(common::port_member("CPUProcessor"), theirs);
}

const GETTERS: [&str; 7] = [
    "isNoOp",
    "isIdentity",
    "hasChannelCrosstalk",
    "getCacheID",
    "getInputBitDepth",
    "getOutputBitDepth",
    "isDynamic",
];

const DYNAMIC: [&str; 7] = [
    "EXPOSURE",
    "CONTRAST",
    "GAMMA",
    "GRADING_PRIMARY",
    "GRADING_RGBCURVE",
    "GRADING_TONE",
    "GRADING_HUECURVE",
];

/// The getters and the dynamic property queries.
fn getters_script() -> Script {
    let mut sc = Script::new();
    let identity = sc.call(n("OCIO.MatrixTransform"), vec![]);
    let m = scale(&mut sc);
    let hsv = sc.call(
        n("OCIO.FixedFunctionTransform"),
        vec![n("OCIO.FIXED_FUNCTION_RGB_TO_HSV")],
    );
    let range = sc.call(n("OCIO.RangeTransform"), vec![]);
    sc.call(ra(range, "setMinInValue"), vec![f(0.0)]);
    sc.call(ra(range, "setMinOutValue"), vec![f(0.0)]);
    let log = sc.call(n("OCIO.LogTransform"), vec![]);
    let group = sc.call(n("OCIO.GroupTransform"), vec![l(vec![r(m), r(log)])]);
    let mut cpus = Vec::new();
    for (t, depths) in [
        (identity, None),
        (m, None),
        (hsv, None),
        (range, None),
        (group, None),
        (m, Some(("UINT8", "F16"))),
        (m, Some(("UINT16", "UINT10"))),
        (log, Some(("F16", "F32"))),
    ] {
        let c = cpu(&mut sc, t, depths);
        sc.eval(r(c), &GETTERS);
        cpus.push(c);
    }
    let c = cpus[1];
    for d in DYNAMIC {
        let ty = n(&format!("OCIO.DYNAMIC_PROPERTY_{d}"));
        sc.call(ra(c, "hasDynamicProperty"), vec![ty.clone()]);
        sc.raises(ra(c, "getDynamicProperty"), vec![ty]);
    }
    let unknown = sc.call(n("OCIO.DynamicPropertyType"), vec![i(42)]);
    sc.call(ra(c, "hasDynamicProperty"), vec![r(unknown)]);
    sc.raises(ra(c, "getDynamicProperty"), vec![r(unknown)]);
    sc.call_kw(
        ra(c, "hasDynamicProperty"),
        vec![],
        vec![("type", n("OCIO.DYNAMIC_PROPERTY_GAMMA"))],
        Expect::Value,
    );
    // The casters.
    sc.raises(ra(c, "hasDynamicProperty"), vec![i(0)]);
    sc.raises(ra(c, "getDynamicProperty"), vec![]);
    sc.raises(n("OCIO.CPUProcessor.isNoOp"), vec![i(1)]);
    sc.raises(n("OCIO.CPUProcessor"), vec![]);
    sc
}

/// `applyRGB` and `applyRGBA` on buffers and lists.
fn apply_rgb_script() -> Script {
    let mut sc = Script::new();
    let m = scale(&mut sc);
    let f32_cpu = cpu(&mut sc, m, None);
    let f16_cpu = cpu(&mut sc, m, Some(("F16", "F16")));
    let u16_cpu = cpu(&mut sc, m, Some(("UINT16", "UINT16")));
    let u8_cpu = cpu(&mut sc, m, Some(("UINT8", "UINT8")));
    for (method, channels) in [("applyRGB", 3usize), ("applyRGBA", 4)] {
        let count = if channels == 3 { 12 } else { 16 };
        let pixels = count / channels;
        // Each bit depth's type with its processor, in several shapes.
        for shape in [
            vec![count],
            vec![pixels, channels],
            vec![2, pixels / 2, channels],
        ] {
            let a = sc.eval(arr("<f4", &shape, &le32(&F32_BITS[..count])), &[]);
            sc.call(ra(f32_cpu, method), vec![r(a)]);
            sc.eval(r(a), &[]);
        }
        let h = sc.eval(arr("<f2", &[12], &le16(&F16_BITS)), &[]);
        sc.call(ra(f16_cpu, method), vec![r(h)]);
        sc.eval(r(h), &[]);
        let u = sc.eval(arr("<u2", &[12], &le16(&U16S)), &[]);
        sc.call(ra(u16_cpu, method), vec![r(u)]);
        sc.eval(r(u), &[]);
        let b = sc.eval(arr("|u1", &[12], &U8S), &[]);
        sc.call(ra(u8_cpu, method), vec![r(b)]);
        sc.eval(r(b), &[]);
        // A bytearray (uint8), a memoryview cast to float.
        let ba = sc.call(n("bytearray"), vec![common::script::by(&U8S)]);
        sc.call(ra(u8_cpu, method), vec![r(ba)]);
        sc.eval(r(ba), &[]);
        // A memoryview cast to float (of a uint8 array's memory); an array over a bytearray.
        let raw = sc.eval(arr("|u1", &[48], &le32(&F32_BITS[..12])), &[]);
        let mv = sc.eval(ra(raw, "data"), &[]);
        sc.unrepr(mv);
        let floats = sc.call(ra(mv, "cast"), vec![s("f")]);
        sc.unrepr(floats);
        sc.call(ra(f32_cpu, method), vec![r(floats)]);
        sc.eval(r(raw), &[]);
        let over = sc.call(n("numpy.frombuffer"), vec![r(ba), s("|u1")]);
        sc.call(ra(u8_cpu, method), vec![r(over)]);
        sc.eval(r(ba), &[]);
        // Read-only buffers the library refuses: nothing is written, the library's error.
        sc.raises(ra(f32_cpu, method), vec![common::script::by(&[0; 12])]);
        let ro = sc.call(
            n("numpy.frombuffer"),
            vec![common::script::by(&le16(&F16_BITS)), s("<f2")],
        );
        sc.raises(ra(f32_cpu, method), vec![r(ro)]);
        // Lists: a new list, the argument unchanged; ints and NumPy floats converted.
        let values = fl(&[
            0.5, -1.0, 2.0, 1e-30, 3.0, -0.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
        ]);
        let list = sc.eval(values, &[]);
        sc.call(ra(f32_cpu, method), vec![r(list)]);
        sc.eval(r(list), &[]);
        sc.call(
            ra(f32_cpu, method),
            vec![l((0..12).map(|k| i(k - 3)).collect())],
        );
        let tuple = sc.eval(
            common::script::t((0..12).map(|k| f(k as f64 / 3.0)).collect()),
            &[],
        );
        sc.call(ra(f32_cpu, method), vec![r(tuple)]);
        // The binding's checks.
        for dt in ["<f8", "<i4", "<u4", "|i1", ">f4", "<c8", "|b1"] {
            let bad = sc.call(n("numpy.zeros"), vec![i(12), s(dt)]);
            sc.raises(ra(f32_cpu, method), vec![r(bad)]);
        }
        let odd = sc.call(n("numpy.zeros"), vec![i(channels as i128 + 1), s("<f4")]);
        sc.raises(ra(f32_cpu, method), vec![r(odd)]);
        let fortran = sc.call_kw(
            n("numpy.zeros"),
            vec![common::script::t(vec![i(channels as i128), i(2)])],
            vec![("dtype", s("<f4")), ("order", s("F"))],
            Expect::Value,
        );
        sc.raises(ra(f32_cpu, method), vec![r(fortran)]);
        let a = sc.eval(
            arr("<f4", &[24], &le32(&[F32_BITS, F32_BITS].concat()[..24])),
            &[],
        );
        let every_other = sc.call(n("slice"), vec![none(), none(), i(2)]);
        let strided = sc.call(n("operator.getitem"), vec![r(a), r(every_other)]);
        sc.raises(ra(f32_cpu, method), vec![r(strided)]);
        sc.raises(ra(f32_cpu, method), vec![l(vec![f(1.0); channels + 1])]);
        // The library's checks: another bit depth than the processor's; no items.
        sc.raises(ra(f32_cpu, method), vec![r(h)]);
        sc.raises(ra(u16_cpu, method), vec![r(b)]);
        let empty = sc.call(n("numpy.zeros"), vec![i(0), s("<f4")]);
        sc.raises(ra(f32_cpu, method), vec![r(empty)]);
        sc.raises(ra(f32_cpu, method), vec![l(vec![])]);
        // The casters.
        sc.raises(ra(f32_cpu, method), vec![s("abc")]);
        sc.raises(ra(f32_cpu, method), vec![none()]);
        sc.raises(ra(f32_cpu, method), vec![]);
    }
    sc
}

/// `apply` of image descriptions.
fn apply_script() -> Script {
    let mut sc = Script::new();
    let m = scale(&mut sc);
    let f32_cpu = cpu(&mut sc, m, None);
    let to_half = cpu(&mut sc, m, Some(("F32", "F16")));
    let image = |sc: &mut Script, args: Vec<Value>| {
        let k = sc.call(n("OCIO.PackedImageDesc"), args);
        sc.unrepr(k);
        k
    };
    let planar = |sc: &mut Script, args: Vec<Value>| {
        let k = sc.call(n("OCIO.PlanarImageDesc"), args);
        sc.unrepr(k);
        k
    };
    let auto = || n("OCIO.AutoStride");
    // Packed, in place: RGBA, then the channel orders.
    let a = sc.eval(arr("<f4", &[16], &le32(&F32_BITS)), &[]);
    let p = image(&mut sc, vec![r(a), i(2), i(2), i(4)]);
    sc.call(ra(f32_cpu, "apply"), vec![r(p)]);
    sc.eval(r(a), &[]);
    for order in ["BGRA", "ABGR", "RGB", "BGR"] {
        let w = if order.len() == 4 { 3 } else { 4 };
        let a = sc.eval(arr("<f4", &[12], &le32(&F32_BITS[4..16])), &[]);
        let k = sc.call_kw(
            n("OCIO.PackedImageDesc"),
            vec![r(a), i(w), i(1)],
            vec![("chanOrder", n(&format!("OCIO.CHANNEL_ORDERING_{order}")))],
            Expect::Value,
        );
        sc.unrepr(k);
        sc.call_kw(
            ra(f32_cpu, "apply"),
            vec![],
            vec![("imgDesc", r(k))],
            Expect::Value,
        );
        sc.eval(r(a), &[]);
    }
    // Strides: a reversed view (from its last item), padded pixels.
    let a = sc.eval(arr("<f4", &[12], &le32(&F32_BITS[..12])), &[]);
    let backwards = sc.call(n("slice"), vec![none(), none(), i(-1)]);
    let reversed = sc.call(n("operator.getitem"), vec![r(a), r(backwards)]);
    let k = image(
        &mut sc,
        vec![
            r(reversed),
            i(2),
            i(2),
            i(3),
            n("OCIO.BIT_DEPTH_F32"),
            i(-4),
            i(-12),
            i(-24),
        ],
    );
    sc.call(ra(f32_cpu, "apply"), vec![r(k)]);
    sc.eval(r(a), &[]);
    // Padded rows: a view of the first four of six columns, its rows 24 bytes apart.
    let grid = sc.eval(arr("<f4", &[2, 6], &le32(&F32_BITS[..12])), &[]);
    let columns = sc.call(n("slice"), vec![none(), i(4)]);
    let all = sc.call(n("slice"), vec![none()]);
    let index = sc.eval(common::script::t(vec![r(all), r(columns)]), &[]);
    let view = sc.call(n("operator.getitem"), vec![r(grid), r(index)]);
    let k = image(
        &mut sc,
        vec![
            r(view),
            i(1),
            i(2),
            i(4),
            n("OCIO.BIT_DEPTH_F32"),
            auto(),
            auto(),
            i(24),
        ],
    );
    sc.call(ra(f32_cpu, "apply"), vec![r(k)]);
    sc.eval(r(grid), &[]);
    // From a source to a destination: of the same bit depth; to half floats; from a read-only
    // source.
    let src = sc.eval(arr("<f4", &[16], &le32(&F32_BITS)), &[]);
    let dst = sc.call(n("numpy.zeros"), vec![i(16), s("<f4")]);
    let s_img = image(&mut sc, vec![r(src), i(4), i(1), i(4)]);
    let d_img = image(&mut sc, vec![r(dst), i(4), i(1), i(4)]);
    sc.call(ra(f32_cpu, "apply"), vec![r(s_img), r(d_img)]);
    sc.eval(r(src), &[]);
    sc.eval(r(dst), &[]);
    let halves = sc.call(n("numpy.zeros"), vec![i(16), s("<f2")]);
    let h_img = image(
        &mut sc,
        vec![
            r(halves),
            i(4),
            i(1),
            i(4),
            n("OCIO.BIT_DEPTH_F16"),
            auto(),
            auto(),
            auto(),
        ],
    );
    sc.call_kw(
        ra(to_half, "apply"),
        vec![],
        vec![("srcImgDesc", r(s_img)), ("dstImgDesc", r(h_img))],
        Expect::Value,
    );
    sc.eval(r(halves), &[]);
    sc.raises(ra(f32_cpu, "apply"), vec![r(s_img), r(h_img)]);
    let ro = sc.call(
        n("numpy.frombuffer"),
        vec![common::script::by(&le32(&F32_BITS)), s("<f4")],
    );
    let ro_img = image(&mut sc, vec![r(ro), i(4), i(1), i(4)]);
    let out = sc.call(n("numpy.zeros"), vec![i(16), s("<f4")]);
    let out_img = image(&mut sc, vec![r(out), i(4), i(1), i(4)]);
    sc.call(ra(f32_cpu, "apply"), vec![r(ro_img), r(out_img)]);
    sc.eval(r(out), &[]);
    sc.eval(r(ro), &[]);
    // One image as source and destination; two descriptions of the same memory.
    let a = sc.eval(arr("<f4", &[16], &le32(&F32_BITS)), &[]);
    let k = image(&mut sc, vec![r(a), i(4), i(1), i(4)]);
    sc.call(ra(f32_cpu, "apply"), vec![r(k), r(k)]);
    sc.eval(r(a), &[]);
    let k2 = image(&mut sc, vec![r(a), i(4), i(1), i(4)]);
    sc.call(ra(f32_cpu, "apply"), vec![r(k), r(k2)]);
    sc.eval(r(a), &[]);
    // Descriptions that share memory, which the library refuses first: nothing is written.
    let k3 = image(&mut sc, vec![r(a), i(2), i(2), i(4)]);
    sc.raises(ra(f32_cpu, "apply"), vec![r(k), r(k3)]);
    let ro_k = image(&mut sc, vec![r(ro), i(2), i(2), i(4)]);
    sc.raises(ra(f32_cpu, "apply"), vec![r(ro_img), r(ro_k)]);
    sc.raises(ra(to_half, "apply"), vec![r(ro_img)]);
    sc.eval(r(a), &[]);
    sc.eval(r(ro), &[]);
    // Planar: in place, with and without alpha; from a source to a destination; planes in one
    // array.
    let planes: Vec<usize> = (0..4)
        .map(|k| sc.eval(arr("<f4", &[4], &le32(&F32_BITS[4 * k..4 * k + 4])), &[]))
        .collect();
    let rgba = planar(
        &mut sc,
        planes
            .iter()
            .map(|&p| r(p))
            .collect::<Vec<_>>()
            .into_iter()
            .chain([i(2), i(2)])
            .collect(),
    );
    sc.call(ra(f32_cpu, "apply"), vec![r(rgba)]);
    for &p in &planes {
        sc.eval(r(p), &[]);
    }
    let rgb = planar(
        &mut sc,
        vec![r(planes[0]), r(planes[1]), r(planes[2]), i(4), i(1)],
    );
    sc.call(ra(f32_cpu, "apply"), vec![r(rgb)]);
    for &p in &planes {
        sc.eval(r(p), &[]);
    }
    let outs: Vec<usize> = (0..4)
        .map(|_| sc.call(n("numpy.zeros"), vec![i(4), s("<f4")]))
        .collect();
    let out_planar = planar(
        &mut sc,
        outs.iter().map(|&p| r(p)).chain([i(2), i(2)]).collect(),
    );
    sc.call(ra(f32_cpu, "apply"), vec![r(rgba), r(out_planar)]);
    for &p in &outs {
        sc.eval(r(p), &[]);
    }
    let packed_out = sc.call(n("numpy.zeros"), vec![i(16), s("<f4")]);
    let po = image(&mut sc, vec![r(packed_out), i(2), i(2), i(4)]);
    sc.call(ra(f32_cpu, "apply"), vec![r(rgba), r(po)]);
    sc.eval(r(packed_out), &[]);
    let one = sc.eval(arr("<f4", &[4], &le32(&F32_BITS[8..12])), &[]);
    let same = planar(&mut sc, vec![r(one), r(one), r(one), r(one), i(4), i(1)]);
    sc.call(ra(f32_cpu, "apply"), vec![r(same)]);
    sc.eval(r(one), &[]);
    // The casters (an image description's repr, its address, would be in the TypeError).
    sc.raises(ra(f32_cpu, "apply"), vec![i(1)]);
    sc.raises(ra(f32_cpu, "apply"), vec![i(1), i(2)]);
    sc.raises(ra(f32_cpu, "apply"), vec![]);
    sc
}

/// Each method behaves as the wheel's.
#[test]
fn the_methods_behave_as_the_wheels() {
    start();
    let scripts = [getters_script(), apply_rgb_script(), apply_script()];
    compare(&scripts);
}

/// Runs `code` in the port's module, which asserts what it expects.
fn port_run(code: &str) {
    start();
    Python::attach(|py| {
        let code = std::ffi::CString::new(code).unwrap();
        py.run(&code, None, None).unwrap_or_else(|e| panic!("{e}"));
    });
}

/// Where the wheel's process dies, or it writes memory it may not, the port's errors: an
/// `ImageDesc()` applied (U-146: the wheel dereferences its null image); a read-only buffer
/// written (U-156); a source and a destination that partly overlap, and planes some of which
/// share memory (U-150). Nothing is written then, and the processor stays usable.
#[test]
fn the_ports_errors_where_the_wheel_writes_memory_it_may_not() {
    port_run(
        r#"
import PyOpenColorIO as OCIO, numpy as np

def refused(f, kind, entry):
    try:
        f()
    except kind as e:
        assert entry in str(e), str(e)
    else:
        raise AssertionError("no error")

m = OCIO.MatrixTransform.Scale([0.5, 0.5, 0.5, 1.0])
cpu = OCIO.Config.CreateRaw().getProcessor(m).getDefaultCPUProcessor()
a = np.arange(16, dtype=np.float32)
img = OCIO.PackedImageDesc(a, 4, 1, 4)

refused(lambda: cpu.apply(OCIO.ImageDesc()), ValueError, "U-146")
refused(lambda: cpu.apply(OCIO.ImageDesc(), img), ValueError, "U-146")
refused(lambda: cpu.apply(img, OCIO.ImageDesc()), ValueError, "U-146")
assert np.array_equal(a, np.arange(16, dtype=np.float32))

ro = np.frombuffer(np.arange(12, dtype=np.float32).tobytes(), np.float32)
refused(lambda: cpu.applyRGB(ro), BufferError, "U-156")
refused(lambda: cpu.applyRGBA(ro), BufferError, "U-156")
refused(lambda: cpu.apply(OCIO.PackedImageDesc(ro, 3, 1, 4)), BufferError, "U-156")
refused(lambda: cpu.apply(OCIO.PackedImageDesc(a[:12], 3, 1, 4), OCIO.PackedImageDesc(ro, 3, 1, 4)), BufferError, "U-156")
assert np.array_equal(ro, np.arange(12, dtype=np.float32))

src = OCIO.PackedImageDesc(a[:8], 2, 1, 4)
dst = OCIO.PackedImageDesc(a[4:12], 2, 1, 4)
refused(lambda: cpu.apply(src, dst), BufferError, "U-150")
assert np.array_equal(a, np.arange(16, dtype=np.float32))

p = np.arange(8, dtype=np.float32)
q = np.arange(4, dtype=np.float32)
mixed = OCIO.PlanarImageDesc(p[:4], p[2:6], q, 4, 1)
refused(lambda: cpu.apply(mixed), BufferError, "U-150")
assert np.array_equal(p, np.arange(8, dtype=np.float32))

cpu.apply(img)
assert not np.array_equal(a, np.arange(16, dtype=np.float32))
"#,
    );
}
