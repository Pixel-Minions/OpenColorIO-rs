// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The file formats: what reads, bakes and writes LUT files (src/OpenColorIO/fileformats).

pub mod cdl;
pub mod file_format_3dl;
pub mod file_format_cc;
pub mod file_format_ccc;
pub mod file_format_cdl;
pub mod file_format_discreet1dl;
pub mod file_format_icc;
pub mod file_format_iridas_cube;
pub mod file_format_iridas_itx;
pub mod file_format_resolve_cube;
pub mod file_format_spi1d;
pub mod file_format_spi3d;
pub mod file_format_spimtx;
pub mod file_format_utils;
pub mod icc;
pub mod input_stream;

#[cfg(test)]
pub(crate) mod test_utils;
