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
//!
//! **The shader creator.** Upstream's [`build_resource_name`] and the grading-log helpers
//! ([`add_lin_to_log_shader`], ...) take the shader creator only to read its resource prefix
//! or its pixel name. They take that name here, so they don't depend on the creator's API.

use std::cell::{Cell, RefCell};

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::math_utils::clamp_to_norm_half;
use ocio_ops::utils::string_utils::replace_in_place;
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

/// Upstream's message for the textures OSL can't have.
const OSL_NO_TEXTURES: &str = "Unsupported by the Open Shading language (OSL) translation.";

/// The declarations of an `n`-dimensional texture and of its sampler, `(texture, sampler)`:
/// either may be empty, when the language declares only the other. The Vulkan sampler ends
/// with a space.
///
/// Port of `getTexDecl<N>` (GpuShaderUtils.cpp:79-146 @ v2.5.2).
fn get_tex_decl(
    n: u32,
    lang: GpuLanguage,
    texture_name: &[u8],
    sampler_name: &[u8],
    descriptor_set_index: u32,
    texture_index: u32,
) -> Result<(Vec<u8>, Vec<u8>)> {
    match lang {
        GpuLanguage::Glsl1_2
        | GpuLanguage::Glsl1_3
        | GpuLanguage::Cg
        | GpuLanguage::Glsl4_0
        | GpuLanguage::GlslEs1_0
        | GpuLanguage::GlslEs3_0 => {
            let sampler_decl = cat([
                format!("uniform sampler{n}D ").as_bytes(),
                sampler_name,
                b";",
            ]);
            Ok((Vec::new(), sampler_decl))
        }
        GpuLanguage::GlslVk4_6 => {
            let sampler_decl = cat([
                format!(
                    "layout(set={descriptor_set_index}, binding = {texture_index}) \
                     uniform sampler{n}D "
                )
                .as_bytes(),
                sampler_name,
                b"; ",
            ]);
            Ok((Vec::new(), sampler_decl))
        }
        GpuLanguage::HlslSm5_0 => {
            let texture_decl = cat([format!("Texture{n}D ").as_bytes(), texture_name, b";"]);
            let sampler_decl = cat([b"SamplerState", b" ", sampler_name, b";"]);
            Ok((texture_decl, sampler_decl))
        }
        GpuLanguage::Osl1 => Err(Exception::new(OSL_NO_TEXTURES)),
        GpuLanguage::Msl2_0 => {
            let texture_decl = cat([
                format!("texture{n}d<float> ").as_bytes(),
                texture_name,
                b";",
            ]);
            let sampler_decl = cat([b"sampler", b" ", sampler_name, b";"]);
            Ok((texture_decl, sampler_decl))
        }
    }
}

/// The lookup of an `n`-dimensional texture at `coords`.
///
/// Port of `getTexSample<N>` (GpuShaderUtils.cpp:148-219 @ v2.5.2).
fn get_tex_sample(
    n: u32,
    lang: GpuLanguage,
    texture_name: &[u8],
    sampler_name: &[u8],
    coords: &[u8],
) -> Result<Vec<u8>> {
    const NO_1D_IN_ES: &str = "1D textures are unsupported by OpenGL ES.";
    match lang {
        GpuLanguage::Glsl1_2 => Ok(cat([
            format!("texture{n}D(").as_bytes(),
            sampler_name,
            b", ",
            coords,
            b")",
        ])),
        GpuLanguage::Glsl1_3 => Ok(cat([b"texture(", sampler_name, b", ", coords, b")"])),
        GpuLanguage::GlslEs1_0 => {
            if n == 1 {
                return Err(Exception::new(NO_1D_IN_ES));
            }
            Ok(cat([
                format!("texture{n}D(").as_bytes(),
                sampler_name,
                b", ",
                coords,
                b")",
            ]))
        }
        GpuLanguage::Cg => Ok(cat([
            format!("tex{n}D(").as_bytes(),
            sampler_name,
            b", ",
            coords,
            b")",
        ])),
        GpuLanguage::HlslSm5_0 => Ok(cat([
            texture_name,
            b".Sample(",
            sampler_name,
            b", ",
            coords,
            b")",
        ])),
        GpuLanguage::Glsl4_0 | GpuLanguage::GlslVk4_6 => {
            Ok(cat([b"texture(", sampler_name, b", ", coords, b")"]))
        }
        GpuLanguage::GlslEs3_0 => {
            if n == 1 {
                return Err(Exception::new(NO_1D_IN_ES));
            }
            Ok(cat([b"texture(", sampler_name, b", ", coords, b")"]))
        }
        GpuLanguage::Osl1 => Err(Exception::new(OSL_NO_TEXTURES)),
        GpuLanguage::Msl2_0 => Ok(cat([
            texture_name,
            b".sample(",
            sampler_name,
            b", ",
            coords,
            b")",
        ])),
    }
}

