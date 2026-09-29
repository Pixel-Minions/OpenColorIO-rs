// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! C `printf` and C++ iostream number formatting, byte for byte as OpenColorIO 2.5.2 produces
//! it on each platform (PLAN.md D12).
//!
//! OCIO turns numbers into text in four ways, and all of them end in the C runtime's `printf`:
//! - `std::ostream << double` (and `<< float`, which promotes to `double`): the transforms'
//!   `operator<<`, cache IDs, the CTF writer, shader text, `FloatToString`/`DoubleToString`.
//!   libstdc++'s `num_put::_M_insert_float` builds the format `%[+][#].*{f|e|E|g|G}` and calls
//!   `vsnprintf` (glibc) in the C locale; MSVC's `num_put::do_put(double)` builds the same
//!   format with `_Ffmt` and calls `sprintf_s` (UCRT). The stream then pads to its width.
//! - yaml-cpp 0.8.0's emitter: a `std::stringstream` with precision 7 (float) or 15 (double),
//!   after its own `.nan`/`.inf`/`-.inf` check (see `crate`'s yaml-cpp port).
//! - `std::to_string(double)`, which is `sprintf("%f")` on both platforms.
//! - `snprintf` directly (only `%s` in `CPUInfo.cpp`).
//!
//! UCRT and glibc both convert exactly and round half to even on exact ties (UCRT does so for
//! programs built with VS 2019 16.2 or later, including `msvcp140.dll`: the "standard
//! rounding" printf option). They differ in how they spell NaN, in `#` with precision 0, and
//! in `%a` (which OCIO never uses and this module does not provide). [`Crt`] selects the
//! platform; [`Crt::NATIVE`] is the one the port runs on.
//!
//! The conversion here is exact by construction: a finite binary value `m * 2^e` is expanded
//! to all of its decimal digits with integer arithmetic (`m << e`, or `m * 5^-e` scaled by
//! `10^e`), and rounding looks at those digits. `ocio-ops/tests/cfmt_crt.rs` compares every
//! format OCIO uses with the platform C runtime through FFI.

use std::fmt::Write as _;

/// The C runtime whose conventions to follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crt {
    /// The Universal CRT (Windows), as `msvcp140.dll` calls it.
    Ucrt,
    /// GNU libc (Linux), as libstdc++ calls it.
    Glibc,
}

impl Crt {
    /// The C runtime of the platform being compiled for.
    pub const NATIVE: Crt = if cfg!(windows) { Crt::Ucrt } else { Crt::Glibc };
}

// ---------------------------------------------------------------------------------------
// Exact decimal expansion
// ---------------------------------------------------------------------------------------

/// An unsigned integer as little-endian base-2^32 limbs, without leading zero limbs.
#[derive(Debug, Clone)]
struct Big(Vec<u32>);

impl Big {
    fn from_u64(v: u64) -> Big {
        let mut b = Big(vec![v as u32, (v >> 32) as u32]);
        b.trim();
        b
    }

    fn trim(&mut self) {
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
    }

    fn mul_small(&mut self, m: u32) {
        let mut carry = 0u64;
        for limb in &mut self.0 {
            let v = u64::from(*limb) * u64::from(m) + carry;
            *limb = v as u32;
            carry = v >> 32;
        }
        if carry != 0 {
            self.0.push(carry as u32);
        }
    }

    fn mul_pow5(&mut self, mut k: u32) {
        const POW5_13: u32 = 1_220_703_125; // 5^13, the largest power of 5 below 2^32
        while k >= 13 {
            self.mul_small(POW5_13);
            k -= 13;
        }
        self.mul_small(5u32.pow(k));
    }

    fn shl(&mut self, bits: u32) {
        let limbs = (bits / 32) as usize;
        let rest = bits % 32;
        if rest != 0 {
            let mut carry = 0u32;
            for limb in &mut self.0 {
                let v = *limb;
                *limb = (v << rest) | carry;
                carry = v >> (32 - rest);
            }
            if carry != 0 {
                self.0.push(carry);
            }
        }
        if limbs != 0 {
            self.0.splice(0..0, std::iter::repeat_n(0, limbs));
        }
    }

