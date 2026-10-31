// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/cmsgamma.c` @ lcms2.17: tone curves.
//!
//! Types reached: the parametric types 1 to 5 and their reverses (the `para` and `curv` types of
//! a profile, `cmsReverseToneCurve` on them), 6 to 8 forward and sampled segments (the segmented
//! curves of a multi-process element). Types 108 and 109, and the other reverses, are only
//! reached through plug-ins or user calls, which the tool doesn't make; a segment of such a
//! type evaluates to 0, as the default of the C switch.

use super::cmserr::cms_signal_error;
use super::cmsintrp::{InterpParams, cms_compute_interp_params};
use super::types::*;

/// `MINUS_INF` (src/cmsgamma.c:43 @ lcms2.17).
pub const MINUS_INF: f32 = -1E22f32;
/// `PLUS_INF` (src/cmsgamma.c:44 @ lcms2.17).
pub const PLUS_INF: f32 = 1E22f32;

/// `cmsCurveSegment`.
#[derive(Debug, Clone)]
pub struct CurveSegment {
    pub x0: f32,
    pub x1: f32,
    /// Parametric type. 0 means a sampled segment.
    pub type_: i32,
    pub params: [f64; 10],
    pub n_grid_points: u32,
    pub sampled_points: Option<Vec<f32>>,
}

impl Default for CurveSegment {
    fn default() -> Self {
        CurveSegment {
            x0: 0.0,
            x1: 0.0,
            type_: 0,
            params: [0.0; 10],
            n_grid_points: 0,
            sampled_points: None,
        }
    }
}

/// `cmsToneCurve`.
#[derive(Debug, Clone)]
pub struct ToneCurve {
    interp_params: InterpParams,
    pub segments: Vec<CurveSegment>,
    seg_interp: Vec<Option<InterpParams>>,
    pub table16: Vec<u16>,
}

/// The parameters per type of `DefaultCurves` (src/cmsgamma.c:61-67 @ lcms2.17). `None`: no such
/// curve type.
fn parameter_count(type_: i32) -> Option<usize> {
    match type_.abs() {
        1 => Some(1),
        2 => Some(3),
        3 => Some(4),
        4 => Some(5),
        5 => Some(7),
        6 => Some(4),
        7 => Some(5),
        8 => Some(5),
        108 => Some(1),
        109 => Some(1),
        _ => None,
    }
}

/// Port of `AllocateToneCurveStruct` (src/cmsgamma.c:212-313 @ lcms2.17).
fn allocate_tone_curve_struct(
    n_entries: u32,
    segments: &[CurveSegment],
    values: Option<&[u16]>,
) -> Option<ToneCurve> {
    let n_segments = segments.len() as u32;

    // We allow huge tables, which are then restricted for smoothing operations
    if n_entries > 65530 {
        cms_signal_error(
            CMS_ERROR_RANGE,
            b"Couldn't create tone curve of more than 65530 entries",
        );
        return None;
    }

    if n_entries == 0 && n_segments == 0 {
        cms_signal_error(
            CMS_ERROR_RANGE,
            b"Couldn't create tone curve with zero segments and no table",
        );
        return None;
    }

    // This 16-bit table contains a limited precision representation of the whole curve and is
    // kept for increasing xput on certain operations.
    let mut table16 = vec![0u16; n_entries as usize];

    // Initialize members if requested
    if let Some(values) = values
        && n_entries > 0
    {
        table16.copy_from_slice(&values[..n_entries as usize]);
    }

    // Initialize the segments stuff. The evaluator for each segment is located and a pointer to
    // it is placed in advance to maximize performance.
    let mut seg_interp = Vec::with_capacity(segments.len());
    let mut kept = Vec::with_capacity(segments.len());
    for segment in segments {
        // Type 0 is a special marker for table-based curves
        seg_interp.push(if segment.type_ == 0 {
            cms_compute_interp_params(segment.n_grid_points, 1, 1, CMS_LERP_FLAGS_FLOAT)
        } else {
            None
        });

        let mut segment = segment.clone();
        if segment.type_ != 0 {
            segment.sampled_points = None;
        }
        kept.push(segment);
    }

    let interp_params = cms_compute_interp_params(n_entries, 1, 1, CMS_LERP_FLAGS_16BITS)?;

    Some(ToneCurve {
        interp_params,
        segments: kept,
        seg_interp,
        table16,
    })
}

