// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the file transform: `tests/cpu/transforms/FileTransform_tests.cpp` @ v2.5.2. Its
//! other tests read files (Phase 4) or resolve context variables (WP 3.2a). The text and the
//! validation are compared with the wheel's in `tests/config_transforms_oracle.rs`.

use super::*;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(FileTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut ft = FileTransform::new();
    assert_eq!(ft.direction(), TransformDirection::Forward);
    ft.set_direction(TransformDirection::Inverse);
    assert_eq!(ft.direction(), TransformDirection::Inverse);

    assert_eq!(ft.src(), b"");
    let src: &[u8] = b"source";
    ft.set_src(src);
    assert_eq!(src, ft.src());

    assert_eq!(ft.ccc_id(), b"");
    let cccid: &[u8] = b"cccid";
    ft.set_ccc_id(cccid);
    assert_eq!(cccid, ft.ccc_id());

    assert_eq!(ft.cdl_style(), CdlStyle::NoClamp);
    ft.set_cdl_style(CdlStyle::Asc);
    assert_eq!(ft.cdl_style(), CdlStyle::Asc);

    assert_eq!(ft.interpolation(), Interpolation::Default);
    ft.set_interpolation(Interpolation::Linear);
    assert_eq!(ft.interpolation(), Interpolation::Linear);
}

/// Port of `OCIO_ADD_TEST(FileTransform, validate)` @ v2.5.2.
#[test]
fn validate() {
    let mut tr = FileTransform::new();

    tr.set_src("lut3d_17x17x17_32f_12i.clf");
    assert!(tr.validate().is_ok());

    tr.set_src("");
    check_throw_what(tr.validate(), "FileTransform: empty file path");
}

/// The setters keep a string up to its first NUL, as from a C string, and a copy keeps every
/// field (`Impl::operator=`, FileTransform.cpp:51, 61-66).
#[test]
fn strings_stop_at_nul_and_copies_keep_every_field() {
    let mut ft = FileTransform::new();
    ft.set_src(b"lut.cube\0.3dl");
    ft.set_ccc_id(b"\0id");
    assert_eq!(ft.src(), b"lut.cube");
    assert_eq!(ft.ccc_id(), b"");

    ft.set_ccc_id("shot_010");
    ft.set_cdl_style(CdlStyle::Asc);
    ft.set_interpolation(Interpolation::Unknown);
    ft.set_direction(TransformDirection::Inverse);
    let copy = ft.clone();
    assert_eq!(copy.src(), b"lut.cube");
    assert_eq!(copy.ccc_id(), b"shot_010");
    assert_eq!(copy.cdl_style(), CdlStyle::Asc);
    assert_eq!(copy.interpolation(), Interpolation::Unknown);
    assert_eq!(copy.direction(), TransformDirection::Inverse);
    // An unknown interpolation is valid (version 1 configs use it).
    assert!(copy.validate().is_ok());
}

// The format registry (FileTransform_tests.cpp:161-283 @ v2.5.2).

use crate::transforms::file_format::{
    FILEFORMAT_CLF, FILEFORMAT_CTF, FormatCapabilities, FormatRegistry,
};
use ocio_ops::platform::strcasecmp;

/// Whether a format of `extension` has the name `format_name` (its first info's).
///
/// Port of `FormatNameFoundByExtension` (FileTransform_tests.cpp:163-182 @ v2.5.2).
fn format_name_found_by_extension(extension: &str, format_name: &str) -> bool {
    FormatRegistry::instance()
        .file_formats_for_extension(extension)
        .iter()
        .any(|f| f.name() == format_name)
}

/// Whether the format named `format_name` declares `extension`.
///
/// Port of `FormatExtensionFoundByName` (FileTransform_tests.cpp:184-203 @ v2.5.2).
fn format_extension_found_by_name(extension: &str, format_name: &str) -> bool {
    FormatRegistry::instance()
        .file_format_by_name(format_name)
        .is_some_and(|f| f.format_info().iter().any(|i| i.extension == extension))
}

