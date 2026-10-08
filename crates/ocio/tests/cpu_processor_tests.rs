// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the tests of `tests/cpu/CPUProcessor_tests.cpp` @ v2.5.2 that get their processors
//! from `Config::Create()`: `with_one_matrix`, `planar_vs_packed`, the `scanline_*` tests,
//! `one_pixel` and `optimizations`, with their helpers (`ComputeValues`, `BuildProcessor`,
//! `BuildCPUProcessor`, `Process`, `ComputeImage`). `flag_composition` is in
//! `crates/ocio-ops/src/cpu_processor_tests.rs`. The others wait: `dynamic_properties` for
//! `ExposureContrastTransform` (Phase 5), `with_one_1d_lut` for a `FileTransform` and
//! `image_desc` and `with_several_ops` for configs with one (Phase 4).
//!
//! Upstream passes its buffers as `void *`; the port passes their bytes ([`Bytes`]), with
//! [`At`] where upstream passes a pointer into a buffer. Its `std::vector`s stay `Vec`s.
#![allow(clippy::useless_vec)]

use std::sync::Arc;

use ocio::{BitDepth, Config, GroupTransform, MatrixTransform, Processor, Transform};
use ocio::{OptimizationFlags, TransformDirection};
use ocio_ops::bit_depth_utils::{
    BitDepthInfo, ChannelType, Converter, F32, Uint8, Uint10, Uint12, Uint16,
    get_bit_depth_max_value,
};
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::image_desc::{AUTO_STRIDE, At, Bytes, PackedImageDesc, PlanarImageDesc};
use ocio_ops::open_color_types::ChannelOrdering;
use ocio_testkit::upstream::check_close;

const F: isize = size_of::<f32>() as isize;

/// The bytes of `values`, in the machine's byte order.
fn to_bytes<T: ChannelType>(values: &[T]) -> Vec<u8> {
    let size = size_of::<T>();
    let mut bytes = vec![0u8; size_of_val(values)];
    for (v, b) in values.iter().zip(bytes.chunks_exact_mut(size)) {
        v.write_ne(b);
    }
    bytes
}

/// The values `bytes` hold, in the machine's byte order.
fn from_bytes<T: ChannelType>(bytes: &[u8]) -> Vec<T> {
    let size = size_of::<T>();
    (0..bytes.len() / size)
        .map(|i| T::read_ne(&bytes[i * size..]))
        .collect()
}

/// Port of `ComputeValues<inBD, outBD>` (tests/cpu/CPUProcessor_tests.cpp:60-121 @ v2.5.2).
#[allow(clippy::too_many_arguments)]
#[track_caller]
fn compute_values<I: BitDepthInfo, O: BitDepthInfo>(
    processor: &Processor,
    in_img: &[I::Type],
    in_chans: ChannelOrdering,
    res_img: &[O::Type],
    out_chans: ChannelOrdering,
    num_pixels: usize,
    // Default value to nan to break any float comparisons
    // as a valid error threshold is mandatory in that case.
    abs_error_threshold: f32,
) -> Arc<CpuProcessor>
where
    O::Type: PartialEq,
{
    let cpu_processor = processor
        .optimized_cpu_processor_with_bit_depths(
            I::BIT_DEPTH,
            O::BIT_DEPTH,
            OptimizationFlags::DEFAULT,
        )
        .unwrap();

    let mut num_channels = 4;
    if out_chans == ChannelOrdering::Rgb || out_chans == ChannelOrdering::Bgr {
        num_channels = 3;
    }
    let num_values = num_pixels * num_channels;

    let in_bytes = to_bytes(in_img);
    let src_img_desc = PackedImageDesc::with_channel_order_and_strides(
        Bytes(&in_bytes[..]),
        num_pixels,
        1,
        in_chans,
        I::BIT_DEPTH,
        size_of::<I::Type>() as isize,
        AUTO_STRIDE,
        AUTO_STRIDE,
    )
    .unwrap();

    let mut out_bytes = vec![0u8; num_values * size_of::<O::Type>()];
    {
        let mut dst_img_desc = PackedImageDesc::with_channel_order_and_strides(
            Bytes(&mut out_bytes[..]),
            num_pixels,
            1,
            out_chans,
            O::BIT_DEPTH,
            size_of::<O::Type>() as isize,
            AUTO_STRIDE,
            AUTO_STRIDE,
        )
        .unwrap();

        cpu_processor
            .apply_src_dst(&src_img_desc, &mut dst_img_desc)
            .unwrap();
    }
    let out: Vec<O::Type> = from_bytes(&out_bytes);

    for idx in 0..num_values {
        if O::IS_FLOAT {
            check_close(
                out[idx].to_float(),
                res_img[idx].to_float(),
                abs_error_threshold,
            );
        } else {
            assert_eq!(out[idx], res_img[idx], "index {idx}");
        }
    }

    cpu_processor
}

