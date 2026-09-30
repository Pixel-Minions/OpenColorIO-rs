// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Parameter cases and their generators (T3b).
//!
//! A family describes its transform's numeric parameters as [`Slot`]s (name, precision, and
//! the channels each one applies to) by implementing [`Params`]. From that:
//! - [`Case`]s know which channels carry a NaN or infinite parameter, and compare the channels
//!   of NaN parameters under waiver W0002 and everything else bit for bit ([`Case::compare`],
//!   the only place the battery uses W0002);
//! - [`mutations`] generates, from a typical case, one case per slot and value: extreme finite
//!   values ([`extreme_values`]: ±1e38 and the smallest subnormals for `float` parameters,
//!   ±1e300 and the smallest subnormals for `double` ones) and NaN, +Inf and -Inf
//!   ([`NON_FINITE`]).
//!
//! W0002 covers the channels of NaN parameters, as the owner approved it (`waivers.toml`).
//! Infinite parameters, like extreme finite ones that overflow to infinity in the renderers,
//! compare bit for bit.

use std::fmt::Debug;
use std::sync::OnceLock;

use super::Combo;
use crate::compare::{f32_bits_report, pixels_report_except_nan_bits};

/// Which of a pixel's four channels (R, G, B, A) something applies to.
pub type Channels = [bool; 4];

/// The red channel.
pub const R: Channels = [true, false, false, false];
/// The green channel.
pub const G: Channels = [false, true, false, false];
/// The blue channel.
pub const B: Channels = [false, false, true, false];
/// The alpha channel.
pub const A: Channels = [false, false, false, true];
/// The three colour channels.
pub const RGB: Channels = [true, true, true, false];
/// All four channels.
pub const RGBA: Channels = [true, true, true, true];

/// How the op stores and computes with a parameter; it decides the extreme values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Precision {
    /// A `float` parameter.
    F32,
    /// A `double` parameter.
    F64,
    /// A `double` parameter that the renderers narrow to `float` (most op parameters): the
    /// extremes of both.
    F64AsF32,
}

/// One numeric parameter of a family's transform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    /// Its name, for labels: `"log_side_slope[g]"`.
    pub name: String,
    /// How the op uses it.
    pub precision: Precision,
    /// The output channels it applies to: a NaN or infinite value makes these channels
    /// compare under W0002.
    pub channels: Channels,
}

impl Slot {
    /// A slot.
    pub fn new(name: impl Into<String>, precision: Precision, channels: Channels) -> Self {
        Slot {
            name: name.into(),
            precision,
            channels,
        }
    }

    /// Three slots `name[r]`, `name[g]` and `name[b]` for a per-channel parameter.
    pub fn rgb(name: &str, precision: Precision) -> [Slot; 3] {
        [("r", R), ("g", G), ("b", B)]
            .map(|(c, ch)| Slot::new(format!("{name}[{c}]"), precision, ch))
    }

    /// Four slots `name[r]` to `name[a]` for a per-channel parameter with alpha.
    pub fn rgba(name: &str, precision: Precision) -> [Slot; 4] {
        [("r", R), ("g", G), ("b", B), ("a", A)]
            .map(|(c, ch)| Slot::new(format!("{name}[{c}]"), precision, ch))
    }
}

/// A family's parameters, seen as numbered slots.
///
/// Non-numeric parameters (styles, interpolation, ...) stay in the implementing type; the
/// generators keep them as they are.
pub trait Params: Clone + Debug {
    /// The numeric parameters, in order. They may depend on the value (an optional parameter
    /// has slots only when present).
    fn slots(&self) -> Vec<Slot>;
    /// The value of slot `index`.
    fn get(&self, index: usize) -> f64;
    /// Sets slot `index`.
    fn set(&mut self, index: usize, value: f64);
}

