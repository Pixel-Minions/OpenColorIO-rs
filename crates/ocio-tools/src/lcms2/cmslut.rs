// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/cmslut.c` @ lcms2.17: pipeline stages and pipelines.
//!
//! C links the stages of a pipeline in a list and gives each stage an evaluator, a duplicator and
//! a destructor. Here a pipeline holds a `Vec<Stage>`, a stage's kind is its data, and `Clone`
//! is `cmsStageDup` and `cmsPipelineDup`: their only ways to fail are limits that the original's
//! allocation has passed already.

use super::cmserr::{cms_calloc_fits, cms_signal_error};
use super::cmsgamma::{ToneCurve, cms_build_gamma, cms_eval_tone_curve_float};
use super::cmsintrp::{InterpParams, cms_compute_interp_params_ex};
use super::cmspcs::{cms_lab_2_xyz, cms_xyz_2_lab};
use super::types::*;

/// `cmsStageLoc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageLoc {
    AtBegin,
    AtEnd,
}

/// The table of `_cmsStageCLutData`.
#[derive(Debug, Clone)]
pub enum ClutTable {
    /// `Tab.T`.
    T(Vec<u16>),
    /// `Tab.TFloat`.
    TFloat(Vec<f32>),
}

/// `_cmsStageCLutData`.
#[derive(Debug, Clone)]
pub struct StageClutData {
    pub tab: ClutTable,
    pub params: InterpParams,
}

/// What a stage holds, which also says how it evaluates.
#[derive(Debug, Clone)]
pub enum StageData {
    /// `EvaluateIdentity`.
    Identity,
    /// `EvaluateCurves`, with `_cmsStageToneCurvesData`.
    Curves(Vec<ToneCurve>),
    /// `EvaluateMatrix`, with `_cmsStageMatrixData`.
    Matrix {
        double: Vec<f64>,
        offset: Option<Vec<f64>>,
    },
    /// `EvaluateCLUTfloatIn16` or `EvaluateCLUTfloat`.
    Clut(StageClutData),
    /// `EvaluateLab2XYZ`.
    Lab2Xyz,
    /// `EvaluateXYZ2Lab`.
    Xyz2Lab,
}

/// `cmsStage`.
#[derive(Debug, Clone)]
pub struct Stage {
    /// Identifies the stage.
    pub type_: u32,
    /// Identifies the *function* of the stage (for optimizations).
    pub implements: u32,
    pub input_channels: u32,
    pub output_channels: u32,
    pub data: StageData,
}

/// `cmsPipeline`.
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub elements: Vec<Stage>,
    pub input_channels: u32,
    pub output_channels: u32,
    /// `Eval16Fn` is `FastIdentity16` (src/cmsopt.c @ lcms2.17): nothing is left of the pipeline.
    pub fast_identity16: bool,
    pub save_as_8_bits: bool,
}

/// The two buffers `_LUTeval16` and `_LUTevalFloat` take turns on.
type Storage = [[f32; MAX_STAGE_CHANNELS]; 2];

/// Port of `_cmsStageAllocPlaceholder` (src/cmslut.c:31-58 @ lcms2.17).
fn cms_stage_alloc_placeholder(
    type_: u32,
    input_channels: u32,
    output_channels: u32,
    data: StageData,
) -> Stage {
    Stage {
        type_,
        implements: type_,
        input_channels,
        output_channels,
        data,
    }
}

/// Port of `EvaluateIdentity` (src/cmslut.c:61-67 @ lcms2.17).
fn evaluate_identity(in_: &[f32], out: &mut [f32], mpe: &Stage) {
    let n = mpe.input_channels as usize;
    out[..n].copy_from_slice(&in_[..n]);
}

/// Port of `cmsStageAllocIdentity` (src/cmslut.c:70-79 @ lcms2.17).
pub fn cms_stage_alloc_identity(n_channels: u32) -> Stage {
    cms_stage_alloc_placeholder(
        SIG_IDENTITY_ELEM_TYPE,
        n_channels,
        n_channels,
        StageData::Identity,
    )
}

