// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/GpuShaderClassWrapper.h` and `GpuShaderClassWrapper.cpp` @ v2.5.2:
//! the class wrappers that OSL and MSL shaders put around OCIO's function, and the default
//! wrapper of the other languages, which changes nothing.
//!
//! **The Metal wrapper reads the declarations back.** To pass the textures and uniforms to its
//! class, it parses the shader's declarations line by line with `std::getline` and
//! `std::string`'s `find` family (`extractFunctionParameters`). This port models those calls
//! literally: `npos` is `usize::MAX` and arithmetic on it wraps, `substr` clamps its count,
//! `operator[]` at a line's end gives the terminating NUL, `getline` leaves the line as it is
//! once the stream is at its end, and `c_str()` ends a string at its first NUL.
//!
//! **Character classes.** Upstream passes the declarations' bytes to `std::isspace`, and the
//! class name's first byte to `std::isdigit`. For a non-ASCII byte, a negative `char`, the C++
//! standard leaves both undefined, but both wheels define them, and neither gives such a byte
//! either class (docs/improvements.md, U-6):
//! - the Windows wheel calls the UCRT's `isspace` and `isdigit`, which return 0 for a value
//!   below -1 in a single-byte locale (`ucrt/convert/_ctype.cpp:28-56`, Windows SDK
//!   10.0.22000.0);
//! - the Linux wheel calls glibc's `isspace`, a lookup in the locale's table, which covers -128
//!   to 255 and gives those bytes no class in the C and C.UTF-8 locales, and GCC inlines
//!   `isdigit` as `(unsigned)(c - '0') <= 9`.
//!
//! **Undefined behaviour** (docs/improvements.md, U-10). One read has no defined result: after
//! a texture's declaration, the wrapper looks for `sampler` in the next line and reads on from
//! `find("sampler") + 7` (GpuShaderClassWrapper.cpp:330-335 @ v2.5.2). Without `sampler`, that
//! wraps past `npos` to 6, beyond the end of a line shorter than 6 bytes: upstream reads there,
//! then `substr` throws `std::out_of_range`. The port returns an error instead. Everywhere
//! else, the parse reads only inside the line or its terminating NUL.

use ocio_ops::{Exception, Result};

use crate::gpu_shader_utils::GpuShaderText;
use crate::open_color_types::GpuLanguage;

/// `std::string::npos`.
const NPOS: usize = usize::MAX;

/// `std::isspace` on a `char`, as both wheels give it (see the module notes): the six ASCII
/// white-space characters.
fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// `std::isdigit` on a `char`, as both wheels give it (see the module notes).
fn is_digit(c: u8) -> bool {
    c.is_ascii_digit()
}

/// `s.find(needle, from)`, for a needle that isn't empty.
fn find(s: &[u8], needle: &[u8], from: usize) -> usize {
    if from > s.len() || needle.len() > s.len() - from {
        return NPOS;
    }
    s[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map_or(NPOS, |p| p + from)
}

/// `s.find(c)`.
fn find_byte(s: &[u8], c: u8) -> usize {
    s.iter().position(|&b| b == c).unwrap_or(NPOS)
}

/// `s.find_first_of(chars, from)`.
fn find_first_of(s: &[u8], chars: &[u8], from: usize) -> usize {
    if from >= s.len() {
        return NPOS;
    }
    s[from..]
        .iter()
        .position(|b| chars.contains(b))
        .map_or(NPOS, |p| p + from)
}

/// `s.substr(pos, n)`, where `pos <= s.size()` (every call here): the count is clamped to the
/// end of the string.
fn substr(s: &[u8], pos: usize, n: usize) -> Vec<u8> {
    let end = pos + n.min(s.len() - pos);
    s[pos..end].to_vec()
}

/// `s[i]`, where `i <= s.size()`: `s.size()` gives the terminating NUL.
fn at(s: &[u8], i: usize) -> u8 {
    s.get(i).copied().unwrap_or(0)
}

/// `std::string(s.c_str())`: the string up to its first NUL.
fn c_str(mut s: Vec<u8>) -> Vec<u8> {
    if let Some(nul) = s.iter().position(|&b| b == 0) {
        s.truncate(nul);
    }
    s
}

/// What the Metal wrapper uses of `std::istringstream` and `std::getline`: lines without
/// their `\n`, and the end-of-file flag.
struct LineReader<'a> {
    text: &'a [u8],
    pos: usize,
    eof: bool,
}

