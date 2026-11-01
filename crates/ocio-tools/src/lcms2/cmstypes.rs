// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/cmstypes.c` @ lcms2.17: the tag types a display profile's reachable tags can
//! hold, the types the tool writes, and the table of supported tags.
//!
//! The tags that are read: `A2B0`, `A2B1`, `B2A0`, `B2A1`, `D2B0`, `D2B1`, `B2D0`, `B2D1`, the
//! colorants and curves of a matrix-shaper, `wtpt`, and `clrt` and `clot`. So the readers are of
//! `XYZ `, `curv`, `para`, `mft1`, `mft2`, `mAB `, `mBA `, `mpet` and `clrt`. The tags that are
//! written: `desc`, `cprt`, `wtpt`, `chad`, `A2B0`, `B2A0`, in a v4.2 profile. So the writers are
//! of `mluc`, `XYZ `, `sf32`, `mAB ` and `mBA `, and `para` inside those. The rest of the file
//! isn't reached, but for the table of tags: any tag of a profile can reach it, through the
//! links between tags that `_cmsReadHeader` looks for.

use super::cmserr::{
    cms_calloc_fits, cms_malloc_fits, cms_signal_error, cms_tag_signature_2_string,
};
use super::cmsgamma::{
    CurveSegment, MINUS_INF, PLUS_INF, ToneCurve, cms_build_parametric_tone_curve,
    cms_build_segmented_tone_curve, cms_build_tabulated_tone_curve16, cms_eval_tone_curve_float,
};
use super::cmsio0::{IoHandler, TagData};
use super::cmslut::*;
use super::cmsmtrx::{Mat3, cms_mat3_is_identity};
use super::cmsnamed::Mlu;
use super::cmsplugin::*;
use super::types::*;

/// `cmsTagDescriptor`, without what is reached through `DecideType`: [`decide_type`] does that.
#[derive(Debug, Clone, Copy)]
pub struct TagDescriptor {
    pub elem_count: u32,
    pub supported_types: &'static [u32],
}

/// Port of `ReadPositionTable` (src/cmstypes.c:218-269 @ lcms2.17). `element_fn` is the reader callback,
/// with its cargo.
fn read_position_table(
    io: &mut IoHandler,
    count: u32,
    base_offset: u32,
    mut element_fn: impl FnMut(&mut IoHandler) -> bool,
) -> bool {
    let current_position = io.tell();

    // Verify there is enough space left to read at least two cmsUInt32Number items for Count
    // items.
    if (io.reported_size.wrapping_sub(current_position) / 8) < count {
        return false;
    }

    // Let's take the offsets to each element
    if !cms_calloc_fits(count, 4) {
        return false;
    }
    let mut element_offsets = vec![0u32; count as usize];

    for offset in &mut element_offsets {
        let Some(read) = cms_read_u32(io) else {
            return false;
        };
        if cms_read_u32(io).is_none() {
            return false;
        }
        *offset = read.wrapping_add(base_offset);
    }

    // Seek to each element and read it
    for offset in element_offsets {
        if !io.seek(offset) {
            return false;
        }

        // This is the reader callback
        if !element_fn(io) {
            return false;
        }
    }

    // Success
    true
}

/// Port of `Type_XYZ_Read` (src/cmstypes.c:346-364 @ lcms2.17).
fn type_xyz_read(io: &mut IoHandler) -> Option<(TagData, u32)> {
    Some((TagData::Xyz(cms_read_xyz(io)?), 1))
}

/// Port of `Type_S15Fixed16_Write` (src/cmstypes.c:775-789 @ lcms2.17).
fn type_s15_fixed16_write(io: &mut IoHandler, value: &[f64], n_items: u32) -> bool {
    for number in &value[..n_items as usize] {
        if !cms_write_15_fixed16(io, *number) {
            return false;
        }
    }
    true
}

/// Port of `Type_Curve_Read` (src/cmstypes.c:1330-1381 @ lcms2.17).
fn type_curve_read(io: &mut IoHandler) -> Option<ToneCurve> {
    let count = cms_read_u32(io)?;

    match count {
        // Linear.
        0 => cms_build_parametric_tone_curve(1, &[1.0]),

        // Specified as the exponent of gamma function
        1 => {
            let single_gamma_fixed = cms_read_u16(io)?;
            let single_gamma = cms_8_fixed8_to_double(single_gamma_fixed);
            cms_build_parametric_tone_curve(1, &[single_gamma])
        }

        // Curve
        _ => {
            if count > 0x7FFF {
                return None; // This is to prevent bad guys for doing bad things
            }

            let mut new_gamma = cms_build_tabulated_tone_curve16(count, None)?;

            if !cms_read_u16_array(io, &mut new_gamma.table16) {
                return None;
            }

            Some(new_gamma)
        }
    }
}

/// Port of `Type_ParametricCurve_Read` (src/cmstypes.c:1448-1480 @ lcms2.17).
fn type_parametric_curve_read(io: &mut IoHandler) -> Option<ToneCurve> {
    const PARAMS_BY_TYPE: [usize; 5] = [1, 3, 4, 5, 7];

    let type_ = cms_read_u16(io)?;
    cms_read_u16(io)?; // Reserved

    if type_ > 4 {
        let text = format!("Unknown parametric curve type '{type_}'");
        cms_signal_error(CMS_ERROR_UNKNOWN_EXTENSION, text.as_bytes());
        return None;
    }

    let mut params = [0f64; 10];
    let n = PARAMS_BY_TYPE[type_ as usize];

    for param in &mut params[..n] {
        *param = cms_read_15_fixed16(io)?;
    }

    cms_build_parametric_tone_curve(type_ as i32 + 1, &params)
}

/// Port of `Type_ParametricCurve_Write` (src/cmstypes.c:1483-1516 @ lcms2.17), for the curves the tool's
/// pipelines hold: `cmsBuildGamma`'s, of type 1.
fn type_parametric_curve_write(io: &mut IoHandler, curve: &ToneCurve) -> bool {
    const PARAMS_BY_TYPE: [usize; 6] = [0, 1, 3, 4, 5, 7];

    let typen = curve.segments[0].type_;
    let n_params = PARAMS_BY_TYPE[typen as usize];

    if !cms_write_u16(io, (curve.segments[0].type_ - 1) as u16) {
        return false;
    }
    if !cms_write_u16(io, 0) {
        return false; // Reserved
    }

    for param in &curve.segments[0].params[..n_params] {
        if !cms_write_15_fixed16(io, *param) {
            return false;
        }
    }

    true
}

/// Port of `Type_MLU_Write` (src/cmstypes.c:1771-1812 @ lcms2.17).
fn type_mlu_write(io: &mut IoHandler, mlu: &Mlu) -> bool {
    let used_entries = mlu.entries.len() as u32;

    if !cms_write_u32(io, used_entries) {
        return false;
    }
    if !cms_write_u32(io, 12) {
        return false;
    }

    let header_size = 12 * used_entries + 8;

    for entry in &mlu.entries {
        let len = entry.len * 2;
        let offset = entry.str_w * 2 + header_size + 8;

        if !cms_write_u16(io, entry.language) {
            return false;
        }
        if !cms_write_u16(io, entry.country) {
            return false;
        }
        if !cms_write_u32(io, len) {
            return false;
        }
        if !cms_write_u32(io, offset) {
            return false;
        }
    }

    // _cmsWriteWCharArray (src/cmstypes.c:123-134 @ lcms2.17)
    for unit in &mlu.pool {
        if !cms_write_u16(io, *unit) {
            return false;
        }
    }

    true
}

