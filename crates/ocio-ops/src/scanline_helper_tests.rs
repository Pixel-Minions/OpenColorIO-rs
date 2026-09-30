// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Very wide and very tall images (improvement candidate U-3, decided by the general rule of
//! `docs/deviations.md`). Upstream has no test of the scanline helper alone, and the oracle
//! refuses these sizes: through the wheel they crash or need gigabytes. So these tests define
//! the behaviour, as the D-2 tests of `image_desc_tests.rs` do: where upstream raises, the
//! port raises its message (upstream's text, or the C++ library's); where upstream would write
//! outside its buffers, the port returns its own error.
//!
//! Widths are given relative to `quarter`, the width at which `4 * width` overflows a C `long`:
//! 2^29 on Windows, 2^61 on Linux. The images are planar with zero strides, so that one value
//! per plane describes them whatever their size.

use super::*;
use crate::cpu_processor::create_generic_bit_depth_helper;
use crate::exception::ExceptionKind;
use crate::image_desc::{Bytes, PlanarImageDesc};

/// The width at which `4 * width` overflows a C `long`.
const QUARTER: c_long = 1 << (c_long::BITS - 3);

#[track_caller]
fn assert_row_error(result: Result<Option<Exception>>, message: &str) {
    match result {
        Ok(Some(e)) => assert_eq!(e.message(), message),
        other => panic!("{other:?}, expected the first row to raise {message:?}"),
    }
}

#[test]
fn a_negative_size_is_a_length_error_where_upstream_resizes() {
    let error = scratch_rows(QUARTER + 1, false, false, true, false).unwrap_err();
    assert_eq!(error.kind(), ExceptionKind::LengthError);
    assert_eq!(error.message(), VECTOR_TOO_LONG);
    // An RGBA-packed source into an RGBA F32 destination resizes nothing.
    assert!(matches!(
        scratch_rows(QUARTER + 1, true, true, false, false),
        Ok(None)
    ));
    // The largest width whose rows fit.
    assert!(matches!(
        scratch_rows(QUARTER - 1, false, false, true, false),
        Ok(None)
    ));
}

#[test]
fn an_empty_rgba_row_raises_upstreams_message() {
    // Rows of 4 * 2 * QUARTER values: 0 in a C long, so the RGBA row is null.
    assert_row_error(
        scratch_rows(2 * QUARTER, false, false, true, false),
        "Invalid output image buffer",
    );
    assert_row_error(
        scratch_rows(2 * QUARTER, false, false, true, true),
        "Invalid output image buffer.",
    );
    // An RGBA-packed source is converted into the null row: a write outside any buffer.
    assert_row_error(
        scratch_rows(2 * QUARTER, true, false, false, false),
        TOO_WIDE,
    );
}

#[test]
fn rows_too_short_are_the_ports_error() {
    // 4 * (2 * QUARTER + 1) values wrap to 4.
    assert_row_error(
        scratch_rows(2 * QUARTER + 1, false, false, true, false),
        TOO_WIDE,
    );
    assert_row_error(
        scratch_rows(2 * QUARTER + 1, true, false, false, true),
        TOO_WIDE,
    );
    // Into the destination's own rows, only a source packed through m_inBitDepthBuffer
    // overflows it.
    assert_row_error(
        scratch_rows(2 * QUARTER + 1, false, true, true, false),
        TOO_WIDE,
    );
    assert!(matches!(
        scratch_rows(2 * QUARTER + 1, false, true, true, true),
        Ok(None)
    ));
    assert!(matches!(
        scratch_rows(2 * QUARTER + 1, true, true, false, false),
        Ok(None)
    ));
}

/// A helper for F32 images in both directions.
fn f32_helper<'a>() -> GenericScanlineHelper<'a, f32, f32> {
    let cast = create_generic_bit_depth_helper(BitDepth::F32, BitDepth::F32).unwrap();
    GenericScanlineHelper::new(BitDepth::F32, cast.clone(), BitDepth::F32, cast)
}

/// A planar F32 image `width` by `height` whose planes are each one value.
fn one_value_planes(value: &mut [u8], width: c_long, height: c_long) -> PlanarImageDesc<&mut [u8]> {
    let (r, rest) = value.split_at_mut(4);
    let (g, b) = rest.split_at_mut(4);
    PlanarImageDesc::with_strides(
        Bytes(r),
        Bytes(g),
        Bytes(&mut b[..4]),
        None,
        width as usize,
        height as usize,
        BitDepth::F32,
        0,
        0,
    )
    .unwrap()
}

#[test]
fn very_wide_images_fail_before_any_row() {
    let mut values = [0u8; 12];

    // Upstream's resize raises at init.
    let mut img = one_value_planes(&mut values, QUARTER + 1, 1);
    let mut helper = f32_helper();
    let error = helper.init(&mut img).unwrap_err();
    assert_eq!(error.kind(), ExceptionKind::LengthError);
    assert_eq!(error.message(), VECTOR_TOO_LONG);

    // An empty RGBA row: init succeeds, the first row raises.
    let mut img = one_value_planes(&mut values, 2 * QUARTER, 1);
    let mut helper = f32_helper();
    helper.init(&mut img).unwrap();
    assert_eq!(
        helper.prep_rgba_scanline().unwrap_err().message(),
        "Invalid output image buffer."
    );

    // Rows too short for a row: the port's error.
    let mut img = one_value_planes(&mut values, 2 * QUARTER + 1, 1);
    let mut helper = f32_helper();
    helper.init(&mut img).unwrap();
    assert_eq!(helper.prep_rgba_scanline().unwrap_err().message(), TOO_WIDE);
}

/// After row `c_int::MAX`, upstream's `int` row index wraps; the next row is before the image.
/// A C `long` only has more rows than that where it has 64 bits.
#[cfg(target_os = "linux")]
#[test]
fn rows_past_the_int_range_fail() {
    // Packed channel by channel: the packing raises for the negative position, as upstream's
    // does before touching any memory.
    let mut values = [0u8; 12];
    let rows = c_long::from(c_int::MAX) + 2;
    let mut img = one_value_planes(&mut values, 1, rows);
    let mut helper = f32_helper();
    helper.init(&mut img).unwrap();
    helper.y_index = c_int::MAX;
    assert!(helper.prep_rgba_scanline().unwrap().is_some());
    helper.finish_rgba_scanline().unwrap();
    assert_eq!(helper.y_index, c_int::MIN);
    assert_eq!(
        helper.prep_rgba_scanline().unwrap_err().message(),
        "Invalid output image position."
    );

    // An RGBA-packed source row would be outside the image: the port's error. (Such an image
    // needs 2^31 rows of memory; the index is set as if they had been processed.)
    let mut pixels = [0f32; 4];
    let mut img = crate::image_desc::PackedImageDesc::new(&mut pixels[..], 1, 1, 4).unwrap();
    let mut helper = f32_helper();
    helper.init(&mut img).unwrap();
    helper.dst_img.as_mut().unwrap().height = rows;
    helper.y_index = c_int::MIN;
    assert_eq!(helper.prep_rgba_scanline().unwrap_err().message(), TOO_TALL);
}
