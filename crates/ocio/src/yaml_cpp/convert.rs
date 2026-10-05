// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0's conversions of nodes (include/yaml-cpp/node/convert.h,
//! src/convert.cpp) and of `Node::as<T>()`'s helpers (`as_if`, node/impl.h:90-149):
//! strings, `Null`, `bool`, the integer types and sequences of them.
//!
//! Numbers go through `std::stringstream >> std::noskipws >> value` after
//! `unsetf(std::ios::dec)`, so an integer can be written in hexadecimal (`0x15`) or octal
//! (`015`), and only white space may follow it ([`ocio_ops::utils::num_get`]).

use ocio_ops::utils::num_get::{self, Basefield};

use super::exceptions::{Exception, Result};
use super::node::{Node, NodeType};

/// `convert<T>` (convert.h) and `as_if<T, ...>` (node/impl.h:90-149) for one type.
pub trait Convert: Sized {
    /// `convert<T>::decode(const Node&, T&)`: the value, or `None` where the C++ returns
    /// false. Converting the elements of a sequence can throw.
    fn decode(node: &Node) -> Result<Option<Self>>;

    /// `as_if<T, void>::operator()` (node/impl.h:121-135): the value, or
    /// `TypedBadConversion<T>` at the node's mark (the null mark for `Node()`).
    fn as_if(node: &Node) -> Result<Self> {
        if !node.has_data() {
            return Err(Exception::bad_conversion(node.mark()?));
        }
        match Self::decode(node)? {
            Some(value) => Ok(value),
            None => Err(Exception::bad_conversion(node.mark()?)),
        }
    }

    /// `as_if<T, S>::operator()(fallback)` (node/impl.h:91-105).
    fn as_if_or(node: &Node, fallback: Self) -> Result<Self> {
        if !node.has_data() {
            return Ok(fallback);
        }
        Ok(Self::decode(node)?.unwrap_or(fallback))
    }
}

/// `std::string`, as bytes.
impl Convert for Vec<u8> {
    /// `convert<std::string>::decode` (convert.h:52-61): a scalar's text.
    fn decode(node: &Node) -> Result<Option<Self>> {
        if !node.is_scalar()? {
            return Ok(None);
        }
        Ok(Some(node.scalar()?.to_vec()))
    }

    /// `as_if<std::string, void>` (node/impl.h:137-149): `"null"` for a null node (`Node()`
    /// included), the scalar's text, or `TypedBadConversion` for a collection.
    fn as_if(node: &Node) -> Result<Self> {
        if node.node_type()? == NodeType::Null {
            return Ok(b"null".to_vec());
        }
        if node.node_type()? != NodeType::Scalar {
            return Err(Exception::bad_conversion(node.mark()?));
        }
        Ok(node.scalar()?.to_vec())
    }

    /// `as_if<std::string, S>` (node/impl.h:107-119).
    fn as_if_or(node: &Node, fallback: Self) -> Result<Self> {
        if node.node_type()? == NodeType::Null {
            return Ok(b"null".to_vec());
        }
        if node.node_type()? != NodeType::Scalar {
            return Ok(fallback);
        }
        Ok(node.scalar()?.to_vec())
    }
}

/// `YAML::_Null` (include/yaml-cpp/null.h:14), the type of `YAML::Null`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Null;

impl Convert for Null {
    /// `convert<_Null>::decode` (convert.h:93-100): a null node.
    fn decode(node: &Node) -> Result<Option<Self>> {
        Ok(node.is_null()?.then_some(Null))
    }
}

/// `IsLower` and `ToLower` (convert.cpp:7-9): ASCII only.
fn to_lower(s: &[u8]) -> Vec<u8> {
    s.iter().map(|c| c.to_ascii_lowercase()).collect()
}

/// `IsFlexibleCase` (convert.cpp:24-36): lowercase, UPPERCASE or Capitalized.
fn is_flexible_case(s: &[u8]) -> bool {
    if s.is_empty() {
        return true;
    }
    if s.iter().all(u8::is_ascii_lowercase) {
        return true;
    }
    let first_caps = s[0].is_ascii_uppercase();
    let rest = &s[1..];
    first_caps
        && (rest.iter().all(u8::is_ascii_lowercase) || rest.iter().all(u8::is_ascii_uppercase))
}

impl Convert for bool {
    /// `convert<bool>::decode` (convert.cpp:40-73): y/n, yes/no, true/false and on/off, in
    /// lowercase, UPPERCASE or Capitalized (http://yaml.org/type/bool.html).
    fn decode(node: &Node) -> Result<Option<Self>> {
        if !node.is_scalar()? {
            return Ok(None);
        }
        const NAMES: [(&[u8], &[u8]); 4] = [
            (b"y", b"n"),
            (b"yes", b"no"),
            (b"true", b"false"),
            (b"on", b"off"),
        ];
        let scalar = node.scalar()?;
        if !is_flexible_case(scalar) {
            return Ok(None);
        }
        let lower = to_lower(scalar);
        for (true_name, false_name) in NAMES {
            if true_name == lower.as_slice() {
                return Ok(Some(true));
            }
            if false_name == lower.as_slice() {
                return Ok(Some(false));
            }
        }
        Ok(None)
    }
}