/// Port of `Read8bitTables` (src/cmstypes.c:1886-1932 @ lcms2.17).
fn read_8bit_tables(io: &mut IoHandler, lut: &mut Pipeline, n_channels: u32) -> bool {
    if n_channels > CMS_MAX_CHANNELS as u32 {
        return false;
    }
    if n_channels == 0 {
        return false;
    }

    let mut tables = Vec::with_capacity(n_channels as usize);
    for _ in 0..n_channels {
        match cms_build_tabulated_tone_curve16(256, None) {
            Some(table) => tables.push(table),
            None => return false,
        }
    }

    let mut temp = [0u8; 256];
    for table in &mut tables {
        if io.read(&mut temp, 256, 1) != 1 {
            return false;
        }

        for j in 0..256 {
            table.table16[j] = from_8_to_16(temp[j]);
        }
    }

    cms_pipeline_insert_stage(
        lut,
        StageLoc::AtEnd,
        cms_stage_alloc_tone_curves(n_channels, Some(&tables)),
    )
}

/// Port of `uipow` (src/cmstypes.c:1974-1995 @ lcms2.17).
fn uipow(n: u32, a: u32, b: u32) -> u32 {
    let mut rv = 1u32;

    if a == 0 {
        return 0;
    }
    if n == 0 {
        return 0;
    }

    for _ in 0..b {
        rv = rv.wrapping_mul(a);

        // Check for overflow
        if rv > u32::MAX / a {
            return u32::MAX;
        }
    }

    let rc = rv.wrapping_mul(n);

    if rv != rc / n {
        return u32::MAX;
    }
    rc
}

/// The nine `_cmsRead15Fixed16Number` of `Type_LUT8_Read` and `Type_LUT16_Read`.
fn read_lut_matrix(io: &mut IoHandler) -> Option<[f64; 9]> {
    let mut matrix = [0f64; 9];
    for item in &mut matrix {
        *item = cms_read_15_fixed16(io)?;
    }
    Some(matrix)
}

/// `(cmsMAT3*) Matrix` of `Type_LUT8_Read` and `Type_LUT16_Read`.
fn lut_matrix_is_identity(matrix: &[f64; 9]) -> bool {
    let mat: Mat3 = [
        [matrix[0], matrix[1], matrix[2]],
        [matrix[3], matrix[4], matrix[5]],
        [matrix[6], matrix[7], matrix[8]],
    ];
    cms_mat3_is_identity(&mat)
}

/// Port of `Type_LUT8_Read` (src/cmstypes.c:2002-2100 @ lcms2.17).
fn type_lut8_read(io: &mut IoHandler) -> Option<Pipeline> {
    let input_channels = cms_read_u8(io)?;
    let output_channels = cms_read_u8(io)?;
    let clut_points = cms_read_u8(io)?;

    if clut_points == 1 {
        return None; // Impossible value, 0 for no CLUT and then 2 at least
    }

    // Padding
    cms_read_u8(io)?;

    // Do some checking
    if input_channels == 0 || input_channels as usize > CMS_MAX_CHANNELS {
        return None;
    }
    if output_channels == 0 || output_channels as usize > CMS_MAX_CHANNELS {
        return None;
    }

    // Allocates an empty Pipeline
    let mut new_lut = cms_pipeline_alloc(input_channels as u32, output_channels as u32)?;

    // Read the Matrix
    let matrix = read_lut_matrix(io)?;

    // Only operates if not identity...
    if input_channels == 3
        && !lut_matrix_is_identity(&matrix)
        && !cms_pipeline_insert_stage(
            &mut new_lut,
            StageLoc::AtBegin,
            cms_stage_alloc_matrix(3, 3, &matrix, None),
        )
    {
        return None;
    }

    // Get input tables
    if !read_8bit_tables(io, &mut new_lut, input_channels as u32) {
        return None;
    }

    // Get 3D CLUT. Check the overflow....
    let n_tab_size = uipow(
        output_channels as u32,
        clut_points as u32,
        input_channels as u32,
    );
    if n_tab_size == u32::MAX {
        return None;
    }
    if n_tab_size > 0 {
        if !cms_calloc_fits(n_tab_size, 2) || !cms_malloc_fits(n_tab_size) {
            return None;
        }

        let mut temp = vec![0u8; n_tab_size as usize];
        if io.read(&mut temp, n_tab_size, 1) != 1 {
            return None;
        }

        let t: Vec<u16> = temp.iter().map(|&v| from_8_to_16(v)).collect();
        drop(temp);

        if !cms_pipeline_insert_stage(
            &mut new_lut,
            StageLoc::AtEnd,
            cms_stage_alloc_clut16bit(
                clut_points as u32,
                input_channels as u32,
                output_channels as u32,
                Some(&t),
            ),
        ) {
            return None;
        }
    }

    // Get output tables
    if !read_8bit_tables(io, &mut new_lut, output_channels as u32) {
        return None;
    }

    Some(new_lut)
}

/// Port of `Read16bitTables` (src/cmstypes.c:2239-2280 @ lcms2.17).
fn read_16bit_tables(
    io: &mut IoHandler,
    lut: &mut Pipeline,
    n_channels: u32,
    n_entries: u32,
) -> bool {
    // Maybe an empty table? (this is a lcms extension)
    if n_entries == 0 {
        return true;
    }

    // Check for malicious profiles
    if n_entries < 2 {
        return false;
    }
    if n_channels > CMS_MAX_CHANNELS as u32 {
        return false;
    }

    let mut tables = Vec::with_capacity(n_channels as usize);
    for _ in 0..n_channels {
        let Some(mut table) = cms_build_tabulated_tone_curve16(n_entries, None) else {
            return false;
        };

        if !cms_read_u16_array(io, &mut table.table16) {
            return false;
        }
        tables.push(table);
    }

    // Add the table (which may certainly be an identity, but this is up to the optimizer, not
    // the reading code)
    cms_pipeline_insert_stage(
        lut,
        StageLoc::AtEnd,
        cms_stage_alloc_tone_curves(n_channels, Some(&tables)),
    )
}

