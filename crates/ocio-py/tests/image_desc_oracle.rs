// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The image descriptions of chunk 6.5a against the wheel (`ImageDesc`, `PackedImageDesc`,
//! `PlanarImageDesc`, the module's `AutoStride`):
//! - each class's surface as the oracle's walk reports it, against the wheel's py_surface;
//! - their behaviour, through the same step scripts on both sides: every constructor overload,
//!   positional and keyword (a channel order passed without its keyword taken as a channel
//!   count, I-12), the getters, `getData` and the planes' getters (a copy of the items from the
//!   data pointer, which stays live: a change to the buffer shows in the next call), each bit
//!   depth's NumPy type with values whose bits differ (NaN payloads, infinities, signed zeros,
//!   subnormals), buffers of every kind (NumPy arrays of each type and byte order, `bytes`,
//!   `bytearray`, `memoryview`, structured and read-only arrays, non-contiguous views whose data
//!   pointer the description reads from), the binding's checks and their messages (types named
//!   by `formatCodeToDtypeName`, I-23; the entry count in a C++ `long`), the library's checks,
//!   the order in which the planar constructors check their planes (from the last, I-327), and
//!   the casters' refusals;
//! - `repr`, which prints the description's address as each compiler's C++ library prints a
//!   pointer (the address the port's own, D-8), and a null one exactly;
//! - without the wheel, whose process dies or reads memory it doesn't own there, the port's
//!   errors: an `ImageDesc()` used (U-146), a buffer whose memory moved (U-147), items read
//!   past the buffer (U-148), the alpha plane of a planar image without one (U-149), a layout
//!   reaching outside its buffer (refused when made, D-2).
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, by, compare, i, l, n, none, r, ra, s, t};
use serde_json::{Value, json};

/// A NumPy array of `dtype` (NumPy's `str`, `<f4`), `shape` and `bytes` (C order).
fn arr(dtype: &str, shape: &[usize], bytes: &[u8]) -> Value {
    json!({ "array": { "dtype": dtype, "shape": shape, "bytes": ocio_testkit::oracle_values::hex(bytes) } })
}

/// The bytes of `bits`, as little-endian 32-bit items.
fn le32(bits: &[u32]) -> Vec<u8> {
    bits.iter().flat_map(|b| b.to_le_bytes()).collect()
}

/// The bytes of `bits`, as little-endian 16-bit items.
fn le16(bits: &[u16]) -> Vec<u8> {
    bits.iter().flat_map(|b| b.to_le_bytes()).collect()
}

/// float32 bits whose values differ in every way a copy could lose: NaN payloads (quiet and
/// signaling, both signs), infinities, signed zeros, subnormals, extremes, ordinary values.
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
const F16_BITS: [u16; 8] = [
    0x7e01, 0x7d03, 0xfe05, 0x7c00, 0xfc00, 0x8000, 0x0001, 0x3c00,
];

/// The classes this chunk binds.
const CLASSES: [&str; 3] = ["ImageDesc", "PackedImageDesc", "PlanarImageDesc"];

/// Each class has the wheel's surface, member for member, and `AutoStride` its value.
#[test]
fn the_classes_have_the_wheels_surface() {
    let report = common::wheel_surface();
    for c in CLASSES.into_iter().chain(["AutoStride"]) {
        let theirs = common::wheel_member(&report, c);
        let ours = common::port_member(c);
        assert!(
            c == "AutoStride" || theirs["members"].as_array().is_some_and(|m| m.len() > 3),
            "{c}: {theirs}"
        );
        assert_eq!(ours, theirs, "{c}");
    }
}

/// A step whose value is an image description, whose repr (an address) isn't compared.
fn image(sc: &mut Script, target: Value, args: Vec<Value>) -> usize {
    let k = sc.call(target, args);
    sc.unrepr(k);
    k
}

/// [`image`] with keywords.
fn image_kw(sc: &mut Script, target: Value, args: Vec<Value>, kwargs: Vec<(&str, Value)>) -> usize {
    let k = sc.call_kw(target, args, kwargs, Expect::Value);
    sc.unrepr(k);
    k
}

const PACKED_GETTERS: [&str; 10] = [
    "getBitDepth",
    "getWidth",
    "getHeight",
    "getXStrideBytes",
    "getYStrideBytes",
    "isRGBAPacked",
    "isFloat",
    "getChannelOrder",
    "getNumChannels",
    "getChanStrideBytes",
];

const PLANAR_GETTERS: [&str; 7] = [
    "getBitDepth",
    "getWidth",
    "getHeight",
    "getXStrideBytes",
    "getYStrideBytes",
    "isRGBAPacked",
    "isFloat",
];

