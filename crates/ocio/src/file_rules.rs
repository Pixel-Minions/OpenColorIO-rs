// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! File rules: a port of `src/OpenColorIO/FileRules.cpp` and `FileRules.h` @ v2.5.2. They give
//! a file path its color space: the first rule that matches the path, by a glob pattern and an
//! extension, a regular expression, or a color space name in the path; the last rule is the
//! default one.
//!
//! The regular expressions are C++'s `std::regex`, as each wheel's library implements it
//! (`ocio_ops::std_regex`): MSVC's STL on Windows, libstdc++ on Linux. Their errors' texts are
//! part of the messages.
//!
//! Strings are bytes, as upstream's C strings: each argument ends at its first NUL.

use std::fmt;
use std::sync::{Mutex, MutexGuard};

use ocio_ops::exception::{Exception, Result};
use ocio_ops::parse_utils::ROLE_DEFAULT;
use ocio_ops::platform::strcasecmp;
use ocio_ops::std_regex::{Library, Regex, regex_replace};
use ocio_ops::utils::string_utils::{c_str, compare, trim};

use crate::custom_keys::CustomKeysContainer;

/// The `std::regex` of this platform's wheel.
#[cfg(windows)]
pub(crate) const LIBRARY: Library = Library::Msvc;
/// The `std::regex` of this platform's wheel.
#[cfg(not(windows))]
pub(crate) const LIBRARY: Library = Library::Libstdcxx;

/// Rewrites the two forms of "any characters twice" of a regular expression built from a glob
/// into one (`.*`). Its first expression never matches a built one but on Windows, where `^`
/// also matches after a line feed (docs/improvements.md, I-133).
///
/// Port of `SanitizeRegularExpression` (src/OpenColorIO/FileRules.cpp:30-49 @ v2.5.2).
fn sanitize_regular_expression(regex_pattern: &[u8]) -> std::result::Result<Vec<u8>, String> {
    let mut r = regex_pattern.to_vec();

    // regex 1
    // "*?" => "*"
    // "?*" => "*"

    let re1 = Regex::new(b"(\\.\\*\\.^\\*)+|(^\\\\\\.\\.\\*)+", LIBRARY)
        .map_err(|e| e.what().to_string())?;
    r = regex_replace(&r, &re1, b".*").map_err(|e| e.what().to_string())?;

    // regex 2
    // "**" => "*"
    let re2 = Regex::new(b"(\\.\\*)+", LIBRARY).map_err(|e| e.what().to_string())?;
    r = regex_replace(&r, &re2, b".*").map_err(|e| e.what().to_string())?;

    // ex:  "*?*"
    //      regex 1 part 1 ==> "*?*" = "***"
    //      regex 2        ==> "***" = "*"

    Ok(r)
}

/// "File rules: invalid regular expression '<glob_pattern>' with '<what>'."
///
/// Port of `ThrowInvalidRegex` (src/OpenColorIO/FileRules.cpp:51-57 @ v2.5.2).
fn throw_invalid_regex(glob_pattern: &[u8], what: &[u8]) -> Exception {
    Exception::new(
        [
            b"File rules: invalid regular expression '".as_slice(),
            glob_pattern,
            b"' with '",
            c_str(what),
            b"'.",
        ]
        .concat(),
    )
}

