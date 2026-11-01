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

/// What a packed constructor or [`packed_layout`] gave, to compare: the getters, or the error.
fn packed_outcome(made: Result<PackedLayout>) -> std::result::Result<PackedLayout, String> {
    made.map_err(|e| e.message().to_string())
}

/// [`packed_layout`] and [`planar_layout`], the length-only cores the Python binding checks an
/// image with, agree with the constructors over raw bytes of that length on every layout and
/// error: the four packed constructors and the two planar ones, over lengths around each
/// layout's footprint (empty included), origins, channel arguments, bit depths and strides
/// (`AutoStride`, tight, padded, zero, negative, inconsistent).
#[test]
fn the_length_only_cores_agree_with_the_constructors() {
    let memory = vec![0u8; 4096];
    let bit_depths = [
        BitDepth::Uint8,
        BitDepth::Uint16,
        BitDepth::F16,
        BitDepth::F32,
        BitDepth::Uint14,
        BitDepth::Unknown,
    ];
    let chans = [AUTO_STRIDE, 0, 2, 4, 8, -4];
    let xs = [AUTO_STRIDE, 0, 12, 16, 32, -16];
    let ys = [AUTO_STRIDE, 0, 48, 64, 200, -64];
    let sizes = [(0usize, 1usize), (1, 1), (3, 2), (4, 3)];
    let lens = [0usize, 1, 47, 48, 95, 96, 191, 192, 400];
    let origins = [0usize, 1, 64, 128];
    let channel_args = [
        PackedChannels::Count(3),
        PackedChannels::Count(4),
        PackedChannels::Count(2),
        PackedChannels::Order(ChannelOrdering::Bgra),
        PackedChannels::Order(ChannelOrdering::Abgr),
        PackedChannels::Order(ChannelOrdering::Bgr),
    ];
    let (mut accepted, mut refused) = (0usize, 0usize);
    for &len in &lens {
        let bytes = &memory[..len];
        for &origin in &origins {
            for &(w, h) in &sizes {
                for channels in channel_args {
                    let mut strides: Vec<Option<(BitDepth, [isize; 3])>> = vec![None];
                    for &bd in &bit_depths {
                        for &c in &chans {
                            for &x in &xs {
                                for &y in &ys {
                                    strides.push(Some((bd, [c, x, y])));
                                }
                            }
                        }
                    }
                    for s in strides {
                        let data = At(Bytes(bytes), origin);
                        let made = match (channels, s) {
                            (PackedChannels::Count(n), None) => PackedImageDesc::new(data, w, h, n),
                            (PackedChannels::Order(o), None) => {
                                PackedImageDesc::with_channel_order(data, w, h, o)
                            }
                            (PackedChannels::Count(n), Some((bd, [c, x, y]))) => {
                                PackedImageDesc::with_strides(data, w, h, n, bd, c, x, y)
                            }
                            (PackedChannels::Order(o), Some((bd, [c, x, y]))) => {
                                PackedImageDesc::with_channel_order_and_strides(
                                    data, w, h, o, bd, c, x, y,
                                )
                            }
                        }
                        .map(|d| PackedLayout {
                            layout: *d.layout(),
                            chan_order: d.channel_order(),
                            num_channels: d.num_channels(),
                            chan_stride_bytes: d.chan_stride_bytes(),
                        });
                        let core = packed_layout(len, origin, w, h, channels, s);
                        match &made {
                            Ok(_) => accepted += 1,
                            Err(_) => refused += 1,
                        }
                        assert_eq!(
                            packed_outcome(core),
                            packed_outcome(made),
                            "len {len} origin {origin} {w}x{h} {channels:?} {s:?}"
                        );
                    }
                }
            }
        }
    }
    assert!(accepted > 1000 && refused > 1000, "{accepted} {refused}");

    let (mut accepted, mut refused) = (0usize, 0usize);
    let plane_lens = [0usize, 1, 23, 24, 48, 96];
    for &lr in &plane_lens {
        for &lg in &plane_lens {
            for &la in &[None, Some(0usize), Some(24), Some(96)] {
                for &origin in &[0usize, 4, 24] {
                    for &(w, h) in &sizes {
                        let mut strides: Vec<Option<(BitDepth, [isize; 2])>> = vec![None];
                        for &bd in &bit_depths {
                            for &x in &xs {
                                for &y in &ys {
                                    strides.push(Some((bd, [x, y])));
                                }
                            }
                        }
                        for s in strides {
                            let plane = |len: usize| At(Bytes(&memory[..len]), origin);
                            let (r, g, b) = (plane(lr), plane(lg), plane(lr));
                            let a = la.map(plane);
                            let made = match s {
                                None => PlanarImageDesc::new(r, g, b, a, w, h),
                                Some((bd, [x, y])) => {
                                    PlanarImageDesc::with_strides(r, g, b, a, w, h, bd, x, y)
                                }
                            }
                            .map(|d| *d.layout());
                            let core = planar_layout(
                                (lr, origin),
                                (lg, origin),
                                (lr, origin),
                                la.map(|l| (l, origin)),
                                w,
                                h,
                                s,
                            );
                            match &made {
                                Ok(_) => accepted += 1,
                                Err(_) => refused += 1,
                            }
                            assert_eq!(
                                core.map_err(|e| e.message().to_string()),
                                made.map_err(|e| e.message().to_string()),
                                "planes {lr} {lg} {la:?} origin {origin} {w}x{h} {s:?}"
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(accepted > 100 && refused > 1000, "{accepted} {refused}");
}

/// The rows, in order, in as many runs as parts, or rows.
#[test]
fn row_runs_share_the_rows_out() {
    assert_eq!(row_runs(10, 3), [(0, 4), (4, 3), (7, 3)]);
    assert_eq!(row_runs(4, 2), [(0, 2), (2, 2)]);
    assert_eq!(row_runs(2, 5), [(0, 1), (1, 1)]);
    assert_eq!(row_runs(7, 1), [(0, 7)]);
    assert_eq!(row_runs(7, 0), [(0, 7)]);
}

/// Runs of rows of an image are images of their own, over the bytes of those rows.
#[test]
fn split_rows_mut_gives_each_run_its_rows() {
    // Two rows, then one, of three pixels, each with 8 bytes of padding.
    let mut bytes = [0u8; 3 * 40 - 8];
    let mut img = PackedImageDesc::with_strides(
        Bytes(&mut bytes[..]),
        3,
        3,
        4,
        BitDepth::Uint8,
        AUTO_STRIDE,
        AUTO_STRIDE,
        40,
    )
    .unwrap();
    let layout = *img.layout();
    let runs = row_runs(3, 2);
    let mut parts = split_rows_mut(&layout, buffers_mut(&mut img), &runs).unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].layout().height(), 2);
    assert_eq!(parts[1].layout().height(), 1);
    assert_eq!(buffers_mut(&mut parts[0])[0].len(), 80);
    assert_eq!(buffers_mut(&mut parts[1])[0].len(), 32);
    assert_eq!(parts[1].layout().r_data().offset, 0);
    assert_eq!(parts[1].layout().y_stride_bytes(), 40);
    buffers_mut(&mut parts[1])[0][0] = 7;
    drop(parts);
    assert_eq!(bytes[80], 7);
}

/// Planes keep their offsets from the first byte of their rows.
#[test]
fn split_rows_mut_cuts_each_plane() {
    let mut planes = [[0.0f32; 6]; 3];
    let [r, g, b] = &mut planes;
    let mut img = PlanarImageDesc::new(&mut r[..], &mut g[..], &mut b[..], None, 2, 3).unwrap();
    let layout = *img.layout();
    let runs = row_runs(3, 3);
    let mut parts = split_rows_mut(&layout, buffers_mut(&mut img), &runs).unwrap();
    assert_eq!(parts.len(), 3);
    for part in &mut parts {
        let buffers = buffers_mut(part);
        assert_eq!(buffers.len(), 3);
        assert!(buffers.iter().all(|b| b.len() == 8));
    }
}

/// Rows that don't come one after the other in a buffer can't be parted by cutting it.
#[test]
fn split_rows_mut_refuses_rows_that_cuts_cannot_part() {
    let row = 3 * 4 * 4;
    let mut bottom_up = vec![0u8; 2 * row];
    let mut img = PackedImageDesc::with_strides(
        Bytes(&mut bottom_up[..]),
        3,
        2,
        4,
        BitDepth::F32,
        AUTO_STRIDE,
        AUTO_STRIDE,
        AUTO_STRIDE,
    )
    .unwrap();
    let up = ImageLayout {
        y_stride_bytes: -(row as isize),
        ..*img.layout()
    };
    assert!(split_rows_mut(&up, buffers_mut(&mut img), &row_runs(2, 2)).is_none());

    let tight = *img.layout();
    let overlapping = ImageLayout {
        y_stride_bytes: row as isize - 1,
        ..tight
    };
    assert!(split_rows_mut(&overlapping, buffers_mut(&mut img), &row_runs(2, 2)).is_none());
    assert!(split_rows_mut(&tight, buffers_mut(&mut img), &row_runs(1, 2)).is_some());
    assert!(tight.row_starts(2).is_none(), "a buffer without channels");
}

/// Images whose `width * height` wraps in a `long` keep the serial apply, whose scanlines then
/// count as outside the image (I-1): the layout alone decides, as an image that size takes 6 GiB.
/// `long` has 32 bits on Windows and 64 on Linux.
#[test]
fn row_starts_refuses_the_images_of_i_1() {
    let mut pixels = [0.0f32; 4];
    let img = PackedImageDesc::new(&mut pixels[..], 1, 1, 4).unwrap();
    let sized = |width: c_long, height: c_long| ImageLayout {
        width,
        height,
        x_stride_bytes: 16,
        y_stride_bytes: 16 * width as isize,
        ..*img.layout()
    };
    assert!(sized(65535, 32768).row_starts(1).is_some());
    assert!(sized(65536, 32767).row_starts(1).is_some());
    assert_eq!(sized(65536, 32768).row_starts(1).is_none(), cfg!(windows));
}
