// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the packing functions' own checks. Upstream has no tests of ImagePacking; the
//! oracle checks the packing in `tests/image_packing_oracle.rs`. A scanline's pixel index
//! only wraps for images of 2^31 pixels or more on Windows (I-1), which no oracle case can
//! hold, so these call the functions with such an index directly, on small buffers.

use std::sync::Arc;

use super::*;
use crate::cpu_processor::create_generic_bit_depth_helper;
use crate::open_color_types::BitDepth;

/// A one-channel plane: every channel at `offset`, `width` by `height` pixels.
fn plane(
    width: c_long,
    height: c_long,
    x_stride_bytes: isize,
    y_stride_bytes: isize,
    offset: usize,
    bit_depth_op: Arc<dyn crate::op::CpuOp>,
) -> GenericImageDesc {
    let chan = ChannelPos { buffer: 0, offset };
    GenericImageDesc {
        width,
        height,
        x_stride_bytes,
        y_stride_bytes,
        r_data: chan,
        g_data: chan,
        b_data: chan,
        a_data: None,
        bit_depth_op,
        is_rgba_packed: false,
        is_float: false,
    }
}

#[test]
fn a_scanline_reaching_past_the_buffer_is_refused() {
    // A right-to-left F32 plane of 4 by 2 pixels, from pixel 2 of row 0, the start a wrapped
    // index can give (U-15): 4 pixels from there leave the buffer's start.
    let f32_op = create_generic_bit_depth_helper(BitDepth::F32, BitDepth::F32).unwrap();
    let img = plane(4, 2, -4, 16, 12, f32_op);
    let mut bytes = [0xa5u8; 32];
    let mut rgba = [1.0f32; 16];
    let mut unused = [0.0f32; 16];

    let unpacked = <f32 as Generic>::unpack_rgba_to_image_desc(
        &img,
        &mut [&mut bytes[..]],
        &mut rgba,
        &mut unused,
        4,
        2,
    );
    assert_eq!(unpacked.unwrap_err().message(), WRAPPED_INDEX);
    assert!(bytes.iter().all(|&b| b == 0xa5), "nothing is written");

    let packed = <f32 as Generic>::pack_rgba_from_image_desc(
        &img,
        &[&bytes[..]],
        &mut unused,
        &mut rgba,
        4,
        2,
    );
    assert_eq!(packed.unwrap_err().message(), WRAPPED_INDEX);

    // The same for an integer bit depth.
    let u8_out = create_generic_bit_depth_helper(BitDepth::F32, BitDepth::Uint8).unwrap();
    let img = plane(4, 2, -1, 4, 3, u8_out);
    let mut bytes = [0xa5u8; 8];
    let mut out = [0u8; 16];
    let unpacked = <u8 as Generic>::unpack_rgba_to_image_desc(
        &img,
        &mut [&mut bytes[..]],
        &mut rgba,
        &mut out,
        4,
        2,
    );
    assert_eq!(unpacked.unwrap_err().message(), WRAPPED_INDEX);
    assert!(bytes.iter().all(|&b| b == 0xa5), "nothing is written");
}

/// The run check's bounds, exactly (U-15), as the D-2 tests check the descriptions': a run of
/// 4 items that starts at the buffer's first byte and ends at its last is inside; the same run
/// one item earlier, one byte earlier or later, or in a buffer one byte short, is not. Left to
/// right and right to left, for 1- and 4-byte channels.
#[test]
fn a_run_is_inside_up_to_the_buffers_bounds() {
    let at = |offset: isize| [Some((0, offset)), None, None, None];
    for size in [1usize, 4] {
        let s = size as isize;
        let exact = |_: usize| 4 * size;
        let short = |_: usize| 4 * size - 1;

        // Left to right: the first item at byte 0, the last ending at byte 4 * size.
        assert!(run_is_inside(&at(0), s, 4, size, exact));
        assert!(!run_is_inside(&at(0), s, 4, size, short));
        assert!(!run_is_inside(&at(-s), s, 4, size, exact));
        assert!(!run_is_inside(&at(-1), s, 4, size, exact));
        assert!(!run_is_inside(&at(1), s, 4, size, exact));

        // Right to left: the first item is the last in memory.
        assert!(run_is_inside(&at(3 * s), -s, 4, size, exact));
        assert!(!run_is_inside(&at(3 * s), -s, 4, size, short));
        assert!(!run_is_inside(&at(2 * s), -s, 4, size, exact));
        assert!(!run_is_inside(&at(3 * s - 1), -s, 4, size, exact));
        assert!(!run_is_inside(&at(3 * s + 1), -s, 4, size, exact));
    }
    // No item: nothing to reach.
    assert!(run_is_inside(&at(-1), 4, 0, 4, |_| 0));
}

