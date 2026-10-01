// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Deviation D-2 (`docs/deviations.md`): a description whose pixels reach outside its memory is
//! refused. Upstream has no such check, so these tests define the behaviour: every layout gets
//! a buffer that holds exactly its pixels, which the constructor accepts, and the same layout
//! one byte short, or starting one byte too early, which it refuses. The descriptions'
//! getters, and every error upstream raises, are checked against the wheel in
//! `tests/image_desc_oracle.rs`. `upstream/OpenColorIO/src` has no test of image descriptions
//! alone: its tests apply processors to them (`tests/cpu/CPUProcessor_tests.cpp`, chunk 1.2d).

use super::*;

/// The D-2 error of a packed image.
const OUTSIDE: &str =
    "PackedImageDesc Error: The strides and dimensions reach outside the image buffer.";

/// The D-2 error of a planar image.
const PLANAR_OUTSIDE: &str =
    "PlanarImageDesc Error: The strides and dimensions reach outside the image buffer.";

#[track_caller]
fn assert_outside<T>(result: Result<T>) {
    assert_error(result, OUTSIDE);
}

#[track_caller]
fn assert_planar_outside<T>(result: Result<T>) {
    assert_error(result, PLANAR_OUTSIDE);
}

#[track_caller]
fn assert_error<T>(result: Result<T>, message: &str) {
    match result {
        Ok(_) => panic!("the description was accepted"),
        Err(e) => assert_eq!(e.message(), message),
    }
}

/// Packed RGBA F32 with nothing between channels, pixels and rows: `width * height * 4` floats.
#[test]
fn tight_rgba_f32_needs_every_pixel() {
    let mut pixels = vec![0.0f32; 3 * 2 * 4];
    PackedImageDesc::new(&mut pixels, 3, 2, 4).unwrap();

    let mut short = vec![0.0f32; 3 * 2 * 4 - 1];
    assert_outside(PackedImageDesc::new(&mut short, 3, 2, 4));

    // The same pixels as raw bytes: one byte short.
    let mut bytes = [0u8; 3 * 2 * 4 * 4];
    PackedImageDesc::new(Bytes(&mut bytes[..]), 3, 2, 4).unwrap();
    assert_outside(PackedImageDesc::new(Bytes(&mut bytes[1..]), 3, 2, 4));
}

/// A bottom-up image, as upstream's `scanline_packed_custom` test describes one: the first pixel
/// to process starts the last row, and the y stride is negative.
#[test]
fn bottom_up_rows_reach_back_to_the_first_row() {
    let row = 3 * 4 * 4;
    let bytes = vec![0u8; 2 * row];
    let at = |offset| At(Bytes(&bytes[..]), offset);
    let desc = |offset| {
        PackedImageDesc::with_strides(
            at(offset),
            3,
            2,
            4,
            BitDepth::F32,
            AUTO_STRIDE,
            AUTO_STRIDE,
            -(row as isize),
        )
    };
    desc(row).unwrap();
    // One byte earlier, the last row starts one byte before the buffer.
    assert_outside(desc(row - 1));
    // One byte later, the first row ends one byte after it.
    assert_outside(desc(row + 1));
}

/// A right-to-left image with a negative x stride, and a padded one: 16-bit RGB with two bytes
/// after each channel, and rows padded to 256 bytes, as a GPU readback has them.
#[test]
fn negative_and_padded_strides() {
    // x stride -12: pixel x is 12 bytes before pixel x - 1; the first pixel is the last one.
    let (width, height, chan, x, y) = (5usize, 3usize, 4isize, -12isize, 256isize);
    let footprint = (width - 1) * 12 + (height - 1) * 256 + 2 * 4 + 2;
    let mut memory = vec![0u16; footprint / 2];
    let desc = |memory: &mut Vec<u16>, origin| {
        PackedImageDesc::with_strides(
            At(&mut memory[..], origin),
            width,
            height,
            3,
            BitDepth::Uint16,
            chan,
            x,
            y,
        )
        .map(|_| ())
    };
    let origin = (width - 1) * 12;
    desc(&mut memory, origin).unwrap();
    assert_outside(desc(&mut memory, origin - 2));
    memory.pop();
    assert_outside(desc(&mut memory, origin));
}

/// An F16 RGBA image with rows padded to 256 bytes: the last row needs only its pixels.
#[test]
fn padded_rows_need_only_the_last_rows_pixels() {
    let footprint = 256 + 3 * 4 * 2;
    let bytes = vec![0u8; footprint];
    let desc = |bytes: &[u8]| {
        PackedImageDesc::with_strides(
            Bytes(bytes),
            3,
            2,
            4,
            BitDepth::F16,
            AUTO_STRIDE,
            AUTO_STRIDE,
            256,
        )
        .map(|_| ())
    };
    desc(&bytes).unwrap();
    assert_outside(desc(&bytes[..footprint - 1]));
}

