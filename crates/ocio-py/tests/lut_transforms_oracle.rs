// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `Lut1DTransform` and `Lut3DTransform` (chunk 6.5b) against the wheel:
//! - each class's surface, member for member;
//! - their constructors, getters and setters, `equals` and `repr`;
//! - `setData` over buffers of each kind (NumPy arrays of any shape, views whose data pointer
//!   the binding reads from contiguously, their strides aside, read-only arrays, memoryviews)
//!   with values whose bits differ (NaN payloads, infinities, signed zeros, subnormals), then
//!   `getData` and `getValue`; the binding's checks and their messages (the type, the number of
//!   entries, the 3D grid size from the shape, in C++'s `unsigned long`) and the library's;
//! - without the wheel, which reads past the buffer there, the port's error for a view whose
//!   values reach past its items (U-148: a reversed view, read forward from its last item).
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, by, compare, f, fl, i, l, n, none, r, ra, s};
use serde_json::{Value, json};

fn arr(dtype: &str, shape: &[usize], bytes: &[u8]) -> Value {
    json!({ "array": { "dtype": dtype, "shape": shape, "bytes": ocio_testkit::oracle_values::hex(bytes) } })
}

fn le32(bits: &[u32]) -> Vec<u8> {
    bits.iter().flat_map(|b| b.to_le_bytes()).collect()
}

/// float32 bits whose values differ in every way a copy could lose: NaN payloads (quiet and
/// signaling, both signs), infinities, signed zeros, subnormals, extremes, ordinary values.
const F32_BITS: [u32; 24] = [
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
    0x3dcc_cccd,
    0x3f00_0000,
    0x4000_0000,
    0xc000_0000,
    0x3f7f_ffff,
    0x0080_0000,
    0x3eaa_aaab,
    0x4120_0000,
];

#[test]
fn the_classes_have_the_wheels_surface() {
    let report = common::wheel_surface();
    for c in ["Lut1DTransform", "Lut3DTransform"] {
        let theirs = common::wheel_member(&report, c);
        assert!(
            theirs["members"].as_array().is_some_and(|m| m.len() > 10),
            "{c}: {theirs}"
        );
        assert_eq!(common::port_member(c), theirs, "{c}");
    }
}

const LUT1D_GETTERS: [&str; 9] = [
    "getLength",
    "getInputHalfDomain",
    "getOutputRawHalfs",
    "getFileOutputBitDepth",
    "getHueAdjust",
    "getInterpolation",
    "getDirection",
    "getData",
    "getFormatMetadata",
];

const LUT3D_GETTERS: [&str; 6] = [
    "getGridSize",
    "getFileOutputBitDepth",
    "getInterpolation",
    "getDirection",
    "getData",
    "getFormatMetadata",
];

/// The buffers both classes' `setData` reads: each with its NumPy type and shape.
fn buffers(sc: &mut Script, count: usize, shape: &[usize]) -> Vec<usize> {
    let mut out = Vec::new();
    let flat = sc.eval(arr("<f4", &[count], &le32(&F32_BITS[..count])), &[]);
    out.push(flat);
    out.push(sc.eval(arr("<f4", shape, &le32(&F32_BITS[..count])), &[]));
    // A view of every other value: the binding reads contiguously from its data pointer.
    let double: Vec<u32> = F32_BITS.iter().chain(F32_BITS.iter()).copied().collect();
    let base = sc.eval(arr("<f4", &[2 * count], &le32(&double[..2 * count])), &[]);
    let step2 = sc.call(n("slice"), vec![none(), none(), i(2)]);
    out.push(sc.call(n("operator.getitem"), vec![r(base), r(step2)]));
    // Read-only, and a memoryview.
    out.push(sc.call(
        n("numpy.frombuffer"),
        vec![by(&le32(&F32_BITS[..count])), s("<f4")],
    ));
    let mv = sc.eval(ra(flat, "data"), &[]);
    sc.unrepr(mv);
    out.push(mv);
    out
}

