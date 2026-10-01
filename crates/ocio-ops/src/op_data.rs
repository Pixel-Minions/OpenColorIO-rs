// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Op data: the parameters of an op, with its basic behaviors (whether it is a no-op or an
//! identity, whether it mixes channels, its cache ID). A port of the `OpData` part of
//! `src/OpenColorIO/Op.h` and `Op.cpp` @ v2.5.2.
//!
//! Upstream's `OpData` is an abstract class with a subclass per op type. Here it is an enum
//! with a variant per type (`docs/architecture.md`, "Op data and ops"). Each variant holds the
//! port of its type's op data class, with that class's `FormatMetadataImpl`. The variants come
//! with their families (WP 1.3). Every match over `OpData` is exhaustive, with no wildcard
//! arm, so adding a family touches every match.

use std::sync::Arc;

use crate::exception::{Exception, Result};
use crate::format_metadata::{FormatMetadataImpl, METADATA_ID, METADATA_NAME};
use crate::ops::log::log_op_data::LogOpData;
use crate::ops::matrix::MatrixOpData;
use crate::ops::noop::NoOpData;
use crate::ops::range::RangeOpData;
use crate::ops::reference::ReferenceOpData;

/// The type of an op's data.
///
/// Port of `OpData::Type` (src/OpenColorIO/Op.h:98-118 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpDataType {
    /// `CDLType`: a Color Decision List (CDL).
    Cdl,
    /// `ExponentType`: an exponent.
    Exponent,
    /// `ExposureContrastType`: interactive viewport adjustments.
    ExposureContrast,
    /// `FixedFunctionType`: a fixed function, whose style defines the behavior.
    FixedFunction,
    /// `GammaType`: a gamma (an enhanced exponent).
    Gamma,
    /// `GradingPrimaryType`: primary grading controls.
    GradingPrimary,
    /// `GradingRGBCurveType`: an RGB curve.
    GradingRgbCurve,
    /// `GradingHueCurveType`: a hue curve.
    GradingHueCurve,
    /// `GradingToneType`: grading controls for tonal ranges.
    GradingTone,
    /// `LogType`: a log.
    Log,
    /// `Lut1DType`: a 1D LUT.
    Lut1D,
    /// `Lut3DType`: a 3D LUT.
    Lut3D,
    /// `MatrixType`: a matrix.
    Matrix,
    /// `RangeType`: a range.
    Range,
    /// `ReferenceType`: a reference to an external file.
    Reference,
    /// `NoOpType`: the data of an op that leaves pixels alone.
    NoOp,
}

/// The type's name in messages: `CDL`, `Exponent`, ..., `LUT1D`, `LUT3D`, `Matrix`, `Range`.
/// The reference and no-op types have none: "Unexpected op type.".
///
/// Port of `GetTypeName` (src/OpenColorIO/Op.cpp:106-143 @ v2.5.2).
pub fn get_type_name(op_type: OpDataType) -> Result<&'static str> {
    Ok(match op_type {
        OpDataType::Cdl => "CDL",
        OpDataType::Exponent => "Exponent",
        OpDataType::ExposureContrast => "ExposureContrast",
        OpDataType::FixedFunction => "FixedFunction",
        OpDataType::Gamma => "Gamma",
        OpDataType::GradingPrimary => "GradingPrimary",
        OpDataType::GradingRgbCurve => "GradingRGBCurve",
        OpDataType::GradingHueCurve => "GradingHueCurve",
        OpDataType::GradingTone => "GradingTone",
        OpDataType::Log => "Log",
        OpDataType::Lut1D => "LUT1D",
        OpDataType::Lut3D => "LUT3D",
        OpDataType::Matrix => "Matrix",
        OpDataType::Range => "Range",
        OpDataType::Reference | OpDataType::NoOp => {
            return Err(Exception::new("Unexpected op type."));
        }
    })
}

/// An op's data: its type's parameters and format metadata.
///
/// `Clone` is upstream's copy constructor: the variant's class copies its parameters, and the
/// `OpData` base its metadata (src/OpenColorIO/Op.cpp:44-62 @ v2.5.2). `==` is upstream's
/// `operator==` ([`equals`](Self::equals)).
///
/// Port of `OpData` (src/OpenColorIO/Op.h:93-171, Op.cpp:44-104 @ v2.5.2).
#[derive(Debug, Clone)]
pub enum OpData {
    /// `LogOpData`.
    Log(LogOpData),
    /// `MatrixOpData`.
    Matrix(MatrixOpData),
    /// `RangeOpData`.
    Range(RangeOpData),
    /// `ReferenceOpData`.
    Reference(ReferenceOpData),
    /// `NoOpData` and its subclass `FileNoOpData`.
    NoOp(NoOpData),
}

