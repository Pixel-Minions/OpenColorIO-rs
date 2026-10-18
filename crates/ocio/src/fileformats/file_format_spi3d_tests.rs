// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the spi3d format: `tests/cpu/fileformats/FileFormatSpi3D_tests.cpp` @ v2.5.2.

use super::*;
use crate::fileformats::input_stream::OpenMode;
use crate::fileformats::test_utils::{get_test_files_dir, load_test_file};
use crate::test_env::capture_log;
use crate::transform::Transform;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(FileFormatSpi3D, format_info)` @ v2.5.2.
#[test]
fn format_info() {
    let tester = LocalFileFormat;
    let format_info_vec = tester.format_info();

    assert_eq!(1, format_info_vec.len());
    assert_eq!("spi3d", format_info_vec[0].name);
    assert_eq!("spi3d", format_info_vec[0].extension);
    assert_eq!(
        FormatCapabilities::READ | FormatCapabilities::BAKE,
        format_info_vec[0].capabilities
    );
}

/// Port of `LoadLutFile` (FileFormatSpi3D_tests.cpp:29-33 @ v2.5.2).
fn load_lut_file(file_name: &str) -> Result<LocalCachedFile> {
    load_test_file(file_name, OpenMode::Text, |stream, path| {
        LocalFileFormat.read_file(stream, path, Interpolation::Default)
    })
}

/// Port of `OCIO_ADD_TEST(FileFormatSpi3D, test)` @ v2.5.2.
#[test]
fn test() {
    let spi3d_file = "spi_ocio_srgb_test.spi3d";
    let cached_file = load_lut_file(spi3d_file).unwrap();

    let lut_array = cached_file.lut.get_array();
    assert_eq!(32, lut_array.get_length());
    assert_eq!(32 * 32 * 32 * 3, lut_array.get_num_values());
    let lut_array = lut_array.get_values();

    assert_eq!(0.040157f32, lut_array[0]);
    assert_eq!(0.038904f32, lut_array[1]);
    assert_eq!(0.028316f32, lut_array[2]);
    // 10 2 12
    assert_eq!(0.102161f32, lut_array[30948]);
    assert_eq!(0.032187f32, lut_array[30949]);
    assert_eq!(0.175453f32, lut_array[30950]);
}

/// Port of `ReadSpi3d` (FileFormatSpi3D_tests.cpp:58-67 @ v2.5.2).
fn read_spi3d(file_content: &str) -> Result<LocalCachedFile> {
    let mut is = InputStream::from_bytes(file_content);

    // Read file
    let tester = LocalFileFormat;
    let sample_name = b"Memory File";
    tester.read_file(&mut is, sample_name, Interpolation::Default)
}

/// Port of `OCIO_ADD_TEST(FileFormatSpi3D, read_failure)` @ v2.5.2.
#[test]
fn read_failure() {
    {
        // Validate stream can be read with no error.
        // Then stream will be altered to introduce errors.
        let sample_no_error = "SPILUT 1.0\n\
            3 3\n\
            2 2 2\n\
            0 0 0 0.0 0.0 0.0\n\
            0 0 1 0.0 0.0 0.9\n\
            0 1 0 0.0 0.7 0.0\n\
            0 1 1 0.0 0.8 0.8\n\
            1 0 0 0.7 0.0 0.1\n\
            1 0 1 0.7 0.6 0.1\n\
            1 1 0 0.6 0.7 0.1\n\
            1 1 1 0.6 0.7 0.7\n";

        assert!(read_spi3d(sample_no_error).is_ok());
    }
    {
        // Wrong first line
        let sample_error = "SPI LUT 1.0\n\
            3 3\n\
            2 2 2\n\
            0 0 0 0.0 0.0 0.0\n\
            0 0 1 0.0 0.0 0.9\n\
            0 1 0 0.0 0.7 0.0\n\
            0 1 1 0.0 0.8 0.8\n\
            1 0 0 0.7 0.0 0.1\n\
            1 0 1 0.7 0.6 0.1\n\
            1 1 0 0.6 0.7 0.1\n\
            1 1 1 0.6 0.7 0.7\n";

        check_throw_what(read_spi3d(sample_error), "Expected 'SPILUT'");
    }
    {
        // 3 line is not 3 ints
        let sample_error = "SPILUT 1.0\n\
            3 3\n\
            42\n\
            0 0 0 0.0 0.0 0.0\n\
            0 0 1 0.0 0.0 0.9\n\
            0 1 0 0.0 0.7 0.0\n\
            0 1 1 0.0 0.8 0.8\n\
            1 0 0 0.7 0.0 0.1\n\
            1 0 1 0.7 0.6 0.1\n\
            1 1 0 0.6 0.7 0.1\n\
            1 1 1 0.6 0.7 0.7\n";

        check_throw_what(read_spi3d(sample_error), "Error while reading LUT size");
    }
    {
        // index out of range
        let sample_error = "SPILUT 1.0\n\
            3 3\n\
            2 2 2\n\
            0 2 0 0.0 0.0 0.0\n\
            0 0 1 0.0 0.0 0.9\n\
            0 1 0 0.0 0.7 0.0\n\
            0 1 1 0.0 0.8 0.8\n\
            1 0 0 0.7 0.0 0.1\n\
            1 0 1 0.7 0.6 0.1\n\
            1 1 0 0.6 0.7 0.1\n\
            1 1 1 0.6 0.7 0.7\n";

        check_throw_what(read_spi3d(sample_error), "that falls outside of the cube");
    }
    {
        // Duplicated indices
        let sample_error = "SPILUT 1.0\n\
            3 3\n\
            2 2 2\n\
            0 0 0 0.0 0.0 0.0\n\
            0 0 1 0.0 0.0 0.9\n\
            0 0 1 0.0 0.0 0.9\n\
            0 1 0 0.0 0.7 0.0\n\
            0 1 1 0.0 0.8 0.8\n\
            1 0 1 0.7 0.6 0.1\n\
            1 1 0 0.6 0.7 0.1\n\
            1 1 1 0.6 0.7 0.7\n";

        check_throw_what(
            read_spi3d(sample_error),
            "A LUT entry is specified multiple times",
        );
    }
    {
        // Not enough entries
        let sample_error = "SPILUT 1.0\n\
            3 3\n\
            2 2 2\n\
            0 0 0 0.0 0.0 0.0\n\
            0 0 1 0.0 0.0 0.9\n\
            0 1 0 0.0 0.7 0.0\n\
            0 1 1 0.0 0.8 0.8\n\
            1 0 1 0.7 0.6 0.1\n\
            1 1 0 0.6 0.7 0.1\n\
            1 1 1 0.6 0.7 0.7\n";

        check_throw_what(read_spi3d(sample_error), "Not enough entries found");
    }
}

/// The interpolation of the group's only transform, a 3D LUT.
fn lut3d_interpolation(config: &Config, file_transform: &FileTransform) -> Interpolation {
    let proc = config
        .processor(&Transform::File(file_transform.clone()))
        .unwrap();
    let group = proc.create_group_transform().unwrap();
    assert_eq!(group.num_transforms(), 1);
    match group.transform(0).unwrap() {
        Transform::Lut3D(lut3d) => lut3d.interpolation(),
        other => panic!("not a 3D LUT: {other:?}"),
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatSpi3D, lut_interpolation_option)` @ v2.5.2.
#[test]
fn lut_interpolation_option() {
    // Create empty Config to use.
    let mut config = Config::new().unwrap();
    config.set_major_version(2).unwrap();

    let file_path = format!("{}/spi_ocio_srgb_test.spi3d", get_test_files_dir());

    let mut file_transform = FileTransform::new();
    file_transform.set_direction(TransformDirection::Forward);
    file_transform.set_src(&file_path);

    // Check that the specified value (INTERP_BEST) may be set.

    file_transform.set_interpolation(Interpolation::Best);
    assert_eq!(
        lut3d_interpolation(&config, &file_transform),
        Interpolation::Best
    );

    // Check that the specified value (INTERP_DEFAULT) may be set.

    file_transform.set_interpolation(Interpolation::Default);
    assert_eq!(
        lut3d_interpolation(&config, &file_transform),
        Interpolation::Default
    );

    // Additional FileTransforms that do not specify interpolation use "default" and not
    // "best", so the order they enter the cache does not matter.
    let mut file_transform = FileTransform::new();
    file_transform.set_src(&file_path);
    assert_eq!(
        lut3d_interpolation(&config, &file_transform),
        Interpolation::Default
    );

    // File transform specify an interpolation that is not supported by LUT3D, log warning and
    // use the default interpolation.

    file_transform.set_interpolation(Interpolation::Cubic);
    let (interp, log) = capture_log(|| lut3d_interpolation(&config, &file_transform));
    let output: Vec<u8> = log.concat();
    assert!(
        String::from_utf8_lossy(&output).contains("'cubic' is not allowed with the given file"),
        "{}",
        String::from_utf8_lossy(&output)
    );
    assert_eq!(interp, Interpolation::Default);
}