/// Port of `Type_LUT16_Read` (src/cmstypes.c:2307-2395 @ lcms2.17).
fn type_lut16_read(io: &mut IoHandler) -> Option<Pipeline> {
    let input_channels = cms_read_u8(io)?;
    let output_channels = cms_read_u8(io)?;
    let clut_points = cms_read_u8(io)?; // 255 maximum

    // Padding
    cms_read_u8(io)?;

    // Do some checking
    if input_channels == 0 || input_channels as usize > CMS_MAX_CHANNELS {
        return None;
    }
    if output_channels == 0 || output_channels as usize > CMS_MAX_CHANNELS {
        return None;
    }

    // Allocates an empty LUT
    let mut new_lut = cms_pipeline_alloc(input_channels as u32, output_channels as u32)?;

    // Read the Matrix
    let matrix = read_lut_matrix(io)?;

    // Only operates on 3 channels
    if input_channels == 3
        && !lut_matrix_is_identity(&matrix)
        && !cms_pipeline_insert_stage(
            &mut new_lut,
            StageLoc::AtEnd,
            cms_stage_alloc_matrix(3, 3, &matrix, None),
        )
    {
        return None;
    }

    let input_entries = cms_read_u16(io)?;
    let output_entries = cms_read_u16(io)?;

    if input_entries > 0x7FFF || output_entries > 0x7FFF {
        return None;
    }
    if clut_points == 1 {
        return None; // Impossible value, 0 for no CLUT and then 2 at least
    }

    // Get input tables
    if !read_16bit_tables(
        io,
        &mut new_lut,
        input_channels as u32,
        input_entries as u32,
    ) {
        return None;
    }

    // Get 3D CLUT
    let n_tab_size = uipow(
        output_channels as u32,
        clut_points as u32,
        input_channels as u32,
    );
    if n_tab_size == u32::MAX {
        return None;
    }
    if n_tab_size > 0 {
        if !cms_calloc_fits(n_tab_size, 2) {
            return None;
        }
        let mut t = vec![0u16; n_tab_size as usize];

        if !cms_read_u16_array(io, &mut t) {
            return None;
        }

        if !cms_pipeline_insert_stage(
            &mut new_lut,
            StageLoc::AtEnd,
            cms_stage_alloc_clut16bit(
                clut_points as u32,
                input_channels as u32,
                output_channels as u32,
                Some(&t),
            ),
        ) {
            return None;
        }
    }

    // Get output tables
    if !read_16bit_tables(
        io,
        &mut new_lut,
        output_channels as u32,
        output_entries as u32,
    ) {
        return None;
    }

    Some(new_lut)
}

/// Port of `ReadMatrix` (src/cmstypes.c:2563-2591 @ lcms2.17).
fn read_matrix(io: &mut IoHandler, offset: u32) -> Option<Stage> {
    // Go to address
    if !io.seek(offset) {
        return None;
    }

    // Read the Matrix
    let mut d_mat = [0f64; 9];
    for item in &mut d_mat {
        *item = cms_read_15_fixed16(io)?;
    }

    let mut d_off = [0f64; 3];
    for item in &mut d_off {
        *item = cms_read_15_fixed16(io)?;
    }

    cms_stage_alloc_matrix(3, 3, &d_mat, Some(&d_off))
}

/// Port of `ReadCLUT` (src/cmstypes.c:2598-2659 @ lcms2.17).
fn read_clut(
    io: &mut IoHandler,
    offset: u32,
    input_channels: u32,
    output_channels: u32,
) -> Option<Stage> {
    if !io.seek(offset) {
        return None;
    }

    let mut grid_points8 = [0u8; CMS_MAX_CHANNELS]; // Number of grid points in each dimension.
    if io.read(&mut grid_points8, CMS_MAX_CHANNELS as u32, 1) != 1 {
        return None;
    }

    let mut grid_points = [0u32; CMS_MAX_CHANNELS];
    for i in 0..CMS_MAX_CHANNELS {
        if grid_points8[i] == 1 {
            return None; // Impossible value, 0 for no CLUT and then 2 at least
        }
        grid_points[i] = grid_points8[i] as u32;
    }

    let precision = cms_read_u8(io)?;
    cms_read_u8(io)?;
    cms_read_u8(io)?;
    cms_read_u8(io)?;

    let mut clut =
        cms_stage_alloc_clut16bit_granular(&grid_points, input_channels, output_channels, None)?;

    let StageData::Clut(StageClutData {
        tab: ClutTable::T(table),
        ..
    }) = &mut clut.data
    else {
        return None;
    };

    // Precision can be 1 or 2 bytes
    if precision == 1 {
        for item in table.iter_mut() {
            *item = from_8_to_16(cms_read_u8(io)?);
        }
    } else if precision == 2 {
        if !cms_read_u16_array(io, table) {
            return None;
        }
    } else {
        let text = format!("Unknown precision of '{precision}'");
        cms_signal_error(CMS_ERROR_UNKNOWN_EXTENSION, text.as_bytes());
        return None;
    }

    Some(clut)
}

/// Port of `ReadEmbeddedCurve` (src/cmstypes.c:2661-2685 @ lcms2.17).
fn read_embedded_curve(io: &mut IoHandler) -> Option<ToneCurve> {
    let base_type = cms_read_type_base(io);
    match base_type {
        SIG_CURVE_TYPE => type_curve_read(io),
        SIG_PARAMETRIC_CURVE_TYPE => type_parametric_curve_read(io),
        _ => {
            let mut text = b"Unknown curve type '".to_vec();
            text.extend_from_slice(&cms_tag_signature_2_string(base_type));
            text.push(b'\'');
            cms_signal_error(CMS_ERROR_UNKNOWN_EXTENSION, &text);
            None
        }
    }
}

/// Port of `ReadSetOfCurves` (src/cmstypes.c:2689-2718 @ lcms2.17).
fn read_set_of_curves(io: &mut IoHandler, offset: u32, n_curves: u32) -> Option<Stage> {
    if n_curves > CMS_MAX_CHANNELS as u32 {
        return None;
    }

    if !io.seek(offset) {
        return None;
    }

    let mut curves = Vec::with_capacity(n_curves as usize);
    for _ in 0..n_curves {
        curves.push(read_embedded_curve(io)?);

        if !cms_read_alignment(io) {
            return None;
        }
    }

    cms_stage_alloc_tone_curves(n_curves, Some(&curves))
}

/// One of the five reads of `Type_LUTA2B_Read` and `Type_LUTB2A_Read`: `false` is their
/// `goto Error`.
fn insert_read(lut: &mut Pipeline, stage: Option<Stage>) -> bool {
    cms_pipeline_insert_stage(lut, StageLoc::AtEnd, stage)
}