    /// Divides in place and returns the remainder.
    fn divmod_small(&mut self, d: u32) -> u32 {
        let mut rem = 0u64;
        for limb in self.0.iter_mut().rev() {
            let v = (rem << 32) | u64::from(*limb);
            *limb = (v / u64::from(d)) as u32;
            rem = v % u64::from(d);
        }
        self.trim();
        rem as u32
    }

    /// Decimal digits (values 0-9), most significant first. Empty for zero.
    fn into_decimal(mut self) -> Vec<u8> {
        const CHUNK: u32 = 1_000_000_000;
        let mut chunks = Vec::new();
        while !self.0.is_empty() {
            chunks.push(self.divmod_small(CHUNK));
        }
        let mut digits = Vec::with_capacity(chunks.len() * 9);
        for (i, chunk) in chunks.iter().rev().enumerate() {
            let mut buf = [0u8; 9];
            let mut c = *chunk;
            for slot in buf.iter_mut().rev() {
                *slot = (c % 10) as u8;
                c /= 10;
            }
            let skip = if i == 0 {
                buf.iter().position(|&d| d != 0).unwrap_or(9)
            } else {
                0
            };
            digits.extend_from_slice(&buf[skip..]);
        }
        digits
    }
}

/// The exact decimal value of a finite, nonzero magnitude: `0.d[0] d[1] ... * 10^exp10`, with
/// `d[0] != 0` and no trailing zero digits.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Exact {
    digits: Vec<u8>,
    exp10: i32,
}

/// Expands `|value|` (finite, nonzero) exactly.
fn exact_decimal(value: f64) -> Exact {
    let bits = value.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    let (m, e) = if biased == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1u64 << 52), biased - 1075)
    };
    let (digits, exp10) = exact_digits(m, e);
    Exact { digits, exp10 }
}

/// The exact decimal expansion of `m * 2^e` (`m != 0`): the significant digits (values
/// 0-9, no leading or trailing zeros) and `exp10` such that the value is
/// `0.d[0] d[1] ... * 10^exp10`. Also used by `utils::number_utils` to decide whether a
/// parsed decimal is exact.
pub(crate) fn exact_digits(m: u64, e: i32) -> (Vec<u8>, i32) {
    let (mut m, mut e) = (m, e);
    debug_assert!(m != 0, "exact_digits of zero");
    let zeros = m.trailing_zeros();
    m >>= zeros;
    e += zeros as i32;

    let mut n = Big::from_u64(m);
    let scale10 = if e >= 0 {
        n.shl(e as u32);
        0
    } else {
        // m * 2^e = m * 5^-e / 10^-e
        n.mul_pow5((-e) as u32);
        e
    };
    let mut digits = n.into_decimal();
    let exp10 = digits.len() as i32 + scale10;
    while digits.last() == Some(&0) {
        digits.pop();
    }
    (digits, exp10)
}

/// Rounds `exact` to a multiple of `10^unit_exp10`, halves to even, and returns the multiple
/// as decimal digits (most significant first; empty for zero).
fn round_to_unit(exact: &Exact, unit_exp10: i32) -> Vec<u8> {
    let keep = exact.exp10 - unit_exp10;
    let d = &exact.digits;
    if keep < 0 {
        // |value| < 10^(unit - 1): less than half a unit.
        return Vec::new();
    }
    let keep = keep as usize;
    if keep >= d.len() {
        let mut q = d.clone();
        q.resize(keep, 0);
        return q;
    }
    let mut q = d[..keep].to_vec();
    let next = d[keep];
    let sticky = d[keep + 1..].iter().any(|&x| x != 0);
    let odd = q.last().is_some_and(|&x| x % 2 == 1);
    if next > 5 || (next == 5 && (sticky || odd)) {
        // Increment with carry.
        let mut i = q.len();
        loop {
            if i == 0 {
                q.insert(0, 1);
                break;
            }
            i -= 1;
            if q[i] == 9 {
                q[i] = 0;
            } else {
                q[i] += 1;
                break;
            }
        }
    }
    // Strip leading zeros (a zero quotient is empty).
    let first = q.iter().position(|&x| x != 0).unwrap_or(q.len());
    q.drain(..first);
    q
}

// ---------------------------------------------------------------------------------------
// printf
// ---------------------------------------------------------------------------------------