/// Port of `FromFloatTo16` (src/cmslut.c:82-90 @ lcms2.17).
#[inline]
fn from_float_to_16(in_: &[f32], out: &mut [u16], n: usize) {
    for i in 0..n {
        out[i] = cms_quick_saturate_word(in_[i] as f64 * 65535.0);
    }
}

/// Port of `From16ToFloat` (src/cmslut.c:93-101 @ lcms2.17).
#[inline]
fn from_16_to_float(in_: &[u16], out: &mut [f32], n: usize) {
    for i in 0..n {
        out[i] = in_[i] as f32 / 65535.0f32;
    }
}

/// Port of `EvaluateCurves` (src/cmslut.c:166-184 @ lcms2.17).
fn evaluate_curves(in_: &[f32], out: &mut [f32], curves: &[ToneCurve]) {
    for (i, curve) in curves.iter().enumerate() {
        out[i] = cms_eval_tone_curve_float(curve, in_[i]);
    }
}

/// Port of `cmsStageAllocToneCurves` (src/cmslut.c:248-291 @ lcms2.17). `curves` of `None` forces
/// identity curves.
pub fn cms_stage_alloc_tone_curves(n_channels: u32, curves: Option<&[ToneCurve]>) -> Option<Stage> {
    let mut the_curves = Vec::with_capacity(n_channels as usize);
    for i in 0..n_channels as usize {
        the_curves.push(match curves {
            None => cms_build_gamma(1.0)?,
            Some(curves) => curves[i].clone(),
        });
    }
    if n_channels == 0 {
        // _cmsCalloc of nothing.
        return None;
    }
    Some(cms_stage_alloc_placeholder(
        SIG_CURVE_SET_ELEM_TYPE,
        n_channels,
        n_channels,
        StageData::Curves(the_curves),
    ))
}

/// Port of `_cmsStageAllocIdentityCurves` (src/cmslut.c:295-302 @ lcms2.17).
pub fn cms_stage_alloc_identity_curves(n_channels: u32) -> Option<Stage> {
    let mut mpe = cms_stage_alloc_tone_curves(n_channels, None)?;
    mpe.implements = SIG_IDENTITY_ELEM_TYPE;
    Some(mpe)
}

/// Port of `EvaluateMatrix` (src/cmslut.c:311-336 @ lcms2.17).
fn evaluate_matrix(
    in_: &[f32],
    out: &mut [f32],
    mpe: &Stage,
    double: &[f64],
    offset: Option<&[f64]>,
) {
    let input_channels = mpe.input_channels as usize;

    // Input is already in 0..1.0 notation
    for i in 0..mpe.output_channels as usize {
        let mut tmp = 0f64;
        for j in 0..input_channels {
            tmp += in_[j] as f64 * double[i * input_channels + j];
        }

        if let Some(offset) = offset {
            tmp += offset[i];
        }

        out[i] = tmp as f32;
    }

    // Output in 0..1.0 domain
}

/// Port of `cmsStageAllocMatrix` (src/cmslut.c:379-425 @ lcms2.17).
pub fn cms_stage_alloc_matrix(
    rows: u32,
    cols: u32,
    matrix: &[f64],
    offset: Option<&[f64]>,
) -> Option<Stage> {
    let n = rows.wrapping_mul(cols);

    // Check for overflow
    if n == 0 {
        return None;
    }
    if n >= u32::MAX / cols {
        return None;
    }
    if n >= u32::MAX / rows {
        return None;
    }
    if n < rows || n < cols {
        return None;
    }

    Some(cms_stage_alloc_placeholder(
        SIG_MATRIX_ELEM_TYPE,
        cols,
        rows,
        StageData::Matrix {
            double: matrix[..n as usize].to_vec(),
            offset: offset.map(|offset| offset[..rows as usize].to_vec()),
        },
    ))
}

