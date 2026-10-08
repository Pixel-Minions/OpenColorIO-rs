// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Parsing helpers: a port of `src/OpenColorIO/ParseUtils.cpp` @ v2.5.2: the bool and enum
//! string conversions the config needs, the env-style string lists, and (Phase 4, chunk 4.0a)
//! the XML entities and the numbers the file formats read and write. `nextline` comes with the
//! readers' input streams (4.0c); the `*ToString` functions of the Phase 1 enums live next to
//! their enums (`open_color_types.rs`).
//!
//! Strings are bytes, as C strings: an argument ends at its first NUL, and `None` is a null
//! pointer.

use crate::cfmt::{Crt, OStringStream};
use crate::exception::{Exception, Result};
use crate::open_color_types::{
    Allocation, BitDepth, CdlStyle, EnvironmentMode, NegativeStyle, TransformDirection,
};
use crate::ops::lut3d::lut3d_op_data::Interpolation;
use crate::platform::strcasecmp;
use crate::utils::num_get::{Basefield, get_integer, is_c_space};
use crate::utils::number_utils::{Errc, Flavor, from_chars_f32};
use crate::utils::string_utils::{StringVec, c_str, lower, lower_c_str, trim};

/// The bytes of a C string argument: up to the first NUL, or empty for a null pointer, as
/// `(s ? s : "")` reads them.
fn arg(s: Option<&[u8]>) -> &[u8] {
    s.map_or(&[][..], c_str)
}

/// A message: the parts' bytes, as `std::ostream <<` writes them.
fn msg(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

// The roles' names, declared in OpenColorTypes.h.
//
// Port of the `ROLE_*` constants (src/OpenColorIO/ParseUtils.cpp:530-542 @ v2.5.2;
// include/OpenColorIO/OpenColorTypes.h:870-913).

/// `ROLE_DEFAULT`.
pub const ROLE_DEFAULT: &str = "default";
/// `ROLE_REFERENCE`.
pub const ROLE_REFERENCE: &str = "reference";
/// `ROLE_DATA`.
pub const ROLE_DATA: &str = "data";
/// `ROLE_COLOR_PICKING`.
pub const ROLE_COLOR_PICKING: &str = "color_picking";
/// `ROLE_SCENE_LINEAR`.
pub const ROLE_SCENE_LINEAR: &str = "scene_linear";
/// `ROLE_COMPOSITING_LOG`.
pub const ROLE_COMPOSITING_LOG: &str = "compositing_log";
/// `ROLE_COLOR_TIMING`.
pub const ROLE_COLOR_TIMING: &str = "color_timing";
/// `ROLE_TEXTURE_PAINT`: the transform for painting textures.
pub const ROLE_TEXTURE_PAINT: &str = "texture_paint";
/// `ROLE_MATTE_PAINT`: the transform for matte painting.
pub const ROLE_MATTE_PAINT: &str = "matte_paint";
/// `ROLE_RENDERING`: the color space CGI renderers use.
pub const ROLE_RENDERING: &str = "rendering";
/// `ROLE_INTERCHANGE_SCENE`: the config's ACES2065-1 color space.
pub const ROLE_INTERCHANGE_SCENE: &str = "aces_interchange";
/// `ROLE_INTERCHANGE_DISPLAY`: the config's CIE XYZ D65 color space.
pub const ROLE_INTERCHANGE_DISPLAY: &str = "cie_xyz_d65_interchange";

/// `"true"` or `"false"`.
///
/// Port of `BoolToString` (src/OpenColorIO/ParseUtils.cpp:101-104 @ v2.5.2).
pub fn bool_to_string(val: bool) -> &'static str {
    if val { "true" } else { "false" }
}

/// Whether `s` is `true` or `yes`, in any ASCII case.
///
/// Port of `BoolFromString` (src/OpenColorIO/ParseUtils.cpp:106-111 @ v2.5.2).
pub fn bool_from_string(s: Option<&[u8]>) -> bool {
    let str = lower_c_str(s);
    str == b"true" || str == b"yes"
}