/// A floating-point conversion specifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conv {
    /// `%e`
    E,
    /// `%E`
    UpperE,
    /// `%f`
    F,
    /// `%F`
    UpperF,
    /// `%g`
    G,
    /// `%G`
    UpperG,
}

impl Conv {
    fn upper(self) -> bool {
        matches!(self, Conv::UpperE | Conv::UpperF | Conv::UpperG)
    }
}

/// A `printf` conversion for one `double`: `%[flags][width][.precision]conv`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    /// The conversion.
    pub conv: Conv,
    /// The precision; `None` means the default (6).
    pub precision: Option<usize>,
    /// The minimum field width.
    pub width: usize,
    /// `-`: left-justify.
    pub left: bool,
    /// `+`: always print a sign.
    pub plus: bool,
    /// ` `: print a space where a `+` would go.
    pub space: bool,
    /// `#`: the alternative form (keep the decimal point, and trailing zeros for `%g`).
    pub alt: bool,
    /// `0`: pad with zeros after the sign.
    pub zero: bool,
}

impl Spec {
    /// A conversion with the given precision and no flags or width.
    pub const fn new(conv: Conv, precision: usize) -> Spec {
        Spec {
            conv,
            precision: Some(precision),
            width: 0,
            left: false,
            plus: false,
            space: false,
            alt: false,
            zero: false,
        }
    }

    /// Parses a format with exactly one floating-point conversion, e.g. `"%.7g"` or
    /// `"%-+#012.3e"`. Returns `None` for anything else (including `*`, length modifiers and
    /// surrounding text).
    pub fn parse(format: &str) -> Option<Spec> {
        let mut rest = format.strip_prefix('%')?.as_bytes();
        let mut spec = Spec::new(Conv::G, 0);
        spec.precision = None;
        while let Some((&c, tail)) = rest.split_first() {
            match c {
                b'-' => spec.left = true,
                b'+' => spec.plus = true,
                b' ' => spec.space = true,
                b'#' => spec.alt = true,
                b'0' => spec.zero = true,
                _ => break,
            }
            rest = tail;
        }
        let digits = |r: &mut &[u8]| -> Option<usize> {
            let n = r.iter().take_while(|c| c.is_ascii_digit()).count();
            let text = std::str::from_utf8(&r[..n]).ok()?;
            *r = &r[n..];
            if n == 0 { Some(0) } else { text.parse().ok() }
        };
        spec.width = digits(&mut rest)?;
        if let Some(tail) = rest.strip_prefix(b".") {
            rest = tail;
            spec.precision = Some(digits(&mut rest)?);
        }
        spec.conv = match rest {
            b"e" => Conv::E,
            b"E" => Conv::UpperE,
            b"f" => Conv::F,
            b"F" => Conv::UpperF,
            b"g" => Conv::G,
            b"G" => Conv::UpperG,
            _ => return None,
        };
        Some(spec)
    }
}

/// Writes the exponent part `e+XX` (at least two digits).
fn push_exponent(out: &mut String, upper: bool, x: i32) {
    out.push(if upper { 'E' } else { 'e' });
    out.push(if x < 0 { '-' } else { '+' });
    let _ = write!(out, "{:02}", x.unsigned_abs());
}

fn push_digits(out: &mut String, digits: &[u8]) {
    out.extend(digits.iter().map(|&d| char::from(b'0' + d)));
}

/// `%e` of a finite magnitude: returns the body without sign.
fn body_e(exact: Option<&Exact>, precision: usize, upper: bool, alt: bool) -> String {
    let (digits, x) = match exact {
        None => (vec![0; precision + 1], 0),
        Some(ex) => {
            let unit = ex.exp10 - 1 - precision as i32;
            let mut q = round_to_unit(ex, unit);
            let mut x = ex.exp10 - 1;
            if q.len() > precision + 1 {
                // Rounding carried into a new digit (9.99 -> 10.0).
                q.pop();
                x += 1;
            }
            (q, x)
        }
    };
    let mut out = String::new();
    push_digits(&mut out, &digits[..1]);
    if precision > 0 || alt {
        out.push('.');
    }
    push_digits(&mut out, &digits[1..]);
    push_exponent(&mut out, upper, x);
    out
}