/// Port of `EvaluateCLUTfloat` (src/cmslut.c:434-440 @ lcms2.17) and `EvaluateCLUTfloatIn16` (src/cmslut.c:444-456 @ lcms2.17).
fn evaluate_clut(in_: &[f32], out: &mut [f32], mpe: &Stage, data: &StageClutData) {
    match &data.tab {
        ClutTable::TFloat(table) => data.params.lerp_float(table, in_, out),
        ClutTable::T(table) => {
            let mut in16 = [0u16; MAX_STAGE_CHANNELS];
            let mut out16 = [0u16; MAX_STAGE_CHANNELS];

            from_float_to_16(in_, &mut in16, mpe.input_channels as usize);
            data.params.lerp16(table, &in16, &mut out16);
            from_16_to_float(&out16, out, mpe.output_channels as usize);
        }
    }
}

/// Port of `CubeSize` (src/cmslut.c:460-482 @ lcms2.17).
fn cube_size(dims: &[u32], b: u32) -> u32 {
    let mut rv = 1u32;
    for i in (0..b as usize).rev() {
        let dim = dims[i];
        if dim <= 1 {
            return 0; // Error
        }

        rv = rv.wrapping_mul(dim);

        // Check for overflow
        if rv > u32::MAX / dim {
            return 0;
        }
    }

    // Again, prevent overflow
    if rv > u32::MAX / 15 {
        return 0;
    }

    rv
}

/// Port of `cmsStageAllocCLut16bitGranular` (src/cmslut.c:547-605 @ lcms2.17).
pub fn cms_stage_alloc_clut16bit_granular(
    clut_points: &[u32],
    input_chan: u32,
    output_chan: u32,
    table: Option<&[u16]>,
) -> Option<Stage> {
    if input_chan > MAX_INPUT_DIMENSIONS as u32 {
        cms_signal_error(
            CMS_ERROR_RANGE,
            format!("Too many input channels ({input_chan} channels, max={MAX_INPUT_DIMENSIONS})")
                .as_bytes(),
        );
        return None;
    }

    let n = output_chan.wrapping_mul(cube_size(clut_points, input_chan));
    if n == 0 {
        return None;
    }

    if !cms_calloc_fits(n, 2) {
        return None;
    }
    let mut tab = vec![0u16; n as usize];

    if let Some(table) = table {
        tab.copy_from_slice(&table[..n as usize]);
    }

    let params =
        cms_compute_interp_params_ex(clut_points, input_chan, output_chan, CMS_LERP_FLAGS_16BITS)?;

    Some(cms_stage_alloc_placeholder(
        SIG_C_LUT_ELEM_TYPE,
        input_chan,
        output_chan,
        StageData::Clut(StageClutData {
            tab: ClutTable::T(tab),
            params,
        }),
    ))
}

/// Port of `cmsStageAllocCLut16bit` (src/cmslut.c:607-621 @ lcms2.17).
pub fn cms_stage_alloc_clut16bit(
    n_grid_points: u32,
    input_chan: u32,
    output_chan: u32,
    table: Option<&[u16]>,
) -> Option<Stage> {
    // Our resulting LUT would be same gridpoints on all dimensions
    let dimensions = [n_grid_points; MAX_INPUT_DIMENSIONS];
    cms_stage_alloc_clut16bit_granular(&dimensions, input_chan, output_chan, table)
}