/// `getData`'s value, and the getters, of the image of step `k`.
fn packed_getters(sc: &mut Script, k: usize) {
    let g = sc.eval(r(k), &PACKED_GETTERS);
    sc.unrepr(g);
    sc.call(ra(k, "getData"), vec![]);
}

fn auto() -> Value {
    n("OCIO.AutoStride")
}

/// `PackedImageDesc`: its constructors, getters and data.
fn packed_script() -> Script {
    let mut sc = Script::new();
    let a = sc.eval(arr("<f4", &[16], &le32(&F32_BITS)), &[]);
    // (data, width, height, numChannels).
    let p = image(
        &mut sc,
        n("OCIO.PackedImageDesc"),
        vec![r(a), i(4), i(1), i(4)],
    );
    packed_getters(&mut sc, p);
    let p3 = image(
        &mut sc,
        n("OCIO.PackedImageDesc"),
        vec![r(a), i(2), i(2), i(4)],
    );
    packed_getters(&mut sc, p3);
    let p4 = image(
        &mut sc,
        n("OCIO.PackedImageDesc"),
        vec![r(a), i(1), i(4), i(4)],
    );
    packed_getters(&mut sc, p4);
    // The data pointer stays live: a change to the array shows; a change to getData's copy
    // doesn't reach the array.
    let data = sc.call(ra(p, "getData"), vec![]);
    sc.step(
        json!({ "setitem": r(data), "index": i(0), "value": common::script::f(9.5) }),
        Expect::Value,
    );
    sc.step(
        json!({ "setitem": r(a), "index": i(1), "value": common::script::f(-2.25) }),
        Expect::Value,
    );
    sc.call(ra(p, "getData"), vec![]);
    sc.eval(r(a), &[]);
    // (data, width, height, numChannels, bitDepth, chanStrideBytes, xStrideBytes, yStrideBytes):
    // each bit depth's type.
    let a3 = sc.eval(arr("<f4", &[12], &le32(&F32_BITS[..12])), &[]);
    let backwards = sc.call(n("slice"), vec![none(), none(), i(-1)]);
    // A reversed view: its data pointer at the last item, which negative strides start from.
    let a3r = sc.call(n("operator.getitem"), vec![r(a3), r(backwards)]);
    for (data, w, h, c, x, y) in [
        (a3, 4, 1, auto(), auto(), auto()),
        (a3, 2, 2, i(4), i(12), i(24)),
        (a3, 1, 4, i(4), i(12), i(12)),
        (a3, 2, 2, i(4), i(12), auto()),
        (a3r, 1, 4, i(-4), i(-12), i(-12)),
        (a3r, 2, 2, i(-4), i(-12), i(-24)),
    ] {
        let k = image(
            &mut sc,
            n("OCIO.PackedImageDesc"),
            vec![r(data), i(w), i(h), i(3), n("OCIO.BIT_DEPTH_F32"), c, x, y],
        );
        packed_getters(&mut sc, k);
    }
    let h = sc.eval(arr("<f2", &[8], &le16(&F16_BITS)), &[]);
    let k = image(
        &mut sc,
        n("OCIO.PackedImageDesc"),
        vec![
            r(h),
            i(2),
            i(1),
            i(4),
            n("OCIO.BIT_DEPTH_F16"),
            auto(),
            auto(),
            auto(),
        ],
    );
    packed_getters(&mut sc, k);
    let u16s = sc.eval(
        arr("<u2", &[8], &le16(&[0, 1, 1023, 4095, 65535, 32768, 7, 9])),
        &[],
    );
    for bd in ["UINT10", "UINT12", "UINT16"] {
        let k = image(
            &mut sc,
            n("OCIO.PackedImageDesc"),
            vec![
                r(u16s),
                i(2),
                i(1),
                i(4),
                n(&format!("OCIO.BIT_DEPTH_{bd}")),
                auto(),
                auto(),
                auto(),
            ],
        );
        packed_getters(&mut sc, k);
    }
    let u8s = sc.eval(arr("|u1", &[8], &[0, 1, 2, 127, 128, 254, 255, 9]), &[]);
    let k = image(
        &mut sc,
        n("OCIO.PackedImageDesc"),
        vec![
            r(u8s),
            i(2),
            i(1),
            i(4),
            n("OCIO.BIT_DEPTH_UINT8"),
            auto(),
            auto(),
            auto(),
        ],
    );
    packed_getters(&mut sc, k);
    // Bit depths no type takes: Python's "Error: Unsupported bit-depth: ...".
    for bd in ["UINT14", "UINT32", "UNKNOWN"] {
        sc.raises(
            n("OCIO.PackedImageDesc"),
            vec![
                r(a),
                i(4),
                i(1),
                i(4),
                n(&format!("OCIO.BIT_DEPTH_{bd}")),
                auto(),
                auto(),
                auto(),
            ],
        );
    }
    let bd42 = sc.call(n("OCIO.BitDepth"), vec![i(42)]);
    sc.raises(
        n("OCIO.PackedImageDesc"),
        vec![r(a), i(4), i(1), i(4), r(bd42), auto(), auto(), auto()],
    );
    // (data, width, height, chanOrder) and with strides; positionally, a channel order is a
    // channel count (I-12).
    let a12 = sc.eval(arr("<f4", &[12], &le32(&F32_BITS[4..16])), &[]);
    for order in ["RGBA", "BGRA", "ABGR", "RGB", "BGR"] {
        let o = n(&format!("OCIO.CHANNEL_ORDERING_{order}"));
        let w = if order.len() == 4 { 3 } else { 4 };
        let k = image_kw(
            &mut sc,
            n("OCIO.PackedImageDesc"),
            vec![r(a12), i(w), i(1)],
            vec![("chanOrder", o.clone())],
        );
        packed_getters(&mut sc, k);
        let k = image_kw(
            &mut sc,
            n("OCIO.PackedImageDesc"),
            vec![r(a12), i(w), i(1)],
            vec![
                ("chanOrder", o.clone()),
                ("bitDepth", n("OCIO.BIT_DEPTH_F32")),
                ("chanStrideBytes", auto()),
                ("xStrideBytes", auto()),
                ("yStrideBytes", auto()),
            ],
        );
        packed_getters(&mut sc, k);
        // The value as a count: RGB (3) fits 4 pixels of 3 channels.
        let expect = if order == "RGB" {
            Expect::Value
        } else {
            Expect::Raised
        };
        let k = sc.step(
            json!({ "call": n("OCIO.PackedImageDesc"), "args": [r(a12), i(w), i(1), o] }),
            expect,
        );
        sc.unrepr(k);
    }
    let a4 = sc.eval(arr("<f4", &[4], &le32(&F32_BITS[..4])), &[]);
    let rgb = sc.call(
        n("OCIO.PackedImageDesc"),
        vec![r(a4), i(1), i(1), n("OCIO.CHANNEL_ORDERING_BGR")],
    );
    sc.unrepr(rgb);
    sc.call(ra(rgb, "getNumChannels"), vec![]);
    let order9 = sc.call(n("OCIO.ChannelOrdering"), vec![i(9)]);
    sc.raises(
        n("OCIO.PackedImageDesc"),
        vec![r(a12), i(1), i(1), r(order9)],
    );
    sc.call_kw(
        n("OCIO.PackedImageDesc"),
        vec![r(a12), i(1), i(1)],
        vec![("chanOrder", r(order9))],
        Expect::Raised,
    );
    // Every keyword.
    let k = image_kw(
        &mut sc,
        n("OCIO.PackedImageDesc"),
        vec![],
        vec![
            ("data", r(a)),
            ("width", i(2)),
            ("height", i(2)),
            ("numChannels", i(4)),
        ],
    );
    packed_getters(&mut sc, k);
    // The binding's checks: types, entries.
    for dt in [
        "<f8", ">f4", "<f2", "<i4", "<i8", "<u4", "<i2", "|i1", "|u1", "|b1", "<c8", "<c16", "|V4",
        "|S4", "<U1",
    ] {
        let bad = sc.call(n("numpy.zeros"), vec![i(16), s(dt)]);
        sc.raises(n("OCIO.PackedImageDesc"), vec![r(bad), i(4), i(1), i(4)]);
    }
    let structured = sc.call(
        n("numpy.zeros"),
        vec![i(16), l(vec![t(vec![s("x"), s("<f4")])])],
    );
    sc.raises(
        n("OCIO.PackedImageDesc"),
        vec![r(structured), i(4), i(1), i(4)],
    );
    sc.raises(n("OCIO.PackedImageDesc"), vec![r(a), i(3), i(1), i(4)]);
    sc.raises(n("OCIO.PackedImageDesc"), vec![r(a), i(5), i(1), i(4)]);
    sc.raises(n("OCIO.PackedImageDesc"), vec![r(u16s), i(2), i(1), i(4)]);
    sc.raises(
        n("OCIO.PackedImageDesc"),
        vec![
            r(a),
            i(2),
            i(1),
            i(4),
            n("OCIO.BIT_DEPTH_UINT16"),
            auto(),
            auto(),
            auto(),
        ],
    );
    sc.raises(
        n("OCIO.PackedImageDesc"),
        vec![
            r(u8s),
            i(2),
            i(1),
            i(4),
            n("OCIO.BIT_DEPTH_UINT16"),
            auto(),
            auto(),
            auto(),
        ],
    );
    sc.raises(
        n("OCIO.PackedImageDesc"),
        vec![
            r(h),
            i(2),
            i(1),
            i(4),
            n("OCIO.BIT_DEPTH_F32"),
            auto(),
            auto(),
            auto(),
        ],
    );
    // The library's checks.
    let empty = sc.call(n("numpy.zeros"), vec![i(0), s("<f4")]);
    for (data, w, h, c) in [
        (empty, 0, 1, 4),
        (empty, 2, 1, 0),
        (a, -1, -4, 4),
        (a, 16, 1, 1),
        (a, 4, 4, 1),
        (a, 2, 1, 8),
        (a, 8, 1, 2),
    ] {
        sc.raises(n("OCIO.PackedImageDesc"), vec![r(data), i(w), i(h), i(c)]);
    }
    for (c, x, y) in [
        (i(2), i(16), i(32)),
        (i(4), i(8), i(32)),
        (i(4), i(16), i(8)),
        (i(4), i(-16), i(16)),
        (auto(), i(-16), i(-16)),
    ] {
        sc.step(
            json!({ "call": n("OCIO.PackedImageDesc"), "args": [r(a), i(2), i(2), i(4), n("OCIO.BIT_DEPTH_F32"), c, x, y] }),
            Expect::Raised,
        );
    }
    // The casters.
    for bad in [l(vec![common::script::f(1.0); 16]), none(), s("abcd"), i(3)] {
        sc.raises(n("OCIO.PackedImageDesc"), vec![bad, i(4), i(1), i(4)]);
    }
    sc.raises(
        n("OCIO.PackedImageDesc"),
        vec![r(a), common::script::f(4.0), i(1), i(4)],
    );
    sc.raises(n("OCIO.PackedImageDesc"), vec![]);
    sc.raises(n("OCIO.PackedImageDesc"), vec![r(a), i(4), i(1)]);
    // The entry count in a C++ `long`, which wraps at 32 bits on Windows (I-326): "expected 0
    // entries" there, 2^34 on Linux.
    sc.raises(
        n("OCIO.PackedImageDesc"),
        vec![r(a), i(65536), i(65536), i(4)],
    );
    sc
}

