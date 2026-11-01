// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ociobakelut --format icc` against the wheel's: stdout, stderr, exit code and the profile,
//! byte for byte but for its creation time ([`Case::icc`]).
//!
//! Where the wheel's tool leaves its memory the port stops with an error instead
//! (docs/improvements.md, U-650 to U-653, and [`Case::stops`]).

mod common;
mod icc_profiles;

use std::time::Duration;

use common::{Case, ICC_CREATION_TIME, Stop, sweep, test_file, test_file_bytes, wheel_outcomes};
use icc_profiles::{
    Fill, LutV4, Profile, Rng, V2, V4, clutf, corrupt, cvst, formula, gamma, generated, lut_v4,
    matf, mft2, mpet, para, sampled, segmented, table,
};
use ocio_testkit::paths;
use ocio_tools::ocioicc::Stop as IccStop;

const TOOL: &str = "ociobakelut";
const PROGRAM: &str = env!("CARGO_BIN_EXE_ociobakelut");

const CONFIG: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: .
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]
description: A config for the sweep.

roles:
  default: raw
  scene_linear: lin
  compositing_log: log

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: Film, colorspace: gamma, looks: warm}

looks:
  - !<Look>
    name: warm
    process_space: lin
    transform: !<CDLTransform> {slope: [1.1, 1, 0.9]}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true

  - !<ColorSpace>
    name: lin

  - !<ColorSpace>
    name: log
    from_scene_reference: !<LogTransform> {base: 2}

  - !<ColorSpace>
    name: gamma
    from_scene_reference: !<ExponentTransform> {value: 2.2}

  - !<ColorSpace>
    name: nan_maker
    from_scene_reference: !<ExponentTransform> {value: 0.5, style: mirror}

  - !<ColorSpace>
    name: matrix
    from_scene_reference: !<MatrixTransform> {matrix: [2, 0, 0, 0, 0, -1, 0, 0, 0, 0, 0.5, 0, 0, 0, 0, 1], offset: [-0.5, 1, 0.25, 0]}
"#;

/// A LUT without a config, for the cases about something else.
fn lut() -> String {
    test_file("lut1d_5.spi1d")
}

/// Baking a LUT to `out.icc`, with `extra` first.
fn baking(label: impl Into<String>, extra: &[&str]) -> Case {
    let lut = lut();
    let mut argv = extra.to_vec();
    argv.extend(["--lut", lut.as_str(), "--format", "icc", "out.icc"]);
    Case::new(label, &argv).icc()
}

/// The tool's crash on a transform that couldn't be created is a deliberate error here, and so
/// is its read past a float CLUT, which a profile can get by chance: U-652 and U-653. Whether a
/// profile gets to the first is the wheel's to tell.
fn profile_stops() -> Stop {
    Stop::Profile {
        dies: IccStop::NullTransform.message(),
        reads_outside: IccStop::OutOfBounds.message(),
    }
}

/// Bakes with the display profile `bytes`.
fn with_display(label: impl Into<String>, bytes: Vec<u8>) -> Case {
    baking(label, &["--cubesize", "5", "--displayicc", "d.icc", "--v"])
        .file("d.icc", bytes)
        .stops(profile_stops())
}

/// Bakes a cube of `cube` nodes with the display profile `bytes`, and matches the wheel unless it
/// dies. Nothing here lets the port stop over memory the wheel reads (U-653).
fn on_cube(label: impl Into<String>, cube: &str, bytes: Vec<u8>) -> Case {
    baking(label, &["--cubesize", cube, "--displayicc", "d.icc"])
        .file("d.icc", bytes)
        .stops(Stop::IfTheWheelDies(IccStop::NullTransform.message()))
}

/// A `matf` element that gives `offsets` whatever comes in.
fn constant(inputs: usize, offsets: &[f32]) -> Vec<u8> {
    let mut values = vec![0f32; inputs * offsets.len()];
    values.extend_from_slice(offsets);
    matf(inputs, offsets.len(), &values)
}

/// A `matf` element that does nothing.
fn identity_matf() -> Vec<u8> {
    let values = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];
    matf(3, 3, &values)
}

/// A `clut` element of these `values`.
fn clut_of(inputs: usize, outputs: usize, grid: &[usize], values: &[f32]) -> Vec<u8> {
    let mut data = b"clut\0\0\0\0".to_vec();
    data.extend_from_slice(&(inputs as u16).to_be_bytes());
    data.extend_from_slice(&(outputs as u16).to_be_bytes());
    let mut dimensions = [0u8; 16];
    for (slot, points) in dimensions.iter_mut().zip(grid) {
        *slot = *points as u8;
    }
    data.extend_from_slice(&dimensions);
    for value in values {
        data.extend_from_slice(&value.to_be_bytes());
    }
    data
}

/// A monitor profile with `elements` as its float `D2B0` tag, in XYZ, and a way back.
fn float_display(version: u32, elements: &[Vec<u8>]) -> Vec<u8> {
    Profile::new(version, b"mntr", b"RGB ", b"XYZ ")
        .tag(b"D2B0", mpet(3, 3, elements))
        .tag(b"B2D0", mpet(3, 3, &[identity_matf()]))
        .bytes()
}

/// How many cases of a sweep ended in the tool dying.
fn deaths(outcomes: &[common::Outcome]) -> usize {
    outcomes
        .iter()
        .filter(|outcome| !(0..=255).contains(&outcome.exit_code))
        .count()
}

/// Two runs of the wheel, more than a second apart, on the same input: the time in the header is
/// the only thing that changes, which is why the sweeps set only that aside. The description
/// defaults to the file name, so the name is the same, too.
#[test]
fn creation_time_is_all_that_varies() {
    let cases = [baking("twice", &["--cubesize", "5"])];
    let first = wheel_outcomes("ociobakelut_icc_twice", TOOL, &cases);
    std::thread::sleep(Duration::from_millis(1100));
    let second = wheel_outcomes("ociobakelut_icc_twice", TOOL, &cases);

    let (first, second) = (&first[0].files["out.icc"], &second[0].files["out.icc"]);
    assert_eq!(first.len(), second.len());
    let differing: Vec<usize> = (0..first.len())
        .filter(|&i| first[i] != second[i])
        .collect();
    assert!(!differing.is_empty(), "two runs must differ in the time");
    assert!(
        differing.iter().all(|at| ICC_CREATION_TIME.contains(at)),
        "{differing:?}"
    );
    // The seconds are the field's last one, and they did change.
    assert!(differing.iter().any(|&at| at >= ICC_CREATION_TIME.end - 2));
}