/// Port of `OCIO_ADD_TEST(CPUProcessor, with_one_matrix)` @ v2.5.2.
#[test]
#[rustfmt::skip]
fn with_one_matrix() {
    // The unit test validates that pixel formats are correctly
    // processed when the op list contains only one arbitrary Op
    // (except a 1D LUT one which has dedicated optimizations).

    let config = Config::new().unwrap();

    let mut transform = MatrixTransform::new();
    const OFFSET4: [f64; 4] = [1.4002, 0.4005, 0.0807, 0.5];
    transform.set_offset(&OFFSET4);

    let processor = config.processor(&Transform::from(transform)).unwrap();

    const NB_PIXELS: usize = 3;

    let f_in_img: Vec<f32> =
        vec![  -1.0000, -0.8000, -0.1000,  0.0,
                0.1023,  0.5045,  1.5089,  1.0,
                1.0000,  1.2500,  1.9900,  0.0  ];

    {
        let res_img: Vec<f32>
            = vec![ 0.4002, -0.3995, -0.0193,  0.5000,
                    1.5025,  0.9050,  1.5896,  1.5000,
                    2.4002,  1.6505,  2.0707,  0.5000 ];

        compute_values::<F32, F32>(
            &processor,
            &f_in_img, ChannelOrdering::Rgba,
            &res_img,  ChannelOrdering::Rgba,
            NB_PIXELS,
            1e-5);
    }

    {
        let res_img: Vec<f32>
            = vec![ -0.9193,   -0.3995,  1.3002,  0.5000,
                     0.182999,  0.9050,  2.9091,  1.5000,
                     1.0807,    1.6505,  3.3902,  0.5000 ];

        compute_values::<F32, F32>(
            &processor,
            &f_in_img, ChannelOrdering::Bgra,
            &res_img,  ChannelOrdering::Bgra,
            NB_PIXELS,
            1e-5);
    }

    {
        let res_img: Vec<f32>
            = vec![  -0.500000, -0.719300, 0.300500, 1.400200,
                      0.602300,  0.585199, 1.909399, 2.400200,
                      1.500000,  1.330700, 2.390500, 1.400200  ];

        compute_values::<F32, F32>(
            &processor,
            &f_in_img, ChannelOrdering::Abgr,
            &res_img,  ChannelOrdering::Abgr,
            NB_PIXELS,
            1e-5);
    }

    {
        let res_img: Vec<f32>
            = vec![ -0.0193, -0.3995,  0.4002,  0.5000,
                     1.5896,  0.9050,  1.5025,  1.5000,
                     2.0707,  1.6505,  2.4002,  0.5000 ];

        compute_values::<F32, F32>(
            &processor,
            &f_in_img, ChannelOrdering::Rgba,
            &res_img,  ChannelOrdering::Bgra,
            NB_PIXELS,
            1e-5);
    }

    {
        let res_img: Vec<f32>
            = vec![ 0.5000, -0.0193, -0.3995, 0.4002,
                    1.5000,  1.5896,  0.9050, 1.5025,
                    0.5000,  2.0707,  1.6505, 2.4002  ];

        compute_values::<F32, F32>(
            &processor,
            &f_in_img, ChannelOrdering::Rgba,
            &res_img,  ChannelOrdering::Abgr,
            NB_PIXELS,
            1e-5);
    }

    {
        let in_img: Vec<f32> =
            vec![  -1.0000, -0.8000, -0.1000,
                    0.1023,  0.5045,  1.5089,
                    1.0000,  1.2500,  1.9900  ];

        let res_img: Vec<f32>
            = vec![ 0.4002, -0.3995, -0.0193,
                    1.5025,  0.9050,  1.5896,
                    2.4002,  1.6505,  2.0707 ];

        compute_values::<F32, F32>(
            &processor,
            &in_img,  ChannelOrdering::Rgb,
            &res_img, ChannelOrdering::Rgb,
            NB_PIXELS,
            1e-5);
    }

    {
        let in_img: Vec<f32> =
            vec![    -1.0000,    -0.8000, -0.1000,
                       0.1023,    0.5045,  1.5089,
                       1.0000,    1.2500,  1.9900  ];

        let res_img: Vec<f32>
            = vec![ -0.919300, -0.399500,  1.300199,
                     0.182999,  0.905000,  2.909100,
                     1.080700,  1.650500,  3.390200 ];

        compute_values::<F32, F32>(
            &processor,
            &in_img,  ChannelOrdering::Bgr,
            &res_img, ChannelOrdering::Bgr,
            NB_PIXELS,
            1e-5);
    }

    {
        let in_img: Vec<f32> =
            vec![  -1.0000, -0.8000, -0.1000,
                    0.1023,  0.5045,  1.5089,
                    1.0000,  1.2500,  1.9900  ];

        let res_img: Vec<f32>
            = vec![ -0.01929,  -0.3995,  0.4002,
                     1.58960,   0.9050,  1.5025,
                     2.070699,  1.6505,  2.4002 ];

        compute_values::<F32, F32>(
            &processor,
            &in_img,  ChannelOrdering::Rgb,
            &res_img, ChannelOrdering::Bgr,
            NB_PIXELS,
            1e-5);
    }

    {
        let in_img: Vec<f32> =
            vec![  -1.0000, -0.8000, -0.1000,
                    0.1023,  0.5045,  1.5089,
                    1.0000,  1.2500,  1.9900  ];

        let res_img: Vec<f32>
            = vec![ -0.01929,  -0.3995,  0.4002, 0.5,
                     1.58960,   0.9050,  1.5025, 0.5,
                     2.070699,  1.6505,  2.4002, 0.5   ];

        compute_values::<F32, F32>(
            &processor,
            &in_img,  ChannelOrdering::Rgb,
            &res_img, ChannelOrdering::Bgra,
            NB_PIXELS,
            1e-5);
    }

    {
        let in_img: Vec<f32> =
            vec![  -1.0000, -0.8000, -0.1000,  0.0,
                    0.1023,  0.5045,  1.5089,  1.0,
                    1.0000,  1.2500,  1.9900,  0.0  ];

        let res_img: Vec<f32>
            = vec![ -0.01929,  -0.3995,  0.4002,
                     1.58960,   0.9050,  1.5025,
                     2.070699,  1.6505,  2.4002   ];

        compute_values::<F32, F32>(
            &processor,
            &in_img,  ChannelOrdering::Rgba,
            &res_img, ChannelOrdering::Bgr,
            NB_PIXELS,
            1e-5);
    }

    let ui16_in_img: Vec<u16> =
        vec![    0,      8,    32,  0,
                64,    128,   256,  0,
              5120,  20140, 65535,  0  ];

    {
        let res_img: Vec<f32>
            = vec![ 1.40020000,  0.40062206,  0.08118829,  0.5,
                    1.40117657,  0.40245315,  0.08460631,  0.5,
                    1.47832620,  0.70781672,  1.08070004,  0.5 ];

        compute_values::<Uint16, F32>(
            &processor,
            &ui16_in_img, ChannelOrdering::Rgba,
            &res_img,     ChannelOrdering::Rgba,
            NB_PIXELS,
            1e-5);
    }

    {
        let res_img: Vec<u16>
            = vec![ 65535, 26255,  5321, 32768,
                    65535, 26375,  5545, 32768,
                    65535, 46387, 65535, 32768 ];

        compute_values::<Uint16, Uint16>(
            &processor,
            &ui16_in_img, ChannelOrdering::Rgba,
            &res_img,     ChannelOrdering::Rgba,
            NB_PIXELS,
            f32::NAN);
    }

    {
        let res_img: Vec<u16>
            = vec![  5321, 26255, 65535, 32768,
                     5545, 26375, 65535, 32768,
                    65535, 46387, 65535, 32768 ];

        compute_values::<Uint16, Uint16>(
            &processor,
            &ui16_in_img, ChannelOrdering::Rgba,
            &res_img,     ChannelOrdering::Bgra,
            NB_PIXELS,
            f32::NAN);
    }

    {
        let res_img: Vec<u16>
            = vec![  5321, 26255, 65535,
                     5545, 26375, 65535,
                    65535, 46387, 65535 ];

        compute_values::<Uint16, Uint16>(
            &processor,
            &ui16_in_img, ChannelOrdering::Rgba,
            &res_img,     ChannelOrdering::Bgr,
            NB_PIXELS,
            f32::NAN);
    }

    {
        let res_img: Vec<u8>
            = vec![ 255, 102,  21, 128,
                    255, 103,  22, 128,
                    255, 180, 255, 128 ];

        compute_values::<Uint16, Uint8>(
            &processor,
            &ui16_in_img, ChannelOrdering::Rgba,
            &res_img,     ChannelOrdering::Rgba,
            NB_PIXELS,
            f32::NAN);
    }

    {
        let res_img: Vec<u8>
            = vec![  21, 102, 255,
                     22, 103, 255,
                    255, 180, 255 ];

        compute_values::<Uint16, Uint8>(
            &processor,
            &ui16_in_img, ChannelOrdering::Rgba,
            &res_img,     ChannelOrdering::Bgr,
            NB_PIXELS,
            f32::NAN);
    }

    {
        let res_img: Vec<u8>
            = vec![ 128,  21, 102, 255,
                    128,  22, 103, 255,
                    128, 255, 180, 255 ];

        compute_values::<Uint16, Uint8>(
            &processor,
            &ui16_in_img, ChannelOrdering::Rgba,
            &res_img,     ChannelOrdering::Abgr,
            NB_PIXELS,
            f32::NAN);
    }

    // Test OCIO::BIT_DEPTH_UINT10.

    {
        let ui10_res_img: Vec<u16>
            = vec![ 1023,  410,   83,  512,
                    1023,  412,   87,  512,
                    1023,  724, 1023,  512 ];

        compute_values::<Uint16, Uint10>(
            &processor,
            &ui16_in_img,  ChannelOrdering::Rgba,
            &ui10_res_img, ChannelOrdering::Rgba,
            NB_PIXELS,
            f32::NAN);

        let ui10_in_img: Vec<u16>
            = vec![    0,    8,   12,  256,
                     128,   16,   64,  512,
                    1023,   32,   96,  512 ];

        let ui16_res_img: Vec<u16>
            = vec![ 65535, 26759,  6057, 49167,
                    65535, 27272,  9389, 65535,
                    65535, 28297, 11439, 65535 ];

        compute_values::<Uint10, Uint16>(
            &processor,
            &ui10_in_img,  ChannelOrdering::Rgba,
            &ui16_res_img, ChannelOrdering::Rgba,
            NB_PIXELS,
            f32::NAN);
    }

    // Test OCIO::BIT_DEPTH_UINT12.

    {
        let ui12_res_img: Vec<u16>
            = vec![ 4095, 1641,  332, 2048,
                    4095, 1648,  346, 2048,
                    4095, 2899, 4095, 2048 ];

        compute_values::<Uint16, Uint12>(
            &processor,
            &ui16_in_img,  ChannelOrdering::Rgba,
            &ui12_res_img, ChannelOrdering::Rgba,
            NB_PIXELS,
            f32::NAN);

        let ui12_in_img: Vec<u16>
            = vec![     0,    8,    12,   1024,
                     2048,   16,    64,   2048,
                     4095,   32,    96,   4095 ];

        let ui16_res_img: Vec<u16>
            = vec![ 65535, 26375,  5481, 49155,
                    65535, 26503,  6313, 65535,
                    65535, 26759,  6825, 65535 ];

        compute_values::<Uint12, Uint16>(
            &processor,
            &ui12_in_img,  ChannelOrdering::Rgba,
            &ui16_res_img, ChannelOrdering::Rgba,
            NB_PIXELS,
            f32::NAN);
    }
}

// ---------------------------------------------------------------------------------------------
// The image of the scanline tests (tests/cpu/CPUProcessor_tests.cpp:1269-1362 @ v2.5.2).

const NB_PIXELS: usize = 6;

const IN_IMG_R: [f32; NB_PIXELS] = [-1.000012, -0.500012, 0.100012, 0.600012, 1.102312, 1.700012];

const IN_IMG_G: [f32; NB_PIXELS] = [-0.800012, -0.300012, 0.250012, 0.800012, 1.204512, 1.800012];

const IN_IMG_B: [f32; NB_PIXELS] = [-0.600012, -0.100012, 0.450012, 0.900012, 1.508912, 1.990012];

const IN_IMG_A: [f32; NB_PIXELS] = [0.005005, 0.405005, 0.905005, 0.005005, 1.005005, 0.095005];

const RES_IMG_R: [f32; NB_PIXELS] = [
    0.4001879692,
    0.9001880288,
    1.500211954,
    2.000211954,
    2.502511978,
    3.100212097,
];

const RES_IMG_G: [f32; NB_PIXELS] = [
    -0.3995119929,
    0.1004880071,
    0.6505119801,
    1.200511932,
    1.60501194,
    2.200511932,
];

const RES_IMG_B: [f32; NB_PIXELS] = [
    0.2006880045,
    0.7006880045,
    1.250712037,
    1.700711966,
    2.309612036,
    2.790712118,
];

const RES_IMG_A: [f32; NB_PIXELS] = [
    0.5057050, 0.9057050, 1.4057050, 0.5057050, 1.5057050, 0.5957050,
];

/// The planes interleaved: `{ R[0], G[0], B[0], A[0], R[1], ... }`.
fn interleave(r: &[f32], g: &[f32], b: &[f32], a: &[f32]) -> Vec<f32> {
    (0..NB_PIXELS)
        .flat_map(|i| [r[i], g[i], b[i], a[i]])
        .collect()
}

/// `inImg`.
fn in_img() -> Vec<f32> {
    interleave(&IN_IMG_R, &IN_IMG_G, &IN_IMG_B, &IN_IMG_A)
}

/// `resImg`.
fn res_img() -> Vec<f32> {
    interleave(&RES_IMG_R, &RES_IMG_G, &RES_IMG_B, &RES_IMG_A)
}

/// Port of `BuildProcessor` (tests/cpu/CPUProcessor_tests.cpp:1364-1388 @ v2.5.2).
fn build_processor(dir: TransformDirection) -> Arc<Processor> {
    let config = Config::new().unwrap();

    let mut m1 = MatrixTransform::new();
    const OFFSET1: [f64; 4] = [1.0, 0.2, 0.4007, 0.3007];
    m1.set_offset(&OFFSET1);
    m1.set_direction(dir);

    let mut m2 = MatrixTransform::new();
    const OFFSET2: [f64; 4] = [0.2002, 0.2, 0.2, 0.2];
    m2.set_offset(&OFFSET2);
    m2.set_direction(dir);

    let mut m3 = MatrixTransform::new();
    const OFFSET3: [f64; 4] = [0.2, 0.0005, 0.2, 0.0];
    m3.set_offset(&OFFSET3);
    m3.set_direction(dir);

    let mut transform = GroupTransform::new();
    transform.append_transform(Transform::from(m1));
    transform.append_transform(Transform::from(m2));
    transform.append_transform(Transform::from(m3));

    config.processor(&Transform::from(transform)).unwrap()
}

