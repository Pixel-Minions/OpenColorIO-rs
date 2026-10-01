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

    // Packed channel by channel into an RGBA-packed F32 destination, whose row is the RGBA
    // row: upstream computes that row's address, before the image, and the packing raises for
    // the negative position before touching any memory.
    let mut values = [0u8; 12];
    let src = one_value_planes(&mut values, 1, 1);
    let mut pixels = [0f32; 4];
    let mut dst = crate::image_desc::PackedImageDesc::new(&mut pixels[..], 1, 1, 4).unwrap();
    let mut helper = f32_helper();
    helper.init_src_dst(&src, &mut dst).unwrap();
    assert!(helper.use_dst_buffer);
    helper.src_img.as_mut().unwrap().height = rows;
    helper.dst_img.as_mut().unwrap().height = rows;
    helper.y_index = c_int::MIN;
    assert_eq!(
        helper.prep_rgba_scanline().unwrap_err().message(),
        "Invalid output image position."
    );
}

#[test]
fn a_row_past_max_size_is_a_length_error() {
    // `std::vector<T>::resize` past `max_size()`, `PTRDIFF_MAX / sizeof(T)` values in libstdc++.
    let limit = isize::MAX as usize / size_of::<f32>();
    let mut rows = Vec::<f32>::new();
    let error = std_resize(&mut rows, limit + 1, 0.0).unwrap_err();
    assert_eq!(error.kind(), ExceptionKind::LengthError);
    assert_eq!(error.message(), VECTOR_TOO_LONG);
    let mut bytes = Vec::<u8>::new();
    let error = std_resize(&mut bytes, isize::MAX as usize + 1, 0).unwrap_err();
    assert_eq!(error.kind(), ExceptionKind::LengthError);
    // Within it, the values are there.
    std_resize(&mut rows, 3, 0.5).unwrap();
    assert_eq!(rows, [0.5; 3]);
}

/// The review's probe: a planar F32 image of 2^60 by 1 pixels with zero strides. Through the
/// Linux wheel, `init` raises `ValueError` "vector::_M_default_append", the libstdc++ message
/// of the rows' resize (`4 * width` floats are past `max_size()`). A Windows `long` can't hold
/// the width.
#[cfg(target_os = "linux")]
#[test]
fn rows_past_max_size_raise_at_init() {
    let mut values = [0u8; 12];
    let mut img = one_value_planes(&mut values, 1 << 60, 1);
    let mut helper = f32_helper();
    let error = helper.init(&mut img).unwrap_err();
    assert_eq!(error.kind(), ExceptionKind::LengthError);
    assert_eq!(error.message(), "vector::_M_default_append");
}

/// The environment variable that makes [`rows_that_cant_be_allocated_raise_bad_alloc`] run as
/// its child, under a memory limit.
#[cfg(target_os = "linux")]
const BAD_ALLOC_CHILD: &str = "OCIO_RS_SCANLINE_BAD_ALLOC_CHILD";

/// The review's probe: a planar F32 image of 2^32 by 1 pixels with zero strides, under
/// `ulimit -v 4000000`. Through the Linux wheel, `init` raises `MemoryError` "std::bad_alloc":
/// the rows' 64 GiB can't be had. The test runs itself again under that limit (Linux
/// overcommits memory otherwise), where the helper raises the same.
#[cfg(target_os = "linux")]
#[test]
fn rows_that_cant_be_allocated_raise_bad_alloc() {
    if std::env::var_os(BAD_ALLOC_CHILD).is_some() {
        let mut values = [0u8; 12];
        let mut img = one_value_planes(&mut values, 1 << 32, 1);
        let mut helper = f32_helper();
        let error = helper.init(&mut img).unwrap_err();
        println!("child: {:?} {:?}", error.kind(), error.message());
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let command = format!(
        "ulimit -v 4000000 && exec \"{}\" --exact \
         scanline_helper::tests::rows_that_cant_be_allocated_raise_bad_alloc --nocapture \
         --test-threads 1",
        exe.display()
    );
    let output = std::process::Command::new("sh")
        .args(["-c", &command])
        .env(BAD_ALLOC_CHILD, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("child: BadAlloc \"std::bad_alloc\""),
        "{stdout}"
    );
}

#[test]
fn rows_are_sized_only_where_upstream_sizes_them() {
    // In place, an RGBA-packed F32 image is its own RGBA row: no row at all.
    let mut pixels = [0f32; 8];
    let mut img = crate::image_desc::PackedImageDesc::new(&mut pixels[..], 2, 1, 4).unwrap();
    let mut helper = f32_helper();
    helper.init(&mut img).unwrap();
    assert_eq!(helper.rgba_float_buffer.capacity(), 0);
    assert_eq!(helper.in_bit_depth_buffer.capacity(), 0);
    assert_eq!(helper.out_bit_depth_buffer.capacity(), 0);
    // Processing it doesn't size any either (the rows are aligned).
    while helper.prep_rgba_scanline().unwrap().is_some() {
        helper.finish_rgba_scanline().unwrap();
    }
    assert_eq!(helper.own_rgba.capacity(), 0);

    // From planes into an RGBA-packed F32 image: m_inBitDepthBuffer only.
    let mut values = [0u8; 12];
    let src = one_value_planes(&mut values, 2, 1);
    let mut pixels = [0f32; 8];
    let mut dst = crate::image_desc::PackedImageDesc::new(&mut pixels[..], 2, 1, 4).unwrap();
    let mut helper = f32_helper();
    helper.init_src_dst(&src, &mut dst).unwrap();
    assert_eq!(helper.in_bit_depth_buffer.len(), 8);
    assert_eq!(helper.rgba_float_buffer.capacity(), 0);
    assert_eq!(helper.out_bit_depth_buffer.capacity(), 0);

    // From an RGBA-packed image into planes: m_rgbaFloatBuffer and m_outBitDepthBuffer.
    let pixels = [0f32; 8];
    let src = crate::image_desc::PackedImageDesc::new(&pixels[..], 2, 1, 4).unwrap();
    let mut values = [0u8; 12];
    let mut dst = one_value_planes(&mut values, 2, 1);
    let mut helper = f32_helper();
    helper.init_src_dst(&src, &mut dst).unwrap();
    assert_eq!(helper.in_bit_depth_buffer.capacity(), 0);
    assert_eq!(helper.rgba_float_buffer.len(), 8);
    assert_eq!(helper.out_bit_depth_buffer.len(), 8);
}