/// Port of `cmsStageAllocCLutFloatGranular` (src/cmslut.c:642-696 @ lcms2.17).
pub fn cms_stage_alloc_clut_float_granular(
    clut_points: &[u32],
    input_chan: u32,
    output_chan: u32,
    table: Option<&[f32]>,
) -> Option<Stage> {
    if input_chan > MAX_INPUT_DIMENSIONS as u32 {
        cms_signal_error(
            CMS_ERROR_RANGE,
            format!("Too many input channels ({input_chan} channels, max={MAX_INPUT_DIMENSIONS})")
                .as_bytes(),
        );
        return None;
    }

    // There is a potential integer overflow on conputing n and nEntries.
    let n = output_chan.wrapping_mul(cube_size(clut_points, input_chan));
    if n == 0 {
        return None;
    }

    if !cms_calloc_fits(n, 4) {
        return None;
    }
    let mut tab = vec![0f32; n as usize];

    if let Some(table) = table {
        tab.copy_from_slice(&table[..n as usize]);
    }

    let params =
        cms_compute_interp_params_ex(clut_points, input_chan, output_chan, CMS_LERP_FLAGS_FLOAT)?;

    Some(cms_stage_alloc_placeholder(
        SIG_C_LUT_ELEM_TYPE,
        input_chan,
        output_chan,
        StageData::Clut(StageClutData {
            tab: ClutTable::TFloat(tab),
            params,
        }),
    ))
}

/// Port of `_cmsStageAllocIdentityCLut` (src/cmslut.c:712-731 @ lcms2.17).
pub fn cms_stage_alloc_identity_clut(n_chan: u32) -> Option<Stage> {
    let dimensions = [2u32; MAX_INPUT_DIMENSIONS];

    let mut mpe = cms_stage_alloc_clut16bit_granular(&dimensions, n_chan, n_chan, None)?;

    // IdentitySampler
    let identity = |in_: &[u16], out: &mut [u16]| {
        out[..n_chan as usize].copy_from_slice(&in_[..n_chan as usize]);
        true
    };
    if !cms_stage_sample_clut16bit(Some(&mut mpe), identity, 0) {
        return None;
    }

    mpe.implements = SIG_IDENTITY_ELEM_TYPE;
    Some(mpe)
}

/// Port of `_cmsQuantizeVal` (src/cmslut.c:736-742 @ lcms2.17).
pub fn cms_quantize_val(i: f64, max_samples: u32) -> u16 {
    let x = (i * 65535.) / (max_samples - 1) as f64;
    cms_quick_saturate_word(x)
}

/// Port of `cmsStageSampleCLut16bit` (src/cmslut.c:747-810 @ lcms2.17). A closure takes the sampler's
/// place and its cargo.
pub fn cms_stage_sample_clut16bit(
    mpe: Option<&mut Stage>,
    mut sampler: impl FnMut(&[u16], &mut [u16]) -> bool,
    dw_flags: u32,
) -> bool {
    let Some(mpe) = mpe else {
        return false;
    };
    let StageData::Clut(clut) = &mut mpe.data else {
        return false;
    };

    let n_samples = clut.params.n_samples;
    let n_inputs = clut.params.n_inputs;
    let n_outputs = clut.params.n_outputs;

    if n_inputs == 0 {
        return false;
    }
    if n_outputs == 0 {
        return false;
    }
    if n_inputs > MAX_INPUT_DIMENSIONS as u32 {
        return false;
    }
    if n_outputs >= MAX_STAGE_CHANNELS as u32 {
        return false;
    }

    let mut in_ = [0u16; MAX_INPUT_DIMENSIONS + 1];
    let mut out = [0u16; MAX_STAGE_CHANNELS];

    let n_total_points = cube_size(&n_samples, n_inputs);
    if n_total_points == 0 {
        return false;
    }

    let ClutTable::T(table) = &mut clut.tab else {
        return false;
    };

    let n_inputs = n_inputs as usize;
    let n_outputs = n_outputs as usize;
    let mut index = 0usize;
    for i in 0..n_total_points as i32 {
        let mut rest = i as u32;
        for t in (0..n_inputs).rev() {
            let colorant = rest % n_samples[t];

            rest /= n_samples[t];

            in_[t] = cms_quantize_val(colorant as f64, n_samples[t]);
        }

        out[..n_outputs].copy_from_slice(&table[index..index + n_outputs]);

        if !sampler(&in_, &mut out) {
            return false;
        }

        if dw_flags & SAMPLER_INSPECT == 0 {
            table[index..index + n_outputs].copy_from_slice(&out[..n_outputs]);
        }

        index += n_outputs;
    }

    true
}

