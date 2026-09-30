// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! 3D LUT op data: the parts of `Lut3DOpData` (src/OpenColorIO/ops/lut3d/Lut3DOpData.cpp and
//! Lut3DOpData.h @ v2.5.2) that the forward CPU renderers use.
//!
//! Not ported yet: composition, inversion, validation, cache IDs, metadata and bit depths.

/// The largest 3D LUT grid size. Port of `Max3DLUTLength` (src/OpenColorIO/LutLimits.h:15 @ v2.5.2).
pub const MAX_3D_LUT_LENGTH: u32 = 129;

/// Interpolation algorithms. Port of `enum Interpolation`
/// (include/OpenColorIO/OpenColorTypes.h:410-420 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Interpolation {
    /// `INTERP_UNKNOWN`
    Unknown = 0,
    /// `INTERP_NEAREST`: nearest neighbor.
    Nearest = 1,
    /// `INTERP_LINEAR`: linear interpolation (trilinear for Lut3D).
    Linear = 2,
    /// `INTERP_TETRAHEDRAL`: tetrahedral interpolation (Lut3D only).
    Tetrahedral = 3,
    /// `INTERP_CUBIC`: cubic interpolation (not supported).
    Cubic = 4,
    /// `INTERP_DEFAULT`: the default interpolation type.
    Default = 254,
    /// `INTERP_BEST`: the 'best' suitable interpolation type.
    Best = 255,
}

/// The interpolation a renderer implements. Port of `Lut3DOpData::GetConcreteInterpolation`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpData.cpp:289-308 @ v2.5.2). In OCIO v2, `Nearest` is
/// trilinear; `Unknown` is invalid and makes validation fail.
pub fn get_concrete_interpolation(interp: Interpolation) -> Interpolation {
    match interp {
        Interpolation::Best | Interpolation::Tetrahedral => Interpolation::Tetrahedral,
        Interpolation::Default
        | Interpolation::Linear
        | Interpolation::Cubic
        | Interpolation::Nearest
        | Interpolation::Unknown => Interpolation::Linear,
    }
}

/// The values of a 3D LUT: `length^3` RGB triplets with the blue index changing fastest, then
/// green, then red. Port of `Lut3DOpData::Lut3DArray`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpData.cpp:155-234 @ v2.5.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Lut3DArray {
    length: u32,
    values: Vec<f32>,
}

impl Lut3DArray {
    /// Three color components, as `getMaxColorComponents` returns for a 3D LUT.
    const CHANNELS: usize = 3;

    /// An identity LUT. Port of `Lut3DArray::Lut3DArray`, `resize` and `fill`
    /// (src/OpenColorIO/ops/lut3d/Lut3DOpData.cpp:155-204 @ v2.5.2).
    pub fn identity(length: u32) -> Result<Lut3DArray, String> {
        check_length(length)?;
        let mut array = Lut3DArray {
            length,
            values: vec![0.0; entries(length) * Self::CHANNELS],
        };
        array.fill();
        Ok(array)
    }

    /// A LUT from `length^3 * 3` values, blue fastest (the order of `Lut3DTransform::setValue`,
    /// src/OpenColorIO/transforms/Lut3DTransform.cpp:139-143 @ v2.5.2). A wrong value count
    /// fails with the message of `ArrayT::validate`
    /// (src/OpenColorIO/ops/OpArray.h:164-180 @ v2.5.2).
    pub fn from_values(length: u32, values: Vec<f32>) -> Result<Lut3DArray, String> {
        check_length(length)?;
        let expected = entries(length) * Self::CHANNELS;
        if values.len() != expected {
            return Err(format!(
                "Array contains: {} values, but {expected} are expected.",
                values.len()
            ));
        }
        Ok(Lut3DArray { length, values })
    }

    /// Port of `Lut3DArray::fill` (src/OpenColorIO/ops/lut3d/Lut3DOpData.cpp:174-192 @ v2.5.2).
    fn fill(&mut self) {
        let length = i64::from(self.length);
        let step_value = 1.0f32 / (length as f32 - 1.0f32);
        let max_entries = length * length * length;
        for idx in 0..max_entries {
            let at = Self::CHANNELS * idx as usize;
            self.values[at] = ((idx / length / length) % length) as f32 * step_value;
            self.values[at + 1] = ((idx / length) % length) as f32 * step_value;
            self.values[at + 2] = (idx % length) as f32 * step_value;
        }
    }

    /// The grid size. Port of `Array::getLength`.
    pub fn length(&self) -> u32 {
        self.length
    }

    /// The values, blue fastest.
    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// The values, blue fastest, for editing.
    pub fn values_mut(&mut self) -> &mut [f32] {
        &mut self.values
    }
}

fn entries(length: u32) -> usize {
    let n = length as usize;
    n * n * n
}

/// Port of the size check of `Lut3DArray::resize`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpData.cpp:194-204 @ v2.5.2).
fn check_length(length: u32) -> Result<(), String> {
    if length > MAX_3D_LUT_LENGTH {
        return Err(format!(
            "LUT 3D: Grid size '{length}' must not be greater than '{MAX_3D_LUT_LENGTH}'."
        ));
    }
    Ok(())
}

/// A forward 3D LUT. Port of the forward parts of `Lut3DOpData`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpData.cpp:251-308 @ v2.5.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Lut3DOpData {
    interpolation: Interpolation,
    array: Lut3DArray,
}

impl Lut3DOpData {
    /// An identity LUT. Port of `Lut3DOpData(Interpolation, unsigned long gridSize)`
    /// (src/OpenColorIO/ops/lut3d/Lut3DOpData.cpp:267-273 @ v2.5.2).
    pub fn new(interpolation: Interpolation, grid_size: u32) -> Result<Lut3DOpData, String> {
        Ok(Lut3DOpData {
            interpolation,
            array: Lut3DArray::identity(grid_size)?,
        })
    }

    /// A LUT with the given values.
    pub fn from_array(interpolation: Interpolation, array: Lut3DArray) -> Lut3DOpData {
        Lut3DOpData {
            interpolation,
            array,
        }
    }

    /// Port of `Lut3DOpData::getInterpolation`.
    pub fn interpolation(&self) -> Interpolation {
        self.interpolation
    }

    /// Port of `Lut3DOpData::setInterpolation`
    /// (src/OpenColorIO/ops/lut3d/Lut3DOpData.cpp:279-282 @ v2.5.2).
    pub fn set_interpolation(&mut self, interpolation: Interpolation) {
        self.interpolation = interpolation;
    }

    /// Port of `Lut3DOpData::getConcreteInterpolation`
    /// (src/OpenColorIO/ops/lut3d/Lut3DOpData.cpp:284-287 @ v2.5.2).
    pub fn concrete_interpolation(&self) -> Interpolation {
        get_concrete_interpolation(self.interpolation)
    }

    /// Port of `Lut3DOpData::getArray`.
    pub fn array(&self) -> &Lut3DArray {
        &self.array
    }

    /// Port of `Lut3DOpData::getArray` (non-const).
    pub fn array_mut(&mut self) -> &mut Lut3DArray {
        &mut self.array
    }
}
