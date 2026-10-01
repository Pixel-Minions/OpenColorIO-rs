// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Exponent op's data, `ExponentOpData`: part of a port of
//! `src/OpenColorIO/ops/exponent/ExponentOp.h` and `ExponentOp.cpp` @ v2.5.2.

use crate::cfmt::{Crt, OStringStream};
use crate::format_metadata::{FormatMetadataImpl, METADATA_ID, METADATA_NAME};
use crate::math_utils::is_vec_equal_to_one;
use crate::op_data::OpDataType;

/// The precision of the values in the cache ID (`DefaultValues::FLOAT_DECIMALS`,
/// ExponentOp.cpp:18-21 @ v2.5.2).
const FLOAT_DECIMALS: i64 = 7;

/// An exponent per channel, R, G, B and A: `out = max(in, 0) ^ exponent`.
///
/// `Clone` is upstream's copy constructor, which copies the values and the metadata.
///
/// Port of `ExponentOpData` (ExponentOp.h:18-38, ExponentOp.cpp:23-94 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct ExponentOpData {
    /// The `OpData` base's `m_metadata`.
    metadata: FormatMetadataImpl,
    /// `m_exp4`: the R, G, B and A exponents.
    pub exp4: [f64; 4],
}

impl Default for ExponentOpData {
    fn default() -> Self {
        Self::new()
    }
}

impl ExponentOpData {
    /// Exponents of 1, an identity.
    ///
    /// Port of `ExponentOpData::ExponentOpData()` (ExponentOp.cpp:23-30 @ v2.5.2).
    pub fn new() -> Self {
        Self::from_values(&[1.0; 4])
    }

    /// The exponents `exp4`, R, G, B and A. (Upstream's constructor takes a pointer and
    /// refuses a null one, "ExponentOpData: exp4 must not be null."; an array can't be null.)
    ///
    /// Port of `ExponentOpData::ExponentOpData(const double *)` (ExponentOp.cpp:41-49 @
    /// v2.5.2).
    pub fn from_values(exp4: &[f64; 4]) -> Self {
        ExponentOpData {
            metadata: FormatMetadataImpl::default(),
            exp4: *exp4,
        }
    }

    /// Port of `ExponentOpData::getType` (ExponentOp.h:28 @ v2.5.2).
    pub fn get_type(&self) -> OpDataType {
        OpDataType::Exponent
    }

    /// Whether every exponent is 1 ([`is_identity`](Self::is_identity)).
    ///
    /// Port of `ExponentOpData::isNoOp` (ExponentOp.cpp:62-65 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        self.is_identity()
    }

    /// Whether every exponent is 1, within 2 float ULPs (`IsVecEqualToOne`).
    ///
    /// Port of `ExponentOpData::isIdentity` (ExponentOp.cpp:67-70 @ v2.5.2).
    pub fn is_identity(&self) -> bool {
        is_vec_equal_to_one(&self.exp4)
    }

    /// Never: each channel is its own exponent.
    ///
    /// Port of `ExponentOpData::hasChannelCrosstalk` (ExponentOp.h:33 @ v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        false
    }

    /// The ID and a space, if there is an ID, then each exponent with 7 significant digits
    /// (as `std::ostream` writes a `double` at that precision) and a space.
    ///
    /// Port of `ExponentOpData::getCacheID` (ExponentOp.cpp:72-90 @ v2.5.2).
    pub fn get_cache_id(&self) -> Vec<u8> {
        let mut cache_id_stream = OStringStream::new(Crt::NATIVE);
        let mut out = Vec::new();
        if !self.get_id().is_empty() {
            out.extend_from_slice(self.get_id());
            out.push(b' ');
        }

        cache_id_stream.precision = FLOAT_DECIMALS;
        for value in self.exp4 {
            cache_id_stream.put_f64(value);
            cache_id_stream.put_str(" ");
        }
        out.extend_from_slice(cache_id_stream.str().as_bytes());
        out
    }

    /// Nothing to check.
    ///
    /// Port of `ExponentOpData::validate` (ExponentOp.cpp:92-94 @ v2.5.2).
    pub fn validate(&self) {}

    /// Port of `OpData::getFormatMetadata() const` (src/OpenColorIO/Op.h:164 @ v2.5.2).
    pub fn get_format_metadata(&self) -> &FormatMetadataImpl {
        &self.metadata
    }

    /// Port of `OpData::getFormatMetadata()` (src/OpenColorIO/Op.h:163 @ v2.5.2).
    pub fn get_format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        &mut self.metadata
    }

    /// Port of `OpData::getID` (src/OpenColorIO/Op.cpp:81-84 @ v2.5.2).
    pub fn get_id(&self) -> &[u8] {
        self.metadata.get_attribute_value_string(Some(METADATA_ID))
    }

    /// Port of `OpData::setID` (src/OpenColorIO/Op.cpp:86-89 @ v2.5.2).
    pub fn set_id(&mut self, id: &[u8]) {
        self.metadata.set_id(Some(id));
    }

    /// Port of `OpData::getName` (src/OpenColorIO/Op.cpp:91-94 @ v2.5.2).
    pub fn get_name(&self) -> &[u8] {
        self.metadata
            .get_attribute_value_string(Some(METADATA_NAME))
    }

    /// Port of `OpData::setName` (src/OpenColorIO/Op.cpp:96-99 @ v2.5.2).
    pub fn set_name(&mut self, name: &[u8]) {
        self.metadata.set_name(Some(name));
    }
}
