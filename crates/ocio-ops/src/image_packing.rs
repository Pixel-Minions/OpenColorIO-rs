// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Packing and unpacking between an image in any layout and the packed RGBA F32 scanlines the
//! ops process: a port of `src/OpenColorIO/ImagePacking.h` and `ImagePacking.cpp` @ v2.5.2.
//!
//! Upstream walks `Type *` pointers through the image with byte strides. The port walks byte
//! offsets through the description's buffers (`GenericImageDesc`), reading and writing each
//! value with the machine's byte order, so that any stride works at any alignment. The offsets
//! and the pixel counts use upstream's integer types: `long` (32 bits on Windows) and `int`.

use core::ffi::{c_int, c_long};

use crate::bit_depth_utils::ChannelType;
use crate::exception::{Exception, Result};
use crate::image_desc::{ChannelPos, GenericImageDesc};
use crate::op::{Pixels, PixelsMut};

/// Where the channels of the pixel at `image_pixel_start_index` are: each channel's buffer and
/// byte offset (`None` for a missing alpha). `None` when the position isn't in the image.
///
/// Port of the part the four functions share (src/OpenColorIO/ImagePacking.cpp:33-63, 103-133,
/// 173-203, 243-273 @ v2.5.2). `imgWidth * imgHeight` is a `long` product, and so is the
/// scanline helper's start index: on Windows both wrap for images of 2^31 pixels or more, so a
/// scanline's position can count as outside, or land at the wrong pixel (docs/improvements.md,
/// I-1).
fn start_positions(
    img: &GenericImageDesc,
    image_pixel_start_index: c_long,
) -> Option<[Option<(usize, isize)>; 4]> {
    let img_width = img.width;
    let img_height = img.height;
    let img_pixels = img_width.wrapping_mul(img_height);

    if image_pixel_start_index < 0 || image_pixel_start_index >= img_pixels {
        return None;
    }

    let x_stride_bytes = img.x_stride_bytes;
    let y_stride_bytes = img.y_stride_bytes;

    let y_index = image_pixel_start_index / img_width;
    let x_index = image_pixel_start_index % img_width;

    // Figure out our initial positions: the row, then the pixel in it.
    let position = |channel: ChannelPos| {
        let row =
            (channel.offset as isize).wrapping_add(y_stride_bytes.wrapping_mul(y_index as isize));
        (
            channel.buffer,
            row.wrapping_add(x_stride_bytes.wrapping_mul(x_index as isize)),
        )
    };
    Some([
        Some(position(img.r_data)),
        Some(position(img.g_data)),
        Some(position(img.b_data)),
        img.a_data.map(position),
    ])
}

/// The port's error where a scanline's pixel index has wrapped (I-1) and the scanline's pixels
/// would reach outside the image's buffers, which upstream reads or writes (improvement
/// candidate U-15).
pub const WRAPPED_INDEX: &str =
    "ImagePacking Error: The image has too many pixels: the scanline's pixel index overflows.";

/// Whether `count` pixels from `positions`, `x_stride_bytes` apart, are inside their buffers,
/// whose lengths `buffer_len` gives, for channels of `size` bytes.
///
/// A scanline's run of `width` pixels from the start of a row always is: the description's
/// bounds check sees to it (D-2). A run from a wrapped pixel index (I-1) starts inside a row and
/// continues past its end, where it can leave the memory (U-15): upstream reads or writes there,
/// and the port returns [`WRAPPED_INDEX`] instead, before touching any pixel of the scanline.
fn run_is_inside(
    positions: &[Option<(usize, isize)>; 4],
    x_stride_bytes: isize,
    count: c_int,
    size: usize,
    buffer_len: impl Fn(usize) -> usize,
) -> bool {
    let Some(last) = pixel_count(count).checked_sub(1) else {
        return true;
    };
    positions.iter().flatten().all(|&(buffer, offset)| {
        let first = offset as i128;
        let end = first + last as i128 * x_stride_bytes as i128;
        first.min(end) >= 0 && first.max(end) + size as i128 <= buffer_len(buffer) as i128
    })
}

