// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/GpuShaderUtils.h` and `GpuShaderUtils.cpp` @ v2.5.2:
//! [`GpuShaderText`], which writes a shader program line by line in any of the ten shading
//! languages, and the helpers the GPU writers share.
//!
//! **Text is bytes**, like the rest of OCIO's string data (docs/architecture.md, "Strings are
//! bytes"): the names in a shader come from the shader description, which takes any bytes.
//! Functions that take names or pieces of code accept `&str` or `&[u8]`, and functions that
//! build code return `Vec<u8>`.
//!
//! **Numbers** are written as upstream writes them:
//! - `float` and `double` through [`get_float_string`], i.e. C++ iostreams, whose C runtime
//!   spells NaN its own way on each platform (PLAN.md D12);
//! - `unsigned` and `int` in decimal.
//!
//! **Overloads.** Where upstream overloads a method on `float`, `double` and `std::string`,
//! each overload is its own method here (`float3_const_f32`, `float3_const_f64`,
//! `float3_const`). A C++ caller that passes a `double` to a `float` overload converts it to
//! `float` implicitly; the Rust caller has to write the conversion, so the literal's
//! precision (9 or 17 digits) is visible at every call site.
//!
//! **Languages.** Upstream's `switch` statements end with a `default:` arm that throws
//! "Unknown GPU shader language." for a value outside the enumerators. [`GpuLanguage`] holds
//! only upstream's enumerators, so those arms can't be reached here.

use std::cell::{Cell, RefCell};

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::math_utils::clamp_to_norm_half;
use ocio_ops::{Exception, Result};

use crate::open_color_types::GpuLanguage;

/// Upstream's message for an empty variable name.
const EMPTY_NAME: &str = "GPU variable name is empty.";

/// Concatenates byte strings, as upstream's `std::string` `+` does.
fn cat<const N: usize>(parts: [&[u8]; N]) -> Vec<u8> {
    parts.concat()
}