/// Every selection `every_format_from_a_config` gives the other formats, with this tool's own
/// path through them: a look or a display goes in a `LookTransform` or a `LegacyViewingPipeline`,
/// and a shaper is ignored with a warning.
#[test]
fn from_a_config() {
    let selections: Vec<(&str, Vec<&str>)> = vec![
        ("space", vec!["--inputspace", "lin", "--outputspace", "log"]),
        (
            "role",
            vec![
                "--inputspace",
                "scene_linear",
                "--outputspace",
                "compositing_log",
            ],
        ),
        (
            "gamma",
            vec!["--inputspace", "gamma", "--outputspace", "lin"],
        ),
        (
            "shaper",
            vec![
                "--inputspace",
                "lin",
                "--outputspace",
                "gamma",
                "--shaperspace",
                "log",
            ],
        ),
        (
            "displayview",
            vec!["--inputspace", "lin", "--displayview", "sRGB", "Film"],
        ),
        (
            "displayview_raw",
            vec!["--inputspace", "gamma", "--displayview", "sRGB", "Raw"],
        ),
        (
            "displayview_shaper",
            vec![
                "--inputspace",
                "lin",
                "--displayview",
                "sRGB",
                "Film",
                "--shaperspace",
                "log",
            ],
        ),
        (
            "displayview_looks",
            vec![
                "--inputspace",
                "lin",
                "--displayview",
                "sRGB",
                "Raw",
                "--looks",
                "warm",
            ],
        ),
        (
            "looks",
            vec![
                "--inputspace",
                "lin",
                "--outputspace",
                "gamma",
                "--looks",
                "warm",
            ],
        ),
        (
            "looks_shaper",
            vec![
                "--inputspace",
                "lin",
                "--outputspace",
                "gamma",
                "--looks",
                "warm",
                "--shaperspace",
                "log",
            ],
        ),
        ("raw", vec!["--inputspace", "raw", "--outputspace", "raw"]),
        (
            "out_of_range",
            vec!["--inputspace", "lin", "--outputspace", "matrix"],
        ),
        // Negative values through a square root: where the sampler gets NaN.
        (
            "nan",
            vec!["--inputspace", "matrix", "--outputspace", "nan_maker"],
        ),
    ];
    let mut cases = Vec::new();
    for (label, selection) in &selections {
        for (mode, extra) in [
            ("default", vec![]),
            ("cube5", vec!["--cubesize", "5"]),
            ("verbose", vec!["--cubesize", "3", "--v"]),
        ] {
            let mut argv = vec!["--iconfig", "a.ocio", "--format", "icc"];
            argv.extend(selection);
            argv.extend(&extra);
            argv.push("out.icc");
            cases.push(
                Case::new(format!("{label}_{mode}"), &argv)
                    .file("a.ocio", CONFIG)
                    .icc(),
            );
        }
        // With a config from the environment, too.
        let mut argv = vec!["--format", "icc", "--cubesize", "3"];
        argv.extend(selection);
        argv.push("out.icc");
        cases.push(
            Case::new(format!("{label}_env"), &argv)
                .file("a.ocio", CONFIG)
                .env("OCIO", "a.ocio")
                .icc(),
        );
    }
    let summary = sweep("ociobakelut_icc_config", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), cases.len());
    assert_eq!(summary.with_exit_code(0), cases.len());
}

/// The LUT chains of `every_format_from_luts`, and the LUT kinds with a kernel of their own.
#[test]
fn from_luts() {
    let lut1 = test_file("lut1d_1.spi1d");
    let lut3 = test_file("lut3d_1.spi3d");
    let cdl = test_file("cdl_test1.cc");
    let chains: Vec<(&str, Vec<&str>)> = vec![
        ("one", vec!["--lut", &lut1]),
        ("three_d", vec!["--lut", &lut3]),
        ("two", vec!["--lut", &lut1, "--lut", &lut3]),
        ("inverse", vec!["--invlut", &lut1]),
        ("inverse_3d", vec!["--invlut", &lut3]),
        ("forward_and_back", vec!["--lut", &lut1, "--invlut", &lut1]),
        ("slope", vec!["--slope", "1.1", "1", "0.9"]),
        ("offset", vec!["--offset", "0.1", "0", "-0.1"]),
        ("offset10", vec!["--offset10", "1", "2", "3"]),
        ("power", vec!["--power", "2", "1", "0.5"]),
        ("sat", vec!["--sat", "0.5"]),
        (
            "cdl_and_lut",
            vec!["--slope", "1.5", "1", "1", "--lut", &lut1],
        ),
        ("lut_and_cdl", vec!["--lut", &lut1, "--sat", "2"]),
        ("cc", vec!["--lut", &cdl]),
    ];
    let mut cases = Vec::new();
    for (label, chain) in &chains {
        let mut argv = vec!["--cubesize", "9", "--format", "icc"];
        argv.extend(chain);
        argv.push("out.icc");
        cases.push(Case::new(*label, &argv).icc());
    }
    for name in [
        "clf/lut1d_half_domain_raw_half_set.clf",
        "clf/lut3d_17x17x17_10i_12i.clf",
        "matrix_example4x4.ctf",
        "log_lintolog_3chan.ctf",
        "clf/cdl_all_styles.clf",
        "clf/range.clf",
        "gamma_test1.ctf",
        "fixed_function.ctf",
        "exposure_contrast_log.ctf",
    ] {
        let file = test_file(name);
        let argv = [
            "--cubesize",
            "9",
            "--lut",
            &file,
            "--format",
            "icc",
            "out.icc",
        ];
        cases.push(Case::new(name, &argv).icc());
    }
    let summary = sweep("ociobakelut_icc_luts", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), cases.len());
}