/// Packing and unpacking for one channel type (`BitDepthInfo<BD>::Type`). `f32` has its own
/// implementation, as upstream specializes `Generic<float>`.
///
/// Port of `Generic<Type>` (src/OpenColorIO/ImagePacking.h:49-63,
/// src/OpenColorIO/ImagePacking.cpp:21-309 @ v2.5.2).
pub trait Generic: ChannelType {
    /// Reads `output_buffer_size` pixels of `src_img`, from pixel `image_pixel_start_index`
    /// along its row, into `in_bit_depth_buffer` as RGBA (alpha 0 when the image has none),
    /// then converts them into `output_buffer` with the image's bit-depth op. A run that would
    /// reach outside the buffers returns [`WRAPPED_INDEX`] (U-15).
    ///
    /// Port of `Generic<Type>::PackRGBAFromImageDesc` (src/OpenColorIO/ImagePacking.cpp:21-89
    /// @ v2.5.2). Its null-buffer check has no Rust counterpart: slices aren't null.
    fn pack_rgba_from_image_desc(
        src_img: &GenericImageDesc,
        src_buffers: &[&[u8]],
        in_bit_depth_buffer: &mut [Self],
        output_buffer: &mut [f32],
        output_buffer_size: c_int,
        image_pixel_start_index: c_long,
    ) -> Result<()> {
        let pixels_copied = gather(
            src_img,
            src_buffers,
            in_bit_depth_buffer,
            output_buffer_size,
            image_pixel_start_index,
        )?;
        let values = 4 * pixels_copied;

        // Convert from the input bit-depth to F32 (i.e always in RGBA).
        src_img.bit_depth_op.apply_bit_depth(
            Self::pixels(&in_bit_depth_buffer[..values]),
            PixelsMut::F32(&mut output_buffer[..values]),
        );
        Ok(())
    }

    /// Converts `num_pixels_to_unpack` RGBA pixels of `input_buffer` with the image's
    /// bit-depth op into `out_bit_depth_buffer`, then writes them into `dst_img` from pixel
    /// `image_pixel_start_index` along its row (alpha only when the image has one). A position
    /// outside the image writes nothing, and a run that would reach outside the buffers
    /// returns [`WRAPPED_INDEX`] (U-15).
    ///
    /// Port of `Generic<Type>::UnpackRGBAToImageDesc`
    /// (src/OpenColorIO/ImagePacking.cpp:161-229 @ v2.5.2). Its null-buffer check has no Rust
    /// counterpart.
    fn unpack_rgba_to_image_desc(
        dst_img: &GenericImageDesc,
        dst_buffers: &mut [&mut [u8]],
        input_buffer: &mut [f32],
        out_bit_depth_buffer: &mut [Self],
        num_pixels_to_unpack: c_int,
        image_pixel_start_index: c_long,
    ) -> Result<()> {
        let Some(positions) = start_positions(dst_img, image_pixel_start_index) else {
            return Ok(());
        };
        check_run(
            dst_img,
            &positions,
            num_pixels_to_unpack,
            size_of::<Self>(),
            |buffer| dst_buffers[buffer].len(),
        )?;
        let values = 4 * pixel_count(num_pixels_to_unpack);

        // Convert from F32 to the output bit-depth (i.e always RGBA).
        dst_img.bit_depth_op.apply_bit_depth(
            Pixels::F32(&input_buffer[..values]),
            Self::pixels_mut(&mut out_bit_depth_buffer[..values]),
        );

        scatter(
            dst_img,
            dst_buffers,
            positions,
            out_bit_depth_buffer,
            num_pixels_to_unpack,
        );
        Ok(())
    }
}

impl Generic for u8 {}
impl Generic for u16 {}
impl Generic for half::f16 {}

/// Port of `Generic<float>` (src/OpenColorIO/ImagePacking.cpp:91-159, 231-299 @ v2.5.2): the
/// pixels go straight into, or come straight from, the float scanline, and the image's
/// bit-depth op, the first or the last op of the processing, works on it in place.
impl Generic for f32 {
    /// Port of `Generic<float>::PackRGBAFromImageDesc`
    /// (src/OpenColorIO/ImagePacking.cpp:91-159 @ v2.5.2): `in_bit_depth_buffer` is unused.
    fn pack_rgba_from_image_desc(
        src_img: &GenericImageDesc,
        src_buffers: &[&[u8]],
        _in_bit_depth_buffer: &mut [f32],
        output_buffer: &mut [f32],
        output_buffer_size: c_int,
        image_pixel_start_index: c_long,
    ) -> Result<()> {
        let pixels_copied = gather(
            src_img,
            src_buffers,
            output_buffer,
            output_buffer_size,
            image_pixel_start_index,
        )?;

        // In the float specialization, the BitDepthOp is the first Op of the color processing.
        src_img
            .bit_depth_op
            .apply(&mut output_buffer[..4 * pixels_copied]);
        Ok(())
    }

    /// Port of `Generic<float>::UnpackRGBAToImageDesc`
    /// (src/OpenColorIO/ImagePacking.cpp:231-299 @ v2.5.2): `out_bit_depth_buffer` is unused.
    fn unpack_rgba_to_image_desc(
        dst_img: &GenericImageDesc,
        dst_buffers: &mut [&mut [u8]],
        input_buffer: &mut [f32],
        _out_bit_depth_buffer: &mut [f32],
        num_pixels_to_unpack: c_int,
        image_pixel_start_index: c_long,
    ) -> Result<()> {
        let Some(positions) = start_positions(dst_img, image_pixel_start_index) else {
            return Ok(());
        };
        check_run(
            dst_img,
            &positions,
            num_pixels_to_unpack,
            size_of::<f32>(),
            |buffer| dst_buffers[buffer].len(),
        )?;

        // In the float specialization, the BitDepthOp is the last Op of the color processing.
        dst_img
            .bit_depth_op
            .apply(&mut input_buffer[..4 * pixel_count(num_pixels_to_unpack)]);

        scatter(
            dst_img,
            dst_buffers,
            positions,
            input_buffer,
            num_pixels_to_unpack,
        );
        Ok(())
    }
}