/// `Lut1DTransform`.
fn lut1d_script() -> Script {
    let mut sc = Script::new();
    let t = sc.call(n("OCIO.Lut1DTransform"), vec![]);
    sc.eval(r(t), &LUT1D_GETTERS);
    sc.repr(r(t));
    for args in [vec![i(4), b(true)], vec![i(3), b(false)]] {
        let k = sc.call(n("OCIO.Lut1DTransform"), args);
        sc.eval(r(k), &LUT1D_GETTERS);
    }
    let k = sc.call_kw(
        n("OCIO.Lut1DTransform"),
        vec![],
        vec![
            ("length", i(5)),
            ("inputHalfDomain", b(false)),
            ("outputRawHalfs", b(true)),
            ("fileOutputBitDepth", n("OCIO.BIT_DEPTH_UINT10")),
            ("hueAdjust", n("OCIO.HUE_DW3")),
            ("interpolation", n("OCIO.INTERP_LINEAR")),
            ("direction", n("OCIO.TRANSFORM_DIR_INVERSE")),
        ],
        Expect::Value,
    );
    sc.eval(r(k), &LUT1D_GETTERS);
    sc.repr(r(k));
    // setData of each buffer, then getData and the values.
    for data in buffers(&mut sc, 12, &[4, 3]) {
        sc.call(ra(t, "setData"), vec![r(data)]);
        sc.eval(r(t), &LUT1D_GETTERS);
        for index in 0..4 {
            sc.call(ra(t, "getValue"), vec![i(index)]);
        }
    }
    // The array getData returns is a copy.
    let data = sc.call(ra(t, "getData"), vec![]);
    sc.step(
        json!({ "setitem": r(data), "index": i(0), "value": f(9.5) }),
        Expect::Value,
    );
    sc.call(ra(t, "getValue"), vec![i(0)]);
    sc.call(ra(t, "setData"), vec![r(data)]);
    sc.call(ra(t, "getValue"), vec![i(0)]);
    // setValue, setLength, and their errors.
    sc.call(ra(t, "setValue"), vec![i(1), f(0.25), f(-0.5), f(1e30)]);
    sc.call(ra(t, "getData"), vec![]);
    sc.raises(ra(t, "getValue"), vec![i(4)]);
    sc.raises(ra(t, "setValue"), vec![i(4), f(0.0), f(0.0), f(0.0)]);
    sc.call(ra(t, "setLength"), vec![i(2)]);
    sc.call(ra(t, "getData"), vec![]);
    // The binding's checks, the library's.
    for dt in ["<f8", "<f2", "<i4", "|u1"] {
        let bad = sc.call(n("numpy.zeros"), vec![i(12), s(dt)]);
        sc.raises(ra(t, "setData"), vec![r(bad)]);
    }
    sc.raises(ra(t, "setData"), vec![by(&[0; 12])]);
    let odd = sc.call(n("numpy.zeros"), vec![i(7), s("<f4")]);
    sc.raises(ra(t, "setData"), vec![r(odd)]);
    let empty = sc.call(n("numpy.zeros"), vec![i(0), s("<f4")]);
    sc.call_kw(
        ra(t, "setData"),
        vec![],
        vec![("data", r(empty))],
        Expect::Raised,
    );
    let three = sc.call(n("numpy.zeros"), vec![i(3), s("<f4")]);
    sc.raises(ra(t, "setData"), vec![r(three)]);
    sc.eval(r(t), &LUT1D_GETTERS);
    sc.raises(ra(t, "setData"), vec![fl(&[0.0; 6])]);
    sc.raises(ra(t, "setData"), vec![]);
    // equals.
    let u = sc.call(n("OCIO.Lut1DTransform"), vec![]);
    sc.call(ra(t, "equals"), vec![r(u)]);
    sc.call(ra(u, "setData"), vec![r(data)]);
    sc.call(ra(t, "setData"), vec![r(data)]);
    sc.call(ra(t, "equals"), vec![r(u)]);
    sc.raises(ra(t, "equals"), vec![i(1)]);
    let _ = l;
    sc
}