/// The extreme finite values of a parameter of `precision`: ±1e38 and the smallest `float`
/// subnormals for `float`, ±1e300 and the smallest `double` subnormals for `double`, both
/// lists for a `double` used as `float`.
pub fn extreme_values(precision: Precision) -> Vec<f64> {
    let f32_subnormal = f64::from(f32::from_bits(1));
    let f64_subnormal = f64::from_bits(1);
    let f32_extremes = [1e38, -1e38, f32_subnormal, -f32_subnormal];
    let f64_extremes = [1e300, -1e300, f64_subnormal, -f64_subnormal];
    match precision {
        Precision::F32 => f32_extremes.to_vec(),
        Precision::F64 => f64_extremes.to_vec(),
        Precision::F64AsF32 => [f64_extremes, f32_extremes].concat(),
    }
}

/// The non-finite values every parameter is tried with.
pub const NON_FINITE: [f64; 3] = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY];

/// What kind of parameters a case has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Ordinary finite parameters.
    Typical,
    /// Finite parameters at the edges of `float` or `double`: some magnitude at least 1e38,
    /// or a nonzero magnitude below FLT_MIN. Compared bit for bit.
    ExtremeFinite,
    /// At least one NaN or infinite parameter. W0002 applies to the channels of NaN
    /// parameters.
    NonFinite,
}

/// Where a case comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Origin {
    /// Written by the family.
    Explicit,
    /// Made by [`mutations`] from an explicit case: `slot` set to value `value` of the slot's
    /// list (its [`extreme_values`], then [`NON_FINITE`]).
    Generated {
        /// The slot that was changed.
        slot: usize,
        /// The index of the value in the slot's list.
        value: usize,
    },
}

/// Where waiver W0002 applies to a case with a NaN parameter.
#[derive(Debug, Clone, Copy)]
pub enum W0002Scope {
    /// In every combination (the default).
    Everywhere,
    /// Nowhere: the NaN bits are compared exactly too.
    Nowhere,
    /// Only in the combinations the function accepts; exactly elsewhere.
    Only(fn(&Combo) -> bool),
}

/// One parameter set of a family, with its label.
#[derive(Debug, Clone)]
pub struct Case<P> {
    label: String,
    params: P,
    kind: Kind,
    origin: Origin,
    non_finite: Channels,
    nan: Channels,
    w0002: W0002Scope,
}

impl<P: Params> Case<P> {
    /// An explicit case. Its kind and the channels with a NaN or infinite parameter follow from
    /// its slots.
    pub fn new(label: impl Into<String>, params: P) -> Self {
        let (mut non_finite, mut nan) = ([false; 4], [false; 4]);
        let (mut any_non_finite, mut extreme) = (false, false);
        for (i, slot) in params.slots().iter().enumerate() {
            let v = params.get(i);
            if v.is_finite() {
                extreme |= v.abs() >= 1e38 || (v != 0.0 && v.abs() < f64::from(f32::MIN_POSITIVE));
            } else {
                any_non_finite = true;
                for (c, &on) in slot.channels.iter().enumerate() {
                    non_finite[c] |= on;
                    nan[c] |= on && v.is_nan();
                }
            }
        }
        let kind = if any_non_finite {
            Kind::NonFinite
        } else if extreme {
            Kind::ExtremeFinite
        } else {
            Kind::Typical
        };
        Case {
            label: label.into(),
            params,
            kind,
            origin: Origin::Explicit,
            non_finite,
            nan,
            w0002: W0002Scope::Everywhere,
        }
    }

    /// Narrows W0002 for this case to the combinations `applies` accepts; elsewhere its NaN bits
    /// are compared exactly too. A case can only narrow the waiver: it never applies to a case
    /// without a NaN parameter.
    pub fn w0002_only_where(mut self, applies: fn(&Combo) -> bool) -> Self {
        self.w0002 = W0002Scope::Only(applies);
        self
    }

    /// Compares this case's NaN bits exactly too: no W0002.
    pub fn w0002_nowhere(mut self) -> Self {
        self.w0002 = W0002Scope::Nowhere;
        self
    }

    /// The label, for reports.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The parameters.
    pub fn params(&self) -> &P {
        &self.params
    }