/// Port of `BuildCPUProcessor` (tests/cpu/CPUProcessor_tests.cpp:1390-1394 @ v2.5.2).
fn build_cpu_processor(dir: TransformDirection) -> Arc<CpuProcessor> {
    let processor = build_processor(dir);
    processor
        .optimized_cpu_processor(OptimizationFlags::NONE)
        .unwrap()
}

/// Port of `Validate` (tests/cpu/CPUProcessor_tests.cpp:1396-1406 @ v2.5.2): the packed RGBA
/// image `out_img` holds `resImg`.
#[track_caller]
fn validate(out_img: &[f32]) {
    let res_img = res_img();
    for pxl in 0..NB_PIXELS {
        check_close(out_img[4 * pxl], res_img[4 * pxl], 1e-6);
        check_close(out_img[4 * pxl + 1], res_img[4 * pxl + 1], 1e-6);
        check_close(out_img[4 * pxl + 2], res_img[4 * pxl + 2], 1e-6);
        check_close(out_img[4 * pxl + 3], res_img[4 * pxl + 3], 1e-6);
    }
}

/// How a test describes its F32 RGBA output: upstream's constructor and its arguments (width,
/// height, then the channel, pixel and line strides).
#[derive(Clone, Copy)]
enum Dst {
    /// `PackedImageDesc(data, width, height, 4)`.
    New(usize, usize),
    /// `PackedImageDesc(data, width, height, CHANNEL_ORDERING_RGBA, BIT_DEPTH_F32, ...)`.
    Order(usize, usize, isize, isize, isize),
    /// `PackedImageDesc(data, width, height, 4, BIT_DEPTH_F32, ...)`.
    Count(usize, usize, isize, isize, isize),
}

fn dst_desc(out_img: &mut [f32], dst: Dst) -> PackedImageDesc<&mut [u8]> {
    match dst {
        Dst::New(w, h) => PackedImageDesc::new(out_img, w, h, 4),
        Dst::Order(w, h, c, x, y) => PackedImageDesc::with_channel_order_and_strides(
            out_img,
            w,
            h,
            ChannelOrdering::Rgba,
            BitDepth::F32,
            c,
            x,
            y,
        ),
        Dst::Count(w, h, c, x, y) => {
            PackedImageDesc::with_strides(out_img, w, h, 4, BitDepth::F32, c, x, y)
        }
    }
    .unwrap()
}

/// Port of `Process(cpuProcessor, const PackedImageDesc &, PackedImageDesc &)`
/// (tests/cpu/CPUProcessor_tests.cpp:1408-1415 @ v2.5.2): processes `src` into `out_img`,
/// described by `dst`, then validates it.
#[track_caller]
fn process(
    cpu_processor: &CpuProcessor,
    src: &PackedImageDesc<&[u8]>,
    out_img: &mut [f32],
    dst: Dst,
) {
    {
        let mut dst_img_desc = dst_desc(out_img, dst);
        cpu_processor.apply_src_dst(src, &mut dst_img_desc).unwrap();
    }
    validate(out_img);
}

/// Port of `Process(cpuProcessor, PackedImageDesc &)` (tests/cpu/CPUProcessor_tests.cpp:
/// 1417-1423 @ v2.5.2): processes `img`, F32 RGBA pixels of `width` by `height`, in place,
/// then validates it.
#[track_caller]
fn process_in_place(cpu_processor: &CpuProcessor, img: &mut [f32], width: usize, height: usize) {
    {
        let mut img_desc = PackedImageDesc::new(&mut *img, width, height, 4).unwrap();
        cpu_processor.apply(&mut img_desc).unwrap();
    }
    validate(img);
}

/// Port of `Process(cpuProcessor, const PlanarImageDesc &, PlanarImageDesc &)`
/// (tests/cpu/CPUProcessor_tests.cpp:1425-1447 @ v2.5.2): the planes hold `resImg`, the
/// alpha plane where there is one.
#[track_caller]
fn validate_planar(r: &[f32], g: &[f32], b: &[f32], a: Option<&[f32]>) {
    let res_img = res_img();
    for pxl in 0..NB_PIXELS {
        check_close(r[pxl], res_img[4 * pxl], 1e-6);
        check_close(g[pxl], res_img[4 * pxl + 1], 1e-6);
        check_close(b[pxl], res_img[4 * pxl + 2], 1e-6);
        if let Some(a) = a {
            check_close(a[pxl], res_img[4 * pxl + 3], 1e-6);
        }
    }
}

/// Port of `OCIO_ADD_TEST(CPUProcessor, planar_vs_packed)` @ v2.5.2.
#[test]
fn planar_vs_packed() {
    // The unit test validates different types for input and output imageDesc.

    let mut cpu_processor = build_cpu_processor(TransformDirection::Forward);

    // 1. Process from Packed to Planar Image Desc using the forward transform.

    let in_img = in_img();
    let res_img = res_img();
    let src_img_desc = PackedImageDesc::new(&in_img[..], NB_PIXELS, 1, 4).unwrap();

    let mut out_r = vec![0f32; NB_PIXELS];
    let mut out_g = vec![0f32; NB_PIXELS];
    let mut out_b = vec![0f32; NB_PIXELS];
    let mut out_a = vec![0f32; NB_PIXELS];
    {
        let mut dst_img_desc = PlanarImageDesc::new(
            &mut out_r[..],
            &mut out_g[..],
            &mut out_b[..],
            Some(&mut out_a[..]),
            NB_PIXELS,
            1,
        )
        .unwrap();

        cpu_processor
            .apply_src_dst(&src_img_desc, &mut dst_img_desc)
            .unwrap();
    }

    for idx in 0..NB_PIXELS {
        check_close(out_r[idx], res_img[4 * idx], 1e-6);
        check_close(out_g[idx], res_img[4 * idx + 1], 1e-6);
        check_close(out_b[idx], res_img[4 * idx + 2], 1e-6);
        check_close(out_a[idx], res_img[4 * idx + 3], 1e-6);
    }

    // 2. Process from Planar to Packed Image Desc using the inverse transform.

    cpu_processor = build_cpu_processor(TransformDirection::Inverse);

    let mut out_img = vec![-1.0f32; NB_PIXELS * 4];
    {
        let dst_img_desc = PlanarImageDesc::new(
            &out_r[..],
            &out_g[..],
            &out_b[..],
            Some(&out_a[..]),
            NB_PIXELS,
            1,
        )
        .unwrap();
        let mut dst_img_desc2 = PackedImageDesc::new(&mut out_img[..], NB_PIXELS, 1, 4).unwrap();

        cpu_processor
            .apply_src_dst(&dst_img_desc, &mut dst_img_desc2)
            .unwrap();
    }

    for idx in 0..(NB_PIXELS * 4) {
        check_close(out_img[idx], in_img[idx], 1e-6);
    }
}

/// Port of `OCIO_ADD_TEST(CPUProcessor, scanline_packed)` @ v2.5.2.
#[test]
fn scanline_packed() {
    // Test the packed image description.

    let cpu_processor = build_cpu_processor(TransformDirection::Forward);

    let in_img = in_img();
    let mut out_img = vec![0f32; NB_PIXELS * 4];
    let src = |w: usize, h: usize| PackedImageDesc::new(&in_img[..], w, h, 4).unwrap();

    process(
        &cpu_processor,
        &src(NB_PIXELS, 1),
        &mut out_img,
        Dst::New(NB_PIXELS, 1),
    );

    process(
        &cpu_processor,
        &src(1, NB_PIXELS),
        &mut out_img,
        Dst::New(1, NB_PIXELS),
    );

    process(&cpu_processor, &src(2, 3), &mut out_img, Dst::New(2, 3));

    process(&cpu_processor, &src(3, 2), &mut out_img, Dst::New(3, 2));

    process(
        &cpu_processor,
        &src(2, 3),
        &mut out_img,
        Dst::Order(2, 3, AUTO_STRIDE, AUTO_STRIDE, AUTO_STRIDE),
    );

    process(
        &cpu_processor,
        &src(2, 3),
        &mut out_img,
        Dst::Order(2, 3, F, AUTO_STRIDE, AUTO_STRIDE),
    );

    process(
        &cpu_processor,
        &src(2, 3),
        &mut out_img,
        Dst::Order(2, 3, F, 4 * F, AUTO_STRIDE),
    );

    process(
        &cpu_processor,
        &src(2, 3),
        &mut out_img,
        Dst::Count(2, 3, F, 4 * F, AUTO_STRIDE),
    );

    process(
        &cpu_processor,
        &src(2, 3),
        &mut out_img,
        Dst::Count(2, 3, AUTO_STRIDE, 4 * F, AUTO_STRIDE),
    );

    process(
        &cpu_processor,
        &src(2, 3),
        &mut out_img,
        Dst::Count(2, 3, AUTO_STRIDE, 4 * F, 2 * 4 * F),
    );

    process(
        &cpu_processor,
        &src(2, 3),
        &mut out_img,
        Dst::Count(2, 3, AUTO_STRIDE, AUTO_STRIDE, 2 * 4 * F),
    );
}