/// The `N * N` values of a matrix stored by rows, as literals separated by `, `: by columns
/// when `transpose` is set. The last value is on the diagonal, so it is the same either way.
///
/// Port of `getMatrixValues<T, N>` (GpuShaderUtils.cpp:221-237 @ v2.5.2).
fn get_matrix_values<T: ShaderFloat, const N: usize>(
    mtx: &[T],
    lang: GpuLanguage,
    transpose: bool,
) -> String {
    let mut vals = String::new();

    for i in 0..N * N - 1 {
        let line = i / N;
        let col = i % N;
        let idx = if transpose {
            col * N + line
        } else {
            line * N + col
        };

        vals += &get_float_string(mtx[idx], lang);
        vals += ", ";
    }
    vals += &get_float_string(mtx[N * N - 1], lang);

    vals
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

    /// A constant array of floats, on one line. The size is the slice's length.
    ///
    /// Upstream takes a count and a pointer (`int size, const float * v`) and reads `size`
    /// values; here the caller passes exactly those values, `&values[..count]`. A caller that
    /// holds its count apart from its values (as upstream's grading curves do: `getNumKnots()`
    /// with `getKnotsArray()`) must check that the count fits before slicing, and return an
    /// error where upstream would read past the values' end: slicing past it panics (U-12,
    /// docs/improvements.md).
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

    /// A constant array of ints, on one line. The size is the slice's length. As for
    /// [`GpuShaderText::declare_float_array_const`], a caller that holds its count apart from
    /// its values must check that the count fits before slicing them (U-12).
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

    // -- Textures -----------------------------------------------------------------------

    /// The sampler of a texture: its name and `Sampler`.
    ///
    /// Port of `GpuShaderText::getSamplerName` (GpuShaderUtils.cpp:842-845 @ v2.5.2).
    pub fn get_sampler_name(texture_name: impl AsRef<[u8]>) -> Vec<u8> {
        cat([texture_name.as_ref(), b"Sampler"])
    }

    /// Writes the declarations of an `n`-dimensional texture and its sampler, each on its own
    /// line when the language has it. Upstream's three functions differ only in `n`.
    ///
    /// Port of `GpuShaderText::declareTex1D`, `declareTex2D` and `declareTex3D`
    /// (GpuShaderUtils.cpp:847-902 @ v2.5.2).
    fn declare_tex(
        &self,
        n: u32,
        texture_name: &[u8],
        descriptor_set_index: u32,
        texture_index: u32,
    ) -> Result<()> {
        let (texture_decl, sampler_decl) = get_tex_decl(
            n,
            self.lang,
            texture_name,
            &Self::get_sampler_name(texture_name),
            descriptor_set_index,
            texture_index,
        )?;

        if !texture_decl.is_empty() {
            self.new_line().put(texture_decl);
        }

        if !sampler_decl.is_empty() {
            self.new_line().put(sampler_decl);
        }
        Ok(())
    }

    /// Port of `GpuShaderText::declareTex1D` (GpuShaderUtils.cpp:847-864 @ v2.5.2).
    pub fn declare_tex1d(
        &self,
        texture_name: impl AsRef<[u8]>,
        descriptor_set_index: u32,
        texture_index: u32,
    ) -> Result<()> {
        self.declare_tex(
            1,
            texture_name.as_ref(),
            descriptor_set_index,
            texture_index,
        )
    }

    /// Port of `GpuShaderText::declareTex2D` (GpuShaderUtils.cpp:866-883 @ v2.5.2).
    pub fn declare_tex2d(
        &self,
        texture_name: impl AsRef<[u8]>,
        descriptor_set_index: u32,
        texture_index: u32,
    ) -> Result<()> {
        self.declare_tex(
            2,
            texture_name.as_ref(),
            descriptor_set_index,
            texture_index,
        )
    }

    /// Port of `GpuShaderText::declareTex3D` (GpuShaderUtils.cpp:885-902 @ v2.5.2).
    pub fn declare_tex3d(
        &self,
        texture_name: impl AsRef<[u8]>,
        descriptor_set_index: u32,
        texture_index: u32,
    ) -> Result<()> {
        self.declare_tex(
            3,
            texture_name.as_ref(),
            descriptor_set_index,
            texture_index,
        )
    }

    /// Port of `GpuShaderText::sampleTex1D` (GpuShaderUtils.cpp:904-908 @ v2.5.2).
    pub fn sample_tex1d(
        &self,
        texture_name: impl AsRef<[u8]>,
        coords: impl AsRef<[u8]>,
    ) -> Result<Vec<u8>> {
        let texture_name = texture_name.as_ref();
        get_tex_sample(
            1,
            self.lang,
            texture_name,
            &Self::get_sampler_name(texture_name),
            coords.as_ref(),
        )
    }

    /// Port of `GpuShaderText::sampleTex2D` (GpuShaderUtils.cpp:910-914 @ v2.5.2).
    pub fn sample_tex2d(
        &self,
        texture_name: impl AsRef<[u8]>,
        coords: impl AsRef<[u8]>,
    ) -> Result<Vec<u8>> {
        let texture_name = texture_name.as_ref();
        get_tex_sample(
            2,
            self.lang,
            texture_name,
            &Self::get_sampler_name(texture_name),
            coords.as_ref(),
        )
    }

    /// Port of `GpuShaderText::sampleTex3D` (GpuShaderUtils.cpp:916-920 @ v2.5.2).
    pub fn sample_tex3d(
        &self,
        texture_name: impl AsRef<[u8]>,
        coords: impl AsRef<[u8]>,
    ) -> Result<Vec<u8>> {
        let texture_name = texture_name.as_ref();
        get_tex_sample(
            3,
            self.lang,
            texture_name,
            &Self::get_sampler_name(texture_name),
            coords.as_ref(),
        )
    }

    // -- Uniforms -----------------------------------------------------------------------

    /// `uniform `, except in MSL and Vulkan GLSL, whose uniforms go in a structure or a
    /// uniform block.
    fn uniform_decl_string(&self) -> &'static str {
        if self.lang == GpuLanguage::Msl2_0 || self.lang == GpuLanguage::GlslVk4_6 {
            ""
        } else {
            "uniform "
        }
    }

    /// Port of `GpuShaderText::declareUniformFloat` (GpuShaderUtils.cpp:923-931 @ v2.5.2).
    pub fn declare_uniform_float(&self, uniform_name: impl AsRef<[u8]>) {
        self.new_line()
            .put(self.uniform_decl_string())
            .put(self.float_keyword())
            .put(" ")
            .put(uniform_name.as_ref())
            .put(";");
    }

    /// A boolean uniform: an `int` in Vulkan GLSL, which has no boolean uniforms.
    ///
    /// Port of `GpuShaderText::declareUniformBool` (GpuShaderUtils.cpp:933-947 @ v2.5.2).
    pub fn declare_uniform_bool(&self, uniform_name: impl AsRef<[u8]>) {
        let mut uniform_decl_string = "uniform ";
        let mut bool_keyword = "bool";
        if self.lang == GpuLanguage::Msl2_0 {
            uniform_decl_string = "";
        } else if self.lang == GpuLanguage::GlslVk4_6 {
            uniform_decl_string = "";
            bool_keyword = "int";
        }
        self.new_line()
            .put(uniform_decl_string)
            .put(bool_keyword)
            .put(" ")
            .put(uniform_name.as_ref())
            .put(";");
    }

    /// Port of `GpuShaderText::declareUniformFloat3` (GpuShaderUtils.cpp:949-957 @ v2.5.2).
    pub fn declare_uniform_float3(&self, uniform_name: impl AsRef<[u8]>) {
        self.new_line()
            .put(self.uniform_decl_string())
            .put(self.float3_keyword())
            .put(" ")
            .put(uniform_name.as_ref())
            .put(";");
    }

    /// Port of `GpuShaderText::declareUniformArrayFloat` (GpuShaderUtils.cpp:959-967 @ v2.5.2).
    pub fn declare_uniform_array_float(&self, uniform_name: impl AsRef<[u8]>, size: u32) {
        self.new_line()
            .put(self.uniform_decl_string())
            .put(self.float_keyword())
            .put(" ")
            .put(uniform_name.as_ref())
            .put("[")
            .put(size)
            .put("];");
    }

    /// Port of `GpuShaderText::declareUniformArrayInt` (GpuShaderUtils.cpp:969-977 @ v2.5.2).
    pub fn declare_uniform_array_int(&self, uniform_name: impl AsRef<[u8]>, size: u32) {
        self.new_line()
            .put(self.uniform_decl_string())
            .put(self.int_keyword())
            .put(" ")
            .put(uniform_name.as_ref())
            .put("[")
            .put(size)
            .put("];");
    }

    // -- Matrices -----------------------------------------------------------------------

    /// A 3x3 matrix of floats, stored by rows, times a vector of three.
    ///
    /// Port of `GpuShaderText::mat3fMul(const float *, const std::string &)`
    /// (GpuShaderUtils.cpp:1034-1038 @ v2.5.2).
    pub fn mat3f_mul_f32(&self, m3x3: &[f32; 9], vec_name: impl AsRef<[u8]>) -> Result<Vec<u8>> {
        matrix3_mul(m3x3, vec_name.as_ref(), self.lang)
    }

    /// A 3x3 matrix of doubles, stored by rows, times a vector of three.
    ///
    /// Port of `GpuShaderText::mat3fMul(const double *, const std::string &)`
    /// (GpuShaderUtils.cpp:1040-1044 @ v2.5.2).
    pub fn mat3f_mul_f64(&self, m3x3: &[f64; 9], vec_name: impl AsRef<[u8]>) -> Result<Vec<u8>> {
        matrix3_mul(m3x3, vec_name.as_ref(), self.lang)
    }

    /// A 4x4 matrix of floats, stored by rows, times a vector of four.
    ///
    /// Port of `GpuShaderText::mat4fMul(const float *, const std::string &)`
    /// (GpuShaderUtils.cpp:1101-1105 @ v2.5.2).
    pub fn mat4f_mul_f32(&self, m4x4: &[f32; 16], vec_name: impl AsRef<[u8]>) -> Result<Vec<u8>> {
        matrix4_mul(m4x4, vec_name.as_ref(), self.lang)
    }

    /// A 4x4 matrix of doubles, stored by rows, times a vector of four.
    ///
    /// Port of `GpuShaderText::mat4fMul(const double *, const std::string &)`
    /// (GpuShaderUtils.cpp:1107-1111 @ v2.5.2).
    pub fn mat4f_mul_f64(&self, m4x4: &[f64; 16], vec_name: impl AsRef<[u8]>) -> Result<Vec<u8>> {
        matrix4_mul(m4x4, vec_name.as_ref(), self.lang)
    }

    // -- Special functions --------------------------------------------------------------

    /// `mix`, or `lerp` in Cg and HLSL.
    ///
    /// Port of `GpuShaderText::lerp` (GpuShaderUtils.cpp:1113-1145 @ v2.5.2).
    pub fn lerp(&self, x: impl AsRef<[u8]>, y: impl AsRef<[u8]>, a: impl AsRef<[u8]>) -> Vec<u8> {
        let function: &[u8] = match self.lang {
            GpuLanguage::Osl1
            | GpuLanguage::Glsl1_2
            | GpuLanguage::Glsl1_3
            | GpuLanguage::Glsl4_0
            | GpuLanguage::GlslVk4_6
            | GpuLanguage::GlslEs1_0
            | GpuLanguage::GlslEs3_0
            | GpuLanguage::Msl2_0 => b"mix(",
            GpuLanguage::Cg | GpuLanguage::HlslSm5_0 => b"lerp(",
        };
        cat([
            function,
            x.as_ref(),
            b", ",
            y.as_ref(),
            b", ",
            a.as_ref(),
            b")",
        ])
    }

    /// A vector of three whose components are 1 where the component of `a` is greater than
    /// the component of `b`, and 0 elsewhere.
    ///
    /// Port of `GpuShaderText::float3GreaterThan` (GpuShaderUtils.cpp:1147-1181 @ v2.5.2).
    pub fn float3_greater_than(&self, a: impl AsRef<[u8]>, b: impl AsRef<[u8]>) -> Vec<u8> {
        let (a, b) = (a.as_ref(), b.as_ref());
        let kw = self.float3_keyword();
        let kw = kw.as_bytes();
        match self.lang {
            GpuLanguage::Glsl1_2
            | GpuLanguage::Glsl1_3
            | GpuLanguage::Glsl4_0
            | GpuLanguage::GlslVk4_6
            | GpuLanguage::GlslEs1_0
            | GpuLanguage::GlslEs3_0
            | GpuLanguage::Cg => cat([kw, b"(greaterThan( ", a, b", ", b, b"))"]),
            GpuLanguage::Osl1 | GpuLanguage::Msl2_0 | GpuLanguage::HlslSm5_0 => cat([
                kw,
                b"(",
                b"(",
                a,
                b"[0] > ",
                b,
                b"[0]) ? 1.0 : 0.0, ",
                b"(",
                a,
                b"[1] > ",
                b,
                b"[1]) ? 1.0 : 0.0, ",
                b"(",
                a,
                b"[2] > ",
                b,
                b"[2]) ? 1.0 : 0.0)",
            ]),
        }
    }

    /// A vector of four whose components are 1 where the component of `a` is greater than
    /// the component of `b`, and 0 elsewhere. OSL's vector of four has named members.
    ///
    /// Port of `GpuShaderText::float4GreaterThan` (GpuShaderUtils.cpp:1183-1226 @ v2.5.2).
    pub fn float4_greater_than(&self, a: impl AsRef<[u8]>, b: impl AsRef<[u8]>) -> Vec<u8> {
        let (a, b) = (a.as_ref(), b.as_ref());
        let kw = self.float4_keyword();
        let kw = kw.as_bytes();
        match self.lang {
            GpuLanguage::Glsl1_2
            | GpuLanguage::Glsl1_3
            | GpuLanguage::Glsl4_0
            | GpuLanguage::GlslVk4_6
            | GpuLanguage::GlslEs1_0
            | GpuLanguage::GlslEs3_0
            | GpuLanguage::Cg => cat([kw, b"(greaterThan( ", a, b", ", b, b"))"]),
            GpuLanguage::Msl2_0 | GpuLanguage::HlslSm5_0 => cat([
                kw,
                b"(",
                b"(",
                a,
                b"[0] > ",
                b,
                b"[0]) ? 1.0 : 0.0, ",
                b"(",
                a,
                b"[1] > ",
                b,
                b"[1]) ? 1.0 : 0.0, ",
                b"(",
                a,
                b"[2] > ",
                b,
                b"[2]) ? 1.0 : 0.0, ",
                b"(",
                a,
                b"[3] > ",
                b,
                b"[3]) ? 1.0 : 0.0)",
            ]),
            GpuLanguage::Osl1 => cat([
                kw,
                b"(",
                b"(",
                a,
                b".rgb.r > ",
                b,
                b".x) ? 1.0 : 0.0, ",
                b"(",
                a,
                b".rgb.g > ",
                b,
                b".y) ? 1.0 : 0.0, ",
                b"(",
                a,
                b".rgb.b > ",
                b,
                b".z) ? 1.0 : 0.0, ",
                b"(",
                a,
                b".a > ",
                b,
                b".w) ? 1.0 : 0.0)",
            ]),
        }
    }

    /// A vector of three whose components are 1 where the component of `a` is greater than
    /// or equal to the component of `b`, and 0 elsewhere.
    ///
    /// Port of `GpuShaderText::float3GreaterThanEqual` (GpuShaderUtils.cpp:1228-1262 @ v2.5.2).
    pub fn float3_greater_than_equal(&self, a: impl AsRef<[u8]>, b: impl AsRef<[u8]>) -> Vec<u8> {
        let (a, b) = (a.as_ref(), b.as_ref());
        let kw = self.float3_keyword();
        let kw = kw.as_bytes();
        match self.lang {
            GpuLanguage::Glsl1_2
            | GpuLanguage::Glsl1_3
            | GpuLanguage::Glsl4_0
            | GpuLanguage::GlslVk4_6
            | GpuLanguage::GlslEs1_0
            | GpuLanguage::GlslEs3_0
            | GpuLanguage::Cg => cat([kw, b"(greaterThanEqual( ", a, b", ", b, b"))"]),
            GpuLanguage::Osl1 | GpuLanguage::Msl2_0 | GpuLanguage::HlslSm5_0 => cat([
                kw,
                b"(",
                b"(",
                a,
                b"[0] >= ",
                b,
                b"[0]) ? 1.0 : 0.0, ",
                b"(",
                a,
                b"[1] >= ",
                b,
                b"[1]) ? 1.0 : 0.0, ",
                b"(",
                a,
                b"[2] >= ",
                b,
                b"[2]) ? 1.0 : 0.0)",
            ]),
        }
    }

    /// A vector of four whose components are 1 where the component of `a` is greater than
    /// or equal to the component of `b`, and 0 elsewhere. OSL's vector of four has named
    /// members.
    ///
    /// Port of `GpuShaderText::float4GreaterThanEqual` (GpuShaderUtils.cpp:1264-1307 @ v2.5.2).
    pub fn float4_greater_than_equal(&self, a: impl AsRef<[u8]>, b: impl AsRef<[u8]>) -> Vec<u8> {
        let (a, b) = (a.as_ref(), b.as_ref());
        let kw = self.float4_keyword();
        let kw = kw.as_bytes();
        match self.lang {
            GpuLanguage::Glsl1_2
            | GpuLanguage::Glsl1_3
            | GpuLanguage::Glsl4_0
            | GpuLanguage::GlslVk4_6
            | GpuLanguage::GlslEs1_0
            | GpuLanguage::GlslEs3_0
            | GpuLanguage::Cg => cat([kw, b"(greaterThanEqual( ", a, b", ", b, b"))"]),
            GpuLanguage::Msl2_0 | GpuLanguage::HlslSm5_0 => cat([
                kw,
                b"(",
                b"(",
                a,
                b"[0] >= ",
                b,
                b"[0]) ? 1.0 : 0.0, ",
                b"(",
                a,
                b"[1] >= ",
                b,
                b"[1]) ? 1.0 : 0.0, ",
                b"(",
                a,
                b"[2] >= ",
                b,
                b"[2]) ? 1.0 : 0.0, ",
                b"(",
                a,
                b"[3] >= ",
                b,
                b"[3]) ? 1.0 : 0.0)",
            ]),
            GpuLanguage::Osl1 => cat([
                kw,
                b"(",
                b"(",
                a,
                b".rgb.r >= ",
                b,
                b".x) ? 1.0 : 0.0, ",
                b"(",
                a,
                b".rgb.g >= ",
                b,
                b".y) ? 1.0 : 0.0, ",
                b"(",
                a,
                b".rgb.b >= ",
                b,
                b".z) ? 1.0 : 0.0, ",
                b"(",
                a,
                b".a >= ",
                b,
                b".w) ? 1.0 : 0.0)",
            ]),
        }
    }

    /// The four-quadrant arctangent: `atan` in GLSL and Cg, `atan2` elsewhere, with the
    /// arguments in the same order everywhere.
    ///
    /// Port of `GpuShaderText::atan2` (GpuShaderUtils.cpp:1309-1348 @ v2.5.2).
    pub fn atan2(&self, y: impl AsRef<[u8]>, x: impl AsRef<[u8]>) -> Vec<u8> {
        let function: &[u8] = match self.lang {
            GpuLanguage::Cg
            | GpuLanguage::Glsl1_2
            | GpuLanguage::Glsl1_3
            | GpuLanguage::Glsl4_0
            | GpuLanguage::GlslVk4_6
            | GpuLanguage::GlslEs1_0
            | GpuLanguage::GlslEs3_0 => b"atan(",
            GpuLanguage::HlslSm5_0 | GpuLanguage::Osl1 | GpuLanguage::Msl2_0 => b"atan2(",
        };
        cat([function, y.as_ref(), b", ", x.as_ref(), b")"])
    }

    /// The sign of a vector of four, with a `;` after it (upstream's). OSL takes it member by
    /// member.
    ///
    /// Port of `GpuShaderText::sign` (GpuShaderUtils.cpp:1350-1380 @ v2.5.2).
    pub fn sign(&self, v: impl AsRef<[u8]>) -> Vec<u8> {
        let v = v.as_ref();
        match self.lang {
            GpuLanguage::Cg
            | GpuLanguage::Glsl1_2
            | GpuLanguage::Glsl1_3
            | GpuLanguage::Glsl4_0
            | GpuLanguage::GlslVk4_6
            | GpuLanguage::GlslEs1_0
            | GpuLanguage::GlslEs3_0
            | GpuLanguage::HlslSm5_0
            | GpuLanguage::Msl2_0 => cat([b"sign(", v, b");"]),
            GpuLanguage::Osl1 => cat([
                b"sign(",
                &self.float4_const(
                    cat([v, b".rgb.r"]),
                    cat([v, b".rgb.g"]),
                    cat([v, b".rgb.b"]),
                    cat([v, b".a"]),
                ),
                b");",
            ]),
        }
    }

    /// `bool(v)` in Vulkan GLSL, whose boolean uniforms are `int`s; `v` elsewhere. Upstream's
    /// writers use it wherever they test a boolean uniform.
    ///
    /// Port of `GpuShaderText::castToBool` (GpuShaderUtils.cpp:1382-1389 @ v2.5.2).
    pub fn cast_to_bool(&self, v: impl AsRef<[u8]>) -> Vec<u8> {
        if self.lang == GpuLanguage::GlslVk4_6 {
            return cat([b"bool(", v.as_ref(), b")"]);
        }
        v.as_ref().to_vec()
    }
}