/// Port of `DefaultEvalParametricFn` (src/cmsgamma.c:343-720 @ lcms2.17).
fn default_eval_parametric_fn(type_: i32, params: &[f64; 10], r: f64) -> f64 {
    let plus_inf = PLUS_INF as f64;
    let val;
    match type_ {
        // X = Y ^ Gamma
        1 => {
            if r < 0.0 {
                if (params[0] - 1.0).abs() < MATRIX_DET_TOLERANCE {
                    val = r;
                } else {
                    val = 0.0;
                }
            } else {
                val = r.powf(params[0]);
            }
        }

        // Type 1 Reversed: X = Y ^1/gamma
        -1 => {
            if r < 0.0 {
                if (params[0] - 1.0).abs() < MATRIX_DET_TOLERANCE {
                    val = r;
                } else {
                    val = 0.0;
                }
            } else if params[0].abs() < MATRIX_DET_TOLERANCE {
                val = plus_inf;
            } else {
                val = r.powf(1.0 / params[0]);
            }
        }

        // CIE 122-1966
        // Y = (aX + b)^Gamma  | X >= -b/a
        // Y = 0               | else
        2 => {
            if params[1].abs() < MATRIX_DET_TOLERANCE {
                val = 0.0;
            } else {
                let disc = -params[2] / params[1];

                if r >= disc {
                    let e = params[1] * r + params[2];

                    if e > 0.0 {
                        val = e.powf(params[0]);
                    } else {
                        val = 0.0;
                    }
                } else {
                    val = 0.0;
                }
            }
        }

        // Type 2 Reversed
        // X = (Y ^1/g  - b) / a
        -2 => {
            if params[0].abs() < MATRIX_DET_TOLERANCE || params[1].abs() < MATRIX_DET_TOLERANCE {
                val = 0.0;
            } else {
                let mut v = if r < 0.0 {
                    0.0
                } else {
                    (r.powf(1.0 / params[0]) - params[2]) / params[1]
                };

                if v < 0.0 {
                    v = 0.0;
                }
                val = v;
            }
        }

        // IEC 61966-3
        // Y = (aX + b)^Gamma + c | X <= -b/a
        // Y = c                  | else
        3 => {
            if params[1].abs() < MATRIX_DET_TOLERANCE {
                val = 0.0;
            } else {
                let mut disc = -params[2] / params[1];
                if disc < 0.0 {
                    disc = 0.0;
                }

                if r >= disc {
                    let e = params[1] * r + params[2];

                    if e > 0.0 {
                        val = e.powf(params[0]) + params[3];
                    } else {
                        val = 0.0;
                    }
                } else {
                    val = params[3];
                }
            }
        }

        // Type 3 reversed
        // X=((Y-c)^1/g - b)/a      | (Y>=c)
        // X=-b/a                   | (Y<c)
        -3 => {
            if params[0].abs() < MATRIX_DET_TOLERANCE || params[1].abs() < MATRIX_DET_TOLERANCE {
                val = 0.0;
            } else if r >= params[3] {
                let e = r - params[3];

                if e > 0.0 {
                    val = (e.powf(1.0 / params[0]) - params[2]) / params[1];
                } else {
                    val = 0.0;
                }
            } else {
                val = -params[2] / params[1];
            }
        }

        // IEC 61966-2.1 (sRGB)
        // Y = (aX + b)^Gamma | X >= d
        // Y = cX             | X < d
        4 => {
            if r >= params[4] {
                let e = params[1] * r + params[2];

                if e > 0.0 {
                    val = e.powf(params[0]);
                } else {
                    val = 0.0;
                }
            } else {
                val = r * params[3];
            }
        }

        // Type 4 reversed
        // X=((Y^1/g-b)/a)    | Y >= (ad+b)^g
        // X=Y/c              | Y< (ad+b)^g
        -4 => {
            let e = params[1] * params[4] + params[2];
            let disc = if e < 0.0 { 0.0 } else { e.powf(params[0]) };

            if r >= disc {
                if params[0].abs() < MATRIX_DET_TOLERANCE || params[1].abs() < MATRIX_DET_TOLERANCE
                {
                    val = 0.0;
                } else {
                    val = (r.powf(1.0 / params[0]) - params[2]) / params[1];
                }
            } else if params[3].abs() < MATRIX_DET_TOLERANCE {
                val = 0.0;
            } else {
                val = r / params[3];
            }
        }

        // Y = (aX + b)^Gamma + e | X >= d
        // Y = cX + f             | X < d
        5 => {
            if r >= params[4] {
                let e = params[1] * r + params[2];

                if e > 0.0 {
                    val = e.powf(params[0]) + params[5];
                } else {
                    val = params[5];
                }
            } else {
                val = r * params[3] + params[6];
            }
        }

        // Reversed type 5
        // X=((Y-e)1/g-b)/a   | Y >=(ad+b)^g+e), cd+f
        // X=(Y-f)/c          | else
        -5 => {
            let disc = params[3] * params[4] + params[6];
            if r >= disc {
                let e = r - params[5];
                if e < 0.0 {
                    val = 0.0;
                } else if params[0].abs() < MATRIX_DET_TOLERANCE
                    || params[1].abs() < MATRIX_DET_TOLERANCE
                {
                    val = 0.0;
                } else {
                    val = (e.powf(1.0 / params[0]) - params[2]) / params[1];
                }
            } else if params[3].abs() < MATRIX_DET_TOLERANCE {
                val = 0.0;
            } else {
                val = (r - params[6]) / params[3];
            }
        }

        // Types 6,7,8 comes from segmented curves as described in ICCSpecRevision_02_11_06_Float.pdf
        // Type 6 is basically identical to type 5 without d

        // Y = (a * X + b) ^ Gamma + c
        6 => {
            let e = params[1] * r + params[2];

            // On gamma 1.0, don't clamp
            if params[0] == 1.0 {
                val = e + params[3];
            } else if e < 0.0 {
                val = params[3];
            } else {
                val = e.powf(params[0]) + params[3];
            }
        }

        // Y = a * log (b * X^Gamma + c) + d
        7 => {
            let e = params[2] * r.powf(params[0]) + params[3];
            if e <= 0.0 {
                val = params[4];
            } else {
                val = params[1] * e.log10() + params[4];
            }
        }

        //Y = a * b^(c*X+d) + e
        8 => {
            val = params[0] * params[1].powf(params[2] * r + params[3]) + params[4];
        }

        // Unsupported parametric curve. Should never reach here
        _ => return 0.0,
    }

    val
}