/// `forward` or `inverse`, in any ASCII case.
///
/// Port of `TransformDirectionFromString` (src/OpenColorIO/ParseUtils.cpp:140-151 @ v2.5.2).
pub fn transform_direction_from_string(s: Option<&[u8]>) -> Result<TransformDirection> {
    let p = arg(s);
    let str = lower(p);
    if str == b"forward" {
        return Ok(TransformDirection::Forward);
    } else if str == b"inverse" {
        return Ok(TransformDirection::Inverse);
    }
    Err(Exception::new(msg(&[
        b"Unrecognized transform direction: '",
        p,
        b"'.",
    ])))
}

/// The bit depth named `s` (as `BitDepthToString` names them, in any ASCII case), or
/// `Unknown`.
///
/// Port of `BitDepthFromString` (src/OpenColorIO/ParseUtils.cpp:184-197 @ v2.5.2).
pub fn bit_depth_from_string(s: Option<&[u8]>) -> BitDepth {
    match lower_c_str(s).as_slice() {
        b"8ui" => BitDepth::Uint8,
        b"10ui" => BitDepth::Uint10,
        b"12ui" => BitDepth::Uint12,
        b"14ui" => BitDepth::Uint14,
        b"16ui" => BitDepth::Uint16,
        b"32ui" => BitDepth::Uint32,
        b"16f" => BitDepth::F16,
        b"32f" => BitDepth::F32,
        _ => BitDepth::Unknown,
    }
}

/// Port of `BitDepthIsFloat` (src/OpenColorIO/ParseUtils.cpp:199-204 @ v2.5.2).
pub fn bit_depth_is_float(bit_depth: BitDepth) -> bool {
    matches!(bit_depth, BitDepth::F16 | BitDepth::F32)
}

/// The bits of an integer bit depth, 0 for the others.
///
/// Port of `BitDepthToInt` (src/OpenColorIO/ParseUtils.cpp:206-216 @ v2.5.2).
pub fn bit_depth_to_int(bit_depth: BitDepth) -> i32 {
    match bit_depth {
        BitDepth::Uint8 => 8,
        BitDepth::Uint10 => 10,
        BitDepth::Uint12 => 12,
        BitDepth::Uint14 => 14,
        BitDepth::Uint16 => 16,
        BitDepth::Uint32 => 32,
        BitDepth::F16 | BitDepth::F32 | BitDepth::Unknown => 0,
    }
}

/// `uniform` or `lg2`, in any ASCII case, or `Unknown`.
///
/// Port of `AllocationFromString` (src/OpenColorIO/ParseUtils.cpp:225-232 @ v2.5.2).
pub fn allocation_from_string(s: Option<&[u8]>) -> Allocation {
    match lower_c_str(s).as_slice() {
        b"uniform" => Allocation::Uniform,
        b"lg2" => Allocation::Lg2,
        _ => Allocation::Unknown,
    }
}

/// The interpolation named `s` (`default` isn't one), in any ASCII case, or `Unknown`.
///
/// Port of `InterpolationFromString` (src/OpenColorIO/ParseUtils.cpp:246-256 @ v2.5.2).
pub fn interpolation_from_string(s: Option<&[u8]>) -> Interpolation {
    match lower_c_str(s).as_slice() {
        b"nearest" => Interpolation::Nearest,
        b"linear" => Interpolation::Linear,
        b"tetrahedral" => Interpolation::Tetrahedral,
        b"best" => Interpolation::Best,
        b"cubic" => Interpolation::Cubic,
        _ => Interpolation::Unknown,
    }
}

/// `loadpredefined`, `loadall` or `unknown`.
///
/// Port of `EnvironmentModeToString` (src/OpenColorIO/ParseUtils.cpp:298-303 @ v2.5.2).
pub fn environment_mode_to_string(mode: EnvironmentMode) -> &'static str {
    match mode {
        EnvironmentMode::LoadPredefined => "loadpredefined",
        EnvironmentMode::LoadAll => "loadall",
        EnvironmentMode::Unknown => "unknown",
    }
}