/// Port of `EvaluateLab2XYZ` (src/cmslut.c:946-971 @ lcms2.17).
fn evaluate_lab2xyz(in_: &[f32], out: &mut [f32]) {
    const XYZ_ADJ: f64 = MAX_ENCODEABLE_XYZ;

    // V4 rules
    let lab = CieLab {
        l: in_[0] as f64 * 100.0,
        a: in_[1] as f64 * 255.0 - 128.0,
        b: in_[2] as f64 * 255.0 - 128.0,
    };

    let xyz = cms_lab_2_xyz(&lab);

    // From XYZ, range 0..19997 to 0..1.0, note that 1.99997 comes from 0xffff
    // encoded as 1.15 fixed point, so 1 + (32767.0 / 32768.0)

    out[0] = (xyz.x / XYZ_ADJ) as f32;
    out[1] = (xyz.y / XYZ_ADJ) as f32;
    out[2] = (xyz.z / XYZ_ADJ) as f32;
}

/// Port of `_cmsStageAllocLab2XYZ` (src/cmslut.c:975-978 @ lcms2.17).
pub fn cms_stage_alloc_lab2xyz() -> Stage {
    cms_stage_alloc_placeholder(SIG_LAB2_XYZ_ELEM_TYPE, 3, 3, StageData::Lab2Xyz)
}

/// Port of `_cmsStageAllocLabV2ToV4` (src/cmslut.c:1026-1038 @ lcms2.17).
pub fn cms_stage_alloc_lab_v2_to_v4() -> Option<Stage> {
    const N: f64 = 65535.0 / 65280.0;
    const V2_TO_V4: [f64; 9] = [N, 0.0, 0.0, 0.0, N, 0.0, 0.0, 0.0, N];

    let mut mpe = cms_stage_alloc_matrix(3, 3, &V2_TO_V4, None)?;
    mpe.implements = SIG_LAB_V2TO_V4;
    Some(mpe)
}

/// Port of `_cmsStageAllocLabV4ToV2` (src/cmslut.c:1042-1054 @ lcms2.17).
pub fn cms_stage_alloc_lab_v4_to_v2() -> Option<Stage> {
    const N: f64 = 65280.0 / 65535.0;
    const V4_TO_V2: [f64; 9] = [N, 0.0, 0.0, 0.0, N, 0.0, 0.0, 0.0, N];

    let mut mpe = cms_stage_alloc_matrix(3, 3, &V4_TO_V2, None)?;
    mpe.implements = SIG_LAB_V4TO_V2;
    Some(mpe)
}

/// Port of `_cmsStageNormalizeFromLabFloat` (src/cmslut.c:1062-1081 @ lcms2.17).
pub fn cms_stage_normalize_from_lab_float() -> Option<Stage> {
    const A1: [f64; 9] = [
        1.0 / 100.0,
        0.0,
        0.0,
        0.0,
        1.0 / 255.0,
        0.0,
        0.0,
        0.0,
        1.0 / 255.0,
    ];
    const O1: [f64; 3] = [0.0, 128.0 / 255.0, 128.0 / 255.0];

    let mut mpe = cms_stage_alloc_matrix(3, 3, &A1, Some(&O1))?;
    mpe.implements = SIG_LAB2_FLOAT_PCS;
    Some(mpe)
}

/// Port of `_cmsStageNormalizeFromXyzFloat` (src/cmslut.c:1084-1099 @ lcms2.17).
pub fn cms_stage_normalize_from_xyz_float() -> Option<Stage> {
    const N: f64 = 32768.0 / 65535.0;
    const A1: [f64; 9] = [N, 0.0, 0.0, 0.0, N, 0.0, 0.0, 0.0, N];

    let mut mpe = cms_stage_alloc_matrix(3, 3, &A1, None)?;
    mpe.implements = SIG_XYZ2_FLOAT_PCS;
    Some(mpe)
}

