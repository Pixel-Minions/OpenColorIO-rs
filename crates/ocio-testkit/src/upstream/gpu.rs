// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! GPU-suite comparisons only: `tests/gpu/GPUUnitTest.cpp:26-124,355-526` @ v2.5.2.
//! These are upstream's tolerances and exceptional-value rules, not oracle comparators.

use sha2::{Digest, Sha256};

/// The fixed image dimensions (`GPUUnitTest.cpp:200-202` @ v2.5.2).
pub const WIDTH: usize = 256;
/// The fixed image height.
pub const HEIGHT: usize = 256;
/// Upstream always uses RGBA.
pub const COMPONENTS: usize = 4;
/// The padded image's float count.
pub const ENTRIES: usize = WIDTH * HEIGHT * COMPONENTS;

/// `Shader::LimitsDiff` (`GPUUnitTest.cpp:38-43` @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitsDiff {
    /// Neither value needs the exceptional-value check.
    NotApplicable,
    /// The exceptional values disagree.
    Inconsistent,
    /// The exceptional values agree under upstream's rule.
    Consistent,
}

/// Port of `Shader::ValidateInf` (`GPUUnitTest.cpp:44-62` @ v2.5.2).
pub fn validate_inf(x1: f32, x2: f32) -> LimitsDiff {
    let large = f32::MAX;
    if (x1.abs() < large && x2.abs() < large) || x1.is_nan() || x2.is_nan() {
        return LimitsDiff::NotApplicable;
    }
    if (x1 >= large && x2 < large)
        || (x1 < large && x2 >= large)
        || (x1 <= -large && x2 > -large)
        || (x1 > -large && x2 <= -large)
    {
        LimitsDiff::Inconsistent
    } else {
        LimitsDiff::Consistent
    }
}

/// Port of `Shader::ValidateNan` (`GPUUnitTest.cpp:64-89` @ v2.5.2).
pub fn validate_nan(x1: f32, x2: f32) -> LimitsDiff {
    match (x1.is_nan(), x2.is_nan()) {
        (false, false) => LimitsDiff::NotApplicable,
        (true, true) => LimitsDiff::Consistent,
        _ => LimitsDiff::Inconsistent,
    }
}

/// Port of `AbsoluteDifference` (`GPUUnitTest.cpp:91-100` @ v2.5.2).
pub fn absolute_difference(x1: f32, x2: f32, diff: &mut f32) -> bool {
    let current = (x2 - x1).abs();
    if current > *diff {
        *diff = current;
        true
    } else {
        false
    }
}

/// Port of `RelativeDifference` (`GPUUnitTest.cpp:102-113` @ v2.5.2).
pub fn relative_difference(x1: f32, x2: f32, min_x1: f32, diff: &mut f32) -> bool {
    let abs_x1 = x1.abs();
    // std::max(a,b) returns a when its comparison is false, including an unordered b.
    let divisor = if abs_x1 < min_x1 { min_x1 } else { abs_x1 };
    let current = (x1 - x2).abs() / divisor;
    if current > *diff {
        *diff = current;
        true
    } else {
        false
    }
}

/// Port of `ComputeDiff` (`GPUUnitTest.cpp:115-124` @ v2.5.2).
pub fn compute_diff(x1: f32, x2: f32, relative: bool, min_x1: f32, diff: &mut f32) -> bool {
    if relative {
        relative_difference(x1, x2, min_x1, diff)
    } else {
        absolute_difference(x1, x2, diff)
    }
}

/// Original input dimensions, including the partially padded last row. Port of
/// `ValidateImageTexture` (`GPUUnitTest.cpp:396-420` @ v2.5.2).
pub fn comparison_dimensions(original_entries: usize) -> Result<(usize, usize), &'static str> {
    let pixels = original_entries / COMPONENTS;
    let (width, height) = if pixels <= WIDTH {
        (pixels, 1)
    } else {
        (WIDTH, pixels.div_ceil(WIDTH))
    };
    if width == 0 || width > WIDTH || height == 0 || height > HEIGHT {
        return Err("Mismatch with the expected image size");
    }
    Ok((width, height))
}

/// Settings from `OCIOGPUTest` (`GPUUnitTest.h:157-173` @ v2.5.2).
#[derive(Debug, Clone, Copy)]
pub struct ComparisonSettings {
    /// Absolute/relative error threshold, default `1e-7f`.
    pub error_threshold: f32,
    /// Lower bound of the CPU-value relative denominator, default `1e-6f`.
    pub expected_minimal_value: f32,
    /// Relative rather than absolute comparison.
    pub relative: bool,
    /// Upstream's diagnostic switch deliberately fails a test.
    pub print_min_max: bool,
}

impl Default for ComparisonSettings {
    fn default() -> Self {
        Self {
            error_threshold: 1e-7,
            expected_minimal_value: 1e-6,
            relative: false,
            print_min_max: false,
        }
    }
}

