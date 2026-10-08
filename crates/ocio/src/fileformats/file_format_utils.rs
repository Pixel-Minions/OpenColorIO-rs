// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! What the readers share when they build their ops: a port of
//! `src/OpenColorIO/fileformats/FileFormatUtils.cpp` @ v2.5.2. `HandleLUT3D` comes with the
//! 3D LUT readers.

use ocio_ops::logging::log_warning;
use ocio_ops::open_color_types::interpolation_to_string;
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;

use crate::transforms::file_transform::FileTransform;

/// A LUT whose interpolation a file transform may set: the statics `HandleLUT` calls.
trait HandledLut: Clone {
    fn is_valid_interpolation(interp: Interpolation) -> bool;
    fn concrete_interpolation(interp: Interpolation) -> Interpolation;
    fn interpolation(&self) -> Interpolation;
    fn set_interpolation(&mut self, interp: Interpolation);
}

impl HandledLut for Lut1DOpData {
    fn is_valid_interpolation(interp: Interpolation) -> bool {
        Lut1DOpData::is_valid_interpolation(interp)
    }

    /// Port of `Lut1DOpData::GetConcreteInterpolation` (src/OpenColorIO/ops/lut1d/
    /// Lut1DOpData.cpp:221-231 @ v2.5.2): linear, whatever the interpolation.
    fn concrete_interpolation(_interp: Interpolation) -> Interpolation {
        Interpolation::Linear
    }

    fn interpolation(&self) -> Interpolation {
        self.get_interpolation()
    }

    fn set_interpolation(&mut self, interp: Interpolation) {
        Lut1DOpData::set_interpolation(self, interp);
    }
}

/// The file's LUT, or a copy of it with the file transform's interpolation when its concrete
/// interpolation differs; `file_interp_used` is set when that interpolation is valid for the
/// LUT (an invalid one counts as the default).
///
/// Port of `HandleLUT` (FileFormatUtils.cpp:11-41 @ v2.5.2).
fn handle_lut<L: HandledLut>(
    file_lut: Option<&L>,
    mut file_interp: Interpolation,
    file_interp_used: &mut bool,
) -> Option<L> {
    let file_lut = file_lut?;
    let valid = L::is_valid_interpolation(file_interp);
    *file_interp_used |= valid;

    file_interp = if valid {
        file_interp
    } else {
        Interpolation::Default
    };

    let lut_interp = file_lut.interpolation();
    if L::concrete_interpolation(lut_interp) == L::concrete_interpolation(file_interp) {
        // Same concrete interpolation, no clone needed.
        Some(file_lut.clone())
    } else {
        // As the FileTransform interpolation (from the config file) is different from the
        // cached interpolation, clone the LUT and use that new interpolation.
        let mut lut = file_lut.clone();
        lut.set_interpolation(file_interp);
        Some(lut)
    }
}

/// Port of `HandleLUT1D` (FileFormatUtils.cpp:43-48 @ v2.5.2).
pub(crate) fn handle_lut1d(
    file_lut1d: Option<&Lut1DOpData>,
    file_interp: Interpolation,
    file_interp_used: &mut bool,
) -> Option<Lut1DOpData> {
    handle_lut(file_lut1d, file_interp, file_interp_used)
}

/// Logs that the file transform's interpolation isn't valid for its file.
///
/// Port of `LogWarningInterpolationNotUsed` (FileFormatUtils.cpp:57-65 @ v2.5.2).
pub(crate) fn log_warning_interpolation_not_used(
    interp: Interpolation,
    file_transform: &FileTransform,
) {
    let oss = [
        b"Interpolation specified by FileTransform '".as_slice(),
        interpolation_to_string(interp).as_bytes(),
        b"' is not allowed with the given file: '",
        file_transform.src(),
        b"'.",
    ]
    .concat();
    log_warning(oss);
}