/// Upstream's check that a variable name isn't empty.
fn check_name(name: &[u8]) -> Result<()> {
    if name.is_empty() {
        return Err(Exception::new(EMPTY_NAME));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Float literals
// ---------------------------------------------------------------------------------------

/// A floating-point type upstream writes into shader code: `float` or `double`.
pub trait ShaderFloat: Copy {
    /// `std::numeric_limits<T>::max_digits10`: the digits that write any value losslessly.
    const MAX_DIGITS10: i64;

    /// `(T)ClampToNormHalf(v)`. `ClampToNormHalf` takes and returns a `double`
    /// (src/OpenColorIO/MathUtils.h:117 @ v2.5.2), so a `float` goes there and back.
    fn clamp_to_norm_half(self) -> Self;

    /// The fractional part `std::modf` returns: `self - trunc(self)` with the sign of `self`,
    /// a zero of that sign for an infinity, and NaN for NaN.
    fn modf_frac(self) -> Self;

    /// `value == (T)0`.
    fn is_zero(self) -> bool;

    /// `std::isfinite(value)`.
    fn is_finite_value(self) -> bool;

    /// `oss << value`: a `float` is promoted to `double` by both C++ libraries.
    fn put(self, oss: &mut OStringStream);
}

impl ShaderFloat for f32 {
    const MAX_DIGITS10: i64 = 9;

    fn clamp_to_norm_half(self) -> f32 {
        clamp_to_norm_half(f64::from(self)) as f32
    }

    fn modf_frac(self) -> f32 {
        if self.is_infinite() {
            0.0f32.copysign(self)
        } else {
            (self - self.trunc()).copysign(self)
        }
    }

    fn is_zero(self) -> bool {
        self == 0.0f32
    }

    fn is_finite_value(self) -> bool {
        self.is_finite()
    }

    fn put(self, oss: &mut OStringStream) {
        oss.put_f32(self);
    }
}

impl ShaderFloat for f64 {
    const MAX_DIGITS10: i64 = 17;

    fn clamp_to_norm_half(self) -> f64 {
        clamp_to_norm_half(self)
    }

    fn modf_frac(self) -> f64 {
        if self.is_infinite() {
            0.0f64.copysign(self)
        } else {
            (self - self.trunc()).copysign(self)
        }
    }

    fn is_zero(self) -> bool {
        self == 0.0f64
    }

    fn is_finite_value(self) -> bool {
        self.is_finite()
    }

    fn put(self, oss: &mut OStringStream) {
        oss.put_f64(self);
    }
}

/// A `float` or `double` as a shader literal: all the digits that write it losslessly
/// (`%.9g` or `%.17g`), and a `.` when the value is a whole number, so that the shader reads
/// it as floating point. For Cg, the value is first clamped to the range of normal halfs.
///
/// Infinities and NaN are written as the C runtime spells them (`inf`, `-nan(ind)` on
/// Windows, `-nan` on Linux), without a `.`. Large whole numbers get a `.` after their
/// exponent (`1e+10.`), as upstream writes them.
///
/// Port of `getFloatString` (GpuShaderUtils.cpp:21-35 @ v2.5.2).
pub fn get_float_string<T: ShaderFloat>(v: T, lang: GpuLanguage) -> String {
    let value = if lang == GpuLanguage::Cg {
        v.clamp_to_norm_half()
    } else {
        v
    };

    let fracpart = value.modf_frac();

    let mut oss = OStringStream::new(Crt::NATIVE);
    oss.precision = T::MAX_DIGITS10;
    value.put(&mut oss);
    oss.put_str(if fracpart.is_zero() && value.is_finite_value() {
        "."
    } else {
        ""
    });
    oss.into_string()
}

/// The keyword of a vector of `n` floats.
///
/// Port of `getVecKeyword<N>` (GpuShaderUtils.cpp:37-77 @ v2.5.2).
fn get_vec_keyword(n: u32, lang: GpuLanguage) -> String {
    match lang {
        GpuLanguage::Glsl1_2
        | GpuLanguage::Glsl1_3
        | GpuLanguage::Glsl4_0
        | GpuLanguage::GlslVk4_6
        | GpuLanguage::GlslEs1_0
        | GpuLanguage::GlslEs3_0 => format!("vec{n}"),
        GpuLanguage::Cg => format!("half{n}"),
        GpuLanguage::Msl2_0 | GpuLanguage::HlslSm5_0 => format!("float{n}"),
        GpuLanguage::Osl1 => format!("vector{n}"),
    }
}

// ---------------------------------------------------------------------------------------
// Lines
// ---------------------------------------------------------------------------------------

/// A value a shader line takes: the operands of upstream's `GpuShaderLine::operator<<`
/// overloads (GpuShaderUtils.cpp:252-289 @ v2.5.2).
pub trait ShaderArg {
    /// Appends the value's code in `lang` to `out`.
    fn write_to(&self, lang: GpuLanguage, out: &mut Vec<u8>);
}

/// `operator<<(const char *)`: the text itself.
impl ShaderArg for str {
    fn write_to(&self, _lang: GpuLanguage, out: &mut Vec<u8>) {
        out.extend_from_slice(self.as_bytes());
    }
}

/// `operator<<(const std::string &)`: the text itself.
impl ShaderArg for String {
    fn write_to(&self, _lang: GpuLanguage, out: &mut Vec<u8>) {
        out.extend_from_slice(self.as_bytes());
    }
}

/// `operator<<(const std::string &)`, for text that isn't UTF-8.
impl ShaderArg for [u8] {
    fn write_to(&self, _lang: GpuLanguage, out: &mut Vec<u8>) {
        out.extend_from_slice(self);
    }
}

/// `operator<<(const std::string &)`, for text that isn't UTF-8.
impl ShaderArg for Vec<u8> {
    fn write_to(&self, _lang: GpuLanguage, out: &mut Vec<u8>) {
        out.extend_from_slice(self);
    }
}

/// `operator<<(float)`: [`get_float_string`].
impl ShaderArg for f32 {
    fn write_to(&self, lang: GpuLanguage, out: &mut Vec<u8>) {
        out.extend_from_slice(get_float_string(*self, lang).as_bytes());
    }
}

/// `operator<<(double)`: [`get_float_string`].
impl ShaderArg for f64 {
    fn write_to(&self, lang: GpuLanguage, out: &mut Vec<u8>) {
        out.extend_from_slice(get_float_string(*self, lang).as_bytes());
    }
}

/// `operator<<(unsigned)`: decimal.
impl ShaderArg for u32 {
    fn write_to(&self, _lang: GpuLanguage, out: &mut Vec<u8>) {
        out.extend_from_slice(self.to_string().as_bytes());
    }
}

/// `operator<<(int)`: decimal.
impl ShaderArg for i32 {
    fn write_to(&self, _lang: GpuLanguage, out: &mut Vec<u8>) {
        out.extend_from_slice(self.to_string().as_bytes());
    }
}

impl<T: ShaderArg + ?Sized> ShaderArg for &T {
    fn write_to(&self, lang: GpuLanguage, out: &mut Vec<u8>) {
        (**self).write_to(lang, out);
    }
}

/// A line of shader code being written. It goes into its text when it is dropped, with the
/// text's indentation at that moment: `ss.new_line().put(a).put(b);` writes one line, as
/// upstream's `ss.newLine() << a << b;` does when the temporary line is destroyed. A line
/// that isn't finished because an error returned early is written too, as a C++ exception
/// unwinding through the temporary writes it.
///
/// Port of `GpuShaderText::GpuShaderLine` (GpuShaderUtils.h:22-45,
/// GpuShaderUtils.cpp:239-298 @ v2.5.2). Its `operator<<` overloads are
/// [`GpuShaderLine::put`], through [`ShaderArg`]. Its copy constructor and `operator=` have
/// no counterpart: upstream never copies a line.
#[derive(Debug)]
pub struct GpuShaderLine<'a> {
    /// `m_text`, never null here.
    text: &'a GpuShaderText,
}

impl GpuShaderLine<'_> {
    /// `line << value`: appends the value's code to the line.
    ///
    /// Port of `GpuShaderLine::operator<<` (GpuShaderUtils.cpp:252-289 @ v2.5.2).
    pub fn put<T: ShaderArg>(&mut self, value: T) -> &mut Self {
        value.write_to(self.text.lang, &mut self.text.line.borrow_mut());
        self
    }
}