/// The regular expression of a glob: `?` any character, `*` any characters, `[...]` a set
/// (`[!...]` its complement); the other special characters escaped. With `ignore_case`, a glob
/// without `[`, `*` or `?` matches its letters in either case.
///
/// Port of `ConvertToRegularExpression` (src/OpenColorIO/FileRules.cpp:59-198 @ v2.5.2). It
/// reads the string's terminating NUL past its last character, as upstream's
/// `std::string::operator[]` does.
fn convert_to_regular_expression(glob_pattern: &[u8], ignore_case: bool) -> Result<Vec<u8>> {
    let glob_pattern = c_str(glob_pattern);
    let mut glob_string: Vec<u8>;

    if ignore_case {
        glob_string = Vec::new();
        let mut respect_case = false;
        for &c in glob_pattern {
            if c == b'[' || c == b'*' || c == b'?' {
                respect_case = true;
                break;
            }
            // `isalpha` in the "C" locale.
            if c.is_ascii_alphabetic() {
                glob_string.push(b'[');
                glob_string.push(c.to_ascii_lowercase());
                glob_string.push(c.to_ascii_uppercase());
                glob_string.push(b']');
            } else {
                glob_string.push(c);
            }
        }
        if respect_case {
            glob_string = glob_pattern.to_vec();
        }
    } else {
        glob_string = glob_pattern.to_vec();
    }

    // `globString[i]`, the terminating NUL at its size.
    let at = |i: usize| glob_string.get(i).copied().unwrap_or(0);

    let mut regex_pattern = Vec::new();
    let glob_size = glob_string.len();
    let mut next_idx;
    let mut idx = 0;
    while idx < glob_size {
        next_idx = idx + 1;
        let c = glob_string[idx];
        let escaped: Option<&[u8]> = match c {
            b'.' => Some(b"\\."),
            b'?' => Some(b"."),
            b'*' => Some(b".*"),
            // Escape regex characters.
            b'+' => Some(b"\\+"),
            b'^' => Some(b"\\^"),
            b'$' => Some(b"\\$"),
            b'{' => Some(b"\\{"),
            b'}' => Some(b"\\}"),
            b'(' => Some(b"\\("),
            b')' => Some(b"\\)"),
            b'|' => Some(b"\\|"),
            _ => None,
        };
        if let Some(e) = escaped {
            regex_pattern.extend_from_slice(e);
            idx = next_idx;
            continue;
        }

        if c == b']' {
            return Err(throw_invalid_regex(glob_pattern, &glob_string[idx..]));
        }

        // Full processing from '[' to ']'.
        if c == b'[' {
            let mut sub_string = b"[".to_vec();
            let mut end = idx + 1; // +1 to bypass the '['
            while at(end) != b']' && end < glob_size {
                let ce = at(end);
                if ce == b'!' {
                    sub_string.push(b'^');
                } else if matches!(ce, b'+' | b'^' | b'$' | b'{' | b'}' | b'(' | b')' | b'|') {
                    // Escape regex characters.
                    sub_string.push(b'\\');
                    sub_string.push(ce);
                } else if ce == b'\\' {
                    sub_string.extend_from_slice(b"\\\\");
                } else if matches!(ce, b'.' | b'?' | b'*') {
                    if at(end - 1) != b'\\' {
                        return Err(throw_invalid_regex(glob_pattern, &glob_string[idx..]));
                    }
                    sub_string.push(ce);
                } else if ce == b'[' {
                    return Err(throw_invalid_regex(glob_pattern, &glob_string[idx..]));
                } else {
                    sub_string.push(ce);
                }
                end += 1;
            }
            if at(end) == b']' {
                sub_string.push(at(end));
            }

            // Some validations.

            if end >= glob_size {
                return Err(throw_invalid_regex(glob_pattern, &glob_string[idx..]));
            } else if sub_string == b"[]" {
                return Err(throw_invalid_regex(glob_pattern, b"[]"));
            } else if sub_string == b"[^]" {
                return Err(throw_invalid_regex(glob_pattern, b"[!]"));
            }

            // Keep the result.
            regex_pattern.extend_from_slice(&sub_string);
            idx = end + 1;
            continue;
        }

        regex_pattern.push(c);
        idx = next_idx;
    }

    Ok(regex_pattern)
}

/// The regular expression of a glob rule: `^((<pattern>)(\.<extension>))$`, an empty pattern
/// any characters and an empty extension any extension, the extension's letters in either
/// case; sanitized.
///
/// Port of `BuildRegularExpression` (src/OpenColorIO/FileRules.cpp:200-261 @ v2.5.2). Its
/// errors for null pointers can't happen with references.
fn build_regular_expression(
    file_path_pattern: &[u8],
    file_name_extension: &[u8],
) -> Result<Vec<u8>> {
    let file_path_pattern = c_str(file_path_pattern);
    let file_name_extension = c_str(file_name_extension);
    let mut str = b"^(".to_vec();

    if file_path_pattern.is_empty() {
        // An empty file path pattern is internally converted to "*" in order to simplify
        // the user writing of the glob pattern.
        str.extend_from_slice(b"(.*)");
    } else {
        str.push(b'(');
        str.extend_from_slice(&convert_to_regular_expression(file_path_pattern, false)?);
        str.push(b')');
    }

    if file_name_extension.is_empty() {
        // An empty file extension is internally converted to ".*" in order to simplify
        // the user writing of the glob pattern.
        str.extend_from_slice(b"(\\..*)");
    } else {
        str.extend_from_slice(b"(\\.");
        str.extend_from_slice(&convert_to_regular_expression(file_name_extension, true)?);
        str.push(b')');
    }

    str.extend_from_slice(b")$");

    sanitize_regular_expression(&str).map_err(|what| {
        Exception::new(
            [
                b"File rules: invalid regular expression '".as_slice(),
                &str,
                b"' built from pattern '",
                file_path_pattern,
                b" and extension '",
                file_name_extension,
                b"': '",
                what.as_bytes(),
                b"'.",
            ]
            .concat(),
        )
    })
}

