// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Viewing rules: a port of `src/OpenColorIO/ViewingRules.cpp` and `ViewingRules.h` @ v2.5.2.
//! A view names a rule, and the rule lists the color spaces (or roles) or the encodings of the
//! images the view is meant for: `Config::getViews(display, colorSpace)` gives the views whose
//! rule fits the image's color space, and the views without a rule.
//!
//! Strings are bytes, as upstream's C strings: each argument ends at its first NUL.

use std::fmt;

use ocio_ops::exception::{Exception, Result};
use ocio_ops::parse_utils::str_equals_case_ignore;
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::string_utils::{c_str, trim};

use crate::custom_keys::CustomKeysContainer;
use crate::tokens_manager::TokensManager;

/// A rule: its name, and either its color spaces or its encodings, and its custom keys.
///
/// Port of `ViewingRule` (src/OpenColorIO/ViewingRules.cpp:37-121 @ v2.5.2), but its
/// `validate`, which comes with the config's (3.8b).
#[derive(Debug, Clone)]
struct ViewingRule {
    /// `m_customKeys`.
    custom_keys: CustomKeysContainer,
    /// `m_colorSpaces`.
    color_spaces: TokensManager,
    /// `m_encodings`.
    encodings: TokensManager,
    /// `m_name`.
    name: Vec<u8>,
}

impl ViewingRule {
    /// Port of `ViewingRule::ViewingRule` (src/OpenColorIO/ViewingRules.cpp:45-48 @ v2.5.2).
    fn new(name: &[u8]) -> ViewingRule {
        ViewingRule {
            custom_keys: CustomKeysContainer::default(),
            color_spaces: TokensManager::default(),
            encodings: TokensManager::default(),
            name: name.to_vec(),
        }
    }
}

/// The viewing rules of a config, in order.
///
/// Port of `ViewingRules` and `ViewingRules::Impl` (include/OpenColorIO/OpenColorIO.h:
/// 1875-1967, src/OpenColorIO/ViewingRules.h:31-52, ViewingRules.cpp:123-406 @ v2.5.2), but
/// `Impl::validate`, which comes with the config's (3.8b). `Clone` is `createEditableCopy`, a
/// deep copy (ViewingRules.cpp:144-164).
#[derive(Debug, Clone, Default)]
pub struct ViewingRules {
    /// `m_rules`: all rules.
    rules: Vec<ViewingRule>,
}