    /// The kind of parameters.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Where the case comes from.
    pub fn origin(&self) -> Origin {
        self.origin
    }

    /// The channels that a NaN or infinite parameter applies to.
    pub fn non_finite_channels(&self) -> Channels {
        self.non_finite
    }

    /// The channels that a NaN parameter applies to: the ones W0002 covers.
    pub fn nan_channels(&self) -> Channels {
        self.nan
    }

    /// Whether W0002 applies to this case in `combo`: the case has a NaN parameter, and its
    /// scope includes `combo`.
    pub fn w0002_applies(&self, combo: &Combo) -> bool {
        self.nan.contains(&true)
            && match self.w0002 {
                W0002Scope::Everywhere => true,
                W0002Scope::Nowhere => false,
                W0002Scope::Only(applies) => applies(combo),
            }
    }

    /// Compares the port's `actual` RGBA pixels with the wheel's `expected` for `inputs`.
    ///
    /// Where W0002 applies ([`Case::w0002_applies`]), a value that is NaN in `expected`, in a
    /// channel with a NaN parameter, only has to be NaN in `actual`. Every other value,
    /// including the channels of infinite parameters, compares bit for bit. This is the
    /// battery's only use of W0002.
    pub fn compare(
        &self,
        combo: &Combo,
        inputs: &[f32],
        expected: &[f32],
        actual: &[f32],
    ) -> Comparison {
        if self.w0002_applies(combo) {
            match pixels_report_except_nan_bits(w0002(), &self.nan, inputs, expected, actual) {
                Ok(0) => Comparison::Exact,
                Ok(waived) => Comparison::W0002 { waived },
                Err(report) => Comparison::Mismatch(report),
            }
        } else {
            match f32_bits_report(expected, actual, Some(inputs), 4) {
                None => Comparison::Exact,
                Some(report) => Comparison::Mismatch(report),
            }
        }
    }
}

/// The result of one comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comparison {
    /// Identical, bit for bit.
    Exact,
    /// Identical except `waived` NaN values whose sign or payload bits differ, in channels
    /// W0002 covers.
    W0002 {
        /// How many NaN values matched only as NaN.
        waived: usize,
    },
    /// Different; the report lists the differences.
    Mismatch(String),
}

/// `"W0002"`, after checking that the owner's waiver is still in `waivers.toml`: if it is
/// withdrawn, every comparison that relies on it fails instead of passing silently.
fn w0002() -> &'static str {
    static CHECKED: OnceLock<()> = OnceLock::new();
    CHECKED.get_or_init(|| {
        let path = crate::paths::workspace_root().join("waivers.toml");
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let doc: toml::Table = text
            .parse()
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let listed = doc
            .get("waiver")
            .and_then(|w| w.as_array())
            .into_iter()
            .flatten()
            .any(|w| w.get("id").and_then(|id| id.as_str()) == Some("W0002"));
        assert!(
            listed,
            "waiver W0002 is not in {}: NaN bits in channels with NaN parameters \
             must match exactly",
            path.display()
        );
    });
    "W0002"
}

/// The generated cases of `base`: one per slot and value, where the slot takes one of its
/// [`extreme_values`] or one of [`NON_FINITE`] and everything else stays as in `base`. They
/// are labelled `"<base label>, <slot> = <value>"`. W0002 applies to the NaN ones in every
/// combination (a narrowing of `base` doesn't carry over).
pub fn mutations<P: Params>(base: &Case<P>) -> Vec<Case<P>> {
    let mut cases = Vec::new();
    for (i, slot) in base.params.slots().iter().enumerate() {
        for (k, &value) in slot_values(slot).iter().enumerate() {
            cases.push(mutation(base, i, slot, k, value));
        }
    }
    cases
}

