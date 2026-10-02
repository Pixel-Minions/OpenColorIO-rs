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