/// Port of `EvalSegmentedFn` (src/cmsgamma.c:724-765 @ lcms2.17).
fn eval_segmented_fn(g: &ToneCurve, r: f64) -> f64 {
    for i in (0..g.segments.len()).rev() {
        let segment = &g.segments[i];

        // Check for domain
        if r > segment.x0 as f64 && r <= segment.x1 as f64 {
            let out;

            // Type == 0 means segment is sampled
            if segment.type_ == 0 {
                let r1 = (r - segment.x0 as f64) as f32 / (segment.x1 - segment.x0);
                let mut out32 = [0f32];

                let interp = g.seg_interp[i].as_ref();
                let table = segment.sampled_points.as_deref().unwrap_or(&[]);
                if let Some(interp) = interp {
                    interp.lerp_float(table, &[r1], &mut out32);
                }
                out = out32[0] as f64;
            } else {
                out = default_eval_parametric_fn(segment.type_, &segment.params, r);
            }

            if out.is_infinite() {
                // isinf(Out) is true for both signs, and the first test takes them both.
                return PLUS_INF as f64;
            }

            return out;
        }
    }

    MINUS_INF as f64
}

/// Port of `cmsBuildTabulatedToneCurve16` (src/cmsgamma.c:783-786 @ lcms2.17).
pub fn cms_build_tabulated_tone_curve16(
    n_entries: u32,
    values: Option<&[u16]>,
) -> Option<ToneCurve> {
    allocate_tone_curve_struct(n_entries, &[], values)
}

