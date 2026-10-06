// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The custom keys of a file or viewing rule: a port of `src/OpenColorIO/CustomKeys.h` @
//! v2.5.2.

use std::collections::BTreeMap;

use ocio_ops::exception::{Exception, Result};
use ocio_ops::utils::string_utils::c_str;

/// A rule's custom keys and their values, in byte order of the keys.
///
/// Port of `CustomKeysContainer` (src/OpenColorIO/CustomKeys.h:19-98 @ v2.5.2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CustomKeysContainer {
    /// `m_customKeys`.
    custom_keys: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl CustomKeysContainer {
    /// Port of `CustomKeysContainer::getSize` (CustomKeys.h:28-31 @ v2.5.2).
    pub(crate) fn size(&self) -> usize {
        self.custom_keys.len()
    }

    /// The key at `idx`.
    ///
    /// Port of `CustomKeysContainer::getName` (CustomKeys.h:33-38 @ v2.5.2).
    pub(crate) fn name(&self, idx: usize) -> Result<&[u8]> {
        self.validate_index(idx)?;
        Ok(self.custom_keys.keys().nth(idx).expect("a valid index"))
    }

    /// The value at `idx`.
    ///
    /// Port of `CustomKeysContainer::getValue` (CustomKeys.h:40-45 @ v2.5.2).
    pub(crate) fn value(&self, idx: usize) -> Result<&[u8]> {
        self.validate_index(idx)?;
        Ok(self.custom_keys.values().nth(idx).expect("a valid index"))
    }

    /// Sets `key` to `value`, or removes it for an empty value (upstream's null pointer too).
    ///
    /// Port of `CustomKeysContainer::set` (CustomKeys.h:47-62 @ v2.5.2).
    pub(crate) fn set(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        let key = c_str(key);
        let value = c_str(value);
        if key.is_empty() {
            return Err(Exception::new("Key has to be a non-empty string."));
        }
        if !value.is_empty() {
            self.custom_keys.insert(key.to_vec(), value.to_vec());
        } else {
            self.custom_keys.remove(key);
        }
        Ok(())
    }

    /// Port of `CustomKeysContainer::validateIndex` (CustomKeys.h:85-95 @ v2.5.2).
    fn validate_index(&self, key: usize) -> Result<()> {
        let num_keys = self.size();
        if key >= num_keys {
            return Err(Exception::new(format!(
                "Key index '{key}' is invalid, there are '{num_keys}' custom keys."
            )));
        }
        Ok(())
    }
}