/// `%f` of a finite magnitude: returns the body without sign.
fn body_f(exact: Option<&Exact>, precision: usize, alt: bool) -> String {
    let q = match exact {
        None => Vec::new(),
        Some(ex) => round_to_unit(ex, -(precision as i32)),
    };
    let mut out = String::new();
    if q.len() > precision {
        push_digits(&mut out, &q[..q.len() - precision]);
    } else {
        out.push('0');
    }
    if precision > 0 || alt {
        out.push('.');
    }
    let frac_len = q.len().min(precision);
    out.extend(std::iter::repeat_n('0', precision - frac_len));
    push_digits(&mut out, &q[q.len() - frac_len..]);
    out
}

/// `%g` of a finite magnitude before trailing-zero removal: returns the body without sign.
fn body_g(exact: Option<&Exact>, precision: usize, upper: bool, alt: bool) -> String {
    let p = precision.max(1);
    // The exponent X of the `%e` conversion with precision P - 1, after rounding.
    let x = match exact {
        None => 0,
        Some(ex) => {
            let q = round_to_unit(ex, ex.exp10 - p as i32);
            if q.len() > p { ex.exp10 } else { ex.exp10 - 1 }
        }
    };
    if (p as i32) > x && x >= -4 {
        body_f(exact, (p as i32 - 1 - x) as usize, alt)
    } else {
        body_e(exact, p - 1, upper, alt)
    }
}

/// glibc's and UCRT's `%g` trailing-zero removal (UCRT `crop_zeroes`): drops zeros at the end
/// of the fraction, then the decimal point if nothing is left after it.
fn crop_zeroes(body: &mut String) {
    let Some(point) = body.find('.') else {
        return;
    };
    let exp = body.find(['e', 'E']).unwrap_or(body.len());
    let frac_end = body[..exp].trim_end_matches('0').len();
    let frac_end = if frac_end == point + 1 {
        point
    } else {
        frac_end
    };
    body.replace_range(frac_end..exp, "");
}

/// UCRT `force_decimal_point`: inserts a '.' after the leading digits (or after the first
/// character when there are none, e.g. `inf` becomes `i.nf`), for `#` with precision 0.
fn ucrt_force_decimal_point(text: &mut String) {
    let bytes = text.as_bytes();
    let mut i = 0;
    if bytes.first().map(u8::to_ascii_lowercase) != Some(b'e') {
        loop {
            i += 1;
            if !bytes.get(i).is_some_and(u8::is_ascii_digit) {
                break;
            }
        }
    }
    let i = i.min(text.len());
    text.insert(i, '.');
}

/// The NaN/infinity spelling, without sign.
fn special_body(crt: Crt, bits: u64, upper: bool) -> &'static str {
    const QUIET: u64 = 1 << 51;
    let mantissa = bits & ((1u64 << 52) - 1);
    let negative = bits >> 63 == 1;
    let (lower, capital) = if mantissa == 0 {
        ("inf", "INF")
    } else {
        match crt {
            Crt::Glibc => ("nan", "NAN"),
            // __acrt_fp_classify (corecrt_internal_fltintrn.h:122-150 @ 10.0.22000.0)
            Crt::Ucrt if negative && mantissa == QUIET => ("nan(ind)", "NAN(IND)"),
            Crt::Ucrt if mantissa & QUIET != 0 => ("nan", "NAN"),
            Crt::Ucrt => ("nan(snan)", "NAN(SNAN)"),
        }
    };
    if upper { capital } else { lower }
}