/// A 3x3 matrix, stored by rows, times the vector `vec_name`, in the language's form.
///
/// Port of `matrix3Mul<T>` (GpuShaderUtils.cpp:979-1032 @ v2.5.2).
fn matrix3_mul<T: ShaderFloat>(
    m3x3: &[T; 9],
    vec_name: &[u8],
    lang: GpuLanguage,
) -> Result<Vec<u8>> {
    check_name(vec_name)?;

    let values = |transpose| get_matrix_values::<T, 3>(m3x3, lang, transpose);
    Ok(match lang {
        GpuLanguage::Glsl1_2
        | GpuLanguage::Glsl1_3
        | GpuLanguage::Glsl4_0
        | GpuLanguage::GlslVk4_6
        | GpuLanguage::GlslEs1_0
        | GpuLanguage::GlslEs3_0 => {
            // OpenGL shader program requests a transposed matrix.
            cat([b"mat3(", values(true).as_bytes(), b") * ", vec_name])
        }
        GpuLanguage::Cg => cat([
            b"mul(half3x3(",
            values(false).as_bytes(),
            b"), ",
            vec_name,
            b")",
        ]),
        GpuLanguage::HlslSm5_0 => cat([
            b"mul(",
            vec_name,
            b", float3x3(",
            values(true).as_bytes(),
            b"))",
        ]),
        GpuLanguage::Osl1 => cat([b"matrix(", values(true).as_bytes(), b") * ", vec_name]),
        GpuLanguage::Msl2_0 => cat([b"float3x3(", values(true).as_bytes(), b") * ", vec_name]),
    })
}