/// Port of `_cmsStageNormalizeToLabFloat` (src/cmslut.c:1101-1119 @ lcms2.17).
pub fn cms_stage_normalize_to_lab_float() -> Option<Stage> {
    const A1: [f64; 9] = [100.0, 0.0, 0.0, 0.0, 255.0, 0.0, 0.0, 0.0, 255.0];
    const O1: [f64; 3] = [0.0, -128.0, -128.0];

    let mut mpe = cms_stage_alloc_matrix(3, 3, &A1, Some(&O1))?;
    mpe.implements = SIG_FLOAT_PCS2_LAB;
    Some(mpe)
}

/// Port of `_cmsStageNormalizeToXyzFloat` (src/cmslut.c:1121-1136 @ lcms2.17).
pub fn cms_stage_normalize_to_xyz_float() -> Option<Stage> {
    const N: f64 = 65535.0 / 32768.0;
    const A1: [f64; 9] = [N, 0.0, 0.0, 0.0, N, 0.0, 0.0, 0.0, N];

    let mut mpe = cms_stage_alloc_matrix(3, 3, &A1, None)?;
    mpe.implements = SIG_FLOAT_PCS2_XYZ;
    Some(mpe)
}

/// Port of `EvaluateXYZ2Lab` (src/cmslut.c:1160-1183 @ lcms2.17).
fn evaluate_xyz2lab(in_: &[f32], out: &mut [f32]) {
    const XYZ_ADJ: f64 = MAX_ENCODEABLE_XYZ;

    // From 0..1.0 to XYZ

    let xyz = CieXyz {
        x: in_[0] as f64 * XYZ_ADJ,
        y: in_[1] as f64 * XYZ_ADJ,
        z: in_[2] as f64 * XYZ_ADJ,
    };

    let lab = cms_xyz_2_lab(&xyz);

    // From V4 Lab to 0..1.0

    out[0] = (lab.l / 100.0) as f32;
    out[1] = ((lab.a + 128.0) / 255.0) as f32;
    out[2] = ((lab.b + 128.0) / 255.0) as f32;
}

/// Port of `_cmsStageAllocXYZ2Lab` (src/cmslut.c:1185-1189 @ lcms2.17).
pub fn cms_stage_alloc_xyz2lab() -> Stage {
    cms_stage_alloc_placeholder(SIG_XYZ2_LAB_ELEM_TYPE, 3, 3, StageData::Xyz2Lab)
}

impl Stage {
    /// `mpe->EvalPtr(In, Out, mpe)`.
    #[inline]
    fn eval(&self, in_: &[f32], out: &mut [f32]) {
        match &self.data {
            StageData::Identity => evaluate_identity(in_, out, self),
            StageData::Curves(curves) => evaluate_curves(in_, out, curves),
            StageData::Matrix { double, offset } => {
                evaluate_matrix(in_, out, self, double, offset.as_deref())
            }
            StageData::Clut(data) => evaluate_clut(in_, out, self, data),
            StageData::Lab2Xyz => evaluate_lab2xyz(in_, out),
            StageData::Xyz2Lab => evaluate_xyz2lab(in_, out),
        }
    }
}

/// Port of `BlessLUT` (src/cmslut.c:1289-1323 @ lcms2.17).
fn bless_lut(lut: &mut Pipeline) -> bool {
    // We can set the input/output channels only if we have elements.
    if let (Some(first), Some(last)) = (lut.elements.first(), lut.elements.last()) {
        lut.input_channels = first.input_channels;
        lut.output_channels = last.output_channels;

        // Check chain consistency
        for pair in lut.elements.windows(2) {
            if pair[1].input_channels != pair[0].output_channels {
                return false;
            }
        }
    }

    true
}

/// The loop of `_LUTeval16` and `_LUTevalFloat` over the stages: where the result ends.
#[inline]
fn run_stages(lut: &Pipeline, storage: &mut Storage) -> usize {
    let mut phase = 0;
    for mpe in &lut.elements {
        let next_phase = phase ^ 1;
        let (first, second) = storage.split_at_mut(1);
        if phase == 0 {
            mpe.eval(&first[0], &mut second[0]);
        } else {
            mpe.eval(&second[0], &mut first[0]);
        }
        phase = next_phase;
    }
    phase
}