/// Port of `OCIO_ADD_TEST(CPUProcessor, scanline_packed_planar)` @ v2.5.2.
#[test]
fn scanline_packed_planar() {
    // Test to validate the conversion from packed to planar images with a bit-depth different
    // from the default F32.

    let mut rout_img = vec![0u8; NB_PIXELS];
    let mut gout_img = vec![0u8; NB_PIXELS];
    let mut bout_img = vec![0u8; NB_PIXELS];
    let mut aout_img = vec![0u8; NB_PIXELS];

    let in_img = in_img();
    let src_img_desc = PackedImageDesc::new(&in_img[..], 2, 3, 4).unwrap();
    {
        let mut dst_img_desc = PlanarImageDesc::with_strides(
            &mut rout_img[..],
            &mut gout_img[..],
            &mut bout_img[..],
            Some(&mut aout_img[..]),
            2,
            3,
            BitDepth::Uint8,
            AUTO_STRIDE,
            AUTO_STRIDE,
        )
        .unwrap();

        let config = Config::new().unwrap();

        let mut m = MatrixTransform::new();

        const OFFSET: [f64; 4] = [0.1, 0.2, 0.4007, 0.3007];
        m.set_offset(&OFFSET);

        let processor = config.processor(&Transform::from(m)).unwrap();
        let cpu_proc = processor
            .optimized_cpu_processor_with_bit_depths(
                BitDepth::F32,
                BitDepth::Uint8,
                OptimizationFlags::NONE,
            )
            .unwrap();

        cpu_proc
            .apply_src_dst(&src_img_desc, &mut dst_img_desc)
            .unwrap();
    }

    let rres_img: Vec<u8> = vec![0, 0, 51, 179, 255, 255];
    let gres_img: Vec<u8> = vec![0, 0, 115, 255, 255, 255];
    let bres_img: Vec<u8> = vec![0, 77, 217, 255, 255, 255];
    let ares_img: Vec<u8> = vec![78, 180, 255, 78, 255, 101];

    assert!(rout_img == rres_img);
    assert!(gout_img == gres_img);
    assert!(bout_img == bres_img);
    assert!(aout_img == ares_img);
}

/// Port of `OCIO_ADD_TEST(CPUProcessor, scanline_packed_one_buffer)` @ v2.5.2.
#[test]
fn scanline_packed_one_buffer() {
    // Now that the previous unit test covers all cases with different buffers,
    // let's test some cases using the same in and out buffer.

    let cpu_processor = build_cpu_processor(TransformDirection::Forward);

    let mut processing_img;

    {
        processing_img = in_img();

        process_in_place(&cpu_processor, &mut processing_img, NB_PIXELS, 1);
    }

    {
        processing_img = in_img();

        process_in_place(&cpu_processor, &mut processing_img, 3, 2);
    }

    {
        processing_img = in_img();

        process_in_place(&cpu_processor, &mut processing_img, 1, NB_PIXELS);
    }
}

/// The source planes of `scanline_planar`: `inImgR`, `inImgG`, `inImgB` and, unless `alpha` is
/// false, `inImgA`, of `width` by `height`, with the pixel and line strides given (`None`: the
/// short constructor).
fn planar_src(
    width: usize,
    height: usize,
    alpha: bool,
    strides: Option<(isize, isize)>,
) -> PlanarImageDesc<&'static [u8]> {
    let a = if alpha { Some(&IN_IMG_A[..]) } else { None };
    match strides {
        None => PlanarImageDesc::new(
            &IN_IMG_R[..],
            &IN_IMG_G[..],
            &IN_IMG_B[..],
            a,
            width,
            height,
        ),
        Some((x, y)) => PlanarImageDesc::with_strides(
            &IN_IMG_R[..],
            &IN_IMG_G[..],
            &IN_IMG_B[..],
            a,
            width,
            height,
            BitDepth::F32,
            x,
            y,
        ),
    }
    .unwrap()
}

/// Port of `Process(cpuProcessor, const PlanarImageDesc &, PlanarImageDesc &)`
/// (tests/cpu/CPUProcessor_tests.cpp:1425-1447 @ v2.5.2): processes `src` into planes of
/// `width` by `height`, with alpha unless `alpha` is false, and the pixel and line strides
/// given (`None`: the short constructor); then validates them.
#[track_caller]
fn process_planar(
    cpu_processor: &CpuProcessor,
    src: &PlanarImageDesc<&[u8]>,
    width: usize,
    height: usize,
    alpha: bool,
    strides: Option<(isize, isize)>,
) {
    let mut out_img_r = vec![0f32; NB_PIXELS];
    let mut out_img_g = vec![0f32; NB_PIXELS];
    let mut out_img_b = vec![0f32; NB_PIXELS];
    let mut out_img_a = vec![0f32; NB_PIXELS];
    {
        let a = if alpha {
            Some(&mut out_img_a[..])
        } else {
            None
        };
        let mut dst_img_desc = match strides {
            None => PlanarImageDesc::new(
                &mut out_img_r[..],
                &mut out_img_g[..],
                &mut out_img_b[..],
                a,
                width,
                height,
            ),
            Some((x, y)) => PlanarImageDesc::with_strides(
                &mut out_img_r[..],
                &mut out_img_g[..],
                &mut out_img_b[..],
                a,
                width,
                height,
                BitDepth::F32,
                x,
                y,
            ),
        }
        .unwrap();

        cpu_processor.apply_src_dst(src, &mut dst_img_desc).unwrap();
    }
    validate_planar(
        &out_img_r,
        &out_img_g,
        &out_img_b,
        alpha.then_some(&out_img_a[..]),
    );
}

/// Port of `OCIO_ADD_TEST(CPUProcessor, scanline_planar)` @ v2.5.2. Upstream's output planes
/// live for the whole test; each case here writes new ones, as each case writes every value
/// it validates.
#[test]
fn scanline_planar() {
    // Test the planar image description.

    let cpu_processor = build_cpu_processor(TransformDirection::Forward);

    for _ in 0..3 {
        let src = planar_src(NB_PIXELS, 1, true, None);
        process_planar(&cpu_processor, &src, NB_PIXELS, 1, true, None);
    }

    {
        let src = planar_src(3, 2, true, None);
        process_planar(&cpu_processor, &src, 3, 2, true, None);
    }

    {
        let src = planar_src(2, 3, true, None);
        process_planar(&cpu_processor, &src, 2, 3, true, Some((F, AUTO_STRIDE)));
    }

    {
        let src = planar_src(2, 3, true, None);
        process_planar(&cpu_processor, &src, 2, 3, true, Some((F, 2 * F)));
    }

    {
        let src = planar_src(2, 3, true, None);
        process_planar(&cpu_processor, &src, 2, 3, true, Some((AUTO_STRIDE, 2 * F)));
    }

    {
        let src = planar_src(2, 3, true, Some((AUTO_STRIDE, 2 * F)));
        process_planar(&cpu_processor, &src, 2, 3, true, Some((AUTO_STRIDE, 2 * F)));
    }

    {
        let src = planar_src(2, 3, true, Some((F, 2 * F)));
        process_planar(&cpu_processor, &src, 2, 3, true, Some((AUTO_STRIDE, 2 * F)));
    }

    {
        let src = planar_src(2, 3, true, Some((F, 2 * F)));
        process_planar(
            &cpu_processor,
            &src,
            2,
            3,
            false,
            Some((AUTO_STRIDE, 2 * F)),
        );
    }

    {
        let src = planar_src(2, 3, false, Some((F, 2 * F)));
        process_planar(
            &cpu_processor,
            &src,
            2,
            3,
            false,
            Some((AUTO_STRIDE, 2 * F)),
        );
    }
}

/// Port of `OCIO_ADD_TEST(CPUProcessor, scanline_packed_tile)` @ v2.5.2.
#[test]
fn scanline_packed_tile() {
    // Process tiles.

    let cpu_processor = build_cpu_processor(TransformDirection::Forward);

    let in_img = in_img();
    let res_img = res_img();
    let mut out_img = vec![0f32; NB_PIXELS * 4];

    {
        // Pixels are { 1, 2, 3,
        //              4, 5, 6  }

        // Copy the 1st pixel which should be untouched.
        out_img[..4].copy_from_slice(&res_img[..4]);
        // Copy the 4th pixel which should be untouched.
        out_img[3 * 4..3 * 4 + 4].copy_from_slice(&res_img[3 * 4..3 * 4 + 4]);

        // Only process the pixels = { 2, 3,
        //                             5, 6  }

        let src_img_desc = PackedImageDesc::with_strides(
            At(&in_img[..], 4 * F as usize),
            2,
            2,
            4, // width=2, height=2, and nchannels=4
            BitDepth::F32,
            F,
            4 * F,
            3 * 4 * F,
        )
        .unwrap();

        {
            let mut dst_img_desc = PackedImageDesc::with_strides(
                At(&mut out_img[..], 4 * F as usize),
                2,
                2,
                4, // width=2, height=2, and nchannels=4
                BitDepth::F32,
                F,
                4 * F,
                3 * 4 * F,
            )
            .unwrap();

            cpu_processor
                .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                .unwrap();
        }

        for pxl in 0..NB_PIXELS {
            check_close(out_img[4 * pxl], res_img[4 * pxl], 1e-6);
            check_close(out_img[4 * pxl + 1], res_img[4 * pxl + 1], 1e-6);
            check_close(out_img[4 * pxl + 2], res_img[4 * pxl + 2], 1e-6);
            check_close(out_img[4 * pxl + 3], res_img[4 * pxl + 3], 1e-6);
        }
    }

    {
        // Pixels are { 1, 2, 3,
        //              4, 5, 6  }

        // Copy the 3rd pixel which should be untouched.
        out_img[2 * 4..2 * 4 + 4].copy_from_slice(&res_img[2 * 4..2 * 4 + 4]);
        // Copy the 6th pixel which should be untouched.
        out_img[5 * 4..5 * 4 + 4].copy_from_slice(&res_img[5 * 4..5 * 4 + 4]);

        // Only process the pixels = { 1, 2,
        //                             4, 5 }

        let src_img_desc = PackedImageDesc::with_strides(
            &in_img[..],
            2,
            2,
            4, // width=2, height=2, and nchannels=4
            BitDepth::F32,
            F,
            4 * F,
            3 * 4 * F,
        )
        .unwrap();

        process(
            &cpu_processor,
            &src_img_desc,
            &mut out_img,
            Dst::Count(2, 2, F, 4 * F, 3 * 4 * F),
        );
    }

    {
        // Pixels are { 1, 2, 3,
        //              4, 5, 6  }

        out_img = in_img.clone(); // Use an in-place image buffer.

        // Copy the 3rd pixel which should be untouched.
        out_img[2 * 4..2 * 4 + 4].copy_from_slice(&res_img[2 * 4..2 * 4 + 4]);
        // Copy the 6th pixel which should be untouched.
        out_img[5 * 4..5 * 4 + 4].copy_from_slice(&res_img[5 * 4..5 * 4 + 4]);

        // Only process the pixels = { 1, 2,
        //                             4, 5 }

        {
            let mut dst_img_desc = PackedImageDesc::with_strides(
                &mut out_img[..],
                2,
                2,
                4, // width=2, height=2, and nchannels=4
                BitDepth::F32,
                F,
                4 * F,
                3 * 4 * F,
            )
            .unwrap();

            // Upstream's `Process(cpuProcessor, dstImgDesc, dstImgDesc)`: the one image as the
            // source and the destination.
            cpu_processor.apply_same(&mut dst_img_desc).unwrap();
        }
        validate(&out_img);
    }
}