/// Port of `OCIO_ADD_TEST(FileTransform, all_formats)` @ v2.5.2.
#[test]
fn all_formats() {
    let format_registry = FormatRegistry::instance();
    assert_eq!(19, format_registry.num_raw_formats());
    assert_eq!(24, format_registry.num_formats(FormatCapabilities::READ));
    assert_eq!(12, format_registry.num_formats(FormatCapabilities::BAKE));
    assert_eq!(5, format_registry.num_formats(FormatCapabilities::WRITE));

    assert!(format_name_found_by_extension("3dl", "flame"));
    assert!(format_name_found_by_extension("cc", "ColorCorrection"));
    assert!(format_name_found_by_extension(
        "ccc",
        "ColorCorrectionCollection"
    ));
    assert!(format_name_found_by_extension("cdl", "ColorDecisionList"));
    assert!(format_name_found_by_extension("clf", FILEFORMAT_CLF));
    assert!(format_name_found_by_extension("csp", "cinespace"));
    assert!(format_name_found_by_extension("cub", "truelight"));
    assert!(format_name_found_by_extension("cube", "iridas_cube"));
    assert!(format_name_found_by_extension("cube", "resolve_cube"));
    assert!(format_name_found_by_extension("itx", "iridas_itx"));
    assert!(format_name_found_by_extension(
        "icc",
        "International Color Consortium profile"
    ));
    assert!(format_name_found_by_extension("look", "iridas_look"));
    assert!(format_name_found_by_extension("lut", "houdini"));
    assert!(format_name_found_by_extension("lut", "Discreet 1D LUT"));
    assert!(format_name_found_by_extension("mga", "pandora_mga"));
    assert!(format_name_found_by_extension("spi1d", "spi1d"));
    assert!(format_name_found_by_extension("spi3d", "spi3d"));
    assert!(format_name_found_by_extension("spimtx", "spimtx"));
    assert!(format_name_found_by_extension("vf", "nukevf"));
    // When a FileFormat handles 2 "formats" it declares both names
    // but only exposes one name using the getName() function.
    assert!(!format_name_found_by_extension("3dl", "lustre"));
    assert!(!format_name_found_by_extension("m3d", "pandora_m3d"));
    assert!(!format_name_found_by_extension(
        "icm",
        "Image Color Matching"
    ));
    assert!(!format_name_found_by_extension("ctf", FILEFORMAT_CTF));

    assert!(format_extension_found_by_name("3dl", "flame"));
    assert!(format_extension_found_by_name("3dl", "lustre"));
    assert!(format_extension_found_by_name("cc", "ColorCorrection"));
    assert!(format_extension_found_by_name(
        "ccc",
        "ColorCorrectionCollection"
    ));
    assert!(format_extension_found_by_name("cdl", "ColorDecisionList"));
    assert!(format_extension_found_by_name("clf", FILEFORMAT_CLF));
    assert!(format_extension_found_by_name("ctf", FILEFORMAT_CTF));
    assert!(format_extension_found_by_name("csp", "cinespace"));
    assert!(format_extension_found_by_name("cub", "truelight"));
    assert!(format_extension_found_by_name("cube", "iridas_cube"));
    assert!(format_extension_found_by_name("cube", "resolve_cube"));
    assert!(format_extension_found_by_name("itx", "iridas_itx"));
    assert!(format_extension_found_by_name(
        "icc",
        "International Color Consortium profile"
    ));
    assert!(format_extension_found_by_name(
        "icm",
        "International Color Consortium profile"
    ));
    assert!(format_extension_found_by_name("look", "iridas_look"));
    assert!(format_extension_found_by_name("lut", "houdini"));
    assert!(format_extension_found_by_name("lut", "Discreet 1D LUT"));
    assert!(format_extension_found_by_name("m3d", "pandora_m3d"));
    assert!(format_extension_found_by_name("mga", "pandora_mga"));
    assert!(format_extension_found_by_name("spi1d", "spi1d"));
    assert!(format_extension_found_by_name("spi3d", "spi3d"));
    assert!(format_extension_found_by_name("spimtx", "spimtx"));
    assert!(format_extension_found_by_name("vf", "nukevf"));
}

/// Port of `ValidateFormatByIndex` (FileTransform_tests.cpp:265-282 @ v2.5.2).
fn validate_format_by_index(reg: &FormatRegistry, cap: FormatCapabilities) {
    let num_format = reg.num_formats(cap);

    // Check out of bounds access
    let empty = |s: &str| strcasecmp(s.as_bytes(), b"").is_eq();
    assert!(empty(reg.format_name_by_index(cap, -1)));
    assert!(empty(reg.format_extension_by_index(cap, -1)));
    assert!(empty(reg.format_name_by_index(cap, num_format)));
    assert!(empty(reg.format_extension_by_index(cap, num_format)));

    // Check valid access
    for i in 0..num_format {
        assert!(!empty(reg.format_name_by_index(cap, i)));
        assert!(!empty(reg.format_extension_by_index(cap, i)));
    }
}

/// Port of `OCIO_ADD_TEST(FileTransform, format_by_index)` @ v2.5.2.
#[test]
fn format_by_index() {
    let format_registry = FormatRegistry::instance();
    validate_format_by_index(format_registry, FormatCapabilities::WRITE);
    validate_format_by_index(format_registry, FormatCapabilities::BAKE);
    validate_format_by_index(format_registry, FormatCapabilities::READ);
}

/// Port of `OCIO_ADD_TEST(FileTransform, is_format_extension_supported)` @ v2.5.2.
#[test]
fn is_format_extension_supported() {
    let format_registry = FormatRegistry::instance();
    assert!(!format_registry.is_format_extension_supported("foo"));
    assert!(!format_registry.is_format_extension_supported("bar"));
    assert!(!format_registry.is_format_extension_supported("."));
    assert!(format_registry.is_format_extension_supported("cdl"));
    assert!(format_registry.is_format_extension_supported(".cdl"));
    assert!(format_registry.is_format_extension_supported("Cdl"));
    assert!(format_registry.is_format_extension_supported(".Cdl"));
    assert!(format_registry.is_format_extension_supported("3dl"));
    assert!(format_registry.is_format_extension_supported(".3dl"));
}