/// [`WRAPPED_INDEX`] when `count` pixels of `img` from `positions` would reach outside the
/// buffers ([`run_is_inside`]).
fn check_run(
    img: &GenericImageDesc,
    positions: &[Option<(usize, isize)>; 4],
    count: c_int,
    size: usize,
    buffer_len: impl Fn(usize) -> usize,
) -> Result<()> {
    if run_is_inside(positions, img.x_stride_bytes, count, size, buffer_len) {
        Ok(())
    } else {
        Err(Exception::new(WRAPPED_INDEX))
    }
}

/// A C++ `int` count of pixels as a length: none for a negative count, where upstream's loops
/// don't run.
fn pixel_count(count: c_int) -> usize {
    usize::try_from(count).unwrap_or(0)
}

/// The loop of the packing functions: reorders `count` pixels from their channel positions to
/// RGBA in `rgba`, with alpha 0 (`(Type)0.0f`) when the image has none, and returns how many
/// pixels it copied. "Invalid output image position." for a position outside the image, and
/// [`WRAPPED_INDEX`] for a run that would reach outside the buffers (U-15).
///
/// Port of src/OpenColorIO/ImagePacking.cpp:65-85 and 135-155 @ v2.5.2, with the checks before
/// them (37-40, 107-110).
fn gather<T: ChannelType>(
    img: &GenericImageDesc,
    buffers: &[&[u8]],
    rgba: &mut [T],
    count: c_int,
    image_pixel_start_index: c_long,
) -> Result<usize> {
    let Some(mut positions) = start_positions(img, image_pixel_start_index) else {
        return Err(Exception::new("Invalid output image position."));
    };
    let size = size_of::<T>();
    check_run(img, &positions, count, size, |buffer| buffers[buffer].len())?;
    let read = |(buffer, offset): (usize, isize)| {
        T::read_ne(&buffers[buffer][offset as usize..offset as usize + size])
    };

    // Process one single, complete scanline.
    let mut pixels_copied = 0;
    while pixels_copied < pixel_count(count) {
        // Reorder channels from arbitrary channel ordering to RGBA.
        let [r, g, b, a] = positions;
        let pixel = &mut rgba[4 * pixels_copied..4 * pixels_copied + 4];
        pixel[0] = read(r.expect("red"));
        pixel[1] = read(g.expect("green"));
        pixel[2] = read(b.expect("blue"));
        pixel[3] = a.map_or(T::default(), read);

        pixels_copied += 1;
        positions = advance(positions, img.x_stride_bytes);
    }
    Ok(pixels_copied)
}

/// The loop of the unpacking functions: writes `count` RGBA pixels of `rgba` to their channel
/// positions, alpha only when the image has one. The run is inside the buffers
/// ([`check_run`]).
///
/// Port of src/OpenColorIO/ImagePacking.cpp:208-228 and 278-298 @ v2.5.2.
fn scatter<T: ChannelType>(
    img: &GenericImageDesc,
    buffers: &mut [&mut [u8]],
    mut positions: [Option<(usize, isize)>; 4],
    rgba: &[T],
    count: c_int,
) {
    let size = size_of::<T>();
    let mut write = |(buffer, offset): (usize, isize), value: T| {
        value.write_ne(&mut buffers[buffer][offset as usize..offset as usize + size]);
    };

    // Process one single, complete scanline.
    let mut pixels_copied = 0;
    while pixels_copied < pixel_count(count) {
        // Copy from the RGBA buffer to arbitrary channel ordering.
        let [r, g, b, a] = positions;
        let pixel = &rgba[4 * pixels_copied..4 * pixels_copied + 4];
        write(r.expect("red"), pixel[0]);
        write(g.expect("green"), pixel[1]);
        write(b.expect("blue"), pixel[2]);
        if let Some(a) = a {
            write(a, pixel[3]);
        }

        pixels_copied += 1;
        positions = advance(positions, img.x_stride_bytes);
    }
}

/// Every channel position moved by one pixel: `ptr += xStrideBytes`.
fn advance(
    positions: [Option<(usize, isize)>; 4],
    x_stride_bytes: isize,
) -> [Option<(usize, isize)>; 4] {
    positions.map(|p| p.map(|(buffer, offset)| (buffer, offset.wrapping_add(x_stride_bytes))))
}

#[cfg(test)]
#[path = "image_packing_tests.rs"]
mod tests;