/// Port of `EntriesByGamma` (src/cmsgamma.c:788-793 @ lcms2.17).
fn entries_by_gamma(gamma: f64) -> u32 {
    if (gamma - 1.0).abs() < 0.001 {
        return 2;
    }
    4096
}

/// Port of `cmsBuildSegmentedToneCurve` (src/cmsgamma.c:797-829 @ lcms2.17).
pub fn cms_build_segmented_tone_curve(segments: &[CurveSegment]) -> Option<ToneCurve> {
    let mut n_grid_points = 4096;

    // Optimizatin for identity curves.
    if segments.len() == 1 && segments[0].type_ == 1 {
        n_grid_points = entries_by_gamma(segments[0].params[0]);
    }

    let mut g = allocate_tone_curve_struct(n_grid_points, segments, None)?;

    // Once we have the floating point version, we can approximate a 16 bit table of 4096 entries
    // for performance reasons. This table would normally not be used except on 8/16 bits
    // transforms.
    for i in 0..n_grid_points {
        let r = i as f64 / (n_grid_points - 1) as f64;

        let val = eval_segmented_fn(&g, r);

        // Round and saturate
        g.table16[i as usize] = cms_quick_saturate_word(val * 65535.0);
    }

    Some(g)
}

/// Port of `cmsBuildParametricToneCurve` (src/cmsgamma.c:880-904 @ lcms2.17).
pub fn cms_build_parametric_tone_curve(type_: i32, params: &[f64]) -> Option<ToneCurve> {
    let Some(count) = parameter_count(type_) else {
        cms_signal_error(
            CMS_ERROR_UNKNOWN_EXTENSION,
            format!("Invalid parametric curve type {type_}").as_bytes(),
        );
        return None;
    };

    let mut seg0 = CurveSegment {
        x0: MINUS_INF,
        x1: PLUS_INF,
        type_,
        ..Default::default()
    };
    seg0.params[..count].copy_from_slice(&params[..count]);

    cms_build_segmented_tone_curve(&[seg0])
}

/// Port of `cmsBuildGamma` (src/cmsgamma.c:909-912 @ lcms2.17).
pub fn cms_build_gamma(gamma: f64) -> Option<ToneCurve> {
    cms_build_parametric_tone_curve(1, &[gamma])
}

/// Port of `GetInterval` (src/cmsgamma.c:1022-1067 @ lcms2.17).
fn get_interval(in_: f64, lut_table: &[u16], p: &InterpParams) -> i32 {
    let domain = p.domain[0] as i32;

    // A 1 point table is not allowed
    if p.domain[0] < 1 {
        return -1;
    }

    let found = |i: i32| {
        let y0 = lut_table[i as usize] as i32;
        let y1 = lut_table[i as usize + 1] as i32;

        if y0 <= y1 {
            // Increasing
            in_ >= y0 as f64 && in_ <= y1 as f64
        } else {
            // Decreasing
            in_ >= y1 as f64 && in_ <= y0 as f64
        }
    };

    // Let's see if ascending or descending.
    if lut_table[0] < lut_table[p.domain[0] as usize] {
        // Table is overall ascending
        for i in (0..domain).rev() {
            if found(i) {
                return i;
            }
        }
    } else {
        // Table is overall descending
        for i in 0..domain {
            if found(i) {
                return i;
            }
        }
    }

    -1
}