/// A sample of [`mutations`]: for each slot, one extreme value and one non-finite value,
/// taken in turn as the slot index goes up, so that consecutive slots try different values.
pub fn sampled_mutations<P: Params>(base: &Case<P>) -> Vec<Case<P>> {
    let mut cases = Vec::new();
    for (i, slot) in base.params.slots().iter().enumerate() {
        let extremes = extreme_values(slot.precision).len();
        let values = slot_values(slot);
        for k in [i % extremes, extremes + i % NON_FINITE.len()] {
            cases.push(mutation(base, i, slot, k, values[k]));
        }
    }
    cases
}

/// The values a slot is tried with: its extremes, then the non-finite values.
fn slot_values(slot: &Slot) -> Vec<f64> {
    let mut values = extreme_values(slot.precision);
    values.extend(NON_FINITE);
    values
}

fn mutation<P: Params>(base: &Case<P>, slot: usize, info: &Slot, value: usize, v: f64) -> Case<P> {
    let mut params = base.params.clone();
    params.set(slot, v);
    let label = format!("{}, {} = {}", base.label, info.name, describe(v));
    let mut case = Case::new(label, params);
    case.origin = Origin::Generated { slot, value };
    case
}

/// A parameter value for a label: `NaN`, `inf`, `-inf` or the shortest exponent form.
fn describe(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else {
        format!("{v:e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battery::{Direction, Format};

    /// A family with a scalar applying to RGB, a per-channel slope and an alpha gain.
    #[derive(Debug, Clone, PartialEq)]
    struct Toy {
        base: f64,
        slope: [f64; 3],
        gain: f64,
    }

    impl Params for Toy {
        fn slots(&self) -> Vec<Slot> {
            let mut s = vec![Slot::new("base", Precision::F64, RGB)];
            s.extend(Slot::rgb("slope", Precision::F64AsF32));
            s.push(Slot::new("gain", Precision::F32, A));
            s
        }
        fn get(&self, index: usize) -> f64 {
            match index {
                0 => self.base,
                1..=3 => self.slope[index - 1],
                _ => self.gain,
            }
        }
        fn set(&mut self, index: usize, value: f64) {
            match index {
                0 => self.base = value,
                1..=3 => self.slope[index - 1] = value,
                _ => self.gain = value,
            }
        }
    }

    fn toy() -> Toy {
        Toy {
            base: 2.0,
            slope: [0.5, 1.0, 1.5],
            gain: 1.0,
        }
    }

    fn combo(direction: Direction, fast_math: bool) -> Combo {
        Combo {
            direction,
            fast_math,
            format: Format::F32_RGBA,
        }
    }

    #[test]
    fn cases_know_their_non_finite_channels() {
        let case = Case::new("typical", toy());
        assert_eq!(case.kind(), Kind::Typical);
        assert_eq!(case.non_finite_channels(), [false; 4]);

        let mut p = toy();
        p.slope[1] = f64::NAN;
        let case = Case::new("NaN green slope", p);
        assert_eq!(case.kind(), Kind::NonFinite);
        assert_eq!(case.non_finite_channels(), G);
        assert_eq!(case.nan_channels(), G);

        let mut p = toy();
        p.base = f64::INFINITY;
        p.gain = f64::NEG_INFINITY;
        let case = Case::new("inf", p.clone());
        assert_eq!(case.kind(), Kind::NonFinite);
        assert_eq!(case.non_finite_channels(), RGBA);
        assert_eq!(case.nan_channels(), [false; 4]);
        p.slope[2] = f64::NAN;
        assert_eq!(Case::new("inf and NaN", p).nan_channels(), B);

        for extreme in [1e38, -1e300, 1e-46, -5e-324] {
            let mut p = toy();
            p.slope[2] = extreme;
            let case = Case::new("extreme", p);
            assert_eq!(case.kind(), Kind::ExtremeFinite, "{extreme:e}");
            assert_eq!(case.non_finite_channels(), [false; 4]);
        }
    }

    #[test]
    fn extreme_values_follow_the_precision() {
        let sub32 = f64::from(f32::from_bits(1));
        let sub64 = f64::from_bits(1);
        assert_eq!(
            extreme_values(Precision::F32),
            vec![1e38, -1e38, sub32, -sub32]
        );
        assert_eq!(
            extreme_values(Precision::F64),
            vec![1e300, -1e300, sub64, -sub64]
        );
        assert_eq!(
            extreme_values(Precision::F64AsF32),
            vec![1e300, -1e300, sub64, -sub64, 1e38, -1e38, sub32, -sub32]
        );
    }

    #[test]
    fn mutations_change_one_slot_each() {
        let base = Case::new("toy", toy());
        let cases = mutations(&base);
        // base: F64 (4 + 3); slopes: F64AsF32 (8 + 3) x 3; gain: F32 (4 + 3).
        assert_eq!(cases.len(), 7 + 3 * 11 + 7);
        let slots = base.params().slots();
        for case in &cases {
            let Origin::Generated { slot, value } = case.origin() else {
                panic!("{} is not generated", case.label());
            };
            for (i, info) in slots.iter().enumerate() {
                let (a, b) = (case.params().get(i), base.params().get(i));
                if i == slot {
                    assert_eq!(a.to_bits(), slot_values(info)[value].to_bits());
                } else {
                    assert_eq!(a.to_bits(), b.to_bits(), "{}", case.label());
                }
            }
            let v = case.params().get(slot);
            if v.is_finite() {
                assert_eq!(case.kind(), Kind::ExtremeFinite, "{}", case.label());
                assert_eq!(case.non_finite_channels(), [false; 4]);
            } else {
                assert_eq!(case.kind(), Kind::NonFinite, "{}", case.label());
                assert_eq!(case.non_finite_channels(), slots[slot].channels);
            }
        }
        let labels: Vec<&str> = cases.iter().map(|c| c.label()).collect();
        assert!(labels.contains(&"toy, slope[g] = NaN"));
        assert!(labels.contains(&"toy, base = -1e300"));
        assert!(labels.contains(&"toy, gain = -inf"));
        assert!(labels.contains(&"toy, slope[b] = 1e38"));
    }

    #[test]
    fn sampled_mutations_try_every_slot_twice() {
        let base = Case::new("toy", toy());
        let all = mutations(&base);
        let sample = sampled_mutations(&base);
        assert_eq!(sample.len(), 2 * base.params().slots().len());
        for case in &sample {
            assert!(all.iter().any(|c| c.label() == case.label()));
        }
        for (i, pair) in sample.chunks(2).enumerate() {
            assert_eq!(pair[0].kind(), Kind::ExtremeFinite);
            assert_eq!(pair[1].kind(), Kind::NonFinite);
            for c in pair {
                assert!(matches!(c.origin(), Origin::Generated { slot, .. } if slot == i));
            }
        }
        // Consecutive slopes try different values.
        assert_ne!(sample[2].params().get(1), sample[4].params().get(2));
    }

    const NAN_A: u32 = 0x7fc0_0000;
    const NAN_B: u32 = 0xffc1_2345;

    fn px(bits: [u32; 4]) -> Vec<f32> {
        bits.map(f32::from_bits).to_vec()
    }

    #[test]
    fn w0002_covers_only_the_channels_of_nan_parameters() {
        let inputs = px([0; 4]);
        let expected = px([NAN_A, NAN_A, 0x3f80_0000, NAN_A]);
        let differs_in_green = px([NAN_A, NAN_B, 0x3f80_0000, NAN_A]);
        let differs_in_red = px([NAN_B, NAN_A, 0x3f80_0000, NAN_A]);
        let not_nan_in_green = px([NAN_A, 0, 0x3f80_0000, NAN_A]);
        let fwd = combo(Direction::Forward, true);

        let mut p = toy();
        p.slope[1] = f64::NAN;
        let case = Case::new("NaN green slope", p);
        assert!(case.w0002_applies(&fwd));
        assert_eq!(
            case.compare(&fwd, &inputs, &expected, &expected),
            Comparison::Exact
        );
        assert_eq!(
            case.compare(&fwd, &inputs, &expected, &differs_in_green),
            Comparison::W0002 { waived: 1 }
        );
        for actual in [&differs_in_red, &not_nan_in_green] {
            let Comparison::Mismatch(report) = case.compare(&fwd, &inputs, &expected, actual)
            else {
                panic!("a mismatch outside W0002 passed");
            };
            assert!(report.contains("1 of 4 values differ bitwise"), "{report}");
        }

        // A case without NaN or infinite parameters never uses W0002.
        let typical = Case::new("typical", toy());
        assert!(!typical.w0002_applies(&fwd));
        assert!(matches!(
            typical.compare(&fwd, &inputs, &expected, &differs_in_green),
            Comparison::Mismatch(_)
        ));
        // Nor does an extreme finite one, or one with an infinite parameter.
        for v in [1e300, f64::INFINITY, f64::NEG_INFINITY] {
            let mut p = toy();
            p.slope[1] = v;
            let case = Case::new("extreme or infinite", p);
            assert!(!case.w0002_applies(&fwd));
            assert!(matches!(
                case.compare(&fwd, &inputs, &expected, &differs_in_green),
                Comparison::Mismatch(_)
            ));
        }
    }

    #[test]
    fn a_case_can_narrow_w0002_but_not_widen_it() {
        let inputs = px([0; 4]);
        let expected = px([NAN_A; 4]);
        let actual = px([NAN_A, NAN_B, NAN_A, NAN_A]);
        let mut p = toy();
        p.slope[1] = f64::NAN;

        let nowhere = Case::new("nowhere", p.clone()).w0002_nowhere();
        let inverse_exact = |c: &Combo| c.direction == Direction::Inverse && !c.fast_math;
        let narrowed = Case::new("narrowed", p).w0002_only_where(inverse_exact);
        let widened = Case::new("typical", toy()).w0002_only_where(|_| true);
        for direction in Direction::BOTH {
            for fast_math in [true, false] {
                let c = combo(direction, fast_math);
                assert!(!nowhere.w0002_applies(&c));
                assert!(matches!(
                    nowhere.compare(&c, &inputs, &expected, &actual),
                    Comparison::Mismatch(_)
                ));
                assert_eq!(narrowed.w0002_applies(&c), inverse_exact(&c));
                assert!(!widened.w0002_applies(&c));
            }
        }
    }

    #[test]
    fn w0002_is_an_approved_waiver() {
        assert_eq!(w0002(), "W0002");
    }

    /// Nothing but the battery compares under W0002: outside `compare.rs`, which defines the
    /// comparison, only `Case::compare` here may call it. Oracle tests go through the battery,
    /// which applies W0002 to the channels of NaN parameters and nothing else.
    #[test]
    fn only_the_battery_uses_the_w0002_comparison() {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path.file_name().is_some_and(|n| n != "target") {
                        walk(&path, out);
                    }
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let root = crate::paths::workspace_root();
        let mut files = Vec::new();
        for dir in ["crates", "xtask"] {
            walk(&root.join(dir), &mut files);
        }
        let allowed: [&[&str]; 2] = [
            &["crates", "ocio-testkit", "src", "compare.rs"],
            &["crates", "ocio-testkit", "src", "battery", "params.rs"],
        ];
        let calls = [
            "assert_pixels_bits_eq_except_nan_bits",
            "pixels_report_except_nan_bits",
        ];
        let mut offenders = Vec::new();
        for file in &files {
            let rel: Vec<String> = file
                .strip_prefix(root)
                .unwrap_or(file)
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            if allowed.iter().any(|a| rel.iter().eq(a.iter())) {
                continue;
            }
            let text = std::fs::read_to_string(file).unwrap_or_default();
            if calls.iter().any(|call| text.contains(call)) {
                offenders.push(rel.join("/"));
            }
        }
        assert!(files.len() > 20, "found only {} sources", files.len());
        assert!(
            offenders.is_empty(),
            "the W0002 comparison is used outside the battery: {offenders:?}"
        );
    }
}