#[test]
fn cube_sizes() {
    let mut cases = vec![baking("default", &[])];
    for size in [
        "-1", "0", "1", "2", "3", "4", "5", "9", "16", "17", "32", "33", "48", "64", "65",
    ] {
        cases.push(baking(format!("size_{size}"), &["--cubesize", size]));
    }
    // What the CLUT's allocation refuses: `CubeSize` from 256 up, and the tags can't be saved.
    for size in [
        "256", "257", "450", "1000", "2000", "1625", "1626", "100000",
    ] {
        cases.push(baking(format!("size_{size}"), &["--cubesize", size, "--v"]));
    }
    let summary = sweep("ociobakelut_icc_cubes", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), 16);
    let refused = &summary.outcomes[16..];
    assert_eq!(refused.len(), 8);
    assert!(refused.iter().all(|outcome| outcome.files.is_empty()));
    assert!(refused.iter().all(|outcome| outcome.exit_code == 0));
}

/// The largest cube there is, 16,581,375 nodes: not for every run.
#[test]
fn largest_cube() {
    if std::env::var("OCIO_RS_TIER").as_deref() != Ok("full")
        && std::env::var("OCIO_RS_TIER").as_deref() != Ok("exhaustive")
    {
        return;
    }
    let cases = [baking("size_255", &["--cubesize", "255"])];
    let summary = sweep("ociobakelut_icc_largest", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), 1);
}

#[test]
fn white_points() {
    let mut cases = Vec::new();
    for temp in [
        "4000", "4001", "4500", "5000", "5003", "6504", "6505", "6506", "6999", "7000", "7001",
        "9300", "10000", "24999", "25000",
    ] {
        cases.push(baking(
            format!("temp_{temp}"),
            &["--cubesize", "5", "--whitepoint", temp],
        ));
    }
    let good = cases.len();
    // cmsWhitePointFromTemp fails and the tool goes on with a point it never set: U-651.
    for temp in ["3999", "25001", "0", "-1", "100000", "2147483647"] {
        cases.push(
            baking(
                format!("temp_{temp}"),
                &["--cubesize", "5", "--whitepoint", temp, "--v"],
            )
            .stops(Stop::Always(IccStop::WhitePoint.message())),
        );
    }
    let summary = sweep("ociobakelut_icc_white", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), cases.len());
    for outcome in &summary.outcomes[good..] {
        assert_eq!(outcome.exit_code, 0);
        assert!(
            String::from_utf8_lossy(&outcome.stderr)
                .contains("cmsWhitePointFromTemp: invalid temp")
        );
    }
}

/// A bad temperature changes nothing but a tag of the output, so everything else the wheel says
/// is said here, too, up to where the profile is written (U-651).
#[test]
fn white_points_with_more_to_report() {
    let shaper = Profile::new(V4, b"mntr", b"RGB ", b"XYZ ")
        .shaper(&gamma(2.2))
        .bytes();
    let stop = IccStop::WhitePoint.message();
    let dies = IccStop::NullTransform.message();
    let mut cases = Vec::new();
    for temp in ["3999", "25001"] {
        let argv = |extra: &'static [&'static str]| {
            let mut argv = vec!["--whitepoint", temp, "--v"];
            argv.extend(extra);
            argv
        };
        let missing = argv(&["--cubesize", "3", "--displayicc", "nope.icc"]);
        cases.push(baking(format!("missing_{temp}"), &missing).stops(Stop::IfTheWheelDies(dies)));
        let display = argv(&["--cubesize", "3", "--displayicc", "d.icc"]);
        cases.push(
            baking(format!("display_{temp}"), &display)
                .file("d.icc", shaper.clone())
                .stops(Stop::Always(stop)),
        );
        // No CLUT, so no transform to die on: the wheel saves nothing (I-650).
        let no_clut = argv(&["--cubesize", "300", "--displayicc", "nope.icc"]);
        cases.push(baking(format!("no_clut_{temp}"), &no_clut).stops(Stop::Always(stop)));
    }
    let summary = sweep("ociobakelut_icc_white_more", TOOL, PROGRAM, &cases);
    assert_eq!(deaths(&summary.outcomes), 2);
    assert_eq!(summary.writing_files(), 2);
    assert_eq!(summary.with_exit_code(0), 4);
    for outcome in &summary.outcomes {
        let stderr = String::from_utf8_lossy(&outcome.stderr);
        assert!(stderr.starts_with("OCIO Error: cmsWhitePointFromTemp: invalid temp"));
    }
    let not_found = summary
        .outcomes
        .iter()
        .filter(|outcome| String::from_utf8_lossy(&outcome.stderr).contains("not found"))
        .count();
    assert_eq!(not_found, 4);
}

#[test]
fn text_tags() {
    let long = "x".repeat(3000);
    let mut cases = vec![
        baking(
            "description",
            &["--cubesize", "3", "--description", "A display"],
        ),
        baking(
            "copyright",
            &["--cubesize", "3", "--copyright", "(c) someone"],
        ),
        baking(
            "both_verbose",
            &[
                "--cubesize",
                "3",
                "--description",
                "d",
                "--copyright",
                "c",
                "--v",
            ],
        ),
        baking(
            "empty_description",
            &["--cubesize", "3", "--description", ""],
        ),
        baking("empty_copyright", &["--cubesize", "3", "--copyright", ""]),
        baking(
            "long_description",
            &["--cubesize", "3", "--description", &long],
        ),
        baking("long_copyright", &["--cubesize", "3", "--copyright", &long]),
        baking("spaces", &["--cubesize", "3", "--description", " a  b "]),
    ];
    // argv reaches the tool as bytes only on Linux (docs/improvements.md, I-601).
    if cfg!(unix) {
        cases.push(baking(
            "not_ascii",
            &["--cubesize", "3", "--description", "caf\u{e9} \u{20ac}"],
        ));
    }
    let summary = sweep("ociobakelut_icc_text", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), cases.len());
}

/// The file name is the description by default, and where lcms2 can fail to write.
#[test]
fn output_paths() {
    let lut = lut();
    let lut = lut.as_str();
    let bake = |output: &str| {
        vec![
            "--cubesize".to_string(),
            "3".to_string(),
            "--lut".to_string(),
            lut.to_string(),
            "--format".to_string(),
            "icc".to_string(),
            "--v".to_string(),
            output.to_string(),
        ]
    };
    let case = |label: &str, output: &str| {
        let argv = bake(output);
        let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
        Case::new(label, &argv).icc()
    };
    let cases = vec![
        case("name", "another name.icc"),
        case("not_icc", "out.bin"),
        case("no_directory", "missing/out.icc"),
        case("into_a_file", "a.txt/out.icc").file("a.txt", "x"),
        case("over_a_file", "a.txt").file("a.txt", "x"),
        case("over_a_directory", "sub").file("sub/a.txt", "x"),
        case("in_a_directory", "sub/out.icc").file("sub/a.txt", "x"),
    ];
    let summary = sweep("ociobakelut_icc_paths", TOOL, PROGRAM, &cases);
    assert!(summary.writing_files() >= 3, "{}", summary.writing_files());
    let failed = summary
        .outcomes
        .iter()
        .filter(|outcome| String::from_utf8_lossy(&outcome.stderr).contains("Couldn't create"))
        .count();
    assert!(failed >= 3, "{failed}");
}