/// `snprintf(buf, n, spec, value)` on the given C runtime.
pub fn sprintf(crt: Crt, spec: &Spec, value: f64) -> String {
    let upper = spec.conv.upper();
    let bits = value.to_bits();
    let negative = bits >> 63 == 1;
    let finite = value.is_finite();
    // UCRT raises a precision of 0 to 1 for %g before formatting, so its `#` rule for
    // precision 0 never applies to %g; the default is 6 for every conversion here.
    let mut precision = spec.precision.unwrap_or(6);
    let is_g = matches!(spec.conv, Conv::G | Conv::UpperG);
    if is_g && precision == 0 {
        precision = 1;
    }

    let mut body = if !finite {
        special_body(crt, bits, upper).to_string()
    } else {
        let exact = (value != 0.0).then(|| exact_decimal(value));
        match spec.conv {
            Conv::E | Conv::UpperE => body_e(exact.as_ref(), precision, upper, spec.alt),
            Conv::F | Conv::UpperF => body_f(exact.as_ref(), precision, spec.alt),
            Conv::G | Conv::UpperG => body_g(exact.as_ref(), precision, upper, spec.alt),
        }
    };
    if is_g && !spec.alt {
        crop_zeroes(&mut body);
    }

    let mut sign_negative = negative;
    let mut zero_pad = spec.zero && !spec.left;
    match crt {
        Crt::Glibc => {
            if !finite {
                zero_pad = false;
            }
        }
        Crt::Ucrt => {
            // corecrt_internal_stdio_output.h type_case_a (10.0.22000.0): the '#' rule runs
            // on the converted text including its '-', then the sign is split off, and
            // special strings are padded with spaces.
            if spec.alt && precision == 0 && !finite {
                let mut text = if negative {
                    format!("-{body}")
                } else {
                    body.clone()
                };
                ucrt_force_decimal_point(&mut text);
                sign_negative = text.starts_with('-');
                body = text.strip_prefix('-').unwrap_or(&text).to_string();
            }
            if body.starts_with(['i', 'I', 'n', 'N']) {
                zero_pad = false;
            }
        }
    }

    let sign = if sign_negative {
        "-"
    } else if spec.plus {
        "+"
    } else if spec.space {
        " "
    } else {
        ""
    };
    let len = sign.len() + body.len();
    let pad = spec.width.saturating_sub(len);
    let mut out = String::with_capacity(len + pad);
    if spec.left {
        out.push_str(sign);
        out.push_str(&body);
        out.extend(std::iter::repeat_n(' ', pad));
    } else if zero_pad {
        out.push_str(sign);
        out.extend(std::iter::repeat_n('0', pad));
        out.push_str(&body);
    } else {
        out.extend(std::iter::repeat_n(' ', pad));
        out.push_str(sign);
        out.push_str(&body);
    }
    out
}

/// `snprintf` with a format string holding one floating-point conversion (see
/// [`Spec::parse`]). Panics on any other format.
pub fn format(crt: Crt, format: &str, value: f64) -> String {
    let spec = Spec::parse(format).unwrap_or_else(|| panic!("unsupported format {format:?}"));
    sprintf(crt, &spec, value)
}

/// `std::to_string(double)`: `%f` on both libraries.
pub fn to_string_f64(crt: Crt, value: f64) -> String {
    sprintf(crt, &Spec::new(Conv::F, 6), value)
}

/// `std::to_string(float)`: the float is promoted to `double`.
pub fn to_string_f32(crt: Crt, value: f32) -> String {
    to_string_f64(crt, f64::from(value))
}

// ---------------------------------------------------------------------------------------
// iostreams
// ---------------------------------------------------------------------------------------

/// `ios_base::floatfield`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatField {
    /// Neither `fixed` nor `scientific` (`std::defaultfloat`): `%g`.
    Default,
    /// `std::fixed`: `%f`.
    Fixed,
    /// `std::scientific`: `%e`.
    Scientific,
}

/// `ios_base::adjustfield`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adjust {
    /// No flag, or `std::right`: fill before the text.
    Right,
    /// `std::left`: fill after the text.
    Left,
    /// `std::internal`: fill after the sign (or after `0x`).
    Internal,
}

/// `ios_base::basefield` for integers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    /// `std::dec`
    Dec,
    /// `std::hex`
    Hex,
    /// `std::oct`
    Oct,
}

/// A `std::ostringstream` imbued with the classic ("C") locale, as OCIO creates them, limited
/// to the operations OCIO uses: numbers and strings, with precision, `floatfield`,
/// `showpoint`, `showpos`, `uppercase`, `basefield`, `showbase`, width, fill and
/// `adjustfield`. Width resets to 0 after every insertion, as in C++.
#[derive(Debug, Clone)]
pub struct OStringStream {
    crt: Crt,
    buf: String,
    /// `precision()`; a new stream has 6.
    pub precision: i64,
    /// The `floatfield` flags.
    pub float_field: FloatField,
    /// `std::showpoint`.
    pub showpoint: bool,
    /// `std::showpos`.
    pub showpos: bool,
    /// `std::uppercase`.
    pub uppercase: bool,
    /// The `basefield` flags.
    pub base: Base,
    /// `std::showbase`.
    pub showbase: bool,
    /// `width()`: applies to the next insertion only.
    pub width: i64,
    /// `fill()`; a new stream has `' '`.
    pub fill: char,
    /// The `adjustfield` flags.
    pub adjust: Adjust,
}