/// `Converter<BIT_DEPTH_UINT8>::CastValue(255.0f * value)`.
fn to_u8(value: f32) -> u8 {
    Uint8::cast_value(255.0f32 * value)
}

/// Port of `OCIO_ADD_TEST(CPUProcessor, scanline_packed_custom)` @ v2.5.2.
#[test]
fn scanline_packed_custom() {
    // Cases testing custom xStrideInBytes and yStrideInBytes values.

    const MAGIC_NUMBER: f32 = 12345.6789;
    const WIDTH: usize = 3;
    const HEIGHT: usize = 2;

    const _: () = assert!(
        WIDTH * HEIGHT == NB_PIXELS,
        "Validation of the image dimensions"
    );

    let cpu_processor = build_cpu_processor(TransformDirection::Forward);

    let in_img = in_img();
    let res_img = res_img();
    let w = WIDTH as isize;

    {
        // Pixels are { RGBA, RGBA, RGBA,
        //              RGBA, RGBA, RGBA  }.

        let img: Vec<f32> = in_img.clone();

        // NB: Do not use OCIO::AutoStride for the y stride to test a custom value.
        let y_stride_in_bytes: isize = w * 4 * F;

        {
            // Test a positive y stride.

            // It means to start the processing from the first pixel of the first line.
            let src_img_desc = PackedImageDesc::with_strides(
                &img[..],
                WIDTH,
                HEIGHT,
                4,
                BitDepth::F32,
                AUTO_STRIDE,
                AUTO_STRIDE,
                // Bytes to the next line.
                y_stride_in_bytes,
            )
            .unwrap();

            let mut out_img = vec![0f32; NB_PIXELS * 4];
            {
                let mut dst_img_desc =
                    PackedImageDesc::new(&mut out_img[..], WIDTH, HEIGHT, 4).unwrap();

                cpu_processor
                    .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                    .unwrap();
            }

            for pxl in 0..NB_PIXELS {
                let dep = 4 * pxl;
                check_close(out_img[dep], res_img[dep], 1e-6);
                check_close(out_img[dep + 1], res_img[dep + 1], 1e-6);
                check_close(out_img[dep + 2], res_img[dep + 2], 1e-6);
                check_close(out_img[dep + 3], res_img[dep + 3], 1e-6);
            }
        }
        {
            // Test a negative y stride.
            //
            // Note: It 'inverts' the processed image i.e. the last line is then moved to become
            // the first line and so on.

            // It means to start the processing from the first pixel of the last line.
            let src_img_desc = PackedImageDesc::with_strides(
                At(&img[..], y_stride_in_bytes as usize),
                WIDTH,
                HEIGHT,
                4,
                BitDepth::F32,
                AUTO_STRIDE,
                AUTO_STRIDE,
                // Bytes to the next line.
                -y_stride_in_bytes,
            )
            .unwrap();

            // Output to 32-bits float.

            let mut float_out_img = vec![0f32; NB_PIXELS * 4];
            {
                let mut float_dst_img_desc =
                    PackedImageDesc::new(&mut float_out_img[..], WIDTH, HEIGHT, 4).unwrap();

                cpu_processor
                    .apply_src_dst(&src_img_desc, &mut float_dst_img_desc)
                    .unwrap();
            }

            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let out_dep = (HEIGHT - y - 1) * WIDTH + x;
                    let res_dep = y * WIDTH + x;

                    check_close(float_out_img[4 * out_dep], res_img[4 * res_dep], 1e-6);
                    check_close(
                        float_out_img[4 * out_dep + 1],
                        res_img[4 * res_dep + 1],
                        1e-6,
                    );
                    check_close(
                        float_out_img[4 * out_dep + 2],
                        res_img[4 * res_dep + 2],
                        1e-6,
                    );
                    check_close(
                        float_out_img[4 * out_dep + 3],
                        res_img[4 * res_dep + 3],
                        1e-6,
                    );
                }
            }

            // Output to 8-bits integer.

            let mut char_out_img = vec![0u8; NB_PIXELS * 4];

            let new_proc = build_processor(TransformDirection::Forward);
            let mut new_cpu = new_proc
                .optimized_cpu_processor_with_bit_depths(
                    BitDepth::F32,
                    BitDepth::Uint8,
                    OptimizationFlags::NONE,
                )
                .unwrap();

            {
                let mut char_dst_img_desc = PackedImageDesc::with_strides(
                    &mut char_out_img[..],
                    WIDTH,
                    HEIGHT,
                    4,
                    BitDepth::Uint8,
                    AUTO_STRIDE,
                    AUTO_STRIDE,
                    AUTO_STRIDE,
                )
                .unwrap();

                new_cpu
                    .apply_src_dst(&src_img_desc, &mut char_dst_img_desc)
                    .unwrap();
            }

            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let out_dep = (HEIGHT - y - 1) * WIDTH + x;
                    let res_dep = y * WIDTH + x;

                    let red = to_u8(res_img[4 * res_dep]);
                    assert_eq!(char_out_img[4 * out_dep], red);

                    let green = to_u8(res_img[4 * res_dep + 1]);
                    assert_eq!(char_out_img[4 * out_dep + 1], green);

                    let blue = to_u8(res_img[4 * res_dep + 2]);
                    assert_eq!(char_out_img[4 * out_dep + 2], blue);

                    let alpha = to_u8(res_img[4 * res_dep + 3]);
                    assert_eq!(char_out_img[4 * out_dep + 3], alpha);
                }
            }

            // Output to 8-bits integer with a negative y stride.

            let out_y_stride_in_bytes: isize = w * 4 * size_of::<u8>() as isize;

            char_out_img.fill(0);

            new_cpu = new_proc
                .optimized_cpu_processor_with_bit_depths(
                    BitDepth::F32,
                    BitDepth::Uint8,
                    OptimizationFlags::NONE,
                )
                .unwrap();

            {
                let mut new_char_dst_img_desc = PackedImageDesc::with_strides(
                    At(&mut char_out_img[..], out_y_stride_in_bytes as usize),
                    WIDTH,
                    HEIGHT,
                    4,
                    BitDepth::Uint8,
                    AUTO_STRIDE,
                    AUTO_STRIDE,
                    -out_y_stride_in_bytes,
                )
                .unwrap();

                new_cpu
                    .apply_src_dst(&src_img_desc, &mut new_char_dst_img_desc)
                    .unwrap();
            }

            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let dep = y * WIDTH + x;

                    let red = to_u8(res_img[4 * dep]);
                    assert_eq!(char_out_img[4 * dep], red);

                    let green = to_u8(res_img[4 * dep + 1]);
                    assert_eq!(char_out_img[4 * dep + 1], green);

                    let blue = to_u8(res_img[4 * dep + 2]);
                    assert_eq!(char_out_img[4 * dep + 2], blue);

                    let alpha = to_u8(res_img[4 * dep + 3]);
                    assert_eq!(char_out_img[4 * dep + 3], alpha);
                }
            }
        }
        {
            // Test a negative y stride for the in and out images.
            //
            // Note: For the two images, the processing starts from the last line which means
            // to process from the first pixel of the last line for the two image buffers.

            let src_img_desc = PackedImageDesc::with_strides(
                At(&img[..], y_stride_in_bytes as usize),
                WIDTH,
                HEIGHT,
                4,
                BitDepth::F32,
                AUTO_STRIDE,
                AUTO_STRIDE,
                // Bytes to the next line.
                -y_stride_in_bytes,
            )
            .unwrap();

            let mut out_img = vec![0f32; NB_PIXELS * 4];
            {
                let mut dst_img_desc = PackedImageDesc::with_strides(
                    At(
                        &mut out_img[..],
                        WIDTH * 4 * (HEIGHT - 1) * size_of::<f32>(),
                    ),
                    WIDTH,
                    HEIGHT,
                    4,
                    BitDepth::F32,
                    AUTO_STRIDE,
                    AUTO_STRIDE,
                    // Bytes to the next line.
                    -(w * 4 * F),
                )
                .unwrap();

                cpu_processor
                    .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                    .unwrap();
            }

            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let dep = y * WIDTH + x;

                    check_close(out_img[4 * dep], res_img[4 * dep], 1e-6);
                    check_close(out_img[4 * dep + 1], res_img[4 * dep + 1], 1e-6);
                    check_close(out_img[4 * dep + 2], res_img[4 * dep + 2], 1e-6);
                    check_close(out_img[4 * dep + 3], res_img[4 * dep + 3], 1e-6);
                }
            }
        }
        {
            // Test a positive y stride with a negative x stride.
            //
            // Note: It 'inverts' the lines of the processed image i.e. the last pixel of a line is
            // then moved to become the first pxel of the same line and so on.

            // It means to start the processing from the last pixel of the first line.
            let src_img_desc = PackedImageDesc::with_strides(
                At(&img[..], (WIDTH - 1) * 4 * size_of::<f32>()),
                WIDTH,
                HEIGHT,
                4,
                BitDepth::F32,
                AUTO_STRIDE,
                -(4 * F),
                // Bytes to the next line.
                y_stride_in_bytes,
            )
            .unwrap();

            let mut out_img = vec![0f32; NB_PIXELS * 4];
            {
                let mut dst_img_desc =
                    PackedImageDesc::new(&mut out_img[..], WIDTH, HEIGHT, 4).unwrap();

                cpu_processor
                    .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                    .unwrap();
            }

            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let out_dep = y * WIDTH + (WIDTH - x - 1);
                    let res_dep = y * WIDTH + x;

                    check_close(out_img[4 * out_dep], res_img[4 * res_dep], 1e-6);
                    check_close(out_img[4 * out_dep + 1], res_img[4 * res_dep + 1], 1e-6);
                    check_close(out_img[4 * out_dep + 2], res_img[4 * res_dep + 2], 1e-6);
                    check_close(out_img[4 * out_dep + 3], res_img[4 * res_dep + 3], 1e-6);
                }
            }
        }
    }
    {
        // Pixels are { RGBA, RGBA, RGBA, x,
        //              RGBA, RGBA, RGBA, x  } where x is not a color channel.

        let mut img: Vec<f32> = Vec::new();
        img.extend_from_slice(&in_img[0..12]);
        img.push(MAGIC_NUMBER);
        img.extend_from_slice(&in_img[12..24]);
        img.push(MAGIC_NUMBER);

        let y_stride_in_bytes: isize = w * 4 * F + F;

        {
            // Test a positive y stride.

            // It means to start the processing from the first pixel of the first line.
            let src_img_desc = PackedImageDesc::with_strides(
                &img[..],
                WIDTH,
                HEIGHT,
                4,
                BitDepth::F32,
                AUTO_STRIDE,
                AUTO_STRIDE,
                // Bytes to the next line.
                y_stride_in_bytes,
            )
            .unwrap();

            let mut out_img = vec![0f32; NB_PIXELS * 4];
            {
                let mut dst_img_desc =
                    PackedImageDesc::new(&mut out_img[..], WIDTH, HEIGHT, 4).unwrap();

                cpu_processor
                    .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                    .unwrap();
            }

            for pxl in 0..NB_PIXELS {
                check_close(out_img[4 * pxl], res_img[4 * pxl], 1e-6);
                check_close(out_img[4 * pxl + 1], res_img[4 * pxl + 1], 1e-6);
                check_close(out_img[4 * pxl + 2], res_img[4 * pxl + 2], 1e-6);
                check_close(out_img[4 * pxl + 3], res_img[4 * pxl + 3], 1e-6);
            }
        }
        {
            // Test a negative y stride.
            //
            // Note: It 'inverts' the processed image i.e. the last line is then moved to become
            // the first line and so on.

            // It means to start the processing from the first pixel of the last line.
            let src_img_desc = PackedImageDesc::with_strides(
                At(&img[..], y_stride_in_bytes as usize),
                WIDTH,
                HEIGHT,
                4,
                BitDepth::F32,
                AUTO_STRIDE,
                AUTO_STRIDE,
                // Bytes to the next line.
                -y_stride_in_bytes,
            )
            .unwrap();

            let mut out_img = vec![0f32; NB_PIXELS * 4];
            {
                let mut dst_img_desc =
                    PackedImageDesc::new(&mut out_img[..], WIDTH, HEIGHT, 4).unwrap();

                cpu_processor
                    .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                    .unwrap();
            }

            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let out_dep = (HEIGHT - y - 1) * WIDTH + x;
                    let res_dep = y * WIDTH + x;

                    check_close(out_img[4 * out_dep], res_img[4 * res_dep], 1e-6);
                    check_close(out_img[4 * out_dep + 1], res_img[4 * res_dep + 1], 1e-6);
                    check_close(out_img[4 * out_dep + 2], res_img[4 * res_dep + 2], 1e-6);
                    check_close(out_img[4 * out_dep + 3], res_img[4 * res_dep + 3], 1e-6);
                }
            }
        }
        {
            // Test a negative y stride for the in and out images.
            //
            // Note: For the two images, the processing starts from the last line which means
            // to process from the first pixel of the last line for the two image buffers.

            let src_img_desc = PackedImageDesc::with_strides(
                At(&img[..], y_stride_in_bytes as usize),
                WIDTH,
                HEIGHT,
                4,
                BitDepth::F32,
                AUTO_STRIDE,
                AUTO_STRIDE,
                // Bytes to the next line.
                -y_stride_in_bytes,
            )
            .unwrap();

            let mut out_img = vec![0f32; NB_PIXELS * 4];
            {
                let mut dst_img_desc = PackedImageDesc::with_strides(
                    At(
                        &mut out_img[..],
                        WIDTH * 4 * (HEIGHT - 1) * size_of::<f32>(),
                    ),
                    WIDTH,
                    HEIGHT,
                    4,
                    BitDepth::F32,
                    AUTO_STRIDE,
                    AUTO_STRIDE,
                    // Bytes to the next line.
                    -(w * 4 * F),
                )
                .unwrap();

                cpu_processor
                    .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                    .unwrap();
            }

            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let dep = y * WIDTH + x;

                    check_close(out_img[4 * dep], res_img[4 * dep], 1e-6);
                    check_close(out_img[4 * dep + 1], res_img[4 * dep + 1], 1e-6);
                    check_close(out_img[4 * dep + 2], res_img[4 * dep + 2], 1e-6);
                    check_close(out_img[4 * dep + 3], res_img[4 * dep + 3], 1e-6);
                }
            }
        }
        {
            // Test a positive y stride with a negative x stride.
            //
            // Note: It 'inverts' the lines of the processed image i.e. the last pixel of a line is
            // then moved to become the first pxel of the same line and so on.

            // It means to start the processing from the last pixel of the first line.
            let src_img_desc = PackedImageDesc::with_strides(
                At(&img[..], (WIDTH - 1) * 4 * size_of::<f32>()),
                WIDTH,
                HEIGHT,
                4,
                BitDepth::F32,
                AUTO_STRIDE,
                -(4 * F),
                // Bytes to the next line.
                y_stride_in_bytes,
            )
            .unwrap();

            let mut out_img = vec![0f32; NB_PIXELS * 4];
            {
                let mut dst_img_desc =
                    PackedImageDesc::new(&mut out_img[..], WIDTH, HEIGHT, 4).unwrap();

                cpu_processor
                    .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                    .unwrap();
            }

            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let out_dep = y * WIDTH + (WIDTH - x - 1);
                    let res_dep = y * WIDTH + x;

                    check_close(out_img[4 * out_dep], res_img[4 * res_dep], 1e-6);
                    check_close(out_img[4 * out_dep + 1], res_img[4 * res_dep + 1], 1e-6);
                    check_close(out_img[4 * out_dep + 2], res_img[4 * res_dep + 2], 1e-6);
                    check_close(out_img[4 * out_dep + 3], res_img[4 * res_dep + 3], 1e-6);
                }
            }
        }
    }

    {
        // Pixels are { RxGxBxAx, RxGxBxAx, RxGxBxAx,
        //              RxGxBxAx, RxGxBxAx, RxGxBxAx  } where x is not a color channel.

        let img: Vec<f32> = in_img.iter().flat_map(|&v| [v, MAGIC_NUMBER]).collect();

        let chan_in_bytes: isize = F + F;
        let x_stride_in_bytes: isize = chan_in_bytes * 4;
        let y_stride_in_bytes: isize = x_stride_in_bytes * w;

        {
            let src_img_desc = PackedImageDesc::with_strides(
                &img[..],
                WIDTH,
                HEIGHT,
                4,
                BitDepth::F32,
                // Bytes to the next color channel.
                chan_in_bytes,
                AUTO_STRIDE,
                AUTO_STRIDE,
            )
            .unwrap();

            let mut out_img = vec![0f32; NB_PIXELS * 3];
            {
                let mut dst_img_desc =
                    PackedImageDesc::new(&mut out_img[..], WIDTH, HEIGHT, 3).unwrap();

                cpu_processor
                    .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                    .unwrap();
            }

            for pxl in 0..NB_PIXELS {
                check_close(out_img[3 * pxl], res_img[4 * pxl], 1e-6);
                check_close(out_img[3 * pxl + 1], res_img[4 * pxl + 1], 1e-6);
                check_close(out_img[3 * pxl + 2], res_img[4 * pxl + 2], 1e-6);
            }
        }
        {
            // Test with a negative y stride.

            let src_img_desc = PackedImageDesc::with_strides(
                At(&img[..], y_stride_in_bytes as usize),
                WIDTH,
                HEIGHT,
                4,
                BitDepth::F32,
                // Bytes to the next color channel.
                chan_in_bytes,
                AUTO_STRIDE,
                // Bytes to the next line.
                -y_stride_in_bytes,
            )
            .unwrap();

            let mut out_img = vec![0f32; NB_PIXELS * 3];
            {
                let mut dst_img_desc =
                    PackedImageDesc::new(&mut out_img[..], WIDTH, HEIGHT, 3).unwrap();

                cpu_processor
                    .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                    .unwrap();
            }

            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let out_dep = (HEIGHT - y - 1) * WIDTH + x;
                    let res_dep = y * WIDTH + x;

                    check_close(out_img[3 * out_dep], res_img[4 * res_dep], 1e-6);
                    check_close(out_img[3 * out_dep + 1], res_img[4 * res_dep + 1], 1e-6);
                    check_close(out_img[3 * out_dep + 2], res_img[4 * res_dep + 2], 1e-6);
                }
            }
        }
        {
            // Test with a negative x stride.

            let src_img_desc = PackedImageDesc::with_strides(
                At(&img[..], x_stride_in_bytes as usize * (WIDTH - 1)),
                WIDTH,
                HEIGHT,
                4,
                BitDepth::F32,
                // Bytes to the next color channel.
                chan_in_bytes,
                // Bytes to the next pixel.
                -x_stride_in_bytes,
                // Bytes to the next line.
                y_stride_in_bytes,
            )
            .unwrap();

            let mut out_img = vec![0f32; NB_PIXELS * 3];
            {
                let mut dst_img_desc =
                    PackedImageDesc::new(&mut out_img[..], WIDTH, HEIGHT, 3).unwrap();

                cpu_processor
                    .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                    .unwrap();
            }

            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let out_dep = y * WIDTH + (WIDTH - x - 1);
                    let res_dep = y * WIDTH + x;

                    check_close(out_img[3 * out_dep], res_img[4 * res_dep], 1e-6);
                    check_close(out_img[3 * out_dep + 1], res_img[4 * res_dep + 1], 1e-6);
                    check_close(out_img[3 * out_dep + 2], res_img[4 * res_dep + 2], 1e-6);
                }
            }
        }
    }

    {
        // Pixels are { RGBAx, RGBAx, RGBAx,
        //              RGBAx, RGBAx, RGBAx  } where x is not a color channel.

        let img: Vec<f32> = (0..NB_PIXELS)
            .flat_map(|p| {
                let px = &in_img[4 * p..4 * p + 4];
                [px[0], px[1], px[2], px[3], MAGIC_NUMBER]
            })
            .collect();

        let src_img_desc = PackedImageDesc::with_strides(
            &img[..],
            WIDTH,
            HEIGHT,
            4,
            BitDepth::F32,
            AUTO_STRIDE,
            // Bytes to the next pixel.
            4 * F + F,
            AUTO_STRIDE,
        )
        .unwrap();

        let mut out_img = vec![0f32; NB_PIXELS * 3];
        {
            let mut dst_img_desc =
                PackedImageDesc::new(&mut out_img[..], WIDTH, HEIGHT, 3).unwrap();

            cpu_processor
                .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                .unwrap();
        }

        for pxl in 0..NB_PIXELS {
            check_close(out_img[3 * pxl], res_img[4 * pxl], 1e-6);
            check_close(out_img[3 * pxl + 1], res_img[4 * pxl + 1], 1e-6);
            check_close(out_img[3 * pxl + 2], res_img[4 * pxl + 2], 1e-6);
        }
    }

    {
        // Pixels are { RGBAx, RGBAx, RGBAx, x
        //              RGBAx, RGBAx, RGBAx, x  } where x is not a color channel.

        let mut img: Vec<f32> = Vec::new();
        for line in 0..HEIGHT {
            for x in 0..WIDTH {
                let px = &in_img[4 * (line * WIDTH + x)..4 * (line * WIDTH + x) + 4];
                img.extend_from_slice(&[px[0], px[1], px[2], px[3], MAGIC_NUMBER]);
            }
            img.push(MAGIC_NUMBER);
        }

        let src_img_desc = PackedImageDesc::with_strides(
            &img[..],
            WIDTH,
            HEIGHT,
            4,
            BitDepth::F32,
            AUTO_STRIDE,
            // Bytes to the next pixel.
            4 * F + F,
            // Bytes to the next line.
            w * (4 * F + F) + F,
        )
        .unwrap();

        let mut out_img = vec![0f32; NB_PIXELS * 3];
        {
            let mut dst_img_desc =
                PackedImageDesc::new(&mut out_img[..], WIDTH, HEIGHT, 3).unwrap();

            cpu_processor
                .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                .unwrap();
        }

        for pxl in 0..NB_PIXELS {
            check_close(out_img[3 * pxl], res_img[4 * pxl], 1e-6);
            check_close(out_img[3 * pxl + 1], res_img[4 * pxl + 1], 1e-6);
            check_close(out_img[3 * pxl + 2], res_img[4 * pxl + 2], 1e-6);
        }
    }
}