/// F32 planes with nothing between pixels and rows: each plane holds `width * height` floats,
/// and any plane one byte short is refused. The alpha plane is optional.
#[test]
fn tight_planes_need_every_pixel() {
    let (width, height) = (3, 2);
    let plane = || vec![0.0f32; width * height];
    let (mut r, mut g, mut b, mut a) = (plane(), plane(), plane(), plane());
    PlanarImageDesc::new(&mut r, &mut g, &mut b, Some(&mut a), width, height).unwrap();
    PlanarImageDesc::new(&mut r, &mut g, &mut b, None, width, height).unwrap();

    let bytes = vec![0u8; width * height * 4];
    let full = || Bytes(&bytes[..]);
    let short = || Bytes(&bytes[1..]);
    let desc = |r, g, b, a| PlanarImageDesc::new(r, g, b, a, width, height).map(|_| ());
    desc(full(), full(), full(), Some(full())).unwrap();
    assert_planar_outside(desc(short(), full(), full(), Some(full())));
    assert_planar_outside(desc(full(), short(), full(), None));
    assert_planar_outside(desc(full(), full(), short(), None));
    assert_planar_outside(desc(full(), full(), full(), Some(short())));
}

/// Four planes in one buffer, one after the other: the last plane ends at the buffer's end.
#[test]
fn planes_in_one_buffer() {
    let (width, height) = (5usize, 2usize);
    let plane = width * height * 2;
    let bytes = vec![0u8; 4 * plane];
    let desc = |bytes: &[u8], a| {
        PlanarImageDesc::in_one_buffer(
            Bytes(bytes),
            0,
            plane,
            2 * plane,
            a,
            width,
            height,
            BitDepth::Uint16,
            AUTO_STRIDE,
            AUTO_STRIDE,
        )
        .map(|_| ())
    };
    desc(&bytes, Some(3 * plane)).unwrap();
    assert_planar_outside(desc(&bytes[..4 * plane - 1], Some(3 * plane)));
    // Without alpha, the blue plane is the last one read.
    desc(&bytes[..3 * plane], None).unwrap();
}

/// A planar x stride of 0 is legal upstream: a whole row reads one value. With a y stride of 0
/// too, every pixel reads the same value, so each plane needs one value only.
#[test]
fn zero_strides_read_one_value() {
    let value = vec![0u8; 2];
    let desc = |bytes: &[u8]| {
        PlanarImageDesc::with_strides(
            Bytes(bytes),
            Bytes(bytes),
            Bytes(bytes),
            None,
            1000,
            1000,
            BitDepth::F16,
            0,
            0,
        )
        .map(|_| ())
    };
    desc(&value).unwrap();
    assert_planar_outside(desc(&value[..1]));
}

/// A bottom-up plane: the first row to process is the last one in memory.
#[test]
fn bottom_up_planes() {
    let (width, height) = (4usize, 3usize);
    let row = width;
    let bytes = vec![0u8; width * height];
    let desc = |origin| {
        PlanarImageDesc::with_strides(
            At(&bytes[..], origin),
            At(&bytes[..], origin),
            At(&bytes[..], origin),
            None,
            width,
            height,
            BitDepth::Uint8,
            AUTO_STRIDE,
            -(row as isize),
        )
        .map(|_| ())
    };
    desc((height - 1) * row).unwrap();
    assert_planar_outside(desc((height - 1) * row - 1));
    assert_planar_outside(desc((height - 1) * row + 1));
}

/// An empty alpha plane is a plane, not a missing one: only an empty R, G or B plane is upstream's
/// null pointer ("Invalid image buffer."). An alpha plane without a byte can't hold the image's
/// alpha, which D-2 refuses.
#[test]
fn an_empty_alpha_plane_reaches_outside() {
    let (width, height) = (3, 2);
    let bytes = vec![0u8; width * height * 4];
    let full = || Bytes(&bytes[..]);
    let empty = || Bytes(&bytes[..0]);
    let desc = |r, g, b, a| PlanarImageDesc::new(r, g, b, a, width, height).map(|_| ());
    assert_planar_outside(desc(full(), full(), full(), Some(empty())));
    assert_error(
        desc(empty(), full(), full(), Some(full())),
        "PlanarImageDesc Error: Invalid image buffer.",
    );
}
