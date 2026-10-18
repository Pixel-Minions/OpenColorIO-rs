// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Ports of `tests/cpu/OCIOZArchive_tests.cpp` @ v2.5.2.

use ocio_ops::exception::ExceptionKind;
use ocio_ops::utils::pystring::os_path;

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::test_utils::get_test_files_dir;
use crate::test_env::EnvGuard;
use crate::transform::Transform;
use crate::transforms::file_transform::FileTransform;

/// `pystring::os::path::normpath(pystring::os::path::join(paths))` of the test files'
/// directory and `parts`, as upstream's tests build an archive's path.
pub(crate) fn archive_path(parts: &[&str]) -> Vec<u8> {
    let mut paths = vec![get_test_files_dir().into_bytes()];
    paths.extend(parts.iter().map(|p| p.as_bytes().to_vec()));
    let joined = if cfg!(windows) {
        os_path::join_nt_all(&paths)
    } else {
        os_path::join_posix_all(&paths)
    };
    os_path::normpath(&joined)
}

/// The first value of the matrix of the first transform of `processor`'s group transform.
fn first_matrix_value(processor: &crate::Processor) -> f64 {
    let group = processor.create_group_transform().unwrap();
    let Transform::Matrix(mtx) = group.transform(0).unwrap() else {
        panic!("OCIO_REQUIRE_ASSERT(mtx) failed");
    };
    mtx.matrix()[0]
}

/// Port of `OCIO_ADD_TEST(OCIOZArchive, context_test_for_search_paths_and_filetransform_source_path)` @ v2.5.2.
#[test]
fn context_test_for_search_paths_and_filetransform_source_path() {
    let _env = EnvGuard::new();

    let archive = |name: &str| archive_path(&["configs", "context_test1", name]);

    let archive_path_windows = archive("context_test1_windows.ocioz");
    let cfg_windows_archive: Config =
        (*Config::create_from_file(&archive_path_windows).unwrap()).clone();
    cfg_windows_archive.validate().unwrap();

    let archive_path_linux = archive("context_test1_linux.ocioz");
    let cfg_linux_archive: Config =
        (*Config::create_from_file(&archive_path_linux).unwrap()).clone();
    cfg_linux_archive.validate().unwrap();

    //  OCIO will pick up context vars from the environment that runs the test,
    //  so set these explicitly, even though the config has default values.

    let context_of = |cfg: &Config| -> Context {
        let mut ctx = (*cfg.current_context().get()).clone();
        ctx.set_string_var("SHOT", Some(b"none"));
        ctx.set_string_var("LUT_PATH", Some(b"none"));
        ctx.set_string_var("CAMERA", Some(b"none"));
        ctx.set_string_var("CCCID", Some(b"none"));
        ctx
    };

    let test_paths = |cfg: &Config, mut ctx: Context| {
        {
            // This is independent of the context.
            let processor = cfg.processor_between_with_context(&ctx, "shot1_lut1_cs", "reference");
            assert_eq!(first_matrix_value(&processor.unwrap()), 10.);
        }

        {
            // This is independent of the context.
            let processor = cfg.processor_between_with_context(&ctx, "shot2_lut1_cs", "reference");
            assert_eq!(first_matrix_value(&processor.unwrap()), 20.);
        }

        {
            // This is independent of the context.
            let processor = cfg.processor_between_with_context(&ctx, "shot2_lut2_cs", "reference");
            assert_eq!(first_matrix_value(&processor.unwrap()), 2.);
        }

        {
            // This is independent of the context but the file is in a second level
            // sub-directory.
            let processor = cfg.processor_between_with_context(&ctx, "lut3_cs", "reference");
            assert_eq!(first_matrix_value(&processor.unwrap()), 3.);
        }

        {
            // This uses a context for the FileTransform src but is independent of the
            // search_path.
            ctx.set_string_var("LUT_PATH", Some(b"shot3/lut1.clf"));
            let processor = cfg.processor_between_with_context(&ctx, "lut_path_cs", "reference");
            assert_eq!(first_matrix_value(&processor.unwrap()), 30.);
        }

        {
            // The FileTransform src is ambiguous and the context configures the search_path.
            ctx.set_string_var("SHOT", Some(b".")); // (use the working directory)
            let processor = cfg.processor_between_with_context(&ctx, "plain_lut1_cs", "reference");
            assert_eq!(first_matrix_value(&processor.unwrap()), 5.);
        }

        {
            // The FileTransform src is ambiguous and the context configures the search_path.
            ctx.set_string_var("SHOT", Some(b"shot2"));
            let processor = cfg.processor_between_with_context(&ctx, "plain_lut1_cs", "reference");
            assert_eq!(first_matrix_value(&processor.unwrap()), 20.);
        }

        {
            // The FileTransform src is ambiguous and the context configures the search_path.
            ctx.set_string_var("SHOT", Some(b"no_shot")); // (path doesn't exist)
            let processor = cfg.processor_between_with_context(&ctx, "plain_lut1_cs", "reference");
            assert_eq!(first_matrix_value(&processor.unwrap()), 10.);
        }

        {
            // This should be in the archive but is not on the search path at all without the
            // context.
            ctx.set_string_var("SHOT", Some(b"no_shot")); // (path doesn't exist)
            let err = cfg
                .processor_between_with_context(&ctx, "lut4_cs", "reference")
                .unwrap_err();
            assert_eq!(err.kind(), ExceptionKind::MissingFile, "{err}");
        }

        {
            // This should be in the archive but is not on the search path at all without the
            // context.
            ctx.set_string_var("SHOT", Some(b"shot4"));
            let processor = cfg.processor_between_with_context(&ctx, "lut4_cs", "reference");
            assert_eq!(first_matrix_value(&processor.unwrap()), 4.);
        }

        {
            // File transform source is an absolute path, not in the archive.
            let file_path = get_test_files_dir() + "/matrix_example4x4.ctf";
            let mut transform = FileTransform::new();
            transform.set_src(file_path.as_bytes());
            let processor = cfg.processor(&Transform::from(transform));
            assert_eq!(first_matrix_value(&processor.unwrap()), 3.24);
        }

        {
            // File transform source is an abs path but doesn't exist anywhere.
            let file_path = get_test_files_dir() + "/missing.ctf";
            let mut transform = FileTransform::new();
            transform.set_src(file_path.as_bytes());
            assert!(cfg.processor(&Transform::from(transform)).is_err());
        }
    };

    test_paths(&cfg_windows_archive, context_of(&cfg_windows_archive));
    test_paths(&cfg_linux_archive, context_of(&cfg_linux_archive));
}