/// `loadpredefined` or `loadall`, in any ASCII case, or `Unknown`.
///
/// Port of `EnvironmentModeFromString` (src/OpenColorIO/ParseUtils.cpp:305-312 @ v2.5.2).
pub fn environment_mode_from_string(s: Option<&[u8]>) -> EnvironmentMode {
    match lower_c_str(s).as_slice() {
        b"loadpredefined" => EnvironmentMode::LoadPredefined,
        b"loadall" => EnvironmentMode::LoadAll,
        _ => EnvironmentMode::Unknown,
    }
}

/// `asc` or `noclamp`, in any ASCII case.
///
/// Port of `CDLStyleFromString` (src/OpenColorIO/ParseUtils.cpp:321-332 @ v2.5.2).
pub fn cdl_style_from_string(style: Option<&[u8]>) -> Result<CdlStyle> {
    let p = arg(style);
    match lower(p).as_slice() {
        b"asc" => Ok(CdlStyle::Asc),
        b"noclamp" => Ok(CdlStyle::NoClamp),
        _ => Err(Exception::new(msg(&[b"Wrong CDL style: '", p, b"'."]))),
    }
}

/// `mirror`, `pass_thru`, `clamp` or `linear`, in any ASCII case.
///
/// Port of `NegativeStyleFromString` (src/OpenColorIO/ParseUtils.cpp:515-528 @ v2.5.2).
pub fn negative_style_from_string(style: Option<&[u8]>) -> Result<NegativeStyle> {
    let p = arg(style);
    match lower(p).as_slice() {
        b"mirror" => Ok(NegativeStyle::Mirror),
        b"pass_thru" => Ok(NegativeStyle::PassThru),
        b"clamp" => Ok(NegativeStyle::Clamp),
        b"linear" => Ok(NegativeStyle::Linear),
        _ => Err(Exception::new(msg(&[
            b"Unknown exponent style: '",
            p,
            b"'.",
        ]))),
    }
}

/// Whether `a` and `b` are equal but for ASCII case (deviation D-4).
///
/// Port of `StrEqualsCaseIgnore` (src/OpenColorIO/ParseUtils.cpp:693-696 @ v2.5.2).
pub fn str_equals_case_ignore(a: &[u8], b: &[u8]) -> bool {
    strcasecmp(a, b).is_eq()
}

/// The position where the name starting at `start` ends: the next `sep` outside quotes, or the
/// end of `s`.
///
/// Port of `FindEndOfName` (src/OpenColorIO/ParseUtils.cpp:702-747 @ v2.5.2).
fn find_end_of_name(s: &[u8], start: usize, sep: u8) -> Result<usize> {
    let mut current_pos = start;
    loop {
        // s.find_first_of("\"" + sep, currentPos)
        let found = s
            .iter()
            .enumerate()
            .skip(current_pos)
            .find(|&(_, &c)| c == b'"' || c == sep)
            .map(|(i, _)| i);
        match found {
            // Reached the end of the list.
            None => return Ok(s.len()),
            Some(i) if s[i] == b'"' => {
                // Found a quote, need to find the next one.
                match s.iter().skip(i + 1).position(|&c| c == b'"') {
                    None => {
                        return Err(Exception::new(msg(&[
                            b"The string '",
                            s,
                            b"' is not correctly formatted. It is missing a closing quote.",
                        ])));
                    }
                    // Found the second quote, continue the search for a separator.
                    Some(j) => current_pos = i + 1 + j + 1,
                }
            }
            // Found a symbol separating the elements, stop here.
            Some(i) => return Ok(i),
        }
    }
}