/// `Lut3DTransform`.
fn lut3d_script() -> Script {
    let mut sc = Script::new();
    let t = sc.call(n("OCIO.Lut3DTransform"), vec![]);
    sc.eval(r(t), &LUT3D_GETTERS);
    sc.repr(r(t));
    let k = sc.call(n("OCIO.Lut3DTransform"), vec![i(3)]);
    sc.eval(r(k), &LUT3D_GETTERS);
    let k = sc.call_kw(
        n("OCIO.Lut3DTransform"),
        vec![],
        vec![
            ("gridSize", i(2)),
            ("fileOutputBitDepth", n("OCIO.BIT_DEPTH_UINT12")),
            ("interpolation", n("OCIO.INTERP_TETRAHEDRAL")),
            ("direction", n("OCIO.TRANSFORM_DIR_INVERSE")),
        ],
        Expect::Value,
    );
    sc.eval(r(k), &LUT3D_GETTERS);
    sc.repr(r(k));
    // setData of each buffer (2 x 2 x 2 entries), then getData and the values.
    for data in buffers(&mut sc, 24, &[2, 2, 2, 3]) {
        sc.call(ra(t, "setData"), vec![r(data)]);
        sc.eval(r(t), &LUT3D_GETTERS);
        sc.call(ra(t, "getValue"), vec![i(1), i(0), i(1)]);
    }
    // The grid size from the shape: its first dimension (here, 2 of (2, 12)).
    let wide = sc.eval(arr("<f4", &[2, 12], &le32(&F32_BITS)), &[]);
    sc.call(ra(t, "setData"), vec![r(wide)]);
    sc.call(ra(t, "getData"), vec![]);
    // setValue, setGridSize, and their errors.
    sc.call(
        ra(t, "setValue"),
        vec![i(1), i(1), i(0), f(0.25), f(-0.5), f(1e30)],
    );
    sc.call(ra(t, "getData"), vec![]);
    sc.raises(ra(t, "getValue"), vec![i(2), i(0), i(0)]);
    sc.raises(
        ra(t, "setValue"),
        vec![i(0), i(2), i(0), f(0.0), f(0.0), f(0.0)],
    );
    sc.call(ra(t, "setGridSize"), vec![i(3)]);
    sc.call(ra(t, "getData"), vec![]);
    // The binding's checks, the library's.
    for dt in ["<f8", "<f2", "<u2"] {
        let bad = sc.call(n("numpy.zeros"), vec![i(24), s(dt)]);
        sc.raises(ra(t, "setData"), vec![r(bad)]);
    }
    for count in [23, 30, 3, 0, 192] {
        let data = sc.call(n("numpy.zeros"), vec![i(count), s("<f4")]);
        // cbrt(10) rounds to 2, which doesn't fit 30 entries; 3 and 0 entries are grids of 1
        // and 0.
        let expect = if [3, 0, 192].contains(&count) {
            Expect::Value
        } else {
            Expect::Raised
        };
        sc.step(
            json!({ "call": ra(t, "setData"), "args": [r(data)] }),
            expect,
        );
    }
    sc.call(ra(t, "getGridSize"), vec![]);
    for shape in [vec![3, 8], vec![1, 24], vec![2, 3, 4], vec![4, 6]] {
        let data = sc.call(
            n("numpy.zeros"),
            vec![
                common::script::t(shape.iter().map(|&d| i(d)).collect()),
                s("<f4"),
            ],
        );
        // (2, 3, 4): 2 x 2 x 2 x 3 entries.
        let expect = if shape == [2, 3, 4] {
            Expect::Value
        } else {
            Expect::Raised
        };
        sc.step(
            json!({ "call": ra(t, "setData"), "args": [r(data)] }),
            expect,
        );
        sc.call(ra(t, "getGridSize"), vec![]);
    }
    sc.raises(ra(t, "setData"), vec![fl(&[0.0; 24])]);
    // The grid size's entries in a C++ `unsigned long`, which wraps at 32 bits on Windows
    // (I-326): 2048^3 * 3 entries are 0 there, which a buffer of shape (2048, 0) has, and the
    // library refuses the grid size; the binding refuses it on Linux. 1024^3 * 3 doesn't wrap.
    for rows in [2048, 1024] {
        let data = sc.call(
            n("numpy.zeros"),
            vec![common::script::t(vec![i(rows), i(0)]), s("<f4")],
        );
        sc.raises(ra(t, "setData"), vec![r(data)]);
    }
    // equals.
    let u = sc.call(n("OCIO.Lut3DTransform"), vec![]);
    sc.call(ra(t, "equals"), vec![r(u)]);
    sc.raises(ra(t, "equals"), vec![none()]);
    sc
}

fn b(v: bool) -> Value {
    common::script::b(v)
}

/// Each class behaves as the wheel's.
#[test]
fn the_classes_behave_as_the_wheels() {
    compare(&[lut1d_script(), lut3d_script()]);
}

/// Where the wheel reads past a buffer, the port's error (U-148): `setData` of a reversed view,
/// whose values the binding reads forward from its data pointer, its last item. The transform
/// stays usable.
#[test]
fn set_data_past_the_buffer_is_the_ports_error() {
    common::start_python();
    pyo3::Python::attach(|py| {
        let code = std::ffi::CString::new(
            r#"
import PyOpenColorIO as OCIO, numpy as np

def refused(f, entry):
    try:
        f()
    except BufferError as e:
        assert entry in str(e), str(e)
    else:
        raise AssertionError("no error")

t = OCIO.Lut1DTransform()
refused(lambda: t.setData(np.arange(12, dtype=np.float32)[::-1]), "U-148")
u = OCIO.Lut3DTransform()
refused(lambda: u.setData(np.arange(24, dtype=np.float32)[::-1]), "U-148")
t.setData(np.arange(12, dtype=np.float32))
assert t.getLength() == 4
"#,
        )
        .unwrap();
        py.run(&code, None, None).unwrap_or_else(|e| panic!("{e}"));
    });
}