/// A 4x4 matrix, stored by rows, times the vector `vec_name`, in the language's form.
///
/// Port of `matrix4Mul<T>` (GpuShaderUtils.cpp:1046-1099 @ v2.5.2).
fn matrix4_mul<T: ShaderFloat>(
    m4x4: &[T; 16],
    vec_name: &[u8],
    lang: GpuLanguage,
) -> Result<Vec<u8>> {
    check_name(vec_name)?;

    let values = |transpose| get_matrix_values::<T, 4>(m4x4, lang, transpose);
    Ok(match lang {
        GpuLanguage::Glsl1_2
        | GpuLanguage::Glsl1_3
        | GpuLanguage::Glsl4_0
        | GpuLanguage::GlslVk4_6
        | GpuLanguage::GlslEs1_0
        | GpuLanguage::GlslEs3_0 => {
            // OpenGL shader program requests a transposed matrix.
            cat([b"mat4(", values(true).as_bytes(), b") * ", vec_name])
        }
        GpuLanguage::Cg => cat([
            b"mul(half4x4(",
            values(false).as_bytes(),
            b"), ",
            vec_name,
            b")",
        ]),
        GpuLanguage::HlslSm5_0 => cat([
            b"mul(",
            vec_name,
            b", float4x4(",
            values(true).as_bytes(),
            b"))",
        ]),
        GpuLanguage::Osl1 => cat([b"matrix(", values(true).as_bytes(), b") * ", vec_name]),
        GpuLanguage::Msl2_0 => cat([b"float4x4(", values(true).as_bytes(), b") * ", vec_name]),
    })
}

