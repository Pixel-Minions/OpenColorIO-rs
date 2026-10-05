// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The constants, hue helpers, tables and parameter sets of ACES 2.0: a port of
//! `src/OpenColorIO/ops/fixedfunction/ACES2/Common.h` @ v2.5.2.
//!
//! Upstream's constants are `constexpr float`s, some computed from others (`0.2713f * 100`,
//! `1 / 0.55f`); Rust evaluates the same `f32` operations, rounded the same way, at compile
//! time.

use super::matrix_lib::{F2, F3, M33f};
use crate::transforms::builtins::color_matrix_helpers::{Chromaticities, Primaries};

/// `PI`.
pub const PI: f32 = 3.14159265358979;

/// `hue_limit`: hues are in degrees.
pub const HUE_LIMIT: f32 = 360.0;

/// A hue below 0 plus [`HUE_LIMIT`].
///
/// Port of `_wrap_to_hue_limit` (src/OpenColorIO/ops/fixedfunction/ACES2/Common.h:21-28 @
/// v2.5.2).
#[inline]
pub fn wrap_to_hue_limit_(y: f32) -> f32 {
    if y < 0.0 { y + HUE_LIMIT } else { y }
}

/// `fmodf(hue, 360)`, wrapped to [0, 360).
///
/// Port of `wrap_to_hue_limit` (src/OpenColorIO/ops/fixedfunction/ACES2/Common.h:30-34 @
/// v2.5.2).
#[inline]
pub fn wrap_to_hue_limit(hue: f32) -> f32 {
    let y = hue % HUE_LIMIT;
    wrap_to_hue_limit_(y)
}

/// Port of `to_degrees` (src/OpenColorIO/ops/fixedfunction/ACES2/Common.h:35 @ v2.5.2).
#[inline]
pub fn to_degrees(v: f32) -> f32 {
    v
}

/// Port of `from_degrees` (src/OpenColorIO/ops/fixedfunction/ACES2/Common.h:36 @ v2.5.2).
#[inline]
pub fn from_degrees(v: f32) -> f32 {
    wrap_to_hue_limit(v)
}

/// `PI * v / 180`.
///
/// Port of `to_radians` (src/OpenColorIO/ops/fixedfunction/ACES2/Common.h:37 @ v2.5.2).
#[inline]
pub fn to_radians(v: f32) -> f32 {
    PI * v / 180.0
}

/// `180 * v / PI` for a `v` already wrapped, made non-negative.
///
/// Port of `_from_radians` (src/OpenColorIO/ops/fixedfunction/ACES2/Common.h:38 @ v2.5.2).
#[inline]
pub fn from_radians_(v: f32) -> f32 {
    wrap_to_hue_limit_(180.0 * v / PI)
}

/// `180 * v / PI`, wrapped to [0, 360).
///
/// Port of `from_radians` (src/OpenColorIO/ops/fixedfunction/ACES2/Common.h:39 @ v2.5.2).
#[inline]
pub fn from_radians(v: f32) -> f32 {
    wrap_to_hue_limit(180.0 * v / PI)
}

/// The layout of the hue tables: one entry per degree, with one entry below and two above for
/// the wrap.
///
/// Port of `TableBase` (src/OpenColorIO/ops/fixedfunction/ACES2/Common.h:48-82 @ v2.5.2).
pub mod table_base {
    use super::HUE_LIMIT;

    /// `_TABLE_ADDITION_LOWER_ENTRIES`.
    pub const TABLE_ADDITION_LOWER_ENTRIES: usize = 1;
    /// `_TABLE_ADDITION_UPPER_ENTRIES`.
    pub const TABLE_ADDITION_UPPER_ENTRIES: usize = 2;
    /// `base_index`.
    pub const BASE_INDEX: usize = TABLE_ADDITION_LOWER_ENTRIES;
    /// `nominal_size`.
    pub const NOMINAL_SIZE: usize = 360;
    /// `total_size`.
    pub const TOTAL_SIZE: usize =
        NOMINAL_SIZE + TABLE_ADDITION_LOWER_ENTRIES + TABLE_ADDITION_UPPER_ENTRIES;