/// `str` split on `,` if it holds one, else on `:` if it holds one, else as one element;
/// separators inside double quotes don't split; each element trimmed, and freed of the quotes
/// around it. An empty (or white space) string gives one empty element.
///
/// Port of `SplitStringEnvStyle` (src/OpenColorIO/ParseUtils.cpp:749-808 @ v2.5.2).
pub fn split_string_env_style(str: &[u8]) -> Result<StringVec> {
    let s = trim(str);
    if s.is_empty() {
        // Look parsing always wants a result, even if an empty string.
        return Ok(vec![Vec::new()]);
    }

    let mut outputvec: StringVec = Vec::new();
    let found_comma = s.contains(&b',');
    let found_colon = s.contains(&b':');

    if found_comma || found_colon {
        let sep = if found_comma { b',' } else { b':' };
        let mut current_pos = 0usize;
        while !s.is_empty() && current_pos <= s.len() {
            let name_end_pos = find_end_of_name(s, current_pos, sep)?;
            if name_end_pos > current_pos {
                outputvec.push(s[current_pos..name_end_pos].to_vec());
                current_pos = name_end_pos + 1;
            } else {
                outputvec.push(Vec::new());
                current_pos += 1;
            }
        }
    } else {
        // If there is no comma or colon, consider the string as a single element.
        outputvec.push(s.to_vec());
    }

    for val in &mut outputvec {
        let trimmed = trim(val).to_vec();
        // If the trimmed value is surrounded by quotes, remove them.
        if trimmed.len() > 1 && trimmed[0] == b'"' && trimmed[trimmed.len() - 1] == b'"' {
            *val = trimmed[1..trimmed.len() - 1].to_vec();
        } else {
            *val = trimmed;
        }
    }

    Ok(outputvec)
}

/// Whether the element needs quotes in `JoinStringEnvStyle`: it holds `,` or `:`, is longer
/// than one byte, and neither starts nor ends with a quote.
fn needs_quotes(value: &[u8]) -> bool {
    value.iter().any(|&c| c == b',' || c == b':')
        && value.len() > 1
        && value[0] != b'"'
        && value[value.len() - 1] != b'"'
}

/// The elements joined with `, `, those holding a separator put in double quotes.
///
/// Port of `JoinStringEnvStyle` (src/OpenColorIO/ParseUtils.cpp:810-850 @ v2.5.2).
pub fn join_string_env_style(outputvec: &[Vec<u8>]) -> Vec<u8> {
    let mut result = Vec::new();
    for (i, value) in outputvec.iter().enumerate() {
        if i != 0 {
            result.extend_from_slice(b", ");
        }
        if needs_quotes(value) {
            result.push(b'"');
            result.extend_from_slice(value);
            result.push(b'"');
        } else {
            result.extend_from_slice(value);
        }
    }
    result
}

/// The strings of `vec1` that `vec2` holds, ignoring ASCII case, in `vec1`'s order and case.
///
/// Port of `IntersectStringVecsCaseIgnore` (src/OpenColorIO/ParseUtils.cpp:855-877 @ v2.5.2).
pub fn intersect_string_vecs_case_ignore(vec1: &[Vec<u8>], vec2: &[Vec<u8>]) -> StringVec {
    let allvalues: std::collections::BTreeSet<Vec<u8>> = vec2.iter().map(|v| lower(v)).collect();
    vec1.iter()
        .filter(|val| allvalues.contains(&lower(val)))
        .cloned()
        .collect()
}

/// The index of the first string of `vec` equal to `str` but for ASCII case, or -1.
///
/// Port of `FindInStringVecCaseIgnore` (src/OpenColorIO/ParseUtils.cpp:879-888 @ v2.5.2).
pub fn find_in_string_vec_case_ignore(vec: &[Vec<u8>], str: &[u8]) -> i32 {
    let teststr = lower(str);
    vec.iter()
        .position(|v| lower(v) == teststr)
        .map_or(-1, |i| i32::try_from(i).expect("an index"))
}

// ---------------------------------------------------------------------------------------------
// XML text, numbers to and from text (Phase 4, chunk 4.0a)