/// Checks that `regex` compiles.
///
/// Port of `ValidateRegularExpression(const char *)` (src/OpenColorIO/FileRules.cpp:263-282
/// @ v2.5.2).
fn validate_regular_expression(regex: &[u8]) -> Result<()> {
    let regex = c_str(regex);
    if regex.is_empty() {
        return Err(Exception::new("File rules: regex is empty."));
    }

    // Throws an exception if the expression is ill-formed.
    match Regex::new(regex, LIBRARY) {
        Ok(_) => Ok(()),
        Err(ex) => Err(Exception::new(
            [
                b"File rules: invalid regular expression '".as_slice(),
                regex,
                b"': '",
                ex.what().as_bytes(),
                b"'.",
            ]
            .concat(),
        )),
    }
}

/// Checks that the regular expression of a glob rule compiles.
///
/// Port of `ValidateRegularExpression(const char *, const char *)`
/// (src/OpenColorIO/FileRules.cpp:284-288 @ v2.5.2).
fn validate_glob(file_path_pattern: &[u8], file_name_extension: &[u8]) -> Result<()> {
    let exp = build_regular_expression(file_path_pattern, file_name_extension)?;
    validate_regular_expression(&exp)
}

/// What a rule matches by.
///
/// Port of `FileRule::RuleType` (src/OpenColorIO/FileRules.cpp:297-303 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuleType {
    /// `FILE_RULE_DEFAULT`.
    Default,
    /// `FILE_RULE_PARSE_FILEPATH`.
    ParseFilepath,
    /// `FILE_RULE_REGEX`.
    Regex,
    /// `FILE_RULE_GLOB`.
    Glob,
}

/// A file rule.
///
/// Port of `FileRule` (src/OpenColorIO/FileRules.cpp:295-532 @ v2.5.2), in part: its matching
/// and validation come with the config's file rules.
#[derive(Debug)]
pub(crate) struct FileRule {
    /// `m_customKeys`.
    pub(crate) custom_keys: CustomKeysContainer,
    /// `m_name`.
    name: Vec<u8>,
    /// `m_colorSpace`, `mutable`: matching a path by the color space name in it sets it.
    color_space: Mutex<Vec<u8>>,
    /// `m_pattern`.
    pattern: Vec<u8>,
    /// `m_extension`.
    extension: Vec<u8>,
    /// `m_regex`.
    regex: Vec<u8>,
    /// `m_type`.
    rule_type: RuleType,
}

impl FileRule {
    /// A rule named `name`: the default rule, the path search rule (their names are matched
    /// ignoring case, and stored as upstream spells them), or a glob rule of `*.*`.
    ///
    /// Port of `FileRule::FileRule` (src/OpenColorIO/FileRules.cpp:309-331 @ v2.5.2).
    fn new(name: &[u8]) -> Result<FileRule> {
        let name = c_str(name);
        let mut rule = FileRule {
            custom_keys: CustomKeysContainer::default(),
            name: name.to_vec(),
            color_space: Mutex::new(Vec::new()),
            pattern: Vec::new(),
            extension: Vec::new(),
            regex: Vec::new(),
            rule_type: RuleType::Glob,
        };
        if rule.name.is_empty() {
            return Err(Exception::new("The file rule name is empty"));
        } else if strcasecmp(name, FileRules::DEFAULT_RULE_NAME).is_eq() {
            rule.name = FileRules::DEFAULT_RULE_NAME.as_bytes().to_vec(); // Enforce case consistency.
            rule.rule_type = RuleType::Default;
        } else if strcasecmp(name, FileRules::FILE_PATH_SEARCH_RULE_NAME).is_eq() {
            rule.name = FileRules::FILE_PATH_SEARCH_RULE_NAME.as_bytes().to_vec(); // Enforce case consistency.
            rule.rule_type = RuleType::ParseFilepath;
        } else {
            rule.pattern = b"*".to_vec();
            rule.extension = b"*".to_vec();
            rule.rule_type = RuleType::Glob;
        }
        Ok(rule)
    }

    fn lock_color_space(&self) -> MutexGuard<'_, Vec<u8>> {
        self.color_space.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Port of `FileRule::getName` (src/OpenColorIO/FileRules.cpp:346-349 @ v2.5.2).
    pub(crate) fn name(&self) -> &[u8] {
        &self.name
    }

    /// The glob pattern; `""` for another kind of rule.
    ///
    /// Port of `FileRule::getPattern` (src/OpenColorIO/FileRules.cpp:351-358 @ v2.5.2).
    fn pattern(&self) -> &[u8] {
        if self.rule_type != RuleType::Glob {
            return &[];
        }
        &self.pattern
    }

