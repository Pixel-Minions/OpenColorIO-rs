// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The scanline helper: a port of `src/OpenColorIO/ScanlineHelper.h` and `ScanlineHelper.cpp`
//! @ v2.5.2. It feeds a CPU processor's ops one row of the image at a time, as packed RGBA F32,
//! and writes each processed row back, converting between the images' bit depths and F32 with
//! the ops at the ends of the chain.
//!
//! Upstream processes a packed RGBA F32 destination in its own memory, and reads and writes
//! the rows of RGBA-packed images in place. So does the port, through typed views of the
//! images' bytes, where a row is aligned for its channel type. A row that isn't, which C++
//! reads and writes at any alignment, is copied through a row of the port's own: the values
//! are the same, since a copy keeps every bit and the ops read each pixel before writing it.
//! The port sizes the scratch rows upstream sizes, where upstream sizes them.
//!
//! Very wide and very tall images (improvement candidate U-3): upstream sizes its scratch rows
//! in a C `long` and counts rows in an `int`, which overflow, and sizes them with
//! `std::vector::resize`, which raises for sizes it can't have. Where upstream then raises,
//! the port raises the same; where it would write outside its buffers, the port returns an
//! error instead (the general rule of `docs/deviations.md`).

use core::ffi::{c_int, c_long};
use std::any::TypeId;
use std::fmt;
use std::sync::Arc;

use crate::bit_depth_utils::ChannelType;
use crate::exception::{Exception, Result};
use crate::image_desc::{self, GenericImageDesc, ImageDesc, ImageDescMut, ImageLayout};
use crate::image_packing::Generic;
use crate::op::{CpuOp, Pixels, PixelsMut};
use crate::open_color_types::BitDepth;

/// The processing optimizations an image allows.
///
/// Port of `Optimizations` (src/OpenColorIO/ScanlineHelper.h:15-23 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Optimizations(u32);

impl Optimizations {
    /// `NO_OPTIMIZATION`.
    pub const NO_OPTIMIZATION: Optimizations = Optimizations(0x00);
    /// `PACKED_OPTIMIZATION`: the image is a packed RGBA buffer.
    pub const PACKED_OPTIMIZATION: Optimizations = Optimizations(0x01);
    /// `FLOAT_OPTIMIZATION`: the image is F32, i.e. 32-bit float.
    pub const FLOAT_OPTIMIZATION: Optimizations = Optimizations(0x02);
    /// `PACKED_FLOAT_OPTIMIZATION`.
    pub const PACKED_FLOAT_OPTIMIZATION: Optimizations = Optimizations(0x03);

    /// `(self & flags) == flags`.
    pub fn has(self, flags: Optimizations) -> bool {
        self.0 & flags.0 == flags.0
    }
}

/// The optimizations `img_desc` allows: packed if it is RGBA-packed, and packed float if it
/// is also F32.
///
/// Port of `GetOptimizationMode` (src/OpenColorIO/ScanlineHelper.cpp:15-30 @ v2.5.2).
pub fn get_optimization_mode(img_desc: &GenericImageDesc) -> Optimizations {
    let mut optim = Optimizations::NO_OPTIMIZATION;

    if img_desc.is_rgba_packed() {
        optim = Optimizations::PACKED_OPTIMIZATION;

        if img_desc.is_float() {
            optim = Optimizations::PACKED_FLOAT_OPTIMIZATION;
        }
    }

    optim
}