    /// `lower_wrap_index`.
    pub const LOWER_WRAP_INDEX: usize = 0;
    /// `upper_wrap_index`.
    pub const UPPER_WRAP_INDEX: usize = BASE_INDEX + NOMINAL_SIZE;
    /// `first_nominal_index`.
    pub const FIRST_NOMINAL_INDEX: usize = BASE_INDEX;
    /// `last_nominal_index`.
    pub const LAST_NOMINAL_INDEX: usize = UPPER_WRAP_INDEX - 1;

    /// The hue of position `i_lo`: the position itself, the table being in degrees.
    ///
    /// Port of `TableBase::base_hue_for_position` (Common.h:61-68 @ v2.5.2).
    #[inline]
    pub fn base_hue_for_position(i_lo: u32) -> f32 {
        if HUE_LIMIT == NOMINAL_SIZE as f32 {
            return i_lo as f32;
        }
        i_lo as f32 * HUE_LIMIT / NOMINAL_SIZE as f32
    }

    /// The position of a wrapped hue: its integer part (`static_cast<unsigned int>`).
    ///
    /// Port of `TableBase::hue_position_in_uniform_table` (Common.h:70-76 @ v2.5.2).
    #[inline]
    pub fn hue_position_in_uniform_table(wrapped_hue: f32) -> u32 {
        if HUE_LIMIT == NOMINAL_SIZE as f32 {
            return super::f32_to_u32(wrapped_hue);
        }
        super::f32_to_u32(wrapped_hue / HUE_LIMIT * NOMINAL_SIZE as f32)
    }

    /// [`FIRST_NOMINAL_INDEX`] plus [`hue_position_in_uniform_table`].
    ///
    /// Port of `TableBase::nominal_hue_position_in_uniform_table` (Common.h:78-81 @ v2.5.2).
    #[inline]
    pub fn nominal_hue_position_in_uniform_table(wrapped_hue: f32) -> u32 {
        (FIRST_NOMINAL_INDEX as u32).wrapping_add(hue_position_in_uniform_table(wrapped_hue))
    }
}

/// `static_cast<unsigned int>(x)` as x86-64 compilers emit it: `cvttss2si` into a 64-bit
/// register, whose low 32 bits are the result. The conversion truncates toward zero, and gives
/// the "integer indefinite" `i64::MIN` for NaN and values outside the `i64` range.
#[inline]
pub fn f32_to_u32(x: f32) -> u32 {
    const LIMIT: f32 = 9_223_372_036_854_775_808.0; // 2^63
    let wide = if x.is_nan() || x >= LIMIT || x < -LIMIT {
        i64::MIN
    } else {
        x as i64
    };
    wide as u32
}

/// `Table3D`: three values per hue position.
pub type Table3D = [[f32; 3]; table_base::TOTAL_SIZE];

/// `Table1D`: one value per hue position.
pub type Table1D = [f32; table_base::TOTAL_SIZE];

/// The matrices and constants of the CAM16-based JMh model for a set of primaries.
///
/// Port of `JMhParams` (src/OpenColorIO/ops/fixedfunction/ACES2/Common.h:92-103 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct JMhParams {
    /// `MATRIX_RGB_to_CAM16_c`.
    pub matrix_rgb_to_cam16_c: M33f,
    /// `MATRIX_CAM16_c_to_RGB`.
    pub matrix_cam16_c_to_rgb: M33f,
    /// `MATRIX_cone_response_to_Aab`.
    pub matrix_cone_response_to_aab: M33f,
    /// `MATRIX_Aab_to_cone_response`.
    pub matrix_aab_to_cone_response: M33f,
    /// `F_L_n`: F_L normalised.
    pub f_l_n: f32,
    /// `cz`.
    pub cz: f32,
    /// `inv_cz`: 1/cz.
    pub inv_cz: f32,
    /// `A_w_J`.
    pub a_w_j: f32,
    /// `inv_A_w_J`: 1/A_w_J.
    pub inv_a_w_j: f32,
}