/// Windows only: a hidden or system file can't be opened for writing, as with `std::ofstream`.
#[cfg(windows)]
#[test]
fn output_file_attributes() {
    let cases: Vec<Case> = ["h", "s", "hs", "r"]
        .iter()
        .map(|letters| {
            baking(format!("attribute_{letters}"), &["--cubesize", "3"])
                .file("out.icc", "before")
                .attribute("out.icc", letters)
        })
        .collect();
    let summary = sweep("ociobakelut_icc_attributes", TOOL, PROGRAM, &cases);
    assert_eq!(summary.outcomes.len(), 4);
}

/// The profiles of upstream's tests, of lcms2's, and of its plug-ins' and fuzzers'.
fn real_profiles() -> Vec<(String, Vec<u8>)> {
    let mut found = Vec::new();
    for name in [
        "icc-test-1.icc",
        "icc-test-2.pf",
        "icc-test-3.icm",
        "icc-test-pc1.icc",
        "icc-test-pc2.icc",
        "icc-test-pc3.icc",
        "icc-test-pc4.icc",
    ] {
        found.push((name.to_string(), test_file_bytes(name)));
    }
    let lcms2 = paths::upstream_dir().parent().unwrap().join("Little-CMS");
    for directory in ["testbed", "plugins/test_profiles", "fuzzers/corpus"] {
        let mut names: Vec<_> = std::fs::read_dir(lcms2.join(directory))
            .unwrap_or_else(|e| {
                panic!("upstream/Little-CMS/{directory}: {e}: initialise that submodule")
            })
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "icc"))
            .collect();
        names.sort();
        for path in names {
            let name = path.file_name().unwrap().to_str().unwrap();
            found.push((format!("{directory}/{name}"), std::fs::read(&path).unwrap()));
        }
    }
    assert!(found.len() >= 24, "{}", found.len());
    found
}

#[test]
fn display_profiles_of_the_test_suites() {
    let profiles = real_profiles();
    let cases: Vec<Case> = profiles
        .into_iter()
        .map(|(name, bytes)| with_display(name, bytes))
        .collect();
    let summary = sweep("ociobakelut_icc_real", TOOL, PROGRAM, &cases);
    // RGB ones are profiles to write, and the CMYK and gray ones are refused.
    assert!(summary.writing_files() >= 15, "{}", summary.writing_files());
    assert!(
        deaths(&summary.outcomes) >= 5,
        "{}",
        deaths(&summary.outcomes)
    );
}

#[test]
fn display_profiles_generated() {
    let profiles = generated();
    let cases: Vec<Case> = profiles
        .into_iter()
        .map(|(name, bytes)| with_display(name, bytes))
        .collect();
    let summary = sweep("ociobakelut_icc_generated", TOOL, PROGRAM, &cases);
    assert!(
        summary.writing_files() >= 150,
        "{}",
        summary.writing_files()
    );
    assert!(
        deaths(&summary.outcomes) >= 100,
        "{}",
        deaths(&summary.outcomes)
    );
    assert!(
        summary.port_stops <= cases.len() / 50,
        "{}",
        summary.port_stops
    );
    // Each message lcms2 can give about a profile shows up somewhere in the sweep.
    let errors: Vec<String> = summary
        .outcomes
        .iter()
        .map(|outcome| String::from_utf8_lossy(&outcome.stderr).into_owned())
        .collect();
    for expected in [
        "Wrong input color space on transform",
        "Wrong output color space on transform",
        "Couldn't link the profiles",
        "ColorSpace mismatch",
        "Corrupted tag 'rTRC'",
        "Corrupted tag 'clrt'",
        "Corrupted tag 'D2B0'",
        "Corrupted tag 'A2B0'",
        "Too many colorants '17'",
        "Unknown parametric curve type '5'",
        "Unknown MPE type 'nope' found.",
        "Unknown precision of '3'",
        "Unsupported profile version '0x5100000'",
        "Unsupported device class '0x6e6f7065'",
        "Duplicate tag found",
        "Too many tags (101)",
        "Couldn't create tone curve of more than 65530 entries",
    ] {
        // `too_long` has 40000 points, which `Type_Curve_Read` refuses first.
        if expected.contains("65530") {
            continue;
        }
        assert!(
            errors.iter().any(|text| text.contains(expected)),
            "nothing says {expected:?}"
        );
    }
}

/// Many nodes through the interpolators and curves of a profile: a rounding that shows once in
/// thousands of values needs them.
#[test]
fn display_profiles_on_a_dense_cube() {
    let prefixes = [
        "shaper_table",
        "shaper_para",
        "mft2_noise",
        "mft2_extreme",
        "mft1_noise",
        "mab_noise",
        "mab_extreme",
        "mpet_matrix_to",
        "mpet_clut3",
    ];
    let cases: Vec<Case> = generated()
        .into_iter()
        .filter(|(name, _)| prefixes.iter().any(|prefix| name.starts_with(prefix)))
        .map(|(name, bytes)| {
            let argv = ["--cubesize", "33", "--displayicc", "d.icc"];
            baking(name, &argv)
                .file("d.icc", bytes)
                .stops(profile_stops())
        })
        .collect();
    assert!(cases.len() >= 60, "{}", cases.len());
    let summary = sweep("ociobakelut_icc_dense", TOOL, PROGRAM, &cases);
    assert!(summary.writing_files() >= 60, "{}", summary.writing_files());
    assert!(
        summary.port_stops <= cases.len() / 20,
        "{}",
        summary.port_stops
    );
}