impl LineReader<'_> {
    /// `std::getline(is, line)`. Once the stream is at its end (`eof`), the sentry fails and
    /// `line` keeps its value. Otherwise `line` becomes the characters up to the next `\n`,
    /// which is consumed; reaching the end of the text instead sets `eof`.
    fn getline(&mut self, line: &mut Vec<u8>) {
        if self.eof {
            return;
        }
        line.clear();
        match self.text[self.pos..].iter().position(|&b| b == b'\n') {
            Some(n) => {
                line.extend_from_slice(&self.text[self.pos..self.pos + n]);
                self.pos += n + 1;
            }
            None => {
                line.extend_from_slice(&self.text[self.pos..]);
                self.pos = self.text.len();
                self.eof = true;
            }
        }
    }
}

/// The name of an array's element count.
///
/// Port of `GetArrayLengthVariableName` (GpuShaderClassWrapper.cpp:13-16 @ v2.5.2).
fn get_array_length_variable_name(variable_name: &[u8]) -> Vec<u8> {
    [variable_name, b"_count"].concat()
}

/// A parameter of the Metal class's constructor: a texture, a sampler or a uniform. A name
/// with a `[` is an array.
///
/// Port of `MetalShaderClassWrapper::FunctionParam` (GpuShaderClassWrapper.h:106-119 @ v2.5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionParam {
    /// `m_type`.
    ty: Vec<u8>,
    /// `m_name`.
    name: Vec<u8>,
    /// `m_isArray`.
    is_array: bool,
}

impl FunctionParam {
    fn new(ty: Vec<u8>, name: Vec<u8>) -> FunctionParam {
        let is_array = find_byte(&name, b'[') != NPOS;
        FunctionParam { ty, name, is_array }
    }
}

/// The Metal wrapper: a class whose constructor takes the textures, samplers and uniforms,
/// and a function that builds the class and calls OCIO's function on the pixel. A copy keeps
/// everything (`operator=`, GpuShaderClassWrapper.cpp:414-420 @ v2.5.2).
///
/// Port of `MetalShaderClassWrapper` (GpuShaderClassWrapper.h:88-129 @ v2.5.2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct MetalShaderClassWrapper {
    /// `m_className`.
    class_name: Vec<u8>,
    /// `m_functionName`.
    function_name: Vec<u8>,
    /// `m_functionParameters`.
    function_parameters: Vec<FunctionParam>,
}

impl MetalShaderClassWrapper {
    /// The resource prefix, or `OCIO_` when it is empty, and the function name.
    ///
    /// Port of `MetalShaderClassWrapper::getClassWrapperName`
    /// (GpuShaderClassWrapper.cpp:280-283 @ v2.5.2).
    fn get_class_wrapper_name(resource_prefix: &[u8], function_name: &[u8]) -> Vec<u8> {
        let prefix: &[u8] = if resource_prefix.is_empty() {
            b"OCIO_"
        } else {
            resource_prefix
        };
        [prefix, function_name].concat()
    }