/// Row by row processing of an image, for a CPU processor.
///
/// Port of `ScanlineHelper` (src/OpenColorIO/ScanlineHelper.h:28-43 @ v2.5.2). The images'
/// buffers are borrowed for `'a`, from `init` to the last row.
pub trait ScanlineHelper<'a>: fmt::Debug {
    /// Port of `init(const ImageDesc & srcImg, const ImageDesc & dstImg)`: from `src` to `dst`.
    fn init_src_dst(&mut self, src: &'a dyn ImageDesc, dst: &'a mut dyn ImageDescMut)
    -> Result<()>;

    /// Port of `init(const ImageDesc & img)`: in place.
    fn init(&mut self, img: &'a mut dyn ImageDescMut) -> Result<()>;

    /// Port of `init(const ImageDesc & srcImg, const ImageDesc & dstImg)` with one image as
    /// both: the path from one image to another, reading the rows from the image it writes.
    /// Rust's borrows can't pass the image as both `src` and `dst`.
    fn init_same(&mut self, img: &'a mut dyn ImageDescMut) -> Result<()>;

    /// The next row, as packed RGBA F32 for the ops to process in place; `None` after the last
    /// row.
    ///
    /// Port of `prepRGBAScanline(float ** buffer, long & numPixels)`, where no more rows is
    /// `numPixels == 0`.
    fn prep_rgba_scanline(&mut self) -> Result<Option<&mut [f32]>>;

    /// Writes the processed row back.
    ///
    /// Port of `finishRGBAScanline()`.
    fn finish_rgba_scanline(&mut self) -> Result<()>;
}

/// The message of the `std::length_error` that `std::vector::resize` raises for a size past
/// its `max_size()`: `_Xlength_error("vector too long")` in MSVC's STL on Windows, and
/// `vector::_M_default_append` in libstdc++ on Linux.
const VECTOR_TOO_LONG: &str = if cfg!(target_os = "windows") {
    "vector too long"
} else {
    "vector::_M_default_append"
};

/// The port's error where upstream's scratch rows, sized in a C `long`, are too small for a
/// row of the image, and upstream would write outside them (improvement candidate U-3).
const TOO_WIDE: &str =
    "ScanlineHelper Error: The image is too wide: 4 * width overflows the scanline buffers.";

/// The port's error where upstream's row index, an `int`, has overflowed and upstream would
/// read and write outside the image, or, with a y stride of 0, go on reading the same row and
/// never stop, as its negative index never reaches the height (improvement candidate U-3).
const TOO_TALL: &str = "ScanlineHelper Error: The image is too tall: the scanline index overflows.";

/// What upstream's scratch rows allow for an image `width` pixels wide (improvement candidate
/// U-3). Upstream sizes each one `const long bufferSize = 4 * m_dstImg.m_width;`, in a C `long`
/// (32 bits on Windows), which wraps for very wide images
/// (src/OpenColorIO/ScanlineHelper.cpp:70-81, 99-109 @ v2.5.2):
/// - a negative size makes `std::vector::resize` raise `std::length_error`, which `init`
///   raises: `Err`;
/// - a size too small for a row makes the first row raise "Invalid output image buffer" (the
///   RGBA F32 row is empty, `&m_rgbaFloatBuffer[0]` is null, and `PackRGBAFromImageDesc` checks
///   it first, src/OpenColorIO/ImagePacking.cpp:26-29, 96-99), or write outside upstream's
///   buffers, for which the port returns [`TOO_WIDE`]: `Ok(Some(the first row's error))`;
/// - otherwise the rows fit: `Ok(None)`.
///
/// `in_packed`: the source is RGBA-packed. `use_dst_buffer`: the destination's own memory is
/// the RGBA F32 row, and `m_rgbaFloatBuffer` is unused. `resize_in`: upstream sizes
/// `m_inBitDepthBuffer` on this path. `float_input`: the input channel type is `float`, which
/// `Generic<float>` packs straight into the RGBA row.
fn scratch_rows(
    width: c_long,
    in_packed: bool,
    use_dst_buffer: bool,
    resize_in: bool,
    float_input: bool,
) -> Result<Option<Exception>> {
    // `const long bufferSize = 4 * m_dstImg.m_width;`, wrapping as both wheels do.
    let size = width.wrapping_mul(4);
    if size < 0 {
        if resize_in || !use_dst_buffer {
            return Err(Exception::length_error(VECTOR_TOO_LONG));
        }
        return Ok(None);
    }
    let row = 4 * i128::from(width);
    let size = i128::from(size);
    if !use_dst_buffer {
        // The rows are processed in `m_rgbaFloatBuffer`, of `size` values.
        if size >= row {
            return Ok(None);
        }
        if !in_packed && size == 0 {
            let message = if float_input {
                "Invalid output image buffer."
            } else {
                "Invalid output image buffer"
            };
            return Ok(Some(Exception::new(message)));
        }
        return Ok(Some(Exception::new(TOO_WIDE)));
    }
    // The rows are processed in the destination. A source packed channel by channel goes
    // through `m_inBitDepthBuffer` first, except for `float`.
    let in_size = if resize_in { size } else { 0 };
    if in_packed || float_input || in_size >= row {
        Ok(None)
    } else {
        Ok(Some(Exception::new(TOO_WIDE)))
    }
}

/// `std::vector<T>::resize(n)` of an empty vector: `n` values, or the C++ library's exception
/// (U-3). libstdc++ raises `std::length_error` past `max_size()`, `PTRDIFF_MAX / sizeof(T)`
/// values, which is where Rust's reservation overflows; MSVC's limit is higher, but a Windows
/// `long` can't reach either. Memory that can't be had raises `std::bad_alloc`, where Rust
/// would abort.
fn std_resize<T: Clone>(values: &mut Vec<T>, n: usize, value: T) -> Result<()> {
    values.clear();
    if n.checked_mul(size_of::<T>())
        .is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(Exception::length_error(VECTOR_TOO_LONG));
    }
    values
        .try_reserve_exact(n)
        .map_err(|_| Exception::bad_alloc())?;
    values.resize(n, value);
    Ok(())
}

