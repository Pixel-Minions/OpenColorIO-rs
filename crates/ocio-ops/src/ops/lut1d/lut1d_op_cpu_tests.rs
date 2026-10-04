// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the lookup renderers' dispatch, and of the 10- and 12-bit codes past the tables
//! (docs/improvements.md, U-1). Their pixels are compared with the wheel's in
//! `tests/lut1d_op_cpu_oracle.rs`; upstream's renderer tests (`Lut1DOpCPU_tests.cpp`) render
//! float input, other output bit depths, hue adjust or inverses, which come with Phase 2.

use super::*;
use crate::ops::lut1d::lut1d_op_data::HalfFlags;

fn message(r: Result<Arc<dyn CpuOp>>) -> String {
    match r {
        Ok(op) => panic!("a renderer: {op:?}"),
        Err(e) => e.message().to_string(),
    }
}

/// The lookups exist for a forward LUT without hue adjust, of one entry per code of the input
/// (a half domain for half codes), to F32; the rest waits for Phase 2.
#[test]
fn dispatch() {
    let lut8 = Lut1DOpData::new(256).unwrap();
    let half = Lut1DOpData::with_half_flags(HalfFlags::INPUT_HALF_CODE, 65536, true).unwrap();
    for (lut, depth) in [(&lut8, BitDepth::Uint8), (&half, BitDepth::F16)] {
        get_lut1d_renderer(lut, depth, BitDepth::F32).unwrap();
    }
    for depth in [BitDepth::Uint10, BitDepth::Uint12, BitDepth::Uint16] {
        let lut = Lut1DOpData::make_lookup_domain(depth).unwrap();
        get_lut1d_renderer(&lut, depth, BitDepth::F32).unwrap();
    }

    assert_eq!(
        message(get_lut1d_renderer(&lut8, BitDepth::Uint14, BitDepth::F32)),
        "Unsupported input bit depth"
    );
    assert_eq!(
        message(get_lut1d_renderer(&lut8, BitDepth::Uint8, BitDepth::Uint32)),
        "Unsupported output bit depth"
    );
    // Float input, an inverse LUT, hue adjust.
    assert_eq!(
        message(get_lut1d_renderer(&lut8, BitDepth::F32, BitDepth::F32)),
        NOT_PORTED_F32
    );
    let inverse = lut8.inverse();
    assert_eq!(
        message(get_lut1d_renderer(&inverse, BitDepth::Uint8, BitDepth::F32)),
        NOT_PORTED_F32
    );
    let mut hue = lut8.clone();
    hue.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    assert_eq!(
        message(get_lut1d_renderer(&hue, BitDepth::Uint8, BitDepth::F32)),
        NOT_PORTED_F32
    );
    // A LUT the lookup must resample first.
    assert_eq!(
        message(get_lut1d_renderer(&lut8, BitDepth::Uint10, BitDepth::F32)),
        NOT_PORTED_COMPOSE
    );
    assert_eq!(
        message(get_lut1d_renderer(&half, BitDepth::Uint16, BitDepth::F32)),
        NOT_PORTED_COMPOSE
    );
    // Another output than F32.
    assert!(
        message(get_lut1d_renderer(&lut8, BitDepth::Uint8, BitDepth::Uint16))
            .contains("not ported yet")
    );
}

/// A 10- or 12-bit code above the maximum is an error before the lookup (docs/improvements.md,
/// U-1): upstream reads past the table. Codes up to the maximum, and alpha, pass.
#[test]
fn codes_past_the_table_are_errors() {
    for (depth, max) in [(BitDepth::Uint10, 1023u16), (BitDepth::Uint12, 4095)] {
        let lut = Lut1DOpData::make_lookup_domain(depth).unwrap();
        let renderer = get_lut1d_renderer(&lut, depth, BitDepth::F32).unwrap();
        renderer
            .check_input(Pixels::U16(&[max, 0, max, u16::MAX]))
            .unwrap();
        for c in 0..3 {
            let mut px = [0u16; 4];
            px[c] = max + 1;
            let e = renderer.check_input(Pixels::U16(&px)).unwrap_err();
            assert_eq!(
                e.message(),
                format!(
                    "Lut1D: a {} value above {max} can't be looked up: upstream reads past the \
                     1D LUT's {} entries.",
                    crate::open_color_types::bit_depth_to_string(depth),
                    max as u32 + 1
                )
            );
        }
    }
}