    /// Reads the parameters back from the declarations: a line starting with `texture`
    /// declares a texture, and the next line its sampler; any other line that isn't empty,
    /// white space or a `//` comment declares a uniform, as a type and a name. The 3D textures
    /// come first with their samplers, then the other textures, then the uniforms.
    ///
    /// Port of `MetalShaderClassWrapper::extractFunctionParameters`
    /// (GpuShaderClassWrapper.cpp:285-372 @ v2.5.2).
    fn extract_function_parameters(&mut self, declaration: &[u8]) -> Result<()> {
        // We want the caller to always pass 3d luts first with their samplers, and then pass
        // other luts.
        let mut lut_3d_textures: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> = Vec::new();
        let mut lut_textures: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> = Vec::new();
        let mut uniforms: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();

        self.function_parameters.clear();

        let mut line_buffer: Vec<u8> = Vec::new();

        let mut is = LineReader {
            text: declaration,
            pos: 0,
            eof: false,
        };
        while !is.eof {
            is.getline(&mut line_buffer);

            if line_buffer.is_empty() {
                continue;
            }

            let mut i: usize = 0;

            // Skip spaces
            while is_space(at(&line_buffer, i)) {
                i += 1;
            }

            // if the line was all skippable characters
            if i >= line_buffer.len() {
                continue;
            }

            // if the line is a comment
            if at(&line_buffer, i) == b'/' && at(&line_buffer, i + 1) == b'/' {
                continue;
            }

            // `lineBuffer.compare(i, 7, "texture") == 0`
            if line_buffer[i..].starts_with(b"texture") {
                // `static_cast<int>(lineBuffer[i+7] - '0')`, on a signed `char`.
                let texture_dim = i32::from(at(&line_buffer, i + 7) as i8) - i32::from(b'0');

                let end_texture_type = find_byte(&line_buffer, b'>');
                let texture_type = substr(
                    &line_buffer,
                    i,
                    end_texture_type.wrapping_sub(i).wrapping_add(1),
                );

                i = end_texture_type.wrapping_add(1);
                while is_space(at(&line_buffer, i)) {
                    i += 1;
                }

                let end_texture_name = find_first_of(&line_buffer, b" \t;", i);
                let texture_name = substr(&line_buffer, i, end_texture_name.wrapping_sub(i));

                is.getline(&mut line_buffer);

                i = find(&line_buffer, b"sampler", 0).wrapping_add(7);
                if i > line_buffer.len() {
                    // No `sampler`, and a line shorter than `npos + 7`, i.e. 6: upstream reads
                    // past the line's end (U-10).
                    return Err(Exception::new(
                        [
                            b"The MSL class wrapper found no sampler after texture '".as_slice(),
                            &texture_name,
                            b"': the next line is '",
                            &line_buffer,
                            b"'.",
                        ]
                        .concat(),
                    ));
                }
                while is_space(at(&line_buffer, i)) {
                    i += 1;
                }
                let end_sampler_name = find_first_of(&line_buffer, b" \t;", i);
                let sampler_name = substr(&line_buffer, i, end_sampler_name.wrapping_sub(i));

                if texture_dim == 3 {
                    lut_3d_textures.push((texture_type, texture_name, sampler_name));
                } else {
                    lut_textures.push((texture_type, texture_name, sampler_name));
                }
            } else {
                let end_type_name = find_first_of(&line_buffer, b" \t", i);
                let variable_type = substr(&line_buffer, i, end_type_name.wrapping_sub(i));

                i = end_type_name.wrapping_add(1);
                while is_space(at(&line_buffer, i)) {
                    i += 1;
                }

                let end_variable_name = find_first_of(&line_buffer, b" \t;", i);
                let variable_name = substr(&line_buffer, i, end_variable_name.wrapping_sub(i));
                uniforms.push((variable_type, variable_name));
            }
        }

        // The textures' parts pass through `c_str()`; the uniforms' don't.
        for (texture_type, texture_name, sampler_name) in lut_3d_textures {
            self.function_parameters
                .push(FunctionParam::new(c_str(texture_type), c_str(texture_name)));
            self.function_parameters
                .push(FunctionParam::new(b"sampler".to_vec(), c_str(sampler_name)));
        }

        for (texture_type, texture_name, sampler_name) in lut_textures {
            self.function_parameters
                .push(FunctionParam::new(c_str(texture_type), c_str(texture_name)));
            self.function_parameters
                .push(FunctionParam::new(b"sampler".to_vec(), c_str(sampler_name)));
        }

        for (variable_type, variable_name) in uniforms {
            self.function_parameters
                .push(FunctionParam::new(variable_type, variable_name));
        }
        Ok(())
    }

    /// Refuses a class name that is empty or starts with a digit. The message passes through
    /// `c_str()`; a name that isn't UTF-8 is written with replacement characters.
    ///
    /// GpuShaderClassWrapper.cpp:153-160 and 222-229 @ v2.5.2.
    fn check_class_name(&self) -> Result<()> {
        if self.class_name.is_empty() {
            return Err(Exception::new(
                "Struct name must include at least 1 character",
            ));
        }
        if is_digit(self.class_name[0]) {
            let mut message =
                b"Struct name must not start with a digit. Invalid className passed in: ".to_vec();
            message.extend_from_slice(&self.class_name);
            return Err(Exception::new(message));
        }
        Ok(())
    }