/// Port of `OCIO_ADD_TEST(CPUProcessor, scanline_planar_custom)` @ v2.5.2.
#[test]
fn scanline_planar_custom() {
    // Cases testing custom stride values for planar.

    const WIDTH: usize = 3;
    const HEIGHT: usize = 2;

    const _: () = assert!(
        WIDTH * HEIGHT == NB_PIXELS,
        "Validation of the image dimensions"
    );

    let w = WIDTH as isize;
    let w_bytes = WIDTH * size_of::<f32>();

    {
        // Test with default strides.

        let cpu_processor = build_cpu_processor(TransformDirection::Forward);

        let mut out_img_r = vec![0f32; NB_PIXELS];
        let mut out_img_g = vec![0f32; NB_PIXELS];
        let mut out_img_b = vec![0f32; NB_PIXELS];
        let mut out_img_a = vec![0f32; NB_PIXELS];

        let src_img_desc = PlanarImageDesc::with_strides(
            &IN_IMG_R[..],
            &IN_IMG_G[..],
            &IN_IMG_B[..],
            Some(&IN_IMG_A[..]),
            WIDTH,
            HEIGHT,
            BitDepth::F32,
            AUTO_STRIDE,
            w * F,
        )
        .unwrap();
        {
            let mut dst_img_desc = PlanarImageDesc::new(
                &mut out_img_r[..],
                &mut out_img_g[..],
                &mut out_img_b[..],
                Some(&mut out_img_a[..]),
                WIDTH,
                HEIGHT,
            )
            .unwrap();

            cpu_processor
                .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                .unwrap();
        }

        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let dep = y * WIDTH + x;
                check_close(out_img_r[dep], RES_IMG_R[dep], 1e-6);
                check_close(out_img_g[dep], RES_IMG_G[dep], 1e-6);
                check_close(out_img_b[dep], RES_IMG_B[dep], 1e-6);
                check_close(out_img_a[dep], RES_IMG_A[dep], 1e-6);
            }
        }
    }

    {
        // Test with default strides, and output in 8-bits integer.

        let processor = build_processor(TransformDirection::Forward);

        let cpu_processor = processor
            .optimized_cpu_processor_with_bit_depths(
                BitDepth::F32,
                BitDepth::Uint8,
                OptimizationFlags::NONE,
            )
            .unwrap();

        let mut out_img_r = vec![0u8; NB_PIXELS];
        let mut out_img_g = vec![0u8; NB_PIXELS];
        let mut out_img_b = vec![0u8; NB_PIXELS];
        let mut out_img_a = vec![0u8; NB_PIXELS];

        let src_img_desc = PlanarImageDesc::with_strides(
            &IN_IMG_R[..],
            &IN_IMG_G[..],
            &IN_IMG_B[..],
            Some(&IN_IMG_A[..]),
            WIDTH,
            HEIGHT,
            BitDepth::F32,
            AUTO_STRIDE,
            w * F,
        )
        .unwrap();
        {
            let mut dst_img_desc = PlanarImageDesc::with_strides(
                &mut out_img_r[..],
                &mut out_img_g[..],
                &mut out_img_b[..],
                Some(&mut out_img_a[..]),
                WIDTH,
                HEIGHT,
                BitDepth::Uint8,
                AUTO_STRIDE,
                AUTO_STRIDE,
            )
            .unwrap();

            cpu_processor
                .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                .unwrap();
        }

        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let dep = y * WIDTH + x;

                let red = to_u8(RES_IMG_R[dep]);
                assert_eq!(out_img_r[dep], red);

                let green = to_u8(RES_IMG_G[dep]);
                assert_eq!(out_img_g[dep], green);

                let blue = to_u8(RES_IMG_B[dep]);
                assert_eq!(out_img_b[dep], blue);

                let alpha = to_u8(RES_IMG_A[dep]);
                assert_eq!(out_img_a[dep], alpha);
            }
        }
    }

    {
        // Test with a negative y stride.

        let cpu_processor = build_cpu_processor(TransformDirection::Forward);

        let mut out_img_r = vec![0f32; NB_PIXELS];
        let mut out_img_g = vec![0f32; NB_PIXELS];
        let mut out_img_b = vec![0f32; NB_PIXELS];
        let mut out_img_a = vec![0f32; NB_PIXELS];

        let src_img_desc = PlanarImageDesc::with_strides(
            At(&IN_IMG_R[..], w_bytes),
            At(&IN_IMG_G[..], w_bytes),
            At(&IN_IMG_B[..], w_bytes),
            Some(At(&IN_IMG_A[..], w_bytes)),
            WIDTH,
            HEIGHT,
            BitDepth::F32,
            AUTO_STRIDE,
            -(w * F),
        )
        .unwrap();
        {
            let mut dst_img_desc = PlanarImageDesc::new(
                &mut out_img_r[..],
                &mut out_img_g[..],
                &mut out_img_b[..],
                Some(&mut out_img_a[..]),
                WIDTH,
                HEIGHT,
            )
            .unwrap();

            cpu_processor
                .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                .unwrap();
        }

        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let out_dep = (HEIGHT - y - 1) * WIDTH + x;
                let res_dep = y * WIDTH + x;
                check_close(out_img_r[out_dep], RES_IMG_R[res_dep], 1e-6);
                check_close(out_img_g[out_dep], RES_IMG_G[res_dep], 1e-6);
                check_close(out_img_b[out_dep], RES_IMG_B[res_dep], 1e-6);
                check_close(out_img_a[out_dep], RES_IMG_A[res_dep], 1e-6);
            }
        }
    }

    {
        // Test with negative y strides on in and out buffers, and output in 8-bits integer.

        let processor = build_processor(TransformDirection::Forward);

        let cpu_processor = processor
            .optimized_cpu_processor_with_bit_depths(
                BitDepth::F32,
                BitDepth::Uint8,
                OptimizationFlags::NONE,
            )
            .unwrap();

        let mut out_img_r = vec![0u8; NB_PIXELS];
        let mut out_img_g = vec![0u8; NB_PIXELS];
        let mut out_img_b = vec![0u8; NB_PIXELS];
        let mut out_img_a = vec![0u8; NB_PIXELS];

        let src_img_desc = PlanarImageDesc::with_strides(
            At(&IN_IMG_R[..], w_bytes),
            At(&IN_IMG_G[..], w_bytes),
            At(&IN_IMG_B[..], w_bytes),
            Some(At(&IN_IMG_A[..], w_bytes)),
            WIDTH,
            HEIGHT,
            BitDepth::F32,
            AUTO_STRIDE,
            -(w * F),
        )
        .unwrap();
        {
            let mut dst_img_desc = PlanarImageDesc::with_strides(
                At(&mut out_img_r[..], WIDTH),
                At(&mut out_img_g[..], WIDTH),
                At(&mut out_img_b[..], WIDTH),
                Some(At(&mut out_img_a[..], WIDTH)),
                WIDTH,
                HEIGHT,
                BitDepth::Uint8,
                AUTO_STRIDE,
                -(w * size_of::<u8>() as isize),
            )
            .unwrap();

            cpu_processor
                .apply_src_dst(&src_img_desc, &mut dst_img_desc)
                .unwrap();
        }

        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let dep = y * WIDTH + x;

                let red = to_u8(RES_IMG_R[dep]);
                assert_eq!(out_img_r[dep], red);

                let green = to_u8(RES_IMG_G[dep]);
                assert_eq!(out_img_g[dep], green);

                let blue = to_u8(RES_IMG_B[dep]);
                assert_eq!(out_img_b[dep], blue);

                let alpha = to_u8(RES_IMG_A[dep]);
                assert_eq!(out_img_a[dep], alpha);
            }
        }
    }
}

