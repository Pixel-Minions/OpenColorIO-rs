// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Parameter cases and their generators (T3b).
//!
//! A family describes its transform's numeric parameters as [`Slot`]s (name, precision, and
//! the channels each one applies to) by implementing [`Params`]. From that:
//! - [`Case`]s know which channels carry a NaN or infinite parameter, and compare the channels
//!   of NaN parameters under waiver W0002 and everything else bit for bit ([`Case::compare_pixels`];
//!   [`Case::compare_baked_luts`] for the 1D LUTs the optimizer bakes from such a case and the
//!   cache ID that hashes them; the only places that apply W0002);
//! - [`mutations`] generates, from a typical case, one case per slot and value: extreme finite
//!   values ([`extreme_values`]: ±1e38 and the smallest subnormals for `float` parameters,
//!   ±1e300 and the smallest subnormals for `double` ones) and NaN, +Inf and -Inf
//!   ([`NON_FINITE`]);
//! - a LUT's chosen entries are slots too ([`LutEntries`], [`Slot::lut_entry`]), so the
//!   generated cases put those values in one component of one entry at a time.
//!
//! W0002 covers the channels of NaN parameters, as the owner approved it (`waivers.toml`), and
//! since 2026-10-04 the NaN entries of the 1D LUTs baked from them, with the cache IDs that
//! hash those LUTs. It never covers a NaN entry of a LUT the transform is given (owner
//! decision P2-6): those compare bit for bit.
//! Infinite parameters, like extreme finite ones that overflow to infinity in the renderers,
//! compare bit for bit.

use std::fmt::Debug;
use std::sync::OnceLock;

use super::Combo;
use crate::compare::{
    InputRange, f32_bits_report, pixels_report_except_nan_bits, pixels_report_within_ulp,
};

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
    /// The output channels it applies to: a NaN value makes these channels compare under
    /// W0002, unless the slot is a LUT entry.
    pub channels: Channels,
    /// Whether the slot is an entry of a LUT ([`Slot::lut_entry`]) rather than a parameter:
    /// W0002 never covers a NaN LUT entry (owner decision P2-6): the renderers sanitize or
    /// clamp them, so the outputs compare bit for bit.
    pub lut_entry: bool,
}

impl Slot {
    /// A slot.
    pub fn new(name: impl Into<String>, precision: Precision, channels: Channels) -> Self {
        Slot {
            name: name.into(),
            precision,
            channels,
            lut_entry: false,
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

    /// A slot for one component of a LUT entry: a `float` that applies to `channels`, which
    /// W0002 never covers ([`Slot::lut_entry`](Slot#structfield.lut_entry)).
    pub fn lut_entry(name: impl Into<String>, channels: Channels) -> Self {
        Slot {
            lut_entry: true,
            ..Slot::new(name, Precision::F32, channels)
        }
    }
}

/// Chosen entries of a LUT of red, green and blue values, as parameter slots: the battery's
/// generated cases put extreme finite, NaN and ±Inf values in one component of one chosen entry
/// at a time, rather than in every entry ([`mutations`]).
///
/// A family keeps the LUT's values (three `f32`s per entry, red first, in the order the
/// transform's `setData` takes them) in its parameters, and implements [`Params`] for them
/// with [`LutEntries::slots`], [`LutEntries::get`] and [`LutEntries::set`]. Component `c` of
/// an entry applies to output channel `c`, as in a 1D LUT and in a 3D LUT's red, green and
/// blue outputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LutEntries {
    name: String,
    indices: Vec<usize>,
}

impl LutEntries {
    /// The entries `indices` of the LUT `name`, in that order.
    pub fn new(name: impl Into<String>, indices: Vec<usize>) -> Self {
        LutEntries {
            name: name.into(),
            indices,
        }
    }

    /// The usual choice for a LUT of `entries` entries: the first, the second (the first node
    /// inside the domain), the middle one and the last, without repeats.
    pub fn first_second_middle_last(name: impl Into<String>, entries: usize) -> Self {
        assert!(entries > 0, "a LUT without entries");
        let mut indices = vec![0, 1.min(entries - 1), entries / 2, entries - 1];
        indices.dedup();
        LutEntries::new(name, indices)
    }

    /// The chosen entries' indices.
    pub fn indices(&self) -> &[usize] {
        &self.indices
    }

    /// Three slots per chosen entry, `name[i].r`, `name[i].g` and `name[i].b`.
    pub fn slots(&self) -> Vec<Slot> {
        self.indices
            .iter()
            .flat_map(|&i| {
                [("r", R), ("g", G), ("b", B)]
                    .map(|(c, ch)| Slot::lut_entry(format!("{}[{i}].{c}", self.name), ch))
            })
            .collect()
    }

    /// Where slot `slot` is in the values (three per entry).
    fn position(&self, slot: usize) -> usize {
        3 * self.indices[slot / 3] + slot % 3
    }

    /// The value of slot `slot` of [`LutEntries::slots`] in `values`.
    pub fn get(&self, values: &[f32], slot: usize) -> f64 {
        f64::from(values[self.position(slot)])
    }