    /// The class: its constructor takes the parameters and copies each into its member. An
    /// array (`name[size]`) comes with its element count, and the elements past the count are
    /// set to 0.
    ///
    /// Port of `MetalShaderClassWrapper::generateClassWrapperHeader`
    /// (GpuShaderClassWrapper.cpp:151-218 @ v2.5.2).
    fn generate_class_wrapper_header(&self, kw: &GpuShaderText) -> Result<Vec<u8>> {
        self.check_class_name()?;

        kw.new_line().put("struct ").put(&self.class_name);
        kw.new_line().put("{");
        kw.new_line().put(&self.class_name).put("(");
        kw.indent();

        let mut separator: &str = "";
        for param in &self.function_parameters {
            kw.new_line()
                .put(separator)
                .put(if param.is_array { "constant " } else { "" })
                .put(&param.ty)
                .put(" ")
                .put(&param.name);
            if param.is_array {
                kw.new_line()
                    .put(", int ")
                    .put(get_array_length_variable_name(&substr(
                        &param.name,
                        0,
                        find_byte(&param.name, b'['),
                    )));
            }
            separator = ", ";
        }
        kw.dedent();
        kw.new_line().put(")");
        kw.new_line().put("{");

        kw.indent();
        for param in &self.function_parameters {
            let open_angled_bracket_pos = find_byte(&param.name, b'[');
            if !param.is_array {
                kw.new_line()
                    .put("this->")
                    .put(&param.name)
                    .put(" = ")
                    .put(&param.name)
                    .put(";");
            } else {
                let close_angled_bracket_pos = find_byte(&param.name, b']');
                let variable_name = substr(&param.name, 0, open_angled_bracket_pos);

                kw.new_line()
                    .put("for(int i = 0; i < ")
                    .put(get_array_length_variable_name(&variable_name))
                    .put("; ++i)");
                kw.new_line().put("{");
                kw.indent();
                kw.new_line()
                    .put("this->")
                    .put(&variable_name)
                    .put("[i] = ")
                    .put(&variable_name)
                    .put("[i];");
                kw.dedent();
                kw.new_line().put("}");

                kw.new_line()
                    .put("for(int i = ")
                    .put(get_array_length_variable_name(&variable_name))
                    .put("; i < ")
                    .put(substr(
                        &param.name,
                        open_angled_bracket_pos + 1,
                        close_angled_bracket_pos
                            .wrapping_sub(open_angled_bracket_pos)
                            .wrapping_sub(1),
                    ))
                    .put("; ++i)");
                kw.new_line().put("{");
                kw.indent();
                kw.new_line()
                    .put("this->")
                    .put(&variable_name)
                    .put("[i] = 0;");
                kw.dedent();
                kw.new_line().put("}");
            }
        }
        kw.dedent();
        kw.new_line().put("}");
        Ok(kw.string())
    }

