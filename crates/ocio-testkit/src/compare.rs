// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Exact comparators with reports that make a mismatch easy to diagnose.
//!
//! Floats compare by bit pattern: `-0.0 != 0.0`, and NaN payloads must match.

use std::fmt::Write as _;

/// How many mismatches a report lists before summarizing.
const MAX_LISTED: usize = 20;

/// Distance in units in the last place between two non-NaN floats, counting `-0.0` and
/// `0.0` as one step apart. For reports, and for the bound of waiver W0001
/// (`pixels_report_within_ulp`); other comparisons are exact.
pub fn ulp_distance(a: f32, b: f32) -> Option<u64> {
    if a.is_nan() || b.is_nan() {
        return None;
    }
    let key = |x: f32| {
        let bits = x.to_bits();
        let magnitude = i64::from(bits & 0x7fff_ffff);
        if bits >> 31 == 1 {
            -magnitude - 1
        } else {
            magnitude
        }
    };
    Some(key(a).abs_diff(key(b)))
}

fn describe_f32(x: f32) -> String {
    format!("{x:e} ({:#010x})", x.to_bits())
}

/// A report of the positions where `actual` differs bitwise from `expected`, or `None`.
///
/// `inputs`, when given, is printed next to each mismatch; `channels` groups values into
/// pixels (1 for plain arrays, 3 for RGB, 4 for RGBA).
pub fn f32_bits_report(
    expected: &[f32],
    actual: &[f32],
    inputs: Option<&[f32]>,
    channels: usize,
) -> Option<String> {
    let channels = channels.max(1);
    let mut report = String::new();
    if expected.len() != actual.len() {
        let _ = writeln!(
            report,
            "length differs: expected {}, actual {}",
            expected.len(),
            actual.len()
        );
    }
    let mut count = 0usize;
    let mut worst: Option<(usize, u64)> = None;
    for (i, (e, a)) in expected.iter().zip(actual).enumerate() {
        if e.to_bits() == a.to_bits() {
            continue;
        }
        count += 1;
        let ulps = ulp_distance(*e, *a);
        if let Some(u) = ulps
            && worst.is_none_or(|(_, w)| u > w)
        {
            worst = Some((i, u));
        }
        if count <= MAX_LISTED {
            let (pixel, channel) = (i / channels, i % channels);
            let _ = write!(
                report,
                "  [{i}] pixel {pixel} ch {channel}: expected {}, actual {}",
                describe_f32(*e),
                describe_f32(*a)
            );
            if let Some(u) = ulps {
                let _ = write!(report, ", {u} ulp");
            }
            if let Some(inputs) = inputs {
                let start = pixel * channels;
                if let Some(px) = inputs.get(start..start + channels) {
                    let px: Vec<_> = px.iter().map(|v| describe_f32(*v)).collect();
                    let _ = write!(report, "; input [{}]", px.join(", "));
                }
            }
            report.push('\n');
        }
    }
    if count == 0 && expected.len() == actual.len() {
        return None;
    }
    let total = expected.len().min(actual.len());
    let mut header = format!("{count} of {total} values differ bitwise");
    if let Some((i, u)) = worst {
        let _ = write!(header, "; largest difference {u} ulp at [{i}]");
    }
    if count > MAX_LISTED {
        let _ = write!(header, "; first {MAX_LISTED} listed");
    }
    Some(format!("{header}\n{report}"))
}

/// Asserts that two `f32` slices are bitwise identical.
#[track_caller]
pub fn assert_f32_bits_eq(label: &str, expected: &[f32], actual: &[f32]) {
    if let Some(report) = f32_bits_report(expected, actual, None, 1) {
        panic!("{label}: {report}");
    }
}

/// Asserts that two pixel buffers are bitwise identical, printing the inputs of mismatches.
#[track_caller]
pub fn assert_pixels_bits_eq(
    label: &str,
    inputs: &[f32],
    channels: usize,
    expected: &[f32],
    actual: &[f32],
) {
    if let Some(report) = f32_bits_report(expected, actual, Some(inputs), channels) {
        panic!("{label}: {report}");
    }
}