/// The tone scale's parameters for a peak luminance.
///
/// Port of `ToneScaleParams` (src/OpenColorIO/ops/fixedfunction/ACES2/Common.h:105-118 @
/// v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ToneScaleParams {
    /// `n`.
    pub n: f32,
    /// `n_r`.
    pub n_r: f32,
    /// `g`.
    pub g: f32,
    /// `t_1`.
    pub t_1: f32,
    /// `c_t`.
    pub c_t: f32,
    /// `s_2`.
    pub s_2: f32,
    /// `u_2`.
    pub u_2: f32,
    /// `m_2`.
    pub m_2: f32,
    /// `forward_limit`.
    pub forward_limit: f32,
    /// `inverse_limit`.
    pub inverse_limit: f32,
    /// `log_peak`.
    pub log_peak: f32,
}

/// The parameters chroma and gamut compression share.
///
/// Port of `SharedCompressionParameters` (src/OpenColorIO/ops/fixedfunction/ACES2/
/// Common.h:120-125 @ v2.5.2).
#[derive(Debug, Clone, PartialEq)]
pub struct SharedCompressionParameters {
    /// `limit_J_max`.
    pub limit_j_max: f32,
    /// `model_gamma_inv`.
    pub model_gamma_inv: f32,
    /// `reach_m_table`.
    pub reach_m_table: Table1D,
}

/// [`SharedCompressionParameters`] at one hue.
///
/// Port of `ResolvedSharedCompressionParameters` (src/OpenColorIO/ops/fixedfunction/ACES2/
/// Common.h:127-132 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ResolvedSharedCompressionParameters {
    /// `limit_J_max`.
    pub limit_j_max: f32,
    /// `model_gamma_inv`.
    pub model_gamma_inv: f32,
    /// `reachMaxM`.
    pub reach_max_m: f32,
}

/// The chroma compression's parameters.
///
/// Port of `ChromaCompressParams` (src/OpenColorIO/ops/fixedfunction/ACES2/Common.h:134-141 @
/// v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ChromaCompressParams {
    /// `sat`.
    pub sat: f32,
    /// `sat_thr`.
    pub sat_thr: f32,
    /// `compr`.
    pub compr: f32,
    /// `chroma_compress_scale`.
    pub chroma_compress_scale: f32,
}

impl ChromaCompressParams {
    /// `cusp_mid_blend`.
    pub const CUSP_MID_BLEND: f32 = 1.3;
}

/// The gamut compression's parameters at one hue.
///
/// Port of `HueDependantGamutParams` (src/OpenColorIO/ops/fixedfunction/ACES2/Common.h:143-150
/// @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HueDependantGamutParams {
    /// `gamma_bottom_inv`.
    pub gamma_bottom_inv: f32,
    /// `JMcusp`.
    pub jm_cusp: F2,
    /// `gamma_top_inv`.
    pub gamma_top_inv: f32,
    /// `focusJ`.
    pub focus_j: f32,
    /// `analytical_threshold`.
    pub analytical_threshold: f32,
}

/// The gamut compression's parameters.
///
/// Port of `GamutCompressParams` (src/OpenColorIO/ops/fixedfunction/ACES2/Common.h:151-159 @
/// v2.5.2).
#[derive(Debug, Clone, PartialEq)]
pub struct GamutCompressParams {
    /// `mid_J`.
    pub mid_j: f32,
    /// `focus_dist`.
    pub focus_dist: f32,
    /// `lower_hull_gamma_inv`.
    pub lower_hull_gamma_inv: f32,
    /// `hue_linearity_search_range`.
    pub hue_linearity_search_range: [i32; 2],
    /// `hue_table`.
    pub hue_table: Table1D,
    /// `gamut_cusp_table`.
    pub gamut_cusp_table: Table3D,
}