    /// Port of `FileRule::setPattern` (src/OpenColorIO/FileRules.cpp:360-381 @ v2.5.2).
    fn set_pattern(&mut self, pattern: &[u8]) -> Result<()> {
        let pattern = c_str(pattern);
        if self.rule_type == RuleType::Default || self.rule_type == RuleType::ParseFilepath {
            if !pattern.is_empty() {
                return Err(Exception::new(
                    "File rules: Default and ColorSpaceNamePathSearch rules do not accept any \
                     pattern.",
                ));
            }
        } else {
            if pattern.is_empty() {
                return Err(Exception::new(
                    "File rules: The file name pattern is empty.",
                ));
            }
            validate_glob(pattern, &self.extension)?;
            self.pattern = pattern.to_vec();
            self.regex.clear();
            self.rule_type = RuleType::Glob;
        }
        Ok(())
    }

    /// The extension; `""` for another kind of rule.
    ///
    /// Port of `FileRule::getExtension` (src/OpenColorIO/FileRules.cpp:383-390 @ v2.5.2).
    fn extension(&self) -> &[u8] {
        if self.rule_type != RuleType::Glob {
            return &[];
        }
        &self.extension
    }

    /// Port of `FileRule::setExtension` (src/OpenColorIO/FileRules.cpp:392-413 @ v2.5.2).
    fn set_extension(&mut self, extension: &[u8]) -> Result<()> {
        let extension = c_str(extension);
        if self.rule_type == RuleType::Default || self.rule_type == RuleType::ParseFilepath {
            if !extension.is_empty() {
                return Err(Exception::new(
                    "File rules: Default and ColorSpaceNamePathSearch rules do not accept any \
                     extension.",
                ));
            }
        } else {
            if extension.is_empty() {
                return Err(Exception::new(
                    "File rules: The file extension pattern is empty.",
                ));
            }
            validate_glob(&self.pattern, extension)?;
            self.extension = extension.to_vec();
            self.regex.clear();
            self.rule_type = RuleType::Glob;
        }
        Ok(())
    }

    /// The regular expression; `""` for another kind of rule.
    ///
    /// Port of `FileRule::getRegex` (src/OpenColorIO/FileRules.cpp:415-422 @ v2.5.2).
    fn regex(&self) -> &[u8] {
        if self.rule_type != RuleType::Regex {
            return &[];
        }
        &self.regex
    }

    /// Port of `FileRule::setRegex` (src/OpenColorIO/FileRules.cpp:424-443 @ v2.5.2).
    fn set_regex(&mut self, regex: &[u8]) -> Result<()> {
        let regex = c_str(regex);
        if self.rule_type == RuleType::Default || self.rule_type == RuleType::ParseFilepath {
            if !regex.is_empty() {
                return Err(Exception::new(
                    "File rules: Default and ColorSpaceNamePathSearch rules do not accept any \
                     regex.",
                ));
            }
        } else {
            validate_regular_expression(regex)?;
            self.regex = regex.to_vec();
            self.pattern.clear();
            self.extension.clear();
            self.rule_type = RuleType::Regex;
        }
        Ok(())
    }

    /// The color space: as set, or for the path search rule, the one it last found in a path.
    ///
    /// Port of `FileRule::getColorSpace` (src/OpenColorIO/FileRules.cpp:445-448 @ v2.5.2).
    pub(crate) fn color_space(&self) -> Vec<u8> {
        self.lock_color_space().clone()
    }

    /// Port of `FileRule::setColorSpace` (src/OpenColorIO/FileRules.cpp:450-467 @ v2.5.2).
    fn set_color_space(&mut self, color_space: &[u8]) -> Result<()> {
        let color_space = c_str(color_space);
        if self.rule_type == RuleType::ParseFilepath {
            if !color_space.is_empty() {
                return Err(Exception::new(
                    "File rules: ColorSpaceNamePathSearch rule does not accept any color space.",
                ));
            }
        } else {
            if color_space.is_empty() {
                return Err(Exception::new(
                    "File rules: color space name can't be empty.",
                ));
            }
            *self
                .color_space
                .get_mut()
                .unwrap_or_else(|e| e.into_inner()) = color_space.to_vec();
        }
        Ok(())
    }
}

impl Clone for FileRule {
    /// Port of `FileRule::clone` (src/OpenColorIO/FileRules.cpp:333-344 @ v2.5.2).
    fn clone(&self) -> FileRule {
        FileRule {
            custom_keys: self.custom_keys.clone(),
            name: self.name.clone(),
            color_space: Mutex::new(self.color_space()),
            pattern: self.pattern.clone(),
            extension: self.extension.clone(),
            regex: self.regex.clone(),
            rule_type: self.rule_type,
        }
    }
}