// ---------------------------------------------------------------------------------------
// Resource names and shared shader code
// ---------------------------------------------------------------------------------------

/// A resource name: the shader's resource prefix, `_`, `prefix`, `_` and `base`, with every
/// `__` replaced by `_` (scanning on after each replacement, so `___` becomes `__`).
///
/// Upstream takes the shader creator and reads its `getResourcePrefix()`; the caller passes
/// that here.
///
/// Port of `BuildResourceName` (GpuShaderUtils.cpp:1392-1404 @ v2.5.2).
pub fn build_resource_name(
    resource_prefix: impl AsRef<[u8]>,
    prefix: impl AsRef<[u8]>,
    base: impl AsRef<[u8]>,
) -> Vec<u8> {
    let mut name = resource_prefix.as_ref().to_vec();
    name.extend_from_slice(b"_");
    name.extend_from_slice(prefix.as_ref());
    name.extend_from_slice(b"_");
    name.extend_from_slice(base.as_ref());

    // Remove potentially problematic double underscores from GLSL resource names.
    replace_in_place(&mut name, b"__", b"_");
    name
}

/// Converts the pixel from scene-linear to "grading log": F-stops with 0 at 18% grey,
/// pseudo-logarithmic below about -5 stops so that 0.0 is at -7 stops instead of -Inf.
///
/// Upstream takes the shader creator and reads its `getPixelName()`; the caller passes that
/// here.
///
/// Port of `AddLinToLogShader` (GpuShaderUtils.cpp:1411-1430 @ v2.5.2).
pub fn add_lin_to_log_shader(pixel_name: impl AsRef<[u8]>, st: &GpuShaderText) -> Result<()> {
    let pix = pixel_name.as_ref();
    add_lin_to_log_shader_head(pix, st)?;
    st.new_line()
        .put(pix)
        .put(".rgb.r = (")
        .put(pix)
        .put(".rgb.r < xbrk) ? ylin.x : ylog.x;");
    st.new_line()
        .put(pix)
        .put(".rgb.g = (")
        .put(pix)
        .put(".rgb.g < xbrk) ? ylin.y : ylog.y;");
    st.new_line()
        .put(pix)
        .put(".rgb.b = (")
        .put(pix)
        .put(".rgb.b < xbrk) ? ylin.z : ylog.z;");
    st.dedent();
    st.new_line().put("}");
    Ok(())
}