/// The values of an image row, `bytes`: the row itself, or, where its bytes aren't aligned for
/// `T`, a copy in the port's row `own`, which grows to fit.
fn row_values<'b, T: ChannelType>(bytes: &'b [u8], own: &'b mut Vec<T>) -> Result<&'b [T]> {
    if let Some(values) = T::view(bytes) {
        return Ok(values);
    }
    let size = size_of::<T>();
    let n = bytes.len() / size;
    if own.len() < n {
        std_resize(own, n, T::default())?;
    }
    for (value, bytes) in own.iter_mut().zip(bytes.chunks_exact(size)) {
        *value = T::read_ne(bytes);
    }
    Ok(&own[..n])
}

/// An image row, `bytes`, as values to write: the row itself, or, where its bytes aren't aligned
/// for `T`, the port's row `own`, which grows to fit and, with `copy_in`, starts with the
/// row's values. [`write_back`] then writes the port's row into the image.
fn row_values_mut<'b, T: ChannelType>(
    bytes: &'b mut [u8],
    own: &'b mut Vec<T>,
    copy_in: bool,
) -> Result<&'b mut [T]> {
    if T::view(bytes).is_some() {
        return Ok(T::view_mut(bytes).expect("the row is aligned"));
    }
    let size = size_of::<T>();
    let n = bytes.len() / size;
    if own.len() < n {
        std_resize(own, n, T::default())?;
    }
    if copy_in {
        for (value, bytes) in own.iter_mut().zip(bytes.chunks_exact(size)) {
            *value = T::read_ne(bytes);
        }
    }
    Ok(&mut own[..n])
}

/// Writes the port's row into the image row `bytes`, when [`row_values_mut`] gave the port's
/// row: when the bytes aren't aligned for `T`.
fn write_back<T: ChannelType>(bytes: &mut [u8], own: &[T]) {
    if T::view(bytes).is_none() {
        let size = size_of::<T>();
        for (bytes, &value) in bytes.chunks_exact_mut(size).zip(own) {
            value.write_ne(bytes);
        }
    }
}

/// The memory a helper reads its source rows from.
enum Source<'a> {
    /// Separate buffers.
    Buffers(Vec<&'a [u8]>),
    /// The destination's buffers: the image is processed in place.
    Destination,
}

/// The scanline helper for images with channel types `I` (input) and `O` (output).
///
/// Port of `GenericScanlineHelper<InputType, OutType>`
/// (src/OpenColorIO/ScanlineHelper.h:45-98, src/OpenColorIO/ScanlineHelper.cpp:33-204
/// @ v2.5.2).
pub struct GenericScanlineHelper<'a, I: Generic, O: Generic> {
    input_bit_depth: BitDepth,
    output_bit_depth: BitDepth,
    in_bit_depth_op: Arc<dyn CpuOp>,
    out_bit_depth_op: Arc<dyn CpuOp>,

    /// Description of the source image.
    src_img: Option<GenericImageDesc>,
    /// Description of the destination image.
    dst_img: Option<GenericImageDesc>,
    src: Source<'a>,
    dst: Vec<&'a mut [u8]>,

    /// Optimization applicable to the input buffer.
    in_optimized_mode: Optimizations,
    /// Optimization applicable to the output buffer.
    out_optimized_mode: Optimizations,

    /// Processing needs an intermediate buffer as CPU Ops only process packed RGBA F32.
    rgba_float_buffer: Vec<f32>,

    /// Processing needs additional buffers of the same pixel type as the input/output in order
    /// to convert arbitrary channel order from/to RGBA.
    in_bit_depth_buffer: Vec<I>,
    out_bit_depth_buffer: Vec<O>,

    /// The port's own rows, for the packed image rows whose bytes aren't aligned for their
    /// channel type, which upstream reads and writes in place (see the module documentation):
    /// the source row, the destination row as RGBA F32, and the destination row.
    own_in: Vec<I>,
    own_rgba: Vec<f32>,
    own_out: Vec<O>,

    /// The index of the current line to process.
    y_index: c_int,

    /// Whether the destination is packed RGBA F32, which upstream processes in its own memory.
    use_dst_buffer: bool,

    /// What the first row raises, when upstream's scratch rows can't hold a row
    /// ([`scratch_rows`], improvement candidate U-3).
    row_error: Option<Exception>,
}

