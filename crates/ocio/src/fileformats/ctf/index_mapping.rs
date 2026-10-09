// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The index map of a CLF LUT: pairs of an input code value and the LUT position it maps to.
//!
//! Port of `src/OpenColorIO/fileformats/ctf/IndexMapping.h` and `IndexMapping.cpp` @ v2.5.2.

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::{Exception, Result};

/// `IndexMapping::Data`: an input value and its index.
pub type Data = (f32, f32);
/// `IndexMapping::ComponentData`.
pub type ComponentData = Vec<Data>;
/// `IndexMapping::Indices`: the three components' pairs (only the first is used).
pub type Indices = [ComponentData; 3];

/// The index mapping for a LUT: a list that is the new mapping of input code values to index
/// positions in a LUT. It holds one map, the first component's. The 1D and 3D LUTs have none:
/// the reader turns a map into a separate Range op.
///
/// Port of `IndexMapping` (IndexMapping.h:26-63, IndexMapping.cpp:11-101 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct IndexMapping {
    /// `m_dimension`.
    dimension: usize,
    /// `m_indices`.
    indices: Indices,
}

impl IndexMapping {
    /// A map of `dimension` pairs of zeros.
    ///
    /// Port of `IndexMapping::IndexMapping` (IndexMapping.cpp:11-15 @ v2.5.2).
    pub fn new(dimension: usize) -> IndexMapping {
        let mut mapping = IndexMapping {
            dimension,
            indices: Default::default(),
        };
        mapping.resize(dimension);
        mapping
    }

    /// Port of `IndexMapping::resize` (IndexMapping.cpp:21-27 @ v2.5.2).
    pub fn resize(&mut self, dimension: usize) {
        self.dimension = dimension;
        // Currently only supporting one index map per LUT.
        self.indices[0].resize(dimension, (0.0, 0.0));
    }

    /// Port of `IndexMapping::getDimension` (IndexMapping.cpp:29-32 @ v2.5.2).
    pub fn get_dimension(&self) -> usize {
        self.dimension
    }

    /// Port of `IndexMapping::getIndices` (IndexMapping.cpp:34-37 @ v2.5.2).
    pub fn get_indices(&self) -> &Indices {
        &self.indices
    }

    /// Port of `IndexMapping::getNumComponents` (IndexMapping.cpp:39-42 @ v2.5.2).
    pub fn get_num_components(&self) -> u32 {
        3
    }

    /// Port of `IndexMapping::validateIndex` (IndexMapping.cpp:44-54 @ v2.5.2).
    fn validate_index(&self, index: usize) -> Result<()> {
        if index >= self.indices[0].len() {
            let mut oss = OStringStream::new(Crt::NATIVE);
            oss.put_str("IndexMapping: Index ");
            oss.put_u64(index as u64);
            oss.put_str(" is invalid. Should be less than ");
            oss.put_u64(self.indices[0].len() as u64);
            oss.put_str(".");
            return Err(Exception::new(oss.into_bytes()));
        }
        Ok(())
    }

    /// The pair at `index`.
    ///
    /// Port of `IndexMapping::getPair` (IndexMapping.cpp:56-62 @ v2.5.2).
    pub fn get_pair(&self, index: usize) -> Result<(f32, f32)> {
        self.validate_index(index)?;

        Ok(self.indices[0][index])
    }

    /// Port of `IndexMapping::setPair` (IndexMapping.cpp:64-70 @ v2.5.2).
    pub fn set_pair(&mut self, index: usize, first: f32, second: f32) -> Result<()> {
        self.validate_index(index)?;

        self.indices[0][index] = (first, second);
        Ok(())
    }

    /// Checks that both halves of the map increase (a NaN compares as increasing).
    ///
    /// Port of `IndexMapping::validate` (IndexMapping.cpp:72-87 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        // Check that both halves of the index map is increasing.
        // For now we are not validating that the index is within the length
        // of the LUT since the LUT renderer will safely handle that situation.
        for i in 1..self.dimension {
            let (first, second) = self.get_pair(i)?;
            let (prev_first, prev_second) = self.get_pair(i - 1)?;
            if first <= prev_first || second <= prev_second {
                return Err(Exception::new("Index values must be increasing."));
            }
        }
        Ok(())
    }
}

impl PartialEq for IndexMapping {
    /// The same dimension and the same pairs (floats compared with `==`).
    ///
    /// Port of `IndexMapping::operator==` (IndexMapping.cpp:89-101 @ v2.5.2).
    fn eq(&self, other: &IndexMapping) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }

        if self.dimension != other.dimension {
            return false;
        }

        for i in 0..self.get_num_components() as usize {
            if self.indices[i] != other.indices[i] {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
#[path = "index_mapping_tests.rs"]
mod tests;