/// Buffers other than C-contiguous NumPy arrays.
fn buffers_script() -> Script {
    let mut sc = Script::new();
    // bytes and bytearray are uint8; a memoryview cast to float; an array's own memoryview.
    sc.raises(
        n("OCIO.PackedImageDesc"),
        vec![by(&[0; 16]), i(4), i(1), i(4)],
    );
    let u8_image = image(
        &mut sc,
        n("OCIO.PackedImageDesc"),
        vec![
            by(&[1, 2, 3, 4, 5, 6, 7, 8]),
            i(2),
            i(1),
            i(4),
            n("OCIO.BIT_DEPTH_UINT8"),
            auto(),
            auto(),
            auto(),
        ],
    );
    packed_getters(&mut sc, u8_image);
    let ba = sc.call(n("bytearray"), vec![by(&le32(&F32_BITS[..8]))]);
    sc.raises(n("OCIO.PackedImageDesc"), vec![r(ba), i(2), i(1), i(4)]);
    let a = sc.eval(arr("<f4", &[16], &le32(&F32_BITS)), &[]);
    let mv = sc.eval(ra(a, "data"), &[]);
    sc.unrepr(mv);
    let k = image(
        &mut sc,
        n("OCIO.PackedImageDesc"),
        vec![r(mv), i(4), i(1), i(4)],
    );
    packed_getters(&mut sc, k);
    // A read-only array.
    let ro = sc.call(
        n("numpy.frombuffer"),
        vec![by(&le32(&F32_BITS[..8])), s("<f4")],
    );
    let k = image(
        &mut sc,
        n("OCIO.PackedImageDesc"),
        vec![r(ro), i(2), i(1), i(4)],
    );
    packed_getters(&mut sc, k);
    // Views: the description reads from the view's data pointer, its strides aside.
    let step2 = sc.call(n("slice"), vec![none(), none(), i(2)]);
    let odd = sc.call(n("slice"), vec![i(1), none(), i(2)]);
    let even = sc.call(n("operator.getitem"), vec![r(a), r(step2)]);
    let k = image(
        &mut sc,
        n("OCIO.PackedImageDesc"),
        vec![r(even), i(2), i(1), i(4)],
    );
    packed_getters(&mut sc, k);
    let odds = sc.call(n("operator.getitem"), vec![r(a), r(odd)]);
    let k = image(
        &mut sc,
        n("OCIO.PackedImageDesc"),
        vec![r(odds), i(2), i(1), i(4)],
    );
    packed_getters(&mut sc, k);
    let grid = sc.call(ra(a, "reshape"), vec![i(4), i(4)]);
    let transposed = sc.eval(ra(grid, "T"), &[]);
    let k = image(
        &mut sc,
        n("OCIO.PackedImageDesc"),
        vec![r(transposed), i(4), i(1), i(4)],
    );
    packed_getters(&mut sc, k);
    let two_d = image(
        &mut sc,
        n("OCIO.PackedImageDesc"),
        vec![r(grid), i(4), i(1), i(4)],
    );
    packed_getters(&mut sc, two_d);
    let scalar = sc.call(n("numpy.float32"), vec![common::script::f(1.5)]);
    sc.raises(n("OCIO.PackedImageDesc"), vec![r(scalar), i(1), i(1), i(4)]);
    sc
}