impl<I: Generic, O: Generic> fmt::Debug for GenericScanlineHelper<'_, I, O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GenericScanlineHelper")
            .field("input_bit_depth", &self.input_bit_depth)
            .field("output_bit_depth", &self.output_bit_depth)
            .field("in_optimized_mode", &self.in_optimized_mode)
            .field("out_optimized_mode", &self.out_optimized_mode)
            .field("y_index", &self.y_index)
            .field("use_dst_buffer", &self.use_dst_buffer)
            .finish_non_exhaustive()
    }
}

impl<'a, I: Generic, O: Generic> GenericScanlineHelper<'a, I, O> {
    /// A helper for images of `input_bit_depth` and `output_bit_depth`, converted from and to
    /// F32 by `in_bit_depth_op` and `out_bit_depth_op`.
    ///
    /// Port of the `GenericScanlineHelper` constructor
    /// (src/OpenColorIO/ScanlineHelper.cpp:33-48 @ v2.5.2).
    pub fn new(
        input_bit_depth: BitDepth,
        in_bit_depth_op: Arc<dyn CpuOp>,
        output_bit_depth: BitDepth,
        out_bit_depth_op: Arc<dyn CpuOp>,
    ) -> Self {
        GenericScanlineHelper {
            input_bit_depth,
            output_bit_depth,
            in_bit_depth_op,
            out_bit_depth_op,
            src_img: None,
            dst_img: None,
            src: Source::Destination,
            dst: Vec::new(),
            in_optimized_mode: Optimizations::NO_OPTIMIZATION,
            out_optimized_mode: Optimizations::NO_OPTIMIZATION,
            rgba_float_buffer: Vec::new(),
            in_bit_depth_buffer: Vec::new(),
            out_bit_depth_buffer: Vec::new(),
            own_in: Vec::new(),
            own_rgba: Vec::new(),
            own_out: Vec::new(),
            y_index: 0,
            use_dst_buffer: false,
            row_error: None,
        }
    }

    /// The part of `init(const ImageDesc & srcImg, const ImageDesc & dstImg)` that the two
    /// images' descriptions decide: the generic descriptions, their sizes, the optimization
    /// modes and the scratch rows (src/OpenColorIO/ScanlineHelper.cpp:50-82 @ v2.5.2).
    fn init_images(&mut self, src: &ImageLayout, dst: &ImageLayout) -> Result<()> {
        self.y_index = 0;

        let src_img =
            GenericImageDesc::init(src, self.input_bit_depth, self.in_bit_depth_op.clone())?;
        let dst_img =
            GenericImageDesc::init(dst, self.output_bit_depth, self.out_bit_depth_op.clone())?;

        if src_img.width != dst_img.width || src_img.height != dst_img.height {
            return Err(Exception::new(
                "Dimension inconsistency between source and destination image buffers.",
            ));
        }

        self.in_optimized_mode = get_optimization_mode(&src_img);
        self.out_optimized_mode = get_optimization_mode(&dst_img);

        // Can the output buffer be used as the internal RGBA F32 buffer?
        self.use_dst_buffer = self
            .out_optimized_mode
            .has(Optimizations::PACKED_FLOAT_OPTIMIZATION);

        let in_packed = self
            .in_optimized_mode
            .has(Optimizations::PACKED_OPTIMIZATION);
        self.size_buffers(dst_img.width, in_packed, false)?;
        self.src_img = Some(src_img);
        self.dst_img = Some(dst_img);
        Ok(())
    }