/// Whether the default rule may be at the index a call checks.
///
/// Port of `FileRules::Impl::DefaultAllowed` (src/OpenColorIO/FileRules.h:34-38 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DefaultAllowed {
    /// `DEFAULT_ALLOWED`.
    Allowed,
    /// `DEFAULT_NOT_ALLOWED`.
    NotAllowed,
}

/// A config's file rules: the rules in order, the default rule last.
///
/// A copy is upstream's `createEditableCopy`: a copy of each rule.
///
/// Port of `FileRules` and `FileRules::Impl` (include/OpenColorIO/OpenColorIO.h:1726-1880,
/// src/OpenColorIO/FileRules.h:30-71, FileRules.cpp:537-958 @ v2.5.2), in part: the matching
/// of paths and the validation come with the config's file rules.
#[derive(Debug, Clone)]
pub struct FileRules {
    /// `m_rules`: all rules, default rule always at the end.
    rules: Vec<FileRule>,
}

impl Default for FileRules {
    fn default() -> FileRules {
        FileRules::new()
    }
}

impl FileRules {
    /// Reserved rule name for the default rule.
    ///
    /// Port of `FileRules::DefaultRuleName` (src/OpenColorIO/FileRules.cpp:24 @ v2.5.2).
    pub const DEFAULT_RULE_NAME: &'static str = "Default";

    /// Reserved rule name for the file path search rule.
    ///
    /// Port of `FileRules::FilePathSearchRuleName` (src/OpenColorIO/FileRules.cpp:25 @ v2.5.2).
    pub const FILE_PATH_SEARCH_RULE_NAME: &'static str = "ColorSpaceNamePathSearch";