/// Port of `OCIO_ADD_TEST(OCIOZArchive, archive_config_and_compare_to_original)` @ v2.5.2.
#[test]
fn archive_config_and_compare_to_original() {
    /*
     * This test is doing the following :
     *
     * 1 - Create a config object from tests/data/files/configs/context_test1/config.ocio.
     * 2 - Archive the config in step 1 and save it in a temporary folder.
     * 3 - Create a config object using the archived config from step 2.
     * 4 - Compare different elements between the two configs.
     *
     * Testing CreateFromFile and archive method on a successful path.
     */
    let config_path = archive_path(&["configs", "context_test1", "config.ocio"]);

    {
        let env = EnvGuard::new();
        env.set(&[("OCIO", std::str::from_utf8(&config_path).unwrap())]);

        // 1 - Create a config from a OCIO file.
        let config_from_ocio_file = Config::create_from_env().unwrap();
        config_from_ocio_file.validate().unwrap();

        // 2 - Archive the config of step 1.
        let mut ostring_stream = Vec::new();
        config_from_ocio_file.archive(&mut ostring_stream).unwrap();

        // 3 - Verify that the binary data starts with "PK".
        assert_eq!(b'P', ostring_stream[0]);
        assert_eq!(b'K', ostring_stream[1]);

        // 4 - Save the archive into a temporary file (FileCreationGuard).
        let filename = ocio_ops::platform::create_temp_filename(b"");
        let path = String::from_utf8(filename.clone()).unwrap();
        std::fs::write(&path, &ostring_stream).unwrap();

        // 5 - Create a config from the archived config of step 4.
        let config_from_archive = Config::create_from_file(&filename).unwrap();
        config_from_archive.validate().unwrap();

        // 6 - Compare config cacheID - configFromOcioFile vs configFromArchive.
        assert_eq!(
            config_from_ocio_file.cache_id_with_context(None).unwrap(),
            config_from_archive.cache_id_with_context(None).unwrap()
        );

        // 7 - Compare a processor cacheID - configFromOcioFile to configFromArchive.
        let proc_config_from_ocio_file = config_from_ocio_file
            .processor_between("plain_lut1_cs", "shot1_lut1_cs")
            .unwrap();
        let proc_config_from_archive = config_from_archive
            .processor_between("plain_lut1_cs", "shot1_lut1_cs")
            .unwrap();
        assert_eq!(
            proc_config_from_ocio_file.cache_id().unwrap(),
            proc_config_from_archive.cache_id().unwrap()
        );

        // 8 - Compare serialization - configFromOcioFile vs configFromArchive.
        assert_eq!(
            config_from_ocio_file.serialize().unwrap(),
            config_from_archive.serialize().unwrap()
        );

        // Even if not strictly required on most OSes, perform the cleanup.
        let _ = std::fs::remove_file(&path);
    }
}