/// [`add_lin_to_log_shader`], for the blue channel only.
///
/// Port of `AddLinToLogShaderChannelBlue` (GpuShaderUtils.cpp:1432-1449 @ v2.5.2).
pub fn add_lin_to_log_shader_channel_blue(
    pixel_name: impl AsRef<[u8]>,
    st: &GpuShaderText,
) -> Result<()> {
    let pix = pixel_name.as_ref();
    add_lin_to_log_shader_head(pix, st)?;
    st.new_line()
        .put(pix)
        .put(".rgb.b = (")
        .put(pix)
        .put(".rgb.b < xbrk) ? ylin.z : ylog.z;");
    st.dedent();
    st.new_line().put("}");
    Ok(())
}

/// The lines the two linear-to-log conversions share, up to the channel assignments
/// (GpuShaderUtils.cpp:1415-1424 and 1436-1445 @ v2.5.2).
fn add_lin_to_log_shader_head(pix: &[u8], st: &GpuShaderText) -> Result<()> {
    st.new_line().put("{"); // establish scope so local variable names won't conflict
    st.indent();
    st.new_line()
        .put(st.float_keyword_const())
        .put(" xbrk = 0.0041318374739483946;");
    st.new_line()
        .put(st.float_keyword_const())
        .put(" shift = -0.000157849851665374;");
    st.new_line()
        .put(st.float_keyword_const())
        .put(" m = 1. / (0.18 + shift);");
    st.new_line()
        .put(st.float_keyword_const())
        .put(" base2 = 1.4426950408889634;"); // 1/log(2)
    st.new_line()
        .put(st.float_keyword_const())
        .put(" gain = 363.034608563;");
    st.new_line()
        .put(st.float_keyword_const())
        .put(" offs = -7.;");
    st.new_line()
        .put(st.float3_decl("ylin")?)
        .put(" = ")
        .put(pix)
        .put(".rgb * gain + offs;");
    st.new_line()
        .put(st.float3_decl("ylog")?)
        .put(" = base2 * log( ( ")
        .put(pix)
        .put(".rgb + shift ) * m );");
    Ok(())
}