#[test]
fn a_scanline_inside_the_buffer_is_written_where_its_index_says() {
    // A top-down F32 plane of 4 by 2 pixels, from pixel 2 of row 0: upstream writes pixels 2
    // and 3 of row 0, then 0 and 1 of row 1, which follow them in memory (I-1).
    let f32_op = create_generic_bit_depth_helper(BitDepth::F32, BitDepth::F32).unwrap();
    let img = plane(4, 2, 4, 16, 0, f32_op);
    let mut bytes = [0u8; 32];
    let mut rgba: Vec<f32> = (0..16).map(|v| v as f32).collect();
    let mut unused = [0.0f32; 16];
    <f32 as Generic>::unpack_rgba_to_image_desc(
        &img,
        &mut [&mut bytes[..]],
        &mut rgba,
        &mut unused,
        4,
        2,
    )
    .unwrap();
    // The plane holds red, green and blue in turn, blue last: each pixel's blue.
    let written: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_ne_bytes(*b))
        .collect();
    assert_eq!(written, [0.0, 0.0, 2.0, 6.0, 10.0, 14.0, 0.0, 0.0]);

    let mut read = [0.0f32; 16];
    <f32 as Generic>::pack_rgba_from_image_desc(&img, &[&bytes[..]], &mut unused, &mut read, 4, 2)
        .unwrap();
    let blues: Vec<f32> = read.as_chunks::<4>().0.iter().map(|p| p[2]).collect();
    assert_eq!(blues, [2.0, 6.0, 10.0, 14.0]);
}

/// Row 87,382 of a right-to-left UINT8 plane of 49,152 by 87,383 pixels: on Windows its first
/// pixel index, 87,382 * 49,152, wraps to 32,768, inside the wrapped pixel count 81,920, so the
/// row starts at pixel 32,768 of row 0 and runs 32,768 bytes before the plane
/// (src/OpenColorIO/ImagePacking.cpp:175-229 @ v2.5.2). A small buffer stands for the plane's
/// first two rows: the scanline must not touch it.
#[cfg(target_os = "windows")]
#[test]
fn a_wrapped_index_that_leaves_the_plane_is_refused() {
    const W: c_long = 49_152;
    const H: c_long = 87_383;
    let start = c_long::from(87_382 as c_int).wrapping_mul(W);
    assert_eq!(start, 32_768);
    assert_eq!(W.wrapping_mul(H), 81_920);

    let u8_out = create_generic_bit_depth_helper(BitDepth::F32, BitDepth::Uint8).unwrap();
    let img = plane(W, H, -1, W as isize, (W - 1) as usize, u8_out);
    let mut bytes = vec![0xa5u8; 2 * W as usize];
    let mut rgba = vec![0.5f32; 4 * W as usize];
    let mut out = vec![0u8; 4 * W as usize];
    let result = <u8 as Generic>::unpack_rgba_to_image_desc(
        &img,
        &mut [&mut bytes[..]],
        &mut rgba,
        &mut out,
        W as c_int,
        start,
    );
    assert_eq!(result.unwrap_err().message(), WRAPPED_INDEX);
    assert!(bytes.iter().all(|&b| b == 0xa5), "nothing is written");
}

/// A scanline whose start isn't a pixel of the image (a wrapped index, I-1) is left unwritten:
/// `UnpackRGBAToImageDesc` returns before converting or writing anything
/// (src/OpenColorIO/ImagePacking.cpp:175-178, 245-248 @ v2.5.2), while packing raises.
#[test]
fn a_scanline_outside_the_image_is_left_unwritten() {
    let f32_op = create_generic_bit_depth_helper(BitDepth::F32, BitDepth::F32).unwrap();
    let img = plane(4, 2, 4, 16, 0, f32_op);
    let mut bytes = [0xa5u8; 32];
    let mut rgba = [1.0f32; 16];
    let mut unused = [0.0f32; 16];
    for start in [-1, 8, 9, c_long::MAX] {
        <f32 as Generic>::unpack_rgba_to_image_desc(
            &img,
            &mut [&mut bytes[..]],
            &mut rgba,
            &mut unused,
            4,
            start,
        )
        .unwrap();
        assert!(bytes.iter().all(|&b| b == 0xa5), "nothing is written");
        let packed = <f32 as Generic>::pack_rgba_from_image_desc(
            &img,
            &[&bytes[..]],
            &mut unused,
            &mut rgba,
            4,
            start,
        );
        assert_eq!(
            packed.unwrap_err().message(),
            "Invalid output image position."
        );
    }

    let u8_out = create_generic_bit_depth_helper(BitDepth::F32, BitDepth::Uint8).unwrap();
    let img = plane(4, 2, 1, 4, 0, u8_out);
    let mut bytes = [0xa5u8; 8];
    let mut out = [0u8; 16];
    <u8 as Generic>::unpack_rgba_to_image_desc(
        &img,
        &mut [&mut bytes[..]],
        &mut rgba,
        &mut out,
        4,
        8,
    )
    .unwrap();
    assert!(bytes.iter().all(|&b| b == 0xa5), "nothing is written");
    assert!(out.iter().all(|&v| v == 0), "nothing is converted");
}
