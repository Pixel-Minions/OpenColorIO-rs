// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The arrays of values of the ops that hold one (Lut1D, Lut3D, Matrix): a port of
//! `src/OpenColorIO/ops/OpArray.h` @ v2.5.2.
//!
//! So far what the Matrix and Lut1D ops use. Upstream's `ArrayT` is an abstract class template: how many
//! values the dimensions call for is its subclass's (`ArrayBase::getNumValues`, pure virtual),
//! so the methods that need that number take it.

use core::ffi::c_ulong;
use std::ops::{Index, IndexMut};

use crate::exception::{Exception, Result};
use crate::math_utils::{SseFloat, sse_mul};

/// The values of a LUT or a matrix, with their dimensions: `length` (entries per side) and the
/// number of color components.
///
/// Port of `ArrayT<T>` (src/OpenColorIO/ops/OpArray.h:33-206 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct ArrayT<T> {
    /// `m_length`.
    length: c_ulong,
    /// `m_numColorComponents`.
    num_color_components: c_ulong,
    /// `m_data`.
    data: Vec<T>,
}

/// `ArrayT<double>`: `ArrayDouble` (src/OpenColorIO/ops/OpArray.h:208 @ v2.5.2).
pub type ArrayDouble = ArrayT<f64>;

/// `ArrayT<float>`: `Array` (src/OpenColorIO/ops/OpArray.h:209 @ v2.5.2).
pub type Array = ArrayT<f32>;

impl<T: Copy + Default> Default for ArrayT<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + Default> ArrayT<T> {
    /// An empty array: no length, no component.
    ///
    /// Port of `ArrayT::ArrayT()` (OpArray.h:44-48 @ v2.5.2).
    pub fn new() -> Self {
        ArrayT {
            length: 0,
            num_color_components: 0,
            data: Vec::new(),
        }
    }

    /// Sets the dimensions, and resizes the values to `num_values`, what the op's array has for
    /// them: new values are 0, as `std::vector::resize` value-initializes them.
    ///
    /// Port of `ArrayT::resize` (OpArray.h:57-62 @ v2.5.2).
    pub fn resize(&mut self, length: c_ulong, num_color_components: c_ulong, num_values: c_ulong) {
        self.length = length;
        self.num_color_components = num_color_components;
        self.data.resize(num_values as usize, T::default());
    }

    /// Changes the length, and resizes the values to `num_values`, what the op's array has for
    /// it, when the length changes.
    ///
    /// Port of `ArrayT::setLength` (OpArray.h:64-71 @ v2.5.2).
    pub fn set_length(&mut self, length: c_ulong, num_values: c_ulong) {
        if self.length != length {
            self.length = length;
            self.data.resize(num_values as usize, T::default());
        }
    }

    /// Changes the number of color components, and resizes the values to `num_values`, what
    /// the op's array has for it, when the number changes.
    ///
    /// Port of `ArrayT::setNumColorComponents` (OpArray.h:102-109 @ v2.5.2).
    pub fn set_num_color_components(&mut self, num_color_components: c_ulong, num_values: c_ulong) {
        if self.num_color_components != num_color_components {
            self.num_color_components = num_color_components;
            self.data.resize(num_values as usize, T::default());
        }
    }

    /// Always 3.
    ///
    /// Port of `ArrayT::getMaxColorComponents` (OpArray.h:139-142 @ v2.5.2).
    pub fn get_max_color_components(&self) -> c_ulong {
        3
    }

    /// Port of `ArrayT::getLength` (OpArray.h:83-86 @ v2.5.2).
    pub fn get_length(&self) -> c_ulong {
        self.length
    }

    /// Port of `ArrayT::getNumColorComponents` (OpArray.h:97-100 @ v2.5.2).
    pub fn get_num_color_components(&self) -> c_ulong {
        self.num_color_components
    }

    /// Port of `ArrayT::getValues() const` (OpArray.h:144-147 @ v2.5.2).
    pub fn get_values(&self) -> &Vec<T> {
        &self.data
    }

    /// Port of `ArrayT::getValues()` (OpArray.h:149-152 @ v2.5.2).
    pub fn get_values_mut(&mut self) -> &mut Vec<T> {
        &mut self.data
    }

    /// Checks that the array has values, and as many as its dimensions call for, `num_values`.
    ///
    /// Port of `ArrayT::validate` (OpArray.h:164-180 @ v2.5.2).
    pub fn validate(&self, num_values: c_ulong) -> Result<()> {
        if self.get_length() == 0 {
            return Err(Exception::new("Array content is empty."));
        }

        // getNumValues is based on the dimensions claimed in the file. Check that this matches
        // the number of values that were actually set. (The `unsigned long` converts to
        // `size_t` without loss on both platforms.)
        if self.data.len() != num_values as usize {
            return Err(Exception::new(format!(
                "Array contains: {} values, but {num_values} are expected.",
                self.data.len()
            )));
        }
        Ok(())
    }
}

impl<T: PartialEq> ArrayT<T> {
    /// Whether `other` is this array, or has its dimensions and values: a NaN value is unequal
    /// to itself, except in the same array.
    ///
    /// Port of `ArrayT::operator==` (OpArray.h:182-188 @ v2.5.2).
    pub fn equals(&self, other: &ArrayT<T>) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.length == other.length
            && self.num_color_components == other.num_color_components
            && self.data == other.data
    }
}

impl<T: SseFloat + PartialEq + From<u8>> ArrayT<T> {
    /// Multiplies every value by `scale`, unless it is 1, each value the first operand, as in
    /// the source.
    ///
    /// Port of `ArrayT::scale` (OpArray.h:190-200 @ v2.5.2).
    pub fn scale(&mut self, scale: T) {
        if scale != T::from(1) {
            for value in &mut self.data {
                *value = sse_mul(*value, scale);
            }
        }
    }
}

impl ArrayT<f32> {
    /// Sets the number of color components to 1, keeping the three values of each entry, when
    /// it is 3 and every entry has three equal values (entries of three NaNs count as equal;
    /// one or two NaNs don't).
    ///
    /// Port of `ArrayT::adjustColorComponentNumber` (OpArray.h:111-137 @ v2.5.2), for the
    /// `float` arrays of the LUTs.
    pub fn adjust_color_component_number(&mut self) {
        if self.num_color_components == 3 {
            let mut same_coeff = true;
            let mut idx = 0;
            while idx < self.length as usize && same_coeff {
                let d = &self.data;
                if d[idx * 3].is_nan() && d[idx * 3 + 1].is_nan() && d[idx * 3 + 2].is_nan() {
                    idx += 1;
                    continue;
                }
                if d[idx * 3] != d[idx * 3 + 1] || d[idx * 3] != d[idx * 3 + 2] {
                    same_coeff = false;
                    break;
                }
                idx += 1;
            }

            if same_coeff {
                self.num_color_components = 1; // But keep the three values...
            }
        }
    }
}

impl<T> Index<usize> for ArrayT<T> {
    type Output = T;

    /// Port of `ArrayT::operator[] const` (OpArray.h:154-157 @ v2.5.2).
    fn index(&self, index: usize) -> &T {
        &self.data[index]
    }
}

impl<T> IndexMut<usize> for ArrayT<T> {
    /// Port of `ArrayT::operator[]` (OpArray.h:159-162 @ v2.5.2).
    fn index_mut(&mut self, index: usize) -> &mut T {
        &mut self.data[index]
    }
}