/// The XML entities `ConvertSpecialCharToXmlToken` writes and `ConvertXmlTokenToSpecialChar`
/// reads, in upstream's order.
///
/// Port of `elts` (src/OpenColorIO/ParseUtils.cpp:21-33 @ v2.5.2).
const XML_ELEMENTS: [(&[u8], u8); 5] = [
    (b"&quot;", b'"'),
    (b"&apos;", b'\''),
    (b"&lt;", b'<'),
    (b"&gt;", b'>'),
    (b"&amp;", b'&'),
];

/// `str` with `"`, `'`, `<`, `>` and `&` written as XML entities.
///
/// Port of `ConvertSpecialCharToXmlToken` (src/OpenColorIO/ParseUtils.cpp:35-59 @ v2.5.2).
pub fn convert_special_char_to_xml_token(str: &[u8]) -> Vec<u8> {
    let mut res = Vec::with_capacity(str.len());
    for &c in str {
        match XML_ELEMENTS.iter().find(|(_, e)| *e == c) {
            Some((token, _)) => res.extend_from_slice(token),
            None => res.push(c),
        }
    }
    res
}

/// `str` with the five XML entities read back as their characters; any other `&` is an error,
/// "Unknown XML tag:" followed by the rest of the text from that `&` (as a C string: up to a
/// NUL).
///
/// Port of `ConvertXmlTokenToSpecialChar` (src/OpenColorIO/ParseUtils.cpp:61-100 @ v2.5.2).
pub fn convert_xml_token_to_special_char(str: &[u8]) -> Result<Vec<u8>> {
    let mut res = Vec::with_capacity(str.len());
    let mut i = 0;
    while i < str.len() {
        if str[i] == b'&' {
            // `strncmp(&(*it), elt.str.c_str(), length)` reads the std::string's own bytes,
            // which end with a NUL: a token cut short by the end of the text doesn't match.
            match XML_ELEMENTS
                .iter()
                .find(|(token, _)| str[i..].starts_with(token))
            {
                Some((token, c)) => {
                    res.push(*c);
                    i += token.len();
                    continue;
                }
                None => {
                    return Err(Exception::new(
                        [b"Unknown XML tag:".as_slice(), c_str(&str[i..])].concat(),
                    ));
                }
            }
        }
        res.push(str[i]);
        i += 1;
    }
    Ok(res)
}

/// `std::ostringstream` with the classic locale and `precision` set: the text of a float or a
/// double as OCIO writes numbers into files and messages.
fn pretty(precision: i64) -> OStringStream {
    let mut os = OStringStream::new(Crt::NATIVE);
    os.precision = precision;
    os
}

/// `FLOAT_DECIMALS`.
const FLOAT_DECIMALS: i64 = 7;

/// `DOUBLE_DECIMALS`.
const DOUBLE_DECIMALS: i64 = 16;

/// `value` with 7 significant digits (`%.7g`).
///
/// Port of `FloatToString` (src/OpenColorIO/ParseUtils.cpp:550-557 @ v2.5.2).
pub fn float_to_string(value: f32) -> Vec<u8> {
    let mut os = pretty(FLOAT_DECIMALS);
    os.put_f32(value);
    os.into_bytes()
}

/// The values with 7 significant digits, separated by spaces; `""` for none.
///
/// Port of `FloatVecToString` (src/OpenColorIO/ParseUtils.cpp:559-573 @ v2.5.2).
pub fn float_vec_to_string(fval: &[f32]) -> Vec<u8> {
    let mut os = pretty(FLOAT_DECIMALS);
    for (i, &v) in fval.iter().enumerate() {
        if i != 0 {
            os.put_str(" ");
        }
        os.put_f32(v);
    }
    os.into_bytes()
}

/// The float at the start of `str` (as a C string), as `NumberUtils::from_chars` reads it for
/// this platform's wheel; `None` where it reads none. Upstream returns `false` then, and leaves
/// the output as it was.
///
/// Port of `StringToFloat` (src/OpenColorIO/ParseUtils.cpp:575-588 @ v2.5.2).
pub fn string_to_float(str: &[u8]) -> Option<f32> {
    let str = c_str(str);
    let mut x = f32::NAN;
    let result = from_chars_f32(Flavor::NATIVE, str, str.len(), &mut x);
    (result.ec == Errc::Ok).then_some(x)
}