/// A NaN from the processor at the nodes of the cube: both wheels' casts make it 0 (I-655). A
/// huge matrix, squared to infinity, then a difference of infinities. The optimizer can't merge
/// the two matrices around the curve.
#[test]
fn nan_from_the_processor() {
    let matrix = |values: &str| {
        format!(
            "<Matrix inBitDepth=\"32f\" outBitDepth=\"32f\"><Array dim=\"3 3\">{values}</Array></Matrix>"
        )
    };
    let lut = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <ProcessList id=\"nan\" compCLFversion=\"3.0\">\n{}\n\
         <Exponent inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"basicFwd\">\
         <ExponentParams exponent=\"2.0\"/></Exponent>\n{}\n</ProcessList>\n",
        matrix("1e30 0 0 0 1e30 0 0 0 1e30"),
        matrix("1 -1 0 0 1 -1 -1 0 1"),
    );
    let argv = [
        "--cubesize",
        "5",
        "--lut",
        "nan.clf",
        "--format",
        "icc",
        "out.icc",
    ];
    let cases = [Case::new("nan_lut", &argv).file("nan.clf", lut).icc()];
    let summary = sweep("ociobakelut_icc_nan", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), 1);
}

/// `--displayicc` with paths that aren't profiles.
#[test]
fn display_paths() {
    let stop = Stop::IfTheWheelDies(IccStop::NullTransform.message());
    let display = |label: &str, path: &str| {
        baking(label, &["--cubesize", "3", "--displayicc", path, "--v"]).stops(stop)
    };
    let cases = vec![
        display("missing", "nope.icc"),
        display("in_a_missing_directory", "no/nope.icc"),
        display("directory", "sub").file("sub/a.txt", "x"),
        display("empty_file", "d.icc").file("d.icc", ""),
        display("short_file", "d.icc").file("d.icc", vec![0u8; 127]),
        display("header_only", "d.icc").file("d.icc", vec![0u8; 128]),
        display("text_file", "d.icc").file("d.icc", vec![b'x'; 400]),
        // An empty argument is no display profile at all: sRGB.
        baking("empty_path", &["--cubesize", "3", "--displayicc", ""]),
        // No CLUT to sample: the samplers don't run, so the missing transforms don't matter.
        baking(
            "no_clut",
            &["--cubesize", "300", "--displayicc", "nope.icc"],
        ),
    ];
    let summary = sweep("ociobakelut_icc_display_paths", TOOL, PROGRAM, &cases);
    assert_eq!(deaths(&summary.outcomes), 7);
    assert_eq!(summary.writing_files(), 1);
}

/// `cmsSignalError` keeps 1022 characters of a message on Linux and 1023 on Windows, where the
/// byte after them is one nobody set: U-650.
#[test]
fn long_messages() {
    let stop = Stop::IfTheWheelDies(IccStop::NullTransform.message());
    let mut cases = Vec::new();
    for length in [1000, 1003, 1004, 1005, 1006, 1015, 1016, 1100, 3000] {
        let name = "x".repeat(length);
        let argv = ["--cubesize", "3", "--displayicc", &name];
        cases.push(baking(format!("name_{length}"), &argv).stops(stop));
    }
    let summary = sweep("ociobakelut_icc_long_messages", TOOL, PROGRAM, &cases);
    assert_eq!(deaths(&summary.outcomes), cases.len());
    let longest = |outcome: &common::Outcome| {
        let stderr = String::from_utf8_lossy(&outcome.stderr).into_owned();
        stderr.lines().map(str::len).max().unwrap()
    };
    let cap = if cfg!(windows) { 1023 } else { 1022 } + "OCIO Error: ".len();
    assert!(longest(&summary.outcomes[0]) < cap);
    assert_eq!(longest(summary.outcomes.last().unwrap()), cap);
}

/// Profiles with something wrong in them, the same ones on each run. A valid start and one
/// defect reach further into the readers than a random file does.
#[test]
fn corrupted_profiles() {
    let full = matches!(
        std::env::var("OCIO_RS_TIER").as_deref(),
        Ok("full") | Ok("exhaustive")
    );
    let count = if full { 3000 } else { 400 };
    let mut starts = real_profiles();
    // Keep to the RGB ones, where the readers run, and to those of a few kilobytes, for speed.
    starts.retain(|(_, bytes)| bytes.len() < 100_000 && &bytes[16..20] == b"RGB ");
    let mut generated = generated();
    generated.retain(|(name, _)| {
        [
            "shaper_table_power",
            "mft2_noise",
            "mft1_smooth",
            "mab_noise",
            "mpet_",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
    });
    starts.extend(generated);
    assert!(starts.len() >= 40, "{}", starts.len());

    let mut rng = Rng(0x0C10_C0DE_1CC0_0001);
    let cases: Vec<Case> = (0..count)
        .map(|index| {
            let (name, bytes) = &starts[rng.below(starts.len())];
            let mut bytes = corrupt(&mut rng, bytes);
            // Some take two defects.
            if rng.below(4) == 0 && bytes.len() > 140 {
                bytes = corrupt(&mut rng, &bytes);
            }
            with_display(format!("corrupt_{index}_{name}"), bytes)
        })
        .collect();
    let summary = sweep("ociobakelut_icc_corrupted", TOOL, PROGRAM, &cases);
    assert!(summary.writing_files() >= count / 8);
    assert!(deaths(&summary.outcomes) >= count / 8);
    assert!(summary.port_stops <= count / 50, "{}", summary.port_stops);
}

/// Curves of `cvst` elements at what bounds them: results that are infinite, and the branches of
/// the formulas that an input in `[0, 1]` doesn't take without a ramp before them.
#[test]
fn segmented_curves_at_their_limits() {
    let three = |segments: Vec<u8>| cvst(&[segments.clone(), segments.clone(), segments]);
    let ramp = || {
        let values = [
            2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, -0.5, -0.5, -0.5,
        ];
        matf(3, 3, &values)
    };
    let flat = |value: f32| formula(0, &[1.0, value, 0.0, 0.0]);
    let curves: Vec<(&str, Vec<u8>)> = vec![
        (
            "minus_inf",
            segmented(&[], &[formula(2, &[-1.0, 1e10, 100.0, 0.0, 0.0])]),
        ),
        (
            "plus_inf",
            segmented(&[], &[formula(2, &[1.0, 1e10, 100.0, 0.0, 0.0])]),
        ),
        (
            "log_of_negative",
            segmented(&[], &[formula(1, &[0.5, 0.25, 1.5, 0.75, 0.1])]),
        ),
        (
            "log_e_zero",
            segmented(&[], &[formula(1, &[1.0, 1.0, 1.0, 0.0, 0.1])]),
        ),
        (
            "gamma_one_negative",
            segmented(&[], &[formula(0, &[1.0, 1.0, -0.5, 0.0])]),
        ),
        (
            "gamma_negative_e",
            segmented(&[], &[formula(0, &[2.0, 1.0, -0.5, 0.0])]),
        ),
        (
            "breaks_equal",
            segmented(&[0.5, 0.5], &[flat(1.0), flat(2.0), flat(3.0)]),
        ),
        (
            "breaks_descending",
            segmented(&[0.75, 0.25], &[flat(1.0), flat(2.0), flat(3.0)]),
        ),
        (
            "sampled_one_point",
            segmented(&[0.0, 1.0], &[flat(0.0), sampled(&[0.5]), flat(0.0)]),
        ),
        (
            "sampled_no_points",
            segmented(&[0.0, 1.0], &[flat(0.0), sampled(&[]), flat(0.0)]),
        ),
        (
            "sampled_narrow",
            segmented(
                &[0.5, 0.5000001],
                &[flat(0.0), sampled(&[0.25, 0.75]), flat(0.0)],
            ),
        ),
        (
            "sampled_last",
            segmented(&[0.5], &[flat(1.0), sampled(&[0.25, 0.75])]),
        ),
        ("huge_result", segmented(&[], &[flat(1e19)])),
        (
            "exp_of_negative_base",
            segmented(&[], &[formula(2, &[1.0, -2.0, 1.0, 0.0, 0.0])]),
        ),
        (
            "exp_zero_base",
            segmented(&[], &[formula(2, &[1.0, 0.0, -1.0, 0.0, 0.0])]),
        ),
    ];
    let mut cases = Vec::new();
    for (name, curve) in curves {
        for (mode, elements) in [
            ("alone", vec![three(curve.clone())]),
            ("after_ramp", vec![ramp(), three(curve.clone())]),
            (
                "before_clut",
                vec![
                    ramp(),
                    three(curve.clone()),
                    clutf(3, 3, &[3, 3, 3], Fill::Noise(5)),
                ],
            ),
            ("then_matrix", vec![three(curve.clone()), identity_matf()]),
        ] {
            let label = format!("{name}_{mode}");
            cases.push(on_cube(label, "9", float_display(V4, &elements)));
        }
    }
    let summary = sweep("ociobakelut_icc_segments", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), cases.len());
}