impl ViewingRules {
    /// Viewing rules without a rule.
    ///
    /// Port of `ViewingRules::Create` (src/OpenColorIO/ViewingRules.cpp:134-137 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> ViewingRules {
        ViewingRules::default()
    }

    /// Port of `ViewingRules::Impl::validatePosition` (src/OpenColorIO/ViewingRules.cpp:166-176
    /// @ v2.5.2).
    fn validate_position(&self, rule_index: usize) -> Result<()> {
        let num_rules = self.rules.len();
        if rule_index >= num_rules {
            return Err(Exception::new(format!(
                "Viewing rules: rule index '{rule_index}' invalid. There are only \
                 '{num_rules}' rules."
            )));
        }
        Ok(())
    }

    /// Port of `ViewingRules::Impl::validateNewRule` (src/OpenColorIO/ViewingRules.cpp:178-195
    /// @ v2.5.2).
    fn validate_new_rule(&self, name: &[u8]) -> Result<()> {
        if name.is_empty() {
            return Err(Exception::new(
                "Viewing rules: rule must have a non-empty name.",
            ));
        }
        if self
            .rules
            .iter()
            .any(|rule| strcasecmp(name, &rule.name).is_eq())
        {
            return Err(Exception::new(
                [
                    b"Viewing rules: A rule named '".as_slice(),
                    name,
                    b"' already exists.",
                ]
                .concat(),
            ));
        }
        Ok(())
    }

    /// The number of rules.
    ///
    /// Port of `ViewingRules::getNumEntries` (src/OpenColorIO/ViewingRules.cpp:207-210 @
    /// v2.5.2).
    #[doc(alias = "getNumEntries")]
    pub fn num_entries(&self) -> usize {
        self.rules.len()
    }

    /// The index of the rule named `rule_name`, ignoring case.
    ///
    /// Port of `ViewingRules::getIndexForRule` (src/OpenColorIO/ViewingRules.cpp:212-226 @
    /// v2.5.2).
    #[doc(alias = "getIndexForRule")]
    pub fn index_for_rule(&self, rule_name: impl AsRef<[u8]>) -> Result<usize> {
        let rule_name = c_str(rule_name.as_ref());
        match self
            .rules
            .iter()
            .position(|rule| strcasecmp(rule_name, &rule.name).is_eq())
        {
            Some(idx) => Ok(idx),
            None => Err(Exception::new(
                [
                    b"Viewing rules: rule name '".as_slice(),
                    rule_name,
                    b"' not found.",
                ]
                .concat(),
            )),
        }
    }

    /// Port of `ViewingRules::getName` (src/OpenColorIO/ViewingRules.cpp:228-232 @ v2.5.2).
    #[doc(alias = "getName")]
    pub fn name(&self, rule_index: usize) -> Result<&[u8]> {
        self.validate_position(rule_index)?;
        Ok(&self.rules[rule_index].name)
    }

    /// Port of `ViewingRules::getNumColorSpaces` (src/OpenColorIO/ViewingRules.cpp:234-238 @
    /// v2.5.2).
    #[doc(alias = "getNumColorSpaces")]
    pub fn num_color_spaces(&self, rule_index: usize) -> Result<usize> {
        self.validate_position(rule_index)?;
        Ok(self.rules[rule_index].color_spaces.num_tokens() as usize)
    }

    /// "Viewing rules: rule '<name>' at index '<rule_index>': <what> index '<index>' is
    /// invalid. There are only '<count>' <what>s."
    fn index_error(&self, rule_index: usize, what: &str, index: usize, count: i32) -> Exception {
        Exception::new(
            [
                b"Viewing rules: rule '".as_slice(),
                &self.rules[rule_index].name,
                format!(
                    "' at index '{rule_index}': {what} index '{index}' is invalid. There are \
                     only '{count}' {what}s."
                )
                .as_bytes(),
            ]
            .concat(),
        )
    }

    /// The color space (or role) at `color_space_index` of a rule. Upstream checks the index
    /// cut to an `int` (`static_cast<int>`), as both wheels cut it: the low 32 bits, signed.
    /// An index whose cut is negative passes the check, and gives `None` (upstream's null
    /// pointer); one whose cut is in the list gives that color space
    /// (docs/improvements.md, I-135).
    ///
    /// Port of `ViewingRules::getColorSpace` (src/OpenColorIO/ViewingRules.cpp:240-253 @
    /// v2.5.2).
    #[doc(alias = "getColorSpace")]
    pub fn color_space(
        &self,
        rule_index: usize,
        color_space_index: usize,
    ) -> Result<Option<&[u8]>> {
        self.validate_position(rule_index)?;
        let num_cs = self.rules[rule_index].color_spaces.num_tokens();
        if color_space_index as i32 >= num_cs {
            return Err(self.index_error(rule_index, "colorspace", color_space_index, num_cs));
        }
        Ok(self.rules[rule_index]
            .color_spaces
            .token(color_space_index as i32))
    }

    /// Adds a color space (or role) to a rule, unless the rule has it (ignoring case and the
    /// surrounding whitespace); refused for a rule with encodings.
    ///
    /// Port of `ViewingRules::addColorSpace` (src/OpenColorIO/ViewingRules.cpp:255-274 @
    /// v2.5.2).
    #[doc(alias = "addColorSpace")]
    pub fn add_color_space(
        &mut self,
        rule_index: usize,
        color_space: impl AsRef<[u8]>,
    ) -> Result<()> {
        self.validate_position(rule_index)?;
        let color_space = c_str(color_space.as_ref());
        if color_space.is_empty() {
            return Err(self.rule_error(rule_index, "colorspace should have a non-empty name."));
        }
        if self.rules[rule_index].encodings.num_tokens() != 0 {
            return Err(self.rule_error(
                rule_index,
                "colorspace can't be added if there are encodings.",
            ));
        }
        self.rules[rule_index].color_spaces.add_token(color_space);
        Ok(())
    }

    /// "Viewing rules: rule '<name>' at index '<rule_index>': <what>"
    fn rule_error(&self, rule_index: usize, what: &str) -> Exception {
        Exception::new(
            [
                b"Viewing rules: rule '".as_slice(),
                &self.rules[rule_index].name,
                format!("' at index '{rule_index}': {what}").as_bytes(),
            ]
            .concat(),
        )
    }

    /// Removes the color space at `color_space_index` of a rule (as `color_space` finds it).
    ///
    /// Port of `ViewingRules::removeColorSpace` (src/OpenColorIO/ViewingRules.cpp:276-280 @
    /// v2.5.2).
    #[doc(alias = "removeColorSpace")]
    pub fn remove_color_space(
        &mut self,
        rule_index: usize,
        color_space_index: usize,
    ) -> Result<()> {
        let cs = self
            .color_space(rule_index, color_space_index)?
            .map(<[u8]>::to_vec);
        // `removeToken` does nothing for a null pointer.
        if let Some(cs) = cs {
            self.rules[rule_index].color_spaces.remove_token(&cs);
        }
        Ok(())
    }

    /// Port of `ViewingRules::getNumEncodings` (src/OpenColorIO/ViewingRules.cpp:282-286 @
    /// v2.5.2).
    #[doc(alias = "getNumEncodings")]
    pub fn num_encodings(&self, rule_index: usize) -> Result<usize> {
        self.validate_position(rule_index)?;
        Ok(self.rules[rule_index].encodings.num_tokens() as usize)
    }

    /// The encoding at `encoding_index` of a rule, the index cut to an `int` as in
    /// [`ViewingRules::color_space`] (docs/improvements.md, I-135).
    ///
    /// Port of `ViewingRules::getEncoding` (src/OpenColorIO/ViewingRules.cpp:288-301 @ v2.5.2).
    #[doc(alias = "getEncoding")]
    pub fn encoding(&self, rule_index: usize, encoding_index: usize) -> Result<Option<&[u8]>> {
        self.validate_position(rule_index)?;
        let num_enc = self.rules[rule_index].encodings.num_tokens();
        if encoding_index as i32 >= num_enc {
            return Err(self.index_error(rule_index, "encoding", encoding_index, num_enc));
        }
        Ok(self.rules[rule_index]
            .encodings
            .token(encoding_index as i32))
    }

    /// Adds an encoding to a rule, unless the rule has it (ignoring case and the surrounding
    /// whitespace); refused for a rule with color spaces.
    ///
    /// Port of `ViewingRules::addEncoding` (src/OpenColorIO/ViewingRules.cpp:303-322 @ v2.5.2).
    #[doc(alias = "addEncoding")]
    pub fn add_encoding(&mut self, rule_index: usize, encoding: impl AsRef<[u8]>) -> Result<()> {
        self.validate_position(rule_index)?;
        let encoding = c_str(encoding.as_ref());
        if encoding.is_empty() {
            return Err(self.rule_error(rule_index, "encoding should have a non-empty name."));
        }
        if self.rules[rule_index].color_spaces.num_tokens() != 0 {
            return Err(self.rule_error(
                rule_index,
                "encoding can't be added if there are colorspaces.",
            ));
        }
        self.rules[rule_index].encodings.add_token(encoding);
        Ok(())
    }

    /// Removes the encoding at `encoding_index` of a rule (as `encoding` finds it).
    ///
    /// Port of `ViewingRules::removeEncoding` (src/OpenColorIO/ViewingRules.cpp:324-328 @
    /// v2.5.2).
    #[doc(alias = "removeEncoding")]
    pub fn remove_encoding(&mut self, rule_index: usize, encoding_index: usize) -> Result<()> {
        let encoding = self
            .encoding(rule_index, encoding_index)?
            .map(<[u8]>::to_vec);
        // `removeToken` does nothing for a null pointer.
        if let Some(encoding) = encoding {
            self.rules[rule_index].encodings.remove_token(&encoding);
        }
        Ok(())
    }

    /// Port of `ViewingRules::getNumCustomKeys` (src/OpenColorIO/ViewingRules.cpp:330-334 @
    /// v2.5.2).
    #[doc(alias = "getNumCustomKeys")]
    pub fn num_custom_keys(&self, rule_index: usize) -> Result<usize> {
        self.validate_position(rule_index)?;
        Ok(self.rules[rule_index].custom_keys.size())
    }

    /// "Viewing rules: rule named '<name>' error: <what>"
    fn custom_key_error(&self, rule_index: usize, e: Exception) -> Exception {
        Exception::new(
            [
                b"Viewing rules: rule named '".as_slice(),
                &self.rules[rule_index].name,
                b"' error: ",
                e.what(),
            ]
            .concat(),
        )
    }

    /// Port of `ViewingRules::getCustomKeyName` (src/OpenColorIO/ViewingRules.cpp:336-350 @
    /// v2.5.2).
    #[doc(alias = "getCustomKeyName")]
    pub fn custom_key_name(&self, rule_index: usize, key: usize) -> Result<&[u8]> {
        self.validate_position(rule_index)?;
        self.rules[rule_index]
            .custom_keys
            .name(key)
            .map_err(|e| self.custom_key_error(rule_index, e))
    }

    /// Port of `ViewingRules::getCustomKeyValue` (src/OpenColorIO/ViewingRules.cpp:352-366 @
    /// v2.5.2).
    #[doc(alias = "getCustomKeyValue")]
    pub fn custom_key_value(&self, rule_index: usize, key: usize) -> Result<&[u8]> {
        self.validate_position(rule_index)?;
        self.rules[rule_index]
            .custom_keys
            .value(key)
            .map_err(|e| self.custom_key_error(rule_index, e))
    }

    /// Sets the custom key `key` of a rule to `value`, or removes it for an empty value.
    ///
    /// Port of `ViewingRules::setCustomKey` (src/OpenColorIO/ViewingRules.cpp:368-382 @ v2.5.2).
    #[doc(alias = "setCustomKey")]
    pub fn set_custom_key(
        &mut self,
        rule_index: usize,
        key: impl AsRef<[u8]>,
        value: impl AsRef<[u8]>,
    ) -> Result<()> {
        self.validate_position(rule_index)?;
        let result = self.rules[rule_index]
            .custom_keys
            .set(key.as_ref(), value.as_ref());
        result.map_err(|e| self.custom_key_error(rule_index, e))
    }

    /// Inserts a rule named `name` (trimmed) at `rule_index`, which may be the end.
    ///
    /// Port of `ViewingRules::insertRule` (src/OpenColorIO/ViewingRules.cpp:384-400 @ v2.5.2).
    #[doc(alias = "insertRule")]
    pub fn insert_rule(&mut self, rule_index: usize, name: impl AsRef<[u8]>) -> Result<()> {
        let rule_name = trim(c_str(name.as_ref())).to_vec();

        self.validate_new_rule(&rule_name)?;

        let new_rule = ViewingRule::new(&rule_name);
        if rule_index == self.num_entries() {
            self.rules.push(new_rule);
        } else {
            self.validate_position(rule_index)?;
            self.rules.insert(rule_index, new_rule);
        }
        Ok(())
    }

    /// Port of `ViewingRules::removeRule` (src/OpenColorIO/ViewingRules.cpp:402-406 @ v2.5.2).
    #[doc(alias = "removeRule")]
    pub fn remove_rule(&mut self, rule_index: usize) -> Result<()> {
        self.validate_position(rule_index)?;
        self.rules.remove(rule_index);
        Ok(())
    }

    /// `operator<<`: a line per rule.
    ///
    /// Port of `operator<<(std::ostream &, const ViewingRules &)` (src/OpenColorIO/
    /// ViewingRules.cpp:408-465 @ v2.5.2).
    pub fn write_to(&self, os: &mut Vec<u8>) {
        let num_rules = self.num_entries();
        for (r, rule) in self.rules.iter().enumerate() {
            os.extend_from_slice(b"<ViewingRule name=");
            os.extend_from_slice(&rule.name);
            for (label, tokens) in [
                (b", colorspaces=[".as_slice(), &rule.color_spaces),
                (b", encodings=[".as_slice(), &rule.encodings),
            ] {
                let num = tokens.num_tokens();
                if num != 0 {
                    os.extend_from_slice(label);
                    for i in 0..num {
                        os.extend_from_slice(tokens.token(i).expect("a token in the list"));
                        if i + 1 != num {
                            os.extend_from_slice(b", ");
                        }
                    }
                    os.push(b']');
                }
            }
            let num_ck = rule.custom_keys.size();
            if num_ck != 0 {
                os.extend_from_slice(b", customKeys=[");
                for ck in 0..num_ck {
                    os.push(b'(');
                    os.extend_from_slice(rule.custom_keys.name(ck).expect("a valid key"));
                    os.extend_from_slice(b", ");
                    os.extend_from_slice(rule.custom_keys.value(ck).expect("a valid key"));
                    os.push(b')');
                    if ck + 1 != num_ck {
                        os.extend_from_slice(b", ");
                    }
                }
                os.push(b']');
            }
            os.push(b'>');
            if r + 1 != num_rules {
                os.push(b'\n');
            }
        }
    }

    /// The text `operator<<` writes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut os = Vec::new();
        self.write_to(&mut os);
        os
    }
}

impl fmt::Display for ViewingRules {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.to_bytes()))
    }
}

/// The index of the rule named `name`, ignoring case.
///
/// Port of `FindRule` (src/OpenColorIO/ViewingRules.cpp:467-480 @ v2.5.2).
pub(crate) fn find_rule(vr: &ViewingRules, name: &[u8]) -> Option<usize> {
    vr.rules
        .iter()
        .position(|rule| str_equals_case_ignore(&rule.name, name))
}

#[cfg(test)]
#[path = "viewing_rules_tests.rs"]
mod tests;