    /// Closes the class, and writes the function that builds it from the parameters and calls
    /// OCIO's function, `ocio_function_name`, on the pixel.
    ///
    /// Port of `MetalShaderClassWrapper::generateClassWrapperFooter`
    /// (GpuShaderClassWrapper.cpp:220-278 @ v2.5.2).
    fn generate_class_wrapper_footer(
        &self,
        kw: &GpuShaderText,
        ocio_function_name: &[u8],
    ) -> Result<Vec<u8>> {
        self.check_class_name()?;

        kw.new_line().put("};");

        kw.new_line()
            .put(kw.float4_keyword())
            .put(" ")
            .put(ocio_function_name)
            .put("(");

        kw.indent();
        let mut separator: &str = "";
        for param in &self.function_parameters {
            kw.new_line()
                .put(separator)
                .put(if param.is_array { "constant " } else { "" })
                .put(&param.ty)
                .put(" ")
                .put(&param.name);
            if param.is_array {
                kw.new_line()
                    .put(", int ")
                    .put(get_array_length_variable_name(&substr(
                        &param.name,
                        0,
                        find_byte(&param.name, b'['),
                    )));
            }
            separator = ", ";
        }
        kw.new_line()
            .put(separator)
            .put(kw.float4_keyword())
            .put(" inPixel)");
        kw.dedent();
        kw.new_line().put("{");
        kw.indent();
        kw.new_line().put("return ").put(&self.class_name).put("(");

        kw.indent();
        separator = "";
        for param in &self.function_parameters {
            let open_angled_bracket_pos = find_byte(&param.name, b'[');
            let is_array = open_angled_bracket_pos != NPOS;

            if !is_array {
                kw.new_line().put(separator).put(&param.name);
            } else {
                kw.new_line()
                    .put(separator)
                    .put(substr(&param.name, 0, open_angled_bracket_pos));
                kw.new_line()
                    .put(", ")
                    .put(get_array_length_variable_name(&substr(
                        &param.name,
                        0,
                        open_angled_bracket_pos,
                    )));
            }
            separator = ", ";
        }
        kw.dedent();

        kw.new_line()
            .put(").")
            .put(ocio_function_name)
            .put("(inPixel);");
        kw.dedent();
        kw.new_line().put("}");

        Ok(kw.string())
    }

    /// Port of `MetalShaderClassWrapper::prepareClassWrapper`
    /// (GpuShaderClassWrapper.cpp:374-379 @ v2.5.2).
    fn prepare(
        &mut self,
        resource_prefix: &[u8],
        function_name: &[u8],
        original_header: &[u8],
    ) -> Result<()> {
        self.function_name = function_name.to_vec();
        self.class_name = Self::get_class_wrapper_name(resource_prefix, function_name);
        self.extract_function_parameters(original_header)
    }

    /// Port of `MetalShaderClassWrapper::getClassWrapperHeader`
    /// (GpuShaderClassWrapper.cpp:381-392 @ v2.5.2).
    fn header(&self, original_header: &[u8]) -> Result<Vec<u8>> {
        let st = GpuShaderText::new(GpuLanguage::Msl2_0);

        self.generate_class_wrapper_header(&st)?;
        st.new_line();

        let mut class_wrap_header = b"\n// Declaration of class wrapper\n\n".to_vec();
        class_wrap_header.extend_from_slice(&st.string());

        class_wrap_header.extend_from_slice(original_header);
        Ok(class_wrap_header)
    }

    /// Port of `MetalShaderClassWrapper::getClassWrapperFooter`
    /// (GpuShaderClassWrapper.cpp:394-405 @ v2.5.2).
    fn footer(&self, original_footer: &[u8]) -> Result<Vec<u8>> {
        let st = GpuShaderText::new(GpuLanguage::Msl2_0);

        st.new_line();
        self.generate_class_wrapper_footer(&st, &self.function_name)?;

        let mut class_wrap_footer = b"\n// Close class wrapper\n\n".to_vec();
        class_wrap_footer.extend_from_slice(&st.string());

        Ok([original_footer, &class_wrap_footer].concat())
    }
}