/// Port of `OCIO_ADD_TEST(CPUProcessor, one_pixel)` @ v2.5.2.
#[test]
fn one_pixel() {
    let cpu_processor = build_cpu_processor(TransformDirection::Forward);

    // The CPU Processor only includes a Matrix with offset:
    //   const float offset4[4] = { 1.4002f, 0.4005f, 0.8007f, 0.5007f };

    {
        let mut pixel: [f32; 4] = [0.1, 0.3, 0.9, 1.0];

        cpu_processor.apply_rgba(&mut pixel).unwrap();

        assert_eq!(pixel[0], 0.1f32 + 1.4002f32);
        assert_eq!(pixel[1], 0.3f32 + 0.4005f32);
        assert_eq!(pixel[2], 0.9f32 + 0.8007f32);
        assert_eq!(pixel[3], 1.0f32 + 0.5007f32);
    }

    {
        let mut pixel: [f32; 3] = [0.1, 0.3, 0.9];

        cpu_processor.apply_rgb(&mut pixel).unwrap();

        assert_eq!(pixel[0], 0.1f32 + 1.4002f32);
        assert_eq!(pixel[1], 0.3f32 + 0.4005f32);
        assert_eq!(pixel[2], 0.9f32 + 0.8007f32);
    }
}

/// Port of `ComputeImage<inBD, outBD>` (tests/cpu/CPUProcessor_tests.cpp:2725-2822 @ v2.5.2).
#[track_caller]
fn compute_image<I: BitDepthInfo, O: BitDepthInfo + Converter>(
    width: usize,
    height: usize,
    n_channels: usize,
    in_buf: &[I::Type],
    out_buf: &mut [O::Type],
) where
    O::Type: PartialEq,
{
    let config = Config::new().unwrap();

    let mut transform = MatrixTransform::new();
    const OFFSET4: [f64; 4] = [1.2002, 0.4005, 0.8007, 0.5];
    transform.set_offset(&OFFSET4);

    let processor = config.processor(&Transform::from(transform)).unwrap();

    let cpu_processor = processor
        .optimized_cpu_processor_with_bit_depths(
            I::BIT_DEPTH,
            O::BIT_DEPTH,
            OptimizationFlags::DEFAULT,
        )
        .unwrap();

    let in_bytes = to_bytes(in_buf);
    let src_img_desc = PackedImageDesc::with_strides(
        Bytes(&in_bytes[..]),
        width,
        height,
        n_channels,
        I::BIT_DEPTH,
        AUTO_STRIDE,
        AUTO_STRIDE,
        AUTO_STRIDE,
    )
    .unwrap();

    let mut out_bytes = to_bytes(out_buf);
    {
        let mut dst_img_desc = PackedImageDesc::with_strides(
            Bytes(&mut out_bytes[..]),
            width,
            height,
            n_channels,
            O::BIT_DEPTH,
            size_of::<O::Type>() as isize,
            AUTO_STRIDE,
            AUTO_STRIDE,
        )
        .unwrap();

        cpu_processor
            .apply_src_dst(&src_img_desc, &mut dst_img_desc)
            .unwrap();
    }
    out_buf.copy_from_slice(&from_bytes::<O::Type>(&out_bytes));

    let in_values = in_buf;
    let out_values = &*out_buf;

    let in_scale = (get_bit_depth_max_value(BitDepth::F32).unwrap()
        / get_bit_depth_max_value(I::BIT_DEPTH).unwrap()) as f32;

    let out_scale = (get_bit_depth_max_value(O::BIT_DEPTH).unwrap()
        / get_bit_depth_max_value(BitDepth::F32).unwrap()) as f32;

    let mut idx = 0;
    while idx < width * height {
        // Manual computation of the results.
        // Break operations into steps similar to cpu processor
        // to avoid potential fma compiler optimizations
        let in_scale4: [f32; 4] = [
            in_values[idx].to_float() * in_scale,
            in_values[idx + 1].to_float() * in_scale,
            in_values[idx + 2].to_float() * in_scale,
            if n_channels == 4 {
                in_values[idx + 3].to_float() * in_scale
            } else {
                0.0
            },
        ];

        let operation: [f32; 4] = [
            in_scale4[0] + OFFSET4[0] as f32,
            in_scale4[1] + OFFSET4[1] as f32,
            in_scale4[2] + OFFSET4[2] as f32,
            in_scale4[3] + OFFSET4[3] as f32,
        ];

        let pxl: [f32; 4] = [
            operation[0] * out_scale,
            operation[1] * out_scale,
            operation[2] * out_scale,
            operation[3] * out_scale,
        ];

        // Validate all the results.

        if O::IS_FLOAT {
            check_close(out_values[idx].to_float(), pxl[0], 1e-6);
            check_close(out_values[idx + 1].to_float(), pxl[1], 1e-6);
            check_close(out_values[idx + 2].to_float(), pxl[2], 1e-6);
            if n_channels == 4 {
                check_close(out_values[idx + 3].to_float(), pxl[3], 1e-6);
            }
        } else {
            assert_eq!(out_values[idx], O::cast_value(pxl[0]), "index {idx}");
            assert_eq!(out_values[idx + 1], O::cast_value(pxl[1]), "index {idx}");
            assert_eq!(out_values[idx + 2], O::cast_value(pxl[2]), "index {idx}");
            if n_channels == 4 {
                assert_eq!(out_values[idx + 3], O::cast_value(pxl[3]), "index {idx}");
            }
        }

        idx += n_channels;
    }
}