/// Asserts that two pixel buffers are bitwise identical, except for the NaN bits that the
/// owner-approved waiver `waiver` (`waivers.toml`) lets differ: in the channels where
/// `waived_channels` is true, a value that is NaN in `expected` only has to be NaN in
/// `actual`, with any sign and payload. Every other value, including every value of those
/// channels that is not NaN in `expected`, must match bit for bit. `waived_channels` has one
/// entry per channel of a pixel.
///
/// The relaxation covers every NaN output of a waived channel, whatever produced it: not only
/// where a NaN parameter meets a NaN pixel, but also, for example, where a finite pixel meets
/// two NaN parameters in turn. Callers waive only the channels (and cases) that need it.
///
/// Returns how many values matched only as NaN, so a test can show what the waiver covered.
#[track_caller]
pub fn assert_pixels_bits_eq_except_nan_bits(
    label: &str,
    waiver: &str,
    waived_channels: &[bool],
    inputs: &[f32],
    expected: &[f32],
    actual: &[f32],
) -> usize {
    assert!(!waived_channels.is_empty(), "{label}: no channels");
    match pixels_report_except_nan_bits(waiver, waived_channels, inputs, expected, actual) {
        Ok(waived) => waived,
        Err(report) => panic!("{label} {report}"),
    }
}

/// The comparison of [`assert_pixels_bits_eq_except_nan_bits`], without the panic: how many
/// values matched only as NaN, or a report that starts with
/// `(NaN bits waived by <waiver> in channels [...]):`.
///
/// Only `battery::params::Case::compare_pixels` and `Case::compare_baked_luts` call it, for the
/// channels of NaN parameters (waiver W0002); a test pins their callers.
pub(crate) fn pixels_report_except_nan_bits(
    waiver: &str,
    waived_channels: &[bool],
    inputs: &[f32],
    expected: &[f32],
    actual: &[f32],
) -> Result<usize, String> {
    assert!(!waived_channels.is_empty(), "no channels");
    let channels = waived_channels.len();
    let mut compared = actual.to_vec();
    let mut waived = 0;
    for (i, (e, a)) in expected.iter().zip(actual).enumerate() {
        if waived_channels[i % channels] && e.is_nan() && a.is_nan() && e.to_bits() != a.to_bits() {
            compared[i] = *e;
            waived += 1;
        }
    }
    match f32_bits_report(expected, &compared, Some(inputs), channels) {
        None => Ok(waived),
        Some(report) => Err(format!(
            "(NaN bits waived by {waiver} in channels {waived_channels:?}): {report}"
        )),
    }
}

/// The comparison of waiver W0001 (`waivers.toml`): `actual` equals `expected` bit for bit,
/// except that a value that is NaN in both may differ in its sign and payload bits, and a value
/// that is finite in both may differ by at most `bound` ulp ([`ulp_distance`]). NaN positions,
/// infinities and every other value compare exactly. How many values differed within the
/// waiver, or a report that starts with `(within <bound> ulp, NaN bits waived, by <waiver>):`.
///
/// Only `battery::params::Case::compare_pixels` calls it, for the cases a family marks with
/// `Case::w0001`; a test pins its callers.
pub(crate) fn pixels_report_within_ulp(
    waiver: &str,
    bound: u64,
    inputs: &[f32],
    expected: &[f32],
    actual: &[f32],
) -> Result<usize, String> {
    let mut compared = actual.to_vec();
    let mut waived = 0;
    for (i, (e, a)) in expected.iter().zip(actual).enumerate() {
        if e.to_bits() == a.to_bits() {
            continue;
        }
        let within = if e.is_nan() || a.is_nan() {
            e.is_nan() && a.is_nan()
        } else {
            e.is_finite() && a.is_finite() && ulp_distance(*e, *a).is_some_and(|d| d <= bound)
        };
        if within {
            compared[i] = *e;
            waived += 1;
        }
    }
    match f32_bits_report(expected, &compared, Some(inputs), 4) {
        None => Ok(waived),
        Some(report) => Err(format!(
            "(within {bound} ulp, NaN bits waived, by {waiver}): {report}"
        )),
    }
}

/// Asserts that two byte strings are identical.
#[track_caller]
pub fn assert_bytes_eq(label: &str, expected: &[u8], actual: &[u8]) {
    if expected == actual {
        return;
    }
    let first = expected
        .iter()
        .zip(actual)
        .position(|(e, a)| e != a)
        .unwrap_or(expected.len().min(actual.len()));
    let window = |bytes: &[u8]| {
        let start = first.saturating_sub(16);
        let end = (first + 16).min(bytes.len());
        format!("{:02x?}", &bytes[start.min(end)..end])
    };
    panic!(
        "{label}: bytes differ at offset {first} (expected {} bytes, actual {})\n  expected ..{}..\n  actual   ..{}..",
        expected.len(),
        actual.len(),
        window(expected),
        window(actual)
    );
}

/// Makes invisible differences visible: `\r`, `\t`, trailing spaces and non-ASCII bytes.
fn visible(line: &str) -> String {
    let trimmed_len = line.trim_end_matches(' ').len();
    let mut out = String::new();
    for (i, c) in line.char_indices() {
        match c {
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ' ' if i >= trimmed_len => out.push('·'),
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            c => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
        }
    }
    out
}