/// A shared op data: `OpDataRcPtr`.
pub type OpDataRcPtr = Arc<OpData>;

/// A list of op data: `OpDataVec` (src/OpenColorIO/Op.h:56 @ v2.5.2).
pub type OpDataVec = Vec<OpDataRcPtr>;

impl OpData {
    /// The type of the data.
    ///
    /// Port of `OpData::getType`, pure virtual (src/OpenColorIO/Op.h:136 @ v2.5.2).
    pub fn get_type(&self) -> OpDataType {
        match self {
            OpData::Log(data) => data.get_type(),
            OpData::Matrix(data) => data.get_type(),
            OpData::Range(data) => data.get_type(),
            OpData::Reference(_) => OpDataType::Reference,
            OpData::NoOp(_) => OpDataType::NoOp,
        }
    }

    /// Checks the parameters, with upstream's message for the first invalid one.
    ///
    /// Port of `OpData::validate`, pure virtual (src/OpenColorIO/Op.h:134 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        match self {
            // On a shared reference: a 3x3 matrix is checked as its 4x4 form, which upstream's
            // `const` validate keeps through a `const_cast` (`MatrixOpData::validate_ref`).
            OpData::Log(data) => data.validate(),
            OpData::Matrix(data) => data.validate_ref(),
            // `const`, and fills the `mutable` scale and offset.
            OpData::Range(data) => data.validate(),
            OpData::Reference(data) => {
                data.validate();
                Ok(())
            }
            OpData::NoOp(data) => {
                data.validate();
                Ok(())
            }
        }
    }

    /// Whether the op leaves every pixel as it is.
    ///
    /// Port of `OpData::isNoOp`, pure virtual (src/OpenColorIO/Op.h:138-139 @ v2.5.2).
    pub fn is_no_op(&self) -> Result<bool> {
        match self {
            OpData::Log(data) => Ok(data.is_no_op()),
            OpData::Matrix(data) => data.is_no_op(),
            OpData::Range(data) => Ok(data.is_no_op()),
            OpData::Reference(data) => Ok(data.is_no_op()),
            OpData::NoOp(data) => Ok(data.is_no_op()),
        }
    }

    /// Whether the op leaves pixels as they are in its intended domain; it may clamp outside
    /// it. For example, a Lut1D may be an identity without being a no-op.
    ///
    /// Port of `OpData::isIdentity`, pure virtual (src/OpenColorIO/Op.h:141-143 @ v2.5.2).
    pub fn is_identity(&self) -> Result<bool> {
        match self {
            OpData::Log(data) => Ok(data.is_identity()),
            OpData::Matrix(data) => data.is_identity(),
            OpData::Range(data) => Ok(data.is_identity()),
            OpData::Reference(data) => Ok(data.is_identity()),
            OpData::NoOp(data) => Ok(data.is_identity()),
        }
    }

    /// Appends to `ops` simpler op data that do what this one does, if the parameters allow
    /// it; the optimizer then replaces the op with them. By default there are none.
    ///
    /// Port of `OpData::getSimplerReplacement` (src/OpenColorIO/Op.h:147-148, Op.cpp:69-71 @
    /// v2.5.2).
    pub fn get_simpler_replacement(&self, _ops: &mut OpDataVec) -> Result<()> {
        match self {
            // The OpData default: nothing.
            OpData::Log(_)
            | OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Reference(_)
            | OpData::NoOp(_) => Ok(()),
        }
    }

    /// The data of an op that replaces this one where the optimizer finds it to be an
    /// identity: an identity matrix by default, a range for the types that clamp. A Log's can
    /// raise ([`LogOpData::get_identity_replacement`]).
    ///
    /// Port of `OpData::getIdentityReplacement` (src/OpenColorIO/Op.h:145, Op.cpp:64-67 @
    /// v2.5.2) and its overrides.
    pub fn get_identity_replacement(&self) -> Result<OpData> {
        match self {
            OpData::Log(data) => data.get_identity_replacement(),
            // The OpData default: `std::make_shared<MatrixOpData>()`, the identity.
            OpData::Matrix(_) | OpData::Range(_) | OpData::Reference(_) | OpData::NoOp(_) => {
                Ok(OpData::Matrix(MatrixOpData::new()))
            }
        }
    }

    /// Whether the output of a channel depends on the other channels: `Rout = 5 * Rin` doesn't
    /// mix channels, `Rout = Rin + Gin` does. It may depend on the parameters.
    ///
    /// Port of `OpData::hasChannelCrosstalk`, pure virtual (src/OpenColorIO/Op.h:150-155 @
    /// v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        match self {
            OpData::Log(data) => data.has_channel_crosstalk(),
            OpData::Matrix(data) => data.has_channel_crosstalk(),
            OpData::Range(data) => data.has_channel_crosstalk(),
            OpData::Reference(data) => data.has_channel_crosstalk(),
            OpData::NoOp(data) => data.has_channel_crosstalk(),
        }
    }

    /// Whether `other` has the same type and parameters. The metadata is ignored.
    ///
    /// Port of `OpData::equals` (src/OpenColorIO/Op.cpp:73-79 @ v2.5.2) and its overrides. The
    /// base compares the types (after a shortcut for the same object, which a type comparison
    /// also covers); each override then compares its parameters.
    pub fn equals(&self, other: &OpData) -> bool {
        match self {
            OpData::Log(data) => matches!(other, OpData::Log(other) if data.equals(other)),
            OpData::Matrix(data) => matches!(other, OpData::Matrix(other) if data.equals(other)),
            OpData::Range(data) => matches!(other, OpData::Range(other) if data.equals(other)),
            OpData::Reference(data) => {
                matches!(other, OpData::Reference(other) if data.equals(other))
            }
            // NoOpData keeps the OpData base: the type only.
            OpData::NoOp(_) => other.get_type() == OpDataType::NoOp,
        }
    }

    /// A text that identifies the data, for the cache IDs of processors.
    ///
    /// Port of `OpData::getCacheID`, pure virtual (src/OpenColorIO/Op.h:159-160 @ v2.5.2).
    pub fn get_cache_id(&self) -> Result<Vec<u8>> {
        match self {
            OpData::Log(data) => data.get_cache_id(),
            OpData::Matrix(data) => data.get_cache_id(),
            OpData::Range(data) => Ok(data.get_cache_id()),
            OpData::Reference(data) => data.get_cache_id(),
            OpData::NoOp(data) => Ok(data.get_cache_id()),
        }
    }

    /// The format metadata: the id, the name, and the descriptions a file gave the op.
    ///
    /// Port of `OpData::getFormatMetadata() const` (src/OpenColorIO/Op.h:164 @ v2.5.2).
    pub fn get_format_metadata(&self) -> &FormatMetadataImpl {
        match self {
            OpData::Log(data) => data.get_format_metadata(),
            OpData::Matrix(data) => data.get_format_metadata(),
            OpData::Range(data) => data.get_format_metadata(),
            OpData::Reference(data) => data.get_format_metadata(),
            OpData::NoOp(data) => data.get_format_metadata(),
        }
    }

    /// The format metadata, to change.
    ///
    /// Port of `OpData::getFormatMetadata()` (src/OpenColorIO/Op.h:163 @ v2.5.2).
    pub fn get_format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        match self {
            OpData::Log(data) => data.get_format_metadata_mut(),
            OpData::Matrix(data) => data.get_format_metadata_mut(),
            OpData::Range(data) => data.get_format_metadata_mut(),
            OpData::Reference(data) => data.get_format_metadata_mut(),
            OpData::NoOp(data) => data.get_format_metadata_mut(),
        }
    }

    /// The whole value of the metadata's `id` attribute (found ignoring ASCII case), or `""`.
    ///
    /// Port of `OpData::getID` (src/OpenColorIO/Op.cpp:81-84 @ v2.5.2).
    pub fn get_id(&self) -> &[u8] {
        self.get_format_metadata()
            .get_attribute_value_string(Some(METADATA_ID))
    }

    /// Sets the metadata's `id` attribute. Upstream passes the string on as a C string, so it
    /// ends at the first NUL.
    ///
    /// Port of `OpData::setID` (src/OpenColorIO/Op.cpp:86-89 @ v2.5.2).
    pub fn set_id(&mut self, id: &[u8]) {
        self.get_format_metadata_mut().set_id(Some(id));
    }

    /// The whole value of the metadata's `name` attribute (found ignoring ASCII case), or
    /// `""`.
    ///
    /// Port of `OpData::getName` (src/OpenColorIO/Op.cpp:91-94 @ v2.5.2).
    pub fn get_name(&self) -> &[u8] {
        self.get_format_metadata()
            .get_attribute_value_string(Some(METADATA_NAME))
    }

    /// Sets the metadata's `name` attribute. Upstream passes the string on as a C string, so
    /// it ends at the first NUL.
    ///
    /// Port of `OpData::setName` (src/OpenColorIO/Op.cpp:96-99 @ v2.5.2).
    pub fn set_name(&mut self, name: &[u8]) {
        self.get_format_metadata_mut().set_name(Some(name));
    }
}

impl PartialEq for OpData {
    /// Port of `operator==(const OpData &, const OpData &)` (src/OpenColorIO/Op.cpp:101-104 @
    /// v2.5.2).
    fn eq(&self, other: &OpData) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "op_data_tests.rs"]
mod tests;