/// `std::istringstream(str) >> *ival` in the classic locale: white space skipped, then a
/// decimal `int` (`num_get`), as the stream stores it even when it fails (0, or the nearest
/// limit on overflow). It fails where the extraction fails, or, with
/// `fail_if_leftover_chars`, where a character follows the number.
///
/// Port of `StringToInt` (src/OpenColorIO/ParseUtils.cpp:590-600 @ v2.5.2).
pub fn string_to_int(ival: &mut i32, str: &[u8], fail_if_leftover_chars: bool) -> bool {
    let str = c_str(str);
    // `>>` skips white space (skipws), then num_get reads at most what follows.
    let skipped = str.iter().take_while(|&&c| is_c_space(c)).count();
    if skipped == str.len() {
        // The sentry hits the end of the text: failbit, and `*ival` isn't written.
        return false;
    }
    let extracted = get_integer(
        &str[skipped..],
        Basefield::Dec,
        i128::from(i32::MIN),
        i128::from(i32::MAX),
    );
    *ival = i32::try_from(extracted.value).expect("a value in int's range");
    if extracted.fail {
        return false;
    }
    // `i.get(c)` succeeds where a character is left.
    !(fail_if_leftover_chars && skipped + extracted.consumed < str.len())
}

/// `value` with 16 significant digits (`%.16g`).
///
/// Port of `DoubleToString` (src/OpenColorIO/ParseUtils.cpp:602-609 @ v2.5.2).
pub fn double_to_string(value: f64) -> Vec<u8> {
    let mut os = pretty(DOUBLE_DECIMALS);
    os.put_f64(value);
    os.into_bytes()
}

/// The values with 16 significant digits, separated by spaces; `""` for none.
///
/// Port of `DoubleVecToString` (src/OpenColorIO/ParseUtils.cpp:611-625 @ v2.5.2).
pub fn double_vec_to_string(val: &[f64]) -> Vec<u8> {
    let mut os = pretty(DOUBLE_DECIMALS);
    for (i, &v) in val.iter().enumerate() {
        if i != 0 {
            os.put_str(" ");
        }
        os.put_f64(v);
    }
    os.into_bytes()
}

/// Each part read as a float (`NumberUtils::from_chars` over the part's bytes), into
/// `float_array`, resized to the parts' count first; `false` at the first part it can't read,
/// the entries before it written.
///
/// Port of `StringVecToFloatVec` (src/OpenColorIO/ParseUtils.cpp:627-645 @ v2.5.2).
pub fn string_vec_to_float_vec(float_array: &mut Vec<f32>, line_parts: &[Vec<u8>]) -> bool {
    float_array.resize(line_parts.len(), 0.0);
    for (i, part) in line_parts.iter().enumerate() {
        let mut x = f32::NAN;
        let result = from_chars_f32(Flavor::NATIVE, part, part.len(), &mut x);
        if result.ec != Errc::Ok {
            return false;
        }
        float_array[i] = x;
    }
    true
}

/// Each part read as an `int` with nothing after it (`StringToInt`), into `int_array`, resized
/// to the parts' count first; `false` at the first part it can't read.
///
/// Port of `StringVecToIntVec` (src/OpenColorIO/ParseUtils.cpp:647-670 @ v2.5.2).
pub fn string_vec_to_int_vec(int_array: &mut Vec<i32>, line_parts: &[Vec<u8>]) -> bool {
    int_array.resize(line_parts.len(), 0);
    for (i, part) in line_parts.iter().enumerate() {
        let mut x = 0;
        if !string_to_int(&mut x, part, true) {
            return false;
        }
        int_array[i] = x;
    }
    true
}

#[cfg(test)]
#[path = "parse_utils_tests.rs"]
mod tests;