/// Port of `cmsReverseToneCurveEx` (src/cmsgamma.c:1070-1134 @ lcms2.17).
pub fn cms_reverse_tone_curve_ex(n_result_samples: u32, in_curve: &ToneCurve) -> Option<ToneCurve> {
    // Try to reverse it analytically whatever possible

    if in_curve.segments.len() == 1
        && in_curve.segments[0].type_ > 0
        && parameter_count(in_curve.segments[0].type_).is_some()
    {
        return cms_build_parametric_tone_curve(
            -in_curve.segments[0].type_,
            &in_curve.segments[0].params,
        );
    }

    // Nope, reverse the table.
    let mut out = cms_build_tabulated_tone_curve16(n_result_samples, None)?;

    // We want to know if this is an ascending or descending table
    let ascending = !cms_is_tone_curve_descending(in_curve);

    let n_entries = in_curve.table16.len() as u32;
    let (mut a, mut b) = (0f64, 0f64);

    // Iterate across Y axis
    for i in 0..n_result_samples as i32 {
        let y = i as f64 * 65535.0 / (n_result_samples - 1) as f64;

        // Find interval in which y is within.
        let j = get_interval(y, &in_curve.table16, &in_curve.interp_params);
        if j >= 0 {
            // Get limits of interval
            let x1 = in_curve.table16[j as usize] as f64;
            let x2 = in_curve.table16[j as usize + 1] as f64;

            let y1 = (j as f64 * 65535.0) / (n_entries - 1) as f64;
            let y2 = ((j + 1) as f64 * 65535.0) / (n_entries - 1) as f64;

            // If collapsed, then use any
            if x1 == x2 {
                out.table16[i as usize] = cms_quick_saturate_word(if ascending { y2 } else { y1 });
                continue;
            } else {
                // Interpolate
                a = (y2 - y1) / (x2 - x1);
                b = y2 - a * x2;
            }
        }

        out.table16[i as usize] = cms_quick_saturate_word(a * y + b);
    }

    Some(out)
}

/// Port of `cmsReverseToneCurve` (src/cmsgamma.c:1137-1142 @ lcms2.17).
pub fn cms_reverse_tone_curve(in_gamma: &ToneCurve) -> Option<ToneCurve> {
    cms_reverse_tone_curve_ex(4096, in_gamma)
}

/// Port of `cmsIsToneCurveDescending` (src/cmsgamma.c:1392-1397 @ lcms2.17).
pub fn cms_is_tone_curve_descending(t: &ToneCurve) -> bool {
    t.table16[0] > t.table16[t.table16.len() - 1]
}

/// Port of `cmsEvalToneCurveFloat` (src/cmsgamma.c:1417-1433 @ lcms2.17).
#[inline]
pub fn cms_eval_tone_curve_float(curve: &ToneCurve, v: f32) -> f32 {
    // Check for 16 bits table. If so, this is a limited-precision tone curve
    if curve.segments.is_empty() {
        let in_ = cms_quick_saturate_word(v as f64 * 65535.0);
        let out = cms_eval_tone_curve16(curve, in_);

        return (out as f64 / 65535.0) as f32;
    }

    eval_segmented_fn(curve, v as f64) as f32
}

/// Port of `cmsEvalToneCurve16` (src/cmsgamma.c:1436-1444 @ lcms2.17).
#[inline]
pub fn cms_eval_tone_curve16(curve: &ToneCurve, v: u16) -> u16 {
    let mut out = [0u16];
    curve.interp_params.lerp16(&curve.table16, &[v], &mut out);
    out[0]
}

#[cfg(test)]
#[path = "cmsgamma_tests.rs"]
mod tests;