/// `applyRGB` and `applyRGBA` with 10- and 12-bit input read the pixel's bytes as codes, which
/// can exceed the table (docs/improvements.md, U-1): an error, the pixel left as it was. A
/// pixel whose bytes are codes within the table is looked up.
#[test]
fn in_place_codes_past_the_table_are_errors() {
    use crate::cpu_processor::CpuProcessor;
    use crate::op::OpVec;
    use crate::open_color_types::OptimizationFlags;
    use crate::ops::lut1d::lut1d_op::create_lut1d_op;

    for depth in [BitDepth::Uint10, BitDepth::Uint12] {
        let lut = Lut1DOpData::make_lookup_domain(depth).unwrap();
        let mut ops = OpVec::new();
        create_lut1d_op(&mut ops, lut, TransformDirection::Forward);
        ops.finalize().unwrap();
        let cpu = CpuProcessor::new(&ops, depth, BitDepth::F32, OptimizationFlags::NONE).unwrap();

        // 0.3f's low 16 bits, 0x999a, are a code past 1023 and 4095.
        let mut pixel = [0.3f32, 0.25, 0.125, 1.0];
        let e = cpu.apply_rgba(&mut pixel).unwrap_err();
        assert!(e.message().starts_with("Lut1D: a "), "{}", e.message());
        assert_eq!(pixel, [0.3f32, 0.25, 0.125, 1.0]);
        let mut rgb = [0.3f32, 0.25, 0.125];
        cpu.apply_rgb(&mut rgb).unwrap_err();
        assert_eq!(rgb, [0.3f32, 0.25, 0.125]);

        // Zero bytes are code 0 everywhere.
        let mut zero = [0.0f32; 4];
        cpu.apply_rgba(&mut zero).unwrap();
    }
}

/// The CPU processor of a forward 10- or 12-bit lookup domain (or, with `zero`, the same LUT
/// with every entry 0.0), to F32, without optimization.
fn lookup_processor(depth: BitDepth, zero: bool) -> crate::cpu_processor::CpuProcessor {
    use crate::cpu_processor::CpuProcessor;
    use crate::op::OpVec;
    use crate::open_color_types::OptimizationFlags;
    use crate::ops::lut1d::lut1d_op::create_lut1d_op;

    let mut lut = Lut1DOpData::make_lookup_domain(depth).unwrap();
    if zero {
        lut.get_array_mut().get_values_mut().fill(0.0);
    }
    let mut ops = OpVec::new();
    create_lut1d_op(&mut ops, lut, TransformDirection::Forward);
    ops.finalize().unwrap();
    CpuProcessor::new(&ops, depth, BitDepth::F32, OptimizationFlags::NONE).unwrap()
}

/// U-1 through the image paths (docs/improvements.md): a 1x1 10-bit image whose green code is
/// past the table, RGBA (the scanline helper's packed path) and RGB (through the packer), to
/// F32, is U-1's error, not a panic.
#[test]
fn image_codes_past_the_table_are_errors() {
    use crate::image_desc::{AUTO_STRIDE, PackedImageDesc};

    let cpu = lookup_processor(BitDepth::Uint10, false);
    for channels in [4usize, 3] {
        let mut src = [100u16, 2000, 300, 1023];
        let src = PackedImageDesc::with_strides(
            &mut src[..channels],
            1,
            1,
            channels,
            BitDepth::Uint10,
            AUTO_STRIDE,
            AUTO_STRIDE,
            AUTO_STRIDE,
        )
        .unwrap();
        let mut dst = [0.0f32; 4];
        let mut dst = PackedImageDesc::new(&mut dst[..channels], 1, 1, channels).unwrap();
        let e = cpu.apply_src_dst(&src, &mut dst).unwrap_err();
        assert_eq!(
            e.message(),
            "Lut1D: a 10ui value above 1023 can't be looked up: upstream reads past the 1D LUT's \
             1024 entries.",
            "{channels} channels"
        );
    }
}