impl Drop for GpuShaderLine<'_> {
    /// Port of `GpuShaderLine::~GpuShaderLine` (GpuShaderUtils.cpp:244-250 @ v2.5.2).
    fn drop(&mut self) {
        self.text.flush_line();
    }
}

// ---------------------------------------------------------------------------------------
// GpuShaderText
// ---------------------------------------------------------------------------------------

/// Writes a shader program line by line in one language.
///
/// Upstream writes a line through a `GpuShaderLine` that points back at the text, while other
/// methods of the same text build the line's pieces (`ss.newLine() << ss.floatKeyword()`).
/// So the state is in cells, and every method takes `&self`.
///
/// Port of `GpuShaderText` (GpuShaderUtils.h:15-273 @ v2.5.2).
#[derive(Debug)]
pub struct GpuShaderText {
    /// `m_lang`.
    lang: GpuLanguage,
    /// `m_ossText`: the text so far.
    text: RefCell<Vec<u8>>,
    /// `m_ossLine`: the line being written, shared by every `GpuShaderLine` of this text.
    line: RefCell<Vec<u8>>,
    /// `m_indent`: the indentation level of the lines written from now on.
    indent: Cell<u32>,
}

impl GpuShaderText {
    /// An empty text in `lang`, at indentation 0.
    ///
    /// Upstream sets the precision of both of its streams to 16, but writes only text and
    /// integers into them (numbers go through [`get_float_string`]), so it shows nowhere.
    ///
    /// Port of `GpuShaderText::GpuShaderText` (GpuShaderUtils.cpp:300-306 @ v2.5.2).
    pub fn new(lang: GpuLanguage) -> GpuShaderText {
        GpuShaderText {
            lang,
            text: RefCell::new(Vec::new()),
            line: RefCell::new(Vec::new()),
            indent: Cell::new(0),
        }
    }

    /// The language the text is written in (`m_lang`, which upstream keeps private).
    pub fn language(&self) -> GpuLanguage {
        self.lang
    }

    /// Port of `GpuShaderText::setIndent` (GpuShaderUtils.cpp:308-311 @ v2.5.2).
    pub fn set_indent(&self, indent: u32) {
        self.indent.set(indent);
    }

    /// Port of `GpuShaderText::indent` (GpuShaderUtils.cpp:313-316 @ v2.5.2).
    pub fn indent(&self) {
        self.indent.set(self.indent.get().wrapping_add(1));
    }

    /// Upstream's level is `unsigned`: going below 0 wraps to 2^32 - 1, and the next line
    /// written then asks for a 4 GiB indentation (as upstream's does). No upstream code
    /// dedents more than it indents.
    ///
    /// Port of `GpuShaderText::dedent` (GpuShaderUtils.cpp:318-321 @ v2.5.2).
    pub fn dedent(&self) {
        self.indent.set(self.indent.get().wrapping_sub(1));
    }