/// `PlanarImageDesc`: its constructors, getters and planes.
fn planar_script() -> Script {
    let mut sc = Script::new();
    let planes: Vec<usize> = (0..4)
        .map(|k| sc.eval(arr("<f4", &[4], &le32(&F32_BITS[4 * k..4 * k + 4])), &[]))
        .collect();
    let [pr, pg, pb, pa] = [planes[0], planes[1], planes[2], planes[3]];
    let getters = |sc: &mut Script, k: usize, alpha: bool| {
        let g = sc.eval(r(k), &PLANAR_GETTERS);
        sc.unrepr(g);
        for get in ["getRData", "getGData", "getBData"] {
            sc.call(ra(k, get), vec![]);
        }
        if alpha {
            sc.call(ra(k, "getAData"), vec![]);
        }
    };
    for (w, h) in [(4, 1), (2, 2), (1, 4)] {
        let k = image(
            &mut sc,
            n("OCIO.PlanarImageDesc"),
            vec![r(pr), r(pg), r(pb), i(w), i(h)],
        );
        getters(&mut sc, k, false);
        let k = image(
            &mut sc,
            n("OCIO.PlanarImageDesc"),
            vec![r(pr), r(pg), r(pb), r(pa), i(w), i(h)],
        );
        getters(&mut sc, k, true);
    }
    let backwards = sc.call(n("slice"), vec![none(), none(), i(-1)]);
    let reversed: Vec<usize> = planes
        .iter()
        .map(|&p| sc.call(n("operator.getitem"), vec![r(p), r(backwards)]))
        .collect();
    // The planes' getters read forward from each plane's data pointer: of reversed planes, past
    // them (U-148), so only the description's getters.
    for (planes, x, y, read) in [
        (&planes, auto(), auto(), true),
        (&planes, i(4), i(8), true),
        (&reversed, i(-4), i(-8), false),
    ] {
        let rgb: Vec<Value> = planes[..3].iter().map(|&p| r(p)).collect();
        let k = image(
            &mut sc,
            n("OCIO.PlanarImageDesc"),
            [
                rgb.clone(),
                vec![i(2), i(2), n("OCIO.BIT_DEPTH_F32"), x.clone(), y.clone()],
            ]
            .concat(),
        );
        let rgba: Vec<Value> = planes.iter().map(|&p| r(p)).collect();
        let k4 = image(
            &mut sc,
            n("OCIO.PlanarImageDesc"),
            [rgba, vec![i(2), i(2), n("OCIO.BIT_DEPTH_F32"), x, y]].concat(),
        );
        if read {
            getters(&mut sc, k, false);
            getters(&mut sc, k4, true);
        } else {
            for k in [k, k4] {
                let g = sc.eval(r(k), &PLANAR_GETTERS);
                sc.unrepr(g);
            }
        }
    }
    // A plane of one item that repeats (a stride of 0): its getter's one item is its span.
    let base = sc.eval(arr("<f4", &[8], &le32(&F32_BITS[..8])), &[]);
    let tail = sc.call(n("slice"), vec![i(5), none()]);
    let from_five = sc.call(n("operator.getitem"), vec![r(base), r(tail)]);
    let one = sc.call_kw(
        n("numpy.lib.stride_tricks.as_strided"),
        vec![r(from_five)],
        vec![("shape", t(vec![i(1)])), ("strides", t(vec![i(0)]))],
        Expect::Value,
    );
    let k = image(
        &mut sc,
        n("OCIO.PlanarImageDesc"),
        vec![r(one), r(one), r(one), i(1), i(1)],
    );
    getters(&mut sc, k, false);
    let halves: Vec<usize> = (0..4)
        .map(|k| sc.eval(arr("<f2", &[2], &le16(&F16_BITS[2 * k..2 * k + 2])), &[]))
        .collect();
    let k = image(
        &mut sc,
        n("OCIO.PlanarImageDesc"),
        vec![
            r(halves[0]),
            r(halves[1]),
            r(halves[2]),
            r(halves[3]),
            i(2),
            i(1),
            n("OCIO.BIT_DEPTH_F16"),
            auto(),
            auto(),
        ],
    );
    getters(&mut sc, k, true);
    let shorts: Vec<usize> = (0..3)
        .map(|k| sc.eval(arr("<u2", &[2], &le16(&[k as u16, 1023 - k as u16])), &[]))
        .collect();
    let k = image(
        &mut sc,
        n("OCIO.PlanarImageDesc"),
        vec![
            r(shorts[0]),
            r(shorts[1]),
            r(shorts[2]),
            i(2),
            i(1),
            n("OCIO.BIT_DEPTH_UINT10"),
            auto(),
            auto(),
        ],
    );
    getters(&mut sc, k, false);
    // One array for every plane.
    let k = image(
        &mut sc,
        n("OCIO.PlanarImageDesc"),
        vec![r(pr), r(pr), r(pr), r(pr), i(4), i(1)],
    );
    getters(&mut sc, k, true);
    // Keywords.
    let k = image_kw(
        &mut sc,
        n("OCIO.PlanarImageDesc"),
        vec![],
        vec![
            ("rData", r(pr)),
            ("gData", r(pg)),
            ("bData", r(pb)),
            ("aData", r(pa)),
            ("width", i(4)),
            ("height", i(1)),
        ],
    );
    getters(&mut sc, k, true);
    // The planes' checks, from the last plane to the first (I-327): the error each first
    // failing plane gives.
    let f64s = sc.call(n("numpy.zeros"), vec![i(4), s("<f8")]);
    let short = sc.call(n("numpy.zeros"), vec![i(3), s("<f4")]);
    for args in [
        vec![r(f64s), r(short), r(pb), i(4), i(1)],
        vec![r(short), r(pg), r(f64s), i(4), i(1)],
        vec![r(pr), r(f64s), r(short), i(4), i(1)],
        vec![r(f64s), r(pg), r(pb), r(short), i(4), i(1)],
        vec![r(short), r(pg), r(pb), r(f64s), i(4), i(1)],
        vec![r(short), r(f64s), r(pb), r(pa), i(4), i(1)],
    ] {
        sc.raises(n("OCIO.PlanarImageDesc"), args);
    }
    sc.raises(
        n("OCIO.PlanarImageDesc"),
        vec![
            r(f64s),
            r(pg),
            r(pb),
            i(4),
            i(1),
            n("OCIO.BIT_DEPTH_UINT14"),
            i(4),
            i(16),
        ],
    );
    sc.raises(
        n("OCIO.PlanarImageDesc"),
        vec![
            r(pr),
            r(pg),
            r(pb),
            i(4),
            i(1),
            n("OCIO.BIT_DEPTH_UINT16"),
            auto(),
            auto(),
        ],
    );
    // The library's checks.
    for (w, h, x, y) in [
        (0, 4, auto(), auto()),
        (4, 1, auto(), i(8)),
        (2, 2, i(4), i(4)),
        (4, -1, auto(), auto()),
    ] {
        sc.step(
            json!({ "call": n("OCIO.PlanarImageDesc"), "args": [r(pr), r(pg), r(pb), i(w), i(h), n("OCIO.BIT_DEPTH_F32"), x, y] }),
            Expect::Raised,
        );
    }
    sc.raises(n("OCIO.PlanarImageDesc"), vec![r(pr), r(pg), i(4), i(1)]);
    // `width * height` in a C++ `long` (I-326).
    sc.raises(
        n("OCIO.PlanarImageDesc"),
        vec![r(pr), r(pg), r(pb), i(65536), i(65536)],
    );
    sc.raises(
        n("OCIO.PlanarImageDesc"),
        vec![r(pr), r(pg), none(), i(4), i(1)],
    );
    sc
}