/// Port of `Type_LUTA2B_Read` (src/cmstypes.c:2742-2808 @ lcms2.17).
fn type_luta2b_read(io: &mut IoHandler) -> Option<Pipeline> {
    let base_offset = io.tell().wrapping_sub(8);

    let input_chan = cms_read_u8(io)?;
    let output_chan = cms_read_u8(io)?;

    cms_read_u16(io)?;

    let offset_b = cms_read_u32(io)?;
    let offset_mat = cms_read_u32(io)?;
    let offset_m = cms_read_u32(io)?;
    let offset_c = cms_read_u32(io)?;
    let offset_a = cms_read_u32(io)?;

    if input_chan == 0 || input_chan as usize >= CMS_MAX_CHANNELS {
        return None;
    }
    if output_chan == 0 || output_chan as usize >= CMS_MAX_CHANNELS {
        return None;
    }

    let (input_chan, output_chan) = (input_chan as u32, output_chan as u32);

    // Allocates an empty LUT
    let mut new_lut = cms_pipeline_alloc(input_chan, output_chan)?;

    let at = |offset: u32| base_offset.wrapping_add(offset);

    if offset_a != 0
        && !insert_read(
            &mut new_lut,
            read_set_of_curves(io, at(offset_a), input_chan),
        )
    {
        return None;
    }

    if offset_c != 0
        && !insert_read(
            &mut new_lut,
            read_clut(io, at(offset_c), input_chan, output_chan),
        )
    {
        return None;
    }

    if offset_m != 0
        && !insert_read(
            &mut new_lut,
            read_set_of_curves(io, at(offset_m), output_chan),
        )
    {
        return None;
    }

    if offset_mat != 0 && !insert_read(&mut new_lut, read_matrix(io, at(offset_mat))) {
        return None;
    }

    if offset_b != 0
        && !insert_read(
            &mut new_lut,
            read_set_of_curves(io, at(offset_b), output_chan),
        )
    {
        return None;
    }

    Some(new_lut)
}

/// Port of `WriteMatrix` (src/cmstypes.c:2811-2844 @ lcms2.17).
fn write_matrix(io: &mut IoHandler, mpe: &Stage) -> bool {
    let StageData::Matrix { double, offset } = &mpe.data else {
        return false;
    };

    let n = (mpe.input_channels * mpe.output_channels) as usize;

    // Write the Matrix
    for value in &double[..n] {
        if !cms_write_15_fixed16(io, *value) {
            return false;
        }
    }

    for i in 0..mpe.output_channels as usize {
        let value = offset.as_ref().map_or(0.0, |offset| offset[i]);
        if !cms_write_15_fixed16(io, value) {
            return false;
        }
    }

    true
}

/// Port of `WriteSetOfCurves` (src/cmstypes.c:2848-2898 @ lcms2.17), for `cmsBuildGamma`'s curves, which
/// are always saved as parametric.
fn write_set_of_curves(io: &mut IoHandler, mpe: &Stage) -> bool {
    let StageData::Curves(curves) = &mpe.data else {
        return false;
    };

    for curve in &curves[..mpe.output_channels as usize] {
        if !cms_write_type_base(io, SIG_PARAMETRIC_CURVE_TYPE) {
            return false;
        }

        if !type_parametric_curve_write(io, curve) {
            return false;
        }

        if !cms_write_alignment(io) {
            return false;
        }
    }

    true
}

/// Port of `WriteCLUT` (src/cmstypes.c:2901-2945 @ lcms2.17), for a 16-bit table saved with precision 2.
/// `Lut->SaveAs8Bits` isn't set on the tool's pipelines.
fn write_clut(io: &mut IoHandler, mpe: &Stage) -> bool {
    let StageData::Clut(StageClutData {
        tab: ClutTable::T(table),
        params,
    }) = &mpe.data
    else {
        return false;
    };

    // Number of grid points in each dimension.
    let mut grid_points = [0u8; CMS_MAX_CHANNELS];

    for i in 0..params.n_inputs as usize {
        grid_points[i] = params.n_samples[i] as u8;
    }

    if !io.write(&grid_points) {
        return false;
    }

    if !cms_write_u8(io, 2) {
        return false;
    }
    if !cms_write_u8(io, 0) {
        return false;
    }
    if !cms_write_u8(io, 0) {
        return false;
    }
    if !cms_write_u8(io, 0) {
        return false;
    }

    // _cmsWriteUInt16Array, all at once.
    let bytes: Vec<u8> = table.iter().flat_map(|value| value.to_be_bytes()).collect();
    if !io.write(&bytes) {
        return false;
    }

    cms_write_alignment(io)
}

/// What `Type_LUTA2B_Write` and `Type_LUTB2A_Write` (src/cmstypes.c:3138-3219 @ lcms2.17) share: the
/// stages by role, already found, and the writing of them.
fn write_lut_v4(
    io: &mut IoHandler,
    lut: &Pipeline,
    base_offset: u32,
    a: &Stage,
    clut: &Stage,
    m: &Stage,
    matrix: &Stage,
    b: &Stage,
) -> bool {
    // Get input, output channels
    let input_chan = lut.input_channels;
    let output_chan = lut.output_channels;

    // Write channel count
    if !cms_write_u8(io, input_chan as u8) {
        return false;
    }
    if !cms_write_u8(io, output_chan as u8) {
        return false;
    }
    if !cms_write_u16(io, 0) {
        return false;
    }

    // Keep directory to be filled latter
    let directory_pos = io.tell();

    // Write the directory
    for _ in 0..5 {
        if !cms_write_u32(io, 0) {
            return false;
        }
    }

    let offset_a = io.tell().wrapping_sub(base_offset);
    if !write_set_of_curves(io, a) {
        return false;
    }

    let offset_c = io.tell().wrapping_sub(base_offset);
    if !write_clut(io, clut) {
        return false;
    }

    let offset_m = io.tell().wrapping_sub(base_offset);
    if !write_set_of_curves(io, m) {
        return false;
    }

    let offset_mat = io.tell().wrapping_sub(base_offset);
    if !write_matrix(io, matrix) {
        return false;
    }

    let offset_b = io.tell().wrapping_sub(base_offset);
    if !write_set_of_curves(io, b) {
        return false;
    }

    let current_pos = io.tell();

    if !io.seek(directory_pos) {
        return false;
    }

    for offset in [offset_b, offset_mat, offset_m, offset_c, offset_a] {
        if !cms_write_u32(io, offset) {
            return false;
        }
    }

    io.seek(current_pos)
}

/// Whether the stages of `lut` are of these types: `cmsPipelineCheckAndRetreiveStages` (src/cmslut.c:110-150 @ lcms2.17).
fn check_stages(lut: &Pipeline, types: &[u32]) -> bool {
    lut.elements.len() == types.len()
        && lut
            .elements
            .iter()
            .zip(types)
            .all(|(mpe, type_)| mpe.type_ == *type_)
}

/// Port of `Type_LUTA2B_Write` (src/cmstypes.c:2950-3037 @ lcms2.17). The tool's `A2B0` has the five
/// stages, or four if its CLUT couldn't be allocated. The structures with fewer aren't reached.
fn type_luta2b_write(io: &mut IoHandler, lut: &Pipeline) -> bool {
    // Get the base for all offsets
    let base_offset = io.tell().wrapping_sub(8);

    let types = [
        SIG_CURVE_SET_ELEM_TYPE,
        SIG_C_LUT_ELEM_TYPE,
        SIG_CURVE_SET_ELEM_TYPE,
        SIG_MATRIX_ELEM_TYPE,
        SIG_CURVE_SET_ELEM_TYPE,
    ];
    if !check_stages(lut, &types) {
        cms_signal_error(
            CMS_ERROR_NOT_SUITABLE,
            b"LUT is not suitable to be saved as LutAToB",
        );
        return false;
    }

    let e = &lut.elements;
    write_lut_v4(io, lut, base_offset, &e[0], &e[1], &e[2], &e[3], &e[4])
}