/// In place, the first code read is red's, the pixel's first 16 bits on both wheels' orders:
/// with a LUT of zeros, the floats stored read back as code 0, so a red code of exactly the
/// maximum is looked up, and one above it is U-1's error.
#[test]
fn in_place_codes_at_the_table_boundary() {
    for (depth, max) in [(BitDepth::Uint10, 1023u32), (BitDepth::Uint12, 4095)] {
        let cpu = lookup_processor(depth, true);
        let mut at_max = [f32::from_bits(max), 0.0, 0.0, 0.0];
        cpu.apply_rgba(&mut at_max).unwrap();
        let mut rgb = [f32::from_bits(max), 0.0, 0.0];
        cpu.apply_rgb(&mut rgb).unwrap();

        let past = [f32::from_bits(max + 1), 0.0, 0.0, 0.0];
        let mut pixel = past;
        let e = cpu.apply_rgba(&mut pixel).unwrap_err();
        assert!(e.message().starts_with("Lut1D: a "), "{}", e.message());
        assert_eq!(pixel.map(f32::to_bits), past.map(f32::to_bits));
        let mut rgb = [past[0], 0.0, 0.0];
        cpu.apply_rgb(&mut rgb).unwrap_err();
    }
}

/// U-1 through the scanline helper's second packed path, where the destination isn't packed
/// RGBA F32 (src/OpenColorIO/ScanlineHelper.cpp @ v2.5.2, the `m_rgbaFloatBuffer` branch): a
/// 1x1 packed RGBA 10-bit image with green 2000, to a 16-bit image, and in place through a
/// 10-bit to 10-bit processor. Both are U-1's error.
#[test]
fn codes_past_the_table_through_the_float_buffer_are_errors() {
    use crate::cpu_processor::CpuProcessor;
    use crate::image_desc::{AUTO_STRIDE, PackedImageDesc};
    use crate::op::OpVec;
    use crate::open_color_types::OptimizationFlags;
    use crate::ops::lut1d::lut1d_op::create_lut1d_op;

    let message = "Lut1D: a 10ui value above 1023 can't be looked up: upstream reads past the 1D \
                   LUT's 1024 entries.";
    let processor = |out: BitDepth| {
        let lut = Lut1DOpData::make_lookup_domain(BitDepth::Uint10).unwrap();
        let mut ops = OpVec::new();
        create_lut1d_op(&mut ops, lut, TransformDirection::Forward);
        ops.finalize().unwrap();
        CpuProcessor::new(&ops, BitDepth::Uint10, out, OptimizationFlags::NONE).unwrap()
    };
    fn packed(px: &mut [u16], depth: BitDepth) -> PackedImageDesc<&mut [u8]> {
        PackedImageDesc::with_strides(px, 1, 1, 4, depth, AUTO_STRIDE, AUTO_STRIDE, AUTO_STRIDE)
            .unwrap()
    }

    let cpu = processor(BitDepth::Uint16);
    let mut src = [100u16, 2000, 300, 1023];
    let src = packed(&mut src[..], BitDepth::Uint10);
    let mut dst = [0u16; 4];
    let mut dst = packed(&mut dst[..], BitDepth::Uint16);
    let e = cpu.apply_src_dst(&src, &mut dst).unwrap_err();
    assert_eq!(e.message(), message);

    let cpu = processor(BitDepth::Uint10);
    let mut px = [100u16, 2000, 300, 1023];
    let mut img = packed(&mut px[..], BitDepth::Uint10);
    let e = cpu.apply(&mut img).unwrap_err();
    assert_eq!(e.message(), message);
}