    /// Sets slot `slot` of [`LutEntries::slots`] in `values`, as a `float` (the C++ cast).
    pub fn set(&self, values: &mut [f32], slot: usize, value: f64) {
        values[self.position(slot)] = value as f32;
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

/// A renderer under waiver W0001 (`waivers.toml`): the PQ curves without fast math on
/// Windows, where the wheel calls SVML's `pow` and the port the UCRT's `powf`. Its name is the
/// key of its bound in the waiver's `bound_ulp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum W0001Function {
    /// `Renderer_LIN_TO_PQ`: `LIN_TO_PQ` forward, `PQ_TO_LIN` inverse.
    LinToPq,
    /// `Renderer_PQ_TO_LIN`: `PQ_TO_LIN` forward, `LIN_TO_PQ` inverse.
    PqToLin,
}

impl W0001Function {
    /// The key of the function's bound in W0001's `bound_ulp`.
    pub fn key(self) -> &'static str {
        match self {
            W0001Function::LinToPq => "LIN_TO_PQ",
            W0001Function::PqToLin => "PQ_TO_LIN",
        }
    }

    /// The inputs for which W0001 waives any difference, with their description: `PQ_TO_LIN`'s
    /// above 1 in magnitude, which the curve maps beyond 10000 nits and to its pole near 1.992,
    /// where the wheel's SVML `pow` and the port's `powf` drift apart without bound, to
    /// infinities and NaNs at different inputs (the owner's split of the waiver by range,
    /// 2026-10-05). Its bound covers the inputs up to 1 in magnitude.
    pub fn unbounded(self) -> Option<InputRange> {
        match self {
            W0001Function::LinToPq => None,
            W0001Function::PqToLin => Some(("|input| > 1", |x: f32| x.abs() > 1.0)),
        }
    }
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
    /// The renderers W0001 covers, forward and inverse.
    w0001: Option<[W0001Function; 2]>,
    allowed_log: Vec<String>,
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
                    // W0002 covers NaN parameters, never NaN LUT entries (P2-6).
                    nan[c] |= on && v.is_nan() && !slot.lut_entry;
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
            w0001: None,
            allowed_log: Vec::new(),
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

    /// Puts this case under waiver W0001 (`waivers.toml`): its renderer is `forward` in the
    /// forward direction and `inverse` in the inverse one, and on Windows with fast math off
    /// ([`Case::w0001_applies`]) its pixels compare within the waiver's bound for that
    /// renderer. Only the PQ curves' cases take it. Generated cases inherit it.
    pub fn w0001(mut self, forward: W0001Function, inverse: W0001Function) -> Self {
        self.w0001 = Some([forward, inverse]);
        self
    }

    /// The renderer W0001 covers for this case in `combo`: on Windows, with fast math off, for
    /// a case marked with [`Case::w0001`]. Elsewhere `None`: the wheel's Linux build and its
    /// fast math compute PQ as the port does, and compare bit for bit.
    pub fn w0001_applies(&self, combo: &Combo) -> Option<W0001Function> {
        let [forward, inverse] = self.w0001?;
        if !cfg!(target_os = "windows") || combo.fast_math {
            return None;
        }
        Some(match combo.direction {
            super::Direction::Forward => forward,
            super::Direction::Inverse => inverse,
        })
    }

    /// Allows OCIO log messages that contain `fragment` for this case. Any other message the
    /// wheel logs fails the case: a warning usually means the spec isn't what the family meant
    /// (OCIO ignores a misspelled optional key with a warning). Generated cases inherit this.
    ///
    /// The fragment must be specific to the message: at least [`MIN_ALLOWED_LOG_TEXT`]
    /// characters of the message's own text, past the `[OpenColorIO <level>]: ` prefix that
    /// OCIO puts on every line it logs (surrounding whitespace doesn't count). Quote what the
    /// message is about, such as the key and the transform
    /// (`"Unknown key in LogTransform: 'bse'"`), not a phrase every warning of its kind shares.
    ///
    /// # Panics
    ///
    /// If the fragment has fewer characters of the message's own text: an empty fragment, or
    /// the prefix alone (`"[OpenColorIO Warning]"`), would allow every message.
    pub fn allow_log(mut self, fragment: impl Into<String>) -> Self {
        let fragment = fragment.into();
        let own = own_log_text(&fragment).chars().count();
        assert!(
            own >= MIN_ALLOWED_LOG_TEXT,
            "case {:?}: allow_log({fragment:?}) is too broad: it has {own} characters of the \
             message's own text, past OCIO's `[OpenColorIO <level>]: ` prefix, and needs at \
             least {MIN_ALLOWED_LOG_TEXT}; quote what the message is about, such as the key \
             and the transform",
            self.label
        );
        self.allowed_log.push(fragment);
        self
    }