/// Port of `Type_LUTB2A_Read` (src/cmstypes.c:3061-3128 @ lcms2.17).
fn type_lutb2a_read(io: &mut IoHandler) -> Option<Pipeline> {
    let base_offset = io.tell().wrapping_sub(8);

    let input_chan = cms_read_u8(io)?;
    let output_chan = cms_read_u8(io)?;

    if input_chan == 0 || input_chan as usize >= CMS_MAX_CHANNELS {
        return None;
    }
    if output_chan == 0 || output_chan as usize >= CMS_MAX_CHANNELS {
        return None;
    }

    // Padding
    cms_read_u16(io)?;

    let offset_b = cms_read_u32(io)?;
    let offset_mat = cms_read_u32(io)?;
    let offset_m = cms_read_u32(io)?;
    let offset_c = cms_read_u32(io)?;
    let offset_a = cms_read_u32(io)?;

    let (input_chan, output_chan) = (input_chan as u32, output_chan as u32);

    // Allocates an empty LUT
    let mut new_lut = cms_pipeline_alloc(input_chan, output_chan)?;

    let at = |offset: u32| base_offset.wrapping_add(offset);

    if offset_b != 0
        && !insert_read(
            &mut new_lut,
            read_set_of_curves(io, at(offset_b), input_chan),
        )
    {
        return None;
    }

    if offset_mat != 0 && !insert_read(&mut new_lut, read_matrix(io, at(offset_mat))) {
        return None;
    }

    if offset_m != 0
        && !insert_read(
            &mut new_lut,
            read_set_of_curves(io, at(offset_m), input_chan),
        )
    {
        return None;
    }

    if offset_c != 0
        && !insert_read(
            &mut new_lut,
            read_clut(io, at(offset_c), input_chan, output_chan),
        )
    {
        return None;
    }

    if offset_a != 0
        && !insert_read(
            &mut new_lut,
            read_set_of_curves(io, at(offset_a), output_chan),
        )
    {
        return None;
    }

    Some(new_lut)
}

/// Port of `Type_LUTB2A_Write` (src/cmstypes.c:3138-3219 @ lcms2.17). As [`type_luta2b_write`].
fn type_lutb2a_write(io: &mut IoHandler, lut: &Pipeline) -> bool {
    let base_offset = io.tell().wrapping_sub(8);

    let types = [
        SIG_CURVE_SET_ELEM_TYPE,
        SIG_MATRIX_ELEM_TYPE,
        SIG_CURVE_SET_ELEM_TYPE,
        SIG_C_LUT_ELEM_TYPE,
        SIG_CURVE_SET_ELEM_TYPE,
    ];
    if !check_stages(lut, &types) {
        cms_signal_error(
            CMS_ERROR_NOT_SUITABLE,
            b"LUT is not suitable to be saved as LutBToA",
        );
        return false;
    }

    let e = &lut.elements;
    write_lut_v4(io, lut, base_offset, &e[4], &e[3], &e[2], &e[1], &e[0])
}

/// Port of `Type_ColorantTable_Read` (src/cmstypes.c:3254-3294 @ lcms2.17). The list isn't kept, as
/// nothing reads it; appending to it can't fail.
fn type_colorant_table_read(io: &mut IoHandler) -> Option<TagData> {
    let count = cms_read_u32(io)?;

    if count > CMS_MAX_CHANNELS as u32 {
        let text = format!("Too many colorants '{}'", count as i32);
        cms_signal_error(CMS_ERROR_RANGE, text.as_bytes());
        return None;
    }

    for _ in 0..count {
        let mut name = [0u8; 32];
        if io.read(&mut name, 32, 1) != 1 {
            return None;
        }
        let mut pcs = [0u16; 3];
        if !cms_read_u16_array(io, &mut pcs) {
            return None;
        }
    }

    Some(TagData::ColorantTable)
}

/// Port of `ReadSegmentedCurve` (src/cmstypes.c:4223-4344 @ lcms2.17).
fn read_segmented_curve(io: &mut IoHandler) -> Option<ToneCurve> {
    const PARAMS_BY_TYPE: [usize; 3] = [4, 5, 5];

    let mut prev_break = MINUS_INF; // - infinite

    // Take signature and channels for each element.
    let element_sig = cms_read_u32(io)?;

    // That should be a segmented curve
    if element_sig != SIG_SEGMENTED_CURVE {
        return None;
    }

    cms_read_u32(io)?;
    let n_segments = cms_read_u16(io)?;
    cms_read_u16(io)?;

    if n_segments < 1 {
        return None;
    }

    let mut segments = vec![CurveSegment::default(); n_segments as usize];

    // Read breakpoints
    for segment in &mut segments[..n_segments as usize - 1] {
        segment.x0 = prev_break;
        segment.x1 = cms_read_f32(io)?;
        prev_break = segment.x1;
    }

    segments[n_segments as usize - 1].x0 = prev_break;
    segments[n_segments as usize - 1].x1 = PLUS_INF; // A big cmsFloat32Number number

    // Read segments
    for segment in &mut segments {
        let element_sig = cms_read_u32(io)?;
        cms_read_u32(io)?;

        match element_sig {
            SIG_FORMULA_CURVE_SEG => {
                let type_ = cms_read_u16(io)?;
                cms_read_u16(io)?;

                segment.type_ = type_ as i32 + 6;
                if type_ > 2 {
                    return None;
                }

                for j in 0..PARAMS_BY_TYPE[type_ as usize] {
                    segment.params[j] = cms_read_f32(io)? as f64;
                }
            }

            SIG_SAMPLED_CURVE_SEG => {
                // The first point is implicit in the last stage, we allocate an extra note to be
                // populated latter on
                let count = cms_read_u32(io)?.wrapping_add(1);
                segment.n_grid_points = count;
                if !cms_calloc_fits(count, 4) {
                    return None;
                }
                let mut sampled_points = vec![0f32; count as usize];

                for point in &mut sampled_points[1..] {
                    *point = cms_read_f32(io)?;
                }
                segment.sampled_points = Some(sampled_points);
            }

            _ => {
                let mut text = b"Unknown curve element type '".to_vec();
                text.extend_from_slice(&cms_tag_signature_2_string(element_sig));
                text.extend_from_slice(b"' found.");
                cms_signal_error(CMS_ERROR_UNKNOWN_EXTENSION, &text);
                return None;
            }
        }
    }

    let mut curve = cms_build_segmented_tone_curve(&segments)?;

    // Explore for missing implicit points
    for i in 0..n_segments as usize {
        // If sampled curve, fix it
        if curve.segments[i].type_ == 0 {
            let first = cms_eval_tone_curve_float(&curve, curve.segments[i].x0);
            if let Some(points) = &mut curve.segments[i].sampled_points {
                points[0] = first;
            }
        }
    }

    Some(curve)
}