    /// File rules with only the default rule, for the `default` role.
    ///
    /// Port of `FileRules::Create` and `FileRules::Impl::Impl` (src/OpenColorIO/FileRules.cpp:
    /// 551-554, 568-573 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> FileRules {
        let mut default_rule =
            FileRule::new(FileRules::DEFAULT_RULE_NAME.as_bytes()).expect("the default rule");
        default_rule
            .set_color_space(ROLE_DEFAULT.as_bytes())
            .expect("the default role");
        FileRules {
            rules: vec![default_rule],
        }
    }

    /// Port of `FileRules::Impl::validatePosition` (src/OpenColorIO/FileRules.cpp:590-606 @
    /// v2.5.2).
    fn validate_position(&self, rule_index: usize, allow_default: DefaultAllowed) -> Result<()> {
        let num_rules = self.rules.len();
        if rule_index >= num_rules {
            return Err(Exception::new(format!(
                "File rules: rule index '{rule_index}' invalid. There are only '{num_rules}' \
                 rules."
            )));
        }
        if allow_default == DefaultAllowed::NotAllowed && rule_index + 1 == num_rules {
            return Err(Exception::new(format!(
                "File rules: rule index '{rule_index}' is the default rule."
            )));
        }
        Ok(())
    }

    /// Port of `FileRules::Impl::validateNewRule` (src/OpenColorIO/FileRules.cpp:608-632 @
    /// v2.5.2).
    fn validate_new_rule(&self, rule_index: usize, name: &[u8]) -> Result<()> {
        if name.is_empty() {
            return Err(Exception::new(
                "File rules: rule should have a non-empty name.",
            ));
        }
        if self
            .rules
            .iter()
            .any(|rule| strcasecmp(name, rule.name()).is_eq())
        {
            return Err(Exception::new(
                [
                    b"File rules: A rule named '".as_slice(),
                    name,
                    b"' already exists.",
                ]
                .concat(),
            ));
        }
        self.validate_position(rule_index, DefaultAllowed::Allowed)?;
        if strcasecmp(name, FileRules::DEFAULT_RULE_NAME).is_eq() {
            return Err(Exception::new(format!(
                "File rules: Default rule already exists at index  '{}'.",
                self.rules.len() - 1
            )));
        }
        Ok(())
    }

    /// Port of `FileRules::Impl::moveRule` (src/OpenColorIO/FileRules.cpp:650-664 @ v2.5.2).
    fn move_rule(&mut self, rule_index: usize, offset: i32) -> Result<()> {
        self.validate_position(rule_index, DefaultAllowed::NotAllowed)?;
        let new_index = rule_index as i32 + offset;
        if new_index < 0 || new_index >= self.rules.len() as i32 - 1 {
            return Err(Exception::new(format!(
                "File rules: rule at index '{rule_index}' may not be moved to index \
                 '{new_index}'."
            )));
        }
        let rule = self.rules.remove(rule_index);
        self.rules.insert(new_index as usize, rule);
        Ok(())
    }

    /// The number of rules, the default rule included.
    ///
    /// Port of `FileRules::getNumEntries` (src/OpenColorIO/FileRules.cpp:687-690 @ v2.5.2).
    #[doc(alias = "getNumEntries")]
    pub fn num_entries(&self) -> usize {
        self.rules.len()
    }

    /// The index of the rule named `rule_name`, ignoring case.
    ///
    /// Port of `FileRules::getIndexForRule` (src/OpenColorIO/FileRules.cpp:692-707 @ v2.5.2).
    #[doc(alias = "getIndexForRule")]
    pub fn index_for_rule(&self, rule_name: impl AsRef<[u8]>) -> Result<usize> {
        let rule_name = c_str(rule_name.as_ref());
        match self
            .rules
            .iter()
            .position(|rule| strcasecmp(rule_name, rule.name()).is_eq())
        {
            Some(idx) => Ok(idx),
            None => Err(Exception::new(
                [
                    b"File rules: rule name '".as_slice(),
                    rule_name,
                    b"' not found.",
                ]
                .concat(),
            )),
        }
    }

    /// Port of `FileRules::getName` (src/OpenColorIO/FileRules.cpp:709-713 @ v2.5.2).
    #[doc(alias = "getName")]
    pub fn name(&self, rule_index: usize) -> Result<&[u8]> {
        self.validate_position(rule_index, DefaultAllowed::Allowed)?;
        Ok(self.rules[rule_index].name())
    }

    /// Port of `FileRules::getPattern` (src/OpenColorIO/FileRules.cpp:715-719 @ v2.5.2).
    #[doc(alias = "getPattern")]
    pub fn pattern(&self, rule_index: usize) -> Result<&[u8]> {
        self.validate_position(rule_index, DefaultAllowed::Allowed)?;
        Ok(self.rules[rule_index].pattern())
    }

    /// Port of `FileRules::setPattern` (src/OpenColorIO/FileRules.cpp:721-725 @ v2.5.2).
    #[doc(alias = "setPattern")]
    pub fn set_pattern(&mut self, rule_index: usize, pattern: impl AsRef<[u8]>) -> Result<()> {
        self.validate_position(rule_index, DefaultAllowed::NotAllowed)?;
        self.rules[rule_index].set_pattern(pattern.as_ref())
    }

    /// Port of `FileRules::getExtension` (src/OpenColorIO/FileRules.cpp:727-731 @ v2.5.2).
    #[doc(alias = "getExtension")]
    pub fn extension(&self, rule_index: usize) -> Result<&[u8]> {
        self.validate_position(rule_index, DefaultAllowed::Allowed)?;
        Ok(self.rules[rule_index].extension())
    }

    /// Port of `FileRules::setExtension` (src/OpenColorIO/FileRules.cpp:733-737 @ v2.5.2).
    #[doc(alias = "setExtension")]
    pub fn set_extension(&mut self, rule_index: usize, extension: impl AsRef<[u8]>) -> Result<()> {
        self.validate_position(rule_index, DefaultAllowed::NotAllowed)?;
        self.rules[rule_index].set_extension(extension.as_ref())
    }

    /// Port of `FileRules::getRegex` (src/OpenColorIO/FileRules.cpp:739-743 @ v2.5.2).
    #[doc(alias = "getRegex")]
    pub fn regex(&self, rule_index: usize) -> Result<&[u8]> {
        self.validate_position(rule_index, DefaultAllowed::Allowed)?;
        Ok(self.rules[rule_index].regex())
    }

    /// Port of `FileRules::setRegex` (src/OpenColorIO/FileRules.cpp:745-749 @ v2.5.2).
    #[doc(alias = "setRegex")]
    pub fn set_regex(&mut self, rule_index: usize, regex: impl AsRef<[u8]>) -> Result<()> {
        self.validate_position(rule_index, DefaultAllowed::NotAllowed)?;
        self.rules[rule_index].set_regex(regex.as_ref())
    }

    /// The color space (or role) of a rule. Owned: the path search rule's changes as it
    /// matches paths (upstream's `mutable` member), so the rules can't lend it.
    ///
    /// Port of `FileRules::getColorSpace` (src/OpenColorIO/FileRules.cpp:751-756 @ v2.5.2).
    #[doc(alias = "getColorSpace")]
    pub fn color_space(&self, rule_index: usize) -> Result<Vec<u8>> {
        self.validate_position(rule_index, DefaultAllowed::Allowed)?;
        Ok(self.rules[rule_index].color_space())
    }

    /// Port of `FileRules::setColorSpace` (src/OpenColorIO/FileRules.cpp:758-762 @ v2.5.2).
    #[doc(alias = "setColorSpace")]
    pub fn set_color_space(
        &mut self,
        rule_index: usize,
        color_space: impl AsRef<[u8]>,
    ) -> Result<()> {
        self.validate_position(rule_index, DefaultAllowed::Allowed)?;
        self.rules[rule_index].set_color_space(color_space.as_ref())
    }

    /// Port of `FileRules::getNumCustomKeys` (src/OpenColorIO/FileRules.cpp:764-768 @ v2.5.2).
    #[doc(alias = "getNumCustomKeys")]
    pub fn num_custom_keys(&self, rule_index: usize) -> Result<usize> {
        self.validate_position(rule_index, DefaultAllowed::Allowed)?;
        Ok(self.rules[rule_index].custom_keys.size())
    }

    /// "File rules: the custom key access for file rule '<name>' failed: <what>"
    fn custom_key_error(&self, rule_index: usize, e: Exception) -> Exception {
        Exception::new(
            [
                b"File rules: the custom key access for file rule '".as_slice(),
                self.rules[rule_index].name(),
                b"' failed: ",
                e.what(),
            ]
            .concat(),
        )
    }

    /// Port of `FileRules::getCustomKeyName` (src/OpenColorIO/FileRules.cpp:770-785 @ v2.5.2).
    #[doc(alias = "getCustomKeyName")]
    pub fn custom_key_name(&self, rule_index: usize, key: usize) -> Result<&[u8]> {
        self.validate_position(rule_index, DefaultAllowed::Allowed)?;
        self.rules[rule_index]
            .custom_keys
            .name(key)
            .map_err(|e| self.custom_key_error(rule_index, e))
    }

    /// Port of `FileRules::getCustomKeyValue` (src/OpenColorIO/FileRules.cpp:787-802 @ v2.5.2).
    #[doc(alias = "getCustomKeyValue")]
    pub fn custom_key_value(&self, rule_index: usize, key: usize) -> Result<&[u8]> {
        self.validate_position(rule_index, DefaultAllowed::Allowed)?;
        self.rules[rule_index]
            .custom_keys
            .value(key)
            .map_err(|e| self.custom_key_error(rule_index, e))
    }

    /// Sets the custom key `key` of a rule to `value`, or removes it for an empty value.
    ///
    /// Port of `FileRules::setCustomKey` (src/OpenColorIO/FileRules.cpp:804-818 @ v2.5.2).
    #[doc(alias = "setCustomKey")]
    pub fn set_custom_key(
        &mut self,
        rule_index: usize,
        key: impl AsRef<[u8]>,
        value: impl AsRef<[u8]>,
    ) -> Result<()> {
        self.validate_position(rule_index, DefaultAllowed::Allowed)?;
        let rule = &mut self.rules[rule_index];
        rule.custom_keys
            .set(key.as_ref(), value.as_ref())
            .map_err(|e| {
                Exception::new(
                    [
                        b"File rules: rule named '".as_slice(),
                        rule.name(),
                        b"' error: ",
                        e.what(),
                    ]
                    .concat(),
                )
            })
    }

    /// Inserts a glob rule at `rule_index`: its name (trimmed), color space, pattern and
    /// extension.
    ///
    /// Port of `FileRules::insertRule(size_t, const char *, const char *, const char *, const
    /// char *)` (src/OpenColorIO/FileRules.cpp:820-832 @ v2.5.2).
    #[doc(alias = "insertRule")]
    pub fn insert_rule(
        &mut self,
        rule_index: usize,
        name: impl AsRef<[u8]>,
        color_space: impl AsRef<[u8]>,
        pattern: impl AsRef<[u8]>,
        extension: impl AsRef<[u8]>,
    ) -> Result<()> {
        let rule_name = trim(c_str(name.as_ref())).to_vec();

        self.validate_new_rule(rule_index, &rule_name)?;

        let mut new_rule = FileRule::new(&rule_name)?;
        new_rule.set_color_space(color_space.as_ref())?;
        new_rule.set_pattern(pattern.as_ref())?;
        new_rule.set_extension(extension.as_ref())?;
        self.rules.insert(rule_index, new_rule);
        Ok(())
    }

    /// Inserts a regular expression rule at `rule_index`: its name (trimmed), color space and
    /// expression.
    ///
    /// Port of `FileRules::insertRule(size_t, const char *, const char *, const char *)`
    /// (src/OpenColorIO/FileRules.cpp:834-845 @ v2.5.2).
    #[doc(alias = "insertRule")]
    pub fn insert_regex_rule(
        &mut self,
        rule_index: usize,
        name: impl AsRef<[u8]>,
        color_space: impl AsRef<[u8]>,
        regex: impl AsRef<[u8]>,
    ) -> Result<()> {
        let rule_name = trim(c_str(name.as_ref())).to_vec();

        self.validate_new_rule(rule_index, &rule_name)?;

        let mut new_rule = FileRule::new(&rule_name)?;
        new_rule.set_color_space(color_space.as_ref())?;
        new_rule.set_regex(regex.as_ref())?;
        self.rules.insert(rule_index, new_rule);
        Ok(())
    }

    /// Inserts the path search rule at `rule_index`: the color space whose name the path holds.
    ///
    /// Port of `FileRules::insertPathSearchRule` (src/OpenColorIO/FileRules.cpp:847-850 @
    /// v2.5.2).
    #[doc(alias = "insertPathSearchRule")]
    pub fn insert_path_search_rule(&mut self, rule_index: usize) -> Result<()> {
        self.insert_regex_rule(rule_index, FileRules::FILE_PATH_SEARCH_RULE_NAME, b"", b"")
    }

    /// Port of `FileRules::setDefaultRuleColorSpace` (src/OpenColorIO/FileRules.cpp:852-855 @
    /// v2.5.2).
    #[doc(alias = "setDefaultRuleColorSpace")]
    pub fn set_default_rule_color_space(&mut self, color_space: impl AsRef<[u8]>) -> Result<()> {
        self.rules
            .last_mut()
            .expect("the default rule")
            .set_color_space(color_space.as_ref())
    }

    /// Removes the rule at `rule_index` (not the default rule).
    ///
    /// Port of `FileRules::removeRule` (src/OpenColorIO/FileRules.cpp:857-861 @ v2.5.2).
    #[doc(alias = "removeRule")]
    pub fn remove_rule(&mut self, rule_index: usize) -> Result<()> {
        self.validate_position(rule_index, DefaultAllowed::NotAllowed)?;
        self.rules.remove(rule_index);
        Ok(())
    }

    /// Moves the rule at `rule_index` one up.
    ///
    /// Port of `FileRules::increaseRulePriority` (src/OpenColorIO/FileRules.cpp:863-866 @
    /// v2.5.2).
    #[doc(alias = "increaseRulePriority")]
    pub fn increase_rule_priority(&mut self, rule_index: usize) -> Result<()> {
        self.move_rule(rule_index, -1)
    }

    /// Moves the rule at `rule_index` one down.
    ///
    /// Port of `FileRules::decreaseRulePriority` (src/OpenColorIO/FileRules.cpp:868-871 @
    /// v2.5.2).
    #[doc(alias = "decreaseRulePriority")]
    pub fn decrease_rule_priority(&mut self, rule_index: usize) -> Result<()> {
        self.move_rule(rule_index, 1)
    }

    /// Whether the rules are only the default rule, for the `default` role (any case), without
    /// custom keys.
    ///
    /// Port of `FileRules::isDefault` (src/OpenColorIO/FileRules.cpp:873-890 @ v2.5.2).
    #[doc(alias = "isDefault")]
    pub fn is_default(&self) -> bool {
        if self.rules.len() == 1 {
            let rule = &self.rules[0];
            // NB: Don't need to check the rule name -- the default rule may not be removed,
            // so if there is only one rule, it's the default one.
            if rule.custom_keys.size() == 0 && compare(&rule.color_space(), ROLE_DEFAULT.as_bytes())
            {
                return true;
            }
        }
        false
    }

    /// `operator<<`: a line per rule.
    ///
    /// Port of `operator<<(std::ostream &, const FileRules &)` (src/OpenColorIO/FileRules.cpp:
    /// 912-958 @ v2.5.2).
    pub fn write_to(&self, os: &mut Vec<u8>) {
        let num_rules = self.num_entries();
        for (r, rule) in self.rules.iter().enumerate() {
            os.extend_from_slice(b"<FileRule name=");
            os.extend_from_slice(rule.name());
            let cs = rule.color_space();
            if !cs.is_empty() {
                os.extend_from_slice(b", colorspace=");
                os.extend_from_slice(&cs);
            }
            let regex = rule.regex();
            if !regex.is_empty() {
                os.extend_from_slice(b", regex=");
                os.extend_from_slice(regex);
            }
            let pattern = rule.pattern();
            if !pattern.is_empty() {
                os.extend_from_slice(b", pattern=");
                os.extend_from_slice(pattern);
            }
            let extension = rule.extension();
            if !extension.is_empty() {
                os.extend_from_slice(b", extension=");
                os.extend_from_slice(extension);
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

impl fmt::Display for FileRules {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.to_bytes()))
    }
}