// CAM
/// `reference_luminance`.
pub const REFERENCE_LUMINANCE: f32 = 100.0;
/// `L_A`.
pub const L_A: f32 = 100.0;
/// `Y_b`.
pub const Y_B: f32 = 20.0;
/// `surround`: dim surround.
pub const SURROUND: F3 = [0.9, 0.59, 0.9];

/// `J_scale`.
pub const J_SCALE: f32 = 100.0;
/// `cam_nl_Y_reference`.
pub const CAM_NL_Y_REFERENCE: f32 = 100.0;
/// `cam_nl_offset`.
pub const CAM_NL_OFFSET: f32 = 0.2713 * CAM_NL_Y_REFERENCE;
/// `cam_nl_scale`.
pub const CAM_NL_SCALE: f32 = 4.0 * CAM_NL_Y_REFERENCE;

// Chroma compression
/// `chroma_compress`.
pub const CHROMA_COMPRESS: f32 = 2.4;
/// `chroma_compress_fact`.
pub const CHROMA_COMPRESS_FACT: f32 = 3.3;
/// `chroma_expand`.
pub const CHROMA_EXPAND: f32 = 1.3;
/// `chroma_expand_fact`.
pub const CHROMA_EXPAND_FACT: f32 = 0.69;
/// `chroma_expand_thr`.
pub const CHROMA_EXPAND_THR: f32 = 0.5;

// Gamut compression
/// `smooth_cusps`.
pub const SMOOTH_CUSPS: f32 = 0.12;
/// `smooth_m`.
pub const SMOOTH_M: f32 = 0.27;
/// `cusp_mid_blend`.
pub const CUSP_MID_BLEND: f32 = 1.3;
/// `focus_gain_blend`.
pub const FOCUS_GAIN_BLEND: f32 = 0.3;
/// `focus_adjust_gain_inv`.
pub const FOCUS_ADJUST_GAIN_INV: f32 = 1.0 / 0.55;
/// `focus_distance`.
pub const FOCUS_DISTANCE: f32 = 1.35;
/// `focus_distance_scaling`.
pub const FOCUS_DISTANCE_SCALING: f32 = 1.75;
/// `compression_threshold`.
pub const COMPRESSION_THRESHOLD: f32 = 0.75;

/// The CAM16 primaries.
///
/// Port of `CAM16` (src/OpenColorIO/ops/fixedfunction/ACES2/Common.h:189-197 @ v2.5.2).
pub mod cam16 {
    use super::{Chromaticities, Primaries};
    /// `CAM16::primaries`.
    pub const PRIMARIES: Primaries = Primaries::new(
        Chromaticities::new(0.8336, 0.1735),
        Chromaticities::new(2.3854, -1.4659),
        Chromaticities::new(0.087, -0.125),
        Chromaticities::new(0.333, 0.333),
    );
}

// Table generation
/// `gammaMinimum`.
pub const GAMMA_MINIMUM: f32 = 0.0;
/// `gammaMaximum`.
pub const GAMMA_MAXIMUM: f32 = 5.0;
/// `gammaSearchStep`.
pub const GAMMA_SEARCH_STEP: f32 = 0.4;
/// `gammaAccuracy`.
pub const GAMMA_ACCURACY: f32 = 1e-5;

/// `cuspCornerCount`.
pub const CUSP_CORNER_COUNT: i32 = 6;
/// `totalCornerCount`.
pub const TOTAL_CORNER_COUNT: i32 = CUSP_CORNER_COUNT + 2;
/// `max_sorted_corners`.
pub const MAX_SORTED_CORNERS: i32 = 2 * CUSP_CORNER_COUNT;
/// `reach_cusp_tolerance`.
pub const REACH_CUSP_TOLERANCE: f32 = 1e-3;
/// `display_cusp_tolerance`.
pub const DISPLAY_CUSP_TOLERANCE: f32 = 1e-7;