/// `clut` elements of the shapes the usual profiles don't have, and inputs at `fclamp`'s limit,
/// which a table of huge values shows.
#[test]
fn float_clut_shapes() {
    let mut cases = Vec::new();
    let ramp = [0.0, 0.1, 0.15, 0.3, 0.2, 0.6, 0.7, 0.9, 1.0];
    let elements = vec![
        matf(3, 1, &[0.3, 0.3, 0.4, 0.0]),
        clut_of(1, 1, &[9], &ramp),
        matf(1, 3, &[1.0, 0.9, 0.8, 0.0, 0.0, 0.0]),
    ];
    cases.push(on_cube("one_to_one", "17", float_display(V4, &elements)));
    for inputs in 9..=15usize {
        let mut values = Vec::new();
        for row in 0..inputs {
            for column in 0..3 {
                values.push(if row % 3 == column { 0.9 } else { 0.05 });
            }
        }
        values.resize(3 * inputs + inputs, 0.0);
        let grid = vec![2; inputs];
        let elements = vec![
            matf(3, inputs, &values),
            clutf(inputs, 3, &grid, Fill::Noise(inputs as u64)),
        ];
        let label = format!("inputs_{inputs}");
        cases.push(on_cube(label, "5", float_display(V4, &elements)));
    }
    // All of the table's height at the last node, where the three inputs add up.
    let table: Vec<f32> = (0..24).map(|i| if i >= 21 { 1e9 } else { 0.0 }).collect();
    for tiny in [0.0, 2e-10f32, 5e-10, 0.9e-9, 1.1e-9, 2e-9] {
        let elements = vec![
            constant(3, &[tiny, tiny, tiny]),
            clut_of(3, 3, &[2, 2, 2], &table),
            identity_matf(),
        ];
        let label = format!("tiny_{tiny}");
        cases.push(on_cube(label, "2", float_display(V4, &elements)));
    }
    let summary = sweep("ociobakelut_icc_clut_shapes", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), cases.len());
}