/// Asserts that two texts are byte-identical, reporting the first differing line.
#[track_caller]
pub fn assert_text_eq(label: &str, expected: &str, actual: &str) {
    if expected == actual {
        return;
    }
    let exp: Vec<&str> = expected.split('\n').collect();
    let act: Vec<&str> = actual.split('\n').collect();
    let line = exp
        .iter()
        .zip(&act)
        .position(|(e, a)| e != a)
        .unwrap_or(exp.len().min(act.len()));
    let mut report = format!(
        "{label}: text differs at line {} (expected {} bytes / {} lines, actual {} bytes / {} lines)\n",
        line + 1,
        expected.len(),
        exp.len(),
        actual.len(),
        act.len()
    );
    let from = line.saturating_sub(3);
    for (i, context) in exp.iter().enumerate().take(line).skip(from) {
        let _ = writeln!(report, "    {:>5} | {}", i + 1, visible(context));
    }
    for i in line..(line + 4) {
        if let Some(e) = exp.get(i) {
            let _ = writeln!(report, "  - {:>5} | {}", i + 1, visible(e));
        }
        if let Some(a) = act.get(i) {
            let _ = writeln!(report, "  + {:>5} | {}", i + 1, visible(a));
        }
    }
    panic!("{report}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_is_none() {
        assert!(f32_bits_report(&[1.0, f32::NAN], &[1.0, f32::NAN], None, 1).is_none());
    }

    #[test]
    fn signed_zero_differs() {
        let report = f32_bits_report(&[0.0], &[-0.0], None, 1).expect("a report");
        assert!(
            report.starts_with("1 of 1 values differ bitwise"),
            "{report}"
        );
    }

    #[test]
    fn nan_payload_differs() {
        let a = f32::from_bits(0x7fc0_0000);
        let b = f32::from_bits(0x7fc0_0001);
        assert!(f32_bits_report(&[a], &[b], None, 1).is_some());
    }

    #[test]
    fn ulps() {
        assert_eq!(
            ulp_distance(1.0, f32::from_bits(1.0f32.to_bits() + 3)),
            Some(3)
        );
        assert_eq!(ulp_distance(0.0, -0.0), Some(1));
        assert_eq!(ulp_distance(f32::from_bits(1), -f32::from_bits(1)), Some(3));
    }

    #[test]
    #[should_panic(expected = "text differs at line 2")]
    fn text_diff_line() {
        assert_text_eq("t", "a\nb\nc", "a\nb \nc");
    }

    const NAN_A: u32 = 0x7fc0_0000;
    const NAN_B: u32 = 0xffc1_2345;

    fn pixel(bits: [u32; 4]) -> [f32; 4] {
        bits.map(f32::from_bits)
    }

    #[test]
    fn nan_bits_are_waived_in_the_given_channels() {
        let waived = assert_pixels_bits_eq_except_nan_bits(
            "t",
            "W-test",
            &[false, true, false, false],
            &[0.0; 4],
            &pixel([0, NAN_A, NAN_A, 0x3f80_0000]),
            &pixel([0, NAN_B, NAN_A, 0x3f80_0000]),
        );
        assert_eq!(waived, 1);
    }

    #[test]
    #[should_panic(expected = "NaN bits waived by W-test in channels [false, true, false, false]")]
    fn nan_bits_are_exact_in_the_other_channels() {
        assert_pixels_bits_eq_except_nan_bits(
            "t",
            "W-test",
            &[false, true, false, false],
            &[0.0; 4],
            &pixel([0, NAN_A, NAN_A, 0]),
            &pixel([0, NAN_A, NAN_B, 0]),
        );
    }

    #[test]
    #[should_panic(expected = "1 of 4 values differ bitwise")]
    fn a_waived_nan_must_stay_nan() {
        assert_pixels_bits_eq_except_nan_bits(
            "t",
            "W-test",
            &[true; 4],
            &[0.0; 4],
            &pixel([0, NAN_A, 0, 0]),
            &pixel([0, 0, 0, 0]),
        );
    }

    #[test]
    #[should_panic(expected = "1 of 4 values differ bitwise")]
    fn values_that_are_not_nan_stay_exact_in_waived_channels() {
        assert_pixels_bits_eq_except_nan_bits(
            "t",
            "W-test",
            &[true; 4],
            &[0.0; 4],
            &pixel([0, 0x3f80_0000, 0, 0]),
            &pixel([0, NAN_A, 0, 0]),
        );
    }
}