/// `ImageDesc` itself, and the base class's methods on its subclasses.
fn base_script() -> Script {
    let mut sc = Script::new();
    let e = sc.call(n("OCIO.ImageDesc"), vec![]);
    sc.repr(r(e));
    sc.call(n("type"), vec![r(e)]);
    let a = sc.eval(arr("<f4", &[8], &le32(&F32_BITS[..8])), &[]);
    let p = image(
        &mut sc,
        n("OCIO.PackedImageDesc"),
        vec![r(a), i(2), i(1), i(4)],
    );
    for get in PLANAR_GETTERS {
        sc.call(n(&format!("OCIO.ImageDesc.{get}")), vec![r(p)]);
    }
    // Not an instance of the class (an image description's repr, its address, would be in the
    // TypeError).
    sc.raises(n("OCIO.PackedImageDesc.getData"), vec![i(1)]);
    sc.raises(n("OCIO.PlanarImageDesc.getRData"), vec![none()]);
    sc.raises(n("OCIO.ImageDesc.getWidth"), vec![s("x")]);
    sc.raises(n("OCIO.ImageDesc"), vec![i(1)]);
    // A Python subclass.
    let sub = sc.call(
        n("type"),
        vec![
            s("Sub"),
            json!({ "tuple": [n("OCIO.PackedImageDesc")] }),
            json!({ "dict": [[s("__module__"), s("probe")]] }),
        ],
    );
    let k = image(&mut sc, r(sub), vec![r(a), i(2), i(1), i(4)]);
    packed_getters(&mut sc, k);
    sc.call(n("isinstance"), vec![r(k), n("OCIO.ImageDesc")]);
    let _ = b;
    sc
}