/// Converts the pixel from "grading log" to scene-linear: the inverse of
/// [`add_lin_to_log_shader`].
///
/// Upstream takes the shader creator and reads its `getPixelName()`; the caller passes that
/// here.
///
/// Port of `AddLogToLinShader` (GpuShaderUtils.cpp:1451-1469 @ v2.5.2).
pub fn add_log_to_lin_shader(pixel_name: impl AsRef<[u8]>, st: &GpuShaderText) -> Result<()> {
    let pix = pixel_name.as_ref();
    add_log_to_lin_shader_head(pix, st)?;
    st.new_line()
        .put(pix)
        .put(".rgb.r = (")
        .put(pix)
        .put(".rgb.r < ybrk) ? xlin.x : xlog.x;");
    st.new_line()
        .put(pix)
        .put(".rgb.g = (")
        .put(pix)
        .put(".rgb.g < ybrk) ? xlin.y : xlog.y;");
    st.new_line()
        .put(pix)
        .put(".rgb.b = (")
        .put(pix)
        .put(".rgb.b < ybrk) ? xlin.z : xlog.z;");
    st.dedent();
    st.new_line().put("}");
    Ok(())
}

/// [`add_log_to_lin_shader`], for the blue channel only.
///
/// Port of `AddLogToLinShaderChannelBlue` (GpuShaderUtils.cpp:1471-1487 @ v2.5.2).
pub fn add_log_to_lin_shader_channel_blue(
    pixel_name: impl AsRef<[u8]>,
    st: &GpuShaderText,
) -> Result<()> {
    let pix = pixel_name.as_ref();
    add_log_to_lin_shader_head(pix, st)?;
    st.new_line()
        .put(pix)
        .put(".rgb.b = (")
        .put(pix)
        .put(".rgb.b < ybrk) ? xlin.z : xlog.z;");
    st.dedent();
    st.new_line().put("}");
    Ok(())
}