impl OStringStream {
    /// An empty stream with the default formatting state.
    pub fn new(crt: Crt) -> OStringStream {
        OStringStream {
            crt,
            buf: String::new(),
            precision: 6,
            float_field: FloatField::Default,
            showpoint: false,
            showpos: false,
            uppercase: false,
            base: Base::Dec,
            showbase: false,
            width: 0,
            fill: ' ',
            adjust: Adjust::Right,
        }
    }

    /// `str()`: the text written so far.
    pub fn str(&self) -> &str {
        &self.buf
    }

    /// `str("")`: clears the text, keeping the formatting state.
    pub fn clear_str(&mut self) {
        self.buf.clear();
    }

    /// Consumes the stream, returning its text.
    pub fn into_string(self) -> String {
        self.buf
    }

    /// Appends `text` padded to `width()` (the stream's `_M_pad` / `_Rep`), with the fill
    /// inserted after `prefix_len` bytes for `std::internal`; then resets the width.
    fn put_padded(&mut self, text: &str, prefix_len: usize) {
        let len = text.chars().count();
        let width = usize::try_from(self.width).unwrap_or(0);
        let pad = width.saturating_sub(len);
        let fill = std::iter::repeat_n(self.fill, pad);
        match self.adjust {
            Adjust::Left => {
                self.buf.push_str(text);
                self.buf.extend(fill);
            }
            Adjust::Internal => {
                self.buf.push_str(&text[..prefix_len]);
                self.buf.extend(fill);
                self.buf.push_str(&text[prefix_len..]);
            }
            Adjust::Right => {
                self.buf.extend(fill);
                self.buf.push_str(text);
            }
        }
        self.width = 0;
    }

    /// `os << value` for a `double`.
    ///
    /// libstdc++ `num_put::_M_insert_float` / `__num_base::_S_format_float`, and MSVC
    /// `num_put::do_put(double)` / `_Ffmt` / `_Fput`: the format is `%[+][#].*<conv>` with the
    /// stream's precision (6 when negative), conv `f` for fixed (MSVC: `F` with
    /// `uppercase`), `e`/`E` for scientific and `g`/`G` otherwise.
    pub fn put_f64(&mut self, value: f64) {
        let conv = match (self.float_field, self.uppercase) {
            (FloatField::Fixed, true) if self.crt == Crt::Ucrt => Conv::UpperF,
            (FloatField::Fixed, _) => Conv::F,
            (FloatField::Scientific, false) => Conv::E,
            (FloatField::Scientific, true) => Conv::UpperE,
            (FloatField::Default, false) => Conv::G,
            (FloatField::Default, true) => Conv::UpperG,
        };
        let precision = usize::try_from(self.precision).unwrap_or(6);
        let spec = Spec {
            plus: self.showpos,
            alt: self.showpoint,
            ..Spec::new(conv, precision)
        };
        let text = sprintf(self.crt, &spec, value);
        let prefix = usize::from(text.starts_with(['+', '-']));
        self.put_padded(&text, prefix);
    }

    /// `os << value` for a `float`: promoted to `double` by both libraries.
    pub fn put_f32(&mut self, value: f32) {
        self.put_f64(f64::from(value));
    }

    /// `os << value` for an unsigned integer.
    pub fn put_u64(&mut self, value: u64) {
        let text = match (self.base, self.uppercase) {
            (Base::Dec, _) => value.to_string(),
            (Base::Hex, false) => format!("{value:x}"),
            (Base::Hex, true) => format!("{value:X}"),
            (Base::Oct, _) => format!("{value:o}"),
        };
        let (prefix, prefix_len) = match self.base {
            Base::Hex if self.showbase && value != 0 => {
                (if self.uppercase { "0X" } else { "0x" }, 2)
            }
            Base::Oct if self.showbase && value != 0 => ("0", 0),
            Base::Dec if self.showpos => ("+", 1),
            _ => ("", 0),
        };
        self.put_padded(&format!("{prefix}{text}"), prefix_len);
    }