    /// Checks upstream's scratch rows for rows of `width` pixels ([`scratch_rows`]), then sizes
    /// the ones upstream's path sizes, in its order, with the C++ library's exceptions
    /// ([`std_resize`]). From one image to another (`in_place` false):
    /// `m_inBitDepthBuffer` for a source packed channel by channel, then `m_rgbaFloatBuffer` and
    /// `m_outBitDepthBuffer` unless the destination's rows are processed in place; in place, all
    /// three unless the image's rows are (src/OpenColorIO/ScanlineHelper.cpp:69-81, 99-109 @
    /// v2.5.2). A row that can't be processed isn't given buffers: its error comes first.
    fn size_buffers(&mut self, width: c_long, in_packed: bool, in_place: bool) -> Result<()> {
        let float_input = TypeId::of::<I>() == TypeId::of::<f32>();
        let resize_in = if in_place {
            !self.use_dst_buffer
        } else {
            !in_packed
        };
        self.row_error = scratch_rows(
            width,
            in_packed,
            self.use_dst_buffer,
            resize_in,
            float_input,
        )?;
        if self.row_error.is_some() {
            return Ok(());
        }
        // `const long bufferSize = 4 * m_dstImg.m_width;`: where a row is resized,
        // `scratch_rows` has refused a negative size.
        let size = || usize::try_from(width.wrapping_mul(4)).expect("a size scratch_rows allows");
        if in_place {
            if !self.use_dst_buffer {
                std_resize(&mut self.rgba_float_buffer, size(), 0.0)?;
                std_resize(&mut self.in_bit_depth_buffer, size(), I::default())?;
                std_resize(&mut self.out_bit_depth_buffer, size(), O::default())?;
            }
        } else {
            if !in_packed {
                std_resize(&mut self.in_bit_depth_buffer, size(), I::default())?;
            }
            if !self.use_dst_buffer {
                std_resize(&mut self.rgba_float_buffer, size(), 0.0)?;
                std_resize(&mut self.out_bit_depth_buffer, size(), O::default())?;
            }
        }
        Ok(())
    }
}

/// The bytes of row `y_index` of a packed RGBA image, `4 * width` channels of `size` bytes from
/// its red channel: `m_rData + m_yStrideBytes * m_yIndex` in upstream's packed paths
/// (src/OpenColorIO/ScanlineHelper.cpp:134, 160 @ v2.5.2).
fn packed_row(img: &GenericImageDesc, y_index: c_int, size: usize) -> (usize, usize) {
    let start = (img.r_data.offset as isize)
        .wrapping_add(img.y_stride_bytes.wrapping_mul(y_index as isize));
    let start = start as usize;
    let len = 4 * usize::try_from(img.width).unwrap_or(0) * size;
    (start, start + len)
}