    /// Whether OCIO may log `message` for this case ([`Case::allow_log`]).
    pub fn allows_log(&self, message: &str) -> bool {
        self.allowed_log
            .iter()
            .any(|fragment| message.contains(fragment.as_str()))
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
    /// Where W0001 applies ([`Case::w0001_applies`]), a value of R, G or B may differ from the
    /// wheel's in any way where its input is one the renderer's [`W0001Function::unbounded`]
    /// covers, and elsewhere by up to the waiver's bound for the renderer if both are finite,
    /// and in its NaN bits if both are NaN (`compare::pixels_report_within_ulp`); everything
    /// else, alpha included, compares bit for bit. Otherwise:
    ///
    /// Where W0002 applies ([`Case::w0002_applies`]), a value that is NaN in `expected`, in a
    /// channel with a NaN parameter, only has to be NaN in `actual`. Every other value,
    /// including the channels of infinite parameters, compares bit for bit. With
    /// [`Case::compare_baked_luts`], the only use of W0002: by the battery, and by the format
    /// sweep through the API (`crates/ocio/tests/api_formats_oracle.rs`), which decodes its
    /// images into RGBA `f32` (no bit of a value lost) and passes whether the pixels were
    /// rendered with fast math in `combo`.
    pub fn compare_pixels(
        &self,
        combo: &Combo,
        inputs: &[f32],
        expected: &[f32],
        actual: &[f32],
    ) -> Comparison {
        if let Some(function) = self.w0001_applies(combo) {
            let (waiver, bound) = w0001_bound(function);
            // The PQ curves compute R, G and B; alpha passes through, exactly.
            let unbounded = function.unbounded();
            return match pixels_report_within_ulp(
                waiver, bound, &RGB, unbounded, inputs, expected, actual,
            ) {
                Ok(0) => Comparison::Exact,
                Ok(waived) => Comparison::W0001 { waived },
                Err(report) => Comparison::Mismatch(report),
            };
        }
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

    /// Compares the CPU processors' cache IDs, `expected` the wheel's and `actual` the port's,
    /// where the optimizer bakes the ops into 1D LUTs: `expected_luts` and `actual_luts` are
    /// the values of the Lut1DTransforms of both optimized processors'
    /// `createGroupTransform()`, in order, three per entry.
    ///
    /// Equal cache IDs are exact. Otherwise W0002 as the owner extended it on 2026-10-04
    /// (`waivers.toml`): the bake renders the ops without fast math (`EvalTransform`,
    /// src/OpenColorIO/ops/OpTools.cpp, through `Op::apply`, Op.h:232-241 @ v2.5.2), so where
    /// W0002 applies to this case in `combo` with fast math off:
    /// - both IDs hold one `<Lut1D <hash> ` per LUT (a hash of 32 hexadecimal digits, as
    ///   `Lut1DOpData::getCacheID` writes it), and are equal but for those hashes;
    /// - LUT `k`'s hash may differ only if LUT `k`'s entries differ, and only in the sign and
    ///   payload bits of NaN entries in the channels of NaN parameters; a LUT whose hash is
    ///   equal must be bit-identical.
    ///
    /// Anything else is a mismatch.
    pub fn compare_baked_luts(
        &self,
        combo: &Combo,
        expected: &str,
        actual: &str,
        expected_luts: &[Vec<f32>],
        actual_luts: &[Vec<f32>],
    ) -> Comparison {
        if expected == actual {
            return Comparison::Exact;
        }
        let ids = || format!("\n  wheel {expected}\n  port  {actual}");
        let combo = Combo {
            fast_math: false,
            ..*combo
        };
        if !self.w0002_applies(&combo) {
            return Comparison::Mismatch(format!(
                "cache IDs differ where W0002 doesn't apply:{}",
                ids()
            ));
        }
        let (Some((expected_rest, expected_hashes)), Some((actual_rest, actual_hashes))) =
            (lut_hashes(expected), lut_hashes(actual))
        else {
            return Comparison::Mismatch(format!(
                "a cache ID has a 1D LUT without a hash of 32 hexadecimal digits:{}",
                ids()
            ));
        };
        if expected_rest != actual_rest {
            return Comparison::Mismatch(format!(
                "cache IDs differ beyond their 1D LUTs' hashes:{}",
                ids()
            ));
        }
        if expected_luts.len() != actual_luts.len() || expected_hashes.len() != expected_luts.len()
        {
            return Comparison::Mismatch(format!(
                "{} and {} 1D LUTs (wheel, port) for {} hashes in the cache IDs:{}",
                expected_luts.len(),
                actual_luts.len(),
                expected_hashes.len(),
                ids()
            ));
        }
        let mut waived = 0;
        for (k, (e, a)) in expected_luts.iter().zip(actual_luts).enumerate() {
            if e.len() != a.len() || e.len() % 3 != 0 {
                return Comparison::Mismatch(format!(
                    "1D LUT {k}: {} and {} values (wheel, port)",
                    e.len(),
                    a.len()
                ));
            }
            let rgba = |values: &[f32]| -> Vec<f32> {
                values
                    .chunks(3)
                    .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 0.0])
                    .collect()
            };
            let (e, a) = (rgba(e), rgba(a));
            let entries = vec![0.0; e.len()];
            let n = match pixels_report_except_nan_bits(w0002(), &self.nan, &entries, &e, &a) {
                Ok(n) => n,
                Err(report) => return Comparison::Mismatch(format!("1D LUT {k}: {report}")),
            };
            let hash_differs = expected_hashes[k] != actual_hashes[k];
            if hash_differs != (n > 0) {
                return Comparison::Mismatch(format!(
                    "1D LUT {k}: its hash {} and {n} of its entries differ in NaN bits:{}",
                    if hash_differs { "differs" } else { "is equal" },
                    ids()
                ));
            }
            waived += n;
        }
        Comparison::W0002 { waived }
    }
}

/// A cache ID without its 1D LUTs' hashes (each `<Lut1D ` followed by 32 hexadecimal digits
/// and a space, as `Lut1DOpData::getCacheID` writes it into the ops' IDs, with the hash
/// replaced by `#`), and the hashes in order; `None` if a `<Lut1D ` isn't followed by such a
/// hash.
fn lut_hashes(cache_id: &str) -> Option<(String, Vec<&str>)> {
    const TAG: &str = "<Lut1D ";
    const DIGITS: usize = 32;
    let mut out = String::new();
    let mut hashes = Vec::new();
    let mut rest = cache_id;
    while let Some(at) = rest.find(TAG) {
        out.push_str(&rest[..at + TAG.len()]);
        rest = &rest[at + TAG.len()..];
        let digits = rest.bytes().take_while(u8::is_ascii_hexdigit).count();
        if digits != DIGITS || rest.as_bytes().get(DIGITS) != Some(&b' ') {
            return None;
        }
        hashes.push(&rest[..DIGITS]);
        out.push('#');
        rest = &rest[DIGITS..];
    }
    out.push_str(rest);
    Some((out, hashes))
}

/// The fewest characters of a message's own text that a [`Case::allow_log`] fragment needs.
pub const MIN_ALLOWED_LOG_TEXT: usize = 10;

/// The prefixes OCIO puts on every line it logs, one per level (`LogError`, `LogWarning`,
/// `LogInfo` and `LogDebug`, src/OpenColorIO/Logging.cpp:162-204 @ v2.5.2, through
/// `LogMessage`, :75-88).
const LOG_PREFIXES: [&str; 4] = [
    "[OpenColorIO Error]: ",
    "[OpenColorIO Warning]: ",
    "[OpenColorIO Info]: ",
    "[OpenColorIO Debug]: ",
];

/// The part of a log fragment that only a message's own text can match: the trimmed fragment
/// without the longest end of a [`LOG_PREFIXES`] prefix that it starts with, trimmed again.
/// Empty when the whole fragment fits in a prefix.
fn own_log_text(fragment: &str) -> &str {
    let fragment = fragment.trim();
    if LOG_PREFIXES.iter().any(|prefix| prefix.contains(fragment)) {
        return "";
    }
    let overlap = LOG_PREFIXES
        .iter()
        .flat_map(|prefix| (0..prefix.len()).map(move |start| &prefix[start..]))
        .filter(|end| fragment.starts_with(*end))
        .map(str::len)
        .max()
        .unwrap_or(0);
    fragment[overlap..].trim()
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
    /// Identical except `waived` values within waiver W0001: finite values within its bound,
    /// NaN values differing in sign or payload bits.
    W0001 {
        /// How many values differed within the waiver.
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

/// `"W0001"` and its bound in ulp for `function`, read from the waiver's `bound_ulp` in
/// `waivers.toml`: the owner's numbers, so that a changed or withdrawn bound fails every
/// comparison that relies on it instead of passing silently.
fn w0001_bound(function: W0001Function) -> (&'static str, u64) {
    static BOUNDS: OnceLock<toml::Table> = OnceLock::new();
    let bounds = BOUNDS.get_or_init(|| {
        let path = crate::paths::workspace_root().join("waivers.toml");
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let doc: toml::Table = text
            .parse()
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        doc.get("waiver")
            .and_then(|w| w.as_array())
            .into_iter()
            .flatten()
            .find(|w| w.get("id").and_then(|id| id.as_str()) == Some("W0001"))
            .and_then(|w| w.get("bound_ulp"))
            .and_then(|b| b.as_table())
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "waiver W0001 has no bound_ulp in {}: the PQ curves must match exactly",
                    path.display()
                )
            })
    });
    let bound = bounds
        .get(function.key())
        .and_then(|b| b.as_integer())
        .and_then(|b| u64::try_from(b).ok())
        .unwrap_or_else(|| {
            panic!(
                "waiver W0001 has no bound for {}: it must match exactly",
                function.key()
            )
        });
    ("W0001", bound)
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
    case.allowed_log = base.allowed_log.clone();
    case.w0001 = base.w0001;
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
            case.compare_pixels(&fwd, &inputs, &expected, &expected),
            Comparison::Exact
        );
        assert_eq!(
            case.compare_pixels(&fwd, &inputs, &expected, &differs_in_green),
            Comparison::W0002 { waived: 1 }
        );
        for actual in [&differs_in_red, &not_nan_in_green] {
            let Comparison::Mismatch(report) =
                case.compare_pixels(&fwd, &inputs, &expected, actual)
            else {
                panic!("a mismatch outside W0002 passed");
            };
            assert!(report.contains("1 of 4 values differ bitwise"), "{report}");
        }

        // A case without NaN parameters never uses W0002.
        let typical = Case::new("typical", toy());
        assert!(!typical.w0002_applies(&fwd));
        assert!(matches!(
            typical.compare_pixels(&fwd, &inputs, &expected, &differs_in_green),
            Comparison::Mismatch(_)
        ));
        // Nor does an extreme finite one, or one with an infinite parameter.
        for v in [1e300, f64::INFINITY, f64::NEG_INFINITY] {
            let mut p = toy();
            p.slope[1] = v;
            let case = Case::new("extreme or infinite", p);
            assert!(!case.w0002_applies(&fwd));
            assert!(matches!(
                case.compare_pixels(&fwd, &inputs, &expected, &differs_in_green),
                Comparison::Mismatch(_)
            ));
        }
    }

    /// W0001 applies to the cases marked with `Case::w0001`, on Windows with fast math off, to
    /// the renderer of the combination's direction: finite values within its bound in
    /// `waivers.toml`, NaN bits of NaNs; NaN positions, infinities and values beyond the bound
    /// are mismatches. Elsewhere, and for other cases, the comparison is exact.
    #[test]
    fn w0001_covers_the_pq_renderers_within_their_bound_on_windows_only() {
        let inputs = px([0; 4]);
        let one = 0x3f80_0000;
        let expected = px([one, one, NAN_A, one]);
        let (fwd, inv) = (
            combo(Direction::Forward, false),
            combo(Direction::Inverse, false),
        );
        let case =
            Case::new("typical", toy()).w0001(W0001Function::LinToPq, W0001Function::PqToLin);
        let windows = cfg!(target_os = "windows");
        assert_eq!(
            case.w0001_applies(&fwd),
            windows.then_some(W0001Function::LinToPq)
        );
        assert_eq!(
            case.w0001_applies(&inv),
            windows.then_some(W0001Function::PqToLin)
        );
        assert_eq!(case.w0001_applies(&combo(Direction::Forward, true)), None);
        assert_eq!(Case::new("typical", toy()).w0001_applies(&fwd), None);

        let (_, bound) = w0001_bound(W0001Function::LinToPq);
        let off = |ulp: u64| {
            let mut a = expected.clone();
            a[0] = f32::from_bits(one + u32::try_from(ulp).unwrap());
            a
        };
        let other_nan = px([one, one, NAN_B, one]);
        let mut nan_moved = expected.clone();
        nan_moved[1] = f32::from_bits(NAN_A);
        let mut infinite = expected.clone();
        infinite[0] = f32::INFINITY;
        if windows {
            assert_eq!(
                case.compare_pixels(&fwd, &inputs, &expected, &off(bound)),
                Comparison::W0001 { waived: 1 }
            );
            assert_eq!(
                case.compare_pixels(&fwd, &inputs, &expected, &other_nan),
                Comparison::W0001 { waived: 1 }
            );
        }
        for actual in [&off(bound + 1), &nan_moved, &infinite] {
            assert!(matches!(
                case.compare_pixels(&fwd, &inputs, &expected, actual),
                Comparison::Mismatch(_)
            ));
        }
        // Fast math, and cases without the mark, compare exactly.
        for (c, combo) in [
            (&case, combo(Direction::Forward, true)),
            (&Case::new("typical", toy()), fwd),
        ] {
            assert!(matches!(
                c.compare_pixels(&combo, &inputs, &expected, &off(1)),
                Comparison::Mismatch(_)
            ));
        }
        // Generated cases inherit the mark.
        for generated in sampled_mutations(&case) {
            assert_eq!(generated.w0001_applies(&fwd), case.w0001_applies(&fwd));
        }
    }

    /// W0001 never covers alpha, which the PQ curves pass through: a finite alpha one ulp
    /// off, or a signalling NaN alpha quieted, is a mismatch. It waives any difference of
    /// R, G and B for the inputs of `PQ_TO_LIN` above 1 in magnitude, NaN positions and
    /// infinities included, and none for `LIN_TO_PQ`'s or for inputs up to 1.
    #[test]
    fn w0001_compares_alpha_exactly_and_waives_pq_to_lin_above_1() {
        let one = 0x3f80_0000;
        let snan = 0x7f80_0001;
        let case =
            Case::new("typical", toy()).w0001(W0001Function::LinToPq, W0001Function::PqToLin);
        let (fwd, inv) = (
            combo(Direction::Forward, false),
            combo(Direction::Inverse, false),
        );
        let expected = px([one, one, one, one]);
        let mut alpha_off = expected.clone();
        alpha_off[3] = f32::from_bits(one + 1);
        let snan_alpha = px([one, one, one, snan]);
        let mut quieted = snan_alpha.clone();
        quieted[3] = f32::from_bits(snan | 0x0040_0000);
        for combo in [fwd, inv] {
            let inputs = px([0x3fc0_0000; 4]);
            for (e, a) in [(&expected, &alpha_off), (&snan_alpha, &quieted)] {
                assert!(matches!(
                    case.compare_pixels(&combo, &inputs, e, a),
                    Comparison::Mismatch(_)
                ));
            }
        }
        if !cfg!(target_os = "windows") {
            return;
        }
        // Inputs 1.5 and -1.5 (any difference) and 1.0 (within the bound) in R, G and B.
        let inputs = px([0x3fc0_0000, 0xbfc0_0000, one, one]);
        let (_, bound) = w0001_bound(W0001Function::PqToLin);
        let far = px([0x7f80_0000, NAN_A, one + u32::try_from(bound).unwrap(), one]);
        assert_eq!(
            case.compare_pixels(&inv, &inputs, &expected, &far),
            Comparison::W0001 { waived: 3 }
        );
        let mut beyond = expected.clone();
        beyond[2] = f32::from_bits(one + u32::try_from(bound).unwrap() + 1);
        assert!(matches!(
            case.compare_pixels(&inv, &inputs, &expected, &beyond),
            Comparison::Mismatch(_)
        ));
        // Not forward (LIN_TO_PQ), whatever the input.
        assert!(matches!(
            case.compare_pixels(&fwd, &inputs, &expected, &far),
            Comparison::Mismatch(_)
        ));
    }

    /// W0002 covers the channels of NaN parameters and no other, alpha included: for a NaN
    /// green slope, a NaN-bit difference in red, blue or alpha is a mismatch; for a NaN base
    /// (the three colour channels), one in alpha is.
    #[test]
    fn w0002_never_covers_a_channel_without_a_nan_parameter() {
        let inputs = px([0; 4]);
        let expected = px([NAN_A; 4]);
        let fwd = combo(Direction::Forward, true);
        let mut green = toy();
        green.slope[1] = f64::NAN;
        let mut base = toy();
        base.base = f64::NAN;
        for (params, covered) in [(green, G), (base, RGB)] {
            let case = Case::new("NaN", params);
            for c in 0..4 {
                let mut actual = expected.clone();
                actual[c] = f32::from_bits(NAN_B);
                let result = case.compare_pixels(&fwd, &inputs, &expected, &actual);
                if covered[c] {
                    assert_eq!(result, Comparison::W0002 { waived: 1 }, "channel {c}");
                } else {
                    assert!(
                        matches!(result, Comparison::Mismatch(_)),
                        "channel {c}: {result:?}"
                    );
                }
            }
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
                    nowhere.compare_pixels(&c, &inputs, &expected, &actual),
                    Comparison::Mismatch(_)
                ));
                assert_eq!(narrowed.w0002_applies(&c), inverse_exact(&c));
                assert!(!widened.w0002_applies(&c));
            }
        }
    }

    /// A baked LUT's cache ID may differ only in its LUTs' hashes, only where the LUTs differ
    /// in NaN bits of the channels of NaN parameters, and only where W0002 applies with fast
    /// math off, whatever the combination's own fast math.
    #[test]
    fn baked_luts_differ_only_in_the_nan_bits_w0002_covers() {
        let (nan_a, nan_b) = (f32::from_bits(NAN_A), f32::from_bits(NAN_B));
        let id = |hash: &str| {
            format!(
                "CPU Processor: from 8ui to 8ui oFlags 1 ops:  <Lut1D {hash} forward default standard domain none>"
            )
        };
        let (h1, h2) = (
            "c2a35507e24998642c4814384624406e",
            "bdcf4e53a2b87a7e6abed7e715eb0032",
        );
        let mut p = toy();
        p.slope[1] = f64::NAN;
        let case = Case::new("NaN green slope", p.clone())
            .w0002_only_where(|c| c.direction == Direction::Inverse && !c.fast_math);
        let wheel = vec![vec![0.5, nan_a, 0.25, 0.75, nan_a, 1.0]];
        let green = vec![vec![0.5, nan_b, 0.25, 0.75, nan_a, 1.0]];
        let red = vec![vec![nan_a, nan_a, 0.25, 0.75, nan_a, 1.0]];
        let value = vec![vec![0.5, nan_a, 0.25, 0.75, nan_a, 0.5]];
        for fast_math in [true, false] {
            let inverse = combo(Direction::Inverse, fast_math);
            let forward = combo(Direction::Forward, fast_math);
            assert_eq!(
                case.compare_baked_luts(&inverse, &id(h1), &id(h2), &wheel, &green),
                Comparison::W0002 { waived: 1 }
            );
            assert_eq!(
                case.compare_baked_luts(&forward, &id(h1), &id(h1), &wheel, &green),
                Comparison::Exact
            );
            let mismatch = |c: &Combo, e: &str, a: &str, luts: &[Vec<f32>]| {
                matches!(
                    case.compare_baked_luts(c, e, a, &wheel, luts),
                    Comparison::Mismatch(_)
                )
            };
            // W0002 doesn't apply forward.
            assert!(mismatch(&forward, &id(h1), &id(h2), &green));
            // The same LUTs can't explain different hashes.
            assert!(mismatch(&inverse, &id(h1), &id(h2), &wheel));
            // A NaN where the wheel has a value, in a channel without a NaN parameter.
            assert!(mismatch(&inverse, &id(h1), &id(h2), &red));
            // Any other value.
            assert!(mismatch(&inverse, &id(h1), &id(h2), &value));
            // A difference beyond the hashes.
            let other = id(h2).replace("8ui to 8ui", "8ui to 16ui");
            assert!(mismatch(&inverse, &id(h1), &other, &green));
            // Another number of LUTs.
            assert!(mismatch(&inverse, &id(h1), &id(h2), &[]));
            let nowhere = Case::new("nowhere", p.clone()).w0002_nowhere();
            assert!(matches!(
                nowhere.compare_baked_luts(&inverse, &id(h1), &id(h2), &wheel, &green),
                Comparison::Mismatch(_)
            ));
        }
        let inverse = combo(Direction::Inverse, false);
        let check = |e: &str, a: &str, wheel: &[Vec<f32>], port: &[Vec<f32>]| {
            case.compare_baked_luts(&inverse, e, a, wheel, port)
        };
        // NaNs of other bits in a channel without a NaN parameter (red, blue).
        let red_nan = vec![vec![nan_a, 0.5, 0.25, 0.75, 0.5, 1.0]];
        let red_other = vec![vec![nan_b, 0.5, 0.25, 0.75, 0.5, 1.0]];
        assert!(matches!(
            check(&id(h1), &id(h2), &red_nan, &red_other),
            Comparison::Mismatch(_)
        ));
        // Two LUTs: each hash may differ only where its own LUT differs in NaN bits.
        let two = |a: &str, b: &str| format!("{} <Lut1D {b} forward>", id(a));
        let wheel_two = vec![wheel[0].clone(), wheel[0].clone()];
        let second = vec![wheel[0].clone(), green[0].clone()];
        assert_eq!(
            check(&two(h1, h1), &two(h1, h2), &wheel_two, &second),
            Comparison::W0002 { waived: 1 }
        );
        // The first LUT's hash differs, but its entries are identical.
        assert!(matches!(
            check(&two(h1, h1), &two(h2, h2), &wheel_two, &second),
            Comparison::Mismatch(_)
        ));
        // The second LUT differs in NaN bits, but its hash is equal.
        assert!(matches!(
            check(&two(h1, h1), &two(h2, h1), &wheel_two, &second),
            Comparison::Mismatch(_)
        ));
        // Hashes are 32 hexadecimal digits.
        assert!(matches!(
            check(&id("abc"), &id(h2), &wheel, &green),
            Comparison::Mismatch(_)
        ));
        assert!(matches!(
            check(&id("deadbeef"), &id("cafe"), &wheel, &green),
            Comparison::Mismatch(_)
        ));
        assert_eq!(
            lut_hashes(&two(h1, h2)),
            Some((
                "CPU Processor: from 8ui to 8ui oFlags 1 ops:  <Lut1D # forward default \
                 standard domain none> <Lut1D # forward>"
                    .to_string(),
                vec![h1, h2]
            ))
        );
        assert_eq!(lut_hashes(&id(&format!("{h1}0"))), None);
    }

    /// A LUT's values with chosen entries as slots, as a family keeps them.
    #[derive(Debug, Clone)]
    struct Lut {
        values: Vec<f32>,
        entries: LutEntries,
    }

    impl Params for Lut {
        fn slots(&self) -> Vec<Slot> {
            self.entries.slots()
        }
        fn get(&self, index: usize) -> f64 {
            self.entries.get(&self.values, index)
        }
        fn set(&mut self, index: usize, value: f64) {
            self.entries.set(&mut self.values, index, value);
        }
    }

    /// A LUT of `n` entries whose value `k` is `k`.
    fn lut(n: usize) -> Lut {
        Lut {
            values: (0..3 * n).map(|k| k as f32).collect(),
            entries: LutEntries::first_second_middle_last("lut", n),
        }
    }

    #[test]
    fn lut_entries_are_the_first_second_middle_and_last() {
        let indices = |n| {
            LutEntries::first_second_middle_last("lut", n)
                .indices()
                .to_vec()
        };
        assert_eq!(indices(1), [0]);
        assert_eq!(indices(2), [0, 1]);
        assert_eq!(indices(3), [0, 1, 2]);
        assert_eq!(indices(17), [0, 1, 8, 16]);
        assert_eq!(indices(4096), [0, 1, 2048, 4095]);
        let slots = LutEntries::new("lut", vec![5, 2]).slots();
        let names: Vec<&str> = slots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "lut[5].r", "lut[5].g", "lut[5].b", "lut[2].r", "lut[2].g", "lut[2].b"
            ]
        );
        for (slot, channels) in slots.iter().zip([R, G, B, R, G, B]) {
            assert_eq!(slot.channels, channels);
            assert_eq!(slot.precision, Precision::F32);
            assert!(slot.lut_entry);
        }
        assert!(!Slot::new("base", Precision::F32, RGB).lut_entry);
    }

    /// Each generated case of a LUT changes one component of one chosen entry, as a `float`,
    /// and leaves every other value alone.
    #[test]
    fn lut_mutations_change_one_entry_component() {
        let base = Case::new("lut", lut(17));
        let cases = mutations(&base);
        // 4 entries, 3 components, F32 (4 + 3) values each.
        assert_eq!(cases.len(), 4 * 3 * 7);
        for case in &cases {
            let Origin::Generated { slot, value } = case.origin() else {
                panic!("{} is not generated", case.label());
            };
            let at = 3 * [0, 1, 8, 16][slot / 3] + slot % 3;
            let wanted = slot_values(&base.params().slots()[slot])[value] as f32;
            for (k, (a, b)) in case
                .params()
                .values
                .iter()
                .zip(&base.params().values)
                .enumerate()
            {
                if k == at {
                    assert_eq!(a.to_bits(), wanted.to_bits(), "{}", case.label());
                } else {
                    assert_eq!(a.to_bits(), b.to_bits(), "{}", case.label());
                }
            }
        }
        let labels: Vec<&str> = cases.iter().map(|c| c.label()).collect();
        assert!(labels.contains(&"lut, lut[8].g = NaN"), "{labels:?}");
        assert!(labels.contains(&"lut, lut[16].b = -1e38"), "{labels:?}");
    }

    /// W0002 never covers a NaN LUT entry (owner decision P2-6): such a case is non-finite in
    /// the entry's channel, but compares NaN bits exactly, in every combination.
    #[test]
    fn w0002_never_covers_nan_lut_entries() {
        let mut p = lut(17);
        p.set(4, f64::NAN);
        let case = Case::new("NaN lut[1].g", p);
        assert_eq!(case.kind(), Kind::NonFinite);
        assert_eq!(case.non_finite_channels(), G);
        assert_eq!(case.nan_channels(), [false; 4]);
        let inputs = px([0; 4]);
        let expected = px([0, NAN_A, 0, 0]);
        let actual = px([0, NAN_B, 0, 0]);
        for direction in Direction::BOTH {
            for fast_math in [true, false] {
                let c = combo(direction, fast_math);
                assert!(!case.w0002_applies(&c));
                assert!(matches!(
                    case.compare_pixels(&c, &inputs, &expected, &actual),
                    Comparison::Mismatch(_)
                ));
            }
        }
    }

    #[test]
    fn w0002_is_an_approved_waiver() {
        assert_eq!(w0002(), "W0002");
    }

    /// `allow_log` takes only a fragment of a message's own text: an empty one, OCIO's prefix
    /// (whole, or the end of it that a fragment starts with) or a few characters past it would
    /// allow every message, or every message of a kind. Generated cases inherit what it takes.
    #[test]
    fn allow_log_takes_only_a_fragment_specific_to_the_message() {
        for fragment in [
            "",
            " \n",
            "[OpenColorIO Warning]",
            "[OpenColorIO Warning]: ",
            "[OpenColorIO Error]:",
            "[OpenColorIO",
            "OpenColorIO Warning",
            "Warning]: ",
            "]: At line",
            "[OpenColorIO Warning]: At line",
            "unknown",
        ] {
            let result =
                std::panic::catch_unwind(|| Case::new("typical", toy()).allow_log(fragment));
            let Err(payload) = result else {
                panic!("allow_log({fragment:?}) took the fragment");
            };
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .unwrap_or_default();
            assert!(message.contains("is too broad"), "{fragment:?}: {message}");
        }

        // What the logging function receives for a key LogTransform doesn't know
        // (`LogUnknownKeyWarning`, src/OpenColorIO/OCIOYaml.cpp:248-257 @ v2.5.2).
        let message = "[OpenColorIO Warning]: Unknown key in LogTransform: 'bse'.\n";
        let other_key = "[OpenColorIO Warning]: Unknown key in LogTransform: 'bas'.\n";
        for fragment in [
            "Unknown key in LogTransform: 'bse'",
            "[OpenColorIO Warning]: Unknown key in LogTransform: 'bse'",
            "in LogTransform: 'bse'",
        ] {
            let case = Case::new("typical", toy()).allow_log(fragment);
            assert!(case.allows_log(message), "{fragment:?}");
            assert!(!case.allows_log(other_key), "{fragment:?}");
            for generated in sampled_mutations(&case) {
                assert!(generated.allows_log(message), "{fragment:?}");
                assert!(!generated.allows_log(other_key), "{fragment:?}");
            }
        }
    }

    /// Nothing but the battery and the format sweep through the API compares under W0002, and
    /// nothing but them under W0001 (`compare_pixels` with a case marked `Case::w0001`, which
    /// only the PQ cases of the op battery and of the API's cases are).
    /// Outside `compare.rs`, which defines the comparisons, only the `Case` methods here may call
    /// them, and only `w0001_bound` reads W0001's bound; and they are called only by the
    /// battery's engine, by this file's tests, and by `crates/ocio/tests/api_formats_oracle.rs`
    /// (`compare_pixels` on its decoded images, `compare_baked_luts` on the LUTs the optimizer
    /// bakes), whatever `Combo` a caller builds.
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
        const COMPARE: &str = "crates/ocio-testkit/src/compare.rs";
        const PARAMS: &str = "crates/ocio-testkit/src/battery/params.rs";
        const ENGINE: &str = "crates/ocio-testkit/src/battery/engine.rs";
        const SWEEP: &str = "crates/ocio/tests/api_formats_oracle.rs";
        // The cases under W0001: the PQ curves' in the op battery and in the API's cases.
        const FIXED_FUNCTION: &str = "crates/ocio-ops/tests/fixed_function_oracle.rs";
        const API_CASES: &str = "crates/ocio/tests/common/api_cases.rs";
        // Each name, and the only files that may contain it.
        let rules: [(&str, &[&str]); 8] = [
            ("assert_pixels_bits_eq_except_nan_bits", &[COMPARE, PARAMS]),
            ("pixels_report_except_nan_bits", &[COMPARE, PARAMS]),
            ("pixels_report_within_ulp", &[COMPARE, PARAMS]),
            ("w0001_bound(", &[PARAMS]),
            (".w0001(", &[PARAMS, FIXED_FUNCTION, API_CASES]),
            (".unbounded()", &[PARAMS]),
            ("compare_pixels(", &[PARAMS, ENGINE, SWEEP]),
            ("compare_baked_luts(", &[PARAMS, SWEEP]),
        ];
        let mut offenders = Vec::new();
        for file in &files {
            let rel: Vec<String> = file
                .strip_prefix(root)
                .unwrap_or(file)
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            let rel = rel.join("/");
            let text = std::fs::read_to_string(file).unwrap_or_default();
            for (name, allowed) in rules {
                if text.contains(name) && !allowed.contains(&rel.as_str()) {
                    offenders.push(format!("{rel}: {name}"));
                }
            }
        }
        assert!(files.len() > 20, "found only {} sources", files.len());
        assert!(
            offenders.is_empty(),
            "the W0002 or W0001 comparison is used outside the battery and the format sweep: \
             {offenders:?}"
        );
    }
}