/// Port of `Type_MPEcurve_Read` (src/cmstypes.c:4362-4400 @ lcms2.17).
fn type_mpe_curve_read(io: &mut IoHandler) -> Option<Stage> {
    // Get actual position as a basis for element offsets
    let base_offset = io.tell().wrapping_sub(8);

    let input_chans = cms_read_u16(io)?;
    let output_chans = cms_read_u16(io)?;

    if input_chans != output_chans {
        return None;
    }

    if input_chans == 0 {
        return None; // _cmsCalloc of nothing
    }

    let mut gamma_tables = Vec::with_capacity(input_chans as usize);
    let read =
        read_position_table(
            io,
            input_chans as u32,
            base_offset,
            |io| match read_segmented_curve(io) {
                Some(curve) => {
                    gamma_tables.push(curve);
                    true
                }
                None => false,
            },
        );

    if read {
        cms_stage_alloc_tone_curves(input_chans as u32, Some(&gamma_tables))
    } else {
        None
    }
}

/// Port of `Type_MPEmatrix_Read` (src/cmstypes.c:4514-4579 @ lcms2.17).
fn type_mpe_matrix_read(io: &mut IoHandler) -> Option<Stage> {
    let input_chans = cms_read_u16(io)?;
    let output_chans = cms_read_u16(io)?;

    // Input and output chans may be ANY (up to 0xffff),
    // but we choose to limit to 16 channels for now
    if input_chans as usize >= CMS_MAX_CHANNELS {
        return None;
    }
    if output_chans as usize >= CMS_MAX_CHANNELS {
        return None;
    }

    let n_elems = input_chans as u32 * output_chans as u32;

    if n_elems == 0 {
        return None; // _cmsCalloc of nothing
    }

    let mut matrix = vec![0f64; n_elems as usize];
    let mut offsets = vec![0f64; output_chans as usize];

    for item in &mut matrix {
        *item = cms_read_f32(io)? as f64;
    }

    for item in &mut offsets {
        *item = cms_read_f32(io)? as f64;
    }

    cms_stage_alloc_matrix(
        output_chans as u32,
        input_chans as u32,
        &matrix,
        Some(&offsets),
    )
}

/// Port of `Type_MPEclut_Read` (src/cmstypes.c:4617-4663 @ lcms2.17).
fn type_mpe_clut_read(io: &mut IoHandler) -> Option<Stage> {
    let input_chans = cms_read_u16(io)?;
    let output_chans = cms_read_u16(io)?;

    if input_chans == 0 || input_chans as usize >= CMS_MAX_CHANNELS {
        return None;
    }
    if output_chans == 0 || output_chans as usize >= CMS_MAX_CHANNELS {
        return None;
    }

    let mut dimensions8 = [0u8; 16];
    if io.read(&mut dimensions8, 1, 16) != 16 {
        return None;
    }

    // Copy MAX_INPUT_DIMENSIONS at most. Expand to cmsUInt32Number
    let n_max_grids = (input_chans as usize).min(MAX_INPUT_DIMENSIONS);
    let mut grid_points = [0u32; MAX_INPUT_DIMENSIONS];

    for i in 0..n_max_grids {
        if dimensions8[i] == 1 {
            return None; // Impossible value, 0 for no CLUT and then 2 at least
        }
        grid_points[i] = dimensions8[i] as u32;
    }

    // Allocate the true CLUT
    let mut mpe = cms_stage_alloc_clut_float_granular(
        &grid_points,
        input_chans as u32,
        output_chans as u32,
        None,
    )?;

    // Read and sanitize the data
    if let StageData::Clut(StageClutData {
        tab: ClutTable::TFloat(table),
        ..
    }) = &mut mpe.data
    {
        for item in table.iter_mut() {
            *item = cms_read_f32(io)?;
        }
    }

    Some(mpe)
}

/// Port of `ReadMPEElem` (src/cmstypes.c:4716-4762 @ lcms2.17).
fn read_mpe_elem(io: &mut IoHandler, new_lut: &mut Pipeline) -> bool {
    // Take signature and channels for each element.
    let Some(element_sig) = cms_read_u32(io) else {
        return false;
    };

    // The reserved placeholder
    if cms_read_u32(io).is_none() {
        return false;
    }

    // Read diverse MPE types
    let stage = match element_sig {
        // If no read method, just ignore the element (valid for cmsSigBAcsElemType and
        // cmsSigEAcsElemType)
        SIG_B_ACS_ELEM_TYPE | SIG_E_ACS_ELEM_TYPE => return true,
        SIG_CURVE_SET_ELEM_TYPE => type_mpe_curve_read(io),
        SIG_MATRIX_ELEM_TYPE => type_mpe_matrix_read(io),
        SIG_C_LUT_ELEM_TYPE => type_mpe_clut_read(io),
        _ => {
            let mut text = b"Unknown MPE type '".to_vec();
            text.extend_from_slice(&cms_tag_signature_2_string(element_sig));
            text.extend_from_slice(b"' found.");

            // An unknown element was found.
            cms_signal_error(CMS_ERROR_UNKNOWN_EXTENSION, &text);
            return false;
        }
    };

    // This is a real element which should be read and processed
    cms_pipeline_insert_stage(new_lut, StageLoc::AtEnd, stage)
}

/// Port of `Type_MPE_Read` (src/cmstypes.c:4766-4806 @ lcms2.17).
fn type_mpe_read(io: &mut IoHandler) -> Option<Pipeline> {
    // Get actual position as a basis for element offsets
    let base_offset = io.tell().wrapping_sub(8);

    // Read channels and element count
    let input_chans = cms_read_u16(io)?;
    let output_chans = cms_read_u16(io)?;

    if input_chans == 0 || input_chans as usize >= CMS_MAX_CHANNELS {
        return None;
    }
    if output_chans == 0 || output_chans as usize >= CMS_MAX_CHANNELS {
        return None;
    }

    // Allocates an empty LUT
    let mut new_lut = cms_pipeline_alloc(input_chans as u32, output_chans as u32)?;

    let element_count = cms_read_u32(io)?;
    if !read_position_table(io, element_count, base_offset, |io| {
        read_mpe_elem(io, &mut new_lut)
    }) {
        return None;
    }

    // Check channel count
    if input_chans as u32 != new_lut.input_channels
        || output_chans as u32 != new_lut.output_channels
    {
        return None;
    }

    // Success
    Some(new_lut)
}

/// `TypeHandler->ReadPtr` (src/cmstypes.c:5887-5945 @ lcms2.17): the data and `*nItems`, or `None`
/// for a null. Only the types of the tags that are read get here.
pub fn type_read(base_type: u32, io: &mut IoHandler) -> Option<(TagData, u32)> {
    match base_type {
        SIG_XYZ_TYPE | CMS_CORBIS_BROKEN_XYZ_TYPE => type_xyz_read(io),
        SIG_CURVE_TYPE | CMS_MONACO_BROKEN_CURVE_TYPE => {
            Some((TagData::Curve(type_curve_read(io)?), 1))
        }
        SIG_PARAMETRIC_CURVE_TYPE => Some((TagData::Curve(type_parametric_curve_read(io)?), 1)),
        SIG_LUT8_TYPE => Some((TagData::Pipeline(type_lut8_read(io)?), 1)),
        SIG_LUT16_TYPE => Some((TagData::Pipeline(type_lut16_read(io)?), 1)),
        SIG_LUT_ATO_B_TYPE => Some((TagData::Pipeline(type_luta2b_read(io)?), 1)),
        SIG_LUT_BTO_A_TYPE => Some((TagData::Pipeline(type_lutb2a_read(io)?), 1)),
        SIG_MULTI_PROCESS_ELEMENT_TYPE => Some((TagData::Pipeline(type_mpe_read(io)?), 1)),
        SIG_COLORANT_TABLE_TYPE => Some((type_colorant_table_read(io)?, 1)),
        _ => None,
    }
}