/// All failures survive the component scan. Upstream keeps the last NaN/Inf
/// failure and the first component attaining a new largest error.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageComparison {
    /// Largest numeric error, computed entirely in f32.
    pub max_diff: f32,
    /// Component at the last strict increase of max_diff.
    pub index_diff: Option<usize>,
    /// Last inconsistent infinity/FLT_MAX component.
    pub index_inf: Option<usize>,
    /// Last inconsistent NaN component.
    pub index_nan: Option<usize>,
    /// GPU minima, ignoring infinity and unordered NaN comparisons.
    pub min_values: [f32; COMPONENTS],
    /// GPU maxima, with the same exceptional-value rules.
    pub max_values: [f32; COMPONENTS],
}

impl ImageComparison {
    /// Port of the failure condition (`GPUUnitTest.cpp:469` @ v2.5.2).
    pub fn passes(&self, settings: ComparisonSettings) -> bool {
        !(self.max_diff > settings.error_threshold
            || self.index_inf.is_some()
            || self.index_nan.is_some()
            || settings.print_min_max)
    }
}

/// Port of `DiffComponent` and the scan in `ValidateImageTexture`
/// (`GPUUnitTest.cpp:355-383,435-467` @ v2.5.2). `entries` is the entire
/// comparison rectangle, including the last row's padding, not just custom values.
pub fn compare_image(
    cpu: &[f32],
    gpu: &[f32],
    entries: usize,
    settings: ComparisonSettings,
) -> ImageComparison {
    assert!(entries > 0 && entries.is_multiple_of(COMPONENTS));
    assert!(cpu.len() >= entries && gpu.len() >= entries);
    let mut result = ImageComparison {
        max_diff: 0.0,
        index_diff: None,
        index_inf: None,
        index_nan: None,
        min_values: [f32::MAX; COMPONENTS],
        max_values: [-f32::MAX; COMPONENTS],
    };
    for index in 0..entries {
        let (x1, x2) = (cpu[index], gpu[index]);
        match validate_inf(x1, x2) {
            LimitsDiff::Inconsistent => result.index_inf = Some(index),
            LimitsDiff::Consistent => {}
            LimitsDiff::NotApplicable => match validate_nan(x1, x2) {
                LimitsDiff::Inconsistent => result.index_nan = Some(index),
                LimitsDiff::Consistent => {}
                LimitsDiff::NotApplicable => {
                    if compute_diff(
                        x1,
                        x2,
                        settings.relative,
                        settings.expected_minimal_value,
                        &mut result.max_diff,
                    ) {
                        result.index_diff = Some(index);
                    }
                }
            },
        }
        let channel = index % COMPONENTS;
        let min = if x2.is_infinite() { f32::MAX } else { x2 };
        let max = if x2.is_infinite() { -f32::MAX } else { x2 };
        if min < result.min_values[channel] {
            result.min_values[channel] = min;
        }
        if result.max_values[channel] < max {
            result.max_values[channel] = max;
        }
    }
    result
}

/// SHA256 of generated input bits only. CLAUDE.md rule 2 permits this guard
/// against an input generator silently changing both sides of a comparison.
pub fn input_digest(values: &[f32]) -> String {
    let mut digest = Sha256::new();
    for value in values {
        digest.update(value.to_le_bytes());
    }
    digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exceptional_values_and_inclusive_tolerance_follow_upstream() {
        // ValidateInf deliberately equates same-sign FLT_MAX and infinity.
        let settings = ComparisonSettings::default();
        let cpu = [f32::MAX, -f32::MAX, f32::NAN, 0.0];
        let gpu = [
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            settings.error_threshold,
        ];
        let comparison = compare_image(&cpu, &gpu, 4, settings);
        assert!(comparison.passes(settings));
        assert_eq!(comparison.index_inf, None);
        assert_eq!(comparison.index_nan, None);
        assert_eq!(comparison.index_diff, Some(3));
        assert_eq!(
            validate_inf(f32::INFINITY, f32::NEG_INFINITY),
            LimitsDiff::Inconsistent
        );
        assert_eq!(
            validate_inf(f32::NAN, f32::INFINITY),
            LimitsDiff::NotApplicable
        );
        assert_eq!(
            validate_nan(f32::NAN, f32::INFINITY),
            LimitsDiff::Inconsistent
        );
        let below_max = f32::from_bits(f32::MAX.to_bits() - 1);
        assert_eq!(
            validate_inf(below_max, f32::INFINITY),
            LimitsDiff::Inconsistent
        );
        assert!(!comparison.passes(ComparisonSettings {
            print_min_max: true,
            ..settings
        }));
    }

    #[test]
    fn every_component_and_partial_last_row_are_compared() {
        let cpu = [f32::NAN, f32::INFINITY, 0.0, 0.0, f32::NAN, 0.0, 0.0, 0.0];
        let gpu = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let settings = ComparisonSettings::default();
        let comparison = compare_image(&cpu, &gpu, 8, settings);
        assert_eq!(comparison.index_nan, Some(4));
        assert_eq!(comparison.index_inf, Some(1));
        assert_eq!(comparison.index_diff, Some(3)); // Equal later errors do not update it.
        assert!(!comparison.passes(settings));
        assert_eq!(comparison_dimensions(257 * COMPONENTS), Ok((WIDTH, 2)));
        assert!(comparison_dimensions(0).is_err());
        assert!(comparison_dimensions(ENTRIES + COMPONENTS).is_err());
    }
}