/// Each class behaves as the wheel's.
#[test]
fn the_classes_behave_as_the_wheels() {
    let scripts = [
        packed_script(),
        buffers_script(),
        planar_script(),
        base_script(),
    ];
    compare(&scripts);
}

/// `repr` prints the description's address as the wheel's compiler prints a pointer: MSVC's 16
/// upper-case hexadecimal digits on Windows, libstdc++'s `0x` and lower-case digits on Linux;
/// a null description's exactly.
#[test]
fn repr_prints_an_address_as_the_wheels_cpp_library() {
    let mut sc = Script::new();
    let e = sc.call(n("OCIO.ImageDesc"), vec![]);
    sc.repr(r(e));
    let a = sc.eval(arr("<f4", &[8], &le32(&F32_BITS[..8])), &[]);
    let p = image(
        &mut sc,
        n("OCIO.PackedImageDesc"),
        vec![r(a), i(2), i(1), i(4)],
    );
    let rp = sc.repr(r(p));
    let theirs = common::wheel(&[sc.json()]).remove(0);
    let ours = common::port(&sc.json());
    let text = |report: &Value, k: usize| -> String {
        String::from_utf8(ocio_testkit::oracle_values::bytes(
            &report["steps"][k]["value"]["str"],
        ))
        .unwrap()
    };
    assert_eq!(text(&ours, 1), text(&theirs, 1));
    let pointer = |t: &str| {
        if cfg!(windows) {
            t.len() == 16
                && t.chars()
                    .all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c))
        } else {
            t.strip_prefix("0x").is_some_and(|h| {
                !h.is_empty()
                    && h.chars()
                        .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
            })
        }
    };
    assert!(pointer(&text(&theirs, rp)), "{}", text(&theirs, rp));
    assert!(pointer(&text(&ours, rp)), "{}", text(&ours, rp));
    assert_ne!(text(&ours, rp), text(&ours, 1));
}