impl<'a, I: Generic, O: Generic> ScanlineHelper<'a> for GenericScanlineHelper<'a, I, O> {
    /// Port of `GenericScanlineHelper::init(const ImageDesc &, const ImageDesc &)`
    /// (src/OpenColorIO/ScanlineHelper.cpp:50-82 @ v2.5.2).
    fn init_src_dst(
        &mut self,
        src: &'a dyn ImageDesc,
        dst: &'a mut dyn ImageDescMut,
    ) -> Result<()> {
        self.init_images(src.layout(), dst.layout())?;
        self.src = Source::Buffers(image_desc::buffers(src));
        self.dst = image_desc::buffers_mut(dst);
        Ok(())
    }

    /// Port of `GenericScanlineHelper::init(const ImageDesc &, const ImageDesc &)`
    /// (src/OpenColorIO/ScanlineHelper.cpp:50-82 @ v2.5.2) with one image as both: the rows
    /// are read from the image they are written to.
    fn init_same(&mut self, img: &'a mut dyn ImageDescMut) -> Result<()> {
        let layout = img.layout();
        self.init_images(layout, layout)?;
        self.src = Source::Destination;
        self.dst = image_desc::buffers_mut(img);
        Ok(())
    }

    /// Port of `GenericScanlineHelper::init(const ImageDesc &)`
    /// (src/OpenColorIO/ScanlineHelper.cpp:84-110 @ v2.5.2).
    fn init(&mut self, img: &'a mut dyn ImageDescMut) -> Result<()> {
        self.y_index = 0;

        let src_img = GenericImageDesc::init(
            img.layout(),
            self.input_bit_depth,
            self.in_bit_depth_op.clone(),
        )?;
        let dst_img = GenericImageDesc::init(
            img.layout(),
            self.output_bit_depth,
            self.out_bit_depth_op.clone(),
        )?;

        self.in_optimized_mode = get_optimization_mode(&src_img);
        self.out_optimized_mode = self.in_optimized_mode;

        // Can the output buffer be used as the internal RGBA F32 buffer?
        self.use_dst_buffer = self
            .out_optimized_mode
            .has(Optimizations::PACKED_FLOAT_OPTIMIZATION);

        let in_packed = self
            .in_optimized_mode
            .has(Optimizations::PACKED_OPTIMIZATION);
        self.size_buffers(dst_img.width, in_packed, true)?;
        self.src = Source::Destination;
        self.dst = image_desc::buffers_mut(img);
        self.src_img = Some(src_img);
        self.dst_img = Some(dst_img);
        Ok(())
    }

    /// Port of `GenericScanlineHelper::prepRGBAScanline`
    /// (src/OpenColorIO/ScanlineHelper.cpp:117-150 @ v2.5.2).
    fn prep_rgba_scanline(&mut self) -> Result<Option<&mut [f32]>> {
        let GenericScanlineHelper {
            src_img: Some(src_img),
            dst_img: Some(dst_img),
            src,
            dst,
            in_optimized_mode,
            rgba_float_buffer,
            in_bit_depth_buffer,
            own_in,
            own_rgba,
            y_index,
            use_dst_buffer,
            row_error,
            ..
        } = self
        else {
            panic!("the scanline helper is used before init");
        };

        // Note that only a line-by-line processing is done on the image buffer.
        if c_long::from(*y_index) >= dst_img.height {
            return Ok(None);
        }

        // A row upstream's scratch rows can't hold (U-3).
        if let Some(error) = row_error {
            return Err(error.clone());
        }

        let in_packed = in_optimized_mode.has(Optimizations::PACKED_OPTIMIZATION);

        // After row `c_int::MAX` of an image with more rows, upstream's row index has wrapped
        // (U-3): an RGBA-packed source row is then outside the image. Other sources raise
        // "Invalid output image position." when packed, before touching any memory
        // (src/OpenColorIO/ImagePacking.cpp:37-40, 107-110 @ v2.5.2). Where the destination's
        // row is the RGBA row, upstream only computes its address, outside the image, before
        // packing; the port can't take that row, so it raises the packing's error first.
        if *y_index < 0 {
            if in_packed {
                return Err(Exception::new(TOO_TALL));
            }
            if *use_dst_buffer {
                return Err(Exception::new("Invalid output image position."));
            }
        }

        let width = dst_img.width;
        let values = 4 * usize::try_from(width).unwrap_or(0);
        let index = c_long::from(*y_index).wrapping_mul(width);

        // The RGBA F32 row: the destination's row when it is packed RGBA F32, as upstream's
        // `*buffer`, else `m_rgbaFloatBuffer`.
        if *use_dst_buffer {
            let (start, end) = packed_row(dst_img, *y_index, size_of::<f32>());
            match src {
                Source::Destination => {
                    // In place: the image's row is both the source and the RGBA row, and the
                    // source's op converts it in place.
                    let row = &mut dst[dst_img.r_data.buffer][start..end];
                    let rgba = row_values_mut(row, own_rgba, true)?;
                    src_img.bit_depth_op.apply(rgba);
                    Ok(Some(rgba))
                }
                Source::Buffers(src_buffers) => {
                    let row = &mut dst[dst_img.r_data.buffer][start..end];
                    let rgba = row_values_mut(row, own_rgba, false)?;
                    if in_packed {
                        let (start, end) = packed_row(src_img, *y_index, size_of::<I>());
                        let src_row = &src_buffers[src_img.r_data.buffer][start..end];
                        let src_values = row_values(src_row, own_in)?;
                        // The codes a 1D LUT lookup can't take are an error first
                        // (docs/improvements.md, U-1).
                        src_img.bit_depth_op.check_input(I::pixels(src_values))?;
                        src_img
                            .bit_depth_op
                            .apply_bit_depth(I::pixels(src_values), PixelsMut::F32(rgba));
                    } else {
                        // Pack from any channel ordering & bit-depth to a packed RGBA F32
                        // buffer.
                        I::pack_rgba_from_image_desc(
                            src_img,
                            src_buffers,
                            in_bit_depth_buffer,
                            rgba,
                            width as c_int,
                            index,
                        )?;
                    }
                    Ok(Some(rgba))
                }
            }
        } else {
            let src_buffers: Vec<&[u8]> = match src {
                Source::Buffers(buffers) => buffers.clone(),
                Source::Destination => dst.iter().map(|b| &**b).collect(),
            };
            let rgba = &mut rgba_float_buffer[..values];
            if in_packed {
                // The source row, converted to F32 by the source's op.
                let (start, end) = packed_row(src_img, *y_index, size_of::<I>());
                let src_row = &src_buffers[src_img.r_data.buffer][start..end];
                let src_values = row_values(src_row, own_in)?;
                // The codes a 1D LUT lookup can't take are an error first
                // (docs/improvements.md, U-1).
                src_img.bit_depth_op.check_input(I::pixels(src_values))?;
                src_img
                    .bit_depth_op
                    .apply_bit_depth(I::pixels(src_values), PixelsMut::F32(rgba));
            } else {
                // Pack from any channel ordering & bit-depth to a packed RGBA F32 buffer.
                I::pack_rgba_from_image_desc(
                    src_img,
                    &src_buffers,
                    in_bit_depth_buffer,
                    rgba,
                    width as c_int,
                    index,
                )?;
            }
            Ok(Some(rgba))
        }
    }

    /// Port of `GenericScanlineHelper::finishRGBAScanline`
    /// (src/OpenColorIO/ScanlineHelper.cpp:152-177 @ v2.5.2).
    fn finish_rgba_scanline(&mut self) -> Result<()> {
        let GenericScanlineHelper {
            dst_img: Some(dst_img),
            dst,
            out_optimized_mode,
            rgba_float_buffer,
            out_bit_depth_buffer,
            own_rgba,
            own_out,
            y_index,
            use_dst_buffer,
            ..
        } = self
        else {
            panic!("the scanline helper is used before init");
        };
        let width = dst_img.width;
        let values = 4 * usize::try_from(width).unwrap_or(0);

        // Note that only a line-by-line processing is done on the image buffer.
        if out_optimized_mode.has(Optimizations::PACKED_OPTIMIZATION) {
            if *use_dst_buffer {
                // A packed RGBA F32 destination: its row is the RGBA row, which the
                // destination's op converts in place (`in == out`).
                let (start, end) = packed_row(dst_img, *y_index, size_of::<f32>());
                let row = &mut dst[dst_img.r_data.buffer][start..end];
                let rgba = row_values_mut(row, own_rgba, false)?;
                dst_img.bit_depth_op.apply(rgba);
                write_back(&mut dst[dst_img.r_data.buffer][start..end], own_rgba);
            } else {
                // The RGBA row converted into the destination's row by its op.
                let (start, end) = packed_row(dst_img, *y_index, size_of::<O>());
                let row = &mut dst[dst_img.r_data.buffer][start..end];
                let out = row_values_mut(row, own_out, false)?;
                dst_img.bit_depth_op.apply_bit_depth(
                    Pixels::F32(&rgba_float_buffer[..values]),
                    O::pixels_mut(out),
                );
                write_back(&mut dst[dst_img.r_data.buffer][start..end], own_out);
            }
        } else {
            // Unpack from packed RGBA F32 to any channel ordering & bit-depth.
            O::unpack_rgba_to_image_desc(
                dst_img,
                dst,
                rgba_float_buffer,
                out_bit_depth_buffer,
                width as c_int,
                c_long::from(*y_index).wrapping_mul(width),
            )?;
        }

        // `++m_yIndex`: an `int`, which wraps after row `c_int::MAX` of an image with more
        // rows, as both wheels do (U-3); the next row is refused.
        self.y_index = self.y_index.wrapping_add(1);
        Ok(())
    }
}

#[cfg(test)]
#[path = "scanline_helper_tests.rs"]
mod tests;