/// Port of `OCIO_ADD_TEST(CPUProcessor, optimizations)` @ v2.5.2.
#[test]
fn optimizations() {
    // The unit test validates some 'optimization' paths now implemented
    // by the ScanlineHelper class. To fully validate these paths a 'normal' image
    // must be used (i.e. 'few pixels' image is not enough).

    const WIDTH: usize = 640;
    const HEIGHT: usize = 480;
    const N_CHANNELS: usize = 4;

    let u16_ramp = |len: usize| -> Vec<u16> {
        (0..len)
            .map(|idx| (idx % Uint16::MAX_VALUE as usize) as u16)
            .collect()
    };
    let f32_ramp =
        |len: usize| -> Vec<f32> { (0..len).map(|idx| idx as f32 / len as f32).collect() };

    // Input and Output are not packed RGBA i.e no optimizations.
    {
        let in_buf = u16_ramp(WIDTH * HEIGHT * 3);

        let mut out_buf = vec![0u16; WIDTH * HEIGHT * 3];

        compute_image::<Uint16, Uint16>(WIDTH, HEIGHT, 3, &in_buf, &mut out_buf);
    }

    // Input and Output are packed RGBA but not F32.
    {
        let in_buf = u16_ramp(WIDTH * HEIGHT * N_CHANNELS);

        let mut out_buf = vec![0u16; WIDTH * HEIGHT * N_CHANNELS];

        compute_image::<Uint16, Uint16>(WIDTH, HEIGHT, N_CHANNELS, &in_buf, &mut out_buf);
    }

    // Input is packed RGBA but not F32, and output is packed RGBA F32.
    {
        let in_buf = u16_ramp(WIDTH * HEIGHT * N_CHANNELS);

        let mut out_buf = vec![0f32; WIDTH * HEIGHT * N_CHANNELS];

        compute_image::<Uint16, F32>(WIDTH, HEIGHT, N_CHANNELS, &in_buf, &mut out_buf);
    }

    // Input is packed RGBA F32, and output is packed RGBA but not F32.
    {
        let in_buf = f32_ramp(WIDTH * HEIGHT * N_CHANNELS);

        let mut out_buf = vec![0u16; WIDTH * HEIGHT * N_CHANNELS];

        compute_image::<F32, Uint16>(WIDTH, HEIGHT, N_CHANNELS, &in_buf, &mut out_buf);
    }

    // Input and output are both packed RGBA F32.
    {
        let in_buf = f32_ramp(WIDTH * HEIGHT * N_CHANNELS);

        let mut out_buf = vec![0f32; WIDTH * HEIGHT * N_CHANNELS];

        compute_image::<F32, F32>(WIDTH, HEIGHT, N_CHANNELS, &in_buf, &mut out_buf);
    }
}