/// Port of `OCIO_ADD_TEST(FileFormatSpi3D, bake_3d)` @ v2.5.2.
#[test]
fn bake_3d() {
    let mut config = crate::Config::new().unwrap();
    {
        let mut cs = crate::ColorSpace::new();
        cs.set_name("input");
        cs.set_family("input");
        config.add_color_space(&cs).unwrap();
        config
            .set_role(ocio_ops::parse_utils::ROLE_REFERENCE, Some(cs.name()))
            .unwrap();
    }
    {
        let mut cs = crate::ColorSpace::new();
        cs.set_name("target");
        cs.set_family("target");

        // Set saturation to cause channel crosstalk, making a 3D LUT
        let mut transform1 = crate::CdlTransform::new();
        transform1.set_sat(f64::from(0.5f32));
        cs.set_transform(
            Some(&crate::Transform::from(transform1)),
            ocio_ops::open_color_types::ColorSpaceDirection::FromReference,
        )
        .unwrap();

        config.add_color_space(&cs).unwrap();
    }

    let bout = "SPILUT 1.0\n\
                3 3\n\
                2 2 2\n\
                0 0 0 0.000000 0.000000 0.000000\n\
                0 0 1 0.036100 0.036100 0.536100\n\
                0 1 0 0.357600 0.857600 0.357600\n\
                0 1 1 0.393700 0.893700 0.893700\n\
                1 0 0 0.606300 0.106300 0.106300\n\
                1 0 1 0.642400 0.142400 0.642400\n\
                1 1 0 0.963900 0.963900 0.463900\n\
                1 1 1 1.000000 1.000000 1.000000\n";

    let mut baker = Baker::new();
    baker.set_config(&config);
    baker.set_format("spi3d").unwrap();
    baker.set_input_space("input");
    baker.set_target_space("target");
    baker.set_cube_size(2);
    let mut output = Vec::new();
    baker.bake(&mut output).unwrap();

    //
    let osvec = ocio_ops::utils::string_utils::split_by_lines(&output);
    let resvec = ocio_ops::utils::string_utils::split_by_lines(bout.as_bytes());
    assert_eq!(osvec.len(), resvec.len());
    for i in 0..resvec.len() {
        assert_eq!(
            String::from_utf8_lossy(&osvec[i]),
            String::from_utf8_lossy(&resvec[i])
        );
    }
}