/// `(stream >> std::ws).eof()` after an extraction that read `consumed` bytes: the rest is
/// white space, or the extraction already reached the end.
fn only_space_follows(input: &[u8], consumed: usize, eof: bool) -> bool {
    eof || input[consumed..].iter().all(|&c| num_get::is_c_space(c))
}

/// `conversion::ConvertStreamTo` (convert.h:132-157) for an integer of the range
/// `min..=max`: `stream >> std::noskipws >> rhs` succeeds and only white space follows.
fn stream_to_integer(input: &[u8], min: i128, max: i128) -> Option<i128> {
    let extracted = num_get::get_integer(input, Basefield::Auto, min, max);
    if extracted.fail || !only_space_follows(input, extracted.consumed, extracted.eof) {
        return None;
    }
    Some(extracted.value)
}

/// `YAML_DEFINE_CONVERT_STREAMABLE`'s `decode` (convert.h:160-201) for an integer type:
/// a scalar, no `-` before an unsigned type, then the stream conversion.
fn decode_integer(node: &Node, min: i128, max: i128) -> Result<Option<i128>> {
    if node.node_type()? != NodeType::Scalar {
        return Ok(None);
    }
    let input = node.scalar()?;
    if input.first() == Some(&b'-') && min >= 0 {
        return Ok(None);
    }
    Ok(stream_to_integer(input, min, max))
}

macro_rules! convert_integer {
    ($($t:ty => $cpp:literal),* $(,)?) => {$(
        impl Convert for $t {
            #[doc = concat!("`convert<", $cpp, ">::decode` (convert.h:160-201, 204-218).")]
            fn decode(node: &Node) -> Result<Option<Self>> {
                Ok(decode_integer(node, i128::from(<$t>::MIN), i128::from(<$t>::MAX))?
                    .map(|v| v as $t))
            }
        }
    )*};
}

convert_integer!(
    i16 => "short",
    u16 => "unsigned short",
    i32 => "int",
    u32 => "unsigned",
    i64 => "long long",
    u64 => "unsigned long long",
);

/// `signed char` and `unsigned char` (`int8_t`, `uint8_t`): read as an `int`, then checked
/// against the type's range (`ConvertStreamTo`, convert.h:132-145).
macro_rules! convert_small_integer {
    ($($t:ty => $cpp:literal),* $(,)?) => {$(
        impl Convert for $t {
            #[doc = concat!("`convert<", $cpp, ">::decode` (convert.h:132-145, 160-201).")]
            fn decode(node: &Node) -> Result<Option<Self>> {
                if node.node_type()? != NodeType::Scalar {
                    return Ok(None);
                }
                let input = node.scalar()?;
                if input.first() == Some(&b'-') && <$t>::MIN == 0 {
                    return Ok(None);
                }
                let num = stream_to_integer(input, i128::from(i32::MIN), i128::from(i32::MAX));
                Ok(num.and_then(|n| <$t>::try_from(n).ok()))
            }
        }
    )*};
}

convert_small_integer!(i8 => "signed char", u8 => "unsigned char");

/// C++ `char`, a type of its own: `stream >> c` reads one character, whatever it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CChar(pub u8);

impl Convert for CChar {
    /// `convert<char>::decode` (convert.h:146-157, 160-201): exactly one character, and white
    /// space after it.
    fn decode(node: &Node) -> Result<Option<Self>> {
        if node.node_type()? != NodeType::Scalar {
            return Ok(None);
        }
        let input = node.scalar()?;
        // `stream >> std::noskipws >> c`: the next character, or eofbit and failbit.
        let Some(&c) = input.first() else {
            return Ok(None);
        };
        Ok(only_space_follows(input, 1, false).then_some(CChar(c)))
    }
}

/// `convert<std::vector<T>>::decode` (convert.h:295-320): a sequence, each element through
/// `as<T>()` (which throws for an element it can't convert).
fn decode_vector<T: Convert>(node: &Node) -> Result<Option<Vec<T>>> {
    if !node.is_sequence()? {
        return Ok(None);
    }
    let mut rhs = Vec::new();
    for element in node.iter() {
        rhs.push(element.node.as_::<T>()?);
    }
    Ok(Some(rhs))
}

macro_rules! convert_vector {
    ($($t:ty => $cpp:literal),* $(,)?) => {$(
        impl Convert for Vec<$t> {
            #[doc = concat!("`convert<std::vector<", $cpp, ">>::decode` (convert.h:295-320).")]
            fn decode(node: &Node) -> Result<Option<Self>> {
                decode_vector::<$t>(node)
            }
        }
    )*};
}

// `Vec<u8>` is `std::string`; a `Vec<Vec<u8>>` is a `std::vector<std::string>` (OCIO's
// `StringVec`).
convert_vector!(
    Vec<u8> => "std::string",
    bool => "bool",
    i32 => "int",
    u32 => "unsigned",
    i64 => "long long",
    u64 => "unsigned long long",
);