/// The tolerance of the matrix check in the `mft2` reader, on both sides of it, and the limits of
/// the parametric curves' inverses.
#[test]
fn near_identity_matrices_and_reversed_curves() {
    // In units of the file's 15.16 numbers, the last under the limit, 1/65535, and the first over.
    let close = 1.0 / 65536.0;
    let apart = 2.0 / 65536.0;
    let near = [1.0 + close, close, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let far = [1.0 + apart, apart, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let curve = table(256, |x| x.powf(1.2));
    let mut cases = Vec::new();
    for (pcs_name, pcs) in [("xyz", b"XYZ "), ("lab", b"Lab ")] {
        for (name, matrix) in [("near", near), ("far", far)] {
            let bytes = Profile::new(V2, b"prtr", b"RGB ", pcs)
                .tag(b"A2B0", mft2(3, 3, 5, Some(matrix), &curve, Fill::Noise(3)))
                .tag(b"B2A0", mft2(3, 3, 5, Some(matrix), &curve, Fill::Noise(5)))
                .bytes();
            cases.push(on_cube(format!("mft2_{name}_{pcs_name}"), "9", bytes));
        }
    }
    for (name, curve) in [
        ("a_small", para(1, &[2.2, 0.00005, 0.0])),
        ("g_small", para(1, &[0.00005, 1.0, 0.0])),
        ("c3_small", para(3, &[2.4, 1.0, 0.0, 0.00005, 0.5])),
        (
            "c4_small",
            para(4, &[2.4, 1.0, 0.0, 0.00005, 0.5, 0.0, 0.0]),
        ),
        ("e_zero", para(3, &[2.4, 0.5, -0.5, 0.1, 0.0])),
        ("e4", para(4, &[2.4, 1.0, -0.5, 0.1, 0.0, 0.5, 0.0])),
        ("b_negative", para(2, &[2.2, 1.0, -0.2, 0.3])),
        ("b_positive", para(2, &[2.2, 1.0, 0.2, 0.3])),
        ("type_1", para(1, &[2.2, 1.0, 0.2])),
    ] {
        for (version_name, version) in [("v2", V2), ("v4", V4)] {
            let bytes = Profile::new(version, b"mntr", b"RGB ", b"XYZ ")
                .shaper(&curve)
                .bytes();
            cases.push(on_cube(format!("para_{name}_{version_name}"), "9", bytes));
        }
    }
    let summary = sweep("ociobakelut_icc_near_limits", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), cases.len());
}

/// Negative values into `curv` and `para` curves, which a matrix before the `B` curves of a `mAB`
/// makes.
#[test]
fn curves_after_a_matrix() {
    let matrix = Some([
        2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, -0.5, -0.5, -0.5,
    ]);
    let srgb = [2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045];
    let kinds: Vec<(&str, Vec<u8>)> = vec![
        ("gamma", gamma(2.2)),
        ("gamma_one", gamma(1.0)),
        ("para0", para(0, &[2.2])),
        ("para0_one", para(0, &[1.0])),
        ("para1", para(1, &[2.2, 1.0, 0.2])),
        ("para2_b_positive", para(2, &[2.2, 1.0, 0.2, 0.05])),
        ("para2_b_negative", para(2, &[2.2, 1.0, -0.2, 0.05])),
        ("para3", para(3, &srgb)),
        ("para4", para(4, &[2.4, 0.9, 0.05, 0.08, 0.04, 0.01, 0.002])),
    ];
    let mut cases = Vec::new();
    for (name, curve) in kinds {
        let parts = LutV4 {
            m: Some(vec![gamma(1.0); 3]),
            matrix,
            b: Some(vec![curve; 3]),
            ..LutV4::default()
        };
        let bytes = Profile::new(V4, b"prtr", b"RGB ", b"XYZ ")
            .tag(b"A2B0", lut_v4(b"mAB ", 3, 3, &parts))
            .tag(b"B2A0", lut_v4(b"mBA ", 3, 3, &parts))
            .bytes();
        cases.push(on_cube(name, "9", bytes));
    }
    let summary = sweep("ociobakelut_icc_negative_curves", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), cases.len());
}

/// A v2 profile that has to work out its black point, with L* under 0 there, in both PCSs, and a
/// float tag alone on a v2 profile, which supports no intent, so that it has no black point.
#[test]
fn black_points() {
    let down = Some([
        1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, -0.2, -0.2, -0.2,
    ]);
    let mut cases = Vec::new();
    for (pcs_name, pcs) in [("xyz", b"XYZ "), ("lab", b"Lab ")] {
        let parts = LutV4 {
            m: Some(vec![gamma(1.0); 3]),
            matrix: down,
            b: Some(vec![gamma(1.0); 3]),
            ..LutV4::default()
        };
        let bytes = Profile::new(V2, b"prtr", b"RGB ", pcs)
            .tag(b"A2B0", lut_v4(b"mAB ", 3, 3, &parts))
            .tag(b"B2A0", lut_v4(b"mBA ", 3, 3, &parts))
            .bytes();
        cases.push(on_cube(format!("under_zero_{pcs_name}"), "9", bytes));
    }
    let scale = [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 0.2, 0.2, 0.2];
    let elements = vec![matf(3, 3, &scale)];
    cases.push(on_cube("v2_float_only", "5", float_display(V2, &elements)));
    let summary = sweep("ociobakelut_icc_black_points", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), cases.len());
}

/// Tables the readers look at twice: a tag at offset 0, a CLUT that claims a grid of 1 past its
/// inputs, and `mft2` curves of one and of no entries.
#[test]
fn table_shapes() {
    let mut at_zero = Profile::new(V4, b"mntr", b"RGB ", b"XYZ ")
        .shaper(&gamma(2.2))
        .bytes();
    at_zero[132 + 4..132 + 8].copy_from_slice(&0u32.to_be_bytes());
    let parts = LutV4 {
        a: Some(vec![gamma(1.0); 3]),
        clut: Some((vec![3, 3, 3], Fill::Noise(3), 2)),
        b: Some(vec![gamma(1.0); 3]),
        ..LutV4::default()
    };
    let mut tag = lut_v4(b"mAB ", 3, 3, &parts);
    let clut_at = u32::from_be_bytes(tag[24..28].try_into().unwrap()) as usize;
    tag[clut_at + 8] = 1;
    let one_past = Profile::new(V4, b"prtr", b"RGB ", b"XYZ ")
        .tag(b"A2B0", tag)
        .tag(b"B2A0", lut_v4(b"mBA ", 3, 3, &parts))
        .bytes();
    let one_entry = Profile::new(V2, b"prtr", b"RGB ", b"XYZ ")
        .tag(b"A2B0", mft2(3, 3, 5, None, &[0x8000], Fill::Noise(3)))
        .tag(b"B2A0", mft2(3, 3, 5, None, &[], Fill::Noise(3)))
        .bytes();
    let cases = vec![
        on_cube("offset_zero", "5", at_zero),
        on_cube("grid_1_past_inputs", "5", one_past),
        on_cube("mft2_one_entry", "5", one_entry),
    ];
    let summary = sweep("ociobakelut_icc_table_shapes", TOOL, PROGRAM, &cases);
    // Each is refused, with the reader's error.
    assert_eq!(deaths(&summary.outcomes), 3);
}

/// `wtpt` is read twice for each of the two transforms, so that an error in it shows four times.
#[test]
fn white_tag_that_cannot_be_read() {
    let bytes = Profile::new(V4, b"mntr", b"RGB ", b"XYZ ")
        .shaper(&gamma(2.2))
        .tag(b"wtpt", vec![b'X', b'Y', b'Z', b' ', 0, 0, 0, 0, 0, 1])
        .bytes();
    let cases = vec![on_cube("white_at_end", "3", bytes)];
    let summary = sweep("ociobakelut_icc_white_tag", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), 1);
    let stderr = String::from_utf8_lossy(&summary.outcomes[0].stderr);
    assert_eq!(stderr.matches("Corrupted tag 'wtpt'").count(), 4);
}