    /// `os << value` for a signed integer of `bits` bits (hex and octal print the two's
    /// complement bit pattern of that width, as C++ does).
    fn put_signed(&mut self, value: i64, bits: u32) {
        if self.base == Base::Dec {
            let text = if value < 0 || !self.showpos {
                value.to_string()
            } else {
                format!("+{value}")
            };
            let prefix = usize::from(text.starts_with(['+', '-']));
            self.put_padded(&text, prefix);
        } else {
            let mask = if bits == 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            self.put_u64(value as u64 & mask);
        }
    }

    /// `os << value` for a 64-bit signed integer (`long` on Linux, `long long`).
    pub fn put_i64(&mut self, value: i64) {
        self.put_signed(value, 64);
    }

    /// `os << value` for an `int`.
    pub fn put_i32(&mut self, value: i32) {
        self.put_signed(i64::from(value), 32);
    }

    /// `os << value` for an `unsigned int`.
    pub fn put_u32(&mut self, value: u32) {
        self.put_u64(u64::from(value));
    }

    /// `os << text` for a string: padded to the width, which then resets.
    pub fn put_str(&mut self, text: &str) {
        self.put_padded(text, 0);
    }
}

#[cfg(test)]
mod tests {
    //! Unit checks of the exact expansion against the C runtime. The formatted output of
    //! every conversion is compared with the C runtime in `tests/cfmt_crt.rs`.
    use super::*;
    use ocio_testkit::crt;
    use ocio_testkit::probe::Rng;

    /// The C runtime prints the exact decimal expansion when asked for enough digits: `%.Ne`
    /// with N + 1 = the number of significant digits has nothing left to round.
    fn crt_exact(value: f64, digits: usize) -> (Vec<u8>, i32) {
        let text = crt::format_f64(&format!("%.{}e", digits - 1), value.abs());
        let (mantissa, exponent) = text.split_once('e').expect("an exponent");
        let digits = mantissa
            .bytes()
            .filter(u8::is_ascii_digit)
            .map(|b| b - b'0')
            .collect();
        (digits, exponent.parse::<i32>().expect("an integer") + 1)
    }

    fn check_exact(value: f64) {
        let ex = exact_decimal(value);
        let (digits, exp10) = crt_exact(value, ex.digits.len());
        assert_eq!(ex.digits, digits, "{value:e}");
        assert_eq!(ex.exp10, exp10, "{value:e}");
        // One more digit from the C runtime must be a zero: the expansion ended.
        let (longer, _) = crt_exact(value, ex.digits.len() + 1);
        assert_eq!(longer.last(), Some(&0), "{value:e}");
    }

    #[test]
    fn exact_decimal_matches_the_crt() {
        let mut rng = Rng::new(0x0cf4);
        for _ in 0..3000 {
            let v = f64::from_bits(rng.next_u64());
            if v.is_finite() && v != 0.0 {
                check_exact(v);
            }
            let f = f32::from_bits(rng.next_u64() as u32);
            if f.is_finite() && f != 0.0 {
                check_exact(f64::from(f));
            }
        }
        for bits in [1, 2, 3, (1 << 52) - 1, 1 << 52, 0x7fef_ffff_ffff_ffff] {
            check_exact(f64::from_bits(bits));
        }
    }

    #[test]
    fn parse_reads_every_part_of_the_spec() {
        let spec = Spec::parse("%-+ #012.7e").expect("valid");
        assert_eq!(spec.conv, Conv::E);
        assert_eq!(spec.precision, Some(7));
        assert_eq!(spec.width, 12);
        assert!(spec.left && spec.plus && spec.space && spec.alt && spec.zero);
        assert_eq!(Spec::parse("%g").map(|s| s.precision), Some(None));
        assert_eq!(Spec::parse("%.g").map(|s| s.precision), Some(Some(0)));
        assert!(Spec::parse("%.*g").is_none());
        assert!(Spec::parse("%lf").is_none());
        assert!(Spec::parse("x%g").is_none());
    }
}