/// The OSL wrapper's header: OSL's includes, the operators OCIO's OSL code relies on, the
/// shader's signature and its opening brace, then the declarations.
///
/// Port of `OSLShaderClassWrapper::getClassWrapperHeader`
/// (GpuShaderClassWrapper.cpp:53-138 @ v2.5.2).
fn osl_header(function_name: &[u8], original_header: &[u8]) -> Vec<u8> {
    let st = GpuShaderText::new(GpuLanguage::Osl1);

    st.new_line().put("");
    st.new_line().put("/* All the includes */");
    st.new_line().put("");
    st.new_line().put("#include \"vector4.h\"");
    st.new_line().put("#include \"color4.h\"");

    st.new_line().put("");
    st.new_line().put("/* All the generic helper methods */");

    st.new_line().put("");
    st.new_line()
        .put("vector4 __operator__mul__(matrix m, vector4 v)");
    st.new_line().put("{");
    st.indent();
    st.new_line().put("return transform(m, v);");
    st.dedent();
    st.new_line().put("}");

    st.new_line().put("");
    st.new_line()
        .put("vector4 __operator__mul__(color4 c, vector4 v)");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put("return vector4(c.rgb.r, c.rgb.g, c.rgb.b, c.a) * v;");
    st.dedent();
    st.new_line().put("}");

    st.new_line().put("");
    st.new_line()
        .put("vector4 __operator__mul__(vector4 v, color4 c)");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put("return v * vector4(c.rgb.r, c.rgb.g, c.rgb.b, c.a);");
    st.dedent();
    st.new_line().put("}");

    st.new_line().put("");
    st.new_line()
        .put("vector4 __operator__sub__(color4 c, vector4 v)");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put("return vector4(c.rgb.r, c.rgb.g, c.rgb.b, c.a) - v;");
    st.dedent();
    st.new_line().put("}");

    st.new_line().put("");
    st.new_line()
        .put("vector4 __operator__add__(vector4 v, color4 c)");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put("return v + vector4(c.rgb.r, c.rgb.g, c.rgb.b, c.a);");
    st.dedent();
    st.new_line().put("}");

    st.new_line().put("");
    st.new_line()
        .put("vector4 __operator__add__(color4 c, vector4 v)");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put("return vector4(c.rgb.r, c.rgb.g, c.rgb.b, c.a) + v;");
    st.dedent();
    st.new_line().put("}");

    st.new_line().put("");
    st.new_line().put("vector4 pow(color4 c, vector4 v)");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put("return pow(vector4(c.rgb.r, c.rgb.g, c.rgb.b, c.a), v);");
    st.dedent();
    st.new_line().put("}");

    st.new_line().put("");
    st.new_line().put("vector4 max(vector4 v, color4 c)");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put("return max(v, vector4(c.rgb.r, c.rgb.g, c.rgb.b, c.a));");
    st.dedent();
    st.new_line().put("}");

    st.new_line().put("");
    st.new_line().put("/* The shader implementation */");
    st.new_line().put("");
    st.new_line()
        .put("shader ")
        .put("OSL_")
        .put(function_name)
        .put("(color4 inColor = {color(0), 1}, output color4 outColor = {color(0), 1})");
    st.new_line().put("{");

    [&st.string()[..], original_header].concat()
}

/// The OSL wrapper's footer: the shader calls OCIO's function on its input, and closes.
///
/// Port of `OSLShaderClassWrapper::getClassWrapperFooter`
/// (GpuShaderClassWrapper.cpp:140-149 @ v2.5.2).
fn osl_footer(function_name: &[u8], original_footer: &[u8]) -> Vec<u8> {
    let st = GpuShaderText::new(GpuLanguage::Osl1);

    st.new_line().put("");
    st.new_line()
        .put("outColor = ")
        .put(function_name)
        .put("(inColor);");
    st.new_line().put("}");

    [original_footer, &st.string()[..]].concat()
}

/// Which wrapper: upstream's three subclasses.
#[derive(Debug, PartialEq, Eq)]
enum Kind {
    /// `NullGpuShaderClassWrapper`.
    Null,
    /// `OSLShaderClassWrapper`, with its `m_functionName`.
    Osl(Vec<u8>),
    /// `MetalShaderClassWrapper`.
    Metal(MetalShaderClassWrapper),
}

/// The class wrapper a shading language needs around OCIO's function: the Metal wrapper in
/// MSL, the OSL wrapper in OSL, and in the other languages the default wrapper, which changes
/// nothing.
///
/// It isn't `Clone`: upstream's `clone()` doesn't copy everything, see
/// [`GpuShaderClassWrapper::clone_wrapper`].
///
/// Port of `GpuShaderClassWrapper`, `NullGpuShaderClassWrapper`, `OSLShaderClassWrapper` and
/// `MetalShaderClassWrapper` (GpuShaderClassWrapper.h:19-129 @ v2.5.2).
#[derive(Debug, PartialEq, Eq)]
pub struct GpuShaderClassWrapper {
    kind: Kind,
}