/// Port of `_LUTeval16` (src/cmslut.c:1327-1348 @ lcms2.17).
fn lut_eval16(in_: &[u16], out: &mut [u16], lut: &Pipeline) {
    let mut storage: Storage = [[0.0; MAX_STAGE_CHANNELS]; 2];

    from_16_to_float(in_, &mut storage[0], lut.input_channels as usize);

    let phase = run_stages(lut, &mut storage);

    from_float_to_16(&storage[phase], out, lut.output_channels as usize);
}

/// Port of `_LUTevalFloat` (src/cmslut.c:1353-1373 @ lcms2.17).
fn lut_eval_float(in_: &[f32], out: &mut [f32], lut: &Pipeline) {
    let mut storage: Storage = [[0.0; MAX_STAGE_CHANNELS]; 2];

    let input_channels = lut.input_channels as usize;
    storage[0][..input_channels].copy_from_slice(&in_[..input_channels]);

    let phase = run_stages(lut, &mut storage);

    let output_channels = lut.output_channels as usize;
    out[..output_channels].copy_from_slice(&storage[phase][..output_channels]);
}

/// Port of `cmsPipelineAlloc` (src/cmslut.c:1377-1405 @ lcms2.17).
pub fn cms_pipeline_alloc(input_channels: u32, output_channels: u32) -> Option<Pipeline> {
    // A value of zero in channels is allowed as placeholder
    if input_channels >= CMS_MAX_CHANNELS as u32 || output_channels >= CMS_MAX_CHANNELS as u32 {
        return None;
    }

    Some(Pipeline {
        elements: Vec::new(),
        input_channels,
        output_channels,
        fast_identity16: false,
        save_as_8_bits: false,
    })
}

/// Port of `cmsPipelineEval16` (src/cmslut.c:1447-1451 @ lcms2.17).
#[inline]
pub fn cms_pipeline_eval16(in_: &[u16], out: &mut [u16], lut: &Pipeline) {
    if lut.fast_identity16 {
        // FastIdentity16 (src/cmsopt.c @ lcms2.17)
        let n = lut.input_channels as usize;
        out[..n].copy_from_slice(&in_[..n]);
    } else {
        lut_eval16(in_, out, lut);
    }
}

/// Port of `cmsPipelineEvalFloat` (src/cmslut.c:1455-1459 @ lcms2.17).
#[inline]
pub fn cms_pipeline_eval_float(in_: &[f32], out: &mut [f32], lut: &Pipeline) {
    lut_eval_float(in_, out, lut);
}

/// Port of `cmsPipelineInsertStage` (src/cmslut.c:1519-1552 @ lcms2.17).
pub fn cms_pipeline_insert_stage(lut: &mut Pipeline, loc: StageLoc, mpe: Option<Stage>) -> bool {
    let Some(mpe) = mpe else {
        return false;
    };

    match loc {
        StageLoc::AtBegin => lut.elements.insert(0, mpe),
        StageLoc::AtEnd => lut.elements.push(mpe),
    }

    bless_lut(lut)
}

/// Port of `cmsPipelineCat` (src/cmslut.c:1612-1634 @ lcms2.17).
pub fn cms_pipeline_cat(l1: &mut Pipeline, l2: &Pipeline) -> bool {
    // If both LUTS does not have elements, we need to inherit
    // the number of channels
    if l1.elements.is_empty() && l2.elements.is_empty() {
        l1.input_channels = l2.input_channels;
        l1.output_channels = l2.output_channels;
    }

    // Cat second
    for mpe in &l2.elements {
        // We have to dup each element
        if !cms_pipeline_insert_stage(l1, StageLoc::AtEnd, Some(mpe.clone())) {
            return false;
        }
    }

    bless_lut(l1)
}

#[cfg(test)]
#[path = "cmslut_tests.rs"]
mod tests;