/// The limit of `f` in `cmsXYZ2Lab`, `(24/116)^3`, between two floats that a rounded `0.008856`
/// would put on the other side of it: the wheel's `a` is 28320 at this X and Y, and 28319 with the
/// rounded limit.
#[test]
fn lab_limit() {
    let scale = 32768.0f64 / 65535.0;
    // The input that the 16-bit scale of the XYZ PCS makes `target`.
    let input_for = |target: f32| {
        let mut input = (target as f64 / scale) as f32;
        for _ in 0..8 {
            let got = (input as f64 * scale) as f32;
            if got == target {
                return input;
            }
            let steps = target.to_bits() as i64 - got.to_bits() as i64;
            input = f32::from_bits((input.to_bits() as i64 + steps) as u32);
        }
        panic!("no input gives {target}");
    };
    let x = input_for(0.004_269_704_688_340_425_5);
    let y = input_for(0.007_131_286_896_765_232);
    let z = input_for(0.005);
    let bytes = float_display(V2, &[constant(3, &[x, y, z])]);
    let cases = vec![on_cube("limit", "2", bytes)];
    let summary = sweep("ociobakelut_icc_lab_limit", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), 1);
}

/// Both sites of U-653 and what lies next to them. An input that `_cmsQuickFloor` rounds up to the
/// last node reads the table's next one, which isn't there, and the port stops: in those cases and
/// no other, which the count of its stops says. The wheel may die on them, even on these small
/// tables. The others compare exactly: the same rounding on an inner node, or on an input with a
/// row after it, stays in the table, and the three inputs under the fourth use `floor`.
#[test]
fn float_cluts_at_their_last_node() {
    let below_one = f32::from_bits(0x3f7f_ffff); // 1 - 2^-24
    let past = |label: &str, bytes: Vec<u8>| {
        baking(label, &["--cubesize", "2", "--displayicc", "d.icc"])
            .file("d.icc", bytes)
            .stops(profile_stops())
    };
    let inside = |label: &str, bytes: Vec<u8>| {
        baking(label, &["--cubesize", "2", "--displayicc", "d.icc"]).file("d.icc", bytes)
    };

    // Two inputs: `BilinearInterpFloat`, with 4 and 6 cells, so 2^-17/4 of the last node.
    let grid2 = [5, 7];
    let noise2: Vec<f32> = (0..5 * 7 * 3)
        .map(|i| ((i * 37 % 101) as f32) / 100.0)
        .collect();
    let two = |a: f32, b: f32| {
        let clut = clut_of(2, 3, &grid2, &noise2);
        float_display(V2, &[constant(3, &[a, b]), clut])
    };
    // Four inputs: `Eval4InputsFloat`, with 2 cells each, so 2^-17/2.
    let grid4 = [3, 3, 3, 3];
    let noise4: Vec<f32> = (0..3 * 3 * 3 * 3 * 3)
        .map(|i| ((i * 41 % 103) as f32) / 102.0)
        .collect();
    let four = |a: f32, b: f32, c: f32, d: f32| {
        let clut = clut_of(4, 3, &grid4, &noise4);
        float_display(V2, &[constant(3, &[a, b, c, d]), clut])
    };
    // Five inputs: `Eval5InputsFloat`, a level over the four, with its own first input.
    let grid5 = [3, 3, 3, 3, 3];
    let noise5: Vec<f32> = (0..3 * 3 * 3 * 3 * 3 * 3)
        .map(|i| ((i * 43 % 107) as f32) / 106.0)
        .collect();
    let five = |a: f32, b: f32| {
        let clut = clut_of(5, 3, &grid5, &noise5);
        float_display(V2, &[constant(3, &[a, b, 0.4, 0.3, 0.2]), clut])
    };
    // A curve can get there, too, after a matrix that keeps the inputs.
    let keep = matf(3, 2, &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
    let scale = |by: f32| segmented(&[], &[formula(0, &[1.0, by, 0.0, 0.0])]);
    let curved = |first: f32, second: f32| {
        let curves = cvst(&[scale(first), scale(second)]);
        let clut = clut_of(2, 3, &grid2, &noise2);
        float_display(V2, &[keep.clone(), curves, clut])
    };

    let cases = vec![
        // The first input.
        past("two_first", two(below_one, 0.5)),
        past("two_first_1.5e-6", two(1.0 - 1.5e-6, 0.25)),
        inside("two_first_3e-6", two(1.0 - 3.0e-6, 0.25)),
        past("two_both", two(below_one, below_one)),
        past("four_first", four(below_one, 0.5, 0.4, 0.3)),
        past("four_first_3e-6", four(1.0 - 3.0e-6, 0.5, 0.4, 0.3)),
        inside("four_first_6e-6", four(1.0 - 6.0e-6, 0.5, 0.4, 0.3)),
        past("five_first", five(below_one, 0.5)),
        // An inner input, with the first on the last node: the same, from there.
        past("two_second_after_one", two(1.0, below_one)),
        past("five_second_after_one", five(1.0, below_one)),
        // The second call of the level above starts on the last row.
        past("five_second_on_the_last_row", five(0.5, below_one)),
        // By way of a curve.
        past("curve", curved(below_one, 0.5)),
        inside("curve_second", curved(0.5, below_one)),
        // An inner input elsewhere: the next row, not past the table.
        inside("two_second", two(0.5, below_one)),
        inside("two_second_near", two(0.0, 1.0 - 3.0e-7)),
        inside("four_second", four(0.5, below_one, 0.4, 0.3)),
        inside("four_second_after_one", four(1.0, below_one, 0.4, 0.3)),
        inside("five_second", five(0.25, below_one)),
        // An inner node from below: rounded up, with a negative weight, in the table.
        inside("two_inner_x", two(0.5 - 2.0e-7, 0.3)),
        inside("two_inner_y", two(0.3, 0.5 - 2.0e-7)),
        inside("four_inner", four(0.5 - 2.0e-7, 0.6, 0.4, 0.3)),
        // Exactly 1: the next node is the same one.
        inside("two_one", two(1.0, 1.0)),
        inside("four_one", four(1.0, 1.0, 1.0, 1.0)),
        inside("five_one", five(1.0, 1.0)),
    ];
    let summary = sweep("ociobakelut_icc_last_node", TOOL, PROGRAM, &cases);
    let reads_outside = cases.iter().filter(|case| case.stop.is_some()).count();
    assert_eq!(reads_outside, 10);
    assert_eq!(summary.port_stops, reads_outside);
    for (case, outcome) in cases.iter().zip(&summary.outcomes) {
        assert!(
            case.stop.is_some() || !outcome.files.is_empty(),
            "{}",
            case.label
        );
    }
}