/// An image description's constructor reads none of the buffer's memory, as the wheel's (which
/// makes one over an array while another thread's in-place `applyRGBA` of it runs): it runs
/// while another thread's call holds the memory to write. `getData`, which reads the items, is
/// refused meanwhile (U-150), and runs once that call is done.
#[test]
fn a_constructor_reads_no_memory() {
    use pyo3::prelude::*;
    use std::sync::{Arc, Barrier};
    common::start_python();
    let array = pyo3::Python::attach(|py| {
        py.eval(c"__import__('numpy').arange(16, dtype='<f4')", None, None)
            .unwrap()
            .unbind()
    });
    let held = Arc::new(Barrier::new(2));
    let tried = Arc::new(Barrier::new(2));
    let (h, t) = (held.clone(), tried.clone());
    let a = pyo3::Python::attach(|py| array.clone_ref(py));
    let worker = std::thread::spawn(move || {
        h.wait();
        let made = pyo3::Python::attach(|py| {
            let globals = pyo3::types::PyDict::new(py);
            globals.set_item("a", a.bind(py)).unwrap();
            let code = c"
import PyOpenColorIO as OCIO
d = OCIO.PackedImageDesc(a, 2, 2, 4)
p = OCIO.PlanarImageDesc(a[:4], a[4:8], a[8:12], a[12:], 2, 2)
try:
    d.getData()
except BufferError as e:
    assert 'U-150' in str(e), str(e)
else:
    raise AssertionError('getData read memory another call writes')
";
            py.run(code, Some(&globals), None)
                .map(|_| globals.get_item("d").unwrap().unwrap().unbind())
        });
        t.wait();
        made
    });
    pyo3::Python::attach(|py| {
        let buffer = ocio_py_ffi::Buffer::request(array.bind(py)).unwrap();
        ocio_py_ffi::with_bytes(py, &[(&buffer, true)], |_, _| {
            held.wait();
            tried.wait();
        })
        .unwrap();
    });
    let d = worker.join().unwrap().unwrap_or_else(|e| panic!("{e}"));
    pyo3::Python::attach(|py| {
        let data = d.bind(py).call_method0("getData").unwrap();
        let expected = array.bind(py);
        assert!(
            data.call_method1("__eq__", (expected,))
                .unwrap()
                .call_method0("all")
                .unwrap()
                .is_truthy()
                .unwrap()
        );
    });
}