/// The lines the two log-to-linear conversions share, up to the channel assignments
/// (GpuShaderUtils.cpp:1455-1463 and 1475-1483 @ v2.5.2).
fn add_log_to_lin_shader_head(pix: &[u8], st: &GpuShaderText) -> Result<()> {
    st.new_line().put("{"); // establish scope so local variable names won't conflict
    st.indent();
    st.new_line()
        .put(st.float_keyword_const())
        .put(" ybrk = -5.5;");
    st.new_line()
        .put(st.float_keyword_const())
        .put(" shift = -0.000157849851665374;");
    st.new_line()
        .put(st.float_keyword_const())
        .put(" gain = 363.034608563;");
    st.new_line()
        .put(st.float_keyword_const())
        .put(" offs = -7.;");
    st.new_line()
        .put(st.float3_decl("xlin")?)
        .put(" = (")
        .put(pix)
        .put(".rgb - offs) / gain;");
    st.new_line()
        .put(st.float3_decl("xlog")?)
        .put(" = pow( ")
        .put(st.float3_splat_f32(2.0f32))
        .put(", ")
        .put(pix)
        .put(".rgb ) * (0.18 + shift) - shift;");
    Ok(())
}

#[cfg(test)]
#[path = "gpu_shader_utils_tests.rs"]
mod tests;