/// Port of `OCIO_ADD_TEST(OCIOZArchive, extract_config_and_compare_to_original)` @ v2.5.2.
#[test]
fn extract_config_and_compare_to_original() {
    /*
     * This test is doing the following :
     *
     * 1 - Create a config object from context_test1_windows.ocioz.
     * 2 - Extract the config context_test1_windows.ocioz.
     * 3 - Create a config object using the extracted config in step 1.
     * 4 - Compare different elements between the two configs.
     *
     * Testing CreateFromFile and ExtractOCIOZArchive on a successful path.
     */
    let _env = EnvGuard::new();
    let archive_path = archive_path(&["configs", "context_test1", "context_test1_windows.ocioz"]);

    {
        // 1 - Create config from OCIOZ archive.
        let config_from_archive = Config::create_from_file(&archive_path).unwrap();
        config_from_archive.validate().unwrap();

        // 2 - Extract the OCIOZ archive to temporary directory (DirectoryCreationGuard).
        let directory = std::env::temp_dir().join(format!(
            "OpenColorIO_Test_context_test1_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir(&directory).unwrap();
        let directory_path = directory.to_str().unwrap().to_owned();

        crate::ocioz_archive::extract_ocioz_archive(&archive_path, directory_path.as_bytes())
            .unwrap();

        // 3 - Create config from extracted OCIOZ archive.
        let config_from_extracted_archive =
            Config::create_from_file(os_path::join(directory_path.as_bytes(), b"config.ocio"))
                .unwrap();
        config_from_extracted_archive.validate().unwrap();

        // 4 - Compare config cacheID - configFromArchive vs configFromExtractedArchive.
        assert_eq!(
            config_from_archive.cache_id_with_context(None).unwrap(),
            config_from_extracted_archive
                .cache_id_with_context(None)
                .unwrap()
        );

        // 5 - Compare a processor cacheID - configFromArchive to configFromExtractedArchive.
        let proc_config_from_archive = config_from_archive
            .processor_between("plain_lut1_cs", "shot1_lut1_cs")
            .unwrap();
        let proc_config_from_extracted_archive = config_from_extracted_archive
            .processor_between("plain_lut1_cs", "shot1_lut1_cs")
            .unwrap();
        assert_eq!(
            proc_config_from_archive.cache_id().unwrap(),
            proc_config_from_extracted_archive.cache_id().unwrap()
        );

        // 6 - Compare serialization - configFromArchive vs configFromExtractedArchive.
        assert_eq!(
            config_from_archive.serialize().unwrap(),
            config_from_extracted_archive.serialize().unwrap()
        );

        let _ = std::fs::remove_dir_all(&directory);
    }
}
