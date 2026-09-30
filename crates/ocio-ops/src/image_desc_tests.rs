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

#[track_caller]
fn assert_outside<T>(result: Result<T>) {
    match result {
        Ok(_) => panic!("the description was accepted"),
        Err(e) => assert_eq!(e.message(), OUTSIDE),
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