/// `TypeHandler->WritePtr` (src/cmstypes.c:5887-5945 @ lcms2.17), for the types that are written.
pub fn type_write(type_: u32, io: &mut IoHandler, data: &TagData, n_items: u32) -> bool {
    match (type_, data) {
        (SIG_XYZ_TYPE, TagData::Xyz(xyz)) => cms_write_xyz(io, xyz),
        (SIG_S15_FIXED16_ARRAY_TYPE, TagData::S15Fixed16(value)) => {
            type_s15_fixed16_write(io, value, n_items)
        }
        (SIG_MULTI_LOCALIZED_UNICODE_TYPE, TagData::Mlu(mlu)) => type_mlu_write(io, mlu),
        (SIG_LUT_ATO_B_TYPE, TagData::Pipeline(lut)) => type_luta2b_write(io, lut),
        (SIG_LUT_BTO_A_TYPE, TagData::Pipeline(lut)) => type_lutb2a_write(io, lut),
        _ => false,
    }
}

/// `TagDescriptor->DecideType(Version, Data)`, or the first supported type where a tag has none:
/// `DecideTextDescType`, `DecideTextType`, `DecideCurveType`, `DecideLUTtypeA2B`,
/// `DecideLUTtypeB2A` and `DecideXYZtype` (src/cmstypes.c:390-397 @ lcms2.17).
pub fn decide_type(sig: u32, icc_version: f64, data: &TagData, descriptor: &TagDescriptor) -> u32 {
    match (sig, data) {
        (SIG_PROFILE_DESCRIPTION_TAG, _) => {
            if icc_version >= 4.0 {
                SIG_MULTI_LOCALIZED_UNICODE_TYPE
            } else {
                SIG_TEXT_DESCRIPTION_TYPE
            }
        }
        (SIG_COPYRIGHT_TAG, _) => {
            if icc_version >= 4.0 {
                SIG_MULTI_LOCALIZED_UNICODE_TYPE
            } else {
                SIG_TEXT_TYPE
            }
        }
        (SIG_RED_TRC_TAG | SIG_GREEN_TRC_TAG | SIG_BLUE_TRC_TAG, TagData::Curve(curve)) => {
            if icc_version < 4.0 {
                return SIG_CURVE_TYPE;
            }
            if curve.segments.len() != 1 {
                return SIG_CURVE_TYPE; // Only 1-segment curves can be saved as parametric
            }
            if curve.segments[0].type_ < 0 {
                return SIG_CURVE_TYPE; // Only non-inverted curves
            }
            if curve.segments[0].type_ > 5 {
                return SIG_CURVE_TYPE; // Only ICC parametric curves
            }
            SIG_PARAMETRIC_CURVE_TYPE
        }
        (SIG_A_TO_B0_TAG, TagData::Pipeline(lut)) => {
            if icc_version < 4.0 {
                if lut.save_as_8_bits {
                    SIG_LUT8_TYPE
                } else {
                    SIG_LUT16_TYPE
                }
            } else {
                SIG_LUT_ATO_B_TYPE
            }
        }
        (SIG_B_TO_A0_TAG, TagData::Pipeline(lut)) => {
            if icc_version < 4.0 {
                if lut.save_as_8_bits {
                    SIG_LUT8_TYPE
                } else {
                    SIG_LUT16_TYPE
                }
            } else {
                SIG_LUT_BTO_A_TYPE
            }
        }
        _ => descriptor.supported_types[0],
    }
}