impl GpuShaderClassWrapper {
    /// The wrapper of `language`.
    ///
    /// Port of `GpuShaderClassWrapper::CreateClassWrapper`
    /// (GpuShaderClassWrapper.cpp:18-41 @ v2.5.2).
    pub fn create_class_wrapper(language: GpuLanguage) -> GpuShaderClassWrapper {
        let kind = match language {
            GpuLanguage::Msl2_0 => Kind::Metal(MetalShaderClassWrapper::default()),
            GpuLanguage::Osl1 => Kind::Osl(Vec::new()),
            // Most of the supported GPU shader languages do not have needs imposing a custom
            // class wrapper so, the default class wrapper does nothing.
            GpuLanguage::Cg
            | GpuLanguage::Glsl1_2
            | GpuLanguage::Glsl1_3
            | GpuLanguage::Glsl4_0
            | GpuLanguage::GlslVk4_6
            | GpuLanguage::HlslSm5_0
            | GpuLanguage::GlslEs1_0
            | GpuLanguage::GlslEs3_0 => Kind::Null,
        };
        GpuShaderClassWrapper { kind }
    }

    /// A copy, as upstream's `clone()` makes it: the Metal wrapper copies its state, but the
    /// OSL wrapper starts afresh, without its function name. (Upstream's shader description
    /// prepares the wrapper again before writing with it.)
    ///
    /// Port of `NullGpuShaderClassWrapper::clone`, `OSLShaderClassWrapper::clone` and
    /// `MetalShaderClassWrapper::clone` (GpuShaderClassWrapper.cpp:43-51, 407-412 @ v2.5.2).
    pub fn clone_wrapper(&self) -> GpuShaderClassWrapper {
        let kind = match &self.kind {
            Kind::Null => Kind::Null,
            Kind::Osl(_) => Kind::Osl(Vec::new()),
            Kind::Metal(metal) => Kind::Metal(metal.clone()),
        };
        GpuShaderClassWrapper { kind }
    }

    /// Takes what the wrapper needs from the shader description: the OSL wrapper the function
    /// name; the Metal wrapper the function name, its class name, and the parameters the
    /// declarations (`original_header`) hold.
    ///
    /// Port of `prepareClassWrapper` (GpuShaderClassWrapper.h:43-47, 67-72,
    /// GpuShaderClassWrapper.cpp:374-379 @ v2.5.2).
    pub fn prepare_class_wrapper(
        &mut self,
        resource_prefix: &[u8],
        function_name: &[u8],
        original_header: &[u8],
    ) -> Result<()> {
        match &mut self.kind {
            Kind::Null => Ok(()),
            Kind::Osl(name) => {
                *name = function_name.to_vec();
                Ok(())
            }
            Kind::Metal(metal) => metal.prepare(resource_prefix, function_name, original_header),
        }
    }

    /// The declarations with the wrapper's header around them; the default wrapper gives them
    /// back.
    ///
    /// Port of `getClassWrapperHeader` (GpuShaderClassWrapper.h:48-51,
    /// GpuShaderClassWrapper.cpp:53-138, 381-392 @ v2.5.2).
    pub fn get_class_wrapper_header(&self, original_header: &[u8]) -> Result<Vec<u8>> {
        match &self.kind {
            Kind::Null => Ok(original_header.to_vec()),
            Kind::Osl(function_name) => Ok(osl_header(function_name, original_header)),
            Kind::Metal(metal) => metal.header(original_header),
        }
    }

    /// The function's footer with the wrapper's after it; the default wrapper gives it back.
    ///
    /// Port of `getClassWrapperFooter` (GpuShaderClassWrapper.h:52-55,
    /// GpuShaderClassWrapper.cpp:140-149, 394-405 @ v2.5.2).
    pub fn get_class_wrapper_footer(&self, original_footer: &[u8]) -> Result<Vec<u8>> {
        match &self.kind {
            Kind::Null => Ok(original_footer.to_vec()),
            Kind::Osl(function_name) => Ok(osl_footer(function_name, original_footer)),
            Kind::Metal(metal) => metal.footer(original_footer),
        }
    }

    /// Whether the wrapper's header replaces the declarations (it holds them).
    ///
    /// Port of `hasClassWrapperHeader` (GpuShaderClassWrapper.h:56-59, 77-80, 97-100 @ v2.5.2).
    pub fn has_class_wrapper_header(&self) -> bool {
        !matches!(self.kind, Kind::Null)
    }
}

#[cfg(test)]
#[path = "gpu_shader_class_wrapper_tests.rs"]
mod tests;
