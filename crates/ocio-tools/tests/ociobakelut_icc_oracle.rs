// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ociobakelut --format icc` against the wheel's: stdout, stderr, exit code and the profile,
//! byte for byte but for its creation time ([`Case::icc`]).
//!
//! Where the wheel's tool leaves its memory the port stops with an error instead
//! (docs/improvements.md, U-650 to U-652, and [`Case::stops`]).

mod common;
mod icc_profiles;

use std::time::Duration;

use common::{Case, ICC_CREATION_TIME, Stop, sweep, test_file, test_file_bytes, wheel_outcomes};
use icc_profiles::{Rng, corrupt, generated};
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
            .unwrap_or_else(|e| panic!("{directory}: {e}"))
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

/// `cmsSignalError` keeps 1022 characters of a message (U-650 is why this is Linux only).
#[cfg(unix)]
#[test]
fn long_messages() {
    let stop = Stop::IfTheWheelDies(IccStop::NullTransform.message());
    let name = |length: usize| "x".repeat(length);
    let (short, exact, long) = (name(1000), name(1015), name(1100));
    let cases = vec![
        baking("short", &["--cubesize", "3", "--displayicc", &short]).stops(stop),
        baking("exact", &["--cubesize", "3", "--displayicc", &exact]).stops(stop),
        baking("long", &["--cubesize", "3", "--displayicc", &long]).stops(stop),
    ];
    let summary = sweep("ociobakelut_icc_long_messages", TOOL, PROGRAM, &cases);
    assert_eq!(deaths(&summary.outcomes), 3);
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