    /// A new line, written into the text when it is dropped.
    ///
    /// Port of `GpuShaderText::newLine` (GpuShaderUtils.cpp:323-326 @ v2.5.2).
    pub fn new_line(&self) -> GpuShaderLine<'_> {
        GpuShaderLine { text: self }
    }

    /// The code written so far.
    ///
    /// Port of `GpuShaderText::string` (GpuShaderUtils.cpp:328-331 @ v2.5.2).
    pub fn string(&self) -> Vec<u8> {
        self.text.borrow().clone()
    }

    /// Writes the current line into the text: two spaces per indentation level, the line,
    /// and a newline (`std::endl` writes `\n` into a string stream on both platforms). The
    /// indentation is `unsigned` arithmetic, which wraps.
    ///
    /// Port of `GpuShaderText::flushLine` (GpuShaderUtils.cpp:333-343 @ v2.5.2).
    fn flush_line(&self) {
        const TAB_SIZE: u32 = 2;

        let line = std::mem::take(&mut *self.line.borrow_mut());
        let mut text = self.text.borrow_mut();
        let spaces = TAB_SIZE.wrapping_mul(self.indent.get()) as usize;
        let len = text.len();
        text.resize(len + spaces, b' ');
        text.extend_from_slice(&line);
        text.push(b'\n');
    }

    /// `const ` in the languages that have constants, `static const ` in HLSL, and nothing in
    /// OSL and Cg.
    ///
    /// Port of `GpuShaderText::constKeyword` (GpuShaderUtils.cpp:345-375 @ v2.5.2).
    pub fn const_keyword(&self) -> &'static str {
        match self.lang {
            GpuLanguage::Glsl1_2
            | GpuLanguage::Glsl1_3
            | GpuLanguage::Glsl4_0
            | GpuLanguage::GlslVk4_6
            | GpuLanguage::GlslEs1_0
            | GpuLanguage::GlslEs3_0
            | GpuLanguage::Msl2_0 => "const ",
            GpuLanguage::HlslSm5_0 => "static const ",
            GpuLanguage::Osl1 | GpuLanguage::Cg => "",
        }
    }

    /// Port of `GpuShaderText::floatKeyword` (GpuShaderUtils.cpp:377-380 @ v2.5.2).
    pub fn float_keyword(&self) -> &'static str {
        if self.lang == GpuLanguage::Cg {
            "half"
        } else {
            "float"
        }
    }

    /// Port of `GpuShaderText::floatKeywordConst` (GpuShaderUtils.cpp:382-390 @ v2.5.2).
    pub fn float_keyword_const(&self) -> String {
        let mut s = String::new();
        s += self.const_keyword();
        s += self.float_keyword();
        s
    }

    /// Port of `GpuShaderText::floatDecl` (GpuShaderUtils.cpp:392-400 @ v2.5.2).
    pub fn float_decl(&self, name: impl AsRef<[u8]>) -> Result<Vec<u8>> {
        let name = name.as_ref();
        check_name(name)?;
        Ok(cat([self.float_keyword().as_bytes(), b" ", name]))
    }

    /// Port of `GpuShaderText::intKeyword` (GpuShaderUtils.cpp:402-405 @ v2.5.2).
    pub fn int_keyword(&self) -> &'static str {
        "int"
    }

    /// Port of `GpuShaderText::intKeywordConst` (GpuShaderUtils.cpp:407-415 @ v2.5.2).
    pub fn int_keyword_const(&self) -> String {
        let mut s = String::new();
        s += self.const_keyword();
        s += self.int_keyword();
        s
    }

    /// Port of `GpuShaderText::intDecl` (GpuShaderUtils.cpp:417-425 @ v2.5.2).
    pub fn int_decl(&self, name: impl AsRef<[u8]>) -> Result<Vec<u8>> {
        let name = name.as_ref();
        check_name(name)?;
        Ok(cat([self.int_keyword().as_bytes(), b" ", name]))
    }

    /// `color` in OSL, the float3 keyword elsewhere.
    ///
    /// Port of `GpuShaderText::colorDecl` (GpuShaderUtils.cpp:427-435 @ v2.5.2).
    pub fn color_decl(&self, name: impl AsRef<[u8]>) -> Result<Vec<u8>> {
        let name = name.as_ref();
        check_name(name)?;
        let keyword = if self.lang == GpuLanguage::Osl1 {
            "color".to_string()
        } else {
            self.float3_keyword()
        };
        Ok(cat([keyword.as_bytes(), b" ", name]))
    }

    /// Port of `GpuShaderText::declareVarConst(const std::string &, float)`
    /// (GpuShaderUtils.cpp:437-440 @ v2.5.2).
    pub fn declare_var_const_f32(&self, name: impl AsRef<[u8]>, v: f32) -> Result<()> {
        self.new_line()
            .put(self.const_keyword())
            .put(self.declare_var_str_f32(name, v)?)
            .put(";");
        Ok(())
    }

    /// Port of `GpuShaderText::declareVar(const std::string &, float)`
    /// (GpuShaderUtils.cpp:442-445 @ v2.5.2).
    pub fn declare_var_f32(&self, name: impl AsRef<[u8]>, v: f32) -> Result<()> {
        self.new_line()
            .put(self.declare_var_str_f32(name, v)?)
            .put(";");
        Ok(())
    }

    /// `float name = v`. OSL has no infinities, so an infinity is written as the largest
    /// float of its sign, with 9 digits and no `.` (for Cg too, without the half clamp).
    ///
    /// Port of `GpuShaderText::declareVarStr(const std::string &, float)`
    /// (GpuShaderUtils.cpp:450-480 @ v2.5.2).
    pub fn declare_var_str_f32(&self, name: impl AsRef<[u8]>, v: f32) -> Result<Vec<u8>> {
        let name = name.as_ref();
        check_name(name)?;

        if v.is_infinite() {
            // `std::signbit(v)`. Upstream's `-1 * std::numeric_limits<float>::max()` converts
            // the `int` to `float`; the product is exactly `-FLT_MAX`.
            let new_val = if v.is_sign_negative() {
                -f32::MAX
            } else {
                f32::MAX
            };

            let mut oss = OStringStream::new(Crt::NATIVE);
            oss.precision = <f32 as ShaderFloat>::MAX_DIGITS10;
            oss.put_f32(new_val);

            return Ok(cat([&self.float_decl(name)?, b" = ", oss.str().as_bytes()]));
        }

        Ok(cat([
            &self.float_decl(name)?,
            b" = ",
            get_float_string(v, self.lang).as_bytes(),
        ]))
    }

    /// `lhs op rhs`, wrapped in `any( ... )` in MSL and HLSL, whose `if` conditions take no
    /// vector of booleans.
    ///
    /// Port of `GpuShaderText::vectorCompareExpression` (GpuShaderUtils.cpp:482-491 @ v2.5.2).
    pub fn vector_compare_expression(
        &self,
        lhs: impl AsRef<[u8]>,
        op: impl AsRef<[u8]>,
        rhs: impl AsRef<[u8]>,
    ) -> Vec<u8> {
        let mut ret = cat([lhs.as_ref(), b" ", op.as_ref(), b" ", rhs.as_ref()]);
        if self.lang == GpuLanguage::Msl2_0 || self.lang == GpuLanguage::HlslSm5_0 {
            ret = cat([b"any( ", &ret, b" )"]);
        }
        ret
    }

    /// Port of `GpuShaderText::declareVarConst(const std::string &, bool)`
    /// (GpuShaderUtils.cpp:493-496 @ v2.5.2).
    pub fn declare_var_const_bool(&self, name: impl AsRef<[u8]>, v: bool) -> Result<()> {
        self.new_line()
            .put(self.const_keyword())
            .put(self.declare_var_str_bool(name, v)?)
            .put(";");
        Ok(())
    }

    /// Port of `GpuShaderText::declareVar(const std::string &, bool)`
    /// (GpuShaderUtils.cpp:498-501 @ v2.5.2).
    pub fn declare_var_bool(&self, name: impl AsRef<[u8]>, v: bool) -> Result<()> {
        self.new_line()
            .put(self.declare_var_str_bool(name, v)?)
            .put(";");
        Ok(())
    }

    /// `bool name = true`; OSL has no booleans, so there `int name = 1`.
    ///
    /// Port of `GpuShaderText::declareVarStr(const std::string &, bool)`
    /// (GpuShaderUtils.cpp:503-518 @ v2.5.2).
    pub fn declare_var_str_bool(&self, name: impl AsRef<[u8]>, v: bool) -> Result<Vec<u8>> {
        let name = name.as_ref();
        check_name(name)?;

        if self.lang == GpuLanguage::Osl1 {
            Ok(cat([
                self.int_keyword().as_bytes(),
                b" ",
                name,
                b" = ",
                if v { b"1" } else { b"0" },
            ]))
        } else {
            Ok(cat([
                b"bool ",
                name,
                b" = ",
                if v { b"true" as &[u8] } else { b"false" },
            ]))
        }
    }

    /// A constant array of floats, on one line. The size is the slice's length (upstream's
    /// callers pass their vector's size as `int`).
    ///
    /// Port of `GpuShaderText::declareFloatArrayConst` (GpuShaderUtils.cpp:520-581 @ v2.5.2).
    pub fn declare_float_array_const(&self, name: impl AsRef<[u8]>, v: &[f32]) -> Result<()> {
        let name = name.as_ref();
        let size = v.len() as i32;
        if size == 0 {
            return Err(Exception::new("GPU array size is 0."));
        }
        check_name(name)?;

        let mut nl = self.new_line();

        let emit_array_values = |nl: &mut GpuShaderLine<'_>| {
            for (i, value) in v.iter().enumerate() {
                nl.put(get_float_string(*value, self.lang));
                if i + 1 != v.len() {
                    nl.put(", ");
                }
            }
        };

        match self.lang {
            GpuLanguage::Glsl1_2
            | GpuLanguage::Glsl1_3
            | GpuLanguage::Glsl4_0
            | GpuLanguage::GlslVk4_6
            | GpuLanguage::GlslEs1_0
            | GpuLanguage::GlslEs3_0 => {
                nl.put(self.float_keyword_const())
                    .put(" ")
                    .put(name)
                    .put("[")
                    .put(size)
                    .put("] = ");
                nl.put(self.float_keyword()).put("[").put(size).put("](");
                emit_array_values(&mut nl);
                nl.put(");");
            }
            GpuLanguage::Osl1 | GpuLanguage::Cg | GpuLanguage::HlslSm5_0 => {
                nl.put(self.float_keyword_const());
                nl.put(" ").put(name).put("[").put(size).put("] = {");
                emit_array_values(&mut nl);
                nl.put("};");
            }
            GpuLanguage::Msl2_0 => {
                nl.put("constant constexpr static float");
                nl.put(" ").put(name).put("[").put(size).put("] = {");
                emit_array_values(&mut nl);
                nl.put("};");
            }
        }
        Ok(())
    }

    /// A constant array of ints, on one line. The size is the slice's length.
    ///
    /// Port of `GpuShaderText::declareIntArrayConst` (GpuShaderUtils.cpp:583-648 @ v2.5.2).
    pub fn declare_int_array_const(&self, name: impl AsRef<[u8]>, v: &[i32]) -> Result<()> {
        let name = name.as_ref();
        let size = v.len() as i32;
        if size == 0 {
            return Err(Exception::new("GPU array size is 0."));
        }
        check_name(name)?;

        let mut nl = self.new_line();

        let emit_array_values = |nl: &mut GpuShaderLine<'_>| {
            for (i, value) in v.iter().enumerate() {
                nl.put(*value);
                if i + 1 != v.len() {
                    nl.put(", ");
                }
            }
        };

        match self.lang {
            GpuLanguage::Glsl1_2
            | GpuLanguage::Glsl1_3
            | GpuLanguage::Glsl4_0
            | GpuLanguage::GlslVk4_6
            | GpuLanguage::GlslEs1_0
            | GpuLanguage::GlslEs3_0 => {
                nl.put(self.int_keyword_const())
                    .put(" ")
                    .put(name)
                    .put("[")
                    .put(size)
                    .put("] = ")
                    .put(self.int_keyword())
                    .put("[")
                    .put(size)
                    .put("](");
                emit_array_values(&mut nl);
                nl.put(");");
            }
            GpuLanguage::HlslSm5_0 => {
                nl.put(self.int_keyword_const());
                nl.put(" ").put(name).put("[").put(size).put("] = {");
                emit_array_values(&mut nl);
                nl.put("};");
            }
            GpuLanguage::Msl2_0 => {
                nl.put("constant constexpr static int");
                nl.put(" ").put(name).put("[").put(size).put("] = {");
                emit_array_values(&mut nl);
                nl.put("};");
            }
            GpuLanguage::Osl1 | GpuLanguage::Cg => {
                nl.put(self.int_keyword())
                    .put(" ")
                    .put(name)
                    .put("[")
                    .put(size)
                    .put("] = {");
                emit_array_values(&mut nl);
                nl.put("};");
            }
        }
        Ok(())
    }

    // -- Float2 -------------------------------------------------------------------------

    /// Port of `GpuShaderText::float2Keyword` (GpuShaderUtils.cpp:650-653 @ v2.5.2).
    pub fn float2_keyword(&self) -> String {
        get_vec_keyword(2, self.lang)
    }

    /// Port of `GpuShaderText::float2Const` (GpuShaderUtils.cpp:655-660 @ v2.5.2).
    pub fn float2_const(&self, x: impl AsRef<[u8]>, y: impl AsRef<[u8]>) -> Vec<u8> {
        cat([
            self.float2_keyword().as_bytes(),
            b"(",
            x.as_ref(),
            b", ",
            y.as_ref(),
            b")",
        ])
    }

    /// Port of `GpuShaderText::float2Decl` (GpuShaderUtils.cpp:662-670 @ v2.5.2).
    pub fn float2_decl(&self, name: impl AsRef<[u8]>) -> Result<Vec<u8>> {
        let name = name.as_ref();
        check_name(name)?;
        Ok(cat([self.float2_keyword().as_bytes(), b" ", name]))
    }

    // -- Float3 -------------------------------------------------------------------------

    /// `vector` in OSL, the vector keyword of 3 elsewhere.
    ///
    /// Port of `GpuShaderText::float3Keyword` (GpuShaderUtils.cpp:672-675 @ v2.5.2).
    pub fn float3_keyword(&self) -> String {
        if self.lang == GpuLanguage::Osl1 {
            "vector".to_string()
        } else {
            get_vec_keyword(3, self.lang)
        }
    }

    /// Port of `GpuShaderText::float3Const(float, float, float)`
    /// (GpuShaderUtils.cpp:677-682 @ v2.5.2).
    pub fn float3_const_f32(&self, x: f32, y: f32, z: f32) -> Vec<u8> {
        self.float3_const(
            get_float_string(x, self.lang),
            get_float_string(y, self.lang),
            get_float_string(z, self.lang),
        )
    }

    /// Port of `GpuShaderText::float3Const(double, double, double)`
    /// (GpuShaderUtils.cpp:684-689 @ v2.5.2).
    pub fn float3_const_f64(&self, x: f64, y: f64, z: f64) -> Vec<u8> {
        self.float3_const(
            get_float_string(x, self.lang),
            get_float_string(y, self.lang),
            get_float_string(z, self.lang),
        )
    }

    /// Port of `GpuShaderText::float3Const(const std::string &, ...)`
    /// (GpuShaderUtils.cpp:691-698 @ v2.5.2).
    pub fn float3_const(
        &self,
        x: impl AsRef<[u8]>,
        y: impl AsRef<[u8]>,
        z: impl AsRef<[u8]>,
    ) -> Vec<u8> {
        cat([
            self.float3_keyword().as_bytes(),
            b"(",
            x.as_ref(),
            b", ",
            y.as_ref(),
            b", ",
            z.as_ref(),
            b")",
        ])
    }

    /// The same value three times.
    ///
    /// Port of `GpuShaderText::float3Const(float)` (GpuShaderUtils.cpp:700-703 @ v2.5.2).
    pub fn float3_splat_f32(&self, v: f32) -> Vec<u8> {
        self.float3_splat(get_float_string(v, self.lang))
    }

    /// The same value three times.
    ///
    /// Port of `GpuShaderText::float3Const(double)` (GpuShaderUtils.cpp:705-708 @ v2.5.2).
    pub fn float3_splat_f64(&self, v: f64) -> Vec<u8> {
        self.float3_splat(get_float_string(v, self.lang))
    }

    /// The same value three times.
    ///
    /// Port of `GpuShaderText::float3Const(const std::string &)`
    /// (GpuShaderUtils.cpp:710-713 @ v2.5.2).
    pub fn float3_splat(&self, v: impl AsRef<[u8]>) -> Vec<u8> {
        let v = v.as_ref();
        self.float3_const(v, v, v)
    }

    /// Port of `GpuShaderText::float3Decl` (GpuShaderUtils.cpp:715-723 @ v2.5.2).
    pub fn float3_decl(&self, name: impl AsRef<[u8]>) -> Result<Vec<u8>> {
        let name = name.as_ref();
        check_name(name)?;
        Ok(cat([self.float3_keyword().as_bytes(), b" ", name]))
    }

    /// Port of `GpuShaderText::declareFloat3(const std::string &, float, float, float)`
    /// (GpuShaderUtils.cpp:725-730 @ v2.5.2).
    pub fn declare_float3_f32(&self, name: impl AsRef<[u8]>, x: f32, y: f32, z: f32) -> Result<()> {
        self.declare_float3(
            name,
            get_float_string(x, self.lang),
            get_float_string(y, self.lang),
            get_float_string(z, self.lang),
        )
    }

    /// Port of `GpuShaderText::declareFloat3(const std::string &, const Float3 &)`
    /// (GpuShaderUtils.cpp:732-735 @ v2.5.2).
    pub fn declare_float3_vec(&self, name: impl AsRef<[u8]>, vec3: &[f32; 3]) -> Result<()> {
        self.declare_float3_f32(name, vec3[0], vec3[1], vec3[2])
    }

    /// Port of `GpuShaderText::declareFloat3(const std::string &, double, double, double)`
    /// (GpuShaderUtils.cpp:737-743 @ v2.5.2).
    pub fn declare_float3_f64(&self, name: impl AsRef<[u8]>, x: f64, y: f64, z: f64) -> Result<()> {
        self.declare_float3(
            name,
            get_float_string(x, self.lang),
            get_float_string(y, self.lang),
            get_float_string(z, self.lang),
        )
    }

    /// Port of `GpuShaderText::declareFloat3(const std::string &, const std::string &, ...)`
    /// (GpuShaderUtils.cpp:745-751 @ v2.5.2).
    pub fn declare_float3(
        &self,
        name: impl AsRef<[u8]>,
        x: impl AsRef<[u8]>,
        y: impl AsRef<[u8]>,
        z: impl AsRef<[u8]>,
    ) -> Result<()> {
        self.new_line()
            .put(self.float3_decl(name)?)
            .put(" = ")
            .put(self.float3_const(x, y, z))
            .put(";");
        Ok(())
    }

    // -- Float4 -------------------------------------------------------------------------

    /// Port of `GpuShaderText::float4Keyword` (GpuShaderUtils.cpp:753-756 @ v2.5.2).
    pub fn float4_keyword(&self) -> String {
        get_vec_keyword(4, self.lang)
    }

    /// Port of `GpuShaderText::float4Const(float, float, float, float)`
    /// (GpuShaderUtils.cpp:758-764 @ v2.5.2).
    pub fn float4_const_f32(&self, x: f32, y: f32, z: f32, w: f32) -> Vec<u8> {
        self.float4_const(
            get_float_string(x, self.lang),
            get_float_string(y, self.lang),
            get_float_string(z, self.lang),
            get_float_string(w, self.lang),
        )
    }

    /// Port of `GpuShaderText::float4Const(double, double, double, double)`
    /// (GpuShaderUtils.cpp:766-772 @ v2.5.2).
    pub fn float4_const_f64(&self, x: f64, y: f64, z: f64, w: f64) -> Vec<u8> {
        self.float4_const(
            get_float_string(x, self.lang),
            get_float_string(y, self.lang),
            get_float_string(z, self.lang),
            get_float_string(w, self.lang),
        )
    }

    /// Port of `GpuShaderText::float4Const(const std::string &, ...)`
    /// (GpuShaderUtils.cpp:774-787 @ v2.5.2).
    pub fn float4_const(
        &self,
        x: impl AsRef<[u8]>,
        y: impl AsRef<[u8]>,
        z: impl AsRef<[u8]>,
        w: impl AsRef<[u8]>,
    ) -> Vec<u8> {
        cat([
            self.float4_keyword().as_bytes(),
            b"(",
            x.as_ref(),
            b", ",
            y.as_ref(),
            b", ",
            z.as_ref(),
            b", ",
            w.as_ref(),
            b")",
        ])
    }

    /// The same value four times. Upstream has no `double` overload: a C++ caller's `double`
    /// converts to `float`.
    ///
    /// Port of `GpuShaderText::float4Const(float)` (GpuShaderUtils.cpp:789-792 @ v2.5.2).
    pub fn float4_splat_f32(&self, v: f32) -> Vec<u8> {
        self.float4_splat(get_float_string(v, self.lang))
    }

    /// The same value four times.
    ///
    /// Port of `GpuShaderText::float4Const(const std::string &)`
    /// (GpuShaderUtils.cpp:794-797 @ v2.5.2).
    pub fn float4_splat(&self, v: impl AsRef<[u8]>) -> Vec<u8> {
        let v = v.as_ref();
        self.float4_const(v, v, v, v)
    }

    /// Port of `GpuShaderText::float4Decl` (GpuShaderUtils.cpp:799-807 @ v2.5.2).
    pub fn float4_decl(&self, name: impl AsRef<[u8]>) -> Result<Vec<u8>> {
        let name = name.as_ref();
        check_name(name)?;
        Ok(cat([self.float4_keyword().as_bytes(), b" ", name]))
    }

    /// Port of `GpuShaderText::declareFloat4(const std::string &, float, float, float, float)`
    /// (GpuShaderUtils.cpp:809-819 @ v2.5.2).
    pub fn declare_float4_f32(
        &self,
        name: impl AsRef<[u8]>,
        x: f32,
        y: f32,
        z: f32,
        w: f32,
    ) -> Result<()> {
        self.declare_float4(
            name,
            get_float_string(x, self.lang),
            get_float_string(y, self.lang),
            get_float_string(z, self.lang),
            get_float_string(w, self.lang),
        )
    }

    /// Port of `GpuShaderText::declareFloat4(const std::string &, double, double, double,
    /// double)` (GpuShaderUtils.cpp:821-831 @ v2.5.2).
    pub fn declare_float4_f64(
        &self,
        name: impl AsRef<[u8]>,
        x: f64,
        y: f64,
        z: f64,
        w: f64,
    ) -> Result<()> {
        self.declare_float4(
            name,
            get_float_string(x, self.lang),
            get_float_string(y, self.lang),
            get_float_string(z, self.lang),
            get_float_string(w, self.lang),
        )
    }

    /// Port of `GpuShaderText::declareFloat4(const std::string &, const std::string &, ...)`
    /// (GpuShaderUtils.cpp:833-840 @ v2.5.2).
    pub fn declare_float4(
        &self,
        name: impl AsRef<[u8]>,
        x: impl AsRef<[u8]>,
        y: impl AsRef<[u8]>,
        z: impl AsRef<[u8]>,
        w: impl AsRef<[u8]>,
    ) -> Result<()> {
        self.new_line()
            .put(self.float4_decl(name)?)
            .put(" = ")
            .put(self.float4_const(x, y, z, w))
            .put(";");
        Ok(())
    }
}

#[cfg(test)]
#[path = "gpu_shader_utils_tests.rs"]
mod tests;