/// Runs `code` in the port's module (the embedded interpreter), which asserts what it expects.
fn port_run(code: &str) {
    common::start_python();
    pyo3::Python::attach(|py| {
        let code = std::ffi::CString::new(code).unwrap();
        py.run(&code, None, None).unwrap_or_else(|e| panic!("{e}"));
    });
}

/// Where the wheel's process dies or reads memory it doesn't own, the port's errors: an
/// `ImageDesc()`'s getters (U-146); a buffer whose memory moved since the description was made
/// (U-147: an exporter giving new memory to each request); a plane's items read past its buffer
/// (U-148: getRData of a reversed plane, which reads forward from its data pointer, the last
/// item); the alpha plane of a planar image without one (U-149). Each is the error the register
/// names, and the description stays usable where it can. And a layout reaching outside its
/// buffer, which the wheel makes and then reads and writes past, refused when made (D-2).
#[test]
fn the_ports_errors_where_the_wheel_reads_memory_it_doesnt_own() {
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

e = OCIO.ImageDesc()
for m in ["getBitDepth", "getWidth", "getHeight", "getXStrideBytes", "getYStrideBytes",
          "isRGBAPacked", "isFloat"]:
    refused(getattr(e, m), ValueError, "U-146")

class Moving:
    def __init__(self):
        self.keep = []
    def __buffer__(self, flags):
        b = bytearray(32)
        self.keep.append(b)
        return memoryview(b).cast("f")

m = Moving()
p = OCIO.PackedImageDesc(m, 2, 1, 4)
assert len(m.keep) == 1 and p.getWidth() == 2
refused(p.getData, BufferError, "U-147")
assert len(m.keep) == 2

planes = [np.arange(4, dtype=np.float32)[::-1] for _ in range(4)]
q = OCIO.PlanarImageDesc(*planes, 2, 2, OCIO.BIT_DEPTH_F32, -4, -8)
assert q.getXStrideBytes() == -4
for get in ["getRData", "getGData", "getBData", "getAData"]:
    refused(getattr(q, get), BufferError, "U-148")

# At the span's end: planes whose two items repeat (a stride of 0) span one item, and their
# getters copy two items forward from it: the second is outside the span, by one item.
two = np.lib.stride_tricks.as_strided(np.arange(8, dtype=np.float32)[5:], shape=(2,), strides=(0,))
q2 = OCIO.PlanarImageDesc(two, two, two, 2, 1, OCIO.BIT_DEPTH_F32, 0, 0)
for get in ["getRData", "getGData", "getBData"]:
    refused(getattr(q2, get), BufferError, "U-148")

flat = [np.arange(4, dtype=np.float32) + k for k in range(3)]
r = OCIO.PlanarImageDesc(*flat, 2, 2)
refused(r.getAData, ValueError, "U-149")

# A layout reaching outside the buffer, which the wheel's binding doesn't check (it checks the
# entry count) and its CPU processor reads and writes past: refused when made (D-2).
a = np.arange(16, dtype=np.float32)
for args in [(a, 2, 2, 4, OCIO.BIT_DEPTH_F32, 4, 16, 64),
             (a, 2, 2, 4, OCIO.BIT_DEPTH_F32, 8, 32, 64),
             (a[:8], 1, 2, 4, OCIO.BIT_DEPTH_F32, OCIO.AutoStride, OCIO.AutoStride, 64),
             # One value past the buffer's end.
             (a[:8], 2, 1, 4, OCIO.BIT_DEPTH_F32, 4, 20, 40)]:
    try:
        OCIO.PackedImageDesc(*args)
    except OCIO.Exception as e:
        assert str(e) == ("PackedImageDesc Error: The strides and dimensions reach outside "
                          "the image buffer."), str(e)
    else:
        raise AssertionError("no error")
for planes in [(a[:4], 2, 2, OCIO.BIT_DEPTH_F32, 4, 16),
               # One value past each plane's end.
               (a[:2], 2, 1, OCIO.BIT_DEPTH_F32, 8, 16)]:
    try:
        OCIO.PlanarImageDesc(planes[0], planes[0], planes[0], *planes[1:])
    except OCIO.Exception as e:
        assert str(e) == ("PlanarImageDesc Error: The strides and dimensions reach outside "
                          "the image buffer."), str(e)
    else:
        raise AssertionError("no error")
assert np.array_equal(r.getBData(), flat[2])
"#,
    );
}