/// Port of `_cmsGetTagDescriptor` (src/cmstypes.c:6211-6231 @ lcms2.17), with the table `SupportedTags`
/// (src/cmstypes.c:6113-6209 @ lcms2.17), generated from it one for one.
pub fn cms_get_tag_descriptor(sig: u32) -> Option<TagDescriptor> {
    let (elem_count, supported_types): (u32, &'static [u32]) = match sig {
        SIG_A_TO_B0_TAG => (1, &[SIG_LUT16_TYPE, SIG_LUT_ATO_B_TYPE, SIG_LUT8_TYPE]),
        SIG_A_TO_B1_TAG => (1, &[SIG_LUT16_TYPE, SIG_LUT_ATO_B_TYPE, SIG_LUT8_TYPE]),
        SIG_A_TO_B2_TAG => (1, &[SIG_LUT16_TYPE, SIG_LUT_ATO_B_TYPE, SIG_LUT8_TYPE]),
        SIG_B_TO_A0_TAG => (1, &[SIG_LUT16_TYPE, SIG_LUT_BTO_A_TYPE, SIG_LUT8_TYPE]),
        SIG_B_TO_A1_TAG => (1, &[SIG_LUT16_TYPE, SIG_LUT_BTO_A_TYPE, SIG_LUT8_TYPE]),
        SIG_B_TO_A2_TAG => (1, &[SIG_LUT16_TYPE, SIG_LUT_BTO_A_TYPE, SIG_LUT8_TYPE]),
        SIG_RED_COLORANT_TAG => (1, &[SIG_XYZ_TYPE, CMS_CORBIS_BROKEN_XYZ_TYPE]),
        SIG_GREEN_COLORANT_TAG => (1, &[SIG_XYZ_TYPE, CMS_CORBIS_BROKEN_XYZ_TYPE]),
        SIG_BLUE_COLORANT_TAG => (1, &[SIG_XYZ_TYPE, CMS_CORBIS_BROKEN_XYZ_TYPE]),
        SIG_RED_TRC_TAG => (
            1,
            &[
                SIG_CURVE_TYPE,
                SIG_PARAMETRIC_CURVE_TYPE,
                CMS_MONACO_BROKEN_CURVE_TYPE,
            ],
        ),
        SIG_GREEN_TRC_TAG => (
            1,
            &[
                SIG_CURVE_TYPE,
                SIG_PARAMETRIC_CURVE_TYPE,
                CMS_MONACO_BROKEN_CURVE_TYPE,
            ],
        ),
        SIG_BLUE_TRC_TAG => (
            1,
            &[
                SIG_CURVE_TYPE,
                SIG_PARAMETRIC_CURVE_TYPE,
                CMS_MONACO_BROKEN_CURVE_TYPE,
            ],
        ),
        SIG_CALIBRATION_DATE_TIME_TAG => (1, &[SIG_DATE_TIME_TYPE]),
        SIG_CHAR_TARGET_TAG => (1, &[SIG_TEXT_TYPE]),
        SIG_CHROMATIC_ADAPTATION_TAG => (9, &[SIG_S15_FIXED16_ARRAY_TYPE]),
        SIG_CHROMATICITY_TAG => (1, &[SIG_CHROMATICITY_TYPE]),
        SIG_COLORANT_ORDER_TAG => (1, &[SIG_COLORANT_ORDER_TYPE]),
        SIG_COLORANT_TABLE_TAG => (1, &[SIG_COLORANT_TABLE_TYPE]),
        SIG_COLORANT_TABLE_OUT_TAG => (1, &[SIG_COLORANT_TABLE_TYPE]),
        SIG_COPYRIGHT_TAG => (
            1,
            &[
                SIG_TEXT_TYPE,
                SIG_MULTI_LOCALIZED_UNICODE_TYPE,
                SIG_TEXT_DESCRIPTION_TYPE,
            ],
        ),
        SIG_DATE_TIME_TAG => (1, &[SIG_DATE_TIME_TYPE]),
        SIG_DEVICE_MFG_DESC_TAG => (
            1,
            &[
                SIG_TEXT_DESCRIPTION_TYPE,
                SIG_MULTI_LOCALIZED_UNICODE_TYPE,
                SIG_TEXT_TYPE,
            ],
        ),
        SIG_DEVICE_MODEL_DESC_TAG => (
            1,
            &[
                SIG_TEXT_DESCRIPTION_TYPE,
                SIG_MULTI_LOCALIZED_UNICODE_TYPE,
                SIG_TEXT_TYPE,
            ],
        ),
        SIG_GAMUT_TAG => (1, &[SIG_LUT16_TYPE, SIG_LUT_BTO_A_TYPE, SIG_LUT8_TYPE]),
        SIG_GRAY_TRC_TAG => (1, &[SIG_CURVE_TYPE, SIG_PARAMETRIC_CURVE_TYPE]),
        SIG_LUMINANCE_TAG => (1, &[SIG_XYZ_TYPE]),
        SIG_MEDIA_BLACK_POINT_TAG => (1, &[SIG_XYZ_TYPE, CMS_CORBIS_BROKEN_XYZ_TYPE]),
        SIG_MEDIA_WHITE_POINT_TAG => (1, &[SIG_XYZ_TYPE, CMS_CORBIS_BROKEN_XYZ_TYPE]),
        SIG_NAMED_COLOR2_TAG => (1, &[SIG_NAMED_COLOR2_TYPE]),
        SIG_PREVIEW0_TAG => (1, &[SIG_LUT16_TYPE, SIG_LUT_BTO_A_TYPE, SIG_LUT8_TYPE]),
        SIG_PREVIEW1_TAG => (1, &[SIG_LUT16_TYPE, SIG_LUT_BTO_A_TYPE, SIG_LUT8_TYPE]),
        SIG_PREVIEW2_TAG => (1, &[SIG_LUT16_TYPE, SIG_LUT_BTO_A_TYPE, SIG_LUT8_TYPE]),
        SIG_PROFILE_DESCRIPTION_TAG => (
            1,
            &[
                SIG_TEXT_DESCRIPTION_TYPE,
                SIG_MULTI_LOCALIZED_UNICODE_TYPE,
                SIG_TEXT_TYPE,
            ],
        ),
        SIG_PROFILE_SEQUENCE_DESC_TAG => (1, &[SIG_PROFILE_SEQUENCE_DESC_TYPE]),
        SIG_TECHNOLOGY_TAG => (1, &[SIG_SIGNATURE_TYPE]),
        SIG_COLORIMETRIC_INTENT_IMAGE_STATE_TAG => (1, &[SIG_SIGNATURE_TYPE]),
        SIG_PERCEPTUAL_RENDERING_INTENT_GAMUT_TAG => (1, &[SIG_SIGNATURE_TYPE]),
        SIG_SATURATION_RENDERING_INTENT_GAMUT_TAG => (1, &[SIG_SIGNATURE_TYPE]),
        SIG_MEASUREMENT_TAG => (1, &[SIG_MEASUREMENT_TYPE]),
        SIG_PS2_CRD0_TAG => (1, &[SIG_DATA_TYPE]),
        SIG_PS2_CRD1_TAG => (1, &[SIG_DATA_TYPE]),
        SIG_PS2_CRD2_TAG => (1, &[SIG_DATA_TYPE]),
        SIG_PS2_CRD3_TAG => (1, &[SIG_DATA_TYPE]),
        SIG_PS2_CSA_TAG => (1, &[SIG_DATA_TYPE]),
        SIG_PS2_RENDERING_INTENT_TAG => (1, &[SIG_DATA_TYPE]),
        SIG_VIEWING_COND_DESC_TAG => (
            1,
            &[
                SIG_TEXT_DESCRIPTION_TYPE,
                SIG_MULTI_LOCALIZED_UNICODE_TYPE,
                SIG_TEXT_TYPE,
            ],
        ),
        SIG_UCR_BG_TAG => (1, &[SIG_UCR_BG_TYPE]),
        SIG_CRD_INFO_TAG => (1, &[SIG_CRD_INFO_TYPE]),
        SIG_D_TO_B0_TAG => (1, &[SIG_MULTI_PROCESS_ELEMENT_TYPE]),
        SIG_D_TO_B1_TAG => (1, &[SIG_MULTI_PROCESS_ELEMENT_TYPE]),
        SIG_D_TO_B2_TAG => (1, &[SIG_MULTI_PROCESS_ELEMENT_TYPE]),
        SIG_D_TO_B3_TAG => (1, &[SIG_MULTI_PROCESS_ELEMENT_TYPE]),
        SIG_B_TO_D0_TAG => (1, &[SIG_MULTI_PROCESS_ELEMENT_TYPE]),
        SIG_B_TO_D1_TAG => (1, &[SIG_MULTI_PROCESS_ELEMENT_TYPE]),
        SIG_B_TO_D2_TAG => (1, &[SIG_MULTI_PROCESS_ELEMENT_TYPE]),
        SIG_B_TO_D3_TAG => (1, &[SIG_MULTI_PROCESS_ELEMENT_TYPE]),
        SIG_SCREENING_DESC_TAG => (1, &[SIG_TEXT_DESCRIPTION_TYPE]),
        SIG_VIEWING_CONDITIONS_TAG => (1, &[SIG_VIEWING_CONDITIONS_TYPE]),
        SIG_SCREENING_TAG => (1, &[SIG_SCREENING_TYPE]),
        SIG_VCGT_TAG => (1, &[SIG_VCGT_TYPE]),
        SIG_META_TAG => (1, &[SIG_DICT_TYPE]),
        SIG_PROFILE_SEQUENCE_ID_TAG => (1, &[SIG_PROFILE_SEQUENCE_ID_TYPE]),
        SIG_PROFILE_DESCRIPTION_ML_TAG => (1, &[SIG_MULTI_LOCALIZED_UNICODE_TYPE]),
        SIG_CICP_TAG => (1, &[SIG_CICP_TYPE]),
        SIG_ARGYLL_ARTS_TAG => (9, &[SIG_S15_FIXED16_ARRAY_TYPE]),
        SIG_MHC2_TAG => (1, &[SIG_MHC2_TYPE]),
        _ => return None,
    };
    Some(TagDescriptor {
        elem_count,
        supported_types,
    })
}
